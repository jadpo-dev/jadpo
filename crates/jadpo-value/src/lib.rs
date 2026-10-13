//! Runtime values and the standard library's operations
//! (docs/compiler-runtime.md §2.1 and §2.6).
//!
//! Every standard-library operation is listed once, typed, in `jadpo-std`'s
//! table, declared for programs in a Jadpo stub in `jadpo-check`, and
//! implemented once, here, as one Rust function that both the interpreter
//! and compiled code call. [`run`] matches on the table's [`Op`]
//! exhaustively, so an operation without an implementation does not
//! compile. [`validate`] checks a value against its type's rules with the
//! same validators the checker runs on literals.
//!
//! This crate builds for `wasm32-unknown-unknown` and depends on no other
//! workspace crate than `jadpo-std`, the leaf it shares with the compiler.

// A panic aborts a Workers instance, so runtime code reports faults as
// values (docs/compiler-runtime.md §2.6); tests may still unwrap.
#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

pub mod collection;
pub mod decimal;
pub mod integer;
mod rounding;
pub mod validate;
mod value;

pub use jadpo_std::{Op, Rounding};
pub use value::{Decimal, Value};

/// Runs one standard-library operation with `args` in its table order;
/// `each` runs the call's inline function on one item. `None` is a fault:
/// an overflow, a zero divisor the checker could not rule out, or
/// arguments the checker would have refused.
pub fn run(op: Op, args: &[Value], each: &mut dyn FnMut(&Value) -> Option<Value>) -> Option<Value> {
    match (op, args) {
        (Op::CollectionContains, [Value::List(items), item]) => {
            Some(Value::Bool(collection::contains(items, item)))
        }
        (Op::CollectionCount, [Value::List(items)]) => Some(Value::Int(collection::count(items))),
        (Op::CollectionFilter, [Value::List(items)]) => {
            let mut kept = Vec::new();
            for item in items {
                if each(item)? == Value::Bool(true) {
                    kept.push(item.clone());
                }
            }
            Some(Value::List(kept))
        }
        (Op::CollectionMap, [Value::List(items)]) => {
            let mut mapped = Vec::with_capacity(items.len());
            for item in items {
                mapped.push(each(item)?);
            }
            Some(Value::List(mapped))
        }
        (Op::DecimalDivide, [dividend, divisor, rounding]) => {
            let decimal = |value: &Value| match value {
                Value::Decimal(d) => Some(*d),
                Value::Int(n) => Some(Decimal::new(i128::from(*n), 0)),
                _ => None,
            };
            decimal::divide(
                decimal(dividend)?,
                decimal(divisor)?,
                rounding_of(rounding)?,
            )
            .map(Value::Decimal)
        }
        (Op::IntegerDivide, [Value::Int(dividend), Value::Int(divisor), rounding]) => {
            integer::divide(*dividend, *divisor, rounding_of(rounding)?).map(Value::Int)
        }
        _ => None,
    }
}

/// A `Rounding` value as the runtime holds it: a plain variant.
fn rounding_of(value: &Value) -> Option<Rounding> {
    match value {
        Value::Variant(name, fields) if fields.is_empty() => Rounding::from_name(name),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    fn variant(name: &str) -> Value {
        Value::Variant(name.to_string(), BTreeMap::new())
    }

    #[test]
    fn every_operation_in_the_table_runs() {
        let items = Value::List(vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
        let mut double = |item: &Value| match item {
            Value::Int(n) => Some(Value::Int(n * 2)),
            _ => None,
        };
        for operation in jadpo_std::OPERATIONS {
            let args: Vec<Value> = match operation.op {
                Op::CollectionContains => vec![items.clone(), Value::Int(2)],
                Op::CollectionCount | Op::CollectionFilter | Op::CollectionMap => {
                    vec![items.clone()]
                }
                Op::DecimalDivide => vec![
                    Value::Decimal(Decimal::new(1000, 2)),
                    Value::Int(3),
                    variant("half_even"),
                ],
                Op::IntegerDivide => vec![Value::Int(25), Value::Int(10), variant("up")],
            };
            let result = run(operation.op, &args, &mut |item| {
                Some(Value::Bool(matches!(item, Value::Int(n) if n % 2 == 1)))
            });
            assert!(result.is_some(), "{} runs", operation.qualified());
        }
        assert_eq!(
            run(Op::CollectionMap, std::slice::from_ref(&items), &mut double),
            Some(Value::List(vec![
                Value::Int(2),
                Value::Int(4),
                Value::Int(6)
            ]))
        );
        assert_eq!(
            run(
                Op::DecimalDivide,
                &[
                    Value::Decimal(Decimal::new(1000, 2)),
                    Value::Int(3),
                    variant("half_even")
                ],
                &mut double
            ),
            Some(Value::Decimal(Decimal::new(333, 2)))
        );
        assert_eq!(
            run(
                Op::IntegerDivide,
                &[Value::Int(1), Value::Int(0), variant("up")],
                &mut double
            ),
            None,
            "a zero divisor is a fault"
        );
    }
}
