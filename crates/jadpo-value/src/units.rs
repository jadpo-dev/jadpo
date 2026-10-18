//! Physical quantities at runtime (area 19, "Physical quantities"): the
//! `units` operations and the operator table's quantity rows, over the
//! units and the product table in `jadpo_std::quantity`, so both executors
//! convert and multiply with the same exact arithmetic. A quantity is held
//! in its kind's base unit at the kind's scale; every result is exact and
//! then rounded half to even to its kind's scale.

use std::cmp::Ordering;

use jadpo_std::quantity::{self, Side};
use jadpo_std::{QuantityKind, Rounding};

use crate::{Decimal, Fault, Quantity, Value};

/// A quantity of `kind` holding `amount` in its base unit.
pub fn value(kind: QuantityKind, amount: i128) -> Value {
    Value::Quantity(Quantity { kind, amount })
}

/// The unit a variant of a unit enum is (`LengthUnit.kilometre`).
fn unit_of(unit: &Value) -> Result<&'static jadpo_std::QuantityUnit, Fault> {
    match unit {
        Value::Variant(variant) => QuantityKind::of_unit_enum(&variant.ty)
            .and_then(|kind| kind.unit_named(&variant.name))
            .ok_or(Fault::Shape),
        _ => Err(Fault::Shape),
    }
}

/// A number as a mantissa and its places.
fn number(value: &Value) -> Result<(i128, u32), Fault> {
    match value {
        Value::Int(n) => Ok((i128::from(*n), 0)),
        Value::Decimal(d) => Ok((d.mantissa, d.scale)),
        _ => Err(Fault::Shape),
    }
}

/// `units.of(value, unit)`: `value` of `unit`, in the base unit at its
/// kind's scale, rounded half to even.
pub fn of(value: &Value, unit: &Value) -> Result<Value, Fault> {
    let unit = unit_of(unit)?;
    let (mantissa, scale) = number(value)?;
    let amount = quantity::to_base(mantissa, scale, unit).ok_or(Fault::DecimalOverflow)?;
    Ok(self::value(unit.kind, amount))
}

/// `units.in(value, unit, scale:, rounding:)`: the quantity in `unit`, a
/// `Decimal` of `scale` places rounded by `rounding`.
pub fn in_unit(
    value: &Value,
    unit: &Value,
    scale: &Value,
    rounding: &Value,
) -> Result<Value, Fault> {
    let (Value::Quantity(held), Value::Int(places)) = (value, scale) else {
        return Err(Fault::Shape);
    };
    let unit = unit_of(unit)?;
    if unit.kind != held.kind {
        return Err(Fault::Shape);
    }
    let places = u32::try_from(*places).map_err(|_| Fault::Shape)?;
    let rounding = crate::rounding_of(rounding)?;
    let mantissa =
        quantity::from_base(held.amount, unit, places, rounding).ok_or(Fault::DecimalOverflow)?;
    Ok(Value::Decimal(Decimal::new(mantissa, places)))
}

/// `units.sum(items)`: the items, all of one kind, added up; `zero`, a
/// quantity of their kind the checker gives, for none.
pub fn sum(items: &Value, zero: &Value) -> Result<Value, Fault> {
    let Value::Quantity(zero) = zero else {
        return Err(Fault::Shape);
    };
    let items = items.items().ok_or(Fault::Shape)?;
    let mut total = zero.amount;
    for item in items {
        match item {
            Value::Quantity(item) if item.kind == zero.kind => {
                total = total
                    .checked_add(item.amount)
                    .ok_or(Fault::DecimalOverflow)?;
            }
            _ => return Err(Fault::Shape),
        }
    }
    bounded(zero.kind, total)
}

/// A quantity whose amount still has at most 38 digits.
fn bounded(kind: QuantityKind, amount: i128) -> Result<Value, Fault> {
    if amount.unsigned_abs() >= 10u128.pow(38) {
        return Err(Fault::DecimalOverflow);
    }
    Ok(value(kind, amount))
}

/// One side of an operator as the product table sees it.
fn side(value: &Value) -> Option<(Side, i128)> {
    match value {
        Value::Quantity(held) => Some((Side::Quantity(held.kind), held.amount)),
        Value::Duration(millis) => Some((Side::Duration, i128::from(*millis))),
        _ => None,
    }
}

/// What the table gives, as a value.
fn from_side((gives, mantissa): (Side, i128)) -> Result<Value, Fault> {
    match gives {
        Side::Quantity(kind) => bounded(kind, mantissa),
        Side::Duration => i64::try_from(mantissa)
            .map(Value::Duration)
            .map_err(|_| Fault::IntOverflow),
    }
}

/// `a + b` where either is a quantity: two of one kind, or a
/// `Temperature` and a `TemperatureDifference`.
pub fn add(a: &Value, b: &Value) -> Result<Value, Fault> {
    combine(a, '+', b)
}

