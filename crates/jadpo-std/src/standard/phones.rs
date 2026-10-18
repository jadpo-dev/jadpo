//! Phone numbers and calling codes (area 19, "Groups with the baseline
//! only", chunk 5.5l2), from libphonenumber's metadata (`phones_data`,
//! generated once from its `PhoneNumberMetadata.xml`; its header says
//! which release).
//!
//! A `PhoneNumber` is held as its E.164 text, `+447400123456`: a calling
//! code and the national number, with nothing else. It is valid as
//! libphonenumber's `isValidNumber` says, which is stricter than "possible"
//! (a length the region uses): the national number matches one of its
//! region's number types (a fixed line, a mobile, a freephone ...), each
//! with its own lengths, so a number in a range no operator was given is
//! refused until a release's metadata adds it, as a currency is.
//! `text.parse` reads only that text, but given a `country:` it reads a
//! number as people write it there, national (`07400 123456`) or
//! international (`+44 7400 123456`, `00 44 ...`). `text.format` shows it
//! in the national format to a reader in a region with the same calling
//! code and in the international format to anyone else, by
//! libphonenumber's formats for the code's main region.
//!
//! A `CallingCode` is a code ITU-T assigned (E.164), with its `+`: `+44`,
//! the non-geographic codes (`+800`) among them.

use super::phones_data::REGIONS;
use super::{
    Examples, FIRST_RELEASE, Format, FormatKey, Holds, Json, NO_OPTIONS, StandardType, Storage,
    TextForm, Validator,
};
use crate::read::SqlType;

/// One region's numbering plan as libphonenumber holds it: what validates,
/// parses and formats a number of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Region {
    /// Its ISO 3166-1 alpha-2 code, or `001` for a non-geographic code.
    pub id: &'static str,
    /// Its calling code's digits, `44`.
    pub calling: &'static str,
    /// Whether its formats are the code's: the main region of a code
    /// regions share (`US` for `+1`).
    pub main: bool,
    /// For a region sharing its code, the national numbers that begin
    /// with this are its own (`JE`'s); empty when its number types say.
    pub leading: &'static str,
    /// What is dialled to call abroad from it, a pattern: `00`, `011`.
    pub idd: &'static str,
    /// Its national prefix, written before a national number: `0`.
    pub prefix: &'static str,
    /// A pattern for the national prefix as it is read, with carrier
    /// codes; empty where it is `prefix`.
    pub parsing: &'static str,
    /// What a number whose prefix `parsing` matched becomes, its groups
    /// written `${1}`; empty where the prefix is just left out.
    pub transform: &'static str,
    /// The pattern every national number of it matches.
    pub general: &'static str,
    /// Each number type's pattern with its national lengths.
    pub types: &'static [(&'static str, &'static [u8])],
    /// How its numbers are written, the first that fits.
    pub formats: &'static [NumberFormat],
}

/// One of libphonenumber's number formats.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NumberFormat {
    /// The national numbers it writes, matched whole, its groups the
    /// parts.
    pub pattern: &'static str,
    /// The start of the numbers it writes, a pattern; empty for any.
    pub leading: &'static str,
    /// The national form, with the national prefix where it is written:
    /// `0${1} ${2}`.
    pub national: &'static str,
    /// The international form after the calling code, `${1} ${2}`; empty
    /// for numbers not dialled from abroad.
    pub international: &'static str,
}

/// A compiled pattern from the metadata: matched whole, or from the start
/// (`lookingAt`), each compiled once.
fn pattern(source: &'static str, whole: bool) -> Option<regex::Regex> {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    type Cache = Mutex<HashMap<(&'static str, bool), Option<regex::Regex>>>;
    static COMPILED: OnceLock<Cache> = OnceLock::new();
    let mut compiled = COMPILED.get_or_init(Mutex::default).lock().ok()?;
    compiled
        .entry((source, whole))
        .or_insert_with(|| {
            let anchored = if whole {
                format!("^(?:{source})$")
            } else {
                format!("^(?:{source})")
            };
            regex::Regex::new(&anchored).ok()
        })
        .clone()
}

/// Whether `digits` match `source` whole.
fn matches(source: &'static str, digits: &str) -> bool {
    pattern(source, true).is_some_and(|re| re.is_match(digits))
}

