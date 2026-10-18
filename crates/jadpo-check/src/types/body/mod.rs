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

mod across;
pub(crate) mod backfill;
mod calls;
mod contract;
mod currency;
mod expr;
pub(crate) mod facts;
mod failing;
mod identity;
mod json;
mod known;
mod lower;
mod matching;
mod numbers;
mod ops;
mod outcome;
mod prompt;
mod quantities;
mod ranges;
mod request;
mod standard_rules;
mod temporal;
mod text;
mod translation;
mod writes;

use std::collections::{BTreeMap, BTreeSet};

use jadpo_db::{Db, File, ItemKind, Ownership, Project, item_tree, location};
use jadpo_diag::{Diagnostic, Span, rule};
use jadpo_graph::Key;
use jadpo_syntax::SyntaxKind;
use jadpo_syntax::ast::{self, AstNode, BlockItem, Decl, Expr, Stmt};

use super::decl::Decls;
use super::table::TypeView;
use super::ty::{Prim, Ty, TypeNode, TypeRef};
use crate::lookups::{FailureView, Lookup, NameView, SigView, WriteView};
use crate::resolve::{Names, Walk, Want};
use crate::std_lib::StdNames;
use crate::tree::{self, node_span};

pub(crate) use expr::{members_of, range_unordered};
use facts::{Facts, LocalFact};
pub(crate) use facts::{NodeKey, node_key};
pub use failing::Fallible;
pub(crate) use lower::transition_key;

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

/// A callable with a body: a function, action, fixture, test or seed, by
/// its declaration's position among its file's declarations and, for a
/// library's function, its position in the library. Positions among
/// declarations rather than bytes, so an edit inside a body keeps every
/// callable's identity.
#[salsa::interned(debug)]
pub struct Callable<'db> {
    #[returns(copy)]
    pub file: File,
    #[returns(copy)]
    pub index: u32,
    #[returns(copy)]
    pub function: Option<u32>,
}

/// Everything checking one callable finds.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Checked {
    pub problems: Vec<Diagnostic>,
    /// Errors saying a call to another callable can't fail, by that
    /// callable's name as [`FailureSets`] names it. They hold only when
    /// the callee's body lets out nothing its `fails` list leaves out, so
    /// [`call_problems`] reports them after reading every callee.
    pub deferred: Vec<(String, Diagnostic)>,
    /// Its body lets out a failure its `fails` list doesn't name.
    pub undeclared: bool,
    /// For a function, action or library function: its name
    /// (`library.function` for a library's) and the failures that can
    /// leave it.
    pub set: Option<(String, FailureSet)>,
    /// Its key in the graph.
    pub key: Option<Key>,
    /// Its typed body (docs/compiler-runtime.md §2.3), which the graph's
    /// edges, sites and branches, usage, recursion, the language server and
    /// both executors read; shared behind `Arc`, so nothing copies it.
    pub body: std::sync::Arc<jadpo_graph::body::Body>,
    /// What the editor's completion reads of it (area 14, "Completion").
    pub(crate) view: std::sync::Arc<BodyView>,
}

/// What the editor's completion reads of a checked body (area 14,
/// "Completion"): the type the checker gave each expression, the type each
/// position required, and every local with where its name is written, so
/// the editor offers what the check itself decided.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct BodyView {
    pub types: BTreeMap<NodeKey, Ty>,
    pub expected: BTreeMap<NodeKey, Ty>,
    pub locals: Vec<ViewLocal>,
}

/// A local as completion offers it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ViewLocal {
    pub name: String,
    pub ty: Ty,
    pub kind: LocalKind,
    /// Where its name is written.
    pub start: u32,
    pub end: u32,
}

/// The callables of `file` that have bodies, in source order.
pub fn callables_of<'db>(db: &'db dyn Db, file: File) -> Vec<Callable<'db>> {
    let mut out = Vec::new();
    for item in &item_tree(db, file).items {
        match item.kind {
            ItemKind::Function
            | ItemKind::Constant
            | ItemKind::Action
            | ItemKind::Fixture
            | ItemKind::Test
            | ItemKind::Seed => out.push(Callable::new(db, file, item.index, None)),
            ItemKind::Library => {
                for function in &item.functions {
                    out.push(Callable::new(db, file, item.index, Some(function.index)));
                }
            }
            _ => {}
        }
    }
    out
}

