//! The one order of values (area 2, "Set order", decided 2026-10-14): a
//! set has no order code can use, but everything that shows one (reports,
//! JSON, iteration once `for` takes sets) lists it in this order, the same
//! in interpreted and compiled code. It is by value: numbers numerically,
//! text by Unicode code point, instants, dates, times and durations as the
//! moments and lengths they are, enum variants in the order their enum
//! declares them, and objects and tagged unions field by field in declared
//! order. Sets keep their items, and maps their keys, in this order, which
//! is `Value`'s `Ord`.
//!
//! Values of one type are what a set holds, so the rules above decide
//! every order a program can see. Past them the order only has to stay
//! total and agree with `==`: values of different kinds go by kind, and
//! records or variants of different types by their names.

use std::cmp::Ordering;

use crate::{Decimal, Record, Value, Variant};

/// Where a kind of value stands among the others, which only values of
/// different types ever compare by.
fn rank(value: &Value) -> u8 {
    match value {
        Value::Unit => 0,
        Value::None => 1,
        Value::Bool(_) => 2,
        Value::Int(_) | Value::Decimal(_) => 3,
        Value::Text(_) => 4,
        Value::Bytes(_) => 5,
        Value::Uuid(_) => 6,
        Value::Instant(_) => 7,
        Value::Date(_) => 8,
        Value::Zoned(_) => 9,
        Value::Duration(_) => 10,
        Value::List(_) => 11,
        Value::Set(_) => 12,
        Value::Map(_) => 13,
        Value::Record(_) => 14,
        Value::Variant(_) => 15,
    }
}

/// `a` against `b` in the canonical order.
pub fn compare(a: &Value, b: &Value) -> Ordering {
    match (a, b) {
        (Value::Unit, Value::Unit) | (Value::None, Value::None) => Ordering::Equal,
        (Value::Bool(a), Value::Bool(b)) => a.cmp(b),
        (Value::Int(a), Value::Int(b)) => a.cmp(b),
        (Value::Decimal(a), Value::Decimal(b)) => a.cmp(b),
        // Numerically; an `Int` and a `Decimal` of the same value aren't
        // equal (`==` keeps them apart), so the `Int` goes first.
        (Value::Int(a), Value::Decimal(b)) => Decimal::from_int(*a).cmp(b).then(Ordering::Less),
        (Value::Decimal(a), Value::Int(b)) => a.cmp(&Decimal::from_int(*b)).then(Ordering::Greater),
        // UTF-8 bytes sort as their code points do.
        (Value::Text(a), Value::Text(b)) | (Value::Uuid(a), Value::Uuid(b)) => {
            a.as_bytes().cmp(b.as_bytes())
        }
        (Value::Bytes(a), Value::Bytes(b)) => a.cmp(b),
        (Value::Instant(a), Value::Instant(b)) | (Value::Duration(a), Value::Duration(b)) => {
            a.cmp(b)
        }
        (Value::Date(a), Value::Date(b)) => a.cmp(b),
        // On the timeline, then by zone, so the order agrees with `==`.
        (Value::Zoned(a), Value::Zoned(b)) => a.cmp(b),
        (Value::List(a), Value::List(b)) | (Value::Set(a), Value::Set(b)) => items(a, b),
        (Value::Map(a), Value::Map(b)) => {
            for ((ak, av), (bk, bv)) in a.iter().zip(b.iter()) {
                let found = compare(ak, bk).then_with(|| compare(av, bv));
                if found != Ordering::Equal {
                    return found;
                }
            }
            a.len().cmp(&b.len())
        }
        (Value::Record(a), Value::Record(b)) => records(a, b),
        (Value::Variant(a), Value::Variant(b)) => variants(a, b),
        _ => rank(a).cmp(&rank(b)),
    }
}

/// Item by item, then the shorter first.
fn items(a: &[Value], b: &[Value]) -> Ordering {
    for (a, b) in a.iter().zip(b) {
        let found = compare(a, b);
        if found != Ordering::Equal {
            return found;
        }
    }
    a.len().cmp(&b.len())
}

/// Field by field, in the order the type declares them, which is the
/// order a value holds them in.
fn fields(a: &[(std::rc::Rc<str>, Value)], b: &[(std::rc::Rc<str>, Value)]) -> Ordering {
    for ((_, a), (_, b)) in a.iter().zip(b) {
        let found = compare(a, b);
        if found != Ordering::Equal {
            return found;
        }
    }
    a.len().cmp(&b.len()).then_with(|| {
        a.iter()
            .map(|(name, _)| name.as_bytes())
            .cmp(b.iter().map(|(name, _)| name.as_bytes()))
    })
}

