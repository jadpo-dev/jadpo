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
/// `predicate` (a condition, without parameters) lets through when given.
pub fn create_index(
    table: &str,
    name: &str,
    columns: &[IndexColumn<'_>],
    predicate: Option<&str>,
) -> String {
    let columns = columns
        .iter()
        .map(|column| {
            if column.collate {
                format!("{} COLLATE \"C\"", ident(column.name))
            } else {
                ident(column.name)
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    let partial = predicate
        .map(|predicate| format!(" WHERE {predicate}"))
        .unwrap_or_default();
    format!(
        "CREATE INDEX {} ON {} ({columns}){partial}",
        ident(name),
        ident(table)
    )
}

/// A line for a person, not the database: `-- text`.
pub fn comment(text: &str) -> String {
    format!("-- {text}")
}
