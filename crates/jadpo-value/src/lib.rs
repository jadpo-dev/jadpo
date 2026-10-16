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
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing,
        clippy::unreachable,
        clippy::unimplemented
    )
)]

pub mod collection;
pub mod decimal;
pub mod integer;
pub mod json;
pub mod ops;
pub mod order;
mod rounding;
pub mod shape;
pub mod temporal;
pub mod validate;
mod value;

pub use jadpo_std::{Op, Rounding};
pub use ops::Fault;
pub use validate::{Invalid, Validator};
pub use value::{Decimal, QUOTIENT_PLACES, Record, Value, Variant, Zoned};

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
        (
            Op::TemporalAddDays
            | Op::TemporalAddMonths
            | Op::TemporalAddYears
            | Op::TemporalBetween
            | Op::TemporalDate
            | Op::TemporalDay
            | Op::TemporalDayBounds
            | Op::TemporalHour
            | Op::TemporalInZone
            | Op::TemporalInstant
            | Op::TemporalMinute
            | Op::TemporalMonth
            | Op::TemporalMonthBounds
            | Op::TemporalResolve
            | Op::TemporalWeekBounds
            | Op::TemporalWeekday
            | Op::TemporalYear
            | Op::TemporalYearBounds
            | Op::TemporalZone,
            _,
        ) => run_temporal(op, args),
        // The text these give in each locale comes with locales (area 10,
        // "Locales", roadmap chunk 5.3); until then the checker refuses
        // every call to them (`toolchain.not_built_yet`), so a program
        // that runs never reaches here.
        (Op::TemporalFormat | Op::TemporalFormatRelative, _) => Err(Fault::Shape),
    }
}

/// A `temporal` operation (area 10): [`temporal`] holds what each does.
fn run_temporal(op: Op, args: &[Value]) -> Result<Value, Fault> {
    use temporal::{Dated, Period};
    let bounds = |date: &Value, zone: &Value, period: Period| match date {
        Value::Date(day) => {
            let (start, end) = temporal::bounds(*day, temporal::zone_of(zone)?, period)?;
            Ok(temporal::range(start, end))
        }
        _ => Err(Fault::Shape),
    };
    let zoned = |value: &Value| match value {
        Value::Zoned(zoned) => Ok(*zoned),
        _ => Err(Fault::Shape),
    };
    let part = |value: &Value, at: usize| -> Result<Value, Fault> {
        let (year, month, day) = temporal::civil(Dated::of(value)?)?;
        Ok(Value::Int([year, month, day].get(at).copied().unwrap_or(0)))
    };
    match (op, args) {
        (Op::TemporalAddDays, [value, Value::Int(days), dst]) => {
            temporal::add_days(Dated::of(value)?, *days, temporal::dst_of(dst)?).map(Dated::value)
        }
        (Op::TemporalAddMonths, [value, Value::Int(months), invalid_day, dst]) => {
            temporal::add_months(
                Dated::of(value)?,
                *months,
                temporal::invalid_day_of(invalid_day)?,
                temporal::dst_of(dst)?,
            )
            .map(Dated::value)
        }
        (Op::TemporalAddYears, [value, Value::Int(years), invalid_day, dst]) => {
            temporal::add_years(
                Dated::of(value)?,
                *years,
                temporal::invalid_day_of(invalid_day)?,
                temporal::dst_of(dst)?,
            )
            .map(Dated::value)
        }
        (Op::TemporalBetween, [Value::Instant(start), Value::Instant(end)]) => {
            temporal::between(*start, *end).map(Value::Duration)
        }
        (Op::TemporalDate, [value]) => temporal::local_date(zoned(value)?).map(Value::Date),
        (Op::TemporalDay, [value]) => part(value, 2),
        (Op::TemporalMonth, [value]) => part(value, 1),
        (Op::TemporalYear, [value]) => part(value, 0),
        (Op::TemporalWeekday, [value]) => {
            temporal::weekday(Dated::of(value)?).map(temporal::weekday_value)
        }
        (Op::TemporalHour, [value]) => {
            temporal::clock(zoned(value)?).map(|clock| Value::Int(i64::from(clock / 3_600_000)))
        }
        (Op::TemporalMinute, [value]) => {
            temporal::clock(zoned(value)?).map(|clock| Value::Int(i64::from(clock / 60_000 % 60)))
        }
        (Op::TemporalInZone, [Value::Instant(at), zone]) => {
            temporal::in_zone(*at, temporal::zone_of(zone)?).map(Value::Zoned)
        }
        (Op::TemporalInstant, [value]) => zoned(value).map(|zoned| Value::Instant(zoned.millis)),
        (Op::TemporalZone, [value]) => zoned(value).map(|zoned| temporal::zone_value(zoned.zone)),
        (Op::TemporalResolve, [Value::Date(day), Value::Text(at), zone, dst]) => {
            let clock = jadpo_std::clock_millis(at).ok_or(Fault::Shape)?;
            temporal::resolve(
                *day,
                clock,
                temporal::zone_of(zone)?,
                temporal::dst_of(dst)?,
            )
            .map(Value::Zoned)
        }
        (Op::TemporalDayBounds, [date, zone]) => bounds(date, zone, Period::Day),
        (Op::TemporalWeekBounds, [date, zone, starts_on]) => {
            bounds(date, zone, Period::Week(temporal::weekday_of(starts_on)?))
        }
        (Op::TemporalMonthBounds, [date, zone]) => bounds(date, zone, Period::Month),
        (Op::TemporalYearBounds, [date, zone]) => bounds(date, zone, Period::Year),
        _ => Err(Fault::Shape),
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
        let london = temporal::zone_value(
            jadpo_std::variant_ordinal(jadpo_std::ZONE, "europe_london").unwrap_or(0),
        );
        let date = Value::Date(jadpo_std::date_number("2026-10-25").unwrap_or(0));
        let zoned = Value::Zoned(Zoned {
            millis: 0,
            zone: temporal::zone_of(&london).unwrap_or(0),
        });
        let items = Value::list(vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
        let mut double = |item: &Value| match item {
            Value::Int(n) => Some(Value::Int(n * 2)),
            _ => None,
        };
        for operation in jadpo_std::OPERATIONS {
            let args: Vec<Value> = match operation.op {
                // Refused while checking until locales come (chunk 5.3).
                Op::TemporalFormat | Op::TemporalFormatRelative => continue,
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
                Op::TemporalAddDays => vec![date.clone(), Value::Int(1), Value::None],
                Op::TemporalAddMonths | Op::TemporalAddYears => {
                    vec![zoned.clone(), Value::Int(1), Value::None, Value::None]
                }
                Op::TemporalBetween => vec![Value::Instant(0), Value::Instant(1000)],
                Op::TemporalInZone => vec![Value::Instant(0), london.clone()],
                Op::TemporalResolve => vec![
                    date.clone(),
                    Value::text("09:30"),
                    london.clone(),
                    Value::None,
                ],
                Op::TemporalDayBounds | Op::TemporalMonthBounds | Op::TemporalYearBounds => {
                    vec![date.clone(), london.clone()]
                }
                Op::TemporalWeekBounds => {
                    vec![date.clone(), london.clone(), temporal::weekday_value(0)]
                }
                Op::TemporalDay | Op::TemporalMonth | Op::TemporalYear | Op::TemporalWeekday => {
                    vec![date.clone()]
                }
                Op::TemporalDate
                | Op::TemporalHour
                | Op::TemporalMinute
                | Op::TemporalInstant
                | Op::TemporalZone => vec![zoned.clone()],
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
