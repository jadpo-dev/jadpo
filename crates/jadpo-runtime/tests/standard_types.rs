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

/// The codec of a standard enum: its wire text is the variant's spelling.
fn enum_codec(entry: &StandardType, codes: &[(&str, &str)]) -> Codec {
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
    Codec::Enum {
        ty: entry.name.to_string(),
        variants,
    }
}

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
        // Text with its own check (area 19, chunk 5.5i), as the plan's
        // validator chain holds it.
        Holds::Text { .. } => (
            vec![scalar(Builtin::Text, 0)],
            vec![Validator::new(
                Some(Builtin::Text),
                jadpo_std::standard::text_rules(entry.name)
                    .into_iter()
                    .collect(),
            )],
            Vec::new(),
        ),
        Holds::Enum(codes) => (vec![enum_codec(entry, codes)], Vec::new(), Vec::new()),
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
                fields: Vec::new(),
            }];
            let mut validators = Vec::new();
            let mut plans = Vec::new();
            let mut tops = Vec::new();
            for (name, part) in fields {
                let validator = ValidatorId(validators.len() as u32);
                tops.push(part_codec(*part, &mut codecs, &mut validators));
                plans.push(FieldPlan {
                    key: format!("field {}.{name}", entry.name),
                    name: (*name).to_string(),
                    validator,
                    omittable: false,
                    absent: None,
                    none_left_out: false,
                    section: None,
                });
            }
            codecs[0] = Codec::Object {
                shape: ShapeId(0),
                fields: tops,
            };
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

/// The codec of one part of a standard object, pushed with the codecs and
/// validators it needs, as the plan makes them: a built-in type with the
/// rules its part keeps (a line of text's pattern), a standard enum by its
/// codes, a type held as text with its own check, the part or `null`, or a
/// list of it.
fn part_codec(
    part: jadpo_std::Part,
    codecs: &mut Vec<Codec>,
    validators: &mut Vec<Validator>,
) -> CodecId {
    let codec = match part {
        jadpo_std::Part::Optional(inner) => {
            let inner = part_codec(*inner, codecs, validators);
            Codec::Nullable(inner)
        }
        jadpo_std::Part::List(inner) => {
            let item = part_codec(*inner, codecs, validators);
            let validator = ValidatorId(validators.len() as u32);
            validators.push(Validator::new(None, Vec::new()));
            Codec::List { item, validator }
        }
        jadpo_std::Part::Standard(other) => {
            let other = jadpo_std::standard_type(other).expect("a standard type");
            match (other.codes(), other.holds) {
                (Some(codes), _) => enum_codec(other, codes),
                (None, Holds::Text { .. }) => {
                    let validator = ValidatorId(validators.len() as u32);
                    validators.push(Validator::new(
                        Some(Builtin::Text),
                        jadpo_std::standard::text_rules(other.name)
                            .into_iter()
                            .collect(),
                    ));
                    Codec::Scalar {
                        builtin: Builtin::Text,
                        validator,
                        int_as_number: false,
                    }
                }
                // Another standard object sent as its canonical text: an
                // event's local times (chunk 5.5k3).
                (None, Holds::Object(_)) if other.json == Json::Text => Codec::Standard {
                    ty: other.name.to_string(),
                },
                _ => panic!("{}: a part sent as fields", other.name),
            }
        }
        part => {
            let builtin = part.builtin().expect("a built-in part");
            let validator = ValidatorId(validators.len() as u32);
            validators.push(Validator::new(Some(builtin), vec![part.rules()]));
            Codec::Scalar {
                builtin,
                validator,
                int_as_number: false,
            }
        }
    };
    codecs.push(codec);
    CodecId(codecs.len() as u32 - 1)
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
            // A type with no single text form: its examples are its JSON.
            let value = if entry.text.is_none() {
                read_json(&table, text).unwrap_or_else(|| panic!("{name}: {text} reads"))
            } else {
                read(entry, text)
                    .unwrap_or_else(|fault| panic!("{name}: `{text}` reads ({fault:?})"))
            };
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
                        let sql = jadpo_std::SqlType::of_column(column).expect("a column type");
                        jadpo_runtime::to_sql(part, sql)
                            .unwrap_or_else(|why| panic!("{name}: a part is stored ({why})"))
                    })
                    .collect();
                let mut columns = vec![jadpo_std::read::Column::Compound(name.to_string())];
                columns.resize(row.len(), jadpo_std::read::Column::Part);
                assert_eq!(
                    jadpo_runtime::field_value(&row, &columns, 0),
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
            assert_eq!(
                read(entry, text),
                Err(Fault::Broke(RuleKey::Malformed)),
                "{name}: `{text}` is malformed"
            );
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
    let (codecs, validators, shapes) = codecs(address);
    let table = Codecs {
        codecs: &codecs,
        validators: &validators,
        shapes: &shapes,
    };
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
            assert!(
                problems
                    .iter()
                    .any(|problem| problem.field.ends_with("postal_code")),
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
        let (codecs, validators, shapes) = codecs(entry);
        let table = Codecs {
            codecs: &codecs,
            validators: &validators,
            shapes: &shapes,
        };
        let read = |number: &str| -> (Option<Value>, Vec<String>) {
            // A VAT number is sent as VIES writes it.
            if examples.ty == VAT_NUMBER {
                let text = format!("{}{number}", vat_prefix(examples.country));
                let read = standard::read(entry, &text);
                assert!(
                    matches!(read, Ok(_) | Err(Fault::Broke(RuleKey::Malformed))),
                    "{text}: {read:?}"
                );
                return (read.ok(), vec!["number".to_string()]);
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
            let fields = problems.iter().map(|p| p.field.clone()).collect();
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
            assert!(
                problems.iter().any(|field| field.ends_with("number")),
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
    let (codecs, validators, shapes) = codecs(event);
    let table = Codecs {
        codecs: &codecs,
        validators: &validators,
        shapes: &shapes,
    };
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
    let broken = |changes: serde_json::Value, part: &str| {
        let (read, problems) = posted(changes.clone());
        assert_eq!(read, None, "{changes}");
        assert!(
            problems
                .iter()
                .any(|problem| problem.field.ends_with(part) && problem.rule == "format"),
            "{changes}: {problems:?}"
        );
    };
    broken(serde_json::json!({ "first_day": "2026-10-24" }), "last_day");
    broken(
        serde_json::json!({ "first_day": "2026-10-24", "last_day": "2026-10-26" }),
        "start",
    );
    broken(serde_json::json!({ "zone": null }), "zone");
    broken(serde_json::json!({ "end": "2026-10-24T21:00:00" }), "end");
    broken(
        serde_json::json!({ "recurrence": "DTSTART;TZID=Europe/Paris:20261024T220000\nRRULE:FREQ=WEEKLY" }),
        "recurrence",
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
        "last_day",
    );
}
