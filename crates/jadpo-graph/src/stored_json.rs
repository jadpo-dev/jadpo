//! What a column holding a structured value as JSON reads (area 5,
//! "Storage types"; chunk 5.9c): the value's type as the route codec reads
//! it back, with every rule along the way, so a migration snapshot records
//! it and a change to the type, or to the field's own rules, is a change
//! to the schema like any other.
//!
//! [`reads_every`] says whether a type reads every value another type
//! wrote, conservatively: anything it can't show only widens what is read
//! counts as reading less, and the migration checks the stored values.

use std::collections::{BTreeMap, BTreeSet};

use crate::json::Value;
use crate::schema::Allows;

/// What a JSON column's type reads: the shape its value has and every
/// object and enum it names, by name, so a type that holds itself is
/// written once.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Reads {
    pub shape: Shape,
    pub types: BTreeMap<String, Named>,
}

/// One value's shape.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Shape {
    /// A value built on a built-in or standard type, held as its JSON
    /// scalar or its standard JSON, with the rules along its chain.
    Value(Scalar),
    /// `T?` inside the value: JSON's `null`, or a `T`.
    Optional(Box<Shape>),
    /// A list, or a set, whose JSON is an array.
    List {
        set: bool,
        item: Box<Shape>,
        min_length: Option<u64>,
        max_length: Option<u64>,
    },
    /// A map, whose JSON is an object keyed by each key's text.
    Map {
        key: Box<Shape>,
        value: Box<Shape>,
        min_length: Option<u64>,
        max_length: Option<u64>,
    },
    /// An object or an enum of the application's, by name.
    Named(String),
}

impl Default for Shape {
    fn default() -> Shape {
        Shape::Value(Scalar::default())
    }
}

/// A value built on a built-in or standard type: the type it is built on
/// (`Text`, `Int`, `Money`, `Currency`) and the rules its chain sets.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Scalar {
    pub ty: String,
    pub min: Option<String>,
    pub max: Option<String>,
    pub min_length: Option<u64>,
    pub max_length: Option<u64>,
    /// Every pattern along its chain, sorted.
    pub patterns: Vec<String>,
    pub scale: Option<u32>,
    /// Every other rule along its chain (a format, a currency, a
    /// duration's bounds), as text, sorted: any change to one counts as
    /// reading less.
    pub other: Vec<String>,
}

/// An object or an enum of the application's.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Named {
    /// Its fields, by name, and its rules across them, each as `name:
    /// condition`, sorted.
    Object {
        fields: BTreeMap<String, Shape>,
        rules: Vec<String>,
    },
    /// Its variants, by name, each with its fields when it carries any.
    Enum {
        variants: BTreeMap<String, Option<BTreeMap<String, Shape>>>,
    },
}

impl Reads {
    pub fn to_value(&self) -> Value {
        let mut value = Value::object();
        value.set("shape", self.shape.to_value());
        let mut types = Value::object();
        for (name, named) in &self.types {
            types.set(name, named.to_value());
        }
        value.set_some("types", types);
        value
    }

    pub fn from_value(value: &Value) -> Result<Reads, String> {
        let shape = Shape::from_value(value.get("shape").ok_or("it has no `shape`")?)?;
        let mut types = BTreeMap::new();
        if let Some(Value::Object(entries)) = value.get("types") {
            for (name, named) in entries {
                types.insert(name.clone(), Named::from_value(named)?);
            }
        }
        Ok(Reads { shape, types })
    }
}

impl Shape {
    fn to_value(&self) -> Value {
        let mut value = Value::object();
        match self {
            Shape::Value(scalar) => {
                value.set("value", &scalar.ty);
                set_bounds(&mut value, &scalar.min, &scalar.max);
                set_lengths(&mut value, scalar.min_length, scalar.max_length);
                value.set_some("patterns", strings(&scalar.patterns));
                if let Some(scale) = scalar.scale {
                    value.set("scale", i64::from(scale));
                }
                value.set_some("other", strings(&scalar.other));
            }
            Shape::Optional(inner) => value.set("optional", inner.to_value()),
            Shape::List {
                set,
                item,
                min_length,
                max_length,
            } => {
                value.set(if *set { "set" } else { "list" }, item.to_value());
                set_lengths(&mut value, *min_length, *max_length);
            }
            Shape::Map {
                key,
                value: item,
                min_length,
                max_length,
            } => {
                let mut map = Value::object();
                map.set("key", key.to_value());
                map.set("value", item.to_value());
                value.set("map", map);
                set_lengths(&mut value, *min_length, *max_length);
            }
            Shape::Named(name) => value.set("named", name),
        }
        value
    }