/// The computed backfills the latest migration's `changes.jadpo` holds,
/// each checked as a callable is: `index` is its `migration` among the
/// file's declarations and `function` its decision's place among the
/// migration's (area 5, "Computed backfills"; chunk 5.6b). An earlier
/// migration's backfills ran with the code they were written against, so
/// only the latest's are read.
pub fn backfill_callables<'db>(db: &'db dyn Db, project: Project, file: File) -> Vec<Callable<'db>> {
    let path = file.path(db);
    if !path.ends_with("/changes.jadpo") {
        return Vec::new();
    }
    let latest = crate::data::migrations::migrations(db, project)
        .list
        .last()
        .map(|migration| migration.changes_path());
    if latest.as_deref() != Some(path.as_str()) {
        return Vec::new();
    }
    let Some(source) = tree::source(db, file) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (index, decl) in source.decls().enumerate() {
        let Decl::SingletonDecl(migration) = decl else {
            continue;
        };
        let count = migration
            .member_list()
            .map_or(0, |list| list.member_items().count());
        for member in 0..count {
            if let Some((_, _, value)) = crate::data::migrations::backfill_member(&migration, member)
                && matches!(
                    super::rules::Const::from_expr(&value),
                    super::rules::Const::Other | super::rules::Const::Path(_)
                )
            {
                out.push(Callable::new(db, file, index as u32, Some(member as u32)));
            }
        }
    }
    out
}

/// The rules across fields `file`'s object types name, each checked as a
/// callable is: `index` is its type's declaration and `function` its place
/// among the type's rules (area 2, "Rules across fields").
pub fn rule_callables<'db>(db: &'db dyn Db, file: File) -> Vec<Callable<'db>> {
    let Some(source) = tree::source(db, file) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (index, decl) in source.decls().enumerate() {
        let Decl::TypeDecl(decl) = decl else { continue };
        let base = decl
            .base()
            .map(|base| tree::written(base.syntax()))
            .unwrap_or_default();
        if !super::shape::FieldsBase::from_base(&base).is_some_and(super::across::has_rules) {
            continue;
        }
        for rule in 0..super::across::rule_members(decl.member_list()).len() {
            out.push(Callable::new(db, file, index as u32, Some(rule as u32)));
        }
    }
    out
}

/// Checks one callable: its diagnostics, its failure set and its typed
/// body, from one walk. It reads its own file and, through the per-name
/// lookups, only the declarations it names, so an edit elsewhere checks it
/// again only when something it reads changed.
#[salsa::tracked(returns(ref))]
pub fn typed_body<'db>(db: &'db dyn Db, project: Project, callable: Callable<'db>) -> Checked {
    let file = callable.file(db);
    let Some(source) = tree::source(db, file) else {
        return Checked::default();
    };
    let Some(decl) = tree::decl_at(&source, callable.index(db)) else {
        return Checked::default();
    };
    let lookup = Lookup::for_file(db, project, file);
    let mut checker = Body {
        db,
        project,
        path: file.path(db),
        ownership: &location(db, project, file).ownership,
        table: &lookup,
        calls: &lookup,
        symbols: &lookup,
        failures: &lookup,
        writes: &lookup,
        scopes: Vec::new(),
        result: Ty::Prim(Prim::Unit),
        context: Context::Action,
        test: false,
        word: "function",
        library: None,
        unknown_name: false,
        unused: Vec::new(),
        last_call: CallKind::Other,
        dead: false,
        flow: Flow::default(),
        handled: Vec::new(),
        arm: Arm::None,
        facts: Facts::default(),
        known: known::Known::default(),
        assigned: Vec::new(),
        set: None,
        out: Vec::new(),
        deferred: Vec::new(),
        undeclared: false,
        resolved: Vec::new(),
        loops: 0,
        pending_block: None,
        stored: None,
        retired: Vec::new(),
        action: None,
    };
    let Some(target) = checker.target(&decl, callable.function(db)) else {
        return Checked::default();
    };
    if target.word == "action" {
        checker.action = target.name.clone();
    }
    let key = crate::graph::callable_key(&lookup, file.path(db), &decl, callable.function(db));
    let syntax = target.syntax.clone();
    let rule = target.rule.is_some();
    checker.callable(target);
    let body = lower::lower(&checker, &lookup, file.text(db), &syntax);
    // A rule reads only its type's fields (area 2): anything else is the
    // one mistake in it.
    if rule {
        let outside = checker.outside_fields(&body, file.text(db));
        if !outside.is_empty() {
            checker.out = outside;
            checker.deferred.clear();
            checker.resolved.clear();
        }
    }
    let view = BodyView {
        types: std::mem::take(&mut checker.facts.types),
        expected: std::mem::take(&mut checker.facts.expected),
        locals: checker
            .facts
            .locals
            .iter()
            .map(|local| ViewLocal {
                name: local.name.clone(),
                ty: local.ty.clone(),
                kind: local.kind,
                start: local.span.start,
                end: local.span.end,
            })
            .collect(),
    };
    let mut problems = Vec::with_capacity(checker.out.len());
    let mut deferred = Vec::new();
    for (index, problem) in checker.out.into_iter().enumerate() {
        match checker.deferred.iter().find(|(at, _)| *at == index) {
            Some((_, callee)) => deferred.push((callee.clone(), problem)),
            None => problems.push(problem),
        }
    }
    problems.append(&mut checker.resolved);
    Checked {
        problems,
        deferred,
        undeclared: checker.undeclared,
        set: checker.set,
        key,
        body: std::sync::Arc::new(body),
        view: std::sync::Arc::new(view),
    }
}

