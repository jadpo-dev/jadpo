//! Area 5's rules on entities, and the physical schema they ask for
//! (docs/compiler-runtime.md §2.2): an entity's `id`, its fields and their
//! settings (`generated:`, `unique:`, `on_delete:`), its references, its
//! `unique:` and `indexes:` sections, the store it lives in, and the
//! tables, columns, keys and indexes that follow, with the system tables
//! each store holds. [`data_model`] reads every entity once, since
//! references and stores cross files; [`data_problems`] gives one file's
//! share of what it found, and [`schema`] the schema `jadpo migrate new`
//! records and `jadpo check` compares with the latest migration
//! ([`migrations`]).

mod columns;
pub mod migrations;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use jadpo_db::{Db, File, ItemKind, Project, symbols};
use jadpo_diag::{Diagnostic, Span, nearest, rule};
use jadpo_graph::Kind;
use jadpo_graph::schema::{self as physical, MAX_NAME, Schema};
use jadpo_std::{Driver, Generated, OnDelete};
use jadpo_syntax::ast::{self, AstNode, Decl, Expr, MemberItem};

use crate::case::to_snake;
use crate::graph::{field_key, registry};
use crate::lookups::Lookup;
use crate::tree::{self, node_span};
use crate::types::{Const, FieldsBase, Ty, TypeRef, TypeView};

pub use columns::column_type;

/// The sections an entity may hold besides its fields (area 5, "Where
/// things live"). The last three are decided and come with their chunks.
pub const SECTIONS: [&str; 6] = [
    "store",
    "unique",
    "indexes",
    "lifecycle",
    "caches",
    "policy",
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

/// One entity as area 5 reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntityModel {
    pub name: String,
    /// Its born name, which its table is named after.
    pub born: String,
    pub path: String,
    pub name_span: Span,
    /// The store it lives in; `None` when its `store:` is wrong.
    pub store: Option<String>,
    pub table: String,
    pub fields: Vec<FieldModel>,
    pub unique: Vec<ListRule>,
    pub indexes: Vec<ListRule>,
}

/// One field of an entity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldModel {
    pub name: String,
    pub born: String,
    pub span: Span,
    pub ty: Ty,
    pub nullable: bool,
    /// The column type; `None` when the field can't be stored.
    pub sql_type: Option<String>,
    pub unique: bool,
    pub generated: Option<Generated>,
    pub reference: Option<Reference>,
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
    pub span: Span,
    /// The fields it names that exist, in order.
    pub fields: Vec<String>,
}

/// Every entity, with the problems area 5 finds in them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DataModel {
    pub entities: Vec<EntityModel>,
    pub problems: Vec<Diagnostic>,
}

impl DataModel {
    pub fn entity(&self, name: &str) -> Option<&EntityModel> {
        self.entities.iter().find(|entity| entity.name == name)
    }
}

/// The project's entities, read once.
#[salsa::tracked(returns(ref))]
pub fn data_model(db: &dyn Db, project: Project) -> Arc<DataModel> {
    let table = symbols(db, project);
    let lookup = Lookup::new(db, project);
    let registry = registry(db, project);
    let connections = connections(db, project);
    let mut reader = Reader {
        lookup: &lookup,
        registry,
        connections,
        out: Vec::new(),
    };
    let mut entities = Vec::new();
    let files: BTreeMap<&str, File> = project
        .files(db)
        .iter()
        .map(|file| (file.path(db).as_str(), *file))
        .collect();
    let mut seen = BTreeSet::new();
    for symbols in table.declarations.values() {
        let Some(symbol) = symbols.iter().find(|s| s.kind == ItemKind::Entity) else {
            continue;
        };
        if !seen.insert(symbol.name.clone()) {
            continue;
        }
        let Some(source) = files
            .get(symbol.path.as_str())
            .and_then(|f| tree::source(db, *f))
        else {
            continue;
        };
        let Some(Decl::EntityDecl(decl)) = tree::decl_at(&source, symbol.index) else {
            continue;
        };
        if let Some(entity) = reader.entity(&symbol.path, &decl) {
            entities.push(entity);
        }
    }
    entities.sort_by(|a, b| a.name.cmp(&b.name));
    reader.across(&entities);
    Arc::new(DataModel {
        entities,
        problems: reader.out,
    })
}

/// Area 5's problems in one file.
#[salsa::tracked(returns(ref))]
pub fn data_problems(db: &dyn Db, project: Project, file: File) -> Vec<Diagnostic> {
    let path = file.path(db);
    data_model(db, project)
        .problems
        .iter()
        .filter(|problem| {
            problem
                .primary
                .as_ref()
                .is_some_and(|label| &label.span.path == path)
        })
        .cloned()
        .collect()
}

