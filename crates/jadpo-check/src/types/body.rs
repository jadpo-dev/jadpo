//! Area 2's rules inside bodies: every value's type, checked
//! bidirectionally. The type a position expects (a variable's declared
//! type, a return, an argument, a field) flows into literals; any other
//! value must widen to it. Building a value checks its constructor, its
//! fields or its variant; reading a field checks the value has it.
//!
//! What logic adds (area 3: operand rules, `match`, unknown values, argument
//! counts) and failures add (area 4) is left to those areas: where this
//! check cannot know a type yet, it uses `Error`, which accepts everything,
//! so nothing here reports a knock-on of something it doesn't check.

use std::collections::BTreeMap;

use jadpo_db::{Db, File, ItemKind, Project, Symbols, symbols};
use jadpo_diag::{Diagnostic, Span, nearest, rule};
use jadpo_syntax::SyntaxKind;
use jadpo_syntax::ast::{self, AstNode, BlockItem, Decl, Expr, MemberItem, Stmt};

use super::signatures::{Callables, Sig, callables};
use super::decl::Decls;
use super::literal;
use super::rules::Const;
use super::table::{Def, FieldDef, TypeTable, type_table};
use super::ty::{Prim, Ty, TypeNode, TypeRef};
use crate::case::starts_upper;
use crate::std_lib::{Namespace, Operation, StdNames};
use crate::tree::{self, dotted, node_span};

/// Area 2's problems inside one file's bodies.
#[salsa::tracked(returns(ref))]
pub fn body_problems(db: &dyn Db, project: Project, file: File) -> Vec<Diagnostic> {
    let Some(source) = tree::source(db, file) else {
        return Vec::new();
    };
    let mut checker = Body {
        path: file.path(db),
        table: type_table(db, project),
        calls: callables(db, project),
        symbols: symbols(db, project),
        scopes: Vec::new(),
        result: Ty::Prim(Prim::Unit),
        library: None,
        out: Vec::new(),
    };
    for decl in source.decls() {
        checker.decl(&decl);
    }
    checker.out
}

struct Body<'a> {
    path: &'a str,
    table: &'a TypeTable,
    calls: &'a Callables,
    symbols: &'a Symbols,
    scopes: Vec<BTreeMap<String, Ty>>,
    /// The result type of the callable being checked.
    result: Ty,
    /// The library whose functions are being checked: they call each other
    /// bare.
    library: Option<String>,
    out: Vec<Diagnostic>,
}

fn text(name: &ast::NameRef) -> String {
    tree::name_ref_text(name).unwrap_or_default()
}

fn did_you_mean(near: Option<&str>) -> String {
    near.map_or_else(String::new, |near| format!(" Did you mean `{near}`?"))
}

/// Whether an expression is a literal, which takes the type it goes into.
fn is_literal(expr: &Expr) -> bool {
    match expr {
        Expr::Literal(_) => true,
        Expr::PrefixExpr(prefix) => {
            prefix.syntax().first_token().map(|t| t.kind()) == Some(SyntaxKind::MINUS)
                && matches!(prefix.expr(), Some(Expr::Literal(_)))
        }
        Expr::ParenExpr(paren) => paren.expr().is_some_and(|inner| is_literal(&inner)),
        _ => false,
    }
}

