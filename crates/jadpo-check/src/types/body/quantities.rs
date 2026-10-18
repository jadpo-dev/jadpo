//! What the `units` operations and `text.format`'s `unit:` add to an
//! ordinary standard-library call (area 19, "Physical quantities"): a
//! `Quantity` parameter takes a value built on any quantity kind, a
//! `QuantityUnit` one a variant of a kind's unit enum, and the two must be
//! one kind, which the call's result then is.

use jadpo_diag::{Diagnostic, rule};
use jadpo_std::QuantityKind;
use jadpo_syntax::ast::{AstNode, Expr};

use super::super::ty::{Prim, Ty};
use super::Body;
use crate::tree;

impl Body<'_> {
    /// The kind a value given for a `Quantity` parameter is; reports any
    /// other value.
    pub(super) fn quantity_argument(
        &mut self,
        value: &Expr,
        given: &Ty,
        written: &str,
        param: &str,
    ) -> Option<QuantityKind> {
        if given.has_error() {
            return None;
        }
        if let Some(Prim::Quantity(kind)) = self.table.representation(given)
            && !self.table.is_nullable(given)
        {
            return Some(kind);
        }
        let span = self.span(value);
        self.out.push(
            Diagnostic::new(
                &rule::TYPE_MISMATCH,
                format!("`{given}` can't be used where a quantity is required"),
            )
            .at(span, format!("this is `{given}`"))
            .reason(format!(
                "`{written}`'s `{param}` takes a physical quantity, a value built on `Length`, `Mass` or another kind; `{given}` is {}.",
                super::super::literal::what(self.table, given)
            ))
            .next("Pass a quantity here"),
        );
        None
    }

    /// The kind a value given for a `QuantityUnit` parameter is a unit of:
    /// a variant of its unit enum, `LengthUnit.mile`. Reports any other
    /// value.
    pub(super) fn unit_argument(
        &mut self,
        value: &Expr,
        given: Option<Ty>,
        written: &str,
        param: &str,
    ) -> Option<QuantityKind> {
        let given = match given {
            Some(given) => given,
            None => self.infer(value),
        };
        if given.has_error() {
            return None;
        }
        if let Ty::Named(name) = given.strip_nullable()
            && !self.table.is_nullable(&given)
            && let Some(kind) = QuantityKind::of_unit_enum(name)
        {
            return Some(kind);
        }
        let span = self.span(value);
        self.out.push(
            Diagnostic::new(
                &rule::TYPE_MISMATCH,
                format!("`{given}` can't be used where a unit is required"),
            )
            .at(span, format!("this is `{given}`"))
            .reason(format!(
                "`{written}`'s `{param}` takes a unit, a variant of a quantity's unit enum such as `LengthUnit.mile`; `{given}` isn't one."
            ))
            .next("Pass a unit here, such as `LengthUnit.metre`"),
        );
        None
    }

    /// The kind of a list given to `units.sum`: its items built on one
    /// quantity kind that adds to itself. Reports any other value.
    pub(super) fn quantities_argument(
        &mut self,
        value: &Expr,
        given: &Ty,
        written: &str,
    ) -> Option<QuantityKind> {
        if given.has_error() {
            return None;
        }
        let item = match self.table.structure(given) {
            Ty::List(item) if !self.table.is_nullable(given) => Some(*item),
            _ => None,
        };
        let kind = item.as_ref().and_then(|item| {
            if self.table.is_nullable(item) {
                return None;
            }
            match self.table.representation(item) {
                Some(Prim::Quantity(kind)) => Some(kind),
                _ => None,
            }
        });
        let span = self.span(value);
        match kind {
            Some(kind) if kind.adds_to_itself() => Some(kind),
            Some(kind) => {
                let reason = if kind == QuantityKind::Temperature {
                    "Temperatures don't add: 20 °C and 20 °C isn't 40 °C, so a list of them has no total."
                } else {
                    "A `SoundLevel` is logarithmic, so sound levels don't add as numbers do, and a list of them has no total."
                };
                self.out.push(
                    Diagnostic::new(
                        &rule::TYPE_ARITHMETIC_OPERAND,
                        format!("`{given}` can't be added up"),
                    )
                    .at(span, format!("a list of `{}` values", kind.name()))
                    .reason(reason)
                    .next("Add up differences, or compare the values instead"),
                );
                None
            }
            None => {
                self.out.push(
                    Diagnostic::new(
                        &rule::TYPE_MISMATCH,
                        format!("`{given}` can't be used where a list of quantities is required"),
                    )
                    .at(span, format!("this is `{given}`"))
                    .reason(format!(
                        "`{written}` adds up a list of one kind of quantity; `{given}` isn't one."
                    ))
                    .next("Pass a list of quantities of one kind here"),
                );
                None
            }
        }
    }

    /// A unit of another kind than the call's quantity: `type.mismatch` at
    /// the unit.
    pub(super) fn unit_of_another_kind(
        &mut self,
        unit: &Expr,
        (unit_kind, quantity): (QuantityKind, QuantityKind),
        written: &str,
    ) {
        let shown = tree::written(unit.syntax());
        let span = self.span(unit);
        self.out.push(
            Diagnostic::new(
                &rule::TYPE_MISMATCH,
                format!(
                    "`{shown}` is a unit of `{}`, and the value is a `{}`",
                    unit_kind.name(),
                    quantity.name()
                ),
            )
            .at(span, format!("a `{}` unit", unit_kind.unit_enum()))
            .reason(format!(
                "`{written}` takes a unit of its value's kind: a `{}` is shown and read in a `{}`.",
                quantity.name(),
                quantity.unit_enum()
            ))
            .next(format!(
                "Pass a `{}`, such as `{}.{}`",
                quantity.unit_enum(),
                quantity.unit_enum(),
                quantity.base_unit().name
            )),
        );
    }
}
