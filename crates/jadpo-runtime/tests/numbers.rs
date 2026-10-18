//! Area 19's "Numbers" example, in the runtime: the values the page's test
//! (`project=number-operations`) asserts, given to each operation's one
//! implementation as compiled code calls it (`jadpo_value::run`, which the
//! interpreter calls too), and every rounding mode by the same table as the
//! interpreter's test (crates/jadpo-interp/tests/numbers.rs).

use jadpo_std::{Op, Rounding};
use jadpo_value::{Fault, Value, run};

fn call(op: Op, args: &[Value]) -> Result<Value, Fault> {
    run(op, args, None, &mut |_| None)
}

fn ok(op: Op, args: &[Value]) -> Value {
    call(op, args).unwrap_or_else(|fault| panic!("{op:?}: {fault:?}"))
}

fn decimal(text: &str) -> Value {
    Value::decimal(text).unwrap_or_else(|| panic!("{text} is a decimal"))
}

fn mode(name: &str) -> Value {
    let ordinal = jadpo_std::variant_ordinal(Rounding::NAME, name)
        .unwrap_or_else(|| panic!("Rounding.{name}"));
    Value::variant(Rounding::NAME, ordinal, name, Vec::new())
}

fn locale(name: &str) -> Value {
    let ordinal = jadpo_std::locale_ordinal(name).unwrap_or_else(|| panic!("Locale.{name}"));
    Value::variant(jadpo_std::LOCALE, ordinal, name, Vec::new())
}

/// Each mode to a whole number and to one place: the same table as the
/// interpreter's test.
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
fn every_rounding_mode_rounds_as_the_interpreter_does() {
    for (given, rounding, whole, place) in ROUNDINGS {
        assert_eq!(
            ok(Op::IntegerFromDecimal, &[decimal(given), mode(rounding)]),
            Value::Int(*whole),
            "{given} {rounding} to a whole number"
        );
        assert_eq!(
            ok(
                Op::DecimalRound,
                &[decimal(given), Value::Int(1), mode(rounding)]
            ),
            decimal(place),
            "{given} {rounding} to one place"
        );
    }
    let ten = decimal("10.00");
    assert_eq!(
        ok(Op::DecimalDivide, &[ten.clone(), Value::Int(3), mode("up")]),
        decimal("3.34")
    );
    assert_eq!(
        ok(Op::DecimalDivide, &[ten, Value::Int(3), mode("down")]),
        decimal("3.33")
    );
    assert_eq!(
        ok(
            Op::IntegerDivide,
            &[Value::Int(7), Value::Int(2), mode("down")]
        ),
        Value::Int(3)
    );
    // Fewer places than `scale:` are kept as they are.
    let kept = ok(
        Op::DecimalRound,
        &[decimal("1.25"), Value::Int(3), mode("up")],
    );
    assert_eq!(kept.to_string(), "1.25");
}

#[test]
fn the_number_operations_give_the_page_s_values() {
    let n = Value::Int;
    // `drift`: how far a weighing is from the recipe, either way.
    assert_eq!(ok(Op::IntegerAbs, &[n(980 - 1000)]), n(20));
    assert_eq!(ok(Op::IntegerAbs, &[n(1020 - 1000)]), n(20));
    // `trays_to_bake`: never more than the oven holds.
    assert_eq!(ok(Op::IntegerMin, &[n(10), n(8)]), n(8));
    assert_eq!(ok(Op::IntegerMin, &[n(3), n(8)]), n(3));
    assert_eq!(ok(Op::IntegerMax, &[n(3), n(8)]), n(8));
    // `loaves_from`: whole loaves at 0.450 kg, rounded down.
    let per_loaf = |flour: &str| {
        let Ok(share) = jadpo_value::ops::divide(&decimal(flour), &decimal("0.450")) else {
            panic!("{flour} / 0.450");
        };
        ok(Op::IntegerFromDecimal, &[share, mode("down")])
    };
    assert_eq!(per_loaf("10.000"), n(22));
    assert_eq!(per_loaf("0.449"), n(0));
    // `sale_price`: a fifth off, never below the floor.
    assert_eq!(
        ok(Op::DecimalMax, &[decimal("3.36"), decimal("3.50")]),
        decimal("3.50")
    );
    assert_eq!(
        ok(Op::DecimalMax, &[decimal("4.00"), decimal("3.50")]),
        decimal("4.00")
    );
    // `label_weight`: to the nearest 100 g.
    let label = |weight: &str| {
        ok(
            Op::DecimalRound,
            &[decimal(weight), Value::Int(1), mode("half_up")],
        )
    };
    assert_eq!(label("1.250"), decimal("1.3"));
    assert_eq!(label("1.249"), decimal("1.2"));
    assert_eq!(ok(Op::DecimalAbs, &[decimal("-2.50")]), decimal("2.50"));
    assert_eq!(
        ok(Op::DecimalMin, &[decimal("4.20"), decimal("3.80")]),
        decimal("3.80")
    );
    // `price_label`: in the reader's locale.
    let shown = |tag: &str| {
        ok(
            Op::TextFormat,
            &[
                decimal("1234.50"),
                locale(tag),
                Value::None,
                Value::None,
                Value::None,
                Value::None,
                Value::None,
                // No `unit:`, and no standard type held as text (area 19).
                Value::None,
                Value::None,
            ],
        )
    };
    assert_eq!(shown("en_gb"), Value::text("1,234.50"));
    assert_eq!(shown("de_de"), Value::text("1.234,50"));
    // Overflow is a defect, as it is for `-`.
    assert_eq!(
        call(Op::IntegerAbs, &[n(i64::MIN)]),
        Err(Fault::IntOverflow)
    );
}
