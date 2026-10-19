//! The physical schema (area 5, "Storage types" and "Migrations"): every
//! store's tables, columns, keys, references and indexes, with the names
//! the database knows them by. `jadpo-check` builds the schema the source
//! asks for; each migration folder holds the whole schema at that point as
//! `snapshot.json` ([`Schema::to_document`], [`Schema::read`]), and
//! [`changes`] lists what differs between two, which `jadpo check` reports
//! as drift and `jadpo migrate new` turns into SQL.
//!
//! Physical names never change once made (area 18, "Stable identity"): a
//! table is named after its entity's born name and a column after its
//! field's, so the snapshot keeps the born names beside them.

use std::collections::BTreeMap;
use std::fmt;

use crate::json::{self, Value};

/// The snapshot format this compiler writes and reads. It changes only
/// when a field is removed or changes meaning (area 18).
pub const FORMAT: i64 = 1;

/// The longest name PostgreSQL keeps: longer identifiers are cut, so two
/// long names could become one.
pub const MAX_NAME: usize = 63;

/// Every store's schema, by connection name.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Schema {
    pub stores: BTreeMap<String, Store>,
}

/// One store: its driver and its tables, by physical name.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Store {
    /// `postgres`: the driver's name in `Driver`.
    pub driver: String,
    pub tables: BTreeMap<String, Table>,
}

/// What a table holds.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Holds {
    /// An entity's records, by the entity's born name.
    Entity(String),
    /// One of the runtime's system tables (area 5, "System tables"), by
    /// what it is for: `outbox`, `deliveries`, `idempotency_keys`.
    System(String),
}

impl fmt::Display for Holds {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Holds::Entity(name) => write!(f, "entity `{name}`"),
            Holds::System(name) => write!(f, "system table for {}", name.replace('_', " ")),
        }
    }
}

/// One table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Table {
    pub holds: Holds,
    /// By physical name.
    pub columns: BTreeMap<String, Column>,
    pub primary_key: Key,
    /// Unique constraints, by name.
    pub unique: BTreeMap<String, Unique>,
    /// Foreign keys, by name.
    pub foreign_keys: BTreeMap<String, ForeignKey>,
    /// Indexes that aren't a constraint's, by name.
    pub indexes: BTreeMap<String, Index>,
    /// Check constraints, by name: a column that holds one of a few
    /// values, as a `Money` type that fixes its currency gives its
    /// currency column (area 19).
    pub checks: BTreeMap<String, Check>,
}

impl Table {
    /// The columns in the order they are declared.
    pub fn ordered(&self) -> Vec<(&String, &Column)> {
        let mut columns: Vec<_> = self.columns.iter().collect();
        columns.sort_by_key(|(name, column)| (column.position, name.as_str()));
        columns
    }

    /// The column holding field `field` (its born name).
    pub fn column_of(&self, field: &str) -> Option<(&String, &Column)> {
        self.columns
            .iter()
            .find(|(_, column)| column.field.as_deref() == Some(field))
    }
}

/// One column.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Column {
    /// Where it stands among the table's columns, from 0.
    pub position: u32,
    /// The born name of the field it holds; `None` in a system table.
    pub field: Option<String>,
    /// The column type as PostgreSQL writes it: `uuid`, `numeric(10, 2)`.
    pub sql_type: String,
    pub nullable: bool,
    /// Retired by a migration (area 5, "Removing data"): no field holds it,
    /// and it stays, nullable, until a later migration drops it.
    pub retired: bool,
    /// The values its field's type allows beyond what the column type
    /// says, which the application checks rather than the database: a
    /// change to them changes which stored values are valid, so it is a
    /// change to the schema like any other (area 5, "Storage types").
    /// `None` in a system table, and in a snapshot written before chunk
    /// 3.10, which didn't record it: the next migration does, with nothing
    /// to decide, since the application always checked it.
    pub allows: Option<Allows>,
    /// The comment PostgreSQL keeps on it: for a quantity, its kind and the
    /// SI unit it holds, `Length in metre (m)` (area 19, chunk 5.5w).
    pub comment: Option<String>,
}

