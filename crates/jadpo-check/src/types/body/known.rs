//! Conditions as proof (area 2, "Conditions as proof"; chunk 2.6): what an
//! `if` condition says about the numbers it compares, which the proof of a
//! construction reads ([`super::ranges`]).
//!
//! A *subject* is a number written from locals, number literals, the
//! fields of those, standard-library calls on them and the arithmetic
//! operators, and two subjects are the same when they are written the same
//! way, brackets aside ([`Term`]). A comparison (`<`, `<=`, `>`, `>=`,
//! `==`, `!=`) between two numbers bounds each side that is a subject by
//! the other side's range at that point; `and`, `or` and `not` combine
//! what their sides say. What holds is a bound per subject ([`Known`]),
//! each with the comparisons it came from, for the errors that name them.
//!
//! The walk keeps what is known as it goes (`Body::known`): the branch
//! where a condition holds knows it, an `else if` or `else` knows every
//! earlier condition is false, and the code after an `if` knows what holds
//! at the end of every branch that can reach it. Assigning a `var mut`
//! forgets every bound that mentions it; a `for` loop forgets, before and
//! after it, the bounds of every `var mut` it assigns; and a block's own
//! findings end with it. Nothing else adds a bound: a variable's starting
//! value, an assignment and a `match` pattern don't.

use std::collections::{BTreeMap, BTreeSet};

use jadpo_std::{Num, Op};
use jadpo_syntax::ast::{self, AstNode, Expr};
use jadpo_syntax::{SyntaxKind, SyntaxNode};

use super::super::rules::Const;
use super::super::ty::Prim;
use super::facts::{NodeKey, Reach};
use super::ranges::{End, Interval, Reading};
use super::{Body, node_key, text};
use crate::tree::dotted;

/// A number a bound can be known for, as written: two subjects are the same
/// when they are written the same way, brackets aside, so `count * 2` and
/// `(count * 2)` are one subject and `2 * count` another.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Term {
    /// A local, by its number in the typed body.
    Local(usize),
    /// A number literal, by value, without trailing zeros.
    Number(i128, u32),
    Negate(Box<Term>),
    /// `+`, `-`, `*`, `/` or `%`.
    Arithmetic(char, Box<Term>, Box<Term>),
    Field(Box<Term>, String),
    /// A standard-library operation, with its arguments in parameter
    /// order.
    Std(Op, Vec<Term>),
}

impl Term {
    fn number(n: Num) -> Term {
        let (mut mantissa, mut scale) = (n.mantissa, n.scale);
        while scale > 0 && mantissa % 10 == 0 {
            mantissa /= 10;
            scale -= 1;
        }
        Term::Number(mantissa, scale)
    }

    /// Whether `local` is part of it.
    fn mentions(&self, local: usize) -> bool {
        match self {
            Term::Local(id) => *id == local,
            Term::Number(..) => false,
            Term::Negate(inner) | Term::Field(inner, _) => inner.mentions(local),
            Term::Arithmetic(_, a, b) => a.mentions(local) || b.mentions(local),
            Term::Std(_, args) => args.iter().any(|arg| arg.mentions(local)),
        }
    }

    /// Whether every local in it is one of `locals`.
    fn within(&self, locals: &BTreeSet<usize>) -> bool {
        match self {
            Term::Local(id) => locals.contains(id),
            Term::Number(..) => true,
            Term::Negate(inner) | Term::Field(inner, _) => inner.within(locals),
            Term::Arithmetic(_, a, b) => a.within(locals) && b.within(locals),
            Term::Std(_, args) => args.iter().all(|arg| arg.within(locals)),
        }
    }

    /// Whether it reads no local, so knowing more about it says nothing.
    fn is_constant(&self) -> bool {
        self.within(&BTreeSet::new())
    }
}

/// A comparison a bound came from, as written, for the errors that name
/// it.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Origin {
    pub key: NodeKey,
    pub written: String,
}

/// A bound on one subject, and the comparisons it came from.
#[derive(Clone, Debug)]
pub(super) struct Fact {
    pub interval: Interval,
    pub from: BTreeSet<Origin>,
}

/// What holds at one point: a bound per subject. A subject with no bound
/// has only its declared range.
#[derive(Clone, Debug, Default)]
pub(super) struct Known {
    facts: BTreeMap<Term, Fact>,
}

