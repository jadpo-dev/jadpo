//! An operation's output type written from the provider's answer (area
//! 11, "Provenance and confirmation" and "Recordings"; chunk K5): when
//! `jadpo package record` calls an operation with no `output:`, the
//! sandbox's answer, along `output_at:`, is read as the JSON Schema it
//! shows, and that schema as the importer reads an operation's answer
//! (area 11, "How the OpenAPI importer reads a document"). So the types
//! are named, mapped and left to check exactly as an imported answer's
//! are: an object `<Resource><Operation>Output`, each object inside it
//! `<Parent><Field>`, members `lower_snake_case` with the provider's
//! spelling as `wire_name:`, each type `public` in its own file under
//! `types/`, and the operation's `output:` written beside its `input:`.
//!
//! One answer can't say everything a description does, so what it only
//! suggests is written the safe way and left to check: a member sent as
//! `null` is optional `Text`, an empty list's items are `Text`, and text
//! written as an instant, a date or a UUID is read as that type. A list
//! whose items differ in kind is left out, as the importer leaves out a
//! schema of several shapes.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use jadpo_syntax::ast::{self, AstNode, Decl, MemberItem};
use jadpo_syntax::{SyntaxKind, SyntaxNode};
use serde_json::{Value, json};

use super::model::{Finding, Mapped, camel, snake, type_file};
use super::openapi;

/// The schema key holding what an answer only suggests, said as a
/// finding where the field is.
pub(crate) const GUESSED: &str = "x-jadpo-guessed";

/// What writing an operation's output type from its answer gives.
#[derive(Debug)]
pub(crate) struct Written {
    /// The operation's `output:`, as written.
    pub output: String,
    /// The types written, by name.
    pub types: Vec<String>,
    /// Each file written, its path in the package and its new text, the
    /// resource's file first.
    pub files: Vec<(String, String)>,
    /// What the answer leaves to check, in the order found.
    pub findings: Vec<Finding>,
    /// Each field mapped to a standard type.
    pub mapped: Vec<Mapped>,
}

/// Why no type could be written: a sentence for a diagnostic's reason.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Unwritten(pub String);

/// The output type of operation `resource`.`operation` of the package in
/// `folder`, written from `answer`, the value at its `output_at:`.
pub(crate) fn written(
    folder: &Path,
    resource: &str,
    operation: &str,
    answer: &Value,
) -> Result<Written, Unwritten> {
    let at = format!("{resource}.{operation}");
    if answer.is_null() {
        return Err(Unwritten(
            "It answered `null`, which says nothing of what the operation gives.".to_string(),
        ));
    }
    let Some((resource_file, resource_text)) = resource_source(folder, resource) else {
        return Err(Unwritten(format!(
            "No file of the package declares `resource {resource}` as Jadpo reads it."
        )));
    };
    let mut schema = schema_of(&shape_of(answer));
    if let Some(object) = schema.as_object_mut()
        && object.contains_key("properties")
    {
        object.insert(
            "description".to_string(),
            Value::from(format!(
                "What `{at}` answers, written from the provider's sandbox's answer."
            )),
        );
    }
    let taken = taken(folder);
    let stem = format!("{}{}", camel(resource), camel(operation));
    let answered = openapi::answered(&schema, &at, &stem, taken);
    let Some(output) = answered.output else {
        let why = answered
            .findings
            .iter()
            .find(|finding| finding.at == at)
            .map_or_else(
                || "Its answer has no shape Jadpo has a type for.".to_string(),
                |finding| format!("{}.", finding.next),
            );
        return Err(Unwritten(why));
    };
    let Some(resource_text) = with_output(&resource_text, resource, operation, &output) else {
        return Err(Unwritten(format!(
            "`{resource_file}` doesn't declare `{operation}` in `resource {resource}` as Jadpo reads it."
        )));
    };
    let mut files = vec![(resource_file, resource_text)];
    let mut types = Vec::new();
    for (name, decl) in &answered.types {
        files.push((
            format!("types/{}.jadpo", snake(name)),
            type_file(name, decl),
        ));
        types.push(name.clone());
    }
    Ok(Written {
        output,
        types,
        files,
        findings: answered.findings,
        mapped: answered.mapped,
    })
}

