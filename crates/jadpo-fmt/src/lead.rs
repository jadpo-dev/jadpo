//! The primary declaration first (area 1, "Files and names", decided
//! 2026-10-15): a file's primary declaration goes after its opening
//! comments and `use` lines, ahead of the declarations that support it,
//! which keep the order they were written in. A declaration moves with the
//! comments the formatter prints with it: those above it, up to the
//! previous token's trailing comment, and its own trailing comment.

use jadpo_syntax::SyntaxKind::{NAME, USE_DECL};
use jadpo_syntax::{SyntaxNode, SyntaxToken, parse_file};

use crate::format::{first_token, leading_comments, own_trivia, trailing_comment};

/// `text` with the declaration named `primary` moved first, or `None`
/// when it is already first, isn't there, or the file doesn't parse.
pub(crate) fn lead_with(text: &str, primary: &str) -> Option<String> {
    let parse = parse_file(text);
    if parse.has_errors() {
        return None;
    }
    let file = parse.syntax();
    let decls: Vec<SyntaxNode> = file
        .children()
        .filter(|child| child.kind() != USE_DECL)
        .collect();
    let at = decls
        .iter()
        .position(|decl| name_of(decl).as_deref() == Some(primary))?;
    if at == 0 {
        return None;
    }
    let insert = chunk_start(&decls[0], true)?;
    let start = chunk_start(&decls[at], false)?;
    let end = chunk_end(&decls[at])?;
    let mut out = String::with_capacity(text.len() + 2);
    out.push_str(&text[..insert]);
    out.push_str(&text[start..end]);
    out.push_str("\n\n");
    out.push_str(text[insert..start].trim_end());
    out.push('\n');
    out.push_str(&text[end..]);
    Some(out)
}

/// The name a declaration is written with; tests and setting blocks have
/// none.
fn name_of(decl: &SyntaxNode) -> Option<String> {
    let name = decl.children().find(|child| child.kind() == NAME)?;
    Some(first_token(&name)?.text().to_string())
}

/// Where a declaration and the comments above it begin. The file's
/// opening comments, up to its last blank line before the first token,
/// stay where they are, as they do above `use` lines.
fn chunk_start(decl: &SyntaxNode, first: bool) -> Option<usize> {
    let token = first_token(decl)?;
    let opens_file = first && trivia_only_before(&token);
    let comments: Vec<SyntaxToken> = if opens_file {
        let leading = leading_comments(&token);
        let kept = leading
            .iter()
            .rposition(|comment| comment.blank_after)
            .map_or(0, |last| last + 1);
        leading[kept..].iter().map(|c| c.token.clone()).collect()
    } else {
        own_trivia(&token)
            .into_iter()
            .filter(|t| crate::format::is_comment(t.kind()))
            .collect()
    };
    let begins = comments.first().unwrap_or(&token);
    Some(usize::from(begins.text_range().start()))
}

/// Whether nothing but trivia comes before `token`.
fn trivia_only_before(token: &SyntaxToken) -> bool {
    let mut current = token.prev_token();
    while let Some(t) = current {
        if !t.kind().is_trivia() {
            return false;
        }
        current = t.prev_token();
    }
    true
}

/// Where a declaration ends: its last token, or the comment after it on
/// the same line.
fn chunk_end(decl: &SyntaxNode) -> Option<usize> {
    let last = decl
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| !token.kind().is_trivia())
        .last()?;
    let end = trailing_comment(&last).unwrap_or(last);
    Some(usize::from(end.text_range().end()))
}
