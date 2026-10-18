//! The records of commerce, travel and the other published standards
//! (area 19, "The standard types", chunk 5.5m2), each made of other types:
//! a `Barcode` by its symbology, a `UnitQuantity` by what it counts or
//! measures, a `Rating` out of its scale, a `ClothingSize` and a `ShoeSize`
//! in their systems, a parcel's `Dimensions` from `Length`s, a
//! `TrackingNumber` on 5.5m's `Carrier`, a `VehicleRegistration` by its
//! country, a `FlightNumber` on 5.5m's `IataAirline`, a `Jurisdiction` of a
//! country or a subdivision, and an AI `ModelId`; with `ClothingSizeSystem`
//! and `ShoeSizeSystem`, the systems the sizes are in. Each is an entry
//! like the rest; what the page left open is area 19's "Chosen in chunk
//! 5.5m2".

use super::across::{Order, StandardRule, in_order};
use super::{
    Examples, FIRST_RELEASE, Format, FormatKey, FormatOption, Holds, Json, NO_OPTIONS,
    Part, ShownIn, StandardType, Storage, TextForm,
};
use crate::read::SqlType;
use crate::{Builtin, Num, QuantityKind};

/// `Barcode`.
pub const BARCODE: &str = "Barcode";
/// `UnitQuantity`.
pub const UNIT_QUANTITY: &str = "UnitQuantity";
/// `Rating`.
pub const RATING: &str = "Rating";
/// `ClothingSize`.
pub const CLOTHING_SIZE: &str = "ClothingSize";
/// `ClothingSizeSystem`: the system a `ClothingSize` is in.
pub const CLOTHING_SIZE_SYSTEM: &str = "ClothingSizeSystem";
/// `ShoeSize`.
pub const SHOE_SIZE: &str = "ShoeSize";
/// `ShoeSizeSystem`: the system a `ShoeSize` is in.
pub const SHOE_SIZE_SYSTEM: &str = "ShoeSizeSystem";
/// `Dimensions`.
pub const DIMENSIONS: &str = "Dimensions";
/// `TrackingNumber`.
pub const TRACKING_NUMBER: &str = "TrackingNumber";
/// `VehicleRegistration`.
pub const VEHICLE_REGISTRATION: &str = "VehicleRegistration";
/// `FlightNumber`.
pub const FLIGHT_NUMBER: &str = "FlightNumber";
/// `Jurisdiction`.
pub const JURISDICTION: &str = "Jurisdiction";
/// `ModelId`.
pub const MODEL_ID: &str = "ModelId";

const ZERO: Num = Num {
    mantissa: 0,
    scale: 0,
};

/// The most a count goes to: JavaScript's largest safe integer, so it is
/// always sent as a JSON number a client reads exactly.
const MOST_COUNTED: i64 = crate::http::SAFE_INTEGER as i64;