/// What one answer shows of a value's type.
#[derive(Clone, Debug, PartialEq)]
struct Shape {
    kind: Kind,
    /// Sent as `null`, here or in another item of its list.
    nullable: bool,
}

#[derive(Clone, Debug, PartialEq)]
enum Kind {
    /// Only ever `null`.
    Unknown,
    Bool,
    Int,
    Number,
    /// Text, and the format its value is written in, when every value
    /// seen is written in one.
    Text(Option<&'static str>),
    /// A list, and its items' shape; `None` while it has none.
    List(Option<Box<Shape>>),
    /// An object's members, each with whether every value seen has it.
    Object(BTreeMap<String, (Shape, bool)>),
    /// Values of different kinds, which no one type holds.
    Mixed,
}

fn shape_of(value: &Value) -> Shape {
    let kind = match value {
        Value::Null => {
            return Shape {
                kind: Kind::Unknown,
                nullable: true,
            };
        }
        Value::Bool(_) => Kind::Bool,
        Value::Number(number) if number.is_i64() || number.is_u64() => Kind::Int,
        Value::Number(_) => Kind::Number,
        Value::String(text) => Kind::Text(format_of(text)),
        Value::Array(items) => Kind::List(items.iter().map(shape_of).reduce(merged).map(Box::new)),
        Value::Object(members) => Kind::Object(
            members
                .iter()
                .map(|(name, value)| (name.clone(), (shape_of(value), true)))
                .collect(),
        ),
    };
    Shape {
        kind,
        nullable: false,
    }
}

/// The format text is written in, of those a description's own `format`
/// maps exactly: an instant, a calendar date or a UUID, as Jadpo reads
/// each.
fn format_of(text: &str) -> Option<&'static str> {
    if text.contains('T') && jadpo_std::instant_millis(text).is_some() {
        Some("date-time")
    } else if text.len() == 10 && jadpo_std::date_number(text).is_some() {
        Some("date")
    } else if text.len() == 36 && jadpo_std::uuid_version(text).is_some() {
        Some("uuid")
    } else {
        None
    }
}

/// Two values' shapes as one: what a list's items have in common.
fn merged(a: Shape, b: Shape) -> Shape {
    let nullable = a.nullable || b.nullable;
    let kind = match (a.kind, b.kind) {
        (Kind::Unknown, kind) | (kind, Kind::Unknown) => kind,
        (Kind::Int, Kind::Number) | (Kind::Number, Kind::Int) => Kind::Number,
        (Kind::Text(x), Kind::Text(y)) => Kind::Text(if x == y { x } else { None }),
        (Kind::List(x), Kind::List(y)) => Kind::List(match (x, y) {
            (Some(x), Some(y)) => Some(Box::new(merged(*x, *y))),
            (x, y) => x.or(y),
        }),
        // A member is required only where every value had it.
        (Kind::Object(mut x), Kind::Object(mut y)) => {
            let names: BTreeSet<String> = x.keys().chain(y.keys()).cloned().collect();
            let mut out = BTreeMap::new();
            for name in names {
                let member = match (x.remove(&name), y.remove(&name)) {
                    (Some((a, always_a)), Some((b, always_b))) => {
                        (merged(a, b), always_a && always_b)
                    }
                    (Some((one, _)), None) | (None, Some((one, _))) => (one, false),
                    (None, None) => continue,
                };
                out.insert(name, member);
            }
            Kind::Object(out)
        }
        (x, y) if x == y => x,
        _ => Kind::Mixed,
    };
    Shape { kind, nullable }
}

