//! What a type is on the wire (area 2's JSON forms), read from the graph:
//! the API record writes it as text and OpenAPI as a JSON Schema, so both
//! are views of the same facts (area 18: every report is a view of the
//! manifest).

use std::collections::BTreeSet;

use jadpo_graph::json::Value;
use jadpo_graph::{Base, Detail, Graph, Key, Kind, Node, TypeRef, rule_pairs};
use jadpo_std::http::SAFE_INTEGER;
use jadpo_std::{Builtin, Num, Rules};

/// A type followed through its refinements and field types to what it is
/// made of.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Shape {
    /// A built-in type with every rule along the way, the most specific
    /// first where two say the same thing.
    Scalar(Builtin, Rules),
    /// An object type, by its key.
    Object(Key),
    /// An enum, by its key, or a standard-library enum by name.
    Enum(Key),
    StdEnum(String),
    /// A standard object type (area 19): sent as its data entry's JSON
    /// says, an object of its fields (`InstantRange`'s `start` and `end`).
    /// A standard object, with the rules a type built on it adds: a rate
    /// type's currencies (chunk 5.5h4).
    StdObject(&'static jadpo_std::StandardType, Rules),
    /// `IssuedToken` or `IssuedKey` (area 8), or `DecodeProblem` (area
    /// 19): built in, with the fields `jadpo_std::http::builtin_object_fields`
    /// lists.
    Issued(&'static str),
    List(TypeRef, Rules),
    Set(TypeRef, Rules),
    Map(TypeRef, TypeRef, Rules),
    Page(TypeRef),
    /// Not a type the boundary carries.
    Other(String),
}

/// Every rule along a chain, the first (most specific) of each kept.
pub fn merge<'r>(chain: impl IntoIterator<Item = &'r Rules>) -> Rules {
    let mut out = Rules::default();
    for rules in chain {
        out.min = out.min.or(rules.min);
        out.max = out.max.or(rules.max);
        out.min_length = out.min_length.or(rules.min_length);
        out.max_length = out.max_length.or(rules.max_length);
        out.pattern = out.pattern.clone().or_else(|| rules.pattern.clone());
        out.format = out.format.or(rules.format);
        out.scale = out.scale.or(rules.scale);
        out.version = out.version.or(rules.version);
        out.min_duration = out.min_duration.or(rules.min_duration);
        out.max_duration = out.max_duration.or(rules.max_duration);
        out.currency = out.currency.or(rules.currency);
        out.rounding = out.rounding.or(rules.rounding);
        out.rate_from = out.rate_from.or(rules.rate_from);
        out.rate_to = out.rate_to.or(rules.rate_to);
        out.dimensions = out.dimensions.or(rules.dimensions);
        out.standard = out.standard.or(rules.standard);
    }
    out
}

/// Whether an `Int` with these rules is a JSON number: its declared range
/// fits JavaScript's safe integers (area 2, question 8).
pub fn int_as_number(rules: &Rules) -> bool {
    let fits = |n: Option<Num>| n.is_some_and(|n| n.scale == 0 && n.mantissa.abs() <= SAFE_INTEGER);
    fits(rules.min) && fits(rules.max)
}

/// Reads types from the graph.
#[derive(Clone, Copy, Debug)]
pub struct Wire<'g> {
    pub graph: &'g Graph,
    /// The locales the application supports, when it declares them: a
    /// `Locale` is sent and read as one of these (area 10, "Locales").
    pub locales: Option<&'g jadpo_std::LocalePlan>,
}

/// How far a chain is followed; only a defect gets this far.
const DEPTH: usize = 64;

