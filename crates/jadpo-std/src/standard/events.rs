//! `CalendarEvent` (area 19, "Time and calendar additions", chunk 5.5k3):
//! an event whose parts code reads, an all-day event's first and last days
//! beside a timed event's local start and end and its zone, with a summary,
//! a location, a description and a recurrence. Its canonical text is an RFC
//! 5545 `VEVENT`, written and read here so the checker and the runtime
//! share one reading, and the rules across its parts are here for the same
//! reason: a literal is proved by them while compiling, and a value read
//! from text or JSON is checked by them as it is read.
//!
//! The text is one property a line, joined by line feeds as a
//! `RecurrenceRule`'s is, in this order, the last three only when the event
//! has them:
//!
//! ```text
//! BEGIN:VEVENT
//! DTSTART;VALUE=DATE:20261224
//! DTEND;VALUE=DATE:20261227
//! SUMMARY:Christmas at home
//! LOCATION:Bath
//! DESCRIPTION:Bring the crackers\, and the cake
//! RRULE:FREQ=YEARLY
//! END:VEVENT
//! ```
//!
//! An all-day event's `DTEND` is the day after its last, as RFC 5545
//! writes it (3.6.1); a timed event's `DTSTART` and `DTEND` are local times
//! in one IANA zone, `DTSTART;TZID=Europe/London:20261024T220000`, as a
//! recurrence's start is. Text values escape `\`, `;`, `,` and a line feed
//! as RFC 5545 3.3.11 writes them.

use super::across::{Check, Given, Known, Order, Parts, StandardRule, Verdict, in_order};
use super::recurrence::{LocalAt, Rule};
use super::{
    Examples, FIRST_RELEASE, Format, FormatKey, Holds, Json, NO_OPTIONS, Part, RECURRENCE_RULE,
    StandardType, Storage, TextForm,
};
use crate::{Builtin, ZONES};

/// `CalendarEvent` (area 19, "Time and calendar additions").
pub const CALENDAR_EVENT: &str = "CalendarEvent";

/// Text for people that may run over several lines, a description: not
/// empty, and no control character but a tab or a line feed, which an RFC
/// 5545 text value can't hold.
pub const PARAGRAPH_PATTERN: &str = r"(?:[^\p{Cc}]|[\t\n])+";

/// `CalendarEvent`'s parts, in order: what it is called, its days or its
/// local times and zone, where it is, what it is about, and how it repeats.
pub const FIELDS: [(&str, Part); 9] = [
    ("summary", Part::Line),
    (
        "first_day",
        Part::Optional(&Part::Of(Builtin::CalendarDate)),
    ),
    ("last_day", Part::Optional(&Part::Of(Builtin::CalendarDate))),
    ("start", Part::Optional(&Part::Standard("LocalDateTime"))),
    ("end", Part::Optional(&Part::Standard("LocalDateTime"))),
    ("zone", Part::Optional(&Part::Standard(crate::ZONE))),
    ("location", Part::Optional(&Part::Line)),
    ("description", Part::Optional(&Part::Paragraph)),
    (
        "recurrence",
        Part::Optional(&Part::Standard(RECURRENCE_RULE)),
    ),
];

/// When an event happens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum When {
    /// All day, from its first day to its last, both included, as days
    /// since 1970-01-01.
    Days { first: i32, last: i32 },
    /// From a local time to a later one, both in one zone (its place among
    /// `Zone`'s variants).
    Timed {
        start: LocalAt,
        end: LocalAt,
        zone: u32,
    },
}

/// A `CalendarEvent` as its text holds it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
    pub summary: String,
    pub when: When,
    pub location: Option<String>,
    pub description: Option<String>,
    /// Its `RecurrenceRule`'s text: the event's own start and the rule.
    pub recurrence: Option<String>,
}