    fn from_value(value: &Value) -> Result<Shape, String> {
        let count = |key: &str| match value.get(key) {
            Some(Value::Int(n)) => u64::try_from(*n).ok(),
            _ => None,
        };
        let text = |key: &str| value.get(key).and_then(Value::as_str).map(String::from);
        if let Some(ty) = text("value") {
            return Ok(Shape::Value(Scalar {
                ty,
                min: text("min"),
                max: text("max"),
                min_length: count("min_length"),
                max_length: count("max_length"),
                patterns: names(value, "patterns"),
                scale: count("scale").and_then(|n| u32::try_from(n).ok()),
                other: names(value, "other"),
            }));
        }
        if let Some(inner) = value.get("optional") {
            return Ok(Shape::Optional(Box::new(Shape::from_value(inner)?)));
        }
        for (key, set) in [("list", false), ("set", true)] {
            if let Some(item) = value.get(key) {
                return Ok(Shape::List {
                    set,
                    item: Box::new(Shape::from_value(item)?),
                    min_length: count("min_length"),
                    max_length: count("max_length"),
                });
            }
        }
        if let Some(map) = value.get("map") {
            let part = |key: &str| -> Result<Box<Shape>, String> {
                Ok(Box::new(Shape::from_value(
                    map.get(key)
                        .ok_or_else(|| format!("a map with no `{key}`"))?,
                )?))
            };
            return Ok(Shape::Map {
                key: part("key")?,
                value: part("value")?,
                min_length: count("min_length"),
                max_length: count("max_length"),
            });
        }
        if let Some(name) = text("named") {
            return Ok(Shape::Named(name));
        }
        Err("a shape that is none of `value`, `optional`, `list`, `set`, `map` and `named`".into())
    }
}

impl Named {
    fn to_value(&self) -> Value {
        let mut value = Value::object();
        match self {
            Named::Object { fields, rules } => {
                let mut object = Value::object();
                object.set("fields", fields_value(fields));
                object.set_some("rules", strings(rules));
                value.set("object", object);
            }
            Named::Enum { variants } => {
                let mut entries = Value::object();
                for (name, fields) in variants {
                    match fields {
                        Some(fields) => entries.set(name, fields_value(fields)),
                        None => entries.set(name, Value::Null),
                    }
                }
                value.set("enum", entries);
            }
        }
        value
    }

    fn from_value(value: &Value) -> Result<Named, String> {
        if let Some(object) = value.get("object") {
            return Ok(Named::Object {
                fields: fields_from(object.get("fields").ok_or("an object with no `fields`")?)?,
                rules: names(object, "rules"),
            });
        }
        if let Some(Value::Object(entries)) = value.get("enum") {
            let mut variants = BTreeMap::new();
            for (name, fields) in entries {
                let fields = match fields {
                    Value::Null => None,
                    fields => Some(fields_from(fields)?),
                };
                variants.insert(name.clone(), fields);
            }
            return Ok(Named::Enum { variants });
        }
        Err("a named type that is neither an `object` nor an `enum`".into())
    }
}

fn fields_value(fields: &BTreeMap<String, Shape>) -> Value {
    let mut out = Value::object();
    for (name, shape) in fields {
        out.set(name, shape.to_value());
    }
    out
}

fn fields_from(value: &Value) -> Result<BTreeMap<String, Shape>, String> {
    let Value::Object(entries) = value else {
        return Err("fields that aren't an object".into());
    };
    entries
        .iter()
        .map(|(name, shape)| Ok((name.clone(), Shape::from_value(shape)?)))
        .collect()
}

fn set_bounds(value: &mut Value, min: &Option<String>, max: &Option<String>) {
    if let Some(min) = min {
        value.set("min", min);
    }
    if let Some(max) = max {
        value.set("max", max);
    }
}

fn set_lengths(value: &mut Value, min: Option<u64>, max: Option<u64>) {
    if let Some(n) = min {
        value.set("min_length", i64::try_from(n).unwrap_or(i64::MAX));
    }
    if let Some(n) = max {
        value.set("max_length", i64::try_from(n).unwrap_or(i64::MAX));
    }
}

fn strings(items: &[String]) -> Value {
    Value::Array(items.iter().map(Value::from).collect())
}

fn names(value: &Value, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

/// Whether `now` reads every value `was` read, as the route codec reads a
/// stored value back: never when it can't show it. An object reads its
/// members exactly, every field required and no other allowed, so a field
/// added or removed reads less; a variant added, a rule loosened or a part
/// made optional reads more.
pub fn reads_every(now: &Reads, was: &Reads) -> bool {
    Compare {
        now,
        was,
        seen: BTreeSet::new(),
    }
    .shape(&now.shape, &was.shape)
}

struct Compare<'r> {
    now: &'r Reads,
    was: &'r Reads,
    /// The pairs of named types being compared: met again, inside a type
    /// that holds itself, they read as each other unless something else
    /// says not.
    seen: BTreeSet<(String, String)>,
}