struct Reader<'a> {
    lookup: &'a Lookup<'a>,
    registry: &'a jadpo_graph::Registry,
    connections: &'a Connections,
    out: Vec<Diagnostic>,
}

impl Reader<'_> {
    fn entity(&mut self, path: &str, decl: &ast::EntityDecl) -> Option<EntityModel> {
        let name_node = decl.name()?;
        let name = tree::name_text(&name_node)?;
        let name_span = node_span(path, name_node.syntax());
        let born = self.registry.born(Kind::Entity, &name, None);
        let table = to_snake(&born);
        let mut entity = EntityModel {
            name: name.clone(),
            born,
            path: path.to_string(),
            name_span: name_span.clone(),
            store: Some(MAIN.to_string()),
            table: table.clone(),
            fields: Vec::new(),
            unique: Vec::new(),
            indexes: Vec::new(),
        };
        let mut store: Option<(String, Span)> = None;
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
                MemberItem::Elision(_) => continue,
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
                // Decided, and checked by the chunks that build them: the
                // lifecycle (3.4) and policy (4.4). A record cache needs the
                // Redis driver, which no chunk builds yet.
                "lifecycle" | "policy" => {}
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
                        }
                    }
                    _ => self.unknown_section(path, key.syntax(), &word, &name),
                },
            }
        }
        // The identity (area 5, "Entities and the `id`").
        match entity.fields.iter().find(|f| f.name == "id") {
            None => self.out.push(
                Diagnostic::new(
                    &rule::DATA_IDENTITY_MISSING,
                    format!("`{name}` has no `id` field"),
                )
                .at(name_span.clone(), "no `id`")
                .reason("Every entity's identity is one field, `id`, based on `Uuid`: generated when a record is created and never supplied by callers, so other code can refer to it as `Entity.id`.")
                .next("Add `id: Uuid` as the entity's first field"),
            ),
            Some(id) => {
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
                        .at(id.span.clone(), label)
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
        entity.store = self.placement(store, &name, &name_span);
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
        for field in &entity.fields {
            if field.born.len() > MAX_NAME {
                self.too_long(
                    &field.span,
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
        let reason = "An entity's file holds only its data definition: fields (`name: Type`), and the sections `store:`, `unique:`, `indexes:`, `lifecycle:`, `caches:` and `policy:`. Its identity is always the field `id`; actions, queries and failures go in the domain's own folders.";
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
        let born = field_key(self.registry, entity, &[name.to_string()])
            .name()
            .rsplit('.')
            .next()
            .unwrap_or(name)
            .to_string();
        let nullable = self.lookup.is_nullable(&ty);
        let mut field = FieldModel {
            name: name.to_string(),
            born,
            span: span.clone(),
            ty: ty.clone(),
            nullable,
            sql_type: None,
            unique: false,
            generated: None,
            reference: None,
        };
        if ty.has_error() {
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
        match column_type(self.lookup, &ty) {
            Ok(sql) => field.sql_type = Some(sql),
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
            if let Some(earlier) = out.iter().find(|r| same(&r.fields, &fields)) {
                self.out.push(
                    Diagnostic::new(
                        &rule::DATA_DUPLICATE_RULE,
                        format!("The {what} `{rule_name}` repeats `{}`", earlier.name),
                    )
                    .at(span, "same fields")
                    .also(earlier.span.clone(), "first here")
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
                span,
                fields,
            });
        }
        out
    }

    /// Rules across entities: references to another store and cycles of
    /// required references.
    fn across(&mut self, entities: &[EntityModel]) {
        let by_name: BTreeMap<&str, &EntityModel> =
            entities.iter().map(|e| (e.name.as_str(), e)).collect();
        for entity in entities {
            for field in &entity.fields {
                let Some(reference) = &field.reference else {
                    continue;
                };
                let Some(target) = by_name.get(reference.target.as_str()) else {
                    continue;
                };
                if let (Some(here), Some(there)) = (&entity.store, &target.store)
                    && here != there
                {
                    self.out.push(
                        Diagnostic::new(
                            &rule::DATA_REFERENCE_ACROSS_STORES,
                            format!("`{}.{}` refers to a `{}` in another store", entity.name, field.name, target.name),
                        )
                        .at(field.span.clone(), format!("`{}` is in `{here}`", entity.name))
                        .also(target.name_span.clone(), format!("`{}` is in `{there}`", target.name))
                        .reason("A reference is a foreign key, which a database keeps only between its own tables; an action writes one store, and work across stores goes through an event and a subscriber.")
                        .next(format!("Keep `{}` and `{}` in one store, or store the id as a plain `Uuid`", entity.name, target.name)),
                    );
                }
            }
        }
        // Required references between entities, in name order, for a
        // cycle found the same way every time.
        let edges: BTreeMap<&str, Vec<(&str, &FieldModel)>> = entities
            .iter()
            .map(|entity| {
                let out = entity
                    .fields
                    .iter()
                    .filter(|f| !f.nullable)
                    .filter_map(|f| {
                        let target = f.reference.as_ref()?.target.as_str();
                        by_name.contains_key(target).then_some((target, f))
                    })
                    .collect();
                (entity.name.as_str(), out)
            })
            .collect();
        let mut reported: BTreeSet<Vec<String>> = BTreeSet::new();
        for start in edges.keys() {
            if let Some(cycle) = find_cycle(&edges, start) {
                let mut members: Vec<String> =
                    cycle.iter().map(|(from, _)| from.to_string()).collect();
                members.sort();
                if !reported.insert(members) {
                    continue;
                }
                let (first_entity, first_field) = cycle[0];
                let path: Vec<String> = cycle
                    .iter()
                    .map(|(from, field)| format!("`{from}.{}`", field.name))
                    .collect();
                let mut problem = Diagnostic::new(
                    &rule::DATA_REFERENCE_CYCLE,
                    if cycle.len() == 1 {
                        format!(
                            "`{first_entity}.{}` requires a `{first_entity}` before any exists",
                            first_field.name
                        )
                    } else {
                        format!("Required references form a cycle through `{first_entity}`")
                    },
                )
                .at(first_field.span.clone(), "required reference");
                for (from, field) in &cycle[1..] {
                    let _ = from;
                    problem = problem.also(field.span.clone(), "and this one");
                }
                self.out.push(
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
        }
    }
}

/// A cycle of required references back to `start`, as the fields it
/// passes through, shortest first by a breadth-first walk.
fn find_cycle<'a>(
    edges: &BTreeMap<&'a str, Vec<(&'a str, &'a FieldModel)>>,
    start: &'a str,
) -> Option<Vec<(&'a str, &'a FieldModel)>> {
    let mut queue: std::collections::VecDeque<Vec<(&'a str, &'a FieldModel)>> =
        std::collections::VecDeque::new();
    for (to, field) in edges.get(start)? {
        queue.push_back(vec![(start, *field)]);
        if *to == start {
            return Some(vec![(start, *field)]);
        }
    }
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    while let Some(path) = queue.pop_front() {
        let (from, field) = *path.last()?;
        let to = field.reference.as_ref()?.target.as_str();
        let _ = from;
        if to == start {
            return Some(path);
        }
        if !seen.insert(to) {
            continue;
        }
        for (_, next) in edges.get(to).into_iter().flatten() {
            let mut longer = path.clone();
            longer.push((to, *next));
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
    for store in schema.stores.values_mut() {
        for name in SYSTEM_TABLES {
            store
                .tables
                .insert(format!("jadpo_{name}"), columns::system_table(name));
        }
    }
    Arc::new(schema)
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
    };
    let column = |name: &str| {
        entity
            .fields
            .iter()
            .find(|f| f.name == name)
            .map_or_else(|| name.to_string(), |f| f.born.clone())
    };
    for (position, field) in (0u32..).zip(&entity.fields) {
        let Some(sql_type) = &field.sql_type else {
            continue;
        };
        out.columns.insert(
            field.born.clone(),
            physical::Column {
                position,
                field: Some(field.born.clone()),
                sql_type: sql_type.clone(),
                nullable: field.nullable,
                retired: false,
            },
        );
        if field.unique && field.name != "id" {
            out.unique.insert(
                physical::object_name(&[table, &field.born], "key"),
                physical::Unique {
                    columns: vec![field.born.clone()],
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
                    },
                );
            }
        }
    }
    for rule in &entity.unique {
        out.unique.insert(
            physical::object_name(&[table, &rule.name], "key"),
            physical::Unique {
                columns: rule.fields.iter().map(|f| column(f)).collect(),
                rule: format!("{}.{}", entity.born, rule.name),
            },
        );
    }
    for index in &entity.indexes {
        out.indexes.insert(
            physical::object_name(&[table, &index.name], "idx"),
            physical::Index {
                columns: index.fields.iter().map(|f| column(f)).collect(),
                reason: format!("index {}.{}", entity.born, index.name),
            },
        );
    }
    out
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
