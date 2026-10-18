//! Tax numbers and identity documents with the country that issued them
//! (area 19, "Tax and identity numbers", chunk 5.5i2): `VatNumber`,
//! `TaxId`, `PassportNumber`, `DrivingLicenceNumber` and
//! `IdentityDocument`, each an object of a `Country` and a number, and
//! `IdentityDocumentKind`, what kind of document one is. A number is upper
//! case with no spaces or punctuation, as the payment numbers are (chunk
//! 5.5i), and fits its country's published form (its rule
//! `number_of_country`, chunk 5.5t2): a pattern
//! and, where the issuer publishes one, a check digit. A country that
//! publishes no form takes any number within the type's bounds, so a
//! number from a country not listed here is never refused for its country
//! alone; a release adds countries as their forms are published.

use super::across::{Check, Parts, StandardRule, Verdict, fits};
use super::{
    Examples, FIRST_RELEASE, Format, FormatKey, Holds, Json, NO_OPTIONS, Part, StandardType,
    Storage, TextForm, identifiers, matches_whole,
};
use crate::Builtin;
use crate::read::SqlType;

/// `VatNumber`.
pub const VAT_NUMBER: &str = "VatNumber";
/// `TaxId`.
pub const TAX_ID: &str = "TaxId";
/// `PassportNumber`.
pub const PASSPORT_NUMBER: &str = "PassportNumber";
/// `DrivingLicenceNumber`.
pub const DRIVING_LICENCE_NUMBER: &str = "DrivingLicenceNumber";
/// `IdentityDocument`.
pub const IDENTITY_DOCUMENT: &str = "IdentityDocument";
/// `IdentityDocumentKind`.
pub const IDENTITY_DOCUMENT_KIND: &str = "IdentityDocumentKind";

/// The kinds of identity document, each its own name.
pub const IDENTITY_DOCUMENT_KINDS: &[(&str, &str)] = &[
    ("passport", "passport"),
    ("identity_card", "identity_card"),
    ("driving_licence", "driving_licence"),
    ("residence_permit", "residence_permit"),
];

/// The bounds of a VAT number after its country's code, where its country
/// publishes no form: 2 to 15 capital letters and digits.
pub const VAT_BOUNDS: &str = "[A-Z0-9]{2,15}";

/// The bounds of a tax or identity document's number, where its country
/// publishes no form: 1 to 25 capital letters and digits, the longest
/// AAMVA's card standard lets a US state's licence number be.
pub const NUMBER_BOUNDS: &str = "[A-Z0-9]{1,25}";

/// One country's published form for a kind of number: a pattern matched
/// whole, and the check digit it carries, if any. A country with more than
/// one form (a UK taxpayer's UTR or National Insurance number) has a row
/// for each, and a number fits when it fits any.
#[derive(Clone, Copy, Debug)]
pub struct CountryForm {
    pub country: &'static str,
    pub pattern: &'static str,
    pub check: Option<fn(&str) -> bool>,
}

const fn form(country: &'static str, pattern: &'static str) -> CountryForm {
    CountryForm {
        country,
        pattern,
        check: None,
    }
}

const fn checked(
    country: &'static str,
    pattern: &'static str,
    check: fn(&str) -> bool,
) -> CountryForm {
    CountryForm {
        country,
        pattern,
        check: Some(check),
    }
}

/// Whether `number` fits one of `country`'s forms in `forms`, or `bounds`
/// where `forms` has none for it.
fn fits_forms(forms: &[CountryForm], bounds: &'static str, number: &str, country: &str) -> bool {
    let mut listed = forms
        .iter()
        .filter(|form| form.country == country)
        .peekable();
    if listed.peek().is_none() {
        return matches_whole(bounds, number);
    }
    listed.any(|form| {
        matches_whole(form.pattern, number) && form.check.is_none_or(|check| check(number))
    })
}

/// VAT numbers, after the country's code: the 27 member states as VIES
/// lists their forms (its FAQ's table), the United Kingdom's from HMRC,
/// Norway's and Switzerland's organisation numbers with their suffix, and,
/// outside Europe, Australia's ABN, which GST registration uses (the
/// ABR's published form). The Commission publishes no member state's check
/// digit; Germany's, France's and Ireland's here are the ones their
/// authorities' numbers all pass, as the reference implementations check
/// them, and the United Kingdom's is HMRC's mod-97 and its 9755 variant.
pub const VAT_FORMS: &[CountryForm] = &[
    form("AT", "U[0-9]{8}"),
    checked("AU", "[1-9][0-9]{10}", australian_business_number),
    form("BE", "[01][0-9]{9}"),
    form("BG", "[0-9]{9,10}"),
    checked("CH", "E[0-9]{9}(?:MWST|TVA|IVA|TPV)", swiss_business_number),
    form("CY", "[0-9]{8}[A-Z]"),
    form("CZ", "[0-9]{8,10}"),
    checked("DE", "[0-9]{9}", iso7064_mod11_10),
    form("DK", "[0-9]{8}"),
    form("EE", "[0-9]{9}"),
    form("ES", "[A-Z][0-9]{7}[A-Z0-9]|[0-9]{8}[A-Z]"),
    form("FI", "[0-9]{8}"),
    checked("FR", "[0-9A-HJ-NP-Z]{2}[0-9]{9}", french_vat),
    checked("GB", "[0-9]{9}|[0-9]{12}", british_vat),
    form("GB", "GD[0-4][0-9]{2}|HA[5-9][0-9]{2}"),
    form("GR", "[0-9]{9}"),
    form("HR", "[0-9]{11}"),
    form("HU", "[0-9]{8}"),
    checked(
        "IE",
        "[0-9]{7}[A-W][A-I]?|[0-9][A-Z][0-9]{5}[A-W]",
        irish_check_letter,
    ),
    form("IT", "[0-9]{11}"),
    form("LT", "[0-9]{9}|[0-9]{12}"),
    form("LU", "[0-9]{8}"),
    form("LV", "[0-9]{11}"),
    form("MT", "[0-9]{8}"),
    form("NL", "[0-9]{9}B[0-9]{2}"),
    checked("NO", "[0-9]{9}MVA", norwegian_organisation_number),
    form("PL", "[0-9]{10}"),
    form("PT", "[0-9]{9}"),
    form("RO", "[0-9]{2,10}"),
    form("SE", "[0-9]{12}"),
    form("SI", "[0-9]{8}"),
    form("SK", "[0-9]{10}"),
];

