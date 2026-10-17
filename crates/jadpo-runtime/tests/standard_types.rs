//! The one harness every standard type runs through, at runtime (area 19,
//! "Every standard type meets one contract", question 2): for each data
//! entry in `jadpo_std::standard`, its valid examples read from their
//! canonical text and write it back, are shown to people as the entry
//! says, round-trip through JSON in the shape the entry names and through
//! their storage column, are equal to themselves and to nothing else, and
//! come in order when the type is ordered; its malformed examples fail
//! `malformed`. `jadpo-check`'s `standard_types` tests are the checker's
//! half: what each type's calls accept. A new type is an entry here, not a
//! new test.

use jadpo_std::config::Spelling;
use jadpo_std::http::{Codec, CodecId, VariantCodec};
use jadpo_std::standard::{Holds, Json, STANDARD_TYPES, StandardType};
use jadpo_std::types::{FieldPlan, ShapeId, ShapePlan, ValidatorId};
use jadpo_std::{Builtin, RuleKey};
use jadpo_value::json::{Codecs, Path};
use jadpo_value::shape::Shape;
use jadpo_value::{Fault, Validator, Value, ops, standard};

/// The codecs a route table holds for the entry's type, made from the
/// entry as the plan makes them: a scalar for a built-in type, an enum
/// whose wire text is the variant's spelling, an object of its fields.
fn codecs(entry: &StandardType) -> (Vec<Codec>, Vec<Validator>, Vec<Shape>) {
    let scalar = |builtin: Builtin, validator: u32| Codec::Scalar {
        builtin,
        validator: ValidatorId(validator),
        int_as_number: false,
    };
    match entry.holds {
        Holds::Builtin(builtin) => (
            vec![scalar(builtin, 0)],
            vec![Validator::new(Some(builtin), Vec::new())],
            Vec::new(),
        ),
        Holds::Enum(codes) => {
            let spelling = Spelling::of(entry.name);
            let variants = codes
                .iter()
                .enumerate()
                .map(|(ordinal, (name, _))| VariantCodec {
                    name: (*name).to_string(),
                    wire: spelling.spelling(name),
                    ordinal: ordinal as u32,
                    fields: None,
                })
                .collect();
            (
                vec![Codec::Enum {
                    ty: entry.name.to_string(),
                    variants,
                }],
                Vec::new(),
                Vec::new(),
            )
        }
        // Sent as its canonical text (chunk 5.5k).
        Holds::Object(_) if entry.json == Json::Text => (
            vec![Codec::Standard {
                ty: entry.name.to_string(),
            }],
            Vec::new(),
            Vec::new(),
        ),
        Holds::Object(fields) => {
            let mut codecs = vec![Codec::Object {
                shape: ShapeId(0),
                fields: (1..=fields.len() as u32).map(CodecId).collect(),
            }];
            let mut validators = Vec::new();
            let mut plans = Vec::new();
            for (at, (name, part)) in fields.iter().enumerate() {
                let builtin = part.builtin().expect("an object sent as fields holds built-in types");
                codecs.push(scalar(builtin, at as u32));
                validators.push(Validator::new(Some(builtin), Vec::new()));
                plans.push(FieldPlan {
                    key: format!("field {}.{name}", entry.name),
                    name: (*name).to_string(),
                    validator: ValidatorId(at as u32),
                    omittable: false,
                    absent: None,
                    none_left_out: false,
                    section: None,
                });
            }
            let shape = Shape::new(&ShapePlan {
                ty: entry.name.to_string(),
                variant: None,
                fields: plans,
            });
            (codecs, validators, vec![shape])
        }
        // A range of its examples' item type: an object of two ends, each
        // the item or `null` for an open end.
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
            let codecs = vec![
                Codec::Object {
                    shape: ShapeId(0),
                    fields: vec![CodecId(1), CodecId(1)],
                },
                Codec::Nullable(CodecId(2)),
                scalar(item, 0),
            ];
            let plans = jadpo_std::RANGE_FIELDS
                .iter()
                .map(|name| FieldPlan {
                    key: format!("field {}.{name}", entry.name),
                    name: (*name).to_string(),
                    validator: ValidatorId(0),
                    omittable: false,
                    absent: None,
                    none_left_out: false,
                    section: None,
                })
                .collect();
            let shape = Shape::new(&ShapePlan {
                ty: entry.name.to_string(),
                variant: None,
                fields: plans,
            });
            (
                codecs,
                vec![Validator::new(Some(item), Vec::new())],
                vec![shape],
            )
        }
    }
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

fn london() -> Value {
    let ordinal = jadpo_std::variant_ordinal(jadpo_std::ZONE, "europe_london").unwrap();
    Value::variant(jadpo_std::ZONE, ordinal, "europe_london", Vec::new())
}

#[test]
fn every_standard_type_meets_the_contract() {
    for entry in STANDARD_TYPES {
        let name = entry.name;
        let (codecs, validators, shapes) = codecs(entry);
        let table = Codecs {
            codecs: &codecs,
            validators: &validators,
            shapes: &shapes,
        };
        let mut values = Vec::new();
        for (text, shown) in entry.examples.valid {
            // Canonical text, both ways.
            let value = read(entry, text)
                .unwrap_or_else(|fault| panic!("{name}: `{text}` reads ({fault:?})"));
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
                Json::Text => assert_eq!(
                    parsed,
                    serde_json::Value::String((*text).to_string()),
                    "{name}: JSON is the canonical text"
                ),
                Json::Fields(fields) => {
                    // Written in the fields' order.
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
            }
            let mut problems = Vec::new();
            let back = table.read(CodecId(0), &parsed, &Path::default(), &mut problems);
            assert_eq!(
                back.as_ref(),
                Some(&value),
                "{name}: {json} reads back ({problems:?})"
            );
            // Storage, in its own column; a type stored as its JSON is
            // stored as the round trip above writes it.
            if let Some(column) = entry.column() {
                let sql = jadpo_runtime::to_sql(&value, entry.storage)
                    .unwrap_or_else(|why| panic!("{name}: `{text}` is stored ({why})"));
                assert_eq!(
                    jadpo_runtime::from_sql(&sql, &column),
                    Ok(value.clone()),
                    "{name}: `{text}` reads back from its column"
                );
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
            assert_eq!(
                read(entry, text),
                Err(Fault::Broke(RuleKey::Malformed)),
                "{name}: `{text}` is malformed"
            );
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
}
