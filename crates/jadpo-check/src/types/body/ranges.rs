//! Proving a construction safe (area 4, question 6): when the declared
//! ranges, and the conditions it sits under (area 2, "Conditions as
//! proof"), show a computed value always follows the target type's rules,
//! `Type(value)` cannot fail and takes no `attempt`. The proof is made rule
//! by rule, because an `attempt` maps each rule that can break (area 2,
//! "Construction failures by rule").
//!
//! Numbers are followed through arithmetic as intervals, from literals and
//! the `min` and `max` of each operand's type, each narrowed by what the
//! conditions it sits under say about it ([`super::known`]). A target with
//! a fixed `scale` keeps the value rounded half to even, so the interval is
//! rounded the same way before its bounds are compared. Text, bytes and
//! collections are proved by their own type's rules implying the target's:
//! a length range inside the target's, a list's narrowed by what is known
//! of `list.count` on it, and the same pattern, format or version.

use std::cmp::Ordering;
use std::collections::BTreeSet;

use jadpo_std::Op;
use jadpo_syntax::SyntaxKind;
use jadpo_syntax::ast::{self, AstNode, Expr};

use super::super::rules::{Const, Num, Rules};
use super::super::ty::{Prim, Ty};
use super::known::{Origin, Term};
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
pub(super) enum End {
    Below,
    At(Num),
    Above,
}

impl End {
    pub(super) fn cmp(self, other: End) -> Ordering {
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

    /// The next whole number past this end, `by` 1 or -1, when the end is
    /// a whole number: what `n > 12` means for a whole `n`. Any other end
    /// is kept, which is still true, only less exact.
    pub(super) fn step(self, by: i128) -> End {
        match self {
            End::At(n) if n.significant_places() == 0 => {
                let whole = 10i128
                    .checked_pow(n.scale)
                    .map(|factor| n.mantissa / factor)
                    .and_then(|whole| whole.checked_add(by));
                whole.map_or(self, |whole| End::At(Num::whole(whole)))
            }
            other => other,
        }
    }

    /// Rounded half to even to `scale` places, as a type with a fixed
    /// `scale` keeps a value; a number too large to round is unbounded on
    /// its side, `low` for a lower end.
    fn rounded(self, scale: u32, low: bool) -> End {
        let End::At(n) = self else { return self };
        if n.scale <= scale {
            return self;
        }
        let Some(factor) = 10i128.checked_pow(n.scale - scale) else {
            return if low { End::Below } else { End::Above };
        };
        let quotient = n.mantissa / factor;
        let twice = (n.mantissa % factor).unsigned_abs() * 2;
        let whole = factor.unsigned_abs();
        let away = twice > whole || (twice == whole && quotient % 2 != 0);
        let mantissa = if away {
            quotient + n.mantissa.signum()
        } else {
            quotient
        };
        End::At(Num { mantissa, scale })
    }
}

/// The values an expression can take, both ends included.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Interval {
    pub low: End,
    pub high: End,
}

impl Interval {
    pub(super) const ANY: Interval = Interval {
        low: End::Below,
        high: End::Above,
    };

    pub(super) fn exact(n: Num) -> Interval {
        Interval {
            low: End::At(n),
            high: End::At(n),
        }
    }

    /// No value fits: the code that knows it can't run.
    pub(super) fn is_empty(self) -> bool {
        self.low.cmp(self.high).is_gt()
    }

    pub(super) fn is_any(self) -> bool {
        self.low == End::Below && self.high == End::Above
    }

    /// The values in both.
    pub(super) fn meet(self, other: Interval) -> Interval {
        Interval {
            low: if other.low.cmp(self.low).is_gt() {
                other.low
            } else {
                self.low
            },
            high: if other.high.cmp(self.high).is_lt() {
                other.high
            } else {
                self.high
            },
        }
    }

    /// Every value either can take, and those between.
    pub(super) fn hull(self, other: Interval) -> Interval {
        if self.is_empty() {
            return other;
        }
        if other.is_empty() {
            return self;
        }
        hull(&[self.low, self.high, other.low, other.high])
    }

    /// As a type with a fixed `scale` keeps its values.
    fn rounded(self, scale: Option<u32>) -> Interval {
        match scale {
            Some(scale) => Interval {
                low: self.low.rounded(scale, true),
                high: self.high.rounded(scale, false),
            },
            None => self,
        }
    }
}

