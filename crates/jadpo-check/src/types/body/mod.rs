//! The rules inside bodies: every value's type, checked bidirectionally
//! (area 2), and the logic of statements, operators, calls and `match`
//! (area 3).
//!
//! The type a position expects (a variable's declared type, a return, an
//! argument, a field, a match arm) flows into literals; any other value
//! must widen to it. Statements are checked in order with what each one
//! leaves in scope, and each statement says whether the code after it can
//! run, which gives unreachable code and callables that end without their
//! result. Every local is tracked until its scope closes, so an unused one
//! is reported once its callable has been read.
//!
//! Failures (area 4) are followed through the same walk: every `reject`,
//! `attempt` and outcome `match` says which failures can leave the
//! callable, and the `fails` list must name exactly those. Where this check
//! cannot know a type or a failure yet (an entity's operations, area 6), it
//! uses `Error` or an open failure set, which accept everything, so nothing
//! here reports a knock-on of something it doesn't check.

mod calls;
mod expr;
mod failing;
mod matching;
mod ops;
mod outcome;
mod ranges;

use std::collections::{BTreeMap, BTreeSet};

use jadpo_db::{Db, File, Project, Symbols, symbols};
use jadpo_diag::{Diagnostic, Span, rule};
use jadpo_syntax::ast::{self, AstNode, BlockItem, Decl, Expr, Stmt};
use jadpo_syntax::{SyntaxKind, SyntaxNode};

use super::decl::Decls;
use super::signatures::{Callables, callables};
use super::table::{TypeTable, type_table};
use super::ty::{Prim, Ty, TypeNode, TypeRef};
use crate::case::is_lower_snake;
use crate::failures::{FailureTable, failure_table};
use crate::std_lib::StdNames;
use crate::tree::{self, node_span};

pub use failing::Fallible;

/// Which failures can leave one callable (area 4).
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct FailureSet {
    /// The declared failures a `reject`, an `attempt` or an outcome `match`
    /// lets out, by name.
    pub failures: BTreeSet<String>,
    /// Whether something else may leave that this check can't name yet,
    /// such as an entity operation's failure (area 6), or a mistake
    /// already reported.
    pub open: bool,
}

/// The failure set of every function and action in one file, by name;
/// a library's functions as `library.function`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct FailureSets {
    pub callables: BTreeMap<String, FailureSet>,
}

/// Everything checking one file's bodies finds.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BodyCheck {
    pub problems: Vec<Diagnostic>,
    pub sets: FailureSets,
}

/// Checks one file's bodies.
#[salsa::tracked(returns(ref))]
pub fn body_check(db: &dyn Db, project: Project, file: File) -> BodyCheck {
    let Some(source) = tree::source(db, file) else {
        return BodyCheck::default();
    };
    let mut checker = Body {
        path: file.path(db),
        table: type_table(db, project),
        calls: callables(db, project),
        symbols: symbols(db, project),
        failures: failure_table(db, project),
        scopes: Vec::new(),
        result: Ty::Prim(Prim::Unit),
        context: Context::Action,
        library: None,
        unknown_name: false,
        unused: Vec::new(),
        last_call: CallKind::Other,
        dead: false,
        flow: Flow::default(),
        handled: Vec::new(),
        constructions: BTreeMap::new(),
        types: BTreeMap::new(),
        arm: Arm::None,
        sets: FailureSets::default(),
        out: Vec::new(),
    };
    for decl in source.decls() {
        checker.decl(&decl);
    }
    BodyCheck {
        problems: checker.out,
        sets: checker.sets,
    }
}

/// The problems inside one file's bodies.
pub fn body_problems(db: &dyn Db, project: Project, file: File) -> &Vec<Diagnostic> {
    &body_check(db, project, file).problems
}

