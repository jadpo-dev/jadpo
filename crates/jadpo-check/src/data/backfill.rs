//! A computed backfill as SQL (area 5, "Computed backfills"; area 19,
//! "SQL forms"; chunk 5.6b): the expression each existing row gets, written
//! from the backfill's typed body by the one SQL-form writer,
//! `jadpo_sql::form`, so a batch's `UPDATE` gives exactly what the runtime
//! would. A function the backfill calls is written in place, its
//! parameters replaced by its arguments' SQL, and its `if` chains and
//! `match`es become `CASE`. Anything without a form (a loop, a `var mut`,
//! an operation the tables give none, a value in several columns) gives
//! the whole backfill none, and it can't run by `UPDATE`: `jadpo migrate
//! apply` then works it out in Jadpo (chunk 5.6b2), running the body as a
//! function whose parameters are the stored fields it reads
//! ([`as_function`]), and the SQL names the digest of the code it runs
//! ([`fingerprint`]), so a change to that code changes the migration.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use jadpo_db::{Db, Project};
use jadpo_diag::{Diagnostic, Span, rule};
use jadpo_graph::body::{
    ArmBody, BinaryOp, Body, Callee, Expr, Literal, Local, LocalId, LocalKind, Node, Pattern,
    SiteId, Stmt, UnaryOp,
};
use jadpo_graph::{Base, Detail, Graph, Key, TypeRef};
use jadpo_sql::form::{self, Arg, Held, Operator};
use jadpo_std::{Builtin, Op, Rounding, SqlType};

/// The row a backfill reads: its table and the column holding each of its
/// fields, by born name, as the snapshot before the migration has them.
#[derive(Debug)]
pub struct Row<'a> {
    pub table: &'a str,
    pub columns: &'a BTreeMap<String, String>,
}

/// The SQL of the backfill whose typed body is `body`, filling a column
/// whose values keep `scale` places when it holds decimals; `None` when
/// something in it has no SQL form.
pub fn sql(
    db: &dyn Db,
    project: Project,
    body: &Body,
    row: &Row<'_>,
    scale: Option<u32>,
) -> Option<String> {
    let graph = Arc::clone(crate::graph::graph(db, project));
    let mut render = Render {
        db,
        project,
        graph: &graph,
        row,
        bodies: None,
        depth: 0,
    };
    let frame = Frame {
        body,
        locals: HashMap::new(),
    };
    let (value, held) = render.block(&frame, &body.block)?;
    // A decimal is stored at its field's places, rounded half to even as
    // the runtime rounds a value made for a type with a fixed scale.
    match (held, scale) {
        (Held::Builtin(builtin), Some(scale)) if builtin.representation() == Builtin::Decimal => {
            form::kept_at_scale(&value, scale)
        }
        _ => Some(value),
    }
}

/// A computed backfill worked out in Jadpo (area 5, "Computed backfills";
/// chunk 5.6b2): what `jadpo migrate apply` runs for each row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InJadpo {
    /// The backfill's body as a function, each stored field it reads a
    /// parameter, in the order of [`InJadpo::reads`].
    pub body: Arc<Body>,
    pub reads: Vec<Read>,
    /// The column type its value is written as, through the storage codec,
    /// and as the SQL names it (`numeric(12,2)`).
    pub write: jadpo_std::SqlType,
    pub sql_type: String,
    /// The places a decimal is stored at, rounded half to even.
    pub scale: Option<u32>,
    /// The field it fills, by its entity's name and its own as they are
    /// now: whose rules each value is kept and checked by (chunk 6.1a).
    pub fills: (String, String),
    /// The digest of the code it runs.
    pub code: String,
}

/// A stored field a backfill worked out in Jadpo reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Read {
    /// Its born name.
    pub field: String,
    /// Its column, as the snapshot before the migration has it.
    pub column: String,
    /// How its column's value is read.
    pub decode: jadpo_std::read::Column,
    /// Its name now, whose rules each value read is held to (chunk 5.9d);
    /// `None` for a field the migration retires, read as its built-in
    /// type alone, whose declaration is gone.
    pub name: Option<String>,
}

