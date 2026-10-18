//! `jadpo suggest`'s scan (area 14, "Suggestions"; chunk 5.5x): where code
//! could use a standard type or a standard operation. It never edits.
//!
//! What it looks for is data: each standard type's entry holds its shape
//! and its [`jadpo_std::Signals`], and operations' loops are
//! [`LOOP_SIGNALS`]. Each kind of signal is one engine below
//! that reads every entry, so no type has code of its own here. Engines
//! match canonical fragments ([`canonical`]): the program graph's typed
//! bodies and declarations with their structure normalised and their names
//! kept. A snippet's hash of its canonical form is the index ignores and
//! taught snippets are found by, and the canonical form confirms a match.

pub mod canonical;
pub mod record;

use std::collections::{BTreeMap, HashMap};

use jadpo_db::{Db, Project, name_words};
use jadpo_graph::body::{BinaryOp, Body, Expr, LocalKind, Node, SiteId, Stmt};
use jadpo_graph::{Base, Detail, Graph, Key, Kind, TypeRef};
use jadpo_std::standard::{Holds, LOOP_SIGNALS, LoopShape};
use jadpo_std::{Builtin, Num, StandardType};

use crate::types::{callables_of, typed_body};
use canonical::{C, Canon, normal};

/// A kind of signal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Signal {
    Shape,
    Conversion,
    Name,
    Format,
    Validation,
    Loop,
    /// A snippet `jadpo suggest found` recorded.
    Taught,
}

impl Signal {
    pub const ALL: [Signal; 7] = [
        Signal::Shape,
        Signal::Conversion,
        Signal::Name,
        Signal::Format,
        Signal::Validation,
        Signal::Loop,
        Signal::Taught,
    ];

    pub fn word(self) -> &'static str {
        match self {
            Signal::Shape => "shape",
            Signal::Conversion => "conversion",
            Signal::Name => "name",
            Signal::Format => "format",
            Signal::Validation => "validation",
            Signal::Loop => "loop",
            Signal::Taught => "taught",
        }
    }

    pub fn from_word(word: &str) -> Option<Signal> {
        Signal::ALL.into_iter().find(|signal| signal.word() == word)
    }
}

/// The smallest expression, statement or declaration member a finding is
/// about, in both its forms.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snippet {
    /// The project-relative file.
    pub path: String,
    /// The one-based line it starts on.
    pub line: u32,
    /// Its byte range in the file.
    pub start: u32,
    pub end: u32,
    /// The source as written.
    pub text: String,
    /// One line for a list: the text, or the names of a shape's fields.
    pub shown: String,
    pub canonical: String,
    /// The canonical form's hash: what an ignore holds.
    pub hash: String,
    /// The hash of the text without comments: what sharing sends.
    pub text_hash: String,
}

/// One place code could use a standard type or operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    pub snippet: Snippet,
    pub signal: Signal,
    /// The standard type or operation: `Money`, `list.sum`.
    pub suggestion: String,
    /// Why, in a sentence.
    pub reason: String,
    /// How many words of its names the signal matched, which decides
    /// between two types that fire on one snippet.
    weight: usize,
    /// Its place among the entries, the tie-break after `weight`.
    order: usize,
}

/// A snippet `jadpo suggest found` taught: the next scan finds its
/// canonical form anywhere.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Taught {
    pub hash: String,
    pub canonical: String,
    pub suggestion: String,
    pub reason: String,
}

/// The hash Jadpo keeps of a text: 16 hex digits of BLAKE3.
pub fn hash(text: &str) -> String {
    blake3::hash(text.as_bytes()).to_hex()[..16].to_string()
}

/// `text` without its `//` comments and the spaces they leave, as sharing
/// hashes it.
pub fn without_comments(text: &str) -> String {
    let mut lines = Vec::new();
    for line in text.lines() {
        let mut quoted = false;
        let mut escaped = false;
        let mut cut = line.len();
        let bytes = line.as_bytes();
        for (i, &b) in bytes.iter().enumerate() {
            if escaped {
                escaped = false;
            } else if b == b'\\' && quoted {
                escaped = true;
            } else if b == b'"' {
                quoted = !quoted;
            } else if !quoted && b == b'/' && bytes.get(i + 1) == Some(&b'/') {
                cut = i;
                break;
            }
        }
        let kept = line[..cut].trim_end();
        if !kept.trim().is_empty() {
            lines.push(kept.to_string());
        }
    }
    lines.join("\n")
}

/// Every standard type and operation a suggestion can name.
pub fn suggestions() -> Vec<String> {
    let mut out: Vec<String> = jadpo_std::STANDARD_TYPES
        .iter()
        .map(|entry| entry.name.to_string())
        .collect();
    out.extend(
        jadpo_std::OPERATIONS
            .iter()
            .map(|operation| operation.qualified()),
    );
    out
}

