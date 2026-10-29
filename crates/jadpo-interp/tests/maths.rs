//! Area 19, "Numbers" (chunk 5.5e), run: the maths on `Decimal` gives its
//! correctly rounded digits through the interpreter, as it does in the
//! runtime against the full reference set
//! (crates/jadpo-runtime/tests/maths.rs); `ln` of a value at or below 0
//! fails with the failure its `attempt` maps `out_of_range` to; overflow
//! is a defect.

use crate::support;

use jadpo_interp::FaultKind;
use support::{decimal, failure, fault, int, main, value};

const SOURCE: &str = "type Answer = Decimal {\n    scale: 18\n}\n\ntype Change = Decimal {\n    scale: 6\n}\n\ntype Measured = Decimal {\n    scale: 3\n    min: 0.000\n}\n\ntype Turns = Int {\n    min: 0\n}\n\ntype NotPositive = Failure {\n    kind: FailureKind.invalid_value\n}\n\nfunction side(area: Measured) -> Answer {\n    return Answer(decimal.sqrt(area, scale: 3, rounding: Rounding.half_even))\n}\n\nfunction side_up(area: Measured) -> Answer {\n    return Answer(decimal.sqrt(area, scale: 3, rounding: Rounding.up))\n}\n\nfunction grown(rate: Change, years: Turns) -> Answer {\n    return Answer(decimal.power(rate, years))\n}\n\nfunction growth(rate: Change) -> Answer {\n    return Answer(decimal.exp(rate, scale: 6, rounding: Rounding.half_even))\n}\n\nfunction logarithm(value: Change) fails NotPositive -> Answer {\n    var result: Decimal = attempt decimal.ln(value, scale: 6, rounding: Rounding.half_even) {\n        out_of_range: NotPositive\n    }\n    return Answer(result)\n}\n\nfunction sine(angle: Change) -> Answer {\n    return Answer(decimal.sin(angle, scale: 6, rounding: Rounding.half_even))\n}\n\nfunction cosine(angle: Change) -> Answer {\n    return Answer(decimal.cos(angle, scale: 6, rounding: Rounding.down))\n}\n\nfunction bearing(y: Change, x: Change) -> Answer {\n    return Answer(decimal.atan2(y, x, scale: 6, rounding: Rounding.half_even))\n}\n\nfunction circle(radius: Measured) -> Measured {\n    return Measured(decimal.pi(scale: 3, rounding: Rounding.half_even) * radius * radius)\n}\n";

#[test]
fn each_operation_gives_its_correctly_rounded_digits() {
    // References: tools/references/decimal_maths_reference.py's values (Python's
    // `decimal` module at 250 digits), as reference.tsv lists them.
    let program = main(SOURCE).with_proof_checks();
    // Equal by value: `Answer` keeps 18 places, the runtime's test the
    // places each call gives.
    let call = |name: &str, args| value(&program, name, args);
    assert_eq!(call("side", vec![decimal("2.000")]), decimal("1.414"));
    assert_eq!(call("side_up", vec![decimal("2.000")]), decimal("1.415"));
    assert_eq!(call("side", vec![decimal("4.000")]), decimal("2.000"));
    assert_eq!(
        call("grown", vec![decimal("1.500000"), int(3)]),
        decimal("3.375")
    );
    assert_eq!(
        call("growth", vec![decimal("1.000000")]),
        decimal("2.718282")
    );
    assert_eq!(
        call("logarithm", vec![decimal("2.000000")]),
        decimal("0.693147")
    );
    assert_eq!(
        call("logarithm", vec![decimal("1.000000")]),
        decimal("0.000000")
    );
    assert_eq!(call("sine", vec![decimal("1.000000")]), decimal("0.841471"));
    assert_eq!(
        call("cosine", vec![decimal("1.000000")]),
        decimal("0.540302")
    );
    assert_eq!(
        call("bearing", vec![decimal("1.000000"), decimal("-1.000000")]),
        decimal("2.356194")
    );
    assert_eq!(
        call("bearing", vec![decimal("0.000000"), decimal("0.000000")]),
        decimal("0.000000")
    );
    assert_eq!(call("circle", vec![decimal("2.000")]), decimal("12.568"));
}

#[test]
fn a_logarithm_of_zero_or_less_is_the_failure_its_attempt_maps() {
    let program = main(SOURCE);
    for given in ["0.000000", "-1.000000"] {
        assert_eq!(
            failure(&program, "logarithm", vec![decimal(given)]).name,
            "NotPositive",
            "ln({given})"
        );
    }
}

#[test]
fn a_result_past_38_digits_is_a_defect() {
    let program = main(SOURCE);
    assert_eq!(
        fault(&program, "growth", vec![decimal("100.000000")]).kind,
        FaultKind::DecimalOverflow
    );
    assert_eq!(
        fault(&program, "grown", vec![decimal("2.000000"), int(200)]).kind,
        FaultKind::DecimalOverflow
    );
}