impl Compare<'_> {
    fn shape(&mut self, now: &Shape, was: &Shape) -> bool {
        match (now, was) {
            (Shape::Optional(now), Shape::Optional(was)) => self.shape(now, was),
            (Shape::Optional(now), was) => self.shape(now, was),
            (_, Shape::Optional(_)) => false,
            (Shape::Value(now), Shape::Value(was)) => scalar_reads(now, was),
            (
                Shape::List {
                    set: now_set,
                    item: now_item,
                    min_length: now_min,
                    max_length: now_max,
                },
                Shape::List {
                    set: was_set,
                    item: was_item,
                    min_length: was_min,
                    max_length: was_max,
                },
            ) => {
                // A set refuses a repeated item a list kept.
                (!now_set || *was_set)
                    && lengths_read(*now_min, *now_max, *was_min, *was_max)
                    && self.shape(now_item, was_item)
            }
            (
                Shape::Map {
                    key: now_key,
                    value: now_value,
                    min_length: now_min,
                    max_length: now_max,
                },
                Shape::Map {
                    key: was_key,
                    value: was_value,
                    min_length: was_min,
                    max_length: was_max,
                },
            ) => {
                lengths_read(*now_min, *now_max, *was_min, *was_max)
                    && self.shape(now_key, was_key)
                    && self.shape(now_value, was_value)
            }
            (Shape::Named(now), Shape::Named(was)) => self.named(now, was),
            _ => false,
        }
    }

    fn named(&mut self, now: &str, was: &str) -> bool {
        if !self.seen.insert((now.to_string(), was.to_string())) {
            return true;
        }
        let (now_reads, was_reads) = (self.now, self.was);
        match (now_reads.types.get(now), was_reads.types.get(was)) {
            (
                Some(Named::Object {
                    fields: now_fields,
                    rules: now_rules,
                }),
                Some(Named::Object {
                    fields: was_fields,
                    rules: was_rules,
                }),
            ) => {
                self.fields(now_fields, was_fields)
                    && now_rules.iter().all(|r| was_rules.contains(r))
            }
            (Some(Named::Enum { variants: now }), Some(Named::Enum { variants: was })) => was
                .iter()
                .all(|(name, was_fields)| match (now.get(name), was_fields) {
                    (Some(None), None) => true,
                    (Some(Some(now_fields)), Some(was_fields)) => {
                        self.fields(now_fields, was_fields)
                    }
                    _ => false,
                }),
            _ => false,
        }
    }

    /// An object's members are read exactly: the same names, each reading
    /// what it read.
    fn fields(&mut self, now: &BTreeMap<String, Shape>, was: &BTreeMap<String, Shape>) -> bool {
        now.len() == was.len()
            && now.iter().all(|(name, now_shape)| match was.get(name) {
                Some(was_shape) => self.shape(now_shape, was_shape),
                None => false,
            })
    }
}

/// Whether a value built on a built-in or standard type reads every value
/// another did: the same type and places, the same other rules, no bound
/// or length tighter or new, and no pattern new.
fn scalar_reads(now: &Scalar, was: &Scalar) -> bool {
    let allows = |scalar: &Scalar| Allows {
        values: Vec::new(),
        min: scalar.min.clone(),
        max: scalar.max.clone(),
        min_length: scalar.min_length,
        max_length: scalar.max_length,
        patterns: Vec::new(),
        json: None,
    };
    now.ty == was.ty
        && now.scale == was.scale
        && now.other == was.other
        && now.patterns.iter().all(|p| was.patterns.contains(p))
        && !crate::migration::narrows(&allows(was), &allows(now))
}

