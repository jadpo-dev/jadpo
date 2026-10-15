//! The graph (docs/compiler-runtime.md §2.3), built from the checker's
//! queries: every domain, file and declaration of areas 1 to 4 with its
//! stable key, place, digest and `///` description; fields and variants;
//! signatures; the `calls`, `uses` and `raises` edges between them; and
//! each action's and fixture's isolation level (area 6, chunk 3.7).
//!
//! Each file's part is a query of its own ([`file_part`]), so an edit
//! rebuilds that file's nodes and the project's graph is only assembled
//! again ([`graph`]). Declarations later areas define (entities' sections,
//! routes, schedules, subscribers, queries' bodies) have their nodes, with
//! identity, place and digest; their contents come with their areas.

mod lookup;
mod part;
pub(crate) mod paths;
mod similar;
mod text;

pub use lookup::{Found, lookup};
pub use part::{FilePart, file_part};
pub use similar::{FunctionShape, function_shapes, similar_problems};
pub use text::LineIndex;

use std::sync::Arc;

use jadpo_db::{Db, ItemKind, Project, symbols};
use jadpo_syntax::ast::Decl;

use crate::lookups::NameView;
use crate::tree;
use jadpo_graph::{Builder, Graph, Key, Kind, Node, Registry, TypeRef};

use crate::types::Ty;

/// The identity registry (area 18), from the project's input, so a rename
/// it records re-keys what it touches and nothing else. Every key is made
/// through it.
#[salsa::tracked(returns(ref))]
pub fn registry(db: &dyn Db, project: Project) -> Registry {
    Registry::from_lines(project.identities(db))
}

/// The graph kind of a declaration.
pub fn kind_of(kind: &ItemKind) -> Option<Kind> {
    Some(match kind {
        ItemKind::Type(_) => Kind::Type,
        ItemKind::Function => Kind::Function,
        ItemKind::Action => Kind::Action,
        ItemKind::Query => Kind::Query,
        ItemKind::Fixture => Kind::Fixture,
        ItemKind::Entity => Kind::Entity,
        ItemKind::Library => Kind::Library,
        ItemKind::Routes => Kind::RouteGroup,
        ItemKind::Schedule => Kind::Schedule,
        ItemKind::Subscriber => Kind::Subscriber,
        ItemKind::Seed => Kind::Seed,
        ItemKind::Test => Kind::Test,
        // Packages, resources and provider mappings come with area 11, the
        // setting blocks with areas 10, 11 and 14.
        ItemKind::Package | ItemKind::Resource | ItemKind::Provide | ItemKind::Singleton => {
            return None;
        }
    })
}

/// The key of the project declaration called `name` (not a library
/// function), when it has a node.
pub fn declaration_key(symbols: &dyn NameView, name: &str) -> Option<Key> {
    let symbol = symbols
        .declared(name)
        .iter()
        .find(|symbol| symbol.library.is_none())?;
    let kind = kind_of(&symbol.kind)?;
    Some(symbols.registry().key(kind, name, None))
}

/// The key of function `function` of library `library`.
pub fn library_function_key(registry: &Registry, library: &str, function: &str) -> Key {
    let born = registry.born(Kind::Library, library, None);
    registry.key(
        Kind::Function,
        &format!("{library}.{function}"),
        Some((library, &born)),
    )
}

/// The born name of the type or entity called `host` now.
fn born_host(registry: &Registry, host: &str) -> String {
    let as_type = registry.born(Kind::Type, host, None);
    if as_type != host {
        return as_type;
    }
    registry.born(Kind::Entity, host, None)
}

/// The key of field `path` (`Note.body`, `Outcome.paid.receipt`) under the
/// type or entity `host`.
pub fn field_key(registry: &Registry, host: &str, path: &[String]) -> Key {
    let born = born_host(registry, host);
    let now = format!("{host}.{}", path.join("."));
    registry.key(Kind::Field, &now, Some((host, &born)))
}

/// The key of variant `variant` of enum `host`.
pub fn variant_key(registry: &Registry, host: &str, variant: &str) -> Key {
    let born = born_host(registry, host);
    registry.key(
        Kind::Variant,
        &format!("{host}.{variant}"),
        Some((host, &born)),
    )
}

/// A checker type as the graph holds it: declared types, entities, fields
/// and variants by key, the standard library's by name, and a type already
/// reported as wrong as it is written.
pub fn type_ref(symbols: &dyn NameView, ty: &Ty) -> TypeRef {
    let boxed = |inner: &Ty| Box::new(type_ref(symbols, inner));
    match ty {
        Ty::Prim(builtin) => TypeRef::Builtin(*builtin),
        Ty::Named(name) => declaration_key(symbols, name)
            .map_or_else(|| TypeRef::Std(name.clone(), Vec::new()), TypeRef::Node),
        Ty::Field(host, path) => TypeRef::Field(field_key(symbols.registry(), host, path)),
        Ty::Variant(host, variant) => {
            TypeRef::Variant(variant_key(symbols.registry(), host, variant))
        }
        Ty::List(inner) => TypeRef::List(boxed(inner)),
        Ty::Set(inner) => TypeRef::Set(boxed(inner)),
        Ty::Map(key, value) => TypeRef::Map(boxed(key), boxed(value)),
        Ty::Nullable(inner) => TypeRef::Nullable(boxed(inner)),
        Ty::None => TypeRef::None,
        Ty::Opaque(name, args) => TypeRef::Std(
            name.clone(),
            args.iter().map(|arg| type_ref(symbols, arg)).collect(),
        ),
        Ty::Error | Ty::Var(_) | Ty::Unknown(_) | Ty::Number | Ty::Function(..) => {
            TypeRef::Written(ty.to_string())
        }
    }
}