impl Body<'_> {
    fn span(&self, node: &impl AstNode) -> Span {
        node_span(self.path, node.syntax())
    }

    fn resolve(&self, ty: Option<ast::Type>) -> Ty {
        ty.map_or(Ty::Error, |ty| self.table.resolve(&TypeRef::from_type(&ty)))
    }

    fn decl(&mut self, decl: &Decl) {
        match decl {
            Decl::FunctionDecl(d) => self.callable(d.param_list(), d.ret_type(), d.block()),
            Decl::ActionDecl(d) => self.callable(d.param_list(), d.ret_type(), d.block()),
            Decl::FixtureDecl(d) => self.callable(d.param_list(), d.ret_type(), d.block()),
            Decl::LibraryDecl(d) => {
                self.library = d.name().as_ref().and_then(tree::name_text);
                for function in tree::library_functions(d) {
                    self.callable(function.param_list(), function.ret_type(), function.block());
                }
                self.library = None;
            }
            Decl::TestDecl(d) => self.callable(None, None, d.block()),
            Decl::SeedDecl(d) => self.callable(None, None, d.block()),
            _ => {}
        }
    }

    fn callable(
        &mut self,
        params: Option<ast::ParamList>,
        result: Option<ast::RetType>,
        body: Option<ast::Block>,
    ) {
        let mut scope = BTreeMap::new();
        for param in params.iter().flat_map(|list| list.params()) {
            if let Some(name) = param.name().as_ref().and_then(tree::name_text) {
                scope.insert(name, self.resolve(param.ty()));
            }
        }
        self.result = match result {
            Some(ret) => self.resolve(ret.ty()),
            None => Ty::Prim(Prim::Unit),
        };
        self.scopes.push(scope);
        self.block(body);
        self.scopes.pop();
    }

    fn local(&self, name: &str) -> Option<Ty> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).cloned())
    }

    fn bind(&mut self, name: Option<ast::Name>, ty: Ty) {
        if let Some(name) = name.as_ref().and_then(tree::name_text)
            && let Some(scope) = self.scopes.last_mut()
        {
            scope.insert(name, ty);
        }
    }

    // --- statements -------------------------------------------------------

    fn block(&mut self, block: Option<ast::Block>) {
        let Some(block) = block else { return };
        self.scopes.push(BTreeMap::new());
        for item in block.block_items() {
            if let BlockItem::Stmt(stmt) = item {
                self.stmt(&stmt);
            }
        }
        self.scopes.pop();
    }

    fn stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::VarStmt(var) => {
                let declared = var.ty().map(|ty| {
                    let mut decls = Decls {
                        path: self.path,
                        table: self.table,
                        out: Vec::new(),
                    };
                    let ok = decls.type_node(&TypeNode::Type(ty.clone()), false);
                    self.out.append(&mut decls.out);
                    if ok {
                        self.table.resolve(&TypeRef::from_type(&ty))
                    } else {
                        Ty::Error
                    }
                });
                let ty = match (declared, var.expr()) {
                    (Some(declared), Some(value)) => {
                        self.check(&value, &declared);
                        declared
                    }
                    (Some(declared), None) => declared,
                    (None, Some(value)) => self.infer(&value),
                    (None, None) => Ty::Error,
                };
                self.bind(var.name(), ty);
            }
            Stmt::AssignStmt(assign) => {
                let target = match assign.target() {
                    Some(Expr::NameRef(name)) => self.local(&text(&name)),
                    _ => None,
                };
                match (target, assign.value()) {
                    (Some(target), Some(value)) => self.check(&value, &target),
                    (None, Some(value)) => {
                        self.infer(&value);
                    }
                    _ => {}
                }
            }
            Stmt::ReturnStmt(ret) => {
                if let Some(value) = ret.expr() {
                    let expected = self.result.clone();
                    self.check(&value, &expected);
                }
            }
            Stmt::IfStmt(branch) => self.if_stmt(branch),
            Stmt::ForStmt(each) => {
                let items = each.expr().map_or(Ty::Error, |expr| self.infer(&expr));
                let item = match items.strip_nullable() {
                    Ty::List(item) => (**item).clone(),
                    _ => Ty::Error,
                };
                self.scopes.push(BTreeMap::new());
                self.bind(each.name(), item);
                self.block(each.block());
                self.scopes.pop();
            }
            Stmt::EmitStmt(emit) => {
                if let Some(expr) = emit.expr() {
                    self.infer(&expr);
                }
            }
            Stmt::AssertStmt(assert) => {
                if let Some(expr) = assert.expr() {
                    self.infer(&expr);
                }
            }
            Stmt::ExprStmt(expr) => {
                if let Some(expr) = expr.expr() {
                    self.infer(&expr);
                }
            }
        }
    }

    fn if_stmt(&mut self, branch: &ast::IfStmt) {
        if let Some(condition) = branch.condition().and_then(|c| c.expr()) {
            self.infer(&condition);
        }
        self.block(branch.block());
        if let Some(otherwise) = branch.else_branch() {
            if let Some(next) = otherwise.if_stmt() {
                self.if_stmt(&next);
            }
            self.block(otherwise.block());
        }
    }

    // --- checking against an expected type -------------------------------

    /// Checks `expr` where `expected` is required.
    fn check(&mut self, expr: &Expr, expected: &Ty) {
        if let Expr::ParenExpr(paren) = expr {
            if let Some(inner) = paren.expr() {
                self.check(&inner, expected);
            }
            return;
        }
        if is_literal(expr) {
            self.literal(expr, expected, false);
            return;
        }
        if let Expr::ListExpr(list) = expr {
            match expected.strip_nullable() {
                Ty::List(item) | Ty::Set(item) => {
                    let item = (**item).clone();
                    for element in list.exprs() {
                        self.check(&element, &item);
                    }
                    let length = Const::List(list.exprs().map(|_| Const::Other).collect());
                    if let Err(problem) = literal::check(self.table, expected, &length) {
                        self.literal_problem(expr, expected, problem, false);
                    }
                    return;
                }
                Ty::Error | Ty::Var(_) | Ty::Opaque(..) => {}
                _ => {
                    for element in list.exprs() {
                        self.infer(&element);
                    }
                    let actual = Ty::List(Box::new(Ty::Error));
                    self.mismatch(expr, &actual, expected);
                    return;
                }
            }
        }
        let actual = self.infer(expr);
        self.require(expr, &actual, expected);
    }

    /// Reports `actual` where `expected` is required, unless it widens.
    fn require(&mut self, expr: &Expr, actual: &Ty, expected: &Ty) {
        if self.table.assignable(actual, expected) {
            return;
        }
        self.mismatch(expr, actual, expected);
    }

    fn mismatch(&mut self, expr: &Expr, actual: &Ty, expected: &Ty) {
        let span = self.span(expr);
        let written = expr.syntax().text().to_string();
        if self.table.siblings(actual, expected) {
            let common = self
                .table
                .parent(expected.strip_nullable())
                .map_or_else(|| "the same type".to_string(), |t| format!("`{t}`"));
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_SIBLING_MISMATCH,
                    format!("`{actual}` can't be used where `{expected}` is required"),
                )
                .at(span, format!("this is `{actual}`"))
                .reason(format!(
                    "Both refine {common}, but a field type widens only to its own declared type, so one type's values never stand in for another's."
                ))
                .next(format!("Pass a `{expected}` here")),
            );
            return;
        }
        let problem = Diagnostic::new(
            &rule::TYPE_MISMATCH,
            format!("`{actual}` can't be used where `{expected}` is required"),
        )
        .at(span.clone(), format!("this is `{actual}`"));
        let representation = self.table.representation(expected);
        let problem = if self.table.is_nullable(actual) && !self.table.is_nullable(expected) {
            problem
                .reason(format!(
                    "`{actual}` may be `none`, and `{expected}` can't be."
                ))
                .next(format!("Use `match` to handle `none` before using `{written}` as `{expected}`"))
        } else if matches!(actual, Ty::Prim(prim) if Some(*prim) == representation && prim.is_representation())
        {
            let target = expected.strip_nullable().to_string();
            problem
                .reason(format!(
                    "Arithmetic works on the representation, so this is a plain `{actual}`; only `{target}(...)` makes it a `{target}`, checking its rules."
                ))
                .next(format!("Convert it with `{target}({written})`"))
                .edit(Span::new(self.path, span.start, span.start), format!("{target}("))
                .edit(Span::new(self.path, span.end, span.end), ")")
        } else {
            problem
                .reason(format!(
                    "`{actual}` is {} and `{expected}` is {}; a value widens only along declared steps, from a field type to its declared type and from a named type to the type it builds on.",
                    literal::what(self.table, actual),
                    literal::what(self.table, expected)
                ))
                .next(format!("Use a `{expected}` value here"))
        };
        self.out.push(problem);
    }

    /// A literal where `expected` is required; `constructor` when it is the
    /// value given to `Type(...)`.
    fn literal(&mut self, expr: &Expr, expected: &Ty, constructor: bool) {
        let value = Const::from_expr(expr);
        if let Err(problem) = literal::check(self.table, expected, &value) {
            self.literal_problem(expr, expected, problem, constructor);
        }
    }

    fn literal_problem(&mut self, expr: &Expr, expected: &Ty, problem: literal::Problem, constructor: bool) {
        let written = tree::written(expr.syntax());
        let span = self.span(expr);
        self.out.push(literal::diagnostic(
            self.table,
            problem,
            &written,
            expected,
            span,
            constructor,
        ));
    }

    // --- the type of an expression ----------------------------------------

    fn infer(&mut self, expr: &Expr) -> Ty {
        match expr {
            Expr::Literal(_) => match Const::from_expr(expr) {
                Const::Number { decimal: true, .. } => Ty::Prim(Prim::Decimal),
                Const::Number { .. } => Ty::Prim(Prim::Int),
                Const::Str(_) => Ty::Prim(Prim::Text),
                Const::Bool(_) => Ty::Prim(Prim::Bool),
                Const::Duration(_) => Ty::Prim(Prim::Duration),
                Const::None => Ty::None,
                _ => Ty::Error,
            },
            Expr::ParenExpr(paren) => paren.expr().map_or(Ty::Error, |inner| self.infer(&inner)),
            Expr::PrefixExpr(prefix) => {
                let inner = prefix.expr().map_or(Ty::Error, |inner| self.infer(&inner));
                let minus = prefix.syntax().first_token().map(|t| t.kind()) == Some(SyntaxKind::MINUS);
                match (minus, self.table.representation(&inner)) {
                    (false, _) => Ty::Prim(Prim::Bool),
                    (true, Some(prim)) if prim.is_number() => Ty::Prim(prim),
                    _ => Ty::Error,
                }
            }
            Expr::NameRef(name) => self.local(&text(name)).unwrap_or(Ty::Error),
            Expr::FieldExpr(field) => self.field_expr(expr, field),
            Expr::CallExpr(call) => self.call(call),
            Expr::RecordExpr(record) => self.record(record),
            Expr::ListExpr(list) => {
                let mut item = Ty::Error;
                for element in list.exprs() {
                    if item.is_error() && !is_literal(&element) {
                        item = self.infer(&element);
                    } else if !item.is_error() {
                        self.check(&element, &item.clone());
                    } else {
                        self.infer(&element);
                    }
                }
                Ty::List(Box::new(item))
            }
            Expr::BinExpr(binary) => self.binary(binary),
            Expr::AttemptExpr(attempt) => attempt.expr().map_or(Ty::Error, |inner| self.infer(&inner)),
            Expr::MatchExpr(subject) => {
                if let Some(inner) = subject.expr() {
                    self.infer(&inner);
                }
                Ty::Error
            }
            _ => Ty::Error,
        }
    }

    fn binary(&mut self, binary: &ast::BinExpr) -> Ty {
        let op = binary
            .syntax()
            .children_with_tokens()
            .filter_map(|element| element.into_token())
            .find(|token| !token.kind().is_trivia())
            .map(|token| token.text().to_string())
            .unwrap_or_default();
        let lhs = binary.lhs().map_or(Ty::Error, |e| self.infer(&e));
        let rhs = binary.rhs().map_or(Ty::Error, |e| self.infer(&e));
        match op.as_str() {
            "+" | "-" | "*" | "/" | "%" => {
                let (Some(a), Some(b)) = (
                    self.table.representation(&lhs),
                    self.table.representation(&rhs),
                ) else {
                    return Ty::Error;
                };
                if a == Prim::Text && b == Prim::Text && op == "+" {
                    return Ty::Prim(Prim::Text);
                }
                if !a.is_number() || !b.is_number() {
                    return Ty::Error;
                }
                if a == Prim::Decimal || b == Prim::Decimal {
                    Ty::Prim(Prim::Decimal)
                } else {
                    Ty::Prim(Prim::Int)
                }
            }
            _ => Ty::Prim(Prim::Bool),
        }
    }

    // --- reading fields ---------------------------------------------------

    fn field_expr(&mut self, expr: &Expr, field: &ast::FieldExpr) -> Ty {
        let Some((first, rest)) = dotted(expr) else {
            let receiver = field.expr().map_or(Ty::Error, |inner| self.infer(&inner));
            return match field.name_ref() {
                Some(name) => self.select(&receiver, &name, field),
                None => Ty::Error,
            };
        };
        let head = text(&first);
        if starts_upper(&head) {
            return self.type_member(&head, &rest, expr);
        }
        let Some(mut receiver) = self.local(&head) else {
            return Ty::Error;
        };
        let fields: Vec<ast::FieldExpr> = expr
            .syntax()
            .descendants()
            .filter_map(ast::FieldExpr::cast)
            .collect();
        // Innermost first: `a.b` before `a.b.c`.
        for (name, node) in rest.iter().zip(fields.iter().rev()) {
            receiver = self.select(&receiver, name, node);
            if receiver.is_error() {
                break;
            }
        }
        receiver
    }

    /// `Type.member` as a value: an enum's variant.
    fn type_member(&mut self, host: &str, rest: &[ast::NameRef], expr: &Expr) -> Ty {
        let Some(variants) = self.table.variants(host) else {
            return Ty::Error;
        };
        let [variant] = rest else {
            return Ty::Error;
        };
        let name = text(variant);
        match variants.iter().find(|v| v.name == name) {
            None => {
                self.unknown_variant(host, variant);
                Ty::Error
            }
            Some(found) => {
                if let Some(fields) = &found.fields
                    && !fields.is_empty()
                {
                    let span = self.span(expr);
                    let names: Vec<String> = fields.iter().map(|f| format!("`{}`", f.name)).collect();
                    self.out.push(
                        Diagnostic::new(
                            &rule::TYPE_MISSING_VARIANT_FIELD,
                            format!("`{host}.{name}` is missing {}", crate::and_list(&names)),
                        )
                        .at(span, "no fields given")
                        .reason(format!(
                            "`{host}.{name}` carries fields, given in a block when the value is built."
                        ))
                        .next(format!("Write `{host}.{name} {{ ... }}` with each field")),
                    );
                }
                Ty::Named(host.to_string())
            }
        }
    }

    fn unknown_variant(&mut self, host: &str, variant: &ast::NameRef) {
        let name = text(variant);
        let span = self.span(variant);
        let names: Vec<String> = self
            .table
            .variants(host)
            .map(|variants| variants.iter().map(|v| v.name.clone()).collect())
            .unwrap_or_default();
        let near = nearest(&name, names.iter().map(String::as_str)).map(str::to_string);
        let listed: Vec<String> = names.iter().map(|n| format!("`{n}`")).collect();
        let problem = Diagnostic::new(
            &rule::TYPE_UNKNOWN_ENUM_VARIANT,
            format!("`{host}` has no variant `{name}`"),
        )
        .at(span.clone(), "not a variant")
        .reason(format!(
            "`{host}`'s variants are {}.{}",
            crate::and_list(&listed),
            did_you_mean(near.as_deref())
        ));
        self.out.push(match near {
            Some(near) => problem
                .next(format!("Use `{host}.{near}`"))
                .edit(span, near),
            None => problem.next(format!("Use one of `{host}`'s variants")),
        });
    }

    /// The object a value's fields come from, following field types to
    /// their declared types.
    fn object_of(&self, ty: &Ty) -> Option<String> {
        let mut current = ty.strip_nullable().clone();
        for _ in 0..64 {
            match &current {
                Ty::Named(name) => {
                    return self.table.fields(name).map(|_| name.clone());
                }
                Ty::Field(..) => current = self.table.parent(&current)?,
                _ => return None,
            }
        }
        None
    }

    /// Reads field `name` from a value of type `receiver`.
    fn select(&mut self, receiver: &Ty, name: &ast::NameRef, node: &ast::FieldExpr) -> Ty {
        if receiver.has_error() {
            return Ty::Error;
        }
        let field = text(name);
        let span = self.span(name);
        let value = node
            .expr()
            .map(|e| tree::written(e.syntax()))
            .unwrap_or_default();
        if self.table.is_nullable(receiver) {
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_NULLABLE_SELECTION,
                    format!("`{value}` may be `none`, so `{field}` can't be read yet"),
                )
                .at(span, "may be `none`")
                .reason(format!(
                    "`{value}` is `{receiver}`; a field is read once `match` has found a value."
                ))
                .next(format!("Match on `{value}` and read `{field}` inside `some(...)`")),
            );
            return Ty::Error;
        }
        let Some(host) = self.object_of(receiver) else {
            let start = node.dot_token().map_or(span.start, |dot| u32::from(dot.text_range().start()));
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_FIELD_ON_NON_RECORD,
                    format!("`{receiver}` has no fields"),
                )
                .at(span.clone(), "no such field")
                .reason(format!(
                    "`{value}` is a `{receiver}`, {}; only objects, inputs, outputs, events, failures and entities have fields.",
                    literal::what(self.table, receiver)
                ))
                .next(format!("Use `{value}` itself"))
                .edit(Span::new(self.path, start, span.end), ""),
            );
            return Ty::Error;
        };
        let fields = self.table.fields(&host).unwrap_or(&[]);
        match fields.iter().find(|f| f.name == field) {
            Some(found) => {
                let ty = Ty::Field(host, vec![field]);
                if found.optional { ty.nullable() } else { ty }
            }
            None => {
                self.unknown_field(&host, &field, fields, span, false);
                Ty::Error
            }
        }
    }

    fn unknown_field(&mut self, host: &str, field: &str, fields: &[FieldDef], span: Span, variant: bool) {
        let names: Vec<&str> = fields.iter().map(|f| f.name.as_str()).collect();
        let near = nearest(field, names.iter().copied());
        let listed: Vec<String> = names.iter().map(|n| format!("`{n}`")).collect();
        let rule = if variant {
            &rule::TYPE_UNKNOWN_VARIANT_FIELD
        } else {
            &rule::TYPE_UNKNOWN_FIELD
        };
        let fields_line = if listed.is_empty() {
            format!("`{host}` has no fields.")
        } else {
            format!("`{host}`'s fields are {}.", crate::and_list(&listed))
        };
        let problem = Diagnostic::new(rule, format!("`{host}` has no field `{field}`"))
            .at(span.clone(), "not declared")
            .reason(format!("{fields_line}{}", did_you_mean(near)));
        self.out.push(match near {
            Some(near) => problem
                .next(format!("Use `{near}` instead of `{field}`"))
                .edit(span, near),
            None => problem.next(format!("Remove `{field}`, or declare it on `{host}`")),
        });
    }

    // --- calls ------------------------------------------------------------

    fn call(&mut self, call: &ast::CallExpr) -> Ty {
        match call.expr() {
            Some(Expr::NameRef(name)) => {
                let callee = text(&name);
                if starts_upper(&callee) {
                    return self.construct(&callee, None, call);
                }
                if self.local(&callee).is_some() {
                    self.args_only(call);
                    return Ty::Error;
                }
                let sig = self
                    .library
                    .as_ref()
                    .and_then(|library| self.calls.library.get(&(library.clone(), callee.clone())))
                    .or_else(|| self.calls.plain.get(&callee))
                    .cloned();
                match sig {
                    Some(sig) => {
                        self.args(&sig, call);
                        sig.result
                    }
                    None => {
                        self.args_only(call);
                        Ty::Error
                    }
                }
            }
            Some(callee @ Expr::FieldExpr(_)) => {
                let Some((first, rest)) = dotted(&callee) else {
                    self.infer(&callee);
                    self.args_only(call);
                    return Ty::Error;
                };
                let head = text(&first);
                let rest: Vec<String> = rest.iter().map(text).collect();
                if starts_upper(&head) {
                    if self.table.field(&head, &rest).is_some() {
                        let target = Ty::Field(head.clone(), rest.clone());
                        return self.construct(&format!("{head}.{}", rest.join(".")), Some(target), call);
                    }
                    self.args_only(call);
                    return Ty::Error;
                }
                if self.local(&head).is_some() {
                    self.args_only(call);
                    return Ty::Error;
                }
                let [member] = rest.as_slice() else {
                    self.args_only(call);
                    return Ty::Error;
                };
                let names = StdNames::get();
                if names.namespace(&head) == Some(Namespace::Standard)
                    && let Some(operation) = names.operation(&head, member)
                {
                    return self.std_call(operation, call);
                }
                let sig = if self.symbols.library(&head).is_some() {
                    self.calls.library.get(&(head.clone(), member.clone()))
                } else if self.symbols.domains.contains(&head) {
                    self.calls.plain.get(member).filter(|_| {
                        self.symbols.get(member).is_some_and(|symbol| {
                            matches!(symbol.kind, ItemKind::Action | ItemKind::Query)
                        })
                    })
                } else {
                    None
                };
                match sig.cloned() {
                    Some(sig) => {
                        self.args(&sig, call);
                        sig.result
                    }
                    None => {
                        self.args_only(call);
                        Ty::Error
                    }
                }
            }
            Some(other) => {
                self.infer(&other);
                self.args_only(call);
                Ty::Error
            }
            None => Ty::Error,
        }
    }

    fn positional(call: &ast::CallExpr) -> Vec<(Option<String>, Expr)> {
        call.arg_list()
            .iter()
            .flat_map(|list| list.args())
            .filter_map(|arg| {
                let name = arg.name().as_ref().and_then(tree::name_text);
                Some((name, arg.expr()?))
            })
            .collect()
    }

    /// Types every argument without an expected type.
    fn args_only(&mut self, call: &ast::CallExpr) {
        for (_, value) in Self::positional(call) {
            self.infer(&value);
        }
    }

    /// Checks arguments against a signature, in order; how many there are
    /// and named arguments are area 3's to check.
    fn args(&mut self, sig: &Sig, call: &ast::CallExpr) {
        for (index, (name, value)) in Self::positional(call).into_iter().enumerate() {
            match (name, sig.params.get(index)) {
                (None, Some((_, expected))) => {
                    let expected = expected.clone();
                    self.check(&value, &expected);
                }
                _ => {
                    self.infer(&value);
                }
            }
        }
    }

    /// A standard-library call: the stub's type variables are fixed by the
    /// arguments that aren't literals, then every argument is checked.
    fn std_call(&mut self, operation: &Operation, call: &ast::CallExpr) -> Ty {
        let params: Vec<(String, Ty)> = operation
            .params
            .iter()
            .map(|(name, ty)| (name.clone(), self.table.resolve_std(ty)))
            .collect();
        let args = Self::positional(call);
        let mut bound: BTreeMap<String, Ty> = BTreeMap::new();
        let mut actual: Vec<Option<Ty>> = Vec::new();
        for (index, (name, value)) in args.iter().enumerate() {
            let param = match name {
                Some(name) => params.iter().find(|(p, _)| p == name).map(|(_, t)| t),
                None => params.get(index).map(|(_, t)| t),
            };
            let is_function = matches!(param, Some(Ty::Function(..)));
            if is_literal(value) || matches!(value, Expr::ListExpr(_)) || is_function {
                actual.push(None);
                continue;
            }
            let ty = self.infer(value);
            if let Some(param) = param {
                unify(param, &ty, &mut bound);
            }
            actual.push(Some(ty));
        }
        for (index, ((name, value), ty)) in args.iter().zip(actual).enumerate() {
            let param = match name {
                Some(name) => params.iter().find(|(p, _)| p == name).map(|(_, t)| t),
                None => params.get(index).map(|(_, t)| t),
            };
            let Some(param) = param else {
                if ty.is_none() {
                    self.infer(value);
                }
                continue;
            };
            if matches!(param, Ty::Function(..)) {
                continue;
            }
            let expected = substitute(param, &bound);
            match ty {
                Some(ty) => self.require(value, &ty, &expected),
                None => self.check(value, &expected),
            }
        }
        substitute(&self.table.resolve_std(&operation.result), &bound)
    }

    // --- building values ----------------------------------------------------

    /// `Type(value)`: re-checks a value against a named type's rules, or a
    /// field type's (`Owner.field(value)`).
    fn construct(&mut self, written: &str, field: Option<Ty>, call: &ast::CallExpr) -> Ty {
        let target = match field {
            Some(field) => field,
            None => {
                if let Some(prim) = Prim::from_name(written) {
                    if prim.is_representation() || prim == Prim::Unit {
                        self.args_only(call);
                        let span = call.expr().map_or_else(|| self.span(call), |e| self.span(&e));
                        self.out.push(
                            Diagnostic::new(
                                &rule::TYPE_CONSTRUCTOR_INPUT,
                                format!("`{written}` has no constructor"),
                            )
                            .at(span, "a built-in type")
                            .reason(format!(
                                "`{written}` is raw material, not a meaning: no value is converted back to it. Literals and arithmetic give `{written}` values directly."
                            ))
                            .next("Use the value as it is, or construct a named type"),
                        );
                        return Ty::Error;
                    }
                    Ty::Prim(prim)
                } else {
                    match self.table.def(written) {
                        Some(Def::Scalar { .. }) => Ty::Named(written.to_string()),
                        Some(Def::Enum { variants }) => {
                            let variants = variants.clone();
                            self.enum_constructor(written, &variants, call);
                            return Ty::Named(written.to_string());
                        }
                        Some(Def::Fields { base, .. }) => {
                            use super::shape::FieldsBase;
                            if matches!(base, FieldsBase::Entity | FieldsBase::Failure) {
                                // Resolution reports these (area 1).
                                self.args_only(call);
                                return Ty::Error;
                            }
                            self.args_only(call);
                            let span = call.expr().map_or_else(|| self.span(call), |e| self.span(&e));
                            self.out.push(
                                Diagnostic::new(
                                    &rule::TYPE_CONSTRUCTOR_INPUT,
                                    format!("`{written}` is built with `{{ ... }}`"),
                                )
                                .at(span, "an object")
                                .reason(format!(
                                    "`{written}` has fields, so a value is written with its name and every field in a block."
                                ))
                                .next(format!("Write `{written} {{ ... }}` with each field")),
                            );
                            return Ty::Named(written.to_string());
                        }
                        None => {
                            self.args_only(call);
                            return Ty::Error;
                        }
                    }
                }
            }
        };
        let args = Self::positional(call);
        if args.len() != 1 {
            for (_, value) in &args {
                self.infer(value);
            }
            let span = self.span(call);
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_CONSTRUCTOR_ARGUMENT_COUNT,
                    format!("`{written}(...)` takes one value, not {}", args.len()),
                )
                .at(span, format!("{} given", args.len()))
                .reason(format!(
                    "Constructing a `{written}` checks one value against its rules."
                ))
                .next(format!("Pass one value to `{written}(...)`")),
            );
            return target;
        }
        let value = &args[0].1;
        if is_literal(value) {
            self.literal(value, &target, true);
            return target;
        }
        let actual = self.infer(value);
        if actual.has_error() {
            return target;
        }
        let from = self.table.representation(&actual);
        let to = self.table.representation(&target);
        if from.is_none() || from != to {
            let span = self.span(value);
            let made_of = |ty: &Ty, prim: Option<Prim>| match prim {
                Some(prim) => format!("`{ty}` is built on `{}`", prim.name()),
                None => format!("`{ty}` is {}", literal::what(self.table, ty)),
            };
            let target_rep = to.map_or_else(|| target.to_string(), |prim| prim.name().to_string());
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_CONSTRUCTOR_INPUT,
                    format!("`{target}` can't be built from `{actual}`"),
                )
                .at(span, format!("this is `{actual}`"))
                .reason(format!(
                    "{} and {}; a constructor converts only between types with the same representation, re-checking the rules.",
                    made_of(&target, to),
                    made_of(&actual, from)
                ))
                .next(format!("Pass a value built on `{target_rep}`")),
            );
        }
        target
    }

    /// `Enum(value)`: an enum is never built from a value.
    fn enum_constructor(&mut self, host: &str, variants: &[super::table::VariantDef], call: &ast::CallExpr) {
        let args = Self::positional(call);
        let matching = args.first().and_then(|(_, value)| match Const::from_expr(value) {
            Const::Str(text) => variants.iter().find(|v| v.name == text).map(|v| v.name.clone()),
            _ => None,
        });
        if matching.is_none() {
            for (_, value) in &args {
                self.infer(value);
            }
        }
        let span = self.span(call);
        let problem = Diagnostic::new(
            &rule::TYPE_CONSTRUCTOR_INPUT,
            format!("`{host}` can't be built from a value"),
        )
        .at(span.clone(), "an enum")
        .reason("An enum's values are its variants, written `Enum.variant`; text from outside becomes a variant only by being decoded at a boundary.");
        self.out.push(match matching {
            Some(variant) => problem
                .next(format!("Write `{host}.{variant}`"))
                .edit(span, format!("{host}.{variant}")),
            None => problem.next(format!("Write one of `{host}`'s variants, such as `{host}.{}`",
                variants.first().map_or("", |v| v.name.as_str()))),
        });
    }

    /// `Type { ... }` and `Enum.variant { ... }`.
    fn record(&mut self, record: &ast::RecordExpr) -> Ty {
        let Some(head) = record.expr() else {
            return Ty::Error;
        };
        let Some((first, rest)) = dotted(&head) else {
            self.infer(&head);
            return Ty::Error;
        };
        let name = text(&first);
        if !starts_upper(&name) {
            return Ty::Error;
        }
        let head_span = self.span(&head);
        match (rest.as_slice(), self.table.def(&name)) {
            ([], Some(Def::Fields { base, fields })) => {
                use super::shape::FieldsBase;
                if matches!(base, FieldsBase::Entity | FieldsBase::Failure) {
                    return Ty::Error;
                }
                let fields = fields.clone();
                let input = *base == FieldsBase::Input;
                self.fields_given(record, &name, &[], &fields, input, head_span);
                Ty::Named(name)
            }
            ([], Some(Def::Scalar { .. })) | ([], None) if Prim::from_name(&name).is_some() || self.table.def(&name).is_some() => {
                self.not_record(record, &name, "scalar");
                Ty::Named(name)
            }
            ([], Some(Def::Enum { .. })) => {
                self.not_record(record, &name, "enum");
                Ty::Named(name)
            }
            ([variant], Some(Def::Enum { variants })) => {
                let variant_name = text(variant);
                let Some(found) = variants.iter().find(|v| v.name == variant_name).cloned() else {
                    self.unknown_variant(&name, variant);
                    return Ty::Error;
                };
                match &found.fields {
                    Some(fields) => {
                        let fields = fields.clone();
                        self.fields_given(record, &name, &[variant_name], &fields, false, head_span);
                    }
                    None => self.not_record(record, &format!("{name}.{variant_name}"), "plain variant"),
                }
                Ty::Named(name)
            }
            _ => Ty::Error,
        }
    }

    fn not_record(&mut self, record: &ast::RecordExpr, written: &str, kind: &str) {
        let span = self.span(record);
        let (reason, next) = match kind {
            "enum" => (
                format!("`{written}` is an enum; its values are its variants."),
                format!("Write one of `{written}`'s variants, such as `{written}.{}`",
                    self.table.variants(written).and_then(|v| v.first()).map_or("", |v| v.name.as_str())),
            ),
            "plain variant" => (
                format!("`{written}` carries no fields."),
                format!("Write `{written}` without a block"),
            ),
            _ => {
                let base = self
                    .table
                    .base_prim(&Ty::Named(written.to_string()))
                    .or_else(|| Prim::from_name(written))
                    .map_or_else(String::new, |prim| format!(", built on `{}`,", prim.name()));
                (
                    format!("Braces build objects and variants with fields; `{written}`{base} takes one value in brackets."),
                    format!("Write `{written}(...)` with one value"),
                )
            }
        };
        self.out.push(
            Diagnostic::new(&rule::TYPE_NOT_RECORD, format!("`{written}` isn't built with `{{ ... }}`"))
                .at(span, "has no fields")
                .reason(reason)
                .next(next),
        );
    }

    /// The fields given in a block: each one declared, none twice, every
    /// required one present, each value checked against its field.
    fn fields_given(
        &mut self,
        record: &ast::RecordExpr,
        host: &str,
        variant: &[String],
        fields: &[FieldDef],
        input: bool,
        head_span: Span,
    ) {
        let is_variant = !variant.is_empty();
        let shown = if is_variant {
            format!("{host}.{}", variant.join("."))
        } else {
            host.to_string()
        };
        let mut given: BTreeMap<String, Span> = BTreeMap::new();
        for item in record.member_list().iter().flat_map(|list| list.member_items()) {
            let MemberItem::Member(member) = item else { continue };
            let Some(key) = member.member_key() else { continue };
            let mut names = key.names();
            let (Some(first), None) = (names.next(), names.next()) else {
                continue;
            };
            let Some(name) = tree::name_text(&first) else { continue };
            let span = self.span(&first);
            let value = member.value();
            if let Some(earlier) = given.get(&name) {
                let rule = if is_variant {
                    &rule::TYPE_DUPLICATE_VARIANT_FIELD
                } else {
                    &rule::TYPE_DUPLICATE_FIELD
                };
                self.out.push(
                    Diagnostic::new(rule, format!("`{name}` is given twice"))
                        .at(span, "given again")
                        .also(earlier.clone(), "first given here")
                        .reason(format!(
                            "Each field of `{shown}` is given once, so there is never a question of which value counts."
                        ))
                        .next(format!("Remove one `{name}`")),
                );
                if let Some(value) = value {
                    self.infer(&value);
                }
                continue;
            }
            given.insert(name.clone(), span.clone());
            let Some(field) = fields.iter().find(|f| f.name == name) else {
                self.unknown_field(&shown, &name, fields, span, is_variant);
                if let Some(value) = value {
                    self.infer(&value);
                }
                continue;
            };
            let Some(value) = value else { continue };
            let mut path = variant.to_vec();
            path.push(name);
            let field_ty = Ty::Field(host.to_string(), path);
            if is_literal(&value) || matches!(value, Expr::ListExpr(_)) {
                // A literal takes the field type, so the field's own rules
                // are checked too.
                let expected = if field.optional && !self.table.is_nullable(&field_ty) {
                    field_ty.nullable()
                } else {
                    field_ty
                };
                self.check(&value, &expected);
            } else {
                let mut expected = field.declared.clone();
                if field.optional {
                    expected = expected.nullable();
                }
                self.check(&value, &expected);
            }
        }
        let missing: Vec<String> = fields
            .iter()
            .filter(|f| !given.contains_key(&f.name))
            .filter(|f| !(input && (f.optional || f.default)))
            .map(|f| format!("`{}`", f.name))
            .collect();
        if missing.is_empty() {
            return;
        }
        let rule = if is_variant {
            &rule::TYPE_MISSING_VARIANT_FIELD
        } else {
            &rule::TYPE_MISSING_FIELD
        };
        let first = missing[0].trim_matches('`').to_string();
        self.out.push(
            Diagnostic::new(rule, format!("`{shown}` is missing {}", crate::and_list(&missing)))
                .at(head_span, "fields missing")
                .reason(format!(
                    "Every field of `{shown}` is given when a value is built; a field that may be absent is declared `T?` and given as `none`."
                ))
                .next(format!("Add `{first}: ...` to the block")),
        );
    }
}

