//! The Jadpo formatter (docs/compiler-runtime.md §2.2): syntax tree to a
//! document algebra to a printer. It owns indentation, spacing, blank lines
//! (one between declarations, none doubled) and one member per line, so
//! equivalent code always looks identical (areas 1 and 14). A file with
//! syntax errors is not formatted: its errors are returned instead.

mod doc;
mod format;

use jadpo_syntax::{SyntaxError, SyntaxNode, parse_file, parse_snippet};

/// The longest line the formatter writes where it has a choice.
pub const WIDTH: usize = 100;

/// Formats a `.jadpo` file.
pub fn format_file(text: &str) -> Result<String, Vec<SyntaxError>> {
    let parse = parse_file(text);
    if parse.has_errors() {
        return Err(parse.errors().to_vec());
    }
    Ok(format_tree(&parse.syntax()))
}

/// Formats a documentation example, which may hold members, statements
/// and `...` at its top level.
pub fn format_snippet(text: &str) -> Result<String, Vec<SyntaxError>> {
    let parse = parse_snippet(text);
    if parse.has_errors() {
        return Err(parse.errors().to_vec());
    }
    Ok(format_tree(&parse.syntax()))
}

fn format_tree(node: &SyntaxNode) -> String {
    let mut formatter = format::Formatter::new();
    let doc = formatter.node(node);
    debug_assert_eq!(
        formatter.unprinted_comments(node),
        0,
        "a comment was dropped"
    );
    doc::print(&doc, WIDTH)
}