/// The words of a name, lower case: `totalPrice` and `total_price` are
/// `total price`.
fn words(name: &str) -> Vec<String> {
    name_words(name)
        .iter()
        .map(|word| word.to_lowercase())
        .collect()
}

/// How many of `written` words are one of `hints`, or its plural.
fn matched(written: &[String], hints: &[String]) -> usize {
    written
        .iter()
        .filter(|word| {
            hints.iter().any(|hint| {
                *word == hint
                    || word.strip_suffix('s') == Some(hint.as_str())
                    || word.strip_suffix("es") == Some(hint.as_str())
            })
        })
        .count()
}

/// Whether `token` stands in `text` with no letter either side.
fn holds_token(text: &str, token: &str) -> bool {
    let mut from = 0;
    while let Some(at) = text[from..].find(token) {
        let start = from + at;
        let end = start + token.len();
        let before = text[..start].chars().next_back();
        let after = text[end..].chars().next();
        if !before.is_some_and(char::is_alphabetic) && !after.is_some_and(char::is_alphabetic) {
            return true;
        }
        from = start + token.len().max(1);
        if from >= text.len() {
            break;
        }
    }
    false
}

/// `p / q` as an exact decimal, when it is one with few digits.
fn exact(p: u128, q: u128) -> Option<Num> {
    fn gcd(a: u128, b: u128) -> u128 {
        if b == 0 { a } else { gcd(b, a % b) }
    }
    let g = gcd(p, q).max(1);
    let (mut p, mut q) = (p / g, q / g);
    let mut scale = 0u32;
    while q % 2 == 0 {
        q /= 2;
        p = p.checked_mul(5)?;
        scale += 1;
    }
    while q % 5 == 0 {
        q /= 5;
        p = p.checked_mul(2)?;
        scale += 1;
    }
    if q != 1 || scale > 9 || p > 10u128.pow(12) {
        return None;
    }
    Some(normal(Num {
        mantissa: i128::try_from(p).ok()?,
        scale,
    }))
}

/// One entry's signals, with its units' added: what the engines read.
struct Reading {
    entry: &'static StandardType,
    order: usize,
    names: Vec<String>,
    conversions: Vec<Num>,
    format_literals: Vec<String>,
}

fn readings() -> Vec<Reading> {
    let mut out = Vec::new();
    for (order, entry) in jadpo_std::STANDARD_TYPES.iter().enumerate() {
        let signals = entry.signals;
        let mut names: Vec<String> = signals.names.iter().map(|n| n.to_string()).collect();
        let mut conversions: Vec<Num> = signals
            .conversions
            .iter()
            .filter_map(|text| Num::parse(text).map(normal))
            .collect();
        let mut format_literals: Vec<String> = signals
            .format_literals
            .iter()
            .map(|t| t.to_string())
            .collect();
        if signals.units
            && let Some(kind) = entry.unit_kind()
        {
            let units = kind.units();
            for unit in units {
                if !unit.name.contains('_') {
                    names.push(unit.name.to_lowercase());
                }
                if unit.code.len() >= 2 && unit.code.chars().all(|c| c.is_ascii_alphabetic()) {
                    names.push(unit.code.to_lowercase());
                    format_literals.push(unit.code.to_string());
                }
                if !unit.symbol.is_empty() && !unit.symbol.is_ascii() {
                    format_literals.push(unit.symbol.to_string());
                }
            }
            for a in units {
                for b in units {
                    if a.offset.0 != 0 || b.offset.0 != 0 {
                        continue;
                    }
                    // How many of `b` make one `a`.
                    let (Some(p), Some(q)) = (a.num.checked_mul(b.den), a.den.checked_mul(b.num))
                    else {
                        continue;
                    };
                    if p > q
                        && let Some(factor) = exact(p, q)
                        && !conversions.contains(&factor)
                    {
                        conversions.push(factor);
                    }
                }
            }
        }
        names.sort();
        names.dedup();
        out.push(Reading {
            entry,
            order,
            names,
            conversions,
            format_literals,
        });
    }
    out
}

/// Whether `ty` is, or is built on, the standard type `entry`.
fn is_standard(graph: &Graph, ty: &TypeRef, entry: &StandardType) -> bool {
    let mut ty = ty.clone();
    for _ in 0..16 {
        match ty {
            TypeRef::Builtin(builtin) => return entry.holds == Holds::Builtin(builtin),
            TypeRef::Std(name, _) => return name == entry.name,
            TypeRef::Nullable(inner) => ty = *inner,
            TypeRef::Node(key) | TypeRef::Field(key) => match graph.get(&key).map(|n| &n.detail) {
                Some(Detail::Type {
                    base: Base::Refines(inner),
                    ..
                }) => ty = inner.clone(),
                Some(Detail::Field { ty: inner, .. }) => ty = inner.clone(),
                _ => return false,
            },
            _ => return false,
        }
    }
    false
}

