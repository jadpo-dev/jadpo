//! Which currency an amount of money is in, as far as the checker knows
//! (area 19, "The standard operator table"): fixed by its type's
//! `currency:` or by a currency written out where it is made, carried
//! through arithmetic in one currency, and proved by a condition that
//! compares currencies ([`super::known`]). `+`, `-`, `/` and ordering on
//! two amounts read it: two currencies known to differ is a compile error,
//! one currency known is a plain operator, and anything else can fail with
//! `currency_mismatch` under `attempt`.

use jadpo_std::Op;
use jadpo_syntax::ast::{self, AstNode, Expr};

use super::super::ty::Ty;
use super::facts::Reach;
use super::known::{Origin, Term};
use super::{Body, node_key};

/// What is known of an amount's currency.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Currency {
    /// This currency, by its place among `Currency`'s variants.
    Fixed(u32),
    /// The currency of this subject, whatever it is.
    Of(Term),
    /// Nothing.
    Unknown,
}

/// Whether two amounts are in one currency, as far as is known.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum OneCurrency {
    /// Yes, by their types, how they were made, or the conditions named.
    Yes(Vec<Origin>),
    /// No: each is fixed, to different currencies.
    No(u32, u32),
    /// Only the running program knows.
    Unknown,
}

impl Body<'_> {
    /// The currency a `Money` type's chain fixes, if it does.
    pub(super) fn fixed_currency(&self, ty: &Ty) -> Option<u32> {
        if self.table.is_nullable(ty) {
            return None;
        }
        self.table
            .rule_chain(ty)
            .into_iter()
            .find_map(|(_, rules)| rules.currency)
    }

    /// What is known of the currency of the amount `expr`.
    pub(super) fn currency_of(&self, expr: &Expr) -> Currency {
        if let Expr::ParenExpr(paren) = expr {
            return paren
                .expr()
                .map_or(Currency::Unknown, |inner| self.currency_of(&inner));
        }
        if let Some(code) = self
            .facts
            .types
            .get(&node_key(expr.syntax()))
            .and_then(|ty| self.fixed_currency(ty))
        {
            return Currency::Fixed(code);
        }
        match expr {
            Expr::CallExpr(call) => self.made_in(call).unwrap_or(Currency::Unknown),
            Expr::BinExpr(binary) => {
                let (Some(lhs), Some(rhs), Some(op)) = (binary.lhs(), binary.rhs(), binary.op())
                else {
                    return Currency::Unknown;
                };
                match op.text() {
                    // Two amounts in one currency, or one scaled by a number.
                    "+" | "-" => match self.one_currency(&lhs, &rhs) {
                        OneCurrency::Yes(_) => self.known_currency(&lhs, &rhs),
                        _ => Currency::Unknown,
                    },
                    "*" | "/" => self.currency_of(&lhs),
                    _ => Currency::Unknown,
                }
            }
            other => {
                self.term(other)
                    .map_or(Currency::Unknown, |term| match self.known.same_as(&term) {
                        Some(same) if let Some(code) = same.currency => Currency::Fixed(code),
                        _ => Currency::Of(term),
                    })
            }
        }
    }

    /// Of two amounts in one currency, what is known of it: a fixed one if
    /// either has it, else the first's.
    fn known_currency(&self, lhs: &Expr, rhs: &Expr) -> Currency {
        match (self.currency_of(lhs), self.currency_of(rhs)) {
            (fixed @ Currency::Fixed(_), _) | (_, fixed @ Currency::Fixed(_)) => fixed,
            (first, _) => first,
        }
    }

    /// The currency a standard operation makes its amount in, where the
    /// call writes it out: `money.from_minor`, `money.of` and `money.sum`.
    fn made_in(&self, call: &ast::CallExpr) -> Option<Currency> {
        let key = node_key(call.syntax());
        let Some(Reach::Std(op)) = self.facts.reaches.get(&key) else {
            return None;
        };
        if !matches!(op, Op::MoneyFromMinor | Op::MoneyOf | Op::MoneySum) {
            return None;
        }
        let code = self.written_currency(*op, call)?;
        Some(Currency::Fixed(code))
    }

    /// The currency a money operation's `currency` argument writes out, by
    /// its place among `Currency`'s variants.
    pub(super) fn written_currency(&self, op: Op, call: &ast::CallExpr) -> Option<u32> {
        let key = node_key(call.syntax());
        let slot = op
            .operation()
            .params
            .iter()
            .position(|param| param.name == "currency")?;
        let slots = self.facts.slots.get(&key)?;
        let arg = call
            .arg_list()?
            .args()
            .zip(slots)
            .find(|(_, at)| **at == Some(slot))?
            .0
            .expr()?;
        match self.facts.variants.get(&node_key(arg.syntax())) {
            Some((ty, variant)) if ty == jadpo_std::CURRENCY => {
                jadpo_std::money::ordinal_of_variant(variant)
            }
            _ => None,
        }
    }

    /// Whether the amounts `lhs` and `rhs` are in one currency.
    pub(super) fn one_currency(&self, lhs: &Expr, rhs: &Expr) -> OneCurrency {
        self.same_currency(&self.currency_of(lhs), &self.currency_of(rhs))
    }

    /// Whether two currencies are one, as far as is known.
    pub(super) fn same_currency(&self, a: &Currency, b: &Currency) -> OneCurrency {
        match (a, b) {
            (Currency::Fixed(x), Currency::Fixed(y)) if x == y => OneCurrency::Yes(Vec::new()),
            (Currency::Fixed(x), Currency::Fixed(y)) => OneCurrency::No(*x, *y),
            (Currency::Of(x), Currency::Of(y)) if x == y => OneCurrency::Yes(Vec::new()),
            (Currency::Of(x), Currency::Of(y)) => match self.known.same_as(x) {
                Some(same) if same.terms.contains(y) => {
                    OneCurrency::Yes(same.from.iter().cloned().collect())
                }
                _ => OneCurrency::Unknown,
            },
            _ => OneCurrency::Unknown,
        }
    }

    /// Whether the amount `value` is in `code`, the currency a type being
    /// built fixes: its `currency` rule can't break when it is.
    pub(super) fn in_currency(&self, value: &Expr, code: u32) -> bool {
        matches!(
            self.same_currency(&self.currency_of(value), &Currency::Fixed(code)),
            OneCurrency::Yes(_)
        )
    }
}