/// A quantity of `kind` never below 0.
const fn never_negative(kind: QuantityKind) -> Part {
    Part::Bounded {
        of: Builtin::Quantity(kind),
        min: Some(ZERO),
        max: None,
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

const CANONICAL: Format = Format {
    key: FormatKey::Canonical,
    options: NO_OPTIONS,
};

/// An enum whose codes are written the same in every language, shown by
/// its code.
const fn system(
    name: &'static str,
    codes: &'static [(&'static str, &'static str)],
    examples: Examples,
) -> StandardType {
    StandardType {
        name,
        holds: Holds::Enum(codes),
        text: Some(TextForm::Code),
        json: Json::Text,
        storage: Storage::Column(SqlType::Text),
        format: CANONICAL,
        ordered: false,
        validator: None,
        shape: None,
        examples,
        release: FIRST_RELEASE,
    }
}

// ---------------------------------------------------------------------
// Barcodes and quantities
// ---------------------------------------------------------------------

/// Code 128's data (ISO/IEC 15417) as Jadpo takes it: 1 to 80 printable
/// ASCII characters, its code sets' text without their control characters.
pub const CODE_128_PATTERN: &str = r"[\x20-\x7E]{1,80}";

/// A QR code's data (ISO/IEC 18004) in byte mode, whose characters are ISO
/// 8859-1's one byte each when no ECI names another set: 1 to 2,953 of its
/// printable characters, tabs and line breaks, the most a version 40 symbol
/// holds at its lowest error correction.
pub const QR_CODE_PATTERN: &str = r"[\t\n\r\x20-\x7E\xA0-\xFF]{1,2953}";

/// `Barcode`'s variants, by symbology: the retail codes GS1 checks (ISO/IEC
/// 15420), Code 128 (ISO/IEC 15417), on most shipping labels, and the QR
/// code (ISO/IEC 18004).
pub const BARCODE_VARIANTS: &[(&str, &[(&str, Part)])] = &[
    ("ean13", &[("data", Part::Standard("Ean13"))]),
    ("upc_a", &[("data", Part::Standard("UpcA"))]),
    ("code128", &[("data", Part::Matching(CODE_128_PATTERN))]),
    ("qr_code", &[("data", Part::Matching(QR_CODE_PATTERN))]),
];

/// What a barcode encodes, by its symbology: a choice, sent tagged, stored
/// as that JSON and shown as its data.
pub const BARCODE_ENTRY: StandardType = StandardType {
    name: BARCODE,
    holds: Holds::Choice(BARCODE_VARIANTS),
    text: None,
    json: Json::Tagged,
    storage: Storage::Column(SqlType::Json),
    format: Format {
        key: FormatKey::Parts {
            parts: &["data"],
            separator: "",
        },
        options: NO_OPTIONS,
    },
    ordered: false,
    validator: None,
    shape: None,
    examples: examples(
        &[
            (r#"{"tag":"ean13","data":"4006381333931"}"#, "4006381333931"),
            (r#"{"tag":"upc_a","data":"036000291452"}"#, "036000291452"),
            (r#"{"tag":"code128","data":"CTN-0042"}"#, "CTN-0042"),
            (
                r#"{"tag":"qr_code","data":"https://example.com/a1"}"#,
                "https://example.com/a1",
            ),
        ],
        &[
            // A GTIN's check digit, a character Code 128 has no code for,
            // one a QR code's byte mode has none for, nothing at all.
            r#"{"tag":"ean13","data":"4006381333932"}"#,
            r#"{"tag":"code128","data":"Crème"}"#,
            r#"{"tag":"qr_code","data":"🍰"}"#,
            r#"{"tag":"code128","data":""}"#,
            // A symbology it doesn't name, no data, data as a number, no
            // tag.
            r#"{"tag":"code39","data":"CTN"}"#,
            r#"{"tag":"ean13"}"#,
            r#"{"tag":"ean13","data":4006381333931}"#,
            r#""4006381333931""#,
        ],
    ),
    release: FIRST_RELEASE,
};

/// `UnitQuantity`'s variants: a count of items (UN/ECE Recommendation 20's
/// `EA`, each), or an amount by weight, volume, length or area, the kinds
/// the UK's Price Marking Order 2004 sells goods by, as `UnitPrice` prices
/// them.
pub const UNIT_QUANTITY_VARIANTS: &[(&str, &[(&str, Part)])] = &[
    ("each", &[("count", Part::Whole(0, MOST_COUNTED))]),
    ("mass", &[("amount", never_negative(QuantityKind::Mass))]),
    (
        "volume",
        &[("amount", never_negative(QuantityKind::Volume))],
    ),
    (
        "length",
        &[("amount", never_negative(QuantityKind::Length))],
    ),
    ("area", &[("amount", never_negative(QuantityKind::Area))]),
];

/// How much of something an order line or a stock count holds: a count or
/// an amount of a quantity, never below 0, sent tagged, stored as that JSON
/// and shown as its count or amount.
pub const UNIT_QUANTITY_ENTRY: StandardType = StandardType {
    name: UNIT_QUANTITY,
    holds: Holds::Choice(UNIT_QUANTITY_VARIANTS),
    text: None,
    json: Json::Tagged,
    storage: Storage::Column(SqlType::Json),
    format: Format {
        key: FormatKey::Parts {
            parts: &["count", "amount"],
            separator: "",
        },
        options: NO_OPTIONS,
    },
    ordered: false,
    validator: None,
    shape: None,
    examples: examples(
        &[
            (r#"{"tag":"each","count":3}"#, "3"),
            (r#"{"tag":"each","count":1200}"#, "1,200"),
            (
                r#"{"tag":"mass","amount":{"value":"2.5","unit":"kg"}}"#,
                "2.5 kg",
            ),
            (
                r#"{"tag":"volume","amount":{"value":"0.0015","unit":"cubic_metre"}}"#,
                "0.0015 m³",
            ),
            (
                r#"{"tag":"length","amount":{"value":"12","unit":"metre"}}"#,
                "12 m",
            ),
            (
                r#"{"tag":"area","amount":{"value":"20","unit":"sqm"}}"#,
                "20 m²",
            ),
        ],
        &[
            // A count or an amount below 0, an amount of another kind, a
            // count that isn't whole, a variant it hasn't, no count.
            r#"{"tag":"each","count":-1}"#,
            r#"{"tag":"mass","amount":{"value":"-1","unit":"kg"}}"#,
            r#"{"tag":"mass","amount":{"value":"1","unit":"metre"}}"#,
            r#"{"tag":"each","count":2.5}"#,
            r#"{"tag":"dozen","count":1}"#,
            r#"{"tag":"each"}"#,
        ],
    ),
    release: FIRST_RELEASE,
};

// ---------------------------------------------------------------------
// Ratings and sizes
// ---------------------------------------------------------------------

/// `Rating`'s parts, in schema.org's order: its value, the best and the
/// worst a rating on its scale can be.
pub const RATING_FIELDS: [(&str, Part); 3] = [
    ("value", Part::Of(Builtin::Decimal)),
    ("best", Part::Of(Builtin::Decimal)),
    ("worst", Part::Of(Builtin::Decimal)),
];

/// `Rating`'s rules across its parts (chunk 5.5t2): its scale runs from a
/// worst below its best, and its value is on it.
pub const RATING_RULES: &[StandardRule] = &[
    StandardRule {
        name: "best_above_worst",
        written: "best > worst",
        label: "not above the worst",
        reason: "A `Rating`'s scale runs from its `worst` up to its `best`, so the best is above the worst (schema.org's `Rating`).",
        next: "Write a `best` above the `worst`",
        check: in_order(&[Order::below("worst", "best")], "best"),
    },
    StandardRule {
        name: "value_on_scale",
        written: "worst <= value and value <= best",
        label: "off its scale",
        reason: "A `Rating`'s `value` is on its scale, from its `worst` to its `best` (schema.org's `Rating`).",
        next: "Write a value from the `worst` to the `best`",
        check: in_order(
            &[Order::at_most("worst", "value"), Order::at_most("value", "best")],
            "value",
        ),
    },
];

/// A rating on its own scale (schema.org's `Rating`), as written: 4.5 of 5
/// and 9 of 10 are different values. Sent as its fields, stored as that
/// JSON and shown as its value out of its best, `4.5/5`.
pub const RATING_ENTRY: StandardType = StandardType {
    name: RATING,
    holds: Holds::Object(&RATING_FIELDS),
    text: None,
    json: Json::Fields(&["value", "best", "worst"]),
    storage: Storage::Column(SqlType::Json),
    format: Format {
        key: FormatKey::Parts {
            parts: &["value", "best"],
            separator: "/",
        },
        options: NO_OPTIONS,
    },
    ordered: false,
    validator: None,
    shape: None,
    examples: examples(
        &[
            (r#"{"value":"4.5","best":"5","worst":"1"}"#, "4.5/5"),
            (r#"{"value":"8","best":"10","worst":"0"}"#, "8/10"),
            (r#"{"value":"92","best":"100","worst":"0"}"#, "92/100"),
            (r#"{"value":"1","best":"5","worst":"1"}"#, "1/5"),
        ],
        &[
            // Above its best, below its worst, a scale whose best isn't
            // above its worst.
            r#"{"value":"6","best":"5","worst":"1"}"#,
            r#"{"value":"0","best":"5","worst":"1"}"#,
            r#"{"value":"1","best":"1","worst":"1"}"#,
            // No worst, a value that isn't a number, a rating as text.
            r#"{"value":"4.5","best":"5"}"#,
            r#"{"value":"good","best":"5","worst":"1"}"#,
            r#""4.5/5""#,
        ],
    ),
    release: FIRST_RELEASE,
};

/// `ClothingSizeSystem`'s variants, by the code a label writes: the UK's,
/// continental Europe's and the US's sizes.
pub const CLOTHING_SIZE_SYSTEMS: &[(&str, &str)] = &[("uk", "UK"), ("eu", "EU"), ("us", "US")];

/// `ShoeSizeSystem`'s variants, by the code a box writes: the UK's,
/// continental Europe's Paris points and the US's sizes.
pub const SHOE_SIZE_SYSTEMS: &[(&str, &str)] = &[("uk", "UK"), ("eu", "EU"), ("us", "US")];

/// A clothing size as a label writes it in its system: a whole number up
/// to three digits, the US's `00`, or a letter size from `XXS` to `5XL`.
pub const CLOTHING_SIZE_PATTERN: &str = "00|0|[1-9][0-9]{0,2}|XXS|XS|S|M|L|XL|XXL|[3-5]XL";

/// A shoe size as a box writes it in its system: a whole size or a half
/// size, `7`, `7.5`.
pub const SHOE_SIZE_PATTERN: &str = r"(?:0|[1-9][0-9]?)(?:\.5)?";

/// The system a clothing size is in.
pub const CLOTHING_SIZE_SYSTEM_ENTRY: StandardType = system(
    CLOTHING_SIZE_SYSTEM,
    CLOTHING_SIZE_SYSTEMS,
    examples(
        &[("UK", "UK"), ("EU", "EU"), ("US", "US")],
        &["FR", "Europe", "UK ", ""],
    ),
);

/// The system a shoe size is in.
pub const SHOE_SIZE_SYSTEM_ENTRY: StandardType = system(
    SHOE_SIZE_SYSTEM,
    SHOE_SIZE_SYSTEMS,
    examples(
        &[("UK", "UK"), ("EU", "EU"), ("US", "US")],
        &["JP", "Mondopoint", "US-M", ""],
    ),
);

/// A clothing size in its system, written as a label writes it, `UK 10`,
/// `US M`: the size is a label, not a measurement, and whose scale it is
/// on (women's, men's or children's) is the garment's.
pub const CLOTHING_SIZE_ENTRY: StandardType = StandardType {
    name: CLOTHING_SIZE,
    holds: Holds::Object(&[
        ("system", Part::Standard(CLOTHING_SIZE_SYSTEM)),
        ("size", Part::Matching(CLOTHING_SIZE_PATTERN)),
    ]),
    text: Some(TextForm::CodeThenText),
    json: Json::Text,
    storage: Storage::Columns {
        parts: &[("system", "text"), ("size", "text")],
        fixed: None,
    },
    format: CANONICAL,
    ordered: false,
    validator: None,
    shape: None,
    examples: examples(
        &[
            ("UK 10", "UK 10"),
            ("EU 38", "EU 38"),
            ("US 00", "US 00"),
            ("US M", "US M"),
            ("UK 3XL", "UK 3XL"),
        ],
        &[
            "UK10", "UK  10", "UK m", "UK 010", "UK XXXL", "FR 38", "10", "",
        ],
    ),
    release: FIRST_RELEASE,
};

/// A shoe size in its system, written as a box writes it, `UK 7.5`, `EU
/// 42`: a label, whose scale (men's, women's or children's) is the shoe's.
pub const SHOE_SIZE_ENTRY: StandardType = StandardType {
    name: SHOE_SIZE,
    holds: Holds::Object(&[
        ("system", Part::Standard(SHOE_SIZE_SYSTEM)),
        ("size", Part::Matching(SHOE_SIZE_PATTERN)),
    ]),
    text: Some(TextForm::CodeThenText),
    json: Json::Text,
    storage: Storage::Columns {
        parts: &[("system", "text"), ("size", "text")],
        fixed: None,
    },
    format: CANONICAL,
    ordered: false,
    validator: None,
    shape: None,
    examples: examples(
        &[
            ("UK 7.5", "UK 7.5"),
            ("EU 42", "EU 42"),
            ("US 9", "US 9"),
            ("EU 38.5", "EU 38.5"),
        ],
        &["UK 7,5", "UK 7.25", "UK 07", "JP 26", "EU42", "US 9.0", ""],
    ),
    release: FIRST_RELEASE,
};

// ---------------------------------------------------------------------
// Parcels, vehicles and flights
// ---------------------------------------------------------------------

/// The unit a parcel's dimensions are shown in unless `unit:` names
/// another: centimetres, as a carrier's label gives them.
const CENTIMETRES: &[ShownIn] = &[ShownIn {
    kind: QuantityKind::Length,
    unit: "centimetre",
    places: None,
}];

/// `Dimensions`' parts, in the order a carrier writes them: length, width,
/// height, none below 0.
pub const DIMENSIONS_FIELDS: [(&str, Part); 3] = [
    ("length", never_negative(QuantityKind::Length)),
    ("width", never_negative(QuantityKind::Length)),
    ("height", never_negative(QuantityKind::Length)),
];

/// A box's length, width and height, sent as named fields so nothing reads
/// them in another order, stored as that JSON and shown in centimetres, or
/// the unit `unit:` names, written once: `30 × 20 × 10 cm`.
pub const DIMENSIONS_ENTRY: StandardType = StandardType {
    name: DIMENSIONS,
    holds: Holds::Object(&DIMENSIONS_FIELDS),
    text: None,
    json: Json::Fields(&["length", "width", "height"]),
    storage: Storage::Column(SqlType::Json),
    format: Format {
        key: FormatKey::Measures {
            parts: &["length", "width", "height"],
            separator: " × ",
            units: CENTIMETRES,
            unit_once: true,
        },
        options: &[FormatOption {
            name: "unit",
            needed: false,
        }],
    },
    ordered: false,
    validator: None,
    shape: None,
    examples: examples(
        &[
            (
                r#"{"length":{"value":"0.3","unit":"metre"},"width":{"value":"0.2","unit":"metre"},"height":{"value":"0.1","unit":"metre"}}"#,
                "30 × 20 × 10 cm",
            ),
            (
                r#"{"length":{"value":"1.2","unit":"metre"},"width":{"value":"0.8","unit":"metre"},"height":{"value":"0.144","unit":"metre"}}"#,
                "120 × 80 × 14.4 cm",
            ),
            (
                r#"{"length":{"value":"12","unit":"inch"},"width":{"value":"9","unit":"inch"},"height":{"value":"4","unit":"inch"}}"#,
                "30.48 × 22.86 × 10.16 cm",
            ),
        ],
        &[
            // A side below 0, a side that isn't a length, a side left out,
            // the sides as text.
            r#"{"length":{"value":"-0.3","unit":"metre"},"width":{"value":"0.2","unit":"metre"},"height":{"value":"0.1","unit":"metre"}}"#,
            r#"{"length":{"value":"0.3","unit":"kg"},"width":{"value":"0.2","unit":"metre"},"height":{"value":"0.1","unit":"metre"}}"#,
            r#"{"length":{"value":"0.3","unit":"metre"},"width":{"value":"0.2","unit":"metre"}}"#,
            r#""30 x 20 x 10 cm""#,
        ],
    ),
    release: FIRST_RELEASE,
};

/// A tracking number as Jadpo takes it: 6 to 40 capitals and digits, which
/// covers the forms the large carriers publish (UPS's `1Z` and 16, FedEx's
/// 12 to 34 digits, USPS's 20 to 34, UPU S10's 13); a carrier's own form
/// and check digit are its own.
pub const TRACKING_NUMBER_PATTERN: &str = "[0-9A-Z]{6,40}";

/// A parcel's tracking number with the carrier that issued it, since a
/// number alone may be two carriers', written as the carrier's code, a
/// space and the number: `UPSN 1Z999AA10123456784`.
pub const TRACKING_NUMBER_ENTRY: StandardType = StandardType {
    name: TRACKING_NUMBER,
    holds: Holds::Object(&[
        ("carrier", Part::Standard("Carrier")),
        ("number", Part::Matching(TRACKING_NUMBER_PATTERN)),
    ]),
    text: Some(TextForm::Joined(" ")),
    json: Json::Text,
    storage: Storage::Columns {
        parts: &[("carrier", "text"), ("number", "text")],
        fixed: None,
    },
    format: CANONICAL,
    ordered: false,
    validator: None,
    shape: None,
    examples: examples(
        &[
            ("UPSN 1Z999AA10123456784", "UPSN 1Z999AA10123456784"),
            ("FDXE 449044304137821", "FDXE 449044304137821"),
            ("USPS 9400111899223197428490", "USPS 9400111899223197428490"),
        ],
        &[
            "upsn 1Z999AA10123456784",
            "UPSN 1z999aa10123456784",
            "UPSN  1Z999AA10123456784",
            "UPSN 12345",
            "UP5N 1Z999AA10123456784",
            "1Z999AA10123456784",
            "UPSN",
            "",
        ],
    ),
    release: FIRST_RELEASE,
};

/// A registration mark as its plate shows it: one to four groups of one
/// to eight capitals, letters of a script with no case, or digits, joined
/// by a space or a hyphen, kept as written since in some countries the
/// separator is part of the mark (a German district's code).
pub const PLATE_PATTERN: &str = r"[\p{Lu}\p{Lo}\p{Nd}]{1,8}(?:[ \-][\p{Lu}\p{Lo}\p{Nd}]{1,8}){0,3}";

/// A vehicle's registration mark with the country that issued it, since
/// one mark may be two countries', written as the country's code, a space
/// and the mark: `GB AB12 CDE`.
pub const VEHICLE_REGISTRATION_ENTRY: StandardType = StandardType {
    name: VEHICLE_REGISTRATION,
    holds: Holds::Object(&[
        ("country", Part::Standard(super::places::COUNTRY)),
        ("plate", Part::Matching(PLATE_PATTERN)),
    ]),
    text: Some(TextForm::CodeThenText),
    json: Json::Text,
    storage: Storage::Columns {
        parts: &[("country", "text"), ("plate", "text")],
        fixed: None,
    },
    format: CANONICAL,
    ordered: false,
    validator: None,
    shape: None,
    examples: examples(
        &[
            ("GB AB12 CDE", "GB AB12 CDE"),
            ("DE M-AB 1234", "DE M-AB 1234"),
            ("FR AA-123-AA", "FR AA-123-AA"),
        ],
        &[
            "GB ab12 cde",
            "GB AB12  CDE",
            "GB AB12-",
            "GB -AB12",
            "XX AB12 CDE",
            "AB12CDE",
            "GB",
            "",
        ],
    ),
    release: FIRST_RELEASE,
};

/// `FlightNumber`'s parts: the airline's IATA designator, the flight's
/// number from 1 to 9999, and the operational suffix IATA's SSIM gives a
/// flight, a capital, when it has one.
pub const FLIGHT_NUMBER_FIELDS: [(&str, Part); 3] = [
    ("airline", Part::Standard("IataAirline")),
    ("number", Part::Whole(1, 9999)),
    ("suffix", Part::Optional(&Part::Matching("[A-Z]"))),
];

/// A flight's designator (IATA's SSIM): its airline's code, its number
/// with no leading zero and any suffix, `BA123`, so `BA0123` and `BA 123`
/// aren't two ways to write it. A flight on a date is an application's
/// record of a `FlightNumber` and a `CalendarDate`.
pub const FLIGHT_NUMBER_ENTRY: StandardType = StandardType {
    name: FLIGHT_NUMBER,
    holds: Holds::Object(&FLIGHT_NUMBER_FIELDS),
    text: Some(TextForm::FlightNumber),
    json: Json::Text,
    storage: Storage::Columns {
        parts: &[
            ("airline", "text"),
            ("number", "integer"),
            ("suffix", "text"),
        ],
        fixed: None,
    },
    format: CANONICAL,
    ordered: false,
    validator: None,
    shape: None,
    examples: examples(
        &[
            ("BA123", "BA123"),
            ("U21234", "U21234"),
            ("6E2001", "6E2001"),
            ("BA5A", "BA5A"),
        ],
        &[
            "BA0123", "BA 123", "ba123", "BA12345", "BA", "BA0", "BAW123", "BA123AB", "",
        ],
    ),
    release: FIRST_RELEASE,
};

// ---------------------------------------------------------------------
// The rest
// ---------------------------------------------------------------------

/// `Jurisdiction`'s variants: a country (ISO 3166-1), or one of its
/// subdivisions (ISO 3166-2), England and not the UK.
pub const JURISDICTION_VARIANTS: &[(&str, &[(&str, Part)])] = &[
    (
        "country",
        &[("country", Part::Standard(super::places::COUNTRY))],
    ),
    (
        "subdivision",
        &[(
            "subdivision",
            Part::Standard(super::places::COUNTRY_SUBDIVISION),
        )],
    ),
];

/// Where a law applies: a country or a subdivision, a choice sent tagged,
/// stored as that JSON and shown by its CLDR name.
pub const JURISDICTION_ENTRY: StandardType = StandardType {
    name: JURISDICTION,
    holds: Holds::Choice(JURISDICTION_VARIANTS),
    text: None,
    json: Json::Tagged,
    storage: Storage::Column(SqlType::Json),
    format: Format {
        key: FormatKey::Parts {
            parts: &["country", "subdivision"],
            separator: "",
        },
        options: NO_OPTIONS,
    },
    ordered: false,
    validator: None,
    shape: None,
    examples: examples(
        &[
            (r#"{"tag":"country","country":"GB"}"#, "United Kingdom"),
            (r#"{"tag":"subdivision","subdivision":"GB-ENG"}"#, "England"),
            (
                r#"{"tag":"subdivision","subdivision":"US-CA"}"#,
                "California",
            ),
        ],
        &[
            r#"{"tag":"country","country":"UK"}"#,
            r#"{"tag":"subdivision","subdivision":"GB"}"#,
            r#"{"tag":"state","state":"CA"}"#,
            r#"{"tag":"country"}"#,
            r#""GB""#,
        ],
    ),
    release: FIRST_RELEASE,
};

/// A model provider's name as Hugging Face writes an organisation: up to
/// 64 letters, digits, `.`, `_` and `-`, starting with a letter or digit.
pub const MODEL_PROVIDER_PATTERN: &str = "[A-Za-z0-9][A-Za-z0-9._-]{0,63}";

/// A model's name as its provider writes it: up to 128 letters, digits,
/// `.`, `_`, `-`, and the `:` and `@` a version tag takes, starting with
/// a letter or digit, with no `/`.
pub const MODEL_NAME_PATTERN: &str = "[A-Za-z0-9][A-Za-z0-9._:@-]{0,127}";

/// An AI model by its provider and name, kept as written, since a model's
/// version is in its name: written as Hugging Face writes a repository,
/// `meta-llama/Llama-3.1-8B-Instruct`.
pub const MODEL_ID_ENTRY: StandardType = StandardType {
    name: MODEL_ID,
    holds: Holds::Object(&[
        ("provider", Part::Matching(MODEL_PROVIDER_PATTERN)),
        ("name", Part::Matching(MODEL_NAME_PATTERN)),
    ]),
    text: Some(TextForm::Joined("/")),
    json: Json::Text,
    storage: Storage::Columns {
        parts: &[("provider", "text"), ("name", "text")],
        fixed: None,
    },
    format: CANONICAL,
    ordered: false,
    validator: None,
    shape: None,
    examples: examples(
        &[
            ("openai/gpt-4o", "openai/gpt-4o"),
            (
                "meta-llama/Llama-3.1-8B-Instruct",
                "meta-llama/Llama-3.1-8B-Instruct",
            ),
            ("ollama/llama3:8b", "ollama/llama3:8b"),
        ],
        &[
            "gpt-4o",
            "openai/",
            "/gpt-4o",
            "openai/gpt/4o",
            "openai /gpt-4o",
            "-openai/gpt-4o",
            "",
        ],
    ),
    release: FIRST_RELEASE,
};

