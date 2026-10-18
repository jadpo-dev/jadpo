//! The standard type contract's operations (area 19, "Every standard type
//! meets one contract"): `text.from`, `text.parse` and `text.format`, each
//! one implementation that reads a type's data entry
//! (`jadpo_std::standard`), so a type added later is an entry, not a new
//! code path. `text.from` and `text.parse` also take the representations
//! JSON carries as text (area 19, "Text"): an `Int`'s digits, a `Decimal`
//! with its places, `true` or `false`, and, for `text.parse`, text itself;
//! `text.format` shows an `Int` or a `Decimal` in the locale's digits and
//! separators (area 19, "Numbers").

use jadpo_std::RuleKey;
use jadpo_std::standard::{FormatKey, Holds, StandardType, TextForm, standard_type};

use crate::{Decimal, Fault, Value, Zoned, present, temporal};

pub mod json;

/// What fails when text isn't the type asked for at all.
const MALFORMED: Fault = Fault::Broke(RuleKey::Malformed);

/// The standard type a value is, from what the runtime holds; `None` for
/// text, whose standard types (`Email`, `Url`, `IpAddress`) are their text,
/// and for representations.
pub fn entry_of(value: &Value) -> Option<&'static StandardType> {
    let builtin = match value {
        Value::Uuid(_) => jadpo_std::Builtin::Uuid,
        Value::Instant(_) => jadpo_std::Builtin::Instant,
        Value::Date(_) => jadpo_std::Builtin::CalendarDate,
        Value::Zoned(_) => jadpo_std::Builtin::ZonedTime,
        Value::Duration(_) => jadpo_std::Builtin::Duration,
        Value::Money(_) => jadpo_std::Builtin::Money,
        Value::Quantity(quantity) => jadpo_std::Builtin::Quantity(quantity.kind),
        Value::Percentage(_) => jadpo_std::Builtin::Percentage,
        Value::Variant(variant) => return standard_type(&variant.ty),
        Value::Record(record) => return standard_type(&record.ty),
        _ => return None,
    };
    jadpo_std::standard::of_builtin(builtin)
}

/// `text.from(value, form:)`: the value's canonical text, or its code in
/// the code system `form` names (a `CodeForm` variant, or `none` for the
/// type's own), which the checker has made sure the type has data for.
pub fn from_in(value: &Value, form: &Value) -> Result<Value, Fault> {
    let Some(form) = code_form(form)? else {
        return from(value);
    };
    let Value::Variant(variant) = value else {
        return Err(Fault::Shape);
    };
    let code = standard_type(&variant.ty)
        .and_then(|entry| entry.form_codes(form))
        .and_then(|codes| codes.get(variant.ordinal as usize))
        .ok_or(Fault::Shape)?;
    Ok(Value::text(code))
}

/// The code system a `form:` argument names, `None` for `none`.
fn code_form(form: &Value) -> Result<Option<jadpo_std::CodeForm>, Fault> {
    match form {
        Value::None => Ok(None),
        Value::Variant(variant) if &*variant.ty == jadpo_std::CodeForm::NAME => {
            jadpo_std::CodeForm::from_name(&variant.name)
                .map(Some)
                .ok_or(Fault::Shape)
        }
        _ => Err(Fault::Shape),
    }
}

/// `text.from(value)`: the value's canonical text.
pub fn from(value: &Value) -> Result<Value, Fault> {
    let text = match (entry_of(value), value) {
        (Some(entry), value) => canonical(entry, value)?,
        (None, Value::Int(n)) => n.to_string(),
        (None, Value::Decimal(d)) => d.to_string(),
        (None, Value::Bool(flag)) => flag.to_string(),
        // `Email`, `Url` and `IpAddress` are their text.
        (None, Value::Text(text)) => text.to_string(),
        // A type built on `Bytes` (`Hash`, `EncryptedValue`) is its Base64,
        // as JSON sends it (chunk 5.5r3).
        (None, Value::Bytes(bytes)) => crate::encoding::to_base64(bytes),
        _ => return Err(Fault::Shape),
    };
    Ok(Value::text(&text))
}

/// A standard type's value as its canonical text, as its entry's text form
/// writes it.
pub fn canonical(entry: &StandardType, value: &Value) -> Result<String, Fault> {
    let form = entry.text.ok_or(Fault::Shape)?;
    // A secret value has no text a program reads back (chunk 5.5r2).
    if form == TextForm::Secret {
        return Err(Fault::Shape);
    }
    if let Some(text) = crate::calendar::canonical(form, value) {
        return text;
    }
    Ok(match (form, value) {
        (TextForm::Checked | TextForm::Html, Value::Text(text)) => text.to_string(),
        (TextForm::Base64, Value::Bytes(bytes)) => crate::encoding::to_base64(bytes),
        // A certificate's DER as one PEM block (chunk 5.5r5).
        (TextForm::Pem, Value::Record(record)) => crate::certificate::pem_text(record)?,
        (TextForm::Uuid, Value::Uuid(text)) => text.to_ascii_lowercase(),
        // A latitude, a longitude, a bearing, with its places (chunk 5.5s).
        (TextForm::Decimal, Value::Decimal(decimal)) => decimal.to_string(),
        (TextForm::Instant, Value::Instant(millis)) => jadpo_std::format_instant(*millis),
        (TextForm::Date, Value::Date(day)) => jadpo_std::format_date(*day),
        (TextForm::Zoned, Value::Zoned(zoned)) => zoned.to_string(),
        (TextForm::Duration, Value::Duration(millis)) => jadpo_std::iso_duration(*millis),
        (TextForm::Money, Value::Money(money)) => money.to_string(),
        (TextForm::Quantity, Value::Quantity(quantity)) => {
            jadpo_std::quantity::canonical(quantity.kind, quantity.amount)
        }
        (TextForm::ExchangeRate, Value::Record(record)) => exchange_rate_text(record)?,
        (TextForm::Percentage, Value::Percentage(fraction)) => {
            jadpo_std::percentage::canonical(fraction.mantissa, fraction.scale)
        }
        (TextForm::Ratio, Value::Record(record)) => {
            match (record.get("numerator"), record.get("denominator")) {
                (Some(Value::Decimal(top)), Some(Value::Decimal(bottom))) => {
                    format!("{top}:{bottom}")
                }
                _ => return Err(Fault::Shape),
            }
        }
        (TextForm::Fraction, Value::Record(record)) => {
            match (record.get("numerator"), record.get("denominator")) {
                (Some(Value::Int(top)), Some(Value::Int(bottom))) => {
                    jadpo_std::percentage::fraction_text(*top, *bottom)
                }
                _ => return Err(Fault::Shape),
            }
        }
        (TextForm::Mailbox, Value::Record(record)) => crate::mailbox::mailbox_text(record)?,
        (TextForm::CardExpiry, Value::Record(record)) => {
            match (record.get("year"), record.get("month")) {
                (Some(Value::Int(year)), Some(Value::Int(month))) => {
                    format!("{year:04}-{month:02}")
                }
                _ => return Err(Fault::Shape),
            }
        }
        (TextForm::VatNumber, Value::Record(record)) => {
            match (record.get("country"), record.get("number")) {
                (Some(Value::Variant(country)), Some(Value::Text(number))) => {
                    let code = standard_type(&country.ty)
                        .and_then(StandardType::codes)
                        .and_then(|codes| codes.get(country.ordinal as usize))
                        .map(|(_, code)| *code)
                        .ok_or(Fault::Shape)?;
                    format!(
                        "{}{number}",
                        jadpo_std::standard::documents::vat_prefix(code)
                    )
                }
                _ => return Err(Fault::Shape),
            }
        }
        (TextForm::Event, Value::Record(record)) => crate::event::event_text(record)?,
        (TextForm::Whole, Value::Int(n)) => n.to_string(),
        (TextForm::UtmQuery, Value::Record(record)) => crate::web::utm_text(record)?,
        (TextForm::SemanticVersion, Value::Record(record)) => crate::semver::version_text(record)?,
        // A colour's hex, its parts' numbers joined, a rating's code and
        // category, a cue (chunk 5.5p).
        (TextForm::Colour, Value::Record(record)) => {
            let channel = |name: &str| match record.get(name) {
                Some(Value::Int(n)) => u8::try_from(*n).map_err(|_| Fault::Shape),
                _ => Err(Fault::Shape),
            };
            jadpo_std::standard::colour::hex(
                channel("red")?,
                channel("green")?,
                channel("blue")?,
                channel("alpha")?,
            )
        }
        (TextForm::Wholes(separator) | TextForm::Decimals(separator), Value::Record(record)) => {
            crate::media::numbers_text(entry, record, separator)?
        }
        (TextForm::CodeThenText, Value::Record(record)) => {
            crate::media::code_then_text(entry, record)?
        }
        // Two parts joined, a flight's designator (chunk 5.5m2).
        (TextForm::Joined(separator), Value::Record(record)) => {
            crate::media::joined_text(entry, record, separator)?
        }
        (TextForm::FlightNumber, Value::Record(record)) => crate::measures::flight_text(record)?,
        (TextForm::Cue, Value::Record(record)) => crate::media::cue_text(record)?,
        (TextForm::Code, Value::Variant(variant)) => entry
            .codes()
            .and_then(|codes| codes.get(variant.ordinal as usize))
            .map(|(_, code)| (*code).to_string())
            .ok_or(Fault::Shape)?,
        (TextForm::Interval, Value::Record(record)) => {
            let (start, end) = interval(entry, record)?;
            format!("{}/{}", text_of(from(start)?)?, text_of(from(end)?)?)
        }
        (TextForm::Range, Value::Record(record)) => {
            let (start, end) = range_ends(record)?;
            let text = |end: Option<&Value>| match end {
                Some(value) => text_of(from(value)?),
                None => Ok(jadpo_std::standard::OPEN_END.to_string()),
            };
            format!("{}/{}", text(start)?, text(end)?)
        }
        _ => return Err(Fault::Shape),
    })
}

