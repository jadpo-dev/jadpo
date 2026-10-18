//! Area 19, "Numbers" (chunk 5.5e): the maths on `Decimal` checks, with
//! `scale:` written out; `sqrt`'s value and `power`'s exponent are proved
//! never negative while compiling; `ln` fails with `out_of_range` unless
//! its value is proved above 0, and the call's `attempt` maps it.

use crate::support;

use jadpo_db::Layout;
use support::rules;

const TYPES: &str = "type Metres = Decimal {\n    scale: 3\n    min: 0.000\n}\n\ntype Factor = Decimal {\n    scale: 6\n    min: 0.000001\n}\n\ntype Change = Decimal {\n    scale: 6\n}\n\ntype Turns = Int {\n    min: 0\n}\n\ntype Step = Int {}\n\ntype NotPositive = Failure {\n    kind: FailureKind.invalid_value\n}\n\n";

fn check(source: &str) -> Vec<String> {
    rules(
        Layout::Loose,
        &[("main.jadpo", &format!("{TYPES}{source}"))],
    )
}

const NONE: &[&str] = &[];

#[test]
fn every_maths_operation_checks() {
    let source = "function side(area: Metres) -> Metres {\n    return Metres(decimal.sqrt(area, scale: 3, rounding: Rounding.half_even))\n}\n\nfunction grown(rate: Change, years: Turns) -> Change {\n    return Change(decimal.power(rate, years))\n}\n\nfunction growth(rate: Change) -> Change {\n    return Change(decimal.exp(rate, scale: 6, rounding: Rounding.half_even))\n}\n\nfunction decay(ratio: Factor) -> Change {\n    return Change(decimal.ln(ratio, scale: 6, rounding: Rounding.down))\n}\n\nfunction turn(angle: Change) -> List<Change> {\n    return [Change(decimal.sin(angle, scale: 6, rounding: Rounding.half_up)), Change(decimal.cos(angle, scale: 6, rounding: Rounding.up))]\n}\n\nfunction bearing(north: Change, east: Change) -> Change {\n    return Change(decimal.atan2(east, north, scale: 6, rounding: Rounding.half_even))\n}\n\nfunction circle(radius: Metres) -> Metres {\n    return Metres(decimal.pi(scale: 3, rounding: Rounding.half_even) * radius * radius)\n}\n\nfunction literals() -> List<Change> {\n    return [Change(decimal.sqrt(2.0, scale: 6, rounding: Rounding.down)), Change(decimal.power(1.5, 3)), Change(decimal.ln(2.0, scale: 6, rounding: Rounding.down))]\n}\n";
    assert_eq!(check(source), NONE);
}

#[test]
fn a_square_root_s_value_and_a_power_are_never_negative() {
    assert_eq!(
        check(
            "function f(change: Change) -> Change {\n    return Change(decimal.sqrt(change, scale: 6, rounding: Rounding.down))\n}\n"
        ),
        ["type.argument_negative"]
    );
    assert_eq!(
        check(
            "function f() -> Change {\n    return Change(decimal.sqrt(-2.0, scale: 6, rounding: Rounding.down))\n}\n"
        ),
        ["type.argument_negative"]
    );
    assert_eq!(
        check(
            "function f(rate: Change, n: Step) -> Change {\n    return Change(decimal.power(rate, n))\n}\n"
        ),
        ["type.argument_negative"]
    );
    assert_eq!(
        check(
            "function f(rate: Change) -> Change {\n    return Change(decimal.power(rate, -1))\n}\n"
        ),
        ["type.argument_negative"]
    );
}

#[test]
fn a_logarithm_fails_unless_its_value_is_proved_above_zero() {
    // A value that may be 0 or less needs its `attempt`.
    assert_eq!(
        check(
            "function f(change: Change) -> Change {\n    return Change(decimal.ln(change, scale: 6, rounding: Rounding.down))\n}\n"
        ),
        ["failure.attempt_required"]
    );
    assert_eq!(
        check(
            "function f(change: Change) fails NotPositive -> Change {\n    var logarithm: Decimal = attempt decimal.ln(change, scale: 6, rounding: Rounding.down) {\n        out_of_range: NotPositive\n    }\n    return Change(logarithm)\n}\n"
        ),
        NONE
    );
    assert_eq!(
        check(
            "function f(change: Change) fails NotPositive -> Change {\n    var logarithm: Decimal = attempt decimal.ln(change, scale: 6, rounding: Rounding.down) {\n        invalid: NotPositive\n    }\n    return Change(logarithm)\n}\n"
        ),
        NONE
    );
    // A `min` of 0 allows 0, which has no logarithm.
    assert_eq!(
        check(
            "function f(length: Metres) -> Change {\n    return Change(decimal.ln(length, scale: 6, rounding: Rounding.down))\n}\n"
        ),
        ["failure.attempt_required"]
    );
    // Proved above 0, it never fails, so it takes no `attempt`.
    assert_eq!(
        check(
            "function f(ratio: Factor) fails NotPositive -> Change {\n    var logarithm: Decimal = attempt decimal.ln(ratio, scale: 6, rounding: Rounding.down) {\n        out_of_range: NotPositive\n    }\n    return Change(logarithm)\n}\n"
        ),
        ["failure.attempt_infallible"]
    );
}

#[test]
fn a_scale_is_written_out() {
    assert_eq!(
        check(
            "function f(places: Step) -> Change {\n    return Change(decimal.pi(scale: places, rounding: Rounding.down))\n}\n"
        ),
        ["type.mismatch"]
    );
    assert_eq!(
        check(
            "function f() -> Change {\n    return Change(decimal.pi(scale: -1, rounding: Rounding.down))\n}\n"
        ),
        ["type.argument_negative"]
    );
    assert_eq!(
        check(
            "function f() -> Change {\n    return Change(decimal.pi(scale: 19, rounding: Rounding.down))\n}\n"
        ),
        ["type.mismatch"]
    );
}

#[test]
fn a_square_root_and_e_to_a_power_are_never_below_zero() {
    // Their results carry their floor of 0, so `Metres` needs no arm.
    assert_eq!(
        check(
            "function f(rate: Change) -> Metres {\n    return Metres(decimal.exp(rate, scale: 3, rounding: Rounding.down))\n}\n"
        ),
        NONE
    );
}