impl<'g> Wire<'g> {
    pub fn node(&self, key: &Key) -> Option<&'g Node> {
        self.graph.get(key)
    }

    /// A key's name now.
    pub fn name(&self, key: &Key) -> String {
        self.node(key)
            .map_or_else(|| key.name().to_string(), |node| node.name.clone())
    }

    /// Whether a value of `ty` may be `null`.
    pub fn nullable(&self, ty: &TypeRef) -> bool {
        let mut current = ty;
        for _ in 0..DEPTH {
            match current {
                TypeRef::Nullable(_) => return true,
                TypeRef::Field(key) => match self.node(key).map(|node| &node.detail) {
                    Some(Detail::Field { ty, .. }) => current = ty,
                    _ => return false,
                },
                _ => return false,
            }
        }
        false
    }

    /// What `ty` is made of.
    pub fn shape(&self, ty: &TypeRef) -> Shape {
        let mut rules: Vec<Rules> = Vec::new();
        let mut current = ty.clone();
        for _ in 0..DEPTH {
            match current {
                TypeRef::Builtin(builtin) => {
                    rules.extend(builtin.rules().cloned());
                    return Shape::Scalar(builtin, merge(&rules));
                }
                TypeRef::Nullable(inner) => current = *inner,
                TypeRef::Field(key) => match self.node(&key).map(|node| &node.detail) {
                    Some(Detail::Field { ty, rules: own, .. }) => {
                        rules.push(own.clone());
                        current = ty.clone();
                    }
                    _ => return Shape::Other(key.name().to_string()),
                },
                TypeRef::Node(key) => {
                    let Some(node) = self.node(&key) else {
                        return Shape::Other(key.name().to_string());
                    };
                    match &node.detail {
                        Detail::Type {
                            base: Base::Refines(parent),
                            rules: own,
                            ..
                        } => {
                            rules.push(own.clone());
                            current = parent.clone();
                        }
                        Detail::Type {
                            base: Base::Enum, ..
                        } => return Shape::Enum(key),
                        _ if node.kind == Kind::Type || node.kind == Kind::Entity => {
                            return Shape::Object(key);
                        }
                        _ => return Shape::Other(node.name.clone()),
                    }
                }
                TypeRef::List(inner) => return Shape::List(*inner, merge(&rules)),
                TypeRef::Set(inner) => return Shape::Set(*inner, merge(&rules)),
                TypeRef::Map(key, value) => return Shape::Map(*key, *value, merge(&rules)),
                // A standard type held as text (area 19, chunk 5.5i): text
                // with its own check, whose pattern OpenAPI shows; or held
                // as a whole number, with its range (chunk 5.5n); or on a
                // built-in type with bounds of its own (chunk 5.5i3).
                TypeRef::Std(name, args)
                    if args.is_empty() && jadpo_std::standard::declared(&name).is_some() =>
                {
                    let Some((builtin, own)) = jadpo_std::standard::declared(&name) else {
                        unreachable!("declared just above");
                    };
                    rules.push(own);
                    current = TypeRef::Builtin(builtin);
                }
                TypeRef::Std(name, args) if name == "Page" && args.len() == 1 => {
                    return Shape::Page(args.into_iter().next().unwrap_or(TypeRef::None));
                }
                TypeRef::Std(name, args) if args.is_empty() => {
                    // A standard object, a choice, sent tagged (chunk
                    // 5.5i3), or a named collection (chunk 5.5o2), each
                    // described from its entry.
                    if let Some(entry) = jadpo_std::standard_type(&name)
                        && matches!(
                            entry.holds,
                            jadpo_std::standard::Holds::Object(_)
                                | jadpo_std::standard::Holds::Choice(_)
                                | jadpo_std::standard::Holds::List(_)
                                | jadpo_std::standard::Holds::Map(..)
                        )
                    {
                        return Shape::StdObject(entry, merge(&rules));
                    }
                    return match issued_name(&name) {
                        Some(issued) => Shape::Issued(issued),
                        None => Shape::StdEnum(name),
                    };
                }
                other => return Shape::Other(other.to_string()),
            }
        }
        Shape::Other(ty.to_string())
    }

    /// An enum's variants in declared order, each with its payload fields.
    pub fn variants(&self, key: &Key) -> Vec<(String, Vec<&'g Node>)> {
        let Some(id) = self.graph.find(key) else {
            return Vec::new();
        };
        let mut variants: Vec<(u32, String, Vec<&'g Node>)> = self
            .graph
            .children(id)
            .map(|child| self.graph.node(child))
            .filter_map(|node| match &node.detail {
                Detail::Variant { order, .. } => {
                    let fields = self
                        .graph
                        .find(&node.key)
                        .map(|vid| {
                            self.graph
                                .fields(vid)
                                .into_iter()
                                .map(|f| self.graph.node(f))
                                .collect()
                        })
                        .unwrap_or_default();
                    let name = node.name.rsplit('.').next().unwrap_or(&node.name);
                    Some((*order, name.to_string(), fields))
                }
                _ => None,
            })
            .collect();
        variants.sort_by_key(|(order, ..)| *order);
        variants
            .into_iter()
            .map(|(_, name, fields)| (name, fields))
            .collect()
    }

    /// A standard-library enum's variants.
    pub fn std_variants(&self, name: &str) -> Vec<&'static str> {
        let variants = jadpo_std::enums()
            .into_iter()
            .find(|(written, _)| *written == name)
            .map(|(_, variants)| variants)
            .unwrap_or_default();
        match (name, self.locales) {
            (jadpo_std::LOCALE, Some(plan)) => variants
                .into_iter()
                .enumerate()
                .filter(|(at, _)| u32::try_from(*at).is_ok_and(|at| plan.supports(at)))
                .map(|(_, variant)| variant)
                .collect(),
            _ => variants,
        }
    }

    /// The fields of an object type or entity, in declared order.
    pub fn fields(&self, key: &Key) -> Vec<&'g Node> {
        self.graph
            .find(key)
            .map(|id| {
                self.graph
                    .fields(id)
                    .into_iter()
                    .map(|f| self.graph.node(f))
                    .collect()
            })
            .unwrap_or_default()
    }

    // --- the record's text --------------------------------------------------

    /// `ty` as the API record writes it: the built-in type with its rules
    /// for a scalar, an object's or enum's name, which `refs` collects for
    /// a section of its own, collections around those, and `?`.
    pub fn render(&self, ty: &TypeRef, refs: &mut BTreeSet<Key>) -> String {
        let shown = match self.shape(ty) {
            Shape::Scalar(builtin, rules) => format!("{}{}", builtin.name(), rules_text(&rules)),
            Shape::Object(key) | Shape::Enum(key) => {
                refs.insert(key.clone());
                self.name(&key)
            }
            Shape::StdEnum(name) => name,
            Shape::StdObject(entry, rules) => format!("{}{}", entry.name, rules_text(&rules)),
            Shape::Issued(name) => {
                refs.insert(issued_key(name));
                name.to_string()
            }
            Shape::List(inner, rules) => {
                format!("List<{}>{}", self.render(&inner, refs), rules_text(&rules))
            }
            Shape::Set(inner, rules) => {
                format!("Set<{}>{}", self.render(&inner, refs), rules_text(&rules))
            }
            Shape::Map(key, value, rules) => format!(
                "Map<{}, {}>{}",
                self.render(&key, refs),
                self.render(&value, refs),
                rules_text(&rules)
            ),
            Shape::Page(inner) => format!("Page<{}>", self.render(&inner, refs)),
            Shape::Other(written) => written,
        };
        if self.nullable(ty) {
            format!("{shown}?")
        } else {
            shown
        }
    }

    // --- JSON Schema ----------------------------------------------------------

    /// `ty` as a JSON Schema (OpenAPI 3.1), objects and enums by reference
    /// to `#/components/schemas/<name>`, which `refs` collects.
    pub fn schema(&self, ty: &TypeRef, refs: &mut BTreeSet<Key>) -> Value {
        let schema = match self.shape(ty) {
            Shape::Scalar(builtin, rules) => scalar_schema(builtin, &rules),
            Shape::Object(key) | Shape::Enum(key) => {
                refs.insert(key.clone());
                reference(&self.name(&key))
            }
            Shape::Issued(name) => {
                refs.insert(issued_key(name));
                reference(name)
            }
            // A map keyed by a standard enum lists the keys the wire takes:
            // a `LocalisedText`'s, the supported locales' tags (chunk
            // 5.5o2).
            Shape::StdObject(entry, rules) => match entry.holds {
                jadpo_std::standard::Holds::Map(jadpo_std::Part::Standard(key), _) => {
                    let mut out = standard_object_schema(entry);
                    out.set(
                        "propertyNames",
                        self.schema(&TypeRef::Std(key.to_string(), Vec::new()), refs),
                    );
                    out
                }
                // A vector type's lengths (chunk 5.5s2).
                jadpo_std::standard::Holds::List(_) => {
                    let mut out = standard_object_schema(entry);
                    item_counts(&mut out, &rules);
                    out
                }
                _ => rate_currencies(standard_object_schema(entry), &rules),
            },
            Shape::StdEnum(name) => {
                let mut out = Value::object();
                out.set("type", "string");
                // As sent: a `Zone` by its IANA name, a `Locale` by its
                // BCP 47 tag (area 10).
                let spelling = jadpo_std::config::Spelling::of(&name);
                out.set(
                    "enum",
                    self.std_variants(&name)
                        .into_iter()
                        .map(|variant| Value::from(spelling.spelling(variant)))
                        .collect::<Vec<_>>(),
                );
                out
            }
            Shape::List(inner, rules) | Shape::Set(inner, rules) => {
                let set = matches!(self.shape(ty), Shape::Set(..));
                let mut out = Value::object();
                out.set("type", "array");
                out.set("items", self.schema(&inner, refs));
                item_counts(&mut out, &rules);
                if set {
                    out.set("uniqueItems", true);
                }
                out
            }
            Shape::Map(_, value, rules) => {
                let mut out = Value::object();
                out.set("type", "object");
                out.set("additionalProperties", self.schema(&value, refs));
                lengths(&mut out, &rules, "minProperties", "maxProperties");
                out
            }
            Shape::Page(inner) => {
                let mut items = Value::object();
                items.set("type", "array");
                items.set("items", self.schema(&inner, refs));
                let mut next = Value::object();
                next.set("type", vec![Value::from("string"), Value::from("null")]);
                next.set(
                    "description",
                    "The cursor for the next page, sent back as the query's `after`; `null` on the last page.",
                );
                let mut properties = Value::object();
                properties.set("items", items);
                properties.set("next", next);
                let mut out = Value::object();
                out.set("type", "object");
                out.set("properties", properties);
                out.set("required", vec![Value::from("items"), Value::from("next")]);
                out.set("additionalProperties", false);
                out
            }
            Shape::Other(_) => Value::object(),
        };
        if self.nullable(ty) {
            let mut null = Value::object();
            null.set("type", "null");
            let mut out = Value::object();
            out.set("anyOf", vec![schema, null]);
            out
        } else {
            schema
        }
    }
}

