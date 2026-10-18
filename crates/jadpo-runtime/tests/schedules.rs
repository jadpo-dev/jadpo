//! Area 19, "Time and calendar additions" (chunk 5.5k2), in the runtime:
//! the values the page's test (`project=schedules`) and the interpreter's
//! `schedules` tests assert, given to `temporal.occurrences` and
//! `temporal.open_at` as compiled code calls them (`jadpo_value::run`),
//! across the daylight-saving changes in London and New York.

use jadpo_std::Op;
use jadpo_value::{Value, run};

fn call(op: Op, args: &[Value]) -> Value {
    run(op, args, None, &mut |_| None).unwrap_or_else(|fault| panic!("{op:?}: {fault:?}"))
}

fn instant(at: &str) -> Value {
    Value::Instant(jadpo_std::instant_millis(at).unwrap_or_else(|| panic!("{at} is an instant")))
}

/// The instants of the next `count` occurrences of `rule` from `from`,
/// each checked to be in `zone`.
fn occurrences(rule: &str, from: &str, count: i64, zone: &str) -> Vec<String> {
    let found = call(
        Op::TemporalOccurrences,
        &[Value::text(rule), instant(from), Value::Int(count)],
    );
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

fn open(hours: &str, at: &str) -> bool {
    call(Op::TemporalOpenAt, &[Value::text(hours), instant(at)]) == Value::Bool(true)
}

#[test]
fn occurrences_keep_their_local_time_across_the_changes() {
    assert_eq!(
        occurrences(
            "DTSTART;TZID=Europe/London:20261019T093000\nRRULE:FREQ=WEEKLY;BYDAY=MO",
            "2026-10-19T00:00:00Z",
            3,
            "europe_london"
        ),
        [
            "2026-10-19T08:30:00Z",
            "2026-10-26T09:30:00Z",
            "2026-11-02T09:30:00Z"
        ]
    );
    assert_eq!(
        occurrences(
            "DTSTART;TZID=America/New_York:20260210T090000\nRRULE:FREQ=MONTHLY;BYDAY=2TU",
            "2026-01-01T00:00:00Z",
            3,
            "america_new_york"
        ),
        [
            "2026-02-10T14:00:00Z",
            "2026-03-10T13:00:00Z",
            "2026-04-14T13:00:00Z"
        ]
    );
    // A later `from` starts the expansion near it, with the answer a
    // rule counted from its start (`COUNT`) gives.
    let later = occurrences(
        "DTSTART;TZID=Europe/London:20200106T093000\nRRULE:FREQ=WEEKLY;INTERVAL=2;BYDAY=MO",
        "2026-10-20T00:00:00Z",
        2,
        "europe_london",
    );
    assert_eq!(later, ["2026-11-02T09:30:00Z", "2026-11-16T09:30:00Z"]);
    assert_eq!(
        occurrences(
            "DTSTART;TZID=Europe/London:20200106T093000\nRRULE:FREQ=WEEKLY;INTERVAL=2;BYDAY=MO;COUNT=500",
            "2026-10-20T00:00:00Z",
            2,
            "europe_london"
        ),
        later
    );
}

#[test]
fn a_gap_is_skipped_uncounted_and_an_overlap_is_its_first() {
    assert_eq!(
        occurrences(
            "DTSTART;TZID=Europe/London:20260327T013000\nRRULE:FREQ=DAILY;COUNT=4",
            "2026-01-01T00:00:00Z",
            20,
            "europe_london"
        ),
        [
            "2026-03-27T01:30:00Z",
            "2026-03-28T01:30:00Z",
            "2026-03-30T00:30:00Z",
            "2026-03-31T00:30:00Z"
        ]
    );
    assert_eq!(
        occurrences(
            "DTSTART;TZID=America/New_York:20260307T023000\nRRULE:FREQ=DAILY",
            "2026-03-07T00:00:00Z",
            3,
            "america_new_york"
        ),
        [
            "2026-03-07T07:30:00Z",
            "2026-03-09T06:30:00Z",
            "2026-03-10T06:30:00Z"
        ]
    );
    assert_eq!(
        occurrences(
            "DTSTART;TZID=Europe/London:20261024T013000\nRRULE:FREQ=DAILY",
            "2026-10-24T00:00:00Z",
            3,
            "europe_london"
        ),
        [
            "2026-10-24T00:30:00Z",
            "2026-10-25T00:30:00Z",
            "2026-10-26T01:30:00Z"
        ]
    );
    assert_eq!(
        occurrences(
            "DTSTART;TZID=America/New_York:20261031T013000\nRRULE:FREQ=DAILY;UNTIL=20261102T063000Z",
            "2026-10-01T00:00:00Z",
            20,
            "america_new_york"
        ),
        [
            "2026-10-31T05:30:00Z",
            "2026-11-01T05:30:00Z",
            "2026-11-02T06:30:00Z"
        ]
    );
    // A start the clocks skip is its first occurrence all the same, an
    // hour on (RFC 5545, 3.3.5).
    assert_eq!(
        occurrences(
            "DTSTART;TZID=America/New_York:20260308T023000\nRRULE:FREQ=DAILY;COUNT=2",
            "2026-01-01T00:00:00Z",
            5,
            "america_new_york"
        ),
        ["2026-03-08T07:30:00Z", "2026-03-09T06:30:00Z"]
    );
}

#[test]
fn opening_hours_are_read_by_the_clock_on_the_wall() {
    let london = "Mo-Th 17:00-23:00; Fr,Sa 17:00-02:00; 2026 Dec 25 off [Europe/London]";
    assert!(open(london, "2026-10-25T00:30:00Z"));
    assert!(open(london, "2026-10-25T01:30:00Z"));
    assert!(!open(london, "2026-10-25T02:00:00Z"));
    assert!(open(london, "2026-03-29T00:59:00Z"));
    assert!(!open(london, "2026-03-29T01:00:00Z"));
    assert!(!open(london, "2026-12-25T18:00:00Z"));
    let new_york = "Mo-Th 17:00-23:00; Fr,Sa 17:00-02:00 [America/New_York]";
    assert!(open(new_york, "2026-03-08T06:30:00Z"));
    assert!(!open(new_york, "2026-03-08T07:00:00Z"));
    assert!(open(new_york, "2026-11-01T05:30:00Z"));
    assert!(open(new_york, "2026-11-01T06:30:00Z"));
    assert!(!open(new_york, "2026-11-01T07:00:00Z"));
}

/// A `LocalDateTime` as compiled code holds one: a record of its date and
/// its `LocalTime`.
fn local(date: &str, hour: i64, minute: i64) -> Value {
    Value::record(
        "LocalDateTime",
        vec![
            (
                "date".into(),
                Value::Date(jadpo_std::date_number(date).expect("a date")),
            ),
            (
                "time".into(),
                Value::record(
                    "LocalTime",
                    vec![
                        ("hour".into(), Value::Int(hour)),
                        ("minute".into(), Value::Int(minute)),
                        ("second".into(), Value::Int(0)),
                    ],
                ),
            ),
        ],
    )
}

fn variant(ty: &str, name: &str) -> Value {
    let ordinal = jadpo_std::variant_ordinal(ty, name).expect("a variant");
    Value::variant(ty, ordinal, name, Vec::new())
}

/// The instant `temporal.resolve` gives for a `LocalDateTime` in `zone`
/// under `dst` (`None` left out), or `None` when it fails.
fn resolved(at: &Value, zone: &str, dst: Option<&str>) -> Option<String> {
    let dst = dst.map_or(Value::None, |name| variant(jadpo_std::Dst::NAME, name));
    let args = [at.clone(), Value::None, variant(jadpo_std::ZONE, zone), dst];
    match run(Op::TemporalResolve, &args, None, &mut |_| None) {
        Ok(Value::Zoned(zoned)) => Some(jadpo_std::format_instant(zoned.millis)),
        Ok(other) => panic!("a zoned time: {other:?}"),
        Err(_) => None,
    }
}

#[test]
fn a_local_time_resolves_across_both_clock_changes() {
    // (local time, zone, earlier, later): an overlap's two instants, or the
    // clock time moved back and forward by a gap's length. `compatible` is
    // earlier in an overlap and later in a gap; left out, both fail.
    let cases = [
        // London's clocks go forward at 01:00 GMT on 29 March 2026.
        (
            local("2026-03-29", 1, 30),
            "europe_london",
            "2026-03-29T00:30:00Z",
            "2026-03-29T01:30:00Z",
            false,
        ),
        // And back at 02:00 BST on 25 October.
        (
            local("2026-10-25", 1, 30),
            "europe_london",
            "2026-10-25T00:30:00Z",
            "2026-10-25T01:30:00Z",
            true,
        ),
        // New York's go forward at 02:00 EST on 8 March 2026.
        (
            local("2026-03-08", 2, 30),
            "america_new_york",
            "2026-03-08T06:30:00Z",
            "2026-03-08T07:30:00Z",
            false,
        ),
        // And back at 02:00 EDT on 1 November.
        (
            local("2026-11-01", 1, 30),
            "america_new_york",
            "2026-11-01T05:30:00Z",
            "2026-11-01T06:30:00Z",
            true,
        ),
    ];
    for (at, zone, earlier, later, overlap) in cases {
        assert_eq!(
            resolved(&at, zone, Some("earlier")).as_deref(),
            Some(earlier)
        );
        assert_eq!(resolved(&at, zone, Some("later")).as_deref(), Some(later));
        let compatible = if overlap { earlier } else { later };
        assert_eq!(
            resolved(&at, zone, Some("compatible")).as_deref(),
            Some(compatible)
        );
        assert_eq!(resolved(&at, zone, Some("reject")), None);
        assert_eq!(resolved(&at, zone, None), None);
    }
    // An ordinary time is one instant whatever `dst:` says.
    assert_eq!(
        resolved(&local("2026-10-24", 22, 0), "europe_london", None).as_deref(),
        Some("2026-10-24T21:00:00Z")
    );
    // A date with a `LocalTime` for `at:` is the same instant.
    let Value::Record(record) = local("2026-10-25", 1, 30) else {
        unreachable!()
    };
    let args = [
        record.get("date").cloned().expect("a date"),
        record.get("time").cloned().expect("a time"),
        variant(jadpo_std::ZONE, "europe_london"),
        variant(jadpo_std::Dst::NAME, "later"),
    ];
    let Value::Zoned(zoned) = call(Op::TemporalResolve, &args) else {
        panic!("a zoned time");
    };
    assert_eq!(
        jadpo_std::format_instant(zoned.millis),
        "2026-10-25T01:30:00Z"
    );
}

/// The days of the next `count` occurrences of `rule` from the day `from`.
fn days(rule: &str, from: &str, count: i64) -> Vec<String> {
    let from = Value::Date(jadpo_std::date_number(from).expect("a date"));
    let found = call(
        Op::TemporalOccurrences,
        &[Value::text(rule), from, Value::Int(count)],
    );
    let Value::List(items) = found else {
        panic!("a list: {found:?}");
    };
    items
        .iter()
        .map(|item| match item {
            Value::Date(day) => jadpo_std::format_date(*day),
            other => panic!("a date: {other:?}"),
        })
        .collect()
}

#[test]
fn a_rule_on_dates_gives_days() {
    // Yearly from 29 February 2024: 2025 to 2027 have no 29 February, so
    // they are skipped and not counted (RFC 5545 3.3.10).
    let leap_day = "DTSTART;VALUE=DATE:20240229\nRRULE:FREQ=YEARLY";
    assert_eq!(
        days(leap_day, "2024-01-01", 3),
        ["2024-02-29", "2028-02-29", "2032-02-29"]
    );
    assert_eq!(days(leap_day, "2024-03-01", 1), ["2028-02-29"]);
    assert_eq!(
        days(
            "DTSTART;VALUE=DATE:20240229\nRRULE:FREQ=YEARLY;COUNT=2",
            "2020-01-01",
            10
        ),
        ["2024-02-29", "2028-02-29"]
    );
    assert_eq!(
        days(
            "DTSTART;VALUE=DATE:20261225\nRRULE:FREQ=YEARLY;UNTIL=20281225",
            "2026-01-01",
            10
        ),
        ["2026-12-25", "2027-12-25", "2028-12-25"]
    );
    // A timed rule's days are its occurrences' local dates in its zone:
    // 22:00 in New York is the next day in UTC.
    assert_eq!(
        days(
            "DTSTART;TZID=America/New_York:20261030T220000\nRRULE:FREQ=DAILY",
            "2026-10-31",
            2
        ),
        ["2026-10-31", "2026-11-01"]
    );
    // A rule on dates has no instants to give.
    let args = [Value::text(leap_day), Value::Instant(0), Value::Int(1)];
    assert_eq!(
        run(Op::TemporalOccurrences, &args, None, &mut |_| None),
        Err(jadpo_value::Fault::DateOnlyRule)
    );
}
