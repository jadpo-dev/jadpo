//! `OpeningHours` (area 19, "Time and calendar additions", chunk 5.5k2):
//! the weekly hours of a shop or a clinic, with the days that differ, in
//! OpenStreetMap's `opening_hours` syntax followed by the zone the clocks
//! are read in, as RFC 9557 writes a zone after a time:
//!
//! ```text
//! Mo-Fr 09:00-17:30; Sa 10:00-16:00; Fr 22:00-02:00; 2026 Dec 25 off [Europe/London]
//! ```
//!
//! The hours are held as that text; this module reads it, for the checker
//! and the runtime alike. Of OpenStreetMap's syntax it takes the part
//! every implementation agrees on (chosen in chunk 5.5k2): rules joined by
//! `; `, each a selector and either times or `off`. A selector is
//! weekdays (`Mo`, `Mo-Fr`, `Sa,Su`, a range across the week's end
//! `Fr-Mo`) or one date (`2026 Dec 25`); times are `HH:MM-HH:MM` spans
//! joined by commas, the end up to `24:00`, and an end at or before the
//! start running past midnight into the next day. A later rule replaces
//! an earlier one on the days both select, as in OpenStreetMap, and a span
//! past midnight belongs to the day it opens on. Days no rule selects are
//! closed.

use crate::ZONES;

/// The days a rule selects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Days {
    /// These weekdays, Monday first.
    Weekdays([bool; 7]),
    /// One date, days since 1970-01-01.
    Date(i32),
}

/// One rule: the days it selects and its spans, each the minutes since
/// midnight it opens and closes at; none for `off`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HoursRule {
    pub days: Days,
    pub spans: Vec<(u32, u32)>,
}

/// Opening hours as `OpeningHours`'s text holds them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpeningHours {
    /// The zone the hours are read in, its place among `Zone`'s variants.
    pub zone: u32,
    pub rules: Vec<HoursRule>,
}

/// OpenStreetMap's two-letter weekdays, Monday first.
const WEEKDAYS: [&str; 7] = ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"];

/// OpenStreetMap's three-letter months.
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// A day's minutes.
const DAY_MINUTES: u32 = 24 * 60;

impl OpeningHours {
    /// The hours `text` holds, or `None` when it isn't the syntax above.
    pub fn read(text: &str) -> Option<OpeningHours> {
        let (rules, zone) = text.strip_suffix(']')?.rsplit_once(" [")?;
        let zone = ZONES.iter().position(|(_, iana)| *iana == zone)?;
        Some(OpeningHours {
            zone: u32::try_from(zone).ok()?,
            rules: rules.split("; ").map(rule).collect::<Option<Vec<_>>>()?,
        })
    }

    /// The rule that decides `date` (days since 1970-01-01, a weekday
    /// `0` for Monday): the last that selects it.
    fn rule_for(&self, date: i64) -> Option<&HoursRule> {
        let weekday = usize::try_from((date + 3).rem_euclid(7)).ok()?;
        self.rules.iter().rev().find(|rule| match rule.days {
            Days::Weekdays(days) => days.get(weekday).copied().unwrap_or(false),
            Days::Date(day) => i64::from(day) == date,
        })
    }

    /// Whether the hours are open at the local clock time `millis` (since
    /// midnight) on `date`: within a span that opens that day, or within
    /// one that opened the day before and runs past midnight.
    pub fn open_at(&self, date: i32, millis: u32) -> bool {
        let minute = |at: u32| u64::from(at) * 60_000;
        let at = u64::from(millis);
        let today = self.rule_for(i64::from(date)).is_some_and(|rule| {
            rule.spans
                .iter()
                .any(|(open, close)| minute(*open) <= at && (close <= open || at < minute(*close)))
        });
        let from_yesterday = self.rule_for(i64::from(date) - 1).is_some_and(|rule| {
            rule.spans
                .iter()
                .any(|(open, close)| close <= open && at < minute(*close))
        });
        today || from_yesterday
    }
}

/// One rule: a selector, a space, and times or `off`.
fn rule(text: &str) -> Option<HoursRule> {
    let (days, times) = match date(text) {
        Some((day, rest)) => (Days::Date(day), rest),
        None => {
            let (selector, rest) = text.split_once(' ')?;
            (Days::Weekdays(weekdays(selector)?), rest)
        }
    };
    let spans = if times == "off" {
        Vec::new()
    } else {
        times.split(',').map(span).collect::<Option<Vec<_>>>()?
    };
    Some(HoursRule { days, spans })
}

