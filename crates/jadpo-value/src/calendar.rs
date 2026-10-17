//! The time group's values (area 19, "Time and calendar additions", chunk
//! 5.5k), once for both executors: a `LocalTime`, `LocalDateTime`,
//! `YearMonth`, `Year`, `Quarter`, `IsoWeek`, `UtcOffset` and
//! `CalendarPeriod` are records of their data entries' fields, each read
//! from and written as its ISO 8601 text here, and `CalendarPeriod` has
//! its operator rows and `temporal.period_between`.
//!
//! A date moved by a period takes its years, then its months, then its
//! days, and a day past a month's end becomes its last day, as area 10's
//! `temporal.add_months` does with `InvalidDay.last_valid_day`.

use std::rc::Rc;

use jadpo_std::standard::TextForm;

use crate::{Fault, Value};

/// A whole-number field of a time record.
fn whole(value: &Value, name: &str) -> Result<i64, Fault> {
    match value {
        Value::Record(record) => match record.get(name) {
            Some(Value::Int(n)) => Ok(*n),
            _ => Err(Fault::Shape),
        },
        _ => Err(Fault::Shape),
    }
}

fn record(ty: &str, fields: &[(&str, Value)]) -> Value {
    Value::record(
        ty,
        fields
            .iter()
            .map(|(name, value)| (Rc::<str>::from(*name), value.clone()))
            .collect(),
    )
}

/// A `LocalTime` of its hour, minute and second.
pub fn local_time(hour: i64, minute: i64, second: i64) -> Value {
    record(
        "LocalTime",
        &[
            ("hour", Value::Int(hour)),
            ("minute", Value::Int(minute)),
            ("second", Value::Int(second)),
        ],
    )
}

/// A `LocalTime`'s milliseconds since midnight, as a clock time is held.
pub fn clock_of(value: &Value) -> Result<u32, Fault> {
    let millis =
        (whole(value, "hour")? * 3600 + whole(value, "minute")? * 60 + whole(value, "second")?)
            * 1000;
    u32::try_from(millis).map_err(|_| Fault::Shape)
}

/// A `LocalDateTime`'s date and clock time.
pub fn local_date_time(value: &Value) -> Result<(i32, u32), Fault> {
    let Value::Record(record) = value else {
        return Err(Fault::Shape);
    };
    match (record.get("date"), record.get("time")) {
        (Some(Value::Date(day)), Some(time)) => Ok((*day, clock_of(time)?)),
        _ => Err(Fault::Shape),
    }
}

/// A `CalendarPeriod` of its years, months and days.
pub fn period(years: i64, months: i64, days: i64) -> Value {
    record(
        jadpo_std::CALENDAR_PERIOD,
        &[
            ("years", Value::Int(years)),
            ("months", Value::Int(months)),
            ("days", Value::Int(days)),
        ],
    )
}

/// A `CalendarPeriod`'s years, months and days.
pub fn period_parts(value: &Value) -> Result<(i64, i64, i64), Fault> {
    Ok((
        whole(value, "years")?,
        whole(value, "months")?,
        whole(value, "days")?,
    ))
}

/// Whether `value` is a `CalendarPeriod`.
pub fn is_period(value: &Value) -> bool {
    matches!(value, Value::Record(record) if &*record.ty == jadpo_std::CALENDAR_PERIOD)
}

/// A value's canonical text, by its type's text form; `None` for a form
/// this module doesn't write.
pub fn canonical(form: TextForm, value: &Value) -> Option<Result<String, Fault>> {
    let text = || -> Result<String, Fault> {
        Ok(match form {
            TextForm::LocalTime => clock_text(clock_of(value)?),
            TextForm::LocalDateTime => {
                let (day, clock) = local_date_time(value)?;
                format!("{}T{}", jadpo_std::format_date(day), clock_text(clock))
            }
            TextForm::YearMonth => {
                format!("{:04}-{:02}", whole(value, "year")?, whole(value, "month")?)
            }
            TextForm::Year => format!("{:04}", whole(value, "year")?),
            TextForm::Quarter => {
                format!("{:04}-Q{}", whole(value, "year")?, whole(value, "quarter")?)
            }
            TextForm::IsoWeek => {
                format!("{:04}-W{:02}", whole(value, "year")?, whole(value, "week")?)
            }
            TextForm::UtcOffset => {
                let minutes = whole(value, "minutes")?;
                let sign = if minutes < 0 { '-' } else { '+' };
                let size = minutes.unsigned_abs();
                format!("{sign}{:02}:{:02}", size / 60, size % 60)
            }
            TextForm::Period => period_text(period_parts(value)?),
            _ => return Err(Fault::Shape),
        })
    };
    match form {
        TextForm::LocalTime
        | TextForm::LocalDateTime
        | TextForm::YearMonth
        | TextForm::Year
        | TextForm::Quarter
        | TextForm::IsoWeek
        | TextForm::UtcOffset
        | TextForm::Period => Some(text()),
        _ => None,
    }
}

