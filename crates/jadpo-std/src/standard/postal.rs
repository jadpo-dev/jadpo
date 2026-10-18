//! Postal codes (area 19, "Groups with the baseline only", chunk 5.5l2),
//! checked per country by libaddressinput's patterns (`postal_codes_data`,
//! generated once from the countryinfo.txt chunk 5.5l's address formats
//! come from).
//!
//! A `PostalCode` on its own is text any country's postal code could be:
//! capitals, digits, spaces and dashes, up to 12, a letter or digit at each
//! end. Its country's rule applies wherever its country is known: in a
//! `PostalAddress`, whose postal code fits its country, and in
//! `text.parse(raw, as: PostalCode, country:)`, which also reads it as
//! people type it (`sw1a1aa` is `SW1A 1AA`). The canonical text is the
//! country's own: in capitals, with a separator the country's pattern
//! leaves optional written as the country's examples write it, so one
//! postal code has one text and `==` holds. A country libaddressinput has
//! no pattern for takes any postal code (Ireland's Eircodes aren't in it).

use super::postal_codes_data::POSTAL_CODES;
use super::{
    Examples, FIRST_RELEASE, Format, FormatKey, Holds, Json, NO_OPTIONS, StandardType, Storage,
    TextForm,
};
use crate::read::SqlType;

/// What any country's postal code could be.
pub const ANY_POSTAL_CODE: &str = "[0-9A-Z](?:[0-9A-Z -]{0,10}[0-9A-Z])?";

/// The pattern the postal codes of the country with ISO 3166-1 code
/// `country` match whole, if libaddressinput has one.
pub fn pattern_of(country: &str) -> Option<&'static str> {
    POSTAL_CODES
        .iter()
        .find(|(code, _, _)| *code == country)
        .map(|(_, pattern, _)| *pattern)
}

/// Whether `text` matches the whole of `pattern`, each compiled once. Text
/// that isn't ASCII never does, so `\d` is an ASCII digit.
fn matches_whole(pattern: &'static str, text: &str) -> bool {
    text.is_ascii() && super::matches_whole(pattern, text)
}

/// Whether canonical `text` is a postal code of the country with code
/// `country`: its pattern, or any postal code where it has none.
pub fn fits(text: &str, country: &str) -> bool {
    matches_whole(ANY_POSTAL_CODE, text)
        && pattern_of(country).is_none_or(|pattern| matches_whole(pattern, text))
}

/// The canonical text of `raw` as people type a postal code of the country
/// with code `country`: in capitals, the spaces between words one each,
/// and the separators where the country writes them (`sw1a1aa` is `SW1A
/// 1AA`, `1540023` is `154-0023`); `None` unless that is one of its postal
/// codes, or where more than one reading is.
pub fn from_written(raw: &str, country: &str) -> Option<String> {
    let typed = raw
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_uppercase();
    if fits(&typed, country) {
        return Some(typed);
    }
    pattern_of(country)?;
    let compact: String = typed.chars().filter(|c| !matches!(c, ' ' | '-')).collect();
    let mut readings: Vec<String> = Vec::new();
    for at in 1..compact.len() {
        for separator in [" ", "-"] {
            let (head, tail) = (compact.get(..at)?, compact.get(at..)?);
            readings.push(format!("{head}{separator}{tail}"));
        }
    }
    readings.push(compact);
    let mut fitting = readings.into_iter().filter(|text| fits(text, country));
    match (fitting.next(), fitting.next()) {
        (Some(only), None) => Some(only),
        _ => None,
    }
}

