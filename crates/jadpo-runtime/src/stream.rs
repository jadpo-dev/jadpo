//! A model's answer streamed to a route (area 21, "The `ai` capability";
//! area 7, "Streaming a model's answer"; chunk 5.5v6): what both executors
//! run at an `ai.stream(prompt)` call's site, and what the HTTP component
//! sends once the route's action has committed.
//!
//! [`stream`] renders the prompt as `ai.generate` does and asks the model
//! through the connection `ai:` names, on the capability seam, for the
//! answer as OpenAI's Chat Completions API streams it (`"stream": true`,
//! server-sent events of `chat.completion.chunk`s). It reads until the
//! first piece of text comes: before it, the call ends in one of its four
//! conditions (`refused`, `timed_out`, `over_budget`, `too_long`) for its
//! `attempt` to map, or in the runtime's `unavailable` or `misconfigured`;
//! once it has come, the open answer is kept on the invocation and the
//! call gives an `AiStream` naming it, which the action returns.
//!
//! The route then sends it ([`Events`]): each piece as a `piece` event as
//! it is read, and `done` once the answer is whole; anything that stops it
//! part-way (the model declining to go on, a cut-off at `max_output:`, the
//! call's `timeout:` or the route's deadline passing, the provider's
//! connection breaking) ends it with an `error` event, `unavailable`.
//! Dropping it, as the host does when the client goes away, stops reading
//! and closes the provider's connection.
//!
//! A test's `ai` fake answers in the same form ([`faked`]): its scripted
//! text cut into pieces of a few words ([`pieces`]), so what a test
//! scripts runs through the real reading. Prompts and answers appear in
//! no error's text, and an error event names no more than its code.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use jadpo_std::ai::{AiDriver, AiPlan, AiUrl, StreamSite, events};
use jadpo_std::subscriber::Repeat;
use jadpo_std::{HttpMethod, RuleKey};
use jadpo_value::Value;

use crate::ai::{Ai, Done};
use crate::capability::{Answer, Call, Source};
use crate::effects::Unanswered;
use crate::provider::{Reading, Request};
use crate::race::{First, race};
use crate::{Ctx, FaultKind, Host, LocalFuture};

/// How a capability answers a streamed call.
pub enum Streamed {
    /// Whole, as [`crate::capability::Capability::call`] answers: a
    /// failure before any of it came, or a driver that reads an answer
    /// only whole.
    Whole(Answer),
    /// Its body as it comes.
    Coming(Box<dyn Coming>),
}

impl std::fmt::Debug for Streamed {
    /// Never what it holds.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Streamed::Whole(answer) => f.debug_tuple("Whole").field(answer).finish(),
            Streamed::Coming(_) => f.write_str("Coming(..)"),
        }
    }
}

/// A body read as it comes.
pub trait Coming {
    /// Its next bytes, or how it ended.
    fn next(&mut self) -> LocalFuture<'_, Chunk>;
}

/// What a body gives next.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Chunk {
    Bytes(Vec<u8>),
    /// It ended, whole.
    End,
    /// No more came within its timeout.
    TimedOut,
    /// The connection broke before it ended.
    Broken,
}

/// A body already read, given back a chunk at a time.
#[derive(Debug, Default)]
pub struct Read {
    chunks: VecDeque<Chunk>,
}

impl Read {
    /// Gives `chunks`, then [`Chunk::End`] unless one of them ends it.
    pub fn new(chunks: impl IntoIterator<Item = Chunk>) -> Read {
        Read {
            chunks: chunks.into_iter().collect(),
        }
    }
}

impl Coming for Read {
    fn next(&mut self) -> LocalFuture<'_, Chunk> {
        let next = self.chunks.pop_front().unwrap_or(Chunk::End);
        Box::pin(async move { next })
    }
}

// --- reading the provider's stream ----------------------------------------

/// What the provider's stream says next.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Said {
    Piece(String),
    /// The answer is whole.
    Whole,
    /// The model declined: a refusal, or its answer stopped by the
    /// provider's filter.
    Refused,
    /// Cut off at `max_output:`.
    CutOff,
    /// No more within the time left.
    TimedOut,
    /// The connection broke, or sent what isn't a chat completion's
    /// stream.
    Broken,
}

/// Reads OpenAI's streamed chat completion: server-sent events whose data
/// is a `chat.completion.chunk`, ended by `data: [DONE]`; or a whole chat
/// completion, from a server that doesn't stream.
#[derive(Debug, Default)]
struct Reader {
    buffer: Vec<u8>,
    /// Its finish reason came, so the answer is whole once the body ends.
    finished: bool,
    /// Any event came.
    evented: bool,
    ended: bool,
}

