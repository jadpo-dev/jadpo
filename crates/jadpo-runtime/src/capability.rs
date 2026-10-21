//! The capability seam (docs/compiler-runtime.md §2.6; chunk 7.0c, from
//! the Phase 6 architecture review): what a call that leaves the process
//! is, what a driver answers it, and what is kept of it.
//!
//! Every outside call goes through one entry point, [`crate::effects::call`],
//! which both executors call: it refuses a call while the action's
//! transaction is open (area 6, "Outside calls hold no transaction"),
//! gives a replayed run its recorded answer, otherwise asks the driver
//! the host gives for the call's connection ([`crate::Host::capability`]),
//! and logs the call on its invocation ([`Logged`]) before it goes out and
//! again when it is answered. The log is what `jadpo calls` lists (area
//! 17; its screen is chunk 7.2's), what a recording keeps beside the
//! answers ([`Recording`]), and how the runner knows a delivery's run may
//! have acted outside, so one whose outcome is unknown is parked for an
//! operator rather than sent again blindly (area 12, `outcome_unknown`;
//! area 16, "Email").
//!
//! A driver is one trait, [`Capability`], named by the connection it
//! serves (area 11): every capability's driver, a package's included,
//! implements it, so the seam logs, records, replays and fakes them all
//! the same way. A fake is a host wrapper ([`Faked`]) giving [`Fake`]s in
//! some connections' place, so a test and `jadpo dev` (chunk 7.2) run the
//! real host with only those calls answered from a script.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use jadpo_value::Value;

pub use jadpo_std::subscriber::Repeat;

use crate::{Database, Host, LocalFuture, Passkeys};

/// One call that leaves the process: an operation of the capability or
/// provider a connection fills (area 11), given its input.
#[derive(Clone, PartialEq)]
pub struct Call {
    /// The connection it goes through, which the host gives a driver for:
    /// the one `email:` names, a `files` store's, `stripe`.
    pub connection: String,
    /// Its operation: `send`, `put`, `payment_intents.create`.
    pub operation: String,
    pub input: Value,
    /// Whether it may be sent again.
    pub repeat: Repeat,
    /// A provider operation's request as the package driver sends it:
    /// its method, address, headers and body, made from `input` by the
    /// runtime (area 11, "Calling"; chunk 7.2c). `None` for every other
    /// capability's call, and for a fake, which reads `input`.
    pub request: Option<Box<crate::provider::Request>>,
    /// Its place among the steps of the running action's record, when one
    /// is kept and the call is a step (chunk 7.5b): written before it goes
    /// out.
    pub step: Option<crate::steps::StepAt>,
    /// The idempotency key a keyed provider call is sent under, which a
    /// package's fake keeps its answer under as its provider would
    /// (chunk 7.5d). Never logged.
    pub key: Option<SentKey>,
    /// Which of a call's answers it asks for, from 1: an `ai.generate`
    /// call's repair asks again (area 21; chunk 5.5v3), which a fake's
    /// `.requests` doesn't list as a call of its own.
    pub attempt: u32,
    /// What an email's message was sent as, for a host that keeps the
    /// mail it catches (the dev inbox, chunk 7.3c2); `None` for every
    /// other call.
    pub mail: Option<Box<MailSent>>,
}

/// An email's message as a host that catches mail sees it (chunk
/// 7.3c2): the email it was rendered from and the trace of the work that
/// emitted it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MailSent {
    /// The email's identity, `accounts.WelcomeEmail`.
    pub email: String,
    /// Its trace's id, 32 hex digits, when the work had one.
    pub trace: Option<String>,
}

/// The idempotency key a provider call goes under, and its package's code
/// for a key that came with a different request (`reused:`), which a fake
/// answers such a call with (area 11, "Tests and tools"; chunk 7.5d).
#[derive(Clone, PartialEq, Eq)]
pub struct SentKey {
    pub key: String,
    pub reused: Option<String>,
    /// Its operation's failure codes that say it did nothing and is worth
    /// sending again (their kind `unavailable` or `rate_limited`), which
    /// keep nothing under the key.
    pub did_nothing: Vec<String>,
}

impl std::fmt::Debug for SentKey {
    /// Never the key.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SentKey").finish_non_exhaustive()
    }
}

impl Call {
    /// A call of `operation` through `connection` with `input`.
    pub fn new(
        connection: impl Into<String>,
        operation: impl Into<String>,
        input: Value,
        repeat: Repeat,
    ) -> Call {
        Call {
            connection: connection.into(),
            operation: operation.into(),
            input,
            repeat,
            request: None,
            step: None,
            key: None,
            attempt: 1,
            mail: None,
        }
    }

