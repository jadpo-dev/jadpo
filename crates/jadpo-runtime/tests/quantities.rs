//! Area 19, "Physical quantities" (chunk 5.5j), in the runtime: the
//! `units` operations and the operator table's quantity rows as compiled
//! code calls them (`jadpo_value::run` and `jadpo_value::ops`, which the
//! interpreter calls too), every unit converting exactly to its kind's base
//! unit and back, a quantity on the wire as `{ "value": ..., "unit": ... }`
//! and in its `numeric` column in the base unit. The interpreter's half is
//! crates/jadpo-interp/tests/quantities.rs and the page's
//! `project=quantities` tests.

use jadpo_std::quantity::{self, UNITS};
use jadpo_std::{Op, QuantityKind, Rounding};
use jadpo_value::{Decimal, Fault, Value, ops, run, units};

fn call(op: Op, args: &[Value]) -> Result<Value, Fault> {
    run(op, args, None, &mut |_| None)
}

fn literal(text: &str) -> Value {
    let (unit, amount) =
        quantity::parse_literal(text).unwrap_or_else(|why| panic!("{text}: {why:?}"));
    units::value(unit.kind, amount)
}

fn unit(kind: QuantityKind, name: &str) -> Value {
    let ordinal = jadpo_std::variant_ordinal(kind.unit_enum(), name).unwrap();
    Value::variant(kind.unit_enum(), ordinal, name, Vec::new())
}

fn rounding(name: &str) -> Value {
    let ordinal = jadpo_std::variant_ordinal(Rounding::NAME, name).unwrap();
    Value::variant(Rounding::NAME, ordinal, name, Vec::new())
}

#[test]
fn the_showcase_is_exact_in_metres() {
    let total = ops::add(
        &ops::add(&literal("2metre"), &literal("45cm")).unwrap(),
        &literal("3inch"),
    )
    .unwrap();
    assert_eq!(total, units::value(QuantityKind::Length, 2_526_200));
    assert!(ops::equal(&literal("1km"), &literal("1000metre")));
    assert_eq!(
        call(
            Op::UnitsIn,
            &[
                total,
                unit(QuantityKind::Length, "centimetre"),
                Value::Int(2),
                rounding("half_even")
            ]
        ),
        Ok(Value::Decimal(Decimal::new(25262, 2)))
    );
}

#[test]
fn every_unit_converts_to_its_base_and_back() {
    for unit_row in UNITS {
        let kind = unit_row.kind;
        let given = unit(kind, unit_row.name);
        let made = call(Op::UnitsOf, &[Value::Int(1), given.clone()]).unwrap();
        let Value::Quantity(held) = &made else {
            panic!("{}: a quantity", unit_row.name);
        };
        assert_eq!(held.kind, kind, "{}", unit_row.name);
        // One of a unit is shown as one in that unit: exactly where the
        // kind's scale holds it, and otherwise within the places that scale
        // gives in the unit (a microgram is below a milligram, the scale
        // of a `Mass`, so it is held as nothing).
        let shown = quantity::shown_in(held.amount, unit_row);
        if unit_row.name == "microgram" {
            assert_eq!(shown, Some((0, 0)));
        } else {
            assert_eq!(shown, Some((1, 0)), "{}", unit_row.name);
        }
        let back = call(
            Op::UnitsIn,
            &[made.clone(), given, Value::Int(0), rounding("half_even")],
        )
        .unwrap();
        if unit_row.name != "microgram" {
            assert_eq!(
                back,
                Value::Decimal(Decimal::new(1, 0)),
                "{}",
                unit_row.name
            );
        }
        if unit_row.suffix {
            assert_eq!(
                made,
                literal(&format!("1{}", unit_row.code)),
                "{}",
                unit_row.code
            );
        }
    }
}

