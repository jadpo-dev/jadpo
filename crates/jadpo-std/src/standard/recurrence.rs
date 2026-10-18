//! `RecurrenceRule` (area 19, "Time and calendar additions", chunk 5.5k2):
//! an RFC 5545 recurrence, its first occurrence and its rule, read from
//! the text calendars exchange, and the local dates and clock times it
//! gives. The rule is held as that text; this module reads it and expands
//! it in local time, with no zone, so the checker and the runtime read a
//! rule with the same code. `jadpo-value` turns each local time into an
//! instant in the rule's zone, where daylight saving decides which exist.
//!
//! The text is two lines joined by a line feed, as `rrule.js` and
//! `python-dateutil` write and read a recurrence:
//!
//! ```text
//! DTSTART;TZID=Europe/London:20261026T093000
//! RRULE:FREQ=WEEKLY;BYDAY=MO
//! ```
//!
//! `DTSTART` is a local date and time with the IANA zone it is in (RFC
//! 5545, 3.3.5's third form), and the rule takes RFC 5545's grammar
//! (3.3.10) with each part at most once, in any order: `FREQ` daily,
//! weekly, monthly or yearly, `INTERVAL`, `COUNT` or `UNTIL` (a UTC time,
//! as the RFC requires with a zoned start), `WKST`, and `BYMONTH`,
//! `BYWEEKNO`, `BYYEARDAY`, `BYMONTHDAY`, `BYDAY` (with ordinals, `2TU`,
//! `-1FR`), `BYHOUR`, `BYMINUTE`, `BYSECOND` and `BYSETPOS`, where the
//! RFC's table allows each with the frequency. The start must be the
//! rule's first occurrence, since RFC 5545 leaves a recurrence whose start
//! the rule doesn't give undefined and calendars disagree on it.

use crate::ZONES;

/// How often a rule repeats: the period its other parts pick from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Frequency {
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

/// Where a rule stops.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum End {
    /// Never: an unbounded rule, which `temporal.occurrences` still takes
    /// only a `count` of.
    Never,
    /// After this many occurrences, the start the first.
    Count(u32),
    /// At this instant, milliseconds since 1970-01-01T00:00:00Z, included
    /// when the rule gives it.
    Until(i64),
}

/// A local date and clock time: days since 1970-01-01 and seconds since
/// midnight. Ordered as time runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct LocalAt {
    pub date: i32,
    pub second: u32,
}

/// A `BYDAY` entry: a weekday (0 for Monday), and the ordinal that picks
/// one of the month's or year's, `2` for the second and `-1` for the last.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WeekdayEntry {
    pub ordinal: Option<i32>,
    pub weekday: i64,
}

/// An RFC 5545 recurrence as `RecurrenceRule`'s text holds it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rule {
    /// The rule's zone, its place among `Zone`'s variants.
    pub zone: u32,
    /// The first occurrence, in local time.
    pub start: LocalAt,
    pub frequency: Frequency,
    pub interval: i64,
    pub end: End,
    /// The day weeks start on, 0 for Monday (RFC 5545's default).
    pub week_start: i64,
    pub months: Vec<i64>,
    pub week_numbers: Vec<i64>,
    pub year_days: Vec<i64>,
    pub month_days: Vec<i64>,
    pub weekdays: Vec<WeekdayEntry>,
    pub hours: Vec<i64>,
    pub minutes: Vec<i64>,
    pub seconds: Vec<i64>,
    pub set_positions: Vec<i64>,
}

/// RFC 5545's two-letter weekdays, Monday first.
const WEEKDAYS: [&str; 7] = ["MO", "TU", "WE", "TH", "FR", "SA", "SU"];

/// The last year a `CalendarDate` holds (area 10).
const LAST_YEAR: i64 = 9999;

impl Rule {
    /// The rule `text` holds, or `None` when it isn't one: not RFC 5545's
    /// grammar, a part the frequency doesn't take, or a start the rule
    /// doesn't give first.
    pub fn read(text: &str) -> Option<Rule> {
        let rule = Rule::parse(text)?;
        rule.period(0)?.contains(&rule.start).then_some(rule)
    }

