//! Records in the editor (area 18, "Records in git and in your editor"):
//! a record's outline, a fold for each entry and a link on each file a
//! fact names. A record is read line by line, as the record format allows
//! (a line's kind is known from its first characters), so a record edited
//! by hand, whose errors the diagnostics show, still has its outline.

use lsp_types::{DocumentSymbol, FoldingRange, SymbolKind};

use crate::convert::{self, Encoding};

/// One line of a record: where it starts and ends, its indent and what
/// follows the indent.
struct Line<'a> {
    number: u32,
    start: u32,
    end: u32,
    indent: usize,
    content: &'a str,
}

fn lines(text: &str) -> Vec<Line<'_>> {
    let mut out = Vec::new();
    let mut start = 0usize;
    for (number, line) in text.split_inclusive('\n').enumerate() {
        let body = line.trim_end_matches('\n').trim_end_matches('\r');
        let indent = body.len() - body.trim_start_matches(' ').len();
        out.push(Line {
            number: number as u32,
            start: start as u32,
            end: (start + body.len()) as u32,
            indent,
            content: &body[indent..],
        });
        start += line.len();
    }
    out
}

/// An entry: its heading line and the lines that belong to it, each
/// member with its own fields.
struct Outline<'a> {
    heading: &'a Line<'a>,
    members: Vec<(&'a Line<'a>, Vec<&'a Line<'a>>)>,
}

/// The entries after the header's two lines. A comment is no entry.
fn outline<'a>(lines: &'a [Line<'a>]) -> Vec<Outline<'a>> {
    let mut out: Vec<Outline<'a>> = Vec::new();
    for line in lines.iter().skip(2) {
        if line.content.is_empty() || (line.indent == 0 && line.content.starts_with("//")) {
            continue;
        }
        match line.indent {
            0 => out.push(Outline {
                heading: line,
                members: Vec::new(),
            }),
            4 => {
                if let Some(entry) = out.last_mut() {
                    entry.members.push((line, Vec::new()));
                }
            }
            _ => {
                if let Some((_, nested)) = out.last_mut().and_then(|e| e.members.last_mut()) {
                    nested.push(line);
                }
            }
        }
    }
    out
}

/// The kind of symbol an entry's heading word makes.
fn kind_of(word: &str) -> SymbolKind {
    match word {
        "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD" | "OPTIONS" | "operation"
        | "action" | "function" | "poll" | "query" | "recipe" => SymbolKind::FUNCTION,
        "input" | "output" | "object" | "entity" | "type" => SymbolKind::STRUCT,
        "enum" => SymbolKind::ENUM,
        "event" | "message" => SymbolKind::EVENT,
        "failure" => SymbolKind::OBJECT,
        "field" => SymbolKind::FIELD,
        "connection" | "provides" => SymbolKind::INTERFACE,
        "api" | "package" | "provider" | "jadpo" => SymbolKind::PACKAGE,
        _ => SymbolKind::KEY,
    }
}

/// A field line's name and value, `name: value`.
fn field(content: &str) -> Option<(&str, &str)> {
    let (name, value) = content.split_once(": ")?;
    (!name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_'))
    .then_some((name, value))
}

#[allow(deprecated, clippy::too_many_arguments)]
fn symbol(
    text: &str,
    name: String,
    detail: Option<String>,
    kind: SymbolKind,
    (from, to): (&Line<'_>, &Line<'_>),
    selection: &Line<'_>,
    children: Vec<DocumentSymbol>,
    encoding: Encoding,
) -> DocumentSymbol {
    DocumentSymbol {
        name,
        detail,
        kind,
        tags: None,
        deprecated: None,
        range: convert::range(text, from.start, to.end, encoding),
        selection_range: convert::range(text, selection.start, selection.end, encoding),
        children: (!children.is_empty()).then_some(children),
    }
}

/// A field of a member or entry as a symbol.
fn field_symbol(text: &str, line: &Line<'_>, encoding: Encoding) -> Option<DocumentSymbol> {
    let (name, value) = field(line.content)?;
    Some(symbol(
        text,
        name.to_string(),
        Some(value.to_string()),
        SymbolKind::FIELD,
        (line, line),
        line,
        Vec::new(),
        encoding,
    ))
}

/// The record's outline: each entry by its heading, with its fields and
/// enum variants beneath it; other facts are left to the text.
pub fn symbols(text: &str, encoding: Encoding) -> Vec<DocumentSymbol> {
    let lines = lines(text);
    outline(&lines)
        .iter()
        .map(|entry| {
            let mut children = Vec::new();
            for (member, nested) in &entry.members {
                if let Some(found) = field_symbol(text, member, encoding) {
                    children.push(found);
                } else if let Some(variant) = member.content.strip_prefix("variant ") {
                    let last = nested.last().copied().unwrap_or(member);
                    children.push(symbol(
                        text,
                        variant.to_string(),
                        None,
                        SymbolKind::ENUM_MEMBER,
                        (member, last),
                        member,
                        nested
                            .iter()
                            .filter_map(|line| field_symbol(text, line, encoding))
                            .collect(),
                        encoding,
                    ));
                }
            }
            let last = entry
                .members
                .last()
                .map(|(member, nested)| nested.last().copied().unwrap_or(member))
                .unwrap_or(entry.heading);
            let word = entry.heading.content.split(' ').next().unwrap_or("");
            symbol(
                text,
                entry.heading.content.to_string(),
                None,
                kind_of(word),
                (entry.heading, last),
                entry.heading,
                children,
                encoding,
            )
        })
        .collect()
}

/// A fold for each entry with members, and for each member with fields of
/// its own.
pub fn folds(text: &str) -> Vec<FoldingRange> {
    let lines = lines(text);
    let fold = |from: u32, to: u32| FoldingRange {
        start_line: from,
        start_character: None,
        end_line: to,
        end_character: None,
        kind: None,
        collapsed_text: None,
    };
    let mut out = Vec::new();
    for entry in outline(&lines) {
        let Some((member, nested)) = entry.members.last() else {
            continue;
        };
        let last = nested.last().copied().unwrap_or(member);
        out.push(fold(entry.heading.number, last.number));
        for (member, nested) in &entry.members {
            if let Some(last) = nested.last() {
                out.push(fold(member.number, last.number));
            }
        }
    }
    out
}

/// Each `file <path>` fact: the path's place in `text` and the path,
/// relative to the record's folder as the record format writes it.
pub fn files(text: &str) -> Vec<(u32, u32, &str)> {
    lines(text)
        .into_iter()
        .filter(|line| line.indent == 4)
        .filter_map(|line| {
            let path = line.content.strip_prefix("file ")?;
            (!path.is_empty() && !path.contains(' ')).then(|| {
                let start = line.end - path.len() as u32;
                (start, line.end, path)
            })
        })
        .collect()
}

/// `path` joined to the folder of the record at `record`, with `..` and
/// `.` taken out; `None` when it leaves the project.
pub fn joined(record: &str, path: &str) -> Option<String> {
    let mut parts: Vec<&str> = record.split('/').collect();
    parts.pop();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            part => parts.push(part),
        }
    }
    Some(parts.join("/"))
}
