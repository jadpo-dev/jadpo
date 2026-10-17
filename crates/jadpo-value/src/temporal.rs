//! The `temporal` namespace (area 10, "Zones and local times"), defined
//! once for both executors over the IANA time zone database jiff bundles,
//! so a deployable never reads the platform's zone files and every target
//! answers the same.
//!
//! An `Instant` is milliseconds since 1970-01-01T00:00:00Z, a
//! `CalendarDate` days since 1970-01-01, and a `ZonedTime` an instant with
//! the `Zone` it is seen in ([`Zoned`]). A local time a zone's clocks skip
//! (a gap) or show twice (an overlap) is settled by `dst:`, and without it
//! fails with [`RuleKey::Dst`], which the call's `attempt` maps; a date
//! moved into a month without its day fails with [`RuleKey::InvalidDay`]
//! unless `invalid_day:` says otherwise. A result outside the years 0000
//! to 9999 is [`Fault::TimeRange`].

use jiff::civil;
use jiff::tz::{self, Disambiguation, TimeZone};
use jiff::{Timestamp, Zoned as JiffZoned};

use jadpo_std::{
    DATE_MAX, DATE_MIN, Dst, INSTANT_MAX, INSTANT_MIN, INSTANT_RANGE, InvalidDay, RuleKey, Weekday,
    ZONE, ZONES,
};

use crate::{Fault, Value, Zoned};

const DAY: i64 = 86_400_000;

/// A local date and clock time: days since 1970-01-01 and milliseconds
/// since midnight.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Local {
    pub date: i32,
    pub clock: u32,
}

/// The zone with this place among `Zone`'s variants.
fn zone_at(ordinal: u32) -> Result<TimeZone, Fault> {
    let (_, name) = ZONES.get(ordinal as usize).ok_or(Fault::Shape)?;
    tz::db().get(name).map_err(|_| Fault::Shape)
}

fn timestamp(millis: i64) -> Result<Timestamp, Fault> {
    if !(INSTANT_MIN..=INSTANT_MAX).contains(&millis) {
        return Err(Fault::TimeRange);
    }
    Timestamp::from_millisecond(millis).map_err(|_| Fault::TimeRange)
}

/// An instant within the years an `Instant` holds.
pub fn instant(millis: i64) -> Result<i64, Fault> {
    if (INSTANT_MIN..=INSTANT_MAX).contains(&millis) {
        Ok(millis)
    } else {
        Err(Fault::TimeRange)
    }
}

fn date(day: i64) -> Result<i32, Fault> {
    i32::try_from(day)
        .ok()
        .filter(|day| (DATE_MIN..=DATE_MAX).contains(day))
        .ok_or(Fault::TimeRange)
}

/// A zoned time's local date and clock time, and its offset from UTC in
/// milliseconds; `None` for a zone or instant out of range, which nothing
/// that made one gives.
pub fn local(zoned: Zoned) -> Option<(Local, i64)> {
    let seen = timestamp(zoned.millis)
        .ok()?
        .to_zoned(zone_at(zoned.zone).ok()?);
    let offset = i64::from(seen.offset().seconds()) * 1000;
    let millis = zoned.millis.checked_add(offset)?;
    let day = i32::try_from(millis.div_euclid(DAY)).ok()?;
    let clock = u32::try_from(millis.rem_euclid(DAY)).ok()?;
    Some((Local { date: day, clock }, offset))
}

fn civil_date(day: i32) -> Result<civil::Date, Fault> {
    let (year, month, day) = jadpo_std::civil_date(day);
    civil::Date::new(
        i16::try_from(year).map_err(|_| Fault::TimeRange)?,
        i8::try_from(month).map_err(|_| Fault::TimeRange)?,
        i8::try_from(day).map_err(|_| Fault::TimeRange)?,
    )
    .map_err(|_| Fault::TimeRange)
}

/// The first instant of `day` in `zone`: midnight, or the first time after
/// it the zone's clocks show when they skip midnight.
fn start_of(day: i32, zone: &TimeZone) -> Result<i64, Fault> {
    let start = civil_date(day)?
        .to_zoned(zone.clone())
        .and_then(|zoned| zoned.start_of_day())
        .map_err(|_| Fault::TimeRange)?;
    instant(start.timestamp().as_millisecond())
}

