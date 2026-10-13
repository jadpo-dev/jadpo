//! Jadpo's lexer, parser and lossless syntax tree (docs/compiler-runtime.md
//! §2.2).
//!
//! [`parse_file`] reads a `.jadpo` file into a rowan tree that holds every
//! byte of the input, comments and whitespace included, and a list of
//! [`SyntaxError`]s. Parsing never fails and never panics; a broken file
//! still gives a tree, with the broken parts in error nodes. The typed AST in
//! [`ast`] is generated from `jadpo.ungram`.

pub mod ast;
mod error;
mod generated;
mod grammar;
mod lexer;
mod parser;
mod sink;

use jadpo_diag::rule;
use rowan::GreenNode;

pub use error::{SyntaxError, TextEdit};
pub use generated::kinds::SyntaxKind;
pub use rowan::TokenAtOffset;

use lexer::{LexError, LexErrorKind, LexMode};
use parser::{Input, Parser};

/// Jadpo's tree language for rowan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum JadpoLanguage {}

impl rowan::Language for JadpoLanguage {
    type Kind = SyntaxKind;

    fn kind_from_raw(raw: rowan::SyntaxKind) -> SyntaxKind {
        SyntaxKind::from_raw(raw.0)
    }

    fn kind_to_raw(kind: SyntaxKind) -> rowan::SyntaxKind {
        rowan::SyntaxKind(kind as u16)
    }
}

pub type SyntaxNode = rowan::SyntaxNode<JadpoLanguage>;
pub type SyntaxToken = rowan::SyntaxToken<JadpoLanguage>;
pub type SyntaxElement = rowan::SyntaxElement<JadpoLanguage>;
pub type SyntaxNodeChildren = rowan::SyntaxNodeChildren<JadpoLanguage>;

/// A parsed text: the lossless tree and its syntax errors, in source order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parse {
    green: GreenNode,
    errors: Vec<SyntaxError>,
}

impl Parse {
    pub fn syntax(&self) -> SyntaxNode {
        SyntaxNode::new_root(self.green.clone())
    }

