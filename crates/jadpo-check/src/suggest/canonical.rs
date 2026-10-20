//! A snippet's canonical form (area 14, "Snippets and their canonical
//! form"): the graph fragment with its structure normalised and its names
//! kept, so the ways of writing one thing are one pattern. Operands of the
//! commutative operators are sorted and their chains flattened; `>` and
//! `>=` turn round into `<` and `<=`; numbers lose their spelling and
//! constants in one sum or product are folded; a named constant is its
//! value; a variable never reassigned is followed to its value; and
//! exactly equivalent forms are one (`x / 0.01` is `x * 100`).

use std::collections::HashMap;

use jadpo_graph::body::{
    BinaryOp, Body, Callee, Expr, Literal, LocalId, LocalKind, Node, SiteId, Stmt, UnaryOp,
};
use jadpo_graph::{Detail, Graph, Key};
use jadpo_std::Num;

/// A canonical fragment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum C {
    Num(Num),
    Text(String),
    /// Any other literal as written, or a word of the structure.
    Atom(String),
    /// A local, a parameter, a setting or a field path: `order.total_price`.
    Name(Vec<String>),
    /// An operator and its operands.
    Op(&'static str, Vec<C>),
    /// A call by what it calls.
    Call(String, Vec<C>),
    /// Any other structure.
    Tag(&'static str, Vec<C>),
}

impl C {
    pub fn render(&self) -> String {
        match self {
            C::Num(num) => num.to_string(),
            C::Text(text) => format!("{text:?}"),
            C::Atom(atom) => atom.clone(),
            C::Name(path) => path.join("."),
            C::Op(op, operands) | C::Tag(op, operands) => {
                let mut out = format!("({op}");
                for operand in operands {
                    out.push(' ');
                    out.push_str(&operand.render());
                }
                out.push(')');
                out
            }
            C::Call(name, args) => {
                let args: Vec<String> = args.iter().map(C::render).collect();
                format!("{name}({})", args.join(", "))
            }
        }
    }

    /// Every name it holds, as written.
    pub fn names(&self, out: &mut Vec<String>) {
        match self {
            C::Name(path) => out.extend(path.iter().cloned()),
            C::Op(_, items) | C::Call(_, items) | C::Tag(_, items) => {
                for item in items {
                    item.names(out);
                }
            }
            C::Num(_) | C::Text(_) | C::Atom(_) => {}
        }
    }

    /// Whether it is a literal of any kind.
    pub fn is_literal(&self) -> bool {
        matches!(self, C::Num(_) | C::Text(_) | C::Atom(_))
    }
}

/// `num` without trailing zeros after the point: `100.0` is `100`.
pub fn without_zeros(num: Num) -> Num {
    let mut num = num;
    while num.scale > 0 && num.mantissa % 10 == 0 {
        num.mantissa /= 10;
        num.scale -= 1;
    }
    num
}

fn product(a: Num, b: Num) -> Option<Num> {
    Some(without_zeros(Num {
        mantissa: a.mantissa.checked_mul(b.mantissa)?,
        scale: a.scale.checked_add(b.scale)?,
    }))
}

fn sum(a: Num, b: Num) -> Option<Num> {
    let scale = a.scale.max(b.scale);
    let lift = |n: Num| n.mantissa.checked_mul(10i128.checked_pow(scale - n.scale)?);
    Some(without_zeros(Num {
        mantissa: lift(a)?.checked_add(lift(b)?)?,
        scale,
    }))
}

/// `1 / num` when that is a whole number other than 1: `0.01` gives
/// `100`, `0.5` gives `2`.
fn reciprocal(num: Num) -> Option<Num> {
    let num = without_zeros(num);
    if num.scale == 0 || num.mantissa == 0 {
        return None;
    }
    let power = 10i128.checked_pow(num.scale)?;
    (power % num.mantissa == 0).then(|| Num::whole(power / num.mantissa))
}

fn is_one(num: Num) -> bool {
    without_zeros(num) == Num::whole(1)
}

fn is_zero(num: Num) -> bool {
    num.mantissa == 0
}

/// The last part of a key's name as code writes it: `total_price` for
/// `field Order.total_price`.
pub fn last(key: &Key) -> String {
    let written = key.written();
    written.rsplit('.').next().unwrap_or(written).to_string()
}

/// Turns one body's sites into canonical fragments, remembering each.
#[derive(Debug)]
pub struct Canon<'a> {
    pub body: &'a Body,
    graph: &'a Graph,
    /// Each variable never reassigned, with the site of its value.
    defs: HashMap<LocalId, SiteId>,
    done: HashMap<SiteId, C>,
    depth: u32,
}

