//! The standard type reference's data (docs/documentation.md, "Standard
//! types"): each standard type's contract entry in `jadpo-std`, its stub in
//! `jadpo-check` with its description, and the standard-library operations
//! that take or give one, as `jadpo help --json` gives them under `types`
//! and `operations`. The docs build renders one page per type from this, so
//! the reference lists exactly what is built.

use jadpo_check::StdOperation;
use jadpo_check::types::TypeRef;
use jadpo_std::standard::groups::{GROUPS, group_of};
use jadpo_std::standard::{Holds, Json as Wire, STANDARD_TYPES, StandardType, Storage, TextForm};
use jadpo_std::{Builtin, Kind};
use jadpo_term::Json;

/// Every standard type, in its group's order (area 19, "The standard
/// types"), each group's types in the order the group lists them.
pub fn types_json() -> Json {
    let stubs = jadpo_check::standard_stubs();
    let operations = jadpo_check::documented_operations();
    let ordered = GROUPS
        .iter()
        .flat_map(|(_, names)| names.iter())
        .filter_map(|name| STANDARD_TYPES.iter().find(|entry| entry.name == *name));
    Json::array(ordered.map(|entry| {
        let stub = stubs.get(entry.name);
        let related: Vec<Json> = operations
            .iter()
            .filter(|operation| !baseline(&operation.name))
            .filter_map(|operation| {
                let kinds = jadpo_std::OPERATIONS
                    .iter()
                    .find(|typed| typed.qualified() == operation.name);
                let gives = gives(entry, operation, kinds);
                (gives || takes(entry, operation, kinds)).then(|| {
                    Json::object([
                        ("name", operation.name.as_str().into()),
                        ("gives", gives.into()),
                    ])
                })
            })
            .collect();
        Json::object([
            ("name", entry.name.into()),
            ("group", group_of(entry.name).map_or(Json::Null, Json::from)),
            (
                "summary",
                stub.and_then(|stub| stub.docs.as_deref())
                    .map_or(Json::Null, |docs| Json::from(paragraph(docs))),
            ),
            ("holds", holds_word(entry.holds).into()),
            (
                "base",
                stub.map_or(Json::Null, |stub| Json::from(stub.base.as_str())),
            ),
            (
                "arguments",
                Json::array(
                    stub.into_iter()
                        .flat_map(|stub| stub.arguments.iter())
                        .map(|argument| Json::from(argument.to_string())),
                ),
            ),
            ("fields", fields_json(entry, stub)),
            ("variants", variants_json(entry, stub)),
            (
                "pattern",
                pattern(entry.holds).map_or(Json::Null, Json::from),
            ),
            ("min", bound(entry.holds, true)),
            ("max", bound(entry.holds, false)),
            (
                "personal",
                personal(entry.holds).map_or(Json::Null, Json::from),
            ),
            ("text", text_word(entry.text).map_or(Json::Null, Json::from)),
            ("json", json_word(entry.json).into()),
            ("storage", storage_word(entry.storage).into()),
            (
                "format",
                Json::object([(
                    "options",
                    Json::array(entry.format.options.iter().map(|option| {
                        Json::object([
                            ("name", option.name.into()),
                            ("needed", option.needed.into()),
                        ])
                    })),
                )]),
            ),
            ("ordered", entry.ordered.into()),
            ("release", entry.release.into()),
            (
                "rules",
                Json::array(
                    jadpo_std::standard::across::rules(entry.name)
                        .iter()
                        .map(|rule| {
                            Json::object([
                                ("name", rule.name.into()),
                                ("written", rule.written.into()),
                            ])
                        }),
                ),
            ),
            (
                "examples",
                Json::object([
                    (
                        "valid",
                        Json::array(entry.examples.valid.iter().map(|(text, shown)| {
                            Json::object([("text", (*text).into()), ("shown", (*shown).into())])
                        })),
                    ),
                    (
                        "format",
                        entry.examples.format.map_or(Json::Null, Json::from),
                    ),
                    ("of", entry.examples.of.map_or(Json::Null, Json::from)),
                    (
                        "malformed",
                        Json::array(entry.examples.malformed.iter().copied()),
                    ),
                ]),
            ),
            ("operations", Json::Array(related)),
        ])
    }))
}

/// Every standard-library operation, by its qualified name.
pub fn operations_json() -> Json {
    Json::array(
        jadpo_check::documented_operations()
            .iter()
            .map(|operation| {
                Json::object([
                    ("name", operation.name.as_str().into()),
                    (
                        "params",
                        Json::array(operation.params.iter().map(|(name, ty)| {
                            let (ty, optional) = match ty {
                                TypeRef::Nullable(inner) => (inner.as_ref(), true),
                                other => (other, false),
                            };
                            Json::object([
                                ("name", name.as_str().into()),
                                ("type", ty.to_string().into()),
                                ("optional", optional.into()),
                            ])
                        })),
                    ),
                    ("result", operation.result.to_string().into()),
                    (
                        "summary",
                        operation
                            .docs
                            .as_deref()
                            .map_or(Json::Null, |docs| Json::from(paragraph(docs))),
                    ),
                ])
            }),
    )
}

