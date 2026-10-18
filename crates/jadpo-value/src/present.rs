//! Text for people (area 10, "Showing dates and times to people"):
//! the time types' `text.format` and `temporal.format_relative` over locale data, so
//! the interpreter, a native deployable and WebAssembly on Workers write
//! the same text for the same locale.
//!
//! The locale data is ICU4X's, with Unicode CLDR compiled into the binary
//! and nothing read from the platform. A date is always shown in the
//! Gregorian calendar and in the locale's own words, order, punctuation
//! and digits, as `Intl.DateTimeFormat` with `calendar: "gregory"` shows it.
//!
//! A format reaches here as [`TimeFormatSpec`]. A built-in format is
//! CLDR's date, time or combined pattern at one of four lengths. A
//! declared format lists the parts it shows: the nearest of CLDR's
//! patterns for those parts is taken from the locale, and each part's
//! width is then set to the style asked for (`month: short` is `MMM`,
//! `day: two_digit` is `dd`), which is how a pattern generator adjusts the
//! pattern it matched. Parts no pattern holds together, a minute without
//! an hour or a year with a day, are shown on their own and written one
//! after another.

use icu_calendar::{Date, Gregorian};
use icu_datetime::fieldsets::builder::{DateFields, FieldSetBuilder, ZoneStyle};
use icu_datetime::fieldsets::enums::CompositeFieldSet;
use icu_datetime::input::ZonedDateTime;
use icu_datetime::options::{Length, TimePrecision, YearStyle};
use icu_datetime::pattern::{DateTimePattern, FixedCalendarDateTimeNames};
use icu_datetime::{DateTimeFormatterPreferences, FixedCalendarDateTimeFormatter};
use icu_locale::Locale;
use icu_time::zone::{IanaParser, UtcOffset, models};
use icu_time::{DateTime, Time, TimeZone, TimeZoneInfo};
use jadpo_std::{
    BuiltinFormat, Component, ComponentStyle, FormatLength, FormatPart, TimeFormatSpec, ZONES,
};
use writeable::{TryWriteable, Writeable};

use crate::{Fault, Value, Zoned, temporal};

pub(crate) mod contact;
mod message;
pub mod money;
pub(crate) mod names;
pub mod pseudo;
mod relative;
pub mod units;

pub use message::{number, translate};
pub use relative::format_relative;

type Zdt = ZonedDateTime<Gregorian, TimeZoneInfo<models::AtTime>>;

/// What is shown: a local date and clock time, and where it is seen when
/// it has a zone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Moment {
    /// Days since 1970-01-01.
    pub date: i32,
    /// Milliseconds since midnight.
    pub clock: u32,
    pub zone: Option<Placed>,
}

/// A zone as it stands at the moment shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placed {
    /// The zone's place among `Zone`'s variants.
    pub zone: u32,
    /// Its offset from UTC, in seconds; ICU4X tells from it and the date
    /// whether the zone is on daylight-saving time.
    pub offset: i32,
}

/// The moment a value is shown at: an `Instant` in the zone given, a
/// `ZonedTime` in its own, a `CalendarDate` with none.
fn moment_of(value: &Value, zone: &Value) -> Result<Moment, Fault> {
    let zoned = match value {
        Value::Date(day) => {
            return Ok(Moment {
                date: *day,
                clock: 0,
                zone: None,
            });
        }
        Value::Instant(millis) => Zoned {
            millis: *millis,
            zone: temporal::zone_of(zone)?,
        },
        Value::Zoned(zoned) => *zoned,
        // A local date and time, or a clock time on no day in particular,
        // which the checker shows only by a format of the parts it has
        // (area 19, "Time and calendar additions").
        Value::Record(record) if &*record.ty == "LocalDateTime" => {
            let (date, clock) = crate::calendar::local_date_time(value)?;
            return Ok(Moment {
                date,
                clock,
                zone: None,
            });
        }
        Value::Record(record) if &*record.ty == "LocalTime" => {
            return Ok(Moment {
                date: 0,
                clock: crate::calendar::clock_of(value)?,
                zone: None,
            });
        }
        _ => return Err(Fault::Shape),
    };
    let (local, offset) = temporal::local(zoned).ok_or(Fault::TimeRange)?;
    Ok(Moment {
        date: local.date,
        clock: local.clock,
        zone: Some(Placed {
            zone: zoned.zone,
            offset: i32::try_from(offset / 1000).map_err(|_| Fault::TimeRange)?,
        }),
    })
}

/// A `Locale` value's BCP 47 tag.
pub(crate) fn tag_of(locale: &Value) -> Result<&'static str, Fault> {
    match locale {
        Value::Variant(variant) if &*variant.ty == jadpo_std::LOCALE => jadpo_std::LOCALES
            .get(variant.ordinal as usize)
            .map(|(_, tag)| *tag)
            .ok_or(Fault::Shape),
        _ => Err(Fault::Shape),
    }
}

/// A time for people, as `text.format(value, locale:, zone:, format:)`
/// shows the time types: in the locale tagged `tag`, in the format given
/// as its text.
pub(crate) fn format_in(
    value: &Value,
    zone: &Value,
    tag: &str,
    format_spec: &Value,
) -> Result<Value, Fault> {
    let Value::Text(spec) = format_spec else {
        return Err(Fault::Shape);
    };
    let spec = TimeFormatSpec::parse(spec).ok_or(Fault::Shape)?;
    format(moment_of(value, zone)?, tag, &spec).map(|text| Value::text(&text))
}

/// A time for people in a `Locale` value's locale, as the tests write it.
#[cfg(test)]
fn format_value(
    value: &Value,
    zone: &Value,
    locale: &Value,
    format_spec: &Value,
) -> Result<Value, Fault> {
    format_in(value, zone, tag_of(locale)?, format_spec)
}

/// `temporal.format_relative(value, now:, zone:, locale:)`.
pub fn format_relative_value(
    value: &Value,
    now: &Value,
    zone: &Value,
    locale: &Value,
) -> Result<Value, Fault> {
    let Value::Instant(now) = now else {
        return Err(Fault::Shape);
    };
    let moment = moment_of(value, zone)?;
    let seen = moment.zone.ok_or(Fault::Shape)?;
    let value_millis = match value {
        Value::Instant(millis) => *millis,
        Value::Zoned(zoned) => zoned.millis,
        _ => return Err(Fault::Shape),
    };
    let now_zoned = Zoned {
        millis: *now,
        zone: seen.zone,
    };
    let (today, _) = temporal::local(now_zoned).ok_or(Fault::TimeRange)?;
    format_relative(moment, value_millis, *now, today.date, tag_of(locale)?)
        .map(|text| Value::text(&text))
}

