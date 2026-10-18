//! Area 19, the records and measures of commerce, health, travel and the
//! rest (chunk 5.5m2), run: the page's examples (`project=records` and
//! `project=health-measures`), an ISBN-10 read as its `Isbn` and a wrong
//! check character refused, a flight read as IATA writes it, a parcel's
//! size and a reading shown in the unit a reader expects, a dosage
//! instruction in words; and each record read from its text or its JSON by
//! its standard, refusing what isn't one, the rules across a rating's and
//! a blood pressure's parts included.

use crate::support;

use jadpo_interp::Value;
use support::{decimal, failure, main, text, value};

const RECORDS: &str = "type ScannedText = Text {}

type NotABook = Failure {
    kind: FailureKind.invalid_value
}

type NotAFlight = Failure {
    kind: FailureKind.invalid_value
}

/// A book the old catalogue lists by its ISBN-10, read as its `Isbn`.
function catalogued_book(raw: ScannedText) fails NotABook -> Isbn {
    var book: Isbn = attempt text.parse(raw, as: Isbn, form: CodeForm.isbn10) {
        malformed: NotABook
    }
    return book
}

/// The flight a booking names, as the airline writes it.
function booked_flight(raw: ScannedText) fails NotAFlight -> FlightNumber {
    var flight: FlightNumber = attempt text.parse(raw, as: FlightNumber) {
        malformed: NotAFlight
    }
    return flight
}

/// A parcel's size as its label prints it.
function parcel_size(parcel: Dimensions, packer: Locale) -> PresentationText {
    return text.format(parcel, locale: packer)
}

/// The same in inches, for a carrier that measures in them.
function parcel_in_inches(parcel: Dimensions, packer: Locale) -> PresentationText {
    return text.format(parcel, locale: packer, unit: LengthUnit.inch)
}

/// A box the warehouse ships most, in inches.
function standard_box() -> Dimensions {
    return Dimensions {
        length: 12inch
        width: 9inch
        height: 4inch
    }
}
";

fn locale(name: &str) -> Value {
    let ordinal = jadpo_std::variant_ordinal(jadpo_std::LOCALE, name).expect("a locale");
    Value::variant(jadpo_std::LOCALE, ordinal, name, Vec::new())
}

#[test]
fn an_isbn_10_reads_as_its_isbn() {
    let program = main(RECORDS).with_proof_checks();
    let book = |raw: &str| value(&program, "catalogued_book", vec![text(raw)]);
    assert_eq!(book("0306406152"), text("9780306406157"));
    assert_eq!(book("080442957X"), text("9780804429573"));
    assert_eq!(book("080442957x"), text("9780804429573"));
    for raw in [
        "0306406153",
        "0-306-40615-2",
        "9780306406157",
        "030640615",
        "",
    ] {
        assert_eq!(
            failure(&program, "catalogued_book", vec![text(raw)]).name,
            "NotABook",
            "{raw}"
        );
    }
}

#[test]
fn a_flight_is_written_one_way() {
    let program = main(RECORDS).with_proof_checks();
    let Value::Record(flight) = value(&program, "booked_flight", vec![text("BA123")]) else {
        panic!("a flight");
    };
    assert_eq!(flight.get("airline"), Some(&text("BA")));
    assert_eq!(flight.get("number"), Some(&Value::Int(123)));
    assert_eq!(flight.get("suffix"), Some(&Value::None));
    let Value::Record(suffixed) = value(&program, "booked_flight", vec![text("U21234A")]) else {
        panic!("a flight");
    };
    assert_eq!(suffixed.get("suffix"), Some(&text("A")));
    for raw in ["BA0123", "BA 123", "ba123", "BA12345", "BAW123", "BA123AB"] {
        assert_eq!(
            failure(&program, "booked_flight", vec![text(raw)]).name,
            "NotAFlight",
            "{raw}"
        );
    }
}

#[test]
fn a_parcel_is_labelled_in_the_carrier_s_units() {
    let program = main(RECORDS).with_proof_checks();
    let parcel = value(&program, "standard_box", vec![]);
    assert_eq!(
        value(
            &program,
            "parcel_size",
            vec![parcel.clone(), locale("en_gb")]
        ),
        text("30.48 × 22.86 × 10.16 cm")
    );
    assert_eq!(
        value(&program, "parcel_in_inches", vec![parcel, locale("en_us")]),
        text("12 × 9 × 4 in")
    );
}

