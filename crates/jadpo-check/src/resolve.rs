//! Name resolution (area 1): every name in a type position, every called
//! name and every qualifier before a dot is looked up in the project's symbol
//! table, the standard library or the scope of locals, and checked for
//! kind, visibility and direction.
//!
//! A file's resolution reads its own tree and the symbol table, never
//! another file's tree, so editing a body elsewhere does not recompute it.
//! Lower-case names used as values (`total`, `todo.title`) are area 3's to
//! check, with types.

use jadpo_db::{
    Base, Db, File, ItemKind, Ownership, Place, Project, Symbol, Symbols, location, symbols,
};
use jadpo_diag::{Diagnostic, Span, nearest, rule};
use jadpo_syntax::ast::{self, AstNode, BlockItem, Decl, Expr, MemberItem, Pat, Stmt, Type};

use crate::case::{is_lower_snake, starts_upper, to_snake};
use crate::naming::{name_case, reserved};
use crate::std_names::{Namespace, StdNames};
use crate::tree::{self, dotted, node_span};

/// What a type position accepts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Want {
    Type,
    Failure,
}

/// Resolution problems of one file.
#[salsa::tracked(returns(ref))]
pub fn resolve_file(db: &dyn Db, project: Project, file: File) -> Vec<Diagnostic> {
    let Some(source) = tree::source(db, file) else {
        return Vec::new();
    };
    let location = location(db, project, file);
    let mut resolver = Resolver {
        path: file.path(db),
        ownership: &location.ownership,
        root_tests: location.place == Place::Tests,
        table: symbols(db, project),
        names: StdNames::get(),
        scopes: Vec::new(),
        library: None,
        out: Vec::new(),
    };
    for decl in source.decls() {
        resolver.decl(&decl);
    }
    resolver.out
}

struct Resolver<'a> {
    path: &'a str,
    ownership: &'a Ownership,
    root_tests: bool,
    table: &'a Symbols,
    names: &'static StdNames,
    scopes: Vec<Vec<String>>,
    /// The library whose functions are being resolved: they call each
    /// other bare, as helpers in their own file.
    library: Option<String>,
    out: Vec<Diagnostic>,
}

fn text(name: &ast::NameRef) -> String {
    tree::name_ref_text(name).unwrap_or_default()
}

fn did_you_mean(near: Option<&str>) -> String {
    near.map_or_else(String::new, |near| format!(" Did you mean `{near}`?"))
}