/// The built-in type `ty` is built on, through refinements.
fn base_builtin(graph: &Graph, ty: &TypeRef) -> Option<Builtin> {
    let mut ty = ty.clone();
    for _ in 0..16 {
        match ty {
            TypeRef::Builtin(builtin) => return Some(builtin),
            TypeRef::Node(key) | TypeRef::Field(key) => match graph.get(&key).map(|n| &n.detail) {
                Some(Detail::Type {
                    base: Base::Refines(inner),
                    ..
                }) => ty = inner.clone(),
                Some(Detail::Field { ty: inner, .. }) => ty = inner.clone(),
                _ => return None,
            },
            _ => return None,
        }
    }
    None
}

/// The name of the project's type `ty` names, if it names one.
fn type_name(ty: &TypeRef) -> Option<&str> {
    match ty {
        TypeRef::Node(key) => Some(key.written().rsplit('.').next().unwrap_or(key.written())),
        _ => None,
    }
}

/// Whether `ty` is an enum without data: the project's own or a standard
/// one.
fn is_enum(graph: &Graph, ty: &TypeRef) -> bool {
    match ty {
        TypeRef::Node(key) => matches!(
            graph.get(key).map(|n| &n.detail),
            Some(Detail::Type {
                base: Base::Enum,
                ..
            })
        ),
        TypeRef::Std(name, _) => {
            jadpo_std::standard_type(name).is_some_and(|e| matches!(e.holds, Holds::Enum(_)))
        }
        _ => false,
    }
}

fn line_of(text: &str, offset: u32) -> u32 {
    let offset = (offset as usize).min(text.len());
    text[..offset].bytes().filter(|&b| b == b'\n').count() as u32 + 1
}

fn slice(text: &str, start: u32, end: u32) -> String {
    text.get(start as usize..end as usize)
        .unwrap_or_default()
        .to_string()
}

fn first_line(text: &str) -> String {
    let mut lines = text.lines();
    let first = lines.next().unwrap_or_default().trim().to_string();
    if lines.next().is_some() {
        format!("{first} …")
    } else {
        first
    }
}

fn snippet(
    path: &str,
    file_text: &str,
    start: u32,
    end: u32,
    shown: Option<String>,
    canonical: &C,
) -> Snippet {
    let text = slice(file_text, start, end);
    let canonical = canonical.render();
    Snippet {
        path: path.to_string(),
        line: line_of(file_text, start),
        start,
        end,
        shown: shown.unwrap_or_else(|| first_line(&text)),
        hash: hash(&canonical),
        text_hash: hash(&without_comments(&text)),
        text,
        canonical,
    }
}

/// A field or parameter's canonical form: its name and declared type.
fn member(name: &str, ty: &TypeRef) -> C {
    C::Tag(
        "member",
        vec![C::Atom(name.to_string()), C::Atom(ty.to_string())],
    )
}

/// The end of a parameter written from `start`: the `,` or `)` closing it.
fn parameter_end(text: &str, start: u32) -> u32 {
    let mut depth = 0i32;
    for (i, c) in text[start as usize..].char_indices() {
        match c {
            '(' | '<' | '[' => depth += 1,
            ')' | '>' | ']' if depth == 0 => return start + i as u32,
            ')' | '>' | ']' => depth -= 1,
            ',' | '\n' if depth == 0 => return start + i as u32,
            _ => {}
        }
    }
    text.len() as u32
}

/// One callable body to read.
struct Read<'a> {
    path: &'a str,
    text: &'a str,
    body: &'a Body,
}

/// A field of a type or entity, as the graph has it.
struct Field<'a> {
    name: String,
    ty: &'a TypeRef,
    place: &'a jadpo_graph::Place,
}

/// Everything `jadpo suggest` finds in `project`, ignores not applied:
/// one finding per snippet, in file order, each the smallest that holds
/// its signal.
pub fn scan(db: &dyn Db, project: Project, taught: &[Taught]) -> Vec<Finding> {
    let graph = crate::graph::graph(db, project).clone();
    let readings = readings();
    let texts: BTreeMap<String, String> = project
        .files(db)
        .iter()
        .map(|file| (file.path(db).clone(), file.text(db).clone()))
        .collect();
    let taught: HashMap<&str, &Taught> = taught.iter().map(|t| (t.hash.as_str(), t)).collect();
    let mut found = Vec::new();

    declarations(db, project, &graph, &readings, &texts, &taught, &mut found);

    for &file in project.files(db) {
        let path = file.path(db);
        let text = file.text(db);
        for callable in callables_of(db, file) {
            let checked = typed_body(db, project, callable);
            if matches!(
                checked.key.as_ref().and_then(Key::kind),
                Some(Kind::Test | Kind::Fixture | Kind::Seed) | None
            ) {
                continue;
            }
            let read = Read {
                path,
                text,
                body: &checked.body,
            };
            body(&read, &graph, &readings, &taught, &mut found);
        }
    }
    settle(found)
}