/// `IssuedToken`, `IssuedKey` or `DecodeProblem` as the built-in object
/// type `name` names.
pub fn issued_name(name: &str) -> Option<&'static str> {
    [
        jadpo_std::http::ISSUED_TOKEN,
        jadpo_std::http::ISSUED_KEY,
        jadpo_std::http::DECODE_PROBLEM,
    ]
    .into_iter()
    .find(|issued| *issued == name)
}

/// The key a built-in `IssuedToken` or `IssuedKey` is referred to by,
/// which no node holds.
pub fn issued_key(name: &str) -> Key {
    Key::new(Kind::Type, name)
}

/// The built-in type a key refers to, when it is `IssuedToken` or
/// `IssuedKey`.
pub fn issued_of(key: &Key) -> Option<&'static str> {
    (key.kind() == Some(Kind::Type))
        .then(|| issued_name(key.name()))
        .flatten()
}

/// The record's section for a built-in `IssuedToken` or `IssuedKey`.
pub fn issued_section(name: &str) -> String {
    let mut text = format!("output {name}\n");
    for (field, builtin) in jadpo_std::http::builtin_object_fields(name).unwrap_or_default() {
        text.push_str(&format!("    {field}: {}\n", builtin.name()));
    }
    text
}

/// The JSON Schema of a built-in `IssuedToken` or `IssuedKey`.
pub fn issued_schema(name: &str) -> Value {
    let mut out = Value::object();
    out.set(
        "description",
        if name == jadpo_std::http::ISSUED_KEY {
            "A service key as it is listed: when it started and when it ends, never the key itself."
        } else if name == jadpo_std::http::DECODE_PROBLEM {
            "The first problem a JSON document had: the field's path, list positions as numbers, and the rule it broke, never the value."
        } else {
            "An API token as it is listed: when it started and when it ends, never the token itself."
        },
    );
    out.set("type", "object");
    let mut properties = Value::object();
    let mut required = Vec::new();
    for (field, builtin) in jadpo_std::http::builtin_object_fields(name).unwrap_or_default() {
        properties.set(field, scalar_schema(*builtin, &Rules::default()));
        required.push(Value::from(*field));
    }
    out.set("properties", properties);
    out.set("required", required);
    out.set("additionalProperties", false);
    out
}