/// How an interval is read: with what the conditions around it say, or
/// from the declared ranges alone, and which conditions it used.
pub(super) struct Reading {
    pub conditions: bool,
    /// The comparisons whose bounds narrowed it.
    pub used: BTreeSet<Origin>,
}

impl Reading {
    pub(super) fn with_conditions() -> Reading {
        Reading {
            conditions: true,
            used: BTreeSet::new(),
        }
    }

    fn ranges_only() -> Reading {
        Reading {
            conditions: false,
            used: BTreeSet::new(),
        }
    }
}

/// Which of a target's rules a value can break, and which the conditions
/// it sits under prove: rules the declared ranges alone don't, each with
/// the comparisons that prove it.
#[derive(Debug, Default)]
pub(super) struct Verdict {
    pub breakable: Vec<String>,
    pub proved: Vec<(String, Vec<Origin>)>,
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

/// What is known of how many items a list holds, and from which
/// comparisons.
struct Counted {
    interval: Interval,
    by: Vec<Origin>,
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
    min_duration: Option<jadpo_std::Dur>,
    max_duration: Option<jadpo_std::Dur>,
    scale: Option<u32>,
    min_length: Option<u64>,
    max_length: Option<u64>,
    patterns: Vec<String>,
    email: bool,
    version: Option<jadpo_std::UuidVersion>,
    /// The validated built-in text type at the bottom of the chain
    /// (`Email`, `Url`, `IpAddress`), whose own check counts as `format`.
    checked: Option<Prim>,
    /// A `Money` type's one currency (area 19).
    currency: Option<u32>,
    /// The standard type held as text whose own check the chain makes
    /// (area 19, chunk 5.5i), which counts as `format`.
    standard: Option<&'static str>,
}

/// A validated built-in text type: its own check of the text is what
/// `format` stands for when a value built on other text becomes one (area
/// 2, "Construction failures by rule").
fn checked_text(prim: Option<Prim>) -> Option<Prim> {
    prim.filter(|prim| matches!(prim, Prim::Email | Prim::Url | Prim::IpAddress))
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
        if let Some(min) = rules.min_duration
            && out.min_duration.is_none_or(|m| min > m)
        {
            out.min_duration = Some(min);
        }
        if let Some(max) = rules.max_duration
            && out.max_duration.is_none_or(|m| max < m)
        {
            out.max_duration = Some(max);
        }
        if let Some(scale) = rules.scale {
            out.scale = Some(out.scale.map_or(scale, |s| s.min(scale)));
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
        out.currency = out.currency.or(rules.currency);
        out.standard = out.standard.or(rules.standard);
    }
    out
}

impl Combined {
    /// The rules it holds, by the keys an `attempt`'s arms use, each as
    /// written: `("max", "max: 1000")`.
    fn written(&self) -> Vec<(&'static str, String)> {
        let mut out = Vec::new();
        if let Some(min) = self.min {
            out.push(("min", format!("min: {min}")));
        } else if let Some(min) = self.min_duration {
            out.push(("min", format!("min: {min}")));
        }
        if let Some(max) = self.max {
            out.push(("max", format!("max: {max}")));
        } else if let Some(max) = self.max_duration {
            out.push(("max", format!("max: {max}")));
        }
        if let Some(n) = self.min_length {
            out.push(("min_length", format!("min_length: {n}")));
        }
        if let Some(n) = self.max_length {
            out.push(("max_length", format!("max_length: {n}")));
        }
        if let Some(pattern) = self.patterns.first() {
            out.push(("pattern", format!("pattern: \"{pattern}\"")));
        }
        if self.email {
            out.push(("format", "format: TextFormat.email".to_string()));
        } else if let Some(builtin) = self.checked {
            out.push(("format", builtin.name().to_string()));
        } else if let Some(standard) = self.standard {
            out.push(("format", standard.to_string()));
        }
        if let Some(scale) = self.scale {
            out.push(("scale", format!("scale: {scale}")));
        }
        if let Some(version) = self.version {
            out.push((
                "version",
                format!(
                    "version: {}.{}",
                    jadpo_std::UuidVersion::NAME,
                    version.name()
                ),
            ));
        }
        if let Some(currency) = self.currency {
            out.push((
                "currency",
                format!(
                    "currency: {}.{}",
                    jadpo_std::CURRENCY,
                    jadpo_std::money::variant(currency).unwrap_or("?")
                ),
            ));
        }
        out
    }
}

impl Body<'_> {
    /// The rules of `target` and every type it builds on, by the keys an
    /// `attempt`'s arms use, each with the rule as written.
    pub(super) fn rules_of(&self, target: &Ty) -> Vec<(&'static str, String)> {
        let mut rules = combine(self.table.rule_chain(target).into_iter().map(|(_, r)| r));
        rules.checked = checked_text(self.table.base_prim(target));
        let mut written = rules.written();
        // A quantity's bounds are held in its base unit, and written as
        // code writes a quantity: `min: 0.1metre` (area 19).
        if let Some(Prim::Quantity(kind)) = self.table.base_prim(target) {
            for (key, text) in &mut written {
                let bound = match *key {
                    "min" => rules.min,
                    "max" => rules.max,
                    _ => None,
                };
                if let Some(at) = bound.and_then(|bound| bound.at_scale(kind.scale())) {
                    *text = format!("{key}: {}", jadpo_std::quantity::written(kind, at.mantissa));
                }
            }
        }
        written
    }

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

