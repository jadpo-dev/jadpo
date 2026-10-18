//! Area 19, "Time and calendar additions" (chunk 5.5k3), in the checker:
//! a `CalendarEvent` built from its parts, an all-day event's days beside a
//! timed one's local times and zone, its rules across them proved where a
//! literal shows them, its text, and its columns.

use crate::support;

use jadpo_db::{Database, Layout};

const NAMES: &str = "type Wire = Text {}\n\ntype Day = CalendarDate {}\n\ntype Unreadable = Failure {\n    kind: FailureKind.invalid_value\n}\n\n";

fn rules(source: &str) -> Vec<String> {
    support::rules(
        Layout::Loose,
        &[("main.jadpo", &format!("{NAMES}{source}"))],
    )
}

const NONE: &[&str] = &[];

/// A `LocalDateTime` literal.
fn at(date: &str, hour: u32, minute: u32) -> String {
    format!(
        "LocalDateTime {{\n            date: \"{date}\"\n            time: LocalTime {{\n                hour: {hour}\n                minute: {minute}\n                second: 0\n            }}\n        }}"
    )
}

/// A function returning an event literal with these parts, each written as
/// given.
fn event(parts: &[(&str, &str)]) -> String {
    let get = |name: &str| {
        parts
            .iter()
            .find(|(part, _)| *part == name)
            .map_or("none".to_string(), |(_, value)| (*value).to_string())
    };
    let summary = parts
        .iter()
        .find(|(part, _)| *part == "summary")
        .map_or("\"Night shift\"", |(_, value)| value);
    format!(
        "function made() -> CalendarEvent {{\n    return CalendarEvent {{\n        summary: {summary}\n        first_day: {}\n        last_day: {}\n        start: {}\n        end: {}\n        zone: {}\n        location: {}\n        description: {}\n        recurrence: {}\n    }}\n}}\n",
        get("first_day"),
        get("last_day"),
        get("start"),
        get("end"),
        get("zone"),
        get("location"),
        get("description"),
        get("recurrence"),
    )
}

#[test]
fn an_event_is_all_day_or_timed() {
    let all_day = [
        ("first_day", "\"2026-12-25\""),
        ("last_day", "\"2026-12-25\""),
    ];
    assert_eq!(rules(&event(&all_day)), NONE);
    let start = at("2026-10-24", 22, 0);
    let end = at("2026-10-25", 2, 0);
    let timed = [
        ("start", start.as_str()),
        ("end", end.as_str()),
        ("zone", "Zone.europe_london"),
    ];
    assert_eq!(rules(&event(&timed)), NONE, "a timed event past midnight");
    assert_eq!(
        rules(&event(&[("first_day", "\"2026-12-25\"")])),
        ["type.invalid_literal"],
        "an all-day event has both its days"
    );
    assert_eq!(
        rules(&event(&[
            ("first_day", "\"2026-12-25\""),
            ("last_day", "\"2026-12-25\""),
            ("zone", "Zone.europe_london"),
        ])),
        ["type.invalid_literal"],
        "and no zone"
    );
    assert_eq!(
        rules(&event(&[("start", start.as_str()), ("end", end.as_str())])),
        ["type.invalid_literal"],
        "a timed event has its zone"
    );
    assert_eq!(
        rules(&event(&[])),
        ["type.invalid_literal"],
        "an event has its days or its times"
    );
}

#[test]
fn days_and_times_come_in_order() {
    assert_eq!(
        rules(&event(&[
            ("first_day", "\"2026-12-26\""),
            ("last_day", "\"2026-12-25\""),
        ])),
        ["type.invalid_literal"],
        "the last day is on or after the first"
    );
    let start = at("2026-10-25", 2, 0);
    let end = at("2026-10-24", 22, 0);
    assert_eq!(
        rules(&event(&[
            ("start", start.as_str()),
            ("end", end.as_str()),
            ("zone", "Zone.europe_london"),
        ])),
        ["type.invalid_literal"],
        "an end before the start"
    );
    assert_eq!(
        rules(&event(&[
            ("start", start.as_str()),
            ("end", start.as_str()),
            ("zone", "Zone.europe_london"),
        ])),
        ["type.invalid_literal"],
        "an end at the start"
    );
}

