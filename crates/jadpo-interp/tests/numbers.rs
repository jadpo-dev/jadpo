//! Area 19, "Numbers" (chunk 5.5d), run: each `integer` and `decimal`
//! operation gives what the page says through the interpreter, every
//! rounding mode rounds the way the runtime's test
//! (crates/jadpo-runtime/tests/numbers.rs) holds it to, overflow is a
//! defect, and `text.format` writes a number in the locale's way.

use crate::support;

use jadpo_interp::{FaultKind, Value};
use support::{decimal, fault, int, main, text, value};

const SOURCE: &str = "type Step = Int {}\n\ntype Grams = Int {\n    min: 0\n}\n\ntype TrayCount = Int {\n    min: 1\n    max: 12\n}\n\ntype Change = Decimal {\n    scale: 3\n}\n\ntype LoafPrice = Decimal {\n    scale: 2\n    min: 0.00\n}\n\nfunction drift(a: Step, b: Step) -> Step {\n    return Step(integer.abs(a - b))\n}\n\nfunction size(a: Step) -> Step {\n    return Step(integer.abs(a))\n}\n\nfunction fewer(a: TrayCount, b: TrayCount) -> TrayCount {\n    return integer.min(a, b)\n}\n\nfunction more(a: TrayCount, b: TrayCount) -> TrayCount {\n    return integer.max(a, b)\n}\n\nfunction whole(value: Change, mode: Rounding) -> Step {\n    return Step(integer.from_decimal(value, rounding: mode))\n}\n\nfunction places(value: Change, mode: Rounding) -> Change {\n    return Change(decimal.round(value, scale: 1, rounding: mode))\n}\n\nfunction kept(value: Change) -> Change {\n    return Change(decimal.round(value, scale: 3, rounding: Rounding.up))\n}\n\nfunction share(total: LoafPrice, people: TrayCount, mode: Rounding) -> LoafPrice {\n    return LoafPrice(decimal.divide(total, people, rounding: mode))\n}\n\nfunction cheaper(a: LoafPrice, b: LoafPrice) -> LoafPrice {\n    return decimal.min(a, b)\n}\n\nfunction dearer(a: LoafPrice, b: LoafPrice) -> LoafPrice {\n    return decimal.max(a, b)\n}\n\nfunction size_of(a: Change) -> Change {\n    return Change(decimal.abs(a))\n}\n\nfunction shown(price: LoafPrice, reader: Locale) -> PresentationText {\n    return text.format(price, locale: reader)\n}\n\nfunction counted(count: Grams, reader: Locale) -> PresentationText {\n    return text.format(count, locale: reader)\n}\n";

/// `Rounding.<name>`, as a call passes it.
fn mode(name: &str) -> Value {
    let ordinal = jadpo_std::variant_ordinal(jadpo_std::Rounding::NAME, name)
        .unwrap_or_else(|| panic!("Rounding.{name}"));
    Value::variant(jadpo_std::Rounding::NAME, ordinal, name, Vec::new())
}

fn locale(name: &str) -> Value {
    let ordinal = jadpo_std::locale_ordinal(name).unwrap_or_else(|| panic!("Locale.{name}"));
    Value::variant(jadpo_std::LOCALE, ordinal, name, Vec::new())
}

/// Each mode to a whole number and to one place: the same table as the
/// runtime's test.
const ROUNDINGS: &[(&str, &str, i64, &str)] = &[
    // (value, mode, whole, one place)
    ("2.250", "half_even", 2, "2.2"),
    ("2.350", "half_even", 2, "2.4"),
    ("2.500", "half_even", 2, "2.5"),
    ("3.500", "half_even", 4, "3.5"),
    ("2.250", "half_up", 2, "2.3"),
    ("-2.250", "half_up", -2, "-2.3"),
    ("-2.500", "half_up", -3, "-2.5"),
    ("2.210", "up", 3, "2.3"),
    ("-2.210", "up", -3, "-2.3"),
    ("2.000", "up", 2, "2.0"),
    ("2.290", "down", 2, "2.2"),
    ("-2.290", "down", -2, "-2.2"),
    ("-0.990", "down", 0, "-0.9"),
];

#[test]
fn every_rounding_mode_rounds_as_the_runtime_does() {
    let program = main(SOURCE);
    for (given, rounding, whole, place) in ROUNDINGS {
        let given_value = decimal(given);
        assert_eq!(
            value(&program, "whole", vec![given_value.clone(), mode(rounding)]),
            int(*whole),
            "{given} {rounding} to a whole number"
        );
        assert_eq!(
            value(&program, "places", vec![given_value, mode(rounding)]),
            decimal(place),
            "{given} {rounding} to one place"
        );
    }
    // A division rounds by the same modes: 10.00 by 3 is 3.333...
    let ten = decimal("10.00");
    assert_eq!(
        value(&program, "share", vec![ten.clone(), int(3), mode("up")]),
        decimal("3.34")
    );
    assert_eq!(
        value(&program, "share", vec![ten, int(3), mode("down")]),
        decimal("3.33")
    );
    // Fewer places than `scale:` are kept as they are.
    assert_eq!(
        value(&program, "kept", vec![decimal("1.250")]),
        decimal("1.25")
    );
}

#[test]
fn min_max_and_abs_give_the_page_s_values() {
    let program = main(SOURCE).with_proof_checks();
    assert_eq!(value(&program, "drift", vec![int(980), int(1000)]), int(20));
    assert_eq!(
        value(&program, "drift", vec![int(1020), int(1000)]),
        int(20)
    );
    assert_eq!(value(&program, "fewer", vec![int(10), int(8)]), int(8));
    assert_eq!(value(&program, "more", vec![int(10), int(8)]), int(10));
    assert_eq!(
        value(&program, "cheaper", vec![decimal("4.20"), decimal("3.80")]),
        decimal("3.80")
    );
    assert_eq!(
        value(&program, "dearer", vec![decimal("4.20"), decimal("3.80")]),
        decimal("4.20")
    );
    let size = value(&program, "size_of", vec![decimal("-2.500")]);
    assert_eq!(size.to_string(), "2.500", "the same scale");
    // The smallest `Int` has no positive counterpart: a defect, as for `-`.
    let overflow = fault(&program, "size", vec![int(i64::MIN)]);
    assert_eq!(overflow.kind, FaultKind::IntOverflow);
    let past = fault(
        &program,
        "whole",
        vec![decimal("100000000000000000000.000"), mode("down")],
    );
    assert_eq!(past.kind, FaultKind::IntOverflow);
}

#[test]
fn a_number_is_shown_in_the_locale_s_digits_and_separators() {
    let program = main(SOURCE);
    let price = decimal("1234.50");
    assert_eq!(
        value(&program, "shown", vec![price.clone(), locale("en_gb")]),
        text("1,234.50")
    );
    assert_eq!(
        value(&program, "shown", vec![price, locale("de_de")]),
        text("1.234,50")
    );
    assert_eq!(
        value(&program, "counted", vec![int(1500), locale("en_gb")]),
        text("1,500")
    );
}
