//! The one host the runtime's Rust tests run on (chunk 7.0c, from the
//! Phase 6 architecture review, which counted twenty hand-written hosts in
//! `crates/jadpo-cli/tests`): a [`Harness`] over one store, whose clock,
//! randomness, timers, budget and background work a test sets. Fakes of
//! outside capabilities wrap it, as they wrap any host
//! ([`jadpo_runtime::capability::Faked`]).
//!
//! It is the runtime's, not a test project's: `jadpo test` runs each test
//! on [`crate::database::TestHost`], and a Rust test that drives the
//! runtime directly runs on a harness.

use std::cell::{Cell, RefCell};

use jadpo_runtime::budget::Invocation;
use jadpo_runtime::{Clock, Ctx, Database, Env, Host, LocalFuture, Places, Row, SqlValue};

/// What a timer of some length does on a [`Harness`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Timer {
    /// Never finishes, so nothing runs out of time: every timer's default.
    Never,
    /// Has finished when it is first looked at.
    Now,
    /// Hasn't finished the first time it is looked at, and has every time
    /// after: a limit that passes while what it races runs.
    Later,
}

/// A host over one store, `database`, given for every store name, whose
/// clock, randomness, timers, monotonic clock and budget a test sets. It
/// keeps the work it is given to run after a response and to hand on,
/// for the test to run or drop.
pub struct Harness<D = jadpo_host_native::Postgres> {
    database: D,
    /// The wall clock, when it has one, and how far it moves on each time
    /// it is read.
    clock: Option<Cell<i64>>,
    step: Cell<i64>,
    /// How many random draws it has given, when it gives random bytes.
    random: Option<Cell<u64>>,
    /// How many contexts it has made, each with a seed of its own.
    seeds: Cell<u64>,
    /// The monotonic clock and how far it moves on each time it is read.
    monotonic: Option<(Cell<u64>, u64)>,
    budget: Option<u64>,
    /// How many of a file's rows a batch reads, when not the runtime's.
    rows_per_batch: Option<u32>,
    timers: RefCell<Vec<(u64, Timer)>>,
    deferred: RefCell<Vec<LocalFuture<'static, ()>>>,
    handed: RefCell<Vec<LocalFuture<'static, ()>>>,
    /// What the runtime fetches on its own account, when the test gives
    /// it (chunk 7.7b).
    outbound: Option<std::rc::Rc<dyn jadpo_runtime::Outbound>>,
}

impl<D> std::fmt::Debug for Harness<D> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Harness")
            .field("clock", &self.clock)
            .field("random", &self.random)
            .field("seeds", &self.seeds)
            .field("budget", &self.budget)
            .finish_non_exhaustive()
    }
}

impl Harness<jadpo_host_native::Postgres> {
    /// A harness over the PostgreSQL database at `url`.
    pub fn postgres(url: &str) -> Harness<jadpo_host_native::Postgres> {
        Harness::new(
            jadpo_host_native::postgres(url)
                .unwrap_or_else(|error| panic!("the store at {url}: {error:?}")),
        )
    }
}

impl<D: Database> Harness<D> {
    /// A harness over `database` with no clock and no randomness, as a
    /// test's host has, so each context runs under its seed and the test
    /// clock; timers that never finish; the default budget; and no
    /// monotonic clock.
    pub fn new(database: D) -> Harness<D> {
        Harness {
            database,
            clock: None,
            step: Cell::new(0),
            random: None,
            seeds: Cell::new(0),
            monotonic: None,
            budget: None,
            rows_per_batch: None,
            timers: RefCell::new(Vec::new()),
            deferred: RefCell::new(Vec::new()),
            handed: RefCell::new(Vec::new()),
            outbound: None,
        }
    }

    /// Gives it `outbound` for what the runtime fetches on its own
    /// account: a test's server standing in for AWS's (chunk 7.7b).
    pub fn outbound(mut self, outbound: std::rc::Rc<dyn jadpo_runtime::Outbound>) -> Harness<D> {
        self.outbound = Some(outbound);
        self
    }

    /// Gives it a wall clock reading `now`, milliseconds since the epoch,
    /// which the test moves.
    pub fn at(mut self, now: i64) -> Harness<D> {
        self.clock = Some(Cell::new(now));
        self
    }

    /// Gives it random bytes, drawn from a counter starting after `drawn`
    /// draws: the same bytes every run, and different benches different
    /// bytes when they start apart.
    pub fn random_from(mut self, drawn: u64) -> Harness<D> {
        self.random = Some(Cell::new(drawn));
        self
    }

    /// Gives it a monotonic clock reading `start`, moving on `step` each
    /// time it is read.
    pub fn monotonic(mut self, start: u64, step: u64) -> Harness<D> {
        self.monotonic = Some((Cell::new(start), step));
        self
    }

    /// Gives each invocation of background work `ms` milliseconds.
    pub fn budget(mut self, ms: u64) -> Harness<D> {
        self.budget = Some(ms);
        self
    }

    /// Has a file helper's run read `rows` of a file's rows into each
    /// batch (chunk 6.4b).
    pub fn rows_per_batch(mut self, rows: u32) -> Harness<D> {
        self.rows_per_batch = Some(rows);
        self
    }

    /// Its store, as the test made it.
    pub fn store(&self) -> &D {
        &self.database
    }

