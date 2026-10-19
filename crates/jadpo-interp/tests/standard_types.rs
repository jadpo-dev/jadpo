//! The one harness every standard type runs through, at runtime (area 19,
//! "Every standard type meets one contract", question 2): for each data
//! entry in `jadpo_std::standard`, its valid examples read from their
//! canonical text and write it back, are shown to people as the entry
//! says, round-trip through JSON in the shape the entry names and through
//! their storage column, are equal to themselves and to nothing else, and
//! come in order when the type is ordered; its malformed examples fail
//! `malformed`. Each type's codecs are made by the plan's own builder
//! (`jadpo_plan::standard_codecs`, chunk 5.5w3), so the harness is held to
//! the codecs a program gets. `jadpo-check`'s `standard_types` tests are
//! the checker's half: what each type's calls accept. A new type is an
//! entry here, not a new test.

use jadpo_graph::TypeRef;
use jadpo_std::config::Spelling;
use jadpo_std::http::CodecId;
use jadpo_std::standard::{Holds, Json, STANDARD_TYPES, StandardType};
use jadpo_std::{Builtin, RuleKey};
use jadpo_value::json::{Codecs, Path};
use jadpo_value::shape::Shape;
use jadpo_value::{Fault, Validator, Value, ops, standard};

/// The codecs a route table holds for some types, as the plan makes them,
/// with their validators compiled and their shapes ready, and each type's
/// codec in the order asked.
struct Table {
    codecs: Vec<jadpo_std::http::Codec>,
    validators: Vec<Validator>,
    shapes: Vec<Shape>,
    ids: Vec<CodecId>,
}

impl Table {
    /// The plan's codecs for `types` (chunk 5.5w3).
    fn of(types: &[TypeRef]) -> Table {
        let made = jadpo_plan::standard_codecs(types);
        Table {
            codecs: made.codecs,
            validators: made
                .validators
                .into_iter()
                .map(|plan| Validator::new(plan.builtin, plan.rules).with_across(plan.across))
                .collect(),
            shapes: made.shapes.iter().map(Shape::new).collect(),
            ids: made.ids,
        }
    }

    fn view(&self) -> Codecs<'_> {
        Codecs {
            codecs: &self.codecs,
            validators: &self.validators,
            shapes: &self.shapes,
        }
    }
}

/// The type a program names the entry's type by: a built-in type by
/// itself, a range by the type its examples hold (`Range<CalendarDate>`),
/// any other by its name.
fn type_of(entry: &StandardType) -> TypeRef {
    match entry.holds {
        Holds::Builtin(builtin) => TypeRef::Builtin(builtin),
        Holds::Range => {
            let item = entry
                .examples
                .of
                .and_then(jadpo_std::standard_type)
                .and_then(|item| match item.holds {
                    Holds::Builtin(builtin) => Some(builtin),
                    _ => None,
                })
                .expect("a range's examples hold a built-in type");
            TypeRef::Std(jadpo_std::RANGE.to_string(), vec![TypeRef::Builtin(item)])
        }
        _ => TypeRef::Std(entry.name.to_string(), Vec::new()),
    }
}

/// The entry's type's codecs, its own first, and a map keyed by it, of
/// small whole numbers (a `PageSize`, a JSON number): what `json.write`
/// and `json.read` send a `Map<T, PageSize>` with.
fn codecs(entry: &StandardType) -> (Table, CodecId) {
    let ty = type_of(entry);
    let keyed = TypeRef::Map(
        Box::new(ty.clone()),
        Box::new(TypeRef::Builtin(Builtin::PageSize)),
    );
    let table = Table::of(&[ty, keyed]);
    assert_eq!(
        table.ids[0],
        CodecId(0),
        "{}: its own codec first",
        entry.name
    );
    let map = table.ids[1];
    (table, map)
}

/// The type name `text.parse` is given for the entry's examples: its own,
/// or with its examples' type argument (`Range<CalendarDate>`).
fn parsed_as(entry: &StandardType) -> String {
    match entry.examples.of {
        Some(item) => format!("{}<{item}>", entry.name),
        None => entry.name.to_string(),
    }
}

/// An example read from its canonical text, as `text.parse` reads it.
fn read(entry: &StandardType, text: &str) -> Result<Value, Fault> {
    match entry.holds {
        Holds::Range => standard::parse(text, &parsed_as(entry)),
        _ => standard::read(entry, text),
    }
}

/// An example of a type with no single text form (`PersonName`), read
/// from its JSON, as the wire reads it.
fn read_json(table: &Codecs<'_>, text: &str) -> Option<Value> {
    let json: serde_json::Value = serde_json::from_str(text).ok()?;
    let mut problems = Vec::new();
    table.read(CodecId(0), &json, &Path::default(), &mut problems)
}