/// `locale` as ICU4X reads it, from a `Locale` variant's BCP 47 tag.
pub(crate) fn locale_of(tag: &str) -> Result<Locale, Fault> {
    tag.parse().map_err(|_| Fault::Shape)
}

/// The moment as ICU4X holds it. A date has no zone, so it is given the
/// unknown one, which no pattern shown for a date reads.
fn zoned(moment: Moment) -> Result<Zdt, Fault> {
    let (year, month, day) = jadpo_std::civil_date(moment.date);
    let date = Date::try_new_gregorian(
        i32::try_from(year).map_err(|_| Fault::TimeRange)?,
        u8::try_from(month).map_err(|_| Fault::TimeRange)?,
        u8::try_from(day).map_err(|_| Fault::TimeRange)?,
    )
    .map_err(|_| Fault::TimeRange)?;
    let clock = moment.clock;
    let time = Time::try_new(
        u8::try_from(clock / 3_600_000).map_err(|_| Fault::Shape)?,
        u8::try_from(clock / 60_000 % 60).map_err(|_| Fault::Shape)?,
        u8::try_from(clock / 1000 % 60).map_err(|_| Fault::Shape)?,
        (clock % 1000) * 1_000_000,
    )
    .map_err(|_| Fault::Shape)?;
    let at = DateTime { date, time };
    let info = match moment.zone {
        Some(placed) => {
            let (_, name) = ZONES.get(placed.zone as usize).ok_or(Fault::Shape)?;
            let offset = UtcOffset::try_from_seconds(placed.offset).map_err(|_| Fault::Shape)?;
            IanaParser::new()
                .parse(name)
                .with_offset(Some(offset))
                .at_date_time(at)
        }
        None => TimeZone::UNKNOWN.with_offset(None).at_date_time(at),
    };
    Ok(Zdt {
        date: at.date,
        time: at.time,
        zone: info,
    })
}

/// What the parts of a declared format ask for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Wants {
    weekday: Option<ComponentStyle>,
    day: Option<ComponentStyle>,
    month: Option<ComponentStyle>,
    year: Option<ComponentStyle>,
    hour: Option<ComponentStyle>,
    minute: Option<ComponentStyle>,
    second: Option<ComponentStyle>,
    zone: Option<ComponentStyle>,
}

impl Wants {
    fn of(parts: &[(Component, ComponentStyle)]) -> Wants {
        let mut wants = Wants::default();
        for (component, style) in parts {
            let slot = match component {
                Component::Weekday => &mut wants.weekday,
                Component::Day => &mut wants.day,
                Component::Month => &mut wants.month,
                Component::Year => &mut wants.year,
                Component::Hour => &mut wants.hour,
                Component::Minute => &mut wants.minute,
                Component::Second => &mut wants.second,
                Component::ZoneName => &mut wants.zone,
            };
            *slot = Some(*style);
        }
        wants
    }

    /// The date parts, as a combination CLDR has patterns for, and the
    /// parts left over.
    fn date_fields(&self) -> (Option<DateFields>, Vec<Component>) {
        use DateFields::{D, DE, E, M, MD, MDE, Y, YM, YMD, YMDE};
        let (e, d, m, y) = (
            self.weekday.is_some(),
            self.day.is_some(),
            self.month.is_some(),
            self.year.is_some(),
        );
        match (y, m, d, e) {
            (false, false, false, false) => (None, Vec::new()),
            (true, true, true, true) => (Some(YMDE), Vec::new()),
            (true, true, true, false) => (Some(YMD), Vec::new()),
            (false, true, true, true) => (Some(MDE), Vec::new()),
            (false, true, true, false) => (Some(MD), Vec::new()),
            (false, false, true, true) => (Some(DE), Vec::new()),
            (false, false, false, true) => (Some(E), Vec::new()),
            (false, true, false, false) => (Some(M), Vec::new()),
            (true, true, false, false) => (Some(YM), Vec::new()),
            (true, false, false, false) => (Some(Y), Vec::new()),
            (false, false, true, false) => (Some(D), Vec::new()),
            // The rest are not a pattern of CLDR's: the day and the
            // weekday with the month, the year left to follow.
            (true, true, false, true) => (Some(YM), vec![Component::Weekday]),
            (true, false, true, false) => (Some(D), vec![Component::Year]),
            (true, false, true, true) => (Some(DE), vec![Component::Year]),
            (true, false, false, true) => (Some(E), vec![Component::Year]),
            (false, true, false, true) => (Some(M), vec![Component::Weekday]),
        }
    }

    /// The length the pattern for the date is taken at: CLDR's long date
    /// pattern holds the month as a name, and its width is then set, so
    /// `month: short` is the long pattern with `MMM`; a number is taken
    /// from the short pattern.
    fn date_length(&self) -> Length {
        match self.month {
            Some(ComponentStyle::Long | ComponentStyle::Short | ComponentStyle::Narrow) => {
                Length::Long
            }
            _ => Length::Short,
        }
    }

    /// The length a date is glued to a time at, as CLDR's own date formats
    /// do: "at" for a long month, a comma for an abbreviated one.
    fn glue_length(&self) -> Length {
        match self.month {
            Some(ComponentStyle::Long) => Length::Long,
            Some(ComponentStyle::Short | ComponentStyle::Narrow) => Length::Medium,
            _ => Length::Short,
        }
    }
}

/// `format`: the text for `moment` in `locale` (a BCP 47 tag).
pub fn format(moment: Moment, locale: &str, spec: &TimeFormatSpec) -> Result<String, Fault> {
    let locale = locale_of(locale)?;
    let at = zoned(moment)?;
    let prefs = DateTimeFormatterPreferences::from(&locale);
    match spec {
        TimeFormatSpec::Builtin(format) => {
            let fields = builtin_fields(*format, moment.zone.is_some())?;
            let pattern = pattern_of(prefs, fields, &at)?;
            let pattern =
                DateTimePattern::try_from_pattern_str(&pattern).map_err(|_| Fault::Shape)?;
            render(prefs, &pattern, &at)
        }
        TimeFormatSpec::Components(parts) => components(prefs, Wants::of(parts), &at),
    }
}

