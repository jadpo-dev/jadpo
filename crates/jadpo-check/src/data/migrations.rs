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
    unexplained_removals,
};
use jadpo_graph::schema::{self, Change, Holds, Schema};
use jadpo_std::{Builtin, EntityChange, FieldChange, Num};
use jadpo_syntax::ast::{self, AstNode, Decl, Expr};

use super::{DataModel, data_model};
use crate::lookups::{Lookup, Name};
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
    /// Where it is: its `migration`'s place among the file's declarations
    /// and its own among the migration's decisions, which name a computed
    /// backfill's body (chunk 5.6b).
    pub decl: u32,
    pub member: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DecisionValue {
    /// `Todo.priority: {}`, as `jadpo migrate new` leaves it to fill in.
    Pending,
    Change(FieldChange),
    /// `Todo: EntityChange.retired`, about the whole entity (chunk V3).
    Entity(EntityChange),
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
    for (decl_index, decl) in source.decls().enumerate() {
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
        let members = singleton
            .member_list()
            .into_iter()
            .flat_map(|list| list.member_items())
            .filter_map(|item| match item {
                ast::MemberItem::Member(member) => Some(member),
                _ => None,
            });
        for (member_index, member) in members.enumerate() {
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
                        Const::Path(p) if p.len() == 2 && p[0] == EntityChange::NAME => {
                            match EntityChange::from_name(&p[1]) {
                                Some(change) => DecisionValue::Entity(change),
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
                decl: decl_index as u32,
                member: member_index as u32,
            });
        }
    }
    (out, at)
}

/// The decision at `index` among a migration's, when it is a backfill:
/// the entity and field it is about and its value (area 5, chunk 5.6b).
pub fn backfill_member(
    migration: &ast::SingletonDecl,
    index: usize,
) -> Option<(String, String, ast::Expr)> {
    if migration.keyword()?.text() != "migration" {
        return None;
    }
    let member = migration
        .member_list()?
        .member_items()
        .filter_map(|item| match item {
            ast::MemberItem::Member(member) => Some(member),
            _ => None,
        })
        .nth(index)?;
    let names: Vec<String> = member
        .member_key()?
        .names()
        .filter_map(|n| tree::name_text(&n))
        .collect();
    let [entity, field] = names.as_slice() else {
        return None;
    };
    let block = member.block().or_else(|| match member.value() {
        Some(Expr::RecordExpr(record)) => record.member_list(),
        _ => None,
    })?;
    let members = tree::members(Some(block));
    let [(name, value)] = members.as_slice() else {
        return None;
    };
    if tree::name_text(name).as_deref() != Some("backfill") {
        return None;
    }
    Some((entity.clone(), field.clone(), value.value()?))
}

/// The fields of `entity` the latest migration retires, each with the
/// built-in type its column holds: what that migration's computed
/// backfills may still read (area 5, "Computed backfills"). A retired
/// column of any other kind (an enum's text, a structured value) isn't
/// offered, since its type is no longer declared anywhere.
pub fn retired_fields(db: &dyn Db, project: Project, entity: &str) -> Vec<(String, Ty)> {
    let set = migrations(db, project);
    let (before, after) = (set.before_latest(), set.latest_schema());
    let mut out = Vec::new();
    for (store, tables) in &after.stores {
        for (name, table) in &tables.tables {
            if table.holds != Holds::Entity(entity.to_string()) {
                continue;
            }
            let was = before.stores.get(store).and_then(|s| s.tables.get(name));
            for (column, now) in &table.columns {
                let held = was.and_then(|t| t.columns.get(column));
                if !now.retired || held.is_none_or(|c| c.retired) {
                    continue;
                }
                let enum_text = now.allows.as_ref().is_some_and(|a| !a.values.is_empty());
                if let (Some(field), Some(prim), false) =
                    (&now.field, retired_type(&now.sql_type), enum_text)
                {
                    out.push((field.clone(), Ty::Prim(prim)));
                }
            }
        }
    }
    out
}

/// The fields of `entity` the latest migration adds: what its computed
/// backfills may not read, since no stored record has a value for them
/// while the backfill runs (chunk 5.8b).
pub fn added_fields(db: &dyn Db, project: Project, entity: &str) -> Vec<String> {
    let set = migrations(db, project);
    let (before, after) = (set.before_latest(), set.latest_schema());
    let mut out = Vec::new();
    for (store, tables) in &after.stores {
        for (name, table) in &tables.tables {
            if table.holds != Holds::Entity(entity.to_string()) {
                continue;
            }
            let was = before.stores.get(store).and_then(|s| s.tables.get(name));
            for (column, now) in &table.columns {
                if now.retired || was.is_some_and(|t| t.columns.contains_key(column)) {
                    continue;
                }
                if let Some(field) = &now.field
                    && !out.contains(field)
                {
                    out.push(field.clone());
                }
            }
        }
    }
    out
}

