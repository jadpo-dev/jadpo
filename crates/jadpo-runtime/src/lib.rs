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
//!   transaction opened at the first store operation, the body (an
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
//!
//! Built so far: pure execution (chunk 2.1) and the executor shape Phase 3
//! plugs into (chunk 2.10). Drivers, hosts and the components of §2.6 come
//! with their areas.

// A panic aborts a Workers instance, so runtime code reports faults as
// values (docs/compiler-runtime.md §2.6); tests may still unwrap.
#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

mod boundary;
mod ctx;
mod env;
mod executor;
mod failure;
mod fault;
mod host;
mod limits;
mod migrate;
mod outcome;
mod places;
mod report;

pub use boundary::{ActionBody, RERUN_ATTEMPTS, action, unavailable};
pub use ctx::Ctx;
pub use env::{Clock, Env};
pub use executor::{Executor, Stalled, block_on};
pub use failure::Failure;
pub use fault::{Fault, FaultKind};
pub use host::{
    Database, Event, Host, LocalFuture, MemoryHost, NoStore, Row, SqlType, SqlValue, StoreError,
    Transaction,
};
pub use limits::MAX_DEPTH;
pub use migrate::{MIGRATIONS_TABLE, MigrateError, Migration, Standing, migrate, standing};
pub use outcome::{Assertion, AssertionDetail, Got, Outcome, Part};
pub use places::{CallablePlace, Frame, Place, Places, Repeated, SitePlace};
pub use report::{Detail, Location, Report, ReportRule, Sources, fields_text};
