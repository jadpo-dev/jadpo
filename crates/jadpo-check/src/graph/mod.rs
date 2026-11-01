//! The graph (docs/compiler-runtime.md §2.3), built from the checker's
//! queries: every domain, file and declaration of areas 1 to 4 with its
//! stable key, place, digest and `///` description; fields and variants;
//! signatures; the `calls`, `uses` and `raises` edges between them; and
//! each action's and fixture's isolation level (area 6, chunk 3.7).
//!
//! Each file's part is a query of its own ([`file_part`]), so an edit
//! rebuilds that file's nodes and the project's graph is only assembled
//! again ([`graph`]). The nodes of areas 8, 10 and 11 that a declaration
//! existing once holds (connections, hosts, lock lines, principals,
//! sign-in, generated routes, settings, the locale set, rate limits) are
//! queries of their own too ([`areas`]); packages, their resources,
//! operations and capability mappings, schedules and subscribers are
//! their files' parts (chunk 10.3g).

pub mod areas;
mod lookup;
mod part;
pub(crate) mod paths;
mod similar;
pub(crate) mod text;

pub(crate) use lookup::callable_at;
pub use lookup::{Found, Reference, lookup, references};
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
        ItemKind::Constant => Kind::Constant,
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
/// function), when it has a node: its kind and the born name of its
/// identity, which carries its domain for a function or type declared in a
/// domain (area 18, "Stable identity", amended 2026-10-16).
pub fn declaration_key(symbols: &dyn NameView, name: &str) -> Option<Key> {
    let symbol = symbols
        .declared(name)
        .iter()
        .find(|symbol| symbol.library.is_none())?;
    let kind = kind_of(&symbol.kind)?;
    Some(symbols.registry().key(kind, &symbol.identity(), None))
}

