//! The Workers host (docs/compiler-runtime.md §2.1 and §2.6, "The Workers
//! host"; chunk 8.2a): what runs a compiled Jadpo program on Cloudflare
//! Workers, over wasm-bindgen and Jadpo's own bindings to what Workers
//! gives a Worker ([`js`]), rather than workers-rs.
//!
//! - [`WorkersHost`]: the runtime's `Host` for one invocation: each
//!   connection's driver as the factory built it from the plan
//!   ([`WorkersHost::supplies`]), built when the invocation first reaches
//!   a connection, so one that reaches none builds nothing (chunk 8.4e),
//!   its connections' tasks run under the invocation's `waitUntil`,
//!   randomness from Web Crypto's
//!   `getRandomValues`, the wall clock (`Date.now()`) and a monotonic clock
//!   made from it, timers on `setTimeout`, settings from the Worker's
//!   bindings, the work a response leaves run through `ctx.waitUntil` and
//!   the work an invocation hands on run in a fresh one, a request to the
//!   Worker itself through the loopback binding `ctx.exports` gives its
//!   own [`HAND_ON_ENTRYPOINT`], whatever name it was deployed under, rate
//!   limits counted in Cloudflare's rate-limiting binding when
//!   `rate_limits:` names one, and in the isolate's memory while it can't
//!   be asked, and Workers' budgets for background work (30 seconds after a
//!   response and for work handed on, counted from the start of the
//!   invocation that takes it on, 15 minutes for the cron, which the
//!   ticks one cron runs share); the six connections an invocation may have
//!   waiting for their answer's head at once ([`Opening`]), shared by its
//!   drivers and its own fetches, and the isolate's `max_concurrent:`
//!   slots, handed from one invocation to another without either waking the
//!   other (chunk 8.2j).
//! - [`WorkersTransport`]: the drivers' `Transport` over `connect()` from
//!   `cloudflare:sockets`, upgraded with `startTls()`; a Hyperdrive
//!   binding's host is the platform's pooler, which PostgreSQL's driver
//!   reaches without TLS (chunk 8.2i).
//! - [`WorkersFetch`] and [`WorkersOutbound`]: HTTP out of the Worker
//!   through `fetch()`, aborted through an `AbortSignal` when its time is
//!   up and following no redirect, for the package driver and for the
//!   runtime's own fetches.
//! - [`deployable`]: what a module `jadpo build` writes for a
//!   `Target.wasm` deployable calls from the handlers it exports (chunk
//!   8.2b): `fetch` serving each request through the runtime, the settings
//!   read from the bindings and the stores' migrations compared once per
//!   isolate (chunk 8.2d), `scheduled` ticking at each run of the minute's
//!   cron trigger (chunk 8.2f), its streamed answers sent as a
//!   `ReadableStream` and `hand_on` taking on work handed on (chunk 8.2h);
//!   and [`wasm_bindgen`], re-exported so that crate exports them through
//!   this one's.
//! - `testing`, with the `testing` feature: a test Worker's handler, which
//!   runs one test of compiled programs built with the runtime's `testing`
//!   feature by its key, so `jadpo-test` compares route tests run inside
//!   workerd with the interpreter's (chunk 8.2e). No deployable turns it
//!   on.
//!
//! The crate builds for wasm32-unknown-unknown, which is what it is for,
//! and natively, where its bindings compile but are never called, so the
//! workspace builds, lints and tests as one; its tests run it under
//! workerd, Cloudflare's own Workers runtime, on this machine.

// A panic aborts a Workers instance, so runtime code reports faults as
// values (docs/compiler-runtime.md §2.6); tests may still unwrap.
#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing,
        clippy::unreachable,
        clippy::unimplemented
    )
)]

pub mod deployable;
mod fetch;
mod hyperdrive;
mod js;
pub mod opening;
mod passkeys;
mod ratelimit;
mod streamed;
#[cfg(feature = "testing")]
pub mod testing;
mod timer;
mod transport;

use std::cell::{Cell, OnceCell, RefCell};
use std::rc::Rc;
use std::sync::OnceLock;

