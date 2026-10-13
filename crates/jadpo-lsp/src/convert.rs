//! Between the protocol's terms and the compiler's: positions as lines and
//! columns in the encoding the editor chose, `file:` URIs as paths, and
//! diagnostics in the one shape every error has (area 14).

use std::path::{Path, PathBuf};
use std::str::FromStr;

use jadpo_diag::Severity;
use lsp_types::{
    DiagnosticRelatedInformation, DiagnosticSeverity, Location, NumberOrString, Position, Range,
    Uri,
};

/// How columns are counted: the protocol's default, UTF-16 code units, or
/// bytes when the editor offers UTF-8 (LSP 3.17).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Encoding {
    Utf8,
    Utf16,
}

/// The position of byte `offset` in `text`.
pub fn position(text: &str, offset: u32, encoding: Encoding) -> Position {
    let offset = (offset as usize).min(text.len());
    let before = &text[..floor(text, offset)];
    let line = before.matches('\n').count() as u32;
    let start = before.rfind('\n').map_or(0, |i| i + 1);
    let column = &before[start..];
    let character = match encoding {
        Encoding::Utf8 => column.len(),
        Encoding::Utf16 => column.encode_utf16().count(),
    } as u32;
    Position { line, character }
}

/// The byte offset of `position` in `text`; a column past the end of its
/// line is the line's end.
pub fn offset(text: &str, position: Position, encoding: Encoding) -> Option<u32> {
    let mut start = 0;
    for _ in 0..position.line {
        start += text[start..].find('\n')? + 1;
    }
    let line_end = text[start..].find('\n').map_or(text.len(), |i| start + i);
    let line = &text[start..line_end];
    let mut units = 0;
    for (i, c) in line.char_indices() {
        if units >= position.character as usize {
            return Some((start + i) as u32);
        }
        units += match encoding {
            Encoding::Utf8 => c.len_utf8(),
            Encoding::Utf16 => c.len_utf16(),
        };
    }
    Some(line_end as u32)
}

/// The nearest character boundary at or before `offset`.
fn floor(text: &str, mut offset: usize) -> usize {
    while offset > 0 && !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

pub fn range(text: &str, start: u32, end: u32, encoding: Encoding) -> Range {
    Range {
        start: position(text, start, encoding),
        end: position(text, end, encoding),
    }
}

/// The path a `file:` URI names.
pub fn path_of(uri: &Uri) -> Option<PathBuf> {
    let text = uri.as_str();
    let rest = text.strip_prefix("file://")?;
    // `file:///path` on Unix, `file://localhost/path` too.
    let rest = rest.strip_prefix("localhost").unwrap_or(rest);
    let decoded = percent_decode(rest)?;
    let path = PathBuf::from(decoded);
    // The editor names files by the path it opened; the project is loaded
    // through symbolic links resolved, so compare in that form.
    Some(match path.canonicalize() {
        Ok(real) => real,
        Err(_) => match (path.parent(), path.file_name()) {
            (Some(parent), Some(name)) => parent
                .canonicalize()
                .map_or_else(|_| path.clone(), |real| real.join(name)),
            _ => path,
        },
    })
}

/// The `file:` URI of an absolute path.
pub fn uri_of(path: &Path) -> Option<Uri> {
    let mut text = String::from("file://");
    for byte in path.to_str()?.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                text.push(byte as char)
            }
            other => text.push_str(&format!("%{other:02X}")),
        }
    }
    Uri::from_str(&text).ok()
}

fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = std::str::from_utf8(bytes.get(i + 1..i + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// A diagnostic as the editor shows it: what is wrong, why, the next step
/// and who takes it, with the rule as its code. Its other spans become
/// related information.
pub fn diagnostic(
    diagnostic: &jadpo_diag::Diagnostic,
    text: &str,
    root: &Path,
    source_of: &dyn Fn(&str) -> Option<String>,
    encoding: Encoding,
) -> lsp_types::Diagnostic {
    let span = diagnostic.primary.as_ref().map(|label| &label.span);
    let mut message = diagnostic.message.clone();
    if let Some(reason) = &diagnostic.reason {
        message.push('\n');
        message.push_str(reason);
    }
    if let Some(next) = &diagnostic.next {
        message.push_str(&format!("\nnext: {} ({})", next.text, next.decides.word()));
    }
    let related: Vec<DiagnosticRelatedInformation> = diagnostic
        .secondary
        .iter()
        .filter_map(|label| {
            let other = source_of(&label.span.path)?;
            Some(DiagnosticRelatedInformation {
                location: Location {
                    uri: uri_of(&root.join(&label.span.path))?,
                    range: range(&other, label.span.start, label.span.end, encoding),
                },
                message: label.text.clone(),
            })
        })
        .collect();
    lsp_types::Diagnostic {
        range: span.map_or_else(Range::default, |span| {
            range(text, span.start, span.end, encoding)
        }),
        severity: Some(match diagnostic.severity() {
            Severity::Error => DiagnosticSeverity::ERROR,
            Severity::Warning => DiagnosticSeverity::WARNING,
        }),
        code: Some(NumberOrString::String(diagnostic.rule.id.to_string())),
        source: Some("jadpo".to_string()),
        message,
        related_information: (!related.is_empty()).then_some(related),
        ..lsp_types::Diagnostic::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn columns_count_in_the_chosen_encoding() {
        let text = "ab\nµ→x\n";
        let at = |offset, encoding| position(text, offset, encoding);
        assert_eq!(at(1, Encoding::Utf16), Position::new(0, 1));
        assert_eq!(at(3, Encoding::Utf16), Position::new(1, 0));
        // `µ` is 2 bytes and 1 unit, `→` is 3 bytes and 1 unit.
        assert_eq!(at(8, Encoding::Utf16), Position::new(1, 2));
        assert_eq!(at(8, Encoding::Utf8), Position::new(1, 5));
        for encoding in [Encoding::Utf8, Encoding::Utf16] {
            for offset in [0, 1, 3, 5, 8, 9] {
                assert_eq!(
                    super::offset(text, at(offset, encoding), encoding),
                    Some(offset),
                    "{offset} {encoding:?}"
                );
            }
        }
        assert_eq!(
            super::offset(text, Position::new(0, 99), Encoding::Utf16),
            Some(2)
        );
    }

    #[test]
    fn file_uris_round_trip() {
        let path = Path::new("/projects/my notes/app.jadpo");
        let uri = uri_of(path).expect("uri");
        assert_eq!(uri.as_str(), "file:///projects/my%20notes/app.jadpo");
        assert_eq!(path_of(&uri), Some(path.to_path_buf()));
    }
}
