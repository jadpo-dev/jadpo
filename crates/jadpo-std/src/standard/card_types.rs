//! Payment cards beyond their number (area 19, "Money and banking",
//! chunk 5.5i2): `CardBrand`, the card schemes as a closed list;
//! `MaskedCardNumber`, what may be kept and shown of a card once its number
//! is gone, its brand and last four, which `money.mask` makes from a
//! `PaymentCardNumber` (chunk 5.5i); and `CardExpiry`, the month a card
//! runs to. A brand is told by the number's first digits, its issuer
//! identification number, from the ranges each scheme publishes.

use super::{
    Examples, FIRST_RELEASE, Format, FormatKey, Holds, Json, NO_OPTIONS, Part, StandardType,
    Storage, TextForm,
};
use crate::read::SqlType;

/// `CardBrand`.
pub const CARD_BRAND: &str = "CardBrand";
/// `MaskedCardNumber`.
pub const MASKED_CARD_NUMBER: &str = "MaskedCardNumber";
/// `CardExpiry`.
pub const CARD_EXPIRY: &str = "CardExpiry";

/// The schemes that issue cards under their own ranges worldwide, by code,
/// each its own name, as `Continent`'s are, since no authority codes them.
pub const CARD_BRANDS: &[(&str, &str)] = &[
    ("american_express", "american_express"),
    ("diners_club", "diners_club"),
    ("discover", "discover"),
    ("jcb", "jcb"),
    ("maestro", "maestro"),
    ("mastercard", "mastercard"),
    ("unionpay", "unionpay"),
    ("visa", "visa"),
];

/// Each brand's name as the scheme writes it, in `CARD_BRANDS`' order.
pub const CARD_BRAND_NAMES: &[&str] = &[
    "American Express",
    "Diners Club",
    "Discover",
    "JCB",
    "Maestro",
    "Mastercard",
    "UnionPay",
    "Visa",
];

/// The issuer ranges that name a brand: the first digits of a card number
/// from `low` to `high`, both included and as long as each other, and the
/// brand's place in `CARD_BRANDS`. The longest range a number starts with
/// wins, so Maestro's `6304` isn't read as anything wider. From Discover
/// Global Network's IIN range summary for its own, Diners Club
/// International's, JCB's and UnionPay's (Discover `6011` and
/// `644`–`658`; Diners Club `300`–`305`, `3095`, `36` and `38`–`39`; JCB
/// `3528`–`3589`; UnionPay `62` and `8100`–`8171`), Mastercard's for its
/// `51`–`55` and its 2-series `2221`–`2720`, and the schemes' long-known
/// ranges for Visa's `4`, American Express's `34` and `37`, and Maestro's
/// `5018`, `5020`, `5038`, `5893`, `6304`, `6759` and `6761`–`6763`.
pub const CARD_RANGES: &[(&str, &str, usize)] = &[
    ("34", "34", 0),
    ("37", "37", 0),
    ("300", "305", 1),
    ("3095", "3095", 1),
    ("36", "36", 1),
    ("38", "39", 1),
    ("6011", "6011", 2),
    ("644", "658", 2),
    ("3528", "3589", 3),
    ("5018", "5018", 4),
    ("5020", "5020", 4),
    ("5038", "5038", 4),
    ("5893", "5893", 4),
    ("6304", "6304", 4),
    ("6759", "6759", 4),
    ("6761", "6763", 4),
    ("51", "55", 5),
    ("2221", "2720", 5),
    ("62", "62", 6),
    ("8100", "8171", 6),
    ("4", "4", 7),
];

/// The brand a card number's issuer range names, by its place in
/// `CARD_BRANDS`; `None` for a range no listed scheme publishes.
pub fn brand_of(number: &str) -> Option<usize> {
    CARD_RANGES
        .iter()
        .filter(|(low, high, _)| {
            number
                .get(..low.len())
                .is_some_and(|head| *low <= head && head <= *high)
        })
        .max_by_key(|(low, _, _)| low.len())
        .map(|(_, _, brand)| *brand)
}

