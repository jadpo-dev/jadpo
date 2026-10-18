//! A `CalendarEvent` literal's rules across its parts (area 19, chunk
//! 5.5k3), proved while compiling where the literal shows them: its days
//! or its times, a last day on or after the first, an end after the start,
//! and a recurrence only on a timed event, starting when it does. A part
//! written as `none` is known to be absent and one of a type that can't be
//! `none` known to be given; one built from a value that may be `none`
//! waits on cross-field rules (chunk 5.5t), as an address's postal code
//! built from values does. The rules are `jadpo-std`'s, which the runtime
//! checks a value read from text or JSON by.

use jadpo_diag::{Diagnostic, rule};
use jadpo_std::standard::events::{CALENDAR_EVENT, Known, Parts, broken};
use jadpo_std::standard::recurrence::LocalAt;
use jadpo_syntax::ast::{self, AstNode, Expr};

use super::super::rules::Const;
use super::{Body, node_key};
use crate::case::starts_upper;
use crate::tree;

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

/// A local date and time written as a `LocalDateTime` literal of a date
/// and a `LocalTime` literal.
fn local_at(expr: &Expr) -> Option<LocalAt> {
    let Expr::RecordExpr(record) = expr else {
        return None;
    };
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

impl Body<'_> {
    /// What a literal shows of one of an event's parts.
    fn known_part<T>(&self, value: Option<&Expr>, read: impl Fn(&Expr) -> Option<T>) -> Known<T> {
        let Some(value) = value else {
            return Known::Unknown;
        };
        match Const::from_expr(value) {
            Const::None => return Known::Absent,
            // Written out: text, a number, a variant, or an object.
            Const::Str(_) | Const::Number { .. } => return Known::Given(read(value)),
            Const::Path(path) if path.first().is_some_and(|head| starts_upper(head)) => {
                return Known::Given(read(value));
            }
            _ if matches!(value, Expr::RecordExpr(_)) => return Known::Given(read(value)),
            _ => {}
        }
        match self.facts.types.get(&node_key(value.syntax())) {
            Some(ty) if !ty.has_error() && !self.table.is_nullable(ty) => Known::Given(read(value)),
            _ => Known::Unknown,
        }
    }

    /// A `CalendarEvent` literal's rules across its parts, where it shows
    /// them.
    pub(super) fn event_rules_hold(&mut self, name: &str, record: &ast::RecordExpr) {
        if name != CALENDAR_EVENT {
            return;
        }
        let value = |part: &str| value_of(record, part);
        let date = |expr: &Expr| match Const::from_expr(expr) {
            Const::Str(text) => jadpo_std::date_number(&text),
            _ => None,
        };
        let zone = |expr: &Expr| match Const::from_expr(expr) {
            Const::Path(path) => match path.as_slice() {
                [head, variant] if head == jadpo_std::ZONE => {
                    jadpo_std::variant_ordinal(jadpo_std::ZONE, variant)
                }
                _ => None,
            },
            _ => None,
        };
        let recurrence_text = value("recurrence").and_then(|expr| match Const::from_expr(&expr) {
            Const::Str(text) => Some(text),
            _ => None,
        });
        let recurrence = match self.known_part(value("recurrence").as_ref(), |_| Some(())) {
            Known::Given(_) => Known::Given(recurrence_text.as_deref()),
            Known::Absent => Known::Absent,
            Known::Unknown => Known::Unknown,
        };
        let parts = Parts {
            first_day: self.known_part(value("first_day").as_ref(), date),
            last_day: self.known_part(value("last_day").as_ref(), date),
            start: self.known_part(value("start").as_ref(), local_at),
            end: self.known_part(value("end").as_ref(), local_at),
            zone: self.known_part(value("zone").as_ref(), zone),
            recurrence,
        };
        let Some(broken) = broken(&parts) else {
            return;
        };
        let Some(written) = value(broken.part) else {
            return;
        };
        self.out.push(
            Diagnostic::new(
                &rule::TYPE_INVALID_LITERAL,
                format!(
                    "This event's `{}` breaks a rule across its parts",
                    broken.part
                ),
            )
            .at(self.span(&written), broken.rule.label)
            .reason(broken.rule.reason)
            .next(broken.rule.next),
        );
    }
}
