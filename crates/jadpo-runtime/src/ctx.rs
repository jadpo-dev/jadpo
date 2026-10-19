//! The per-invocation context (docs/compiler-runtime.md §2.5): what an
//! executor is handed, never what it owns. It keeps the shadow stack of
//! frames both executors push, the depth limit, the faults and failures
//! that capture the stack, the recording of a failed `assert`'s parts, and
//! the state of the action boundary the runtime runs ([`crate::action`]):
//! the transaction it opens at the first store operation and the events
//! waiting to be delivered once it commits; the work it leaves for after
//! its response (chunk 6.1a, [`crate::runner`]); who runs it, a request's
//! signed-in caller or the system; and the effects with one source each,
//! the clock and new identities. Reads and writes run through
//! [`crate::store`] against it; integration calls join it with their area.

use std::fmt;

use jadpo_value::Value;

use crate::outcome::{Assertion, AssertionDetail, Part};
use crate::runner::Deferred;
use crate::{
    Env, Event, Failure, Fault, FaultKind, Frame, Host, Isolation, MAX_DEPTH, Places, SqlType,
    SqlValue, StoreError, Transaction,
};

/// The values an `assert`'s condition gave, site by site, while it runs.
struct Recording {
    /// The asserting callable and the depth of its frame: callees run
    /// deeper, and their values aren't the condition's.
    callable: u32,
    depth: usize,
    values: Vec<(u32, Value)>,
    /// How many inline functions it is inside: their values are per item,
    /// not one a report could show.
    paused: u32,
}

/// An action's boundary while its body runs: opened when the outermost
/// action starts, joined by every action it calls (area 6).
#[derive(Default)]
pub(crate) struct Boundary {
    /// The level its transaction begins at: the outermost action's.
    pub isolation: Isolation,
    /// Begun at the first store operation.
    pub transaction: Option<Box<dyn Transaction>>,
    /// The store it was begun in.
    pub store: Option<String>,
    /// What the action emitted, for after it commits.
    pub outbox: Vec<Event>,
    /// Its events' deliveries, once written to the outbox in its
    /// transaction as it commits (chunk 6.1b), for the runner after it
    /// commits.
    pub sealed: Vec<crate::outbox::Delivery>,
    /// How many events it wrote to the outbox as it sealed (chunk 6.2).
    pub kept: u32,
    /// A store operation found another transaction had changed what this
    /// one read, so the body is run again.
    pub concurrent_change: bool,
    /// What the body did to the request's session, for its response once
    /// it commits (area 8).
    pub session: Option<SessionEffect>,
    /// Whether the body issued an API token or a service key, whose
    /// response is never kept for an idempotent repeat (area 8).
    pub issued: bool,
}

/// What an action did to the session of the request a route answers
/// (area 8, "Starting and ending a session"): the cookies its response
/// sets once the action commits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionEffect {
    /// `sessions.start`: the new session's token and CSRF token.
    Start { token: String, csrf: String },
    /// `sessions.end`.
    End,
}

/// The signed-in caller of the request being answered (area 8).
#[derive(Clone, Debug, PartialEq)]
pub struct Caller {
    /// The caller value code receives: a `Principal.user` or
    /// `Principal.service` record.
    pub value: Value,
    /// Whose it is, for rate limits and idempotency keys: `user <id>` or
    /// `service <id>`.
    pub key: String,
    /// The browser session or API token it came with, when it did, by
    /// `id`, which `sessions.end()` ends.
    pub session: Option<[u8; 16]>,
}

/// The system: who a schedule's run and a subscriber's delivery act as
/// (area 9, `Access.system`; area 12), a caller of its own that only the
/// runtime's runner makes ([`crate::runner`]), never a request (chunk
/// 6.1a). Policy lets it in exactly where a grant names `Access.system`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct System {
    /// The tenant it is limited to: the one a subscriber's event came
    /// from. `None` for a schedule's run, which isn't.
    pub tenant: Option<[u8; 16]>,
}

/// An action's transaction and outbox, kept open after its body succeeded
/// for whoever asked the boundary to hold it ([`Ctx::hold`]): the HTTP
/// component, which writes the request's idempotency key into the same
/// transaction and commits it once the route's deadline can no longer
/// interrupt it.
#[derive(Default)]
pub struct Held {
    /// `None` when the body did no store operation.
    pub transaction: Option<Box<dyn Transaction>>,
    /// The deliveries of what it emitted, written to the outbox in
    /// `transaction`, for the runner once it commits (chunk 6.1b).
    pub deliveries: Vec<crate::outbox::Delivery>,
    /// What the body did to the request's session.
    pub session: Option<SessionEffect>,
    /// Whether the body issued an API token or a service key.
    pub issued: bool,
}

impl fmt::Debug for Held {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Held")
            .field("transaction", &self.transaction.is_some())
            .field("deliveries", &self.deliveries)
            .field("session", &self.session)
            .field("issued", &self.issued)
            .finish()
    }
}

