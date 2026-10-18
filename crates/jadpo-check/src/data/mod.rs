//! Area 5's rules on entities, and the physical schema they ask for
//! (docs/compiler-runtime.md §2.2): an entity's `id`, its fields and their
//! settings (`generated:`, `unique:`, `on_delete:`), its references, its
//! `unique:` and `indexes:` sections, the store it lives in, and the
//! tables, columns, keys and indexes that follow, with the system tables
//! each store holds.
//!
//! Each entity is read by name ([`entity_read`]) into a model without
//! positions ([`entity_model`]) and the problems found in it, so an edit
//! reads again only the entity it is in, and what reads that entity
//! (docs/compiler-runtime.md §2.4) runs again only when its model changes.
//! What crosses entities, references to another store and cycles of
//! required references, is worked out from the models ([`entity_links`]);
//! [`data_problems`] gives one file's share, and [`schema`] the schema
//! `jadpo migrate new` records and `jadpo check` compares with the latest
//! migration ([`migrations`]).

pub mod backfill;
mod columns;
pub mod lifecycle;
pub mod migrations;
pub mod queries;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use jadpo_db::{Db, File, ItemKind, Project, item_tree, symbols};
use jadpo_diag::{Diagnostic, Span, nearest, rule};
use jadpo_graph::Kind;
use jadpo_graph::schema::{self as physical, MAX_NAME, Schema};
use jadpo_std::{Driver, Generated, OnDelete};
use jadpo_syntax::ast::{self, AstNode, Decl, Expr, MemberItem};

use crate::case::to_snake;
use crate::graph::{entity_field_key, registry};
use crate::lookups::{Lookup, Name, NameView};
use crate::tree::{self, node_span};
use crate::types::{Const, Def, FieldsBase, Ty, TypeRef, TypeView};

pub use columns::column_type;

/// The sections an entity may hold besides its fields (area 5, "Where
/// things live"), with area 9's `membership:`. `policy:` and `membership:`
/// are read by [`crate::policy`]; `caches:` comes with Redis.
pub const SECTIONS: [&str; 7] = [
    "store",
    "unique",
    "indexes",
    "lifecycle",
    "caches",
    "policy",
    "membership",
];

/// The store an entity lives in when its file names none (area 5, "Stores").
pub const MAIN: &str = "main";

/// What a connection in `app.jadpo` is, as far as storing entities goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ConnectionKind {
    Postgres,
    Redis,
    /// A provider package (area 11).
    Package,
    /// Neither a built-in driver nor a package: area 11 reports it.
    Other,
}

/// The connections `application { connections: { } }` declares, and
/// whether `main` on PostgreSQL is implied because none of them is a
/// database (area 5, "Stores").
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Connections {
    pub declared: Vec<(String, ConnectionKind)>,
    pub implied_main: bool,
}

impl Connections {
    pub fn kind(&self, name: &str) -> Option<ConnectionKind> {
        if self.implied_main && name == MAIN {
            return Some(ConnectionKind::Postgres);
        }
        self.declared
            .iter()
            .find(|(declared, _)| declared == name)
            .map(|(_, kind)| *kind)
    }

    /// The PostgreSQL stores, by name.
    pub fn databases(&self) -> Vec<String> {
        let mut out: Vec<String> = self
            .declared
            .iter()
            .filter(|(_, kind)| *kind == ConnectionKind::Postgres)
            .map(|(name, _)| name.clone())
            .collect();
        if self.implied_main {
            out.push(MAIN.to_string());
        }
        out.sort();
        out
    }
}

/// The connections declared in `app.jadpo`.
#[salsa::tracked(returns(ref))]
pub fn connections(db: &dyn Db, project: Project) -> Connections {
    let mut declared = Vec::new();
    let app = project
        .files(db)
        .iter()
        .find(|file| file.path(db) == jadpo_db::PROJECT_FILE);
    if let Some(source) = app.and_then(|file| tree::source(db, *file)) {
        for decl in source.decls() {
            let Decl::SingletonDecl(singleton) = decl else {
                continue;
            };
            if singleton.keyword().map(|t| t.text().to_string()).as_deref() != Some("application") {
                continue;
            }
            for (key, member) in tree::members(singleton.member_list()) {
                if tree::name_text(&key).as_deref() != Some("connections") {
                    continue;
                }
                for (name, entry) in tree::members(member.block()) {
                    let Some(name) = tree::name_text(&name) else {
                        continue;
                    };
                    declared.push((name, connection_kind(entry.block())));
                }
            }
        }
    }
    let implied_main = !declared
        .iter()
        .any(|(_, kind)| *kind == ConnectionKind::Postgres);
    Connections {
        declared,
        implied_main,
    }
}

fn connection_kind(list: Option<ast::MemberList>) -> ConnectionKind {
    for (setting, member) in tree::members(list) {
        match tree::name_text(&setting).as_deref() {
            Some("package") => return ConnectionKind::Package,
            Some("driver") => {
                let names = member.value().as_ref().map(path_names).unwrap_or_default();
                return match names.as_slice() {
                    [host, variant] if host == Driver::NAME => match Driver::from_name(variant) {
                        Some(Driver::Postgres) => ConnectionKind::Postgres,
                        Some(Driver::Redis) => ConnectionKind::Redis,
                        None => ConnectionKind::Other,
                    },
                    _ => ConnectionKind::Other,
                };
            }
            _ => {}
        }
    }
    ConnectionKind::Other
}

/// `a.b.c` as its names; empty for anything else.
fn path_names(expr: &Expr) -> Vec<String> {
    match tree::dotted(expr) {
        Some((first, rest)) => std::iter::once(first)
            .chain(rest)
            .filter_map(|name| tree::name_ref_text(&name))
            .collect(),
        None => Vec::new(),
    }
}

/// One entity as area 5 reads it, without positions: a comment or a blank
/// line moves none of it, so what reads it is checked again only when the
/// entity itself changes ([`EntitySpans`] says where its parts are).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntityModel {
    pub name: String,
    /// Its born name, which its table is named after.
    pub born: String,
    pub path: String,
    /// The store it lives in; `None` when its `store:` is wrong.
    pub store: Option<String>,
    pub table: String,
    pub fields: Vec<FieldModel>,
    pub unique: Vec<ListRule>,
    pub indexes: Vec<ListRule>,
    /// Its `lifecycle:` (chunk 3.4), when it has one.
    pub lifecycle: Option<lifecycle::LifecycleModel>,
}

/// One field of an entity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldModel {
    pub name: String,
    pub born: String,
    pub ty: Ty,
    pub nullable: bool,
    /// The column type; `None` when the field can't be stored.
    pub sql_type: Option<String>,
    pub unique: bool,
    pub generated: Option<Generated>,
    pub reference: Option<Reference>,
    /// `immutable: true`: no ordinary update changes it (area 9).
    pub immutable: bool,
    /// What its type allows beyond its column type, which a migration
    /// snapshot records.
    pub allows: physical::Allows,
    /// A compound standard type stored in more than one column, by name
    /// (area 19): `Money`, in `<column>_amount`, a `bigint` of minor units
    /// that `sql_type` is, and `<column>_currency`, the currency's code.
    pub compound: Option<&'static str>,
    /// For a `Range<T>` field, the built-in type its ends are built on
    /// (chunk 5.5w): `Duration` for a `Range<Duration>`.
    pub range_item: Option<&'static str>,
    /// Of a compound type's columns, those its type fixes to one currency,
    /// each by its suffix with that currency's code (chunk 5.5w): a
    /// `Money` type's `currency:`, a rate type's `from:` and `to:`.
    pub fixed: Vec<(&'static str, String)>,
    /// The comment on its column, for a quantity: its kind and the SI unit
    /// it holds (area 19, chunk 5.5w). A compound type's columns take
    /// theirs from its layout.
    pub comment: Option<String>,
    /// A JSON-held standard type, by name (area 19, chunk 5.5w2b):
    /// `RichText`, `RichTextBlock` or `LocalisedText`, stored as its
    /// contract's JSON in the one `jsonb` column `sql_type` is, written and
    /// read by the route codec.
    pub json: Option<&'static str>,
    /// The built-in type its type is built on, which a literal compared
    /// with it or backfilled into it is decoded for, as the plan decodes a
    /// body's (chunk 5.5w3b); `None` for an enum, an object or a container.
    pub builtin: Option<jadpo_std::Builtin>,
    /// The places its type's values keep, for a decimal.
    pub scale: Option<u32>,
}

impl FieldModel {
    /// Its column as a condition or a literal written into SQL sees it.
    pub fn column(&self) -> jadpo_sql::Column<'_> {
        jadpo_sql::Column {
            name: &self.born,
            nullable: self.nullable,
            ty: sql_type(self),
            builtin: self.builtin,
            scale: self.scale,
        }
    }

    /// The data entry of the compound standard type the field holds, if it
    /// holds one.
    pub fn compound_entry(&self) -> Option<&'static jadpo_std::StandardType> {
        self.compound.and_then(jadpo_std::standard_type)
    }

    /// The compound type's name as a read takes it: its entry's, or a
    /// range's with its item, `Range<Duration>` (chunk 5.5w).
    pub fn compound_type(&self) -> String {
        match (self.compound, self.range_item) {
            (Some(name), Some(item)) => format!("{name}<{item}>"),
            (Some(name), None) => name.to_string(),
            (None, _) => String::new(),
        }
    }

    /// The columns the field is stored in, by name, each with its type: one,
    /// or a compound type's, its column suffixes joined to the field's
    /// column name with `_`, in its entry's order; a suffix of `""` is the
    /// field's own column, of the type it was given (a range's, by its
    /// item, chunk 5.5w).
    pub fn columns(&self) -> Vec<(String, String)> {
        let Some(sql_type) = &self.sql_type else {
            return Vec::new();
        };
        match self.compound_entry() {
            Some(entry) => entry
                .columns()
                .iter()
                .map(|(part, ty)| {
                    if part.is_empty() {
                        (self.born.clone(), sql_type.clone())
                    } else {
                        (format!("{}_{part}", self.born), (*ty).to_string())
                    }
                })
                .collect(),
            None => vec![(self.born.clone(), sql_type.clone())],
        }
    }
}

/// Whether a field keeps a vector: a vector type's column, or an
/// `Embedding`'s vector column (area 21, chunk 5.5v1).
pub fn holds_vector(field: &FieldModel) -> bool {
    field
        .columns()
        .iter()
        .any(|(_, sql_type)| sql_type.split('(').next() == Some("vector"))
}

/// A reference: the entity a field typed `X.id` points at.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Reference {
    pub target: String,
    /// `None` when the field doesn't say, which is an error.
    pub on_delete: Option<OnDelete>,
}

