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
use crate::error::TextEdit;
use crate::habits::{self, Habit};

/// One token: its kind and length in bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LexToken {
    pub(crate) kind: SyntaxKind,
    pub(crate) len: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum LexErrorKind {
    UnexpectedCharacter(String),
    /// A habit from another language (`===`, `&&`, `;`, `#`), read as its
    /// Jadpo form so the rest parses as meant.
    Habit(Box<LexHabit>),
    /// A carriage return not followed by a line feed.
    LoneCarriageReturn,
    UnterminatedString,
    InvalidEscape(char),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LexHabit {
    pub(crate) habit: &'static Habit,
    pub(crate) written: String,
    pub(crate) jadpo: String,
    pub(crate) edits: Vec<TextEdit>,
    /// Whether the token now reads exactly as the Jadpo form.
    pub(crate) clean: bool,
    /// A next step in place of the habit's own.
    pub(crate) next: Option<String>,
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

/// Whether `word`, starting at `start` in `text`, reads as a route
/// entry's method: an HTTP method, or any upper-case word that starts its
/// line, such as `FETCH`, which the parser reports as a method a route
/// isn't written with (area 7) rather than as a division.
pub(crate) fn is_method_word(text: &str, start: usize, word: &str) -> bool {
    if HTTP_METHODS.contains(&word) {
        return true;
    }
    word.len() >= 2
        && word.bytes().all(|b| b.is_ascii_uppercase())
        && text
            .get(..start)
            .unwrap_or("")
            .trim_end_matches([' ', '\t'])
            .ends_with(['\n', '{'])
}

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

    fn habit(&mut self, start: usize, end: usize, habit: LexHabit) {
        self.error(start, end, LexErrorKind::Habit(Box::new(habit)));
    }

    /// The text from `at` to the end of its line.
    fn rest_of_line(&self, at: usize) -> &str {
        let rest = &self.text[at..];
        &rest[..rest.find('\n').unwrap_or(rest.len())]
    }

    /// The spaces and tabs that start the line `at` is on.
    fn indent_at(&self, at: usize) -> &str {
        let line = self.text[..at].rfind('\n').map_or(0, |i| i + 1);
        let rest = &self.text[line..];
        &rest[..rest.len() - rest.trim_start_matches([' ', '\t']).len()]
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
            Some((IDENT, start, end)) => is_method_word(self.text, start, &self.text[start..end]),
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
            // A byte-order mark some editors write first is not code.
            '\u{feff}' if start == 0 => {
                self.pos += c.len_utf8();
                self.push(WHITESPACE, start);
            }
            ' ' | '\t' | '\r' | '\n' => {
                self.bump_while(|c| matches!(c, ' ' | '\t' | '\r' | '\n'));
                // A line ended by a carriage return alone would read as one
                // line with the next, and every position after it be wrong.
                let run = &self.text[start..self.pos];
                // One error a file: the whole file uses that line ending.
                let already = self
                    .errors
                    .iter()
                    .any(|e| e.kind == LexErrorKind::LoneCarriageReturn);
                if let Some(at) = run
                    .char_indices()
                    .find(|&(i, c)| c == '\r' && !run[i + 1..].starts_with('\n'))
                    .map(|(i, _)| start + i)
                    .filter(|_| !already)
                {
                    self.error(at, at + 1, LexErrorKind::LoneCarriageReturn);
                }
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
            '/' if self.rest().starts_with("/*") && !self.in_template_code => self.block_comment(),
            '#' if !self.in_template_code && !matches!(self.peek_at(1), Some('[' | '!')) => {
                self.bump_while(|c| c != '\n');
                let trimmed = self.text[start..self.pos].trim_end().len();
                self.pos = start + trimmed;
                self.push(COMMENT, start);
                self.habit(
                    start,
                    start + 1,
                    LexHabit {
                        habit: &habits::HASH_COMMENT,
                        written: "#".into(),
                        jadpo: "//".into(),
                        edits: vec![TextEdit::replace(start as u32, start as u32 + 1, "//")],
                        clean: true,
                        next: None,
                    },
                );
            }
            '\'' | '`' => self.quoted(c),
            ';' => self.semicolon(),
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
        // `===`, `!==` and `<>` from other languages: one mistake each, read
        // as `==` and `!=` so the rest of the line parses as meant.
        for (text, kind, habit) in [
            ("===", EQ2, &habits::STRICT_EQUAL),
            ("!==", NEQ, &habits::STRICT_NOT_EQUAL),
            ("<>", NEQ, &habits::SQL_NOT_EQUAL),
        ] {
            if self.rest().starts_with(text) {
                self.pos += text.len();
                self.push(kind, start);
                let meant = if kind == EQ2 { "==" } else { "!=" };
                self.habit(
                    start,
                    self.pos,
                    LexHabit {
                        habit,
                        written: text.into(),
                        jadpo: meant.into(),
                        edits: vec![TextEdit::replace(start as u32, self.pos as u32, meant)],
                        clean: true,
                        next: None,
                    },
                );
                return;
            }
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
            // `&&` and `||` are one mistake each, and so is `!` before what
            // it negates; the parser reads them as `and`, `or` and `not`.
            let symbol = if self.rest().starts_with("&&") {
                Some(("&&", "and", &habits::SYMBOL_AND))
            } else if self.rest().starts_with("||") {
                Some(("||", "or", &habits::SYMBOL_OR))
            } else if c == '!' && !self.ends_operand_here() {
                Some(("!", "not", &habits::SYMBOL_NOT))
            } else {
                None
            };
            self.pos += symbol.map_or(c.len_utf8(), |(text, _, _)| text.len());
            let found = self.text[start..self.pos].to_string();
            self.push(ERROR, start);
            match symbol {
                Some((written, jadpo, habit)) => {
                    // A word needs spaces around it: `a&&b` becomes `a and b`
                    // and `!a` becomes `not a`.
                    let spaced = |c: Option<char>| c.is_none_or(char::is_whitespace);
                    let before = self.text[..start].chars().next_back();
                    let after = self.peek();
                    let mut replacement = String::new();
                    if written != "!" && !spaced(before) {
                        replacement.push(' ');
                    }
                    replacement.push_str(jadpo);
                    if !spaced(after) {
                        replacement.push(' ');
                    }
                    self.habit(
                        start,
                        self.pos,
                        LexHabit {
                            habit,
                            written: written.into(),
                            jadpo: jadpo.into(),
                            edits: vec![TextEdit::replace(
                                start as u32,
                                self.pos as u32,
                                replacement,
                            )],
                            clean: true,
                            next: None,
                        },
                    );
                }
                None => self.error(start, self.pos, LexErrorKind::UnexpectedCharacter(found)),
            }
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

    /// Whether the last token touches the current position and ends an
    /// operand, so a `!` here follows a value (`x!`, `assert_eq!`) rather
    /// than negating what comes next.
    fn ends_operand_here(&self) -> bool {
        matches!(self.last, Some((IDENT | R_PAREN | R_BRACK | STRING | INT_NUMBER, _, end)) if end == self.pos)
    }

    /// `;`, which Jadpo doesn't have: read as nothing. At the end of a line
    /// (or before the `}` that closes a one-line block) the line then reads
    /// as meant; with more on the line, what follows goes on a line of its
    /// own.
    fn semicolon(&mut self) {
        let start = self.pos;
        // Splitting the line is only the fix when what comes before the `;`
        // can end one: not after `use shared/;` or `a +;`.
        let ends_line = matches!(
            self.last,
            Some((
                IDENT
                    | R_PAREN
                    | R_BRACK
                    | R_BRACE
                    | R_ANGLE
                    | STRING
                    | INT_NUMBER
                    | DECIMAL_NUMBER
                    | UNIT_NUMBER
                    | TRUE_KW
                    | FALSE_KW
                    | NONE_KW
                    | RETURN_KW,
                _,
                _
            ))
        );
        self.pos += 1;
        self.push(SKIPPED, start);
        let after = self.rest_of_line(self.pos);
        let code = after.split("//").next().unwrap_or("").trim();
        let ends = code.is_empty() || code.starts_with('}') || code.chars().all(|c| c == ';');
        let (edit, next) = if ends {
            // Spaces before the `;` go with it.
            let before = self.text[..start].trim_end_matches([' ', '\t']).len();
            (TextEdit::replace(before as u32, self.pos as u32, ""), None)
        } else {
            let spaces = after.len() - after.trim_start_matches([' ', '\t']).len();
            let indent = self.indent_at(start).to_string();
            (
                TextEdit::replace(
                    start as u32,
                    (self.pos + spaces) as u32,
                    format!("\n{indent}"),
                ),
                Some("Put what follows `;` on a line of its own".to_string()),
            )
        };
        self.habit(
            start,
            self.pos,
            LexHabit {
                habit: &habits::SEMICOLON,
                written: ";".into(),
                jadpo: String::new(),
                edits: vec![edit],
                clean: ends || ends_line,
                next,
            },
        );
    }

    /// `/* ... */` or `/** ... */`: a comment, or a documentation comment,
    /// from another language. The edit writes it as `//` or `///` lines
    /// when nothing follows it on its last line.
    fn block_comment(&mut self) {
        let start = self.pos;
        let doc = self.rest().starts_with("/**") && !self.rest().starts_with("/**/");
        let end = self.text[start + 2..]
            .find("*/")
            .map_or(self.text.len(), |i| start + 2 + i + 2);
        self.pos = end;
        self.push(if doc { DOC_COMMENT } else { COMMENT }, start);
        let marker = if doc { "///" } else { "//" };
        let closed = self.text[start..end].ends_with("*/") && end - start >= 4;
        let after = self.rest_of_line(end).trim();
        let alone_after = after.is_empty() || after.starts_with("//");
        let mut edits = Vec::new();
        if closed && alone_after {
            let inner = &self.text[start + if doc { 3 } else { 2 }..end - 2];
            let lines: Vec<String> = inner
                .lines()
                .map(|line| {
                    let line = line.trim();
                    let line = line.strip_prefix('*').unwrap_or(line);
                    line.trim().to_string()
                })
                .collect();
            let first = lines.iter().position(|l| !l.is_empty()).unwrap_or(0);
            let last = lines.iter().rposition(|l| !l.is_empty()).unwrap_or(0);
            let indent = self.indent_at(start).to_string();
            let body: Vec<String> = lines
                .get(first..=last.max(first))
                .unwrap_or(&[])
                .iter()
                .map(|l| {
                    if l.is_empty() {
                        marker.to_string()
                    } else {
                        format!("{marker} {l}")
                    }
                })
                .collect();
            let body = if body.is_empty() {
                vec![marker.to_string()]
            } else {
                body
            };
            edits.push(TextEdit::replace(
                start as u32,
                end as u32,
                body.join(&format!("\n{indent}")),
            ));
        }
        // Without an edit (an unclosed comment, or code after it on its
        // line) the file doesn't read as its Jadpo form.
        let clean = !edits.is_empty();
        let next = edits.is_empty().then(|| {
            format!(
                "Write the comment after `{marker}` at the end of the line, or on lines of its own"
            )
        });
        self.habit(
            start,
            start + if doc { 3 } else { 2 },
            LexHabit {
                habit: &habits::BLOCK_COMMENT,
                written: if doc { "/** */" } else { "/* */" }.into(),
                jadpo: marker.into(),
                edits,
                clean,
                next,
            },
        );
    }

    /// `'text'` or a backtick string without `${`, closed on its line: read
    /// as a string, and its edit writes it in double quotes. Anything else
    /// is a character Jadpo doesn't have.
    fn quoted(&mut self, delimiter: char) {
        let start = self.pos;
        let line = self.rest_of_line(start + 1).to_string();
        let mut close = None;
        let mut escaped = false;
        for (i, c) in line.char_indices() {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == delimiter {
                close = Some(i);
                break;
            }
        }
        let inner = close.map(|i| line[..i].to_string());
        let Some(inner) = inner.filter(|inner| !(delimiter == '`' && inner.contains("${"))) else {
            self.pos += 1;
            self.push(ERROR, start);
            self.error(
                start,
                self.pos,
                LexErrorKind::UnexpectedCharacter(delimiter.to_string()),
            );
            return;
        };
        self.pos = start + 1 + inner.len() + 1;
        self.push(STRING, start);
        let written = self.text[start..self.pos].to_string();
        let mut text = String::new();
        let mut chars = inner.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                '\\' if chars.peek() == Some(&delimiter) => {
                    text.push(delimiter);
                    chars.next();
                }
                '\\' => {
                    text.push('\\');
                    if let Some(next) = chars.next() {
                        text.push(next);
                    }
                }
                '"' => text.push_str("\\\""),
                c => text.push(c),
            }
        }
        let jadpo = format!("\"{text}\"");
        self.habit(
            start,
            self.pos,
            LexHabit {
                habit: &habits::SINGLE_QUOTES,
                written,
                jadpo: jadpo.clone(),
                edits: vec![TextEdit::replace(start as u32, self.pos as u32, jadpo)],
                clean: false,
                next: None,
            },
        );
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
    fn a_leading_byte_order_mark_is_whitespace_and_a_lone_cr_is_an_error() {
        let (tokens, errors) = lex("\u{feff}type A = Int {}\r\n", LexMode::Source);
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(tokens[0].kind, WHITESPACE);
        let (_, errors) = lex("type A = Int {}\rtype B = Int {}\r", LexMode::Source);
        let found: Vec<&LexErrorKind> = errors.iter().map(|e| &e.kind).collect();
        assert_eq!(found, vec![&LexErrorKind::LoneCarriageReturn], "one a file");
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