/// What a column's field allows beyond its column type: an enum's
/// variants, and the field's bounds, lengths and patterns.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Allows {
    /// The variant names of an enum without data, sorted.
    pub values: Vec<String>,
    /// The bounds of a number, written as the numbers they are.
    pub min: Option<String>,
    pub max: Option<String>,
    /// A text's least and most characters.
    pub min_length: Option<u64>,
    pub max_length: Option<u64>,
    /// The regular expressions the whole text must match, sorted.
    pub patterns: Vec<String>,
    /// For a column holding a structured value as its JSON, what its type
    /// reads (chunk 5.9c): the value's shape with every rule inside it.
    /// `None` for any other column, and in a snapshot written before chunk
    /// 5.9c, which recorded nothing of it.
    pub json: Option<crate::stored_json::Reads>,
}

impl Allows {
    fn to_value(&self) -> Value {
        let mut value = Value::object();
        value.set_some("values", strings(&self.values));
        if let Some(min) = &self.min {
            value.set("min", min);
        }
        if let Some(max) = &self.max {
            value.set("max", max);
        }
        if let Some(n) = self.min_length {
            value.set("min_length", i64::try_from(n).unwrap_or(i64::MAX));
        }
        if let Some(n) = self.max_length {
            value.set("max_length", i64::try_from(n).unwrap_or(i64::MAX));
        }
        value.set_some("patterns", strings(&self.patterns));
        if let Some(json) = &self.json {
            value.set("json", json.to_value());
        }
        value
    }

    fn from_value(value: &Value) -> Result<Allows, String> {
        let text = |key: &str| value.get(key).and_then(Value::as_str).map(String::from);
        let count = |key: &str| match value.get(key) {
            Some(Value::Int(n)) => u64::try_from(*n).ok(),
            _ => None,
        };
        Ok(Allows {
            values: list_of(value, "values").unwrap_or_default(),
            min: text("min"),
            max: text("max"),
            min_length: count("min_length"),
            max_length: count("max_length"),
            patterns: list_of(value, "patterns").unwrap_or_default(),
            json: value
                .get("json")
                .map(crate::stored_json::Reads::from_value)
                .transpose()?,
        })
    }
}

/// A primary key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Key {
    pub name: String,
    pub columns: Vec<String>,
}

/// A unique constraint, and the rule it enforces, which a write's conflict
/// names (area 6): `Todo.email` for a field's `unique: true`,
/// `Project.name_per_owner` for a rule in the entity's `unique:` section.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unique {
    pub columns: Vec<String>,
    pub rule: String,
}

/// A check constraint: `column` holds one of `values`, written as text;
/// or, for a `geography` column, a geometry in WGS84 (SRID 4326) whose
/// `GeometryType` is one of `values` (`POINT`, area 20, chunk 5.5u4).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Check {
    pub column: String,
    pub values: Vec<String>,
    /// Whether `values` are the geometry types a `geography` holds.
    pub geography: bool,
}

/// A foreign key: the reference a field typed `X.id` makes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForeignKey {
    pub columns: Vec<String>,
    /// The referenced table, whose primary key it names.
    pub table: String,
    pub on_delete: OnDelete,
}

/// What deleting a referenced row does, as PostgreSQL says it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum OnDelete {
    Restrict,
    Cascade,
    SetNull,
}

impl OnDelete {
    /// As the snapshot writes it.
    pub fn word(self) -> &'static str {
        match self {
            OnDelete::Restrict => "restrict",
            OnDelete::Cascade => "cascade",
            OnDelete::SetNull => "set_null",
        }
    }

    /// As SQL writes it.
    pub fn sql(self) -> &'static str {
        match self {
            OnDelete::Restrict => "RESTRICT",
            OnDelete::Cascade => "CASCADE",
            OnDelete::SetNull => "SET NULL",
        }
    }

    fn from_word(word: &str) -> Option<OnDelete> {
        [OnDelete::Restrict, OnDelete::Cascade, OnDelete::SetNull]
            .into_iter()
            .find(|on| on.word() == word)
    }
}

/// An index, and why it exists: `reference Todo.owner_id` for a
/// reference's, `index Todo.by_due_at` for one the entity declares.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Index {
    pub columns: Vec<String>,
    pub reason: String,
    /// A partial index's condition, as SQL: the records a lifecycle's
    /// `visible:` lets normal reads see (area 5, "Indexes").
    pub predicate: Option<String>,
    /// The text columns it keeps in code-point order (`COLLATE "C"`), as a
    /// query sorts them (area 6), so a sorted read can use it.
    pub collate: Vec<String>,
    /// An access method other than a B-tree, with the operator class of
    /// its one column: `hnsw` with `vector_cosine_ops` for a vector a
    /// `nearest` query searches (area 21, chunk 5.5v1b).
    pub method: Option<IndexMethod>,
}