/// The built-in type a column of `sql_type` holds, when it holds one.
fn retired_type(sql_type: &str) -> Option<Builtin> {
    Some(match sql_type.split('(').next()?.trim() {
        "boolean" => Builtin::Bool,
        "text" => Builtin::Text,
        "uuid" => Builtin::Uuid,
        "smallint" | "integer" | "bigint" => Builtin::Int,
        "numeric" => Builtin::Decimal,
        "timestamptz" => Builtin::Instant,
        "date" => Builtin::CalendarDate,
        _ => return None,
    })
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
    // PostgreSQL text can't hold the NUL character, so a statement can't
    // either, and no parameter holding one is ever stored (chunk 5.5w3b).
    if matches!(value, Const::Str(text) if text.contains('\0')) {
        return Some("PostgreSQL text can't hold the NUL character".into());
    }
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
            // A standard type held as text checks its own (an email
            // address), as any literal of it is checked.
            literal_problem(lookup, ty, value, *prim)
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
        // An amount of money has no literal, and is stored in two columns
        // a backfill would have to fill (area 19).
        (Ty::Prim(Builtin::Money), _) => Some(
            "an amount of money has no literal, so a required one can't be backfilled yet: add it optional and set it in an action".into(),
        ),
        // Every other built-in type's literal (an instant or a date from
        // its text, a duration, a quantity, a percentage), checked as any
        // literal of it is (chunk 5.5w3b).
        (Ty::Prim(prim), value) if !matches!(value, Const::Path(_) | Const::Other) => {
            literal_problem(lookup, ty, value, *prim)
        }
        (Ty::Prim(prim), _) => Some(format!("it isn't a `{}` literal", prim.name())),
        (other, _) => Some(format!(
            "a `{other}` can't be backfilled with a literal yet"
        )),
    }
}

/// Why a literal doesn't fit a field of type `ty`, as the checker says of
/// any literal, if it doesn't.
fn literal_problem(lookup: &dyn TypeView, ty: &Ty, value: &Const, prim: Builtin) -> Option<String> {
    use crate::types::literal::{Problem, check};
    match check(lookup, ty, value) {
        Ok(()) => None,
        Err(Problem::Builtin(why)) => Some(format!("the value {why}")),
        Err(Problem::Broken(_, broken)) => Some(format!("the value {}", broken.why)),
        Err(_) => Some(format!("it isn't a `{}` literal", prim.name())),
    }
}

/// A backfill value as SQL, for `field`: decoded for its type and made the
/// value its column is given by the storage codec, exactly as the runtime
/// would bind it, then written as SQL text, since a migration's
/// `postgres.sql` takes no parameters (area 19, chunk 5.5w3b). `None` for a
/// value SQL text can't hold.
pub fn backfill_sql(field: &super::FieldModel, value: &Const) -> Option<String> {
    let value = match value {
        Const::Path(path) if path.len() == 2 => jadpo_sql::Value::Variant {
            ty: path[0].clone(),
            variant: path[1].clone(),
        },
        other => jadpo_sql::Value::Literal(other.literal()?),
    };
    jadpo_sql::value(&mut jadpo_sql::Fixed, &value, &field.column())
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
    field_model(model, entity, field).map(|f| &f.ty)
}

/// Field `entity.field` by born names, from the entities.
pub fn field_model<'m>(
    model: &'m DataModel,
    entity: &str,
    field: &str,
) -> Option<&'m super::FieldModel> {
    model
        .entities
        .iter()
        .find(|e| e.born == entity)?
        .fields
        .iter()
        .find(|f| f.born == field)
}

/// What the decisions written in a migration's `changes.jadpo` decide:
/// each needed decision as SQL, and why one doesn't fit its change.
#[allow(missing_debug_implementations)]
pub struct Decide<'a> {
    pub db: &'a dyn Db,
    pub project: Project,
    pub lookup: &'a dyn TypeView,
    pub model: &'a DataModel,
    pub decisions: &'a [Decision],
    /// The `changes.jadpo` they are written in, whose computed backfills
    /// are checked as bodies (chunk 5.6b).
    pub file: Option<File>,
    /// The schema the migration starts from: the columns a computed
    /// backfill reads.
    pub before: &'a Schema,
}

/// A computed backfill checked (area 5, "Computed backfills"; chunk
/// 5.6b): what is wrong with it, and its SQL when nothing is and every
/// operation in it has a SQL form, or otherwise how `jadpo migrate apply`
/// works it out in Jadpo (chunk 5.6b2) when it can.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Computed {
    pub problems: Vec<Diagnostic>,
    pub sql: Option<String>,
    pub jadpo: Option<super::backfill::InJadpo>,
}

