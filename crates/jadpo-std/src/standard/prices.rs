//! The money group's records (area 19, "The standard types", chunk 5.5i3):
//! `Price`, `UnitPrice`, `TaxRate`, `Discount`, `BasisPoints` and
//! `InterestRate`, on `Money`, `Percentage` and the quantities, with
//! `InterestRateBasis`, an interest rate's basis. `Discount` and
//! `UnitPrice` are choices of other types ([`Holds::Choice`]), the first
//! standard types that are, and `TaxRate` and `BasisPoints` are built on a
//! built-in type with bounds of their own ([`Holds::Within`]); each is an
//! entry like the rest, with no code of its own.

use super::{
    Examples, FIRST_RELEASE, Format, FormatKey, Holds, Json, NO_OPTIONS, Part, StandardType,
    Storage, TextForm,
};
use crate::read::SqlType;
use crate::{Builtin, Num, QuantityKind};

/// `Price`.
pub const PRICE: &str = "Price";
/// `UnitPrice`.
pub const UNIT_PRICE: &str = "UnitPrice";
/// `TaxRate`.
pub const TAX_RATE: &str = "TaxRate";
/// `Discount`.
pub const DISCOUNT: &str = "Discount";
/// `BasisPoints`.
pub const BASIS_POINTS: &str = "BasisPoints";
/// `InterestRate`.
pub const INTEREST_RATE: &str = "InterestRate";
/// `InterestRateBasis`.
pub const INTEREST_RATE_BASIS: &str = "InterestRateBasis";

const ZERO: Num = Num {
    mantissa: 0,
    scale: 0,
};

/// 100%, the fraction 1.00.
const WHOLE: Num = Num {
    mantissa: 100,
    scale: 2,
};

/// An amount never below 0: a price, an amount off.
const AMOUNT: Part = Part::Within {
    of: Builtin::Money,
    min: Some(ZERO),
    max: None,
};

/// A quantity of `kind` never below 0: what a unit price is per.
const fn per(kind: QuantityKind) -> Part {
    Part::Within {
        of: Builtin::Quantity(kind),
        min: Some(ZERO),
        max: None,
    }
}

/// `Price`'s fields, in order: its amount, and whether that includes tax,
/// the gross price, or not, the net (the catalogue's shape).
pub const PRICE_FIELDS: [(&str, Part); 2] = [
    ("amount", AMOUNT),
    ("includes_tax", Part::Of(Builtin::Bool)),
];

/// `Discount`'s variants: an amount off, never below 0, or a percentage
/// off, from 0% to 100%.
pub const DISCOUNT_VARIANTS: &[(&str, &[(&str, Part)])] = &[
    ("amount_off", &[("amount", AMOUNT)]),
    (
        "percentage_off",
        &[(
            "percentage",
            Part::Within {
                of: Builtin::Percentage,
                min: Some(ZERO),
                max: Some(WHOLE),
            },
        )],
    ),
];

/// `UnitPrice`'s variants: a price per an amount of each kind of quantity
/// the UK's Price Marking Order 2004 (Schedule 1) prices goods by, weight,
/// volume, length and area, in that order.
pub const UNIT_PRICE_VARIANTS: &[(&str, &[(&str, Part)])] = &[
    (
        "per_mass",
        &[("price", AMOUNT), ("per", per(QuantityKind::Mass))],
    ),
    (
        "per_volume",
        &[("price", AMOUNT), ("per", per(QuantityKind::Volume))],
    ),
    (
        "per_length",
        &[("price", AMOUNT), ("per", per(QuantityKind::Length))],
    ),
    (
        "per_area",
        &[("price", AMOUNT), ("per", per(QuantityKind::Area))],
    ),
];

/// `InterestRateBasis`' variants, each its own code: the annual equivalent
/// rate of savings, the annual percentage rate of borrowing, and a flat
/// rate on the sum first lent.
pub const INTEREST_RATE_BASES: &[(&str, &str)] =
    &[("aer", "aer"), ("apr", "apr"), ("flat", "flat")];

/// Each basis as a rate is written beside it, in `INTEREST_RATE_BASES`'
/// order, the same in every locale.
pub const INTEREST_RATE_BASIS_NAMES: &[&str] = &["AER", "APR", "flat"];

