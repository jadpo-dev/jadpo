//! A golden file per standard type (area 19, "Changing a standard type's
//! encoding", decided 2026-10-19; chunk 5.9): each of the entry's valid
//! examples in every form a value of the type outlives a release in, its
//! canonical text, its contract JSON, and the value of each storage column
//! as SQL text, beside the columns themselves. The forms are made by the
//! codecs a program gets (`jadpo_plan::standard_codecs`, the runtime's
//! storage codec and `jadpo_sql::literal`), so a change to any of them
//! fails here until the file is rewritten on purpose.
//!
//! A release changes a form only with a reader for the old one and a
//! release note: `JADPO_BLESS=1` rewrites a file whose forms changed only
//! once `jadpo_std::standard::ENCODING` is past the encoding the file
//! records, and area 19's "Encodings each release writes" names every type
//! a later encoding changed. A new type's file, or a new example in an old
//! one, is written without that.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use jadpo_graph::TypeRef;
use jadpo_std::http::CodecId;
use jadpo_std::standard::{ENCODING, Holds, Json, STANDARD_TYPES, StandardType, Storage};
use jadpo_std::{Builtin, SqlType};
use jadpo_value::json::{Codecs, Path as JsonPath};
use jadpo_value::{Value, standard};

use crate::support;

/// Where the golden files are: one per standard type, by its name.
fn folder() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/goldens/standard")
}

/// The plan's codecs for one type, with their validators compiled and
/// their shapes ready, as the per-type harness makes them.
struct Table {
    codecs: Vec<jadpo_std::http::Codec>,
    validators: Vec<jadpo_value::Validator>,
    shapes: Vec<jadpo_value::shape::Shape>,
}

impl Table {
    fn of(ty: TypeRef) -> Table {
        let made = jadpo_plan::standard_codecs(&[ty]);
        assert_eq!(made.ids, [CodecId(0)], "its own codec first");
        Table {
            codecs: made.codecs,
            validators: made
                .validators
                .into_iter()
                .map(|plan| {
                    jadpo_value::Validator::new(plan.builtin, plan.rules).with_across(plan.across)
                })
                .collect(),
            shapes: made
                .shapes
                .iter()
                .map(jadpo_value::shape::Shape::new)
                .collect(),
        }
    }

    fn view(&self) -> Codecs<'_> {
        Codecs {
            codecs: &self.codecs,
            validators: &self.validators,
            shapes: &self.shapes,
            coder: None,
        }
    }
}

/// The type a program names the entry's type by, as the harness names it.
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

/// One example's forms, each a line of its block.
struct Forms {
    text: String,
    json: String,
    /// Each column's suffix (`""` for the field's own column) and its value
    /// as SQL text.
    stored: Vec<(String, String)>,
}

/// How a type is read and written: by the plan's codecs for its own type,
/// or, for a feature, whose properties are an application's own object
/// (area 20), by a program declaring one.
enum Codec {
    Table(Table),
    Program(Box<jadpo_interp::Program>),
}

/// A feature's examples' properties: `{"name": "Site A"}`.
const FEATURES: &str = "/// A site's name.
type SiteName = Text {}

/// What a map shows beside a site.
type SiteProperties = Object {
    name: SiteName
}

/// A feature's or a layer's JSON.
type Sent = Text {}

/// JSON that isn't the feature or the layer.
type Unreadable = Failure {
    kind: FailureKind.invalid_value
}

/// A feature's JSON, read.
function read_feature(raw: Sent) fails Unreadable -> Feature<Point, SiteProperties> {
    return attempt json.read(raw, as: Feature<Point, SiteProperties>) {
        malformed: Unreadable
        mismatch: Unreadable
    }
}

/// A feature, written.
function write_feature(value: Feature<Point, SiteProperties>) -> Sent {
    return Sent(json.write(value))
}

/// A layer's JSON, read.
function read_feature_collection(raw: Sent) fails Unreadable -> FeatureCollection<Point, SiteProperties> {
    return attempt json.read(raw, as: FeatureCollection<Point, SiteProperties>) {
        malformed: Unreadable
        mismatch: Unreadable
    }
}

/// A layer, written.
function write_feature_collection(value: FeatureCollection<Point, SiteProperties>) -> Sent {
    return Sent(json.write(value))
}
";

/// `FeatureCollection` as `feature_collection`.
fn snake(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_uppercase() && !out.is_empty() {
            out.push('_');
        }
        out.push(c.to_ascii_lowercase());
    }
    out
}

impl Codec {
    fn of(entry: &StandardType) -> Codec {
        if entry.json == Json::Feature {
            Codec::Program(Box::new(support::main(FEATURES)))
        } else {
            Codec::Table(Table::of(type_of(entry)))
        }
    }