#[test]
fn the_table_gives_its_kinds() {
    assert_eq!(
        ops::multiply(&literal("3metre"), &literal("2metre")),
        Ok(literal("6sqm"))
    );
    assert_eq!(
        ops::multiply(&literal("6sqm"), &literal("2metre")),
        Ok(units::value(QuantityKind::Volume, 12_000_000_000))
    );
    assert_eq!(
        ops::divide(&literal("6sqm"), &literal("2metre")),
        Ok(literal("3metre"))
    );
    assert_eq!(
        ops::divide(&literal("100metre"), &Value::Duration(8_000)),
        Ok(literal("12.5mps"))
    );
    assert_eq!(
        ops::multiply(&literal("12.5mps"), &Value::Duration(8_000)),
        Ok(literal("100metre"))
    );
    assert_eq!(
        ops::divide(&literal("100metre"), &literal("12.5mps")),
        Ok(Value::Duration(8_000))
    );
    assert_eq!(
        ops::divide(&literal("2kWh"), &Value::Duration(3_600_000)),
        Ok(literal("2kW"))
    );
    assert_eq!(
        ops::multiply(&literal("230V"), &literal("10A")),
        Ok(literal("2.3kW"))
    );
    assert_eq!(
        ops::divide(&literal("1MB"), &Value::Duration(1_000)),
        Ok(literal("8Mbps"))
    );
    assert_eq!(
        ops::multiply(&literal("8Mbps"), &Value::Duration(1_000)),
        Ok(literal("1MB"))
    );
    assert_eq!(
        ops::divide(&literal("1km"), &literal("250metre")),
        Ok(Value::Decimal(Decimal::new(4, 0)))
    );
    assert_eq!(
        ops::multiply(&literal("1metre"), &Value::Decimal(Decimal::new(15, 2))),
        Ok(literal("15cm"))
    );
    assert_eq!(
        ops::divide(&literal("1metre"), &Value::Int(3)),
        Ok(units::value(QuantityKind::Length, 333_333))
    );
    assert_eq!(
        ops::divide(&literal("1metre"), &Value::Int(0)),
        Err(Fault::DivisionByZero)
    );
    assert_eq!(
        ops::add(&literal("20degC"), &literal("20degC")),
        Err(Fault::Shape)
    );
}

#[test]
fn a_quantity_is_sent_and_stored_in_its_base_unit() {
    use jadpo_std::http::{Codec, CodecId};
    use jadpo_std::types::ValidatorId;
    let codecs = [Codec::Scalar {
        builtin: jadpo_std::Builtin::Quantity(QuantityKind::Mass),
        validator: ValidatorId(0),
        int_as_number: false,
    }];
    let validators = [jadpo_value::Validator::new(
        Some(jadpo_std::Builtin::Quantity(QuantityKind::Mass)),
        Vec::new(),
    )];
    let table = jadpo_value::json::Codecs {
        codecs: &codecs,
        validators: &validators,
        shapes: &[],
    };
    assert_eq!(
        table.write_value(CodecId(0), &literal("2.5kg")),
        "{\"value\":\"2.5\",\"unit\":\"kg\"}"
    );
    // Read in any of the kind's units, the number as a string (area 2).
    let read = |json: &str| {
        let parsed: serde_json::Value = serde_json::from_str(json).unwrap();
        let mut problems = Vec::new();
        let value = table.read(
            CodecId(0),
            &parsed,
            &jadpo_value::json::Path::default(),
            &mut problems,
        );
        (
            value,
            problems
                .into_iter()
                .map(|problem| problem.rule)
                .collect::<Vec<_>>(),
        )
    };
    assert_eq!(
        read("{\"value\": \"250\", \"unit\": \"g\"}"),
        (Some(literal("250g")), vec![])
    );
    assert_eq!(read("{\"value\": 250, \"unit\": \"g\"}").1, ["type"]);
    assert_eq!(
        read("{\"value\": \"250\", \"unit\": \"cm\"}").1,
        ["unknown_unit"]
    );
    assert_eq!(read("{\"value\": \"250\"}").1, ["required"]);
    let sql = jadpo_runtime::to_sql(&literal("45cm"), jadpo_std::SqlType::Numeric).unwrap();
    assert_eq!(
        jadpo_runtime::from_sql(
            &sql,
            &jadpo_std::read::Column::Quantity(QuantityKind::Length)
        ),
        Ok(literal("45cm"))
    );
    assert_eq!(QuantityKind::Length.column_type(), "numeric(38, 6)");
}
