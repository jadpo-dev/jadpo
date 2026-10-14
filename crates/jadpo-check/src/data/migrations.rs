//! Migrations as `jadpo check` sees them (area 5, "Migrations"; area 18):
//! each folder under `database/migrations/` holds the whole schema at that
//! point (`snapshot.json`), the SQL that got there (`postgres.sql`), and
//! `changes.jadpo` when a change needed a person's decision. `jadpo check`
//! fails while the entities and the latest snapshot disagree, naming what
//! changed and the command to run, and while the latest migration waits
//! for a decision or its SQL doesn't hold the decisions yet.
//!
//! What a migration needs and the SQL it gets are worked out in
//! `jadpo_graph::migration`, from the schemas alone, so `jadpo check` and
//! `jadpo migrate new` agree on them.

use std::collections::BTreeMap;
use std::sync::Arc;

use jadpo_db::{Db, File, Layout, Project};
use jadpo_diag::{Diagnostic, Span, rule};
use jadpo_graph::migration::{
    Decided, Needed, NeededKind, decisions_needed, migration_sql, next_snapshot,
};
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
    /// Its snapshot, or why it can't be read; `None` when it is missing.
    pub snapshot: Option<Result<Schema, String>>,
    /// Its `postgres.sql`, or why it can't be read.
    pub sql: Option<Result<String, String>>,
    /// Whether it has a `changes.jadpo`.
    pub changes: bool,
}

impl Migration {
    pub fn folder(&self) -> String {
        format!("{FOLDER}/{}", self.name)
    }

    pub fn snapshot_path(&self) -> String {
        format!("{FOLDER}/{}/snapshot.json", self.name)
    }

    pub fn sql_path(&self) -> String {
        format!("{FOLDER}/{}/postgres.sql", self.name)
    }

    pub fn changes_path(&self) -> String {
        format!("{FOLDER}/{}/changes.jadpo", self.name)
    }

    /// Its snapshot, when it could be read.
    pub fn schema(&self) -> Option<&Schema> {
        self.snapshot.as_ref().and_then(|s| s.as_ref().ok())
    }
}

/// Every migration, in the order they apply.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Migrations {
    pub list: Vec<Migration>,
}

impl Migrations {
    /// The latest migration's schema; empty before the first.
    pub fn latest_schema(&self) -> Schema {
        self.list
            .last()
            .and_then(Migration::schema)
            .cloned()
            .unwrap_or_default()
    }

    /// The schema before the latest migration: the one it migrates from.
    pub fn before_latest(&self) -> Schema {
        self.list
            .len()
            .checked_sub(2)
            .and_then(|at| self.list.get(at))
            .and_then(Migration::schema)
            .cloned()
            .unwrap_or_default()
    }
}

/// Whether a folder name is a timestamp and a purpose:
/// `20261020T1400_add_priority`.
pub fn valid_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    bytes.len() > 14
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
    fn folder<'f>(folders: &'f mut BTreeMap<String, Migration>, name: &str) -> &'f mut Migration {
        folders
            .entry(name.to_string())
            .or_insert_with(|| Migration {
                name: name.to_string(),
                snapshot: None,
                sql: None,
                changes: false,
            })
    }
    for record in project.records(db) {
        if let ["database", "migrations", name, file] =
            record.path.split('/').collect::<Vec<_>>().as_slice()
        {
            let migration = folder(&mut folders, name);
            if *file == "snapshot.json" {
                migration.snapshot = Some(record.text.clone().and_then(|text| Schema::read(&text)));
            } else {
                migration.sql = Some(record.text.clone());
            }
        }
    }
    for file in project.files(db) {
        if let ["database", "migrations", name, "changes.jadpo"] =
            file.path(db).split('/').collect::<Vec<_>>().as_slice()
        {
            folder(&mut folders, name).changes = true;
        }
    }
    Arc::new(Migrations {
        list: folders.into_values().collect(),
    })
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
    /// `Todo.priority: {}`, as `jadpo migrate new` leaves it to fill in.
    Pending,
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
        let Some(keyword) = singleton.keyword() else {
            continue;
        };
        if keyword.text() != "migration" {
            continue;
        }
        let range = keyword.text_range();
        at = Span::new(path, range.start().into(), range.end().into());
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
            let block = member.block().or_else(|| match member.value() {
                Some(Expr::RecordExpr(record)) => record.member_list(),
                _ => None,
            });
            let value = match (block, member.value()) {
                (Some(block), _) => backfill(path, block, &span),
                (None, Some(value)) => {
                    let value_span = node_span(path, value.syntax());
                    match Const::from_expr(&value) {
                        Const::Path(p) if p.len() == 2 && p[0] == FieldChange::NAME => {
                            match FieldChange::from_name(&p[1]) {
                                Some(change) => DecisionValue::Change(change),
                                None => {
                                    DecisionValue::Invalid(format!("`{}`", p.join(".")), value_span)
                                }
                            }
                        }
                        _ => DecisionValue::Invalid(
                            format!("`{}`", tree::written(value.syntax())),
                            value_span,
                        ),
                    }
                }
                (None, None) => DecisionValue::Pending,
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
        [] => DecisionValue::Pending,
        [(name, member)] if tree::name_text(name).as_deref() == Some("backfill") => {
            match member.value() {
                Some(value) => DecisionValue::Backfill(
                    Const::from_expr(&value),
                    node_span(path, value.syntax()),
                ),
                None => DecisionValue::Pending,
            }
        }
        _ => DecisionValue::Invalid(
            "a block that isn't `{ backfill: <value> }`".into(),
            at.clone(),
        ),
    }
}