/// A reference to a schema in the document's components.
pub fn reference(name: &str) -> Value {
    let mut out = Value::object();
    out.set("$ref", format!("#/components/schemas/{name}"));
    out
}

/// A list's lengths as `minItems` and `maxItems`, a vector type's
/// `dimensions:` as both (chunk 5.5s2).
fn item_counts(out: &mut Value, rules: &Rules) {
    lengths(out, rules, "minItems", "maxItems");
    if let Some(n) = rules.dimensions {
        let n = i64::try_from(n).unwrap_or(i64::MAX);
        out.set("minItems", n);
        out.set("maxItems", n);
    }
}

fn lengths(out: &mut Value, rules: &Rules, min: &str, max: &str) {
    if let Some(n) = rules.min_length {
        out.set(min, i64::try_from(n).unwrap_or(i64::MAX));
    }
    if let Some(n) = rules.max_length {
        out.set(max, i64::try_from(n).unwrap_or(i64::MAX));
    }
}

fn whole(n: Num) -> Option<i64> {
    (n.scale == 0)
        .then(|| i64::try_from(n.mantissa).ok())
        .flatten()
}

/// A standard object type's schema, from its data entry (area 19): an
/// object of its fields by their wire names, each its part's, all required,
/// a part that may be `none` also `null`, or, for one sent as RFC 9457
/// sends a problem (`Json::Members`), left out; a type sent as its
/// canonical text (`LocalTime`, chunk 5.5k) is a string.
/// A rate type's `from` and `to` (chunk 5.5h4): each the one code its
/// type fixes, as a `Money` type's currency is shown.
fn rate_currencies(mut schema: Value, rules: &Rules) -> Value {
    for (field, fixed) in [("from", rules.rate_from), ("to", rules.rate_to)] {
        let Some(code) = fixed.and_then(jadpo_std::money::code) else {
            continue;
        };
        let mut one = Value::object();
        one.set("type", "string");
        one.set("enum", vec![code]);
        if let Some(mut properties) = schema.get("properties").cloned() {
            properties.set(field, one);
            schema.set("properties", properties);
        }
    }
    schema
}