/// One finding per snippet, the smallest of nested ones, in file order.
fn settle(mut found: Vec<Finding>) -> Vec<Finding> {
    // Two types firing on one snippet: more matched words win, then the
    // first entry.
    found.sort_by(|a, b| {
        (&a.snippet.path, a.snippet.start, a.snippet.end)
            .cmp(&(&b.snippet.path, b.snippet.start, b.snippet.end))
            .then(b.weight.cmp(&a.weight))
            .then(a.order.cmp(&b.order))
    });
    found.dedup_by(|later, kept| {
        later.snippet.path == kept.snippet.path
            && later.snippet.start == kept.snippet.start
            && later.snippet.end == kept.snippet.end
    });
    let inside = |inner: &Finding, outer: &Finding| {
        inner.snippet.path == outer.snippet.path
            && inner.signal == outer.signal
            && inner.suggestion == outer.suggestion
            && outer.snippet.start <= inner.snippet.start
            && inner.snippet.end <= outer.snippet.end
            && (outer.snippet.start, outer.snippet.end) != (inner.snippet.start, inner.snippet.end)
    };
    let kept: Vec<bool> = found
        .iter()
        .map(|outer| !found.iter().any(|inner| inside(inner, outer)))
        .collect();
    found
        .into_iter()
        .zip(kept)
        .filter_map(|(finding, keep)| keep.then_some(finding))
        .collect()
}

fn finding(
    snippet: Snippet,
    signal: Signal,
    reading: &Reading,
    weight: usize,
    reason: String,
) -> Finding {
    Finding {
        snippet,
        signal,
        suggestion: reading.entry.name.to_string(),
        reason,
        weight,
        order: reading.order,
    }
}

