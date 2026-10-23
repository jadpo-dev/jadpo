//! The `ai` capability's `generate` (area 21, "The `ai` capability" and
//! "Typed output"; chunk 5.5v3): what both executors run at an
//! `ai.generate(prompt, as: T)` call's site.
//!
//! [`generate`] renders the prompt as `prompts.render` does, asks the
//! model the call's `model:` names through the connection `ai:` names, on
//! the one outside-call path ([`crate::outside`]), so a call is refused
//! inside an open transaction, logged without its values, recorded and
//! replayed with the other effects, and held to its connection's
//! `max_concurrent:` where a package fills `ai`, and decodes the answer
//! through `json.read`'s decoding of `T`, every rule included:
//!
//! 1. **The schema.** `T`'s JSON Schema, written at build time with each
//!    field's description, is sent in the provider's structured-output
//!    mode cut to what it enforces ([`strict`]), a `T` whose JSON isn't an
//!    object as the member `value` of one; or, where the provider has no
//!    such mode, added to the instructions under a fixed heading
//!    ([`with_schema`]). A `T` built on `Text` asks for plain text.
//! 2. **One repair.** An answer that doesn't decode is answered with the
//!    answer and its `DecodeProblem` (its path and rule, never a value),
//!    while the call's `attempts:` allow ([`correction`]).
//! 3. **The five failures**, each a condition the call's `attempt` maps:
//!    `refused` (the model declined), `malformed` (no attempt decoded,
//!    with the last `DecodeProblem`), `timed_out` (no whole answer within
//!    `timeout:`, the sends a provider asked for again included),
//!    `over_budget` (cut off at `max_output:`) and `too_long` (the prompt
//!    is longer than the model's context). A rejected key or an unknown
//!    model is `misconfigured`, and a provider that can't be reached
//!    within the call's `timeout:` is `unavailable`.
//! 4. **Answers reused on a rerun.** An action run again after a
//!    concurrent change (area 6) takes the same outside-call steps, so
//!    each call's answer is kept by its step and the request it sent, and
//!    a rerun asking the same is given it back without asking again; in an
//!    action whose steps are recorded (area 12, chunk 7.5b) it is a step of
//!    the record, sealed, so a redelivery reuses it too.
//!
//! The built-in driver, `Driver.openai_compatible`, speaks OpenAI's Chat
//! Completions API (`POST <url>/chat/completions`) through the package
//! driver's HTTP; a test's fake ([`AiFake`]) answers in the same form, so
//! what a test scripts runs through the real reading, decoding and repair.
//! Prompts and answers appear in no error's text; only `jadpo dev`'s call
//! log keeps them, in full, as area 21 allows.

#[cfg(feature = "testing")]
use std::cell::RefCell;
#[cfg(feature = "testing")]
use std::collections::VecDeque;
use std::rc::Rc;

use jadpo_std::ai::{
    AiDriver, AiEnvelope, AiInput, AiPlan, AiUrl, GenerateSite, PackageAi, SchemaMode,
};
use jadpo_std::prompt::PromptPlan;
use jadpo_std::subscriber::Repeat;
use jadpo_std::{HttpMethod, RuleKey};
use jadpo_value::Value;
use jadpo_value::json::{Problem, Unread};

#[cfg(feature = "testing")]
use crate::LocalFuture;
#[cfg(feature = "testing")]
use crate::capability::Capability;
use crate::capability::{Answer, Call, Source};
use crate::effects::Unanswered;
use crate::outside::{Asking, Comes, Form, Rerun, Retry, Sending};
#[cfg(feature = "testing")]
use crate::places::Frame;
use crate::provider::{Providers, Reading, Request};
use crate::{Ctx, Failure, FaultKind, JsonCodecs};

/// What a program asks models with: its `ai:`, its prompts, and the
/// codecs its prompts' fields and its answers' types are written and read
/// with.
#[derive(Clone, Copy)]
pub struct Ai<'a> {
    pub plan: &'a AiPlan,
    pub prompts: &'a PromptPlan,
    pub json: JsonCodecs<'a>,
    /// How a package's operations are called, for a package filling `ai`
    /// (chunk 5.5v3b).
    pub providers: Option<Providers<'a>>,
}

impl std::fmt::Debug for Ai<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Ai")
            .field("connection", &self.plan.connection)
            .finish_non_exhaustive()
    }
}

/// What an `ai.generate` call gives its site.
#[derive(Debug)]
pub enum Done {
    /// The answer, decoded into the call's type.
    Value(Value),
    /// One of the call's five conditions, for its `attempt` to map; with
    /// the last `DecodeProblem` for `malformed`.
    Broke {
        rule: RuleKey,
        problem: Option<Value>,
    },
    /// What the runtime raises in its place: `unavailable`,
    /// `misconfigured`.
    Failed(Box<Failure>),
    Fault(FaultKind),
}

/// The settings a call reads, by number, for its model and its address,
/// and for a package filling `ai` every other its operation's call reads:
/// [`generate`] reads their values at the call's context.
pub fn settings_read(ai: &Ai<'_>, site: &GenerateSite) -> Vec<u32> {
    let mut read: Vec<u32> = ai.plan.model(&site.model).into_iter().collect();
    // The day's cost limit, a `Money` setting (chunk 5.5v5).
    read.extend(crate::ai_budget::settings_read(ai.plan));
    match &ai.plan.driver {
        AiDriver::OpenAiCompatible(AiUrl::Setting(at)) => read.push(*at),
        AiDriver::OpenAiCompatible(AiUrl::Written(_)) => {}
        AiDriver::Package(package) => {
            read.extend(package_reads(ai, package.called, &package.input));
        }
    }
    read.sort_unstable();
    read.dedup();
    read
}

/// The settings the package's operation numbered `called` reads when
/// `ai` calls it, its credential's and address's among them, and those
/// its mapped `input` reads.
pub(crate) fn package_reads(ai: &Ai<'_>, called: u32, input: &[(String, AiInput)]) -> Vec<u32> {
    let mut read = Vec::new();
    if let Some((plan, _, _)) = ai
        .providers
        .as_ref()
        .and_then(|providers| providers.numbered(called))
    {
        read.extend(crate::provider::settings_read(plan));
    }
    fn settings(input: &AiInput, read: &mut Vec<u32>) {
        match input {
            AiInput::Setting(at) => read.push(*at),
            AiInput::Messages(parts) => {
                for (_, part) in parts {
                    settings(part, read);
                }
            }
            _ => {}
        }
    }
    for (_, input) in input {
        settings(input, &mut read);
    }
    read
}

/// What a call ended with, kept for a rerun: the answer as the model gave
/// it, or a condition.
#[derive(Clone, PartialEq, Eq)]
pub(crate) enum Kept {
    Answer(Rc<str>),
    Condition(RuleKey),
    /// `malformed`, with the last answer's problem, never its text.
    Malformed(Problem),
}

impl Kept {
    /// How a record keeps a condition: its word, and for `malformed` its
    /// problem's path and rule after it, each after a space.
    pub(crate) fn word(&self) -> Option<String> {
        match self {
            Kept::Answer(_) => None,
            Kept::Condition(rule) => Some(rule.word().to_string()),
            Kept::Malformed(problem) => Some(format!(
                "{} {} {}",
                RuleKey::Malformed.word(),
                problem.field,
                problem.rule
            )),
        }
    }

    /// How a call given it back is logged: the answer as sent, or the
    /// condition as the failure of its word.
    pub(crate) fn logged(&self) -> Answer {
        match self {
            Kept::Answer(text) => Answer::Wire(text.clone()),
            Kept::Condition(rule) => Answer::Failed(rule.word().to_string()),
            Kept::Malformed(_) => Answer::Failed(RuleKey::Malformed.word().to_string()),
        }
    }

    /// As a step of the action's record keeps it: the answer as sent, or
    /// the condition by its word.
    pub(crate) fn answered(&self) -> crate::steps::Answered {
        match self {
            Kept::Answer(text) => crate::steps::Answered::Wire(text.clone()),
            condition => crate::steps::Answered::Failed(condition.word().unwrap_or_default()),
        }
    }

    /// What a step of the action's record kept, read back; what is wrong
    /// with it otherwise.
    pub(crate) fn recorded(answered: crate::steps::Answered) -> Result<Kept, &'static str> {
        match answered {
            crate::steps::Answered::Wire(text) => Ok(Kept::Answer(text)),
            crate::steps::Answered::Failed(word) => {
                Kept::from_word(&word).ok_or("a recorded answer of no condition")
            }
            crate::steps::Answered::Done => Err("a recorded answer with nothing"),
        }
    }

    /// A condition as [`Kept::word`] writes it.
    pub(crate) fn from_word(word: &str) -> Option<Kept> {
        let mut parts = word.splitn(3, ' ');
        let rule = RuleKey::from_word(parts.next()?)?;
        match (rule, parts.next(), parts.next()) {
            (RuleKey::Malformed, field, rule) => Some(Kept::Malformed(Problem {
                field: field.unwrap_or_default().to_string(),
                rule: rule
                    .filter(|rule| !rule.is_empty())
                    .unwrap_or(RuleKey::Malformed.word())
                    .to_string(),
            })),
            (rule, None, None) => Some(Kept::Condition(rule)),
            _ => None,
        }
    }
}

impl std::fmt::Debug for Kept {
    /// Never the answer.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Kept::Answer(_) => f.write_str("Answer(..)"),
            Kept::Condition(rule) => f.debug_tuple("Condition").field(rule).finish(),
            Kept::Malformed(problem) => f.debug_tuple("Malformed").field(problem).finish(),
        }
    }
}