/// An index's access method and its column's operator class.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndexMethod {
    pub name: String,
    pub operator_class: String,
    /// The type its one column is cast to, which makes it an index of
    /// that expression: `halfvec(3072)` for a vector of 2,001 to 4,000
    /// dimensions, indexed at half precision (area 21, chunk 5.5v1c), or
    /// `geometry` for a `BoundingBox` filter on a point's `geography` (area
    /// 20, "Indexes", chunk 5.5u5).
    pub cast: Option<String>,
    /// Its storage parameters in the order written, `WITH (m = 24,
    /// ef_construction = 128)`: an HNSW index's tuning (area 21, chunk
    /// 5.5v1c).
    pub with: Vec<(String, u32)>,
}

impl IndexMethod {
    /// Whether it builds the same index as `other` but for its storage
    /// parameters: the one a tuned `indexes:` entry and a query's own
    /// would both be.
    pub fn same_but_tuning(&self, other: &IndexMethod) -> bool {
        self.name == other.name
            && self.operator_class == other.operator_class
            && self.cast == other.cast
    }
}

/// A name the database keeps whole: `table_part1_suffix`, cut to fit
/// [`MAX_NAME`] with a digest of the whole so two long names stay apart.
pub fn object_name(parts: &[&str], suffix: &str) -> String {
    let whole = format!("{}_{suffix}", parts.join("_"));
    if whole.len() <= MAX_NAME {
        return whole;
    }
    let digest = blake3::hash(whole.as_bytes()).to_hex();
    let keep = MAX_NAME - suffix.len() - 10;
    let mut head: String = parts.join("_");
    while head.len() > keep {
        head.pop();
    }
    format!("{head}_{}_{suffix}", &digest.as_str()[..8])
}

// --- snapshot.json -----------------------------------------------------------

impl Schema {
    /// The snapshot document, in canonical JSON.
    pub fn to_document(&self) -> String {
        let mut root = Value::object();
        root.set("format", FORMAT);
        let mut stores = Value::object();
        for (name, store) in &self.stores {
            let mut value = Value::object();
            value.set("driver", &store.driver);
            let mut tables = Value::object();
            for (table_name, table) in &store.tables {
                tables.set(table_name, table.to_value());
            }
            value.set("tables", tables);
            stores.set(name, value);
        }
        root.set("stores", stores);
        root.to_document()
    }

    /// Reads a snapshot document; the error says what is wrong.
    pub fn read(text: &str) -> Result<Schema, String> {
        let value = json::parse(text)?;
        match value.get("format") {
            Some(Value::Int(FORMAT)) => {}
            Some(Value::Int(other)) => {
                return Err(format!(
                    "it is format {other}, and this compiler reads format {FORMAT}"
                ));
            }
            _ => return Err("it has no `format`".into()),
        }
        let mut schema = Schema::default();
        for (name, store) in object(&value, "stores")? {
            let mut tables = BTreeMap::new();
            for (table_name, table) in object(store, "tables")? {
                tables.insert(
                    table_name.clone(),
                    Table::from_value(table).map_err(|e| format!("table `{table_name}`: {e}"))?,
                );
            }
            schema.stores.insert(
                name.clone(),
                Store {
                    driver: text_of(store, "driver")?,
                    tables,
                },
            );
        }
        if schema.to_document() != text {
            return Err("it isn't in the canonical form `jadpo migrate new` writes".into());
        }
        Ok(schema)
    }

    /// The digest of the snapshot document.
    pub fn digest(&self) -> String {
        blake3::hash(self.to_document().as_bytes()).to_hex()[..16].to_string()
    }
}

