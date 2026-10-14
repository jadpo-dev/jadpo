//! Migrations as `jadpo check` sees them (area 5, "Migrations"; area 18):
//! each folder under `database/migrations/` holds the whole schema at that
//! point (`snapshot.json`), the SQL that got there (`postgres.sql`), and
//! `changes.jadpo` when a change needed a decision. `jadpo check` fails
//! while the entities and the latest snapshot disagree, naming what changed
//! and the command to run; a migration `jadpo migrate new` started but
//! couldn't finish, because a change needs a decision, waits for that
//! decision in its `changes.jadpo`.
//!
//! The decisions a change needs are worked out here, from the schemas
//! alone ([`decisions_needed`]), so `jadpo check` and `jadpo migrate new`
//! agree on them.

use std::collections::BTreeMap;
use std::sync::Arc;

use jadpo_db::{Db, File, Layout, Project};
use jadpo_diag::{Diagnostic, Span, rule};
use jadpo_graph::schema::{self, Change, Owner, Schema};
use jadpo_std::{Builtin, FieldChange, Num};
use jadpo_syntax::ast::{self, AstNode, Decl, Expr};

use super::{DataModel, data_model};
use crate::lookups::Lookup;
use crate::tree::{self, file_span, node_span};
use crate::types::{Const, Def, Ty, TypeView};

/// Where migrations live.
pub const FOLDER: &str = "database/migrations";

/// One migration folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Migration {
    /// `20261020T1400_add_priority`.
    pub name: String,
    /// Its snapshot: absent while the migration is unfinished, or why it
    /// can't be read.
    pub snapshot: Option<Result<Schema, String>>,
    /// Its `postgres.sql`, or why it can't be read.
    pub sql: Option<Result<String, String>>,
    /// Its `changes.jadpo`, when it has one.
    pub changes: Option<String>,
}

impl Migration {
    pub fn snapshot_path(&self) -> String {
        format!("{FOLDER}/{}/snapshot.json", self.name)
    }

    pub fn sql_path(&self) -> String {
        format!("{FOLDER}/{}/postgres.sql", self.name)
    }

    pub fn changes_path(&self) -> String {
        format!("{FOLDER}/{}/changes.jadpo", self.name)
    }

    /// Finished: `jadpo migrate new` wrote its snapshot.
    pub fn finished(&self) -> bool {
        self.snapshot.is_some()
    }
}

/// Every migration, in the order they apply.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Migrations {
    pub list: Vec<Migration>,
}

impl Migrations {
    /// The latest finished migration's schema; empty before the first.
    pub fn latest_schema(&self) -> Schema {
        self.list
            .iter()
            .rev()
            .find_map(|m| m.snapshot.as_ref().and_then(|s| s.as_ref().ok()))
            .cloned()
            .unwrap_or_default()
    }

    /// The latest folder, when `jadpo migrate new` started it and couldn't
    /// finish it.
    pub fn unfinished(&self) -> Option<&Migration> {
        self.list.last().filter(|m| !m.finished())
    }
}

/// Whether a folder name is a timestamp and a purpose:
/// `20261020T1400_add_priority`.
pub fn valid_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    bytes.len() > 15
        && bytes[..8].iter().all(u8::is_ascii_digit)
        && bytes[8] == b'T'
        && bytes[9..13].iter().all(u8::is_ascii_digit)
        && bytes[13] == b'_'
        && crate::case::is_lower_snake(&name[14..])
}

/// The project's migrations, from its records and `changes.jadpo` files.
#[salsa::tracked(returns(ref))]
pub fn migrations(db: &dyn Db, project: Project) -> Arc<Migrations> {
    let mut folders: BTreeMap<String, Migration> = BTreeMap::new();
    let mut folder = |name: &str| {
        folders
            .entry(name.to_string())
            .or_insert_with(|| Migration {
                name: name.to_string(),
                snapshot: None,
                sql: None,
                changes: None,
            })
            .name
            .clone()
    };
    let mut found: Vec<(String, String, Result<String, String>)> = Vec::new();
    for record in project.records(db) {
        if let ["database", "migrations", name, file] =
            record.path.split('/').collect::<Vec<_>>().as_slice()
        {
            found.push((folder(name), (*file).to_string(), record.text.clone()));
        }
    }
    for file in project.files(db) {
        if let ["database", "migrations", name, "changes.jadpo"] =
            file.path(db).split('/').collect::<Vec<_>>().as_slice()
        {
            let name = folder(name);
            found.push((name, "changes.jadpo".into(), Ok(file.path(db).clone())));
        }
    }
    for (name, file, text) in found {
        let Some(migration) = folders.get_mut(&name) else {
            continue;
        };
        match file.as_str() {
            "snapshot.json" => {
                migration.snapshot = Some(text.and_then(|text| Schema::read(&text)));
            }
            "postgres.sql" => migration.sql = Some(text),
            _ => migration.changes = text.ok(),
        }
    }
    Arc::new(Migrations {
        list: folders.into_values().collect(),
    })
}