impl Resolver<'_> {
    fn span(&self, node: &impl AstNode) -> Span {
        node_span(self.path, node.syntax())
    }

    fn decl(&mut self, decl: &Decl) {
        match decl {
            Decl::TypeDecl(d) => self.type_decl(d),
            Decl::FunctionDecl(d) => {
                self.callable(d.param_list(), d.fails_clause(), d.ret_type(), d.block())
            }
            Decl::ActionDecl(d) => {
                self.callable(d.param_list(), d.fails_clause(), d.ret_type(), d.block())
            }
            Decl::QueryDecl(d) => {
                self.callable(d.param_list(), d.fails_clause(), d.ret_type(), None)
            }
            Decl::FixtureDecl(d) => self.callable(d.param_list(), None, d.ret_type(), d.block()),
            Decl::LibraryDecl(d) => {
                self.library = d.name().as_ref().and_then(tree::name_text);
                for function in tree::library_functions(d) {
                    self.callable(
                        function.param_list(),
                        function.fails_clause(),
                        function.ret_type(),
                        function.block(),
                    );
                }
                self.library = None;
            }
            Decl::TestDecl(d) => self.block(d.block()),
            Decl::SeedDecl(d) => self.block(d.block()),
            _ => {}
        }
    }

    fn callable(
        &mut self,
        params: Option<ast::ParamList>,
        fails: Option<ast::FailsClause>,
        result: Option<ast::RetType>,
        body: Option<ast::Block>,
    ) {
        self.scopes.push(Vec::new());
        for param in params.iter().flat_map(|list| list.params()) {
            self.ty(param.ty(), Want::Type);
            self.declare(param.name(), "parameter");
        }
        for ty in fails.iter().flat_map(|clause| clause.types()) {
            self.ty(Some(ty), Want::Failure);
        }
        self.ty(result.and_then(|ret| ret.ty()), Want::Type);
        self.block(body);
        self.scopes.pop();
    }

    fn type_decl(&mut self, decl: &ast::TypeDecl) {
        let base_text = decl
            .base()
            .map(|base| tree::written(base.syntax()))
            .unwrap_or_default();
        self.ty(decl.base(), Want::Type);
        let base = match base_text.as_str() {
            "Object" => Base::Object,
            "Enum" => Base::Enum,
            "Input" => Base::Input,
            "Output" => Base::Output,
            "Failure" => Base::Failure,
            "Event" => Base::Event,
            "EmailMessage" => Base::EmailMessage,
            other => Base::Named(other.to_string()),
        };
        let list = decl.member_list();
        match base {
            Base::Failure => {
                for (name, member) in tree::members(list) {
                    if matches!(
                        tree::name_text(&name).as_deref(),
                        Some("client" | "internal")
                    ) {
                        self.fields(member.block());
                    }
                }
            }
            Base::Enum => {
                for item in list.iter().flat_map(|list| list.member_items()) {
                    match item {
                        MemberItem::BareMember(variant) => {
                            self.member_name(variant.name(), "variant")
                        }
                        MemberItem::Member(variant) => {
                            if let Some(key) = variant.member_key() {
                                self.member_name(key.names().next(), "variant");
                            }
                            if let Some(value) = variant.value() {
                                self.type_expr(&value, Want::Type);
                            }
                            self.fields(variant.block());
                        }
                        _ => {}
                    }
                }
            }
            base if base.has_fields() => self.fields(list),
            _ => {}
        }
    }

    /// A block of fields: `name: Type`.
    fn fields(&mut self, list: Option<ast::MemberList>) {
        for item in list.iter().flat_map(|list| list.member_items()) {
            if let MemberItem::Member(field) = item {
                if let Some(key) = field.member_key() {
                    self.member_name(key.names().next(), "field");
                }
                if let Some(value) = field.value() {
                    self.type_expr(&value, Want::Type);
                }
            }
        }
    }

    fn member_name(&mut self, name: Option<ast::Name>, word: &str) {
        let Some(name) = name else { return };
        let Some(text) = tree::name_text(&name) else {
            return;
        };
        if !is_lower_snake(&text) {
            let span = self.span(&name);
            self.out.push(name_case(
                &text,
                &to_snake(&text),
                word,
                "lower_snake_case",
                span,
            ));
        }
    }

    /// A parameter, variable or binding coming into scope.
    fn declare(&mut self, name: Option<ast::Name>, word: &str) {
        let Some(name) = name else { return };
        let Some(text) = tree::name_text(&name) else {
            return;
        };
        let span = self.span(&name);
        if !is_lower_snake(&text) {
            self.out.push(name_case(
                &text,
                &to_snake(&text),
                word,
                "lower_snake_case",
                span,
            ));
        } else if self.names.reserved(&text) {
            self.out.push(reserved(&text, span));
        } else if self.table.domains.contains(&text) || self.table.library(&text).is_some() {
            let what = if self.table.domains.contains(&text) {
                "domain"
            } else {
                "library"
            };
            self.out.push(
                Diagnostic::new(
                    &rule::SEMANTIC_DUPLICATE_DECLARATION,
                    format!("`{text}` is already a {what}"),
                )
                .at(span, format!("named like a {what}"))
                .reason("A variable can't reuse a domain's or library's name, so `name.call(...)` has one meaning.")
                .next(format!("Rename this {word}")),
            );
        }
        if let Some(scope) = self.scopes.last_mut() {
            scope.push(text);
        }
    }

    fn is_local(&self, name: &str) -> bool {
        self.scopes
            .iter()
            .any(|scope| scope.iter().any(|local| local == name))
    }

    // --- types -----------------------------------------------------------

    fn ty(&mut self, ty: Option<Type>, want: Want) {
        match ty {
            Some(Type::PathType(path_type)) => {
                let names: Vec<ast::NameRef> = path_type
                    .path()
                    .into_iter()
                    .flat_map(|path| path.name_refs())
                    .collect();
                if let Some(first) = names.first() {
                    let packaged = names.len() > 1 && self.table.packages.contains(&text(first));
                    if !packaged {
                        self.type_name(first, want);
                    }
                }
                for arg in path_type
                    .generic_arg_list()
                    .into_iter()
                    .flat_map(|list| list.types())
                {
                    self.ty(Some(arg), Want::Type);
                }
            }
            Some(Type::NullableType(nullable)) => self.ty(nullable.ty(), want),
            None => {}
        }
    }

    /// A type written as a member's value: `Count`, `Todo.id`, `List<Tag>`,
    /// `Name?`, `ProductName { optional: true }`.
    fn type_expr(&mut self, expr: &Expr, want: Want) {
        match expr {
            Expr::NameRef(name) => self.type_name(name, want),
            Expr::FieldExpr(_) => {
                if let Some((first, _)) = dotted(expr)
                    && !self.table.packages.contains(&text(&first))
                {
                    self.type_name(&first, want);
                }
            }
            Expr::GenericExpr(generic) => {
                if let Some(inner) = generic.expr() {
                    self.type_expr(&inner, Want::Type);
                }
                for arg in generic
                    .generic_arg_list()
                    .into_iter()
                    .flat_map(|list| list.types())
                {
                    self.ty(Some(arg), Want::Type);
                }
            }
            Expr::NullableExpr(nullable) => {
                if let Some(inner) = nullable.expr() {
                    self.type_expr(&inner, want);
                }
            }
            Expr::RecordExpr(record) => {
                if let Some(inner) = record.expr() {
                    self.type_expr(&inner, want);
                }
            }
            _ => {}
        }
    }

    fn type_name(&mut self, name: &ast::NameRef, want: Want) {
        let text = text(name);
        let span = self.span(name);
        if self.names.is_type(&text) {
            if want == Want::Failure {
                self.out.push(not_a_failure(&text, "a built-in type", span));
            }
            return;
        }
        if let Some(symbol) = self.table.get(&text) {
            let fits = match (&symbol.kind, want) {
                (ItemKind::Type(Base::Failure), _) => true,
                (ItemKind::Type(_) | ItemKind::Entity, Want::Type) => true,
                (ItemKind::Type(_) | ItemKind::Entity, Want::Failure) => {
                    let what = symbol.kind.with_a();
                    self.out.push(not_a_failure(&text, &what, span));
                    return;
                }
                _ => false,
            };
            if fits {
                let symbol = symbol.clone();
                self.access(&symbol, span);
            } else {
                let what = symbol.kind.with_a();
                self.out.push(not_a_type(&text, &what, span));
            }
            return;
        }
        if let Some(what) = self.qualifier_kind(&text) {
            self.out.push(not_a_type(&text, &format!("a {what}"), span));
            return;
        }
        let candidates = self.type_names(want);
        let near = nearest(&text, candidates.iter().map(String::as_str)).map(str::to_string);
        self.out.push(unknown_name(&text, near.as_deref(), span));
    }

    /// Every type name a position could have meant.
    fn type_names(&self, want: Want) -> Vec<String> {
        let mut names: Vec<String> = self
            .table
            .declarations
            .iter()
            .filter(|(_, found)| {
                found.iter().any(|symbol| match want {
                    Want::Type => matches!(symbol.kind, ItemKind::Type(_) | ItemKind::Entity),
                    Want::Failure => symbol.kind.is_failure(),
                })
            })
            .map(|(name, _)| name.clone())
            .collect();
        if want == Want::Type {
            names.extend(self.names.types().map(str::to_string));
        }
        names
    }

    /// What a lower-case name that owns calls is, if it is one.
    fn qualifier_kind(&self, name: &str) -> Option<&'static str> {
        if self.table.domains.contains(name) {
            Some("domain")
        } else if self.table.library(name).is_some() {
            Some("library")
        } else if self.names.reserved(name) {
            Some("standard-library namespace")
        } else if self.table.connections.contains(name) {
            Some("connection")
        } else {
            None
        }
    }

    /// Checks that this file may use `symbol`: visible from its domain, and
    /// not domain code used from `shared/`.
    fn access(&mut self, symbol: &Symbol, span: Span) {
        if *self.ownership == Ownership::Shared {
            if let Ownership::Domain(domain) = &symbol.ownership {
                self.out.push(call_direction(symbol, domain, span));
            }
            return;
        }
        if !symbol.visible_from(self.ownership)
            && let Ownership::Domain(domain) = &symbol.ownership
        {
            self.out.push(private(symbol, domain, span));
        }
    }

    // --- bodies ----------------------------------------------------------

    fn block(&mut self, block: Option<ast::Block>) {
        let Some(block) = block else { return };
        self.scopes.push(Vec::new());
        for item in block.block_items() {
            match item {
                BlockItem::Stmt(stmt) => self.stmt(&stmt),
                BlockItem::Member(member) => {
                    if let Some(value) = member.value() {
                        self.expr(&value);
                    }
                    self.member_values(member.block());
                }
                BlockItem::Elision(_) => {}
            }
        }
        self.scopes.pop();
    }

    fn stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::VarStmt(var) => {
                self.ty(var.ty(), Want::Type);
                if let Some(value) = var.expr() {
                    self.expr(&value);
                }
                self.declare(var.name(), "variable");
            }
            Stmt::AssignStmt(assign) => {
                for expr in [assign.target(), assign.value()].into_iter().flatten() {
                    self.expr(&expr);
                }
            }
            Stmt::ReturnStmt(ret) => {
                if let Some(expr) = ret.expr() {
                    self.expr(&expr);
                }
            }
            Stmt::IfStmt(branch) => self.if_stmt(branch),
            Stmt::ForStmt(each) => {
                if let Some(expr) = each.expr() {
                    self.expr(&expr);
                }
                self.scopes.push(Vec::new());
                self.declare(each.name(), "variable");
                self.block(each.block());
                self.scopes.pop();
            }
            Stmt::EmitStmt(emit) => {
                if let Some(expr) = emit.expr() {
                    self.expr(&expr);
                }
            }
            Stmt::AssertStmt(assert) => {
                if let Some(expr) = assert.expr() {
                    self.expr(&expr);
                }
                self.ty(
                    assert.assert_fails().and_then(|fails| fails.ty()),
                    Want::Failure,
                );
            }
            Stmt::ExprStmt(expr) => {
                if let Some(expr) = expr.expr() {
                    self.expr(&expr);
                }
            }
        }
    }

    fn if_stmt(&mut self, branch: &ast::IfStmt) {
        if let Some(condition) = branch.condition().and_then(|c| c.expr()) {
            self.expr(&condition);
        }
        self.block(branch.block());
        if let Some(otherwise) = branch.else_branch() {
            if let Some(next) = otherwise.if_stmt() {
                self.if_stmt(&next);
            }
            self.block(otherwise.block());
        }
    }

    fn member_values(&mut self, list: Option<ast::MemberList>) {
        for item in list.iter().flat_map(|list| list.member_items()) {
            if let MemberItem::Member(member) = item {
                if let Some(value) = member.value() {
                    self.expr(&value);
                }
                self.member_values(member.block());
            }
        }
    }

    fn expr(&mut self, expr: &Expr) {
        match expr {
            Expr::CallExpr(call) => self.call(call),
            Expr::RejectExpr(reject) => match reject.expr() {
                Some(Expr::NameRef(name)) => self.type_name(&name, Want::Failure),
                Some(Expr::RecordExpr(record)) => {
                    match record.expr() {
                        Some(Expr::NameRef(name)) => self.type_name(&name, Want::Failure),
                        Some(other) => self.expr(&other),
                        None => {}
                    }
                    self.member_values(record.member_list());
                }
                Some(other) => self.expr(&other),
                None => {}
            },
            Expr::AttemptExpr(attempt) => {
                if let Some(inner) = attempt.expr() {
                    self.expr(&inner);
                }
                for mapping in attempt.mappings() {
                    match mapping.expr() {
                        Some(Expr::NameRef(name)) => self.type_name(&name, Want::Failure),
                        Some(other) => self.expr(&other),
                        None => {}
                    }
                }
            }
            Expr::RecordExpr(record) => {
                match record.expr() {
                    Some(Expr::NameRef(name)) if starts_upper(&text(&name)) => {
                        self.type_name(&name, Want::Type)
                    }
                    Some(other) => self.expr(&other),
                    None => {}
                }
                self.member_values(record.member_list());
            }
            Expr::FieldExpr(_) | Expr::NameRef(_) => {
                if let Some((first, _)) = dotted(expr) {
                    if starts_upper(&text(&first)) {
                        self.type_qualifier(&first, false);
                    }
                } else {
                    self.children(expr);
                }
            }
            Expr::MatchExpr(subject) => {
                if let Some(inner) = subject.expr() {
                    self.expr(&inner);
                }
                for arm in subject
                    .match_arm_list()
                    .into_iter()
                    .flat_map(|list| list.match_arms())
                {
                    self.scopes.push(Vec::new());
                    if let Some(Pat::CasePat(case)) = arm.pat() {
                        if let Some(first) = case.path().and_then(|path| path.name_refs().next())
                            && starts_upper(&text(&first))
                        {
                            self.type_qualifier(&first, false);
                        }
                        self.declare(case.pat_binding().and_then(|b| b.name()), "variable");
                    }
                    if let Some(value) = arm.expr() {
                        self.expr(&value);
                    }
                    self.block(arm.block());
                    self.scopes.pop();
                }
            }
            Expr::LambdaExpr(lambda) => {
                self.scopes.push(Vec::new());
                self.declare(lambda.name(), "parameter");
                if let Some(body) = lambda.expr() {
                    self.expr(&body);
                }
                self.scopes.pop();
            }
            _ => self.children(expr),
        }
    }

    fn children(&mut self, expr: &Expr) {
        for child in expr.syntax().children().filter_map(Expr::cast) {
            self.expr(&child);
        }
    }

    fn call(&mut self, call: &ast::CallExpr) {
        match call.expr() {
            Some(Expr::NameRef(name)) => self.bare_call(&name, call),
            Some(callee @ Expr::FieldExpr(_)) => match dotted(&callee) {
                Some((first, rest)) => self.dotted_call(&first, &rest),
                None => self.children(&callee),
            },
            Some(other) => self.expr(&other),
            None => {}
        }
        for arg in call.arg_list().iter().flat_map(|list| list.args()) {
            if let Some(value) = arg.expr() {
                self.expr(&value);
            }
        }
    }

    /// `name(...)`: a function, action or query of this domain, or a type
    /// being constructed.
    fn bare_call(&mut self, name: &ast::NameRef, call: &ast::CallExpr) {
        let text = text(name);
        let span = self.span(name);
        if starts_upper(&text) {
            self.constructor(name, call);
            return;
        }
        if self.is_local(&text) {
            self.out.push(not_callable(
                &text,
                "a variable",
                "Jadpo has no function values: only functions, actions, queries and types are called.",
                format!("Use `{text}` as a value, or call a function"),
                span,
            ));
            return;
        }
        if let Some(found) = self.table.declarations.get(&text) {
            let symbol = found
                .iter()
                .find(|symbol| {
                    matches!(
                        symbol.kind,
                        ItemKind::Function | ItemKind::Action | ItemKind::Query | ItemKind::Fixture
                    )
                })
                .or(found.first())
                .cloned();
            if let Some(symbol) = symbol {
                self.bare_symbol(&text, &symbol, span);
            }
            return;
        }
        if let Some(what) = self.qualifier_kind(&text) {
            let (reason, next) = match what {
                "domain" => (
                    format!(
                        "A domain's actions and queries are called through it: `{text}.<action>(...)`."
                    ),
                    format!("Call one of `{text}`'s actions or queries"),
                ),
                "library" => (
                    format!(
                        "A library's functions are called through it: `{text}.<function>(...)`."
                    ),
                    format!("Call one of `{text}`'s functions"),
                ),
                _ => (
                    format!(
                        "`{text}` owns operations, called through it: `{text}.<operation>(...)`."
                    ),
                    format!("Call one of `{text}`'s operations"),
                ),
            };
            self.out.push(not_callable(
                &text,
                &format!("a {what}"),
                &reason,
                next,
                span,
            ));
            return;
        }
        if let Some(namespace) = self.names.namespace_of(&text) {
            self.out.push(
                Diagnostic::new(
                    &rule::SEMANTIC_STANDARD_OPERATION_QUALIFICATION,
                    format!("`{text}` is a standard-library operation"),
                )
                .at(span.clone(), "needs its namespace")
                .reason("The standard library is always called through its namespace, so the dot says who owns the operation.")
                .next(format!("Call `{namespace}.{text}(...)`"))
                .edit(Span::new(self.path, span.start, span.start), format!("{namespace}.")),
            );
            return;
        }
        let candidates: Vec<String> = self
            .table
            .declarations
            .iter()
            .filter(|(_, found)| {
                found.iter().any(|symbol| {
                    symbol.library.is_none()
                        && matches!(
                            symbol.kind,
                            ItemKind::Function | ItemKind::Action | ItemKind::Query
                        )
                })
            })
            .map(|(name, _)| name.clone())
            .collect();
        let near = nearest(&text, candidates.iter().map(String::as_str));
        let mut problem = Diagnostic::new(
            &rule::SEMANTIC_UNKNOWN_CALLEE,
            format!("`{text}` isn't a function, action or query"),
        )
        .at(span.clone(), "not declared")
        .reason(format!(
            "Nothing in this project called `{text}` can be called.{}",
            did_you_mean(near)
        ));
        problem = match near {
            Some(near) => problem
                .next(format!("Use `{near}` instead of `{text}`"))
                .edit(span, near),
            None => problem.next(format!("Declare `{text}`, or call an existing function")),
        };
        self.out.push(problem);
    }

    fn bare_symbol(&mut self, text: &str, symbol: &Symbol, span: Span) {
        let other_domain = match (&symbol.ownership, self.ownership) {
            (Ownership::Domain(theirs), Ownership::Domain(ours)) if theirs != ours => {
                Some(theirs.clone())
            }
            (Ownership::Domain(theirs), Ownership::Root) if self.root_tests => Some(theirs.clone()),
            _ => None,
        };
        match &symbol.kind {
            ItemKind::Function => {
                if let Some((library, _)) = &symbol.library
                    && self.library.as_ref() != Some(library)
                {
                    self.out.push(
                        Diagnostic::new(
                            &rule::SEMANTIC_UNKNOWN_CALLEE,
                            format!("`{text}` is in library `{library}`"),
                        )
                        .at(span.clone(), "needs its library")
                        .reason("A library's functions are called through it, like the standard library.")
                        .next(format!("Call `{library}.{text}(...)`"))
                        .edit(Span::new(self.path, span.start, span.start), format!("{library}.")),
                    );
                } else if symbol.library.is_some() {
                } else if let Some(domain) = other_domain {
                    self.out.push(
                        Diagnostic::new(
                            &rule::SEMANTIC_UNKNOWN_CALLEE,
                            format!("`{text}` is a helper in domain `{domain}`"),
                        )
                        .at(span, "not reachable from here")
                        .reason("A helper function is called from its own domain; another domain uses it through a public library.")
                        .next(format!(
                            "Move `{text}` into a public library in `{domain}` and call it through the library"
                        )),
                    );
                } else {
                    self.access(symbol, span);
                }
            }
            ItemKind::Action | ItemKind::Query => match other_domain {
                Some(domain) => self.out.push(
                    Diagnostic::new(
                        &rule::SEMANTIC_UNKNOWN_CALLEE,
                        format!("`{text}` belongs to domain `{domain}`"),
                    )
                    .at(span.clone(), "needs its domain")
                    .reason("Your own domain's actions and queries are called bare; another domain's are called through it, so each call says which domain it reaches.")
                    .next(format!("Call `{domain}.{text}(...)`"))
                    .edit(Span::new(self.path, span.start, span.start), format!("{domain}.")),
                ),
                None => self.access(symbol, span),
            },
            ItemKind::Fixture => {}
            kind => {
                let (reason, next) = match kind {
                    ItemKind::Schedule | ItemKind::Subscriber => (
                        "Schedules and subscribers run actions; code calls the action itself.",
                        "Call the action it runs",
                    ),
                    ItemKind::Seed => (
                        "A seed fills a local database and is run with `jadpo seed`.",
                        "Call an action instead",
                    ),
                    _ => (
                        "Only functions, actions, queries and types are called.",
                        "Call a function, action or query instead",
                    ),
                };
                self.out.push(not_callable(
                    text,
                    &kind.with_a(),
                    reason,
                    next.to_string(),
                    span,
                ));
            }
        }
    }

    /// `Type(value)`: constructing a value of a named type.
    fn constructor(&mut self, name: &ast::NameRef, call: &ast::CallExpr) {
        let text = text(name);
        let span = self.span(name);
        if self.names.is_type(&text) {
            return;
        }
        let Some(symbol) = self.table.get(&text).cloned() else {
            if !self.table.packages.contains(&text) {
                let candidates = self.type_names(Want::Type);
                let near =
                    nearest(&text, candidates.iter().map(String::as_str)).map(str::to_string);
                self.out.push(unknown_name(&text, near.as_deref(), span));
            }
            return;
        };
        match &symbol.kind {
            ItemKind::Type(Base::Failure) => {
                let arguments = call.arg_list();
                let empty = arguments
                    .as_ref()
                    .is_some_and(|list| list.args().next().is_none());
                let mut problem = not_callable(
                    &text,
                    "a failure",
                    &format!(
                        "A failure is raised with `reject {text}`, with its fields in a block when it has any."
                    ),
                    if empty {
                        format!("Write `{text}` without `()`")
                    } else {
                        format!("Raise it with `reject {text} {{ ... }}`, naming each field")
                    },
                    span,
                );
                if empty && let Some(list) = arguments {
                    problem = problem.edit(self.span(&list), "");
                }
                self.out.push(problem);
            }
            ItemKind::Entity => self.out.push(not_callable(
                &text,
                "an entity",
                &format!("An entity is read and written through its operations, such as `{text}.get(...)`."),
                format!("Use one of `{text}`'s operations"),
                span,
            )),
            ItemKind::Routes => self.out.push(not_callable(
                &text,
                "a route group",
                "Routes answer requests; code calls the action a route runs.",
                "Call the action instead".to_string(),
                span,
            )),
            _ => self.access(&symbol, span),
        }
    }

    /// `Name.member` or `Name.member(...)` with an upper-case qualifier: a
    /// type, an entity or a package.
    fn type_qualifier(&mut self, name: &ast::NameRef, calls: bool) {
        let text = text(name);
        let span = self.span(name);
        if self.names.is_type(&text) || self.table.packages.contains(&text) {
            return;
        }
        let Some(symbol) = self.table.get(&text).cloned() else {
            let candidates = self.type_names(Want::Type);
            let near = nearest(&text, candidates.iter().map(String::as_str)).map(str::to_string);
            self.out.push(unknown_name(&text, near.as_deref(), span));
            return;
        };
        if calls
            && symbol.kind == ItemKind::Entity
            && let (Ownership::Domain(theirs), Ownership::Domain(ours)) =
                (&symbol.ownership, self.ownership)
            && theirs != ours
        {
            self.out.push(
                Diagnostic::new(
                    &rule::VISIBILITY_PRIVATE_CROSS_DOMAIN,
                    format!("`{text}` belongs to domain `{theirs}`"),
                )
                .at(span, "another domain's entity")
                .reason("Another domain never reads or writes an entity directly; it calls the owning domain's public actions and queries, which share the caller's transaction.")
                .next(format!("Call a public action or query of `{theirs}` instead")),
            );
            return;
        }
        self.access(&symbol, span);
    }

    /// `qualifier.member(...)` with a lower-case qualifier.
    fn dotted_call(&mut self, first: &ast::NameRef, rest: &[ast::NameRef]) {
        let qualifier = text(first);
        if starts_upper(&qualifier) {
            self.type_qualifier(first, true);
            return;
        }
        if self.is_local(&qualifier) {
            return;
        }
        let qualifier_span = self.span(first);
        let member = match rest {
            [only] => Some((text(only), self.span(only))),
            _ => None,
        };
        match self.names.namespace(&qualifier) {
            Some(Namespace::Standard) => {
                if let Some((member, span)) = member
                    && !self.names.has_operation(&qualifier, &member)
                {
                    let operations: Vec<&str> = self.names.operations(&qualifier).collect();
                    let near = nearest(&member, operations.iter().copied());
                    self.out.push(no_member(
                        &format!("`{qualifier}` has no operation `{member}`"),
                        &member,
                        near,
                        span,
                    ));
                }
                return;
            }
            Some(Namespace::Generated) => return,
            None => {}
        }
        if let Some(library) = self.table.library(&qualifier).cloned() {
            self.access(&library, qualifier_span);
            if let Some((member, span)) = member
                && self.table.library_function(&qualifier, &member).is_none()
            {
                let functions: Vec<String> = self
                    .table
                    .declarations
                    .values()
                    .flatten()
                    .filter(|symbol| {
                        symbol
                            .library
                            .as_ref()
                            .is_some_and(|(library, _)| *library == qualifier)
                    })
                    .map(|symbol| symbol.name.clone())
                    .collect();
                let near = nearest(&member, functions.iter().map(String::as_str));
                self.out.push(no_member(
                    &format!("Library `{qualifier}` has no function `{member}`"),
                    &member,
                    near,
                    span,
                ));
            }
            return;
        }
        if self.table.domains.contains(&qualifier) {
            let Some((member, span)) = member else {
                return;
            };
            let in_domain =
                |symbol: &&Symbol| symbol.ownership == Ownership::Domain(qualifier.clone());
            let found = self.table.declarations.get(&member).and_then(|found| {
                found
                    .iter()
                    .filter(in_domain)
                    .find(|symbol| matches!(symbol.kind, ItemKind::Action | ItemKind::Query))
                    .cloned()
            });
            match found {
                Some(symbol) => self.access(&symbol, span),
                None => {
                    let operations: Vec<String> = self
                        .table
                        .declarations
                        .values()
                        .flatten()
                        .filter(in_domain)
                        .filter(|symbol| matches!(symbol.kind, ItemKind::Action | ItemKind::Query))
                        .map(|symbol| symbol.name.clone())
                        .collect();
                    let near = nearest(&member, operations.iter().map(String::as_str));
                    self.out.push(no_member(
                        &format!("Domain `{qualifier}` has no action or query `{member}`"),
                        &member,
                        near,
                        span,
                    ));
                }
            }
            return;
        }
        if self.table.connections.contains(&qualifier)
            || self
                .table
                .declarations
                .get(&qualifier)
                .is_some_and(|found| found.iter().any(|symbol| symbol.kind == ItemKind::Schedule))
        {
            return;
        }
        let mut qualifiers: Vec<String> = self.table.domains.iter().cloned().collect();
        qualifiers.extend(self.table.connections.iter().cloned());
        qualifiers.extend(
            self.table
                .declarations
                .values()
                .flatten()
                .filter(|symbol| symbol.kind == ItemKind::Library)
                .map(|symbol| symbol.name.clone()),
        );
        let near = nearest(&qualifier, qualifiers.iter().map(String::as_str)).map(str::to_string);
        let mut problem = Diagnostic::new(
            &rule::SEMANTIC_UNKNOWN_NAME,
            format!("`{qualifier}` isn't a domain, library or namespace"),
        )
        .at(qualifier_span.clone(), "not declared")
        .reason(format!(
            "The name before the dot says who owns a call: a domain, a library, a connection or a standard-library namespace.{}",
            did_you_mean(near.as_deref())
        ));
        problem = match near {
            Some(near) => problem
                .next(format!("Use `{near}` instead of `{qualifier}`"))
                .edit(qualifier_span, near),
            None => problem.next(format!(
                "Declare `{qualifier}`, or call through an existing domain or library"
            )),
        };
        self.out.push(problem);
    }
}