/// The fields a built-in format shows: CLDR's date, time and combined
/// patterns at the length, with the zone for the two longest times.
fn builtin_fields(format: BuiltinFormat, zoned: bool) -> Result<CompositeFieldSet, Fault> {
    let mut builder = FieldSetBuilder::new();
    let (length, precision, zone) = match format.length {
        FormatLength::Full => (
            Length::Long,
            TimePrecision::Second,
            Some(ZoneStyle::SpecificLong),
        ),
        FormatLength::Long => (
            Length::Long,
            TimePrecision::Second,
            Some(ZoneStyle::SpecificShort),
        ),
        FormatLength::Medium => (Length::Medium, TimePrecision::Second, None),
        FormatLength::Short => (Length::Short, TimePrecision::Minute, None),
    };
    builder.length = Some(length);
    if format.part != FormatPart::Time {
        builder.date_fields = Some(if format.length == FormatLength::Full {
            DateFields::YMDE
        } else {
            DateFields::YMD
        });
    }
    if format.part != FormatPart::Date {
        builder.time_precision = Some(precision);
        if zoned {
            builder.zone_style = zone;
        }
    }
    builder.build_composite().map_err(|_| Fault::Shape)
}

/// A declared format: the pattern the locale has for the parts, widths
/// set, then the parts it has no pattern with after it.
fn components(
    prefs: DateTimeFormatterPreferences,
    wants: Wants,
    at: &Zdt,
) -> Result<String, Fault> {
    let (date, leftover) = wants.date_fields();
    let time = time_precision(&wants);
    let zone = wants.zone.map(|style| match style {
        ComponentStyle::Long => ZoneStyle::SpecificLong,
        _ => ZoneStyle::SpecificShort,
    });
    let mut pieces: Vec<String> = Vec::new();
    let parts = Parts { date, time, zone };
    if let Some(pattern) = main_pattern(prefs, &wants, parts, at)? {
        pieces.push(pattern);
    }
    // A minute or a second with no hour is written on its own.
    if time.is_none()
        && let Some(pattern) = clock_parts(&wants)
    {
        pieces.push(pattern);
    }
    for component in leftover {
        let mut single = Wants::default();
        let fields = match component {
            Component::Weekday => {
                single.weekday = wants.weekday;
                DateFields::E
            }
            _ => {
                single.year = wants.year;
                DateFields::Y
            }
        };
        let parts = Parts {
            date: Some(fields),
            time: None,
            zone: None,
        };
        if let Some(pattern) = main_pattern(prefs, &single, parts, at)? {
            pieces.push(pattern);
        }
    }
    let joined =
        DateTimePattern::try_from_pattern_str(&pieces.join(" ")).map_err(|_| Fault::Shape)?;
    render(prefs, &joined, at)
}

/// Which time fields a pattern from the locale is asked for, when the
/// format shows an hour: the locale's patterns hold the hour with the
/// minute, and the second with both.
fn time_precision(wants: &Wants) -> Option<TimePrecision> {
    wants.hour?;
    Some(if wants.second.is_some() {
        TimePrecision::Second
    } else if wants.minute.is_some() {
        TimePrecision::Minute
    } else {
        TimePrecision::Hour
    })
}

/// A minute or a second with no hour, as a pattern: `m`, `ss`, `mm:ss`.
fn clock_parts(wants: &Wants) -> Option<String> {
    let width = |style: Option<ComponentStyle>, both: bool| match style {
        Some(ComponentStyle::TwoDigit) => 2,
        Some(_) if both => 2,
        _ => 1,
    };
    let both = wants.minute.is_some() && wants.second.is_some();
    match (wants.minute, wants.second) {
        (None, None) => None,
        (minute, None) => Some("m".repeat(width(minute, false))),
        (None, second) => Some("s".repeat(width(second, false))),
        (minute, second) => Some(format!(
            "{}:{}",
            "m".repeat(width(minute, both)),
            "s".repeat(width(second, both))
        )),
    }
}

/// The fields a pattern from the locale is asked to hold.
#[derive(Clone, Copy)]
struct Parts {
    date: Option<DateFields>,
    time: Option<TimePrecision>,
    zone: Option<ZoneStyle>,
}

impl Parts {
    fn date_only(self) -> Parts {
        Parts {
            time: None,
            zone: None,
            ..self
        }
    }

    fn rest_only(self) -> Parts {
        Parts { date: None, ..self }
    }

    fn pattern(
        self,
        prefs: DateTimeFormatterPreferences,
        length: Length,
        at: &Zdt,
    ) -> Result<Option<String>, Fault> {
        if self.date.is_none() && self.time.is_none() && self.zone.is_none() {
            return Ok(None);
        }
        let mut builder = FieldSetBuilder::new();
        // A zone alone has no length.
        if self.date.is_some() || self.time.is_some() {
            builder.length = Some(length);
        }
        builder.date_fields = self.date;
        builder.time_precision = self.time;
        builder.zone_style = self.zone;
        if self.date.is_some_and(|fields| {
            matches!(
                fields,
                DateFields::YMD | DateFields::YMDE | DateFields::YM | DateFields::Y
            )
        }) {
            builder.year_style = Some(YearStyle::Full);
        }
        let fields = builder.build_composite().map_err(|_| Fault::Shape)?;
        pattern_of(prefs, fields, at).map(Some)
    }
}

/// The pattern for a declared format's date, time and zone, with the
/// widths asked for. A date and a time are glued the way the locale glues
/// them at the length the month's text asks for ("at" for a long month,
/// a comma for an abbreviated one); with no pattern for the combination
/// they are written one after the other.
fn main_pattern(
    prefs: DateTimeFormatterPreferences,
    wants: &Wants,
    parts: Parts,
    at: &Zdt,
) -> Result<Option<String>, Fault> {
    let mut date = parts.date_only().pattern(prefs, wants.date_length(), at)?;
    // An abbreviated month is taken from the medium pattern when that
    // writes the month as a name (most locales do); where the medium
    // pattern is all numbers (German's `dd.MM.y`) the long one is used.
    if matches!(
        wants.month,
        Some(ComponentStyle::Short | ComponentStyle::Narrow)
    ) {
        let medium = parts.date_only().pattern(prefs, Length::Medium, at)?;
        if let Some(medium) = medium.filter(|pattern| month_width(pattern) >= 3) {
            date = Some(medium);
        }
    }
    // The time and zone are written the same at any length.
    let rest = parts.rest_only().pattern(prefs, Length::Short, at)?;
    let date = date.map(|pattern| adjust(&pattern, wants));
    let rest = rest.map(|pattern| adjust(&pattern, wants));
    Ok(match (date, rest) {
        (None, None) => None,
        (Some(only), None) | (None, Some(only)) => Some(only),
        (Some(date), Some(rest)) => {
            let glue = if parts.date.is_some_and(DateFields::is_calendar_period) {
                None
            } else {
                glue(prefs, wants.glue_length(), parts, at)?
            };
            Some(match glue {
                Some(glue) => glue.replace(DATE_SLOT, &date).replace(REST_SLOT, &rest),
                None => format!("{date}, {rest}"),
            })
        }
    })
}

/// Stand-ins for the date and for the time and zone in a glue pattern.
const DATE_SLOT: char = '\u{1}';
const REST_SLOT: char = '\u{2}';