/// A range's two ends, `None` for an open one.
fn range_ends(record: &crate::Record) -> Result<(Option<&Value>, Option<&Value>), Fault> {
    let [start, end] = jadpo_std::RANGE_FIELDS;
    let end_of = |name: &str| match record.get(name) {
        Some(Value::None) => Ok(None),
        Some(value) => Ok(Some(value)),
        None => Err(Fault::Shape),
    };
    Ok((end_of(start)?, end_of(end)?))
}

/// `Range<T>` read from its canonical text, each end as `item` (the
/// standard type or representation `T` is built on), `..` for an open
/// end. A start after its end breaks its rule `start_not_after_end`, which
/// the range is checked by once it is read (chunk 5.5t2).
pub fn read_range(raw: &str, item: &str) -> Result<Value, Fault> {
    let (from, to) = raw.split_once('/').ok_or(MALFORMED)?;
    let end = |text: &str| -> Result<Option<Value>, Fault> {
        if text == jadpo_std::standard::OPEN_END {
            return Ok(None);
        }
        match parse(text, item) {
            Ok(value) => Ok(Some(value)),
            Err(Fault::Broke(_)) => Err(MALFORMED),
            Err(other) => Err(other),
        }
    };
    let (start, end) = (end(from)?, end(to)?);
    Ok(crate::range::range_of(start, end))
}

/// A compound standard type's value as the columns its entry lists hold
/// it (area 19; laid out by `jadpo_std::standard::storage`, chunk 5.5w):
/// `Money`'s minor units and currency code, a `ZonedTime`'s instant and
/// zone, an object's parts in its fields' order (chunk 5.5l), each in as
/// many columns as it takes, a choice's variant's name and then each field
/// its variants use, `none` where the stored variant hasn't it, and a type
/// stored natively as its one column's value: a range as its ends and
/// whether it holds its end, a `LocalTime` as microseconds since midnight,
/// a `LocalDateTime` as microseconds since 1970-01-01T00:00:00 on no zone.
/// A standard enum is its code, and another standard type stored in one
/// column as it is stored there. `None` for a type stored in one column of
/// its own.
pub fn to_parts(value: &Value) -> Option<Vec<Value>> {
    match value {
        Value::Money(money) => Some(vec![Value::Int(money.minor), Value::text(money.code())]),
        Value::Zoned(zoned) => Some(vec![
            Value::Instant(zoned.millis),
            Value::text(zoned.zone_name()),
        ]),
        Value::Record(record) => {
            let entry = standard_type(&record.ty)?;
            if let Some(native) = native_part(entry, value) {
                return Some(vec![native?]);
            }
            let Holds::Object(fields) = entry.holds else {
                return None;
            };
            if entry.columns().is_empty() {
                return None;
            }
            let mut out = Vec::new();
            for (name, part) in fields {
                out.extend(part_values(record.get(name)?, *part)?);
            }
            Some(out)
        }
        Value::Variant(variant) => {
            let entry = standard_type(&variant.ty)?;
            let Holds::Choice(variants) = entry.holds else {
                return None;
            };
            if entry.columns().is_empty() {
                return None;
            }
            let mut out = vec![Value::text(&variant.name)];
            for (name, part) in jadpo_std::standard::storage::choice_fields(variants) {
                match variant.get(name) {
                    Some(field) => out.extend(part_values(
                        field,
                        own_part(variants, &variant.name, name).unwrap_or(part),
                    )?),
                    None => out.extend(std::iter::repeat_n(
                        Value::None,
                        jadpo_std::standard::storage::width(part),
                    )),
                }
            }
            Some(out)
        }
        _ => None,
    }
}

/// The part a choice's variant `variant` declares its field `field` as.
fn own_part(
    variants: &[(&str, &[(&str, jadpo_std::Part)])],
    variant: &str,
    field: &str,
) -> Option<jadpo_std::Part> {
    variants
        .iter()
        .find(|(name, _)| *name == variant)?
        .1
        .iter()
        .find(|(name, _)| *name == field)
        .map(|(_, part)| *part)
}

/// The value a type stored natively in one column (`[("", ...)]`) has
/// there; `None` for any other type, `Some(None)` for a value that isn't
/// one.
fn native_part(entry: &StandardType, value: &Value) -> Option<Option<Value>> {
    let [("", sql_type)] = entry.columns() else {
        return None;
    };
    let Value::Record(record) = value else {
        return Some(None);
    };
    let base = sql_type.split('(').next().unwrap_or(sql_type);
    Some(match base {
        "time" => local_micros(value),
        "timestamp" => match (record.get("date"), record.get("time")) {
            (Some(Value::Date(day)), Some(time)) => i64::from(*day)
                .checked_mul(MICROS_A_DAY)
                .and_then(|day| match local_micros(time)? {
                    Value::Int(micros) => day.checked_add(micros),
                    _ => None,
                })
                .map(Value::Int),
            _ => None,
        },
        "daterange" | "tstzrange" | "range" => {
            let start = record.get("start")?.clone();
            let end = record.get("end")?.clone();
            let holds_end = jadpo_std::range_type(&record.ty).is_some_and(|(_, held)| held);
            Some(Value::list(vec![start, end, Value::Bool(holds_end)]))
        }
        // An object of one part (`Year`, `UtcOffset`): that part.
        _ => match entry.holds {
            Holds::Object([(name, _)]) => record.get(name).cloned(),
            _ => None,
        },
    })
}

/// Microseconds in a day.
const MICROS_A_DAY: i64 = 86_400_000_000;

/// A `LocalTime` as microseconds since midnight.
fn local_micros(time: &Value) -> Option<Value> {
    let Value::Record(time) = time else {
        return None;
    };
    let part = |name: &str| match time.get(name) {
        Some(Value::Int(n)) => Some(*n),
        _ => None,
    };
    let seconds = part("hour")? * 3600 + part("minute")? * 60 + part("second")?;
    Some(Value::Int(seconds * 1_000_000))
}

/// A `LocalTime` from microseconds since midnight.
fn local_time(micros: i64) -> Result<Value, Fault> {
    if !(0..MICROS_A_DAY).contains(&micros) || micros % 1_000_000 != 0 {
        return Err(Fault::Shape);
    }
    let seconds = micros / 1_000_000;
    Ok(Value::record(
        "LocalTime",
        vec![
            ("hour".into(), Value::Int(seconds / 3600)),
            ("minute".into(), Value::Int(seconds / 60 % 60)),
            ("second".into(), Value::Int(seconds % 60)),
        ],
    ))
}

/// The columns' values of one part of a standard object or choice.
fn part_values(value: &Value, part: jadpo_std::Part) -> Option<Vec<Value>> {
    use jadpo_std::Part;
    match (part, value) {
        (Part::Optional(inner), Value::None) => {
            Some(vec![
                Value::None;
                jadpo_std::standard::storage::width(*inner)
            ])
        }
        (Part::Optional(inner), value) => part_values(value, *inner),
        (Part::List(inner), Value::List(items)) => Some(vec![Value::list(
            items
                .iter()
                .map(|item| match part_values(item, *inner)?.as_slice() {
                    [one] => Some(one.clone()),
                    _ => None,
                })
                .collect::<Option<Vec<Value>>>()?,
        )]),
        (_, Value::Money(_) | Value::Zoned(_)) => to_parts(value),
        // A standard enum, as its code.
        (_, Value::Variant(variant)) if variant.fields.is_empty() => {
            match standard_type(&variant.ty).and_then(StandardType::codes) {
                Some(codes) => codes
                    .get(variant.ordinal as usize)
                    .map(|(_, code)| vec![Value::text(code)]),
                None => to_parts(value),
            }
        }
        (_, Value::Variant(_)) => to_parts(value),
        // Another standard object: its own columns, or its one column: a
        // geometry's `geography` (a circle's centre, area 20, chunk 5.5u4)
        // as the geometry itself.
        (_, Value::Record(record)) => match standard_type(&record.ty) {
            Some(other) if !other.columns().is_empty() => to_parts(value),
            Some(other) if other.storage == jadpo_std::standard::Storage::Geography => {
                Some(vec![value.clone()])
            }
            Some(_) => Some(vec![from(value).ok()?]),
            None => None,
        },
        (_, other) => Some(vec![other.clone()]),
    }
}

