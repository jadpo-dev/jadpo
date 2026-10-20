//! Name resolution (area 1): every name in a type position, every called
//! name and every qualifier before a dot is looked up in the project's symbol
//! table, the standard library or the scope of locals, and checked for
//! kind, visibility and direction.
//!
//! [`resolve_file`] resolves a file's declarations: types and their fields,
//! callable signatures, and the parameters of queries, which have no body.
//! Names inside bodies are resolved by each body's own walk in
//! `typed_body` (chunk 2.1), which calls [`Names`] at every node it checks,
//! so a body's name errors, its types and its unused variables come from
//! one walk over one set of scopes. Where that walk doesn't go into a
//! subtree (a block of settings, the fields of a value already reported as
//! built wrongly), it resolves the subtree with a [`Walk`] over the same
//! scopes.
//!
//! A file's resolution reads its own tree and the symbol table, never
//! another file's tree, so editing a body elsewhere does not recompute it.
//! Lower-case names used as values (`total`, `todo.title`) are area 3's to
//! check, with types.

use jadpo_db::{Base, Db, File, ItemKind, Ownership, Project, Symbol, item_tree, location};
use jadpo_diag::{Diagnostic, Span, nearest, rule};
use jadpo_syntax::ast::{self, AstNode, BlockItem, Decl, Expr, MemberItem, Pat, Stmt, Type};
use jadpo_syntax::habits;

use crate::case::{is_lower_snake, starts_upper, to_snake};
use crate::lookups::{Lookup, NameView};
use crate::naming::{name_case, reserved};
use crate::std_lib::{Namespace, StdNames};
use crate::tree::{self, dotted, node_span};

/// The operations every entity has (area 6).
pub(crate) const ENTITY_OPERATIONS: [&str; 4] = ["get", "create", "update", "delete"];

/// What a type position accepts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Want {
    Type,
    Failure,
}

/// Resolution problems of one file's declarations.
#[salsa::tracked(returns(ref))]
pub fn resolve_file(db: &dyn Db, project: Project, file: File) -> Vec<Diagnostic> {
    let Some(source) = tree::source(db, file) else {
        return Vec::new();
    };
    let location = location(db, project, file);
    let lookup = Lookup::for_file(db, project, file);
    let names = Names {
        path: file.path(db),
        ownership: &location.ownership,
        table: &lookup,
        types: &lookup,
        std: StdNames::get(),
        library: None,
    };
    let items = &item_tree(db, file).items;
    let mut out = Vec::new();
    for (index, decl) in source.decls().enumerate() {
        let item = items.iter().find(|item| item.index as usize == index);
        unchecked_bodies(&names, &decl, item, &mut out);
        match &decl {
            Decl::TypeDecl(d) => names.type_decl(d, &mut out),
            Decl::FunctionDecl(d) => names.signature(
                d.param_list(),
                d.fails_clause(),
                d.ret_type(),
                false,
                &mut out,
            ),
            Decl::ActionDecl(d) => names.signature(
                d.param_list(),
                d.fails_clause(),
                d.ret_type(),
                false,
                &mut out,
            ),
            // A query has no body, so its parameters come into scope here.
            Decl::QueryDecl(d) => names.signature(
                d.param_list(),
                d.fails_clause(),
                d.ret_type(),
                true,
                &mut out,
            ),
            Decl::FixtureDecl(d) => {
                names.signature(d.param_list(), None, d.ret_type(), false, &mut out)
            }
            Decl::ConstantDecl(d) => names.ty(d.ty(), Want::Type, &mut out),
            Decl::LibraryDecl(d) => {
                for member in tree::library_members(d) {
                    match member {
                        Decl::FunctionDecl(function) => names.signature(
                            function.param_list(),
                            function.fails_clause(),
                            function.ret_type(),
                            false,
                            &mut out,
                        ),
                        Decl::ConstantDecl(constant) => {
                            names.ty(constant.ty(), Want::Type, &mut out);
                        }
                        _ => {}
                    }
                }
            }
            Decl::EntityDecl(d) => names.entity(d, &mut out),
            // A setting's type is a name like any other (area 10).
            Decl::SingletonDecl(d) if d.keyword().is_some_and(|word| word.text() == "config") => {
                names.settings(d, &mut out)
            }
            _ => {}
        }
    }
    out
}

/// The bodies no typed body is built for, because the item tree has no
/// name for them (`seed { ... }` without one, which is reported): their
/// names are resolved here, as every body's are.
fn unchecked_bodies(
    names: &Names<'_>,
    decl: &Decl,
    item: Option<&jadpo_db::Item>,
    out: &mut Vec<Diagnostic>,
) {
    let outer = |_: &str| None;
    let body =
        |params: Option<ast::ParamList>, block: Option<ast::Block>, out: &mut Vec<Diagnostic>| {
            let mut walk = Walk::new(names, &outer);
            walk.scopes.push(Vec::new());
            for param in params.iter().flat_map(|list| list.params()) {
                walk.declare(param.name(), "parameter");
            }
            walk.block(block);
            out.append(&mut walk.out);
        };
    let checked = item.is_some_and(|item| {
        matches!(
            item.kind,
            ItemKind::Function
                | ItemKind::Constant
                | ItemKind::Action
                | ItemKind::Fixture
                | ItemKind::Test
                | ItemKind::Seed
        )
    });
    match decl {
        Decl::FunctionDecl(d) if !checked => body(d.param_list(), d.block(), out),
        Decl::ActionDecl(d) if !checked => body(d.param_list(), d.block(), out),
        Decl::FixtureDecl(d) if !checked => body(d.param_list(), d.block(), out),
        Decl::TestDecl(d) if !checked => body(None, d.block(), out),
        Decl::SeedDecl(d) if !checked => body(None, d.block(), out),
        // `typed_body` checks a library's first `functions.len()` members.
        Decl::LibraryDecl(d) => {
            let typed = item.map_or(0, |item| item.functions.len());
            for member in tree::library_members(d).into_iter().skip(typed) {
                if let Decl::FunctionDecl(function) = member {
                    body(function.param_list(), function.block(), out);
                }
            }
        }
        _ => {}
    }
}

/// What resolving a name needs: the file, the symbol and type tables read
/// name by name (docs/compiler-runtime.md §2.4), and the standard
/// library. Suggestions on a mistake read the tables whole.
pub(crate) struct Names<'a> {
    pub path: &'a str,
    pub ownership: &'a Ownership,
    pub table: &'a dyn NameView,
    /// The type table: an entity's fields.
    pub types: &'a dyn crate::types::TypeView,
    pub std: &'static StdNames,
    /// The library whose functions are being resolved: they call each
    /// other bare, as helpers in their own file.
    pub library: Option<&'a str>,
}

fn text(name: &ast::NameRef) -> String {
    tree::name_ref_text(name).unwrap_or_default()
}

fn did_you_mean(near: Option<&str>) -> String {
    near.map_or_else(String::new, |near| format!(" Did you mean `{near}`?"))
}