/// How the locale puts a date and a time (and zone) together at `length`:
/// its pattern for both with the pattern for each cut out of it.
fn glue(
    prefs: DateTimeFormatterPreferences,
    length: Length,
    parts: Parts,
    at: &Zdt,
) -> Result<Option<String>, Fault> {
    let (Some(both), Some(date), Some(rest)) = (
        parts.pattern(prefs, length, at)?,
        parts.date_only().pattern(prefs, length, at)?,
        parts.rest_only().pattern(prefs, length, at)?,
    ) else {
        return Ok(None);
    };
    let (Some(date_at), Some(rest_at)) = (both.find(&date), both.find(&rest)) else {
        return Ok(None);
    };
    let (date_end, rest_end) = (date_at + date.len(), rest_at + rest.len());
    let ((start, end, slot), (next_start, next_end, next_slot)) = if date_at < rest_at {
        (
            (date_at, date_end, DATE_SLOT),
            (rest_at, rest_end, REST_SLOT),
        )
    } else {
        (
            (rest_at, rest_end, REST_SLOT),
            (date_at, date_end, DATE_SLOT),
        )
    };
    if end > next_start {
        return Ok(None);
    }
    let mut glue = String::new();
    glue.push_str(both.get(..start).ok_or(Fault::Shape)?);
    glue.push(slot);
    glue.push_str(both.get(end..next_start).ok_or(Fault::Shape)?);
    glue.push(next_slot);
    glue.push_str(both.get(next_end..).ok_or(Fault::Shape)?);
    Ok(Some(glue))
}

/// The locale's pattern for `fields`, as the text CLDR writes it.
fn pattern_of(
    prefs: DateTimeFormatterPreferences,
    fields: CompositeFieldSet,
    at: &Zdt,
) -> Result<String, Fault> {
    let formatter = FixedCalendarDateTimeFormatter::<Gregorian, _>::try_new(prefs, fields)
        .map_err(|_| Fault::Shape)?;
    let formatted = formatter.format(at);
    Ok(formatted.pattern().write_to_string().into_owned())
}

/// A pattern written out with the locale's names and digits.
fn render(
    prefs: DateTimeFormatterPreferences,
    pattern: &DateTimePattern,
    at: &Zdt,
) -> Result<String, Fault> {
    let mut names = FixedCalendarDateTimeNames::<Gregorian, CompositeFieldSet>::try_new(prefs)
        .map_err(|_| Fault::Shape)?;
    names
        .include_for_pattern(pattern)
        .map_err(|_| Fault::Shape)?;
    let text = names
        .with_pattern_unchecked(pattern)
        .format(at)
        .try_write_to_string()
        .map(|text| text.into_owned())
        .unwrap_or_else(|(_, text)| text.into_owned());
    // CLDR puts a narrow no-break space before `PM` and in some date
    // patterns; browsers and Node write a plain space, which is what a
    // test would be written with.
    Ok(text.replace('\u{202f}', " "))
}

/// Sets each field of `pattern` to the width its part asked for, leaving
/// literals alone: the width of a number the part leaves at `numeric` is
/// the locale's own (`dd/MM/y` stays), `two_digit` makes it at least two,
/// and a name's width is its style's.
fn adjust(pattern: &str, wants: &Wants) -> String {
    let mut out = String::new();
    let mut chars = pattern.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\'' {
            // Quoted text, `''` being one apostrophe.
            out.push(c);
            while let Some(inner) = chars.next() {
                out.push(inner);
                if inner == '\'' {
                    if chars.peek() == Some(&'\'') {
                        out.extend(chars.next());
                    } else {
                        break;
                    }
                }
            }
            continue;
        }
        if !c.is_ascii_alphabetic() {
            out.push(c);
            continue;
        }
        let mut width = 1;
        while chars.peek() == Some(&c) {
            chars.next();
            width += 1;
        }
        let width = widened(c, width, wants);
        out.extend(std::iter::repeat_n(c, width));
    }
    out
}

/// The width of the month field a pattern has, 0 for none.
fn month_width(pattern: &str) -> usize {
    let mut width = 0;
    let mut quoted = false;
    let mut run = 0;
    let mut last = ' ';
    for c in pattern.chars().chain(std::iter::once(' ')) {
        if c == '\'' {
            quoted = !quoted;
            continue;
        }
        if quoted {
            continue;
        }
        if c == last {
            run += 1;
            continue;
        }
        if matches!(last, 'M' | 'L') {
            width = width.max(run);
        }
        last = c;
        run = 1;
    }
    width
}

/// The width of a weekday's name for the style asked for.
fn weekday_width(wants: &Wants) -> Option<usize> {
    use ComponentStyle::{Long, Narrow, Short};
    match wants.weekday {
        Some(Long) => Some(4),
        Some(Short) => Some(3),
        Some(Narrow) => Some(5),
        _ => None,
    }
}

fn widened(symbol: char, width: usize, wants: &Wants) -> usize {
    use ComponentStyle::{Long, Narrow, Numeric, Short, TwoDigit};
    let number = |style: Option<ComponentStyle>| match style {
        Some(TwoDigit) => width.max(2),
        _ => width,
    };
    match symbol {
        'd' => number(wants.day),
        'y' => match wants.year {
            Some(TwoDigit) => 2,
            _ => width,
        },
        // A month written as a number beside a literal that names it
        // (Japanese's `M月`) stays as it is.
        'M' | 'L' if width < 3 && matches!(wants.month, Some(Long | Short | Narrow)) => width,
        'M' | 'L' => match wants.month {
            Some(Long) => 4,
            Some(Short) => 3,
            Some(Narrow) => 5,
            Some(TwoDigit) => 2,
            Some(Numeric) if width >= 3 => 1,
            _ => width,
        },
        // `E` is the abbreviated name at any width; `c` and `e` are
        // numbers until their third letter.
        'E' => weekday_width(wants).unwrap_or(width),
        'c' | 'e' if width >= 3 => match wants.weekday {
            Some(Long) => 4,
            Some(Short) => 3,
            Some(Narrow) => 5,
            _ => width,
        },
        'h' | 'H' | 'K' | 'k' => number(wants.hour),
        'm' => number(wants.minute),
        's' => number(wants.second),
        'z' => match wants.zone {
            Some(Long) => 4,
            _ => 1,
        },
        _ => width,
    }
}

/// What the tests here and beside share.
#[cfg(test)]
mod tests_support {
    use super::*;
    use crate::temporal::zone_value;

    pub(super) fn locale_value(tag: &str) -> Value {
        let at = jadpo_std::LOCALES
            .iter()
            .position(|(_, t)| *t == tag)
            .unwrap_or(0) as u32;
        let name = jadpo_std::LOCALES
            .get(at as usize)
            .map_or("", |(name, _)| *name);
        Value::variant(jadpo_std::LOCALE, at, name, Vec::new())
    }

