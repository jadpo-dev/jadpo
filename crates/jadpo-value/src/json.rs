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
use jadpo_std::http::{Codec, CodecId, WireCoding};

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
    /// On a provider's wire (area 11, "Decoding"; chunk 7.2c2): what runs
    /// a field's own decoder and encoder. A provider package's field that
    /// says how it is sent is read and written as it says only here, and
    /// as the contract writes its type everywhere else.
    pub coder: Option<&'a dyn Coder>,
}

/// What reads and writes a provider's wire runs the package's own field
/// decoders and encoders with ([`Codecs::coder`]): the executor, at the
/// call's site (area 11, "Decoding"; chunk 7.2c2).
pub trait Coder: std::fmt::Debug {
    /// The field's value from `wire`, what the provider sent, by the
    /// package's function with identity key `function`; `None` when it
    /// doesn't decode.
    fn decode(&self, function: &str, wire: Value) -> Option<Value>;

    /// What the provider takes for the field's `value`, by the package's
    /// function with identity key `function`; `None` when it doesn't
    /// encode.
    fn encode(&self, function: &str, value: Value) -> Option<Value>;

    /// A value its built-in encoding can't hold, by the rule it broke: it
    /// was written as `null`, and the call goes no further.
    fn unwritten(&self, rule: &str);
}

mod doc;
mod document;
pub mod reading;
mod wire;

pub use doc::{Doc, Item, Items, Kind, Members};
pub use document::{Numbers, parse};

/// What a provider sent in `encoding`, one of the built-in encodings, as
/// the contract's JSON for its type: what reads a value from outside a
/// type's own fields, a webhook's time (area 11, "Receiving from outside";
/// chunk 7.7). The rule broken when it isn't one.
pub fn from_wire(
    encoding: jadpo_std::provider::WireEncoding,
    sent: &serde_json::Value,
) -> Result<serde_json::Value, &'static str> {
    wire::read(encoding, None, sent, None)
}

/// A standard type's part and the member of the object holding its field
/// it is spelled as on a provider's wire (`wire_parts:`, chunk 7.2e).
type Spelled = (String, String);

/// `contract`, the contract's JSON for a value of standard type
/// `standard`, as the members a field's `wire_parts:` sends it as (area
/// 11; chunk 7.2e): each member's spelling and what it holds.
pub fn parts_members(
    standard: &str,
    contract: &serde_json::Value,
    parts: &[(String, String)],
) -> Vec<(String, serde_json::Value)> {
    wire::parts_write(standard, contract, parts)
}

pub use wire::instant_from_wire;

/// Where in the input a value is: field names, list positions and
/// [`MAP_ENTRY`] for a map's entry, as a problem shows it; with where it
/// stands in the document itself and the document's numbers as written,
/// when it was read from text (chunk 5.8c).
#[derive(Clone, Debug, Default)]
pub struct Path {
    shown: Vec<String>,
    at: Vec<String>,
    numbers: Option<Rc<Numbers>>,
}

/// What names a map's entry in a problem's path, in place of its key
/// (chunk 5.8a).
pub const MAP_ENTRY: &str = "<key>";

impl Path {
    fn with(&self, part: impl Into<String>) -> Path {
        let part = part.into();
        let mut path = self.clone();
        path.shown.push(part.clone());
        path.at.push(part);
        path
    }

    /// A map's entry under `key`: shown as [`MAP_ENTRY`], never its key.
    fn entry(&self, key: &str) -> Path {
        let mut path = self.clone();
        path.shown.push(MAP_ENTRY.to_string());
        path.at.push(key.to_string());
        path
    }

    fn shown(&self) -> String {
        self.shown.join(".")
    }

    /// The text of the number here as the document wrote it.
    fn written_number(&self) -> Option<&str> {
        self.numbers.as_ref()?.get(&self.at)
    }

    /// The path of field `name` at the top of the input.
    pub fn field(name: &str) -> Path {
        Path {
            shown: vec![name.to_string()],
            at: vec![name.to_string()],
            numbers: None,
        }
    }

    /// The same place in a document whose numbers were written as
    /// `numbers` says ([`parse`]).
    pub fn numbered(mut self, numbers: &Numbers) -> Path {
        self.numbers = Some(Rc::new(numbers.clone()));
        self
    }

    /// The place `parts` names from the top, in a document whose numbers
    /// are `numbers`: where a generated reader hands a part of a document
    /// to [`Codecs::read`] (chunk 8.1p).
    pub fn of(parts: Vec<String>, numbers: Option<Rc<Numbers>>) -> Path {
        Path {
            shown: parts.clone(),
            at: parts,
            numbers,
        }
    }
}

/// What an object's field is given, by its member (area 2), as
/// [`presence`] decides it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Presence {
    /// Nothing: it holds what it holds when absent.
    Left,
    /// `none`.
    None,
    /// Nothing, though it must be given: `required`.
    Missing,
    /// Its member's value, read by its codec.
    Read,
}

