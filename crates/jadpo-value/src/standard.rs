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
        Value::Variant(variant) => return standard_type(&variant.ty),
        Value::Record(record) => return standard_type(&record.ty),
        _ => return None,
    };
    jadpo_std::standard::of_builtin(builtin)
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
        _ => return Err(Fault::Shape),
    };
    Ok(Value::text(&text))
}

/// A standard type's value as its canonical text, as its entry's text form
/// writes it.
pub fn canonical(entry: &StandardType, value: &Value) -> Result<String, Fault> {
    let form = entry.text.ok_or(Fault::Shape)?;
    Ok(match (form, value) {
        (TextForm::Checked, Value::Text(text)) => text.to_string(),
        (TextForm::Uuid, Value::Uuid(text)) => text.to_ascii_lowercase(),
        (TextForm::Instant, Value::Instant(millis)) => jadpo_std::format_instant(*millis),
        (TextForm::Date, Value::Date(day)) => jadpo_std::format_date(*day),
        (TextForm::Zoned, Value::Zoned(zoned)) => zoned.to_string(),
        (TextForm::Duration, Value::Duration(millis)) => jadpo_std::iso_duration(*millis),
        (TextForm::Code, Value::Variant(variant)) => entry
            .codes()
            .and_then(|codes| codes.get(variant.ordinal as usize))
            .map(|(_, code)| (*code).to_string())
            .ok_or(Fault::Shape)?,
        (TextForm::Interval, Value::Record(record)) => {
            let (start, end) = interval(entry, record)?;
            format!(
                "{}/{}",
                jadpo_std::format_instant(start),
                jadpo_std::format_instant(end)
            )
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
/// end; a start after its end is `malformed`, as for an `InstantRange`.
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
    if let (Some(start), Some(end)) = (&start, &end)
        && crate::ops::compare(start, end) == Some(std::cmp::Ordering::Greater)
    {
        return Err(MALFORMED);
    }
    Ok(crate::range::range_of(start, end))
}

/// The item type a range type's name gives it: `Int` for `Range<Int>`.
fn range_item(ty: &str) -> Option<&str> {
    ty.strip_prefix(jadpo_std::RANGE)?
        .strip_prefix('<')?
        .strip_suffix('>')
}

/// An interval's two instants, by its entry's fields in order.
fn interval(entry: &StandardType, record: &crate::Record) -> Result<(i64, i64), Fault> {
    let Holds::Object([(start, _), (end, _)]) = entry.holds else {
        return Err(Fault::Shape);
    };
    match (record.get(start), record.get(end)) {
        (Some(Value::Instant(start)), Some(Value::Instant(end))) => Ok((*start, *end)),
        _ => Err(Fault::Shape),
    }
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
fn decimal(raw: &str) -> Option<Decimal> {
    let digits = raw.strip_prefix('-').unwrap_or(raw);
    let (whole, places) = digits.split_once('.').unwrap_or((digits, "0"));
    let plain = |part: &str| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit());
    if !plain(whole) || !plain(places) {
        return None;
    }
    Decimal::parse(raw)
}

/// A standard type's value read from its canonical text.
pub fn read(entry: &StandardType, raw: &str) -> Result<Value, Fault> {
    let form = entry.text.ok_or(Fault::Shape)?;
    let read = match form {
        TextForm::Checked => entry
            .validator
            .is_some_and(|validator| validator.accepts(raw))
            .then(|| Value::text(raw)),
        TextForm::Uuid => entry
            .validator
            .is_some_and(|validator| validator.accepts(raw))
            .then(|| Value::uuid(raw)),
        TextForm::Instant => jadpo_std::instant_millis(raw)
            .filter(|_| raw.ends_with('Z'))
            .map(Value::Instant),
        TextForm::Date => jadpo_std::date_number(raw)
            .filter(|_| jadpo_std::check_calendar_date(raw).is_ok())
            .map(Value::Date),
        TextForm::Zoned => zoned(raw).map(Value::Zoned),
        TextForm::Duration => crate::json::iso_duration(raw).map(Value::Duration),
        TextForm::Code => code(entry, raw),
        // Its ends' type is the range type's argument, which `parse` reads.
        TextForm::Range => return Err(Fault::Shape),
        TextForm::Interval => {
            let Holds::Object([(start, _), (end, _)]) = entry.holds else {
                return Err(Fault::Shape);
            };
            raw.split_once('/').and_then(|(from, to)| {
                let from = jadpo_std::instant_millis(from)?;
                let to = jadpo_std::instant_millis(to)?;
                (from <= to).then(|| {
                    Value::record(
                        entry.name,
                        vec![
                            ((*start).into(), Value::Instant(from)),
                            ((*end).into(), Value::Instant(to)),
                        ],
                    )
                })
            })
        }
    };
    read.ok_or(MALFORMED)
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

/// `text.format(value, locale:, zone:, format:)`: the value for people in
/// `locale`, as its entry's format key says. `zone` and `format` are
/// `none` where the type takes neither.
pub fn format(
    value: &Value,
    locale: &Value,
    zone: &Value,
    format_spec: &Value,
) -> Result<Value, Fault> {
    let tag = present::tag_of(locale)?;
    let text = match (entry_of(value), value) {
        (Some(entry), value) => shown(entry, value, tag, zone, format_spec)?,
        // `Email`, `Url` and `IpAddress` are shown as they are written.
        (None, Value::Text(text)) => text.to_string(),
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
        (FormatKey::DateTime, value) => {
            text_of(present::format_in(value, zone, tag, format_spec)?)?
        }
        (FormatKey::Interval, Value::Record(record)) => {
            let (start, end) = interval(entry, record)?;
            let start = present::format_in(&Value::Instant(start), zone, tag, format_spec)?;
            let end = present::format_in(&Value::Instant(end), zone, tag, format_spec)?;
            format!("{} – {}", text_of(start)?, text_of(end)?)
        }
        (FormatKey::DurationUnits, Value::Duration(millis)) => {
            present::names::duration(*millis, tag)?
        }
        (FormatKey::WeekdayName, Value::Variant(variant)) => {
            present::names::weekday(variant.ordinal, tag)?
        }
        (FormatKey::LocaleName, value) => present::names::locale(&canonical(entry, value)?, tag)?,
        (FormatKey::ZoneName, value) => present::names::zone(&canonical(entry, value)?, tag)?,
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

fn text_of(value: Value) -> Result<String, Fault> {
    match value {
        Value::Text(text) => Ok(text.to_string()),
        _ => Err(Fault::Shape),
    }
}
