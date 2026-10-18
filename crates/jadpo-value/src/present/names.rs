//! The names CLDR gives standard values for people (area 19, "Every
//! standard type meets one contract"): a duration in its units, a weekday,
//! a locale and a zone, a country, a language and a script, each in the
//! reader's locale, as `text.format` writes them for the format keys that
//! read them.

use icu_datetime::NoCalendarFormatter;
use icu_datetime::fieldsets::zone::Location;
use icu_experimental::dimension::units::categorized_formatter::CategorizedFormatter;
use icu_experimental::dimension::units::options::{UnitsFormatterOptions, Width};
use icu_experimental::displaynames::DisplayNamesOptions;
use icu_experimental::displaynames::multi::{
    LanguageDisplayNames, LocaleDisplayNamesFormatter, RegionDisplayNames, ScriptDisplayNames,
};
use icu_experimental::measure::category::Duration as Unit;
use icu_list::ListFormatter;
use icu_list::options::{ListFormatterOptions, ListLength};
use icu_time::zone::IanaParser;
use writeable::Writeable;

use super::locale_of;
use crate::Fault;

/// A duration in its units, written out in full (`FormatKey::DurationUnits`):
/// "1 hour, 30 minutes", each unit CLDR's long name for it and the units
/// joined as the locale joins a list of units, as `Intl.DurationFormat`
/// with `style: "long"` writes it. A day is 24 hours, as a `Duration`
/// holds it (area 10); a zero duration is "0 seconds", and a negative one
/// is written as its length.
pub fn duration(millis: i64, tag: &str) -> Result<String, Fault> {
    let locale = locale_of(tag)?;
    let size = millis.unsigned_abs();
    let parts = [
        (size / 86_400_000, Unit::day()),
        (size / 3_600_000 % 24, Unit::hour()),
        (size / 60_000 % 60, Unit::minute()),
        (size / 1000 % 60, Unit::second()),
        (size % 1000, Unit::millisecond()),
    ];
    let mut options = UnitsFormatterOptions::default();
    options.width = Width::Long;
    let mut shown: Vec<String> = Vec::new();
    for (amount, unit) in parts {
        if amount == 0 {
            continue;
        }
        let formatter = CategorizedFormatter::try_new_core((&locale).into(), unit, options)
            .map_err(|_| Fault::Shape)?;
        let amount = fixed_decimal::Decimal::from(amount);
        shown.push(
            formatter
                .format_fixed_decimal(&amount)
                .write_to_string()
                .into_owned(),
        );
    }
    if shown.is_empty() {
        let formatter =
            CategorizedFormatter::try_new_core((&locale).into(), Unit::second(), options)
                .map_err(|_| Fault::Shape)?;
        let zero = fixed_decimal::Decimal::from(0u64);
        shown.push(
            formatter
                .format_fixed_decimal(&zero)
                .write_to_string()
                .into_owned(),
        );
    }
    let mut list = ListFormatterOptions::default();
    list.length = Some(ListLength::Wide);
    let joined = ListFormatter::try_new_unit((&locale).into(), list).map_err(|_| Fault::Shape)?;
    Ok(joined.format(shown.iter()).write_to_string().into_owned())
}

/// A weekday's stand-alone name in full (`FormatKey::WeekdayName`):
/// "Monday", from 0 for Monday.
pub fn weekday(ordinal: u32, tag: &str) -> Result<String, Fault> {
    // 2026-10-19 was a Monday: the weekday is that day's, moved on.
    let day = jadpo_std::date_number("2026-10-19").ok_or(Fault::Shape)?
        + i32::try_from(ordinal).map_err(|_| Fault::Shape)?;
    let spec = jadpo_std::TimeFormatSpec::Components(vec![(
        jadpo_std::Component::Weekday,
        jadpo_std::ComponentStyle::Long,
    )]);
    let moment = super::Moment {
        date: day,
        clock: 0,
        zone: None,
    };
    super::format(moment, tag, &spec)
}

/// A month's stand-alone name in full (`FormatKey::MonthName`):
/// "October", from 0 for January.
pub fn month(ordinal: u32, tag: &str) -> Result<String, Fault> {
    let month = i64::from(ordinal) + 1;
    let day = jadpo_std::epoch_day(2026, month, 1).ok_or(Fault::Shape)?;
    shown_parts(
        day,
        &[(jadpo_std::Component::Month, jadpo_std::ComponentStyle::Long)],
        tag,
    )
}

/// A `YearMonth` or a `Year` in CLDR's pattern for the parts it has
/// (`FormatKey::YearMonth`, `FormatKey::Year`): "October 2026", "2026".
pub fn year_month(value: &crate::Value, tag: &str) -> Result<String, Fault> {
    let crate::Value::Record(record) = value else {
        return Err(Fault::Shape);
    };
    let year = match record.get("year") {
        Some(crate::Value::Int(year)) => *year,
        _ => return Err(Fault::Shape),
    };
    let mut parts = vec![(
        jadpo_std::Component::Year,
        jadpo_std::ComponentStyle::Numeric,
    )];
    let month = match record.get("month") {
        Some(crate::Value::Int(month)) => {
            parts.push((jadpo_std::Component::Month, jadpo_std::ComponentStyle::Long));
            *month
        }
        _ => 1,
    };
    let day = jadpo_std::epoch_day(year, month, 1).ok_or(Fault::Shape)?;
    shown_parts(day, &parts, tag)
}

