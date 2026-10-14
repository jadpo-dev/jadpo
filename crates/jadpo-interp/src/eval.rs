//! The tree walk over typed bodies (docs/compiler-runtime.md §2.5): each
//! statement and expression as the checker decided it, every effect and
//! frame through the runtime's [`Ctx`], so a fault or a failure names the
//! `.jadpo` site it happened at and the frames around it.
//!
//! What stops a body early travels as [`Stop`]: a `return`, a failure on
//! its way to an `attempt` or a `match`, a fault on its way out, a broken
//! rule on its way to the `attempt` that maps it, or a test's assertion.

use std::rc::Rc;

use jadpo_graph::body::{
    Arm, ArmBody, BinaryOp, Body, Callee, Expr, Fallible, Literal, LocalId, Node, Pattern, RuleArm,
    SiteId, Stmt, UnaryOp,
};
use jadpo_graph::{Key, TypeRef};
use jadpo_runtime::{Ctx, Failure, Fault, FaultKind, Frame};
use jadpo_std::{Builtin, Kind as ParamKind};
use jadpo_value::{Value, ops};

use crate::program::Program;
use crate::{Assertion, Outcome, Part};

/// What ends a body before its last statement.
pub(crate) enum Stop {
    Return(Value),
    Failed(Box<Failure>),
    Fault(Box<Fault>),
    /// A construction's value broke a rule, for the `attempt` whose block
    /// maps it: the rule's key (`max`, or `format` when a built-in type
    /// such as `Email` refused the text), and the rule as written for the
    /// report.
    Invalid {
        rule: String,
        written: String,
    },
    Assertion(Box<Assertion>),
}

type Eval<T> = Result<T, Stop>;

/// One running callable: its body and its locals.
struct Running<'p> {
    id: u32,
    body: &'p Body,
    locals: Vec<Value>,
    /// The failures the `match` arms being run handle, innermost last:
    /// what `propagate` passes on and what a `reject` there keeps as its
    /// cause.
    handling: Vec<Failure>,
}

pub(crate) struct Interp<'p> {
    program: &'p Program,
    ctx: Ctx<'p>,
    /// While an `assert` works out its condition: every value its frame's
    /// expressions give, for the report if it fails.
    record: Option<Recording>,
}

/// The values an `assert`'s condition gave, site by site.
struct Recording {
    /// The depth of the asserting frame: callees run deeper.
    depth: usize,
    values: Vec<(SiteId, Value)>,
}

/// A value from text, as a literal of a type built on `builtin` is.
pub(crate) fn text_literal(builtin: Builtin, text: &str) -> Option<Value> {
    Some(match builtin.representation() {
        Builtin::Uuid => Value::uuid(text),
        Builtin::Instant => Value::Instant(jadpo_std::instant_millis(text)?),
        Builtin::CalendarDate => Value::Date(jadpo_std::date_number(text)?),
        Builtin::Time => Value::Time(jadpo_std::time_millis(text)?),
        _ => Value::text(text),
    })
}

/// A comparison's operator as written.
fn symbol(op: BinaryOp) -> &'static str {
    match op {
        BinaryOp::Equal => "==",
        BinaryOp::NotEqual => "!=",
        BinaryOp::Less => "<",
        BinaryOp::LessEqual => "<=",
        BinaryOp::Greater => ">",
        BinaryOp::GreaterEqual => ">=",
        _ => "",
    }
}

fn is_comparison(op: BinaryOp) -> bool {
    matches!(
        op,
        BinaryOp::Equal
            | BinaryOp::NotEqual
            | BinaryOp::Less
            | BinaryOp::LessEqual
            | BinaryOp::Greater
            | BinaryOp::GreaterEqual
    )
}

impl<'p> Interp<'p> {
    /// Runs callable `id` of `program` with `args`.
    pub(crate) fn run(program: &'p Program, id: u32, args: Vec<Value>) -> Outcome {
        let mut interp = Interp {
            program,
            ctx: Ctx::new(&program.places, id),
            record: None,
        };
        match interp.body(id, args) {
            Ok(value) | Err(Stop::Return(value)) => Outcome::Value(value),
            Err(Stop::Failed(failure)) => Outcome::Failed(failure),
            Err(Stop::Fault(fault)) => Outcome::Fault(fault),
            Err(Stop::Assertion(assertion)) => Outcome::Assertion(assertion),
            Err(Stop::Invalid { .. }) => Outcome::Fault(interp.ctx.fault(
                FaultKind::Defect("a construction failed outside an `attempt`".into()),
                Frame::ENTRY,
            )),
        }
    }

