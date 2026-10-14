//! Proving a construction safe (area 4, question 6): when the declared
//! ranges show a computed value always follows the target type's rules,
//! `Type(value)` cannot fail and takes no `attempt`.
//!
//! Numbers are followed through arithmetic as intervals, from literals and
//! the `min` and `max` of each operand's type. Text, bytes and collections
//! are proved by their own type's rules implying the target's: a length
//! range inside the target's, and the same pattern, format or version.

use std::cmp::Ordering;

use jadpo_std::Op;
use jadpo_syntax::SyntaxKind;
use jadpo_syntax::ast::{self, AstNode, Expr};

use super::super::rules::{Const, Num, Rules};
use super::super::ty::Ty;
use super::{Body, node_key};

/// Dividing `a` by `b`, or taking the remainder: a value that is never
/// negative divided by a divisor above zero is never negative either, and
/// no larger than the dividend once the divisor is at least 1; anything
/// finer is left to an explicit check.
fn divided(a: Interval, b: Interval, remainder: bool) -> Interval {
    let zero = End::At(Num {
        mantissa: 0,
        scale: 0,
    });
    let one = End::At(Num {
        mantissa: 1,
        scale: 0,
    });
    if a.low.cmp(zero).is_ge() && b.low.cmp(zero).is_gt() {
        let high = if remainder {
            b.high
        } else if b.low.cmp(one).is_ge() {
            a.high
        } else {
            End::Above
        };
        Interval { low: zero, high }
    } else {
        Interval::ANY
    }
}

/// One end of a range: unbounded below, a number, or unbounded above.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum End {
    Below,
    At(Num),
    Above,
}

impl End {
    fn cmp(self, other: End) -> Ordering {
        match (self, other) {
            (End::Below, End::Below) | (End::Above, End::Above) => Ordering::Equal,
            (End::Below, _) | (_, End::Above) => Ordering::Less,
            (_, End::Below) | (End::Above, _) => Ordering::Greater,
            (End::At(a), End::At(b)) => a.compare(b),
        }
    }

    fn sign(self) -> Ordering {
        match self {
            End::Below => Ordering::Less,
            End::Above => Ordering::Greater,
            End::At(n) => n.mantissa.cmp(&0),
        }
    }

    fn neg(self) -> End {
        match self {
            End::Below => End::Above,
            End::Above => End::Below,
            End::At(n) => End::At(Num {
                mantissa: -n.mantissa,
                scale: n.scale,
            }),
        }
    }
}

/// The values an expression can take; `None` when nothing is known.
#[derive(Clone, Copy, Debug)]
struct Interval {
    low: End,
    high: End,
}

impl Interval {
    const ANY: Interval = Interval {
        low: End::Below,
        high: End::Above,
    };

    fn exact(n: Num) -> Interval {
        Interval {
            low: End::At(n),
            high: End::At(n),
        }
    }
}

fn lift(a: Num, b: Num) -> Option<(i128, i128, u32)> {
    let scale = a.scale.max(b.scale);
    let x = a
        .mantissa
        .checked_mul(10i128.checked_pow(scale - a.scale)?)?;
    let y = b
        .mantissa
        .checked_mul(10i128.checked_pow(scale - b.scale)?)?;
    Some((x, y, scale))
}

/// `a + b` for one end of a range, `low` for the lower end: an unbounded
/// end, or a sum too large to follow, leaves that end unbounded.
fn add(a: End, b: End, low: bool) -> End {
    let sum = match (a, b) {
        (End::At(a), End::At(b)) => lift(a, b).and_then(|(x, y, scale)| {
            Some(Num {
                mantissa: x.checked_add(y)?,
                scale,
            })
        }),
        _ => None,
    };
    match sum {
        Some(n) => End::At(n),
        None if low => End::Below,
        None => End::Above,
    }
}