/// The contract's own operations, which every standard type has and its
/// page describes once: `text.parse`, `text.format`, `text.from` and JSON's.
fn baseline(name: &str) -> bool {
    matches!(name, "text.parse" | "text.format" | "text.from") || name.starts_with("json.")
}

/// Whether `operation` takes a value of `entry`'s type: by its name in a
/// parameter as written, or by the kind its stub's `T` or `Quantity`
/// stands for.
fn takes(
    entry: &StandardType,
    operation: &StdOperation,
    kinds: Option<&jadpo_std::Operation>,
) -> bool {
    operation
        .params
        .iter()
        .any(|(_, ty)| mentions(ty, entry.name))
        || kinds.is_some_and(|typed| typed.params.iter().any(|param| fits(entry, &param.kind)))
}

/// Whether `operation` gives a value of `entry`'s type, as [`takes`] reads
/// its result.
fn gives(
    entry: &StandardType,
    operation: &StdOperation,
    kinds: Option<&jadpo_std::Operation>,
) -> bool {
    mentions(&operation.result, entry.name) || kinds.is_some_and(|typed| fits(entry, &typed.result))
}

/// Whether `ty` names `name` anywhere: `List<Money>` names `Money`.
fn mentions(ty: &TypeRef, name: &str) -> bool {
    match ty {
        TypeRef::Path(names, args) => {
            names.last().is_some_and(|last| last == name)
                || args.iter().any(|arg| mentions(arg, name))
        }
        TypeRef::Nullable(inner) => mentions(inner, name),
        TypeRef::Invalid => false,
    }
}

/// Whether a parameter or result of `kind` takes `entry`'s values, where
/// its stub writes a type variable for several types.
fn fits(entry: &StandardType, kind: &Kind) -> bool {
    match (kind, entry.holds) {
        (Kind::Quantity, Holds::Builtin(Builtin::Quantity(_))) => true,
        (Kind::OneOf(builtins), Holds::Builtin(builtin)) => builtins.contains(&builtin),
        (Kind::Range, Holds::Range) => true,
        (Kind::Range, _) => jadpo_std::range_type(entry.name).is_some(),
        (Kind::Shapes(names), _) => names.contains(&entry.name),
        (Kind::Std(name) | Kind::Enum(name), _) => *name == entry.name,
        (Kind::Builtin(builtin), Holds::Builtin(held)) => *builtin == held,
        _ => false,
    }
}

/// A `///` description as one paragraph.
fn paragraph(docs: &str) -> String {
    docs.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn holds_word(holds: Holds) -> &'static str {
    match holds {
        Holds::Builtin(_) => "builtin",
        Holds::Enum(_) => "enum",
        Holds::Object(_) => "object",
        Holds::Range => "range",
        Holds::Text { .. } => "text",
        Holds::Whole { .. } => "whole",
        Holds::Bounded { .. } => "bounded",
        Holds::Choice(_) => "choice",
        Holds::List(_) => "list",
        Holds::Map(..) => "map",
        Holds::Bytes => "bytes",
        Holds::Feature => "feature",
        Holds::FeatureCollection => "featureCollection",
    }
}

/// An object's fields as its stub writes them.
fn fields_json(entry: &StandardType, stub: Option<&jadpo_check::StandardStub>) -> Json {
    let fields: Vec<Json> = match (entry.holds, stub) {
        (
            Holds::Object(_) | Holds::Range | Holds::Feature | Holds::FeatureCollection,
            Some(stub),
        ) => stub
            .members
            .iter()
            .filter_map(|(name, ty)| Some(field_json(name, ty.as_ref()?)))
            .collect(),
        _ => Vec::new(),
    };
    Json::Array(fields)
}

fn field_json(name: &str, ty: &TypeRef) -> Json {
    let (ty, optional) = match ty {
        TypeRef::Nullable(inner) => (inner.as_ref(), true),
        other => (other, false),
    };
    Json::object([
        ("name", name.into()),
        ("type", ty.to_string().into()),
        ("optional", optional.into()),
    ])
}

/// An enum's variants with their codes, or a choice's with their fields.
fn variants_json(entry: &StandardType, stub: Option<&jadpo_check::StandardStub>) -> Json {
    match (entry.holds, stub) {
        (Holds::Enum(table), _) => Json::array(table.iter().map(|(variant, code)| {
            Json::object([
                ("name", (*variant).into()),
                ("code", (*code).into()),
                ("fields", Json::Array(Vec::new())),
            ])
        })),
        (Holds::Choice(_), Some(stub)) => {
            Json::array(stub.payloads.iter().map(|(variant, fields)| {
                Json::object([
                    ("name", variant.as_str().into()),
                    ("code", variant.as_str().into()),
                    (
                        "fields",
                        Json::array(
                            fields
                                .iter()
                                .filter_map(|(name, ty)| Some(field_json(name, ty.as_ref()?))),
                        ),
                    ),
                ])
            }))
        }
        _ => Json::Array(Vec::new()),
    }
}

fn pattern(holds: Holds) -> Option<&'static str> {
    match holds {
        Holds::Text { pattern, .. } => Some(pattern),
        _ => None,
    }
}