/// The code VIES writes a country's VAT numbers with: Greece's is `EL`,
/// every other its ISO 3166-1 code.
pub fn vat_prefix(country: &str) -> &str {
    if country == "GR" { "EL" } else { country }
}

/// The country a VAT number's code names, `EL` being Greece; `None` for a
/// code that isn't a current country's.
pub fn vat_country(prefix: &str) -> Option<&'static str> {
    let code = if prefix == "EL" { "GR" } else { prefix };
    if code == "GR" && prefix != "EL" {
        // VIES writes Greece as `EL`, so `GR` is no VAT number's code.
        return None;
    }
    let countries = super::standard_type(super::places::COUNTRY)?.codes()?;
    countries
        .iter()
        .find(|(name, listed)| {
            *listed == code && super::retired(super::places::COUNTRY, name).is_none()
        })
        .map(|(_, listed)| *listed)
}

/// Whether `number` is a VAT number of `country`.
pub fn vat_fits(number: &str, country: &str) -> bool {
    matches_whole(VAT_BOUNDS, number) && fits_forms(VAT_FORMS, VAT_BOUNDS, number, country)
}

/// Taxpayer identification numbers, by the OECD's page for each country
/// and the EU's TIN algorithms (DG TAXUD's functional specification, 5.03
/// of 2017-02-24): the forms a country issues to people and businesses
/// for tax.
pub const TAX_ID_FORMS: &[CountryForm] = &[
    // A tax file number; the ATO gives its check only on request, so the
    // nine-digit form's widely published one is checked, and the older
    // eight-digit form only by its length.
    checked("AU", "[0-9]{8,9}", australian_tax_file_number),
    checked("CA", "[0-9]{9}", identifiers::passes_luhn),
    checked("DE", "[1-9][0-9]{10}", german_tax_id),
    checked("FR", "[0-3][0-9]{12}", french_tax_number),
    // A UTR, whose check HMRC doesn't publish, or a National Insurance
    // number, by HMRC's NIM39110 (chunk 5.5i).
    form("GB", "[0-9]{10}"),
    checked("GB", "[A-Z]{2}[0-9]{6}[A-D]", |number| {
        identifiers::UK_NATIONAL_INSURANCE_NUMBER.accepts(number)
    }),
    checked("IE", "[0-9]{7}[A-W][A-IW]?", irish_check_letter),
    // A PAN: its fourth letter the holder's category (P a person, C a
    // company ...); its last letter's check isn't published.
    form("IN", "[A-Z]{3}[ABCFGHJLPT][A-Z][0-9]{4}[A-Z]"),
    // A Social Security number by the SSA's rules (chunk 5.5i), an ITIN,
    // and an EIN with one of the prefixes the IRS assigns.
    checked("US", "[0-9]{9}", |number| {
        identifiers::US_SSN.accepts(number)
    }),
    form(
        "US",
        "9[0-9]{2}(?:5[0-9]|6[0-5]|7[0-9]|8[0-8]|9[0-2]|9[4-9])[0-9]{4}",
    ),
    form(
        "US",
        "(?:0[1-6]|1[0-6]|2[0-7]|3[0-9]|4[0-8]|5[0-9]|6[0-8]|7[1-7]|8[0-8]|9[0-5]|9[89])[0-9]{7}",
    ),
];

/// Passport numbers, by each issuer's published form: the United
/// Kingdom's nine digits (HMPO), Germany's nine of its 26 characters, a
/// passport's first one of six (the Federal Interior Ministry), the
/// United States' nine digits or a letter and eight (the State
/// Department's Next Generation passport), and France's, Ireland's,
/// India's, Canada's and Japan's forms as their passport offices answer
/// for them. ICAO Doc 9303 prints up to nine in the machine-readable zone.
pub const PASSPORT_FORMS: &[CountryForm] = &[
    form("CA", "[A-Z]{2}[0-9]{6}"),
    form("DE", "[CFGHJK][CFGHJKLMNPRTVWXYZ0-9]{8}"),
    form("FR", "[0-9]{2}[A-Z]{2}[0-9]{5}"),
    form("GB", "[0-9]{9}"),
    form("IE", "[PL][A-Z][0-9]{7}"),
    form("IN", "[A-Z][0-9]{7}"),
    form("JP", "[A-Z]{2}[0-9]{7}"),
    form("US", "[0-9]{9}|[A-Z][0-9]{8}"),
];