/// Whether `digits` begin with what `source` matches.
fn begins(source: &'static str, digits: &str) -> bool {
    pattern(source, false).is_some_and(|re| re.is_match(digits))
}

/// The regions a calling code's digits name, the main one first.
fn regions_of(calling: &str) -> impl Iterator<Item = &'static Region> {
    let calling = calling.to_string();
    REGIONS
        .iter()
        .filter(move |region| region.calling == calling)
}

/// The region with ISO 3166-1 code `id`, if libphonenumber holds one.
pub fn region(id: &str) -> Option<&'static Region> {
    REGIONS.iter().find(|region| region.id == id)
}

/// Whether `calling`'s digits are a calling code ITU-T assigned.
pub fn is_assigned(calling: &str) -> bool {
    regions_of(calling).next().is_some()
}

/// An E.164 text's calling code and national number; `None` for text
/// that isn't `+` and digits or has no assigned code. Calling codes are
/// prefix-free, so the first that is assigned is the number's.
pub fn split(text: &str) -> Option<(&'static str, &str)> {
    let digits = text.strip_prefix('+')?;
    if !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    (1..=3).find_map(|len| {
        let calling = digits.get(..len)?;
        let main = regions_of(calling).next()?;
        Some((main.calling, digits.get(len..)?))
    })
}

/// Whether national number `digits` is one of `region`'s number types:
/// its general pattern, then a type's pattern and lengths
/// (libphonenumber's `getNumberType` isn't `UNKNOWN`).
fn has_type(region: &Region, digits: &str) -> bool {
    matches(region.general, digits)
        && region.types.iter().any(|(source, lengths)| {
            lengths
                .iter()
                .any(|length| usize::from(*length) == digits.len())
                && matches(source, digits)
        })
}

/// The region a national number of calling code `calling` is from: the
/// first whose leading digits begin it or, with none, whose number types
/// hold it (libphonenumber's `getRegionCodeForNumber`).
fn region_for(calling: &str, digits: &str) -> Option<&'static Region> {
    regions_of(calling).find(|region| {
        if region.leading.is_empty() {
            has_type(region, digits)
        } else {
            begins(region.leading, digits)
        }
    })
}

/// Whether `text` is a valid E.164 number: an assigned code and a national
/// number its region's metadata says is one of its number types, with at
/// most 15 digits in all.
pub fn is_valid(text: &str) -> bool {
    let Some((calling, digits)) = split(text) else {
        return false;
    };
    calling.len() + digits.len() <= 15
        && region_for(calling, digits).is_some_and(|region| has_type(region, digits))
}

/// The E.164 text of `raw` as people write a number in the region with
/// ISO 3166-1 code `country`: with its national prefix (`07400 123456`), or
/// international, after a `+` or what the region dials to call abroad,
/// with spaces, dashes, dots, slashes and brackets anywhere; `None` unless
/// the number is valid.
pub fn from_written(raw: &str, country: &str) -> Option<String> {
    let region = region(country)?;
    let trimmed = raw.trim();
    let (plus, rest) = match trimmed.strip_prefix('+') {
        Some(rest) => (true, rest),
        None => (false, trimmed),
    };
    let mut digits = String::new();
    for c in rest.chars() {
        match c {
            '0'..='9' => digits.push(c),
            ' ' | '-' | '.' | '/' | '(' | ')' | '\u{a0}' => {}
            _ => return None,
        }
    }
    if plus {
        return international_written(&digits);
    }
    // Dialled abroad from the region: the rest begins with a calling code.
    if let Some(found) = pattern(region.idd, false).and_then(|re| re.find(&digits))
        && found.end() > 0
        && let Some(after) = digits.get(found.end()..)
        && !after.starts_with('0')
    {
        return international_written(after);
    }
    in_region(region, &digits)
}

/// The E.164 text of a calling code and national number written as
/// `digits`, a national prefix written after the code (`+44 (0)7400 ...`)
/// left out as its main region reads one.
fn international_written(digits: &str) -> Option<String> {
    let (calling, national) =
        split(&format!("+{digits}")).map(|(calling, national)| (calling, national.to_string()))?;
    in_region(regions_of(calling).next()?, &national)
}

