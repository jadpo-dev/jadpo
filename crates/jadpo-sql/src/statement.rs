//! The statements the plan runs (area 6, "Named queries", "The operations
//! every entity gets"; area 5, "Lifecycle"): reads with their includes and
//! pages, writes, and a lifecycle's purge, rendered from the columns,
//! conditions and parameters the plan chose. Each statement's text is fixed
//! when the project is compiled, so the same operation always sends the
//! same statement.
//!
//! Clauses come one to a line, an inner statement two spaces in, so
//! `jadpo sql` prints a statement as it is sent.

use crate::{ident, idents};

/// `SELECT` of `columns` from `table`, each line led by `indent`.
pub fn select<S: AsRef<str>>(columns: &[S], table: &str, indent: &str) -> String {
    format!(
        "{indent}SELECT {}\n{indent}FROM {}",
        idents(columns),
        ident(table)
    )
}

/// `SELECT` of `columns` and whether `grant` holds for each row, the last
/// column, from `table`: a read that tells a row policy hides from one that
/// isn't there in one statement.
pub fn select_granted<S: AsRef<str>>(columns: &[S], grant: &str, table: &str) -> String {
    format!(
        "SELECT {}, {} IS TRUE AS \"jadpo_permitted\"\nFROM {}",
        idents(columns),
        parenthesised(grant),
        ident(table)
    )
}

/// `condition` in parentheses, unless one pair already holds all of it.
fn parenthesised(condition: &str) -> String {
    let mut depth = 0usize;
    let mut closes_at_end = false;
    for (at, c) in condition.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    closes_at_end = at + 1 == condition.len();
                    if !closes_at_end {
                        break;
                    }
                }
            }
            _ if depth == 0 => break,
            _ => {}
        }
    }
    if condition.starts_with('(') && closes_at_end {
        condition.to_string()
    } else {
        format!("({condition})")
    }
}

/// The column a nearest-neighbour read selects its distance as.
pub const DISTANCE: &str = "jadpo_distance";

/// A nearest-neighbour read (area 21, chunk 5.5v1b): `columns` of `table`
/// and the `distance` expression as [`DISTANCE`], where `conditions`
/// hold, nearest first by the `order` expression alone and at most
/// `limit`, the one order an HNSW index serves; then the rows it found
/// ordered again by the distance and `tie` (the `id`), outside, since
/// PostgreSQL reads no HNSW index for an order with a second key. `order`
/// is the distance itself, or for a vector indexed at half precision the
/// same distance between `halfvec` casts, as its index holds (chunk
/// 5.5v1c).
pub fn nearest<S: AsRef<str>>(
    columns: &[S],
    table: &str,
    distance: &str,
    order: &str,
    conditions: &[String],
    limit: &str,
    tie: &str,
) -> String {
    let mut inner = format!(
        "  SELECT {}, {distance} AS {}\n  FROM {}",
        idents(columns),
        ident(DISTANCE),
        ident(table)
    );
    where_clause(&mut inner, conditions, "  ");
    inner.push_str(&format!("\n  ORDER BY {order}\n  LIMIT {limit}"));
    format!(
        "SELECT *\nFROM (\n{inner}\n) AS \"nearest\"\nORDER BY {}, {}",
        ident(DISTANCE),
        ident(tie)
    )
}

/// The distance by `metric` from `from` (a parameter) to `column`'s vector
/// of `dimensions` items, both cast to half precision, as an index on a
/// vector of 2,001 to 4,000 dimensions holds it (area 21, chunk 5.5v1c):
/// the parameter is cast to `vector(n)` first, so the server infers
/// `vector` for it, the binary format the runtime sends.
pub fn half_precision_distance(
    column: &str,
    metric: jadpo_std::VectorDistance,
    from: &str,
    dimensions: u64,
) -> String {
    let n = dimensions;
    format!(
        "{}::halfvec({n}) {} {from}::vector({n})::halfvec({n})",
        ident(column),
        metric.operator()
    )
}

/// The settings a nearest-neighbour read runs with first, for the rest of
/// its transaction (area 21, chunks 5.5v1b and 5.8c): `ef_search`
/// candidates, and pgvector's iterative scans in strict order, visiting at
/// most `max_scan_tuples` records, so a filtered search still fills its
/// limit (pgvector 0.8.0 and later).
pub fn search_settings(ef_search: u64, max_scan_tuples: u32) -> Vec<String> {
    vec![
        format!("SET LOCAL hnsw.ef_search = {ef_search}"),
        "SET LOCAL hnsw.iterative_scan = strict_order".to_string(),
        format!("SET LOCAL hnsw.max_scan_tuples = {max_scan_tuples}"),
    ]
}