/// Driving licence numbers, by each issuer's published form. France and
/// Ireland publish none, and the United States has none nationally (each
/// state issues its own, up to the 25 characters AAMVA's card standard
/// allows), so each takes any number within the bounds.
pub const DRIVING_LICENCE_FORMS: &[CountryForm] = &[
    // Germany's eleven characters (FeV, Anlage 8): an authority, a serial
    // number, a check digit and an issue number.
    form("DE", "[A-Z0-9]{11}"),
    // Great Britain's DVLA, as its own API checks one: five letters of the
    // surname padded with 9s, six digits of the birth date, two initials
    // padded with 9s, a character and two check letters; Northern
    // Ireland's DVA issues eight digits.
    form("GB", "[A-Z9]{5}[0-9]{6}[A-Z9]{2}[A-HJ-NPR-X2-9][A-Z]{2}"),
    form("GB", "[0-9]{8}"),
    // India's: a state, its transport office, the year and a serial.
    form("IN", "[A-Z]{2}[0-9]{13}"),
];

/// National identity card numbers, by each issuer's published form:
/// Germany's identity card, which starts with one of ten letters (the
/// Federal Interior Ministry), Ireland's Passport Card, and India's
/// Aadhaar number with its Verhoeff check (UIDAI). The United Kingdom and
/// the United States issue none nationally and France publishes none, so
/// each takes any number within the bounds.
pub const IDENTITY_CARD_FORMS: &[CountryForm] = &[
    form("DE", "[LMNPRTVWXY][CFGHJKLMNPRTVWXYZ0-9]{8}"),
    form("IE", "C[0-9]{8}"),
    checked("IN", "[2-9][0-9]{11}", verhoeff),
];

/// Residence permit numbers: none is published (the United Kingdom's
/// biometric permits, now replaced by eVisas, had no published form), so
/// every country takes any number within the bounds.
pub const RESIDENCE_PERMIT_FORMS: &[CountryForm] = &[];

/// Whether `number` is one of `country`'s numbers for the document kind
/// whose code is `kind`.
pub fn document_fits(number: &str, kind: &str, country: &str) -> bool {
    let forms = match kind {
        "passport" => PASSPORT_FORMS,
        "identity_card" => IDENTITY_CARD_FORMS,
        "driving_licence" => DRIVING_LICENCE_FORMS,
        "residence_permit" => RESIDENCE_PERMIT_FORMS,
        _ => return false,
    };
    fits_forms(forms, NUMBER_BOUNDS, number, country)
}

/// A document type's rule that its `number` is in its country's form,
/// named `number_of_country` (chunk 5.5t2), for a document whose forms
/// `fit` says.
const fn number_of_country(reason: &'static str, check: fn(&dyn Parts) -> Verdict) -> StandardRule {
    StandardRule {
        name: "number_of_country",
        written: "number is in the form of its country's numbers",
        label: "not a number of the document's country",
        reason,
        next: "Write a number in the country's form, or change `country`",
        check: Check::By(check),
    }
}

/// The rules across the parts of the document type `ty` (chunk 5.5t2): its
/// number fits its country, and its kind for an `IdentityDocument`.
pub fn rules(ty: &str) -> &'static [StandardRule] {
    const VAT: &[StandardRule] = &[number_of_country(
        "A `VatNumber`'s `number` is in the form its `country`'s tax authority writes, with its check digits where it has them.",
        |parts| {
            fits(parts, "number", &["country"], |number, by| {
                by.first().is_some_and(|country| vat_fits(number, country))
            })
        },
    )];
    const TAX: &[StandardRule] = &[number_of_country(
        "A `TaxId`'s `number` is in the form its `country`'s tax authority writes, with its check digits where it has them.",
        |parts| {
            fits(parts, "number", &["country"], |number, by| {
                by.first()
                    .is_some_and(|country| fits_forms(TAX_ID_FORMS, NUMBER_BOUNDS, number, country))
            })
        },
    )];
    const PASSPORT: &[StandardRule] = &[number_of_country(
        "A `PassportNumber`'s `number` is in the form its `country`'s passport office writes.",
        |parts| {
            fits(parts, "number", &["country"], |number, by| {
                by.first().is_some_and(|country| {
                    fits_forms(PASSPORT_FORMS, NUMBER_BOUNDS, number, country)
                })
            })
        },
    )];
    const LICENCE: &[StandardRule] = &[number_of_country(
        "A `DrivingLicenceNumber`'s `number` is in the form its `country`'s licensing authority writes.",
        |parts| {
            fits(parts, "number", &["country"], |number, by| {
                by.first().is_some_and(|country| {
                    fits_forms(DRIVING_LICENCE_FORMS, NUMBER_BOUNDS, number, country)
                })
            })
        },
    )];
    const DOCUMENT: &[StandardRule] = &[StandardRule {
        name: "number_of_kind_and_country",
        written: "number is in the form of its kind of document from its country",
        label: "not a number of the document's kind and country",
        reason: "An `IdentityDocument`'s `number` is in the form its `country` issues its `kind` of document in, with its check digit where it has one.",
        next: "Write a number in the form of that kind of document from that country, or change `kind` or `country`",
        check: Check::By(|parts| {
            fits(
                parts,
                "number",
                &["kind", "country"],
                |number, by| match by {
                    [kind, country] => document_fits(number, kind, country),
                    _ => false,
                },
            )
        }),
    }];
    match ty {
        VAT_NUMBER => VAT,
        TAX_ID => TAX,
        PASSPORT_NUMBER => PASSPORT,
        DRIVING_LICENCE_NUMBER => LICENCE,
        IDENTITY_DOCUMENT => DOCUMENT,
        _ => &[],
    }
}