/// A compound standard type's value from its columns' values, in the order
/// its entry lists them, as [`to_parts`] gives them.
pub fn from_parts(entry: &StandardType, parts: &[Value]) -> Result<Value, Fault> {
    from_columns(entry, None, parts)
}

/// As [`from_parts`], for the type `ty` names, which may be a range of an
/// item type (`Range<Duration>`), whose ends need it.
pub fn from_parts_of(ty: &str, parts: &[Value]) -> Result<Value, Fault> {
    let item = range_item(ty);
    let name = if item.is_some() { jadpo_std::RANGE } else { ty };
    let entry = standard_type(name).ok_or(Fault::Shape)?;
    from_columns(entry, item, parts)
}

fn from_columns(entry: &StandardType, item: Option<&str>, parts: &[Value]) -> Result<Value, Fault> {
    if let [("", _)] = entry.columns() {
        let [part] = parts else {
            return Err(Fault::Shape);
        };
        return from_native(entry, item, part);
    }
    match (entry.holds, parts) {
        (Holds::Builtin(jadpo_std::Builtin::Money), [Value::Int(minor), Value::Text(code)]) => {
            let currency = jadpo_std::money::ordinal_of_code(code.trim()).ok_or(Fault::Shape)?;
            Ok(crate::money::value(
                currency,
                crate::money::minor(i128::from(*minor))?,
            ))
        }
        (
            Holds::Builtin(jadpo_std::Builtin::ZonedTime),
            [Value::Instant(millis), Value::Text(zone)],
        ) => {
            let zone = jadpo_std::ZONES
                .iter()
                .position(|(_, name)| *name == &**zone)
                .ok_or(Fault::Shape)?;
            Ok(Value::Zoned(Zoned {
                millis: *millis,
                zone: u32::try_from(zone).map_err(|_| Fault::Shape)?,
            }))
        }
        (Holds::Object(fields), parts) => {
            let mut rest = parts;
            let mut values = Vec::new();
            for (name, part) in fields {
                let (value, after) = take_part(*part, rest)?;
                values.push(((*name).into(), value));
                rest = after;
            }
            if !rest.is_empty() {
                return Err(Fault::Shape);
            }
            Ok(Value::record(entry.name, values))
        }
        (Holds::Choice(variants), [Value::Text(kind), rest @ ..]) => {
            let (ordinal, (variant, fields)) = variants
                .iter()
                .enumerate()
                .find(|(_, (name, _))| *name == &**kind)
                .ok_or(Fault::Shape)?;
            // Where each stored field starts among the columns after the
            // kind.
            let mut at = 0;
            let mut starts = Vec::new();
            for (name, part) in jadpo_std::standard::storage::choice_fields(variants) {
                starts.push((name, at));
                at += jadpo_std::standard::storage::width(part);
            }
            if at != rest.len() {
                return Err(Fault::Shape);
            }
            let mut values = Vec::new();
            for (name, part) in *fields {
                let start = starts
                    .iter()
                    .find(|(field, _)| field == name)
                    .map(|(_, start)| *start)
                    .ok_or(Fault::Shape)?;
                let (value, _) = take_part(*part, rest.get(start..).ok_or(Fault::Shape)?)?;
                values.push(((*name).into(), value));
            }
            Ok(Value::variant(
                entry.name,
                u32::try_from(ordinal).map_err(|_| Fault::Shape)?,
                variant,
                values,
            ))
        }
        _ => Err(Fault::Shape),
    }
}

/// The value of a type stored natively in one column, from that column's
/// value as [`native_part`] gives it.
fn from_native(entry: &StandardType, item: Option<&str>, part: &Value) -> Result<Value, Fault> {
    let [("", sql_type)] = entry.columns() else {
        return Err(Fault::Shape);
    };
    let base = sql_type.split('(').next().unwrap_or(sql_type);
    match (base, part) {
        ("time", Value::Int(micros)) => local_time(*micros),
        ("timestamp", Value::Int(micros)) => {
            let day = micros.div_euclid(MICROS_A_DAY);
            Ok(Value::record(
                entry.name,
                vec![
                    (
                        "date".into(),
                        Value::Date(i32::try_from(day).map_err(|_| Fault::Shape)?),
                    ),
                    ("time".into(), local_time(micros.rem_euclid(MICROS_A_DAY))?),
                ],
            ))
        }
        ("daterange" | "tstzrange" | "range", Value::List(ends)) => {
            let [start, end, Value::Bool(held)] = &ends[..] else {
                return Err(Fault::Shape);
            };
            let holds_end = jadpo_std::range_type(entry.name).is_some_and(|(_, held)| held);
            let end_of = |value: &Value| -> Result<Value, Fault> {
                Ok(match (item, value) {
                    (Some("Duration"), Value::Int(millis)) => Value::Duration(*millis),
                    _ => value.clone(),
                })
            };
            let (start, mut end) = (end_of(start)?, end_of(end)?);
            // A range held up to and including its end (`DateRange`) comes
            // back from PostgreSQL's discrete form, which holds up to its
            // next day, not including it.
            if holds_end && !held {
                end = match end {
                    Value::Date(day) => Value::Date(day - 1),
                    other => other,
                };
            }
            if entry.name == jadpo_std::RANGE {
                let open = |value: Value| (!matches!(value, Value::None)).then_some(value);
                return Ok(crate::range::range_of(open(start), open(end)));
            }
            Ok(Value::record(
                entry.name,
                vec![("start".into(), start), ("end".into(), end)],
            ))
        }
        (_, value) => match entry.holds {
            Holds::Object([(name, _)]) => Ok(Value::record(
                entry.name,
                vec![((*name).into(), value.clone())],
            )),
            _ => Err(Fault::Shape),
        },
    }
}

/// One part's value from the columns at the front of `parts`, and the
/// columns after it.
fn take_part(part: jadpo_std::Part, parts: &[Value]) -> Result<(Value, &[Value]), Fault> {
    use jadpo_std::{Builtin, Part};
    let width = jadpo_std::standard::storage::width(part);
    let (mine, rest) = parts.split_at_checked(width).ok_or(Fault::Shape)?;
    let (part, may_be_none) = part.required();
    if may_be_none && mine.iter().all(|value| matches!(value, Value::None)) {
        return Ok((Value::None, rest));
    }
    let one = || match mine {
        [one] => Ok(one),
        _ => Err(Fault::Shape),
    };
    let value = match part {
        Part::Of(builtin) | Part::Bounded { of: builtin, .. } => match (builtin, mine) {
            (Builtin::Money, _) => from_parts(
                jadpo_std::standard::of_builtin(Builtin::Money).ok_or(Fault::Shape)?,
                mine,
            )?,
            (builtin, [value]) => as_builtin(builtin, value)?,
            _ => return Err(Fault::Shape),
        },
        Part::List(inner) => match one()? {
            Value::List(items) => Value::list(
                items
                    .iter()
                    .map(|item| {
                        take_part(*inner, std::slice::from_ref(item)).map(|(value, _)| value)
                    })
                    .collect::<Result<Vec<Value>, Fault>>()?,
            ),
            _ => return Err(Fault::Shape),
        },
        Part::Standard(name) => {
            let other = standard_type(name).ok_or(Fault::Shape)?;
            if !other.columns().is_empty() {
                from_parts(other, mine)?
            } else {
                match (other.holds, one()?) {
                    (Holds::Builtin(builtin) | Holds::Bounded { of: builtin, .. }, value) => {
                        as_builtin(builtin, value)?
                    }
                    (_, Value::Text(code)) => read(other, code.trim()).map_err(|_| Fault::Shape)?,
                    (_, value) => value.clone(),
                }
            }
        }
        Part::Whole(..) | Part::Line | Part::Matching(_) | Part::Paragraph | Part::Float => {
            one()?.clone()
        }
        Part::Optional(_) => return Err(Fault::Shape),
    };
    Ok((value, rest))
}

