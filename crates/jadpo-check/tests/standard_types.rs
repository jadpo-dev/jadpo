//! The one harness every standard type runs through, in the checker (area
//! 19, "Every standard type meets one contract"): each type's
//! stub declaration agrees with its data entry in `jadpo-std`, and the
//! contract's calls and `<` accept a value of the type exactly as its entry
//! says. `jadpo-runtime`'s `standard_types` tests are the runtime's half,
//! over the same entries. A new type is a stub and an entry, not a new
//! test.

use crate::support;

use jadpo_check::StandardStub;
use jadpo_db::Layout;
use jadpo_std::Builtin;
use jadpo_std::standard::{Holds, STANDARD_TYPES, StandardType};

fn check(source: &str) -> Vec<String> {
    support::rules(Layout::Loose, &[("main.jadpo", source)])
}

/// The declarations a check of `entry`'s values starts from: a named type
/// built on a built-in standard type (a signature takes named types), and
/// the types the calls give and fail with.
fn types(entry: &StandardType) -> (String, String) {
    let declared = match entry.holds {
        // With the settings its examples take: a `Money` type's currency,
        // since only amounts in one currency are ordered.
        Holds::Builtin(_) if !entry.examples.settings.is_empty() => format!(
            "type Shown = {} {{\n    {}\n}}\n\n",
            entry.name, entry.examples.settings
        ),
        Holds::Builtin(_) => format!("type Shown = {} {{}}\n\n", entry.name),
        _ => String::new(),
    };
    let value = match (entry.holds, entry.examples.of) {
        (Holds::Builtin(_), _) => "Shown".to_string(),
        // A range of its examples' type (area 19).
        (_, Some(item)) => format!("{}<{item}>", entry.name),
        _ => entry.name.to_string(),
    };
    (
        format!(
            "{declared}type Wire = Text {{}}\n\ntype Answer = Bool {{}}\n\ntype Unreadable = Failure {{\n    kind: FailureKind.invalid_value\n}}\n\n"
        ),
        value,
    )
}

/// A part as a stub writes it: the type it is built on, `Text` for a
/// line, another standard type by its name, `T?` for a part that may be
/// `none`, `List<T>` for a list.
fn written(part: jadpo_std::Part) -> jadpo_check::types::TypeRef {
    use jadpo_check::types::TypeRef;
    match part {
        jadpo_std::Part::Optional(inner) => TypeRef::Nullable(Box::new(written(*inner))),
        jadpo_std::Part::List(inner) => {
            TypeRef::Path(vec!["List".to_string()], vec![written(*inner)])
        }
        jadpo_std::Part::Set(inner) => {
            TypeRef::Path(vec!["Set".to_string()], vec![written(*inner)])
        }
        jadpo_std::Part::Standard(other) => TypeRef::Path(vec![other.to_string()], Vec::new()),
        part => TypeRef::Path(
            vec![part.builtin().map_or("", Builtin::name).to_string()],
            Vec::new(),
        ),
    }
}

