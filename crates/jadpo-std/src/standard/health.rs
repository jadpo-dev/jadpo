//! The health records and measures (area 19, "The standard types", chunk
//! 5.5m2): a medicine's `Dose` by mass or volume and how it is taken, a
//! `DosageInstruction`, with `RouteOfAdministration`, the routes it is
//! taken by; and a person's `HeartRate`, `BloodPressure` and
//! `BloodGlucose`, each on a physical quantity (`Frequency`, `Pressure`,
//! `Concentration`) and shown in the unit a clinician reads. The measures
//! are a person's health (area 15); a dose and an instruction describe a
//! medicine, as a leaflet gives them, and a patient's prescription is an
//! application's own record, which it marks, as a person's allergy is
//! (chunk 5.5m). What the page left open is area 19's "Chosen in chunk
//! 5.5m2".

use super::across::{Order, StandardRule, in_order};
use super::{
    Examples, FIRST_RELEASE, Format, FormatKey, FormatOption, Holds, Json, NO_OPTIONS, Part,
    QUANTITY_FIELDS, ShownIn, StandardType, Storage, TextForm,
};
use crate::read::SqlType;
use crate::{Builtin, Num, QuantityKind};

/// `Dose`.
pub const DOSE: &str = "Dose";
/// `DosageInstruction`.
pub const DOSAGE_INSTRUCTION: &str = "DosageInstruction";
/// `RouteOfAdministration`: how a `DosageInstruction`'s dose is taken.
pub const ROUTE_OF_ADMINISTRATION: &str = "RouteOfAdministration";
/// `HeartRate`.
pub const HEART_RATE: &str = "HeartRate";
/// `BloodPressure`.
pub const BLOOD_PRESSURE: &str = "BloodPressure";
/// `BloodGlucose`.
pub const BLOOD_GLUCOSE: &str = "BloodGlucose";

/// The measures that are a person's health (area 15).
pub const PERSONAL: &[(&str, &str)] = &[
    (BLOOD_GLUCOSE, "health"),
    (BLOOD_PRESSURE, "health"),
    (HEART_RATE, "health"),
];

const ZERO: Num = Num {
    mantissa: 0,
    scale: 0,
};

/// The smallest step of a `Mass` or a `Volume`, each held to 9 places: a
/// microgram, a microlitre. A dose is at least that, so never nothing.
const SMALLEST_STEP: Num = Num {
    mantissa: 1,
    scale: 9,
};

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

const UNIT_OPTION: &[FormatOption] = &[FormatOption {
    name: "unit",
    needed: false,
}];

// ---------------------------------------------------------------------
// Medicines
// ---------------------------------------------------------------------

/// An amount of `kind` above nothing.
const fn some(kind: QuantityKind) -> Part {
    Part::Bounded {
        of: Builtin::Quantity(kind),
        min: Some(SMALLEST_STEP),
        max: None,
    }
}

/// `Dose`'s variants: an amount by mass or by volume.
pub const DOSE_VARIANTS: &[(&str, &[(&str, Part)])] = &[
    ("mass", &[("amount", some(QuantityKind::Mass))]),
    ("volume", &[("amount", some(QuantityKind::Volume))]),
];

/// The units a dose is shown in: a mass in milligrams, so a microgram dose
/// is never shown in a unit a reader can take for a milligram, and a
/// volume in millilitres, each exactly, never rounded.
const DOSE_UNITS: &[ShownIn] = &[
    ShownIn {
        kind: QuantityKind::Mass,
        unit: "milligram",
        places: None,
    },
    ShownIn {
        kind: QuantityKind::Volume,
        unit: "millilitre",
        places: None,
    },
];