/// `InterestRate`'s fields, in order: the rate a year, which may be below
/// 0, as central banks' have been, and its basis.
pub const INTEREST_RATE_FIELDS: [(&str, Part); 2] = [
    ("rate", Part::Of(Builtin::Percentage)),
    ("basis", Part::Standard(INTEREST_RATE_BASIS)),
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

/// A count of hundredths of a percent, a whole number of any sign, shown
/// with `bp`.
pub const BASIS_POINTS_ENTRY: StandardType = StandardType {
    name: BASIS_POINTS,
    holds: Holds::Within {
        of: Builtin::Int,
        min: None,
        max: None,
    },
    text: Some(TextForm::Whole),
    json: Json::Text,
    storage: Storage::Column(SqlType::Int8),
    format: Format {
        key: FormatKey::Suffixed(" bp"),
        options: NO_OPTIONS,
    },
    ordered: true,
    validator: None,
    shape: None,
    examples: examples(
        &[
            ("-25", "-25 bp"),
            ("0", "0 bp"),
            ("25", "25 bp"),
            ("1500", "1,500 bp"),
        ],
        &["25bp", "25 bp", "0.25%", "2.5", "+25", ""],
    ),
    release: FIRST_RELEASE,
};

/// An amount off or a percentage off: a choice, sent tagged and stored as
/// that JSON, shown as the part its variant has.
pub const DISCOUNT_ENTRY: StandardType = StandardType {
    name: DISCOUNT,
    holds: Holds::Choice(DISCOUNT_VARIANTS),
    text: None,
    json: Json::Tagged,
    storage: Storage::Column(SqlType::Json),
    format: Format {
        key: FormatKey::Parts {
            parts: &["amount", "percentage"],
            separator: "",
        },
        options: NO_OPTIONS,
    },
    ordered: false,
    validator: None,
    shape: None,
    examples: examples(
        &[
            (
                r#"{"tag":"amount_off","amount":{"amount":500,"currency":"GBP"}}"#,
                "£5.00",
            ),
            (r#"{"tag":"percentage_off","percentage":"20%"}"#, "20%"),
            (r#"{"tag":"percentage_off","percentage":"12.5%"}"#, "12.5%"),
        ],
        &[
            // An amount below 0, a percentage past either end.
            r#"{"tag":"amount_off","amount":{"amount":-500,"currency":"GBP"}}"#,
            r#"{"tag":"percentage_off","percentage":"120%"}"#,
            r#"{"tag":"percentage_off","percentage":"-5%"}"#,
            // A percentage as a bare number, a variant it hasn't, no tag,
            // a variant without its part.
            r#"{"tag":"percentage_off","percentage":0.2}"#,
            r#"{"tag":"bogof"}"#,
            r#"{"percentage":"20%"}"#,
            r#"{"tag":"amount_off"}"#,
            r#""20%""#,
        ],
    ),
    release: FIRST_RELEASE,
};

/// A rate of interest a year and its basis, sent as its fields, stored as
/// that JSON and shown as a lender writes it, `5.5% APR`.
pub const INTEREST_RATE_ENTRY: StandardType = StandardType {
    name: INTEREST_RATE,
    holds: Holds::Object(&INTEREST_RATE_FIELDS),
    text: None,
    json: Json::Fields(&["rate", "basis"]),
    storage: Storage::Column(SqlType::Json),
    format: Format {
        key: FormatKey::Parts {
            parts: &["rate", "basis"],
            separator: " ",
        },
        options: NO_OPTIONS,
    },
    ordered: false,
    validator: None,
    shape: None,
    examples: examples(
        &[
            (r#"{"rate":"5.5%","basis":"apr"}"#, "5.5% APR"),
            (r#"{"rate":"4.75%","basis":"aer"}"#, "4.75% AER"),
            (r#"{"rate":"-0.5%","basis":"aer"}"#, "-0.5% AER"),
            (r#"{"rate":"3%","basis":"flat"}"#, "3% flat"),
        ],
        &[
            r#"{"rate":"5.5","basis":"apr"}"#,
            r#"{"rate":0.055,"basis":"apr"}"#,
            r#"{"rate":"5.5%","basis":"apy"}"#,
            r#"{"rate":"5.5%"}"#,
            r#""5.5% APR""#,
        ],
    ),
    release: FIRST_RELEASE,
};

/// The basis an interest rate is quoted on.
pub const INTEREST_RATE_BASIS_ENTRY: StandardType = StandardType {
    name: INTEREST_RATE_BASIS,
    holds: Holds::Enum(INTEREST_RATE_BASES),
    text: Some(TextForm::Code),
    json: Json::Text,
    storage: Storage::Column(SqlType::Text),
    format: Format {
        key: FormatKey::Names(INTEREST_RATE_BASIS_NAMES),
        options: NO_OPTIONS,
    },
    ordered: false,
    validator: None,
    shape: None,
    examples: examples(
        &[("aer", "AER"), ("apr", "APR"), ("flat", "flat")],
        &["apy", "annual", "a p r", ""],
    ),
    release: FIRST_RELEASE,
};

/// An amount and whether it includes tax, sent as its fields, stored as
/// that JSON and shown as its amount.
pub const PRICE_ENTRY: StandardType = StandardType {
    name: PRICE,
    holds: Holds::Object(&PRICE_FIELDS),
    text: None,
    json: Json::Fields(&["amount", "includes_tax"]),
    storage: Storage::Column(SqlType::Json),
    format: Format {
        key: FormatKey::Parts {
            parts: &["amount"],
            separator: "",
        },
        options: NO_OPTIONS,
    },
    ordered: false,
    validator: None,
    shape: None,
    examples: examples(
        &[
            (
                r#"{"amount":{"amount":1050,"currency":"GBP"},"includes_tax":true}"#,
                "£10.50",
            ),
            (
                r#"{"amount":{"amount":875,"currency":"GBP"},"includes_tax":false}"#,
                "£8.75",
            ),
            (
                r#"{"amount":{"amount":1200,"currency":"EUR"},"includes_tax":true}"#,
                "€12.00",
            ),
        ],
        &[
            r#"{"amount":{"amount":-1,"currency":"GBP"},"includes_tax":true}"#,
            r#"{"amount":{"amount":1050,"currency":"GBP"}}"#,
            r#"{"amount":{"amount":1050,"currency":"GBP"},"includes_tax":"yes"}"#,
            r#"{"amount":"GBP 10.50","includes_tax":true}"#,
            r#""GBP 10.50""#,
        ],
    ),
    release: FIRST_RELEASE,
};

/// A rate of tax, a percentage never below 0%: its text, JSON, storage and
/// format are a `Percentage`'s.
pub const TAX_RATE_ENTRY: StandardType = StandardType {
    name: TAX_RATE,
    holds: Holds::Within {
        of: Builtin::Percentage,
        min: Some(ZERO),
        max: None,
    },
    text: Some(TextForm::Percentage),
    json: Json::Text,
    storage: Storage::Column(SqlType::Numeric),
    format: Format {
        key: FormatKey::Percent,
        options: NO_OPTIONS,
    },
    ordered: true,
    validator: None,
    shape: None,
    examples: examples(
        &[("0%", "0%"), ("5%", "5%"), ("20%", "20%")],
        &["20", "0.2", "twenty percent", "%", ""],
    ),
    release: FIRST_RELEASE,
};

/// A price per an amount of a quantity: a choice by the quantity's kind,
/// sent tagged, stored as that JSON, and shown as the price over the
/// amount, `£2.50/1 kg`.
pub const UNIT_PRICE_ENTRY: StandardType = StandardType {
    name: UNIT_PRICE,
    holds: Holds::Choice(UNIT_PRICE_VARIANTS),
    text: None,
    json: Json::Tagged,
    storage: Storage::Column(SqlType::Json),
    format: Format {
        key: FormatKey::Parts {
            parts: &["price", "per"],
            separator: "/",
        },
        options: NO_OPTIONS,
    },
    ordered: false,
    validator: None,
    shape: None,
    examples: examples(
        &[
            (
                r#"{"tag":"per_mass","price":{"amount":250,"currency":"GBP"},"per":{"value":"1","unit":"kg"}}"#,
                "£2.50/1 kg",
            ),
            (
                r#"{"tag":"per_volume","price":{"amount":180,"currency":"GBP"},"per":{"value":"0.001","unit":"cubic_metre"}}"#,
                "£1.80/0.001 m³",
            ),
            (
                r#"{"tag":"per_length","price":{"amount":399,"currency":"GBP"},"per":{"value":"1","unit":"metre"}}"#,
                "£3.99/1 m",
            ),
        ],
        &[
            // A price below 0, an amount below 0, an amount of the wrong
            // kind, a kind it doesn't price by, and no amount at all.
            r#"{"tag":"per_mass","price":{"amount":-250,"currency":"GBP"},"per":{"value":"1","unit":"kg"}}"#,
            r#"{"tag":"per_mass","price":{"amount":250,"currency":"GBP"},"per":{"value":"-1","unit":"kg"}}"#,
            r#"{"tag":"per_mass","price":{"amount":250,"currency":"GBP"},"per":{"value":"1","unit":"metre"}}"#,
            r#"{"tag":"per_hour","price":{"amount":250,"currency":"GBP"},"per":{"value":"1","unit":"kg"}}"#,
            r#"{"tag":"per_mass","price":{"amount":250,"currency":"GBP"}}"#,
        ],
    ),
    release: FIRST_RELEASE,
};
