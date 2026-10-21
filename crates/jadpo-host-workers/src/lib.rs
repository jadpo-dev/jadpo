//! The Workers host (docs/compiler-runtime.md §2.1 and §2.6, "The Workers
//! host"; chunk 8.2a): what runs a compiled Jadpo program on Cloudflare
//! Workers, over wasm-bindgen and Jadpo's own bindings to what Workers
//! gives a Worker ([`js`]), rather than workers-rs.
//!
//! - [`WorkersHost`]: the runtime's `Host` for one invocation: each
//!   connection's driver as the factory built it from the plan
//!   ([`supplies`]), randomness from Web Crypto's `getRandomValues`, the
//!   wall clock (`Date.now()`) and a monotonic clock made from it, timers
//!   on `setTimeout`, settings from the Worker's bindings, the work a
//!   response leaves run through `ctx.waitUntil`, rate limits counted in
//!   the isolate's memory, and Workers' budgets for background work (30
//!   seconds after a response, 15 minutes for the cron).
//! - [`WorkersTransport`]: the drivers' `Transport` over `connect()` from
//!   `cloudflare:sockets`, upgraded with `startTls()`.
//! - [`WorkersFetch`] and [`WorkersOutbound`]: HTTP out of the Worker
//!   through `fetch()`, aborted through an `AbortSignal` when its time is
//!   up and following no redirect, for the package driver and for the
//!   runtime's own fetches.
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

mod fetch;
mod js;
mod passkeys;
mod timer;
mod transport;

use std::cell::Cell;
use std::rc::Rc;
use std::sync::OnceLock;

use jadpo_drivers::factory::{Drivers, Supplies};
use jadpo_runtime::budget::Invocation;
use jadpo_runtime::capability::Capability;
use jadpo_runtime::http::address::Target;
use jadpo_runtime::{Allowed, Database, Host, LocalFuture, Outbound, Passkeys, RateLimits};
use jadpo_std::http::RateLimit;
use js_sys::{Reflect, Uint8Array};
use wasm_bindgen::{JsCast, JsValue};

pub use fetch::{WorkersFetch, WorkersOutbound};
pub use js::ExecutionContext;
pub use passkeys::WorkersPasskeys;
pub use transport::{WorkersStream, WorkersTransport};

/// How long the work a response leaves may run on Workers: `waitUntil`
/// keeps an invocation 30 seconds past its response. Work handed on runs
/// the same way, so it has as long.
pub const AFTER_RESPONSE_MS: u64 = 30_000;

/// How long a cron-triggered invocation may run: 15 minutes of wall time.
pub const CRON_MS: u64 = 15 * 60 * 1000;

/// What each kind of background work may run for on Workers (area 12,
/// "Running background work").
pub fn budget_for(invocation: Invocation) -> u64 {
    match invocation {
        Invocation::AfterResponse | Invocation::HandedOn => AFTER_RESPONSE_MS,
        Invocation::Tick => CRON_MS,
    }
}

/// What the driver factory builds a Worker's drivers over: the target,
/// `connect()` and `fetch()`, and no disk, which a Worker hasn't got.
pub fn supplies() -> Supplies {
    Supplies::on(Target::Workers)
        .with_transport(Rc::new(WorkersTransport))
        .with_fetch(Rc::new(WorkersFetch))
}

/// The wall clock, `Date.now()`, in whole milliseconds. Workers moves it
/// on only as the invocation waits, never while it computes.
pub(crate) fn wall_clock() -> Option<i64> {
    let now = js_sys::Date::now();
    // A whole number of milliseconds, well inside an `i64`.
    (now.is_finite() && now >= 0.0).then_some(now as i64)
}

/// The runtime's host for one invocation of a Worker: its `env` and `ctx`,
/// and the drivers built for it. A socket belongs to the invocation that
/// opened it, so the drivers that hold connections are built for each.
pub struct WorkersHost {
    /// The Worker's bindings: its settings among them.
    env: JsValue,
    /// The invocation's context, when it has one.
    ctx: Option<ExecutionContext>,
    drivers: Drivers,
    outbound: WorkersOutbound,
}

impl std::fmt::Debug for WorkersHost {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkersHost")
            .field("ctx", &self.ctx.is_some())
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
        WorkersHost {
            env,
            ctx,
            drivers: Drivers::default(),
            outbound: WorkersOutbound::default(),
        }
    }

    /// Gives each connection the driver `drivers` built for it from the
    /// plan's connections table, over [`supplies`].
    pub fn with_drivers(mut self, drivers: Drivers) -> WorkersHost {
        self.drivers = drivers;
        self
    }

    /// Guards the runtime's own fetches with `outbound`.
    pub fn with_outbound(mut self, outbound: WorkersOutbound) -> WorkersHost {
        self.outbound = outbound;
        self
    }
}

/// The isolate's rate-limit counters, which last as long as it does and
/// count every request it serves: on Workers the fallback area 16 allows
/// while no Redis connection or Cloudflare's rate-limiting binding counts
/// them.
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
        self.drivers.capability(connection)
    }

    /// The PostgreSQL store the factory built, with the `postgres`
    /// feature; none without it.
    fn database(&self, store: &str) -> Option<&dyn Database> {
        #[cfg(feature = "postgres")]
        return self
            .drivers
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

    fn budget_ms(&self, invocation: Invocation) -> u64 {
        budget_for(invocation)
    }

    /// The Worker's binding of that name, when it is text: a variable or
    /// a secret.
    fn variable(&self, name: &str) -> Option<String> {
        Reflect::get(&self.env, &JsValue::from_str(name))
            .ok()
            .and_then(|value| value.as_string())
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
        let now = u64::try_from(wall_clock()?).ok()?;
        Some(MONOTONIC.with(|last| {
            let now = now.max(last.get());
            last.set(now);
            now
        }))
    }

    fn now_ms(&self) -> Option<i64> {
        wall_clock()
    }

    /// Counted in the isolate's memory ([`isolate_limits`]).
    fn rate_limit<'a>(
        &'a self,
        key: &'a str,
        limit: RateLimit,
        now: i64,
    ) -> LocalFuture<'a, Allowed> {
        let allowed = isolate_limits().allow(key, limit, now);
        Box::pin(async move { allowed })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Background work has Workers' budgets, none past the 15-minute limit
    /// of a run.
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