    /// The same call, sent as `request` by the package driver.
    pub fn with_request(mut self, request: crate::provider::Request) -> Call {
        self.request = Some(Box::new(request));
        self
    }

    /// How a report names it: `mail.send`.
    pub fn name(&self) -> String {
        format!("{}.{}", self.connection, self.operation)
    }
}

impl std::fmt::Debug for Call {
    /// Never its input, which can hold secrets and personal data.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Call")
            .field("connection", &self.connection)
            .field("operation", &self.operation)
            .field("repeat", &self.repeat)
            .finish_non_exhaustive()
    }
}

/// What a driver answers a call, as each driver classifies its own (§2.6),
/// so every capability and target agree on what happened.
#[derive(Clone, PartialEq)]
pub enum Answer {
    /// What the operation gives.
    Value(Value),
    /// A provider's answer as it was sent, its JSON text, for the call's
    /// site to decode into its operation's output (area 11, "Decoding";
    /// chunk 7.2c): what the package driver answers, and a recording
    /// keeps, so a replay decodes it again as the build it runs under
    /// does.
    Wire(Rc<str>),
    /// One of the operation's own failures, by its code: a condition the
    /// call maps (area 11, "Calling"), which the provider refused.
    Failed(String),
    /// It surely did nothing: it never left, or the provider said so.
    /// Safe to send again.
    Unavailable,
    /// It may have acted: it went out and no answer came back (area 4's
    /// `outcome_unknown`).
    Unknown,
    /// It went out and no answer came back within its timeout (chunk
    /// 7.2c2): as [`Answer::Unknown`], but a call safe to send again may
    /// map it at its site as `timed_out`.
    TimedOut,
    /// It surely did nothing, and the provider asked for it to be sent
    /// again only after this many milliseconds, its `Retry-After` (chunk
    /// 7.2c2): as [`Answer::Unavailable`] otherwise.
    RetryAfter(u64),
    /// The connection's credential or settings were refused.
    Misconfigured,
    /// A provider operation's generated default (area 11, "Tests and
    /// fakes"; chunk 7.2d): what a package's fake answers a call nothing
    /// scripted, made by the call's site from the operation's output type,
    /// as the provider would send it, and read back as any answer is.
    Generated,
    /// A provider operation's recording (area 11, "Provider calls in
    /// development"; chunk 7.2d2): what `jadpo dev`'s fake answers a call
    /// nothing scripted with when the operation's package keeps one, its
    /// placeholders filled by the call's site with generated values of
    /// their types.
    Recorded,
}

impl std::fmt::Debug for Answer {
    /// Never the value it gives.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Answer::Value(_) => f.write_str("Value(..)"),
            Answer::Wire(_) => f.write_str("Wire(..)"),
            Answer::Failed(code) => f.debug_tuple("Failed").field(code).finish(),
            Answer::Unavailable => f.write_str("Unavailable"),
            Answer::Unknown => f.write_str("Unknown"),
            Answer::TimedOut => f.write_str("TimedOut"),
            Answer::RetryAfter(millis) => f.debug_tuple("RetryAfter").field(millis).finish(),
            Answer::Misconfigured => f.write_str("Misconfigured"),
            Answer::Generated => f.write_str("Generated"),
            Answer::Recorded => f.write_str("Recorded"),
        }
    }
}

/// Where a call's answer came from (area 17, "Provider calls in
/// development").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// The connection's own driver.
    Live,
    /// A fake in its place (area 13).
    Fake,
    /// A recorded run's answer, given back without the call going out
    /// (area 17, "Replaying a production request").
    Replayed,
    /// A package's fake, answering as a test's `fakes:` or `jadpo calls
    /// script` scripted it (chunk 7.2d).
    Scripted,
    /// A package's fake, answering with the operation's generated default
    /// (chunk 7.2d).
    Generated,
    /// The provider's sandbox, which `jadpo dev` lets the call through to
    /// (area 11, "Provider calls in development"; chunk 7.2d).
    Sandbox,
    /// A package's fake, answering from the operation's recording with its
    /// placeholders filled (chunk 7.2d2).
    Recording,
}

impl Source {
    /// How the call log names it (area 17).
    pub fn word(self) -> &'static str {
        match self {
            Source::Live => "live",
            Source::Fake => "fake",
            Source::Replayed => "replayed",
            Source::Scripted => "scripted",
            Source::Generated => "generated",
            Source::Sandbox => "sandbox",
            Source::Recording => "recording",
        }
    }
}

