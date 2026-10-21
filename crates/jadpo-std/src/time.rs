//! Time values as source writes them (area 10): an `Instant` or a
//! `CalendarDate` is an ISO 8601 string, checked while compiling, and a
//! `Duration` is a number with a unit, `500ms`, `30s`, `5m`, `2h` or `30d`,
//! a day being exactly 24 hours.

use std::cmp::Ordering;
use std::fmt;

/// A length of time, to the millisecond.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub struct Dur {
    pub millis: i128,
}

/// Each unit with its length in milliseconds and what it means, longest
/// first.
const UNITS: [(&str, i128, &str); 5] = [
    ("d", 86_400_000, "days"),
    ("h", 3_600_000, "hours"),
    ("m", 60_000, "minutes"),
    ("s", 1_000, "seconds"),
    ("ms", 1, "milliseconds"),
];

/// A unit a number may be written with, `30s`, `2h` or `5km`: the
/// built-in type the number is then a value of, the suffix, and what it
/// means. `Duration`'s units and every quantity's suffixed units (area
/// 19, "Physical quantities"); the editor offers them after a number,
/// narrowed to the type expected there (area 14, "Completion"), so a type
/// given units here is offered with no editor code of its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitSuffix {
    pub of: crate::Builtin,
    pub suffix: &'static str,
    pub name: &'static str,
}

/// Every unit a number may be written with: the durations longest first,
/// then each quantity's suffixed units in its enum's order.
pub fn unit_suffixes() -> Vec<UnitSuffix> {
    let durations = UNITS.iter().map(|(suffix, _, name)| UnitSuffix {
        of: crate::Builtin::Duration,
        suffix,
        name,
    });
    let quantities = crate::quantity::UNITS
        .iter()
        .filter(|unit| unit.suffix)
        .map(|unit| UnitSuffix {
            of: crate::Builtin::Quantity(unit.kind),
            suffix: unit.code,
            name: unit.meaning,
        });
    durations.chain(quantities).collect()
}

impl Dur {
    /// Reads `500ms`, `30s`, `5m`, `2h` or `30d`; `None` for anything else.
    pub fn parse(text: &str) -> Option<Dur> {
        let digits = text.find(|c: char| !c.is_ascii_digit() && c != '_')?;
        let (number, unit) = text.split_at(digits);
        if number.is_empty() || number.starts_with('_') {
            return None;
        }
        let count: i128 = number.replace('_', "").parse().ok()?;
        let (_, size, _) = UNITS.iter().find(|(name, _, _)| *name == unit)?;
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
            .find(|(_, size, _)| self.millis % size == 0)
            .map(|(unit, size, _)| (*unit, *size))
            .unwrap_or(("ms", 1));
        write!(f, "{}{unit}", self.millis / size)
    }
}

/// The first instant an `Instant` holds: 0000-01-01T00:00:00Z.
pub const INSTANT_MIN: i64 = -62_167_219_200_000;

/// The last instant an `Instant` holds, 9999-12-30T22:00:00.999Z: the last
/// whose local time is a date of year 9999 or before in every zone.
pub const INSTANT_MAX: i64 = 253_402_207_200_999;

/// The first date a `CalendarDate` holds, 0000-01-01, in days since
/// 1970-01-01.
pub const DATE_MIN: i32 = -719_528;

/// The last date a `CalendarDate` holds, 9999-12-31.
pub const DATE_MAX: i32 = 2_932_896;

