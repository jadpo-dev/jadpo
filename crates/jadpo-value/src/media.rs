//! `media`'s operation and the media types' text (area 19, "Media", chunk
//! 5.5p): `media.fit`, the canonical texts of the objects whose parts are
//! numbers joined by a separator (`1920x1080`, `16:9`, `0 0 300 200`), a
//! content rating's `BBFC 12A`, a WebVTT cue, a timecode in SMPTE's
//! notation, and the rules across a timecode's and a cue's parts as a value
//! read from JSON is checked by them. The grammar and the arithmetic are
//! `jadpo_std::standard::media`'s.

use jadpo_std::Part;
use jadpo_std::standard::media::{self, IMAGE_DIMENSIONS};
use jadpo_std::standard::{Holds, StandardType, standard_type};

use crate::value::Decimal;
use crate::{Fault, Record, Value};

/// A whole-number part of a record.
fn whole(record: &Record, name: &str) -> Result<i64, Fault> {
    match record.get(name) {
        Some(Value::Int(n)) => Ok(*n),
        _ => Err(Fault::Shape),
    }
}

/// `media.fit(dimensions, within:)`: `dimensions` scaled to fit inside
/// `within`, keeping its aspect ratio, never larger, each side rounded down
/// and never below 1.
pub fn fit(dimensions: &Value, within: &Value) -> Result<Value, Fault> {
    let (Value::Record(image), Value::Record(bounds)) = (dimensions, within) else {
        return Err(Fault::Shape);
    };
    let (width, height) = media::fit(
        whole(image, "width")?,
        whole(image, "height")?,
        whole(bounds, "width")?,
        whole(bounds, "height")?,
    );
    Ok(Value::record(
        IMAGE_DIMENSIONS,
        vec![
            ("width".into(), Value::Int(width)),
            ("height".into(), Value::Int(height)),
        ],
    ))
}

/// The parts of a standard object, in order.
fn fields(entry: &StandardType) -> Result<&'static [(&'static str, Part)], Fault> {
    match entry.holds {
        Holds::Object(fields) => Ok(fields),
        _ => Err(Fault::Shape),
    }
}

/// An object whose parts are numbers, as its canonical text writes them:
/// each part's number joined by `separator` (`TextForm::Wholes` and
/// `TextForm::Decimals`).
pub fn numbers_text(
    entry: &StandardType,
    record: &Record,
    separator: &str,
) -> Result<String, Fault> {
    let mut parts = Vec::new();
    for (name, _) in fields(entry)? {
        parts.push(match record.get(name) {
            Some(Value::Int(n)) => n.to_string(),
            Some(Value::Decimal(d)) => d.to_string(),
            _ => return Err(Fault::Shape),
        });
    }
    Ok(parts.join(separator))
}

/// Whether `n`, at `scale` places, is within the part's bounds.
fn within(part: Part, n: crate::Decimal) -> bool {
    let rules = part.rules();
    let compare = |bound: Option<jadpo_std::Num>, keep: fn(std::cmp::Ordering) -> bool| {
        bound.is_none_or(|bound| {
            crate::ops::compare(
                &Value::Decimal(n),
                &Value::Decimal(Decimal::new(bound.mantissa, bound.scale)),
            )
            .is_some_and(keep)
        })
    };
    compare(rules.min, |o| o != std::cmp::Ordering::Less)
        && compare(rules.max, |o| o != std::cmp::Ordering::Greater)
}

/// An object whose parts are numbers, read from its canonical text: as
/// many numbers as it has parts, joined by `separator`, each a whole number
/// within its range (`TextForm::Wholes`) or a decimal within its bounds
/// (`TextForm::Decimals`).
pub fn read_numbers(entry: &StandardType, raw: &str, separator: &str) -> Option<Value> {
    let fields = fields(entry).ok()?;
    let texts: Vec<&str> = raw.split(separator).collect();
    if texts.len() != fields.len() {
        return None;
    }
    let mut values = Vec::new();
    for ((name, part), text) in fields.iter().zip(texts) {
        let value = match *part {
            Part::Whole(min, max) => Value::Int(media::whole_in(text, min, max)?),
            part => {
                let n = media::plain_decimal(text)?;
                let n = Decimal::new(n.mantissa, n.scale);
                if !within(part, n) {
                    return None;
                }
                Value::Decimal(n)
            }
        };
        values.push(((*name).into(), value));
    }
    Some(Value::record(entry.name, values))
}