/// A compound unique rule or a declared index.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListRule {
    pub name: String,
    /// The fields it names that exist, in order.
    pub fields: Vec<String>,
}

/// Where an entity's parts are written: what a problem points at.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntitySpans {
    /// Its name.
    pub name: Span,
    /// Each field's key, in the order of [`EntityModel::fields`].
    pub fields: Vec<Span>,
}

/// One entity read from its file: its model, where its parts are, and the
/// problems found in it alone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntityRead {
    pub model: Arc<EntityModel>,
    pub spans: EntitySpans,
    pub problems: Vec<Diagnostic>,
}

/// Every entity, for what reads them all: the schema, the migrations and
/// the plan.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DataModel {
    pub entities: Vec<EntityModel>,
}

impl DataModel {
    pub fn entity(&self, name: &str) -> Option<&EntityModel> {
        self.entities.iter().find(|entity| entity.name == name)
    }
}

/// The names of the project's entities, in name order.
#[salsa::tracked(returns(ref))]
pub fn entity_names(db: &dyn Db, project: Project) -> Vec<String> {
    symbols(db, project)
        .declarations
        .iter()
        .filter(|(_, found)| found.iter().any(|s| s.kind == ItemKind::Entity))
        .map(|(name, _)| name.clone())
        .collect()
}

/// The entity called `name`, read from the first file that declares one:
/// a query of its own, which reads that file and the names the entity
/// uses, so an edit elsewhere never reads it again.
#[salsa::tracked(returns(ref))]
pub fn entity_read<'db>(db: &'db dyn Db, project: Project, name: Name<'db>) -> Option<EntityRead> {
    let lookup = Lookup::new(db, project);
    let symbol = lookup
        .declared(name.text(db))
        .iter()
        .find(|s| s.kind == ItemKind::Entity)?;
    let file = crate::lookups::file_at(db, project, Name::new(db, symbol.path.clone()))?;
    let source = tree::source(db, file)?;
    let Some(Decl::EntityDecl(decl)) = tree::decl_at(&source, symbol.index) else {
        return None;
    };
    let mut reader = Reader {
        lookup: &lookup,
        registry: registry(db, project),
        connections: connections(db, project),
        spans: EntitySpans {
            name: Span::new(&symbol.path, 0, 0),
            fields: Vec::new(),
        },
        out: Vec::new(),
    };
    let model = reader.entity(&symbol.path, &decl)?;
    Some(EntityRead {
        model: Arc::new(model),
        spans: reader.spans,
        problems: reader.out,
    })
}

/// The entity called `name`, without positions: what every check that
/// reads an entity depends on.
#[salsa::tracked(returns(ref))]
pub fn entity_model<'db>(
    db: &'db dyn Db,
    project: Project,
    name: Name<'db>,
) -> Option<Arc<EntityModel>> {
    entity_read(db, project, name)
        .as_ref()
        .map(|read| Arc::clone(&read.model))
}

/// Whether the entity called `name` read without a problem.
#[salsa::tracked(returns(copy))]
fn entity_clean<'db>(db: &'db dyn Db, project: Project, name: Name<'db>) -> bool {
    entity_read(db, project, name)
        .as_ref()
        .is_none_or(|read| read.problems.is_empty())
}

/// The project's entities, read by name: each read is the dependency of
/// the check that makes it on that one entity.
#[derive(Clone, Copy)]
pub struct Entities<'db> {
    db: &'db dyn Db,
    project: Project,
}

impl std::fmt::Debug for Entities<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Entities").finish_non_exhaustive()
    }
}

impl<'db> Entities<'db> {
    pub fn new(db: &'db dyn Db, project: Project) -> Entities<'db> {
        Entities { db, project }
    }

    pub fn get(&self, name: &str) -> Option<&'db EntityModel> {
        entity_model(self.db, self.project, Name::new(self.db, name.to_string())).as_deref()
    }

    /// Every entity: for suggestions on a mistake, since reading them all
    /// depends on every entity.
    pub fn all(&self) -> &'db DataModel {
        data_model(self.db, self.project)
    }
}

/// Every entity, without positions, in name order.
#[salsa::tracked(returns(ref))]
pub fn data_model(db: &dyn Db, project: Project) -> Arc<DataModel> {
    let entities = Entities::new(db, project);
    Arc::new(DataModel {
        entities: entity_names(db, project)
            .iter()
            .filter_map(|name| entities.get(name).cloned())
            .collect(),
    })
}

/// Whether every entity and every query read without a problem, so the
/// schema they ask for is whole. It holds no positions, so a comment
/// doesn't compare the migrations again.
#[salsa::tracked(returns(copy))]
pub fn data_clean(db: &dyn Db, project: Project) -> bool {
    let entities = entity_names(db, project)
        .iter()
        .all(|name| entity_clean(db, project, Name::new(db, name.clone())));
    let links = entity_links(db, project);
    entities
        && links
            .values()
            .all(|links| links.across.is_empty() && links.cycles.is_empty())
        && queries::queries_clean(db, project)
}

/// Area 5's problems in one file: those of the entities it declares, and
/// of their references to other entities.
#[salsa::tracked(returns(ref))]
pub fn data_problems(db: &dyn Db, project: Project, file: File) -> Vec<Diagnostic> {
    let path = file.path(db);
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    for item in &item_tree(db, file).items {
        if item.kind != ItemKind::Entity || !seen.insert(item.name.as_str()) {
            continue;
        }
        let name = Name::new(db, item.name.clone());
        let Some(read) = entity_read(db, project, name) else {
            continue;
        };
        if read.model.path != *path {
            continue;
        }
        out.extend(read.problems.iter().cloned());
        out.extend(link_problems(
            db,
            project,
            read,
            links_of(db, project, name),
        ));
    }
    out
}

struct Reader<'a> {
    lookup: &'a Lookup<'a>,
    registry: &'a jadpo_graph::Registry,
    connections: &'a Connections,
    spans: EntitySpans,
    out: Vec<Diagnostic>,
}