impl Known {
    pub(super) fn is_empty(&self) -> bool {
        self.facts.is_empty()
    }

    pub(super) fn get(&self, term: &Term) -> Option<&Fact> {
        self.facts.get(term)
    }

    /// Adds a bound: it and whatever was known both hold.
    fn add(&mut self, term: Term, fact: Fact) {
        match self.facts.get_mut(&term) {
            Some(known) => {
                known.interval = known.interval.meet(fact.interval);
                known.from.extend(fact.from);
            }
            None => {
                self.facts.insert(term, fact);
            }
        }
    }

    /// What holds where both hold.
    pub(super) fn and(&self, other: &Known) -> Known {
        let mut out = self.clone();
        for (term, fact) in &other.facts {
            out.add(term.clone(), fact.clone());
        }
        out
    }

    /// What holds where either holds: only subjects both bound, by the
    /// range covering both.
    pub(super) fn or(&self, other: &Known) -> Known {
        let mut out = Known::default();
        for (term, a) in &self.facts {
            let Some(b) = other.facts.get(term) else {
                continue;
            };
            let interval = a.interval.hull(b.interval);
            if interval.is_any() {
                continue;
            }
            let from = a.from.union(&b.from).cloned().collect();
            out.facts.insert(term.clone(), Fact { interval, from });
        }
        out
    }

    /// What holds where any of `each` holds; `None` when there is nothing
    /// to join, which no code reaches.
    pub(super) fn any(each: Vec<Known>) -> Option<Known> {
        each.into_iter().reduce(|joined, next| joined.or(&next))
    }

    /// Forgets every bound that mentions `local`, which has just been
    /// pointed at a new value.
    pub(super) fn forget(&mut self, local: usize) {
        self.facts.retain(|term, _| !term.mentions(local));
    }

    /// Keeps only bounds on subjects made of `locals`, those in scope.
    pub(super) fn keep_within(&mut self, locals: &BTreeSet<usize>) {
        self.facts.retain(|term, _| term.within(locals));
    }

    fn bound(&mut self, term: &Option<Term>, interval: Interval, from: &BTreeSet<Origin>) {
        if let Some(term) = term
            && !interval.is_any()
        {
            self.add(
                term.clone(),
                Fact {
                    interval,
                    from: from.clone(),
                },
            );
        }
    }
}

/// A node as written, on one line: its tokens, one space wherever the
/// source has spacing, a line break or a comment between them.
fn spaced(node: &SyntaxNode) -> String {
    let mut out = String::new();
    let mut gap = false;
    for token in node
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
    {
        if token.kind().is_trivia() {
            gap = !out.is_empty();
            continue;
        }
        if gap {
            out.push(' ');
            gap = false;
        }
        out.push_str(token.text());
    }
    out
}

/// The comparison that holds exactly when `op` doesn't.
fn negated(op: &str) -> &'static str {
    match op {
        "<" => ">=",
        "<=" => ">",
        ">" => "<=",
        ">=" => "<",
        "==" => "!=",
        _ => "==",
    }
}

/// The single value an interval holds, if it holds one.
fn single(interval: Interval) -> Option<Num> {
    match (interval.low, interval.high) {
        (End::At(low), End::At(high)) if low.compare(high).is_eq() => Some(low),
        _ => None,
    }
}

/// `interval` without `value`, for a whole number: only a range that ends
/// at `value` gets narrower.
fn excluding(interval: Interval, value: Num) -> Interval {
    let at = End::At(value);
    let mut out = Interval::ANY;
    if interval.low.cmp(at).is_eq() {
        out.low = at.step(1);
    }
    if interval.high.cmp(at).is_eq() {
        out.high = at.step(-1);
    }
    out
}