/// The `shape`, `name` and `validation` signals on declarations, and
/// taught snippets that are declaration members.
fn declarations(
    db: &dyn Db,
    project: Project,
    graph: &Graph,
    readings: &[Reading],
    texts: &BTreeMap<String, String>,
    taught: &HashMap<&str, &Taught>,
    found: &mut Vec<Finding>,
) {
    let declined: Vec<(String, String)> = crate::declines::declines(db, project)
        .into_iter()
        .filter(|decline| decline.rule == "type.standard_exists")
        .filter_map(|decline| Some((decline.path, decline.subject?)))
        .collect();
    let mut fields: BTreeMap<&Key, Vec<Field<'_>>> = BTreeMap::new();
    for node in graph.nodes() {
        match (&node.kind, &node.detail, &node.place) {
            (Kind::Field, Detail::Field { ty, .. }, Some(place)) => {
                if let Some(parent) = &node.parent {
                    fields.entry(parent).or_default().push(Field {
                        name: node
                            .name
                            .rsplit('.')
                            .next()
                            .unwrap_or(&node.name)
                            .to_string(),
                        ty,
                        place,
                    });
                }
                let Some(text) = texts.get(&place.file) else {
                    continue;
                };
                let name = node.name.rsplit('.').next().unwrap_or(&node.name);
                let canonical = member(name, ty);
                if let Some(builtin) = base_builtin(graph, ty) {
                    named(
                        readings,
                        name,
                        builtin,
                        type_name(ty),
                        || snippet(&place.file, text, place.start, place.end, None, &canonical),
                        found,
                    );
                }
                taught_at(
                    taught,
                    &canonical,
                    || snippet(&place.file, text, place.start, place.end, None, &canonical),
                    found,
                );
            }
            (
                Kind::Type,
                Detail::Type {
                    base: Base::Refines(inner),
                    rules,
                    ..
                },
                Some(place),
            ) => {
                let Some(text) = texts.get(&place.file) else {
                    continue;
                };
                let decl = slice(text, place.start, place.end);
                if let TypeRef::Builtin(builtin) = inner {
                    let canonical = C::Tag(
                        "type",
                        vec![
                            C::Atom(node.name.clone()),
                            C::Atom(builtin.name().to_string()),
                        ],
                    );
                    let end = decl
                        .find('{')
                        .map_or(place.end, |at| place.start + at as u32);
                    let end = place.start + slice(text, place.start, end).trim_end().len() as u32;
                    named(
                        readings,
                        &node.name,
                        *builtin,
                        None,
                        || snippet(&place.file, text, place.start, end, None, &canonical),
                        found,
                    );
                }
                if let Some(pattern) = &rules.pattern {
                    // The `pattern:` member inside the declaration.
                    let (start, end) = match decl.find("pattern:") {
                        Some(at) => {
                            let start = place.start + at as u32;
                            let line_end = decl[at..].find('\n').map_or(decl.len(), |n| at + n);
                            (start, place.start + line_end as u32)
                        }
                        None => (place.start, place.end),
                    };
                    let canonical = C::Tag("pattern", vec![C::Text(pattern.clone())]);
                    validation(
                        readings,
                        pattern,
                        || snippet(&place.file, text, start, end, None, &canonical),
                        found,
                    );
                }
            }
            _ => {}
        }
    }
    for (host, members) in &fields {
        let Some(node) = graph.get(host) else {
            continue;
        };
        let object = match (&node.kind, &node.detail) {
            (
                Kind::Type,
                Detail::Type {
                    base: Base::Object, ..
                },
            ) => true,
            (
                Kind::Type,
                Detail::Type {
                    base: Base::Input | Base::Output,
                    ..
                },
            )
            | (Kind::Entity, _) => false,
            _ => continue,
        };
        let host_name = node.name.clone();
        if let Some(place) = &node.place
            && declined
                .iter()
                .any(|(path, subject)| *path == place.file && *subject == host_name)
        {
            continue;
        }
        for reading in readings {
            let Some(shape) = reading.entry.shape else {
                continue;
            };
            let mut taken: Vec<&Field<'_>> = Vec::new();
            let mut missing = false;
            for wanted in shape {
                let hit = members.iter().find(|field| {
                    wanted.names.contains(&field.name.as_str())
                        && !matches!(field.ty, TypeRef::Nullable(_))
                        && (base_builtin(graph, field.ty)
                            .is_some_and(|b| wanted.built_on.contains(&b))
                            || (wanted.or_enum && is_enum(graph, field.ty)))
                        && !taken.iter().any(|t| t.name == field.name)
                });
                match hit {
                    Some(field) => taken.push(field),
                    None if wanted.optional => {}
                    None => {
                        missing = true;
                        break;
                    }
                }
            }
            // Exactly the shape on an `Object` is `type.standard_exists`,
            // which `jadpo check` reports.
            if missing || taken.is_empty() || (object && taken.len() == members.len()) {
                continue;
            }
            let path = &taken[0].place.file;
            let Some(text) = texts.get(path) else {
                continue;
            };
            let start = taken.iter().map(|f| f.place.start).min().unwrap_or(0);
            let end = taken.iter().map(|f| f.place.end).max().unwrap_or(0);
            let mut parts: Vec<String> = taken
                .iter()
                .map(|f| format!("{}:{}", f.name, f.ty))
                .collect();
            parts.sort();
            let canonical = C::Tag(
                "shape",
                std::iter::once(C::Atom(host_name.clone()))
                    .chain(parts.into_iter().map(C::Atom))
                    .collect(),
            );
            let names: Vec<String> = taken.iter().map(|f| f.name.clone()).collect();
            let mut snip = snippet(path, text, start, end, Some(names.join(", ")), &canonical);
            snip.text = taken
                .iter()
                .map(|f| slice(text, f.place.start, f.place.end))
                .collect::<Vec<_>>()
                .join("\n");
            snip.text_hash = hash(&without_comments(&snip.text));
            let reason = format!(
                "`{host_name}` holds {}, which together are a `{}`; the standard type has one construction, wire shape and storage shape.",
                crate::and_list(&names.iter().map(|n| format!("`{n}`")).collect::<Vec<_>>()),
                reading.entry.name
            );
            found.push(finding(snip, Signal::Shape, reading, taken.len(), reason));
        }
    }
}

/// The `name` signal on one field, parameter or type built on `builtin`.
fn named(
    readings: &[Reading],
    name: &str,
    builtin: Builtin,
    declared: Option<&str>,
    snip: impl Fn() -> Snippet,
    found: &mut Vec<Finding>,
) {
    let written = words(name);
    let shown = declared;
    let declared = declared.map(words).unwrap_or_default();
    for reading in readings {
        let on: &[Builtin] = reading.entry.signals.named_on;
        if !on.contains(&builtin) || reading.entry.holds == Holds::Builtin(builtin) {
            continue;
        }
        // A value of a type named like it already has the type's
        // finding, at the type.
        if matched(&declared, &reading.names) > 0 {
            continue;
        }
        let weight = matched(&written, &reading.names);
        if weight == 0 {
            continue;
        }
        let held = match shown {
            Some(declared) => format!("`{declared}`, built on `{}`", builtin.name()),
            None => format!("`{}`", builtin.name()),
        };
        let reason = format!(
            "`{name}` is named like a `{}` but holds {held}, so nothing checks its unit or meaning.",
            reading.entry.name
        );
        found.push(finding(snip(), Signal::Name, reading, weight, reason));
    }
}