/// `CalendarEvent`'s rules across its parts (chunk 5.5t2), in order: its
/// days or its times, a last day on or after the first, an end after the
/// start, and a recurrence from the event's own start.
pub const RULES: &[StandardRule] = &[
    StandardRule {
        name: "days_or_times",
        written: "first_day and last_day with no start, end or zone, or start, end and zone with no days",
        label: "an event has its days or its times",
        reason: "A `CalendarEvent` is all-day, with a `first_day` and a `last_day`, or timed, with a `start`, an `end` and a `zone`; never parts of both.",
        next: "Give an all-day event both days and `none` for `start`, `end` and `zone`, or a timed one those three and `none` for both days",
        check: Check::By(days_or_times),
    },
    StandardRule {
        name: "last_day_not_before_first",
        written: "last_day >= first_day",
        label: "before the first day",
        reason: "An all-day event's `last_day` is the last day it covers, included, so it is on or after its `first_day`; a one-day event's two days are the same.",
        next: "Write a last day on or after the first",
        check: in_order(&[Order::at_most("first_day", "last_day")], "last_day"),
    },
    StandardRule {
        name: "end_after_start",
        written: "end > start",
        label: "not after the start",
        reason: "A timed event's `end` is later than its `start`, as RFC 5545 requires; one that runs past midnight ends on the next day's date.",
        next: "Write an end later than the start",
        check: in_order(&[Order::below("start", "end")], "end"),
    },
    StandardRule {
        name: "recurrence_from_start",
        written: "recurrence starts on first_day, or at start in zone",
        label: "doesn't start when the event does",
        reason: "An event's recurrence starts when the event does, a `VEVENT` writing one `DTSTART` for both: an all-day event's on its `first_day`, a date with no clock time or zone (`DTSTART;VALUE=DATE:`), a timed one's at its `start` in its `zone`.",
        next: "Write the recurrence's `DTSTART` as the event's first day, `DTSTART;VALUE=DATE:`, or as its start in its zone",
        check: Check::By(recurrence_from_start),
    },
];

const DAYS: [&str; 2] = ["first_day", "last_day"];
const TIMES: [&str; 3] = ["start", "end", "zone"];

/// An event has both days and no times, or all three times and no days.
fn days_or_times(parts: &dyn Parts) -> Verdict {
    let known = |names: &[&'static str]| -> Vec<(&'static str, Known<()>)> {
        names
            .iter()
            .map(|name| (*name, parts.part(name).map(|_| Some(()))))
            .collect()
    };
    let (days, times) = (known(&DAYS), known(&TIMES));
    let undecided = days.iter().chain(&times).any(|(_, k)| *k == Known::Unknown);
    let (first, second) = if days.iter().any(|(_, k)| k.given()) {
        (days, times)
    } else if times.iter().any(|(_, k)| k.given()) {
        (times, days)
    } else if undecided {
        return Verdict::Unknown;
    } else {
        // Nothing at all: neither days nor times.
        return Verdict::Breaks("first_day");
    };
    // The kind it is, all of its parts given; and none of the other.
    if let Some((name, _)) = first.iter().find(|(_, k)| k.absent()) {
        return Verdict::Breaks(name);
    }
    if let Some((name, _)) = second.iter().find(|(_, k)| k.given()) {
        return Verdict::Breaks(name);
    }
    if undecided {
        Verdict::Unknown
    } else {
        Verdict::Holds
    }
}

/// A recurrence starts when the event does: on an all-day event's first
/// day, as a date-only rule (chunk 5.5k4), or at a timed event's start in
/// its zone.
fn recurrence_from_start(parts: &dyn Parts) -> Verdict {
    let text = match parts.part("recurrence") {
        Known::Absent => return Verdict::Holds,
        Known::Given(Some(given)) => match given.text() {
            Some(text) => text,
            None => return Verdict::Unknown,
        },
        _ => return Verdict::Unknown,
    };
    let refused = |held: bool| {
        if held {
            Verdict::Holds
        } else {
            Verdict::Breaks("recurrence")
        }
    };
    match (parts.part("first_day"), parts.part("start")) {
        (Known::Given(Some(first)), _) => {
            let Some(first) = first.date() else {
                return Verdict::Unknown;
            };
            refused(
                Rule::read(text)
                    .is_some_and(|rule| rule.zone.is_none() && rule.start.date == first),
            )
        }
        (Known::Absent, Known::Given(Some(start))) => {
            let (Some(start), Some(zone)) = (
                start.local(),
                parts.part("zone").value().and_then(|zone| zone.ordinal()),
            ) else {
                return Verdict::Unknown;
            };
            refused(
                Rule::read(text).is_some_and(|rule| rule.start == start && rule.zone == Some(zone)),
            )
        }
        // Neither days nor a start: `days_or_times` refuses it.
        (Known::Absent, Known::Absent) => Verdict::Holds,
        _ => Verdict::Unknown,
    }
}

