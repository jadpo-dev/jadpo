//! The contract's JSON for a standard type a built-in type holds (area 19,
//! "Every boundary through the contract"; chunk 5.5w3): its canonical text
//! as a JSON string, or, for one its entry sends as an object
//! ([`Json::Parts`]), an object of its members, each read and written as
//! its [`Member`] says. Every boundary that carries JSON (a request, a
//! response, `json.read` and `json.write`) reads and writes a standard type
//! through here, by its entry, so none has an encoder of its own.

use std::fmt::Write as _;

use jadpo_std::standard::{Holds, Json, Member, StandardType};

use crate::json::write_string;
use crate::{Decimal, Money, Quantity, Value, Zoned};

/// One problem reading a standard type's JSON: the member it is at, or
/// `None` for the value as a whole, and the rule it broke, never the value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refused {
    pub member: Option<String>,
    pub rule: &'static str,
}

impl Refused {
    fn whole(rule: &'static str) -> Refused {
        Refused { member: None, rule }
    }

    fn at(member: &str, rule: &'static str) -> Refused {
        Refused {
            member: Some(member.to_string()),
            rule,
        }
    }
}

/// The value of `entry`'s type that `json` holds, as the contract sends
/// it, before a type's own rules are checked: its canonical text as a JSON
/// string, read by the one reader of its text
/// ([`super::scalar_text`]), or an object of its members, each problem at
/// its member, a member the entry hasn't `unknown_field`. JSON of another
/// kind breaks `type`.
pub fn read(entry: &StandardType, json: &serde_json::Value) -> Result<Value, Vec<Refused>> {
    match (entry.json, entry.holds, json) {
        (Json::Parts(members), _, serde_json::Value::Object(given)) => {
            object(entry, members, given)
        }
        (Json::Parts(_), ..) => Err(vec![Refused::whole("type")]),
        (_, Holds::Builtin(builtin), serde_json::Value::String(text)) => {
            super::scalar_text(builtin, text).map_err(|rule| vec![Refused::whole(rule)])
        }
        _ => Err(vec![Refused::whole("type")]),
    }
}

/// `value`, of `entry`'s type, written as the contract sends it: its
/// canonical text, or an object of its members in its entry's order.
pub fn write(entry: &StandardType, value: &Value, out: &mut String) {
    match entry.json {
        Json::Parts(members) => match parts_of(value) {
            Some(parts) if parts.len() == members.len() => {
                out.push('{');
                for (at, ((name, _), part)) in members.iter().zip(&parts).enumerate() {
                    if at > 0 {
                        out.push(',');
                    }
                    write_string(name, out);
                    out.push(':');
                    match part {
                        // A whole number of minor units, as a JSON number.
                        Value::Int(n) => {
                            let _ = write!(out, "{n}");
                        }
                        Value::Text(text) => write_string(text, out),
                        other => match super::from(other) {
                            Ok(Value::Text(text)) => write_string(&text, out),
                            _ => out.push_str("null"),
                        },
                    }
                }
                out.push('}');
            }
            _ => out.push_str("null"),
        },
        _ => match super::canonical(entry, value) {
            Ok(text) => write_string(&text, out),
            Err(_) => write_string(&value.to_string(), out),
        },
    }
}

/// A value of a type sent as an object of its members, as its members'
/// values in its entry's order: `Money`'s and a `ZonedTime`'s are its
/// columns' ([`super::to_parts`]: its minor units and its currency's code,
/// its instant and its zone's name); a quantity's its number in its kind's
/// base unit, at the kind's scale, and that unit's code.
fn parts_of(value: &Value) -> Option<Vec<Value>> {
    match value {
        Value::Quantity(quantity) => Some(vec![
            Value::text(&jadpo_std::quantity::number_text(
                quantity.amount,
                quantity.kind.scale(),
            )),
            Value::text(quantity.kind.base_unit().code),
        ]),
        _ => super::to_parts(value),
    }
}

/// What one member read gives.
enum Got {
    Value(Value),
    Unit(&'static jadpo_std::quantity::Unit),
}

/// An object of `members` from `given`, each problem reported at its own
/// member, then made into the value.
fn object(
    entry: &StandardType,
    members: &[(&'static str, Member)],
    given: &serde_json::Map<String, serde_json::Value>,
) -> Result<Value, Vec<Refused>> {
    let mut refused = Vec::new();
    let mut got = Vec::with_capacity(members.len());
    for (name, member) in members {
        match given.get(*name) {
            Some(json) => match read_member(entry, *member, json) {
                Ok(value) => got.push(value),
                Err(rule) => refused.push(Refused::at(name, rule)),
            },
            None => refused.push(Refused::at(name, "required")),
        }
    }
    for name in given.keys() {
        if !members.iter().any(|(member, _)| member == name) {
            refused.push(Refused::at(name, "unknown_field"));
        }
    }
    if !refused.is_empty() {
        return Err(refused);
    }
    made(entry, got).map_err(|rule| vec![Refused::whole(rule)])
}

/// One member's value from its JSON, or the rule it breaks.
fn read_member(
    entry: &StandardType,
    member: Member,
    json: &serde_json::Value,
) -> Result<Got, &'static str> {
    match (member, json) {
        (Member::MinorUnits, serde_json::Value::Number(number)) => {
            let n = number.as_i64().ok_or("type")?;
            crate::money::minor(i128::from(n))
                .map(|minor| Got::Value(Value::Int(minor)))
                .map_err(|_| "max")
        }
        (Member::Code(of), serde_json::Value::String(code)) => jadpo_std::standard_type(of)
            .and_then(|of| super::variant_of(of, code))
            .map(Got::Value)
            .ok_or("variant"),
        (Member::Instant, serde_json::Value::String(text)) => super::instant(text)
            .map(|millis| Got::Value(Value::Instant(millis)))
            .ok_or("format"),
        (Member::Number, serde_json::Value::String(text)) => super::decimal(text)
            .map(|number| Got::Value(Value::Decimal(number)))
            .ok_or("type"),
        (Member::Unit, serde_json::Value::String(code)) => entry
            .quantity()
            .and_then(|kind| kind.unit_coded(code))
            .map(Got::Unit)
            .ok_or("unknown_unit"),
        _ => Err("type"),
    }
}

/// The value its members' values make, as its entry holds it; a quantity
/// past what its kind's base unit holds breaks `max`.
fn made(entry: &StandardType, got: Vec<Got>) -> Result<Value, &'static str> {
    use jadpo_std::Builtin;
    let Holds::Builtin(builtin) = entry.holds else {
        return Err("type");
    };
    match (builtin, got.as_slice()) {
        (
            Builtin::Money,
            [
                Got::Value(Value::Int(minor)),
                Got::Value(Value::Variant(currency)),
            ],
        ) => Ok(Value::Money(Money {
            currency: currency.ordinal,
            minor: *minor,
        })),
        (
            Builtin::ZonedTime,
            [
                Got::Value(Value::Instant(millis)),
                Got::Value(Value::Variant(zone)),
            ],
        ) => Ok(Value::Zoned(Zoned {
            millis: *millis,
            zone: zone.ordinal,
        })),
        (
            Builtin::Quantity(kind),
            [
                Got::Value(Value::Decimal(Decimal { mantissa, scale })),
                Got::Unit(unit),
            ],
        ) => jadpo_std::quantity::to_base(*mantissa, *scale, unit)
            .map(|amount| Value::Quantity(Quantity { kind, amount }))
            .ok_or("max"),
        _ => Err("type"),
    }
}
