//! A quantity for people (area 19, "Physical quantities"): `text.format`
//! in a chosen unit, its kind's base unit by default. The number is the
//! value in that unit, exact where the conversion ends and otherwise to
//! the kind's scale, with no trailing zeros, written as the locale writes
//! numbers. Where ICU4X compiles CLDR's unit names (lengths, areas,
//! volumes and masses), the locale's short pattern places it, `2.5 m`,
//! `247,62 cm`; for the other kinds, which ICU4X has no names for yet, it
//! is the number, a space and the unit's symbol, as SI writes it: `21 °C`,
//! and `90°` for the degree of angle, which takes no space.

use icu_decimal::DecimalFormatter;
use icu_experimental::dimension::provider::units::categorized_display_names::{
    UnitsNamesAreaCoreV1, UnitsNamesAreaExtendedV1, UnitsNamesAreaOutlierV1,
    UnitsNamesLengthCoreV1, UnitsNamesLengthExtendedV1, UnitsNamesLengthOutlierV1,
    UnitsNamesMassCoreV1, UnitsNamesMassExtendedV1, UnitsNamesMassOutlierV1,
    UnitsNamesVolumeCoreV1, UnitsNamesVolumeExtendedV1, UnitsNamesVolumeOutlierV1,
};
use icu_experimental::dimension::provider::units::display_names::UnitsDisplayNames;
use icu_locale::Locale;
use icu_plurals::PluralRules;
use icu_provider::marker::ErasedMarker;
use icu_provider::prelude::*;
use jadpo_std::quantity::{CldrCategory, Unit, shown_in};
use writeable::Writeable;

use super::locale_of;
use crate::Fault;

/// `amount`, held in `unit`'s kind's base unit, in `unit`, for people in
/// the locale tagged `tag`.
pub fn format(amount: i128, unit: &Unit, tag: &str) -> Result<String, Fault> {
    format_at(amount, unit, None, tag)
}

/// The same, rounded half to even to `places` places where given, with no
/// trailing zeros (chunk 5.5m2): a heart rate in whole beats a minute.
pub fn format_at(
    amount: i128,
    unit: &Unit,
    places: Option<u32>,
    tag: &str,
) -> Result<String, Fault> {
    let (number, fixed) = number_in(amount, unit, places, tag)?;
    with_unit(&number, &fixed, unit, tag)
}

/// `amount`'s number in `unit`, as the locale writes numbers, with the
/// decimal it is: exact as [`shown_in`] gives it, or rounded half to even
/// to `places` places where given, with no trailing zeros.
pub fn number_in(
    amount: i128,
    unit: &Unit,
    places: Option<u32>,
    tag: &str,
) -> Result<(String, fixed_decimal::Decimal), Fault> {
    let locale = locale_of(tag)?;
    let (mut mantissa, mut scale) = match places {
        None => shown_in(amount, unit).ok_or(Fault::DecimalOverflow)?,
        Some(places) => (
            jadpo_std::quantity::from_base(amount, unit, places, jadpo_std::Rounding::HalfEven)
                .ok_or(Fault::DecimalOverflow)?,
            places,
        ),
    };
    while scale > 0 && mantissa % 10 == 0 {
        mantissa /= 10;
        scale -= 1;
    }
    let written = jadpo_std::Num { mantissa, scale }.to_string();
    let fixed: fixed_decimal::Decimal = written.parse().map_err(|_| Fault::Shape)?;
    let numbers = DecimalFormatter::try_new((&locale).into(), Default::default())
        .map_err(|_| Fault::Shape)?;
    let number = numbers.format(&fixed).write_to_string().into_owned();
    Ok((number, fixed))
}

/// `number`, the locale's text of the decimal `fixed` or of several joined,
/// with `unit` placed as the locale places it: CLDR's short pattern where
/// ICU4X has one, chosen by `fixed`'s plural, else a space and the unit's
/// symbol.
pub fn with_unit(
    number: &str,
    fixed: &fixed_decimal::Decimal,
    unit: &Unit,
    tag: &str,
) -> Result<String, Fault> {
    let locale = locale_of(tag)?;
    if let (Some(category), Some(id)) = (unit.kind.cldr_category(), unit.cldr)
        && let Some(names) = names(category, id, &locale)?
    {
        let rules = PluralRules::try_new_cardinal((&locale).into()).map_err(|_| Fault::Shape)?;
        let pattern = names.get().get(fixed.into(), &rules);
        return Ok(pattern
            .interpolate((number,))
            .write_to_string()
            .into_owned());
    }
    // SI writes a space between a number and its unit, but none before
    // the degree of angle.
    if unit.symbol == "°" {
        return Ok(format!("{number}°"));
    }
    Ok(format!("{number} {}", unit.symbol))
}

