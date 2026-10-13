//! Builds the rowan tree from the parser's events and the lexer's tokens,
//! putting every whitespace and comment in the tree by one rule
//! (docs/compiler-runtime.md §2.2, after Biome):
//!
//! - a comment on the same line after a token is **trailing**: it belongs to
//!   that token and sits right after it, inside the same node;
//! - every other comment is **leading**: it belongs to the next token and
//!   sits before it, inside the outermost node that starts with that token,
//!   so a declaration's `///` comments are its first children;
//! - whitespace before a node's leading comments stays outside the node, so
//!   a node without comments starts at its first token.
//!
//! Tokens the parser did not consume (it stopped early) are kept in an
//! error node, so the tree always holds the whole text.

use rowan::{GreenNode, GreenNodeBuilder};

use crate::SyntaxKind::{self, *};
use crate::lexer::LexToken;
use crate::parser::Event;

pub(crate) fn build(text: &str, tokens: &[LexToken], mut events: Vec<Event>) -> GreenNode {
    let mut sink = Sink {
        text,
        tokens,
        raw: 0,
        offset: 0,
        builder: GreenNodeBuilder::new(),
        depth: 0,
    };
    let mut forward = Vec::new();
    for i in 0..events.len() {
        match std::mem::replace(&mut events[i], Event::Tombstone) {
            Event::Start {
                kind,
                forward_parent,
            } => {
                // A node preceded by later ones: start the outermost first.
                forward.push(kind);
                let mut index = i;
                let mut next = forward_parent;
                while let Some(step) = next {
                    index += step as usize;
                    next = match events
                        .get_mut(index)
                        .map(|e| std::mem::replace(e, Event::Tombstone))
                    {
                        Some(Event::Start {
                            kind,
                            forward_parent,
                        }) => {
                            forward.push(kind);
                            forward_parent
                        }
                        _ => None,
                    };
                }
                for kind in forward.drain(..).rev() {
                    sink.start_node(kind);
                }
            }
            Event::Finish => sink.finish_node(),
            Event::Token { kind } => sink.token(kind),
            Event::Tombstone => {}
        }
    }
    sink.finish()
}

struct Sink<'a> {
    text: &'a str,
    tokens: &'a [LexToken],
    /// The next raw token.
    raw: usize,
    offset: usize,
    builder: GreenNodeBuilder<'static>,
    depth: usize,
}

impl Sink<'_> {
    fn raw_kind(&self, i: usize) -> Option<SyntaxKind> {
        self.tokens.get(i).map(|token| token.kind)
    }

    fn emit_raw(&mut self, kind: SyntaxKind) {
        let Some(token) = self.tokens.get(self.raw) else {
            return;
        };
        let end = self.offset + token.len as usize;
        let text = self.text.get(self.offset..end).unwrap_or("");
        self.builder.token(kind.into_raw(), text);
        self.offset = end;
        self.raw += 1;
    }

    /// The text of the next raw token.
    fn next_raw_text(&self) -> &str {
        let len = self.tokens.get(self.raw).map_or(0, |t| t.len as usize);
        self.text.get(self.offset..self.offset + len).unwrap_or("")
    }

    /// Emits trivia up to the first comment: whitespace that belongs
    /// outside the next node.
    fn emit_leading_whitespace(&mut self) {
        while self.raw_kind(self.raw) == Some(WHITESPACE) {
            self.emit_raw(WHITESPACE);
        }
    }

    fn emit_all_trivia(&mut self) {
        while let Some(kind) = self.raw_kind(self.raw) {
            if !kind.is_trivia() {
                break;
            }
            self.emit_raw(kind);
        }
    }

    /// A comment on the same line as the token just emitted.
    fn emit_trailing_comment(&mut self) {
        let mut i = self.raw;
        if self.raw_kind(i) == Some(WHITESPACE) {
            if self.next_raw_text().contains('\n') {
                return;
            }
            i += 1;
        }
        if matches!(self.raw_kind(i), Some(COMMENT | DOC_COMMENT)) {
            while self.raw < i {
                self.emit_raw(WHITESPACE);
            }
            let kind = self.raw_kind(self.raw).unwrap_or(COMMENT);
            self.emit_raw(kind);
        }
    }

    fn start_node(&mut self, kind: SyntaxKind) {
        if self.depth > 0 {
            self.emit_leading_whitespace();
        }
        self.builder.start_node(kind.into_raw());
        self.depth += 1;
        if self.depth > 1 {
            self.emit_all_trivia();
        }
    }

    fn finish_node(&mut self) {
        if self.depth == 1 {
            // Closing the root: keep everything left over inside it.
            self.finish_rest();
        }
        if self.depth > 0 {
            self.builder.finish_node();
            self.depth -= 1;
        }
    }

    fn token(&mut self, kind: SyntaxKind) {
        self.emit_all_trivia();
        if self.raw >= self.tokens.len() {
            return;
        }
        self.emit_raw(kind);
        self.emit_trailing_comment();
    }

    /// Unconsumed tokens go in one error node; trailing trivia stays loose.
    fn finish_rest(&mut self) {
        self.emit_leading_whitespace();
        let rest_has_tokens = self.tokens[self.raw..].iter().any(|t| !t.kind.is_trivia());
        if rest_has_tokens {
            self.builder.start_node(ERROR.into_raw());
            while let Some(kind) = self.raw_kind(self.raw) {
                self.emit_raw(kind);
            }
            self.builder.finish_node();
        }
        self.emit_all_trivia();
    }

    fn finish(mut self) -> GreenNode {
        while self.depth > 0 {
            self.finish_node();
        }
        self.builder.finish()
    }
}

impl SyntaxKind {
    pub(crate) fn into_raw(self) -> rowan::SyntaxKind {
        rowan::SyntaxKind(self as u16)
    }
}