/// A day shown by a declared format of `parts`.
fn shown_parts(
    day: i32,
    parts: &[(jadpo_std::Component, jadpo_std::ComponentStyle)],
    tag: &str,
) -> Result<String, Fault> {
    let spec = jadpo_std::TimeFormatSpec::Components(parts.to_vec());
    let moment = super::Moment {
        date: day,
        clock: 0,
        zone: None,
    };
    super::format(moment, tag, &spec)
}

/// A `UtcOffset` in CLDR's localized GMT format, in full
/// (`FormatKey::OffsetName`): "GMT+01:00", "GMT+00:00".
pub fn offset(value: &crate::Value, tag: &str) -> Result<String, Fault> {
    let minutes = match value {
        crate::Value::Record(record) => match record.get("minutes") {
            Some(crate::Value::Int(minutes)) => *minutes,
            _ => return Err(Fault::Shape),
        },
        _ => return Err(Fault::Shape),
    };
    let reader = locale_of(tag)?;
    let seconds = i32::try_from(minutes * 60).map_err(|_| Fault::Shape)?;
    let offset = icu_time::zone::UtcOffset::try_from_seconds(seconds).map_err(|_| Fault::Shape)?;
    let formatter = NoCalendarFormatter::try_new(
        (&reader).into(),
        icu_datetime::fieldsets::zone::LocalizedOffsetLong,
    )
    .map_err(|_| Fault::Shape)?;
    Ok(formatter.format(&offset).write_to_string().into_owned())
}

/// A `CalendarPeriod` in its units, written out in full
/// (`FormatKey::PeriodUnits`): "1 year, 2 months, 10 days", each part that
/// isn't zero with its sign, joined as the locale joins a list of units;
/// nothing at all is "0 days".
pub fn period((years, months, days): (i64, i64, i64), tag: &str) -> Result<String, Fault> {
    let locale = locale_of(tag)?;
    let mut options = UnitsFormatterOptions::default();
    options.width = Width::Long;
    type UnitOf = fn() -> icu_experimental::measure::category::CategorizedMeasureUnit<Unit>;
    let one = |amount: i64, unit: UnitOf| -> Result<String, Fault> {
        // ICU4X sorts units' names into sets by how often they are used: a
        // day's are in the core set, a month's and a year's in the outlier
        // set, which is looked in after the other two.
        let formatter = CategorizedFormatter::try_new_outlier_unstable(
            &Silent,
            (&locale).into(),
            unit(),
            options,
        )
        .map_err(|_| Fault::Shape)?;
        let amount = fixed_decimal::Decimal::from(amount);
        Ok(formatter
            .format_fixed_decimal(&amount)
            .write_to_string()
            .into_owned())
    };
    let mut shown: Vec<String> = Vec::new();
    let units: [(i64, UnitOf); 3] = [
        (years, Unit::year),
        (months, Unit::month),
        (days, Unit::day),
    ];
    for (amount, unit) in units {
        if amount != 0 {
            shown.push(one(amount, unit)?);
        }
    }
    if shown.is_empty() {
        shown.push(one(0, Unit::day)?);
    }
    let mut list = ListFormatterOptions::default();
    list.length = Some(ListLength::Wide);
    let joined = ListFormatter::try_new_unit((&locale).into(), list).map_err(|_| Fault::Shape)?;
    Ok(joined.format(shown.iter()).write_to_string().into_owned())
}

/// ICU4X's compiled data, read without logging what a set of units'
/// names doesn't hold: a month's name is looked for in the core and
/// extended sets before the outlier set that has it, and a debug build
/// would print each miss.
struct Silent;

macro_rules! silently {
    ($baked:path: $($marker:path),*) => {
        $(
            impl icu_provider::DataProvider<$marker> for Silent {
                fn load(
                    &self,
                    mut req: icu_provider::DataRequest,
                ) -> Result<icu_provider::DataResponse<$marker>, icu_provider::DataError> {
                    req.metadata.silent = true;
                    icu_provider::DataProvider::<$marker>::load(&$baked, req)
                }
            }
        )*
    };
}

silently!(icu_experimental::provider::Baked:
    icu_experimental::dimension::provider::units::categorized_display_names::UnitsNamesDurationCoreV1,
    icu_experimental::dimension::provider::units::categorized_display_names::UnitsNamesDurationExtendedV1,
    icu_experimental::dimension::provider::units::categorized_display_names::UnitsNamesDurationOutlierV1
);
silently!(icu_decimal::provider::Baked:
    icu_decimal::provider::DecimalSymbolsV1,
    icu_decimal::provider::DecimalDigitsV1
);
silently!(icu_plurals::provider::Baked: icu_plurals::provider::PluralsCardinalV1);

