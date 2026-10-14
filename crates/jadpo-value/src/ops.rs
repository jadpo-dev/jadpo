//! Operators on values (area 3, "Expressions and operators"), defined once
//! for both executors: arithmetic is checked, so overflow is a [`Fault`]
//! and never a wrap; `Int` division drops the remainder as Rust's does;
//! `Decimal` division is exact to [`crate::QUOTIENT_PLACES`] past its
//! operands and is rounded where it is stored; a plain `Int` meeting a
//! `Decimal` is the same number as a `Decimal`; `==` compares contents.
//!
//! The checker has already decided which operands an operator takes, so a
//! value of any other shape is [`Fault::Shape`], a defect.

use std::cmp::Ordering;

use crate::{Decimal, Value};

/// Why an operation gives no value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fault {
    /// An `Int` result past the 64-bit range.
    IntOverflow,
    /// A `Decimal` result past 38 digits.
    DecimalOverflow,
    /// A zero divisor, which the checker rules out (area 3, question 7).
    DivisionByZero,
    /// Operands of a shape the checker would have refused.
    Shape,
    /// A standard-library operation's inline function stopped without a
    /// value; the caller knows why.
    Stopped,
}

pub fn int_add(a: i64, b: i64) -> Result<i64, Fault> {
    a.checked_add(b).ok_or(Fault::IntOverflow)
}

pub fn int_subtract(a: i64, b: i64) -> Result<i64, Fault> {
    a.checked_sub(b).ok_or(Fault::IntOverflow)
}

pub fn int_multiply(a: i64, b: i64) -> Result<i64, Fault> {
    a.checked_mul(b).ok_or(Fault::IntOverflow)
}

/// `a / b`, dropping the remainder: `10 / 3` is `3`, `-7 / 2` is `-3`.
pub fn int_divide(a: i64, b: i64) -> Result<i64, Fault> {
    if b == 0 {
        return Err(Fault::DivisionByZero);
    }
    a.checked_div(b).ok_or(Fault::IntOverflow)
}

/// `a % b`, with the sign of `a`, as Rust's `%`.
pub fn int_remainder(a: i64, b: i64) -> Result<i64, Fault> {
    if b == 0 {
        return Err(Fault::DivisionByZero);
    }
    a.checked_rem(b).ok_or(Fault::IntOverflow)
}

pub fn int_negate(a: i64) -> Result<i64, Fault> {
    a.checked_neg().ok_or(Fault::IntOverflow)
}

/// Both operands as decimals when either is one.
fn decimals(a: &Value, b: &Value) -> Option<(Decimal, Decimal)> {
    let decimal = |value: &Value| match value {
        Value::Decimal(d) => Some(*d),
        Value::Int(n) => Some(Decimal::from_int(*n)),
        _ => None,
    };
    Some((decimal(a)?, decimal(b)?))
}

fn decimal_result(result: Option<Decimal>) -> Result<Value, Fault> {
    result.map(Value::Decimal).ok_or(Fault::DecimalOverflow)
}

/// `a + b`: numbers, or two texts joined.
pub fn add(a: &Value, b: &Value) -> Result<Value, Fault> {
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => int_add(*x, *y).map(Value::Int),
        (Value::Text(x), Value::Text(y)) => Ok(Value::text(&format!("{x}{y}"))),
        _ => {
            let (x, y) = decimals(a, b).ok_or(Fault::Shape)?;
            decimal_result(x.checked_add(y))
        }
    }
}

pub fn subtract(a: &Value, b: &Value) -> Result<Value, Fault> {
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => int_subtract(*x, *y).map(Value::Int),
        _ => {
            let (x, y) = decimals(a, b).ok_or(Fault::Shape)?;
            decimal_result(x.checked_sub(y))
        }
    }
}

pub fn multiply(a: &Value, b: &Value) -> Result<Value, Fault> {
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => int_multiply(*x, *y).map(Value::Int),
        _ => {
            let (x, y) = decimals(a, b).ok_or(Fault::Shape)?;
            decimal_result(x.checked_mul(y))
        }
    }
}

pub fn divide(a: &Value, b: &Value) -> Result<Value, Fault> {
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => int_divide(*x, *y).map(Value::Int),
        _ => {
            let (x, y) = decimals(a, b).ok_or(Fault::Shape)?;
            if y.is_zero() {
                return Err(Fault::DivisionByZero);
            }
            decimal_result(x.quotient(y))
        }
    }
}