/// `09:30:00`.
fn clock_text(millis: u32) -> String {
    let seconds = millis / 1000;
    format!(
        "{:02}:{:02}:{:02}",
        seconds / 3600,
        seconds / 60 % 60,
        seconds % 60
    )
}

/// `P1Y2M10D`, each part that isn't zero with its own sign, `P0D` for
/// none.
fn period_text((years, months, days): (i64, i64, i64)) -> String {
    let mut text = String::from("P");
    for (amount, unit) in [(years, 'Y'), (months, 'M'), (days, 'D')] {
        if amount != 0 {
            text.push_str(&format!("{amount}{unit}"));
        }
    }
    if text.len() == 1 {
        text.push_str("0D");
    }
    text
}

/// A value of the type called `ty` read from its canonical text, by its
/// text form; `None` when the text isn't one. Only the form the type
/// writes is read, as for every standard type.
pub fn read(form: TextForm, raw: &str) -> Option<Option<Value>> {
    Some(match form {
        TextForm::LocalTime => clock(raw).map(|(h, m, s)| local_time(h, m, s)),
        TextForm::LocalDateTime => raw.split_once('T').and_then(|(date, time)| {
            let day = jadpo_std::date_number(date)?;
            let (h, m, s) = clock(time)?;
            Some(record(
                "LocalDateTime",
                &[("date", Value::Date(day)), ("time", local_time(h, m, s))],
            ))
        }),
        TextForm::YearMonth => raw.split_once('-').and_then(|(year, month)| {
            let (year, month) = (year_of(year)?, digits(month, 2)?);
            (1..=12).contains(&month).then(|| {
                record(
                    "YearMonth",
                    &[("year", Value::Int(year)), ("month", Value::Int(month))],
                )
            })
        }),
        TextForm::Year => year_of(raw).map(|year| record("Year", &[("year", Value::Int(year))])),
        TextForm::Quarter => raw.split_once("-Q").and_then(|(year, quarter)| {
            let (year, quarter) = (year_of(year)?, digits(quarter, 1)?);
            (1..=4).contains(&quarter).then(|| {
                record(
                    "Quarter",
                    &[("year", Value::Int(year)), ("quarter", Value::Int(quarter))],
                )
            })
        }),
        TextForm::IsoWeek => raw.split_once("-W").and_then(|(year, week)| {
            let (year, week) = (year_of(year)?, digits(week, 2)?);
            (1..=weeks_in(year)?).contains(&week).then(|| {
                record(
                    "IsoWeek",
                    &[("year", Value::Int(year)), ("week", Value::Int(week))],
                )
            })
        }),
        TextForm::UtcOffset => {
            offset(raw).map(|minutes| record("UtcOffset", &[("minutes", Value::Int(minutes))]))
        }
        TextForm::Period => period_of(raw).map(|(y, m, d)| period(y, m, d)),
        _ => return None,
    })
}

/// Exactly `width` ASCII digits as a number.
fn digits(text: &str, width: usize) -> Option<i64> {
    (text.len() == width && text.bytes().all(|b| b.is_ascii_digit()))
        .then(|| text.parse().ok())
        .flatten()
}

/// A four-digit year, `0000` to `9999`.
fn year_of(text: &str) -> Option<i64> {
    digits(text, 4)
}

/// `HH:MM:SS`, from `00:00:00` to `23:59:59`.
fn clock(text: &str) -> Option<(i64, i64, i64)> {
    let mut parts = text.split(':');
    let (hour, minute, second) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() {
        return None;
    }
    let (hour, minute, second) = (digits(hour, 2)?, digits(minute, 2)?, digits(second, 2)?);
    (hour < 24 && minute < 60 && second < 60).then_some((hour, minute, second))
}

/// `+HH:MM` or `-HH:MM`, within 18 hours of UTC; UTC itself is `+00:00`
/// only.
fn offset(text: &str) -> Option<i64> {
    let (negative, rest) = match text.as_bytes().first()? {
        b'+' => (false, text.get(1..)?),
        b'-' => (true, text.get(1..)?),
        _ => return None,
    };
    let (hours, minutes) = rest.split_once(':')?;
    let (hours, minutes) = (digits(hours, 2)?, digits(minutes, 2)?);
    let total = hours * 60 + minutes;
    if minutes >= 60 || total > 18 * 60 || (negative && total == 0) {
        return None;
    }
    Some(if negative { -total } else { total })
}

