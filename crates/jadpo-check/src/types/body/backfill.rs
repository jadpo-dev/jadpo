//! A computed backfill (area 5, "Computed backfills"; chunk 5.6b): the
//! expression a migration's `changes.jadpo` gives the rows already stored,
//! checked as a `return` of the field it fills. It reads its row's stored
//! fields (`Todo.legacy_flag`), those this migration retires included,
//! constants and pure functions, and nothing else; and nothing in it may
//! fail, since area 5 doesn't say what a row whose value failed gets.

use jadpo_diag::{Diagnostic, Span, rule};
use jadpo_syntax::ast::{self, AstNode};

use super::{BackfillTarget, Body, Fallible};

impl Body<'_> {
    pub(super) fn backfill(&mut self, target: BackfillTarget) {
        let BackfillTarget {
            entity,
            ty,
            value,
            retired,
            added,
        } = target;
        self.stored = Some(entity);
        self.retired = retired;
        self.added = added;
        self.result = ty.clone();
        self.check(&value, &ty);
        // `attempt` passes a failure on, and a backfill has nowhere to
        // pass it.
        if let Some((failure, (span, _))) = self
            .flow
            .leaving
            .iter()
            .next()
            .map(|(f, at)| (f.clone(), at.clone()))
        {
            self.backfill_can_fail(span, format!("fails with `{failure}`"));
        }
    }

    /// The error for a backfill that can fail, at what fails.
    pub(super) fn backfill_can_fail(&mut self, span: Span, label: String) {
        let key = self.flow.name.clone();
        self.out.push(
            Diagnostic::new(
                &rule::MIGRATION_BACKFILL_CAN_FAIL,
                format!("The backfill for `{key}` can fail"),
            )
            .at(span, label)
            .reason(format!(
                "A backfill gives every `{}` record already stored a value, and a record whose value failed would have none, so nothing in a backfill may fail.",
                key.split('.').next().unwrap_or_default()
            ))
            .next("Work the value out with operations that can't fail, such as `integer.min` and `integer.max` to keep a number in its range"),
        );
    }

    /// A call nothing handles in a backfill, which may fail: the backfill
    /// can fail. `true` when it was one.
    pub(super) fn backfill_unhandled(&mut self, call: &ast::CallExpr) -> bool {
        if self.context != super::Context::Backfill {
            return false;
        }
        let label = match self.call_fallible(call) {
            Fallible::Fails(names) => {
                let listed: Vec<String> = names.iter().map(|n| format!("`{n}`")).collect();
                format!("can fail with {}", crate::and_list(&listed))
            }
            Fallible::Construction(rules) => {
                let written = call
                    .expr()
                    .map(|callee| crate::tree::written(callee.syntax()))
                    .unwrap_or_default();
                let listed: Vec<String> = rules.iter().map(|r| format!("`{r}`")).collect();
                format!("can break `{written}`'s {}", crate::and_list(&listed))
            }
            Fallible::Never | Fallible::Unknown => return true,
        };
        let span = self.span(call);
        self.flow.open = true;
        self.backfill_can_fail(span, label);
        true
    }

    /// The error for a backfill that reads more than its row: another
    /// entity's stored field, or a setting.
    pub(super) fn backfill_outside_row(&mut self, span: Span, written: &str, label: &str) {
        let key = self.flow.name.clone();
        self.out.push(crate::data::backfill::outside_row(
            &key, written, span, label,
        ));
    }

    /// The error for a backfill that reads a field the same migration
    /// adds, which no stored record has a value for yet (chunk 5.8b).
    pub(super) fn backfill_reads_added(&mut self, span: Span, written: &str) {
        let key = self.flow.name.clone();
        let entity = key.split('.').next().unwrap_or_default().to_string();
        self.out.push(
            crate::data::backfill::outside_row(&key, written, span, "a field this migration adds")
                .reason(format!(
                    "`{written}` is added by this same migration, so no `{entity}` record already stored has a value for it while the backfill runs."
                ))
                .next(format!(
                    "Work the value out from the fields `{entity}` had before this migration, or a constant"
                )),
        );
    }

    /// The type of a field this migration retires, read by a backfill of
    /// its entity, if `field` is one.
    pub(super) fn retired_field(&self, field: &str) -> Option<super::Ty> {
        self.retired
            .iter()
            .find(|(name, _)| name == field)
            .map(|(_, ty)| ty.clone())
    }
}