    /// The value an example's JSON holds.
    fn read_json(&self, entry: &StandardType, text: &str) -> Value {
        match self {
            Codec::Table(table) => {
                let json: serde_json::Value = serde_json::from_str(text)
                    .unwrap_or_else(|_| panic!("{}: {text} is JSON", entry.name));
                let mut problems = Vec::new();
                table
                    .view()
                    .read(CodecId(0), &json, &JsonPath::default(), &mut problems)
                    .unwrap_or_else(|| panic!("{}: {text} reads ({problems:?})", entry.name))
            }
            Codec::Program(program) => support::value(
                program,
                &format!("read_{}", snake(entry.name)),
                vec![Value::text(text)],
            ),
        }
    }

    /// The contract JSON of `value`.
    fn write_json(&self, entry: &StandardType, value: &Value) -> String {
        match self {
            Codec::Table(table) => {
                let mut json = String::new();
                table.view().write(CodecId(0), value, &mut json);
                json
            }
            Codec::Program(program) => {
                match support::value(
                    program,
                    &format!("write_{}", snake(entry.name)),
                    vec![value.clone()],
                ) {
                    Value::Text(text) => text.to_string(),
                    other => panic!("{}: written as {other:?}", entry.name),
                }
            }
        }
    }
}

/// The columns a value of the type is stored in: each one's suffix, its
/// PostgreSQL type and its comment, if any. A range's is its item's range
/// type (chunk 5.5w).
fn columns(entry: &StandardType) -> Vec<(String, String, Option<String>)> {
    let range = match entry.examples.of {
        Some("CalendarDate") => "daterange",
        Some("Instant") => "tstzrange",
        Some("Decimal") => "numrange",
        _ => "int8range",
    };
    if !entry.columns().is_empty() {
        return entry
            .columns()
            .iter()
            .enumerate()
            .map(|(at, (suffix, ty))| {
                let ty = if entry.name == jadpo_std::RANGE {
                    range
                } else {
                    ty
                };
                (suffix.to_string(), ty.to_string(), entry.part_comment(at))
            })
            .collect();
    }
    let comment = match entry.holds {
        Holds::Builtin(Builtin::Quantity(kind))
        | Holds::Bounded {
            of: Builtin::Quantity(kind),
            ..
        } => Some(jadpo_std::standard::storage::unit_comment(kind)),
        _ => None,
    };
    match entry.storage {
        Storage::Never => Vec::new(),
        _ => vec![(String::new(), entry.column_type().to_string(), comment)],
    }
}

/// `value` as each column holds it, written as SQL text: through the
/// runtime's storage codec, or as its contract JSON where it is stored as
/// that (chunk 5.5w2b).
fn stored(entry: &StandardType, value: &Value, json: &str) -> Vec<(String, String)> {
    let name = entry.name;
    let literal = |sql: &jadpo_std::SqlValue| {
        jadpo_sql::literal::sql(sql).unwrap_or_else(|| panic!("{name}: {sql:?} as SQL text"))
    };
    let laid = columns(entry);
    if !entry.columns().is_empty() {
        let parts = standard::to_parts(value).unwrap_or_else(|| panic!("{name}: its parts"));
        assert_eq!(parts.len(), laid.len(), "{name}");
        return parts
            .iter()
            .zip(laid)
            .map(|(part, (suffix, column, _))| {
                let sql = SqlType::of_column(&column).expect("a column type");
                let bound = jadpo_runtime::to_sql(part, sql)
                    .unwrap_or_else(|why| panic!("{name}: a part is stored ({why})"));
                (suffix, literal(&bound))
            })
            .collect();
    }
    let bound = match entry.storage {
        Storage::Never => return Vec::new(),
        Storage::Column(SqlType::Json) => jadpo_std::SqlValue::Json(json.to_string()),
        Storage::Column(sql) => {
            jadpo_runtime::to_sql(value, sql).unwrap_or_else(|why| panic!("{name}: stored ({why})"))
        }
        Storage::Geography => jadpo_runtime::to_sql(value, SqlType::Geography)
            .unwrap_or_else(|why| panic!("{name}: stored ({why})")),
        Storage::Vector => jadpo_runtime::to_sql(value, SqlType::Vector)
            .unwrap_or_else(|why| panic!("{name}: stored ({why})")),
        Storage::Columns { .. } => unreachable!("{name}: laid out above"),
    };
    vec![(String::new(), literal(&bound))]
}