pub fn standard_object_schema(entry: &jadpo_std::StandardType) -> Value {
    let mut out = Value::object();
    // OpenAPI can't say its rules across its parts, so its description
    // lists them by name (chunk 5.5t2), as an object type's are (area 2).
    if let Some(described) = jadpo_std::standard::across::described(entry.name) {
        out.set("description", described);
    }
    if entry.json == jadpo_std::standard::Json::Text {
        out.set("type", "string");
        return out;
    }
    // A geometry (chunk 5.5s): the GeoJSON object RFC 7946 defines.
    if entry.json == jadpo_std::standard::Json::GeoJson {
        return geojson_schema(entry.name);
    }
    // A named collection (chunk 5.5o2): an array of its items, or an object
    // of its entries, each key its key part's text.
    match entry.holds {
        jadpo_std::standard::Holds::List(item) => {
            out.set("type", "array");
            out.set("items", part_schema(item));
            // A vector's own lengths (chunk 5.5s2).
            if let Some(rules) = jadpo_std::standard::collection_rules(entry.name) {
                item_counts(&mut out, &rules);
            }
            return out;
        }
        jadpo_std::standard::Holds::Map(key, value) => {
            out.set("type", "object");
            out.set("propertyNames", part_schema(key));
            out.set("additionalProperties", part_schema(value));
            return out;
        }
        _ => {}
    }
    // A choice (chunk 5.5i3): one of its variants, each an object of
    // `tag`, the variant's name, and its fields, as area 2 sends an enum's
    // variant with data.
    if let jadpo_std::standard::Holds::Choice(variants) = entry.holds {
        let one_of: Vec<Value> = variants
            .iter()
            .map(|(variant, fields)| {
                let mut tag = Value::object();
                tag.set("const", *variant);
                let mut properties = Value::object();
                properties.set("tag", tag);
                let mut required = vec!["tag"];
                for (name, part) in *fields {
                    properties.set(name, part_schema(*part));
                    required.push(name);
                }
                let mut schema = Value::object();
                schema.set("type", "object");
                schema.set("properties", properties);
                schema.set("required", required);
                schema.set("additionalProperties", false);
                schema
            })
            .collect();
        out.set("oneOf", one_of);
        return out;
    }
    out.set("type", "object");
    let mut properties = Value::object();
    let mut required = Vec::new();
    let members = matches!(entry.json, jadpo_std::standard::Json::Members(_));
    if let jadpo_std::standard::Holds::Object(fields) = entry.holds {
        for (name, part) in fields {
            let name = jadpo_std::standard::wire_name(entry.name, name);
            properties.set(name, part_schema(*part));
            if !(members && part.required().1) {
                required.push(name);
            }
        }
    }
    out.set("properties", properties);
    out.set("required", required);
    out.set("additionalProperties", false);
    out
}

