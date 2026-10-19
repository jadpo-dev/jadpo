//! An aggregate query's parts (area 6, "Counts, sums and groups"; chunk
//! 5.6c): each group's and aggregate's SQL, and how the grouped rows sort,
//! which [`crate::statement::grouped`] puts into one statement. What each
//! keeps to:
//!
//! - `count()` is `count(*)`, a `bigint`.
//! - `sum` is PostgreSQL's exact `sum`, zero for no values, at the field's
//!   places for a decimal; a whole number, a duration and money's minor
//!   units are cast back to `bigint`, so a sum past 64 bits stops the
//!   read, as it is a defect in code.
//! - `min` and `max` are PostgreSQL's, text compared by code point
//!   (`COLLATE "C"`).
//! - An average is the sum at its places and the count of values, which
//!   `decimal.divide`'s SQL form divides, `NULL` when there are none.
//! - A group sorts in Jadpo's own order: text by code point, an enum's
//!   variants in their declared order (`array_position` of the codes they
//!   are stored as).

use crate::{ident, text_literal};

/// How many records the group holds.
pub fn count() -> String {
    "count(*)".to_string()
}

/// How many values `column` holds in the group, its `NULL`s left out.
pub fn count_of(column: &str) -> String {
    format!("count({column})")
}

/// The sum of a whole number's or a duration's values, `0` for none, cast
/// back to `bigint` so a sum past 64 bits stops the read.
pub fn sum_whole(column: &str) -> String {
    format!("coalesce(sum({column}), 0)::bigint")
}

/// The sum of a decimal's or a quantity's values, zero at `places` places
/// for none.
pub fn sum_at(column: &str, places: u32) -> String {
    format!("coalesce(sum({column}), {})", zero(places))
}

/// The sum at `scale` places, an average's dividend.
pub fn sum_rounded(column: &str, scale: u32) -> String {
    format!("round((sum({column}))::numeric, {scale})")
}

/// The least (`max` false) or greatest value, text compared by code point.
pub fn extreme(column: &str, max: bool, text: bool) -> String {
    format!(
        "{}({column}{})",
        if max { "max" } else { "min" },
        if text { " COLLATE \"C\"" } else { "" }
    )
}

/// A count as a divisor: `NULL` for none, so an average of no values is
/// `NULL` rather than a division by zero.
pub fn nonzero(name: &str) -> String {
    format!("NULLIF({}, 0)", ident(name))
}

/// A value bound as text: money's currency, which its type fixes.
pub fn as_text(param: &str) -> String {
    format!("{param}::text")
}

/// How rows sort by a group of text: by code point.
pub fn text_order(alias: &str) -> String {
    format!("{alias} COLLATE \"C\"")
}

/// How rows sort by a group of an enum: its variants in declared order,
/// by the codes they are stored as.
pub fn enum_order(alias: &str, codes: &[&str]) -> String {
    let codes: Vec<String> = codes.iter().map(|code| text_literal(code)).collect();
    format!(
        "array_position(ARRAY[{}]::text[], ({alias})::text)",
        codes.join(", ")
    )
}

/// Zero with `places` places, as a sum of no values at a scale gives it.
fn zero(places: u32) -> String {
    if places == 0 {
        "0".to_string()
    } else {
        format!("0.{}", "0".repeat(places as usize))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_part_is_what_the_query_selected_before() {
        assert_eq!(sum_whole("\"n\""), "coalesce(sum(\"n\"), 0)::bigint");
        assert_eq!(sum_at("\"d\"", 2), "coalesce(sum(\"d\"), 0.00)");
        assert_eq!(sum_at("\"d\"", 0), "coalesce(sum(\"d\"), 0)");
        assert_eq!(extreme("\"t\"", false, true), "min(\"t\" COLLATE \"C\")");
        assert_eq!(nonzero("mean_count"), "NULLIF(\"mean_count\", 0)");
        assert_eq!(
            enum_order("\"s\"", &["open", "it's"]),
            "array_position(ARRAY['open', 'it''s']::text[], (\"s\")::text)"
        );
    }
}