impl Reader {
    /// What the events now complete in `buffer` say.
    fn feed(&mut self, bytes: &[u8], out: &mut VecDeque<Said>) {
        self.buffer.extend_from_slice(bytes);
        while let Some((at, gap)) = boundary(&self.buffer) {
            let event: Vec<u8> = self.buffer.drain(..at + gap).take(at).collect();
            self.event(&String::from_utf8_lossy(&event), out);
            if self.ended {
                return;
            }
        }
    }

    /// What the rest of the body says once it has ended.
    fn end(&mut self, out: &mut VecDeque<Said>) {
        if self.ended {
            return;
        }
        let rest = String::from_utf8_lossy(&std::mem::take(&mut self.buffer)).into_owned();
        if !self.evented && rest.trim_start().starts_with('{') {
            // A server that answered whole, not as a stream.
            self.ended = true;
            match crate::ai::reply_of(&rest) {
                Some(reply) if reply.refused => out.push_back(Said::Refused),
                Some(reply) => {
                    let content = reply.content.unwrap_or_default();
                    if !content.is_empty() {
                        out.push_back(Said::Piece(content));
                    }
                    out.push_back(if reply.cut_off {
                        Said::CutOff
                    } else {
                        Said::Whole
                    });
                }
                None => out.push_back(Said::Broken),
            }
            return;
        }
        if !rest.trim().is_empty() {
            self.event(&rest, out);
        }
        if !self.ended {
            self.ended = true;
            out.push_back(if self.finished {
                Said::Whole
            } else {
                Said::Broken
            });
        }
    }

    /// One event's lines.
    fn event(&mut self, text: &str, out: &mut VecDeque<Said>) {
        let data: Vec<&str> = text
            .lines()
            .filter_map(|line| line.strip_prefix("data:"))
            .map(|data| data.strip_prefix(' ').unwrap_or(data))
            .collect();
        if data.is_empty() {
            return;
        }
        self.evented = true;
        let data = data.join("\n");
        if data.trim() == "[DONE]" {
            self.ended = true;
            out.push_back(Said::Whole);
            return;
        }
        let Ok(json) = serde_json::from_str::<serde_json::Value>(&data) else {
            self.ended = true;
            out.push_back(Said::Broken);
            return;
        };
        if json.get("error").is_some() {
            self.ended = true;
            out.push_back(Said::Broken);
            return;
        }
        // A chunk with no choices carries only usage.
        let Some(choice) = json
            .get("choices")
            .and_then(serde_json::Value::as_array)
            .and_then(|choices| choices.first())
        else {
            return;
        };
        let delta = choice.get("delta");
        let text_of = |name: &str| {
            delta
                .and_then(|delta| delta.get(name))
                .and_then(serde_json::Value::as_str)
                .filter(|text| !text.is_empty())
        };
        if let Some(content) = text_of("content") {
            out.push_back(Said::Piece(content.to_string()));
        }
        if text_of("refusal").is_some_and(|refusal| !refusal.trim().is_empty()) {
            self.ended = true;
            out.push_back(Said::Refused);
            return;
        }
        match choice
            .get("finish_reason")
            .and_then(serde_json::Value::as_str)
        {
            Some("length") => {
                self.ended = true;
                out.push_back(Said::CutOff);
            }
            Some("content_filter") => {
                self.ended = true;
                out.push_back(Said::Refused);
            }
            Some(_) => self.finished = true,
            None => {}
        }
    }
}

/// Where the first whole event in `buffer` ends, and the length of the
/// blank line that ends it.
fn boundary(buffer: &[u8]) -> Option<(usize, usize)> {
    let lf = buffer
        .windows(2)
        .position(|w| w == b"\n\n")
        .map(|at| (at, 2));
    let crlf = buffer
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map(|at| (at, 4));
    match (lf, crlf) {
        (Some(a), Some(b)) => Some(if a.0 <= b.0 { a } else { b }),
        (one, other) => one.or(other),
    }
}

/// A model's answer being read: what an `AiStream` names, and what a
/// streaming route sends.
struct Reply {
    body: Box<dyn Coming>,
    reader: Reader,
    said: VecDeque<Said>,
    /// When, on the host's monotonic clock, it must have ended: the call's
    /// `timeout:` from when it was asked, or the route's deadline, the
    /// earlier.
    ends_at: Option<u64>,
    over: bool,
}