fn mul(a: End, b: End) -> End {
    match (a, b) {
        (End::At(a), End::At(b)) => {
            match a.mantissa.checked_mul(b.mantissa).map(|mantissa| Num {
                mantissa,
                scale: a.scale + b.scale,
            }) {
                Some(n) => End::At(n),
                // Too large to follow: as good as unbounded, on the side
                // its sign says.
                None if (a.mantissa < 0) != (b.mantissa < 0) => End::Below,
                None => End::Above,
            }
        }
        _ => match (a.sign(), b.sign()) {
            (Ordering::Equal, _) | (_, Ordering::Equal) => End::At(Num {
                mantissa: 0,
                scale: 0,
            }),
            (x, y) if x == y => End::Above,
            _ => End::Below,
        },
    }
}

fn hull(ends: &[End]) -> Interval {
    let mut low = ends[0];
    let mut high = ends[0];
    for &end in ends {
        if end.cmp(low).is_lt() {
            low = end;
        }
        if end.cmp(high).is_gt() {
            high = end;
        }
    }
    Interval { low, high }
}

fn at_least(interval: &Interval, n: Num) -> bool {
    interval.low.cmp(End::At(n)).is_ge()
}

fn at_most(interval: &Interval, n: Num) -> bool {
    interval.high.cmp(End::At(n)).is_le()
}

/// Every rule of a chain at once: the tightest bounds, and any pattern,
/// format or version.
#[derive(Default)]
struct Combined {
    min: Option<Num>,
    max: Option<Num>,
    min_length: Option<u64>,
    max_length: Option<u64>,
    patterns: Vec<String>,
    email: bool,
    version: Option<jadpo_std::UuidVersion>,
}

fn combine<'r>(chain: impl Iterator<Item = &'r Rules>) -> Combined {
    let mut out = Combined::default();
    for rules in chain {
        if let Some(min) = rules.min
            && out.min.is_none_or(|m| min.compare(m).is_gt())
        {
            out.min = Some(min);
        }
        if let Some(max) = rules.max
            && out.max.is_none_or(|m| max.compare(m).is_lt())
        {
            out.max = Some(max);
        }
        if let Some(n) = rules.min_length {
            out.min_length = Some(out.min_length.map_or(n, |m| m.max(n)));
        }
        if let Some(n) = rules.max_length {
            out.max_length = Some(out.max_length.map_or(n, |m| m.min(n)));
        }
        out.patterns.extend(rules.pattern.clone());
        out.email |= rules.format == Some(jadpo_std::TextFormat::Email);
        if rules.version.is_some() {
            out.version = rules.version;
        }
    }
    out
}