/// `body` as a function whose parameters are the stored fields it reads,
/// each once, in the order it first reads them: the function and their
/// born names.
pub fn as_function(body: &Body) -> (Body, Vec<String>) {
    let mut out = body.clone();
    let mut reads: Vec<(String, LocalId)> = Vec::new();
    for at in 0..out.sites.len() {
        let Node::Expr(Expr::Stored { field }) = &out.sites[at].node else {
            continue;
        };
        let name = field
            .written()
            .rsplit('.')
            .next()
            .unwrap_or_default()
            .to_string();
        let local = match reads.iter().find(|(read, _)| *read == name) {
            Some((_, local)) => *local,
            None => {
                let local = LocalId(out.locals.len() as u32);
                out.locals.push(Local {
                    name: name.clone(),
                    ty: out.sites[at].ty.clone(),
                    kind: LocalKind::Param,
                    start: 0,
                    end: 0,
                });
                out.params.push(local);
                reads.push((name, local));
                local
            }
        };
        out.sites[at].node = Node::Expr(Expr::Local(local));
    }
    (out, reads.into_iter().map(|(name, _)| name).collect())
}

/// The digest of the code a backfill worked out in Jadpo runs: its body
/// and every function it reaches, with where each is written left out,
/// the value of every constant they read, every type they name, along
/// its chain and with its fields and variants, rules and scales included
/// (chunk 5.8b), and the toolchain's version, whose runtime works the
/// values out. A change to any of them changes the migration's SQL, as a
/// change to a backfill by `UPDATE` does, so `jadpo check` asks for the
/// migration to be written again before it runs anywhere, and a database
/// that ran the old code refuses the new one as changed, rather than
/// filling some rows one way and the rest another.
pub fn fingerprint(db: &dyn Db, project: Project, body: &Body) -> String {
    let bodies = bodies(db, project);
    let table = crate::constants::constant_table(db, project);
    let graph = crate::graph::graph(db, project);
    let mut text = format!("jadpo {}\n{}", env!("CARGO_PKG_VERSION"), canonical(body));
    let mut seen: Vec<Key> = Vec::new();
    let mut constants: Vec<Key> = Vec::new();
    let mut types: Vec<Key> = Vec::new();
    let mut queue: Vec<Arc<Body>> = vec![Arc::new(body.clone())];
    while let Some(next) = queue.pop() {
        for local in &next.locals {
            types.extend(local.ty.keys().into_iter().cloned());
        }
        for site in &next.sites {
            types.extend(site.ty.keys().into_iter().cloned());
            match &site.node {
                Node::Expr(Expr::Call {
                    callee: Callee::Function(key) | Callee::LibraryFunction(key),
                    ..
                }) if !seen.contains(key) => {
                    seen.push(key.clone());
                    if let Some(found) = bodies.get(key) {
                        queue.push(Arc::clone(found));
                    }
                }
                Node::Expr(Expr::Call {
                    callee: Callee::Construct(ty),
                    ..
                }) => types.extend(ty.keys().into_iter().cloned()),
                Node::Expr(Expr::Constant { constant }) if !constants.contains(constant) => {
                    constants.push(constant.clone());
                }
                _ => {}
            }
        }
    }
    seen.sort();
    for key in &seen {
        text.push_str(&format!("\n{}: ", key.as_str()));
        if let Some(found) = bodies.get(key) {
            text.push_str(&canonical(found));
        }
    }
    constants.sort();
    for key in &constants {
        let name = key.written();
        let value = table
            .values()
            .find(|evaluated| {
                evaluated.name == name || name.ends_with(&format!(".{}", evaluated.name))
            })
            .map(|evaluated| format!("{:?}", evaluated.value));
        text.push_str(&format!("\n{}: {value:?}", key.as_str()));
    }
    for (key, detail) in declarations(graph, types) {
        text.push_str(&format!("\n{}: {detail}", key.as_str()));
    }
    jadpo_graph::migration::section_digest(&text)
}

