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
    /// An `Instant` or a `CalendarDate` outside the years 0000 to 9999
    /// (area 10).
    TimeRange,
    /// A condition a `temporal` operation fails on, which the call's
    /// `attempt` maps by this key, as a construction's broken rule
    /// (area 10, question 7).
    Broke(jadpo_std::RuleKey),
    /// No random bytes to give: a host with no generator and no seed, which
    /// never falls back to a weaker source (area 19, "Randomness").
    NoRandomness,
    /// `random.integer` given a `min` above its `max`.
    EmptyRange,
    /// `temporal.occurrences` asked for the instants of a recurrence on
    /// dates, which has days and no zone (chunk 5.5k4): its days are asked
    /// for from a date.
    DateOnlyRule,
    /// `geo.centroid` of a shape whose weights cancel, two points opposite
    /// each other, which leaves no direction (area 20, "Operations").
    NoCentre,
    /// `ai.distance` by cosine from a vector of zeros, which has no
    /// direction, so no angle to another (area 21, chunk 5.5v1).
    ZeroVector,
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

/// `a % b`, with the sign of `a`, as Rust's `%`. A remainder is never
/// larger than its divisor, so it never overflows: `i64::MIN % -1` is 0,
/// though `i64::MIN / -1` is past the range.
pub fn int_remainder(a: i64, b: i64) -> Result<i64, Fault> {
    if b == 0 {
        return Err(Fault::DivisionByZero);
    }
    Ok(a.wrapping_rem(b))
}

pub fn int_negate(a: i64) -> Result<i64, Fault> {
    a.checked_neg().ok_or(Fault::IntOverflow)
}

/// A number as a decimal.
fn as_decimal(value: &Value) -> Option<Decimal> {
    match value {
        Value::Decimal(d) => Some(*d),
        Value::Int(n) => Some(Decimal::from_int(*n)),
        _ => None,
    }
}

/// Both operands as decimals when either is one.
fn decimals(a: &Value, b: &Value) -> Option<(Decimal, Decimal)> {
    Some((as_decimal(a)?, as_decimal(b)?))
}

fn decimal_result(result: Option<Decimal>) -> Result<Value, Fault> {
    result.map(Value::Decimal).ok_or(Fault::DecimalOverflow)
}

/// `a + b`: numbers, two texts joined, two durations, or an instant moved
/// by a duration (area 10, "Arithmetic and comparison").
pub fn add(a: &Value, b: &Value) -> Result<Value, Fault> {
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => int_add(*x, *y).map(Value::Int),
        (Value::Instant(at), Value::Duration(by)) | (Value::Duration(by), Value::Instant(at)) => {
            moved(at.checked_add(*by))
        }
        (Value::Duration(x), Value::Duration(y)) => int_add(*x, *y).map(Value::Duration),
        (Value::Text(x), Value::Text(y)) => Ok(Value::text(&format!("{x}{y}"))),
        // A date moved by a period, and two periods added part by part
        // (area 19, the standard operator table).
        (Value::Date(day), period) if crate::calendar::is_period(period) => {
            crate::calendar::add_period(*day, crate::calendar::period_parts(period)?)
                .map(Value::Date)
        }
        (a, b) if crate::calendar::is_period(a) => crate::calendar::combine(a, b, false),
        (Value::Money(x), Value::Money(y)) => crate::money::add(*x, *y).map(Value::Money),
        (Value::Quantity(_), _) | (_, Value::Quantity(_)) => crate::units::add(a, b),
        // Two percentages add to a percentage; nothing else adds one
        // (area 19, the standard operator table).
        (Value::Percentage(x), Value::Percentage(y)) => percentage(x.checked_add(*y)),
        (Value::Percentage(_), _) | (_, Value::Percentage(_)) => Err(Fault::Shape),
        _ => {
            let (x, y) = decimals(a, b).ok_or(Fault::Shape)?;
            decimal_result(x.checked_add(y))
        }
    }
}

fn percentage(result: Option<Decimal>) -> Result<Value, Fault> {
    result.map(Value::Percentage).ok_or(Fault::DecimalOverflow)
}

