//! The payment and identity numbers held as text (area 19, "Groups with
//! the baseline only", chunk 5.5i): each a pattern, matched against the
//! whole canonical text, and, where the page names one, a check digit
//! (IBAN's and LEI's mod-97, Luhn, an NHS number's mod-11), and the
//! published values the harness checks each with. A value is its canonical
//! text: the electronic form, upper case with no spaces or dashes, which is
//! what the wire, storage and `text.parse` carry; `text.format` writes the
//! printed form people read, grouped, or masked to its last four for a card
//! number and a social security number.

use super::{
    Examples, FIRST_RELEASE, Format, FormatKey, Grouping, Holds, Json, NO_OPTIONS, StandardType,
    Storage, TextForm, Validator,
};
use crate::read::SqlType;

/// A standard type held as text, checked by its pattern and validator.
const fn identifier(
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

/// Every four characters, as an IBAN is printed (ISO 13616).
const FOURS: Grouping = Grouping {
    sizes: &[4],
    separator: " ",
};

/// A routing number of the US banks (ABA): nine digits whose weighted sum,
/// 3, 7 and 1 in turn, is a multiple of ten.
pub const ABA_ROUTING_NUMBER: StandardType = identifier(
    "AbaRoutingNumber",
    "[0-9]{9}",
    Some(Validator::AbaRoutingNumber),
    CANONICAL,
    None,
    // The Federal Reserve Bank of Boston's and JPMorgan Chase's routing
    // numbers, from the Federal Reserve's E-Payments Routing Directory.
    examples(
        &[("011000015", "011000015"), ("021000021", "021000021")],
        &["011000016", "01100001", "0110000150", "01100001X", ""],
    ),
);

/// A bank identifier code (ISO 9362, SWIFT): a bank, a country, a place
/// and an optional branch, 8 or 11 characters.
pub const BIC: StandardType = identifier(
    "Bic",
    "[A-Z]{4}[A-Z]{2}[A-Z0-9]{2}(?:[A-Z0-9]{3})?",
    None,
    CANONICAL,
    None,
    // Deutsche Bank's head office and its branch 500, and NatWest's, as
    // each bank publishes them.
    examples(
        &[
            ("DEUTDEFF", "DEUTDEFF"),
            ("DEUTDEFF500", "DEUTDEFF500"),
            ("NWBKGB2L", "NWBKGB2L"),
        ],
        &["DEUTDEF", "deutdeff", "DEUTDEFF50", "DEU1DEFF", ""],
    ),
);

/// The first 6 or 8 digits of a card number, which name its issuer
/// (ISO/IEC 7812-1:2017 moved from 6 to 8).
pub const CARD_BIN: StandardType = identifier(
    "CardBin",
    "[0-9]{6}(?:[0-9]{2})?",
    None,
    CANONICAL,
    None,
    // The first six and eight digits of Stripe's test Visa card.
    examples(
        &[("424242", "424242"), ("42424242", "42424242")],
        &["4242", "4242424", "424242424", "42424A", ""],
    ),
);

/// An IBAN (ISO 13616) in its electronic form: a country, two check
/// digits making the whole mod-97 1, and the country's account number, as
/// long as the IBAN registry says for that country.
pub const IBAN: StandardType = identifier(
    "Iban",
    "[A-Z]{2}[0-9]{2}[A-Z0-9]{11,30}",
    Some(Validator::Iban),
    FormatKey::Grouped(FOURS),
    Some("financial"),
    // Examples from SWIFT's IBAN Registry, each country's own.
    examples(
        &[
            ("GB29NWBK60161331926819", "GB29 NWBK 6016 1331 9268 19"),
            ("DE89370400440532013000", "DE89 3704 0044 0532 0130 00"),
            (
                "FR1420041010050500013M02606",
                "FR14 2004 1010 0505 0001 3M02 606",
            ),
            ("NO9386011117947", "NO93 8601 1117 947"),
            (
                "MT84MALT011000012345MTLCAST001S",
                "MT84 MALT 0110 0001 2345 MTLC AST0 01S",
            ),
        ],
        &[
            // The check digits.
            "GB29NWBK60161331926818",
            // The printed form, and lower case.
            "GB29 NWBK 6016 1331 9268 19",
            "gb29nwbk60161331926819",
            // Too short for the United Kingdom's 22.
            "GB29NWBK6016133192681",
            // No such country.
            "XX29NWBK60161331926819",
            "",
        ],
    ),
);

/// An invoice's number (EN 16931, BT-1): any text a supplier gives, from 1
/// to 200 characters, with no control characters and no space at either
/// end.
pub const INVOICE_NUMBER: StandardType = identifier(
    "InvoiceNumber",
    r"[^\s\p{Cc}](?:[^\p{Cc}]{0,198}[^\s\p{Cc}])?",
    None,
    CANONICAL,
    None,
    // EN 16931 sets no form, so these are made up.
    examples(
        &[("INV-2026-0042", "INV-2026-0042"), ("2026/17", "2026/17")],
        &[" INV-1", "INV-1 ", "INV\n1", ""],
    ),
);

/// An International Securities Identification Number (ISO 6166): a
/// country, nine characters and a Luhn check digit over the letters as
/// numbers.
pub const ISIN: StandardType = identifier(
    "Isin",
    "[A-Z]{2}[A-Z0-9]{9}[0-9]",
    Some(Validator::Isin),
    CANONICAL,
    None,
    // Apple's, BAE Systems' and an Australian ISIN, the examples ANNA's
    // guide to ISO 6166 uses.
    examples(
        &[
            ("US0378331005", "US0378331005"),
            ("GB0002634946", "GB0002634946"),
            ("AU0000XVGZA3", "AU0000XVGZA3"),
        ],
        &[
            "US0378331004",
            "us0378331005",
            "US037833100",
            "US03783310050",
            "",
        ],
    ),
);

/// A Legal Entity Identifier (ISO 17442): 18 characters and two check
/// digits making the whole mod-97 1 (ISO 7064 mod 97-10).
pub const LEI: StandardType = identifier(
    "Lei",
    "[A-Z0-9]{18}[0-9]{2}",
    Some(Validator::Lei),
    CANONICAL,
    None,
    // GLEIF's own LEI and Apple's, from GLEIF's LEI search.
    examples(
        &[
            ("506700GE1G29325QX363", "506700GE1G29325QX363"),
            ("HWUPKR0MPOU8FGXBT394", "HWUPKR0MPOU8FGXBT394"),
        ],
        &[
            "506700GE1G29325QX364",
            "506700ge1g29325qx363",
            "506700GE1G29325QX3",
            "",
        ],
    ),
);

/// An NHS number (NHS England's Data Dictionary): ten digits, the last a
/// mod-11 check digit, never 10.
pub const NHS_NUMBER: StandardType = identifier(
    "NhsNumber",
    "[0-9]{10}",
    Some(Validator::NhsNumber),
    FormatKey::Grouped(Grouping {
        sizes: &[3, 3, 4],
        separator: " ",
    }),
    Some("health"),
    // The NHS Number standard's own example, and one from the 999 range
    // NHS England keeps for testing.
    examples(
        &[
            ("9434765919", "943 476 5919"),
            ("9990000018", "999 000 0018"),
        ],
        &[
            // The check digit.
            "9434765918",
            // Its check digit would be 10, so it is never issued.
            "1234567890",
            "943 476 5919",
            "943476591",
            "",
        ],
    ),
);

/// A payment card's number (ISO/IEC 7812): 12 to 19 digits, the last a
/// Luhn check digit. Shown to people as its last four only (PCI DSS).
pub const PAYMENT_CARD_NUMBER: StandardType = identifier(
    "PaymentCardNumber",
    "[0-9]{12,19}",
    Some(Validator::Luhn),
    FormatKey::LastFour {
        mask: "•••• "
    },
    Some("financial"),
    // Stripe's test Visa, American Express and Mastercard numbers
    // (Stripe's "Testing" documentation).
    examples(
        &[
            ("4242424242424242", "•••• 4242"),
            ("378282246310005", "•••• 0005"),
            ("5555555555554444", "•••• 4444"),
        ],
        &[
            // The Luhn check digit.
            "4242424242424241",
            "4242 4242 4242 4242",
            "42424242424",
            "42424242424242424242",
            "",
        ],
    ),
);

/// The reference a payer sees on a UK bank payment (Bacs and Faster
/// Payments): up to 18 of Bacs's characters, upper-case letters, digits,
/// `.`, `&`, `/`, `-` and spaces, with no space at either end.
pub const PAYMENT_REFERENCE: StandardType = identifier(
    "PaymentReference",
    "[A-Z0-9.&/-](?:[A-Z0-9.&/ -]{0,16}[A-Z0-9.&/-])?",
    None,
    CANONICAL,
    None,
    // Bacs's character set and Faster Payments' 18-character reference
    // (Pay.UK); the references are made up.
    examples(
        &[
            ("INV-2026/0042", "INV-2026/0042"),
            ("ACME & CO", "ACME & CO"),
        ],
        &[
            "inv-42",
            "REFERENCE-NUMBER-123",
            " INV42",
            "INV42 ",
            "INV#42",
            "",
        ],
    ),
);

/// A UK bank account number (Pay.UK): eight digits, leading zeros kept.
/// Its modulus check needs the sort code too, so it comes with
/// `BankAccount` (chunk 5.5i2).
pub const UK_ACCOUNT_NUMBER: StandardType = identifier(
    "UkAccountNumber",
    "[0-9]{8}",
    None,
    CANONICAL,
    Some("financial"),
    // Test cases 1 and 2 of Vocalink's "Validating account numbers"
    // (modulus checking) specification.
    examples(
        &[("66374958", "66374958"), ("88837491", "88837491")],
        &["6637495", "663749580", "6637495A", ""],
    ),
);

/// A UK company's number (Companies House): eight digits, or two letters
/// for the register (`SC` for Scotland) and six digits, leading zeros
/// kept.
pub const UK_COMPANY_NUMBER: StandardType = identifier(
    "UkCompanyNumber",
    "(?:[0-9]{2}|[A-Z]{2})[0-9]{6}",
    None,
    CANONICAL,
    None,
    // Tesco PLC's number on the Companies House register, and one on its
    // Scottish register, prefix `SC`.
    examples(
        &[("00445790", "00445790"), ("SC123456", "SC123456")],
        &["445790", "sc123456", "SC12345", "S1234567", ""],
    ),
);

/// A UK National Insurance number (HMRC's National Insurance Manual,
/// NIM39110): two prefix letters (never D, F, I, Q, U or V, the second
/// never O, and never `BG`, `GB`, `KN`, `NK`, `NT`, `TN` or `ZZ`), six
/// digits and a suffix from A to D.
pub const UK_NATIONAL_INSURANCE_NUMBER: StandardType = identifier(
    "UkNationalInsuranceNumber",
    "[A-CEGHJ-PR-TW-Z][A-CEGHJ-NPR-TW-Z][0-9]{6}[A-D]",
    Some(Validator::UkNationalInsuranceNumber),
    FormatKey::Grouped(Grouping {
        sizes: &[2],
        separator: " ",
    }),
    Some("government_id"),
    // Made up to NIM39110's rules; the refused `QQ123456C` is HMRC's own
    // specimen, which is never issued.
    examples(
        &[
            ("AB123456C", "AB 12 34 56 C"),
            ("CE987654A", "CE 98 76 54 A"),
        ],
        &[
            "QQ123456C",
            "GB123456A",
            "AO123456A",
            "AB123456E",
            "AB 12 34 56 C",
            "ab123456c",
            "",
        ],
    ),
);

/// A UK bank sort code (Pay.UK): six digits, written without dashes and
/// printed with them.
pub const UK_SORT_CODE: StandardType = identifier(
    "UkSortCode",
    "[0-9]{6}",
    None,
    FormatKey::Grouped(Grouping {
        sizes: &[2],
        separator: "-",
    }),
    None,
    // Test cases 1 and 2 of Vocalink's modulus checking specification.
    examples(
        &[("089999", "08-99-99"), ("107999", "10-79-99")],
        &["08-99-99", "08999", "0899991", ""],
    ),
);

/// A UK Unique Taxpayer Reference (HMRC): ten digits.
pub const UK_UTR: StandardType = identifier(
    "UkUtr",
    "[0-9]{10}",
    None,
    CANONICAL,
    Some("government_id"),
    // HMRC's form, ten digits; the numbers are made up.
    examples(
        &[("1234567890", "1234567890"), ("9876543210", "9876543210")],
        &["123456789", "12345 67890", "12345678901", ""],
    ),
);

/// A US Employer Identification Number (IRS): nine digits, written
/// without the dash and printed `12-3456789`.
pub const US_EIN: StandardType = identifier(
    "UsEin",
    "[0-9]{9}",
    None,
    FormatKey::Grouped(Grouping {
        sizes: &[2, 7],
        separator: "-",
    }),
    None,
    // The IRS's form, two digits and seven; the numbers are made up.
    examples(
        &[("123456789", "12-3456789"), ("987654321", "98-7654321")],
        &["12-3456789", "12345678", "1234567890", ""],
    ),
);

/// A US Social Security number: nine digits, written without dashes,
/// never area 000, 666 or 900 to 999, group 00 or serial 0000 (the Social
/// Security Administration's "Social Security Number Randomization").
/// Shown to people as its last four only.
pub const US_SSN: StandardType = identifier(
    "UsSsn",
    "(?:00[1-9]|0[1-9][0-9]|[1-5][0-9]{2}|6[0-57-9][0-9]|66[0-57-9]|[78][0-9]{2})(?:0[1-9]|[1-9][0-9])(?:000[1-9]|00[1-9][0-9]|0[1-9][0-9]{2}|[1-9][0-9]{3})",
    None,
    FormatKey::LastFour {
        mask: "•••-••-"
    },
    Some("government_id"),
    // Made up to the SSA's rules; each refused one breaks one of them.
    examples(
        &[("123456789", "•••-••-6789"), ("001010001", "•••-••-0001")],
        &[
            "000123456",
            "666123456",
            "900123456",
            "123006789",
            "123450000",
            "123-45-6789",
            "12345678",
            "",
        ],
    ),
);

/// Each country's IBAN length, from SWIFT's IBAN Registry.
const IBAN_LENGTHS: &[(&str, usize)] = &[
    ("AD", 24),
    ("AE", 23),
    ("AL", 28),
    ("AT", 20),
    ("AZ", 28),
    ("BA", 20),
    ("BE", 16),
    ("BG", 22),
    ("BH", 22),
    ("BI", 27),
    ("BR", 29),
    ("BY", 28),
    ("CH", 21),
    ("CR", 22),
    ("CY", 28),
    ("CZ", 24),
    ("DE", 22),
    ("DJ", 27),
    ("DK", 18),
    ("DO", 28),
    ("EE", 20),
    ("EG", 29),
    ("ES", 24),
    ("FI", 18),
    ("FK", 18),
    ("FO", 18),
    ("FR", 27),
    ("GB", 22),
    ("GE", 22),
    ("GI", 23),
    ("GL", 18),
    ("GR", 27),
    ("GT", 28),
    ("HR", 21),
    ("HU", 28),
    ("IE", 22),
    ("IL", 23),
    ("IQ", 23),
    ("IS", 26),
    ("IT", 27),
    ("JO", 30),
    ("KW", 30),
    ("KZ", 20),
    ("LB", 28),
    ("LC", 32),
    ("LI", 21),
    ("LT", 20),
    ("LU", 20),
    ("LV", 21),
    ("LY", 25),
    ("MC", 27),
    ("MD", 24),
    ("ME", 22),
    ("MK", 19),
    ("MN", 20),
    ("MR", 27),
    ("MT", 31),
    ("MU", 30),
    ("NI", 28),
    ("NL", 18),
    ("NO", 15),
    ("OM", 23),
    ("PK", 24),
    ("PL", 28),
    ("PS", 29),
    ("PT", 25),
    ("QA", 29),
    ("RO", 24),
    ("RS", 22),
    ("RU", 33),
    ("SA", 24),
    ("SC", 31),
    ("SD", 18),
    ("SE", 24),
    ("SI", 19),
    ("SK", 24),
    ("SM", 27),
    ("SO", 23),
    ("ST", 25),
    ("SV", 28),
    ("TL", 23),
    ("TN", 24),
    ("TR", 26),
    ("UA", 29),
    ("VA", 22),
    ("VG", 24),
    ("XK", 20),
    ("YE", 30),
];

/// The remainder of the digits `chars` make, each letter as its two-digit
/// number (`A` is 10, `Z` 35, ISO 7064), divided by 97; `None` for any
/// other character.
fn mod_97(chars: impl Iterator<Item = char>) -> Option<u32> {
    let mut remainder = 0u32;
    for c in chars {
        let value = c.to_digit(36)?;
        remainder = if value < 10 {
            (remainder * 10 + value) % 97
        } else {
            (remainder * 100 + value) % 97
        };
    }
    Some(remainder)
}

/// An IBAN: its country's length, and mod-97 1 with its first four
/// characters moved to the end (ISO 13616).
pub fn is_iban(text: &str) -> bool {
    let (Some(code), Some(rest)) = (text.get(..2), text.get(4..)) else {
        return false;
    };
    let fits = IBAN_LENGTHS
        .iter()
        .any(|(listed, length)| *listed == code && *length == text.len());
    fits && mod_97(rest.chars().chain(text.chars().take(4))) == Some(1)
}

/// A Legal Entity Identifier: mod-97 1 as it is (ISO 7064 mod 97-10).
pub fn is_lei(text: &str) -> bool {
    mod_97(text.chars()) == Some(1)
}

/// The Luhn check over `digits`, the last of them the check digit.
fn luhn(digits: impl DoubleEndedIterator<Item = u32>) -> bool {
    let sum: u32 = digits
        .rev()
        .enumerate()
        .map(|(at, digit)| match (at % 2, digit * 2) {
            (0, _) => digit,
            (_, doubled) if doubled > 9 => doubled - 9,
            (_, doubled) => doubled,
        })
        .sum();
    sum % 10 == 0
}

/// A card number's Luhn check digit.
pub fn passes_luhn(text: &str) -> bool {
    text.chars().all(|c| c.is_ascii_digit()) && luhn(text.chars().filter_map(|c| c.to_digit(10)))
}

/// An ISIN's check digit: Luhn over the digits its characters make, each
/// letter as its two-digit number (ISO 6166).
pub fn is_isin(text: &str) -> bool {
    let mut digits = Vec::new();
    for c in text.chars() {
        let Some(value) = c.to_digit(36) else {
            return false;
        };
        if value >= 10 {
            digits.push(value / 10);
        }
        digits.push(value % 10);
    }
    luhn(digits.into_iter())
}

/// An ABA routing number's checksum: 3, 7 and 1 times its digits in turn,
/// a multiple of ten.
pub fn is_aba_routing_number(text: &str) -> bool {
    let sum: u32 = text
        .chars()
        .filter_map(|c| c.to_digit(10))
        .zip([3, 7, 1].into_iter().cycle())
        .map(|(digit, times)| digit * times)
        .sum();
    text.len() == 9 && sum % 10 == 0
}

/// An NHS number's check digit: 11 less the sum of its first nine digits
/// weighted 10 down to 2, mod 11, with 11 as 0 and 10 never issued.
pub fn is_nhs_number(text: &str) -> bool {
    let digits: Vec<u32> = text.chars().filter_map(|c| c.to_digit(10)).collect();
    let [first @ .., check] = digits.as_slice() else {
        return false;
    };
    if digits.len() != 10 {
        return false;
    }
    let sum: u32 = first
        .iter()
        .zip((2..=10).rev())
        .map(|(digit, times)| digit * times)
        .sum();
    match 11 - sum % 11 {
        11 => *check == 0,
        10 => false,
        expected => *check == expected,
    }
}

/// A National Insurance number's prefix: none HMRC never allocates
/// (NIM39110). The letters each place takes are its pattern.
pub fn is_uk_national_insurance_number(text: &str) -> bool {
    text.get(..2)
        .is_some_and(|prefix| !["BG", "GB", "KN", "NK", "NT", "TN", "ZZ"].contains(&prefix))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_digits_take_the_published_values() {
        assert!(is_iban("GB82WEST12345698765432"), "ECBS's example");
        assert!(!is_iban("GB82WEST12345698765431"));
        assert!(is_lei("5493001KJTIIGC8Y1R12"));
        assert!(passes_luhn("4111111111111111"));
        assert!(!passes_luhn("4111111111111112"));
        assert!(is_isin("US5949181045"));
        assert!(is_aba_routing_number("111000025"));
        assert!(is_nhs_number("4010232137"));
        assert!(!is_nhs_number("123456789"));
        assert!(!is_uk_national_insurance_number("TN123456A"));
    }

    #[test]
    fn iban_lengths_are_sorted_and_within_the_standard() {
        let codes: Vec<&str> = IBAN_LENGTHS.iter().map(|(code, _)| *code).collect();
        assert!(codes.is_sorted());
        assert!(
            IBAN_LENGTHS
                .iter()
                .all(|(_, length)| (15..=34).contains(length))
        );
    }
}