/// A column's value as a value built on `builtin`: a quantity from its
/// number in the base unit, a percentage from its fraction, a duration
/// from its milliseconds.
fn as_builtin(builtin: jadpo_std::Builtin, value: &Value) -> Result<Value, Fault> {
    use jadpo_std::Builtin;
    Ok(match (builtin, value) {
        (Builtin::Quantity(kind), Value::Decimal(number)) => {
            let amount = number
                .rescale(kind.scale(), jadpo_std::Rounding::HalfEven)
                .ok_or(Fault::Shape)?;
            Value::Quantity(crate::Quantity {
                kind,
                amount: amount.mantissa,
            })
        }
        (Builtin::Percentage, Value::Decimal(fraction)) => Value::Percentage(*fraction),
        (Builtin::Decimal, Value::Int(n)) => Value::Decimal(Decimal::from_int(*n)),
        (Builtin::Duration, Value::Int(millis)) => Value::Duration(*millis),
        (_, value) => value.clone(),
    })
}

/// The item type a range type's name gives it: `Int` for `Range<Int>`.
fn range_item(ty: &str) -> Option<&str> {
    ty.strip_prefix(jadpo_std::RANGE)?
        .strip_prefix('<')?
        .strip_suffix('>')
}

/// An interval's two ends, by its entry's fields in order: an
/// `InstantRange`'s instants, a `DateRange`'s dates.
fn interval<'v>(
    entry: &StandardType,
    record: &'v crate::Record,
) -> Result<(&'v Value, &'v Value), Fault> {
    let Holds::Object([(start, _), (end, _)]) = entry.holds else {
        return Err(Fault::Shape);
    };
    match (record.get(start), record.get(end)) {
        (Some(start @ Value::Instant(_)), Some(end @ Value::Instant(_)))
        | (Some(start @ Value::Date(_)), Some(end @ Value::Date(_))) => Ok((start, end)),
        _ => Err(Fault::Shape),
    }
}

/// `text.parse(raw, as:, form:, country:)`: as [`parse`]; given a
/// `CodeForm` variant, a closed list's variant by its code in that code
/// system, ignoring case, a code a retired variant shares with a current
/// one read as the current one; given a `Country`, a phone number or a
/// postal code as people write it there (chunk 5.5l2).
pub fn parse_in(raw: &str, ty: &str, form: &Value, country: &Value) -> Result<Value, Fault> {
    if let Value::Variant(country) = country {
        return written_in(raw, ty, country);
    }
    let Some(form) = code_form(form)? else {
        return parse(raw, ty);
    };
    // A form read but never written: an `Isbn` from an ISBN-10 (chunk
    // 5.5m2), `malformed` for a wrong check character.
    if jadpo_std::standard::reads_form(ty, form) {
        return jadpo_std::standard::isbn_from_isbn10(raw)
            .map(|isbn| Value::text(&isbn))
            .ok_or(MALFORMED);
    }
    let entry = standard_type(ty).ok_or(Fault::Shape)?;
    let (Some(codes), Some(forms)) = (entry.codes(), entry.form_codes(form)) else {
        return Err(Fault::Shape);
    };
    let matching = |current: bool| {
        forms.iter().enumerate().find(|(at, code)| {
            !code.is_empty()
                && code.eq_ignore_ascii_case(raw)
                && codes
                    .get(*at)
                    .is_some_and(|(name, _)| entry.is_retired(name) != current)
        })
    };
    let (at, _) = matching(true)
        .or_else(|| matching(false))
        .ok_or(MALFORMED)?;
    let (name, _) = codes.get(at).ok_or(Fault::Shape)?;
    Ok(Value::variant(
        entry.name,
        u32::try_from(at).map_err(|_| Fault::Shape)?,
        name,
        Vec::new(),
    ))
}

/// A phone number or a postal code as people write it in `country`, a
/// `Country` variant: its canonical text, or `malformed` where it isn't
/// one of the country's.
fn written_in(raw: &str, ty: &str, country: &crate::Variant) -> Result<Value, Fault> {
    if &*country.ty != jadpo_std::standard::places::COUNTRY {
        return Err(Fault::Shape);
    }
    let code = standard_type(&country.ty)
        .and_then(StandardType::codes)
        .and_then(|codes| codes.get(country.ordinal as usize))
        .map(|(_, code)| *code)
        .ok_or(Fault::Shape)?;
    let read = match ty {
        "PhoneNumber" => jadpo_std::standard::phones::from_written(raw, code),
        "PostalCode" => jadpo_std::standard::postal::from_written(raw, code),
        _ => return Err(Fault::Shape),
    };
    read.map(|text| Value::text(&text)).ok_or(MALFORMED)
}

/// `text.parse(raw, as:)`, given the name of the standard type or the
/// representation whose canonical text `raw` is read as; the rules of the
/// type `as:` names are checked by its construction after this. Text that
/// isn't that type at all fails `malformed`.
pub fn parse(raw: &str, ty: &str) -> Result<Value, Fault> {
    if let Some(item) = range_item(ty) {
        return read_range(raw, item);
    }
    if let Some(entry) = standard_type(ty) {
        return read(entry, raw);
    }
    match ty {
        "Text" => Ok(Value::text(raw)),
        "Int" => int(raw).map(Value::Int).ok_or(MALFORMED),
        "Decimal" => decimal(raw).map(Value::Decimal).ok_or(MALFORMED),
        "Bool" => match raw {
            "true" => Ok(Value::Bool(true)),
            "false" => Ok(Value::Bool(false)),
            _ => Err(MALFORMED),
        },
        _ => Err(Fault::Shape),
    }
}

/// An `Int`'s digits, with a `-` when negative and nothing else.
fn int(raw: &str) -> Option<i64> {
    let digits = raw.strip_prefix('-').unwrap_or(raw);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    raw.parse().ok()
}

/// A `Decimal` as JSON carries it: digits with an optional `-` and
/// places.
pub(crate) fn decimal(raw: &str) -> Option<Decimal> {
    let digits = raw.strip_prefix('-').unwrap_or(raw);
    let (whole, places) = digits.split_once('.').unwrap_or((digits, "0"));
    let plain = |part: &str| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit());
    if !plain(whole) || !plain(places) {
        return None;
    }
    Decimal::parse(raw)
}

/// An `Instant`'s canonical text: ISO 8601 ending in `Z`, never with an
/// offset (area 19, "Chosen in chunk 5.5a"), as every boundary reads one.
pub(crate) fn instant(raw: &str) -> Option<i64> {
    jadpo_std::instant_millis(raw).filter(|_| raw.ends_with('Z'))
}

/// The one reader of a built-in type's text, which `text.parse`, a JSON
/// string, a query string, a path, a header, a setting and a map's key all
/// read through, so none is looser than another (chunk 5.5w2): an `Int` as
/// digits with a `-` when negative, a `Decimal` with places too, a `Bool`
/// as `true` or `false`, `Bytes` in Base64, and a built-in standard type
/// (`Instant`, `Money`, a quantity, `ZonedTime`...) by its entry's
/// canonical text. Text is the caller's, which composes it first. `None`
/// for text that isn't one, or a type with no single text form.
pub fn builtin_text(builtin: jadpo_std::Builtin, raw: &str) -> Option<Value> {
    use jadpo_std::Builtin;
    match builtin.representation() {
        Builtin::Int => int(raw).map(Value::Int),
        Builtin::Decimal => decimal(raw).map(Value::Decimal),
        Builtin::Bool => match raw {
            "true" => Some(Value::Bool(true)),
            "false" => Some(Value::Bool(false)),
            _ => None,
        },
        Builtin::Bytes => {
            crate::encoding::from_base64(raw).map(|bytes| Value::Bytes(std::rc::Rc::from(bytes)))
        }
        Builtin::Text | Builtin::PresentationText => Some(Value::text(raw)),
        representation => {
            jadpo_std::standard::of_builtin(representation).and_then(|entry| read(entry, raw).ok())
        }
    }
}

/// A value of a type built on `builtin` read from its text in a place
/// that holds text, before its rules are checked: a JSON string, a path, a
/// query string, a header, a setting or a map's key. Text is composed to
/// NFC first, so text that looks equal is equal and its rules count the
/// code points it will be kept as (areas 2, 7 and 19); every other type is
/// read by the one reader `text.parse` reads it with ([`builtin_text`]),
/// so no boundary is looser than another (chunk 5.5w2). Text that isn't a
/// number or a `Bool` breaks `type`; any other unreadable text `format`.
pub fn scalar_text(builtin: jadpo_std::Builtin, raw: &str) -> Result<Value, &'static str> {
    use jadpo_std::Builtin;
    match builtin.representation() {
        Builtin::Text => Ok(Value::text(&crate::text::composed(raw))),
        // Text for people is sent and read as the text it is (area 10).
        Builtin::PresentationText => Ok(Value::text(raw)),
        representation => builtin_text(builtin, raw).ok_or(match representation {
            Builtin::Int | Builtin::Decimal | Builtin::Bool => "type",
            _ => "format",
        }),
    }
}

