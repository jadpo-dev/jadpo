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

/// The column a search's ranked read selects its rank as (area 16,
/// "Search"; chunk B12).
pub const RANK: &str = "jadpo_rank";

/// A search's read with no `order:` (area 16, "Search"; chunk B12):
/// `columns` of `table` and the `rank` expression as [`RANK`], where
/// `conditions` hold, the best matches first, then by `tie` (the `id`),
/// at most `limit`.
pub fn ranked<S: AsRef<str>>(
    columns: &[S],
    table: &str,
    rank: &str,
    conditions: &[String],
    limit: Option<&str>,
    tie: &str,
) -> String {
    let mut sql = format!(
        "SELECT {}, {rank} AS {}\nFROM {}",
        idents(columns),
        ident(RANK),
        ident(table)
    );
    where_clause(&mut sql, conditions, "");
    sql.push_str(&format!(
        "\nORDER BY {} DESC, {} ASC",
        ident(RANK),
        ident(tie)
    ));
    if let Some(limit) = limit {
        self::limit(&mut sql, limit, "");
    }
    sql
}

/// The condition that starts a ranked page after its cursor: the records
/// ranked below the last one's `rank_param`, or ranked the same with a
/// greater `tie`; every record when there is no cursor.
pub fn after_rank(rank: &str, rank_param: &str, tie: &str, tie_param: &str) -> String {
    let tie = ident(tie);
    format!(
        "({rank_param} IS NULL OR {rank} < {rank_param} OR ({rank} = {rank_param} AND {tie} > {tie_param}))"
    )
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

/// One retention batch's clearing (area 15, "Retention"): an `UPDATE` of
/// `table` setting each of `cleared` to `NULL` and each of `set`, a
/// column, to the SQL beside it, on at most `batch` rows meeting
/// `conditions`, oldest `since` first, locked without waiting on any
/// another transaction holds, returning their `id`s.
pub fn clear(
    table: &str,
    cleared: &[String],
    set: &[(String, String)],
    conditions: &[String],
    since: &str,
    batch: u32,
) -> String {
    let mut inner = format!("  SELECT {}\n  FROM {}", ident("id"), ident(table));
    where_clause(&mut inner, conditions, "  ");
    inner.push_str(&format!(
        "\n  ORDER BY {} ASC, {} ASC\n  LIMIT {batch}\n  FOR UPDATE SKIP LOCKED",
        ident(since),
        ident("id")
    ));
    let set: Vec<String> = cleared
        .iter()
        .map(|column| format!("{} = NULL", ident(column)))
        .chain(
            set.iter()
                .map(|(column, value)| format!("{} = {value}", ident(column))),
        )
        .collect();
    format!(
        "UPDATE {}\nSET {}\nWHERE {} IN (\n{inner}\n)\nRETURNING {}",
        ident(table),
        set.join(",\n  "),
        ident("id"),
        ident("id")
    )
}

/// Any of `columns` holding a value: a field stored in several columns,
/// such as `Money`, holds one when any of them does.
pub fn any_present(columns: &[&str]) -> String {
    let each: Vec<String> = columns
        .iter()
        .map(|column| format!("{} IS NOT NULL", ident(column)))
        .collect();
    match each.as_slice() {
        [one] => one.clone(),
        _ => format!("({})", each.join(" OR ")),
    }
}

/// `delete`, a `DELETE ... RETURNING "id"`, with the rows of each of
/// `removed`, a table and the column holding the `id` of a record of the
/// entity numbered beside it, deleted in the same statement: a person's
/// sign-in state deleted with them (area 15, "Retention"). Entity 0 is
/// the one `delete` deletes; any other is reached from it through
/// `cascades`, which a recursive query follows from the records deleted,
/// as the statement's snapshot still shows them, each record once
/// (chunk P4). It returns the deleted records' `id`s, as `delete` does.
pub fn with_removed(
    delete: &str,
    cascades: &[Cascade<'_>],
    removed: &[(usize, &str, &str)],
) -> String {
    let mut parts = vec![format!(
        "{} AS (\n{}\n)",
        ident("expired"),
        indented(delete, "  ")
    )];
    let reaching = removed.iter().any(|(entity, ..)| *entity > 0) && !cascades.is_empty();
    if reaching {
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
        parts.push(format!(
            "{} ({}, {}) AS (\n  SELECT 0, {}.{} FROM {}\n  UNION\n  SELECT \"edge\".\"entity\", \"edge\".\"id\"\n  FROM \"reached\"\n  JOIN (\n    {}\n  ) AS \"edge\" ON \"edge\".\"parent\" = \"reached\".\"entity\" AND \"edge\".\"parent_id\" = \"reached\".\"id\"\n)",
            ident("reached"),
            ident("entity"),
            ident("id"),
            ident("expired"),
            ident("id"),
            ident("expired"),
            edges.join("\n    UNION ALL\n    ")
        ));
    }
    for (i, (entity, table, column)) in removed.iter().enumerate() {
        let whose = if reaching {
            format!(
                "SELECT {} FROM {} WHERE {} = {entity}",
                ident("id"),
                ident("reached"),
                ident("entity")
            )
        } else {
            format!("SELECT {} FROM {}", ident("id"), ident("expired"))
        };
        parts.push(format!(
            "{} AS (\n  DELETE FROM {}\n  WHERE {} IN ({whose})\n)",
            ident(&format!("removed{}", i + 1)),
            ident(table),
            ident(column),
        ));
    }
    format!(
        "WITH{} {}\nSELECT {} FROM {}",
        if reaching { " RECURSIVE" } else { "" },
        parts.join(", "),
        ident("id"),
        ident("expired")
    )
}

/// `text` with every line indented by `by`.
fn indented(text: &str, by: &str) -> String {
    text.lines()
        .map(|line| format!("{by}{line}"))
        .collect::<Vec<_>>()
        .join("\n")
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

/// A reference a person's erasure follows (area 15, "Erasure"; chunk P3):
/// from the records of the entity numbered `from` among those it deletes
/// to the records of the entity numbered `to` that refer to one through
/// `table`'s `column`, which it deletes too.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Followed<'a> {
    pub from: usize,
    pub to: usize,
    pub table: &'a str,
    pub column: &'a str,
}

/// A table whose records a person's erasure keeps while they refer to
/// one it deletes, and what it clears on them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Keeps<'a> {
    pub table: &'a str,
    /// Its entity's number among those erasure deletes records of, when it
    /// is one: those records are deleted, not kept.
    pub deleted: Option<usize>,
    /// Each reference column it clears where it refers to a record of the
    /// entity numbered beside it, and whether that anonymises the record.
    pub references: Vec<(&'a str, usize, bool)>,
    /// The columns of the marked fields it clears on a record an
    /// anonymising reference keeps.
    pub clears: Vec<String>,
    /// Each column set on every record it changes, to the SQL beside it:
    /// a `Generated.on_change` field, to the operation's time.
    pub set: Vec<(String, String)>,
    /// When a record may belong to another person too, how to look for
    /// one the erasure keeps: its marked fields are cleared only on a
    /// record none of these lead from to such a person (chunk P4).
    pub others: Option<Others<'a>>,
}

/// How a person's erasure looks for another person a record it keeps
/// belongs to (area 15, "Records that belong to several people"; chunk
/// P4): from each of the record's references that leads to a person,
/// through the records they refer to, each numbered by its owner, an
/// entity looked through, until it reaches a person's record. A
/// reference to a record the erasure deletes leads nowhere, since it is
/// cleared.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Others<'a> {
    pub starts: Vec<Start<'a>>,
    pub climbs: Vec<Climbed<'a>>,
    /// The owners that are person entities, whose records end the search.
    pub people: Vec<usize>,
}

/// One of a kept record's references that leads to a person: its column,
/// the owner it refers to, and, when the erasure deletes records of that
/// owner's entity, the entity's number among them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Start<'a> {
    pub column: &'a str,
    pub owner: usize,
    pub erased: Option<usize>,
}

