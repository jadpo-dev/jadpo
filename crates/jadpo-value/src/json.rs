//! Values crossing the boundary as JSON (area 2's forms; area 7, "One
//! input"), driven by the route table's codecs, so the interpreter and
//! compiled code read and write a request the same way.
//!
//! Reading checks as it goes: each problem names the field by its path in
//! the input, list positions as numbers (`lines.2.quantity`), and the rule
//! it broke, never the value (area 4, question 4). The rules are the type's
//! own (`max`, `max_length`, `pattern`, `format`), and these:
//!
//! - `required`: a field that must be given is missing, or `null`;
//! - `unknown_field`: the input has no such field;
//! - `type`: a value of the wrong JSON kind, such as text for a number;
//! - `variant`: text that names no variant of the enum;
//! - `duplicate`: a set repeats an item.
//!
//! Writing follows area 2: an `Int` is a number when its declared range
//! fits JavaScript's safe integers and a string otherwise, a `Decimal` is
//! always a string, instants are ISO 8601 to the millisecond, a plain
//! variant is its name and a variant with data an object with `tag`.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::rc::Rc;

use jadpo_std::Builtin;
use jadpo_std::http::{Codec, CodecId};

use crate::shape::Shape;
use crate::{Decimal, Validator, Value};

/// One problem with a request's input: the field and the rule.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
    pub field: String,
    pub rule: String,
}

/// The codecs of a route table, with each type's validator and shape ready
/// to run.
#[derive(Clone, Copy, Debug)]
pub struct Codecs<'a> {
    pub codecs: &'a [Codec],
    /// The plan's validators, compiled, by number.
    pub validators: &'a [Validator],
    /// The plan's shapes, by number.
    pub shapes: &'a [Shape],
}

/// Where in the input a value is: field names and list positions.
#[derive(Clone, Debug, Default)]
pub struct Path(Vec<String>);

impl Path {
    fn with(&self, part: impl Into<String>) -> Path {
        let mut parts = self.0.clone();
        parts.push(part.into());
        Path(parts)
    }

    fn shown(&self) -> String {
        self.0.join(".")
    }

    /// The path of field `name` at the top of the input.
    pub fn field(name: &str) -> Path {
        Path(vec![name.to_string()])
    }
}

