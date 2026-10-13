//! The hand-written lexer (docs/compiler-runtime.md §2.2). It never fails:
//! every byte of the input lands in exactly one token, and problems are
//! reported beside the tokens.
//!
//! Three modes, chosen by what came before rather than by the parser:
//!
//! - **Route paths.** After `:` or an HTTP method (`GET`), a `/` starts a
//!   path: `/items/{item_id: Item.id}/complete` is `/`, a segment, `/` and a
//!   typed placeholder whose inside is ordinary Jadpo.
//! - **Quantities.** Digits followed by letters are one token: `15m`, `30d`,
//!   `5mb`.
//! - **Templates.** In an email template, text runs until `{{`, and Jadpo is
//!   read until the matching `}}`.

use crate::SyntaxKind::{self, *};

/// One token: its kind and length in bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LexToken {
    pub(crate) kind: SyntaxKind,
    pub(crate) len: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum LexErrorKind {
    UnexpectedCharacter(String),
    UnterminatedString,
    InvalidEscape(char),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LexError {
    pub(crate) start: u32,
    pub(crate) end: u32,
    pub(crate) kind: LexErrorKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LexMode {
    Source,
    Template,
}

pub(crate) const HTTP_METHODS: &[&str] =
    &["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];

pub(crate) fn lex(text: &str, mode: LexMode) -> (Vec<LexToken>, Vec<LexError>) {
    let mut lexer = Lexer {
        text,
        pos: 0,
        tokens: Vec::new(),
        errors: Vec::new(),
        last: None,
        param_depth: 0,
        template: mode == LexMode::Template,
        in_template_code: false,
        template_depth: 0,
    };
    lexer.run();
    (lexer.tokens, lexer.errors)
}

struct Lexer<'a> {
    text: &'a str,
    pos: usize,
    tokens: Vec<LexToken>,
    errors: Vec<LexError>,
    /// The last significant token: its kind and text range.
    last: Option<(SyntaxKind, usize, usize)>,
    /// Brace depth inside a route path's `{placeholder}`; 0 outside one.
    param_depth: u32,
    template: bool,
    in_template_code: bool,
    /// Brace depth inside `{{ ... }}`.
    template_depth: u32,
}

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

fn is_ident_continue(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn is_segment_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '~')
}

impl Lexer<'_> {
    fn rest(&self) -> &str {
        &self.text[self.pos..]
    }

    fn peek(&self) -> Option<char> {
        self.rest().chars().next()
    }

    fn peek_at(&self, n: usize) -> Option<char> {
        self.rest().chars().nth(n)
    }

    fn push(&mut self, kind: SyntaxKind, start: usize) {
        let len = (self.pos - start) as u32;
        if len == 0 {
            return;
        }
        self.tokens.push(LexToken { kind, len });
        if !kind.is_trivia() {
            self.last = Some((kind, start, self.pos));
        }
    }

    fn error(&mut self, start: usize, end: usize, kind: LexErrorKind) {
        self.errors.push(LexError {
            start: start as u32,
            end: end as u32,
            kind,
        });
    }

    fn bump_while(&mut self, pred: impl Fn(char) -> bool) {
        while let Some(c) = self.peek() {
            if !pred(c) {
                break;
            }
            self.pos += c.len_utf8();
        }
    }

    fn run(&mut self) {
        while self.pos < self.text.len() {
            if self.template && !self.in_template_code {
                self.template_text();
                continue;
            }
            self.token();
        }
    }

    /// Text in a template up to the next `{{`.
    fn template_text(&mut self) {
        let start = self.pos;
        if self.rest().starts_with("{{") {
            self.pos += 2;
            self.push(L_DBRACE, start);
            self.in_template_code = true;
            self.template_depth = 0;
            return;
        }
        match self.rest().find("{{") {
            Some(offset) => self.pos += offset,
            None => self.pos = self.text.len(),
        }
        self.push(TEMPLATE_TEXT, start);
    }

    /// Whether a `/` here starts a route path rather than dividing.
    fn path_starts_here(&self) -> bool {
        if self.peek_at(1) == Some('/') {
            return false;
        }
        match self.last {
            Some((COLON, _, _)) => true,
            Some((IDENT, start, end)) => HTTP_METHODS.contains(&&self.text[start..end]),
            _ => false,
        }
    }

    /// `/segment/{param}/...`, stopping at a placeholder's `{`, which the
    /// ordinary lexer then reads until its `}`.
    fn route_path(&mut self) {
        loop {
            match self.peek() {
                Some('/') if self.peek_at(1) != Some('/') => {
                    let start = self.pos;
                    self.pos += 1;
                    self.push(SLASH, start);
                    let start = self.pos;
                    self.bump_while(is_segment_char);
                    self.push(PATH_SEGMENT, start);
                }
                Some('{') => {
                    let start = self.pos;
                    self.pos += 1;
                    self.push(L_BRACE, start);
                    self.param_depth = 1;
                    return;
                }
                _ => return,
            }
        }
    }

    fn token(&mut self) {
        let start = self.pos;
        let Some(c) = self.peek() else { return };
        match c {
            ' ' | '\t' | '\r' | '\n' => {
                self.bump_while(|c| matches!(c, ' ' | '\t' | '\r' | '\n'));
                self.push(WHITESPACE, start);
            }
            '/' if self.rest().starts_with("//") => {
                let doc = self.rest().starts_with("///") && !self.rest().starts_with("////");
                self.bump_while(|c| c != '\n');
                // A comment keeps no trailing carriage return or spaces.
                let trimmed = self.text[start..self.pos].trim_end().len();
                self.pos = start + trimmed;
                self.push(if doc { DOC_COMMENT } else { COMMENT }, start);
            }
            '/' if self.param_depth == 0 && self.path_starts_here() => self.route_path(),
            '"' => self.string(),
            c if c.is_ascii_digit() => self.number(),
            c if is_ident_start(c) => {
                self.bump_while(is_ident_continue);
                let word = &self.text[start..self.pos];
                let kind = if word == "_" {
                    UNDERSCORE
                } else {
                    SyntaxKind::from_keyword(word).unwrap_or(IDENT)
                };
                self.push(kind, start);
            }
            _ => self.punctuation(c),
        }
    }

    fn punctuation(&mut self, c: char) {
        let start = self.pos;
        const MULTI: &[(&str, SyntaxKind)] = &[
            ("...", DOT3),
            ("->", THIN_ARROW),
            ("=>", FAT_ARROW),
            ("==", EQ2),
            ("!=", NEQ),
            ("<=", LTEQ),
            (">=", GTEQ),
        ];
        if self.in_template_code && self.template_depth == 0 && self.rest().starts_with("}}") {
            self.pos += 2;
            self.push(R_DBRACE, start);
            self.in_template_code = false;
            return;
        }
        for (text, kind) in MULTI {
            if self.rest().starts_with(text) {
                self.pos += text.len();
                self.push(*kind, start);
                return;
            }
        }
        let kind = match c {
            '{' => L_BRACE,
            '}' => R_BRACE,
            '(' => L_PAREN,
            ')' => R_PAREN,
            '[' => L_BRACK,
            ']' => R_BRACK,
            '<' => L_ANGLE,
            '>' => R_ANGLE,
            ',' => COMMA,
            ':' => COLON,
            '.' => DOT,
            '=' => EQ,
            '+' => PLUS,
            '-' => MINUS,
            '*' => STAR,
            '/' => SLASH,
            '%' => PERCENT,
            '?' => QUESTION,
            _ => ERROR,
        };
        if kind == ERROR {
            // `&&` and `||` are reported as one mistake each.
            let width = if self.rest().starts_with("&&") || self.rest().starts_with("||") {
                2
            } else {
                c.len_utf8()
            };
            self.pos += width;
            let found = self.text[start..self.pos].to_string();
            self.push(ERROR, start);
            self.error(start, self.pos, LexErrorKind::UnexpectedCharacter(found));
            return;
        }
        self.pos += 1;
        match kind {
            L_BRACE if self.in_template_code => self.template_depth += 1,
            R_BRACE if self.in_template_code => {
                self.template_depth = self.template_depth.saturating_sub(1)
            }
            _ => {}
        }
        let closes_placeholder = kind == R_BRACE && self.param_depth == 1;
        if self.param_depth > 0 {
            match kind {
                L_BRACE => self.param_depth += 1,
                R_BRACE => self.param_depth -= 1,
                _ => {}
            }
        }
        self.push(kind, start);
        // A path goes on after a placeholder: `/{todo_id}/complete`.
        if closes_placeholder && self.peek() == Some('/') && self.peek_at(1) != Some('/') {
            self.route_path();
        }
    }

    fn string(&mut self) {
        let start = self.pos;
        self.pos += 1;
        loop {
            match self.peek() {
                None | Some('\n') => {
                    let end = self.pos;
                    self.push(STRING, start);
                    self.error(start, end, LexErrorKind::UnterminatedString);
                    return;
                }
                Some('"') => {
                    self.pos += 1;
                    self.push(STRING, start);
                    return;
                }
                Some('\\') => {
                    let escape_start = self.pos;
                    self.pos += 1;
                    match self.peek() {
                        Some('"' | '\\' | 'n' | 'r' | 't') => self.pos += 1,
                        Some('\n') | None => {}
                        Some(other) => {
                            self.pos += other.len_utf8();
                            self.error(escape_start, self.pos, LexErrorKind::InvalidEscape(other));
                        }
                    }
                }
                Some(c) => self.pos += c.len_utf8(),
            }
        }
    }

    fn number(&mut self) {
        let start = self.pos;
        self.bump_while(|c| c.is_ascii_digit());
        let mut kind = INT_NUMBER;
        if self.peek() == Some('.') && self.peek_at(1).is_some_and(|c| c.is_ascii_digit()) {
            self.pos += 1;
            self.bump_while(|c| c.is_ascii_digit());
            kind = DECIMAL_NUMBER;
        }
        if self.peek().is_some_and(|c| c.is_ascii_alphabetic()) {
            self.bump_while(is_ident_continue);
            kind = UNIT_NUMBER;
        }
        self.push(kind, start);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(text: &str) -> Vec<(SyntaxKind, &str)> {
        let (tokens, _) = lex(text, LexMode::Source);
        let mut pos = 0usize;
        tokens
            .iter()
            .map(|token| {
                let piece = &text[pos..pos + token.len as usize];
                pos += token.len as usize;
                (token.kind, piece)
            })
            .filter(|(kind, _)| *kind != WHITESPACE)
            .collect()
    }

    #[test]
    fn route_paths_are_segments_and_typed_placeholders() {
        assert_eq!(
            kinds("GET /items/{item_id: Item.id}/complete {"),
            vec![
                (IDENT, "GET"),
                (SLASH, "/"),
                (PATH_SEGMENT, "items"),
                (SLASH, "/"),
                (L_BRACE, "{"),
                (IDENT, "item_id"),
                (COLON, ":"),
                (IDENT, "Item"),
                (DOT, "."),
                (IDENT, "id"),
                (R_BRACE, "}"),
                (SLASH, "/"),
                (PATH_SEGMENT, "complete"),
                (L_BRACE, "{"),
            ]
        );
        assert_eq!(
            kinds("prefix: /"),
            vec![(IDENT, "prefix"), (COLON, ":"), (SLASH, "/")]
        );
        assert_eq!(
            kinds("a / b"),
            vec![(IDENT, "a"), (SLASH, "/"), (IDENT, "b")]
        );
    }

    #[test]
    fn quantities_numbers_and_strings() {
        assert_eq!(
            kinds("15m 5mb 1.25 42 \"a\\\"b\""),
            vec![
                (UNIT_NUMBER, "15m"),
                (UNIT_NUMBER, "5mb"),
                (DECIMAL_NUMBER, "1.25"),
                (INT_NUMBER, "42"),
                (STRING, "\"a\\\"b\"")
            ]
        );
    }

    #[test]
    fn comments_keep_their_kind() {
        assert_eq!(
            kinds("/// doc\n// note\n//// rule"),
            vec![
                (DOC_COMMENT, "/// doc"),
                (COMMENT, "// note"),
                (COMMENT, "//// rule")
            ]
        );
    }

    #[test]
    fn templates_alternate_text_and_code() {
        let text = "Hi {{ name }}, {{ if (a) }}x{{ end }}";
        let (tokens, errors) = lex(text, LexMode::Template);
        assert!(errors.is_empty());
        let total: u32 = tokens.iter().map(|t| t.len).sum();
        assert_eq!(total as usize, text.len());
        assert_eq!(tokens[0].kind, TEMPLATE_TEXT);
        assert_eq!(tokens[1].kind, L_DBRACE);
        assert!(tokens.iter().any(|t| t.kind == R_DBRACE));
    }

    #[test]
    fn problems_are_reported_beside_tokens() {
        let (_, errors) = lex("min: 0 $ \"a\\qb\" \"open", LexMode::Source);
        let found: Vec<&LexErrorKind> = errors.iter().map(|e| &e.kind).collect();
        assert_eq!(
            found,
            vec![
                &LexErrorKind::UnexpectedCharacter("$".into()),
                &LexErrorKind::InvalidEscape('q'),
                &LexErrorKind::UnterminatedString
            ]
        );
    }
}