/// An ISO 8601 duration of days, hours, minutes and seconds (`PT5M`,
/// `P1DT2H`, `PT0.5S`), negative with a leading `-` as one is written
/// (`-PT5M`), in milliseconds.
pub(crate) fn iso_duration(text: &str) -> Option<i64> {
    if let Some(positive) = text.strip_prefix('-') {
        return iso_duration(positive.strip_prefix('P').map(|_| positive)?)?.checked_neg();
    }
    let rest = text.strip_prefix('P')?;
    let (date_part, time) = match rest.split_once('T') {
        Some((date_part, time)) => (date_part, time),
        None => (rest, ""),
    };
    let mut total: i64 = 0;
    if !date_part.is_empty() {
        let days = date_part.strip_suffix('D')?;
        if days.is_empty() || !days.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let count: i64 = days.parse().ok()?;
        total = total.checked_add(count.checked_mul(86_400_000)?)?;
    }
    let mut number = String::new();
    for c in time.chars() {
        match c {
            '0'..='9' | '.' => number.push(c),
            'H' | 'M' | 'S' => {
                let unit: i64 = match c {
                    'H' => 3_600_000,
                    'M' => 60_000,
                    _ => 1_000,
                };
                let millis = match number.split_once('.') {
                    Some((whole, part)) if c == 'S' => {
                        let whole: i64 = whole.parse().ok()?;
                        let mut part = part.to_string();
                        part.truncate(3);
                        while part.len() < 3 {
                            part.push('0');
                        }
                        whole
                            .checked_mul(1_000)?
                            .checked_add(part.parse::<i64>().ok()?)?
                    }
                    Some(_) => return None,
                    None => number.parse::<i64>().ok()?.checked_mul(unit)?,
                };
                total = total.checked_add(millis)?;
                number.clear();
            }
            _ => return None,
        }
    }
    (number.is_empty() && text.len() > 1 && text != "PT").then_some(total)
}

/// A standard type's value read from its canonical text.
pub fn read(entry: &StandardType, raw: &str) -> Result<Value, Fault> {
    let form = entry.text.ok_or(Fault::Shape)?;
    if let Some(read) = crate::calendar::read(form, raw) {
        return read.ok_or(MALFORMED);
    }
    let read = match form {
        TextForm::Checked => entry.accepts(raw).then(|| Value::text(raw)),
        // A secret value, held so that it is never written out (chunk
        // 5.5r2).
        TextForm::Secret => entry.accepts(raw).then(|| Value::secret(entry.name, raw)),
        // A type built on `Bytes`, its bytes passing its check (chunk
        // 5.5r3).
        TextForm::Base64 => crate::encoding::from_base64(raw)
            .filter(|bytes| {
                entry
                    .validator
                    .is_some_and(|validator| validator.accepts_bytes(bytes))
            })
            .map(|bytes| Value::Bytes(std::rc::Rc::from(bytes))),
        // An X.509 certificate, its parts read from its DER (chunk
        // 5.5r5).
        TextForm::Pem => crate::certificate::read_pem(raw),
        TextForm::Uuid => entry
            .validator
            .is_some_and(|validator| validator.accepts(raw))
            .then(|| Value::uuid(raw)),
        TextForm::Instant => instant(raw).map(Value::Instant),
        TextForm::Date => jadpo_std::date_number(raw)
            .filter(|_| jadpo_std::check_calendar_date(raw).is_ok())
            .map(Value::Date),
        TextForm::Zoned => zoned(raw).map(Value::Zoned),
        TextForm::Duration => iso_duration(raw).map(Value::Duration),
        TextForm::Code => code(entry, raw),
        // Its bounds are its construction's, as a `TaxRate`'s are.
        TextForm::Decimal => decimal(raw).map(Value::Decimal),
        TextForm::Money => crate::money::read(raw).map(Value::Money),
        TextForm::Quantity => {
            // A quantity, or a type built on one with bounds of its own (a
            // heart rate, chunk 5.5m2), whose bounds its construction keeps.
            let (Holds::Builtin(jadpo_std::Builtin::Quantity(kind))
            | Holds::Bounded {
                of: jadpo_std::Builtin::Quantity(kind),
                ..
            }) = entry.holds
            else {
                return Err(Fault::Shape);
            };
            return match jadpo_std::quantity::parse_text(kind, raw) {
                Ok(amount) => Ok(Value::Quantity(crate::Quantity { kind, amount })),
                Err(jadpo_std::quantity::TextProblem::Malformed) => Err(MALFORMED),
                Err(jadpo_std::quantity::TextProblem::UnknownUnit) => {
                    Err(Fault::Broke(RuleKey::UnknownUnit))
                }
            };
        }
        TextForm::ExchangeRate => read_exchange_rate(raw),
        TextForm::Percentage => jadpo_std::percentage::parse_text(raw)
            .map(|fraction| Value::Percentage(Decimal::new(fraction.mantissa, fraction.scale))),
        TextForm::Ratio => jadpo_std::percentage::parse_ratio(raw).map(|(top, bottom)| {
            Value::record(
                entry.name,
                vec![
                    (
                        "numerator".into(),
                        Value::Decimal(Decimal::new(top.mantissa, top.scale)),
                    ),
                    (
                        "denominator".into(),
                        Value::Decimal(Decimal::new(bottom.mantissa, bottom.scale)),
                    ),
                ],
            )
        }),
        TextForm::Fraction => jadpo_std::percentage::parse_fraction(raw).map(|(top, bottom)| {
            Value::record(
                entry.name,
                vec![
                    ("numerator".into(), Value::Int(top)),
                    ("denominator".into(), Value::Int(bottom)),
                ],
            )
        }),
        TextForm::Mailbox => crate::mailbox::read_mailbox(entry, raw),
        TextForm::CardExpiry => read_card_expiry(entry, raw),
        TextForm::VatNumber => read_vat_number(entry, raw),
        TextForm::Event => crate::event::read_event(raw),
        // Digits within its range (chunk 5.5n): `0200` and `600` aren't an
        // HTTP status at all.
        TextForm::Whole => match entry.holds {
            Holds::Whole { min, max } if jadpo_std::standard::web::whole_in(raw, min, max) => {
                raw.parse().ok().map(Value::Int)
            }
            _ => None,
        },
        TextForm::UtmQuery => crate::web::read_utm(raw),
        TextForm::SemanticVersion => crate::semver::read_version(raw),
        // Any text is `Html` once it is made safe (chunk 5.5o).
        TextForm::Html => Some(Value::text(&jadpo_std::standard::markup::sanitise(raw))),
        TextForm::Colour => jadpo_std::standard::colour::read_hex(raw)
            .map(|rgba| crate::colour::colour_value(rgba.map(i64::from))),
        TextForm::Wholes(separator) | TextForm::Decimals(separator) => {
            crate::media::read_numbers(entry, raw, separator)
        }
        TextForm::CodeThenText => crate::media::read_code_then_text(entry, raw),
        TextForm::Joined(separator) => crate::media::read_joined(entry, raw, separator),
        TextForm::FlightNumber => crate::measures::read_flight(entry, raw),
        TextForm::Cue => crate::media::read_cue(entry, raw),
        // Its ends' type is the range type's argument, which `parse` reads.
        TextForm::Range => return Err(Fault::Shape),
        // The time group's forms, which `calendar` reads above.
        TextForm::LocalTime
        | TextForm::LocalDateTime
        | TextForm::YearMonth
        | TextForm::Year
        | TextForm::Quarter
        | TextForm::IsoWeek
        | TextForm::UtcOffset
        | TextForm::Period => return Err(Fault::Shape),
        TextForm::Interval => {
            let Holds::Object([(start, ends), (end, _)]) = entry.holds else {
                return Err(Fault::Shape);
            };
            // Each end in its own canonical text: an instant ending in `Z`,
            // or a date.
            let end_of = |text: &str| match ends.builtin() {
                Some(jadpo_std::Builtin::Instant) => instant(text).map(Value::Instant),
                Some(jadpo_std::Builtin::CalendarDate) => jadpo_std::date_number(text)
                    .filter(|_| jadpo_std::check_calendar_date(text).is_ok())
                    .map(Value::Date),
                _ => None,
            };
            // A start after its end breaks the range's rule, which it is
            // checked by once it is read (chunk 5.5t2).
            raw.split_once('/').and_then(|(from, to)| {
                let (from, to) = (end_of(from)?, end_of(to)?);
                Some(Value::record(
                    entry.name,
                    vec![((*start).into(), from), ((*end).into(), to)],
                ))
            })
        }
    };
    read.ok_or(MALFORMED)
}