/// One call's answer an invocation keeps for a rerun: the step it took
/// and a digest of the request it sent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Reused {
    pub step: u32,
    pub digest: [u8; 32],
    pub kept: Kept,
}

/// The operation a call is logged and recorded as: the prompt's name.
fn operation(site: &GenerateSite) -> &str {
    &site.prompt
}

/// How a call's requests are sent: to the built-in driver's address, or
/// as a package's operation.
enum Via<'p> {
    /// `<url>/chat/completions`.
    Driver(String),
    Package {
        package: &'p PackageAi,
        providers: Providers<'p>,
    },
}

/// An `ai` call's site in a program, as an executor hands it to
/// [`generate`], [`crate::stream::stream`] or [`crate::embed::embed`]
/// (chunk 7.9e): the plan it is made in and its site as the plan decodes
/// it.
pub struct Calling<'a, S> {
    pub plan: crate::Plan<'a>,
    pub site: &'a S,
}

impl<S> Clone for Calling<'_, S> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<S> Copy for Calling<'_, S> {}

impl<S> std::fmt::Debug for Calling<'_, S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Calling").finish_non_exhaustive()
    }
}

impl<'a, S> Calling<'a, S> {
    /// What the call runs with, its credentials from `started`, and the
    /// values of the settings `read` names it reads at `ctx`: a test's
    /// own, else the ones the program started with. The fault its site
    /// raises, saying `what`, in a program that asks no model.
    pub(crate) fn ai(
        self,
        ctx: &Ctx<'_>,
        started: &'a crate::Started,
        read: impl FnOnce(&Ai<'a>) -> Vec<u32>,
        what: &str,
    ) -> Result<(Ai<'a>, Vec<(u32, Value)>), Done> {
        let Some(ai) = self.plan.ai(started) else {
            return Err(Done::Fault(FaultKind::Defect(what.to_string())));
        };
        let settings = ctx.settings_of(read(&ai));
        Ok((ai, settings))
    }
}

/// Asks the model for an answer to `prompt`, a value of the prompt's type
/// `calling.site` renders, as the site's type: what both executors run at
/// an `ai.generate` call's site.
pub async fn generate(
    ctx: &mut Ctx<'_>,
    calling: Calling<'_, GenerateSite>,
    prompt: Value,
) -> Done {
    let started = ctx.started();
    let (ai, settings) = match calling.ai(
        ctx,
        &started,
        |ai| settings_read(ai, calling.site),
        "a model asked in a program that names none",
    ) {
        Ok(made) => made,
        Err(done) => return done,
    };
    generated(ctx, &ai, calling.site, prompt, &settings).await
}

/// [`generate`], with `ai` and the values `settings` gives the settings
/// [`settings_read`] names.
async fn generated(
    ctx: &mut Ctx<'_>,
    ai: &Ai<'_>,
    site: &GenerateSite,
    prompt: Value,
    settings: &[(u32, Value)],
) -> Done {
    let defect = |what: &str| Done::Fault(FaultKind::Defect(what.to_string()));
    let Some(entry) = ai.prompts.entries.get(site.entry as usize) else {
        return defect("a prompt the plan doesn't have");
    };
    let codecs = ai.json.codecs;
    let rendered = jadpo_value::prompt::render(entry, &prompt, &mut |place, value| {
        let number = site.fields.get(place)?;
        let codec = ai.json.codec_of(*number)?;
        Some(codecs.write_value(codec, value))
    });
    let Some(messages) = rendered.as_ref().and_then(messages_of) else {
        return defect("a prompt that doesn't render");
    };
    let setting = |number: u32| {
        settings
            .iter()
            .find(|(at, _)| *at == number)
            .map(|(_, value)| value)
    };
    // The model's text, `openai/gpt-4.1-mini`, as its setting holds it.
    let model = ai
        .plan
        .model(&site.model)
        .and_then(setting)
        .and_then(|value| match jadpo_value::standard::from(value) {
            Ok(Value::Text(text)) => Some(text.to_string()),
            _ => None,
        });
    let made = Call::new(
        &ai.plan.connection,
        operation(site),
        prompt.clone(),
        Repeat::Safe,
    );
    let misconfigured = |ctx: &mut Ctx<'_>| {
        // A model or an address the connection can't be reached with is
        // the connection's settings: nothing is sent.
        let now = ctx.now();
        ctx.capabilities_mut().refused(&made, now);
        Done::Failed(Box::new(crate::misconfigured()))
    };
    let Some(model) = model else {
        return misconfigured(ctx);
    };
    let mode = ai.plan.schema;
    let asked = Asked {
        model,
        schema: site.schema.as_deref().map(|schema| {
            serde_json::from_str::<serde_json::Value>(schema).unwrap_or(serde_json::Value::Null)
        }),
        wrapped: mode == SchemaMode::StrictSubset && !site.object,
        mode,
        envelope: match &ai.plan.driver {
            AiDriver::Package(package) => package.envelope,
            AiDriver::OpenAiCompatible(_) => AiEnvelope::OpenAi,
        },
    };
    let mut messages = match &asked.schema {
        Some(schema) if mode != SchemaMode::StrictSubset => with_schema(messages, schema),
        _ => messages,
    };
    let via = match &ai.plan.driver {
        AiDriver::OpenAiCompatible(url) => {
            let base = match url {
                AiUrl::Written(text) => Some(text.clone()),
                AiUrl::Setting(at) => match setting(*at) {
                    Some(Value::Text(text)) => Some(text.to_string()),
                    _ => None,
                },
            };
            let Some(base) = base.and_then(|base| crate::provider::sendable(&base)) else {
                return misconfigured(ctx);
            };
            Via::Driver(format!("{}/chat/completions", base.trim_end_matches('/')))
        }
        AiDriver::Package(package) => {
            let Some(providers) = ai.providers else {
                return defect("a package filling `ai` with no providers to call");
            };
            // Checked before sending, from the package's table, so a prompt
            // longer than the model's context costs nothing (area 21).
            if let Some(context) = package.context(model_name(&asked.model)) {
                let prompt_tokens: u64 = messages
                    .iter()
                    .map(|(_, content)| jadpo_std::ai::estimated_tokens(content))
                    .sum();
                if prompt_tokens > context {
                    return broke(RuleKey::TooLong);
                }
            }
            Via::Package { package, providers }
        }
    };
    let first = match prepared(ctx, &via, &asked, &messages, site, settings) {
        Ok(request) => request,
        Err(crate::provider::Unready::Misconfigured) => return misconfigured(ctx),
        Err(crate::provider::Unready::Fault(kind)) => return Done::Fault(kind),
    };
    let body = first.body.clone().unwrap_or_default();
    let digest = jadpo_value::hash::sha256(body.as_bytes());
    // Given back on a rerun: from the record of the action's steps, or
    // from the first run's answers kept by the invocation.
    let name = format!("{}.{}", ai.plan.connection, site.prompt);
    let fields = vec![("request".to_string(), body.into_bytes())];
    let comes = crate::outside::comes_from(
        ctx,
        Asking {
            step: Some(crate::steps::Proposed {
                operation: &name,
                fields,
                repeat: Repeat::Safe,
                changes: false,
                window_ms: None,
                did_nothing: &[],
            }),
            rerun: Some(Rerun::Answer(digest)),
        },
    )
    .await;
    let place = match comes {
        Ok(Comes::Send(place)) => place,
        Ok(Comes::Recorded { answered, at }) => {
            let kept = match Kept::recorded(answered) {
                Ok(kept) => kept,
                Err(what) => return defect(what),
            };
            // Logged as the step it is (chunk 7.5f).
            let mut made = made;
            made.step = at;
            return replayed(ctx, ai, site, &asked, made, kept);
        }
        Ok(Comes::Reused(kept)) => return replayed(ctx, ai, site, &asked, made, kept),
        Ok(Comes::Opened(_)) => return defect("a model's answer given back as a stream"),
        Err(refused) => return unanswered(refused),
    };
    // The prompt's values, for a development log alone, where `jadpo ai
    // record` reads the last call's (chunk 5.5v4b).
    let values = ctx
        .host()
        .call_log()
        .and_then(|_| prompt_values(ai, site, &prompt));
    let logged: crate::outside::Log<'_> =
        &|ctx, call, answer| log(ctx, call, values.as_deref(), answer);
    let max_concurrent = max_concurrent_of(ai);
    let host = ctx.host();
    let began = host.monotonic_ms();
    let mut last_problem: Option<Problem> = None;
    let mut ended: Option<(Kept, Done)> = None;
    let mut first = Some(first);
    // What the call may use, taken from every limit before it is sent
    // (area 21, "Budgets"; chunk 5.5v5).
    let input: u64 = messages
        .iter()
        .map(|(_, content)| jadpo_std::ai::estimated_tokens(content))
        .sum();
    let mut taken = match crate::ai_budget::take(
        ctx,
        ai,
        crate::ai_budget::Asked::of(site),
        &asked.model,
        input,
        settings,
    )
    .await
    {
        Ok(taken) => taken,
        Err(crate::ai_budget::Refused::OverBudget) => {
            let now = ctx.now();
            ctx.capabilities_mut().refused(&made, now);
            ended = Some((
                Kept::Condition(RuleKey::OverBudget),
                broke(RuleKey::OverBudget),
            ));
            None
        }
        Err(crate::ai_budget::Refused::Unavailable) => {
            return Done::Failed(Box::new(crate::unavailable()));
        }
        Err(crate::ai_budget::Refused::Misconfigured) => return misconfigured(ctx),
    };
    let mut used = crate::ai_budget::Used::default();
    let attempts = if ended.is_some() {
        0
    } else {
        site.attempts.max(1)
    };
    for attempt in 1..=attempts {
        let request = match first.take() {
            Some(request) => request,
            None => match prepared(ctx, &via, &asked, &messages, site, settings) {
                Ok(request) => request,
                Err(crate::provider::Unready::Misconfigured) => {
                    let done = misconfigured(ctx);
                    return crate::ai_budget::debited(ctx, taken, used, done).await;
                }
                Err(crate::provider::Unready::Fault(kind)) => {
                    return crate::ai_budget::debited(ctx, taken, used, Done::Fault(kind)).await;
                }
            },
        };
        let sent_tokens: u64 = messages
            .iter()
            .map(|(_, content)| jadpo_std::ai::estimated_tokens(content))
            .sum();
        let mut call = made.clone();
        call.attempt = attempt;
        call.step = place.step;
        // A repair longer than its call counted takes the rest before it
        // is sent, and isn't sent when a limit can't cover it (chunk 7.9c).
        let counted = crate::ai_budget::sent_most(input, site.max_output, attempt);
        if sent_tokens > counted
            && !crate::ai_budget::top_up(ctx, &mut taken, sent_tokens - counted).await
        {
            let now = ctx.now();
            ctx.capabilities_mut().refused(&call, now);
            ended = Some((
                Kept::Condition(RuleKey::OverBudget),
                broke(RuleKey::OverBudget),
            ));
            break;
        }
        let sending = Sending {
            retry: Retry::timed(began, site.timeout_ms),
            form: Form::Whole,
            max_concurrent,
            resend: Some(crate::ai_budget::Resend {
                taken: &mut taken,
                used: &mut used,
                input: sent_tokens,
            }),
            log: Some(logged),
        };
        let (answer, source) =
            match crate::outside::send(ctx, &call.with_request(request), sending).await {
                Ok(sent) => {
                    let source = sent.source;
                    (sent.whole(), source)
                }
                Err(refused) => {
                    used.add(crate::ai_budget::Used {
                        input: sent_tokens,
                        ..Default::default()
                    });
                    let done = unanswered(refused);
                    return crate::ai_budget::debited(ctx, taken, used, done).await;
                }
            };
        // A fake answers in the built-in driver's own form, whatever the
        // connection (area 21, "Tests"), and so does its generated
        // default: an answer of the call's type, made as the model would
        // send it.
        let faked = !matches!(source, Source::Live | Source::Sandbox);
        let (answer, faked) = match answer {
            #[cfg(feature = "testing")]
            Answer::Generated => (Answer::Wire(generated_answer(ctx, ai, site, &asked)), true),
            other => (other, faked),
        };
        let built_in = faked || matches!(via, Via::Driver(_));
        // An attempt with no answer is charged what it sent.
        let unanswered = crate::ai_budget::Used {
            input: sent_tokens,
            ..Default::default()
        };
        let reply = match &answer {
            Answer::Wire(text) if built_in => match reply_of(text) {
                Some(reply) => reply,
                None => {
                    used.add(unanswered);
                    return crate::ai_budget::debited(
                        ctx,
                        taken,
                        used,
                        Done::Fault(FaultKind::Provider(format!(
                            "`{}`'s answer to `{}` isn't a chat completion",
                            ai.plan.connection, site.prompt
                        ))),
                    )
                    .await;
                }
            },
            Answer::Wire(text) => match &via {
                Via::Package { package, providers } => {
                    match package_reply(ctx, providers, &ai.plan.connection, package, text) {
                        Ok(reply) => reply,
                        Err(done) => {
                            used.add(unanswered);
                            return crate::ai_budget::debited(ctx, taken, used, done).await;
                        }
                    }
                }
                Via::Driver(_) => {
                    return crate::ai_budget::debited(
                        ctx,
                        taken,
                        used,
                        defect("a driver's answer read as a package's"),
                    )
                    .await;
                }
            },
            Answer::TimedOut => {
                used.add(unanswered);
                ended = Some((Kept::Condition(RuleKey::TimedOut), broke(RuleKey::TimedOut)));
                break;
            }
            Answer::Failed(code) if built_in && code == CONTEXT_LENGTH_EXCEEDED => {
                ended = Some((Kept::Condition(RuleKey::TooLong), broke(RuleKey::TooLong)));
                break;
            }
            Answer::Failed(_) | Answer::Misconfigured => {
                return crate::ai_budget::debited(
                    ctx,
                    taken,
                    used,
                    Done::Failed(Box::new(crate::misconfigured())),
                )
                .await;
            }
            Answer::Unavailable | Answer::RetryAfter(_) | Answer::Unknown => {
                used.add(unanswered);
                return crate::ai_budget::debited(
                    ctx,
                    taken,
                    used,
                    Done::Failed(Box::new(crate::unavailable())),
                )
                .await;
            }
            Answer::Value(_) => {
                return crate::ai_budget::debited(
                    ctx,
                    taken,
                    used,
                    defect("an answer to `ai.generate` that isn't a chat completion"),
                )
                .await;
            }
            #[cfg(feature = "testing")]
            Answer::Generated | Answer::Recorded => {
                return crate::ai_budget::debited(
                    ctx,
                    taken,
                    used,
                    defect("an answer to `ai.generate` that isn't a chat completion"),
                )
                .await;
            }
        };
        // What the provider says the attempt used, or the estimate of
        // what was sent and answered.
        used.add(reply.usage.unwrap_or(crate::ai_budget::Used {
            input: sent_tokens,
            output: jadpo_std::ai::estimated_tokens(reply.content.as_deref().unwrap_or_default()),
            cached: 0,
        }));
        if reply.refused {
            ended = Some((Kept::Condition(RuleKey::Refused), broke(RuleKey::Refused)));
            break;
        }
        if reply.cut_off {
            ended = Some((
                Kept::Condition(RuleKey::OverBudget),
                broke(RuleKey::OverBudget),
            ));
            break;
        }
        let content = reply.content.unwrap_or_default();
        match decode(ai, site, &asked, &content) {
            Ok(value) => {
                ended = Some((Kept::Answer(Rc::from(content.as_str())), Done::Value(value)));
                break;
            }
            Err(problem) => {
                if attempt < site.attempts {
                    messages.push(("assistant".to_string(), content));
                    messages.push(("user".to_string(), correction(&problem, &asked)));
                }
                last_problem = Some(problem);
            }
        }
    }
    crate::ai_budget::debit(ctx, taken, used).await;
    let (kept, done) = ended.unwrap_or_else(|| {
        let problem = last_problem.unwrap_or_else(|| Problem {
            field: String::new(),
            rule: RuleKey::Malformed.word().to_string(),
        });
        (
            Kept::Malformed(problem.clone()),
            Done::Broke {
                rule: RuleKey::Malformed,
                problem: Some(crate::effects::decode_problem(&problem)),
            },
        )
    });
    if let Err(refused) = crate::outside::keep(ctx, place, kept).await {
        return unanswered(refused);
    }
    done
}

