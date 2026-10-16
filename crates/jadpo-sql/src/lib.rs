//! PostgreSQL text (docs/compiler-runtime.md §2.1): every condition,
//! statement and table definition the compiler writes is rendered here,
//! from parts the checker, the graph's migrations and the plan decide, so
//! the SQL has one author and reads the same wherever it lands.
//!
//! - [`condition`]: the model of a condition on one entity's fields (a
//!   query's `where:` lines, a lifecycle's `visible:` and `when:`) and its
//!   renderer, which the plan's statements and the checker's partial
//!   indexes both use, through a [`Bind`] that says where a parameter goes.
//! - [`statement`]: the reads, writes and purges the plan runs.
//! - [`table`]: what a migration does to a store's tables.
//!
//! It depends only on `jadpo-std`, so the graph, the checker and the plan
//! may all depend on it. Nothing here decides what a statement does: a
//! function takes the columns, conditions and values its caller chose, and
//! writes them as PostgreSQL does. Identifiers are always quoted, so a
//! table called `user` or `order` is a name, not a keyword.

pub mod condition;
pub mod policy;
pub mod statement;
pub mod table;

pub use condition::{Bind, Column, Columns, Compare, Condition, Fixed, Value, condition, value};

/// An identifier, always quoted: a table called `user` or `order` is a
/// keyword unquoted.
pub fn ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

/// Identifiers, quoted, joined by `, `.
pub fn idents<S: AsRef<str>>(names: &[S]) -> String {
    names
        .iter()
        .map(|name| ident(name.as_ref()))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Text as a literal: `'it''s'`.
pub fn text_literal(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}

/// The parameter at `position` among a statement's, counted from 0:
/// `$1` for the first.
pub fn placeholder(position: usize) -> String {
    format!("${}", position + 1)
}

/// `NULL`: a field left without a value.
pub const NULL: &str = "NULL";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_are_quoted_and_text_escaped() {
        assert_eq!(ident("order"), "\"order\"");
        assert_eq!(ident("a\"b"), "\"a\"\"b\"");
        assert_eq!(idents(&["id", "title"]), "\"id\", \"title\"");
        assert_eq!(text_literal("it's"), "'it''s'");
        assert_eq!(placeholder(0), "$1");
    }
}