/// The driver of a capability a connection fills: the one trait the seam
/// calls, whatever the capability. A driver classifies what happened into
/// an [`Answer`]; the seam does the rest.
pub trait Capability {
    /// Sends `call` and gives its answer.
    fn call<'a>(&'a self, call: &'a Call) -> LocalFuture<'a, Answer>;

    /// Where its answers come from.
    fn source(&self) -> Source {
        Source::Live
    }

    /// Where its answer to `call` comes from, asked as it goes out: a
    /// package's fake tells a scripted answer from a generated one.
    fn source_of(&self, call: &Call) -> Source {
        let _ = call;
        self.source()
    }

    /// The messages it caught in place of sending them, in the order they
    /// were sent: what `email.sent` reads of the email connection's fake
    /// in a test (area 16; chunk 7.3c). A driver that sends catches none.
    fn caught(&self) -> Vec<Value> {
        Vec::new()
    }

    /// Whether it catches mail in place of sending it, so the message the
    /// runtime rendered is handed to it whole, whatever connection it
    /// stands in for, a mail package's included (area 13, "One fake per
    /// capability"; chunk 7.3c).
    fn catches_mail(&self) -> bool {
        false
    }

    /// Scripts the next answer to a call of `operation` through
    /// `connection` (area 13, `fakes:`; chunk 7.2d), scripted by the test
    /// at `from`: whether it takes scripts, as only a package's fake does.
    fn script(
        &self,
        connection: &str,
        operation: &str,
        answer: Answer,
        from: crate::Frame,
    ) -> bool {
        let _ = (connection, operation, answer, from);
        false
    }

    /// Sends `call` and gives its answer as it comes, for `ai.stream`
    /// (area 21; chunk 5.5v6): the whole answer, as [`Capability::call`]
    /// gives it, unless the driver can read it piece by piece.
    fn stream<'a>(&'a self, call: &'a Call) -> LocalFuture<'a, crate::stream::Streamed> {
        Box::pin(async move { crate::stream::Streamed::Whole(self.call(call).await) })
    }

    /// The inputs of the calls of `operation` through `connection` it was
    /// sent, in order, one per call however often it asked again: what a
    /// test reads as `.requests` (area 13; area 21, chunk 5.5v3). Only a
    /// fake keeps them.
    fn requests(&self, connection: &str, operation: &str) -> Vec<Value> {
        let _ = (connection, operation);
        Vec::new()
    }
}

/// How a logged call ended. Never with its values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ended {
    /// Sent, and not answered yet: what a call whose run was cut off while
    /// it was out stays as.
    InFlight,
    Answered,
    /// One of its operation's own failures, by its code.
    Failed(String),
    Unavailable,
    Unknown,
    Misconfigured,
    /// Never sent: the runtime refused it (inside an open transaction, no
    /// driver for its connection, a replay that no longer matches its
    /// recording).
    Refused,
}

impl Ended {
    pub(crate) fn of(answer: &Answer) -> Ended {
        match answer {
            Answer::Value(_) | Answer::Wire(_) => Ended::Answered,
            Answer::Failed(code) => Ended::Failed(code.clone()),
            Answer::Unavailable | Answer::RetryAfter(_) => Ended::Unavailable,
            Answer::Unknown | Answer::TimedOut => Ended::Unknown,
            Answer::Misconfigured => Ended::Misconfigured,
            Answer::Generated | Answer::Recorded => Ended::Answered,
        }
    }
}

/// One outside call as its invocation's log keeps it: which connection and
/// operation, whether it may be sent again, where its answer came from,
/// when it went out, how long it took and how it ended. Never its input
/// nor its answer, which can hold secrets and personal data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Logged {
    pub connection: String,
    pub operation: String,
    pub repeat: Repeat,
    pub source: Source,
    /// When it went out, on the invocation's clock, in milliseconds since
    /// 1970-01-01T00:00:00Z.
    pub at: i64,
    /// How long it took to be answered, on the host's monotonic clock:
    /// `None` while it is out, or on a host with no such clock.
    pub took_ms: Option<u64>,
    pub ended: Ended,
    /// Its place among the steps of an action with several changing
    /// calls, from 1, when it is one, sent or given back from the record
    /// (chunk 7.5f).
    pub step: Option<u32>,
}

impl Logged {
    /// Whether it may have acted outside in a way sending it again would
    /// repeat: it changes something with no key, and was answered, or went
    /// out and no answer came back (area 16: "a send whose outcome is
    /// unknown is parked for an operator, never resent blindly").
    pub fn may_have_acted(&self) -> bool {
        self.repeat == Repeat::Unkeyed
            && matches!(
                self.ended,
                Ended::InFlight | Ended::Answered | Ended::Unknown
            )
    }
}

