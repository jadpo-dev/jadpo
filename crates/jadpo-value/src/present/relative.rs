//! `temporal.format_relative` (area 10, "Showing dates and times to
//! people"): fixed rules, the words from the locale.
//!
//! Against `now`, in the zone the value is shown in:
//!
//! - under a minute either way: "just now" in English, the locale's "now"
//!   elsewhere;
//! - under an hour: whole minutes, "3 minutes ago" or "in 3 minutes",
//!   counting a past value down and a future one up, so a value is never
//!   said to be further past than it is;
//! - the same local date: "Today at" and the short time;
//! - the date before or after: "Yesterday at" or "Tomorrow at" and the
//!   short time;
//! - two to six days away: the weekday and the short time;
//! - anything else: the long date.
//!
//! The days are the dates on the clock in that zone, not blocks of 24
//! hours, so a day with 23 or 25 hours is still one day.

use fixed_decimal::Decimal;
use icu_datetime::DateTimeFormatterPreferences;
use icu_datetime::fieldsets::builder::DateFields;
use icu_datetime::options::{Length, TimePrecision};
use icu_datetime::pattern::DateTimePattern;
use icu_experimental::relativetime::options::Numeric;
use icu_experimental::relativetime::{
    RelativeTimeFormatter, RelativeTimeFormatterOptions, RelativeTimeFormatterPreferences,
};
use icu_locale::Locale;
use writeable::Writeable;

use super::{DATE_SLOT, Moment, Parts, REST_SLOT, glue, locale_of, render, zoned};
use crate::Fault;

const MINUTE: i64 = 60_000;
const HOUR: i64 = 60 * MINUTE;

/// How `value` reads against `now`; `now_date` is the date it is where
/// the value is seen.
pub fn format_relative(
    value: Moment,
    value_millis: i64,
    now_millis: i64,
    now_date: i32,
    locale: &str,
) -> Result<String, Fault> {
    let locale = locale_of(locale)?;
    let difference = value_millis
        .checked_sub(now_millis)
        .ok_or(Fault::TimeRange)?;
    let apart = difference.checked_abs().ok_or(Fault::TimeRange)?;
    if apart < MINUTE {
        return now_text(&locale);
    }
    if apart < HOUR {
        let minutes = if difference < 0 {
            apart / MINUTE
        } else {
            (apart + MINUTE - 1) / MINUTE
        };
        let signed = if difference < 0 { -minutes } else { minutes };
        return relative_number(&locale, Unit::Minute, signed, Numeric::Always);
    }
    let days = i64::from(value.date) - i64::from(now_date);
    match days {
        -1..=1 => day_at(&locale, value, days),
        2..=6 | -6..=-2 => weekday_at(&locale, value),
        _ => long_date(&locale, value),
    }
}

#[derive(Clone, Copy)]
enum Unit {
    Second,
    Minute,
    Day,
}

fn relative_number(
    locale: &Locale,
    unit: Unit,
    value: i64,
    numeric: Numeric,
) -> Result<String, Fault> {
    let prefs = RelativeTimeFormatterPreferences::from(locale);
    let mut options = RelativeTimeFormatterOptions::default();
    options.numeric = numeric;
    let formatter = match unit {
        Unit::Second => RelativeTimeFormatter::try_new_long_second(prefs, options),
        Unit::Minute => RelativeTimeFormatter::try_new_long_minute(prefs, options),
        Unit::Day => RelativeTimeFormatter::try_new_long_day(prefs, options),
    }
    .map_err(|_| Fault::Shape)?;
    Ok(formatter
        .format(Decimal::from(value))
        .write_to_string()
        .into_owned())
}

/// "Just now", or the locale's word for it.
fn now_text(locale: &Locale) -> Result<String, Fault> {
    if locale.id.language.as_str() == "en" {
        return Ok("just now".into());
    }
    relative_number(locale, Unit::Second, 0, Numeric::Auto)
}

