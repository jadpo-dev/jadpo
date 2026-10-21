//! A body's sites of areas 10, 11, 16 and 21 (roadmap chunk 8.1i;
//! docs/compiler-runtime.md §2.5, "Compiled code"): one entry point each,
//! which the interpreter and compiled code both call, so what a site does
//! is written once and only how each executor stops is its own.
//!
//! - [`files`]: a store's `put` and `read` (area 16), through the `files`
//!   capability.
//! - [`provider`]: a provider package's operation (area 11), and
//!   [`provider_maps`] and [`mapped`], how an `attempt` maps a failure of
//!   one, or of a package's action, by its code.
//! - [`ai`]: `ai.generate`, `ai.stream`, `ai.embed` and `ai.embed_all`
//!   (area 21), each by its site as the plan decodes it ([`Asked`]).
//! - [`translate`], [`render_prompt`] and [`escape`]: a translation call
//!   (area 10), `prompts.render` (area 21), and a value an email's HTML
//!   template writes out (area 16).
//! - With the `testing` feature, a test's `fakes:` ([`fake`]), its
//!   `fakes: { ai: }` ([`ai_fake`]) and `ai.<prompt>.requests`
//!   ([`ai_requests`]) (area 13).
//!
//! Each gives a [`Called`], which each executor turns into the stop its
//! walker raises at the site.

use jadpo_std::RuleKey;
use jadpo_std::ai::{GenerateSite, StreamSite};
use jadpo_std::compiled::CallPlan;
use jadpo_std::embed::EmbedSite;
use jadpo_std::files::FilesOp;
use jadpo_std::provider::{CallSite, Called as Operation};
use jadpo_value::Value;

use crate::provider::Functions;
use crate::{Ctx, Failure, Fault, FaultKind, Plan};

/// How a site's call ended: what each executor turns into its stop.
#[derive(Debug)]
pub enum Called {
    Value(Value),
    /// A condition the call's `attempt` maps (`storage_full`, an `ai`
    /// call's five), as a report names it, with `json.read`'s or an
    /// answer's last `DecodeProblem` for `malformed`.
    Broke {
        rule: RuleKey,
        written: String,
        problem: Option<Value>,
    },
    /// The operation's own failure, or what the runtime raises in its
    /// place: `unavailable`, `misconfigured`, `outcome_unknown`.
    Failed(Box<Failure>),
    Fault(Box<Fault>),
}

impl Called {
    fn fault(ctx: &mut Ctx<'_>, kind: FaultKind, at: u32) -> Called {
        Called::Fault(ctx.fault(kind, at))
    }

    fn defect(ctx: &mut Ctx<'_>, what: &str, at: u32) -> Called {
        Called::fault(ctx, FaultKind::Defect(what.to_string()), at)
    }

    fn broke(rule: RuleKey, problem: Option<Value>) -> Called {
        Called::Broke {
            rule,
            written: rule.word().to_string(),
            problem,
        }
    }
}

// --- files ----------------------------------------------------------------------

/// `store.put(bytes, type)` or `store.read(file)` at site `at`, `store`
/// being connection `connection` (area 16, chunk 6.4b), through the
/// runtime's `files` capability: an outside call, which the seam refuses
/// once the action's transaction is open.
pub async fn files(
    ctx: &mut Ctx<'_>,
    connection: &str,
    op: FilesOp,
    args: &[Value],
    at: u32,
) -> Called {
    use crate::files::{self, Done};
    let done = match (op, args) {
        (FilesOp::Put, [bytes, file_type]) => files::put(ctx, connection, bytes, file_type).await,
        (FilesOp::Read, [file]) => files::read_whole(ctx, file).await,
        _ => Done::Fault(FaultKind::Defect(format!(
            "`{connection}.{}` called with the wrong arguments",
            op.name()
        ))),
    };
    match done {
        Done::Value(value) => Called::Value(value),
        Done::Broke(rule) => Called::broke(rule, None),
        Done::Failed(failure) => Called::Failed(failure),
        Done::Fault(kind) => Called::fault(ctx, kind, at),
    }
}

// --- providers --------------------------------------------------------------------

/// `connection.resource.operation(input)` at site `at`, as `call` decodes
/// it (area 11, chunk 7.2c): the runtime makes the request from `plan`,
/// sends it through the capability seam and decodes the answer, the
/// package's own decoders and encoders run by `functions`, the executor,
/// at the site (chunk 7.2c2). A failure the operation declares is the
/// call's, for its `attempt` to map.
pub async fn provider(
    ctx: &mut Ctx<'_>,
    plan: Plan<'_>,
    functions: &dyn Functions,
    call: &CallSite,
    input: Option<Value>,
    at: u32,
) -> Called {
    use crate::provider::{self, Done};
    let calling = provider::Calling {
        plan,
        site: provider::Site {
            call,
            functions,
            at,
        },
    };
    match provider::call(ctx, calling, input).await {
        Done::Value(value) => Called::Value(value),
        Done::Failed(failure) => Called::Failed(failure),
        Done::Fault(kind) => Called::fault(ctx, kind, at),
    }
}