/// One call's answer as a recording keeps it, given back in order when the
/// run is replayed (area 17).
#[derive(Clone, PartialEq)]
pub struct Recorded {
    pub connection: String,
    pub operation: String,
    pub answer: Answer,
}

impl std::fmt::Debug for Recorded {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Recorded")
            .field("connection", &self.connection)
            .field("operation", &self.operation)
            .field("answer", &self.answer)
            .finish()
    }
}

/// What a recorded run kept of its effects (area 17, "Replaying a
/// production request"): the random bytes it was given and each outside
/// call's answer, in order. Never the credentials a run made, which are
/// fresh in a replay ([`crate::Ctx::secret_random_bytes`]).
#[derive(Clone, Default, PartialEq)]
pub struct Recording {
    pub random: Vec<u8>,
    pub calls: Vec<Recorded>,
}

impl std::fmt::Debug for Recording {
    /// Never the bytes or the answers: they can hold tokens and personal
    /// data.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Recording")
            .field("random", &self.random.len())
            .field("calls", &self.calls.len())
            .finish()
    }
}

/// A fake's answer to one call.
#[derive(Clone, Debug, PartialEq)]
pub enum Scripted {
    Answer(Answer),
    /// Never answers: a call still out when its run is cut off.
    Hang,
}

/// A capability that answers from a script (area 13's `fakes:`): each call
/// of an operation with the next answer scripted for it, or else the
/// fake's default; every call it was sent kept in order, for a test to
/// read as area 13's `.requests`.
pub struct Fake {
    script: RefCell<VecDeque<(String, Scripted)>>,
    default: Answer,
    sent: RefCell<Vec<Call>>,
}

impl Default for Fake {
    fn default() -> Fake {
        Fake::new()
    }
}

impl std::fmt::Debug for Fake {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Fake")
            .field("scripted", &self.script.borrow().len())
            .field("sent", &self.sent.borrow().len())
            .finish()
    }
}

impl Fake {
    /// A fake answering every call with `Unit`.
    pub fn new() -> Fake {
        Fake::answering(Answer::Value(Value::Unit))
    }

    /// A fake answering every call not scripted with `default`.
    pub fn answering(default: Answer) -> Fake {
        Fake {
            script: RefCell::new(VecDeque::new()),
            default,
            sent: RefCell::new(Vec::new()),
        }
    }

    /// Answers the next call of `operation` with `answer`, after what is
    /// scripted for it already.
    pub fn script(&self, operation: &str, answer: Scripted) {
        self.script
            .borrow_mut()
            .push_back((operation.to_string(), answer));
    }

    /// The calls it was sent, in order.
    pub fn sent(&self) -> Vec<Call> {
        self.sent.borrow().clone()
    }

    /// The next answer for a call of `operation`.
    fn next(&self, operation: &str) -> Scripted {
        let mut script = self.script.borrow_mut();
        match script
            .iter()
            .position(|(scripted, _)| scripted == operation)
        {
            Some(at) => script
                .remove(at)
                .map_or(Scripted::Answer(self.default.clone()), |(_, answer)| answer),
            None => Scripted::Answer(self.default.clone()),
        }
    }
}

impl Capability for Fake {
    fn call<'a>(&'a self, call: &'a Call) -> LocalFuture<'a, Answer> {
        self.sent.borrow_mut().push(call.clone());
        match self.next(&call.operation) {
            Scripted::Answer(answer) => Box::pin(async move { answer }),
            Scripted::Hang => Box::pin(std::future::pending()),
        }
    }

    fn source(&self) -> Source {
        Source::Fake
    }

    fn catches_mail(&self) -> bool {
        true
    }

    /// Each message a `send` gave it.
    fn caught(&self) -> Vec<Value> {
        self.sent
            .borrow()
            .iter()
            .filter(|call| call.operation == jadpo_std::email::SEND)
            .map(|call| call.input.clone())
            .collect()
    }
}

/// What picks a [`Faked`]'s chosen fake for a connection, by its place.
type Choose = Rc<dyn Fn(&str) -> Option<usize>>;

/// `host` with fakes in some connections' place (area 13): a test runs on
/// it, and `jadpo dev` will (chunk 7.2). Every other capability, and
/// everything else a host gives, is `host`'s own.
pub struct Faked<H> {
    host: H,
    fakes: Vec<(String, Rc<dyn Capability>)>,
    /// Fakes chosen for a connection as it is asked (`jadpo dev`'s, whose
    /// connections change as the project is saved), and what chooses.
    chosen: Vec<Rc<dyn Capability>>,
    choose: Option<Choose>,
    /// The record a test keeps of what it did (chunk 8.1c), when it keeps
    /// one.
    record: Option<Rc<crate::record::Record>>,
}