/// The problems inside one file's bodies.
#[salsa::tracked(returns(ref))]
pub fn body_problems(db: &dyn Db, project: Project, file: File) -> Vec<Diagnostic> {
    callables_of(db, file)
        .into_iter()
        .chain(rule_callables(db, file))
        // A constant whose value is refused as not fixed has that one
        // error (area 3); what its value would have been isn't checked.
        .filter(|callable| {
            crate::constants::evaluated(
                db,
                project,
                file.path(db),
                callable.index(db),
                callable.function(db),
            )
            .is_none_or(|evaluated| !evaluated.not_fixed)
        })
        .flat_map(|callable| typed_body(db, project, callable).problems.iter().cloned())
        .collect()
}

/// The errors saying a call can't fail, across the project, leaving out
/// the calls to a callable whose body lets out a failure its `fails` list
/// misses: that callable's own `failure.undeclared_propagation` is the one
/// mistake, and its callers' `attempt`s and outcome `match`es are right
/// for what it really does. Callees are named as [`FailureSets`] names
/// them, so a name two domains share counts as one.
#[salsa::tracked(returns(ref))]
pub fn call_problems(db: &dyn Db, project: Project) -> Vec<Diagnostic> {
    let checked: Vec<&Checked> = project
        .files(db)
        .iter()
        .flat_map(|file| callables_of(db, *file))
        .map(|callable| typed_body(db, project, callable))
        .collect();
    let undeclared: std::collections::BTreeSet<&str> = checked
        .iter()
        .filter(|checked| checked.undeclared)
        .filter_map(|checked| checked.set.as_ref().map(|(name, _)| name.as_str()))
        .collect();
    checked
        .iter()
        .flat_map(|checked| checked.deferred.iter())
        .filter(|(callee, _)| !undeclared.contains(callee.as_str()))
        .map(|(_, problem)| problem.clone())
        .collect()
}

/// Which failures can leave one callable: a query of its own, so whatever
/// reads the set is reused when a body edit leaves it equal, though the
/// typed body itself changed.
#[salsa::tracked(returns(ref))]
pub fn failure_set<'db>(
    db: &'db dyn Db,
    project: Project,
    callable: Callable<'db>,
) -> Option<(String, FailureSet)> {
    typed_body(db, project, callable).set.clone()
}

/// Which failures can leave each function and action in one file.
#[salsa::tracked(returns(ref))]
pub fn failure_sets(db: &dyn Db, project: Project, file: File) -> FailureSets {
    let mut sets = FailureSets::default();
    for callable in callables_of(db, file) {
        if let Some((name, set)) = failure_set(db, project, callable) {
            sets.callables
                .entry(name.clone())
                .or_insert_with(|| set.clone());
        }
    }
    sets
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

/// What kind of code is being checked, which decides what it may call
/// (area 3, "Functions and actions").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Context {
    /// A function or library function: pure.
    Function,
    /// An inline function given to a collection operation: pure too.
    Inline,
    /// An action: application code.
    Action,
    /// A test, fixture or seed: test-only code that acts like an action
    /// and may call fixtures too (area 13).
    Test,
    /// A constant's value (area 3): worked out while compiling, so pure,
    /// and what it may be built from is checked by [`crate::constants`].
    Constant,
    /// A rule's condition across an object's fields (area 2, "Rules across
    /// fields"): pure, and reading only those fields ([`across`]).
    Rule,
    /// A computed backfill in a migration's `changes.jadpo` (area 5,
    /// "Computed backfills"): pure, reading its row's stored fields, and
    /// never able to fail.
    Backfill,
}

impl Context {
    fn is_pure(self) -> bool {
        matches!(
            self,
            Context::Function
                | Context::Inline
                | Context::Constant
                | Context::Rule
                | Context::Backfill
        )
    }

    fn with_a(self) -> &'static str {
        match self {
            Context::Function => "a function",
            Context::Inline => "an inline function",
            Context::Action => "an action",
            Context::Test => "a test",
            Context::Constant => "a constant",
            Context::Rule => "a rule",
            Context::Backfill => "a backfill",
        }
    }
}