/// Each of the entry's valid examples in every form, by its example.
fn forms(entry: &StandardType) -> Vec<(String, Forms)> {
    let codec = Codec::of(entry);
    entry
        .examples
        .valid
        .iter()
        .map(|(text, _)| {
            let value = if entry.text.is_none() {
                codec.read_json(entry, text)
            } else if entry.holds == Holds::Range {
                let of = entry.examples.of.unwrap_or_default();
                standard::parse(text, &format!("{}<{of}>", entry.name))
                    .unwrap_or_else(|fault| panic!("{}: `{text}` reads ({fault:?})", entry.name))
            } else {
                standard::read(entry, text)
                    .unwrap_or_else(|fault| panic!("{}: `{text}` reads ({fault:?})", entry.name))
            };
            let canonical = match standard::canonical(entry, &value) {
                Ok(text) if entry.text.is_some() => text,
                _ => "none".to_string(),
            };
            let json = codec.write_json(entry, &value);
            let stored = stored(entry, &value, &json);
            (
                (*text).to_string(),
                Forms {
                    text: canonical,
                    json,
                    stored,
                },
            )
        })
        .collect()
}

/// `text` on one line: each control character escaped.
fn line(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{{{:x}}}", u32::from(c))),
            c => out.push(c),
        }
    }
    out
}

/// A golden file, read: the encoding it records, the lines naming its
/// columns, and each example's block of lines.
struct Golden {
    encoding: u32,
    header: Vec<String>,
    examples: BTreeMap<String, Vec<String>>,
}

impl Golden {
    /// The file `entry`'s forms make, recording `encoding`.
    fn make(entry: &StandardType, encoding: u32) -> Golden {
        let header = columns(entry)
            .into_iter()
            .map(|(suffix, ty, comment)| {
                let named = if suffix.is_empty() {
                    String::new()
                } else {
                    format!("{suffix} ")
                };
                match comment {
                    Some(comment) => format!("column {named}{ty} -- {comment}"),
                    None => format!("column {named}{ty}"),
                }
            })
            .collect();
        let examples = forms(entry)
            .into_iter()
            .map(|(example, forms)| {
                let mut lines = vec![
                    format!("text {}", line(&forms.text)),
                    format!("json {}", line(&forms.json)),
                ];
                if forms.stored.is_empty() {
                    lines.push("stored never".to_string());
                }
                for (suffix, value) in forms.stored {
                    lines.push(if suffix.is_empty() {
                        format!("stored {}", line(&value))
                    } else {
                        format!("stored {suffix} {}", line(&value))
                    });
                }
                (example, lines)
            })
            .collect();
        Golden {
            encoding,
            header,
            examples,
        }
    }

    fn write(&self, entry: &StandardType) -> String {
        let mut out = format!(
            "{name}: every form its examples take (area 19, \"Changing a standard type's encoding\")\nencoding {encoding}\n",
            name = entry.name,
            encoding = self.encoding
        );
        for line in &self.header {
            out.push_str(line);
            out.push('\n');
        }
        // In the entry's order, so the file reads as the entry does.
        for (example, _) in entry.examples.valid {
            let Some(lines) = self.examples.get(*example) else {
                continue;
            };
            out.push_str(&format!("\nexample {}\n", line(example)));
            for form in lines {
                out.push_str(form);
                out.push('\n');
            }
        }
        out
    }

    fn read(text: &str) -> Option<Golden> {
        let mut blocks = text.split("\n\n");
        let mut head = blocks.next()?.lines();
        head.next()?;
        let encoding = head.next()?.strip_prefix("encoding ")?.parse().ok()?;
        let header = head.map(str::to_string).collect();
        let mut examples = BTreeMap::new();
        for block in blocks {
            let mut lines = block.lines();
            let example = lines.next()?.strip_prefix("example ")?.to_string();
            examples.insert(example, lines.map(str::to_string).collect());
        }
        Some(Golden {
            encoding,
            header,
            examples,
        })
    }

    /// Whether `now` writes any form this file holds differently: its
    /// columns, or an example both hold. An example only one holds, a
    /// new one or one the entry dropped, changes no form.
    fn changed_in(&self, now: &Golden) -> Option<String> {
        if self.header != now.header {
            return Some(format!(
                "its columns, {:?} where they were {:?}",
                now.header, self.header
            ));
        }
        self.examples.iter().find_map(|(example, was)| {
            let is = now.examples.get(example)?;
            (is != was).then(|| format!("example `{example}`, {is:?} where it was {was:?}"))
        })
    }
}

