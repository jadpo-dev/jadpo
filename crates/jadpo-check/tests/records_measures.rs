//! Area 19, the records and measures of commerce, health, travel and the
//! rest (chunk 5.5m2), in the checker: `form: CodeForm.isbn10` read but
//! never written; a rating's and a blood pressure's rules across their
//! parts, and the other records' parts, checked while compiling where a
//! literal writes them; `text.format`'s `unit:` a unit of the kind a
//! measure is shown in; the clinical units written `120mmHg` and `72bpm`,
//! or by `units.of` for the concentrations, which have no suffix; and an
//! application's own type refused under a new standard name.

use crate::support;

use jadpo_db::Layout;

const NAMES: &str = "type Wire = Text {}\n\ntype Unreadable = Failure {\n    kind: FailureKind.invalid_value\n}\n\n";

fn rules(source: &str) -> Vec<String> {
    support::rules(
        Layout::Loose,
        &[("main.jadpo", &format!("{NAMES}{source}"))],
    )
}

const NONE: &[&str] = &[];

#[test]
fn an_isbn_10_is_read_and_never_written() {
    assert_eq!(
        rules(
            "function book(raw: Wire) fails Unreadable -> Isbn {\n    return attempt text.parse(raw, as: Isbn, form: CodeForm.isbn10) {\n        malformed: Unreadable\n    }\n}\n"
        ),
        NONE
    );
    assert_eq!(
        rules(
            "function old(book: Isbn) -> Wire {\n    return Wire(text.from(book, form: CodeForm.isbn10))\n}\n"
        ),
        ["type.mismatch"],
        "an ISBN is text already, written as it is"
    );
    assert_eq!(
        rules(
            "function code(made_in: Country) -> Wire {\n    return Wire(text.from(made_in, form: CodeForm.isbn10))\n}\n"
        ),
        ["type.temporal_option_unused"],
        "no other type is written as an ISBN-10"
    );
    assert_eq!(
        rules(
            "function made_in(raw: Wire) fails Unreadable -> Country {\n    return attempt text.parse(raw, as: Country, form: CodeForm.isbn10) {\n        malformed: Unreadable\n    }\n}\n"
        ),
        ["type.temporal_option_unused"],
        "a country has no ISBN-10"
    );
}

#[test]
fn rules_across_parts_are_proved_on_literals() {
    let rating = |value: &str, best: &str, worst: &str| {
        rules(&format!(
            "function stars() -> Rating {{\n    return Rating {{\n        value: {value}\n        best: {best}\n        worst: {worst}\n    }}\n}}\n"
        ))
    };
    assert_eq!(rating("4.5", "5.0", "1.0"), NONE);
    assert_eq!(
        rating("1.0", "5.0", "1.0"),
        NONE,
        "the worst is on the scale"
    );
    assert_eq!(
        rating("6.0", "5.0", "1.0"),
        ["type.invalid_literal"],
        "above the best"
    );
    assert_eq!(
        rating("0.5", "5.0", "1.0"),
        ["type.invalid_literal"],
        "below the worst"
    );
    assert_eq!(
        rating("1.0", "1.0", "1.0"),
        ["type.invalid_literal"],
        "a scale of one point"
    );
    let pressure = |systolic: &str, diastolic: &str| {
        rules(&format!(
            "function reading() -> BloodPressure {{\n    return BloodPressure {{\n        systolic: {systolic}\n        diastolic: {diastolic}\n    }}\n}}\n"
        ))
    };
    assert_eq!(pressure("16kPa", "10.7kPa"), NONE);
    assert_eq!(
        pressure("10.7kPa", "16kPa"),
        ["type.invalid_literal"],
        "the pair the other way round"
    );
    assert_eq!(
        pressure("12kPa", "12kPa"),
        ["type.invalid_literal"],
        "the two the same"
    );
    assert_eq!(
        pressure("16kPa", "-1kPa"),
        ["type.invalid_literal"],
        "a pressure below 0"
    );
    assert_eq!(
        pressure("16kg", "10.7kPa"),
        ["type.mismatch"],
        "a mass isn't a pressure"
    );
}