/// An exact nearest-neighbour read, for a vector too long for an index
/// (area 21, chunk 5.5v1c): every row where `conditions` hold measured,
/// then ordered by the distance and `tie` in one statement, since no
/// index is read.
pub fn nearest_exact<S: AsRef<str>>(
    columns: &[S],
    table: &str,
    distance: &str,
    conditions: &[String],
    limit: &str,
    tie: &str,
) -> String {
    let mut sql = format!(
        "SELECT {}, {distance} AS {}\nFROM {}",
        idents(columns),
        ident(DISTANCE),
        ident(table)
    );
    where_clause(&mut sql, conditions, "");
    sql.push_str(&format!(
        "\nORDER BY {}, {}\nLIMIT {limit}",
        ident(DISTANCE),
        ident(tie)
    ));
    sql
}

/// The column a read ordered by `geo.distance` selects PostGIS's distance
/// as (area 20, chunk 5.5u5).
pub const MEASURED: &str = "jadpo_measured";

/// A read ordered by `geo.distance` (area 20, "Queries", chunk 5.5u5):
/// `columns` of `table` and the `measured` distance as [`MEASURED`], where
/// `conditions` hold, nearest first by it and then `tie` (the `id`), at
/// most `limit`. The runtime orders the rows again by the distance Jadpo
/// works out, which the margin the conditions search keeps in reach.
pub fn by_distance<S: AsRef<str>>(
    columns: &[S],
    table: &str,
    measured: &str,
    conditions: &[String],
    limit: &str,
    tie: &str,
) -> String {
    let mut sql = format!(
        "SELECT {}, {measured} AS {}\nFROM {}",
        idents(columns),
        ident(MEASURED),
        ident(table)
    );
    where_clause(&mut sql, conditions, "");
    sql.push_str(&format!(
        "\nORDER BY {}, {}\nLIMIT {limit}",
        ident(MEASURED),
        ident(tie)
    ));
    sql
}

/// The alias of an aggregate read's grouped rows.
const GROUPED: &str = "grouped";

/// An aggregate read (area 6, "Counts, sums and groups"; chunk 5.6c), one
/// statement however many records it counts: inside, each of `columns`
/// (an expression and the name it is selected as) from `table` where
/// `conditions` hold, `GROUP BY` the `groups` expressions when there are
/// any; outside, each of `rows` (an expression of the inside's names and
/// the name it is selected as), ordered by `order` (an expression of the
/// inside's names and whether it sorts descending) and at most `most`
/// rows. Outside, a value worked out from several totals (an average from
/// its sum and its count) names each of them once.
pub fn grouped(
    columns: &[(String, String)],
    table: &str,
    conditions: &[String],
    groups: &[String],
    rows: &[(String, String)],
    order: &[(String, bool)],
    most: Option<&str>,
) -> String {
    let selected: Vec<String> = columns
        .iter()
        .map(|(expr, name)| format!("    {expr} AS {}", ident(name)))
        .collect();
    let mut inner = format!(
        "  SELECT\n{}\n  FROM {}",
        selected.join(",\n"),
        ident(table)
    );
    where_clause(&mut inner, conditions, "  ");
    if !groups.is_empty() {
        inner.push_str(&format!("\n  GROUP BY {}", groups.join(", ")));
    }
    let outside: Vec<String> = rows
        .iter()
        .map(|(expr, name)| {
            if *expr == ident(name) {
                expr.clone()
            } else {
                format!("{expr} AS {}", ident(name))
            }
        })
        .collect();
    let mut sql = format!(
        "SELECT {}\nFROM (\n{inner}\n) AS {}",
        outside.join(", "),
        ident(GROUPED)
    );
    if !order.is_empty() {
        let terms: Vec<String> = order
            .iter()
            .map(|(expr, descending)| {
                format!("{expr} {}", if *descending { "DESC" } else { "ASC" })
            })
            .collect();
        sql.push_str(&format!("\nORDER BY {}", terms.join(", ")));
    }
    if let Some(n) = most {
        limit(&mut sql, n, "");
    }
    sql
}

/// `SELECT 1` from `table`: whether a row is there.
pub fn select_one(table: &str) -> String {
    format!("SELECT 1\nFROM {}", ident(table))
}

