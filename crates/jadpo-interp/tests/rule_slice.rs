//! Rules across fields on every way a value is made (area 2, "Rules across
//! fields"; area 19, "Rules across a type's parts"), the slice the Phase 5
//! soundness review couldn't cover (chunk 5.8c): every standard object
//! type's named rules broken in JSON, by `json.read`, each named at the
//! value's path; the text forms `text.parse` reads, each naming its rule;
//! and an application's own rule inside a list and a map in JSON, at the
//! item's path.

use std::rc::Rc;

use crate::support;

use jadpo_interp::Value;
use support::{failure, main, text, value};

/// The standard object types with rules, as `json.read` reads them.
const TYPES: &[&str] = &[
    "InstantRange",
    "DateRange",
    "IsoWeek",
    "CalendarEvent",
    "PostalAddress",
    "ContentRating",
    "Timecode",
    "SubtitleCue",
    "Rating",
    "BloodPressure",
    "MapTile",
    "BoundingBox",
    "Polygon",
    "LineString",
    "MultiPoint",
    "MultiLineString",
    "MultiPolygon",
    "TokenUsage",
    "PasswordPolicy",
    "PassportNumber",
    "DrivingLicenceNumber",
    "TaxId",
    "VatNumber",
    "IdentityDocument",
];

/// The types `text.parse` reads with rules its text can break.
const TEXT_TYPES: &[&str] = &[
    "InstantRange",
    "DateRange",
    "IsoWeek",
    "ContentRating",
    "VatNumber",
    "SubtitleCue",
];

fn snake(name: &str) -> String {
    let mut out = String::new();
    for (i, c) in name.chars().enumerate() {
        if c.is_ascii_uppercase() && i > 0 {
            out.push('_');
        }
        out.push(c.to_ascii_lowercase());
    }
    out
}