    /// `list.sum`: items never below zero add up to a sum never below
    /// zero, however many there are, and items never above zero to one
    /// never above it (area 19); anything finer needs a check.
    fn summed(&self, call: &ast::CallExpr) -> Option<Interval> {
        let super::facts::Reach::Std(Op::ListSum | Op::MoneySum) =
            self.facts.reaches.get(&node_key(call.syntax()))?
        else {
            return None;
        };
        let items = call.arg_list()?.args().next()?.expr()?;
        let item = match self
            .facts
            .types
            .get(&node_key(items.syntax()))
            .map(|ty| self.table.structure(ty))
        {
            Some(Ty::List(item)) => *item,
            _ => return Some(Interval::ANY),
        };
        let each = self.type_interval(&item);
        let zero = End::At(Num::whole(0));
        Some(if each.low.cmp(zero).is_ge() {
            Interval {
                low: zero,
                high: End::Above,
            }
        } else if each.high.cmp(zero).is_le() {
            Interval {
                low: End::Below,
                high: zero,
            }
        } else {
            Interval::ANY
        })
    }

    /// `min` and `max` (area 19, "Numbers"): the smaller of two values
    /// lies between the smaller of their lows and the smaller of their
    /// highs, and the larger likewise, so `integer.min(wanted, 12)` is
    /// never above 12.
    fn chosen(&self, call: &ast::CallExpr, reading: &mut Reading) -> Option<Interval> {
        let super::facts::Reach::Std(op) = self.facts.reaches.get(&node_key(call.syntax()))? else {
            return None;
        };
        let smaller = match op {
            Op::IntegerMin | Op::DecimalMin => true,
            Op::IntegerMax | Op::DecimalMax => false,
            _ => return None,
        };
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
        let a = self.interval(&find("a", 0)?, reading);
        let b = self.interval(&find("b", 1)?, reading);
        let pick = |x: End, y: End| {
            if (x.cmp(y) == Ordering::Less) == smaller {
                x
            } else {
                y
            }
        };
        Some(Interval {
            low: pick(a.low, b.low),
            high: pick(a.high, b.high),
        })
    }

    /// `integer.from_decimal` and `decimal.round` (area 19, "Numbers"):
    /// rounding never crosses zero, whichever way it goes, so a value never
    /// below zero rounds to one never below zero, and one never above zero
    /// to one never above it; anything finer needs a check.
    fn rounded(&self, call: &ast::CallExpr, reading: &mut Reading) -> Option<Interval> {
        let super::facts::Reach::Std(Op::IntegerFromDecimal | Op::DecimalRound) =
            self.facts.reaches.get(&node_key(call.syntax()))?
        else {
            return None;
        };
        let value = call
            .arg_list()?
            .args()
            .find(|arg| {
                arg.name().is_none()
                    || arg
                        .name()
                        .and_then(|n| n.ident_token())
                        .is_some_and(|t| t.text() == "value")
            })?
            .expr()?;
        let interval = self.interval(&value, reading);
        let zero = End::At(Num::whole(0));
        Some(Interval {
            low: if interval.low.cmp(zero).is_ge() {
                zero
            } else {
                End::Below
            },
            high: if interval.high.cmp(zero).is_le() {
                zero
            } else {
                End::Above
            },
        })
    }