/// What an `ai` call that couldn't go out, or whose answer couldn't be
/// kept, gives its site: the runtime's failure or fault, and a
/// capability's own failure, which an `ai` call has none of, as
/// `unavailable`.
pub(crate) fn unanswered(refused: Unanswered) -> Done {
    match refused {
        Unanswered::Failure(failure) => Done::Failed(failure),
        Unanswered::Fault(kind) => Done::Fault(kind),
        Unanswered::Failed(_) => Done::Failed(Box::new(crate::unavailable())),
    }
}

/// The `max_concurrent:` of the connection `ai:` names, when a package
/// fills it (area 11, "Calling"; chunk 7.9d): its `ai` calls take the
/// connection's slots as its own operations' calls do. The built-in
/// driver's connection takes none.
pub(crate) fn max_concurrent_of(ai: &Ai<'_>) -> Option<u32> {
    let AiDriver::Package(_) = &ai.plan.driver else {
        return None;
    };
    let providers = ai.providers?;
    jadpo_std::connections::package(providers.connections, providers.number(&ai.plan.connection))?
        .max_concurrent
}

/// The model's name as a package is asked for it: what its `ModelId`
/// writes after its provider, `gpt-4.1-mini` for `openai/gpt-4.1-mini`,
/// since the connection is the provider.
pub fn model_name(model: &str) -> &str {
    model.split_once('/').map_or(model, |(_, name)| name)
}

/// The request one attempt sends, with `messages`.
fn prepared(
    ctx: &mut Ctx<'_>,
    via: &Via<'_>,
    asked: &Asked,
    messages: &[(String, String)],
    site: &GenerateSite,
    settings: &[(u32, Value)],
) -> Result<Request, crate::provider::Unready> {
    use crate::provider::Unready;
    let (package, providers) = match via {
        Via::Driver(url) => {
            return Ok(Request {
                method: HttpMethod::Post,
                url: url.clone(),
                headers: vec![
                    ("Accept".to_string(), "application/json".to_string()),
                    ("Content-Type".to_string(), "application/json".to_string()),
                    ("user-agent".to_string(), "jadpo".to_string()),
                ],
                body: Some(request_body(asked, messages, site)),
                timeout_ms: site.timeout_ms,
                reading: Reading {
                    error_code: Some("error.code".to_string()),
                    ..Reading::default()
                },
                // Set when the call takes it.
                repeatable: false,
            });
        }
        Via::Package { package, providers } => (package, providers),
    };
    let given = Given {
        model: model_name(&asked.model),
        max_output: site.max_output,
        messages,
        instructions_apart: package.instructions_apart,
        texts: &[],
        dimensions: 0,
    };
    let json = mapped_input(&package.input, &given, settings).ok_or(Unready::Misconfigured)?;
    let added = package.schema_at.as_deref().zip(envelope(asked, site));
    package_request(
        ctx,
        providers,
        package.called,
        &json,
        settings,
        added,
        &site.prompt,
    )
}