/// The key of the failure a provider call that ran out of time raises
/// ([`crate::provider::timed_out`]).
const TIMED_OUT: &str = "runtime timed_out";

/// Whether failure `key`, which a call of `call` gave, is one its
/// `attempt` maps by its code (area 11, chunk 7.2c): one its operation
/// declares, its package's, or `timed_out` (chunk 7.2c2). A package
/// action's own operation's (`Called::Own`) passes on as it is.
pub fn provider_maps(plan: Plan<'_>, call: &CallSite, key: &str) -> bool {
    let Operation::Operation(number) = call.called else {
        return false;
    };
    key == TIMED_OUT
        || plan
            .providers
            .and_then(|providers| providers.operations.get(number as usize))
            .is_some_and(|op| op.failures.iter().any(|known| known == key))
}

/// The arm of an `attempt` that maps `failure`, a provider operation's or
/// a package action's, by its code, among `named`, each arm's code in the
/// order written; the fault naming `connection.operation` at the call's
/// site, `inner`, when none does: such a failure is never passed on.
pub fn mapped(
    ctx: &mut Ctx<'_>,
    named: &[Option<&str>],
    failure: &Failure,
    (connection, operation): (&str, &str),
    inner: u32,
) -> Result<u32, Box<Fault>> {
    let code = failure.code.as_str();
    match named.iter().position(|arm| *arm == Some(code)) {
        Some(at) => Ok(u32::try_from(at).unwrap_or(u32::MAX)),
        None => Err(ctx.fault(
            FaultKind::Provider(format!(
                "`{connection}.{operation}` was refused with `{code}`, which its call maps to nothing"
            )),
            inner,
        )),
    }
}

// --- models -------------------------------------------------------------------------

