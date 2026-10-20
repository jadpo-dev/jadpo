//! `ai.stream(prompt)` (area 21, "The `ai` capability"; area 7,
//! "Streaming a model's answer"; chunk 5.5v6): a model asked for the
//! prompt's answer as text, sent to the client piece by piece as it is
//! written. It gives an `AiStream`, which only an action returns and only
//! a streaming route sends.
//!
//! - **The prompt** is a value of a prompt's type, given first.
//! - **The options** are `ai.generate`'s but `attempts:` and `as:`:
//!   `model:`, `max_output:` and `timeout:`, written out. A stream asks
//!   once, since its text isn't decoded and can't be asked for again from
//!   where it stopped.
//! - **Its four failures** are conditions of the call, before its first
//!   piece comes, each mapped by its `attempt` to an `unavailable` failure
//!   of the app's: `refused`, `timed_out`, `over_budget` and `too_long`.
//!   After the first piece the route has started its answer, and a failure
//!   ends the stream with an `error` event (area 7).

use jadpo_diag::{Diagnostic, nearest, rule};
use jadpo_syntax::ast::{self, AstNode};

use super::facts::Reach;
use super::generate::Options;
use super::{Body, CallKind, Context, Fallible, node_key};
use crate::types::ty::Ty;

/// `ai.stream`'s named arguments.
const OPTIONS: [&str; 3] = ["model", "max_output", "timeout"];

/// `ai.stream`'s four conditions, as an `attempt`'s arms name them: a text
/// answer isn't decoded, so it is never `malformed`.
pub(crate) const CONDITIONS: [jadpo_std::RuleKey; 4] = [
    jadpo_std::RuleKey::Refused,
    jadpo_std::RuleKey::TimedOut,
    jadpo_std::RuleKey::OverBudget,
    jadpo_std::RuleKey::TooLong,
];

/// The conditions a stream can stop with after its first piece, which a
/// test's `partial(...)` scripts: the model declines to go on, the time
/// runs out, or the answer reaches `max_output:`.
pub(crate) const AFTER_FIRST: [jadpo_std::RuleKey; 3] = [
    jadpo_std::RuleKey::Refused,
    jadpo_std::RuleKey::TimedOut,
    jadpo_std::RuleKey::OverBudget,
];

/// The type `ai.stream` gives.
pub(crate) fn ai_stream() -> Ty {
    Ty::Opaque(jadpo_std::ai::AI_STREAM.to_string(), Vec::new())
}

impl Body<'_> {
    /// `ai.stream(prompt)`.
    pub(super) fn stream_call(
        &mut self,
        call: &ast::CallExpr,
        callee: &ast::Expr,
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
                .next(format!("Call `{written}` in an action that a streaming route runs")),
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
                    "`{written}` asks a model; a test reaches one through a streaming route, with every answer from the `ai` fake (area 21, \"Tests\")."
                ))
                .next("Send a request to the route that streams, and script its answers in `fakes: { ai.<prompt>: [...] }`"),
            );
            return failed(self);
        }
        let mut prompt: Option<String> = None;
        let mut positional = 0;
        let mut options = Options::default();
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
                "model" => ok &= self.model_option(&value, written, &mut options),
                "max_output" => ok &= self.max_output_option(&value, written, &mut options),
                "timeout" => ok &= self.timeout_option(&value, written, &mut options),
                other => {
                    ok = false;
                    if other != "as" {
                        self.infer(&value);
                    }
                    let params: Vec<(String, Ty)> = std::iter::once("prompt")
                        .chain(OPTIONS)
                        .map(|param| (param.to_string(), Ty::Error))
                        .collect();
                    let before = self.out.len();
                    self.unknown_argument(written, other, &params, name_span);
                    // Why the two `ai.generate` takes aren't `ai.stream`'s.
                    let why = match other {
                        "attempts" => Some(
                            "A stream asks once: its pieces reach the client as they come, so an answer can't be asked for again from where it stopped.",
                        ),
                        "as" => Some(
                            "A stream is text, sent piece by piece, so it has no type to decode into; `ai.generate(prompt, as: T)` asks for a typed value.",
                        ),
                        _ => None,
                    };
                    if let (Some(why), Some(problem)) = (why, self.out.get_mut(before)) {
                        let taken = problem.clone().reason(why);
                        *problem = taken;
                    }
                }
            }
        }
        if positional != 1 {
            let at = self.arg_list_span(call);
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_ARGUMENT_COUNT,
                    format!("`{written}` takes one prompt"),
                )
                .at(at, format!("{positional} values given"))
                .reason(format!(
                    "`{written}` asks a model to answer one prompt, given first; `model:`, `max_output:` and `timeout:` may follow."
                ))
                .next(format!("Write `{written}(SomePrompt {{ ... }})`")),
            );
            return (
                Ty::Error,
                CallKind::Effect(written.to_string()),
                Reach::Unknown,
            );
        }
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
        } else if let Some(connection) = &settings.connection
            && crate::ai_mapping::maps(self.db, self.project, connection, "stream") == Some(false)
        {
            // A package's `provide ai` that maps no `stream:` (chunk 5.5v7).
            let package = crate::connections::package_connections(self.db, self.project)
                .connections
                .iter()
                .find(|known| known.name == *connection)
                .map(|known| known.package.clone())
                .unwrap_or_default();
            self.out.push(
                Diagnostic::new(
                    &rule::AI_SETTINGS_INVALID,
                    format!("`{connection}` doesn't stream"),
                )
                .at(span, "no `stream:` to ask through")
                .reason(format!(
                    "`ai: {{ connection: }}` names `{connection}`, and its package `{package}` maps no `stream:` in its `provides/ai.jadpo`, so it has no streamed answer to give."
                ))
                .next("Name a connection in `ai:` whose package maps `stream:`, or ask with `ai.generate`"),
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
            let mut problem = Diagnostic::new(
                &rule::SEMANTIC_UNKNOWN_NAME,
                format!("`{model}` isn't a model `ai:` names"),
            )
            .at(at.clone(), "no such model")
            .reason(format!(
                "`model:` names one of the models `ai: {{ models: }}` lists in `app.jadpo`: {}.",
                crate::and_list(&names)
            ));
            problem = match nearest(model, settings.model_names()) {
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
        let Some(prompt) = prompt else {
            return (
                Ty::Error,
                CallKind::Effect(written.to_string()),
                Reach::Unknown,
            );
        };
        let key = node_key(call.syntax());
        self.facts.types.insert(key, ai_stream());
        if ok && !self.streams.contains(&prompt) {
            self.streams.push(prompt.clone());
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
            ai_stream(),
            CallKind::Effect(written.to_string()),
            Reach::Stream {
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
            },
        )
    }
}