/// How a name came into scope.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LocalKind {
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
    /// Its number in the typed body.
    id: usize,
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
    /// The project, for what a body reads only when it needs it: the
    /// identity declarations (area 8).
    db: &'a dyn Db,
    project: Project,
    path: &'a str,
    /// Who owns the file: what it may use (area 1).
    ownership: &'a Ownership,
    table: &'a dyn TypeView,
    calls: &'a dyn SigView,
    symbols: &'a dyn NameView,
    failures: &'a dyn FailureView,
    writes: &'a dyn WriteView,
    scopes: Vec<Vec<Local>>,
    /// The result type of the callable being checked.
    result: Ty,
    context: Context,
    /// Whether the callable is a test, rather than a fixture or seed: it
    /// writes records only through actions and fixtures (area 6).
    test: bool,
    /// What the callable is, as a message names it: `function`, `test`.
    word: &'static str,
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
    /// The calls an `attempt` or an outcome `match` is handling.
    handled: Vec<NodeKey>,
    arm: Arm,
    /// What the walk decided, for the typed body.
    facts: Facts,
    /// What the conditions around the code being checked say about the
    /// numbers they compare (area 2, "Conditions as proof").
    known: known::Known,
    /// Each `var mut` assigned so far, in order, so a block forgets what
    /// it knew of the ones assigned inside it.
    assigned: Vec<usize>,
    /// The callable's name and failure set, once checked.
    set: Option<(String, FailureSet)>,
    out: Vec<Diagnostic>,
    /// Which of `out` say a call can't fail, with the callee's name: see
    /// [`Checked::deferred`].
    deferred: Vec<(usize, String)>,
    /// See [`Checked::undeclared`].
    undeclared: bool,
    /// What resolving the body's names finds (area 1), kept apart from
    /// `out`, whose length tells the checks what is already reported, so
    /// a name error never changes what the type checks decide.
    resolved: Vec<Diagnostic>,
    /// How many `for` loops the code being checked is inside: a read there
    /// reads once per item (area 6).
    loops: u32,
    /// The block after an `update` or `delete` being checked, which an
    /// `attempt` or a block-built value hands it (area 6).
    pending_block: Option<ast::MemberList>,
    /// While an update by `id` of this entity checks its values: the
    /// entity whose stored fields they may read (area 6, "Counters").
    stored: Option<String>,
    /// While a computed backfill is checked (area 5, chunk 5.6b): the
    /// fields of `stored`'s entity this migration retires, with their
    /// types, which the backfill may read though the entity no longer
    /// declares them.
    retired: Vec<(String, Ty)>,
    /// The action being checked, by name: whether root `policy {
    /// actions: }` lists it decides which fields its updates may change
    /// (area 9).
    action: Option<String>,
}

/// One callable to check.
struct Target {
    /// Its declaration, or its function in a library.
    syntax: jadpo_syntax::SyntaxNode,
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
    /// A constant's type and value (area 3): its value is checked as a
    /// `return` of its type would be.
    constant: Option<(Option<ast::Type>, Option<ast::Expr>)>,
    /// A rule across an object's fields (area 2): its type's name and its
    /// condition, read with the type's fields in scope.
    rule: Option<(String, Option<ast::Expr>, Option<ast::MemberList>)>,
    /// A computed backfill (area 5, chunk 5.6b).
    backfill: Option<BackfillTarget>,
}

/// A computed backfill to check: the field it fills, its type, its value,
/// and the fields this migration retires that it may read.
struct BackfillTarget {
    entity: String,
    ty: Ty,
    value: ast::Expr,
    retired: Vec<(String, Ty)>,
}

fn text(name: &ast::NameRef) -> String {
    tree::name_ref_text(name).unwrap_or_default()
}

fn did_you_mean(near: Option<&str>) -> String {
    near.map_or_else(String::new, |near| format!(" Did you mean `{near}`?"))
}

/// "`Status`'s variants are `open` and `done`.", or a few of them for a
/// long enum such as `Zone`.
fn variants_line(host: &str, names: &[String]) -> String {
    let listed: Vec<String> = names.iter().map(|n| format!("`{n}`")).collect();
    if listed.len() <= 12 {
        return format!("`{host}`'s variants are {}.", crate::and_list(&listed));
    }
    format!(
        "`{host}` has {} variants, such as {}.",
        listed.len(),
        crate::and_list(&listed[..3])
    )
}