impl Reply {
    fn new(body: Box<dyn Coming>, ends_at: Option<u64>) -> Reply {
        Reply {
            body,
            reader: Reader::default(),
            said: VecDeque::new(),
            ends_at,
            over: false,
        }
    }

    /// What the stream says next, reading more as needed, within the time
    /// left on `host`'s clock.
    async fn next_said(&mut self, host: Option<&dyn Host>) -> Said {
        loop {
            if let Some(said) = self.said.pop_front() {
                return said;
            }
            if self.reader.ended {
                return Said::Broken;
            }
            let left = match (host, self.ends_at) {
                (Some(host), Some(ends)) => host.monotonic_ms().map(|now| ends.saturating_sub(now)),
                _ => None,
            };
            let chunk = match (host, left) {
                (_, Some(0)) => Chunk::TimedOut,
                (Some(host), Some(left)) => match race(self.body.next(), host.sleep(left)).await {
                    First::Left(chunk) => chunk,
                    First::Right(()) => Chunk::TimedOut,
                },
                _ => self.body.next().await,
            };
            match chunk {
                Chunk::Bytes(bytes) => self.reader.feed(&bytes, &mut self.said),
                Chunk::End => self.reader.end(&mut self.said),
                Chunk::TimedOut => {
                    self.reader.ended = true;
                    return Said::TimedOut;
                }
                Chunk::Broken => {
                    self.reader.ended = true;
                    return Said::Broken;
                }
            }
        }
    }

    /// Reads until its first piece, which stays to be sent, or until it
    /// ends without one: `None` once there is something to send, the
    /// condition the call ends in otherwise.
    async fn first(&mut self, host: &dyn Host) -> Option<Said> {
        let said = self.next_said(Some(host)).await;
        match said {
            Said::Piece(_) | Said::Whole => {
                self.said.push_front(said);
                None
            }
            other => Some(other),
        }
    }
}

/// One event a streaming route sends (area 7, "Streaming a model's
/// answer").
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ServerEvent {
    /// The next part of the answer.
    Piece(String),
    /// The answer is whole.
    Done,
    /// It stopped before it was whole, with the runtime's failure: its
    /// code and message.
    Failed {
        code: &'static str,
        message: &'static str,
    },
}

impl ServerEvent {
    /// As it is sent: `event:`, one `data:` line of compact JSON, and a
    /// blank line.
    pub fn written(&self, request_id: &str) -> Vec<u8> {
        let (name, data) = match self {
            ServerEvent::Piece(text) => {
                let mut data = String::from("{\"text\":");
                jadpo_value::json::write_string(text, &mut data);
                data.push('}');
                (events::PIECE, data)
            }
            ServerEvent::Done => (events::DONE, "{}".to_string()),
            ServerEvent::Failed { code, message } => (
                events::ERROR,
                String::from_utf8_lossy(&crate::http::envelope(code, message, request_id, None))
                    .into_owned(),
            ),
        };
        format!("event: {name}\ndata: {data}\n\n").into_bytes()
    }

    /// Whether it is the last.
    pub fn ends(&self) -> bool {
        !matches!(self, ServerEvent::Piece(_))
    }
}

/// What a streaming route sends: each event in turn, the last `done` or
/// `error`.
pub trait Events {
    /// The next event, read within the time left on `host`'s clock;
    /// `None` once the last has been given.
    fn next<'a>(&'a mut self, host: Option<&'a dyn Host>) -> LocalFuture<'a, Option<ServerEvent>>;

    /// Ends it by `at` on the host's monotonic clock, if that is earlier:
    /// the route's deadline.
    fn end_by(&mut self, at: Option<u64>);
}

impl Events for Reply {
    fn next<'a>(&'a mut self, host: Option<&'a dyn Host>) -> LocalFuture<'a, Option<ServerEvent>> {
        Box::pin(async move {
            if self.over {
                return None;
            }
            let said = self.next_said(host).await;
            let event = match said {
                Said::Piece(text) => ServerEvent::Piece(text),
                Said::Whole => ServerEvent::Done,
                Said::Refused | Said::CutOff | Said::TimedOut | Said::Broken => {
                    let (_, code, message) = crate::http::UNAVAILABLE;
                    ServerEvent::Failed { code, message }
                }
            };
            self.over = event.ends();
            Some(event)
        })
    }

    fn end_by(&mut self, at: Option<u64>) {
        self.ends_at = match (self.ends_at, at) {
            (Some(own), Some(at)) => Some(own.min(at)),
            (own, at) => own.or(at),
        };
    }
}