/// An amount of a medicine, by mass or by volume, above nothing: a choice
/// sent tagged, stored as that JSON and shown in milligrams or millilitres,
/// `500 mg`, `0.125 mg`, `5 ml`.
pub const DOSE_ENTRY: StandardType = StandardType {
    name: DOSE,
    holds: Holds::Choice(DOSE_VARIANTS),
    text: None,
    json: Json::Tagged,
    storage: Storage::Columns {
        parts: &[("kind", "text"), ("amount", "numeric(38, 9)")],
        fixed: &[],
    },
    format: Format {
        key: FormatKey::Measures {
            parts: &["amount"],
            separator: "",
            units: DOSE_UNITS,
            unit_once: false,
        },
        options: NO_OPTIONS,
    },
    ordered: false,
    validator: None,
    shape: None,
    examples: examples(
        &[
            (
                r#"{"tag":"mass","amount":{"value":"500","unit":"mg"}}"#,
                "500 mg",
            ),
            (
                r#"{"tag":"mass","amount":{"value":"125","unit":"mcg"}}"#,
                "0.125 mg",
            ),
            (
                r#"{"tag":"mass","amount":{"value":"1","unit":"g"}}"#,
                "1,000 mg",
            ),
            (
                r#"{"tag":"volume","amount":{"value":"5","unit":"ml"}}"#,
                "5 ml",
            ),
        ],
        &[
            // Nothing, less than nothing, an amount of another kind, a
            // form it isn't, no amount.
            r#"{"tag":"mass","amount":{"value":"0","unit":"mg"}}"#,
            r#"{"tag":"mass","amount":{"value":"-5","unit":"mg"}}"#,
            r#"{"tag":"mass","amount":{"value":"5","unit":"metre"}}"#,
            r#"{"tag":"tablets","count":2}"#,
            r#"{"tag":"volume"}"#,
            r#""500 mg""#,
        ],
    ),
    release: FIRST_RELEASE,
    signals: crate::standard::NO_SIGNALS,
};

/// `RouteOfAdministration`'s variants, each its own code, in order: the
/// routes of the European Directorate for the Quality of Medicines'
/// Standard Terms a medicine's leaflet most often names.
pub const ROUTES: &[(&str, &str)] = &[
    ("auricular", "auricular"),
    ("buccal", "buccal"),
    ("cutaneous", "cutaneous"),
    ("inhalation", "inhalation"),
    ("intramuscular", "intramuscular"),
    ("intravenous", "intravenous"),
    ("nasal", "nasal"),
    ("ocular", "ocular"),
    ("oral", "oral"),
    ("rectal", "rectal"),
    ("subcutaneous", "subcutaneous"),
    ("sublingual", "sublingual"),
    ("transdermal", "transdermal"),
    ("vaginal", "vaginal"),
];

/// Each route as EDQM's Standard Terms word it, in `ROUTES`' order.
pub const ROUTE_NAMES: &[&str] = &[
    "Auricular use",
    "Buccal use",
    "Cutaneous use",
    "Inhalation use",
    "Intramuscular use",
    "Intravenous use",
    "Nasal use",
    "Ocular use",
    "Oral use",
    "Rectal use",
    "Subcutaneous use",
    "Sublingual use",
    "Transdermal use",
    "Vaginal use",
];

/// How a medicine is taken: written as its own name, `oral`, and shown as
/// EDQM words it, "Oral use", in every locale, since CLDR names none.
pub const ROUTE_OF_ADMINISTRATION_ENTRY: StandardType = StandardType {
    name: ROUTE_OF_ADMINISTRATION,
    holds: Holds::Enum(ROUTES),
    text: Some(TextForm::Code),
    json: Json::Text,
    storage: Storage::Column(SqlType::Text),
    format: Format {
        key: FormatKey::Names(ROUTE_NAMES),
        options: NO_OPTIONS,
    },
    ordered: false,
    validator: None,
    shape: None,
    examples: examples(
        &[
            ("oral", "Oral use"),
            ("intravenous", "Intravenous use"),
            ("subcutaneous", "Subcutaneous use"),
        ],
        &["by mouth", "IV", "Oral use", "po", ""],
    ),
    release: FIRST_RELEASE,
    signals: crate::standard::NO_SIGNALS,
};