/// `P1Y2M10D` as written: `P`, then each part that isn't zero in the
/// order years, months, days, with `-` before a negative one, or `P0D`.
fn period_of(text: &str) -> Option<(i64, i64, i64)> {
    if text == "P0D" {
        return Some((0, 0, 0));
    }
    let mut rest = text.strip_prefix('P')?;
    let mut parts = [0i64; 3];
    let mut next = 0;
    if rest.is_empty() {
        return None;
    }
    while !rest.is_empty() {
        let negative = rest.starts_with('-');
        let body = if negative { rest.get(1..)? } else { rest };
        let length = body.bytes().take_while(u8::is_ascii_digit).count();
        let (number, after) = (body.get(..length)?, body.get(length..)?);
        let unit = after.chars().next()?;
        let at = ["Y", "M", "D"]
            .iter()
            .position(|written| unit.to_string() == *written)?;
        // Each part once, in order, and never zero (a zero part isn't
        // written), so one period has one text.
        if at < next || number.is_empty() || (number.starts_with('0')) {
            return None;
        }
        let amount: i64 = number.parse().ok()?;
        *parts.get_mut(at)? = if negative {
            amount.checked_neg()?
        } else {
            amount
        };
        next = at + 1;
        rest = after.get(1..)?;
    }
    let [years, months, days] = parts;
    Some((years, months, days))
}

/// How many weeks ISO week-numbering year `year` has: 53 when it starts
/// on a Thursday, or on a Wednesday in a leap year, and 52 otherwise.
fn weeks_in(year: i64) -> Option<i64> {
    let first = jadpo_std::epoch_day(year, 1, 1)?;
    // 1970-01-01 was a Thursday; Monday is 0.
    let weekday = (i64::from(first) + 3).rem_euclid(7);
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    Some(if weekday == 3 || (leap && weekday == 2) {
        53
    } else {
        52
    })
}

/// `date + period` (the operator table): its years, then its months,
/// then its days, a day past a month's end becoming its last day.
pub fn add_period(day: i32, (years, months, days): (i64, i64, i64)) -> Result<i32, Fault> {
    let day = shift_months(day, years.checked_mul(12).ok_or(Fault::TimeRange)?)?;
    let day = shift_months(day, months)?;
    let moved = i64::from(day).checked_add(days).ok_or(Fault::TimeRange)?;
    i32::try_from(moved)
        .ok()
        .filter(|day| (jadpo_std::DATE_MIN..=jadpo_std::DATE_MAX).contains(day))
        .ok_or(Fault::TimeRange)
}

/// `day` moved by whole months, the day of the month kept, or the month's
/// last day where it has fewer.
fn shift_months(day: i32, months: i64) -> Result<i32, Fault> {
    if months == 0 {
        return Ok(day);
    }
    let (year, month, day_of_month) = jadpo_std::civil_date(day);
    let total = (year * 12 + month - 1)
        .checked_add(months)
        .ok_or(Fault::TimeRange)?;
    let (year, month) = (total.div_euclid(12), total.rem_euclid(12) + 1);
    let day_of_month = day_of_month.min(jadpo_std::days_in_month(year, month));
    jadpo_std::epoch_day(year, month, day_of_month).ok_or(Fault::TimeRange)
}

/// A period with each part negated.
fn negated((years, months, days): (i64, i64, i64)) -> Result<(i64, i64, i64), Fault> {
    let negate = |n: i64| n.checked_neg().ok_or(Fault::IntOverflow);
    Ok((negate(years)?, negate(months)?, negate(days)?))
}

/// `date - period`: the period with each part negated, added.
pub fn subtract_period(day: i32, parts: (i64, i64, i64)) -> Result<i32, Fault> {
    add_period(day, negated(parts)?)
}

/// `a + b` or `a - b` on two periods, part by part.
pub fn combine(a: &Value, b: &Value, subtract: bool) -> Result<Value, Fault> {
    let ((ay, am, ad), b) = (period_parts(a)?, period_parts(b)?);
    let (by, bm, bd) = if subtract { negated(b)? } else { b };
    let add = |x: i64, y: i64| x.checked_add(y).ok_or(Fault::IntOverflow);
    Ok(period(add(ay, by)?, add(am, bm)?, add(ad, bd)?))
}

/// `period * n`: each part multiplied.
pub fn times(value: &Value, n: i64) -> Result<Value, Fault> {
    let (years, months, days) = period_parts(value)?;
    let times = |x: i64| x.checked_mul(n).ok_or(Fault::IntOverflow);
    Ok(period(times(years)?, times(months)?, times(days)?))
}

