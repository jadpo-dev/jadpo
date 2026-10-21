//! The binary operators both executors run (docs/compiler-runtime.md
//! §2.5, "Compiled code"; chunk 8.1f): comparisons, membership and checked
//! arithmetic over `jadpo-value`'s one implementation, so the interpreter
//! and compiled code can't disagree on what `+` or `<` gives. `and` and
//! `or` aren't here: they stop as soon as their answer is known, which is
//! control flow, each executor's own. Neither is `matches`, a query's
//! condition that no body runs yet.

use jadpo_value::{Fault, Rounding, Value, ops};

/// A binary operator a body runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Operator {
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    /// `item in collection`.
    In,
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
}

impl Operator {
    /// A comparison's operator as written, for a failed assertion's
    /// report; empty for one that isn't a comparison.
    pub fn symbol(self) -> &'static str {
        match self {
            Operator::Equal => "==",
            Operator::NotEqual => "!=",
            Operator::Less => "<",
            Operator::LessEqual => "<=",
            Operator::Greater => ">",
            Operator::GreaterEqual => ">=",
            Operator::In
            | Operator::Add
            | Operator::Subtract
            | Operator::Multiply
            | Operator::Divide
            | Operator::Remainder => "",
        }
    }
}

/// `left op right`. An amount scaled by a number rounds by `rounding`,
/// its type's `rounding:` (area 19). Overflow is the operator's fault,
/// which the executor raises at the operator's site; two amounts in
/// different currencies are [`Fault::Broke`], which the operator's
/// `attempt` maps.
pub fn operate(
    op: Operator,
    left: &Value,
    right: &Value,
    rounding: Rounding,
) -> Result<Value, Fault> {
    let ordered = |wanted: fn(std::cmp::Ordering) -> bool| {
        ops::order(left, right).map(|ordering| Value::Bool(wanted(ordering)))
    };
    match op {
        Operator::Equal => Ok(Value::Bool(ops::equal(left, right))),
        Operator::NotEqual => Ok(Value::Bool(!ops::equal(left, right))),
        Operator::Less => ordered(std::cmp::Ordering::is_lt),
        Operator::LessEqual => ordered(std::cmp::Ordering::is_le),
        Operator::Greater => ordered(std::cmp::Ordering::is_gt),
        Operator::GreaterEqual => ordered(std::cmp::Ordering::is_ge),
        Operator::Add => ops::add(left, right),
        Operator::Subtract => ops::subtract(left, right),
        Operator::Multiply => ops::multiply_rounded(left, right, rounding),
        Operator::Divide => ops::divide_rounded(left, right, rounding),
        Operator::Remainder => ops::remainder(left, right),
        Operator::In => ops::contains(right, left)
            .map(Value::Bool)
            .ok_or(Fault::Shape),
    }
}
