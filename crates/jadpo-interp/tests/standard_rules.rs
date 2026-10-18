//! Area 19, the standard object types' rules across their parts (chunk
//! 5.5t2), run: a value built in code that breaks one fails its
//! construction by the rule's name, mapped by its own arm or `invalid:`;
//! `text.parse` and JSON reading one that breaks it name the same rule, JSON
//! at the object's path; and literals the checker proves hold under proof
//! checks, so the checker and the runtime read each rule one way.

use std::rc::Rc;

use crate::support;

use jadpo_interp::Value;
use support::{failure, int, main, text, value};

const SOURCE: &str = "type Wire = Text {}

type Broken = Failure {
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

type Delivery = Input {
    slot: InstantRange
    week: IsoWeek
    address: PostalAddress
}

function reversed(window: InstantRange) fails Broken -> InstantRange {
    return attempt InstantRange {
        start: window.end
        end: window.start
    } {
        start_not_after_end: Broken
    }
}

function week_of(year: IsoWeek.year, week: IsoWeek.week) fails Broken -> IsoWeek {
    return attempt IsoWeek {
        year: year
        week: week
    } {
        invalid: Broken
    }
}

function tile(x: MapTile.x) fails Broken -> MapTile {
    return attempt MapTile {
        zoom: 1
        x: x
        y: 0
    } {
        within_zoom: Broken
    }
}

function usage(input: TokenCount, cached: TokenCount) fails Broken -> TokenUsage {
    return attempt TokenUsage {
        input: input
        output: 5
        cached: cached
    } {
        cached_within_input: Broken
    }
}

function office(code: PostalCode) fails Broken -> PostalAddress {
    return attempt PostalAddress {
        lines: [\"1 High Street\"]
        locality: none
        region: none
        postal_code: code
        country: Country.gb
    } {
        postal_code_of_country: Broken
    }
}

function read_week(raw: Wire) fails Unreadable, Broken -> IsoWeek {
    return attempt text.parse(raw, as: IsoWeek) {
        malformed: Unreadable
        week_in_year: Broken
    }
}

function read_cue(raw: Wire) fails Unreadable, Broken -> SubtitleCue {
    return attempt text.parse(raw, as: SubtitleCue) {
        malformed: Unreadable
        invalid: Broken
    }
}

function read_delivery(feed: Wire) fails Unreadable, Invalid -> Delivery {
    return attempt json.read(feed, as: Delivery) {
        malformed: Unreadable
        mismatch: Invalid
    }
}

function read_event(feed: Wire) fails Unreadable, Invalid -> CalendarEvent {
    return attempt json.read(feed, as: CalendarEvent) {
        malformed: Unreadable
        mismatch: Invalid
    }
}

function written() -> List<Timecode> {
    return [
        Timecode {
            hours: 0
            minutes: 10
            seconds: 0
            frames: 0
            rate: \"30000/1001\"
            drop_frame: true
        },
        Timecode {
            hours: 1
            minutes: 1
            seconds: 0
            frames: 2
            rate: FrameRate {
                numerator: 30000
                denominator: 1001
            }
            drop_frame: true
        }
    ]
}

function caption() -> SubtitleCue {
    return SubtitleCue {
        start: 0s
        end: 4s
        text: \"Never drink liquid nitrogen.\"
    }
}

function fair() -> CalendarEvent {
    return CalendarEvent {
        summary: \"Fair\"
        first_day: \"2026-12-25\"
        last_day: \"2026-12-26\"
        start: none
        end: none
        zone: none
        location: none
        description: none
        recurrence: \"DTSTART;VALUE=DATE:20261225\\nRRULE:FREQ=YEARLY\"
    }
}

function band() -> Range<TokenCount> {
    return Range {
        start: 10
        end: none
    }
}
";

fn instant(text: &str) -> Value {
    Value::Instant(jadpo_std::instant_millis(text).expect("an instant"))
}

fn window(start: &str, end: &str) -> Value {
    Value::record(
        "InstantRange",
        vec![("start".into(), instant(start)), ("end".into(), instant(end))],
    )
}

fn problem(field: &str, rule: &str) -> Value {
    Value::record(
        "DecodeProblem",
        vec![
            (Rc::from("field"), text(field)),
            (Rc::from("rule"), text(rule)),
        ],
    )
}

const NINE: &str = "2026-03-02T09:00:00Z";
const TEN: &str = "2026-03-02T10:00:00Z";

#[test]
fn a_value_built_in_code_fails_by_the_rule_s_name() {
    let program = main(SOURCE).with_proof_checks();
    let found = failure(&program, "reversed", vec![window(NINE, TEN)]);
    assert_eq!(found.name, "Broken");
    assert_eq!(found.broken.as_deref(), Some("`start_not_after_end`"));
    // An empty range keeps it.
    assert!(matches!(
        value(&program, "reversed", vec![window(TEN, TEN)]),
        Value::Record(_)
    ));
    let found = failure(&program, "week_of", vec![int(2027), int(53)]);
    assert_eq!(found.broken.as_deref(), Some("`week_in_year`"));
    assert!(matches!(
        value(&program, "week_of", vec![int(2026), int(53)]),
        Value::Record(_)
    ));
    let found = failure(&program, "tile", vec![int(2)]);
    assert_eq!(found.broken.as_deref(), Some("`within_zoom`"));
    assert!(matches!(value(&program, "tile", vec![int(1)]), Value::Record(_)));
    let found = failure(&program, "usage", vec![int(10), int(11)]);
    assert_eq!(found.broken.as_deref(), Some("`cached_within_input`"));
    let found = failure(&program, "office", vec![text("75004")]);
    assert_eq!(found.broken.as_deref(), Some("`postal_code_of_country`"));
    assert!(matches!(
        value(&program, "office", vec![text("SW1A 1AA")]),
        Value::Record(_)
    ));
}

#[test]
fn text_that_breaks_a_rule_names_it() {
    let program = main(SOURCE).with_proof_checks();
    assert_eq!(
        failure(&program, "read_week", vec![text("2027-W53")]).broken.as_deref(),
        Some("`week_in_year`")
    );
    assert_eq!(
        failure(&program, "read_week", vec![text("2027-W54")]).name,
        "Unreadable"
    );
    assert!(matches!(
        value(&program, "read_week", vec![text("2026-W53")]),
        Value::Record(_)
    ));
    let found = failure(
        &program,
        "read_cue",
        vec![text("00:00:04.000 --> 00:00:01.000\nBackwards")],
    );
    assert_eq!(found.name, "Broken");
    assert_eq!(found.broken.as_deref(), Some("`end_after_start`"));
}

#[test]
fn json_that_breaks_a_rule_names_it_at_the_object_s_path() {
    let program = main(SOURCE).with_proof_checks();
    let delivery = |slot: &str, week: &str, code: &str| {
        format!(
            r#"{{"slot": {slot}, "week": "{week}", "address": {{"lines": ["1 High Street"], "locality": null, "region": null, "postal_code": "{code}", "country": "GB"}}}}"#
        )
    };
    let forwards = format!(r#"{{"start": "{NINE}", "end": "{TEN}"}}"#);
    let backwards = format!(r#"{{"start": "{TEN}", "end": "{NINE}"}}"#);
    assert!(matches!(
        value(
            &program,
            "read_delivery",
            vec![text(&delivery(&forwards, "2026-W53", "SW1A 1AA"))]
        ),
        Value::Record(_)
    ));
    for (feed, field, rule) in [
        (
            delivery(&backwards, "2026-W53", "SW1A 1AA"),
            "slot",
            "start_not_after_end",
        ),
        (delivery(&forwards, "2027-W53", "SW1A 1AA"), "week", "week_in_year"),
        (
            delivery(&forwards, "2026-W53", "75004"),
            "address",
            "postal_code_of_country",
        ),
    ] {
        let found = failure(&program, "read_delivery", vec![text(&feed)]);
        assert_eq!(found.name, "Invalid", "{feed}");
        assert_eq!(
            found.client,
            [("problem".to_string(), problem(field, rule))],
            "{feed}"
        );
    }
    // A whole input that breaks one is a problem at the empty path.
    let both = r#"{"summary": "Fair", "first_day": "2026-12-25", "last_day": "2026-12-25", "start": null, "end": null, "zone": "Europe/London", "location": null, "description": null, "recurrence": null}"#;
    let found = failure(&program, "read_event", vec![text(both)]);
    assert_eq!(
        found.client,
        [("problem".to_string(), problem("", "days_or_times"))]
    );
}

#[test]
fn literals_the_checker_proves_hold_when_run() {
    let program = main(SOURCE).with_proof_checks();
    for name in ["written", "caption", "fair", "band"] {
        assert!(
            !matches!(value(&program, name, vec![]), Value::None),
            "{name}"
        );
    }
}