/// The request the package's operation numbered `called` sends with
/// `json`, its input as the mapping made it, and `added` written where it
/// says; `asking` names the prompt or operation it is made for, in a
/// fault.
pub(crate) fn package_request(
    ctx: &mut Ctx<'_>,
    providers: &Providers<'_>,
    called: u32,
    json: &serde_json::Value,
    settings: &[(u32, Value)],
    added: Option<(&str, serde_json::Value)>,
    asking: &str,
) -> Result<Request, crate::provider::Unready> {
    use crate::provider::Unready;
    let Some((_, op, connection)) = providers.numbered(called) else {
        return Err(Unready::Fault(FaultKind::Defect(format!(
            "the operation `{asking}` calls, which the plan doesn't have"
        ))));
    };
    let operation = op.name.as_str();
    let input = match op.input {
        Some(number) => {
            let text = json.to_string();
            let Some((json, numbers)) = jadpo_value::json::parse(text.as_bytes()) else {
                return Err(Unready::Fault(FaultKind::Defect(
                    "a mapped input that isn't JSON".into(),
                )));
            };
            match crate::webhooks::read(providers.json, number, &json, &numbers) {
                Ok(value) => Some(value),
                // Problems by field and rule, never a value.
                Err(problems) => {
                    let described: Vec<String> = problems
                        .iter()
                        .map(|problem| format!("{} {}", problem.field, problem.rule))
                        .collect();
                    return Err(Unready::Fault(FaultKind::Provider(format!(
                        "`{connection}.{operation}`'s input mapped from `{asking}` doesn't read: {}",
                        described.join(", ")
                    ))));
                }
            }
        }
        None => None,
    };
    let call = jadpo_std::provider::CallSite::default();
    crate::provider::request_adding(
        ctx,
        providers,
        called,
        input.as_ref(),
        settings,
        crate::provider::Site {
            call: &call,
            functions: &NoFunctions,
            at: 0,
        },
        added,
    )
}

/// A package's own functions, which nothing at an `ai` call's site runs:
/// its operation's fields are read and written by their types alone.
pub(crate) struct NoFunctions;

impl crate::provider::Functions for NoFunctions {
    fn function(
        &self,
        _ctx: &mut Ctx<'_>,
        function: &str,
        _args: Vec<Value>,
        _at: u32,
    ) -> Result<Value, String> {
        Err(format!("`{function}`, a package function, at an `ai` call"))
    }
}

/// What a package's mapping reads of a call (chunks 5.5v3b and 5.5v7):
/// the model's name after its provider, `max_output:`, the rendered
/// messages, and for an embedding its texts and their dimensions.
pub(crate) struct Given<'g> {
    pub model: &'g str,
    pub max_output: u32,
    pub messages: &'g [(String, String)],
    pub instructions_apart: bool,
    pub texts: &'g [String],
    pub dimensions: u64,
}

/// An operation's input as JSON, its members by their spellings on the
/// wire, made from what a package's mapping gives them; `None` when a
/// setting it reads has no value.
pub(crate) fn mapped_input(
    input: &[(String, AiInput)],
    given: &Given<'_>,
    settings: &[(u32, Value)],
) -> Option<serde_json::Value> {
    // With `request.instructions` mapped, the instructions go there and
    // the messages are the rest; otherwise they open the messages.
    let (instructions, messages): (Vec<&str>, Vec<&(String, String)>) = if given.instructions_apart
    {
        (
            given
                .messages
                .iter()
                .filter(|(role, _)| role == "system")
                .map(|(_, content)| content.as_str())
                .collect(),
            given
                .messages
                .iter()
                .filter(|(role, _)| role != "system")
                .collect(),
        )
    } else {
        (Vec::new(), given.messages.iter().collect())
    };
    let instructions = instructions.join("\n\n");
    let one = |input: &AiInput, message: Option<&(String, String)>| -> Option<serde_json::Value> {
        Some(match input {
            AiInput::Model => given.model.into(),
            AiInput::MaxOutput => given.max_output.into(),
            AiInput::Instructions => instructions.as_str().into(),
            AiInput::Role => message?.0.as_str().into(),
            AiInput::Content => message?.1.as_str().into(),
            AiInput::Texts => serde_json::Value::Array(
                given
                    .texts
                    .iter()
                    .map(|text| text.as_str().into())
                    .collect(),
            ),
            AiInput::Dimensions => given.dimensions.into(),
            AiInput::Text(text) => text.as_str().into(),
            AiInput::Int(n) => (*n).into(),
            AiInput::Bool(flag) => (*flag).into(),
            AiInput::Setting(at) => match settings.iter().find(|(number, _)| number == at)?.1 {
                Value::Text(ref text) => serde_json::Value::from(&**text),
                Value::Int(n) => n.into(),
                Value::Bool(flag) => flag.into(),
                _ => return None,
            },
            AiInput::Messages(_) => return None,
        })
    };
    let mut members = serde_json::Map::new();
    for (member, input) in input {
        let value = match input {
            AiInput::Messages(parts) => serde_json::Value::Array(
                messages
                    .iter()
                    .map(|message| {
                        parts
                            .iter()
                            .map(|(part, input)| Some((part.clone(), one(input, Some(message))?)))
                            .collect::<Option<serde_json::Map<_, _>>>()
                            .map(serde_json::Value::Object)
                    })
                    .collect::<Option<_>>()?,
            ),
            other => one(other, None)?,
        };
        members.insert(member.clone(), value);
    }
    Some(serde_json::Value::Object(members))
}

/// What a package's answer says, read as its mapping says from the
/// operation's output, decoded by its types: the answer's text, a
/// refusal, a cut-off.
fn package_reply(
    ctx: &mut Ctx<'_>,
    providers: &Providers<'_>,
    connection: &str,
    package: &PackageAi,
    text: &str,
) -> Result<Reply, Done> {
    let Some((_, op, _)) = providers.numbered(package.called) else {
        return Err(Done::Fault(FaultKind::Defect(format!(
            "`{connection}.{}`, which the plan doesn't have",
            package.operation
        ))));
    };
    let sent = serde_json::from_str::<serde_json::Value>(text).ok();
    // On the answer as sent.
    let cut_off = package.cut_off_when.as_ref().is_some_and(|(path, equals)| {
        sent.as_ref()
            .is_some_and(|sent| equal_at(sent, path, equals))
    });
    let call = jadpo_std::provider::CallSite::default();
    let name = format!("{connection}.{}", package.operation);
    let output = match crate::provider::decode(
        ctx,
        providers,
        op,
        &name,
        text,
        crate::provider::Site {
            call: &call,
            functions: &NoFunctions,
            at: 0,
        },
    ) {
        crate::provider::Done::Value(value) => value,
        crate::provider::Done::Failed(failure) => return Err(Done::Failed(failure)),
        crate::provider::Done::Fault(kind) => return Err(Done::Fault(kind)),
    };
    let content = text_at(&output, &package.answer);
    let refused = package.refused_when.iter().any(|marker| match marker {
        jadpo_std::ai::AiMarker::Holds(path) => value_at(&output, path).is_some_and(holds),
        jadpo_std::ai::AiMarker::Equals(path, equals) => sent
            .as_ref()
            .is_some_and(|sent| equal_at(sent, path, equals)),
    });
    // Where the mapping says the tokens are; one written `0` is none.
    let usage = package.usage.as_ref().and_then(|usage| {
        let count = |path: &Option<Vec<String>>| match path {
            None => Some(0),
            Some(path) => match value_at(&output, path) {
                Some(Value::Int(n)) => u64::try_from(*n).ok(),
                Some(Value::None) => Some(0),
                Some(other) => match jadpo_value::standard::from(other) {
                    Ok(Value::Int(n)) => u64::try_from(n).ok(),
                    _ => None,
                },
                None => None,
            },
        };
        Some(crate::ai_budget::Used {
            input: count(&usage.input)?,
            output: count(&usage.output)?,
            cached: count(&usage.cached).unwrap_or(0),
        })
    });
    Ok(Reply {
        content,
        refused,
        cut_off,
        usage,
    })
}

/// The tokens a package's answer to `generate:` says it used, read as its
/// `usage:` says, `text` the answer as sent: input, output and cached;
/// `None` when the answer doesn't read or says nothing. What `jadpo
/// package conform` checks the usage of (chunk 5.5v7b).
pub fn reported_usage(ctx: &mut Ctx<'_>, ai: &Ai<'_>, text: &str) -> Option<(u64, u64, u64)> {
    let AiDriver::Package(package) = &ai.plan.driver else {
        return None;
    };
    let providers = ai.providers?;
    let reply = package_reply(ctx, &providers, &ai.plan.connection, package, text).ok()?;
    reply
        .usage
        .map(|used| (used.input, used.output, used.cached))
}