/// Milliseconds as the wire writes a duration (area 10): ISO 8601 in
/// hours, minutes and seconds, `PT5M`, `PT1.5S`, `PT720H`, `-PT5M`, `PT0S`.
pub fn iso_duration(millis: i64) -> String {
    use std::fmt::Write;
    let sign = if millis < 0 { "-" } else { "" };
    let mut rest = millis.unsigned_abs();
    let hours = rest / 3_600_000;
    rest %= 3_600_000;
    let minutes = rest / 60_000;
    rest %= 60_000;
    let seconds = rest / 1_000;
    let ms = rest % 1_000;
    let mut out = format!("{sign}PT");
    if hours > 0 {
        let _ = write!(out, "{hours}H");
    }
    if minutes > 0 {
        let _ = write!(out, "{minutes}M");
    }
    if seconds > 0 || ms > 0 || (hours == 0 && minutes == 0) {
        if ms > 0 {
            let fraction = format!("{ms:03}");
            let _ = write!(out, "{seconds}.{}S", fraction.trim_end_matches('0'));
        } else {
            let _ = write!(out, "{seconds}S");
        }
    }
    out
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
    if instant_value(text).is_none_or(|millis| !(INSTANT_MIN..=INSTANT_MAX).contains(&millis)) {
        return Err("is past 9999-12-30T22:00:00.999Z, the last instant an `Instant` holds".into());
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

/// Days since 1970-01-01 of a date in the proleptic Gregorian calendar
/// (Howard Hinnant's `epoch_day_from_civil`).
fn epoch_day_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month_from_march = (month + 9) % 12;
    let day_of_year = (153 * month_from_march + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// The date `epoch_day` days after 1970-01-01, as year, month and day.
fn civil_from_epoch_day(epoch_day: i64) -> (i64, i64, i64) {
    let shifted = epoch_day + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_from_march = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_from_march + 2) / 5 + 1;
    let month = if month_from_march < 10 {
        month_from_march + 3
    } else {
        month_from_march - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// The year, month and day of the date `epoch_day` days after 1970-01-01.
pub fn civil_date(epoch_day: i32) -> (i64, i64, i64) {
    civil_from_epoch_day(i64::from(epoch_day))
}

/// Days since 1970-01-01 of a year, month and day, which may be past the
/// end of the month (`2026-02-31` is `2026-03-03`), or `None` outside the
/// years a `CalendarDate` holds.
pub fn epoch_day(year: i64, month: i64, day: i64) -> Option<i32> {
    i32::try_from(epoch_day_from_civil(year, month, day))
        .ok()
        .filter(|day| (DATE_MIN..=DATE_MAX).contains(day))
}

/// How many weeks ISO 8601's week-numbering year `year` has: 53 when it
/// starts on a Thursday, or on a Wednesday in a leap year, otherwise 52;
/// `None` outside the years a `CalendarDate` holds.
pub fn iso_weeks_in(year: i64) -> Option<i64> {
    let first = epoch_day(year, 1, 1)?;
    // 1970-01-01 was a Thursday; Monday is 0.
    let weekday = (i64::from(first) + 3).rem_euclid(7);
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    Some(if weekday == 3 || (leap && weekday == 2) {
        53
    } else {
        52
    })
}

/// How many days month `month` of `year` has.
pub fn days_in_month(year: i64, month: i64) -> i64 {
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    match month {
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => 31,
    }
}

/// A `CalendarDate` as the runtime holds it: days since 1970-01-01; `None`
/// when `text` isn't one ([`check_calendar_date`] says why).
pub fn date_number(text: &str) -> Option<i32> {
    check_calendar_date(text).ok()?;
    let mut parts = text.split('-').map(|part| part.parse::<i64>().ok());
    let (Some(Some(year)), Some(Some(month)), Some(Some(day))) =
        (parts.next(), parts.next(), parts.next())
    else {
        return None;
    };
    i32::try_from(epoch_day_from_civil(year, month, day)).ok()
}

/// An `Instant` as the runtime holds it: milliseconds since
/// 1970-01-01T00:00:00Z, its offset applied; `None` when `text` isn't one
/// ([`check_instant`] says why).
pub fn instant_millis(text: &str) -> Option<i64> {
    check_instant(text).ok()?;
    instant_value(text)
}

/// The milliseconds of text already in an instant's form.
fn instant_value(text: &str) -> Option<i64> {
    let (date, time) = text.split_once('T')?;
    check_calendar_date(date).ok()?;
    let epoch_day = i64::from(date_number(date)?);
    let (clock, offset) = match time.strip_suffix('Z') {
        Some(clock) => (clock, 0),
        None => {
            let at = time.rfind(['+', '-'])?;
            let (hours, minutes) = time[at + 1..].split_once(':')?;
            let minutes = hours.parse::<i64>().ok()? * 60 + minutes.parse::<i64>().ok()?;
            let sign = if time[at..].starts_with('-') { -1 } else { 1 };
            (&time[..at], sign * minutes * 60_000)
        }
    };
    let (whole, fraction) = clock.split_once('.').unwrap_or((clock, ""));
    let mut fields = whole.split(':').map(|part| part.parse::<i64>().ok());
    let (Some(Some(hour)), Some(Some(minute)), Some(Some(second))) =
        (fields.next(), fields.next(), fields.next())
    else {
        return None;
    };
    let millis = if fraction.is_empty() {
        0
    } else {
        format!("{fraction:0<3}").parse::<i64>().ok()?
    };
    let clock = ((hour * 60 + minute) * 60 + second) * 1000 + millis;
    Some(epoch_day * 86_400_000 + clock - offset)
}

/// A clock time as `temporal.resolve`'s `at:` writes it (area 10, "Zones
/// and local times"): milliseconds since midnight, from `HH:MM`,
/// `HH:MM:SS` or `HH:MM:SS.mmm`, each field two digits.
pub fn clock_millis(text: &str) -> Option<u32> {
    let (whole, fraction) = text.split_once('.').unwrap_or((text, ""));
    let parts: Vec<&str> = whole.split(':').collect();
    let field = |text: &str, max: u32| {
        (text.len() == 2)
            .then(|| text.parse::<u32>().ok())
            .flatten()
            .filter(|n| *n <= max)
    };
    let (hour, minute, second) = match parts.as_slice() {
        [hour, minute] if fraction.is_empty() => (field(hour, 23)?, field(minute, 59)?, 0),
        [hour, minute, second] => (field(hour, 23)?, field(minute, 59)?, field(second, 59)?),
        _ => return None,
    };
    if fraction.len() > 3 || !fraction.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let millis = if fraction.is_empty() {
        0
    } else {
        format!("{fraction:0<3}").parse::<u32>().ok()?
    };
    Some(((hour * 60 + minute) * 60 + second) * 1000 + millis)
}

/// `2026-10-25`.
pub fn format_date(epoch_day: i32) -> String {
    let (year, month, day) = civil_from_epoch_day(i64::from(epoch_day));
    format!("{year:04}-{month:02}-{day:02}")
}

/// `2026-10-25T08:30:00Z`, with milliseconds only when it has some.
pub fn format_instant(millis: i64) -> String {
    let epoch_day = millis.div_euclid(86_400_000);
    let clock = millis.rem_euclid(86_400_000);
    let (year, month, day) = civil_from_epoch_day(epoch_day);
    format!(
        "{year:04}-{month:02}-{day:02}T{}Z",
        format_clock(u32::try_from(clock).unwrap_or(0), true)
    )
}

/// A clock time, `08:30` or `08:30:15.250`: seconds only when there are
/// some.
pub fn format_clock_time(millis: u32) -> String {
    format_clock(millis, false)
}

fn format_clock(millis: u32, seconds: bool) -> String {
    let (hour, minute) = (millis / 3_600_000, millis / 60_000 % 60);
    let (second, rest) = (millis / 1000 % 60, millis % 1000);
    let mut out = format!("{hour:02}:{minute:02}");
    if seconds || second != 0 || rest != 0 {
        out.push_str(&format!(":{second:02}"));
    }
    if rest != 0 {
        out.push_str(&format!(".{rest:03}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_values_read_as_numbers_and_print_back() {
        assert_eq!(date_number("1970-01-01"), Some(0));
        assert_eq!(
            date_number("2024-02-29").map(format_date).as_deref(),
            Some("2024-02-29")
        );
        assert_eq!(date_number("1969-12-31"), Some(-1));
        assert_eq!(date_number("2026-02-29"), None);
        assert_eq!(instant_millis("1970-01-01T00:00:01Z"), Some(1000));
        assert_eq!(
            instant_millis("2026-10-25T09:30:00+01:00"),
            instant_millis("2026-10-25T08:30:00Z")
        );
        assert_eq!(
            instant_millis("2026-10-25T08:30:00.5Z")
                .map(format_instant)
                .as_deref(),
            Some("2026-10-25T08:30:00.500Z")
        );
        assert_eq!(
            instant_millis("2026-10-25T08:30:00Z")
                .map(format_instant)
                .as_deref(),
            Some("2026-10-25T08:30:00Z")
        );
        assert_eq!(clock_millis("09:00"), Some(32_400_000));
        assert_eq!(
            clock_millis("09:00").map(format_clock_time).as_deref(),
            Some("09:00")
        );
        assert_eq!(clock_millis("24:00"), None);
    }

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
        assert!(check_instant("9999-12-31T00:00:00Z").is_err());
        assert_eq!(instant_millis("0000-01-01T00:00:00Z"), Some(INSTANT_MIN));
        assert_eq!(
            instant_millis("9999-12-30T22:00:00.999Z"),
            Some(INSTANT_MAX)
        );
        assert_eq!(date_number("0000-01-01"), Some(DATE_MIN));
        assert_eq!(date_number("9999-12-31"), Some(DATE_MAX));
        assert!(check_calendar_date("2026-10-25").is_ok());
        assert!(check_calendar_date("2024-02-29").is_ok());
        assert!(check_calendar_date("2026-02-29").is_err());
        assert!(check_calendar_date("2026-13-45").is_err());
        assert!(check_calendar_date("26-10-25").is_err());
    }
}