/// A GeoJSON geometry's schema (RFC 7946, chunk 5.5s): its `type` and its
/// `coordinates`, each position two or three numbers, longitude first;
/// `Geometry`'s is one of the six.
fn geojson_schema(name: &str) -> Value {
    use jadpo_std::standard::geo;
    let array = |items: Value, least: u32| {
        let mut out = Value::object();
        out.set("type", "array");
        out.set("items", items);
        out.set("minItems", least);
        out
    };
    let mut number = Value::object();
    number.set("type", "number");
    let mut position = array(number, 2);
    position.set("maxItems", 3u32);
    let line = array(position.clone(), 2);
    let polygon = array(array(position.clone(), 4), 1);
    let coordinates = match name {
        geo::POINT => position,
        geo::LINE_STRING => line,
        geo::POLYGON => polygon,
        geo::MULTI_POINT => array(position, 1),
        geo::MULTI_LINE_STRING => array(line, 1),
        geo::MULTI_POLYGON => array(polygon, 1),
        _ => {
            let one_of: Vec<Value> = geo::GEOJSON_TYPES
                .iter()
                .map(|(_, ty)| geojson_schema(ty))
                .collect();
            let mut out = Value::object();
            out.set("oneOf", one_of);
            return out;
        }
    };
    let mut ty = Value::object();
    ty.set("const", name);
    let mut properties = Value::object();
    properties.set("type", ty);
    properties.set("coordinates", coordinates);
    let mut bbox = array(Value::object(), 4);
    bbox.set("maxItems", 6u32);
    properties.set("bbox", bbox);
    let mut out = Value::object();
    out.set("type", "object");
    out.set("properties", properties);
    out.set("required", vec!["type", "coordinates"]);
    out.set("additionalProperties", false);
    out
}

/// One part of a standard object (area 19): its built-in type's schema with
/// the rules the part keeps (a line's pattern), a standard enum by its
/// codes, a type held as text with its pattern, another standard object's,
/// `null` too for a part that may be `none`, or an array of the part.
fn part_schema(part: jadpo_std::Part) -> Value {
    match part {
        jadpo_std::Part::Optional(inner) => {
            let mut null = Value::object();
            null.set("type", "null");
            let mut out = Value::object();
            out.set("anyOf", vec![part_schema(*inner), null]);
            out
        }
        jadpo_std::Part::List(inner) => {
            let mut out = Value::object();
            out.set("type", "array");
            out.set("items", part_schema(*inner));
            out
        }
        jadpo_std::Part::Standard(other) => match jadpo_std::standard_type(other) {
            Some(entry) if entry.codes().is_some() => {
                let mut out = Value::object();
                out.set("type", "string");
                out.set(
                    "enum",
                    entry
                        .codes()
                        .unwrap_or_default()
                        .iter()
                        .map(|(_, code)| Value::from(*code))
                        .collect::<Vec<_>>(),
                );
                out
            }
            Some(entry) => match jadpo_std::standard::declared(entry.name) {
                Some((builtin, rules)) => scalar_schema(builtin, &rules),
                None => standard_object_schema(entry),
            },
            None => Value::object(),
        },
        // A vector's item, a JSON number held as a single-precision float
        // (chunk 5.5s2).
        jadpo_std::Part::Float => {
            let mut out = Value::object();
            out.set("type", "number");
            out.set("format", "float");
            out
        }
        part => match part.builtin() {
            Some(builtin) => scalar_schema(builtin, &part.rules()),
            None => Value::object(),
        },
    }
}

