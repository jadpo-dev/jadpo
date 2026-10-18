//! `web`'s operations (area 19, "Web and software", chunk 5.5n), over
//! `jadpo_std::standard::web`, where the grammar they share with the
//! types' validators lives; and the texts of the two web objects: a
//! `UtmParameters`' query, and a `ProblemDetails` as people read it.

use jadpo_std::standard::web;

use crate::{Fault, Record, Value};

/// `web.cidr_contains(range, address)`.
pub fn cidr_contains(range: &Value, address: &Value) -> Result<Value, Fault> {
    match (range, address) {
        (Value::Text(range), Value::Text(address)) => web::cidr_contains(range, address)
            .map(Value::Bool)
            .ok_or(Fault::Shape),
        _ => Err(Fault::Shape),
    }
}

/// `web.resolve(base, relative)`.
pub fn resolve(base: &Value, relative: &Value) -> Result<Value, Fault> {
    match (base, relative) {
        (Value::Text(base), Value::Text(relative)) => {
            Ok(Value::text(&web::resolve(base, relative)))
        }
        _ => Err(Fault::Shape),
    }
}

/// `web.with_query(url, entries)`: the entries in key order, by code point,
/// whatever order the map holds them in.
pub fn with_query(url: &Value, entries: &Value) -> Result<Value, Fault> {
    let (Value::Text(url), Value::Map(map)) = (url, entries) else {
        return Err(Fault::Shape);
    };
    let mut entries = map
        .iter()
        .map(|(key, value)| match (key, value) {
            (Value::Text(key), Value::Text(value)) => Ok((&**key, &**value)),
            _ => Err(Fault::Shape),
        })
        .collect::<Result<Vec<(&str, &str)>, Fault>>()?;
    entries.sort_by(|a, b| a.0.cmp(b.0));
    Ok(Value::text(&web::with_query(url, &entries)))
}

/// A `UtmParameters`' canonical text.
pub fn utm_text(record: &Record) -> Result<String, Fault> {
    let parts = web::UTM_FIELDS
        .iter()
        .map(|(name, _)| match record.get(name) {
            Some(Value::Text(text)) => Ok(Some(&**text)),
            Some(Value::None) => Ok(None),
            _ => Err(Fault::Shape),
        })
        .collect::<Result<Vec<_>, Fault>>()?;
    Ok(web::utm_text(&parts))
}

/// A `UtmParameters` read from its canonical text.
pub fn read_utm(raw: &str) -> Option<Value> {
    let parts = web::read_utm(raw)?;
    Some(Value::record(
        web::UTM_PARAMETERS,
        web::UTM_FIELDS
            .iter()
            .zip(parts)
            .map(|((name, _), part)| {
                (
                    (*name).into(),
                    part.map_or(Value::None, |text| Value::text(&text)),
                )
            })
            .collect(),
    ))
}

/// A `ProblemDetails` as people read it: its title and its detail, each
/// on its own line; with neither, its status; with no status either, its
/// type, `about:blank` when it has none (RFC 9457 §4.2).
pub fn problem_text(record: &Record) -> Result<String, Fault> {
    let text = |name: &str| match record.get(name) {
        Some(Value::Text(text)) => Some(text.to_string()),
        _ => None,
    };
    let lines: Vec<String> = [text("title"), text("detail")]
        .into_iter()
        .flatten()
        .collect();
    if !lines.is_empty() {
        return Ok(lines.join("\n"));
    }
    if let Some(Value::Int(status)) = record.get("status") {
        return Ok(status.to_string());
    }
    Ok(text("type_").unwrap_or_else(|| "about:blank".to_string()))
}
