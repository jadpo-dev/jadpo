//! Syntax errors, in the shape every Jadpo diagnostic shares (area 14).

use jadpo_diag::{Diagnostic, Rule, Span};

/// A replacement of `start..end` with `text`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextEdit {
    pub start: u32,
    pub end: u32,
    pub text: String,
}

impl TextEdit {
    pub fn insert(at: u32, text: impl Into<String>) -> TextEdit {
        TextEdit {
            start: at,
            end: at,
            text: text.into(),
        }
    }

    pub fn replace(start: u32, end: u32, text: impl Into<String>) -> TextEdit {
        TextEdit {
            start,
            end,
            text: text.into(),
        }
    }
}

/// One syntax problem in one file, with byte offsets into that file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyntaxError {
    pub rule: &'static Rule,
    /// The headline: what is wrong, in the reader's terms.
    pub message: String,
    pub start: u32,
    pub end: u32,
    /// The few words beside the carets.
    pub label: String,
    pub secondary: Vec<(u32, u32, String)>,
    pub reason: String,
    pub next: String,
    pub edits: Vec<TextEdit>,
}

impl SyntaxError {
    pub fn new(
        rule: &'static Rule,
        message: impl Into<String>,
        start: u32,
        end: u32,
    ) -> SyntaxError {
        SyntaxError {
            rule,
            message: message.into(),
            start,
            end,
            label: String::new(),
            secondary: Vec::new(),
            reason: String::new(),
            next: String::new(),
            edits: Vec::new(),
        }
    }

    pub fn label(mut self, label: impl Into<String>) -> SyntaxError {
        self.label = label.into();
        self
    }

    pub fn reason(mut self, reason: impl Into<String>) -> SyntaxError {
        self.reason = reason.into();
        self
    }

    pub fn next(mut self, next: impl Into<String>) -> SyntaxError {
        self.next = next.into();
        self
    }

    pub fn edit(mut self, edit: TextEdit) -> SyntaxError {
        self.edits.push(edit);
        self
    }

    pub fn also(mut self, start: u32, end: u32, label: impl Into<String>) -> SyntaxError {
        self.secondary.push((start, end, label.into()));
        self
    }

    /// The error as a diagnostic about the file at `path`.
    pub fn to_diagnostic(&self, path: &str) -> Diagnostic {
        let mut diagnostic = Diagnostic::new(self.rule, self.message.clone())
            .at(Span::new(path, self.start, self.end), self.label.clone());
        for (start, end, label) in &self.secondary {
            diagnostic = diagnostic.also(Span::new(path, *start, *end), label.clone());
        }
        if !self.reason.is_empty() {
            diagnostic = diagnostic.reason(self.reason.clone());
        }
        if !self.next.is_empty() {
            diagnostic = diagnostic.next(self.next.clone());
            for edit in &self.edits {
                diagnostic =
                    diagnostic.edit(Span::new(path, edit.start, edit.end), edit.text.clone());
            }
        }
        diagnostic
    }
}