/// A built-in type's schema, with its rules; a standard type stored and
/// sent as an object (`Money`, `ZonedTime`) is that object.
pub fn scalar_schema(builtin: Builtin, rules: &Rules) -> Value {
    let mut out = Value::object();
    let string = |out: &mut Value, format: Option<&str>| {
        out.set("type", "string");
        if let Some(format) = format {
            out.set("format", format);
        }
    };
    match builtin.representation() {
        Builtin::Bool => out.set("type", "boolean"),
        Builtin::Int if int_as_number(rules) => {
            out.set("type", "integer");
            if let Some(min) = rules.min.and_then(whole) {
                out.set("minimum", min);
            }
            if let Some(max) = rules.max.and_then(whole) {
                out.set("maximum", max);
            }
        }
        Builtin::Int => {
            string(&mut out, None);
            out.set("pattern", "^-?[0-9]+$");
            describe_range(&mut out, rules);
        }
        Builtin::Decimal => {
            string(&mut out, None);
            out.set("pattern", "^-?[0-9]+(\\.[0-9]+)?$");
            describe_range(&mut out, rules);
        }
        Builtin::Text => {
            let format = match builtin {
                Builtin::Email => Some("email"),
                Builtin::Url => Some("uri"),
                Builtin::IpAddress => Some("ip-address"),
                _ => rules.format.map(|_| "email"),
            };
            string(&mut out, format);
            lengths(&mut out, rules, "minLength", "maxLength");
            let standard = rules
                .standard
                .and_then(jadpo_std::standard_type)
                .and_then(|entry| match entry.holds {
                    jadpo_std::standard::Holds::Text { pattern, .. } => Some(pattern),
                    _ => None,
                });
            if let Some(pattern) = rules.pattern.as_deref().or(standard) {
                out.set("pattern", format!("^(?:{pattern})$"));
            }
            // A secret value is only ever sent to the server, never back
            // (chunk 5.5r2), and a password is hidden as it is typed.
            if let Some(secret) = rules
                .standard
                .and_then(jadpo_std::standard::security::secret_value)
            {
                out.set("writeOnly", true);
                if secret == jadpo_std::standard::security::PASSWORD {
                    out.set("format", "password");
                }
            }
        }
        Builtin::Bytes => {
            string(&mut out, None);
            out.set("contentEncoding", "base64");
        }
        Builtin::Uuid => string(&mut out, Some("uuid")),
        Builtin::Instant => string(&mut out, Some("date-time")),
        Builtin::CalendarDate => string(&mut out, Some("date")),
        Builtin::Duration => string(&mut out, Some("duration")),
        // An instant and the zone it is seen in (area 10).
        Builtin::ZonedTime => {
            out.set("type", "object");
            let mut instant = Value::object();
            string(&mut instant, Some("date-time"));
            let mut zone = Value::object();
            string(&mut zone, None);
            zone.set(
                "description",
                "An IANA time zone name, such as `Europe/London`.",
            );
            let mut properties = Value::object();
            properties.set("instant", instant);
            properties.set("zone", zone);
            out.set("properties", properties);
            out.set("required", vec!["instant", "zone"]);
            out.set("additionalProperties", false);
        }
        // An amount of money in minor units and its currency's code, as
        // Stripe sends it (area 19, "Money and payments").
        Builtin::Money => {
            out.set("type", "object");
            let [amount_field, currency_field] = jadpo_std::MONEY_FIELDS;
            let mut amount = Value::object();
            amount.set("type", "integer");
            if let Some(min) = rules.min.and_then(whole) {
                amount.set("minimum", min);
            }
            if let Some(max) = rules.max.and_then(whole) {
                amount.set("maximum", max);
            }
            amount.set(
                "description",
                "The amount in the currency's minor units: pence, cents.",
            );
            let mut currency = Value::object();
            string(&mut currency, None);
            // The one code its type fixes, or every code `Currency` has.
            let codes: Vec<&str> = match rules.currency.and_then(jadpo_std::money::code) {
                Some(code) => vec![code],
                None => jadpo_std::CURRENCIES
                    .iter()
                    .map(|(_, code)| *code)
                    .collect(),
            };
            currency.set("enum", codes);
            let mut properties = Value::object();
            properties.set(amount_field, amount);
            properties.set(currency_field, currency);
            out.set("properties", properties);
            out.set("required", vec![amount_field, currency_field]);
            out.set("additionalProperties", false);
        }
        // A physical quantity: its number as a decimal string, as area 2
        // sends every `Decimal`, and one of its kind's units by its code;
        // written in the base unit, read in any (area 19).
        Builtin::Quantity(kind) => {
            out.set("type", "object");
            let [value_field, unit_field] = jadpo_std::standard::QUANTITY_FIELDS;
            let mut value = Value::object();
            string(&mut value, None);
            value.set("pattern", "^-?[0-9]+(\\.[0-9]+)?$");
            value.set(
                "description",
                format!(
                    "The number in `unit`, written as text; sent in {}.",
                    kind.base_unit().meaning
                ),
            );
            let mut unit = Value::object();
            string(&mut unit, None);
            unit.set(
                "enum",
                kind.units()
                    .iter()
                    .map(|unit| unit.code)
                    .collect::<Vec<_>>(),
            );
            let mut properties = Value::object();
            properties.set(value_field, value);
            properties.set(unit_field, unit);
            out.set("properties", properties);
            out.set("required", vec![value_field, unit_field]);
            out.set("additionalProperties", false);
        }
        // Its canonical text, so a client never reads 0.2 as 0.2% (area 19).
        Builtin::Percentage => {
            string(&mut out, None);
            out.set("pattern", "^-?[0-9]+(\\.[0-9]+)?%$");
            out.set(
                "description",
                "A percentage written with `%`, such as `20%`.",
            );
        }
        Builtin::Unit => out.set("type", "null"),
        _ => string(&mut out, None),
    }
    out
}