impl Table {
    fn to_value(&self) -> Value {
        let mut value = Value::object();
        match &self.holds {
            Holds::Entity(name) => value.set("entity", name),
            Holds::System(name) => value.set("system", name),
        }
        let mut columns = Value::object();
        for (name, column) in &self.columns {
            let mut entry = Value::object();
            entry.set("position", column.position);
            if let Some(field) = &column.field {
                entry.set("field", field);
            }
            entry.set("type", &column.sql_type);
            entry.set_some("nullable", column.nullable);
            entry.set_some("retired", column.retired);
            if let Some(allows) = &column.allows {
                entry.set("allows", allows.to_value());
            }
            if let Some(comment) = &column.comment {
                entry.set("comment", comment);
            }
            columns.set(name, entry);
        }
        value.set("columns", columns);
        let mut key = Value::object();
        key.set("name", &self.primary_key.name);
        key.set("columns", strings(&self.primary_key.columns));
        value.set("primary_key", key);
        let mut unique = Value::object();
        for (name, rule) in &self.unique {
            let mut entry = Value::object();
            entry.set("columns", strings(&rule.columns));
            entry.set("rule", &rule.rule);
            unique.set(name, entry);
        }
        value.set_some("unique", unique);
        let mut foreign = Value::object();
        for (name, key) in &self.foreign_keys {
            let mut entry = Value::object();
            entry.set("columns", strings(&key.columns));
            entry.set("table", &key.table);
            entry.set("on_delete", key.on_delete.word());
            foreign.set(name, entry);
        }
        value.set_some("foreign_keys", foreign);
        let mut indexes = Value::object();
        for (name, index) in &self.indexes {
            let mut entry = Value::object();
            entry.set("columns", strings(&index.columns));
            entry.set("reason", &index.reason);
            if let Some(predicate) = &index.predicate {
                entry.set("where", predicate);
            }
            entry.set_some("collate_c", strings(&index.collate));
            if let Some(method) = &index.method {
                entry.set("method", &method.name);
                entry.set("operator_class", &method.operator_class);
                if let Some(cast) = &method.cast {
                    entry.set("cast", cast);
                }
                let with: Vec<String> = method
                    .with
                    .iter()
                    .map(|(name, value)| format!("{name} = {value}"))
                    .collect();
                entry.set_some("with", strings(&with));
            }
            indexes.set(name, entry);
        }
        value.set_some("indexes", indexes);
        let mut checks = Value::object();
        for (name, check) in &self.checks {
            let mut entry = Value::object();
            entry.set("column", &check.column);
            entry.set("values", strings(&check.values));
            if check.geography {
                entry.set("geography", true);
            }
            checks.set(name, entry);
        }
        value.set_some("checks", checks);
        value
    }

    fn from_value(value: &Value) -> Result<Table, String> {
        let holds = match (value.get("entity"), value.get("system")) {
            (Some(Value::Str(name)), None) => Holds::Entity(name.clone()),
            (None, Some(Value::Str(name))) => Holds::System(name.clone()),
            _ => return Err("it names neither an `entity` nor a `system` table".into()),
        };
        let mut columns = BTreeMap::new();
        for (name, entry) in object(value, "columns")? {
            let position = match entry.get("position") {
                Some(Value::Int(n)) => u32::try_from(*n).map_err(|_| "a bad `position`")?,
                _ => return Err(format!("column `{name}` has no `position`")),
            };
            columns.insert(
                name.clone(),
                Column {
                    position,
                    field: entry.get("field").and_then(Value::as_str).map(String::from),
                    sql_type: text_of(entry, "type")?,
                    nullable: flag(entry, "nullable"),
                    retired: flag(entry, "retired"),
                    allows: entry
                        .get("allows")
                        .map(Allows::from_value)
                        .transpose()
                        .map_err(|e| format!("column `{name}`: {e}"))?,
                    comment: entry
                        .get("comment")
                        .and_then(Value::as_str)
                        .map(String::from),
                },
            );
        }
        let key = value.get("primary_key").ok_or("it has no `primary_key`")?;
        let primary_key = Key {
            name: text_of(key, "name")?,
            columns: list_of(key, "columns")?,
        };
        let mut unique = BTreeMap::new();
        for (name, entry) in optional_object(value, "unique")? {
            unique.insert(
                name.clone(),
                Unique {
                    columns: list_of(entry, "columns")?,
                    rule: text_of(entry, "rule")?,
                },
            );
        }
        let mut foreign_keys = BTreeMap::new();
        for (name, entry) in optional_object(value, "foreign_keys")? {
            let on_delete = text_of(entry, "on_delete")?;
            foreign_keys.insert(
                name.clone(),
                ForeignKey {
                    columns: list_of(entry, "columns")?,
                    table: text_of(entry, "table")?,
                    on_delete: OnDelete::from_word(&on_delete)
                        .ok_or_else(|| format!("`{on_delete}` isn't an `on_delete`"))?,
                },
            );
        }
        let mut indexes = BTreeMap::new();
        for (name, entry) in optional_object(value, "indexes")? {
            indexes.insert(
                name.clone(),
                Index {
                    columns: list_of(entry, "columns")?,
                    reason: text_of(entry, "reason")?,
                    predicate: entry.get("where").and_then(Value::as_str).map(String::from),
                    collate: list_of(entry, "collate_c").unwrap_or_default(),
                    method: match entry.get("method").and_then(Value::as_str) {
                        Some(method) => Some(IndexMethod {
                            name: method.to_string(),
                            operator_class: text_of(entry, "operator_class")?,
                            cast: entry.get("cast").and_then(Value::as_str).map(String::from),
                            with: list_of(entry, "with")
                                .unwrap_or_default()
                                .iter()
                                .map(|setting| {
                                    setting
                                        .split_once(" = ")
                                        .and_then(|(name, value)| {
                                            Some((name.to_string(), value.parse().ok()?))
                                        })
                                        .ok_or_else(|| {
                                            format!("`{setting}` isn't an index's setting")
                                        })
                                })
                                .collect::<Result<_, _>>()?,
                        }),
                        None => None,
                    },
                },
            );
        }
        let mut checks = BTreeMap::new();
        for (name, entry) in optional_object(value, "checks")? {
            checks.insert(
                name.clone(),
                Check {
                    column: text_of(entry, "column")?,
                    values: list_of(entry, "values")?,
                    geography: matches!(entry.get("geography"), Some(Value::Bool(true))),
                },
            );
        }
        Ok(Table {
            holds,
            columns,
            primary_key,
            unique,
            foreign_keys,
            indexes,
            checks,
        })
    }
}

