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
use super::{Body, capital, is_literal, node_key};
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

/// What ordering applies to: numbers, and the standard types whose data
/// entry says they are ordered (area 19), the time values.
pub(super) fn ordered(prim: Prim) -> bool {
    prim.is_number() || jadpo_std::standard::of_builtin(prim).is_some_and(|entry| entry.ordered)
}

/// What `+` and `-` work on besides numbers and text (area 10).
fn is_time(prim: Prim) -> bool {
    matches!(
        prim,
        Prim::Instant | Prim::Duration | Prim::CalendarDate | Prim::ZonedTime
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
    /// Whether `ty` is a standard object type its data entry orders
    /// (`LocalTime`, `YearMonth`, area 19), compared part by part.
    pub(super) fn ordered_standard(&self, ty: &Ty) -> bool {
        match self.table.structure(ty) {
            Ty::Opaque(name, args) if args.is_empty() => {
                jadpo_std::standard_type(&name).is_some_and(|entry| entry.ordered)
            }
            _ => false,
        }
    }

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
    pub(super) fn same_type(&self, a: &Operand, b: &Operand) -> bool {
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
        let (a, b) = match op.as_str() {
            "==" | "!=" => self.compared(&lhs, &rhs),
            // The right side runs only where the left side holds, for
            // `and`, or doesn't, for `or`, and knows it (area 2,
            // "Conditions as proof").
            "and" | "or" => {
                let a = self.operand(&lhs);
                let (holds, fails) = self.split(&lhs);
                let left = if op == "and" { holds } else { fails };
                let b = self.knowing(&left, |body| body.operand(&rhs));
                (a, b)
            }
            _ => (self.operand(&lhs), self.operand(&rhs)),
        };
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

    /// Both sides of `==` or `!=`. A list written on one side is checked
    /// against a collection on the other, as a literal takes the other
    /// side's type, so `tags == ["a", "b"]` compares two sets when `tags`
    /// is a set.
    fn compared(&mut self, lhs: &Expr, rhs: &Expr) -> (Operand, Operand) {
        let written = |expr: &Expr| matches!(expr, Expr::ListExpr(_));
        if written(lhs) && !written(rhs) {
            let b = self.operand(rhs);
            let a = self.against(lhs, &b);
            return (a, b);
        }
        let a = self.operand(lhs);
        let b = if written(rhs) && !written(lhs) {
            self.against(rhs, &a)
        } else {
            self.operand(rhs)
        };
        (a, b)
    }

    /// A written list beside `other`: checked as `other`'s collection type
    /// when it is one.
    fn against(&mut self, list: &Expr, other: &Operand) -> Operand {
        match other {
            Operand::Value(ty)
                if matches!(
                    self.table.structure(ty),
                    Ty::List(_) | Ty::Set(_) | Ty::Map(..)
                ) =>
            {
                let ty = ty.strip_nullable().clone();
                self.check(list, &ty);
                Operand::Value(ty)
            }
            _ => self.operand(list),
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
            // A literal takes the other side's type, so text written for a
            // UUID, an instant or a date is read as one (area 3).
            for (literal, other, literal_expr) in [(a, b, lhs), (b, a, rhs)] {
                if let (Operand::Literal(_), Operand::Value(ty)) = (literal, other) {
                    self.facts
                        .types
                        .entry(node_key(literal_expr.syntax()))
                        .or_insert_with(|| ty.strip_nullable().clone());
                }
            }
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
                    _ if matches!(operand, Operand::Value(ty) if self.ordered_standard(ty)) => None,
                    Some(Prim::ZonedTime) => Some((
                        format!("{} has no order of its own", capital(&shown)),
                        format!("this is {shown}"),
                        "A `ZonedTime` is an instant and the zone it is seen in, and two in different zones have no one order; their instants do.".to_string(),
                        format!("Compare `temporal.instant({written})`"),
                    )),
                    _ if self.is_period(operand) => Some((
                        format!("{} has no order", capital(&shown)),
                        format!("this is {shown}"),
                        "A `CalendarPeriod` is years, months and days, which don't compare: one month and 30 days are neither more nor less than each other.".to_string(),
                        "Compare the periods with `==`, or the dates they reach".to_string(),
                    )),
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
        if self.is_period(a) || self.is_period(b) {
            return self.period_arithmetic(binary, op, a, b);
        }
        let (ra, rb) = (self.rep(a), self.rep(b));
        if [ra, rb].iter().any(|prim| prim.is_some_and(is_time)) {
            return self.time_arithmetic(binary, op, a, b);
        }
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
                        if matches!(a, Operand::Literal(_)) {
                            &sa
                        } else {
                            &sb
                        }
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

    /// `+` and `-` on time values (area 10, "Arithmetic and comparison"):
    /// an `Instant` moved by a `Duration` keeps the instant's named type
    /// (question 6), two instants of the same type give the `Duration`
    /// between them, and two durations of the same type, or one and a
    /// literal, give a plain `Duration`, as two numbers give a plain number.
    /// Whether an operand is a `CalendarPeriod` (area 19).
    fn is_period(&self, operand: &Operand) -> bool {
        matches!(operand, Operand::Value(ty) if matches!(
            self.table.structure(ty),
            Ty::Opaque(name, args) if name == jadpo_std::CALENDAR_PERIOD && args.is_empty()
        ))
    }

    /// The standard operator table's `CalendarPeriod` rows (area 19): a
    /// date moved by a period gives the date's own type, two periods add
    /// and subtract part by part, and a period times an `Int` multiplies
    /// each part. Anything else with a period is refused.
    fn period_arithmetic(&mut self, binary: &ast::BinExpr, op: &str, a: &Operand, b: &Operand) -> Ty {
        let period = || Ty::Opaque(jadpo_std::CALENDAR_PERIOD.to_string(), Vec::new());
        let (pa, pb) = (self.is_period(a), self.is_period(b));
        match (op, a, pa, pb) {
            ("+" | "-", Operand::Value(ty), false, true)
                if self.rep(a) == Some(Prim::CalendarDate) =>
            {
                return ty.strip_nullable().clone();
            }
            ("+" | "-", _, true, true) => return period(),
            ("*", _, true, false)
                if self.rep(b) == Some(Prim::Int)
                    && !matches!(b, Operand::Literal(Const::Number { decimal: true, .. })) =>
            {
                return period();
            }
            _ => {}
        }
        let (sa, sb) = (a.shown(), b.shown());
        let span = self.span(binary);
        let reason = if matches!(op, "+" | "-") && pa && self.rep(b) == Some(Prim::CalendarDate) {
            format!("A date is moved by a period with the date first, `day {op} period`, as the operator table writes it.")
        } else if op == "*" && pb && !pa {
            "A period is multiplied with the period first, `period * 2`, as the operator table writes it.".to_string()
        } else if self.rep(a) == Some(Prim::ZonedTime) {
            "A `ZonedTime` takes no `CalendarPeriod` by operator, since moving a local time by months would also need a `dst:` choice; `temporal.add_months` and its siblings take one.".to_string()
        } else {
            format!(
                "A `CalendarPeriod` moves a `CalendarDate` with `+` and `-`, adds to or subtracts another period, and is multiplied by an `Int`; {sa} and {sb} are none of those."
            )
        };
        self.out.push(
            Diagnostic::new(
                &rule::TYPE_ARITHMETIC_OPERAND,
                format!("{} and {} can't be {}", capital(&sa), sb, verb(op)),
            )
            .at(span, "not in the operator table")
            .reason(reason)
            .next("Move a date by a period, combine two periods, or multiply a period by an `Int`"),
        );
        Ty::Error
    }

    fn time_arithmetic(&mut self, binary: &ast::BinExpr, op: &str, a: &Operand, b: &Operand) -> Ty {
        let (ra, rb) = (self.rep(a), self.rep(b));
        let (sa, sb) = (a.shown(), b.shown());
        let span = self.span(binary);
        let instant_of = |operand: &Operand| match operand {
            Operand::Value(ty) => ty.strip_nullable().clone(),
            Operand::Literal(_) => Ty::Prim(Prim::Instant),
        };
        let same = self.same_type(a, b) || Self::unitless(a) || Self::unitless(b);
        match (op, ra, rb) {
            ("+" | "-", Some(Prim::Instant), Some(Prim::Duration)) => return instant_of(a),
            ("+", Some(Prim::Duration), Some(Prim::Instant)) => return instant_of(b),
            ("-", Some(Prim::Instant), Some(Prim::Instant)) if self.same_type(a, b) => {
                return Ty::Prim(Prim::Duration);
            }
            ("+" | "-", Some(Prim::Duration), Some(Prim::Duration)) if same => {
                return Ty::Prim(Prim::Duration);
            }
            _ => {}
        }
        let both = |prim: Prim| ra == Some(prim) && rb == Some(prim);
        let (label, reason, next) = if matches!(op, "+" | "-")
            && (both(Prim::Instant) || both(Prim::Duration))
        {
            (
                "different types",
                format!(
                    "`{op}` needs the same named type on both sides, or a literal, because {sa} and {sb} are different units."
                ),
                "Use two values of the same type".to_string(),
            )
        } else if [ra, rb].contains(&Some(Prim::CalendarDate)) {
            (
                "a date",
                format!(
                    "A `CalendarDate` has no clock time, so a `Duration` doesn't move it; {sa} and {sb} can't be {}.",
                    verb(op)
                ),
                "Move a date by calendar days with `temporal.add_days`".to_string(),
            )
        } else if [ra, rb].contains(&Some(Prim::ZonedTime)) {
            (
                "a local time",
                format!(
                    "A `ZonedTime` is moved on its calendar, by `temporal.add_days`, or by its instant, `temporal.instant(...)`; {sa} and {sb} can't be {}.",
                    verb(op)
                ),
                "Move it with `temporal.add_days`, or work on `temporal.instant(...)`".to_string(),
            )
        } else {
            (
                "not a time and a duration",
                format!(
                    "`+` and `-` move an `Instant` by a `Duration` and add or subtract durations, and `-` gives the `Duration` between two instants; {sa} and {sb} are none of those."
                ),
                format!("Use an instant and a duration, or two durations, with `{op}`"),
            )
        };
        let (reason, next) = if matches!(op, "*" | "/" | "%") {
            (
                format!("`{op}` works on numbers; time values are only added and subtracted."),
                "Work on numbers here".to_string(),
            )
        } else {
            (reason, next)
        };
        self.out.push(
            Diagnostic::new(
                &rule::TYPE_ARITHMETIC_OPERAND,
                format!("{} and {} can't be {}", capital(&sa), sb, verb(op)),
            )
            .at(span, label)
            .reason(reason)
            .next(next),
        );
        Ty::Error
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
