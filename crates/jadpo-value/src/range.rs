//! The `range` namespace (area 19, "Ranges"): each operation once, for
//! both executors, by the range's own end rule. An `InstantRange` holds
//! every instant from its start up to, not including, its end (area 10),
//! so one whose start is its end holds nothing and overlaps nothing, as
//! PostgreSQL's `tstzrange` agrees. `Range<T>` and `DateRange` join in
//! chunks 5.5c2 and 5.5k.

use crate::temporal;
use crate::{Fault, Value};

/// An `InstantRange`'s start and end, in milliseconds.
fn ends(range: &Value) -> Result<(i64, i64), Fault> {
    let Value::Record(record) = range else {
        return Err(Fault::Shape);
    };
    let field = |name: &str| match record.fields.iter().find(|(field, _)| &**field == name) {
        Some((_, Value::Instant(at))) => Ok(*at),
        _ => Err(Fault::Shape),
    };
    Ok((field("start")?, field("end")?))
}

/// Whether `value` is inside `range`: at its start or after, and before
/// its end.
pub fn contains(range: &Value, value: &Value) -> Result<bool, Fault> {
    let (start, end) = ends(range)?;
    match value {
        Value::Instant(at) => Ok(start <= *at && *at < end),
        _ => Err(Fault::Shape),
    }
}

/// The shared part of two ranges, as its start and end; `None` when they
/// share no instant.
fn shared(a: &Value, b: &Value) -> Result<Option<(i64, i64)>, Fault> {
    let ((a_start, a_end), (b_start, b_end)) = (ends(a)?, ends(b)?);
    let (start, end) = (a_start.max(b_start), a_end.min(b_end));
    Ok((start < end).then_some((start, end)))
}

/// Whether two ranges share at least one instant.
pub fn overlaps(a: &Value, b: &Value) -> Result<bool, Fault> {
    Ok(shared(a, b)?.is_some())
}

/// The part two ranges share, or `none` when they don't overlap.
pub fn intersection(a: &Value, b: &Value) -> Result<Value, Fault> {
    Ok(shared(a, b)?.map_or(Value::None, |(start, end)| temporal::range(start, end)))
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