/// A postal code: text any country's could be, checked by its country's
/// pattern where the country is known. Personal data (`contact`), as an
/// address is.
pub const POSTAL_CODE: StandardType = StandardType {
    name: "PostalCode",
    holds: Holds::Text {
        pattern: ANY_POSTAL_CODE,
        personal: Some("contact"),
    },
    text: Some(TextForm::Checked),
    json: Json::Text,
    storage: Storage::Column(SqlType::Text),
    format: Format {
        key: FormatKey::Canonical,
        options: NO_OPTIONS,
    },
    ordered: false,
    validator: None,
    shape: None,
    examples: Examples {
        valid: &[
            ("SW1A 1AA", "SW1A 1AA"),
            ("22162-1010", "22162-1010"),
            ("100-8994", "100-8994"),
        ],
        format: None,
        malformed: &[
            "sw1a 1aa",
            " SW1A 1AA",
            "SW1A 1AA ",
            "SW1A_1AA",
            "1234567890123",
            "",
        ],
        of: None,
        settings: "",
    },
    release: FIRST_RELEASE,
    signals: crate::standard::NO_SIGNALS,
};

/// A country's postal codes as the harness checks them: each as people
/// type it with its canonical text, and codes that aren't the country's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PostalExamples {
    /// The country's ISO 3166-1 code.
    pub country: &'static str,
    /// Typed, and canonical.
    pub valid: &'static [(&'static str, &'static str)],
    /// Not a postal code of the country, however it is typed.
    pub invalid: &'static [&'static str],
}

/// Valid and invalid postal codes for each country the harness checks:
/// libaddressinput's examples and its postal services' own, typed in
/// lower case or without their separators too.
pub const POSTAL_EXAMPLES: &[PostalExamples] = &[
    PostalExamples {
        country: "GB",
        valid: &[
            ("SW1A 1AA", "SW1A 1AA"),
            ("sw1a1aa", "SW1A 1AA"),
            ("EC1Y 8SY", "EC1Y 8SY"),
            ("m2  5bq", "M2 5BQ"),
            ("GIR 0AA", "GIR 0AA"),
        ],
        invalid: &["SW1A 1A", "QQ1 1AA", "SW1A 1AAA", "75004"],
    },
    PostalExamples {
        country: "US",
        valid: &[
            ("95014", "95014"),
            ("22162-1010", "22162-1010"),
            ("22162 1010", "22162-1010"),
            ("221621010", "22162-1010"),
        ],
        invalid: &["9501", "950141", "ABCDE", "22162-101"],
    },
    PostalExamples {
        country: "DE",
        valid: &[("26133", "26133"), ("53225", "53225")],
        invalid: &["2613", "261334", "D-26133"],
    },
    PostalExamples {
        country: "FR",
        valid: &[("33380", "33380"), ("75 004", "75004")],
        invalid: &["3338", "333800", "2A004"],
    },
    PostalExamples {
        country: "JP",
        valid: &[("154-0023", "154-0023"), ("1540023", "154-0023")],
        invalid: &["154-002", "154-00234", "ABC-DEFG"],
    },
    PostalExamples {
        country: "IN",
        valid: &[("110034", "110034"), ("110 001", "110001")],
        invalid: &["11003", "1100345", "11003A"],
    },
    PostalExamples {
        country: "BR",
        valid: &[("40301-110", "40301-110"), ("70002900", "70002-900")],
        invalid: &["40301-11", "403011100", "4030A-110"],
    },
    PostalExamples {
        country: "AU",
        valid: &[("2060", "2060"), ("3171", "3171")],
        invalid: &["206", "20600", "NSW 2060"],
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_country_s_pattern_compiles_and_holds_its_examples() {
        let mut previous = "";
        for (code, pattern, examples) in POSTAL_CODES {
            assert!(*code > previous, "sorted, once each");
            previous = code;
            assert!(crate::compile_pattern(pattern).is_ok(), "{code}");
            for example in *examples {
                assert!(fits(example, code), "{code}: {example}");
                assert_eq!(from_written(example, code).as_deref(), Some(*example));
            }
        }
    }

    #[test]
    fn a_country_with_no_pattern_takes_any_postal_code() {
        assert_eq!(pattern_of("IE"), None);
        assert!(fits("D02 X285", "IE"));
        assert!(!fits("d02 x285", "IE"));
        assert_eq!(from_written("d02 x285", "IE").as_deref(), Some("D02 X285"));
    }
}