/// Which failures can leave each function and action in one file: a query
/// of its own, so whatever reads the sets is reused when an edit leaves
/// them equal.
#[salsa::tracked(returns(ref))]
pub fn failure_sets(db: &dyn Db, project: Project, file: File) -> FailureSets {
    body_check(db, project, file).sets.clone()
}

/// What can leave the callable being checked.
#[derive(Clone, Debug, Default)]
struct Flow {
    /// The callable's name, for messages; empty for tests and seeds.
    name: String,
    /// Each failure that can leave, with where it first does and how.
    leaving: BTreeMap<String, (Span, &'static str)>,
    /// See [`FailureSet::open`].
    open: bool,
}

/// The `match` arm whose body is being checked, which decides what
/// `propagate` means.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Arm {
    /// Not in an outcome arm: `propagate` has nothing to pass on.
    None,
    /// The success arm of an outcome `match`.
    Success,
    /// A failure arm, with the failure it handles; `None` when the arm's
    /// pattern is already reported.
    Failure(Option<String>),
}

/// A node's position in the file, as a key.
fn key(node: &SyntaxNode) -> (u32, u32) {
    let range = node.text_range();
    (range.start().into(), range.end().into())
}

/// What kind of code is being checked, which decides what it may call
/// (area 3, "Functions and actions").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Context {
    /// A function or library function: pure.
    Function,
    /// An inline function given to a collection operation: pure too.
    Inline,
    /// An action, and the test-only callables that act like one.
    Action,
}

impl Context {
    fn is_pure(self) -> bool {
        matches!(self, Context::Function | Context::Inline)
    }

    fn with_a(self) -> &'static str {
        match self {
            Context::Function => "a function",
            Context::Inline => "an inline function",
            Context::Action => "an action",
        }
    }
}

/// How a name came into scope.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LocalKind {
    Param,
    Var,
    VarMut,
    /// A `for` loop's item.
    Item,
    /// A name bound by a `match` arm.
    Binding,
    /// An inline function's parameter.
    Inline,
}

#[derive(Clone, Debug)]
struct Local {
    name: String,
    ty: Ty,
    kind: LocalKind,
    /// Where it is declared: its name.
    span: Span,
    /// For a binding, the brackets around it, which go when it is unused.
    binding: Option<Span>,
    used: bool,
}

/// What the last call checked was, for a call standing on its own.
#[derive(Clone, Debug, PartialEq, Eq)]
enum CallKind {
    /// A function, a constructor or a standard-library operation: it only
    /// computes its result.
    Pure(String),
    /// An action, or anything else that may have effects.
    Effect(String),
    /// Not known yet.
    Other,
}

struct Body<'a> {
    path: &'a str,
    table: &'a TypeTable,
    calls: &'a Callables,
    symbols: &'a Symbols,
    failures: &'a FailureTable,
    scopes: Vec<Vec<Local>>,
    /// The result type of the callable being checked.
    result: Ty,
    context: Context,
    /// The library whose functions are being checked: they call each other
    /// bare.
    library: Option<String>,
    /// Whether this callable uses a name that isn't defined: an unused
    /// variable is then most likely the name that was meant.
    unknown_name: bool,
    /// Locals whose scope has closed without a read.
    unused: Vec<Local>,
    last_call: CallKind,
    /// Whether the statements being checked are already reported as
    /// unreachable.
    dead: bool,
    flow: Flow,
    /// The calls an `attempt` or an outcome `match` is handling, by
    /// position.
    handled: Vec<(u32, u32)>,
    /// Whether each runtime construction can fail, by the call's position.
    constructions: BTreeMap<(u32, u32), Fallible>,
    /// The type each expression was found to have, by position, for
    /// proving a construction safe.
    types: BTreeMap<(u32, u32), Ty>,
    arm: Arm,
    sets: FailureSets,
    out: Vec<Diagnostic>,
}