/// Whether a setting may hold the entry's type (area 10, "What a setting
/// can hold"): a type read from one piece of text, built on `Text`, `Int`,
/// `Decimal`, `Bool`, `Uuid`, `Duration`, `Instant` or `CalendarDate`, or
/// an enum without data; an object, a choice, a collection, an amount of
/// money and the rest are `configuration.type_unsupported`, which the
/// checker's half holds.
fn a_setting_holds(entry: &StandardType) -> bool {
    let read = |builtin: Builtin| {
        matches!(
            builtin.representation(),
            Builtin::Text
                | Builtin::Int
                | Builtin::Decimal
                | Builtin::Bool
                | Builtin::Uuid
                | Builtin::Duration
                | Builtin::Instant
                | Builtin::CalendarDate
        )
    };
    match entry.holds {
        Holds::Builtin(builtin) | Holds::Bounded { of: builtin, .. } => read(builtin),
        Holds::Text { .. } | Holds::Whole { .. } | Holds::Enum(_) => true,
        _ => false,
    }
}

/// `text` as setting `value` of the entry's type, as the check before the
/// app starts reads an environment variable: a code in any case, a
/// duration as source writes it or as its canonical text.
fn setting(table: &Codecs<'_>, entry: &StandardType, text: &str) -> Result<Value, String> {
    let spelling = match entry.holds {
        Holds::Builtin(Builtin::Duration) => Spelling::Duration,
        _ => Spelling::of(entry.name),
    };
    let plan = jadpo_std::config::ConfigPlan {
        fields: vec![jadpo_std::config::ConfigField {
            name: "value".into(),
            variable: "VALUE".into(),
            secret: false,
            codec: CodecId(0),
            spelling,
            default: None,
            test: None,
            generated: None,
            rotates: false,
        }],
        codecs: table.codecs.to_vec(),
        connections: Vec::new(),
    };
    jadpo_runtime::config::decode(&plan, table.validators, &plan.fields[0], text)
}

fn london() -> Value {
    let ordinal = jadpo_std::variant_ordinal(jadpo_std::ZONE, "europe_london").unwrap();
    Value::variant(jadpo_std::ZONE, ordinal, "europe_london", Vec::new())
}