/// One open stream an invocation keeps, by the step its call took and
/// the request it sent, so an action run again after a concurrent change
/// is given it back rather than asking again.
pub(crate) struct OpenStream {
    pub step: Option<u32>,
    pub digest: [u8; 32],
    events: Option<Box<dyn Events>>,
}

impl OpenStream {
    pub(crate) fn open(&self) -> bool {
        self.events.is_some()
    }

    pub(crate) fn take(&mut self) -> Option<Box<dyn Events>> {
        self.events.take()
    }
}

impl std::fmt::Debug for OpenStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenStream")
            .field("step", &self.step)
            .field("open", &self.open())
            .finish_non_exhaustive()
    }
}

/// The value `ai.stream` gives: the number of the open stream the
/// invocation keeps.
fn ai_stream(number: u32) -> Value {
    Value::record(
        jadpo_std::ai::AI_STREAM,
        vec![("stream".into(), Value::Int(i64::from(number)))],
    )
}

/// The number of the open stream `value`, an `AiStream`, names.
pub fn number_of(value: &Value) -> Option<u32> {
    let Value::Record(record) = value else {
        return None;
    };
    match record.get("stream")? {
        Value::Int(number) => u32::try_from(*number).ok(),
        _ => None,
    }
}

/// A route's stream, taken from the invocation once its action has
/// committed: what the response carries for the host to send, with the
/// request's id its error event names.
#[derive(Clone)]
pub struct EventStream(Rc<Sent>);

/// What an [`EventStream`] holds.
struct Sent {
    events: RefCell<Option<Box<dyn Events>>>,
    host: RefCell<Option<Rc<dyn Host>>>,
    request_id: String,
}

impl std::fmt::Debug for EventStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventStream")
            .field("request_id", &self.0.request_id)
            .finish_non_exhaustive()
    }
}

impl PartialEq for EventStream {
    fn eq(&self, other: &EventStream) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for EventStream {}

impl EventStream {
    pub(crate) fn new(events: Box<dyn Events>, request_id: &str) -> EventStream {
        EventStream(Rc::new(Sent {
            events: RefCell::new(Some(events)),
            host: RefCell::new(None),
            request_id: request_id.to_string(),
        }))
    }

    /// Reads it on `host`, whose clock holds it to its time: what a
    /// deployable's serving gives it before the host sends it.
    pub fn attach(&self, host: Rc<dyn Host>) {
        *self.0.host.borrow_mut() = Some(host);
    }

    /// The next event on the host it was given, as sent; `None` after the
    /// last.
    pub async fn next_written(&self) -> Option<Vec<u8>> {
        let host = self.0.host.borrow().clone();
        let event = self.next_on(host.as_deref()).await?;
        Some(event.written(&self.0.request_id))
    }

