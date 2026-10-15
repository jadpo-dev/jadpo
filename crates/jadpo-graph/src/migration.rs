//! A migration from one schema to the next (area 5, "Migrations"): which
//! changes need a person's decision, the snapshot the migration records,
//! and the SQL that gets there, for every store. Pure functions of the two
//! schemas and the decisions, so `jadpo migrate new` writes exactly what
//! `jadpo check` expects to find.
//!
//! A migration's `postgres.sql` holds one section per store, each opened
//! by a `-- store: <name>` line; `migrate apply` and the test database run
//! a store's section in one transaction, and record its digest.

use std::collections::BTreeMap;

use crate::schema::{Change, Column, Holds, Schema, Table, changes};

/// A decision a change needs before its SQL can be written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Needed {
    pub store: String,
    pub table: String,
    pub column: String,
    /// The entity's and field's born names, as `changes.jadpo` names them.
    pub entity: String,
    pub field: String,
    pub kind: NeededKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NeededKind {
    /// A required column existing rows have no value for: a new one, or
    /// one that could be `none` until now.
    Backfill,
    /// A field gone from its entity: its column is retired, kept with its
    /// data until a later migration drops it.
    Retire,
}

impl Needed {
    /// `Todo.priority`.
    pub fn key(&self) -> String {
        format!("{}.{}", self.entity, self.field)
    }
}

/// What a person decided about one change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Decided {
    /// The SQL literal existing rows get.
    Backfill(String),
    Retired,
}

/// A change this compiler can't migrate yet, and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refusal {
    pub change: String,
    pub reason: String,
}

/// The snapshot the next migration records, from the latest one and the
/// schema the source asks for: retired columns stay, and a column whose
/// field is gone is retired (the only first step a removal has).
pub fn next_snapshot(latest: &Schema, source: &Schema) -> Schema {
    let mut out = source.clone();
    for (store_name, store) in &latest.stores {
        for (table_name, table) in &store.tables {
            let Some(now) = out
                .stores
                .get_mut(store_name)
                .and_then(|s| s.tables.get_mut(table_name))
            else {
                continue;
            };
            if !matches!(table.holds, Holds::Entity(_)) {
                continue;
            }
            for (column_name, column) in &table.columns {
                if !now.columns.contains_key(column_name) {
                    now.columns.insert(
                        column_name.clone(),
                        Column {
                            nullable: true,
                            retired: true,
                            ..column.clone()
                        },
                    );
                }
            }
        }
    }
    out
}

/// The decisions the migration from `before` to `after` (a snapshot from
/// [`next_snapshot`]) needs.
pub fn decisions_needed(before: &Schema, after: &Schema) -> Vec<Needed> {
    let mut out = Vec::new();
    for change in changes(before, after) {
        let (store, table, column) = match &change {
            Change::AddColumn {
                store,
                table,
                column,
            }
            | Change::AlterColumn {
                store,
                table,
                column,
            } => (store, table, column),
            _ => continue,
        };
        let (Some(old), Some(new)) = (
            table_in(before, store, table),
            table_in(after, store, table),
        ) else {
            continue;
        };
        let Holds::Entity(entity) = &new.holds else {
            continue;
        };
        let was = old.columns.get(column);
        let Some(now) = new.columns.get(column) else {
            continue;
        };
        let kind = match was {
            None if !now.nullable => NeededKind::Backfill,
            Some(was) if !was.retired && now.retired => NeededKind::Retire,
            Some(was) if was.nullable && !now.nullable && !now.retired => NeededKind::Backfill,
            _ => continue,
        };
        out.push(Needed {
            store: store.clone(),
            table: table.clone(),
            column: column.clone(),
            entity: entity.clone(),
            field: now.field.clone().unwrap_or_else(|| column.clone()),
            kind,
        });
    }
    out
}

fn table_in<'s>(schema: &'s Schema, store: &str, table: &str) -> Option<&'s Table> {
    schema.stores.get(store).and_then(|s| s.tables.get(table))
}

/// A migration's SQL, per store, and a word for each statement.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Sql {
    /// Each store's statements, in the order they run.
    pub stores: BTreeMap<String, Vec<Statement>>,
}

