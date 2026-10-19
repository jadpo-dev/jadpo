//! Values crossing the boundary as JSON (area 2's forms; area 7, "One
//! input"), driven by the route table's codecs, so the interpreter and
//! compiled code read and write a request the same way. `json.read` and
//! `json.write` (area 19, "Data formats") are the same reading and writing
//! over a document in memory ([`Codecs::read_value`], [`Codecs::write_value`]),
//! with codecs the plan numbers per call.
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
use crate::{Validator, Value};

/// One problem with a request's input: the field and the rule.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
    pub field: String,
    pub rule: String,
}

/// Why `json.read` gave no value (area 19, "Data formats"): the two
/// conditions its `attempt` maps.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Unread {
    /// Not one JSON document, or bytes that aren't UTF-8.
    Malformed,
    /// A JSON document that isn't the type: the first problem found.
    Mismatch(Problem),
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

/// Where in the input a value is: field names, list positions and
/// [`MAP_ENTRY`] for a map's entry.
#[derive(Clone, Debug, Default)]
pub struct Path(Vec<String>);

/// What names a map's entry in a problem's path, in place of its key
/// (chunk 5.8a).
pub const MAP_ENTRY: &str = "<key>";

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
                let value = match (jadpo_std::standard::of_builtin(*builtin), json) {
                    // A standard type, as its entry says the contract sends
                    // it: its canonical text, or an object of its members,
                    // each problem at its member (chunk 5.5w3).
                    (Some(entry), json) => match crate::standard::json::read(entry, json) {
                        Ok(value) => Ok(value),
                        Err(refused) => {
                            for one in refused {
                                let at = match one.member {
                                    Some(member) => path.with(member),
                                    None => path.clone(),
                                };
                                Self::problem(problems, &at, one.rule);
                            }
                            return None;
                        }
                    },
                    (None, serde_json::Value::String(text)) => {
                        crate::standard::scalar_text(*builtin, text)
                    }
                    (None, serde_json::Value::Bool(flag))
                        if builtin.representation() == Builtin::Bool =>
                    {
                        Ok(Value::Bool(*flag))
                    }
                    (None, serde_json::Value::Number(number))
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
            // A geometry, as GeoJSON (chunk 5.5s).
            Codec::Standard { ty } if crate::geo::is_geojson(ty) => {
                match crate::geo::read(ty, json) {
                    Ok(value) => Some(value),
                    Err(refused) => {
                        let at = refused
                            .at
                            .into_iter()
                            .fold(path.clone(), |path, part| path.with(part));
                        Self::problem(problems, &at, refused.rule);
                        None
                    }
                }
            }
            Codec::Standard { .. } => match json {
                serde_json::Value::String(text) => self.read_text(id, text, path, problems),
                _ => {
                    Self::problem(problems, path, "type");
                    None
                }
            },
            // A vector's item: the nearest single-precision float to the
            // number, past which a decimal can't hold it (chunk 5.5s2).
            Codec::Float => match json {
                serde_json::Value::Number(number) => {
                    let written = number.to_string();
                    match jadpo_std::standard::ai::float(&written)
                        .and_then(|text| Value::decimal(&text))
                    {
                        Some(value) => Some(value),
                        None => {
                            let rule = if written.starts_with('-') {
                                "min"
                            } else {
                                "max"
                            };
                            Self::problem(problems, path, rule);
                            None
                        }
                    }
                }
                _ => {
                    Self::problem(problems, path, "type");
                    None
                }
            },
            Codec::Object {
                shape,
                fields,
                validator,
            } => {
                let serde_json::Value::Object(entries) = json else {
                    Self::problem(problems, path, "type");
                    return None;
                };
                let shape = self.shapes.get(shape.0 as usize)?;
                let value = self.object(shape, fields, entries, path, problems)?;
                match validator {
                    Some(validator) => self.checked(*validator, value, path, problems),
                    None => Some(value),
                }
            }
            Codec::Enum { ty, variants } => match json {
                serde_json::Value::String(name) => {
                    // A standard enum's code is read ignoring case (area
                    // 19, question 22): `gb` typed into a form is `GB`.
                    let coded = jadpo_std::standard::stored_by_code(ty).is_some()
                        || jadpo_std::standard_type(ty).is_some();
                    match variants.iter().find(|v| {
                        v.fields.is_none()
                            && (v.wire == *name || (coded && v.wire.eq_ignore_ascii_case(name)))
                    }) {
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
                // An entry is `<key>` in a problem's path, whether its key
                // or its value broke the rule: a key is a value, an email
                // address as often as not, and a problem never holds one
                // (chunk 5.8a).
                let at = path.with(MAP_ENTRY);
                for (name, entry) in entries {
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
            // A GeoJSON feature and a collection of them (area 20, chunk
            // 5.5u6), read strictly as RFC 7946 §3.2 and §3.3 write them.
            Codec::Feature {
                geometry,
                properties,
            } => {
                let entries = Self::geojson_object(
                    json,
                    jadpo_std::standard::geo::FEATURE,
                    &["geometry", "properties"],
                    path,
                    problems,
                )?;
                let before = problems.len();
                let geometry = self.read(
                    *geometry,
                    entries.get("geometry").unwrap_or(&serde_json::Value::Null),
                    &path.with("geometry"),
                    problems,
                );
                let properties = self.read(
                    *properties,
                    entries
                        .get("properties")
                        .unwrap_or(&serde_json::Value::Null),
                    &path.with("properties"),
                    problems,
                );
                match (geometry, properties) {
                    (Some(geometry), Some(properties)) if problems.len() == before => {
                        Some(Value::record(
                            jadpo_std::standard::geo::FEATURE,
                            vec![
                                (Rc::from("geometry"), geometry),
                                (Rc::from("properties"), properties),
                            ],
                        ))
                    }
                    _ => None,
                }
            }
            Codec::FeatureCollection { feature } => {
                let entries = Self::geojson_object(
                    json,
                    jadpo_std::standard::geo::FEATURE_COLLECTION,
                    &["features"],
                    path,
                    problems,
                )?;
                let at = path.with("features");
                let listed = match entries.get("features") {
                    Some(serde_json::Value::Array(listed)) => listed,
                    None | Some(serde_json::Value::Null) => {
                        Self::problem(problems, &at, "required");
                        return None;
                    }
                    Some(_) => {
                        Self::problem(problems, &at, "type");
                        return None;
                    }
                };
                let before = problems.len();
                let mut features = Vec::with_capacity(listed.len());
                for (n, entry) in listed.iter().enumerate() {
                    if let Some(value) =
                        self.read(*feature, entry, &at.with(n.to_string()), problems)
                    {
                        features.push(value);
                    }
                }
                if problems.len() > before {
                    return None;
                }
                Some(Value::record(
                    jadpo_std::standard::geo::FEATURE_COLLECTION,
                    vec![(Rc::from("features"), Value::list(features))],
                ))
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

    /// A GeoJSON object whose `type` is `ty`, with `members` besides and
    /// the `bbox` RFC 7946 §5 lets any object carry, checked as four or six
    /// numbers and otherwise ignored, as a geometry's is (chunk 5.5s); any
    /// other member is `unknown_field`, an `id` included, since an
    /// application's identity goes in its properties (area 20, question 6).
    fn geojson_object<'j>(
        json: &'j serde_json::Value,
        ty: &str,
        members: &[&str],
        path: &Path,
        problems: &mut Vec<Problem>,
    ) -> Option<&'j serde_json::Map<String, serde_json::Value>> {
        let serde_json::Value::Object(entries) = json else {
            Self::problem(problems, path, "type");
            return None;
        };
        let before = problems.len();
        for name in entries.keys() {
            if name != "type" && name != "bbox" && !members.contains(&name.as_str()) {
                Self::problem(problems, &path.with(name.clone()), "unknown_field");
            }
        }
        match entries.get("type") {
            Some(serde_json::Value::String(given)) if given == ty => {}
            Some(serde_json::Value::String(_)) => {
                Self::problem(problems, &path.with("type"), "variant")
            }
            None | Some(serde_json::Value::Null) => {
                Self::problem(problems, &path.with("type"), "required")
            }
            Some(_) => Self::problem(problems, &path.with("type"), "type"),
        }
        if let Some(bbox) = entries.get("bbox") {
            let numbers = bbox.as_array().is_some_and(|items| {
                matches!(items.len(), 4 | 6) && items.iter().all(serde_json::Value::is_number)
            });
            if !numbers {
                Self::problem(problems, &path.with("bbox"), "type");
            }
        }
        (problems.len() == before).then_some(entries)
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
            // A standard object's part named for a reserved word is sent
            // without its `_` (chunk 5.5n).
            let name = jadpo_std::standard::wire_name(shape.ty(), name);
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
            if !(0..shape.len()).any(|slot| {
                shape
                    .field(slot)
                    .is_some_and(|(n, _)| jadpo_std::standard::wire_name(shape.ty(), n) == name)
            }) {
                // A key set's members a `Jwk` doesn't hold are ignored, as
                // RFC 7517 §4 requires, but a private key's (chunk 5.5r4).
                if shape.ty() == jadpo_std::standard::security::JWK
                    && jadpo_std::standard::jwk::ignores_member(name)
                {
                    continue;
                }
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
                // A whole number with a text form of its own (an HTTP
                // status, basis points) is read as `text.parse` reads it.
                let whole = self
                    .validators
                    .get(validator.0 as usize)
                    .and_then(Validator::standard)
                    .and_then(jadpo_std::standard_type)
                    .filter(|entry| {
                        matches!(entry.holds, jadpo_std::standard::Holds::Whole { .. })
                    });
                let value = match whole {
                    Some(entry) => crate::standard::read(entry, text).map_err(|_| "type"),
                    None => crate::standard::scalar_text(*builtin, text),
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
            // A standard object sent as its fields, or a range as its ends,
            // in a place that holds text (a query string, a setting, a
            // map's key): its canonical text, read as `text.parse` reads
            // it (chunk 5.5w2), then checked by its rules across its parts.
            Some(Codec::Object {
                shape,
                fields,
                validator,
            }) => {
                let ty = self.shapes.get(shape.0 as usize).map(Shape::ty);
                let read_as = match ty {
                    Some(jadpo_std::RANGE) => {
                        fields.first().and_then(|end| match self.codec(*end) {
                            Some(Codec::Nullable(inner)) => match self.codec(*inner) {
                                Some(Codec::Scalar { builtin, .. }) => Some(format!(
                                    "{}<{}>",
                                    jadpo_std::RANGE,
                                    builtin.representation().name()
                                )),
                                _ => None,
                            },
                            _ => None,
                        })
                    }
                    Some(ty) => jadpo_std::standard_type(ty)
                        .filter(|entry| entry.text.is_some())
                        .map(|entry| entry.name.to_string()),
                    None => None,
                };
                let Some(read_as) = read_as else {
                    Self::problem(problems, path, "type");
                    return None;
                };
                match crate::standard::parse(text, &read_as) {
                    Ok(value) => match crate::across::broken_standard(&value) {
                        Some(rule) => {
                            Self::problem(problems, path, rule);
                            None
                        }
                        None => match validator {
                            Some(validator) => self.checked(*validator, value, path, problems),
                            None => Some(value),
                        },
                    },
                    Err(_) => {
                        Self::problem(problems, path, "format");
                        None
                    }
                }
            }
            // Its canonical text, read only as it is written.
            Some(Codec::Standard { ty }) => match crate::standard::parse(text, ty) {
                // A standard object sent as its text keeps its rules across
                // its parts, each reported by its name at its path (area
                // 19, chunk 5.5t2).
                Ok(value) => match crate::across::broken_standard(&value) {
                    Some(rule) => {
                        Self::problem(problems, path, rule);
                        None
                    }
                    None => Some(value),
                },
                Err(_) => {
                    Self::problem(problems, path, "format");
                    None
                }
            },
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
            // A rule across an object's fields is reported by its name, at
            // the object's path (area 2, "Rules across fields").
            Err(invalid) => {
                Self::problem(
                    problems,
                    path,
                    invalid.name().unwrap_or(invalid.key().word()),
                );
                None
            }
        }
    }

    /// `json.read` (area 19, "Data formats"): the value of codec `id`'s
    /// type the JSON document `text` holds, read with an `Input`'s rules
    /// as a request's body is. `text` is `Text`, or `Bytes` in UTF-8.
    /// Text that isn't one JSON document, or bytes that aren't UTF-8, are
    /// `malformed`; a document that isn't the type is `mismatch`, with
    /// the first problem found, its field's path and the rule, never the
    /// value.
    pub fn read_value(&self, id: CodecId, text: &Value) -> Result<Value, Unread> {
        let text = match text {
            Value::Text(text) => &**text,
            Value::Bytes(bytes) => std::str::from_utf8(bytes).map_err(|_| Unread::Malformed)?,
            _ => return Err(Unread::Malformed),
        };
        let json: serde_json::Value = serde_json::from_str(text).map_err(|_| Unread::Malformed)?;
        let mut problems = Vec::new();
        let value = self.read(id, &json, &Path::default(), &mut problems);
        match (value, problems.into_iter().next()) {
            (Some(value), None) => Ok(value),
            (_, Some(problem)) => Err(Unread::Mismatch(problem)),
            (None, None) => Err(Unread::Mismatch(Problem {
                field: String::new(),
                rule: "type".to_string(),
            })),
        }
    }

    /// `json.write` (area 19, "Data formats"): `value`, of codec `id`'s
    /// type, as compact JSON, as a response's body is written: fields in
    /// declared order, sets and map keys in the canonical order, no white
    /// space, so the same value always gives the same text, on every
    /// target.
    pub fn write_value(&self, id: CodecId, value: &Value) -> String {
        let mut out = String::new();
        self.write(id, value, &mut out);
        out
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
                // A problem's members that are `none` are left out, as RFC
                // 9457 sends them (chunk 5.5n).
                let members = jadpo_std::standard_type(&record.ty).is_some_and(|entry| {
                    matches!(entry.json, jadpo_std::standard::Json::Members(_))
                });
                for (at, ((name, field), codec)) in record.fields.iter().zip(fields).enumerate() {
                    // An optional `Input` field left out is left out on the
                    // wire too, so it never reads back as one sent as
                    // `null` (area 6, "patch").
                    if omitted.iter().any(|place| *place as usize == at)
                        || (members && *field == Value::None)
                    {
                        continue;
                    }
                    if !first {
                        out.push(',');
                    }
                    first = false;
                    write_string(jadpo_std::standard::wire_name(&record.ty, name), out);
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
            (
                Some(Codec::Map {
                    key: key_codec,
                    value: codec,
                    ..
                }),
                Value::Map(entries),
            ) => {
                out.push('{');
                for (at, (key, entry)) in entries.iter().enumerate() {
                    if at > 0 {
                        out.push(',');
                    }
                    // An enum's key is its wire text, as its value would be
                    // written: a `LocalisedText`'s locale is its tag (chunk
                    // 5.5o2).
                    match (self.codec(*key_codec), key) {
                        (Some(Codec::Enum { variants, .. }), Value::Variant(variant)) => {
                            let found = variants.iter().find(|v| v.name == *variant.name);
                            write_string(found.map_or(&*variant.name, |v| &v.wire), out);
                        }
                        _ => write_string(&key_text(key), out),
                    }
                    out.push(':');
                    self.write(*codec, entry, out);
                }
                out.push('}');
            }
            (Some(Codec::Standard { ty }), value) if crate::geo::is_geojson(ty) => {
                if !crate::geo::write(value, out) {
                    out.push_str("null");
                }
            }
            (Some(Codec::Standard { .. }), value) => match crate::standard::from(value) {
                Ok(Value::Text(text)) => write_string(&text, out),
                _ => out.push_str("null"),
            },
            // A vector's item, as the number it holds (chunk 5.5s2).
            (Some(Codec::Float), Value::Decimal(decimal)) => out.push_str(&decimal.to_string()),
            (
                Some(Codec::Feature {
                    geometry,
                    properties,
                }),
                Value::Record(record),
            ) => {
                out.push_str("{\"type\":\"Feature\",\"geometry\":");
                self.write(
                    *geometry,
                    record.get("geometry").unwrap_or(&Value::None),
                    out,
                );
                out.push_str(",\"properties\":");
                self.write(
                    *properties,
                    record.get("properties").unwrap_or(&Value::None),
                    out,
                );
                out.push('}');
            }
            (Some(Codec::FeatureCollection { feature }), Value::Record(record)) => {
                out.push_str("{\"type\":\"FeatureCollection\",\"features\":[");
                let features = record
                    .get("features")
                    .and_then(Value::items)
                    .unwrap_or_default();
                for (at, entry) in features.iter().enumerate() {
                    if at > 0 {
                        out.push(',');
                    }
                    self.write(*feature, entry, out);
                }
                out.push_str("]}");
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

/// A map key as JSON's object keys hold it: text as it is, anything else
/// as its canonical text, as `text.from` writes it and a key is read back
/// (chunk 5.5w2). The checker refuses a key type with no single text form,
/// so the last arm is only a value the checker let through by mistake.
fn key_text(key: &Value) -> String {
    match key {
        Value::Text(text) => text.to_string(),
        other => match crate::standard::from(other) {
            Ok(Value::Text(text)) => text.to_string(),
            _ => other.to_string(),
        },
    }
}

/// A scalar written as JSON: a representation as area 2 writes it, and a
/// standard type as the contract sends it, by its entry (chunk 5.5w3).
fn write_scalar(value: &Value, int_as_number: bool, out: &mut String) {
    match value {
        Value::Bool(flag) => out.push_str(if *flag { "true" } else { "false" }),
        Value::Int(n) if int_as_number => {
            let _ = write!(out, "{n}");
        }
        Value::Int(n) => write_string(&n.to_string(), out),
        Value::Decimal(d) => write_string(&d.to_string(), out),
        Value::Text(text) => write_string(text, out),
        Value::Bytes(bytes) => write_string(&crate::encoding::to_base64(bytes), out),
        Value::None | Value::Unit => out.push_str("null"),
        other => match crate::standard::entry_of(other) {
            Some(entry) => crate::standard::json::write(entry, other, out),
            None => write_string(&other.to_string(), out),
        },
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_are_padded_base64_both_ways() {
        let codecs = [Codec::Scalar {
            builtin: Builtin::Bytes,
            validator: jadpo_std::types::ValidatorId(0),
            int_as_number: true,
        }];
        let validators = [Validator::new(Some(Builtin::Bytes), Vec::new())];
        let codecs = Codecs {
            codecs: &codecs,
            validators: &validators,
            shapes: &[],
        };
        let hello = Value::Bytes(Rc::from(&b"hello"[..]));
        assert_eq!(codecs.write_value(CodecId(0), &hello), r#""aGVsbG8=""#);
        assert_eq!(
            codecs.read_value(CodecId(0), &Value::text(r#""aGVsbG8=""#)),
            Ok(hello)
        );
        // Only the text `encoding.to_base64` writes reads back.
        assert_eq!(
            codecs.read_value(CodecId(0), &Value::text(r#""aGVsbG8""#)),
            Err(Unread::Mismatch(Problem {
                field: String::new(),
                rule: "format".to_string()
            }))
        );
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
                validator: None,
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

    /// A map's key is a value, an email address as often as not, so a
    /// problem in an entry names it `<key>`, never by its text, whether
    /// the key or its value broke the rule, at any depth (chunk 5.8a).
    #[test]
    fn a_problem_in_a_map_names_its_entry_never_its_key() {
        use jadpo_std::types::ValidatorId;
        use jadpo_std::{Num, Rules};
        let score = Validator::new(
            Some(Builtin::Int),
            vec![Rules {
                max: Num::parse("100"),
                ..Rules::default()
            }],
        );
        let short = Validator::new(
            Some(Builtin::Text),
            vec![Rules {
                max_length: Some(10),
                ..Rules::default()
            }],
        );
        let codecs = [
            Codec::Scalar {
                builtin: Builtin::Int,
                validator: ValidatorId(0),
                int_as_number: true,
            },
            Codec::Scalar {
                builtin: Builtin::Text,
                validator: ValidatorId(1),
                int_as_number: true,
            },
            // No validator of its own: the map has no rules.
            Codec::Map {
                key: CodecId(1),
                value: CodecId(0),
                validator: ValidatorId(9),
            },
            Codec::Map {
                key: CodecId(1),
                value: CodecId(2),
                validator: ValidatorId(9),
            },
        ];
        let validators = [score, short];
        let codecs = Codecs {
            codecs: &codecs,
            validators: &validators,
            shapes: &[],
        };
        let read = |id: u32, text: &str| {
            let mut problems = Vec::new();
            let json: serde_json::Value = serde_json::from_str(text).unwrap();
            let value = codecs.read(CodecId(id), &json, &Path::field("scores"), &mut problems);
            assert!(value.is_none(), "{text}");
            problems
                .into_iter()
                .map(|p| (p.field, p.rule))
                .collect::<Vec<_>>()
        };
        let pair = |field: &str, rule: &str| (field.to_string(), rule.to_string());
        assert_eq!(
            read(2, r#"{"ann@ex.io": 101, "bo@ex.io": 5}"#),
            [pair("scores.<key>", "max")],
            "a value's rule"
        );
        assert_eq!(
            read(2, r#"{"alice@example.com": 5}"#),
            [pair("scores.<key>", "max_length")],
            "a key's rule"
        );
        assert_eq!(
            read(3, r#"{"ann": {"bo": "x"}}"#),
            [pair("scores.<key>.<key>", "type")],
            "a map in a map"
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