fn describe_range(out: &mut Value, rules: &Rules) {
    let mut parts = Vec::new();
    if let Some(min) = rules.min {
        parts.push(format!("at least {min}"));
    }
    if let Some(max) = rules.max {
        parts.push(format!("at most {max}"));
    }
    if let Some(scale) = rules.scale {
        parts.push(format!("{scale} decimal places"));
    }
    if !parts.is_empty() {
        out.set(
            "description",
            format!("A number written as text: {}.", parts.join(", ")),
        );
    }
}

/// Rules as the record writes them: ` { min_length: 1, max_length: 200 }`,
/// or nothing.
pub fn rules_text(rules: &Rules) -> String {
    let pairs = rule_pairs(rules);
    if pairs.is_empty() {
        return String::new();
    }
    let shown: Vec<String> = pairs
        .into_iter()
        .map(|(name, value)| {
            if name == "pattern" {
                format!("{name}: \"{value}\"")
            } else {
                format!("{name}: {value}")
            }
        })
        .collect();
    format!(" {{ {} }}", shown.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rate_type_shows_the_currencies_it_fixes() {
        let entry = jadpo_std::standard_type(jadpo_std::standard::EXCHANGE_RATE).expect("a rate");
        let code = |schema: &Value, field: &str| {
            schema
                .get("properties")
                .and_then(|properties| properties.get(field))
                .and_then(|one| one.get("enum"))
                .and_then(Value::as_array)
                .map(<[Value]>::len)
        };
        let rules = Rules {
            rate_from: jadpo_std::money::ordinal_of_code("GBP"),
            ..Rules::default()
        };
        let typed = rate_currencies(standard_object_schema(entry), &rules);
        assert_eq!(
            typed
                .get("properties")
                .and_then(|p| p.get("from"))
                .and_then(|one| one.get("enum")),
            Some(&Value::Array(vec![Value::from("GBP")]))
        );
        assert!(
            code(&typed, "to").is_some_and(|codes| codes > 1),
            "a `to` it leaves open is any currency"
        );
    }
}
