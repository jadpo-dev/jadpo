//! People and contact (area 19, "The standard types"): the names, handles
//! and numbers held as text, each a pattern on the contract as the payment
//! numbers are (chunk 5.5i), and the three objects, `PersonName`,
//! `EmailWithName` and `PostalAddress`, whose parts code reads and which
//! are stored in a named column per part. A person's name and an address
//! have no single text form; `text.format` writes each in the order of the
//! reader's locale, a name by CLDR's person-name patterns and an address
//! by libaddressinput's per-country formats (`addresses_data`).
//!
//! Each is personal data whatever field holds it, and names the area 15
//! category it is: a name or a username `identifying`, a way to reach
//! someone `contact`, a nationality `demographic`.

use super::{
    Examples, FIRST_RELEASE, Format, FormatKey, Holds, Json, NO_OPTIONS, Part, StandardType,
    Storage, TextForm,
};
use crate::Builtin;
use crate::read::SqlType;

/// One line of text for people: 1 to 200 characters, with no control
/// character (so no line break) and no space at either end, as an invoice
/// number is (chunk 5.5i).
pub const LINE_PATTERN: &str = r"[^\s\p{Cc}](?:[^\p{Cc}]{0,198}[^\s\p{Cc}])?";

/// The longest a [`Part::Line`] is, in characters.
pub const LINE_MAX: usize = 200;

/// The categories of the standard types that are personal data and aren't
/// held as text, by name (area 15); a type held as text names its own.
pub const PERSONAL: &[(&str, &str)] = &[
    ("EmailWithName", "contact"),
    ("Nationality", "demographic"),
    ("PersonName", "identifying"),
    ("PostalAddress", "contact"),
];

/// A standard type held as text, checked by its pattern alone.
const fn text(
    name: &'static str,
    pattern: &'static str,
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
            key: FormatKey::Canonical,
            options: NO_OPTIONS,
        },
        ordered: false,
        validator: None,
        shape: None,
        examples,
        release: FIRST_RELEASE,
        signals: crate::standard::NO_SIGNALS,
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

/// A given name, as the person writes it.
pub const GIVEN_NAME: StandardType = text(
    "GivenName",
    LINE_PATTERN,
    Some("identifying"),
    examples(
        &[
            ("Mary", "Mary"),
            ("José María", "José María"),
            ("太郎", "太郎"),
        ],
        &["", " Mary", "Mary ", "Mary\nSomerville"],
    ),
);

/// A family name, as the person writes it.
pub const FAMILY_NAME: StandardType = text(
    "FamilyName",
    LINE_PATTERN,
    Some("identifying"),
    examples(
        &[
            ("Somerville", "Somerville"),
            ("van der Berg", "van der Berg"),
            ("山田", "山田"),
        ],
        &["", " Somerville", "Somerville\t", "Somer\nville"],
    ),
);

/// A login handle: lower-case letters, digits, `_`, `.` and `-`, 1 to 64
/// of them, as the catalogue sketched it.
pub const USERNAME: StandardType = text(
    "Username",
    "[a-z0-9_.-]{1,64}",
    Some("identifying"),
    examples(
        &[
            ("mary", "mary"),
            ("mary.somerville_1780", "mary.somerville_1780"),
        ],
        &["Mary", "mary somerville", "mary@example.com", ""],
    ),
);

/// An internal extension, dialled after a phone number: up to 20 digits,
/// the longest libphonenumber reads after `ext.`.
pub const PHONE_EXTENSION: StandardType = text(
    "PhoneExtension",
    "[0-9]{1,20}",
    Some("contact"),
    examples(
        &[("1", "1"), ("2047", "2047")],
        &["", "x2047", "20 47", "123456789012345678901"],
    ),
);

/// A Unique Property Reference Number (GeoPlace, UK): up to 12 digits
/// with no leading zero, held as text so none is ever lost.
pub const UPRN: StandardType = text(
    "Uprn",
    "[1-9][0-9]{0,11}",
    Some("contact"),
    // Buckingham Palace's, from OS Places, and the smallest.
    examples(
        &[("1", "1"), ("100023336956", "100023336956")],
        &["0", "0100023336956", "1000233369561", "UPRN1", ""],
    ),
);

/// A mailbox (RFC 5322): an address with the name shown with it, if any.
pub const EMAIL_WITH_NAME: StandardType = StandardType {
    name: "EmailWithName",
    holds: Holds::Object(&[
        ("name", Part::Optional(&Part::Line)),
        ("address", Part::Of(Builtin::Email)),
    ]),
    text: Some(TextForm::Mailbox),
    json: Json::Fields(&["name", "address"]),
    storage: Storage::Columns {
        parts: &[("name", "text"), ("address", "text")],
        fixed: &[],
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
            (
                "Mary Somerville <mary@example.com>",
                "Mary Somerville <mary@example.com>",
            ),
            (
                "\"Somerville, Mary\" <mary@example.com>",
                "\"Somerville, Mary\" <mary@example.com>",
            ),
            ("mary@example.com", "mary@example.com"),
        ],
        &[
            "Mary Somerville",
            "<mary@example.com>",
            "Mary Somerville <mary>",
            "\"Mary Somerville\" <mary@example.com>",
            "Mary Somerville  <mary@example.com>",
            "",
        ],
    ),
    release: FIRST_RELEASE,
    signals: crate::standard::NO_SIGNALS,
};