/// The row version (PostgreSQL's `xmin`, as text) of each of `table`'s
/// rows whose `id` is among the array `$1`, with its `id`: what a snapshot
/// read before an outside call loaded, read again in that snapshot before
/// it closes (area 6, "Outside calls hold no transaction"; chunk 7.4).
pub fn row_versions(table: &str) -> String {
    format!(
        "SELECT {}, {}::text\nFROM {}\nWHERE {} = ANY($1)",
        ident("id"),
        ident("xmin"),
        ident(table),
        ident("id")
    )
}

/// The row version of `table`'s row whose `id` is `$1`, locked as an
/// update locks it: what a write after an outside call compares with the
/// version loaded before it (chunk 7.4).
pub fn row_version(table: &str) -> String {
    format!(
        "SELECT {}::text\nFROM {}\nWHERE {} = $1\nFOR NO KEY UPDATE",
        ident("xmin"),
        ident(table),
        ident("id")
    )
}

/// A mutation's expected pre-call version, or typed NULL when it needs
/// no check. The UPDATE or DELETE both checks and locks its matching row.
pub fn holds_row_version(expected: &str) -> String {
    format!("({expected}::text IS NULL OR \"xmin\"::text = {expected})")
}

/// Adds `conditions` to `sql` as its `WHERE`, one to a line.
pub fn where_clause(sql: &mut String, conditions: &[String], indent: &str) {
    for (i, condition) in conditions.iter().enumerate() {
        let word = if i == 0 { "WHERE" } else { "  AND" };
        sql.push_str(&format!("\n{indent}{word} {condition}"));
    }
}

/// One field of an order: its column, whether it sorts text by code point
/// (`COLLATE "C"`, as Jadpo compares text), and its direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Order<'a> {
    pub column: &'a str,
    pub collate: bool,
    pub descending: bool,
}

