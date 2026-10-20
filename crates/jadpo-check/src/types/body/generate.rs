//! `ai.generate(prompt, as: T)` (area 21, "The `ai` capability" and "Typed
//! output"; chunk 5.5v3): a model asked to answer a prompt, its answer a
//! `T` that has passed every rule of `T`, as a request body is. It leaves
//! the process, so only an action calls it, before its first write (area
//! 6, "Outside calls hold no transaction"), through the connection
//! `ai: { connection: }` names.
//!
//! - **The prompt** is a value of a prompt's type, given first.
//! - **`as:`** names `T`, a type written out that `json.read` reads, but
//!   never a reference (`X.id`), a `File`, `Bytes` or `PresentationText`
//!   (`semantic.ai_output_type`), nor a secret value, which a model never
//!   makes (`configuration.secret_flow`).
//! - **The options** are written out: `model:`, a name `ai: { models: }`
//!   gives (`default` when left out); `max_output:`, the most tokens the
//!   answer may take, 1 to 1,000,000 (1,000); `timeout:`, how long the
//!   call waits for a whole answer, 1 second to 10 minutes (60 seconds);
//!   and `attempts:`, the answers asked for in all while one doesn't
//!   decode, 1 to 3 (2).
//! - **Its five failures** are conditions of the call, each mapped by its
//!   `attempt` to an `unavailable` failure of the app's, as a store's
//!   `storage_full` is: `refused`, `malformed` (whose failure receives the
//!   last `DecodeProblem` in a `problem:` field), `timed_out`,
//!   `over_budget` and `too_long`; `invalid:` covers none of them.
//!
//! A test reaches a model only through the `ai` fake: its `fakes: {
//! ai.<prompt>: [...] }` scripts the answers, and `ai.<prompt>.requests`
//! lists the prompt values the code asked with.

use std::collections::BTreeSet;

use jadpo_diag::{Diagnostic, Span, nearest, rule};
use jadpo_syntax::ast::{self, AstNode, Expr};

use super::super::json_form::{JsonUse, no_json_form};
use super::super::rules::Const;
use super::super::shape::FieldsBase;
use super::super::table::Def;
use super::super::ty::{Prim, Ty, TypeRef};
use super::facts::Reach;
use super::{Body, CallKind, Context, Fallible, node_key};
use crate::resolve::Want;
use crate::tree;

/// `ai.generate`'s named arguments.
const OPTIONS: [&str; 5] = ["as", "model", "max_output", "timeout", "attempts"];

/// `ai.generate`'s five conditions, as an `attempt`'s arms name them.
pub(crate) const CONDITIONS: [jadpo_std::RuleKey; 5] = [
    jadpo_std::RuleKey::Refused,
    jadpo_std::RuleKey::Malformed,
    jadpo_std::RuleKey::TimedOut,
    jadpo_std::RuleKey::OverBudget,
    jadpo_std::RuleKey::TooLong,
];