/// Whether the member of `sent` at `path`, dotted by spellings on the
/// wire, is the JSON value `equals`.
pub(crate) fn equal_at(sent: &serde_json::Value, path: &str, equals: &str) -> bool {
    let marker = serde_json::from_str::<serde_json::Value>(equals).ok();
    crate::provider::at_path(sent, path).is_some_and(|found| Some(found) == marker.as_ref())
}

/// Whether a value says something: anything but `none`, blank text or
/// `false`.
fn holds(found: &Value) -> bool {
    match found {
        Value::None => false,
        Value::Text(text) => !text.trim().is_empty(),
        Value::Bool(flag) => *flag,
        _ => true,
    }
}

/// The text at `path`, by field names, `0` for a list's first item and
/// [`jadpo_std::ai::EACH`] for every item's, their texts joined in order
/// and the items with none left out; `None` where there is no text.
fn text_at(value: &Value, path: &[String]) -> Option<String> {
    let Some((step, rest)) = path.split_first() else {
        return match value {
            Value::Text(text) => Some(text.to_string()),
            _ => None,
        };
    };
    match value {
        Value::List(items) if step == jadpo_std::ai::EACH => {
            let texts: Vec<String> = items
                .iter()
                .filter_map(|item| text_at(item, rest))
                .collect();
            (!texts.is_empty()).then(|| texts.concat())
        }
        Value::Record(record) => text_at(record.get(step)?, rest),
        Value::List(items) => text_at(items.get(step.parse::<usize>().ok()?)?, rest),
        _ => None,
    }
}

/// The value at `path`, by field names, `0` for a list's first item.
fn value_at<'v>(value: &'v Value, path: &[String]) -> Option<&'v Value> {
    path.iter().try_fold(value, |at, step| match at {
        Value::Record(record) => record.get(step),
        Value::List(items) => items.get(step.parse::<usize>().ok()?),
        _ => None,
    })
}

/// The count of tokens at `path`, by field names: a whole number or a
/// `TokenCount`.
pub(crate) fn count_at(value: &Value, path: &[String]) -> Option<u64> {
    match value_at(value, path)? {
        Value::Int(n) => u64::try_from(*n).ok(),
        other => match jadpo_value::standard::from(other) {
            Ok(Value::Int(n)) => u64::try_from(n).ok(),
            _ => None,
        },
    }
}

/// A condition, for the call's `attempt` to map.
fn broke(rule: RuleKey) -> Done {
    Done::Broke {
        rule,
        problem: None,
    }
}

/// What the call gives when its answer is given back without asking
/// again: logged as replayed, then decoded as any answer is.
fn replayed(
    ctx: &mut Ctx<'_>,
    ai: &Ai<'_>,
    site: &GenerateSite,
    asked: &Asked,
    made: Call,
    kept: Kept,
) -> Done {
    crate::outside::given_back(ctx, &made, &kept.logged());
    match kept {
        Kept::Answer(text) => match decode(ai, site, asked, &text) {
            Ok(value) => Done::Value(value),
            // What decoded when it came decodes no more under this build.
            Err(problem) => Done::Broke {
                rule: RuleKey::Malformed,
                problem: Some(crate::effects::decode_problem(&problem)),
            },
        },
        Kept::Condition(rule) => broke(rule),
        Kept::Malformed(problem) => Done::Broke {
            rule: RuleKey::Malformed,
            problem: Some(crate::effects::decode_problem(&problem)),
        },
    }
}

/// What a call asks for besides its messages.
struct Asked {
    /// The model's text, `openai/gpt-4.1-mini`.
    model: String,
    /// `T`'s schema, whole; `None` for plain text.
    schema: Option<serde_json::Value>,
    mode: SchemaMode,
    /// Whether `T` is asked for as the member `value` of an object.
    wrapped: bool,
    /// The shape the mode's envelope is sent in (chunk 5.5v7).
    envelope: AiEnvelope,
}

/// The code an OpenAI-compatible server answers a prompt longer than the
/// model's context with (a `400`).
pub const CONTEXT_LENGTH_EXCEEDED: &str = "context_length_exceeded";

/// Each message's role and text.
pub(crate) fn messages_of(rendered: &Value) -> Option<Vec<(String, String)>> {
    let Value::List(items) = rendered else {
        return None;
    };
    items
        .iter()
        .map(|item| {
            let Value::Record(record) = item else {
                return None;
            };
            let role = match record.get("role")? {
                Value::Variant(variant) => variant.name.to_string(),
                _ => return None,
            };
            let content = match record.get("content")? {
                Value::Text(text) => text.to_string(),
                _ => return None,
            };
            Some((role, content))
        })
        .collect()
}

/// The messages with `schema` added to the instructions under a fixed
/// heading, asking for JSON alone: what a provider with no structured mode,
/// or one that promises only JSON, is sent (area 21, "Typed output", step
/// 2).
pub fn with_schema(
    mut messages: Vec<(String, String)>,
    schema: &serde_json::Value,
) -> Vec<(String, String)> {
    let added = format!(
        "\n\n{}\n\nAnswer with one JSON document alone, matching this JSON Schema:\n\n{schema}",
        jadpo_std::ai::SCHEMA_HEADING
    );
    match messages.iter_mut().find(|(role, _)| role == "system") {
        Some((_, content)) => content.push_str(&added),
        None => messages.insert(0, ("system".to_string(), added.trim_start().to_string())),
    }
    messages
}

/// The correction a repair is asked with: what didn't decode, by its path
/// and rule, never a value (area 21, "Typed output", step 4).
fn correction(problem: &Problem, asked: &Asked) -> String {
    let what = if problem.rule == RuleKey::Malformed.word() {
        if asked.schema.is_some() {
            "Your answer wasn't one JSON document.".to_string()
        } else {
            "Your answer couldn't be read.".to_string()
        }
    } else if problem.field.is_empty() {
        format!("Your answer breaks the rule `{}`.", problem.rule)
    } else {
        format!(
            "In your answer, `{}` breaks the rule `{}`.",
            problem.field, problem.rule
        )
    };
    let again = if asked.schema.is_some() {
        "Answer again with one JSON document alone that matches the schema."
    } else {
        "Answer again."
    };
    format!("{what} {again}")
}

/// The request's JSON body: OpenAI's Chat Completions request, the
/// schema in its structured-output mode where the provider has one.
fn request_body(asked: &Asked, messages: &[(String, String)], site: &GenerateSite) -> String {
    let mut body = serde_json::Map::new();
    body.insert("model".into(), asked.model.as_str().into());
    body.insert(
        "messages".into(),
        serde_json::Value::Array(
            messages
                .iter()
                .map(|(role, content)| {
                    let mut message = serde_json::Map::new();
                    message.insert("role".into(), role.as_str().into());
                    message.insert("content".into(), content.as_str().into());
                    serde_json::Value::Object(message)
                })
                .collect(),
        ),
    );
    body.insert("max_tokens".into(), site.max_output.into());
    if let Some(format) = envelope(asked, site) {
        body.insert("response_format".into(), format);
    }
    serde_json::Value::Object(body).to_string()
}

/// What the structured-output mode is asked with, OpenAI's form unless
/// the package's envelope is Anthropic's (`{"type": "json_schema",
/// "schema": ...}`, chunk 5.5v7): for a
/// strict mode, `{"type": "json_schema", "json_schema": {"name": <prompt>,
/// "strict": true, "schema": ...}}` with the schema cut to what it
/// enforces; for one that promises only JSON, `{"type": "json_object"}`;
/// nothing with no mode, or for a `T` built on `Text`. The built-in driver
/// sends it as `response_format`, and a package where its `schema: { at: }`
/// says.
fn envelope(asked: &Asked, site: &GenerateSite) -> Option<serde_json::Value> {
    match (&asked.schema, asked.mode) {
        (Some(schema), SchemaMode::StrictSubset) if asked.envelope == AiEnvelope::Anthropic => {
            let cut = strict(schema);
            let cut = if asked.wrapped { wrapped(cut) } else { cut };
            let mut format = serde_json::Map::new();
            format.insert("type".into(), "json_schema".into());
            format.insert("schema".into(), cut);
            Some(serde_json::Value::Object(format))
        }
        (Some(schema), SchemaMode::StrictSubset) => {
            let cut = strict(schema);
            let cut = if asked.wrapped { wrapped(cut) } else { cut };
            let mut format = serde_json::Map::new();
            format.insert("type".into(), "json_schema".into());
            let mut named = serde_json::Map::new();
            named.insert("name".into(), site.prompt.as_str().into());
            named.insert("strict".into(), true.into());
            named.insert("schema".into(), cut);
            format.insert("json_schema".into(), serde_json::Value::Object(named));
            Some(serde_json::Value::Object(format))
        }
        (Some(_), SchemaMode::JsonOnly) => {
            let mut format = serde_json::Map::new();
            format.insert("type".into(), "json_object".into());
            Some(serde_json::Value::Object(format))
        }
        _ => None,
    }
}