use jadpo_drivers::factory::{Drivers, Supplies, Unbuilt};
use jadpo_runtime::budget::Invocation;
use jadpo_runtime::capability::Capability;
use jadpo_runtime::deployable::Log;
use jadpo_runtime::http::address::Target;
use jadpo_runtime::slots::Slots;
use jadpo_runtime::{Allowed, Database, Host, LocalFuture, Outbound, Passkeys, RateLimits};
use jadpo_std::http::{RateLimit, RateLimitStore};
use js_sys::{Reflect, Uint8Array};
use wasm_bindgen::{JsCast, JsValue};

pub use fetch::{WorkersFetch, WorkersOutbound};
pub use js::ExecutionContext;
pub use opening::Opening;
pub use passkeys::WorkersPasskeys;
pub use ratelimit::LIMIT_TIMEOUT_MS;
pub use transport::{WorkersStream, WorkersTransport};
/// The release of wasm-bindgen this crate binds with, for the handlers a
/// deployable's crate exports (`#[wasm_bindgen(wasm_bindgen = …)]`), so
/// that crate depends on this one alone.
pub use wasm_bindgen;

/// How long the work a response leaves may run on Workers: `waitUntil`
/// keeps an invocation 30 seconds past its response. Work handed on runs
/// the same way, so it has as long.
pub const AFTER_RESPONSE_MS: u64 = 30_000;

/// How long a cron-triggered invocation may run: 15 minutes of wall time.
pub const CRON_MS: u64 = 15 * 60 * 1000;

/// The named entrypoint Jadpo's module shim exports for work handed on:
/// its `handOn(work)` takes the work on in an invocation of its own. Not
/// the Worker's default export, so nothing from outside reaches it. A
/// Worker reaches its own through `ctx.exports` (chunk 8.4b), the loopback
/// binding Workers gives each entrypoint a Worker exports, so the work
/// goes to the Worker that handed it on whatever name or environment
/// `wrangler deploy` gave it, never to another deployed from the same
/// file.
pub const HAND_ON_ENTRYPOINT: &str = "JadpoHandOn";

/// What each kind of background work may run for on Workers (area 12,
/// "Running background work").
pub fn budget_for(invocation: Invocation) -> u64 {
    match invocation {
        Invocation::AfterResponse | Invocation::HandedOn => AFTER_RESPONSE_MS,
        Invocation::Tick => CRON_MS,
    }
}

/// What is left at `now` of a cron invocation's 15 minutes, begun at
/// `began`, both on the monotonic clock: a tick it starts later in its
/// minute has only that (chunk 8.2f).
pub fn cron_left(began: u64, now: u64) -> u64 {
    CRON_MS.saturating_sub(now.saturating_sub(began))
}

/// What is left at `now` of the 30 seconds of an invocation taking on work
/// handed on, begun at `began`, both on the monotonic clock: Workers counts
/// them from the invocation's start, which answers at once, so the time
/// spent starting the isolate and reading the work back is spent from them
/// (chunk 8.4b).
pub fn hand_on_left(began: u64, now: u64) -> u64 {
    AFTER_RESPONSE_MS.saturating_sub(now.saturating_sub(began))
}

/// The isolate's `max_concurrent:` slots, which every invocation it
/// serves shares (area 11, decided 2026-10-21): a looking set, so a slot
/// one invocation gives back is handed to a call of another without the
/// first waking it, the second finding it on its own timer (chunk 8.2j).
fn isolate_slots() -> &'static Slots {
    static SLOTS: OnceLock<Slots> = OnceLock::new();
    SLOTS.get_or_init(|| Slots::looking(monotonic_ms))
}

/// `Date.now()`, never going back past an earlier reading: the isolate's
/// monotonic clock.
fn monotonic_ms() -> Option<u64> {
    let now = u64::try_from(wall_clock()?).ok()?;
    Some(MONOTONIC.with(|last| {
        let now = now.max(last.get());
        last.set(now);
        now
    }))
}