#[test]
fn every_stub_agrees_with_its_data_entry() {
    let stubs = jadpo_check::standard_stubs();
    let names: Vec<&str> = stubs.keys().map(String::as_str).collect();
    let entries: Vec<&str> = STANDARD_TYPES.iter().map(|entry| entry.name).collect();
    assert_eq!(
        names, entries,
        "a stub for every entry, an entry for every stub"
    );
    for entry in STANDARD_TYPES {
        let StandardStub {
            base,
            arguments,
            members,
            payloads,
        } = &stubs[entry.name];
        match entry.holds {
            Holds::Builtin(builtin) => {
                assert_eq!(base, builtin.name(), "{} is built in", entry.name);
                assert_eq!(base, entry.name);
                assert!(members.is_empty());
            }
            Holds::Enum(codes) => {
                assert_eq!(base, "Enum", "{}", entry.name);
                let listed: Vec<&str> = members.iter().map(|(name, _)| name.as_str()).collect();
                let variants: Vec<&str> = codes.iter().map(|(name, _)| *name).collect();
                // A long table isn't repeated in the stub.
                assert!(
                    listed.is_empty() || listed == variants,
                    "{}'s stub lists its variants as its entry does",
                    entry.name
                );
                let known = jadpo_std::enums()
                    .into_iter()
                    .find(|(name, _)| *name == entry.name)
                    .map(|(_, variants)| variants)
                    .unwrap_or_default();
                assert_eq!(
                    known, variants,
                    "{} is the enum the runtime reads",
                    entry.name
                );
            }
            Holds::Object(fields) => {
                assert_eq!(base, "Object", "{}", entry.name);
                let declared: Vec<(String, String)> = members
                    .iter()
                    .map(|(name, ty)| (name.clone(), format!("{ty:?}")))
                    .collect();
                let expected: Vec<(String, String)> = fields
                    .iter()
                    .map(|(name, part): &(&str, jadpo_std::Part)| {
                        ((*name).to_string(), format!("{:?}", Some(written(*part))))
                    })
                    .collect();
                assert_eq!(declared, expected, "{}'s fields", entry.name);
            }
            // A type built on a built-in type, its bounds in its entry
            // (chunk 5.5i3).
            Holds::Bounded { of, .. } => {
                assert_eq!(base, of.name(), "{} is built on {}", entry.name, of.name());
                assert!(
                    members.is_empty(),
                    "{}'s bounds are its entry's",
                    entry.name
                );
            }
            // A choice: an enum whose variants carry their entry's parts
            // as fields, each written as the type it is built on (chunk
            // 5.5i3).
            Holds::Choice(variants) => {
                assert_eq!(base, "Enum", "{}", entry.name);
                let listed: Vec<&str> = members.iter().map(|(name, _)| name.as_str()).collect();
                let names: Vec<&str> = variants.iter().map(|(name, _)| *name).collect();
                assert_eq!(listed, names, "{}'s variants", entry.name);
                let declared: Vec<(String, Vec<(String, String)>)> = payloads
                    .iter()
                    .map(|(variant, fields)| {
                        let fields = fields
                            .iter()
                            .map(|(name, ty)| (name.clone(), format!("{ty:?}")))
                            .collect();
                        (variant.clone(), fields)
                    })
                    .collect();
                let expected: Vec<(String, Vec<(String, String)>)> = variants
                    .iter()
                    .map(|(variant, fields)| {
                        let fields = fields
                            .iter()
                            .map(|(name, part)| {
                                ((*name).to_string(), format!("{:?}", Some(written(*part))))
                            })
                            .collect();
                        ((*variant).to_string(), fields)
                    })
                    .collect();
                assert_eq!(declared, expected, "{}'s variants' fields", entry.name);
            }
            // A type built on `Text` the standard library declares, its
            // check in its entry (area 19, chunk 5.5i), or on the standard
            // type it is a narrower form of (chunk 5.5n).
            Holds::Text { .. } => {
                let wide = jadpo_std::standard::within(entry.name).unwrap_or("Text");
                assert_eq!(base, wide, "{} is text", entry.name);
                assert!(members.is_empty(), "{}'s check is its entry's", entry.name);
            }
            // A type built on `Bytes`, its check in its entry (chunk
            // 5.5r3).
            Holds::Bytes => {
                assert_eq!(base, "Bytes", "{} is bytes", entry.name);
                assert!(members.is_empty(), "{}'s check is its entry's", entry.name);
            }
            // A type built on `Int`, its range in its entry (chunk 5.5n).
            Holds::Whole { .. } => {
                assert_eq!(base, "Int", "{} is a whole number", entry.name);
                assert!(members.is_empty(), "{}'s range is its entry's", entry.name);
            }
            // A named collection, built on a list or a map of its parts
            // (chunk 5.5o2).
            Holds::List(item) => {
                assert_eq!(base, "List", "{}", entry.name);
                assert_eq!(
                    format!("{arguments:?}"),
                    format!("{:?}", [written(item)]),
                    "{}'s items",
                    entry.name
                );
                assert!(members.is_empty(), "{}", entry.name);
            }
            Holds::Map(key, value) => {
                assert_eq!(base, "Map", "{}", entry.name);
                assert_eq!(
                    format!("{arguments:?}"),
                    format!("{:?}", [written(key), written(value)]),
                    "{}'s entries",
                    entry.name
                );
                assert!(members.is_empty(), "{}", entry.name);
            }
            Holds::Range => {
                assert_eq!(base, "Object", "{}", entry.name);
                let end = jadpo_check::types::TypeRef::Nullable(Box::new(
                    jadpo_check::types::TypeRef::Path(vec!["T".to_string()], Vec::new()),
                ));
                let expected: Vec<(String, Option<jadpo_check::types::TypeRef>)> =
                    jadpo_std::RANGE_FIELDS
                        .iter()
                        .map(|name| ((*name).to_string(), Some(end.clone())))
                        .collect();
                assert_eq!(members, &expected, "{}'s ends", entry.name);
            }
            // A feature's geometry `G` and properties `P`, and a
            // collection's features, each a `Feature<G, P>` (area 20).
            Holds::Feature | Holds::FeatureCollection => {
                use jadpo_check::types::TypeRef;
                assert_eq!(base, "Object", "{}", entry.name);
                let var = |name: &str| TypeRef::Path(vec![name.to_string()], Vec::new());
                let expected: Vec<(String, Option<TypeRef>)> = if entry.holds == Holds::Feature {
                    vec![
                        ("geometry".to_string(), Some(var("G"))),
                        ("properties".to_string(), Some(var("P"))),
                    ]
                } else {
                    vec![(
                        "features".to_string(),
                        Some(TypeRef::Path(
                            vec!["List".to_string()],
                            vec![TypeRef::Path(
                                vec!["Feature".to_string()],
                                vec![var("G"), var("P")],
                            )],
                        )),
                    )]
                };
                assert_eq!(members, &expected, "{}'s fields", entry.name);
            }
        }
    }
}