impl Body<'_> {
    /// Whether a call settles a money operation's condition by proof:
    /// `money.sum`'s `currency_mismatch` when its items' type fixes the
    /// currency it writes out, and `money.of`'s `too_many_places` when it
    /// writes out a currency and its amount has no more places than that
    /// currency, by a literal's own or its type's `scale` (area 19).
    pub(super) fn money_condition_settled<'a>(
        &self,
        table: &jadpo_std::Operation,
        call: &ast::CallExpr,
        key: jadpo_std::RuleKey,
        given: &dyn Fn(usize) -> Option<&'a ast::Arg>,
    ) -> bool {
        let Some(code) = self.written_currency(table.op, call) else {
            return false;
        };
        let value = |name: &str| {
            table
                .params
                .iter()
                .position(|param| param.name == name)
                .and_then(given)
                .and_then(ast::Arg::expr)
        };
        match key {
            jadpo_std::RuleKey::CurrencyMismatch => {
                let Some(items) = value("items") else {
                    return false;
                };
                let item = match self
                    .facts
                    .types
                    .get(&node_key(items.syntax()))
                    .map(|ty| (self.table.is_nullable(ty), self.table.structure(ty)))
                {
                    Some((false, Ty::List(item))) => *item,
                    _ => return false,
                };
                self.fixed_currency(&item) == Some(code)
            }
            jadpo_std::RuleKey::TooManyPlaces => {
                let Some(amount) = value("amount") else {
                    return false;
                };
                let Some(exponent) = jadpo_std::money::exponent(code) else {
                    return false;
                };
                if super::is_literal(&amount) {
                    return match super::super::rules::Const::from_expr(&amount) {
                        super::super::rules::Const::Number { value, .. } => {
                            value.significant_places() <= exponent
                        }
                        _ => false,
                    };
                }
                self.facts
                    .types
                    .get(&node_key(amount.syntax()))
                    .filter(|ty| !self.table.is_nullable(ty))
                    .is_some_and(|ty| {
                        self.table.representation(ty) == Some(super::super::ty::Prim::Int)
                            || self
                                .table
                                .rule_chain(ty)
                                .into_iter()
                                .find_map(|(_, rules)| rules.scale)
                                .is_some_and(|scale| scale <= exponent)
                    })
            }
            _ => false,
        }
    }
}
