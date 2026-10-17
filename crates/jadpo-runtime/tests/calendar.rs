//! Area 19's "Time and calendar additions" (chunk 5.5k), in the runtime:
//! the values the page's test (`project=calendar`) and the interpreter's
//! `calendar` tests assert, given to each operation's one implementation
//! as compiled code calls it (`jadpo_value::run` and the operators in
//! `jadpo_value::ops`), across the daylight-saving changes in London and
//! New York.

use std::rc::Rc;

use jadpo_std::Op;
use jadpo_value::{Value, calendar, ops, run};

fn call(op: Op, args: &[Value]) -> Value {
    run(op, args, None, &mut |_| None).unwrap_or_else(|fault| panic!("{op:?}: {fault:?}"))
}

fn day(text: &str) -> Value {
    Value::Date(jadpo_std::date_number(text).unwrap_or_else(|| panic!("{text} is a date")))
}

fn zone(name: &str) -> Value {
    let ordinal = jadpo_std::variant_ordinal(jadpo_std::ZONE, name).unwrap();
    Value::variant(jadpo_std::ZONE, ordinal, name, Vec::new())
}

/// The local day an instant falls on in a zone.
fn local_day(at: &str, zone_name: &str) -> Value {
    let at = Value::Instant(jadpo_std::instant_millis(at).unwrap());
    let seen = call(Op::TemporalInZone, &[at, zone(zone_name)]);
    call(Op::TemporalDate, &[seen])
}

#[test]
fn a_period_between_local_days_ignores_the_clock_changes() {
    for (zone_name, spring, autumn, (years, months, days)) in [
        (
            "europe_london",
            "2026-03-29T00:30:00Z",
            "2026-10-25T23:30:00Z",
            (0, 6, 26),
        ),
        (
            "america_new_york",
            "2026-03-08T07:30:00Z",
            "2026-11-02T03:30:00Z",
            (0, 7, 24),
        ),
    ] {
        let (start, end) = (local_day(spring, zone_name), local_day(autumn, zone_name));
        let between = call(Op::TemporalPeriodBetween, &[start.clone(), end.clone()]);
        assert_eq!(between, calendar::period(years, months, days), "{zone_name}");
        assert_eq!(ops::add(&start, &between), Ok(end.clone()));
        let backwards = call(Op::TemporalPeriodBetween, &[end.clone(), start.clone()]);
        assert_eq!(ops::add(&end, &backwards), Ok(start));
    }
    assert_eq!(local_day("2026-11-02T03:30:00Z", "america_new_york"), day("2026-11-01"));
    assert_eq!(local_day("2026-10-25T23:30:00Z", "europe_london"), day("2026-10-25"));
}

#[test]
fn the_operator_table_s_period_rows() {
    let month = calendar::period(0, 1, 0);
    assert_eq!(ops::add(&day("2026-01-31"), &month), Ok(day("2026-02-28")));
    assert_eq!(ops::add(&day("2028-01-31"), &month), Ok(day("2028-02-29")));
    assert_eq!(ops::subtract(&day("2026-03-31"), &month), Ok(day("2026-02-28")));
    let ten_days = calendar::period(0, 0, 10);
    assert_eq!(ops::add(&month, &ten_days), Ok(calendar::period(0, 1, 10)));
    assert_eq!(ops::subtract(&month, &ten_days), Ok(calendar::period(0, 1, -10)));
    assert_eq!(ops::multiply(&month, &Value::Int(12)), Ok(calendar::period(0, 12, 0)));
    // `==` part by part: a year isn't twelve months.
    assert!(!ops::equal(&calendar::period(1, 0, 0), &calendar::period(0, 12, 0)));
    assert_eq!(ops::compare(&month, &ten_days), None);
    assert_eq!(
        ops::multiply(&calendar::period(0, 0, i64::MAX), &Value::Int(2)),
        Err(jadpo_value::Fault::IntOverflow)
    );
    assert_eq!(
        ops::add(&day("9999-12-31"), &ten_days),
        Err(jadpo_value::Fault::TimeRange)
    );
}

#[test]
fn ordered_time_types_compare_part_by_part() {
    let earlier = calendar::local_time(9, 30, 0);
    let later = calendar::local_time(10, 0, 0);
    assert_eq!(ops::compare(&earlier, &later), Some(std::cmp::Ordering::Less));
    assert_eq!(ops::compare(&later, &later), Some(std::cmp::Ordering::Equal));
}

#[test]
fn a_date_range_holds_its_last_day() {
    let days = |start: &str, end: &str| {
        Value::record(
            jadpo_std::DATE_RANGE,
            vec![(Rc::from("start"), day(start)), (Rc::from("end"), day(end))],
        )
    };
    let october = days("2026-10-01", "2026-10-31");
    assert_eq!(
        call(Op::RangeContains, &[october.clone(), day("2026-10-31")]),
        Value::Bool(true)
    );
    assert_eq!(
        call(Op::RangeContains, &[october.clone(), day("2026-11-01")]),
        Value::Bool(false)
    );
    assert_eq!(
        call(Op::RangeOverlaps, &[october.clone(), days("2026-10-31", "2026-11-30")]),
        Value::Bool(true)
    );
    assert_eq!(
        call(Op::RangeIntersection, &[october, days("2026-11-01", "2026-11-30")]),
        Value::None
    );
}