/// Binds the type variables in `param` from an argument's type.
fn unify(param: &Ty, actual: &Ty, bound: &mut BTreeMap<String, Ty>) {
    match (param, actual.strip_nullable()) {
        (Ty::Var(name), _) => {
            bound.entry(name.clone()).or_insert_with(|| actual.clone());
        }
        (Ty::List(p), Ty::List(a)) | (Ty::Set(p), Ty::Set(a)) => unify(p, a, bound),
        (Ty::Nullable(p), a) => unify(p, a, bound),
        (Ty::Map(pk, pv), Ty::Map(ak, av)) => {
            unify(pk, ak, bound);
            unify(pv, av, bound);
        }
        _ => {}
    }
}

/// `param` with its bound type variables filled in; unbound ones are
/// `Error`.
fn substitute(param: &Ty, bound: &BTreeMap<String, Ty>) -> Ty {
    match param {
        Ty::Var(name) => bound.get(name).cloned().unwrap_or(Ty::Error),
        Ty::List(inner) => Ty::List(Box::new(substitute(inner, bound))),
        Ty::Set(inner) => Ty::Set(Box::new(substitute(inner, bound))),
        Ty::Nullable(inner) => substitute(inner, bound).nullable(),
        Ty::Map(key, value) => Ty::Map(
            Box::new(substitute(key, bound)),
            Box::new(substitute(value, bound)),
        ),
        Ty::Function(from, to) => Ty::Function(
            Box::new(substitute(from, bound)),
            Box::new(substitute(to, bound)),
        ),
        other => other.clone(),
    }
}