#[test]
fn every_standard_type_meets_the_contract_in_the_checker() {
    for entry in STANDARD_TYPES {
        let name = entry.name;
        // A feature takes two type arguments and has no text or form for
        // people; `geo_interop`'s tests check it (area 20).
        if matches!(entry.holds, Holds::Feature | Holds::FeatureCollection) {
            continue;
        }
        let (declared, value) = types(entry);
        // `text.format`, with the options its entry takes, and none other:
        // a range's are its item type's.
        let formatted = entry
            .examples
            .of
            .and_then(jadpo_std::standard_type)
            .unwrap_or(entry);
        let mut options = String::new();
        if formatted.format_option("zone").is_some() {
            options.push_str(", zone: Zone.utc");
        }
        if let Some(format) = entry.examples.format {
            options.push_str(&format!(", format: TimeFormat.{format}"));
        }
        let format = |options: &str| {
            check(&format!(
                "{declared}function f(value: {value}, shown: Locale) -> PresentationText {{\n    return text.format(value, locale: shown{options})\n}}\n"
            ))
        };
        // A secret value is read from text and never written, shown or
        // ordered (chunk 5.5r2).
        if entry.text == Some(jadpo_std::standard::TextForm::Secret) {
            assert_eq!(format(""), ["type.secret_exposed"], "{name}: never shown");
            assert_eq!(
                check(&format!(
                    "{declared}function f(value: {value}) -> Wire {{\n    return Wire(text.from(value))\n}}\n"
                )),
                ["type.secret_exposed"],
                "{name}: never written"
            );
            assert_eq!(
                check(&format!(
                    "{declared}function f(raw: Wire) fails Unreadable -> {value} {{\n    var read: {value} = attempt text.parse(raw, as: {value}) {{\n        malformed: Unreadable\n    }}\n    return read\n}}\n"
                )),
                Vec::<String>::new(),
                "{name}: read from text"
            );
            assert_eq!(
                check(&format!(
                    "{declared}function f(a: {value}, b: {value}) -> Answer {{\n    return Answer(a < b)\n}}\n"
                )),
                ["type.secret_exposed"],
                "{name}: never ordered"
            );
            assert_eq!(
                check(&format!(
                    "{declared}function f(a: {value}, b: {value}) -> Answer {{\n    return Answer(a == b)\n}}\n"
                )),
                Vec::<String>::new(),
                "{name}: compared"
            );
            continue;
        }
        assert_eq!(
            format(&options),
            Vec::<String>::new(),
            "{name}: text.format"
        );
        if formatted.format_option("zone").is_none() {
            assert_eq!(
                format(&format!("{options}, zone: Zone.utc")),
                ["type.temporal_option_unused"],
                "{name}: `zone:` means nothing"
            );
        } else {
            let without = options.replace(", zone: Zone.utc", "");
            assert_eq!(
                format(&without),
                ["type.temporal_zone_required"],
                "{name}: `zone:` is needed"
            );
        }
        // `text.from`: the canonical text, except for a type built on
        // `Text`, which is text already.
        let has_text = entry.text.is_some()
            && !matches!(entry.holds, Holds::Text { .. })
            && !matches!(entry.holds, Holds::Builtin(builtin) if builtin.representation() == Builtin::Text);
        let from = check(&format!(
            "{declared}function f(value: {value}) -> Wire {{\n    return Wire(text.from(value))\n}}\n"
        ));
        if has_text {
            assert_eq!(from, Vec::<String>::new(), "{name}: text.from");
        } else {
            assert_eq!(from, ["type.mismatch"], "{name}: text.from takes no text");
        }
        // `text.parse`: the type, failing `malformed`, and by the rules
        // its examples' settings give it.
        // A type with rules of its own breaks them by name: a `Money`
        // type's currency, a `TaxRate`'s bounds (chunk 5.5i3).
        // So does one with rules across its parts (chunk 5.5t2).
        let bounded = matches!(entry.holds, Holds::Bounded { min, max, .. } if min.is_some() || max.is_some())
            || !jadpo_std::standard::across::rules(name).is_empty();
        let mut others = if entry.examples.settings.is_empty() && !bounded {
            String::new()
        } else {
            "\n        invalid: Unreadable".to_string()
        };
        // A type with no single text form (`PersonName`) has none to read.
        let parsed: &[&str] = if entry.text.is_some() {
            &[]
        } else {
            &["type.mismatch"]
        };
        // A quantity's text may have a unit of another kind (area 19).
        if matches!(entry.holds, Holds::Builtin(Builtin::Quantity(_))) {
            others.push_str("\n        unknown_unit: Unreadable");
        }
        // Any text is `Html` once it is made safe (chunk 5.5o): reading
        // one never fails, so it needs no `attempt`.
        if jadpo_std::standard::markup::sanitises(name) {
            assert_eq!(
                check(&format!(
                    "{declared}function f(raw: Wire) -> {value} {{\n    return text.parse(raw, as: {value})\n}}\n"
                )),
                Vec::<String>::new(),
                "{name}: text.parse never fails"
            );
        } else {
            assert_eq!(
                check(&format!(
                    "{declared}function f(raw: Wire) fails Unreadable -> {value} {{\n    var read: {value} = attempt text.parse(raw, as: {value}) {{\n        malformed: Unreadable{others}\n    }}\n    return read\n}}\n"
                )),
                parsed,
                "{name}: text.parse"
            );
        }
        // `<`, where the type is ordered.
        let ordered = check(&format!(
            "{declared}function f(a: {value}, b: {value}) -> Answer {{\n    return Answer(a < b)\n}}\n"
        ));
        assert_eq!(
            ordered.is_empty(),
            entry.ordered,
            "{name}: `<` exactly when ordered ({ordered:?})"
        );
    }
}