fn records(a: &Record, b: &Record) -> Ordering {
    fields(&a.fields, &b.fields).then_with(|| a.ty.as_bytes().cmp(b.ty.as_bytes()))
}

/// By the variant's place in its enum, then its payload's fields.
fn variants(a: &Variant, b: &Variant) -> Ordering {
    a.ordinal
        .cmp(&b.ordinal)
        .then_with(|| fields(&a.fields, &b.fields))
        .then_with(|| a.ty.as_bytes().cmp(b.ty.as_bytes()))
        .then_with(|| a.name.as_bytes().cmp(b.name.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn variant(ty: &str, ordinal: u32, name: &str) -> Value {
        Value::variant(ty, ordinal, name, Vec::new())
    }

    fn shown(value: &Value) -> String {
        value.to_string()
    }

    #[test]
    fn enum_variants_go_in_declared_order_not_by_name() {
        // `Size` declares small, medium, large.
        let set = Value::set(vec![
            variant("Size", 2, "large"),
            variant("Size", 0, "small"),
            variant("Size", 1, "medium"),
        ]);
        assert_eq!(shown(&set), "[Size.small, Size.medium, Size.large]");
    }

    #[test]
    fn numbers_go_by_value_and_text_by_code_point() {
        let numbers = Value::set(vec![Value::Int(10), Value::Int(-2), Value::Int(3)]);
        assert_eq!(shown(&numbers), "[-2, 3, 10]");
        let decimals = Value::set(vec![
            Value::decimal("2.50").expect("a decimal"),
            Value::decimal("10.1").expect("a decimal"),
            Value::decimal("2.5").expect("a decimal"),
        ]);
        assert_eq!(shown(&decimals), "[2.50, 10.1]", "2.5 is 2.50");
        let text = Value::set(vec![
            Value::text("b"),
            Value::text("é"),
            Value::text("B"),
            Value::text("a"),
        ]);
        assert_eq!(shown(&text), "[\"B\", \"a\", \"b\", \"é\"]");
        assert_eq!(
            compare(&Value::Int(1), &Value::decimal("1.0").expect("a decimal")),
            Ordering::Less,
            "unequal values never compare equal"
        );
    }

    #[test]
    fn objects_and_tagged_unions_go_field_by_field_in_declared_order() {
        let point = |y: i64, x: i64| {
            Value::record(
                "Point",
                vec![("y".into(), Value::Int(y)), ("x".into(), Value::Int(x))],
            )
        };
        // `y` is declared first, so it decides before `x`.
        let set = Value::set(vec![point(2, 1), point(1, 9), point(1, 3)]);
        assert_eq!(
            shown(&set),
            "[Point { y: 1, x: 3 }, Point { y: 1, x: 9 }, Point { y: 2, x: 1 }]"
        );
        let paid = |amount: i64| {
            Value::variant(
                "Payment",
                1,
                "paid",
                vec![("amount".into(), Value::Int(amount))],
            )
        };
        let set = Value::set(vec![
            paid(5),
            variant("Payment", 2, "refunded"),
            paid(2),
            variant("Payment", 0, "pending"),
        ]);
        assert_eq!(
            shown(&set),
            "[Payment.pending, Payment.paid { amount: 2 }, Payment.paid { amount: 5 }, Payment.refunded]"
        );
    }

    #[test]
    fn the_order_is_total_and_agrees_with_equality() {
        let values = vec![
            Value::Unit,
            Value::None,
            Value::Bool(true),
            Value::Int(1),
            Value::decimal("1.0").expect("a decimal"),
            Value::text("1"),
            Value::uuid("0192f0c4-3b5e-7c1a-9d2e-4f6a8b0c1d2e"),
            Value::Instant(0),
            Value::Date(0),
            Value::Zoned(crate::Zoned { millis: 0, zone: 0 }),
            Value::Duration(0),
            Value::list(vec![Value::Int(1)]),
            Value::set(vec![Value::Int(1)]),
            Value::record("A", Vec::new()),
            Value::record("B", Vec::new()),
            variant("E", 0, "a"),
            variant("F", 0, "a"),
        ];
        for a in &values {
            for b in &values {
                assert_eq!(compare(a, b) == Ordering::Equal, a == b, "{a} and {b}");
                assert_eq!(compare(a, b), compare(b, a).reverse(), "{a} and {b}");
            }
        }
    }
}