// --- check digits ----------------------------------------------------------

/// The digits of `text`, or `None` if it has anything else.
fn digits(text: &str) -> Option<Vec<u32>> {
    text.chars().map(|c| c.to_digit(10)).collect()
}

/// ISO 7064's MOD 11,10 over all but the last digit, which it gives: a
/// German VAT number's and tax identification number's check digit.
fn iso7064_mod11_10(number: &str) -> bool {
    let Some(digits) = digits(number) else {
        return false;
    };
    let Some((last, body)) = digits.split_last() else {
        return false;
    };
    let mut product = 10;
    for digit in body {
        let mut sum = (digit + product) % 10;
        if sum == 0 {
            sum = 10;
        }
        product = (2 * sum) % 11;
    }
    let check = (11 - product) % 10;
    check == *last
}

/// A German tax identification number (Steuerliche
/// Identifikationsnummer, the EU's "Germany 2"): its check digit, and in
/// its first ten digits one digit written two or three times, never three
/// times running, and the others at most once.
fn german_tax_id(number: &str) -> bool {
    let Some(digits) = digits(number) else {
        return false;
    };
    let mut counts = [0u8; 10];
    for digit in digits.iter().take(10) {
        if let Some(count) = counts.get_mut(*digit as usize) {
            *count += 1;
        }
    }
    let repeated = counts.iter().filter(|count| **count > 1).count();
    let most = counts.iter().copied().max().unwrap_or(0);
    // A digit written three times isn't written three times running.
    let three_running = digits
        .iter()
        .take(10)
        .collect::<Vec<_>>()
        .windows(3)
        .any(|run| matches!(run, [a, b, c] if a == b && b == c));
    repeated == 1 && (2..=3).contains(&most) && !three_running && iso7064_mod11_10(number)
}

/// A UK VAT number's check (HMRC): the first seven digits weighted 8 down
/// to 2, plus the last two as a number, a multiple of 97, or 55 short of
/// one for the numbers issued since 2010; a branch's three digits after
/// them aren't checked.
fn british_vat(number: &str) -> bool {
    let Some(digits) = digits(number) else {
        return false;
    };
    let weighted: u32 = digits
        .iter()
        .take(7)
        .zip([8, 7, 6, 5, 4, 3, 2])
        .map(|(digit, factor)| digit * factor)
        .sum();
    let check = digits.get(7).zip(digits.get(8)).map(|(a, b)| a * 10 + b);
    check.is_some_and(|check| {
        let total = weighted + check;
        total.is_multiple_of(97) || (total + 55).is_multiple_of(97)
    })
}

/// A French VAT number's key: for a key of two digits, 12 plus three times
/// the SIREN modulo 97, modulo 97. A key with a letter, which newer
/// numbers have, has no published check.
fn french_vat(number: &str) -> bool {
    let (key, siren) = (number.get(..2), number.get(2..));
    match (key.and_then(|key| key.parse::<u64>().ok()), siren) {
        (Some(key), Some(siren)) => siren
            .parse::<u64>()
            .is_ok_and(|siren| (12 + 3 * (siren % 97)) % 97 == key),
        (None, Some(_)) => true,
        _ => false,
    }
}

/// A French tax number (numéro fiscal de référence): its last three digits
/// are its first ten modulo 511.
fn french_tax_number(number: &str) -> bool {
    let (Some(body), Some(key)) = (number.get(..10), number.get(10..)) else {
        return false;
    };
    match (body.parse::<u64>(), key.parse::<u64>()) {
        (Ok(body), Ok(key)) => body % 511 == key,
        _ => false,
    }
}