    /// The next event, read on `host`; `None` after the last.
    pub async fn next_on(&self, host: Option<&dyn Host>) -> Option<ServerEvent> {
        let mut events = self.0.events.borrow_mut().take()?;
        let event = events.next(host).await;
        if event.as_ref().is_some_and(|event| !event.ends()) {
            *self.0.events.borrow_mut() = Some(events);
        }
        event
    }
}

// --- the call --------------------------------------------------------------

/// The settings a call reads, by number, for its model and its address.
pub fn settings_read(plan: &AiPlan, site: &StreamSite) -> Vec<u32> {
    let mut read: Vec<u32> = plan.model(&site.model).into_iter().collect();
    if let AiDriver::OpenAiCompatible(AiUrl::Setting(at)) = &plan.driver {
        read.push(*at);
    }
    read
}

/// Asks the model for its answer to `prompt`, a value of the prompt's
/// type `site` renders, as it is written; `settings` gives the values of
/// the settings [`settings_read`] names.
pub async fn stream(
    ctx: &mut Ctx<'_>,
    ai: &Ai<'_>,
    site: &StreamSite,
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
    let Some(messages) = rendered.as_ref().and_then(crate::ai::messages_of) else {
        return defect("a prompt that doesn't render");
    };
    let setting = |number: u32| {
        settings
            .iter()
            .find(|(at, _)| *at == number)
            .map(|(_, value)| value)
    };
    let model = ai
        .plan
        .model(&site.model)
        .and_then(setting)
        .and_then(|value| match jadpo_value::standard::from(value) {
            Ok(Value::Text(text)) => Some(text.to_string()),
            _ => None,
        });
    // Through the built-in driver only: a package's `stream:` mapping
    // comes with chunk 5.5v7, and the checker refuses a stream through one.
    let base = match &ai.plan.driver {
        AiDriver::OpenAiCompatible(AiUrl::Written(text)) => Some(text.clone()),
        AiDriver::OpenAiCompatible(AiUrl::Setting(at)) => match setting(*at) {
            Some(Value::Text(text)) => Some(text.to_string()),
            _ => None,
        },
        AiDriver::Package(_) => return defect("a stream through a package's connection"),
    };
    let made = Call::new(&ai.plan.connection, &site.prompt, prompt, Repeat::Safe);
    let (Some(model), Some(base)) = (
        model,
        base.and_then(|base| crate::provider::sendable(&base)),
    ) else {
        let now = ctx.now();
        ctx.capabilities_mut().refused(&made, now);
        return Done::Failed(Box::new(crate::misconfigured()));
    };
    let url = format!("{}/chat/completions", base.trim_end_matches('/'));
    let body = request_body(&model, &messages, site.max_output);
    let digest = jadpo_value::hash::sha256(body.as_bytes());
    let step = ctx.next_step();
    // An action run again after a concurrent change is given the stream
    // its first run opened, which nothing has read from yet.
    if let Some(step) = step
        && let Some(number) = ctx.reused_stream(step, &digest)
    {
        let now = ctx.now();
        let capabilities = ctx.capabilities_mut();
        let place = capabilities.sent(&made, Source::Replayed, now);
        capabilities.ended(place, &Answer::Wire(Rc::from("")), Some(0));
        return Done::Value(ai_stream(number));
    }
    let host = ctx.host();
    let began = host.monotonic_ms();
    let ends_at = began.map(|began| began.saturating_add(site.timeout_ms));
    let opened = match opened(ctx, made, &url, &body, began, site.timeout_ms).await {
        Ok(opened) => opened,
        Err(done) => return done,
    };
    let coming: Box<dyn Coming> = match opened {
        Streamed::Coming(body) => body,
        Streamed::Whole(answer) => match answer {
            Answer::Wire(text) => Box::new(Read::new([Chunk::Bytes(text.as_bytes().to_vec())])),
            Answer::TimedOut => return broke(RuleKey::TimedOut),
            Answer::Failed(code) if code == crate::ai::CONTEXT_LENGTH_EXCEEDED => {
                return broke(RuleKey::TooLong);
            }
            Answer::Failed(_) | Answer::Misconfigured => {
                return Done::Failed(Box::new(crate::misconfigured()));
            }
            Answer::Unavailable | Answer::RetryAfter(_) | Answer::Unknown => {
                return Done::Failed(Box::new(crate::unavailable()));
            }
            Answer::Value(_) | Answer::Generated | Answer::Recorded => {
                return defect("an answer to `ai.stream` that isn't a chat completion");
            }
        },
    };
    let mut reply = Reply::new(coming, ends_at);
    match reply.first(host).await {
        None => {}
        Some(Said::Refused) => return broke(RuleKey::Refused),
        Some(Said::CutOff) => return broke(RuleKey::OverBudget),
        Some(Said::TimedOut) => return broke(RuleKey::TimedOut),
        Some(_) => return Done::Failed(Box::new(crate::unavailable())),
    }
    let number = ctx.keep_stream(OpenStream {
        step,
        digest,
        events: Some(Box::new(reply)),
    });
    Done::Value(ai_stream(number))
}

/// A condition, for the call's `attempt` to map.
fn broke(rule: RuleKey) -> Done {
    Done::Broke {
        rule,
        problem: None,
    }
}

/// The request's JSON body: OpenAI's Chat Completions request, streamed.
fn request_body(model: &str, messages: &[(String, String)], max_output: u32) -> String {
    let mut body = serde_json::Map::new();
    body.insert("model".into(), model.into());
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
    body.insert("max_tokens".into(), max_output.into());
    body.insert("stream".into(), true.into());
    serde_json::Value::Object(body).to_string()
}

/// Sends the request through the capability seam, and again while the
/// provider couldn't be reached or asked for it later, as long as the
/// call's `timeout:` lasts, until its answer starts to come: nothing has
/// reached the client yet, so asking again is safe.
async fn opened(
    ctx: &mut Ctx<'_>,
    made: Call,
    url: &str,
    body: &str,
    began: Option<u64>,
    timeout_ms: u64,
) -> Result<Streamed, Done> {
    let host = ctx.host();
    let mut sends = 1;
    loop {
        let spent = match (began, host.monotonic_ms()) {
            (Some(began), Some(now)) => now.saturating_sub(began),
            _ => 0,
        };
        let left = timeout_ms.saturating_sub(spent);
        if left == 0 {
            return Ok(Streamed::Whole(Answer::TimedOut));
        }
        let request = Request {
            method: HttpMethod::Post,
            url: url.to_string(),
            headers: vec![
                ("Accept".to_string(), events::CONTENT_TYPE.to_string()),
                ("Content-Type".to_string(), "application/json".to_string()),
                ("user-agent".to_string(), "jadpo".to_string()),
            ],
            body: Some(body.to_string()),
            timeout_ms: left,
            reading: Reading {
                error_code: Some("error.code".to_string()),
                ..Reading::default()
            },
        };
        let call = made.clone().with_request(request);
        let streamed = match exchange(ctx, call.clone()).await {
            Ok(streamed) => streamed,
            Err(Unanswered::Failure(failure)) => return Err(Done::Failed(failure)),
            Err(Unanswered::Fault(kind)) => return Err(Done::Fault(kind)),
            Err(Unanswered::Failed(_)) => return Err(Done::Failed(Box::new(crate::unavailable()))),
        };
        if ctx.host().call_log().is_some() {
            let answer = match &streamed {
                Streamed::Whole(answer) => answer.clone(),
                Streamed::Coming(_) => Answer::Unknown,
            };
            crate::ai::log(ctx, &call, (url, body, None), &answer);
        }
        let asked = match &streamed {
            Streamed::Whole(Answer::Unavailable | Answer::Unknown) => None,
            Streamed::Whole(Answer::RetryAfter(millis)) => Some(*millis),
            _ => return Ok(streamed),
        };
        if sends >= jadpo_std::ai::SENDS {
            return Ok(streamed);
        }
        sends += 1;
        let Some(began) = began else {
            continue;
        };
        let spent = host.monotonic_ms().unwrap_or(began).saturating_sub(began);
        let left = timeout_ms.saturating_sub(spent);
        let mut random = [0u8; 8];
        let random = host.random(&mut random).then_some(random);
        let Some(wait) = crate::provider::retry_wait(sends - 1, asked, left, random) else {
            return Ok(streamed);
        };
        if wait > 0 {
            host.sleep(wait).await;
        }
    }
}

/// The capability seam for a streamed call, as [`crate::effects::exchange`]
/// is for any other: refused inside an open transaction or with no driver,
/// a replayed run's recorded answer, or the driver's, logged without its
/// values.
async fn exchange(ctx: &mut Ctx<'_>, call: Call) -> Result<Streamed, Unanswered> {
    let now = ctx.now();
    if ctx.close_snapshot().await.is_err() {
        ctx.capabilities_mut().refused(&call, now);
        return Err(Unanswered::Failure(Box::new(crate::unavailable())));
    }
    if ctx.transaction_open() {
        ctx.capabilities_mut().refused(&call, now);
        return Err(Unanswered::Fault(FaultKind::CallInTransaction(call.name())));
    }
    if let Some(replayed) = ctx.capabilities_mut().replayed(&call) {
        return match replayed {
            Ok(answer) => {
                let capabilities = ctx.capabilities_mut();
                let place = capabilities.sent(&call, Source::Replayed, now);
                capabilities.ended(place, &answer, Some(0));
                Ok(Streamed::Whole(answer))
            }
            Err(recorded) => {
                ctx.capabilities_mut().refused(&call, now);
                Err(Unanswered::Fault(FaultKind::Defect(format!(
                    "a replayed run calling `{}` where its recording has {recorded}",
                    call.name()
                ))))
            }
        };
    }
    let host = ctx.host();
    let Some(driver) = host.capability(&call.connection) else {
        ctx.capabilities_mut().refused(&call, now);
        return Err(Unanswered::Failure(Box::new(crate::misconfigured())));
    };
    let began = host.monotonic_ms();
    let place = ctx
        .capabilities_mut()
        .sent(&call, driver.source_of(&call), now);
    let streamed = driver.stream(&call).await;
    let took = began.and_then(|began| Some(host.monotonic_ms()?.saturating_sub(began)));
    // A stream that has started is logged as answered, never with its text.
    let answer = match &streamed {
        Streamed::Whole(answer) => answer.clone(),
        Streamed::Coming(_) => Answer::Wire(Rc::from("")),
    };
    ctx.capabilities_mut().ended(place, &answer, took);
    Ok(streamed)
}

// --- the fake --------------------------------------------------------------

/// The finish a scripted `partial(...)` stops with, as the provider says
/// it: a cut-off's `length`, a refusal's `content_filter`, and none for an
/// answer out of time, whose stream just stops.
pub(crate) fn finish_of(rule: RuleKey) -> &'static str {
    match rule {
        RuleKey::OverBudget => "length",
        RuleKey::Refused => "content_filter",
        _ => "",
    }
}