/// A subscriber's delivery being run: the handled record its action
/// commits with its writes, in the store it writes (area 12, chunk 6.1b).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Handled {
    pub store: String,
    /// The delivery's `id`.
    pub id: [u8; 16],
}

/// Why an `emit` wasn't kept (area 12, "Limits and runtime faults").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EmitRefused {
    /// No action is running.
    OutsideAction,
    /// The action has emitted [`crate::outbox::MAX_EVENTS`] already.
    TooMany,
    /// The event's payload is past [`crate::outbox::MAX_EVENT_BYTES`].
    TooLarge,
}

/// What happens to the random bytes an invocation is given (area 17).
enum Tape {
    Off,
    /// Kept, in the order they were given.
    Recording(Vec<u8>),
    /// A recorded run's, given back from `at`.
    Replaying {
        bytes: Vec<u8>,
        at: usize,
    },
}

/// One invocation's context.
pub struct Ctx<'p> {
    places: &'p Places,
    host: &'p dyn Host,
    env: Env,
    /// Outermost first.
    frames: Vec<Frame>,
    recording: Option<Recording>,
    pub(crate) boundary: Option<Boundary>,
    /// How many boundaries it has opened.
    pub(crate) actions: u32,
    /// How many identities it has made.
    ids: u64,
    /// The next action to open a boundary of its own holds its commit.
    pub(crate) hold: bool,
    /// What a held boundary left open.
    pub(crate) held: Option<Held>,
    /// The signed-in caller of the request being answered (area 8).
    pub(crate) caller: Option<Caller>,
    /// The system, when the runner runs a schedule's or a subscriber's
    /// work; never alongside a caller.
    pub(crate) system: Option<System>,
    /// What runs after the response, in the order it was left: what
    /// committed actions emitted, and runs the runner was handed (chunk
    /// 6.1a).
    pub(crate) deferred: Vec<Deferred>,
    /// The address the request being answered counts as (area 10, "Client
    /// addresses"); `None` when none is known.
    client: Option<String>,
    /// Whether the statements it runs leave their policy out: only for a
    /// call a test, fixture or seed makes without a caller (area 9).
    pub(crate) unpoliced: bool,
    /// How many random values it has made from its seed.
    draws: u64,
    /// How many trace and span ids it has worked out without the host's
    /// randomness.
    spans: u64,
    /// The random bytes a recorded run keeps, or a replayed run gives back
    /// (area 17, "Replaying a production request").
    tape: Tape,
    /// When the request being answered runs out of time, on the host's
    /// monotonic clock (area 7, "Time limits").
    deadline: Option<u64>,
    /// The settings a test gives its own values (area 13): each by its
    /// number in the plan.
    settings: Vec<(u32, Value)>,
    /// The trace context of the work under way (area 12, question 17):
    /// a request's, or the event's a delivery continues.
    trace: Option<crate::outbox::Trace>,
    /// How many events led to the work under way from the request that
    /// set the chain off: 0 for a request (area 12, "Limits").
    pub(crate) chain: u32,
    /// The subscriber's delivery being run, whose handled record the
    /// outermost action commits.
    pub(crate) handled: Option<Handled>,
    /// How many events the actions it ran kept in the outbox once they
    /// committed, which a schedule's run record counts (area 12,
    /// "Watching schedules").
    pub(crate) emitted: u32,
}

impl fmt::Debug for Ctx<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Ctx")
            .field("env", &self.env)
            .field("frames", &self.frames)
            .field("recording", &self.recording.is_some())
            .field("in_action", &self.boundary.is_some())
            .finish()
    }
}