/// A shape as the JSON Schema the importer reads.
fn schema_of(shape: &Shape) -> Value {
    let typed = |kind: &str| {
        if shape.nullable {
            json!({ "type": [kind, "null"] })
        } else {
            json!({ "type": kind })
        }
    };
    match &shape.kind {
        Kind::Unknown => json!({
            "type": ["string", "null"],
            GUESSED: "Sent as `null`, so written as optional `Text`: check its type against the provider's documentation",
        }),
        Kind::Bool => typed("boolean"),
        Kind::Int => typed("integer"),
        Kind::Number => typed("number"),
        Kind::Text(format) => {
            let mut schema = typed("string");
            if let Some(format) = format {
                let ty = match *format {
                    "date-time" => "Instant",
                    "date" => "CalendarDate",
                    _ => "Uuid",
                };
                schema["format"] = Value::from(*format);
                schema[GUESSED] = Value::from(format!(
                    "Read as `{ty}` from how its value is written: check the provider's documentation, then keep it or make it `Text`"
                ));
            }
            schema
        }
        Kind::List(items) => {
            let mut schema = typed("array");
            schema["items"] = match items {
                Some(items) => schema_of(items),
                None => json!({
                    "type": "string",
                    GUESSED: "Sent as an empty list, so its items are written as `Text`: check them against the provider's documentation",
                }),
            };
            schema
        }
        Kind::Object(members) => {
            let mut schema = typed("object");
            let properties: serde_json::Map<String, Value> = members
                .iter()
                .map(|(name, (shape, _))| (name.clone(), schema_of(shape)))
                .collect();
            let required: Vec<Value> = members
                .iter()
                .filter(|(_, (shape, always))| *always && !shape.nullable)
                .map(|(name, _)| Value::from(name.as_str()))
                .collect();
            schema["properties"] = Value::Object(properties);
            schema["required"] = Value::Array(required);
            schema["description"] = Value::from(
                "Part of what the operation answers, written from its sandbox's answer.",
            );
            schema
        }
        Kind::Mixed => json!({
            "oneOf": [{ "type": "string" }, { "type": "integer" }],
        }),
    }
}

/// Every name the package's files give a type or failure, and every type
/// file's name read as one, so a type written never takes one.
fn taken(folder: &Path) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for (path, text) in sources(folder) {
        if let Some(stem) = path
            .strip_prefix("types/")
            .and_then(|file| file.strip_suffix(".jadpo"))
        {
            out.insert(camel(stem));
        }
        for (name, _) in super::merge::type_members(&text) {
            out.insert(name);
        }
    }
    out
}

/// Every `.jadpo` file of the package, its path from `folder` with `/`,
/// but its recipes' and its sample app's.
fn sources(folder: &Path) -> Vec<(String, String)> {
    fn walk(at: &Path, prefix: &str, out: &mut Vec<(String, String)>) {
        let Ok(entries) = std::fs::read_dir(at) else {
            return;
        };
        let mut entries: Vec<_> = entries.flatten().collect();
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for entry in entries {
            let Some(name) = entry.file_name().to_str().map(str::to_string) else {
                continue;
            };
            if name.starts_with('.') {
                continue;
            }
            let path = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}/{name}")
            };
            if entry.path().is_dir() {
                if prefix.is_empty() && ["recipes", "sample", "build"].contains(&name.as_str()) {
                    continue;
                }
                walk(&entry.path(), &path, out);
            } else if name.ends_with(".jadpo")
                && let Ok(text) = std::fs::read_to_string(entry.path())
            {
                out.push((path, text));
            }
        }
    }
    let mut out = Vec::new();
    walk(folder, "", &mut out);
    out
}

/// The file declaring `resource <name>`, and its text.
fn resource_source(folder: &Path, resource: &str) -> Option<(String, String)> {
    sources(folder)
        .into_iter()
        .find(|(_, text)| resource_decl(text, resource).is_some())
}

