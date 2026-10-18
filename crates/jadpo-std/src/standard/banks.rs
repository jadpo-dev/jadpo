//! `BankAccount` (area 19, "Money and payments", chunk 5.5i4): a choice
//! ([`Holds::Choice`]) of a UK sort code and account number, an IBAN, or
//! another country's account, each part checked by its own type's rules.
//! A UK account's number is checked against its sort code by Vocalink's
//! modulus check ([`super::modulus`]) only once its tables can ship, which
//! they can't yet ("Chosen in chunk 5.5i4"). It is financial personal data
//! (area 15), as an IBAN and a UK account number are, and is shown masked
//! to its number's last four, as a card number is (chunk 5.5i).

use super::{
    Examples, FIRST_RELEASE, Format, FormatKey, Holds, Json, NO_OPTIONS, Part, StandardType,
    Storage,
};
use crate::read::SqlType;

/// `BankAccount`.
pub const BANK_ACCOUNT: &str = "BankAccount";

/// The bounds of another country's bank code: 1 to 35 capital letters and
/// digits, the longest ISO 20022 lets a clearing system's member
/// identification be (`Max35Text`).
pub const BANK_CODE_BOUNDS: &str = "[A-Z0-9]{1,35}";

/// The bounds of another country's account number: 1 to 34 capital letters
/// and digits, the longest ISO 20022 lets an account's other
/// identification be (`Max34Text`), as long as an IBAN can be.
pub const ACCOUNT_NUMBER_BOUNDS: &str = "[A-Z0-9]{1,34}";

/// `BankAccount`'s variants: a UK account by its sort code and number, an
/// account by its IBAN, and another country's by its country, the code of
/// its bank where the country uses one (a US routing number, an Indian
/// IFSC), and its number.
pub const BANK_ACCOUNT_VARIANTS: &[(&str, &[(&str, Part)])] = &[
    (
        "uk",
        &[
            ("sort_code", Part::Standard("UkSortCode")),
            ("account_number", Part::Standard("UkAccountNumber")),
        ],
    ),
    ("iban", &[("iban", Part::Standard("Iban"))]),
    (
        "other",
        &[
            ("country", Part::Standard(super::places::COUNTRY)),
            (
                "bank_code",
                Part::Optional(&Part::Matching(BANK_CODE_BOUNDS)),
            ),
            ("account_number", Part::Matching(ACCOUNT_NUMBER_BOUNDS)),
        ],
    ),
];

/// The categories of the bank types that are personal data (area 15):
/// a bank account says where a person's money is, as its IBAN and its UK
/// account number do on their own (chunk 5.5i).
pub const PERSONAL: &[(&str, &str)] = &[(BANK_ACCOUNT, "financial")];

/// A bank account: a choice, sent tagged and stored as that JSON, shown as
/// its number's or IBAN's last four.
pub const BANK_ACCOUNT_ENTRY: StandardType = StandardType {
    name: BANK_ACCOUNT,
    holds: Holds::Choice(BANK_ACCOUNT_VARIANTS),
    text: None,
    json: Json::Tagged,
    storage: Storage::Column(SqlType::Json),
    format: Format {
        key: FormatKey::PartLastFour {
            parts: &["account_number", "iban"],
            mask: "•••• ",
        },
        options: NO_OPTIONS,
    },
    ordered: false,
    validator: None,
    shape: None,
    // Vocalink's test case 1 (its modulus checking specification, 9.10),
    // SWIFT's IBAN Registry's example for the United Kingdom, a US account
    // at JPMorgan Chase's published routing number, and a New Zealand
    // account, whose bank is part of its number; the account numbers but
    // Vocalink's are made up.
    examples: Examples {
        valid: &[
            (
                r#"{"tag":"uk","sort_code":"089999","account_number":"66374958"}"#,
                "•••• 4958",
            ),
            (
                r#"{"tag":"iban","iban":"GB29NWBK60161331926819"}"#,
                "•••• 6819",
            ),
            (
                r#"{"tag":"other","country":"US","bank_code":"021000021","account_number":"123456789"}"#,
                "•••• 6789",
            ),
            (
                r#"{"tag":"other","country":"NZ","bank_code":null,"account_number":"010202012345600"}"#,
                "•••• 5600",
            ),
        ],
        format: None,
        malformed: &[
            // A sort code with its dashes, an account number of seven
            // digits.
            r#"{"tag":"uk","sort_code":"08-99-99","account_number":"66374958"}"#,
            r#"{"tag":"uk","sort_code":"089999","account_number":"6637495"}"#,
            // An IBAN whose check digits fail, one as it is printed.
            r#"{"tag":"iban","iban":"GB29NWBK60161331926818"}"#,
            r#"{"tag":"iban","iban":"GB29 NWBK 6016 1331 9268 19"}"#,
            // No country `UK`, a number with a dash, a bank code too long,
            // an account with no number.
            r#"{"tag":"other","country":"UK","bank_code":"021000021","account_number":"123456789"}"#,
            r#"{"tag":"other","country":"US","bank_code":"021000021","account_number":"1234-5678"}"#,
            r#"{"tag":"other","country":"US","bank_code":"021000021021000021021000021021000021","account_number":"123456789"}"#,
            r#"{"tag":"other","country":"US","bank_code":"021000021"}"#,
            // A variant it hasn't, no tag, an IBAN on its own.
            r#"{"tag":"us","routing":"021000021","account":"123456789"}"#,
            r#"{"sort_code":"089999","account_number":"66374958"}"#,
            r#""GB29NWBK60161331926819""#,
        ],
        of: None,
        settings: "",
    },
    release: FIRST_RELEASE,
};