    pub fn errors(&self) -> &[SyntaxError] {
        &self.errors
    }

    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Entry {
    File,
    Snippet,
    Template,
}

/// Parses a `.jadpo` file: declarations only.
pub fn parse_file(text: &str) -> Parse {
    parse(text, Entry::File)
}

/// Parses a documentation example: declarations, members, route entries and
/// statements may stand at the top level, and `...` stands for code left
/// out.
pub fn parse_snippet(text: &str) -> Parse {
    parse(text, Entry::Snippet)
}

/// Parses an email template: text with Jadpo inside `{{ }}` (area 16).
pub fn parse_template(text: &str) -> Parse {
    parse(text, Entry::Template)
}

fn parse(text: &str, entry: Entry) -> Parse {
    let mode = if entry == Entry::Template {
        LexMode::Template
    } else {
        LexMode::Source
    };
    let (tokens, lex_errors) = lexer::lex(text, mode);
    let input = Input::new(text, &tokens);
    let mut parser = Parser::new(&input, text, entry == Entry::Snippet);
    parser.seed_errors(
        lex_errors
            .iter()
            .map(|error| lex_error(text, error))
            .collect(),
    );
    match entry {
        Entry::File => grammar::source_file(&mut parser),
        Entry::Snippet => grammar::snippet(&mut parser),
        Entry::Template => grammar::template(&mut parser),
    }
    let mut errors = std::mem::take(&mut parser.errors);
    let green = sink::build(text, &tokens, std::mem::take(&mut parser.events));
    errors.sort_by_key(|error| (error.start, error.end));
    Parse { green, errors }
}

fn lex_error(text: &str, error: &LexError) -> SyntaxError {
    let (start, end) = (error.start, error.end);
    match &error.kind {
        LexErrorKind::UnexpectedCharacter(found) => {
            let base = SyntaxError::new(
                &rule::SYNTAX_UNEXPECTED_CHARACTER,
                format!("`{found}` isn't part of Jadpo"),
                start,
                end,
            )
            .label("not Jadpo");
            let (reason, next, replacement): (&str, String, Option<&str>) = match found.as_str() {
                ";" => (
                    "Statements and members end at the end of their line, so Jadpo has no `;`.",
                    "Remove `;`".into(),
                    Some(""),
                ),
                "!" => (
                    "Jadpo writes negation as the word `not`.",
                    "Use `not` instead of `!`".into(),
                    Some("not "),
                ),
                "&&" => (
                    "Jadpo writes logical operators as words.",
                    "Use `and` instead of `&&`".into(),
                    Some("and"),
                ),
                "||" => (
                    "Jadpo writes logical operators as words.",
                    "Use `or` instead of `||`".into(),
                    Some("or"),
                ),
                "'" | "`" => (
                    "Strings are written in double quotes: `\"text\"`.",
                    "Use double quotes around the text".into(),
                    None,
                ),
                other if !other.is_ascii() => (
                    "Names use ASCII letters, digits and `_`; other characters belong in strings.",
                    format!("Remove `{other}`, or put the text in a string"),
                    None,
                ),
                other => (
                    "Jadpo source is names, literals, brackets and operators such as `+` and `==`; this character has no meaning.",
                    format!("Remove `{other}`"),
                    Some(""),
                ),
            };
            let error = base.reason(reason).next(next);
            match replacement {
                Some(text) => error.edit(TextEdit::replace(start, end, text)),
                None => error,
            }
        }
        LexErrorKind::UnterminatedString => SyntaxError::new(
            &rule::SYNTAX_UNTERMINATED_STRING,
            "This string isn't closed",
            start,
            end,
        )
        .label("no closing `\"` on this line")
        .reason("A string starts and ends with `\"` on the same line.")
        .next("Add `\"` where the string ends")
        .edit(TextEdit::insert(end, "\"")),
        LexErrorKind::InvalidEscape(c) => {
            let written = text.get(start as usize..end as usize).unwrap_or("");
            SyntaxError::new(
                &rule::SYNTAX_INVALID_ESCAPE,
                format!("`{written}` isn't an escape Jadpo knows"),
                start,
                end,
            )
            .label("unknown escape")
            .reason("Inside a string, `\\` starts one of `\\\"`, `\\\\`, `\\n`, `\\r` or `\\t`.")
            .next(format!("Write `\\\\{c}` for a backslash followed by `{c}`"))
            .edit(TextEdit::replace(start, start + 1, "\\\\"))
        }
    }
}

/// The candidate closest to `word` by edit distance, if it is close enough
/// to be what was meant.
pub(crate) fn nearest<'a>(
    word: &str,
    candidates: impl Iterator<Item = &'a str>,
) -> Option<&'a str> {
    let limit = (word.chars().count() / 3).max(1);
    candidates
        .map(|candidate| (distance(word, candidate), candidate))
        .filter(|(d, _)| *d <= limit)
        .min_by_key(|(d, _)| *d)
        .map(|(_, candidate)| candidate)
}

fn distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut previous = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let current = row[j + 1];
            row[j + 1] = if ca == *cb {
                previous
            } else {
                1 + previous.min(row[j]).min(row[j + 1])
            };
            previous = current;
        }
    }
    row[b.len()]
}

/// The tree as indented text, one node or token per line: for tests and
/// debugging.
pub fn debug_tree(node: &SyntaxNode) -> String {
    let mut out = String::new();
    let mut depth = 0usize;
    for event in node.preorder_with_tokens() {
        match event {
            rowan::WalkEvent::Enter(element) => {
                out.push_str(&"  ".repeat(depth));
                match element {
                    rowan::NodeOrToken::Node(node) => {
                        out.push_str(&format!("{:?}\n", node.kind()));
                        depth += 1;
                    }
                    rowan::NodeOrToken::Token(token) => {
                        out.push_str(&format!("{:?} {:?}\n", token.kind(), token.text()));
                    }
                }
            }
            rowan::WalkEvent::Leave(rowan::NodeOrToken::Node(_)) => depth -= 1,
            rowan::WalkEvent::Leave(_) => {}
        }
    }
    out
}