/// The `validation` signal on one pattern.
fn validation(
    readings: &[Reading],
    pattern: &str,
    snip: impl Fn() -> Snippet,
    found: &mut Vec<Finding>,
) {
    let Ok(compiled) = jadpo_std::compile_pattern(pattern) else {
        return;
    };
    for reading in readings {
        let Some(wanted) = reading.entry.signals.validation else {
            continue;
        };
        if wanted.accepts.iter().all(|text| compiled.is_match(text))
            && wanted.refuses.iter().all(|text| !compiled.is_match(text))
        {
            let reason = format!(
                "This pattern checks a `{}` by hand; the standard type validates it, and its JSON and storage are its own.",
                reading.entry.name
            );
            found.push(finding(snip(), Signal::Validation, reading, 1, reason));
        }
    }
}

/// A taught snippet whose canonical form this is.
fn taught_at(
    taught: &HashMap<&str, &Taught>,
    canonical: &C,
    snip: impl Fn() -> Snippet,
    found: &mut Vec<Finding>,
) {
    if taught.is_empty() {
        return;
    }
    let rendered = canonical.render();
    let Some(t) = taught.get(hash(&rendered).as_str()) else {
        return;
    };
    if t.canonical != rendered {
        return;
    }
    found.push(Finding {
        snippet: snip(),
        signal: Signal::Taught,
        suggestion: t.suggestion.clone(),
        reason: if t.reason.is_empty() {
            "`jadpo suggest found` recorded this pattern.".to_string()
        } else {
            format!("`jadpo suggest found` recorded this pattern: {}", t.reason)
        },
        weight: 1,
        order: usize::MAX,
    });
}