/// The wall clock, `Date.now()`, in whole milliseconds. Workers moves it
/// on only as the invocation waits, never while it computes.
pub(crate) fn wall_clock() -> Option<i64> {
    let now = js_sys::Date::now();
    // A whole number of milliseconds, well inside an `i64`.
    (now.is_finite() && now >= 0.0).then_some(now as i64)
}

/// What builds an invocation's drivers over what its host supplies
/// ([`WorkersHost::supplies`]).
type BuildDrivers = Box<dyn FnOnce(&Supplies) -> Drivers>;

/// The runtime's host for one invocation of a Worker: its `env` and `ctx`,
/// and the drivers built for it. A socket belongs to the invocation that
/// opened it, so the drivers that hold connections are built for each,
/// when it first reaches a connection (chunk 8.4e).
pub struct WorkersHost {
    /// The Worker's bindings: its settings among them.
    env: JsValue,
    /// The invocation's context, when it has one.
    ctx: Option<ExecutionContext>,
    /// The invocation's drivers, once built.
    drivers: OnceCell<Drivers>,
    /// What builds them, until they are built: none builds none.
    build: RefCell<Option<BuildDrivers>>,
    outbound: WorkersOutbound,
    /// Where rate limits are counted: the plan's store, when it names one.
    rate_limits: Option<Rc<RateLimitStore>>,
    /// The deployable's log, which a rate-limiting binding that can't be
    /// asked is logged to once an isolate (chunk 8.4b).
    log: Option<Log>,
    /// When the cron invocation it serves began on the monotonic clock:
    /// each tick it runs has what is left of the cron's 15 minutes.
    cron: Option<u64>,
    /// When the invocation taking on work handed on began on the
    /// monotonic clock: its work has what is left of its 30 seconds.
    handed_on: Option<u64>,
    /// The invocation's six places for connections being opened, which
    /// its drivers and its own fetches share.
    opening: Rc<Opening>,
}

impl std::fmt::Debug for WorkersHost {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkersHost")
            .field("ctx", &self.ctx.is_some())
            .field("cron", &self.cron)
            .field("handed_on", &self.handed_on)
            .finish_non_exhaustive()
    }
}

impl WorkersHost {
    /// The host for an invocation given `env` and `ctx`, with no driver
    /// built yet.
    pub fn new(env: JsValue, ctx: JsValue) -> WorkersHost {
        let ctx = ctx
            .is_object()
            .then(|| ctx.unchecked_into::<ExecutionContext>());
        let opening = Rc::new(Opening::default());
        WorkersHost {
            env,
            ctx,
            drivers: OnceCell::new(),
            build: RefCell::new(None),
            outbound: WorkersOutbound::default().counted(Rc::clone(&opening)),
            rate_limits: None,
            log: None,
            cron: None,
            handed_on: None,
            opening,
        }
    }

    /// What the driver factory builds this invocation's drivers over: the
    /// target, `connect()` and `fetch()`, their connections counted among
    /// the invocation's six with its own fetches and their tasks run under
    /// its `waitUntil` (chunk 8.4e), and no disk, which a Worker hasn't got.
    pub fn supplies(&self) -> Supplies {
        let transport =
            WorkersTransport::counted(Rc::clone(&self.opening)).within(self.ctx.clone());
        Supplies::on(Target::Workers)
            .with_transport(Rc::new(transport))
            .with_fetch(Rc::new(WorkersFetch::counted(Rc::clone(&self.opening))))
    }

    /// The invocation's places for connections being opened.
    pub fn opening(&self) -> &Rc<Opening> {
        &self.opening
    }

    /// Counts rate limits in `store`, the one `rate_limits:` names: in
    /// Cloudflare's rate-limiting binding for each limit it has one for.
    pub fn with_rate_limits(mut self, store: Option<Rc<RateLimitStore>>) -> WorkersHost {
        self.rate_limits = store;
        self
    }