/// The locale's word for a day one off today, when it has one, set at the
/// start of a sentence.
fn day_word(locale: &Locale, days: i64) -> Result<Option<String>, Fault> {
    let word = relative_number(locale, Unit::Day, days, Numeric::Auto)?;
    let counted = relative_number(locale, Unit::Day, days, Numeric::Always)?;
    // Where the locale has no word for the day, "automatic" is the count.
    if word == counted {
        return Ok(None);
    }
    Ok(Some(capitalized(&word)))
}

/// The first letter in upper case, where the script has the difference.
fn capitalized(word: &str) -> String {
    let mut chars = word.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    // Georgian's capitals are not for the start of a sentence.
    let upper: String = if ('\u{10D0}'..='\u{10FF}').contains(&first) {
        first.to_string()
    } else {
        first.to_uppercase().collect()
    };
    format!("{upper}{}", chars.as_str())
}

/// Text as a pattern's literal.
fn literal(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}

/// The weekday, or a word for the day, with the time the locale glues to
/// a long date ("Yesterday at 14:30"): the glue is cut from its pattern
/// for a date and a time, and the date put back as the day.
fn word_at(locale: &Locale, value: Moment, word: Option<&str>) -> Result<String, Fault> {
    let at = zoned(value)?;
    let prefs = DateTimeFormatterPreferences::from(locale);
    let dated = Parts {
        date: Some(DateFields::YMD),
        time: Some(TimePrecision::Minute),
        zone: None,
    };
    let glue = glue(prefs, Length::Long, dated, &at)?;
    let time = dated
        .rest_only()
        .pattern(prefs, Length::Short, &at)?
        .ok_or(Fault::Shape)?;
    let weekday = Parts {
        date: Some(DateFields::E),
        time: None,
        zone: None,
    }
    .pattern(prefs, Length::Long, &at)?
    .ok_or(Fault::Shape)?;
    let day = word.map_or(weekday, literal);
    let pattern = match glue {
        Some(glue) => glue.replace(DATE_SLOT, &day).replace(REST_SLOT, &time),
        None => format!("{day} {time}"),
    };
    let pattern = DateTimePattern::try_from_pattern_str(&pattern).map_err(|_| Fault::Shape)?;
    render(prefs, &pattern, &at).map(|text| capitalized(&text))
}

fn day_at(locale: &Locale, value: Moment, days: i64) -> Result<String, Fault> {
    match day_word(locale, days)? {
        Some(word) => word_at(locale, value, Some(&word)),
        None => long_date(locale, value),
    }
}

fn weekday_at(locale: &Locale, value: Moment) -> Result<String, Fault> {
    word_at(locale, value, None)
}

fn long_date(locale: &Locale, value: Moment) -> Result<String, Fault> {
    let at = zoned(value)?;
    let prefs = DateTimeFormatterPreferences::from(locale);
    let parts = Parts {
        date: Some(DateFields::YMD),
        time: None,
        zone: None,
    };
    let pattern = parts
        .pattern(prefs, Length::Long, &at)?
        .ok_or(Fault::Shape)?;
    let pattern = DateTimePattern::try_from_pattern_str(&pattern).map_err(|_| Fault::Shape)?;
    render(prefs, &pattern, &at)
}

#[cfg(test)]
mod tests {
    use super::super::{format_relative_value, tests_support as support};
    use crate::Value;

    /// 2026-09-27 13:30:00Z, 14:30 in London (British Summer Time).
    const NOW: i64 = 1_790_515_800_000;
    const SECOND: i64 = 1000;
    const MINUTE: i64 = 60 * SECOND;
    const HOUR: i64 = 60 * MINUTE;
    const DAY: i64 = 24 * HOUR;

    fn said(locale: &str, zone: &str, offset: i64) -> String {
        support::text(format_relative_value(
            &Value::Instant(NOW + offset),
            &Value::Instant(NOW),
            &support::zone_named(zone),
            &support::locale_value(locale),
        ))
    }

    fn london(locale: &str, offset: i64) -> String {
        said(locale, "Europe/London", offset)
    }

    #[test]
    fn under_a_minute_either_way_is_just_now() {
        for offset in [0, 20 * SECOND, -20 * SECOND, 59 * SECOND, -59 * SECOND] {
            assert_eq!(london("en-GB", offset), "just now", "{offset}");
        }
        assert_eq!(london("fr-FR", -20 * SECOND), "maintenant");
        assert_eq!(london("de-DE", 5 * SECOND), "jetzt");
        assert_eq!(london("ja-JP", 0), "今");
    }

