//! The records and measures of commerce, health, travel and the rest
//! (area 19, chunk 5.5m2): a flight number's IATA text, the measures shown
//! in the unit a reader expects (`FormatKey::UnitsIn`, `FormatKey::Measures`),
//! a dosage instruction in words (`FormatKey::Dosage`), and the rules
//! ordering an object's parts (`jadpo_std::standard::order_rules`) as a
//! value read from JSON is checked by them. The data is
//! `jadpo_std::standard::records`' and `health`'s.

use jadpo_std::QuantityUnit;
use jadpo_std::standard::{FormatKey, ShownIn, StandardType, standard_type};

use crate::{Fault, Record, Value, present};

/// A flight number as IATA writes it, `BA123`, `BA5A`.
pub fn flight_text(record: &Record) -> Result<String, Fault> {
    let (Some(Value::Text(airline)), Some(Value::Int(number))) =
        (record.get("airline"), record.get("number"))
    else {
        return Err(Fault::Shape);
    };
    let suffix = match record.get("suffix") {
        Some(Value::Text(suffix)) => suffix.to_string(),
        Some(Value::None) | None => String::new(),
        _ => return Err(Fault::Shape),
    };
    Ok(format!("{airline}{number}{suffix}"))
}

/// A flight number read from IATA's text: an airline's two characters, a
/// number from 1 to 9999 with no leading zero, and one capital, its
/// suffix, if it has one.
pub fn read_flight(entry: &StandardType, raw: &str) -> Option<Value> {
    let airline = raw.get(..2)?;
    let rest = raw.get(2..)?;
    let airlines = standard_type("IataAirline")?;
    if !airlines.accepts(airline) {
        return None;
    }
    let (digits, suffix) = match rest.char_indices().find(|(_, c)| !c.is_ascii_digit()) {
        Some((at, _)) => rest.split_at(at),
        None => (rest, ""),
    };
    let number = jadpo_std::standard::web::whole_in(digits, 1, 9999)
        .then(|| digits.parse::<i64>().ok())
        .flatten()?;
    let suffix = match suffix {
        "" => Value::None,
        one if one.len() == 1 && one.bytes().all(|b| b.is_ascii_uppercase()) => Value::text(one),
        _ => return None,
    };
    Some(Value::record(
        entry.name,
        vec![
            ("airline".into(), Value::text(airline)),
            ("number".into(), Value::Int(number)),
            ("suffix".into(), suffix),
        ],
    ))
}

/// The unit a quantity of `kind` is shown in, and its places: the one
/// `unit:` names, at its listed places or exactly; else the one listed for
/// its kind; else its kind's base unit, exactly.
fn unit_for(
    kind: jadpo_std::QuantityKind,
    listed: &[ShownIn],
    unit: &Value,
) -> Result<(&'static QuantityUnit, Option<u32>), Fault> {
    if let Value::Variant(variant) = unit
        && &*variant.ty == kind.unit_enum()
    {
        let named = kind.unit_named(&variant.name).ok_or(Fault::Shape)?;
        let places = listed
            .iter()
            .find(|shown| shown.kind == kind && shown.unit == named.name)
            .and_then(|shown| shown.places);
        return Ok((named, places));
    }
    if !matches!(unit, Value::None) && !matches!(unit, Value::Variant(_)) {
        return Err(Fault::Shape);
    }
    match listed.iter().find(|shown| shown.kind == kind) {
        Some(shown) => Ok((shown.unit().ok_or(Fault::Shape)?, shown.places)),
        None => Ok((kind.base_unit(), None)),
    }
}

/// A quantity shown as `FormatKey::UnitsIn` says: a heart rate in whole
/// beats a minute, a glucose level in millimoles a litre to one place.
pub fn units_in(
    entry: &StandardType,
    value: &Value,
    unit: &Value,
    tag: &str,
) -> Result<String, Fault> {
    let (FormatKey::UnitsIn(listed), Value::Quantity(quantity)) = (entry.format.key, value) else {
        return Err(Fault::Shape);
    };
    let (unit, places) = unit_for(quantity.kind, listed, unit)?;
    present::units::format_at(quantity.amount, unit, places, tag)
}

/// A part of a record or of a choice's variant.
fn part<'v>(value: &'v Value, name: &str) -> Option<&'v Value> {
    match value {
        Value::Record(record) => record.get(name),
        Value::Variant(variant) => variant
            .fields
            .iter()
            .find(|(field, _)| &**field == name)
            .map(|(_, value)| value),
        _ => None,
    }
}

/// An object's or a choice's quantities shown as `FormatKey::Measures`
/// says: each in its kind's unit, or the one `unit:` names, joined; with
/// the unit written once after the numbers where it says so, `120/80
/// mmHg`.
pub fn measures(
    entry: &StandardType,
    value: &Value,
    unit: &Value,
    tag: &str,
) -> Result<String, Fault> {
    let FormatKey::Measures {
        parts,
        separator,
        units,
        unit_once,
    } = entry.format.key
    else {
        return Err(Fault::Shape);
    };
    let mut numbers = Vec::new();
    let mut shown = Vec::new();
    let mut last = None;
    for name in parts {
        let Some(found) = part(value, name) else {
            continue;
        };
        let Value::Quantity(quantity) = found else {
            return Err(Fault::Shape);
        };
        let (unit, places) = unit_for(quantity.kind, units, unit)?;
        if unit_once && last.is_some_and(|(seen, _): (&QuantityUnit, _)| seen != unit) {
            return Err(Fault::Shape);
        }
        let (number, fixed) = present::units::number_in(quantity.amount, unit, places, tag)?;
        if !unit_once {
            shown.push(present::units::with_unit(&number, &fixed, unit, tag)?);
        }
        numbers.push(number);
        last = Some((unit, fixed));
    }
    let Some((unit, fixed)) = last else {
        return Err(Fault::Shape);
    };
    if unit_once {
        return present::units::with_unit(&numbers.join(separator), &fixed, unit, tag);
    }
    Ok(shown.join(separator))
}

/// A dosage instruction in words: its dose, its route's name, how many
/// times in what period, and `when needed` when it is taken only then.
pub fn dosage(record: &Record, tag: &str) -> Result<String, Fault> {
    let (Some(dose), Some(route), Some(Value::Int(times)), Some(Value::Duration(per))) = (
        record.get("dose"),
        record.get("route"),
        record.get("times"),
        record.get("per"),
    ) else {
        return Err(Fault::Shape);
    };
    let shown = |value: &Value| {
        let entry = crate::standard::entry_of(value).ok_or(Fault::Shape)?;
        crate::standard::shown(entry, value, tag, &Value::None, &Value::None)
    };
    let times = if *times == 1 {
        "1 time".to_string()
    } else {
        format!("{} times", present::number(&Value::Int(*times), tag)?)
    };
    let mut text = format!(
        "{}, {}, {times} per {}",
        shown(dose)?,
        shown(route)?,
        present::names::duration(*per, tag)?
    );
    if matches!(record.get("as_needed"), Some(Value::Bool(true))) {
        text.push_str(", when needed");
    }
    Ok(text)
}

