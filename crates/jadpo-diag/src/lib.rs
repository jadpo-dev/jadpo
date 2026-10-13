//! The rule catalogue and the diagnostic model (docs/compiler-runtime.md §2.2).
//!
//! Every rule is declared once in the `rules!` table below and used through
//! its typed constant in [`rule`], so a misspelt rule does not compile. A
//! [`Diagnostic`] carries the rule, the spans it points at, the reason, the
//! recommended next step with any edits, and who decides. `jadpo-term`
//! renders it; nothing here knows about terminals.

use std::fmt;

/// How serious a rule is. The severity word in a headline comes from here.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    Error,
    Warning,
}

impl Severity {
    /// The lower-case word used in headlines and JSON.
    pub fn word(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
        }
    }
}

/// Who acts on a diagnostic's next step (area 14): a coding agent can make
/// the change, or a person has to decide.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Decider {
    Agent,
    Human,
}

impl Decider {
    pub fn word(self) -> &'static str {
        match self {
            Decider::Agent => "agent",
            Decider::Human => "human",
        }
    }
}

/// One entry in the rule catalogue.
#[derive(Debug, PartialEq, Eq)]
pub struct Rule {
    /// `area.name`, lower case, for example `cli.unknown_command`.
    pub id: &'static str,
    /// The language area (1–18) whose page documents the rule.
    pub area: u8,
    pub severity: Severity,
    /// Who usually decides the next step.
    pub decides: Decider,
    /// The rule's entry on the language pages, relative to `docs/language/`.
    pub anchor: &'static str,
    /// One line, sentence case, no full stop.
    pub summary: &'static str,
}

impl Rule {
    /// The help link printed on the `help` line.
    pub fn help(&self) -> String {
        format!("docs/language/{}", self.anchor)
    }
}

impl fmt::Display for Rule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.id)
    }
}

macro_rules! rules {
    ($($name:ident: $id:literal, area $area:literal, $severity:ident, $decides:ident, $anchor:literal, $summary:literal;)*) => {
        /// Typed constants for every rule in the catalogue.
        pub mod rule {
            use super::{Decider, Rule, Severity};
            $(
                #[doc = $summary]
                pub const $name: Rule = Rule {
                    id: $id,
                    area: $area,
                    severity: Severity::$severity,
                    decides: Decider::$decides,
                    anchor: $anchor,
                    summary: $summary,
                };
            )*
        }

        /// Every rule, sorted by id.
        pub static CATALOGUE: &[Rule] = &[$(rule::$name),*];
    };
}