/// `temporal.period_between(start, end)`: whole years, months and days
/// from `start` to `end`, so that `start` plus the period is `end`, every
/// part of the one sign, negative when `end` is earlier.
///
/// A month counts once the end's day of the month reaches the start's,
/// as PostgreSQL's `age` and `java.time` count them, so 30 January to 28
/// February is 29 days; fewer are taken where the operator's clamping
/// would pass `end`, and the days are what is left, exactly.
pub fn period_between(start: i32, end: i32) -> Result<Value, Fault> {
    let (sy, sm, sd) = jadpo_std::civil_date(start);
    let (ey, em, ed) = jadpo_std::civil_date(end);
    let forward = end >= start;
    let mut months = (ey * 12 + em) - (sy * 12 + sm);
    if forward && ed < sd {
        months -= 1;
    } else if !forward && ed > sd {
        months += 1;
    }
    let reach = |months: i64| -> Result<i32, Fault> {
        // Years first, then months, as the operator adds them.
        let (years, rest) = (months / 12, months % 12);
        add_period(start, (years, rest, 0))
    };
    let passes = |reached: i32| {
        if forward {
            reached > end
        } else {
            reached < end
        }
    };
    // The month count of `end` itself may pass it by its day; one fewer
    // never does.
    while months != 0 && passes(reach(months)?) {
        months += if forward { -1 } else { 1 };
    }
    let reached = reach(months)?;
    let days = i64::from(end) - i64::from(reached);
    Ok(period(months / 12, months % 12, days))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(text: &str) -> i32 {
        jadpo_std::date_number(text).unwrap()
    }

    fn between(start: &str, end: &str) -> (i64, i64, i64) {
        period_parts(&period_between(day(start), day(end)).unwrap()).unwrap()
    }

    #[test]
    fn a_period_between_two_dates_adds_back_to_the_end() {
        assert_eq!(between("2026-01-31", "2026-03-01"), (0, 1, 1));
        assert_eq!(between("2026-01-30", "2026-02-28"), (0, 0, 29));
        assert_eq!(between("2028-01-29", "2028-03-01"), (0, 1, 1));
        assert_eq!(between("2026-03-31", "2026-02-28"), (0, -1, 0));
        assert_eq!(between("2026-03-01", "2026-01-31"), (0, -1, -1));
        assert_eq!(between("2028-02-29", "2029-03-29"), (1, 1, 1));
        assert_eq!(between("2026-10-25", "2026-10-25"), (0, 0, 0));
        assert_eq!(between("1990-07-14", "2026-10-17"), (36, 3, 3));
        // Every pair over three years adds back to its end.
        let first = day("2027-12-01");
        for start in first..first + 800 {
            for end in (first..first + 800).step_by(7) {
                let parts = period_parts(&period_between(start, end).unwrap()).unwrap();
                assert_eq!(add_period(start, parts), Ok(end), "{start} to {end}");
                let signs = [parts.0.signum(), parts.1.signum(), parts.2.signum()];
                assert!(
                    signs.iter().all(|s| *s >= 0) || signs.iter().all(|s| *s <= 0),
                    "{parts:?}"
                );
            }
        }
    }

    #[test]
    fn a_period_moves_a_date_by_years_then_months_then_days() {
        assert_eq!(
            add_period(day("2026-01-31"), (0, 1, 0)),
            Ok(day("2026-02-28"))
        );
        assert_eq!(
            add_period(day("2028-01-31"), (0, 1, 0)),
            Ok(day("2028-02-29"))
        );
        assert_eq!(
            add_period(day("2028-02-29"), (1, 0, 0)),
            Ok(day("2029-02-28"))
        );
        assert_eq!(
            add_period(day("2028-02-29"), (1, 1, 0)),
            Ok(day("2029-03-28"))
        );
        assert_eq!(
            subtract_period(day("2026-03-31"), (0, 1, 0)),
            Ok(day("2026-02-28"))
        );
        assert_eq!(
            add_period(day("9999-12-31"), (0, 0, 1)),
            Err(Fault::TimeRange)
        );
    }

    #[test]
    fn texts_read_back_as_they_are_written() {
        for (form, text) in [
            (TextForm::LocalTime, "09:30:05"),
            (TextForm::LocalDateTime, "2026-10-25T01:30:00"),
            (TextForm::YearMonth, "0001-01"),
            (TextForm::Year, "0000"),
            (TextForm::Quarter, "9999-Q4"),
            (TextForm::IsoWeek, "2020-W53"),
            (TextForm::UtcOffset, "-09:30"),
            (TextForm::UtcOffset, "+14:00"),
            (TextForm::Period, "P-1Y2M-3D"),
            (TextForm::Period, "P10D"),
        ] {
            let value = read(form, text)
                .unwrap()
                .unwrap_or_else(|| panic!("{text}"));
            assert_eq!(canonical(form, &value).unwrap().as_deref(), Ok(text));
        }
        assert_eq!(read(TextForm::IsoWeek, "2021-W53"), Some(None));
        assert_eq!(read(TextForm::Period, "P01M"), Some(None));
        assert_eq!(read(TextForm::Period, "P-0D"), Some(None));
        assert!(read(TextForm::UtcOffset, "+18:00").flatten().is_some());
    }
}