impl Codecs<'_> {
    fn codec(&self, id: CodecId) -> Option<&Codec> {
        self.codecs.get(id.0 as usize)
    }

    fn problem(problems: &mut Vec<Problem>, path: &Path, rule: &str) {
        problems.push(Problem {
            field: path.shown(),
            rule: rule.to_string(),
        });
    }

    /// The value `json` holds as a value of codec `id`'s type; `None` with
    /// the problems found when it isn't one.
    pub fn read(
        &self,
        id: CodecId,
        json: &serde_json::Value,
        path: &Path,
        problems: &mut Vec<Problem>,
    ) -> Option<Value> {
        let Some(codec) = self.codec(id) else {
            Self::problem(problems, path, "type");
            return None;
        };
        match codec {
            Codec::Nullable(inner) => match json {
                serde_json::Value::Null => Some(Value::None),
                _ => self.read(*inner, json, path, problems),
            },
            _ if json.is_null() => {
                Self::problem(problems, path, "required");
                None
            }
            Codec::Scalar {
                builtin, validator, ..
            } => {
                let value = match json {
                    serde_json::Value::Object(entries)
                        if builtin.representation() == Builtin::ZonedTime =>
                    {
                        zoned_from_json(entries, path, problems)?
                    }
                    serde_json::Value::String(text) => scalar_from_text(*builtin, text),
                    serde_json::Value::Bool(flag) if *builtin == Builtin::Bool => {
                        Ok(Value::Bool(*flag))
                    }
                    serde_json::Value::Number(number)
                        if builtin.representation() == Builtin::Int =>
                    {
                        number.as_i64().map(Value::Int).ok_or("type")
                    }
                    _ => Err("type"),
                };
                match value {
                    Ok(value) => self.checked(*validator, value, path, problems),
                    Err(rule) => {
                        Self::problem(problems, path, rule);
                        None
                    }
                }
            }
            Codec::Object { shape, fields } => {
                let serde_json::Value::Object(entries) = json else {
                    Self::problem(problems, path, "type");
                    return None;
                };
                let shape = self.shapes.get(shape.0 as usize)?;
                self.object(shape, fields, entries, path, problems)
            }
            Codec::Enum { ty, variants } => match json {
                serde_json::Value::String(name) => {
                    match variants
                        .iter()
                        .find(|v| v.wire == *name && v.fields.is_none())
                    {
                        Some(variant) => Some(Value::variant(
                            ty,
                            variant.ordinal,
                            &variant.name,
                            Vec::new(),
                        )),
                        None => {
                            Self::problem(problems, path, "variant");
                            None
                        }
                    }
                }
                serde_json::Value::Object(entries) => {
                    let Some(serde_json::Value::String(tag)) = entries.get("tag") else {
                        Self::problem(problems, &path.with("tag"), "required");
                        return None;
                    };
                    let Some(variant) = variants.iter().find(|v| v.name == *tag) else {
                        Self::problem(problems, &path.with("tag"), "variant");
                        return None;
                    };
                    let Some((shape, fields)) = &variant.fields else {
                        Self::problem(problems, path, "type");
                        return None;
                    };
                    let shape = self.shapes.get(shape.0 as usize)?;
                    let mut rest = entries.clone();
                    rest.remove("tag");
                    self.object(shape, fields, &rest, path, problems)
                }
                _ => {
                    Self::problem(problems, path, "type");
                    None
                }
            },
            Codec::List { item, validator } | Codec::Set { item, validator } => {
                let serde_json::Value::Array(items) = json else {
                    Self::problem(problems, path, "type");
                    return None;
                };
                let set = matches!(codec, Codec::Set { .. });
                let before = problems.len();
                let mut values = Vec::with_capacity(items.len());
                for (at, item_json) in items.iter().enumerate() {
                    if let Some(value) =
                        self.read(*item, item_json, &path.with(at.to_string()), problems)
                    {
                        values.push(value);
                    }
                }
                if problems.len() > before {
                    return None;
                }
                let value = if set {
                    let count = values.len();
                    let value = Value::set(values);
                    if value.items().map_or(0, <[Value]>::len) != count {
                        Self::problem(problems, path, "duplicate");
                        return None;
                    }
                    value
                } else {
                    Value::list(values)
                };
                self.checked(*validator, value, path, problems)
            }
            Codec::Map {
                key,
                value,
                validator,
            } => {
                let serde_json::Value::Object(entries) = json else {
                    Self::problem(problems, path, "type");
                    return None;
                };
                let before = problems.len();
                let mut map = BTreeMap::new();
                for (name, entry) in entries {
                    let at = path.with(name.clone());
                    let Some(k) = self.read_text(*key, name, &at, problems) else {
                        continue;
                    };
                    if let Some(v) = self.read(*value, entry, &at, problems) {
                        map.insert(k, v);
                    }
                }
                if problems.len() > before {
                    return None;
                }
                self.checked(*validator, Value::Map(Rc::new(map)), path, problems)
            }
            // A page is only ever an output, read back by a route test as
            // `Response.output`: `items` and the `next` cursor, as written.
            Codec::Page { item } => {
                let serde_json::Value::Object(entries) = json else {
                    Self::problem(problems, path, "type");
                    return None;
                };
                let Some(serde_json::Value::Array(listed)) = entries.get("items") else {
                    Self::problem(problems, &path.with("items"), "type");
                    return None;
                };
                let before = problems.len();
                let mut items = Vec::with_capacity(listed.len());
                for (at, entry) in listed.iter().enumerate() {
                    let at = path.with("items").with(at.to_string());
                    if let Some(value) = self.read(*item, entry, &at, problems) {
                        items.push(value);
                    }
                }
                if problems.len() > before {
                    return None;
                }
                let items = Value::list(items);
                let next = match entries.get("next") {
                    Some(serde_json::Value::String(cursor)) => Value::text(cursor),
                    None | Some(serde_json::Value::Null) => Value::None,
                    Some(_) => {
                        Self::problem(problems, &path.with("next"), "type");
                        return None;
                    }
                };
                Some(Value::record(
                    "Page",
                    vec![(Rc::from("items"), items), (Rc::from("next"), next)],
                ))
            }
        }
    }

    fn object(
        &self,
        shape: &Shape,
        fields: &[CodecId],
        entries: &serde_json::Map<String, serde_json::Value>,
        path: &Path,
        problems: &mut Vec<Problem>,
    ) -> Option<Value> {
        let before = problems.len();
        let mut given = Vec::new();
        for (slot, codec) in fields.iter().enumerate() {
            let Some((name, absent)) = shape.field(slot) else {
                continue;
            };
            let at = path.with(name);
            match entries.get(name) {
                None if absent => {}
                None => Self::problem(problems, &at, "required"),
                Some(serde_json::Value::Null) if shape.omittable(slot) => {
                    given.push((Some(slot as u32), Value::None));
                }
                Some(json) => {
                    if let Some(value) = self.read(*codec, json, &at, problems) {
                        given.push((Some(slot as u32), value));
                    }
                }
            }
        }
        for name in entries.keys() {
            if !(0..shape.len()).any(|slot| shape.field(slot).is_some_and(|(n, _)| n == name)) {
                Self::problem(problems, &path.with(name.clone()), "unknown_field");
            }
        }
        if problems.len() > before {
            return None;
        }
        shape.build(given)
    }

    /// The value `text` holds as a value of codec `id`'s type: a path
    /// placeholder, a query-string value, a header or a map's key.
    pub fn read_text(
        &self,
        id: CodecId,
        text: &str,
        path: &Path,
        problems: &mut Vec<Problem>,
    ) -> Option<Value> {
        match self.codec(id) {
            Some(Codec::Nullable(inner)) => self.read_text(*inner, text, path, problems),
            Some(Codec::Scalar {
                builtin, validator, ..
            }) => {
                let value = match builtin.representation() {
                    Builtin::Bool => match text {
                        "true" => Ok(Value::Bool(true)),
                        "false" => Ok(Value::Bool(false)),
                        _ => Err("type"),
                    },
                    Builtin::Int => text.parse::<i64>().map(Value::Int).map_err(|_| "type"),
                    _ => scalar_from_text(*builtin, text),
                };
                match value {
                    Ok(value) => self.checked(*validator, value, path, problems),
                    Err(rule) => {
                        Self::problem(problems, path, rule);
                        None
                    }
                }
            }
            Some(Codec::Enum { .. }) => self.read(
                id,
                &serde_json::Value::String(text.to_string()),
                path,
                problems,
            ),
            _ => {
                Self::problem(problems, path, "type");
                None
            }
        }
    }

    /// `values`, a field's values from a query string, each once for a
    /// scalar or as a list for a list type.
    pub fn read_query(
        &self,
        id: CodecId,
        values: &[String],
        path: &Path,
        problems: &mut Vec<Problem>,
    ) -> Option<Value> {
        let inner = match self.codec(id) {
            Some(Codec::Nullable(inner)) => *inner,
            _ => id,
        };
        match self.codec(inner) {
            Some(Codec::List { item, validator }) | Some(Codec::Set { item, validator }) => {
                let before = problems.len();
                let items: Vec<Value> = values
                    .iter()
                    .enumerate()
                    .filter_map(|(at, text)| {
                        self.read_text(*item, text, &path.with(at.to_string()), problems)
                    })
                    .collect();
                if problems.len() > before {
                    return None;
                }
                let value = match self.codec(inner) {
                    Some(Codec::Set { .. }) => {
                        let count = items.len();
                        let set = Value::set(items);
                        if set.items().map_or(0, <[Value]>::len) != count {
                            Self::problem(problems, path, "duplicate");
                            return None;
                        }
                        set
                    }
                    _ => Value::list(items),
                };
                self.checked(*validator, value, path, problems)
            }
            _ => match values {
                [one] => self.read_text(id, one, path, problems),
                _ => {
                    Self::problem(problems, path, "type");
                    None
                }
            },
        }
    }

    /// `value` stored at its type's scale and checked against its rules.
    fn checked(
        &self,
        validator: jadpo_std::types::ValidatorId,
        value: Value,
        path: &Path,
        problems: &mut Vec<Problem>,
    ) -> Option<Value> {
        let Some(validator) = self.validators.get(validator.0 as usize) else {
            return Some(value);
        };
        let Some(value) = validator.store(value) else {
            Self::problem(problems, path, "max");
            return None;
        };
        match validator.check(&value) {
            Ok(()) => Some(value),
            Err(invalid) => {
                Self::problem(problems, path, invalid.key().word());
                None
            }
        }
    }

    /// `value`, of codec `id`'s type, written as JSON onto `out`.
    pub fn write(&self, id: CodecId, value: &Value, out: &mut String) {
        let codec = self.codec(id);
        match (codec, value) {
            (_, Value::None) | (_, Value::Unit) => out.push_str("null"),
            (Some(Codec::Nullable(inner)), value) => self.write(*inner, value, out),
            (Some(Codec::Scalar { int_as_number, .. }), value) => {
                write_scalar(value, *int_as_number, out)
            }
            (Some(Codec::Object { fields, .. }), Value::Record(record)) => {
                out.push('{');
                let mut first = true;
                let omitted = record.omitted.as_deref().unwrap_or(&[]);
                for (at, ((name, field), codec)) in record.fields.iter().zip(fields).enumerate() {
                    // An optional `Input` field left out is left out on the
                    // wire too, so it never reads back as one sent as
                    // `null` (area 6, "patch").
                    if omitted.iter().any(|place| *place as usize == at) {
                        continue;
                    }
                    if !first {
                        out.push(',');
                    }
                    first = false;
                    write_string(name, out);
                    out.push(':');
                    self.write(*codec, field, out);
                }
                out.push('}');
            }
            (Some(Codec::Enum { variants, .. }), Value::Variant(variant)) => {
                let found = variants.iter().find(|v| v.name == *variant.name);
                match found.and_then(|v| v.fields.as_ref()) {
                    None => write_string(found.map_or(&*variant.name, |v| &v.wire), out),
                    Some((_, codecs)) => {
                        out.push_str("{\"tag\":");
                        write_string(&variant.name, out);
                        for ((name, field), codec) in variant.fields.iter().zip(codecs) {
                            out.push(',');
                            write_string(name, out);
                            out.push(':');
                            self.write(*codec, field, out);
                        }
                        out.push('}');
                    }
                }
            }
            (Some(Codec::List { item, .. } | Codec::Set { item, .. }), value) => {
                out.push('[');
                for (at, entry) in value.items().unwrap_or_default().iter().enumerate() {
                    if at > 0 {
                        out.push(',');
                    }
                    self.write(*item, entry, out);
                }
                out.push(']');
            }
            (Some(Codec::Map { value: codec, .. }), Value::Map(entries)) => {
                out.push('{');
                for (at, (key, entry)) in entries.iter().enumerate() {
                    if at > 0 {
                        out.push(',');
                    }
                    write_string(&key_text(key), out);
                    out.push(':');
                    self.write(*codec, entry, out);
                }
                out.push('}');
            }
            (Some(Codec::Page { item }), Value::Record(record)) => {
                out.push_str("{\"items\":[");
                let items = record
                    .get("items")
                    .and_then(Value::items)
                    .unwrap_or_default();
                for (at, entry) in items.iter().enumerate() {
                    if at > 0 {
                        out.push(',');
                    }
                    self.write(*item, entry, out);
                }
                out.push_str("],\"next\":");
                match record.get("next") {
                    Some(Value::Text(cursor)) => write_string(cursor, out),
                    _ => out.push_str("null"),
                }
                out.push('}');
            }
            (_, value) => write_scalar(value, true, out),
        }
    }
}

