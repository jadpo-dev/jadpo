//! The closed lists of places and languages (area 19, "The standard types",
//! question 22): each a built-in enum whose variants are its authority's
//! codes in lower case (`Country.gb`, `CountrySubdivision.gb_eng`), a code
//! that is a reserved word or `all` taking a trailing `_` (`Country.in_`,
//! `Language.or_`), read ignoring case and written as the code (`GB`).
//! Other code systems are forms (`form: CodeForm.alpha3`), each a list of
//! codes in the variants' order. A code its authority withdraws stays a
//! variant, retired: stored data and old messages still read it, and new
//! code that writes one is `type.variant_retired`.
//!
//! The lists ship with each release, as the time-zone database does, from
//! `places_data` (generated once from CLDR 48.2, the Library of Congress's
//! ISO 639-2 list and ISO 15924's list; its header says how).

use super::places_data::{
    COUNTRIES, COUNTRIES_RETIRED, COUNTRY_ALPHA3, COUNTRY_NUMERIC, LANGUAGE_ALPHA3, LANGUAGES,
    LANGUAGES_RETIRED, SCRIPT_NUMERIC, SCRIPTS, SUBDIVISIONS, SUBDIVISIONS_RETIRED,
};
use super::{
    Examples, FIRST_RELEASE, Format, FormatKey, Holds, Json, NO_OPTIONS, StandardType, Storage,
    TextForm,
};
use crate::CodeForm;
use crate::read::SqlType;

/// A closed list's other code systems and retired variants (question 22).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClosedList {
    /// The standard enum it is.
    pub ty: &'static str,
    /// The authority that publishes it, as an error names it: `ISO 3166-1`.
    pub authority: &'static str,
    /// Each other code system with the code of every variant, in the
    /// variants' order: what `form:` reads and writes.
    pub forms: &'static [(CodeForm, &'static [&'static str])],
    /// The variants whose codes were withdrawn, each with an English name
    /// for one CLDR doesn't name (empty where CLDR does).
    pub retired: &'static [(&'static str, &'static str)],
}

/// `Country` (ISO 3166-1).
pub const COUNTRY: &str = "Country";
/// `Nationality`: a person's, named by the country (ISO 3166-1).
pub const NATIONALITY: &str = "Nationality";
/// `CountrySubdivision` (ISO 3166-2).
pub const COUNTRY_SUBDIVISION: &str = "CountrySubdivision";
/// `Continent` (UN M49's regions).
pub const CONTINENT: &str = "Continent";
/// `Language` (ISO 639-1).
pub const LANGUAGE: &str = "Language";
/// `WritingScript` (ISO 15924).
pub const WRITING_SCRIPT: &str = "WritingScript";
/// `TextDirection`, as HTML's `dir` writes it.
pub const TEXT_DIRECTION: &str = "TextDirection";

/// `Continent`'s variants: UN M49's five regions and Antarctica, which M49
/// lists beside them. Its codes are digits, so each variant's code is its
/// own name, as a `Weekday`'s is, and M49's number is its numeric form.
pub const CONTINENTS: &[(&str, &str)] = &[
    ("africa", "africa"),
    ("americas", "americas"),
    ("antarctica", "antarctica"),
    ("asia", "asia"),
    ("europe", "europe"),
    ("oceania", "oceania"),
];

/// Each continent's UN M49 number, in `CONTINENTS`' order.
pub const CONTINENT_NUMERIC: &[&str] = &["002", "019", "010", "142", "150", "009"];

/// The CLDR region each continent is named by, in `CONTINENTS`' order: its
/// M49 region, or Antarctica's territory, which CLDR names instead.
pub const CONTINENT_REGIONS: &[&str] = &["002", "019", "AQ", "142", "150", "009"];

/// `TextDirection`'s variants, in `crate::TextDirection`'s order, with the
/// codes HTML's `dir` attribute takes (area 10's wording of them).
pub const TEXT_DIRECTIONS: &[(&str, &str)] = &[("left_to_right", "ltr"), ("right_to_left", "rtl")];