fn strings(items: &[String]) -> Value {
    Value::Array(items.iter().map(Value::from).collect())
}

fn object<'v>(value: &'v Value, key: &str) -> Result<&'v BTreeMap<String, Value>, String> {
    match value.get(key) {
        Some(Value::Object(entries)) => Ok(entries),
        _ => Err(format!("it has no `{key}` object")),
    }
}

fn optional_object<'v>(
    value: &'v Value,
    key: &str,
) -> Result<std::collections::btree_map::Iter<'v, String, Value>, String> {
    static EMPTY: BTreeMap<String, Value> = BTreeMap::new();
    match value.get(key) {
        None => Ok(EMPTY.iter()),
        Some(Value::Object(entries)) => Ok(entries.iter()),
        Some(_) => Err(format!("`{key}` isn't an object")),
    }
}

fn text_of(value: &Value, key: &str) -> Result<String, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(String::from)
        .ok_or_else(|| format!("it has no `{key}` text"))
}

fn list_of(value: &Value, key: &str) -> Result<Vec<String>, String> {
    value
        .get(key)
        .and_then(Value::as_array)
        .and_then(|items| {
            items
                .iter()
                .map(|item| item.as_str().map(String::from))
                .collect()
        })
        .ok_or_else(|| format!("it has no `{key}` list of names"))
}

fn flag(value: &Value, key: &str) -> bool {
    matches!(value.get(key), Some(Value::Bool(true)))
}

// --- changes -----------------------------------------------------------------

/// One difference between two schemas, in the order a migration makes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Change {
    AddStore {
        store: String,
    },
    AddTable {
        store: String,
        table: String,
    },
    DropTable {
        store: String,
        table: String,
    },
    AddColumn {
        store: String,
        table: String,
        column: String,
    },
    /// A column the source no longer has a field for.
    RemoveColumn {
        store: String,
        table: String,
        column: String,
    },
    /// A column whose type, nullability or retirement differs.
    AlterColumn {
        store: String,
        table: String,
        column: String,
    },
    AddUnique {
        store: String,
        table: String,
        name: String,
    },
    DropUnique {
        store: String,
        table: String,
        name: String,
    },
    AddForeignKey {
        store: String,
        table: String,
        name: String,
    },
    DropForeignKey {
        store: String,
        table: String,
        name: String,
    },
    AddIndex {
        store: String,
        table: String,
        name: String,
    },
    DropIndex {
        store: String,
        table: String,
        name: String,
    },
    AddCheck {
        store: String,
        table: String,
        name: String,
    },
    DropCheck {
        store: String,
        table: String,
        name: String,
    },
}

/// What differs from `before` to `after`.
pub fn changes(before: &Schema, after: &Schema) -> Vec<Change> {
    let empty = Store::default();
    let mut out = Vec::new();
    for (store_name, store) in &after.stores {
        let old = before.stores.get(store_name).unwrap_or(&empty);
        if !before.stores.contains_key(store_name) {
            out.push(Change::AddStore {
                store: store_name.clone(),
            });
        }
        for (table_name, table) in &store.tables {
            match old.tables.get(table_name) {
                None => out.push(Change::AddTable {
                    store: store_name.clone(),
                    table: table_name.clone(),
                }),
                Some(was) => table_changes(store_name, table_name, was, table, &mut out),
            }
        }
        for table_name in old.tables.keys() {
            if !store.tables.contains_key(table_name) {
                out.push(Change::DropTable {
                    store: store_name.clone(),
                    table: table_name.clone(),
                });
            }
        }
    }
    for (store_name, store) in &before.stores {
        if !after.stores.contains_key(store_name) {
            for table_name in store.tables.keys() {
                out.push(Change::DropTable {
                    store: store_name.clone(),
                    table: table_name.clone(),
                });
            }
        }
    }
    out
}