/// A `ZonedTime` from `{ "instant": ..., "zone": ... }` (area 10), each
/// problem reported at its own field.
fn zoned_from_json(
    entries: &serde_json::Map<String, serde_json::Value>,
    path: &Path,
    problems: &mut Vec<Problem>,
) -> Option<Result<Value, &'static str>> {
    let before = problems.len();
    let instant = match entries.get("instant") {
        Some(serde_json::Value::String(text)) => {
            let found = jadpo_std::instant_millis(text);
            if found.is_none() {
                Codecs::problem(problems, &path.with("instant"), "format");
            }
            found
        }
        Some(_) => {
            Codecs::problem(problems, &path.with("instant"), "type");
            None
        }
        None => {
            Codecs::problem(problems, &path.with("instant"), "required");
            None
        }
    };
    let zone = match entries.get("zone") {
        Some(serde_json::Value::String(name)) => {
            let found = jadpo_std::ZONES.iter().position(|(_, iana)| iana == name);
            if found.is_none() {
                Codecs::problem(problems, &path.with("zone"), "variant");
            }
            found.and_then(|at| u32::try_from(at).ok())
        }
        Some(_) => {
            Codecs::problem(problems, &path.with("zone"), "type");
            None
        }
        None => {
            Codecs::problem(problems, &path.with("zone"), "required");
            None
        }
    };
    for name in entries.keys() {
        if name != "instant" && name != "zone" {
            Codecs::problem(problems, &path.with(name.clone()), "unknown_field");
        }
    }
    match (instant, zone) {
        (Some(millis), Some(zone)) if problems.len() == before => {
            Some(Ok(Value::Zoned(crate::Zoned { millis, zone })))
        }
        _ => None,
    }
}