/// An Irish check letter, a PPS number's and a VAT number's: the seven
/// digits weighted 8 down to 2, plus nine times a second letter's place
/// (A is 1, W is 0), modulo 23, as a letter (0 is W, 1 is A). A VAT
/// number of the old form, with a letter second, is read as `0`, its
/// third to seventh digits and its first.
fn irish_check_letter(number: &str) -> bool {
    let chars: Vec<char> = number.chars().collect();
    let (body, check, second): (Vec<char>, Option<&char>, Option<&char>) =
        if chars.get(1).is_some_and(char::is_ascii_uppercase) {
            // `1A23456B`: `0`, `23456`, `1`.
            let mut body = vec!['0'];
            body.extend(chars.get(2..7).unwrap_or_default());
            body.extend(chars.first());
            (body, chars.get(7), None)
        } else {
            (
                chars.get(..7).unwrap_or_default().to_vec(),
                chars.get(7),
                chars.get(8),
            )
        };
    let Some(body) = body
        .iter()
        .map(|c| c.to_digit(10))
        .collect::<Option<Vec<u32>>>()
    else {
        return false;
    };
    let letter_value = |c: char| {
        if c == 'W' {
            0
        } else {
            u32::from(c) - u32::from('A') + 1
        }
    };
    let mut total: u32 = body
        .iter()
        .zip([8, 7, 6, 5, 4, 3, 2])
        .map(|(digit, factor)| digit * factor)
        .sum();
    if let Some(second) = second {
        total += 9 * letter_value(*second);
    }
    let expected = match total % 23 {
        0 => 'W',
        n => char::from_u32(u32::from('A') + n - 1).unwrap_or('?'),
    };
    check == Some(&expected)
}

/// An Australian Business Number's check (ATO): one off the first digit,
/// the eleven weighted 10, 1, 3, 5 ... 19, a multiple of 89.
fn australian_business_number(number: &str) -> bool {
    let Some(mut digits) = digits(number) else {
        return false;
    };
    let Some(first) = digits.first_mut() else {
        return false;
    };
    if *first == 0 {
        return false;
    }
    *first -= 1;
    let total: u32 = digits
        .iter()
        .zip([10, 1, 3, 5, 7, 9, 11, 13, 15, 17, 19])
        .map(|(digit, factor)| digit * factor)
        .sum();
    digits.len() == 11 && total.is_multiple_of(89)
}

/// An Australian tax file number's check: the nine digits weighted 1, 4,
/// 3, 7, 5, 8, 6, 9, 10, a multiple of 11; the older eight-digit form,
/// whose weights aren't published, by its length alone.
fn australian_tax_file_number(number: &str) -> bool {
    let Some(digits) = digits(number) else {
        return false;
    };
    match digits.len() {
        9 => {
            let total: u32 = digits
                .iter()
                .zip([1, 4, 3, 7, 5, 8, 6, 9, 10])
                .map(|(digit, factor)| digit * factor)
                .sum();
            total.is_multiple_of(11)
        }
        8 => true,
        _ => false,
    }
}

/// An organisation number's mod-11 check digit over its first eight
/// digits with `weights`: 11 less their sum's remainder, 11 written 0 and
/// 10 never issued.
fn organisation_mod11(number: &str, weights: [u32; 8]) -> bool {
    let Some(digits) = digits(number.get(..9).unwrap_or_default()) else {
        return false;
    };
    let total: u32 = digits.iter().zip(weights).map(|(d, w)| d * w).sum();
    let check = match 11 - total % 11 {
        11 => 0,
        check => check,
    };
    digits.len() == 9 && check != 10 && digits.get(8) == Some(&check)
}

/// A Norwegian organisation number (Brønnøysund): weights 3, 2, 7, 6, 5,
/// 4, 3, 2.
fn norwegian_organisation_number(number: &str) -> bool {
    organisation_mod11(number, [3, 2, 7, 6, 5, 4, 3, 2])
}

/// A Swiss business number (UID, after its `E`): weights 5, 4, 3, 2, 7, 6,
/// 5, 4.
fn swiss_business_number(number: &str) -> bool {
    organisation_mod11(
        number.get(1..).unwrap_or_default(),
        [5, 4, 3, 2, 7, 6, 5, 4],
    )
}

/// Verhoeff's check digit, an Aadhaar number's (UIDAI).
fn verhoeff(number: &str) -> bool {
    const MULTIPLY: [[u8; 10]; 10] = [
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 9],
        [1, 2, 3, 4, 0, 6, 7, 8, 9, 5],
        [2, 3, 4, 0, 1, 7, 8, 9, 5, 6],
        [3, 4, 0, 1, 2, 8, 9, 5, 6, 7],
        [4, 0, 1, 2, 3, 9, 5, 6, 7, 8],
        [5, 9, 8, 7, 6, 0, 4, 3, 2, 1],
        [6, 5, 9, 8, 7, 1, 0, 4, 3, 2],
        [7, 6, 5, 9, 8, 2, 1, 0, 4, 3],
        [8, 7, 6, 5, 9, 3, 2, 1, 0, 4],
        [9, 8, 7, 6, 5, 4, 3, 2, 1, 0],
    ];
    const PERMUTE: [[u8; 10]; 8] = [
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 9],
        [1, 5, 7, 6, 2, 8, 3, 0, 9, 4],
        [5, 8, 0, 3, 7, 9, 6, 1, 4, 2],
        [8, 9, 1, 6, 0, 4, 3, 5, 2, 7],
        [9, 4, 5, 3, 1, 2, 6, 8, 7, 0],
        [4, 2, 8, 6, 5, 7, 3, 9, 0, 1],
        [2, 7, 9, 3, 8, 0, 6, 4, 1, 5],
        [7, 0, 4, 6, 9, 1, 3, 2, 5, 8],
    ];
    let Some(digits) = digits(number) else {
        return false;
    };
    let mut check = 0u8;
    for (at, digit) in digits.iter().rev().enumerate() {
        let permuted = PERMUTE
            .get(at % 8)
            .and_then(|row| row.get(*digit as usize))
            .copied()
            .unwrap_or(0);
        check = MULTIPLY
            .get(check as usize)
            .and_then(|row| row.get(permuted as usize))
            .copied()
            .unwrap_or(1);
    }
    check == 0
}

