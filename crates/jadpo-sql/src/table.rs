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

/// Drops `column` from `table`: a generated column, which holds nothing
/// of its own (area 16, "Search"; chunk B12), or a retired one a person
/// accepted losing (area 5, "Migrations"; chunk V3).
pub fn drop_column(table: &str, column: &str) -> String {
    format!("ALTER TABLE {} DROP COLUMN {}", ident(table), ident(column))
}

/// Drops `table`, retired, which a person accepted losing (area 5,
/// "Migrations"; chunk V3). No `CASCADE`: a reference nothing expected
/// stops the migration rather than going with it.
pub fn drop_table(table: &str) -> String {
    format!("DROP TABLE {}", ident(table))
}

/// `sql_type`, generated from `expression` when it is given: the type a
/// column definition writes after the column's name.
pub fn generated_type(sql_type: &str, expression: Option<&str>) -> String {
    match expression {
        Some(expression) => format!("{sql_type} GENERATED ALWAYS AS ({expression}) STORED"),
        None => sql_type.to_string(),
    }
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

/// A computed backfill of `column` worked out in Jadpo (area 5,
/// "Computed backfills"; chunk 5.6b2), in the batches of
/// [`backfill_in_batches`]: the line naming `field` and saying so, with
/// the digest of the code it runs (`code`), the statement counting the
/// rows left, one batch's read, and its write. The read takes the key the
/// last batch ended at as `$1` and gives the next rows still without a
/// value, in key order, each its key and then the `reads` columns, locked
/// as an `UPDATE` would lock them; the write takes their keys and values
/// as two arrays (`$1`, `$2`) and fills those still without one, so a
/// batch run twice fills nothing twice.
///
/// A value held in several columns (an amount of money's two, chunk Q3b)
/// fills each of `columns` from an array of its own, the first deciding
/// which rows are still without a value.
pub fn backfill_in_jadpo(
    field: &str,
    table: &str,
    key: &str,
    columns: &[(&str, &str)],
    reads: &[String],
    code: &str,
) -> String {
    let column = columns.first().map_or("", |(column, _)| *column);
    let (t, c) = (ident(table), ident(column));
    format!(
        "{marker}{field}: {how}, {BATCH_ROWS} rows a batch, each batch its own transaction, slower than by UPDATE since something in it has no SQL form; its code's digest {code}\n\
         SELECT count(*) FROM {t} WHERE {c} IS NULL;\n\
         {read} WHERE {k} > $1 AND {c} IS NULL ORDER BY {k} LIMIT {BATCH_ROWS} FOR NO KEY UPDATE;\n\
         {write}",
        marker = jadpo_std::BACKFILL_MARKER,
        how = jadpo_std::IN_JADPO,
        k = ident(key),
        read = jadpo_read(table, key, reads),
        write = jadpo_write(table, key, columns),
    )
}

/// The last step's fill of a backfill worked out in Jadpo (chunk 5.6b2),
/// run once the table is locked against writes: the line naming `field`,
/// the read of every row still without a value, and the write, as in
/// [`backfill_in_jadpo`].
pub fn fill_in_jadpo(
    field: &str,
    table: &str,
    key: &str,
    columns: &[(&str, &str)],
    reads: &[String],
) -> String {
    let column = columns.first().map_or("", |(column, _)| *column);
    format!(
        "{marker}{field} {how}: the rows written meanwhile\n\
         {read} WHERE {c} IS NULL ORDER BY {k};\n\
         {write}",
        marker = jadpo_std::FILL_MARKER,
        how = jadpo_std::IN_JADPO,
        c = ident(column),
        k = ident(key),
        read = jadpo_read(table, key, reads),
        write = jadpo_write(table, key, columns),
    )
}

/// The check of every value stored in `column`, a structured field's JSON
/// whose type may no longer read them (area 5, "Storage types"; chunk
/// 5.9c), in batches of [`BATCH_ROWS`], each in a transaction of its own:
/// the line naming `field` and saying so, the statement counting the
/// values to check, and one batch's read, which takes the key the last
/// batch ended at as `$1` and gives the next rows with a value, each its
/// key and the value. `jadpo migrate apply` reads each with the field's
/// type now, and refuses the migration while any doesn't read.
pub fn check_in_jadpo(field: &str, table: &str, key: &str, column: &str) -> String {
    let (t, k, c) = (ident(table), ident(key), ident(column));
    format!(
        "{marker}{field}: {how}, {BATCH_ROWS} rows a batch, each batch its own transaction, reading every stored value with the field's type now and refusing the migration while one doesn't read\n\
         SELECT count(*) FROM {t} WHERE {c} IS NOT NULL;\n\
         SELECT {k}, {c} FROM {t} WHERE {k} > $1 AND {c} IS NOT NULL ORDER BY {k} LIMIT {BATCH_ROWS}",
        marker = jadpo_std::CHECK_MARKER,
        how = jadpo_std::IN_JADPO,
    )
}

/// The rewrite of the values stored in `column` that a structured field's
/// type no longer reads (chunk 5.9c), with a backfill worked out in Jadpo
/// as [`backfill_in_jadpo`] works one out: the line naming `field`, with
/// the digest of the code it runs (`code`), the statement counting the
/// values to read, one batch's read, which gives the next rows with a
/// value past `$1`, each its key, the value and then the `reads` columns,
/// locked as an `UPDATE` would lock them, and the write, which takes the
/// keys of those that don't read and their new values as two arrays.
pub fn rewrite_in_jadpo(
    field: &str,
    table: &str,
    key: &str,
    column: &str,
    sql_type: &str,
    reads: &[String],
    code: &str,
) -> String {
    rewritten_in_jadpo(
        field,
        "rewriting the stored values the field's type no longer reads",
        (table, key, column, sql_type),
        reads,
        code,
    )
}

/// The refit of the values stored in `column` that its field's rules no
/// longer allow, a new `pattern:`'s among them, with a backfill worked out
/// in Jadpo (chunk Q3): read in batches as [`rewrite_in_jadpo`] reads
/// them, `jadpo migrate apply` checking each value with the field's rules
/// now and writing the backfill's value back for those they refuse.
pub fn refit_in_jadpo(
    field: &str,
    table: &str,
    key: &str,
    column: &str,
    sql_type: &str,
    reads: &[String],
    code: &str,
) -> String {
    rewritten_in_jadpo(
        field,
        "replacing the stored values the field's rules no longer allow",
        (table, key, column, sql_type),
        reads,
        code,
    )
}

fn rewritten_in_jadpo(
    field: &str,
    doing: &str,
    (table, key, column, sql_type): (&str, &str, &str, &str),
    reads: &[String],
    code: &str,
) -> String {
    let (t, k, c) = (ident(table), ident(key), ident(column));
    let mut read = vec![column.to_string()];
    read.extend(reads.iter().cloned());
    format!(
        "{marker}{field}: {how}, {BATCH_ROWS} rows a batch, each batch its own transaction, {doing}; its code's digest {code}\n\
         SELECT count(*) FROM {t} WHERE {c} IS NOT NULL;\n\
         {select} WHERE {k} > $1 AND {c} IS NOT NULL ORDER BY {k} LIMIT {BATCH_ROWS} FOR NO KEY UPDATE;\n\
         UPDATE {t} SET {c} = \"jadpo_value\".\"value\" FROM unnest($1::uuid[], $2::{sql_type}[]) AS \"jadpo_value\"(\"key\", \"value\") WHERE {t}.{k} = \"jadpo_value\".\"key\"",
        marker = jadpo_std::REWRITE_MARKER,
        how = jadpo_std::IN_JADPO,
        select = jadpo_read(table, key, &read),
    )
}

/// The refit of the values stored in `column` that its field's rules no
/// longer allow, by a computed backfill with a SQL form (chunk Q3): in
/// the batches of [`backfill_in_batches`], each giving `value` to the rows
/// of the batch meeting any of `broken`, the conditions under which a
/// stored value breaks them. Its count is of the rows to replace.
pub fn refit_in_batches(
    field: &str,
    table: &str,
    key: &str,
    column: &str,
    value: &str,
    broken: &[String],
) -> String {
    let (t, k, c) = (ident(table), ident(key), ident(column));
    let broken = broken.join(" OR ");
    format!(
        "{marker}{field}: by UPDATE, {BATCH_ROWS} rows a batch, each batch its own transaction, replacing the stored values the field's rules no longer allow\n\
         SELECT count(*) FROM {t} WHERE {broken};\n\
         WITH \"batch\" AS (SELECT {k} AS \"jadpo_key\" FROM {t} WHERE {k} > $1 ORDER BY {k} LIMIT {BATCH_ROWS}), \
         \"filled\" AS (UPDATE {t} SET {c} = {value} FROM \"batch\" WHERE {t}.{k} = \"batch\".\"jadpo_key\" AND ({broken}) RETURNING 1) \
         SELECT (SELECT \"jadpo_key\" FROM \"batch\" ORDER BY \"jadpo_key\" DESC LIMIT 1), (SELECT count(*) FROM \"filled\")",
        marker = jadpo_std::REWRITE_MARKER,
    )
}

/// `SELECT key, reads… FROM table`.
fn jadpo_read(table: &str, key: &str, reads: &[String]) -> String {
    let mut columns = vec![ident(key)];
    columns.extend(reads.iter().map(|read| ident(read)));
    format!("SELECT {} FROM {}", columns.join(", "), ident(table))
}

/// Fills `columns` from arrays, the keys and then each column's values,
/// the rows whose first column is still without one: one column's values
/// are `"value"`, several columns' `"value_1"` on (chunk Q3b).
fn jadpo_write(table: &str, key: &str, columns: &[(&str, &str)]) -> String {
    let (t, k) = (ident(table), ident(key));
    let first = ident(columns.first().map_or("", |(column, _)| *column));
    let named = |at: usize| match columns.len() {
        1 => "\"value\"".to_string(),
        _ => format!("\"value_{}\"", at + 1),
    };
    let set: Vec<String> = columns
        .iter()
        .enumerate()
        .map(|(at, (column, _))| format!("{} = \"jadpo_value\".{}", ident(column), named(at)))
        .collect();
    let mut arrays = vec!["$1::uuid[]".to_string()];
    let mut names = vec!["\"key\"".to_string()];
    for (at, (_, sql_type)) in columns.iter().enumerate() {
        arrays.push(format!("${}::{sql_type}[]", at + 2));
        names.push(named(at));
    }
    format!(
        "UPDATE {t} SET {} FROM unnest({}) AS \"jadpo_value\"({}) WHERE {t}.{k} = \"jadpo_value\".\"key\" AND {t}.{first} IS NULL",
        set.join(", "),
        arrays.join(", "),
        names.join(", ")
    )
}

/// Keeps every other transaction from writing `table` until this one
/// ends, while reads go on: what a computed backfill's last step takes
/// before it fills the rows written meanwhile and makes the column
/// required, so no row without a value can arrive in between.
pub fn lock_against_writes(table: &str) -> String {
    format!("LOCK TABLE {} IN SHARE ROW EXCLUSIVE MODE", ident(table))
}

/// Adds the check `name` that every row of `table` has a value in
/// `column`, `NOT VALID`, so adding it reads no row: what a backfill's
/// last step adds once it has filled the rows written meanwhile, holding
/// every row written from then on to it (chunk 5.8b).
pub fn add_filled_check(table: &str, name: &str, column: &str) -> String {
    format!(
        "ALTER TABLE {} ADD CONSTRAINT {} CHECK ({} IS NOT NULL) NOT VALID",
        ident(table),
        ident(name),
        ident(column)
    )
}

/// Proves the check `name`, added `NOT VALID`, against every row: it
/// takes `SHARE UPDATE EXCLUSIVE`, so reads and writes go on while it
/// reads the table (chunk 5.8b).
pub fn validate_constraint(table: &str, name: &str) -> String {
    format!(
        "ALTER TABLE {} VALIDATE CONSTRAINT {}",
        ident(table),
        ident(name)
    )
}

/// `sql` opening a step of a migration of its own, run in a transaction of
/// its own after the step before commits: the line saying what it does,
/// then the statement (chunk 5.8b).
pub fn in_a_step(what: &str, sql: &str) -> String {
    format!("{}{what}\n{sql}", jadpo_std::STEP_MARKER)
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

/// Adds the constraint `name` holding each row of `table` to `condition`,
/// which fails if a row doesn't meet it.
pub fn add_condition(table: &str, name: &str, condition: &str) -> String {
    format!(
        "ALTER TABLE {} ADD CONSTRAINT {} CHECK ({condition})",
        ident(table),
        ident(name)
    )
}

/// Drops the index `name`.
pub fn drop_index(name: &str) -> String {
    format!("DROP INDEX {}", ident(name))
}

/// Adds the unique constraint `name` on `columns` of `table`, counting
/// their `NULL`s as equal when `nulls_equal` (`NULLS NOT DISTINCT`,
/// PostgreSQL 15 and later): a key over a value kept in several columns,
/// one of which may be `NULL` while the value is there, so two values
/// equal in every part, an empty one included, are one value (area 5,
/// "Unique rules").
pub fn add_unique(table: &str, name: &str, columns: &[String], nulls_equal: bool) -> String {
    format!(
        "ALTER TABLE {} ADD CONSTRAINT {} UNIQUE{} ({})",
        ident(table),
        ident(name),
        if nulls_equal {
            " NULLS NOT DISTINCT"
        } else {
            ""
        },
        idents(columns)
    )
}

/// A value kept in `parts`, each a column and whether it may be `NULL`
/// while the value is there, being there: its first column that never
/// is `NULL` while it is, or any column not `NULL`.
pub fn holds_value(parts: &[(&str, bool)]) -> String {
    match parts.iter().find(|(_, may_be_null)| !may_be_null) {
        Some((column, _)) => format!("{} IS NOT NULL", ident(column)),
        None => format!(
            "({})",
            parts
                .iter()
                .map(|(column, _)| format!("{} IS NOT NULL", ident(column)))
                .collect::<Vec<_>>()
                .join(" OR ")
        ),
    }
}

/// The rows of `table` whose `columns` hold the same values as another
/// row's, `NULL`s counted as equal, among those `predicate` lets through:
/// what keeps a unique key over them from being made. Each row is one
/// set of such rows: the `id`s of its first 5, in order and between
/// spaces, and how many there are; the first 10 sets (area 5, "Unique
/// rules").
pub fn duplicates(table: &str, columns: &[IndexColumn<'_>], predicate: Option<&str>) -> String {
    let partial = predicate
        .map(|predicate| format!(" WHERE {predicate}"))
        .unwrap_or_default();
    format!(
        "SELECT array_to_string((array_agg({id}::text ORDER BY {id}))[1:5], ' '), count(*) FROM {}{partial} GROUP BY {} HAVING count(*) > 1 ORDER BY min({id}::text) LIMIT 10",
        ident(table),
        index_columns(columns, None),
        id = ident("id"),
    )
}

/// The digest of `column`'s JSON, `md5("tags"::text)`, which a key or a
/// B-tree holds in place of a value kept whole as its JSON (area 5,
/// "Unique rules"; chunk Q2): a B-tree's entry holds about 2.7 kB, and a
/// stored value up to 1 MiB. `jsonb` writes one text for one value, its
/// keys in one order, and both functions are fixed (`IMMUTABLE`), as an
/// index's expression must be.
pub fn json_digest(column: &str) -> String {
    format!("md5({}::text)", ident(column))
}

/// Creates the unique index `name` on `columns` of `table`: a unique rule
/// one of whose columns holds a value kept whole as its JSON, which it
/// holds as that JSON's digest (chunk Q2). PostgreSQL names the index in
/// a duplicate's error as it names a constraint, so a write maps it to
/// its rule the same way.
///
/// It counts `NULL`s as equal when `nulls_equal`, and holds only the rows
/// `predicate` lets through when given: a key over a field that may be
/// `none`, holding only the records that hold a value, so any number may
/// hold none.
pub fn create_unique_index(
    table: &str,
    name: &str,
    columns: &[IndexColumn<'_>],
    nulls_equal: bool,
    predicate: Option<&str>,
) -> String {
    let partial = predicate
        .map(|predicate| format!(" WHERE {predicate}"))
        .unwrap_or_default();
    format!(
        "CREATE UNIQUE INDEX {} ON {} ({}){}{partial}",
        ident(name),
        ident(table),
        index_columns(columns, None),
        if nulls_equal {
            " NULLS NOT DISTINCT"
        } else {
            ""
        },
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

/// One column of an index: its name, whether it keeps text in code
/// point order (`COLLATE "C"`), as a query sorts it, and whether it holds
/// a value kept whole as its JSON, which a B-tree holds as its digest
/// ([`json_digest`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IndexColumn<'a> {
    pub name: &'a str,
    pub collate: bool,
    pub digest: bool,
}

/// An index's access method other than a B-tree: its name, its column's
/// operator class (empty for its type's own), the type its column is cast
/// to when it indexes that expression, and its storage parameters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IndexMethod<'a> {
    pub name: &'a str,
    pub operator_class: &'a str,
    pub cast: Option<&'a str>,
    pub with: &'a [(String, u32)],
}

/// Creates the index `name` on `columns` of `table`, holding only the rows
/// `predicate` (a condition, without parameters) lets through when given,
/// by `method` when given, else a B-tree: `hnsw` with `vector_cosine_ops`,
/// or on `("column"::halfvec(3072))` with `halfvec_cosine_ops`, tuned
/// `WITH (m = 24, ef_construction = 128)` (area 21); `gist` with none, its
/// type's own, for a geometry, or on `("column"::geometry)` for a box
/// filter (area 20).
pub fn create_index(
    table: &str,
    name: &str,
    columns: &[IndexColumn<'_>],
    predicate: Option<&str>,
    method: Option<IndexMethod<'_>>,
) -> String {
    let columns = index_columns(columns, method);
    let using = method
        .map(|method| format!(" USING {}", method.name))
        .unwrap_or_default();
    let with = match method {
        Some(method) if !method.with.is_empty() => format!(
            " WITH ({})",
            method
                .with
                .iter()
                .map(|(setting, value)| format!("{setting} = {value}"))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        _ => String::new(),
    };
    let partial = predicate
        .map(|predicate| format!(" WHERE {predicate}"))
        .unwrap_or_default();
    format!(
        "CREATE INDEX {} ON {}{using} ({columns}){with}{partial}",
        ident(name),
        ident(table)
    )
}

/// An index's columns as `CREATE INDEX` lists them.
fn index_columns(columns: &[IndexColumn<'_>], method: Option<IndexMethod<'_>>) -> String {
    columns
        .iter()
        .map(|column| {
            let indexed = match method.and_then(|method| method.cast) {
                Some(cast) => format!("({}::{cast})", ident(column.name)),
                None if column.digest => format!("({})", json_digest(column.name)),
                None => ident(column.name),
            };
            match method {
                Some(IndexMethod {
                    operator_class: "", ..
                }) => indexed,
                Some(method) => format!("{indexed} {}", method.operator_class),
                None if column.collate => format!("{indexed} COLLATE \"C\""),
                None => indexed,
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// `CREATE INDEX` of `expression` on `table` (area 6, "Operations in
/// conditions", chunk 6.3b): a B-tree of an operation's SQL form, the
/// expression a query's condition compares, partial with `predicate`.
pub fn create_expression_index(
    table: &str,
    name: &str,
    expression: &str,
    predicate: Option<&str>,
) -> String {
    let partial = predicate
        .map(|predicate| format!(" WHERE {predicate}"))
        .unwrap_or_default();
    format!(
        "CREATE INDEX {} ON {} (({expression})){partial}",
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

/// A column whose rows hold stored files (area 16, "Deleting and
/// erasing"): a `File` column, or one keeping a structured value as its
/// JSON with files inside, however deep, with where they are in it.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Held {
    pub column: String,
    /// For a structured value, where its files are, as JSON text
    /// (`jadpo_graph::stored_json::files_within`): `None` for a `File`.
    pub within: Option<String>,
}

impl Held {
    /// A `File` column.
    pub fn file(column: impl Into<String>) -> Held {
        Held {
            column: column.into(),
            within: None,
        }
    }

    /// The trigger's argument for it: a `File` column's name, or the
    /// structured value's `{"column", "at", "types"}`.
    fn argument(&self) -> String {
        match &self.within {
            None => self.column.clone(),
            Some(within) => {
                // `within` is an object's JSON, `{...}`, which the column's
                // name, an identifier, joins as its first member.
                let rest = within.strip_prefix('{').unwrap_or(within);
                let comma = if rest.starts_with('}') { "" } else { "," };
                format!("{{\"column\":\"{}\"{comma}{rest}", self.column)
            }
        }
    }

    /// Each file a row of `table` holds in it, as a `"held"` column.
    fn files_of(&self, table: &str) -> String {
        match &self.within {
            None => format!(
                "SELECT {} AS \"held\" FROM {}",
                ident(&self.column),
                ident(table)
            ),
            Some(within) => {
                let within = text_literal(within);
                format!(
                    "SELECT \"held\" FROM {table}, LATERAL {walk}({table}.{column}, {within}::jsonb -> 'at', {within}::jsonb -> 'types') AS \"held\"",
                    table = ident(table),
                    walk = ident(FILES_WITHIN),
                    column = ident(&self.column),
                )
            }
        }
    }
}

/// The function that walks a structured value for the files it holds
/// (area 16, "Deleting and erasing"): given the value, where its files
/// are and the named types that hold them
/// (`jadpo_graph::stored_json::files_within`), each `File` inside it, a
/// `null` or a part of another shape giving none.
pub const FILES_WITHIN: &str = "jadpo_files_within";

/// Creates [`FILES_WITHIN`].
pub fn create_files_within() -> String {
    format!(
        "CREATE OR REPLACE FUNCTION {name}(held jsonb, at jsonb, types jsonb) RETURNS SETOF jsonb LANGUAGE plpgsql IMMUTABLE AS $jadpo$
DECLARE
    part record;
BEGIN
    IF held IS NULL OR at IS NULL OR jsonb_typeof(held) = 'null' THEN
        RETURN;
    END IF;
    IF jsonb_typeof(at) = 'string' THEN
        IF at = '\"file\"'::jsonb AND jsonb_typeof(held) = 'object' THEN
            RETURN NEXT held;
        END IF;
        RETURN;
    END IF;
    IF at ? 'named' THEN
        RETURN QUERY SELECT * FROM {name}(held, types -> (at ->> 'named'), types);
    ELSIF at ? 'items' THEN
        IF jsonb_typeof(held) = 'array' THEN
            FOR part IN SELECT value FROM jsonb_array_elements(held) LOOP
                RETURN QUERY SELECT * FROM {name}(part.value, at -> 'items', types);
            END LOOP;
        END IF;
    ELSIF at ? 'values' THEN
        IF jsonb_typeof(held) = 'object' THEN
            FOR part IN SELECT value FROM jsonb_each(held) LOOP
                RETURN QUERY SELECT * FROM {name}(part.value, at -> 'values', types);
            END LOOP;
        END IF;
    ELSIF at ? 'fields' THEN
        IF jsonb_typeof(held) = 'object' THEN
            FOR part IN SELECT key, value FROM jsonb_each(at -> 'fields') LOOP
                RETURN QUERY SELECT * FROM {name}(held -> part.key, part.value, types);
            END LOOP;
        END IF;
    ELSIF at ? 'variants' THEN
        IF jsonb_typeof(held) = 'object' AND jsonb_typeof(held -> 'tag') = 'string' THEN
            FOR part IN SELECT key, value FROM jsonb_each(COALESCE(at -> 'variants' -> (held ->> 'tag'), '{{}}'::jsonb)) LOOP
                RETURN QUERY SELECT * FROM {name}(held -> part.key, part.value, types);
            END LOOP;
        END IF;
    END IF;
END
$jadpo$",
        name = ident(FILES_WITHIN)
    )
}

/// The function each entity table's `jadpo_files` trigger runs (area 16,
/// "Deleting and erasing"; chunk B3), made in a store that keeps
/// `jadpo_files`: across the columns the trigger names, `File` columns and
/// structured values with files inside, however deep ([`FILES_WITHIN`]),
/// each file the row now holds more often than it did counts as many
/// holders more, refused once its bytes are being deleted or are gone, and
/// each it holds less often counts as many fewer, its `since` cleared so
/// the commit's end leaves the event that deletes its bytes once none
/// holds it. Files are counted in key order, so two writes wait for each
/// other rather than deadlocking. Its `RETURN NULL` changes no row.
pub const FILES_FUNCTION: &str = "jadpo_files_held";

/// The trigger's name on each table with a column holding files.
pub const FILES_TRIGGER: &str = "jadpo_files";

/// Creates [`FILES_FUNCTION`].
pub fn create_files_function() -> String {
    format!(
        "CREATE OR REPLACE FUNCTION {}() RETURNS trigger LANGUAGE plpgsql AS $jadpo$
DECLARE
    argument text;
    field_name text;
    within jsonb;
    row_before jsonb;
    row_after jsonb;
    held_before jsonb[] := '{{}}';
    held_after jsonb[] := '{{}}';
    change record;
BEGIN
    IF TG_OP <> 'INSERT' THEN
        row_before := to_jsonb(OLD);
    END IF;
    IF TG_OP <> 'DELETE' THEN
        row_after := to_jsonb(NEW);
    END IF;
    FOREACH argument IN ARRAY TG_ARGV LOOP
        IF left(argument, 1) = '{{' THEN
            within := argument::jsonb;
            field_name := within ->> 'column';
        ELSE
            within := '{{\"at\": \"file\"}}'::jsonb;
            field_name := argument;
        END IF;
        held_before := held_before || ARRAY(SELECT {walk}(row_before -> field_name, within -> 'at', within -> 'types'));
        held_after := held_after || ARRAY(SELECT {walk}(row_after -> field_name, within -> 'at', within -> 'types'));
    END LOOP;
    FOR change IN
        SELECT \"file\" ->> 'store' AS \"store\", \"file\" ->> 'key' AS \"key\", sum(\"n\") AS \"n\"
        FROM (
            SELECT \"file\", 1 AS \"n\" FROM unnest(held_after) AS \"file\"
            UNION ALL
            SELECT \"file\", -1 AS \"n\" FROM unnest(held_before) AS \"file\"
        ) AS \"each\"
        WHERE \"file\" ->> 'store' IS NOT NULL AND \"file\" ->> 'key' IS NOT NULL
        GROUP BY 1, 2
        HAVING sum(\"n\") <> 0
        ORDER BY 1, 2
    LOOP
        IF change.\"n\" > 0 THEN
            INSERT INTO \"jadpo_files\" (\"store\", \"key\", \"holders\", \"since\")
                VALUES (change.\"store\", change.\"key\", change.\"n\", NULL)
                ON CONFLICT (\"store\", \"key\") DO UPDATE
                SET \"holders\" = \"jadpo_files\".\"holders\" + EXCLUDED.\"holders\", \"since\" = NULL
                WHERE \"jadpo_files\".\"holders\" >= 0;
            IF NOT FOUND THEN
                RAISE EXCEPTION 'a File whose bytes are deleted can''t be kept in a record (%)', change.\"key\";
            END IF;
        ELSE
            UPDATE \"jadpo_files\" SET \"holders\" = GREATEST(\"holders\" + change.\"n\", 0), \"since\" = NULL
                WHERE \"store\" = change.\"store\"
                AND \"key\" = change.\"key\"
                AND \"holders\" > 0;
        END IF;
    END LOOP;
    RETURN NULL;
END
$jadpo$",
        ident(FILES_FUNCTION),
        walk = ident(FILES_WITHIN),
    )
}

/// Counts the holders of the files `columns` of `table` hold, in the
/// order its columns stand, as its rows change (chunk B3): one trigger
/// after each insert, each update of those columns and each delete,
/// cascades included.
pub fn create_files_trigger(table: &str, columns: &[Held]) -> String {
    let names: Vec<String> = columns.iter().map(|held| held.column.clone()).collect();
    let arguments: Vec<String> = columns
        .iter()
        .map(|held| text_literal(&held.argument()))
        .collect();
    format!(
        "CREATE TRIGGER {} AFTER INSERT OR UPDATE OF {} OR DELETE ON {} FOR EACH ROW EXECUTE FUNCTION {}({})",
        ident(FILES_TRIGGER),
        idents(&names),
        ident(table),
        ident(FILES_FUNCTION),
        arguments.join(", ")
    )
}

/// Drops `table`'s trigger, so one naming other columns can take its
/// place.
pub fn drop_files_trigger(table: &str) -> String {
    format!(
        "DROP TRIGGER IF EXISTS {} ON {}",
        ident(FILES_TRIGGER),
        ident(table)
    )
}

/// Counts the files `columns` of `table`'s rows already hold, as a store
/// that starts keeping `jadpo_files` finds them (chunk B3).
pub fn count_held_files(table: &str, columns: &[Held]) -> String {
    let held: Vec<String> = columns.iter().map(|held| held.files_of(table)).collect();
    format!(
        "INSERT INTO \"jadpo_files\" (\"store\", \"key\", \"holders\") SELECT \"held\" ->> 'store', \"held\" ->> 'key', count(*) FROM ({}) AS \"kept\" WHERE jsonb_typeof(\"held\") = 'object' GROUP BY 1, 2 ON CONFLICT (\"store\", \"key\") DO UPDATE SET \"holders\" = \"jadpo_files\".\"holders\" + EXCLUDED.\"holders\"",
        held.join(" UNION ALL ")
    )
}

/// Takes one holder away from each file `columns` of `table`'s rows hold,
/// as a drop's rows let go of them with no trigger to see it (chunk V3). A
/// file left with none is deleted by the tick, as any held by none is, so
/// `since` is cleared, as the trigger's function does; a file being deleted
/// or gone has a count below zero, which stays.
pub fn release_held_files(table: &str, columns: &[Held]) -> String {
    let held: Vec<String> = columns.iter().map(|held| held.files_of(table)).collect();
    format!(
        "UPDATE \"jadpo_files\" AS \"f\" SET \"holders\" = GREATEST(\"f\".\"holders\" - \"gone\".\"holders\", 0), \"since\" = NULL FROM (SELECT \"held\" ->> 'store' AS \"store\", \"held\" ->> 'key' AS \"key\", count(*) AS \"holders\" FROM ({}) AS \"kept\" WHERE jsonb_typeof(\"held\") = 'object' GROUP BY 1, 2) AS \"gone\" WHERE \"f\".\"store\" = \"gone\".\"store\" AND \"f\".\"key\" = \"gone\".\"key\" AND \"f\".\"holders\" > 0",
        held.join(" UNION ALL ")
    )
}

/// A line for a person, not the database: `-- text`.
pub fn comment(text: &str) -> String {
    format!("-- {text}")
}