    /// Logs to `log` what the host has to say outside a request's own
    /// events: a rate-limiting binding that can't be asked, once an
    /// isolate (chunk 8.4b).
    pub fn with_log(mut self, log: Log) -> WorkersHost {
        self.log = Some(log);
        self
    }

    /// The host for a run of the cron trigger, which began now: the
    /// ticks it runs share the cron's 15 minutes ([`cron_left`]).
    pub fn for_cron(mut self) -> WorkersHost {
        self.cron = self.monotonic_ms();
        self
    }

    /// The host for an invocation taking on work handed on, which began
    /// now: what it runs has what is left of its 30 seconds
    /// ([`hand_on_left`]).
    pub fn for_hand_on(mut self) -> WorkersHost {
        self.handed_on = self.monotonic_ms();
        self
    }

    /// Gives each connection the driver `drivers` built for it from the
    /// plan's connections table, over [`WorkersHost::supplies`].
    pub fn with_drivers(self, drivers: Drivers) -> WorkersHost {
        let _ = self.drivers.set(drivers);
        self
    }

    /// Builds the invocation's drivers with `build`, over
    /// [`WorkersHost::supplies`], when it first reaches a connection: a
    /// request that reaches none, as the readiness check mostly doesn't,
    /// builds none (chunk 8.4e).
    pub fn with_drivers_built_by(
        self,
        build: impl FnOnce(&Supplies) -> Drivers + 'static,
    ) -> WorkersHost {
        *self.build.borrow_mut() = Some(Box::new(build));
        self
    }

    /// The invocation's drivers, built now if they weren't yet.
    fn drivers(&self) -> &Drivers {
        self.drivers.get_or_init(|| {
            let build = self.build.borrow_mut().take();
            match build {
                Some(build) => build(&self.supplies()),
                None => Drivers::default(),
            }
        })
    }

    /// Guards the runtime's own fetches with `outbound`, which take places
    /// among the invocation's six.
    pub fn with_outbound(mut self, outbound: WorkersOutbound) -> WorkersHost {
        self.outbound = outbound.counted(Rc::clone(&self.opening));
        self
    }

    /// Each connection whose driver the factory couldn't build, and why:
    /// building the drivers if they weren't yet.
    pub fn unbuilt(&self) -> &[Unbuilt] {
        self.drivers().unbuilt()
    }
}

/// The isolate's rate-limit counters, which last as long as it does and
/// count every request it serves: on Workers the fallback area 16 allows
/// while no Redis connection or Cloudflare's rate-limiting binding can
/// count them.
fn isolate_limits() -> &'static RateLimits {
    static LIMITS: OnceLock<RateLimits> = OnceLock::new();
    LIMITS.get_or_init(RateLimits::default)
}

/// The most bytes one `getRandomValues` fills.
const MOST_RANDOM: usize = 65_536;

/// Fills `bytes` from Web Crypto's `getRandomValues`, 65,536 bytes at a
/// time: `false` when it can't, never a weaker source.
pub fn web_crypto_random(bytes: &mut [u8]) -> bool {
    bytes.chunks_mut(MOST_RANDOM).all(|chunk| {
        let Ok(length) = u32::try_from(chunk.len()) else {
            return false;
        };
        let array = Uint8Array::new_with_length(length);
        if js::get_random_values(&array).is_err() {
            return false;
        }
        array.copy_to(chunk);
        true
    })
}

/// The passkey ceremonies, their challenges from Web Crypto.
static PASSKEYS: WorkersPasskeys = WorkersPasskeys::new(web_crypto_random);

thread_local! {
    /// The monotonic clock's last reading, which it never goes back past.
    static MONOTONIC: Cell<u64> = const { Cell::new(0) };
}

impl Host for WorkersHost {
    fn capability(&self, connection: &str) -> Option<&dyn Capability> {
        self.drivers().capability(connection)
    }

    /// The PostgreSQL store the factory built, with the `postgres`
    /// feature; none without it.
    fn database(&self, store: &str) -> Option<&dyn Database> {
        #[cfg(feature = "postgres")]
        return self
            .drivers()
            .database(store)
            .map(|store| store as &dyn Database);
        #[cfg(not(feature = "postgres"))]
        {
            let _ = store;
            None
        }
    }