/// Every standard type's forms are its golden file's, and a file changes
/// only by `JADPO_BLESS=1` and only past the encoding it records when a
/// form it holds changed.
#[test]
fn every_standard_type_s_forms_match_its_golden_file() {
    let folder = folder();
    let bless = std::env::var_os("JADPO_BLESS").is_some();
    let mut problems = Vec::new();
    for entry in STANDARD_TYPES {
        let path = folder.join(format!("{}.golden", entry.name));
        let saved = std::fs::read_to_string(&path).ok();
        let kept = saved.as_deref().and_then(Golden::read);
        let made = Golden::make(entry, ENCODING);
        let (encoding, changed) = match &kept {
            None => (ENCODING, None),
            Some(kept) => match kept.changed_in(&made) {
                Some(change) => (ENCODING, Some((kept.encoding, change))),
                None => (kept.encoding, None),
            },
        };
        let text = Golden { encoding, ..made }.write(entry);
        if bless {
            if let Some((was, change)) = &changed
                && ENCODING <= *was
            {
                problems.push(format!(
                    "{}: {change}. A form changes only in a new encoding: raise `jadpo_std::standard::ENCODING` past {was}, read the old form where it is kept, and name the type in area 19's \"Encodings each release writes\"",
                    entry.name
                ));
                continue;
            }
            std::fs::create_dir_all(&folder).expect("the goldens folder");
            if saved.as_deref() != Some(text.as_str()) {
                std::fs::write(&path, &text).expect("bless");
            }
            continue;
        }
        match (&saved, &changed) {
            (None, _) => problems.push(format!(
                "{}: no golden file; JADPO_BLESS=1 cargo test -p jadpo-interp --test integration standard_goldens:: writes it",
                entry.name
            )),
            (Some(_), Some((_, change))) => problems.push(format!(
                "{}: {change}. A release changes a form only with a reader for the old one and a release note (area 19, \"Changing a standard type's encoding\")",
                entry.name
            )),
            (Some(saved), None) if *saved != text => problems.push(format!(
                "{}: its golden file is out of date (an example added or dropped); JADPO_BLESS=1 cargo test -p jadpo-interp --test integration standard_goldens:: rewrites it",
                entry.name
            )),
            _ => {}
        }
    }
    // A file for a type the release no longer has.
    let names: Vec<String> = STANDARD_TYPES
        .iter()
        .map(|entry| format!("{}.golden", entry.name))
        .collect();
    for file in std::fs::read_dir(&folder).into_iter().flatten().flatten() {
        let name = file.file_name().to_string_lossy().to_string();
        if !names.contains(&name) {
            problems.push(format!(
                "{name}: no standard type has this golden file; a release keeps reading every type it ever wrote"
            ));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// Area 19's "Encodings each release writes", the release notes' list of
/// encodings, ends at the one this release writes, and names each type
/// whose golden file a later encoding changed.
#[test]
fn the_release_notes_name_every_encoding_and_the_types_it_changed() {
    let page = include_str!("../../../docs/language/19-standard-library.md");
    let start = page
        .find("| Encoding | Release | What it changed |")
        .expect("the encodings' table");
    let rows: Vec<(u32, String)> = page[start..]
        .lines()
        .skip(2)
        .take_while(|line| line.starts_with('|'))
        .map(|line| {
            let cells: Vec<&str> = line.trim_matches('|').split('|').map(str::trim).collect();
            (cells[0].parse().expect("an encoding"), cells[2].to_string())
        })
        .collect();
    let listed: Vec<u32> = rows.iter().map(|(encoding, _)| *encoding).collect();
    assert_eq!(
        listed,
        (1..=ENCODING).collect::<Vec<_>>(),
        "one row per encoding, the last this release's"
    );
    for entry in STANDARD_TYPES {
        let path = folder().join(format!("{}.golden", entry.name));
        let Some(golden) = std::fs::read_to_string(&path)
            .ok()
            .as_deref()
            .and_then(Golden::read)
        else {
            continue;
        };
        assert!(golden.encoding <= ENCODING, "{}", entry.name);
        if golden.encoding > 1 {
            let (_, changed) = &rows[golden.encoding as usize - 1];
            assert!(
                changed.contains(&format!("`{}`", entry.name)),
                "encoding {}'s row names `{}`",
                golden.encoding,
                entry.name
            );
        }
    }
}

/// A golden file reads back as it was written, and a changed form is told
/// apart from an added example.
#[test]
fn a_changed_form_is_told_apart_from_a_new_example() {
    let entry = jadpo_std::standard_type("CalendarDate").expect("an entry");
    let made = Golden::make(entry, 1);
    let text = made.write(entry);
    let read = Golden::read(&text).expect("it reads back");
    assert_eq!(read.encoding, 1);
    assert_eq!(read.write(entry), text);
    assert_eq!(read.changed_in(&made), None);
    let mut fewer = Golden::read(&text).expect("it reads back");
    fewer.examples.pop_first();
    assert_eq!(fewer.changed_in(&made), None, "an example added");
    let mut altered = Golden::read(&text).expect("it reads back");
    if let Some(lines) = altered.examples.values_mut().next() {
        lines[1] = "json \"28/02/2026\"".to_string();
    }
    assert!(altered.changed_in(&made).is_some(), "a form changed");
}
