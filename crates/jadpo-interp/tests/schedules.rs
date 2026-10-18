//! Area 19, "Time and calendar additions" (chunk 5.5k2), run: a
//! `RecurrenceRule`'s occurrences and whether `OpeningHours` are open,
//! across the daylight-saving changes in London and New York. A
//! recurrence keeps its local time, skips one the clocks don't show
//! without counting it, and takes the first of one they show twice (RFC
//! 5545); opening hours are read by the clock on the wall.

use crate::support;

use jadpo_interp::Value;
use support::{failure, int, main, text, value};

const SOURCE: &str = "type ClassSchedule = RecurrenceRule {}

type ClassTime = ZonedTime {}

type SearchFrom = Instant {}

type ClassCount = Int {
    min: 1
    max: 20
}

type BarHours = OpeningHours {}

type CheckedAt = Instant {}

type IsOpen = Bool {}

function next_classes(timetable: ClassSchedule, from: SearchFrom, count: ClassCount) -> List<ClassTime> {
    return list.map(temporal.occurrences(timetable, from: from, count: count), at -> ClassTime(at))
}

function first_three(from: SearchFrom) -> List<ClassTime> {
    return list.map(
        temporal.occurrences(
            \"DTSTART;TZID=Europe/London:20261019T093000\\nRRULE:FREQ=WEEKLY;BYDAY=MO\",
            from: from,
            count: 3
        ),
        at -> ClassTime(at)
    )
}

function open(hours: BarHours, at: CheckedAt) -> IsOpen {
    return IsOpen(temporal.open_at(hours, at))
}
";

fn instant(at: &str) -> Value {
    Value::Instant(jadpo_std::instant_millis(at).unwrap_or_else(|| panic!("{at} is an instant")))
}

/// The instants of a list of zoned times, each in `zone`.
fn instants(found: &Value, zone: &str) -> Vec<String> {
    let Value::List(items) = found else {
        panic!("a list: {found:?}");
    };
    let ordinal = jadpo_std::variant_ordinal(jadpo_std::ZONE, zone).unwrap();
    items
        .iter()
        .map(|item| match item {
            Value::Zoned(zoned) => {
                assert_eq!(zoned.zone, ordinal, "in the rule's zone");
                jadpo_std::format_instant(zoned.millis)
            }
            other => panic!("a zoned time: {other:?}"),
        })
        .collect()
}

fn next(program: &jadpo_interp::Program, rule: &str, from: &str, count: i64) -> Value {
    value(
        program,
        "next_classes",
        vec![text(rule), instant(from), int(count)],
    )
}

#[test]
fn a_weekly_class_keeps_its_local_time_across_the_change() {
    let program = main(SOURCE).with_proof_checks();
    // London's clocks go back on 25 October 2026: 09:30 is 08:30 UTC
    // before and 09:30 UTC after.
    assert_eq!(
        instants(
            &value(
                &program,
                "first_three",
                vec![instant("2026-10-19T00:00:00Z")]
            ),
            "europe_london"
        ),
        [
            "2026-10-19T08:30:00Z",
            "2026-10-26T09:30:00Z",
            "2026-11-02T09:30:00Z"
        ]
    );
    // At or after `from`: the first is skipped once it has started.
    assert_eq!(
        instants(
            &value(
                &program,
                "first_three",
                vec![instant("2026-10-19T08:30:00.001Z")]
            ),
            "europe_london"
        )[0],
        "2026-10-26T09:30:00Z"
    );
    // New York's go forward on 8 March 2026: the second Tuesday at 09:00
    // is 14:00 UTC in February and 13:00 UTC in March.
    assert_eq!(
        instants(
            &next(
                &program,
                "DTSTART;TZID=America/New_York:20260210T090000\nRRULE:FREQ=MONTHLY;BYDAY=2TU",
                "2026-01-01T00:00:00Z",
                3
            ),
            "america_new_york"
        ),
        [
            "2026-02-10T14:00:00Z",
            "2026-03-10T13:00:00Z",
            "2026-04-14T13:00:00Z"
        ]
    );
}

#[test]
fn a_local_time_the_clocks_skip_is_left_out_and_not_counted() {
    let program = main(SOURCE).with_proof_checks();
    // London skips 01:00 to 02:00 on 29 March 2026: four occurrences,
    // none that day.
    assert_eq!(
        instants(
            &next(
                &program,
                "DTSTART;TZID=Europe/London:20260327T013000\nRRULE:FREQ=DAILY;COUNT=4",
                "2026-01-01T00:00:00Z",
                20
            ),
            "europe_london"
        ),
        [
            "2026-03-27T01:30:00Z",
            "2026-03-28T01:30:00Z",
            "2026-03-30T00:30:00Z",
            "2026-03-31T00:30:00Z"
        ]
    );
    // New York skips 02:00 to 03:00 on 8 March 2026.
    assert_eq!(
        instants(
            &next(
                &program,
                "DTSTART;TZID=America/New_York:20260307T023000\nRRULE:FREQ=DAILY",
                "2026-03-07T00:00:00Z",
                3
            ),
            "america_new_york"
        ),
        [
            "2026-03-07T07:30:00Z",
            "2026-03-09T06:30:00Z",
            "2026-03-10T06:30:00Z"
        ]
    );
}

