//! Source text for code frames and line and column numbers.

use std::collections::{BTreeMap, HashMap};

/// Where renderers find a file's text by its project-relative path.
pub trait Sources {
    fn source(&self, path: &str) -> Option<&str>;
}

/// No sources: diagnostics print without frames or line numbers.
impl Sources for () {
    fn source(&self, _path: &str) -> Option<&str> {
        None
    }
}

impl Sources for BTreeMap<String, String> {
    fn source(&self, path: &str) -> Option<&str> {
        self.get(path).map(String::as_str)
    }
}

impl Sources for HashMap<String, String> {
    fn source(&self, path: &str) -> Option<&str> {
        self.get(path).map(String::as_str)
    }
}

/// A 1-based line and column. Columns count characters, not bytes, so they
/// match what an editor shows for any text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Position {
    pub line: usize,
    pub column: usize,
}

impl Position {
    pub fn of(text: &str, offset: u32) -> Position {
        let offset = floor_boundary(text, offset as usize);
        let before = &text[..offset];
        let line_start = before.rfind('\n').map_or(0, |i| i + 1);
        Position {
            line: before.matches('\n').count() + 1,
            column: before[line_start..].chars().count() + 1,
        }
    }
}

fn floor_boundary(text: &str, offset: usize) -> usize {
    let mut offset = offset.min(text.len());
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

/// The text of a 1-based line, without its line end.
pub fn line_text(text: &str, line: usize) -> &str {
    text.split('\n')
        .nth(line.saturating_sub(1))
        .unwrap_or("")
        .trim_end_matches('\r')
}