/// An order's fields, `qualifier.`-prefixed when one is given.
fn order_terms(order: &[Order<'_>], qualifier: Option<&str>) -> String {
    order
        .iter()
        .map(|field| {
            let prefix = qualifier.map_or_else(String::new, |q| format!("{}.", ident(q)));
            format!(
                "{prefix}{}{} {}",
                ident(field.column),
                collate(field.collate),
                if field.descending { "DESC" } else { "ASC" }
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// ` COLLATE "C"` when `text` sorts by code point.
fn collate(text: bool) -> &'static str {
    if text { " COLLATE \"C\"" } else { "" }
}

/// Adds `ORDER BY` of `order` to `sql`.
pub fn order_by(sql: &mut String, order: &[Order<'_>], indent: &str) {
    sql.push_str(&format!("\n{indent}ORDER BY {}", order_terms(order, None)));
}

/// Adds `LIMIT` of `n` (a number or a parameter) to `sql`.
pub fn limit(sql: &mut String, n: &str, indent: &str) {
    sql.push_str(&format!("\n{indent}LIMIT {n}"));
}

/// `column = value`, a value as SQL: a condition, or a `SET` assignment.
pub fn equal(column: &str, value: &str) -> String {
    format!("{} = {value}", ident(column))
}

/// `column < value`.
pub fn less_than(column: &str, value: &str) -> String {
    format!("{} < {value}", ident(column))
}

/// `column = ANY(keys)`: the rows an include's keys match.
pub fn any(column: &str, keys: &str) -> String {
    format!("{} = ANY({keys})", ident(column))
}

/// One field of the cursor a page continues after: its column, its
/// parameter, whether it sorts text by code point, and its direction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct After<'a> {
    pub column: &'a str,
    pub param: String,
    pub collate: bool,
    pub descending: bool,
}

/// The condition that starts a page after its cursor: the rows past the
/// cursor's values of the order's fields, or every row when there is no
/// cursor (its first parameter `NULL`); `None` for an order of no fields.
pub fn after(parts: &[After<'_>]) -> Option<String> {
    let first = parts.first()?;
    let column = |part: &After<'_>| format!("{}{}", ident(part.column), collate(part.collate));
    let param = |part: &After<'_>| format!("{}{}", part.param, collate(part.collate));
    let past_op = |part: &After<'_>| if part.descending { "<" } else { ">" };
    let same = parts.iter().all(|p| p.descending == first.descending);
    let past = if same {
        let op = past_op(first);
        if parts.len() == 1 {
            format!("{} {op} {}", column(first), param(first))
        } else {
            format!(
                "({}) {op} ({})",
                parts.iter().map(column).collect::<Vec<_>>().join(", "),
                parts.iter().map(param).collect::<Vec<_>>().join(", ")
            )
        }
    } else {
        // Mixed directions: past on the first field, or equal on it and
        // past on the next, and so on.
        let mut alternatives = Vec::new();
        for (i, part) in parts.iter().enumerate() {
            let mut terms: Vec<String> = parts
                .iter()
                .take(i)
                .map(|p| format!("{} = {}", column(p), param(p)))
                .collect();
            terms.push(format!(
                "{} {} {}",
                column(part),
                past_op(part),
                param(part)
            ));
            alternatives.push(if terms.len() == 1 {
                terms.remove(0)
            } else {
                format!("({})", terms.join(" AND "))
            });
        }
        format!("({})", alternatives.join(" OR "))
    };
    Some(format!("({} IS NULL OR {past})", first.param))
}

/// The alias of an include's keys and of each one's rows.
const KEY: &str = "key";
const ROW: &str = "row";

/// `column` matching the key a children include's row is read for.
pub fn matches_key(column: &str) -> String {
    format!("{} = {}.{}", ident(column), ident(KEY), ident("id"))
}

/// A children include: for each of `keys`, the rows of `inner` (which
/// orders and limits each record's list on its own, its first column the
/// key), as the key then each of `columns`, ordered by key, then `order`.
pub fn children(keys: &str, columns: &[&str], inner: &str, order: &[Order<'_>]) -> String {
    let key = format!("{}.{}", ident(KEY), ident("id"));
    let outer: Vec<String> = std::iter::once(key.clone())
        .chain(
            columns
                .iter()
                .map(|column| format!("{}.{}", ident(ROW), ident(column))),
        )
        .collect();
    format!(
        "SELECT {}\nFROM unnest({keys}) AS {}({})\nCROSS JOIN LATERAL (\n{inner}\n) AS {}\nORDER BY {key}, {}",
        outer.join(", "),
        ident(KEY),
        ident("id"),
        ident(ROW),
        order_terms(order, Some(ROW))
    )
}

/// `INSERT` of `values` into `columns` of `table`, returning `returning`.
pub fn insert<S: AsRef<str>>(
    table: &str,
    columns: &[&str],
    values: &[String],
    returning: &[S],
) -> String {
    format!(
        "INSERT INTO {} ({})\nVALUES ({})\nRETURNING {}",
        ident(table),
        idents(columns),
        values.join(", "),
        idents(returning)
    )
}

/// `UPDATE` of `table` with `assignments` ([`equal`], [`set_when`]), one
/// to a line.
pub fn update(table: &str, assignments: &[String]) -> String {
    format!(
        "UPDATE {}\nSET {}",
        ident(table),
        assignments.join(",\n    ")
    )
}

/// `DELETE` from `table`.
pub fn delete(table: &str) -> String {
    format!("DELETE FROM {}", ident(table))
}

/// Adds `RETURNING` of `columns` to `sql`.
pub fn returning<S: AsRef<str>>(sql: &mut String, columns: &[S]) {
    sql.push_str(&format!("\nRETURNING {}", idents(columns)));
}

/// `column = CASE WHEN when THEN value ELSE column END`: `column` set to
/// `value` only when `when` holds, as a patch sets a field it supplied.
pub fn set_when(column: &str, when: &str, value: &str) -> String {
    let column = ident(column);
    format!("{column} = CASE WHEN {when} THEN {value} ELSE {column} END")
}

/// A column's own value, as a `SET` that changes nothing assigns it.
pub fn column(name: &str) -> String {
    ident(name)
}

/// `lhs + rhs`, or `lhs || rhs` joining text, worked out by the database.
pub fn plus(lhs: &str, rhs: &str, text: bool) -> String {
    format!("({lhs} {} {rhs})", if text { "||" } else { "+" })
}

/// `lhs - rhs`.
pub fn minus(lhs: &str, rhs: &str) -> String {
    format!("({lhs} - {rhs})")
}

/// `-operand`.
pub fn negate(operand: &str) -> String {
    format!("(-{operand})")
}

/// One purge batch: `DELETE` from `table` of at most `batch` rows meeting
/// `conditions`, oldest `since` first, locked without waiting on any
/// another transaction holds, returning their `id`s.
pub fn purge(table: &str, conditions: &[String], since: &str, batch: u32) -> String {
    let mut inner = format!("  SELECT {}\n  FROM {}", ident("id"), ident(table));
    where_clause(&mut inner, conditions, "  ");
    inner.push_str(&format!(
        "\n  ORDER BY {} ASC, {} ASC\n  LIMIT {batch}\n  FOR UPDATE SKIP LOCKED",
        ident(since),
        ident("id")
    ));
    format!(
        "DELETE FROM {}\nWHERE {} IN (\n{inner}\n)\nRETURNING {}",
        ident(table),
        ident("id"),
        ident("id")
    )
}

/// `NOT EXISTS` a record holding a row of `table` through `path`: each
/// step a table and its column referring to the step before, the first to
/// `table`.
pub fn not_held(table: &str, path: &[(&str, &str)]) -> String {
    let mut from = Vec::new();
    let mut first_link = String::new();
    for (i, (holder, column)) in path.iter().enumerate() {
        let alias = ident(&format!("held{}", i + 1));
        let column = ident(column);
        if i == 0 {
            from.push(format!("{} AS {alias}", ident(holder)));
            first_link = format!("{alias}.{column} = {}.{}", ident(table), ident("id"));
        } else {
            from.push(format!(
                "JOIN {} AS {alias} ON {alias}.{column} = {}.{}",
                ident(holder),
                ident(&format!("held{i}")),
                ident("id")
            ));
        }
    }
    format!(
        "NOT EXISTS (\n      SELECT 1\n      FROM {}\n      WHERE {first_link}\n    )",
        from.join("\n      ")
    )
}

/// A cascading reference a purge follows: the entity it leaves from and
/// the one it reaches, by their numbers among those reached, and the
/// reaching table and its column naming the record it leaves from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cascade<'a> {
    pub from: usize,
    pub to: usize,
    pub table: &'a str,
    pub column: &'a str,
}

/// A restricting reference that holds a record of the entity numbered
/// `entity` among those reached: the holding table and its column.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hold<'a> {
    pub entity: usize,
    pub table: &'a str,
    pub column: &'a str,
}

/// `NOT EXISTS` a record a restricting reference holds among those
/// deleting a row of `table` (entity 0) would cascade to, however deep and
/// wherever the cascades lead back: a recursive query walks `cascades`
/// from the row, each record once, so a loop in the data ends too, then
/// asks whether any of `holds` holds one. `TRUE` when nothing can.
pub fn not_held_anywhere(table: &str, cascades: &[Cascade<'_>], holds: &[Hold<'_>]) -> String {
    if holds.is_empty() {
        return "TRUE".to_string();
    }
    let edges: Vec<String> = cascades
        .iter()
        .enumerate()
        .map(|(i, cascade)| {
            let alias = ident(&format!("edge{}", i + 1));
            format!(
                "SELECT {} AS \"parent\", {alias}.{} AS \"parent_id\", {} AS \"entity\", {alias}.\"id\" AS \"id\" FROM {} AS {alias}",
                cascade.from,
                ident(cascade.column),
                cascade.to,
                ident(cascade.table)
            )
        })
        .collect();
    let held: Vec<String> = holds
        .iter()
        .enumerate()
        .map(|(i, hold)| {
            let alias = ident(&format!("held{}", i + 1));
            format!(
                "(\"reach\".\"entity\" = {} AND EXISTS (SELECT 1 FROM {} AS {alias} WHERE {alias}.{} = \"reach\".\"id\"))",
                hold.entity,
                ident(hold.table),
                ident(hold.column)
            )
        })
        .collect();
    format!(
        "NOT EXISTS (\n      WITH RECURSIVE \"reach\" (\"entity\", \"id\") AS (\n        SELECT 0, {}.\"id\"\n        UNION\n        SELECT \"edge\".\"entity\", \"edge\".\"id\"\n        FROM \"reach\"\n        JOIN (\n          {}\n        ) AS \"edge\" ON \"edge\".\"parent\" = \"reach\".\"entity\" AND \"edge\".\"parent_id\" = \"reach\".\"id\"\n      )\n      SELECT 1\n      FROM \"reach\"\n      WHERE {}\n    )",
        ident(table),
        edges.join("\n          UNION ALL\n          "),
        held.join("\n        OR "),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_starts_after_its_cursor_in_each_direction() {
        let part = |column, param: &str, descending| After {
            column,
            param: param.to_string(),
            collate: false,
            descending,
        };
        assert_eq!(after(&[]), None);
        assert_eq!(
            after(&[part("due", "$2", false)]).as_deref(),
            Some("($2 IS NULL OR \"due\" > $2)")
        );
        assert_eq!(
            after(&[part("due", "$2", false), part("id", "$3", false)]).as_deref(),
            Some("($2 IS NULL OR (\"due\", \"id\") > ($2, $3))")
        );
        assert_eq!(
            after(&[part("due", "$2", true), part("id", "$3", false)]).as_deref(),
            Some("($2 IS NULL OR (\"due\" < $2 OR (\"due\" = $2 AND \"id\" > $3)))")
        );
    }
}
