//! The time group's schedules held as text (area 19, "Time and calendar
//! additions", chunk 5.5k2): `RecurrenceRule`, in RFC 5545's text, and
//! `OpeningHours`, in OpenStreetMap's, each with its zone. A value is the
//! text, as for the identifiers of chunk 5.5i: the wire, storage and
//! `text.parse` carry it, construction checks it with its validator
//! ([`super::recurrence`], [`super::opening_hours`]), and `text.format`
//! shows it as written, since CLDR has no words for either.

use super::{
    Examples, FIRST_RELEASE, Format, FormatKey, Holds, Json, NO_OPTIONS, StandardType, Storage,
    TextForm, Validator,
};
use crate::read::SqlType;

const fn schedule(
    name: &'static str,
    pattern: &'static str,
    validator: Validator,
    valid: &'static [(&'static str, &'static str)],
    malformed: &'static [&'static str],
) -> StandardType {
    StandardType {
        name,
        holds: Holds::Text {
            pattern,
            personal: None,
        },
        text: Some(TextForm::Checked),
        json: Json::Text,
        storage: Storage::Column(SqlType::Text),
        format: Format {
            key: FormatKey::Canonical,
            options: NO_OPTIONS,
        },
        ordered: false,
        validator: Some(validator),
        shape: None,
        examples: Examples {
            valid,
            format: None,
            malformed,
            of: None,
            settings: "",
        },
        release: FIRST_RELEASE,
        signals: crate::standard::NO_SIGNALS,
    }
}

/// When a recurring thing happens (RFC 5545): its first occurrence as a
/// local time in its zone, or a date alone (chunk 5.5k4), then its rule,
/// on two lines.
pub const RECURRENCE_RULE: StandardType = schedule(
    super::RECURRENCE_RULE,
    "DTSTART(?:;TZID=[A-Za-z0-9/_+-]+:[0-9]{8}T[0-9]{6}|;VALUE=DATE:[0-9]{8})\nRRULE:[A-Z0-9=;,+-]+",
    Validator::RecurrenceRule,
    &[
        (
            "DTSTART;TZID=Europe/London:20261026T093000\nRRULE:FREQ=WEEKLY;BYDAY=MO",
            "DTSTART;TZID=Europe/London:20261026T093000\nRRULE:FREQ=WEEKLY;BYDAY=MO",
        ),
        (
            "DTSTART;TZID=America/New_York:20260310T090000\nRRULE:FREQ=MONTHLY;BYDAY=2TU;COUNT=12",
            "DTSTART;TZID=America/New_York:20260310T090000\nRRULE:FREQ=MONTHLY;BYDAY=2TU;COUNT=12",
        ),
        (
            "DTSTART;TZID=Europe/London:20260130T170000\nRRULE:FREQ=MONTHLY;BYDAY=-1FR;UNTIL=20261231T235959Z",
            "DTSTART;TZID=Europe/London:20260130T170000\nRRULE:FREQ=MONTHLY;BYDAY=-1FR;UNTIL=20261231T235959Z",
        ),
        (
            "DTSTART;VALUE=DATE:20240229\nRRULE:FREQ=YEARLY",
            "DTSTART;VALUE=DATE:20240229\nRRULE:FREQ=YEARLY",
        ),
    ],
    &[
        "FREQ=WEEKLY;BYDAY=MO",
        // 26 October 2026 is a Monday, which the rule doesn't give.
        "DTSTART;TZID=Europe/London:20261026T093000\nRRULE:FREQ=WEEKLY;BYDAY=TU",
        "DTSTART;TZID=Europe/London:20261026T093000\nRRULE:FREQ=HOURLY",
        "DTSTART:20261026T093000Z\nRRULE:FREQ=DAILY",
        "DTSTART;TZID=Europe/London:20261026T093000\nRRULE:FREQ=DAILY;COUNT=2;UNTIL=20261231T000000Z",
        // A clock time picked for a rule on dates.
        "DTSTART;VALUE=DATE:20240229\nRRULE:FREQ=YEARLY;BYHOUR=9",
        "",
    ],
);

/// When a place is open each week, and the days that differ, read in its
/// zone (OpenStreetMap's `opening_hours`).
pub const OPENING_HOURS: StandardType = schedule(
    super::OPENING_HOURS,
    "[A-Za-z0-9 :;,-]+ \\[[A-Za-z0-9/_+-]+\\]",
    Validator::OpeningHours,
    &[
        (
            "Mo-Fr 09:00-17:30; Sa 10:00-16:00 [Europe/London]",
            "Mo-Fr 09:00-17:30; Sa 10:00-16:00 [Europe/London]",
        ),
        (
            "Mo-Th 17:00-23:00; Fr,Sa 17:00-02:00; 2026 Dec 25 off [America/New_York]",
            "Mo-Th 17:00-23:00; Fr,Sa 17:00-02:00; 2026 Dec 25 off [America/New_York]",
        ),
    ],
    &[
        "Mo-Fr 09:00-17:30",
        "Mo-Fr 9:00-17:30 [Europe/London]",
        "Mo-Fr 09:00-25:00 [Europe/London]",
        "Monday 09:00-17:30 [Europe/London]",
        "",
    ],
);
