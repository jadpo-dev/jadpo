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
            ("Ada", "Ada"),
            ("José María", "José María"),
            ("太郎", "太郎"),
        ],
        &["", " Ada", "Ada ", "Ada\nLovelace"],
    ),
);

/// A family name, as the person writes it.
pub const FAMILY_NAME: StandardType = text(
    "FamilyName",
    LINE_PATTERN,
    Some("identifying"),
    examples(
        &[
            ("Lovelace", "Lovelace"),
            ("van der Berg", "van der Berg"),
            ("山田", "山田"),
        ],
        &["", " Lovelace", "Lovelace\t", "Love\nlace"],
    ),
);

/// A login handle: lower-case letters, digits, `_`, `.` and `-`, 1 to 64
/// of them, as the catalogue sketched it.
pub const USERNAME: StandardType = text(
    "Username",
    "[a-z0-9_.-]{1,64}",
    Some("identifying"),
    examples(
        &[("ada", "ada"), ("ada.lovelace_1815", "ada.lovelace_1815")],
        &["Ada", "ada lovelace", "ada@example.com", ""],
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
    // 10 Downing Street's, from OS Places, and the smallest.
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
            (
                "Ada Lovelace <ada@example.com>",
                "Ada Lovelace <ada@example.com>",
            ),
            (
                "\"Lovelace, Ada\" <ada@example.com>",
                "\"Lovelace, Ada\" <ada@example.com>",
            ),
            ("ada@example.com", "ada@example.com"),
        ],
        &[
            "Ada Lovelace",
            "<ada@example.com>",
            "Ada Lovelace <ada>",
            "\"Ada Lovelace\" <ada@example.com>",
            "Ada Lovelace  <ada@example.com>",
            "",
        ],
    ),
    release: FIRST_RELEASE,
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
        fixed: None,
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
            (r#"{"given":"Ada","family":"Lovelace"}"#, "Ada Lovelace"),
            (r#"{"given":"Sukarno","family":null}"#, "Sukarno"),
        ],
        &[
            r#"{"family":"Lovelace"}"#,
            r#"{"given":"","family":"Lovelace"}"#,
            r#"{"given":"Ada\nLovelace","family":null}"#,
            r#""Ada Lovelace""#,
        ],
    ),
    release: FIRST_RELEASE,
};

/// A postal address in any country: its street lines, its locality, its
/// region and its postal code where the country uses them, and the
/// country. The lines are text as the person wrote them, as the catalogue
/// sketched them, stored as a `text[]`; the other parts are one line each.
/// It has no single text form; its examples are its JSON.
pub const POSTAL_ADDRESS: StandardType = StandardType {
    name: "PostalAddress",
    holds: Holds::Object(&[
        ("lines", Part::List(&Part::Of(Builtin::Text))),
        ("locality", Part::Optional(&Part::Line)),
        ("region", Part::Optional(&Part::Line)),
        ("postal_code", Part::Optional(&Part::Line)),
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
        fixed: None,
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
                r#"{"lines":["10 Downing Street"],"locality":"London","region":null,"postal_code":"SW1A 2AA","country":"GB"}"#,
                "10 Downing Street\nLondon\nSW1A 2AA",
            ),
            (
                r#"{"lines":["2-7-2 Marunouchi"],"locality":"Chiyoda-ku","region":"Tokyo","postal_code":"100-8994","country":"JP"}"#,
                "2-7-2 Marunouchi\nChiyoda-ku, Tokyo\n100-8994\nJapan",
            ),
        ],
        &[
            r#"{"lines":["10 Downing Street"],"locality":"London","region":null,"postal_code":"SW1A 2AA"}"#,
            r#"{"lines":["10 Downing Street"],"locality":" London","region":null,"postal_code":null,"country":"GB"}"#,
            r#"{"lines":[],"locality":null,"region":null,"postal_code":null,"country":"UK"}"#,
            r#""10 Downing Street, London""#,
        ],
    ),
    release: FIRST_RELEASE,
};
