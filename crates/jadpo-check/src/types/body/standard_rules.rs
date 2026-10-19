//! A standard object literal's rules across its parts (area 19, chunk
//! 5.5t2): a range's start not after its end, an address's postal code in
//! its country's form, a timecode's frame below its rate. Each is one of
//! `jadpo-std`'s named rules, read here by what the literal shows of each
//! part: a part written out is known, one written `none` absent, and one
//! built from a value of a type that can't be `none` given. A rule its
//! known parts decide holds or is `type.invalid_literal`; one they don't is
//! proved, for a rule ordering two parts, as an application's condition
//! comparing two fields is (area 2, "Rules across fields"), or left to the
//! running program, so the literal is attempted and mapped by the rule's
//! name, as an application's object literal is.

use std::collections::BTreeMap;

use jadpo_diag::{Diagnostic, rule};
use jadpo_std::standard::across::{Altitudes, Check, Given, Known, Listed, StandardRule, Verdict};
use jadpo_std::standard::recurrence::LocalAt;
use jadpo_syntax::ast::{self, AstNode, Expr};

use super::super::across::Cond;
use super::super::rules::Const;
use super::super::ty::{Prim, Ty};
use super::across::Truth;
use super::{Body, Fallible, node_key};
use crate::case::starts_upper;
use crate::tree;

/// What a literal writes for a part, read once, its text owned so a rule
/// can borrow it.
enum Shown {
    Absent,
    Given(Option<Owned>),
    Unknown,
}

enum Owned {
    Number(jadpo_std::Num),
    Quantity(jadpo_std::QuantityKind, i128),
    Instant(i64),
    Date(i32),
    Duration(i64),
    Text(String),
    Variant(u32, &'static str),
    Bool(bool),
    Local(LocalAt),
    Rate(i64, i64),
}

impl Owned {
    fn given(&self) -> Given<'_> {
        match self {
            Owned::Number(n) => Given::Number(*n),
            Owned::Quantity(kind, amount) => Given::Quantity(*kind, *amount),
            Owned::Instant(ms) => Given::Instant(*ms),
            Owned::Date(day) => Given::Date(*day),
            Owned::Duration(ms) => Given::Duration(*ms),
            Owned::Text(text) => Given::Text(text),
            Owned::Variant(ordinal, code) => Given::Variant {
                ordinal: *ordinal,
                code,
            },
            Owned::Bool(flag) => Given::Bool(*flag),
            Owned::Local(at) => Given::Local(*at),
            Owned::Rate(n, d) => Given::Rate(*n, *d),
        }
    }
}

/// The value a literal writes for part `part`, if it writes the part.
fn value_of(record: &ast::RecordExpr, part: &str) -> Option<Expr> {
    tree::members(record.member_list())
        .into_iter()
        .find(|(field, _)| tree::name_text(field).as_deref() == Some(part))
        .and_then(|(_, member)| member.value())
}

/// A whole number written as a literal.
fn whole(expr: &Expr) -> Option<i64> {
    match Const::from_expr(expr) {
        Const::Number { value, .. } if value.scale == 0 => i64::try_from(value.mantissa).ok(),
        _ => None,
    }
}

/// A `LocalDateTime` literal of a date and a `LocalTime` literal.
fn local_at(record: &ast::RecordExpr) -> Option<LocalAt> {
    let Const::Str(date) = Const::from_expr(&value_of(record, "date")?) else {
        return None;
    };
    let Expr::RecordExpr(time) = value_of(record, "time")? else {
        return None;
    };
    let part = |name: &str| value_of(&time, name).as_ref().and_then(whole);
    let second = part("hour")? * 3600 + part("minute")? * 60 + part("second")?;
    Some(LocalAt {
        date: jadpo_std::date_number(&date)?,
        second: u32::try_from(second).ok()?,
    })
}

/// A `FrameRate` literal's numerator and denominator.
fn rate_of(record: &ast::RecordExpr) -> Option<(i64, i64)> {
    Some((
        whole(&value_of(record, "numerator")?)?,
        whole(&value_of(record, "denominator")?)?,
    ))
}