    // --- stopping ---------------------------------------------------------

    fn fault(&mut self, kind: FaultKind, at: SiteId) -> Stop {
        Stop::Fault(self.ctx.fault(kind, at.0))
    }

    fn defect(&mut self, what: &str, at: SiteId) -> Stop {
        self.fault(FaultKind::Defect(what.to_string()), at)
    }

    /// Something a later chunk builds, named as written.
    fn not_built_yet(&mut self, what: impl Into<String>, at: SiteId) -> Stop {
        self.fault(FaultKind::NotBuiltYet(what.into()), at)
    }

    /// An operator's or operation's fault, at its site.
    fn value_fault(&mut self, fault: jadpo_value::Fault, at: SiteId) -> Stop {
        let kind = match fault {
            jadpo_value::Fault::IntOverflow => FaultKind::IntOverflow,
            jadpo_value::Fault::DecimalOverflow => FaultKind::DecimalOverflow,
            jadpo_value::Fault::DivisionByZero => {
                FaultKind::Defect("a division by zero, which the checker rules out".into())
            }
            jadpo_value::Fault::Shape | jadpo_value::Fault::Stopped => {
                FaultKind::Defect("a value of a kind the checker would have refused".into())
            }
        };
        self.fault(kind, at)
    }

    /// What a site is written as, up to its arguments or its block: what
    /// a fault for something not built yet names (`clock.now`,
    /// `Todo.update`).
    fn written(&self, f: &Running<'p>, at: SiteId) -> String {
        let Some(site) = f.body.sites.get(at.0 as usize) else {
            return String::new();
        };
        let text = self
            .program
            .callable(f.id)
            .and_then(|callable| self.program.text(&callable.file))
            .and_then(|text| text.get(site.start as usize..site.end as usize))
            .unwrap_or_default();
        let line = text.lines().next().unwrap_or_default();
        let end = line.find(['(', '{']).unwrap_or(line.len());
        line[..end].trim().to_string()
    }

    // --- callables --------------------------------------------------------

    /// Runs callable `id`'s body with `args`; its result.
    fn body(&mut self, id: u32, args: Vec<Value>) -> Eval<Value> {
        let program = self.program;
        let Some(callable) = program.callable(id) else {
            return Err(self.defect(
                "a call to a callable that has no body",
                SiteId(Frame::ENTRY),
            ));
        };
        let body = &callable.body;
        let mut frame = Running {
            id,
            body,
            locals: vec![Value::Unit; body.locals.len()],
            handling: Vec::new(),
        };
        for (param, arg) in body.params.iter().zip(args) {
            if let Some(slot) = frame.locals.get_mut(param.0 as usize) {
                *slot = arg;
            }
        }
        match self.block(&mut frame, &body.block) {
            Ok(()) => Ok(Value::Unit),
            Err(Stop::Return(value)) => Ok(value),
            Err(Stop::Invalid { .. }) => Err(self.defect(
                "a construction failed outside an `attempt`",
                SiteId(Frame::ENTRY),
            )),
            Err(other) => Err(other),
        }
    }

    /// Calls callable `id` from site `at` of the current frame.
    fn invoke(&mut self, id: u32, args: Vec<Value>, at: SiteId) -> Eval<Value> {
        self.ctx.enter(id, at.0).map_err(Stop::Fault)?;
        let result = self.body(id, args);
        self.ctx.leave();
        result
    }

    // --- statements -------------------------------------------------------

    fn block(&mut self, f: &mut Running<'p>, block: &'p [SiteId]) -> Eval<()> {
        for &id in block {
            self.stmt(f, id)?;
        }
        Ok(())
    }

    /// The value of an expression a statement needs.
    fn needed(&mut self, f: &mut Running<'p>, value: Option<SiteId>, at: SiteId) -> Eval<Value> {
        match value {
            Some(value) => self.eval(f, value),
            None => Err(self.defect("a statement without its value", at)),
        }
    }

    fn set(
        &mut self,
        f: &mut Running<'p>,
        local: Option<LocalId>,
        value: Value,
        at: SiteId,
    ) -> Eval<()> {
        match local.and_then(|local| f.locals.get_mut(local.0 as usize)) {
            Some(slot) => {
                *slot = value;
                Ok(())
            }
            None => Err(self.defect("a value given to no local", at)),
        }
    }