/// `DosageInstruction`'s parts, in order: the dose, the route it is taken
/// by, how many times in what period (FHIR's `Timing`: a frequency of at
/// least once, in a period above nothing), and whether only when needed.
pub const DOSAGE_INSTRUCTION_FIELDS: [(&str, Part); 5] = [
    ("dose", Part::Standard(DOSE)),
    ("route", Part::Standard(ROUTE_OF_ADMINISTRATION)),
    ("times", Part::Whole(1, 2_147_483_647)),
    (
        "per",
        Part::Bounded {
            of: Builtin::Duration,
            min: Some(Num {
                mantissa: 1,
                scale: 0,
            }),
            max: None,
        },
    ),
    ("as_needed", Part::Of(Builtin::Bool)),
];

/// How to take a medicine, structured as FHIR's `Dosage` structures its
/// commonest form: a dose, a route, a number of times in a period and
/// whether only when needed, never free text to read. Sent as its fields,
/// stored as that JSON and shown in English, as its route's name is:
/// `500 mg, Oral use, 2 times per 1 day`.
pub const DOSAGE_INSTRUCTION_ENTRY: StandardType = StandardType {
    name: DOSAGE_INSTRUCTION,
    holds: Holds::Object(&DOSAGE_INSTRUCTION_FIELDS),
    text: None,
    json: Json::Fields(&["dose", "route", "times", "per", "as_needed"]),
    storage: Storage::Columns {
        parts: &[
            ("dose_kind", "text"),
            ("dose_amount", "numeric(38, 9)"),
            ("route", "text"),
            ("times", "integer"),
            ("per", "bigint"),
            ("as_needed", "boolean"),
        ],
        fixed: &[],
    },
    format: Format {
        key: FormatKey::Dosage,
        options: NO_OPTIONS,
    },
    ordered: false,
    validator: None,
    shape: None,
    examples: examples(
        &[
            (
                r#"{"dose":{"tag":"mass","amount":{"value":"500","unit":"mg"}},"route":"oral","times":2,"per":"PT24H","as_needed":false}"#,
                "500 mg, Oral use, 2 times per 1 day",
            ),
            (
                r#"{"dose":{"tag":"volume","amount":{"value":"10","unit":"ml"}},"route":"oral","times":1,"per":"PT6H","as_needed":true}"#,
                "10 ml, Oral use, 1 time per 6 hours, when needed",
            ),
            (
                r#"{"dose":{"tag":"mass","amount":{"value":"40","unit":"mg"}},"route":"subcutaneous","times":1,"per":"PT168H","as_needed":false}"#,
                "40 mg, Subcutaneous use, 1 time per 7 days",
            ),
        ],
        &[
            // Never, in no time at all, a route EDQM doesn't name, no
            // dose, the instruction as free text.
            r#"{"dose":{"tag":"mass","amount":{"value":"500","unit":"mg"}},"route":"oral","times":0,"per":"PT24H","as_needed":false}"#,
            r#"{"dose":{"tag":"mass","amount":{"value":"500","unit":"mg"}},"route":"oral","times":2,"per":"PT0S","as_needed":false}"#,
            r#"{"dose":{"tag":"mass","amount":{"value":"500","unit":"mg"}},"route":"by mouth","times":2,"per":"PT24H","as_needed":false}"#,
            r#"{"route":"oral","times":2,"per":"PT24H","as_needed":false}"#,
            r#""500 mg twice a day""#,
        ],
    ),
    release: FIRST_RELEASE,
    signals: crate::standard::NO_SIGNALS,
};

// ---------------------------------------------------------------------
// Measures
// ---------------------------------------------------------------------

/// 300 beats a minute, the catalogue's and LOINC 8867-4's top: 5 Hz.
const MOST_BEATS: Num = Num {
    mantissa: 5,
    scale: 0,
};