    /// `money.from_minor` and `money.minor` (area 19): an amount made
    /// from a number of minor units has that number's bounds, and an
    /// amount's minor units its own.
    fn amount(&self, call: &ast::CallExpr, reading: &mut Reading) -> Option<Interval> {
        let super::facts::Reach::Std(Op::MoneyFromMinor | Op::MoneyMinor) =
            self.facts.reaches.get(&node_key(call.syntax()))?
        else {
            return None;
        };
        let value = call.arg_list()?.args().next()?.expr()?;
        Some(self.interval(&value, reading))
    }

    /// `random.integer` (area 19, "Randomness"): a number drawn from `min`
    /// to `max` is never below the lowest `min` can be nor above the
    /// highest `max` can be, so `DiceRoll(random.integer(1, 6))` needs no
    /// arm.
    fn drawn(&self, call: &ast::CallExpr, reading: &mut Reading) -> Option<Interval> {
        let super::facts::Reach::Std(Op::RandomInteger) =
            self.facts.reaches.get(&node_key(call.syntax()))?
        else {
            return None;
        };
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
        let min = self.interval(&find("min", 0)?, reading);
        let max = self.interval(&find("max", 1)?, reading);
        Some(Interval {
            low: min.low,
            high: max.high,
        })
    }

    fn floor(&self, call: &ast::CallExpr) -> Option<i64> {
        match self.facts.reaches.get(&node_key(call.syntax()))? {
            super::facts::Reach::Std(op) => op.operation().at_least,
            _ => None,
        }
    }