impl Reader<'_> {
    fn entity(&mut self, path: &str, decl: &ast::EntityDecl) -> Option<EntityModel> {
        let name_node = decl.name()?;
        let name = tree::name_text(&name_node)?;
        let name_span = node_span(path, name_node.syntax());
        let born = self.registry.born(Kind::Entity, &name, None);
        let table = to_snake(&born);
        self.spans.name = name_span.clone();
        let mut entity = EntityModel {
            name: name.clone(),
            born,
            path: path.to_string(),
            store: Some(MAIN.to_string()),
            table: table.clone(),
            fields: Vec::new(),
            unique: Vec::new(),
            indexes: Vec::new(),
            lifecycle: None,
        };
        let mut store: Option<(String, Span)> = None;
        let mut lifecycle = None;
        let mut unique = None;
        let mut indexes = None;
        let items: Vec<MemberItem> = decl
            .member_list()
            .iter()
            .flat_map(|list| list.member_items())
            .collect();
        for item in &items {
            let member = match item {
                MemberItem::Member(member) => member,
                MemberItem::Decl(inner) => {
                    self.unknown_section(path, inner.syntax(), &head(inner.syntax()), &name);
                    continue;
                }
                MemberItem::BareMember(bare) => {
                    self.unknown_section(path, bare.syntax(), &tree::written(bare.syntax()), &name);
                    continue;
                }
                MemberItem::RouteEntry(entry) => {
                    self.unknown_section(path, entry.syntax(), &head(entry.syntax()), &name);
                    continue;
                }
                // A rule here is reported where it is written (area 2).
                MemberItem::Elision(_) | MemberItem::RuleMember(_) => continue,
            };
            let Some(key) = member.member_key() else {
                continue;
            };
            let names: Vec<String> = key.names().filter_map(|n| tree::name_text(&n)).collect();
            let key_span = node_span(path, key.syntax());
            let word = match names.as_slice() {
                [word] => word.clone(),
                _ => {
                    self.unknown_section(path, key.syntax(), &names.join("."), &name);
                    continue;
                }
            };
            match word.as_str() {
                "store" => store = Some(self.store_value(path, member, &key_span)),
                "unique" => unique = Some(member.clone()),
                "indexes" => indexes = Some(member.clone()),
                "lifecycle" => lifecycle = Some(member.clone()),
                // Read by `crate::policy` (area 9). A record cache needs
                // the Redis driver, which no chunk builds yet.
                "policy" | "membership" => {}
                "caches" => self.out.push(
                    Diagnostic::new(
                        &rule::TOOLCHAIN_NOT_BUILT_YET,
                        "An entity's `caches:` isn't built yet",
                    )
                    .at(key_span, "decided, not built")
                    .reason("A record cache keeps copies in a Redis store (area 5), and this compiler builds no Redis driver yet, so an entity using it couldn't run.")
                    .next("Leave `caches:` out for now"),
                ),
                _ => match member.value() {
                    Some(value) if crate::types::looks_like_type(&value) => {
                        if let Some(field) = self.field(path, &name, &word, &key_span, &value) {
                            entity.fields.push(field);
                            self.spans.fields.push(key_span.clone());
                        }
                    }
                    _ => self.unknown_section(path, key.syntax(), &word, &name),
                },
            }
        }
        // The identity (area 5, "Entities and the `id`").
        match entity.fields.iter().position(|f| f.name == "id") {
            None => self.out.push(
                Diagnostic::new(
                    &rule::DATA_IDENTITY_MISSING,
                    format!("`{name}` has no `id` field"),
                )
                .at(name_span.clone(), "no `id`")
                .reason("Every entity's identity is one field, `id`, based on `Uuid`: generated when a record is created and never supplied by callers, so other code can refer to it as `Entity.id`.")
                .next("Add `id: Uuid` as the entity's first field"),
            ),
            Some(at) => {
                let id = &entity.fields[at];
                let uuid = self.lookup.representation(&id.ty)
                    == Some(jadpo_std::Builtin::Uuid)
                    && !matches!(id.ty.strip_nullable(), Ty::Field(..));
                let problem = if id.nullable {
                    Some(("can be `none`", "An identity always exists, so `id` can't be optional."))
                } else if !uuid {
                    Some(("not a `Uuid`", "An entity's `id` is a `Uuid`, or a named type built on one; links between entities are references to it."))
                } else if id.unique || id.generated.is_some() || id.reference.is_some() {
                    Some(("has data settings", "`id` is always unique and generated on create, so it takes no `unique:`, `generated:` or `on_delete:`."))
                } else {
                    None
                };
                if let Some((label, reason)) = problem {
                    self.out.push(
                        Diagnostic::new(
                            &rule::DATA_IDENTITY_TYPE,
                            format!("`{name}.id` must be a required `Uuid`"),
                        )
                        .at(self.spans.fields[at].clone(), label)
                        .reason(reason)
                        .next("Write `id: Uuid`"),
                    );
                }
            }
        }
        if let Some(member) = unique {
            entity.unique = self.rules(path, &entity, &member, true);
        }
        if let Some(member) = indexes {
            entity.indexes = self.rules(path, &entity, &member, false);
        }
        let field_spans = self.spans.fields.clone();
        entity.store = self.placement(store, &name, &name_span, (&entity.fields, &field_spans));
        if let Some(member) = lifecycle {
            let lookup = self.lookup;
            entity.lifecycle = lifecycle::read(
                lookup,
                &|name| lookup.setting(name).cloned(),
                path,
                &entity,
                &self.spans,
                &member,
                &mut self.out,
            );
        }
        // Names the database keeps (area 5, "Storage types").
        if table.starts_with("jadpo_") {
            self.out.push(
                Diagnostic::new(
                    &rule::DATA_RESERVED_TABLE_NAME,
                    format!("`{name}`'s table would be called `{table}`"),
                )
                .at(name_span.clone(), "a system table's prefix")
                .reason("Tables whose names start with `jadpo_` are the runtime's own (the outbox, event deliveries, idempotency keys), so no entity's table may take that prefix.")
                .next(format!("Rename `{name}`")),
            );
        } else if table.len() > MAX_NAME {
            self.too_long(&name_span, &name, &table, "table");
        }
        let spans = self.spans.fields.clone();
        for (field, span) in entity.fields.iter().zip(&spans) {
            if field.born.len() > MAX_NAME {
                self.too_long(
                    span,
                    &format!("{name}.{}", field.name),
                    &field.born,
                    "column",
                );
            }
        }
        Some(entity)
    }

    fn too_long(&mut self, span: &Span, what: &str, physical: &str, kind: &str) {
        self.out.push(
            Diagnostic::new(
                &rule::DATA_NAME_TOO_LONG,
                format!("`{what}` is too long a name for a {kind}"),
            )
            .at(span.clone(), format!("{} characters", physical.len()))
            .reason(format!(
                "Its {kind} would be called `{physical}`, and PostgreSQL keeps names of at most {MAX_NAME} bytes, so a longer one would be cut and could clash with another."
            ))
            .next("Choose a shorter name"),
        );
    }

    fn unknown_section(
        &mut self,
        path: &str,
        node: &jadpo_syntax::SyntaxNode,
        written: &str,
        entity: &str,
    ) {
        let span = node_span(path, node);
        let near = nearest(written, SECTIONS.iter().copied());
        let mut problem = Diagnostic::new(
            &rule::DATA_UNKNOWN_SECTION,
            format!("`{written}` isn't something an entity holds"),
        )
        .at(span.clone(), "not a field or a section");
        let reason = "An entity's file holds only its data definition: fields (`name: Type`), and the sections `store:`, `unique:`, `indexes:`, `lifecycle:`, `caches:`, `policy:` and `membership:`. Its identity is always the field `id`; actions, queries and failures go in the domain's own folders.";
        problem = match near {
            Some(near) if near != written => problem
                .reason(format!("{reason} Did you mean `{near}`?"))
                .next(format!("Use `{near}`")),
            _ => problem.reason(reason).next(format!(
                "Remove it from `{entity}`, or move it where it belongs"
            )),
        };
        self.out.push(problem);
    }

    /// `store: name`, as written, with where.
    fn store_value(&mut self, path: &str, member: &ast::Member, key_span: &Span) -> (String, Span) {
        match member.value() {
            Some(value) => {
                let span = node_span(path, value.syntax());
                match value {
                    Expr::NameRef(name) => (tree::name_ref_text(&name).unwrap_or_default(), span),
                    other => (tree::written(other.syntax()), span),
                }
            }
            None => (String::new(), key_span.clone()),
        }
    }

    /// The store an entity lives in, checked against the connections.
    fn placement(
        &mut self,
        store: Option<(String, Span)>,
        entity: &str,
        at: &Span,
        (fields, field_spans): (&[FieldModel], &[Span]),
    ) -> Option<String> {
        let databases = self.connections.databases();
        let Some((name, span)) = store else {
            if self.connections.kind(MAIN) == Some(ConnectionKind::Postgres) {
                return Some(MAIN.to_string());
            }
            self.out.push(
                Diagnostic::new(
                    &rule::DATA_STORE_REQUIRED,
                    format!("`{entity}` names no store, and there is no `main`"),
                )
                .at(at.clone(), "which store?")
                .reason(format!(
                    "An entity lives in `main` unless its file says `store: <connection>`, and `app.jadpo` declares no database called `main`; its databases are {}.",
                    crate::and_list(&databases.iter().map(|d| format!("`{d}`")).collect::<Vec<_>>())
                ))
                .next(match databases.first() {
                    Some(first) => format!("Add `store: {first}` to `{entity}`"),
                    None => format!("Add `store: <connection>` to `{entity}`"),
                }),
            );
            return None;
        };
        match self.connections.kind(&name) {
            Some(ConnectionKind::Postgres) => Some(name),
            // A vector needs pgvector, which only PostgreSQL has (area 21,
            // "Without pgvector"): said before Redis entities' own gap,
            // since it stays when they are built.
            Some(ConnectionKind::Redis) if let Some(at) = fields.iter().position(holds_vector) => {
                let field = &fields[at];
                self.out.push(
                    Diagnostic::new(
                        &rule::DATA_VECTOR_STORE_UNSUPPORTED,
                        format!("`{entity}.{}` is a vector, which `{name}` can't store", field.name),
                    )
                    .at(field_spans[at].clone(), "a vector, in Redis")
                    .reason(format!(
                        "A vector is kept in a column of pgvector, an extension of PostgreSQL, and `{name}` is a Redis connection. There is no fallback, since a search held anywhere else would behave differently from the one that was tested."
                    ))
                    .next(if self.connections.kind(MAIN) == Some(ConnectionKind::Postgres) {
                        format!("Remove `store: {name}` to keep `{entity}` in `main`, a PostgreSQL store")
                    } else if let Some(first) = databases.first() {
                        format!("Write `store: {first}` to keep `{entity}` in a PostgreSQL store")
                    } else {
                        format!("Declare a connection with `driver: Driver.postgres` in `app.jadpo`, and keep `{entity}` there")
                    }),
                );
                None
            }
            Some(ConnectionKind::Redis) => {
                self.out.push(
                    Diagnostic::new(
                        &rule::TOOLCHAIN_NOT_BUILT_YET,
                        format!("An entity stored in Redis (`{name}`) isn't built yet"),
                    )
                    .at(span, "a Redis connection")
                    .reason("An entity in Redis must declare its expiry (area 5), whose syntax is still an open question, and this compiler builds PostgreSQL stores only.")
                    .next(format!("Keep `{entity}` in a PostgreSQL store for now")),
                );
                None
            }
            found => {
                let what = match found {
                    Some(ConnectionKind::Package) => {
                        format!("`{name}` is a provider package's connection")
                    }
                    Some(_) => format!("`{name}` has no database driver"),
                    None => format!("`app.jadpo` declares no connection called `{name}`"),
                };
                let near = nearest(&name, databases.iter().map(String::as_str));
                let problem = Diagnostic::new(
                    &rule::DATA_STORE_UNKNOWN,
                    format!("`{name}` isn't a database `{entity}` can live in"),
                )
                .at(span.clone(), "not a database")
                .reason(format!(
                    "{what}; an entity's `store:` names a connection with `driver: Driver.postgres`, or `main` when `app.jadpo` declares no database."
                ));
                self.out.push(match near {
                    Some(near) => problem
                        .next(format!("Use `store: {near}`"))
                        .edit(span, near),
                    None => {
                        problem.next("Name a database connection, or declare one in `app.jadpo`")
                    }
                });
                None
            }
        }
    }

    /// One field, with its data settings checked.
    fn field(
        &mut self,
        path: &str,
        entity: &str,
        name: &str,
        span: &Span,
        value: &Expr,
    ) -> Option<FieldModel> {
        let written = TypeRef::from_expr(value);
        let ty = self.lookup.resolve(&written);
        let born = entity_field_key(self.registry, entity, &[name.to_string()])
            .name()
            .rsplit('.')
            .next()
            .unwrap_or(name)
            .to_string();
        let nullable = self.lookup.is_nullable(&ty);
        let mut field = FieldModel {
            name: name.to_string(),
            born,
            ty: ty.clone(),
            nullable,
            sql_type: None,
            unique: false,
            generated: None,
            reference: None,
            immutable: false,
            allows: physical::Allows::default(),
            compound: None,
            range_item: None,
            fixed: Vec::new(),
            comment: None,
            json: None,
            builtin: None,
            scale: None,
        };
        if ty.has_error() {
            // A type that names nothing is resolution's one error: the
            // field keeps what its settings say, and whether it was written
            // optional, so no rule that reads it reports a second.
            field.nullable = matches!(written, TypeRef::Nullable(_))
                || matches!(value, Expr::NullableExpr(_))
                || matches!(value, Expr::RecordExpr(record)
                    if matches!(record.expr(), Some(Expr::NullableExpr(_))));
            if let Expr::RecordExpr(record) = value {
                field.unique = tree::members(record.member_list())
                    .iter()
                    .any(|(key, member)| {
                        tree::name_text(key).as_deref() == Some("unique")
                            && member.value().as_ref().map(Const::from_expr)
                                == Some(Const::Bool(true))
                    });
            }
            return Some(field);
        }
        if let Some(inner) = holds_entity(self.lookup, &ty) {
            self.out.push(
                Diagnostic::new(
                    &rule::DATA_ENTITY_IN_FIELD,
                    format!("`{entity}.{name}` holds a whole `{inner}`"),
                )
                .at(node_span(path, value.syntax()), format!("a `{inner}` record"))
                .reason(format!(
                    "An entity value is a snapshot, never stored inside another record; a field refers to a record by its identity, `{inner}.id`, which the database keeps as a foreign key."
                ))
                .next(format!("Store `{inner}.id` instead")),
            );
            return Some(field);
        }
        // Text for people is shown, never stored (area 10).
        if let Some(held) = holds_presentation(self.lookup, &ty, &mut Vec::new()) {
            let through = if held == "PresentationText" {
                String::new()
            } else {
                format!(", through `{held}`,")
            };
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_PRESENTATION_TEXT_PERSISTENCE,
                    format!("`{entity}.{name}` would store text formatted for a person"),
                )
                .at(node_span(path, value.syntax()), "`PresentationText`, stored")
                .reason(format!(
                    "`{entity}.{name}`{through} holds `PresentationText`: text in one reader's language and zone, which another reader, a change of locale or a correction to the zone database would show differently, and which can't be read back as a time. A record keeps the value itself and formats it when it is shown."
                ))
                .next(format!(
                    "Store the `Instant`, `CalendarDate` or `ZonedTime` instead, and format `{entity}.{name}` in the output that shows it"
                )),
            );
            return Some(field);
        }
        // A secret value is never stored as itself (area 19, chunk 5.5r2).
        if let Some(secret) = crate::types::secrets::held_secret(self.lookup, &ty, &mut Vec::new())
        {
            self.out.push(crate::types::secrets::stored(
                entity,
                name,
                secret,
                node_span(path, value.syntax()),
            ));
            return Some(field);
        }
        field.allows = columns::allows(
            self.lookup,
            &Ty::Field(entity.to_string(), vec![name.to_string()]),
        );
        if let Ty::Prim(builtin) = self.lookup.structure(ty.strip_nullable()) {
            field.builtin = Some(builtin);
        }
        field.scale = self
            .lookup
            .rule_chain(ty.strip_nullable())
            .into_iter()
            .find_map(|(_, rules)| rules.scale);
        // A token or a key is shown once and kept only as its verifier, by
        // the runtime (area 8).
        if let Ty::Prim(
            credential @ (jadpo_std::Builtin::ApiToken | jadpo_std::Builtin::ServiceKey),
        ) = self.lookup.structure(&ty)
        {
            let what = if credential == jadpo_std::Builtin::ApiToken {
                "An API token"
            } else {
                "A service key"
            };
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_AUTH_CREDENTIAL_STORED,
                    format!(
                        "`{entity}.{name}` keeps {}",
                        if credential == jadpo_std::Builtin::ApiToken {
                            "an API token"
                        } else {
                            "a service key"
                        }
                    ),
                )
                .at(node_span(path, value.syntax()), "a secret, stored")
                .reason(format!(
                    "{what} is shown once, in the output of the action that issues it, and the runtime keeps only its verifier, so a copy of the database signs no one in."
                ))
                .next(format!(
                    "Remove `{name}`, and give the {} in the output of the action that issues it",
                    if credential == jadpo_std::Builtin::ApiToken { "token" } else { "key" }
                )),
            );
            return Some(field);
        }
        match column_type(self.lookup, &ty) {
            // A geometry is stored in a PostGIS `geography` column, which
            // the geospatial area builds (area 19, chunk 5.5s).
            Ok(sql) if sql == "geography" => {
                let shown = ty.strip_nullable().to_string();
                self.out.push(
                    Diagnostic::new(
                        &rule::TOOLCHAIN_NOT_BUILT_YET,
                        format!("Storing a `{shown}` field isn't built yet"),
                    )
                    .at(node_span(path, value.syntax()), "a geometry, stored as `geography`")
                    .reason(format!(
                        "Area 19 stores a geometry in a PostGIS `geography` column in WGS84, with its query conditions and indexes, and that storage comes with the geospatial area (roadmap 5.5u); until then this compiler can't read or write `{entity}.{name}`."
                    ))
                    .next(format!(
                        "Keep what `{entity}.{name}` holds out of storage for now, or store its latitude and longitude as `Latitude` and `Longitude` fields of their own"
                    ))
                    .help_at("19-standard-library.html#geospatial-a-future-area"),
                );
                field.sql_type = Some(sql);
            }
            // A vector is stored in a pgvector `vector` column, fixed to its
            // type's `dimensions:` where it has them (area 21, chunk
            // 5.5v1). The column keeps its length, and pgvector its 1 to
            // 16,000 items, so no length is checked beside it.
            Ok(sql) if sql == "vector" => {
                let dimensions = self
                    .lookup
                    .rule_chain(&Ty::Field(entity.to_string(), vec![name.to_string()]))
                    .iter()
                    .find_map(|(_, rules)| rules.dimensions);
                field.sql_type = Some(match dimensions {
                    Some(n) => format!("vector({n})"),
                    None => sql,
                });
                field.allows.min_length = None;
                field.allows.max_length = None;
            }
            // A JSON-held standard type is stored as its contract's JSON,
            // written and read by the route codec (area 19, chunk 5.5w2b).
            Ok(sql) if sql == "jsonb" && json_held(self.lookup, &ty).is_some() => {
                field.json = json_held(self.lookup, &ty);
                field.sql_type = Some(sql);
            }
            // Any other structured field is stored as `jsonb` in the API's
            // JSON (area 5), whose codec structured fields don't have yet.
            Ok(sql) if sql == "jsonb" => {
                let shown = ty.strip_nullable().to_string();
                self.out.push(
                    Diagnostic::new(
                        &rule::TOOLCHAIN_NOT_BUILT_YET,
                        format!("Storing a `{shown}` field isn't built yet"),
                    )
                    .at(node_span(path, value.syntax()), "structured, stored as `jsonb`")
                    .reason(format!(
                        "Area 5 stores an enum with data, an object, a list, a set or a map as `jsonb` holding the API's JSON, and that JSON comes with area 7; until then this compiler can't read or write `{entity}.{name}`."
                    ))
                    .next(format!(
                        "Store what `{entity}.{name}` holds in fields of its own, or as records of an entity that refers to `{entity}`, for now"
                    ))
                    .help_at("05-data.html#storage-types"),
                );
                field.sql_type = Some(sql);
            }
            Ok(sql) => {
                // A compound standard type (area 19): `Money`, built in, or
                // an object stored in a column per part (`PostalAddress`,
                // chunk 5.5l).
                let structure = self.lookup.structure(&ty);
                let object = match &structure {
                    Ty::Named(name) | Ty::Opaque(name, _) => jadpo_std::standard_type(name),
                    _ => None,
                };
                // A range's ends' type, which its column and its reading
                // need (chunk 5.5w).
                if let Ty::Opaque(name, args) = &structure
                    && name == jadpo_std::RANGE
                    && let [item] = args.as_slice()
                {
                    field.range_item = self.lookup.representation(item).map(|item| item.name());
                }
                field.compound = self
                    .lookup
                    .representation(&ty)
                    .and_then(jadpo_std::standard::of_builtin)
                    .or(object)
                    .filter(|entry| !entry.columns().is_empty())
                    .map(|entry| entry.name);
                if let Some(jadpo_std::Builtin::Quantity(kind)) = self.lookup.representation(&ty) {
                    field.comment = Some(jadpo_std::standard::storage::unit_comment(kind));
                }
                // The columns its type fixes, each held to its currency by a
                // check constraint.
                if let Some(entry) = field.compound_entry() {
                    let field_ty = Ty::Field(entity.to_string(), vec![name.to_string()]);
                    let chain = self.lookup.rule_chain(&field_ty);
                    for (suffix, fixed) in entry.fixed_parts() {
                        if let Some(code) = chain
                            .iter()
                            .find_map(|(_, rules)| fixed.of(rules))
                            .and_then(jadpo_std::money::code)
                        {
                            field.fixed.push((suffix, code.to_string()));
                        }
                    }
                }
                field.sql_type = Some(sql);
            }
            Err(what) => {
                self.out.push(
                    Diagnostic::new(
                        &rule::TOOLCHAIN_NOT_BUILT_YET,
                        format!("Storing a `{what}` field isn't built yet"),
                    )
                    .at(node_span(path, value.syntax()), "no column type yet")
                    .reason(format!(
                        "Area 5 gives every stored field a column type, and `{what}` has none in this compiler yet."
                    ))
                    .next(format!("Store `{entity}.{name}` as another type for now")),
                );
            }
        }
        let target = match ty.strip_nullable() {
            Ty::Field(host, rest)
                if rest.len() == 1
                    && rest[0] == "id"
                    && self.lookup.base(host) == Some(FieldsBase::Entity) =>
            {
                Some(host.clone())
            }
            _ => None,
        };
        let settings = match value {
            Expr::RecordExpr(record) => tree::members(record.member_list()),
            _ => Vec::new(),
        };
        let mut on_delete: Option<(OnDelete, Span)> = None;
        for (key, member) in settings {
            let Some(word) = tree::name_text(&key) else {
                continue;
            };
            let key_span = node_span(path, key.syntax());
            let value_span = member
                .value()
                .map_or(key_span.clone(), |v| node_span(path, v.syntax()));
            let names = member.value().as_ref().map(path_names).unwrap_or_default();
            match word.as_str() {
                "generated" => match names.as_slice() {
                    [host, variant]
                        if host == Generated::NAME && Generated::from_name(variant).is_some() =>
                    {
                        field.generated = Generated::from_name(variant);
                        let instant =
                            self.lookup.representation(&ty) == Some(jadpo_std::Builtin::Instant);
                        if !instant || nullable {
                            self.out.push(
                                Diagnostic::new(
                                    &rule::DATA_GENERATED_TYPE,
                                    format!("`{entity}.{name}` can't be generated"),
                                )
                                .at(key_span, if nullable { "can be `none`" } else { "not an `Instant`" })
                                .reason("A generated field is a required `Instant` set from the operation's clock, on create or on every change; code never assigns it.")
                                .next(if nullable {
                                    format!("Make `{name}` required, or remove `generated:`")
                                } else {
                                    format!("Type `{name}` as `Instant`, or remove `generated:`")
                                }),
                            );
                        }
                    }
                    _ => self.setting_value(
                        value_span,
                        "generated",
                        "`Generated.on_create` or `Generated.on_change`",
                    ),
                },
                "unique" => match member.value().as_ref().map(Const::from_expr) {
                    Some(Const::Bool(true)) => field.unique = true,
                    _ => self.setting_value(value_span, "unique", "`true`"),
                },
                "immutable" => match member.value().as_ref().map(Const::from_expr) {
                    Some(Const::Bool(true)) => field.immutable = true,
                    Some(Const::Bool(false)) => {}
                    _ => self.setting_value(value_span, "immutable", "`true` or `false`"),
                },
                // A field's own grants narrow its entity's (area 9, "Field
                // restrictions"), read by `crate::policy`.
                "policy" => {}
                "on_delete" => match names.as_slice() {
                    [host, variant]
                        if host == OnDelete::NAME && OnDelete::from_name(variant).is_some() =>
                    {
                        on_delete = OnDelete::from_name(variant).map(|o| (o, value_span));
                    }
                    _ => self.setting_value(
                        value_span,
                        "on_delete",
                        "`OnDelete.restricted`, `OnDelete.cascade` or `OnDelete.cleared`",
                    ),
                },
                _ => {}
            }
        }
        match (&target, &on_delete) {
            (Some(target), None) => self.out.push(
                Diagnostic::new(
                    &rule::DATA_ON_DELETE_REQUIRED,
                    format!("`{entity}.{name}` doesn't say what deleting its `{target}` does"),
                )
                .at(span.clone(), format!("refers to a `{target}`"))
                .reason(format!(
                    "Any field typed `{target}.id` is a reference with a foreign key, and its block must say what happens when the `{target}` is deleted: `OnDelete.restricted` refuses the delete, `OnDelete.cascade` deletes this record too, and `OnDelete.cleared` sets an optional reference to `none`. There is no default."
                ))
                .next(format!("Add `on_delete:` to `{name}`'s block")),
            ),
            (None, Some((_, at))) => self.out.push(
                Diagnostic::new(
                    &rule::DATA_ON_DELETE_NOT_REFERENCE,
                    format!("`{entity}.{name}` isn't a reference"),
                )
                .at(at.clone(), "nothing to delete")
                .reason("`on_delete:` says what deleting a referenced record does, so it belongs only on a field typed `X.id`.")
                .next("Remove `on_delete:`"),
            ),
            (Some(target), Some((OnDelete::Cleared, at))) if !nullable => self.out.push(
                Diagnostic::new(
                    &rule::DATA_CLEARED_REQUIRED_REFERENCE,
                    format!("`{entity}.{name}` can't be cleared"),
                )
                .at(at.clone(), "the field can't be `none`")
                .reason(format!(
                    "`OnDelete.cleared` sets the reference to `none` when the `{target}` is deleted, which only an optional reference (`{target}.id?`) can hold."
                ))
                .next(format!("Make it `{target}.id?`, or use `OnDelete.restricted` or `OnDelete.cascade`")),
            ),
            _ => {}
        }
        field.reference = target.map(|target| Reference {
            target,
            on_delete: on_delete.map(|(o, _)| o),
        });
        Some(field)
    }

    fn setting_value(&mut self, span: Span, setting: &str, takes: &str) {
        self.out.push(
            Diagnostic::new(
                &rule::TYPE_SETTING_VALUE,
                format!("`{setting}` takes {takes}"),
            )
            .at(span, "not one of these")
            .reason(format!("A field's `{setting}:` setting is one of {takes}."))
            .next(format!("Give `{setting}` {takes}")),
        );
    }

    /// The rules in a `unique:` or `indexes:` section.
    fn rules(
        &mut self,
        path: &str,
        entity: &EntityModel,
        member: &ast::Member,
        unique: bool,
    ) -> Vec<ListRule> {
        let section = if unique { "unique" } else { "indexes" };
        let what = if unique { "unique rule" } else { "index" };
        let a_what = if unique { "a unique rule" } else { "an index" };
        let Some(list) = member.block() else {
            let at = member
                .value()
                .map_or_else(|| member.syntax().clone(), |v| v.syntax().clone());
            self.out.push(
                Diagnostic::new(
                    &rule::DATA_FIELD_LIST_REQUIRED,
                    format!("`{section}:` holds named lists of fields"),
                )
                .at(node_span(path, &at), "not a block")
                .reason(format!(
                    "`{section}:` is a block of named {what}s, each a list of the entity's fields: `{section}: {{ name: [field, other] }}`."
                ))
                .next(format!("Write `{section}: {{ ... }}`")),
            );
            return Vec::new();
        };
        let mut out: Vec<ListRule> = Vec::new();
        // Where each rule kept in `out` is written.
        let mut spans: Vec<Span> = Vec::new();
        for (name, rule_member) in tree::members(Some(list)) {
            let Some(rule_name) = tree::name_text(&name) else {
                continue;
            };
            let span = node_span(path, name.syntax());
            let options = rule_member.block().is_some()
                || matches!(rule_member.value(), Some(Expr::RecordExpr(_)));
            let entries = match rule_member.value() {
                Some(Expr::ListExpr(entries)) => entries,
                _ if options && !unique => {
                    self.out.push(
                        Diagnostic::new(
                            &rule::TOOLCHAIN_NOT_BUILT_YET,
                            "An index with options isn't built yet",
                        )
                        .at(span, "options")
                        .reason("Special index kinds take a `kind:` option where the store supports it (area 5), and this compiler builds plain indexes only: a list of fields.")
                        .next(format!("Write `{rule_name}: [field, ...]`")),
                    );
                    continue;
                }
                other => {
                    let at = other.map_or(rule_member.syntax().clone(), |v| v.syntax().clone());
                    self.out.push(
                        Diagnostic::new(
                            &rule::DATA_FIELD_LIST_REQUIRED,
                            format!("The {what} `{rule_name}` isn't a list of fields"),
                        )
                        .at(node_span(path, &at), "not a list")
                        .reason(if unique {
                            format!("Each unique rule is a list of the entity's fields in brackets: `{rule_name}: [field, other]`.")
                        } else {
                            format!("Each index is a list of the entity's fields in brackets, even with one: `{rule_name}: [field]`.")
                        })
                        .next(format!("Write `{rule_name}: [field, ...]`")),
                    );
                    continue;
                }
            };
            let mut fields: Vec<String> = Vec::new();
            let mut broken = false;
            for entry in entries.exprs() {
                let entry_span = node_span(path, entry.syntax());
                let Expr::NameRef(field_ref) = &entry else {
                    broken = true;
                    self.out.push(
                        Diagnostic::new(
                            &rule::DATA_FIELD_LIST_REQUIRED,
                            format!("The {what} `{rule_name}` lists something that isn't a field"),
                        )
                        .at(entry_span, "not a field's name")
                        .reason(format!(
                            "{} lists the entity's own fields by name.",
                            capitalise(a_what)
                        ))
                        .next("Write the field's name"),
                    );
                    continue;
                };
                let field = tree::name_ref_text(field_ref).unwrap_or_default();
                let Some(found) = entity.fields.iter().find(|f| f.name == field) else {
                    broken = true;
                    let near = nearest(&field, entity.fields.iter().map(|f| f.name.as_str()));
                    let problem = Diagnostic::new(
                        &rule::DATA_UNKNOWN_FIELD,
                        format!("`{}` has no field `{field}`", entity.name),
                    )
                    .at(entry_span.clone(), "no such field")
                    .reason(format!(
                        "The {what} `{rule_name}` names fields of `{}`, its own.",
                        entity.name
                    ));
                    self.out.push(match near {
                        Some(near) => problem.next(format!("Use `{near}`")).edit(entry_span, near),
                        None => problem.next(format!("Remove `{field}`")),
                    });
                    continue;
                };
                if let Some(first) = fields.iter().position(|f| *f == field) {
                    broken = true;
                    let _ = first;
                    self.out.push(
                        Diagnostic::new(
                            &rule::DATA_REPEATED_FIELD,
                            format!("The {what} `{rule_name}` lists `{field}` twice"),
                        )
                        .at(entry_span, "listed again")
                        .reason(format!("Each field counts once in {a_what}."))
                        .next(format!("Remove the second `{field}`")),
                    );
                    continue;
                }
                if unique && found.nullable {
                    broken = true;
                    self.out.push(
                        Diagnostic::new(
                            &rule::DATA_UNIQUE_NULLABLE_FIELD,
                            format!("`{}.{field}` can be `none`", entity.name),
                        )
                        .at(entry_span, "optional field")
                        .reason("The fields of a unique rule over several fields must be required, so every engine agrees on what a duplicate is.")
                        .next(format!("Make `{field}` required, or leave it out of `{rule_name}`")),
                    );
                    continue;
                }
                fields.push(field);
            }
            if broken {
                continue;
            }
            if unique && fields.len() == 1 {
                let only = &fields[0];
                self.out.push(
                    Diagnostic::new(
                        &rule::DATA_UNIQUE_SINGLE_FIELD,
                        format!("The unique rule `{rule_name}` has one field"),
                    )
                    .at(span, "one field")
                    .reason(format!(
                        "The `unique:` section holds rules over several fields; a single field is made unique in its own block, `{only}: ... {{ unique: true }}`."
                    ))
                    .next(format!("Move it to `{only}`'s block as `unique: true`")),
                );
                continue;
            }
            if fields.is_empty() {
                self.out.push(
                    Diagnostic::new(
                        &rule::DATA_FIELD_LIST_REQUIRED,
                        format!("The {what} `{rule_name}` lists no fields"),
                    )
                    .at(span, "empty")
                    .reason(format!(
                        "{} names at least one of the entity's fields.",
                        capitalise(a_what)
                    ))
                    .next(format!("List its fields, or remove `{rule_name}`")),
                );
                continue;
            }
            let same = |a: &[String], b: &[String]| {
                if unique {
                    let a: BTreeSet<&String> = a.iter().collect();
                    let b: BTreeSet<&String> = b.iter().collect();
                    a == b
                } else {
                    a == b
                }
            };
            if let Some(at) = out.iter().position(|r| same(&r.fields, &fields)) {
                let earlier = &out[at];
                self.out.push(
                    Diagnostic::new(
                        &rule::DATA_DUPLICATE_RULE,
                        format!("The {what} `{rule_name}` repeats `{}`", earlier.name),
                    )
                    .at(span, "same fields")
                    .also(spans[at].clone(), "first here")
                    .reason(if unique {
                        "Both cover the same fields, in whatever order, so they are one rule; a write's conflict names one of them (area 6).".to_string()
                    } else {
                        "Both list the same fields in the same order, so the database would keep the same index twice.".to_string()
                    })
                    .next(format!("Remove `{rule_name}`")),
                );
                continue;
            }
            out.push(ListRule {
                name: rule_name,
                fields,
            });
            spans.push(span);
        }
        out
    }
}