impl Decide<'_> {
    /// What a needed decision is, as SQL: `None` for one still to make,
    /// or that doesn't fit, or a computed backfill that can't run yet.
    pub fn decided(&self, need: &Needed) -> Option<Decided> {
        let decision = self.decisions.iter().find(|d| d.key == need.key())?;
        match (&decision.value, need.kind) {
            (DecisionValue::Backfill(value, _), NeededKind::Backfill | NeededKind::Unreadable)
                if self.is_computed(need, value) =>
            {
                let computed = self.computed(decision, need)?;
                if !computed.problems.is_empty() {
                    return None;
                }
                match (computed.sql, computed.jadpo) {
                    (Some(sql), _) => Some(Decided::Computed(sql)),
                    (None, Some(jadpo)) => Some(Decided::InJadpo {
                        reads: jadpo.reads.iter().map(|read| read.column.clone()).collect(),
                        code: jadpo.code,
                    }),
                    (None, None) => None,
                }
            }
            (DecisionValue::Backfill(value, _), NeededKind::Refit)
                if self.is_computed(need, value) =>
            {
                None
            }
            (DecisionValue::Backfill(value, _), NeededKind::Backfill | NeededKind::Refit) => {
                let field = field_model(self.model, &need.entity, &need.field)?;
                if backfill_problem(self.lookup, &field.ty, value).is_some() {
                    return None;
                }
                backfill_sql(field, value).map(Decided::Backfill)
            }
            (DecisionValue::Change(FieldChange::Retired), NeededKind::Retire)
            | (DecisionValue::Entity(EntityChange::Retired), NeededKind::RetireEntity) => {
                Some(Decided::Retired)
            }
            (DecisionValue::Change(FieldChange::Dropped), NeededKind::DropField)
            | (DecisionValue::Entity(EntityChange::Dropped), NeededKind::DropEntity) => {
                Some(Decided::Dropped)
            }
            _ => None,
        }
    }

    /// Why the decision written about `need` doesn't fit its change, when
    /// one is written and doesn't: what `jadpo migrate new` shows beside
    /// it. A computed backfill's own problems are `jadpo check`'s.
    pub fn misfit(&self, need: &Needed) -> Option<String> {
        let decision = self.decisions.iter().find(|d| d.key == need.key())?;
        if matches!(decision.value, DecisionValue::Pending) {
            return None;
        }
        if let DecisionValue::Backfill(value, _) = &decision.value
            && self.is_computed(need, value)
        {
            return None;
        }
        misfit(self.lookup, self.model, need, decision).map(|(_, why)| why)
    }

    /// Whether a backfill's value is worked out from each row rather than
    /// written out: anything but a literal or an enum's variant
    /// (`Priority.normal`), which a backfill has given since chunk 3.1.
    pub fn is_computed(&self, need: &Needed, value: &Const) -> bool {
        // A structured value stored as JSON is written through the route
        // codec, so even a literal or a variant is worked out in Jadpo
        // (chunk 5.9c).
        if field_model(self.model, &need.entity, &need.field).is_some_and(|f| f.json.is_some()) {
            return true;
        }
        match value {
            Const::Other => true,
            Const::Path(path) => {
                !(path.len() == 2
                    && path[0] != need.entity
                    && (matches!(self.lookup.def(&path[0]), Some(Def::Enum { .. }))
                        || jadpo_std::standard_type(&path[0]).is_some_and(|entry| {
                            matches!(entry.holds, jadpo_std::standard::Holds::Enum(_))
                        })))
            }
            _ => false,
        }
    }

    /// A computed backfill checked as a body, and its SQL.
    pub fn computed(&self, decision: &Decision, need: &Needed) -> Option<Computed> {
        let file = self.file?;
        let callable =
            crate::types::Callable::new(self.db, file, decision.decl, Some(decision.member));
        let checked = crate::types::typed_body(self.db, self.project, callable);
        let mut problems: Vec<Diagnostic> = checked.problems.clone();
        problems.extend(checked.deferred.iter().map(|(_, problem)| problem.clone()));
        // A setting read through a function it calls, which `jadpo migrate
        // apply` has none of (chunk 5.8b).
        let path = file.path(self.db);
        for (at, function, setting) in
            super::backfill::settings_reached(self.db, self.project, &checked.body)
        {
            let site = checked.body.site(at);
            let entity = &need.entity;
            problems.push(
                super::backfill::outside_row(
                    &need.key(),
                    &setting,
                    Span::new(path.clone(), site.start, site.end),
                    &format!("reads a setting, through `{function}`"),
                )
                .reason(format!(
                    "A backfill gives each `{entity}` record the value its own fields give, whenever and wherever the migration runs, and `{function}` reads `{setting}`, which each deployment sets for itself."
                ))
                .next(format!(
                    "Work the value out without `{setting}`: from `{entity}`'s own fields, or a constant"
                )),
            );
        }
        if !problems.is_empty() {
            return Some(Computed {
                problems,
                sql: None,
                jadpo: None,
            });
        }
        let columns: BTreeMap<String, String> = self
            .before
            .stores
            .get(&need.store)
            .and_then(|store| store.tables.get(&need.table))
            .map(|table| {
                table
                    .columns
                    .iter()
                    .filter_map(|(column, def)| Some((def.field.clone()?, column.clone())))
                    .collect()
            })
            .unwrap_or_default();
        let scale = field_model(self.model, &need.entity, &need.field).and_then(|f| f.scale);
        let row = super::backfill::Row {
            table: &need.table,
            columns: &columns,
        };
        // A structured value has no SQL form: its JSON is the route
        // codec's, written in Jadpo (chunk 5.9c).
        let json =
            field_model(self.model, &need.entity, &need.field).is_some_and(|f| f.json.is_some());
        let sql = (!json)
            .then(|| super::backfill::sql(self.db, self.project, &checked.body, &row, scale))
            .flatten();
        let jadpo = match sql {
            Some(_) => None,
            None => self.in_jadpo(need, &checked.body, &columns, scale),
        };
        Some(Computed {
            problems: Vec::new(),
            sql,
            jadpo,
        })
    }

    /// How `jadpo migrate apply` works out in Jadpo a computed backfill
    /// with no SQL form (chunk 5.6b2), whose typed body is `body` and which
    /// reads the row's `columns` (by born name): `None` when it reads a
    /// value held in several columns or as JSON, or fills one held in
    /// several columns, which this compiler doesn't read or write one value
    /// at a time yet. A structured value it fills is written through the
    /// route codec (chunk 5.9c).
    fn in_jadpo(
        &self,
        need: &Needed,
        body: &jadpo_graph::body::Body,
        columns: &BTreeMap<String, String>,
        scale: Option<u32>,
    ) -> Option<super::backfill::InJadpo> {
        let entity = self.model.entities.iter().find(|e| e.born == need.entity)?;
        let field = field_model(self.model, &need.entity, &need.field)?;
        // A structured value is written through the route codec (chunk
        // 5.9c); any other only when held in one column.
        if field.json.is_none() {
            decode_column(self.lookup, &entity.name, field)?;
        }
        let sql_type = field.sql_type.clone()?;
        let write = jadpo_std::SqlType::of_column(&sql_type)?;
        let (function, names) = super::backfill::as_function(body);
        let retired = retired_fields(self.db, self.project, &need.entity);
        let mut reads = Vec::new();
        for name in names {
            let (decode, now) = match retired.iter().find(|(field, _)| *field == name) {
                Some((_, Ty::Prim(builtin))) => {
                    (jadpo_std::read::Column::of_builtin(*builtin)?, None)
                }
                Some(_) => return None,
                None => {
                    let read = field_model(self.model, &need.entity, &name)?;
                    (
                        decode_column(self.lookup, &entity.name, read)?,
                        Some(read.name.clone()),
                    )
                }
            };
            reads.push(super::backfill::Read {
                column: columns.get(&name)?.clone(),
                field: name,
                decode,
                name: now,
            });
        }
        Some(super::backfill::InJadpo {
            body: Arc::new(function),
            reads,
            write,
            sql_type,
            scale,
            fills: (entity.name.clone(), field.name.clone()),
            code: super::backfill::fingerprint(self.db, self.project, body),
        })
    }
}

