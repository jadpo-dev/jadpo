//! The interpreter (docs/compiler-runtime.md §2.5): runs a checked
//! project's typed bodies (`jadpo_graph::body`) with its plan
//! (`jadpo-plan`) on the runtime it shares with compiled code
//! (`jadpo-runtime`, `jadpo-value`), so `jadpo test`, `jadpo dev` and
//! `jadpo call` run a save in milliseconds.
//!
//! [`Prepared`] reads a project that checked without errors, once, for
//! every thread; [`Program`] is one thread's view of it and the runtime's
//! [`Executor`]: it runs a callable against a context the caller hands it
//! and gives its [`Outcome`]. One walker ([`machine`]) runs every body:
//! functions synchronously, and actions, fixtures and tests until a site
//! the typed body says can pause, where it waits on the runtime (a call
//! into the action boundary, or a read or write through its store entry
//! point) and carries on. [`Program::call`] runs one callable on a host with no store,
//! and [`Program::report`] renders what it gave as a person reads it.
//!
//! Built in chunk 2.1 for pure logic and reshaped in chunk 2.10 for Phase
//! 3, which added queries, `get`, writes, lifecycles and `clock.now`.
//! What later areas build (`emit` and `matches` among it) ends in the
//! `not_built_yet` fault where it is reached.

mod drive;
mod machine;
mod probe;
mod program;
mod values;

pub use jadpo_runtime::{
    Assertion, AssertionDetail, Executor, Failure, Fault, FaultKind, Frame, Got, MAX_DEPTH,
    Outcome, Part, Places, Report,
};
pub use jadpo_value::Value;
pub use program::{CallError, Prepared, Program};
pub use values::Codec;

/// How much stack [`on_large_stack`] gives. The walker keeps its own
/// stacks, so a recursion to the runtime's depth limit doesn't grow the
/// thread's; only an inline function's body, run inside a collection
/// operation, does, a few frames per level.
pub const STACK_BYTES: usize = 64 * 1024 * 1024;

/// Runs `work` on a thread with [`STACK_BYTES`] of stack, so recursion to
/// the runtime's depth limit through inline functions ends in its fault
/// rather than a stack overflow. Values are shared behind `Rc` and never
/// cross threads, so `work` makes its program and values itself. A panic in
/// `work` carries on in the caller.
pub fn on_large_stack<T: Send>(work: impl FnOnce() -> T + Send) -> std::io::Result<T> {
    std::thread::scope(|scope| {
        let worker = std::thread::Builder::new()
            .name("jadpo-interp".into())
            .stack_size(STACK_BYTES)
            .spawn_scoped(scope, work)?;
        match worker.join() {
            Ok(value) => Ok(value),
            Err(panic) => std::panic::resume_unwind(panic),
        }
    })
}