// --- entries ----------------------------------------------------------------

/// The categories of the document types that are personal data (area 15):
/// each names a person to the state that issued it. A VAT number is a
/// business's, published on its invoices, as a US EIN is (chunk 5.5i).
pub const PERSONAL: &[(&str, &str)] = &[
    (DRIVING_LICENCE_NUMBER, "government_id"),
    (IDENTITY_DOCUMENT, "government_id"),
    (PASSPORT_NUMBER, "government_id"),
    (TAX_ID, "government_id"),
];

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

/// Shown as its number's last four, as a card number is (chunk 5.5i).
const MASKED: Format = Format {
    key: FormatKey::PartLastFour {
        parts: &["number"],
        mask: "•••• ",
    },
    options: NO_OPTIONS,
};

/// A document of a country and a number, sent as its parts and stored a
/// column per part.
const fn issued(
    name: &'static str,
    valid: &'static [(&'static str, &'static str)],
    malformed: &'static [&'static str],
) -> StandardType {
    StandardType {
        name,
        holds: Holds::Object(&[
            ("country", Part::Standard(super::places::COUNTRY)),
            ("number", Part::Matching(NUMBER_BOUNDS)),
        ]),
        text: None,
        json: Json::Fields(&["country", "number"]),
        storage: Storage::Columns {
            parts: &[("country", "text"), ("number", "text")],
            fixed: None,
        },
        format: MASKED,
        ordered: false,
        validator: None,
        shape: None,
        examples: examples(valid, malformed),
        release: FIRST_RELEASE,
    }
}

/// A VAT registration number and its country.
pub const VAT_NUMBER_TYPE: StandardType = StandardType {
    name: VAT_NUMBER,
    holds: Holds::Object(&[
        ("country", Part::Standard(super::places::COUNTRY)),
        ("number", Part::Matching(VAT_BOUNDS)),
    ]),
    text: Some(TextForm::VatNumber),
    json: Json::Text,
    storage: Storage::Columns {
        parts: &[("country", "text"), ("number", "text")],
        fixed: None,
    },
    format: Format {
        key: FormatKey::Canonical,
        options: NO_OPTIONS,
    },
    ordered: false,
    validator: None,
    shape: None,
    examples: examples(
        &[
            ("GB980780684", "GB980780684"),
            ("DE143454214", "DE143454214"),
            ("FR59542051180", "FR59542051180"),
            ("IE6388047V", "IE6388047V"),
            ("EL094259216", "EL094259216"),
            ("AU51824753556", "AU51824753556"),
        ],
        &[
            "GB802311781",
            "DE143454215",
            "FR84323140391",
            "GR094259216",
            "gb980780684",
            "GB 980 7806 84",
            "XX123456789",
            "GB",
            "",
        ],
    ),
    release: FIRST_RELEASE,
};