    fn stmt(&mut self, f: &mut Running<'p>, id: SiteId) -> Eval<()> {
        let body = f.body;
        let Some(Node::Stmt(stmt)) = body.sites.get(id.0 as usize).map(|site| &site.node) else {
            return Err(self.defect("an expression where a statement belongs", id));
        };
        match stmt {
            Stmt::Var { local, value } | Stmt::Assign { local, value } => {
                let value = self.needed(f, *value, id)?;
                self.set(f, *local, value, id)
            }
            Stmt::Return { value } => {
                let value = match value {
                    Some(value) => self.eval(f, *value)?,
                    None => Value::Unit,
                };
                Err(Stop::Return(value))
            }
            Stmt::If {
                arms, otherwise, ..
            } => {
                for arm in arms {
                    match self.needed(f, arm.condition, id)? {
                        Value::Bool(true) => return self.block(f, &arm.then),
                        Value::Bool(false) => {}
                        _ => return Err(self.defect("a condition that isn't a `Bool`", id)),
                    }
                }
                self.block(f, otherwise)
            }
            Stmt::For { item, list, body } => {
                let list = self.needed(f, *list, id)?;
                let Value::List(items) = list else {
                    return Err(self.defect("a loop over something that isn't a list", id));
                };
                for value in items.iter() {
                    self.set(f, *item, value.clone(), id)?;
                    self.block(f, body)?;
                }
                Ok(())
            }
            Stmt::Emit { .. } => Err(self.not_built_yet("emit", id)),
            Stmt::Assert { value, fails } => self.assert(f, id, *value, fails.as_ref()),
            Stmt::Expr { value } => self.needed(f, *value, id).map(|_| ()),
        }
    }

    /// `assert condition`, or `assert call(...) fails Failure` (area 13).
    fn assert(
        &mut self,
        f: &mut Running<'p>,
        id: SiteId,
        value: Option<SiteId>,
        fails: Option<&TypeRef>,
    ) -> Eval<()> {
        let Some(value) = value else {
            return Err(self.defect("an `assert` without a condition", id));
        };
        self.record = Some(Recording {
            depth: self.ctx.depth(),
            values: Vec::new(),
        });
        let result = self.assertion(f, id, value, fails);
        let recorded = self.record.take();
        match result {
            Err(Stop::Assertion(mut assertion)) => {
                if let Some(recorded) = recorded {
                    assertion.parts = self.parts(f, recorded);
                }
                Err(Stop::Assertion(assertion))
            }
            other => other,
        }
    }

    /// What an `assert`'s condition gives: nothing when it holds, the
    /// failed assertion otherwise.
    fn assertion(
        &mut self,
        f: &mut Running<'p>,
        id: SiteId,
        value: SiteId,
        fails: Option<&TypeRef>,
    ) -> Eval<()> {
        let body = f.body;
        if let Some(expected) = fails {
            let TypeRef::Node(expected) = expected else {
                return Err(self.defect("an `assert … fails` naming no failure", id));
            };
            let got = match self.subject(f, value)? {
                Err(Stop::Failed(failure)) if failure.key == expected.as_str() => return Ok(()),
                Err(Stop::Failed(failure)) => crate::Got::Failure(failure),
                Ok(value) => crate::Got::Value(value),
                Err(other) => return Err(other),
            };
            self.ctx.at(id.0);
            return Err(Stop::Assertion(Box::new(Assertion {
                frames: self.ctx.frames(),
                detail: crate::AssertionDetail::Fails {
                    expected: self.program.types.name(expected).to_string(),
                    got,
                },
                parts: Vec::new(),
            })));
        }
        let comparison = match body.sites.get(value.0 as usize).map(|site| &site.node) {
            Some(Node::Expr(Expr::Binary { op, lhs, rhs })) if is_comparison(*op) => {
                Some((*op, *lhs, *rhs))
            }
            _ => None,
        };
        let (holds, comparison) = match comparison {
            Some((op, lhs, rhs)) => {
                let left = self.eval(f, lhs)?;
                let right = self.eval(f, rhs)?;
                let holds = self.apply(op, &left, &right, value)? == Value::Bool(true);
                (holds, Some((symbol(op), left, right)))
            }
            None => (self.eval(f, value)? == Value::Bool(true), None),
        };
        if holds {
            return Ok(());
        }
        self.ctx.at(id.0);
        Err(Stop::Assertion(Box::new(Assertion {
            frames: self.ctx.frames(),
            detail: crate::AssertionDetail::False { comparison },
            parts: Vec::new(),
        })))
    }