/// What the rules across an event's parts see of a whole event.
#[derive(Clone, Debug)]
pub struct EventParts<'a> {
    event: &'a Event,
}

impl Parts for EventParts<'_> {
    fn part(&self, name: &str) -> Known<Given<'_>> {
        let event = self.event;
        let given = |value| Known::Given(Some(value));
        match (name, event.when) {
            ("first_day", When::Days { first, .. }) => given(Given::Date(first)),
            ("last_day", When::Days { last, .. }) => given(Given::Date(last)),
            ("start", When::Timed { start, .. }) => given(Given::Local(start)),
            ("end", When::Timed { end, .. }) => given(Given::Local(end)),
            ("zone", When::Timed { zone, .. }) => given(Given::Variant {
                ordinal: zone,
                code: "",
            }),
            ("first_day" | "last_day", _) | ("start" | "end" | "zone", _) => Known::Absent,
            ("summary", _) => given(Given::Text(&event.summary)),
            ("location", _) => event
                .location
                .as_deref()
                .map_or(Known::Absent, |text| given(Given::Text(text))),
            ("description", _) => event
                .description
                .as_deref()
                .map_or(Known::Absent, |text| given(Given::Text(text))),
            ("recurrence", _) => event
                .recurrence
                .as_deref()
                .map_or(Known::Absent, |text| given(Given::Text(text))),
            _ => Known::Unknown,
        }
    }
}

impl Event {
    /// What the rules across its parts see of it.
    pub fn parts(&self) -> EventParts<'_> {
        EventParts { event: self }
    }

    /// Its canonical text; `None` when its last day is the last a
    /// `CalendarDate` holds, since the day after, its `DTEND`, isn't one.
    pub fn text(&self) -> Option<String> {
        let mut lines = vec!["BEGIN:VEVENT".to_string()];
        match self.when {
            When::Days { first, last } => {
                let after = last.checked_add(1).filter(|day| *day <= crate::DATE_MAX)?;
                lines.push(format!("DTSTART;VALUE=DATE:{}", basic_date(first)));
                lines.push(format!("DTEND;VALUE=DATE:{}", basic_date(after)));
            }
            When::Timed { start, end, zone } => {
                lines.push(zoned_line("DTSTART", start, zone)?);
                lines.push(zoned_line("DTEND", end, zone)?);
            }
        }
        lines.push(format!("SUMMARY:{}", escape(&self.summary)));
        if let Some(location) = &self.location {
            lines.push(format!("LOCATION:{}", escape(location)));
        }
        if let Some(description) = &self.description {
            lines.push(format!("DESCRIPTION:{}", escape(description)));
        }
        if let Some(recurrence) = &self.recurrence {
            lines.push(recurrence.split_once('\n')?.1.to_string());
        }
        lines.push("END:VEVENT".to_string());
        Some(lines.join("\n"))
    }

    /// The event `text` holds, or `None` when it isn't one's canonical
    /// text. The rules across its parts are its rules ([`RULES`]), which
    /// it is checked by once it is read.
    pub fn read(text: &str) -> Option<Event> {
        let mut lines = text
            .strip_prefix("BEGIN:VEVENT\n")?
            .strip_suffix("\nEND:VEVENT")?
            .split('\n')
            .peekable();
        let start_line = lines.next()?;
        let end_line = lines.next()?;
        let when = match (
            start_line.strip_prefix("DTSTART;VALUE=DATE:"),
            end_line.strip_prefix("DTEND;VALUE=DATE:"),
        ) {
            (Some(first), Some(after)) => {
                let (first, after) = (
                    super::recurrence::read_basic_date(first)?,
                    super::recurrence::read_basic_date(after)?,
                );
                When::Days {
                    first,
                    last: after.checked_sub(1)?,
                }
            }
            (None, None) => {
                let (zone, start) = read_zoned_line(start_line, "DTSTART")?;
                let (end_zone, end) = read_zoned_line(end_line, "DTEND")?;
                if end_zone != zone {
                    return None;
                }
                When::Timed { start, end, zone }
            }
            _ => return None,
        };
        let summary = read_text(lines.next()?.strip_prefix("SUMMARY:")?, Part::Line)?;
        let mut take = |name: &str, part: Part| -> Option<Option<String>> {
            match lines.peek().and_then(|line| line.strip_prefix(name)) {
                Some(value) => {
                    let read = read_text(value, part)?;
                    lines.next();
                    Some(Some(read))
                }
                None => Some(None),
            }
        };
        let location = take("LOCATION:", Part::Line)?;
        let description = take("DESCRIPTION:", Part::Paragraph)?;
        let recurrence = match lines.peek() {
            Some(line) if line.starts_with("RRULE:") => {
                let rule = format!("{start_line}\n{line}");
                lines.next();
                Some(rule)
            }
            _ => None,
        };
        if lines.next().is_some() {
            return None;
        }
        Some(Event {
            summary,
            when,
            location,
            description,
            recurrence,
        })
    }
}