/// The schema the source asks for, with the columns earlier migrations
/// retired carried on: they stay until a migration drops them.
pub fn expected(source: &Schema, latest: &Schema) -> Schema {
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
            for (column_name, column) in &table.columns {
                if column.retired && !now.columns.contains_key(column_name) {
                    now.columns.insert(column_name.clone(), column.clone());
                }
            }
        }
    }
    out
}

/// A decision a change needs before `jadpo migrate new` can write it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Needed {
    pub store: String,
    pub table: String,
    pub column: String,
    /// `Todo`, `priority`: the born names, as the decision is written.
    pub entity: String,
    pub field: String,
    pub kind: NeededKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NeededKind {
    /// A required column existing rows have no value for: a new one, or
    /// one that could be `none` until now.
    Backfill,
    /// A field gone from its entity: its column is retired.
    Retire,
}

impl Needed {
    /// `Todo.priority`, as `changes.jadpo` names it.
    pub fn key(&self) -> String {
        format!("{}.{}", self.entity, self.field)
    }
}

/// The decisions the changes from `before` to `after` need.
pub fn decisions_needed(before: &Schema, after: &Schema) -> Vec<Needed> {
    let mut out = Vec::new();
    for change in schema::changes(before, after) {
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
            }
            | Change::RemoveColumn {
                store,
                table,
                column,
            } => (store, table, column),
            _ => continue,
        };
        let old = before.stores.get(store).and_then(|s| s.tables.get(table));
        let new = after.stores.get(store).and_then(|s| s.tables.get(table));
        let (Some(old), Some(new)) = (old, new) else {
            continue;
        };
        let Owner::Entity(entity) = &new.owner else {
            continue;
        };
        let was = old.columns.get(column);
        let now = new.columns.get(column);
        let kind = match (&change, was, now) {
            (Change::AddColumn { .. }, None, Some(now)) if !now.nullable => NeededKind::Backfill,
            (Change::AlterColumn { .. }, Some(was), Some(now))
                if was.nullable
                    && !now.nullable
                    && !now.retired
                    && was.sql_type == now.sql_type =>
            {
                NeededKind::Backfill
            }
            (Change::RemoveColumn { .. }, Some(was), None) if !was.retired => NeededKind::Retire,
            _ => continue,
        };
        let field = now
            .or(was)
            .and_then(|c| c.field.clone())
            .unwrap_or_else(|| column.clone());
        out.push(Needed {
            store: store.clone(),
            table: table.clone(),
            column: column.clone(),
            entity: entity.clone(),
            field,
            kind,
        });
    }
    out
}

/// One decision written in a `changes.jadpo`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decision {
    /// `Todo.priority`.
    pub key: String,
    pub span: Span,
    pub value: DecisionValue,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DecisionValue {
    Change(FieldChange),
    /// `backfill:`'s value, with where it is written.
    Backfill(Const, Span),
    /// Anything else: what it is, for the error.
    Invalid(String, Span),
}

