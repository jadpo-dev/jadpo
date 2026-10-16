//! Lowering a checked callable to its typed body (docs/compiler-runtime.md
//! §2.3): the callable's syntax walked in source order, each statement and
//! expression numbered as it is reached, with what the check decided about
//! it read from its facts. Nothing is decided here: a call reaches what the
//! check resolved it to, a literal has the type the check gave it, and a
//! name means what the check found it to mean, so the typed body, the
//! diagnostics and everything that reads the body agree.

use std::collections::BTreeMap;

use jadpo_graph::body::{
    self as typed, ArmBody, BinaryOp, Branch, BranchId, Callee, Literal, LocalId, Pattern, Refers,
    SiteId, Stmt as TypedStmt, UnaryOp,
};
use jadpo_graph::{Key, TypeRef};
use jadpo_std::RuleKey;
use jadpo_syntax::ast::{self, AstNode, BlockItem, Expr, Pat, Stmt};
use jadpo_syntax::{SyntaxKind, SyntaxNode};

use super::super::rules::Const;
use super::super::ty::{Prim, Ty};
use super::facts::{BuildFact, Facts, NodeKey, PatternFact, Reach, node_key};
use super::writes::{Breach, WriteFact};
use super::{Body, Fallible, LocalKind};
use crate::graph::paths::Resolver;
use crate::graph::{
    LineIndex, declaration_key, field_key, library_function_key, type_ref, variant_key,
};
use crate::lookups::{FailureView, Lookup, NameView};
use crate::tree::{self, dotted};

/// The typed body of the callable `syntax`, from what checking it
/// decided.
pub(super) fn lower(
    checker: &Body<'_>,
    lookup: &Lookup<'_>,
    text: &str,
    syntax: &SyntaxNode,
) -> typed::Body {
    let mut lower = Lower {
        facts: &checker.facts,
        names: lookup,
        failures: lookup,
        resolver: Resolver {
            symbols: lookup,
            table: lookup,
        },
        lines: LineIndex::new(text),
        sites: Vec::new(),
        branches: Vec::new(),
        refers: Vec::new(),
        at: BTreeMap::new(),
        write_block: None,
    };
    let mut body = typed::Body::default();
    for (id, local) in checker.facts.locals.iter().enumerate() {
        body.locals.push(typed::Local {
            name: local.name.clone(),
            ty: type_ref(lookup, &local.ty),
            kind: local_kind(local.kind),
            start: local.span.start,
            end: local.span.end,
        });
        if local.kind == LocalKind::Param {
            body.params.push(LocalId(id as u32));
        }
    }
    let (params, result, fails, block) = parts(syntax);
    for param in params.iter().flat_map(ast::ParamList::params) {
        if let Some(name) = param.name() {
            lower.declared(&name);
        }
        if let Some(ty) = param.ty() {
            lower.type_names(ty.syntax());
        }
    }
    if let Some(ty) = result.and_then(|ret| ret.ty()) {
        lower.type_names(ty.syntax());
    }
    for ty in fails.iter().flat_map(ast::FailsClause::types) {
        lower.type_names(ty.syntax());
    }
    body.block = lower.block(block);
    body.proofs = lower.proofs();
    body.sites = lower.sites.into_iter().flatten().collect();
    body.pauses = body.sites.iter().any(|site| site.pauses);
    body.branches = lower.branches;
    lower.refers.sort_by_key(|name| (name.start, name.end));
    lower.refers.dedup();
    body.names = lower.refers;
    body.failures = checker
        .flow
        .leaving
        .keys()
        .filter_map(|failure| declaration_key(lookup, failure))
        .collect();
    body.open = checker.flow.open;
    body
}

/// A callable's parameters, result, `fails` list and block.
fn parts(
    syntax: &SyntaxNode,
) -> (
    Option<ast::ParamList>,
    Option<ast::RetType>,
    Option<ast::FailsClause>,
    Option<ast::Block>,
) {
    if let Some(function) = ast::FunctionDecl::cast(syntax.clone()) {
        return (
            function.param_list(),
            function.ret_type(),
            function.fails_clause(),
            function.block(),
        );
    }
    if let Some(action) = ast::ActionDecl::cast(syntax.clone()) {
        return (
            action.param_list(),
            action.ret_type(),
            action.fails_clause(),
            action.block(),
        );
    }
    if let Some(fixture) = ast::FixtureDecl::cast(syntax.clone()) {
        return (
            fixture.param_list(),
            fixture.ret_type(),
            None,
            fixture.block(),
        );
    }
    if let Some(test) = ast::TestDecl::cast(syntax.clone()) {
        return (None, None, None, test.block());
    }
    if let Some(seed) = ast::SeedDecl::cast(syntax.clone()) {
        return (None, None, None, seed.block());
    }
    (None, None, None, None)
}

fn local_kind(kind: LocalKind) -> typed::LocalKind {
    match kind {
        LocalKind::Param => typed::LocalKind::Param,
        LocalKind::Var => typed::LocalKind::Var,
        LocalKind::VarMut => typed::LocalKind::VarMut,
        LocalKind::Item => typed::LocalKind::Item,
        LocalKind::Binding => typed::LocalKind::Binding,
        LocalKind::Inline => typed::LocalKind::Inline,
    }
}

fn literal(value: &Const) -> Option<Literal> {
    value.literal()
}