    /// The recorded values of an assertion's condition as parts, in the
    /// order their anchors are written; literals left out.
    fn parts(&self, f: &Running<'p>, recorded: Recording) -> Vec<Part> {
        let text = self
            .program
            .callable(f.id)
            .and_then(|callable| self.program.text(&callable.file))
            .unwrap_or_default();
        let mut seen = std::collections::HashSet::new();
        let mut parts: Vec<Part> = Vec::new();
        for (id, value) in recorded.values {
            let Some(site) = f.body.sites.get(id.0 as usize) else {
                continue;
            };
            if matches!(site.node, Node::Expr(Expr::Literal(_))) || !seen.insert(id) {
                continue;
            }
            parts.push(Part {
                start: site.start,
                end: site.end,
                anchor: self.anchor(f, site, text),
                value,
            });
        }
        parts.sort_by_key(|part| (part.anchor, std::cmp::Reverse(part.end)));
        parts
    }

    /// Where a part's value points: an operator, a field's name, or the
    /// part's start.
    fn anchor(&self, f: &Running<'p>, site: &jadpo_graph::body::Site, text: &str) -> u32 {
        let sites = &f.body.sites;
        match &site.node {
            Node::Expr(Expr::Binary { lhs, rhs, .. }) => {
                let (Some(lhs), Some(rhs)) = (sites.get(lhs.0 as usize), sites.get(rhs.0 as usize))
                else {
                    return site.start;
                };
                text.get(lhs.end as usize..rhs.start as usize)
                    .and_then(|between| between.find(|c: char| !c.is_whitespace()))
                    .map_or(site.start, |offset| lhs.end + offset as u32)
            }
            Node::Expr(Expr::Field { field, .. }) => {
                let name = self.program.types.field_name(field);
                let written = text.get(site.start as usize..site.end as usize);
                match written {
                    Some(written) if written.ends_with(name) => site.end - name.len() as u32,
                    _ => site.start,
                }
            }
            _ => site.start,
        }
    }

    // --- expressions ------------------------------------------------------

    fn eval(&mut self, f: &mut Running<'p>, id: SiteId) -> Eval<Value> {
        let value = self.evaluate(f, id)?;
        self.remember(id, &value);
        Ok(value)
    }

    /// Keeps a value an `assert`'s condition gave, for its report.
    fn remember(&mut self, id: SiteId, value: &Value) {
        if let Some(record) = &mut self.record
            && record.depth == self.ctx.depth()
        {
            record.values.push((id, value.clone()));
        }
    }

    /// The subject of an outcome `match` or of `assert ... fails`, split
    /// in two: the outer result is its arguments, worked out first, whose
    /// failures leave the enclosing callable as they would anywhere else;
    /// the inner one is the call itself, the only outcome the `match` or
    /// the `assert` handles (areas 3 and 13).
    fn subject(&mut self, f: &mut Running<'p>, id: SiteId) -> Eval<Eval<Value>> {
        let body = f.body;
        let called = match body.sites.get(id.0 as usize).map(|site| &site.node) {
            Some(Node::Expr(Expr::Call {
                callee:
                    Callee::Function(key)
                    | Callee::Action(key)
                    | Callee::Fixture(key)
                    | Callee::LibraryFunction(key),
                args,
                ..
            })) => Some((key, args)),
            _ => None,
        };
        let Some((key, args)) = called else {
            // Nothing else an outcome is matched on has arguments that can
            // fail: the checker refuses the rest as subjects.
            return Ok(self.eval(f, id));
        };
        let Some(callee) = self.program.id(key) else {
            return Err(self.defect("a call to a callable that has no body", id));
        };
        let values = self.args(f, args, id)?;
        let outcome = self.invoke(callee, values, id);
        if let Ok(value) = &outcome {
            self.remember(id, value);
        }
        Ok(outcome)
    }

