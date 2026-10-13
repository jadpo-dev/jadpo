//! Reading a node's place, description and canonical form from the tree.

use jadpo_graph::{Digest, Place};
use jadpo_syntax::{SyntaxKind, SyntaxNode};

use crate::tree::node_span;

/// Where each line of a text starts, for turning byte offsets into lines.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct LineIndex {
    starts: Vec<u32>,
}

impl LineIndex {
    pub fn new(text: &str) -> LineIndex {
        let mut starts = vec![0];
        for (i, byte) in text.bytes().enumerate() {
            if byte == b'\n' {
                starts.push(i as u32 + 1);
            }
        }
        LineIndex { starts }
    }

    /// The 1-based line holding byte `offset`.
    pub fn line(&self, offset: u32) -> u32 {
        self.starts.partition_point(|&start| start <= offset) as u32
    }

    /// The byte where 0-based line `line` starts.
    pub fn line_start(&self, line: u32) -> Option<u32> {
        self.starts.get(line as usize).copied()
    }
}

/// The place of `node` in `path`, without the comments kept beside it.
pub fn place(path: &str, lines: &LineIndex, node: &SyntaxNode) -> Place {
    let span = node_span(path, node);
    // The last line is the one holding the last byte, not the one after.
    let last = span.end.saturating_sub(1).max(span.start);
    Place {
        file: path.to_string(),
        start: span.start,
        end: span.end,
        lines: (lines.line(span.start), lines.line(last)),
    }
}

/// The digest of `node`'s canonical form.
pub fn digest(node: &SyntaxNode) -> Digest {
    let tokens = jadpo_fmt::canonical_tokens(node);
    Digest::of(tokens.iter().map(|token| token.text()))
}

/// The `///` lines written just before `node`, without their markers.
pub fn doc(node: &SyntaxNode) -> Option<String> {
    let mut lines = Vec::new();
    for element in node.descendants_with_tokens() {
        let Some(token) = element.into_token() else {
            continue;
        };
        if !token.kind().is_trivia() {
            break;
        }
        if token.kind() == SyntaxKind::DOC_COMMENT {
            let text = token.text().trim_start_matches("///");
            let text = text.strip_prefix(' ').unwrap_or(text);
            lines.push(text.trim_end().to_string());
        } else if token.kind() == SyntaxKind::COMMENT {
            // A plain comment between them ends the description above it.
            lines.clear();
        }
    }
    (!lines.is_empty()).then(|| lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_count_from_one() {
        let index = LineIndex::new("a\nbc\n\nd");
        assert_eq!(index.line(0), 1);
        assert_eq!(index.line(1), 1);
        assert_eq!(index.line(2), 2);
        assert_eq!(index.line(5), 3);
        assert_eq!(index.line(6), 4);
    }
}