/// What a field is given when its member is missing (`None`), `null`
/// (`Some(true)`) or anything else: left to what it holds when `absent`
/// says it holds something; `none` when a provider's open shape leaves
/// out a nullable member (chunk 7.7c) or an optional `Input` field without
/// a default, `omittable`, is sent as `null`; otherwise read, or missing.
/// The one rule both the codec walk and compiled code's readers follow.
pub fn presence(
    member: Option<bool>,
    absent: bool,
    open_nullable: bool,
    omittable: bool,
) -> Presence {
    match member {
        None if absent => Presence::Left,
        None if open_nullable => Presence::None,
        None => Presence::Missing,
        Some(true) if omittable => Presence::None,
        Some(_) => Presence::Read,
    }
}

/// A scalar of `builtin` from `item`, as the contract sends one: a
/// standard type's canonical text by its entry (chunk 5.5w3), or a
/// representation's text, `true` or `false`, or a whole number; the rule
/// broken when it isn't one. `None` for a standard type sent as an object
/// of its members, which [`Codecs::read`] reads member by member. The one
/// reader of scalars both the codec walk and compiled code's readers call.
pub fn scalar(builtin: Builtin, item: Item<'_>) -> Option<Result<Value, &'static str>> {
    use jadpo_std::standard::{Holds, Json};
    match jadpo_std::standard::of_builtin(builtin) {
        Some(entry) => match (entry.json, entry.holds) {
            (Json::Parts(_), _) => None,
            (_, Holds::Builtin(held)) => Some(match item.text() {
                Some(text) if item.kind() == Kind::String => {
                    crate::standard::scalar_text(held, &text)
                }
                _ => Err("type"),
            }),
            _ => Some(Err("type")),
        },
        None => Some(match item.kind() {
            Kind::String => match item.text() {
                Some(text) => crate::standard::scalar_text(builtin, &text),
                None => Err("type"),
            },
            Kind::Bool if builtin.representation() == Builtin::Bool => {
                item.as_bool().map(Value::Bool).ok_or("type")
            }
            Kind::Number if builtin.representation() == Builtin::Int => {
                item.as_i64().map(Value::Int).ok_or("type")
            }
            _ => Err("type"),
        }),
    }
}

/// `values`, read in order, as a list, or as a set when `set`: `None` for
/// a set that repeats one (area 2, question 5).
pub fn collected(values: Vec<Value>, set: bool) -> Option<Value> {
    if !set {
        return Some(Value::list(values));
    }
    let count = values.len();
    let value = Value::set(values);
    (value.items().map_or(0, <[Value]>::len) == count).then_some(value)
}

/// A value its type's rules refused: the rule, as a problem names it, and
/// a vector's item that broke it, by its place (chunk 5.8c).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Broken {
    pub rule: String,
    pub item: Option<usize>,
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
            // A provider package's field, as the provider sends it (chunk
            // 7.2c2), and as the contract writes it anywhere else.
            Codec::Wired { contract, coding } => match (self.coder, coding) {
                (None, _) => self.read(*contract, json, path, problems),
                _ if json.is_null() => self.read(*contract, json, path, problems),
                (Some(_), WireCoding::Encoding { encoding, currency }) => {
                    match wire::read(*encoding, currency.as_deref(), json, path.written_number()) {
                        Ok(contract_json) => self.read(*contract, &contract_json, path, problems),
                        Err(rule) => {
                            Self::problem(problems, path, rule);
                            None
                        }
                    }
                }
                // Read from its object's members by the object, which
                // gives the contract's JSON (chunk 7.2e).
                (Some(_), WireCoding::Parts { .. }) => self.read(*contract, json, path, problems),
                (Some(coder), WireCoding::Functions { decode, wire, .. }) => {
                    let sent = self.read(*wire, json, path, problems)?;
                    let decoded = coder.decode(decode, sent);
                    if decoded.is_none() {
                        Self::problem(problems, path, "decode");
                    }
                    decoded
                }
            },
            _ if json.is_null() => {
                Self::problem(problems, path, "required");
                None
            }
            Codec::Scalar {
                builtin, validator, ..
            } => {
                let value = match scalar(*builtin, Item::Json(json)) {
                    Some(value) => value,
                    // A standard type sent as an object of its members,
                    // each problem at its member (chunk 5.5w3).
                    None => match jadpo_std::standard::of_builtin(*builtin)
                        .map(|entry| crate::standard::json::read(entry, json))
                    {
                        Some(Ok(value)) => Ok(value),
                        Some(Err(refused)) => {
                            for one in refused {
                                let at = match one.member {
                                    Some(member) => path.with(member),
                                    None => path.clone(),
                                };
                                Self::problem(problems, &at, one.rule);
                            }
                            return None;
                        }
                        None => Err("type"),
                    },
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
                    // The number's own text where the document's is known,
                    // so it is rounded to a float once (chunk 5.8c).
                    let written = path
                        .written_number()
                        .map_or_else(|| number.to_string(), str::to_string);
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
                    // A provider package's variant is sent as its
                    // `wire_name:` (area 11, chunk 7.2c).
                    let Some(variant) = variants.iter().find(|v| v.wire == *tag || v.name == *tag)
                    else {
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
                let Some(value) = collected(values, set) else {
                    Self::problem(problems, path, "duplicate");
                    return None;
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
                for (name, entry) in entries {
                    let at = path.entry(name);
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

    /// `json` read as codec `id`'s object, only the fields named in
    /// `only` read and checked, each other holding `none` (area 11,
    /// "Decoding": a provider's answer decoded lazily, only the fields
    /// code reads; chunk 7.2c2). Its rules across its fields are checked
    /// only when every field is read. Anything but an object, or `null` for
    /// one that may be left out, is read whole.
    pub fn read_fields(
        &self,
        id: CodecId,
        json: &serde_json::Value,
        path: &Path,
        problems: &mut Vec<Problem>,
        only: &[String],
    ) -> Option<Value> {
        let (id, nullable) = match self.codec(id) {
            Some(Codec::Nullable(inner)) => (*inner, true),
            _ => (id, false),
        };
        let (Some(Codec::Object { shape, fields, .. }), serde_json::Value::Object(entries)) =
            (self.codec(id), json)
        else {
            return match (nullable, json) {
                (true, serde_json::Value::Null) => Some(Value::None),
                _ => self.read(id, json, path, problems),
            };
        };
        let shape = self.shapes.get(shape.0 as usize)?;
        let every = (0..shape.len())
            .filter_map(|slot| shape.field(slot))
            .all(|(name, _)| only.iter().any(|read| read == name));
        if every {
            return self.read(id, json, path, problems);
        }
        let before = problems.len();
        let mut given = Vec::new();
        for (slot, codec) in fields.iter().enumerate() {
            let Some((name, absent)) = shape.field(slot) else {
                continue;
            };
            if !only.iter().any(|read| read == name) {
                given.push((Some(slot as u32), Value::None));
                continue;
            }
            if let Some(read) = self.parts(*codec, entries, path, absent, problems) {
                if let Some(value) = read {
                    given.push((Some(slot as u32), value));
                }
                continue;
            }
            let Some(member) = shape.json_name(slot) else {
                continue;
            };
            let at = path.with(member);
            match entries.get(member) {
                None if absent => {}
                None if shape.open() && matches!(self.codec(*codec), Some(Codec::Nullable(_))) => {
                    given.push((Some(slot as u32), Value::None));
                }
                None => Self::problem(problems, &at, "required"),
                Some(json) => {
                    if let Some(value) = self.read(*codec, json, &at, problems) {
                        given.push((Some(slot as u32), value));
                    }
                }
            }
        }
        if problems.len() > before {
            return None;
        }
        shape.build(given)
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
            let Some((_, absent)) = shape.field(slot) else {
                continue;
            };
            if let Some(read) = self.parts(*codec, entries, path, absent, problems) {
                if let Some(value) = read {
                    given.push((Some(slot as u32), value));
                }
                continue;
            }
            // A standard object's part named for a reserved word is sent
            // without its `_` (chunk 5.5n), and a provider package's field
            // by its `wire_name:` (chunk 7.2c).
            let Some(name) = shape.json_name(slot) else {
                continue;
            };
            let at = path.with(name);
            let member = entries.get(name);
            // A provider leaving out a member its type may hold `none` in
            // sends `none` (chunk 7.7c): SQS leaves `Messages` out when its
            // queue is empty.
            let open_nullable =
                shape.open() && matches!(self.codec(*codec), Some(Codec::Nullable(_)));
            match presence(
                member.map(serde_json::Value::is_null),
                absent,
                open_nullable,
                shape.omittable(slot),
            ) {
                Presence::Left => {}
                Presence::None => given.push((Some(slot as u32), Value::None)),
                Presence::Missing => Self::problem(problems, &at, "required"),
                Presence::Read => {
                    if let Some(json) = member
                        && let Some(value) = self.read(*codec, json, &at, problems)
                    {
                        given.push((Some(slot as u32), value));
                    }
                }
            }
        }
        for name in entries.keys() {
            // A provider adding members breaks nothing (area 11, chunk
            // 7.2c): its type's are read and the rest left out.
            if shape.open() {
                break;
            }
            if !(0..shape.len()).any(|slot| shape.json_name(slot) == Some(name.as_str()))
                && !fields.iter().any(|codec| {
                    self.parts_of(*codec)
                        .is_some_and(|(_, _, parts)| parts.iter().any(|(_, at)| at == name))
                })
            {
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

    /// On a provider's wire, field codec `id`'s `wire_parts:` (chunk
    /// 7.2e): the contract's codec, its standard type and each part's
    /// spelling.
    fn parts_of(&self, id: CodecId) -> Option<(CodecId, &str, &[Spelled])> {
        self.coder?;
        match self.codec(id)? {
            Codec::Wired {
                contract,
                coding: WireCoding::Parts { standard, parts },
            } => Some((*contract, standard.as_str(), parts.as_slice())),
            _ => None,
        }
    }

    /// Field codec `id` read from its standard type's parts among
    /// `entries`, the members of the object holding it, when it gives
    /// `wire_parts:` (chunk 7.2e): `None` when it doesn't; otherwise the
    /// value, or `None` inside when the field is left to what it holds
    /// when absent or a problem was found. A field none of whose parts is
    /// sent is `none`, or missing; one with only some, each missing part a
    /// problem at its member.
    fn parts(
        &self,
        id: CodecId,
        entries: &serde_json::Map<String, serde_json::Value>,
        path: &Path,
        absent: bool,
        problems: &mut Vec<Problem>,
    ) -> Option<Option<Value>> {
        let (contract, standard, parts) = self.parts_of(id)?;
        let sent: Vec<(&str, &str, Option<&serde_json::Value>)> = parts
            .iter()
            .map(|(part, at)| {
                (
                    part.as_str(),
                    at.as_str(),
                    entries.get(at).filter(|json| !json.is_null()),
                )
            })
            .collect();
        if sent.iter().all(|(_, _, json)| json.is_none()) {
            if absent {
                return Some(None);
            }
            if matches!(self.codec(contract), Some(Codec::Nullable(_))) {
                return Some(Some(Value::None));
            }
        }
        let mut found = Vec::new();
        for (part, at, json) in &sent {
            match json {
                Some(json) => found.push((part.to_string(), (*json).clone())),
                None => Self::problem(problems, &path.with(*at), "required"),
            }
        }
        if found.len() < sent.len() {
            return Some(None);
        }
        let json = wire::parts_read(standard, &found);
        // Each problem at the member its part was read from.
        let before = problems.len();
        let value = self.read(contract, &json, path, problems);
        for problem in problems.iter_mut().skip(before) {
            let member = problem.field.rsplit('.').next().unwrap_or("");
            if let Some((_, at)) = parts.iter().find(|(part, _)| part == member) {
                let shown = path.with(at.as_str()).shown();
                problem.field = shown;
            }
        }
        Some(value)
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
                match self.text_scalar(*builtin, *validator, text) {
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
        match self.check(validator, value) {
            Ok(value) => Some(value),
            Err(broken) => {
                let at = match broken.item {
                    Some(item) => path.with(item.to_string()),
                    None => path.clone(),
                };
                Self::problem(problems, &at, &broken.rule);
                None
            }
        }
    }

    /// `value` stored at its type's scale (`max` when it can't be) and
    /// checked against validator `validator`'s rules: a rule across an
    /// object's fields by its name (area 2, "Rules across fields"), and a
    /// vector's item past what its half-precision index holds at its
    /// place, as one past a decimal's digits is (chunk 5.8c).
    pub fn check(
        &self,
        validator: jadpo_std::types::ValidatorId,
        value: Value,
    ) -> Result<Value, Broken> {
        let Some(validator) = self.validators.get(validator.0 as usize) else {
            return Ok(value);
        };
        let Some(value) = validator.store(value) else {
            return Err(Broken {
                rule: "max".to_string(),
                item: None,
            });
        };
        match validator.check(&value) {
            Ok(()) => Ok(value),
            Err(invalid) => {
                let item = match validator.vector_holding(&value) {
                    Some((key, Some(item))) if key == invalid.key() => Some(item),
                    _ => None,
                };
                Err(Broken {
                    rule: invalid.name().unwrap_or(invalid.key().word()).to_string(),
                    item,
                })
            }
        }
    }

    /// A scalar of `builtin` from text (a placeholder, a query string's
    /// value, a header, a map's key): a whole number with a text form of
    /// its own (an HTTP status, basis points) as `text.parse` reads it,
    /// anything else by its representation's text. Not yet checked
    /// against validator `validator`'s rules.
    pub fn text_scalar(
        &self,
        builtin: Builtin,
        validator: jadpo_std::types::ValidatorId,
        text: &str,
    ) -> Result<Value, &'static str> {
        let whole = self
            .validators
            .get(validator.0 as usize)
            .and_then(Validator::standard)
            .and_then(jadpo_std::standard_type)
            .filter(|entry| matches!(entry.holds, jadpo_std::standard::Holds::Whole { .. }));
        match whole {
            Some(entry) => crate::standard::read(entry, text).map_err(|_| "type"),
            None => crate::standard::scalar_text(builtin, text),
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
        // A member named twice makes a document unreadable (chunk 5.8c).
        let (json, numbers) = parse(text.as_bytes()).ok_or(Unread::Malformed)?;
        let mut problems = Vec::new();
        let value = self.read(
            id,
            &json,
            &Path::default().numbered(&numbers),
            &mut problems,
        );
        match (value, problems.into_iter().next()) {
            (Some(value), None) => Ok(value),
            (_, Some(problem)) => Err(Unread::Mismatch(problem)),
            (None, None) => Err(Unread::Mismatch(Problem {
                field: String::new(),
                rule: "type".to_string(),
            })),
        }
    }

    /// [`Codecs::read_value`] of the member `member` of the document
    /// `text`: what an `ai.generate` answer wrapped in an object for a
    /// structured mode holds (area 21; chunk 5.5v3). A problem's path is
    /// the member's own, without `member`; a document that isn't an object
    /// with `member` is `mismatch` at its root.
    pub fn read_member(&self, id: CodecId, text: &str, member: &str) -> Result<Value, Unread> {
        let (json, numbers) = parse(text.as_bytes()).ok_or(Unread::Malformed)?;
        let Some(inner) = json.get(member) else {
            return Err(Unread::Mismatch(Problem {
                field: String::new(),
                rule: "type".to_string(),
            }));
        };
        let path = Path {
            shown: Vec::new(),
            at: vec![member.to_string()],
            numbers: Some(Rc::new(numbers)),
        };
        let mut problems = Vec::new();
        let value = self.read(id, inner, &path, &mut problems);
        match (value, problems.into_iter().next()) {
            (Some(value), None) => Ok(value),
            (_, Some(problem)) => Err(Unread::Mismatch(problem)),
            (None, None) => Err(Unread::Mismatch(Problem {
                field: String::new(),
                rule: "type".to_string(),
            })),
        }
    }

    /// The fields of the object codec `id` is, an `Input`'s, in order,
    /// each by its name with whether a value may leave it out: what a file
    /// row's columns are held to (area 19, "Data formats"; chunk 6.4b).
    /// With the type's name; `None` for a codec that isn't an object's.
    pub fn input_fields(&self, id: CodecId) -> Option<(String, Vec<(String, bool)>)> {
        let Some(Codec::Object { shape, .. }) = self.codec(id) else {
            return None;
        };
        let shape = self.shapes.get(shape.0 as usize)?;
        let fields = (0..shape.len())
            .filter_map(|slot| shape.field(slot))
            .map(|(name, absent)| (name.to_string(), absent))
            .collect();
        Some((shape.ty().to_string(), fields))
    }

    /// A file row's cells read as the `Input` codec `id` is (area 19, "Data
    /// formats"; chunk 6.4b): each field's cell, in its fields' order, read
    /// as a query string's value is ([`Codecs::read_text`]), a standard
    /// type in its canonical text; `None` for a cell left empty or a column
    /// the file hasn't, which a field that may be left out takes as a
    /// request leaving it out does. The first problem found, by its field's
    /// name and the rule, never the value.
    pub fn read_cells(&self, id: CodecId, cells: &[Option<&str>]) -> Result<Value, Problem> {
        let unread = |field: &str, rule: &str| Problem {
            field: field.to_string(),
            rule: rule.to_string(),
        };
        let Some(Codec::Object {
            shape,
            fields,
            validator,
        }) = self.codec(id)
        else {
            return Err(unread("", "type"));
        };
        let Some(shape) = self.shapes.get(shape.0 as usize) else {
            return Err(unread("", "type"));
        };
        let mut problems = Vec::new();
        let mut given = Vec::new();
        for (slot, codec) in fields.iter().enumerate() {
            let Some((name, absent)) = shape.field(slot) else {
                continue;
            };
            let at = Path::field(name);
            match cells.get(slot).copied().flatten() {
                None if absent => {}
                None => Self::problem(&mut problems, &at, "required"),
                Some(text) => {
                    if let Some(value) = self.read_text(*codec, text, &at, &mut problems) {
                        given.push((Some(slot as u32), value));
                    }
                }
            }
            if let Some(problem) = problems.first() {
                return Err(problem.clone());
            }
        }
        let Some(value) = shape.build(given) else {
            return Err(unread("", "type"));
        };
        let value = match validator {
            Some(validator) => self.checked(*validator, value, &Path::default(), &mut problems),
            None => Some(value),
        };
        match (value, problems.into_iter().next()) {
            (Some(value), None) => Ok(value),
            (_, Some(problem)) => Err(problem),
            (None, None) => Err(unread("", "type")),
        }
    }

    /// A file row's cells written from `value`, of the `Output` codec
    /// `id` is (area 19, "Data formats"; chunk 6.4b2), in its fields'
    /// order: each field's text as a query string's value holds it
    /// ([`Codecs::read_text`] reads it back), a standard type in its
    /// canonical text, `None` for a field that is `none`; with whether the
    /// field is built on `Text`, which a spreadsheet's escaping is for.
    /// `None` for a value that isn't an object of its codec.
    pub fn write_cells(&self, id: CodecId, value: &Value) -> Option<Vec<(Option<String>, bool)>> {
        let Some(Codec::Object { fields, .. }) = self.codec(id) else {
            return None;
        };
        let Value::Record(record) = value else {
            return None;
        };
        Some(
            record
                .fields
                .iter()
                .zip(fields)
                .map(|((_, field), codec)| self.cell(*codec, field))
                .collect(),
        )
    }

    /// One cell: the text of `value`, of codec `id`'s type, and whether
    /// the type is built on `Text`.
    fn cell(&self, id: CodecId, value: &Value) -> (Option<String>, bool) {
        let text_of = |value: &Value| match crate::standard::from(value) {
            Ok(Value::Text(text)) => Some(text.to_string()),
            _ => None,
        };
        match (self.codec(id), value) {
            (Some(Codec::Nullable(inner)), value) => self.cell(*inner, value),
            (Some(Codec::Scalar { builtin, .. }), value) => {
                let text = builtin.representation() == jadpo_std::Builtin::Text;
                match value {
                    Value::None | Value::Unit => (None, text),
                    Value::Text(held) => (Some(held.to_string()), text),
                    other => (text_of(other), text),
                }
            }
            (_, Value::None | Value::Unit) => (None, false),
            (Some(Codec::Enum { variants, .. }), Value::Variant(variant)) => {
                let found = variants.iter().find(|v| v.name == *variant.name);
                (
                    Some(found.map_or(variant.name.to_string(), |v| v.wire.clone())),
                    false,
                )
            }
            (_, other) => (text_of(other), false),
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
            // A provider package's field, as the provider takes it (chunk
            // 7.2c2), and as the contract writes it anywhere else.
            (Some(Codec::Wired { contract, coding }), value) => match (self.coder, coding) {
                (None, _) => self.write(*contract, value, out),
                (Some(coder), WireCoding::Encoding { encoding, .. }) => {
                    let mut written = String::new();
                    self.write(*contract, value, &mut written);
                    match wire::write(*encoding, &written) {
                        Ok(sent) => out.push_str(&sent),
                        Err(rule) => {
                            coder.unwritten(rule);
                            out.push_str("null");
                        }
                    }
                }
                (Some(_), WireCoding::Parts { .. }) => self.write(*contract, value, out),
                (Some(coder), WireCoding::Functions { encode, wire, .. }) => {
                    match coder.encode(encode, value.clone()) {
                        Some(sent) => self.write(*wire, &sent, out),
                        None => out.push_str("null"),
                    }
                }
            },
            (Some(Codec::Scalar { int_as_number, .. }), value) => {
                write_scalar(value, *int_as_number, out)
            }
            (Some(Codec::Object { fields, shape, .. }), Value::Record(record)) => {
                let shape = self.shapes.get(shape.0 as usize);
                out.push('{');
                let mut first = true;
                let omitted = record.omitted.as_deref().unwrap_or(&[]);
                // A problem's members that are `none` are left out, as RFC
                // 9457 sends them (chunk 5.5n).
                let members = jadpo_std::standard_type(&record.ty).is_some_and(|entry| {
                    matches!(entry.json, jadpo_std::standard::Json::Members(_))
                });
                // Each part's member written so far, for fields that share
                // one (chunk 7.2e).
                let mut parted: BTreeMap<String, String> = BTreeMap::new();
                for (at, ((name, field), codec)) in record.fields.iter().zip(fields).enumerate() {
                    // An optional `Input` field left out is left out on the
                    // wire too, so it never reads back as one sent as
                    // `null` (area 6, "patch").
                    if omitted.iter().any(|place| *place as usize == at)
                        || (members && *field == Value::None)
                    {
                        continue;
                    }
                    // A field sent as its standard type's parts, each its
                    // own member of this object (chunk 7.2e).
                    if let Some((contract, standard, parts)) = self.parts_of(*codec) {
                        let mut written = String::new();
                        self.write(contract, field, &mut written);
                        let json: serde_json::Value =
                            serde_json::from_str(&written).unwrap_or(serde_json::Value::Null);
                        for (member, part) in wire::parts_write(standard, &json, parts) {
                            let part = part.to_string();
                            match parted.get(&member) {
                                Some(sent) if *sent == part => continue,
                                // Two fields that share a member, as a
                                // charge's amounts share its currency,
                                // can't send it two ways: the member is
                                // named, never a value.
                                Some(_) => {
                                    if let Some(coder) = self.coder {
                                        coder.unwritten(&member);
                                    }
                                    continue;
                                }
                                None => {}
                            }
                            if !first {
                                out.push(',');
                            }
                            first = false;
                            write_string(&member, out);
                            out.push(':');
                            out.push_str(&part);
                            parted.insert(member, part);
                        }
                        continue;
                    }
                    if !first {
                        out.push(',');
                    }
                    first = false;
                    let name = shape
                        .and_then(|shape| shape.json_name(at))
                        .unwrap_or_else(|| jadpo_std::standard::wire_name(&record.ty, name));
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
                    Some((shape, codecs)) => {
                        let shape = self.shapes.get(shape.0 as usize);
                        out.push_str("{\"tag\":");
                        write_string(found.map_or(&*variant.name, |v| &v.wire), out);
                        for (at, ((name, field), codec)) in
                            variant.fields.iter().zip(codecs).enumerate()
                        {
                            out.push(',');
                            write_string(
                                shape.and_then(|shape| shape.json_name(at)).unwrap_or(name),
                                out,
                            );
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
pub fn write_scalar(value: &Value, int_as_number: bool, out: &mut String) {
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
    // Reserve the ordinary UTF-8 length; escaping may still need more space.
    out.reserve(text.len() + 2);
    out.push('"');
    if let Some(index) = text
        .bytes()
        .position(|byte| byte < 0x20 || byte == b'"' || byte == b'\\')
    {
        // An ASCII escape byte is always a UTF-8 boundary.
        out.push_str(&text[..index]);
        // Repeated spans regressed escape-dense strings; keep the scalar suffix.
        // See docs/compiler-runtime.md §2.11 for the measured tradeoffs.
        for c in text[index..].chars() {
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
    } else {
        out.push_str(text);
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
            coder: None,
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
            wire: None,
        };
        let shape = Shape::new(&ShapePlan {
            ty: "Line".to_string(),
            open: false,
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
            coder: None,
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
            coder: None,
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
            coder: None,
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
            coder: None,
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
    #[test]
    fn string_bytes_preserve_the_canonical_escapes_and_unicode() {
        // Fixed bytes are independent of both this writer and serde's choice
        // of escape spelling (serde uses \b and \f, unlike Jadpo).
        let controls: String = (0u8..32).map(char::from).collect();
        let expected_controls = concat!(
            "\"\\u0000\\u0001\\u0002\\u0003\\u0004\\u0005\\u0006\\u0007",
            "\\u0008\\t\\n\\u000b\\u000c\\r\\u000e\\u000f",
            "\\u0010\\u0011\\u0012\\u0013\\u0014\\u0015\\u0016\\u0017",
            "\\u0018\\u0019\\u001a\\u001b\\u001c\\u001d\\u001e\\u001f\""
        );
        for (text, expected) in [
            ("", "\"\""),
            (controls.as_str(), expected_controls),
            ("\u{7f}", "\"\u{7f}\""),
            ("say \"hi\"\\", "\"say \\\"hi\\\"\\\\\""),
            (
                "e\u{301}é🙂\u{2028}\u{2029}",
                "\"e\u{301}é🙂\u{2028}\u{2029}\"",
            ),
        ] {
            for prefix in ["", "already written: é "] {
                let mut out = prefix.to_owned();
                write_string(text, &mut out);
                assert_eq!(&out[..prefix.len()], prefix);
                assert_eq!(&out[prefix.len()..], expected);
                assert_eq!(
                    serde_json::from_str::<String>(&out[prefix.len()..]).unwrap(),
                    text
                );
            }
        }
    }

    #[test]
    fn string_bytes_cover_dense_alternating_and_late_escapes() {
        for (text, expected) in [
            (
                "\"\\\n\u{8}".repeat(1024),
                format!("\"{}\"", "\\\"\\\\\\n\\u0008".repeat(1024)),
            ),
            (
                "é\"🙂\\e\u{301}\t".repeat(1024),
                format!("\"{}\"", "é\\\"🙂\\\\e\u{301}\\t".repeat(1024)),
            ),
            (
                format!("{}\n", "x".repeat(65_535)),
                format!("\"{}\\n\"", "x".repeat(65_535)),
            ),
        ] {
            let mut out = "prefix:".to_owned();
            write_string(&text, &mut out);
            assert_eq!(&out[..7], "prefix:");
            assert_eq!(&out[7..], expected);
            assert_eq!(serde_json::from_str::<String>(&out[7..]).unwrap(), text);
        }
    }

    #[test]
    fn seeded_unicode_scalar_strings_round_trip_without_rewriting_text() {
        let mut state = 0x5a17_9d3bu32;
        for _ in 0..128 {
            let mut text = String::from("\0\"\\\n\r\té🙂e\u{301}");
            for _ in 0..256 {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                if let Some(c) = char::from_u32(state % 0x11_0000) {
                    text.push(c);
                }
            }
            let mut out = String::from("prefix:");
            write_string(&text, &mut out);
            assert_eq!(&out[..7], "prefix:");
            assert_eq!(serde_json::from_str::<String>(&out[7..]).unwrap(), text);
            assert!(!out[8..out.len() - 1].chars().any(|c| c < '\u{20}'));
        }
    }
    /// Codecs for vectors: 0 an item, 1 a 2-item vector compared by
    /// cosine, 2 a 2,001-item one indexed at half precision, by euclidean
    /// distance, 3 a map's key, 4 a map of 2-item vectors.
    fn vector_codecs() -> (Vec<Codec>, Vec<Validator>) {
        use jadpo_std::types::ValidatorId;
        use jadpo_std::{Rules, VectorDistance};
        let vector = |dimensions: u64, distance: VectorDistance| {
            Validator::new(
                None,
                vec![
                    Rules {
                        dimensions: Some(dimensions),
                        distance: Some(distance),
                        ..Rules::default()
                    },
                    jadpo_std::standard::ai::vector_rules(),
                ],
            )
        };
        let codecs = vec![
            Codec::Float,
            Codec::List {
                item: CodecId(0),
                validator: ValidatorId(0),
            },
            Codec::List {
                item: CodecId(0),
                validator: ValidatorId(1),
            },
            Codec::Scalar {
                builtin: Builtin::Text,
                validator: ValidatorId(2),
                int_as_number: false,
            },
            Codec::Map {
                key: CodecId(3),
                value: CodecId(1),
                validator: ValidatorId(2),
            },
        ];
        let validators = vec![
            vector(2, VectorDistance::Cosine),
            vector(2001, VectorDistance::Euclidean),
            Validator::new(Some(Builtin::Text), Vec::new()),
        ];
        (codecs, validators)
    }

    #[test]
    fn a_vector_s_item_is_the_float_nearest_its_own_text() {
        let (codecs, validators) = vector_codecs();
        let codecs = Codecs {
            codecs: &codecs,
            validators: &validators,
            shapes: &[],
            coder: None,
        };
        // Just above the midpoint of 1 and the float after it, but nearer
        // the midpoint than any other double: read as a double first, the
        // tie goes to 1; its own text is nearer the float after.
        let read = codecs.read_value(
            CodecId(1),
            &Value::text("[1.00000005960464477539062501, 0.5]"),
        );
        assert_eq!(
            read.map(|vector| vector.to_string()),
            Ok(Value::list(vec![
                Value::decimal("1.0000001").unwrap(),
                Value::decimal("0.5").unwrap()
            ])
            .to_string())
        );
        // A request's body is read the same way.
        let (json, numbers) =
            super::parse(br#"{"m": [1.00000005960464477539062501, 0.5]}"#).expect("a document");
        let mut problems = Vec::new();
        let read = codecs.read(
            CodecId(1),
            &json["m"],
            &Path::field("m").numbered(&numbers),
            &mut problems,
        );
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(
            read.map(|vector| vector.to_string()),
            Some(
                Value::list(vec![
                    Value::decimal("1.0000001").unwrap(),
                    Value::decimal("0.5").unwrap()
                ])
                .to_string()
            )
        );
    }

    #[test]
    fn a_vector_holds_what_its_index_and_its_metric_need() {
        let (codecs, validators) = vector_codecs();
        let codecs = Codecs {
            codecs: &codecs,
            validators: &validators,
            shapes: &[],
            coder: None,
        };
        let problem = |id: u32, text: &str| match codecs.read_value(CodecId(id), &Value::text(text))
        {
            Err(Unread::Mismatch(problem)) => (problem.field, problem.rule),
            other => panic!("{text}: {other:?}"),
        };
        // A vector of zeros has no direction to compare by cosine.
        assert_eq!(
            problem(1, "[0, 0.0]"),
            (String::new(), "distance".to_string())
        );
        // An item past a half-precision float, at its place.
        let mut items = vec!["0.5"; 2001];
        items[7] = "70000";
        assert_eq!(
            problem(2, &format!("[{}]", items.join(","))),
            ("7".to_string(), "max".to_string())
        );
        items[7] = "-65505";
        assert_eq!(
            problem(2, &format!("[{}]", items.join(","))),
            ("7".to_string(), "min".to_string())
        );
        items[7] = "65504";
        assert!(
            codecs
                .read_value(CodecId(2), &Value::text(&format!("[{}]", items.join(","))))
                .is_ok()
        );
        // Zeros by straight-line distance are a vector like any other.
        items = vec!["0"; 2001];
        assert!(
            codecs
                .read_value(CodecId(2), &Value::text(&format!("[{}]", items.join(","))))
                .is_ok()
        );
    }

    #[test]
    fn a_member_named_twice_makes_a_document_unreadable() {
        let (codecs, validators) = vector_codecs();
        let codecs = Codecs {
            codecs: &codecs,
            validators: &validators,
            shapes: &[],
            coder: None,
        };
        assert!(
            codecs
                .read_value(CodecId(4), &Value::text(r#"{"a": [1, 0], "b": [0, 1]}"#))
                .is_ok()
        );
        // Last one winning would read a value its sender may not have
        // meant; a JWT's claims refuse it too.
        assert_eq!(
            codecs.read_value(CodecId(4), &Value::text(r#"{"a": [1, 0], "a": [0, 1]}"#)),
            Err(Unread::Malformed)
        );
        assert_eq!(super::parse(br#"{"x": {"y": 1, "y": 2}}"#), None);
        assert!(super::parse(br#"[{"y": 1}, {"y": 2}]"#).is_some());
        // What serde_json reads and refuses, it reads and refuses.
        for text in [
            "",
            " ",
            "[1,]",
            "{\"a\" 1}",
            "01",
            "1.",
            ".5",
            "-",
            "1e",
            "+1",
            "NaN",
            "\"\\x\"",
            "\"\u{1}\"",
            "[1] 2",
            "1e400",
            "\"\\ud800\"",
        ] {
            assert_eq!(super::parse(text.as_bytes()), None, "{text:?}");
        }
        for text in [
            "0",
            "-0",
            "1.5e-3",
            "18446744073709551615",
            "-9223372036854775808",
            "123456789012345678901234567890",
            "\"\\u00e9\\ud83d\\ude00\\n\"",
            "true",
            "null",
            " {\"a\": [false, null, \"\"]} ",
        ] {
            let ours = super::parse(text.as_bytes()).map(|(json, _)| json);
            let theirs: Option<serde_json::Value> = serde_json::from_str(text).ok();
            assert_eq!(ours, theirs, "{text:?}");
        }
        // Too deep, as serde_json's limit.
        let deep = format!("{}{}", "[".repeat(200), "]".repeat(200));
        assert_eq!(super::parse(deep.as_bytes()), None);
    }
}
