//! Each standard type's page and their index (docs/writing.md, "How a
//! standard type's page reads"): what `jadpo help <Type>` prints and the docs
//! bundle publishes, so the two can't differ. The words a person wrote are
//! `jadpo_std::standard::pages`'; the shape, JSON, storage and operations are
//! the compiler's data (`type_reference`) and the type's golden file.

use jadpo_std::standard::pages::{self, TypePage};
use jadpo_term::{Block, Json, Row, Screen};

use crate::run::Reply;

/// Each golden file, `crates/jadpo-interp/tests/goldens/standard/`, packed by
/// `build.rs`.
const GOLDENS: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/goldens.deflate"));

/// An enum with more values than this gives them in Details, not under
/// `## Shape`; one with more than [`LISTED`] names only [`IN_SHAPE`].
const IN_SHAPE: usize = 40;
const LISTED: usize = 500;
/// How many of a type's JSON examples its page shows.
const JSON_SHOWN: usize = 3;
/// The topic that says how every standard type behaves.
const ALL_TYPES_TOPIC: &str = "every-standard-type-meets-one-contract";

/// A standard type's page ID: `type-` and its name in kebab case,
/// `type-uk-sort-code`, `type-h3-cell`, `type-o-auth-scope`.
pub fn type_id(name: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    let mut id = String::from("type-");
    for (i, c) in chars.iter().enumerate() {
        let after_word = i > 0 && (chars[i - 1].is_ascii_lowercase() || chars[i - 1].is_ascii_digit());
        let starts_word = i > 0
            && chars[i - 1].is_ascii_uppercase()
            && chars.get(i + 1).is_some_and(char::is_ascii_lowercase);
        if c.is_ascii_uppercase() && (after_word || starts_word) {
            id.push('-');
        }
        id.push(c.to_ascii_lowercase());
    }
    id
}

/// A member of a JSON object.
fn get<'a>(value: &'a Json, key: &str) -> Option<&'a Json> {
    match value {
        Json::Object(entries) => entries
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value),
        _ => None,
    }
}

/// The text of a member that is one, else nothing.
fn text<'a>(value: &'a Json, key: &str) -> Option<&'a str> {
    match get(value, key) {
        Some(Json::Str(text)) => Some(text),
        _ => None,
    }
}

fn items<'a>(value: &'a Json, key: &str) -> &'a [Json] {
    match get(value, key) {
        Some(Json::Array(items)) => items,
        _ => &[],
    }
}

fn flag(value: &Json, key: &str) -> bool {
    matches!(get(value, key), Some(Json::Bool(true)))
}

/// A number or a text member as it reads.
fn bound(value: &Json, key: &str) -> Option<String> {
    match get(value, key) {
        Some(Json::Str(text)) => Some(text.clone()),
        Some(Json::Null) | None => None,
        Some(other) => Some(other.to_line()),
    }
}

/// `text` as code: in a table's cell, a line break or a tab written as a JSON
/// string so the cell stays one line.
fn cell(written: &str) -> String {
    let flat = if written.chars().any(char::is_control) {
        Json::from(written).to_line()
    } else {
        written.to_string()
    };
    code(&flat).replace('|', "\\|")
}

fn code(written: &str) -> String {
    if written.is_empty() {
        "`\"\"`".to_string()
    } else if written.contains('`') {
        format!("`` {written} ``")
    } else {
        format!("`{written}`")
    }
}

fn joined(items: &[String], last: &str) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., final_one] => format!("{} {last} {final_one}", rest.join(", ")),
    }
}

/// A person's words with each standard type they name, in backticks, linked
/// to its page, but for the page's own type.
fn linked_prose(written: &str, own: &str, names: &[&str]) -> String {
    let mut out = String::new();
    let mut rest = written;
    while let Some(open) = rest.find('`') {
        let Some(len) = rest[open + 1..].find('`') else {
            break;
        };
        let inside = &rest[open + 1..open + 1 + len];
        out.push_str(&rest[..open]);
        let already_linked = out.ends_with('[');
        if inside != own && names.contains(&inside) && !already_linked {
            out.push_str(&format!("[`{inside}`]({}.md)", type_id(inside)));
        } else {
            out.push_str(&format!("`{inside}`"));
        }
        rest = &rest[open + 1 + len + 1..];
    }
    out.push_str(rest);
    out
}

/// A type as written, `List<Money>?`, with each standard type in it linking
/// its page.
fn type_link(written: &str, names: &[&str]) -> String {
    let mut out = String::new();
    let mut word = String::new();
    let mut other = String::new();
    let flush_other = |other: &mut String, out: &mut String| {
        if !other.is_empty() {
            out.push_str(&format!("`{other}`"));
            other.clear();
        }
    };
    for c in written.chars().chain(std::iter::once(' ')) {
        if c.is_ascii_alphanumeric() || c == '_' || c == '.' {
            word.push(c);
            continue;
        }
        if !word.is_empty() {
            if names.contains(&word.as_str()) {
                flush_other(&mut other, &mut out);
                out.push_str(&format!("[`{word}`]({}.md)", type_id(&word)));
            } else {
                other.push_str(&word);
            }
            word.clear();
        }
        other.push(c);
    }
    other.pop();
    flush_other(&mut other, &mut out);
    out
}

/// What a golden file says of one example.
struct GoldenExample {
    text: String,
    json: String,
    stored: Vec<(String, String)>,
}

struct Golden {
    /// Each column: its suffix (`""` for a type in one column), type and
    /// what it holds, if it says.
    columns: Vec<(String, String, Option<String>)>,
    examples: Vec<GoldenExample>,
}