    fn outbound(&self) -> Option<&dyn Outbound> {
        Some(&self.outbound)
    }

    /// Run through `ctx.waitUntil`, once the response is handed back; with
    /// no context to keep the invocation alive, dropped, as a host with
    /// nothing to run it on drops it, the outbox keeping what it held.
    fn defer(&self, work: LocalFuture<'static, ()>) {
        let Some(ctx) = &self.ctx else {
            return;
        };
        let promise = wasm_bindgen_futures::future_to_promise(async move {
            work.await;
            Ok(JsValue::UNDEFINED)
        });
        let _ = ctx.wait_until(&promise);
    }

    /// A fresh invocation of the Worker, with Workers' 30 seconds of its
    /// own (chunk 8.2h): a request to itself through the loopback binding
    /// `ctx.exports` gives its [`HAND_ON_ENTRYPOINT`] (chunk 8.4b), sent
    /// the work's [`HandOn::message`], whose invocation reads it back and
    /// runs it in its own `waitUntil` ([`deployable::hand_on`]); this
    /// invocation is kept until that one has taken it on. Without the
    /// entrypoint or a context, or when the call fails, as past the
    /// platform's limit on invocations one request chains, it is dropped:
    /// what it names waits in the store, and the minute's cron takes it up.
    ///
    /// [`HandOn::message`]: jadpo_runtime::runner::HandOn::message
    fn hand_on(&self, work: jadpo_runtime::runner::HandOn) {
        let message = work.message();
        drop(work);
        let Some(ctx) = &self.ctx else {
            return;
        };
        // The loopback is a function, which a call can give props to, with
        // the entrypoint's methods on it.
        let reachable = |value: &JsValue| value.is_object() || value.is_function();
        let Some(binding) = Reflect::get(ctx, &JsValue::from_str("exports"))
            .ok()
            .filter(reachable)
            .and_then(|exports| Reflect::get(&exports, &JsValue::from_str(HAND_ON_ENTRYPOINT)).ok())
            .filter(reachable)
        else {
            return;
        };
        let binding = binding.unchecked_into::<js::HandOnBinding>();
        let Ok(called) = binding.hand_on(&message) else {
            return;
        };
        // Workers' RPC gives a thenable: kept as a promise settling either
        // way, so a refusal is no unhandled rejection.
        let called = js_sys::Promise::resolve(&called);
        let kept = wasm_bindgen_futures::future_to_promise(async move {
            let _ = wasm_bindgen_futures::JsFuture::from(called).await;
            Ok(JsValue::UNDEFINED)
        });
        let _ = ctx.wait_until(&kept);
    }

    /// Workers' budgets; a cron's tick has what is left of its
    /// invocation's 15 minutes, and the work an invocation taking on work
    /// handed on runs what is left of its 30 seconds.
    fn budget_ms(&self, invocation: Invocation) -> u64 {
        match (invocation, self.monotonic_ms()) {
            (Invocation::Tick, Some(now)) => match self.cron {
                Some(began) => cron_left(began, now),
                None => budget_for(invocation),
            },
            (Invocation::AfterResponse | Invocation::HandedOn, Some(now)) => match self.handed_on {
                Some(began) => hand_on_left(began, now),
                None => budget_for(invocation),
            },
            _ => budget_for(invocation),
        }
    }

    /// The Worker's binding of that name: its text, a variable's or a
    /// secret's; or, for a Hyperdrive binding, the address it gives, its
    /// host the platform's pooler (chunk 8.2i).
    fn variable(&self, name: &str) -> Option<String> {
        let binding = Reflect::get(&self.env, &JsValue::from_str(name)).ok()?;
        binding
            .as_string()
            .or_else(|| hyperdrive::address(&binding))
    }

    /// Web Crypto's `getRandomValues` ([`web_crypto_random`]).
    fn random(&self, bytes: &mut [u8]) -> bool {
        web_crypto_random(bytes)
    }