const HEALTH: &str = "type MonitorText = Text {}

type MeterReading = Decimal {
    scale: 1
}

type BadPulse = Failure {
    kind: FailureKind.invalid_value
}

type BadGlucose = Failure {
    kind: FailureKind.invalid_value
}

/// A pulse as a monitor sends it, `72 bpm`.
function monitored_pulse(raw: MonitorText) fails BadPulse -> HeartRate {
    var pulse: HeartRate = attempt text.parse(raw, as: HeartRate) {
        malformed: BadPulse
        unknown_unit: BadPulse
        min: BadPulse
        max: BadPulse
    }
    return pulse
}

/// A glucose level as a UK meter gives it.
function metered_glucose(millimoles: MeterReading) fails BadGlucose -> BloodGlucose {
    var glucose: BloodGlucose = attempt BloodGlucose(
        units.of(millimoles, ConcentrationUnit.millimole_of_glucose_per_litre)
    ) {
        min: BadGlucose
    }
    return glucose
}

/// A pulse as the chart shows it.
function pulse_shown(pulse: HeartRate, viewer: Locale) -> PresentationText {
    return text.format(pulse, locale: viewer)
}

/// The same in hertz.
function pulse_in_hertz(pulse: HeartRate, viewer: Locale) -> PresentationText {
    return text.format(pulse, locale: viewer, unit: FrequencyUnit.hertz)
}

/// A glucose level in millimoles a litre.
function glucose_shown(glucose: BloodGlucose, viewer: Locale) -> PresentationText {
    return text.format(glucose, locale: viewer)
}

/// The same in milligrams a decilitre.
function glucose_for_us_lab(glucose: BloodGlucose, viewer: Locale) -> PresentationText {
    return text.format(glucose, locale: viewer, unit: ConcentrationUnit.milligram_per_decilitre)
}

/// A reading as the cuff gives it.
function cuff_reading() -> BloodPressure {
    return BloodPressure {
        systolic: 120mmHg
        diastolic: 80mmHg
    }
}

/// A pulse as a nurse counts it.
function counted_pulse() -> HeartRate {
    return HeartRate(72bpm)
}

/// A reading typed in kilopascals.
function typed_reading() -> BloodPressure {
    return BloodPressure {
        systolic: 16kPa
        diastolic: 10.7kPa
    }
}

/// A reading as the chart shows it.
function pressure_shown(pressure: BloodPressure, viewer: Locale) -> PresentationText {
    return text.format(pressure, locale: viewer)
}

/// The same in kilopascals.
function pressure_in_kilopascals(pressure: BloodPressure, viewer: Locale) -> PresentationText {
    return text.format(pressure, locale: viewer, unit: PressureUnit.kilopascal)
}

/// A medicine taken when needed.
function when_needed() -> DosageInstruction {
    return DosageInstruction {
        dose: Dose.volume {
            amount: 10ml
        }
        route: RouteOfAdministration.oral
        times: 1
        per: 6h
        as_needed: true
    }
}

/// How to take a medicine, as the label prints it.
function label_line(instruction: DosageInstruction, viewer: Locale) -> PresentationText {
    return text.format(instruction, locale: viewer)
}
";

#[test]
fn a_pulse_is_read_in_either_unit_and_shown_in_beats_a_minute() {
    let program = main(HEALTH).with_proof_checks();
    let pulse = value(&program, "monitored_pulse", vec![text("72 bpm")]);
    assert_eq!(
        value(&program, "monitored_pulse", vec![text("1.2 Hz")]),
        pulse
    );
    assert_eq!(
        value(
            &program,
            "pulse_shown",
            vec![pulse.clone(), locale("en_gb")]
        ),
        text("72 bpm")
    );
    assert_eq!(
        value(&program, "pulse_in_hertz", vec![pulse, locale("de_de")]),
        text("1,2 Hz")
    );
    // A pulse that isn't a whole number of hertz' thousandths is shown as
    // the beats it was read as.
    let uneven = value(&program, "monitored_pulse", vec![text("73 bpm")]);
    assert_eq!(
        value(&program, "pulse_shown", vec![uneven, locale("en_gb")]),
        text("73 bpm")
    );
    for raw in ["72", "72 mmHg", "400 bpm", "-1 bpm", ""] {
        assert_eq!(
            failure(&program, "monitored_pulse", vec![text(raw)]).name,
            "BadPulse",
            "{raw}"
        );
    }
}