/// `schema` as the member `value` of an object: what a strict structured
/// mode, which wants an object at its root, is asked for a `T` whose JSON
/// isn't one. Its definitions stay at the root.
fn wrapped(mut schema: serde_json::Value) -> serde_json::Value {
    let defs = schema
        .as_object_mut()
        .and_then(|members| members.remove("$defs"));
    let mut properties = serde_json::Map::new();
    properties.insert(jadpo_std::ai::WRAPPED.into(), schema);
    let mut out = serde_json::Map::new();
    out.insert("type".into(), "object".into());
    out.insert("properties".into(), serde_json::Value::Object(properties));
    out.insert(
        "required".into(),
        serde_json::Value::Array(vec![jadpo_std::ai::WRAPPED.into()]),
    );
    out.insert("additionalProperties".into(), false.into());
    if let Some(defs) = defs {
        out.insert("$defs".into(), defs);
    }
    serde_json::Value::Object(out)
}

/// The keywords a strict structured mode enforces (area 21, "Typed
/// output", step 2: types, required fields, enums, nesting): OpenAI's
/// structured outputs' subset. Lengths, patterns, bounds and formats are
/// left out and checked by decoding.
const KEPT: [&str; 11] = [
    "type",
    "properties",
    "required",
    "items",
    "enum",
    "const",
    "anyOf",
    "$ref",
    "$defs",
    "description",
    "additionalProperties",
];