/// The decisions a `changes.jadpo` holds, and the span of its `migration`
/// keyword, where a missing decision is reported.
pub fn read_decisions(path: &str, source: &ast::SourceFile) -> (Vec<Decision>, Span) {
    let mut out = Vec::new();
    let mut at = file_span(path);
    for decl in source.decls() {
        let Decl::SingletonDecl(singleton) = decl else {
            continue;
        };
        if singleton.keyword().map(|t| t.text().to_string()).as_deref() != Some("migration") {
            continue;
        }
        if let Some(keyword) = singleton.keyword() {
            let range = keyword.text_range();
            at = Span::new(path, range.start().into(), range.end().into());
        }
        for item in singleton
            .member_list()
            .iter()
            .flat_map(|list| list.member_items())
        {
            let ast::MemberItem::Member(member) = item else {
                continue;
            };
            let Some(key) = member.member_key() else {
                continue;
            };
            let names: Vec<String> = key.names().filter_map(|n| tree::name_text(&n)).collect();
            let span = node_span(path, key.syntax());
            let value = if let Some(block) = member.block() {
                backfill(path, block, &span)
            } else {
                match member.value() {
                    Some(Expr::RecordExpr(record)) => match record.member_list() {
                        Some(block) => backfill(path, block, &span),
                        None => DecisionValue::Invalid("an empty block".into(), span.clone()),
                    },
                    Some(value) => {
                        let value_span = node_span(path, value.syntax());
                        match Const::from_expr(&value) {
                            Const::Path(p) if p.len() == 2 && p[0] == FieldChange::NAME => {
                                match FieldChange::from_name(&p[1]) {
                                    Some(change) => DecisionValue::Change(change),
                                    None => DecisionValue::Invalid(
                                        format!("`{}`", p.join(".")),
                                        value_span,
                                    ),
                                }
                            }
                            _ => DecisionValue::Invalid(
                                format!("`{}`", tree::written(value.syntax())),
                                value_span,
                            ),
                        }
                    }
                    None => DecisionValue::Invalid("nothing".into(), span.clone()),
                }
            };
            out.push(Decision {
                key: names.join("."),
                span,
                value,
            });
        }
    }
    (out, at)
}

fn backfill(path: &str, block: ast::MemberList, at: &Span) -> DecisionValue {
    let members = tree::members(Some(block));
    match members.as_slice() {
        [(name, member)] if tree::name_text(name).as_deref() == Some("backfill") => {
            match member.value() {
                Some(value) => DecisionValue::Backfill(
                    Const::from_expr(&value),
                    node_span(path, value.syntax()),
                ),
                None => DecisionValue::Invalid("`backfill:` without a value".into(), at.clone()),
            }
        }
        _ => DecisionValue::Invalid(
            "a block that isn't `{ backfill: <value> }`".into(),
            at.clone(),
        ),
    }
}

/// Why a backfill value doesn't fit its field, if it doesn't.
pub fn backfill_problem(lookup: &dyn TypeView, ty: &Ty, value: &Const) -> Option<String> {
    let structure = lookup.structure(ty);
    let rules: Vec<_> = lookup
        .rule_chain(ty)
        .into_iter()
        .map(|(_, r)| r.clone())
        .collect();
    match (&structure, value) {
        (Ty::Prim(prim), Const::Str(text)) if prim.representation() == Builtin::Text => {
            let length = text.chars().count() as u64;
            for rules in &rules {
                if rules.min_length.is_some_and(|min| length < min) {
                    return Some(format!("it is shorter than the field's `min_length`"));
                }
                if rules.max_length.is_some_and(|max| length > max) {
                    return Some(format!("it is longer than the field's `max_length`"));
                }
            }
            None
        }
        (Ty::Prim(Builtin::Uuid), Const::Str(text)) => (text.len() != 36
            || text.chars().filter(|c| *c == '-').count() != 4)
            .then(|| "it isn't a UUID".to_string()),
        (Ty::Prim(Builtin::Bool), Const::Bool(_)) => None,
        (Ty::Prim(prim @ (Builtin::Int | Builtin::Decimal)), Const::Number { value, .. }) => {
            if *prim == Builtin::Int && !value.fits_int() {
                return Some("an `Int` holds whole numbers".into());
            }
            for rules in &rules {
                if rules.min.is_some_and(|min| value.compare(min).is_lt()) {
                    return Some("it is below the field's `min`".into());
                }
                if rules.max.is_some_and(|max| value.compare(max).is_gt()) {
                    return Some("it is above the field's `max`".into());
                }
                if rules
                    .scale
                    .is_some_and(|scale| value.significant_places() > scale)
                {
                    return Some("it has more places than the field's `scale`".into());
                }
            }
            None
        }
        (Ty::Named(name), Const::Path(path)) => match lookup.def(name) {
            Some(Def::Enum { variants })
                if path.len() == 2
                    && &path[0] == name
                    && variants
                        .iter()
                        .any(|v| v.name == path[1] && v.fields.is_none()) =>
            {
                None
            }
            Some(Def::Enum { .. }) => Some(format!("it isn't a plain variant of `{name}`")),
            _ => Some(format!("a `{name}` can't be written as a literal here")),
        },
        (Ty::Prim(prim), _) => Some(format!("it isn't a `{}` literal", prim.name())),
        (other, _) => Some(format!(
            "a `{other}` can't be backfilled with a literal yet"
        )),
    }
}