#[test]
fn a_local_time_the_clocks_show_twice_is_the_first() {
    let program = main(SOURCE).with_proof_checks();
    // London shows 01:00 to 02:00 twice on 25 October 2026, first in
    // summer time.
    assert_eq!(
        instants(
            &next(
                &program,
                "DTSTART;TZID=Europe/London:20261024T013000\nRRULE:FREQ=DAILY",
                "2026-10-24T00:00:00Z",
                3
            ),
            "europe_london"
        ),
        [
            "2026-10-24T00:30:00Z",
            "2026-10-25T00:30:00Z",
            "2026-10-26T01:30:00Z"
        ]
    );
    // New York shows 01:00 to 02:00 twice on 1 November 2026.
    assert_eq!(
        instants(
            &next(
                &program,
                "DTSTART;TZID=America/New_York:20261031T013000\nRRULE:FREQ=DAILY;UNTIL=20261102T063000Z",
                "2026-10-01T00:00:00Z",
                20
            ),
            "america_new_york"
        ),
        [
            "2026-10-31T05:30:00Z",
            "2026-11-01T05:30:00Z",
            "2026-11-02T06:30:00Z"
        ]
    );
}

#[test]
fn a_night_past_midnight_is_open_by_the_clock_on_the_wall() {
    let program = main(SOURCE).with_proof_checks();
    let open = |hours: &str, at: &str| {
        value(&program, "open", vec![text(hours), instant(at)]) == Value::Bool(true)
    };
    let london = "Mo-Th 17:00-23:00; Fr,Sa 17:00-02:00; 2026 Dec 25 off [Europe/London]";
    // Saturday 24 October's night runs to 02:00 on the wall, and 01:00 to
    // 02:00 happens twice, so it is open an hour longer.
    assert!(open(london, "2026-10-24T22:00:00Z"));
    assert!(open(london, "2026-10-25T00:30:00Z"));
    assert!(open(london, "2026-10-25T01:30:00Z"));
    assert!(!open(london, "2026-10-25T02:00:00Z"));
    // Saturday 28 March's: the clocks go from 01:00 to 02:00, closing it.
    assert!(open(london, "2026-03-29T00:59:00Z"));
    assert!(!open(london, "2026-03-29T01:00:00Z"));
    // A Monday afternoon, and Christmas Day, a Friday.
    assert!(!open(london, "2026-10-26T16:59:00Z"));
    assert!(open(london, "2026-10-26T17:00:00Z"));
    assert!(!open(london, "2026-12-25T18:00:00Z"));
    let new_york = "Mo-Th 17:00-23:00; Fr,Sa 17:00-02:00 [America/New_York]";
    // Saturday 7 March's night: 02:00 EST is 03:00 EDT.
    assert!(open(new_york, "2026-03-08T06:30:00Z"));
    assert!(!open(new_york, "2026-03-08T07:00:00Z"));
    // Saturday 31 October's: 01:30 is open both times.
    assert!(open(new_york, "2026-11-01T05:30:00Z"));
    assert!(open(new_york, "2026-11-01T06:30:00Z"));
    assert!(!open(new_york, "2026-11-01T07:00:00Z"));
    assert_eq!(
        value(
            &program,
            "open",
            vec![text(new_york), instant("2026-11-02T22:00:00Z")]
        ),
        Value::Bool(true)
    );
}

/// A timed event's start resolved in its zone, and an all-day event's
/// days (chunk 5.5k4).
const LOCAL: &str = "type ShiftStart = LocalDateTime {}

type Doors = LocalTime {}

type ShopDay = CalendarDate {}

type Opening = ZonedTime {}

type Birthdays = RecurrenceRule {}

type DayCount = Int {
    min: 1
    max: 20
}

type Unclear = Failure {
    kind: FailureKind.invalid_value
}

function start_earlier(start: ShiftStart, zone: Zone) -> Opening {
    return Opening(temporal.resolve(start, zone: zone, dst: Dst.earlier))
}

function start_later(start: ShiftStart, zone: Zone) -> Opening {
    return Opening(temporal.resolve(start, zone: zone, dst: Dst.later))
}

function start_compatible(start: ShiftStart, zone: Zone) -> Opening {
    return Opening(temporal.resolve(start, zone: zone, dst: Dst.compatible))
}