/// A scalar read from its text, before its rules are checked.
fn scalar_from_text(builtin: Builtin, text: &str) -> Result<Value, &'static str> {
    match builtin.representation() {
        Builtin::Text => Ok(Value::text(text)),
        Builtin::Int => text.parse::<i64>().map(Value::Int).map_err(|_| "type"),
        Builtin::Decimal => Decimal::parse(text).map(Value::Decimal).ok_or("type"),
        Builtin::Uuid => {
            jadpo_std::check_builtin_text(Builtin::Uuid, text).map_err(|_| "format")?;
            Ok(Value::uuid(text))
        }
        Builtin::Instant => jadpo_std::instant_millis(text)
            .map(Value::Instant)
            .ok_or("format"),
        Builtin::CalendarDate => jadpo_std::date_number(text)
            .map(Value::Date)
            .ok_or("format"),
        // A `ZonedTime` is an object (`read`), never text.
        Builtin::ZonedTime => Err("type"),
        Builtin::Duration => iso_duration(text).map(Value::Duration).ok_or("format"),
        Builtin::Bytes => base64_decode(text)
            .map(|bytes| Value::Bytes(Rc::from(bytes)))
            .ok_or("format"),
        Builtin::Bool => match text {
            "true" => Ok(Value::Bool(true)),
            "false" => Ok(Value::Bool(false)),
            _ => Err("type"),
        },
        _ => Err("type"),
    }
}