/// A backfill value as SQL.
pub fn backfill_sql(value: &Const) -> Option<String> {
    Some(match value {
        Const::Str(text) => format!("'{}'", text.replace('\'', "''")),
        Const::Bool(value) => value.to_string(),
        Const::Number { value, .. } => number_text(*value),
        Const::Path(path) if path.len() == 2 => format!("'{}'", path[1].replace('\'', "''")),
        _ => return None,
    })
}

fn number_text(value: Num) -> String {
    let negative = value.mantissa < 0;
    let digits = value.mantissa.unsigned_abs().to_string();
    let scale = value.scale as usize;
    let text = if scale == 0 {
        digits
    } else if digits.len() > scale {
        format!(
            "{}.{}",
            &digits[..digits.len() - scale],
            &digits[digits.len() - scale..]
        )
    } else {
        format!("0.{}{digits}", "0".repeat(scale - digits.len()))
    };
    if negative { format!("-{text}") } else { text }
}

/// The type of field `entity.field` (born names, which are the names now
/// until renames are read), from the entities.
fn field_type<'m>(model: &'m DataModel, entity: &str, field: &str) -> Option<&'m Ty> {
    model
        .entities
        .iter()
        .find(|e| e.born == entity)?
        .fields
        .iter()
        .find(|f| f.born == field)
        .map(|f| &f.ty)
}

/// What `jadpo check` says about the migrations: unreadable records, an
/// unfinished migration's decisions, and drift between the entities and
/// the latest snapshot.
#[salsa::tracked(returns(ref))]
pub fn migration_problems(db: &dyn Db, project: Project) -> Vec<Diagnostic> {
    if project.layout(db) != Layout::Project {
        return Vec::new();
    }
    let set = migrations(db, project);
    let mut out = Vec::new();
    for migration in &set.list {
        let first = format!("{FOLDER}/{}", migration.name);
        if !valid_name(&migration.name) {
            out.push(
                Diagnostic::new(
                    &rule::MIGRATION_RECORD_INVALID,
                    format!("`{}` isn't a migration's name", migration.name),
                )
                .at(file_span(&first), "not a timestamp and a purpose")
                .reason("A migration folder is named by the time `jadpo migrate new` made it and its purpose, `20261020T1400_add_priority`, so the folders sort in the order they apply.")
                .next("Put the folder back as `jadpo migrate new` named it"),
            );
            continue;
        }
        if let Some(Err(why)) = &migration.snapshot {
            out.push(record_invalid(&migration.snapshot_path(), why));
        }
        match (&migration.snapshot, &migration.sql) {
            (Some(_), None) => out.push(record_invalid(
                &migration.sql_path(),
                "it is missing, and every finished migration has one",
            )),
            (_, Some(Err(why))) => out.push(record_invalid(&migration.sql_path(), why)),
            _ => {}
        }
    }
    if !out.is_empty() {
        return out;
    }
    let model = data_model(db, project);
    let source = super::schema(db, project);
    let latest = set.latest_schema();
    let expected = expected(source, &latest);
    if let Some(unfinished) = set.unfinished() {
        out.extend(unfinished_problems(
            db, project, unfinished, &latest, &expected, model,
        ));
        return out;
    }
    if expected == latest {
        return out;
    }
    // What a person changed first: entities before the system tables that
    // follow them, and a new store says nothing its tables don't.
    let system = |change: &Change| {
        let (store, table) = change.table();
        let owner = |schema: &Schema| {
            schema
                .stores
                .get(store)
                .and_then(|s| s.tables.get(table?))
                .map(|t| matches!(t.owner, Owner::System(_)))
        };
        owner(&expected).or_else(|| owner(&latest)).unwrap_or(true)
    };
    let mut changes: Vec<Change> = schema::changes(&latest, &expected)
        .into_iter()
        .filter(|c| !matches!(c, Change::AddStore { .. }))
        .collect();
    changes.sort_by_key(system);
    let shown: Vec<String> = changes
        .iter()
        .take(5)
        .map(|c| c.describe(&latest, &expected))
        .collect();
    let more = changes.len().saturating_sub(shown.len());
    let mut listed = crate::and_list(&shown);
    if more > 0 {
        listed = format!("{}, and {more} more", shown.join(", "));
    }
    let at = changes
        .iter()
        .find_map(|change| {
            let (store, table) = change.table();
            let owner = expected
                .stores
                .get(store)
                .and_then(|s| s.tables.get(table?))
                .map(|t| t.owner.clone())?;
            let Owner::Entity(born) = owner else {
                return None;
            };
            model
                .entities
                .iter()
                .find(|e| e.born == born)
                .map(|e| e.name_span.clone())
        })
        .unwrap_or_else(|| file_span(jadpo_db::PROJECT_FILE));
    let purpose = if set.list.is_empty() {
        "initial".to_string()
    } else {
        purpose(&changes, &latest, &expected)
    };
    let (message, compared) = match set.list.last() {
        Some(last) => (
            "The entities have changed since the latest migration".to_string(),
            format!(
                "the latest migration's snapshot (`{}`)",
                last.snapshot_path()
            ),
        ),
        None => (
            "No migration creates these entities' tables yet".to_string(),
            "the migrations in `database/migrations/`, and there are none".to_string(),
        ),
    };
    out.push(
        Diagnostic::new(&rule::MIGRATION_OUT_OF_DATE, message)
            .at(at, if set.list.is_empty() { "no table yet" } else { "changed" })
            .reason(format!(
                "`jadpo check` compares the database schema the entities ask for with {compared}: {listed}."
            ))
            .next(format!("Run `jadpo migrate new {purpose}`")),
    );
    out
}

