//! Area 19, "Time and calendar additions" (chunk 5.5k), run: a date moved
//! by a `CalendarPeriod` and `temporal.period_between` across the
//! daylight-saving changes in London and New York, the period's other
//! operator rows, a `LocalTime` built from parts and ordered, and a
//! `DateRange` holding its end date.

use std::rc::Rc;

use crate::support;

use jadpo_interp::Value;
use jadpo_value::calendar;
use support::{failure, int, main, text, value};

const SOURCE: &str = "type Day = CalendarDate {}

type Moment = Instant {}

type Hour = Int {}

type Flag = Bool {}

type Wire = Text {}

type NoSuchHour = Failure {
    kind: FailureKind.invalid_value
}

function local_day(at: Moment, zone: Zone) -> Day {
    return Day(temporal.date(temporal.in_zone(at, zone)))
}

function lived(moved_in: Day, today: Day) -> CalendarPeriod {
    return temporal.period_between(moved_in, today)
}

function moved(day: Day, term: CalendarPeriod) -> Day {
    return day + term
}

function back(day: Day, term: CalendarPeriod) -> Day {
    return day - term
}

function combined(a: CalendarPeriod, b: CalendarPeriod) -> List<CalendarPeriod> {
    return [a + b, a - b, a * 3]
}

function written(term: CalendarPeriod) -> Wire {
    return Wire(text.from(term))
}

function opening(h: Hour) fails NoSuchHour -> LocalTime {
    var hour: LocalTime.hour = attempt LocalTime.hour(h) {
        invalid: NoSuchHour
    }
    return LocalTime {
        hour: hour
        minute: 30
        second: 0
    }
}

function earlier(a: LocalTime, b: LocalTime) -> Flag {
    return Flag(a < b)
}

function inside(days: DateRange, day: Day) -> Flag {
    return Flag(range.contains(days, day))
}

function shared(a: DateRange, b: DateRange) -> DateRange? {
    return range.intersection(a, b)
}
";

fn day(text: &str) -> Value {
    Value::Date(jadpo_std::date_number(text).unwrap_or_else(|| panic!("{text} is a date")))
}

fn instant(at: &str) -> Value {
    Value::Instant(jadpo_std::instant_millis(at).unwrap_or_else(|| panic!("{at} is an instant")))
}

fn zone(name: &str) -> Value {
    let ordinal = jadpo_std::variant_ordinal(jadpo_std::ZONE, name).unwrap();
    Value::variant(jadpo_std::ZONE, ordinal, name, Vec::new())
}

fn days(start: &str, end: &str) -> Value {
    Value::record(
        jadpo_std::DATE_RANGE,
        vec![(Rc::from("start"), day(start)), (Rc::from("end"), day(end))],
    )
}

#[test]
fn a_period_between_local_days_ignores_the_clock_changes() {
    let program = main(SOURCE).with_proof_checks();
    // Each change's day, just after its local midnight, and the day after
    // the next change: London's clocks go forward on 29 March 2026 and
    // back on 25 October, New York's on 8 March and 1 November.
    for (zone_name, spring, autumn, period) in [
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
        let start = value(&program, "local_day", vec![instant(spring), zone(zone_name)]);
        let end = value(&program, "local_day", vec![instant(autumn), zone(zone_name)]);
        let (years, months, days) = period;
        let between = value(&program, "lived", vec![start.clone(), end.clone()]);
        assert_eq!(between, calendar::period(years, months, days), "{zone_name}");
        // Adding it reaches the end; the other way round, every part is
        // negative, counted from the end, and adding it reaches the start.
        assert_eq!(value(&program, "moved", vec![start.clone(), between]), end);
        let backwards = value(&program, "lived", vec![end.clone(), start.clone()]);
        let (y, m, d) = calendar::period_parts(&backwards).unwrap();
        assert!(y <= 0 && m < 0 && d < 0, "{zone_name}: {backwards:?}");
        assert_eq!(value(&program, "moved", vec![end, backwards]), start);
    }
}

#[test]
fn a_period_moves_a_date_and_combines_part_by_part() {
    let program = main(SOURCE).with_proof_checks();
    let month = calendar::period(0, 1, 0);
    assert_eq!(
        value(&program, "moved", vec![day("2026-01-31"), month.clone()]),
        day("2026-02-28")
    );
    assert_eq!(
        value(&program, "back", vec![day("2026-03-31"), month.clone()]),
        day("2026-02-28")
    );
    assert_eq!(
        value(
            &program,
            "combined",
            vec![month.clone(), calendar::period(0, 0, 10)]
        ),
        Value::list(vec![
            calendar::period(0, 1, 10),
            calendar::period(0, 1, -10),
            calendar::period(0, 3, 0),
        ])
    );
    assert_eq!(
        value(&program, "written", vec![calendar::period(1, -2, 3)]),
        text("P1Y-2M3D")
    );
}

#[test]
fn a_local_time_is_built_from_parts_that_fit() {
    let program = main(SOURCE).with_proof_checks();
    let nine = value(&program, "opening", vec![int(9)]);
    assert_eq!(nine, calendar::local_time(9, 30, 0));
    assert_eq!(failure(&program, "opening", vec![int(24)]).name, "NoSuchHour");
    assert_eq!(
        value(
            &program,
            "earlier",
            vec![nine.clone(), calendar::local_time(10, 0, 0)]
        ),
        Value::Bool(true)
    );
    assert_eq!(
        value(&program, "earlier", vec![nine.clone(), nine]),
        Value::Bool(false)
    );
}

#[test]
fn a_date_range_holds_its_last_day() {
    let program = main(SOURCE).with_proof_checks();
    let october = days("2026-10-01", "2026-10-31");
    assert_eq!(
        value(&program, "inside", vec![october.clone(), day("2026-10-31")]),
        Value::Bool(true)
    );
    assert_eq!(
        value(&program, "inside", vec![october.clone(), day("2026-11-01")]),
        Value::Bool(false)
    );
    // Two ranges that meet on a day share it.
    assert_eq!(
        value(
            &program,
            "shared",
            vec![october.clone(), days("2026-10-31", "2026-11-30")]
        ),
        days("2026-10-31", "2026-10-31")
    );
    assert_eq!(
        value(&program, "shared", vec![october, days("2026-11-01", "2026-11-30")]),
        Value::None
    );
}