    /// The rules of `target` that neither the declared ranges nor the
    /// conditions around it prove `value`, of type `actual`, follows, by
    /// their keys, in the order the reader learns them, with the rules
    /// only the conditions prove. A rule the compiler can't reason about,
    /// such as a pattern other than the value's own, can always break;
    /// `scale` never does, because it rounds the value as it is kept.
    pub(super) fn verdict(&self, value: &Expr, actual: &Ty, target: &Ty) -> Verdict {
        let needs = combine(self.table.rule_chain(target).into_iter().map(|(_, r)| r));
        let has = combine(self.table.rule_chain(actual).into_iter().map(|(_, r)| r));
        let mut verdict = Verdict::default();
        let out = &mut verdict.breakable;
        let proved = &mut verdict.proved;
        if needs.min.is_some() || needs.max.is_some() {
            // The scale the value is kept at: the most specific along the
            // chain, as the runtime's validator takes it.
            let kept = self
                .table
                .rule_chain(target)
                .into_iter()
                .find_map(|(_, rules)| rules.scale);
            let mut reading = Reading::with_conditions();
            let interval = self.interval(value, &mut reading).rounded(kept);
            // Read again without the conditions only when one narrowed it,
            // to tell the rules they prove from those the ranges do.
            let plain = (!reading.used.is_empty()).then(|| {
                self.interval(value, &mut Reading::ranges_only())
                    .rounded(kept)
            });
            let by: Vec<Origin> = reading.used.iter().cloned().collect();
            if let Some(min) = needs.min {
                if !at_least(&interval, min) {
                    out.push("min".to_string());
                } else if plain.is_some_and(|plain| !at_least(&plain, min)) {
                    proved.push(("min".to_string(), by.clone()));
                }
            }
            if let Some(max) = needs.max {
                if !at_most(&interval, max) {
                    out.push("max".to_string());
                } else if plain.is_some_and(|plain| !at_most(&plain, max)) {
                    proved.push(("max".to_string(), by.clone()));
                }
            }
        }
        if needs
            .min_duration
            .is_some_and(|n| has.min_duration.is_none_or(|m| m < n))
        {
            out.push("min".to_string());
        }
        if needs
            .max_duration
            .is_some_and(|n| has.max_duration.is_none_or(|m| m > n))
        {
            out.push("max".to_string());
        }
        if needs.min_length.is_some() || needs.max_length.is_some() {
            let counted = self.counted(value, actual);
            // A text operation's result is never longer than the text it
            // cuts (`at_most_length_of`, area 19, question 29).
            let high = has.max_length.or_else(|| self.length_bound(value));
            let low = has.min_length;
            let at_least_length = |n: u64, low: Option<u64>, counted: Option<&Counted>| {
                low.is_some_and(|m| m >= n)
                    || counted
                        .is_some_and(|c| c.interval.low.cmp(End::At(Num::whole(n.into()))).is_ge())
            };
            let at_most_length = |n: u64, high: Option<u64>, counted: Option<&Counted>| {
                high.is_some_and(|m| m <= n)
                    || counted
                        .is_some_and(|c| c.interval.high.cmp(End::At(Num::whole(n.into()))).is_le())
            };
            // A set built from a list (area 2, "Named collection
            // construction") drops the list's repeats, so it holds at most
            // as many items as the list and, when the list has any, at
            // least one: nothing proves two items differ.
            let collapses = matches!(self.table.structure(target), Ty::Set(_))
                && matches!(self.table.structure(actual), Ty::List(_));
            if let Some(n) = needs.min_length {
                if (collapses && n > 1) || !at_least_length(n, low, counted.as_ref()) {
                    out.push("min_length".to_string());
                } else if !at_least_length(n, low, None) {
                    let by = counted.as_ref().map(|c| c.by.clone()).unwrap_or_default();
                    proved.push(("min_length".to_string(), by));
                }
            }
            if let Some(n) = needs.max_length {
                if !at_most_length(n, high, counted.as_ref()) {
                    out.push("max_length".to_string());
                } else if !at_most_length(n, high, None) {
                    let by = counted.as_ref().map(|c| c.by.clone()).unwrap_or_default();
                    proved.push(("max_length".to_string(), by));
                }
            }
        }
        if needs.patterns.iter().any(|p| !has.patterns.contains(p)) {
            out.push("pattern".to_string());
        }
        // `format`: a `format: TextFormat.email` rule, or the target's
        // validated built-in text type, whose check the value's own type
        // makes only when it is built on the same one (an `Email` and a
        // `format: TextFormat.email` text check the same).
        let base = self.table.base_prim(actual);
        let is_email = has.email || base == Some(Prim::Email);
        let checked = match checked_text(self.table.base_prim(target)) {
            None => true,
            Some(Prim::Email) => is_email,
            Some(builtin) => base == Some(builtin),
        };
        // A standard type held as text: the value's own type makes the
        // same check only when it is built on the same one. `Html` is made
        // safe as it is built, so its construction never breaks it (chunk
        // 5.5o).
        let standard = needs.standard.is_none_or(|name| {
            has.standard == Some(name) || jadpo_std::standard::markup::sanitises(name)
        });
        if (needs.email && !is_email) || !checked || !standard {
            out.push("format".to_string());
        }
        if needs.version.is_some() && needs.version != has.version {
            out.push("version".to_string());
        }
        // A `Money` type's currency: the amount's own type fixing the same
        // one, how it was made, or a condition around it (area 19).
        if let Some(code) = needs.currency
            && !self.in_currency(value, code)
        {
            out.push("currency".to_string());
        }
        verdict
    }

    /// The most code points a text expression can hold when its type
    /// can't say: a literal's own length, or, for a text operation whose
    /// result is never longer than one of its arguments (`text.trim`,
    /// `slice` and the four cuts), that argument's.
    fn length_bound(&self, value: &Expr) -> Option<u64> {
        match value {
            Expr::ParenExpr(paren) => self.length_bound(&paren.expr()?),
            Expr::Literal(_) => match Const::from_expr(value) {
                Const::Str(text) => u64::try_from(text.chars().count()).ok(),
                _ => None,
            },
            Expr::CallExpr(call) => {
                let super::facts::Reach::Std(op) =
                    self.facts.reaches.get(&node_key(call.syntax()))?
                else {
                    return None;
                };
                let slot = op.operation().params.iter().position(|p| p.bounds_length)?;
                let slots = self.facts.slots.get(&node_key(call.syntax()))?;
                let cut = call
                    .arg_list()?
                    .args()
                    .zip(slots)
                    .find(|(_, at)| **at == Some(slot))?
                    .0
                    .expr()?;
                let own = self
                    .facts
                    .types
                    .get(&node_key(cut.syntax()))
                    .filter(|ty| !ty.has_error() && !self.table.is_nullable(ty))
                    .and_then(|ty| {
                        combine(self.table.rule_chain(ty).into_iter().map(|(_, r)| r)).max_length
                    });
                own.or_else(|| self.length_bound(&cut))
            }
            _ => None,
        }
    }

