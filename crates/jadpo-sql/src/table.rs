//! What a migration does to a store's tables (area 5, "Migrations"): the
//! statements `jadpo migrate new` writes into a migration's `postgres.sql`,
//! rendered from the changes the graph's migration diff decided.

use crate::{ident, idents, text_literal};

/// A column as a table definition lists it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ColumnDef<'a> {
    pub name: &'a str,
    /// Its type as PostgreSQL writes it: `uuid`, `numeric(10, 2)`.
    pub sql_type: &'a str,
    pub nullable: bool,
}

/// `CREATE TABLE` of `columns`, its primary key the constraint `key` on
/// `key_columns`.
pub fn create_table(
    name: &str,
    columns: &[ColumnDef<'_>],
    key: &str,
    key_columns: &[String],
) -> String {
    let mut lines: Vec<String> = columns
        .iter()
        .map(|column| {
            let null = if column.nullable { "" } else { " NOT NULL" };
            format!("    {} {}{null}", ident(column.name), column.sql_type)
        })
        .collect();
    lines.push(format!(
        "    CONSTRAINT {} PRIMARY KEY ({})",
        ident(key),
        idents(key_columns)
    ));
    format!("CREATE TABLE {} (\n{}\n)", ident(name), lines.join(",\n"))
}

/// Adds `column` of `sql_type` to `table`, allowing `NULL` until a
/// backfill fills it.
pub fn add_column(table: &str, column: &str, sql_type: &str) -> String {
    format!(
        "ALTER TABLE {} ADD COLUMN {} {sql_type}",
        ident(table),
        ident(column)
    )
}

/// Sets the comment on `column`, or removes it for `None`: what a
/// quantity's column holds, its kind and SI unit (area 19, chunk 5.5w).
pub fn comment_on_column(table: &str, column: &str, comment: Option<&str>) -> String {
    format!(
        "COMMENT ON COLUMN {}.{} IS {}",
        ident(table),
        ident(column),
        comment.map_or_else(|| "NULL".to_string(), text_literal)
    )
}

/// Changes `column`'s type to `sql_type`.
pub fn change_type(table: &str, column: &str, sql_type: &str) -> String {
    format!(
        "ALTER TABLE {} ALTER COLUMN {} TYPE {sql_type}",
        ident(table),
        ident(column)
    )
}

/// Lets `column` hold `NULL`.
pub fn drop_not_null(table: &str, column: &str) -> String {
    format!(
        "ALTER TABLE {} ALTER COLUMN {} DROP NOT NULL",
        ident(table),
        ident(column)
    )
}

/// Requires a value in `column`.
pub fn set_not_null(table: &str, column: &str) -> String {
    format!(
        "ALTER TABLE {} ALTER COLUMN {} SET NOT NULL",
        ident(table),
        ident(column)
    )
}

/// Sets `column` to `value` (SQL) in the rows meeting any of `conditions`.
pub fn fill(table: &str, column: &str, value: &str, conditions: &[String]) -> String {
    format!(
        "UPDATE {} SET {} = {value} WHERE {}",
        ident(table),
        ident(column),
        conditions.join(" OR ")
    )
}

/// How many rows a computed backfill fills in one batch, each batch in a
/// transaction of its own (area 5, "Computed backfills"; chosen in chunk
/// 5.6b): small enough that a batch holds its rows' locks for a moment,
/// large enough that a million rows take a thousand batches.
pub const BATCH_ROWS: u32 = 1000;

/// A computed backfill of `column` with `value` (SQL reading the row's
/// columns as `"table"."column"`), filled in batches by `UPDATE`: the line
/// that names `field` and says how, the statement counting the rows left,
/// and one batch, which takes the key the last batch ended at as `$1` and
/// gives the key it ended at and how many rows it filled. Batches go in
/// key order, so each starts where the last ended, through the key's
/// index; the rows each fills are those still without a value, so a
/// batch run twice fills nothing twice.
pub fn backfill_in_batches(
    field: &str,
    table: &str,
    key: &str,
    column: &str,
    value: &str,
) -> String {
    let (t, k, c) = (ident(table), ident(key), ident(column));
    format!(
        "{marker}{field}: by UPDATE, {BATCH_ROWS} rows a batch, each batch its own transaction\n\
         SELECT count(*) FROM {t} WHERE {c} IS NULL;\n\
         WITH \"batch\" AS (SELECT {k} AS \"jadpo_key\" FROM {t} WHERE {k} > $1 ORDER BY {k} LIMIT {BATCH_ROWS}), \
         \"filled\" AS (UPDATE {t} SET {c} = {value} FROM \"batch\" WHERE {t}.{k} = \"batch\".\"jadpo_key\" AND {t}.{c} IS NULL RETURNING 1) \
         SELECT (SELECT \"jadpo_key\" FROM \"batch\" ORDER BY \"jadpo_key\" DESC LIMIT 1), (SELECT count(*) FROM \"filled\")",
        marker = jadpo_std::BACKFILL_MARKER,
    )
}

/// Keeps every other transaction from writing `table` until this one
/// ends, while reads go on: what a computed backfill's last step takes
/// before it fills the rows written meanwhile and makes the column
/// required, so no row without a value can arrive in between.
pub fn lock_against_writes(table: &str) -> String {
    format!("LOCK TABLE {} IN SHARE ROW EXCLUSIVE MODE", ident(table))
}

/// `column IS NULL`: a row a new required column has no value in.
pub fn is_null(column: &str) -> String {
    format!("{} IS NULL", ident(column))
}

/// What a column's type allows beyond the column type: an enum's variants,
/// a number's bounds and a text's lengths, as the snapshot records them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Allowed<'a> {
    pub values: &'a [String],
    pub min: Option<&'a str>,
    pub max: Option<&'a str>,
    pub min_length: Option<u64>,
    pub max_length: Option<u64>,
}