/// An `ai` call's site, as the plan decodes it: `None` in a program that
/// names no model.
#[derive(Clone, Copy, Debug)]
pub enum Asked<'s> {
    Generate(Option<&'s GenerateSite>),
    Stream(Option<&'s StreamSite>),
    Embed(Option<&'s EmbedSite>),
}

impl<'s> Asked<'s> {
    /// The `ai` call outside call `call` is; `None` for a provider's.
    pub fn of(call: &'s CallPlan) -> Option<Asked<'s>> {
        match call {
            CallPlan::Generate(site) => Some(Asked::Generate(Some(site))),
            CallPlan::Stream(site) => Some(Asked::Stream(Some(site))),
            CallPlan::Embed(site) => Some(Asked::Embed(Some(site))),
            CallPlan::Provider(_) => None,
        }
    }
}

/// An `ai` call at site `at` (area 21): `ai.generate(prompt, as: T)`
/// (chunk 5.5v3), `ai.stream(prompt)` (chunk 5.5v6), `ai.embed(text, as:
/// V)` or `ai.embed_all(texts, as: V)` (chunk 5.5v4), with `given`, which
/// the runtime answers through the connection `ai:` names; a condition it
/// ends in is the call's, for its `attempt` to map.
pub async fn ai(
    ctx: &mut Ctx<'_>,
    plan: Plan<'_>,
    asked: Asked<'_>,
    given: Value,
    at: u32,
) -> Called {
    use crate::ai::{Calling, Done};
    let done = match asked {
        Asked::Generate(Some(site)) => {
            crate::ai::generate(ctx, Calling { plan, site }, given).await
        }
        Asked::Stream(Some(site)) => {
            crate::stream::stream(ctx, Calling { plan, site }, given).await
        }
        Asked::Embed(Some(site)) => crate::embed::embed(ctx, Calling { plan, site }, given).await,
        Asked::Generate(None) | Asked::Stream(None) => Done::Fault(FaultKind::Defect(
            "a model asked in a program that names none".into(),
        )),
        Asked::Embed(None) => Done::Fault(FaultKind::Defect(
            "an embedding asked in a program that names no model".into(),
        )),
    };
    match done {
        Done::Value(value) => Called::Value(value),
        Done::Broke { rule, problem } => Called::broke(rule, problem),
        Done::Failed(failure) => Called::Failed(failure),
        Done::Fault(kind) => Called::fault(ctx, kind, at),
    }
}

// --- translations, prompts and templates ------------------------------------------

/// `translations.<message>(locale, …)` at site `at` (area 10): the plan's
/// translation `key` written in the locale `values` starts with, from the
/// rest, in the order of its parameters.
pub fn translate(
    ctx: &mut Ctx<'_>,
    plan: Plan<'_>,
    key: &str,
    values: &[Value],
    at: u32,
) -> Called {
    let Some((locale, rest)) = values.split_first() else {
        return Called::defect(ctx, "a translation without its locale", at);
    };
    let Some((_, entry)) = plan
        .translations
        .and_then(|translations| translations.entry(key))
    else {
        return Called::defect(ctx, "a translation key the plan doesn't have", at);
    };
    let Some(locales) = plan.locales else {
        return Called::defect(ctx, "a translation in a project with no locales", at);
    };
    match jadpo_value::present::translate(entry, locale, rest, locales) {
        Ok(text) => Called::Value(text),
        Err(fault) => Called::Fault(ctx.fault(fault.into(), at)),
    }
}

/// `prompts.render(prompt)` at site `at` (area 21): the prompt at place
/// `entry` among the plan's, its value `prompt`, as the messages a model
/// is sent, each field it writes given as its type's JSON, by `fields`,
/// each its number in the plan's JSON table.
pub fn render_prompt(
    ctx: &mut Ctx<'_>,
    plan: Plan<'_>,
    entry: u32,
    fields: &[u32],
    prompt: &Value,
    at: u32,
) -> Called {
    let rendered = plan
        .prompts
        .and_then(|prompts| prompts.entries.get(entry as usize))
        .and_then(|entry| {
            jadpo_value::prompt::render(entry, prompt, &mut |place, value| {
                let number = fields.get(place)?;
                let codec = plan.json.codec_of(*number)?;
                Some(plan.json.codecs.write_value(codec, value))
            })
        });
    match rendered {
        Some(messages) => Called::Value(messages),
        None => Called::defect(ctx, "a prompt the plan can't render", at),
    }
}

/// A value an email's HTML template writes out at site `at` (area 16):
/// its text, escaped.
pub fn escape(ctx: &mut Ctx<'_>, value: &Value, at: u32) -> Called {
    match value {
        Value::Text(text) => Called::Value(Value::text(&jadpo_std::email::escape_html(text))),
        _ => Called::defect(ctx, "a template writing out something that isn't text", at),
    }
}

// --- a test's fakes -------------------------------------------------------------------

/// What a test's `fakes:` scripts a provider operation to answer (area 13;
/// chunk 7.2d), as written.
#[cfg(feature = "testing")]
#[derive(Clone, Debug, PartialEq)]
pub enum FakeAnswer {
    /// A value of the operation's output type.
    Value(Value),
    /// One of its package's failures, by its code.
    Failed(String),
    /// `unavailable` or `in_use`: it surely did nothing.
    Unavailable,
    /// `lost`: it went and no answer came back.
    Lost,
    Misconfigured,
}

/// A test's `fakes:` at site `at` giving the fake in connection
/// `connection`'s place the next answer to `operation` (area 13; chunk
/// 7.2d): a value, written as the provider sends it where the operation's
/// `output_at:` reads it, its package's encoders run by `functions`, so it
/// is read back through the operation's real decoding.
#[cfg(feature = "testing")]
pub fn fake(
    ctx: &mut Ctx<'_>,
    plan: Plan<'_>,
    functions: &dyn Functions,
    (connection, operation): (&str, &str),
    answer: FakeAnswer,
    at: u32,
) -> Called {
    use crate::fakes::Script;
    let given = match answer {
        FakeAnswer::Value(value) => Script::Value(value),
        FakeAnswer::Failed(code) => Script::Failed(code),
        FakeAnswer::Unavailable => Script::Unavailable,
        FakeAnswer::Lost => Script::Lost,
        FakeAnswer::Misconfigured => Script::Misconfigured,
    };
    ctx.at(at);
    let started = ctx.started();
    let Some(providers) = plan.providers(&started) else {
        return Called::defect(
            ctx,
            "a scripted answer in a program with no provider plan",
            at,
        );
    };
    match crate::fakes::script(
        ctx,
        &providers,
        (functions, at),
        (connection, operation),
        given,
    ) {
        Ok(()) => Called::Value(Value::Unit),
        Err(why) => Called::defect(ctx, why, at),
    }
}

/// What a test's `fakes: { ai.<prompt>: [...] }` scripts one attempt of a
/// model's call to answer (area 21, "Tests"; chunk 5.5v3), as written.
#[cfg(feature = "testing")]
#[derive(Clone, Debug, PartialEq)]
pub enum AiAnswer<'a> {
    /// A value of the call's `as:` type, with the number of that type in
    /// the plan's JSON table.
    Value(Value, Option<u32>),
    /// `raw("...")`'s text.
    Raw(Value),
    /// One of the call's failures, by its word.
    Failed(&'a str),
    /// `partial("...", <condition>)`: its text, and the condition's word.
    Partial(Value, &'a str),
    /// `recorded("<name>")`: the prompt's recording of that name.
    Recorded(&'a str),
}

/// A test's `fakes: { ai.<prompt>: }` at site `at` giving the `ai` fake
/// the next answer to prompt `prompt` (area 21; chunk 5.5v3): a value
/// written as its call's type's JSON, `raw(...)`'s text, a condition, or a
/// recording's answer with its placeholders filled; for an embedding,
/// one of its failures (chunk 5.5v4).
#[cfg(feature = "testing")]
pub fn ai_fake(
    ctx: &mut Ctx<'_>,
    plan: Plan<'_>,
    prompt: &str,
    answer: AiAnswer<'_>,
    at: u32,
) -> Called {
    match ai_script(ctx, plan, prompt, answer, at) {
        Ok(()) => Called::Value(Value::Unit),
        Err(why) => Called::defect(ctx, why, at),
    }
}

#[cfg(feature = "testing")]
fn ai_script(
    ctx: &mut Ctx<'_>,
    plan: Plan<'_>,
    prompt: &str,
    answer: AiAnswer<'_>,
    at: u32,
) -> Result<(), &'static str> {
    use crate::ai::Script;
    let started = ctx.started();
    let Some(ai) = plan.ai(&started) else {
        return Err("a model's scripted answer in a program that asks none");
    };
    // An embedding's failure (chunk 5.5v4).
    if prompt == jadpo_std::embed::OPERATION {
        let AiAnswer::Failed(word) = answer else {
            return Err("an embedding scripted with what isn't a failure");
        };
        let Some(rule) = RuleKey::from_word(word) else {
            return Err("a scripted failure an embedding has none of");
        };
        ctx.at(at);
        return crate::embed::script(ctx, &ai, rule);
    }
    let given = match answer {
        AiAnswer::Value(value, number) => {
            let Some(codec) = number.and_then(|number| ai.json.codec_of(number)) else {
                return Err("a scripted value with no codec");
            };
            let json = ai.json.codecs.write_value(codec, &value);
            // As the model sends it: a text answer as its text, a value
            // that isn't an object wrapped as a strict mode asks.
            match plan.generate_site(prompt) {
                Some(generate) => Script::Content(crate::ai::as_sent(&ai, generate, &json)),
                // A streamed prompt's text, as it is (chunk 5.5v6).
                None => match value {
                    Value::Text(text) => Script::Content(text.to_string()),
                    _ => return Err("a scripted value of a prompt no call asks"),
                },
            }
        }
        AiAnswer::Partial(value, word) => match (value, RuleKey::from_word(word)) {
            (Value::Text(text), Some(rule)) => Script::Partial(text.to_string(), rule),
            _ => return Err("`partial(...)` given no text or condition"),
        },
        AiAnswer::Raw(value) => match value {
            Value::Text(text) => Script::Content(text.to_string()),
            _ => return Err("`raw(...)` given no text"),
        },
        AiAnswer::Failed(word) => match RuleKey::from_word(word) {
            Some(rule) => Script::Condition(rule),
            None => return Err("a scripted failure `ai.generate` has none of"),
        },
        // The prompt's recording, its placeholders filled, sent as the
        // model sends an answer (chunk 5.5v4).
        AiAnswer::Recorded(name) => {
            let Some(generate) = plan.generate_site(prompt) else {
                return Err("a recorded answer of a prompt no call asks");
            };
            let Some(recording) = plan
                .testing()
                .prompt_recordings
                .iter()
                .find(|recording| recording.prompt == prompt && recording.name == name)
            else {
                return Err("a recording the plan doesn't have");
            };
            match crate::prompt_recordings::answer_at(
                ctx,
                &ai.json,
                generate.answer,
                &recording.text,
            ) {
                Some(json) => Script::Content(crate::ai::as_sent(&ai, generate, &json)),
                // One that keeps no answer, which its own test reports,
                // answers with nothing to decode.
                None => Script::Condition(RuleKey::Malformed),
            }
        }
    };
    ctx.at(at);
    crate::ai::script(ctx, &ai, prompt, given)
}

/// `ai.<prompt>.requests` at site `at` (area 21): the prompt values the
/// code asked prompt `prompt` with, as the `ai` fake kept them.
#[cfg(feature = "testing")]
pub fn ai_requests(ctx: &mut Ctx<'_>, plan: Plan<'_>, prompt: &str, at: u32) -> Called {
    let started = ctx.started();
    match plan.ai(&started) {
        Some(ai) => Called::Value(crate::ai::requests(ctx, &ai, prompt)),
        None => Called::defect(ctx, "`.requests` in a program that asks no model", at),
    }
}