/// A taxpayer identification number and its country.
pub const TAX_ID_TYPE: StandardType = issued(
    TAX_ID,
    &[
        (r#"{"country":"GB","number":"AB123456C"}"#, "•••• 456C"),
        (r#"{"country":"US","number":"123456789"}"#, "•••• 6789"),
    ],
    &[
        r#"{"country":"GB","number":"ab123456c"}"#,
        r#"{"country":"GB","number":"AB12345"}"#,
        r#"{"country":"UK","number":"AB123456C"}"#,
        r#"{"number":"AB123456C"}"#,
    ],
);

/// A passport's number and the country that issued it.
pub const PASSPORT_NUMBER_TYPE: StandardType = issued(
    PASSPORT_NUMBER,
    &[
        (r#"{"country":"GB","number":"123456789"}"#, "•••• 6789"),
        (r#"{"country":"FR","number":"12AB34567"}"#, "•••• 4567"),
    ],
    &[
        r#"{"country":"GB","number":"12345678"}"#,
        r#"{"country":"FR","number":"123456789"}"#,
        r#"{"country":"GB","number":"1234 5678 9"}"#,
        r#"{"country":"GB"}"#,
    ],
);

/// A driving licence's number and the country that issued it.
pub const DRIVING_LICENCE_NUMBER_TYPE: StandardType = issued(
    DRIVING_LICENCE_NUMBER,
    &[
        (
            r#"{"country":"GB","number":"MCADI801206HZ9HM"}"#,
            "•••• Z9HM",
        ),
        (r#"{"country":"US","number":"D1234567"}"#, "•••• 4567"),
    ],
    &[
        r#"{"country":"GB","number":"MCADI801206"}"#,
        r#"{"country":"GB","number":"mcadi801206hz9hm"}"#,
        r#"{"country":"US","number":"D123-4567"}"#,
        r#"{"number":"D1234567"}"#,
    ],
);

/// What kind of identity document one is.
pub const IDENTITY_DOCUMENT_KIND_TYPE: StandardType = StandardType {
    name: IDENTITY_DOCUMENT_KIND,
    holds: Holds::Enum(IDENTITY_DOCUMENT_KINDS),
    text: Some(TextForm::Code),
    json: Json::Text,
    storage: Storage::Column(SqlType::Text),
    format: Format {
        key: FormatKey::Canonical,
        options: NO_OPTIONS,
    },
    ordered: false,
    validator: None,
    shape: None,
    examples: examples(
        &[
            ("passport", "passport"),
            ("driving_licence", "driving_licence"),
        ],
        &["passports", "visa", "id_card", ""],
    ),
    release: FIRST_RELEASE,
};

/// An identity document: its kind, its issuing country, its number and
/// the day it expires, if it does.
pub const IDENTITY_DOCUMENT_TYPE: StandardType = StandardType {
    name: IDENTITY_DOCUMENT,
    holds: Holds::Object(&[
        ("kind", Part::Standard(IDENTITY_DOCUMENT_KIND)),
        ("country", Part::Standard(super::places::COUNTRY)),
        ("number", Part::Matching(NUMBER_BOUNDS)),
        ("expires", Part::Optional(&Part::Of(Builtin::CalendarDate))),
    ]),
    text: None,
    json: Json::Fields(&["kind", "country", "number", "expires"]),
    storage: Storage::Columns {
        parts: &[
            ("kind", "text"),
            ("country", "text"),
            ("number", "text"),
            ("expires", "date"),
        ],
        fixed: None,
    },
    format: MASKED,
    ordered: false,
    validator: None,
    shape: None,
    examples: examples(
        &[
            (
                r#"{"kind":"passport","country":"GB","number":"123456789","expires":"2030-01-31"}"#,
                "•••• 6789",
            ),
            (
                r#"{"kind":"identity_card","country":"DE","number":"L01X00T47","expires":null}"#,
                "•••• 0T47",
            ),
        ],
        &[
            r#"{"kind":"driving_licence","country":"GB","number":"123456789","expires":null}"#,
            r#"{"kind":"visa","country":"GB","number":"123456789","expires":null}"#,
            r#"{"kind":"passport","country":"GB","number":"123456789","expires":"2030-02-30"}"#,
            r#"{"kind":"passport","country":"GB","number":"123456789"}"#,
        ],
    ),
    release: FIRST_RELEASE,
};

/// One country's valid and invalid numbers for a document type, each a
/// rule's: the harness reads each as the type's JSON (as its text, for a
/// VAT number) with that country.
#[derive(Clone, Copy, Debug)]
pub struct DocumentExamples {
    pub ty: &'static str,
    /// An identity document's kind, by its code.
    pub kind: Option<&'static str>,
    pub country: &'static str,
    pub valid: &'static [&'static str],
    pub invalid: &'static [&'static str],
}

const fn of(
    ty: &'static str,
    country: &'static str,
    valid: &'static [&'static str],
    invalid: &'static [&'static str],
) -> DocumentExamples {
    DocumentExamples {
        ty,
        kind: None,
        country,
        valid,
        invalid,
    }
}

const fn document(
    kind: &'static str,
    country: &'static str,
    valid: &'static [&'static str],
    invalid: &'static [&'static str],
) -> DocumentExamples {
    DocumentExamples {
        ty: IDENTITY_DOCUMENT,
        kind: Some(kind),
        country,
        valid,
        invalid,
    }
}

