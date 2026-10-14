//! The interpreter (docs/compiler-runtime.md §2.5): runs a checked
//! project's typed bodies (`jadpo_graph::body`, built in chunk 1.8) on the
//! runtime it shares with compiled code (`jadpo-runtime`, `jadpo-value`),
//! so `jadpo test`, `jadpo dev` and `jadpo call` run a save in
//! milliseconds.
//!
//! [`Program::load`] reads a project that checked without errors;
//! [`Program::call`] runs one callable with argument values and gives its
//! [`Outcome`]: its value, the declared failure that left it, a fault, or a
//! test's failed assertion. A fault or a failure carries the frames of the
//! `.jadpo` sites it passed through, and [`Program::report`] renders it as
//! a person reads it, on the grid every Jadpo error shares.
//!
//! Built in chunk 2.1: pure logic (areas 2 to 4) in functions, library
//! functions, and the parts of actions, fixtures and tests that compute.
//! Effects later areas build (an entity's operations, queries, `emit`,
//! `clock.now`) end in the `not_built_yet` fault where they are reached.

mod eval;
mod program;
mod report;
mod types;

pub use jadpo_runtime::{Failure, Fault, FaultKind, Frame, MAX_DEPTH, Places};
pub use jadpo_value::Value;
pub use program::{CallError, Program};

pub use report::Report;

/// How a call ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// It gave its result; `Unit` for an action that returns nothing.
    Value(Value),
    /// A declared failure left it (area 4).
    Failed(Box<Failure>),
    /// The runtime stopped it (area 4, "Faults").
    Fault(Box<Fault>),
    /// A test's `assert` didn't hold (area 13).
    Assertion(Box<Assertion>),
}

/// A test's `assert` that didn't hold, and where.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Assertion {
    /// The frames at the `assert`, innermost first.
    pub frames: Vec<Frame>,
    pub detail: AssertionDetail,
    /// The value of each sub-expression of the condition the assertion
    /// worked out, in the order their anchors are written (area 13: "every
    /// sub-expression's value, as pytest and power-assert show").
    /// Literals are left out, and so is anything inside a callee or an
    /// inline function, which has no single value here.
    pub parts: Vec<Part>,
}

/// One sub-expression of a failed assertion and its value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Part {
    /// Its bytes in the test's file.
    pub start: u32,
    pub end: u32,
    /// Where its value points: an operator's first byte for arithmetic,
    /// comparisons and `and`/`or`, a field's name for a field read,
    /// otherwise its start.
    pub anchor: u32,
    pub value: Value,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AssertionDetail {
    /// `assert condition` was false; for a comparison, its operator and
    /// the value of each side (area 13, question 6).
    False {
        comparison: Option<(&'static str, Value, Value)>,
    },
    /// `assert call(...) fails Expected`, and the call did something else.
    Fails { expected: String, got: Got },
}

/// What a call did instead of failing as expected.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Got {
    Value(Value),
    Failure(Box<Failure>),
}

/// How much stack [`on_large_stack`] gives: the interpreter walks a body
/// on the Rust stack, a few frames for every Jadpo frame, and a call stack
/// may reach [`MAX_DEPTH`] Jadpo frames.
pub const STACK_BYTES: usize = 512 * 1024 * 1024;

/// Runs `work` on a thread with [`STACK_BYTES`] of stack, so recursion to
/// the runtime's depth limit ends in its fault rather than in a stack
/// overflow. Values are shared behind `Rc` and never cross threads, so
/// `work` loads the program and makes its values itself. A panic in `work`
/// carries on in the caller.
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
