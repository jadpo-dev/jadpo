//! A migration from one schema to the next (area 5, "Migrations"): which
//! changes need a person's decision, the snapshot the migration records,
//! and the SQL that gets there, for every store. Pure functions of the two
//! schemas and the decisions, so `jadpo migrate new` writes exactly what
//! `jadpo check` expects to find.
//!
//! A migration's `postgres.sql` holds one section per store, each opened
//! by a `-- store: <name>` line; `migrate apply` and the test database run
//! a store's section in one transaction, and record its digest.

use std::collections::{BTreeMap, BTreeSet};

use jadpo_std::Num;

use jadpo_sql::table;

use crate::schema::{Allows, Change, Column, Holds, Schema, Table, changes};

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
    /// A field whose type allows less than it did (a variant gone, a
    /// shorter `max_length`, a narrower range): stored rows it no longer
    /// allows need a value it does.
    Refit,
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
    /// A computed backfill (area 5, "Computed backfills"; chunk 5.6b):
    /// the SQL expression each existing row gets, reading the row's own
    /// columns as `"table"."column"`, filled in batches.
    Computed(String),
    /// A computed backfill with an operation that has no SQL form (chunk
    /// 5.6b2): `jadpo migrate apply` reads each batch's `reads` columns,
    /// works the values out in Jadpo with the code whose digest is `code`,
    /// and writes them back.
    InJadpo {
        reads: Vec<String>,
        code: String,
    },
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
            Some(was) if !now.retired && narrowed(was, now) => NeededKind::Refit,
            _ => continue,
        };
        let needed = Needed {
            store: store.clone(),
            table: table.clone(),
            column: column.clone(),
            entity: entity.clone(),
            field: now.field.clone().unwrap_or_else(|| column.clone()),
            kind,
        };
        // A field stored in two columns (an amount of money, area 19) is
        // one decision.
        let decided = out.iter().any(|other: &Needed| {
            other.store == needed.store
                && other.table == needed.table
                && other.key() == needed.key()
                && other.kind == needed.kind
        });
        if !decided {
            out.push(needed);
        }
    }
    out
}

/// Whether column `now` refuses a value `was` allowed; never when `was`
/// comes from a snapshot that didn't record what it allows.
pub fn narrowed(was: &Column, now: &Column) -> bool {
    match (&was.allows, &now.allows) {
        (Some(was), Some(now)) => narrows(was, now),
        _ => false,
    }
}

/// Whether `now` refuses a value `was` allowed: a variant gone, or a
/// bound or length tighter or new. A new pattern is refused by
/// [`migration_sql`] instead, since no SQL finds the values it refuses.
pub fn narrows(was: &Allows, now: &Allows) -> bool {
    let tighter = |old: &Option<String>, new: &Option<String>, below: bool| match (old, new) {
        (_, None) => false,
        (None, Some(_)) => true,
        (Some(old), Some(new)) => match (Num::parse(old), Num::parse(new)) {
            (Some(old), Some(new)) => {
                let order = new.compare(old);
                if below { order.is_gt() } else { order.is_lt() }
            }
            _ => old != new,
        },
    };
    let shorter = |old: Option<u64>, new: Option<u64>, below: bool| match (old, new) {
        (_, None) => false,
        (None, Some(_)) => true,
        (Some(old), Some(new)) => {
            if below {
                new > old
            } else {
                new < old
            }
        }
    };
    (!was.values.is_empty() && was.values.iter().any(|v| !now.values.contains(v)))
        || (was.values.is_empty() && !now.values.is_empty())
        || tighter(&was.min, &now.min, true)
        || tighter(&was.max, &now.max, false)
        || shorter(was.min_length, now.min_length, true)
        || shorter(was.max_length, now.max_length, false)
}

/// The conditions under which a stored value of `column`, which `was`
/// held, breaks what `now` allows: none unless `now` allows less. A
/// pattern is left out, since PostgreSQL's regular expressions are not
/// Jadpo's.
fn refit(was: &Column, now: &Column, column: &str) -> Vec<String> {
    match &now.allows {
        Some(allows) if narrowed(was, now) => table::breaks(column, &allowed(allows)),
        _ => Vec::new(),
    }
}

