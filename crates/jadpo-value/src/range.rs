//! The `range` namespace (area 19, "Ranges"): each operation once, for
//! both executors, by the range's own end rule. A `Range<T>` and an
//! `InstantRange` hold every value from their start up to, not including,
//! their end, so one whose start is its end holds nothing and overlaps
//! nothing, as PostgreSQL's ranges agree. A `Range<T>`'s end may be `none`,
//! an open end, which holds everything on its side. `DateRange` joins in
//! chunk 5.5k.

use std::cmp::Ordering;

use crate::{Fault, Value, ops};

/// A range's type, start and end, each end `None` when open.
fn ends(range: &Value) -> Result<(&str, Option<&Value>, Option<&Value>), Fault> {
    let Value::Record(record) = range else {
        return Err(Fault::Shape);
    };
    let field = |name: &str| match record.fields.iter().find(|(field, _)| &**field == name) {
        Some((_, Value::None)) => Ok(None),
        Some((_, value)) => Ok(Some(value)),
        None => Err(Fault::Shape),
    };
    Ok((&record.ty, field("start")?, field("end")?))
}

/// The order of two ends of one range type.
fn order(a: &Value, b: &Value) -> Result<Ordering, Fault> {
    ops::compare(a, b).ok_or(Fault::Shape)
}

/// Whether `value` is inside `range`: at its start or after, and before
/// its end; an open end holds everything on its side.
pub fn contains(range: &Value, value: &Value) -> Result<bool, Fault> {
    let (_, start, end) = ends(range)?;
    let after_start = match start {
        Some(start) => order(start, value)? != Ordering::Greater,
        None => true,
    };
    let before_end = match end {
        Some(end) => order(value, end)? == Ordering::Less,
        None => true,
    };
    Ok(after_start && before_end)
}

/// The later of two starts, an open one the earliest of all.
fn later<'v>(a: Option<&'v Value>, b: Option<&'v Value>) -> Result<Option<&'v Value>, Fault> {
    Ok(match (a, b) {
        (Some(a), Some(b)) => Some(if order(a, b)? == Ordering::Less { b } else { a }),
        (Some(one), None) | (None, Some(one)) => Some(one),
        (None, None) => None,
    })
}

/// The earlier of two ends, an open one the latest of all.
fn earlier<'v>(a: Option<&'v Value>, b: Option<&'v Value>) -> Result<Option<&'v Value>, Fault> {
    Ok(match (a, b) {
        (Some(a), Some(b)) => Some(if order(b, a)? == Ordering::Less { b } else { a }),
        (Some(one), None) | (None, Some(one)) => Some(one),
        (None, None) => None,
    })
}

/// The shared part of two ranges of one type, as the type, its start and
/// its end; `None` when they share no value.
#[allow(clippy::type_complexity)]
fn shared<'v>(
    a: &'v Value,
    b: &'v Value,
) -> Result<Option<(&'v str, Option<&'v Value>, Option<&'v Value>)>, Fault> {
    let ((ty, a_start, a_end), (other, b_start, b_end)) = (ends(a)?, ends(b)?);
    if ty != other {
        return Err(Fault::Shape);
    }
    let (start, end) = (later(a_start, b_start)?, earlier(a_end, b_end)?);
    let holds = match (start, end) {
        (Some(start), Some(end)) => order(start, end)? == Ordering::Less,
        _ => true,
    };
    Ok(holds.then_some((ty, start, end)))
}

/// Whether two ranges share at least one value.
pub fn overlaps(a: &Value, b: &Value) -> Result<bool, Fault> {
    Ok(shared(a, b)?.is_some())
}

/// The part two ranges share, of their range type, or `none` when they
/// don't overlap.
pub fn intersection(a: &Value, b: &Value) -> Result<Value, Fault> {
    Ok(match shared(a, b)? {
        Some((ty, start, end)) => {
            let end_value = |end: Option<&Value>| end.cloned().unwrap_or(Value::None);
            Value::record(
                ty,
                vec![
                    ("start".into(), end_value(start)),
                    ("end".into(), end_value(end)),
                ],
            )
        }
        None => Value::None,
    })
}

