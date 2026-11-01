//! Search on PostgreSQL (area 16, "Search"; chunk B12): the column an
//! entity's searchable words are kept in, the query a `SearchText` is
//! read as, and how well a record matches.
//!
//! - The column is a stored generated `tsvector`
//!   ([`jadpo_std::search::COLUMN`]), so PostgreSQL works it out from the
//!   fields on every write and it can't disagree with them: each field's
//!   words under `to_tsvector` in the entity's language, a field that is
//!   `none` giving none, and weighted `A` to `D` by where it is listed.
//! - The query is `websearch_to_tsquery`, which takes words, quoted
//!   phrases and `-word` and never fails on what it is given; the runtime
//!   makes `or` a word first ([`jadpo_std::search::phrase`]).
//! - The rank is `ts_rank`, which reads only the record's own words and
//!   the query, never other records', so a record a caller can't see
//!   never moves one they can (area 9): cast to `float8`, which holds its
//!   `real` exactly, so a page's cursor carries it unchanged.

use jadpo_std::SearchLanguage;
use jadpo_std::search::{COLUMN, weight};

use crate::{ident, text_literal};

/// The language as the `regconfig` PostgreSQL names it: `'english'::regconfig`.
pub fn config(language: SearchLanguage) -> String {
    format!("{}::regconfig", text_literal(language.name()))
}

/// The generated column's expression: each of `columns` (a text field's
/// column, in `fields:` order) as words in `language`, weighted by its
/// place, joined into one `tsvector`.
pub fn document<S: AsRef<str>>(language: SearchLanguage, columns: &[S]) -> String {
    columns
        .iter()
        .enumerate()
        .map(|(at, column)| {
            format!(
                "setweight(to_tsvector({}, coalesce({}, '')), '{}')",
                config(language),
                ident(column.as_ref()),
                weight(at)
            )
        })
        .collect::<Vec<_>>()
        .join(" || ")
}

/// The column's type and how it is generated, as a column definition
/// writes it after its name.
pub fn column_type(language: SearchLanguage, columns: &[impl AsRef<str>]) -> String {
    format!(
        "tsvector GENERATED ALWAYS AS ({}) STORED",
        document(language, columns)
    )
}

/// What `phrase` (a parameter holding the phrase) asks for, in `language`.
pub fn query(language: SearchLanguage, phrase: &str) -> String {
    format!("websearch_to_tsquery({}, {phrase})", config(language))
}

/// `matches`: the record's words meet the query.
pub fn matches(query: &str) -> String {
    format!("{} @@ {query}", ident(COLUMN))
}

/// How well the record matches the query, as a `float8`.
pub fn rank(query: &str) -> String {
    format!("ts_rank({}, {query})::float8", ident(COLUMN))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fields_listed_first_weigh_most() {
        assert_eq!(
            column_type(SearchLanguage::English, &["title", "notes"]),
            "tsvector GENERATED ALWAYS AS (setweight(to_tsvector('english'::regconfig, coalesce(\"title\", '')), 'A') || setweight(to_tsvector('english'::regconfig, coalesce(\"notes\", '')), 'B')) STORED"
        );
        let query = query(SearchLanguage::Simple, "$2");
        assert_eq!(query, "websearch_to_tsquery('simple'::regconfig, $2)");
        assert_eq!(
            matches(&query),
            "\"jadpo_search\" @@ websearch_to_tsquery('simple'::regconfig, $2)"
        );
        assert_eq!(
            rank(&query),
            "ts_rank(\"jadpo_search\", websearch_to_tsquery('simple'::regconfig, $2))::float8"
        );
    }
}