fn personal(holds: Holds) -> Option<&'static str> {
    match holds {
        Holds::Text { personal, .. } => personal,
        _ => None,
    }
}

/// A whole number's or a bounded value's least (`min`) or greatest value,
/// as its rules write it.
fn bound(holds: Holds, min: bool) -> Json {
    match holds {
        Holds::Whole {
            min: low,
            max: high,
        } => Json::from(if min { low } else { high }),
        Holds::Bounded {
            min: low,
            max: high,
            ..
        } => {
            let value = if min { low } else { high };
            value.map_or(Json::Null, |value| Json::from(value.to_string()))
        }
        _ => Json::Null,
    }
}

/// Whether `text.from` writes it and `text.parse` reads it (`canonical`),
/// only `text.parse` reads it (`secret`), or neither (`null`).
fn text_word(text: Option<TextForm>) -> Option<&'static str> {
    match text? {
        TextForm::Secret => Some("secret"),
        _ => Some("canonical"),
    }
}

fn json_word(json: Wire) -> &'static str {
    match json {
        Wire::Text => "text",
        Wire::Fields(_) | Wire::Parts(_) | Wire::Members(_) => "object",
        Wire::Tagged => "tagged",
        Wire::Number => "number",
        Wire::Array => "array",
        Wire::Keyed => "keyed",
        Wire::Secret => "secret",
        Wire::GeoJson => "geojson",
        Wire::Feature => "feature",
    }
}

fn storage_word(storage: Storage) -> &'static str {
    match storage {
        Storage::Column(_) => "column",
        Storage::Columns { .. } => "columns",
        Storage::Geography => "geography",
        Storage::Vector => "vector",
        Storage::Never => "never",
    }
}

#[cfg(test)]
mod tests {
    use super::types_json;
    use jadpo_term::Json;

    /// A member of an object.
    trait Get {
        fn get(&self, key: &str) -> Option<&Json>;
    }

    impl Get for Json {
        fn get(&self, key: &str) -> Option<&Json> {
            match self {
                Json::Object(entries) => entries
                    .iter()
                    .find(|(name, _)| name == key)
                    .map(|(_, value)| value),
                _ => None,
            }
        }
    }

    fn entry(name: &str) -> Json {
        let Json::Array(types) = types_json() else {
            panic!("types is a list");
        };
        types
            .into_iter()
            .find(|entry| entry.get("name") == Some(&Json::from(name)))
            .unwrap_or_else(|| panic!("{name} is listed"))
    }

    fn names(value: Option<&Json>) -> Vec<String> {
        let Some(Json::Array(items)) = value else {
            return Vec::new();
        };
        items
            .iter()
            .filter_map(|item| match item.get("name") {
                Some(Json::Str(name)) => Some(name.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn every_standard_type_has_an_entry_with_its_group_and_description() {
        let Json::Array(types) = types_json() else {
            panic!("types is a list");
        };
        assert_eq!(types.len(), jadpo_std::standard::STANDARD_TYPES.len());
        for entry in &types {
            assert!(
                matches!(entry.get("group"), Some(Json::Str(_))),
                "{entry:?}"
            );
            assert!(
                matches!(entry.get("summary"), Some(Json::Str(_))),
                "{entry:?}"
            );
        }
    }

    #[test]
    fn money_lists_the_operations_that_take_or_give_one() {
        let money = entry("Money");
        let operations = names(money.get("operations"));
        for name in [
            "money.allocate",
            "money.from_minor",
            "money.of",
            "money.split",
        ] {
            assert!(operations.contains(&name.to_string()), "{operations:?}");
        }
        assert!(
            !operations.contains(&"text.format".to_string()),
            "the baseline is said once"
        );
        assert_eq!(
            money.get("group"),
            Some(&Json::from("Money, payments, banking"))
        );
    }

    #[test]
    fn an_operation_says_whether_it_gives_one() {
        let money = entry("Money");
        let Some(Json::Array(operations)) = money.get("operations") else {
            panic!("operations is a list");
        };
        let gives = |name: &str| {
            operations
                .iter()
                .find(|operation| operation.get("name") == Some(&Json::from(name)))
                .and_then(|operation| operation.get("gives"))
                .cloned()
        };
        assert_eq!(gives("money.of"), Some(Json::Bool(true)));
        assert_eq!(gives("money.minor"), Some(Json::Bool(false)));
    }

    #[test]
    fn a_quantity_and_a_range_are_matched_by_their_kind() {
        assert!(names(entry("Length").get("operations")).contains(&"units.of".to_string()));
        assert!(
            names(entry("DateRange").get("operations")).contains(&"range.contains".to_string())
        );
    }

    #[test]
    fn an_object_gives_its_fields_and_a_choice_its_variants() {
        let fields = names(entry("PostalAddress").get("fields"));
        assert_eq!(fields.first().map(String::as_str), Some("lines"));
        let variants = names(entry("Discount").get("variants"));
        assert_eq!(variants, ["amount_off", "percentage_off"]);
        let rules = names(entry("DateRange").get("rules"));
        assert!(!rules.is_empty());
    }
}