/// The key of the callable declaration `decl` in the file at `path`, or of
/// its library's function `function`.
pub fn callable_key(
    symbols: &dyn NameView,
    path: &str,
    decl: &Decl,
    function: Option<u32>,
) -> Option<Key> {
    match (decl, function) {
        (Decl::LibraryDecl(library), Some(index)) => {
            let name = library.name().as_ref().and_then(tree::name_text)?;
            let function = tree::library_functions(library)
                .into_iter()
                .nth(index as usize)?;
            let function = function.name().as_ref().and_then(tree::name_text)?;
            Some(library_function_key(symbols.registry(), &name, &function))
        }
        (Decl::TestDecl(test), None) => {
            let name = test.string_token()?.text().trim_matches('"').to_string();
            Some(test_key(symbols.registry(), path, &name))
        }
        (Decl::SeedDecl(seed), None) => {
            let name = seed.name().as_ref().and_then(tree::name_text)?;
            Some(symbols.registry().key(Kind::Seed, &name, None))
        }
        (decl, None) => {
            let name = tree::decl_name(decl).as_ref().and_then(tree::name_text)?;
            declaration_key(symbols, &name)
        }
        _ => None,
    }
}

/// A test's key: the subject it sits beside (its file's name without
/// `.test`) and its sentence, with no path, so moving the domain it is in
/// keeps it (area 18, question 8; decided in the Phase 1 review).
pub(crate) fn test_key(registry: &jadpo_graph::Registry, path: &str, sentence: &str) -> Key {
    let subject = jadpo_db::locate(path, jadpo_db::Layout::Loose).stem;
    registry.key(Kind::Test, &format!("{subject} \"{sentence}\""), None)
}

/// The project's graph, shared behind `Arc` (docs/compiler-runtime.md
/// §2.3): what reads it whole, such as the interpreter's plan or watch
/// mode's last run, keeps a reference rather than a copy.
#[salsa::tracked(returns(ref))]
pub fn graph(db: &dyn Db, project: Project) -> Arc<Graph> {
    let symbols = symbols(db, project);
    let registry = registry(db, project);
    let mut builder = Builder::new();
    for domain in &symbols.domains {
        let mut node = Node::new(
            registry.key(Kind::Domain, domain, None),
            Kind::Domain,
            domain,
        );
        node.domain = Some(domain.clone());
        builder.node(node);
    }
    // In path order whatever order the files were given in, so the first
    // of two declarations with one name is always the same one.
    let mut files: Vec<_> = project.files(db).to_vec();
    files.sort_by(|a, b| a.path(db).cmp(b.path(db)));
    // Each action's and fixture's isolation follows from what it and the
    // actions it calls do, so it is set once every file is read (area 6).
    let decided = crate::types::isolation(db, project);
    for file in files {
        let part = file_part(db, project, file);
        for node in &part.nodes {
            let mut node = node.clone();
            if crate::types::has_isolation(node.kind)
                && let jadpo_graph::Detail::Callable { isolation, .. } = &mut node.detail
            {
                *isolation = decided.get(&node.key).map(|decided| decided.isolation);
            }
            builder.node(node);
        }
        for (from, kind, to) in &part.edges {
            builder.edge(from.clone(), *kind, to.clone());
        }
    }
    for node in records(db, project) {
        builder.node(node.clone());
    }
    Arc::new(builder.build())
}

/// The project's data nodes that no source file holds (area 5): the system
/// tables each store gets, and each finished migration's snapshot.
#[salsa::tracked(returns(ref))]
pub fn records(db: &dyn Db, project: Project) -> Vec<Node> {
    let registry = registry(db, project);
    let mut out = Vec::new();
    for (store_name, store) in &crate::data::schema(db, project).stores {
        for (table_name, table) in &store.tables {
            if !matches!(table.holds, jadpo_graph::schema::Holds::System(_)) {
                continue;
            }
            let name = format!("{store_name}.{table_name}");
            let mut node = Node::new(
                registry.key(Kind::SystemTable, &name, None),
                Kind::SystemTable,
                name.clone(),
            );
            let mut one = jadpo_graph::schema::Schema::default();
            let mut only = jadpo_graph::schema::Store {
                driver: store.driver.clone(),
                tables: Default::default(),
            };
            only.tables.insert(table_name.clone(), table.clone());
            one.stores.insert(store_name.clone(), only);
            node.digest = Some(jadpo_graph::Digest::of([one.to_document().as_str()]));
            node.detail = jadpo_graph::Detail::SystemTable {
                store: store_name.clone(),
                table: table_name.clone(),
            };
            out.push(node);
        }
    }
    let texts: std::collections::BTreeMap<&str, &str> = project
        .records(db)
        .iter()
        .filter_map(|r| Some((r.path.as_str(), r.text.as_ref().ok()?.as_str())))
        .collect();
    for migration in &crate::data::migrations::migrations(db, project).list {
        let path = migration.snapshot_path();
        let (Some(Ok(_)), Some(text)) = (&migration.snapshot, texts.get(path.as_str())) else {
            continue;
        };
        let mut node = Node::new(
            registry.key(Kind::MigrationSnapshot, &migration.name, None),
            Kind::MigrationSnapshot,
            migration.name.clone(),
        );
        let lines = LineIndex::new(text);
        let end = u32::try_from(text.len()).unwrap_or(u32::MAX);
        node.place = Some(jadpo_graph::Place {
            file: path.clone(),
            start: 0,
            end,
            lines: (1, lines.line(end.saturating_sub(1))),
        });
        node.digest = Some(jadpo_graph::Digest::of([*text]));
        out.push(node);
    }
    out
}