    /// The ceremonies in pure Rust ([`WorkersPasskeys`]): `webauthn-rs`'s
    /// checks, on jadpo-value's ES256 and RS256.
    fn passkeys(&self) -> Option<&dyn Passkeys> {
        Some(&PASSKEYS)
    }

    fn sleep(&self, millis: u64) -> LocalFuture<'_, ()> {
        Box::pin(timer::sleep(millis))
    }

    /// `Date.now()`, never going back past an earlier reading.
    fn monotonic_ms(&self) -> Option<u64> {
        monotonic_ms()
    }

    fn now_ms(&self) -> Option<i64> {
        wall_clock()
    }

    /// Counted in the rate-limiting binding the store gives `limit`, and
    /// in the isolate's memory ([`isolate_limits`]) when there is none or
    /// it can't be asked: missing, failing, or not answering within half a
    /// second, logged once an isolate (chunk 8.4b). A binding says only
    /// whether the request is within the limit, so a refusal is told to
    /// try again after the window's whole length, the longest it can have
    /// to wait.
    fn rate_limit<'a>(
        &'a self,
        key: &'a str,
        limit: RateLimit,
        now: i64,
    ) -> LocalFuture<'a, Allowed> {
        let binding = self
            .rate_limits
            .as_ref()
            .and_then(|store| store.binding(limit))
            .map(|binding| binding.name.clone());
        Box::pin(async move {
            if let Some(binding) = binding {
                match ratelimit::within(&self.env, &binding, key).await {
                    Ok(true) => return Allowed::Yes,
                    Ok(false) => {
                        return Allowed::No {
                            retry_after: u64::from(limit.window_ms.div_ceil(1000).max(1)),
                        };
                    }
                    Err(why) => ratelimit::fell_back(self.log.as_ref(), &binding, why),
                }
            }
            isolate_limits().allow(key, limit, now)
        })
    }

    /// The isolate's looking set ([`isolate_slots`]), which every
    /// invocation shares.
    fn slots(&self) -> &Slots {
        isolate_slots()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Background work has Workers' budgets, none past the 15-minute limit
    /// of a run.
    /// A tick a cron starts later in its invocation has only what is left
    /// of its 15 minutes, and none once they are spent.
    #[test]
    fn a_crons_later_tick_has_what_is_left_of_its_minutes() {
        assert_eq!(cron_left(1_000, 1_000), CRON_MS);
        assert_eq!(cron_left(1_000, 51_000), CRON_MS - 50_000);
        assert_eq!(cron_left(1_000, 1_000 + CRON_MS), 0);
        assert_eq!(cron_left(1_000, 2_000 + CRON_MS), 0);
        // A clock that went back gives the whole budget, never more.
        assert_eq!(cron_left(5_000, 1_000), CRON_MS);
    }

    /// Work handed on has what is left of the 30 seconds of the
    /// invocation taking it on, counted from that invocation's start, and
    /// none once they are spent.
    #[test]
    fn work_handed_on_has_what_is_left_of_its_invocations_seconds() {
        assert_eq!(hand_on_left(1_000, 1_000), AFTER_RESPONSE_MS);
        assert_eq!(hand_on_left(1_000, 4_500), AFTER_RESPONSE_MS - 3_500);
        assert_eq!(hand_on_left(1_000, 31_000), 0);
        assert_eq!(hand_on_left(1_000, 90_000), 0);
        assert_eq!(hand_on_left(5_000, 1_000), AFTER_RESPONSE_MS);
    }

    #[test]
    fn background_work_has_the_workers_budgets() {
        assert_eq!(budget_for(Invocation::AfterResponse), 30_000);
        assert_eq!(budget_for(Invocation::HandedOn), 30_000);
        assert_eq!(budget_for(Invocation::Tick), 900_000);
        for invocation in [
            Invocation::AfterResponse,
            Invocation::HandedOn,
            Invocation::Tick,
        ] {
            assert!(budget_for(invocation) <= jadpo_runtime::budget::RUN_LIMIT_MS);
        }
    }
}
