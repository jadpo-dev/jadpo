//! Operators (area 3, "Expressions and operators"). Operators work on the
//! representation, so their result is a plain `Int`, `Decimal`, `Text` or
//! `Bool`. `+`, `-`, `/` and every comparison need both sides to be the
//! same named type after widening, or one of them a literal or a plain
//! value another operator made; `*` and `%` may mix types. Ordering is for
//! numbers and time values, `%` for `Int`, `and`, `or` and `not` for
//! `Bool`, and a divisor's type must exclude zero.

use jadpo_diag::{Diagnostic, rule};
use jadpo_syntax::SyntaxKind;
use jadpo_syntax::ast::{self, AstNode, Expr};

use super::super::literal;
use super::super::rules::Const;
use super::super::ty::{Prim, Ty};
use super::{Body, capital, is_literal};
use crate::tree;

/// One side of an operator.
#[derive(Clone, Debug)]
pub(super) enum Operand {
    /// A literal, which takes the other side's type.
    Literal(Const),
    Value(Ty),
}

impl Operand {
    fn has_error(&self) -> bool {
        matches!(self, Operand::Value(ty) if ty.has_error())
    }

    /// Shown in a message: `` `Score` `` or "a whole number".
    fn shown(&self) -> String {
        match self {
            Operand::Value(ty) => format!("`{ty}`"),
            Operand::Literal(value) => value.describe().to_string(),
        }
    }

    /// Shown after "is": `` a `Score` `` or "a whole number".
    fn shown_a(&self) -> String {
        match self {
            Operand::Value(ty) => {
                let written = ty.to_string();
                let an = if written.starts_with(['A', 'E', 'I', 'O']) {
                    "an"
                } else {
                    "a"
                };
                format!("{an} `{written}`")
            }
            Operand::Literal(value) => value.describe().to_string(),
        }
    }
}

/// What ordering applies to: numbers and time values.
fn ordered(prim: Prim) -> bool {
    matches!(
        prim,
        Prim::Int
            | Prim::Decimal
            | Prim::Instant
            | Prim::CalendarDate
            | Prim::Time
            | Prim::Duration
    )
}

fn verb(op: &str) -> &'static str {
    match op {
        "+" => "added",
        "-" => "subtracted",
        "*" => "multiplied",
        _ => "divided",
    }
}

