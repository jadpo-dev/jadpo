//! Area 19, the records and measures (chunk 5.5m2), in the runtime: an
//! ISBN-10 read as its `Isbn` by `text.parse` as compiled code calls it
//! (`jadpo_value::run`), a heart rate and a parcel shown in the unit the
//! call names or the one a reader expects, and the rules across a rating's
//! and a blood pressure's parts as a value read from JSON meets them. The
//! page's tests (`project=records`, `project=health-measures`) and the
//! interpreter's `records_measures` tests assert the same values; the
//! types' text, JSON and columns are the one harness's (`standard_types`).

use jadpo_std::{Op, QuantityKind};
use jadpo_value::{Decimal, Fault, Quantity, Value, run, standard};

fn call(op: Op, args: &[Value]) -> Result<Value, Fault> {
    run(op, args, None, &mut |_| None)
}

fn variant(ty: &str, name: &str) -> Value {
    let ordinal = jadpo_std::variant_ordinal(ty, name).expect("a variant");
    Value::variant(ty, ordinal, name, Vec::new())
}

fn quantity(kind: QuantityKind, amount: i128) -> Value {
    Value::Quantity(Quantity { kind, amount })
}

#[test]
fn an_isbn_10_reads_as_its_isbn() {
    let isbn10 = variant(jadpo_std::CodeForm::NAME, "isbn10");
    let parse = |raw: &str| {
        call(
            Op::TextParse,
            &[
                Value::text(raw),
                Value::text("Isbn"),
                isbn10.clone(),
                Value::None,
            ],
        )
    };
    assert_eq!(parse("0306406152"), Ok(Value::text("9780306406157")));
    assert_eq!(parse("080442957x"), Ok(Value::text("9780804429573")));
    for raw in ["0306406153", "0-306-40615-2", "9780306406157", ""] {
        assert_eq!(
            parse(raw),
            Err(Fault::Broke(jadpo_std::RuleKey::Malformed)),
            "{raw}"
        );
    }
}

/// `text.format` as compiled code calls it, with `unit:` and the standard
/// type the checker names.
fn shown(value: Value, tag: &str, unit: Value, standard: Value) -> Value {
    call(
        Op::TextFormat,
        &[
            value,
            variant(jadpo_std::LOCALE, tag),
            Value::None,
            Value::None,
            Value::None,
            Value::None,
            Value::None,
            unit,
            standard,
        ],
    )
    .expect("shown")
}

#[test]
fn measures_are_shown_in_the_unit_a_reader_expects() {
    // 73 beats a minute, held as 1.217 Hz, shown as the beats it was.
    let pulse = quantity(QuantityKind::Frequency, 1217);
    let heart_rate = Value::text("HeartRate");
    assert_eq!(
        shown(pulse.clone(), "en_gb", Value::None, heart_rate.clone()),
        Value::text("73 bpm")
    );
    assert_eq!(
        shown(
            pulse.clone(),
            "en_gb",
            variant("FrequencyUnit", "hertz"),
            heart_rate
        ),
        Value::text("1.217 Hz")
    );
    // Without the checker naming it, a frequency is shown as any is.
    assert_eq!(
        shown(pulse, "en_gb", Value::None, Value::None),
        Value::text("1.217 Hz")
    );
    // 5.5 mmol/L of glucose, in either clinical unit.
    let glucose = quantity(QuantityKind::Concentration, 990_856_350);
    let blood_glucose = Value::text("BloodGlucose");
    assert_eq!(
        shown(glucose.clone(), "en_gb", Value::None, blood_glucose.clone()),
        Value::text("5.5 mmol/L")
    );
    assert_eq!(
        shown(
            glucose,
            "de_de",
            variant("ConcentrationUnit", "milligram_per_decilitre"),
            blood_glucose
        ),
        Value::text("99 mg/dL")
    );
    // A parcel in inches, the unit written once.
    let side = |micrometres: i128| quantity(QuantityKind::Length, micrometres);
    let parcel = Value::record(
        "Dimensions",
        vec![
            ("length".into(), side(304_800)),
            ("width".into(), side(228_600)),
            ("height".into(), side(101_600)),
        ],
    );
    assert_eq!(
        shown(
            parcel.clone(),
            "en_us",
            variant("LengthUnit", "inch"),
            Value::None
        ),
        Value::text("12 × 9 × 4 in")
    );
    assert_eq!(
        shown(parcel, "de_de", Value::None, Value::None),
        Value::text("30,48 × 22,86 × 10,16 cm")
    );
}

/// The first rule across its parts a value breaks, by name (chunk 5.5t2).
fn broken(value: &Value) -> Option<&'static str> {
    assert!(matches!(value, Value::Record(_)), "a record");
    jadpo_value::across::broken_standard(value)
}

#[test]
fn rules_across_parts_hold_where_json_is_read() {
    let number = |text: &str| Value::Decimal(Decimal::parse(text).expect("a number"));
    let rating = |value: &str, best: &str, worst: &str| {
        Value::record(
            "Rating",
            vec![
                ("value".into(), number(value)),
                ("best".into(), number(best)),
                ("worst".into(), number(worst)),
            ],
        )
    };
    assert_eq!(broken(&rating("4.5", "5", "1")), None);
    assert_eq!(
        broken(&rating("5", "5", "1")),
        None,
        "the best is on the scale"
    );
    assert_eq!(broken(&rating("5.5", "5", "1")), Some("value_on_scale"));
    assert_eq!(broken(&rating("0.5", "5", "1")), Some("value_on_scale"));
    assert_eq!(broken(&rating("1", "1", "1")), Some("best_above_worst"));
    let pressure = |systolic: i128, diastolic: i128| {
        Value::record(
            "BloodPressure",
            vec![
                (
                    "systolic".into(),
                    quantity(QuantityKind::Pressure, systolic),
                ),
                (
                    "diastolic".into(),
                    quantity(QuantityKind::Pressure, diastolic),
                ),
            ],
        )
    };
    // 120/80 mmHg in thousandths of a pascal.
    assert_eq!(broken(&pressure(15_998_686, 10_665_791)), None);
    assert_eq!(broken(&pressure(10_665_791, 15_998_686)), Some("diastolic_below_systolic"));
    assert_eq!(broken(&pressure(12_000_000, 12_000_000)), Some("diastolic_below_systolic"));
}
