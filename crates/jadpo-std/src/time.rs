//! Time values as source writes them (area 10): an `Instant` or a
//! `CalendarDate` is an ISO 8601 string, checked while compiling, and a
//! `Duration` is a number with a unit, `500ms`, `30s`, `5m`, `2h` or `30d`,
//! a day being exactly 24 hours.

use std::cmp::Ordering;
use std::fmt;

/// A length of time, to the millisecond.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Dur {
    pub millis: i128,
}

/// Each unit with its length in milliseconds, longest first.
const UNITS: [(&str, i128); 5] = [
    ("d", 86_400_000),
    ("h", 3_600_000),
    ("m", 60_000),
    ("s", 1_000),
    ("ms", 1),
];

impl Dur {
    /// Reads `500ms`, `30s`, `5m`, `2h` or `30d`; `None` for anything else.
    pub fn parse(text: &str) -> Option<Dur> {
        let digits = text.find(|c: char| !c.is_ascii_digit() && c != '_')?;
        let (number, unit) = text.split_at(digits);
        if number.is_empty() || number.starts_with('_') {
            return None;
        }
        let count: i128 = number.replace('_', "").parse().ok()?;
        let (_, size) = UNITS.iter().find(|(name, _)| *name == unit)?;
        Some(Dur {
            millis: count.checked_mul(*size)?,
        })
    }

    pub fn compare(self, other: Dur) -> Ordering {
        self.millis.cmp(&other.millis)
    }
}

impl fmt::Display for Dur {
    /// In the longest unit that holds it exactly: `90m`, `2h`, `1500ms`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.millis == 0 {
            return f.write_str("0s");
        }
        let (unit, size) = UNITS
            .iter()
            .find(|(_, size)| self.millis % size == 0)
            .copied()
            .unwrap_or(("ms", 1));
        write!(f, "{}{unit}", self.millis / size)
    }
}

/// Why `text` isn't an `Instant`, completing "The value ...": ISO 8601 in
/// UTC or with an offset, to the millisecond, as `2026-10-25T08:30:00Z`.
pub fn check_instant(text: &str) -> Result<(), String> {
    const FORM: &str = "isn't an instant, written `2026-10-25T08:30:00Z` with a `Z` or an offset";
    let Some((date, time)) = text.split_once('T') else {
        return Err(FORM.into());
    };
    check_calendar_date(date).map_err(|_| String::from(FORM))?;
    let (clock, offset) = match time.strip_suffix('Z') {
        Some(clock) => (clock, None),
        None => {
            let at = time.rfind(['+', '-']).ok_or_else(|| String::from(FORM))?;
            (&time[..at], Some(&time[at + 1..]))
        }
    };
    if let Some(offset) = offset
        && !hours_minutes(offset, 23)
    {
        return Err(FORM.into());
    }
    let (whole, fraction) = clock.split_once('.').unwrap_or((clock, ""));
    let parts: Vec<&str> = whole.split(':').collect();
    let [hour, minute, second] = parts.as_slice() else {
        return Err(FORM.into());
    };
    let field =
        |text: &str, max: u32| text.len() == 2 && text.parse::<u32>().is_ok_and(|n| n <= max);
    if !(field(hour, 23) && field(minute, 59) && field(second, 59)) {
        return Err(FORM.into());
    }
    if clock.contains('.') && (fraction.is_empty() || !fraction.chars().all(|c| c.is_ascii_digit()))
    {
        return Err(FORM.into());
    }
    if fraction.len() > 3 {
        return Err("is finer than a millisecond, the precision an `Instant` keeps".into());
    }
    Ok(())
}

/// `HH:MM` with the hour at most `max_hour`.
fn hours_minutes(text: &str, max_hour: u32) -> bool {
    let Some((hour, minute)) = text.split_once(':') else {
        return false;
    };
    hour.len() == 2
        && minute.len() == 2
        && hour.parse::<u32>().is_ok_and(|h| h <= max_hour)
        && minute.parse::<u32>().is_ok_and(|m| m <= 59)
}

/// Why `text` isn't a `CalendarDate`, completing "The value ...": a real
/// date written `2026-10-25`.
pub fn check_calendar_date(text: &str) -> Result<(), String> {
    const FORM: &str = "isn't a date, written `2026-10-25`";
    let parts: Vec<&str> = text.split('-').collect();
    let [year, month, day] = parts.as_slice() else {
        return Err(FORM.into());
    };
    let digits = |t: &str, n: usize| t.len() == n && t.chars().all(|c| c.is_ascii_digit());
    if !(digits(year, 4) && digits(month, 2) && digits(day, 2)) {
        return Err(FORM.into());
    }
    let (Ok(year), Ok(month), Ok(day)) = (
        year.parse::<u32>(),
        month.parse::<u32>(),
        day.parse::<u32>(),
    ) else {
        return Err(FORM.into());
    };
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let length = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return Err("has no such month".into()),
    };
    if day == 0 || day > length {
        return Err("has no such day in its month".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_read_in_their_units_and_print_in_the_longest() {
        assert_eq!(Dur::parse("2h").map(|d| d.millis), Some(7_200_000));
        assert_eq!(
            Dur::parse("500ms").map(|d| d.to_string()).as_deref(),
            Some("500ms")
        );
        assert_eq!(
            Dur::parse("120m").map(|d| d.to_string()).as_deref(),
            Some("2h")
        );
        assert_eq!(Dur::parse("5mb"), None);
        assert_eq!(Dur::parse("banana"), None);
        assert_eq!(Dur::parse("PT5M"), None);
        assert!(
            Dur::parse("1h").unwrap_or(Dur { millis: 0 })
                < Dur::parse("2h").unwrap_or(Dur { millis: 0 })
        );
    }

    #[test]
    fn instants_and_dates_are_iso_8601() {
        assert!(check_instant("2026-10-25T08:30:00Z").is_ok());
        assert!(check_instant("2026-10-25T08:30:00.123Z").is_ok());
        assert!(check_instant("2026-10-25T08:30:00+01:00").is_ok());
        assert!(check_instant("2026-10-25T08:30:00.123456Z").is_err());
        assert!(check_instant("not a time").is_err());
        assert!(check_instant("2026-10-25T24:00:00Z").is_err());
        assert!(check_instant("2026-10-25T08:30:00").is_err());
        assert!(check_calendar_date("2026-10-25").is_ok());
        assert!(check_calendar_date("2024-02-29").is_ok());
        assert!(check_calendar_date("2026-02-29").is_err());
        assert!(check_calendar_date("2026-13-45").is_err());
        assert!(check_calendar_date("26-10-25").is_err());
    }
}
