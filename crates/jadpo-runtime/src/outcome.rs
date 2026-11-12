//! How a call ends, the same whichever executor ran it, so the test runner
//! and the differential run (docs/compiler-runtime.md §2.5) compare one
//! shape: its value, the declared failure that left it, a fault, or a
//! test's failed assertion with the value of each part of its condition.

use jadpo_value::Value;

use crate::{Failure, Fault, Frame};

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
    /// the value of each side (area 13, "Assertions").
    False {
        comparison: Option<(&'static str, Value, Value)>,
    },
    /// `assert call(...) fails Expected`, and the call did something else.
    Fails { expected: String, got: Got },
    /// `assert emitted Event { ... }` (area 13, "Events and query cost"),
    /// and no event of that type the test's committed steps kept had the
    /// fields it gives: `event`, the type's name now, the fields given
    /// with their values in the order written, and every event of that
    /// type that was kept, in the order they committed.
    Emitted {
        event: String,
        given: Vec<(String, Value)>,
        emitted: Vec<Value>,
    },
}

/// What `assert emitted` expected, as a report shows it: the event's type
/// with the fields it gives, `OrderPlaced { order_id: 7 }`.
pub fn emitted_expected(event: &str, given: &[(String, Value)]) -> String {
    let fields: Vec<String> = given
        .iter()
        .map(|(name, value)| format!("{name}: {value}"))
        .collect();
    if fields.is_empty() {
        format!("an `{event}`")
    } else {
        format!("{event} {{ {} }}", fields.join(", "))
    }
}

/// What the test's steps emitted of `event`'s type instead, as a report
/// shows it: `no OrderPlaced emitted`, or each one kept, in order.
pub fn emitted_actual(event: &str, emitted: &[Value]) -> String {
    if emitted.is_empty() {
        return format!("no {event} emitted");
    }
    emitted
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

/// What a call did instead of failing as expected.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Got {
    Value(Value),
    Failure(Box<Failure>),
}
