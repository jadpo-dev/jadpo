//! What a running Jadpo program needs from its context
//! (docs/compiler-runtime.md §2.5 and §2.6), shared by the interpreter and
//! compiled code so both name the same places, stop the same way and end
//! in the same outcomes.
//!
//! - [`Ctx`]: the per-invocation context an executor is handed.
//!   `enter(callable, site)` and `leave` keep a shadow stack of
//!   [`Frame`]s, `fail` and `fault` capture it, so nothing depends on
//!   unwinding or on debug information; an `assert` records its parts
//!   through it (`assertion_begin`, `record`, `assertion_end`), so both
//!   executors feed one recorder.
//! - [`action`]: the action boundary, which the runtime owns: the
//!   transaction opened at the first store operation, at the
//!   [`Isolation`] the body's plan names (chunk 3.7), the body (an
//!   [`ActionBody`] either executor supplies) rerun on a concurrent change
//!   within [`RERUN_ATTEMPTS`], the commit, then the events delivered.
//! - [`Host`] and the capability traits ([`Database`], [`Transaction`]),
//!   with [`MemoryHost`], which holds no store, until the drivers come.
//! - [`Env`]: what one invocation runs under: its seed and clock.
//! - [`Outcome`]: how a call ends, and [`Report`], what a person reads
//!   for it, rendered from [`Places`] and the sources alone.
//! - [`Executor`]: what the test runner calls, and [`block_on`], which
//!   drives one invocation on its thread.
//! - [`Failure`], [`Fault`] and [`MAX_DEPTH`]: declared failures, what
//!   the runtime raises itself, and its limits.
//! - [`store`]: the one entry point for a query's or an entity's read
//!   (chunk 3.2) and a `create`, `update` or `delete` (chunk 3.3), run from
//!   their plans in the action's transaction or a boundary of its own, and
//!   giving the value, the failure its plan names, or the `conflict:` arm
//!   its site maps a refusal to (chunk 3.12); [`purge`], a lifecycle's
//!   permanent deletion of hidden records (chunk 3.4), outside any action.
//!
//! - [`http`]: the HTTP component (chunk 4.1): a request routed by the
//!   route table, its placeholders and input decoded, what the route runs
//!   run, and the output or the failure envelope written, with the default
//!   rate limits ([`RateLimits`]).
//! - [`effects`]: the one entry point for operations with effects
//!   (chunk 7.0b): `clock.now`, `random`, `crypto` with its key,
//!   `json.read` and `json.write` with the plan's codec, and `emit`, which
//!   both executors call; and since chunk 7.0c every call that leaves the
//!   process ([`effects::call`]), through the [`capability`] seam.
//! - [`runner`]: the runner (chunk 6.1a), which drives an executor for
//!   work no request waits on, as the system ([`System`]): deferred until
//!   after a response, while a route waits for it, or once per item of a
//!   batch, each invocation within the [`budget`] its host gives it and
//!   each claim it takes a [`lease`] of the same length (chunk 6.7).
//! - [`serve`]: what every host does with a request and a tick (chunk
//!   6.9), which a host's server handler wraps.
//!
//! Built so far: pure execution (chunk 2.1) and the executor shape Phase 3
//! plugs into (chunk 2.10). Drivers, hosts and the components of §2.6 come
//! with their areas.

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

pub mod batches;
mod boundary;
pub mod budget;
pub mod capability;
pub mod config;
mod ctx;
pub mod effects;
mod env;
mod executor;
mod failure;
mod fault;
mod host;
pub mod http;
pub mod lease;
mod limits;
mod migrate;
pub mod outbox;
mod outcome;
mod places;
mod policy;
mod purge;
mod race;
mod ratelimit;
mod read;
mod report;
pub mod runner;
pub mod schedules;
pub mod serve;
pub mod statements;
mod store;
mod write;

pub use boundary::{
    ActionBody, RERUN_ATTEMPTS, action, commit_held, misconfigured, outcome_unknown, unavailable,
};
pub use ctx::{Caller, Capabilities, Ctx, EmitRefused, Held, SessionEffect, System};
pub use env::{Clock, Env};
pub use executor::{Executor, Stalled, YieldNow, block_on, yield_now};
pub use failure::Failure;
pub use fault::{Fault, FaultKind};
pub use host::{
    Database, Event, Host, LocalFuture, MemoryHost, NoStore, PasskeyAnswer, PasskeyError,
    PasskeyParts, Passkeys, RELEASE_SAVEPOINT, ROLLBACK_TO_SAVEPOINT, Row, SAVEPOINT, SqlType,
    SqlValue, StoreError, Transaction,
};
pub use jadpo_std::Isolation;
pub use jadpo_std::geography;
pub use limits::{JSON_KEPT_DEPTH, JSON_KEPT_LIMIT, MAX_DEPTH};
pub use migrate::{
    Backfills, MIGRATIONS_TABLE, MigrateError, Migration, NoBackfills, Progress, STEPS_TABLE,
    Standing, Unworked, has_extensions, migrate, migrate_reporting, migrate_with, standing,
};
pub use outcome::{Assertion, AssertionDetail, Got, Outcome, Part};
pub use places::{CallablePlace, Frame, Place, Places, Repeated, SitePlace};
pub use policy::{admit, call_as};
pub use purge::{PURGE_BATCHES, Purged, purge, purge_after};
pub use ratelimit::{Allowed, RateLimits};
pub use read::{
    JsonCodecs, PAGE, cursor, field_value, from_sql, held_to, invalid_cursor, kept_by,
    not_permitted, stored_value, to_sql, uuid_text,
};
pub use report::{Detail, Location, Report, ReportRule, Sources, fields_text};
pub use store::{StoreOp, Stored, store};
