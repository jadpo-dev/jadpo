//! A computed backfill as SQL (area 5, "Computed backfills"; area 19,
//! "SQL forms"; chunk 5.6b): the expression each existing row gets, written
//! from the backfill's typed body by the one SQL-form writer,
//! `jadpo_sql::form`, so a batch's `UPDATE` gives exactly what the runtime
//! would. A function the backfill calls is written in place, its
//! parameters replaced by its arguments' SQL, and its `if` chains and
//! `match`es become `CASE`. Anything without a form (a loop, a `var mut`,
//! an operation the tables give none, a value in several columns) gives
//! the whole backfill none, and it can't run by `UPDATE`.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use jadpo_db::{Db, Project};
use jadpo_graph::body::{
    ArmBody, BinaryOp, Body, Callee, Expr, Literal, LocalId, Node, Pattern, SiteId, Stmt,
    UnaryOp,
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
            form::operation(
                Op::DecimalRound,
                &[
                    Arg::Value(&value, held),
                    Arg::Whole(i64::from(scale)),
                    Arg::Rounding(Rounding::HalfEven),
                ],
            )
            .and_then(|form| form.value().map(str::to_string))
        }
        _ => Some(value),
    }
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
            Operator::Add | Operator::Subtract | Operator::Multiply | Operator::Divide
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
            // fail: the value itself.
            Callee::Construct(_) => {
                let [Some(arg)] = args else { return None };
                let (value, _) = self.expr(frame, *arg)?;
                Some((value, self.held(ty)?))
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

    /// What an operation takes written out rather than as a value: a
    /// rounding mode, `decimal.round`'s `scale:`, `text.pad_start`'s
    /// `fill:`.
    fn written_out(&self, frame: &Frame<'_>, op: Op, at: usize, arg: SiteId) -> Option<Arg<'static>> {
        let Node::Expr(expr) = &frame.body.site(arg).node else {
            return None;
        };
        match (op, at, expr) {
            (_, _, Expr::Build {
                callee: Callee::Variant(key),
                ..
            }) => {
                let (host, variant) = key.written().rsplit_once('.')?;
                (host == Rounding::NAME).then(|| rounding(variant)).flatten().map(Arg::Rounding)
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
                    &[Arg::Value(&value, held), Arg::Value(&variant_text(key)?, held)],
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
            .find(|evaluated| evaluated.name == name || name.ends_with(&format!(".{}", evaluated.name)))?
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
                        Some(Detail::Field { ty: inner, rules, .. }) => {
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
            let mut bodies = HashMap::new();
            for &file in self.project.files(self.db) {
                for callable in crate::types::callables_of(self.db, file) {
                    let checked = crate::types::typed_body(self.db, self.project, callable);
                    if let Some(key) = &checked.key {
                        bodies.insert(key.clone(), Arc::clone(&checked.body));
                    }
                }
            }
            self.bodies = Some(bodies);
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

/// The column type a literal of `builtin` is bound for: one column of a
/// type a form takes.
fn column_type(builtin: Builtin) -> Option<SqlType> {
    SqlType::of_column(match builtin.representation() {
        Builtin::Bool => "boolean",
        Builtin::Int | Builtin::Duration => "bigint",
        Builtin::Decimal => "numeric",
        Builtin::Text => "text",
        Builtin::Uuid => "uuid",
        Builtin::Bytes => "bytea",
        Builtin::CalendarDate => "date",
        Builtin::Instant => "timestamptz(3)",
        _ => return None,
    })
}