    fn parse(text: &str) -> Option<Rule> {
        let (start_line, rule_line) = text.split_once('\n')?;
        let (zone_name, start) = start_line.strip_prefix("DTSTART;TZID=")?.split_once(':')?;
        let zone = ZONES.iter().position(|(_, iana)| *iana == zone_name)?;
        let (start, utc) = basic_date_time(start)?;
        if utc {
            return None;
        }
        let mut rule = Rule {
            zone: u32::try_from(zone).ok()?,
            start,
            frequency: Frequency::Daily,
            interval: 1,
            end: End::Never,
            week_start: 0,
            months: Vec::new(),
            week_numbers: Vec::new(),
            year_days: Vec::new(),
            month_days: Vec::new(),
            weekdays: Vec::new(),
            hours: Vec::new(),
            minutes: Vec::new(),
            seconds: Vec::new(),
            set_positions: Vec::new(),
        };
        let mut seen: Vec<&str> = Vec::new();
        let mut frequency = None;
        for part in rule_line.strip_prefix("RRULE:")?.split(';') {
            let (name, value) = part.split_once('=')?;
            if seen.contains(&name) {
                return None;
            }
            seen.push(name);
            match name {
                "FREQ" => {
                    frequency = Some(match value {
                        "DAILY" => Frequency::Daily,
                        "WEEKLY" => Frequency::Weekly,
                        "MONTHLY" => Frequency::Monthly,
                        "YEARLY" => Frequency::Yearly,
                        // `SECONDLY`, `MINUTELY` and `HOURLY` aren't taken
                        // (chosen in chunk 5.5k2).
                        _ => return None,
                    });
                }
                "INTERVAL" => rule.interval = number(value, 1, i64::from(u32::MAX), false)?,
                "COUNT" if rule.end == End::Never => {
                    rule.end = End::Count(
                        u32::try_from(number(value, 1, i64::from(u32::MAX), false)?).ok()?,
                    );
                }
                "UNTIL" if rule.end == End::Never => {
                    let (until, utc) = basic_date_time(value)?;
                    if !utc {
                        return None;
                    }
                    rule.end = End::Until(
                        i64::from(until.date) * 86_400_000 + i64::from(until.second) * 1000,
                    );
                }
                "WKST" => rule.week_start = weekday(value)?,
                "BYMONTH" => rule.months = list(value, 1, 12, false)?,
                "BYWEEKNO" => rule.week_numbers = list(value, 1, 53, true)?,
                "BYYEARDAY" => rule.year_days = list(value, 1, 366, true)?,
                "BYMONTHDAY" => rule.month_days = list(value, 1, 31, true)?,
                "BYDAY" => {
                    rule.weekdays = value
                        .split(',')
                        .map(weekday_entry)
                        .collect::<Option<Vec<_>>>()?;
                }
                "BYHOUR" => rule.hours = list(value, 0, 23, false)?,
                "BYMINUTE" => rule.minutes = list(value, 0, 59, false)?,
                "BYSECOND" => rule.seconds = list(value, 0, 59, false)?,
                "BYSETPOS" => rule.set_positions = list(value, 1, 366, true)?,
                _ => return None,
            }
        }
        rule.frequency = frequency?;
        rule.allowed().then_some(rule)
    }

    /// Whether each part goes with the frequency, as RFC 5545's table
    /// says (3.3.10): `BYWEEKNO` and `BYYEARDAY` only yearly, `BYMONTHDAY`
    /// never weekly, an ordinal on `BYDAY` only monthly or yearly (and not
    /// with `BYWEEKNO`), and `BYSETPOS` only beside another `BY` part.
    fn allowed(&self) -> bool {
        let yearly = self.frequency == Frequency::Yearly;
        let ordinals = self.weekdays.iter().any(|entry| entry.ordinal.is_some());
        let picks = !self.months.is_empty()
            || !self.week_numbers.is_empty()
            || !self.year_days.is_empty()
            || !self.month_days.is_empty()
            || !self.weekdays.is_empty()
            || !self.hours.is_empty()
            || !self.minutes.is_empty()
            || !self.seconds.is_empty();
        (self.week_numbers.is_empty() || yearly)
            && (self.year_days.is_empty() || yearly)
            && (self.month_days.is_empty() || self.frequency != Frequency::Weekly)
            && (!ordinals
                || matches!(self.frequency, Frequency::Monthly | Frequency::Yearly)
                    && self.week_numbers.is_empty())
            && (self.set_positions.is_empty() || picks)
    }