    fn evaluate(&mut self, f: &mut Running<'p>, id: SiteId) -> Eval<Value> {
        let body = f.body;
        let Some(site) = body.sites.get(id.0 as usize) else {
            return Err(self.defect("a site the body doesn't have", id));
        };
        let Node::Expr(expr) = &site.node else {
            return Err(self.defect("a statement where a value belongs", id));
        };
        match expr {
            Expr::Literal(literal) => self.literal(literal, &site.ty, id),
            Expr::Local(local) => match f.locals.get(local.0 as usize) {
                Some(value) => Ok(value.clone()),
                None => Err(self.defect("a local the body doesn't have", id)),
            },
            Expr::Field { receiver, field } => {
                let receiver = self.eval(f, *receiver)?;
                let name = self.program.types.field_name(field);
                let found = match &receiver {
                    Value::Record(record) => record.get(name),
                    Value::Variant(variant) => variant.get(name),
                    _ => None,
                };
                match found {
                    Some(value) => Ok(value.clone()),
                    None => Err(self.defect("a field read from a value that hasn't it", id)),
                }
            }
            Expr::Call {
                callee,
                args,
                fallible,
            } => self.call(f, id, callee, args, fallible),
            Expr::Build {
                callee,
                fields,
                fallible,
            } => self.build(f, id, callee, fields, fallible),
            Expr::List { items } => {
                let mut values = Vec::with_capacity(items.len());
                for item in items {
                    values.push(self.eval(f, *item)?);
                }
                let list = if self.program.types.is_set(&site.ty) {
                    Value::set(values)
                } else {
                    Value::list(values)
                };
                // A list written into a type with lengths is proved by the
                // checker, as a literal is.
                if self.program.proof_checks
                    && let Err(invalid) = self.program.types.validator(&site.ty).check(&list)
                {
                    return Err(self.proof_broken(&invalid, id));
                }
                Ok(list)
            }
            Expr::Binary { op, lhs, rhs } => self.binary(f, id, *op, *lhs, *rhs),
            Expr::Unary { op, operand } => {
                let value = self.eval(f, *operand)?;
                match (op, &value) {
                    (UnaryOp::Not, Value::Bool(b)) => Ok(Value::Bool(!b)),
                    (UnaryOp::Not, _) => {
                        Err(self.defect("`not` on something that isn't a `Bool`", id))
                    }
                    (UnaryOp::Negate, _) => {
                        ops::negate(&value).map_err(|fault| self.value_fault(fault, id))
                    }
                }
            }
            Expr::Match {
                subject,
                outcome,
                arms,
            } => self.matching(f, id, *subject, *outcome, arms),
            Expr::Attempt { inner, arms } => self.attempt(id, *inner, arms, f),
            Expr::Reject { failure, fields } => self.reject(f, id, failure.as_ref(), fields),
            Expr::Propagate { .. } => match f.handling.last() {
                Some(failure) => Err(Stop::Failed(Box::new(failure.clone()))),
                None => Err(self.defect("`propagate` outside a failure arm", id)),
            },
            Expr::Inline { .. } => {
                Err(self.defect("an inline function outside a collection operation", id))
            }
            Expr::Unchecked => {
                let written = self.written(f, id);
                Err(self.not_built_yet(written, id))
            }
        }
    }

    /// A literal as the type it goes into holds it (area 2, question 15).
    fn literal(&mut self, literal: &Literal, ty: &TypeRef, id: SiteId) -> Eval<Value> {
        match self.program.types.literal(literal, ty) {
            Some(value) => Ok(value),
            None => Err(self.defect("a literal the checker would have refused", id)),
        }
    }

    fn binary(
        &mut self,
        f: &mut Running<'p>,
        id: SiteId,
        op: BinaryOp,
        lhs: SiteId,
        rhs: SiteId,
    ) -> Eval<Value> {
        match op {
            // `and` and `or` stop as soon as the answer is known.
            BinaryOp::And | BinaryOp::Or => {
                let stop_at = op == BinaryOp::Or;
                match self.eval(f, lhs)? {
                    Value::Bool(b) if b == stop_at => Ok(Value::Bool(b)),
                    Value::Bool(_) => match self.eval(f, rhs)? {
                        Value::Bool(b) => Ok(Value::Bool(b)),
                        _ => Err(self.defect("`and` or `or` on something that isn't a `Bool`", id)),
                    },
                    _ => Err(self.defect("`and` or `or` on something that isn't a `Bool`", id)),
                }
            }
            _ => {
                let left = self.eval(f, lhs)?;
                let right = self.eval(f, rhs)?;
                self.apply(op, &left, &right, id)
            }
        }
    }