/// A heart rate's beats a minute, shown whole.
const BEATS: &[ShownIn] = &[ShownIn {
    kind: QuantityKind::Frequency,
    unit: "beat_per_minute",
    places: Some(0),
}];

/// A heart rate (LOINC 8867-4): a `Frequency` from 0 to 300 beats a
/// minute, held in hertz as every frequency is, so its text, JSON and
/// storage are a frequency's, `1.2 Hz`, read in beats a minute too, `72
/// bpm`; shown in whole beats a minute, `72 bpm`, or the unit `unit:`
/// names.
pub const HEART_RATE_ENTRY: StandardType = StandardType {
    name: HEART_RATE,
    holds: Holds::Bounded {
        of: Builtin::Quantity(QuantityKind::Frequency),
        min: Some(ZERO),
        max: Some(MOST_BEATS),
    },
    text: Some(TextForm::Quantity),
    json: Json::Fields(&QUANTITY_FIELDS),
    storage: Storage::Column(SqlType::Numeric),
    format: Format {
        key: FormatKey::UnitsIn(BEATS),
        options: UNIT_OPTION,
    },
    ordered: true,
    validator: None,
    shape: None,
    examples: examples(
        &[
            ("1 Hz", "60 bpm"),
            ("1.2 Hz", "72 bpm"),
            ("1.217 Hz", "73 bpm"),
            ("2.5 Hz", "150 bpm"),
        ],
        &["72", "bpm", "", "seventy-two", "1.2.3 Hz"],
    ),
    release: FIRST_RELEASE,
    signals: crate::standard::NO_SIGNALS,
};

/// A blood pressure in millimetres of mercury, shown whole.
const MILLIMETRES_OF_MERCURY: &[ShownIn] = &[ShownIn {
    kind: QuantityKind::Pressure,
    unit: "millimetre_of_mercury",
    places: Some(0),
}];

/// `BloodPressure`'s parts, in the order a reading writes them: the
/// pressure as the heart beats and between beats, neither below 0.
pub const BLOOD_PRESSURE_FIELDS: [(&str, Part); 2] = [
    (
        "systolic",
        Part::Bounded {
            of: Builtin::Quantity(QuantityKind::Pressure),
            min: Some(ZERO),
            max: None,
        },
    ),
    (
        "diastolic",
        Part::Bounded {
            of: Builtin::Quantity(QuantityKind::Pressure),
            min: Some(ZERO),
            max: None,
        },
    ),
];

/// The rule across a blood pressure's parts: the pressure between beats is
/// below the pressure as the heart beats.
pub const BLOOD_PRESSURE_RULES: &[StandardRule] = &[StandardRule {
    name: "diastolic_below_systolic",
    written: "diastolic < systolic",
    label: "not below the systolic",
    reason: "A `BloodPressure`'s `diastolic`, the pressure between beats, is below its `systolic`, the pressure as the heart beats (LOINC 85354-9).",
    next: "Write a `diastolic` below the `systolic`, or swap the two",
    check: in_order(&[Order::below("diastolic", "systolic")], "diastolic"),
}];