/// A date in RFC 5545's basic form, `20261225`.
fn basic_date(day: i32) -> String {
    let (year, month, day) = crate::civil_date(day);
    format!("{year:04}{month:02}{day:02}")
}

/// `DTSTART;TZID=Europe/London:20261024T220000`.
fn zoned_line(name: &str, at: LocalAt, zone: u32) -> Option<String> {
    let (_, iana) = ZONES.get(usize::try_from(zone).ok()?)?;
    let second = at.second;
    Some(format!(
        "{name};TZID={iana}:{}T{:02}{:02}{:02}",
        basic_date(at.date),
        second / 3600,
        second / 60 % 60,
        second % 60
    ))
}

/// A zone and a local time from a line [`zoned_line`] writes.
fn read_zoned_line(line: &str, name: &str) -> Option<(u32, LocalAt)> {
    let (zone, at) = line
        .strip_prefix(name)?
        .strip_prefix(";TZID=")?
        .split_once(':')?;
    let zone = ZONES.iter().position(|(_, iana)| *iana == zone)?;
    let (at, utc) = super::recurrence::basic_date_time(at)?;
    if utc {
        return None;
    }
    Some((u32::try_from(zone).ok()?, at))
}

/// A text value as RFC 5545 writes one (3.3.11): `\`, `;` and `,` after a
/// `\`, and a line feed as `\n`.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            ';' => out.push_str("\\;"),
            ',' => out.push_str("\\,"),
            '\n' => out.push_str("\\n"),
            other => out.push(other),
        }
    }
    out
}

/// A text value read back from [`escape`]'s form, kept to `part`'s rules:
/// only that form is read, so a bare `;` or `,`, `\N` and any other escape
/// are refused.
fn read_text(raw: &str, part: Part) -> Option<String> {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next()? {
                '\\' => out.push('\\'),
                ';' => out.push(';'),
                ',' => out.push(','),
                'n' => out.push('\n'),
                _ => return None,
            },
            ';' | ',' => return None,
            other => out.push(other),
        }
    }
    part.rules().check_text(&out).is_ok().then_some(out)
}

/// The text of the examples below, one per line.
macro_rules! vevent {
    ($($line:literal),+ $(,)?) => {
        concat!("BEGIN:VEVENT", $("\n", $line,)+ "\nEND:VEVENT")
    };
}