/// How a field's one column is read, as the plan reads it for a query:
/// `None` for a value held in several columns, as JSON or as a vector.
fn decode_column(
    lookup: &dyn TypeView,
    entity: &str,
    field: &super::FieldModel,
) -> Option<jadpo_std::read::Column> {
    if field.json.is_some() || field.compound.is_some() || field.range_item.is_some() {
        return None;
    }
    if field
        .sql_type
        .as_deref()
        .is_some_and(|ty| ty.starts_with("vector"))
    {
        return None;
    }
    let ty = Ty::Field(entity.to_string(), vec![field.name.clone()]);
    if let Some(builtin) = lookup.representation(&ty) {
        return jadpo_std::read::Column::of_builtin(builtin);
    }
    match lookup.structure(&ty) {
        Ty::Named(name) => match lookup.def(&name) {
            Some(Def::Enum { variants }) if variants.iter().all(|v| v.fields.is_none()) => {
                Some(jadpo_std::read::Column::Enum {
                    ty: name.clone(),
                    variants: variants.iter().map(|v| v.name.clone()).collect(),
                })
            }
            _ => None,
        },
        _ => None,
    }
}

/// The latest migration's computed backfills that `jadpo migrate apply`
/// works out in Jadpo (chunk 5.6b2), each with the migration and field it
/// fills and where its decision is written: what the interpreter plans to
/// run. Only the latest migration's are checked against the project's
/// code, so only they are offered.
pub fn jadpo_backfills(db: &dyn Db, project: Project) -> Vec<JadpoBackfill> {
    let mut out = Vec::new();
    if project.layout(db) != Layout::Project {
        return out;
    }
    let set = migrations(db, project);
    let Some(migration) = set.list.last() else {
        return out;
    };
    let Some(after) = migration.schema() else {
        return out;
    };
    let before = set.before_latest();
    let path = migration.changes_path();
    let Some(file) = project
        .files(db)
        .iter()
        .copied()
        .find(|f| f.path(db) == &path)
    else {
        return out;
    };
    let Some(source) = tree::source(db, file) else {
        return out;
    };
    let (decisions, _) = read_decisions(&path, &source);
    let model = data_model(db, project);
    let lookup = Lookup::new(db, project);
    let decide = Decide {
        db,
        project,
        lookup: &lookup,
        model,
        decisions: &decisions,
        file: Some(file),
        before: &before,
    };
    for need in decisions_needed(&before, after) {
        let Some(decision) = decisions.iter().find(|d| d.key == need.key()) else {
            continue;
        };
        let DecisionValue::Backfill(value, span) = &decision.value else {
            continue;
        };
        if !matches!(need.kind, NeededKind::Backfill | NeededKind::Unreadable)
            || !decide.is_computed(&need, value)
        {
            continue;
        }
        let Some(computed) = decide.computed(decision, &need) else {
            continue;
        };
        if !computed.problems.is_empty() || computed.sql.is_some() {
            continue;
        }
        if let Some(plan) = computed.jadpo {
            out.push(JadpoBackfill {
                migration: migration.name.clone(),
                field: need.key(),
                file: path.clone(),
                line: file
                    .text(db)
                    .get(..span.start as usize)
                    .map_or(1, |text| text.matches('\n').count() as u32 + 1),
                plan,
            });
        }
    }
    out
}