/// The key of a declaration of `kind` called `name` in a file owned by
/// `ownership` (not a library function).
pub fn item_key(
    registry: &Registry,
    kind: Kind,
    item: &ItemKind,
    name: &str,
    ownership: &jadpo_db::Ownership,
) -> Key {
    registry.key(
        kind,
        &jadpo_db::identity_name(name, item, false, ownership),
        None,
    )
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

/// The key of the constant `name` (area 3): one beside its use, whose
/// identity carries its domain as a function's does, or library
/// `library`'s, under its library.
pub fn constant_key(symbols: &dyn NameView, library: Option<&str>, name: &str) -> Option<Key> {
    match library {
        Some(library) => {
            let born = symbols.registry().born(Kind::Library, library, None);
            Some(symbols.registry().key(
                Kind::Constant,
                &format!("{library}.{name}"),
                Some((library, &born)),
            ))
        }
        None => {
            let symbol = symbols
                .declared(name)
                .iter()
                .find(|symbol| symbol.library.is_none() && symbol.kind == ItemKind::Constant)?;
            Some(
                symbols
                    .registry()
                    .key(Kind::Constant, &symbol.identity(), None),
            )
        }
    }
}

/// The identity of the type or entity `host` names now, and its born name:
/// a type declared in a domain carries the domain (area 18).
fn host_identity(symbols: &dyn NameView, host: &str) -> (String, String) {
    let found = symbols.declared(host).iter().find(|symbol| {
        symbol.library.is_none() && matches!(symbol.kind, ItemKind::Type(_) | ItemKind::Entity)
    });
    match found.and_then(|symbol| Some((symbol, kind_of(&symbol.kind)?))) {
        Some((symbol, kind)) => {
            let now = symbol.identity();
            let born = symbols.registry().born(kind, &now, None);
            (now, born)
        }
        None => {
            let shown = crate::types::shown(host).to_string();
            (shown.clone(), shown)
        }
    }
}

/// The key of field `path` (`Note.body`, `Outcome.paid.receipt`) under the
/// type or entity `host`.
pub fn field_key(symbols: &dyn NameView, host: &str, path: &[String]) -> Key {
    let (now, born) = host_identity(symbols, host);
    host_field_key(symbols.registry(), &now, &born, path)
}

/// The key of field `path` under the host whose identity is `now`, born
/// `born`.
fn host_field_key(registry: &Registry, now: &str, born: &str, path: &[String]) -> Key {
    let field = format!("{now}.{}", path.join("."));
    registry.key(Kind::Field, &field, Some((now, born)))
}

/// The key of field `path` of entity `entity`, whose identity is its name.
pub fn entity_field_key(registry: &Registry, entity: &str, path: &[String]) -> Key {
    let born = registry.born(Kind::Entity, entity, None);
    host_field_key(registry, entity, &born, path)
}

/// The key of rule `rule` across the fields of object type `host` (area 2,
/// "Rules across fields").
pub fn rule_key(symbols: &dyn NameView, host: &str, rule: &str) -> Key {
    let (now, born) = host_identity(symbols, host);
    symbols
        .registry()
        .key(Kind::Rule, &format!("{now}.{rule}"), Some((&now, &born)))
}

/// The key of variant `variant` of enum `host`.
pub fn variant_key(symbols: &dyn NameView, host: &str, variant: &str) -> Key {
    let (now, born) = host_identity(symbols, host);
    symbols.registry().key(
        Kind::Variant,
        &format!("{now}.{variant}"),
        Some((&now, &born)),
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
        Ty::Field(host, path) => TypeRef::Field(field_key(symbols, host, path)),
        Ty::Variant(host, variant) => TypeRef::Variant(variant_key(symbols, host, variant)),
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
            // A package's library is known through its package (area 11).
            let name = symbols.canonical(&name);
            let member = tree::library_members(library)
                .into_iter()
                .nth(index as usize)?;
            let member_name = tree::decl_name(&member)
                .as_ref()
                .and_then(tree::name_text)?;
            match member {
                Decl::ConstantDecl(_) => constant_key(symbols, Some(&name), &member_name),
                _ => Some(library_function_key(
                    symbols.registry(),
                    &name,
                    &member_name,
                )),
            }
        }
        (Decl::ConstantDecl(_), None) => {
            let name = tree::decl_name(decl).as_ref().and_then(tree::name_text)?;
            constant_key(symbols, None, &name)
        }
        (Decl::TypeDecl(ty), Some(index)) => {
            let host = ty.name().as_ref().and_then(tree::name_text)?;
            let rule = crate::types::across::rule_members(ty.member_list())
                .into_iter()
                .nth(index as usize)?;
            let name = rule.name().as_ref().and_then(tree::name_text)?;
            Some(rule_key(symbols, &host, &name))
        }
        (Decl::TestDecl(test), None) => {
            let name = test.string_token()?.text().trim_matches('"').to_string();
            Some(test_key(symbols.registry(), path, &name))
        }
        (Decl::SeedDecl(seed), None) => {
            let name = jadpo_db::seed_name(seed, path);
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
/// keeps it (area 18, "Stable identity"; decided in the Phase 1 review).
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
    for node in translation_nodes(db, project) {
        builder.node(node.clone());
    }
    for node in prompt_nodes(db, project) {
        builder.node(node.clone());
    }
    let (nodes, edges) = policy_nodes(db, project);
    for node in nodes {
        builder.node(node.clone());
    }
    for (from, kind, to) in edges {
        builder.edge(from.clone(), *kind, to.clone());
    }
    // Areas 8, 10 and 11 (chunk 10.3g).
    for (nodes, edges) in [
        areas::outside_nodes(db, project),
        areas::access_nodes(db, project),
        areas::service_nodes(db, project),
    ] {
        for node in nodes {
            builder.node(node.clone());
        }
        for (from, kind, to) in edges {
            builder.edge(from.clone(), *kind, to.clone());
        }
    }
    // Each entity's built-in purge schedule (area 12), placed at its
    // `purge:`, which deletes the entity's hidden records.
    for (model, span) in crate::schedules::purge_schedules(db, project) {
        let crate::schedules::ScheduleRun::Purge(entity) = &model.run else {
            continue;
        };
        let mut node = Node::new(model.key.clone(), Kind::Schedule, model.name.clone());
        if let Some(file) = crate::lookups::file_at(
            db,
            project,
            crate::lookups::Name::new(db, span.path.clone()),
        ) {
            let lines = LineIndex::new(file.text(db));
            let last = span.end.saturating_sub(1).max(span.start);
            node.place = Some(jadpo_graph::Place {
                file: span.path.clone(),
                start: span.start,
                end: span.end,
                lines: (lines.line(span.start), lines.line(last)),
            });
        }
        node.digest = Some(jadpo_graph::Digest::of([
            model.key.as_str(),
            &model.every_ms.to_string(),
        ]));
        let entity = registry.key(Kind::Entity, entity, None);
        node.parent = Some(entity.clone());
        node.detail = jadpo_graph::Detail::Schedule {
            every_ms: model.every_ms,
        };
        builder.edge(model.key.clone(), jadpo_graph::EdgeKind::Writes, entity);
        builder.node(node);
    }
    // The field key's built-in re-seal (area 15; chunk P9), placed at
    // `field_encryption_key:`.
    if let Some((model, span)) = crate::schedules::reseal_schedule(db, project) {
        let mut node = Node::new(model.key.clone(), Kind::Schedule, model.name.clone());
        if let Some(file) = crate::lookups::file_at(
            db,
            project,
            crate::lookups::Name::new(db, span.path.clone()),
        ) {
            let lines = LineIndex::new(file.text(db));
            let last = span.end.saturating_sub(1).max(span.start);
            node.place = Some(jadpo_graph::Place {
                file: span.path.clone(),
                start: span.start,
                end: span.end,
                lines: (lines.line(span.start), lines.line(last)),
            });
        }
        node.digest = Some(jadpo_graph::Digest::of([
            model.key.as_str(),
            &model.every_ms.to_string(),
        ]));
        node.detail = jadpo_graph::Detail::Schedule {
            every_ms: model.every_ms,
        };
        builder.node(node);
    }
    // Each person entity (area 15; chunk P1), placed at its `person:`.
    for node in person_nodes(db, project) {
        builder.node(node.clone());
    }
    // Each retention (area 15; chunk P2), placed at its `retention:`.
    for node in retention_nodes(db, project) {
        builder.node(node.clone());
    }
    // The APIs (area 7), once there is a route or `apis:` declares them.
    let apis = crate::routes::api_table(db, project);
    if has_apis(db, project) {
        for api in &apis.apis {
            let mut node = Node::new(
                registry.key(Kind::Api, &api.name, None),
                Kind::Api,
                &api.name,
            );
            node.detail = jadpo_graph::Detail::Api { prefix: api.path() };
            builder.node(node);
        }
    }
    Arc::new(builder.build())
}

/// A `person` node for each person entity (area 15, "The person"; chunk
/// P1), under its entity and keyed by its born name, placed at its
/// `person: true` and listing each reference its records belong to the
/// person through, so a reference that starts or stops making records a
/// person's changes the node.
#[salsa::tracked(returns(ref))]
pub fn person_nodes(db: &dyn Db, project: Project) -> Vec<Node> {
    let registry = registry(db, project);
    let mut out = Vec::new();
    for (entity, person) in &crate::persons::persons(db, project).people {
        let Some(read) =
            crate::data::entity_read(db, project, crate::lookups::Name::new(db, entity.clone()))
        else {
            continue;
        };
        let (true, Some(span)) = (read.model.person, &read.spans.person) else {
            continue;
        };
        let Some(file) = crate::lookups::file_at(
            db,
            project,
            crate::lookups::Name::new(db, span.path.clone()),
        ) else {
            continue;
        };
        let born = registry.born(Kind::Entity, entity, None);
        let key = Key::new(Kind::Person, &born);
        let entity_key = registry.key(Kind::Entity, entity, None);
        let through: Vec<String> = person
            .through
            .iter()
            .map(|(holder, field)| format!("{holder}.{field}"))
            .collect();
        let mut node = Node::new(key.clone(), Kind::Person, entity.clone());
        let lines = LineIndex::new(file.text(db));
        let last = span.end.saturating_sub(1).max(span.start);
        node.place = Some(jadpo_graph::Place {
            file: span.path.clone(),
            start: span.start,
            end: span.end,
            lines: (lines.line(span.start), lines.line(last)),
        });
        node.domain = file_part(db, project, file)
            .nodes
            .iter()
            .find(|found| found.key == entity_key)
            .and_then(|found| found.domain.clone());
        node.digest = Some(jadpo_graph::Digest::of(
            std::iter::once(key.as_str()).chain(through.iter().map(String::as_str)),
        ));
        node.parent = Some(entity_key);
        node.detail = jadpo_graph::Detail::Person { through };
        out.push(node);
    }
    out
}

/// A `retention` node for each `retention:` that reads cleanly (area 15,
/// "Retention"; chunk P2): an entity's under its entity, keyed by its born
/// name, and a field's under its field, keyed by the entity's and field's
/// born names, placed at the `retention:` and holding its period and
/// `since:` field, so a retention made longer, shorter or counted from
/// another field changes the node; and an audited entity's `audit` node
/// under the entity, holding how long its history is kept (area 16).
#[salsa::tracked(returns(ref))]
pub fn retention_nodes(db: &dyn Db, project: Project) -> Vec<Node> {
    let registry = registry(db, project);
    let mut out = Vec::new();
    for entity in crate::data::entity_names(db, project) {
        let Some(read) =
            crate::data::entity_read(db, project, crate::lookups::Name::new(db, entity.clone()))
        else {
            continue;
        };
        let born = registry.born(Kind::Entity, entity, None);
        let entity_key = registry.key(Kind::Entity, entity, None);
        let mut kept: Vec<(
            String,
            Key,
            &crate::data::retention::RetentionModel,
            &jadpo_diag::Span,
            bool,
        )> = Vec::new();
        if let (Some(retention), Some(span)) = (&read.model.retention, &read.spans.retention) {
            kept.push((born.clone(), entity_key.clone(), retention, span, false));
        }
        for (field, span) in &read.spans.field_retentions {
            let Some(found) = read.model.fields.iter().find(|f| f.name == *field) else {
                continue;
            };
            let Some(retention) = &found.retention else {
                continue;
            };
            let field_key = entity_field_key(registry, entity, std::slice::from_ref(field));
            let field_born = field_key
                .name()
                .rsplit('.')
                .next()
                .unwrap_or(field)
                .to_string();
            kept.push((
                format!("{born}.{field_born}"),
                field_key,
                retention,
                span,
                true,
            ));
        }
        let Some(file) = crate::lookups::file_at(
            db,
            project,
            crate::lookups::Name::new(db, read.model.path.clone()),
        ) else {
            continue;
        };
        let lines = LineIndex::new(file.text(db));
        let domain = file_part(db, project, file)
            .nodes
            .iter()
            .find(|found| found.key == entity_key)
            .and_then(|found| found.domain.clone());
        for (keyed, parent, retention, span, clears) in kept {
            let key = Key::new(Kind::Retention, &keyed);
            let name = match parent.name().split_once('.') {
                Some((_, field)) if clears => format!("{entity}.{field}"),
                _ => entity.clone(),
            };
            let mut node = Node::new(key.clone(), Kind::Retention, name);
            let last = span.end.saturating_sub(1).max(span.start);
            node.place = Some(jadpo_graph::Place {
                file: span.path.clone(),
                start: span.start,
                end: span.end,
                lines: (lines.line(span.start), lines.line(last)),
            });
            node.domain = domain.clone();
            let shown = retention.shown();
            node.digest = Some(jadpo_graph::Digest::of([
                key.as_str(),
                shown.as_str(),
                if clears { "clears" } else { "deletes" },
            ]));
            node.parent = Some(parent);
            node.detail = jadpo_graph::Detail::Retention {
                retention: shown,
                clears,
            };
            out.push(node);
        }
        // Its audit trail (area 16; chunk B11), keyed by its born name,
        // placed at its `audit:`, so starting, stopping or changing how
        // long its history is kept shows in a diff.
        if let (Some(audit), Some(span)) = (&read.model.audit, &read.spans.audit) {
            let key = Key::new(Kind::Audit, &born);
            let mut node = Node::new(key.clone(), Kind::Audit, entity.clone());
            let last = span.end.saturating_sub(1).max(span.start);
            node.place = Some(jadpo_graph::Place {
                file: span.path.clone(),
                start: span.start,
                end: span.end,
                lines: (lines.line(span.start), lines.line(last)),
            });
            node.domain = domain.clone();
            let kept = audit.shown();
            node.digest = Some(jadpo_graph::Digest::of([key.as_str(), kept.as_str()]));
            node.parent = Some(entity_key.clone());
            node.detail = jadpo_graph::Detail::Audit { kept };
            out.push(node);
        }
    }
    out
}

/// Whether the project has an HTTP surface: a route, or APIs `apis:`
/// declares (area 7).
pub(crate) fn has_apis(db: &dyn Db, project: Project) -> bool {
    crate::routes::api_table(db, project).declared
        || !crate::routes::group_names(db, project).is_empty()
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

/// Each message and enum with wording (area 10), keyed
/// `<domain>.<message>`, with a digest of its wording in every language
/// file, so an edit to any wording changes the key and reaches the tests of
/// what calls it.
#[salsa::tracked(returns(ref))]
pub fn translation_nodes(db: &dyn Db, project: Project) -> Vec<Node> {
    let registry = registry(db, project);
    let mut out = Vec::new();
    for facts in crate::translations::entry_facts(db, project) {
        let mut node = Node::new(
            registry.key(Kind::TranslationKey, &facts.key, None),
            Kind::TranslationKey,
            facts.key.clone(),
        );
        if let Some((path, span)) = &facts.place
            && let Some(file) =
                crate::lookups::file_at(db, project, crate::lookups::Name::new(db, path.clone()))
        {
            let lines = LineIndex::new(file.text(db));
            let last = span.end.saturating_sub(1).max(span.start);
            node.place = Some(jadpo_graph::Place {
                file: path.clone(),
                start: span.start,
                end: span.end,
                lines: (lines.line(span.start), lines.line(last)),
            });
        }
        node.digest = Some(jadpo_graph::Digest::of(
            std::iter::once(facts.key.as_str()).chain(facts.text.iter().map(String::as_str)),
        ));
        out.push(node);
    }
    out
}

/// Each prompt (area 21), keyed by its name, placed at its `.prompt` file,
/// with its type as its parent and its version, `answer_question@3fa9c2d1`:
/// the first 8 hex characters of a SHA-256 over its two files, which every
/// call's trace carries. Its digest covers both files, so an edit to its
/// words reaches the tests of what renders it.
#[salsa::tracked(returns(ref))]
pub fn prompt_nodes(db: &dyn Db, project: Project) -> Vec<Node> {
    let registry = registry(db, project);
    let records = project.records(db);
    let mut out = Vec::new();
    for facts in &crate::prompts::analysis(db, project).prompts {
        let Some(version) = &facts.digest else {
            continue;
        };
        let Some(text) = records
            .iter()
            .find(|record| record.path == facts.text_path)
            .and_then(|record| record.text.as_ref().ok())
        else {
            continue;
        };
        let Some(file) = crate::lookups::file_at(
            db,
            project,
            crate::lookups::Name::new(db, facts.path.clone()),
        ) else {
            continue;
        };
        let mut node = Node::new(
            registry.key(Kind::Prompt, &facts.key, None),
            Kind::Prompt,
            facts.key.clone(),
        );
        let lines = LineIndex::new(text);
        let end = u32::try_from(text.len()).unwrap_or(u32::MAX);
        node.place = Some(jadpo_graph::Place {
            file: facts.text_path.clone(),
            start: 0,
            end,
            lines: (1, lines.line(end.saturating_sub(1)).max(1)),
        });
        node.digest = Some(jadpo_graph::Digest::of([
            file.text(db).as_str(),
            text.as_str(),
        ]));
        node.domain = facts.ownership.domain().map(str::to_string);
        node.parent = Some(item_key(
            registry,
            Kind::Type,
            &ItemKind::Type(jadpo_db::Base::Prompt),
            &facts.type_name,
            &facts.ownership,
        ));
        node.doc = facts.description.clone();
        node.detail = jadpo_graph::Detail::Prompt {
            version: format!("{}@{version}", facts.key),
        };
        out.push(node);
    }
    out
}

/// Policy's nodes (area 9): each list of an entity's `policy:` as a
/// `grant`, each field's own policy as a `field_restriction`, each
/// tenant's entity's `tenancy`, and each line under root `policy {
/// actions: }` as an `action_exception`; with the `grants` edges from
/// each to the entity, field or action it decides, and the `scopes` edges
/// from each tenancy to its entity.
#[salsa::tracked(returns(ref))]
pub fn policy_nodes(
    db: &dyn Db,
    project: Project,
) -> (Vec<Node>, Vec<(Key, jadpo_graph::EdgeKind, Key)>) {
    use crate::policy::{Effect, FieldPolicy};
    use jadpo_graph::{Detail, Digest, EdgeKind};
    let registry = registry(db, project);
    let lookup = crate::lookups::Lookup::new(db, project);
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let place = |path: &str, span: &jadpo_diag::Span| -> Option<jadpo_graph::Place> {
        let file =
            crate::lookups::file_at(db, project, crate::lookups::Name::new(db, path.to_string()))?;
        let lines = LineIndex::new(file.text(db));
        let last = span.end.saturating_sub(1).max(span.start);
        Some(jadpo_graph::Place {
            file: path.to_string(),
            start: span.start,
            end: span.end,
            lines: (lines.line(span.start), lines.line(last)),
        })
    };
    let domain_of = |name: &str, kind: ItemKind| {
        lookup
            .declared(name)
            .iter()
            .find(|symbol| symbol.kind == kind)
            .and_then(|symbol| match &symbol.ownership {
                jadpo_db::Ownership::Domain(domain) => Some(domain.clone()),
                _ => None,
            })
    };
    for name in crate::data::entity_names(db, project) {
        let Some(read) =
            crate::policy::policy_read(db, project, crate::lookups::Name::new(db, name.clone()))
        else {
            continue;
        };
        let Some(entity) = declaration_key(&lookup, name) else {
            continue;
        };
        let born = registry.born(Kind::Entity, name, None);
        let domain = domain_of(name, ItemKind::Entity);
        let at = |word: &str| {
            read.places
                .iter()
                .find(|(part, _)| part == word)
                .and_then(|(_, span)| place(&read.path, span))
        };
        let policy = &read.model;
        for effect in Effect::ALL {
            let Some(list) = policy.lists.get(&effect) else {
                continue;
            };
            let now = format!("{name}.{}", effect.word());
            let mut node = Node::new(
                registry.key(Kind::Grant, &now, Some((name, &born))),
                Kind::Grant,
                now,
            );
            node.place = at(effect.word());
            node.digest = Some(Digest::of(
                std::iter::once(effect.word()).chain(list.written.iter().map(String::as_str)),
            ));
            node.domain = domain.clone();
            node.parent = Some(entity.clone());
            node.detail = Detail::Grant {
                effect: effect.word().to_string(),
                who: list.written.clone(),
            };
            edges.push((node.key.clone(), EdgeKind::Grants, entity.clone()));
            nodes.push(node);
        }
        for field in &policy.fields {
            let now = format!("{name}.{}", field.field);
            let mut node = Node::new(
                registry.key(Kind::FieldRestriction, &now, Some((name, &born))),
                Kind::FieldRestriction,
                now,
            );
            let grants: Vec<(String, Vec<String>)> = FieldPolicy::EFFECTS
                .iter()
                .filter_map(|effect| {
                    field
                        .lists
                        .get(effect)
                        .map(|list| (effect.word().to_string(), list.written.clone()))
                })
                .collect();
            let mut parts: Vec<String> = vec![if field.server_only {
                "server_only".to_string()
            } else {
                String::new()
            }];
            for (effect, who) in &grants {
                parts.push(effect.clone());
                parts.extend(who.iter().cloned());
            }
            node.place = at(&format!("field {}", field.field));
            node.digest = Some(Digest::of(parts.iter().map(String::as_str)));
            node.domain = domain.clone();
            let target = entity_field_key(registry, name, std::slice::from_ref(&field.field));
            node.parent = Some(target.clone());
            node.detail = Detail::FieldRestriction {
                server_only: field.server_only,
                grants,
            };
            edges.push((node.key.clone(), EdgeKind::Grants, target));
            nodes.push(node);
        }
        if let Some(tenancy) = &policy.tenancy
            && let Some(tenant) = declaration_key(&lookup, &tenancy.entity)
        {
            let mut node = Node::new(
                registry.key(Kind::Tenancy, name, Some((name, &born))),
                Kind::Tenancy,
                name.clone(),
            );
            // Where `tenant:` is written, or else the reference it
            // follows, or the tenant's own name.
            node.place = at("tenant").or_else(|| {
                let entity_read = crate::data::entity_read(
                    db,
                    project,
                    crate::lookups::Name::new(db, name.clone()),
                )
                .as_ref()?;
                let span = match entity_read
                    .model
                    .fields
                    .iter()
                    .position(|f| f.name == tenancy.via)
                {
                    Some(at) if tenancy.via != "id" => entity_read.spans.fields.get(at)?,
                    _ => &entity_read.spans.name,
                };
                place(&entity_read.model.path, span)
            });
            node.digest = Some(Digest::of([tenancy.entity.as_str(), tenancy.via.as_str()]));
            node.domain = domain.clone();
            node.parent = Some(entity.clone());
            node.detail = Detail::Tenancy {
                tenant,
                via: tenancy.via.clone(),
            };
            edges.push((node.key.clone(), EdgeKind::Scopes, entity.clone()));
            nodes.push(node);
        }
    }
    let root = crate::policy::root_read(db, project);
    // Each line under `actions:`, an action's or a query's (area 9,
    // "Listing a query"), grants the declaration it names.
    let lines = root
        .model
        .actions
        .iter()
        .map(|line| (line, ItemKind::Action))
        .chain(
            root.model
                .queries
                .iter()
                .map(|line| (line, ItemKind::Query)),
        );
    for (line, item) in lines {
        let granted = match item {
            ItemKind::Action => declaration_key(&lookup, &line.action),
            _ => lookup
                .declared(&line.action)
                .iter()
                .find(|symbol| symbol.kind == item && symbol.library.is_none())
                .and_then(|symbol| {
                    Some(
                        lookup
                            .registry()
                            .key(kind_of(&symbol.kind)?, &symbol.identity(), None),
                    )
                }),
        };
        let Some(action) = granted else {
            continue;
        };
        let mut node = Node::new(
            registry.key(Kind::ActionException, &line.written, None),
            Kind::ActionException,
            line.written.clone(),
        );
        node.place = match (
            &root.path,
            root.places.iter().find(|(a, _)| *a == line.action),
        ) {
            (Some(path), Some((_, span))) => place(path, span),
            _ => None,
        };
        node.digest = Some(Digest::of(
            std::iter::once(line.written.as_str())
                .chain(line.written_who.iter().map(String::as_str)),
        ));
        node.domain = domain_of(&line.action, item);
        node.detail = Detail::ActionException {
            who: line.written_who.clone(),
        };
        edges.push((node.key.clone(), EdgeKind::Grants, action));
        nodes.push(node);
    }
    (nodes, edges)
}