/// How many words a fake's piece holds (area 21, "Tests": "the scripted
/// text, cut into pieces of a few words").
const WORDS_PER_PIECE: usize = 3;

/// `text` cut into pieces of a few words, each keeping the space after
/// it, so joined they are the text again.
pub fn pieces(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut piece = String::new();
    let mut words = 0;
    let mut in_word = false;
    for c in text.chars() {
        if c.is_whitespace() {
            if in_word {
                words += 1;
                in_word = false;
            }
        } else {
            if !in_word && words == WORDS_PER_PIECE {
                out.push(std::mem::take(&mut piece));
                words = 0;
            }
            in_word = true;
        }
        piece.push(c);
    }
    if !piece.is_empty() {
        out.push(piece);
    }
    out
}

/// One `chat.completion.chunk` event, as an OpenAI-compatible server
/// streams it.
fn chunk_event(delta: serde_json::Value, finish: Option<&str>) -> Chunk {
    let mut choice = serde_json::Map::new();
    choice.insert("index".into(), 0.into());
    choice.insert("delta".into(), delta);
    choice.insert(
        "finish_reason".into(),
        finish.map_or(serde_json::Value::Null, serde_json::Value::from),
    );
    let mut chunk = serde_json::Map::new();
    chunk.insert("object".into(), "chat.completion.chunk".into());
    chunk.insert(
        "choices".into(),
        serde_json::Value::Array(vec![serde_json::Value::Object(choice)]),
    );
    Chunk::Bytes(format!("data: {}\n\n", serde_json::Value::Object(chunk)).into_bytes())
}