fn table_changes(store: &str, table: &str, was: &Table, now: &Table, out: &mut Vec<Change>) {
    let at = |kind: fn(String, String, String) -> Change, name: &str| {
        kind(store.to_string(), table.to_string(), name.to_string())
    };
    for (name, column) in &now.columns {
        match was.columns.get(name) {
            None => out.push(at(add_column, name)),
            Some(old) if old != column => out.push(at(alter_column, name)),
            Some(_) => {}
        }
    }
    for name in was.columns.keys() {
        if !now.columns.contains_key(name) {
            out.push(at(remove_column, name));
        }
    }
    keyed(&was.unique, &now.unique, |name, added| {
        out.push(at(if added { add_unique } else { drop_unique }, name));
    });
    keyed(&was.foreign_keys, &now.foreign_keys, |name, added| {
        out.push(at(if added { add_foreign } else { drop_foreign }, name));
    });
    keyed(&was.indexes, &now.indexes, |name, added| {
        out.push(at(if added { add_index } else { drop_index }, name));
    });
    keyed(&was.checks, &now.checks, |name, added| {
        out.push(at(if added { add_check } else { drop_check }, name));
    });
}

/// Calls `found(name, false)` for each entry gone or changed, then
/// `found(name, true)` for each new or changed one: a changed constraint
/// is dropped and made again.
fn keyed<T: PartialEq>(
    was: &BTreeMap<String, T>,
    now: &BTreeMap<String, T>,
    mut found: impl FnMut(&str, bool),
) {
    for (name, old) in was {
        if now.get(name) != Some(old) {
            found(name, false);
        }
    }
    for (name, new) in now {
        if was.get(name) != Some(new) {
            found(name, true);
        }
    }
}

fn add_column(store: String, table: String, column: String) -> Change {
    Change::AddColumn {
        store,
        table,
        column,
    }
}
fn remove_column(store: String, table: String, column: String) -> Change {
    Change::RemoveColumn {
        store,
        table,
        column,
    }
}
fn alter_column(store: String, table: String, column: String) -> Change {
    Change::AlterColumn {
        store,
        table,
        column,
    }
}
fn add_unique(store: String, table: String, name: String) -> Change {
    Change::AddUnique { store, table, name }
}
fn drop_unique(store: String, table: String, name: String) -> Change {
    Change::DropUnique { store, table, name }
}
fn add_foreign(store: String, table: String, name: String) -> Change {
    Change::AddForeignKey { store, table, name }
}
fn drop_foreign(store: String, table: String, name: String) -> Change {
    Change::DropForeignKey { store, table, name }
}
fn add_index(store: String, table: String, name: String) -> Change {
    Change::AddIndex { store, table, name }
}
fn drop_index(store: String, table: String, name: String) -> Change {
    Change::DropIndex { store, table, name }
}
fn add_check(store: String, table: String, name: String) -> Change {
    Change::AddCheck { store, table, name }
}
fn drop_check(store: String, table: String, name: String) -> Change {
    Change::DropCheck { store, table, name }
}

impl Change {
    /// The store and table it is about.
    pub fn table(&self) -> (&str, Option<&str>) {
        match self {
            Change::AddStore { store } => (store, None),
            Change::AddTable { store, table }
            | Change::DropTable { store, table }
            | Change::AddColumn { store, table, .. }
            | Change::RemoveColumn { store, table, .. }
            | Change::AlterColumn { store, table, .. }
            | Change::AddUnique { store, table, .. }
            | Change::DropUnique { store, table, .. }
            | Change::AddForeignKey { store, table, .. }
            | Change::DropForeignKey { store, table, .. }
            | Change::AddIndex { store, table, .. }
            | Change::DropIndex { store, table, .. }
            | Change::AddCheck { store, table, .. }
            | Change::DropCheck { store, table, .. } => (store, Some(table)),
        }
    }

