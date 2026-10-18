//! Area 2, "Rules across fields" (chunk 5.5t), run: a booking whose end is
//! before its start fails construction with the rule's name, mapped by its
//! own arm or by `invalid:`; a proved construction runs no check and holds
//! under proof checks; a rule over a field that may be `none` and a rule
//! calling a standard operation run as written; and JSON that breaks a
//! rule is a problem at the object's path, named by the rule.

use std::rc::Rc;

use crate::support;

use jadpo_interp::Value;
use support::{failure, int, main, text, value};

const SOURCE: &str = "type Moment = Instant {}

type Day = CalendarDate {}

type Guests = Int {
    min: 1
    max: 12
}

type Wire = Text {}

type Backwards = Failure {
    kind: FailureKind.invalid_value
}

type Refused = Failure {
    kind: FailureKind.invalid_value
}

type Unreadable = Failure {
    kind: FailureKind.invalid_value
}

type Invalid = Failure {
    kind: FailureKind.invalid_value
    client: {
        problem: DecodeProblem
    }
}

type Booking = Object {
    starts: Moment
    ends: Moment
    rule ends_after_start: ends > starts
}

type Search = Input {
    from: Day
    until: Day {
        optional: true
    }
    adults: Guests
    rule ordered: match until {
        some(day) -> day >= from
        none -> true
    }
    rule small: integer.min(adults, 12) <= 8
}

type Request = Input {
    booking: Booking
    note: Wire
}

function book(starts: Moment, ends: Moment) fails Backwards -> Booking {
    return attempt Booking {
        starts: starts
        ends: ends
    } {
        ends_after_start: Backwards
    }
}

function book_any(starts: Moment, ends: Moment) fails Refused -> Booking {
    return attempt Booking {
        starts: starts
        ends: ends
    } {
        invalid: Refused
    }
}

function book_checked(starts: Moment, ends: Moment) fails Backwards -> Booking {
    if (ends <= starts) {
        reject Backwards
    }
    return Booking {
        starts: starts
        ends: ends
    }
}

function search(from: Day, adults: Guests) fails Backwards -> Search {
    return attempt Search {
        from: from
        until: \"2026-03-01\"
        adults: adults
    } {
        invalid: Backwards
    }
}

function read_request(feed: Wire) fails Unreadable, Invalid -> Request {
    return attempt json.read(feed, as: Request) {
        malformed: Unreadable
        mismatch: Invalid
    }
}
";

fn instant(text: &str) -> Value {
    Value::Instant(jadpo_std::instant_millis(text).expect("an instant"))
}

fn day(text: &str) -> Value {
    Value::Date(jadpo_std::date_number(text).expect("a date"))
}

const NINE: &str = "2026-03-02T09:00:00Z";
const TEN: &str = "2026-03-02T10:00:00Z";

#[test]
fn a_booking_that_ends_before_it_starts_fails_with_the_rule_s_name() {
    let program = main(SOURCE);
    let found = failure(&program, "book", vec![instant(TEN), instant(NINE)]);
    assert_eq!(found.name, "Backwards");
    assert_eq!(found.broken.as_deref(), Some("`ends_after_start`"));
    // One that ends at the moment it starts breaks it too.
    assert_eq!(
        failure(&program, "book", vec![instant(TEN), instant(TEN)]).name,
        "Backwards"
    );
    // `invalid:` covers the rule as it covers any.
    let found = failure(&program, "book_any", vec![instant(TEN), instant(NINE)]);
    assert_eq!(found.name, "Refused");
    assert_eq!(found.broken.as_deref(), Some("`ends_after_start`"));
    // A booking the right way round is built.
    let Value::Record(booking) = value(&program, "book", vec![instant(NINE), instant(TEN)]) else {
        panic!("a booking");
    };
    assert_eq!(booking.get("ends"), Some(&instant(TEN)));
}

#[test]
fn a_proved_construction_holds_without_a_check() {
    let program = main(SOURCE).with_proof_checks();
    assert!(matches!(
        value(&program, "book_checked", vec![instant(NINE), instant(TEN)]),
        Value::Record(_)
    ));
    assert_eq!(
        failure(&program, "book_checked", vec![instant(TEN), instant(NINE)]).name,
        "Backwards"
    );
}

#[test]
fn a_matched_field_and_a_standard_operation_run_as_written() {
    let program = main(SOURCE);
    assert!(matches!(
        value(&program, "search", vec![day("2026-02-01"), int(2)]),
        Value::Record(_)
    ));
    let found = failure(&program, "search", vec![day("2026-04-01"), int(2)]);
    assert_eq!(found.broken.as_deref(), Some("`ordered`"));
    let found = failure(&program, "search", vec![day("2026-02-01"), int(9)]);
    assert_eq!(found.broken.as_deref(), Some("`small`"));
}

#[test]
fn json_that_breaks_a_rule_names_it_at_the_object_s_path() {
    let program = main(SOURCE);
    let problem = |field: &str, rule: &str| {
        Value::record(
            "DecodeProblem",
            vec![
                (Rc::from("field"), text(field)),
                (Rc::from("rule"), text(rule)),
            ],
        )
    };
    let backwards =
        format!(r#"{{"booking": {{"starts": "{TEN}", "ends": "{NINE}"}}, "note": "window seat"}}"#);
    let found = failure(&program, "read_request", vec![text(&backwards)]);
    assert_eq!(found.name, "Invalid");
    assert_eq!(
        found.client,
        [(
            "problem".to_string(),
            problem("booking", "ends_after_start")
        )]
    );
    let forwards =
        format!(r#"{{"booking": {{"starts": "{NINE}", "ends": "{TEN}"}}, "note": "window seat"}}"#);
    assert!(matches!(
        value(&program, "read_request", vec![text(&forwards)]),
        Value::Record(_)
    ));
}