/// A reference climbed from a record of the owner `from`, in `table`, by
/// its `column`, to a record of the owner `to`, whose entity's number
/// among those the erasure deletes is `erased`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Climbed<'a> {
    pub from: usize,
    pub table: &'a str,
    pub column: &'a str,
    pub to: usize,
    pub erased: Option<usize>,
}

/// Whether `column`, which refers to a record numbered `erased` among
/// those a person's erasure deletes when it is one, leads on: it holds a
/// value, and not one of those.
fn leads(column: &str, erased_entity: Option<usize>) -> String {
    match erased_entity {
        Some(entity) => format!(
            "{column} IS NOT NULL AND {column} NOT IN ({})",
            erased(entity)
        ),
        None => format!("{column} IS NOT NULL"),
    }
}

/// The condition, on a record of `table`, that none of `others` leads to
/// a person the erasure keeps. With every start a person's, it asks each
/// reference directly; otherwise a recursive query climbs from the
/// record's references, each record once, so a loop in the data ends too.
fn no_other_person(table: &str, others: &Others<'_>) -> String {
    let column = |name: &str| format!("{}.{}", ident(table), ident(name));
    if others.climbs.is_empty() {
        return others
            .starts
            .iter()
            .map(|start| {
                let at = column(start.column);
                match start.erased {
                    Some(entity) => format!("({at} IS NULL OR {at} IN ({}))", erased(entity)),
                    None => format!("{at} IS NULL"),
                }
            })
            .collect::<Vec<_>>()
            .join("\n    AND ");
    }
    let starts: Vec<String> = others
        .starts
        .iter()
        .map(|start| {
            let at = column(start.column);
            format!(
                "SELECT {}, {at} WHERE {}",
                start.owner,
                leads(&at, start.erased)
            )
        })
        .collect();
    let climbs: Vec<String> = others
        .climbs
        .iter()
        .enumerate()
        .map(|(i, up)| {
            let alias = ident(&format!("up{}", i + 1));
            let at = format!("{alias}.{}", ident(up.column));
            format!(
                "SELECT {} AS \"from\", {alias}.\"id\" AS \"from_id\", {} AS \"entity\", {at} AS \"id\" FROM {} AS {alias} WHERE {}",
                up.from,
                up.to,
                ident(up.table),
                leads(&at, up.erased)
            )
        })
        .collect();
    let people: Vec<String> = others.people.iter().map(usize::to_string).collect();
    format!(
        "NOT EXISTS (\n      WITH RECURSIVE \"owner\" (\"entity\", \"id\") AS (\n        {}\n        UNION\n        SELECT \"up\".\"entity\", \"up\".\"id\"\n        FROM \"owner\"\n        JOIN (\n          {}\n        ) AS \"up\" ON \"up\".\"from\" = \"owner\".\"entity\" AND \"up\".\"from_id\" = \"owner\".\"id\"\n      )\n      SELECT 1 FROM \"owner\" WHERE \"owner\".\"entity\" IN ({})\n    )",
        starts.join("\n        UNION ALL\n        "),
        climbs.join("\n          UNION ALL\n          "),
        people.join(", ")
    )
}

