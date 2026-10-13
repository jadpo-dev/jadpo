//! One file's part of the graph: its own node, its declarations and their
//! fields and variants, and the edges that start at them.

use std::collections::{BTreeMap, BTreeSet};

use jadpo_db::{Db, File, Item, Project, item_tree, location, symbols};
use jadpo_graph::{
    Base, Detail, EdgeKind, FailureDetail, Key, Kind, Node, Section, Signature, TypeRef,
    catalogue::status_of,
};
use jadpo_syntax::SyntaxNode;
use jadpo_syntax::ast::{self, AstNode, Decl, MemberItem};

use super::paths::Resolver;
use super::text::{LineIndex, digest, doc, place};
use super::{declaration_key, field_key, library_function_key, registry, type_ref, variant_key};
use crate::failures::{FailureTable, failure_table};
use crate::tree;
use crate::types::{
    Callables, Const, Def, FieldDef, FieldsBase, Rules, Sig, Ty, TypeTable, callables,
    failure_sets, type_table,
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
    let symbols = symbols(db, project);
    let mut part = Part {
        path: file.path(db),
        lines: LineIndex::new(file.text(db)),
        domain: location(db, project, file)
            .ownership
            .domain()
            .map(str::to_string),
        resolver: Resolver {
            symbols,
            table: type_table(db, project),
            callables: callables(db, project),
        },
        table: type_table(db, project),
        callables: callables(db, project),
        failures: failure_table(db, project),
        items: &item_tree(db, file).items,
        out: FilePart::default(),
    };
    let mut node = part.node(
        registry().key(Kind::File, part.path, None),
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
    for (name, set) in &failure_sets(db, project, file).callables {
        let from = match name.split_once('.') {
            Some((library, function)) => Some(library_function_key(library, function)),
            None => declaration_key(symbols, name),
        };
        let Some(from) = from else { continue };
        for failure in &set.failures {
            if let Some(to) = declaration_key(symbols, failure) {
                part.out.edges.push((from.clone(), EdgeKind::Raises, to));
            }
        }
    }
    part.out
}

struct Part<'a> {
    path: &'a String,
    lines: LineIndex,
    domain: Option<String>,
    resolver: Resolver<'a>,
    table: &'a TypeTable,
    callables: &'a Callables,
    failures: &'a FailureTable,
    items: &'a [Item],
    out: FilePart,
}

