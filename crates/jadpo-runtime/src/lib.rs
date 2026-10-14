//! What a running Jadpo program needs from its context
//! (docs/compiler-runtime.md §2.5 and §2.6), shared by the interpreter and
//! compiled code so both name the same places and stop the same way.
//!
//! - [`Ctx`]: the per-invocation context. `enter(callable, site)` and
//!   `leave` keep a shadow stack of [`Frame`]s, `fail` and `fault` capture
//!   it, so nothing depends on unwinding or on debug information.
//! - [`Places`]: each callable's label, file and the line of every site,
//!   from the graph: what turns a frame into `price_line.jadpo:4  action
//!   price_line` (area 17).
//! - [`Failure`]: a declared failure raised with `reject`, passed on with
//!   `attempt` or `propagate`, mapped with its cause kept (area 4).
//! - [`Fault`]: what the runtime raises itself: overflow, the recursion
//!   limit, something a later area builds, or a defect (area 4, question
//!   9).
//! - [`MAX_DEPTH`]: the runtime's limits, as constants.
//!
//! Built so far: what pure execution needs (chunk 2.1). Transactions,
//! drivers and hosts come with their areas.

// A panic aborts a Workers instance, so runtime code reports faults as
// values (docs/compiler-runtime.md §2.6); tests may still unwrap.
#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

mod ctx;
mod failure;
mod fault;
mod limits;
mod places;

pub use ctx::Ctx;
pub use failure::Failure;
pub use fault::{Fault, FaultKind};
pub use limits::MAX_DEPTH;
pub use places::{CallablePlace, Frame, Place, Places};