/// The record an audited write starts from (area 16, "Audit trail"; chunk
/// B11): every column of `table`'s row whose `id` is `$1`, locked as the
/// write will lock it, `FOR UPDATE` before a delete and `FOR NO KEY
/// UPDATE` before an update, so no other change comes between.
pub fn audit_before(columns: &[String], table: &str, deleting: bool) -> String {
    format!(
        "SELECT {}\nFROM {}\nWHERE {} = $1\nFOR {}UPDATE",
        idents(columns),
        ident(table),
        ident("id"),
        if deleting { "" } else { "NO KEY " }
    )
}

/// Whether a caller may read an audited record's history: one row when
/// `table` holds the record its conditions find and the caller may read
/// it, with whether they may read each field `grants` gives a condition
/// for, in order.
pub fn history_access(table: &str, grants: &[String]) -> String {
    let mut columns = vec!["TRUE".to_string()];
    columns.extend(
        grants
            .iter()
            .map(|grant| format!("{} IS TRUE", parenthesised(grant))),
    );
    format!("SELECT {}\nFROM {}", columns.join(", "), ident(table))
}

/// One batch of an audit retention (area 16; chunk B11): the entries of
/// the entity whose born name is `entity`, a parameter, written before
/// `cutoff`, oldest first, at most `batch` of them, each returning its
/// `seq`.
pub fn audit_purge(entity: &str, cutoff: &str, batch: u32) -> String {
    let key = idents(&["entity", "record_id", "seq"]);
    format!(
        "DELETE FROM {table}\nWHERE ({key}) IN (\n  SELECT {key}\n  FROM {table}\n  WHERE {} = {entity} AND {} < {cutoff}\n  ORDER BY {}, {}, {}\n  LIMIT {batch}\n  FOR UPDATE SKIP LOCKED\n)\nRETURNING {}",
        ident("entity"),
        ident("at"),
        ident("at"),
        ident("record_id"),
        ident("seq"),
        ident("seq"),
        table = ident("jadpo_audit"),
    )
}

