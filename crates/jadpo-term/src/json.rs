//! JSON documents for `--json`: a small value type that keeps keys in the
//! order they are written, and a writer with two-space indent and LF line
//! ends, so the same data always prints the same bytes.

use std::fmt::Write as _;

use jadpo_diag::{Diagnostic, Label, Severity, Span};

use crate::sources::{Position, Sources};

/// Raised when a document's shape changes in a way readers must notice.
pub const SCHEMA_VERSION: i64 = 2;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Json {
    Null,
    Bool(bool),
    Int(i64),
    Str(String),
    Array(Vec<Json>),
    Object(Vec<(String, Json)>),
}

impl Json {
    pub fn object<K: Into<String>>(entries: impl IntoIterator<Item = (K, Json)>) -> Json {
        Json::Object(
            entries
                .into_iter()
                .map(|(key, value)| (key.into(), value))
                .collect(),
        )
    }

    pub fn array<T: Into<Json>>(items: impl IntoIterator<Item = T>) -> Json {
        Json::Array(items.into_iter().map(Into::into).collect())
    }

    /// The document as printed: indented, newline-terminated.
    pub fn to_document(&self) -> String {
        let mut out = String::new();
        self.write(&mut out, 0);
        out.push('\n');
        out
    }

    /// The document on one line, newline-terminated: one object per line
    /// for streaming commands (CLI design language, "Plain and `--json`").
    pub fn to_line(&self) -> String {
        let mut out = String::new();
        self.write_line(&mut out);
        out.push('\n');
        out
    }

    fn write_line(&self, out: &mut String) {
        match self {
            Json::Array(items) => {
                out.push('[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    item.write_line(out);
                }
                out.push(']');
            }
            Json::Object(entries) => {
                out.push('{');
                for (i, (key, value)) in entries.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    write_string(out, key);
                    out.push(':');
                    value.write_line(out);
                }
                out.push('}');
            }
            scalar => scalar.write(out, 0),
        }
    }

    fn write(&self, out: &mut String, indent: usize) {
        match self {
            Json::Null => out.push_str("null"),
            Json::Bool(value) => out.push_str(if *value { "true" } else { "false" }),
            Json::Int(value) => {
                let _ = write!(out, "{value}");
            }
            Json::Str(value) => write_string(out, value),
            Json::Array(items) if items.is_empty() => out.push_str("[]"),
            Json::Object(entries) if entries.is_empty() => out.push_str("{}"),
            Json::Array(items) => {
                out.push('[');
                for (i, item) in items.iter().enumerate() {
                    out.push_str(if i == 0 { "\n" } else { ",\n" });
                    pad(out, indent + 1);
                    item.write(out, indent + 1);
                }
                out.push('\n');
                pad(out, indent);
                out.push(']');
            }
            Json::Object(entries) => {
                out.push('{');
                for (i, (key, value)) in entries.iter().enumerate() {
                    out.push_str(if i == 0 { "\n" } else { ",\n" });
                    pad(out, indent + 1);
                    write_string(out, key);
                    out.push_str(": ");
                    value.write(out, indent + 1);
                }
                out.push('\n');
                pad(out, indent);
                out.push('}');
            }
        }
    }
}

fn pad(out: &mut String, indent: usize) {
    for _ in 0..indent {
        out.push_str("  ");
    }
}