function start_strict(start: ShiftStart, zone: Zone) fails Unclear -> Opening {
    var at: ZonedTime = attempt temporal.resolve(start, zone: zone) {
        dst: Unclear
    }
    return Opening(at)
}

function doors_open(day: ShopDay, doors: Doors, zone: Zone) -> Opening {
    return Opening(temporal.resolve(day, at: doors, zone: zone, dst: Dst.later))
}

function next_birthdays(rule: Birthdays, from: ShopDay, count: DayCount) -> List<ShopDay> {
    return list.map(temporal.occurrences(rule, from: from, count: count), day -> ShopDay(day))
}
";

fn date(text: &str) -> Value {
    Value::Date(jadpo_std::date_number(text).unwrap_or_else(|| panic!("{text} is a date")))
}

fn clock(hour: i64, minute: i64) -> Value {
    Value::record(
        "LocalTime",
        vec![
            ("hour".into(), int(hour)),
            ("minute".into(), int(minute)),
            ("second".into(), int(0)),
        ],
    )
}

fn local(day: &str, hour: i64, minute: i64) -> Value {
    Value::record(
        "LocalDateTime",
        vec![
            ("date".into(), date(day)),
            ("time".into(), clock(hour, minute)),
        ],
    )
}

fn zone(name: &str) -> Value {
    let ordinal = jadpo_std::variant_ordinal(jadpo_std::ZONE, name).unwrap();
    Value::variant(jadpo_std::ZONE, ordinal, name, Vec::new())
}

fn instant_of(found: &Value) -> String {
    match found {
        Value::Zoned(zoned) => jadpo_std::format_instant(zoned.millis),
        other => panic!("a zoned time: {other:?}"),
    }
}

#[test]
fn an_event_s_start_resolves_across_both_clock_changes() {
    let program = main(LOCAL).with_proof_checks();
    // (local time, zone, earlier, later, an overlap rather than a gap)
    let cases = [
        (
            "2026-03-29",
            1,
            "europe_london",
            "2026-03-29T00:30:00Z",
            "2026-03-29T01:30:00Z",
            false,
        ),
        (
            "2026-10-25",
            1,
            "europe_london",
            "2026-10-25T00:30:00Z",
            "2026-10-25T01:30:00Z",
            true,
        ),
        (
            "2026-03-08",
            2,
            "america_new_york",
            "2026-03-08T06:30:00Z",
            "2026-03-08T07:30:00Z",
            false,
        ),
        (
            "2026-11-01",
            1,
            "america_new_york",
            "2026-11-01T05:30:00Z",
            "2026-11-01T06:30:00Z",
            true,
        ),
    ];
    for (day, hour, name, earlier, later, overlap) in cases {
        let args = || vec![local(day, hour, 30), zone(name)];
        assert_eq!(
            instant_of(&value(&program, "start_earlier", args())),
            earlier
        );
        assert_eq!(instant_of(&value(&program, "start_later", args())), later);
        let compatible = if overlap { earlier } else { later };
        assert_eq!(
            instant_of(&value(&program, "start_compatible", args())),
            compatible
        );
        assert_eq!(failure(&program, "start_strict", args()).name, "Unclear");
    }
    assert_eq!(
        instant_of(&value(
            &program,
            "start_strict",
            vec![local("2026-10-24", 22, 0), zone("europe_london")]
        )),
        "2026-10-24T21:00:00Z"
    );
    assert_eq!(
        instant_of(&value(
            &program,
            "doors_open",
            vec![date("2026-03-29"), clock(1, 30), zone("europe_london")]
        )),
        "2026-03-29T01:30:00Z"
    );
}

#[test]
fn a_birthday_on_29_february_skips_the_years_without_one() {
    let program = main(LOCAL).with_proof_checks();
    let days = |rule: &str, from: &str, count: i64| -> Vec<String> {
        let Value::List(items) = value(
            &program,
            "next_birthdays",
            vec![text(rule), date(from), int(count)],
        ) else {
            panic!("a list");
        };
        items
            .iter()
            .map(|item| match item {
                Value::Date(day) => jadpo_std::format_date(*day),
                other => panic!("a date: {other:?}"),
            })
            .collect()
    };
    assert_eq!(
        days(
            "DTSTART;VALUE=DATE:20240229\nRRULE:FREQ=YEARLY",
            "2024-01-01",
            3
        ),
        ["2024-02-29", "2028-02-29", "2032-02-29"]
    );
    assert_eq!(
        days(
            "DTSTART;VALUE=DATE:20240229\nRRULE:FREQ=YEARLY;COUNT=2",
            "2025-01-01",
            5
        ),
        ["2028-02-29"]
    );
}
