//! `ai.embed(text, as: V)` and `ai.embed_all(texts, as: V)` (area 21,
//! "The `ai` capability" and "Embeddings and search"; chunk 5.5v4): a
//! model asked for a text's embedding, a `V`, a vector type with
//! `dimensions:`, or for a list of texts' embeddings in the same order. It
//! leaves the process, so only an action calls it, through the connection
//! `ai: { connection: }` names.
//!
//! - **The text** is given first: a value of a type built on `Text`, or a
//!   `Markdown`; for `embed_all`, a list of them.
//! - **`as:`** names `V`, written out, a type built on `Vector` with
//!   `dimensions:`, as many as `ai:`'s `embedding_dimensions:` says the
//!   model makes (`semantic.ai_dimensions_mismatch` otherwise), so a model
//!   change that alters the length is caught before a column holds two
//!   kinds of vector.
//! - **The options** are written out: `model:`, a name `ai: { models: }`
//!   gives (`embedding` when left out), and `timeout:`, 1 second to 10
//!   minutes (15 seconds).
//! - **Its three failures** are conditions of the call, each mapped by its
//!   `attempt` to an `unavailable` failure of the app's: `timed_out`,
//!   `over_budget` and `too_long` (a text longer than the model takes, or
//!   more than 100 texts).

use jadpo_diag::{Diagnostic, nearest, rule};
use jadpo_syntax::ast::{self, AstNode, Expr};

use super::super::rules::Const;
use super::super::ty::{Prim, Ty, TypeRef};
use super::facts::Reach;
use super::{Body, CallKind, Context, Fallible, node_key};
use crate::resolve::Want;
use crate::tree;

/// `ai.embed`'s named arguments.
const OPTIONS: [&str; 3] = ["as", "model", "timeout"];

/// `ai.embed`'s three conditions, as an `attempt`'s arms name them.
pub(crate) const CONDITIONS: [jadpo_std::RuleKey; 3] = [
    jadpo_std::RuleKey::TimedOut,
    jadpo_std::RuleKey::OverBudget,
    jadpo_std::RuleKey::TooLong,
];