/// One statement and what it does, in words: `add column`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Statement {
    pub sql: String,
    pub does: &'static str,
}

impl Sql {
    /// The `postgres.sql` document.
    pub fn to_document(&self, name: &str) -> String {
        let mut out = format!(
            "-- {name}: written by `jadpo migrate new` from the snapshots; never edit it by hand.\n"
        );
        for (store, statements) in &self.stores {
            out.push_str(&format!("\n-- store: {store}\n"));
            for statement in statements {
                out.push_str(&statement.sql);
                out.push_str(";\n");
            }
        }
        out
    }

    pub fn count(&self) -> usize {
        self.stores.values().map(Vec::len).sum()
    }

    /// What its statements do, each kind once, in order of first use.
    pub fn summary(&self) -> Vec<&'static str> {
        let mut out: Vec<&'static str> = Vec::new();
        for statement in self.stores.values().flatten() {
            if !out.contains(&statement.does) {
                out.push(statement.does);
            }
        }
        out
    }
}

/// A store's section of a `postgres.sql`, as `migrate apply` runs it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Section {
    pub store: String,
    pub sql: String,
}

/// The sections of a `postgres.sql` document, in order.
pub fn sections(document: &str) -> Vec<Section> {
    let mut out: Vec<Section> = Vec::new();
    for line in document.lines() {
        if let Some(store) = line.strip_prefix("-- store: ") {
            out.push(Section {
                store: store.trim().to_string(),
                sql: String::new(),
            });
            continue;
        }
        if let Some(section) = out.last_mut() {
            section.sql.push_str(line);
            section.sql.push('\n');
        }
    }
    for section in &mut out {
        section.sql = section.sql.trim().to_string();
    }
    out
}

/// The digest a store records for a migration's section.
pub fn section_digest(sql: &str) -> String {
    blake3::hash(sql.as_bytes()).to_hex()[..16].to_string()
}

/// A double-quoted identifier, so a table called `user` is a name, not a
/// keyword.
pub fn ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

