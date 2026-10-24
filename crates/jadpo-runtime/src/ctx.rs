//! The per-invocation context (docs/compiler-runtime.md §2.5): what an
//! executor is handed, never what it owns. It keeps the shadow stack of
//! frames both executors push, the depth limit, the faults and failures
//! that capture the stack, the recording of a failed `assert`'s parts, the
//! action boundary the runtime runs ([`crate::action`]), who runs it (a
//! request's signed-in caller, or no one under a test), and the clock.
//!
//! Two parts of it are structs of their own, each reached through
//! methods, never written field by field from elsewhere (chunk 7.0b): its
//! background-run state ([`Background`]: the system, the delivery being
//! handled, the chain and trace, the work left for after the response and
//! the events kept), which the runner sets; and its capabilities
//! ([`Capabilities`]: the host's stores, passkeys and random bytes, with
//! the identities made from them, what a recording keeps, and the log of
//! the calls that left the process through the capability seam, chunk
//! 7.0c). Reads and writes run through [`crate::store`] against it, and
//! operations with effects, outside calls among them, through
//! [`crate::effects`].

use std::fmt;

use jadpo_value::Value;

use crate::boundary::Boundary;
use crate::outcome::{Assertion, AssertionDetail, Part};
use crate::runner::Deferred;
use crate::{
    Env, Event, Failure, Fault, FaultKind, Frame, Host, Isolation, MAX_DEPTH, Places, SqlType,
    SqlValue, StoreError, Transaction,
};

mod background;
mod capabilities;

pub(crate) use background::{Background, Handled};
pub use capabilities::Capabilities;
pub(crate) use capabilities::random_id;

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

/// Why a sign-in was refused (area 8, "Events"; chunk 7.3b4), which the
/// log keeps while the person sees only the refusal's own failure: a
/// `SignInRefusal`'s name, and for `refused_by_app` the failure the
/// application's action refused with. Never an address, a link or a code.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignInRefused {
    pub reason: &'static str,
    pub failure: Option<String>,
}

/// What an action did to the cookie binding a `same_browser:` magic link
/// to the browser that asked for it (area 8, "Writing the routes
/// yourself"; chunk 7.3b3): the response of the route whose action calls
/// `magic_links.request` sets it, and one that signs in with the link
/// clears it, as the generated routes do.
#[derive(Clone, PartialEq, Eq)]
pub enum Binding {
    /// Set: its token, and how long it lasts in seconds.
    Set { token: String, max_age: i64 },
    /// Cleared.
    Clear,
}

impl fmt::Debug for Binding {
    /// Never the token.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Binding::Set { max_age, .. } => f
                .debug_struct("Set")
                .field("max_age", max_age)
                .finish_non_exhaustive(),
            Binding::Clear => f.write_str("Clear"),
        }
    }
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
    /// from, or a batch's saved tenant id names. `None` for a schedule's
    /// run, which isn't.
    pub tenant: Option<[u8; 16]>,
}

impl System {
    /// The system limited to no tenant, reaching only the records every
    /// tenant shares: a webhook's reply action and `on_first_sign_in:`'s,
    /// which run inside a request no tenant's event starts (area 9,
    /// "Background work"; chunk 7.9b). Never `None`, which is every
    /// tenant, a schedule's.
    pub const NO_TENANT: System = System {
        tenant: Some(crate::outbox::NO_TENANT),
    };
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
    /// How many events it kept, counted once it commits (a schedule's run
    /// record keeps the count; chunk 6.7).
    pub kept: u32,
    /// What the body did to the request's session.
    pub session: Option<SessionEffect>,
    /// Whether the body issued an API token or a service key.
    pub issued: bool,
    /// What the body did to a `same_browser:` magic link's cookie.
    pub binding: Option<Binding>,
}

impl fmt::Debug for Held {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Held")
            .field("transaction", &self.transaction.is_some())
            .field("deliveries", &self.deliveries)
            .field("session", &self.session)
            .field("issued", &self.issued)
            .field("binding", &self.binding)
            .finish()
    }
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

