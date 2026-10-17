//! What area 19's text operations add to an ordinary standard-library call
//! ("Text"): text that is never empty is proved so, as a divisor's zero is
//! ruled out; `fill:` is one character written out; `text.join` takes a
//! list of any text; and `list.sort_by`'s `locale:` orders text keys only.

use jadpo_diag::{Diagnostic, rule};
use jadpo_syntax::ast::{self, AstNode, Expr};

use super::super::rules::Const;
use super::super::ty::{Prim, Ty};
use super::{Body, is_literal, node_key};
use crate::tree;

impl Body<'_> {
    /// Text that is never empty (area 19, "What every operation follows"):
    /// a literal isn't `""` and a value's type has a `min_length` of at
    /// least 1, so `text.replace` never searches for nothing.
    pub(super) fn never_empty(
        &mut self,
        table: &jadpo_std::Operation,
        args: &[ast::Arg],
        slots: &[Option<usize>],
        written: &str,
    ) {
        for (arg, slot) in args.iter().zip(slots) {
            let (Some(value), Some(slot)) = (arg.expr(), slot) else {
                continue;
            };
            let Some(param) = table.params.get(*slot).filter(|p| p.never_empty) else {
                continue;
            };
            let span = self.span(&value);
            let shown = tree::written(value.syntax());
            // A list that is never empty: `money.allocate`'s ratios (area
            // 19), a list written out with items, or a list type with a
            // `min_length` of at least 1.
            if param.kind == jadpo_std::Kind::List {
                let why = match &value {
                    Expr::ListExpr(list) if list.exprs().next().is_none() => {
                        format!("`{shown}` has no items.")
                    }
                    Expr::ListExpr(_) => continue,
                    _ => match self.facts.types.get(&node_key(value.syntax())).cloned() {
                        Some(ty)
                            if !ty.has_error()
                                && !self.table.is_nullable(&ty)
                                && matches!(self.table.structure(&ty), Ty::List(_)) =>
                        {
                            if self.never_empty_type(&ty) {
                                continue;
                            }
                            format!("`{ty}` allows an empty list.")
                        }
                        _ => continue,
                    },
                };
                self.out.push(
                    Diagnostic::new(
                        &rule::TYPE_ARGUMENT_EMPTY,
                        format!("`{written}`'s `{}` is never empty", param.name),
                    )
                    .at(span, "may be empty")
                    .reason(format!(
                        "`{written}`'s `{}` is never empty, which is proved while compiling: a list written out has items and a list type has a `min_length` of at least 1; {why}",
                        param.name
                    ))
                    .next("Pass a list whose type has `min_length: 1`, building it where an empty one is handled"),
                );
                continue;
            }
            let (message, label, why) = if is_literal(&value) {
                match Const::from_expr(&value) {
                    Const::Str(text) if text.is_empty() => (
                        format!("`{written}`'s `{}` is never empty", param.name),
                        "empty".to_string(),
                        format!("`{shown}` is empty."),
                    ),
                    _ => continue,
                }
            } else {
                match self.facts.types.get(&node_key(value.syntax())).cloned() {
                    // A value that isn't text is already reported.
                    Some(ty)
                        if !ty.has_error()
                            && !self.table.is_nullable(&ty)
                            && self.table.representation(&ty) == Some(Prim::Text) =>
                    {
                        if self.never_empty_type(&ty) {
                            continue;
                        }
                        let why = match &ty {
                            Ty::Prim(prim) => {
                                format!(
                                    "`{shown}` is a plain `{}`, which can be empty.",
                                    prim.name()
                                )
                            }
                            other => format!("`{other}` allows empty text."),
                        };
                        (
                            format!("`{shown}` may be empty"),
                            format!("`{ty}` can be empty"),
                            why,
                        )
                    }
                    _ => continue,
                }
            };
            self.out.push(
                Diagnostic::new(&rule::TYPE_ARGUMENT_EMPTY, message)
                    .at(span, label)
                    .reason(format!(
                        "`{written}`'s `{}` is never empty, which is proved while compiling: a literal isn't `\"\"` and a value's type has a `min_length` of at least 1; {why}",
                        param.name
                    ))
                    .next("Pass a value whose type has `min_length: 1`, building it where empty text is handled"),
            );
        }
    }

    /// Whether a type's rules leave out empty text: a `min_length` of at
    /// least 1 anywhere along its chain.
    fn never_empty_type(&self, ty: &Ty) -> bool {
        self.table
            .rule_chain(ty)
            .iter()
            .any(|(_, rules)| rules.min_length.is_some_and(|min| min >= 1))
    }

    /// `fill:` on `text.pad_start`: one character written at the call,
    /// never a value, as `scale:` is a number written out (area 19).
    pub(super) fn character_argument(&mut self, value: &Expr, written: &str, param: &str) {
        let span = self.span(value);
        let shown = tree::written(value.syntax());
        let (message, label) = match Const::from_expr(value) {
            Const::Str(text) if text.chars().count() == 1 => {
                self.check(value, &Ty::Prim(Prim::Text));
                return;
            }
            Const::Str(_) => (
                format!("`{shown}` isn't one character"),
                "not one character".to_string(),
            ),
            _ => {
                if !is_literal(value) && !self.facts.types.contains_key(&node_key(value.syntax())) {
                    self.infer(value);
                }
                (
                    format!("`{written}`'s `{param}:` is one character written out, not a value"),
                    "not one character written out".to_string(),
                )
            }
        };
        self.out.push(
            Diagnostic::new(&rule::TYPE_MISMATCH, message)
                .at(span, label)
                .reason(format!(
                    "`{written}` adds `{param}:` once for each code point the text is short of, so it is one character, written at the call, as `\"0\"` or `\" \"` is."
                ))
                .next(format!("Write one character, such as `{param}: \"0\"`")),
        );
    }

    /// A list of text, for `text.join`: any list whose items are built on
    /// `Text`, as `list.sum` takes any list of numbers.
    pub(super) fn texts_argument(&mut self, value: &Expr, given: &Ty, written: &str, param: &str) {
        if given.has_error() {
            return;
        }
        let item = match self.table.structure(given) {
            Ty::List(item) if !self.table.is_nullable(given) => Some(*item),
            _ => None,
        };
        let built_on = match written {
            "money.allocate" => Prim::Int,
            _ if written.starts_with("money.") => Prim::Money,
            _ => Prim::Text,
        };
        let texts = item.as_ref().is_some_and(|item| {
            !self.table.is_nullable(item) && self.table.representation(item) == Some(built_on)
        });
        if texts {
            return;
        }
        if built_on != Prim::Text {
            let (what, next) = if built_on == Prim::Money {
                ("amounts", "Pass a list of amounts of money here")
            } else {
                ("whole numbers", "Pass a list of whole numbers here")
            };
            let span = self.span(value);
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_MISMATCH,
                    format!("`{given}` can't be used where a list of {what} is required"),
                )
                .at(span, format!("this is `{given}`"))
                .reason(format!(
                    "`{written}`'s `{param}` takes a list of values built on `{}`; `{given}` isn't one.",
                    built_on.name()
                ))
                .next(next),
            );
            return;
        }
        let span = self.span(value);
        let what = match &item {
            Some(item) => format!("a list of `{item}`"),
            None => super::super::literal::what(self.table, given),
        };
        self.out.push(
            Diagnostic::new(
                &rule::TYPE_MISMATCH,
                format!("`{given}` can't be used where a list of text is required"),
            )
            .at(span, format!("this is `{given}`"))
            .reason(format!(
                "`{written}`'s `{param}` takes a list of values built on `Text`; `{given}` is {what}."
            ))
            .next("Pass a list of text here, turning each item into text first"),
        );
    }

    /// `locale:` on `list.sort_by` orders text by a locale's rules, so it
    /// means nothing for a key that isn't text (area 19, question 26).
    pub(super) fn sort_locale(
        &mut self,
        table: &jadpo_std::Operation,
        args: &[ast::Arg],
        slots: &[Option<usize>],
        key: &Ty,
        written: &str,
    ) {
        if key.has_error() || matches!(key, Ty::Var(_)) {
            return;
        }
        if self.table.representation(key) == Some(Prim::Text) {
            return;
        }
        let Some(slot) = table.params.iter().position(|p| p.name == "locale") else {
            return;
        };
        let Some(arg) = args
            .iter()
            .zip(slots)
            .find(|(_, at)| **at == Some(slot))
            .map(|(arg, _)| arg)
        else {
            return;
        };
        let span = arg
            .name()
            .map_or_else(|| self.span(arg), |name| self.span(&name));
        let removal = self.argument_removal(arg);
        self.out.push(
            Diagnostic::new(
                &rule::TYPE_TEMPORAL_OPTION_UNUSED,
                format!("`locale:` means nothing for a `{key}` key"),
            )
            .at(span, "orders text only")
            .reason(format!(
                "`{written}`'s `locale:` orders text by a locale's rules, as a dictionary does; a `{key}` key has one order everywhere."
            ))
            .next("Remove `locale: ...`")
            .edit(removal, ""),
        );
    }
}