/// Each type `named` names, the types along its chain, and its fields,
/// variants and rules with the types they name, each with what its
/// declaration says (its base, rules and scale; a field's type), in key
/// order: what a value of one of them is worked out by.
fn declarations(graph: &Graph, named: Vec<Key>) -> BTreeMap<Key, String> {
    let mut out: BTreeMap<Key, String> = BTreeMap::new();
    let mut queue = named;
    while let Some(key) = queue.pop() {
        if out.contains_key(&key) {
            continue;
        }
        let Some(id) = graph.find(&key) else {
            out.insert(key, String::new());
            continue;
        };
        let node = graph.node(id);
        match &node.detail {
            Detail::Type {
                base: Base::Refines(parent),
                ..
            } => queue.extend(parent.keys().into_iter().cloned()),
            Detail::Field { ty, .. } => queue.extend(ty.keys().into_iter().cloned()),
            _ => {}
        }
        queue.extend(
            graph
                .children(id)
                .map(|child| graph.node(child).key.clone()),
        );
        out.insert(key, format!("{:?}", node.detail));
    }
    out
}

/// The error for a backfill of `key` (`Todo.label`) that reads more than
/// its row, `written`, at `span` (area 5, `migration.backfill_outside_row`).
pub fn outside_row(key: &str, written: &str, span: Span, label: &str) -> Diagnostic {
    let entity = key.split('.').next().unwrap_or_default();
    Diagnostic::new(
        &rule::MIGRATION_BACKFILL_OUTSIDE_ROW,
        format!("The backfill for `{key}` can't read `{written}`"),
    )
    .at(span, label)
    .reason(format!(
        "A backfill works each `{entity}` record's value out from that record's own fields, constants and pure functions, so it gives the same value whenever and wherever the migration runs."
    ))
    .next(format!(
        "Work the value out from `{entity}`'s own fields, or a constant"
    ))
}

/// What a backfill reads through the functions it calls that `jadpo
/// migrate apply` has none of (chunk 5.8b): for each call in its own body
/// whose function, or one that function calls, reads a setting, a key a
/// `crypto` operation takes included, the call, the function as code
/// writes it, and the first setting found (`config.notes_key`).
pub fn settings_reached(
    db: &dyn Db,
    project: Project,
    body: &Body,
) -> Vec<(SiteId, String, String)> {
    let mut out = Vec::new();
    let mut bodies: Option<HashMap<Key, Arc<Body>>> = None;
    for (at, site) in body.sites.iter().enumerate() {
        let Node::Expr(Expr::Call {
            callee: Callee::Function(key) | Callee::LibraryFunction(key),
            ..
        }) = &site.node
        else {
            continue;
        };
        let bodies = bodies.get_or_insert_with(|| self::bodies(db, project));
        if let Some(setting) = setting_in(bodies, key) {
            out.push((SiteId(at as u32), key.written().to_string(), setting));
        }
    }
    out
}

/// The first setting the function with key `start` reads, itself or
/// through a function it calls.
fn setting_in(bodies: &HashMap<Key, Arc<Body>>, start: &Key) -> Option<String> {
    let mut seen = vec![start.clone()];
    let mut queue = vec![start.clone()];
    while let Some(key) = queue.pop() {
        let Some(body) = bodies.get(&key) else {
            continue;
        };
        for site in &body.sites {
            match &site.node {
                Node::Expr(Expr::Config { field }) => {
                    let name = field.name().rsplit('.').next().unwrap_or_default();
                    return Some(format!("config.{name}"));
                }
                Node::Expr(Expr::Call {
                    callee: Callee::Function(next) | Callee::LibraryFunction(next),
                    ..
                }) if !seen.contains(next) => {
                    seen.push(next.clone());
                    queue.push(next.clone());
                }
                _ => {}
            }
        }
    }
    None
}