#[test]
fn the_contract_refuses_what_has_no_contract() {
    let types = "type Wire = Text {}\n\ntype Count = Int {}\n\ntype Shade = Enum {\n    plain\n    tinted\n}\n\ntype Unreadable = Failure {\n    kind: FailureKind.invalid_value\n}\n\n";
    let cases: &[(&str, &[&str])] = &[
        // A number has canonical text, and a format for people since
        // chunk 5.5d (area 19, "Numbers").
        (
            "function f(n: Count) -> Wire {\n    return Wire(text.from(n))\n}\n",
            &[],
        ),
        (
            "function f(n: Count, shown: Locale) -> PresentationText {\n    return text.format(n, locale: shown)\n}\n",
            &[],
        ),
        // An application's enum has no text of its own.
        (
            "function f(shade: Shade) -> Wire {\n    return Wire(text.from(shade))\n}\n",
            &["type.mismatch"],
        ),
        // `as:` is a type written out.
        (
            "function f(raw: Wire, count: Count) fails Unreadable -> Count {\n    var read: Count = attempt text.parse(raw, as: count) {\n        invalid: Unreadable\n    }\n    return read\n}\n",
            &["type.mismatch"],
        ),
        // Text read as text can't be malformed.
        (
            "function f(raw: Wire) -> Wire {\n    return text.parse(raw, as: Wire)\n}\n",
            &[],
        ),
        // A number read from text: `malformed`, and its rules.
        (
            "function f(raw: Wire) fails Unreadable -> Count {\n    var read: Count = attempt text.parse(raw, as: Count) {\n        malformed: Unreadable\n    }\n    return read\n}\n",
            &[],
        ),
    ];
    for (source, expected) in cases {
        assert_eq!(&check(&format!("{types}{source}")), expected, "{source}");
    }
}