/// Its examples: an all-day event of one day, one over the night the
/// clocks go back with a location and a description to escape, an
/// anniversary that repeats yearly on its day, and a timed event across
/// midnight that repeats weekly.
const VALID: &[(&str, &str)] = &[
    (
        vevent!(
            "DTSTART;VALUE=DATE:20261225",
            "DTEND;VALUE=DATE:20261226",
            "SUMMARY:Christmas Day",
        ),
        vevent!(
            "DTSTART;VALUE=DATE:20261225",
            "DTEND;VALUE=DATE:20261226",
            "SUMMARY:Christmas Day",
        ),
    ),
    (
        vevent!(
            "DTSTART;VALUE=DATE:20261024",
            "DTEND;VALUE=DATE:20261027",
            "SUMMARY:Half-term walk",
            "LOCATION:Keswick\\, Cumbria",
            "DESCRIPTION:Boots\\; maps\\nFlask",
        ),
        vevent!(
            "DTSTART;VALUE=DATE:20261024",
            "DTEND;VALUE=DATE:20261027",
            "SUMMARY:Half-term walk",
            "LOCATION:Keswick\\, Cumbria",
            "DESCRIPTION:Boots\\; maps\\nFlask",
        ),
    ),
    (
        vevent!(
            "DTSTART;VALUE=DATE:20240229",
            "DTEND;VALUE=DATE:20240301",
            "SUMMARY:Wedding anniversary",
            "RRULE:FREQ=YEARLY",
        ),
        vevent!(
            "DTSTART;VALUE=DATE:20240229",
            "DTEND;VALUE=DATE:20240301",
            "SUMMARY:Wedding anniversary",
            "RRULE:FREQ=YEARLY",
        ),
    ),
    (
        vevent!(
            "DTSTART;TZID=Europe/London:20261024T220000",
            "DTEND;TZID=Europe/London:20261025T020000",
            "SUMMARY:Night shift",
            "RRULE:FREQ=WEEKLY;COUNT=4",
        ),
        vevent!(
            "DTSTART;TZID=Europe/London:20261024T220000",
            "DTEND;TZID=Europe/London:20261025T020000",
            "SUMMARY:Night shift",
            "RRULE:FREQ=WEEKLY;COUNT=4",
        ),
    ),
];

/// Texts no event reads, each refused for one reason.
const MALFORMED: &[&str] = &[
    // An all-day end is exclusive, so a one-day event ends the next day.
    vevent!(
        "DTSTART;VALUE=DATE:20261225",
        "DTEND;VALUE=DATE:20261225",
        "SUMMARY:Christmas Day",
    ),
    // A date and a time together.
    vevent!(
        "DTSTART;VALUE=DATE:20261225",
        "DTEND;TZID=Europe/London:20261225T120000",
        "SUMMARY:Christmas Day",
    ),
    // One zone for both ends.
    vevent!(
        "DTSTART;TZID=Europe/London:20261024T220000",
        "DTEND;TZID=Europe/Paris:20261025T020000",
        "SUMMARY:Night shift",
    ),
    // An end before its start, and a time with no zone.
    vevent!(
        "DTSTART;TZID=Europe/London:20261025T020000",
        "DTEND;TZID=Europe/London:20261024T220000",
        "SUMMARY:Night shift",
    ),
    vevent!(
        "DTSTART:20261024T220000",
        "DTEND:20261025T020000",
        "SUMMARY:Night shift",
    ),
    // An all-day recurrence on days the event doesn't start on: 25
    // December 2026 is a Friday.
    vevent!(
        "DTSTART;VALUE=DATE:20261225",
        "DTEND;VALUE=DATE:20261226",
        "SUMMARY:Christmas Day",
        "RRULE:FREQ=WEEKLY;BYDAY=MO",
    ),
    // A recurrence at a time on an all-day event, and one on days on a
    // timed event.
    "BEGIN:VEVENT\nDTSTART;VALUE=DATE:20261225\nDTEND;VALUE=DATE:20261226\nSUMMARY:Christmas Day\nRRULE:FREQ=YEARLY;BYHOUR=9\nEND:VEVENT",
    // A recurrence whose first occurrence isn't the event's start.
    vevent!(
        "DTSTART;TZID=Europe/London:20261024T220000",
        "DTEND;TZID=Europe/London:20261025T020000",
        "SUMMARY:Night shift",
        "RRULE:FREQ=WEEKLY;BYDAY=MO",
    ),
    // A comma not escaped, an escape RFC 5545 writes in lower case, and
    // the parts out of order.
    vevent!(
        "DTSTART;VALUE=DATE:20261225",
        "DTEND;VALUE=DATE:20261226",
        "SUMMARY:Christmas, at home",
    ),
    vevent!(
        "DTSTART;VALUE=DATE:20261225",
        "DTEND;VALUE=DATE:20261226",
        "SUMMARY:Christmas Day",
        "DESCRIPTION:Boots\\Nmaps",
    ),
    vevent!(
        "DTSTART;VALUE=DATE:20261225",
        "DTEND;VALUE=DATE:20261226",
        "LOCATION:Bath",
        "SUMMARY:Christmas Day",
    ),
    // No summary, and a calendar file's carriage returns.
    vevent!("DTSTART;VALUE=DATE:20261225", "DTEND;VALUE=DATE:20261226"),
    "BEGIN:VEVENT\r\nDTSTART;VALUE=DATE:20261225\r\nDTEND;VALUE=DATE:20261226\r\nSUMMARY:Christmas Day\r\nEND:VEVENT",
    "",
];