/// A body as text, without where anything in it is written.
fn canonical(body: &Body) -> String {
    let mut body = body.clone();
    for site in &mut body.sites {
        (site.start, site.end, site.line) = (0, 0, 0);
    }
    for local in &mut body.locals {
        (local.start, local.end) = (0, 0);
    }
    for branch in &mut body.branches {
        (branch.start, branch.end, branch.line) = (0, 0, 0);
    }
    body.names.clear();
    format!("{body:?}")
}

/// Every callable's typed body, by key.
fn bodies(db: &dyn Db, project: Project) -> HashMap<Key, Arc<Body>> {
    let mut out = HashMap::new();
    for &file in project.files(db) {
        for callable in crate::types::callables_of(db, file) {
            let checked = crate::types::typed_body(db, project, callable);
            if let Some(key) = &checked.key {
                out.insert(key.clone(), Arc::clone(&checked.body));
            }
        }
    }
    out
}

/// How deep functions may call functions before a backfill has no form.
const DEPTH: u32 = 16;

struct Render<'a> {
    db: &'a dyn Db,
    project: Project,
    graph: &'a Graph,
    row: &'a Row<'a>,
    /// Every callable's typed body by key, read once a call needs one.
    bodies: Option<HashMap<Key, Arc<Body>>>,
    depth: u32,
}

/// A body being written, with each local it binds as SQL.
struct Frame<'b> {
    body: &'b Body,
    locals: HashMap<LocalId, (String, Held)>,
}