/// An object of an enum and a text part as its canonical text writes it:
/// the enum's code, a space, the text (`TextForm::CodeThenText`).
pub fn code_then_text(entry: &StandardType, record: &Record) -> Result<String, Fault> {
    joined_text(entry, record, " ")
}

/// An object of two parts as its canonical text writes them, joined by
/// `separator` (`TextForm::Joined`, chunk 5.5m2): the first an enum's code
/// or its text, the second its text.
pub fn joined_text(
    entry: &StandardType,
    record: &Record,
    separator: &str,
) -> Result<String, Fault> {
    let [(first, _), (second, _)] = fields(entry)? else {
        return Err(Fault::Shape);
    };
    let head = match record.get(first) {
        Some(Value::Variant(variant)) => standard_type(&variant.ty)
            .and_then(StandardType::codes)
            .and_then(|codes| codes.get(variant.ordinal as usize))
            .map(|(_, code)| (*code).to_string())
            .ok_or(Fault::Shape)?,
        Some(Value::Text(text)) => text.to_string(),
        _ => return Err(Fault::Shape),
    };
    let Some(Value::Text(tail)) = record.get(second) else {
        return Err(Fault::Shape);
    };
    Ok(format!("{head}{separator}{tail}"))
}

/// An object of an enum and a text part read from its canonical text: the
/// enum's code, ignoring case, one space, and text its part's pattern
/// takes.
pub fn read_code_then_text(entry: &StandardType, raw: &str) -> Option<Value> {
    read_joined(entry, raw, " ")
}

/// An object of two parts read from its canonical text, split at the
/// first `separator` (`TextForm::Joined`, chunk 5.5m2): the first an
/// enum's code, ignoring case, text a standard type held as text accepts,
/// or text its part's pattern takes; the second text its part's pattern
/// takes. The rules across the parts are the type's rules, which the value
/// is checked by once it is read (chunk 5.5t2).
pub fn read_joined(entry: &StandardType, raw: &str, separator: &str) -> Option<Value> {
    let [(first, head_part), (second, tail_part)] = fields(entry).ok()? else {
        return None;
    };
    let (head, tail) = raw.split_once(separator)?;
    let head = match head_part {
        Part::Standard(other) => {
            let other = standard_type(other)?;
            match other.codes() {
                Some(_) => crate::standard::variant_of(other, head)?,
                None => crate::standard::read(other, head).ok()?,
            }
        }
        part => {
            part.rules().check_text(head).ok()?;
            Value::text(head)
        }
    };
    if tail_part.rules().check_text(tail).is_err() {
        return None;
    }
    Some(Value::record(
        entry.name,
        vec![
            ((*first).into(), head),
            ((*second).into(), Value::text(tail)),
        ],
    ))
}

/// A `SubtitleCue`'s canonical text.
pub fn cue_text(record: &Record) -> Result<String, Fault> {
    match (record.get("start"), record.get("end"), record.get("text")) {
        (Some(Value::Duration(start)), Some(Value::Duration(end)), Some(Value::Text(text))) => {
            media::cue_text(*start, *end, text).ok_or(Fault::Shape)
        }
        _ => Err(Fault::Shape),
    }
}

/// A `SubtitleCue` read from its canonical text.
pub fn read_cue(entry: &StandardType, raw: &str) -> Option<Value> {
    let (start, end, text) = media::read_cue(raw)?;
    Some(Value::record(
        entry.name,
        vec![
            ("start".into(), Value::Duration(start)),
            ("end".into(), Value::Duration(end)),
            ("text".into(), Value::text(text)),
        ],
    ))
}

/// A `Timecode` in SMPTE ST 12-1's notation, `01:00:00;00`.
pub fn timecode_shown(record: &Record) -> Result<String, Fault> {
    let drop = matches!(record.get("drop_frame"), Some(Value::Bool(true)));
    Ok(media::timecode_notation(
        whole(record, "hours")?,
        whole(record, "minutes")?,
        whole(record, "seconds")?,
        whole(record, "frames")?,
        drop,
    ))
}