/// The conditions under which a stored value of `column` breaks what
/// `allowed` allows. A pattern is left out, since PostgreSQL's regular
/// expressions are not Jadpo's.
pub fn breaks(column: &str, allowed: &Allowed<'_>) -> Vec<String> {
    let column = ident(column);
    let mut out = Vec::new();
    if !allowed.values.is_empty() {
        let values: Vec<String> = allowed.values.iter().map(|v| text_literal(v)).collect();
        out.push(format!("{column} NOT IN ({})", values.join(", ")));
    }
    if let Some(min) = allowed.min {
        out.push(format!("{column} < {min}"));
    }
    if let Some(max) = allowed.max {
        out.push(format!("{column} > {max}"));
    }
    if let Some(n) = allowed.min_length {
        out.push(format!("char_length({column}) < {n}"));
    }
    if let Some(n) = allowed.max_length {
        out.push(format!("char_length({column}) > {n}"));
    }
    out
}

/// Drops the constraint `name` of `table`.
pub fn drop_constraint(table: &str, name: &str) -> String {
    format!(
        "ALTER TABLE {} DROP CONSTRAINT {}",
        ident(table),
        ident(name)
    )
}

/// Drops the index `name`.
pub fn drop_index(name: &str) -> String {
    format!("DROP INDEX {}", ident(name))
}

/// Adds the unique constraint `name` on `columns` of `table`.
pub fn add_unique(table: &str, name: &str, columns: &[String]) -> String {
    format!(
        "ALTER TABLE {} ADD CONSTRAINT {} UNIQUE ({})",
        ident(table),
        ident(name),
        idents(columns)
    )
}

/// Adds the check constraint `name` that `column` of `table` holds one of
/// `values`, each text.
pub fn add_check(table: &str, name: &str, column: &str, values: &[String]) -> String {
    let condition = match values {
        [one] => format!("{} = {}", ident(column), text_literal(one)),
        many => format!(
            "{} IN ({})",
            ident(column),
            many.iter()
                .map(|v| text_literal(v))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    };
    format!(
        "ALTER TABLE {} ADD CONSTRAINT {} CHECK ({condition})",
        ident(table),
        ident(name)
    )
}

/// Adds the check constraint `name` that the `geography` column `column`
/// of `table` holds a geometry in WGS84 whose `GeometryType` is one of
/// `types` (`POINT`): an untyped column, so a point may have an altitude
/// or not, value by value, held to its type and system here (area 20,
/// "Storage"; chunk 5.5u4).
pub fn add_geography_check(table: &str, name: &str, column: &str, types: &[String]) -> String {
    let column = ident(column);
    let kind = match types {
        [one] => format!("GeometryType({column}) = {}", text_literal(one)),
        many => format!(
            "GeometryType({column}) IN ({})",
            many.iter()
                .map(|v| text_literal(v))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    };
    format!(
        "ALTER TABLE {} ADD CONSTRAINT {} CHECK (ST_SRID({column}) = {} AND {kind})",
        ident(table),
        ident(name),
        jadpo_std::geography::SRID
    )
}

/// Adds the foreign key `name` from `columns` of `table` to `target`'s
/// `id`, doing `on_delete` (`RESTRICT`, `CASCADE`, `SET NULL`) when the
/// record it names is deleted.
pub fn add_foreign_key(
    table: &str,
    name: &str,
    columns: &[String],
    target: &str,
    on_delete: &str,
) -> String {
    format!(
        "ALTER TABLE {} ADD CONSTRAINT {} FOREIGN KEY ({}) REFERENCES {} (\"id\") ON DELETE {on_delete}",
        ident(table),
        ident(name),
        idents(columns),
        ident(target)
    )
}

/// One column of an index: its name, and whether it keeps text in code
/// point order (`COLLATE "C"`), as a query sorts it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IndexColumn<'a> {
    pub name: &'a str,
    pub collate: bool,
}

/// Creates the index `name` on `columns` of `table`, holding only the rows
/// `predicate` (a condition, without parameters) lets through when given,
/// by `method` (an access method and its columns' operator class, `hnsw`
/// with `vector_cosine_ops`, area 21; `gist` with none, its type's own,
/// for a geometry, area 20) when given, else a B-tree.
pub fn create_index(
    table: &str,
    name: &str,
    columns: &[IndexColumn<'_>],
    predicate: Option<&str>,
    method: Option<(&str, &str)>,
) -> String {
    let columns = columns
        .iter()
        .map(|column| match method {
            Some((_, "")) => ident(column.name),
            Some((_, class)) => format!("{} {class}", ident(column.name)),
            None if column.collate => format!("{} COLLATE \"C\"", ident(column.name)),
            None => ident(column.name),
        })
        .collect::<Vec<_>>()
        .join(", ");
    let using = method
        .map(|(method, _)| format!(" USING {method}"))
        .unwrap_or_default();
    let partial = predicate
        .map(|predicate| format!(" WHERE {predicate}"))
        .unwrap_or_default();
    format!(
        "CREATE INDEX {} ON {}{using} ({columns}){partial}",
        ident(name),
        ident(table)
    )
}

/// Creates the extension `name` unless the database has it: pgvector's
/// `vector` for a store's first vector column (area 21, chunk 5.5v1).
/// `jadpo migrate apply` checks it is available before running anything.
pub fn create_extension(name: &str) -> String {
    format!("CREATE EXTENSION IF NOT EXISTS {}", ident(name))
}

/// A line for a person, not the database: `-- text`.
pub fn comment(text: &str) -> String {
    format!("-- {text}")
}