/// Each country's examples, published ones where the issuer publishes
/// them and made up to its rules where it doesn't; a country with no
/// published form takes any number within the bounds and refuses only
/// what is outside them.
pub const DOCUMENT_EXAMPLES: &[DocumentExamples] = &[
    // VAT numbers: published ones (HMRC's examples, SAP's, TotalEnergies',
    // Google Ireland's, the ATO's own ABN, Equinor's), and made up to the
    // rules for a check-only one.
    of(
        VAT_NUMBER,
        "GB",
        &["980780684", "101522380", "980780684001", "GD001", "HA500"],
        &["802311781", "GD500", "98078068"],
    ),
    of(
        VAT_NUMBER,
        "DE",
        &["143454214", "136695976"],
        &["136695978", "12345678"],
    ),
    of(
        VAT_NUMBER,
        "FR",
        &["59542051180", "40303265045", "K7399859412"],
        &["84323140391", "IO334175221"],
    ),
    of(
        VAT_NUMBER,
        "IE",
        &["6388047V", "8D79739I"],
        &["6433435E", "64334351"],
    ),
    of(
        VAT_NUMBER,
        "ES",
        &["A12345674", "12345678Z"],
        &["123456789", "AB1234567"],
    ),
    of(
        VAT_NUMBER,
        "NO",
        &["923609016MVA"],
        &["923609017MVA", "923609016"],
    ),
    of(
        VAT_NUMBER,
        "CH",
        &["E107787577IVA", "E123456788MWST"],
        &["E107787578IVA", "E10778757MWST"],
    ),
    of(
        VAT_NUMBER,
        "AU",
        &["51824753556", "83914571673"],
        &["51824753557", "5182475355"],
    ),
    of(VAT_NUMBER, "US", &["123456789"], &["1", "1234567890123456"]),
    // Taxpayer numbers: the EU's TIN specification's samples where it has
    // them, the OECD's PAN, and made up to the rules elsewhere.
    of(
        TAX_ID,
        "GB",
        &["1955839661", "AB123456C"],
        &["123456789", "GB123456A"],
    ),
    of(
        TAX_ID,
        "DE",
        &["86095742719", "65929970489"],
        &["86095742718", "01234567890", "11145678908"],
    ),
    of(
        TAX_ID,
        "FR",
        &["3023217600053", "0123456789211"],
        &["3023217600054", "4023217600053"],
    ),
    of(
        TAX_ID,
        "IE",
        &["1234567T", "1234577IA"],
        &["6433435E", "123456T"],
    ),
    of(
        TAX_ID,
        "US",
        &["536904399", "912503456", "911144442"],
        &["000123456", "970001234", "12345678"],
    ),
    of(TAX_ID, "IN", &["AFZPK7190K"], &["AFZQK7190K", "AFZPK719OK"]),
    of(
        TAX_ID,
        "AU",
        &["123456782", "12345678"],
        &["123456789", "1234567"],
    ),
    of(TAX_ID, "CA", &["046454286"], &["046454287", "04645428"]),
    // Passports, made up to each issuer's form.
    of(
        PASSPORT_NUMBER,
        "GB",
        &["123456789"],
        &["12345678", "A23456789"],
    ),
    of(
        PASSPORT_NUMBER,
        "DE",
        &["C01X00T47"],
        &["A01X00T47", "C01X00T4"],
    ),
    of(
        PASSPORT_NUMBER,
        "FR",
        &["12AB34567"],
        &["123456789", "12AB3456"],
    ),
    of(
        PASSPORT_NUMBER,
        "IE",
        &["PA1234567"],
        &["XA1234567", "PA123456"],
    ),
    of(
        PASSPORT_NUMBER,
        "US",
        &["123456789", "A12345678"],
        &["12345678", "AB1234567"],
    ),
    of(
        PASSPORT_NUMBER,
        "IN",
        &["J8369854"],
        &["J836985", "JJ369854"],
    ),
    of(PASSPORT_NUMBER, "JP", &["TK1234567"], &["T1234567"]),
    // Driving licences: the DVLA's own examples, Germany's published
    // sample, and made up to the rules elsewhere; France, Ireland and the
    // United States by the bounds.
    of(
        DRIVING_LICENCE_NUMBER,
        "GB",
        &["MCADI801206HZ9HM", "TCAEU610267NO9EK", "12345678"],
        &["MORGA753116SM1IJ", "MORGA7531161SM9IJ"],
    ),
    of(
        DRIVING_LICENCE_NUMBER,
        "DE",
        &["B072RRE2I55"],
        &["B072RRE2I5", "B072RRE2I555"],
    ),
    of(
        DRIVING_LICENCE_NUMBER,
        "FR",
        &["123456789012"],
        &["1234567890123456789012345X"],
    ),
    of(
        DRIVING_LICENCE_NUMBER,
        "IE",
        &["123456789"],
        &["1234567890123456789012345X"],
    ),
    of(
        DRIVING_LICENCE_NUMBER,
        "US",
        &["D1234567", "123456789012"],
        &["D1234567D1234567D123456789"],
    ),
    of(
        DRIVING_LICENCE_NUMBER,
        "IN",
        &["RJ1320120123456"],
        &["RJ132012012345", "1313201201234567"],
    ),
    // Identity documents, by kind.
    document("passport", "GB", &["123456789"], &["12345678"]),
    document("identity_card", "DE", &["L01X00T47"], &["C01X00T47"]),
    document(
        "identity_card",
        "FR",
        &["X4RTBPFW4"],
        &["X4RTBPFW4X4RTBPFW4X4RTBPFW4"],
    ),
    document("identity_card", "IE", &["C12345678"], &["PA1234567"]),
    document(
        "identity_card",
        "IN",
        &["234123412346", "499118665246"],
        &["234123412347", "123412341234"],
    ),
    document(
        "identity_card",
        "US",
        &["D1234567"],
        &["D1234567D1234567D123456789"],
    ),
    document(
        "driving_licence",
        "GB",
        &["MCADI801206HZ9HM"],
        &["123456789"],
    ),
    document(
        "residence_permit",
        "GB",
        &["ZU1234567"],
        &["ZU1234567ZU1234567ZU1234567"],
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_digits_take_published_values() {
        assert!(iso7064_mod11_10("136695976"));
        assert!(!iso7064_mod11_10("136695975"));
        assert!(british_vat("980780684"));
        assert!(
            australian_business_number("51824753556"),
            "the ATO's example"
        );
        assert!(irish_check_letter("1234567T"));
        assert!(irish_check_letter("1234567FA"));
        assert!(verhoeff("2363"), "Verhoeff's own example");
        assert!(!verhoeff("2364"));
    }

    #[test]
    fn a_vat_number_s_code_is_vies_s() {
        assert_eq!(vat_prefix("GR"), "EL");
        assert_eq!(vat_prefix("GB"), "GB");
        assert_eq!(vat_country("EL"), Some("GR"));
        assert_eq!(vat_country("GR"), None);
        assert_eq!(vat_country("XX"), None);
        assert_eq!(vat_country("YU"), None, "a withdrawn code");
    }
}