#[test]
fn a_reading_is_shown_as_a_clinician_reads_it() {
    let program = main(HEALTH).with_proof_checks();
    let cuff = value(&program, "cuff_reading", vec![]);
    assert_eq!(
        value(&program, "pressure_shown", vec![cuff, locale("en_gb")]),
        text("120/80 mmHg")
    );
    let counted = value(&program, "counted_pulse", vec![]);
    assert_eq!(
        value(&program, "pulse_shown", vec![counted, locale("en_gb")]),
        text("72 bpm")
    );
    let reading = value(&program, "typed_reading", vec![]);
    assert_eq!(
        value(
            &program,
            "pressure_shown",
            vec![reading.clone(), locale("en_gb")]
        ),
        text("120/80 mmHg")
    );
    assert_eq!(
        value(
            &program,
            "pressure_in_kilopascals",
            vec![reading, locale("en_gb")]
        ),
        text("16/10.7 kPa")
    );
    let fasting = value(&program, "metered_glucose", vec![decimal("5.5")]);
    assert_eq!(
        value(
            &program,
            "glucose_shown",
            vec![fasting.clone(), locale("en_gb")]
        ),
        text("5.5 mmol/L")
    );
    assert_eq!(
        value(
            &program,
            "glucose_for_us_lab",
            vec![fasting, locale("en_us")]
        ),
        text("99 mg/dL")
    );
    assert_eq!(
        failure(&program, "metered_glucose", vec![decimal("-1.0")]).name,
        "BadGlucose"
    );
    let instruction = value(&program, "when_needed", vec![]);
    assert_eq!(
        value(&program, "label_line", vec![instruction, locale("en_gb")]),
        text("10 ml, Oral use, 1 time per 6 hours, when needed")
    );
}

const READ: &str = "type FormText = Text {}

type Unreadable = Failure {
    kind: FailureKind.invalid_value
}

function clothing(raw: FormText) fails Unreadable -> ClothingSize {
    return attempt text.parse(raw, as: ClothingSize) {
        malformed: Unreadable
    }
}

function shoe(raw: FormText) fails Unreadable -> ShoeSize {
    return attempt text.parse(raw, as: ShoeSize) {
        malformed: Unreadable
    }
}

function tracking(raw: FormText) fails Unreadable -> TrackingNumber {
    return attempt text.parse(raw, as: TrackingNumber) {
        malformed: Unreadable
    }
}

function registration(raw: FormText) fails Unreadable -> VehicleRegistration {
    return attempt text.parse(raw, as: VehicleRegistration) {
        malformed: Unreadable
    }
}

function model(raw: FormText) fails Unreadable -> ModelId {
    return attempt text.parse(raw, as: ModelId) {
        malformed: Unreadable
    }
}

function glucose(raw: FormText) fails Unreadable -> BloodGlucose {
    return attempt text.parse(raw, as: BloodGlucose) {
        malformed: Unreadable
        unknown_unit: Unreadable
        min: Unreadable
    }
}

function rating(sent: FormText) fails Unreadable -> Rating {
    return attempt json.read(sent, as: Rating) {
        malformed: Unreadable
        mismatch: Unreadable
    }
}

function pressure(sent: FormText) fails Unreadable -> BloodPressure {
    return attempt json.read(sent, as: BloodPressure) {
        malformed: Unreadable
        mismatch: Unreadable
    }
}

function dose(sent: FormText) fails Unreadable -> Dose {
    return attempt json.read(sent, as: Dose) {
        malformed: Unreadable
        mismatch: Unreadable
    }
}

function barcode(sent: FormText) fails Unreadable -> Barcode {
    return attempt json.read(sent, as: Barcode) {
        malformed: Unreadable
        mismatch: Unreadable
    }
}