impl<'p> Ctx<'p> {
    /// A context for one invocation on `host`, under `env`, naming its
    /// frames from `places`. Its stack is empty until the executor enters
    /// the callable the invocation starts at.
    pub fn new(places: &'p Places, host: &'p dyn Host, env: Env) -> Ctx<'p> {
        Ctx {
            places,
            host,
            env,
            frames: Vec::new(),
            recording: None,
            boundary: None,
            actions: 0,
            ids: 0,
            hold: false,
            held: None,
            caller: None,
            system: None,
            deferred: Vec::new(),
            client: None,
            unpoliced: false,
            draws: 0,
            spans: 0,
            tape: Tape::Off,
            deadline: None,
            settings: Vec::new(),
            trace: None,
            chain: 0,
            handled: None,
            emitted: 0,
        }
    }

    /// The trace context of the work under way, when it has one.
    pub fn trace(&self) -> Option<crate::outbox::Trace> {
        self.trace
    }

    /// Sets the trace context of the work under way.
    pub fn set_trace(&mut self, trace: Option<crate::outbox::Trace>) {
        self.trace = trace;
    }

    /// How many events led to the work under way.
    pub fn chain(&self) -> u32 {
        self.chain
    }

    /// Bytes for a new trace or span id: the host's, or worked out from
    /// the seed and the clock, without drawing on what `random` gives, so
    /// tracing never changes a test's values (area 13).
    pub fn trace_bytes(&mut self, out: &mut [u8]) {
        if self.host.random(out) {
            return;
        }
        let mut state = self
            .env
            .seed
            .unwrap_or(0)
            .wrapping_add(self.env.clock.now() as u64)
            .wrapping_add(u64::from(self.chain).wrapping_mul(0x2545_F491_4F6C_DD1D))
            .wrapping_add(self.spans.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        self.spans += 1;
        for chunk in out.chunks_mut(8) {
            state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = state;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^= z >> 31;
            for (to, from) in chunk.iter_mut().zip(z.to_be_bytes()) {
                *to = from;
            }
        }
    }

    /// A new trace, for a request that brings none.
    pub fn new_trace(&mut self) -> crate::outbox::Trace {
        let mut bytes = [0u8; 24];
        self.trace_bytes(&mut bytes);
        crate::outbox::Trace::from_bytes(bytes)
    }

    /// A new span of `trace`, for work it continues.
    pub fn new_span(&mut self, trace: crate::outbox::Trace) -> crate::outbox::Trace {
        let mut bytes = [0u8; 8];
        self.trace_bytes(&mut bytes);
        if bytes == [0; 8] {
            bytes[7] = 1;
        }
        trace.child(bytes)
    }

    /// The signed-in caller of the request being answered, when there is
    /// one (area 8).
    pub fn caller(&self) -> Option<&Caller> {
        self.caller.as_ref()
    }

    /// The address the request being answered counts as: what the default
    /// rate limits count, and what its request line and timeline show.
    pub fn client(&self) -> Option<&str> {
        self.client.as_deref()
    }

    /// Sets the address the request being answered counts as.
    pub fn set_client(&mut self, client: Option<String>) {
        self.client = client;
    }

    /// Sets the request's signed-in caller, for what a route runs; the
    /// policy compiled into each statement applies to it (area 9). Whoever
    /// signed in, or no one, it is never the system.
    pub fn set_caller(&mut self, caller: Option<Caller>) {
        self.caller = caller;
        self.system = None;
        self.unpoliced = false;
    }

    /// Runs what follows with no caller and without policy: a call a test,
    /// fixture or seed makes without passing a caller, which proves
    /// behaviour and data, as a route test or a call with a caller proves
    /// policy (areas 9 and 13). Nothing a request runs does this.
    pub fn set_unpoliced(&mut self) {
        self.caller = None;
        self.system = None;
        self.unpoliced = true;
    }

    /// The system, when what runs is a schedule's or a subscriber's work
    /// (area 12); `None` for everything a request or a test's call runs.
    pub fn system(&self) -> Option<System> {
        self.system
    }

    /// Runs what follows as the system, with no signed-in caller and with
    /// policy: only the runner does this ([`crate::runner`]), so nothing
    /// outside the runtime, a request least of all, can.
    pub(crate) fn become_system(&mut self, system: System) {
        self.caller = None;
        self.system = Some(system);
        self.unpoliced = false;
    }

    /// Who runs what follows, to put back with [`Ctx::restore_runner`]
    /// once the runner's work inside it is done.
    pub(crate) fn runner(&self) -> (Option<Caller>, Option<System>, bool) {
        (self.caller.clone(), self.system, self.unpoliced)
    }

    pub(crate) fn restore_runner(
        &mut self,
        (caller, system, unpoliced): (Option<Caller>, Option<System>, bool),
    ) {
        self.caller = caller;
        self.system = system;
        self.unpoliced = unpoliced;
    }

    /// Leaves `work` to run after the response: the runner runs it, in
    /// the background on a deployable (`Host::defer`), or at once for a
    /// route that waits for it (chunk 6.1a).
    pub fn defer(&mut self, work: Deferred) {
        self.deferred.push(work);
    }

    /// The work left for after the response, in the order it was left.
    pub fn deferred(&self) -> &[Deferred] {
        &self.deferred
    }

    /// Takes the work left for after the response.
    pub fn take_deferred(&mut self) -> Vec<Deferred> {
        std::mem::take(&mut self.deferred)
    }

    /// Whether statements leave their policy out ([`Ctx::set_unpoliced`]).
    pub fn unpoliced(&self) -> bool {
        self.unpoliced
    }

    /// The value of part `part` of the caller, for the policy compiled into
    /// a statement: the signed-in user's or service's `id`, `NULL` for
    /// anyone else, whether anyone is signed in, whether the system runs
    /// it, and the tenant the system is limited to, `NULL` when it isn't.
    pub fn caller_param(&self, part: jadpo_std::policy::CallerPart) -> SqlValue {
        use jadpo_std::policy::CallerPart;
        let held = self.caller.as_ref().and_then(|caller| match &caller.value {
            Value::Record(record) => Some((record.ty.to_string(), record.get("id").cloned()?)),
            _ => None,
        });
        match part {
            CallerPart::System => SqlValue::Bool(self.system.is_some()),
            CallerPart::SystemTenant => match self.system.and_then(|system| system.tenant) {
                Some(tenant) => SqlValue::Uuid(tenant),
                None => SqlValue::Null(SqlType::Uuid),
            },
            CallerPart::Signed => SqlValue::Bool(held.is_some()),
            CallerPart::User | CallerPart::Service => {
                let wanted = if part == CallerPart::User {
                    jadpo_std::http::PrincipalKind::User
                } else {
                    jadpo_std::http::PrincipalKind::Service
                };
                match held {
                    Some((ty, id)) if ty == wanted.type_name() => {
                        crate::to_sql(&id, SqlType::Uuid).unwrap_or(SqlValue::Null(SqlType::Uuid))
                    }
                    _ => SqlValue::Null(SqlType::Uuid),
                }
            }
        }
    }

    /// Fills `out` with random bytes: in a replayed run, the next bytes the
    /// recorded run was given; otherwise the host's, or under a seed (a
    /// test's) from a generator the seed starts, so a test reproduces.
    /// While recording, what it gives is kept. `false` when there is none
    /// to give, a replay that has run out included: never a weaker source.
    /// What `random` gives and the tokens signing in makes come from here
    /// (areas 8 and 19).
    pub fn random_bytes(&mut self, out: &mut [u8]) -> bool {
        if let Tape::Replaying { bytes, at } = &mut self.tape {
            let Some(next) = at
                .checked_add(out.len())
                .and_then(|end| bytes.get(*at..end))
            else {
                return false;
            };
            out.copy_from_slice(next);
            *at += out.len();
            return true;
        }
        if !self.fresh_random(out) {
            return false;
        }
        if let Tape::Recording(kept) = &mut self.tape {
            kept.extend_from_slice(out);
        }
        true
    }

    /// Fills `out` with random bytes a credential is made of (a session's
    /// or an API token's, a service key's, a CSRF token's; area 8): the
    /// host's, or under a seed the seed's, as [`Ctx::random_bytes`] gives,
    /// but never kept on a recording's tape nor taken from a replayed one,
    /// so a recording never holds what would sign someone in (area 17,
    /// chunk 5.5r2). A replayed run makes a fresh credential, and the
    /// draws around it still replay in order.
    pub fn secret_random_bytes(&mut self, out: &mut [u8]) -> bool {
        self.fresh_random(out)
    }

    /// Random bytes no recording gave: the host's, or the seed's.
    fn fresh_random(&mut self, out: &mut [u8]) -> bool {
        if self.host.random(out) {
            return true;
        }
        let Some(seed) = self.env.seed else {
            return false;
        };
        for chunk in out.chunks_mut(8) {
            self.draws += 1;
            let mut z = seed
                .wrapping_add(self.draws.wrapping_mul(0xD1B5_4A32_D192_ED03))
                .wrapping_add(0x9E37_79B9_7F4A_7C15);
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^= z >> 31;
            let bytes = z.to_be_bytes();
            for (to, from) in chunk.iter_mut().zip(bytes) {
                *to = from;
            }
        }
        true
    }

    /// Keeps the random bytes the invocation is given from now on, for
    /// [`Ctx::recorded_random`]: what a recorded run replays (area 17).
    pub fn record_random(&mut self) {
        self.tape = Tape::Recording(Vec::new());
    }

    /// The random bytes kept since [`Ctx::record_random`]; `None` when not
    /// recording. They can hold tokens, so they never leave the process
    /// except as area 17's recording does.
    pub fn recorded_random(&self) -> Option<&[u8]> {
        match &self.tape {
            Tape::Recording(kept) => Some(kept),
            _ => None,
        }
    }

    /// Gives `bytes`, a recorded run's, as the invocation's random bytes,
    /// in order, so it makes the same tokens, codes and shuffles; once
    /// they run out it has no randomness to give (area 17).
    pub fn replay_random(&mut self, bytes: Vec<u8>) {
        self.tape = Tape::Replaying { bytes, at: 0 };
    }

    /// Records what the running action did to the request's session;
    /// `false` outside an action.
    pub fn session(&mut self, effect: SessionEffect) -> bool {
        match &mut self.boundary {
            Some(boundary) => {
                boundary.session = Some(effect);
                true
            }
            None => false,
        }
    }

    /// Records that the running action issued an API token or a service
    /// key; `false` outside an action.
    pub fn issued(&mut self) -> bool {
        match &mut self.boundary {
            Some(boundary) => {
                boundary.issued = true;
                true
            }
            None => false,
        }
    }

    pub fn places(&self) -> &'p Places {
        self.places
    }

    pub fn host(&self) -> &'p dyn Host {
        self.host
    }

    pub fn env(&self) -> &Env {
        &self.env
    }

    /// Moves a test's clock on by `by` milliseconds (area 13, "Controlling
    /// time"): what the next call reads, never one already running. `false`
    /// for a move backwards.
    pub fn advance_clock(&mut self, by: i64) -> bool {
        match self.env.clock.advanced(by) {
            Some(clock) => {
                self.env.clock = clock;
                true
            }
            None => false,
        }
    }

    // --- frames -----------------------------------------------------------

    /// How many frames the stack holds.
    pub fn depth(&self) -> usize {
        self.frames.len()
    }

    /// Enters `callable`, called from `site` of the current frame. Past
    /// [`MAX_DEPTH`] frames it is the `recursion_limit` fault, at that site.
    pub fn enter(&mut self, callable: u32, site: u32) -> Result<(), Box<Fault>> {
        if self.frames.len() >= MAX_DEPTH {
            return Err(self.fault(FaultKind::RecursionLimit, site));
        }
        self.at(site);
        self.frames.push(Frame {
            callable,
            site: Frame::ENTRY,
        });
        Ok(())
    }

    /// Leaves the current callable.
    pub fn leave(&mut self) {
        self.frames.pop();
    }

    /// Moves the current frame to `site`.
    pub fn at(&mut self, site: u32) {
        if let Some(top) = self.frames.last_mut() {
            top.site = site;
        }
    }

    /// The frames, innermost first.
    pub fn frames(&self) -> Vec<Frame> {
        self.frames.iter().rev().copied().collect()
    }

    /// A fault at `site` of the current frame, with the stack.
    pub fn fault(&mut self, kind: FaultKind, site: u32) -> Box<Fault> {
        self.at(site);
        Box::new(Fault {
            kind,
            frames: self.frames(),
        })
    }

    /// `failure`, raised at `site` of the current frame.
    pub fn fail(&mut self, mut failure: Failure, site: u32) -> Box<Failure> {
        self.at(site);
        failure.frames = self.frames();
        Box::new(failure)
    }

    // --- recording an assertion -------------------------------------------

    /// An `assert` starts working out its condition in the current frame:
    /// from here until [`Ctx::assertion_end`], the values its frame's
    /// expressions give are kept for the report if it fails.
    pub fn assertion_begin(&mut self) {
        self.recording = self.frames.last().map(|frame| Recording {
            callable: frame.callable,
            depth: self.frames.len(),
            values: Vec::new(),
            paused: 0,
        });
    }

    /// Whether a value the current frame works out now is one of the
    /// condition's parts.
    pub fn recording(&self) -> bool {
        self.recording
            .as_ref()
            .is_some_and(|recording| recording.paused == 0 && recording.depth == self.frames.len())
    }

    /// Keeps the value site `site` of the current frame gave, when it is
    /// one of the condition's parts.
    pub fn record(&mut self, site: u32, value: &Value) {
        if self.recording()
            && let Some(recording) = &mut self.recording
        {
            recording.values.push((site, value.clone()));
        }
    }

    /// An inline function runs: its values are per item, so none is kept
    /// until [`Ctx::resume_recording`].
    pub fn pause_recording(&mut self) {
        if let Some(recording) = &mut self.recording {
            recording.paused += 1;
        }
    }

    pub fn resume_recording(&mut self) {
        if let Some(recording) = &mut self.recording {
            recording.paused = recording.paused.saturating_sub(1);
        }
    }

    /// The `assert` is done: the parts it recorded, each site once, in the
    /// order their anchors are written.
    pub fn assertion_end(&mut self) -> Vec<Part> {
        let Some(recording) = self.recording.take() else {
            return Vec::new();
        };
        let sites = self
            .places
            .callable(recording.callable)
            .map(|callable| callable.sites.as_slice())
            .unwrap_or_default();
        let mut seen = std::collections::HashSet::new();
        let mut parts: Vec<Part> = Vec::new();
        for (site, value) in recording.values {
            let Some(place) = sites.get(site as usize) else {
                continue;
            };
            if !seen.insert(site) {
                continue;
            }
            parts.push(Part {
                start: place.start,
                end: place.end,
                anchor: place.anchor,
                value,
            });
        }
        parts.sort_by_key(|part| (part.anchor, std::cmp::Reverse(part.end)));
        parts
    }

    /// The `assert` at `site` of the current frame didn't hold.
    pub fn assertion_failed(&mut self, site: u32, detail: AssertionDetail) -> Box<Assertion> {
        self.at(site);
        Box::new(Assertion {
            frames: self.frames(),
            detail,
            parts: Vec::new(),
        })
    }

    // --- the action boundary ----------------------------------------------

    /// How many actions this invocation has run in a boundary of their
    /// own: each action or fixture a test calls opens one, and an action it
    /// calls joins it.
    pub fn actions(&self) -> u32 {
        self.actions
    }

    /// Whether an action is running: its boundary is open.
    pub fn in_action(&self) -> bool {
        self.boundary.is_some()
    }

    /// The level the running action's transaction begins, or began, at;
    /// `None` outside an action.
    pub fn isolation(&self) -> Option<Isolation> {
        self.boundary.as_ref().map(|boundary| boundary.isolation)
    }

    /// Gives the invocation its own values for some settings, by their
    /// numbers in the plan: what a test's `config:` says (area 13), read
    /// instead of the values the application started with.
    pub fn set_settings(&mut self, settings: Vec<(u32, Value)>) {
        self.settings = settings;
    }

    /// The value this invocation gives setting number `at`, if it has its
    /// own.
    pub fn setting(&self, at: u32) -> Option<&Value> {
        self.settings
            .iter()
            .find(|(number, _)| *number == at)
            .map(|(_, value)| value)
    }

    /// `clock.now`: the time the invocation's clock reads, in milliseconds
    /// since 1970-01-01T00:00:00Z. A test's clock is fixed (area 13); the
    /// same source fills generated fields and a query's `clock.now`.
    pub fn now(&self) -> i64 {
        self.env.clock.now()
    }

    /// Sets the request being answered to run out of time `millis` from
    /// now on the host's monotonic clock, or with `None` to have no
    /// deadline. A host without that clock, as a test's, sets none.
    pub fn set_deadline(&mut self, millis: Option<u64>) {
        self.deadline = millis.and_then(|millis| {
            self.host
                .monotonic_ms()
                .map(|now| now.saturating_add(millis))
        });
    }

    /// Whether the invocation answers a request with a deadline: then an
    /// executor gives the thread back as it checks the deadline
    /// ([`crate::yield_now`]), so other requests the thread serves carry on.
    pub fn has_deadline(&self) -> bool {
        self.deadline.is_some()
    }

    /// Whether the request's deadline has passed: what an executor checks
    /// as it runs work that never waits, and the route once its work ends
    /// (area 7, "Time limits").
    pub fn out_of_time(&self) -> bool {
        self.deadline
            .is_some_and(|at| self.host.monotonic_ms().is_some_and(|now| now >= at))
    }

    /// The running action's transaction in `store`, begun at the first
    /// store operation at the action's isolation level (area 6): every
    /// action it calls joins it. Outside an action there is none, and an
    /// action writes one store (area 5), so asking for a second is a
    /// fault.
    pub async fn transaction(&mut self, store: &str) -> Result<&mut dyn Transaction, StoreError> {
        let host = self.host;
        let Some(boundary) = &mut self.boundary else {
            return Err(StoreError::Fault(
                "a store operation outside an action".into(),
            ));
        };
        match &boundary.store {
            Some(open) if open != store => {
                return Err(StoreError::Fault(format!(
                    "an action writes one store, and this one already uses `{open}`, not `{store}`"
                )));
            }
            Some(_) => {}
            None => {
                let Some(database) = host.database(store) else {
                    return Err(StoreError::Fault(format!("no store called `{store}`")));
                };
                boundary.transaction = Some(database.begin_at(boundary.isolation).await?);
                boundary.store = Some(store.to_string());
            }
        }
        match &mut boundary.transaction {
            Some(transaction) => Ok(transaction.as_mut()),
            None => Err(StoreError::Fault("no transaction".into())),
        }
    }

    /// A new identity: a version 7 UUID (area 5), its first 48 bits the
    /// clock's milliseconds and the rest random: in a deployable, from the
    /// host's random bytes; under a seed (a test's), from a generator the
    /// seed starts, so a test that makes records reproduces with its seed.
    /// `None` when there is no seed and the host has no randomness to give.
    pub fn new_id(&mut self) -> Option<[u8; 16]> {
        self.ids += 1;
        let (a, b) = match self.env.seed {
            Some(seed) => {
                let mut state = seed ^ self.ids.wrapping_mul(0x9E37_79B9_7F4A_7C15);
                let mut next = || {
                    state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
                    let mut z = state;
                    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
                    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
                    z ^ (z >> 31)
                };
                (next(), next())
            }
            None => {
                let mut random = [0u8; 16];
                if !self.host.random(&mut random) {
                    return None;
                }
                let [a @ .., _, _, _, _, _, _, _, _] = random;
                let [_, _, _, _, _, _, _, _, b @ ..] = random;
                (u64::from_be_bytes(a), u64::from_be_bytes(b))
            }
        };
        Some(uuid_v7(self.env.clock.now(), a, b))
    }

    /// Queues `event` in the running action's outbox, written to it as
    /// the action commits and delivered once it has (area 12); refused
    /// outside an action, and past the limits of one action's events.
    pub fn emit(&mut self, event: Event) -> Result<(), EmitRefused> {
        let Some(boundary) = &mut self.boundary else {
            return Err(EmitRefused::OutsideAction);
        };
        if boundary.outbox.len() >= crate::outbox::MAX_EVENTS {
            return Err(EmitRefused::TooMany);
        }
        if event.payload.len() > crate::outbox::MAX_EVENT_BYTES {
            return Err(EmitRefused::TooLarge);
        }
        boundary.outbox.push(event);
        Ok(())
    }

    /// Asks the next action that opens a boundary of its own not to commit
    /// when its body succeeds, but to leave its transaction and outbox for
    /// [`Ctx::take_held`]. A body that fails, or runs out of reruns, ends
    /// as it always does.
    pub fn hold(&mut self) {
        self.hold = true;
    }

    /// What the held boundary left open, once; `None` when no action held
    /// one, or it didn't succeed.
    pub fn take_held(&mut self) -> Option<Held> {
        self.hold = false;
        self.held.take()
    }

    /// Gives up whatever is running: an open or held transaction is
    /// dropped without a commit, which aborts it in the store, and the
    /// stack is emptied. What a route does when its deadline passes
    /// (area 7, "Time limits").
    pub fn abandon(&mut self) {
        self.boundary = None;
        self.held = None;
        self.hold = false;
        self.frames.clear();
        self.recording = None;
    }

    /// A store operation found what this action read changed by another
    /// transaction: the boundary runs the whole action again within its
    /// budget (area 6).
    pub fn concurrent_change(&mut self) {
        if let Some(boundary) = &mut self.boundary {
            boundary.concurrent_change = true;
        }
    }
}