/// CLDR's short names for unit `id` in `category`, in the locale or the
/// nearest it falls back to; `None` where CLDR has none.
fn names(
    category: CldrCategory,
    id: &str,
    locale: &Locale,
) -> Result<Option<DataPayload<ErasedMarker<UnitsDisplayNames<'static>>>>, Fault> {
    let attribute = format!("short-{id}");
    let attributes = DataMarkerAttributes::try_from_str(&attribute).map_err(|_| Fault::Shape)?;
    let data_locale: DataLocale = locale.into();
    let mut request = DataRequest {
        id: DataIdentifierBorrowed::for_marker_attributes_and_locale(attributes, &data_locale),
        ..Default::default()
    };
    // A unit is looked for in each of its category's tiers in turn; one
    // missing from the first isn't a problem to report.
    request.metadata.silent = true;
    let baked = &icu_experimental::provider::Baked;
    macro_rules! first {
        ($($marker:ty),*) => {{
            $(
                let loaded: Option<DataResponse<$marker>> =
                    DataProvider::<$marker>::load(baked, request)
                        .allow_identifier_not_found()
                        .map_err(|_| Fault::Shape)?;
                if let Some(response) = loaded {
                    return Ok(Some(response.payload.cast()));
                }
            )*
            Ok(None)
        }};
    }
    match category {
        CldrCategory::Length => first!(
            UnitsNamesLengthCoreV1,
            UnitsNamesLengthExtendedV1,
            UnitsNamesLengthOutlierV1
        ),
        CldrCategory::Area => first!(
            UnitsNamesAreaCoreV1,
            UnitsNamesAreaExtendedV1,
            UnitsNamesAreaOutlierV1
        ),
        CldrCategory::Volume => first!(
            UnitsNamesVolumeCoreV1,
            UnitsNamesVolumeExtendedV1,
            UnitsNamesVolumeOutlierV1
        ),
        CldrCategory::Mass => first!(
            UnitsNamesMassCoreV1,
            UnitsNamesMassExtendedV1,
            UnitsNamesMassOutlierV1
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jadpo_std::QuantityKind;

    fn shown(kind: QuantityKind, literal: &str, unit: &str, tag: &str) -> String {
        let (_, amount) = jadpo_std::quantity::parse_literal(literal).unwrap();
        format(amount, kind.unit_named(unit).unwrap(), tag).unwrap()
    }

    #[test]
    fn every_cldr_unit_has_names() {
        for unit in jadpo_std::quantity::UNITS {
            if let (Some(category), Some(id)) = (unit.kind.cldr_category(), unit.cldr) {
                let locale = locale_of("en-GB").unwrap();
                assert!(names(category, id, &locale).unwrap().is_some(), "{id}");
            }
        }
    }

    #[test]
    fn quantities_are_written_as_the_locale_writes_them() {
        use QuantityKind as K;
        let total = "2.5262metre";
        assert_eq!(shown(K::Length, total, "centimetre", "en-GB"), "252.62 cm");
        assert_eq!(shown(K::Length, total, "centimetre", "de-DE"), "252,62 cm");
        assert_eq!(shown(K::Length, total, "metre", "en-GB"), "2.5262 m");
        assert_eq!(shown(K::Length, "1000metre", "metre", "en-GB"), "1,000 m");
        assert_eq!(shown(K::Length, "3inch", "inch", "en-GB"), "3 in");
        assert_eq!(shown(K::Length, "5km", "mile", "en-US"), "3.10685596 mi");
        assert_eq!(shown(K::Volume, "500ml", "litre", "en-GB"), "0.5 l");
        assert_eq!(shown(K::Volume, "1floz", "fluid_ounce", "en-US"), "1 fl oz");
        assert_eq!(shown(K::Mass, "250g", "gram", "de-DE"), "250 g");
        assert_eq!(shown(K::Mass, "1lb", "pound", "en-US"), "1 lb");
        assert_eq!(shown(K::Temperature, "21degC", "celsius", "en-GB"), "21 °C");
        assert_eq!(
            shown(K::Temperature, "70degF", "fahrenheit", "en-US"),
            "70 °F"
        );
        assert_eq!(
            shown(K::Pressure, "1013.25hPa", "pascal", "de-DE"),
            "101.325 Pa"
        );
        assert_eq!(shown(K::Angle, "90deg", "degree", "en-GB"), "90°");
    }
}