function quantity(sent: FormText) fails Unreadable -> UnitQuantity {
    return attempt json.read(sent, as: UnitQuantity) {
        malformed: Unreadable
        mismatch: Unreadable
    }
}

function jurisdiction(sent: FormText) fails Unreadable -> Jurisdiction {
    return attempt json.read(sent, as: Jurisdiction) {
        malformed: Unreadable
        mismatch: Unreadable
    }
}

function instruction(sent: FormText) fails Unreadable -> DosageInstruction {
    return attempt json.read(sent, as: DosageInstruction) {
        malformed: Unreadable
        mismatch: Unreadable
    }
}

function parcel(sent: FormText) fails Unreadable -> Dimensions {
    return attempt json.read(sent, as: Dimensions) {
        malformed: Unreadable
        mismatch: Unreadable
    }
}
";

#[test]
fn records_are_read_by_their_standards() {
    let program = main(READ).with_proof_checks();
    let cases: &[(&str, &str, &str)] = &[
        ("clothing", "UK 10", "UK XXXL"),
        ("clothing", "US M", "FR 38"),
        ("shoe", "EU 42", "UK 7.25"),
        ("tracking", "UPSN 1Z999AA10123456784", "UPSN 12345"),
        ("registration", "DE M-AB 1234", "GB ab12 cde"),
        ("model", "meta-llama/Llama-3.1-8B-Instruct", "openai/gpt/4o"),
        (
            "glucose",
            "5.5 millimole_of_glucose_per_litre",
            "5.5 mmol/L",
        ),
        ("glucose", "99 mgdL", "-1 mgdL"),
        (
            "rating",
            r#"{"value":"4.5","best":"5","worst":"1"}"#,
            r#"{"value":"6","best":"5","worst":"1"}"#,
        ),
        (
            "rating",
            r#"{"value":"1","best":"5","worst":"1"}"#,
            r#"{"value":"1","best":"1","worst":"1"}"#,
        ),
        (
            "pressure",
            r#"{"systolic":{"value":"120","unit":"mmHg"},"diastolic":{"value":"80","unit":"mmHg"}}"#,
            r#"{"systolic":{"value":"80","unit":"mmHg"},"diastolic":{"value":"120","unit":"mmHg"}}"#,
        ),
        (
            "dose",
            r#"{"tag":"mass","amount":{"value":"125","unit":"mcg"}}"#,
            r#"{"tag":"mass","amount":{"value":"0","unit":"mg"}}"#,
        ),
        (
            "barcode",
            r#"{"tag":"qr_code","data":"https://example.com/a1"}"#,
            r#"{"tag":"ean13","data":"4006381333932"}"#,
        ),
        (
            "quantity",
            r#"{"tag":"each","count":3}"#,
            r#"{"tag":"each","count":-1}"#,
        ),
        (
            "jurisdiction",
            r#"{"tag":"subdivision","subdivision":"GB-ENG"}"#,
            r#"{"tag":"country","country":"UK"}"#,
        ),
        (
            "instruction",
            r#"{"dose":{"tag":"mass","amount":{"value":"500","unit":"mg"}},"route":"oral","times":2,"per":"PT24H","as_needed":false}"#,
            r#"{"dose":{"tag":"mass","amount":{"value":"500","unit":"mg"}},"route":"oral","times":2,"per":"PT0S","as_needed":false}"#,
        ),
        (
            "parcel",
            r#"{"length":{"value":"0.3","unit":"metre"},"width":{"value":"0.2","unit":"metre"},"height":{"value":"0.1","unit":"metre"}}"#,
            r#"{"length":{"value":"-0.3","unit":"metre"},"width":{"value":"0.2","unit":"metre"},"height":{"value":"0.1","unit":"metre"}}"#,
        ),
    ];
    for (function, valid, invalid) in cases {
        assert!(
            !matches!(value(&program, function, vec![text(valid)]), Value::None),
            "{function}: `{valid}`"
        );
        assert_eq!(
            failure(&program, function, vec![text(invalid)]).name,
            "Unreadable",
            "{function}: `{invalid}`"
        );
    }
}