/// `a - b`: numbers, two durations, an instant moved back by a duration,
/// or the duration from one instant to another.
pub fn subtract(a: &Value, b: &Value) -> Result<Value, Fault> {
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => int_subtract(*x, *y).map(Value::Int),
        (Value::Instant(at), Value::Duration(by)) => moved(at.checked_sub(*by)),
        (Value::Instant(x), Value::Instant(y)) => int_subtract(*x, *y).map(Value::Duration),
        (Value::Duration(x), Value::Duration(y)) => int_subtract(*x, *y).map(Value::Duration),
        (Value::Date(day), period) if crate::calendar::is_period(period) => {
            crate::calendar::subtract_period(*day, crate::calendar::period_parts(period)?)
                .map(Value::Date)
        }
        (a, b) if crate::calendar::is_period(a) => crate::calendar::combine(a, b, true),
        (Value::Money(x), Value::Money(y)) => crate::money::subtract(*x, *y).map(Value::Money),
        (Value::Quantity(_), _) | (_, Value::Quantity(_)) => crate::units::subtract(a, b),
        (Value::Percentage(x), Value::Percentage(y)) => percentage(x.checked_sub(*y)),
        (Value::Percentage(_), _) | (_, Value::Percentage(_)) => Err(Fault::Shape),
        _ => {
            let (x, y) = decimals(a, b).ok_or(Fault::Shape)?;
            decimal_result(x.checked_sub(y))
        }
    }
}

/// An instant moved, within the years an `Instant` holds.
fn moved(to: Option<i64>) -> Result<Value, Fault> {
    let to = to.ok_or(Fault::TimeRange)?;
    crate::temporal::instant(to).map(Value::Instant)
}

/// `a * b`: numbers, or an amount of money by a number, rounded half to
/// even unless the site says otherwise ([`multiply_rounded`]).
pub fn multiply(a: &Value, b: &Value) -> Result<Value, Fault> {
    multiply_rounded(a, b, jadpo_std::Rounding::HalfEven)
}

/// `a * b`, an amount of money rounded to a whole minor unit as
/// `rounding` says: its type's `rounding:` (area 19).
pub fn multiply_rounded(
    a: &Value,
    b: &Value,
    rounding: jadpo_std::Rounding,
) -> Result<Value, Fault> {
    match (a, b) {
        // An amount or a quantity times a percentage is times the fraction
        // it is (area 19, the standard operator table).
        (Value::Money(amount), Value::Percentage(fraction)) => {
            crate::money::multiply(*amount, &Value::Decimal(*fraction), rounding).map(Value::Money)
        }
        (Value::Money(amount), factor) => {
            crate::money::multiply(*amount, factor, rounding).map(Value::Money)
        }
        (Value::Quantity(_), Value::Percentage(fraction)) => {
            crate::units::multiply(a, &Value::Decimal(*fraction))
        }
        (Value::Quantity(_), _) | (_, Value::Quantity(_)) => crate::units::multiply(a, b),
        // A percentage times a number or a percentage is a percentage; a
        // number times a percentage is the number's kind, a `Decimal`.
        (Value::Percentage(x), Value::Percentage(y)) => percentage(x.checked_mul(*y)),
        (Value::Percentage(x), number) => {
            percentage(x.checked_mul(as_decimal(number).ok_or(Fault::Shape)?))
        }
        (number, Value::Percentage(y)) => {
            decimal_result(as_decimal(number).ok_or(Fault::Shape)?.checked_mul(*y))
        }
        (Value::Int(x), Value::Int(y)) => int_multiply(*x, *y).map(Value::Int),
        // A period's parts each multiplied (area 19).
        (period, Value::Int(n)) if crate::calendar::is_period(period) => {
            crate::calendar::times(period, *n)
        }
        _ => {
            let (x, y) = decimals(a, b).ok_or(Fault::Shape)?;
            decimal_result(x.checked_mul(y))
        }
    }
}

pub fn divide(a: &Value, b: &Value) -> Result<Value, Fault> {
    divide_rounded(a, b, jadpo_std::Rounding::HalfEven)
}

/// `a / b`; an amount of money by a decimal rate is rounded to a whole
/// minor unit as `rounding` says (its type's `rounding:`, area 19), and
/// two amounts in one currency give a `Decimal`.
pub fn divide_rounded(a: &Value, b: &Value, rounding: jadpo_std::Rounding) -> Result<Value, Fault> {
    match (a, b) {
        (Value::Money(x), Value::Money(y)) => crate::money::ratio(*x, *y),
        (Value::Money(amount), rate) => {
            crate::money::divide(*amount, rate, rounding).map(Value::Money)
        }
        (Value::Quantity(_), _) | (_, Value::Quantity(_)) => crate::units::divide(a, b),
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
/// number as a `Decimal`. A secret value equals the text written in source
/// in constant time, inside any container too ([`Value`]'s `PartialEq`,
/// chunk 5.8a).
pub fn equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Int(_), Value::Decimal(_)) | (Value::Decimal(_), Value::Int(_)) => {
            decimals(a, b).is_some_and(|(x, y)| x == y)
        }
        _ => a == b,
    }
}

/// The order `<` gives two values: [`compare`], except that two amounts
/// of money in different currencies fail `currency_mismatch` (area 19).
pub fn order(a: &Value, b: &Value) -> Result<Ordering, Fault> {
    match (a, b) {
        (Value::Money(x), Value::Money(y)) => crate::money::compare(*x, *y),
        _ => compare(a, b).ok_or(Fault::Shape),
    }
}