fn unpack(name: &str) -> Option<String> {
    let packed = miniz_oxide::inflate::decompress_to_vec(GOLDENS).ok()?;
    let mut rest = packed.as_slice();
    let take = |rest: &mut &[u8]| -> Option<Vec<u8>> {
        let (len, after) = rest.split_first_chunk::<4>()?;
        let len = u32::from_le_bytes(*len) as usize;
        let bytes = after.get(..len)?.to_vec();
        *rest = &after[len..];
        Some(bytes)
    };
    while !rest.is_empty() {
        let id = take(&mut rest)?;
        let page = take(&mut rest)?;
        if id == name.as_bytes() {
            return String::from_utf8(page).ok();
        }
    }
    None
}

fn read_golden(name: &str) -> Golden {
    let source = unpack(name).unwrap_or_default();
    let mut columns: Vec<(String, Option<String>)> = Vec::new();
    let mut raw: Vec<(String, String, Vec<String>)> = Vec::new();
    for line in source.lines().skip(1) {
        if let Some(rest) = line.strip_prefix("column ") {
            match rest.split_once(" -- ") {
                Some((column, comment)) => {
                    columns.push((column.to_string(), Some(comment.to_string())))
                }
                None => columns.push((rest.to_string(), None)),
            }
        } else if line.starts_with("example ") {
            raw.push((String::new(), String::new(), Vec::new()));
        } else if let Some(last) = raw.last_mut() {
            if let Some(rest) = line.strip_prefix("text ") {
                last.0 = rest.to_string();
            } else if let Some(rest) = line.strip_prefix("json ") {
                last.1 = rest.to_string();
            } else if let Some(rest) = line.strip_prefix("stored ") {
                last.2.push(rest.to_string());
            }
        }
    }
    let single = columns.len() == 1;
    let named = columns
        .into_iter()
        .map(|(column, comment)| {
            if single {
                (String::new(), column, comment)
            } else {
                let (suffix, ty) = column.split_once(' ').unwrap_or((column.as_str(), ""));
                (suffix.to_string(), ty.to_string(), comment)
            }
        })
        .collect();
    let examples = raw
        .into_iter()
        .map(|(text, json, stored)| {
            let stored = if stored == ["never"] {
                Vec::new()
            } else if single {
                stored
                    .into_iter()
                    .map(|value| (String::new(), value))
                    .collect()
            } else {
                stored
                    .iter()
                    .map(|value| {
                        let (suffix, written) = value.split_once(' ').unwrap_or((value, ""));
                        (suffix.to_string(), written.to_string())
                    })
                    .collect()
            };
            GoldenExample { text, json, stored }
        })
        .collect();
    Golden {
        columns: named,
        examples,
    }
}

/// Every type's data, its page's words and its golden file, by name.
pub struct Reference {
    pub types: Vec<Json>,
    operations: Vec<Json>,
    names: Vec<String>,
    prose: Vec<TypePage>,
}

impl Reference {
    pub fn new() -> Reference {
        let Json::Array(types) = crate::type_reference::types_json() else {
            unreachable!("types is a list");
        };
        let Json::Array(operations) = crate::type_reference::operations_json() else {
            unreachable!("operations is a list");
        };
        let names = types
            .iter()
            .filter_map(|entry| text(entry, "name").map(str::to_string))
            .collect();
        Reference {
            types,
            operations,
            names,
            prose: pages::all(),
        }
    }

    fn names(&self) -> Vec<&str> {
        self.names.iter().map(String::as_str).collect()
    }

    /// `entry`'s page, as Markdown.
    pub fn page(&self, entry: &Json) -> String {
        let name = text(entry, "name").unwrap_or_default();
        let names = self.names();
        let golden = read_golden(name);
        let prose = self.prose.iter().find(|page| page.name == name);
        let words = |written: &str| linked_prose(written, name, &names);
        let mut out = vec![format!("# `{name}`"), String::new()];
        if let Some(prose) = prose {
            out.extend([words(&prose.meaning), String::new()]);
            for paragraph in prose.when.split("\n\n") {
                out.extend([words(paragraph), String::new()]);
            }
        }
        out.extend(["## Shape".to_string(), String::new()]);
        out.extend(self.shape(entry));
        out.extend(["## Example".to_string(), String::new()]);
        if let Some(prose) = prose {
            out.extend([
                "```jadpo".to_string(),
                prose.example.clone(),
                "```".to_string(),
                String::new(),
            ]);
        }
        out.extend(make_one(entry));
        out.extend(["## Good to know".to_string(), String::new()]);
        if let Some(prose) = prose {
            out.extend(prose.notes.iter().map(|note| format!("- {}", words(note))));
        }
        out.extend(standing_notes(entry));
        out.push(String::new());
        out.extend(["## In JSON".to_string(), String::new()]);
        out.extend(in_json(entry, &golden));
        out.extend(["## Details".to_string(), String::new()]);
        out.extend([
            "<details>".to_string(),
            "<summary>Text form, storage, how it is shown, and operations</summary>".to_string(),
            String::new(),
        ]);
        out.extend(self.details(entry, &golden));
        out.extend(["</details>".to_string()]);
        out.join("\n") + "\n"
    }