/// A date selector at the start of `text`, `2026 Dec 25 `, and what
/// follows it.
fn date(text: &str) -> Option<(i32, &str)> {
    let mut parts = text.splitn(4, ' ');
    let (year, month, day, rest) = (parts.next()?, parts.next()?, parts.next()?, parts.next()?);
    let digits = |part: &str, length: usize| {
        (part.len() == length && part.bytes().all(|b| b.is_ascii_digit()))
            .then(|| part.parse::<i64>().ok())
            .flatten()
    };
    let (year, day) = (digits(year, 4)?, digits(day, 2)?);
    let month = i64::try_from(MONTHS.iter().position(|name| *name == month)? + 1).ok()?;
    if !(1..=crate::days_in_month(year, month)).contains(&day) {
        return None;
    }
    Some((crate::epoch_day(year, month, day)?, rest))
}

/// Weekdays: single days and ranges joined by commas.
fn weekdays(text: &str) -> Option<[bool; 7]> {
    let at = |name: &str| WEEKDAYS.iter().position(|day| *day == name);
    let mut days = [false; 7];
    for item in text.split(',') {
        let (first, last) = match item.split_once('-') {
            Some((first, last)) => (at(first)?, at(last)?),
            None => (at(item)?, at(item)?),
        };
        let mut day = first;
        loop {
            *days.get_mut(day)? = true;
            if day == last {
                break;
            }
            day = (day + 1) % 7;
        }
    }
    Some(days)
}

/// A span, `09:00-17:30`: it opens from `00:00` to `23:59` and closes up
/// to `24:00`, never at the minute it opens.
fn span(text: &str) -> Option<(u32, u32)> {
    let (open, close) = text.split_once('-')?;
    let (open, close) = (clock(open)?, clock(close)?);
    (open < DAY_MINUTES && open != close).then_some((open, close))
}

/// A clock time to the minute, `HH:MM`, up to `24:00`.
fn clock(text: &str) -> Option<u32> {
    let (hour, minute) = text.split_once(':')?;
    let two = |part: &str| {
        (part.len() == 2 && part.bytes().all(|b| b.is_ascii_digit()))
            .then(|| part.parse::<u32>().ok())
            .flatten()
    };
    let (hour, minute) = (two(hour)?, two(minute)?);
    let at = hour * 60 + minute;
    (minute < 60 && at <= DAY_MINUTES).then_some(at)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(text: &str) -> i32 {
        crate::date_number(text).expect("a date")
    }

    fn clock_at(text: &str) -> u32 {
        crate::clock_millis(text).expect("a clock time")
    }

    #[test]
    fn a_week_s_hours_with_a_night_and_an_exception() {
        let hours = OpeningHours::read(
            "Mo-Fr 09:00-17:30; Sa 10:00-16:00; Fr 22:00-02:00; 2026 Dec 25 off [Europe/London]",
        )
        .expect("hours");
        // Friday 23 October 2026: open in the day, and the night replaces
        // the day's hours, as a later rule for the same days does.
        assert!(!hours.open_at(day("2026-10-23"), clock_at("12:00")));
        assert!(hours.open_at(day("2026-10-23"), clock_at("23:00")));
        // Its night runs into Saturday.
        assert!(hours.open_at(day("2026-10-24"), clock_at("01:59")));
        assert!(!hours.open_at(day("2026-10-24"), clock_at("02:00")));
        assert!(hours.open_at(day("2026-10-24"), clock_at("10:00")));
        // Sunday has no rule.
        assert!(!hours.open_at(day("2026-10-25"), clock_at("12:00")));
        assert!(hours.open_at(day("2026-10-26"), clock_at("09:00")));
        assert!(!hours.open_at(day("2026-10-26"), clock_at("17:30")));
        // Christmas Day, a Friday, is off, night and all.
        assert!(!hours.open_at(day("2026-12-25"), clock_at("23:00")));
        assert!(!hours.open_at(day("2026-12-26"), clock_at("01:00")));
    }

    #[test]
    fn what_isn_t_opening_hours() {
        assert!(OpeningHours::read("Mo-Su 00:00-24:00 [UTC]").is_some());
        assert!(OpeningHours::read("Sa-Mo 10:00-12:00,13:00-16:00 [America/New_York]").is_some());
        for text in [
            "Mo-Fr 09:00-17:30",
            "Mo-Fr 09:00-17:30 [London]",
            "Mo-Fr 09:00-17:30 [Europe/London",
            "Mo-Fr 9:00-17:30 [Europe/London]",
            "Mo-Fr 09:00-25:00 [Europe/London]",
            "Mo-Fr 24:00-02:00 [Europe/London]",
            "Mo-Fr 09:00-09:00 [Europe/London]",
            "Mo-Fr 09:60-17:00 [Europe/London]",
            "mo-fr 09:00-17:30 [Europe/London]",
            "Mo-Fr closed [Europe/London]",
            "Mo-Fr 09:00-17:30;Sa 10:00-16:00 [Europe/London]",
            "2026 Feb 30 off [Europe/London]",
            "2026 Dec 5 off [Europe/London]",
            "Dec 25 off [Europe/London]",
            " [Europe/London]",
            "",
        ] {
            assert_eq!(OpeningHours::read(text), None, "`{text}`");
        }
    }
}