    /// The occurrences in local time from period `first` on (the start's
    /// is period 0), none before the start, in order, until the years a
    /// `CalendarDate` holds run out. `COUNT` and `UNTIL` are the caller's
    /// to apply, once each local time is an instant.
    pub fn local_occurrences(&self, first: i64) -> Occurrences<'_> {
        Occurrences {
            rule: self,
            period: first.max(0),
            pending: Vec::new(),
            done: false,
        }
    }

    /// The period holding `date`, or the last one starting before it: an
    /// expansion can start there rather than at the start.
    pub fn period_holding(&self, date: i32) -> i64 {
        let (year, month, _) = civil(i64::from(date));
        let (start_year, start_month, _) = civil(i64::from(self.start.date));
        let periods = match self.frequency {
            Frequency::Yearly => year - start_year,
            Frequency::Monthly => (year * 12 + month) - (start_year * 12 + start_month),
            Frequency::Weekly => (i64::from(date) - self.week_of_start()).div_euclid(7),
            Frequency::Daily => i64::from(date) - i64::from(self.start.date),
        };
        periods.div_euclid(self.interval)
    }

    /// The first day of the week holding the start.
    fn week_of_start(&self) -> i64 {
        let start = i64::from(self.start.date);
        start - (weekday_of(start) - self.week_start).rem_euclid(7)
    }

    /// The days period `k` runs over, or `None` once it starts past the
    /// last year a date holds.
    fn days(&self, k: i64) -> Option<Vec<i64>> {
        let step = k.checked_mul(self.interval)?;
        let (start_year, start_month, _) = civil(i64::from(self.start.date));
        let (first, last) = match self.frequency {
            Frequency::Yearly => {
                let year = start_year.checked_add(step)?;
                (days_from_civil(year, 1, 1), days_from_civil(year, 12, 31))
            }
            Frequency::Monthly => {
                let months = (start_year * 12 + start_month - 1).checked_add(step)?;
                let (year, month) = (months.div_euclid(12), months.rem_euclid(12) + 1);
                (
                    days_from_civil(year, month, 1),
                    days_from_civil(year, month, crate::days_in_month(year, month)),
                )
            }
            Frequency::Weekly => {
                let first = self.week_of_start().checked_add(step.checked_mul(7)?)?;
                (first, first + 6)
            }
            Frequency::Daily => {
                let day = i64::from(self.start.date).checked_add(step)?;
                (day, day)
            }
        };
        if first > i64::from(crate::DATE_MAX) || civil(first).0 > LAST_YEAR {
            return None;
        }
        Some(
            (first..=last.min(i64::from(crate::DATE_MAX)))
                .filter(|day| self.picks(*day))
                .collect(),
        )
    }

    /// Whether the rule's `BY` parts, with the defaults RFC 5545 takes
    /// from the start, pick `day`.
    fn picks(&self, day: i64) -> bool {
        let (year, month, month_day) = civil(day);
        let by_day = !self.week_numbers.is_empty()
            || !self.year_days.is_empty()
            || !self.month_days.is_empty()
            || !self.weekdays.is_empty();
        let (_, start_month, start_day) = civil(i64::from(self.start.date));
        let months: &[i64] = match self.frequency {
            Frequency::Yearly if !by_day && self.months.is_empty() => &[start_month],
            _ => &self.months,
        };
        if !months.is_empty() && !months.contains(&month) {
            return false;
        }
        if !self.week_numbers.is_empty() {
            let (week, weeks) = week_number(day, self.week_start);
            if !self
                .week_numbers
                .iter()
                .any(|n| *n == week || *n < 0 && weeks + 1 + n == week)
            {
                return false;
            }
        }
        let year_day = day - days_from_civil(year, 1, 1) + 1;
        let year_length = days_from_civil(year, 12, 31) - days_from_civil(year, 1, 1) + 1;
        if !self.year_days.is_empty()
            && !self
                .year_days
                .iter()
                .any(|n| *n == year_day || *n < 0 && year_length + 1 + n == year_day)
        {
            return false;
        }
        let month_length = crate::days_in_month(year, month);
        let month_days: &[i64] = match self.frequency {
            Frequency::Yearly | Frequency::Monthly if !by_day => &[start_day],
            _ => &self.month_days,
        };
        if !month_days.is_empty()
            && !month_days
                .iter()
                .any(|n| *n == month_day || *n < 0 && month_length + 1 + n == month_day)
        {
            return false;
        }
        let start_weekday = WeekdayEntry {
            ordinal: None,
            weekday: weekday_of(i64::from(self.start.date)),
        };
        let weekdays: &[WeekdayEntry] = match self.frequency {
            Frequency::Weekly if !by_day => std::slice::from_ref(&start_weekday),
            _ => &self.weekdays,
        };
        if weekdays.is_empty() {
            return true;
        }
        // An ordinal counts within the month when the rule is monthly or
        // names months, and within the year otherwise (RFC 5545's notes 1
        // and 2 to its table).
        let (at, length) = if self.frequency == Frequency::Monthly || !self.months.is_empty() {
            (month_day, month_length)
        } else {
            (year_day, year_length)
        };
        let weekday = weekday_of(day);
        let from_start = (at - 1) / 7 + 1;
        let from_end = -((length - at) / 7 + 1);
        weekdays.iter().any(|entry| {
            entry.weekday == weekday
                && entry.ordinal.is_none_or(|n| {
                    let n = i64::from(n);
                    n == from_start || n == from_end
                })
        })
    }

    /// The clock times each picked day gives, in order: `BYHOUR`,
    /// `BYMINUTE` and `BYSECOND`, each the start's where the rule has none.
    fn times(&self) -> Vec<u32> {
        let start = i64::from(self.start.second);
        let or_start = |given: &[i64], at: i64| -> Vec<i64> {
            if given.is_empty() {
                vec![at]
            } else {
                given.to_vec()
            }
        };
        let mut times = Vec::new();
        for hour in or_start(&self.hours, start / 3600) {
            for minute in or_start(&self.minutes, start / 60 % 60) {
                for second in or_start(&self.seconds, start % 60) {
                    times.extend(u32::try_from(hour * 3600 + minute * 60 + second).ok());
                }
            }
        }
        times.sort_unstable();
        times.dedup();
        times
    }

    /// Period `k`'s occurrences in local time, in order, `BYSETPOS`
    /// applied; `None` once the period is past the years a date holds.
    fn period(&self, k: i64) -> Option<Vec<LocalAt>> {
        let times = self.times();
        let mut all = Vec::new();
        for day in self.days(k)? {
            let date = i32::try_from(day).ok()?;
            all.extend(times.iter().map(|second| LocalAt {
                date,
                second: *second,
            }));
        }
        if self.set_positions.is_empty() {
            return Some(all);
        }
        let count = i64::try_from(all.len()).ok()?;
        let mut picked: Vec<LocalAt> = self
            .set_positions
            .iter()
            .filter_map(|position| {
                let at = if *position > 0 {
                    position - 1
                } else {
                    count + position
                };
                usize::try_from(at).ok().and_then(|at| all.get(at).copied())
            })
            .collect();
        picked.sort_unstable();
        picked.dedup();
        Some(picked)
    }
}