    fn shape(&self, entry: &Json) -> Vec<String> {
        let names = self.names();
        let name = text(entry, "name").unwrap_or_default();
        let holds = text(entry, "holds").unwrap_or_default();
        let fields = items(entry, "fields");
        let variants = items(entry, "variants");
        let mut out = Vec::new();
        let field_type = |field: &Json| {
            type_link(text(field, "type").unwrap_or_default(), &names)
        };
        let required = |field: &Json| if flag(field, "optional") { "No" } else { "Yes" };
        if !fields.is_empty() {
            out.extend(["| Field | Type | Required? |".to_string(), "|---|---|---|".to_string()]);
            for field in fields {
                out.push(format!(
                    "| `{}` | {} | {} |",
                    text(field, "name").unwrap_or_default(),
                    field_type(field),
                    required(field)
                ));
            }
            out.push(String::new());
            if fields.iter().any(|field| flag(field, "optional")) {
                out.extend([
                    "Write every field when you build one, and give a field that isn't required as `none`."
                        .to_string(),
                    String::new(),
                ]);
            }
        } else if holds == "choice" {
            out.extend([
                "One of these, each with its own fields:".to_string(),
                String::new(),
                "| Variant | Field | Type | Required? |".to_string(),
                "|---|---|---|---|".to_string(),
            ]);
            for variant in variants {
                let variant_name = text(variant, "name").unwrap_or_default();
                let inside = items(variant, "fields");
                if inside.is_empty() {
                    out.push(format!("| `{name}.{variant_name}` | | | |"));
                }
                for (i, field) in inside.iter().enumerate() {
                    let label = if i == 0 {
                        format!("`{name}.{variant_name}`")
                    } else {
                        String::new()
                    };
                    out.push(format!(
                        "| {label} | `{}` | {} | {} |",
                        text(field, "name").unwrap_or_default(),
                        field_type(field),
                        required(field)
                    ));
                }
            }
            out.push(String::new());
        } else if holds == "enum" {
            out.extend(enum_shape(entry));
        } else {
            let sample = sample_text(entry);
            let sentence = match holds {
                "text" | "bytes" => match text(entry, "text") {
                    Some("secret") => "One secret value, with no parts.".to_string(),
                    _ => format!("One value, held as text in one set form, with no parts: {}.", code(&sample)),
                },
                "whole" => format!(
                    "One whole number from {} to {}.",
                    bound(entry, "min").unwrap_or_default(),
                    bound(entry, "max").unwrap_or_default()
                ),
                "bounded" => {
                    let base = text(entry, "base").unwrap_or_default();
                    match (bound(entry, "min"), bound(entry, "max")) {
                        (Some(low), Some(high)) => format!("One `{base}` from {low} to {high}."),
                        (Some(low), None) => format!("One `{base}`, never below {low}."),
                        (None, Some(high)) => format!("One `{base}`, never above {high}."),
                        (None, None) => format!("One `{base}`."),
                    }
                }
                "list" => format!(
                    "A list of {}.",
                    type_link(argument(entry, 0).unwrap_or("items"), &names)
                ),
                "map" => format!(
                    "A map from {} to {}.",
                    type_link(argument(entry, 0).unwrap_or("K"), &names),
                    type_link(argument(entry, 1).unwrap_or("V"), &names)
                ),
                _ => format!("One value, with no fields to fill in. As text it reads {}.", code(&sample)),
            };
            out.extend([sentence, String::new()]);
        }
        let rules = items(entry, "rules");
        if !rules.is_empty() {
            out.extend([
                "Its parts keep these rules, and a value that breaks one fails with the rule's name:".to_string(),
                String::new(),
            ]);
            for rule in rules {
                out.push(format!(
                    "- `{}`: {}",
                    text(rule, "name").unwrap_or_default(),
                    text(rule, "written").unwrap_or_default()
                ));
            }
            out.push(String::new());
        }
        out
    }

    fn details(&self, entry: &Json, golden: &Golden) -> Vec<String> {
        let name = text(entry, "name").unwrap_or_default();
        let group = text(entry, "group").unwrap_or_default();
        let release = text(entry, "release").unwrap_or_default();
        let mut out = vec![
            format!("`{name}` is in the group {group}, and has been in Jadpo since {release}."),
            String::new(),
        ];
        out.extend(all_values(entry));
        out.extend(text_form(entry, golden));
        out.extend(stored(entry, golden));
        out.extend(shown(entry));
        out.extend(self.operations_section(entry));
        out
    }

    fn signature(&self, name: &str) -> String {
        let Some(operation) = self
            .operations
            .iter()
            .find(|operation| text(operation, "name") == Some(name))
        else {
            return format!("{name}()");
        };
        let params: Vec<String> = items(operation, "params")
            .iter()
            .map(|param| {
                format!(
                    "{}: {}{}",
                    text(param, "name").unwrap_or_default(),
                    text(param, "type").unwrap_or_default(),
                    if flag(param, "optional") { "?" } else { "" }
                )
            })
            .collect();
        format!(
            "{name}({}) -> {}",
            params.join(", "),
            text(operation, "result").unwrap_or_default()
        )
    }

    fn operations_section(&self, entry: &Json) -> Vec<String> {
        let mut out = vec!["### Operations".to_string(), String::new()];
        for operation in items(entry, "operations") {
            let name = text(operation, "name").unwrap_or_default();
            let summary = self
                .operations
                .iter()
                .find(|known| text(known, "name") == Some(name))
                .and_then(|known| text(known, "summary"))
                .map(|summary| format!(": {}", without_area(summary)))
                .unwrap_or_default();
            out.push(format!("- `{}`{summary}", self.signature(name)));
        }
        let mut always: Vec<String> = Vec::new();
        match text(entry, "text") {
            Some("canonical") => {
                always.extend(["`text.parse`".to_string(), "`text.from`".to_string()]);
            }
            Some("secret") => always.push("`text.parse`".to_string()),
            _ => {}
        }
        if text(entry, "text") != Some("secret") {
            always.push("`text.format`".to_string());
        }
        always.extend(["`json.read`".to_string(), "`json.write`".to_string(), "`==`".to_string()]);
        if !items(entry, "operations").is_empty() {
            out.push(String::new());
        }
        out.extend([
            format!(
                "It also takes {}. [How every standard type behaves](../topics/{ALL_TYPES_TOPIC}.md) says how they work.",
                joined(&always, "and")
            ),
            String::new(),
        ]);
        out
    }