/// The bounds `left op right` puts on its sides: `a` and `b` are their
/// ranges, `whole` whether both are whole numbers, so `n > 12` means
/// `n >= 13`.
fn bounds(op: &str, whole: bool, a: Interval, b: Interval) -> (Interval, Interval) {
    let by = |end: End, step: i128| if whole { end.step(step) } else { end };
    let upto = |high| Interval {
        low: End::Below,
        high,
    };
    let from = |low| Interval {
        low,
        high: End::Above,
    };
    match op {
        "<" => (upto(by(b.high, -1)), from(by(a.low, 1))),
        "<=" => (upto(b.high), from(a.low)),
        ">" => (from(by(b.low, 1)), upto(by(a.high, -1))),
        ">=" => (from(b.low), upto(a.high)),
        "==" => (b, a),
        // `!=` narrows only a whole number whose range ends at the value
        // the other side is: a decimal can come as close as it likes.
        _ if whole => (
            single(b).map_or(Interval::ANY, |value| excluding(a, value)),
            single(a).map_or(Interval::ANY, |value| excluding(b, value)),
        ),
        _ => (Interval::ANY, Interval::ANY),
    }
}

impl Body<'_> {
    /// The subject `expr` is, if it is one.
    pub(super) fn term(&self, expr: &Expr) -> Option<Term> {
        match expr {
            Expr::ParenExpr(paren) => self.term(&paren.expr()?),
            Expr::Literal(_) => match Const::from_expr(expr) {
                Const::Number { value, .. } => Some(Term::number(value)),
                _ => None,
            },
            Expr::PrefixExpr(prefix)
                if prefix.syntax().first_token().map(|t| t.kind()) == Some(SyntaxKind::MINUS) =>
            {
                Some(Term::Negate(Box::new(self.term(&prefix.expr()?)?)))
            }
            Expr::NameRef(name) => self
                .facts
                .reads
                .get(&node_key(name.syntax()))
                .map(|id| Term::Local(*id)),
            Expr::BinExpr(binary) => {
                let op = binary.op()?.text().chars().next()?;
                if !matches!(op, '+' | '-' | '*' | '/' | '%') || binary.op()?.text().len() != 1 {
                    return None;
                }
                Some(Term::Arithmetic(
                    op,
                    Box::new(self.term(&binary.lhs()?)?),
                    Box::new(self.term(&binary.rhs()?)?),
                ))
            }
            Expr::FieldExpr(_) => {
                // A field of a local that never changes, or of one whose
                // bounds go when it changes: `line.quantity`.
                if !self.facts.fields.contains_key(&node_key(expr.syntax())) {
                    return None;
                }
                let (first, rest) = dotted(expr)?;
                let id = self.facts.reads.get(&node_key(first.syntax()))?;
                Some(rest.iter().fold(Term::Local(*id), |term, name| {
                    Term::Field(Box::new(term), text(name))
                }))
            }
            Expr::CallExpr(call) => {
                let key = node_key(call.syntax());
                let Some(Reach::Std(op)) = self.facts.reaches.get(&key) else {
                    return None;
                };
                let slots = self.facts.slots.get(&key)?;
                let args: Vec<ast::Arg> = call.arg_list()?.args().collect();
                if args.len() != slots.len() {
                    return None;
                }
                let mut given: Vec<(usize, Term)> = Vec::new();
                for (arg, slot) in args.iter().zip(slots) {
                    given.push(((*slot)?, self.term(&arg.expr()?)?));
                }
                given.sort_by_key(|(slot, _)| *slot);
                Some(Term::Std(
                    *op,
                    given.into_iter().map(|(_, term)| term).collect(),
                ))
            }
            _ => None,
        }
    }

    /// Whether `expr` is a number that is never `none`, and whether it is
    /// whole: a literal by how it is written, a value by its type.
    fn numeric(&self, expr: &Expr) -> Option<bool> {
        if super::is_literal(expr) {
            return match Const::from_expr(expr) {
                Const::Number { decimal, .. } => Some(!decimal),
                _ => None,
            };
        }
        let ty = self.facts.types.get(&node_key(expr.syntax()))?;
        if ty.has_error() || self.table.is_nullable(ty) {
            return None;
        }
        match self.table.representation(ty)? {
            Prim::Int => Some(true),
            Prim::Decimal => Some(false),
            _ => None,
        }
    }

    /// What holds where `condition` is true, and where it is false, from
    /// what is known here. The right side of `and` is read knowing its left
    /// side holds, and of `or` knowing its left side doesn't, as they run.
    pub(super) fn split(&mut self, condition: &Expr) -> (Known, Known) {
        match condition {
            Expr::ParenExpr(paren) => match paren.expr() {
                Some(inner) => self.split(&inner),
                None => Default::default(),
            },
            Expr::PrefixExpr(prefix)
                if prefix.op().map(|t| t.kind()) != Some(SyntaxKind::MINUS) =>
            {
                match prefix.expr() {
                    Some(inner) => {
                        let (holds, fails) = self.split(&inner);
                        (fails, holds)
                    }
                    None => Default::default(),
                }
            }
            Expr::BinExpr(binary) => {
                let (Some(lhs), Some(rhs), Some(op)) = (binary.lhs(), binary.rhs(), binary.op())
                else {
                    return Default::default();
                };
                match op.text() {
                    "and" => {
                        let (a_holds, a_fails) = self.split(&lhs);
                        let (b_holds, b_fails) = self.knowing(&a_holds, |body| body.split(&rhs));
                        (a_holds.and(&b_holds), a_fails.or(&a_holds.and(&b_fails)))
                    }
                    "or" => {
                        let (a_holds, a_fails) = self.split(&lhs);
                        let (b_holds, b_fails) = self.knowing(&a_fails, |body| body.split(&rhs));
                        (a_holds.or(&a_fails.and(&b_holds)), a_fails.and(&b_fails))
                    }
                    "<" | "<=" | ">" | ">=" | "==" | "!=" => {
                        self.comparison(binary, &lhs, op.text(), &rhs)
                    }
                    _ => Default::default(),
                }
            }
            _ => Default::default(),
        }
    }

    /// Runs `then` with `extra` known as well as what is known now.
    pub(super) fn knowing<T>(&mut self, extra: &Known, then: impl FnOnce(&mut Self) -> T) -> T {
        if extra.is_empty() {
            return then(self);
        }
        let outer = self.known.clone();
        self.known = outer.and(extra);
        let out = then(self);
        self.known = outer;
        out
    }

    /// What `lhs op rhs` says about each side, where it holds and where it
    /// doesn't.
    fn comparison(
        &self,
        binary: &ast::BinExpr,
        lhs: &Expr,
        op: &str,
        rhs: &Expr,
    ) -> (Known, Known) {
        let (Some(left_whole), Some(right_whole)) = (self.numeric(lhs), self.numeric(rhs)) else {
            return Default::default();
        };
        let whole = left_whole && right_whole;
        let left = self.term(lhs).filter(|term| !term.is_constant());
        let right = self.term(rhs).filter(|term| !term.is_constant());
        if left.is_none() && right.is_none() {
            return Default::default();
        }
        let origin = Origin {
            key: node_key(binary.syntax()),
            written: spaced(binary.syntax()),
        };
        // Each side is bounded by the other side's range, so its bound
        // rests on the comparison and on whatever narrowed that range.
        let read = |side: &Expr| {
            let mut reading = Reading::with_conditions();
            let interval = self.interval(side, &mut reading);
            let mut from = reading.used;
            from.insert(origin.clone());
            (interval, from)
        };
        let (a, from_left) = read(lhs);
        let (b, from_right) = read(rhs);
        let mut out = [Known::default(), Known::default()];
        for (known, op) in out.iter_mut().zip([op, negated(op)]) {
            let (on_left, on_right) = bounds(op, whole, a, b);
            known.bound(&left, on_left, &from_right);
            known.bound(&right, on_right, &from_left);
        }
        let [holds, fails] = out;
        (holds, fails)
    }

    /// The locals a block assigns anywhere in it, among those in scope
    /// here: what a loop forgets before and after it runs.
    pub(super) fn assigned_in(&self, block: &ast::Block) -> Vec<usize> {
        let mut out: Vec<usize> = block
            .syntax()
            .descendants()
            .filter_map(ast::AssignStmt::cast)
            .filter_map(|assign| match assign.target() {
                Some(Expr::NameRef(name)) => self.find(&text(&name)).map(|local| local.id),
                _ => None,
            })
            .collect();
        out.sort_unstable();
        out.dedup();
        out
    }

    /// Forgets the bounds that mention `local`, which is being pointed at
    /// a new value, here and in every block around this one.
    pub(super) fn forget(&mut self, local: usize) {
        self.known.forget(local);
        self.assigned.push(local);
    }

    /// The locals in scope.
    pub(super) fn in_scope(&self) -> BTreeSet<usize> {
        self.scopes.iter().flatten().map(|local| local.id).collect()
    }
}