impl<H> std::fmt::Debug for Faked<H> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let names: Vec<&str> = self.fakes.iter().map(|(name, _)| name.as_str()).collect();
        f.debug_struct("Faked")
            .field("fakes", &names)
            .finish_non_exhaustive()
    }
}

impl<H: Host> Faked<H> {
    pub fn new(host: H) -> Faked<H> {
        Faked {
            host,
            fakes: Vec::new(),
            chosen: Vec::new(),
            choose: None,
            record: None,
        }
    }

    /// Keeps each outside call in `record` (chunk 8.1c).
    pub fn with_record(mut self, record: Rc<crate::record::Record>) -> Faked<H> {
        self.record = Some(record);
        self
    }

    /// Gives `fake` in place of connection `connection`'s driver.
    pub fn with(mut self, connection: &str, fake: Rc<dyn Capability>) -> Faked<H> {
        self.fakes.push((connection.to_string(), fake));
        self
    }

    /// Gives `fakes[choose(connection)]` in place of the driver of each
    /// connection `choose` picks one for, decided each time a connection
    /// is asked for: `jadpo dev`'s fakes, chosen from the program it runs
    /// now, whose connections change as the project is saved. A fake
    /// given by name ([`Faked::with`]) comes first.
    pub fn choosing(
        mut self,
        fakes: Vec<Rc<dyn Capability>>,
        choose: impl Fn(&str) -> Option<usize> + 'static,
    ) -> Faked<H> {
        self.chosen = fakes;
        self.choose = Some(Rc::new(choose));
        self
    }

    /// The host it wraps.
    pub fn host(&self) -> &H {
        &self.host
    }
}

impl<H: Host> Host for Faked<H> {
    fn capability(&self, connection: &str) -> Option<&dyn Capability> {
        if let Some((_, fake)) = self.fakes.iter().find(|(name, _)| name == connection) {
            return Some(fake.as_ref());
        }
        let chosen = self
            .choose
            .as_ref()
            .and_then(|choose| choose(connection))
            .and_then(|at| self.chosen.get(at));
        match chosen {
            Some(fake) => Some(fake.as_ref()),
            None => self.host.capability(connection),
        }
    }

    fn call_log(&self) -> Option<&dyn crate::call_log::CallLog> {
        self.host.call_log()
    }

    fn record(&self) -> Option<&crate::record::Record> {
        self.record.as_deref().or_else(|| self.host.record())
    }

    fn passkeys(&self) -> Option<&dyn Passkeys> {
        self.host.passkeys()
    }

    fn test_authenticators(&self) -> Option<&dyn crate::TestAuthenticators> {
        self.host.test_authenticators()
    }

    fn outbound(&self) -> Option<&dyn crate::Outbound> {
        self.host.outbound()
    }

    fn database(&self, store: &str) -> Option<&dyn Database> {
        self.host.database(store)
    }

    fn defer(&self, work: LocalFuture<'static, ()>) {
        self.host.defer(work);
    }

    fn budget_ms(&self, invocation: crate::budget::Invocation) -> u64 {
        self.host.budget_ms(invocation)
    }

    fn rows_per_batch(&self) -> u32 {
        self.host.rows_per_batch()
    }