/// One invocation's context.
pub struct Ctx<'p> {
    places: &'p Places,
    env: Env,
    /// Outermost first.
    frames: Vec<Frame>,
    recording: Option<Recording>,
    /// The running action's boundary, while one is open.
    boundary: Option<Boundary>,
    /// How many boundaries it has opened.
    actions: u32,
    /// The next action to open a boundary of its own holds its commit.
    hold: bool,
    /// What a held boundary left open.
    held: Option<Held>,
    /// The signed-in caller of the request being answered (area 8).
    caller: Option<Caller>,
    /// The address the request being answered counts as (area 10, "Client
    /// addresses"); `None` when none is known.
    client: Option<String>,
    /// The cookie binding a `same_browser:` magic link to this browser,
    /// as the request being answered sent it (area 8; chunk 7.3b3).
    sign_in_cookie: Option<String>,
    /// Why a sign-in was refused while answering the request, for the
    /// host's log line (area 8, "Events"; chunk 7.3b4).
    sign_in_refused: Option<SignInRefused>,
    /// Whether the statements it runs leave their policy out: only for a
    /// call a test, fixture or seed makes without a caller (area 9).
    unpoliced: bool,
    /// When the request being answered runs out of time, on the host's
    /// monotonic clock (area 7, "Time limits").
    deadline: Option<u64>,
    /// How many inline functions are running, one inside another's body
    /// ([`Ctx::inline_begin`]).
    inline: usize,
    /// The settings a test gives its own values (area 13): each by its
    /// number in the plan.
    settings: Vec<(u32, Value)>,
    /// What the program running in it started with (chunk 7.9e): its
    /// settings' values, keys, credentials, sign-in settings, how a
    /// request's client address is chosen and its purges' periods. `None`
    /// until the runtime gives it ([`Ctx::of`], [`Ctx::start`]).
    started: Option<std::rc::Rc<crate::Started>>,
    /// What background work carries while it runs.
    background: Background,
    /// What it reaches outside itself, through its host.
    capabilities: Capabilities<'p>,
    /// The scope and key a provider call's idempotency key is derived
    /// from (area 12, "Keys"; chunk 7.2c): the caller's scope and the
    /// client's `Idempotency-Key` or the request's id in a route, the
    /// delivery's id in a subscriber; one made from the invocation's
    /// randomness otherwise.
    call_key: Option<CallKey>,
    /// How many keyed provider calls its actions have made: each call's
    /// step, which its key is derived from (chunk 7.2c).
    steps: u32,
    /// The id of the request being answered, which the development call
    /// log files each provider call under (chunk 7.2d).
    request_id: Option<String>,
    /// Whether its keyed provider calls count their steps outside an
    /// action: only `jadpo package record`'s one call (chunk 7.2d2).
    standalone_calls: bool,
    /// The generation of the runner that runs it where its key survives a
    /// retry: a delivery's attempt, a route's claim (area 12, "One runner
    /// per record"; chunk 7.5b). `None` for anything else, which keeps no
    /// record of its steps.
    resumable: Option<i64>,
    /// The record of the steps of the action its outermost boundary runs,
    /// while that is open (chunk 7.5b).
    record: Option<crate::steps::Record>,
    /// The connections the package actions it is running were called
    /// through, innermost last (area 11, "What package code may do";
    /// chunk 7.6): a package action's own operations go through it.
    through: Vec<String>,
    /// The steps it reached under a record, and the crash `jadpo test`
    /// asked for, if any (chunk 7.5d).
    #[cfg(feature = "testing")]
    crashes: crate::crashes::Crashes,
    /// Each `ai.generate` call's answer, by the step it took and the
    /// request it sent, given back to an action run again after a
    /// concurrent change (area 21; chunk 5.5v3).
    reused: Vec<crate::ai::Reused>,
    /// The steps each record it ran reached, as the record closed: what
    /// `jadpo dev` shows under a request or a delivery (chunk 7.5f).
    shown: Vec<crate::steps::RunSteps>,
    /// Each `ai.stream` call's answer, open and waiting to be sent by the
    /// route its action answers, by the step it took and the request it
    /// sent (area 21; chunk 5.5v6): what an `AiStream` value names.
    streams: Vec<crate::stream::OpenStream>,
    /// The tokens the `ai` calls of the request being answered used (area
    /// 21, "Budgets"; chunk 5.5v5), with the action that request is: each
    /// action a test calls is a request of its own.
    ai_spent: (u32, u64),
}

/// What a provider call's idempotency key is derived from besides the
/// application, the connection and the step (area 12, "Keys").
#[derive(Clone, PartialEq, Eq)]
pub struct CallKey {
    /// Whose it is: a route's caller's idempotency scope, `delivery`.
    pub scope: String,
    /// The action's key: the client's `Idempotency-Key`, the request's id,
    /// the delivery's id.
    pub key: String,
}