/// A rule's occurrences in local time, in order (see
/// [`Rule::local_occurrences`]).
#[derive(Debug)]
pub struct Occurrences<'r> {
    rule: &'r Rule,
    period: i64,
    pending: Vec<LocalAt>,
    done: bool,
}

impl Iterator for Occurrences<'_> {
    type Item = LocalAt;

    fn next(&mut self) -> Option<LocalAt> {
        while self.pending.is_empty() {
            if self.done {
                return None;
            }
            match self.rule.period(self.period) {
                Some(mut found) => {
                    found.retain(|at| *at >= self.rule.start);
                    found.reverse();
                    self.pending = found;
                    self.period += 1;
                }
                None => self.done = true,
            }
        }
        self.pending.pop()
    }
}

/// A number from `min` to `max`, with a sign when `signed` (and then never
/// zero), as RFC 5545 writes one: digits, `+` or `-` first.
fn number(text: &str, min: i64, max: i64, signed: bool) -> Option<i64> {
    let (negative, digits) = match text.as_bytes().first() {
        Some(b'-') if signed => (true, text.get(1..)?),
        Some(b'+') if signed => (false, text.get(1..)?),
        _ => (false, text),
    };
    if digits.is_empty() || digits.len() > 10 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let value: i64 = digits.parse().ok()?;
    (min..=max)
        .contains(&value)
        .then_some(if negative { -value } else { value })
}