fn record_invalid(path: &str, why: &str) -> Diagnostic {
    Diagnostic::new(
        &rule::MIGRATION_RECORD_INVALID,
        format!("`{path}` can't be read"),
    )
    .at(file_span(path), "unreadable")
    .reason(format!(
        "A migration's records are written by `jadpo migrate new` and never edited by hand; this one can't be used: {why}."
    ))
    .next("Restore it from version control")
}

/// A purpose naming the first change: `add_priority`, `add_tag`.
fn purpose(changes: &[Change], before: &Schema, after: &Schema) -> String {
    let words = |text: &str| crate::case::to_snake(text);
    for change in changes {
        match change {
            Change::AddTable { store, table } => {
                if let Some(Owner::Entity(name)) = after
                    .stores
                    .get(store)
                    .and_then(|s| s.tables.get(table))
                    .map(|t| &t.owner)
                {
                    return format!("add_{}", words(name));
                }
            }
            Change::AddColumn { column, .. } => return format!("add_{column}"),
            Change::RemoveColumn { column, .. } => return format!("retire_{column}"),
            Change::AlterColumn { column, .. } => return format!("change_{column}"),
            _ => {}
        }
    }
    let _ = before;
    "<purpose>".to_string()
}

fn unfinished_problems(
    db: &dyn Db,
    project: Project,
    migration: &Migration,
    latest: &Schema,
    expected: &Schema,
    model: &DataModel,
) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    let path = migration.changes_path();
    let file: Option<File> = project
        .files(db)
        .iter()
        .copied()
        .find(|f| f.path(db) == &path);
    let (decisions, at) = match file.and_then(|f| tree::source(db, f)) {
        Some(source) => read_decisions(&path, &source),
        None => (
            Vec::new(),
            file_span(&format!("{FOLDER}/{}", migration.name)),
        ),
    };
    let needed = decisions_needed(latest, expected);
    let lookup = Lookup::new(db, project);
    for need in &needed {
        let key = need.key();
        match decisions.iter().find(|d| d.key == key) {
            None => {
                let (what, write) = match need.kind {
                    NeededKind::Backfill => (
                        format!("`{key}` is required, and existing rows have no value for it"),
                        format!("`{key}: {{ backfill: <value> }}`, the value existing rows get"),
                    ),
                    NeededKind::Retire => (
                        format!("`{key}` is gone from `{}`", need.entity),
                        format!(
                            "`{key}: FieldChange.retired`, which keeps the column and its data while code stops using it; if it was renamed, give it back its name, since a rename that keeps the column comes with `jadpo rename`"
                        ),
                    ),
                };
                out.push(
                    Diagnostic::new(
                        &rule::MIGRATION_DECISION_REQUIRED,
                        format!("Migration `{}` needs a decision about `{key}`", migration.name),
                    )
                    .at(at.clone(), "decide here")
                    .reason(format!("{what}, so `jadpo migrate new` can't write the migration without a decision."))
                    .next(format!("Add {write} to `{path}`, then run `jadpo migrate new` again")),
                );
            }
            Some(decision) => {
                let problem = match (&decision.value, need.kind) {
                    (DecisionValue::Backfill(value, span), NeededKind::Backfill) => {
                        let ty = field_type(model, &need.entity, &need.field);
                        ty.and_then(|ty| backfill_problem(&lookup, ty, value))
                            .map(|why| (span.clone(), format!("This value can't fill `{key}`: {why}.")))
                    }
                    (DecisionValue::Change(FieldChange::Retired), NeededKind::Retire) => None,
                    (DecisionValue::Change(FieldChange::Dropped), NeededKind::Retire) => Some((
                        decision.span.clone(),
                        "A field is removed in two migrations: `FieldChange.retired` first, and `FieldChange.dropped` in a later one.".to_string(),
                    )),
                    (DecisionValue::Invalid(what, span), _) => {
                        Some((span.clone(), format!("{what} isn't a decision `jadpo migrate new` understands.")))
                    }
                    (_, NeededKind::Backfill) => Some((
                        decision.span.clone(),
                        format!("`{key}` is a new required field, so its decision is `{{ backfill: <value> }}`."),
                    )),
                    (_, NeededKind::Retire) => Some((
                        decision.span.clone(),
                        format!("`{key}` is gone, so its decision is `FieldChange.retired`."),
                    )),
                };
                if let Some((span, reason)) = problem {
                    out.push(
                        Diagnostic::new(
                            &rule::MIGRATION_DECISION_INVALID,
                            format!("The decision about `{key}` doesn't fit its change"),
                        )
                        .at(span, "doesn't fit")
                        .reason(reason)
                        .next(match need.kind {
                            NeededKind::Backfill => format!(
                                "Write `{key}: {{ backfill: <value> }}` with a value of its type"
                            ),
                            NeededKind::Retire => format!("Write `{key}: FieldChange.retired`"),
                        }),
                    );
                }
            }
        }
    }
    for decision in &decisions {
        if needed.iter().any(|n| n.key() == decision.key) {
            continue;
        }
        if matches!(decision.value, DecisionValue::Change(FieldChange::Dropped)) {
            out.push(
                Diagnostic::new(
                    &rule::TOOLCHAIN_NOT_BUILT_YET,
                    format!("Dropping `{}` isn't built yet", decision.key),
                )
                .at(decision.span.clone(), "a drop")
                .reason("A drop removes a retired column and its data for good, so it needs a person's explicit acceptance (area 5), which comes with acceptances (area 18).")
                .next(format!("Keep `{}` retired for now", decision.key)),
            );
            continue;
        }
        out.push(
            Diagnostic::new(
                &rule::MIGRATION_DECISION_UNKNOWN,
                format!("Nothing about `{}` needs a decision", decision.key),
            )
            .at(decision.span.clone(), "no such change")
            .reason("A migration's `changes.jadpo` holds only the decisions its changes need: a value for existing rows of a new required field, or retiring a field that is gone.")
            .next(format!("Remove `{}`", decision.key)),
        );
    }
    if out.is_empty() {
        out.push(
            Diagnostic::new(
                &rule::MIGRATION_UNFINISHED,
                format!("Migration `{}` isn't finished", migration.name),
            )
            .at(at, "waiting")
            .reason("`jadpo migrate new` started it and waited for decisions, which are all written now; it writes the snapshot and the SQL when it runs again.")
            .next("Run `jadpo migrate new`"),
        );
    }
    out
}