fn disambiguation(dst: Dst) -> Disambiguation {
    match dst {
        Dst::Reject => Disambiguation::Reject,
        Dst::Earlier => Disambiguation::Earlier,
        Dst::Later => Disambiguation::Later,
        Dst::Compatible => Disambiguation::Compatible,
    }
}

/// The instant `clock` on `day` is in zone `zone`, as `dst` settles a gap
/// or an overlap.
pub fn resolve(day: i32, clock: u32, zone: u32, dst: Dst) -> Result<Zoned, Fault> {
    let tz = zone_at(zone)?;
    let date = civil_date(day)?;
    let (hour, minute) = (clock / 3_600_000, clock / 60_000 % 60);
    let (second, millis) = (clock / 1000 % 60, clock % 1000);
    let time = civil::Time::new(
        i8::try_from(hour).map_err(|_| Fault::Shape)?,
        i8::try_from(minute).map_err(|_| Fault::Shape)?,
        i8::try_from(second).map_err(|_| Fault::Shape)?,
        i32::try_from(millis * 1_000_000).map_err(|_| Fault::Shape)?,
    )
    .map_err(|_| Fault::Shape)?;
    let ambiguous = tz.to_ambiguous_zoned(date.to_datetime(time));
    if ambiguous.is_ambiguous() && dst == Dst::Reject {
        return Err(Fault::Broke(RuleKey::Dst));
    }
    let resolved: JiffZoned = ambiguous
        .disambiguate(disambiguation(dst))
        .map_err(|_| Fault::TimeRange)?;
    Ok(Zoned {
        millis: instant(resolved.timestamp().as_millisecond())?,
        zone,
    })
}

/// A date or a zoned time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dated {
    Date(i32),
    Zoned(Zoned),
}

impl Dated {
    pub fn of(value: &Value) -> Result<Dated, Fault> {
        match value {
            Value::Date(day) => Ok(Dated::Date(*day)),
            Value::Zoned(zoned) => Ok(Dated::Zoned(*zoned)),
            _ => Err(Fault::Shape),
        }
    }

    pub fn value(self) -> Value {
        match self {
            Dated::Date(day) => Value::Date(day),
            Dated::Zoned(zoned) => Value::Zoned(zoned),
        }
    }

    /// Its local date, and for a zoned time its local clock time.
    fn local(self) -> Result<(i32, Option<u32>), Fault> {
        match self {
            Dated::Date(day) => Ok((day, None)),
            Dated::Zoned(zoned) => {
                let (local, _) = local(zoned).ok_or(Fault::TimeRange)?;
                Ok((local.date, Some(local.clock)))
            }
        }
    }

    /// The same kind of value on `day`, at the same clock time in the same
    /// zone for a zoned time.
    fn moved(self, day: i32, dst: Dst) -> Result<Dated, Fault> {
        match self {
            Dated::Date(_) => Ok(Dated::Date(day)),
            Dated::Zoned(zoned) => {
                let (local, _) = local(zoned).ok_or(Fault::TimeRange)?;
                resolve(day, local.clock, zoned.zone, dst).map(Dated::Zoned)
            }
        }
    }
}

/// `temporal.in_zone`.
pub fn in_zone(millis: i64, zone: u32) -> Result<Zoned, Fault> {
    zone_at(zone)?;
    Ok(Zoned {
        millis: instant(millis)?,
        zone,
    })
}

/// `temporal.between`: `end - start`.
pub fn between(start: i64, end: i64) -> Result<i64, Fault> {
    end.checked_sub(start).ok_or(Fault::IntOverflow)
}

/// `temporal.add_days`: whole days on the calendar, the clock time kept.
pub fn add_days(value: Dated, days: i64, dst: Dst) -> Result<Dated, Fault> {
    let (day, _) = value.local()?;
    let moved = i64::from(day).checked_add(days).ok_or(Fault::TimeRange)?;
    value.moved(date(moved)?, dst)
}

