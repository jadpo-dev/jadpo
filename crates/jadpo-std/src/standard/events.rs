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

/// What a rule across an event's parts knows of one of them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Known<T> {
    /// It is `none`.
    Absent,
    /// It is given, with its value where that is known.
    Given(Option<T>),
    /// It may or may not be `none`: a value built in code.
    Unknown,
}

impl<T> Known<T> {
    fn given(&self) -> bool {
        matches!(self, Known::Given(_))
    }

    fn absent(&self) -> bool {
        matches!(self, Known::Absent)
    }

    fn value(&self) -> Option<&T> {
        match self {
            Known::Given(value) => value.as_ref(),
            _ => None,
        }
    }
}

/// An event's parts the rules across them read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Parts<'a> {
    pub first_day: Known<i32>,
    pub last_day: Known<i32>,
    pub start: Known<LocalAt>,
    pub end: Known<LocalAt>,
    pub zone: Known<u32>,
    pub recurrence: Known<&'a str>,
}

/// A rule across an event's parts that they break: the part it points at,
/// and what that part must be.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Broken {
    pub part: &'static str,
    pub says: &'static str,
}

const DAYS_OR_TIMES: &str = "an event's days, or its start, end and zone: an all-day event has a first and a last day, a timed one a start, an end and a zone, and none has both";

/// The first rule across an event's parts that `parts` breaks, by what is
/// known of them; a part whose value isn't known breaks nothing.
pub fn broken(parts: &Parts<'_>) -> Option<Broken> {
    let refuse = |part, says| Some(Broken { part, says });
    if parts.first_day.given() || parts.last_day.given() {
        // All day.
        for (part, known) in [("first_day", parts.first_day), ("last_day", parts.last_day)] {
            if known.absent() {
                return refuse(part, DAYS_OR_TIMES);
            }
        }
        for (part, given) in [
            ("start", parts.start.given()),
            ("end", parts.end.given()),
            ("zone", parts.zone.given()),
        ] {
            if given {
                return refuse(part, DAYS_OR_TIMES);
            }
        }
        if let (Some(first), Some(last)) = (parts.first_day.value(), parts.last_day.value())
            && last < first
        {
            return refuse("last_day", "an event's last day, on or after its first");
        }
        if parts.recurrence.given() {
            return refuse(
                "recurrence",
                "a timed event's recurrence: a recurrence starts at a local time in a zone, which an all-day event doesn't have",
            );
        }
        return None;
    }
    let timed = [
        ("start", parts.start.given(), parts.start.absent()),
        ("end", parts.end.given(), parts.end.absent()),
        ("zone", parts.zone.given(), parts.zone.absent()),
    ];
    if timed.iter().any(|(_, given, _)| *given) {
        if let Some((part, _, _)) = timed.iter().find(|(_, _, absent)| *absent) {
            return refuse(part, DAYS_OR_TIMES);
        }
    } else if parts.first_day.absent()
        && parts.last_day.absent()
        && timed.iter().all(|(_, _, absent)| *absent)
    {
        return refuse("first_day", DAYS_OR_TIMES);
    }
    if let (Some(start), Some(end)) = (parts.start.value(), parts.end.value())
        && end <= start
    {
        return refuse("end", "a timed event's end, later than its start");
    }
    if let (Some(text), Some(start), Some(zone)) = (
        parts.recurrence.value(),
        parts.start.value(),
        parts.zone.value(),
    ) && Rule::read(text).is_none_or(|rule| rule.start != *start || rule.zone != *zone)
    {
        return refuse(
            "recurrence",
            "a recurrence that starts when the event does, at its start in its zone",
        );
    }
    None
}