/// The structured fields the latest migration reads or writes as JSON
/// in Jadpo (chunk 5.9c): those whose stored values it checks or
/// rewrites, since their type may no longer read them, and those a
/// backfill fills. `jadpo migrate apply` reads and writes each through
/// the route codec of the field's type now, which only the latest
/// migration is checked against.
pub fn stored_json_fields(db: &dyn Db, project: Project) -> Vec<StoredJson> {
    let mut out: Vec<StoredJson> = Vec::new();
    if project.layout(db) != Layout::Project {
        return out;
    }
    let set = migrations(db, project);
    let Some(migration) = set.list.last() else {
        return out;
    };
    let Some(after) = migration.schema() else {
        return out;
    };
    let before = set.before_latest();
    let model = data_model(db, project);
    for change in schema::changes(&before, after) {
        let (Change::AlterColumn {
            store,
            table,
            column,
        }
        | Change::AddColumn {
            store,
            table,
            column,
        }) = &change
        else {
            continue;
        };
        fn find<'s>(
            schema: &'s Schema,
            store: &str,
            table: &str,
            column: &str,
        ) -> Option<(&'s schema::Table, &'s schema::Column)> {
            let table = schema.stores.get(store)?.tables.get(table)?;
            Some((table, table.columns.get(column)?))
        }
        let was = find(&before, store, table, column).map(|(_, was)| was);
        let Some((now_table, now)) = find(after, store, table, column) else {
            continue;
        };
        let (Holds::Entity(entity), Some(field)) = (&now_table.holds, &now.field) else {
            continue;
        };
        let read = was.is_some_and(|was| jadpo_graph::migration::reads_less(was, now));
        let filled = was.is_none_or(|was| was.nullable && !now.nullable);
        if !(read || filled) || now.sql_type != "jsonb" {
            continue;
        }
        let Some(current) = model.entities.iter().find(|e| &e.born == entity) else {
            continue;
        };
        let Some(found) = current.fields.iter().find(|f| &f.born == field) else {
            continue;
        };
        if found.json.is_none() {
            continue;
        }
        let key = format!("{entity}.{field}");
        if out.iter().any(|known| known.field == key) {
            continue;
        }
        out.push(StoredJson {
            migration: migration.name.clone(),
            field: key,
            entity: current.name.clone(),
            name: found.name.clone(),
        });
    }
    out
}

/// A structured field the latest migration reads or writes as JSON in
/// Jadpo (chunk 5.9c).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredJson {
    /// The migration's folder name.
    pub migration: String,
    /// `Parcel.label`, by born names, as the migration's SQL names it.
    pub field: String,
    /// The entity and the field as code names them now.
    pub entity: String,
    pub name: String,
}