impl fmt::Debug for CallKey {
    /// Never the key, which a client sent.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CallKey")
            .field("scope", &self.scope)
            .finish_non_exhaustive()
    }
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
            env,
            frames: Vec::new(),
            recording: None,
            boundary: None,
            actions: 0,
            hold: false,
            held: None,
            caller: None,
            client: None,
            sign_in_cookie: None,
            sign_in_refused: None,
            unpoliced: false,
            deadline: None,
            inline: 0,
            settings: Vec::new(),
            started: None,
            background: Background::default(),
            capabilities: Capabilities::new(host, env.seed),
            call_key: None,
            steps: 0,
            request_id: None,
            standalone_calls: false,
            resumable: None,
            record: None,
            through: Vec::new(),
            #[cfg(feature = "testing")]
            crashes: crate::crashes::Crashes::default(),
            reused: Vec::new(),
            shown: Vec::new(),
            streams: Vec::new(),
            ai_spent: (0, 0),
        }
    }

    /// A context for one invocation of `executor`'s program on `host`,
    /// under `env`: its frames named from the program's places, and what
    /// the program started with given to it (chunk 7.9e). What the
    /// runtime makes a context with for work of its own: a request, a
    /// delivery, a tick.
    pub fn of(executor: &'p dyn crate::Executor, host: &'p dyn Host, env: Env) -> Ctx<'p> {
        let mut ctx = Ctx::new(executor.places(), host, env);
        if let Some(started) = executor.started() {
            ctx.start(started);
        }
        ctx
    }

    /// Gives it what the program running in it started with: its
    /// settings' values and what is held apart from them (chunk 7.9e).
    pub fn start(&mut self, started: std::rc::Rc<crate::Started>) {
        self.started = Some(started);
    }

    /// Whether it has been given what its program started with.
    pub fn is_started(&self) -> bool {
        self.started.is_some()
    }

    /// What the program running in it started with (chunk 7.9e): nothing,
    /// no setting with a value, until it is given that.
    pub fn started(&self) -> std::rc::Rc<crate::Started> {
        match &self.started {
            Some(started) => std::rc::Rc::clone(started),
            None => crate::started::unstarted(),
        }
    }

    #[cfg(feature = "testing")]
    pub(crate) fn crashes(&self) -> &crate::crashes::Crashes {
        &self.crashes
    }

    #[cfg(feature = "testing")]
    pub(crate) fn crashes_mut(&mut self) -> &mut crate::crashes::Crashes {
        &mut self.crashes
    }

    /// Runs what follows where its key survives a retry, for the runner
    /// whose generation is `generation` (a delivery's attempt, a route's
    /// claim), or with `None` where it doesn't: an action with several
    /// changing calls keeps a record of its steps only there (area 12;
    /// chunk 7.5b).
    pub fn set_resumable(&mut self, generation: Option<i64>) {
        self.resumable = generation;
    }

    /// The generation of the runner that runs it where its key survives a
    /// retry.
    pub fn resumable(&self) -> Option<i64> {
        self.resumable
    }

    /// The record of the running action's steps, while one is kept.
    pub(crate) fn steps_record(&self) -> Option<&crate::steps::Record> {
        self.record.as_ref()
    }

    pub(crate) fn steps_record_mut(&mut self) -> Option<&mut crate::steps::Record> {
        self.record.as_mut()
    }

    pub(crate) fn set_steps_record(&mut self, record: Option<crate::steps::Record>) {
        self.record = record;
    }

    pub(crate) fn take_steps_record(&mut self) -> Option<crate::steps::Record> {
        self.record.take()
    }

    /// Where the running action's record has got to: the step it reached
    /// last, with that step's key generation (chunk 7.5b).
    pub(crate) fn steps_at(&self) -> Option<crate::steps::StepAt> {
        self.record.as_ref().and_then(crate::steps::Record::reached)
    }

    /// Stops the invocation's clock at `clock`: a record's first run's
    /// start, which `clock.now` reads for every rerun (chunk 7.5b).
    pub(crate) fn set_clock(&mut self, clock: crate::Clock) {
        self.env.clock = clock;
    }

    /// A package action called through `connection` starts (chunk 7.6).
    pub fn enter_package(&mut self, connection: &str) {
        self.through.push(connection.to_string());
    }

    /// The package action [`Ctx::enter_package`] started has ended.
    pub fn leave_package(&mut self) {
        self.through.pop();
    }

    /// The connection the package action running was called through, which
    /// its own operations go through; `None` outside one.
    pub fn package_connection(&self) -> Option<&str> {
        self.through.last().map(String::as_str)
    }

    /// Gives the invocation the id of the request it answers (chunk 7.2d).
    pub fn set_request_id(&mut self, id: impl Into<String>) {
        self.request_id = Some(id.into());
    }

    /// The id of the request it answers, when it answers one.
    pub fn request_id(&self) -> Option<&str> {
        self.request_id.as_deref()
    }

    /// Gives the invocation the scope and key its provider calls' keys are
    /// derived from (area 12, "Keys"; chunk 7.2c).
    pub fn set_call_key(&mut self, scope: impl Into<String>, key: impl Into<String>) {
        self.call_key = Some(CallKey {
            scope: scope.into(),
            key: key.into(),
        });
    }

    /// Puts `key` in place of the scope and key its provider calls' keys
    /// are derived from, giving back what it had: a test's delivery runs
    /// under its own and gives the test's back (chunk 7.5d).
    pub(crate) fn swap_call_key(&mut self, key: Option<CallKey>) -> Option<CallKey> {
        std::mem::replace(&mut self.call_key, key)
    }

    /// The scope and key its provider calls' keys are derived from: the
    /// one it was given, or one made from its randomness the first time
    /// it is asked, kept for the rest of the invocation so an action run
    /// again sends the same keys; `None` with no randomness to make one.
    pub fn call_key(&mut self) -> Option<CallKey> {
        if self.call_key.is_none() {
            let id = self.new_id()?;
            self.call_key = Some(CallKey {
                scope: "invocation".to_string(),
                key: jadpo_value::encoding::to_hex(&id),
            });
        }
        self.call_key.clone()
    }

    /// The running action's next keyed call's step, from 1 (chunk 7.2c),
    /// counted across the invocation, so two actions it runs never send
    /// one key; `None` outside an action.
    pub fn next_step(&mut self) -> Option<u32> {
        if !self.standalone_calls {
            self.boundary.as_ref()?;
        }
        self.steps = self.steps.saturating_add(1);
        Some(self.steps)
    }

    /// Lets a provider call it makes outside an action take a key, its
    /// step counted as an action's would be: `jadpo package record`'s one
    /// call to a sandbox, which no action makes (chunk 7.2d2).
    pub fn allow_standalone_calls(&mut self) {
        self.standalone_calls = true;
    }

    /// The answer the `ai.generate` call at step `step` was given when it
    /// sent the request digested as `digest`, on an earlier run of its
    /// action (chunk 5.5v3).
    pub(crate) fn reused(&self, step: u32, digest: &[u8; 32]) -> Option<crate::ai::Kept> {
        self.reused
            .iter()
            .find(|kept| kept.step == step && kept.digest == *digest)
            .map(|kept| kept.kept.clone())
    }

    /// Keeps an `ai.generate` call's answer for a rerun of its action,
    /// in place of any its step had.
    pub(crate) fn keep_reused(&mut self, reused: crate::ai::Reused) {
        self.reused.retain(|kept| kept.step != reused.step);
        self.reused.push(reused);
    }

    /// Puts `reused` in place of the `ai.generate` answers kept for a
    /// rerun, giving back what it had: a delivery's attempt runs with none
    /// of whoever ran it, as a runner's fresh context does (chunk 7.9a).
    pub(crate) fn swap_reused(&mut self, reused: Vec<crate::ai::Reused>) -> Vec<crate::ai::Reused> {
        std::mem::replace(&mut self.reused, reused)
    }

    /// Forgets the `ai.generate` answers kept for a rerun: a request sent
    /// again after a crash `jadpo test` made goes, as a client's retry
    /// does, to a runner that kept none (chunk 7.9a).
    pub(crate) fn forget_reused(&mut self) {
        self.reused.clear();
    }

    /// The open stream an `ai.stream` call at step `step` gave when it
    /// sent the request digested as `digest`, on an earlier run of its
    /// action (chunk 5.5v6): its number, while it is still open.
    pub(crate) fn reused_stream(&self, step: u32, digest: &[u8; 32]) -> Option<u32> {
        self.streams
            .iter()
            .position(|slot| slot.open() && slot.step == Some(step) && slot.digest == *digest)
            .and_then(|at| u32::try_from(at).ok())
    }

    /// Keeps an open stream for the route its action answers, and gives
    /// its number.
    pub(crate) fn keep_stream(&mut self, slot: crate::stream::OpenStream) -> u32 {
        self.streams.push(slot);
        u32::try_from(self.streams.len() - 1).unwrap_or(u32::MAX)
    }

    /// The open stream numbered `number`, taken to be sent.
    pub(crate) fn take_stream(&mut self, number: u32) -> Option<Box<dyn crate::stream::Events>> {
        self.streams.get_mut(number as usize)?.take()
    }

    /// How many keyed calls it has made.
    pub(crate) fn steps(&self) -> u32 {
        self.steps
    }

    /// Counts its keyed calls from `steps` again: an action run again
    /// after a concurrent change takes the steps it took before.
    pub(crate) fn set_steps(&mut self, steps: u32) {
        self.steps = steps;
    }

    /// Its background-run state, for the runner and the boundary.
    pub(crate) fn background(&self) -> &Background {
        &self.background
    }

    pub(crate) fn background_mut(&mut self) -> &mut Background {
        &mut self.background
    }

    /// What it reaches outside itself, through its host: its stores, its
    /// passkey ceremonies, its random bytes and the calls it made.
    pub fn capabilities(&self) -> &Capabilities<'p> {
        &self.capabilities
    }

    pub(crate) fn capabilities_mut(&mut self) -> &mut Capabilities<'p> {
        &mut self.capabilities
    }

    /// Takes the log of the calls it made through the capability seam
    /// (chunk 7.0c), leaving it empty: what a request's response carries
    /// for `jadpo calls`.
    pub fn take_calls(&mut self) -> Vec<crate::capability::Logged> {
        self.capabilities.take_calls()
    }

    /// Keeps the steps a record's run reached as it closes (chunk 7.5f).
    pub(crate) fn push_steps(&mut self, steps: crate::steps::RunSteps) {
        self.shown.push(steps);
    }

    /// How many records' steps it has kept so far.
    pub(crate) fn steps_kept(&self) -> usize {
        self.shown.len()
    }

    /// Takes the steps the records it ran reached from the `from`th on,
    /// leaving those before: what a response, or a delivery's line in
    /// `jadpo dev`, shows (chunk 7.5f).
    pub fn take_steps(&mut self, from: usize) -> Vec<crate::steps::RunSteps> {
        if from >= self.shown.len() {
            return Vec::new();
        }
        self.shown.split_off(from)
    }

    /// The trace context of the work under way, when it has one.
    pub fn trace(&self) -> Option<crate::outbox::Trace> {
        self.background.trace()
    }

    /// Sets the trace context of the work under way.
    pub fn set_trace(&mut self, trace: Option<crate::outbox::Trace>) {
        self.background.set_trace(trace);
    }

    /// How many events led to the work under way.
    pub fn chain(&self) -> u32 {
        self.background.chain()
    }

    /// Bytes for a new trace or span id: the host's, or worked out from
    /// the seed and the clock, without drawing on what `random` gives, so
    /// tracing never changes a test's values (area 13).
    pub fn trace_bytes(&mut self, out: &mut [u8]) {
        let (now, chain) = (self.env.clock.now(), self.background.chain());
        self.capabilities.trace_bytes(now, chain, out);
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

    /// The cookie binding a `same_browser:` magic link to the browser the
    /// request being answered came from, when it sent one.
    pub fn sign_in_cookie(&self) -> Option<&str> {
        self.sign_in_cookie.as_deref()
    }

    /// Sets the cookie binding a magic link to the request's browser.
    pub fn set_sign_in_cookie(&mut self, cookie: Option<String>) {
        self.sign_in_cookie = cookie;
    }

    /// Keeps why a sign-in was refused, for the request's log line.
    pub fn refuse_sign_in(&mut self, refused: SignInRefused) {
        self.sign_in_refused = Some(refused);
    }

    /// Why a sign-in was refused while answering the request, once.
    pub fn take_sign_in_refused(&mut self) -> Option<SignInRefused> {
        self.sign_in_refused.take()
    }

    /// Records what the running action did to a magic link's binding
    /// cookie; `false` outside an action.
    pub fn binding(&mut self, binding: Binding) -> bool {
        match &mut self.boundary {
            Some(boundary) => {
                boundary.set_binding(binding);
                true
            }
            None => false,
        }
    }

    /// Whether the running action has asked for a magic link, whose row
    /// its transaction holds until it ends.
    pub fn asked_sign_in(&self) -> bool {
        self.boundary
            .as_ref()
            .is_some_and(|boundary| boundary.asked_sign_in())
    }

    /// Records that the running action asked for a magic link.
    pub fn ask_sign_in(&mut self) {
        if let Some(boundary) = &mut self.boundary {
            boundary.ask_sign_in();
        }
    }

    /// Takes the running action's boundary away, for work that must run
    /// in a transaction of its own while it waits, such as
    /// `on_first_sign_in:`'s action (area 8); [`Ctx::give_boundary`] puts
    /// it back.
    pub(crate) fn take_boundary(&mut self) -> Option<Boundary> {
        self.boundary.take()
    }

    pub(crate) fn give_boundary(&mut self, boundary: Option<Boundary>) {
        self.boundary = boundary;
    }

    /// Sets the request's signed-in caller, for what a route runs; the
    /// policy compiled into each statement applies to it (area 9). Whoever
    /// signed in, or no one, it is never the system.
    pub fn set_caller(&mut self, caller: Option<Caller>) {
        self.caller = caller;
        self.background.set_system(None);
        self.unpoliced = false;
    }

    /// Runs what follows with no caller and without policy: a call a test,
    /// fixture or seed makes without passing a caller, which proves
    /// behaviour and data, as a route test or a call with a caller proves
    /// policy (areas 9 and 13). Nothing a request runs does this.
    pub fn set_unpoliced(&mut self) {
        self.caller = None;
        self.background.set_system(None);
        self.unpoliced = true;
    }

    /// The system, when what runs is a schedule's or a subscriber's work
    /// (area 12); `None` for everything a request or a test's call runs.
    pub fn system(&self) -> Option<System> {
        self.background.system()
    }

    /// Runs what follows as the system, with no signed-in caller and with
    /// policy: only the runner does this ([`crate::runner`]), so nothing
    /// outside the runtime, a request least of all, can.
    pub(crate) fn become_system(&mut self, system: System) {
        self.caller = None;
        self.background.set_system(Some(system));
        self.unpoliced = false;
    }

    /// Who runs what follows, to put back with [`Ctx::restore_runner`]
    /// once the runner's work inside it is done.
    pub(crate) fn runner(&self) -> (Option<Caller>, Option<System>, bool) {
        (
            self.caller.clone(),
            self.background.system(),
            self.unpoliced,
        )
    }

    pub(crate) fn restore_runner(
        &mut self,
        (caller, system, unpoliced): (Option<Caller>, Option<System>, bool),
    ) {
        self.caller = caller;
        self.background.set_system(system);
        self.unpoliced = unpoliced;
    }

    /// Leaves `work` to run after the response: the runner runs it, in
    /// the background on a deployable (`Host::defer`), or at once for a
    /// route that waits for it (chunk 6.1a).
    pub fn defer(&mut self, work: Deferred) {
        self.background.defer(work);
    }

    /// The work left for after the response, in the order it was left.
    pub fn deferred(&self) -> &[Deferred] {
        self.background.deferred()
    }

    /// Takes the work left for after the response.
    pub fn take_deferred(&mut self) -> Vec<Deferred> {
        self.background.take_deferred()
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
            CallerPart::System => SqlValue::Bool(self.background.system().is_some()),
            CallerPart::SystemTenant => {
                match self.background.system().and_then(|system| system.tenant) {
                    Some(tenant) => SqlValue::Uuid(tenant),
                    None => SqlValue::Null(SqlType::Uuid),
                }
            }
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
        self.capabilities.random_bytes(out)
    }

    /// Fills `out` with random bytes a credential is made of (a session's
    /// or an API token's, a service key's, a CSRF token's; area 8): the
    /// host's, or under a seed the seed's, as [`Ctx::random_bytes`] gives,
    /// but never kept on a recording's tape nor taken from a replayed one,
    /// so a recording never holds what would sign someone in (area 17,
    /// chunk 5.5r2). A replayed run makes a fresh credential, and the
    /// draws around it still replay in order.
    pub fn secret_random_bytes(&mut self, out: &mut [u8]) -> bool {
        self.capabilities.secret_random_bytes(out)
    }

    /// Keeps the random bytes the invocation is given and its outside
    /// calls' answers from now on, for [`Ctx::recorded_run`]: what a recorded
    /// run replays (area 17).
    pub fn record_run(&mut self) {
        self.capabilities.record();
    }

    /// What was kept since [`Ctx::record_run`]; `None` when not recording. It
    /// can hold tokens and personal data, so it never leaves the process
    /// except as area 17's recording does.
    pub fn recorded_run(&self) -> Option<&crate::capability::Recording> {
        self.capabilities.recorded()
    }

    /// Gives `recording`, a recorded run's, as the invocation's random
    /// bytes and its outside calls' answers, in order, so it makes the
    /// same tokens, codes and shuffles and no call leaves the process;
    /// once the bytes run out it has no randomness to give, and a call the
    /// recording doesn't have next is refused (area 17).
    pub fn replay_run(&mut self, recording: crate::capability::Recording) {
        self.capabilities.replay(recording);
    }

    /// Records what the running action did to the request's session;
    /// `false` outside an action.
    pub fn session(&mut self, effect: SessionEffect) -> bool {
        match &mut self.boundary {
            Some(boundary) => {
                boundary.set_session(effect);
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
                boundary.set_issued();
                true
            }
            None => false,
        }
    }

    pub fn places(&self) -> &'p Places {
        self.places
    }

    pub fn host(&self) -> &'p dyn Host {
        self.capabilities.host()
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
    /// [`MAX_DEPTH`] frames it is the `recursion_limit` fault, at that site,
    /// and in a WebAssembly module so it is with less than
    /// [`crate::WASM_STACK_RESERVE`] of its own stack left, so frames too
    /// large for it stop there rather than write past its end.
    pub fn enter(&mut self, callable: u32, site: u32) -> Result<(), Box<Fault>> {
        if self.frames.len() >= MAX_DEPTH || stack_low() {
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

    /// An operation is about to run an inline function: `false`, and
    /// nothing counted, when [`crate::MAX_INLINE_DEPTH`] are running
    /// already, which the operation's site raises as the `recursion_limit`
    /// fault; otherwise it counts until [`Ctx::inline_end`].
    pub fn inline_begin(&mut self) -> bool {
        if self.inline >= crate::MAX_INLINE_DEPTH {
            return false;
        }
        self.inline += 1;
        true
    }

    /// The operation [`Ctx::inline_begin`] let run its inline function has
    /// ended.
    pub fn inline_end(&mut self) {
        self.inline = self.inline.saturating_sub(1);
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

    /// The tokens the `ai` calls of the request being answered have used.
    pub(crate) fn ai_spent(&self) -> u64 {
        if self.ai_spent.0 == self.actions {
            self.ai_spent.1
        } else {
            0
        }
    }

    /// Counts `tokens` against the request being answered.
    pub(crate) fn add_ai_spent(&mut self, tokens: u64) {
        if self.ai_spent.0 != self.actions {
            self.ai_spent = (self.actions, 0);
        }
        self.ai_spent.1 = self.ai_spent.1.saturating_add(tokens);
    }

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

    /// Whether a standalone read's boundary can end at its main statement.
    /// Check before opening it: `own_boundary` consumes the pending hold.
    pub(crate) fn can_end_read(&self) -> bool {
        !self.in_action()
            && !self.hold
            && self.held.is_none()
            && self.background.system().is_none()
            && self.background.handled().is_none()
            && self.resumable.is_none()
            && self.record.is_none()
            && self.steps == 0
            && !self.standalone_calls
    }

    /// Whether the running action has a transaction open: it has read or
    /// written since its boundary opened, or since an outside call closed
    /// what it had only read. No outside call goes out then (area 6,
    /// "Outside calls hold no transaction"; chunks 7.0c and 7.4).
    pub fn transaction_open(&self) -> bool {
        self.boundary
            .as_ref()
            .is_some_and(Boundary::has_transaction)
    }

    /// The level the running action's transaction begins, or began, at;
    /// `None` outside an action.
    pub fn isolation(&self) -> Option<Isolation> {
        self.boundary.as_ref().map(Boundary::isolation)
    }

    /// The running action's boundary, while one is open.
    pub(crate) fn boundary(&self) -> Option<&Boundary> {
        self.boundary.as_ref()
    }

    pub(crate) fn boundary_mut(&mut self) -> Option<&mut Boundary> {
        self.boundary.as_mut()
    }

    /// Opens a boundary whose transaction begins at `isolation`.
    pub(crate) fn open_boundary(&mut self, isolation: Isolation) {
        self.boundary = Some(Boundary::new(isolation));
    }

    /// Closes the running action's boundary, giving back what it holds.
    pub(crate) fn close_boundary(&mut self) -> Boundary {
        self.boundary.take().unwrap_or_default()
    }

    /// Counts an action that opens a boundary of its own, and says whether
    /// it is to hold its commit ([`Ctx::hold`]), which only it is.
    pub(crate) fn own_boundary(&mut self) -> bool {
        self.actions += 1;
        std::mem::take(&mut self.hold)
    }

    /// Keeps what a held boundary left open, for [`Ctx::take_held`].
    pub(crate) fn keep_held(&mut self, held: Held) {
        self.held = Some(held);
    }

    /// Gives the invocation its own values for some settings, by their
    /// numbers in the plan: what a test's `config:` says (area 13), read
    /// instead of the values the application started with.
    pub fn set_settings(&mut self, settings: Vec<(u32, Value)>) {
        self.settings = settings;
    }

    /// The value setting number `at` has in this invocation, read one way
    /// wherever a setting is read (chunk 7.9e): its own, a test's
    /// `config:` (area 13), else the one its program started with. `None`
    /// when neither gives one, and for a secret, which code never reads.
    pub fn setting(&self, at: u32) -> Option<&Value> {
        self.settings
            .iter()
            .find(|(number, _)| *number == at)
            .map(|(_, value)| value)
            .or_else(|| self.started.as_deref()?.setting(at))
    }

    /// The values of the settings numbered `numbers` in this invocation,
    /// each read as [`Ctx::setting`] reads it: what a call's site hands the
    /// part of the runtime that makes its request, its address's and its
    /// model's settings among them. One no value reached is left out.
    pub fn settings_of(&self, numbers: impl IntoIterator<Item = u32>) -> Vec<(u32, Value)> {
        numbers
            .into_iter()
            .filter_map(|at| Some((at, self.setting(at)?.clone())))
            .collect()
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
        let host = self.capabilities.host();
        self.deadline =
            millis.and_then(|millis| host.monotonic_ms().map(|now| now.saturating_add(millis)));
    }

    /// Whether the invocation answers a request with a deadline: then an
    /// executor gives the thread back as it checks the deadline
    /// ([`crate::yield_now`]), so other requests the thread serves carry on.
    pub fn has_deadline(&self) -> bool {
        self.deadline.is_some()
    }

    /// When, on the host's monotonic clock, the invocation's time is up:
    /// the request's deadline, or what background work's budget leaves it
    /// ([`Ctx::set_deadline`]); `None` with none. A call never waits to be
    /// sent again past it (chunk 7.9d).
    pub(crate) fn deadline_at(&self) -> Option<u64> {
        self.deadline
    }

    /// Whether the request's deadline has passed: what an executor checks
    /// as it runs work that never waits, and the route once its work ends
    /// (area 7, "Time limits").
    pub fn out_of_time(&self) -> bool {
        let host = self.capabilities.host();
        self.deadline
            .is_some_and(|at| host.monotonic_ms().is_some_and(|now| now >= at))
    }

    /// The running action's transaction in `store`, begun at the first
    /// store operation at the action's isolation level (area 6): every
    /// action it calls joins it. Outside an action there is none, and an
    /// action writes one store (area 5), so asking for a second is a
    /// fault.
    pub async fn transaction(&mut self, store: &str) -> Result<&mut dyn Transaction, StoreError> {
        let Some(boundary) = &mut self.boundary else {
            return Err(StoreError::Fault(
                "a store operation outside an action".into(),
            ));
        };
        boundary.transaction(&self.capabilities, store, true).await
    }

    /// [`Ctx::transaction`], for a statement that only reads: while the
    /// action has only read, its transaction is a snapshot, which an
    /// outside call closes before it goes out (area 6, "Outside calls
    /// hold no transaction"; chunk 7.4).
    pub async fn read_transaction(
        &mut self,
        store: &str,
    ) -> Result<&mut dyn Transaction, StoreError> {
        let Some(boundary) = &mut self.boundary else {
            return Err(StoreError::Fault(
                "a store operation outside an action".into(),
            ));
        };
        boundary.transaction(&self.capabilities, store, false).await
    }

    /// Keeps `ids`, rows a read just loaded, while the running action has
    /// only read, for the versions an outside call reads before it closes
    /// the snapshot (chunk 7.4).
    pub(crate) fn loaded(&mut self, versions: &jadpo_std::read::RowVersions, ids: Vec<SqlValue>) {
        if let Some(boundary) = &mut self.boundary {
            boundary.loaded(versions, ids);
        }
    }

    /// Closes the running action's transaction when it has only read,
    /// once the versions of the rows it loaded are read: what an outside
    /// call does before it goes out, so no transaction is open while it is
    /// out (area 6, "Outside calls hold no transaction"; chunk 7.4). A
    /// transaction that has written stays open.
    pub async fn close_snapshot(&mut self) -> Result<(), StoreError> {
        match &mut self.boundary {
            Some(boundary) => boundary.close_snapshot().await,
            None => Ok(()),
        }
    }

    /// Whether the running action loaded rows in a snapshot an outside
    /// call closed (chunk 7.4).
    pub(crate) fn loaded_before_call(&self) -> bool {
        self.boundary
            .as_ref()
            .is_some_and(Boundary::loaded_before_call)
    }

    pub(crate) fn expected_row(
        &self,
        row: &jadpo_std::write::RowCheck,
        id: &SqlValue,
    ) -> Option<String> {
        self.boundary
            .as_ref()
            .and_then(|boundary| boundary.expected(row, id))
    }

    pub(crate) fn checked_row(&mut self, row: &jadpo_std::write::RowCheck, id: SqlValue) {
        if let Some(boundary) = &mut self.boundary {
            boundary.checked(row, id);
        }
    }

    /// Holds the row `row` names, `id`, to the version a snapshot an
    /// outside call closed loaded it at, when a write in `store` changed
    /// no row: `ConcurrentChange` before its ordinary refusal when it
    /// changed or went since (chunk 9.1b4).
    pub(crate) async fn check_row(
        &mut self,
        store: &str,
        row: &jadpo_std::write::RowCheck,
        id: SqlValue,
    ) -> Result<(), StoreError> {
        let Some(boundary) = &mut self.boundary else {
            return Ok(());
        };
        boundary.check(&self.capabilities, store, row, id).await
    }

    /// A new identity: a version 7 UUID (area 5), its first 48 bits the
    /// clock's milliseconds and the rest random: in a deployable, from the
    /// host's random bytes; under a seed (a test's), from a generator the
    /// seed starts, so a test that makes records reproduces with its seed.
    /// `None` when there is no seed and the host has no randomness to give.
    pub fn new_id(&mut self) -> Option<[u8; 16]> {
        self.capabilities.new_id(self.env.clock.now())
    }

    /// Queues `event` in the running action's outbox, written to it as
    /// the action commits and delivered once it has (area 12); refused
    /// outside an action, and past the limits of one action's events.
    pub fn emit(&mut self, event: Event) -> Result<(), EmitRefused> {
        match &mut self.boundary {
            Some(boundary) => boundary.emit(event),
            None => Err(EmitRefused::OutsideAction),
        }
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
            boundary.mark_changed();
        }
    }
}

/// Whether a WebAssembly module's own stack has less than
/// [`crate::WASM_STACK_RESERVE`] left. Rust links it first in the module's
/// memory (`--stack-first`), growing down towards address 0, so the
/// address of a value on it is what is left. Natively a thread's stack is
/// [`crate::STACK_BYTES`], which a thousand frames of any reasonable size
/// don't reach.
#[cfg(target_arch = "wasm32")]
fn stack_low() -> bool {
    let here = 0u8;
    let at = std::ptr::addr_of!(here) as usize;
    at < crate::WASM_STACK_RESERVE
}

#[cfg(not(target_arch = "wasm32"))]
fn stack_low() -> bool {
    false
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
        recorded.record_run();
        let given = (draw(&mut recorded), draw(&mut recorded));
        let tape = recorded.recorded_run().expect("recording").random.clone();
        assert_eq!(tape.len(), 24);
        // ...replays on a host with no generator and no seed, and once
        // the recording runs out there is no randomness, never a weaker one.
        let mut replayed = Ctx::new(&places, &none, live);
        replayed.replay_run(crate::capability::Recording {
            random: tape,
            calls: Vec::new(),
        });
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
        recorded.record_run();
        let mut code = [0u8; 4];
        assert!(recorded.random_bytes(&mut code));
        let mut token = [0u8; 32];
        assert!(recorded.secret_random_bytes(&mut token));
        let mut after = [0u8; 4];
        assert!(recorded.random_bytes(&mut after));
        // Only the draws a program makes are kept, in order.
        let tape = recorded.recorded_run().expect("recording").random.clone();
        assert_eq!(tape, [code, after].concat());
        // A replay gives them back in order, and the token's draw takes
        // fresh bytes of its own, never the recording's.
        let other = Random(0x11);
        let mut replayed = Ctx::new(&places, &other, live);
        replayed.replay_run(crate::capability::Recording {
            random: tape,
            calls: Vec::new(),
        });
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