/// A person's name: their given name and, where they have one, their
/// family name. A single name is a given name, as CLDR holds a mononym.
/// It has no single text form; its examples are its JSON.
pub const PERSON_NAME: StandardType = StandardType {
    name: "PersonName",
    holds: Holds::Object(&[
        ("given", Part::Standard("GivenName")),
        ("family", Part::Optional(&Part::Standard("FamilyName"))),
    ]),
    text: None,
    json: Json::Fields(&["given", "family"]),
    storage: Storage::Columns {
        parts: &[("given", "text"), ("family", "text")],
        fixed: &[],
    },
    format: Format {
        key: FormatKey::PersonName,
        options: NO_OPTIONS,
    },
    ordered: false,
    validator: None,
    shape: None,
    examples: examples(
        &[
            (
                r#"{"given":"Mary","family":"Somerville"}"#,
                "Mary Somerville",
            ),
            (r#"{"given":"Sukarno","family":null}"#, "Sukarno"),
        ],
        &[
            r#"{"family":"Somerville"}"#,
            r#"{"given":"","family":"Somerville"}"#,
            r#"{"given":"Mary\nSomerville","family":null}"#,
            r#""Mary Somerville""#,
        ],
    ),
    release: FIRST_RELEASE,
    signals: crate::standard::NO_SIGNALS,
};

/// `PostalAddress`.
pub const POSTAL_ADDRESS_NAME: &str = "PostalAddress";

/// `PostalAddress`'s rule across its parts (chunk 5.5t2): its postal code,
/// where it has one, in its country's form (chunk 5.5l2).
pub const POSTAL_ADDRESS_RULES: &[super::across::StandardRule] = &[super::across::StandardRule {
    name: "postal_code_of_country",
    written: "postal_code is in the form of its country's postal codes",
    label: "not a postal code of the address's country",
    reason: "A `PostalAddress`'s `postal_code` is in the form its `country`'s postal service writes, so an address's code and country agree.",
    next: "Write a postal code of the country in its canonical text, or change `country`",
    check: super::across::Check::By(|parts| {
        super::across::fits(parts, "postal_code", &["country"], |code, by| {
            by.first()
                .is_some_and(|country| super::postal::fits(code, country))
        })
    }),
}];

/// A postal address in any country: its street lines, its locality, its
/// region and its postal code where the country uses them, and the
/// country. The lines are text as the person wrote them, as the catalogue
/// sketched them, stored as a `text[]`; the locality and region are one
/// line each, and the postal code a `PostalCode` that fits the country
/// (its rule `postal_code_of_country`, chunk 5.5t2).
/// It has no single text form; its examples are its JSON.
pub const POSTAL_ADDRESS: StandardType = StandardType {
    name: POSTAL_ADDRESS_NAME,
    holds: Holds::Object(&[
        ("lines", Part::List(&Part::Of(Builtin::Text))),
        ("locality", Part::Optional(&Part::Line)),
        ("region", Part::Optional(&Part::Line)),
        ("postal_code", Part::Optional(&Part::Standard("PostalCode"))),
        ("country", Part::Standard(super::places::COUNTRY)),
    ]),
    text: None,
    json: Json::Fields(&["lines", "locality", "region", "postal_code", "country"]),
    storage: Storage::Columns {
        parts: &[
            ("lines", "text[]"),
            ("locality", "text"),
            ("region", "text"),
            ("postal_code", "text"),
            ("country", "text"),
        ],
        fixed: &[],
    },
    format: Format {
        key: FormatKey::PostalAddress,
        options: NO_OPTIONS,
    },
    ordered: false,
    validator: None,
    shape: None,
    examples: examples(
        &[
            (
                r#"{"lines":["Buckingham Palace"],"locality":"London","region":null,"postal_code":"SW1A 1AA","country":"GB"}"#,
                "Buckingham Palace\nLondon\nSW1A 1AA",
            ),
            (
                r#"{"lines":["2-7-2 Marunouchi"],"locality":"Chiyoda-ku","region":"Tokyo","postal_code":"100-8994","country":"JP"}"#,
                "2-7-2 Marunouchi\nChiyoda-ku, Tokyo\n100-8994\nJapan",
            ),
        ],
        &[
            r#"{"lines":["Buckingham Palace"],"locality":"London","region":null,"postal_code":"SW1A 1AA"}"#,
            r#"{"lines":["Buckingham Palace"],"locality":" London","region":null,"postal_code":null,"country":"GB"}"#,
            r#"{"lines":[],"locality":null,"region":null,"postal_code":null,"country":"UK"}"#,
            r#"{"lines":["Buckingham Palace"],"locality":"London","region":null,"postal_code":"75004","country":"GB"}"#,
            r#"{"lines":["Buckingham Palace"],"locality":"London","region":null,"postal_code":"sw1a 1aa","country":"GB"}"#,
            r#""Buckingham Palace, London""#,
        ],
    ),
    release: FIRST_RELEASE,
    signals: crate::standard::NO_SIGNALS,
};