/// Area 19's "Names each release adds", the release notes' list, is every
/// data entry by its release (question 4).
#[test]
fn the_release_notes_list_each_release_s_names() {
    let page = include_str!("../../../docs/language/19-standard-library.md");
    let start = page
        .find("| Release | Standard types it adds |")
        .expect("the release notes' table");
    let listed: Vec<(String, Vec<String>)> = page[start..]
        .lines()
        .skip(2)
        .take_while(|line| line.starts_with('|'))
        .map(|line| {
            let cells: Vec<&str> = line.trim_matches('|').split('|').map(str::trim).collect();
            let names = cells[1]
                .split(", ")
                .map(|name| name.trim_matches('`').to_string())
                .collect();
            (cells[0].to_string(), names)
        })
        .collect();
    let entries: Vec<(String, Vec<String>)> = jadpo_std::standard::releases()
        .into_iter()
        .map(|(release, names)| {
            (
                release.to_string(),
                names.into_iter().map(str::to_string).collect(),
            )
        })
        .collect();
    assert_eq!(listed, entries);
}

/// A field declared `InstantRange` is read through, as a value of the type
/// is (chunk 5.5a2).
#[test]
fn an_instant_range_field_has_its_start_and_end() {
    let source = "type Booking = Object {\n    slot: InstantRange\n    held: InstantRange?\n}\n\ntype SlotStart = Instant {}\n\nfunction start_of(booking: Booking) -> SlotStart {\n    return SlotStart(booking.slot.start)\n}\n";
    assert_eq!(check(source), Vec::<String>::new());
    let unknown = source.replace("booking.slot.start", "booking.slot.begin");
    assert_eq!(check(&unknown), ["type.unknown_field"]);
    let nullable = source.replace("booking.slot.start", "booking.held.start");
    assert_eq!(check(&nullable), ["type.nullable_selection"]);
}

