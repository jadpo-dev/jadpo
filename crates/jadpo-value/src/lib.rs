//! Runtime values, the operators and the standard library's operations
//! (docs/compiler-runtime.md §2.1, §2.5 and §2.6).
//!
//! Every standard-library operation is listed once, typed, in `jadpo-std`'s
//! table, declared for programs in a Jadpo stub in `jadpo-check`, and
//! implemented once, here, as one Rust function that both the interpreter
//! and compiled code call. [`run`] matches on the table's [`Op`]
//! exhaustively, so an operation without an implementation does not
//! compile. [`ops`] defines the operators once, with checked arithmetic,
//! and a [`Validator`] checks a value against its type's rules with the
//! same validators the checker runs on literals.
//!
//! [`Value`] is shared behind `Rc`: values never change (area 2) and the
//! runtime is single-threaded (§2.6), so copying a value is a reference
//! count.
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
pub mod ops;
pub mod order;
mod rounding;
pub mod validate;
mod value;

pub use jadpo_std::{Op, Rounding};
pub use ops::Fault;
pub use validate::{Invalid, Validator};
pub use value::{Decimal, QUOTIENT_PLACES, Record, Value, Variant};

/// Runs one standard-library operation with the values of its parameters
/// in table order, its inline-function parameter left out; `each` runs the
/// call's inline function on one item, and gives no value when it stopped
/// (the caller knows why: it is [`Fault::Stopped`] here).
pub fn run(
    op: Op,
    args: &[Value],
    each: &mut dyn FnMut(&Value) -> Option<Value>,
) -> Result<Value, Fault> {
    match (op, args) {
        (Op::CollectionContains, [Value::List(items), item]) => {
            Ok(Value::Bool(collection::contains(items, item)))
        }
        (Op::CollectionCount, [Value::List(items)]) => Ok(Value::Int(collection::count(items))),
        (Op::CollectionFilter, [Value::List(items)]) => {
            collection::filter(items, each).map(Value::list)
        }
        (Op::CollectionMap, [Value::List(items)]) => collection::map(items, each).map(Value::list),
        (Op::DecimalDivide, [dividend, divisor, rounding]) => {
            let decimal = |value: &Value| match value {
                Value::Decimal(d) => Ok(*d),
                Value::Int(n) => Ok(Decimal::from_int(*n)),
                _ => Err(Fault::Shape),
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
        (
            Op::CollectionContains
            | Op::CollectionCount
            | Op::CollectionFilter
            | Op::CollectionMap
            | Op::DecimalDivide
            | Op::IntegerDivide,
            _,
        ) => Err(Fault::Shape),
    }
}

/// A `Rounding` value as the runtime holds it: a plain variant.
fn rounding_of(value: &Value) -> Result<Rounding, Fault> {
    match value {
        Value::Variant(variant) if variant.fields.is_empty() => {
            Rounding::from_name(&variant.name).ok_or(Fault::Shape)
        }
        _ => Err(Fault::Shape),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn variant(name: &str) -> Value {
        let ordinal = jadpo_std::variant_ordinal(Rounding::NAME, name).unwrap_or(0);
        Value::variant(Rounding::NAME, ordinal, name, Vec::new())
    }

    #[test]
    fn every_operation_in_the_table_runs() {
        let items = Value::list(vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
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
            assert!(result.is_ok(), "{} runs", operation.qualified());
        }
        assert_eq!(
            run(Op::CollectionMap, std::slice::from_ref(&items), &mut double),
            Ok(Value::list(vec![
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
            Ok(Value::Decimal(Decimal::new(333, 2)))
        );
        assert_eq!(
            run(
                Op::IntegerDivide,
                &[Value::Int(1), Value::Int(0), variant("up")],
                &mut double
            ),
            Err(Fault::DivisionByZero),
            "a zero divisor is a fault"
        );
        assert_eq!(
            run(Op::CollectionCount, &[Value::Int(1)], &mut double),
            Err(Fault::Shape)
        );
    }
}