rules! {
    CLI_COMPLETIONS_ARGUMENTS: "cli.completions_arguments", area 14, Error, Agent,
        "14-tooling.html#command-errors", "Completions command has invalid arguments";
    CLI_FMT_ARGUMENTS: "cli.fmt_arguments", area 14, Error, Agent,
        "14-tooling.html#command-errors", "Format command has invalid arguments";
    CLI_HELP_ARGUMENTS: "cli.help_arguments", area 14, Error, Agent,
        "14-tooling.html#command-errors", "Help command has invalid arguments";
    CLI_PRESENTATION_ARGUMENTS: "cli.presentation_arguments", area 14, Error, Agent,
        "14-tooling.html#command-errors", "Terminal presentation option is invalid";
    CLI_UNKNOWN_COMMAND: "cli.unknown_command", area 14, Error, Agent,
        "14-tooling.html#command-errors", "Unknown Jadpo command";
    CLI_UNKNOWN_FLAG: "cli.unknown_flag", area 14, Error, Agent,
        "14-tooling.html#command-errors", "Unknown flag";
    CLI_VERSION_ARGUMENTS: "cli.version_arguments", area 14, Error, Agent,
        "14-tooling.html#command-errors", "Version command has invalid arguments";
    FORMAT_CHANGES_REQUIRED: "format.changes_required", area 14, Error, Agent,
        "14-tooling.html#formatting", "Source file is not formatted";
    FORMAT_WRITE_FAILED: "format.write_failed", area 14, Error, Human,
        "14-tooling.html#formatting", "Formatted source could not be written";
    SYNTAX_ATTEMPT_PLACEMENT: "syntax.attempt_placement", area 3, Error, Agent,
        "03-logic.html#syntax-attempt-placement", "Attempt is used inside an expression";
    SYNTAX_COLON_REQUIRED: "syntax.colon_required", area 3, Error, Agent,
        "03-logic.html#syntax-colon-required", "Member has no colon before its value";
    SYNTAX_CONDITION_BRACKETS: "syntax.condition_brackets", area 3, Error, Agent,
        "03-logic.html#syntax-condition-brackets", "Condition is not in brackets";
    SYNTAX_EXPECTED_DECLARATION: "syntax.expected_declaration", area 1, Error, Agent,
        "01-project-layout-and-modules.html#syntax-expected-declaration", "Expected a declaration";
    SYNTAX_EXPECTED_EXPRESSION: "syntax.expected_expression", area 3, Error, Agent,
        "03-logic.html#syntax-expected-expression", "Expected a value";
    SYNTAX_EXPECTED_NAME: "syntax.expected_name", area 1, Error, Agent,
        "01-project-layout-and-modules.html#syntax-expected-name", "Expected a name";
    SYNTAX_EXPECTED_STATEMENT: "syntax.expected_statement", area 3, Error, Agent,
        "03-logic.html#syntax-expected-statement", "Expected a statement";
    SYNTAX_FAILS_AFTER_RESULT: "syntax.fails_after_result", area 4, Error, Agent,
        "04-failures.html#syntax-fails-after-result", "Fails list comes after the result type";
    SYNTAX_INVALID_ESCAPE: "syntax.invalid_escape", area 1, Error, Agent,
        "01-project-layout-and-modules.html#syntax-invalid-escape", "Unknown escape in a string";
    SYNTAX_MATCH_ARROW: "syntax.match_arrow", area 3, Error, Agent,
        "03-logic.html#syntax-match-arrow", "Match arm uses `=>`";
    SYNTAX_MATCH_PATTERN: "syntax.match_pattern", area 3, Error, Agent,
        "03-logic.html#syntax-match-pattern", "Expected a match pattern";
    SYNTAX_NAMED_ARGUMENT_DUPLICATE: "syntax.named_argument_duplicate", area 3, Error, Agent,
        "03-logic.html#syntax-named-argument-duplicate", "Named argument is given twice";
    SYNTAX_POSITIONAL_AFTER_NAMED: "syntax.positional_after_named", area 3, Error, Agent,
        "03-logic.html#syntax-positional-after-named", "Positional argument follows a named one";
    SYNTAX_TYPE_BLOCK_REQUIRED: "syntax.type_block_required", area 2, Error, Agent,
        "02-values-and-types.html#syntax-type-block-required", "Type has no block";
    SYNTAX_TYPE_PARENT_REQUIRED: "syntax.type_parent_required", area 2, Error, Agent,
        "02-values-and-types.html#syntax-type-parent-required", "Type has no base";
    SYNTAX_UNEXPECTED_CHARACTER: "syntax.unexpected_character", area 1, Error, Agent,
        "01-project-layout-and-modules.html#syntax-unexpected-character", "Character is not part of Jadpo";
    SYNTAX_UNEXPECTED_TOKEN: "syntax.unexpected_token", area 1, Error, Agent,
        "01-project-layout-and-modules.html#syntax-unexpected-token", "Something else was expected here";
    SYNTAX_UNSUPPORTED_THROW: "syntax.unsupported_throw", area 3, Error, Agent,
        "03-logic.html#syntax-unsupported-throw", "Jadpo has no `throw`";
    SYNTAX_UNTERMINATED_STRING: "syntax.unterminated_string", area 1, Error, Agent,
        "01-project-layout-and-modules.html#syntax-unterminated-string", "String is not closed";
    SYNTAX_VARIABLE_TYPE_REQUIRED: "syntax.variable_type_required", area 2, Error, Agent,
        "02-values-and-types.html#syntax-variable-type-required", "Variable has no type";
    TOOLCHAIN_PROJECT_NOT_FOUND: "toolchain.project_not_found", area 14, Error, Agent,
        "14-tooling.html#the-jadpo-command", "Project could not be found";
    TOOLCHAIN_SOURCE_READ_FAILED: "toolchain.source_read_failed", area 14, Error, Human,
        "14-tooling.html#the-jadpo-command", "Jadpo source file could not be read";
}

/// Looks a rule up by id, for tools that read ids back from JSON.
pub fn find(id: &str) -> Option<&'static Rule> {
    CATALOGUE.iter().find(|rule| rule.id == id)
}

/// A byte range in one project file. Paths are project-relative.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    pub path: String,
    pub start: u32,
    pub end: u32,
}

impl Span {
    pub fn new(path: impl Into<String>, start: u32, end: u32) -> Span {
        Span {
            path: path.into(),
            start,
            end,
        }
    }
}