/// `temporal.add_months`: whole months on the calendar, the day and the
/// clock time kept, and a day the month doesn't have settled by
/// `invalid_day`.
pub fn add_months(
    value: Dated,
    months: i64,
    invalid_day: InvalidDay,
    dst: Dst,
) -> Result<Dated, Fault> {
    let (day, _) = value.local()?;
    let (year, month, day_of_month) = jadpo_std::civil_date(day);
    let total = (year * 12 + month - 1)
        .checked_add(months)
        .ok_or(Fault::TimeRange)?;
    let (year, month) = (total.div_euclid(12), total.rem_euclid(12) + 1);
    let last = jadpo_std::days_in_month(year, month);
    let day_of_month = if day_of_month > last {
        match invalid_day {
            InvalidDay::Reject => return Err(Fault::Broke(RuleKey::InvalidDay)),
            InvalidDay::LastValidDay => last,
        }
    } else {
        day_of_month
    };
    let moved = jadpo_std::epoch_day(year, month, day_of_month).ok_or(Fault::TimeRange)?;
    value.moved(moved, dst)
}

/// `temporal.add_years`: twelve months each.
pub fn add_years(
    value: Dated,
    years: i64,
    invalid_day: InvalidDay,
    dst: Dst,
) -> Result<Dated, Fault> {
    let months = years.checked_mul(12).ok_or(Fault::TimeRange)?;
    add_months(value, months, invalid_day, dst)
}

/// The year, month and day of a date or of a zoned time's local date.
pub fn civil(value: Dated) -> Result<(i64, i64, i64), Fault> {
    let (day, _) = value.local()?;
    Ok(jadpo_std::civil_date(day))
}

/// The day of the week, from Monday at 0.
pub fn weekday(value: Dated) -> Result<u32, Fault> {
    let (day, _) = value.local()?;
    // 1970-01-01 was a Thursday.
    u32::try_from((i64::from(day) + 3).rem_euclid(7)).map_err(|_| Fault::Shape)
}

/// A zoned time's local clock time.
pub fn clock(zoned: Zoned) -> Result<u32, Fault> {
    local(zoned)
        .map(|(local, _)| local.clock)
        .ok_or(Fault::TimeRange)
}

/// A zoned time's local date.
pub fn local_date(zoned: Zoned) -> Result<i32, Fault> {
    local(zoned)
        .map(|(local, _)| local.date)
        .ok_or(Fault::TimeRange)
}

/// What a calendar period's bounds are made from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Period {
    Day,
    /// A week starting on this day, from Monday at 0.
    Week(u32),
    Month,
    Year,
}

/// The period around `day` in `zone`, from its first instant up to but not
/// including the next period's first, so a day has 23, 24 or 25 hours as
/// the zone's clocks say.
pub fn bounds(day: i32, zone: u32, period: Period) -> Result<(i64, i64), Fault> {
    let tz = zone_at(zone)?;
    let (year, month, _) = jadpo_std::civil_date(day);
    let (first, next) = match period {
        Period::Day => (i64::from(day), i64::from(day) + 1),
        Period::Week(starts_on) => {
            let back = (i64::from(weekday(Dated::Date(day))?) - i64::from(starts_on)).rem_euclid(7);
            let first = i64::from(day) - back;
            (first, first + 7)
        }
        Period::Month => {
            let first = jadpo_std::epoch_day(year, month, 1).ok_or(Fault::TimeRange)?;
            let next = jadpo_std::epoch_day(year + month / 12, month % 12 + 1, 1)
                .ok_or(Fault::TimeRange)?;
            (i64::from(first), i64::from(next))
        }
        Period::Year => {
            let first = jadpo_std::epoch_day(year, 1, 1).ok_or(Fault::TimeRange)?;
            let next = jadpo_std::epoch_day(year + 1, 1, 1).ok_or(Fault::TimeRange)?;
            (i64::from(first), i64::from(next))
        }
    };
    Ok((start_of(date(first)?, &tz)?, start_of(date(next)?, &tz)?))
}

/// An `InstantRange` as the runtime holds it.
pub fn range(start: i64, end: i64) -> Value {
    Value::record(
        INSTANT_RANGE,
        vec![
            ("start".into(), Value::Instant(start)),
            ("end".into(), Value::Instant(end)),
        ],
    )
}

/// The `Zone` value with this place among its variants.
pub fn zone_value(ordinal: u32) -> Value {
    let name = ZONES
        .get(ordinal as usize)
        .map_or("utc", |(variant, _)| *variant);
    Value::variant(ZONE, ordinal, name, Vec::new())
}