/// Every closed list with forms or retired codes.
pub const CLOSED_LISTS: &[ClosedList] = &[
    ClosedList {
        ty: crate::CURRENCY,
        authority: "ISO 4217",
        forms: &[],
        retired: crate::money::CURRENCIES_RETIRED,
    },
    ClosedList {
        ty: COUNTRY,
        authority: "ISO 3166-1",
        forms: &[
            (CodeForm::Alpha3, COUNTRY_ALPHA3),
            (CodeForm::Numeric, COUNTRY_NUMERIC),
        ],
        retired: COUNTRIES_RETIRED,
    },
    ClosedList {
        ty: NATIONALITY,
        authority: "ISO 3166-1",
        forms: &[
            (CodeForm::Alpha3, COUNTRY_ALPHA3),
            (CodeForm::Numeric, COUNTRY_NUMERIC),
        ],
        retired: COUNTRIES_RETIRED,
    },
    ClosedList {
        ty: COUNTRY_SUBDIVISION,
        authority: "ISO 3166-2",
        forms: &[],
        retired: SUBDIVISIONS_RETIRED,
    },
    ClosedList {
        ty: CONTINENT,
        authority: "UN M49",
        forms: &[(CodeForm::Numeric, CONTINENT_NUMERIC)],
        retired: &[],
    },
    ClosedList {
        ty: LANGUAGE,
        authority: "ISO 639-1",
        forms: &[(CodeForm::Alpha3, LANGUAGE_ALPHA3)],
        retired: LANGUAGES_RETIRED,
    },
    ClosedList {
        ty: WRITING_SCRIPT,
        authority: "ISO 15924",
        forms: &[(CodeForm::Numeric, SCRIPT_NUMERIC)],
        retired: &[],
    },
    ClosedList {
        ty: TEXT_DIRECTION,
        authority: "HTML",
        forms: &[],
        retired: &[],
    },
];

/// The closed list `ty` names, if it is one.
pub fn closed_list(ty: &str) -> Option<&'static ClosedList> {
    CLOSED_LISTS.iter().find(|list| list.ty == ty)
}

/// Whether variant `variant` of enum `ty` is retired, with its English
/// name where CLDR has none.
pub fn retired(ty: &str, variant: &str) -> Option<&'static str> {
    closed_list(ty)?
        .retired
        .iter()
        .find(|(name, _)| *name == variant)
        .map(|(_, english)| *english)
}

/// The English name CLDR gives subdivision `ordinal`, if any.
pub fn subdivision_name(ordinal: u32) -> Option<&'static str> {
    super::places_data::SUBDIVISION_NAMES
        .get(ordinal as usize)
        .copied()
        .filter(|name| !name.is_empty())
}

/// A closed list held as an enum, written as its code and shown by `key`.
const fn closed(
    name: &'static str,
    codes: &'static [(&'static str, &'static str)],
    key: FormatKey,
    examples: Examples,
) -> StandardType {
    StandardType {
        name,
        holds: Holds::Enum(codes),
        text: Some(TextForm::Code),
        json: Json::Text,
        storage: Storage::Column(SqlType::Text),
        format: Format {
            key,
            options: NO_OPTIONS,
        },
        ordered: false,
        validator: None,
        shape: None,
        examples,
        release: FIRST_RELEASE,
    }
}

const fn examples(
    valid: &'static [(&'static str, &'static str)],
    malformed: &'static [&'static str],
) -> Examples {
    Examples {
        valid,
        format: None,
        malformed,
        of: None,
        settings: "",
    }
}

/// A country, as ISO 3166-1 codes it: shown by CLDR's name for it; a
/// withdrawn code (Yugoslavia's `YU`) by ISO 3166-3's.
pub const COUNTRY_ENTRY: StandardType = closed(
    COUNTRY,
    COUNTRIES,
    FormatKey::RegionName,
    examples(
        &[
            ("GB", "United Kingdom"),
            ("IN", "India"),
            ("JP", "Japan"),
            ("YU", "Yugoslavia"),
        ],
        &["GBR", "UK", "826", "Great Britain", "XK", "G B", ""],
    ),
);

/// A country's principal subdivision, as ISO 3166-2 codes it: shown by
/// CLDR's English name, which is all CLDR's data names in most languages.
pub const COUNTRY_SUBDIVISION_ENTRY: StandardType = closed(
    COUNTRY_SUBDIVISION,
    SUBDIVISIONS,
    FormatKey::SubdivisionName,
    examples(
        &[
            ("GB-ENG", "England"),
            ("JP-13", "Tokyo"),
            ("US-CA", "California"),
        ],
        &["GB", "ENG", "GB_ENG", "GBENG", "XX-YY", "England", ""],
    ),
);

/// A continent, as UN M49's regions are: shown by CLDR's name for it.
pub const CONTINENT_ENTRY: StandardType = closed(
    CONTINENT,
    CONTINENTS,
    FormatKey::ContinentName,
    examples(
        &[("antarctica", "Antarctica"), ("europe", "Europe")],
        &["Europa", "150", "north_america", ""],
    ),
);