impl<'a> Canon<'a> {
    pub fn new(body: &'a Body, graph: &'a Graph) -> Canon<'a> {
        let mut defs = HashMap::new();
        for site in &body.sites {
            if let Node::Stmt(Stmt::Var {
                local: Some(local),
                value: Some(value),
            }) = &site.node
                && body.local(*local).kind == LocalKind::Var
            {
                defs.insert(*local, *value);
            }
        }
        Canon {
            body,
            graph,
            defs,
            done: HashMap::new(),
            depth: 0,
        }
    }

    /// The canonical form of a statement or expression.
    pub fn site(&mut self, id: SiteId) -> C {
        if let Some(done) = self.done.get(&id) {
            return done.clone();
        }
        // A cycle can't happen in a checked body, but a broken one is
        // read too.
        if self.depth > 64 {
            return C::Atom("…".into());
        }
        self.depth += 1;
        let out = match &self.body.site(id).node {
            Node::Expr(expr) => self.expr(expr),
            Node::Stmt(stmt) => self.stmt(stmt),
        };
        self.depth -= 1;
        self.done.insert(id, out.clone());
        out
    }

    fn opt(&mut self, id: Option<SiteId>) -> C {
        id.map_or_else(|| C::Atom("_".into()), |id| self.site(id))
    }

    fn block(&mut self, ids: &[SiteId]) -> C {
        C::Tag("block", ids.iter().map(|id| self.site(*id)).collect())
    }

    fn local_name(&self, id: Option<LocalId>) -> C {
        id.map_or_else(
            || C::Atom("_".into()),
            |id| C::Name(vec![self.body.local(id).name.clone()]),
        )
    }

    fn stmt(&mut self, stmt: &Stmt) -> C {
        match stmt {
            Stmt::Var { local, value } => {
                let name = self.local_name(*local);
                let value = self.opt(*value);
                C::Tag("var", vec![name, value])
            }
            Stmt::Assign { local, value } => {
                let name = self.local_name(*local);
                let value = self.opt(*value);
                C::Tag("set", vec![name, value])
            }
            Stmt::Return { value } => {
                let value = self.opt(*value);
                C::Tag("return", vec![value])
            }
            Stmt::If {
                arms, otherwise, ..
            } => {
                let mut parts = Vec::new();
                for arm in arms {
                    parts.push(self.opt(arm.condition));
                    parts.push(self.block(&arm.then));
                }
                if !otherwise.is_empty() {
                    parts.push(self.block(otherwise));
                }
                C::Tag("if", parts)
            }
            Stmt::For { item, list, body } => {
                let item = self.local_name(*item);
                let list = self.opt(*list);
                let body = self.block(body);
                C::Tag("for", vec![item, list, body])
            }
            Stmt::Emit { value } => {
                let value = self.opt(*value);
                C::Tag("emit", vec![value])
            }
            Stmt::Assert { value, .. } => {
                let value = self.opt(*value);
                C::Tag("assert", vec![value])
            }
            Stmt::Expr { value } => self.opt(*value),
        }
    }

    fn expr(&mut self, expr: &Expr) -> C {
        match expr {
            Expr::Literal(Literal::Number { value, .. }) => C::Num(without_zeros(*value)),
            Expr::Literal(Literal::Text(text)) => C::Text(text.clone()),
            Expr::Literal(other) => C::Atom(other.written()),
            Expr::Local(id) => match self.defs.get(id).copied() {
                Some(value) => self.site(value),
                None => C::Name(vec![self.body.local(*id).name.clone()]),
            },
            Expr::Field { receiver, field } => {
                let name = last(field);
                match self.site(*receiver) {
                    C::Name(mut path) => {
                        path.push(name);
                        C::Name(path)
                    }
                    other => C::Tag("field", vec![other, C::Atom(name)]),
                }
            }
            // `Pence(x)` gives a value a type's meaning: `as`.
            Expr::Call {
                callee: Callee::Construct(ty),
                args,
                ..
            } if args.len() == 1 => {
                let value = self.opt(args[0]);
                C::Tag("as", vec![C::Atom(ty.to_string()), value])
            }
            Expr::Call { callee, args, .. } => {
                let args = args.iter().map(|arg| self.opt(*arg)).collect();
                C::Call(self.callee(callee), args)
            }
            Expr::Build { callee, fields, .. } => {
                let mut parts: Vec<C> = fields
                    .iter()
                    .map(|(key, value)| C::Tag("=", vec![C::Atom(last(key)), self.site(*value)]))
                    .collect();
                parts.sort_by_key(C::render);
                parts.insert(0, C::Atom(self.callee(callee)));
                C::Tag("build", parts)
            }
            Expr::List { items } => C::Tag("list", items.iter().map(|id| self.site(*id)).collect()),
            Expr::Binary { op, lhs, rhs } => {
                let (lhs, rhs) = (self.site(*lhs), self.site(*rhs));
                binary(*op, lhs, rhs)
            }
            Expr::Unary { op, operand } => {
                let operand = self.site(*operand);
                match (op, operand) {
                    (UnaryOp::Negate, C::Num(num)) => C::Num(Num {
                        mantissa: -num.mantissa,
                        scale: num.scale,
                    }),
                    (UnaryOp::Negate, other) => C::Op("neg", vec![other]),
                    (UnaryOp::Not, other) => C::Op("not", vec![other]),
                }
            }
            Expr::Attempt { inner, .. } => {
                let inner = self.opt(*inner);
                C::Tag("attempt", vec![inner])
            }
            Expr::Constant { constant } => {
                match self.graph.get(constant).map(|node| &node.detail) {
                    Some(Detail::Constant { value, .. }) => match Num::parse(value) {
                        Some(num) => C::Num(without_zeros(num)),
                        None => C::Atom(value.clone()),
                    },
                    _ => C::Name(vec![constant.written().to_string()]),
                }
            }
            Expr::Config { field } => C::Name(vec!["config".into(), last(field)]),
            Expr::Stored { field } => C::Name(vec![field.written().to_string()]),
            Expr::Inline { params, body } => {
                let mut parts: Vec<C> = params
                    .iter()
                    .map(|id| C::Name(vec![self.body.local(*id).name.clone()]))
                    .collect();
                parts.push(self.opt(*body));
                C::Tag("inline", parts)
            }
            Expr::Match { subject, .. } => {
                let subject = self.opt(*subject);
                C::Tag("match", vec![subject])
            }
            Expr::Reject { failure, .. } => C::Tag(
                "reject",
                failure
                    .iter()
                    .map(|key| C::Atom(key.written().to_string()))
                    .collect(),
            ),
            Expr::Write { entity, .. } => {
                C::Tag("write", vec![C::Atom(entity.written().to_string())])
            }
            Expr::Propagate { .. } => C::Tag("propagate", Vec::new()),
            Expr::Request { route, .. } => {
                C::Tag("request", vec![C::Atom(route.written().to_string())])
            }
            Expr::Now | Expr::Unchecked => C::Atom("…".into()),
        }
    }

    fn callee(&self, callee: &Callee) -> String {
        match callee {
            Callee::Std(op) => jadpo_std::OPERATIONS
                .iter()
                .find(|operation| operation.op == *op)
                .map_or_else(|| format!("{op:?}"), |operation| operation.qualified()),
            Callee::Construct(ty) => ty.to_string(),
            Callee::Entity { entity, operation } => format!("{}.{operation}", entity.written()),
            Callee::Translation(key) => format!("translations.{key}"),
            Callee::Caller(kind) => format!("{kind:?}"),
            Callee::Session(op) => format!("{op:?}"),
            Callee::Batch(
                op @ (jadpo_std::batch::BatchOp::Read(format)
                | jadpo_std::batch::BatchOp::Write(format)),
            ) => {
                format!("{}.{}", format.namespace(), op.name())
            }
            Callee::Batch(op) => format!("batches.{}", op.name()),
            Callee::Files { connection, op } => format!("{connection}.{}", op.name()),
            Callee::AdvanceClock => "clock.advance".into(),
            Callee::Unknown => "…".into(),
            other => other
                .key()
                .map_or_else(|| "…".into(), |key| key.written().to_string()),
        }
    }
}