fn unknown_name(name: &str, near: Option<&str>, span: Span) -> Diagnostic {
    let problem = Diagnostic::new(
        &rule::SEMANTIC_UNKNOWN_NAME,
        format!("`{name}` isn't declared"),
    )
    .at(span.clone(), "not declared")
    .reason(format!(
        "Nothing in this project or the standard library is called `{name}`.{}",
        did_you_mean(near)
    ));
    match near {
        Some(near) => problem
            .next(format!("Use `{near}` instead of `{name}`"))
            .edit(span, near),
        None => problem.next(format!("Declare `{name}`, or use an existing name")),
    }
}

fn no_member(message: &str, member: &str, near: Option<&str>, span: Span) -> Diagnostic {
    let problem = Diagnostic::new(&rule::SEMANTIC_UNKNOWN_CALLEE, message)
        .at(span.clone(), "not declared")
        .reason(format!(
            "Nothing called `{member}` can be called through it.{}",
            did_you_mean(near)
        ));
    match near {
        Some(near) => problem
            .next(format!("Use `{near}` instead of `{member}`"))
            .edit(span, near),
        None => problem.next("Call something it declares"),
    }
}

fn not_callable(name: &str, what: &str, reason: &str, next: String, span: Span) -> Diagnostic {
    Diagnostic::new(
        &rule::SEMANTIC_NOT_CALLABLE,
        format!("`{name}` is {what} and can't be called"),
    )
    .at(span, "not callable")
    .reason(reason)
    .next(next)
}