/// What `allows` allows, as the SQL renderer reads it.
fn allowed(allows: &Allows) -> table::Allowed<'_> {
    table::Allowed {
        values: &allows.values,
        min: allows.min.as_deref(),
        max: allows.max.as_deref(),
        min_length: allows.min_length,
        max_length: allows.max_length,
    }
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
    // Each decision worked out once: a computed backfill's is a body
    // checked and written as SQL.
    let made: Vec<Option<Decided>> = needed.iter().map(decided).collect();
    let decided = |need: &Needed| -> Option<Decided> {
        needed
            .iter()
            .position(|n| n == need)
            .and_then(|at| made[at].clone())
    };
    // The stores whose section fills a backfill in batches: there every
    // column a backfill fills is made required in the section's last
    // steps, once every batch is filled (chunk 5.8b).
    let batched: BTreeSet<&str> = needed
        .iter()
        .zip(&made)
        .filter(|(need, made)| {
            need.kind == NeededKind::Backfill
                && matches!(made, Some(Decided::Computed(_) | Decided::InJadpo { .. }))
        })
        .map(|(need, _)| need.store.as_str())
        .collect();
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
                    // What a quantity's column holds (area 19, chunk 5.5w).
                    let mut commented: Vec<(&String, &Column)> = new
                        .columns
                        .iter()
                        .filter(|(_, column)| column.comment.is_some())
                        .collect();
                    commented.sort_by_key(|(_, column)| column.position);
                    for (name, column) in commented {
                        plan.create.push(Statement {
                            sql: table::comment_on_column(table, name, column.comment.as_deref()),
                            does: "comment on column",
                        });
                    }
                    for name in new.unique.keys() {
                        plan.constraints.push(add_unique(table, name, new));
                    }
                    for name in new.checks.keys() {
                        plan.constraints.push(add_check(table, name, new));
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
                plan.columns.push(Statement {
                    sql: table::add_column(table, column, &col.sql_type),
                    does: "add column",
                });
                if let Some(comment) = &col.comment {
                    plan.columns.push(Statement {
                        sql: table::comment_on_column(table, column, Some(comment)),
                        does: "comment on column",
                    });
                }
                if let Some(need) = need {
                    let key = new
                        .map(|t| t.primary_key.columns.clone())
                        .unwrap_or_default();
                    let filled = backfill(
                        table,
                        &key,
                        (column, &col.sql_type),
                        &[],
                        need,
                        &decided,
                        batched.contains(store),
                    );
                    plan.fill(table, filled);
                }
            }
            Change::AlterColumn { table, column, .. } => {
                let (Some(was), Some(now)) = (
                    old.and_then(|t| t.columns.get(column)),
                    new.and_then(|t| t.columns.get(column)),
                ) else {
                    continue;
                };
                if was.comment != now.comment {
                    plan.columns.push(Statement {
                        sql: table::comment_on_column(table, column, now.comment.as_deref()),
                        does: "comment on column",
                    });
                }
                if was.sql_type != now.sql_type {
                    if widens(&was.sql_type, &now.sql_type) {
                        plan.columns.push(Statement {
                            sql: table::change_type(table, column, &now.sql_type),
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
                let new_pattern = match (&was.allows, &now.allows) {
                    (Some(was), Some(now)) => {
                        now.patterns.iter().any(|p| !was.patterns.contains(p))
                    }
                    _ => false,
                };
                if !now.retired && new_pattern {
                    refusals.push(Refusal {
                        change: format!("column `{table}.{column}` takes a new `pattern`"),
                        reason: "Stored values a new pattern refuses would need a value it allows, and PostgreSQL's regular expressions aren't Jadpo's, so this compiler can't find them yet.".into(),
                    });
                    continue;
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
                    Some(need) => {
                        let key = new
                            .map(|t| t.primary_key.columns.clone())
                            .unwrap_or_default();
                        let filled = backfill(
                            table,
                            &key,
                            (column, &now.sql_type),
                            &refit(was, now, column),
                            need,
                            &decided,
                            batched.contains(store),
                        );
                        plan.fill(table, filled);
                    }
                    None if !was.nullable && now.nullable => plan.columns.push(Statement {
                        sql: table::drop_not_null(table, column),
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
            Change::DropForeignKey { table, name, .. }
            | Change::DropUnique { table, name, .. }
            | Change::DropCheck { table, name, .. } => {
                plan.drop.push(Statement {
                    sql: table::drop_constraint(table, name),
                    does: "drop constraint",
                });
            }
            Change::DropIndex { name, .. } => plan.drop.push(Statement {
                sql: table::drop_index(name),
                does: "drop index",
            }),
            Change::AddUnique { table, name, .. } => {
                if let Some(new) = new {
                    plan.constraints.push(add_unique(table, name, new));
                }
            }
            Change::AddCheck { table, name, .. } => {
                if let Some(new) = new {
                    plan.constraints.push(add_check(table, name, new));
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
    for (store, mut plan) in plans {
        // An extension the store's columns need now and didn't before,
        // created first (area 21, chunk 5.5v1).
        let had = extensions(before, &store);
        let created: Vec<Statement> = extensions(after, &store)
            .into_iter()
            .filter(|extension| !had.contains(extension))
            .map(|extension| Statement {
                sql: table::create_extension(extension),
                does: "create extension",
            })
            .collect();
        let last = plan.last_steps();
        let statements: Vec<Statement> = created
            .into_iter()
            .chain(plan.drop)
            .chain(plan.create)
            .chain(plan.columns)
            .chain(plan.batches)
            .chain(last)
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

/// The PostgreSQL extensions `store`'s columns need in `schema`: pgvector's
/// `vector` for a vector column (area 21, chunk 5.5v1), PostGIS's
/// `postgis` for a `geography` column, which every geometry index is on
/// (area 20, chunk 5.5u4). An application with none needs none, so its
/// migrations never create one.
pub fn extensions(schema: &Schema, store: &str) -> BTreeSet<&'static str> {
    let mut out = BTreeSet::new();
    let Some(store) = schema.stores.get(store) else {
        return out;
    };
    for table in store.tables.values() {
        for column in table.columns.values() {
            match column.sql_type.split('(').next() {
                Some("vector") => {
                    out.insert("vector");
                }
                Some("geography") => {
                    out.insert("postgis");
                }
                _ => {}
            }
        }
    }
    out
}

/// One store's statements, grouped so each can depend on the ones before:
/// constraints are dropped first, tables made before columns change, and
/// references added once every table they name exists. A section that
/// fills backfills in batches fills every one of them before its last
/// steps make any column required (chunk 5.8b).
#[derive(Default)]
struct Plan {
    drop: Vec<Statement>,
    create: Vec<Statement>,
    columns: Vec<Statement>,
    /// Each computed backfill's batches.
    batches: Vec<Statement>,
    /// The columns a backfill fills that become required only in the
    /// last steps, by table in the order first filled: each with what
    /// fills the rows written meanwhile.
    required: Vec<(String, Vec<(String, Statement)>)>,
    constraints: Vec<Statement>,
    references: Vec<Statement>,
    indexes: Vec<Statement>,
}

impl Plan {
    fn fill(&mut self, table: &str, filled: Filled) {
        self.columns.extend(filled.now);
        self.batches.extend(filled.batches);
        if let Some((column, meanwhile)) = filled.required {
            match self.required.iter_mut().find(|(t, _)| t == table) {
                Some((_, columns)) => columns.push((column, meanwhile)),
                None => self
                    .required
                    .push((table.to_string(), vec![(column, meanwhile)])),
            }
        }
    }

    /// The last steps of a section that fills backfills in batches (area
    /// 5, "Computed backfills"; chunk 5.8b), once every batch is filled.
    /// First, in one transaction, each table is locked against writes and
    /// never reads (`SHARE ROW EXCLUSIVE`), the rows servers still on the
    /// old build wrote meanwhile are filled, and a check that every row
    /// has a value is added `NOT VALID`, reading no row, so every row
    /// written from then on is held to it. Then, in a step of its own,
    /// each check is proved, reading every row under `SHARE UPDATE
    /// EXCLUSIVE`, which lets reads and writes go on. Last, each column is
    /// made required, which PostgreSQL does without reading a row since
    /// the proved check already says so, and the check goes; the step
    /// that records the migration.
    fn last_steps(&mut self) -> Vec<Statement> {
        let required = std::mem::take(&mut self.required);
        let mut fill = Vec::new();
        let mut checks = Vec::new();
        let mut proofs = Vec::new();
        let mut columns = Vec::new();
        let mut drops = Vec::new();
        for (table, filled) in &required {
            fill.push(Statement {
                sql: table::lock_against_writes(table),
                does: "lock against writes",
            });
            for (column, meanwhile) in filled {
                fill.push(meanwhile.clone());
                let name = crate::schema::object_name(&[table.as_str(), column.as_str()], "filled");
                checks.push(Statement {
                    sql: table::add_filled_check(table, &name, column),
                    does: "add check",
                });
                proofs.push(Statement {
                    sql: table::validate_constraint(table, &name),
                    does: "check every row has a value",
                });
                columns.push(Statement {
                    sql: table::set_not_null(table, column),
                    does: "require a value",
                });
                drops.push(Statement {
                    sql: table::drop_constraint(table, &name),
                    does: "drop constraint",
                });
            }
        }
        let opening = |statements: &mut Vec<Statement>, what: &str| {
            if let Some(first) = statements.first_mut() {
                first.sql = table::in_a_step(what, &first.sql);
            }
        };
        opening(
            &mut proofs,
            "check every row has a value, locking out neither reads nor writes",
        );
        opening(
            &mut columns,
            "require a value, which the proved check lets PostgreSQL do without reading a row",
        );
        fill.into_iter()
            .chain(checks)
            .chain(proofs)
            .chain(columns)
            .chain(drops)
            .collect()
    }
}

/// What a decision about one column runs, and where.
#[derive(Default)]
struct Filled {
    /// With the section's other column changes.
    now: Vec<Statement>,
    /// A computed backfill's batches.
    batches: Vec<Statement>,
    /// In a section that fills backfills in batches, the column made
    /// required in its last steps, and what fills the rows written
    /// meanwhile there, with the table locked against writes.
    required: Option<(String, Statement)>,
}

#[allow(clippy::too_many_arguments)]
fn backfill(
    table: &str,
    key: &[String],
    (column, sql_type): (&str, &str),
    broken: &[String],
    need: &Needed,
    decided: &dyn Fn(&Needed) -> Option<Decided>,
    batched: bool,
) -> Filled {
    match (need.kind, decided(need)) {
        // The same steps, each batch read, worked out in Jadpo and written
        // back by `jadpo migrate apply`, and the rows written meanwhile
        // too (chunk 5.6b2).
        (NeededKind::Backfill, Some(Decided::InJadpo { reads, code })) if key.len() == 1 => {
            Filled {
                batches: vec![Statement {
                    sql: table::backfill_in_jadpo(
                        &need.key(),
                        table,
                        &key[0],
                        column,
                        sql_type,
                        &reads,
                        &code,
                    ),
                    does: "fill existing rows in batches in Jadpo",
                }],
                required: Some((
                    column.to_string(),
                    Statement {
                        sql: table::fill_in_jadpo(
                            &need.key(),
                            table,
                            &key[0],
                            column,
                            sql_type,
                            &reads,
                        ),
                        does: "fill rows written meanwhile in Jadpo",
                    },
                )),
                ..Filled::default()
            }
        }
        // Filled in batches, each its own transaction, then the rows
        // written meanwhile filled with the table locked against writes,
        // and the column made required, in the migration's last steps
        // (chunks 5.6b and 5.8b). An entity's key is its one `id`.
        (NeededKind::Backfill, Some(Decided::Computed(value))) if key.len() == 1 => Filled {
            batches: vec![Statement {
                sql: table::backfill_in_batches(&need.key(), table, &key[0], column, &value),
                does: "fill existing rows in batches",
            }],
            required: Some((
                column.to_string(),
                Statement {
                    sql: table::fill(table, column, &value, &[table::is_null(column)]),
                    does: "fill rows written meanwhile",
                },
            )),
            ..Filled::default()
        },
        (NeededKind::Backfill, Some(Decided::Backfill(value))) => {
            let mut empty = vec![table::is_null(column)];
            empty.extend(broken.iter().cloned());
            let now = Statement {
                sql: table::fill(table, column, &value, &empty),
                does: "fill existing rows",
            };
            // Beside backfills filled in batches, it too is required only
            // once they are filled, its rows written meanwhile filled
            // again in the last step (chunk 5.8b).
            if batched {
                return Filled {
                    now: vec![now],
                    required: Some((
                        column.to_string(),
                        Statement {
                            sql: table::fill(table, column, &value, &empty),
                            does: "fill rows written meanwhile",
                        },
                    )),
                    ..Filled::default()
                };
            }
            Filled {
                now: vec![
                    now,
                    Statement {
                        sql: table::set_not_null(table, column),
                        does: "require a value",
                    },
                ],
                ..Filled::default()
            }
        }
        (NeededKind::Refit, Some(Decided::Backfill(value))) => Filled {
            now: vec![Statement {
                sql: table::fill(table, column, &value, broken),
                does: "replace values no longer allowed",
            }],
            ..Filled::default()
        },
        (NeededKind::Retire, Some(Decided::Retired)) => Filled {
            now: vec![Statement {
                sql: table::drop_not_null(table, column),
                does: "retire column",
            }],
            ..Filled::default()
        },
        _ => Filled {
            now: vec![Statement {
                sql: table::comment(&format!(
                    "waiting for the decision about {} in changes.jadpo",
                    need.key()
                )),
                does: "decision to make",
            }],
            ..Filled::default()
        },
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
        .map(|u| u.columns.as_slice())
        .unwrap_or_default();
    Statement {
        sql: table::add_unique(table, name, columns),
        does: "add unique rule",
    }
}

fn add_check(table: &str, name: &str, def: &Table) -> Statement {
    let sql = match def.checks.get(name) {
        Some(check) if check.geography => {
            table::add_geography_check(table, name, &check.column, &check.values)
        }
        Some(check) => table::add_check(table, name, &check.column, &check.values),
        None => table::add_check(table, name, "", &[]),
    };
    Statement {
        sql,
        does: "add check",
    }
}

fn add_foreign_key(table: &str, name: &str, def: &Table) -> Statement {
    let sql = match def.foreign_keys.get(name) {
        Some(key) => {
            table::add_foreign_key(table, name, &key.columns, &key.table, key.on_delete.sql())
        }
        None => table::add_foreign_key(table, name, &[], "", "RESTRICT"),
    };
    Statement {
        sql,
        does: "add reference",
    }
}

fn create_index(table: &str, name: &str, def: &Table) -> Statement {
    let index = def.indexes.get(name);
    // An index of an operation's result (chunk 6.3b).
    if let Some(found) = index
        && let Some(expression) = &found.expression
    {
        return Statement {
            sql: table::create_expression_index(
                table,
                name,
                expression,
                found.predicate.as_deref(),
            ),
            does: "create index",
        };
    }
    let columns: Vec<table::IndexColumn<'_>> = index
        .map(|i| {
            i.columns
                .iter()
                .map(|column| table::IndexColumn {
                    name: column,
                    collate: i.collate.contains(column),
                })
                .collect()
        })
        .unwrap_or_default();
    let predicate = index.and_then(|i| i.predicate.as_deref());
    let method = index
        .and_then(|i| i.method.as_ref())
        .map(|m| table::IndexMethod {
            name: &m.name,
            operator_class: &m.operator_class,
            cast: m.cast.as_deref(),
            with: &m.with,
        });
    Statement {
        sql: table::create_index(table, name, &columns, predicate, method),
        does: "create index",
    }
}

fn create_table(name: &str, def: &Table) -> String {
    let columns: Vec<table::ColumnDef<'_>> = def
        .ordered()
        .into_iter()
        .map(|(column, def)| table::ColumnDef {
            name: column,
            sql_type: &def.sql_type,
            nullable: def.nullable,
        })
        .collect();
    table::create_table(
        name,
        &columns,
        &def.primary_key.name,
        &def.primary_key.columns,
    )
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
            checks: BTreeMap::new(),
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
                    allows: None,
                    comment: None,
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
    fn a_computed_backfill_is_filled_in_batches_then_required() {
        let before = schema(&[
            ("id", "uuid", false, false),
            ("flag", "boolean", false, false),
        ]);
        let after = schema(&[
            ("id", "uuid", false, false),
            ("flag", "boolean", false, false),
            ("rank", "bigint", false, false),
        ]);
        let value = "(CASE WHEN \"todo\".\"flag\" THEN 1 ELSE 2 END)";
        let sql = migration_sql(&before, &after, &|_| Some(Decided::Computed(value.into())))
            .expect("sql");
        let document = sql.to_document("x");
        let section = &sections(&document)[0];
        let steps = jadpo_std::Migration {
            name: "x".into(),
            digest: section_digest(&section.sql),
            sql: section.sql.clone(),
        }
        .steps();
        assert_eq!(steps.len(), 5, "{document}");
        assert_eq!(
            steps[0],
            jadpo_std::Step::Once("ALTER TABLE \"todo\" ADD COLUMN \"rank\" bigint;".into())
        );
        let jadpo_std::Step::Batches(batches) = &steps[1] else {
            panic!("{document}");
        };
        assert_eq!(batches.field, "Todo.rank");
        assert_eq!(batches.how, "by UPDATE");
        assert_eq!(
            batches.count,
            "SELECT count(*) FROM \"todo\" WHERE \"rank\" IS NULL"
        );
        assert!(
            batches
                .batch
                .contains(&format!("SET \"rank\" = {value} FROM \"batch\""))
                && batches
                    .batch
                    .contains("WHERE \"id\" > $1 ORDER BY \"id\" LIMIT 1000"),
            "{}",
            batches.batch
        );
        // The last steps (chunk 5.8b): the rows written meanwhile filled
        // with writes locked out and a check added that reads no row; the
        // check proved, locking out neither reads nor writes; the column
        // required, which the proved check lets PostgreSQL do without
        // reading a row, and the check dropped.
        assert_eq!(
            steps[2],
            jadpo_std::Step::Once(format!(
                "LOCK TABLE \"todo\" IN SHARE ROW EXCLUSIVE MODE;\nUPDATE \"todo\" SET \"rank\" = {value} WHERE \"rank\" IS NULL;\nALTER TABLE \"todo\" ADD CONSTRAINT \"todo_rank_filled\" CHECK (\"rank\" IS NOT NULL) NOT VALID;"
            ))
        );
        assert_eq!(
            steps[3],
            jadpo_std::Step::Once(
                "ALTER TABLE \"todo\" VALIDATE CONSTRAINT \"todo_rank_filled\";".into()
            )
        );
        assert_eq!(
            steps[4],
            jadpo_std::Step::Once(
                "ALTER TABLE \"todo\" ALTER COLUMN \"rank\" SET NOT NULL;\nALTER TABLE \"todo\" DROP CONSTRAINT \"todo_rank_filled\";".into()
            )
        );
    }

    #[test]
    fn a_literal_backfill_beside_one_in_batches_is_required_in_the_last_steps_too() {
        let before = schema(&[
            ("id", "uuid", false, false),
            ("flag", "boolean", false, false),
        ]);
        let after = schema(&[
            ("id", "uuid", false, false),
            ("flag", "boolean", false, false),
            ("rank", "bigint", false, false),
            ("size", "bigint", false, false),
        ]);
        let decided = |need: &Needed| {
            Some(if need.column == "rank" {
                Decided::Computed("(\"todo\".\"flag\")::bigint".into())
            } else {
                Decided::Backfill("3".into())
            })
        };
        let document = migration_sql(&before, &after, &decided)
            .expect("sql")
            .to_document("x");
        let filled = document
            .find("UPDATE \"todo\" SET \"size\" = 3 WHERE \"size\" IS NULL;\n-- backfill")
            .expect(&document);
        let lock = document.find("LOCK TABLE").expect(&document);
        assert!(filled < lock, "{document}");
        assert!(
            document[lock..].contains("UPDATE \"todo\" SET \"size\" = 3 WHERE \"size\" IS NULL;"),
            "{document}"
        );
        assert!(
            document
                .find("ALTER COLUMN \"size\" SET NOT NULL")
                .expect(&document)
                > lock,
            "{document}"
        );
        // Alone, it is filled and required in one transaction, as since
        // chunk 3.1.
        let alone = migration_sql(&before, &after, &|_| Some(Decided::Backfill("3".into())))
            .expect("sql")
            .to_document("x");
        assert!(!alone.contains("LOCK TABLE"), "{alone}");
        assert!(
            alone.contains("UPDATE \"todo\" SET \"size\" = 3 WHERE \"size\" IS NULL;\nALTER TABLE \"todo\" ALTER COLUMN \"size\" SET NOT NULL;"),
            "{alone}"
        );
    }

    #[test]
    fn a_backfill_in_jadpo_reads_and_writes_each_batch_and_its_last_step() {
        let before = schema(&[
            ("id", "uuid", false, false),
            ("flag", "boolean", false, false),
        ]);
        let after = schema(&[
            ("id", "uuid", false, false),
            ("flag", "boolean", false, false),
            ("rank", "bigint", false, false),
        ]);
        let sql = migration_sql(&before, &after, &|_| {
            Some(Decided::InJadpo {
                reads: vec!["flag".into()],
                code: "0123456789abcdef".into(),
            })
        })
        .expect("sql");
        assert!(
            sql.summary()
                .contains(&"fill existing rows in batches in Jadpo")
        );
        let document = sql.to_document("x");
        let section = &sections(&document)[0];
        let steps = jadpo_std::Migration {
            name: "x".into(),
            digest: section_digest(&section.sql),
            sql: section.sql.clone(),
        }
        .steps();
        assert_eq!(steps.len(), 5, "{document}");
        let jadpo_std::Step::Batches(batches) = &steps[1] else {
            panic!("{document}");
        };
        assert_eq!(batches.how, jadpo_std::IN_JADPO);
        assert_eq!(
            batches.batch,
            "SELECT \"id\", \"flag\" FROM \"todo\" WHERE \"id\" > $1 AND \"rank\" IS NULL ORDER BY \"id\" LIMIT 1000 FOR NO KEY UPDATE"
        );
        let write = "UPDATE \"todo\" SET \"rank\" = \"jadpo_value\".\"value\" FROM unnest($1::uuid[], $2::bigint[]) AS \"jadpo_value\"(\"key\", \"value\") WHERE \"todo\".\"id\" = \"jadpo_value\".\"key\" AND \"todo\".\"rank\" IS NULL";
        assert_eq!(batches.write.as_deref(), Some(write));
        assert!(
            document.contains("its code's digest 0123456789abcdef\n"),
            "{document}"
        );
        assert_eq!(
            steps[2],
            jadpo_std::Step::Mixed(vec![
                jadpo_std::migration::Part::Sql("LOCK TABLE \"todo\" IN SHARE ROW EXCLUSIVE MODE;".into()),
                jadpo_std::migration::Part::Fill(jadpo_std::migration::Fill {
                    field: "Todo.rank".into(),
                    read: "SELECT \"id\", \"flag\" FROM \"todo\" WHERE \"rank\" IS NULL ORDER BY \"id\"".into(),
                    write: write.into(),
                }),
                jadpo_std::migration::Part::Sql(
                    "ALTER TABLE \"todo\" ADD CONSTRAINT \"todo_rank_filled\" CHECK (\"rank\" IS NOT NULL) NOT VALID;".into()
                ),
            ])
        );
        assert_eq!(
            steps[4],
            jadpo_std::Step::Once(
                "ALTER TABLE \"todo\" ALTER COLUMN \"rank\" SET NOT NULL;\nALTER TABLE \"todo\" DROP CONSTRAINT \"todo_rank_filled\";".into()
            )
        );
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
                    expression: None,
                    columns: vec!["old".into()],
                    reason: "index Todo.by_old".into(),
                    predicate: None,
                    collate: Vec::new(),
                    method: None,
                },
            );
        }
        let sql = migration_sql(&wider, &indexed, &|_| None).expect("sql");
        assert_eq!(sql.summary(), vec!["create index"]);
    }
}