    /// What changed, as a person reads it: `new field \`Todo.priority\``.
    /// `before` and `after` are the schemas it was found between.
    pub fn describe(&self, before: &Schema, after: &Schema) -> String {
        let table = |schema: &Schema, store: &str, table: &str| {
            schema
                .stores
                .get(store)
                .and_then(|s| s.tables.get(table))
                .cloned()
        };
        let holds = |store: &str, name: &str| {
            table(after, store, name)
                .or_else(|| table(before, store, name))
                .map_or_else(|| format!("table `{name}`"), |t| t.holds.to_string())
        };
        let field = |store: &str, name: &str, column: &str| {
            let found = table(after, store, name).or_else(|| table(before, store, name));
            match found {
                Some(Table {
                    holds: Holds::Entity(entity),
                    columns,
                    ..
                }) => {
                    let field = columns
                        .get(column)
                        .and_then(|c| c.field.clone())
                        .or_else(|| {
                            table(before, store, name)
                                .and_then(|t| t.columns.get(column).and_then(|c| c.field.clone()))
                        })
                        .unwrap_or_else(|| column.to_string());
                    format!("field `{entity}.{field}`")
                }
                Some(t) => format!("column `{column}` of the {}", t.holds),
                None => format!("column `{name}.{column}`"),
            }
        };
        match self {
            Change::AddStore { store } => format!("new store `{store}`"),
            Change::AddTable { store, table } => format!("new {}", holds(store, table)),
            Change::DropTable { store, table } => format!("{} removed", holds(store, table)),
            Change::AddColumn {
                store,
                table,
                column,
            } => format!("new {}", field(store, table, column)),
            Change::RemoveColumn {
                store,
                table,
                column,
            } => format!("{} removed", field(store, table, column)),
            Change::AlterColumn {
                store,
                table,
                column,
            } => {
                let retired = self::table(after, store, table)
                    .and_then(|t| t.columns.get(column))
                    .is_some_and(|c| c.retired);
                if retired {
                    format!("{} removed", field(store, table, column))
                } else {
                    format!("{} changed", field(store, table, column))
                }
            }
            Change::AddUnique { store, table, name }
            | Change::DropUnique { store, table, name } => {
                let rule = self::table(after, store, table)
                    .and_then(|t| t.unique.get(name).map(|u| u.rule.clone()))
                    .or_else(|| {
                        self::table(before, store, table)
                            .and_then(|t| t.unique.get(name).map(|u| u.rule.clone()))
                    })
                    .unwrap_or_else(|| name.clone());
                let verb = if matches!(self, Change::AddUnique { .. }) {
                    "new"
                } else {
                    "removed"
                };
                format!("{verb} unique rule `{rule}`")
            }
            Change::AddForeignKey { store, table, name }
            | Change::DropForeignKey { store, table, name } => {
                let verb = if matches!(self, Change::AddForeignKey { .. }) {
                    "new"
                } else {
                    "removed"
                };
                format!("{verb} reference `{name}` on {}", holds(store, table))
            }
            Change::AddIndex { store, table, name } | Change::DropIndex { store, table, name } => {
                let reason = self::table(after, store, table)
                    .and_then(|t| t.indexes.get(name).map(|i| i.reason.clone()))
                    .or_else(|| {
                        self::table(before, store, table)
                            .and_then(|t| t.indexes.get(name).map(|i| i.reason.clone()))
                    })
                    .unwrap_or_else(|| format!("index {name}"));
                let verb = if matches!(self, Change::AddIndex { .. }) {
                    "new"
                } else {
                    "removed"
                };
                match reason.split_once(' ') {
                    Some(("query", what)) => format!("{verb} index for query `{what}`"),
                    Some(("reference", what)) => format!("{verb} index on reference `{what}`"),
                    Some((kind, what)) => format!("{verb} {kind} `{what}`"),
                    None => format!("{verb} {reason}"),
                }
            }
            Change::AddCheck { store, table, name } | Change::DropCheck { store, table, name } => {
                let check = self::table(after, store, table)
                    .and_then(|t| t.checks.get(name).cloned())
                    .or_else(|| {
                        self::table(before, store, table).and_then(|t| t.checks.get(name).cloned())
                    });
                let verb = if matches!(self, Change::AddCheck { .. }) {
                    "new"
                } else {
                    "removed"
                };
                match check {
                    Some(check) if check.geography => format!(
                        "{verb} check that `{}` is {} in WGS84",
                        check.column,
                        check
                            .values
                            .iter()
                            .map(|v| format!("`{v}`"))
                            .collect::<Vec<_>>()
                            .join(" or ")
                    ),
                    Some(check) => format!(
                        "{verb} check that `{}` is {}",
                        check.column,
                        check
                            .values
                            .iter()
                            .map(|v| format!("`{v}`"))
                            .collect::<Vec<_>>()
                            .join(" or ")
                    ),
                    None => format!("{verb} check `{name}`"),
                }
            }
        }
    }
}