/// One callable to check.
struct Callable {
    context: Context,
    /// `function`, `action`, `test` ...
    word: &'static str,
    /// Its name, `library.function` for a library's; `None` for tests and
    /// seeds.
    name: Option<String>,
    params: Option<ast::ParamList>,
    fails: Option<ast::FailsClause>,
    /// Whether it states what can leave it with `fails`.
    contract: bool,
    result: Option<ast::RetType>,
    body: Option<ast::Block>,
}

fn text(name: &ast::NameRef) -> String {
    tree::name_ref_text(name).unwrap_or_default()
}

fn did_you_mean(near: Option<&str>) -> String {
    near.map_or_else(String::new, |near| format!(" Did you mean `{near}`?"))
}

fn capital(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
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

/// The span of a node's first line, without leading trivia: where a
/// statement starts, for statements that run over several lines.
fn line_span(path: &str, node: &jadpo_syntax::SyntaxNode) -> Span {
    let tokens: Vec<_> = node
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .collect();
    let Some(first) = tokens.iter().find(|t| !t.kind().is_trivia()) else {
        return node_span(path, node);
    };
    let start = u32::from(first.text_range().start());
    let mut end = u32::from(first.text_range().end());
    let mut seen_first = false;
    for token in &tokens {
        if token == first {
            seen_first = true;
            continue;
        }
        if !seen_first {
            continue;
        }
        if token.kind().is_trivia() {
            if token.text().contains('\n') {
                break;
            }
            continue;
        }
        end = u32::from(token.text_range().end());
    }
    Span::new(path, start, end)
}

impl Body<'_> {
    fn span(&self, node: &impl AstNode) -> Span {
        node_span(self.path, node.syntax())
    }

    fn resolve(&self, ty: Option<ast::Type>) -> Ty {
        ty.map_or(Ty::Error, |ty| self.table.resolve(&TypeRef::from_type(&ty)))
    }

    fn decl(&mut self, decl: &Decl) {
        let name = tree::decl_name(decl).as_ref().and_then(tree::name_text);
        let callable = |context, word, params, fails, contract, result, body| Callable {
            context,
            word,
            name: name.clone(),
            params,
            fails,
            contract,
            result,
            body,
        };
        match decl {
            Decl::FunctionDecl(d) => self.callable(callable(
                Context::Function,
                "function",
                d.param_list(),
                d.fails_clause(),
                true,
                d.ret_type(),
                d.block(),
            )),
            Decl::ActionDecl(d) => self.callable(callable(
                Context::Action,
                "action",
                d.param_list(),
                d.fails_clause(),
                true,
                d.ret_type(),
                d.block(),
            )),
            Decl::FixtureDecl(d) => self.callable(callable(
                Context::Action,
                "fixture",
                d.param_list(),
                None,
                false,
                d.ret_type(),
                d.block(),
            )),
            Decl::LibraryDecl(d) => {
                self.library = name.clone();
                for function in tree::library_functions(d) {
                    let function_name = function.name().as_ref().and_then(tree::name_text);
                    self.callable(Callable {
                        context: Context::Function,
                        word: "function",
                        name: function_name
                            .map(|f| format!("{}.{f}", name.clone().unwrap_or_default())),
                        params: function.param_list(),
                        fails: function.fails_clause(),
                        contract: true,
                        result: function.ret_type(),
                        body: function.block(),
                    });
                }
                self.library = None;
            }
            Decl::TestDecl(d) => self.callable(Callable {
                name: None,
                ..callable(Context::Action, "test", None, None, false, None, d.block())
            }),
            Decl::SeedDecl(d) => self.callable(Callable {
                name: None,
                ..callable(Context::Action, "seed", None, None, false, None, d.block())
            }),
            _ => {}
        }
    }

    fn callable(&mut self, callable: Callable) {
        let Callable {
            context,
            word,
            name,
            params,
            fails,
            contract,
            result,
            body,
        } = callable;
        self.context = context;
        self.unknown_name = false;
        self.unused.clear();
        self.flow = Flow {
            name: name.clone().unwrap_or_default(),
            ..Flow::default()
        };
        self.arm = Arm::None;
        self.scopes.push(Vec::new());
        for param in params.iter().flat_map(|list| list.params()) {
            let mut ty = self.resolve(param.ty());
            // A built-in representation in a function's or action's
            // signature is already reported (area 2, question 11): what
            // the body does with it follows from that mistake.
            if contract && matches!(ty.strip_nullable(), Ty::Prim(prim) if prim.is_representation())
            {
                ty = Ty::Error;
            }
            self.declare(param.name(), ty, LocalKind::Param, None);
        }
        self.result = match result {
            Some(ret) => self.resolve(ret.ty()),
            None => Ty::Prim(Prim::Unit),
        };
        let completes = self.block(body.clone());
        self.pop_scope();
        if completes
            && let Some(body) = &body
            && !self.result.has_error()
            && self.result != Ty::Prim(Prim::Unit)
        {
            self.missing_return(word, body);
        }
        if !self.unknown_name {
            let unused = std::mem::take(&mut self.unused);
            for local in unused {
                self.unused_variable(&local);
            }
        }
        self.unused.clear();
        if contract {
            self.fails_list(fails.as_ref(), params.as_ref(), word);
        }
        if let Some(name) = name
            && contract
        {
            let flow = std::mem::take(&mut self.flow);
            self.sets.callables.entry(name).or_insert(FailureSet {
                failures: flow.leaving.into_keys().collect(),
                open: flow.open,
            });
        }
    }

    // --- locals -------------------------------------------------------------

    /// Brings a name into the innermost scope. Resolution reports a name
    /// declared twice (area 1); here the latest one wins.
    fn declare(&mut self, name: Option<ast::Name>, ty: Ty, kind: LocalKind, binding: Option<Span>) {
        let Some(name) = name else { return };
        let Some(text) = tree::name_text(&name) else {
            return;
        };
        let span = self.span(&name);
        // A name resolution reports (area 1, and shadowing from area 3) is
        // the mistake on this line, as is code that never runs; whether the
        // name is read is not another one.
        let reported = self.dead
            || self.is_local(&text)
            || !is_lower_snake(&text)
            || StdNames::get().reserved(&text)
            || self.symbols.domains.contains(&text)
            || self.symbols.library(&text).is_some();
        if let Some(scope) = self.scopes.last_mut() {
            scope.push(Local {
                name: text,
                ty,
                kind,
                span,
                binding,
                used: reported,
            });
        }
    }

    fn find(&self, name: &str) -> Option<&Local> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.iter().rev().find(|local| local.name == name))
    }

    /// The type of a local, marking it read.
    fn read(&mut self, name: &str) -> Option<Ty> {
        let local = self
            .scopes
            .iter_mut()
            .rev()
            .find_map(|scope| scope.iter_mut().rev().find(|local| local.name == name))?;
        local.used = true;
        Some(local.ty.clone())
    }

    fn is_local(&self, name: &str) -> bool {
        self.find(name).is_some()
    }

    fn pop_scope(&mut self) {
        let Some(scope) = self.scopes.pop() else {
            return;
        };
        for local in scope {
            if !local.used && !matches!(local.kind, LocalKind::Param | LocalKind::Inline) {
                self.unused.push(local);
            }
        }
    }

    /// The names in scope, for "did you mean".
    fn names_in_scope(&self) -> Vec<String> {
        self.scopes
            .iter()
            .flatten()
            .map(|local| local.name.clone())
            .collect()
    }

    fn unused_variable(&mut self, local: &Local) {
        let name = &local.name;
        let (reason, next) = match local.kind {
            LocalKind::Binding => (
                "A value nobody reads is never bound to a name; a pattern without brackets matches the case without naming its data.".to_string(),
                format!("Remove `({name})` from the pattern"),
            ),
            LocalKind::Item => (
                format!(
                    "A value nobody reads is never bound to a name, and the loop never reads `{name}`."
                ),
                format!("Use `{name}` in the loop, or count the list with `collection.count`"),
            ),
            _ => (
                "A value nobody reads is never bound to a name, so a reader never wonders what it was for.".to_string(),
                format!("Use `{name}`, or remove its `var` line"),
            ),
        };
        let mut problem = Diagnostic::new(
            &rule::SEMANTIC_UNUSED_VARIABLE,
            format!("`{name}` is never used"),
        )
        .at(local.span.clone(), "never read")
        .reason(reason)
        .next(next);
        if let Some(brackets) = &local.binding {
            problem = problem.edit(brackets.clone(), "");
        }
        self.out.push(problem);
    }

    // --- statements -----------------------------------------------------------

    /// Checks a block in a scope of its own; whether the code after it can
    /// run.
    fn block(&mut self, block: Option<ast::Block>) -> bool {
        let Some(block) = block else { return true };
        self.scopes.push(Vec::new());
        let completes = self.statements(&block);
        self.pop_scope();
        completes
    }

    fn statements(&mut self, block: &ast::Block) -> bool {
        let mut completes = true;
        let mut leaving: Option<Span> = None;
        let mut reported = false;
        for item in block.block_items() {
            let BlockItem::Stmt(stmt) = item else {
                continue;
            };
            if !completes && !reported {
                reported = true;
                self.unreachable(&stmt, leaving.clone());
            }
            let outer = self.dead;
            self.dead |= !completes;
            let goes_on = self.stmt(&stmt);
            self.dead = outer;
            if completes && !goes_on {
                leaving = Some(line_span(self.path, stmt.syntax()));
            }
            completes = completes && goes_on;
        }
        completes
    }

    fn unreachable(&mut self, stmt: &Stmt, leaving: Option<Span>) {
        let span = line_span(self.path, stmt.syntax());
        let mut problem = Diagnostic::new(&rule::SEMANTIC_UNREACHABLE_CODE, "This code never runs")
            .at(span, "never reached");
        if let Some(leaving) = leaving {
            problem = problem.also(leaving, "leaves on every path");
        }
        self.out.push(
            problem
                .reason("The statement before it ends in `return` or `reject` on every path, so nothing after it in this block can run.")
                .next("Remove it, or move it before the statement that leaves"),
        );
    }

    fn missing_return(&mut self, word: &str, body: &ast::Block) {
        let result = self.result.clone();
        let span = body
            .r_brace_token()
            .map(|brace| {
                let range = brace.text_range();
                Span::new(self.path, range.start().into(), range.end().into())
            })
            .unwrap_or_else(|| self.span(body));
        self.out.push(
            Diagnostic::new(
                &rule::SEMANTIC_MISSING_RETURN,
                format!("This {word} can end without returning a `{result}`"),
            )
            .at(span, "reached without `return`")
            .reason(format!(
                "Every path through a {word} with a result ends in `return` or `reject`; at least one path here reaches the end of the body."
            ))
            .next(format!(
                "Return a `{result}` at the end, or on every branch that reaches it"
            )),
        );
    }

    /// Checks a statement; whether the code after it can run.
    fn stmt(&mut self, stmt: &Stmt) -> bool {
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
                let kind = if var.mut_kw_token().is_some() {
                    LocalKind::VarMut
                } else {
                    LocalKind::Var
                };
                self.declare(var.name(), ty, kind, None);
                true
            }
            Stmt::AssignStmt(assign) => {
                self.assign(assign);
                true
            }
            Stmt::ReturnStmt(ret) => {
                self.return_stmt(ret);
                false
            }
            Stmt::IfStmt(branch) => self.if_stmt(branch),
            Stmt::ForStmt(each) => {
                let item = match each.expr() {
                    Some(expr) => self.loop_item(&expr),
                    None => Ty::Error,
                };
                self.scopes.push(Vec::new());
                self.declare(each.name(), item, LocalKind::Item, None);
                self.block(each.block());
                self.pop_scope();
                true
            }
            Stmt::EmitStmt(emit) => {
                if let Some(expr) = emit.expr() {
                    self.infer(&expr);
                }
                true
            }
            Stmt::AssertStmt(assert) => {
                // `assert complete(id) fails TodoNotFound` expects the call
                // to fail (area 13): it is handled there.
                let handled = match (assert.expr(), assert.assert_fails()) {
                    (Some(Expr::CallExpr(call)), Some(_)) => Some(key(call.syntax())),
                    _ => None,
                };
                self.handled.extend(handled);
                if let Some(expr) = assert.expr() {
                    self.infer(&expr);
                }
                if handled.is_some() {
                    self.handled.pop();
                }
                true
            }
            Stmt::ExprStmt(expr) => match expr.expr() {
                Some(expr) => self.standalone(&expr),
                None => true,
            },
        }
    }

    fn return_stmt(&mut self, ret: &ast::ReturnStmt) {
        let expected = self.result.clone();
        match ret.expr() {
            Some(value) => self.check(&value, &expected),
            None if !expected.has_error() && expected != Ty::Prim(Prim::Unit) => {
                let span = self.span(ret);
                self.out.push(
                    Diagnostic::new(
                        &rule::SEMANTIC_MISSING_RETURN,
                        format!("This `return` gives no `{expected}`"),
                    )
                    .at(span, "no value")
                    .reason(format!(
                        "A bare `return` leaves a callable whose result is `Unit`; this one returns a `{expected}`."
                    ))
                    .next(format!("Write the `{expected}` to return after `return`")),
                );
            }
            None => {}
        }
    }

    fn if_stmt(&mut self, branch: &ast::IfStmt) -> bool {
        if let Some(condition) = branch.condition().and_then(|c| c.expr()) {
            self.condition(&condition);
        }
        let then = self.block(branch.block());
        let otherwise = match branch.else_branch() {
            Some(otherwise) => match otherwise.if_stmt() {
                Some(next) => self.if_stmt(&next),
                None => self.block(otherwise.block()),
            },
            None => true,
        };
        then || otherwise
    }

    /// A condition: a `Bool`, or a type built on `Bool`, never `none`.
    fn condition(&mut self, expr: &Expr) {
        if is_literal(expr) {
            self.check(expr, &Ty::Prim(Prim::Bool));
            return;
        }
        let ty = self.infer(expr);
        if ty.has_error() {
            return;
        }
        let nullable = self.table.is_nullable(&ty);
        if self.table.representation(&ty) == Some(Prim::Bool) && !nullable {
            return;
        }
        let span = self.span(expr);
        let written = tree::written(expr.syntax());
        let problem = Diagnostic::new(
            &rule::TYPE_MISMATCH,
            format!("`{ty}` can't be used as a condition"),
        )
        .at(span, format!("this is `{ty}`"));
        self.out.push(if nullable {
            problem
                .reason(format!(
                    "A condition is a `Bool` that is never `none`; `{written}` may be `none`."
                ))
                .next("Use `match` to handle `none` first")
        } else {
            problem
                .reason(
                    "A condition is a `Bool`, or a type built on `Bool`; Jadpo has no truthiness.",
                )
                .next(format!("Compare `{written}` to get a `Bool`"))
        });
    }

    /// The type of a `for` loop's item: the list's element type.
    fn loop_item(&mut self, expr: &Expr) -> Ty {
        let items = self.infer(expr);
        if items.has_error() {
            return Ty::Error;
        }
        if !self.table.is_nullable(&items)
            && let Ty::List(item) = self.table.structure(&items)
        {
            return *item;
        }
        let span = self.span(expr);
        let written = tree::written(expr.syntax());
        let problem = Diagnostic::new(
            &rule::TYPE_MISMATCH,
            format!("`{items}` can't be looped over"),
        )
        .at(span, format!("this is `{items}`"));
        self.out.push(if self.table.is_nullable(&items) {
            problem
                .reason(format!(
                    "`for` loops over a `List`, and `{written}` may be `none`."
                ))
                .next("Use `match` to handle `none` before the loop")
        } else {
            problem
                .reason("`for` loops over a `List`, the only loop in Jadpo.")
                .next(format!("Loop over a `List` instead of `{written}`"))
        });
        Ty::Error
    }

    fn assign(&mut self, assign: &ast::AssignStmt) {
        let value = assign.value();
        match assign.target() {
            Some(Expr::NameRef(name)) => {
                let written = text(&name);
                match self.find(&written).cloned() {
                    Some(local) if local.kind == LocalKind::VarMut => {
                        if let Some(value) = value {
                            self.check(&value, &local.ty);
                        }
                    }
                    Some(local) => {
                        if let Some(value) = &value {
                            self.check(value, &local.ty);
                        }
                        self.assign_immutable(&name, &local);
                    }
                    None => {
                        let ty = value.as_ref().map_or(Ty::Error, |value| self.infer(value));
                        self.assign_unknown(&name, &ty);
                    }
                }
            }
            Some(target @ Expr::FieldExpr(_)) => {
                let receiver = self.infer(&target);
                if let Some(value) = &value {
                    self.infer(value);
                }
                if receiver.has_error() {
                    return;
                }
                let span = self.span(&target);
                let written = tree::written(target.syntax());
                self.out.push(
                    Diagnostic::new(
                        &rule::TYPE_ASSIGN_IMMUTABLE,
                        format!("`{written}` can't be changed"),
                    )
                    .at(span, "a field")
                    .reason("Values never change: a field is given when its value is built, and a changed value is a new one.")
                    .next("Build a new value with the field changed, in a `var mut` if it has to replace the old one"),
                );
            }
            Some(other) => {
                self.infer(&other);
                if let Some(value) = &value {
                    self.infer(value);
                }
            }
            None => {
                if let Some(value) = &value {
                    self.infer(value);
                }
            }
        }
    }

    fn assign_immutable(&mut self, name: &ast::NameRef, local: &Local) {
        let written = &local.name;
        let span = self.span(name);
        let problem = Diagnostic::new(
            &rule::TYPE_ASSIGN_IMMUTABLE,
            format!("`{written}` can't be changed"),
        )
        .at(span, "assigned here");
        self.out.push(match local.kind {
            LocalKind::Var => {
                let insert = Span::new(self.path, local.span.start, local.span.start);
                problem
                    .also(local.span.clone(), "declared without `mut`")
                    .reason("`var` names a value that never changes; only a `var mut` can be pointed at a new value.")
                    .next(format!("Declare it `var mut {written}`"))
                    .edit(insert, "mut ")
            }
            LocalKind::Param => problem
                .also(local.span.clone(), "a parameter")
                .reason("Parameters never change; only a `var mut` can be pointed at a new value.")
                .next(format!("Copy `{written}` into a `var mut` with a name of its own, and change that")),
            LocalKind::Item => problem
                .also(local.span.clone(), "the loop's item")
                .reason("A loop's item never changes; only a `var mut` can be pointed at a new value.")
                .next("Declare a `var mut` before the loop, and change that"),
            _ => problem
                .also(local.span.clone(), "bound here")
                .reason("A name bound by a pattern never changes; only a `var mut` can be pointed at a new value.")
                .next(format!("Copy `{written}` into a `var mut` with a name of its own, and change that")),
        });
    }

    fn assign_unknown(&mut self, name: &ast::NameRef, ty: &Ty) {
        self.unknown_name = true;
        let written = text(name);
        let span = self.span(name);
        let changeable: Vec<String> = self
            .scopes
            .iter()
            .flatten()
            .filter(|local| local.kind == LocalKind::VarMut)
            .map(|local| local.name.clone())
            .collect();
        let near = jadpo_diag::nearest(&written, changeable.iter().map(String::as_str))
            .map(str::to_string);
        let problem = Diagnostic::new(
            &rule::TYPE_ASSIGN_UNKNOWN,
            format!("`{written}` isn't declared"),
        )
        .at(span.clone(), "not declared")
        .reason(format!(
            "An assignment changes a `var mut` declared earlier in this block or one around it.{}",
            did_you_mean(near.as_deref())
        ));
        self.out.push(match near {
            Some(near) => problem
                .next(format!("Use `{near}` instead of `{written}`"))
                .edit(span, near),
            None if !ty.has_error() => problem.next(format!(
                "Declare it first, as in `var mut {written}: {ty} = ...`"
            )),
            None => problem.next(format!("Declare `{written}` first with `var mut`")),
        });
    }

    /// An expression standing as a statement: a call whose result is
    /// `Unit`, `attempt`, `reject` or a `match` (area 3, question 4).
    fn standalone(&mut self, expr: &Expr) -> bool {
        match expr {
            Expr::RejectExpr(reject) => {
                self.reject(reject);
                false
            }
            Expr::PropagateExpr(propagate) => {
                self.propagate(propagate);
                false
            }
            Expr::AttemptExpr(attempt) => {
                self.attempt(attempt, None);
                true
            }
            Expr::MatchExpr(subject) => self.match_stmt(subject),
            Expr::ElisionExpr(_) => true,
            Expr::CallExpr(_) | Expr::RecordExpr(_) => {
                self.last_call = CallKind::Other;
                let ty = self.infer(expr);
                let kind = std::mem::replace(&mut self.last_call, CallKind::Other);
                if ty.has_error() {
                    return true;
                }
                match kind {
                    CallKind::Effect(_) if ty == Ty::Prim(Prim::Unit) => {}
                    CallKind::Effect(name) => self.unused_result(
                        expr,
                        &ty,
                        format!(
                            "`{name}` gives a `{ty}`; a call stands on its own only when its result is `Unit`."
                        ),
                    ),
                    CallKind::Pure(name) => self.unused_result(
                        expr,
                        &ty,
                        format!(
                            "`{name}` only computes its result, so calling it without using the result does nothing."
                        ),
                    ),
                    CallKind::Other => self.unused_result(
                        expr,
                        &ty,
                        "A value standing on its own does nothing.".to_string(),
                    ),
                }
                true
            }
            _ => {
                let ty = self.infer(expr);
                if !ty.has_error() {
                    self.unused_result(
                        expr,
                        &ty,
                        "A value standing on its own does nothing.".to_string(),
                    );
                }
                true
            }
        }
    }

    fn unused_result(&mut self, expr: &Expr, ty: &Ty, reason: String) {
        let span = line_span(self.path, expr.syntax());
        let next = if *ty == Ty::Prim(Prim::Unit) {
            "Remove it".to_string()
        } else {
            "Use the value, or remove it".to_string()
        };
        self.out.push(
            Diagnostic::new(
                &rule::SEMANTIC_UNUSED_RESULT,
                format!("This `{ty}` is thrown away"),
            )
            .at(span, "result not used")
            .reason(reason)
            .next(next),
        );
    }

    /// A block a later area checks (an entity write's `set:`): its values
    /// are read, and the write is that area's to check.
    fn failure_value(&mut self, expr: &Expr) {
        match expr {
            Expr::RecordExpr(record) => self.member_values(record.member_list()),
            Expr::NameRef(_) => {}
            other => {
                self.infer(other);
            }
        }
    }

    fn member_values(&mut self, list: Option<ast::MemberList>) {
        for item in list.iter().flat_map(|list| list.member_items()) {
            if let ast::MemberItem::Member(member) = item {
                if let Some(value) = member.value() {
                    self.infer(&value);
                }
                self.member_values(member.block());
            }
        }
    }
}