impl Body<'_> {
    /// The smallest result a standard-library call gives, when the
    /// operation table says.
    /// The dividend and divisor of a call to a standard-library division,
    /// given by position or by name.
    fn division(&self, call: &ast::CallExpr) -> Option<(Expr, Expr)> {
        let super::facts::Reach::Std(op) = self.facts.reaches.get(&node_key(call.syntax()))? else {
            return None;
        };
        if !matches!(op, Op::IntegerDivide | Op::DecimalDivide) {
            return None;
        }
        let args: Vec<ast::Arg> = call.arg_list()?.args().collect();
        let find = |name: &str, position: usize| {
            let named = args.iter().find(|arg| {
                arg.name()
                    .and_then(|n| n.ident_token())
                    .is_some_and(|t| t.text() == name)
            });
            let positional = args.get(position).filter(|arg| arg.name().is_none());
            named.or(positional).and_then(ast::Arg::expr)
        };
        Some((find("dividend", 0)?, find("divisor", 1)?))
    }

    fn floor(&self, call: &ast::CallExpr) -> Option<i64> {
        match self.facts.reaches.get(&node_key(call.syntax()))? {
            super::facts::Reach::Std(op) => op.operation().at_least,
            _ => None,
        }
    }

    /// Whether the declared ranges prove `value`, of type `actual`, always
    /// follows `target`'s rules.
    pub(super) fn proves(&self, value: &Expr, actual: &Ty, target: &Ty) -> bool {
        let needs = combine(self.table.rule_chain(target).into_iter().map(|(_, r)| r));
        let has = combine(self.table.rule_chain(actual).into_iter().map(|(_, r)| r));
        if needs.min.is_some() || needs.max.is_some() {
            let interval = self.interval(value);
            if needs.min.is_some_and(|min| !at_least(&interval, min))
                || needs.max.is_some_and(|max| !at_most(&interval, max))
            {
                return false;
            }
        }
        if needs
            .min_length
            .is_some_and(|n| has.min_length.is_none_or(|m| m < n))
            || needs
                .max_length
                .is_some_and(|n| has.max_length.is_none_or(|m| m > n))
        {
            return false;
        }
        if needs.patterns.iter().any(|p| !has.patterns.contains(p)) {
            return false;
        }
        let is_email = self.table.base_prim(actual) == Some(super::super::ty::Prim::Email);
        if needs.email && !(has.email || is_email) {
            return false;
        }
        if needs.version.is_some() && needs.version != has.version {
            return false;
        }
        true
    }

    /// The values a numeric expression can take.
    fn interval(&self, expr: &Expr) -> Interval {
        match expr {
            Expr::ParenExpr(paren) => paren
                .expr()
                .map_or(Interval::ANY, |inner| self.interval(&inner)),
            Expr::Literal(_) => match Const::from_expr(expr) {
                Const::Number { value, .. } => Interval::exact(value),
                _ => Interval::ANY,
            },
            Expr::PrefixExpr(prefix) => {
                let minus =
                    prefix.syntax().first_token().map(|t| t.kind()) == Some(SyntaxKind::MINUS);
                match (minus, prefix.expr()) {
                    (true, Some(inner)) => {
                        let inner = self.interval(&inner);
                        Interval {
                            low: inner.high.neg(),
                            high: inner.low.neg(),
                        }
                    }
                    _ => Interval::ANY,
                }
            }
            Expr::BinExpr(binary) => self.binary_interval(binary),
            // A standard-library operation whose result has a floor its
            // type can't say, as `collection.count`'s is 0 (the operation
            // table's `at_least`).
            Expr::CallExpr(call) if let Some(floor) = self.floor(call) => Interval {
                low: End::At(Num::whole(i128::from(floor))),
                high: End::Above,
            },
            // `integer.divide` and `decimal.divide` are bounded as `/` is,
            // whichever way they round: rounding away from zero never
            // passes the dividend while the divisor is at least 1.
            Expr::CallExpr(call) if let Some((dividend, divisor)) = self.division(call) => {
                divided(self.interval(&dividend), self.interval(&divisor), false)
            }
            other => match self.facts.types.get(&node_key(other.syntax())) {
                Some(ty) => self.type_interval(ty),
                None => Interval::ANY,
            },
        }
    }

    fn type_interval(&self, ty: &Ty) -> Interval {
        if self.table.is_nullable(ty) {
            return Interval::ANY;
        }
        let rules = combine(self.table.rule_chain(ty).into_iter().map(|(_, r)| r));
        Interval {
            low: rules.min.map_or(End::Below, End::At),
            high: rules.max.map_or(End::Above, End::At),
        }
    }

    fn binary_interval(&self, binary: &ast::BinExpr) -> Interval {
        let (Some(lhs), Some(rhs)) = (binary.lhs(), binary.rhs()) else {
            return Interval::ANY;
        };
        let op = binary
            .op()
            .map(|token| token.text().to_string())
            .unwrap_or_default();
        let a = self.interval(&lhs);
        let b = self.interval(&rhs);
        match op.as_str() {
            "+" => Interval {
                low: add(a.low, b.low, true),
                high: add(a.high, b.high, false),
            },
            "-" => Interval {
                low: add(a.low, b.high.neg(), true),
                high: add(a.high, b.low.neg(), false),
            },
            "*" => hull(&[
                mul(a.low, b.low),
                mul(a.low, b.high),
                mul(a.high, b.low),
                mul(a.high, b.high),
            ]),
            "/" => divided(a, b, false),
            "%" => divided(a, b, true),
            _ => Interval::ANY,
        }
    }
}