/// `a % b`, on `Int` values only (area 3, question 7).
pub fn remainder(a: &Value, b: &Value) -> Result<Value, Fault> {
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => int_remainder(*x, *y).map(Value::Int),
        _ => Err(Fault::Shape),
    }
}

/// `-a`.
pub fn negate(a: &Value) -> Result<Value, Fault> {
    match a {
        Value::Int(n) => int_negate(*n).map(Value::Int),
        Value::Decimal(d) => decimal_result(d.checked_neg()),
        _ => Err(Fault::Shape),
    }
}

/// `a == b`: contents compared, and a plain `Int` equal to the same
/// number as a `Decimal`.
pub fn equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Int(_), Value::Decimal(_)) | (Value::Decimal(_), Value::Int(_)) => {
            decimals(a, b).is_some_and(|(x, y)| x == y)
        }
        _ => a == b,
    }
}

/// The order of two numbers or two time values of the same kind; `None`
/// for anything else.
pub fn compare(a: &Value, b: &Value) -> Option<Ordering> {
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => Some(x.cmp(y)),
        (Value::Instant(x), Value::Instant(y)) | (Value::Duration(x), Value::Duration(y)) => {
            Some(x.cmp(y))
        }
        (Value::Date(x), Value::Date(y)) => Some(x.cmp(y)),
        (Value::Time(x), Value::Time(y)) => Some(x.cmp(y)),
        _ => decimals(a, b).map(|(x, y)| x.cmp(&y)),
    }
}

/// `item in items`: whether a list or set holds the item, or a map has it
/// as a key.
pub fn contains(items: &Value, item: &Value) -> Option<bool> {
    match items {
        Value::List(items) | Value::Set(items) => Some(items.iter().any(|i| equal(i, item))),
        Value::Map(entries) => Some(entries.contains_key(item)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decimal(text: &str) -> Value {
        Value::decimal(text).expect("a decimal")
    }

    #[test]
    fn arithmetic_is_checked() {
        assert_eq!(add(&Value::Int(2), &Value::Int(3)), Ok(Value::Int(5)));
        assert_eq!(
            multiply(&Value::Int(i64::MAX), &Value::Int(2)),
            Err(Fault::IntOverflow)
        );
        assert_eq!(negate(&Value::Int(i64::MIN)), Err(Fault::IntOverflow));
        assert_eq!(divide(&Value::Int(10), &Value::Int(3)), Ok(Value::Int(3)));
        assert_eq!(divide(&Value::Int(-7), &Value::Int(2)), Ok(Value::Int(-3)));
        assert_eq!(
            divide(&Value::Int(i64::MIN), &Value::Int(-1)),
            Err(Fault::IntOverflow)
        );
        assert_eq!(
            remainder(&Value::Int(-7), &Value::Int(2)),
            Ok(Value::Int(-1))
        );
        assert_eq!(
            divide(&Value::Int(1), &Value::Int(0)),
            Err(Fault::DivisionByZero)
        );
        assert_eq!(add(&decimal("1.25"), &Value::Int(1)), Ok(decimal("2.25")));
        assert_eq!(
            add(&Value::text("a"), &Value::text("b")),
            Ok(Value::text("ab"))
        );
        assert_eq!(
            remainder(&decimal("1.5"), &Value::Int(1)),
            Err(Fault::Shape)
        );
    }

    #[test]
    fn comparison_and_equality_read_numbers_by_value() {
        assert!(equal(&decimal("1.0"), &Value::Int(1)));
        assert!(!equal(&Value::text("1"), &Value::Int(1)));
        assert_eq!(
            compare(&decimal("1.5"), &Value::Int(2)),
            Some(Ordering::Less)
        );
        assert_eq!(
            compare(&Value::Instant(5), &Value::Instant(4)),
            Some(Ordering::Greater)
        );
        assert_eq!(compare(&Value::text("a"), &Value::text("b")), None);
        let items = Value::list(vec![Value::Int(1), Value::Int(2)]);
        assert_eq!(contains(&items, &Value::Int(2)), Some(true));
        assert_eq!(contains(&Value::Int(1), &Value::Int(2)), None);
    }
}
