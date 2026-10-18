//! A quantity for people (area 19, "Physical quantities"): `text.format`
//! in a chosen unit, its kind's base unit by default. The number is the
//! value in that unit, exact where the conversion ends and otherwise to
//! the kind's scale, with no trailing zeros, written as the locale writes
//! numbers. Where ICU4X compiles CLDR's unit names (lengths, areas,
//! volumes and masses), the locale's short pattern places it, `2.5 m`,
//! `247,62 cm`; for the other kinds, which ICU4X has no names for yet, it
//! is the number, a space and the unit's symbol, as SI writes it: `21 °C`.

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
    let locale = locale_of(tag)?;
    let (mantissa, places) = shown_in(amount, unit).ok_or(Fault::DecimalOverflow)?;
    let written = jadpo_std::Num {
        mantissa,
        scale: places,
    }
    .to_string();
    let fixed: fixed_decimal::Decimal = written.parse().map_err(|_| Fault::Shape)?;
    let numbers = DecimalFormatter::try_new((&locale).into(), Default::default())
        .map_err(|_| Fault::Shape)?;
    let number = numbers.format(&fixed).write_to_string().into_owned();
    if let (Some(category), Some(id)) = (unit.kind.cldr_category(), unit.cldr)
        && let Some(names) = names(category, id, &locale)?
    {
        let rules = PluralRules::try_new_cardinal((&locale).into()).map_err(|_| Fault::Shape)?;
        let pattern = names.get().get((&fixed).into(), &rules);
        return Ok(pattern
            .interpolate((number,))
            .write_to_string()
            .into_owned());
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
        println!("{}", shown(K::Length, total, "centimetre", "en-GB"));
        println!("{}", shown(K::Length, total, "centimetre", "de-DE"));
        println!("{}", shown(K::Length, total, "centimetre", "fr-FR"));
        println!("{}", shown(K::Length, total, "metre", "en-GB"));
        println!("{}", shown(K::Length, "1000metre", "metre", "en-GB"));
        println!("{}", shown(K::Length, "1metre", "metre", "en-GB"));
        println!("{}", shown(K::Length, "3inch", "inch", "en-GB"));
        println!("{}", shown(K::Length, "3inch", "foot", "en-US"));
        println!("{}", shown(K::Length, "5km", "mile", "en-US"));
        println!("{}", shown(K::Area, "12sqm", "square_metre", "en-GB"));
        println!("{}", shown(K::Area, "2ha", "acre", "en-GB"));
        println!("{}", shown(K::Volume, "2litre", "cubic_metre", "en-GB"));
        println!("{}", shown(K::Volume, "500ml", "litre", "en-GB"));
        println!("{}", shown(K::Volume, "500ml", "millilitre", "en-GB"));
        println!("{}", shown(K::Volume, "1usgal", "us_gallon", "en-US"));
        println!("{}", shown(K::Volume, "1ukgal", "uk_gallon", "en-GB"));
        println!("{}", shown(K::Volume, "1floz", "fluid_ounce", "en-US"));
        println!("{}", shown(K::Mass, "2.5kg", "kilogram", "en-GB"));
        println!("{}", shown(K::Mass, "250g", "gram", "de-DE"));
        println!("{}", shown(K::Mass, "1lb", "pound", "en-US"));
        println!("{}", shown(K::Mass, "1oz", "ounce", "en-US"));
        println!("{}", shown(K::Mass, "1t", "tonne", "en-GB"));
        println!("{}", shown(K::Mass, "1mcg", "microgram", "en-GB"));
        println!("{}", shown(K::Temperature, "21degC", "celsius", "en-GB"));
        println!("{}", shown(K::Pressure, "1013.25hPa", "pascal", "de-DE"));
        println!("{}", shown(K::Length, "-1.5metre", "metre", "en-GB"));
        println!("{}", shown(K::Length, "0.0254metre", "metre", "en-GB"));
        println!("{}", shown(K::Area, "0.5sqm", "square_metre", "en-GB"));
        println!("{}", shown(K::Area, "10000sqm", "square_metre", "en-GB"));
        println!("{}", shown(K::Volume, "0.5litre", "cubic_metre", "en-GB"));
        println!("{}", shown(K::Volume, "1litre", "cubic_metre", "en-GB"));
        println!("{}", shown(K::Mass, "0.25kg", "kilogram", "en-GB"));
        println!("{}", shown(K::Mass, "1000kg", "kilogram", "en-GB"));
    }
}