fn write_string(out: &mut String, value: &str) {
    out.push('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 || c == '\u{7f}' => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

impl From<&str> for Json {
    fn from(value: &str) -> Json {
        Json::Str(value.to_string())
    }
}

impl From<String> for Json {
    fn from(value: String) -> Json {
        Json::Str(value)
    }
}

impl From<&String> for Json {
    fn from(value: &String) -> Json {
        Json::Str(value.clone())
    }
}

impl From<bool> for Json {
    fn from(value: bool) -> Json {
        Json::Bool(value)
    }
}

impl From<i64> for Json {
    fn from(value: i64) -> Json {
        Json::Int(value)
    }
}

impl From<i32> for Json {
    fn from(value: i32) -> Json {
        Json::Int(i64::from(value))
    }
}

impl From<u32> for Json {
    fn from(value: u32) -> Json {
        Json::Int(i64::from(value))
    }
}

impl From<usize> for Json {
    fn from(value: usize) -> Json {
        Json::Int(i64::try_from(value).unwrap_or(i64::MAX))
    }
}

impl<T: Into<Json>> From<Option<T>> for Json {
    fn from(value: Option<T>) -> Json {
        value.map_or(Json::Null, Into::into)
    }
}

impl From<Vec<Json>> for Json {
    fn from(value: Vec<Json>) -> Json {
        Json::Array(value)
    }
}

/// The document every `--json` command prints: `schemaVersion`, the
/// canonical command name, whether it succeeded, its data, and its
/// diagnostics. A command fails when any diagnostic is an error.
pub fn envelope(
    command: Option<&str>,
    data: Option<Json>,
    diagnostics: &[Diagnostic],
    sources: &dyn Sources,
) -> Json {
    let ok = diagnostics
        .iter()
        .all(|diagnostic| diagnostic.severity() != Severity::Error);
    Json::object([
        ("schemaVersion", Json::Int(SCHEMA_VERSION)),
        ("command", command.into()),
        ("ok", ok.into()),
        ("data", data.unwrap_or(Json::Null)),
        (
            "diagnostics",
            Json::array(
                diagnostics
                    .iter()
                    .map(|diagnostic| diagnostic_json(diagnostic, sources)),
            ),
        ),
    ])
}

/// One diagnostic as data, with line and column beside byte offsets.
pub fn diagnostic_json(diagnostic: &Diagnostic, sources: &dyn Sources) -> Json {
    let next = diagnostic.next.as_ref().map(|next| {
        Json::object([
            ("text", Json::from(&next.text)),
            ("decides", next.decides.word().into()),
            (
                "edits",
                Json::array(next.edits.iter().map(|edit| {
                    let mut entries = span_entries(&edit.span, sources);
                    entries.push(("replacement".into(), Json::from(&edit.replacement)));
                    Json::Object(entries)
                })),
            ),
        ])
    });
    Json::object([
        ("rule", diagnostic.rule.id.into()),
        ("severity", diagnostic.severity().word().into()),
        ("message", Json::from(&diagnostic.message)),
        (
            "location",
            diagnostic
                .primary
                .as_ref()
                .map(|label| label_json(label, sources))
                .into(),
        ),
        (
            "related",
            Json::array(
                diagnostic
                    .secondary
                    .iter()
                    .map(|label| label_json(label, sources)),
            ),
        ),
        ("reason", diagnostic.reason.as_ref().into()),
        ("next", next.into()),
        (
            "inUse",
            Json::array(diagnostic.in_use.iter().map(|(name, path)| {
                Json::object([("name", Json::from(name)), ("path", Json::from(path))])
            })),
        ),
        ("impact", diagnostic.impact.as_ref().into()),
        ("help", diagnostic.help().into()),
    ])
}

fn label_json(label: &Label, sources: &dyn Sources) -> Json {
    let mut entries = span_entries(&label.span, sources);
    entries.push(("label".into(), Json::from(&label.text)));
    Json::Object(entries)
}

fn span_entries(span: &Span, sources: &dyn Sources) -> Vec<(String, Json)> {
    let text = sources.source(&span.path);
    let start = text.map(|text| Position::of(text, span.start));
    let end = text.map(|text| Position::of(text, span.end));
    vec![
        ("path".into(), Json::from(&span.path)),
        ("start".into(), span.start.into()),
        ("end".into(), span.end.into()),
        ("line".into(), start.map(|p| p.line).into()),
        ("column".into(), start.map(|p| p.column).into()),
        ("endLine".into(), end.map(|p| p.line).into()),
        ("endColumn".into(), end.map(|p| p.column).into()),
    ]
}