impl Event {
    /// What the rules across its parts see of a whole event.
    pub fn parts(&self) -> Parts<'_> {
        let (first_day, last_day, start, end, zone) = match self.when {
            When::Days { first, last } => (
                Known::Given(Some(first)),
                Known::Given(Some(last)),
                Known::Absent,
                Known::Absent,
                Known::Absent,
            ),
            When::Timed { start, end, zone } => (
                Known::Absent,
                Known::Absent,
                Known::Given(Some(start)),
                Known::Given(Some(end)),
                Known::Given(Some(zone)),
            ),
        };
        Parts {
            first_day,
            last_day,
            start,
            end,
            zone,
            recurrence: match &self.recurrence {
                Some(text) => Known::Given(Some(text.as_str())),
                None => Known::Absent,
            },
        }
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
    /// text or its parts break a rule across them.
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
                let (first, after) = (read_basic_date(first)?, read_basic_date(after)?);
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
        let event = Event {
            summary,
            when,
            location,
            description,
            recurrence,
        };
        broken(&event.parts()).is_none().then_some(event)
    }
}

/// A date in RFC 5545's basic form, `20261225`.
fn basic_date(day: i32) -> String {
    let (year, month, day) = crate::civil_date(day);
    format!("{year:04}{month:02}{day:02}")
}

/// A date read from RFC 5545's basic form.
fn read_basic_date(text: &str) -> Option<i32> {
    if text.len() != 8 || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let year: i64 = text.get(0..4)?.parse().ok()?;
    let month: i64 = text.get(4..6)?.parse().ok()?;
    let day: i64 = text.get(6..8)?.parse().ok()?;
    if !(1..=12).contains(&month) || !(1..=crate::days_in_month(year, month)).contains(&day) {
        return None;
    }
    crate::epoch_day(year, month, day)
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
/// clocks go back with a location and a description to escape, and a
/// timed event across midnight that repeats weekly.
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
            "DESCRIPTION:Boots\\; maps\\nand a flask",
        ),
        vevent!(
            "DTSTART;VALUE=DATE:20261024",
            "DTEND;VALUE=DATE:20261027",
            "SUMMARY:Half-term walk",
            "LOCATION:Keswick\\, Cumbria",
            "DESCRIPTION:Boots\\; maps\\nand a flask",
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
    // An all-day event doesn't recur yet.
    vevent!(
        "DTSTART;VALUE=DATE:20261225",
        "DTEND;VALUE=DATE:20261226",
        "SUMMARY:Christmas Day",
        "RRULE:FREQ=YEARLY",
    ),
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
        "DESCRIPTION:Crackers\\Nand cake",
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
        for text in MALFORMED {
            assert_eq!(Event::read(text), None, "`{text}`");
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
        assert_eq!(
            event.description.as_deref(),
            Some("Boots; maps\nand a flask")
        );
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

    #[test]
    fn rules_across_parts_refuse_only_what_is_known() {
        let unknown = Parts {
            first_day: Known::Unknown,
            last_day: Known::Unknown,
            start: Known::Unknown,
            end: Known::Unknown,
            zone: Known::Unknown,
            recurrence: Known::Unknown,
        };
        assert_eq!(broken(&unknown), None);
        let nothing = Parts {
            first_day: Known::Absent,
            last_day: Known::Absent,
            start: Known::Absent,
            end: Known::Absent,
            zone: Known::Absent,
            recurrence: Known::Absent,
        };
        assert_eq!(broken(&nothing).map(|b| b.part), Some("first_day"));
        let half = Parts {
            first_day: Known::Given(None),
            ..nothing
        };
        assert_eq!(broken(&half).map(|b| b.part), Some("last_day"));
        let both = Parts {
            first_day: Known::Given(None),
            last_day: Known::Given(None),
            zone: Known::Given(None),
            ..nothing
        };
        assert_eq!(broken(&both).map(|b| b.part), Some("zone"));
        let timed = Parts {
            start: Known::Given(None),
            end: Known::Unknown,
            zone: Known::Given(None),
            ..nothing
        };
        assert_eq!(broken(&timed), None);
        let backwards = Parts {
            first_day: Known::Given(Some(day("2026-12-26"))),
            last_day: Known::Given(Some(day("2026-12-25"))),
            ..nothing
        };
        assert_eq!(broken(&backwards).map(|b| b.part), Some("last_day"));
    }
}