#[test]
fn every_standard_type_meets_the_contract() {
    for entry in STANDARD_TYPES {
        let name = entry.name;
        // A feature takes two type arguments, the second an application's
        // own `Object`; `geo_interop`'s tests run it (area 20).
        if entry.json == Json::Feature {
            continue;
        }
        let (tables, map) = codecs(entry);
        let table = tables.view();
        let keyed_table = tables.view();
        let mut values = Vec::new();
        for (text, shown) in entry.examples.valid {
            // A type with no single text form: its examples are its JSON.
            let value = if entry.text.is_none() {
                read_json(&table, text).unwrap_or_else(|| panic!("{name}: {text} reads"))
            } else {
                read(entry, text)
                    .unwrap_or_else(|fault| panic!("{name}: `{text}` reads ({fault:?})"))
            };
            if entry.text == Some(jadpo_std::standard::TextForm::Secret) {
                // A secret value (chunk 5.5r2): read from text and JSON,
                // compared, and never written, shown or stored.
                assert!(matches!(value, Value::Secret(_)), "{name}: held secret");
                assert_eq!(
                    standard::parse(text, &parsed_as(entry)),
                    Ok(value.clone()),
                    "{name}: `text.parse` reads it"
                );
                assert_eq!(standard::from(&value), Err(Fault::Shape), "{name}");
                assert_eq!(
                    standard::canonical(entry, &value),
                    Err(Fault::Shape),
                    "{name}"
                );
                assert_eq!(value.to_string(), *shown, "{name}: written redacted");
                assert!(
                    !format!("{value:?}").contains(text),
                    "{name}: never debugged"
                );
                assert_eq!(
                    standard::shown(entry, &value, "en-GB", &Value::None, &Value::None).as_deref(),
                    Ok(*shown),
                    "{name}: never shown"
                );
                let mut json = String::new();
                table.write(CodecId(0), &value, &mut json);
                assert_eq!(
                    json,
                    format!("\"{shown}\""),
                    "{name}: never written as JSON"
                );
                let mut problems = Vec::new();
                let sent = serde_json::Value::String((*text).to_string());
                assert_eq!(
                    table.read(CodecId(0), &sent, &Path::default(), &mut problems),
                    Some(value.clone()),
                    "{name}: read from JSON ({problems:?})"
                );
                assert_eq!(entry.storage, jadpo_std::standard::Storage::Never, "{name}");
                assert!(
                    jadpo_runtime::to_sql(&value, jadpo_std::SqlType::Text)
                        .is_err_and(|why| !why.contains(text)),
                    "{name}: never stored"
                );
                values.push(value);
                continue;
            }
            if entry.text.is_none() {
                assert_eq!(
                    standard::from(&value),
                    Err(Fault::Shape),
                    "{name}: has no canonical text"
                );
            } else {
                // Canonical text, both ways.
                assert_eq!(
                    standard::canonical(entry, &value).as_deref(),
                    Ok(*text),
                    "{name}: `{text}` is written back as it was read"
                );
                assert_eq!(
                    standard::from(&value),
                    Ok(Value::text(text)),
                    "{name}: `text.from` gives the canonical text"
                );
                assert_eq!(
                    standard::parse(text, &parsed_as(entry)),
                    Ok(value.clone()),
                    "{name}: `text.parse` reads it"
                );
            }
            // For people, in en_gb, with the options the type takes: a
            // range's are its item type's.
            let formatted = entry
                .examples
                .of
                .and_then(jadpo_std::standard_type)
                .unwrap_or(entry);
            let zone = if formatted.format_option("zone").is_some() {
                london()
            } else {
                Value::None
            };
            let format = match entry.examples.format {
                Some(format) => Value::text(format),
                None => Value::None,
            };
            assert_eq!(
                standard::shown(entry, &value, "en-GB", &zone, &format).as_deref(),
                Ok(*shown),
                "{name}: `{text}` is shown to people as its entry says"
            );
            // JSON, in the entry's shape, and back.
            let mut json = String::new();
            table.write(CodecId(0), &value, &mut json);
            let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
            match entry.json {
                Json::Text if entry.text.is_none() => panic!("{name}: JSON is its fields"),
                Json::Secret => unreachable!("{name}: a secret value is checked above"),
                Json::Text => assert_eq!(
                    parsed,
                    serde_json::Value::String((*text).to_string()),
                    "{name}: JSON is the canonical text"
                ),
                Json::Number => assert_eq!(
                    parsed,
                    serde_json::from_str::<serde_json::Value>(text).unwrap(),
                    "{name}: JSON is the number"
                ),
                Json::Fields(_) | Json::Parts(_) => {
                    // Written in the fields' order, or its members' (chunk
                    // 5.5w3).
                    let fields: Vec<&str> = match entry.json {
                        Json::Parts(members) => members.iter().map(|(name, _)| *name).collect(),
                        _ => entry.json.fields().unwrap_or_default().to_vec(),
                    };
                    let written: Vec<usize> = fields
                        .iter()
                        .filter_map(|field| json.find(&format!("\"{field}\":")))
                        .collect();
                    assert!(
                        written.len() == fields.len() && written.is_sorted(),
                        "{name}: {json} is an object of its fields in order"
                    );
                    assert_eq!(parsed.as_object().map(|o| o.len()), Some(fields.len()));
                }
                // Area 2's form of a variant with data: `tag` first, then
                // the variant's fields (chunk 5.5i3).
                Json::Tagged => {
                    let Holds::Choice(variants) = entry.holds else {
                        panic!("{name}: only a choice is sent tagged");
                    };
                    let tag = parsed.get("tag").and_then(|tag| tag.as_str());
                    let fields = variants
                        .iter()
                        .find(|(variant, _)| Some(*variant) == tag)
                        .map(|(_, fields)| *fields)
                        .unwrap_or_else(|| panic!("{name}: {json} names its variant"));
                    assert!(
                        json.starts_with("{\"tag\":"),
                        "{name}: {json} is tagged first"
                    );
                    assert_eq!(
                        parsed.as_object().map(|o| o.len()),
                        Some(fields.len() + 1),
                        "{name}: {json} is its tag and its variant's fields"
                    );
                }
                // The members it has, in order, and none of those that are
                // `none`: as its example writes it.
                Json::Members(_) => assert_eq!(
                    json, *text,
                    "{name}: {json} leaves out the members that are `none`"
                ),
                // JSON's own collections (chunk 5.5o2): an array of the
                // items, an object of the entries, each key the wire text
                // its example writes.
                Json::Array => assert!(parsed.is_array(), "{name}: {json} is an array"),
                Json::Keyed => assert_eq!(
                    parsed,
                    serde_json::from_str::<serde_json::Value>(text).unwrap(),
                    "{name}: {json} is an object keyed as its example is"
                ),
                // A GeoJSON geometry, written exactly as its example is:
                // its `type`, then its `coordinates`, each position
                // longitude first (chunk 5.5s).
                Json::GeoJson => {
                    assert_eq!(json, *text, "{name}: {json} is its GeoJSON");
                    assert!(json.starts_with("{\"type\":"), "{name}: {json}");
                }
                Json::Feature => unreachable!("{name}: a feature is skipped above"),
            }
            let mut problems = Vec::new();
            let back = table.read(CodecId(0), &parsed, &Path::default(), &mut problems);
            assert_eq!(
                back.as_ref(),
                Some(&value),
                "{name}: {json} reads back ({problems:?})"
            );
            if entry.text.is_some() {
                // A place that holds text, a path, a query string, a header,
                // a setting or a map's key, reads the canonical text as
                // `text.parse` does (chunk 5.5w2).
                let mut problems = Vec::new();
                assert_eq!(
                    table.read_text(CodecId(0), text, &Path::default(), &mut problems),
                    Some(value.clone()),
                    "{name}: `{text}` is read where text is held ({problems:?})"
                );
                assert_eq!(
                    table.read_query(
                        CodecId(0),
                        &[(*text).to_string()],
                        &Path::default(),
                        &mut problems
                    ),
                    Some(value.clone()),
                    "{name}: `{text}` is read from a query string ({problems:?})"
                );
                // A map's key is its canonical text, written by `json.write`
                // and read back by `json.read`.
                let entries = Value::Map(std::rc::Rc::new(
                    [(value.clone(), Value::Int(1))].into_iter().collect(),
                ));
                let written = keyed_table.write_value(map, &entries);
                let mut expected = String::from("{");
                jadpo_value::json::write_string(text, &mut expected);
                expected.push_str(":1}");
                assert_eq!(written, expected, "{name}: a key is its canonical text");
                assert_eq!(
                    keyed_table.read_value(map, &Value::text(&written)),
                    Ok(entries),
                    "{name}: {written} reads back keyed by `{text}`"
                );
                if a_setting_holds(entry) {
                    assert_eq!(
                        setting(&table, entry, text),
                        Ok(value.clone()),
                        "{name}: `{text}` is read as a setting"
                    );
                }
            }
            // Storage, in its own column; a type stored as its JSON is
            // stored as the round trip above writes it; one stored in more
            // than one column (`Money`, area 19) as its parts, read back
            // together.
            if !entry.columns().is_empty() {
                let parts = standard::to_parts(&value)
                    .unwrap_or_else(|| panic!("{name}: `{text}` has its parts"));
                assert_eq!(parts.len(), entry.columns().len(), "{name}");
                let row: Vec<_> = parts
                    .iter()
                    .zip(entry.columns())
                    .map(|(part, (_, column))| {
                        // A range's column is its item's range type (chunk
                        // 5.5w).
                        let column = match (entry.name, entry.examples.of) {
                            (jadpo_std::RANGE, Some("CalendarDate")) => "daterange",
                            (jadpo_std::RANGE, Some("Instant")) => "tstzrange",
                            (jadpo_std::RANGE, Some("Decimal")) => "numrange",
                            (jadpo_std::RANGE, _) => "int8range",
                            _ => column,
                        };
                        let sql = jadpo_std::SqlType::of_column(column).expect("a column type");
                        jadpo_runtime::to_sql(part, sql)
                            .unwrap_or_else(|why| panic!("{name}: a part is stored ({why})"))
                    })
                    .collect();
                let mut columns = vec![jadpo_std::read::Column::Compound {
                    ty: parsed_as(entry),
                    may_be_none: false,
                }];
                columns.resize(row.len(), jadpo_std::read::Column::Part);
                assert_eq!(
                    jadpo_runtime::field_value(&row, &columns, 0, jadpo_runtime::JsonCodecs::NONE),
                    Ok(value.clone()),
                    "{name}: `{text}` reads back from its columns"
                );
            } else if let (Some(column), jadpo_std::standard::Storage::Column(storage)) =
                (entry.column(), entry.storage)
            {
                let sql = jadpo_runtime::to_sql(&value, storage)
                    .unwrap_or_else(|why| panic!("{name}: `{text}` is stored ({why})"));
                assert_eq!(
                    jadpo_runtime::from_sql(&sql, &column),
                    Ok(value.clone()),
                    "{name}: `{text}` reads back from its column"
                );
            } else if entry.storage == jadpo_std::standard::Storage::Geography {
                // A PostGIS `geography` column (area 20, chunk 5.5u4),
                // which holds a geometry whose positions all have an
                // altitude or none do, as every geometry is made (chunk
                // 5.5u7).
                assert_eq!(entry.column_type(), "geography", "{name}");
                assert!(
                    jadpo_value::geo::shape(&value)
                        .is_some_and(|shape| shape.has_altitude().is_some()),
                    "{name}: `{text}` mixes altitudes"
                );
                let sql = jadpo_runtime::to_sql(&value, jadpo_std::SqlType::Geography)
                    .unwrap_or_else(|why| panic!("{name}: `{text}` is stored ({why})"));
                assert_eq!(
                    jadpo_runtime::from_sql(
                        &sql,
                        &jadpo_std::read::Column::Geography(name.to_string())
                    ),
                    Ok(value.clone()),
                    "{name}: `{text}` reads back from its column"
                );
            } else if entry.storage == jadpo_std::standard::Storage::Vector {
                // A pgvector `vector` column, which the AI area builds
                // (chunk 5.5s2).
                assert_eq!(entry.column_type(), "vector", "{name}");
            } else {
                assert_eq!(entry.column_type(), "jsonb", "{name}");
            }
            values.push(value);
        }
        // `==`, and order where the type has one.
        for (at, value) in values.iter().enumerate() {
            assert!(ops::equal(value, value), "{name}: a value equals itself");
            for other in &values[at + 1..] {
                assert!(!ops::equal(value, other), "{name}: different values differ");
                if entry.ordered {
                    assert_eq!(
                        ops::compare(value, other),
                        Some(std::cmp::Ordering::Less),
                        "{name}: its examples come in order"
                    );
                }
            }
        }
        for text in entry.examples.malformed {
            // A type with no single text form: JSON that isn't one.
            if entry.text.is_none() {
                assert_eq!(read_json(&table, text), None, "{name}: {text} isn't one");
                continue;
            }
            // Text that isn't one at all, or one that breaks a rule across
            // its parts, which it is checked by once read (chunk 5.5t2).
            match read(entry, text) {
                Err(fault) => assert_eq!(
                    fault,
                    Fault::Broke(RuleKey::Malformed),
                    "{name}: `{text}` is malformed"
                ),
                Ok(value) => assert!(
                    jadpo_value::across::broken_standard(&value).is_some(),
                    "{name}: `{text}` is refused"
                ),
            }
            // Every place that holds text refuses it as `text.parse` does
            // (chunk 5.5w2): a path, a query string, a header, a map's key
            // and a setting, and JSON where the type is sent as its text.
            let mut problems = Vec::new();
            assert_eq!(
                table.read_text(CodecId(0), text, &Path::default(), &mut problems),
                None,
                "{name}: `{text}` isn't read where text is held"
            );
            if entry.json == Json::Text {
                let json = serde_json::Value::String((*text).to_string());
                assert_eq!(
                    table.read(CodecId(0), &json, &Path::default(), &mut problems),
                    None,
                    "{name}: `{text}` isn't read from JSON"
                );
            }
            if a_setting_holds(entry) {
                assert!(
                    setting(&table, entry, text).is_err(),
                    "{name}: `{text}` isn't read as a setting"
                );
            }
            // A type held as text refuses it wherever it is built: its
            // own check breaks `format`, and JSON doesn't read it.
            if let Holds::Text { .. } = entry.holds {
                let rules = jadpo_std::standard::text_rules(name).expect("its rules");
                assert_eq!(
                    rules.check_text(text).map_err(|broken| broken.key),
                    Err(RuleKey::Format),
                    "{name}: building one from `{text}` breaks `format`"
                );
                let mut problems = Vec::new();
                let json = serde_json::Value::String((*text).to_string());
                assert_eq!(
                    table.read(CodecId(0), &json, &Path::default(), &mut problems),
                    None,
                    "{name}: `{text}` isn't read from JSON"
                );
            }
        }
    }
}