/// A ring written out: a list of `Point` literals, each latitude and
/// longitude a number written out, as `rings_valid` reads it (area 20),
/// each coordinate as it is held, the nearest double (question 2);
/// `None` for anything else, which only the running program knows.
fn ring_literal(value: &Expr) -> Option<Vec<(jadpo_std::Num, jadpo_std::Num)>> {
    let Expr::ListExpr(list) = value else {
        return None;
    };
    list.exprs()
        .map(|item| {
            let Expr::RecordExpr(point) = item else {
                return None;
            };
            let head = point.expr().map(|head| tree::written(head.syntax()))?;
            if head != jadpo_std::standard::geo::POINT {
                return None;
            }
            let coordinate = |name: &str| match Const::from_expr(&value_of(&point, name)?) {
                Const::Number { value, .. } => {
                    jadpo_std::standard::geo::nearest_double(&value.to_string())
                        .and_then(|held| jadpo_std::Num::parse(&held))
                }
                _ => None,
            };
            Some((coordinate("latitude")?, coordinate("longitude")?))
        })
        .collect()
}

/// What a literal shows of a polygon's ring part: one ring for `outer`
/// (`depth` 1), each ring for `holes` (2), known only when written out.
fn rings_shown(value: Option<&Expr>, depth: usize) -> Known<jadpo_std::standard::across::Rings> {
    let Some(value) = value else {
        return Known::Unknown;
    };
    let read = match depth {
        1 => ring_literal(value).map(|ring| vec![ring]),
        _ => match value {
            Expr::ListExpr(list) => list
                .exprs()
                .map(|ring| ring_literal(&ring))
                .collect::<Option<Vec<_>>>(),
            _ => None,
        },
    };
    match read {
        Some(rings) => Known::Given(Some(rings)),
        None => Known::Unknown,
    }
}

/// A literal read as the part it is written for: text as an instant or a
/// date where the part is one, a frame rate where it is one.
fn owned(value: &Expr, prim: Option<Prim>, standard: Option<&str>) -> Option<Owned> {
    if let Expr::RecordExpr(record) = value {
        let head = record.expr().map(|head| tree::written(head.syntax()));
        return match head.as_deref() {
            Some("LocalDateTime") => local_at(record).map(Owned::Local),
            Some(jadpo_std::standard::media::FRAME_RATE) => {
                rate_of(record).map(|(n, d)| Owned::Rate(n, d))
            }
            _ => None,
        };
    }
    // A duration or a quantity written negative: `-1s`.
    if let Expr::PrefixExpr(prefix) = value
        && prefix.syntax().first_token().map(|t| t.kind()) == Some(jadpo_syntax::SyntaxKind::MINUS)
        && let Some(inner) = prefix.expr()
        && !matches!(Const::from_expr(value), Const::Number { .. })
    {
        return match owned(&inner, prim, standard)? {
            Owned::Duration(ms) => Some(Owned::Duration(ms.checked_neg()?)),
            Owned::Quantity(kind, amount) => Some(Owned::Quantity(kind, amount.checked_neg()?)),
            _ => None,
        };
    }
    match Const::from_expr(value) {
        Const::Number { value, .. } => Some(Owned::Number(value)),
        Const::Quantity(text) => jadpo_std::quantity::parse_literal(&text)
            .ok()
            .map(|(unit, amount)| Owned::Quantity(unit.kind, amount)),
        Const::Duration(text) => jadpo_std::Dur::parse(&text)
            .and_then(|duration| i64::try_from(duration.millis).ok())
            .map(Owned::Duration),
        Const::Bool(flag) => Some(Owned::Bool(flag)),
        Const::Str(text) => match (prim, standard) {
            (Some(Prim::Instant), _) => jadpo_std::instant_millis(&text).map(Owned::Instant),
            (Some(Prim::CalendarDate), _) => jadpo_std::date_number(&text).map(Owned::Date),
            (_, Some(jadpo_std::standard::media::FRAME_RATE)) => {
                jadpo_std::percentage::parse_fraction(&text).map(|(n, d)| Owned::Rate(n, d))
            }
            (_, Some("LocalDateTime")) => {
                jadpo_std::standard::across::local_at_text(&text).map(Owned::Local)
            }
            _ => Some(Owned::Text(text)),
        },
        Const::Path(path) => match path.as_slice() {
            [head, variant] if starts_upper(head) => {
                let ordinal = jadpo_std::variant_ordinal(head, variant)?;
                let code = jadpo_std::standard_type(head)
                    .and_then(jadpo_std::StandardType::codes)
                    .and_then(|codes| codes.iter().find(|(name, _)| name == variant))
                    .map_or("", |(_, code)| *code);
                Some(Owned::Variant(ordinal, code))
            }
            _ => None,
        },
        _ => None,
    }
}