/// A field of an entity, by name and by its place among the entity's
/// fields.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FieldAt {
    pub entity: String,
    pub field: String,
    /// Its place in [`EntityModel::fields`].
    pub index: usize,
}

/// A reference another entity, or the entity itself, holds to an entity.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Referrer {
    pub holder: FieldAt,
    pub on_delete: Option<OnDelete>,
    /// Whether the entity holding it has a lifecycle.
    pub lifecycle: bool,
}

/// A reference to an entity in another store.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Across {
    pub field: FieldAt,
    pub target: String,
    /// The holder's store and the target's.
    pub here: String,
    pub there: String,
}

/// One entity's links with the others, without positions (area 5,
/// "References").
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct EntityLinks {
    /// The references to it, in the holders' name order, then their
    /// fields' order.
    pub referrers: Vec<Referrer>,
    /// Its references to an entity in another store.
    pub across: Vec<Across>,
    /// The cycles of required references reported at it, each as the
    /// fields it passes through, starting with one of its own.
    pub cycles: Vec<Vec<FieldAt>>,
}

/// Every entity's links, by name: what crosses entities, read once from
/// the models, so only a change to an entity can change it.
#[salsa::tracked(returns(ref))]
pub fn entity_links(db: &dyn Db, project: Project) -> BTreeMap<String, EntityLinks> {
    let entities = &data_model(db, project).entities;
    let by_name: BTreeMap<&str, &EntityModel> =
        entities.iter().map(|e| (e.name.as_str(), e)).collect();
    let mut out: BTreeMap<String, EntityLinks> = BTreeMap::new();
    for entity in entities {
        for (index, field) in entity.fields.iter().enumerate() {
            let Some(reference) = &field.reference else {
                continue;
            };
            let at = FieldAt {
                entity: entity.name.clone(),
                field: field.name.clone(),
                index,
            };
            out.entry(reference.target.clone())
                .or_default()
                .referrers
                .push(Referrer {
                    holder: at.clone(),
                    on_delete: reference.on_delete,
                    lifecycle: entity.lifecycle.is_some(),
                });
            let Some(target) = by_name.get(reference.target.as_str()) else {
                continue;
            };
            if let (Some(here), Some(there)) = (&entity.store, &target.store)
                && here != there
            {
                out.entry(entity.name.clone())
                    .or_default()
                    .across
                    .push(Across {
                        field: at,
                        target: target.name.clone(),
                        here: here.clone(),
                        there: there.clone(),
                    });
            }
        }
    }
    // Required references between entities, in name order, for a cycle
    // found the same way every time.
    let edges: BTreeMap<&str, Vec<Hop>> = entities
        .iter()
        .map(|entity| {
            let hops = entity
                .fields
                .iter()
                .enumerate()
                .filter(|(_, f)| !f.nullable)
                .filter_map(|(index, f)| {
                    let target = f.reference.as_ref()?.target.as_str();
                    by_name.contains_key(target).then_some(Hop {
                        from: entity.name.as_str(),
                        to: target,
                        field: f.name.as_str(),
                        index,
                    })
                })
                .collect();
            (entity.name.as_str(), hops)
        })
        .collect();
    let mut reported: BTreeSet<Vec<String>> = BTreeSet::new();
    for start in edges.keys() {
        if let Some(cycle) = find_cycle(&edges, start) {
            let mut members: Vec<String> = cycle.iter().map(|hop| hop.from.to_string()).collect();
            members.sort();
            if !reported.insert(members) {
                continue;
            }
            out.entry(start.to_string()).or_default().cycles.push(
                cycle
                    .iter()
                    .map(|hop| FieldAt {
                        entity: hop.from.to_string(),
                        field: hop.field.to_string(),
                        index: hop.index,
                    })
                    .collect(),
            );
        }
    }
    out
}