/// Every standard type is described in OpenAPI as its entry's JSON is
/// sent (area 19): its canonical text as a string, an object of the
/// entry's fields, and an enum by its codes, never by its variants'
/// names.
#[test]
fn every_standard_type_is_described_in_openapi_as_it_is_sent() {
    use jadpo_check::routes::wire::{scalar_schema, standard_object_schema, standard_schema};
    use jadpo_std::config::Spelling;
    use jadpo_std::standard::Json;
    for entry in STANDARD_TYPES {
        let name = entry.name;
        let schema = match entry.holds {
            Holds::Builtin(_) => standard_schema(entry),
            Holds::Text { pattern, .. } => {
                let rules = jadpo_std::standard::text_rules(name).expect("a text type");
                let schema = scalar_schema(Builtin::Text, &rules);
                assert_eq!(
                    schema.get("pattern").and_then(|v| v.as_str()),
                    Some(format!("^(?:{pattern})$").as_str()),
                    "{name}: OpenAPI shows its pattern"
                );
                schema
            }
            Holds::Whole { .. } | Holds::Bytes => {
                let (builtin, rules) = jadpo_std::standard::declared(name).expect("its rules");
                scalar_schema(builtin, &rules)
            }
            // A geometry: its GeoJSON object (chunk 5.5s).
            _ if entry.json == Json::GeoJson => {
                let schema = standard_object_schema(entry);
                let kinds: Vec<_> = match schema.get("oneOf").and_then(|v| v.as_array()) {
                    Some(each) => each.to_vec(),
                    None => vec![schema],
                };
                for kind in kinds {
                    assert_eq!(
                        kind.get("type").and_then(|v| v.as_str()),
                        Some("object"),
                        "{name}"
                    );
                    let properties = kind.get("properties").expect("properties");
                    assert!(
                        properties
                            .get("type")
                            .and_then(|t| t.get("const"))
                            .is_some(),
                        "{name}"
                    );
                    assert!(properties.get("coordinates").is_some(), "{name}");
                }
                continue;
            }
            Holds::Object(_) => standard_object_schema(entry),
            // Its built-in type's, with its bounds (chunk 5.5i3).
            Holds::Bounded { of, .. } => {
                let (_, rules) = jadpo_std::standard::declared(name).expect("its bounds");
                scalar_schema(of, &rules)
            }
            // One of its variants, each an object of `tag` and its fields
            // (chunk 5.5i3).
            Holds::Choice(variants) => {
                let schema = standard_object_schema(entry);
                let one_of = schema
                    .get("oneOf")
                    .and_then(|v| v.as_array())
                    .expect("one of its variants");
                assert_eq!(one_of.len(), variants.len(), "{name}");
                for (schema, (variant, fields)) in one_of.iter().zip(variants) {
                    let properties = schema.get("properties").expect("properties");
                    assert_eq!(
                        properties
                            .get("tag")
                            .and_then(|tag| tag.get("const"))
                            .and_then(|v| v.as_str()),
                        Some(*variant),
                        "{name}.{variant}"
                    );
                    for (field, _) in *fields {
                        assert!(properties.get(field).is_some(), "{name}.{variant}.{field}");
                    }
                }
                assert_eq!(entry.json, Json::Tagged, "{name}");
                continue;
            }
            Holds::Enum(codes) => {
                let spelling = Spelling::of(name);
                for (variant, code) in codes {
                    assert_eq!(spelling.spelling(variant), *code, "{name}.{variant}");
                }
                continue;
            }
            // A range's schema is its item type's ends; a feature's, its
            // type arguments' (area 20, `geo_interop`).
            Holds::Range | Holds::Feature | Holds::FeatureCollection => continue,
            // A named collection's: an array of its items, an object of its
            // entries (chunk 5.5o2).
            Holds::List(_) | Holds::Map(..) => standard_object_schema(entry),
        };
        let kind = schema
            .get("type")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        match entry.json {
            Json::Text => assert_eq!(kind.as_deref(), Some("string"), "{name}"),
            // Sent only to the server, never back (chunk 5.5r2).
            Json::Secret => {
                assert_eq!(kind.as_deref(), Some("string"), "{name}");
                assert!(
                    matches!(
                        schema.get("writeOnly"),
                        Some(jadpo_graph::json::Value::Bool(true))
                    ),
                    "{name}"
                );
            }
            Json::Tagged => unreachable!("{name}: only a choice is sent tagged"),
            Json::GeoJson => unreachable!("{name}: a geometry's schema is above"),
            Json::Feature => unreachable!("{name}: a feature's schema is its arguments'"),
            Json::Array => {
                assert_eq!(kind.as_deref(), Some("array"), "{name}");
                assert!(schema.get("items").is_some(), "{name}: its items");
            }
            Json::Keyed => {
                assert_eq!(kind.as_deref(), Some("object"), "{name}");
                assert!(schema.get("propertyNames").is_some(), "{name}: its keys");
                assert!(
                    schema.get("additionalProperties").is_some(),
                    "{name}: its values"
                );
            }
            Json::Number => assert_eq!(kind.as_deref(), Some("integer"), "{name}"),
            // An object of its members, each as its member says (chunk
            // 5.5w3).
            Json::Parts(members) => {
                assert_eq!(kind.as_deref(), Some("object"), "{name}");
                let properties = schema.get("properties").expect("properties");
                for (member, held) in members {
                    let one = properties.get(member).expect("a member");
                    let kind = one.get("type").and_then(|v| v.as_str());
                    match held {
                        jadpo_std::standard::Member::MinorUnits => {
                            assert_eq!(kind, Some("integer"), "{name}: `{member}`");
                        }
                        jadpo_std::standard::Member::Code(_)
                        | jadpo_std::standard::Member::Unit => {
                            assert_eq!(kind, Some("string"), "{name}: `{member}`");
                            assert!(one.get("enum").is_some(), "{name}: `{member}`'s codes");
                        }
                        _ => assert_eq!(kind, Some("string"), "{name}: `{member}`"),
                    }
                }
            }
            Json::Fields(fields) | Json::Members(fields) => {
                assert_eq!(kind.as_deref(), Some("object"), "{name}");
                let properties = schema.get("properties").expect("properties");
                for field in fields {
                    assert!(properties.get(field).is_some(), "{name}: `{field}`");
                }
                // A part that may be `none` is `null` too (chunk 5.5l).
                if let Holds::Object(parts) = entry.holds {
                    for (field, part) in parts {
                        let field = jadpo_std::standard::wire_name(name, field);
                        let schema = properties.get(field).expect("a property");
                        assert_eq!(
                            schema.get("anyOf").is_some(),
                            part.required().1,
                            "{name}: `{field}` may be `null` exactly when it may be `none`"
                        );
                        // A part that is another standard type is described
                        // as that type is (chunk 5.5w3): `OidcClaims`'
                        // `email` as an `Email`, with its format.
                        let (held, inner) = match part {
                            jadpo_std::Part::Optional(inner) => (
                                schema
                                    .get("anyOf")
                                    .and_then(|v| v.as_array())
                                    .and_then(|options| options.first()),
                                **inner,
                            ),
                            part => (Some(schema), *part),
                        };
                        if let jadpo_std::Part::Standard(other) = inner {
                            let other = jadpo_std::standard_type(other).expect("a standard type");
                            if other.codes().is_none() {
                                assert_eq!(
                                    held,
                                    Some(&standard_schema(other)),
                                    "{name}: `{field}` is described as a `{}`",
                                    other.name
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}