#[test]
fn other_parts_are_checked_by_their_standards() {
    assert_eq!(
        rules(
            "function size() -> ClothingSize {\n    return ClothingSize {\n        system: ClothingSizeSystem.uk\n        size: \"XXXL\"\n    }\n}\n"
        ),
        ["type.invalid_literal"],
        "a letter size past the pattern"
    );
    assert_eq!(
        rules(
            "function flight() -> FlightNumber {\n    return FlightNumber {\n        airline: \"BA\"\n        number: 0\n        suffix: none\n    }\n}\n"
        ),
        ["type.invalid_literal"],
        "a flight's number from 1"
    );
    assert_eq!(
        rules(
            "function scanned() -> Barcode {\n    return Barcode.ean13 {\n        data: \"4006381333932\"\n    }\n}\n"
        ),
        ["type.invalid_literal"],
        "a GTIN's check digit"
    );
    assert_eq!(
        rules(
            "function twice() -> DosageInstruction {\n    return DosageInstruction {\n        dose: Dose.mass {\n            amount: 500mg\n        }\n        route: RouteOfAdministration.oral\n        times: 0\n        per: 24h\n        as_needed: false\n    }\n}\n"
        ),
        ["type.invalid_literal"],
        "never isn't a frequency"
    );
}

#[test]
fn a_measure_s_unit_is_of_its_kind() {
    assert_eq!(
        rules(
            "function a(parcel: Dimensions) -> PresentationText {\n    return text.format(parcel, locale: Locale.en_gb, unit: LengthUnit.inch)\n}\n\nfunction b(pulse: HeartRate) -> PresentationText {\n    return text.format(pulse, locale: Locale.en_gb, unit: FrequencyUnit.hertz)\n}\n\nfunction c(pressure: BloodPressure) -> PresentationText {\n    return text.format(pressure, locale: Locale.en_gb, unit: PressureUnit.kilopascal)\n}\n\nfunction d(glucose: BloodGlucose) -> PresentationText {\n    return text.format(glucose, locale: Locale.en_gb, unit: ConcentrationUnit.milligram_per_decilitre)\n}\n"
        ),
        NONE
    );
    assert_eq!(
        rules(
            "function shown(parcel: Dimensions) -> PresentationText {\n    return text.format(parcel, locale: Locale.en_gb, unit: MassUnit.kilogram)\n}\n"
        ),
        ["type.mismatch"],
        "a parcel's sides are lengths"
    );
    assert_eq!(
        rules(
            "function shown(pulse: HeartRate) -> PresentationText {\n    return text.format(pulse, locale: Locale.en_gb, unit: PressureUnit.kilopascal)\n}\n"
        ),
        ["type.mismatch"],
        "a heart rate is a frequency"
    );
    assert_eq!(
        rules(
            "function shown(amount: Dose) -> PresentationText {\n    return text.format(amount, locale: Locale.en_gb, unit: MassUnit.gram)\n}\n"
        ),
        ["type.temporal_option_unused"],
        "a dose is shown in milligrams or millilitres, by its kind"
    );
}

#[test]
fn the_clinical_units_are_written_by_suffix_or_by_units_of() {
    assert_eq!(
        rules(
            "type Pulse = Frequency {}\n\ntype Top = Pressure {}\n\ntype Fasting = Concentration {}\n\nfunction pulse() -> Pulse {\n    return Pulse(72bpm)\n}\n\nfunction top() -> Top {\n    return Top(120mmHg)\n}\n\nfunction fasting() -> Fasting {\n    return Fasting(units.of(5.5, ConcentrationUnit.millimole_of_glucose_per_litre))\n}\n"
        ),
        NONE
    );
    assert_eq!(
        rules("type Top = Pressure {}\n\nfunction top() -> Top {\n    return Top(120mmhg)\n}\n"),
        ["type.unit_unknown"],
        "a suffix is matched by case"
    );
    assert_eq!(
        rules(
            "type Fasting = Concentration {}\n\nfunction fasting() -> Fasting {\n    return Fasting(99mgdL)\n}\n"
        ),
        ["type.unit_unknown"],
        "a concentration's units have no suffix"
    );
    assert_eq!(
        rules(
            "function reading() -> BloodPressure {\n    return BloodPressure {\n        systolic: 80mmHg\n        diastolic: 120mmHg\n    }\n}\n"
        ),
        ["type.invalid_literal"],
        "a reading written with the suffix is proved"
    );
}

#[test]
fn an_application_s_own_type_is_refused_under_a_standard_name() {
    assert_eq!(
        rules("type Rating = Object {\n    stars: Int\n}\n"),
        ["type.standard_clash"]
    );
    assert_eq!(rules("type BoxSize = Object {\n    stars: Int\n}\n"), NONE);
}
