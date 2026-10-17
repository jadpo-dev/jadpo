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