fn table<'s>(schema: &'s Schema, store: &str, name: &str) -> Option<&'s Table> {
    schema.stores.get(store).and_then(|s| s.tables.get(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn todo(extra: bool) -> Schema {
        let mut columns = BTreeMap::new();
        columns.insert(
            "id".to_string(),
            Column {
                position: 0,
                field: Some("id".into()),
                sql_type: "uuid".into(),
                nullable: false,
                retired: false,
                allows: None,
                comment: None,
            },
        );
        if extra {
            columns.insert(
                "due_at".to_string(),
                Column {
                    position: 1,
                    field: Some("due_at".into()),
                    sql_type: "timestamptz(3)".into(),
                    nullable: true,
                    retired: false,
                    allows: None,
                    comment: None,
                },
            );
        }
        let table = Table {
            holds: Holds::Entity("Todo".into()),
            columns,
            primary_key: Key {
                name: "todo_pkey".into(),
                columns: vec!["id".into()],
            },
            unique: BTreeMap::new(),
            foreign_keys: BTreeMap::new(),
            indexes: BTreeMap::new(),
            checks: BTreeMap::new(),
        };
        let mut store = Store {
            driver: "postgres".into(),
            tables: BTreeMap::new(),
        };
        store.tables.insert("todo".into(), table);
        let mut schema = Schema::default();
        schema.stores.insert("main".into(), store);
        schema
    }

    #[test]
    fn a_snapshot_reads_back_as_written_and_only_in_canonical_form() {
        let schema = todo(true);
        let document = schema.to_document();
        assert_eq!(Schema::read(&document), Ok(schema));
        assert!(Schema::read(&document.replace("  ", "    ")).is_err());
        assert!(Schema::read("{\"format\": 2, \"stores\": {}}\n").is_err());
    }

    #[test]
    fn a_tuned_half_precision_index_reads_back_as_written() {
        // Area 21, "The index" (chunk 5.5v1c): its cast and its storage
        // parameters, in the order written.
        let mut schema = todo(true);
        let table = schema
            .stores
            .get_mut("main")
            .and_then(|store| store.tables.get_mut("todo"))
            .expect("the table");
        table.indexes.insert(
            "todo_meaning_idx".into(),
            Index {
                columns: vec!["meaning".into()],
                reason: "index Todo.by_meaning".into(),
                predicate: None,
                collate: Vec::new(),
                method: Some(IndexMethod {
                    name: "hnsw".into(),
                    operator_class: "halfvec_cosine_ops".into(),
                    cast: Some("halfvec(3072)".into()),
                    with: vec![("m".into(), 24), ("ef_construction".into(), 128)],
                }),
            },
        );
        let document = schema.to_document();
        assert!(document.contains("\"with\": [\n"), "{document}");
        assert_eq!(Schema::read(&document), Ok(schema));
    }

    #[test]
    fn changes_name_what_a_person_would() {
        let before = todo(false);
        let after = todo(true);
        let found = changes(&before, &after);
        assert_eq!(
            found,
            vec![Change::AddColumn {
                store: "main".into(),
                table: "todo".into(),
                column: "due_at".into(),
            }]
        );
        assert_eq!(
            found[0].describe(&before, &after),
            "new field `Todo.due_at`"
        );
        let gone = changes(&after, &before);
        assert_eq!(
            gone[0].describe(&after, &before),
            "field `Todo.due_at` removed"
        );
        let fresh = changes(&Schema::default(), &before);
        assert_eq!(fresh.len(), 2);
        assert_eq!(
            fresh[1].describe(&Schema::default(), &before),
            "new entity `Todo`"
        );
    }

    #[test]
    fn long_names_are_cut_with_a_digest() {
        assert_eq!(
            object_name(&["todo", "owner_id"], "fkey"),
            "todo_owner_id_fkey"
        );
        let long = "a".repeat(70);
        let cut = object_name(&[&long, "b"], "key");
        assert!(cut.len() <= MAX_NAME && cut.ends_with("_key"));
        assert_ne!(cut, object_name(&[&long, "c"], "key"));
    }
}