/// An exchange rate's canonical text, `GBP/EUR 1.17 2026-10-18T09:00:00Z`:
/// its pair as codes, its rate with the places it was given, and the
/// instant it was quoted at (chosen in chunk 5.5h2).
fn exchange_rate_text(record: &crate::Record) -> Result<String, Fault> {
    let code = |name: &str| match record.get(name) {
        Some(Value::Variant(variant)) if &*variant.ty == jadpo_std::CURRENCY => {
            jadpo_std::money::code(variant.ordinal).ok_or(Fault::Shape)
        }
        _ => Err(Fault::Shape),
    };
    let (Some(Value::Decimal(rate)), Some(Value::Instant(at))) =
        (record.get("rate"), record.get("at"))
    else {
        return Err(Fault::Shape);
    };
    Ok(format!(
        "{}/{} {rate} {}",
        code("from")?,
        code("to")?,
        jadpo_std::format_instant(*at)
    ))
}

/// An exchange rate read from its canonical text: two currency codes,
/// read ignoring case, joined by `/`, one space, a decimal rate, one
/// space, and an instant in UTC ending `Z`.
fn read_exchange_rate(raw: &str) -> Option<Value> {
    let mut parts = raw.split(' ');
    let (pair, rate, at) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() {
        return None;
    }
    let (from, to) = pair.split_once('/')?;
    let currency = |code: &str| {
        jadpo_std::money::ordinal_of_code(code)
            .and_then(|ordinal| crate::money::currency_value(ordinal).ok())
    };
    let at = jadpo_std::instant_millis(at).filter(|_| at.ends_with('Z'))?;
    Some(Value::record(
        jadpo_std::standard::EXCHANGE_RATE,
        vec![
            ("from".into(), currency(from)?),
            ("to".into(), currency(to)?),
            ("rate".into(), Value::Decimal(decimal(rate)?)),
            ("at".into(), Value::Instant(at)),
        ],
    ))
}

/// A card's expiry from ISO 8601's calendar month, `2028-12`, its year
/// from 2000 to 2099 (chosen in chunk 5.5i2).
fn read_card_expiry(entry: &StandardType, raw: &str) -> Option<Value> {
    let (year, month) = raw.split_once('-')?;
    let digits = |text: &str, size: usize| {
        (text.len() == size && text.bytes().all(|b| b.is_ascii_digit()))
            .then(|| text.parse::<i64>().ok())
            .flatten()
    };
    let (year, month) = (digits(year, 4)?, digits(month, 2)?);
    ((2000..=2099).contains(&year) && (1..=12).contains(&month)).then(|| {
        Value::record(
            entry.name,
            vec![
                ("year".into(), Value::Int(year)),
                ("month".into(), Value::Int(month)),
            ],
        )
    })
}

/// A VAT number from VIES's form: its country's code (`EL` for Greece),
/// read ignoring case as every code is, then the number within a VAT
/// number's bounds (chosen in chunk 5.5i2). A number not in its country's
/// form breaks the type's rule `number_of_country`, which it is checked by
/// once it is read (chunk 5.5t2).
fn read_vat_number(entry: &StandardType, raw: &str) -> Option<Value> {
    let (prefix, number) = (raw.get(..2)?, raw.get(2..)?);
    let country_code = jadpo_std::standard::documents::vat_country(prefix)?;
    let countries = standard_type(jadpo_std::standard::places::COUNTRY)?;
    let country = code(countries, country_code)?;
    let bounds = jadpo_std::Part::Matching(jadpo_std::standard::documents::VAT_BOUNDS);
    bounds.rules().check_text(number).is_ok().then(|| {
        Value::record(
            entry.name,
            vec![
                ("country".into(), country),
                ("number".into(), Value::text(number)),
            ],
        )
    })
}

/// An enum's variant by its code, ignoring case (question 22).
pub(crate) fn variant_of(entry: &StandardType, raw: &str) -> Option<Value> {
    code(entry, raw)
}

/// An enum's variant by its code, ignoring case (question 22).
fn code(entry: &StandardType, raw: &str) -> Option<Value> {
    let codes = entry.codes()?;
    let at = codes
        .iter()
        .position(|(_, code)| code.eq_ignore_ascii_case(raw))?;
    let (name, _) = codes.get(at)?;
    Some(Value::variant(
        entry.name,
        u32::try_from(at).ok()?,
        name,
        Vec::new(),
    ))
}

/// A `ZonedTime` from RFC 9557: its local time with the offset the zone
/// has then, and the zone, `2026-10-25T09:30:00+00:00[Europe/London]`.
fn zoned(raw: &str) -> Option<Zoned> {
    let (at, zone) = raw.strip_suffix(']')?.split_once('[')?;
    let written = at.get(at.len().checked_sub(6)?..)?;
    let millis = jadpo_std::instant_millis(at)?;
    let zone = jadpo_std::ZONES
        .iter()
        .position(|(_, iana)| iana.eq_ignore_ascii_case(zone))?;
    let zoned = Zoned {
        millis,
        zone: u32::try_from(zone).ok()?,
    };
    // The offset written is the one the zone has at that instant.
    let (_, offset) = temporal::local(zoned)?;
    let sign = if offset < 0 { '-' } else { '+' };
    let minutes = offset.unsigned_abs() / 60_000;
    let expected = format!("{sign}{:02}:{:02}", minutes / 60, minutes % 60);
    (expected == written).then_some(zoned)
}

/// `text.format`'s options for an amount of money (area 19, "Money and
/// payments"), each its default where the call leaves it out.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MoneyOptions {
    pub precision: Option<jadpo_std::MoneyPrecision>,
    pub display: Option<jadpo_std::MoneyDisplay>,
    pub sign: Option<jadpo_std::MoneySign>,
}

impl MoneyOptions {
    /// The options from `precision:`, `display:` and `sign:` as a call
    /// gives them, each `none` when left out.
    pub fn of(precision: &Value, display: &Value, sign: &Value) -> Result<MoneyOptions, Fault> {
        fn read<T>(
            value: &Value,
            name: &str,
            from: fn(&str) -> Option<T>,
        ) -> Result<Option<T>, Fault> {
            match value {
                Value::None => Ok(None),
                Value::Variant(variant) if &*variant.ty == name => {
                    from(&variant.name).map(Some).ok_or(Fault::Shape)
                }
                _ => Err(Fault::Shape),
            }
        }
        Ok(MoneyOptions {
            precision: read(
                precision,
                jadpo_std::MoneyPrecision::NAME,
                jadpo_std::MoneyPrecision::from_name,
            )?,
            display: read(
                display,
                jadpo_std::MoneyDisplay::NAME,
                jadpo_std::MoneyDisplay::from_name,
            )?,
            sign: read(
                sign,
                jadpo_std::MoneySign::NAME,
                jadpo_std::MoneySign::from_name,
            )?,
        })
    }
}

/// The unit a quantity is shown in: `unit:` as a call gives it, a variant
/// of the kind's unit enum, or the kind's base unit when left out.
pub fn unit_of(
    kind: jadpo_std::QuantityKind,
    unit: &Value,
) -> Result<&'static jadpo_std::QuantityUnit, Fault> {
    match unit {
        Value::None => Ok(kind.base_unit()),
        Value::Variant(variant) if &*variant.ty == kind.unit_enum() => {
            kind.unit_named(&variant.name).ok_or(Fault::Shape)
        }
        _ => Err(Fault::Shape),
    }
}

/// `text.format` as a call runs it: given, besides the value and its
/// options, the name of the standard type held as text the value is, which
/// the checker fills (an IBAN is text to the runtime), or `none`.
pub fn format_as(
    standard: &Value,
    value: &Value,
    locale: &Value,
    zone: &Value,
    format_spec: &Value,
    options: &MoneyOptions,
    unit: &Value,
) -> Result<Value, Fault> {
    if let (Value::Text(name), Value::Text(_) | Value::Int(_) | Value::List(_) | Value::Map(_)) =
        (standard, value)
        && let Some(entry) = standard_type(name)
        && matches!(
            entry.holds,
            Holds::Text { .. } | Holds::Whole { .. } | Holds::List(_) | Holds::Map(..)
        )
    {
        let tag = present::tag_of(locale)?;
        return Ok(Value::text(&shown(entry, value, tag, zone, format_spec)?));
    }
    // A type built on `Decimal` shown in a form of its own (a latitude
    // with its hemisphere, a bearing in degrees, chunk 5.5s), which the
    // runtime holds as the decimal.
    if let (Value::Text(name), Value::Decimal(_)) = (standard, value)
        && let Some(entry) = standard_type(name)
        && matches!(
            entry.holds,
            Holds::Bounded {
                of: jadpo_std::Builtin::Decimal,
                ..
            }
        )
    {
        let tag = present::tag_of(locale)?;
        return Ok(Value::text(&shown(entry, value, tag, zone, format_spec)?));
    }
    // A type built on a quantity shown in units of its own (a heart rate
    // in beats a minute, chunk 5.5m2), which the runtime holds as the
    // quantity.
    if let (Value::Text(name), Value::Quantity(_)) = (standard, value)
        && let Some(entry) = standard_type(name)
        && matches!(entry.format.key, FormatKey::UnitsIn(_))
    {
        let tag = present::tag_of(locale)?;
        return Ok(Value::text(&crate::measures::units_in(
            entry, value, unit, tag,
        )?));
    }
    format(value, locale, zone, format_spec, options, unit)
}