fn resource_decl(text: &str, resource: &str) -> Option<ast::ResourceDecl> {
    let parse = jadpo_syntax::parse_file(text);
    let file = ast::SourceFile::cast(parse.syntax())?;
    file.decls().find_map(|decl| match decl {
        Decl::ResourceDecl(decl)
            if decl
                .name()
                .and_then(|name| name.ident_token())
                .is_some_and(|token| token.text() == resource) =>
        {
            Some(decl)
        }
        _ => None,
    })
}

/// `text` with `output: <output>` written into operation `operation` of
/// `resource <resource>`: after its `input:`, else its `path:`, else last;
/// `None` when the file doesn't declare it.
fn with_output(text: &str, resource: &str, operation: &str, output: &str) -> Option<String> {
    let decl = resource_decl(text, resource)?;
    let key = |member: &ast::Member| {
        member
            .member_key()
            .map(|key| key.syntax().text().to_string().trim().to_string())
    };
    let found = decl
        .member_list()?
        .member_items()
        .find_map(|item| match item {
            MemberItem::Member(member) if key(&member).as_deref() == Some(operation) => {
                Some(member)
            }
            _ => None,
        })?;
    let block = found.block()?;
    let members: Vec<ast::Member> = block
        .member_items()
        .filter_map(|item| match item {
            MemberItem::Member(member) => Some(member),
            _ => None,
        })
        .collect();
    let after = ["input", "path"]
        .iter()
        .find_map(|wanted| members.iter().find(|m| key(m).as_deref() == Some(*wanted)));
    let line = |at: usize| text[..at].rfind('\n').map_or(0, |start| start + 1);
    let (at, indent) = match after {
        Some(member) => {
            let range = member.syntax().text_range();
            let start = usize::from(range.start());
            let indent = text[line(start)..start].to_string();
            (usize::from(range.end()), indent)
        }
        None => {
            let close = close_of(block.syntax())?;
            let start = usize::from(found.syntax().text_range().start());
            let indent = format!("{}    ", &text[line(start)..start]);
            let before = text[..close].trim_end_matches([' ', '\n']).len();
            (before, indent)
        }
    };
    let mut out = text.to_string();
    out.insert_str(at, &format!("\n{indent}output: {output}"));
    Some(jadpo_fmt::format_file(&out).unwrap_or(out))
}