#[test]
fn representations_read_and_write_as_json_carries_them() {
    assert_eq!(standard::parse("-42", "Int"), Ok(Value::Int(-42)));
    assert_eq!(
        standard::parse("4 2", "Int"),
        Err(Fault::Broke(RuleKey::Malformed))
    );
    assert_eq!(
        standard::parse("+1", "Int"),
        Err(Fault::Broke(RuleKey::Malformed))
    );
    let price = standard::parse("12.50", "Decimal").unwrap();
    assert_eq!(standard::from(&price), Ok(Value::text("12.50")));
    assert_eq!(
        standard::parse("1e3", "Decimal"),
        Err(Fault::Broke(RuleKey::Malformed))
    );
    assert_eq!(standard::parse("true", "Bool"), Ok(Value::Bool(true)));
    assert_eq!(
        standard::parse("yes", "Bool"),
        Err(Fault::Broke(RuleKey::Malformed))
    );
    assert_eq!(
        standard::parse(" any text ", "Text"),
        Ok(Value::text(" any text "))
    );
    assert_eq!(standard::from(&Value::Int(7)), Ok(Value::text("7")));
}

/// Every place a value is read from text reads it as `text.parse` does,
/// no looser (chunk 5.5w2): a JSON body, a path, a query string, a header,
/// a setting and a map's key refuse an `Int` with `+`, a `Decimal` written
/// `1_000`, `.5` or `--5`, and an `Instant` with an offset, also as a
/// `ZonedTime`'s instant.
#[test]
fn every_reader_is_as_strict_as_text_parse() {
    for (builtin, refused, read) in [
        (Builtin::Int, &["+5", "1_000", "--5", " 5", "5 "][..], "-5"),
        (
            Builtin::Decimal,
            &["1_000", ".5", "5.", "--5", "+1.5", "1e3"][..],
            "-1.50",
        ),
        (
            Builtin::Instant,
            &["2026-10-25T10:30:00+01:00", "2026-10-25T09:30:00-00:00"][..],
            "2026-10-25T09:30:00Z",
        ),
        (Builtin::Duration, &["P+1D", "PT+5M", "5m"][..], "PT5M"),
    ] {
        let tables = Table::of(&[TypeRef::Builtin(builtin)]);
        let table = tables.view();
        let name = builtin.name();
        let parsed = standard::parse(read, name).expect("reads");
        let mut problems = Vec::new();
        assert_eq!(
            table.read_text(CodecId(0), read, &Path::default(), &mut problems),
            Some(parsed),
            "{name}: `{read}`"
        );
        for text in refused {
            assert!(standard::parse(text, name).is_err(), "{name}: `{text}`");
            let json = serde_json::Value::String((*text).to_string());
            assert_eq!(
                table.read(CodecId(0), &json, &Path::default(), &mut problems),
                None,
                "{name}: `{text}` in JSON"
            );
            assert_eq!(
                table.read_text(CodecId(0), text, &Path::default(), &mut problems),
                None,
                "{name}: `{text}` where text is held"
            );
        }
    }
    // A zoned time's instant, as JSON carries it.
    let tables = Table::of(&[TypeRef::Builtin(Builtin::ZonedTime)]);
    let table = tables.view();
    let mut problems = Vec::new();
    let offset: serde_json::Value = serde_json::from_str(
        r#"{"instant": "2026-10-25T10:30:00+01:00", "zone": "Europe/London"}"#,
    )
    .unwrap();
    assert_eq!(
        table.read(CodecId(0), &offset, &Path::default(), &mut problems),
        None
    );
    assert_eq!(problems[0].field, "instant");
    assert_eq!(problems[0].rule, "format");
}