/// A `Range<T>` as the runtime holds it, `None` for an open end.
pub fn range_of(start: Option<Value>, end: Option<Value>) -> Value {
    Value::record(
        jadpo_std::RANGE,
        vec![
            ("start".into(), start.unwrap_or(Value::None)),
            ("end".into(), end.unwrap_or(Value::None)),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::temporal;

    #[test]
    fn a_range_holds_its_start_and_not_its_end() {
        let hours = temporal::range(10, 20);
        assert_eq!(contains(&hours, &Value::Instant(10)), Ok(true));
        assert_eq!(contains(&hours, &Value::Instant(19)), Ok(true));
        assert_eq!(contains(&hours, &Value::Instant(20)), Ok(false));
        assert_eq!(contains(&hours, &Value::Instant(9)), Ok(false));
        assert_eq!(contains(&hours, &Value::Int(12)), Err(Fault::Shape));
        let empty = temporal::range(15, 15);
        assert_eq!(contains(&empty, &Value::Instant(15)), Ok(false));
    }

    #[test]
    fn ranges_that_meet_share_nothing() {
        let (morning, noon, afternoon) = (
            temporal::range(9, 12),
            temporal::range(11, 13),
            temporal::range(12, 17),
        );
        assert_eq!(overlaps(&morning, &noon), Ok(true));
        assert_eq!(overlaps(&morning, &afternoon), Ok(false));
        assert_eq!(
            overlaps(&morning, &temporal::range(10, 10)),
            Ok(false),
            "an empty range overlaps nothing"
        );
        assert_eq!(intersection(&morning, &noon), Ok(temporal::range(11, 12)));
        assert_eq!(intersection(&noon, &morning), Ok(temporal::range(11, 12)));
        assert_eq!(intersection(&morning, &afternoon), Ok(Value::None));
        assert_eq!(
            intersection(&temporal::range(0, 100), &morning),
            Ok(morning.clone())
        );
        assert_eq!(overlaps(&morning, &Value::Int(1)), Err(Fault::Shape));
    }

    #[test]
    fn an_open_end_holds_everything_on_its_side() {
        let n = |n: i64| Some(Value::Int(n));
        let from_ten = range_of(n(10), None);
        let to_ten = range_of(None, n(10));
        let everything = range_of(None, None);
        assert_eq!(contains(&from_ten, &Value::Int(i64::MAX)), Ok(true));
        assert_eq!(contains(&from_ten, &Value::Int(9)), Ok(false));
        assert_eq!(contains(&to_ten, &Value::Int(i64::MIN)), Ok(true));
        assert_eq!(contains(&to_ten, &Value::Int(10)), Ok(false));
        assert_eq!(contains(&everything, &Value::Int(0)), Ok(true));
        assert_eq!(overlaps(&from_ten, &to_ten), Ok(false), "they meet at 10");
        assert_eq!(
            intersection(&from_ten, &range_of(n(5), n(20))),
            Ok(range_of(n(10), n(20)))
        );
        assert_eq!(intersection(&everything, &to_ten), Ok(to_ten.clone()));
        assert_eq!(
            intersection(&everything, &everything),
            Ok(everything.clone())
        );
        // Numbers of either representation compare as numbers.
        let price = |text: &str| Some(Value::decimal(text).expect("a decimal"));
        let band = range_of(price("1.50"), price("3.00"));
        assert_eq!(contains(&band, &Value::decimal("2.5").unwrap()), Ok(true));
        assert_eq!(contains(&band, &Value::decimal("3.00").unwrap()), Ok(false));
        // Two ranges of different types are a shape the checker refuses.
        assert_eq!(
            overlaps(&from_ten, &temporal::range(0, 1)),
            Err(Fault::Shape)
        );
    }
}