/// A backfill `jadpo migrate apply` works out in Jadpo (chunk 5.6b2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JadpoBackfill {
    /// The migration's folder name.
    pub migration: String,
    /// `Todo.priority`.
    pub field: String,
    /// Its `changes.jadpo`, and the line its value is written on.
    pub file: String,
    pub line: u32,
    pub plan: super::backfill::InJadpo,
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
                .reason("A migration folder is named by when `jadpo migrate new` made it and what it does, `20261020T1400_add_rating`, so the folders sort in the order they apply.")
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
    // A hand's `DROP` in a migration's SQL, which no acceptance would be
    // asked about since the snapshots don't show it (chunk V3).
    let empty = Schema::default();
    let mut previous = &empty;
    for migration in &set.list {
        let Some(schema) = migration.schema() else {
            continue;
        };
        if let Some(Ok(sql)) = &migration.sql {
            for statement in unexplained_removals(sql, previous, schema) {
                out.push(
                    Diagnostic::new(
                        &rule::MIGRATION_RECORD_INVALID,
                        format!("`{}` drops what its snapshot keeps", migration.sql_path()),
                    )
                    .at(file_span(&migration.sql_path()), "edited by hand")
                    .reason(format!(
                        "A migration's SQL is written by `jadpo migrate new` from its snapshots, and `{statement}` isn't what they say, so no person would be asked to accept what it removes."
                    ))
                    .next("Restore it from version control"),
                );
            }
        }
        previous = schema;
    }
    if !out.is_empty() {
        return out;
    }
    let model = data_model(db, project);
    let unknown_type = model
        .entities
        .iter()
        .any(|entity| entity.fields.iter().any(|field| field.ty.has_error()));
    // Schedules' tables have nowhere to go while no store is named for
    // them (`schedule.store_required`, chunk 6.2c).
    let schedules_placed = !*crate::schedules::declares_schedules(db, project)
        || crate::schedules::schedules_settings(db, project)
            .store
            .is_some();
    if unknown_type || !super::data_clean(db, project) || !schedules_placed {
        // The schema is partial until the entities and the queries, which
        // ask for indexes, check, while a field's type names nothing, and
        // while the schedules have no store: their own errors come first.
        return out;
    }
    let source = super::schema(db, project);
    let latest = set.latest_schema();
    let next = next_snapshot(&latest, source);
    if !next.same_but_reasons(&latest) {
        out.push(out_of_date(db, project, set, model, &latest, &next));
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
        let holds = |schema: &Schema| {
            schema
                .stores
                .get(store)
                .and_then(|s| s.tables.get(table?))
                .map(|t| matches!(t.holds, Holds::System(_)))
        };
        holds(next).or_else(|| holds(latest)).unwrap_or(true)
    };
    let mut changes: Vec<Change> = schema::changes(latest, next)
        .into_iter()
        .filter(|c| !matches!(c, Change::AddStore { .. }))
        .collect();
    changes.sort_by_key(system);
    changes
}