/// A `Zone` value's place among its variants.
pub fn zone_of(value: &Value) -> Result<u32, Fault> {
    match value {
        Value::Variant(variant) if &*variant.ty == ZONE => Ok(variant.ordinal),
        _ => Err(Fault::Shape),
    }
}

/// The `Weekday` value from Monday at 0.
pub fn weekday_value(ordinal: u32) -> Value {
    let name = Weekday::NAMES
        .get(ordinal as usize)
        .copied()
        .unwrap_or("monday");
    Value::variant(Weekday::NAME, ordinal, name, Vec::new())
}

/// A `Weekday` value from Monday at 0.
pub fn weekday_of(value: &Value) -> Result<u32, Fault> {
    match value {
        Value::Variant(variant) if &*variant.ty == Weekday::NAME => Ok(variant.ordinal),
        _ => Err(Fault::Shape),
    }
}

/// `dst:` as given, `reject` when left out.
pub fn dst_of(value: &Value) -> Result<Dst, Fault> {
    match value {
        Value::None => Ok(Dst::Reject),
        Value::Variant(variant) => Dst::from_name(&variant.name).ok_or(Fault::Shape),
        _ => Err(Fault::Shape),
    }
}

/// `invalid_day:` as given, `reject` when left out.
pub fn invalid_day_of(value: &Value) -> Result<InvalidDay, Fault> {
    match value {
        Value::None => Ok(InvalidDay::Reject),
        Value::Variant(variant) => InvalidDay::from_name(&variant.name).ok_or(Fault::Shape),
        _ => Err(Fault::Shape),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zone(name: &str) -> u32 {
        jadpo_std::variant_ordinal(ZONE, name).expect("a zone")
    }

    fn day(text: &str) -> i32 {
        jadpo_std::date_number(text).expect("a date")
    }

    fn at(text: &str) -> i64 {
        jadpo_std::instant_millis(text).expect("an instant")
    }

    fn clock_of(text: &str) -> u32 {
        jadpo_std::clock_millis(text).expect("a clock time")
    }

    #[test]
    fn every_zone_is_in_the_bundled_database() {
        for (ordinal, (variant, name)) in ZONES.iter().enumerate() {
            let ordinal = u32::try_from(ordinal).expect("few zones");
            assert!(zone_at(ordinal).is_ok(), "{variant} ({name}) is bundled");
        }
    }

    #[test]
    fn a_local_time_in_a_gap_or_an_overlap_needs_dst() {
        let london = zone("europe_london");
        // Clocks go back at 02:00 BST on 25 October 2026: 01:30 happens twice.
        let back = day("2026-10-25");
        assert_eq!(
            resolve(back, clock_of("01:30"), london, Dst::Reject),
            Err(Fault::Broke(RuleKey::Dst))
        );
        let earlier = resolve(back, clock_of("01:30"), london, Dst::Earlier).expect("earlier");
        let later = resolve(back, clock_of("01:30"), london, Dst::Later).expect("later");
        assert_eq!(earlier.millis, at("2026-10-25T00:30:00Z"));
        assert_eq!(later.millis, at("2026-10-25T01:30:00Z"));
        assert_eq!(
            resolve(back, clock_of("01:30"), london, Dst::Compatible).map(|z| z.millis),
            Ok(earlier.millis)
        );
        // Clocks go forward at 01:00 GMT on 29 March 2026: 01:30 never happens.
        let forward = day("2026-03-29");
        assert_eq!(
            resolve(forward, clock_of("01:30"), london, Dst::Reject),
            Err(Fault::Broke(RuleKey::Dst))
        );
        assert_eq!(
            resolve(forward, clock_of("01:30"), london, Dst::Compatible).map(|z| z.millis),
            Ok(at("2026-03-29T01:30:00Z"))
        );
        assert_eq!(
            resolve(forward, clock_of("01:30"), london, Dst::Earlier).map(|z| z.millis),
            Ok(at("2026-03-29T00:30:00Z"))
        );
        // An ordinary time is the same whatever `dst` says.
        assert_eq!(
            resolve(back, clock_of("09:30"), london, Dst::Reject).map(|z| z.millis),
            Ok(at("2026-10-25T09:30:00Z"))
        );
    }

    #[test]
    fn days_across_a_clock_change_keep_the_clock_time() {
        let london = zone("europe_london");
        let before = resolve(day("2026-10-24"), clock_of("09:00"), london, Dst::Reject)
            .expect("an ordinary time");
        let after = add_days(Dated::Zoned(before), 1, Dst::Reject).expect("a day on");
        let Dated::Zoned(after) = after else {
            panic!("a zoned time");
        };
        // 09:00 BST, then 09:00 GMT: 25 hours apart.
        assert_eq!(after.millis - before.millis, 25 * 3_600_000);
        assert_eq!(clock(after), Ok(clock_of("09:00")));
        let into_gap = resolve(day("2026-03-28"), clock_of("01:30"), london, Dst::Reject)
            .expect("an ordinary time");
        assert_eq!(
            add_days(Dated::Zoned(into_gap), 1, Dst::Reject),
            Err(Fault::Broke(RuleKey::Dst))
        );
    }

    #[test]
    fn months_settle_a_missing_day() {
        let end_of_january = Dated::Date(day("2026-01-31"));
        assert_eq!(
            add_months(end_of_january, 1, InvalidDay::Reject, Dst::Reject),
            Err(Fault::Broke(RuleKey::InvalidDay))
        );
        assert_eq!(
            add_months(end_of_january, 1, InvalidDay::LastValidDay, Dst::Reject),
            Ok(Dated::Date(day("2026-02-28")))
        );
        assert_eq!(
            add_months(end_of_january, -2, InvalidDay::Reject, Dst::Reject),
            Err(Fault::Broke(RuleKey::InvalidDay))
        );
        assert_eq!(
            add_years(
                Dated::Date(day("2024-02-29")),
                1,
                InvalidDay::LastValidDay,
                Dst::Reject
            ),
            Ok(Dated::Date(day("2025-02-28")))
        );
        assert_eq!(
            add_months(
                Dated::Date(day("2026-11-15")),
                3,
                InvalidDay::Reject,
                Dst::Reject
            ),
            Ok(Dated::Date(day("2027-02-15")))
        );
    }

    #[test]
    fn bounds_follow_the_zone_s_clocks() {
        let london = zone("europe_london");
        let (start, end) = bounds(day("2026-10-25"), london, Period::Day).expect("a day");
        assert_eq!(start, at("2026-10-24T23:00:00Z"));
        assert_eq!(end, at("2026-10-26T00:00:00Z"));
        assert_eq!(end - start, 25 * 3_600_000);
        let (start, end) = bounds(day("2026-03-29"), london, Period::Day).expect("a day");
        assert_eq!(end - start, 23 * 3_600_000);
        // Santiago skips midnight on 6 September 2026: the day starts at 01:00.
        let santiago = zone("america_santiago");
        let (start, _) = bounds(day("2026-09-06"), santiago, Period::Day).expect("a day");
        assert_eq!(start, at("2026-09-06T04:00:00Z"));
        // Thursday 15 October 2026's Monday week.
        let (start, end) = bounds(day("2026-10-15"), london, Period::Week(0)).expect("a week");
        assert_eq!(start, at("2026-10-11T23:00:00Z"));
        assert_eq!(end, at("2026-10-18T23:00:00Z"));
        let (start, end) = bounds(day("2026-12-15"), london, Period::Month).expect("a month");
        assert_eq!(start, at("2026-12-01T00:00:00Z"));
        assert_eq!(end, at("2027-01-01T00:00:00Z"));
        let (start, end) = bounds(day("2026-06-01"), london, Period::Year).expect("a year");
        assert_eq!(start, at("2026-01-01T00:00:00Z"));
        assert_eq!(end, at("2027-01-01T00:00:00Z"));
    }

    #[test]
    fn local_parts_read_through_the_zone() {
        let tokyo = zone("asia_tokyo");
        let zoned = in_zone(at("2026-12-31T20:00:00Z"), tokyo).expect("in range");
        assert_eq!(civil(Dated::Zoned(zoned)), Ok((2027, 1, 1)));
        assert_eq!(clock(zoned), Ok(clock_of("05:00")));
        // 1 January 2027 is a Friday.
        assert_eq!(weekday(Dated::Zoned(zoned)), Ok(4));
        assert_eq!(zoned.to_string(), "2027-01-01T05:00:00+09:00[Asia/Tokyo]");
        assert_eq!(in_zone(INSTANT_MAX + 1, tokyo), Err(Fault::TimeRange));
    }
}