/// A blood pressure (LOINC 85354-9): its systolic and diastolic
/// `Pressure`s, held in pascals as every pressure is, sent as named fields,
/// so nothing reads the pair the other way round, stored as that JSON and
/// shown as a reading writes it, `120/80 mmHg`.
pub const BLOOD_PRESSURE_ENTRY: StandardType = StandardType {
    name: BLOOD_PRESSURE,
    holds: Holds::Object(&BLOOD_PRESSURE_FIELDS),
    text: None,
    json: Json::Fields(&["systolic", "diastolic"]),
    storage: Storage::Columns {
        parts: &[
            ("systolic", "numeric(38, 3)"),
            ("diastolic", "numeric(38, 3)"),
        ],
        fixed: &[],
    },
    format: Format {
        key: FormatKey::Measures {
            parts: &["systolic", "diastolic"],
            separator: "/",
            units: MILLIMETRES_OF_MERCURY,
            unit_once: true,
        },
        options: UNIT_OPTION,
    },
    ordered: false,
    validator: None,
    shape: None,
    examples: examples(
        &[
            (
                r#"{"systolic":{"value":"120","unit":"mmHg"},"diastolic":{"value":"80","unit":"mmHg"}}"#,
                "120/80 mmHg",
            ),
            (
                r#"{"systolic":{"value":"145","unit":"mmHg"},"diastolic":{"value":"92","unit":"mmHg"}}"#,
                "145/92 mmHg",
            ),
            (
                r#"{"systolic":{"value":"16","unit":"kPa"},"diastolic":{"value":"10.7","unit":"kPa"}}"#,
                "120/80 mmHg",
            ),
        ],
        &[
            // The pair the other way round, the two the same, below 0, not
            // a pressure, one left out, a reading as text.
            r#"{"systolic":{"value":"80","unit":"mmHg"},"diastolic":{"value":"120","unit":"mmHg"}}"#,
            r#"{"systolic":{"value":"90","unit":"mmHg"},"diastolic":{"value":"90","unit":"mmHg"}}"#,
            r#"{"systolic":{"value":"120","unit":"mmHg"},"diastolic":{"value":"-80","unit":"mmHg"}}"#,
            r#"{"systolic":{"value":"120","unit":"bpm"},"diastolic":{"value":"80","unit":"mmHg"}}"#,
            r#"{"systolic":{"value":"120","unit":"mmHg"}}"#,
            r#""120/80 mmHg""#,
        ],
    ),
    release: FIRST_RELEASE,
    signals: crate::standard::NO_SIGNALS,
};

/// A blood glucose level in millimoles of glucose a litre, to one place,
/// as a UK or European meter reads it, or in milligrams a decilitre, whole,
/// as a US one does, when `unit:` names that.
const GLUCOSE_UNITS: &[ShownIn] = &[
    ShownIn {
        kind: QuantityKind::Concentration,
        unit: "millimole_of_glucose_per_litre",
        places: Some(1),
    },
    ShownIn {
        kind: QuantityKind::Concentration,
        unit: "milligram_per_decilitre",
        places: Some(0),
    },
];

/// A blood glucose level (LOINC 2339-0 by mass, 15074-8 by substance): a
/// `Concentration` never below 0, held in grams a litre as every
/// concentration is, so its text, JSON and storage are a concentration's,
/// `0.99085635 gL`, read in either clinical unit too; shown in millimoles
/// of glucose a litre to one place, `5.5 mmol/L`, or whole milligrams a
/// decilitre with `unit: ConcentrationUnit.milligram_per_decilitre`, `99
/// mg/dL`.
pub const BLOOD_GLUCOSE_ENTRY: StandardType = StandardType {
    name: BLOOD_GLUCOSE,
    holds: Holds::Bounded {
        of: Builtin::Quantity(QuantityKind::Concentration),
        min: Some(ZERO),
        max: None,
    },
    text: Some(TextForm::Quantity),
    json: Json::Fields(&QUANTITY_FIELDS),
    storage: Storage::Column(SqlType::Numeric),
    format: Format {
        key: FormatKey::UnitsIn(GLUCOSE_UNITS),
        options: UNIT_OPTION,
    },
    ordered: true,
    validator: None,
    shape: None,
    examples: examples(
        &[
            ("0.7206228 gL", "4 mmol/L"),
            ("0.99 gL", "5.5 mmol/L"),
            ("0.99085635 gL", "5.5 mmol/L"),
            ("2.01774384 gL", "11.2 mmol/L"),
        ],
        &["5.5", "mmol/L", "", "high", "5.5.5 gL"],
    ),
    release: FIRST_RELEASE,
    signals: crate::standard::NO_SIGNALS,
};