    /// The A-to-Z index, as Markdown.
    pub fn index(&self) -> String {
        let mut rows: Vec<(&str, &str, String)> = self
            .types
            .iter()
            .map(|entry| {
                let name = text(entry, "name").unwrap_or_default();
                let meaning = self
                    .prose
                    .iter()
                    .find(|page| page.name == name)
                    .map(|page| page.meaning.replace('|', "\\|"))
                    .unwrap_or_default();
                (name, text(entry, "group").unwrap_or_default(), meaning)
            })
            .collect();
        rows.sort_by_key(|(name, _, _)| name.to_lowercase());
        let mut out = vec![
            "# Standard types".to_string(),
            String::new(),
            format!(
                "Every standard type Jadpo has, {} of them, from A to Z. `jadpo help <Type>` prints the same page in your terminal. [The standard types](../topics/the-standard-types.md) explains how the list is arranged.",
                rows.len()
            ),
            String::new(),
            "| Type | Group | What it is |".to_string(),
            "|---|---|---|".to_string(),
        ];
        for (name, group, meaning) in rows {
            out.push(format!(
                "| [`{name}`]({}.md) | {group} | {meaning} |",
                type_id(name)
            ));
        }
        out.join("\n") + "\n"
    }
}

/// A type's first example as text.
fn sample_text(entry: &Json) -> String {
    let examples = get(entry, "examples");
    examples
        .map(|examples| items(examples, "valid"))
        .and_then(|valid| valid.last())
        .and_then(|valid| text(valid, "text"))
        .unwrap_or_default()
        .to_string()
}

fn argument(entry: &Json, at: usize) -> Option<&str> {
    match items(entry, "arguments").get(at) {
        Some(Json::Str(argument)) => Some(argument),
        _ => None,
    }
}

/// An operation's description as a reader sees it, the area it cites by
/// number left out.
fn without_area(summary: &str) -> String {
    let mut out = summary.to_string();
    while let Some(open) = out.find("(area ") {
        let Some(len) = out[open..].find(')') else {
            break;
        };
        let inside = &out[open + 1..open + len];
        match inside.split_once(", ") {
            // The standard it names is kept: `(ISO 13616)`.
            Some((_, standard)) if !standard.starts_with('"') => {
                let replaced = format!("({standard})");
                out.replace_range(open..=open + len, &replaced);
            }
            _ => {
                let start = if out[..open].ends_with(' ') { open - 1 } else { open };
                out.replace_range(start..=open + len, "");
            }
        }
    }
    out
}

fn enum_shape(entry: &Json) -> Vec<String> {
    let name = text(entry, "name").unwrap_or_default();
    let variants = items(entry, "variants");
    let pair = |variant: &Json| {
        (
            text(variant, "name").unwrap_or_default().to_string(),
            text(variant, "code").unwrap_or_default().to_string(),
        )
    };
    if variants.len() > IN_SHAPE {
        let (first, written) = variants.first().map(pair).unwrap_or_default();
        return vec![
            format!(
                "One of {} values, such as `{name}.{first}`, written {}. Details has them.",
                variants.len(),
                code(&written)
            ),
            String::new(),
        ];
    }
    let mut out = vec![
        format!("One of these {} values:", variants.len()),
        String::new(),
    ];
    if variants.iter().map(pair).all(|(name, written)| name == written) {
        out.extend([
            variants
                .iter()
                .map(pair)
                .map(|(variant, _)| format!("`{name}.{variant}`"))
                .collect::<Vec<_>>()
                .join(", ")
                + ".",
            String::new(),
        ]);
    } else {
        out.extend(["| Value | Written |".to_string(), "|---|---|".to_string()]);
        out.extend(
            variants
                .iter()
                .map(pair)
                .map(|(variant, written)| format!("| `{name}.{variant}` | {} |", cell(&written))),
        );
        out.push(String::new());
    }
    out
}

/// A long enum's values, in Details.
fn all_values(entry: &Json) -> Vec<String> {
    let variants = items(entry, "variants");
    if text(entry, "holds") != Some("enum") || variants.len() <= IN_SHAPE {
        return Vec::new();
    }
    let name = text(entry, "name").unwrap_or_default();
    let shown = variants.len().min(if variants.len() > LISTED { IN_SHAPE } else { LISTED });
    let listed: Vec<String> = variants[..shown]
        .iter()
        .map(|variant| {
            let (variant_name, written) = (
                text(variant, "name").unwrap_or_default(),
                text(variant, "code").unwrap_or_default(),
            );
            if variant_name == written {
                format!("`{variant_name}`")
            } else {
                format!("`{variant_name}` ({})", code(written))
            }
        })
        .collect();
    let lead = if shown < variants.len() {
        format!(
            "The first {shown} of {}; `jadpo help {name} --json` lists every one:",
            variants.len()
        )
    } else {
        "Every value, each after `".to_string() + name + ".`, with how it is written:"
    };
    vec![
        "### Every value".to_string(),
        String::new(),
        lead,
        String::new(),
        listed.join(", ") + ".",
        String::new(),
    ]
}

