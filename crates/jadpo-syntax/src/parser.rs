//! The event parser (rust-analyzer's design): grammar functions look at
//! significant tokens only and record start, finish and token events; the
//! sink then builds the tree with every whitespace and comment in place.
//!
//! The parser never panics and always terminates: every loop in the grammar
//! either consumes a token or stops, and a parser that looks at the same
//! token too many times without moving sees the end of the input instead.

use std::cell::Cell;

use jadpo_diag::Rule;

use crate::SyntaxKind::{self, *};
use crate::error::{SyntaxError, TextEdit};
use crate::lexer::LexToken;

/// The significant tokens the grammar sees, with where each one is.
#[derive(Debug)]
pub(crate) struct Input {
    pub(crate) kinds: Vec<SyntaxKind>,
    pub(crate) starts: Vec<u32>,
    pub(crate) ends: Vec<u32>,
    /// Whether a line break comes between this token and the one before.
    pub(crate) newline_before: Vec<bool>,
}

impl Input {
    pub(crate) fn new(text: &str, tokens: &[LexToken]) -> Input {
        let mut input = Input {
            kinds: Vec::new(),
            starts: Vec::new(),
            ends: Vec::new(),
            newline_before: Vec::new(),
        };
        let mut offset = 0u32;
        let mut newline = true;
        for token in tokens {
            let start = offset;
            offset += token.len;
            if token.kind.is_trivia() {
                // A `/* */` comment from another language may span lines.
                if matches!(token.kind, WHITESPACE | COMMENT | DOC_COMMENT)
                    && text
                        .get(start as usize..offset as usize)
                        .is_some_and(|piece| piece.contains('\n'))
                {
                    newline = true;
                }
                continue;
            }
            input.kinds.push(token.kind);
            input.starts.push(start);
            input.ends.push(offset);
            input.newline_before.push(newline);
            newline = false;
        }
        input
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Event {
    Start {
        kind: SyntaxKind,
        forward_parent: Option<u32>,
    },
    Finish,
    /// The next significant token, possibly with its kind remapped (a
    /// contextual keyword, or a keyword used as a field name).
    Token {
        kind: SyntaxKind,
    },
    Tombstone,
}

/// How long the parser may look at one token before it is treated as stuck.
const STEP_LIMIT: u32 = 4096;

/// How deeply brackets and blocks may nest before the parser stops
/// descending, so hostile input cannot exhaust the stack.
const MAX_DEPTH: u32 = 96;

pub(crate) struct Parser<'a> {
    input: &'a Input,
    text: &'a str,
    pos: usize,
    pub(crate) events: Vec<Event>,
    pub(crate) errors: Vec<SyntaxError>,
    steps: Cell<u32>,
    /// Inside `( )`, `[ ]` and `< >` line breaks never end anything.
    newlines_ignored: bool,
    /// Whether `Name { ... }` may follow an expression here.
    pub(crate) records_allowed: bool,
    /// While an `attempt`'s call is parsed: a block after the call holds
    /// the `attempt`'s mappings, not a value built from the call.
    pub(crate) attempted: bool,
    pub(crate) snippet: bool,
    /// While a statement's first value is parsed: `=` after it assigns,
    /// rather than being SQL's comparison.
    pub(crate) assigning: bool,
    /// Where the current statement's value starts, so Rust's `call()?`
    /// can be rewritten as `attempt call()`.
    pub(crate) stmt_value_start: Option<u32>,
    depth: u32,
    too_deep: bool,
    error_starts: std::collections::HashSet<u32>,
    /// Whether an error was dropped because another starts at the same
    /// place: the tree then holds more than one mistake there.
    pub(crate) suppressed: bool,
}

impl std::fmt::Debug for Parser<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Parser").field("pos", &self.pos).finish()
    }
}