fn out_of_date(
    db: &dyn Db,
    project: Project,
    set: &Migrations,
    model: &DataModel,
    latest: &Schema,
    next: &Schema,
) -> Diagnostic {
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
            let holds = next
                .stores
                .get(store)
                .and_then(|s| s.tables.get(table?))
                .map(|t| t.holds.clone())?;
            let Holds::Entity(born) = holds else {
                return None;
            };
            let entity = model.entities.iter().find(|e| e.born == born)?;
            let read = super::entity_read(db, project, Name::new(db, entity.name.clone()));
            read.as_ref().map(|read| read.spans.name.clone())
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
                if let Some(Holds::Entity(name)) = after
                    .stores
                    .get(store)
                    .and_then(|s| s.tables.get(table))
                    .map(|t| &t.holds)
                {
                    // Its table is retired in another store: it moved.
                    let moved = after.stores.iter().any(|(other, kept)| {
                        other != store
                            && kept.tables.get(table).is_some_and(|t| {
                                t.retired && t.holds == Holds::Entity(name.clone())
                            })
                    });
                    let verb = if moved { "move" } else { "add" };
                    return format!("{verb}_{}", crate::case::to_snake(name));
                }
            }
            Change::RetireTable { table, .. } => return format!("remove_{table}"),
            Change::DropTable { table, .. } => return format!("drop_{table}"),
            // An entity's search (area 16, chunk B12), by its table.
            Change::AddColumn { table, column, .. } if column == jadpo_std::search::COLUMN => {
                return format!("search_{table}");
            }
            Change::RemoveColumn { table, column, .. } if column == jadpo_std::search::COLUMN => {
                return format!("unsearch_{table}");
            }
            Change::AlterColumn { table, column, .. } if column == jadpo_std::search::COLUMN => {
                return format!("change_{table}_search");
            }
            Change::AddColumn { column, .. } => return format!("add_{column}"),
            Change::RemoveColumn { column, .. } => return format!("drop_{column}"),
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
            Change::AddIndex { store, table, name } => {
                // An index a query needs is named after the query.
                let reason = after
                    .stores
                    .get(store)
                    .and_then(|s| s.tables.get(table))
                    .and_then(|t| t.indexes.get(name))
                    .map(|i| i.reason.as_str());
                return match reason.and_then(|r| r.strip_prefix("query ")) {
                    Some(query) => format!("index_{query}"),
                    None => format!("add_{name}"),
                };
            }
            Change::AddUnique { name, .. } => {
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
    let decide = Decide {
        db,
        project,
        lookup: &lookup,
        model,
        decisions: &decisions,
        file,
        before: &before,
    };
    for need in &needed {
        let key = need.key();
        let written = decisions.iter().find(|d| d.key == key);
        if matches!(
            written.map(|d| &d.value),
            None | Some(DecisionValue::Pending)
        ) {
            // A structured field's stored values are checked when no
            // `backfill:` rewrites them (chunk 5.9c).
            if need.kind.optional() {
                continue;
            }
            let span = written.map_or_else(|| at.clone(), |d| d.span.clone());
            let example = field_type(model, &need.entity, &need.field)
                .and_then(|ty| example_backfill(&lookup, ty));
            out.push(required(migration, need, span, example));
            continue;
        }
        let Some(decision) = written else {
            continue;
        };
        // A computed backfill is checked as a body (chunk 5.6b).
        if let DecisionValue::Backfill(value, span) = &decision.value
            && decide.is_computed(need, value)
        {
            out.extend(computed_problems(&decide, need, decision, span));
            continue;
        }
        if let Some((span, reason)) = misfit(&lookup, model, need, decision) {
            out.push(
                Diagnostic::new(
                    &rule::MIGRATION_DECISION_INVALID,
                    format!("The decision about `{key}` doesn't fit its change"),
                )
                .at(span, "doesn't fit")
                .reason(reason)
                .next(match need.kind {
                    NeededKind::Backfill | NeededKind::Refit | NeededKind::Unreadable => {
                        format!("Write `{key}: {{ backfill: <value> }}` with a value of its type")
                    }
                    NeededKind::Retire => format!("Write `{key}: FieldChange.retired`"),
                    NeededKind::DropField => format!("Write `{key}: FieldChange.dropped`"),
                    NeededKind::RetireEntity => format!("Write `{key}: EntityChange.retired`"),
                    NeededKind::DropEntity => format!("Write `{key}: EntityChange.dropped`"),
                }),
            );
        }
    }
    for decision in &decisions {
        if needed.iter().any(|n| n.key() == decision.key) {
            continue;
        }
        // A drop isn't written by hand: the migration has to leave the
        // column or table out of its snapshot and hold the statement.
        if matches!(
            decision.value,
            DecisionValue::Change(FieldChange::Dropped)
                | DecisionValue::Entity(EntityChange::Dropped)
        ) {
            out.push(
                Diagnostic::new(
                    &rule::MIGRATION_DECISION_UNKNOWN,
                    format!("`{}` isn't dropped by this migration", decision.key),
                )
                .at(decision.span.clone(), "no such change")
                .reason(format!(
                    "`{}` still has `{}` in its snapshot, so nothing here drops it: a drop comes from a migration of its own, which writes this line, its snapshot and its SQL.",
                    migration.name, decision.key
                ))
                .next(format!(
                    "Remove the line, then run `jadpo migrate new <purpose> --drop {}` once the migration is finished",
                    decision.key
                )),
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
                "A migration's `changes.jadpo` holds only the decisions its own changes need, and nothing `{}` changes about `{}` does: a decision is a value for existing rows of a required field or for rows a field no longer allows, retiring a field or an entity that is gone, or dropping one that is retired.",
                migration.name, decision.key
            ))
            .next(format!("Remove `{}`", decision.key)),
        );
    }
    if !out.is_empty() {
        return out;
    }
    // Every decision is made: the SQL must hold them. A change no SQL is
    // written for can't have come from `jadpo migrate new`.
    let decided = |need: &Needed| decide.decided(need);
    let sql = match migration_sql(&before, after, &decided) {
        Ok(sql) => sql,
        Err(refusals) => {
            let said: Vec<String> = refusals.iter().map(|r| r.change.clone()).collect();
            out.push(record_invalid(
                &migration.snapshot_path(),
                &format!(
                    "`jadpo migrate new` writes no migration for {}",
                    crate::and_list(&said)
                ),
            ));
            return out;
        }
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

/// What is wrong with a computed backfill (area 5, "Computed backfills";
/// chunk 5.6b): its body's problems, or that it can't run yet, for a
/// field that allows less, or with an operation that has no SQL form
/// reading or filling a value `jadpo migrate apply` can't yet work out in
/// Jadpo (chunk 5.6b2).
fn computed_problems(
    decide: &Decide<'_>,
    need: &Needed,
    decision: &Decision,
    span: &Span,
) -> Vec<Diagnostic> {
    let key = need.key();
    if need.kind == NeededKind::Refit {
        return vec![
            Diagnostic::new(
                &rule::TOOLCHAIN_NOT_BUILT_YET,
                format!("A backfill worked out from each record for `{key}` isn't built yet"),
            )
            .at(span.clone(), "worked out from each record")
            .reason(format!(
                "`{key}` allows less than it did, and this compiler replaces the values it no longer allows only with one value written out so far."
            ))
            .next(format!("Write `{key}: {{ backfill: <value> }}` with one value of its type")),
        ];
    }
    let Some(computed) = decide.computed(decision, need) else {
        return Vec::new();
    };
    if !computed.problems.is_empty() {
        return computed.problems;
    }
    if computed.sql.is_some() || computed.jadpo.is_some() {
        return Vec::new();
    }
    vec![
        Diagnostic::new(
            &rule::TOOLCHAIN_NOT_BUILT_YET,
            format!("Working out the backfill for `{key}` in Jadpo isn't built yet"),
        )
        .at(span.clone(), "no SQL form")
        .reason(format!(
            "Something in this backfill has no SQL form (area 19), so `jadpo migrate apply` would work its values out in Jadpo, row by row; it reads values held in one column, and fills those or a structured value held as JSON, and `{key}` is held in several columns or reads a field held so or as JSON."
        ))
        .next(format!(
            "Work `{key}`'s value out with operators, `if`, `match` and standard operations that have a SQL form for now"
        )),
    ]
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
        (
            DecisionValue::Backfill(value, span),
            NeededKind::Backfill | NeededKind::Refit | NeededKind::Unreadable,
        ) => {
            field_type(model, &need.entity, &need.field)
                .and_then(|ty| backfill_problem(lookup, ty, value))
                .map(|why| (span.clone(), format!("This value can't fill `{key}`: {why}.")))
        }
        (DecisionValue::Change(FieldChange::Retired), NeededKind::Retire)
        | (DecisionValue::Change(FieldChange::Dropped), NeededKind::DropField)
        | (DecisionValue::Entity(EntityChange::Retired), NeededKind::RetireEntity)
        | (DecisionValue::Entity(EntityChange::Dropped), NeededKind::DropEntity) => None,
        (DecisionValue::Entity(EntityChange::Dropped), NeededKind::RetireEntity) => Some((
            decision.span.clone(),
            "An entity is removed in two migrations: `EntityChange.retired` first, and `EntityChange.dropped` in a later one.".to_string(),
        )),
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
        (_, NeededKind::Refit) => Some((
            decision.span.clone(),
            format!("`{key}` allows less than it did, so its decision is `{{ backfill: <value> }}`, the value stored records it no longer allows get."),
        )),
        (_, NeededKind::Unreadable) => Some((
            decision.span.clone(),
            format!("`{key}`'s type may not read every value stored before, so its decision, which may be left out, is `{{ backfill: <value> }}`, the value stored records it doesn't read get."),
        )),
        (_, NeededKind::DropField) => Some((
            decision.span.clone(),
            format!("This migration drops `{key}`'s column, so its decision is `FieldChange.dropped`."),
        )),
        (_, NeededKind::RetireEntity) => Some((
            decision.span.clone(),
            format!("`{key}` is gone, so its decision is `EntityChange.retired`."),
        )),
        (_, NeededKind::DropEntity) => Some((
            decision.span.clone(),
            format!("This migration drops `{key}`'s table, so its decision is `EntityChange.dropped`."),
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
        NeededKind::Refit => (
            format!(
                "`{key}` allows less than it did (a variant gone, or a tighter rule), and `{}` records already stored may hold a value it no longer allows; they need one it does.",
                need.entity
            ),
            match example {
                Some(example) => {
                    format!(
                        "Set `backfill:` for `{key}`, the value those records get, such as `backfill: {example}`"
                    )
                }
                None => format!("Set `backfill:` for `{key}`, the value those records get"),
            },
        ),
        // Never required: its stored values are checked instead.
        NeededKind::Unreadable => (
            format!(
                "`{key}`'s type may not read every value stored before, and `{}` records already stored may hold one it doesn't read.",
                need.entity
            ),
            format!("Set `backfill:` for `{key}`, the value those records get"),
        ),
        NeededKind::Retire => (
            format!(
                "`{key}` is gone from `{}`. Retiring its column keeps the column and its data while code stops using it; if the field was renamed instead, give it back its old name, since a rename that keeps the column comes with `jadpo rename`.",
                need.entity
            ),
            format!("Write `{key}: FieldChange.retired`"),
        ),
        NeededKind::DropField => (
            format!("This migration drops `{key}`'s column, and its data with it, so it says so."),
            format!("Write `{key}: FieldChange.dropped`"),
        ),
        NeededKind::RetireEntity => (
            format!(
                "`{key}` is gone. Retiring its table keeps the table and its records while code stops using them, out of the app's reach until a later migration drops them; if the entity was renamed instead, give it back its old name, since a rename that keeps the table comes with `jadpo rename`."
            ),
            format!("Write `{key}: EntityChange.retired`"),
        ),
        NeededKind::DropEntity => (
            format!(
                "This migration drops `{key}`'s table, and its records with it, so it says so."
            ),
            format!("Write `{key}: EntityChange.dropped`"),
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

/// Each store's steps, in the order they apply: every migration's section
/// for that store, as `migrate apply` and the test database hand them to
/// the runtime. Migrations whose records can't be read give none, and
/// `jadpo check` reports them.
pub fn store_steps(set: &Migrations) -> BTreeMap<String, Vec<jadpo_std::Migration>> {
    let mut out: BTreeMap<String, Vec<jadpo_std::Migration>> = BTreeMap::new();
    for migration in &set.list {
        let Some(Ok(sql)) = &migration.sql else {
            continue;
        };
        for section in jadpo_graph::migration::sections(sql) {
            out.entry(section.store)
                .or_default()
                .push(jadpo_std::Migration {
                    name: migration.name.clone(),
                    digest: jadpo_graph::migration::section_digest(&section.sql),
                    sql: section.sql,
                });
        }
    }
    out
}