/// `text.format(value, locale:, zone:, format:, precision:, display:,
/// sign:, unit:)`: the value for people in `locale`, as its entry's format
/// key says. Each option is `none` where the type doesn't take it.
pub fn format(
    value: &Value,
    locale: &Value,
    zone: &Value,
    format_spec: &Value,
    options: &MoneyOptions,
    unit: &Value,
) -> Result<Value, Fault> {
    let tag = present::tag_of(locale)?;
    let text = match (entry_of(value), value) {
        (Some(_), Value::Money(money)) => present::money::format(*money, tag, options)?,
        (Some(_), Value::Quantity(quantity)) => {
            present::units::format(quantity.amount, unit_of(quantity.kind, unit)?, tag)?
        }
        (Some(_), Value::Percentage(fraction)) => present::percent(fraction, tag)?,
        // Measures in the unit `unit:` names, for their kind (chunk 5.5m2).
        (Some(entry), value) if matches!(entry.format.key, FormatKey::Measures { .. }) => {
            crate::measures::measures(entry, value, unit, tag)?
        }
        (Some(entry), value) => shown(entry, value, tag, zone, format_spec)?,
        // `Email`, `Url` and `IpAddress` are shown as they are written.
        (None, Value::Text(text)) => text.to_string(),
        // A type built on `Bytes` is shown as its canonical text, its
        // Base64 (chunk 5.5r3).
        (None, Value::Bytes(bytes)) => crate::encoding::to_base64(bytes),
        // A number in the locale's digits and separators (area 19,
        // "Numbers").
        (None, Value::Int(_) | Value::Decimal(_)) => present::number(value, tag)?,
        _ => return Err(Fault::Shape),
    };
    Ok(Value::text(&text))
}

/// A standard type's value for people in the locale tagged `tag`.
pub fn shown(
    entry: &StandardType,
    value: &Value,
    tag: &str,
    zone: &Value,
    format_spec: &Value,
) -> Result<String, Fault> {
    Ok(match (entry.format.key, value) {
        (FormatKey::Canonical, value) => canonical(entry, value)?,
        // A secret value is never shown: only as a report writes it
        // (chunk 5.5r2).
        (FormatKey::Redacted, value @ Value::Secret(_)) => value.to_string(),
        // The text people read in it (chunk 5.5o).
        (FormatKey::PlainText, Value::Text(text))
            if entry.name == jadpo_std::standard::markup::MARKDOWN =>
        {
            crate::markup::markdown_text(text)
        }
        (FormatKey::PlainText, Value::Text(text)) => jadpo_std::standard::markup::plain_text(text),
        // A document, a block or a span, once written as HTML (chunk
        // 5.5o2).
        (FormatKey::PlainText, value) => crate::markup::rich_plain_text(entry.name, value)?,
        // The entry `text.pick` gives for the locale, with no application
        // default to fall back on here (chunk 5.5o2); `text.format` gives
        // the application's own through `run`.
        (FormatKey::Picked, value) => {
            let ordinal = jadpo_std::LOCALES
                .iter()
                .position(|(_, listed)| listed.eq_ignore_ascii_case(tag))
                .and_then(|at| u32::try_from(at).ok())
                .ok_or(Fault::Shape)?;
            text_of(crate::locale::pick(value, ordinal, None)?)?
        }
        (FormatKey::DateTime, value) => {
            text_of(present::format_in(value, zone, tag, format_spec)?)?
        }
        (FormatKey::Interval, Value::Record(record)) => {
            let (start, end) = interval(entry, record)?;
            let start = present::format_in(start, zone, tag, format_spec)?;
            let end = present::format_in(end, zone, tag, format_spec)?;
            format!("{} – {}", text_of(start)?, text_of(end)?)
        }
        (FormatKey::MonthName, Value::Variant(variant)) => {
            present::names::month(variant.ordinal, tag)?
        }
        (FormatKey::YearMonth | FormatKey::Year, value) => present::names::year_month(value, tag)?,
        (FormatKey::OffsetName, value) => present::names::offset(value, tag)?,
        (FormatKey::PeriodUnits, value) => {
            present::names::period(crate::calendar::period_parts(value)?, tag)?
        }
        (FormatKey::DurationUnits, Value::Duration(millis)) => {
            present::names::duration(*millis, tag)?
        }
        (FormatKey::WeekdayName, Value::Variant(variant)) => {
            present::names::weekday(variant.ordinal, tag)?
        }
        (FormatKey::LocaleName, value) => present::names::locale(&canonical(entry, value)?, tag)?,
        (FormatKey::ZoneName, value) => present::names::zone(&canonical(entry, value)?, tag)?,
        (FormatKey::Money, Value::Money(money)) => {
            present::money::format(*money, tag, &MoneyOptions::default())?
        }
        (FormatKey::Units, Value::Quantity(quantity)) => {
            present::units::format(quantity.amount, quantity.kind.base_unit(), tag)?
        }
        (FormatKey::Percent, Value::Percentage(fraction)) => present::percent(fraction, tag)?,
        (FormatKey::CurrencyName, value) => {
            present::money::currency_name(&canonical(entry, value)?, tag)?
        }
        // A place, a language or a script by CLDR's name for it, a
        // withdrawn code by its list's English name (chunk 5.5l).
        (FormatKey::RegionName, Value::Variant(variant)) => {
            let code = canonical(entry, value)?;
            match present::names::region(&code, tag)? {
                Some(name) => name,
                None => fallback_name(entry, variant, &code),
            }
        }
        (FormatKey::ContinentName, Value::Variant(variant)) => {
            let region = jadpo_std::standard::places::CONTINENT_REGIONS
                .get(variant.ordinal as usize)
                .ok_or(Fault::Shape)?;
            present::names::region(region, tag)?.unwrap_or_else(|| canonical_or_name(variant))
        }
        (FormatKey::LanguageName, Value::Variant(variant)) => {
            let code = canonical(entry, value)?;
            match present::names::language(&code, tag)? {
                Some(name) => name,
                None => fallback_name(entry, variant, &code),
            }
        }
        (FormatKey::ScriptName, Value::Variant(variant)) => {
            let code = canonical(entry, value)?;
            match present::names::script(&code, tag)? {
                Some(name) => name,
                None => fallback_name(entry, variant, &code),
            }
        }
        (FormatKey::SubdivisionName, Value::Variant(variant)) => {
            match jadpo_std::standard::subdivision_name(variant.ordinal) {
                Some(name) => name.to_string(),
                None => canonical(entry, value)?,
            }
        }
        (FormatKey::PersonName, value) => present::contact::person_name(value, tag)?,
        (FormatKey::ProblemDetails, Value::Record(record)) => crate::web::problem_text(record)?,
        // SMPTE's notation, the same in every locale (chunk 5.5p).
        (FormatKey::Timecode, Value::Record(record)) => crate::media::timecode_shown(record)?,
        // In the units a reader expects (chunk 5.5m2).
        (FormatKey::UnitsIn(_), value) => {
            crate::measures::units_in(entry, value, &Value::None, tag)?
        }
        (FormatKey::Measures { .. }, value) => {
            crate::measures::measures(entry, value, &Value::None, tag)?
        }
        (FormatKey::Dosage, Value::Record(record)) => crate::measures::dosage(record, tag)?,
        // The first part named that the value has (chunk 5.5r): text as
        // written, an instant as its canonical text.
        (FormatKey::FirstPart(parts), Value::Record(record)) => {
            // Text that is empty is passed over, as a certificate's empty
            // subject is (chunk 5.5r5).
            let shown = parts.iter().find_map(|name| match record.get(name) {
                Some(Value::Text(text)) if !text.is_empty() => Some(text.to_string()),
                Some(Value::Instant(at)) => Some(jadpo_std::format_instant(*at)),
                _ => None,
            });
            shown.ok_or(Fault::Shape)?
        }
        // A latitude or a longitude with its hemisphere, a geospatial
        // record (chunk 5.5s).
        (FormatKey::Coordinate { positive, negative }, Value::Decimal(value)) => {
            crate::geo::coordinate(*value, positive, negative, tag)?
        }
        (FormatKey::Geometry, value) => crate::geo::shown(value, tag)?,
        // A vector's numbers, an embedding's model and vector (chunk
        // 5.5s2).
        (FormatKey::Vector, Value::List(items)) => {
            let numbers = items
                .iter()
                .map(|item| present::number(item, tag))
                .collect::<Result<Vec<_>, _>>()?;
            format!("[{}]", numbers.join("; "))
        }
        (FormatKey::Vector, Value::Record(record)) => {
            let (Some(model), Some(vector)) = (record.get("model"), record.get("vector")) else {
                return Err(Fault::Shape);
            };
            let models = entry_of(model).ok_or(Fault::Shape)?;
            format!(
                "{} {}",
                shown(models, model, tag, zone, format_spec)?,
                shown(entry, vector, tag, zone, format_spec)?
            )
        }
        // Each part the value has, by its label (chunk 5.5s2).
        (FormatKey::ByLabel(labels), Value::Record(record)) => {
            let mut out = Vec::new();
            for (label, part) in labels {
                let shown = match record.get(part) {
                    None | Some(Value::None) => continue,
                    Some(value @ (Value::Int(_) | Value::Decimal(_))) => {
                        present::number(value, tag)?
                    }
                    Some(value) => {
                        let entry = entry_of(value).ok_or(Fault::Shape)?;
                        shown(entry, value, tag, zone, format_spec)?
                    }
                };
                out.push(format!("{label} {shown}"));
            }
            out.join(", ")
        }
        (FormatKey::PostalAddress, value) => present::contact::postal_address(value, tag)?,
        // National for a reader whose region shares the number's calling
        // code, international otherwise (chunk 5.5l2).
        (FormatKey::PhoneNumber, value) => {
            let text = canonical(entry, value)?;
            let reader = present::contact::reader_region(tag)?;
            jadpo_std::standard::phones::shown(&text, reader.as_deref()).ok_or(Fault::Shape)?
        }
        // A payment or identity number's printed form (area 19, chunk
        // 5.5i): grouped, or its last four after a mask.
        (FormatKey::Grouped(grouping), value) => grouping.apply(&canonical(entry, value)?),
        (FormatKey::LastFour { mask }, value) => last_four(mask, &canonical(entry, value)?),
        // A card brand by the scheme's own name (chunk 5.5i2); a closed
        // list CLDR names none of by its authority's English wording
        // (chunk 5.5m).
        (FormatKey::Names(names), Value::Variant(variant)) => names
            .get(variant.ordinal as usize)
            .map(|name| (*name).to_string())
            .ok_or(Fault::Shape)?,
        // A masked card as a receipt prints it, its brand's name first
        // where it is known.
        (FormatKey::MaskedCard, Value::Record(record)) => {
            let Some(Value::Text(digits)) = record.get("last_four") else {
                return Err(Fault::Shape);
            };
            let masked = last_four("•••• ", digits);
            match record.get("brand") {
                Some(brand @ Value::Variant(_)) => {
                    let brands = entry_of(brand).ok_or(Fault::Shape)?;
                    format!("{} {masked}", shown(brands, brand, tag, zone, format_spec)?)
                }
                _ => masked,
            }
        }
        // As the card prints it, `12/28`.
        (FormatKey::CardExpiry, Value::Record(record)) => {
            match (record.get("year"), record.get("month")) {
                (Some(Value::Int(year)), Some(Value::Int(month))) => {
                    format!("{month:02}/{:02}", year % 100)
                }
                _ => return Err(Fault::Shape),
            }
        }
        // A tax or identity document's number, masked (chunk 5.5i2).
        // A bank account's number or IBAN, whichever its variant has
        // (chunk 5.5i4).
        (FormatKey::PartLastFour { parts, mask }, value) => {
            let part = |name: &str| match value {
                Value::Record(record) => record.get(name),
                Value::Variant(variant) => variant
                    .fields
                    .iter()
                    .find(|(field, _)| &**field == name)
                    .map(|(_, value)| value),
                _ => None,
            };
            match parts.iter().find_map(|name| part(name)) {
                Some(Value::Text(text)) => last_four(mask, text),
                _ => return Err(Fault::Shape),
            }
        }
        // The parts named, each as its own type is shown, joined: a
        // price's amount, an interest rate's rate and basis, a choice's
        // parts its variant has (chunk 5.5i3).
        (FormatKey::Parts { parts, separator }, value) => {
            let part = |name: &str| match value {
                Value::Record(record) => record.get(name),
                Value::Variant(variant) => variant
                    .fields
                    .iter()
                    .find(|(field, _)| &**field == name)
                    .map(|(_, value)| value),
                _ => None,
            };
            let mut out = Vec::new();
            for value in parts.iter().filter_map(|name| part(name)) {
                out.push(match value {
                    Value::Int(_) | Value::Decimal(_) => present::number(value, tag)?,
                    // Text as it is written: a barcode's data (chunk 5.5m2).
                    Value::Text(text) => text.to_string(),
                    value => {
                        let entry = entry_of(value).ok_or(Fault::Shape)?;
                        shown(entry, value, tag, zone, format_spec)?
                    }
                });
            }
            if out.is_empty() {
                return Err(Fault::Shape);
            }
            out.join(separator)
        }
        (FormatKey::Suffixed(suffix), value @ (Value::Int(_) | Value::Decimal(_))) => {
            format!("{}{suffix}", present::number(value, tag)?)
        }
        (FormatKey::Range, Value::Record(record)) => {
            let (start, end) = range_ends(record)?;
            let end_shown = |end: Option<&Value>| match end {
                Some(value @ (Value::Int(_) | Value::Decimal(_))) => present::number(value, tag),
                Some(value) => {
                    let entry = entry_of(value).ok_or(Fault::Shape)?;
                    shown(entry, value, tag, zone, format_spec)
                }
                None => Ok("…".to_string()),
            };
            format!("{} – {}", end_shown(start)?, end_shown(end)?)
        }
        _ => return Err(Fault::Shape),
    })
}

