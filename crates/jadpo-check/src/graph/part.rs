//! One file's part of the graph: its own node, its declarations and their
//! fields and variants, and the edges that start at them.

use std::collections::{BTreeMap, BTreeSet};

use jadpo_db::{Db, File, Item, Project, item_tree, location};
use jadpo_graph::body::Refers;
use jadpo_graph::{
    Base, Detail, EdgeKind, FailureDetail, FieldDefault, Key, Kind, Node, Section, Signature,
    TypeRef, catalogue::status_of,
};
use jadpo_syntax::SyntaxNode;
use jadpo_syntax::ast::{self, AstNode, Decl, MemberItem};

use super::paths::Resolver;
use super::text::{LineIndex, digest, doc, place};
use super::{
    declaration_key, entity_field_key, field_key, item_key, library_function_key, type_ref,
    variant_key,
};
use crate::lookups::{FailureView, Lookup, Name, NameView, SigView};
use crate::tree;
use crate::types::{
    Callable, Const, Def, FieldDef, FieldsBase, Rules, Sig, Ty, TypeView, typed_body,
};

/// The nodes a file declares and the edges that start at them.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct FilePart {
    pub nodes: Vec<Node>,
    pub edges: Vec<(Key, EdgeKind, Key)>,
}

#[salsa::tracked(returns(ref))]
pub fn file_part(db: &dyn Db, project: Project, file: File) -> FilePart {
    let Some(source) = tree::source(db, file) else {
        return FilePart::default();
    };
    let lookup = Lookup::for_file(db, project, file);
    let mut part = Part {
        db,
        project,
        file,
        path: file.path(db),
        lines: LineIndex::new(file.text(db)),
        domain: location(db, project, file)
            .ownership
            .domain()
            .map(str::to_string),
        ownership: location(db, project, file).ownership.clone(),
        resolver: Resolver {
            symbols: &lookup,
            table: &lookup,
        },
        lookup: &lookup,
        items: &item_tree(db, file).items,
        out: FilePart::default(),
    };
    let mut node = part.node(
        lookup.registry().key(Kind::File, part.path, None),
        Kind::File,
        part.path.clone(),
        source.syntax().clone(),
    );
    let text = file.text(db);
    node.place = Some(jadpo_graph::Place {
        file: part.path.clone(),
        start: 0,
        end: text.len() as u32,
        lines: (1, part.lines.line((text.len() as u32).saturating_sub(1))),
    });
    node.doc = None;
    part.out.nodes.push(node);
    for (index, decl) in source.decls().enumerate() {
        part.decl(&decl, index);
    }
    part.out
}

/// A type declaration, for its key: every base keys the same way.
const TYPE: jadpo_db::ItemKind = jadpo_db::ItemKind::Type(jadpo_db::Base::Object);

struct Part<'a> {
    db: &'a dyn Db,
    project: Project,
    file: File,
    path: &'a String,
    lines: LineIndex,
    domain: Option<String>,
    /// Who owns the file: a domain, or a package whose declarations' keys
    /// carry its name (area 11).
    ownership: jadpo_db::Ownership,
    resolver: Resolver<'a>,
    lookup: &'a Lookup<'a>,
    items: &'a [Item],
    out: FilePart,
}