impl Render<'_> {
    /// Statements that end in a `return`, as one expression.
    fn block(&mut self, frame: &Frame<'_>, statements: &[SiteId]) -> Option<(String, Held)> {
        let (first, rest) = statements.split_first()?;
        match &frame.body.site(*first).node {
            Node::Stmt(Stmt::Return { value }) => self.expr(frame, (*value)?),
            Node::Stmt(Stmt::Var { local, value }) => {
                let bound = self.expr(frame, (*value)?)?;
                let mut inner = Frame {
                    body: frame.body,
                    locals: frame.locals.clone(),
                };
                inner.locals.insert((*local)?, bound);
                self.block(&inner, rest)
            }
            Node::Stmt(Stmt::If {
                arms, otherwise, ..
            }) => {
                let mut written = Vec::new();
                let mut held = None;
                for arm in arms {
                    let (condition, _) = self.expr(frame, arm.condition?)?;
                    let then: Vec<SiteId> = arm.then.iter().chain(rest).copied().collect();
                    let (value, h) = self.block(frame, &then)?;
                    held = Some(h);
                    written.push((condition, value));
                }
                let otherwise: Vec<SiteId> = otherwise.iter().chain(rest).copied().collect();
                let (value, h) = self.block(frame, &otherwise)?;
                Some((form::case(&written, &value), held.unwrap_or(h)))
            }
            _ => None,
        }
    }

    fn expr(&mut self, frame: &Frame<'_>, id: SiteId) -> Option<(String, Held)> {
        let site = frame.body.site(id);
        let Node::Expr(expr) = &site.node else {
            return None;
        };
        match expr {
            Expr::Literal(literal) => {
                let held = self.held(&site.ty)?;
                Some((self.literal(literal, &site.ty)?, held))
            }
            Expr::Local(local) => frame.locals.get(local).cloned(),
            Expr::Stored { field } => {
                let held = self.held(&site.ty)?;
                let name = field.written().rsplit('.').next()?;
                let column = self.row.columns.get(name)?;
                Some((
                    format!(
                        "{}.{}",
                        jadpo_sql::ident(self.row.table),
                        jadpo_sql::ident(column)
                    ),
                    held,
                ))
            }
            Expr::Constant { constant } => self.constant(constant, &site.ty),
            Expr::Build {
                callee: Callee::Variant(key),
                fields,
                ..
            } if fields.is_empty() => Some((variant_text(key)?, Held::Builtin(Builtin::Text))),
            Expr::Binary { op, lhs, rhs } => self.binary(frame, *op, *lhs, *rhs),
            Expr::Unary { op, operand } => {
                let (value, held) = self.expr(frame, *operand)?;
                let op = match op {
                    UnaryOp::Not => Operator::Not,
                    UnaryOp::Negate => Operator::Negate,
                };
                Some((form::operator(op, &[Arg::Value(&value, held)])?, held))
            }
            Expr::Call {
                callee,
                args,
                fallible,
            } => {
                if *fallible != jadpo_graph::body::Fallible::Never {
                    return None;
                }
                self.call(frame, callee, args, &site.ty)
            }
            Expr::Match {
                subject,
                outcome: false,
                arms,
            } => self.choose(frame, (*subject)?, arms),
            _ => None,
        }
    }

    fn binary(
        &mut self,
        frame: &Frame<'_>,
        op: BinaryOp,
        lhs: SiteId,
        rhs: SiteId,
    ) -> Option<(String, Held)> {
        // `x == none` and `x != none`.
        let is_none = |id: SiteId| {
            matches!(
                frame.body.site(id).node,
                Node::Expr(Expr::Literal(Literal::None))
            )
        };
        if matches!(op, BinaryOp::Equal | BinaryOp::NotEqual) && (is_none(lhs) || is_none(rhs)) {
            let other = if is_none(lhs) { rhs } else { lhs };
            let (value, _) = self.expr(frame, other)?;
            return Some((
                form::none_check(&value, op == BinaryOp::NotEqual),
                Held::Builtin(Builtin::Bool),
            ));
        }
        let (a, ha) = self.expr(frame, lhs)?;
        let (b, hb) = self.expr(frame, rhs)?;
        let operator = match op {
            BinaryOp::Or => Operator::Or,
            BinaryOp::And => Operator::And,
            BinaryOp::Equal => Operator::Equal,
            BinaryOp::NotEqual => Operator::NotEqual,
            BinaryOp::Less => Operator::Less,
            BinaryOp::LessEqual => Operator::LessEqual,
            BinaryOp::Greater => Operator::Greater,
            BinaryOp::GreaterEqual => Operator::GreaterEqual,
            BinaryOp::Add => Operator::Add,
            BinaryOp::Subtract => Operator::Subtract,
            BinaryOp::Multiply => Operator::Multiply,
            BinaryOp::Divide => Operator::Divide,
            BinaryOp::Remainder => Operator::Remainder,
            BinaryOp::In | BinaryOp::Matches => return None,
        };
        let sql = form::operator(operator, &[Arg::Value(&a, ha), Arg::Value(&b, hb)])?;
        let held = match operator {
            Operator::Add
            | Operator::Subtract
            | Operator::Multiply
            | Operator::Divide
            | Operator::Remainder => {
                // A whole number meeting a decimal is a decimal.
                match (ha, hb) {
                    (Held::Builtin(x), Held::Builtin(y))
                        if x.representation() == Builtin::Decimal
                            || y.representation() == Builtin::Decimal =>
                    {
                        Held::Builtin(Builtin::Decimal)
                    }
                    _ => ha,
                }
            }
            _ => Held::Builtin(Builtin::Bool),
        };
        Some((sql, held))
    }

    fn call(
        &mut self,
        frame: &Frame<'_>,
        callee: &Callee,
        args: &[Option<SiteId>],
        ty: &TypeRef,
    ) -> Option<(String, Held)> {
        match callee {
            Callee::Std(op) => {
                let mut written: Vec<(Option<String>, Option<Held>, Option<Arg<'static>>)> =
                    Vec::new();
                for (at, arg) in args.iter().enumerate() {
                    let Some(arg) = arg else {
                        written.push((None, None, Some(Arg::Absent)));
                        continue;
                    };
                    if let Some(fixed) = self.written_out(frame, *op, at, *arg) {
                        written.push((None, None, Some(fixed)));
                        continue;
                    }
                    let (sql, held) = self.expr(frame, *arg)?;
                    written.push((Some(sql), Some(held), None));
                }
                let args: Vec<Arg<'_>> = written
                    .iter()
                    .map(|(sql, held, fixed)| match (sql, held, fixed) {
                        (Some(sql), Some(held), _) => Arg::Value(sql, *held),
                        (_, _, Some(fixed)) => *fixed,
                        _ => Arg::Absent,
                    })
                    .collect();
                let value = form::operation(*op, &args)?;
                Some((value.value()?.to_string(), self.held(ty)?))
            }
            // A construction an `if` proves, or a re-check that can't
            // fail: the value as the type keeps it (chunk 5.8b).
            Callee::Construct(target) => {
                let [Some(arg)] = args else { return None };
                let (value, _) = self.expr(frame, *arg)?;
                Some((self.kept(value, target)?, self.held(ty)?))
            }
            Callee::Function(key) | Callee::LibraryFunction(key) => {
                if self.depth >= DEPTH {
                    return None;
                }
                let body = self.body_of(key)?;
                let mut locals = HashMap::new();
                for (param, arg) in body.params.iter().zip(args) {
                    locals.insert(*param, self.expr(frame, (*arg)?)?);
                }
                self.depth += 1;
                let inner = Frame {
                    body: &body,
                    locals,
                };
                let written = self.block(&inner, &body.block);
                self.depth -= 1;
                written
            }
            _ => None,
        }
    }

    /// `value` as a value of `ty` keeps it once built, as the runtime's
    /// construction does (area 2, "A fixed `scale` rounds first"; chunk
    /// 5.8b): a decimal at the type's fixed scale, rounded half to even;
    /// anything else as it is, since a construction a backfill makes
    /// can't fail and only a scale changes the value of a type built on a
    /// built-in one. A standard type, which may change a value as it is
    /// built (`Html` made safe, a coordinate kept as its double), has no
    /// form, so a backfill building one is worked out in Jadpo.
    fn kept(&self, value: String, ty: &TypeRef) -> Option<String> {
        match self.builtin(ty)? {
            (builtin, Some(scale)) if builtin.representation() == Builtin::Decimal => {
                form::kept_at_scale(&value, scale)
            }
            _ => Some(value),
        }
    }

    /// What an operation takes written out rather than as a value: a
    /// rounding mode, `decimal.round`'s `scale:`, `text.pad_start`'s
    /// `fill:`.
    fn written_out(
        &self,
        frame: &Frame<'_>,
        op: Op,
        at: usize,
        arg: SiteId,
    ) -> Option<Arg<'static>> {
        let Node::Expr(expr) = &frame.body.site(arg).node else {
            return None;
        };
        match (op, at, expr) {
            (
                _,
                _,
                Expr::Build {
                    callee: Callee::Variant(key),
                    ..
                },
            ) => {
                let (host, variant) = key.written().rsplit_once('.')?;
                (host == Rounding::NAME)
                    .then(|| rounding(variant))
                    .flatten()
                    .map(Arg::Rounding)
            }
            (Op::DecimalRound, 1, Expr::Literal(Literal::Number { value, .. })) => {
                (value.scale == 0).then_some(())?;
                Some(Arg::Whole(i64::try_from(value.mantissa).ok()?))
            }
            (Op::TextPadStart, 2, Expr::Literal(Literal::Text(text))) => {
                let mut chars = text.chars();
                match (chars.next(), chars.next()) {
                    (Some(c), None) => Some(Arg::Character(c)),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// A `match` that isn't on a call's outcome, as `CASE`: its last arm
    /// is the `else`, since a `match` covers every case.
    fn choose(
        &mut self,
        frame: &Frame<'_>,
        subject: SiteId,
        arms: &[jadpo_graph::body::Arm],
    ) -> Option<(String, Held)> {
        let (value, held) = self.expr(frame, subject)?;
        let mut written = Vec::new();
        let mut result = None;
        let (last, rest) = arms.split_last()?;
        for arm in rest {
            let condition = match &arm.pattern {
                Pattern::Variant(key) => form::operator(
                    Operator::Equal,
                    &[
                        Arg::Value(&value, held),
                        Arg::Value(&variant_text(key)?, held),
                    ],
                )?,
                Pattern::True => value.clone(),
                Pattern::False => form::operator(Operator::Not, &[Arg::Value(&value, held)])?,
                Pattern::None => form::none_check(&value, false),
                Pattern::Some => form::none_check(&value, true),
                Pattern::Literal(literal) => {
                    let ty = frame.body.site(subject).ty.clone();
                    let literal = self.literal(literal, &ty)?;
                    form::operator(
                        Operator::Equal,
                        &[Arg::Value(&value, held), Arg::Value(&literal, held)],
                    )?
                }
                // A wildcard before the last arm takes every case left.
                Pattern::Wildcard => {
                    let (arm_value, h) = self.arm(frame, arm, &value, held)?;
                    return Some((form::case(&written, &arm_value), result.unwrap_or(h)));
                }
                _ => return None,
            };
            let (arm_value, h) = self.arm(frame, arm, &value, held)?;
            result.get_or_insert(h);
            written.push((condition, arm_value));
        }
        let (otherwise, h) = self.arm(frame, last, &value, held)?;
        Some((form::case(&written, &otherwise), result.unwrap_or(h)))
    }

    fn arm(
        &mut self,
        frame: &Frame<'_>,
        arm: &jadpo_graph::body::Arm,
        subject: &str,
        held: Held,
    ) -> Option<(String, Held)> {
        let mut inner = Frame {
            body: frame.body,
            locals: frame.locals.clone(),
        };
        // `some(x)` binds the value itself, now known not to be `none`.
        if let Some(binding) = arm.binding {
            if arm.pattern != Pattern::Some {
                return None;
            }
            inner.locals.insert(binding, (subject.to_string(), held));
        }
        match &arm.body {
            ArmBody::Value(value) => self.expr(&inner, *value),
            ArmBody::Block(statements) => self.block(&inner, statements),
            ArmBody::Empty => None,
        }
    }

    /// A literal of type `ty` as the storage codec would bind it.
    fn literal(&self, literal: &Literal, ty: &TypeRef) -> Option<String> {
        let (builtin, scale) = self.builtin(ty)?;
        let decoded = jadpo_std::constant::literal(literal, Some(builtin), scale)?;
        let bound =
            jadpo_std::standard::storage::bind(&decoded.stored()?, column_type(builtin)?).ok()?;
        jadpo_sql::literal::sql(&bound)
    }

    /// A constant's value as a literal of type `ty`.
    fn constant(&self, key: &Key, ty: &TypeRef) -> Option<(String, Held)> {
        use crate::constants::Value;
        let table = crate::constants::constant_table(self.db, self.project);
        let name = key.written();
        let value = table
            .values()
            .find(|evaluated| {
                evaluated.name == name || name.ends_with(&format!(".{}", evaluated.name))
            })?
            .value
            .clone()?;
        let held = self.held(ty)?;
        let literal = match value {
            Value::Number { value, decimal } => Literal::Number { value, decimal },
            Value::Text(text) => Literal::Text(text),
            Value::Bool(value) => Literal::Bool(value),
            Value::Duration(text) => Literal::Duration(text),
            Value::Variant(host, variant, fields) if fields.is_empty() => {
                let bound = jadpo_std::standard::storage::bind(
                    &jadpo_std::standard::storage::Stored::Variant {
                        ty: &host,
                        name: &variant,
                    },
                    SqlType::Text,
                )
                .ok()?;
                return Some((jadpo_sql::literal::sql(&bound)?, held));
            }
            _ => return None,
        };
        Some((self.literal(&literal, ty)?, held))
    }

    /// How a value of `ty` is held in one column: the built-in type it is
    /// built on, or text for an enum without data.
    fn held(&self, ty: &TypeRef) -> Option<Held> {
        if self.is_enum(ty) {
            return Some(Held::Builtin(Builtin::Text));
        }
        let (builtin, _) = self.builtin(ty)?;
        // A value in several columns (an amount of money) or JSON has no
        // single column here.
        column_type(builtin)?;
        Some(Held::Builtin(builtin))
    }

    /// The built-in type `ty` is built on, and the places its values keep.
    fn builtin(&self, ty: &TypeRef) -> Option<(Builtin, Option<u32>)> {
        let mut ty = ty.clone();
        let mut scale = None;
        for _ in 0..DEPTH {
            match ty {
                TypeRef::Builtin(builtin) => return Some((builtin, scale)),
                TypeRef::Nullable(inner) => ty = *inner,
                TypeRef::Node(key) | TypeRef::Field(key) => {
                    match self.graph.get(&key).map(|n| &n.detail) {
                        Some(Detail::Type {
                            base: Base::Refines(inner),
                            rules,
                            ..
                        }) => {
                            scale = scale.or(rules.scale);
                            ty = inner.clone();
                        }
                        Some(Detail::Field {
                            ty: inner, rules, ..
                        }) => {
                            scale = scale.or(rules.scale);
                            ty = inner.clone();
                        }
                        _ => return None,
                    }
                }
                _ => return None,
            }
        }
        None
    }

    fn is_enum(&self, ty: &TypeRef) -> bool {
        let mut ty = ty.clone();
        for _ in 0..DEPTH {
            match ty {
                TypeRef::Nullable(inner) => ty = *inner,
                TypeRef::Node(key) | TypeRef::Field(key) => {
                    match self.graph.get(&key).map(|n| &n.detail) {
                        Some(Detail::Type {
                            base: Base::Enum, ..
                        }) => return true,
                        Some(Detail::Type {
                            base: Base::Refines(inner),
                            ..
                        })
                        | Some(Detail::Field { ty: inner, .. }) => ty = inner.clone(),
                        _ => return false,
                    }
                }
                TypeRef::Std(name, _) => {
                    return jadpo_std::standard_type(&name).is_some_and(|entry| {
                        matches!(entry.holds, jadpo_std::standard::Holds::Enum(_))
                    });
                }
                _ => return false,
            }
        }
        false
    }

    /// The typed body of the callable with key `key`.
    fn body_of(&mut self, key: &Key) -> Option<Arc<Body>> {
        if self.bodies.is_none() {
            self.bodies = Some(bodies(self.db, self.project));
        }
        self.bodies.as_ref()?.get(key).cloned()
    }
}

/// An enum's plain variant as the text its column holds: its name, or a
/// standard enum's code.
fn variant_text(key: &Key) -> Option<String> {
    let (host, variant) = key.written().rsplit_once('.')?;
    let bound = jadpo_std::standard::storage::bind(
        &jadpo_std::standard::storage::Stored::Variant {
            ty: host,
            name: variant,
        },
        SqlType::Text,
    )
    .ok()?;
    jadpo_sql::literal::sql(&bound)
}

fn rounding(variant: &str) -> Option<Rounding> {
    Some(match variant {
        "up" => Rounding::Up,
        "down" => Rounding::Down,
        "half_up" => Rounding::HalfUp,
        "half_even" => Rounding::HalfEven,
        _ => return None,
    })
}

/// The column type a literal of `builtin` is bound for: the one column the
/// storage table keeps a value of a type a form reads in, at its widest
/// (chunk 5.8b).
fn column_type(builtin: Builtin) -> Option<SqlType> {
    if !form::reads(builtin) {
        return None;
    }
    jadpo_std::standard::storage::builtin_column(builtin.representation())
}