fn source() -> String {
    let mut source = String::from(
        "type Wire = Text {}

type Unreadable = Failure {
    kind: FailureKind.invalid_value
}

type Broken = Failure {
    kind: FailureKind.invalid_value
}

type Invalid = Failure {
    kind: FailureKind.invalid_value
    client: {
        problem: DecodeProblem
    }
}

type Moment = Instant {}

type Stay = Object {
    arrives: Moment
    leaves: Moment
    rule leaves_after_arrival: leaves > arrives
}

type Stays = List<Stay> {}

type StaysByGuest = Map<Wire, Stay> {}

type Trip = Input {
    stays: Stays
    by_guest: StaysByGuest
}

function read_trip(feed: Wire) fails Unreadable, Invalid -> Trip {
    return attempt json.read(feed, as: Trip) {
        malformed: Unreadable
        mismatch: Invalid
    }
}
",
    );
    for ty in TYPES {
        source.push_str(&format!(
            "
function read_{}(feed: Wire) fails Unreadable, Invalid -> {ty} {{
    return attempt json.read(feed, as: {ty}) {{
        malformed: Unreadable
        mismatch: Invalid
    }}
}}
",
            snake(ty)
        ));
    }
    for ty in TEXT_TYPES {
        source.push_str(&format!(
            "
function parse_{}(raw: Wire) fails Unreadable, Broken -> {ty} {{
    return attempt text.parse(raw, as: {ty}) {{
        malformed: Unreadable
        invalid: Broken
    }}
}}
",
            snake(ty)
        ));
    }
    source
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

#[test]
fn json_that_breaks_a_standard_type_s_rule_names_it() {
    let program = main(&source()).with_proof_checks();
    let event = |first: &str, last: &str, start: &str, end: &str, zone: &str| {
        format!(
            r#"{{"summary": "Fair", "first_day": {first}, "last_day": {last}, "start": {start}, "end": {end}, "zone": {zone}, "location": null, "description": null, "recurrence": null}}"#
        )
    };
    let cases: Vec<(&str, String, &str, &str)> = vec![
        (
            "InstantRange",
            r#"{"start": "2026-10-25T17:00:00Z", "end": "2026-10-25T09:00:00Z"}"#.into(),
            "",
            "start_not_after_end",
        ),
        (
            "DateRange",
            r#"{"start": "2026-10-31", "end": "2026-10-01"}"#.into(),
            "",
            "start_not_after_end",
        ),
        ("IsoWeek", r#""2027-W53""#.into(), "", "week_in_year"),
        (
            "CalendarEvent",
            event(
                r#""2026-12-26""#,
                r#""2026-12-25""#,
                "null",
                "null",
                "null",
            ),
            "",
            "last_day_not_before_first",
        ),
        (
            "CalendarEvent",
            event(
                "null",
                "null",
                r#""2026-12-25T10:00:00""#,
                r#""2026-12-25T09:00:00""#,
                r#""Europe/London""#,
            ),
            "",
            "end_after_start",
        ),
        (
            "PostalAddress",
            r#"{"lines":["1 High Street"],"locality":"London","region":null,"postal_code":"75004","country":"GB"}"#.into(),
            "",
            "postal_code_of_country",
        ),
        (
            "Timecode",
            r#"{"hours":0,"minutes":0,"seconds":0,"frames":25,"rate":"25/1","drop_frame":false}"#.into(),
            "",
            "frames_within_rate",
        ),
        (
            "Timecode",
            r#"{"hours":0,"minutes":0,"seconds":0,"frames":0,"rate":"25/1","drop_frame":true}"#.into(),
            "",
            "drop_frame_rate",
        ),
        (
            "Timecode",
            r#"{"hours":0,"minutes":1,"seconds":0,"frames":0,"rate":"30000/1001","drop_frame":true}"#.into(),
            "",
            "frame_not_dropped",
        ),
        (
            "Rating",
            r#"{"value":"1","best":"1","worst":"5"}"#.into(),
            "",
            "best_above_worst",
        ),
        (
            "Rating",
            r#"{"value":"6","best":"5","worst":"1"}"#.into(),
            "",
            "value_on_scale",
        ),
        (
            "BloodPressure",
            r#"{"systolic":{"value":"80","unit":"mmHg"},"diastolic":{"value":"120","unit":"mmHg"}}"#.into(),
            "",
            "diastolic_below_systolic",
        ),
        (
            "MapTile",
            r#"{"zoom":1,"x":2,"y":0}"#.into(),
            "",
            "within_zoom",
        ),
        (
            "BoundingBox",
            r#"{"south":"52","west":"-0.51","north":"51","east":"0.33"}"#.into(),
            "",
            "south_not_above_north",
        ),
        (
            "TokenUsage",
            r#"{"input":10,"output":1,"cached":11}"#.into(),
            "",
            "cached_within_input",
        ),
        (
            "PasswordPolicy",
            r#"{"min_length":"80","max_length":"64","check_breached":true}"#.into(),
            "",
            "min_not_above_max",
        ),
        (
            "PassportNumber",
            r#"{"country":"GB","number":"12"}"#.into(),
            "",
            "number_of_country",
        ),
        (
            "IdentityDocument",
            r#"{"kind":"passport","country":"GB","number":"12","expires":"2030-01-31"}"#.into(),
            "",
            "number_of_kind_and_country",
        ),
        // GeoJSON whose positions mix altitudes is refused as it is read,
        // `format` at the first of the other size (area 20, chunk 5.5u7),
        // never built.
        (
            "LineString",
            r#"{"type":"LineString","coordinates":[[0,0,5],[1,1]]}"#.into(),
            "coordinates.1",
            "format",
        ),
        (
            "MultiPoint",
            r#"{"type":"MultiPoint","coordinates":[[0,0],[1,1,5]]}"#.into(),
            "coordinates.1",
            "format",
        ),
        (
            "MultiLineString",
            r#"{"type":"MultiLineString","coordinates":[[[0,0],[1,1]],[[2,2,1],[3,3,1]]]}"#.into(),
            "coordinates.1.0",
            "format",
        ),
        (
            "Polygon",
            r#"{"type":"Polygon","coordinates":[[[0,0,1],[1,0,1],[1,1],[0,1,1],[0,0,1]]]}"#.into(),
            "coordinates.0.2",
            "format",
        ),
        (
            "MultiPolygon",
            r#"{"type":"MultiPolygon","coordinates":[[[[0,0],[1,0],[1,1],[0,1],[0,0]]],[[[2,2,1],[3,2,1],[3,3,1],[2,3,1],[2,2,1]]]]}"#.into(),
            "coordinates.1.0.0",
            "format",
        ),
    ];
    let mut wrong = Vec::new();
    for (ty, feed, field, rule) in &cases {
        let read = format!("read_{}", snake(ty));
        let found = failure(&program, &read, vec![text(feed)]);
        let wanted = [("problem".to_string(), problem(field, rule))];
        if found.name != "Invalid" || found.client != wanted {
            wrong.push(format!("{ty} {feed}: {} {:?}", found.name, found.client));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
fn text_that_breaks_a_standard_type_s_rule_names_it() {
    let program = main(&source()).with_proof_checks();
    for (ty, raw, rule) in [
        (
            "InstantRange",
            "2026-10-25T17:00:00Z/2026-10-25T09:00:00Z",
            "start_not_after_end",
        ),
        ("DateRange", "2026-10-31/2026-10-01", "start_not_after_end"),
        ("IsoWeek", "2027-W53", "week_in_year"),
        (
            "SubtitleCue",
            "00:00:04.000 --> 00:00:01.000\nBackwards",
            "end_after_start",
        ),
    ] {
        let parse = format!("parse_{}", snake(ty));
        let found = failure(&program, &parse, vec![text(raw)]);
        assert_eq!(found.name, "Broken", "{ty} {raw}");
        assert_eq!(found.broken, Some(format!("`{rule}`")), "{ty} {raw}");
    }
}

#[test]
fn an_application_s_rule_is_named_at_its_item_in_json() {
    let program = main(&source()).with_proof_checks();
    let stay = |arrives: &str, leaves: &str| {
        format!(
            r#"{{"arrives": "2026-03-0{arrives}T12:00:00Z", "leaves": "2026-03-0{leaves}T10:00:00Z"}}"#
        )
    };
    let trip = |stays: &[String], by_guest: &[(&str, String)]| {
        let by_guest: Vec<String> = by_guest
            .iter()
            .map(|(guest, stay)| format!(r#""{guest}": {stay}"#))
            .collect();
        format!(
            r#"{{"stays": [{}], "by_guest": {{{}}}}}"#,
            stays.join(", "),
            by_guest.join(", ")
        )
    };
    assert!(matches!(
        value(
            &program,
            "read_trip",
            vec![text(&trip(&[stay("1", "3")], &[("ada", stay("1", "2"))]))]
        ),
        Value::Record(_)
    ));
    let found = failure(
        &program,
        "read_trip",
        vec![text(&trip(&[stay("1", "3"), stay("4", "2")], &[]))],
    );
    assert_eq!(
        found.client,
        [(
            "problem".to_string(),
            problem("stays.1", "leaves_after_arrival")
        )]
    );
    // A map's entry is `<key>`, never the key (chunk 5.8a).
    let found = failure(
        &program,
        "read_trip",
        vec![text(&trip(&[], &[("ada", stay("5", "2"))]))],
    );
    assert_eq!(
        found.client,
        [(
            "problem".to_string(),
            problem("by_guest.<key>", "leaves_after_arrival")
        )]
    );
    // A member named twice is no document at all (chunk 5.8c).
    let twice = r#"{"stays": [], "by_guest": {}, "stays": []}"#;
    assert_eq!(
        failure(&program, "read_trip", vec![text(twice)]).name,
        "Unreadable"
    );
}