/// `a - b` where either is a quantity: two of one kind, or by the table,
/// a `Temperature` from a `Temperature`.
pub fn subtract(a: &Value, b: &Value) -> Result<Value, Fault> {
    combine(a, '-', b)
}

fn combine(a: &Value, op: char, b: &Value) -> Result<Value, Fault> {
    let (left, right) = (side(a).ok_or(Fault::Shape)?, side(b).ok_or(Fault::Shape)?);
    if let Some(found) = quantity::apply(left, op, right) {
        return from_side(found);
    }
    match (a, b) {
        (Value::Quantity(x), Value::Quantity(y)) if x.kind == y.kind && x.kind.adds_to_itself() => {
            let amount = if op == '+' {
                x.amount.checked_add(y.amount)
            } else {
                x.amount.checked_sub(y.amount)
            };
            bounded(x.kind, amount.ok_or(Fault::DecimalOverflow)?)
        }
        _ => Err(Fault::Shape),
    }
}

/// `a * b` where either is a quantity: a quantity by a plain number keeps
/// its kind, rounded half to even to its scale, and two kinds give what
/// the table says.
pub fn multiply(a: &Value, b: &Value) -> Result<Value, Fault> {
    if let (Some(left), Some(right)) = (side(a), side(b)) {
        return quantity::apply(left, '*', right)
            .ok_or(Fault::Shape)
            .and_then(from_side);
    }
    let (Value::Quantity(held), factor) = (a, b) else {
        return Err(Fault::Shape);
    };
    let (mantissa, places) = number(factor)?;
    let size = quantity::ratio(
        held.amount,
        &[mantissa.unsigned_abs()],
        0,
        &[1],
        places,
        Rounding::HalfEven,
    )
    .ok_or(Fault::DecimalOverflow)?;
    bounded(held.kind, if mantissa < 0 { -size } else { size })
}

/// `a / b` where either is a quantity: by a plain number it keeps its
/// kind, by one of its own kind it gives a plain `Decimal`, as `/` does for
/// a named type (area 3), and two kinds give what the table says.
pub fn divide(a: &Value, b: &Value) -> Result<Value, Fault> {
    if let (Value::Quantity(x), Value::Quantity(y)) = (a, b)
        && x.kind == y.kind
    {
        if y.amount == 0 {
            return Err(Fault::DivisionByZero);
        }
        return x
            .decimal()
            .quotient(y.decimal())
            .map(Value::Decimal)
            .ok_or(Fault::DecimalOverflow);
    }
    if let (Some(left), Some(right)) = (side(a), side(b)) {
        if right.1 == 0 {
            return Err(Fault::DivisionByZero);
        }
        return quantity::apply(left, '/', right)
            .ok_or(Fault::Shape)
            .and_then(from_side);
    }
    let (Value::Quantity(held), divisor) = (a, b) else {
        return Err(Fault::Shape);
    };
    let (mantissa, places) = number(divisor)?;
    if mantissa == 0 {
        return Err(Fault::DivisionByZero);
    }
    let size = quantity::ratio(
        held.amount,
        &[1],
        places,
        &[mantissa.unsigned_abs()],
        0,
        Rounding::HalfEven,
    )
    .ok_or(Fault::DecimalOverflow)?;
    bounded(held.kind, if mantissa < 0 { -size } else { size })
}

/// The order of two quantities of one kind; `None` across kinds.
pub fn compare(a: Quantity, b: Quantity) -> Option<Ordering> {
    (a.kind == b.kind).then(|| a.amount.cmp(&b.amount))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn literal(text: &str) -> Value {
        let (unit, amount) = quantity::parse_literal(text).unwrap();
        value(unit.kind, amount)
    }

    #[test]
    fn quantities_add_and_compare_across_units() {
        assert_eq!(literal("1km"), literal("1000metre"));
        let total = add(
            &add(&literal("2metre"), &literal("45cm")).unwrap(),
            &literal("3inch"),
        )
        .unwrap();
        assert_eq!(total, value(QuantityKind::Length, 2_526_200));
        assert_eq!(
            multiply(&literal("3metre"), &literal("2metre")).unwrap(),
            literal("6sqm")
        );
        assert_eq!(
            divide(&literal("100metre"), &Value::Duration(8_000)).unwrap(),
            literal("12.5mps")
        );
        assert_eq!(
            divide(&literal("1km"), &literal("250metre")).unwrap(),
            Value::Decimal(Decimal::new(4, 0))
        );
        assert_eq!(
            multiply(&literal("1metre"), &Value::Decimal(Decimal::new(15, 2))).unwrap(),
            literal("15cm")
        );
        assert_eq!(
            subtract(&literal("25degC"), &literal("20degC")).unwrap(),
            value(QuantityKind::TemperatureDifference, 5_000)
        );
        assert!(add(&literal("25degC"), &literal("20degC")).is_err());
    }
}
