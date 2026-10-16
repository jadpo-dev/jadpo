//! The per-invocation context (docs/compiler-runtime.md §2.5): what an
//! executor is handed, never what it owns. It keeps the shadow stack of
//! frames both executors push, the depth limit, the faults and failures
//! that capture the stack, the recording of a failed `assert`'s parts, and
//! the state of the action boundary the runtime runs ([`crate::action`]):
//! the transaction it opens at the first store operation and the events
//! waiting to be delivered once it commits; and the effects with one
//! source each, the clock and new identities. Reads and writes run through
//! [`crate::store`] against it; integration calls join it with their area.

use std::fmt;

use jadpo_value::Value;

use crate::outcome::{Assertion, AssertionDetail, Part};
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

/// An action's transaction and outbox, kept open after its body succeeded
/// for whoever asked the boundary to hold it ([`Ctx::hold`]): the HTTP
/// component, which writes the request's idempotency key into the same
/// transaction and commits it once the route's deadline can no longer
/// interrupt it.
#[derive(Default)]
pub struct Held {
    /// `None` when the body did no store operation.
    pub transaction: Option<Box<dyn Transaction>>,
    pub outbox: Vec<Event>,
    /// What the body did to the request's session.
    pub session: Option<SessionEffect>,
    /// Whether the body issued an API token or a service key.
    pub issued: bool,
}

impl fmt::Debug for Held {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Held")
            .field("transaction", &self.transaction.is_some())
            .field("outbox", &self.outbox)
            .field("session", &self.session)
            .field("issued", &self.issued)
            .finish()
    }
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
    /// The address the request being answered counts as (area 10, "Client
    /// addresses"); `None` when none is known.
    client: Option<String>,
    /// Whether the statements it runs leave their policy out: only for a
    /// call a test, fixture or seed makes without a caller (area 9).
    pub(crate) unpoliced: bool,
    /// How many random values it has made from its seed.
    draws: u64,
    /// When the request being answered runs out of time, on the host's
    /// monotonic clock (area 7, "Time limits").
    deadline: Option<u64>,
    /// The settings a test gives its own values (area 13): each by its
    /// number in the plan.
    settings: Vec<(u32, Value)>,
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
            client: None,
            unpoliced: false,
            draws: 0,
            deadline: None,
            settings: Vec::new(),
        }
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
    /// policy compiled into each statement applies to it (area 9).
    pub fn set_caller(&mut self, caller: Option<Caller>) {
        self.caller = caller;
        self.unpoliced = false;
    }

    /// Runs what follows with no caller and without policy: a call a test,
    /// fixture or seed makes without passing a caller, which proves
    /// behaviour and data, as a route test or a call with a caller proves
    /// policy (areas 9 and 13). Nothing a request runs does this.
    pub fn set_unpoliced(&mut self) {
        self.caller = None;
        self.unpoliced = true;
    }

    /// Whether statements leave their policy out ([`Ctx::set_unpoliced`]).
    pub fn unpoliced(&self) -> bool {
        self.unpoliced
    }

    /// The value of part `part` of the caller, for the policy compiled into
    /// a statement: the signed-in user's or service's `id`, `NULL` for
    /// anyone else, and whether anyone is signed in.
    pub fn caller_param(&self, part: jadpo_std::policy::CallerPart) -> SqlValue {
        use jadpo_std::policy::CallerPart;
        let held = self.caller.as_ref().and_then(|caller| match &caller.value {
            Value::Record(record) => Some((record.ty.to_string(), record.get("id").cloned()?)),
            _ => None,
        });
        match part {
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

    /// Fills `out` with random bytes: the host's, or under a seed (a
    /// test's) from a generator the seed starts, so a test reproduces.
    /// `false` when there is neither.
    pub fn random_bytes(&mut self, out: &mut [u8]) -> bool {
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

    /// Queues `event` in the running action's outbox, delivered once it
    /// commits (area 12); `false` outside an action.
    pub fn emit(&mut self, event: Event) -> bool {
        match &mut self.boundary {
            Some(boundary) => {
                boundary.outbox.push(event);
                true
            }
            None => false,
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

        fn deliver(&self, _events: Vec<Event>) -> crate::LocalFuture<'_, ()> {
            Box::pin(async {})
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