/// A map key as JSON's object keys hold it.
fn key_text(key: &Value) -> String {
    match key {
        Value::Text(text) | Value::Uuid(text) => text.to_string(),
        other => other.to_string(),
    }
}

fn write_scalar(value: &Value, int_as_number: bool, out: &mut String) {
    match value {
        Value::Bool(flag) => out.push_str(if *flag { "true" } else { "false" }),
        Value::Int(n) if int_as_number => {
            let _ = write!(out, "{n}");
        }
        Value::Int(n) => write_string(&n.to_string(), out),
        Value::Decimal(d) => write_string(&d.to_string(), out),
        Value::Text(text) | Value::Uuid(text) => write_string(text, out),
        Value::Instant(millis) => write_string(&jadpo_std::format_instant(*millis), out),
        Value::Date(day) => write_string(&jadpo_std::format_date(*day), out),
        Value::Zoned(zoned) => {
            out.push_str("{\"instant\":");
            write_string(&jadpo_std::format_instant(zoned.millis), out);
            out.push_str(",\"zone\":");
            write_string(zoned.zone_name(), out);
            out.push('}');
        }
        Value::Duration(millis) => write_string(&iso_duration_text(*millis), out),
        Value::Bytes(bytes) => write_string(&base64_encode(bytes), out),
        Value::None | Value::Unit => out.push_str("null"),
        other => write_string(&other.to_string(), out),
    }
}