fn idents(names: &[String]) -> String {
    names
        .iter()
        .map(|n| ident(n))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The SQL from `before` to `after`, with `decided` giving each needed
/// decision (`None` while it is still to make, which leaves a comment in
/// its place). Changes this compiler can't migrate yet are refused.
pub fn migration_sql(
    before: &Schema,
    after: &Schema,
    decided: &dyn Fn(&Needed) -> Option<Decided>,
) -> Result<Sql, Vec<Refusal>> {
    let all = changes(before, after);
    let needed = decisions_needed(before, after);
    let mut refusals = Vec::new();
    let mut plans: BTreeMap<String, Plan> = BTreeMap::new();
    for change in &all {
        let (store, table) = change.table();
        let plan = plans.entry(store.to_string()).or_default();
        let new = table.and_then(|t| table_in(after, store, t));
        let old = table.and_then(|t| table_in(before, store, t));
        match change {
            Change::AddStore { .. } => {}
            Change::AddTable { table, .. } => {
                if let Some(new) = new {
                    plan.create.push(Statement {
                        sql: create_table(table, new),
                        does: "create table",
                    });
                    for name in new.unique.keys() {
                        plan.constraints.push(add_unique(table, name, new));
                    }
                    for name in new.foreign_keys.keys() {
                        plan.references.push(add_foreign_key(table, name, new));
                    }
                    for name in new.indexes.keys() {
                        plan.indexes.push(create_index(table, name, new));
                    }
                }
            }
            Change::DropTable { table, .. } => {
                let what = old.map_or_else(|| format!("table `{table}`"), |t| t.holds.to_string());
                refusals.push(Refusal {
                    change: format!("{what} removed"),
                    reason: "Removing an entity, or moving it to another store, needs a decision whose form isn't decided yet, so this compiler can't migrate it.".into(),
                });
            }
            Change::AddColumn { table, column, .. } => {
                let Some(col) = new.and_then(|t| t.columns.get(column)) else {
                    continue;
                };
                let need = needed
                    .iter()
                    .find(|n| &n.table == table && &n.column == column);
                let add = format!(
                    "ALTER TABLE {} ADD COLUMN {} {}",
                    ident(table),
                    ident(column),
                    col.sql_type
                );
                plan.columns.push(Statement {
                    sql: add,
                    does: "add column",
                });
                if let Some(need) = need {
                    plan.columns.extend(backfill(table, column, need, decided));
                }
            }
            Change::AlterColumn { table, column, .. } => {
                let (Some(was), Some(now)) = (
                    old.and_then(|t| t.columns.get(column)),
                    new.and_then(|t| t.columns.get(column)),
                ) else {
                    continue;
                };
                if was.sql_type != now.sql_type {
                    if widens(&was.sql_type, &now.sql_type) {
                        plan.columns.push(Statement {
                            sql: format!(
                                "ALTER TABLE {} ALTER COLUMN {} TYPE {}",
                                ident(table),
                                ident(column),
                                now.sql_type
                            ),
                            does: "change column type",
                        });
                    } else {
                        refusals.push(Refusal {
                            change: format!(
                                "column `{table}.{column}` changes from `{}` to `{}`",
                                was.sql_type, now.sql_type
                            ),
                            reason: "Only a change that keeps every stored value (a wider whole number, more digits) is migrated by this compiler; others need a conversion whose form isn't decided yet.".into(),
                        });
                        continue;
                    }
                }
                if was.retired && !now.retired {
                    refusals.push(Refusal {
                        change: format!("retired column `{table}.{column}` comes back"),
                        reason: "A field can't take back a retired column's name until a later migration drops it, which needs acceptances (area 18).".into(),
                    });
                    continue;
                }
                let need = needed
                    .iter()
                    .find(|n| &n.table == table && &n.column == column);
                match need {
                    Some(need) => plan.columns.extend(backfill(table, column, need, decided)),
                    None if !was.nullable && now.nullable => plan.columns.push(Statement {
                        sql: format!(
                            "ALTER TABLE {} ALTER COLUMN {} DROP NOT NULL",
                            ident(table),
                            ident(column)
                        ),
                        does: "allow none",
                    }),
                    None => {}
                }
            }
            Change::RemoveColumn { table, column, .. } => {
                refusals.push(Refusal {
                    change: format!("column `{table}.{column}` dropped"),
                    reason: "A column is dropped only after it was retired, and dropping needs acceptances (area 18), which aren't built yet.".into(),
                });
            }
            Change::DropForeignKey { table, name, .. } | Change::DropUnique { table, name, .. } => {
                plan.drop.push(Statement {
                    sql: format!(
                        "ALTER TABLE {} DROP CONSTRAINT {}",
                        ident(table),
                        ident(name)
                    ),
                    does: "drop constraint",
                });
            }
            Change::DropIndex { name, .. } => plan.drop.push(Statement {
                sql: format!("DROP INDEX {}", ident(name)),
                does: "drop index",
            }),
            Change::AddUnique { table, name, .. } => {
                if let Some(new) = new {
                    plan.constraints.push(add_unique(table, name, new));
                }
            }
            Change::AddForeignKey { table, name, .. } => {
                if let Some(new) = new {
                    plan.references.push(add_foreign_key(table, name, new));
                }
            }
            Change::AddIndex { table, name, .. } => {
                if let Some(new) = new {
                    plan.indexes.push(create_index(table, name, new));
                }
            }
        }
    }
    if !refusals.is_empty() {
        return Err(refusals);
    }
    let mut out = Sql::default();
    for (store, plan) in plans {
        let statements: Vec<Statement> = plan
            .drop
            .into_iter()
            .chain(plan.create)
            .chain(plan.columns)
            .chain(plan.constraints)
            .chain(plan.references)
            .chain(plan.indexes)
            .collect();
        if !statements.is_empty() {
            out.stores.insert(store, statements);
        }
    }
    Ok(out)
}

/// One store's statements, grouped so each can depend on the ones before:
/// constraints are dropped first, tables made before columns change, and
/// references added once every table they name exists.
#[derive(Default)]
struct Plan {
    drop: Vec<Statement>,
    create: Vec<Statement>,
    columns: Vec<Statement>,
    constraints: Vec<Statement>,
    references: Vec<Statement>,
    indexes: Vec<Statement>,
}

fn backfill(
    table: &str,
    column: &str,
    need: &Needed,
    decided: &dyn Fn(&Needed) -> Option<Decided>,
) -> Vec<Statement> {
    let (table, column) = (ident(table), ident(column));
    match (need.kind, decided(need)) {
        (NeededKind::Backfill, Some(Decided::Backfill(value))) => vec![
            Statement {
                sql: format!("UPDATE {table} SET {column} = {value} WHERE {column} IS NULL"),
                does: "fill existing rows",
            },
            Statement {
                sql: format!("ALTER TABLE {table} ALTER COLUMN {column} SET NOT NULL"),
                does: "require a value",
            },
        ],
        (NeededKind::Retire, Some(Decided::Retired)) => vec![Statement {
            sql: format!("ALTER TABLE {table} ALTER COLUMN {column} DROP NOT NULL"),
            does: "retire column",
        }],
        _ => vec![Statement {
            sql: format!(
                "-- waiting for the decision about {} in changes.jadpo",
                need.key()
            ),
            does: "decision to make",
        }],
    }
}

/// Whether a column type changes to one that holds every value it held.
fn widens(from: &str, to: &str) -> bool {
    let rank = |t: &str| match t {
        "smallint" => Some(0),
        "integer" => Some(1),
        "bigint" => Some(2),
        _ => None,
    };
    if let (Some(a), Some(b)) = (rank(from), rank(to)) {
        return a < b;
    }
    let numeric = |t: &str| -> Option<(u32, u32)> {
        let inner = t.strip_prefix("numeric(")?.strip_suffix(')')?;
        let (p, s) = inner.split_once(", ")?;
        Some((p.parse().ok()?, s.parse().ok()?))
    };
    match (numeric(from), numeric(to)) {
        (Some((p1, s1)), Some((p2, s2))) => s1 == s2 && p2 >= p1,
        (Some(_), None) => to == "numeric",
        _ => false,
    }
}

fn add_unique(table: &str, name: &str, def: &Table) -> Statement {
    let columns = def
        .unique
        .get(name)
        .map(|u| idents(&u.columns))
        .unwrap_or_default();
    Statement {
        sql: format!(
            "ALTER TABLE {} ADD CONSTRAINT {} UNIQUE ({columns})",
            ident(table),
            ident(name)
        ),
        does: "add unique rule",
    }
}

fn add_foreign_key(table: &str, name: &str, def: &Table) -> Statement {
    let (columns, target, on_delete) = def.foreign_keys.get(name).map_or_else(
        || (String::new(), String::new(), "RESTRICT"),
        |key| (idents(&key.columns), ident(&key.table), key.on_delete.sql()),
    );
    Statement {
        sql: format!(
            "ALTER TABLE {} ADD CONSTRAINT {} FOREIGN KEY ({columns}) REFERENCES {target} (\"id\") ON DELETE {on_delete}",
            ident(table),
            ident(name)
        ),
        does: "add reference",
    }
}

fn create_index(table: &str, name: &str, def: &Table) -> Statement {
    let index = def.indexes.get(name);
    let columns = index.map(|i| idents(&i.columns)).unwrap_or_default();
    let partial = index
        .and_then(|i| i.predicate.as_ref())
        .map(|predicate| format!(" WHERE {predicate}"))
        .unwrap_or_default();
    Statement {
        sql: format!(
            "CREATE INDEX {} ON {} ({columns}){partial}",
            ident(name),
            ident(table)
        ),
        does: "create index",
    }
}

fn create_table(name: &str, table: &Table) -> String {
    let mut lines: Vec<String> = table
        .ordered()
        .into_iter()
        .map(|(column, def)| {
            let null = if def.nullable { "" } else { " NOT NULL" };
            format!("    {} {}{null}", ident(column), def.sql_type)
        })
        .collect();
    lines.push(format!(
        "    CONSTRAINT {} PRIMARY KEY ({})",
        ident(&table.primary_key.name),
        idents(&table.primary_key.columns)
    ));
    format!("CREATE TABLE {} (\n{}\n)", ident(name), lines.join(",\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{Index, Key, Store};

    fn schema(columns: &[(&str, &str, bool, bool)]) -> Schema {
        let mut table = Table {
            holds: Holds::Entity("Todo".into()),
            columns: BTreeMap::new(),
            primary_key: Key {
                name: "todo_pkey".into(),
                columns: vec!["id".into()],
            },
            unique: BTreeMap::new(),
            foreign_keys: BTreeMap::new(),
            indexes: BTreeMap::new(),
        };
        for (position, (name, ty, nullable, retired)) in (0u32..).zip(columns) {
            table.columns.insert(
                name.to_string(),
                Column {
                    position,
                    field: Some(name.to_string()),
                    sql_type: ty.to_string(),
                    nullable: *nullable,
                    retired: *retired,
                },
            );
        }
        let mut store = Store {
            driver: "postgres".into(),
            tables: BTreeMap::new(),
        };
        store.tables.insert("todo".into(), table);
        let mut out = Schema::default();
        out.stores.insert("main".into(), store);
        out
    }

    #[test]
    fn a_new_table_is_created_with_its_key() {
        let after = schema(&[
            ("id", "uuid", false, false),
            ("due_at", "timestamptz(3)", true, false),
        ]);
        let sql = migration_sql(&Schema::default(), &after, &|_| None).expect("sql");
        assert_eq!(
            sql.to_document("x"),
            "-- x: written by `jadpo migrate new` from the snapshots; never edit it by hand.\n\n-- store: main\nCREATE TABLE \"todo\" (\n    \"id\" uuid NOT NULL,\n    \"due_at\" timestamptz(3),\n    CONSTRAINT \"todo_pkey\" PRIMARY KEY (\"id\")\n);\n"
        );
        assert_eq!(sections(&sql.to_document("x"))[0].store, "main");
    }

    #[test]
    fn a_required_field_waits_for_its_backfill() {
        let before = schema(&[("id", "uuid", false, false)]);
        let after = schema(&[
            ("id", "uuid", false, false),
            ("rank", "integer", false, false),
        ]);
        let needed = decisions_needed(&before, &after);
        assert_eq!(needed.len(), 1);
        assert_eq!(needed[0].key(), "Todo.rank");
        let waiting = migration_sql(&before, &after, &|_| None).expect("sql");
        assert!(waiting.summary().contains(&"decision to make"));
        let done =
            migration_sql(&before, &after, &|_| Some(Decided::Backfill("3".into()))).expect("sql");
        let text = done.to_document("x");
        assert!(text.contains("UPDATE \"todo\" SET \"rank\" = 3 WHERE \"rank\" IS NULL;"));
        assert!(text.contains("SET NOT NULL;"));
    }

    #[test]
    fn a_removed_field_is_retired_and_narrowing_is_refused() {
        let before = schema(&[("id", "uuid", false, false), ("old", "text", false, false)]);
        let source = schema(&[("id", "uuid", false, false)]);
        let next = next_snapshot(&before, &source);
        let old = &next.stores["main"].tables["todo"].columns["old"];
        assert!(old.retired && old.nullable);
        assert_eq!(decisions_needed(&before, &next)[0].kind, NeededKind::Retire);
        let narrower = schema(&[
            ("id", "uuid", false, false),
            ("old", "smallint", false, false),
        ]);
        let wider = schema(&[
            ("id", "uuid", false, false),
            ("old", "integer", false, false),
        ]);
        assert!(migration_sql(&wider, &narrower, &|_| None).is_err());
        assert!(migration_sql(&narrower, &wider, &|_| None).is_ok());
        let mut indexed = wider.clone();
        if let Some(table) = indexed
            .stores
            .get_mut("main")
            .and_then(|s| s.tables.get_mut("todo"))
        {
            table.indexes.insert(
                "todo_old_idx".into(),
                Index {
                    columns: vec!["old".into()],
                    reason: "index Todo.by_old".into(),
                    predicate: None,
                },
            );
        }
        let sql = migration_sql(&wider, &indexed, &|_| None).expect("sql");
        assert_eq!(sql.summary(), vec!["create index"]);
    }
}