/// The entity called `name`'s links: what its references to others, and
/// theirs to it, mean for it.
#[salsa::tracked(returns(ref))]
pub fn links_of<'db>(db: &'db dyn Db, project: Project, name: Name<'db>) -> EntityLinks {
    entity_links(db, project)
        .get(name.text(db))
        .cloned()
        .unwrap_or_default()
}

/// The problems an entity's links bring: references to another store and
/// cycles of required references, at its own fields, naming the other
/// entities' parts where they are written.
fn link_problems(
    db: &dyn Db,
    project: Project,
    read: &EntityRead,
    links: &EntityLinks,
) -> Vec<Diagnostic> {
    let spans = |entity: &str| -> Option<&EntitySpans> {
        if entity == read.model.name {
            return Some(&read.spans);
        }
        entity_read(db, project, Name::new(db, entity.to_string()))
            .as_ref()
            .map(|read| &read.spans)
    };
    let field_span =
        |at: &FieldAt| -> Option<Span> { spans(&at.entity)?.fields.get(at.index).cloned() };
    let entity = &read.model;
    let mut out = Vec::new();
    for across in &links.across {
        let (Some(at), here, there) = (field_span(&across.field), &across.here, &across.there)
        else {
            continue;
        };
        let mut problem = Diagnostic::new(
            &rule::DATA_REFERENCE_ACROSS_STORES,
            format!(
                "`{}.{}` refers to a `{}` in another store",
                entity.name, across.field.field, across.target
            ),
        )
        .at(at, format!("`{}` is in `{here}`", entity.name));
        if let Some(target) = spans(&across.target) {
            problem = problem.also(
                target.name.clone(),
                format!("`{}` is in `{there}`", across.target),
            );
        }
        out.push(
            problem
                .reason("A reference is a foreign key, which a database keeps only between its own tables; an action writes one store, and work across stores goes through an event and a subscriber.")
                .next(format!("Keep `{}` and `{}` in one store, or store the id as a plain `Uuid`", entity.name, across.target)),
        );
    }
    for cycle in &links.cycles {
        let Some(first) = cycle.first() else {
            continue;
        };
        let Some(at) = field_span(first) else {
            continue;
        };
        let path: Vec<String> = cycle
            .iter()
            .map(|at| format!("`{}.{}`", at.entity, at.field))
            .collect();
        let mut problem = Diagnostic::new(
            &rule::DATA_REFERENCE_CYCLE,
            if cycle.len() == 1 {
                format!(
                    "`{}.{}` requires a `{}` before any exists",
                    first.entity, first.field, first.entity
                )
            } else {
                format!(
                    "Required references form a cycle through `{}`",
                    first.entity
                )
            },
        )
        .at(at, "required reference");
        for other in &cycle[1..] {
            if let Some(span) = field_span(other) {
                problem = problem.also(span, "and this one");
            }
        }
        out.push(
            problem
                .reason(format!(
                    "{} {} required, so no record could be created first: each needs one that already exists.",
                    crate::and_list(&path),
                    match cycle.len() {
                        1 => "is",
                        2 => "are both",
                        _ => "are all",
                    }
                ))
                .next("Make one of these references optional (`X.id?`)"),
        );
    }
    out
}