/// How to make one beyond the example: what is checked, and what gives one.
fn make_one(entry: &Json) -> Vec<String> {
    let name = text(entry, "name").unwrap_or_default();
    let holds = text(entry, "holds").unwrap_or_default();
    let mut out = Vec::new();
    match (holds, text(entry, "text")) {
        ("text", Some("secret")) => out.push(
            "In a real app it arrives from outside, such as in a request, rather than as text you write in your code.".to_string(),
        ),
        ("text", _) => out.push(format!(
            "`{name}(raw)` checks the text, and text that isn't one breaks the `format` rule."
        )),
        ("whole", _) => out.push(format!(
            "`{name}(number)` checks the number, and one outside its limits breaks `min` or `max`."
        )),
        ("bounded", _) => out.push(format!(
            "A value outside its limits breaks `min` or `max`."
        )),
        ("bytes", _) => out.push(format!(
            "`{name}(bytes)` checks them, and bytes that aren't one break the `format` rule."
        )),
        _ => {}
    }
    if matches!(text(entry, "text"), Some("canonical" | "secret")) {
        let written = match (holds, get(entry, "examples").and_then(|e| text(e, "of"))) {
            ("range", Some(of)) => format!("{name}<{of}>"),
            _ => name.to_string(),
        };
        let malformed: Vec<String> = get(entry, "examples")
            .map(|examples| items(examples, "malformed"))
            .unwrap_or_default()
            .iter()
            .filter_map(|bad| match bad {
                Json::Str(bad) if !bad.is_empty() && bad.len() <= 40 && !bad.chars().any(char::is_control) => {
                    Some(code(bad))
                }
                _ => None,
            })
            .take(3)
            .collect();
        let such = if malformed.is_empty() {
            String::new()
        } else {
            format!(", such as {}", joined(&malformed, "or"))
        };
        out.push(format!(
            "`text.parse(raw, as: {written})` reads one from text that comes from outside. Text that isn't one at all fails `malformed`{such}."
        ));
    }
    let gives: Vec<String> = items(entry, "operations")
        .iter()
        .filter(|operation| flag(operation, "gives"))
        .filter_map(|operation| text(operation, "name"))
        .map(|name| format!("`{name}`"))
        .collect();
    if !gives.is_empty() {
        out.push(format!(
            "{} one: {}.",
            if gives.len() == 1 { "This operation gives" } else { "These operations give" },
            joined(&gives, "and")
        ));
    }
    if out.is_empty() {
        return Vec::new();
    }
    let mut lines = Vec::new();
    for sentence in out {
        lines.extend([sentence, String::new()]);
    }
    lines
}

/// What the compiler's data says every reader should know: how two compare,
/// and whether it counts as personal data.
fn standing_notes(entry: &Json) -> Vec<String> {
    let mut out = Vec::new();
    if flag(entry, "ordered") {
        out.push(
            "- `==` tells you whether two are the same, and `<` and `list.sort_by` compare and sort them."
                .to_string(),
        );
    } else if items(entry, "fields").is_empty() {
        out.push(
            "- `==` tells you whether two are the same. `<` and `list.sort_by` don't take it."
                .to_string(),
        );
    } else {
        out.push(
            "- `==` tells you whether two are the same. `<` and `list.sort_by` don't take it, so sort by one of its parts instead."
                .to_string(),
        );
    }
    if let Some(category) = text(entry, "personal") {
        out.push(format!(
            "- A field that holds one counts as personal data (`{category}`), so you don't mark it yourself."
        ));
    }
    out
}

fn in_json(entry: &Json, golden: &Golden) -> Vec<String> {
    let words = match text(entry, "json").unwrap_or_default() {
        "text" => "A string".to_string(),
        "object" => "An object of its parts".to_string(),
        "tagged" => "An object of its variant's name, as `tag`, and the variant's fields".to_string(),
        "number" => "A number".to_string(),
        "array" => "An array of its items".to_string(),
        "keyed" => "An object of its entries, each key written as its text".to_string(),
        "geojson" => "A GeoJSON geometry, with each position's longitude first".to_string(),
        "feature" => "A GeoJSON object of its geometry and properties".to_string(),
        "secret" => "Never its value: it is written redacted".to_string(),
        _ => "One shape".to_string(),
    };
    let mut out = vec![format!("{words}, in a request, a response or `json.write`:"), String::new()];
    if !golden.examples.is_empty() {
        out.push("```json".to_string());
        out.extend(golden.examples.iter().take(JSON_SHOWN).map(|example| example.json.clone()));
        out.extend(["```".to_string(), String::new()]);
    }
    if items(entry, "fields").iter().any(|field| flag(field, "optional")) {
        out.extend([
            "When you read one, a field that isn't required may be left out, or sent as `null`.".to_string(),
            String::new(),
        ]);
    }
    out
}

fn text_form(entry: &Json, golden: &Golden) -> Vec<String> {
    let mut out = vec!["### Text form".to_string(), String::new()];
    match text(entry, "text") {
        Some("secret") => out.extend([
            "`text.parse` reads one from its text as it is typed. `text.from` and `text.format` don't take it, so nothing writes it.".to_string(),
            String::new(),
        ]),
        Some(_) => {
            let shown: Vec<String> = golden
                .examples
                .iter()
                .filter(|example| example.text != "none")
                .take(JSON_SHOWN)
                .map(|example| cell(&example.text))
                .collect();
            out.extend([
                format!(
                    "`text.from` writes one standard text form, the same form `text.parse` reads back: {}.",
                    joined(&shown, "and")
                ),
                String::new(),
            ]);
        }
        None => out.extend([
            "It has no single text form, so `text.from` and `text.parse` don't take it.".to_string(),
            String::new(),
        ]),
    }
    out
}