    /// Its clock's time, or the test clock's without one.
    pub fn now(&self) -> i64 {
        self.clock
            .as_ref()
            .map_or_else(|| Clock::TEST.now(), Cell::get)
    }

    /// Sets its clock, which [`Harness::at`] gave it, to `now`.
    pub fn set_now(&self, now: i64) {
        self.clock
            .as_ref()
            .expect("a harness with a clock: give it one with `at`")
            .set(now);
    }

    /// Moves its clock on by `ms`.
    pub fn advance(&self, ms: i64) {
        self.set_now(self.now() + ms);
    }

    /// Moves its clock on by `ms` each time the host's time is read, as
    /// work that takes time would.
    pub fn step(&self, ms: i64) {
        self.step.set(ms);
    }

    /// Sets what a timer of `millis` does, from its next start.
    pub fn set_timer(&self, millis: u64, timer: Timer) {
        let mut timers = self.timers.borrow_mut();
        timers.retain(|(length, _)| *length != millis);
        if timer != Timer::Never {
            timers.push((millis, timer));
        }
    }

    /// A context for one invocation on it, under a seed of its own and its
    /// clock's time.
    pub fn ctx<'p>(&'p self, places: &'p Places) -> Ctx<'p> {
        self.ctx_on(places, self)
    }

    /// A context for one invocation on `host`, a wrapper of it (fakes in
    /// some capabilities' place), as [`Harness::ctx`] makes one.
    pub fn ctx_on<'p>(&self, places: &'p Places, host: &'p dyn Host) -> Ctx<'p> {
        self.seeds.set(self.seeds.get() + 1);
        let env = Env {
            seed: Some(self.seeds.get()),
            clock: Clock::fixed(self.now()),
        };
        Ctx::new(places, host, env)
    }

    /// The work it was given to run after a response, in order.
    pub fn take_deferred(&self) -> Vec<LocalFuture<'static, ()>> {
        std::mem::take(&mut *self.deferred.borrow_mut())
    }

    /// The work it was given to hand on, in order.
    pub fn take_handed(&self) -> Vec<LocalFuture<'static, ()>> {
        std::mem::take(&mut *self.handed.borrow_mut())
    }

    /// The rows `sql` gives, run on its own.
    pub async fn rows(&self, sql: &str) -> Vec<Row> {
        self.database
            .query_once(sql, &[])
            .await
            .unwrap_or_else(|error| panic!("{sql}: {error:?}"))
    }

    /// The count `sql`, a `SELECT count(*)`, gives.
    pub async fn count(&self, sql: &str) -> i64 {
        match self.rows(sql).await.first().and_then(|row| row.first()) {
            Some(SqlValue::Int8(n)) => *n,
            Some(SqlValue::Int4(n)) => i64::from(*n),
            other => panic!("not a count: {other:?}"),
        }
    }
}

/// A future that is pending the first time it is polled, asking to be
/// polled again, and finished after.
struct Later(bool);

impl std::future::Future for Later {
    type Output = ();

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<()> {
        if self.0 {
            return std::task::Poll::Ready(());
        }
        self.0 = true;
        cx.waker().wake_by_ref();
        std::task::Poll::Pending
    }
}

impl<D: Database> Host for Harness<D> {
    fn database(&self, _store: &str) -> Option<&dyn Database> {
        Some(&self.database)
    }

    fn outbound(&self) -> Option<&dyn jadpo_runtime::Outbound> {
        self.outbound.as_deref()
    }

    fn random(&self, bytes: &mut [u8]) -> bool {
        let Some(drawn) = &self.random else {
            return false;
        };
        for chunk in bytes.chunks_mut(8) {
            drawn.set(drawn.get() + 1);
            let mut z = drawn.get().wrapping_mul(0x9E37_79B9_7F4A_7C15);
            z = (z ^ (z >> 31)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            for (to, from) in chunk.iter_mut().zip(z.to_be_bytes()) {
                *to = from;
            }
        }
        true
    }

    fn now_ms(&self) -> Option<i64> {
        let clock = self.clock.as_ref()?;
        let now = clock.get();
        clock.set(now + self.step.get());
        Some(now)
    }

    fn monotonic_ms(&self) -> Option<u64> {
        let (now, step) = self.monotonic.as_ref()?;
        let then = now.get();
        now.set(then + step);
        Some(then)
    }

    fn sleep(&self, millis: u64) -> LocalFuture<'_, ()> {
        let timer = self
            .timers
            .borrow()
            .iter()
            .find(|(length, _)| *length == millis)
            .map_or(Timer::Never, |(_, timer)| *timer);
        match timer {
            Timer::Never => Box::pin(std::future::pending()),
            Timer::Now => Box::pin(async {}),
            Timer::Later => Box::pin(Later(false)),
        }
    }

    fn budget_ms(&self, invocation: Invocation) -> u64 {
        let _ = invocation;
        self.budget.unwrap_or(jadpo_runtime::budget::GENEROUS_MS)
    }

    fn rows_per_batch(&self) -> u32 {
        self.rows_per_batch
            .unwrap_or(jadpo_runtime::batches::ROWS_PER_BATCH)
    }

    fn defer(&self, work: LocalFuture<'static, ()>) {
        self.deferred.borrow_mut().push(work);
    }

    fn hand_on(&self, work: LocalFuture<'static, ()>) {
        self.handed.borrow_mut().push(work);
    }
}