/// Grouped by thousands, `1,536`.
fn grouped(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (at, digit) in digits.chars().enumerate() {
        if at > 0 && (digits.len() - at).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

impl Body<'_> {
    /// `ai.embed(text, as: V)`, or `ai.embed_all(texts, as: V)` when `all`.
    pub(super) fn embed_call(
        &mut self,
        call: &ast::CallExpr,
        callee: &Expr,
        written: &str,
        all: bool,
    ) -> (Ty, CallKind, Reach) {
        let failed = |body: &mut Self| {
            body.embed_args_only(call);
            (
                Ty::Error,
                CallKind::Effect(written.to_string()),
                Reach::Unknown,
            )
        };
        let span = self.span(callee);
        if self.context.is_pure() {
            let who = super::capital(self.context.with_a());
            self.out.push(
                Diagnostic::new(
                    &rule::EFFECT_FUNCTION_PERSISTENCE,
                    format!("{who} can't call `{written}`"),
                )
                .at(span, "asks a model")
                .reason(format!(
                    "{who} is pure, and `{written}` asks a model for an embedding, outside the program; only an action asks one."
                ))
                .next(format!("Call `{written}` in an action, and pass in what it gives")),
            );
            return failed(self);
        }
        if self.context == Context::Test {
            self.out.push(
                Diagnostic::new(
                    &rule::SEMANTIC_NOT_CALLABLE,
                    format!("A test can't call `{written}`"),
                )
                .at(span, "asks a model")
                .reason(format!(
                    "`{written}` asks a model for an embedding; a test reaches one through the application's actions, every embedding made by the `ai` fake from the text (area 21, \"Tests\")."
                ))
                .next("Call an action that embeds the text"),
            );
            return failed(self);
        }
        let mut positional = 0;
        let mut given_text = false;
        let mut vector: Option<(Ty, u64)> = None;
        let mut answer_given = false;
        let mut model: Option<(jadpo_diag::Span, String)> = None;
        let mut timeout_ms: Option<u64> = None;
        let mut ok = true;
        for arg in Self::arguments(call) {
            let Some(value) = arg.expr() else { continue };
            let Some(name) = Self::arg_name(&arg) else {
                positional += 1;
                let ty = self.infer(&value);
                if positional > 1 || ty.has_error() {
                    ok &= !ty.has_error();
                    continue;
                }
                given_text = true;
                if !self.embeddable(&ty, all) {
                    ok = false;
                    let at = self.span(&value);
                    let (wanted, what) = if all {
                        (
                            "a list of texts",
                            "a `List` of values built on `Text`, or of `Markdown`",
                        )
                    } else {
                        (
                            "a text",
                            "a value of a type built on `Text`, or a `Markdown`",
                        )
                    };
                    self.out.push(
                        Diagnostic::new(
                            &rule::TYPE_MISMATCH,
                            format!("`{ty}` can't be used where {wanted} is required"),
                        )
                        .at(at, format!("this is `{ty}`"))
                        .reason(format!(
                            "`{written}` asks a model for the embedding of {wanted}: {what}, which is there to embed."
                        ))
                        .next(if self.table.is_nullable(&ty) {
                            "Embed a text that is there, after checking it isn't `none`"
                        } else {
                            "Pass the text to embed"
                        }),
                    );
                }
                continue;
            };
            let name_span = arg
                .name()
                .map_or_else(|| self.span(&value), |name| self.span(&name));
            match name.as_str() {
                "as" => {
                    answer_given = true;
                    vector = self.vector_answer(&value, written);
                    ok &= vector.is_some();
                }
                "model" => {
                    if let Expr::NameRef(named) = &value
                        && let Some(text) = tree::name_ref_text(named)
                    {
                        model = Some((self.span(&value), text));
                    } else {
                        ok = false;
                        self.out.push(
                            Diagnostic::new(
                                &rule::TYPE_MISMATCH,
                                format!("`{written}`'s `model:` is a model's name"),
                            )
                            .at(self.span(&value), "not a model's name")
                            .reason("`model:` names one of the models `ai: { models: }` lists in `app.jadpo`, written bare, `model: embedding`; a model itself is a `ModelId` setting, so code never writes one.")
                            .next("Name a model `ai: { models: }` lists, such as `model: embedding`"),
                        );
                    }
                }
                "timeout" => {
                    timeout_ms = match Const::from_expr(&value) {
                        Const::Duration(text) => jadpo_std::Dur::parse(&text)
                            .and_then(|dur| u64::try_from(dur.millis).ok())
                            .filter(|millis| {
                                (1_000..=jadpo_std::ai::MOST_TIMEOUT_MS).contains(millis)
                            }),
                        _ => None,
                    };
                    if timeout_ms.is_none() {
                        ok = false;
                        self.option_invalid(
                            &value,
                            written,
                            "timeout",
                            "a duration from `1s` to `10m`, written out",
                            "`timeout:` is how long the call waits for the embeddings, its retries included, before it fails `timed_out`. Without it the call waits 15 seconds.",
                            "Write `timeout: 30s`",
                        );
                    }
                }
                other => {
                    ok = false;
                    self.infer(&value);
                    let params: Vec<(String, Ty)> =
                        std::iter::once(if all { "texts" } else { "text" })
                            .chain(OPTIONS)
                            .map(|param| (param.to_string(), Ty::Error))
                            .collect();
                    self.unknown_argument(written, other, &params, name_span);
                }
            }
        }
        if positional != 1 || !answer_given {
            let at = self.arg_list_span(call);
            let what = if all { "a list of texts" } else { "a text" };
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_ARGUMENT_COUNT,
                    format!("`{written}` takes {what} and `as:`"),
                )
                .at(
                    at,
                    if answer_given {
                        format!("{positional} values given")
                    } else {
                        "no `as:`".to_string()
                    },
                )
                .reason(format!(
                    "`{written}` asks a model for the embedding of {what}, given first, as the vector type `as:` names; `model:` and `timeout:` may follow."
                ))
                .next(format!(
                    "Write `{written}({}, as: SomeMeaning)`",
                    if all { "texts" } else { "text" }
                )),
            );
            return (
                Ty::Error,
                CallKind::Effect(written.to_string()),
                Reach::Unknown,
            );
        }
        // Asked through the connection `ai:` names, and as long as it says
        // the model's embeddings are.
        let settings = crate::ai::settings(self.db, self.project);
        if !settings.written {
            self.out.push(
                Diagnostic::new(
                    &rule::AI_CONNECTION_REQUIRED,
                    format!("`{written}` asks a model, and the application names none"),
                )
                .at(span.clone(), "no `ai:` to ask through")
                .reason("Every `ai` call goes through the connection `application { ai: { connection: } }` names, asking a model `models:` lists, each a `ModelId` setting, so changing a model or a provider changes `app.jadpo` and nothing else.")
                .next("Add `ai: { connection: <name>  models: { default: config.<name>  embedding: config.<name> }  embedding_dimensions: <n> }` to `application { }` in `app.jadpo`"),
            );
            ok = false;
        } else if let Some(connection) = settings.connection.as_ref().filter(|connection| {
            crate::ai_mapping::maps(self.db, self.project, connection, "embed") == Some(false)
        }) {
            // A package's `provide ai` that maps no `embed:` (chunk 5.5v7).
            let package = crate::connections::package_connections(self.db, self.project)
                .connections
                .iter()
                .find(|known| known.name == *connection)
                .map(|known| known.package.clone())
                .unwrap_or_default();
            self.out.push(
                Diagnostic::new(
                    &rule::AI_SETTINGS_INVALID,
                    format!("`{connection}` doesn't embed"),
                )
                .at(span.clone(), "no `embed:` to ask through")
                .reason(format!(
                    "`ai: {{ connection: }}` names `{connection}`, and its package `{package}` maps no `embed:` in its `provides/ai.jadpo`, so it has no embeddings to give."
                ))
                .next("Name a connection in `ai:` whose package maps `embed:`, or one with `driver: Driver.openai_compatible`"),
            );
            ok = false;
        } else if settings.connection.is_some() && settings.complete() {
            let names = settings.model_names();
            let listed: Vec<String> = names.iter().map(|name| format!("`{name}`")).collect();
            match &model {
                Some((at, asked)) if !names.contains(&asked.as_str()) => {
                    let mut problem = Diagnostic::new(
                        &rule::SEMANTIC_UNKNOWN_NAME,
                        format!("`{asked}` isn't a model `ai:` names"),
                    )
                    .at(at.clone(), "no such model")
                    .reason(format!(
                        "`model:` names one of the models `ai: {{ models: }}` lists in `app.jadpo`: {}.",
                        crate::and_list(&listed)
                    ));
                    problem = match nearest(asked, names.iter().copied()) {
                        Some(near) => problem
                            .next(format!("Use `{near}` instead of `{asked}`"))
                            .edit(at.clone(), near),
                        None => problem.next(format!(
                            "Add `{asked}: config.<name>` to `ai: {{ models: }}`, or name one it has"
                        )),
                    };
                    self.out.push(problem);
                    ok = false;
                }
                None if !names.contains(&jadpo_std::embed::DEFAULT_MODEL) => {
                    self.out.push(
                        Diagnostic::new(
                            &rule::AI_SETTINGS_INVALID,
                            format!("`{written}` asks the `embedding` model, and `ai:` names none"),
                        )
                        .at(span.clone(), "no `embedding` model")
                        .reason(format!(
                            "An embedding is asked of the model `ai: {{ models: }}` calls `embedding` unless the call names another with `model:`, and it names {}. A model that writes text seldom makes embeddings, so an application that embeds names the one that does.",
                            crate::and_list(&listed)
                        ))
                        .next("Add `embedding: config.<name>` to `ai: { models: }`, a `ModelId` setting naming the embedding model"),
                    );
                    ok = false;
                }
                _ => {}
            }
            if let Some((ty, dimensions)) = &vector {
                let at = Self::arguments(call)
                    .iter()
                    .find(|arg| Self::arg_name(arg).as_deref() == Some("as"))
                    .and_then(ast::Arg::expr)
                    .map_or_else(|| span.clone(), |value| self.span(&value));
                match settings.embedding_dimensions.as_ref() {
                    None => {
                        self.out.push(
                            Diagnostic::new(
                                &rule::SEMANTIC_AI_DIMENSIONS_MISMATCH,
                                format!("`{ty}` has {} dimensions, and `ai:` doesn't say how long the model's embeddings are", grouped(*dimensions)),
                            )
                            .at(at, format!("{} dimensions", grouped(*dimensions)))
                            .reason("`embedding_dimensions:` in `ai:` says how many numbers each of the model's embeddings has, and every vector type `ai.embed` answers as must have as many `dimensions:`, so a model change that alters the length is caught before a column holds two kinds of vector.")
                            .next(format!("Add `embedding_dimensions: {dimensions}` to `ai:` in `app.jadpo`, if that is the model's length")),
                        );
                        ok = false;
                    }
                    Some((made, _)) if made != dimensions => {
                        self.out.push(
                            Diagnostic::new(
                                &rule::SEMANTIC_AI_DIMENSIONS_MISMATCH,
                                format!("`{ty}` has {} dimensions, and the model makes {}", grouped(*dimensions), grouped(*made)),
                            )
                            .at(at, format!("{} dimensions, not {}", grouped(*dimensions), grouped(*made)))
                            .reason(format!(
                                "Each embedding the model makes has the {} numbers `ai:`'s `embedding_dimensions:` says, and `{ty}` holds vectors of {}, so a column of it would hold two kinds of vector.",
                                grouped(*made),
                                grouped(*dimensions)
                            ))
                            .next(format!(
                                "Give `{ty}` `dimensions: {made}`, or set `embedding_dimensions: {dimensions}` in `ai:` if the model makes vectors that long"
                            )),
                        );
                        ok = false;
                    }
                    Some(_) => {}
                }
            }
        }
        let (true, Some((vector, dimensions))) = (given_text, vector) else {
            return (
                Ty::Error,
                CallKind::Effect(written.to_string()),
                Reach::Unknown,
            );
        };
        let key = node_key(call.syntax());
        let ty = if all {
            Ty::List(Box::new(vector))
        } else {
            vector
        };
        self.facts.types.insert(key, ty.clone());
        if ok {
            self.embeds += 1;
        }
        self.facts.constructions.insert(
            key,
            Fallible::Construction(
                CONDITIONS
                    .iter()
                    .map(|condition| condition.word().to_string())
                    .collect(),
            ),
        );
        (
            ty,
            CallKind::Effect(written.to_string()),
            Reach::Embed {
                model: model.map_or_else(
                    || jadpo_std::embed::DEFAULT_MODEL.to_string(),
                    |(_, name)| name,
                ),
                timeout_ms: timeout_ms.unwrap_or(jadpo_std::embed::DEFAULT_TIMEOUT_MS),
                all,
                dimensions,
            },
        )
    }

    /// Types the text and the values of options that aren't written out,
    /// without reading `as:` or `model:` as values.
    fn embed_args_only(&mut self, call: &ast::CallExpr) {
        for arg in Self::arguments(call) {
            if matches!(Self::arg_name(&arg).as_deref(), Some("as" | "model")) {
                continue;
            }
            if let Some(value) = arg.expr() {
                self.infer(&value);
            }
        }
    }

    /// Whether `ty` is a text a model embeds, or for `all` a list of them:
    /// built on `Text`, or a `Markdown`, and never `none`.
    fn embeddable(&self, ty: &Ty, all: bool) -> bool {
        if self.table.is_nullable(ty) {
            return false;
        }
        if all {
            return match self.table.structure(ty) {
                Ty::List(item) => self.embeddable(&item, false),
                _ => false,
            };
        }
        if self.table.base_prim(ty) == Some(Prim::Text) {
            return true;
        }
        matches!(
            self.table.structure(ty),
            Ty::Opaque(name, args)
                if args.is_empty() && name == jadpo_std::standard::markup::MARKDOWN
        )
    }

    /// `as:`'s vector type and its `dimensions:`: written out, built on
    /// `Vector`, with a length.
    fn vector_answer(&mut self, value: &Expr, written: &str) -> Option<(Ty, u64)> {
        let target = match TypeRef::from_expr(value) {
            TypeRef::Path(names, args)
                if names
                    .first()
                    .is_some_and(|name| name.starts_with(|c: char| c.is_ascii_uppercase())) =>
            {
                if !self.resolved_by(|names, out| names.type_expr(value, Want::Type, out)) {
                    return None;
                }
                Some(self.table.resolve(&TypeRef::Path(names, args)))
            }
            _ => None,
        };
        let Some(target) = target else {
            let shown = tree::written(value.syntax());
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_MISMATCH,
                    format!("`{written}`'s `as:` is a type written out, not `{shown}`"),
                )
                .at(self.span(value), "not a type")
                .reason(format!(
                    "`{written}` reads the model's embedding as the vector type `as:` names, written out, `as: <Type>`: a value can't say which type to read."
                ))
                .next("Write the vector type itself, `as: <Type>`"),
            );
            return None;
        };
        if target.has_error() {
            return None;
        }
        let at = self.span(value);
        let chain = self.table.rule_chain(&target);
        let is_vector = chain
            .iter()
            .any(|(_, rules)| rules.standard == Some(jadpo_std::standard::ai::VECTOR));
        let dimensions = chain.iter().find_map(|(_, rules)| rules.dimensions);
        match (is_vector, dimensions) {
            (true, Some(dimensions)) => Some((target, dimensions)),
            (true, None) => {
                self.out.push(
                    Diagnostic::new(
                        &rule::TYPE_MISMATCH,
                        format!("`{written}` can't answer as `{target}`, which has no length"),
                    )
                    .at(at, "a `Vector` without `dimensions:`")
                    .reason(format!(
                        "`{written}` reads each embedding as a vector type with the model's `dimensions:`, so a vector of another length is caught when the model answers; `{target}` has none."
                    ))
                    .next("Answer as a type built on `Vector` with `dimensions:`, as many as `ai:`'s `embedding_dimensions:`"),
                );
                None
            }
            (false, _) => {
                self.out.push(
                    Diagnostic::new(
                        &rule::TYPE_MISMATCH,
                        format!("`{written}` can't answer as `{target}`"),
                    )
                    .at(at, "not a vector type")
                    .reason(format!(
                        "`{written}` gives the model's embedding as a vector type: one built on `Vector` with `dimensions:`, which says how its vectors are compared and stored; `{target}` isn't one."
                    ))
                    .next("Declare `type <Name>Meaning = Vector { dimensions: <n> }` and answer as it"),
                );
                None
            }
        }
    }
}