fn lengths_read(
    now_min: Option<u64>,
    now_max: Option<u64>,
    was_min: Option<u64>,
    was_max: Option<u64>,
) -> bool {
    let at_least = match (was_min, now_min) {
        (_, None) => true,
        (None, Some(_)) => false,
        (Some(was), Some(now)) => now <= was,
    };
    let at_most = match (was_max, now_max) {
        (_, None) => true,
        (None, Some(_)) => false,
        (Some(was), Some(now)) => now >= was,
    };
    at_least && at_most
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(max_length: u64) -> Shape {
        Shape::Value(Scalar {
            ty: "Text".into(),
            max_length: Some(max_length),
            ..Scalar::default()
        })
    }

    /// `Label = Object { line: Text { max_length: n }, note: <note> }` in a
    /// `List`, with `Delivery = Enum { pickup, courier: { company } }`.
    fn label(line: u64, note: Shape, rules: &[&str], variants: &[&str]) -> Reads {
        let mut types = BTreeMap::new();
        types.insert(
            "Label".to_string(),
            Named::Object {
                fields: [("line".to_string(), text(line)), ("note".to_string(), note)]
                    .into_iter()
                    .collect(),
                rules: rules.iter().map(|r| r.to_string()).collect(),
            },
        );
        let mut all: BTreeMap<String, Option<BTreeMap<String, Shape>>> = BTreeMap::new();
        for variant in variants {
            let fields = (*variant == "courier")
                .then(|| [("company".to_string(), text(20))].into_iter().collect());
            all.insert(variant.to_string(), fields);
        }
        types.insert("Delivery".to_string(), Named::Enum { variants: all });
        let mut object = BTreeMap::new();
        object.insert("label".to_string(), Shape::Named("Label".into()));
        object.insert("delivery".to_string(), Shape::Named("Delivery".into()));
        types.insert(
            "Parcel".to_string(),
            Named::Object {
                fields: object,
                rules: Vec::new(),
            },
        );
        Reads {
            shape: Shape::List {
                set: false,
                item: Box::new(Shape::Named("Parcel".into())),
                min_length: None,
                max_length: Some(3),
            },
            types,
        }
    }

    fn base() -> Reads {
        label(
            20,
            text(20),
            &["short: line != note"],
            &["pickup", "courier"],
        )
    }

    #[test]
    fn a_type_reads_what_it_wrote_and_survives_the_snapshot() {
        let reads = base();
        assert!(reads_every(&reads, &reads));
        let document = reads.to_value().to_document();
        let back = Reads::from_value(&crate::json::parse(&document).expect("json")).expect("reads");
        assert_eq!(back, reads);
    }

    #[test]
    fn a_change_that_only_widens_reads_every_stored_value() {
        let was = base();
        // A longer `max_length`, a part made optional, a rule gone, a
        // variant added, and a list allowed more items.
        let mut now = label(
            40,
            Shape::Optional(Box::new(text(20))),
            &[],
            &["pickup", "courier", "post"],
        );
        if let Shape::List { max_length, .. } = &mut now.shape {
            *max_length = Some(10);
        }
        assert!(reads_every(&now, &was));
    }

    #[test]
    fn a_change_that_may_narrow_does_not() {
        let was = base();
        let narrower = [
            label(
                5,
                text(20),
                &["short: line != note"],
                &["pickup", "courier"],
            ),
            label(
                20,
                text(20),
                &["short: line != note", "other: true"],
                &["pickup", "courier"],
            ),
            label(20, text(20), &["short: line != note"], &["courier"]),
            label(
                20,
                Shape::Value(Scalar {
                    ty: "Int".into(),
                    ..Scalar::default()
                }),
                &["short: line != note"],
                &["pickup", "courier"],
            ),
        ];
        for now in &narrower {
            assert!(!reads_every(now, &was), "{now:?}");
        }
        // A field added to an object, even an optional one, is a member
        // stored values lack; one removed is a member they hold that is
        // no longer read.
        let mut added = base();
        if let Some(Named::Object { fields, .. }) = added.types.get_mut("Label") {
            fields.insert("extra".into(), Shape::Optional(Box::new(text(5))));
        }
        assert!(!reads_every(&added, &was));
        assert!(!reads_every(&was, &added));
        // A part made required, a list made a set, fewer items.
        let optional = label(
            20,
            Shape::Optional(Box::new(text(20))),
            &["short: line != note"],
            &["pickup", "courier"],
        );
        assert!(!reads_every(&was, &optional));
        let mut set = base();
        if let Shape::List { set: s, .. } = &mut set.shape {
            *s = true;
        }
        assert!(!reads_every(&set, &was));
        assert!(reads_every(&was, &set));
        let mut fewer = base();
        if let Shape::List { max_length, .. } = &mut fewer.shape {
            *max_length = Some(2);
        }
        assert!(!reads_every(&fewer, &was));
    }

    #[test]
    fn a_type_that_holds_itself_is_compared_once() {
        let tree = |line: u64| {
            let mut types = BTreeMap::new();
            types.insert(
                "Tree".to_string(),
                Named::Object {
                    fields: [
                        ("name".to_string(), text(line)),
                        (
                            "children".to_string(),
                            Shape::List {
                                set: false,
                                item: Box::new(Shape::Named("Tree".into())),
                                min_length: None,
                                max_length: None,
                            },
                        ),
                    ]
                    .into_iter()
                    .collect(),
                    rules: Vec::new(),
                },
            );
            Reads {
                shape: Shape::Named("Tree".into()),
                types,
            }
        };
        assert!(reads_every(&tree(20), &tree(20)));
        assert!(reads_every(&tree(30), &tree(20)));
        assert!(!reads_every(&tree(10), &tree(20)));
    }
}