impl<'a> Parser<'a> {
    pub(crate) fn new(input: &'a Input, text: &'a str, snippet: bool) -> Parser<'a> {
        Parser {
            input,
            text,
            pos: 0,
            events: Vec::new(),
            errors: Vec::new(),
            steps: Cell::new(0),
            newlines_ignored: false,
            records_allowed: true,
            attempted: false,
            snippet,
            assigning: false,
            stmt_value_start: None,
            depth: 0,
            too_deep: false,
            error_starts: std::collections::HashSet::new(),
            suppressed: false,
        }
    }

    fn stuck(&self) -> bool {
        self.steps.get() > STEP_LIMIT
    }

    /// The kind of the token `n` ahead, or `EOF`.
    pub(crate) fn nth(&self, n: usize) -> SyntaxKind {
        let steps = self.steps.get();
        self.steps.set(steps.saturating_add(1));
        if self.stuck() {
            return EOF;
        }
        self.input.kinds.get(self.pos + n).copied().unwrap_or(EOF)
    }

    pub(crate) fn current(&self) -> SyntaxKind {
        self.nth(0)
    }

    pub(crate) fn at(&self, kind: SyntaxKind) -> bool {
        self.nth(0) == kind
    }

    pub(crate) fn at_eof(&self) -> bool {
        self.at(EOF)
    }

    /// The text of the token `n` ahead.
    pub(crate) fn nth_text(&self, n: usize) -> &'a str {
        let i = self.pos + n;
        match (self.input.starts.get(i), self.input.ends.get(i)) {
            (Some(start), Some(end)) => self.text.get(*start as usize..*end as usize).unwrap_or(""),
            _ => "",
        }
    }

    pub(crate) fn current_text(&self) -> &'a str {
        self.nth_text(0)
    }

    /// Whether the source at byte `offset` starts with a lower-case
    /// letter, as a local's name does and a type's never does.
    pub(crate) fn lowercase_at(&self, offset: u32) -> bool {
        self.text
            .get(offset as usize..)
            .and_then(|rest| rest.chars().next())
            .is_some_and(|c| c.is_lowercase())
    }

    /// An identifier with exactly this text, `n` tokens ahead.
    pub(crate) fn nth_word(&self, n: usize, word: &str) -> bool {
        self.nth(n) == IDENT && self.nth_text(n) == word
    }

    pub(crate) fn at_word(&self, word: &str) -> bool {
        self.nth_word(0, word)
    }

    /// Whether a line break comes before the token `n` ahead. The end of
    /// the input counts as a new line.
    pub(crate) fn nth_on_new_line(&self, n: usize) -> bool {
        self.input
            .newline_before
            .get(self.pos + n)
            .copied()
            .unwrap_or(true)
    }

    pub(crate) fn on_new_line(&self) -> bool {
        self.nth_on_new_line(0)
    }

    /// Whether the current token starts its line, with nothing before it.
    pub(crate) fn at_column_zero(&self) -> bool {
        let (start, _) = self.range();
        start == 0 || self.text().as_bytes().get(start as usize - 1) == Some(&b'\n')
    }

    /// Whether line breaks end things here: they do except inside brackets.
    pub(crate) fn line_break_ends(&self) -> bool {
        !self.newlines_ignored && self.on_new_line()
    }

    /// Whether the token `n` ahead starts exactly where the one before it
    /// ends, with nothing between.
    pub(crate) fn nth_adjacent(&self, n: usize) -> bool {
        let i = self.pos + n;
        match (
            i.checked_sub(1).and_then(|p| self.input.ends.get(p)),
            self.input.starts.get(i),
        ) {
            (Some(end), Some(start)) => end == start,
            _ => false,
        }
    }

    /// The byte range of the token `n` ahead; at the end, an empty range
    /// after the last token.
    pub(crate) fn nth_range(&self, n: usize) -> (u32, u32) {
        let i = self.pos + n;
        match (self.input.starts.get(i), self.input.ends.get(i)) {
            (Some(start), Some(end)) => (*start, *end),
            _ => {
                let end = self.input.ends.last().copied().unwrap_or(0);
                (end, end)
            }
        }
    }

    pub(crate) fn range(&self) -> (u32, u32) {
        self.nth_range(0)
    }

    /// The end of the last consumed token.
    pub(crate) fn prev_end(&self) -> u32 {
        self.pos
            .checked_sub(1)
            .and_then(|i| self.input.ends.get(i))
            .copied()
            .unwrap_or(0)
    }

    pub(crate) fn text(&self) -> &'a str {
        self.text
    }

    /// How many significant tokens have been consumed.
    pub(crate) fn position(&self) -> usize {
        self.pos
    }

    /// The range of the last consumed token: where to point when something
    /// is missing at the end of a line.
    pub(crate) fn nth_range_before(&self) -> (u32, u32) {
        match self.pos.checked_sub(1) {
            Some(i) => (
                self.input.starts.get(i).copied().unwrap_or(0),
                self.input.ends.get(i).copied().unwrap_or(0),
            ),
            None => (0, 0),
        }
    }

    /// Goes one level deeper into brackets or blocks; false (once reported)
    /// when the input nests too deeply to follow.
    pub(crate) fn enter(&mut self) -> bool {
        if self.depth >= MAX_DEPTH {
            if !self.too_deep {
                let (start, end) = self.range();
                self.push_error(
                    SyntaxError::new(
                        &jadpo_diag::rule::SYNTAX_UNEXPECTED_TOKEN,
                        "This is nested too deeply to read",
                        start,
                        end,
                    )
                    .label("too deep")
                    .reason(format!(
                        "Jadpo reads brackets and blocks nested up to {MAX_DEPTH} levels deep."
                    ))
                    .next("Move the inner part into a function or a variable of its own"),
                );
                self.too_deep = true;
            }
            return false;
        }
        self.depth += 1;
        true
    }

    pub(crate) fn leave(&mut self) {
        self.depth = self.depth.saturating_sub(1);
    }

    pub(crate) fn start(&mut self) -> Marker {
        let pos = self.events.len() as u32;
        self.events.push(Event::Tombstone);
        Marker { pos }
    }

    /// Consumes the current token, whatever it is.
    pub(crate) fn bump_any(&mut self) {
        let kind = self.current();
        if kind == EOF {
            return;
        }
        self.do_bump(kind);
    }

    /// Consumes the current token as `kind`.
    pub(crate) fn bump_remap(&mut self, kind: SyntaxKind) {
        if self.current() == EOF {
            return;
        }
        self.do_bump(kind);
    }

    pub(crate) fn bump(&mut self, kind: SyntaxKind) {
        debug_assert_eq!(self.current(), kind);
        self.bump_any();
    }

    fn do_bump(&mut self, kind: SyntaxKind) {
        self.pos += 1;
        self.steps.set(0);
        self.events.push(Event::Token { kind });
    }

    pub(crate) fn eat(&mut self, kind: SyntaxKind) -> bool {
        if self.at(kind) {
            self.bump_any();
            true
        } else {
            false
        }
    }

    /// Runs `f` with line breaks ignored (inside brackets) or significant
    /// (inside braces), restoring the setting afterwards.
    pub(crate) fn with_newlines<T>(
        &mut self,
        ignored: bool,
        f: impl FnOnce(&mut Parser<'a>) -> T,
    ) -> T {
        let saved = self.newlines_ignored;
        self.newlines_ignored = ignored;
        let out = f(self);
        self.newlines_ignored = saved;
        out
    }

    pub(crate) fn with_records<T>(
        &mut self,
        allowed: bool,
        f: impl FnOnce(&mut Parser<'a>) -> T,
    ) -> T {
        let saved = (self.records_allowed, self.attempted, self.assigning);
        self.records_allowed = allowed;
        // Inside brackets or braces, a call is no longer the attempted one,
        // and `=` no longer assigns.
        self.attempted = false;
        self.assigning = false;
        let out = f(self);
        (self.records_allowed, self.attempted, self.assigning) = saved;
        out
    }

    /// Runs `f` parsing an `attempt`'s call: a block straight after it is
    /// left for the `attempt`.
    pub(crate) fn attempting<T>(&mut self, f: impl FnOnce(&mut Parser<'a>) -> T) -> T {
        let saved = self.attempted;
        self.attempted = true;
        let out = f(self);
        self.attempted = saved;
        out
    }

    /// Errors found before parsing, by the lexer.
    pub(crate) fn seed_errors(&mut self, errors: Vec<SyntaxError>) {
        for error in errors {
            self.push_error(error);
        }
    }

    pub(crate) fn error_count(&self) -> usize {
        self.errors.len()
    }

    /// Forgets the errors after the first `n`.
    pub(crate) fn truncate_errors(&mut self, n: usize) {
        for error in self.errors.drain(n..) {
            self.error_starts.remove(&error.start);
        }
    }

    /// Records an error, unless one already starts at the same place: one
    /// mistake gives one error.
    pub(crate) fn push_error(&mut self, error: SyntaxError) {
        // Once something is nested too deeply, what follows is unread and
        // any other error would be a knock-on of that one.
        if self.too_deep {
            return;
        }
        if !self.error_starts.insert(error.start) {
            // A clean habit left unreported still reads as its Jadpo form.
            self.suppressed |= !error.clean;
            return;
        }
        self.errors.push(error);
    }

    /// Records `error` in place of the errors already found from `start`
    /// to `end`: the lexer's `!` in `assert_eq!(` or `|` in `Tag | null`,
    /// which are one habit with what the parser then reads.
    pub(crate) fn replace_errors(&mut self, start: u32, end: u32, error: SyntaxError) {
        let (gone, kept): (Vec<SyntaxError>, Vec<SyntaxError>) = std::mem::take(&mut self.errors)
            .into_iter()
            .partition(|e| e.start >= start && e.start < end);
        self.errors = kept;
        for e in gone {
            self.error_starts.remove(&e.start);
        }
        self.push_error(error);
    }

    /// Forgets the errors that start from `start` to `end`: text an edit
    /// removes, such as a `break;` line in a `switch` arm.
    pub(crate) fn remove_errors(&mut self, start: u32, end: u32) {
        let (gone, kept): (Vec<SyntaxError>, Vec<SyntaxError>) = std::mem::take(&mut self.errors)
            .into_iter()
            .partition(|e| e.start >= start && e.start < end);
        self.errors = kept;
        for e in gone {
            self.error_starts.remove(&e.start);
        }
    }

    /// Consumes the current token as text the Jadpo form doesn't have
    /// (`await`, `new`, `export`): it stays in the tree, read as nothing.
    pub(crate) fn skip_token(&mut self) {
        self.bump_remap(SKIPPED);
    }

    /// An error at the current token.
    pub(crate) fn error_here(
        &mut self,
        rule: &'static Rule,
        message: impl Into<String>,
    ) -> SyntaxError {
        let (start, end) = self.range();
        SyntaxError::new(rule, message, start, end)
    }

    /// A short description of the current token for messages: `` `->` ``,
    /// `the end of the line`, `the end of the file`.
    pub(crate) fn found(&self) -> String {
        describe(self.current(), self.current_text(), self.on_new_line())
    }

    /// Wraps the current token in an error node.
    pub(crate) fn bump_error(&mut self) {
        let m = self.start();
        self.bump_any();
        m.complete(self, ERROR);
    }

    /// An insertion right after the last consumed token.
    pub(crate) fn insert_after_prev(&self, text: impl Into<String>) -> TextEdit {
        TextEdit::insert(self.prev_end(), text)
    }
}

/// How a token reads in a message.
pub(crate) fn describe(kind: SyntaxKind, text: &str, new_line: bool) -> String {
    match kind {
        EOF => "the end of the file".to_string(),
        _ if new_line && kind != R_BRACE => format!("`{text}` on the next line"),
        _ => format!("`{text}`"),
    }
}

#[derive(Debug)]
pub(crate) struct Marker {
    pos: u32,
}

impl Marker {
    pub(crate) fn complete(self, p: &mut Parser<'_>, kind: SyntaxKind) -> CompletedMarker {
        if let Some(event) = p.events.get_mut(self.pos as usize) {
            *event = Event::Start {
                kind,
                forward_parent: None,
            };
        }
        p.events.push(Event::Finish);
        CompletedMarker {
            pos: self.pos,
            kind,
        }
    }

    /// Makes this marker the start of the one node parsed from event `at`
    /// on, so what was parsed between this marker and `at` (a
    /// declaration's `decline` lines) becomes that node's first children.
    /// False, changing nothing, unless everything from `at` is one node.
    pub(crate) fn adopt(&self, p: &mut Parser<'_>, at: usize) -> bool {
        let Some(first) = (at..p.events.len()).find(|&i| !matches!(p.events[i], Event::Tombstone))
        else {
            return false;
        };
        let Event::Start {
            kind,
            forward_parent: None,
        } = p.events[first]
        else {
            return false;
        };
        let mut depth = 0i64;
        for (i, event) in p.events.iter().enumerate().skip(first) {
            match event {
                Event::Start { .. } => depth += 1,
                Event::Finish => {
                    depth -= 1;
                    if depth == 0 && i + 1 != p.events.len() {
                        return false;
                    }
                }
                _ => {}
            }
        }
        if depth != 0 {
            return false;
        }
        p.events[first] = Event::Tombstone;
        p.events[self.pos as usize] = Event::Start {
            kind,
            forward_parent: None,
        };
        true
    }

    pub(crate) fn abandon(self, p: &mut Parser<'_>) {
        if self.pos as usize == p.events.len() - 1 {
            p.events.pop();
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct CompletedMarker {
    pos: u32,
    kind: SyntaxKind,
}

impl CompletedMarker {
    pub(crate) fn kind(&self) -> SyntaxKind {
        self.kind
    }

    /// A new node that will contain this one: `a` becomes the left side of
    /// `a + b`.
    pub(crate) fn precede(self, p: &mut Parser<'_>) -> Marker {
        let new = p.start();
        if let Some(Event::Start { forward_parent, .. }) = p.events.get_mut(self.pos as usize) {
            *forward_parent = Some(new.pos - self.pos);
        }
        new
    }
}