fn capital(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// Whether an expression is a name starting with a capital: a type, an
/// enum or an entity, resolved rather than read as a local.
fn is_type_name(expr: &Expr) -> bool {
    matches!(expr, Expr::NameRef(name) if crate::case::starts_upper(&text(name)))
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

    /// The identity declarations (area 8), read only by a body that names
    /// a caller or a session, so an edit to them checks only those again.
    fn identity(&self) -> std::sync::Arc<crate::identity::Identity> {
        std::sync::Arc::clone(crate::identity::identity(self.db, self.project))
    }

    fn resolve(&self, ty: Option<ast::Type>) -> Ty {
        ty.map_or(Ty::Error, |ty| self.table.resolve(&TypeRef::from_type(&ty)))
    }

    /// What to check for declaration `decl`, or its library's function
    /// `function`; `None` for a declaration without a body.
    fn target(&mut self, decl: &Decl, function: Option<u32>) -> Option<Target> {
        let name = tree::decl_name(decl).as_ref().and_then(tree::name_text);
        let syntax = decl.syntax().clone();
        let target = |context, word, params, fails, contract, result, body| Target {
            syntax: syntax.clone(),
            context,
            word,
            name: name.clone(),
            params,
            fails,
            contract,
            result,
            body,
            constant: None,
            rule: None,
            backfill: None,
        };
        let constant = |syntax: jadpo_syntax::SyntaxNode, name, d: &ast::ConstantDecl| Target {
            syntax,
            context: Context::Constant,
            word: "constant",
            name,
            params: None,
            fails: None,
            contract: false,
            result: None,
            body: None,
            constant: Some((d.ty(), d.value())),
            rule: None,
            backfill: None,
        };
        Some(match (decl, function) {
            (Decl::FunctionDecl(d), None) => target(
                Context::Function,
                "function",
                d.param_list(),
                d.fails_clause(),
                true,
                d.ret_type(),
                d.block(),
            ),
            (Decl::ActionDecl(d), None) => target(
                Context::Action,
                "action",
                d.param_list(),
                d.fails_clause(),
                true,
                d.ret_type(),
                d.block(),
            ),
            (Decl::FixtureDecl(d), None) => target(
                Context::Test,
                "fixture",
                d.param_list(),
                None,
                false,
                d.ret_type(),
                d.block(),
            ),
            (Decl::ConstantDecl(d), None) => constant(syntax.clone(), name.clone(), d),
            (Decl::LibraryDecl(d), Some(index)) => {
                // A library's functions call each other bare.
                self.library = name.clone();
                let member = tree::library_members(d).into_iter().nth(index as usize)?;
                if let Decl::ConstantDecl(c) = &member {
                    let constant_name = c.name().as_ref().and_then(tree::name_text);
                    return Some(constant(
                        c.syntax().clone(),
                        constant_name.map(|c| format!("{}.{c}", name.clone().unwrap_or_default())),
                        c,
                    ));
                }
                let Decl::FunctionDecl(function) = member else {
                    return None;
                };
                let function_name = function.name().as_ref().and_then(tree::name_text);
                Target {
                    syntax: function.syntax().clone(),
                    context: Context::Function,
                    word: "function",
                    name: function_name
                        .map(|f| format!("{}.{f}", name.clone().unwrap_or_default())),
                    params: function.param_list(),
                    fails: function.fails_clause(),
                    contract: true,
                    result: function.ret_type(),
                    body: function.block(),
                    constant: None,
                    rule: None,
                    backfill: None,
                }
            }
            // A rule across an object type's fields (area 2).
            (Decl::TypeDecl(d), Some(index)) => {
                let rule = super::across::rule_members(d.member_list())
                    .into_iter()
                    .nth(index as usize)?;
                let host = name.clone()?;
                let rule_name = rule.name().as_ref().and_then(tree::name_text);
                Target {
                    syntax: rule.syntax().clone(),
                    context: Context::Rule,
                    word: "rule",
                    name: rule_name.map(|r| format!("{host}.{r}")),
                    params: None,
                    fails: None,
                    contract: false,
                    result: None,
                    body: None,
                    constant: None,
                    rule: Some((host, rule.condition(), d.member_list())),
                    backfill: None,
                }
            }
            // A computed backfill in a migration's `changes.jadpo` (area 5,
            // chunk 5.6b): `index` is its decision among the migration's.
            (Decl::SingletonDecl(d), Some(index)) => {
                let (entity, field, value) =
                    crate::data::migrations::backfill_member(d, index as usize)?;
                // Typed as a value set on the field is (area 6): its
                // declared type, or the field's own when it has rules of
                // its own, which a value then meets only by construction.
                let path = std::slice::from_ref(&field);
                let ty = match self.table.field(&entity, path) {
                    Some(def) if def.rules == jadpo_std::Rules::default() => {
                        if def.optional {
                            def.declared.clone().nullable()
                        } else {
                            def.declared.clone()
                        }
                    }
                    _ => self.table.field_type(&entity, path),
                };
                let retired = crate::data::migrations::retired_fields(self.db, self.project, &entity);
                Target {
                    syntax: value.syntax().clone(),
                    context: Context::Backfill,
                    word: "backfill",
                    name: Some(format!("{entity}.{field}")),
                    params: None,
                    fails: None,
                    contract: false,
                    result: None,
                    body: None,
                    constant: None,
                    rule: None,
                    backfill: Some(BackfillTarget {
                        entity,
                        ty,
                        value,
                        retired,
                    }),
                }
            }
            (Decl::TestDecl(d), None) => Target {
                name: None,
                ..target(Context::Test, "test", None, None, false, None, d.block())
            },
            (Decl::SeedDecl(d), None) => Target {
                name: None,
                ..target(Context::Test, "seed", None, None, false, None, d.block())
            },
            _ => return None,
        })
    }

    fn callable(&mut self, callable: Target) {
        let Target {
            syntax: _,
            context,
            word,
            name,
            params,
            fails,
            contract,
            result,
            body,
            constant,
            rule,
            backfill,
        } = callable;
        self.context = context;
        self.test = word == "test";
        self.word = word;
        self.unknown_name = false;
        self.unused.clear();
        self.flow = Flow {
            name: name.clone().unwrap_or_default(),
            ..Flow::default()
        };
        self.arm = Arm::None;
        self.scopes.push(Vec::new());
        if let Some((host, condition, members)) = rule {
            self.rule(&host, condition.as_ref(), members);
            self.pop_scope();
            return;
        }
        if let Some(backfill) = backfill {
            self.backfill(backfill);
            self.pop_scope();
            return;
        }
        for param in params.iter().flat_map(|list| list.params()) {
            let mut ty = self.resolve(param.ty());
            // A built-in representation in a function's or action's
            // signature is already reported (area 2, question 11): what
            // the body does with it follows from that mistake.
            if contract && matches!(ty.strip_nullable(), Ty::Prim(prim) if prim.is_representation())
            {
                ty = Ty::Error;
            }
            self.declare(param.name(), ty, LocalKind::Param, None, "parameter");
        }
        self.result = match (&constant, result) {
            (Some((ty, _)), _) => {
                let ty = self.resolve(ty.clone());
                // A built-in representation is reported where the type is
                // written, as a signature's is (area 2, question 11).
                if matches!(ty.strip_nullable(), Ty::Prim(prim) if prim.is_representation()) {
                    Ty::Error
                } else {
                    ty
                }
            }
            (None, Some(ret)) => self.resolve(ret.ty()),
            (None, None) => Ty::Prim(Prim::Unit),
        };
        if let Some((_, value)) = constant {
            if let Some(value) = value {
                let expected = self.result.clone();
                self.check(&value, &expected);
            }
            self.pop_scope();
            return;
        }
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
        if contract {
            self.set = name.map(|name| {
                (
                    name,
                    FailureSet {
                        failures: self.flow.leaving.keys().cloned().collect(),
                        open: self.flow.open,
                    },
                )
            });
        }
    }

    // --- locals -------------------------------------------------------------

    /// Brings a name into the innermost scope, resolving it as it comes
    /// in (area 1): its case, a reserved word, a name already in scope or
    /// one a domain or library has. The latest declaration wins; `word`
    /// names it in those errors: `parameter` or `variable`.
    fn declare(
        &mut self,
        name: Option<ast::Name>,
        ty: Ty,
        kind: LocalKind,
        binding: Option<Span>,
        word: &str,
    ) {
        let Some(name) = name else { return };
        let Some(text) = tree::name_text(&name) else {
            return;
        };
        let span = self.span(&name);
        let earlier = self.declared_at(&text);
        let mut found = Vec::new();
        let misnamed = self.names().declare(&name, word, earlier, &mut found);
        self.resolved.append(&mut found);
        // A name reported as it comes in is the mistake on this line, as
        // is code that never runs; whether the name is read is not another
        // one.
        let reported = self.dead || misnamed;
        let id = self.facts.locals.len();
        self.facts.locals.push(LocalFact {
            name: text.clone(),
            ty: ty.clone(),
            kind,
            span: span.clone(),
        });
        self.facts.declared.insert(node_key(name.syntax()), id);
        if let Some(scope) = self.scopes.last_mut() {
            scope.push(Local {
                id,
                name: text,
                ty,
                kind,
                span,
                binding,
                used: reported,
            });
        }
    }

    /// Resolving names, for this file and the library being checked.
    fn names(&self) -> Names<'_> {
        Names {
            path: self.path,
            ownership: self.ownership,
            table: self.symbols,
            types: self.table,
            std: StdNames::get(),
            library: self.library.as_deref(),
        }
    }

    /// Where the local `name` in scope is first declared, outermost first.
    fn declared_at(&self, name: &str) -> Option<Span> {
        self.scopes
            .iter()
            .flatten()
            .find(|local| local.name == name)
            .map(|local| local.span.clone())
    }

    /// Resolves a subtree this walk doesn't check, with the locals in
    /// scope here.
    fn walk(&mut self, resolve: impl FnOnce(&mut Walk<'_>)) {
        let names = self.names();
        let outer = |name: &str| self.declared_at(name);
        let mut walk = Walk::new(&names, &outer);
        resolve(&mut walk);
        let mut found = walk.out;
        self.resolved.append(&mut found);
    }

    /// Resolves names with `resolve`, keeping what it reports; whether it
    /// reported nothing.
    fn resolved_by(&mut self, resolve: impl FnOnce(&Names<'_>, &mut Vec<Diagnostic>)) -> bool {
        let mut found = Vec::new();
        resolve(&self.names(), &mut found);
        let clean = found.is_empty();
        self.resolved.append(&mut found);
        clean
    }

    fn find(&self, name: &str) -> Option<&Local> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.iter().rev().find(|local| local.name == name))
    }

    /// The type of the local `name` reads, marking it read.
    fn read(&mut self, name: &ast::NameRef) -> Option<Ty> {
        let written = text(name);
        let local = self
            .scopes
            .iter_mut()
            .rev()
            .find_map(|scope| scope.iter_mut().rev().find(|local| local.name == written))?;
        local.used = true;
        let (id, ty) = (local.id, local.ty.clone());
        self.facts.reads.insert(node_key(name.syntax()), id);
        Some(ty)
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
                format!("Use `{name}` in the loop, or count the list with `list.count`"),
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
    /// run. What it finds out ends with it, and what was known before it
    /// of a `var mut` it assigns is forgotten.
    fn block(&mut self, block: Option<ast::Block>) -> bool {
        let Some(block) = block else { return true };
        let entry = self.known.clone();
        let mark = self.assigned.len();
        self.scopes.push(Vec::new());
        let completes = self.statements(&block);
        self.pop_scope();
        let mut known = entry;
        for local in &self.assigned[mark..] {
            known.forget(*local);
        }
        self.known = known;
        completes
    }

    /// One way through an `if`, checked knowing `entry`; whether the code
    /// after it can run, and what is known at its end.
    fn branch(&mut self, block: Option<ast::Block>, entry: known::Known) -> (bool, known::Known) {
        self.known = entry;
        let Some(block) = block else {
            return (true, self.known.clone());
        };
        self.scopes.push(Vec::new());
        let completes = self.statements(&block);
        let end = self.known.clone();
        self.pop_scope();
        (completes, end)
    }

    fn statements(&mut self, block: &ast::Block) -> bool {
        let mut completes = true;
        let mut leaving: Option<Span> = None;
        let mut reported = false;
        for item in block.block_items() {
            let stmt = match item {
                BlockItem::Stmt(stmt) => stmt,
                // A member is a later area's setting (an entity write's
                // block, a test's settings): its names are resolved.
                BlockItem::Member(member) => {
                    self.walk(|walk| walk.member(&member));
                    continue;
                }
                BlockItem::Elision(_) => continue,
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
                self.resolved_by(|names, out| names.ty(var.ty(), Want::Type, out));
                let declared = var.ty().map(|ty| {
                    let mut decls = Decls {
                        path: self.path,
                        table: self.table,
                        out: Vec::new(),
                        headers: Vec::new(),
                        failure: false,
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
                    // `var response: Response = request { ... }`: the response
                    // carries the route's output type (area 13).
                    (Some(Ty::Opaque(name, args)), Some(value))
                        if name == request::RESPONSE && args.is_empty() =>
                    {
                        let actual = self.infer(&value);
                        match &actual {
                            Ty::Opaque(found, _) if *found == name => actual,
                            _ if actual.has_error() => Ty::Error,
                            _ => {
                                let declared = Ty::Opaque(name, args);
                                self.mismatch(&value, &actual, &declared);
                                declared
                            }
                        }
                    }
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
                self.declare(var.name(), ty, kind, None, "variable");
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
                // A later pass through the loop sees what an earlier one
                // assigned: every `var mut` it assigns loses its bounds
                // before it, and so after it.
                if let Some(block) = each.block() {
                    for local in self.assigned_in(&block) {
                        self.forget(local);
                    }
                }
                self.scopes.push(Vec::new());
                self.declare(each.name(), item, LocalKind::Item, None, "variable");
                self.loops += 1;
                self.block(each.block());
                self.loops -= 1;
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
                self.resolved_by(|names, out| {
                    names.ty(
                        assert.assert_fails().and_then(|fails| fails.ty()),
                        Want::Failure,
                        out,
                    );
                });
                // `assert complete(id) fails TodoNotFound` expects the call
                // to fail (area 13): it is handled there.
                let handled = match (assert.expr(), assert.assert_fails()) {
                    (Some(Expr::CallExpr(call)), Some(_)) => Some(node_key(call.syntax())),
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

    /// An `if` with its `else if` branches, one after another, and its
    /// `else`; whether the code after it can run. Each branch knows its
    /// condition holds and every earlier one doesn't; the code after it
    /// knows what holds at the end of every branch that reaches it, and,
    /// with no `else`, that every condition is false.
    fn if_stmt(&mut self, branch: &ast::IfStmt) -> bool {
        let before = self.known.clone();
        let arms = std::iter::once((branch.condition(), branch.block())).chain(
            branch
                .else_ifs()
                .map(|next| (next.condition(), next.block())),
        );
        // What holds where every condition so far is false.
        let mut rest = before.clone();
        let mut ends = Vec::new();
        let mut completes = false;
        for (condition, block) in arms {
            self.known = rest.clone();
            let (holds, fails) = match condition.and_then(|c| c.expr()) {
                Some(condition) => {
                    self.condition(&condition);
                    self.split(&condition)
                }
                None => Default::default(),
            };
            let (goes_on, end) = self.branch(block, rest.and(&holds));
            if goes_on {
                ends.push(end);
            }
            completes |= goes_on;
            rest = rest.and(&fails);
        }
        match branch.else_branch() {
            Some(otherwise) => {
                let (goes_on, end) = self.branch(otherwise.block(), rest);
                if goes_on {
                    ends.push(end);
                }
                completes |= goes_on;
            }
            None => {
                ends.push(rest);
                completes = true;
            }
        }
        let mut after = known::Known::any(ends).unwrap_or(before);
        after.keep_within(&self.in_scope());
        self.known = after;
        completes
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
        // A list in its order, a set in the canonical order (area 2, "Set
        // order"; area 19, question 8).
        if !self.table.is_nullable(&items)
            && let Ty::List(item) | Ty::Set(item) = self.table.structure(&items)
        {
            return *item;
        }
        let span = self.span(expr);
        let written = tree::written(expr.syntax());
        let problem = Diagnostic::new(
            &rule::TYPE_MISMATCH,
            format!("`{items}` can't be looped over"),
        )
        .at(span.clone(), format!("this is `{items}`"));
        self.out.push(if self.table.is_nullable(&items) {
            problem
                .reason(format!(
                    "`for` loops over a `List` or a `Set`, and `{written}` may be `none`."
                ))
                .next("Use `match` to handle `none` before the loop")
        } else if let Ty::Map(key, _) = self.table.structure(&items) {
            // A map is walked through its keys, a set (area 19, "Maps").
            let keys = format!("map.keys({written})");
            problem
                .reason(format!(
                    "`for` loops over a `List` or a `Set`; a map is walked through its keys, a `Set<{key}>`, and each key's value read with `map.get`."
                ))
                .next(format!("Loop over `{keys}`"))
                .edit(span, keys)
        } else {
            problem
                .reason("`for` loops over a `List` or a `Set`, the only loop in Jadpo.")
                .next(format!("Loop over a `List` instead of `{written}`"))
        });
        Ty::Error
    }

    fn assign(&mut self, assign: &ast::AssignStmt) {
        self.assign_checked(assign);
        // The local now holds a new value: what was known of the old one
        // no longer holds (area 2, "Conditions as proof").
        if let Some(Expr::NameRef(name)) = assign.target()
            && let Some(local) = self.find(&text(&name))
        {
            let id = local.id;
            self.forget(id);
        }
    }

    fn assign_checked(&mut self, assign: &ast::AssignStmt) {
        let value = assign.value();
        if let Some(target) = assign.target().filter(is_type_name) {
            self.resolved_by(|names, out| names.value_name(&target, out));
        }
        match assign.target() {
            Some(Expr::NameRef(name)) => {
                let written = text(&name);
                if let Some(local) = self.find(&written) {
                    let id = local.id;
                    self.facts.reads.insert(node_key(name.syntax()), id);
                }
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