impl Part<'_> {
    /// A type as the graph holds it: canonically when it resolved, as
    /// written otherwise.
    fn type_of(&self, ty: Option<&Ty>, written: &str) -> TypeRef {
        match ty {
            Some(ty) if !ty.has_error() => type_ref(self.resolver.symbols, ty),
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
                .map(
                    |failure| match declaration_key(self.resolver.symbols, failure) {
                        Some(key) if self.failures.contains(failure) => TypeRef::Node(key),
                        _ => TypeRef::Written(failure.clone()),
                    },
                )
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
            Decl::TypeDecl(d) => self.type_decl(d),
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
            Decl::RoutesDecl(_) | Decl::ScheduleDecl(_) | Decl::SubscriberDecl(_) => {
                if let Some(item) = item
                    && let Some(kind) = super::kind_of(&item.kind)
                {
                    let key = registry().key(kind, &item.name, None);
                    let node = self.node(key, kind, item.name.clone(), decl.syntax().clone());
                    self.out.nodes.push(node);
                }
            }
            Decl::SeedDecl(d) => {
                if let Some(name) = d.name().as_ref().and_then(tree::name_text) {
                    let key = registry().key(Kind::Seed, &name, None);
                    self.runnable(key, Kind::Seed, name, d.syntax());
                }
            }
            Decl::TestDecl(d) => {
                if let Some(token) = d.string_token() {
                    let name = token.text().trim_matches('"').to_string();
                    let key =
                        registry().key(Kind::Test, &format!("{} \"{name}\"", self.path), None);
                    self.runnable(key, Kind::Test, name, d.syntax());
                }
            }
            Decl::PackageDecl(_)
            | Decl::ResourceDecl(_)
            | Decl::ProvideDecl(_)
            | Decl::SingletonDecl(_) => {}
        }
    }

    /// A test or a seed: statements that call actions.
    fn runnable(&mut self, key: Key, kind: Kind, name: String, syntax: &SyntaxNode) {
        let node = self.node(key.clone(), kind, name, syntax.clone());
        self.out.nodes.push(node);
        let uses = self.resolver.uses(syntax, &|_| false);
        let calls = self.resolver.calls(syntax, None);
        self.edges(&key, EdgeKind::Uses, uses);
        self.edges(&key, EdgeKind::Calls, calls);
    }

    fn callable(&mut self, syntax: &SyntaxNode, item: &Item, public: bool) {
        let Some(kind) = super::kind_of(&item.kind) else {
            return;
        };
        let key = registry().key(kind, &item.name, None);
        let mut node = self.node(key.clone(), kind, item.name.clone(), syntax.clone());
        node.public = public;
        node.detail =
            Detail::Callable(self.signature(self.callables.plain.get(&item.name), &item.signature));
        self.out.nodes.push(node);
        let uses = self.resolver.uses(syntax, &|_| false);
        let calls = self.resolver.calls(syntax, None);
        self.edges(&key, EdgeKind::Uses, uses);
        self.edges(&key, EdgeKind::Calls, calls);
    }

    fn library(&mut self, decl: &ast::LibraryDecl, item: &Item) {
        let key = registry().key(Kind::Library, &item.name, None);
        let mut node = self.node(
            key.clone(),
            Kind::Library,
            item.name.clone(),
            decl.syntax().clone(),
        );
        node.public = item.public;
        self.out.nodes.push(node);
        for (function, member) in tree::library_functions(decl)
            .iter()
            .zip(item.functions.iter())
        {
            let child = library_function_key(&item.name, &member.name);
            let mut node = self.node(
                child.clone(),
                Kind::Function,
                format!("{}.{}", item.name, member.name),
                function.syntax().clone(),
            );
            node.parent = Some(key.clone());
            node.public = item.public;
            node.detail = Detail::Callable(
                self.signature(
                    self.callables
                        .library
                        .get(&(item.name.clone(), member.name.clone())),
                    &member.signature,
                ),
            );
            self.out.nodes.push(node);
            let uses = self.resolver.uses(function.syntax(), &|_| false);
            let calls = self.resolver.calls(function.syntax(), Some(&item.name));
            self.edges(&child, EdgeKind::Uses, uses);
            self.edges(&child, EdgeKind::Calls, calls);
        }
    }

    fn type_decl(&mut self, decl: &ast::TypeDecl) {
        let Some(name) = decl.name().as_ref().and_then(tree::name_text) else {
            return;
        };
        let key = registry().key(Kind::Type, &name, None);
        let mut node = self.node(key.clone(), Kind::Type, name.clone(), decl.syntax().clone());
        node.public = decl.visibility().is_some();
        let written_base = decl
            .base()
            .map(|base| tree::written(base.syntax()))
            .unwrap_or_default();
        let def = self.table.def(&name);
        let base = match def {
            Some(Def::Scalar { parent, .. }) if !parent.has_error() => {
                Base::Refines(type_ref(self.resolver.symbols, parent))
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
                _ => Base::Refines(TypeRef::Written(written_base.clone())),
            },
        };
        let rules = match def {
            Some(Def::Scalar { rules, .. }) => (**rules).clone(),
            _ => Rules::default(),
        };
        let failure = (base == Base::Failure).then(|| {
            let info = self.failures.get(&name);
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
                for (block, member) in tree::members(decl.member_list()) {
                    let section = match tree::name_text(&block).as_deref() {
                        Some("client") => Section::Client,
                        Some("internal") => Section::Internal,
                        _ => continue,
                    };
                    for (field, inner) in tree::members(member.block()) {
                        if let Some(field) = tree::name_text(&field) {
                            self.field(&name, &key, vec![field], &inner, Some(section));
                            children.push(inner.syntax().clone());
                        }
                    }
                }
            }
            Base::Enum => {
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
                    self.variant(&name, &key, &variant, block, &syntax);
                    children.push(syntax);
                }
            }
            Base::Refines(_) => {}
            _ => children.extend(self.written_fields(&name, &key, decl.member_list())),
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
        let key = registry().key(Kind::Entity, &name, None);
        let mut node = self.node(
            key.clone(),
            Kind::Entity,
            name.clone(),
            decl.syntax().clone(),
        );
        node.public = decl.visibility().is_some();
        self.out.nodes.push(node);
        let children = self.written_fields(&name, &key, decl.member_list());
        let uses = self
            .resolver
            .uses(decl.syntax(), &|n| children.iter().any(|child| child == n));
        self.edges(&key, EdgeKind::Uses, uses);
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
            .table
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
        for field in &fields {
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
                self.copied_field(host, host_key, field, entry.map(|(_, node)| node));
            } else if let Some(member) = written.get(&field.name) {
                self.field(host, host_key, vec![field.name.clone()], member, None);
                children.push(member.syntax().clone());
            }
        }
        children
    }

    fn field(
        &mut self,
        host: &str,
        parent: &Key,
        path: Vec<String>,
        member: &ast::Member,
        section: Option<Section>,
    ) {
        let key = field_key(host, &path);
        let mut node = self.node(
            key.clone(),
            Kind::Field,
            format!("{host}.{}", path.join(".")),
            member.syntax().clone(),
        );
        node.parent = Some(parent.clone());
        let def = self.table.field(host, &path);
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
            ty: def
                .filter(|def| !def.declared.has_error())
                .map_or(TypeRef::Written(written), |def| {
                    type_ref(self.resolver.symbols, &def.declared)
                }),
            optional: def.is_some_and(|def| def.optional),
            default: def.is_some_and(|def| def.default),
            section,
            copied_from: None,
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
    ) {
        let key = field_key(host, std::slice::from_ref(&field.name));
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
            Ty::Field(source, path) => Some(field_key(source, path)),
            _ => None,
        };
        node.detail = Detail::Field {
            ty: type_ref(self.resolver.symbols, &field.declared),
            optional: field.optional,
            default: field.default,
            section: None,
            copied_from: from.clone(),
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
    ) {
        let key = variant_key(host, variant);
        let mut node = self.node(
            key.clone(),
            Kind::Variant,
            format!("{host}.{variant}"),
            syntax.clone(),
        );
        node.parent = Some(parent.clone());
        node.detail = Detail::Variant {
            payload: block.is_some(),
        };
        self.out.nodes.push(node);
        for (field, member) in tree::members(block) {
            if let Some(field) = tree::name_text(&field) {
                self.field(host, &key, vec![variant.to_string(), field], &member, None);
            }
        }
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
        FieldsBase::Failure => Base::Failure,
    }
}