/// The categories of the card types that are personal data (area 15): a
/// card's last four with its brand, and its expiry, are what PCI DSS calls
/// cardholder data beside the number, so `financial`, as the number is.
pub const PERSONAL: &[(&str, &str)] = &[
    (CARD_EXPIRY, "financial"),
    (MASKED_CARD_NUMBER, "financial"),
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

/// A card scheme.
pub const CARD_BRAND_TYPE: StandardType = StandardType {
    name: CARD_BRAND,
    holds: Holds::Enum(CARD_BRANDS),
    text: Some(TextForm::Code),
    json: Json::Text,
    storage: Storage::Column(SqlType::Text),
    format: Format {
        key: FormatKey::Names(CARD_BRAND_NAMES),
        options: NO_OPTIONS,
    },
    ordered: false,
    validator: None,
    shape: None,
    examples: examples(
        &[
            ("american_express", "American Express"),
            ("mastercard", "Mastercard"),
            ("visa", "Visa"),
        ],
        &["americanexpress", "Visa Electron", "master_card", ""],
    ),
    release: FIRST_RELEASE,
    signals: crate::standard::NO_SIGNALS,
};

/// The month a card runs to, to the end of it: a year from 2000 to 2099
/// and a month, ordered, sent as its text and stored a column per part.
pub const CARD_EXPIRY_TYPE: StandardType = StandardType {
    name: CARD_EXPIRY,
    holds: Holds::Object(&[
        ("year", Part::Whole(2000, 2099)),
        ("month", Part::Whole(1, 12)),
    ]),
    text: Some(TextForm::CardExpiry),
    json: Json::Text,
    storage: Storage::Columns {
        parts: &[("year", "integer"), ("month", "integer")],
        fixed: &[],
    },
    format: Format {
        key: FormatKey::CardExpiry,
        options: NO_OPTIONS,
    },
    ordered: true,
    validator: None,
    shape: None,
    examples: examples(
        &[("2026-12", "12/26"), ("2028-01", "01/28")],
        &[
            "12/28", "2028-13", "2028-00", "1999-12", "2100-01", "2028-1", "",
        ],
    ),
    release: FIRST_RELEASE,
    signals: crate::standard::NO_SIGNALS,
};

/// What may be kept of a card once its number is gone: its brand, where a
/// listed scheme issued it, and its last four digits.
pub const MASKED_CARD_NUMBER_TYPE: StandardType = StandardType {
    name: MASKED_CARD_NUMBER,
    holds: Holds::Object(&[
        ("brand", Part::Optional(&Part::Standard(CARD_BRAND))),
        ("last_four", Part::Matching("[0-9]{4}")),
    ]),
    text: None,
    json: Json::Fields(&["brand", "last_four"]),
    storage: Storage::Columns {
        parts: &[("brand", "text"), ("last_four", "text")],
        fixed: &[],
    },
    format: Format {
        key: FormatKey::MaskedCard,
        options: NO_OPTIONS,
    },
    ordered: false,
    validator: None,
    shape: None,
    examples: examples(
        &[
            (r#"{"brand":"visa","last_four":"4242"}"#, "Visa •••• 4242"),
            (
                r#"{"brand":"american_express","last_four":"0005"}"#,
                "American Express •••• 0005",
            ),
            (r#"{"brand":null,"last_four":"1234"}"#, "•••• 1234"),
        ],
        &[
            r#"{"brand":"visa","last_four":"424"}"#,
            r#"{"brand":"visa","last_four":"42424"}"#,
            r#"{"brand":"diners","last_four":"0005"}"#,
            r#"{"brand":"visa"}"#,
            r#""4242""#,
        ],
    ),
    release: FIRST_RELEASE,
    signals: crate::standard::NO_SIGNALS,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_number_s_first_digits_name_its_brand() {
        let brand = |number: &str| brand_of(number).map(|at| CARD_BRANDS[at].0);
        // Stripe's test cards, one per brand.
        assert_eq!(brand("4242424242424242"), Some("visa"));
        assert_eq!(brand("5555555555554444"), Some("mastercard"));
        assert_eq!(brand("2223003122003222"), Some("mastercard"));
        assert_eq!(brand("378282246310005"), Some("american_express"));
        assert_eq!(brand("6011111111111117"), Some("discover"));
        assert_eq!(brand("3056930009020004"), Some("diners_club"));
        assert_eq!(brand("36227206271667"), Some("diners_club"));
        assert_eq!(brand("3566002020360505"), Some("jcb"));
        assert_eq!(brand("6200000000000005"), Some("unionpay"));
        assert_eq!(brand("6205500000000000004"), Some("unionpay"));
        assert_eq!(brand("6555900000604105"), Some("discover"));
        assert_eq!(brand("6590000000000000"), None, "past Discover's 658");
        assert_eq!(brand("6304000000000000"), Some("maestro"));
        // The 2-series' ends, and a number no listed scheme issues.
        assert_eq!(brand("2221000000000009"), Some("mastercard"));
        assert_eq!(brand("2720990000000007"), Some("mastercard"));
        assert_eq!(brand("2721000000000004"), None);
        assert_eq!(brand("9999999999999995"), None);
        assert_eq!(brand(""), None);
    }
}
