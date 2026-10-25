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
use super::currency::OneCurrency;
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
            let value = Const::from_expr(expr);
            // A suffix that names no unit is reported here, once, and the
            // operand is then an error the operators stay quiet about.
            if matches!(value, Const::Quantity(_)) && self.quantity_literal(expr, &value).is_error()
            {
                return Operand::Value(Ty::Error);
            }
            Operand::Literal(value)
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
            Operand::Literal(value @ Const::Quantity(_)) => {
                value.quantity().map(|(unit, _)| Prim::Quantity(unit.kind))
            }
            Operand::Literal(Const::Percentage(_)) => Some(Prim::Percentage),
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
                Err(literal::Problem::Kind { .. } | literal::Problem::PercentRequired(_))
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
        // A secret value is compared with `==` and `!=`, in constant time,
        // and nothing else (area 19, chunk 5.5r2): an order, a match or a
        // sum would make another value from it.
        if !matches!(op.as_str(), "==" | "!=" | "and" | "or") {
            for (side, operand) in [(&lhs, &a), (&rhs, &b)] {
                if let Operand::Value(ty) = operand
                    && let Some(secret) = super::super::secrets::secret_in(self.table, ty)
                {
                    let span = self.span(side);
                    self.out.push(super::super::secrets::made_from(
                        &format!("`{op}`"),
                        secret,
                        span,
                    ));
                    return if comparison {
                        Ty::Prim(Prim::Bool)
                    } else {
                        Ty::Error
                    };
                }
            }
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

    /// A bare number compared with a quantity, reported as
    /// `type.unit_required` (area 19); whether one was.
    fn bare_beside_quantity(
        &mut self,
        (lhs, a): (&Expr, &Operand),
        (rhs, b): (&Expr, &Operand),
    ) -> bool {
        for (side, operand, other) in [(lhs, a, b), (rhs, b, a)] {
            if let Some(Prim::Quantity(kind)) = self.rep(other)
                && self.rep(operand).is_some_and(Prim::is_number)
                && !self.nullable(operand)
            {
                let written = tree::written(side.syntax());
                let span = self.span(side);
                self.out
                    .push(literal::unit_required(kind, &written, kind.name(), span));
                return true;
            }
        }
        false
    }

    /// A bare number compared with a percentage (area 19): it could be a
    /// ratio or a percentage already, so the next step names both; whether
    /// one was.
    fn bare_beside_percentage(
        &mut self,
        binary: &ast::BinExpr,
        rule: &'static jadpo_diag::Rule,
        done: &str,
        (lhs, a): (&Expr, &Operand),
        (rhs, b): (&Expr, &Operand),
    ) -> bool {
        for (side, operand, other) in [(lhs, a, b), (rhs, b, a)] {
            if let (Operand::Literal(Const::Number { value, .. }), Some(Prim::Percentage)) =
                (operand, self.rep(other))
            {
                let written = tree::written(side.syntax());
                let span = self.span(binary);
                self.out.push(
                    Diagnostic::new(
                        rule,
                        format!(
                            "{} and `{written}` can't be {done}: `{written}` is a bare number",
                            capital(&other.shown())
                        ),
                    )
                    .at(span, "a percentage and a bare number")
                    .reason(format!(
                        "A percentage is written with `%`, so `{written}` could be read two ways: as a ratio, where 0.2 is 20%, or as a percentage already, where 20 is 20%."
                    ))
                    .next(literal::percent_readings(*value, &written)),
                );
                return true;
            }
        }
        false
    }

    fn equality(
        &mut self,
        binary: &ast::BinExpr,
        op: &str,
        (lhs, a): (&Expr, &Operand),
        (rhs, b): (&Expr, &Operand),
    ) {
        if self.bare_beside_quantity((lhs, a), (rhs, b))
            || self.bare_beside_percentage(
                binary,
                &rule::TYPE_INCOMPARABLE,
                "compared",
                (lhs, a),
                (rhs, b),
            )
        {
            return;
        }
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
        if self.bare_beside_quantity((lhs, a), (rhs, b))
            || self.bare_beside_percentage(
                binary,
                &rule::TYPE_ORDERING_OPERAND,
                "ordered",
                (lhs, a),
                (rhs, b),
            )
        {
            return;
        }
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
        if self.rep(a) == Some(Prim::Money)
            && self.rep(b) == Some(Prim::Money)
            && let OneCurrency::No(x, y) = self.one_currency(lhs, rhs)
        {
            self.different_currencies(
                binary,
                &rule::TYPE_ORDERING_OPERAND,
                "ordered",
                (a, x),
                (b, y),
            );
            return;
        }
        if self.same_type(a, b) {
            if self.rep(a) == Some(Prim::Money) && self.rep(b) == Some(Prim::Money) {
                self.one_currency_or_mapped(binary, op, lhs, rhs);
            }
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
        if [ra, rb].contains(&Some(Prim::Money)) {
            return self.money_arithmetic(binary, op, (lhs, a), (rhs, b));
        }
        if [ra, rb]
            .iter()
            .any(|prim| matches!(prim, Some(Prim::Quantity(_))))
        {
            return self.quantity_arithmetic(binary, op, (lhs, a), (rhs, b));
        }
        if [ra, rb].contains(&Some(Prim::Percentage)) {
            return self.percentage_arithmetic(binary, op, (lhs, a), (rhs, b));
        }
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

    /// The operator table's quantity rows (area 19, "Physical quantities"):
    /// `+` and `-` within one kind (but a `Temperature`, which adds only a
    /// `TemperatureDifference`, and a `SoundLevel`, which never adds), `*`
    /// and `/` by a plain number keeping the kind, one kind over the same
    /// kind giving a `Decimal`, and the products and quotients the table
    /// defines; any other mix is refused naming the two kinds. Area 3's
    /// named-type rule holds for `+`, `-` and `/` of one kind, and the
    /// quantity is written first.
    fn quantity_arithmetic(
        &mut self,
        binary: &ast::BinExpr,
        op: &str,
        (lhs, a): (&Expr, &Operand),
        (rhs, b): (&Expr, &Operand),
    ) -> Ty {
        use jadpo_std::quantity::{self, Side};
        let (ra, rb) = (self.rep(a), self.rep(b));
        let (sa, sb) = (a.shown(), b.shown());
        let span = self.span(binary);
        let side = |prim: Option<Prim>| match prim {
            Some(Prim::Quantity(kind)) => Some(Side::Quantity(kind)),
            Some(Prim::Duration) => Some(Side::Duration),
            _ => None,
        };
        let of = |side: Side| match side {
            Side::Quantity(kind) => Ty::Prim(Prim::Quantity(kind)),
            Side::Duration => Ty::Prim(Prim::Duration),
        };
        let refuse =
            |body: &mut Self, message: String, label: &str, reason: String, next: String| {
                body.out.push(
                    Diagnostic::new(&rule::TYPE_ARITHMETIC_OPERAND, message)
                        .at(span.clone(), label.to_string())
                        .reason(reason)
                        .next(next),
                );
                Ty::Error
            };
        let symbol = op.chars().next().unwrap_or('+');
        if let (Some(left), Some(right)) = (side(ra), side(rb))
            && let Some(gives) = quantity::product(left, symbol, right)
        {
            if op == "/" {
                self.divisor(rhs, b);
            }
            return of(gives);
        }
        let number = |prim: Option<Prim>| prim.is_some_and(Prim::is_number);
        match (op, ra, rb) {
            ("+" | "-" | "/", Some(Prim::Quantity(x)), Some(Prim::Quantity(y))) if x == y => {
                if op != "/" && !x.adds_to_itself() {
                    let reason = if x == jadpo_std::QuantityKind::Temperature {
                        "Temperatures don't add: 20 °C and 20 °C isn't 40 °C. A `Temperature` adds only a `TemperatureDifference`, and two temperatures subtract to one.".to_string()
                    } else {
                        "A `SoundLevel` is logarithmic, so two don't add as numbers do: it compares, but never adds.".to_string()
                    };
                    return refuse(
                        self,
                        format!("{} and {sb} can't be {}", capital(&sa), verb(op)),
                        &format!("two `{}` values", x.name()),
                        reason,
                        "Compare the values, or add a difference to a temperature".to_string(),
                    );
                }
                if !Self::unitless(a) && !Self::unitless(b) && !self.same_type(a, b) {
                    return refuse(
                        self,
                        format!("{} and {sb} can't be {}", capital(&sa), verb(op)),
                        "different types",
                        format!(
                            "`{op}` needs the same named type on both sides, or a plain value another operator or a `units` operation made; {sa} and {sb} are different types."
                        ),
                        "Use two values of the same type".to_string(),
                    );
                }
                if op == "/" {
                    self.divisor(rhs, b);
                    return Ty::Prim(Prim::Decimal);
                }
                Ty::Prim(Prim::Quantity(x))
            }
            ("*", Some(Prim::Quantity(x)), other)
                if number(other) || other == Some(Prim::Percentage) =>
            {
                Ty::Prim(Prim::Quantity(x))
            }
            ("/", Some(Prim::Quantity(x)), other) if number(other) => {
                self.divisor(rhs, b);
                Ty::Prim(Prim::Quantity(x))
            }
            ("*", other, Some(Prim::Quantity(_)))
                if number(other) || other == Some(Prim::Percentage) =>
            {
                let (left, right) = (tree::written(lhs.syntax()), tree::written(rhs.syntax()));
                self.out.push(
                    Diagnostic::new(
                        &rule::TYPE_ARITHMETIC_OPERAND,
                        format!("{} and {sb} can't be multiplied this way round", capital(&sa)),
                    )
                    .at(span.clone(), "the number first")
                    .reason(format!(
                        "The standard operator table multiplies a quantity by a number or a percentage, the quantity first, as it does money: `{right} * {left}`."
                    ))
                    .next(format!("Write `{right} * {left}`"))
                    .edit(span, format!("{right} * {left}")),
                );
                Ty::Error
            }
            ("+" | "-", Some(Prim::Quantity(kind)), other)
            | ("+" | "-", other, Some(Prim::Quantity(kind)))
                if number(other) =>
            {
                let (bare, bare_expr) = if number(ra) { (a, lhs) } else { (b, rhs) };
                let _ = bare;
                let written = tree::written(bare_expr.syntax());
                let span = self.span(bare_expr);
                self.out.push(super::super::literal::unit_required(
                    kind,
                    &written,
                    kind.name(),
                    span,
                ));
                Ty::Error
            }
            _ => {
                let kind_of = |prim: Option<Prim>, shown: &str| match prim {
                    Some(prim @ (Prim::Quantity(_) | Prim::Duration)) => {
                        let name = prim.name();
                        let an = if name.starts_with(['A', 'E', 'I', 'O', 'U']) {
                            "an"
                        } else {
                            "a"
                        };
                        format!("{an} `{name}`")
                    }
                    Some(prim) => format!("a `{}`", prim.name()),
                    None => shown.to_string(),
                };
                let (ka, kb) = (kind_of(ra, &sa), kind_of(rb, &sb));
                let (headline, reason) = match op {
                    "+" | "-" => (
                        format!("{ka} and {kb} are different kinds"),
                        format!(
                            "`{op}` works within one kind of quantity, in any of its units; {ka} and {kb} are different kinds, so the sum means nothing."
                        ),
                    ),
                    "*" => (
                        format!("{ka} times {kb} is no quantity"),
                        format!(
                            "Two kinds multiply only as the standard operator table defines (`Length * Length` is an `Area`, `Speed * Duration` a `Length`); {ka} times {kb} isn't one of its rows."
                        ),
                    ),
                    "/" => (
                        format!("{ka} over {kb} is no quantity"),
                        format!(
                            "Two kinds divide only as the standard operator table defines (`Length / Duration` is a `Speed`), and one kind over the same kind is a plain `Decimal`; {ka} over {kb} isn't one of its rows."
                        ),
                    ),
                    _ => (
                        "`%` takes whole numbers".to_string(),
                        format!(
                            "`%` gives the remainder of dividing whole numbers; {sa} and {sb} aren't."
                        ),
                    ),
                };
                refuse(
                    self,
                    format!(
                        "{} and {sb} can't be {}: {headline}",
                        capital(&sa),
                        verb(op)
                    ),
                    "different kinds",
                    reason,
                    "Use values of one kind, or a product or quotient the table defines"
                        .to_string(),
                )
            }
        }
    }

    /// The operator table's `Percentage` rows (area 19, "The standard
    /// operator table") besides money's and quantities': two percentages
    /// add, subtract and multiply to a percentage (area 3's named-type
    /// rule holding for `+` and `-`), a percentage times a number is a
    /// percentage, and a number times a percentage keeps the number's kind,
    /// a `Decimal`, as `*` by a `Decimal` gives (area 3). Anything else is
    /// refused: no row divides a percentage or adds one to a number.
    fn percentage_arithmetic(
        &mut self,
        binary: &ast::BinExpr,
        op: &str,
        (lhs, a): (&Expr, &Operand),
        (rhs, b): (&Expr, &Operand),
    ) -> Ty {
        let (ra, rb) = (self.rep(a), self.rep(b));
        let (sa, sb) = (a.shown(), b.shown());
        let span = self.span(binary);
        let pct = Some(Prim::Percentage);
        let number = |prim: Option<Prim>| prim.is_some_and(Prim::is_number);
        let refuse = |body: &mut Self, label: &str, reason: String, next: String| {
            body.out.push(
                Diagnostic::new(
                    &rule::TYPE_ARITHMETIC_OPERAND,
                    format!("{} and {} can't be {}", capital(&sa), sb, verb(op)),
                )
                .at(span.clone(), label.to_string())
                .reason(reason)
                .next(next),
            );
            Ty::Error
        };
        match (op, ra, rb) {
            ("+" | "-", x, y) if x == pct && y == pct => {
                if !Self::unitless(a) && !Self::unitless(b) && !self.same_type(a, b) {
                    return refuse(
                        self,
                        "different types",
                        format!(
                            "`{op}` needs the same named type on both sides, or a plain percentage a literal or another operator made; {sa} and {sb} are different types."
                        ),
                        "Use two percentages of the same type".to_string(),
                    );
                }
                Ty::Prim(Prim::Percentage)
            }
            ("*", x, y) if x == pct && (y == pct || number(y)) => Ty::Prim(Prim::Percentage),
            ("*", x, y) if number(x) && y == pct => Ty::Prim(Prim::Decimal),
            ("+" | "-", x, y) if (x == pct && number(y)) || (number(x) && y == pct) => {
                let (bare, bare_expr) = if number(ra) { (a, lhs) } else { (b, rhs) };
                match bare {
                    Operand::Literal(Const::Number { value, .. }) => {
                        let written = tree::written(bare_expr.syntax());
                        refuse(
                            self,
                            "a bare number",
                            format!(
                                "A percentage adds only a percentage, and `{written}` is a bare number, which could be read two ways: as a ratio, where 0.2 is 20%, or as a percentage already, where 20 is 20%."
                            ),
                            literal::percent_readings(*value, &written),
                        )
                    }
                    _ => refuse(
                        self,
                        "a number and a percentage",
                        format!(
                            "A percentage adds only a percentage; {} is a number. A number is multiplied by a percentage with `*`.",
                            bare.shown()
                        ),
                        "Multiply the number by the percentage, or add two percentages".to_string(),
                    ),
                }
            }
            ("/", x, _) if x == pct => refuse(
                self,
                "a percentage divided",
                "The standard operator table multiplies by a percentage and adds percentages; no row divides one, which would leave the reader to work out what the result is.".to_string(),
                format!(
                    "Multiply instead, such as `{} * 50%` to halve it",
                    tree::written(lhs.syntax())
                ),
            ),
            _ => refuse(
                self,
                "not in the operator table",
                format!(
                    "A percentage adds to and subtracts from a percentage, and is multiplied by a number or a percentage, or scales a number, an amount of money or a quantity written before it; {sa} and {sb} are none of those."
                ),
                "Use an operator the standard operator table gives percentages".to_string(),
            ),
        }
    }

    /// The operator table's `Money` rows (area 19, "The standard operator
    /// table"): `+` and `-` of two amounts in one currency, `*` by a
    /// number, `/` by a `Decimal` rate, each giving a plain `Money`, and
    /// `/` of two amounts in one currency giving a `Decimal`. `/` by an
    /// `Int` is `type.money_split`; anything else is refused.
    fn money_arithmetic(
        &mut self,
        binary: &ast::BinExpr,
        op: &str,
        (lhs, a): (&Expr, &Operand),
        (rhs, b): (&Expr, &Operand),
    ) -> Ty {
        let (ra, rb) = (self.rep(a), self.rep(b));
        let (sa, sb) = (a.shown(), b.shown());
        let span = self.span(binary);
        let money = Some(Prim::Money);
        let number = |prim: Option<Prim>| prim.is_some_and(Prim::is_number);
        let refuse = |body: &mut Self, label: &str, reason: String, next: String| {
            body.out.push(
                Diagnostic::new(
                    &rule::TYPE_ARITHMETIC_OPERAND,
                    format!("{} and {} can't be {}", capital(&sa), sb, verb(op)),
                )
                .at(span.clone(), label.to_string())
                .reason(reason)
                .next(next),
            );
            Ty::Error
        };
        match (op, ra == money, rb == money) {
            ("+" | "-" | "/", true, true) => {
                if let OneCurrency::No(x, y) = self.one_currency(lhs, rhs) {
                    self.different_currencies(
                        binary,
                        &rule::TYPE_ARITHMETIC_OPERAND,
                        verb(op),
                        (a, x),
                        (b, y),
                    );
                    return Ty::Error;
                }
                if !Self::unitless(a) && !Self::unitless(b) && !self.same_type(a, b) {
                    return refuse(
                        self,
                        "different types",
                        format!(
                            "`{op}` needs the same named type on both sides, or a plain amount another operator or `money` operation made, because {sa} and {sb} are different kinds of money."
                        ),
                        "Use two amounts of the same type".to_string(),
                    );
                }
                if op == "/" {
                    self.divisor(rhs, b);
                }
                self.one_currency_or_mapped(binary, op, lhs, rhs);
                if op == "/" {
                    Ty::Prim(Prim::Decimal)
                } else {
                    Ty::Prim(Prim::Money)
                }
            }
            ("*", true, false) if number(rb) || rb == Some(Prim::Percentage) => {
                Ty::Prim(Prim::Money)
            }
            // Adding a percentage to money could mean two things (area
            // 19): the fix writes each.
            ("+" | "-", true, false) if rb == Some(Prim::Percentage) => {
                let amount_text = tree::written(lhs.syntax());
                let rate_text = tree::written(rhs.syntax());
                let as_whole = match b {
                    Operand::Literal(value) => value.percentage().and_then(|fraction| {
                        let whole = jadpo_std::Num {
                            mantissa: 10i128.checked_pow(fraction.scale)?,
                            scale: fraction.scale,
                        };
                        let mantissa = if op == "+" {
                            whole.mantissa.checked_add(fraction.mantissa)?
                        } else {
                            whole.mantissa.checked_sub(fraction.mantissa)?
                        };
                        Some(jadpo_std::percentage::canonical(mantissa, fraction.scale))
                    }),
                    Operand::Value(_) => None,
                }
                .unwrap_or_else(|| format!("(100% {op} {rate_text})"));
                let span = span.clone();
                self.out.push(
                    Diagnostic::new(
                        &rule::TYPE_ARITHMETIC_OPERAND,
                        format!(
                            "{} and {} can't be {}: it could mean two things",
                            capital(&sa),
                            sb,
                            verb(op)
                        ),
                    )
                    .at(span, "an amount and a percentage")
                    .reason(format!(
                        "`{amount_text} {op} {rate_text}` could mean {} by {rate_text} of itself, or {rate_text} of some other amount; the standard operator table refuses it rather than guess. A percentage scales money by `*`."
                    , if op == "+" { "the amount raised" } else { "the amount lowered" }))
                    .next(format!(
                        "Write `{amount_text} * {as_whole}`, or `{amount_text} {op} {amount_text} * {rate_text}`, whichever is meant"
                    )),
                );
                Ty::Error
            }
            ("/", true, false) if rb == Some(Prim::Decimal) => {
                self.divisor(rhs, b);
                Ty::Prim(Prim::Money)
            }
            ("/", true, false) if rb == Some(Prim::Int) => {
                let total = tree::written(lhs.syntax());
                let parts = tree::written(rhs.syntax());
                let split = format!("money.split({total}, {parts})");
                self.out.push(
                    Diagnostic::new(
                        &rule::TYPE_MONEY_SPLIT,
                        format!("{} is divided by a count", capital(&sa)),
                    )
                    .at(span.clone(), "money shared out by `/`")
                    .reason(format!(
                        "Dividing money by a count is sharing it out, and rounding each share loses minor units: £10.00 / 3 is £3.33, three times £9.99. `money.split` hands every minor unit to some share, and `/` takes only a `Decimal` rate, as `/ 1.20` takes a 20% tax off; `{parts}` is a whole number."
                    ))
                    .next(format!(
                        "Share it with `{split}`, which gives a list of shares adding up to the total, or `money.allocate` for unequal shares; halve an amount with `* 0.5`"
                    ))
                    .edit(span, split),
                );
                Ty::Error
            }
            (_, false, true)
                if number(ra)
                    || ra == Some(Prim::Percentage)
                    || matches!(a, Operand::Literal(_)) =>
            {
                refuse(
                self,
                "a number and an amount",
                format!(
                    "Amounts of money are written first: `*` scales an amount by a number and `/` divides it by a rate, and {sa} is not an amount."
                ),
                if op == "*" {
                    format!(
                        "Write the amount first: `{} * {}`",
                        tree::written(rhs.syntax()),
                        tree::written(lhs.syntax())
                    )
                } else {
                    "Use an amount on the left of the operator".to_string()
                },
            )
            }
            ("*", true, true) => refuse(
                self,
                "two amounts",
                "`*` scales an amount of money by a number; one amount times another means nothing."
                    .to_string(),
                "Multiply the amount by a number".to_string(),
            ),
            _ => refuse(
                self,
                "not an operator money takes",
                format!(
                    "An amount of money is added to and subtracted from an amount in the same currency, multiplied by a number and divided by a `Decimal` rate or by an amount in the same currency; {sa} and {sb} are none of those."
                ),
                "Use an operator the standard operator table gives money".to_string(),
            ),
        }
    }

    /// Two amounts in currencies their types or how they were made fix
    /// differently, given to `op`: a compile error (area 19).
    fn different_currencies(
        &mut self,
        binary: &ast::BinExpr,
        rule: &'static jadpo_diag::Rule,
        done: &str,
        (a, x): (&Operand, u32),
        (b, y): (&Operand, u32),
    ) {
        let code = |ordinal: u32| jadpo_std::money::code(ordinal).unwrap_or("?");
        let (sa, sb) = (a.shown(), b.shown());
        self.out.push(
            Diagnostic::new(
                rule,
                format!("{} and {} can't be {done}: they're in different currencies", capital(&sa), sb),
            )
            .at(self.span(binary), format!("{} and {}", code(x), code(y)))
            .reason(format!(
                "{} is in {} and {} in {}, as their types or the calls that made them fix; amounts are {done} only in one currency.",
                capital(&sa),
                code(x),
                sb,
                code(y)
            ))
            .next("Use two amounts in the same currency"),
        );
    }

    /// `+`, `-`, `/` or an ordering of two amounts not known to differ:
    /// plain where one currency is known, otherwise an operation that can
    /// fail with `currency_mismatch`, which an `attempt` around it maps.
    pub(super) fn one_currency_or_mapped(
        &mut self,
        binary: &ast::BinExpr,
        op: &str,
        lhs: &Expr,
        rhs: &Expr,
    ) {
        let key = node_key(binary.syntax());
        match self.one_currency(lhs, rhs) {
            OneCurrency::Yes(by) => {
                if !by.is_empty() {
                    self.facts
                        .proofs
                        .insert(key, vec![("currency_mismatch".to_string(), by)]);
                }
                return;
            }
            OneCurrency::No(..) => return,
            OneCurrency::Unknown => {}
        }
        self.facts.fallible_ops.insert(key);
        if self.handled.contains(&key) {
            return;
        }
        let written = tree::written(binary.syntax());
        let (left, right) = (tree::written(lhs.syntax()), tree::written(rhs.syntax()));
        self.flow.open = true;
        self.out.push(
            Diagnostic::new(
                &rule::FAILURE_ATTEMPT_REQUIRED,
                format!("`{written}` can fail at runtime, and nothing handles it"),
            )
            .at(self.span(binary), "the currencies are known only when it runs")
            .reason(format!(
                "`{left}` and `{right}` are amounts whose currencies are known only when the program runs, so `{op}` fails with `currency_mismatch` when they differ."
            ))
            .next(format!(
                "Write `attempt {written} {{ currency_mismatch: SomeFailure }}`, or `{op}` them inside `if (money.currency({left}) == money.currency({right}))`"
            )),
        );
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
    fn period_arithmetic(
        &mut self,
        binary: &ast::BinExpr,
        op: &str,
        a: &Operand,
        b: &Operand,
    ) -> Ty {
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
            format!(
                "A date is moved by a period with the date first, `day {op} period`, as the operator table writes it."
            )
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
            // A quantity or a duration written as zero (area 19).
            Operand::Literal(value @ Const::Quantity(_))
                if value.quantity().is_some_and(|(_, amount)| amount == 0) =>
            {
                self.out.push(
                    Diagnostic::new(&rule::TYPE_DIVISOR_ZERO, "This divides by zero")
                        .at(span, "zero")
                        .reason("Dividing by zero has no answer, so Jadpo rules it out while compiling.")
                        .next("Divide by a quantity other than zero"),
                );
            }
            Operand::Literal(_) => {}
        }
    }

    /// Whether a type's rules leave zero out: a `min` above it or a `max`
    /// below it, anywhere along its chain, a duration's included.
    fn excludes_zero(&self, ty: &Ty) -> bool {
        self.table.rule_chain(ty).iter().any(|(_, rules)| {
            rules.min.is_some_and(|min| min.mantissa > 0)
                || rules.max.is_some_and(|max| max.mantissa < 0)
                || rules.min_duration.is_some_and(|min| min.millis > 0)
                || rules.max_duration.is_some_and(|max| max.millis < 0)
        })
    }
}