/// The fake's answer to a streamed call of prompt `prompt`, from what its
/// script gives (a chat completion, a condition, or a generated default),
/// in the built-in driver's own form: the text's pieces, each an event of
/// its own, then the finish and `[DONE]`; a `partial(...)` out of time
/// stops after its pieces.
pub fn faked(prompt: &str, answer: Answer) -> Streamed {
    let completion = match &answer {
        Answer::Generated => crate::ai::completion(
            Some(&format!("A generated answer to `{prompt}`.")),
            None,
            "stop",
        ),
        Answer::Wire(text) => text.clone(),
        _ => return Streamed::Whole(answer),
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&completion) else {
        return Streamed::Whole(answer);
    };
    let Some(choice) = json
        .get("choices")
        .and_then(serde_json::Value::as_array)
        .and_then(|choices| choices.first())
    else {
        return Streamed::Whole(answer);
    };
    let message = choice.get("message");
    let text = |name: &str| {
        message
            .and_then(|message| message.get(name))
            .and_then(serde_json::Value::as_str)
    };
    let mut chunks = Vec::new();
    if let Some(refusal) = text("refusal") {
        chunks.push(chunk_event(
            serde_json::json!({ "role": "assistant", "refusal": refusal }),
            Some("stop"),
        ));
        chunks.push(Chunk::Bytes(b"data: [DONE]\n\n".to_vec()));
        return Streamed::Coming(Box::new(Read::new(chunks)));
    }
    for piece in pieces(text("content").unwrap_or_default()) {
        chunks.push(chunk_event(serde_json::json!({ "content": piece }), None));
    }
    match choice
        .get("finish_reason")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
    {
        "" => chunks.push(Chunk::TimedOut),
        finish => {
            chunks.push(chunk_event(serde_json::json!({}), Some(finish)));
            chunks.push(Chunk::Bytes(b"data: [DONE]\n\n".to_vec()));
        }
    }
    Streamed::Coming(Box::new(Read::new(chunks)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fakes_text_is_cut_into_pieces_of_a_few_words() {
        assert_eq!(
            pieces("Our sourdough is baked beside sesame."),
            ["Our sourdough is ", "baked beside sesame."]
        );
        assert_eq!(
            pieces("Our sourdough is baked"),
            ["Our sourdough is ", "baked"]
        );
        assert_eq!(pieces("Yes."), ["Yes."]);
        assert_eq!(pieces(""), Vec::<String>::new());
        assert_eq!(pieces("a  b\nc d"), ["a  b\nc ", "d"]);
    }

    fn said(body: &str) -> Vec<Said> {
        let mut reader = Reader::default();
        let mut out = VecDeque::new();
        reader.feed(body.as_bytes(), &mut out);
        reader.end(&mut out);
        out.into_iter().collect()
    }

    /// OpenAI's stream is read for its pieces and how it ends, in any
    /// line ending, its usage chunk and comments skipped.
    #[test]
    fn a_streamed_chat_completion_is_read_piece_by_piece() {
        let body = ": keep-alive\r\n\r\ndata: {\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"\"},\"finish_reason\":null}]}\r\n\r\ndata: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Our \"},\"finish_reason\":null}]}\n\ndata: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"rye\"},\"finish_reason\":\"stop\"}]}\n\ndata: {\"choices\":[],\"usage\":{\"prompt_tokens\":3}}\n\ndata: [DONE]\n\n";
        assert_eq!(
            said(body),
            [
                Said::Piece("Our ".into()),
                Said::Piece("rye".into()),
                Said::Whole
            ]
        );
        // A finish and no `[DONE]` is still whole; neither is broken.
        assert_eq!(
            said(
                "data: {\"choices\":[{\"delta\":{\"content\":\"a\"},\"finish_reason\":\"stop\"}]}\n\n"
            ),
            [Said::Piece("a".into()), Said::Whole]
        );
        assert_eq!(
            said(
                "data: {\"choices\":[{\"delta\":{\"content\":\"a\"},\"finish_reason\":null}]}\n\n"
            ),
            [Said::Piece("a".into()), Said::Broken]
        );
        assert_eq!(
            said(
                "data: {\"choices\":[{\"delta\":{\"content\":\"a\"},\"finish_reason\":\"length\"}]}\n\n"
            ),
            [Said::Piece("a".into()), Said::CutOff]
        );
        assert_eq!(
            said(
                "data: {\"choices\":[{\"delta\":{\"refusal\":\"No.\"},\"finish_reason\":null}]}\n\n"
            ),
            [Said::Refused]
        );
        assert_eq!(
            said("data: {\"error\":{\"message\":\"overloaded\"}}\n\n"),
            [Said::Broken]
        );
        // A server that answered whole.
        assert_eq!(
            said(&crate::ai::completion(Some("Rye."), None, "stop")),
            [Said::Piece("Rye.".into()), Said::Whole]
        );
    }

    /// An event is written as `event:`, one `data:` line of JSON whose
    /// newlines are escaped, and a blank line.
    #[test]
    fn an_event_is_written_as_server_sent_events_write_one() {
        assert_eq!(
            String::from_utf8(ServerEvent::Piece("a\n\"b\"".into()).written("req_1")).unwrap(),
            "event: piece\ndata: {\"text\":\"a\\n\\\"b\\\"\"}\n\n"
        );
        assert_eq!(
            String::from_utf8(ServerEvent::Done.written("req_1")).unwrap(),
            "event: done\ndata: {}\n\n"
        );
        let (_, code, message) = crate::http::UNAVAILABLE;
        assert_eq!(
            String::from_utf8(ServerEvent::Failed { code, message }.written("req_1")).unwrap(),
            "event: error\ndata: {\"error\":{\"code\":\"unavailable\",\"message\":\"A dependency is temporarily unavailable.\",\"request_id\":\"req_1\"}}\n\n"
        );
    }

    /// A fake's scripted text streams as its pieces then `done`; a
    /// `partial(...)` stops part-way with an error event.
    #[test]
    fn a_fakes_stream_is_read_as_the_drivers_is() {
        let events = |answer: Answer| {
            let Streamed::Coming(body) = faked("answer_question", answer) else {
                panic!("a stream");
            };
            let mut reply = Reply::new(body, None);
            let mut out = Vec::new();
            while let Some(event) = crate::block_on(reply.next(None)).expect("nothing waits") {
                out.push(event);
            }
            out
        };
        let (_, code, message) = crate::http::UNAVAILABLE;
        assert_eq!(
            events(Answer::Wire(crate::ai::completion(
                Some("Our sourdough is baked beside sesame."),
                None,
                "stop"
            ))),
            [
                ServerEvent::Piece("Our sourdough is ".into()),
                ServerEvent::Piece("baked beside sesame.".into()),
                ServerEvent::Done
            ]
        );
        for rule in [RuleKey::TimedOut, RuleKey::OverBudget, RuleKey::Refused] {
            assert_eq!(
                events(Answer::Wire(crate::ai::completion(
                    Some("Our sourdough is baked"),
                    None,
                    finish_of(rule)
                ))),
                [
                    ServerEvent::Piece("Our sourdough is ".into()),
                    ServerEvent::Piece("baked".into()),
                    ServerEvent::Failed { code, message }
                ],
                "{rule:?}"
            );
        }
    }
}