/// `text`'s last four characters after `mask`, as PCI DSS lets a card
/// number be shown: `•••• 4242`.
fn last_four(mask: &str, text: &str) -> String {
    let skip = text.chars().count().saturating_sub(4);
    format!("{mask}{}", text.chars().skip(skip).collect::<String>())
}

/// A withdrawn code's English name from its list, which CLDR doesn't
/// name, or the code itself.
fn fallback_name(entry: &StandardType, variant: &crate::Variant, code: &str) -> String {
    jadpo_std::standard::retired(entry.name, &variant.name)
        .filter(|name| !name.is_empty())
        .map_or_else(|| code.to_string(), str::to_string)
}

/// A variant's own name, for a continent CLDR has no name for.
fn canonical_or_name(variant: &crate::Variant) -> String {
    variant.name.to_string()
}

fn text_of(value: Value) -> Result<String, Fault> {
    match value {
        Value::Text(text) => Ok(text.to_string()),
        _ => Err(Fault::Shape),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An embedding is stored as its model's provider and name and its
    /// vector (chunk 5.5v1), and made again from them.
    #[test]
    fn an_embedding_is_its_model_and_its_vector() {
        let vector = crate::vector::from_floats(&[0.5, -0.25, 1.0]).unwrap();
        let model = [Value::text("openai"), Value::text("text-embedding-3-small")];
        let read = from_parts_of(
            "Embedding",
            &[model[0].clone(), model[1].clone(), vector.clone()],
        );
        let Ok(embedding) = read else {
            panic!("{read:?}");
        };
        assert_eq!(
            to_parts(&embedding),
            Some(vec![model[0].clone(), model[1].clone(), vector])
        );
    }

    #[test]
    fn durations_read_and_write_as_iso_8601() {
        assert_eq!(iso_duration("PT5M"), Some(300_000));
        assert_eq!(iso_duration("P1DT2H"), Some(93_600_000));
        assert_eq!(iso_duration("PT1.5S"), Some(1_500));
        assert_eq!(iso_duration("PT"), None);
        assert_eq!(iso_duration("5m"), None);
        assert_eq!(iso_duration("-PT5M"), Some(-300_000));
        assert_eq!(iso_duration("--PT5M"), None);
        assert_eq!(iso_duration("P+1D"), None);
        assert_eq!(iso_duration("P-1D"), None);
        assert_eq!(jadpo_std::iso_duration(-300_000), "-PT5M");
        assert_eq!(jadpo_std::iso_duration(300_000), "PT5M");
        assert_eq!(jadpo_std::iso_duration(1_500), "PT1.5S");
        assert_eq!(jadpo_std::iso_duration(0), "PT0S");
    }
}