/// A version 7 UUID made at `millis` (milliseconds since the epoch), its
/// 12 bits of `rand_a` from `a` and 62 of `rand_b` from `b` (RFC 9562).
fn uuid_v7(millis: i64, a: u64, b: u64) -> [u8; 16] {
    let millis = u64::try_from(millis).unwrap_or(0) & 0xFFFF_FFFF_FFFF;
    let [_, _, m0, m1, m2, m3, m4, m5] = millis.to_be_bytes();
    let [a0, a1] = ((a & 0x0FFF) as u16 | 0x7000).to_be_bytes();
    let [b0, b1, b2, b3, b4, b5, b6, b7] = b.to_be_bytes();
    [
        m0,
        m1,
        m2,
        m3,
        m4,
        m5,
        a0,
        a1,
        (b0 & 0x3F) | 0x80,
        b1,
        b2,
        b3,
        b4,
        b5,
        b6,
        b7,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CallablePlace, MemoryHost, SitePlace};

    fn places() -> Places {
        let mut places = Places::new();
        for (label, line) in [("action place_order", 10), ("function countdown", 20)] {
            places.push(CallablePlace {
                label: label.into(),
                file: "main.jadpo".into(),
                line,
                sites: vec![
                    SitePlace {
                        line: line + 1,
                        start: 5,
                        end: 9,
                        anchor: 7,
                    },
                    SitePlace {
                        line: line + 2,
                        start: 1,
                        end: 9,
                        anchor: 1,
                    },
                ],
            });
        }
        places
    }

    #[test]
    fn faults_carry_the_frames_innermost_first() {
        let places = places();
        let host = MemoryHost::new();
        let mut ctx = Ctx::new(&places, &host, Env::default());
        ctx.enter(0, Frame::ENTRY).expect("room");
        ctx.enter(1, 1).expect("room");
        let fault = ctx.fault(FaultKind::IntOverflow, 0);
        assert_eq!(
            fault.stack(&places),
            [
                "main.jadpo:21  function countdown",
                "main.jadpo:12  action place_order"
            ]
        );
        ctx.leave();
        let failure = ctx.fail(Failure::new("type Gone", "Gone", "gone"), 0);
        assert_eq!(failure.trace(&places), ["action place_order"]);
        assert_eq!(
            failure.site(),
            Some(Frame {
                callable: 0,
                site: 0
            })
        );
    }

    /// A host whose random bytes are all `byte`.
    struct Random(u8);

    impl Host for Random {
        fn database(&self, _store: &str) -> Option<&dyn crate::Database> {
            None
        }

        fn random(&self, bytes: &mut [u8]) -> bool {
            bytes.fill(self.0);
            true
        }
    }

    #[test]
    fn a_new_identity_takes_the_hosts_random_bytes_unless_a_seed_reproduces_it() {
        let places = places();
        let seeded = Env {
            seed: Some(7),
            clock: crate::Clock::fixed(0x0102_0304_0506),
        };
        let random = Random(0xFF);
        let mut ctx = Ctx::new(&places, &random, seeded);
        let first = ctx.new_id().expect("seeded");
        let mut again = Ctx::new(&places, &random, seeded);
        assert_eq!(
            again.new_id(),
            Some(first),
            "a seed reproduces its identities"
        );
        assert_ne!(ctx.new_id(), Some(first), "each identity is new");
        assert_eq!(first[..6], [1, 2, 3, 4, 5, 6], "the clock's milliseconds");
        let live = Env {
            seed: None,
            ..seeded
        };
        let mut ctx = Ctx::new(&places, &random, live);
        assert_eq!(
            crate::uuid_text(&ctx.new_id().expect("random")),
            "01020304-0506-7fff-bfff-ffffffffffff",
            "version 7 and RFC 9562's variant around the host's bytes"
        );
        let none = MemoryHost::new();
        let mut ctx = Ctx::new(&places, &none, live);
        assert_eq!(ctx.new_id(), None, "no seed and no randomness");
    }

    #[test]
    fn random_bytes_follow_the_seed_and_a_recorded_run_replays_them() {
        let places = places();
        let none = MemoryHost::new();
        let seeded = Env {
            seed: Some(7),
            clock: crate::Clock::TEST,
        };
        let draw = |ctx: &mut Ctx<'_>| {
            let mut bytes = [0u8; 12];
            assert!(ctx.random_bytes(&mut bytes));
            bytes
        };
        // A seed gives the same bytes every time, each draw new.
        let mut ctx = Ctx::new(&places, &none, seeded);
        let first = draw(&mut ctx);
        let second = draw(&mut ctx);
        assert_ne!(first, second);
        let mut again = Ctx::new(&places, &none, seeded);
        assert_eq!((draw(&mut again), draw(&mut again)), (first, second));
        // A deployable's run, on the host's generator, recorded...
        let live = Env {
            seed: None,
            ..seeded
        };
        let host = Random(0x5a);
        let mut recorded = Ctx::new(&places, &host, live);
        recorded.record_random();
        let given = (draw(&mut recorded), draw(&mut recorded));
        let tape = recorded.recorded_random().expect("recording").to_vec();
        assert_eq!(tape.len(), 24);
        // ...replays on a host with no generator and no seed, and once
        // the recording runs out there is no randomness, never a weaker one.
        let mut replayed = Ctx::new(&places, &none, live);
        replayed.replay_random(tape);
        assert_eq!((draw(&mut replayed), draw(&mut replayed)), given);
        assert!(!replayed.random_bytes(&mut [0u8; 1]));
        assert!(!Ctx::new(&places, &none, live).random_bytes(&mut [0u8; 1]));
        assert!(format!("{recorded:?}").contains("Ctx"));
        assert!(
            !format!("{recorded:?}").contains("90, 90"),
            "never the bytes"
        );
    }

    #[test]
    fn a_credential_s_bytes_are_never_on_a_recording() {
        let places = places();
        let live = Env {
            seed: None,
            clock: crate::Clock::TEST,
        };
        let host = Random(0x5a);
        let mut recorded = Ctx::new(&places, &host, live);
        recorded.record_random();
        let mut code = [0u8; 4];
        assert!(recorded.random_bytes(&mut code));
        let mut token = [0u8; 32];
        assert!(recorded.secret_random_bytes(&mut token));
        let mut after = [0u8; 4];
        assert!(recorded.random_bytes(&mut after));
        // Only the draws a program makes are kept, in order.
        let tape = recorded.recorded_random().expect("recording").to_vec();
        assert_eq!(tape, [code, after].concat());
        // A replay gives them back in order, and the token's draw takes
        // fresh bytes of its own, never the recording's.
        let other = Random(0x11);
        let mut replayed = Ctx::new(&places, &other, live);
        replayed.replay_random(tape);
        let mut again = [0u8; 4];
        assert!(replayed.random_bytes(&mut again));
        assert_eq!(again, code);
        let mut fresh = [0u8; 32];
        assert!(replayed.secret_random_bytes(&mut fresh));
        assert_ne!(fresh, token);
        assert!(replayed.random_bytes(&mut again));
        assert_eq!(again, after);
    }

    #[test]
    fn the_depth_limit_is_a_fault_at_the_call() {
        let places = places();
        let host = MemoryHost::new();
        let mut ctx = Ctx::new(&places, &host, Env::default());
        ctx.enter(0, Frame::ENTRY).expect("room");
        for _ in 1..MAX_DEPTH {
            ctx.enter(1, 0).expect("room");
        }
        let fault = ctx.enter(1, 1).expect_err("too deep");
        assert_eq!(fault.kind, FaultKind::RecursionLimit);
        assert_eq!(fault.frames.len(), MAX_DEPTH);
        assert_eq!(
            fault.site(),
            Some(Frame {
                callable: 1,
                site: 1
            })
        );
        assert_eq!(fault.kind.classification(), "recursion_limit");
    }

    #[test]
    fn an_assertion_records_its_own_frames_values_once_in_anchor_order() {
        let places = places();
        let host = MemoryHost::new();
        let mut ctx = Ctx::new(&places, &host, Env::default());
        ctx.enter(0, Frame::ENTRY).expect("room");
        ctx.assertion_begin();
        assert!(ctx.recording());
        ctx.record(0, &Value::Int(1));
        ctx.record(0, &Value::Int(2));
        ctx.enter(1, 1).expect("room");
        ctx.record(1, &Value::Int(3));
        ctx.leave();
        ctx.pause_recording();
        ctx.record(1, &Value::Int(4));
        ctx.resume_recording();
        ctx.record(1, &Value::Int(5));
        let parts: Vec<(u32, Value)> = ctx
            .assertion_end()
            .into_iter()
            .map(|part| (part.anchor, part.value))
            .collect();
        assert_eq!(parts, [(1, Value::Int(5)), (7, Value::Int(1))]);
        assert!(!ctx.recording());
    }
}
