//! Area 19, "The standard operator table" (chunk 5.5j2), in the runtime:
//! the `Percentage` rows as compiled code calls them (`jadpo_value::ops`,
//! which the interpreter calls too), a percentage on the wire as its
//! canonical text and in its `numeric` column as the fraction it is, and
//! `{n, number, percent}` in a message, written as `text.format` shows it.
//! The interpreter's half is crates/jadpo-interp/tests/percentages.rs and
//! the page's `project=percentages` tests.

use jadpo_std::{QuantityKind, Rounding};
use jadpo_value::{Decimal, Fault, Value, ops, units};

fn percent(fraction: &str) -> Value {
    Value::Percentage(Decimal::parse(fraction).unwrap())
}

fn literal(text: &str) -> Value {
    let (unit, amount) = jadpo_std::quantity::parse_literal(text).unwrap();
    units::value(unit.kind, amount)
}

fn amount(minor: i64) -> Value {
    jadpo_value::money::value(jadpo_std::money::ordinal_of_code("GBP").unwrap(), minor)
}

#[test]
fn the_showcase_is_exact_in_metres() {
    let total = ops::add(
        &ops::add(&literal("2metre"), &literal("45cm")).unwrap(),
        &literal("3inch"),
    )
    .unwrap();
    let trim = ops::multiply(&total, &percent("0.15")).unwrap();
    assert_eq!(trim, units::value(QuantityKind::Length, 378_930));
    assert_eq!(
        jadpo_std::percentage::parse_literal("15%"),
        Some(jadpo_std::Num {
            mantissa: 15,
            scale: 2
        })
    );
}

#[test]
fn the_table_s_percentage_rows() {
    // An amount times a percentage, rounded as its type says.
    assert_eq!(
        ops::multiply_rounded(&amount(100), &percent("0.125"), Rounding::HalfEven),
        Ok(amount(12))
    );
    assert_eq!(
        ops::multiply_rounded(&amount(100), &percent("0.125"), Rounding::HalfUp),
        Ok(amount(13))
    );
    // A number keeps its kind; a percentage first gives a percentage.
    assert_eq!(
        ops::multiply(&Value::Int(3), &percent("0.20")),
        Ok(Value::Decimal(Decimal::new(60, 2)))
    );
    assert_eq!(
        ops::multiply(&percent("0.20"), &Value::Int(3)),
        Ok(percent("0.60"))
    );
    assert_eq!(
        ops::multiply(&percent("0.50"), &percent("0.20")),
        Ok(percent("0.10"))
    );
    assert_eq!(
        ops::add(&percent("0.20"), &percent("0.05")),
        Ok(percent("0.25"))
    );
    assert_eq!(
        ops::subtract(&percent("0.20"), &percent("0.05")),
        Ok(percent("0.15"))
    );
    assert_eq!(
        ops::compare(&percent("0.2"), &percent("0.125")),
        Some(std::cmp::Ordering::Greater)
    );
    assert!(ops::equal(&percent("0.2"), &percent("0.20")));
    // A quantity keeps its kind, rounded half to even to its scale.
    assert_eq!(
        ops::multiply(&literal("1metre"), &percent("0.0000015")),
        Ok(units::value(QuantityKind::Length, 2))
    );
    // No row adds a percentage to an amount or a number.
    assert_eq!(ops::add(&amount(100), &percent("0.20")), Err(Fault::Shape));
    assert_eq!(
        ops::add(&Value::Decimal(Decimal::new(5, 1)), &percent("0.20")),
        Err(Fault::Shape)
    );
}

#[test]
fn a_percentage_is_sent_as_its_text_and_stored_as_its_fraction() {
    use jadpo_std::http::{Codec, CodecId};
    use jadpo_std::types::ValidatorId;
    let codecs = [Codec::Scalar {
        builtin: jadpo_std::Builtin::Percentage,
        validator: ValidatorId(0),
        int_as_number: false,
    }];
    let validators = [jadpo_value::Validator::new(
        Some(jadpo_std::Builtin::Percentage),
        Vec::new(),
    )];
    let table = jadpo_value::json::Codecs {
        codecs: &codecs,
        validators: &validators,
        shapes: &[],
    };
    assert_eq!(
        table.write_value(CodecId(0), &percent("0.125")),
        "\"12.5%\""
    );
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
    assert_eq!(read("\"20%\""), (Some(percent("0.2")), vec![]));
    assert_eq!(read("0.2").1, ["type"]);
    assert_eq!(read("\"0.2\"").1, ["format"]);
    let sql = jadpo_runtime::to_sql(&percent("0.125"), jadpo_std::SqlType::Numeric).unwrap();
    assert_eq!(
        jadpo_runtime::from_sql(&sql, &jadpo_std::read::Column::Percentage),
        Ok(percent("0.125"))
    );
}

#[test]
fn a_percentage_is_shown_as_the_locale_writes_one() {
    let shown = |fraction: &str, tag: &str| {
        jadpo_value::present::percent(&Decimal::parse(fraction).unwrap(), tag).unwrap()
    };
    assert_eq!(shown("0.2", "en-GB"), "20%");
    assert_eq!(shown("0.125", "en-GB"), "12.5%");
    assert_eq!(shown("12.345", "en-GB"), "1,234.5%");
    assert_eq!(shown("0.2", "de-DE"), "20\u{a0}%");
    assert_eq!(shown("0.2", "fr-FR"), "20\u{a0}%");
    assert_eq!(shown("-0.05", "en-GB"), "-5%");
}