/// The operator's word in the canonical form.
fn word(op: BinaryOp) -> &'static str {
    match op {
        BinaryOp::Or => "or",
        BinaryOp::And => "and",
        BinaryOp::Equal => "==",
        BinaryOp::NotEqual => "!=",
        BinaryOp::In => "in",
        BinaryOp::Matches => "matches",
        BinaryOp::Less | BinaryOp::Greater => "<",
        BinaryOp::LessEqual | BinaryOp::GreaterEqual => "<=",
        BinaryOp::Add => "+",
        BinaryOp::Subtract => "-",
        BinaryOp::Multiply => "*",
        BinaryOp::Divide => "/",
        BinaryOp::Remainder => "%",
    }
}

/// One binary operation, normalised.
pub fn binary(op: BinaryOp, lhs: C, rhs: C) -> C {
    match op {
        BinaryOp::Greater => binary(BinaryOp::Less, rhs, lhs),
        BinaryOp::GreaterEqual => binary(BinaryOp::LessEqual, rhs, lhs),
        BinaryOp::Add | BinaryOp::Multiply | BinaryOp::And | BinaryOp::Or => {
            chain(word(op), vec![lhs, rhs])
        }
        BinaryOp::Equal | BinaryOp::NotEqual => {
            let mut operands = vec![lhs, rhs];
            operands.sort_by_key(C::render);
            C::Op(word(op), operands)
        }
        BinaryOp::Divide => match &rhs {
            C::Num(num) if is_one(*num) => lhs,
            C::Num(num) => match reciprocal(*num) {
                Some(whole) => chain("*", vec![lhs, C::Num(whole)]),
                None => C::Op("/", vec![lhs, rhs]),
            },
            _ => C::Op("/", vec![lhs, rhs]),
        },
        _ => C::Op(word(op), vec![lhs, rhs]),
    }
}