/// The E.164 text of national number `digits` of `region`, its national
/// prefix left out, or as written where that is what is valid.
fn in_region(region: &Region, digits: &str) -> Option<String> {
    [strip_prefix(region, digits), digits.to_string()]
        .into_iter()
        .map(|national| format!("+{}{national}", region.calling))
        .find(|text| is_valid(text))
}

/// A national number with the region's national prefix left out, or
/// changed as its transform says (libphonenumber's
/// `maybeStripNationalPrefixAndCarrierCode`); as it was when it has none,
/// or when leaving it out would make a number the region holds one it
/// doesn't.
fn strip_prefix(region: &Region, digits: &str) -> String {
    let source = if region.parsing.is_empty() {
        region.prefix
    } else {
        region.parsing
    };
    if source.is_empty() {
        return digits.to_string();
    }
    let Some(re) = pattern(source, false) else {
        return digits.to_string();
    };
    let Some(captures) = re.captures(digits) else {
        return digits.to_string();
    };
    let Some(whole) = captures.get(0) else {
        return digits.to_string();
    };
    let viable = matches(region.general, digits);
    let last = captures.len().saturating_sub(1);
    let changed = if region.transform.is_empty() || last == 0 || captures.get(last).is_none() {
        digits.get(whole.end()..).unwrap_or_default().to_string()
    } else {
        let mut out = String::new();
        captures.expand(region.transform, &mut out);
        out.push_str(digits.get(whole.end()..).unwrap_or_default());
        out
    };
    if viable && !matches(region.general, &changed) {
        digits.to_string()
    } else {
        changed
    }
}

/// National number `digits` written by the first of `formats` that fits
/// it, through `form` (a format's national or international form); `None`
/// when none does.
fn written(
    formats: &'static [NumberFormat],
    digits: &str,
    form: fn(&NumberFormat) -> &'static str,
) -> Option<String> {
    let chosen = formats.iter().find(|format| {
        !form(format).is_empty()
            && (format.leading.is_empty() || begins(format.leading, digits))
            && matches(format.pattern, digits)
    })?;
    let re = pattern(chosen.pattern, true)?;
    let captures = re.captures(digits)?;
    let mut out = String::new();
    captures.expand(form(chosen), &mut out);
    Some(out)
}

/// A valid number in its national format, `07400 123456`, by its code's
/// main region's formats; the national number as it is where none fits.
pub fn national(text: &str) -> Option<String> {
    let (calling, digits) = split(text)?;
    let main = regions_of(calling).next()?;
    Some(written(main.formats, digits, |f| f.national).unwrap_or_else(|| digits.to_string()))
}

/// A valid number in its international format, `+44 7400 123456`.
pub fn international(text: &str) -> Option<String> {
    let (calling, digits) = split(text)?;
    let main = regions_of(calling).next()?;
    let shown =
        written(main.formats, digits, |f| f.international).unwrap_or_else(|| digits.to_string());
    Some(format!("+{calling} {shown}"))
}

/// A number for a reader in region `reader` (an ISO 3166-1 code, `None`
/// for a locale with no region): national where the reader's region has
/// the number's calling code, international otherwise.
pub fn shown(text: &str, reader: Option<&str>) -> Option<String> {
    let (calling, _) = split(text)?;
    let home = reader.and_then(region).map(|region| region.calling);
    if home == Some(calling) {
        national(text)
    } else {
        international(text)
    }
}

/// A phone number in E.164 (ITU-T), `+447400123456`: valid by
/// libphonenumber's metadata for its region. Personal data (`contact`).
pub const PHONE_NUMBER: StandardType = StandardType {
    name: "PhoneNumber",
    holds: Holds::Text {
        pattern: r"\+[1-9][0-9]{1,14}",
        personal: Some("contact"),
    },
    text: Some(TextForm::Checked),
    json: Json::Text,
    storage: Storage::Column(SqlType::Text),
    format: Format {
        key: FormatKey::PhoneNumber,
        options: NO_OPTIONS,
    },
    ordered: false,
    validator: Some(Validator::PhoneNumber),
    shape: None,
    // libphonenumber's example numbers: a UK mobile, shown in `en_gb`'s
    // national format, and a US and a French number in the international
    // one.
    examples: Examples {
        valid: &[
            ("+447400123456", "07400 123456"),
            ("+12015550123", "+1 201-555-0123"),
            ("+33612345678", "+33 6 12 34 56 78"),
        ],
        format: None,
        malformed: &[
            "07400123456",
            "+44 7400 123456",
            "447400123456",
            "+441000123456",
            "+4474001234",
            "+999123456789",
            "+0447400123456",
            "",
        ],
        of: None,
        settings: "",
    },
    release: FIRST_RELEASE,
    signals: crate::standard::NO_SIGNALS,
};