/// Where a member list's closing `}` starts.
fn close_of(list: &SyntaxNode) -> Option<usize> {
    list.children_with_tokens()
        .filter_map(|element| element.into_token())
        .last()
        .filter(|token| token.kind() == SyntaxKind::R_BRACE)
        .map(|token| usize::from(token.text_range().start()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn package(name: &str, resource: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("jadpo-answer-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("resources")).expect("folder");
        std::fs::create_dir_all(dir.join("types")).expect("folder");
        std::fs::write(dir.join("resources/bookings.jadpo"), resource).expect("write");
        std::fs::write(
            dir.join("types/create_booking.jadpo"),
            "/// What booking takes.\npublic type CreateBooking = Object {\n    postcode: Text\n}\n",
        )
        .expect("write");
        dir
    }

    const RESOURCE: &str = "public resource bookings {\n    create: {\n        method: HttpMethod.post\n        path: \"/v1/bookings\"\n        input: CreateBooking\n        changes: true\n        source: PackageSource.written\n    }\n}\n";

    #[test]
    fn an_answer_is_read_as_the_importer_reads_an_answer() {
        let dir = package("read", RESOURCE);
        let answer = serde_json::json!({
            "id": "bk_481",
            "collectAt": "2026-10-21T09:00:00Z",
            "price": 12.5,
            "pieces": 2,
            "fragile": false,
            "note": null,
            "tags": [],
            "window": { "from": "09:00", "to": "12:00" },
            "stops": [{ "postcode": "N1 9GU", "lat": 51.5, "lng": -0.1 }, { "postcode": "E1 6AN" }]
        });
        let written = written(&dir, "bookings", "create", &answer).expect("written");
        assert_eq!(written.output, "BookingsCreateOutput");
        assert_eq!(
            written.types,
            [
                "BookingsCreateOutput",
                "BookingsCreateOutputStopsElement",
                "BookingsCreateOutputWindow"
            ]
        );
        let files: BTreeMap<&str, &str> = written
            .files
            .iter()
            .map(|(path, text)| (path.as_str(), text.as_str()))
            .collect();
        assert_eq!(
            files["resources/bookings.jadpo"],
            "public resource bookings {\n    create: {\n        method: HttpMethod.post\n        path: \"/v1/bookings\"\n        input: CreateBooking\n        output: BookingsCreateOutput\n        changes: true\n        source: PackageSource.written\n    }\n}\n"
        );
        assert_eq!(
            files["types/bookings_create_output.jadpo"],
            "/// What `bookings.create` answers, written from the provider's sandbox's answer.\npublic type BookingsCreateOutput = Object {\n    collect_at: Instant {\n        wire_name: \"collectAt\"\n    }\n    fragile: Bool\n    id: Text\n    note: Text?\n    pieces: Int\n    price: Decimal\n    stops: List<BookingsCreateOutputStopsElement>\n    tags: List<Text>\n    window: BookingsCreateOutputWindow\n}\n"
        );
        // Members some items lack are optional; a latitude beside a
        // longitude is read as a point, as an import reads one.
        let stops = files["types/bookings_create_output_stops_element.jadpo"];
        assert!(stops.contains("    location: Point? {\n"), "{stops}");
        assert!(stops.contains("    postcode: Text\n"), "{stops}");
        let at: Vec<(&str, &str)> = written
            .findings
            .iter()
            .map(|finding| (finding.at.as_str(), &finding.next[..12]))
            .collect();
        assert_eq!(
            at,
            [
                ("BookingsCreateOutput.collect_at", "Read as `Ins"),
                ("BookingsCreateOutput.note", "Sent as `nul"),
                ("BookingsCreateOutput.tags", "Sent as an e"),
            ]
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_name_the_package_has_is_never_given_again() {
        let dir = package("taken", RESOURCE);
        std::fs::write(
            dir.join("types/bookings_create_output.jadpo"),
            "public type BookingsCreateOutput = Object {\n    id: Text\n}\n",
        )
        .expect("write");
        let written = written(
            &dir,
            "bookings",
            "create",
            &serde_json::json!([{ "id": "a" }]),
        )
        .expect("written");
        assert_eq!(written.output, "List<BookingsCreateOutputElement>");
        let written = written_named(&dir, &serde_json::json!({ "id": "a" }));
        assert_eq!(written.output, "BookingsCreateOutput2");
        let _ = std::fs::remove_dir_all(dir);
    }

    fn written_named(dir: &Path, answer: &Value) -> Written {
        written(dir, "bookings", "create", answer).expect("written")
    }

    #[test]
    fn an_answer_with_no_type_writes_nothing() {
        let dir = package("none", RESOURCE);
        assert!(written(&dir, "bookings", "create", &Value::Null).is_err());
        let mixed =
            written(&dir, "bookings", "create", &serde_json::json!([1, "a"])).expect_err("no type");
        assert!(mixed.0.contains("`oneOf`"), "{mixed:?}");
        assert!(written(&dir, "parcels", "create", &serde_json::json!({ "a": 1 })).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn output_goes_after_path_when_there_is_no_input() {
        let text = "public resource bookings {\n    list: {\n        method: HttpMethod.get\n        path: \"/v1/bookings\"\n        source: PackageSource.written\n    }\n}\n";
        assert_eq!(
            with_output(text, "bookings", "list", "List<Booking>").as_deref(),
            Some(
                "public resource bookings {\n    list: {\n        method: HttpMethod.get\n        path: \"/v1/bookings\"\n        output: List<Booking>\n        source: PackageSource.written\n    }\n}\n"
            )
        );
        assert!(with_output(text, "bookings", "get", "Booking").is_none());
    }
}