/// The signals inside one body: `conversion`, `format`, `validation` on
/// `matches`, `loop`, `name` on parameters, and taught snippets.
fn body(
    read: &Read<'_>,
    graph: &Graph,
    readings: &[Reading],
    taught: &HashMap<&str, &Taught>,
    found: &mut Vec<Finding>,
) {
    let body = read.body;
    let mut canon = Canon::new(body, graph);
    for &param in &body.params {
        let local = body.local(param);
        if local.kind != LocalKind::Param {
            continue;
        }
        let end = parameter_end(read.text, local.start);
        let canonical = member(&local.name, &local.ty);
        let snip = || snippet(read.path, read.text, local.start, end, None, &canonical);
        if let Some(builtin) = base_builtin(graph, &local.ty) {
            named(
                readings,
                &local.name,
                builtin,
                type_name(&local.ty),
                snip,
                found,
            );
        }
        taught_at(taught, &canonical, snip, found);
    }
    for (index, site) in body.sites.iter().enumerate() {
        let id = SiteId(index as u32);
        let shape = canon.site(id);
        let snip =
            |canonical: &C| snippet(read.path, read.text, site.start, site.end, None, canonical);
        // A variable's use is its value's snippet, not one of its own.
        if !matches!(site.node, Node::Expr(Expr::Local(_))) {
            taught_at(taught, &shape, || snip(&shape), found);
        }
        match &site.node {
            Node::Expr(Expr::Binary {
                op: BinaryOp::Multiply | BinaryOp::Divide,
                lhs,
                rhs,
            }) => {
                let types = [&site.ty, &body.site(*lhs).ty, &body.site(*rhs).ty];
                conversion(graph, readings, &types, &shape, || snip(&shape), found);
            }
            Node::Expr(Expr::Binary {
                op: BinaryOp::Add, ..
            }) => format(readings, &shape, || snip(&shape), found),
            Node::Expr(Expr::Binary {
                op: BinaryOp::Matches,
                ..
            }) => {
                if let C::Op(_, operands) = &shape
                    && let Some(C::Text(pattern)) = operands.get(1)
                {
                    validation(readings, pattern, || snip(&shape), found);
                }
            }
            Node::Stmt(Stmt::For {
                item: Some(item),
                body: inner,
                ..
            }) => {
                let item = body.local(*item).name.clone();
                if let [only] = inner.as_slice() {
                    let only = canon.site(*only);
                    if let Some(operation) = loop_operation(&item, &only) {
                        for signal in LOOP_SIGNALS {
                            if signal.shape == operation {
                                found.push(Finding {
                                    snippet: snip(&shape),
                                    signal: Signal::Loop,
                                    suggestion: signal.operation.to_string(),
                                    reason: format!(
                                        "This loop is {}, which `{}` does in one call.",
                                        signal.described, signal.operation
                                    ),
                                    weight: 1,
                                    order: usize::MAX,
                                });
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

/// The `conversion` signal on one multiplication or division.
fn conversion(
    graph: &Graph,
    readings: &[Reading],
    types: &[&TypeRef],
    shape: &C,
    snip: impl Fn() -> Snippet,
    found: &mut Vec<Finding>,
) {
    let C::Op("*" | "/", operands) = shape else {
        return;
    };
    let numbers: Vec<Num> = operands
        .iter()
        .filter_map(|operand| match operand {
            C::Num(num) => Some(*num),
            _ => None,
        })
        .collect();
    let mut names = Vec::new();
    for operand in operands.iter().filter(|o| !o.is_literal()) {
        operand.names(&mut names);
    }
    let written: Vec<String> = names.iter().flat_map(|name| words(name)).collect();
    for reading in readings {
        if types.iter().any(|ty| is_standard(graph, ty, reading.entry)) {
            continue;
        }
        let Some(by) = numbers.iter().find(|n| reading.conversions.contains(n)) else {
            continue;
        };
        let weight = matched(&written, &reading.names);
        if weight == 0 {
            continue;
        }
        let reason = format!(
            "Multiplying or dividing a value named like a `{}` by {by} converts it by hand; `{}` makes the conversion itself.",
            reading.entry.name, reading.entry.name
        );
        found.push(finding(snip(), Signal::Conversion, reading, weight, reason));
    }
}

/// The `format` signal on one `+`.
fn format(readings: &[Reading], shape: &C, snip: impl Fn() -> Snippet, found: &mut Vec<Finding>) {
    let C::Op("+", operands) = shape else { return };
    if operands.iter().all(C::is_literal) {
        return;
    }
    for reading in readings {
        let hit = operands.iter().find_map(|operand| match operand {
            C::Text(text) => reading
                .format_literals
                .iter()
                .find(|token| holds_token(text, token)),
            _ => None,
        });
        if let Some(token) = hit {
            let reason = format!(
                "Joining `{token}` to a value formats a `{}` by hand; `text.format` shows one for the reader's locale.",
                reading.entry.name
            );
            found.push(finding(snip(), Signal::Format, reading, 1, reason));
        }
    }
}

/// `c` without the types `as` gives it.
fn bare(c: &C) -> &C {
    match c {
        C::Tag("as", parts) if parts.len() == 2 => bare(&parts[1]),
        other => other,
    }
}

/// The loop shape a `for` loop whose only statement is `only` has.
fn loop_operation(item: &str, only: &C) -> Option<LoopShape> {
    let mentions = |c: &C| {
        let mut names = Vec::new();
        c.names(&mut names);
        names.first().is_some_and(|first| first == item)
            || matches!(c, C::Name(path) if path.first().map(String::as_str) == Some(item))
    };
    match only {
        // `total = total + item.price`
        C::Tag("set", parts) => {
            let [C::Name(target), value] = parts.as_slice() else {
                return None;
            };
            let C::Op("+", operands) = bare(value) else {
                return None;
            };
            let has_target = operands.iter().any(|o| o == &C::Name(target.clone()));
            let has_item = operands
                .iter()
                .any(|o| o != &C::Name(target.clone()) && mentions(o));
            (has_target && has_item).then_some(LoopShape::Accumulate)
        }
        // `if item.price < best.price { best = item }`
        C::Tag("if", parts) => {
            let [C::Op("<" | "<=", sides), C::Tag("block", then)] = parts.as_slice() else {
                return None;
            };
            let [C::Tag("set", set)] = then.as_slice() else {
                return None;
            };
            let [C::Name(kept), C::Name(value)] = set.as_slice() else {
                return None;
            };
            if value.as_slice() != [item.to_string()] {
                return None;
            }
            let starts =
                |c: &C, name: &[String]| matches!(c, C::Name(path) if path.first() == name.first());
            match sides.as_slice() {
                [smaller, larger] if starts(smaller, value) && starts(larger, kept) => {
                    Some(LoopShape::KeepSmallest)
                }
                [smaller, larger] if starts(smaller, kept) && starts(larger, value) => {
                    Some(LoopShape::KeepLargest)
                }
                _ => None,
            }
        }
        _ => None,
    }
}

/// The snippet `jadpo suggest found` records at `line` of `path`: the
/// value of the first statement that starts on it (or the statement), or
/// else the field or parameter declared there.
pub fn snippet_at(db: &dyn Db, project: Project, path: &str, line: u32) -> Option<Snippet> {
    let file = project
        .files(db)
        .iter()
        .find(|file| file.path(db) == path)?;
    let text = file.text(db);
    let graph = crate::graph::graph(db, project).clone();
    for callable in callables_of(db, file.to_owned()) {
        let checked = typed_body(db, project, callable);
        let body = &checked.body;
        let mut canon = Canon::new(body, &graph);
        for (index, site) in body.sites.iter().enumerate() {
            let Node::Stmt(stmt) = &site.node else {
                continue;
            };
            if site.line != line {
                continue;
            }
            let value = match stmt {
                Stmt::Var { value, .. }
                | Stmt::Assign { value, .. }
                | Stmt::Return { value }
                | Stmt::Emit { value }
                | Stmt::Expr { value } => *value,
                _ => None,
            };
            let mut id = value.unwrap_or(SiteId(index as u32));
            // `Pence(x)` only gives `x` a type's meaning: the snippet is `x`.
            if let Node::Expr(Expr::Call {
                callee: jadpo_graph::body::Callee::Construct(_),
                args,
                ..
            }) = &body.site(id).node
                && let [Some(inner)] = args.as_slice()
            {
                id = *inner;
            }
            let at = body.site(id);
            let canonical = canon.site(id);
            return Some(snippet(path, text, at.start, at.end, None, &canonical));
        }
        for &param in &body.params {
            let local = body.local(param);
            if line_of(text, local.start) == line {
                let end = parameter_end(text, local.start);
                return Some(snippet(
                    path,
                    text,
                    local.start,
                    end,
                    None,
                    &member(&local.name, &local.ty),
                ));
            }
        }
    }
    for node in graph.nodes() {
        if let (Kind::Field, Detail::Field { ty, .. }, Some(place)) =
            (&node.kind, &node.detail, &node.place)
            && place.file == path
            && place.lines.0 == line
        {
            let name = node.name.rsplit('.').next().unwrap_or(&node.name);
            return Some(snippet(
                path,
                text,
                place.start,
                place.end,
                None,
                &member(name, ty),
            ));
        }
    }
    None
}

/// One line of the catalogue: a signal of one standard type or operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub suggestion: String,
    pub signal: Signal,
    /// What it looks for, in words.
    pub looks_for: String,
    /// The same as data: the words, constants, texts or fields.
    pub values: Vec<String>,
}

/// Every signal, generated from the entries and the loop table.
pub fn catalogue() -> Vec<Entry> {
    let mut out = Vec::new();
    for reading in readings() {
        let entry = reading.entry;
        let name = entry.name.to_string();
        if let Some(shape) = entry.shape {
            let values: Vec<String> = shape.iter().map(|f| f.names[0].to_string()).collect();
            out.push(Entry {
                suggestion: name.clone(),
                signal: Signal::Shape,
                looks_for: format!("a type or entity holding {}", values.join(", ")),
                values,
            });
        }
        let names = reading.names.join(", ");
        if !reading.conversions.is_empty() && !reading.names.is_empty() {
            let values: Vec<String> = reading.conversions.iter().map(Num::to_string).collect();
            out.push(Entry {
                suggestion: name.clone(),
                signal: Signal::Conversion,
                looks_for: format!("× or ÷ {} on a name with {names}", values.join(", ")),
                values,
            });
        }
        if !reading.names.is_empty() {
            let on: Vec<&str> = entry.signals.named_on.iter().map(|b| b.name()).collect();
            out.push(Entry {
                suggestion: name.clone(),
                signal: Signal::Name,
                looks_for: format!("a {} named with {names}", on.join(" or ")),
                values: reading.names.clone(),
            });
        }
        if !reading.format_literals.is_empty() {
            out.push(Entry {
                suggestion: name.clone(),
                signal: Signal::Format,
                looks_for: format!(
                    "{} in text joined to a value",
                    reading.format_literals.join(", ")
                ),
                values: reading.format_literals.clone(),
            });
        }
        if let Some(validation) = entry.signals.validation {
            out.push(Entry {
                suggestion: name.clone(),
                signal: Signal::Validation,
                looks_for: format!("a pattern accepting {}", validation.accepts.join(", ")),
                values: validation.accepts.iter().map(|t| t.to_string()).collect(),
            });
        }
    }
    for signal in LOOP_SIGNALS {
        out.push(Entry {
            suggestion: signal.operation.to_string(),
            signal: Signal::Loop,
            looks_for: signal.described.to_string(),
            values: Vec::new(),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comments_are_left_out_of_the_shared_text() {
        assert_eq!(
            without_comments("total * 100 // pence\n// note\n\"a // b\""),
            "total * 100\n\"a // b\""
        );
    }

    #[test]
    fn a_token_needs_no_letter_beside_it() {
        assert!(holds_token("£", "£"));
        assert!(holds_token("5 km", "km"));
        assert!(!holds_token("smoke", "km"));
    }

    #[test]
    fn a_factor_between_units_is_exact_or_none() {
        assert_eq!(exact(1000, 1), Some(Num::whole(1000)));
        assert_eq!(exact(254, 100).map(|n| n.to_string()), Some("2.54".into()));
        assert_eq!(exact(1, 3), None);
    }
}