/// One required reference, as a step of a cycle.
#[derive(Clone, Copy)]
struct Hop<'a> {
    from: &'a str,
    to: &'a str,
    field: &'a str,
    index: usize,
}

/// A cycle of required references back to `start`, as the fields it
/// passes through, shortest first by a breadth-first walk.
fn find_cycle<'a>(edges: &BTreeMap<&'a str, Vec<Hop<'a>>>, start: &'a str) -> Option<Vec<Hop<'a>>> {
    let mut queue: std::collections::VecDeque<Vec<Hop<'a>>> = std::collections::VecDeque::new();
    for hop in edges.get(start)? {
        queue.push_back(vec![*hop]);
        if hop.to == start {
            return Some(vec![*hop]);
        }
    }
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    while let Some(path) = queue.pop_front() {
        let to = path.last()?.to;
        if to == start {
            return Some(path);
        }
        if !seen.insert(to) {
            continue;
        }
        for next in edges.get(to).into_iter().flatten() {
            let mut longer = path.clone();
            longer.push(*next);
            queue.push_back(longer);
        }
    }
    None
}

fn capitalise(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// The first two words of a declaration or entry, to name it by:
/// `action add`.
fn head(node: &jadpo_syntax::SyntaxNode) -> String {
    node.descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| !token.kind().is_trivia())
        .take(2)
        .map(|token| token.text().to_string())
        .collect::<Vec<_>>()
        .join(" ")
}

/// The entity a type holds, directly or inside a collection.
/// Whether a value of `ty` holds `PresentationText`, directly or in a
/// collection, an object's field or a variant's data: the type that holds
/// it, or `PresentationText` itself.
fn holds_presentation(lookup: &dyn TypeView, ty: &Ty, seen: &mut Vec<String>) -> Option<String> {
    if lookup.base_prim(ty) == Some(jadpo_std::Builtin::PresentationText) {
        return Some(match ty.strip_nullable() {
            Ty::Named(name) => name.clone(),
            _ => "PresentationText".to_string(),
        });
    }
    match lookup.structure(ty) {
        Ty::Nullable(inner) | Ty::List(inner) | Ty::Set(inner) => {
            holds_presentation(lookup, &inner, seen)
        }
        Ty::Map(key, value) => holds_presentation(lookup, &key, seen)
            .or_else(|| holds_presentation(lookup, &value, seen)),
        Ty::Named(name) if !seen.contains(&name) => {
            seen.push(name.clone());
            let held = match lookup.def(&name) {
                Some(Def::Fields { fields, base, .. }) if *base != FieldsBase::Entity => fields
                    .iter()
                    .any(|field| holds_presentation(lookup, &field.declared, seen).is_some()),
                Some(Def::Enum { variants }) => variants
                    .iter()
                    .filter_map(|variant| variant.fields.as_deref())
                    .flatten()
                    .any(|field| holds_presentation(lookup, &field.declared, seen).is_some()),
                _ => false,
            };
            held.then_some(name)
        }
        _ => None,
    }
}

/// The JSON-held standard type a value of `ty` is, by name: one stored as
/// its contract's JSON in a `jsonb` column (`RichText`, `RichTextBlock`,
/// `LocalisedText`; area 19, chunk 5.5w2b).
pub fn json_held(lookup: &dyn TypeView, ty: &Ty) -> Option<&'static str> {
    // A named collection is its own standard type, which its structure,
    // a list or a map, no longer names (chunk 5.5o2).
    lookup
        .standard_text(ty)
        .or_else(|| match lookup.structure(ty) {
            Ty::Named(name) | Ty::Opaque(name, _) => jadpo_std::standard_type(&name),
            _ => None,
        })
        .filter(|entry| {
            entry.storage == jadpo_std::standard::Storage::Column(jadpo_std::read::SqlType::Json)
        })
        .map(|entry| entry.name)
}

fn holds_entity(lookup: &dyn TypeView, ty: &Ty) -> Option<String> {
    match ty {
        Ty::Named(name) if lookup.base(name) == Some(FieldsBase::Entity) => Some(name.clone()),
        Ty::Nullable(inner) | Ty::List(inner) | Ty::Set(inner) => holds_entity(lookup, inner),
        Ty::Map(key, value) => holds_entity(lookup, key).or_else(|| holds_entity(lookup, value)),
        _ => None,
    }
}

// --- the schema ---------------------------------------------------------------

/// The system tables a store holds (area 5, "System tables"): what must
/// commit with an action's own writes goes in every store that holds an
/// entity, since any of them can be written. Their behaviour comes with
/// areas 7 and 12; their schema is here so migrations create them.
pub const SYSTEM_TABLES: [&str; 3] = ["outbox", "deliveries", "idempotency_keys"];

/// The table of browser sessions and API tokens, in the store the
/// principal's users live in when anything signs one in (area 8).
pub const SESSIONS: &str = "jadpo_sessions";

/// The service keys' table, in the store the principal's services live in
/// when service keys are declared (area 8).
pub const SERVICE_KEYS: &str = "jadpo_service_keys";

/// The passkeys' table and their ceremonies', beside the sessions they
/// start, when `passkey:` is declared (area 8, "Passkeys").
pub const PASSKEYS: &str = "jadpo_passkeys";
pub const PASSKEY_CHALLENGES: &str = "jadpo_passkey_challenges";