    /// An operator on two values, at its site.
    fn apply(&mut self, op: BinaryOp, left: &Value, right: &Value, id: SiteId) -> Eval<Value> {
        let ordered = |wanted: fn(std::cmp::Ordering) -> bool| {
            ops::compare(left, right)
                .map(|ordering| Value::Bool(wanted(ordering)))
                .ok_or(jadpo_value::Fault::Shape)
        };
        let result = match op {
            BinaryOp::Equal => Ok(Value::Bool(ops::equal(left, right))),
            BinaryOp::NotEqual => Ok(Value::Bool(!ops::equal(left, right))),
            BinaryOp::Less => ordered(std::cmp::Ordering::is_lt),
            BinaryOp::LessEqual => ordered(std::cmp::Ordering::is_le),
            BinaryOp::Greater => ordered(std::cmp::Ordering::is_gt),
            BinaryOp::GreaterEqual => ordered(std::cmp::Ordering::is_ge),
            BinaryOp::Add => ops::add(left, right),
            BinaryOp::Subtract => ops::subtract(left, right),
            BinaryOp::Multiply => ops::multiply(left, right),
            BinaryOp::Divide => ops::divide(left, right),
            BinaryOp::Remainder => ops::remainder(left, right),
            BinaryOp::In => ops::contains(right, left)
                .map(Value::Bool)
                .ok_or(jadpo_value::Fault::Shape),
            // A query's condition (area 6).
            BinaryOp::Matches => return Err(self.not_built_yet("matches", id)),
            BinaryOp::And | BinaryOp::Or => Err(jadpo_value::Fault::Shape),
        };
        result.map_err(|fault| self.value_fault(fault, id))
    }

    // --- calls ------------------------------------------------------------

    fn args(
        &mut self,
        f: &mut Running<'p>,
        args: &[Option<SiteId>],
        id: SiteId,
    ) -> Eval<Vec<Value>> {
        let mut values = Vec::with_capacity(args.len());
        for arg in args {
            values.push(self.needed(f, *arg, id)?);
        }
        Ok(values)
    }

    fn call(
        &mut self,
        f: &mut Running<'p>,
        id: SiteId,
        callee: &'p Callee,
        args: &'p [Option<SiteId>],
        fallible: &Fallible,
    ) -> Eval<Value> {
        match callee {
            Callee::Function(key)
            | Callee::Action(key)
            | Callee::Fixture(key)
            | Callee::LibraryFunction(key) => {
                let Some(callee) = self.program.id(key) else {
                    return Err(self.defect("a call to a callable that has no body", id));
                };
                let values = self.args(f, args, id)?;
                self.invoke(callee, values, id)
            }
            // A query is declarative (area 6).
            Callee::Query(key) => {
                let name = self.program.types.name(key).to_string();
                Err(self.not_built_yet(format!("query {name}"), id))
            }
            Callee::Entity { entity, operation } => {
                let name = self.program.types.name(entity).to_string();
                Err(self.not_built_yet(format!("{name}.{operation}"), id))
            }
            Callee::Std(op) => self.standard(f, id, *op, args),
            Callee::Construct(ty) => {
                let [Some(arg)] = args else {
                    return Err(self.defect("a construction without one value", id));
                };
                let value = self.eval(f, *arg)?;
                self.construct(ty, value, fallible, id)
            }
            Callee::Object(_) | Callee::Variant(_) | Callee::Unknown => {
                Err(self.defect("a call to something that can't be called", id))
            }
        }
    }

    /// A standard-library operation, run by its one implementation in
    /// `jadpo-value`; an inline function runs in this frame.
    fn standard(
        &mut self,
        f: &mut Running<'p>,
        id: SiteId,
        op: jadpo_std::Op,
        args: &'p [Option<SiteId>],
    ) -> Eval<Value> {
        let operation = op.operation();
        let mut values = Vec::new();
        let mut inline = None;
        for (param, arg) in operation.params.iter().zip(args) {
            let Some(arg) = arg else {
                return Err(self.defect("an operation without one of its values", id));
            };
            if param.kind == ParamKind::Function {
                match f.body.sites.get(arg.0 as usize).map(|site| &site.node) {
                    Some(Node::Expr(Expr::Inline {
                        param: Some(param),
                        body: Some(body),
                    })) => inline = Some((*param, *body)),
                    _ => return Err(self.defect("an operation's function that isn't inline", id)),
                }
            } else {
                values.push(self.eval(f, *arg)?);
            }
        }
        let mut stopped: Option<Stop> = None;
        // An inline function's expressions give a value per item, not one
        // an assertion's report could show.
        let paused = if inline.is_some() {
            self.record.take()
        } else {
            None
        };
        let result = {
            let mut each = |item: &Value| -> Option<Value> {
                let (param, body) = inline?;
                if let Some(slot) = f.locals.get_mut(param.0 as usize) {
                    *slot = item.clone();
                }
                match self.eval(f, body) {
                    Ok(value) => Some(value),
                    Err(stop) => {
                        stopped = Some(stop);
                        None
                    }
                }
            };
            jadpo_value::run(op, &values, &mut each)
        };
        if paused.is_some() {
            self.record = paused;
        }
        match (result, stopped) {
            (Ok(value), _) => Ok(value),
            (Err(_), Some(stop)) => Err(stop),
            (Err(fault), None) => Err(self.value_fault(fault, id)),
        }
    }