/// A country calling code ITU-T assigned (E.164), with its `+`: `+44`,
/// `+1` (shared by the North American Numbering Plan), `+800`.
pub const CALLING_CODE: StandardType = StandardType {
    name: "CallingCode",
    holds: Holds::Text {
        pattern: r"\+[1-9][0-9]{0,2}",
        personal: None,
    },
    text: Some(TextForm::Checked),
    json: Json::Text,
    storage: Storage::Column(SqlType::Text),
    format: Format {
        key: FormatKey::Canonical,
        options: NO_OPTIONS,
    },
    ordered: false,
    validator: Some(Validator::CallingCode),
    shape: None,
    examples: Examples {
        valid: &[("+1", "+1"), ("+44", "+44"), ("+800", "+800")],
        format: None,
        malformed: &["44", "+0", "+999", "+4", "+44 ", "+0044", ""],
        of: None,
        settings: "",
    },
    release: FIRST_RELEASE,
    signals: crate::standard::NO_SIGNALS,
};

/// A number rule's examples for one country, the harness's: each as people
/// write it there with its E.164 text and how it is shown to a reader
/// there and to one in another region, and numbers written there that
/// aren't valid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PhoneExamples {
    /// The country's ISO 3166-1 code.
    pub country: &'static str,
    /// The tag of a locale of the country, its reader's.
    pub locale: &'static str,
    /// Written there, its E.164 text, national, international.
    pub valid: &'static [(&'static str, &'static str, &'static str, &'static str)],
    /// Written there, and not a valid number.
    pub invalid: &'static [&'static str],
}

