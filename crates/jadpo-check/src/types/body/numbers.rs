//! What area 19's number operations add to an ordinary standard-library
//! call ("Numbers"): `min` and `max` take two values of one type, as `+`
//! does, and give one of them, keeping its named type; `decimal.round`'s
//! `scale:` is a whole number written out, as a type's `scale` is.

use jadpo_diag::{Diagnostic, rule};
use jadpo_syntax::ast::{AstNode, Expr};

use super::super::rules::Const;
use super::super::ty::{Prim, Ty};
use super::ops::Operand;
use super::{Body, capital, is_literal, node_key};
use crate::tree;

/// The places a type's `scale` may keep, and so a rounding's.
const MOST_PLACES: i128 = 18;

impl Body<'_> {
    /// The type a call gives when it takes two or more values of the type
    /// it returns (`min(a: T, b: T) -> T`): the values must be one type
    /// after widening, as `+` needs, or a literal, which takes the other's
    /// type and is checked against it, or a plain number, which makes the
    /// result plain. `None` when the call has no such values, as
    /// `temporal`'s operations, each taking one.
    pub(super) fn shared_type(
        &mut self,
        table: &jadpo_std::Operation,
        given: &[(usize, Expr)],
        written: &str,
    ) -> Option<Ty> {
        let jadpo_std::Kind::OneOf(accepts) = table.result else {
            return None;
        };
        let shared: Vec<&Expr> = given
            .iter()
            .filter(|(slot, _)| {
                table
                    .params
                    .get(*slot)
                    .is_some_and(|param| param.kind == table.result)
            })
            .map(|(_, value)| value)
            .collect();
        let [first, second] = shared.as_slice() else {
            return None;
        };
        let fits = |prim: Option<Prim>| prim.is_some_and(|prim| accepts.contains(&prim));
        let mut operands = Vec::new();
        for value in [*first, *second] {
            let operand = if is_literal(value) {
                Operand::Literal(Const::from_expr(value))
            } else {
                match self.facts.types.get(&node_key(value.syntax())) {
                    Some(ty) => Operand::Value(ty.clone()),
                    None => return Some(Ty::Error),
                }
            };
            // What is wrong with one value alone is already reported.
            let alone = match &operand {
                Operand::Value(ty) => {
                    !ty.has_error()
                        && !self.table.is_nullable(ty)
                        && fits(self.table.representation(ty))
                }
                Operand::Literal(Const::Number { decimal, .. }) => {
                    fits(Some(if *decimal { Prim::Decimal } else { Prim::Int }))
                }
                Operand::Literal(_) => false,
            };
            if !alone {
                return Some(Ty::Error);
            }
            operands.push((value, operand));
        }
        let [(_, a), (second_value, b)] = operands.as_slice() else {
            return Some(Ty::Error);
        };
        let plain = accepts.first().map_or(Ty::Error, |prim| Ty::Prim(*prim));
        match (a, b) {
            (Operand::Literal(_), Operand::Literal(_)) => Some(plain),
            (Operand::Literal(_), Operand::Value(ty))
            | (Operand::Value(ty), Operand::Literal(_)) => {
                let literal = if matches!(a, Operand::Literal(_)) {
                    *first
                } else {
                    *second
                };
                let ty = ty.strip_nullable().clone();
                // The result may be the literal, so it must be a value of
                // the type the result keeps.
                self.literal(literal, &ty, false);
                Some(ty)
            }
            (Operand::Value(x), Operand::Value(y)) => {
                if !self.same_type(a, b) {
                    let span = self.span(*second_value);
                    self.out.push(
                        Diagnostic::new(
                            &rule::TYPE_MISMATCH,
                            format!(
                                "{} and `{y}` can't both be given to `{written}`",
                                capital(&format!("`{x}`"))
                            ),
                        )
                        .at(span, format!("this is `{y}`"))
                        .reason(format!(
                            "`{written}` gives one of its two values as it is, keeping its named type, so both are the same type after widening, as `+` needs, or one is a literal or a plain number; `{x}` and `{y}` are different types."
                        ))
                        .next("Pass two values of the same type"),
                    );
                    return Some(Ty::Error);
                }
                Some(if self.table.assignable(y, x) {
                    x.clone()
                } else if self.table.assignable(x, y) {
                    y.clone()
                } else {
                    plain
                })
            }
        }
    }

    /// `scale:` on `decimal.round`: a whole number of places from 0 to 18
    /// written at the call, never a value, so the places the result keeps
    /// are known while compiling (area 19, "Numbers").
    pub(super) fn written_out_argument(&mut self, value: &Expr, written: &str, param: &str) {
        let span = self.span(value);
        let shown = tree::written(value.syntax());
        if !is_literal(value) {
            if !self.facts.types.contains_key(&node_key(value.syntax())) {
                self.infer(value);
            }
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_MISMATCH,
                    format!("`{written}`'s `{param}:` is a whole number written out, not a value"),
                )
                .at(span, "a value")
                .reason(format!(
                    "The places a rounding keeps are read while compiling, as a type's `scale` is, so `{param}:` is a whole number from 0 to {MOST_PLACES} written at the call."
                ))
                .next(format!("Write the number of places itself, such as `{param}: 2`")),
            );
            return;
        }
        let literal = Const::from_expr(value);
        if matches!(literal, Const::Number { decimal: false, .. }) {
            self.check(value, &Ty::Prim(Prim::Int));
        }
        let problem = match literal {
            Const::Number {
                value: number,
                decimal: false,
            } if (0..=MOST_PLACES).contains(&number.mantissa) => None,
            Const::Number {
                value: number,
                decimal: false,
            } if number.mantissa < 0 => Some((
                &rule::TYPE_ARGUMENT_NEGATIVE,
                format!("`{written}`'s `{param}` is never negative"),
                "below `0`",
            )),
            _ => Some((
                &rule::TYPE_MISMATCH,
                format!("`{shown}` isn't a number of decimal places"),
                "not a whole number from 0 to 18",
            )),
        };
        let Some((code, message, label)) = problem else {
            return;
        };
        self.out.push(
            Diagnostic::new(code, message)
                .at(span, label)
                .reason(format!(
                    "`{written}`'s `{param}:` is how many decimal places the result keeps, a whole number from 0 to {MOST_PLACES} as a type's `scale` is; `{shown}` isn't one."
                ))
                .next(format!("Write a whole number from 0 to {MOST_PLACES}, such as `{param}: 2`")),
        );
    }
}