    /// `Type(value)`: the value as the type keeps it, checked against the
    /// type's rules when the checker couldn't prove it follows them.
    fn construct(
        &mut self,
        ty: &TypeRef,
        value: Value,
        fallible: &Fallible,
        id: SiteId,
    ) -> Eval<Value> {
        let validator = self.program.types.validator(ty);
        let Some(value) = validator.store(value) else {
            return Err(self.fault(FaultKind::DecimalOverflow, id));
        };
        if matches!(fallible, Fallible::Construction { .. }) {
            if let Err(invalid) = validator.check(&value) {
                return Err(Stop::Invalid {
                    rule: invalid.key().to_string(),
                    written: format!("`{}`", invalid.rule()),
                });
            }
        } else if self.program.proof_checks
            && let Err(invalid) = validator.check(&value)
        {
            return Err(self.proof_broken(&invalid, id));
        }
        Ok(value)
    }

    /// A value the checker proved follows its type's rules, breaking one:
    /// a hole in the proof, found only when proofs are checked again
    /// ([`Program::with_proof_checks`]).
    fn proof_broken(&mut self, invalid: &jadpo_value::Invalid, id: SiteId) -> Stop {
        self.defect(
            &format!(
                "a value the checker proved follows its type's rules, breaking `{}`",
                invalid.rule()
            ),
            id,
        )
    }

    /// `Type { ... }` or `Enum.variant { ... }`: each field as its field
    /// keeps it, in the order the type declares them.
    fn build(
        &mut self,
        f: &mut Running<'p>,
        id: SiteId,
        callee: &Callee,
        fields: &'p [(Key, SiteId)],
        fallible: &Fallible,
    ) -> Eval<Value> {
        let (Callee::Object(key) | Callee::Variant(key)) = callee else {
            return Err(self.defect("a build of something that isn't an object or a variant", id));
        };
        let mut given = Vec::with_capacity(fields.len());
        for (field, site) in fields {
            let value = self.eval(f, *site)?;
            let validator = self.program.types.validator(&TypeRef::Field(field.clone()));
            let Some(value) = validator.store(value) else {
                return Err(self.fault(FaultKind::DecimalOverflow, id));
            };
            if matches!(fallible, Fallible::Construction { .. }) {
                if let Err(invalid) = validator.check(&value) {
                    let name = self.program.types.field_name(field).to_string();
                    return Err(Stop::Invalid {
                        rule: invalid.key().to_string(),
                        written: format!("`{}` on `{name}`", invalid.rule()),
                    });
                }
            } else if self.program.proof_checks
                && let Err(invalid) = validator.check(&value)
            {
                return Err(self.proof_broken(&invalid, *site));
            }
            given.push((field.clone(), value));
        }
        match self.program.types.build(key, given) {
            Some(value) => Ok(value),
            None => Err(self.defect("a build without one of its fields", id)),
        }
    }

    // --- failures ---------------------------------------------------------

    fn given(
        &mut self,
        f: &mut Running<'p>,
        fields: &'p [(Key, SiteId)],
    ) -> Eval<Vec<(Key, Value)>> {
        let mut given = Vec::with_capacity(fields.len());
        for (field, site) in fields {
            given.push((field.clone(), self.eval(f, *site)?));
        }
        Ok(given)
    }

    /// `reject Failure { ... }`: raised here, with the failure a `match`
    /// arm handles as its cause (area 4, question 8).
    fn reject(
        &mut self,
        f: &mut Running<'p>,
        id: SiteId,
        failure: Option<&Key>,
        fields: &'p [(Key, SiteId)],
    ) -> Eval<Value> {
        let Some(key) = failure else {
            return Err(self.defect("a `reject` naming no failure", id));
        };
        let given = self.given(f, fields)?;
        let Some(mut failure) = self.program.types.failure(key, given) else {
            return Err(self.defect("a `reject` of something that isn't a failure", id));
        };
        failure.cause = f.handling.last().cloned().map(Box::new);
        Err(Stop::Failed(self.ctx.fail(failure, id.0)))
    }