/// A language, as ISO 639-1 codes it: shown by CLDR's name for it.
pub const LANGUAGE_ENTRY: StandardType = closed(
    LANGUAGE,
    LANGUAGES,
    FormatKey::LanguageName,
    examples(
        &[
            ("en", "English"),
            ("iw", "Hebrew"),
            ("ja", "Japanese"),
            ("or", "Odia"),
        ],
        &["eng", "en-GB", "english", "xx", ""],
    ),
);

/// A person's nationality, named by the country (ISO 3166-1), as a
/// passport's is: shown by CLDR's name for the country, since CLDR has no
/// names for nationalities.
pub const NATIONALITY_ENTRY: StandardType = closed(
    NATIONALITY,
    COUNTRIES,
    FormatKey::RegionName,
    examples(
        &[("FR", "France"), ("GB", "United Kingdom")],
        &["GBR", "British", "UK", ""],
    ),
);

/// Which way text runs, as HTML's `dir` attribute writes it.
pub const TEXT_DIRECTION_ENTRY: StandardType = closed(
    TEXT_DIRECTION,
    TEXT_DIRECTIONS,
    FormatKey::Canonical,
    examples(
        &[("ltr", "ltr"), ("rtl", "rtl")],
        &["left_to_right", "auto", ""],
    ),
);

/// A writing system, as ISO 15924 codes it: shown by CLDR's name for it.
pub const WRITING_SCRIPT_ENTRY: StandardType = closed(
    WRITING_SCRIPT,
    SCRIPTS,
    FormatKey::ScriptName,
    examples(
        &[("Jpan", "Japanese"), ("Latn", "Latin"), ("Zyyy", "Common")],
        &["latin", "Lat", "Latnn", "Qaaa", ""],
    ),
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_form_has_a_code_for_every_variant() {
        for list in CLOSED_LISTS {
            let entry = super::super::standard_type(list.ty).expect("a standard type");
            let codes = entry.codes().expect("an enum");
            for (form, listed) in list.forms {
                assert_eq!(listed.len(), codes.len(), "{}'s {form:?}", list.ty);
            }
            for (variant, _) in list.retired {
                assert!(
                    codes.iter().any(|(name, _)| name == variant),
                    "{}.{variant} is a variant",
                    list.ty
                );
            }
        }
        assert_eq!(CONTINENT_NUMERIC.len(), CONTINENTS.len());
        assert_eq!(CONTINENT_REGIONS.len(), CONTINENTS.len());
        let names: Vec<&str> = TEXT_DIRECTIONS.iter().map(|(name, _)| *name).collect();
        assert_eq!(names, crate::TextDirection::NAMES);
        assert_eq!(
            super::super::places_data::SUBDIVISION_NAMES.len(),
            SUBDIVISIONS.len()
        );
    }

    #[test]
    fn codes_are_the_authorities_and_sorted() {
        for codes in [COUNTRIES, SUBDIVISIONS, LANGUAGES, SCRIPTS] {
            let written: Vec<&str> = codes.iter().map(|(_, code)| *code).collect();
            let mut sorted = written.clone();
            sorted.sort_unstable();
            sorted.dedup();
            assert_eq!(written, sorted, "sorted by code, each once");
            for (variant, code) in codes {
                assert_eq!(
                    variant.trim_end_matches('_'),
                    code.to_ascii_lowercase().replace('-', "_"),
                    "{code}"
                );
            }
        }
        // ISO 3166-1's 249 and the 11 withdrawn codes still read.
        assert_eq!(COUNTRIES.len(), 249 + COUNTRIES_RETIRED.len());
        assert_eq!(retired(COUNTRY, "yu"), Some("Yugoslavia"));
        assert_eq!(retired(COUNTRY, "gb"), None);
        assert_eq!(retired(crate::CURRENCY, "hrk"), Some(""));
        assert_eq!(retired(LANGUAGE, "in_"), Some("Indonesian"));
        let gb = COUNTRIES
            .iter()
            .position(|(name, _)| *name == "gb")
            .unwrap();
        assert_eq!((COUNTRY_ALPHA3[gb], COUNTRY_NUMERIC[gb]), ("GBR", "826"));
        let eng = SUBDIVISIONS
            .iter()
            .position(|(name, _)| *name == "gb_eng")
            .unwrap();
        assert_eq!(subdivision_name(eng as u32), Some("England"));
    }
}