fn not_a_type(name: &str, what: &str, span: Span) -> Diagnostic {
    Diagnostic::new(
        &rule::SEMANTIC_WRONG_NAME_KIND,
        format!("`{name}` is {what}, not a type"),
    )
    .at(span, "expected a type")
    .reason("Here Jadpo expects a type, such as `Count` or `List<Count>`.")
    .next("Name a type here")
}

fn not_a_failure(name: &str, what: &str, span: Span) -> Diagnostic {
    Diagnostic::new(
        &rule::SEMANTIC_WRONG_NAME_KIND,
        format!("`{name}` is {what}, not a failure"),
    )
    .at(span, "expected a failure")
    .reason("`fails`, `reject` and failure mappings name failures: types declared `= Failure`.")
    .next("Name a failure here")
}

fn private(symbol: &Symbol, domain: &str, span: Span) -> Diagnostic {
    let name = &symbol.name;
    Diagnostic::new(
        &rule::VISIBILITY_PRIVATE_CROSS_DOMAIN,
        format!("`{name}` is private to domain `{domain}`"),
    )
    .at(span, "private")
    .reason(format!(
        "Another domain may use only what a domain marks `public`; `{name}` is {} in `{}`.",
        symbol.kind.with_a(),
        symbol.path
    ))
    .next(format!(
        "Mark `{name}` `public` if `{domain}` should offer it to other domains"
    ))
}

fn call_direction(symbol: &Symbol, domain: &str, span: Span) -> Diagnostic {
    let name = &symbol.name;
    Diagnostic::new(
        &rule::LAYOUT_CALL_DIRECTION,
        format!("Shared code uses `{name}` from domain `{domain}`"),
    )
    .at(span, "domain code")
    .reason(format!(
        "`shared/` holds code every domain may use, so it can't depend on any one domain; `{name}` is {} in `{}`.",
        symbol.kind.with_a(),
        symbol.path
    ))
    .next(format!(
        "Move the shared declaration into `{domain}`, or stop it depending on `{name}`"
    ))
}