    pub(super) fn zone_named(iana: &str) -> Value {
        zone_value(ZONES.iter().position(|(_, n)| *n == iana).unwrap_or(0) as u32)
    }

    pub(super) fn text(value: Result<Value, Fault>) -> String {
        match value {
            Ok(Value::Text(text)) => text.to_string(),
            other => format!("{other:?}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `(locale, zone, instant, a date, format, what Intl.DateTimeFormat
    /// writes for the same options and locale)`, with `calendar:
    /// "gregory"`. Node 24.18 (ICU 78.3, CLDR 48.0) wrote them; a plain
    /// space stands for the narrow no-break space CLDR puts before `PM`,
    /// as browsers and Node write it. The first rows are every built-in
    /// format in two locales; the rest are declared formats in a spread of
    /// scripts: right to left (`ar-EG`, `he`, `fa`, `ur`), other digits
    /// (`ar-EG`, `fa`, `bn`, `mr`, `ne`, `my`, `hi-IN`) and other orders.
    #[rustfmt::skip]
    const INTL: &[(&str, &str, i64, bool, &str, &str)] = &[
        ("en-GB", "Europe/London", 1790515805000, false, "date_full", "Sunday, 27 September 2026"),
        ("en-GB", "Europe/London", 1790515805000, false, "date_long", "27 September 2026"),
        ("en-GB", "Europe/London", 1790515805000, false, "date_medium", "27 Sept 2026"),
        ("en-GB", "Europe/London", 1790515805000, false, "date_short", "27/09/2026"),
        ("en-GB", "Europe/London", 1790515805000, false, "time_full", "14:30:05 British Summer Time"),
        ("en-GB", "Europe/London", 1790515805000, false, "time_long", "14:30:05 BST"),
        ("en-GB", "Europe/London", 1790515805000, false, "time_medium", "14:30:05"),
        ("en-GB", "Europe/London", 1790515805000, false, "time_short", "14:30"),
        ("en-GB", "Europe/London", 1790515805000, false, "date_time_full", "Sunday, 27 September 2026 at 14:30:05 British Summer Time"),
        ("en-GB", "Europe/London", 1790515805000, false, "date_time_long", "27 September 2026 at 14:30:05 BST"),
        ("en-GB", "Europe/London", 1790515805000, false, "date_time_medium", "27 Sept 2026, 14:30:05"),
        ("en-GB", "Europe/London", 1790515805000, false, "date_time_short", "27/09/2026, 14:30"),
        ("en-US", "America/New_York", 1767654429000, false, "date_full", "Monday, January 5, 2026"),
        ("en-US", "America/New_York", 1767654429000, false, "date_long", "January 5, 2026"),
        ("en-US", "America/New_York", 1767654429000, false, "date_medium", "Jan 5, 2026"),
        ("en-US", "America/New_York", 1767654429000, false, "date_short", "1/5/26"),
        ("en-US", "America/New_York", 1767654429000, false, "time_full", "6:07:09 PM Eastern Standard Time"),
        ("en-US", "America/New_York", 1767654429000, false, "time_long", "6:07:09 PM EST"),
        ("en-US", "America/New_York", 1767654429000, false, "time_medium", "6:07:09 PM"),
        ("en-US", "America/New_York", 1767654429000, false, "time_short", "6:07 PM"),
        ("en-US", "America/New_York", 1767654429000, false, "date_time_full", "Monday, January 5, 2026 at 6:07:09 PM Eastern Standard Time"),
        ("en-US", "America/New_York", 1767654429000, false, "date_time_long", "January 5, 2026 at 6:07:09 PM EST"),
        ("en-US", "America/New_York", 1767654429000, false, "date_time_medium", "Jan 5, 2026, 6:07:09 PM"),
        ("en-US", "America/New_York", 1767654429000, false, "date_time_short", "1/5/26, 6:07 PM"),
        ("en-GB", "Europe/London", 1790515805000, false, "date_long", "27 September 2026"),
        ("en-GB", "Europe/London", 1790515805000, false, "date_time_long", "27 September 2026 at 14:30:05 BST"),
        ("en-GB", "Europe/London", 1790515805000, false, "time_short", "14:30"),
        ("en-GB", "Europe/London", 1790515805000, false, "day:numeric month:short year:numeric", "27 Sept 2026"),
        ("en-GB", "Europe/London", 1790515805000, false, "weekday:long day:numeric month:long year:numeric", "Sunday, 27 September 2026"),
        ("en-GB", "Europe/London", 1790515805000, false, "hour:numeric minute:two_digit", "14:30"),
        ("en-GB", "Europe/London", 1790515805000, false, "hour:numeric minute:two_digit zone_name:long", "14:30 British Summer Time"),
        ("en-US", "Europe/London", 1790515805000, false, "date_long", "September 27, 2026"),
        ("en-US", "Europe/London", 1790515805000, false, "date_time_long", "September 27, 2026 at 2:30:05 PM GMT+1"),
        ("en-US", "Europe/London", 1790515805000, false, "time_short", "2:30 PM"),
        ("en-US", "Europe/London", 1790515805000, false, "day:numeric month:short year:numeric", "Sep 27, 2026"),
        ("en-US", "Europe/London", 1790515805000, false, "weekday:long day:numeric month:long year:numeric", "Sunday, September 27, 2026"),
        ("en-US", "Europe/London", 1790515805000, false, "hour:numeric minute:two_digit", "2:30 PM"),
        ("en-US", "Europe/London", 1790515805000, false, "hour:numeric minute:two_digit zone_name:long", "2:30 PM British Summer Time"),
        ("fr-FR", "Europe/London", 1790515805000, false, "date_long", "27 septembre 2026"),
        ("fr-FR", "Europe/London", 1790515805000, false, "date_time_long", "27 septembre 2026 à 14:30:05 UTC+1"),
        ("fr-FR", "Europe/London", 1790515805000, false, "day:numeric month:short year:numeric", "27 sept. 2026"),
        ("fr-FR", "Europe/London", 1790515805000, false, "weekday:long day:numeric month:long year:numeric", "dimanche 27 septembre 2026"),
        ("fr-FR", "Europe/London", 1790515805000, false, "hour:numeric minute:two_digit", "14:30"),
        ("fr-FR", "Europe/London", 1790515805000, false, "hour:numeric minute:two_digit zone_name:long", "14:30 heure d’été britannique"),
        ("de-DE", "Europe/London", 1790515805000, false, "date_long", "27. September 2026"),
        ("de-DE", "Europe/London", 1790515805000, false, "date_time_long", "27. September 2026 um 14:30:05 GMT+1"),
        ("de-DE", "Europe/London", 1790515805000, false, "day:numeric month:short year:numeric", "27. Sept. 2026"),
        ("de-DE", "Europe/London", 1790515805000, false, "weekday:long day:numeric month:long year:numeric", "Sonntag, 27. September 2026"),
        ("de-DE", "Europe/London", 1790515805000, false, "hour:numeric minute:two_digit", "14:30"),
        ("de-DE", "Europe/London", 1790515805000, false, "hour:numeric minute:two_digit zone_name:long", "14:30 Britische Sommerzeit"),
        ("es-ES", "Europe/London", 1790515805000, false, "date_long", "27 de septiembre de 2026"),
        ("es-ES", "Europe/London", 1790515805000, false, "date_time_long", "27 de septiembre de 2026 a las 14:30:05 GMT+1"),
        ("es-ES", "Europe/London", 1790515805000, false, "day:numeric month:short year:numeric", "27 sept 2026"),
        ("es-ES", "Europe/London", 1790515805000, false, "weekday:long day:numeric month:long year:numeric", "domingo, 27 de septiembre de 2026"),
        ("es-ES", "Europe/London", 1790515805000, false, "hour:numeric minute:two_digit", "14:30"),
        ("es-ES", "Europe/London", 1790515805000, false, "hour:numeric minute:two_digit zone_name:long", "14:30 hora de verano británica"),
        ("ru-RU", "Europe/London", 1790515805000, false, "date_long", "27 сентября 2026 г."),
        ("ru-RU", "Europe/London", 1790515805000, false, "date_time_long", "27 сентября 2026 г. в 14:30:05 GMT+1"),
        ("ru-RU", "Europe/London", 1790515805000, false, "day:numeric month:short year:numeric", "27 сент. 2026 г."),
        ("ru-RU", "Europe/London", 1790515805000, false, "weekday:long day:numeric month:long year:numeric", "воскресенье, 27 сентября 2026 г."),
        ("ru-RU", "Europe/London", 1790515805000, false, "hour:numeric minute:two_digit", "14:30"),
        ("ru-RU", "Europe/London", 1790515805000, false, "hour:numeric minute:two_digit zone_name:long", "14:30 Великобритания, летнее время"),
        ("ja-JP", "Europe/London", 1790515805000, false, "date_long", "2026年9月27日"),
        ("ja-JP", "Europe/London", 1790515805000, false, "date_time_long", "2026年9月27日 14:30:05 GMT+1"),
        ("ja-JP", "Europe/London", 1790515805000, false, "day:numeric month:short year:numeric", "2026年9月27日"),
        ("ja-JP", "Europe/London", 1790515805000, false, "weekday:long day:numeric month:long year:numeric", "2026年9月27日日曜日"),
        ("ja-JP", "Europe/London", 1790515805000, false, "hour:numeric minute:two_digit", "14:30"),
        ("ja-JP", "Europe/London", 1790515805000, false, "hour:numeric minute:two_digit zone_name:long", "14:30 英国夏時間"),
        ("ko-KR", "Europe/London", 1790515805000, false, "date_long", "2026년 9월 27일"),
        ("ko-KR", "Europe/London", 1790515805000, false, "day:numeric month:short year:numeric", "2026년 9월 27일"),
        ("ko-KR", "Europe/London", 1790515805000, false, "weekday:long day:numeric month:long year:numeric", "2026년 9월 27일 일요일"),
        ("ko-KR", "Europe/London", 1790515805000, false, "hour:numeric minute:two_digit", "오후 2:30"),
        ("ko-KR", "Europe/London", 1790515805000, false, "hour:numeric minute:two_digit zone_name:long", "오후 2:30 영국 하계 표준시"),
        ("hi-IN", "Europe/London", 1790515805000, false, "date_long", "27 सितंबर 2026"),
        ("hi-IN", "Europe/London", 1790515805000, false, "date_time_long", "27 सितंबर 2026 को 2:30:05 pm GMT+1 बजे"),
        ("hi-IN", "Europe/London", 1790515805000, false, "day:numeric month:short year:numeric", "27 सित॰ 2026"),
        ("hi-IN", "Europe/London", 1790515805000, false, "weekday:long day:numeric month:long year:numeric", "रविवार, 27 सितंबर 2026"),
        ("hi-IN", "Europe/London", 1790515805000, false, "hour:numeric minute:two_digit", "2:30 pm"),
        ("hi-IN", "Europe/London", 1790515805000, false, "hour:numeric minute:two_digit zone_name:long", "2:30 pm ब्रिटिश ग्रीष्मकालीन समय"),
        ("ar-EG", "Europe/London", 1790515805000, false, "date_long", "٢٧ سبتمبر ٢٠٢٦"),
        ("ar-EG", "Europe/London", 1790515805000, false, "day:numeric month:short year:numeric", "٢٧ سبتمبر ٢٠٢٦"),
        ("ar-EG", "Europe/London", 1790515805000, false, "weekday:long day:numeric month:long year:numeric", "الأحد، ٢٧ سبتمبر ٢٠٢٦"),
        ("ar-EG", "Europe/London", 1790515805000, false, "hour:numeric minute:two_digit", "٢:٣٠ م"),
        ("ar-EG", "Europe/London", 1790515805000, false, "hour:numeric minute:two_digit zone_name:long", "٢:٣٠ م توقيت بريطانيا الصيفي"),
        ("he", "Europe/London", 1790515805000, false, "date_long", "27 בספטמבר 2026"),
        ("he", "Europe/London", 1790515805000, false, "date_time_long", "27 בספטמבר 2026 בשעה 14:30:05 GMT\u{200e}+1\u{200e}"),
        ("he", "Europe/London", 1790515805000, false, "day:numeric month:short year:numeric", "27 בספט׳ 2026"),
        ("he", "Europe/London", 1790515805000, false, "weekday:long day:numeric month:long year:numeric", "יום ראשון, 27 בספטמבר 2026"),
        ("he", "Europe/London", 1790515805000, false, "hour:numeric minute:two_digit", "14:30"),
        ("he", "Europe/London", 1790515805000, false, "hour:numeric minute:two_digit zone_name:long", "14:30 שעון קיץ בריטניה"),
        ("fa", "Europe/London", 1790515805000, false, "date_long", "۲۷ سپتامبر ۲۰۲۶"),
        ("fa", "Europe/London", 1790515805000, false, "date_time_long", "۲۷ سپتامبر ۲۰۲۶ ساعت ۱۴:۳۰:۰۵ (\u{200e}+۱ گرینویچ)"),
        ("fa", "Europe/London", 1790515805000, false, "day:numeric month:short year:numeric", "۲۷ سپتامبر ۲۰۲۶"),
        ("fa", "Europe/London", 1790515805000, false, "weekday:long day:numeric month:long year:numeric", "یکشنبه ۲۷ سپتامبر ۲۰۲۶"),
        ("fa", "Europe/London", 1790515805000, false, "hour:numeric minute:two_digit", "۱۴:۳۰"),
        ("fa", "Europe/London", 1790515805000, false, "hour:numeric minute:two_digit zone_name:long", "۱۴:۳۰ (وقت تابستانی بریتانیا)"),
        ("th", "Europe/London", 1790515805000, false, "date_long", "27 กันยายน ค.ศ. 2026"),
        ("th", "Europe/London", 1790515805000, false, "day:numeric month:short year:numeric", "27 ก.ย. 2026"),
        ("th", "Europe/London", 1790515805000, false, "hour:numeric minute:two_digit", "14:30"),
        ("th", "Europe/London", 1790515805000, false, "hour:numeric minute:two_digit zone_name:long", "14:30 เวลาฤดูร้อนอังกฤษ"),
        ("bn", "Europe/London", 1790515805000, false, "date_long", "২৭ সেপ্টেম্বর, ২০২৬"),
        ("bn", "Europe/London", 1790515805000, false, "date_time_long", "২৭ সেপ্টেম্বর, ২০২৬ এ ২:৩০:০৫ PM GMT +১"),
        ("bn", "Europe/London", 1790515805000, false, "day:numeric month:short year:numeric", "২৭ সেপ, ২০২৬"),
        ("bn", "Europe/London", 1790515805000, false, "weekday:long day:numeric month:long year:numeric", "রবিবার, ২৭ সেপ্টেম্বর, ২০২৬"),
        ("bn", "Europe/London", 1790515805000, false, "hour:numeric minute:two_digit", "২:৩০ PM"),
        ("bn", "Europe/London", 1790515805000, false, "hour:numeric minute:two_digit zone_name:long", "২:৩০ PM ব্রিটিশ গ্রীষ্মকালীন সময়"),
        ("mr", "Europe/London", 1790515805000, false, "date_long", "२७ सप्टेंबर, २०२६"),
        ("mr", "Europe/London", 1790515805000, false, "date_time_long", "२७ सप्टेंबर, २०२६ रोजी २:३०:०५ PM [GMT]+१ वाजता"),
        ("mr", "Europe/London", 1790515805000, false, "day:numeric month:short year:numeric", "२७ सप्टें, २०२६"),
        ("mr", "Europe/London", 1790515805000, false, "hour:numeric minute:two_digit", "२:३० PM"),
        ("mr", "Europe/London", 1790515805000, false, "hour:numeric minute:two_digit zone_name:long", "२:३० PM ब्रिटिश उन्हाळी वेळ"),
        ("my", "Europe/London", 1790515805000, false, "date_long", "၂၀၂၆ စက်တင်ဘာ ၂၇"),
        ("my", "Europe/London", 1790515805000, false, "date_time_long", "၂၀၂၆ စက်တင်ဘာ ၂၇ GMT+၁ ၁၄:၃၀:၀၅"),
        ("my", "Europe/London", 1790515805000, false, "day:numeric month:short year:numeric", "၂၀၂၆ စက် ၂၇"),
        ("my", "Europe/London", 1790515805000, false, "weekday:long day:numeric month:long year:numeric", "၂၀၂၆ စက်တင်ဘာ ၂၇ တနင်္ဂနွေ"),
        ("my", "Europe/London", 1790515805000, false, "hour:numeric minute:two_digit", "၁၄:၃၀"),
        ("my", "Europe/London", 1790515805000, false, "hour:numeric minute:two_digit zone_name:long", "ဗြိတိန် နွေရာသီ အချိန် ၁၄:၃၀"),
        ("ne", "Europe/London", 1790515805000, false, "date_long", "२०२६ सेप्टेम्बर २७"),
        ("ne", "Europe/London", 1790515805000, false, "date_time_long", "२०२६ सेप्टेम्बर २७: १४:३०:०५ GMT+१"),
        ("ne", "Europe/London", 1790515805000, false, "day:numeric month:short year:numeric", "२०२६ सेप्टेम्बर २७"),
        ("ne", "Europe/London", 1790515805000, false, "weekday:long day:numeric month:long year:numeric", "२०२६ सेप्टेम्बर २७, आइतबार"),
        ("ne", "Europe/London", 1790515805000, false, "hour:numeric minute:two_digit", "१४:३०"),
        ("ne", "Europe/London", 1790515805000, false, "hour:numeric minute:two_digit zone_name:long", "१४:३० बेलायती ग्रीष्मकालीन समय"),
        ("ur", "Europe/London", 1790515805000, false, "date_long", "27 ستمبر، 2026"),
        ("ur", "Europe/London", 1790515805000, false, "day:numeric month:short year:numeric", "27 ستمبر، 2026"),
        ("ur", "Europe/London", 1790515805000, false, "weekday:long day:numeric month:long year:numeric", "اتوار، 27 ستمبر، 2026"),
        ("ur", "Europe/London", 1790515805000, false, "hour:numeric minute:two_digit", "2:30 PM"),
        ("ur", "Europe/London", 1790515805000, false, "hour:numeric minute:two_digit zone_name:long", "2:30 PM برٹش سمر ٹائم"),
        ("en-GB", "Europe/London", 1790515805000, true, "date_full", "Sunday, 27 September 2026"),
        ("en-GB", "Europe/London", 1790515805000, true, "date_long", "27 September 2026"),
        ("en-GB", "Europe/London", 1790515805000, true, "date_medium", "27 Sept 2026"),
        ("en-GB", "Europe/London", 1790515805000, true, "date_short", "27/09/2026"),
        ("en-GB", "Europe/London", 1790515805000, true, "day:numeric month:short year:numeric", "27 Sept 2026"),
        ("en-GB", "Europe/London", 1790515805000, true, "weekday:long day:numeric month:long year:numeric", "Sunday, 27 September 2026"),
        ("fr-FR", "Europe/London", 1790515805000, true, "date_full", "dimanche 27 septembre 2026"),
        ("fr-FR", "Europe/London", 1790515805000, true, "date_long", "27 septembre 2026"),
        ("fr-FR", "Europe/London", 1790515805000, true, "date_medium", "27 sept. 2026"),
        ("fr-FR", "Europe/London", 1790515805000, true, "date_short", "27/09/2026"),
        ("fr-FR", "Europe/London", 1790515805000, true, "day:numeric month:short year:numeric", "27 sept. 2026"),
        ("fr-FR", "Europe/London", 1790515805000, true, "weekday:long day:numeric month:long year:numeric", "dimanche 27 septembre 2026"),
        ("de-DE", "Europe/London", 1790515805000, true, "date_full", "Sonntag, 27. September 2026"),
        ("de-DE", "Europe/London", 1790515805000, true, "date_long", "27. September 2026"),
        ("de-DE", "Europe/London", 1790515805000, true, "date_medium", "27.09.2026"),
        ("de-DE", "Europe/London", 1790515805000, true, "date_short", "27.09.26"),
        ("de-DE", "Europe/London", 1790515805000, true, "day:numeric month:short year:numeric", "27. Sept. 2026"),
        ("de-DE", "Europe/London", 1790515805000, true, "weekday:long day:numeric month:long year:numeric", "Sonntag, 27. September 2026"),
        ("ja-JP", "Europe/London", 1790515805000, true, "date_full", "2026年9月27日日曜日"),
        ("ja-JP", "Europe/London", 1790515805000, true, "date_long", "2026年9月27日"),
        ("ja-JP", "Europe/London", 1790515805000, true, "date_medium", "2026/09/27"),
        ("ja-JP", "Europe/London", 1790515805000, true, "date_short", "2026/09/27"),
        ("ja-JP", "Europe/London", 1790515805000, true, "day:numeric month:short year:numeric", "2026年9月27日"),
        ("ja-JP", "Europe/London", 1790515805000, true, "weekday:long day:numeric month:long year:numeric", "2026年9月27日日曜日"),
        ("ar-EG", "Europe/London", 1790515805000, true, "date_full", "الأحد، ٢٧ سبتمبر ٢٠٢٦"),
        ("ar-EG", "Europe/London", 1790515805000, true, "date_long", "٢٧ سبتمبر ٢٠٢٦"),
        ("ar-EG", "Europe/London", 1790515805000, true, "date_medium", "٢٧\u{200f}/٠٩\u{200f}/٢٠٢٦"),
        ("ar-EG", "Europe/London", 1790515805000, true, "date_short", "٢٧\u{200f}/٩\u{200f}/٢٠٢٦"),
        ("ar-EG", "Europe/London", 1790515805000, true, "day:numeric month:short year:numeric", "٢٧ سبتمبر ٢٠٢٦"),
        ("ar-EG", "Europe/London", 1790515805000, true, "weekday:long day:numeric month:long year:numeric", "الأحد، ٢٧ سبتمبر ٢٠٢٦"),
    ];

    use super::tests_support::{locale_value, text, zone_named};

    #[test]
    fn it_writes_what_intl_writes_for_the_same_locale_and_options() {
        let mut wrong = Vec::new();
        for (locale, zone, instant, date, spec, want) in INTL {
            let shown = if *date {
                let zoned = Zoned {
                    millis: *instant,
                    zone: temporal::zone_of(&zone_named(zone)).unwrap_or(0),
                };
                Value::Date(temporal::local_date(zoned).unwrap_or(0))
            } else {
                Value::Instant(*instant)
            };
            let got = text(format_value(
                &shown,
                &zone_named(zone),
                &locale_value(locale),
                &Value::text(spec),
            ));
            if got != *want {
                wrong.push(format!(
                    "{locale} {zone} {spec}: wanted {want:?}, got {got:?}"
                ));
            }
        }
        assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    }

    fn moment(date: &str, clock: &str) -> Moment {
        let day = jadpo_std::date_number(date).unwrap_or(0);
        let clock = jadpo_std::clock_millis(clock).unwrap_or(0);
        Moment {
            date: day,
            clock,
            zone: Some(Placed { zone: 0, offset: 0 }),
        }
    }

    #[test]
    fn a_pattern_takes_the_width_each_part_asked_for() {
        let wants = Wants::of(&[
            (Component::Day, ComponentStyle::TwoDigit),
            (Component::Month, ComponentStyle::Short),
            (Component::Year, ComponentStyle::Numeric),
        ]);
        assert_eq!(adjust("d MMMM y", &wants), "dd MMM y");
        assert_eq!(adjust("EEEE, d 'de' MMMM", &wants), "EEEE, dd 'de' MMM");
        assert_eq!(
            adjust("'d' d", &wants),
            "'d' dd",
            "quoted text is left alone"
        );
    }

    #[test]
    fn a_month_written_as_a_number_beside_its_name_is_left_alone() {
        let wants = Wants::of(&[(Component::Month, ComponentStyle::Long)]);
        assert_eq!(adjust("y年M月d日", &wants), "y年M月d日");
    }

    #[test]
    fn the_fields_a_pattern_has_are_found() {
        assert_eq!(month_width("d MMMM y"), 4);
        assert_eq!(month_width("dd.MM.y"), 2);
        assert_eq!(month_width("d 'MMMM' y"), 0, "quoted text is not a field");
    }

    #[test]
    fn parts_with_no_pattern_together_are_written_one_after_another() {
        let shown = Value::Instant(1_790_515_805_000);
        let london = zone_named("Europe/London");
        let en = locale_value("en-GB");
        let spec = |text: &str| Value::text(text);
        // A year with a day, a minute with a second and no hour.
        assert_eq!(
            text(format_value(
                &shown,
                &london,
                &en,
                &spec("day:numeric year:numeric")
            )),
            "27 2026"
        );
        assert_eq!(
            text(format_value(
                &shown,
                &london,
                &en,
                &spec("minute:two_digit second:two_digit")
            )),
            "30:05"
        );
        assert_eq!(
            text(format_value(&shown, &london, &en, &spec("zone_name:long"))),
            "British Summer Time"
        );
        assert_eq!(
            text(format_value(
                &shown,
                &london,
                &en,
                &spec("month:long hour:numeric minute:two_digit")
            )),
            "September, 14:30"
        );
    }

    #[test]
    fn it_reads_the_daylight_saving_name_from_the_instant() {
        let london = zone_named("Europe/London");
        let en = locale_value("en-GB");
        let long = Value::text("zone_name:long");
        let summer = Value::Instant(1_790_515_805_000);
        let winter = Value::Instant(1_767_654_429_000);
        assert_eq!(
            text(format_value(&summer, &london, &en, &long)),
            "British Summer Time"
        );
        assert_eq!(
            text(format_value(&winter, &london, &en, &long)),
            "Greenwich Mean Time"
        );
    }

    #[test]
    fn a_date_has_no_zone_to_show() {
        let date = Value::Date(jadpo_std::date_number("2026-10-25").unwrap_or(0));
        let got = format_value(
            &date,
            &Value::None,
            &locale_value("fr-FR"),
            &Value::text("date_full"),
        );
        assert_eq!(text(got), "dimanche 25 octobre 2026");
        let _ = moment("2026-10-25", "09:30");
    }
}