/// A literal's own type, when no position gave it one.
fn literal_type(value: &Const) -> Ty {
    match value {
        Const::Number { decimal: true, .. } => Ty::Prim(Prim::Decimal),
        Const::Number { .. } => Ty::Prim(Prim::Int),
        Const::Str(_) => Ty::Prim(Prim::Text),
        Const::Bool(_) => Ty::Prim(Prim::Bool),
        Const::Duration(_) => Ty::Prim(Prim::Duration),
        Const::None => Ty::None,
        _ => Ty::Error,
    }
}

/// A node's range without the trivia beside it.
fn range(node: &SyntaxNode) -> (u32, u32) {
    let span = tree::node_span("", node);
    (span.start, span.end)
}

struct Lower<'a> {
    facts: &'a Facts,
    names: &'a dyn NameView,
    failures: &'a dyn FailureView,
    resolver: Resolver<'a>,
    lines: LineIndex,
    /// Each site once lowered; `None` while its children are.
    sites: Vec<Option<typed::Site>>,
    branches: Vec<Branch>,
    refers: Vec<typed::Name>,
    /// The site each expression became.
    at: BTreeMap<NodeKey, SiteId>,
    /// The block an `attempt` hands the write it is lowering.
    write_block: Option<ast::MemberList>,
}