impl Names<'_> {
    fn span(&self, node: &impl AstNode) -> Span {
        node_span(self.path, node.syntax())
    }

    // --- declarations ------------------------------------------------------

    /// A callable's parameter types, `fails` list and result; `params`
    /// says whether its parameters come into scope here, for a query.
    fn signature(
        &self,
        params: Option<ast::ParamList>,
        fails: Option<ast::FailsClause>,
        result: Option<ast::RetType>,
        declare_params: bool,
        out: &mut Vec<Diagnostic>,
    ) {
        let mut scope: Vec<(String, Span)> = Vec::new();
        for param in params.iter().flat_map(|list| list.params()) {
            self.ty(param.ty(), Want::Type, out);
            if declare_params && let Some(name) = param.name() {
                let earlier = tree::name_text(&name).and_then(|text| {
                    scope
                        .iter()
                        .find(|(local, _)| *local == text)
                        .map(|(_, span)| span.clone())
                });
                self.declare(&name, "parameter", earlier, out);
                if let Some(text) = tree::name_text(&name) {
                    scope.push((text, self.span(&name)));
                }
            }
        }
        for ty in fails.iter().flat_map(|clause| clause.types()) {
            self.ty(Some(ty), Want::Failure, out);
        }
        self.ty(result.and_then(|ret| ret.ty()), Want::Type, out);
    }

    fn type_decl(&self, decl: &ast::TypeDecl, out: &mut Vec<Diagnostic>) {
        let base_text = decl
            .base()
            .map(|base| tree::written(base.syntax()))
            .unwrap_or_default();
        self.ty(decl.base(), Want::Type, out);
        let base = Base::from_written(&base_text);
        let list = decl.member_list();
        match base {
            Base::Failure => {
                for (name, member) in tree::members(list) {
                    if matches!(
                        tree::name_text(&name).as_deref(),
                        Some("client" | "internal")
                    ) {
                        self.fields(member.block(), out);
                    }
                }
            }
            Base::Enum => {
                for item in list.iter().flat_map(|list| list.member_items()) {
                    match item {
                        MemberItem::BareMember(variant) => {
                            self.member_name(variant.name(), "variant", out)
                        }
                        MemberItem::Member(variant) => {
                            if let Some(key) = variant.member_key() {
                                self.member_name(key.names().next(), "variant", out);
                            }
                            if let Some(value) = variant.value() {
                                self.type_expr(&value, Want::Type, out);
                            }
                            self.fields(variant.block(), out);
                        }
                        _ => {}
                    }
                }
            }
            base if base.has_fields() => self.fields(list, out),
            _ => {}
        }
    }

    /// An entity's fields, with the names in their settings, and the names
    /// in its lifecycle (areas 5 and 6). Its other sections name its own
    /// fields, which the data rules check.
    fn entity(&self, decl: &ast::EntityDecl, out: &mut Vec<Diagnostic>) {
        let outer = |_: &str| None;
        let mut walk = Walk::new(self, &outer);
        for (name, member) in tree::members(decl.member_list()) {
            match tree::name_text(&name).as_deref() {
                Some("store" | "unique" | "indexes" | "policy" | "caches") | None => {}
                Some("lifecycle") => walk.member_values(member.block()),
                Some(_) => {
                    let Some(value) = member.value() else {
                        continue;
                    };
                    if !crate::types::looks_like_type(&value) {
                        continue;
                    }
                    self.type_expr(&value, Want::Type, &mut walk.out);
                    if let Expr::RecordExpr(record) = &value {
                        walk.member_values(record.member_list());
                    }
                }
            }
        }
        out.append(&mut walk.out);
    }

    /// A block of fields: `name: Type`, and the lines that copy fields
    /// from other types (area 2): `include: [Todo]`, `pick: [Todo.title]`,
    /// `omit: [...]` and `optional: [...]` or `all`.
    fn fields(&self, list: Option<ast::MemberList>, out: &mut Vec<Diagnostic>) {
        for item in list.iter().flat_map(|list| list.member_items()) {
            let MemberItem::Member(field) = item else {
                continue;
            };
            let key = field
                .member_key()
                .and_then(|key| key.names().next())
                .and_then(|name| tree::name_text(&name));
            match key.as_deref() {
                Some("include" | "pick" | "omit") => {
                    if let Some(Expr::ListExpr(list)) = field.value() {
                        for entry in list.exprs() {
                            self.type_expr(&entry, Want::Type, out);
                        }
                    }
                    continue;
                }
                Some("optional") => continue,
                _ => {}
            }
            if let Some(key) = field.member_key() {
                self.member_name(key.names().next(), "field", out);
            }
            if let Some(value) = field.value() {
                self.type_expr(&value, Want::Type, out);
            }
        }
    }

    fn member_name(&self, name: Option<ast::Name>, word: &str, out: &mut Vec<Diagnostic>) {
        let Some(name) = name else { return };
        let Some(text) = tree::name_text(&name) else {
            return;
        };
        if !is_lower_snake(&text) {
            let span = self.span(&name);
            out.push(name_case(
                &text,
                &to_snake(&text),
                word,
                "lower_snake_case",
                span,
            ));
        }
    }

    /// A parameter, variable or binding coming into scope, `earlier` being
    /// where a local of the same name in scope is declared. Whether it is
    /// reported, so an unused-variable error never repeats the mistake.
    pub fn declare(
        &self,
        name: &ast::Name,
        word: &str,
        earlier: Option<Span>,
        out: &mut Vec<Diagnostic>,
    ) -> bool {
        let Some(text) = tree::name_text(name) else {
            return false;
        };
        let span = self.span(name);
        if !is_lower_snake(&text) {
            out.push(name_case(
                &text,
                &to_snake(&text),
                word,
                "lower_snake_case",
                span,
            ));
        } else if self.std.reserved(&text) {
            out.push(reserved(&text, span));
        } else if let Some(earlier) = earlier {
            out.push(
                Diagnostic::new(
                    &rule::SEMANTIC_DUPLICATE_DECLARATION,
                    format!("`{text}` is already declared"),
                )
                .at(span, "declared again")
                .also(earlier, "first declared here")
                .reason("A name in scope can't be declared again, parameters included, so one name means one value wherever it is read.")
                .next(format!("Give this {word} a name of its own")),
            );
        } else if self.table.is_domain(&text) || self.table.library(&text).is_some() {
            let what = if self.table.is_domain(&text) {
                "domain"
            } else {
                "library"
            };
            out.push(
                Diagnostic::new(
                    &rule::SEMANTIC_DUPLICATE_DECLARATION,
                    format!("`{text}` is already a {what}"),
                )
                .at(span, format!("named like a {what}"))
                .reason("A variable can't reuse a domain's or library's name, so `name.call(...)` has one meaning.")
                .next(format!("Rename this {word}")),
            );
        } else {
            return false;
        }
        true
    }

    // --- types -----------------------------------------------------------

    /// The types of the settings `config { }` declares (area 10).
    fn settings(&self, decl: &ast::SingletonDecl, out: &mut Vec<Diagnostic>) {
        for (_, member) in tree::members(decl.member_list()) {
            let Some(value) = member.value() else {
                continue;
            };
            self.type_expr(&value, Want::Type, out);
        }
    }

    pub fn ty(&self, ty: Option<Type>, want: Want, out: &mut Vec<Diagnostic>) {
        match ty {
            Some(Type::PathType(path_type)) => {
                let names: Vec<ast::NameRef> = path_type
                    .path()
                    .into_iter()
                    .flat_map(|path| path.name_refs())
                    .collect();
                let args: Vec<Type> = path_type
                    .generic_arg_list()
                    .into_iter()
                    .flat_map(|list| list.types())
                    .collect();
                // Rust's `Option<Tag>` and Java's `Optional<Tag>`.
                if let ([first], [arg]) = (names.as_slice(), args.as_slice())
                    && self.nullable_habit(first, path_type.syntax(), arg.syntax(), out)
                {
                    self.ty(Some(arg.clone()), Want::Type, out);
                    return;
                }
                if let Some(first) = names.first() {
                    let packaged = names.len() > 1 && self.table.is_package(&text(first));
                    if packaged {
                        if let Some(Through::Package(via, taken)) = self.through(&names, out)
                            && let Some(member) = names.get(taken)
                        {
                            self.package_type(&via, member, want, out);
                        }
                    } else {
                        self.type_name(first, want, out);
                    }
                }
                for arg in path_type
                    .generic_arg_list()
                    .into_iter()
                    .flat_map(|list| list.types())
                {
                    self.ty(Some(arg), Want::Type, out);
                }
            }
            Some(Type::NullableType(nullable)) => self.ty(nullable.ty(), want, out),
            None => {}
        }
    }

    /// A type written as a member's value: `Count`, `Todo.id`, `List<Tag>`,
    /// `Name?`, `ProductName { optional: true }`.
    pub(crate) fn type_expr(&self, expr: &Expr, want: Want, out: &mut Vec<Diagnostic>) {
        match expr {
            Expr::NameRef(name) => self.type_name(name, want, out),
            Expr::FieldExpr(_) => {
                if let Some((first, rest)) = dotted(expr) {
                    if !self.table.is_package(&text(&first)) {
                        self.type_name(&first, want, out);
                    } else {
                        let names: Vec<ast::NameRef> = std::iter::once(first).chain(rest).collect();
                        if let Some(Through::Package(via, taken)) = self.through(&names, out)
                            && let Some(member) = names.get(taken)
                        {
                            self.package_type(&via, member, want, out);
                        }
                    }
                }
            }
            Expr::GenericExpr(generic) => {
                let args: Vec<Type> = generic
                    .generic_arg_list()
                    .into_iter()
                    .flat_map(|list| list.types())
                    .collect();
                if let (Some(Expr::NameRef(first)), [arg]) = (generic.expr(), args.as_slice())
                    && self.nullable_habit(&first, generic.syntax(), arg.syntax(), out)
                {
                    self.ty(Some(arg.clone()), Want::Type, out);
                    return;
                }
                if let Some(inner) = generic.expr() {
                    self.type_expr(&inner, Want::Type, out);
                }
                for arg in generic
                    .generic_arg_list()
                    .into_iter()
                    .flat_map(|list| list.types())
                {
                    self.ty(Some(arg), Want::Type, out);
                }
            }
            Expr::NullableExpr(nullable) => {
                if let Some(inner) = nullable.expr() {
                    self.type_expr(&inner, want, out);
                }
            }
            Expr::RecordExpr(record) => {
                if let Some(inner) = record.expr() {
                    self.type_expr(&inner, want, out);
                }
            }
            _ => {}
        }
    }

    pub fn type_name(&self, name: &ast::NameRef, want: Want, out: &mut Vec<Diagnostic>) {
        let text = text(name);
        let span = self.span(name);
        if self.std.is_type(&text) {
            if want == Want::Failure {
                out.push(not_a_failure(&text, "a built-in type", span));
            }
            return;
        }
        if let Some(symbol) = self.table.get(&text) {
            let fits = match (&symbol.kind, want) {
                (ItemKind::Type(Base::Failure), _) => true,
                (ItemKind::Type(_) | ItemKind::Entity, Want::Type) => true,
                (ItemKind::Type(_) | ItemKind::Entity, Want::Failure) => {
                    let what = symbol.kind.with_a();
                    out.push(not_a_failure(&text, &what, span));
                    return;
                }
                _ => false,
            };
            if fits {
                let symbol = symbol.clone();
                self.access(&symbol, span, out);
            } else {
                let what = symbol.kind.with_a();
                out.push(not_a_type(&text, &what, span));
            }
            return;
        }
        if let Some(what) = self.qualifier_kind(&text) {
            out.push(not_a_type(&text, &format!("a {what}"), span));
            return;
        }
        // `string`, `int`, `Vec`: another language's name for a type Jadpo
        // has (decision register, 2026-10-15).
        if let Some((meaning, jadpo)) = habits::foreign_type(&text) {
            let shown = if jadpo == "?" { "T?" } else { jadpo };
            let mut problem = foreign_type(&text, meaning, jadpo, shown, span.clone());
            if jadpo != "?" {
                problem = problem.edit(span, jadpo);
            }
            out.push(problem);
            return;
        }
        let candidates = self.type_names(want);
        let near = nearest(&text, candidates.iter().map(String::as_str)).map(str::to_string);
        out.push(unknown_name(&text, near.as_deref(), span));
    }

    /// `Option<Tag>` written for `Tag?`: reported, with the edit that
    /// writes `Tag?` over `whole`, when `name` is `Option` or `Optional`
    /// and the project declares no such type.
    fn nullable_habit(
        &self,
        name: &ast::NameRef,
        whole: &jadpo_syntax::SyntaxNode,
        arg: &jadpo_syntax::SyntaxNode,
        out: &mut Vec<Diagnostic>,
    ) -> bool {
        let text = text(name);
        let Some((meaning, "?")) = habits::foreign_type(&text) else {
            return false;
        };
        if self.std.is_type(&text) || self.table.get(&text).is_some() {
            return false;
        }
        let whole_span = node_span(self.path, whole);
        let written = whole.text().to_string();
        let shown = format!("{}?", arg.text());
        out.push(
            foreign_type(&written, meaning, "?", &shown, self.span(name))
                .edit(whole_span, shown.clone()),
        );
        true
    }

    /// Every type name a position could have meant.
    fn type_names(&self, want: Want) -> Vec<String> {
        let mut names: Vec<String> = self
            .table
            .all()
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
            names.extend(self.std.types().map(str::to_string));
        }
        names
    }

    /// What a lower-case name that owns calls is, if it is one.
    fn qualifier_kind(&self, name: &str) -> Option<&'static str> {
        if self.table.is_domain(name) {
            Some("domain")
        } else if self.table.library(name).is_some() {
            Some("library")
        } else if self.std.reserved(name) {
            Some("standard-library namespace")
        } else if self.table.is_connection(name) {
            Some("connection")
        } else {
            None
        }
    }

    /// Checks that this file may use `symbol`: visible from its domain, and
    /// not domain code used from `shared/`.
    fn access(&self, symbol: &Symbol, span: Span, out: &mut Vec<Diagnostic>) {
        if let Ownership::Package(home) = &symbol.ownership {
            if !symbol.visible_from(self.ownership) {
                out.push(package_private(symbol, &home.through, span));
            }
            return;
        }
        if *self.ownership == Ownership::Shared {
            if let Ownership::Domain(domain) = &symbol.ownership {
                out.push(call_direction(symbol, domain, span));
            }
            return;
        }
        // A scenario in the root `tests/` folder sees what another domain
        // would see: only what a domain makes public (area 13, question 1).
        // So does a seed calling a fixture: a private fixture is its own
        // domain's (area 13, decided 2026-10-14).
        let outside = *self.ownership == Ownership::Root
            && (self.path.starts_with("tests/") || symbol.kind == ItemKind::Fixture);
        if (!symbol.visible_from(self.ownership) || outside && !symbol.public)
            && let Ownership::Domain(domain) = &symbol.ownership
        {
            out.push(private(symbol, domain, span));
        }
    }

    // --- failures ----------------------------------------------------------

    /// Where a failure is named (`reject`, a mapping), anything but a
    /// failure's name: an enum's variant such as `FailureKind.rejected`, a
    /// field or any other value (area 4). `reject OutOfStock()` is a call
    /// to a type, reported where the call is resolved: whether it is one.
    pub fn not_a_failure_value(&self, expr: &Expr, out: &mut Vec<Diagnostic>) -> bool {
        if let Expr::CallExpr(call) = expr
            && let Some(Expr::NameRef(name)) = call.expr()
            && starts_upper(&text(&name))
        {
            return true;
        }
        let written = tree::written(expr.syntax());
        let span = self.span(expr);
        out.push(not_a_failure(&written, "a value", span));
        false
    }

    /// The failure a `reject` names, or the head of its block, as written.
    /// Whether the head is a call to a type, which the caller resolves.
    pub fn rejected(&self, expr: &Expr, out: &mut Vec<Diagnostic>) -> bool {
        match expr {
            Expr::NameRef(name) => {
                self.type_name(name, Want::Failure, out);
                false
            }
            Expr::RecordExpr(record) => match record.expr() {
                Some(Expr::NameRef(name)) => {
                    self.type_name(&name, Want::Failure, out);
                    false
                }
                Some(other) => self.not_a_failure_value(&other, out),
                None => false,
            },
            other => self.not_a_failure_value(other, out),
        }
    }

    /// A mapping's failure, `max: QuantityTooLarge`. Whether it is a
    /// call to a type, which the caller resolves.
    pub fn mapped(&self, expr: &Expr, out: &mut Vec<Diagnostic>) -> bool {
        match expr {
            Expr::NameRef(name) => {
                self.type_name(name, Want::Failure, out);
                false
            }
            // A failure with its fields, as `reject` gives them; the
            // fields' values are the caller's to resolve.
            Expr::RecordExpr(record) if let Some(Expr::NameRef(name)) = record.expr() => {
                self.type_name(&name, Want::Failure, out);
                false
            }
            other => self.not_a_failure_value(other, out),
        }
    }

    // --- calls -------------------------------------------------------------

    /// What a call reaches, by its callee: a callable, a constructor, an
    /// entity's operation or the standard library. `is_local` says whether
    /// the callee's first name is a local in scope. The arguments are the
    /// caller's to resolve.
    pub fn callee(&self, call: &ast::CallExpr, is_local: bool, out: &mut Vec<Diagnostic>) {
        match call.expr() {
            Some(Expr::NameRef(name)) => self.bare_call(&name, call, is_local, out),
            Some(callee @ Expr::FieldExpr(_)) => {
                if let Some((first, rest)) = dotted(&callee) {
                    self.dotted_call(&first, &rest, is_local, out);
                }
            }
            _ => {}
        }
    }

    /// `name(...)`: a function, action or query of this domain, or a type
    /// being constructed.
    fn bare_call(
        &self,
        name: &ast::NameRef,
        call: &ast::CallExpr,
        is_local: bool,
        out: &mut Vec<Diagnostic>,
    ) {
        let text = text(name);
        let span = self.span(name);
        if starts_upper(&text) {
            self.constructor(name, call, out);
            return;
        }
        if is_local {
            out.push(not_callable(
                &text,
                "a variable",
                "Jadpo has no function values: only functions, actions, queries and types are called.",
                format!("Use `{text}` as a value, or call a function"),
                span,
            ));
            return;
        }
        let found = self.table.declared(&text);
        if !found.is_empty() {
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
                self.bare_symbol(&text, &symbol, span, out);
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
            out.push(not_callable(
                &text,
                &format!("a {what}"),
                &reason,
                next,
                span,
            ));
            return;
        }
        if let Some(namespace) = self.std.namespace_of(&text) {
            out.push(
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
            .all()
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
        out.push(problem);
    }

    fn bare_symbol(&self, text: &str, symbol: &Symbol, span: Span, out: &mut Vec<Diagnostic>) {
        let other_domain = match (&symbol.ownership, self.ownership) {
            (Ownership::Domain(theirs), Ownership::Domain(ours)) if theirs != ours => {
                Some(theirs.clone())
            }
            (Ownership::Domain(theirs), Ownership::Root) => Some(theirs.clone()),
            _ => None,
        };
        match &symbol.kind {
            ItemKind::Function => match (&symbol.library, other_domain) {
                (Some((library, _)), _)
                    if self.library.map(|own| self.table.canonical(own)).as_ref()
                        != Some(library) =>
                {
                    out.push(
                        Diagnostic::new(
                            &rule::SEMANTIC_UNKNOWN_CALLEE,
                            format!("`{text}` is in library `{library}`"),
                        )
                        .at(span.clone(), "needs its library")
                        .reason("A library's functions are called through it, like the standard library.")
                        .next(format!("Call `{library}.{text}(...)`"))
                        .edit(Span::new(self.path, span.start, span.start), format!("{library}.")),
                    );
                }
                // A sibling in the same library.
                (Some(_), _) => {}
                (None, Some(domain)) => out.push(
                    Diagnostic::new(
                        &rule::SEMANTIC_UNKNOWN_CALLEE,
                        format!("`{text}` is a helper in domain `{domain}`"),
                    )
                    .at(span, "not reachable from here")
                    .reason("A helper function is called from its own domain; another domain uses it through a public library.")
                    .next(format!(
                        "Move `{text}` into a public library in `{domain}` and call it through the library"
                    )),
                ),
                (None, None) => self.access(symbol, span, out),
            },
            // A fixture is called like an action: bare in its own domain,
            // through its domain from another (area 13, decided
            // 2026-10-14).
            ItemKind::Action | ItemKind::Query | ItemKind::Fixture => match other_domain {
                Some(domain) => out.push(
                    Diagnostic::new(
                        &rule::SEMANTIC_UNKNOWN_CALLEE,
                        format!("`{text}` belongs to domain `{domain}`"),
                    )
                    .at(span.clone(), "needs its domain")
                    .reason("Your own domain's actions, queries and fixtures are called bare; another domain's are called through it, so each call says which domain it reaches.")
                    .next(format!("Call `{domain}.{text}(...)`"))
                    .edit(Span::new(self.path, span.start, span.start), format!("{domain}.")),
                ),
                None => self.access(symbol, span, out),
            },
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
                out.push(not_callable(
                    text,
                    &kind.with_a(),
                    reason,
                    next.to_string(),
                    span,
                ));
            }
        }
    }

    /// A call through a package (area 11): `Postcodes.uk.parse(...)`, a
    /// function of its public library, or `Postcodes.UkPostcode(...)`, its
    /// type built from a value.
    fn package_call(&self, first: &ast::NameRef, rest: &[ast::NameRef], out: &mut Vec<Diagnostic>) {
        let names: Vec<ast::NameRef> = std::iter::once(first.clone())
            .chain(rest.iter().cloned())
            .collect();
        let Some(Through::Package(via, taken)) = self.through(&names, out) else {
            return;
        };
        let rest = &names[taken..];
        let Some((member, after)) = rest.split_first() else {
            return;
        };
        // `Postcodes.translations.<message>(...)` is checked as a message's
        // call (area 10).
        if text(member) == crate::translations::TRANSLATIONS {
            return;
        }
        let Some(symbol) = self.package_member(&via, member, out) else {
            return;
        };
        if symbol.kind != ItemKind::Library {
            return;
        }
        let library = format!("{}.{}", via.package, text(member));
        match after {
            [function] => {
                let name = text(function);
                if self.table.library_function(&library, &name).is_none() {
                    let functions: Vec<String> = self
                        .table
                        .all()
                        .declarations
                        .values()
                        .flatten()
                        .filter(|symbol| {
                            symbol
                                .library
                                .as_ref()
                                .is_some_and(|(held_by, _)| *held_by == library)
                        })
                        .map(|symbol| symbol.name.clone())
                        .collect();
                    let near = nearest(&name, functions.iter().map(String::as_str));
                    out.push(no_member(
                        &format!("Library `{library}` has no function `{name}`"),
                        &name,
                        near,
                        self.span(function),
                    ));
                }
            }
            [] => {}
            _ => out.push(self.too_many_names(first, &names[1..])),
        }
    }

    /// `owner.a.b(...)` through anything but a connection: a call names one
    /// owner and what it calls (area 1). A library called through its
    /// domain gets the edit that drops the domain.
    fn too_many_names(&self, first: &ast::NameRef, rest: &[ast::NameRef]) -> Diagnostic {
        let qualifier = text(first);
        let names: Vec<String> = rest.iter().map(text).collect();
        let written = format!("{qualifier}.{}", names.join("."));
        let start = self.span(first).start;
        let end = rest.last().map_or(start, |last| self.span(last).end);
        let span = Span::new(self.path, start, end);
        let reason = "A call names who it belongs to, then what it calls: `domain.action(...)`, `library.function(...)` or `namespace.operation(...)`. Only a connection's operations take three names, `connection.resource.operation(...)` (area 11).";
        let problem = Diagnostic::new(
            &rule::SEMANTIC_UNKNOWN_CALLEE,
            format!("`{written}` has more dots than a call takes"),
        )
        .at(span, "not a call Jadpo makes")
        .reason(reason);
        let library = names
            .first()
            .filter(|name| self.table.library(name).is_some());
        if self.table.is_domain(&qualifier)
            && let (Some(library), [_, function]) = (library, names.as_slice())
        {
            let library_start = self.span(&rest[0]).start;
            return problem
                .reason(format!(
                    "{reason} A library is called by its own name, which is unique in the project, so `{library}` needs no domain."
                ))
                .next(format!("Call `{library}.{function}(...)`"))
                .edit(Span::new(self.path, start, library_start), "");
        }
        let next = if self.table.is_domain(&qualifier) {
            format!("Call one of `{qualifier}`'s actions or queries, `{qualifier}.<action>(...)`")
        } else if self.table.library(&qualifier).is_some() {
            format!("Call one of `{qualifier}`'s functions, `{qualifier}.<function>(...)`")
        } else if self.std.namespace(&qualifier).is_some() {
            format!("Call one of `{qualifier}`'s operations, `{qualifier}.<operation>(...)`")
        } else {
            "Call a function, action or query as `name(...)`, `domain.action(...)` or `library.function(...)`".to_string()
        };
        problem.next(next)
    }

    /// `Type(value)`: constructing a value of a named type.
    fn constructor(&self, name: &ast::NameRef, call: &ast::CallExpr, out: &mut Vec<Diagnostic>) {
        let text = text(name);
        let span = self.span(name);
        if self.std.is_type(&text) {
            return;
        }
        let Some(symbol) = self.table.get(&text).cloned() else {
            if !self.table.is_package(&text) {
                let candidates = self.type_names(Want::Type);
                let near =
                    nearest(&text, candidates.iter().map(String::as_str)).map(str::to_string);
                out.push(unknown_name(&text, near.as_deref(), span));
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
                out.push(problem);
            }
            ItemKind::Entity => out.push(not_callable(
                &text,
                "an entity",
                &format!("An entity is read and written through its operations, such as `{text}.get(...)`."),
                format!("Use one of `{text}`'s operations"),
                span,
            )),
            ItemKind::Routes => out.push(not_callable(
                &text,
                "a route group",
                "Routes answer requests; code calls the action a route runs.",
                "Call the action instead".to_string(),
                span,
            )),
            _ => self.access(&symbol, span, out),
        }
    }

    /// `Entity.operation(...)`: an entity's operations are `get`,
    /// `create`, `update`, `delete` and one per transition its lifecycle
    /// names (area 6).
    fn entity_operation(&self, entity: &str, operation: &ast::NameRef, out: &mut Vec<Diagnostic>) {
        let Some(symbol) = self
            .table
            .declared(entity)
            .iter()
            .find(|symbol| symbol.kind == ItemKind::Entity)
            .cloned()
        else {
            return;
        };
        let name = text(operation);
        // `Todo.title(value)` builds the field's type.
        if self
            .types
            .fields(entity)
            .is_some_and(|fields| fields.iter().any(|field| field.name == name))
        {
            return;
        }
        // The service entity's key operations are checked with the body,
        // which knows the principal (area 8).
        if jadpo_std::http::SessionOp::key_op(&name).is_some() {
            return;
        }
        let mut operations: Vec<String> =
            ENTITY_OPERATIONS.iter().map(|op| op.to_string()).collect();
        operations.extend(symbol.transitions.iter().cloned());
        if operations.contains(&name) {
            return;
        }
        let span = self.span(operation);
        let near = nearest(&name, operations.iter().map(String::as_str)).map(str::to_string);
        let listed: Vec<String> = operations.iter().map(|op| format!("`{op}`")).collect();
        let problem = Diagnostic::new(
            &rule::SEMANTIC_UNKNOWN_CALLEE,
            format!("`{entity}` has no operation `{name}`"),
        )
        .at(span.clone(), "not an operation")
        .reason(format!(
            "An entity's operations are {}, with one more for each transition its lifecycle names.{}",
            crate::and_list(&listed),
            did_you_mean(near.as_deref())
        ));
        out.push(match near {
            Some(near) => problem
                .next(format!("Use `{entity}.{near}`"))
                .edit(span, near),
            None => problem.next(format!("Call one of `{entity}`'s operations")),
        });
    }

    /// `Name.member` or `Name.member(...)` with an upper-case qualifier: a
    /// type, an entity or a package.
    pub fn type_qualifier(&self, name: &ast::NameRef, calls: bool, out: &mut Vec<Diagnostic>) {
        let text = text(name);
        let span = self.span(name);
        if self.std.is_type(&text) || self.table.is_package(&text) {
            return;
        }
        let Some(symbol) = self.table.get(&text).cloned() else {
            let candidates = self.type_names(Want::Type);
            let near = nearest(&text, candidates.iter().map(String::as_str)).map(str::to_string);
            out.push(unknown_name(&text, near.as_deref(), span));
            return;
        };
        if calls
            && symbol.kind == ItemKind::Entity
            && let (Ownership::Domain(theirs), Ownership::Domain(ours)) =
                (&symbol.ownership, self.ownership)
            && theirs != ours
        {
            out.push(
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
        self.access(&symbol, span, out);
    }

    /// A value, a pattern or an assignment's target written as a name or a
    /// dotted name: an upper-case first name is a type, an enum or an
    /// entity, resolved as a qualifier; a lower-case one is a local, area
    /// 3's to check.
    pub fn value_name(&self, expr: &Expr, out: &mut Vec<Diagnostic>) {
        if let Some((first, rest)) = dotted(expr)
            && starts_upper(&text(&first))
        {
            if self.table.is_package(&text(&first)) && !rest.is_empty() {
                let names: Vec<ast::NameRef> = std::iter::once(first).chain(rest).collect();
                if let Some(Through::Package(via, taken)) = self.through(&names, out)
                    && let Some(member) = names.get(taken)
                {
                    self.package_member(&via, member, out);
                }
                return;
            }
            self.type_qualifier(&first, false, out);
        }
    }

    /// How a dotted name whose first name is a package's reaches it (area
    /// 11, "Where packages live"; chunk 7.1c2): through `Postcodes` while
    /// the project holds one version, through `Postcodes.v3` while it holds
    /// two side by side. A version missing while two are held, or written
    /// while one is, is reported here; one written needlessly is still
    /// followed, so the rest of the name is checked once.
    fn through(&self, names: &[ast::NameRef], out: &mut Vec<Diagnostic>) -> Option<Through> {
        let first = names.first()?;
        let code = text(first);
        if !self.table.is_package(&code) {
            return None;
        }
        let start = self.span(first).start;
        let second = names.get(1).map(text);
        let member = |at: usize| names.get(at).map(text);
        let versions = self.table.versions(&code);
        if !versions.is_empty() {
            if let Some(version) = &second {
                let via = format!("{code}.{version}");
                if versions.contains(&via) {
                    return Some(Through::Package(
                        Via {
                            package: via,
                            start,
                        },
                        2,
                    ));
                }
            }
            out.push(version_needed(
                &code,
                &versions,
                second.as_deref(),
                self.span(first),
            ));
            return Some(Through::Reported);
        }
        let via = Via {
            package: code.clone(),
            start,
        };
        if let Some(version) = &second
            && names.len() > 2
            && jadpo_db::is_version_label(version)
            && self.table.declared(&format!("{code}.{version}")).is_empty()
        {
            let at = self.span(&names[1]);
            let dot = Span::new(self.path, self.span(first).end, at.end);
            out.push(version_unneeded(&code, member(2).as_deref(), dot));
            return Some(Through::Package(via, 2));
        }
        Some(Through::Package(via, 1))
    }

    /// A name written through a package, `Postcodes.UkPostcode` or
    /// `Postcodes.uk` (area 11): the package's declaration, reported when
    /// it has none by that name or doesn't make it public.
    fn package_member(
        &self,
        via: &Via,
        member: &ast::NameRef,
        out: &mut Vec<Diagnostic>,
    ) -> Option<Symbol> {
        let package = via.package.clone();
        let name = text(member);
        let joined = format!("{package}.{name}");
        let found = self
            .table
            .declared(&joined)
            .iter()
            .find(|symbol| symbol.library.is_none() && symbol.ownership.package().is_some())
            .cloned();
        let Some(symbol) = found else {
            let offered: Vec<String> = self
                .table
                .all()
                .declarations
                .values()
                .flatten()
                .filter(|symbol| {
                    symbol.library.is_none()
                        && symbol.public
                        && symbol
                            .ownership
                            .package()
                            .is_some_and(|home| home.through == package)
                })
                .map(|symbol| symbol.name.clone())
                .collect();
            let near = nearest(&name, offered.iter().map(String::as_str)).map(str::to_string);
            let span = self.span(member);
            let problem = Diagnostic::new(
                &rule::SEMANTIC_UNKNOWN_NAME,
                format!("Package `{package}` has no `{name}`"),
            )
            .at(span.clone(), "not in the package")
            .reason(format!(
                "Nothing `{package}` declares is called `{name}`.{}",
                did_you_mean(near.as_deref())
            ));
            out.push(match near {
                Some(near) => problem
                    .next(format!("Use `{package}.{near}` instead"))
                    .edit(span, near),
                None => problem.next(format!("Use something `{package}` marks `public`")),
            });
            return None;
        };
        let span = Span::new(self.path, via.start, self.span(member).end);
        self.access(&symbol, span, out);
        Some(symbol)
    }

    /// A type written through a package, `Postcodes.UkPostcode`, in a
    /// position that wants a type or a failure.
    fn package_type(
        &self,
        via: &Via,
        member: &ast::NameRef,
        want: Want,
        out: &mut Vec<Diagnostic>,
    ) {
        let Some(symbol) = self.package_member(via, member, out) else {
            return;
        };
        let written = format!("{}.{}", via.package, text(member));
        let span = Span::new(self.path, via.start, self.span(member).end);
        match (&symbol.kind, want) {
            (ItemKind::Type(Base::Failure), _) => {}
            (ItemKind::Type(_), Want::Type) => {}
            (ItemKind::Type(_), Want::Failure) => {
                out.push(not_a_failure(&written, &symbol.kind.with_a(), span));
            }
            (kind, _) => out.push(not_a_type(&written, &kind.with_a(), span)),
        }
    }

    /// `qualifier.member(...)` with a lower-case qualifier.
    fn dotted_call(
        &self,
        first: &ast::NameRef,
        rest: &[ast::NameRef],
        is_local: bool,
        out: &mut Vec<Diagnostic>,
    ) {
        let qualifier = text(first);
        if starts_upper(&qualifier) && self.table.is_package(&qualifier) && !is_local {
            self.package_call(first, rest, out);
            return;
        }
        if starts_upper(&qualifier) {
            self.type_qualifier(first, true, out);
            if let [operation] = rest {
                self.entity_operation(&qualifier, operation, out);
            }
            return;
        }
        if is_local {
            return;
        }
        let qualifier_span = self.span(first);
        // Only a connection's operations take three names (area 11), and
        // the checker says those aren't built yet.
        // A message is `translations.<domain>.<message>` (area 10).
        let message = qualifier == "translations"
            && self.std.namespace(&qualifier) == Some(Namespace::Generated);
        if rest.len() > 1 && !message && !self.table.is_connection(&qualifier) {
            out.push(self.too_many_names(first, rest));
            return;
        }
        let member = match rest {
            [only] => Some((text(only), self.span(only))),
            _ => None,
        };
        match self.std.namespace(&qualifier) {
            Some(Namespace::Standard) => {
                // A namespace that moved whole (area 19): where a call
                // through it moves to depends on its first argument's type,
                // so the type checker reports it.
                if jadpo_std::moved_namespace(&qualifier).is_some() {
                    return;
                }
                if let Some((member, span)) = &member
                    && let Some(to) = jadpo_std::moved(&qualifier, member)
                {
                    // An operation that moved (area 19): its edit writes
                    // the new name.
                    let whole =
                        Span::new(qualifier_span.path.clone(), qualifier_span.start, span.end);
                    let from = format!("{qualifier}.{member}");
                    out.push(
                        Diagnostic::new(
                            &rule::SEMANTIC_STANDARD_OPERATION_MOVED,
                            format!("`{from}` is now `{to}`"),
                        )
                        .at(whole.clone(), "moved")
                        .reason(format!(
                            "The standard library has one name for each operation; `{from}` moved to `{to}`, which takes the same values."
                        ))
                        .next(format!("Write `{to}`"))
                        .edit(whole, to),
                    );
                    return;
                }
                if let Some((member, span)) = member
                    && !self.std.has_operation(&qualifier, &member)
                {
                    let operations = self.std.operations(&qualifier);
                    let near = nearest(&member, operations.iter().copied());
                    out.push(no_member(
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
            self.access(&library, qualifier_span, out);
            if let Some((member, span)) = member
                && self.table.library_function(&qualifier, &member).is_none()
            {
                let functions: Vec<String> = self
                    .table
                    .all()
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
                out.push(no_member(
                    &format!("Library `{qualifier}` has no function `{member}`"),
                    &member,
                    near,
                    span,
                ));
            }
            return;
        }
        if self.table.is_domain(&qualifier) {
            let Some((member, span)) = member else {
                return;
            };
            if *self.ownership == Ownership::Domain(qualifier.clone()) {
                let end = span.start;
                out.push(
                    Diagnostic::new(
                        &rule::SEMANTIC_OWN_DOMAIN_QUALIFICATION,
                        format!("`{member}` is in this file's own domain"),
                    )
                    .at(qualifier_span.clone(), "own domain")
                    .reason(format!(
                        "The dot names who owns what you call, and `{member}` belongs to `{qualifier}` itself, so code there calls it by its bare name."
                    ))
                    .next(format!("Call `{member}(...)` without `{qualifier}.`"))
                    .edit(Span::new(self.path, qualifier_span.start, end), ""),
                );
                return;
            }
            let in_domain =
                |symbol: &&Symbol| symbol.ownership == Ownership::Domain(qualifier.clone());
            // A domain offers its actions and queries, and its fixtures to
            // tests, fixtures and seeds; who may call a fixture is the
            // typed body's to check (area 13).
            let callable = |symbol: &&Symbol| {
                matches!(
                    symbol.kind,
                    ItemKind::Action | ItemKind::Query | ItemKind::Fixture
                )
            };
            let found = self
                .table
                .declared(&member)
                .iter()
                .filter(in_domain)
                .find(callable)
                .cloned();
            match found {
                Some(symbol) => self.access(&symbol, span, out),
                None => {
                    let operations: Vec<String> = self
                        .table
                        .all()
                        .declarations
                        .values()
                        .flatten()
                        .filter(in_domain)
                        .filter(|symbol| matches!(symbol.kind, ItemKind::Action | ItemKind::Query))
                        .map(|symbol| symbol.name.clone())
                        .collect();
                    let near = nearest(&member, operations.iter().map(String::as_str));
                    out.push(no_member(
                        &format!("Domain `{qualifier}` has no action or query `{member}`"),
                        &member,
                        near,
                        span,
                    ));
                }
            }
            return;
        }
        if self.table.is_connection(&qualifier)
            || self
                .table
                .declared(&qualifier)
                .iter()
                .any(|symbol| symbol.kind == ItemKind::Schedule)
        {
            return;
        }
        let all = self.table.all();
        let mut qualifiers: Vec<String> = all.domains.iter().cloned().collect();
        qualifiers.extend(all.connections.iter().cloned());
        qualifiers.extend(
            self.table
                .all()
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
        out.push(problem);
    }
}

/// Resolution of a subtree the body's walk doesn't go into: every name in
/// it, with the locals in scope around it and the ones it declares itself.
pub(crate) struct Walk<'a> {
    names: &'a Names<'a>,
    /// Where a local in scope around the subtree is declared, outermost
    /// first.
    outer: &'a dyn Fn(&str) -> Option<Span>,
    /// Locals the subtree declares: a `match` arm's binding, an inline
    /// function's parameter, a variable in a nested block.
    scopes: Vec<Vec<(String, Span)>>,
    pub out: Vec<Diagnostic>,
}

impl<'a> Walk<'a> {
    pub fn new(names: &'a Names<'a>, outer: &'a dyn Fn(&str) -> Option<Span>) -> Walk<'a> {
        Walk {
            names,
            outer,
            scopes: Vec::new(),
            out: Vec::new(),
        }
    }

    fn declared_at(&self, name: &str) -> Option<Span> {
        (self.outer)(name).or_else(|| {
            self.scopes
                .iter()
                .flatten()
                .find(|(local, _)| local == name)
                .map(|(_, span)| span.clone())
        })
    }

    fn is_local(&self, name: &str) -> bool {
        self.declared_at(name).is_some()
    }

    fn declare(&mut self, name: Option<ast::Name>, word: &str) {
        let Some(name) = name else { return };
        let Some(text) = tree::name_text(&name) else {
            return;
        };
        let earlier = self.declared_at(&text);
        self.names.declare(&name, word, earlier, &mut self.out);
        let span = self.names.span(&name);
        if let Some(scope) = self.scopes.last_mut() {
            scope.push((text, span));
        } else {
            self.scopes.push(vec![(text, span)]);
        }
    }

    pub fn block(&mut self, block: Option<ast::Block>) {
        let Some(block) = block else { return };
        self.scopes.push(Vec::new());
        for item in block.block_items() {
            match item {
                BlockItem::Stmt(stmt) => self.stmt(&stmt),
                BlockItem::Member(member) => self.member(&member),
                BlockItem::Elision(_) => {}
            }
        }
        self.scopes.pop();
    }

    /// A member of a block: its value and any block of its own.
    pub fn member(&mut self, member: &ast::Member) {
        if let Some(value) = member.value() {
            self.expr(&value);
        }
        self.member_values(member.block());
    }

    fn stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::VarStmt(var) => {
                self.names.ty(var.ty(), Want::Type, &mut self.out);
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
            Stmt::IfStmt(branch) => {
                if let Some(condition) = branch.condition().and_then(|c| c.expr()) {
                    self.expr(&condition);
                }
                self.block(branch.block());
                for next in branch.else_ifs() {
                    if let Some(condition) = next.condition().and_then(|c| c.expr()) {
                        self.expr(&condition);
                    }
                    self.block(next.block());
                }
                if let Some(otherwise) = branch.else_branch() {
                    self.block(otherwise.block());
                }
            }
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
                self.names.ty(
                    assert.assert_fails().and_then(|fails| fails.ty()),
                    Want::Failure,
                    &mut self.out,
                );
            }
            Stmt::ExprStmt(expr) => {
                if let Some(expr) = expr.expr() {
                    self.expr(&expr);
                }
            }
        }
    }

    /// The values of a block's members, and of the blocks inside them.
    pub fn member_values(&mut self, list: Option<ast::MemberList>) {
        for item in list.iter().flat_map(|list| list.member_items()) {
            if let MemberItem::Member(member) = item {
                self.member(&member);
            }
        }
    }

    /// A write's block (area 6): its sections' values, and each
    /// `conflict:` arm's failure.
    pub fn write_block(&mut self, list: Option<ast::MemberList>) {
        for (name, member) in crate::tree::members(list) {
            if crate::tree::name_text(&name).as_deref() != Some("conflict")
                || member.block().is_none()
            {
                self.member(&member);
                continue;
            }
            for (_, arm) in crate::tree::members(member.block()) {
                if let Some(value) = arm.value() {
                    if self.names.mapped(&value, &mut self.out) {
                        self.expr(&value);
                    } else if let Expr::RecordExpr(record) = &value {
                        self.member_values(record.member_list());
                    }
                }
            }
        }
    }

    /// Every expression directly inside `expr`.
    pub fn children(&mut self, expr: &Expr) {
        for child in expr.syntax().children().filter_map(Expr::cast) {
            self.expr(&child);
        }
    }

    pub fn expr(&mut self, expr: &Expr) {
        match expr {
            Expr::CallExpr(call) => {
                let first = match call.expr() {
                    Some(Expr::NameRef(name)) => Some(text(&name)),
                    Some(callee @ Expr::FieldExpr(_)) => {
                        dotted(&callee).map(|(first, _)| text(&first))
                    }
                    _ => None,
                };
                let local = first.is_some_and(|first| self.is_local(&first));
                self.names.callee(call, local, &mut self.out);
                match call.expr() {
                    Some(callee @ Expr::FieldExpr(_)) if dotted(&callee).is_none() => {
                        self.children(&callee)
                    }
                    Some(Expr::NameRef(_) | Expr::FieldExpr(_)) | None => {}
                    Some(other) => self.expr(&other),
                }
                for arg in call.arg_list().iter().flat_map(|list| list.args()) {
                    if let Some(value) = arg.expr() {
                        self.expr(&value);
                    }
                }
            }
            Expr::RejectExpr(reject) => {
                if let Some(inner) = reject.expr() {
                    let call = self.names.rejected(&inner, &mut self.out);
                    let head = match &inner {
                        Expr::RecordExpr(record) => record.expr(),
                        other => Some(other.clone()),
                    };
                    if call && let Some(head) = head {
                        self.expr(&head);
                    }
                    if let Expr::RecordExpr(record) = &inner {
                        self.member_values(record.member_list());
                    }
                }
            }
            Expr::AttemptExpr(attempt) => {
                if let Some(inner) = attempt.expr() {
                    self.expr(&inner);
                }
                if matches!(attempt.expr(), Some(Expr::CallExpr(call)) if is_write(&call)) {
                    self.write_block(attempt.member_list());
                    return;
                }
                for (_, mapping) in crate::tree::members(attempt.member_list()) {
                    if let Some(value) = mapping.value() {
                        if self.names.mapped(&value, &mut self.out) {
                            self.expr(&value);
                        } else if let Expr::RecordExpr(record) = &value {
                            self.member_values(record.member_list());
                        }
                    }
                }
            }
            Expr::RecordExpr(record) => {
                self.record_head(record);
                let write = match record.expr() {
                    Some(Expr::CallExpr(call)) => is_write(&call),
                    Some(head) => dotted(&head).is_some_and(|(first, rest)| {
                        starts_upper(&text(&first)) && rest.len() == 1 && text(&rest[0]) == "create"
                    }),
                    None => false,
                };
                if write {
                    self.write_block(record.member_list());
                } else {
                    self.member_values(record.member_list());
                }
            }
            Expr::FieldExpr(_) | Expr::NameRef(_) => {
                if dotted(expr).is_some() {
                    self.names.value_name(expr, &mut self.out);
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
                        pattern(self.names, &case, &mut self.out);
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
                for name in lambda.names() {
                    self.declare(Some(name), "parameter");
                }
                if let Some(body) = lambda.expr() {
                    self.expr(&body);
                }
                self.scopes.pop();
            }
            _ => self.children(expr),
        }
    }

    /// The type or expression a block is built from.
    fn record_head(&mut self, record: &ast::RecordExpr) {
        match record.expr() {
            Some(Expr::NameRef(name)) if starts_upper(&text(&name)) => {
                self.names.type_name(&name, Want::Type, &mut self.out)
            }
            Some(other) => self.expr(&other),
            None => {}
        }
    }
}

/// Whether a call is an entity's `update` or `delete`, as written:
/// `Todo.update(...)`, whose block is the write's (area 6).
fn is_write(call: &ast::CallExpr) -> bool {
    call.expr()
        .as_ref()
        .and_then(dotted)
        .is_some_and(|(first, rest)| {
            starts_upper(&text(&first))
                && rest.len() == 1
                && matches!(text(&rest[0]).as_str(), "update" | "delete")
        })
}

/// A `match` arm's case pattern: `Enum.variant` names a type, `some(...)`
/// and `success(...)` don't.
pub(crate) fn pattern(names: &Names<'_>, case: &ast::CasePat, out: &mut Vec<Diagnostic>) {
    if let Some(first) = case.path().and_then(|path| path.name_refs().next())
        && starts_upper(&text(&first))
    {
        names.type_qualifier(&first, false, out);
    }
}

/// The head of a block-built value: its type, an enum's variant, or an
/// expression such as an entity write's call. `None` when the body's walk
/// resolves it as an expression of its own.
pub(crate) fn record_head(names: &Names<'_>, record: &ast::RecordExpr, out: &mut Vec<Diagnostic>) {
    match record.expr() {
        Some(Expr::NameRef(name)) if starts_upper(&text(&name)) => {
            names.type_name(&name, Want::Type, out)
        }
        Some(head @ (Expr::NameRef(_) | Expr::FieldExpr(_))) => names.value_name(&head, out),
        _ => {}
    }
}

/// `type.foreign_type`: `written` names a type the way another language
/// does, `meaning` says where, and Jadpo writes `shown` (decision register,
/// 2026-10-15). The caller adds the edit.
fn foreign_type(written: &str, meaning: &str, jadpo: &str, shown: &str, span: Span) -> Diagnostic {
    let habit = &habits::FOREIGN_TYPE;
    Diagnostic::new(habit.rule, habit.message(written, shown))
        .at(span, habit.label_for(written, shown))
        .reason(habits::foreign_reason(written, meaning, jadpo, "calls it"))
        .next(habit.next_for(written, shown))
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
    .next(if symbol.kind == ItemKind::Fixture {
        format!("Mark `{name}` `public` if `{domain}` should offer it to other domains' tests")
    } else {
        format!("Mark `{name}` `public` if `{domain}` should offer it to other domains")
    })
}

fn package_private(symbol: &Symbol, package: &str, span: Span) -> Diagnostic {
    let name = &symbol.name;
    Diagnostic::new(
        &rule::PACKAGE_NOT_PUBLIC,
        format!("`{package}.{name}` isn't public"),
    )
    .at(span, "not part of the package's API")
    .reason(format!(
        "A package offers only what it marks `public`; `{name}` is {} in `{}` that only the package's own code uses, and may change in any version.",
        symbol.kind.with_a(),
        symbol.path
    ))
    .next(format!(
        "Use what `{package}` marks `public` instead"
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

/// How a dotted name reaches a package ([`Resolver::through`]).
enum Through {
    /// Through this package, after this many of the names.
    Package(Via, usize),
    /// A version missing, already reported.
    Reported,
}

/// A package as a name is written through it: `Postcodes`, or
/// `Postcodes.v3` side by side, starting at `start`.
struct Via {
    package: String,
    start: u32,
}

/// `package.version_needed`: a name through a package held in two versions
/// that doesn't say which (chunk 7.1c2).
fn version_needed(code: &str, versions: &[String], member: Option<&str>, span: Span) -> Diagnostic {
    let held: Vec<String> = versions.iter().map(|via| format!("`{via}`")).collect();
    let written: Vec<String> = versions
        .iter()
        .map(|via| format!("`{via}.{}`", member.unwrap_or("<name>")))
        .collect();
    Diagnostic::new(
        &rule::PACKAGE_VERSION_NEEDED,
        format!("`{code}` is held in two versions, so a name through it says which"),
    )
    .at(span, "which version?")
    .reason(format!(
        "The project holds {} side by side while callers move across, so a name written through the package carries its version, and each use says which one it means.",
        crate::and_list(&held)
    ))
    .next(format!("Write {}", crate::or_list(&written)))
}

/// `package.version_unneeded`: a version written through a package the
/// project holds in one version (chunk 7.1c2).
fn version_unneeded(code: &str, member: Option<&str>, span: Span) -> Diagnostic {
    let bare = format!("{code}.{}", member.unwrap_or("<name>"));
    Diagnostic::new(
        &rule::PACKAGE_VERSION_UNNEEDED,
        format!("`{code}` is held in one version, so its names don't carry it"),
    )
    .at(span.clone(), "not needed")
    .reason(format!(
        "A name carries its package's version only while two versions sit side by side, to say which one it means; with one version of `{code}` held, `{bare}` is the one way to write it."
    ))
    .next(format!("Write `{bare}`"))
    .edit(span, "")
}
