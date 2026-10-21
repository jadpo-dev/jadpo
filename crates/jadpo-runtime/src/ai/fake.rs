//! The `ai` fake (area 21, "Tests"), which only a build with the
//! `testing` feature holds (chunk 8.1e).

use super::*;
use crate::fakes::Usage;

/// One answer a test's `fakes: { ai.<prompt>: }` scripts (area 21,
/// "Tests").
#[derive(Clone, Debug, PartialEq)]
pub enum Script {
    /// The model's text: a value of the call's type written as JSON, or
    /// `raw(...)`'s text.
    Content(String),
    /// One of `ai.generate`'s five conditions.
    Condition(RuleKey),
    /// `partial("...", <condition>)`: a stream that sends the text, then
    /// stops with the condition (chunk 5.5v6).
    Partial(String, RuleKey),
}

/// A value of `site`'s type, written as `json`, as the model sends it:
/// a text answer as its text, one in a strict mode that isn't an object as
/// the member `value` of one, and anything else as it is.
pub fn as_sent(ai: &Ai<'_>, site: &GenerateSite, json: &str) -> String {
    if site.schema.is_none() {
        return match serde_json::from_str::<serde_json::Value>(json) {
            Ok(serde_json::Value::String(text)) => text,
            _ => json.to_string(),
        };
    }
    if ai.plan.schema == SchemaMode::StrictSubset && !site.object {
        return format!("{{\"{}\":{json}}}", jadpo_std::ai::WRAPPED);
    }
    json.to_string()
}

/// The answer a scripted one is, as a chat completion or as the driver
/// classifies what an OpenAI-compatible server answers.
pub fn scripted(script: Script) -> Answer {
    match script {
        Script::Content(text) => Answer::Wire(completion(Some(&text), None, "stop")),
        Script::Condition(RuleKey::Refused) => Answer::Wire(completion(
            None,
            Some("The model declined to answer."),
            "stop",
        )),
        // An answer with no text, which no type decodes from.
        Script::Condition(RuleKey::Malformed) => Answer::Wire(completion(None, None, "stop")),
        Script::Condition(RuleKey::OverBudget) => {
            Answer::Wire(completion(Some(""), None, "length"))
        }
        Script::Condition(RuleKey::TooLong) => Answer::Failed(CONTEXT_LENGTH_EXCEEDED.to_string()),
        Script::Condition(_) => Answer::TimedOut,
        // A stream's text and how it stops: the provider's own finish for
        // a cut-off or a refusal, none for an answer out of time.
        Script::Partial(text, rule) => Answer::Wire(completion(
            Some(&text),
            None,
            crate::stream::finish_of(rule),
        )),
    }
}

/// One prompt's script.
#[derive(Debug)]
struct Scripts {
    prompt: String,
    from: Frame,
    answers: VecDeque<Answer>,
    scripted: usize,
    called: usize,
}

/// The `ai` fake (area 21, "Tests"): every `ai` call in `jadpo test`
/// answers through it, whatever the connection. Each attempt of a call of
/// a scripted prompt takes the prompt's next answer; any other is answered
/// with a generated value of the call's type. It keeps the prompt values
/// each call asked with, for `ai.<prompt>.requests`.
#[derive(Debug, Default)]
pub struct AiFake {
    scripts: RefCell<Vec<Scripts>>,
    /// Each call's prompt value, by its prompt, in order.
    requests: RefCell<Vec<(String, Value)>>,
}

impl AiFake {
    pub fn new() -> AiFake {
        AiFake::default()
    }

    /// How each scripted prompt was used, in the order first scripted:
    /// named `ai.<prompt>`, as the test's `fakes:` writes it.
    pub fn usage(&self) -> Vec<Usage> {
        self.scripts
            .borrow()
            .iter()
            .map(|script| Usage {
                operation: format!("{}.{}", jadpo_std::standard::ai::AI, script.prompt),
                from: script.from,
                scripted: script.scripted,
                called: script.called,
            })
            .collect()
    }
}

impl Capability for AiFake {
    fn call<'a>(&'a self, call: &'a Call) -> LocalFuture<'a, Answer> {
        if call.attempt <= 1 {
            self.requests
                .borrow_mut()
                .push((call.operation.clone(), call.input.clone()));
        }
        let answer = {
            let mut scripts = self.scripts.borrow_mut();
            match scripts
                .iter_mut()
                .find(|script| script.prompt == call.operation)
            {
                Some(script) => {
                    script.called += 1;
                    // Past its script, a generated value: the test fails
                    // as `test.fake_exhausted` once it ends.
                    script.answers.pop_front().unwrap_or(Answer::Generated)
                }
                None => Answer::Generated,
            }
        };
        Box::pin(async move { answer })
    }

    fn source(&self) -> Source {
        Source::Generated
    }

    /// A stream's answer: the next scripted one, cut into pieces of a few
    /// words, as an OpenAI-compatible server streams it (chunk 5.5v6).
    fn stream<'a>(&'a self, call: &'a Call) -> LocalFuture<'a, crate::stream::Streamed> {
        Box::pin(async move { crate::stream::faked(&call.operation, self.call(call).await) })
    }

    fn source_of(&self, call: &Call) -> Source {
        let scripted = self
            .scripts
            .borrow()
            .iter()
            .any(|script| script.prompt == call.operation && !script.answers.is_empty());
        if scripted {
            Source::Scripted
        } else {
            Source::Generated
        }
    }

    fn script(&self, _connection: &str, operation: &str, answer: Answer, from: Frame) -> bool {
        let mut scripts = self.scripts.borrow_mut();
        match scripts.iter_mut().find(|script| script.prompt == operation) {
            Some(script) => {
                script.answers.push_back(answer);
                script.scripted += 1;
            }
            None => scripts.push(Scripts {
                prompt: operation.to_string(),
                from,
                answers: VecDeque::from([answer]),
                scripted: 1,
                called: 0,
            }),
        }
        true
    }

    fn requests(&self, _connection: &str, operation: &str) -> Vec<Value> {
        self.requests
            .borrow()
            .iter()
            .filter(|(prompt, _)| prompt == operation)
            .map(|(_, value)| value.clone())
            .collect()
    }
}

/// Gives the `ai` fake the next answer to prompt `prompt` (area 21,
/// "Tests"), scripted by the test at the current frame of `ctx`.
pub fn script(
    ctx: &mut Ctx<'_>,
    ai: &Ai<'_>,
    prompt: &str,
    given: Script,
) -> Result<(), &'static str> {
    let Some(from) = ctx.frames().first().copied() else {
        return Err("a scripted answer outside a test");
    };
    let answer = scripted(given);
    let scripted = ctx
        .host()
        .capability(&ai.plan.connection)
        .is_some_and(|fake| fake.script(&ai.plan.connection, prompt, answer, from));
    if scripted {
        Ok(())
    } else {
        Err("a test's `fakes:` on a host with no `ai` fake")
    }
}

/// The prompt values the code asked prompt `prompt` with, as the `ai`
/// fake kept them (`ai.<prompt>.requests`).
pub fn requests(ctx: &Ctx<'_>, ai: &Ai<'_>, prompt: &str) -> Value {
    let sent = ctx
        .host()
        .capability(&ai.plan.connection)
        .map(|fake| fake.requests(&ai.plan.connection, prompt))
        .unwrap_or_default();
    Value::list(sent)
}