#[test]
fn enum_codes_and_zones_read_ignoring_case() {
    let entry = jadpo_std::standard_type("Zone").unwrap();
    assert_eq!(
        standard::read(entry, "europe/london"),
        standard::read(entry, "Europe/London")
    );
    let entry = jadpo_std::standard_type("Locale").unwrap();
    assert!(standard::read(entry, "EN-gb").is_ok());
    // A zoned time's offset is the one its zone has then.
    let entry = jadpo_std::standard_type("ZonedTime").unwrap();
    assert!(standard::read(entry, "2026-07-01T09:30:00+01:00[Europe/London]").is_ok());
    assert_eq!(
        standard::read(entry, "2026-07-01T09:30:00+00:00[Europe/London]"),
        Err(Fault::Broke(RuleKey::Malformed))
    );
    // As is a zoned time's zone in JSON (chunk 5.5w3).
    let (tables, _) = codecs(entry);
    let mut problems = Vec::new();
    let sent: serde_json::Value =
        serde_json::from_str(r#"{"instant": "2026-07-01T08:30:00Z", "zone": "europe/london"}"#)
            .unwrap();
    assert_eq!(
        tables
            .view()
            .read(CodecId(0), &sent, &Path::default(), &mut problems),
        standard::read(entry, "2026-07-01T09:30:00+01:00[Europe/London]").ok(),
        "{problems:?}"
    );
}

/// A `Country` value, by its ISO 3166-1 code.
fn country(code: &str) -> Value {
    let entry = jadpo_std::standard_type("Country").unwrap();
    let codes = entry.codes().unwrap();
    let at = codes.iter().position(|(_, c)| *c == code).unwrap();
    Value::variant("Country", at as u32, codes[at].0, Vec::new())
}

#[test]
fn phone_numbers_follow_each_country_s_rules() {
    let entry = jadpo_std::standard_type("PhoneNumber").unwrap();
    for examples in jadpo_std::standard::phones::PHONE_EXAMPLES {
        let home = country(examples.country);
        for (written, e164, at_home, abroad) in examples.valid {
            // A reader in a region with another calling code sees the
            // international format.
            let abroad_tag = if e164.starts_with("+44") {
                "en-US"
            } else {
                "en-GB"
            };
            let read = standard::parse_in(written, entry.name, &Value::None, &home);
            assert_eq!(
                read,
                Ok(Value::text(e164)),
                "{written} in {}",
                examples.country
            );
            assert_eq!(standard::parse(e164, entry.name), Ok(Value::text(e164)));
            let value = Value::text(e164);
            let shown = |tag: &str| standard::shown(entry, &value, tag, &Value::None, &Value::None);
            assert_eq!(shown(examples.locale).as_deref(), Ok(*at_home), "{e164}");
            assert_eq!(shown(abroad_tag).as_deref(), Ok(*abroad), "{e164}");
        }
        for written in examples.invalid {
            assert_eq!(
                standard::parse_in(written, entry.name, &Value::None, &home),
                Err(Fault::Broke(RuleKey::Malformed)),
                "{written} isn't a number of {}",
                examples.country
            );
        }
    }
    // Only E.164 is read without a country.
    assert_eq!(
        standard::parse("07400 123456", entry.name),
        Err(Fault::Broke(RuleKey::Malformed))
    );
}

#[test]
fn postal_codes_follow_each_country_s_rules() {
    let address = jadpo_std::standard_type("PostalAddress").unwrap();
    let (tables, _) = codecs(address);
    let table = tables.view();
    let posted = |code: &str, country: &str| {
        let json = serde_json::json!({
            "lines": ["1 High Street"],
            "locality": "Town",
            "region": null,
            "postal_code": code,
            "country": country,
        });
        let mut problems = Vec::new();
        let read = table.read(CodecId(0), &json, &Path::default(), &mut problems);
        (read, problems)
    };
    for examples in jadpo_std::standard::postal::POSTAL_EXAMPLES {
        let home = country(examples.country);
        for (typed, canonical) in examples.valid {
            assert_eq!(
                standard::parse_in(typed, "PostalCode", &Value::None, &home),
                Ok(Value::text(canonical)),
                "{typed} in {}",
                examples.country
            );
            let (read, problems) = posted(canonical, examples.country);
            assert!(read.is_some(), "{canonical}: {problems:?}");
        }
        for typed in examples.invalid {
            assert_eq!(
                standard::parse_in(typed, "PostalCode", &Value::None, &home),
                Err(Fault::Broke(RuleKey::Malformed)),
                "{typed} isn't a postal code of {}",
                examples.country
            );
            let (read, problems) = posted(typed, examples.country);
            assert_eq!(read, None, "{typed} in an address in {}", examples.country);
            // Refused at `postal_code` by its own check, or by the rule
            // across the address's parts (chunk 5.5t2).
            assert!(
                problems
                    .iter()
                    .any(|problem| problem.field.ends_with("postal_code")
                        || problem.rule == "postal_code_of_country"),
                "{problems:?}"
            );
        }
    }
}

#[test]
fn identity_documents_follow_each_country_s_forms() {
    use jadpo_std::standard::documents::{DOCUMENT_EXAMPLES, VAT_NUMBER, vat_prefix};
    for examples in DOCUMENT_EXAMPLES {
        let entry = jadpo_std::standard_type(examples.ty).unwrap();
        let (tables, _) = codecs(entry);
        let table = tables.view();
        let read = |number: &str| -> (Option<Value>, Vec<String>) {
            // A VAT number is sent as VIES writes it.
            if examples.ty == VAT_NUMBER {
                let text = format!("{}{number}", vat_prefix(examples.country));
                let read = standard::read(entry, &text);
                assert!(
                    matches!(read, Ok(_) | Err(Fault::Broke(RuleKey::Malformed))),
                    "{text}: {read:?}"
                );
                // A number not in its country's form breaks the type's
                // rule, which the value is checked by once read (chunk
                // 5.5t2).
                let kept = read
                    .ok()
                    .filter(|value| jadpo_value::across::broken_standard(value).is_none());
                return (kept, vec!["number".to_string()]);
            }
            let mut json = serde_json::json!({
                "country": examples.country,
                "number": number,
            });
            if let Some(kind) = examples.kind {
                json["kind"] = serde_json::Value::String(kind.to_string());
                json["expires"] = serde_json::Value::Null;
            }
            let mut problems = Vec::new();
            let read = table.read(CodecId(0), &json, &Path::default(), &mut problems);
            let fields = problems
                .iter()
                .map(|p| format!("{} {}", p.field, p.rule))
                .collect();
            (read, fields)
        };
        for number in examples.valid {
            let (value, problems) = read(number);
            assert!(
                value.is_some(),
                "{} {number} in {}: {problems:?}",
                examples.ty,
                examples.country
            );
        }
        for number in examples.invalid {
            let (value, problems) = read(number);
            assert_eq!(
                value, None,
                "{} {number} isn't one of {}'s",
                examples.ty, examples.country
            );
            // Refused at `number` by its own pattern, or by the rule
            // across its parts, `number_of_country` (chunk 5.5t2).
            assert!(
                problems.iter().any(|problem| problem.contains("number")),
                "{} {number}: {problems:?}",
                examples.ty
            );
        }
    }
}

#[test]
fn a_masked_card_keeps_the_brand_and_last_four() {
    let masked = |number: &str| {
        jadpo_value::run(
            jadpo_std::Op::MoneyMask,
            &[Value::text(number)],
            None,
            &mut |_| None,
        )
        .unwrap_or_else(|fault| panic!("{number}: {fault:?}"))
    };
    let entry = jadpo_std::standard_type("MaskedCardNumber").unwrap();
    let shown = |value: &Value| standard::shown(entry, value, "en-GB", &Value::None, &Value::None);
    assert_eq!(
        shown(&masked("4242424242424242")).as_deref(),
        Ok("Visa •••• 4242")
    );
    assert_eq!(
        shown(&masked("378282246310005")).as_deref(),
        Ok("American Express •••• 0005")
    );
    assert_eq!(
        shown(&masked("2223003122003222")).as_deref(),
        Ok("Mastercard •••• 3222")
    );
    // A range no listed scheme issues has no brand.
    assert_eq!(
        shown(&masked("9999999999999995")).as_deref(),
        Ok("•••• 9995")
    );
}

#[test]
fn an_event_s_parts_are_checked_together_as_json_is_read() {
    let event = jadpo_std::standard_type("CalendarEvent").unwrap();
    let (tables, _) = codecs(event);
    let table = tables.view();
    let posted = |changes: serde_json::Value| {
        let mut json = serde_json::json!({
            "summary": "Night shift",
            "first_day": null,
            "last_day": null,
            "start": "2026-10-24T22:00:00",
            "end": "2026-10-25T02:00:00",
            "zone": "Europe/London",
            "location": null,
            "description": null,
            "recurrence": "DTSTART;TZID=Europe/London:20261024T220000\nRRULE:FREQ=WEEKLY;COUNT=4",
        });
        for (name, value) in changes.as_object().unwrap() {
            json[name] = value.clone();
        }
        let mut problems = Vec::new();
        let read = table.read(CodecId(0), &json, &Path::default(), &mut problems);
        (read, problems)
    };
    let (read, problems) = posted(serde_json::json!({}));
    assert!(read.is_some(), "{problems:?}");
    // Each rule broken is reported by its name at the event's path
    // (chunk 5.5t2).
    let broken = |changes: serde_json::Value, rule: &str| {
        let (read, problems) = posted(changes.clone());
        assert_eq!(read, None, "{changes}");
        assert!(
            problems
                .iter()
                .any(|problem| problem.field.is_empty() && problem.rule == rule),
            "{changes}: {problems:?}"
        );
    };
    broken(
        serde_json::json!({ "first_day": "2026-10-24" }),
        "days_or_times",
    );
    broken(
        serde_json::json!({ "first_day": "2026-10-24", "last_day": "2026-10-26" }),
        "days_or_times",
    );
    broken(serde_json::json!({ "zone": null }), "days_or_times");
    broken(
        serde_json::json!({ "end": "2026-10-24T21:00:00" }),
        "end_after_start",
    );
    broken(
        serde_json::json!({ "recurrence": "DTSTART;TZID=Europe/Paris:20261024T220000\nRRULE:FREQ=WEEKLY" }),
        "recurrence_from_start",
    );
    broken(
        serde_json::json!({
            "first_day": "2026-10-24",
            "last_day": "2026-10-23",
            "start": null,
            "end": null,
            "zone": null,
            "recurrence": null,
        }),
        "last_day_not_before_first",
    );
}

/// A range whose start is its end holds nothing, and PostgreSQL keeps every
/// such range as `empty`, without its ends: storing one is refused rather
/// than losing where it was (chunk 5.5w), while one that holds its end, a
/// `DateRange` of one day, is stored.
#[test]
fn a_range_that_holds_nothing_is_not_stored() {
    let empty = standard::parse("2026-10-25T09:00:00Z/2026-10-25T09:00:00Z", "InstantRange")
        .expect("an empty range");
    let parts = standard::to_parts(&empty).expect("its column");
    let sql = jadpo_std::SqlType::of_column("tstzrange").expect("a range type");
    let refused = jadpo_runtime::to_sql(&parts[0], sql).expect_err("refused");
    assert!(
        refused.contains("PostgreSQL keeps without its ends"),
        "{refused}"
    );
    let day = standard::parse("2026-10-25/2026-10-25", "DateRange").expect("one day");
    let parts = standard::to_parts(&day).expect("its column");
    let sql = jadpo_std::SqlType::of_column("daterange").expect("a range type");
    assert!(jadpo_runtime::to_sql(&parts[0], sql).is_ok());
}