impl Lower<'_> {
    // --- names ------------------------------------------------------------

    fn refer(&mut self, node: &SyntaxNode, refers: Refers) {
        let (start, end) = range(node);
        self.refers.push(typed::Name { start, end, refers });
    }

    fn node(&mut self, node: &SyntaxNode, key: Option<Key>, uses: bool) {
        if let Some(key) = key {
            self.refer(node, Refers::Node { key, uses });
        }
    }

    /// The local a declared name brings in.
    fn declared(&mut self, name: &ast::Name) -> Option<LocalId> {
        let id = self.facts.declared.get(&node_key(name.syntax()))?;
        let id = LocalId(*id as u32);
        self.refer(name.syntax(), Refers::Local(id));
        Some(id)
    }

    /// The local a name reads.
    fn read(&mut self, name: &ast::NameRef) -> Option<LocalId> {
        let id = self.facts.reads.get(&node_key(name.syntax()))?;
        let id = LocalId(*id as u32);
        self.refer(name.syntax(), Refers::Local(id));
        Some(id)
    }

    /// A dotted name written as a type or a type's member: each name in it
    /// refers to what the names up to it mean, and the whole chain is a
    /// type the callable uses.
    fn path_names(&mut self, refs: &[ast::NameRef]) {
        let names: Vec<String> = refs.iter().filter_map(tree::name_ref_text).collect();
        if names.len() != refs.len() {
            return;
        }
        for (i, name) in refs.iter().enumerate() {
            let key = self.resolver.path(&names[..=i]);
            self.node(name.syntax(), key, i + 1 == refs.len());
        }
    }

    /// Every type a written type names: `List<Tag>`, `Note.id?`.
    fn type_names(&mut self, ty: &SyntaxNode) {
        for path in ty.descendants().filter_map(ast::Path::cast) {
            let refs: Vec<ast::NameRef> = path.name_refs().collect();
            self.path_names(&refs);
        }
    }

    /// The types, entities, fields and variants named inside something
    /// with no typed form yet (an entity write's block, area 6).
    fn scan(&mut self, node: &SyntaxNode) {
        for (named, key) in self.resolver.named(node) {
            self.refer(&named, Refers::Node { key, uses: true });
        }
        for name in node.descendants().filter_map(ast::NameRef::cast) {
            if self.facts.reads.contains_key(&node_key(name.syntax())) {
                self.read(&name);
            }
        }
    }

    fn type_key(&self, name: &str) -> Option<Key> {
        declaration_key(self.names, name)
    }

    fn ty(&self, ty: &Ty) -> TypeRef {
        type_ref(self.names, ty)
    }

    // --- sites ------------------------------------------------------------

    fn alloc(&mut self) -> usize {
        self.sites.push(None);
        self.sites.len() - 1
    }

    fn fill(&mut self, id: usize, node: &SyntaxNode, ty: TypeRef, site: typed::Node) {
        let (start, end) = range(node);
        let pauses = self.pauses(node, &site);
        self.sites[id] = Some(typed::Site {
            start,
            end,
            line: self.lines.line(start),
            ty,
            node: site,
            pauses,
        });
    }

    /// Whether a site can pause (docs/compiler-runtime.md §2.5): an effect,
    /// or a call into the runtime's action boundary. Functions are pure
    /// (area 3), so calls to them, operations and constructions never do.
    fn pauses(&self, node: &SyntaxNode, site: &typed::Node) -> bool {
        match site {
            typed::Node::Stmt(TypedStmt::Emit { .. }) => true,
            typed::Node::Expr(typed::Expr::Call { callee, .. }) => matches!(
                callee,
                Callee::Action(_)
                    | Callee::Fixture(_)
                    | Callee::Query(_)
                    | Callee::Entity { .. }
                    | Callee::Session(_)
            ),
            // `create`, `update` and `delete` write a record, and a
            // route test's request goes through the HTTP boundary.
            typed::Node::Expr(typed::Expr::Write { .. } | typed::Expr::Request { .. }) => true,
            typed::Node::Expr(typed::Expr::Unchecked) => {
                self.facts.effects.contains(&node_key(node))
            }
            _ => false,
        }
    }

    fn branch(&mut self, of: usize, node: &SyntaxNode) -> BranchId {
        let (start, end) = range(node);
        self.branches.push(Branch {
            of: SiteId(of as u32),
            start,
            end,
            line: self.lines.line(start),
        });
        BranchId(self.branches.len() as u32 - 1)
    }

    // --- statements -------------------------------------------------------

    fn block(&mut self, block: Option<ast::Block>) -> Vec<SiteId> {
        let mut out = Vec::new();
        for item in block.iter().flat_map(ast::Block::block_items) {
            match item {
                BlockItem::Stmt(stmt) => out.push(self.stmt(&stmt)),
                BlockItem::Member(member) => self.scan(member.syntax()),
                BlockItem::Elision(_) => {}
            }
        }
        out
    }

    fn stmt(&mut self, stmt: &Stmt) -> SiteId {
        let id = self.alloc();
        let lowered = match stmt {
            Stmt::VarStmt(var) => {
                if let Some(ty) = var.ty() {
                    self.type_names(ty.syntax());
                }
                let value = var.expr().map(|value| self.expr(&value));
                let local = var.name().and_then(|name| self.declared(&name));
                TypedStmt::Var { local, value }
            }
            Stmt::AssignStmt(assign) => {
                let local = match assign.target() {
                    Some(Expr::NameRef(name)) => self.read(&name),
                    Some(other) => {
                        self.scan(other.syntax());
                        None
                    }
                    None => None,
                };
                let value = assign.value().map(|value| self.expr(&value));
                TypedStmt::Assign { local, value }
            }
            Stmt::ReturnStmt(ret) => TypedStmt::Return {
                value: ret.expr().map(|value| self.expr(&value)),
            },
            Stmt::IfStmt(branch) => self.if_stmt(id, branch),
            Stmt::ForStmt(each) => {
                let list = each.expr().map(|list| self.expr(&list));
                let item = each.name().and_then(|name| self.declared(&name));
                let body = self.block(each.block());
                TypedStmt::For { item, list, body }
            }
            Stmt::EmitStmt(emit) => TypedStmt::Emit {
                value: emit.expr().map(|value| self.expr(&value)),
            },
            Stmt::AssertStmt(assert) => {
                let value = assert.expr().map(|value| self.expr(&value));
                let fails = assert.assert_fails().and_then(|fails| fails.ty());
                if let Some(ty) = &fails {
                    self.type_names(ty.syntax());
                }
                TypedStmt::Assert {
                    value,
                    fails: fails.map(|ty| {
                        let written = tree::written(ty.syntax());
                        self.type_key(&written)
                            .filter(|_| self.failures.is_declared_failure(&written))
                            .map_or(TypeRef::Written(written), TypeRef::Node)
                    }),
                }
            }
            Stmt::ExprStmt(expr) => TypedStmt::Expr {
                value: expr.expr().map(|value| self.expr(&value)),
            },
        };
        self.fill(
            id,
            stmt.syntax(),
            TypeRef::Builtin(Prim::Unit),
            typed::Node::Stmt(lowered),
        );
        SiteId(id as u32)
    }

    /// An `if` chain as one flat list: each condition with its block, then
    /// the `else`, without recursing however long the chain is.
    fn if_stmt(&mut self, id: usize, branch: &ast::IfStmt) -> TypedStmt {
        let mut arms = Vec::new();
        let first = (branch.condition(), branch.block(), branch.syntax().clone());
        let rest = branch
            .else_ifs()
            .map(|next| (next.condition(), next.block(), next.syntax().clone()));
        for (condition, block, node) in std::iter::once(first).chain(rest) {
            let condition = condition
                .and_then(|c| c.expr())
                .map(|condition| self.expr(&condition));
            let at = match &block {
                Some(block) => self.branch(id, block.syntax()),
                None => self.branch(id, &node),
            };
            let then = self.block(block);
            arms.push(typed::IfArm {
                condition,
                then,
                branch: at,
            });
        }
        let (otherwise_branch, otherwise) = match branch.else_branch() {
            Some(otherwise) => {
                let at = self.branch(id, otherwise.syntax());
                (at, self.block(otherwise.block()))
            }
            // The way past an `if` without `else` is a branch too.
            None => (self.branch(id, branch.syntax()), Vec::new()),
        };
        TypedStmt::If {
            arms,
            otherwise,
            otherwise_branch,
        }
    }

    // --- expressions ------------------------------------------------------

    fn type_of(&self, expr: &Expr) -> Ty {
        let key = node_key(expr.syntax());
        if let Some(ty) = self.facts.types.get(&key) {
            return ty.clone();
        }
        if let Some((host, path)) = self.facts.fields.get(&key) {
            return self.resolver.table.field_type(host, path);
        }
        match expr {
            Expr::Literal(_) | Expr::PrefixExpr(_) => literal_type(&Const::from_expr(expr)),
            _ => Ty::Error,
        }
    }

    fn expr(&mut self, expr: &Expr) -> SiteId {
        if let Expr::ParenExpr(paren) = expr
            && let Some(inner) = paren.expr()
        {
            return self.expr(&inner);
        }
        let id = self.alloc();
        self.at.insert(node_key(expr.syntax()), SiteId(id as u32));
        let ty = self.type_of(expr);
        let lowered = self.expr_node(expr);
        let ty = self.ty(&ty);
        self.fill(id, expr.syntax(), ty, typed::Node::Expr(lowered));
        SiteId(id as u32)
    }

    /// Each construction a condition proves a rule of, with the
    /// comparisons that prove it, in source order.
    fn proofs(&self) -> Vec<typed::Proof> {
        let mut out: Vec<typed::Proof> = self
            .facts
            .proofs
            .iter()
            .filter_map(|(key, proved)| {
                let site = *self.at.get(key)?;
                let mut by: Vec<SiteId> = proved
                    .iter()
                    .flat_map(|(_, by)| by.iter())
                    .filter_map(|origin| self.at.get(&origin.key).copied())
                    .collect();
                by.sort_unstable();
                by.dedup();
                Some(typed::Proof { site, by })
            })
            .collect();
        out.sort_by_key(|proof| proof.site);
        out
    }

    fn expr_node(&mut self, expr: &Expr) -> typed::Expr {
        let key = node_key(expr.syntax());
        match expr {
            Expr::Literal(_) => self.literal(expr),
            Expr::PrefixExpr(prefix) => {
                let negative = prefix.op().map(|t| t.kind()) == Some(SyntaxKind::MINUS);
                if negative && matches!(prefix.expr(), Some(Expr::Literal(_))) {
                    return self.literal(expr);
                }
                match prefix.expr() {
                    Some(operand) => typed::Expr::Unary {
                        op: if negative {
                            UnaryOp::Negate
                        } else {
                            UnaryOp::Not
                        },
                        operand: self.expr(&operand),
                    },
                    None => typed::Expr::Unchecked,
                }
            }
            Expr::NameRef(name) => match self.read(name) {
                Some(local) => typed::Expr::Local(local),
                None => {
                    self.scan(expr.syntax());
                    typed::Expr::Unchecked
                }
            },
            Expr::FieldExpr(field) => self.field(expr, field),
            Expr::CallExpr(call) => self.call(call),
            Expr::RecordExpr(record) => self.record(record),
            Expr::ListExpr(list) => typed::Expr::List {
                items: list.exprs().map(|item| self.expr(&item)).collect(),
            },
            Expr::BinExpr(binary) => {
                let op = binary
                    .op()
                    .and_then(|token| BinaryOp::from_token(token.text()));
                match (op, binary.lhs(), binary.rhs()) {
                    (Some(op), Some(lhs), Some(rhs)) => typed::Expr::Binary {
                        op,
                        lhs: self.expr(&lhs),
                        rhs: self.expr(&rhs),
                    },
                    _ => {
                        self.scan(expr.syntax());
                        typed::Expr::Unchecked
                    }
                }
            }
            Expr::MatchExpr(subject) => self.match_expr(subject),
            Expr::AttemptExpr(attempt) => {
                let write = matches!(attempt.expr(), Some(Expr::CallExpr(call))
                    if self.facts.writes.contains_key(&node_key(call.syntax())));
                if write {
                    // The block is the write's, lowered with it.
                    self.write_block = attempt.member_list();
                    let inner = attempt.expr().map(|inner| self.expr(&inner));
                    self.write_block = None;
                    return typed::Expr::Attempt {
                        inner,
                        arms: Vec::new(),
                    };
                }
                let inner = attempt.expr().map(|inner| self.expr(&inner));
                // The arms follow the call, so their sites do too.
                let mapped = self.facts.mapped.get(&key).cloned().unwrap_or_default();
                let mut arms = Vec::new();
                for (name, member) in tree::members(attempt.member_list()) {
                    let rule = tree::name_text(&name).unwrap_or_default();
                    let rule = (rule != "invalid").then_some(rule);
                    let failure = mapped
                        .iter()
                        .find(|(key, _)| *key == rule)
                        .map(|(_, failure)| failure.clone());
                    let failure_key = failure.as_deref().and_then(|f| self.type_key(f));
                    let fields = match (member.value(), &failure) {
                        (Some(Expr::RecordExpr(record)), Some(failure)) => {
                            if let Some(Expr::NameRef(name)) = record.expr() {
                                self.path_names(&[name]);
                            }
                            self.fields_given(record.member_list(), failure, &[])
                        }
                        (Some(value), _) => {
                            self.scan(value.syntax());
                            Vec::new()
                        }
                        (None, _) => Vec::new(),
                    };
                    // An arm naming no rule is reported; it maps nothing.
                    let rule = match rule.as_deref().map(RuleKey::from_word) {
                        None => None,
                        Some(Some(key)) => Some(key),
                        Some(None) => continue,
                    };
                    if let Some(failure) = failure_key {
                        arms.push(typed::RuleArm {
                            rule,
                            failure,
                            fields,
                        });
                    }
                }
                typed::Expr::Attempt { inner, arms }
            }
            Expr::RejectExpr(reject) => self.reject(reject),
            Expr::PropagateExpr(_) => typed::Expr::Propagate {
                failure: self
                    .facts
                    .propagates
                    .get(&key)
                    .and_then(|failure| self.type_key(failure)),
            },
            Expr::LambdaExpr(lambda) => {
                let param = lambda.name().and_then(|name| self.declared(&name));
                let body = lambda.expr().map(|body| self.expr(&body));
                typed::Expr::Inline { param, body }
            }
            Expr::ParenExpr(_)
            | Expr::OrderedExpr(_)
            | Expr::RoutePathExpr(_)
            | Expr::GenericExpr(_)
            | Expr::NullableExpr(_)
            | Expr::ElisionExpr(_) => {
                self.scan(expr.syntax());
                typed::Expr::Unchecked
            }
        }
    }

    fn literal(&mut self, expr: &Expr) -> typed::Expr {
        literal(&Const::from_expr(expr)).map_or(typed::Expr::Unchecked, typed::Expr::Literal)
    }

    /// `value.field`, a variant's data's field, or `Enum.variant`.
    fn field(&mut self, expr: &Expr, field: &ast::FieldExpr) -> typed::Expr {
        let key = node_key(expr.syntax());
        if let Some(name) = self.facts.settings.get(&key) {
            return typed::Expr::Config {
                field: self
                    .names
                    .registry()
                    .key(jadpo_graph::Kind::ConfigField, name, None),
            };
        }
        if self.facts.stored.contains(&key)
            && let Some((first, rest)) = dotted(expr)
        {
            let refs: Vec<ast::NameRef> = std::iter::once(first).chain(rest).collect();
            let names: Vec<String> = refs.iter().filter_map(tree::name_ref_text).collect();
            self.path_names(&refs);
            if let [entity, name] = names.as_slice() {
                return typed::Expr::Stored {
                    field: field_key(self.names.registry(), entity, std::slice::from_ref(name)),
                };
            }
        }
        if let Some((host, variant)) = self.facts.variants.get(&key).cloned() {
            if let Some((first, rest)) = dotted(expr) {
                let refs: Vec<ast::NameRef> = std::iter::once(first).chain(rest).collect();
                self.path_names(&refs);
            }
            return typed::Expr::Build {
                callee: Callee::Variant(variant_key(self.names.registry(), &host, &variant)),
                fields: Vec::new(),
                fallible: typed::Fallible::Never,
            };
        }
        if let Some((host, path)) = self.facts.fields.get(&key).cloned()
            && let Some(receiver) = field.expr()
        {
            let receiver = self.expr(&receiver);
            let field_key = field_key(self.names.registry(), &host, &path);
            if let Some(name) = field.name_ref() {
                self.node(name.syntax(), Some(field_key.clone()), false);
            }
            return typed::Expr::Field {
                receiver,
                field: field_key,
            };
        }
        self.scan(expr.syntax());
        typed::Expr::Unchecked
    }

    fn call(&mut self, call: &ast::CallExpr) -> typed::Expr {
        let key = node_key(call.syntax());
        if let Some(fact) = self.facts.writes.get(&key) {
            let block = self.write_block.take().or_else(|| fact.block.clone());
            return self.write(fact, call.expr(), Some(call), block);
        }
        let reach = self
            .facts
            .reaches
            .get(&key)
            .cloned()
            .unwrap_or(Reach::Unknown);
        let callee = match &reach {
            Reach::Callable { kind, name, .. } => match self.type_key(name) {
                Some(key) => match kind {
                    jadpo_db::ItemKind::Function => Callee::Function(key),
                    jadpo_db::ItemKind::Query => Callee::Query(key),
                    jadpo_db::ItemKind::Fixture => Callee::Fixture(key),
                    _ => Callee::Action(key),
                },
                None => Callee::Unknown,
            },
            Reach::Library {
                library, function, ..
            } => Callee::LibraryFunction(library_function_key(
                self.names.registry(),
                library,
                function,
            )),
            Reach::Std(op) => Callee::Std(*op),
            Reach::Construct(ty) => Callee::Construct(self.ty(ty)),
            Reach::Entity { entity, operation } => match self.type_key(entity) {
                Some(key) => Callee::Entity {
                    entity: key,
                    operation: operation.clone(),
                },
                None => Callee::Unknown,
            },
            Reach::Caller(kind) => Callee::Caller(*kind),
            Reach::Session(op) => Callee::Session(*op),
            Reach::Unknown => Callee::Unknown,
        };
        if let Some(head) = call.expr() {
            self.callee_names(&head, &reach, &callee);
        }
        let arguments: Vec<ast::Arg> = call
            .arg_list()
            .iter()
            .flat_map(ast::ArgList::args)
            .collect();
        let args = match (&reach, self.facts.slots.get(&key).cloned()) {
            (Reach::Std(op), Some(slots)) => {
                let mut args = vec![None; op.operation().params.len()];
                for (arg, slot) in arguments.iter().zip(slots) {
                    let lowered = arg.expr().map(|value| self.expr(&value));
                    if let Some(slot) = slot
                        && let Some(place) = args.get_mut(slot)
                    {
                        *place = lowered;
                    }
                }
                args
            }
            _ => arguments
                .iter()
                .map(|arg| arg.expr().map(|value| self.expr(&value)))
                .collect(),
        };
        let fallible = self.fallible(self.facts.fallible.get(&key));
        typed::Expr::Call {
            callee,
            args,
            fallible,
        }
    }

    /// What each name in a callee refers to: the callable, its library or
    /// domain, or the type being constructed.
    fn callee_names(&mut self, head: &Expr, reach: &Reach, callee: &Callee) {
        let Some((first, rest)) = dotted(head) else {
            self.scan(head.syntax());
            return;
        };
        match reach {
            Reach::Construct(_) | Reach::Entity { .. } => {
                let refs: Vec<ast::NameRef> = std::iter::once(first).chain(rest).collect();
                self.path_names(&refs);
            }
            Reach::Callable { .. } | Reach::Library { .. } => {
                let last = rest.last().cloned().unwrap_or_else(|| first.clone());
                if let Some(qualifier) = (!rest.is_empty()).then_some(&first)
                    && let Some(text) = tree::name_ref_text(qualifier)
                {
                    let qualifies = if self.names.library(&text).is_some() {
                        Some(
                            self.names
                                .registry()
                                .key(jadpo_graph::Kind::Library, &text, None),
                        )
                    } else if self.names.is_domain(&text) {
                        Some(
                            self.names
                                .registry()
                                .key(jadpo_graph::Kind::Domain, &text, None),
                        )
                    } else {
                        None
                    };
                    self.node(qualifier.syntax(), qualifies, false);
                }
                self.node(last.syntax(), callee.key().cloned(), false);
            }
            Reach::Std(_) | Reach::Session(_) => {}
            Reach::Caller(_) => {
                self.scan(head.syntax());
            }
            Reach::Unknown => {
                if self.facts.reads.contains_key(&node_key(first.syntax())) {
                    self.read(&first);
                } else {
                    self.scan(head.syntax());
                }
            }
        }
    }

    fn fallible(&self, fallible: Option<&Fallible>) -> typed::Fallible {
        match fallible {
            Some(Fallible::Never) => typed::Fallible::Never,
            Some(Fallible::Fails(names)) => {
                let keys: Vec<Key> = names
                    .iter()
                    .filter_map(|name| self.type_key(name))
                    .collect();
                if keys.len() == names.len() {
                    typed::Fallible::Fails(keys)
                } else {
                    typed::Fallible::Unknown
                }
            }
            Some(Fallible::Construction(rules)) => typed::Fallible::Construction {
                rules: rules
                    .iter()
                    .filter_map(|rule| RuleKey::from_word(rule))
                    .collect(),
            },
            Some(Fallible::Unknown) | None => typed::Fallible::Unknown,
        }
    }

    /// `Type { ... }` or `Enum.variant { ... }`.
    fn record(&mut self, record: &ast::RecordExpr) -> typed::Expr {
        let key = node_key(record.syntax());
        if let Some(fact) = self.facts.requests.get(&key).cloned() {
            return self.request(&fact, record);
        }
        if let Some(fact) = self.facts.writes.get(&key) {
            return self.write(fact, record.expr(), None, record.member_list());
        }
        if let Some(Expr::CallExpr(call)) = record.expr()
            && let Some(fact) = self.facts.writes.get(&node_key(call.syntax()))
        {
            return self.write(fact, call.expr(), Some(&call), record.member_list());
        }
        let Some(build) = self.facts.builds.get(&key).cloned() else {
            self.scan(record.syntax());
            return typed::Expr::Unchecked;
        };
        if let Some(head) = record.expr()
            && let Some((first, rest)) = dotted(&head)
        {
            let refs: Vec<ast::NameRef> = std::iter::once(first).chain(rest).collect();
            self.path_names(&refs);
        }
        let (callee, host, prefix) = match &build {
            BuildFact::Object(name) => (
                self.type_key(name).map_or(Callee::Unknown, Callee::Object),
                name.clone(),
                Vec::new(),
            ),
            BuildFact::Variant(host, variant) => (
                Callee::Variant(variant_key(self.names.registry(), host, variant)),
                host.clone(),
                vec![variant.clone()],
            ),
        };
        let fields = self.fields_given(record.member_list(), &host, &prefix);
        let fallible = match self.facts.constructions.get(&key) {
            Some(verdict) => self.fallible(Some(verdict)),
            None => typed::Fallible::Never,
        };
        typed::Expr::Build {
            callee,
            fields,
            fallible,
        }
    }

    /// A route test's request (area 13): its placeholders in the route's
    /// order, then its input.
    fn request(
        &mut self,
        fact: &super::request::RequestFact,
        record: &ast::RecordExpr,
    ) -> typed::Expr {
        let members = crate::tree::members(record.member_list());
        let value = |name: &str| {
            members
                .iter()
                .find(|(key, _)| crate::tree::name_text(key).as_deref() == Some(name))
                .map(|(_, member)| member.clone())
        };
        let given: Vec<(String, Expr)> = value("params")
            .map(|member| {
                crate::tree::members(member.block())
                    .into_iter()
                    .filter_map(|(name, param)| {
                        Some((crate::tree::name_text(&name)?, param.value()?))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let mut params = Vec::new();
        for name in &fact.params {
            if let Some((_, expr)) = given.iter().find(|(given, _)| given == name) {
                params.push(self.expr(expr));
            }
        }
        let input = value("input")
            .and_then(|member| member.value())
            .map(|expr| self.expr(&expr));
        let idempotency_key = value("idempotency_key")
            .and_then(|member| member.value())
            .map(|expr| self.expr(&expr));
        let caller = value("as")
            .and_then(|member| member.value())
            .map(|expr| self.expr(&expr));
        if let Some(method) = value("method").and_then(|member| member.value()) {
            self.scan(method.syntax());
        }
        typed::Expr::Request {
            route: fact.route.clone(),
            params,
            input,
            idempotency_key,
            caller,
        }
    }

    /// An entity's write (area 6): its record, then its block's sections
    /// in the order they are written.
    fn write(
        &mut self,
        fact: &WriteFact,
        head: Option<Expr>,
        call: Option<&ast::CallExpr>,
        block: Option<ast::MemberList>,
    ) -> typed::Expr {
        let Some(entity) = self.type_key(&fact.entity) else {
            if let Some(call) = call {
                self.scan(call.syntax());
            }
            return typed::Expr::Unchecked;
        };
        if let Some(head) = &head
            && let Some((first, rest)) = dotted(head)
        {
            let refs: Vec<ast::NameRef> = std::iter::once(first).chain(rest).collect();
            self.path_names(&refs);
        }
        let target = call
            .and_then(|call| call.arg_list())
            .and_then(|list| list.args().next())
            .and_then(|arg| arg.expr())
            .map(|value| self.expr(&value));
        let mut set = Vec::new();
        let mut patch = None;
        let mut patch_input: Option<String> = None;
        let mut when_supplied = Vec::new();
        let mut conflicts = Vec::new();
        for (name, member) in tree::members(block) {
            match tree::name_text(&name).as_deref() {
                Some("set") => set.extend(self.fields_given(member.block(), &fact.entity, &[])),
                Some("patch") => {
                    if let Some(value) = member.value() {
                        if let Some(Ty::Named(input)) =
                            self.facts.types.get(&node_key(value.syntax()))
                        {
                            patch_input = Some(input.clone());
                        }
                        patch = Some(self.expr(&value));
                    }
                }
                Some("when_supplied") => {
                    for (supplied, fields) in tree::members(member.block()) {
                        let Some(supplied_name) = tree::name_text(&supplied) else {
                            continue;
                        };
                        let input_field = patch_input.as_ref().map(|input| {
                            field_key(
                                self.names.registry(),
                                input,
                                std::slice::from_ref(&supplied_name),
                            )
                        });
                        self.node(supplied.syntax(), input_field.clone(), false);
                        let given = self.fields_given(fields.block(), &fact.entity, &[]);
                        if let Some(input_field) = input_field {
                            when_supplied.push((input_field, given));
                        }
                    }
                }
                Some("conflict") => {
                    let arms: Vec<ast::Member> = member
                        .block()
                        .into_iter()
                        .flat_map(|list| list.member_items())
                        .filter_map(|item| match item {
                            ast::MemberItem::Member(arm) => Some(arm),
                            _ => None,
                        })
                        .collect();
                    for arm in arms {
                        let Some((_, breach, failure)) = fact
                            .arms
                            .iter()
                            .find(|(written, _, _)| written.syntax() == arm.syntax())
                        else {
                            self.scan(arm.syntax());
                            continue;
                        };
                        let rule = match breach {
                            Breach::Unique {
                                entity: holder,
                                name,
                                field: true,
                            } => {
                                field_key(self.names.registry(), holder, std::slice::from_ref(name))
                            }
                            Breach::Unique {
                                entity: holder,
                                name,
                                ..
                            } => {
                                let born = self.names.registry().born(
                                    jadpo_graph::Kind::Entity,
                                    holder,
                                    None,
                                );
                                self.names.registry().key(
                                    jadpo_graph::Kind::UniqueRule,
                                    &format!("{holder}.{name}"),
                                    Some((holder, &born)),
                                )
                            }
                            Breach::Reference {
                                entity: holder,
                                field,
                            } => {
                                let born = self.names.registry().born(
                                    jadpo_graph::Kind::Entity,
                                    holder,
                                    None,
                                );
                                self.names.registry().key(
                                    jadpo_graph::Kind::Reference,
                                    &format!("{holder}.{field}"),
                                    Some((holder, &born)),
                                )
                            }
                            Breach::Transition {
                                entity: holder,
                                name,
                            } => transition_key(self.names.registry(), holder, name),
                        };
                        if let Some(key) = arm.member_key() {
                            let names: Vec<ast::Name> = key.names().collect();
                            if let Some(first) = names.first() {
                                let holder = breach.holder().to_string();
                                let holder_key = self.type_key(&holder);
                                self.node(first.syntax(), holder_key, false);
                            }
                            if let Some(last) = names.get(1) {
                                self.node(last.syntax(), Some(rule.clone()), false);
                            }
                        }
                        let fields = match arm.value() {
                            Some(Expr::RecordExpr(record)) => {
                                if let Some(Expr::NameRef(name)) = record.expr() {
                                    self.path_names(&[name]);
                                }
                                self.fields_given(record.member_list(), failure, &[])
                            }
                            Some(Expr::NameRef(name)) => {
                                self.path_names(&[name]);
                                Vec::new()
                            }
                            Some(other) => {
                                self.scan(other.syntax());
                                Vec::new()
                            }
                            None => Vec::new(),
                        };
                        if let Some(failure) = self.type_key(failure) {
                            conflicts.push(typed::ConflictArm {
                                rule,
                                failure,
                                fields,
                            });
                        }
                    }
                }
                _ => self.scan(member.syntax()),
            }
        }
        let missing = fact
            .missing
            .iter()
            .filter_map(|(field, failure)| {
                Some((
                    field_key(
                        self.names.registry(),
                        &fact.entity,
                        std::slice::from_ref(field),
                    ),
                    self.type_key(failure)?,
                ))
            })
            .collect();
        let transition = fact
            .transition
            .as_deref()
            .map(|name| transition_key(self.names.registry(), &fact.entity, name));
        typed::Expr::Write {
            entity,
            op: fact.op,
            transition,
            target,
            by_id: fact.by_id,
            set,
            patch,
            when_supplied,
            conflicts,
            missing,
            not_found: fact
                .not_found
                .as_deref()
                .and_then(|failure| self.type_key(failure)),
            fallible: self.fallible(Some(&fact.fallible)),
        }
    }

    /// The fields given in a block, each as its field's key and value.
    fn fields_given(
        &mut self,
        list: Option<ast::MemberList>,
        host: &str,
        prefix: &[String],
    ) -> Vec<(Key, SiteId)> {
        let mut out = Vec::new();
        for (name, member) in tree::members(list) {
            let Some(text) = tree::name_text(&name) else {
                continue;
            };
            let mut path = prefix.to_vec();
            path.push(text);
            let field = field_key(self.names.registry(), host, &path);
            self.node(name.syntax(), Some(field.clone()), false);
            if let Some(value) = member.value() {
                out.push((field, self.expr(&value)));
            }
        }
        out
    }

    fn reject(&mut self, reject: &ast::RejectExpr) -> typed::Expr {
        let key = node_key(reject.syntax());
        let failure = self.facts.rejects.get(&key).cloned();
        let failure_key = failure.as_deref().and_then(|name| self.type_key(name));
        let fields = match (reject.expr(), &failure) {
            (Some(Expr::NameRef(name)), _) => {
                self.path_names(&[name]);
                Vec::new()
            }
            (Some(Expr::RecordExpr(record)), Some(failure)) => {
                if let Some(Expr::NameRef(name)) = record.expr() {
                    self.path_names(&[name]);
                }
                self.fields_given(record.member_list(), failure, &[])
            }
            (Some(other), _) => {
                self.scan(other.syntax());
                Vec::new()
            }
            (None, _) => Vec::new(),
        };
        typed::Expr::Reject {
            failure: failure_key,
            fields,
        }
    }

    fn match_expr(&mut self, subject: &ast::MatchExpr) -> typed::Expr {
        let key = node_key(subject.syntax());
        let at = self.sites.len() - 1;
        let lowered_subject = subject.expr().map(|value| self.expr(&value));
        let mut arms = Vec::new();
        for arm in subject
            .match_arm_list()
            .iter()
            .flat_map(ast::MatchArmList::match_arms)
        {
            let branch = self.branch(at, arm.syntax());
            let (pattern, binding) = match arm.pat() {
                Some(pat) => self.pattern(&pat),
                None => (Pattern::Unknown, None),
            };
            let body = match (arm.block(), arm.expr()) {
                (Some(block), _) => ArmBody::Block(self.block(Some(block))),
                (None, Some(value)) => ArmBody::Value(self.expr(&value)),
                (None, None) => ArmBody::Empty,
            };
            arms.push(typed::Arm {
                pattern,
                binding,
                body,
                branch,
            });
        }
        typed::Expr::Match {
            subject: lowered_subject,
            outcome: self.facts.outcomes.contains(&key),
            arms,
        }
    }

    fn pattern(&mut self, pat: &Pat) -> (Pattern, Option<LocalId>) {
        match pat {
            Pat::WildcardPat(_) => (Pattern::Wildcard, None),
            Pat::LiteralPat(written) => {
                let value = written.literal().map_or(Const::Other, |value| {
                    Const::from_literal(&value, written.minus_token().is_some())
                });
                let pattern = match value {
                    Const::Bool(true) => Pattern::True,
                    Const::Bool(false) => Pattern::False,
                    Const::None => Pattern::None,
                    other => literal(&other).map_or(Pattern::Unknown, Pattern::Literal),
                };
                (pattern, None)
            }
            Pat::CasePat(case) => {
                let refs: Vec<ast::NameRef> = case
                    .path()
                    .map(|path| path.name_refs().collect())
                    .unwrap_or_default();
                let pattern = match self.facts.patterns.get(&node_key(pat.syntax())).cloned() {
                    Some(PatternFact::Variant(host, variant)) => {
                        self.path_names(&refs);
                        Pattern::Variant(variant_key(self.names.registry(), &host, &variant))
                    }
                    Some(PatternFact::Some) => Pattern::Some,
                    Some(PatternFact::Success) => Pattern::Success,
                    Some(PatternFact::Failure(failure)) => {
                        self.path_names(&refs);
                        self.type_key(&failure)
                            .map_or(Pattern::Unknown, Pattern::Failure)
                    }
                    Some(PatternFact::Wildcard) => Pattern::Wildcard,
                    Some(PatternFact::Unknown) | None => {
                        self.path_names(&refs);
                        Pattern::Unknown
                    }
                };
                let binding = case
                    .pat_binding()
                    .and_then(|binding| binding.name())
                    .and_then(|name| self.declared(&name));
                (pattern, binding)
            }
        }
    }
}

/// The key of `entity`'s transition `name`: `transition Loan.check_in`,
/// under the entity so a rename of the entity carries it.
pub(crate) fn transition_key(registry: &jadpo_graph::Registry, entity: &str, name: &str) -> Key {
    let born = registry.born(jadpo_graph::Kind::Entity, entity, None);
    registry.key(
        jadpo_graph::Kind::Transition,
        &format!("{entity}.{name}"),
        Some((entity, &born)),
    )
}