impl Body<'_> {
    /// `ai.generate(prompt, as: T)`.
    pub(super) fn generate_call(
        &mut self,
        call: &ast::CallExpr,
        callee: &Expr,
        written: &str,
    ) -> (Ty, CallKind, Reach) {
        let failed = |body: &mut Self| {
            body.generate_args_only(call);
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
                    "{who} is pure, and `{written}` asks a model, outside the program; only an action asks one."
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
                    "`{written}` asks a model; a test reaches one through the application's actions, with every answer from the `ai` fake (area 21, \"Tests\")."
                ))
                .next("Call an action that asks the model, and script its answers in `fakes: { ai.<prompt>: [...] }`"),
            );
            return failed(self);
        }
        let args = Self::arguments(call);
        // The prompt, given first and without a name.
        let mut prompt: Option<String> = None;
        let mut positional = 0;
        let mut answer: Option<Ty> = None;
        let mut answer_given = false;
        let mut options = Options::default();
        let mut ok = true;
        for arg in &args {
            let Some(value) = arg.expr() else { continue };
            let Some(name) = Self::arg_name(arg) else {
                positional += 1;
                if positional > 1 {
                    self.infer(&value);
                    continue;
                }
                let ty = self.infer(&value);
                if ty.has_error() {
                    ok = false;
                    continue;
                }
                match crate::prompts::prompt_of(self.table, &ty) {
                    Some(key) => prompt = Some(key),
                    None => {
                        ok = false;
                        let at = self.span(&value);
                        self.out.push(
                            Diagnostic::new(
                                &rule::TYPE_MISMATCH,
                                format!("`{ty}` can't be used where a prompt is required"),
                            )
                            .at(at, format!("this is `{ty}`"))
                            .reason(format!(
                                "`{written}` asks a model with a prompt: a value of a `type X = Prompt {{ ... }}`, whose fields fill the placeholders of its `.prompt` file, so a model is only ever sent text a reviewer read."
                            ))
                            .next("Pass a value of a prompt's type, built with its fields"),
                        );
                    }
                }
                continue;
            };
            let name_span = arg
                .name()
                .map_or_else(|| self.span(&value), |name| self.span(&name));
            match name.as_str() {
                "as" => {
                    answer_given = true;
                    answer = self.answer_type(&value, written);
                    if answer.is_none() {
                        ok = false;
                    }
                }
                "model" => ok &= self.model_option(&value, written, &mut options),
                "max_output" => ok &= self.max_output_option(&value, written, &mut options),
                "timeout" => ok &= self.timeout_option(&value, written, &mut options),
                "attempts" => ok &= self.attempts_option(&value, written, &mut options),
                other => {
                    ok = false;
                    self.infer(&value);
                    let params: Vec<(String, Ty)> = std::iter::once("prompt")
                        .chain(OPTIONS)
                        .map(|param| (param.to_string(), Ty::Error))
                        .collect();
                    self.unknown_argument(written, other, &params, name_span);
                }
            }
        }
        if positional != 1 || !answer_given {
            let at = self.arg_list_span(call);
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_ARGUMENT_COUNT,
                    format!("`{written}` takes a prompt and `as:`"),
                )
                .at(at, if answer_given {
                    format!("{positional} values given")
                } else {
                    "no `as:`".to_string()
                })
                .reason(format!(
                    "`{written}` asks a model to answer one prompt, given first, as the type `as:` names, which its answer is decoded into; `model:`, `max_output:`, `timeout:` and `attempts:` may follow."
                ))
                .next(format!("Write `{written}(SomePrompt {{ ... }}, as: SomeAnswer)`")),
            );
            return (
                answer.unwrap_or(Ty::Error),
                CallKind::Effect(written.to_string()),
                Reach::Unknown,
            );
        }
        // Asked through the connection `ai:` names (area 21, "Connecting a
        // provider").
        let settings = crate::ai::settings(self.db, self.project);
        if !settings.written {
            self.out.push(
                Diagnostic::new(
                    &rule::AI_CONNECTION_REQUIRED,
                    format!("`{written}` asks a model, and the application names none"),
                )
                .at(span, "no `ai:` to ask through")
                .reason("Every `ai` call goes through the connection `application { ai: { connection: } }` names, asking a model `models:` lists, each a `ModelId` setting, so changing a model or a provider changes `app.jadpo` and nothing else.")
                .next("Add `ai: { connection: <name>  models: { default: config.<name> } }` to `application { }` in `app.jadpo`, naming a connection with `driver: Driver.openai_compatible`"),
            );
            ok = false;
        } else if let Some((at, model)) = &options.model
            && settings.connection.is_some()
            && !settings.models.iter().any(|(name, _)| name == model)
            && settings
                .model_names()
                .contains(&jadpo_std::ai::DEFAULT_MODEL)
        {
            let names: Vec<String> = settings
                .model_names()
                .iter()
                .map(|name| format!("`{name}`"))
                .collect();
            let near = nearest(model, settings.model_names());
            let mut problem = Diagnostic::new(
                &rule::SEMANTIC_UNKNOWN_NAME,
                format!("`{model}` isn't a model `ai:` names"),
            )
            .at(at.clone(), "no such model")
            .reason(format!(
                "`model:` names one of the models `ai: {{ models: }}` lists in `app.jadpo`: {}.",
                crate::and_list(&names)
            ));
            problem = match near {
                Some(near) => problem
                    .next(format!("Use `{near}` instead of `{model}`"))
                    .edit(at.clone(), near),
                None => problem.next(format!(
                    "Add `{model}: config.<name>` to `ai: {{ models: }}`, or name one it has"
                )),
            };
            self.out.push(problem);
            ok = false;
        }
        let (Some(prompt), Some(answer)) = (prompt, answer) else {
            return (
                Ty::Error,
                CallKind::Effect(written.to_string()),
                Reach::Unknown,
            );
        };
        let key = node_key(call.syntax());
        // The call's site is the type it gives, however the call is
        // reached, which the plan reads its codec from. One whose options
        // are reported still fails as any call does, so what maps its
        // conditions is checked too.
        self.facts.types.insert(key, answer.clone());
        if ok {
            self.generates.push((prompt.clone(), answer.clone()));
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
            answer,
            CallKind::Effect(written.to_string()),
            Reach::Generate {
                prompt,
                model: options.model.map_or_else(
                    || jadpo_std::ai::DEFAULT_MODEL.to_string(),
                    |(_, name)| name,
                ),
                max_output: options
                    .max_output
                    .unwrap_or(jadpo_std::ai::DEFAULT_MAX_OUTPUT),
                timeout_ms: options
                    .timeout_ms
                    .unwrap_or(jadpo_std::ai::DEFAULT_TIMEOUT_MS),
                attempts: options.attempts.unwrap_or(jadpo_std::ai::DEFAULT_ATTEMPTS),
            },
        )
    }

    /// Types the prompt and the values of options that aren't written
    /// out, without reading `as:` or `model:` as values.
    fn generate_args_only(&mut self, call: &ast::CallExpr) {
        for arg in Self::arguments(call) {
            let named = Self::arg_name(&arg);
            if matches!(named.as_deref(), Some("as" | "model")) {
                continue;
            }
            if let Some(value) = arg.expr() {
                self.infer(&value);
            }
        }
    }

    /// `as:`'s type: written out, a type `json.read` reads, and one a
    /// model's answer can be.
    fn answer_type(&mut self, value: &Expr, written: &str) -> Option<Ty> {
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
                    "`{written}` decodes the model's answer into the type `as:` names, written out, such as `as: <Type>`: a value can't say which type to read."
                ))
                .next("Write the type itself, `as: <Type>`"),
            );
            return None;
        };
        if target.has_error() {
            return None;
        }
        let at = self.span(value);
        if let Some(secret) =
            super::super::secrets::held_secret(self.table, &target, &mut Vec::new())
        {
            self.out.push(
                Diagnostic::new(
                    &rule::CONFIGURATION_SECRET_FLOW,
                    format!("`{written}` can't answer as `{target}`, which holds a `{secret}`"),
                )
                .at(at, format!("holds a `{secret}`"))
                .reason(format!(
                    "`{secret}` is a secret value, and a model never makes a secret: what it answers came from outside, and a provider keeps what it sends."
                ))
                .next("Ask for the parts that aren't secret, and make the secret where the app keeps it"),
            );
            return None;
        }
        let shown = target.to_string();
        if let Some(found) = self.unanswerable(&target, &shown, &mut BTreeSet::new(), 0) {
            let Unanswerable {
                place,
                what,
                instead,
            } = found;
            self.out.push(
                Diagnostic::new(
                    &rule::SEMANTIC_AI_OUTPUT_TYPE,
                    format!("A model can't answer as `{target}`"),
                )
                .at(at, format!("`{place}` is {what}"))
                .reason(format!(
                    "A model's answer is JSON decoded into `as:`'s type, and `{place}` is {what}, which a model's JSON can't be. {instead}"
                ))
                .next(if what == "a reference to a record" {
                    format!(
                        "Ask for an enum of the candidates, or a code the record is looked up by, in place of `{place}`"
                    )
                } else {
                    format!("Ask for a type without `{place}`")
                }),
            );
            return None;
        }
        if let Some((offending, what)) =
            no_json_form(self.table, &target, &mut BTreeSet::new(), JsonUse::Read)
        {
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_MISMATCH,
                    format!("`{written}` can't decode an answer as `{target}`"),
                )
                .at(at, format!("`{offending}` has no JSON form here"))
                .reason(format!(
                    "`{written}` decodes a model's answer as `json.read` reads a document, into any type an `Input` may hold: values built on the built-in and standard types, enums, and objects, inputs and outputs made of them, in lists, sets and maps; `{offending}` is {what}."
                ))
                .next("Ask for a type made of those"),
            );
            return None;
        }
        Some(target)
    }

    /// The first part of `ty` a model's answer can't be, named `place`
    /// (`Recommendation.article`), with what it is and what to do instead:
    /// a reference to a record, a file, bytes or text made for display.
    fn unanswerable(
        &self,
        ty: &Ty,
        place: &str,
        seen: &mut BTreeSet<String>,
        depth: u32,
    ) -> Option<Unanswerable> {
        if depth > 32 {
            return None;
        }
        let found = |what: &'static str, instead: String| {
            Some(Unanswerable {
                place: place.to_string(),
                what,
                instead,
            })
        };
        let reference = |host: &str| {
            found(
                "a reference to a record",
                format!(
                    "A model can't know which `{host}` records exist: pass the candidates in the prompt, and ask for an enum, or a field the code looks the `{host}` up by."
                ),
            )
        };
        match self.table.base_prim(ty) {
            Some(Prim::Bytes) => {
                return found("bytes", "A model answers in text.".to_string());
            }
            Some(Prim::PresentationText) => {
                return found(
                    "text Jadpo makes for display",
                    "`PresentationText` is only made by translations and formatting, so a model's words are never shown as the app's own.".to_string(),
                );
            }
            _ => {}
        }
        let id_of = |ty: &Ty| match ty.strip_nullable() {
            Ty::Field(host, path)
                if path.len() == 1
                    && path.first().map(String::as_str) == Some("id")
                    && self.table.base(host) == Some(FieldsBase::Entity) =>
            {
                Some(host.clone())
            }
            _ => None,
        };
        if let Some(host) = id_of(ty) {
            return reference(&host);
        }
        let structure = self.table.structure(ty);
        if let Some(host) = id_of(&structure) {
            return reference(&host);
        }
        match structure {
            Ty::Opaque(name, _) if name == jadpo_std::files::FILE => found(
                "a file",
                "A file is put in a store by the app; a model answers in text.".to_string(),
            ),
            Ty::List(item) | Ty::Set(item) | Ty::Nullable(item) => {
                self.unanswerable(&item, place, seen, depth + 1)
            }
            Ty::Map(key, value) => self
                .unanswerable(&key, place, seen, depth + 1)
                .or_else(|| self.unanswerable(&value, place, seen, depth + 1)),
            Ty::Named(name) => {
                if !seen.insert(name.clone()) {
                    return None;
                }
                let at = |field: &str| format!("{name}.{field}");
                match self.table.def(&name)? {
                    Def::Fields { fields, .. } => fields.iter().find_map(|field| {
                        self.unanswerable(&field.declared, &at(&field.name), seen, depth + 1)
                    }),
                    Def::Enum { variants } => variants.iter().find_map(|variant| {
                        variant.fields.as_ref()?.iter().find_map(|field| {
                            self.unanswerable(
                                &field.declared,
                                &format!("{name}.{}.{}", variant.name, field.name),
                                seen,
                                depth + 1,
                            )
                        })
                    }),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// `model: <name>`: a name `ai: { models: }` gives, checked once the
    /// call is.
    fn model_option(&mut self, value: &Expr, written: &str, options: &mut Options) -> bool {
        if let Expr::NameRef(name) = value
            && let Some(model) = tree::name_ref_text(name)
        {
            options.model = Some((self.span(value), model));
            return true;
        }
        self.out.push(
            Diagnostic::new(
                &rule::TYPE_MISMATCH,
                format!("`{written}`'s `model:` is a model's name"),
            )
            .at(self.span(value), "not a model's name")
            .reason("`model:` names one of the models `ai: { models: }` lists in `app.jadpo`, written bare, `model: drafting`; a model itself is a `ModelId` setting, so code never writes one.")
            .next("Name a model `ai: { models: }` lists, such as `model: default`"),
        );
        false
    }

    /// `max_output:`: a whole number of tokens written out.
    fn max_output_option(&mut self, value: &Expr, written: &str, options: &mut Options) -> bool {
        let most = jadpo_std::ai::MOST_MAX_OUTPUT;
        let given = match Const::from_expr(value) {
            Const::Number {
                value: number,
                decimal: false,
            } if number.scale == 0 => u32::try_from(number.mantissa)
                .ok()
                .filter(|n| (1..=most).contains(n)),
            _ => None,
        };
        if given.is_none() {
            self.option_invalid(
                value,
                written,
                "max_output",
                "a whole number of tokens from 1 to 1,000,000, written out",
                "`max_output:` is the most tokens the model may answer with, sent as the provider's limit; an answer cut off there is `over_budget`. Without it the call allows 1,000.",
                "Write `max_output: 600`",
            );
        }
        options.max_output = given;
        given.is_some()
    }

    /// `timeout:`: a duration written out, 1 second to 10 minutes.
    fn timeout_option(&mut self, value: &Expr, written: &str, options: &mut Options) -> bool {
        let given = match Const::from_expr(value) {
            Const::Duration(text) => jadpo_std::Dur::parse(&text)
                .and_then(|dur| u64::try_from(dur.millis).ok())
                .filter(|millis| (1_000..=jadpo_std::ai::MOST_TIMEOUT_MS).contains(millis)),
            _ => None,
        };
        if given.is_none() {
            self.option_invalid(
                value,
                written,
                "timeout",
                "a duration from `1s` to `10m`, written out",
                "`timeout:` is how long the call waits for a whole answer, its retries included, before it fails `timed_out`. Without it the call waits 60 seconds.",
                "Write `timeout: 30s`",
            );
        }
        options.timeout_ms = given;
        given.is_some()
    }

    /// `attempts:`: 1 to 3, written out.
    fn attempts_option(&mut self, value: &Expr, written: &str, options: &mut Options) -> bool {
        let most = jadpo_std::ai::MOST_ATTEMPTS;
        let given = match Const::from_expr(value) {
            Const::Number {
                value: number,
                decimal: false,
            } if number.scale == 0 => u32::try_from(number.mantissa)
                .ok()
                .filter(|n| (1..=most).contains(n)),
            _ => None,
        };
        if given.is_none() {
            self.option_invalid(
                value,
                written,
                "attempts",
                "a whole number from 1 to 3, written out",
                "`attempts:` is how many answers the call asks for in all while one doesn't decode into its type, each after the first sent the problem with the one before; each counts against every limit. Without it the call asks twice: an answer, and one corrected.",
                "Write `attempts: 3`, or leave it out to correct an answer once",
            );
        }
        options.attempts = given;
        given.is_some()
    }

    pub(super) fn option_invalid(
        &mut self,
        value: &Expr,
        written: &str,
        option: &str,
        what: &str,
        reason: &str,
        next: &str,
    ) {
        if !super::is_literal(value) {
            self.infer(value);
        }
        let shown = tree::written(value.syntax());
        self.out.push(
            Diagnostic::new(
                &rule::TYPE_INVALID_LITERAL,
                format!("`{shown}` isn't a valid `{option}:` for `{written}`"),
            )
            .at(self.span(value), format!("not {what}"))
            .reason(reason.to_string())
            .next(next.to_string()),
        );
    }

    /// `ai.<prompt>.requests` in a test: the prompt values the code asked
    /// a model with, in order, typed as the prompt's type (area 21,
    /// "Tests").
    pub(super) fn ai_requests(&mut self, expr: &Expr, prompt: &str) -> Ty {
        let span = self.span(expr);
        let written = tree::written(expr.syntax());
        if self.context != Context::Test || !self.test {
            self.out.push(
                Diagnostic::new(
                    &rule::SEMANTIC_NOT_CALLABLE,
                    format!("Only a test reads `{written}`"),
                )
                .at(span, "what a fake was sent")
                .reason("`ai.<prompt>.requests` lists the prompt values the code asked a model with, which only a test's `ai` fake keeps; a running app never sees what another call sent.")
                .next(format!("Read `{written}` in a test")),
            );
            return Ty::Error;
        }
        let Some(ty) = self.prompt_type(prompt) else {
            let known = crate::prompts::analysis(self.db, self.project)
                .prompts
                .iter()
                .map(|found| found.key.clone())
                .collect::<Vec<_>>();
            let near = nearest(prompt, known.iter().map(String::as_str));
            let mut problem = Diagnostic::new(
                &rule::SEMANTIC_UNKNOWN_NAME,
                format!("`{prompt}` isn't a prompt"),
            )
            .at(span, "no such prompt")
            .reason("`ai.<prompt>.requests` names a prompt by its name, its type's name in lower_snake_case.");
            problem = match near {
                Some(near) => problem.next(format!("Use `ai.{near}.requests`")),
                None => problem.next("Name one of the project's prompts"),
            };
            self.out.push(problem);
            return Ty::Error;
        };
        self.facts
            .ai_requests
            .insert(node_key(expr.syntax()), prompt.to_string());
        Ty::List(Box::new(ty))
    }

    /// The type of the prompt called `prompt`, as this body names it.
    pub(super) fn prompt_type(&self, prompt: &str) -> Option<Ty> {
        let analysis = crate::prompts::analysis(self.db, self.project);
        let found = analysis.prompts.iter().find(|found| found.key == prompt)?;
        let ty = self
            .table
            .resolve(&TypeRef::Path(vec![found.type_name.clone()], Vec::new()));
        (!ty.has_error()).then_some(ty)
    }
}

/// `ai.generate`'s options as written.
#[derive(Default)]
struct Options {
    model: Option<(Span, String)>,
    max_output: Option<u32>,
    timeout_ms: Option<u64>,
    attempts: Option<u32>,
}

/// What a model's answer can't be, and where in the type.
struct Unanswerable {
    place: String,
    what: &'static str,
    instead: String,
}