/// A calendar event (RFC 5545's `VEVENT`): what it is called, when it is,
/// where, what it is about, and how it repeats.
pub const CALENDAR_EVENT_ENTRY: StandardType = StandardType {
    name: CALENDAR_EVENT,
    holds: Holds::Object(&FIELDS),
    text: Some(TextForm::Event),
    json: Json::Fields(&[
        "summary",
        "first_day",
        "last_day",
        "start",
        "end",
        "zone",
        "location",
        "description",
        "recurrence",
    ]),
    // A column per part: the days as `date`s, a local time as its
    // canonical text, the zone as its IANA name.
    storage: Storage::Columns {
        parts: &[
            ("summary", "text"),
            ("first_day", "date"),
            ("last_day", "date"),
            ("start", "text"),
            ("end", "text"),
            ("zone", "text"),
            ("location", "text"),
            ("description", "text"),
            ("recurrence", "text"),
        ],
        fixed: None,
    },
    // CLDR has no pattern for an event: an application shows the parts it
    // wants, each by its own format.
    format: Format {
        key: FormatKey::Canonical,
        options: NO_OPTIONS,
    },
    ordered: false,
    validator: None,
    shape: None,
    examples: Examples {
        valid: VALID,
        format: None,
        malformed: MALFORMED,
        of: None,
        settings: "",
    },
    release: FIRST_RELEASE,
    signals: crate::standard::NO_SIGNALS,
};

#[cfg(test)]
mod tests {
    use super::*;

    fn day(text: &str) -> i32 {
        crate::date_number(text).unwrap()
    }

    #[test]
    fn each_example_reads_and_is_written_back() {
        for (text, _) in VALID {
            let event = Event::read(text).unwrap_or_else(|| panic!("`{text}` reads"));
            assert_eq!(event.text().as_deref(), Some(*text));
        }
        // Text that isn't an event, or an event that breaks one of its
        // rules, which it is checked by once it is read.
        for text in MALFORMED {
            assert!(
                Event::read(text)
                    .is_none_or(|event| RULES.iter().any(|rule| !rule.holds(&event.parts()))),
                "`{text}`"
            );
        }
    }

    #[test]
    fn an_all_day_end_is_the_day_after_the_last() {
        let event = Event::read(VALID[1].0).unwrap();
        assert_eq!(
            event.when,
            When::Days {
                first: day("2026-10-24"),
                last: day("2026-10-26"),
            }
        );
        assert_eq!(event.location.as_deref(), Some("Keswick, Cumbria"));
        assert_eq!(event.description.as_deref(), Some("Boots; maps\nFlask"));
        // The last day a date holds has no day after it to write.
        let last = Event {
            summary: "The end".to_string(),
            when: When::Days {
                first: crate::DATE_MAX,
                last: crate::DATE_MAX,
            },
            location: None,
            description: None,
            recurrence: None,
        };
        assert_eq!(last.text(), None);
    }