/// The physical schema the source asks for: every entity's table in its
/// store, and the system tables. Fields that can't be stored and entities
/// in a wrong store are left out; their errors stop the build anyway.
#[salsa::tracked(returns(ref))]
pub fn schema(db: &dyn Db, project: Project) -> Arc<Schema> {
    let model = data_model(db, project);
    let mut schema = Schema::default();
    let by_name: BTreeMap<&str, &EntityModel> = model
        .entities
        .iter()
        .map(|e| (e.name.as_str(), e))
        .collect();
    for entity in &model.entities {
        let Some(store) = &entity.store else {
            continue;
        };
        let tables = &mut schema
            .stores
            .entry(store.clone())
            .or_insert_with(|| physical::Store {
                driver: Driver::Postgres.name().to_string(),
                tables: BTreeMap::new(),
            })
            .tables;
        tables.insert(entity.table.clone(), table_of(entity, &by_name));
    }
    derived_indexes(db, project, model, &mut schema);
    for store in schema.stores.values_mut() {
        for name in SYSTEM_TABLES {
            store
                .tables
                .insert(format!("jadpo_{name}"), columns::system_table(name));
        }
    }
    // Sessions live beside the users they sign in, so one statement reads
    // a session with its user's record (area 8).
    if let Some(store) = crate::identity::identity(db, project).sessions_store()
        && let Some(found) = schema.stores.get_mut(store)
    {
        found
            .tables
            .insert(SESSIONS.to_string(), columns::system_table("sessions"));
    }
    if let Some(store) = crate::identity::identity(db, project).sessions_store()
        && crate::identity::identity(db, project).passkeys().is_some()
        && let Some(found) = schema.stores.get_mut(store)
    {
        found
            .tables
            .insert(PASSKEYS.to_string(), columns::system_table("passkeys"));
        found.tables.insert(
            PASSKEY_CHALLENGES.to_string(),
            columns::system_table("passkey_challenges"),
        );
    }
    if let Some(store) = crate::identity::identity(db, project).keys_store()
        && let Some(found) = schema.stores.get_mut(store)
    {
        found.tables.insert(
            SERVICE_KEYS.to_string(),
            columns::system_table("service_keys"),
        );
    }
    Arc::new(schema)
}

/// The indexes the queries need (area 5, "Indexes"): for each query, and
/// each list it includes, the fields it always tests with `==` or `in`,
/// in the order its lines name them, then its order's fields, unless an
/// index the table has already starts with those columns. The columns are
/// ascending: PostgreSQL reads an index backwards for a descending order.
/// The records of an entity with a lifecycle are indexed only as far as
/// its `visible:` lets queries see them.
fn derived_indexes(db: &dyn Db, project: Project, model: &DataModel, schema: &mut Schema) {
    use queries::{Condition, Include, IncludeKind, OrderField};
    let queries = queries::query_models(db, project);
    let lookup = Lookup::new(db, project);
    let mut wanted: Vec<(String, (Vec<String>, usize), String)> = Vec::new();
    fn equal_fields(filter: &[Condition], out: &mut Vec<String>) {
        for line in filter {
            let mut push = |field: &String| {
                if !out.contains(field) {
                    out.push(field.clone());
                }
            };
            match line {
                Condition::Compare {
                    field,
                    op: queries::Compare::Equal,
                    ..
                }
                | Condition::In { field, .. } => push(field),
                Condition::And(parts) => equal_fields(parts, out),
                _ => {}
            }
        }
    }
    /// The columns, and how many of them lead as equality tests.
    fn columns(
        prefix: &[String],
        filter: &[Condition],
        order: &[OrderField],
    ) -> (Vec<String>, usize) {
        let mut out = prefix.to_vec();
        equal_fields(filter, &mut out);
        let equal = out.len();
        for field in order {
            if !out.contains(&field.field) {
                out.push(field.field.clone());
            }
        }
        (out, equal)
    }
    #[allow(clippy::type_complexity)]
    fn walk(
        includes: &[Include],
        query: &str,
        wanted: &mut Vec<(String, (Vec<String>, usize), String)>,
    ) {
        for include in includes {
            if include.kind == IncludeKind::Children {
                let cols = columns(
                    std::slice::from_ref(&include.via),
                    &include.filter,
                    &include.order,
                );
                wanted.push((include.entity.clone(), cols, query.to_string()));
            }
            walk(&include.includes, query, wanted);
        }
    }
    for query in &queries.queries {
        let cols = columns(&[], &query.filter, &query.order);
        if !cols.0.is_empty() {
            wanted.push((query.entity.clone(), cols, query.name.clone()));
        }
        walk(&query.includes, &query.name, &mut wanted);
    }
    for (entity_name, (fields, equal), query) in wanted {
        let Some(entity) = model.entity(&entity_name) else {
            continue;
        };
        let Some(store) = &entity.store else { continue };
        let Some(table) = schema
            .stores
            .get_mut(store)
            .and_then(|store| store.tables.get_mut(&entity.table))
        else {
            continue;
        };
        let born: Vec<String> = fields
            .iter()
            .filter_map(|f| entity.fields.iter().find(|field| field.name == *f))
            .map(|field| field.born.clone())
            .collect();
        if born.len() != fields.len() {
            continue;
        }
        // An index that starts with these columns already serves the
        // query, and so does a unique key all of whose columns it tests
        // for equality: at most one record matches each value.
        let tested = &born[..equal];
        // The text columns it sorts by, after those it tests for
        // equality, are kept in code-point order, as the query sorts
        // them; an equality test compares under the column's own order.
        let collate: Vec<String> = born[equal..]
            .iter()
            .zip(&fields[equal..])
            .filter(|(_, name)| {
                lookup.representation(&Ty::Field(entity.name.clone(), vec![(*name).clone()]))
                    == Some(jadpo_std::Builtin::Text)
            })
            .map(|(column, _)| column.clone())
            .collect();
        let serves = |existing: &[String], sorted: &[String]| {
            existing.len() >= born.len()
                && existing[..born.len()] == born[..]
                && born
                    .iter()
                    .all(|column| collate.contains(column) == sorted.contains(column))
        };
        let covered = std::iter::once(&table.primary_key.columns)
            .chain(table.unique.values().map(|u| &u.columns))
            .any(|existing| serves(existing, &[]))
            || table
                .indexes
                .values()
                .any(|index| serves(&index.columns, &index.collate))
            || std::iter::once(&table.primary_key.columns)
                .chain(table.unique.values().map(|u| &u.columns))
                .any(|key| key.iter().all(|column| tested.contains(column)));
        if covered {
            continue;
        }
        // Every query of an entity with a lifecycle reads only the
        // records its `visible:` lets through, so its index holds only
        // those: a partial index, unless `visible:` reads `clock.now`,
        // which changes as time passes (area 5, "Indexes").
        let predicate = entity
            .lifecycle
            .as_ref()
            .and_then(|lifecycle| lifecycle.visible.as_ref())
            .filter(|visible| !lifecycle::reads_now(visible))
            .and_then(|visible| {
                jadpo_sql::condition(&mut jadpo_sql::Fixed, entity, visible, false)
            });
        // A reference's own index is no longer needed once this one
        // starts with its column (area 5), unless this one is partial: a
        // delete of the record it points at still finds hidden records.
        if predicate.is_none() {
            table.indexes.retain(|_, index| {
                !(index.reason.starts_with("reference ")
                    && index.columns.len() == 1
                    && born.first() == index.columns.first()
                    && born.len() > 1)
            });
        }
        let mut parts: Vec<&str> = vec![entity.table.as_str()];
        parts.extend(born.iter().map(String::as_str));
        if !collate.is_empty() {
            // Apart from an index on the same columns that tests them.
            parts.push("sorted");
        }
        table.indexes.insert(
            physical::object_name(&parts, "idx"),
            physical::Index {
                columns: born,
                reason: format!("query {query}"),
                predicate,
                collate,
                method: None,
            },
        );
    }
    nearest_indexes(model, &queries.queries, schema);
}

/// The HNSW index each nearest-neighbour query searches (area 21, "The
/// index"): on its vector field's column, with the operator class of the
/// distance its type measures by, one for each field however many queries
/// search it.
fn nearest_indexes(model: &DataModel, queries: &[queries::QueryModel], schema: &mut Schema) {
    for query in queries {
        let Some(nearest) = &query.nearest else {
            continue;
        };
        let Some(entity) = model.entity(&query.entity) else {
            continue;
        };
        let Some(field) = entity.fields.iter().find(|f| f.name == nearest.field) else {
            continue;
        };
        let Some(store) = &entity.store else { continue };
        let Some(table) = schema
            .stores
            .get_mut(store)
            .and_then(|store| store.tables.get_mut(&entity.table))
        else {
            continue;
        };
        let method = physical::IndexMethod {
            name: "hnsw".to_string(),
            operator_class: nearest.metric.operator_class().to_string(),
        };
        let column = field.born.clone();
        if table.indexes.values().any(|index| {
            index.columns == [column.clone()] && index.method.as_ref() == Some(&method)
        }) {
            continue;
        }
        let mut name = physical::object_name(&[entity.table.as_str(), column.as_str()], "idx");
        if table.indexes.contains_key(&name) {
            name = physical::object_name(&[entity.table.as_str(), column.as_str(), "hnsw"], "idx");
        }
        // Only the records its `visible:` lets queries see, as every
        // query's index of an entity with a lifecycle (area 5, "Indexes").
        let predicate = entity
            .lifecycle
            .as_ref()
            .and_then(|lifecycle| lifecycle.visible.as_ref())
            .filter(|visible| !lifecycle::reads_now(visible))
            .and_then(|visible| {
                jadpo_sql::condition(&mut jadpo_sql::Fixed, entity, visible, false)
            });
        table.indexes.insert(
            name,
            physical::Index {
                columns: vec![column],
                reason: format!("query {}", query.name),
                predicate,
                collate: Vec::new(),
                method: Some(method),
            },
        );
    }
}