/// Why a backfill value doesn't fit a field of type `ty`, if it doesn't.
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
                    return Some("it is shorter than the field's `min_length`".into());
                }
                if rules.max_length.is_some_and(|max| length > max) {
                    return Some("it is longer than the field's `max_length`".into());
                }
            }
            None
        }
        (Ty::Prim(Builtin::Uuid), Const::Str(text)) => {
            let shape = text.len() == 36
                && text.chars().enumerate().all(|(i, c)| {
                    matches!(i, 8 | 13 | 18 | 23) == (c == '-')
                        && (c == '-' || c.is_ascii_hexdigit())
                });
            (!shape).then(|| "it isn't a UUID".to_string())
        }
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

/// A value of a field's type to suggest for its backfill: the first plain
/// variant of an enum, `false`, the smallest number its rules allow, or a
/// text of the shortest length allowed.
pub fn example_backfill(lookup: &dyn TypeView, ty: &Ty) -> Option<String> {
    let rules: Vec<_> = lookup
        .rule_chain(ty)
        .into_iter()
        .map(|(_, r)| r.clone())
        .collect();
    Some(match lookup.structure(ty) {
        Ty::Named(name) => match lookup.def(&name)? {
            Def::Enum { variants } => {
                let first = variants.iter().find(|v| v.fields.is_none())?;
                format!("{name}.{}", first.name)
            }
            _ => return None,
        },
        Ty::Prim(Builtin::Bool) => "false".into(),
        Ty::Prim(Builtin::Int | Builtin::Decimal) => rules
            .iter()
            .find_map(|r| r.min)
            .map_or_else(|| "0".into(), number_text),
        Ty::Prim(prim) if prim.representation() == Builtin::Text => {
            let min = rules.iter().find_map(|r| r.min_length).unwrap_or(0);
            format!("\"{}\"", "x".repeat(usize::try_from(min).unwrap_or(0)))
        }
        _ => return None,
    })
}

/// The type of field `entity.field` by born names, from the entities.
pub fn field_type<'m>(model: &'m DataModel, entity: &str, field: &str) -> Option<&'m Ty> {
    model
        .entities
        .iter()
        .find(|e| e.born == entity)?
        .fields
        .iter()
        .find(|f| f.born == field)
        .map(|f| &f.ty)
}

/// What a needed decision is, as SQL, from what `changes.jadpo` says:
/// `None` for one still to make, or that doesn't fit.
pub fn decided(
    lookup: &dyn TypeView,
    model: &DataModel,
    decisions: &[Decision],
    need: &Needed,
) -> Option<Decided> {
    let decision = decisions.iter().find(|d| d.key == need.key())?;
    match (&decision.value, need.kind) {
        (DecisionValue::Backfill(value, _), NeededKind::Backfill) => {
            let ty = field_type(model, &need.entity, &need.field)?;
            if backfill_problem(lookup, ty, value).is_some() {
                return None;
            }
            backfill_sql(value).map(Decided::Backfill)
        }
        (DecisionValue::Change(FieldChange::Retired), NeededKind::Retire) => Some(Decided::Retired),
        _ => None,
    }
}