#[test]
fn an_event_recurs_from_its_start() {
    let start = at("2026-10-24", 22, 0);
    let end = at("2026-10-25", 2, 0);
    let weekly = |rule: &'static str| {
        [
            ("start", start.clone()),
            ("end", end.clone()),
            ("zone", "Zone.europe_london".to_string()),
            ("recurrence", rule.to_string()),
        ]
    };
    let parts = |parts: &[(&'static str, String)]| -> Vec<String> {
        let borrowed: Vec<(&str, &str)> = parts.iter().map(|(p, v)| (*p, v.as_str())).collect();
        rules(&event(&borrowed))
    };
    assert_eq!(
        parts(&weekly(
            "\"DTSTART;TZID=Europe/London:20261024T220000\\nRRULE:FREQ=WEEKLY;COUNT=4\""
        )),
        NONE
    );
    assert_eq!(
        parts(&weekly(
            "\"DTSTART;TZID=Europe/London:20261031T220000\\nRRULE:FREQ=WEEKLY;COUNT=4\""
        )),
        ["type.invalid_literal"],
        "a recurrence from another start"
    );
    assert_eq!(
        parts(&weekly(
            "\"DTSTART;TZID=Europe/Paris:20261024T220000\\nRRULE:FREQ=WEEKLY;COUNT=4\""
        )),
        ["type.invalid_literal"],
        "or in another zone"
    );
    assert_eq!(
        rules(&event(&[
            ("first_day", "\"2026-12-25\""),
            ("last_day", "\"2026-12-25\""),
            (
                "recurrence",
                "\"DTSTART;TZID=Europe/London:20261225T000000\\nRRULE:FREQ=YEARLY\""
            ),
        ])),
        ["type.invalid_literal"],
        "an all-day event recurs on days, not at a time"
    );
    // An all-day event recurs on days from its first (chunk 5.5k4).
    assert_eq!(
        rules(&event(&[
            ("first_day", "\"2024-02-29\""),
            ("last_day", "\"2024-02-29\""),
            (
                "recurrence",
                "\"DTSTART;VALUE=DATE:20240229\\nRRULE:FREQ=YEARLY\""
            ),
        ])),
        NONE
    );
    assert_eq!(
        rules(&event(&[
            ("first_day", "\"2024-02-29\""),
            ("last_day", "\"2024-02-29\""),
            (
                "recurrence",
                "\"DTSTART;VALUE=DATE:20280229\\nRRULE:FREQ=YEARLY\""
            ),
        ])),
        ["type.invalid_literal"],
        "from another day"
    );
    assert_eq!(
        parts(&weekly(
            "\"DTSTART;VALUE=DATE:20261024\\nRRULE:FREQ=WEEKLY;COUNT=4\""
        )),
        ["type.invalid_literal"],
        "a timed event recurs at its start, not on days"
    );
}

#[test]
fn parts_from_values_are_attempted_by_their_rules() {
    // Days that may be `none` may break `days_or_times`, and their order
    // `last_day_not_before_first` (chunk 5.5t2).
    let made = |arms: &str| {
        rules(&format!(
            "function made(holiday: CalendarEvent) fails Unreadable -> CalendarEvent {{\n    return attempt CalendarEvent {{\n        summary: \"Holiday\"\n        first_day: holiday.first_day\n        last_day: holiday.last_day\n        start: none\n        end: none\n        zone: none\n        location: none\n        description: none\n        recurrence: none\n    }} {{\n{arms}    }}\n}}\n"
        ))
    };
    assert_eq!(
        made("        days_or_times: Unreadable\n        last_day_not_before_first: Unreadable\n"),
        NONE
    );
    assert_eq!(
        made("        days_or_times: Unreadable\n"),
        ["failure.rule_unmapped"]
    );
    assert_eq!(
        rules(
            "function made() -> CalendarEvent {\n    return CalendarEvent {\n        summary: \"Holiday\"\n        first_day: \"2026-12-24\"\n        last_day: none\n        start: none\n        end: none\n        zone: none\n        location: none\n        description: none\n        recurrence: none\n    }\n}\n"
        ),
        ["type.invalid_literal"],
        "a day written out is given"
    );
}