/// The order of two numbers, two time values of the same kind, or two
/// amounts in one currency; `None` for anything else.
pub fn compare(a: &Value, b: &Value) -> Option<Ordering> {
    match (a, b) {
        (Value::Money(x), Value::Money(y)) => crate::money::compare(*x, *y).ok(),
        (Value::Quantity(x), Value::Quantity(y)) => crate::units::compare(*x, *y),
        (Value::Percentage(x), Value::Percentage(y)) => Some(x.cmp(y)),
        (Value::Int(x), Value::Int(y)) => Some(x.cmp(y)),
        (Value::Instant(x), Value::Instant(y)) | (Value::Duration(x), Value::Duration(y)) => {
            Some(x.cmp(y))
        }
        (Value::Date(x), Value::Date(y)) => Some(x.cmp(y)),
        // A version by its precedence (SemVer 2.0.0 §11, chunk 5.5n2),
        // build metadata ignored.
        (Value::Record(x), Value::Record(y))
            if &*x.ty == jadpo_std::standard::software::SEMANTIC_VERSION && x.ty == y.ty =>
        {
            crate::semver::precedence(x, y)
        }
        // A standard type its data entry orders, held as a record of its
        // parts (`LocalTime`, `YearMonth`): field by field in declared
        // order, which is the canonical order (area 19).
        (Value::Record(x), Value::Record(y))
            if x.ty == y.ty
                && jadpo_std::standard_type(&x.ty).is_some_and(|entry| entry.ordered) =>
        {
            Some(crate::order::compare(a, b))
        }
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
            remainder(&Value::Int(i64::MIN), &Value::Int(-1)),
            Ok(Value::Int(0))
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
    fn time_arithmetic_moves_instants_by_durations() {
        let noon = Value::Instant(43_200_000);
        let hour = Value::Duration(3_600_000);
        assert_eq!(add(&noon, &hour), Ok(Value::Instant(46_800_000)));
        assert_eq!(add(&hour, &noon), Ok(Value::Instant(46_800_000)));
        assert_eq!(subtract(&noon, &hour), Ok(Value::Instant(39_600_000)));
        assert_eq!(
            subtract(&noon, &Value::Instant(0)),
            Ok(Value::Duration(43_200_000))
        );
        assert_eq!(
            subtract(&Value::Instant(0), &noon),
            Ok(Value::Duration(-43_200_000))
        );
        assert_eq!(add(&hour, &hour), Ok(Value::Duration(7_200_000)));
        assert_eq!(
            add(&Value::Instant(jadpo_std::INSTANT_MAX), &hour),
            Err(Fault::TimeRange)
        );
        assert_eq!(
            add(&Value::Instant(i64::MAX), &Value::Duration(1)),
            Err(Fault::TimeRange)
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

    /// A secret value equals the text written in source wherever the two
    /// meet: at the top, and inside a list, a set, a map's keys and values,
    /// an object and an enum's data, however deep (chunk 5.8a). The order
    /// agrees, so a set finds one by the other.
    #[test]
    fn a_secret_equals_its_text_inside_every_container() {
        let secret = || Value::secret("Password", "hunter2hunter2");
        let same = || Value::text("hunter2hunter2");
        let other = || Value::text("hunter3hunter3");
        let map = |key: Value, value: Value| {
            Value::Map(std::rc::Rc::new(
                [(key, value)]
                    .into_iter()
                    .collect::<std::collections::BTreeMap<_, _>>(),
            ))
        };
        let record = |pw: Value| Value::record("Holder", vec![("pw".into(), pw)]);
        let variant = |pw: Value| Value::variant("Outcome", 0, "held", vec![("pw".into(), pw)]);
        let shapes: [&dyn Fn(Value) -> Value; 7] = [
            &|v| v,
            &|v| Value::list(vec![v]),
            &|v| Value::set(vec![v]),
            &|v| map(v, Value::Int(1)),
            &|v| map(Value::Int(1), v),
            &|v| Value::list(vec![record(v)]),
            &|v| variant(Value::list(vec![v])),
        ];
        for (at, shape) in shapes.iter().enumerate() {
            assert!(equal(&shape(secret()), &shape(same())), "shape {at}");
            assert!(
                equal(&shape(same()), &shape(secret())),
                "shape {at}, swapped"
            );
            assert!(!equal(&shape(secret()), &shape(other())), "shape {at}");
            assert_eq!(
                crate::order::compare(&shape(secret()), &shape(same())),
                Ordering::Equal,
                "shape {at}"
            );
        }
        assert!(crate::set::contains(&[secret()], &same()));
        assert!(crate::set::contains(&[same()], &secret()));
        assert!(!crate::set::contains(&[secret()], &other()));
        assert_eq!(contains(&Value::list(vec![secret()]), &same()), Some(true));
    }
}