fn table_of(entity: &EntityModel, by_name: &BTreeMap<&str, &EntityModel>) -> physical::Table {
    let table = entity.table.as_str();
    let mut out = physical::Table {
        holds: physical::Holds::Entity(entity.born.clone()),
        columns: BTreeMap::new(),
        primary_key: physical::Key {
            name: physical::object_name(&[table], "pkey"),
            columns: vec!["id".to_string()],
        },
        unique: BTreeMap::new(),
        foreign_keys: BTreeMap::new(),
        indexes: BTreeMap::new(),
        checks: BTreeMap::new(),
    };
    // A field's columns, by its name: a compound type's several.
    let column = |name: &str| -> Vec<String> {
        match entity.fields.iter().find(|f| f.name == name) {
            Some(field) => field.columns().into_iter().map(|(c, _)| c).collect(),
            None => vec![name.to_string()],
        }
    };
    let mut position = 0u32;
    for field in &entity.fields {
        let columns = field.columns();
        let entry = field.compound_entry();
        for (at, (name, sql_type)) in columns.iter().enumerate() {
            // Of a compound type's columns (area 19), each one its type
            // fixes holds the currency it allows, under a check constraint,
            // and the first takes the field's other rules.
            let fixed = entry.and_then(|_| {
                field
                    .fixed
                    .iter()
                    .find(|(suffix, _)| name == &format!("{}_{suffix}", field.born))
                    .map(|(_, code)| vec![code.clone()])
            });
            let fixable = entry.is_some_and(|entry| {
                entry
                    .fixed_parts()
                    .iter()
                    .any(|(suffix, _)| name == &format!("{}_{suffix}", field.born))
            });
            let allows = match entry {
                None => field.allows.clone(),
                Some(_) if fixable => physical::Allows {
                    values: fixed.clone().unwrap_or_default(),
                    ..physical::Allows::default()
                },
                Some(_) if at == 0 => physical::Allows {
                    values: Vec::new(),
                    ..field.allows.clone()
                },
                Some(_) => physical::Allows::default(),
            };
            if let Some(values) = fixed {
                out.checks.insert(
                    physical::object_name(&[table, name], "check"),
                    physical::Check {
                        column: name.clone(),
                        values,
                    },
                );
            }
            out.columns.insert(
                name.clone(),
                physical::Column {
                    position,
                    field: Some(field.born.clone()),
                    sql_type: sql_type.clone(),
                    // An object's part that may be `none` (`PostalAddress`'s
                    // region) is a column that may be `NULL`.
                    nullable: field.nullable
                        || entry.is_some_and(|entry| entry.part_may_be_none(at)),
                    retired: false,
                    allows: Some(allows),
                    comment: match entry {
                        Some(entry) => entry.part_comment(at),
                        None => field.comment.clone(),
                    },
                },
            );
            position += 1;
        }
        if columns.is_empty() {
            continue;
        }
        if field.unique && field.name != "id" {
            out.unique.insert(
                physical::object_name(&[table, &field.born], "key"),
                physical::Unique {
                    columns: columns.iter().map(|(name, _)| name.clone()).collect(),
                    rule: format!("{}.{}", entity.born, field.born),
                },
            );
        }
        if let Some(reference) = &field.reference
            && let (Some(on_delete), Some(target)) =
                (reference.on_delete, by_name.get(reference.target.as_str()))
        {
            out.foreign_keys.insert(
                physical::object_name(&[table, &field.born], "fkey"),
                physical::ForeignKey {
                    columns: vec![field.born.clone()],
                    table: target.table.clone(),
                    on_delete: match on_delete {
                        OnDelete::Restricted => physical::OnDelete::Restrict,
                        OnDelete::Cascade => physical::OnDelete::Cascade,
                        OnDelete::Cleared => physical::OnDelete::SetNull,
                    },
                },
            );
            // A reference is indexed (area 5), unless a unique rule
            // starting with it already gives one.
            let covered = field.unique
                || entity
                    .unique
                    .iter()
                    .any(|rule| rule.fields.first() == Some(&field.name));
            if !covered {
                out.indexes.insert(
                    physical::object_name(&[table, &field.born], "idx"),
                    physical::Index {
                        columns: vec![field.born.clone()],
                        reason: format!("reference {}.{}", entity.born, field.born),
                        predicate: None,
                        collate: Vec::new(),
                        method: None,
                    },
                );
            }
        }
    }
    for rule in &entity.unique {
        out.unique.insert(
            physical::object_name(&[table, &rule.name], "key"),
            physical::Unique {
                columns: rule.fields.iter().flat_map(|f| column(f)).collect(),
                rule: format!("{}.{}", entity.born, rule.name),
            },
        );
    }
    for index in &entity.indexes {
        out.indexes.insert(
            physical::object_name(&[table, &index.name], "idx"),
            physical::Index {
                columns: index.fields.iter().flat_map(|f| column(f)).collect(),
                reason: format!("index {}.{}", entity.born, index.name),
                predicate: None,
                collate: Vec::new(),
                method: None,
            },
        );
    }
    out
}

/// The field called `name` of `entity`.
pub fn field<'e>(entity: &'e EntityModel, name: &str) -> Option<&'e FieldModel> {
    entity.fields.iter().find(|f| f.name == name)
}

/// A field's column type, as a parameter or column is sent; `None` when
/// the field can't be stored.
pub fn sql_type(field: &FieldModel) -> Option<jadpo_std::SqlType> {
    jadpo_std::SqlType::of_column(field.sql_type.as_deref()?)
}

/// An entity's fields as the columns a condition on them renders.
impl jadpo_sql::Columns for EntityModel {
    fn column(&self, name: &str) -> Option<jadpo_sql::Column<'_>> {
        Some(field(self, name)?.column())
    }
}

/// The name the database knows a unique rule by.
pub fn unique_constraint(entity: &EntityModel, rule: &str) -> String {
    physical::object_name(&[&entity.table, rule], "key")
}

/// The name the database knows a declared index by.
pub fn index_name(entity: &EntityModel, index: &str) -> String {
    physical::object_name(&[&entity.table, index], "idx")
}

/// The name the database knows a reference's foreign key by.
pub fn foreign_key(entity: &EntityModel, field: &FieldModel) -> String {
    physical::object_name(&[&entity.table, &field.born], "fkey")
}

/// One unique rule of an entity (area 5, "Unique rules"): a field's own
/// `unique: true`, named after the field, or a named rule over several.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct UniqueRule {
    /// The field's name, or the rule's.
    pub name: String,
    /// Whether it is a field's own rule.
    pub field: bool,
    pub fields: Vec<String>,
}

/// What a write to one entity can break (area 6, "When a write is
/// refused"): its unique rules, its references, and the references that
/// can refuse deleting it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct WriteRules {
    pub unique: Vec<UniqueRule>,
    /// Each reference: the field and the entity it points at.
    pub references: Vec<(String, String)>,
    /// What can refuse deleting a record: each `restricted` reference to
    /// it, or to a record its delete cascades to, as the entity holding
    /// the reference and the field, in name order.
    pub restricting: Vec<(String, String)>,
    /// Whether the entity has a lifecycle, so code never removes its
    /// records (area 5): `delete` runs its `delete` transition.
    pub lifecycle: bool,
    /// The fields only the lifecycle's transitions change.
    pub owned: Vec<String>,
    /// The lifecycle's transitions, each an operation of the entity.
    pub transitions: Vec<TransitionRule>,
    /// A reference through which removing a record would cascade to an
    /// entity with a lifecycle: the entity holding it and the field.
    pub cascades_to_lifecycle: Option<(String, String)>,
    /// The fields no update changes: `immutable: true` (area 9).
    pub immutable: Vec<String>,
    /// The fields only an action listed in root `policy { actions: }`
    /// changes: those policy reaches the caller or the tenant through
    /// (area 9, "A user field on the record").
    pub fixed: Vec<String>,
}

/// What one transition writes (area 5, "Lifecycle").
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct TransitionRule {
    pub name: String,
    /// Whether its `when:` can refuse a visible record: a rule it can
    /// break, keyed `Entity.transition` in `conflict:`.
    pub guarded: bool,
    /// The fields it sets.
    pub sets: Vec<String>,
}

impl WriteRules {
    /// The unique rule called `name`: a field's own or a named one.
    pub fn unique(&self, name: &str) -> Option<&UniqueRule> {
        self.unique.iter().find(|rule| rule.name == name)
    }

    /// The transition called `name`.
    pub fn transition(&self, name: &str) -> Option<&TransitionRule> {
        self.transitions.iter().find(|t| t.name == name)
    }

    /// The entity the reference field `field` points at.
    pub fn reference(&self, field: &str) -> Option<&str> {
        self.references
            .iter()
            .find(|(name, _)| name == field)
            .map(|(_, target)| target.as_str())
    }
}

/// What a write to the entity called `entity` can break: a query of its
/// own, which reads that entity and the references to it, so a body that
/// writes one entity is checked again only when that entity's rules, or
/// the references to it, change.
#[salsa::tracked(returns(ref))]
pub fn write_rules<'db>(
    db: &'db dyn Db,
    project: Project,
    entity: crate::lookups::Name<'db>,
) -> Option<WriteRules> {
    let found = entity_model(db, project, entity).as_deref()?;
    let mut rules = WriteRules {
        fixed: crate::policy::fixed_fields(db, project, &found.name),
        ..WriteRules::default()
    };
    for field in &found.fields {
        if field.immutable {
            rules.immutable.push(field.name.clone());
        }
        if field.unique && field.name != "id" {
            rules.unique.push(UniqueRule {
                name: field.name.clone(),
                field: true,
                fields: vec![field.name.clone()],
            });
        }
        if let Some(reference) = &field.reference {
            rules
                .references
                .push((field.name.clone(), reference.target.clone()));
        }
    }
    for rule in &found.unique {
        rules.unique.push(UniqueRule {
            name: rule.name.clone(),
            field: false,
            fields: rule.fields.clone(),
        });
    }
    if let Some(lifecycle) = &found.lifecycle {
        rules.lifecycle = true;
        rules.owned = lifecycle.owned.clone();
        rules.transitions = lifecycle
            .transitions
            .iter()
            .map(|t| TransitionRule {
                name: t.name.clone(),
                guarded: t.guarded,
                sets: t.set.iter().map(|(field, _)| field.clone()).collect(),
            })
            .collect();
    }
    // A delete cascades through every `cascade` reference to what it
    // deletes, and any `restricted` reference on the way refuses it.
    let mut seen: BTreeSet<String> = BTreeSet::from([found.name.clone()]);
    let mut queue: Vec<String> = vec![found.name.clone()];
    let mut restricting = BTreeSet::new();
    while let Some(current) = queue.pop() {
        for referrer in &links_of(db, project, Name::new(db, current)).referrers {
            let holder = &referrer.holder;
            match referrer.on_delete {
                // A reference that doesn't say is reported where it is
                // declared; taken as refusing, nothing here reports a
                // knock-on of it.
                Some(OnDelete::Restricted) | None => {
                    restricting.insert((holder.entity.clone(), holder.field.clone()));
                }
                Some(OnDelete::Cascade) => {
                    if referrer.lifecycle && rules.cascades_to_lifecycle.is_none() {
                        rules.cascades_to_lifecycle =
                            Some((holder.entity.clone(), holder.field.clone()));
                    }
                    if seen.insert(holder.entity.clone()) {
                        queue.push(holder.entity.clone());
                    }
                }
                Some(OnDelete::Cleared) => {}
            }
        }
    }
    rules.restricting = restricting.into_iter().collect();
    Some(rules)
}
