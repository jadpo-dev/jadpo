//! The names CLDR gives standard values for people (area 19, "Every
//! standard type meets one contract"): a duration in its units, a weekday,
//! a locale and a zone, each in the reader's locale, as `text.format`
//! writes them for the format keys that read them.

use icu_datetime::NoCalendarFormatter;
use icu_datetime::fieldsets::zone::Location;
use icu_experimental::displaynames::DisplayNamesOptions;
use icu_experimental::displaynames::multi::LocaleDisplayNamesFormatter;
use icu_experimental::dimension::units::categorized_formatter::CategorizedFormatter;
use icu_experimental::dimension::units::options::{UnitsFormatterOptions, Width};
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
        shown.push(formatter.format_fixed_decimal(&amount).write_to_string().into_owned());
    }
    if shown.is_empty() {
        let formatter =
            CategorizedFormatter::try_new_core((&locale).into(), Unit::second(), options)
                .map_err(|_| Fault::Shape)?;
        let zero = fixed_decimal::Decimal::from(0u64);
        shown.push(formatter.format_fixed_decimal(&zero).write_to_string().into_owned());
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

/// A locale's display name (`FormatKey::LocaleName`): "British English"
/// for `en-GB`, in the reader's locale.
pub fn locale(shown: &str, tag: &str) -> Result<String, Fault> {
    let reader = locale_of(tag)?;
    let shown = locale_of(shown)?;
    let formatter = LocaleDisplayNamesFormatter::try_new((&reader).into(), DisplayNamesOptions::default())
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

#[cfg(test)]
mod tests {
    use super::*;

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