/// What `jadpo check` says about the migrations: unreadable records,
/// drift between the entities and the latest snapshot, and the latest
/// migration's decisions.
#[salsa::tracked(returns(ref))]
pub fn migration_problems(db: &dyn Db, project: Project) -> Vec<Diagnostic> {
    if project.layout(db) != Layout::Project {
        return Vec::new();
    }
    let set = migrations(db, project);
    let mut out = Vec::new();
    for migration in &set.list {
        if !valid_name(&migration.name) {
            out.push(
                Diagnostic::new(
                    &rule::MIGRATION_RECORD_INVALID,
                    format!("`{}` isn't a migration's name", migration.name),
                )
                .at(file_span(&migration.folder()), "not a timestamp and a purpose")
                .reason("A migration folder is named by when `jadpo migrate new` made it and what it does, `20261020T1400_add_priority`, so the folders sort in the order they apply.")
                .next("Put the folder back as `jadpo migrate new` named it"),
            );
            continue;
        }
        match &migration.snapshot {
            None => out.push(record_invalid(
                &migration.snapshot_path(),
                "it is missing, and every migration has one",
            )),
            Some(Err(why)) => out.push(record_invalid(&migration.snapshot_path(), why)),
            Some(Ok(_)) => {}
        }
        match &migration.sql {
            None => out.push(record_invalid(
                &migration.sql_path(),
                "it is missing, and every migration has one",
            )),
            Some(Err(why)) => out.push(record_invalid(&migration.sql_path(), why)),
            Some(Ok(_)) => {}
        }
    }
    if !out.is_empty() {
        return out;
    }
    let model = data_model(db, project);
    if !model.problems.is_empty() {
        // The schema is partial until the entities check: their own
        // errors come first.
        return out;
    }
    let source = super::schema(db, project);
    let latest = set.latest_schema();
    let next = next_snapshot(&latest, source);
    if next != latest {
        out.push(out_of_date(set, model, &latest, &next));
        return out;
    }
    if let Some(migration) = set.list.last() {
        out.extend(decision_problems(db, project, set, migration, model));
    }
    out
}

/// The changes from `latest` to `next` in the order a person reads them:
/// entities before the system tables that follow them, and no new store,
/// which its tables already say.
pub fn readable_changes(latest: &Schema, next: &Schema) -> Vec<Change> {
    let system = |change: &Change| {
        let (store, table) = change.table();
        let owner = |schema: &Schema| {
            schema
                .stores
                .get(store)
                .and_then(|s| s.tables.get(table?))
                .map(|t| matches!(t.owner, Owner::System(_)))
        };
        owner(next).or_else(|| owner(latest)).unwrap_or(true)
    };
    let mut changes: Vec<Change> = schema::changes(latest, next)
        .into_iter()
        .filter(|c| !matches!(c, Change::AddStore { .. }))
        .collect();
    changes.sort_by_key(system);
    changes
}