/// A locale's display name (`FormatKey::LocaleName`): "British English"
/// for `en-GB`, in the reader's locale.
pub fn locale(shown: &str, tag: &str) -> Result<String, Fault> {
    let reader = locale_of(tag)?;
    let shown = locale_of(shown)?;
    let formatter =
        LocaleDisplayNamesFormatter::try_new((&reader).into(), DisplayNamesOptions::default())
            .map_err(|_| Fault::Shape)?;
    Ok(formatter.of(&shown).into_owned())
}

/// A zone's generic location name (`FormatKey::ZoneName`): "UK Time"
/// for `Europe/London`, "New York Time" for
/// `America/New_York`.
pub fn zone(iana: &str, tag: &str) -> Result<String, Fault> {
    let reader = locale_of(tag)?;
    let formatter =
        NoCalendarFormatter::try_new((&reader).into(), Location).map_err(|_| Fault::Shape)?;
    let zone = IanaParser::new().parse(iana).without_offset();
    Ok(formatter.format(&zone).write_to_string().into_owned())
}

/// A territory's name (`FormatKey::RegionName`, `ContinentName`): "United
/// Kingdom" for `GB`, "Europe" for UN M49's `150`, in the reader's locale;
/// `None` where CLDR has none.
pub fn region(code: &str, tag: &str) -> Result<Option<String>, Fault> {
    let reader = locale_of(tag)?;
    let Ok(region) = code.parse::<icu_locale::subtags::Region>() else {
        return Ok(None);
    };
    let names = RegionDisplayNames::try_new((&reader).into(), DisplayNamesOptions::default())
        .map_err(|_| Fault::Shape)?;
    Ok(names.of(region).map(str::to_string))
}

/// A language's name (`FormatKey::LanguageName`): "Japanese" for `ja`, in
/// the reader's locale; `None` where CLDR has none.
pub fn language(code: &str, tag: &str) -> Result<Option<String>, Fault> {
    let reader = locale_of(tag)?;
    let Ok(language) = code.parse::<icu_locale::subtags::Language>() else {
        return Ok(None);
    };
    let names = LanguageDisplayNames::try_new((&reader).into(), DisplayNamesOptions::default())
        .map_err(|_| Fault::Shape)?;
    Ok(names.of(language).map(str::to_string))
}

/// A script's name (`FormatKey::ScriptName`): "Latin" for `Latn`, in the
/// reader's locale; `None` where CLDR has none.
pub fn script(code: &str, tag: &str) -> Result<Option<String>, Fault> {
    let reader = locale_of(tag)?;
    let Ok(script) = code.parse::<icu_locale::subtags::Script>() else {
        return Ok(None);
    };
    let names = ScriptDisplayNames::try_new((&reader).into(), DisplayNamesOptions::default())
        .map_err(|_| Fault::Shape)?;
    Ok(names.of(script).map(str::to_string))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn places_and_languages_are_named_by_cldr() {
        assert_eq!(
            region("GB", "en-GB"),
            Ok(Some("United Kingdom".to_string()))
        );
        assert_eq!(region("GB", "fr-FR"), Ok(Some("Royaume-Uni".to_string())));
        assert_eq!(region("150", "en-GB"), Ok(Some("Europe".to_string())));
        assert_eq!(region("YU", "en-GB"), Ok(None));
        assert_eq!(language("ja", "en-GB"), Ok(Some("Japanese".to_string())));
        assert_eq!(language("or", "en-GB"), Ok(Some("Odia".to_string())));
        assert_eq!(script("Latn", "en-GB"), Ok(Some("Latin".to_string())));
        assert_eq!(script("Latn", "de-DE"), Ok(Some("Lateinisch".to_string())));
    }

    #[test]
    fn the_time_group_s_names_come_from_cldr() {
        assert_eq!(period((0, 0, 10), "en-GB"), Ok("10 days".to_string()));
        assert_eq!(period((0, 2, 0), "en-GB"), Ok("2 months".to_string()));
        assert_eq!(period((1, 0, 0), "en-GB"), Ok("1 year".to_string()));
        assert_eq!(
            period((1, 2, 10), "en-GB"),
            Ok("1 year, 2 months, 10 days".to_string())
        );
        assert_eq!(month(9, "en-GB"), Ok("October".to_string()));
    }

    #[test]
    fn names_come_from_cldr() {
        assert_eq!(duration(5_400_000, "en-GB").unwrap(), "1 hour, 30 minutes");
        assert_eq!(duration(0, "en-GB").unwrap(), "0 seconds");
        assert_eq!(weekday(0, "en-GB").unwrap(), "Monday");
        assert_eq!(weekday(6, "fr-FR").unwrap(), "dimanche");
        assert_eq!(locale("en-GB", "en-GB").unwrap(), "British English");
        assert_eq!(locale("fr-FR", "en-GB").unwrap(), "French (France)");
        assert_eq!(zone("Europe/London", "en-GB").unwrap(), "UK Time");
        assert_eq!(zone("America/New_York", "en-GB").unwrap(), "New York Time");
    }
}