    fn stopping(&self) -> LocalFuture<'_, ()> {
        self.host.stopping()
    }

    fn hand_on(&self, work: LocalFuture<'static, ()>) {
        self.host.hand_on(work);
    }

    fn variable(&self, name: &str) -> Option<String> {
        self.host.variable(name)
    }

    fn random(&self, bytes: &mut [u8]) -> bool {
        self.host.random(bytes)
    }

    fn sleep(&self, millis: u64) -> LocalFuture<'_, ()> {
        self.host.sleep(millis)
    }

    fn yield_now(&self) -> LocalFuture<'_, ()> {
        self.host.yield_now()
    }

    fn monotonic_ms(&self) -> Option<u64> {
        self.host.monotonic_ms()
    }

    fn now_ms(&self) -> Option<i64> {
        self.host.now_ms()
    }

    fn rate_limit<'a>(
        &'a self,
        key: &'a str,
        limit: jadpo_std::http::RateLimit,
        now: i64,
    ) -> LocalFuture<'a, crate::Allowed> {
        self.host.rate_limit(key, limit, now)
    }

    fn slots(&self) -> &crate::slots::Slots {
        self.host.slots()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effects::{Unanswered, call};
    use crate::{ActionBody, Clock, Ctx, Env, FaultKind, Isolation, MemoryHost, Outcome, Places};

    /// A test capability: the `mail` connection's `send`, a fake answering
    /// as scripted.
    fn mail() -> (Rc<Fake>, Faked<MemoryHost>) {
        let fake = Rc::new(Fake::answering(Answer::Value(Value::text("receipt-1"))));
        let host =
            Faked::new(MemoryHost::new()).with("mail", Rc::clone(&fake) as Rc<dyn Capability>);
        (fake, host)
    }

    fn send(to: &str, repeat: Repeat) -> Call {
        Call::new("mail", "send", Value::text(to), repeat)
    }

    /// A fake chosen as each connection is asked stands in for it, as the
    /// chooser decides then (chunk 8.1d): `jadpo dev`'s, whose program
    /// changes as the project is saved; a fake given by name comes first.
    #[test]
    fn a_chosen_fake_is_decided_as_its_connection_is_asked() {
        let faked = Rc::new(std::cell::RefCell::new(vec!["couriers".to_string()]));
        let seen = Rc::clone(&faked);
        let (named, host) = mail();
        let chosen = Rc::new(Fake::new());
        let host = host.choosing(
            vec![Rc::clone(&chosen) as Rc<dyn Capability>],
            move |asked| seen.borrow().iter().any(|name| name == asked).then_some(0),
        );
        let is = |connection: &str, fake: &Rc<Fake>| {
            host.capability(connection).is_some_and(|found| {
                std::ptr::addr_eq(found as *const dyn Capability, Rc::as_ptr(fake))
            })
        };
        assert!(is("couriers", &chosen));
        assert!(host.capability("rates").is_none());
        faked.borrow_mut().push("rates".into());
        faked.borrow_mut().push("mail".into());
        assert!(is("rates", &chosen));
        assert!(is("mail", &named));
    }

    fn env() -> Env {
        Env {
            seed: Some(1),
            clock: Clock::TEST,
        }
    }

    fn run(ctx: &mut Ctx<'_>, made: Call) -> Result<Value, Unanswered> {
        crate::block_on(call(ctx, made)).expect("nothing waits")
    }

    fn code(result: &Result<Value, Unanswered>) -> String {
        match result {
            Ok(_) => "value".into(),
            Err(Unanswered::Failed(code)) => format!("failed {code}"),
            Err(Unanswered::Failure(failure)) => failure.code.clone(),
            Err(Unanswered::Fault(kind)) => kind.classification().into(),
        }
    }

    /// Each call is logged on its invocation as it goes out and as it is
    /// answered: its connection, operation, how it may be repeated, where
    /// its answer came from, when, and how it ended. Never its input or its
    /// answer, which the fake was sent and gave.
    #[test]
    fn a_call_is_logged_on_its_invocation_without_its_values() {
        let (fake, host) = mail();
        let places = Places::new();
        let mut ctx = Ctx::new(&places, &host, env());
        let sent = run(&mut ctx, send("ada@example.com", Repeat::Unkeyed));
        assert_eq!(sent, Ok(Value::text("receipt-1")));
        assert_eq!(
            ctx.capabilities().calls(),
            [Logged {
                connection: "mail".into(),
                operation: "send".into(),
                repeat: Repeat::Unkeyed,
                source: Source::Fake,
                at: Clock::TEST.now(),
                took_ms: None,
                ended: Ended::Answered,
                step: None,
            }]
        );
        assert_eq!(fake.sent(), [send("ada@example.com", Repeat::Unkeyed)]);
        let shown = format!("{ctx:?} {:?}", fake.sent());
        assert!(!shown.contains("ada@example.com"), "{shown}");
        assert!(!shown.contains("receipt-1"), "{shown}");
        assert_eq!(ctx.take_calls().len(), 1);
        assert!(ctx.capabilities().calls().is_empty());
    }

    /// What a driver answers decides what the call gives: a value, its
    /// operation's own failure for the call to map, or what the runtime
    /// raises in its place. An answer that may have acted is
    /// `outcome_unknown` only when sending it again could act twice.
    #[test]
    fn a_drivers_answer_decides_what_the_call_gives() {
        let (fake, host) = mail();
        let places = Places::new();
        let mut ctx = Ctx::new(&places, &host, env());
        for answer in [
            Answer::Failed("recipient_rejected".into()),
            Answer::Unavailable,
            Answer::Unknown,
            Answer::Unknown,
            Answer::Unknown,
            Answer::Misconfigured,
        ] {
            fake.script("send", Scripted::Answer(answer));
        }
        let given: Vec<String> = [
            Repeat::Unkeyed,
            Repeat::Unkeyed,
            Repeat::Unkeyed,
            Repeat::Keyed,
            Repeat::Safe,
            Repeat::Unkeyed,
        ]
        .into_iter()
        .map(|repeat| code(&run(&mut ctx, send("ada", repeat))))
        .collect();
        assert_eq!(
            given,
            [
                "failed recipient_rejected",
                "unavailable",
                "outcome_unknown",
                "unavailable",
                "unavailable",
                "misconfigured",
            ]
        );
        let ended: Vec<Ended> = ctx
            .capabilities()
            .calls()
            .iter()
            .map(|logged| logged.ended.clone())
            .collect();
        assert_eq!(
            ended,
            [
                Ended::Failed("recipient_rejected".into()),
                Ended::Unavailable,
                Ended::Unknown,
                Ended::Unknown,
                Ended::Unknown,
                Ended::Misconfigured,
            ]
        );
        // A connection no driver fills is misconfigured, and never sent.
        let refused = run(
            &mut ctx,
            Call::new("sms", "send", Value::Unit, Repeat::Unkeyed),
        );
        assert_eq!(code(&refused), "misconfigured");
        assert_eq!(
            ctx.capabilities()
                .calls()
                .last()
                .map(|logged| &logged.ended),
            Some(&Ended::Refused)
        );
    }

    /// Whether a call may have acted in a way sending it again repeats: an
    /// unkeyed change that was answered, or went out with no answer.
    #[test]
    fn only_an_unkeyed_change_that_went_out_may_have_acted() {
        let logged = |repeat, ended| Logged {
            connection: "mail".into(),
            operation: "send".into(),
            repeat,
            source: Source::Live,
            at: 0,
            took_ms: None,
            ended,
            step: None,
        };
        for (repeat, ended, acted) in [
            (Repeat::Unkeyed, Ended::Answered, true),
            (Repeat::Unkeyed, Ended::InFlight, true),
            (Repeat::Unkeyed, Ended::Unknown, true),
            (Repeat::Unkeyed, Ended::Unavailable, false),
            (Repeat::Unkeyed, Ended::Failed("x".into()), false),
            (Repeat::Unkeyed, Ended::Refused, false),
            (Repeat::Keyed, Ended::Unknown, false),
            (Repeat::Safe, Ended::Answered, false),
        ] {
            assert_eq!(
                logged(repeat, ended.clone()).may_have_acted(),
                acted,
                "{repeat:?} {ended:?}"
            );
        }
    }

    /// A recorded run keeps each call's answer beside its random bytes; a
    /// replay gives them back in order without the call going out, and a
    /// call the recording doesn't have next is refused before it goes out.
    #[test]
    fn a_recorded_run_replays_its_calls_without_sending_them() {
        let (fake, host) = mail();
        let places = Places::new();
        let mut recorded = Ctx::new(&places, &host, env());
        recorded.record_run();
        assert_eq!(
            run(&mut recorded, send("ada", Repeat::Unkeyed)),
            Ok(Value::text("receipt-1"))
        );
        let recording = recorded.recorded_run().expect("recording").clone();
        assert_eq!(recording.calls.len(), 1);
        assert!(
            !format!("{recording:?}").contains("receipt-1"),
            "never the answer"
        );

        // On a host whose fake would answer otherwise: nothing is sent.
        fake.script("send", Scripted::Answer(Answer::Unavailable));
        let mut replayed = Ctx::new(&places, &host, env());
        replayed.replay_run(recording.clone());
        assert_eq!(
            run(&mut replayed, send("ada", Repeat::Unkeyed)),
            Ok(Value::text("receipt-1"))
        );
        assert_eq!(
            fake.sent().len(),
            1,
            "only the recorded run's call went out"
        );
        assert_eq!(
            replayed
                .capabilities()
                .calls()
                .last()
                .map(|logged| logged.source),
            Some(Source::Replayed)
        );
        // Past the recording, or a call it doesn't have next: refused.
        let past = run(&mut replayed, send("ada", Repeat::Unkeyed));
        assert!(
            matches!(&past, Err(Unanswered::Fault(FaultKind::Defect(what))) if what.contains("nothing")),
            "{past:?}"
        );
        let mut other = Ctx::new(&places, &host, env());
        other.replay_run(recording);
        let diverged = run(
            &mut other,
            Call::new("mail", "suppress", Value::Unit, Repeat::Safe),
        );
        assert!(
            matches!(&diverged, Err(Unanswered::Fault(FaultKind::Defect(what))) if what.contains("`mail.send`")),
            "{diverged:?}"
        );
        assert_eq!(fake.sent().len(), 1);
    }

    /// An action that makes a call before its first write: then one after
    /// it, by which time its transaction is open.
    struct Calls {
        before: Option<Result<Value, Unanswered>>,
        after: Option<Result<Value, Unanswered>>,
    }

    impl<'p> ActionBody<'p> for Calls {
        fn run<'c>(&'c mut self, ctx: &'c mut Ctx<'p>) -> LocalFuture<'c, Outcome> {
            Box::pin(async move {
                self.before = Some(call(ctx, send("ada", Repeat::Unkeyed)).await);
                if ctx.transaction("main").await.is_err() {
                    return Outcome::Value(Value::Unit);
                }
                self.after = Some(call(ctx, send("ada", Repeat::Unkeyed)).await);
                Outcome::Value(Value::Unit)
            })
        }

        fn isolation(&self) -> Isolation {
            Isolation::ReadCommitted
        }
    }

    /// No call goes out while the action's transaction is open (area 6,
    /// "Outside calls hold no transaction"): one made after the action's
    /// first write is refused with a fault naming it, and never
    /// reaches the driver; one made before goes out.
    #[test]
    fn a_call_inside_an_open_transaction_is_refused_before_it_goes_out() {
        let (fake, host) = mail();
        let places = Places::new();
        let mut ctx = Ctx::new(&places, &host, env());
        let mut body = Calls {
            before: None,
            after: None,
        };
        let outcome = crate::block_on(crate::action(&mut ctx, &mut body)).expect("nothing waits");
        assert!(matches!(outcome, Outcome::Value(_)), "{outcome:?}");
        assert_eq!(body.before, Some(Ok(Value::text("receipt-1"))));
        assert_eq!(
            body.after,
            Some(Err(Unanswered::Fault(FaultKind::CallInTransaction(
                "mail.send".into()
            ))))
        );
        assert_eq!(fake.sent().len(), 1, "only the call before went out");
        let ended: Vec<&Ended> = ctx
            .capabilities()
            .calls()
            .iter()
            .map(|logged| &logged.ended)
            .collect();
        assert_eq!(ended, [&Ended::Answered, &Ended::Refused]);
        assert_eq!(
            FaultKind::CallInTransaction("mail.send".into()).classification(),
            "call_in_transaction"
        );
    }

    /// An action that reads, then makes a call: what it read is a
    /// snapshot, closed before the call goes out (area 6, "Outside calls
    /// hold no transaction"; chunk 7.4).
    struct ReadsThenCalls {
        called: Option<Result<Value, Unanswered>>,
        open_after: bool,
    }

    impl<'p> ActionBody<'p> for ReadsThenCalls {
        fn run<'c>(&'c mut self, ctx: &'c mut Ctx<'p>) -> LocalFuture<'c, Outcome> {
            Box::pin(async move {
                if ctx.read_transaction("main").await.is_err() {
                    return Outcome::Value(Value::Unit);
                }
                self.called = Some(call(ctx, send("ada", Repeat::Unkeyed)).await);
                self.open_after = ctx.transaction_open();
                Outcome::Value(Value::Unit)
            })
        }

        fn isolation(&self) -> Isolation {
            Isolation::RepeatableRead
        }
    }

    /// A call after the action has only read goes out, its snapshot
    /// closed first, and no transaction is open when it is answered.
    #[test]
    fn a_call_after_only_reads_closes_the_snapshot_and_goes_out() {
        let (fake, host) = mail();
        let places = Places::new();
        let mut ctx = Ctx::new(&places, &host, env());
        let mut body = ReadsThenCalls {
            called: None,
            open_after: true,
        };
        let outcome = crate::block_on(crate::action(&mut ctx, &mut body)).expect("nothing waits");
        assert!(matches!(outcome, Outcome::Value(_)), "{outcome:?}");
        assert_eq!(body.called, Some(Ok(Value::text("receipt-1"))));
        assert!(!body.open_after, "the snapshot closed before the call");
        assert_eq!(fake.sent().len(), 1);
    }

    /// A call whose run is dropped while it is out stays logged as in
    /// flight: it may have acted.
    #[test]
    fn a_call_cut_off_while_it_is_out_stays_in_flight() {
        let (fake, host) = mail();
        fake.script("send", Scripted::Hang);
        let places = Places::new();
        let mut ctx = Ctx::new(&places, &host, env());
        let stalled = crate::block_on(call(&mut ctx, send("ada", Repeat::Unkeyed)));
        assert!(stalled.is_err(), "it never answers");
        assert_eq!(
            ctx.capabilities()
                .calls()
                .last()
                .map(|logged| &logged.ended),
            Some(&Ended::InFlight)
        );
        assert!(ctx.capabilities().may_have_acted(0));
        assert!(!ctx.capabilities().may_have_acted(1));
    }
}