fn out_of_date(set: &Migrations, model: &DataModel, latest: &Schema, next: &Schema) -> Diagnostic {
    let changes = readable_changes(latest, next);
    let shown: Vec<String> = changes
        .iter()
        .take(5)
        .map(|c| c.describe(latest, next))
        .collect();
    let more = changes.len().saturating_sub(shown.len());
    let listed = if more > 0 {
        format!("{}, and {more} more", shown.join(", "))
    } else {
        crate::and_list(&shown)
    };
    let at = changes
        .iter()
        .find_map(|change| {
            let (store, table) = change.table();
            let owner = next
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
    let (message, label, compared, purpose) = match set.list.last() {
        Some(last) => (
            "The entities have changed since the latest migration",
            "changed",
            format!(
                "the latest migration's snapshot (`{}`)",
                last.snapshot_path()
            ),
            purpose(&changes, next),
        ),
        None => (
            "No migration creates these entities' tables yet",
            "no table yet",
            "the migrations in `database/migrations/`, and there are none".to_string(),
            "initial".to_string(),
        ),
    };
    Diagnostic::new(&rule::MIGRATION_OUT_OF_DATE, message)
        .at(at, label)
        .reason(format!(
            "`jadpo check` compares the database schema the entities ask for with {compared}: {listed}."
        ))
        .next(format!("Run `jadpo migrate new {purpose}`"))
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
pub fn purpose(changes: &[Change], after: &Schema) -> String {
    for change in changes {
        match change {
            Change::AddTable { store, table } => {
                if let Some(Owner::Entity(name)) = after
                    .stores
                    .get(store)
                    .and_then(|s| s.tables.get(table))
                    .map(|t| &t.owner)
                {
                    return format!("add_{}", crate::case::to_snake(name));
                }
            }
            Change::AddColumn { column, .. } => return format!("add_{column}"),
            Change::RemoveColumn { column, .. } => return format!("retire_{column}"),
            Change::AlterColumn {
                store,
                table,
                column,
            } => {
                let retired = after
                    .stores
                    .get(store)
                    .and_then(|s| s.tables.get(table))
                    .and_then(|t| t.columns.get(column))
                    .is_some_and(|c| c.retired);
                return if retired {
                    format!("retire_{column}")
                } else {
                    format!("change_{column}")
                };
            }
            Change::AddIndex { name, .. } | Change::AddUnique { name, .. } => {
                return format!("add_{name}");
            }
            _ => {}
        }
    }
    "describe_the_change".to_string()
}

/// The latest migration's decisions: each one its changes need must be
/// written in its `changes.jadpo` and fit, and its SQL must hold them.
fn decision_problems(
    db: &dyn Db,
    project: Project,
    set: &Migrations,
    migration: &Migration,
    model: &DataModel,
) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    let before = set.before_latest();
    let Some(after) = migration.schema() else {
        return out;
    };
    let needed = decisions_needed(&before, after);
    let path = migration.changes_path();
    let file: Option<File> = project
        .files(db)
        .iter()
        .copied()
        .find(|f| f.path(db) == &path);
    let (decisions, at) = match file.and_then(|f| tree::source(db, f)) {
        Some(source) => read_decisions(&path, &source),
        None => (Vec::new(), file_span(&migration.folder())),
    };
    if needed.is_empty() && decisions.is_empty() {
        return out;
    }
    let lookup = Lookup::new(db, project);
    for need in &needed {
        let key = need.key();
        let written = decisions.iter().find(|d| d.key == key);
        if matches!(
            written.map(|d| &d.value),
            None | Some(DecisionValue::Pending)
        ) {
            let span = written.map_or_else(|| at.clone(), |d| d.span.clone());
            let example = field_type(model, &need.entity, &need.field)
                .and_then(|ty| example_backfill(&lookup, ty));
            out.push(required(migration, need, span, example));
            continue;
        }
        let Some(decision) = written else {
            continue;
        };
        if let Some((span, reason)) = misfit(&lookup, model, need, decision) {
            out.push(
                Diagnostic::new(
                    &rule::MIGRATION_DECISION_INVALID,
                    format!("The decision about `{key}` doesn't fit its change"),
                )
                .at(span, "doesn't fit")
                .reason(reason)
                .next(match need.kind {
                    NeededKind::Backfill => {
                        format!("Write `{key}: {{ backfill: <value> }}` with a value of its type")
                    }
                    NeededKind::Retire => format!("Write `{key}: FieldChange.retired`"),
                }),
            );
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
            .reason(format!(
                "A migration's `changes.jadpo` holds only the decisions its own changes need, and nothing `{}` changes about `{}` does: a decision is a value for existing rows of a required field, or retiring a field that is gone.",
                migration.name, decision.key
            ))
            .next(format!("Remove `{}`", decision.key)),
        );
    }
    if !out.is_empty() {
        return out;
    }
    // Every decision is made: the SQL must hold them.
    let decide = |need: &Needed| decided(&lookup, model, &decisions, need);
    let Ok(sql) = migration_sql(&before, after, &decide) else {
        return out;
    };
    let written = migration.sql.as_ref().and_then(|s| s.as_ref().ok());
    if written.map(String::as_str) != Some(sql.to_document(&migration.name).as_str()) {
        out.push(
            Diagnostic::new(
                &rule::MIGRATION_UNFINISHED,
                format!("Migration `{}` doesn't hold its decisions yet", migration.name),
            )
            .at(at, "decided")
            .reason(format!(
                "Its decisions are all written in `changes.jadpo`, but `{}` was written before them, so it doesn't carry them out.",
                migration.sql_path()
            ))
            .next("Run `jadpo migrate new` to finish it"),
        );
    }
    out
}

/// Why a written decision doesn't fit the change it is about, with where.
fn misfit(
    lookup: &dyn TypeView,
    model: &DataModel,
    need: &Needed,
    decision: &Decision,
) -> Option<(Span, String)> {
    let key = need.key();
    match (&decision.value, need.kind) {
        (DecisionValue::Backfill(value, span), NeededKind::Backfill) => {
            field_type(model, &need.entity, &need.field)
                .and_then(|ty| backfill_problem(lookup, ty, value))
                .map(|why| (span.clone(), format!("This value can't fill `{key}`: {why}.")))
        }
        (DecisionValue::Change(FieldChange::Retired), NeededKind::Retire) => None,
        (DecisionValue::Change(FieldChange::Dropped), NeededKind::Retire) => Some((
            decision.span.clone(),
            "A field is removed in two migrations: `FieldChange.retired` first, and `FieldChange.dropped` in a later one.".to_string(),
        )),
        (DecisionValue::Invalid(what, span), _) => Some((
            span.clone(),
            format!("{what} isn't a decision `jadpo migrate new` understands."),
        )),
        (_, NeededKind::Backfill) => Some((
            decision.span.clone(),
            format!("`{key}` is required and existing rows have no value for it, so its decision is `{{ backfill: <value> }}`."),
        )),
        (_, NeededKind::Retire) => Some((
            decision.span.clone(),
            format!("`{key}` is gone, so its decision is `FieldChange.retired`."),
        )),
    }
}

fn required(
    migration: &Migration,
    need: &Needed,
    span: Span,
    example: Option<String>,
) -> Diagnostic {
    let key = need.key();
    let (what, next) = match need.kind {
        NeededKind::Backfill => (
            format!(
                "`{key}` is required, and the `{}` records already stored have no value for it.",
                need.entity
            ),
            match example {
                Some(example) => {
                    format!("Set `backfill:` for `{key}`, such as `backfill: {example}`")
                }
                None => format!("Set `backfill:` for `{key}`"),
            },
        ),
        NeededKind::Retire => (
            format!(
                "`{key}` is gone from `{}`. Retiring its column keeps the column and its data while code stops using it; if the field was renamed instead, give it back its old name, since a rename that keeps the column comes with `jadpo rename`.",
                need.entity
            ),
            format!("Write `{key}: FieldChange.retired`"),
        ),
    };
    Diagnostic::new(
        &rule::MIGRATION_DECISION_REQUIRED,
        format!(
            "Migration `{}` needs a decision about `{key}`",
            migration.name
        ),
    )
    .at(span, "decide here")
    .reason(what)
    .next_for(
        format!("{next}, then run `jadpo migrate new` to finish the migration"),
        jadpo_diag::Decider::Human,
    )
}

/// One migration's SQL for one store, as `migrate apply` and the test
/// database run it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
    /// The migration's folder name.
    pub name: String,
    /// The digest a store records for it.
    pub digest: String,
    pub sql: String,
}

/// Each store's steps, in the order they apply: every migration's section
/// for that store. Migrations whose records can't be read give none, and
/// `jadpo check` reports them.
pub fn store_steps(set: &Migrations) -> BTreeMap<String, Vec<Step>> {
    let mut out: BTreeMap<String, Vec<Step>> = BTreeMap::new();
    for migration in &set.list {
        let Some(Ok(sql)) = &migration.sql else {
            continue;
        };
        for section in jadpo_graph::migration::sections(sql) {
            out.entry(section.store).or_default().push(Step {
                name: migration.name.clone(),
                digest: jadpo_graph::migration::section_digest(&section.sql),
                sql: section.sql,
            });
        }
    }
    out
}