/// `text` as a JSON string.
pub fn write_string(text: &str, out: &mut String) {
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// An ISO 8601 duration of days, hours, minutes and seconds (`PT5M`,
/// `P1DT2H`, `PT0.5S`), negative with a leading `-` as one is written
/// (`-PT5M`), in milliseconds.
fn iso_duration(text: &str) -> Option<i64> {
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
        let count: i64 = date_part.strip_suffix('D')?.parse().ok()?;
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

/// Milliseconds as an ISO 8601 duration: `PT5M`, `PT1.5S`, `PT0S`.
fn iso_duration_text(millis: i64) -> String {
    jadpo_std::iso_duration(millis)
}

const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64_encode(bytes: &[u8]) -> String {
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b = [
            chunk.first().copied().unwrap_or(0),
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                let index = ((n >> (18 - 6 * i)) & 63) as usize;
                out.push(BASE64.get(index).copied().map_or('A', char::from));
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn base64_decode(text: &str) -> Option<Vec<u8>> {
    let text = text.trim_end_matches('=');
    let mut out = Vec::new();
    let mut buffer: u32 = 0;
    let mut bits = 0;
    for c in text.bytes() {
        let value = BASE64.iter().position(|b| *b == c)? as u32;
        buffer = (buffer << 6) | value;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((buffer >> bits) & 0xFF) as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_read_and_write_as_iso_8601() {
        assert_eq!(iso_duration("PT5M"), Some(300_000));
        assert_eq!(iso_duration("P1DT2H"), Some(93_600_000));
        assert_eq!(iso_duration("PT1.5S"), Some(1_500));
        assert_eq!(iso_duration("PT"), None);
        assert_eq!(iso_duration("5m"), None);
        assert_eq!(iso_duration("-PT5M"), Some(-300_000));
        assert_eq!(iso_duration("--PT5M"), None);
        assert_eq!(iso_duration_text(-300_000), "-PT5M");
        assert_eq!(iso_duration_text(300_000), "PT5M");
        assert_eq!(iso_duration_text(1_500), "PT1.5S");
        assert_eq!(iso_duration_text(0), "PT0S");
    }

    #[test]
    fn bytes_round_trip_through_base64() {
        for bytes in [&b""[..], b"a", b"ab", b"abc", b"hello world"] {
            assert_eq!(base64_decode(&base64_encode(bytes)).as_deref(), Some(bytes));
        }
        assert_eq!(base64_encode(b"hello"), "aGVsbG8=");
    }

    #[test]
    fn values_read_with_their_rules_and_write_as_area_2_says() {
        use jadpo_std::types::{FieldPlan, ShapeId, ShapePlan, ValidatorId};
        use jadpo_std::{Num, Rules};
        let quantity = Validator::new(
            Some(Builtin::Int),
            vec![Rules {
                min: Num::parse("1"),
                max: Num::parse("100"),
                ..Rules::default()
            }],
        );
        let any_int = Validator::new(Some(Builtin::Int), Vec::new());
        let field = |name: &str, validator: u32, absent: Option<jadpo_std::Constant>| FieldPlan {
            key: format!("field Line.{name}"),
            name: name.to_string(),
            validator: ValidatorId(validator),
            omittable: absent.is_some(),
            absent,
            none_left_out: false,
            section: None,
        };
        let shape = Shape::new(&ShapePlan {
            ty: "Line".to_string(),
            variant: None,
            fields: vec![
                field("quantity", 0, None),
                field("total", 1, Some(jadpo_std::Constant::None)),
            ],
        });
        let codecs = [
            Codec::Scalar {
                builtin: Builtin::Int,
                validator: ValidatorId(0),
                int_as_number: true,
            },
            Codec::Scalar {
                builtin: Builtin::Int,
                validator: ValidatorId(1),
                int_as_number: false,
            },
            Codec::Object {
                shape: ShapeId(0),
                fields: vec![CodecId(0), CodecId(1)],
            },
        ];
        let validators = [quantity, any_int];
        let shapes = [shape];
        let codecs = Codecs {
            codecs: &codecs,
            validators: &validators,
            shapes: &shapes,
        };
        let read = |text: &str| {
            let mut problems = Vec::new();
            let json: serde_json::Value = serde_json::from_str(text).unwrap();
            let value = codecs.read(CodecId(2), &json, &Path::default(), &mut problems);
            (value, problems)
        };
        let (value, problems) = read(r#"{"quantity": 3, "total": "12"}"#);
        assert!(problems.is_empty(), "{problems:?}");
        let mut out = String::new();
        codecs.write(CodecId(2), &value.unwrap(), &mut out);
        assert_eq!(
            out, r#"{"quantity":3,"total":"12"}"#,
            "an unbounded Int is text"
        );
        // A field left out is written left out, and one sent as `null`
        // as `null`, so each reads back as it was.
        for (sent, written) in [
            (r#"{"quantity": 3}"#, r#"{"quantity":3}"#),
            (
                r#"{"quantity": 3, "total": null}"#,
                r#"{"quantity":3,"total":null}"#,
            ),
        ] {
            let (value, problems) = read(sent);
            assert!(problems.is_empty(), "{problems:?}");
            let mut out = String::new();
            codecs.write(CodecId(2), &value.unwrap(), &mut out);
            assert_eq!(out, written);
        }
        let (value, problems) = read(r#"{"quantity": 0, "colour": "red"}"#);
        assert!(value.is_none());
        let shown: Vec<(String, String)> =
            problems.into_iter().map(|p| (p.field, p.rule)).collect();
        assert_eq!(
            shown,
            [
                ("quantity".to_string(), "min".to_string()),
                ("colour".to_string(), "unknown_field".to_string())
            ]
        );
        let (_, problems) = read(r#"{"total": true}"#);
        let shown: Vec<(String, String)> =
            problems.into_iter().map(|p| (p.field, p.rule)).collect();
        assert_eq!(
            shown,
            [
                ("quantity".to_string(), "required".to_string()),
                ("total".to_string(), "type".to_string())
            ]
        );
    }

    #[test]
    fn a_page_reads_back_as_it_was_written() {
        let codecs = [
            Codec::Scalar {
                builtin: Builtin::Int,
                validator: jadpo_std::types::ValidatorId(0),
                int_as_number: true,
            },
            Codec::Page { item: CodecId(0) },
        ];
        let validators = [Validator::new(Some(Builtin::Int), Vec::new())];
        let codecs = Codecs {
            codecs: &codecs,
            validators: &validators,
            shapes: &[],
        };
        let page = Value::record(
            "Page",
            vec![
                (
                    Rc::from("items"),
                    Value::list(vec![Value::Int(1), Value::Int(2)]),
                ),
                (Rc::from("next"), Value::text("cursor")),
            ],
        );
        let mut out = String::new();
        codecs.write(CodecId(1), &page, &mut out);
        assert_eq!(out, r#"{"items":[1,2],"next":"cursor"}"#);
        let json: serde_json::Value = serde_json::from_str(&out).unwrap();
        let mut problems = Vec::new();
        let read = codecs.read(CodecId(1), &json, &Path::default(), &mut problems);
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(read, Some(page));
        let json: serde_json::Value =
            serde_json::from_str(r#"{"items":[1,"x"],"next":null}"#).unwrap();
        assert_eq!(
            codecs.read(CodecId(1), &json, &Path::default(), &mut problems),
            None
        );
        assert_eq!(problems[0].field, "items.1");
    }

    #[test]
    fn a_zoned_time_and_a_zone_are_sent_as_area_10_writes_them() {
        use jadpo_std::http::VariantCodec;
        let london = jadpo_std::variant_ordinal(jadpo_std::ZONE, "europe_london").unwrap();
        let codecs = [
            Codec::Scalar {
                builtin: Builtin::ZonedTime,
                validator: jadpo_std::types::ValidatorId(0),
                int_as_number: true,
            },
            Codec::Enum {
                ty: jadpo_std::ZONE.into(),
                variants: vec![VariantCodec {
                    name: "europe_london".into(),
                    wire: "Europe/London".into(),
                    ordinal: london,
                    fields: None,
                }],
            },
        ];
        let validators = [Validator::new(Some(Builtin::ZonedTime), Vec::new())];
        let codecs = Codecs {
            codecs: &codecs,
            validators: &validators,
            shapes: &[],
        };
        let zoned = Value::Zoned(crate::Zoned {
            millis: jadpo_std::instant_millis("2026-10-25T08:30:00Z").unwrap(),
            zone: london,
        });
        let mut out = String::new();
        codecs.write(CodecId(0), &zoned, &mut out);
        assert_eq!(
            out,
            r#"{"instant":"2026-10-25T08:30:00Z","zone":"Europe/London"}"#
        );
        let mut problems = Vec::new();
        let json: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(
            codecs.read(CodecId(0), &json, &Path::default(), &mut problems),
            Some(zoned)
        );
        let json: serde_json::Value =
            serde_json::from_str(r#"{"instant":"noon","zone":"Mars/Olympus","at":1}"#).unwrap();
        assert_eq!(
            codecs.read(CodecId(0), &json, &Path::default(), &mut problems),
            None
        );
        let found: Vec<(&str, &str)> = problems
            .iter()
            .map(|p| (p.field.as_str(), p.rule.as_str()))
            .collect();
        assert_eq!(
            found,
            [
                ("instant", "format"),
                ("zone", "variant"),
                ("at", "unknown_field")
            ]
        );
        let zone = crate::temporal::zone_value(london);
        let mut out = String::new();
        codecs.write(CodecId(1), &zone, &mut out);
        assert_eq!(out, r#""Europe/London""#);
        let mut problems = Vec::new();
        let json: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(
            codecs.read(CodecId(1), &json, &Path::default(), &mut problems),
            Some(zone)
        );
    }

    #[test]
    fn strings_escape_what_json_requires() {
        let mut out = String::new();
        write_string("say \"hi\"\n\u{1}", &mut out);
        assert_eq!(out, "\"say \\\"hi\\\"\\n\\u0001\"");
    }
}