/// A comma-separated list of numbers, none empty.
fn list(text: &str, min: i64, max: i64, signed: bool) -> Option<Vec<i64>> {
    text.split(',')
        .map(|item| number(item, min, max, signed))
        .collect()
}

/// A weekday by its two letters, 0 for Monday.
fn weekday(text: &str) -> Option<i64> {
    WEEKDAYS
        .iter()
        .position(|day| *day == text)
        .and_then(|at| i64::try_from(at).ok())
}

/// A `BYDAY` entry: an optional ordinal from 1 to 53 with its sign, then
/// the weekday, `2TU`, `-1FR`, `MO`.
fn weekday_entry(text: &str) -> Option<WeekdayEntry> {
    let split = text.len().checked_sub(2)?;
    let (ordinal, day) = (text.get(..split)?, text.get(split..)?);
    Some(WeekdayEntry {
        ordinal: if ordinal.is_empty() {
            None
        } else {
            Some(i32::try_from(number(ordinal, 1, 53, true)?).ok()?)
        },
        weekday: weekday(day)?,
    })
}

/// RFC 5545's basic date and time, `20261026T093000`, with a `Z` for UTC:
/// a real date in the years a `CalendarDate` holds and a clock time to the
/// second (no leap second).
pub(super) fn basic_date_time(text: &str) -> Option<(LocalAt, bool)> {
    let (text, utc) = match text.strip_suffix('Z') {
        Some(local) => (local, true),
        None => (text, false),
    };
    if text.len() != 15 || text.as_bytes().get(8) != Some(&b'T') {
        return None;
    }
    let digits = |range: std::ops::Range<usize>| -> Option<i64> {
        let part = text.get(range)?;
        part.bytes()
            .all(|b| b.is_ascii_digit())
            .then(|| part.parse().ok())
            .flatten()
    };
    let (year, month, day) = (digits(0..4)?, digits(4..6)?, digits(6..8)?);
    let (hour, minute, second) = (digits(9..11)?, digits(11..13)?, digits(13..15)?);
    if !(1..=12).contains(&month)
        || !(1..=crate::days_in_month(year, month)).contains(&day)
        || hour > 23
        || minute > 59
        || second > 59
    {
        return None;
    }
    Some((
        LocalAt {
            date: crate::epoch_day(year, month, day)?,
            second: u32::try_from(hour * 3600 + minute * 60 + second).ok()?,
        },
        utc,
    ))
}

/// The weekday of the day `day` days after 1970-01-01 (a Thursday), 0 for
/// Monday.
pub fn weekday_of(day: i64) -> i64 {
    (day + 3).rem_euclid(7)
}

/// The year, month and day of `day` days after 1970-01-01, for any day.
fn civil(day: i64) -> (i64, i64, i64) {
    let z = day + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_from_march = (5 * day_of_year + 2) / 153;
    let month_day = day_of_year - (153 * month_from_march + 2) / 5 + 1;
    let month = if month_from_march < 10 {
        month_from_march + 3
    } else {
        month_from_march - 9
    };
    (
        year_of_era + era * 400 + i64::from(month <= 2),
        month,
        month_day,
    )
}