    /// What the conditions around a list say about how many items it
    /// holds: the bounds known for `list.count` on it, which counts
    /// exactly the items `min_length` and `max_length` count.
    fn counted(&self, value: &Expr, actual: &Ty) -> Option<Counted> {
        if self.known.is_empty()
            || self.table.is_nullable(actual)
            || !matches!(self.table.structure(actual), Ty::List(_))
        {
            return None;
        }
        let term = Term::Std(Op::ListCount, vec![self.term(value)?]);
        let fact = self.known.get(&term)?;
        Some(Counted {
            interval: fact.interval,
            by: fact.from.iter().cloned().collect(),
        })
    }

    /// The values a numeric expression can take: from its parts and the
    /// declared ranges, narrowed by what the conditions around it say
    /// about it as written, when `reading` asks for them.
    pub(super) fn interval(&self, expr: &Expr, reading: &mut Reading) -> Interval {
        let base = self.structural(expr, reading);
        if !reading.conditions || self.known.is_empty() {
            return base;
        }
        let Some(fact) = self.term(expr).and_then(|term| self.known.get(&term)) else {
            return base;
        };
        let narrowed = base.meet(fact.interval);
        if narrowed != base {
            reading.used.extend(fact.from.iter().cloned());
        }
        narrowed
    }

    /// The values a numeric expression can take, from its parts.
    fn structural(&self, expr: &Expr, reading: &mut Reading) -> Interval {
        match expr {
            Expr::ParenExpr(paren) => paren
                .expr()
                .map_or(Interval::ANY, |inner| self.interval(&inner, reading)),
            Expr::Literal(_) => match Const::from_expr(expr) {
                Const::Number { value, .. } => Interval::exact(value),
                _ => Interval::ANY,
            },
            Expr::PrefixExpr(prefix) => {
                let minus =
                    prefix.syntax().first_token().map(|t| t.kind()) == Some(SyntaxKind::MINUS);
                match (minus, prefix.expr()) {
                    (true, Some(inner)) => {
                        let inner = self.interval(&inner, reading);
                        Interval {
                            low: inner.high.neg(),
                            high: inner.low.neg(),
                        }
                    }
                    _ => Interval::ANY,
                }
            }
            Expr::BinExpr(binary) => self.binary_interval(binary, reading),
            // A standard-library operation whose result has a floor its
            // type can't say, as `list.count`'s is 0 (the operation
            // table's `at_least`).
            Expr::CallExpr(call) if let Some(floor) = self.floor(call) => Interval {
                low: End::At(Num::whole(i128::from(floor))),
                high: End::Above,
            },
            Expr::CallExpr(call) if let Some(summed) = self.summed(call) => summed,
            Expr::CallExpr(call) if let Some(chosen) = self.chosen(call, reading) => chosen,
            Expr::CallExpr(call) if let Some(rounded) = self.rounded(call, reading) => rounded,
            Expr::CallExpr(call) if let Some(amount) = self.amount(call, reading) => amount,
            Expr::CallExpr(call) if let Some(drawn) = self.drawn(call, reading) => drawn,
            // `integer.divide` and `decimal.divide` are bounded as `/` is,
            // whichever way they round: rounding away from zero never
            // passes the dividend while the divisor is at least 1.
            Expr::CallExpr(call) if let Some((dividend, divisor)) = self.division(call) => divided(
                self.interval(&dividend, reading),
                self.interval(&divisor, reading),
                false,
            ),
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

    fn binary_interval(&self, binary: &ast::BinExpr, reading: &mut Reading) -> Interval {
        let (Some(lhs), Some(rhs)) = (binary.lhs(), binary.rhs()) else {
            return Interval::ANY;
        };
        let op = binary
            .op()
            .map(|token| token.text().to_string())
            .unwrap_or_default();
        let a = self.interval(&lhs, reading);
        let b = self.interval(&rhs, reading);
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