#[test]
fn its_text_parts_are_kept_to_their_rules() {
    let days = [
        ("first_day", "\"2026-12-25\""),
        ("last_day", "\"2026-12-25\""),
    ];
    let with = |part: (&'static str, &'static str)| {
        let mut parts = days.to_vec();
        parts.push(part);
        rules(&event(&parts))
    };
    assert_eq!(with(("summary", "\"\"")), ["type.invalid_literal"]);
    assert_eq!(
        with(("location", "\"Bath\\nSomerset\"")),
        ["type.invalid_literal"]
    );
    assert_eq!(
        with(("description", "\"Crackers,\\nand cake; at noon\"")),
        NONE,
        "a description runs over lines"
    );
    assert_eq!(
        with(("description", "\"Crackers\\r\\nand cake\"")),
        ["type.invalid_literal"],
        "with no carriage return"
    );
}

#[test]
fn an_event_has_its_text_and_is_read_from_it() {
    assert_eq!(
        rules(
            "function written(event: CalendarEvent) -> Wire {\n    return Wire(text.from(event))\n}\n"
        ),
        NONE
    );
    assert_eq!(
        rules(
            "function read(raw: Wire) fails Unreadable -> CalendarEvent {\n    var event: CalendarEvent = attempt text.parse(raw, as: CalendarEvent) {\n        malformed: Unreadable\n        invalid: Unreadable\n    }\n    return event\n}\n"
        ),
        NONE
    );
    assert_eq!(
        rules(
            "function first(event: CalendarEvent) -> Day {\n    return Day(event.first_day)\n}\n"
        ),
        ["type.constructor_input"],
        "a day that may be `none` is matched first"
    );
    assert_eq!(
        rules(
            "function first(event: CalendarEvent) -> Day? {\n    return match event.first_day {\n        some(day) -> Day(day)\n        none -> none\n    }\n}\n"
        ),
        NONE
    );
}

#[test]
fn an_event_is_stored_in_a_column_per_part() {
    let db = Database::default();
    let source = "entity Booking {\n    id: Uuid\n    slot: CalendarEvent\n}\n";
    let project = support::project(&db, Layout::Loose, &[("main.jadpo", source)]);
    let found: Vec<String> = jadpo_check::check(&db, project)
        .iter()
        .map(|d| d.rule.id.to_string())
        .collect();
    assert_eq!(found, NONE);
    let model = jadpo_check::data::data_model(&db, project);
    let booking = model.entity("Booking").expect("Booking");
    let columns: Vec<(String, String)> = booking
        .fields
        .iter()
        .flat_map(|field| field.columns())
        .collect();
    let columns: Vec<(&str, &str)> = columns
        .iter()
        .map(|(name, ty)| (name.as_str(), ty.as_str()))
        .collect();
    assert_eq!(
        columns,
        [
            ("id", "uuid"),
            ("slot_summary", "text"),
            ("slot_first_day", "date"),
            ("slot_last_day", "date"),
            ("slot_start", "text"),
            ("slot_end", "text"),
            ("slot_zone", "text"),
            ("slot_location", "text"),
            ("slot_description", "text"),
            ("slot_recurrence", "text"),
        ]
    );
    let schema = jadpo_check::data::schema(&db, project);
    let table = schema
        .stores
        .values()
        .flat_map(|store| store.tables.values())
        .next()
        .expect("a table");
    let nullable = |name: &str| table.columns.get(name).map(|column| column.nullable);
    assert_eq!(nullable("slot_summary"), Some(false));
    assert_eq!(nullable("slot_first_day"), Some(true));
    assert_eq!(nullable("slot_zone"), Some(true));
}