fn stored(entry: &Json, golden: &Golden) -> Vec<String> {
    let mut out = vec!["### How it is stored".to_string(), String::new()];
    if text(entry, "text") == Some("secret") && golden.columns.is_empty() {
        out.extend(["It is never stored.".to_string(), String::new()]);
        return out;
    }
    let middle = golden.examples.get(golden.examples.len() / 2);
    let written = middle
        .map(|example| {
            if example.text != "none" && !example.text.is_empty() {
                example.text.clone()
            } else {
                example.json.clone()
            }
        })
        .unwrap_or_default();
    match golden.columns.as_slice() {
        [] => out.extend(["It is never stored.".to_string(), String::new()]),
        [(_, ty, comment)] => {
            let what = comment
                .as_ref()
                .map(|comment| {
                    let mut chars = comment.chars();
                    let first = chars.next().map(|c| c.to_ascii_lowercase()).unwrap_or_default();
                    format!(", holding the {first}{}", chars.as_str())
                })
                .unwrap_or_default();
            let mut sentence = format!("It is stored in one column named for the field, of type `{ty}`{what}.");
            if let Some((_, value)) = middle.and_then(|example| example.stored.first()) {
                sentence.push_str(&format!(" {} is stored as {}.", cell(&written), cell(value)));
            }
            out.extend([sentence, String::new()]);
        }
        columns => {
            out.extend([
                format!(
                    "It is stored in {} columns, each named for the field, `_` and its suffix. This is {}:",
                    columns.len(),
                    cell(&written)
                ),
                String::new(),
                "| Column | Type | Holds |".to_string(),
                "|---|---|---|".to_string(),
            ]);
            for (suffix, ty, _) in columns {
                let value = middle
                    .and_then(|example| example.stored.iter().find(|(name, _)| name == suffix))
                    .map(|(_, value)| cell(value))
                    .unwrap_or_default();
                out.push(format!("| `<field>_{suffix}` | `{ty}` | {value} |"));
            }
            out.push(String::new());
        }
    }
    out
}

fn shown(entry: &Json) -> Vec<String> {
    let name = text(entry, "name").unwrap_or_default();
    let mut out = vec!["### How it is shown to people".to_string(), String::new()];
    if text(entry, "text") == Some("secret") {
        out.extend([format!("`text.format` doesn't take it: a `{name}` is never shown."), String::new()]);
        return out;
    }
    let options = get(entry, "format").map(|format| items(format, "options")).unwrap_or_default();
    let of = |needed: bool| -> Vec<String> {
        options
            .iter()
            .filter(|option| flag(option, "needed") == needed)
            .filter_map(|option| text(option, "name"))
            .map(|option| format!("`{option}:`"))
            .collect()
    };
    let mut sentence = "`text.format(value, locale:)` shows it to people in their locale.".to_string();
    if !of(true).is_empty() {
        sentence.push_str(&format!(" A call also gives {}.", joined(&of(true), "and")));
    }
    if !of(false).is_empty() {
        sentence.push_str(&format!(" A call may also give {}.", joined(&of(false), "or")));
    }
    out.extend([sentence, String::new()]);
    let examples = get(entry, "examples");
    let valid = examples.map(|examples| items(examples, "valid")).unwrap_or_default();
    if !valid.is_empty() {
        let mut given = vec!["`locale: Locale.en_gb`".to_string()];
        if options.iter().any(|option| text(option, "name") == Some("zone")) {
            given.push("`zone: Zone.europe_london`".to_string());
        }
        if let Some(format) = examples.and_then(|examples| text(examples, "format")) {
            given.push(format!("`format: TimeFormat.{format}`"));
        }
        out.extend([
            format!("With {}:", joined(&given, "and")),
            String::new(),
            "| Value | Shown |".to_string(),
            "|---|---|".to_string(),
        ]);
        for example in valid {
            out.push(format!(
                "| {} | {} |",
                cell(text(example, "text").unwrap_or_default()),
                cell(text(example, "shown").unwrap_or_default())
            ));
        }
        out.push(String::new());
    }
    out
}

/// `[text](target)` read as `text`.
fn plain_links(paragraph: &str) -> String {
    crate::help::link_text(paragraph)
}