/// Days since 1970-01-01 of a year, month and day, for any year.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month_from_march = (month + 9) % 12;
    let day_of_year = (153 * month_from_march + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// The first day of week 1 of `year`, weeks starting on `week_start`: the
/// first week with at least four of the year's days (RFC 5545, `BYWEEKNO`).
fn week_one(year: i64, week_start: i64) -> i64 {
    let first = days_from_civil(year, 1, 1);
    let into_week = (weekday_of(first) - week_start).rem_euclid(7);
    if 7 - into_week >= 4 {
        first - into_week
    } else {
        first - into_week + 7
    }
}

/// The week `day` is in and how many weeks its week-numbering year has:
/// a day of late December may be in week 1 of the next year, and one of
/// early January in the last week of the year before.
fn week_number(day: i64, week_start: i64) -> (i64, i64) {
    let (mut year, _, _) = civil(day);
    if day < week_one(year, week_start) {
        year -= 1;
    } else if day >= week_one(year + 1, week_start) {
        year += 1;
    }
    let first = week_one(year, week_start);
    (
        (day - first) / 7 + 1,
        (week_one(year + 1, week_start) - first) / 7,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(text: &str) -> LocalAt {
        basic_date_time(text).expect("a date and time").0
    }

    fn written(at: LocalAt) -> String {
        let (year, month, day) = civil(i64::from(at.date));
        format!(
            "{year:04}{month:02}{day:02}T{:02}{:02}{:02}",
            at.second / 3600,
            at.second / 60 % 60,
            at.second % 60
        )
    }

    /// The first `n` occurrences of `rule` starting at `start` in New
    /// York, in local time.
    fn first(start: &str, rule: &str, n: usize) -> Vec<String> {
        let text = format!("DTSTART;TZID=America/New_York:{start}\nRRULE:{rule}");
        let rule = Rule::read(&text).unwrap_or_else(|| panic!("`{text}` is a rule"));
        rule.local_occurrences(0).take(n).map(written).collect()
    }

    fn days(start: &str, rule: &str, n: usize) -> Vec<String> {
        first(start, rule, n)
            .into_iter()
            .map(|text| text.get(..8).unwrap_or_default().to_string())
            .collect()
    }

    #[test]
    fn rfc_5545_s_examples() {
        // Each from RFC 5545, 3.8.5.3, in local time (the zone comes in
        // `jadpo-value`).
        // Daily for 10 occurrences.
        assert_eq!(
            days("19970902T090000", "FREQ=DAILY;COUNT=10", 3),
            ["19970902", "19970903", "19970904"]
        );
        // Every other day.
        assert_eq!(
            days("19970902T090000", "FREQ=DAILY;INTERVAL=2", 3),
            ["19970902", "19970904", "19970906"]
        );
        // Every 10 days.
        assert_eq!(
            days("19970902T090000", "FREQ=DAILY;INTERVAL=10;COUNT=5", 5),
            ["19970902", "19970912", "19970922", "19971002", "19971012"]
        );
        // Every day in January, for 3 years.
        assert_eq!(days("19980101T090000", "FREQ=DAILY;BYMONTH=1", 33), {
            let mut expected: Vec<String> = (1..=31).map(|day| format!("199801{day:02}")).collect();
            expected.extend(["19990101".to_string(), "19990102".to_string()]);
            expected
        });
        // Weekly on Tuesday and Thursday, weeks from Sunday.
        assert_eq!(
            days("19970902T090000", "FREQ=WEEKLY;WKST=SU;BYDAY=TU,TH", 4),
            ["19970902", "19970904", "19970909", "19970911"]
        );
        // Every other week on Monday, Wednesday and Friday.
        assert_eq!(
            days(
                "19970901T090000",
                "FREQ=WEEKLY;INTERVAL=2;WKST=SU;BYDAY=MO,WE,FR",
                6
            ),
            [
                "19970901", "19970903", "19970905", "19970915", "19970917", "19970919"
            ]
        );
        // Monthly on the first Friday.
        assert_eq!(
            days("19970905T090000", "FREQ=MONTHLY;COUNT=10;BYDAY=1FR", 3),
            ["19970905", "19971003", "19971107"]
        );
        // Every other month on the first and last Sunday.
        assert_eq!(
            days(
                "19970907T090000",
                "FREQ=MONTHLY;INTERVAL=2;COUNT=10;BYDAY=1SU,-1SU",
                4
            ),
            ["19970907", "19970928", "19971102", "19971130"]
        );
        // Monthly on the second-to-last Monday.
        assert_eq!(
            days("19970922T090000", "FREQ=MONTHLY;COUNT=6;BYDAY=-2MO", 3),
            ["19970922", "19971020", "19971117"]
        );
        // Monthly on the third-to-last day.
        assert_eq!(
            days("19970928T090000", "FREQ=MONTHLY;BYMONTHDAY=-3", 3),
            ["19970928", "19971029", "19971128"]
        );
        // Monthly on the 2nd and 15th.
        assert_eq!(
            days(
                "19970902T090000",
                "FREQ=MONTHLY;COUNT=10;BYMONTHDAY=2,15",
                4
            ),
            ["19970902", "19970915", "19971002", "19971015"]
        );
        // Monthly on the first and last day.
        assert_eq!(
            days(
                "19970930T090000",
                "FREQ=MONTHLY;COUNT=10;BYMONTHDAY=1,-1",
                4
            ),
            ["19970930", "19971001", "19971031", "19971101"]
        );
        // Yearly in June and July.
        assert_eq!(
            days("19970610T090000", "FREQ=YEARLY;COUNT=10;BYMONTH=6,7", 4),
            ["19970610", "19970710", "19980610", "19980710"]
        );
        // Every third year on the 1st, 100th and 200th day.
        assert_eq!(
            days(
                "19970101T090000",
                "FREQ=YEARLY;INTERVAL=3;COUNT=10;BYYEARDAY=1,100,200",
                4
            ),
            ["19970101", "19970410", "19970719", "20000101"]
        );
        // Every 20th Monday of the year.
        assert_eq!(
            days("19970519T090000", "FREQ=YEARLY;BYDAY=20MO", 3),
            ["19970519", "19980518", "19990517"]
        );
        // Monday of week 20, weeks from Monday.
        assert_eq!(
            days("19970512T090000", "FREQ=YEARLY;BYWEEKNO=20;BYDAY=MO", 3),
            ["19970512", "19980511", "19990517"]
        );
        // Every Thursday in March.
        assert_eq!(
            days("19970313T090000", "FREQ=YEARLY;BYMONTH=3;BYDAY=TH", 4),
            ["19970313", "19970320", "19970327", "19980305"]
        );
        // Every Friday the 13th.
        assert_eq!(
            days("19980213T090000", "FREQ=MONTHLY;BYDAY=FR;BYMONTHDAY=13", 3),
            ["19980213", "19980313", "19981113"]
        );
        // The first Saturday after the first Sunday of the month.
        assert_eq!(
            days(
                "19970913T090000",
                "FREQ=MONTHLY;BYDAY=SA;BYMONTHDAY=7,8,9,10,11,12,13",
                3
            ),
            ["19970913", "19971011", "19971108"]
        );
        // US presidential election day: every four years, the first
        // Tuesday after a Monday in November.
        assert_eq!(
            days(
                "19961105T090000",
                "FREQ=YEARLY;INTERVAL=4;BYMONTH=11;BYDAY=TU;BYMONTHDAY=2,3,4,5,6,7,8",
                3
            ),
            ["19961105", "20001107", "20041102"]
        );
        // The third Tuesday, Wednesday or Thursday of the month.
        assert_eq!(
            days(
                "19970904T090000",
                "FREQ=MONTHLY;COUNT=3;BYDAY=TU,WE,TH;BYSETPOS=3",
                3
            ),
            ["19970904", "19971007", "19971106"]
        );
        // The second-to-last weekday of the month.
        assert_eq!(
            days(
                "19970929T090000",
                "FREQ=MONTHLY;BYDAY=MO,TU,WE,TH,FR;BYSETPOS=-2",
                3
            ),
            ["19970929", "19971030", "19971127"]
        );
        // Every 20 minutes from 9 to 16:40, every day: as a daily rule with
        // `BYHOUR` and `BYMINUTE`.
        let times = first(
            "19970902T090000",
            "FREQ=DAILY;BYHOUR=9,10,11,12,13,14,15,16;BYMINUTE=0,20,40",
            4,
        );
        assert_eq!(
            times,
            [
                "19970902T090000",
                "19970902T092000",
                "19970902T094000",
                "19970902T100000"
            ]
        );
        // Week starts change an every-other-week rule.
        assert_eq!(
            days(
                "19970805T090000",
                "FREQ=WEEKLY;INTERVAL=2;COUNT=4;BYDAY=TU,SU;WKST=MO",
                4
            ),
            ["19970805", "19970810", "19970819", "19970824"]
        );
        assert_eq!(
            days(
                "19970805T090000",
                "FREQ=WEEKLY;INTERVAL=2;COUNT=4;BYDAY=TU,SU;WKST=SU",
                4
            ),
            ["19970805", "19970817", "19970819", "19970831"]
        );
    }

    #[test]
    fn days_a_month_doesn_t_have_are_skipped() {
        // RFC 5545: an invalid date (the 31st of a 30-day month, 30
        // February) is ignored and not counted.
        assert_eq!(
            days("20260131T090000", "FREQ=MONTHLY;COUNT=5", 4),
            ["20260131", "20260331", "20260531", "20260731"]
        );
        assert_eq!(
            days("20280229T090000", "FREQ=YEARLY", 3),
            ["20280229", "20320229", "20360229"]
        );
        // The last day of each month, by counting from the end.
        assert_eq!(
            days("20260131T090000", "FREQ=MONTHLY;BYMONTHDAY=-1", 3),
            ["20260131", "20260228", "20260331"]
        );
        // The last Friday of each month.
        assert_eq!(
            days("20260130T170000", "FREQ=MONTHLY;BYDAY=-1FR", 3),
            ["20260130", "20260227", "20260327"]
        );
    }

    #[test]
    fn what_isn_t_a_rule() {
        let zoned =
            |rule: &str| format!("DTSTART;TZID=Europe/London:20261026T093000\nRRULE:{rule}");
        assert!(Rule::read(&zoned("FREQ=WEEKLY;BYDAY=MO")).is_some());
        for rule in [
            // No frequency, or one not taken.
            "BYDAY=MO",
            "FREQ=HOURLY",
            "FREQ=weekly",
            // A part twice, both ends, an unknown part.
            "FREQ=WEEKLY;FREQ=DAILY",
            "FREQ=DAILY;COUNT=3;UNTIL=20261231T000000Z",
            "FREQ=DAILY;X-NAME=1",
            // Out of range or the wrong sign.
            "FREQ=DAILY;INTERVAL=0",
            "FREQ=DAILY;COUNT=0",
            "FREQ=MONTHLY;BYMONTHDAY=0",
            "FREQ=MONTHLY;BYMONTHDAY=32",
            "FREQ=YEARLY;BYMONTH=13",
            "FREQ=DAILY;BYHOUR=24",
            "FREQ=DAILY;BYMONTH=-1",
            // A local `UNTIL` with a zoned start.
            "FREQ=DAILY;UNTIL=20261231T000000",
            // A part the frequency doesn't take.
            "FREQ=WEEKLY;BYMONTHDAY=1",
            "FREQ=MONTHLY;BYWEEKNO=1",
            "FREQ=DAILY;BYYEARDAY=1",
            "FREQ=WEEKLY;BYDAY=1MO",
            "FREQ=DAILY;BYSETPOS=1",
            // A start the rule doesn't give: 26 October 2026 is a Monday.
            "FREQ=WEEKLY;BYDAY=TU",
            "FREQ=MONTHLY;BYMONTHDAY=1",
            "",
        ] {
            assert_eq!(Rule::read(&zoned(rule)), None, "`{rule}`");
        }
        for text in [
            "DTSTART;TZID=Europe/London:20261026T093000",
            "DTSTART:20261026T093000Z\nRRULE:FREQ=DAILY",
            "DTSTART;TZID=Europe/Nowhere:20261026T093000\nRRULE:FREQ=DAILY",
            "DTSTART;TZID=Europe/London:20261026T243000\nRRULE:FREQ=DAILY",
            "DTSTART;TZID=Europe/London:20260230T093000\nRRULE:FREQ=DAILY",
            "DTSTART;TZID=Europe/London:20261026T093000Z\nRRULE:FREQ=DAILY",
            "DTSTART;TZID=Europe/London:20261026T093000\r\nRRULE:FREQ=DAILY",
            "RRULE:FREQ=DAILY\nDTSTART;TZID=Europe/London:20261026T093000",
        ] {
            assert_eq!(Rule::read(text), None, "`{text}`");
        }
    }

    #[test]
    fn an_expansion_can_start_near_a_later_day() {
        let rule = Rule::read(
            "DTSTART;TZID=Europe/London:20260105T090000\nRRULE:FREQ=WEEKLY;INTERVAL=2;BYDAY=MO",
        )
        .expect("a rule");
        let later = at("20261026T000000");
        let period = rule.period_holding(later.date);
        let from_there: Vec<LocalAt> = rule
            .local_occurrences(period)
            .take_while(|at| at.date < later.date + 30)
            .collect();
        let from_start: Vec<LocalAt> = rule
            .local_occurrences(0)
            .filter(|at| at.date >= later.date - 14)
            .take_while(|at| at.date < later.date + 30)
            .collect();
        assert!(from_start.ends_with(&from_there) && !from_there.is_empty());
        assert_eq!(week_number(days_from_civil(2026, 12, 31), 0), (53, 53));
        assert_eq!(week_number(days_from_civil(2027, 1, 1), 0), (53, 53));
        assert_eq!(week_number(days_from_civil(2025, 12, 29), 0), (1, 53));
    }
}