    /// The first rule `parts` break, by name, with the part pointed at.
    fn broken(
        parts: &[(&'static str, Known<Given<'static>>)],
    ) -> Option<(&'static str, &'static str)> {
        let listed = super::super::across::Listed {
            parts: parts.to_vec(),
        };
        RULES.iter().find_map(|rule| match rule.verdict(&listed) {
            Verdict::Breaks(at) => Some((rule.name, at)),
            _ => None,
        })
    }

    #[test]
    fn rules_across_parts_refuse_only_what_is_known() {
        let all = |known: Known<Given<'static>>| -> Vec<(&'static str, Known<Given<'static>>)> {
            [
                "first_day",
                "last_day",
                "start",
                "end",
                "zone",
                "recurrence",
            ]
            .into_iter()
            .map(|name| (name, known))
            .collect()
        };
        let with = |base: &[(&'static str, Known<Given<'static>>)],
                    changes: &[(&'static str, Known<Given<'static>>)]| {
            base.iter()
                .map(|(name, known)| {
                    changes
                        .iter()
                        .find(|(changed, _)| changed == name)
                        .map_or((*name, *known), |change| *change)
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(broken(&all(Known::Unknown)), None);
        let nothing = all(Known::Absent);
        assert_eq!(broken(&nothing), Some(("days_or_times", "first_day")));
        let half = with(&nothing, &[("first_day", Known::Given(None))]);
        assert_eq!(broken(&half), Some(("days_or_times", "last_day")));
        let both = with(
            &nothing,
            &[
                ("first_day", Known::Given(None)),
                ("last_day", Known::Given(None)),
                ("zone", Known::Given(None)),
            ],
        );
        assert_eq!(broken(&both), Some(("days_or_times", "zone")));
        let timed = with(
            &nothing,
            &[
                ("start", Known::Given(None)),
                ("end", Known::Unknown),
                ("zone", Known::Given(None)),
            ],
        );
        assert_eq!(broken(&timed), None);
        let date = |text| Known::Given(Some(Given::Date(day(text))));
        let backwards = with(
            &nothing,
            &[
                ("first_day", date("2026-12-26")),
                ("last_day", date("2026-12-25")),
            ],
        );
        assert_eq!(
            broken(&backwards),
            Some(("last_day_not_before_first", "last_day"))
        );
        // An all-day event recurs on days from its first (chunk 5.5k4),
        // never at a time; a timed one at its start, never on days.
        let christmas = with(
            &nothing,
            &[
                ("first_day", date("2026-12-25")),
                ("last_day", date("2026-12-25")),
            ],
        );
        let yearly = |text: &'static str| {
            with(
                &christmas,
                &[("recurrence", Known::Given(Some(Given::Text(text))))],
            )
        };
        assert_eq!(
            broken(&yearly("DTSTART;VALUE=DATE:20261225\nRRULE:FREQ=YEARLY")),
            None
        );
        for text in [
            "DTSTART;VALUE=DATE:20251225\nRRULE:FREQ=YEARLY",
            "DTSTART;TZID=Europe/London:20261225T000000\nRRULE:FREQ=YEARLY",
        ] {
            assert_eq!(
                broken(&yearly(text)),
                Some(("recurrence_from_start", "recurrence")),
                "`{text}`"
            );
        }
        let shift = with(
            &nothing,
            &[
                (
                    "start",
                    Known::Given(Some(Given::Local(LocalAt {
                        date: day("2026-12-25"),
                        second: 0,
                    }))),
                ),
                ("end", Known::Unknown),
                (
                    "zone",
                    Known::Given(Some(Given::Variant {
                        ordinal: 0,
                        code: "",
                    })),
                ),
                (
                    "recurrence",
                    Known::Given(Some(Given::Text(
                        "DTSTART;VALUE=DATE:20261225\nRRULE:FREQ=YEARLY",
                    ))),
                ),
            ],
        );
        assert_eq!(
            broken(&shift),
            Some(("recurrence_from_start", "recurrence"))
        );
    }
}