impl Body<'_> {
    fn operand(&mut self, expr: &Expr) -> Operand {
        if is_literal(expr) {
            Operand::Literal(Const::from_expr(expr))
        } else {
            Operand::Value(self.infer(expr))
        }
    }

    /// What an operand is made of.
    fn rep(&self, operand: &Operand) -> Option<Prim> {
        match operand {
            Operand::Literal(Const::Number { decimal: true, .. }) => Some(Prim::Decimal),
            Operand::Literal(Const::Number { .. }) => Some(Prim::Int),
            Operand::Literal(Const::Str(_)) => Some(Prim::Text),
            Operand::Literal(Const::Bool(_)) => Some(Prim::Bool),
            Operand::Literal(Const::Duration(_)) => Some(Prim::Duration),
            Operand::Literal(_) => None,
            Operand::Value(ty) => self.table.representation(ty),
        }
    }

    fn nullable(&self, operand: &Operand) -> bool {
        match operand {
            Operand::Value(ty) => self.table.is_nullable(ty),
            Operand::Literal(Const::None) => true,
            Operand::Literal(_) => false,
        }
    }

    /// A literal, or a plain value another operator or a standard-library
    /// operation made: it carries no unit of its own.
    fn unitless(operand: &Operand) -> bool {
        matches!(operand, Operand::Literal(_) | Operand::Value(Ty::Prim(_)))
    }

    /// Whether two operands are the same type after widening, or one of
    /// them takes the other's type.
    fn same_type(&self, a: &Operand, b: &Operand) -> bool {
        match (a, b) {
            (Operand::Literal(_), Operand::Literal(_)) => match (self.rep(a), self.rep(b)) {
                (Some(x), Some(y)) => x == y || (x.is_number() && y.is_number()),
                (x, y) => x == y,
            },
            (Operand::Literal(value), Operand::Value(ty))
            | (Operand::Value(ty), Operand::Literal(value)) => !matches!(
                literal::check(self.table, ty.strip_nullable(), value),
                Err(literal::Problem::Kind { .. })
            ),
            (Operand::Value(x), Operand::Value(y)) => {
                let (x, y) = (x.strip_nullable(), y.strip_nullable());
                if self.table.assignable(x, y) || self.table.assignable(y, x) {
                    return true;
                }
                if matches!(x, Ty::Prim(_)) || matches!(y, Ty::Prim(_)) {
                    let (rx, ry) = (self.table.representation(x), self.table.representation(y));
                    return match (rx, ry) {
                        (Some(rx), Some(ry)) => rx == ry || (rx.is_number() && ry.is_number()),
                        _ => false,
                    };
                }
                let mine = self.named_ancestors(x);
                self.named_ancestors(y).iter().any(|t| mine.contains(t))
            }
        }
    }

    /// The named types `ty` widens to, itself first; built-in types are
    /// left out, since two values built on `Uuid` are not the same type.
    fn named_ancestors(&self, ty: &Ty) -> Vec<Ty> {
        let mut found = Vec::new();
        let mut current = Some(ty.clone());
        for _ in 0..64 {
            let Some(next) = current else { break };
            if !matches!(next, Ty::Prim(_)) {
                found.push(next.clone());
            }
            current = self.table.parent(&next);
        }
        found
    }

    pub(super) fn prefix(&mut self, prefix: &ast::PrefixExpr) -> Ty {
        let minus = prefix.op().map(|t| t.kind()) == Some(SyntaxKind::MINUS);
        let Some(inner) = prefix.expr() else {
            return Ty::Error;
        };
        let operand = self.operand(&inner);
        if operand.has_error() {
            return if minus {
                Ty::Error
            } else {
                Ty::Prim(Prim::Bool)
            };
        }
        let rep = self.rep(&operand);
        let nullable = self.nullable(&operand);
        let written = tree::written(inner.syntax());
        let span = self.span(&inner);
        let shown = operand.shown();
        if minus {
            if let Some(prim) = rep.filter(|prim| prim.is_number() && !nullable) {
                return Ty::Prim(prim);
            }
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_UNARY_OPERAND,
                    format!(
                        "`-` needs a number, and `{written}` is {}",
                        operand.shown_a()
                    ),
                )
                .at(span, format!("this is {shown}"))
                .reason(if nullable {
                    format!("`{written}` may be `none`, which has no negative.")
                } else {
                    "`-` negates a number: a value built on `Int` or `Decimal`.".to_string()
                })
                .next(if nullable {
                    format!("Match on `{written}` and negate inside `some(...)`")
                } else {
                    "Use a number here".to_string()
                }),
            );
            return Ty::Error;
        }
        if rep != Some(Prim::Bool) || nullable {
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_UNARY_OPERAND,
                    format!("`not` needs a `Bool`, and `{written}` is {}", operand.shown_a()),
                )
                .at(span, format!("this is {shown}"))
                .reason(if nullable {
                    format!("`not` works on a `Bool` that is never `none`; `{written}` may be `none`.")
                } else {
                    "`not` works on `Bool` values and types built on `Bool`; Jadpo has no truthiness.".to_string()
                })
                .next(format!("Compare `{written}` to get a `Bool`")),
            );
        }
        Ty::Prim(Prim::Bool)
    }

    pub(super) fn binary(&mut self, binary: &ast::BinExpr) -> Ty {
        let op = binary
            .op()
            .map(|token| token.text().to_string())
            .unwrap_or_default();
        let (Some(lhs), Some(rhs)) = (binary.lhs(), binary.rhs()) else {
            for side in [binary.lhs(), binary.rhs()].into_iter().flatten() {
                self.infer(&side);
            }
            return Ty::Error;
        };
        let a = self.operand(&lhs);
        let b = self.operand(&rhs);
        let comparison = matches!(
            op.as_str(),
            "and" | "or" | "==" | "!=" | "<" | "<=" | ">" | ">=" | "in" | "matches"
        );
        if a.has_error() || b.has_error() {
            return if comparison {
                Ty::Prim(Prim::Bool)
            } else {
                Ty::Error
            };
        }
        match op.as_str() {
            "and" | "or" => {
                for (side, operand) in [(&lhs, &a), (&rhs, &b)] {
                    if !self.logical_operand(&op, side, operand) {
                        break;
                    }
                }
                Ty::Prim(Prim::Bool)
            }
            "==" | "!=" => {
                self.equality(binary, &op, (&lhs, &a), (&rhs, &b));
                Ty::Prim(Prim::Bool)
            }
            "<" | "<=" | ">" | ">=" => {
                self.ordering(binary, &op, (&lhs, &a), (&rhs, &b));
                Ty::Prim(Prim::Bool)
            }
            "+" | "-" | "*" | "/" | "%" => self.arithmetic(binary, &op, (&lhs, &a), (&rhs, &b)),
            _ => Ty::Prim(Prim::Bool),
        }
    }

    /// Whether one side of `and` or `or` is a `Bool`; reports it if not.
    fn logical_operand(&mut self, op: &str, side: &Expr, operand: &Operand) -> bool {
        let nullable = self.nullable(operand);
        if self.rep(operand) == Some(Prim::Bool) && !nullable {
            return true;
        }
        let written = tree::written(side.syntax());
        let shown = operand.shown();
        let span = self.span(side);
        self.out.push(
            Diagnostic::new(
                &rule::TYPE_LOGICAL_OPERAND,
                format!("`{op}` needs `Bool` values, and `{written}` is {}", operand.shown_a()),
            )
            .at(span, format!("this is {shown}"))
            .reason(if nullable {
                format!(
                    "`and` and `or` work on `Bool` values that are never `none`; `{written}` may be `none`."
                )
            } else {
                "`and` and `or` work on `Bool` values and types built on `Bool`; Jadpo has no truthiness.".to_string()
            })
            .next(format!("Compare `{written}` to get a `Bool`")),
        );
        false
    }

    fn equality(
        &mut self,
        binary: &ast::BinExpr,
        op: &str,
        (lhs, a): (&Expr, &Operand),
        (rhs, b): (&Expr, &Operand),
    ) {
        let span = self.span(binary);
        // `none` on one side: the other must be nullable.
        for (none, other, other_expr) in [(a, b, rhs), (b, a, lhs)] {
            if matches!(none, Operand::Literal(Const::None)) {
                if self.nullable(other) {
                    return;
                }
                let written = tree::written(other_expr.syntax());
                let shown = other.shown();
                self.out.push(
                    Diagnostic::new(
                        &rule::TYPE_INCOMPARABLE,
                        format!("`{written}` is never `none`"),
                    )
                    .at(span, format!("{shown} compared with `none`"))
                    .reason(format!(
                        "Only a nullable value, written `T?`, can be `none`; `{written}` is {shown}."
                    ))
                    .next("Remove the comparison, since it is always the same"),
                );
                return;
            }
        }
        if self.same_type(a, b) {
            return;
        }
        let (sa, sb) = (a.shown(), b.shown());
        self.out.push(
            Diagnostic::new(
                &rule::TYPE_INCOMPARABLE,
                format!("{} and {} can't be compared", capital(&sa), sb),
            )
            .at(span, "different types")
            .reason(format!(
                "`{op}` compares two values of the same type, after widening; {sa} and {sb} are different types, and neither widens to the other."
            ))
            .next("Compare two values of the same type"),
        );
    }

    fn ordering(
        &mut self,
        binary: &ast::BinExpr,
        op: &str,
        (lhs, a): (&Expr, &Operand),
        (rhs, b): (&Expr, &Operand),
    ) {
        for (side, operand) in [(lhs, a), (rhs, b)] {
            let written = tree::written(side.syntax());
            let shown = operand.shown();
            let span = self.span(side);
            let problem = if self.nullable(operand) {
                let ty = match operand {
                    Operand::Value(ty) => format!("`{ty}`"),
                    Operand::Literal(_) => "`none`".to_string(),
                };
                Some((
                    format!("`{written}` may be `none`, so it can't be ordered"),
                    "may be `none`".to_string(),
                    format!(
                        "`{written}` is {ty}; `<`, `<=`, `>` and `>=` order two values, and `none` has no place in an order."
                    ),
                    format!("Match on `{written}` and compare inside `some(...)`"),
                ))
            } else {
                match self.rep(operand) {
                    Some(prim) if ordered(prim) => None,
                    Some(Prim::Text) => Some((
                        format!("{} has no order", capital(&shown)),
                        format!("this is {shown}"),
                        "Text order depends on the reader's language, so `<`, `<=`, `>` and `>=` work only on numbers and time values.".to_string(),
                        "Compare the text with `==`, or order it with a standard-library function".to_string(),
                    )),
                    _ => Some((
                        format!("{} has no order", capital(&shown)),
                        format!("this is {shown}"),
                        "`<`, `<=`, `>` and `>=` work only on numbers and time values.".to_string(),
                        "Order numbers or time values".to_string(),
                    )),
                }
            };
            if let Some((message, label, reason, next)) = problem {
                self.out.push(
                    Diagnostic::new(&rule::TYPE_ORDERING_OPERAND, message)
                        .at(span, label)
                        .reason(reason)
                        .next(next),
                );
                return;
            }
        }
        if self.same_type(a, b) {
            return;
        }
        let (sa, sb) = (a.shown(), b.shown());
        let span = self.span(binary);
        self.out.push(
            Diagnostic::new(
                &rule::TYPE_ORDERING_OPERAND,
                format!("{} and {} can't be ordered", capital(&sa), sb),
            )
            .at(span, "different types")
            .reason(format!(
                "`{op}` orders two values of the same named type, after widening, or a value and a literal; {sa} and {sb} are different types."
            ))
            .next("Order two values of the same type"),
        );
    }

    fn arithmetic(
        &mut self,
        binary: &ast::BinExpr,
        op: &str,
        (lhs, a): (&Expr, &Operand),
        (rhs, b): (&Expr, &Operand),
    ) -> Ty {
        for (side, operand) in [(lhs, a), (rhs, b)] {
            if self.nullable(operand) {
                let written = tree::written(side.syntax());
                let span = self.span(side);
                self.out.push(
                    Diagnostic::new(
                        &rule::TYPE_ARITHMETIC_OPERAND,
                        format!("`{written}` may be `none`, so it can't be {}", verb(op)),
                    )
                    .at(span, "may be `none`")
                    .reason(format!(
                        "`{op}` works on values that are never `none`; `none` has no number."
                    ))
                    .next(format!(
                        "Match on `{written}` and use it inside `some(...)`"
                    )),
                );
                return Ty::Error;
            }
        }
        let (ra, rb) = (self.rep(a), self.rep(b));
        let (sa, sb) = (a.shown(), b.shown());
        let span = self.span(binary);
        // `+`, `-` and `/` take the same named type on both sides, or a
        // literal, which takes the other side's type (area 3, "Expressions
        // and operators"); joining text follows the same rule.
        if matches!(op, "+" | "-" | "/") && !self.same_type(a, b) {
            let literal = matches!(a, Operand::Literal(_)) || matches!(b, Operand::Literal(_));
            let both_values = !Self::unitless(a) && !Self::unitless(b);
            let numbers = ra.is_some_and(Prim::is_number) && rb.is_some_and(Prim::is_number);
            let texts = op == "+" && ra == Some(Prim::Text) && rb == Some(Prim::Text);
            if (literal && numbers) || (texts && both_values) {
                let reason = if literal {
                    format!(
                        "A literal takes the type on the other side of `{op}`, and {} can't be one; write it the way that type's values are written.",
                        if matches!(a, Operand::Literal(_)) { &sa } else { &sb }
                    )
                } else {
                    format!(
                        "`+` joins two values of the same named type, after widening, or a value and a literal; {sa} and {sb} are different types."
                    )
                };
                self.out.push(
                    Diagnostic::new(
                        &rule::TYPE_ARITHMETIC_OPERAND,
                        format!("{} and {} can't be {}", capital(&sa), sb, verb(op)),
                    )
                    .at(span, "different types")
                    .reason(reason)
                    .next(if literal {
                        "Write the literal as the other side's kind of number, such as `3.0` for a `Decimal`".to_string()
                    } else {
                        "Use two values of the same type".to_string()
                    }),
                );
                return Ty::Error;
            }
        }
        if op == "+" && ra == Some(Prim::Text) && rb == Some(Prim::Text) {
            return Ty::Prim(Prim::Text);
        }
        let numbers = ra.is_some_and(Prim::is_number) && rb.is_some_and(Prim::is_number);
        if !numbers {
            let made = |operand: &Operand, prim: Option<Prim>| match (operand, prim) {
                (Operand::Value(Ty::Prim(_)), _) | (Operand::Literal(_), _) => operand.shown(),
                (_, Some(prim)) => format!("{} is built on `{}`", operand.shown(), prim.name()),
                (_, None) => format!("{} is not a number", operand.shown()),
            };
            let what = if op == "+" {
                "`+` adds two numbers, or joins two `Text` values".to_string()
            } else {
                format!("`{op}` works on numbers")
            };
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_ARITHMETIC_OPERAND,
                    format!("{} and {} can't be {}", capital(&sa), sb, verb(op)),
                )
                .at(span, "not two numbers")
                .reason(format!("{what}; {}, and {}.", made(a, ra), made(b, rb)))
                .next(format!("Use numbers on both sides of `{op}`")),
            );
            return Ty::Error;
        }
        if op == "%" && (ra == Some(Prim::Decimal) || rb == Some(Prim::Decimal)) {
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_ARITHMETIC_OPERAND,
                    "`%` works on `Int` values",
                )
                .at(span, "a `Decimal` remainder")
                .reason(format!(
                    "`%` gives the remainder of dividing whole numbers; {sa} and {sb} are not both built on `Int`."
                ))
                .next("Use `Int` values on both sides of `%`"),
            );
            return Ty::Error;
        }
        if matches!(op, "+" | "-" | "/")
            && !Self::unitless(a)
            && !Self::unitless(b)
            && !self.same_type(a, b)
        {
            let (reason, next) = if op == "/" {
                (
                    format!(
                        "`/` divides two values of the same named type; dividing {sa} by {sb}, or rounding another way, is `integer.divide` or `decimal.divide`."
                    ),
                    "Divide with `integer.divide` or `decimal.divide`".to_string(),
                )
            } else {
                (
                    format!(
                        "`+` and `-` need the same named type on both sides, or a literal, because {sa} and {sb} are different units; `*` and `%` may mix types."
                    ),
                    "Use two values of the same type".to_string(),
                )
            };
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_ARITHMETIC_OPERAND,
                    format!("{} and {} can't be {}", capital(&sa), sb, verb(op)),
                )
                .at(span, "different types")
                .reason(reason)
                .next(next),
            );
            return Ty::Error;
        }
        if matches!(op, "/" | "%") {
            self.divisor(rhs, b);
        }
        if ra == Some(Prim::Decimal) || rb == Some(Prim::Decimal) {
            Ty::Prim(Prim::Decimal)
        } else {
            Ty::Prim(Prim::Int)
        }
    }

    /// Division by zero is ruled out while compiling (area 3, question 7):
    /// a literal divisor is not `0`, and any other divisor's type excludes
    /// zero.
    pub(super) fn divisor(&mut self, rhs: &Expr, operand: &Operand) {
        let span = self.span(rhs);
        let written = tree::written(rhs.syntax());
        match operand {
            Operand::Literal(Const::Number { value, .. }) => {
                if value.mantissa == 0 {
                    self.out.push(
                        Diagnostic::new(&rule::TYPE_DIVISOR_ZERO, "This divides by zero")
                            .at(span, "zero")
                            .reason("Dividing by zero has no answer, so Jadpo rules it out while compiling.")
                            .next("Divide by a number other than `0`"),
                    );
                }
            }
            Operand::Value(ty) => {
                if self.excludes_zero(ty) {
                    return;
                }
                let why = match ty {
                    Ty::Prim(prim) => format!(
                        "`{written}` is a plain `{}`, which can be `0`.",
                        prim.name()
                    ),
                    other => format!("`{other}` allows `0`."),
                };
                self.out.push(
                    Diagnostic::new(
                        &rule::TYPE_DIVISOR_ZERO,
                        format!("`{written}` may be zero"),
                    )
                    .at(span, format!("a `{ty}` can be `0`"))
                    .reason(format!(
                        "Division by zero is ruled out while compiling: a divisor's type excludes zero, as `Int {{ min: 1 }}` does, and {why}"
                    ))
                    .next("Divide by a value whose type excludes zero, building it with its constructor where the zero case is handled"),
                );
            }
            Operand::Literal(_) => {}
        }
    }

    /// Whether a type's rules leave zero out: a `min` above it or a `max`
    /// below it, anywhere along its chain.
    fn excludes_zero(&self, ty: &Ty) -> bool {
        self.table.rule_chain(ty).iter().any(|(_, rules)| {
            rules.min.is_some_and(|min| min.mantissa > 0)
                || rules.max.is_some_and(|max| max.mantissa < 0)
        })
    }
}
