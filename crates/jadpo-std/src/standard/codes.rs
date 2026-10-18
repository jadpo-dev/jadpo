//! The codes of commerce, health, travel and the other published standards
//! held as text (area 19, "Groups with the baseline only", chunk 5.5m):
//! each a pattern, matched against the whole canonical text, and, where the
//! page names one, a check digit (GS1's mod 10, a VIN's, ISO 6346's, ISO
//! 7064's MOD 11-2, SNOMED CT's Verhoeff, an MPAN's), with the published
//! values the harness checks each with. A value is its canonical text, as
//! the payment numbers' is (chunk 5.5i): what the wire, storage and
//! `text.parse` carry.
//!
//! The lists some of them name (IATA's airports and airlines, ICAO's
//! location indicators, ICD-10's release) aren't shipped: IATA's and
//! ICAO's are sold under licence, and WHO's ICD-10 may not be redistributed
//! without its permission, so each is checked by its pattern only ("Chosen
//! in chunk 5.5m").

use super::{
    Examples, FIRST_RELEASE, Format, FormatKey, Grouping, Holds, Json, NO_OPTIONS, StandardType,
    Storage, TextForm, Validator,
};
use crate::read::SqlType;

/// A standard type held as text, checked by its pattern and validator.
const fn code(
    name: &'static str,
    pattern: &'static str,
    validator: Option<Validator>,
    key: FormatKey,
    personal: Option<&'static str>,
    examples: Examples,
) -> StandardType {
    StandardType {
        name,
        holds: Holds::Text { pattern, personal },
        text: Some(TextForm::Checked),
        json: Json::Text,
        storage: Storage::Column(SqlType::Text),
        format: Format {
            key,
            options: NO_OPTIONS,
        },
        ordered: false,
        validator,
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

const CANONICAL: FormatKey = FormatKey::Canonical;

/// 1 to 20 characters of GS1's character set 82 (GS1 General
/// Specifications, figure 7.11-1): a batch's and a serial number's pattern.
const GS1_UP_TO_20: &str = r#"[!"%&'()*+,\-./0-9:;<=>?A-Z_a-z]{1,20}"#;

/// A seller's stock-keeping unit: 1 to 64 characters, letters, digits and
/// punctuation, with no space or control character anywhere, kept as
/// written (no standard says two cases are one SKU).
pub const SKU: StandardType = code(
    "Sku",
    r"[^\s\p{Cc}]{1,64}",
    None,
    CANONICAL,
    None,
    // No standard sets a SKU's form, so these are made up.
    examples(
        &[
            ("AB-1234-XL", "AB-1234-XL"),
            ("tshirt/red/M", "tshirt/red/M"),
        ],
        &[" AB-1234-XL", "AB 1234 XL", "AB-1234-XL\n", ""],
    ),
);

/// A GTIN-13, the European article number (GS1 General Specifications):
/// 13 digits, the last GS1's mod-10 check digit.
pub const EAN13: StandardType = code(
    "Ean13",
    "[0-9]{13}",
    Some(Validator::Gs1),
    CANONICAL,
    None,
    // The GS1 General Specifications' check-digit example (section 7.9.1)
    // and a STABILO pen's, as its barcode prints it.
    examples(
        &[
            ("4006381333931", "4006381333931"),
            ("6291041500213", "6291041500213"),
        ],
        &[
            // The check digit.
            "4006381333932",
            // A UPC-A's 12, and a digit too many.
            "036000291452",
            "40063813339310",
            "4006381 333931",
            "",
        ],
    ),
);

/// A GTIN-12, the universal product code (GS1 General Specifications): 12
/// digits, the last GS1's mod-10 check digit, its leading zero kept.
pub const UPC_A: StandardType = code(
    "UpcA",
    "[0-9]{12}",
    Some(Validator::Gs1),
    CANONICAL,
    None,
    // A Kleenex box's UPC, and GS1 US's example company prefix 0614141's.
    examples(
        &[
            ("036000291452", "036000291452"),
            ("614141000036", "614141000036"),
        ],
        &[
            "036000291453",
            "36000291452",
            "4006381333931",
            "03600029145X",
            "",
        ],
    ),
);

/// An ISBN (ISO 2108) in its 13 digits: the GS1 prefix 978 or 979, and
/// GS1's mod-10 check digit, with no hyphens, since where they go depends
/// on the ISBN agency's ranges.
pub const ISBN: StandardType = code(
    "Isbn",
    "97[89][0-9]{10}",
    Some(Validator::Gs1),
    CANONICAL,
    None,
    // The ISBN Users' Manual's example and a 979 ISBN from the French
    // agency's range.
    examples(
        &[
            ("9780306406157", "9780306406157"),
            ("9791090636071", "9791090636071"),
        ],
        &[
            // The check digit.
            "9780306406158",
            // An ISBN-10, and the printed form.
            "0306406152",
            "978-0-306-40615-7",
            // A GTIN whose check digit passes but isn't a book's (977 is
            // ISSN's prefix).
            "9770306406158",
            "",
        ],
    ),
);

/// A serial shipping container code (GS1 General Specifications): 18
/// digits, an extension digit, a company prefix and a serial reference,
/// the last GS1's mod-10 check digit.
pub const SSCC: StandardType = code(
    "Sscc",
    "[0-9]{18}",
    Some(Validator::Gs1),
    CANONICAL,
    None,
    // The GS1 General Specifications' SSCC example, and one made up to its
    // rules for GS1 UK's prefix 5012345.
    examples(
        &[
            ("106141412345678908", "106141412345678908"),
            ("350123450000000016", "350123450000000016"),
        ],
        &[
            "106141412345678909",
            "10614141234567890",
            "(00)106141412345678908",
            "",
        ],
    ),
);

/// A Harmonized System commodity code (WCO): its six digits, chapter 01
/// upward, and a country's own two or four more, as a tariff writes it
/// without dots.
pub const HS_CODE: StandardType = code(
    "HsCode",
    "(?:0[1-9]|[1-9][0-9])[0-9]{4}(?:[0-9]{1,4})?",
    None,
    CANONICAL,
    None,
    // Coffee, not roasted or decaffeinated (HS 0901.11), and the UK
    // tariff's commodity code for laptops.
    examples(
        &[("090111", "090111"), ("8471300000", "8471300000")],
        &["0901", "0901.11", "000111", "84713000000", ""],
    ),
);

/// A shipping carrier, by its Standard Carrier Alpha Code (NMFTA): two to
/// four capital letters.
pub const CARRIER: StandardType = code(
    "Carrier",
    "[A-Z]{2,4}",
    None,
    CANONICAL,
    None,
    // UPS's and Maersk's codes, as each publishes them.
    examples(
        &[("UPSN", "UPSN"), ("MAEU", "MAEU")],
        &["upsn", "UP5N", "U", "UPSNX", ""],
    ),
);

/// An intermodal container's number (ISO 6346): an owner code, a category
/// (`U` freight, `J` detachable equipment, `Z` trailer), six digits and a
/// check digit, printed with spaces between them.
pub const CONTAINER_NUMBER: StandardType = code(
    "ContainerNumber",
    "[A-Z]{3}[UJZ][0-9]{7}",
    Some(Validator::ContainerNumber),
    FormatKey::Grouped(Grouping {
        sizes: &[4, 6, 1],
        separator: " ",
    }),
    None,
    // ISO 6346's worked example, and two made up to its rules.
    examples(
        &[
            ("CSQU3054383", "CSQU 305438 3"),
            ("MSKU9070323", "MSKU 907032 3"),
            ("TOLU4734787", "TOLU 473478 7"),
        ],
        &[
            // The check digit.
            "CSQU3054384",
            // A category ISO 6346 doesn't have, and the printed form.
            "CSQA3054383",
            "CSQU 305438 3",
            "csqu3054383",
            "",
        ],
    ),
);

/// A manufacturing batch or lot (GS1 Application Identifier 10): 1 to 20
/// characters of GS1's character set 82.
pub const LOT_NUMBER: StandardType = code(
    "LotNumber",
    GS1_UP_TO_20,
    None,
    CANONICAL,
    None,
    // GS1 sets the characters, not a form, so these are made up.
    examples(
        &[("ABC123", "ABC123"), ("2026-10/B", "2026-10/B")],
        &["LOT 1", "Lot#1", "ABCDEFGHIJKLMNOPQRSTU", ""],
    ),
);

/// An item's serial number (GS1 Application Identifier 21): 1 to 20
/// characters of GS1's character set 82.
pub const SERIAL_NUMBER: StandardType = code(
    "SerialNumber",
    GS1_UP_TO_20,
    None,
    CANONICAL,
    None,
    // Made up to GS1's characters.
    examples(
        &[("12345678", "12345678"), ("SN-0042a", "SN-0042a")],
        &["SN 0042", "SN#0042", "123456789012345678901", ""],
    ),
);

/// A vehicle identification number (ISO 3779): 17 capitals and digits
/// with no `I`, `O` or `Q`, its ninth the check digit 49 CFR 565 requires
/// of a North American manufacturer.
pub const VIN: StandardType = code(
    "Vin",
    "[A-HJ-NPR-Z0-9]{17}",
    Some(Validator::Vin),
    CANONICAL,
    None,
    // NHTSA's worked example of the check digit, a Honda Accord's, and a
    // Volkswagen built for Europe, whose ninth character isn't one.
    examples(
        &[
            ("1HGCM82633A004352", "1HGCM82633A004352"),
            ("1M8GDM9AXKP042788", "1M8GDM9AXKP042788"),
            ("WVWZZZ1JZXW000001", "WVWZZZ1JZXW000001"),
        ],
        &[
            // A North American check digit.
            "1M8GDM9AYKP042788",
            // `I`, `O` and `Q` are never used.
            "1M8GDM9AXKP04278O",
            "1m8gdm9axkp042788",
            "1M8GDM9AXKP04278",
            "",
        ],
    ),
);

/// A diagnosis's ICD-10 code (WHO, and the national modifications such as
/// ICD-10-CM that extend it): a letter, two digits and, after a dot, up
/// to four more characters, as the classification writes it. Checked by
/// its pattern only.
pub const ICD10_CODE: StandardType = code(
    "Icd10Code",
    r"[A-Z][0-9]{2}(?:\.[0-9A-Z]{1,4})?",
    None,
    CANONICAL,
    Some("health"),
    // Acute myocardial infarction, unspecified, and asthma in WHO's
    // ICD-10; a fracture's seventh character in ICD-10-CM.
    examples(
        &[("I21.9", "I21.9"), ("J45", "J45"), ("S52.521A", "S52.521A")],
        &["I219", "i21.9", "121.9", "I21.", "I21.12345", ""],
    ),
);

/// A SNOMED CT concept's identifier (SNOMED International): 6 to 18
/// digits, its partition identifier a concept's (`00`, or `10` in an
/// extension's namespace) and its last digit Verhoeff's check digit.
pub const SNOMED_CONCEPT: StandardType = code(
    "SnomedConcept",
    "[1-9][0-9]{5,17}",
    Some(Validator::SnomedConcept),
    CANONICAL,
    Some("health"),
    // Myocardial infarction, diabetes mellitus and the root concept, from
    // SNOMED CT's International Edition; an extension's concept from
    // SNOMED International's guide to identifiers.
    examples(
        &[
            ("22298006", "22298006"),
            ("73211009", "73211009"),
            ("138875005", "138875005"),
            ("999480561000087100", "999480561000087100"),
        ],
        &[
            // The check digit.
            "22298007",
            // A description's partition (`01`), with its check digit.
            "22298010",
            "022298006",
            "12345",
            "",
        ],
    ),
);

/// An airport's IATA location code: three capitals. IATA's list is sold
/// under licence, so only the pattern is checked.
pub const IATA_AIRPORT: StandardType = code(
    "IataAirport",
    "[A-Z]{3}",
    None,
    CANONICAL,
    None,
    // London Heathrow and New York JFK.
    examples(
        &[("LHR", "LHR"), ("JFK", "JFK")],
        &["lhr", "LH", "EGLL", "L1R", ""],
    ),
);

/// An airport's ICAO location indicator (ICAO Doc 7910): four capitals.
/// ICAO's list is sold, so only the pattern is checked.
pub const ICAO_AIRPORT: StandardType = code(
    "IcaoAirport",
    "[A-Z]{4}",
    None,
    CANONICAL,
    None,
    // London Heathrow and New York JFK.
    examples(
        &[("EGLL", "EGLL"), ("KJFK", "KJFK")],
        &["egll", "LHR", "EGLLX", "EG1L", ""],
    ),
);

/// An airline's IATA designator (Resolution 762): two letters, or a letter
/// and a digit either way round. Only the pattern is checked.
pub const IATA_AIRLINE: StandardType = code(
    "IataAirline",
    "[A-Z][A-Z0-9]|[0-9][A-Z]",
    None,
    CANONICAL,
    None,
    // British Airways, easyJet and IndiGo.
    examples(
        &[("BA", "BA"), ("U2", "U2"), ("6E", "6E")],
        &["ba", "99", "B", "BAW", ""],
    ),
);

/// A booking's record locator (IATA's passenger name record): six capitals
/// and digits.
pub const BOOKING_REFERENCE: StandardType = code(
    "BookingReference",
    "[A-Z0-9]{6}",
    None,
    CANONICAL,
    None,
    // Airlines publish no form beyond this, so these are made up.
    examples(
        &[("X7K9QZ", "X7K9QZ"), ("ABC123", "ABC123")],
        &["abc123", "ABC12", "ABC1234", "ABC 12", ""],
    ),
);

/// A dangerous good's UN number (the UN Model Regulations): `UN` and four
/// digits, printed with a space as a transport document writes it.
pub const UN_NUMBER: StandardType = code(
    "UnNumber",
    "UN[0-9]{4}",
    None,
    FormatKey::Grouped(Grouping {
        sizes: &[2, 4],
        separator: " ",
    }),
    None,
    // Petrol, and lithium-ion batteries, from the Model Regulations'
    // dangerous goods list.
    examples(
        &[("UN1203", "UN 1203"), ("UN3480", "UN 3480")],
        &["1203", "UN 1203", "un1203", "UN12034", ""],
    ),
);

/// A researcher's ORCID iD: 16 characters in fours joined by dashes, the
/// last ISO 7064's MOD 11-2 check character (a digit or `X`).
pub const ORCID: StandardType = code(
    "Orcid",
    "[0-9]{4}-[0-9]{4}-[0-9]{4}-[0-9]{3}[0-9X]",
    Some(Validator::Orcid),
    CANONICAL,
    Some("identifying"),
    // The three iDs ORCID's "Structure of the ORCID Identifier" uses.
    examples(
        &[
            ("0000-0002-1825-0097", "0000-0002-1825-0097"),
            ("0000-0001-5109-3700", "0000-0001-5109-3700"),
            ("0000-0002-1694-233X", "0000-0002-1694-233X"),
        ],
        &[
            // The check character.
            "0000-0002-1825-0098",
            "0000000218250097",
            "https://orcid.org/0000-0002-1825-0097",
            "0000-0002-1694-233x",
            "",
        ],
    ),
);

/// A UK electricity supply's MPAN core (Elexon's Master Registration
/// Agreement): 13 digits, a distributor, a unique number and a check digit.
pub const MPAN: StandardType = code(
    "Mpan",
    "[0-9]{13}",
    Some(Validator::Mpan),
    CANONICAL,
    Some("contact"),
    // Made up to the MRA's check digit, for two distributors.
    examples(
        &[
            ("1200012345679", "1200012345679"),
            ("2300004567890", "2300004567890"),
        ],
        &["1200012345678", "120001234567", "S 1200012345679", ""],
    ),
);

/// A UK gas supply's meter point reference number (Xoserve): 6 to 10
/// digits.
pub const MPRN: StandardType = code(
    "Mprn",
    "[0-9]{6,10}",
    None,
    CANONICAL,
    Some("contact"),
    // Made up to Xoserve's lengths.
    examples(
        &[("1234567", "1234567"), ("9876543210", "9876543210")],
        &["12345", "12345678901", "12 34567", ""],
    ),
);

/// GS1's mod-10 check digit, the last of `text`'s digits: 3 and 1 times
/// the digits before it in turn, from the right, and the check digit
/// making the sum a multiple of ten (GS1 General Specifications 7.9).
pub fn passes_gs1(text: &str) -> bool {
    let digits: Option<Vec<u32>> = text.chars().map(|c| c.to_digit(10)).collect();
    let Some([body @ .., check]) = digits.as_deref() else {
        return false;
    };
    let sum: u32 = body
        .iter()
        .rev()
        .zip([3, 1].into_iter().cycle())
        .map(|(digit, times)| digit * times)
        .sum();
    (10 - sum % 10) % 10 == *check
}

/// What a VIN's character counts as in its check digit (49 CFR 565.15).
fn vin_value(c: char) -> Option<u32> {
    Some(match c {
        '0'..='9' => c.to_digit(10)?,
        'A' | 'J' => 1,
        'B' | 'K' | 'S' => 2,
        'C' | 'L' | 'T' => 3,
        'D' | 'M' | 'U' => 4,
        'E' | 'N' | 'V' => 5,
        'F' | 'W' => 6,
        'G' | 'P' | 'X' => 7,
        'H' | 'Y' => 8,
        'R' | 'Z' => 9,
        _ => return None,
    })
}

/// A VIN's check digit, its ninth character, where 49 CFR 565 requires
/// one: of a North American manufacturer, whose identifier starts 1 to 5.
/// Elsewhere ISO 3779 leaves the ninth character to the manufacturer.
pub fn is_vin(text: &str) -> bool {
    const WEIGHTS: [u32; 17] = [8, 7, 6, 5, 4, 3, 2, 10, 0, 9, 8, 7, 6, 5, 4, 3, 2];
    if !text.starts_with(['1', '2', '3', '4', '5']) {
        return true;
    }
    let mut sum = 0;
    for (c, times) in text.chars().zip(WEIGHTS) {
        let Some(value) = vin_value(c) else {
            return false;
        };
        sum += value * times;
    }
    let expected = match sum % 11 {
        10 => 'X',
        n => char::from_digit(n, 10).unwrap_or('?'),
    };
    text.chars().nth(8) == Some(expected) && text.chars().count() == 17
}

/// What a container number's character counts as in its check digit (ISO
/// 6346): a digit itself, a capital its number from A's 10, skipping 11, 22
/// and 33.
fn container_value(c: char) -> Option<u32> {
    if let Some(digit) = c.to_digit(10) {
        return Some(digit);
    }
    if !c.is_ascii_uppercase() {
        return None;
    }
    let raw = 10 + u32::from(c as u8 - b'A');
    Some(raw + (raw - 1) / 10)
}

/// An ISO 6346 container number's check digit: each letter its number (A
/// is 10, skipping multiples of 11), each place weighted 2 to its power,
/// the sum mod 11, with 10 as 0.
pub fn is_container_number(text: &str) -> bool {
    let chars: Vec<char> = text.chars().collect();
    let [body @ .., check] = chars.as_slice() else {
        return false;
    };
    if chars.len() != 11 {
        return false;
    }
    let mut sum = 0u32;
    for (at, c) in body.iter().enumerate() {
        let Some(value) = container_value(*c) else {
            return false;
        };
        sum += value << at;
    }
    check.to_digit(10) == Some(sum % 11 % 10)
}

/// Verhoeff's dihedral group table: `D[a][b]` is `a` times `b`.
const VERHOEFF_D: [[u8; 10]; 10] = [
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

/// Verhoeff's permutation table: `P[i]` is the permutation for the digit
/// `i` places from the right, repeating every eight.
const VERHOEFF_P: [[u8; 10]; 8] = [
    [0, 1, 2, 3, 4, 5, 6, 7, 8, 9],
    [1, 5, 7, 6, 2, 8, 3, 0, 9, 4],
    [5, 8, 0, 3, 7, 9, 6, 1, 4, 2],
    [8, 9, 1, 6, 0, 4, 3, 5, 2, 7],
    [9, 4, 5, 3, 1, 2, 6, 8, 7, 0],
    [4, 2, 8, 6, 5, 7, 3, 9, 0, 1],
    [2, 7, 9, 3, 8, 0, 6, 4, 1, 5],
    [7, 0, 4, 6, 9, 1, 3, 2, 5, 8],
];

/// Whether `text`'s digits pass Verhoeff's check, its last the check digit.
fn passes_verhoeff(text: &str) -> bool {
    let mut check = Some(0u8);
    for (at, c) in text.chars().rev().enumerate() {
        let permuted = c.to_digit(10).and_then(|digit| {
            VERHOEFF_P
                .get(at % 8)
                .and_then(|row| row.get(digit as usize))
        });
        check = check.zip(permuted).and_then(|(check, permuted)| {
            VERHOEFF_D
                .get(usize::from(check))
                .and_then(|row| row.get(usize::from(*permuted)))
                .copied()
        });
    }
    check == Some(0)
}

/// A SNOMED CT concept identifier: Verhoeff's check digit, and a concept's
/// partition identifier, the two digits before it (`00` in the
/// International Edition, `10` in an extension's namespace).
pub fn is_snomed_concept(text: &str) -> bool {
    let partition = text
        .len()
        .checked_sub(3)
        .and_then(|at| text.get(at..at + 2));
    matches!(partition, Some("00" | "10")) && passes_verhoeff(text)
}

/// An ORCID iD's check character: ISO 7064 MOD 11-2 over its 15 digits.
pub fn is_orcid(text: &str) -> bool {
    let chars: Vec<char> = text.chars().filter(|c| *c != '-').collect();
    let [body @ .., check] = chars.as_slice() else {
        return false;
    };
    let mut total = 0u32;
    for c in body {
        let Some(digit) = c.to_digit(10) else {
            return false;
        };
        total = (total + digit) * 2;
    }
    let expected = match (12 - total % 11) % 11 {
        10 => 'X',
        n => char::from_digit(n, 10).unwrap_or('?'),
    };
    *check == expected
}

/// An MPAN core's check digit: its first 12 digits times the primes 3 to
/// 43 in turn (skipping 11), the sum mod 11, then mod 10.
pub fn is_mpan(text: &str) -> bool {
    const PRIMES: [u32; 12] = [3, 5, 7, 13, 17, 19, 23, 29, 31, 37, 41, 43];
    let digits: Option<Vec<u32>> = text.chars().map(|c| c.to_digit(10)).collect();
    let Some([body @ .., check]) = digits.as_deref() else {
        return false;
    };
    body.len() == PRIMES.len()
        && body
            .iter()
            .zip(PRIMES)
            .map(|(digit, prime)| digit * prime)
            .sum::<u32>()
            % 11
            % 10
            == *check
}

/// An ISBN-10 as its `Isbn` (ISO 2108; the ISBN Users' Manual, 7th
/// edition, 5): nine digits and a check character, a digit or `X` for 10,
/// read in either case, such that the ten weighted 10 down to 1 sum to a
/// multiple of 11; it is the ISBN-13 `978`, its first nine digits and
/// GS1's check digit (chunk 5.5m2). `None` for anything else, a wrong
/// check character included.
pub fn isbn_from_isbn10(text: &str) -> Option<String> {
    let chars: Vec<char> = text.chars().collect();
    let [body @ .., check] = chars.as_slice() else {
        return None;
    };
    if body.len() != 9 {
        return None;
    }
    let digits: Vec<u32> = body.iter().map(|c| c.to_digit(10)).collect::<Option<_>>()?;
    let check = match check {
        'X' | 'x' => 10,
        c => c.to_digit(10)?,
    };
    let sum: u32 = digits
        .iter()
        .chain(std::iter::once(&check))
        .zip((1..=10).rev())
        .map(|(digit, factor)| digit * factor)
        .sum();
    if !sum.is_multiple_of(11) {
        return None;
    }
    let mut isbn: String = std::iter::once("978".to_string())
        .chain(digits.iter().map(u32::to_string))
        .collect();
    let gs1: u32 = isbn
        .chars()
        .filter_map(|c| c.to_digit(10))
        .zip([1, 3].into_iter().cycle())
        .map(|(digit, times)| digit * times)
        .sum();
    isbn.push(char::from_digit((10 - gs1 % 10) % 10, 10)?);
    Some(isbn)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_digits_take_the_published_values() {
        // GS1's: a GTIN-13, a GTIN-12 and an SSCC.
        assert!(passes_gs1("5901234123457"));
        assert!(passes_gs1("036000291452"));
        assert!(!passes_gs1("036000291453"));
        assert!(passes_gs1("106141412345678908"));
        // NHTSA's example, and the VIN of all ones, whose check digit is 1.
        assert!(is_vin("1M8GDM9AXKP042788"));
        assert!(is_vin("11111111111111111"));
        assert!(!is_vin("1M8GDM9A1KP042788"));
        // ISO 6346's example.
        assert!(is_container_number("CSQU3054383"));
        assert!(!is_container_number("CSQU3054380"));
        // ORCID's examples.
        assert!(is_orcid("0000-0002-1694-233X"));
        assert!(!is_orcid("0000-0002-1694-2330"));
        // SNOMED CT's clinical finding.
        assert!(is_snomed_concept("404684003"));
        assert!(!is_snomed_concept("404684004"));
        assert!(is_mpan("1200012345679"));
        assert!(!is_mpan("1200012345670"));
    }

    #[test]
    fn an_isbn_10_reads_as_its_isbn() {
        // The ISBN Users' Manual's example, its check character `X` in
        // either case, and a wrong check character.
        assert_eq!(
            isbn_from_isbn10("0306406152").as_deref(),
            Some("9780306406157")
        );
        assert_eq!(
            isbn_from_isbn10("080442957X").as_deref(),
            Some("9780804429573")
        );
        assert_eq!(
            isbn_from_isbn10("080442957x").as_deref(),
            Some("9780804429573")
        );
        assert!(passes_gs1("9780804429573"));
        assert_eq!(isbn_from_isbn10("0306406153"), None);
        assert_eq!(isbn_from_isbn10("0-306-40615-2"), None);
        assert_eq!(isbn_from_isbn10("030640615"), None);
        assert_eq!(isbn_from_isbn10("X306406152"), None);
    }

    #[test]
    fn container_letters_skip_multiples_of_eleven() {
        let values: Vec<Option<u32>> = "ABKLUVZ".chars().map(container_value).collect();
        let expected = [10, 12, 21, 23, 32, 34, 38].map(Some);
        assert_eq!(values, expected);
        assert_eq!(container_value('a'), None);
    }
}