    /// `attempt`: the inner call's failures pass straight through; a
    /// construction's broken rule becomes the failure its arm names, or
    /// `invalid:`'s for a rule without an arm of its own.
    fn attempt(
        &mut self,
        id: SiteId,
        inner: Option<SiteId>,
        arms: &'p [RuleArm],
        f: &mut Running<'p>,
    ) -> Eval<Value> {
        let Some(inner) = inner else {
            return Err(self.defect("an `attempt` without a call", id));
        };
        match self.eval(f, inner) {
            Err(Stop::Invalid { rule, written }) => {
                let arm = RuleArm::for_rule(arms, &rule);
                let Some(arm) = arm else {
                    return Err(
                        self.defect("a construction that can fail, with no failure for it", id)
                    );
                };
                // The failure's fields are worked out only when it is
                // raised, as a `reject`'s are.
                let given = self.given(f, &arm.fields)?;
                let Some(mut failure) = self.program.types.failure(&arm.failure, given) else {
                    return Err(self.defect("an arm naming something that isn't a failure", id));
                };
                failure.broken = Some(written);
                Err(Stop::Failed(self.ctx.fail(failure, inner.0)))
            }
            other => other,
        }
    }

    // --- match ------------------------------------------------------------

    fn matching(
        &mut self,
        f: &mut Running<'p>,
        id: SiteId,
        subject: Option<SiteId>,
        outcome: bool,
        arms: &'p [Arm],
    ) -> Eval<Value> {
        let Some(subject) = subject else {
            return Err(self.defect("a `match` without a subject", id));
        };
        if outcome {
            return self.outcome(f, id, subject, arms);
        }
        let value = self.eval(f, subject)?;
        let ty = f
            .body
            .sites
            .get(subject.0 as usize)
            .map(|site| site.ty.clone());
        for arm in arms {
            if self.matches(&arm.pattern, &value, ty.as_ref(), id)? {
                if let Some(binding) = arm.binding {
                    self.set(f, Some(binding), value.clone(), id)?;
                }
                return self.arm(f, arm);
            }
        }
        Err(self.defect("a `match` whose arms miss a case", id))
    }

    /// A `match` on a call's outcome: its value, or a failure it gives.
    fn outcome(
        &mut self,
        f: &mut Running<'p>,
        id: SiteId,
        subject: SiteId,
        arms: &'p [Arm],
    ) -> Eval<Value> {
        match self.subject(f, subject)? {
            Ok(value) => {
                let Some(arm) = arms.iter().find(|arm| arm.pattern == Pattern::Success) else {
                    return Err(self.defect("an outcome `match` without `success`", id));
                };
                if let Some(binding) = arm.binding {
                    self.set(f, Some(binding), value, id)?;
                }
                self.arm(f, arm)
            }
            Err(Stop::Failed(failure)) => {
                let Some(arm) = arms
                    .iter()
                    .find(|arm| matches!(&arm.pattern, Pattern::Failure(key) if key.as_str() == failure.key))
                else {
                    return Err(Stop::Failed(failure));
                };
                if let Some(binding) = arm.binding {
                    let fields = failure
                        .client
                        .iter()
                        .chain(&failure.internal)
                        .map(|(name, value)| (Rc::from(name.as_str()), value.clone()))
                        .collect();
                    self.set(f, Some(binding), Value::record(&failure.name, fields), id)?;
                }
                f.handling.push(*failure);
                let result = self.arm(f, arm);
                f.handling.pop();
                result
            }
            Err(other) => Err(other),
        }
    }

    fn arm(&mut self, f: &mut Running<'p>, arm: &'p Arm) -> Eval<Value> {
        match &arm.body {
            ArmBody::Value(value) => self.eval(f, *value),
            ArmBody::Block(block) => self.block(f, block).map(|()| Value::Unit),
            ArmBody::Empty => Ok(Value::Unit),
        }
    }

    /// Whether `value` is the case `pattern` names.
    fn matches(
        &mut self,
        pattern: &Pattern,
        value: &Value,
        ty: Option<&TypeRef>,
        id: SiteId,
    ) -> Eval<bool> {
        Ok(match pattern {
            Pattern::Variant(key) => {
                let shape = self.program.types.shape(key);
                matches!(value, Value::Variant(variant)
                    if variant.ty == shape.ty && Some(&variant.name) == shape.variant.as_ref())
            }
            Pattern::True => *value == Value::Bool(true),
            Pattern::False => *value == Value::Bool(false),
            Pattern::Some => *value != Value::None,
            Pattern::None => *value == Value::None,
            Pattern::Wildcard => true,
            Pattern::Literal(literal) => {
                let ty = ty.cloned().unwrap_or(TypeRef::None);
                let expected = self.literal(literal, &ty, id)?;
                ops::equal(value, &expected)
            }
            Pattern::Success | Pattern::Failure(_) | Pattern::Unknown => {
                return Err(self.defect("an outcome pattern on a value", id));
            }
        })
    }
}