/// A page's Markdown as blocks for the terminal: every section in the same
/// order, Details open, each table as aligned columns.
fn page_blocks(page: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut heading: Option<String> = None;
    let mut paragraph = String::new();
    let mut prose: Vec<String> = Vec::new();
    let mut lines = page
        .lines()
        .skip_while(|line| line.starts_with("# ") || line.is_empty())
        .peekable();
    // The header holds the meaning, so it isn't printed twice.
    lines.next();
    let flush_prose = |paragraph: &mut String, prose: &mut Vec<String>, heading: &mut Option<String>, blocks: &mut Vec<Block>| {
        if !paragraph.is_empty() {
            prose.push(plain_links(&std::mem::take(paragraph)));
        }
        if !prose.is_empty() {
            if let Some(heading) = heading.take() {
                blocks.push(Block::section(heading, Vec::<String>::new()));
            }
            blocks.push(Block::prose(std::mem::take(prose)));
        }
    };
    while let Some(line) = lines.next() {
        if matches!(line, "<details>" | "</details>") || line.starts_with("<summary>") {
            continue;
        }
        if line.starts_with("```") {
            flush_prose(&mut paragraph, &mut prose, &mut heading, &mut blocks);
            let body: Vec<String> = lines
                .by_ref()
                .take_while(|line| line.trim_end() != "```")
                .map(str::to_string)
                .collect();
            blocks.push(Block::section(heading.take().unwrap_or_default(), body));
            continue;
        }
        if line.starts_with('|') {
            flush_prose(&mut paragraph, &mut prose, &mut heading, &mut blocks);
            let mut rows = vec![table_cells(line)];
            while let Some(next) = lines.peek().copied() {
                if !next.starts_with('|') {
                    break;
                }
                lines.next();
                if !next.starts_with("|---") {
                    rows.push(table_cells(next));
                }
            }
            blocks.push(Block::section(heading.take().unwrap_or_default(), aligned(rows)));
            continue;
        }
        if let Some(title) = line.strip_prefix("## ").or_else(|| line.strip_prefix("### ")) {
            flush_prose(&mut paragraph, &mut prose, &mut heading, &mut blocks);
            if let Some(empty) = heading.take() {
                blocks.push(Block::section(empty, Vec::<String>::new()));
            }
            heading = Some(title.to_string());
        } else if line.is_empty() {
            if !paragraph.is_empty() {
                prose.push(plain_links(&std::mem::take(&mut paragraph)));
            }
        } else if line.starts_with("- ") {
            if !paragraph.is_empty() {
                prose.push(plain_links(&std::mem::take(&mut paragraph)));
            }
            paragraph.push_str(line);
        } else {
            if !paragraph.is_empty() {
                paragraph.push(' ');
            }
            paragraph.push_str(line);
        }
    }
    flush_prose(&mut paragraph, &mut prose, &mut heading, &mut blocks);
    blocks
}

fn table_cells(line: &str) -> Vec<String> {
    line.trim_matches('|')
        .split(" | ")
        .map(|cell| plain_links(cell.trim()).replace('`', "").replace("\\|", "|"))
        .collect()
}

/// Rows in columns with two spaces between, the header in lower case.
fn aligned(mut rows: Vec<Vec<String>>) -> Vec<String> {
    if let Some(header) = rows.first_mut() {
        for cell in header.iter_mut() {
            *cell = cell.to_lowercase();
        }
    }
    let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
    let widths: Vec<usize> = (0..columns)
        .map(|column| {
            rows.iter()
                .filter_map(|row| row.get(column))
                .map(|cell| cell.chars().count())
                .max()
                .unwrap_or(0)
        })
        .collect();
    rows.into_iter()
        .map(|row| {
            let mut line = String::new();
            for (column, cell) in row.iter().enumerate() {
                line.push_str(cell);
                if column + 1 < row.len() {
                    let pad = widths[column].saturating_sub(cell.chars().count()) + 2;
                    line.push_str(&" ".repeat(pad));
                }
            }
            line.trim_end().to_string()
        })
        .collect()
}

/// The standard type a word names: exactly, or written another way, such as
/// `personname`, `person_name` or `Range<CalendarDate>`.
pub fn find(word: &str) -> Option<&'static str> {
    let wanted = |written: &str| {
        written
            .chars()
            .take_while(|c| *c != '<')
            .filter(|c| *c != '_' && *c != '-')
            .map(|c| c.to_ascii_lowercase())
            .collect::<String>()
    };
    let word = wanted(word);
    jadpo_std::standard::STANDARD_TYPES
        .iter()
        .find(|entry| wanted(entry.name) == word)
        .map(|entry| entry.name)
}

/// `jadpo help <Type>`: its page. With `--json`, the same data as `jadpo help
/// --json` gives it, its page and its own words, which is its shape in one call.
pub fn type_help(name: &str) -> Reply {
    let reference = Reference::new();
    let Some(entry) = reference
        .types
        .iter()
        .find(|entry| text(entry, "name") == Some(name))
    else {
        return Reply::failed(crate::run::unknown_topic(name));
    };
    let page = reference.page(entry);
    let mut facts = vec![text(entry, "group").unwrap_or_default().to_string()];
    if let Some(category) = text(entry, "personal") {
        facts.push(format!("personal data ({category})"));
    }
    let meaning = reference
        .prose
        .iter()
        .find(|prose| prose.name == name)
        .map(|prose| prose.meaning.clone())
        .unwrap_or_default();
    let mut screen = Screen::new().with(Block::header(name, facts).with_text(meaning));
    for block in page_blocks(&page) {
        screen.push(block);
    }
    screen.push(Block::hint("jadpo help types", "lists every standard type"));
    let mut data = entry.clone();
    if let Json::Object(fields) = &mut data {
        fields.extend(words_json(&reference, name));
        fields.push(("page".to_string(), Json::from(page)));
    }
    Reply::screen(screen, data)
}