/// `schema` cut to what a strict structured mode enforces: every object's
/// properties all required and no others allowed, one that may be left
/// out made one that may be `null`; a choice of shapes as `anyOf`.
pub fn strict(schema: &serde_json::Value) -> serde_json::Value {
    use serde_json::Value as J;
    match schema {
        J::Object(members) => {
            let mut out = serde_json::Map::new();
            for (key, value) in members {
                let key = if key == "oneOf" {
                    "anyOf"
                } else {
                    key.as_str()
                };
                if !KEPT.contains(&key) {
                    continue;
                }
                let value = match key {
                    // Names, not schemas.
                    "required" | "enum" | "const" | "type" | "description" | "$ref" => {
                        value.clone()
                    }
                    "properties" | "$defs" => match value {
                        J::Object(entries) => J::Object(
                            entries
                                .iter()
                                .map(|(name, schema)| (name.clone(), strict(schema)))
                                .collect(),
                        ),
                        other => other.clone(),
                    },
                    "additionalProperties" => match value {
                        J::Bool(flag) => J::Bool(*flag),
                        schema => strict(schema),
                    },
                    _ => strict(value),
                };
                out.insert(key.to_string(), value);
            }
            // Every property required, as the mode wants: one that may be
            // left out may be `null` instead.
            if let Some(J::Object(properties)) = out.get_mut("properties") {
                let required: Vec<String> = members
                    .get("required")
                    .and_then(J::as_array)
                    .map(|names| {
                        names
                            .iter()
                            .filter_map(|name| name.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default();
                let mut all = Vec::new();
                for (name, property) in properties.iter_mut() {
                    if !required.contains(name) {
                        let schema = std::mem::take(property);
                        let mut null = serde_json::Map::new();
                        null.insert("type".into(), "null".into());
                        let mut either = serde_json::Map::new();
                        either.insert("anyOf".into(), J::Array(vec![schema, J::Object(null)]));
                        *property = J::Object(either);
                    }
                    all.push(J::String(name.clone()));
                }
                out.insert("required".into(), J::Array(all));
                out.insert("additionalProperties".into(), J::Bool(false));
            }
            J::Object(out)
        }
        J::Array(items) => J::Array(items.iter().map(strict).collect()),
        other => other.clone(),
    }
}

/// What a chat completion says.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Reply {
    pub(crate) content: Option<String>,
    /// The model declined: a refusal it gave, or its answer stopped by the
    /// provider's filter.
    pub(crate) refused: bool,
    /// Its answer was cut off at the most tokens it was allowed.
    pub(crate) cut_off: bool,
    /// The tokens the provider says it used, when it says (chunk 5.5v5).
    pub(crate) usage: Option<crate::ai_budget::Used>,
}

/// A chat completion's first choice, as OpenAI's API answers one; `None`
/// for anything else.
pub(crate) fn reply_of(text: &str) -> Option<Reply> {
    let json: serde_json::Value = serde_json::from_str(text).ok()?;
    let choice = json.get("choices")?.as_array()?.first()?;
    let message = choice.get("message")?;
    let finish = choice
        .get("finish_reason")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let refusal = message
        .get("refusal")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|refusal| !refusal.trim().is_empty());
    Some(Reply {
        content: message
            .get("content")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
        refused: refusal || finish == "content_filter",
        cut_off: finish == "length",
        usage: json.get("usage").and_then(|usage| {
            let count = |at: &[&str]| {
                at.iter()
                    .try_fold(usage, |at, step| at.get(step))
                    .and_then(serde_json::Value::as_u64)
            };
            Some(crate::ai_budget::Used {
                input: count(&["prompt_tokens"])?,
                output: count(&["completion_tokens"])?,
                cached: count(&["prompt_tokens_details", "cached_tokens"]).unwrap_or(0),
            })
        }),
    })
}

/// A chat completion answering with `content`, or declining, or cut off:
/// how a fake answers, in the built-in driver's own form.
pub fn completion(content: Option<&str>, refusal: Option<&str>, finish: &str) -> Rc<str> {
    let mut message = serde_json::Map::new();
    message.insert("role".into(), "assistant".into());
    message.insert(
        "content".into(),
        content.map_or(serde_json::Value::Null, serde_json::Value::from),
    );
    if let Some(refusal) = refusal {
        message.insert("refusal".into(), refusal.into());
    }
    let mut choice = serde_json::Map::new();
    choice.insert("index".into(), 0.into());
    choice.insert("message".into(), serde_json::Value::Object(message));
    choice.insert("finish_reason".into(), finish.into());
    let mut out = serde_json::Map::new();
    out.insert("object".into(), "chat.completion".into());
    out.insert(
        "choices".into(),
        serde_json::Value::Array(vec![serde_json::Value::Object(choice)]),
    );
    Rc::from(serde_json::Value::Object(out).to_string())
}

/// The answer `content` holds as the call's type, read as `json.read`
/// reads a document, every rule included; the first problem otherwise. A
/// `T` built on `Text` is the text as it is.
fn decode(
    ai: &Ai<'_>,
    site: &GenerateSite,
    asked: &Asked,
    content: &str,
) -> Result<Value, Problem> {
    let Some(codec) = ai.json.codec_of(site.answer) else {
        return Err(Problem {
            field: String::new(),
            rule: "type".to_string(),
        });
    };
    let codecs = ai.json.codecs;
    let malformed = || Problem {
        field: String::new(),
        rule: RuleKey::Malformed.word().to_string(),
    };
    let unread = |unread: Unread| match unread {
        Unread::Malformed => malformed(),
        Unread::Mismatch(problem) => problem,
    };
    if asked.schema.is_none() {
        let text = serde_json::Value::String(content.to_string()).to_string();
        return codecs
            .read_value(codec, &Value::text(&text))
            .map_err(unread);
    }
    let document = strip_fence(content);
    if asked.wrapped {
        return codecs
            .read_member(codec, document, jadpo_std::ai::WRAPPED)
            .map_err(unread);
    }
    codecs
        .read_value(codec, &Value::text(document))
        .map_err(unread)
}

/// A document a model wrapped in a Markdown code fence, without it: what
/// a model asked for JSON alone in its instructions often sends.
fn strip_fence(content: &str) -> &str {
    let trimmed = content.trim();
    let Some(rest) = trimmed.strip_prefix("```") else {
        return content;
    };
    let rest = rest.strip_prefix("json").unwrap_or(rest);
    match rest.trim_start_matches(['\r', '\n']).strip_suffix("```") {
        Some(inner) => inner.trim(),
        None => content,
    }
}

/// A prompt's values as a development log keeps them, which `jadpo ai
/// record` reads (chunk 5.5v4b): none without the `testing` feature,
/// whose recordings they are kept for.
fn prompt_values(ai: &Ai<'_>, site: &GenerateSite, prompt: &Value) -> Option<String> {
    #[cfg(feature = "testing")]
    return crate::prompt_recordings::values_written(ai, site, prompt);
    #[cfg(not(feature = "testing"))]
    {
        let _ = (ai, site, prompt);
        None
    }
}

/// A fake's generated default, as a chat completion: a value of the
/// call's type that keeps its rules, made as a provider's generated
/// default is (area 13; chunk 7.2d), and wrapped as the call asks.
#[cfg(feature = "testing")]
fn generated_answer(ctx: &mut Ctx<'_>, ai: &Ai<'_>, site: &GenerateSite, asked: &Asked) -> Rc<str> {
    let Some(codec) = ai.json.codec_of(site.answer) else {
        return completion(None, None, "stop");
    };
    let now = ctx.now();
    let mut uuid = || crate::fakes::uuid_text(ctx.new_id().unwrap_or_default());
    let mut making = crate::fakes::Making {
        now,
        uuid: &mut uuid,
    };
    let json = crate::fakes::generated(&ai.json.codecs, codec, &mut making);
    let content = match (&asked.schema, json) {
        (None, serde_json::Value::String(text)) => text,
        (None, other) => other.to_string(),
        (Some(_), json) if asked.wrapped => {
            let mut members = serde_json::Map::new();
            members.insert(jadpo_std::ai::WRAPPED.into(), json);
            serde_json::Value::Object(members).to_string()
        }
        (Some(_), json) => json.to_string(),
    };
    completion(Some(&content), None, "stop")
}

/// Keeps one attempt in `jadpo dev`'s call log, its request and answer in
/// full (area 21, "Prompts and answers in logs": in development, in
/// full).
pub(crate) fn log(ctx: &Ctx<'_>, call: &Call, values: Option<&str>, answer: &Answer) {
    let (Some(log), Some(logged)) = (ctx.host().call_log(), ctx.capabilities().calls().last())
    else {
        return;
    };
    let (url, body) = call.request.as_deref().map_or(("", ""), |request| {
        (
            request.url.as_str(),
            request.body.as_deref().unwrap_or_default(),
        )
    });
    let sent = match answer {
        Answer::Wire(text) => Some(text.as_ref()),
        _ => None,
    };
    log.seen(&crate::call_log::Seen {
        connection: &call.connection,
        operation: &call.operation,
        request_id: ctx.request_id(),
        at: logged.at,
        source: logged.source,
        took_ms: logged.took_ms,
        method: HttpMethod::Post.word(),
        url,
        body: Some(body),
        answer: sent,
        ended: &logged.ended,
        values,
    });
}

// --- the fake -------------------------------------------------------------------

#[cfg(feature = "testing")]
mod fake;
#[cfg(feature = "testing")]
pub use fake::{AiFake, Script, as_sent, requests, script, scripted};

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    use jadpo_std::http::{Codec, CodecId};
    use jadpo_std::prompt::{Piece, PromptEntry};
    use jadpo_std::types::ValidatorId;

    use crate::capability::Faked;
    use crate::{
        ActionBody, Clock, Database, Env, Host, Isolation, Outcome, Places, SqlValue, StoreError,
        Transaction,
    };

    /// A store whose first `changes` commits find a concurrent change.
    struct Changing(Rc<Cell<u32>>);

    struct Tx(Rc<Cell<u32>>);

    impl Transaction for Tx {
        fn query<'a>(
            &'a mut self,
            _sql: &'a str,
            _params: &'a [SqlValue],
        ) -> LocalFuture<'a, Result<Vec<Vec<SqlValue>>, StoreError>> {
            Box::pin(async { Ok(Vec::new()) })
        }

        fn execute<'a>(
            &'a mut self,
            _sql: &'a str,
            _params: &'a [SqlValue],
        ) -> LocalFuture<'a, Result<u64, StoreError>> {
            Box::pin(async { Ok(0) })
        }

        fn batch<'a>(&'a mut self, _sql: &'a str) -> LocalFuture<'a, Result<(), StoreError>> {
            Box::pin(async { Ok(()) })
        }

        fn commit(self: Box<Self>) -> LocalFuture<'static, Result<(), StoreError>> {
            let changes = Rc::clone(&self.0);
            Box::pin(async move {
                if changes.get() > 0 {
                    changes.set(changes.get() - 1);
                    return Err(StoreError::ConcurrentChange);
                }
                Ok(())
            })
        }

        fn rollback(self: Box<Self>) -> LocalFuture<'static, Result<(), StoreError>> {
            Box::pin(async { Ok(()) })
        }
    }

    impl Database for Changing {
        fn begin_at(
            &self,
            _isolation: Isolation,
        ) -> LocalFuture<'_, Result<Box<dyn Transaction>, StoreError>> {
            let changes = Rc::clone(&self.0);
            Box::pin(async move { Ok(Box::new(Tx(changes)) as Box<dyn Transaction>) })
        }
    }

    impl Host for Changing {
        fn database(&self, _store: &str) -> Option<&dyn Database> {
            Some(self)
        }

        fn random(&self, bytes: &mut [u8]) -> bool {
            bytes.fill(7);
            true
        }
    }

    /// An action that asks the model, then writes.
    struct Asks<'a> {
        ai: Ai<'a>,
        site: &'a GenerateSite,
        answers: Vec<Value>,
    }

    impl<'p> ActionBody<'p> for Asks<'_> {
        fn isolation(&self) -> Isolation {
            Isolation::ReadCommitted
        }

        fn run<'c>(&'c mut self, ctx: &'c mut Ctx<'p>) -> LocalFuture<'c, Outcome> {
            Box::pin(async move {
                let model =
                    jadpo_value::standard::parse("local/llama3.2", "ModelId").expect("a model");
                let prompt = Value::record("AnswerQuestion", Vec::new());
                match generated(ctx, &self.ai, self.site, prompt, &[(0, model)]).await {
                    Done::Value(value) => self.answers.push(value),
                    other => panic!("{other:?}"),
                }
                if ctx.transaction("main").await.is_err() {
                    return Outcome::Value(Value::Unit);
                }
                Outcome::Value(Value::Unit)
            })
        }
    }

    /// An action run again after a concurrent change (area 6) is given
    /// the model's answer from its first run without asking again, so the
    /// rerun neither spends twice nor answers differently.
    #[test]
    fn a_rerun_is_given_the_answer_its_first_run_had() {
        let plan = AiPlan {
            connection: "local_model".into(),
            driver: AiDriver::OpenAiCompatible(AiUrl::Written("http://localhost:11434/v1".into())),
            models: vec![("default".into(), 0)],
            schema: SchemaMode::StrictSubset,
            limits: None,
            prices: Vec::new(),
        };
        let prompts = PromptPlan {
            entries: vec![PromptEntry {
                key: "answer_question".into(),
                type_name: "AnswerQuestion".into(),
                digest: "00000000".into(),
                instructions: vec![Piece::Text("Answer.".into())],
                request: vec![Piece::Text("Is it rye?".into())],
                fields: Vec::new(),
                conversation: None,
            }],
        };
        let codecs = [Codec::Scalar {
            builtin: jadpo_std::Builtin::Text,
            validator: ValidatorId(0),
            int_as_number: false,
        }];
        let types = [CodecId(0)];
        let ai = Ai {
            plan: &plan,
            prompts: &prompts,
            json: JsonCodecs {
                codecs: jadpo_value::json::Codecs {
                    codecs: &codecs,
                    validators: &[],
                    shapes: &[],
                    coder: None,
                },
                types: &types,
            },
            providers: None,
        };
        let site = GenerateSite {
            prompt: "answer_question".into(),
            entry: 0,
            fields: Vec::new(),
            answer: 0,
            schema: None,
            object: false,
            model: "default".into(),
            max_output: 100,
            timeout_ms: 1_000,
            attempts: 2,
        };
        let fake = Rc::new(AiFake::new());
        fake.script(
            "local_model",
            "answer_question",
            scripted(Script::Content("Yes, rye.".into())),
            Frame {
                callable: 0,
                site: Frame::ENTRY,
            },
        );
        let host = Faked::new(Changing(Rc::new(Cell::new(1))))
            .with("local_model", Rc::clone(&fake) as Rc<dyn Capability>);
        let places = Places::new();
        let mut ctx = Ctx::new(
            &places,
            &host,
            Env {
                seed: Some(1),
                clock: Clock::TEST,
            },
        );
        let mut body = Asks {
            ai,
            site: &site,
            answers: Vec::new(),
        };
        let outcome = crate::block_on(crate::action(&mut ctx, &mut body)).expect("nothing waits");
        assert!(matches!(outcome, Outcome::Value(_)), "{outcome:?}");
        assert_eq!(
            body.answers,
            [Value::text("Yes, rye."), Value::text("Yes, rye.")]
        );
        assert_eq!(fake.usage()[0].called, 1, "asked once, on the first run");
        let sources: Vec<Source> = ctx
            .capabilities()
            .calls()
            .iter()
            .map(|logged| logged.source)
            .collect();
        assert_eq!(sources, [Source::Scripted, Source::Replayed]);
        assert_eq!(
            fake.requests("local_model", "answer_question").len(),
            1,
            "one request"
        );
    }

    /// A store that keeps each statement run on its own with its
    /// parameters, and takes every reservation.
    #[derive(Default)]
    struct Metered(RefCell<Vec<(String, Vec<SqlValue>)>>);

    impl Database for Metered {
        fn begin_at(
            &self,
            _isolation: Isolation,
        ) -> LocalFuture<'_, Result<Box<dyn Transaction>, StoreError>> {
            Box::pin(async { Err(StoreError::Unavailable) })
        }

        fn query_once<'a>(
            &'a self,
            sql: &'a str,
            params: &'a [SqlValue],
        ) -> LocalFuture<'a, Result<Vec<crate::Row>, StoreError>> {
            self.0.borrow_mut().push((sql.to_string(), params.to_vec()));
            Box::pin(async { Ok(vec![vec![SqlValue::Int4(1)]]) })
        }
    }

    impl Host for Metered {
        fn database(&self, _store: &str) -> Option<&dyn Database> {
            Some(self)
        }

        fn random(&self, bytes: &mut [u8]) -> bool {
            bytes.fill(7);
            true
        }
    }

    fn tokens(params: &[SqlValue], at: usize) -> i64 {
        match params.get(at) {
            Some(SqlValue::Int8(n)) => *n,
            other => panic!("not a count: {other:?}"),
        }
    }

    /// A send the provider couldn't answer is sent again, and each send is
    /// charged what it sent; the second is taken from the day's limit
    /// before it goes, so concurrent calls can't pass it between them
    /// (area 21, "Budgets"; Phase 7 review, chunk 7.9c).
    #[test]
    fn every_send_is_taken_from_the_limits_and_charged() {
        let plan = AiPlan {
            connection: "local_model".into(),
            driver: AiDriver::OpenAiCompatible(AiUrl::Written("http://localhost:11434/v1".into())),
            models: vec![("default".into(), 0)],
            schema: SchemaMode::StrictSubset,
            limits: Some(jadpo_std::ai::AiLimits {
                store: "main".into(),
                per_request: None,
                per_day: 1_000_000,
                cost: None,
                per_caller_per_day: None,
            }),
            prices: Vec::new(),
        };
        let prompts = PromptPlan {
            entries: vec![PromptEntry {
                key: "answer_question".into(),
                type_name: "AnswerQuestion".into(),
                digest: "00000000".into(),
                instructions: vec![Piece::Text("Answer the question about the bread.".into())],
                request: vec![Piece::Text("Is it rye?".into())],
                fields: Vec::new(),
                conversation: None,
            }],
        };
        let codecs = [Codec::Scalar {
            builtin: jadpo_std::Builtin::Text,
            validator: ValidatorId(0),
            int_as_number: false,
        }];
        let types = [CodecId(0)];
        let ai = Ai {
            plan: &plan,
            prompts: &prompts,
            json: JsonCodecs {
                codecs: jadpo_value::json::Codecs {
                    codecs: &codecs,
                    validators: &[],
                    shapes: &[],
                    coder: None,
                },
                types: &types,
            },
            providers: None,
        };
        let site = GenerateSite {
            prompt: "answer_question".into(),
            entry: 0,
            fields: Vec::new(),
            answer: 0,
            schema: None,
            object: false,
            model: "default".into(),
            max_output: 100,
            timeout_ms: 1_000,
            attempts: 1,
        };
        let fake = Rc::new(AiFake::new());
        let frame = Frame {
            callable: 0,
            site: Frame::ENTRY,
        };
        fake.script("local_model", "answer_question", Answer::Unavailable, frame);
        fake.script(
            "local_model",
            "answer_question",
            scripted(Script::Content("Yes, rye.".into())),
            frame,
        );
        let host = Faked::new(Metered::default())
            .with("local_model", Rc::clone(&fake) as Rc<dyn Capability>);
        let places = Places::new();
        let mut ctx = Ctx::new(
            &places,
            &host,
            Env {
                seed: Some(1),
                clock: Clock::TEST,
            },
        );
        let model = jadpo_value::standard::parse("local/llama3.2", "ModelId").expect("a model");
        let prompt = Value::record("AnswerQuestion", Vec::new());
        let done = crate::block_on(generated(&mut ctx, &ai, &site, prompt, &[(0, model)]))
            .expect("nothing waits");
        assert!(matches!(done, Done::Value(_)), "{done:?}");
        assert_eq!(fake.requests("local_model", "answer_question").len(), 2);
        let statements = host.host().0.borrow();
        let taken: Vec<i64> = statements
            .iter()
            .filter(|(sql, _)| sql == crate::ai_budget::TAKE_APP)
            .map(|(_, params)| tokens(params, 3))
            .collect();
        let [first, again] = taken[..] else {
            panic!("taken {taken:?}");
        };
        // The first send's prompt and its answer; then the prompt again.
        assert_eq!(first, again + 100);
        let (_, charged) = statements
            .iter()
            .find(|(sql, params)| {
                sql == crate::ai_budget::DEBIT
                    && params.get(1) == Some(&SqlValue::Text("app".into()))
            })
            .expect("charged");
        assert_eq!(tokens(charged, 3), 2 * again, "both sends' input");
        assert_eq!(tokens(charged, 6), first + again, "all it took given back");
    }

    fn object_schema() -> serde_json::Value {
        serde_json::json!({
            "$ref": "#/$defs/SupportAnswer",
            "$defs": {
                "SupportAnswer": {
                    "type": "object",
                    "description": "What the assistant says back.",
                    "properties": {
                        "reply": {"type": "string", "maxLength": 2000, "description": "The reply."},
                        "cited": {"type": "array", "items": {"type": "string", "pattern": "^a$"}, "maxItems": 5},
                        "note": {"anyOf": [{"type": "string"}, {"type": "null"}]}
                    },
                    "required": ["reply", "cited"],
                    "additionalProperties": false
                },
                "Mood": {"oneOf": [{"type": "string", "const": "calm"}, {"type": "object", "properties": {"tag": {"const": "x"}}, "required": ["tag"], "additionalProperties": false}]}
            }
        })
    }

    /// A strict structured mode is sent the types, required fields, enums
    /// and nesting, with every property required and none other allowed;
    /// lengths, patterns and counts are left for decoding.
    #[test]
    fn a_strict_mode_is_sent_what_it_enforces() {
        let cut = strict(&object_schema());
        let answer = &cut["$defs"]["SupportAnswer"];
        assert_eq!(
            answer["required"],
            serde_json::json!(["cited", "note", "reply"])
        );
        assert_eq!(answer["additionalProperties"], serde_json::json!(false));
        assert_eq!(
            answer["properties"]["reply"],
            serde_json::json!({"type": "string", "description": "The reply."})
        );
        assert_eq!(
            answer["properties"]["cited"],
            serde_json::json!({"type": "array", "items": {"type": "string"}})
        );
        // One the type may leave out may be `null`.
        assert_eq!(
            answer["properties"]["note"],
            serde_json::json!({"anyOf": [{"anyOf": [{"type": "string"}, {"type": "null"}]}, {"type": "null"}]})
        );
        assert!(cut["$defs"]["Mood"].get("oneOf").is_none());
        assert!(cut["$defs"]["Mood"].get("anyOf").is_some());
        assert_eq!(cut["$ref"], serde_json::json!("#/$defs/SupportAnswer"));
    }

    /// A type whose JSON isn't an object is asked for as the member
    /// `value` of one, its definitions at the root.
    #[test]
    fn a_type_that_isnt_an_object_is_wrapped() {
        let schema = serde_json::json!({"$ref": "#/$defs/Mood", "$defs": {"Mood": {"type": "string", "enum": ["calm", "cross"]}}});
        let out = wrapped(strict(&schema));
        assert_eq!(
            out["properties"]["value"],
            serde_json::json!({"$ref": "#/$defs/Mood"})
        );
        assert_eq!(out["required"], serde_json::json!(["value"]));
        assert_eq!(
            out["$defs"]["Mood"]["enum"],
            serde_json::json!(["calm", "cross"])
        );
    }

    /// Where the provider has no structured mode the schema goes in the
    /// instructions under a fixed heading, asking for JSON alone.
    #[test]
    fn without_a_structured_mode_the_schema_is_in_the_instructions() {
        let messages = vec![
            ("system".to_string(), "Answer the question.".to_string()),
            (
                "user".to_string(),
                "<question>\nhi\n</question>".to_string(),
            ),
        ];
        let out = with_schema(messages, &serde_json::json!({"type": "string"}));
        assert_eq!(
            out[0].1,
            "Answer the question.\n\n# Answer format\n\nAnswer with one JSON document alone, matching this JSON Schema:\n\n{\"type\":\"string\"}"
        );
        assert_eq!(out[1].1, "<question>\nhi\n</question>");
    }

    /// A condition a record keeps reads back as it was, `malformed` with
    /// its problem's path and rule.
    #[test]
    fn a_kept_condition_reads_back_as_it_was_written() {
        for kept in [
            Kept::Condition(RuleKey::Refused),
            Kept::Condition(RuleKey::TooLong),
            Kept::Malformed(Problem {
                field: "cited.2".into(),
                rule: "max_length".into(),
            }),
            Kept::Malformed(Problem {
                field: String::new(),
                rule: "malformed".into(),
            }),
        ] {
            let word = kept.word().expect("a condition");
            assert_eq!(Kept::from_word(&word), Some(kept), "{word}");
        }
        assert_eq!(Kept::from_word("refused extra"), None);
        assert_eq!(Kept::Answer(Rc::from("{}")).word(), None);
    }

    /// A repair names the problem's path and rule, never a value.
    #[test]
    fn a_correction_names_the_path_and_the_rule() {
        let asked = Asked {
            model: "local/llama".into(),
            schema: Some(serde_json::json!({})),
            mode: SchemaMode::StrictSubset,
            wrapped: false,
            envelope: AiEnvelope::OpenAi,
        };
        let problem = Problem {
            field: "cited.2".into(),
            rule: "max_length".into(),
        };
        assert_eq!(
            correction(&problem, &asked),
            "In your answer, `cited.2` breaks the rule `max_length`. Answer again with one JSON document alone that matches the schema."
        );
        let malformed = Problem {
            field: String::new(),
            rule: "malformed".into(),
        };
        assert_eq!(
            correction(&malformed, &asked),
            "Your answer wasn't one JSON document. Answer again with one JSON document alone that matches the schema."
        );
    }

    /// A chat completion is read for its text, a refusal and a cut-off;
    /// a fake answers in the same form.
    #[test]
    fn a_chat_completion_is_read_as_the_driver_reads_one() {
        let said = reply_of(&completion(Some("{}"), None, "stop")).expect("a completion");
        assert_eq!(said.content.as_deref(), Some("{}"));
        assert!(!said.refused && !said.cut_off);
        let declined = reply_of(&completion(None, Some("No."), "stop")).expect("a completion");
        assert!(declined.refused);
        let cut = reply_of(&completion(Some("{\"re"), None, "length")).expect("a completion");
        assert!(cut.cut_off);
        let filtered =
            reply_of(&completion(Some(""), None, "content_filter")).expect("a completion");
        assert!(filtered.refused);
        assert_eq!(reply_of("{\"error\": {}}"), None);
        assert_eq!(strip_fence("```json\n{\"a\": 1}\n```"), "{\"a\": 1}");
        assert_eq!(strip_fence("{\"a\": 1}"), "{\"a\": 1}");
    }
}