    #[test]
    fn under_an_hour_is_whole_minutes_rounded_toward_now_for_the_past() {
        assert_eq!(london("en-GB", -MINUTE), "1 minute ago");
        assert_eq!(
            london("en-GB", -119 * SECOND),
            "1 minute ago",
            "a past value counts down"
        );
        assert_eq!(
            london("en-GB", 61 * SECOND),
            "in 2 minutes",
            "a future value counts up"
        );
        assert_eq!(london("en-GB", -4 * MINUTE), "4 minutes ago");
        assert_eq!(london("en-GB", 10 * MINUTE), "in 10 minutes");
        assert_eq!(
            london("en-GB", -59 * MINUTE - 59 * SECOND),
            "59 minutes ago"
        );
        assert_eq!(london("fr-FR", -4 * MINUTE), "il y a 4 minutes");
        assert_eq!(london("de-DE", -4 * MINUTE), "vor 4 Minuten");
        assert_eq!(london("ja-JP", 10 * MINUTE), "10 分後");
        assert_eq!(london("ar-EG", -4 * MINUTE), "قبل ٤ دقائق");
    }

    #[test]
    fn the_same_date_the_day_before_and_after_say_so_with_the_time() {
        assert_eq!(london("en-GB", -HOUR - 30 * MINUTE), "Today at 13:00");
        assert_eq!(london("en-US", -HOUR - 30 * MINUTE), "Today at 1:00 PM");
        assert_eq!(london("en-GB", -DAY), "Yesterday at 14:30");
        assert_eq!(london("en-GB", DAY), "Tomorrow at 14:30");
        assert_eq!(london("fr-FR", -DAY), "Hier à 14:30");
        assert_eq!(london("de-DE", DAY), "Morgen um 14:30");
        assert_eq!(london("ja-JP", -DAY), "昨日 14:30");
        assert_eq!(london("ar-EG", -DAY), "أمس في ٢:٣٠ م");
    }

    #[test]
    fn two_to_six_days_away_is_the_weekday_and_the_time() {
        assert_eq!(london("en-GB", -2 * DAY - 7 * HOUR), "Friday at 07:30");
        assert_eq!(london("en-GB", 2 * DAY), "Tuesday at 14:30");
        assert_eq!(london("fr-FR", 3 * DAY), "Mercredi à 14:30");
        assert_eq!(london("en-GB", -6 * DAY), "Monday at 14:30");
    }

    #[test]
    fn anything_further_is_the_long_date() {
        assert_eq!(london("en-GB", -7 * DAY), "20 September 2026");
        assert_eq!(london("en-US", 30 * DAY), "October 27, 2026");
        assert_eq!(london("de-DE", -40 * DAY), "18. August 2026");
    }

    #[test]
    fn days_are_the_dates_on_the_clock_in_the_zone_shown() {
        // Now is 14:30 in London, 22:30 in Tokyo and 09:30 in New York.
        assert_eq!(said("en-GB", "Europe/London", 6 * HOUR), "Today at 20:30");
        assert_eq!(
            said("en-GB", "Asia/Tokyo", HOUR + 30 * MINUTE),
            "Tomorrow at 00:00"
        );
        assert_eq!(
            said("en-GB", "America/New_York", -15 * HOUR),
            "Yesterday at 18:30"
        );
    }

    #[test]
    fn a_day_that_has_25_hours_is_still_one_day() {
        // London's clocks go back at 02:00 on 2026-10-25; now is 23:30 on
        // the 25th (23:30 UTC) and the value 01:30 the same day, 22 hours
        // earlier in elapsed time but the second 01:30 of the night.
        let now = 1_792_971_000_000; // 2026-10-25T23:30:00Z
        let value = now - 22 * HOUR;
        let got = support::text(format_relative_value(
            &Value::Instant(value),
            &Value::Instant(now),
            &support::zone_named("Europe/London"),
            &support::locale_value("en-GB"),
        ));
        assert_eq!(got, "Today at 01:30");
    }
}
