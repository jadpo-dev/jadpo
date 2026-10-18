//! A `CalendarEvent`'s values (area 19, "Time and calendar additions",
//! chunk 5.5k3): its record turned into the event `jadpo-std`'s `events`
//! writes as an RFC 5545 `VEVENT` and reads back. The rules across its
//! parts are its named rules (chunk 5.5t2), run as every standard type's.

use std::rc::Rc;

use jadpo_std::standard::events::{Event, When};
use jadpo_std::standard::recurrence::LocalAt;

use crate::{Fault, Record, Value};

/// A `LocalDateTime` as a local time.
fn local_at(value: &Value) -> Result<LocalAt, Fault> {
    let (date, millis) = crate::calendar::local_date_time(value)?;
    Ok(LocalAt {
        date,
        second: millis / 1000,
    })
}

/// A local time as a `LocalDateTime`.
fn local_date_time(at: LocalAt) -> Value {
    let second = i64::from(at.second);
    Value::record(
        "LocalDateTime",
        vec![
            ("date".into(), Value::Date(at.date)),
            (
                "time".into(),
                crate::calendar::local_time(second / 3600, second / 60 % 60, second % 60),
            ),
        ],
    )
}

/// A zone's place among `Zone`'s variants.
fn zone_of(value: &Value) -> Option<u32> {
    match value {
        Value::Variant(variant) if &*variant.ty == jadpo_std::ZONE => Some(variant.ordinal),
        _ => None,
    }
}

/// The `Zone` variant at `ordinal`.
fn zone(ordinal: u32) -> Value {
    let name = jadpo_std::ZONES
        .get(ordinal as usize)
        .map_or("", |(name, _)| *name);
    Value::variant(jadpo_std::ZONE, ordinal, name, Vec::new())
}

fn text_of(value: Option<&Value>) -> Option<String> {
    match value {
        Some(Value::Text(text)) => Some(text.to_string()),
        _ => None,
    }
}

/// The event a record holds, by its parts.
fn event(record: &Record) -> Result<Event, Fault> {
    let part = |name: &str| record.get(name).unwrap_or(&Value::None);
    let when = match (
        part("first_day"),
        part("last_day"),
        part("start"),
        part("end"),
        part("zone"),
    ) {
        (Value::Date(first), Value::Date(last), Value::None, Value::None, Value::None) => {
            When::Days {
                first: *first,
                last: *last,
            }
        }
        (Value::None, Value::None, start, end, zone) => When::Timed {
            start: local_at(start)?,
            end: local_at(end)?,
            zone: zone_of(zone).ok_or(Fault::Shape)?,
        },
        _ => return Err(Fault::Shape),
    };
    Ok(Event {
        summary: text_of(record.get("summary")).ok_or(Fault::Shape)?,
        when,
        location: text_of(record.get("location")),
        description: text_of(record.get("description")),
        recurrence: text_of(record.get("recurrence")),
    })
}

/// A `CalendarEvent`'s canonical text. An event whose last day is the last
/// a date holds has no `DTEND` to write, the day after it.
pub fn event_text(record: &Record) -> Result<String, Fault> {
    event(record)?.text().ok_or(Fault::TimeRange)
}

/// A `CalendarEvent` read from its canonical text.
pub fn read_event(raw: &str) -> Option<Value> {
    let event = Event::read(raw)?;
    let optional = |text: &Option<String>| text.as_deref().map_or(Value::None, Value::text);
    let (first_day, last_day, start, end, zone_value) = match event.when {
        When::Days { first, last } => (
            Value::Date(first),
            Value::Date(last),
            Value::None,
            Value::None,
            Value::None,
        ),
        When::Timed {
            start,
            end,
            zone: at,
        } => (
            Value::None,
            Value::None,
            local_date_time(start),
            local_date_time(end),
            zone(at),
        ),
    };
    let fields: Vec<(Rc<str>, Value)> = vec![
        ("summary".into(), Value::text(&event.summary)),
        ("first_day".into(), first_day),
        ("last_day".into(), last_day),
        ("start".into(), start),
        ("end".into(), end),
        ("zone".into(), zone_value),
        ("location".into(), optional(&event.location)),
        ("description".into(), optional(&event.description)),
        ("recurrence".into(), optional(&event.recurrence)),
    ];
    Some(Value::record(
        jadpo_std::standard::events::CALENDAR_EVENT,
        fields,
    ))
}