/// The `id`s of the records of the entity numbered `entity` that a
/// person's erasure deletes.
fn erased(entity: usize) -> String {
    format!(
        "SELECT {} FROM {} WHERE {} = {entity}",
        ident("id"),
        ident("erased"),
        ident("entity")
    )
}

/// One person's erasure (area 15, "Erasure"; chunk P3), as one statement,
/// so it is all or nothing. A recursive query finds the person's record
/// in `tables[0]` meeting `person` (the `id` and the policy that lets the
/// caller delete it, hidden or not) and every record each of `follows`
/// leads to from one already found, tagging each with its entity's
/// number in `tables`, each record once, so a loop in the data ends too.
/// Then each of `keeps` clears its references to them, and its marked
/// fields where an anonymising reference matched and, for a record that
/// may belong to someone else too, none of its other references leads to
/// a person the erasure keeps (`"last"`); every record found is
/// deleted, whether a lifecycle hides it or not; and each of `signed_in`,
/// a table and the column holding the `id` of a record of the entity
/// numbered beside it, loses its rows for the records deleted. Every
/// part sees the same snapshot, and the foreign keys are checked once at
/// its end, when nothing refers to a deleted record. It returns the
/// person's `id`, or no row when the record isn't there or policy
/// refuses the caller.
pub fn erase(
    tables: &[&str],
    person: &[String],
    follows: &[Followed<'_>],
    keeps: &[Keeps<'_>],
    signed_in: &[(usize, &str, &str)],
    audited: &[(usize, String)],
) -> String {
    let mut base = format!(
        "  SELECT 0, {}.{}\n  FROM {}",
        ident(tables[0]),
        ident("id"),
        ident(tables[0])
    );
    where_clause(&mut base, person, "  ");
    let found = if follows.is_empty() {
        base
    } else {
        let edges: Vec<String> = follows
            .iter()
            .enumerate()
            .map(|(i, follow)| {
                let alias = ident(&format!("edge{}", i + 1));
                format!(
                    "SELECT {} AS \"parent\", {alias}.{} AS \"parent_id\", {} AS \"entity\", {alias}.\"id\" AS \"id\" FROM {} AS {alias}",
                    follow.from,
                    ident(follow.column),
                    follow.to,
                    ident(follow.table)
                )
            })
            .collect();
        format!(
            "{base}\n  UNION\n  SELECT \"edge\".\"entity\", \"edge\".\"id\"\n  FROM \"erased\"\n  JOIN (\n    {}\n  ) AS \"edge\" ON \"edge\".\"parent\" = \"erased\".\"entity\" AND \"edge\".\"parent_id\" = \"erased\".\"id\"",
            edges.join("\n    UNION ALL\n    ")
        )
    };
    let mut parts = vec![format!(
        "{} ({}, {}) AS (\n{found}\n)",
        ident("erased"),
        ident("entity"),
        ident("id")
    )];
    for (i, keep) in keeps.iter().enumerate() {
        let matched =
            |column: &str, entity: usize| format!("{} IN ({})", ident(column), erased(entity));
        let one = keep.references.len() == 1;
        let mut set: Vec<String> = keep
            .references
            .iter()
            .map(|(column, entity, _)| {
                if one {
                    format!("{} = NULL", ident(column))
                } else {
                    set_when(column, &matched(column, *entity), "NULL")
                }
            })
            .collect();
        let anonymising: Vec<String> = keep
            .references
            .iter()
            .filter(|(_, _, anonymises)| *anonymises)
            .map(|(column, entity, _)| matched(column, *entity))
            .collect();
        let every = anonymising.len() == keep.references.len();
        // A record that may belong to someone else too: the records
        // whose marked fields go, those whose last person this is.
        let last = match &keep.others {
            Some(others) if !keep.clears.is_empty() => {
                let name = format!("last{}", i + 1);
                let mut found = format!(
                    "  SELECT {}.{}\n  FROM {}",
                    ident(keep.table),
                    ident("id"),
                    ident(keep.table)
                );
                let anonymised = if anonymising.len() == 1 {
                    anonymising.join("")
                } else {
                    format!("({})", anonymising.join(" OR "))
                };
                where_clause(
                    &mut found,
                    &[anonymised, no_other_person(keep.table, others)],
                    "  ",
                );
                parts.push(format!(
                    "{} ({}) AS (\n{found}\n)",
                    ident(&name),
                    ident("id")
                ));
                Some(format!(
                    "{} IN (SELECT {} FROM {})",
                    ident("id"),
                    ident("id"),
                    ident(&name)
                ))
            }
            _ => None,
        };
        for column in &keep.clears {
            match &last {
                Some(last) => set.push(set_when(column, last, "NULL")),
                None if every => set.push(format!("{} = NULL", ident(column))),
                None => set.push(set_when(column, &anonymising.join(" OR "), "NULL")),
            }
        }
        set.extend(
            keep.set
                .iter()
                .map(|(column, value)| format!("{} = {value}", ident(column))),
        );
        let any: Vec<String> = keep
            .references
            .iter()
            .map(|(column, entity, _)| matched(column, *entity))
            .collect();
        let mut condition = if one {
            any.join("")
        } else {
            format!("({})", any.join(" OR "))
        };
        if let Some(entity) = keep.deleted {
            condition.push_str(&format!(
                "\n    AND {} NOT IN ({})",
                ident("id"),
                erased(entity)
            ));
        }
        parts.push(format!(
            "{} AS (\n  UPDATE {}\n  SET {}\n  WHERE {condition}\n)",
            ident(&format!("kept{}", i + 1)),
            ident(keep.table),
            set.join(",\n    ")
        ));
    }
    for (i, (entity, table, column)) in signed_in.iter().enumerate() {
        parts.push(format!(
            "{} AS (\n  DELETE FROM {}\n  WHERE {} IN ({})\n)",
            ident(&format!("signed_out{}", i + 1)),
            ident(table),
            ident(column),
            erased(*entity)
        ));
    }
    // An audited record's history goes with it (area 16, "Audit trail";
    // chunk B11): each of `audited`, the number of an entity and its born
    // name as a parameter.
    for (i, (entity, born)) in audited.iter().enumerate() {
        parts.push(format!(
            "{} AS (\n  DELETE FROM {}\n  WHERE {} = {born} AND {} IN ({})\n)",
            ident(&format!("history{}", i + 1)),
            ident("jadpo_audit"),
            ident("entity"),
            ident("record_id"),
            erased(*entity)
        ));
    }
    for (entity, table) in tables.iter().enumerate().skip(1) {
        parts.push(format!(
            "{} AS (\n  DELETE FROM {}\n  WHERE {} IN ({})\n)",
            ident(&format!("deleted{entity}")),
            ident(table),
            ident("id"),
            erased(entity)
        ));
    }
    parts.push(format!(
        "{} AS (\n  DELETE FROM {}\n  WHERE {} IN ({})\n  RETURNING {}\n)",
        ident("deleted0"),
        ident(tables[0]),
        ident("id"),
        erased(0),
        ident("id")
    ));
    format!(
        "WITH{} {}\nSELECT {} FROM {}",
        if follows.is_empty() { "" } else { " RECURSIVE" },
        parts.join(", "),
        ident("id"),
        ident("deleted0")
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