/// `jadpo help types`: every standard type from A to Z. With `--json`, each
/// one's page too, which the docs bundle publishes.
pub fn index_help() -> Reply {
    let reference = Reference::new();
    let mut named: Vec<&Json> = reference.types.iter().collect();
    named.sort_by_key(|entry| text(entry, "name").unwrap_or_default().to_lowercase());
    let meaning = |name: &str| {
        reference
            .prose
            .iter()
            .find(|prose| prose.name == name)
            .map(|prose| prose.meaning.clone())
            .unwrap_or_default()
    };
    let rows: Vec<Row> = named
        .iter()
        .map(|entry| {
            let name = text(entry, "name").unwrap_or_default();
            Row::new(name, meaning(name))
        })
        .collect();
    let screen = Screen::new()
        .with(Block::list(format!("Standard types ({})", rows.len()), rows))
        .with(Block::hint("jadpo help Money", "shows one type's shape and an example"));
    let data = Json::object([
        ("index", Json::from(reference.index())),
        (
            "types",
            Json::array(named.iter().map(|entry| {
                let name = text(entry, "name").unwrap_or_default();
                Json::object([
                    ("name", Json::from(name)),
                    ("id", Json::from(type_id(name))),
                    ("group", Json::from(text(entry, "group").unwrap_or_default())),
                    ("meaning", Json::from(meaning(name))),
                    (
                        "example",
                        Json::from(
                            reference
                                .prose
                                .iter()
                                .find(|prose| prose.name == name)
                                .map(|prose| prose.example.clone())
                                .unwrap_or_default(),
                        ),
                    ),
                    ("page", Json::from(reference.page(entry))),
                ])
            })),
        ),
    ]);
    Reply::screen(screen, data)
}

fn words_json(reference: &Reference, name: &str) -> Vec<(String, Json)> {
    let Some(prose) = reference.prose.iter().find(|prose| prose.name == name) else {
        return Vec::new();
    };
    vec![
        ("meaning".to_string(), Json::from(prose.meaning.as_str())),
        ("when".to_string(), Json::from(prose.when.as_str())),
        ("example".to_string(), Json::from(prose.example.as_str())),
        (
            "notes".to_string(),
            Json::array(prose.notes.iter().map(|note| Json::from(note.as_str()))),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(name: &str) -> String {
        let reference = Reference::new();
        let entry = reference
            .types
            .iter()
            .find(|entry| text(entry, "name") == Some(name))
            .expect("listed");
        reference.page(entry)
    }

    #[test]
    fn a_page_id_is_kebab_case() {
        assert_eq!(type_id("UkSortCode"), "type-uk-sort-code");
        assert_eq!(type_id("H3Cell"), "type-h3-cell");
        assert_eq!(type_id("Ean13"), "type-ean13");
        assert_eq!(type_id("Money"), "type-money");
        assert_eq!(type_id("OAuthScope"), "type-o-auth-scope");
    }

    #[test]
    fn a_page_reads_in_the_agreed_order() {
        for entry in Reference::new().types.iter() {
            let name = text(entry, "name").unwrap_or_default();
            let page = page(name);
            let mut from = 0;
            for heading in ["## Shape", "## Example", "## Good to know", "## In JSON", "## Details"] {
                let Some(at) = page[from..].find(heading) else {
                    panic!("{name}: {heading} is missing or out of order:\n{page}");
                };
                from += at;
            }
        }
    }

    #[test]
    fn person_name_shows_what_the_agreed_sample_shows() {
        let page = page("PersonName");
        assert!(page.starts_with("# `PersonName`\n\nA person's name"), "{page}");
        assert!(page.contains("| Field | Type | Required? |"), "{page}");
        assert!(page.contains("| `given` | [`GivenName`](type-given-name.md) | Yes |"), "{page}");
        assert!(page.contains("| `family` | [`FamilyName`](type-family-name.md) | No |"), "{page}");
        assert!(page.contains("PersonName { given: \"Mary\" family: \"Somerville\" }"), "{page}");
        assert!(page.contains("{\"given\":\"Mary\",\"family\":\"Somerville\"}"), "{page}");
        assert!(page.contains("<details>"), "{page}");
    }

    #[test]
    fn no_page_uses_the_words_the_template_rules_out() {
        for entry in Reference::new().types.iter() {
            let name = text(entry, "name").unwrap_or_default();
            let page = page(name);
            let plain = page.replace(&format!("../topics/{ALL_TYPES_TOPIC}.md"), "");
            for word in ["contract", "baseline", "not ordered"] {
                let found = plain
                    .split('`')
                    .step_by(2)
                    .any(|prose| prose.to_lowercase().contains(word));
                assert!(!found, "{name}: \"{word}\" in\n{page}");
            }
        }
    }

    #[test]
    fn every_type_has_a_golden_file_packed() {
        for entry in jadpo_std::standard::STANDARD_TYPES {
            assert!(unpack(entry.name).is_some(), "{} has no golden file", entry.name);
        }
    }

    #[test]
    fn the_index_runs_a_to_z_with_a_group_column() {
        let index = Reference::new().index();
        assert!(index.contains("| Type | Group | What it is |"), "{index}");
        let names: Vec<&str> = index
            .lines()
            .filter_map(|line| line.strip_prefix("| [`"))
            .filter_map(|line| line.split('`').next())
            .collect();
        assert_eq!(names.len(), jadpo_std::standard::STANDARD_TYPES.len());
        let mut sorted = names.clone();
        sorted.sort_by_key(|name| name.to_lowercase());
        assert_eq!(names, sorted);
    }

    #[test]
    fn a_type_is_found_however_it_is_written() {
        assert_eq!(find("PersonName"), Some("PersonName"));
        assert_eq!(find("personname"), Some("PersonName"));
        assert_eq!(find("person_name"), Some("PersonName"));
        assert_eq!(find("Range<CalendarDate>"), Some("Range"));
        assert_eq!(find("Nonsense"), None);
    }

    #[test]
    fn the_terminal_prints_a_table_in_columns() {
        let rows = aligned(vec![
            table_cells("| Field | Type | Required? |"),
            table_cells("| `given` | [`GivenName`](type-given-name.md) | Yes |"),
        ]);
        assert_eq!(rows[0], "field  type       required?");
        assert_eq!(rows[1], "given  GivenName  Yes");
    }
}