impl Body<'_> {
    /// What a literal shows of a geometry's part holding positions, as
    /// `altitudes_agree` reads it (area 20): each `Point` written out with
    /// an altitude, with `none`, or with a value that may be `none`, at any
    /// depth; anything else written there is a position only the running
    /// program knows.
    fn altitudes_shown(&self, value: Option<&Expr>) -> Known<Altitudes> {
        let Some(value) = value else {
            return Known::Unknown;
        };
        let mut out = Vec::new();
        self.positions_shown(value, &mut out);
        Known::Given(Some(out))
    }

    fn positions_shown(&self, value: &Expr, out: &mut Altitudes) {
        use jadpo_std::standard::geo::{LINE_STRING, POINT, POLYGON};
        let part = |name: &str, record: &ast::RecordExpr, out: &mut Altitudes| match value_of(
            record, name,
        ) {
            Some(inner) => self.positions_shown(&inner, out),
            None => out.push(None),
        };
        match value {
            Expr::ListExpr(list) => {
                for item in list.exprs() {
                    self.positions_shown(&item, out);
                }
            }
            Expr::RecordExpr(record) => {
                let head = record.expr().map(|head| tree::written(head.syntax()));
                match head.as_deref() {
                    Some(POINT) => out.push(
                        match self.shown(value_of(record, "altitude").as_ref(), None, None) {
                            Shown::Absent => Some(false),
                            Shown::Given(_) => Some(true),
                            Shown::Unknown => None,
                        },
                    ),
                    Some(LINE_STRING) => part("points", record, out),
                    Some(POLYGON) => {
                        part("outer", record, out);
                        part("holes", record, out);
                    }
                    _ => out.push(None),
                }
            }
            _ => out.push(None),
        }
    }

    /// What a literal shows of one of its parts.
    fn shown(&self, value: Option<&Expr>, prim: Option<Prim>, standard: Option<&str>) -> Shown {
        let Some(value) = value else {
            return Shown::Unknown;
        };
        let written = Const::from_expr(value);
        if written == Const::None {
            return Shown::Absent;
        }
        let literal = super::is_literal(value)
            || matches!(value, Expr::RecordExpr(_))
            || matches!(&written, Const::Path(path) if path.len() == 2 && path.first().is_some_and(|head| starts_upper(head)));
        if literal {
            return Shown::Given(owned(value, prim, standard));
        }
        match self.facts.types.get(&node_key(value.syntax())) {
            Some(ty) if !ty.has_error() && !self.table.is_nullable(ty) => Shown::Given(None),
            _ => Shown::Unknown,
        }
    }

    /// The parts of `host` and the built-in type, or the standard type,
    /// each is built on, in declared order; a range's ends are `item`.
    fn standard_parts(
        &self,
        host: &str,
        item: Option<&Ty>,
    ) -> Vec<(&'static str, Option<Prim>, Option<&'static str>)> {
        match jadpo_std::standard_type(host).map(|entry| entry.holds) {
            Some(jadpo_std::standard::Holds::Object(fields)) => fields
                .iter()
                .map(|(name, part)| {
                    let inner = part.required().0;
                    let standard = match inner {
                        jadpo_std::Part::Standard(other) => Some(other),
                        _ => None,
                    };
                    (*name, inner.builtin(), standard)
                })
                .collect(),
            Some(jadpo_std::standard::Holds::Range) => {
                let prim = item.and_then(|item| self.table.base_prim(item));
                jadpo_std::RANGE_FIELDS
                    .iter()
                    .map(|name| (*name, prim, None))
                    .collect()
            }
            _ => Vec::new(),
        }
    }

    /// Records whether building the standard object `host` with `record`
    /// can fail: each of its rules across its parts held by what the
    /// literal shows, broken by it (`type.invalid_literal`), proved, or left
    /// to the running program, which an `attempt` maps.
    pub(super) fn standard_rules(
        &mut self,
        record: &ast::RecordExpr,
        host: &str,
        item: Option<&Ty>,
    ) {
        let key = node_key(record.syntax());
        let rules = jadpo_std::standard::across::rules(host);
        if rules.is_empty() {
            self.facts.constructions.insert(key, Fallible::Never);
            return;
        }
        let shown: Vec<(&'static str, Shown)> = self
            .standard_parts(host, item)
            .into_iter()
            .map(|(name, prim, standard)| {
                (
                    name,
                    self.shown(value_of(record, name).as_ref(), prim, standard),
                )
            })
            .collect();
        let rings = shown
            .iter()
            .filter_map(|(name, _)| {
                let depth = jadpo_std::standard::across::ring_depth(host, name)?;
                Some((*name, rings_shown(value_of(record, name).as_ref(), depth)))
            })
            .collect();
        let altitudes = shown
            .iter()
            .filter(|(name, _)| jadpo_std::standard::across::holds_positions(host, name))
            .map(|(name, _)| (*name, self.altitudes_shown(value_of(record, name).as_ref())))
            .collect();
        let parts = Listed {
            parts: shown
                .iter()
                .map(|(name, shown)| {
                    (
                        *name,
                        match shown {
                            Shown::Absent => Known::Absent,
                            Shown::Given(value) => Known::Given(value.as_ref().map(Owned::given)),
                            Shown::Unknown => Known::Unknown,
                        },
                    )
                })
                .collect(),
            rings,
            altitudes,
        };
        let given: BTreeMap<String, Expr> = tree::members(record.member_list())
            .into_iter()
            .filter_map(|(name, member)| Some((tree::name_text(&name)?, member.value()?)))
            .collect();
        let mut breakable = Vec::new();
        let mut proved = Vec::new();
        for rule in rules {
            match rule.verdict(&parts) {
                Verdict::Holds => {}
                Verdict::Breaks(at) => {
                    self.standard_rule_broken(record, host, rule, at);
                    self.facts.constructions.insert(key, Fallible::Never);
                    return;
                }
                Verdict::Unknown => match self.standard_proof(rule, &parts, &given, host) {
                    Some(by) => {
                        if !by.is_empty() {
                            proved.push((rule.name.to_string(), by));
                        }
                    }
                    None => breakable.push(rule.name.to_string()),
                },
            }
        }
        if !proved.is_empty() {
            self.facts.proofs.insert(key, proved);
        }
        let verdict = if breakable.is_empty() {
            Fallible::Never
        } else {
            Fallible::Construction(breakable.clone())
        };
        self.facts.constructions.insert(key, verdict);
        if !breakable.is_empty() && !self.handled.contains(&key) {
            self.unhandled_record(record, host, &breakable);
        }
    }

    /// The comparisons that prove a rule ordering two parts, each given
    /// and never `none`, as a condition comparing two fields is proved;
    /// `None` when they don't.
    fn standard_proof(
        &self,
        rule: &StandardRule,
        parts: &Listed<'_>,
        given: &BTreeMap<String, Expr>,
        host: &str,
    ) -> Option<Vec<super::known::Origin>> {
        let Check::InOrder { orders, .. } = rule.check else {
            return None;
        };
        let mut by = std::collections::BTreeSet::new();
        for order in orders {
            use jadpo_std::standard::across::Parts;
            let decided = |name: &str| parts.part(name);
            match (decided(order.lower), decided(order.upper)) {
                // An open end keeps it.
                (Known::Absent, _) | (_, Known::Absent) => continue,
                (Known::Given(_), Known::Given(_)) => {}
                _ => return None,
            }
            let condition = Cond::Binary(
                order.operator().to_string(),
                Box::new(Cond::Name(order.lower.to_string())),
                Box::new(Cond::Name(order.upper.to_string())),
            );
            match self.truth(&condition, given, host) {
                Truth::Holds(origins) => by.extend(origins),
                _ => return None,
            }
        }
        Some(by.into_iter().collect())
    }

    fn standard_rule_broken(
        &mut self,
        record: &ast::RecordExpr,
        host: &str,
        rule: &StandardRule,
        at: &str,
    ) {
        let span = match value_of(record, at) {
            Some(value) => self.span(&value),
            None => record
                .expr()
                .map_or_else(|| self.span(record), |head| self.span(&head)),
        };
        self.out.push(
            Diagnostic::new(
                &rule::TYPE_INVALID_LITERAL,
                format!("This `{host}` breaks its rule `{}`", rule.name),
            )
            .at(span, rule.label)
            .reason(format!(
                "{} A literal is checked while compiling.",
                rule.reason
            ))
            .next(rule.next),
        );
    }
}
