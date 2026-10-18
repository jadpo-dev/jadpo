//! The rules ordering a standard object's parts (area 19, chunk 5.5m2),
//! proved while compiling where a literal writes both parts as numbers or
//! as quantities of one kind: a `Rating`'s value from its worst to its best,
//! its worst below its best, and a `BloodPressure`'s diastolic below its
//! systolic. A part built from a value waits on cross-field rules (chunk
//! 5.5t), as an address's postal code built from values does. The rules are
//! `jadpo-std`'s, which the runtime checks a value read from JSON by.

use std::cmp::Ordering;

use jadpo_diag::{Diagnostic, rule};
use jadpo_std::standard::order_rules;
use jadpo_syntax::ast::{self, Expr};

use super::super::rules::Const;
use super::Body;
use crate::tree;

/// The value a literal writes for part `part`, if it writes the part.
fn value_of(record: &ast::RecordExpr, part: &str) -> Option<Expr> {
    tree::members(record.member_list())
        .into_iter()
        .find(|(field, _)| tree::name_text(field).as_deref() == Some(part))
        .and_then(|(_, member)| member.value())
}

/// What a literal writes for a part, as something to compare: a number,
/// or a quantity in its kind's base unit.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Written {
    Number(i128, u32),
    Quantity(jadpo_std::QuantityKind, i128),
}

fn written(record: &ast::RecordExpr, part: &str) -> Option<Written> {
    match Const::from_expr(&value_of(record, part)?) {
        Const::Number { value, .. } => Some(Written::Number(value.mantissa, value.scale)),
        Const::Quantity(text) => jadpo_std::quantity::parse_literal(&text)
            .ok()
            .map(|(unit, amount)| Written::Quantity(unit.kind, amount)),
        _ => None,
    }
}

/// How two written parts compare, where they are alike.
fn compare(lower: Written, upper: Written) -> Option<Ordering> {
    match (lower, upper) {
        (Written::Number(a, a_scale), Written::Number(b, b_scale)) => {
            Some(jadpo_std::compare_exact(a, a_scale, b, b_scale))
        }
        (Written::Quantity(kind, a), Written::Quantity(other, b)) if kind == other => {
            Some(a.cmp(&b))
        }
        _ => None,
    }
}

impl Body<'_> {
    /// A standard object literal's rules ordering its parts, where it
    /// writes both parts each reads.
    pub(super) fn order_rules_hold(&mut self, name: &str, record: &ast::RecordExpr) {
        for rule in order_rules(name) {
            let (Some(lower), Some(upper)) =
                (written(record, rule.lower), written(record, rule.upper))
            else {
                continue;
            };
            if compare(lower, upper).is_none_or(|order| rule.holds(order)) {
                continue;
            }
            let Some(at) = value_of(record, rule.at) else {
                continue;
            };
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_INVALID_LITERAL,
                    format!(
                        "This `{name}`'s `{}` breaks a rule across its parts",
                        rule.at
                    ),
                )
                .at(self.span(&at), rule.label)
                .reason(rule.reason)
                .next(rule.next),
            );
            // One refusal for a literal: the rules share their parts.
            return;
        }
    }
}