/// Valid and invalid numbers for each country the harness checks, from
/// libphonenumber's example numbers (a fixed line and a mobile each);
/// the invalid are a digit short, a digit long or in a range no type has.
pub const PHONE_EXAMPLES: &[PhoneExamples] = &[
    PhoneExamples {
        country: "GB",
        locale: "en-GB",
        valid: &[
            (
                "0121 234 5678",
                "+441212345678",
                "0121 234 5678",
                "+44 121 234 5678",
            ),
            (
                "07400 123456",
                "+447400123456",
                "07400 123456",
                "+44 7400 123456",
            ),
            (
                "+44 (0)7400 123456",
                "+447400123456",
                "07400 123456",
                "+44 7400 123456",
            ),
            (
                "00 44 7400 123456",
                "+447400123456",
                "07400 123456",
                "+44 7400 123456",
            ),
        ],
        invalid: &[
            "07400 12345",
            "07400 1234567",
            "01000 123456",
            "04400 123456",
        ],
    },
    PhoneExamples {
        country: "US",
        locale: "en-US",
        valid: &[
            (
                "(201) 555-0123",
                "+12015550123",
                "(201) 555-0123",
                "+1 201-555-0123",
            ),
            (
                "1 201 555 0123",
                "+12015550123",
                "(201) 555-0123",
                "+1 201-555-0123",
            ),
            (
                "011 44 7400 123456",
                "+447400123456",
                "+44 7400 123456",
                "+44 7400 123456",
            ),
        ],
        invalid: &["201 555 012", "201 555 01234", "101 555 0123"],
    },
    PhoneExamples {
        country: "DE",
        locale: "de-DE",
        valid: &[
            ("030 123456", "+4930123456", "030 123456", "+49 30 123456"),
            (
                "01512 3456789",
                "+4915123456789",
                "01512 3456789",
                "+49 1512 3456789",
            ),
        ],
        invalid: &["01512 34567890", "0012"],
    },
    PhoneExamples {
        country: "FR",
        locale: "fr-FR",
        valid: &[
            (
                "01 23 45 67 89",
                "+33123456789",
                "01 23 45 67 89",
                "+33 1 23 45 67 89",
            ),
            (
                "06.12.34.56.78",
                "+33612345678",
                "06 12 34 56 78",
                "+33 6 12 34 56 78",
            ),
        ],
        invalid: &["06 12 34 56 7", "06 12 34 56 789", "00 12 34 56 78"],
    },
    PhoneExamples {
        country: "JP",
        locale: "ja-JP",
        valid: &[
            (
                "03-1234-5678",
                "+81312345678",
                "03-1234-5678",
                "+81 3-1234-5678",
            ),
            (
                "090-1234-5678",
                "+819012345678",
                "090-1234-5678",
                "+81 90-1234-5678",
            ),
        ],
        invalid: &["090-1234-567", "090-1234-56789"],
    },
    PhoneExamples {
        country: "IN",
        locale: "en-IN",
        valid: &[
            (
                "074104 10123",
                "+917410410123",
                "074104 10123",
                "+91 74104 10123",
            ),
            (
                "081234 56789",
                "+918123456789",
                "081234 56789",
                "+91 81234 56789",
            ),
        ],
        invalid: &["081234 5678", "081234 567890", "000000 00000"],
    },
    PhoneExamples {
        country: "BR",
        locale: "pt-BR",
        valid: &[
            (
                "(11) 2345-6789",
                "+551123456789",
                "(11) 2345-6789",
                "+55 11 2345-6789",
            ),
            (
                "(11) 96123-4567",
                "+5511961234567",
                "(11) 96123-4567",
                "+55 11 96123-4567",
            ),
        ],
        invalid: &["(11) 96123-456", "(11) 96123-45678"],
    },
    PhoneExamples {
        country: "AU",
        locale: "en-AU",
        valid: &[
            (
                "(02) 1234 5678",
                "+61212345678",
                "(02) 1234 5678",
                "+61 2 1234 5678",
            ),
            (
                "0412 345 678",
                "+61412345678",
                "0412 345 678",
                "+61 412 345 678",
            ),
        ],
        invalid: &["0412 345 67", "0412 345 6789", "0012 345 678"],
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_pattern_compiles() {
        for region in REGIONS {
            let mut sources = vec![region.general];
            sources.extend(region.types.iter().map(|(source, _)| *source));
            for format in region.formats {
                sources.push(format.pattern);
                if !format.leading.is_empty() {
                    sources.push(format.leading);
                }
            }
            for extra in [region.leading, region.idd, region.parsing] {
                if !extra.is_empty() {
                    sources.push(extra);
                }
            }
            for source in sources {
                assert!(pattern(source, true).is_some(), "{}: {source}", region.id);
            }
        }
    }

    #[test]
    fn each_code_has_one_main_region_first() {
        let mut seen: Vec<&str> = Vec::new();
        for region in REGIONS {
            if !seen.contains(&region.calling) {
                assert!(
                    region.main,
                    "+{} begins with its main region",
                    region.calling
                );
                seen.push(region.calling);
            }
        }
        assert!(is_assigned("44") && is_assigned("800") && !is_assigned("999"));
    }

    #[test]
    fn each_country_s_examples_read_and_show_as_listed() {
        for examples in PHONE_EXAMPLES {
            let country = examples.country;
            for (written, e164, at_home, abroad) in examples.valid {
                assert_eq!(from_written(written, country).as_deref(), Some(*e164));
                assert_eq!(shown(e164, Some(country)).as_deref(), Some(*at_home));
                assert_eq!(international(e164).as_deref(), Some(*abroad));
            }
            for written in examples.invalid {
                assert_eq!(from_written(written, country), None, "{country}: {written}");
            }
        }
    }

    #[test]
    fn a_shared_code_finds_its_region() {
        let (calling, digits) = split("+441534456789").expect("a number");
        assert_eq!(region_for(calling, digits).map(|r| r.id), Some("JE"));
        let (calling, digits) = split("+15062345678").expect("a number");
        assert_eq!(region_for(calling, digits).map(|r| r.id), Some("CA"));
        assert_eq!(
            shown("+15062345678", Some("US")).as_deref(),
            Some("(506) 234-5678")
        );
    }
}