/// A span with the few words printed beside its carets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Label {
    pub span: Span,
    pub text: String,
}

/// A text replacement an editor or agent can apply.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edit {
    pub span: Span,
    pub replacement: String,
}

/// The recommended next step.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Next {
    pub text: String,
    pub decides: Decider,
    pub edits: Vec<Edit>,
}

/// One problem, in the shape every Jadpo error shares (area 14).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub rule: &'static Rule,
    /// The headline's subject: what is wrong, in the reader's terms.
    pub message: String,
    pub primary: Option<Label>,
    pub secondary: Vec<Label>,
    pub reason: Option<String>,
    pub next: Option<Next>,
    /// Printed only when it says something the rest does not.
    pub impact: Option<String>,
}

impl Diagnostic {
    pub fn new(rule: &'static Rule, message: impl Into<String>) -> Diagnostic {
        Diagnostic {
            rule,
            message: message.into(),
            primary: None,
            secondary: Vec::new(),
            reason: None,
            next: None,
            impact: None,
        }
    }

    pub fn severity(&self) -> Severity {
        self.rule.severity
    }

    pub fn at(mut self, span: Span, label: impl Into<String>) -> Diagnostic {
        self.primary = Some(Label {
            span,
            text: label.into(),
        });
        self
    }

    pub fn also(mut self, span: Span, label: impl Into<String>) -> Diagnostic {
        self.secondary.push(Label {
            span,
            text: label.into(),
        });
        self
    }

    pub fn reason(mut self, reason: impl Into<String>) -> Diagnostic {
        self.reason = Some(reason.into());
        self
    }

    /// The next step, decided by whoever the rule names.
    pub fn next(mut self, text: impl Into<String>) -> Diagnostic {
        self.next = Some(Next {
            text: text.into(),
            decides: self.rule.decides,
            edits: Vec::new(),
        });
        self
    }

    /// The next step, decided by someone other than the rule's default.
    pub fn next_for(mut self, text: impl Into<String>, decides: Decider) -> Diagnostic {
        self.next = Some(Next {
            text: text.into(),
            decides,
            edits: Vec::new(),
        });
        self
    }

    /// Adds an edit to the next step. Call after [`Diagnostic::next`].
    pub fn edit(mut self, span: Span, replacement: impl Into<String>) -> Diagnostic {
        if let Some(next) = self.next.as_mut() {
            next.edits.push(Edit {
                span,
                replacement: replacement.into(),
            });
        }
        self
    }

    pub fn impact(mut self, impact: impl Into<String>) -> Diagnostic {
        self.impact = Some(impact.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalogue_is_sorted_and_unique() {
        let ids: Vec<&str> = CATALOGUE.iter().map(|rule| rule.id).collect();
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(
            ids, sorted,
            "keep the rules! table sorted by id, with no repeats"
        );
    }

    #[test]
    fn rule_ids_summaries_and_anchors_are_well_formed() {
        for rule in CATALOGUE {
            let (area, name) = rule.id.split_once('.').expect("ids are area.name");
            for part in [area, name] {
                assert!(
                    !part.is_empty() && part.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
                    "{}",
                    rule.id
                );
            }
            assert!(
                (1..=18).contains(&rule.area),
                "{}: area {}",
                rule.id,
                rule.area
            );
            let first = rule.summary.chars().next().expect("summary");
            assert!(
                first.is_uppercase() && !rule.summary.ends_with('.'),
                "{}: {}",
                rule.id,
                rule.summary
            );
            assert!(
                rule.anchor.starts_with(&format!("{:02}-", rule.area))
                    && rule.anchor.contains(".html#"),
                "{}",
                rule.id
            );
        }
    }

    #[test]
    fn find_returns_the_typed_constant() {
        assert_eq!(
            find("cli.unknown_command"),
            Some(&rule::CLI_UNKNOWN_COMMAND)
        );
        assert_eq!(find("cli.no_such_rule"), None);
    }

    #[test]
    fn builder_fills_the_shared_shape() {
        let diagnostic = Diagnostic::new(&rule::CLI_UNKNOWN_COMMAND, "`x` isn't a jadpo command")
            .reason("Jadpo has no command with this name.")
            .next("Run `jadpo help` to see every command");
        assert_eq!(diagnostic.severity(), Severity::Error);
        assert_eq!(
            diagnostic.next.as_ref().map(|next| next.decides),
            Some(Decider::Agent)
        );
        assert_eq!(
            diagnostic.rule.help(),
            "docs/language/14-tooling.html#command-errors"
        );
    }
}