/// A flattened, sorted chain of one associative, commutative operator,
/// its constants folded.
fn chain(op: &'static str, operands: Vec<C>) -> C {
    let mut flat = Vec::new();
    for operand in operands {
        match operand {
            C::Op(inner, items) if inner == op => flat.extend(items),
            other => flat.push(other),
        }
    }
    let mut constant: Option<Num> = None;
    let mut rest = Vec::new();
    for operand in flat {
        match (op, operand) {
            ("+" | "*", C::Num(num)) => {
                constant = Some(match constant {
                    None => num,
                    Some(before) => {
                        let folded = if op == "+" {
                            sum(before, num)
                        } else {
                            product(before, num)
                        };
                        match folded {
                            Some(folded) => folded,
                            None => {
                                rest.push(C::Num(num));
                                before
                            }
                        }
                    }
                });
            }
            (_, other) => rest.push(other),
        }
    }
    if let Some(num) = constant {
        let identity = if op == "+" { is_zero(num) } else { is_one(num) };
        if !identity || rest.is_empty() {
            // `x * 0.01` is `x / 100`.
            if op == "*"
                && !rest.is_empty()
                && let Some(whole) = reciprocal(num)
            {
                let rest = if rest.len() == 1 {
                    rest.remove(0)
                } else {
                    rest.sort_by_key(C::render);
                    C::Op("*", rest)
                };
                return C::Op("/", vec![rest, C::Num(whole)]);
            }
            rest.push(C::Num(num));
        }
    }
    if rest.len() == 1 {
        return rest.remove(0);
    }
    rest.sort_by_key(C::render);
    C::Op(op, rest)
}