impl Part<'_> {
    /// The key of this file's declaration `name` of `item`, which carries
    /// the file's domain for a function or type (area 18).
    fn item_key(&self, kind: Kind, item: &jadpo_db::ItemKind, name: &str) -> Key {
        let ownership = match (&self.ownership, &self.domain) {
            (jadpo_db::Ownership::Package(_), _) => self.ownership.clone(),
            (_, Some(domain)) => jadpo_db::Ownership::Domain(domain.clone()),
            (_, None) => jadpo_db::Ownership::Root,
        };
        item_key(self.lookup.registry(), kind, item, name, &ownership)
    }

    /// A type as the graph holds it: canonically when it resolved, as
    /// written otherwise.
    fn type_of(&self, ty: Option<&Ty>, written: &str) -> TypeRef {
        match ty {
            Some(ty) if !ty.has_error() => type_ref(self.lookup, ty),
            _ => TypeRef::Written(written.to_string()),
        }
    }

    fn signature(&self, sig: Option<&Sig>, written: &jadpo_db::Signature) -> Signature {
        Signature {
            params: written
                .params
                .iter()
                .enumerate()
                .map(|(i, param)| {
                    let resolved = sig.and_then(|sig| sig.params.get(i)).map(|(_, ty)| ty);
                    (param.name.clone(), self.type_of(resolved, &param.ty))
                })
                .collect(),
            result: match &written.result {
                Some(result) => self.type_of(sig.map(|sig| &sig.result), result),
                None => TypeRef::Builtin(jadpo_std::Builtin::Unit),
            },
            fails: written
                .fails
                .iter()
                .map(|failure| match declaration_key(self.lookup, failure) {
                    Some(key) if self.lookup.is_declared_failure(failure) => TypeRef::Node(key),
                    _ => TypeRef::Written(failure.clone()),
                })
                .collect(),
        }
    }

    /// A node for `syntax`, with its place, digest and description.
    fn node(&self, key: Key, kind: Kind, name: String, syntax: SyntaxNode) -> Node {
        let mut node = Node::new(key, kind, name);
        node.place = Some(place(self.path, &self.lines, &syntax));
        node.digest = Some(digest(&syntax));
        node.domain = self.domain.clone();
        node.doc = doc(&syntax);
        node
    }

    /// Edges from `from`; one back to itself is recursion, and kept.
    fn edges(&mut self, from: &Key, kind: EdgeKind, to: BTreeSet<Key>) {
        for to in to {
            self.out.edges.push((from.clone(), kind, to));
        }
    }

    fn decl(&mut self, decl: &Decl, index: usize) {
        let item = self.items.iter().find(|item| item.index as usize == index);
        match decl {
            Decl::TypeDecl(d) => self.type_decl(d, index),
            Decl::EntityDecl(d) => self.entity(d),
            Decl::FunctionDecl(_)
            | Decl::ActionDecl(_)
            | Decl::QueryDecl(_)
            | Decl::FixtureDecl(_) => {
                if let Some(item) = item {
                    self.callable(decl.syntax(), item, public(decl));
                }
            }
            Decl::LibraryDecl(d) => {
                if let Some(item) = item {
                    self.library(d, item);
                }
            }
            Decl::ConstantDecl(_) => {
                if let Some(item) = item {
                    let key = self.item_key(Kind::Constant, &item.kind, &item.name);
                    self.constant(key, item.name.clone(), decl.syntax(), item, None, None);
                }
            }
            Decl::RoutesDecl(group) => {
                if let Some(item) = item
                    && let Some(kind) = super::kind_of(&item.kind)
                {
                    let key = self.lookup.registry().key(kind, &item.name, None);
                    let node =
                        self.node(key.clone(), kind, item.name.clone(), decl.syntax().clone());
                    self.out.nodes.push(node);
                    self.routes(group, item, &key);
                }
            }
            Decl::ScheduleDecl(_) | Decl::SubscriberDecl(_) => {
                if let Some(item) = item
                    && let Some(kind) = super::kind_of(&item.kind)
                {
                    // A package's own subscriber carries its package's
                    // name, as its model's key does (chunk 7.6b).
                    let key = self.item_key(kind, &item.kind, &item.name);
                    let name = key.name().to_string();
                    let node = self.node(key.clone(), kind, name, decl.syntax().clone());
                    self.out.nodes.push(node);
                    self.trigger(kind, &key, &item.name);
                }
            }
            Decl::SeedDecl(d) => {
                if let Some(name) = d.name().as_ref().and_then(tree::name_text) {
                    let key = self.lookup.registry().key(Kind::Seed, &name, None);
                    self.runnable(key, Kind::Seed, name, d.syntax(), index);
                }
            }
            Decl::TestDecl(d) => {
                if let Some(token) = d.string_token() {
                    let name = token.text().trim_matches('"').to_string();
                    let key = super::test_key(self.lookup.registry(), self.path, &name);
                    self.runnable(key, Kind::Test, name, d.syntax(), index);
                }
            }
            Decl::PackageDecl(d) => self.package(d),
            Decl::ResourceDecl(d) => self.resource(d),
            Decl::ProvideDecl(d) => self.mapping(d),
            Decl::SingletonDecl(_) => {}
        }
    }

    /// A schedule's or subscriber's detail and what it runs, hears or
    /// deletes, from its model (area 12).
    fn trigger(&mut self, kind: Kind, key: &Key, name: &str) {
        let Some(node) = self.out.nodes.last_mut() else {
            return;
        };
        if kind == Kind::Schedule {
            let Some(model) = crate::schedules::schedules_in(self.db, self.project, self.file)
                .iter()
                .find(|read| read.name == name)
                .and_then(|read| read.model.as_ref())
            else {
                return;
            };
            node.detail = Detail::Schedule {
                every_ms: model.every_ms,
            };
            if let Some(action) = model.run.action() {
                self.out
                    .edges
                    .push((key.clone(), EdgeKind::Runs, action.clone()));
            }
            return;
        }
        let Some(model) = crate::events::subscribers_in(self.db, self.project, self.file)
            .iter()
            .filter_map(|read| read.model.as_ref())
            .find(|model| model.key == *key)
        else {
            return;
        };
        node.detail = Detail::Subscriber {
            attempts: model.retry.attempts,
            within_ms: model.retry.within_ms,
            tenant_field: match &model.tenant {
                crate::events::Tenant::Field(field) => Some(field.clone()),
                crate::events::Tenant::Untenanted | crate::events::Tenant::None => None,
            },
        };
        self.out
            .edges
            .push((key.clone(), EdgeKind::Subscribes, model.event.clone()));
        self.out
            .edges
            .push((key.clone(), EdgeKind::Runs, model.run.clone()));
    }

    /// The package this file belongs to, when it is a package's.
    fn home(&self) -> Option<&jadpo_db::PackageHome> {
        match &self.ownership {
            jadpo_db::Ownership::Package(home) => Some(home),
            _ => None,
        }
    }

    fn package_key(&self, home: &jadpo_db::PackageHome) -> Key {
        self.lookup
            .registry()
            .key(Kind::Package, &home.through, None)
    }

    /// A package's index (area 11): the package, keyed by the name its
    /// declarations are written through.
    fn package(&mut self, decl: &ast::PackageDecl) {
        let Some(home) = self.home().cloned() else {
            return;
        };
        let key = self.package_key(&home);
        let mut node = self.node(
            key,
            Kind::Package,
            home.through.clone(),
            decl.syntax().clone(),
        );
        let provider = crate::connections::reach(self.db, self.project, &home).is_some();
        node.detail = Detail::Package {
            version: home.version.clone(),
            package_kind: if provider { "provider" } else { "library" }.to_string(),
            installed: home.installed,
        };
        self.out.nodes.push(node);
    }

    /// A provider package's resource and its operations (area 11), each
    /// with the failures it answers with and the types it takes and gives.
    fn resource(&mut self, decl: &ast::ResourceDecl) {
        let Some(home) = self.home().cloned() else {
            return;
        };
        let Some(resource) = decl.name().as_ref().and_then(tree::name_text) else {
            return;
        };
        let registry = self.lookup.registry();
        let key = registry.key(
            Kind::Resource,
            &format!("{}.{resource}", home.through),
            None,
        );
        let mut node = self.node(
            key.clone(),
            Kind::Resource,
            key.name().to_string(),
            decl.syntax().clone(),
        );
        node.public = decl.visibility().is_some();
        node.parent = Some(self.package_key(&home));
        self.out.nodes.push(node);
        let operations = crate::providers::operations(self.db, self.project);
        for (name, member) in tree::members(decl.member_list()) {
            let Some(operation) = tree::name_text(&name) else {
                continue;
            };
            let op_key = super::areas::operation_key(
                self.db,
                self.project,
                &home.through,
                &format!("{resource}.{operation}"),
            );
            let mut node = self.node(
                op_key.clone(),
                Kind::Operation,
                op_key.name().to_string(),
                member.syntax().clone(),
            );
            node.parent = Some(key.clone());
            node.public = decl.visibility().is_some();
            if let Some(read) = operations.find(&home.folder, &resource, &operation) {
                let input = read.input.as_ref().map(|(ty, _)| type_ref(self.lookup, ty));
                let output = type_ref(self.lookup, &read.output);
                let mut uses: BTreeSet<Key> = BTreeSet::new();
                for ty in input.iter().chain(std::iter::once(&output)) {
                    uses.extend(ty.keys().into_iter().cloned());
                }
                let raises: BTreeSet<Key> = read
                    .failures
                    .iter()
                    .filter_map(|failure| declaration_key(self.lookup, failure))
                    .collect();
                node.detail = Detail::Operation {
                    method: read.method.word().to_string(),
                    path: read.written_path(),
                    input,
                    output,
                    changes: read.changes,
                    idempotent: read.idempotent,
                };
                self.edges(&op_key, EdgeKind::Uses, uses);
                self.edges(&op_key, EdgeKind::Raises, raises);
            }
            self.out.nodes.push(node);
        }
    }

    /// A provider package's `provide <capability>` (area 11,
    /// "Capabilities"): the mapping its package provides, calling each
    /// operation a `call:` in it names.
    fn mapping(&mut self, decl: &ast::ProvideDecl) {
        let Some(home) = self.home().cloned() else {
            return;
        };
        let Some(capability) = decl.name().as_ref().and_then(tree::name_text) else {
            return;
        };
        let key = self.lookup.registry().key(
            Kind::CapabilityMapping,
            &format!("{}.{capability}", home.through),
            None,
        );
        let mut node = self.node(
            key.clone(),
            Kind::CapabilityMapping,
            key.name().to_string(),
            decl.syntax().clone(),
        );
        let package = self.package_key(&home);
        node.parent = Some(package.clone());
        node.detail = Detail::CapabilityMapping {
            capability: capability.clone(),
        };
        self.out.nodes.push(node);
        self.out
            .edges
            .push((package, EdgeKind::Provides, key.clone()));
        let calls: BTreeSet<Key> = decl
            .syntax()
            .descendants()
            .filter_map(ast::Member::cast)
            .filter(|member| {
                member
                    .member_key()
                    .map(|key| tree::written(key.syntax()))
                    .as_deref()
                    == Some("call")
            })
            .filter_map(|member| member.value())
            .map(|value| tree::written(value.syntax()))
            .map(|operation| {
                super::areas::operation_key(self.db, self.project, &home.through, &operation)
            })
            .collect();
        self.edges(&key, EdgeKind::Calls, calls);
    }

    /// A group's routes (area 7), each a node under it with what it runs
    /// and the types it takes and sends; only the routes that read
    /// cleanly, from the group's model.
    fn routes(&mut self, decl: &ast::RoutesDecl, item: &Item, group: &Key) {
        let Some(read) =
            crate::routes::group_read(self.db, self.project, Name::new(self.db, item.name.clone()))
                .as_ref()
        else {
            return;
        };
        let Some(model) = &read.model else { return };
        if read.path != *self.path {
            return;
        }
        let entries: Vec<ast::RouteEntry> = decl
            .member_list()
            .iter()
            .flat_map(|list| list.member_items())
            .filter_map(|member| match member {
                MemberItem::RouteEntry(entry) => Some(entry),
                _ => None,
            })
            .collect();
        for route in &model.routes {
            let Some((_, span)) = read.spans.iter().find(|(label, _)| *label == route.label())
            else {
                continue;
            };
            let Some(entry) = entries.iter().find(|entry| {
                let range = entry.syntax().text_range();
                u32::from(range.start()) <= span.start && span.start < u32::from(range.end())
            }) else {
                continue;
            };
            let mut node = self.node(
                route.key.clone(),
                Kind::Route,
                route.label(),
                entry.syntax().clone(),
            );
            node.parent = Some(group.clone());
            let input = route
                .input
                .as_ref()
                .and_then(|input| declaration_key(self.lookup, input));
            let output = route.output.as_ref().map(|ty| type_ref(self.lookup, ty));
            node.detail = Detail::Route {
                api: model.api.clone(),
                status: route.status.code(),
                input: input.clone().map(TypeRef::Node),
                output: output.clone(),
                deadline_ms: route.deadline_ms,
            };
            self.out.nodes.push(node);
            if let Some(callee) = declaration_key(self.lookup, &route.run.name) {
                self.out
                    .edges
                    .push((route.key.clone(), EdgeKind::Runs, callee));
            }
            // The query it answers with once its work settles (chunk 6.3).
            if let Some(settle) = &route.settle
                && let Some(answer) = declaration_key(self.lookup, &settle.answer.name)
            {
                self.out
                    .edges
                    .push((route.key.clone(), EdgeKind::Runs, answer));
            }
            let mut uses: BTreeSet<Key> = BTreeSet::new();
            uses.extend(input);
            if let Some(output) = &output {
                uses.extend(output.keys().into_iter().cloned());
            }
            for (_, ty) in route.params() {
                uses.extend(type_ref(self.lookup, ty).keys().into_iter().cloned());
            }
            self.edges(&route.key, EdgeKind::Uses, uses);
        }
    }

    /// A test or a seed: statements that call actions.
    fn runnable(&mut self, key: Key, kind: Kind, name: String, syntax: &SyntaxNode, index: usize) {
        let node = self.node(key.clone(), kind, name, syntax.clone());
        self.out.nodes.push(node);
        self.body(&key, index as u32, None);
    }

    fn callable(&mut self, syntax: &SyntaxNode, item: &Item, public: bool) {
        let Some(kind) = super::kind_of(&item.kind) else {
            return;
        };
        let key = self.item_key(kind, &item.kind, &item.name);
        let mut node = self.node(key.clone(), kind, item.name.clone(), syntax.clone());
        node.public = public;
        node.detail = Detail::Callable {
            signature: self.signature(self.lookup.signature(&item.name), &item.signature),
            isolation: None,
        };
        self.out.nodes.push(node);
        if kind == Kind::Query {
            // A query is a declarative node, not a body (§2.3): what its
            // block names is read from the block, and what it reads,
            // includes and raises from its model (area 6).
            let uses = self.resolver.uses(syntax, &|_| false);
            self.edges(&key, EdgeKind::Uses, uses);
            self.query(&key, &item.name);
        } else {
            self.body(&key, item.index, None);
        }
    }

    /// A query's `reads`, `includes` and `raises` edges, from its model.
    fn query(&mut self, key: &Key, name: &str) {
        let name = Name::new(self.db, name.to_string());
        let Some(query) = crate::data::queries::query_model(self.db, self.project, name)
            .as_deref()
            .filter(|query| query.key == *key)
        else {
            return;
        };
        let entity = |name: &str| declaration_key(self.lookup, name);
        let reads: BTreeSet<Key> = entity(&query.entity).into_iter().collect();
        let mut includes = BTreeSet::new();
        let mut stack: Vec<&crate::data::queries::Include> = query.includes.iter().collect();
        while let Some(include) = stack.pop() {
            includes.extend(entity(&include.entity));
            stack.extend(include.includes.iter());
        }
        let raises: BTreeSet<Key> = query
            .missing
            .iter()
            .filter_map(|missing| declaration_key(self.lookup, &missing.failure))
            .collect();
        self.edges(key, EdgeKind::Reads, reads);
        self.edges(key, EdgeKind::Includes, includes);
        self.edges(key, EdgeKind::Raises, raises);
    }

    fn library(&mut self, decl: &ast::LibraryDecl, item: &Item) {
        // A package's library is known through its package (area 11).
        let library = self.lookup.canonical_name(&item.name);
        let key = self.lookup.registry().key(Kind::Library, &library, None);
        let mut node = self.node(
            key.clone(),
            Kind::Library,
            item.name.clone(),
            decl.syntax().clone(),
        );
        node.public = item.public;
        self.out.nodes.push(node);
        for (function, member) in tree::library_members(decl)
            .iter()
            .zip(item.functions.iter())
        {
            if member.kind == jadpo_db::ItemKind::Constant {
                let Some(child) = super::constant_key(self.lookup, Some(&library), &member.name)
                else {
                    continue;
                };
                self.constant(
                    child,
                    format!("{library}.{}", member.name),
                    function.syntax(),
                    item,
                    Some(member),
                    Some(&key),
                );
                continue;
            }
            let child = library_function_key(self.lookup.registry(), &library, &member.name);
            let mut node = self.node(
                child.clone(),
                Kind::Function,
                format!("{library}.{}", member.name),
                function.syntax().clone(),
            );
            node.parent = Some(key.clone());
            node.public = item.public;
            node.detail = Detail::Callable {
                signature: self.signature(
                    self.lookup.library_signature(&item.name, &member.name),
                    &member.signature,
                ),
                isolation: None,
            };
            self.out.nodes.push(node);
            self.body(&child, item.index, Some(member.index));
        }
    }

    /// A constant (area 3): its type and the value the compiler worked
    /// out, and what its value reads, from its typed body.
    fn constant(
        &mut self,
        key: Key,
        name: String,
        syntax: &SyntaxNode,
        item: &Item,
        member: Option<&Item>,
        library: Option<&Key>,
    ) {
        let declared = member.unwrap_or(item);
        let written = declared.signature.result.clone().unwrap_or_default();
        let ty = match &declared.signature.result {
            Some(result) => {
                let canonical = |written: &str| NameView::canonical(self.lookup, written);
                let resolved = self
                    .lookup
                    .resolve(&crate::types::TypeRef::parse(result).scoped(&canonical));
                self.type_of(Some(&resolved), &written)
            }
            None => TypeRef::Written(written),
        };
        let value = crate::constants::evaluated(
            self.db,
            self.project,
            self.path,
            item.index,
            member.map(|member| member.index),
        )
        .and_then(|evaluated| evaluated.value.as_ref())
        .map(crate::constants::Value::written)
        .unwrap_or_default();
        let mut node = self.node(key.clone(), Kind::Constant, name, syntax.clone());
        node.public = item.public;
        node.parent = library.cloned();
        node.detail = Detail::Constant { ty, value };
        self.out.nodes.push(node);
        self.body(&key, item.index, member.map(|member| member.index));
    }

    /// What a callable's typed body says: the types it uses, what it calls
    /// and the failures that can leave it, as edges, and its sites and
    /// branches, as nodes numbered per build.
    fn body(&mut self, key: &Key, index: u32, function: Option<u32>) {
        let callable = Callable::new(self.db, self.file, index, function);
        let checked = typed_body(self.db, self.project, callable);
        let body = &checked.body;
        let uses: BTreeSet<Key> = body
            .names
            .iter()
            .filter_map(|name| match &name.refers {
                Refers::Node { key, uses: true } => Some(key.clone()),
                _ => None,
            })
            .collect();
        let calls: BTreeSet<Key> = body
            .calls()
            .filter(|(_, callee)| callee.is_callable())
            .filter_map(|(_, callee)| callee.key().cloned())
            .chain(body.requests().cloned())
            // A test running a schedule calls the action it runs as well.
            .chain(body.calls().filter_map(|(_, callee)| match callee {
                jadpo_graph::body::Callee::Schedule { action, .. } => Some(action.clone()),
                _ => None,
            }))
            .collect();
        // An entity's `get` reads it (area 6).
        let reads: BTreeSet<Key> = body
            .calls()
            .filter_map(|(_, callee)| match callee {
                jadpo_graph::body::Callee::Entity { entity, operation } if operation == "get" => {
                    Some(entity.clone())
                }
                _ => None,
            })
            .collect();
        // Its `create`, `update` and `delete` write the entity (area 6).
        let writes: BTreeSet<Key> = body.writes().map(|(_, entity, _)| entity.clone()).collect();
        // The events it emits (area 12), by their types.
        let emits: BTreeSet<Key> = body
            .sites
            .iter()
            .filter_map(|site| match &site.node {
                jadpo_graph::body::Node::Stmt(jadpo_graph::body::Stmt::Emit {
                    value: Some(value),
                }) => match &body.site(*value).ty {
                    TypeRef::Node(event) => Some(event.clone()),
                    _ => None,
                },
                _ => None,
            })
            .collect();
        // A provider operation it calls through a connection (area 11),
        // or a package's own action through its own; and the connection.
        let mut calls = calls;
        let mut connections: BTreeSet<String> = BTreeSet::new();
        let through = self.home().map(|home| home.through.clone());
        for (_, callee) in body.calls() {
            match callee {
                jadpo_graph::body::Callee::Provider {
                    connection,
                    operation,
                } => {
                    let package = if connection == jadpo_graph::body::OWN_CONNECTION {
                        through.clone()
                    } else {
                        connections.insert(connection.clone());
                        super::areas::connection_packages(self.db, self.project)
                            .get(connection)
                            .cloned()
                    };
                    if let Some(package) = package {
                        calls.insert(super::areas::operation_key(
                            self.db,
                            self.project,
                            &package,
                            operation,
                        ));
                    }
                }
                jadpo_graph::body::Callee::PackageAction { connection, .. }
                | jadpo_graph::body::Callee::Files { connection, .. } => {
                    connections.insert(connection.clone());
                }
                _ => {}
            }
        }
        // The translation keys it shows people (area 10): a change to one
        // reaches it.
        let registry = self.lookup.registry();
        let mut uses = uses;
        uses.extend(body.calls().filter_map(|(_, callee)| match callee {
            jadpo_graph::body::Callee::Translation(name) => {
                Some(registry.key(Kind::TranslationKey, name, None))
            }
            // The prompts it renders (area 21): a change to one's text
            // reaches it.
            jadpo_graph::body::Callee::Prompt(name) => Some(registry.key(Kind::Prompt, name, None)),
            _ => None,
        }));
        // The settings it reads (area 10), and the connections it goes
        // through (area 11).
        uses.extend(body.sites.iter().filter_map(|site| match &site.node {
            jadpo_graph::body::Node::Expr(jadpo_graph::body::Expr::Config { field }) => {
                Some(field.clone())
            }
            _ => None,
        }));
        uses.extend(
            connections
                .iter()
                .filter(|name| !name.is_empty())
                .map(|name| registry.key(Kind::Connection, name, None)),
        );
        self.edges(key, EdgeKind::Uses, uses);
        self.edges(key, EdgeKind::Calls, calls);
        self.edges(key, EdgeKind::Reads, reads);
        self.edges(key, EdgeKind::Writes, writes);
        self.edges(key, EdgeKind::Emits, emits);
        if let Some((_, set)) = &checked.set {
            let raises: BTreeSet<Key> = set
                .failures
                .iter()
                .filter_map(|failure| declaration_key(self.lookup, failure))
                .collect();
            self.edges(key, EdgeKind::Raises, raises);
        }
        for (number, site) in body.sites.iter().enumerate() {
            if site.is_fault_site() {
                self.per_build(Kind::Site, key, number, site.start, site.end);
            }
        }
        for (number, branch) in body.branches.iter().enumerate() {
            self.per_build(Kind::Branch, key, number, branch.start, branch.end);
        }
    }

    /// A site or branch of `parent`, by its number in this build (area 18).
    fn per_build(&mut self, kind: Kind, parent: &Key, number: usize, start: u32, end: u32) {
        let name = format!("{}.{number}", parent.name());
        let mut node = Node::new(Key::new(kind, &name), kind, name);
        let last = end.saturating_sub(1).max(start);
        node.place = Some(jadpo_graph::Place {
            file: self.path.clone(),
            start,
            end,
            lines: (self.lines.line(start), self.lines.line(last)),
        });
        node.domain = self.domain.clone();
        node.parent = Some(parent.clone());
        self.out.nodes.push(node);
    }

    fn type_decl(&mut self, decl: &ast::TypeDecl, index: usize) {
        let Some(name) = decl.name().as_ref().and_then(tree::name_text) else {
            return;
        };
        let key = self.item_key(Kind::Type, &TYPE, &name);
        let mut node = self.node(key.clone(), Kind::Type, name.clone(), decl.syntax().clone());
        node.public = decl.visibility().is_some();
        let written_base = decl
            .base()
            .map(|base| tree::written(base.syntax()))
            .unwrap_or_default();
        let def = self.lookup.def(&name);
        let base = match def {
            Some(Def::Scalar { parent, .. }) if !parent.has_error() => {
                Base::Refines(type_ref(self.lookup, parent))
            }
            Some(Def::Fields { base, .. }) => fields_base(*base),
            Some(Def::Enum { .. }) => Base::Enum,
            _ => match written_base.as_str() {
                "Object" => Base::Object,
                "Enum" => Base::Enum,
                "Input" => Base::Input,
                "Output" => Base::Output,
                "Failure" => Base::Failure,
                "Event" => Base::Event,
                "EmailMessage" => Base::EmailMessage,
                "EmailPartial" => Base::EmailPartial,
                "Message" => Base::Message,
                "Prompt" => Base::Prompt,
                _ => Base::Refines(TypeRef::Written(written_base.clone())),
            },
        };
        let rules = match def {
            Some(Def::Scalar { rules, .. }) => (**rules).clone(),
            _ => Rules::default(),
        };
        let failure = (base == Base::Failure).then(|| {
            let info = self.lookup.failure_info(&name);
            let kind = info.and_then(|info| info.kind.clone());
            FailureDetail {
                status: kind.as_deref().and_then(status_of),
                kind,
                code: info.map_or_else(|| crate::case::to_snake(&name), |info| info.code.clone()),
                message: tree::members(decl.member_list())
                    .into_iter()
                    .find(|(key, _)| tree::name_text(key).as_deref() == Some("message"))
                    .and_then(
                        |(_, member)| match member.value().map(|v| Const::from_expr(&v)) {
                            Some(Const::Str(text)) => Some(text),
                            _ => None,
                        },
                    ),
            }
        });
        node.detail = Detail::Type {
            base: base.clone(),
            rules,
            failure,
        };
        self.out.nodes.push(node);

        // Fields and variants are nodes of their own, so the type's `uses`
        // leave them out.
        let mut children: Vec<SyntaxNode> = Vec::new();
        match base {
            Base::Failure => {
                let mut order = 0;
                for (block, member) in tree::members(decl.member_list()) {
                    let section = match tree::name_text(&block).as_deref() {
                        Some("client") => Section::Client,
                        Some("internal") => Section::Internal,
                        _ => continue,
                    };
                    for (field, inner) in tree::members(member.block()) {
                        if let Some(field) = tree::name_text(&field) {
                            self.field(&name, &key, vec![field], &inner, Some(section), order);
                            order += 1;
                            children.push(inner.syntax().clone());
                        }
                    }
                }
            }
            Base::Enum => {
                let mut order = 0;
                for item in decl
                    .member_list()
                    .into_iter()
                    .flat_map(|list| list.member_items())
                {
                    let (variant, block, syntax) = match &item {
                        MemberItem::BareMember(bare) => (
                            bare.name().as_ref().and_then(tree::name_text),
                            None,
                            bare.syntax().clone(),
                        ),
                        MemberItem::Member(member) => {
                            (single_key(member), member.block(), member.syntax().clone())
                        }
                        _ => continue,
                    };
                    let Some(variant) = variant else { continue };
                    self.variant(&name, &key, &variant, block, &syntax, order);
                    order += 1;
                    children.push(syntax);
                }
            }
            Base::Refines(_) => {}
            _ => children.extend(self.written_fields(&name, &key, decl.member_list())),
        }
        // Each rule across its fields, with its typed body (area 2, "Rules
        // across fields").
        if matches!(base, Base::Object | Base::Input | Base::Output) {
            for (order, rule) in crate::types::across::rule_members(decl.member_list())
                .into_iter()
                .enumerate()
            {
                let Some(rule_name) = rule.name().as_ref().and_then(tree::name_text) else {
                    continue;
                };
                let rule_key = crate::graph::rule_key(self.lookup, &name, &rule_name);
                let mut node = self.node(
                    rule_key.clone(),
                    Kind::Rule,
                    format!("{name}.{rule_name}"),
                    rule.syntax().clone(),
                );
                node.parent = Some(key.clone());
                node.detail = Detail::Rule {
                    order: order as u32,
                    condition: rule
                        .condition()
                        .map(|c| crate::types::across::spaced(c.syntax()))
                        .unwrap_or_default(),
                };
                self.out.nodes.push(node);
                self.body(&rule_key, index as u32, Some(order as u32));
                children.push(rule.syntax().clone());
            }
        }
        let uses = self
            .resolver
            .uses(decl.syntax(), &|n| children.iter().any(|child| child == n));
        self.edges(&key, EdgeKind::Uses, uses);
    }

    fn entity(&mut self, decl: &ast::EntityDecl) {
        let Some(name) = decl.name().as_ref().and_then(tree::name_text) else {
            return;
        };
        let key = self.lookup.registry().key(Kind::Entity, &name, None);
        let mut node = self.node(
            key.clone(),
            Kind::Entity,
            name.clone(),
            decl.syntax().clone(),
        );
        node.public = decl.visibility().is_some();
        let entity = crate::data::Entities::new(self.db, self.project)
            .get(&name)
            .filter(|e| &e.path == self.path);
        if let Some(entity) = entity
            && let Some(store) = &entity.store
        {
            node.detail = Detail::Entity {
                store: store.clone(),
                table: entity.table.clone(),
            };
        }
        self.out.nodes.push(node);
        let children = self.written_fields(&name, &key, decl.member_list());
        let uses = self
            .resolver
            .uses(decl.syntax(), &|n| children.iter().any(|child| child == n));
        self.edges(&key, EdgeKind::Uses, uses);
        if let Some(entity) = entity {
            self.entity_data(decl, &key, entity);
        }
        self.provided_failure(decl, &name);
    }

    /// The not-found failure the compiler provides for an entity whose
    /// project declares none (area 6, "Write failures"): a failure type with
    /// kind `not_found` and no fields, placed at the entity.
    fn provided_failure(&mut self, decl: &ast::EntityDecl, entity: &str) {
        let name = format!("{entity}NotFound");
        let provided =
            self.lookup.declared(&name).iter().any(|symbol| {
                symbol.provided.as_deref() == Some(entity) && symbol.path == *self.path
            });
        if !provided {
            return;
        }
        let key = self.item_key(Kind::Type, &TYPE, &name);
        let mut node = self.node(key, Kind::Type, name.clone(), decl.syntax().clone());
        node.public = true;
        node.doc = Some(format!(
            "No `{entity}` has that `id`: provided by the compiler, since the project declares no `{name}`."
        ));
        let kind = "not_found".to_string();
        node.detail = Detail::Type {
            base: Base::Failure,
            rules: Rules::default(),
            failure: Some(FailureDetail {
                status: status_of(&kind),
                kind: Some(kind),
                code: crate::case::to_snake(&name),
                message: None,
            }),
        };
        self.out.nodes.push(node);
    }

    /// An entity's data nodes (area 5): where it is stored, its compound
    /// unique rules, its declared indexes and its references, each keyed
    /// under the entity so a rename of the entity carries them.
    fn entity_data(
        &mut self,
        decl: &ast::EntityDecl,
        key: &Key,
        entity: &crate::data::EntityModel,
    ) {
        let registry = self.lookup.registry();
        let born = registry.born(Kind::Entity, &entity.name, None);
        let child = |kind: Kind, name: &str| {
            registry.key(
                kind,
                &format!("{}.{name}", entity.name),
                Some((&entity.name, &born)),
            )
        };
        let sections: BTreeMap<String, ast::Member> = tree::members(decl.member_list())
            .into_iter()
            .filter_map(|(name, member)| Some((tree::name_text(&name)?, member)))
            .collect();
        if let Some(store) = &entity.store {
            let syntax = sections
                .get("store")
                .map_or_else(|| decl.syntax().clone(), |m| m.syntax().clone());
            let mut node = self.node(
                registry.key(Kind::StorePlacement, &entity.name, None),
                Kind::StorePlacement,
                entity.name.clone(),
                syntax,
            );
            node.parent = Some(key.clone());
            node.doc = None;
            node.detail = Detail::Placement {
                store: store.clone(),
            };
            self.out.nodes.push(node);
        }
        for (section, kind, rules) in [
            ("unique", Kind::UniqueRule, &entity.unique),
            ("indexes", Kind::Index, &entity.indexes),
        ] {
            let members: BTreeMap<String, ast::Member> = sections
                .get(section)
                .map(|m| tree::members(m.block()))
                .unwrap_or_default()
                .into_iter()
                .filter_map(|(name, member)| Some((tree::name_text(&name)?, member)))
                .collect();
            for rule in rules {
                let Some(member) = members.get(&rule.name) else {
                    continue;
                };
                let mut node = self.node(
                    child(kind, &rule.name),
                    kind,
                    format!("{}.{}", entity.name, rule.name),
                    member.syntax().clone(),
                );
                node.parent = Some(key.clone());
                node.detail = Detail::Columns {
                    fields: rule.fields.clone(),
                    constraint: if kind == Kind::UniqueRule {
                        crate::data::unique_constraint(entity, &rule.name)
                    } else {
                        crate::data::index_name(entity, &rule.name)
                    },
                };
                self.out.nodes.push(node);
            }
        }
        for field in &entity.fields {
            let (Some(reference), Some(member)) = (&field.reference, sections.get(&field.name))
            else {
                continue;
            };
            let Some(on_delete) = reference.on_delete else {
                continue;
            };
            let Some(target) = declaration_key(self.lookup, &reference.target) else {
                continue;
            };
            let mut node = self.node(
                child(Kind::Reference, &field.name),
                Kind::Reference,
                format!("{}.{}", entity.name, field.name),
                member.syntax().clone(),
            );
            node.parent = Some(entity_field_key(
                registry,
                &entity.name,
                std::slice::from_ref(&field.name),
            ));
            node.doc = None;
            node.detail = Detail::Reference {
                target: target.clone(),
                on_delete: on_delete.name().to_string(),
                constraint: crate::data::foreign_key(entity, field),
            };
            self.out.nodes.push(node);
            self.out
                .edges
                .push((key.clone(), EdgeKind::References, target));
        }
        self.lifecycle(&sections, key, entity);
    }

    /// An entity's lifecycle node and a node for each of its transitions
    /// (area 5), keyed under the entity.
    fn lifecycle(
        &mut self,
        sections: &BTreeMap<String, ast::Member>,
        key: &Key,
        entity: &crate::data::EntityModel,
    ) {
        let (Some(lifecycle), Some(member)) = (&entity.lifecycle, sections.get("lifecycle")) else {
            return;
        };
        let registry = self.lookup.registry();
        let born = registry.born(Kind::Entity, &entity.name, None);
        let parts: BTreeMap<String, ast::Member> = tree::members(member.block())
            .into_iter()
            .filter_map(|(name, member)| Some((tree::name_text(&name)?, member)))
            .collect();
        let shown = |member: Option<&ast::Member>| {
            member.and_then(|m| m.value()).map(|value| {
                value
                    .syntax()
                    .text()
                    .to_string()
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
            })
        };
        let lifecycle_key =
            registry.key(Kind::Lifecycle, &entity.name, Some((&entity.name, &born)));
        let mut node = self.node(
            lifecycle_key.clone(),
            Kind::Lifecycle,
            entity.name.clone(),
            member.syntax().clone(),
        );
        node.parent = Some(key.clone());
        node.doc = None;
        node.detail = Detail::Lifecycle {
            visible: lifecycle.visible.as_ref().and(shown(parts.get("visible"))),
            purge: lifecycle
                .purge
                .as_ref()
                .map(|purge| format!("after {} since {}", purge.after_written, purge.since)),
        };
        self.out.nodes.push(node);
        let transitions: BTreeMap<String, ast::Member> = parts
            .get("transitions")
            .map(|m| tree::members(m.block()))
            .unwrap_or_default()
            .into_iter()
            .filter_map(|(name, member)| Some((tree::name_text(&name)?, member)))
            .collect();
        for transition in &lifecycle.transitions {
            let Some(member) = transitions.get(&transition.name) else {
                continue;
            };
            let when = tree::members(member.block())
                .into_iter()
                .find(|(name, _)| tree::name_text(name).as_deref() == Some("when"))
                .map(|(_, member)| member);
            let mut node = self.node(
                crate::types::transition_key(registry, &entity.name, &transition.name),
                Kind::Transition,
                format!("{}.{}", entity.name, transition.name),
                member.syntax().clone(),
            );
            node.parent = Some(lifecycle_key.clone());
            node.doc = None;
            node.detail = Detail::Transition {
                when: transition.when.as_ref().and(shown(when.as_ref())),
                guarded: transition.guarded,
                sets: transition
                    .set
                    .iter()
                    .map(|(field, _)| field.clone())
                    .collect(),
            };
            self.out.nodes.push(node);
        }
    }

    /// The written fields of an object or entity, and a node for each
    /// field its copying lines bring in; the members that became fields.
    fn written_fields(
        &mut self,
        host: &str,
        host_key: &Key,
        list: Option<ast::MemberList>,
    ) -> Vec<SyntaxNode> {
        let fields: Vec<FieldDef> = self
            .lookup
            .fields(host)
            .map(<[_]>::to_vec)
            .unwrap_or_default();
        let mut written: BTreeMap<String, ast::Member> = BTreeMap::new();
        // Each `include` and `pick` entry, as the names it is written with.
        let mut copying: Vec<(Vec<String>, SyntaxNode)> = Vec::new();
        for (name, member) in tree::members(list) {
            let Some(name) = tree::name_text(&name) else {
                continue;
            };
            if matches!(name.as_str(), "include" | "pick")
                && let Some(ast::Expr::ListExpr(entries)) = member.value()
            {
                for entry in entries.exprs() {
                    if let crate::types::TypeRef::Path(names, _) =
                        crate::types::TypeRef::from_expr(&entry)
                    {
                        copying.push((names, entry.syntax().clone()));
                    }
                }
            }
            written.entry(name).or_insert(member);
        }
        let mut children = Vec::new();
        // The table lists the fields as the copying lines leave them, top
        // to bottom (area 2): that is their declared order.
        for (order, field) in (0u32..).zip(&fields) {
            if field.copied {
                // At the entry that brought it in: `Note.body` in a `pick`,
                // or `Note` in an `include`.
                let entry = match &field.declared {
                    Ty::Field(source, path) => copying.iter().find(|(names, _)| {
                        names.first() == Some(source)
                            && (names.len() == 1 || names[1..] == path[..])
                    }),
                    _ => None,
                };
                self.copied_field(host, host_key, field, entry.map(|(_, node)| node), order);
            } else if let Some(member) = written.get(&field.name) {
                self.field(
                    host,
                    host_key,
                    vec![field.name.clone()],
                    member,
                    None,
                    order,
                );
                children.push(member.syntax().clone());
            } else if crate::types::EMAIL_FIELDS
                .iter()
                .any(|(name, _)| *name == field.name)
            {
                // Every email's `to:` and `locale:` (area 16): written
                // nowhere, so placed at its type.
                self.copied_field(host, host_key, field, None, order);
            }
        }
        children
    }

    /// How a provider package's field is sent (area 11, "Decoding"; chunk
    /// 7.2c2): its built-in encoding, or the package's functions that
    /// decode and encode it, once they are checked.
    fn coding(
        &self,
        host: &str,
        path: &[String],
        def: &crate::types::FieldDef,
    ) -> Option<jadpo_graph::FieldCoding> {
        self.ownership.package()?;
        match def.coding.as_ref()? {
            crate::types::WrittenCoding::Encoding(name) => {
                jadpo_std::provider::WireEncoding::from_name(name)
                    .map(|_| jadpo_graph::FieldCoding::Encoding(name.clone()))
            }
            // Its parts in the standard type's own order, once checked to
            // be all of them (chunk 7.2e).
            crate::types::WrittenCoding::Parts(written) => {
                use crate::types::TypeView;
                let declared = def.declared.strip_nullable();
                let standard = match self
                    .lookup
                    .base_prim(declared)
                    .and_then(jadpo_std::standard::of_builtin)
                {
                    Some(entry) => entry.name.to_string(),
                    None => match self.lookup.structure(declared) {
                        crate::types::Ty::Opaque(name, args) if args.is_empty() => name,
                        _ => return None,
                    },
                };
                let wanted = jadpo_std::provider::wire_parts(&standard)?;
                let parts = wanted
                    .iter()
                    .map(|part| {
                        written
                            .iter()
                            .find(|(given, _)| given == part)
                            .map(|(_, spelling)| (part.to_string(), spelling.clone()))
                    })
                    .collect::<Option<Vec<_>>>()?;
                (written.len() == parts.len())
                    .then_some(jadpo_graph::FieldCoding::Parts { standard, parts })
            }
            crate::types::WrittenCoding::Functions { .. } => {
                let [field] = path else {
                    return None;
                };
                let host = self.lookup.canonical_name(host);
                let decoder =
                    crate::providers::operations(self.db, self.project).decoder(&host, field)?;
                Some(jadpo_graph::FieldCoding::Functions {
                    decode: decoder.decode.clone(),
                    encode: decoder.encode.clone(),
                    wire: type_ref(self.lookup, &decoder.wire),
                })
            }
        }
    }

    fn field(
        &mut self,
        host: &str,
        parent: &Key,
        path: Vec<String>,
        member: &ast::Member,
        section: Option<Section>,
        order: u32,
    ) {
        let key = field_key(self.lookup, host, &path);
        let mut node = self.node(
            key.clone(),
            Kind::Field,
            format!("{host}.{}", path.join(".")),
            member.syntax().clone(),
        );
        node.parent = Some(parent.clone());
        let def = self.lookup.field(host, &path);
        let written = member
            .value()
            .map(|value| match &value {
                ast::Expr::RecordExpr(record) => record
                    .expr()
                    .map_or_else(String::new, |e| tree::written(e.syntax())),
                other => tree::written(other.syntax()),
            })
            .unwrap_or_default();
        node.detail = Detail::Field {
            order,
            ty: def
                .filter(|def| !def.declared.has_error())
                .map_or(TypeRef::Written(written), |def| {
                    type_ref(self.lookup, &def.declared)
                }),
            optional: def.is_some_and(|def| def.optional),
            default: def.and_then(|def| field_default(self.lookup, def.default.as_ref()?)),
            header: def.and_then(|def| def.header.clone()),
            column: def.and_then(|def| def.column.clone()),
            wire_name: def.and_then(|def| def.wire_name.clone()),
            coding: def.and_then(|def| self.coding(host, &path, def)),
            section,
            copied_from: None,
            replaces: def.and_then(|def| def.replaces.as_ref()).map(|source| {
                field_key(
                    self.lookup,
                    source,
                    std::slice::from_ref(&path[path.len() - 1]),
                )
            }),
            rules: def.map(|def| def.rules.clone()).unwrap_or_default(),
        };
        self.out.nodes.push(node);
        let uses = self.resolver.uses(member.syntax(), &|_| false);
        self.edges(&key, EdgeKind::Uses, uses);
    }

    /// A field brought in by `include` or `pick`: it has no text of its
    /// own, so it takes the place of the entry that copied it, and no
    /// digest.
    fn copied_field(
        &mut self,
        host: &str,
        parent: &Key,
        field: &FieldDef,
        entry: Option<&SyntaxNode>,
        order: u32,
    ) {
        let key = field_key(self.lookup, host, std::slice::from_ref(&field.name));
        let parent_place = self
            .out
            .nodes
            .iter()
            .find(|node| &node.key == parent)
            .and_then(|node| node.place.clone());
        let mut node = Node::new(key.clone(), Kind::Field, format!("{host}.{}", field.name));
        node.place = match entry {
            Some(entry) => Some(place(self.path, &self.lines, entry)),
            None => parent_place,
        };
        node.domain = self.domain.clone();
        node.parent = Some(parent.clone());
        let from = match &field.declared {
            Ty::Field(source, path) => Some(field_key(self.lookup, source, path)),
            _ => None,
        };
        node.detail = Detail::Field {
            order,
            ty: type_ref(self.lookup, &field.declared),
            optional: field.optional,
            default: field
                .default
                .as_ref()
                .and_then(|value| field_default(self.lookup, value)),
            header: field.header.clone(),
            column: field.column.clone(),
            wire_name: field.wire_name.clone(),
            coding: None,
            section: None,
            copied_from: from.clone(),
            replaces: None,
            rules: field.rules.clone(),
        };
        self.out.nodes.push(node);
        if let Some(from) = from {
            self.out.edges.push((key, EdgeKind::Uses, from));
        }
    }

    fn variant(
        &mut self,
        host: &str,
        parent: &Key,
        variant: &str,
        block: Option<ast::MemberList>,
        syntax: &SyntaxNode,
        order: u32,
    ) {
        let key = variant_key(self.lookup, host, variant);
        let mut node = self.node(
            key.clone(),
            Kind::Variant,
            format!("{host}.{variant}"),
            syntax.clone(),
        );
        node.parent = Some(parent.clone());
        // A block holding only `wire_name:` gives a variant without data
        // its spelling on the wire (area 11, decided 2026-10-20).
        let (wire_name, payload) = crate::types::variant_block(block.as_ref());
        node.detail = Detail::Variant {
            order,
            payload,
            wire_name,
        };
        self.out.nodes.push(node);
        let mut order = 0;
        for (field, member) in tree::members(block) {
            if crate::types::is_wire_name(&member) {
                continue;
            }
            if let Some(field) = tree::name_text(&field) {
                let path = vec![variant.to_string(), field];
                self.field(host, &key, path, &member, None, order);
                order += 1;
            }
        }
    }
}

