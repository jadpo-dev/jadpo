//! The Jadpo formatter (docs/compiler-runtime.md §2.2): syntax tree to a
//! document algebra to a printer. It owns indentation, spacing, blank lines
//! (one between declarations, none doubled), one member per line and the
//! order of sections inside a declaration, so equivalent code always looks
//! identical (areas 1 and 14), and, when told a file's primary declaration,
//! writes its declarations in definition-before-use order. A file with
//! syntax errors is not formatted: its errors are returned instead.

mod canonical;
mod doc;
mod format;
mod imports;
mod order;
mod sections;

pub use canonical::canonical_tokens;
pub use imports::{FOLDERS, ROOT_FILES};
use jadpo_syntax::{SyntaxError, SyntaxNode, parse_file, parse_snippet};

/// The longest line the formatter writes where it has a choice.
pub const WIDTH: usize = 100;

/// Formats a `.jadpo` file.
pub fn format_file(text: &str) -> Result<String, Vec<SyntaxError>> {
    format_with(text, parse_file)
}

/// Formats a `.jadpo` file whose primary declaration (area 1, "Files and
/// names") is named `primary`, and writes its declarations in definition
/// before use order: after the file's opening comments and `use` lines,
/// each before the first that uses it, so the primary comes last; ties by
/// kind, then by the order the primary first reaches them, and
/// declarations that use each other keep their written order. The caller
/// knows the primary from the file's place in the layout; `None` keeps the
/// written order.
pub fn format_file_in_order(text: &str, primary: Option<&str>) -> Result<String, Vec<SyntaxError>> {
    match primary.and_then(|name| order::definition_before_use(text, name)) {
        Some(moved) => format_file(&moved),
        None => format_file(text),
    }
}

/// Formats a documentation example, which may hold members, statements
/// and `...` at its top level.
pub fn format_snippet(text: &str) -> Result<String, Vec<SyntaxError>> {
    format_with(text, parse_snippet)
}

/// Formats `text`, then reads the result back: it must parse to the same
/// tokens and comments and format to itself, or the formatter refuses
/// rather than write a file that means something else or never settles.
fn format_with(
    text: &str,
    parse: fn(&str) -> jadpo_syntax::Parse,
) -> Result<String, Vec<SyntaxError>> {
    let before = parse(text);
    if before.has_errors() {
        return Err(before.errors().to_vec());
    }
    let formatted = format_tree(&before.syntax());
    let after = parse(&formatted);
    let same = !after.has_errors()
        && shape(&before.syntax()) == shape(&after.syntax())
        && format_tree(&after.syntax()) == formatted;
    if same {
        return Ok(formatted);
    }
    Err(vec![unsafe_to_format()])
}

/// `format.unsafe`: the formatter's own mistake, never the file's. Its
/// headline says the file wasn't formatted, not that formatting would
/// change it, which formatting always does.
fn unsafe_to_format() -> SyntaxError {
    SyntaxError::new(
        &jadpo_diag::rule::FORMAT_UNSAFE,
        "This file couldn't be formatted safely",
        0,
        0,
    )
    .label("left as it is")
    .reason(
        "The formatted text didn't read back as the same code, or didn't stay the same when formatted again, so the formatter wrote nothing. This is a mistake in the formatter, not in the file.",
    )
    .next("Leave the file as it is, and report it to Jadpo's maintainers with the file")
}

/// What formatting must keep: every significant token in the order the
/// formatter writes them, each node at its depth, and every comment.
fn shape(node: &SyntaxNode) -> Vec<String> {
    use jadpo_syntax::SyntaxKind::{COMMENT, DOC_COMMENT};
    let mut out: Vec<String> = canonical_tokens(node)
        .iter()
        .map(|token| format!("{:?} {}", token.kind(), token.text()))
        .collect();
    let mut nodes: Vec<String> = node
        .descendants()
        .map(|inner| format!("{:?} {}", inner.kind(), inner.ancestors().count()))
        .collect();
    nodes.sort();
    out.extend(nodes);
    let mut comments: Vec<String> = node
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| matches!(token.kind(), COMMENT | DOC_COMMENT))
        .map(|token| token.text().trim_end().to_string())
        .collect();
    comments.sort();
    out.extend(comments);
    out
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refusal_says_the_file_was_left_as_it_was() {
        let refused = unsafe_to_format();
        assert_eq!(refused.rule.id, "format.unsafe");
        assert_eq!(refused.message, "This file couldn't be formatted safely");
        assert_eq!(refused.label, "left as it is");
        assert!(refused.reason.contains("mistake in the formatter"));
    }
}