/// An input field's `default:` as the graph holds it: a literal, or a
/// plain variant written `Enum.variant`.
fn field_default(symbols: &dyn NameView, value: &Const) -> Option<FieldDefault> {
    match value {
        Const::Path(path) if path.len() == 2 => Some(FieldDefault::Variant(variant_key(
            symbols, &path[0], &path[1],
        ))),
        other => other.literal().map(FieldDefault::Literal),
    }
}

fn public(decl: &Decl) -> bool {
    match decl {
        Decl::FunctionDecl(d) => d.visibility().is_some(),
        Decl::ActionDecl(d) => d.visibility().is_some(),
        Decl::QueryDecl(d) => d.visibility().is_some(),
        _ => false,
    }
}

fn single_key(member: &ast::Member) -> Option<String> {
    let mut names = member.member_key()?.names();
    let first = names.next()?;
    if names.next().is_some() {
        return None;
    }
    tree::name_text(&first)
}

fn fields_base(base: FieldsBase) -> Base {
    match base {
        FieldsBase::Object | FieldsBase::Entity => Base::Object,
        FieldsBase::Input => Base::Input,
        FieldsBase::Output => Base::Output,
        FieldsBase::Event => Base::Event,
        FieldsBase::EmailMessage => Base::EmailMessage,
        FieldsBase::EmailPartial => Base::EmailPartial,
        FieldsBase::Message => Base::Message,
        FieldsBase::Prompt => Base::Prompt,
        FieldsBase::Failure => Base::Failure,
    }
}
