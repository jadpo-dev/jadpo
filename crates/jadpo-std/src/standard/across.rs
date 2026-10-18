//! The rules across a standard object's parts (area 19, chunk 5.5t2), each
//! a named rule as an application's object type names its own (area 2,
//! "Rules across fields"): a range's start not after its end, an address's
//! postal code in its country's form, a timecode's frame below its rate.
//!
//! Each rule's one check is here. The checker reads a literal by it while
//! compiling, from what the literal shows of each part, and proves the rest
//! by the comparisons a rule in order lists or leaves them to an `attempt`;
//! the runtime runs it on every value made, an object literal built from
//! values, `text.parse`'s and JSON's, through the plan's
//! [`crate::across::RuleExpr::Standard`]. A rule that reads a table (a
//! country's postal codes, a rating body's categories) can't be a
//! condition in Jadpo, which reads only operators and the standard
//! operations, so every rule is a check of its own here, named, proved,
//! attempted and reported exactly as a condition is.

use std::cmp::Ordering;

use super::recurrence::LocalAt;
use crate::{Num, QuantityKind};

/// What a rule knows of one part.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Known<T> {
    /// It is `none`.
    Absent,
    /// It is given, with its value where that is known.
    Given(Option<T>),
    /// It may or may not be `none`: a value built in code.
    Unknown,
}

impl<T: Copy> Known<T> {
    /// Whether it is given, its value known or not.
    pub fn given(&self) -> bool {
        matches!(self, Known::Given(_))
    }

    /// Whether it is `none`.
    pub fn absent(&self) -> bool {
        matches!(self, Known::Absent)
    }

    /// Its value, where it is given and known.
    pub fn value(&self) -> Option<T> {
        match self {
            Known::Given(value) => *value,
            _ => None,
        }
    }

    /// What is known of the part read through `read`.
    pub fn map<U>(self, read: impl FnOnce(T) -> Option<U>) -> Known<U> {
        match self {
            Known::Absent => Known::Absent,
            Known::Given(value) => Known::Given(value.and_then(read)),
            Known::Unknown => Known::Unknown,
        }
    }
}

/// A part's value, as a rule reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Given<'a> {
    /// A whole or decimal number.
    Number(Num),
    /// A quantity in its kind's base unit, at the kind's scale.
    Quantity(QuantityKind, i128),
    /// Milliseconds since 1970-01-01T00:00:00Z.
    Instant(i64),
    /// Days since 1970-01-01.
    Date(i32),
    /// Milliseconds.
    Duration(i64),
    Text(&'a str),
    /// A standard enum's variant: its place among the variants, and its
    /// code (empty for an enum with none).
    Variant {
        ordinal: u32,
        code: &'a str,
    },
    Bool(bool),
    /// A `LocalDateTime`.
    Local(LocalAt),
    /// A `FrameRate`: its numerator and denominator.
    Rate(i64, i64),
}

impl<'a> Given<'a> {
    /// How two values of one kind compare; `None` across kinds.
    pub fn compare(&self, other: &Given<'_>) -> Option<Ordering> {
        Some(match (self, other) {
            (Given::Number(a), Given::Number(b)) => a.compare(*b),
            (Given::Quantity(kind, a), Given::Quantity(other, b)) if kind == other => a.cmp(b),
            (Given::Instant(a), Given::Instant(b)) | (Given::Duration(a), Given::Duration(b)) => {
                a.cmp(b)
            }
            (Given::Date(a), Given::Date(b)) => a.cmp(b),
            (Given::Local(a), Given::Local(b)) => a.cmp(b),
            _ => return None,
        })
    }

    /// A whole number.
    pub fn whole(&self) -> Option<i64> {
        match self {
            Given::Number(n) if n.scale == 0 => i64::try_from(n.mantissa).ok(),
            _ => None,
        }
    }

    pub fn text(&self) -> Option<&'a str> {
        match self {
            Given::Text(text) => Some(text),
            _ => None,
        }
    }

    /// A variant's code.
    pub fn code(&self) -> Option<&'a str> {
        match self {
            Given::Variant { code, .. } => Some(code),
            _ => None,
        }
    }

    /// A variant's place among its enum's variants.
    pub fn ordinal(&self) -> Option<u32> {
        match self {
            Given::Variant { ordinal, .. } => Some(*ordinal),
            _ => None,
        }
    }

    pub fn flag(&self) -> Option<bool> {
        match self {
            Given::Bool(flag) => Some(*flag),
            _ => None,
        }
    }

    pub fn millis(&self) -> Option<i64> {
        match self {
            Given::Duration(millis) => Some(*millis),
            _ => None,
        }
    }

    pub fn date(&self) -> Option<i32> {
        match self {
            Given::Date(day) => Some(*day),
            _ => None,
        }
    }

    pub fn local(&self) -> Option<LocalAt> {
        match self {
            Given::Local(at) => Some(*at),
            _ => None,
        }
    }

    pub fn rate(&self) -> Option<(i64, i64)> {
        match self {
            Given::Rate(numerator, denominator) => Some((*numerator, *denominator)),
            _ => None,
        }
    }
}

/// An object's parts as a rule reads them, by name.
pub trait Parts {
    fn part(&self, name: &str) -> Known<Given<'_>>;
}

/// Parts listed by name; any not listed is unknown.
#[derive(Clone, Debug, Default)]
pub struct Listed<'a> {
    pub parts: Vec<(&'a str, Known<Given<'a>>)>,
}

impl Parts for Listed<'_> {
    fn part(&self, name: &str) -> Known<Given<'_>> {
        self.parts
            .iter()
            .find(|(listed, _)| *listed == name)
            .map_or(Known::Unknown, |(_, known)| *known)
    }
}

/// What a rule comes to for what is known of the parts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Holds,
    /// It is broken, a literal's refusal pointing at this part.
    Breaks(&'static str),
    /// Only the running program knows.
    Unknown,
}

impl Verdict {
    /// Both hold: the first broken, else unknown if either is.
    fn and(self, other: Verdict) -> Verdict {
        match (self, other) {
            (Verdict::Breaks(at), _) | (_, Verdict::Breaks(at)) => Verdict::Breaks(at),
            (Verdict::Holds, Verdict::Holds) => Verdict::Holds,
            _ => Verdict::Unknown,
        }
    }
}

/// One part of a rule in order: `lower` before `upper`, or at it unless
/// `strict`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Order {
    pub lower: &'static str,
    pub upper: &'static str,
    pub strict: bool,
}

impl Order {
    pub const fn at_most(lower: &'static str, upper: &'static str) -> Order {
        Order {
            lower,
            upper,
            strict: false,
        }
    }

    pub const fn below(lower: &'static str, upper: &'static str) -> Order {
        Order {
            lower,
            upper,
            strict: true,
        }
    }

    /// The operator a condition would write it with: `<` or `<=`.
    pub fn operator(&self) -> &'static str {
        if self.strict { "<" } else { "<=" }
    }

    /// What it comes to; a part that is `none` keeps it, as an open end
    /// does.
    fn verdict(&self, parts: &dyn Parts, at: &'static str) -> Verdict {
        match (parts.part(self.lower), parts.part(self.upper)) {
            (Known::Absent, _) | (_, Known::Absent) => Verdict::Holds,
            (Known::Given(Some(lower)), Known::Given(Some(upper))) => match lower.compare(&upper) {
                Some(Ordering::Less) => Verdict::Holds,
                Some(Ordering::Equal) if !self.strict => Verdict::Holds,
                Some(_) => Verdict::Breaks(at),
                None => Verdict::Unknown,
            },
            _ => Verdict::Unknown,
        }
    }
}

/// How a rule is checked.
#[derive(Clone, Copy, Debug)]
pub enum Check {
    /// Its parts in these orders, all of them, a refusal pointing at `at`.
    /// A value built in code is proved by the same comparisons known around
    /// it, as a condition comparing two fields is (area 2).
    InOrder {
        orders: &'static [Order],
        at: &'static str,
    },
    /// A check of its own over what is known of the parts.
    By(fn(&dyn Parts) -> Verdict),
}

/// A standard object type's rule across its parts.
#[derive(Clone, Copy, Debug)]
pub struct StandardRule {
    /// What an `attempt`'s arm and a failure's report call it.
    pub name: &'static str,
    /// The rule in words, as OpenAPI's description and an error list it:
    /// `start <= end`.
    pub written: &'static str,
    /// A literal's refusal: the label at the part, why, and what to write.
    pub label: &'static str,
    pub reason: &'static str,
    pub next: &'static str,
    pub check: Check,
}

impl StandardRule {
    /// What it comes to for what is known of the parts.
    pub fn verdict(&self, parts: &dyn Parts) -> Verdict {
        match self.check {
            Check::InOrder { orders, at } => orders.iter().fold(Verdict::Holds, |sum, order| {
                sum.and(order.verdict(parts, at))
            }),
            Check::By(check) => check(parts),
        }
    }

    /// Whether a value whose parts are all known keeps it. A rule the parts
    /// can't decide (a part of the wrong kind) counts as broken, so the
    /// value is refused rather than let through unchecked, as an
    /// application's condition that can't be worked out is.
    pub fn holds(&self, parts: &dyn Parts) -> bool {
        self.verdict(parts) == Verdict::Holds
    }

    /// As an error lists it: `rule start_not_after_end: start <= end`.
    pub fn shown(&self) -> String {
        format!("rule {}: {}", self.name, self.written)
    }
}

/// A rule ordering two parts: `lower` before `upper`, or at it unless
/// `strict`, a refusal pointing at `at`.
pub const fn in_order(orders: &'static [Order], at: &'static str) -> Check {
    Check::InOrder { orders, at }
}

/// Text in part `part` that fits the codes of the variants in parts `by`,
/// in `by`'s order, as a postal code fits its address's country; `none`
/// keeps it.
pub fn fits(
    parts: &dyn Parts,
    part: &'static str,
    by: &[&str],
    fit: fn(&str, &[&str]) -> bool,
) -> Verdict {
    let text = match parts.part(part) {
        Known::Absent => return Verdict::Holds,
        Known::Given(Some(given)) => match given.text() {
            Some(text) => text,
            None => return Verdict::Unknown,
        },
        _ => return Verdict::Unknown,
    };
    let mut codes = Vec::with_capacity(by.len());
    for name in by {
        match parts.part(name).value().and_then(|given| given.code()) {
            Some(code) => codes.push(code),
            None => return Verdict::Unknown,
        }
    }
    if fit(text, &codes) {
        Verdict::Holds
    } else {
        Verdict::Breaks(part)
    }
}

/// A `LocalDateTime` written as its canonical text, `2026-10-24T22:00:00`.
pub fn local_at_text(text: &str) -> Option<LocalAt> {
    let (date, clock) = text.split_once('T')?;
    Some(LocalAt {
        date: crate::date_number(date)?,
        second: crate::clock_millis(clock)? / 1000,
    })
}

/// The standard type part `part` of standard object `ty` is built on, if
/// it is one: `FrameRate` for a `Timecode`'s `rate`.
pub fn part_standard(ty: &str, part: &str) -> Option<&'static str> {
    let super::Holds::Object(fields) = super::standard_type(ty)?.holds else {
        return None;
    };
    match fields
        .iter()
        .find(|(name, _)| *name == part)?
        .1
        .required()
        .0
    {
        super::Part::Standard(other) => Some(other),
        _ => None,
    }
}

/// `Range<T>`, `InstantRange` and `DateRange`: a start not after the end.
/// An open end of a `Range<T>` keeps it.
pub const RANGE_RULES: &[StandardRule] = &[StandardRule {
    name: "start_not_after_end",
    written: "start <= end",
    label: "before the start",
    reason: "A range runs from its `start` to its `end`, so its end is never before its start; a range whose start is its end is empty, or a single day for a `DateRange`.",
    next: "Write an end at or after the start, or swap the two",
    check: in_order(&[Order::at_most("start", "end")], "end"),
}];

/// The rules across the parts of standard object type `ty` (`Range` for
/// `Range<T>`), in the order they are checked; none for any other type.
pub fn rules(ty: &str) -> &'static [StandardRule] {
    match ty {
        crate::RANGE | super::INSTANT_RANGE | super::DATE_RANGE => RANGE_RULES,
        super::ISO_WEEK => super::ISO_WEEK_RULES,
        super::events::CALENDAR_EVENT => super::events::RULES,
        super::contact::POSTAL_ADDRESS_NAME => super::contact::POSTAL_ADDRESS_RULES,
        super::media::CONTENT_RATING => super::media::CONTENT_RATING_RULES,
        super::media::TIMECODE => super::media::TIMECODE_RULES,
        super::media::SUBTITLE_CUE => super::media::SUBTITLE_CUE_RULES,
        super::records::RATING => super::records::RATING_RULES,
        super::health::BLOOD_PRESSURE => super::health::BLOOD_PRESSURE_RULES,
        super::geo::MAP_TILE => super::geo::MAP_TILE_RULES,
        super::geo::BOUNDING_BOX => super::geo::BOUNDING_BOX_RULES,
        super::ai::TOKEN_USAGE => super::ai::TOKEN_USAGE_RULES,
        super::security::PASSWORD_POLICY => super::security::PASSWORD_POLICY_RULES,
        super::security::JWK => super::jwk::RULES,
        _ => super::documents::rules(ty),
    }
}

/// The rule of `ty` called `name`.
pub fn rule(ty: &str, name: &str) -> Option<&'static StandardRule> {
    rules(ty).iter().find(|rule| rule.name == name)
}

/// The names of `ty`'s parts in their declared order, as its value's
/// fields hold them: its entry's, or a range's `start` and `end`.
pub fn part_names(ty: &str) -> Vec<&'static str> {
    match super::standard_type(ty).map(|entry| entry.holds) {
        Some(super::Holds::Object(fields)) => fields.iter().map(|(name, _)| *name).collect(),
        Some(super::Holds::Range) => crate::RANGE_FIELDS.to_vec(),
        _ => Vec::new(),
    }
}

/// `ty`'s rules as the plan carries them on its validator, each run by its
/// check here.
pub fn cross_rules(ty: &str) -> Vec<crate::across::CrossRule> {
    rules(ty)
        .iter()
        .map(|rule| crate::across::CrossRule {
            name: rule.name.to_string(),
            locals: 0,
            check: crate::across::RuleExpr::Standard {
                ty: ty.to_string(),
                rule: rule.name.to_string(),
            },
        })
        .collect()
}

/// The first of `ty`'s rules a value whose parts are `parts` breaks.
pub fn first_broken(ty: &str, parts: &dyn Parts) -> Option<&'static StandardRule> {
    rules(ty).iter().find(|rule| !rule.holds(parts))
}

/// "Rules across its parts: `start_not_after_end`: `start <= end`.", as
/// OpenAPI describes a standard object, or `None` for one without.
pub fn described(ty: &str) -> Option<String> {
    let rules = rules(ty);
    if rules.is_empty() {
        return None;
    }
    let listed: Vec<String> = rules
        .iter()
        .map(|rule| format!("`{}`: {}", rule.name, rule.written))
        .collect();
    Some(format!("Rules across its parts: {}.", listed.join("; ")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listed<'a>(parts: &[(&'a str, Known<Given<'a>>)]) -> Listed<'a> {
        Listed {
            parts: parts.to_vec(),
        }
    }

    fn number(n: i128) -> Known<Given<'static>> {
        Known::Given(Some(Given::Number(Num::whole(n))))
    }

    #[test]
    fn a_range_refuses_only_an_end_known_before_its_start() {
        let rule = &RANGE_RULES[0];
        let range = |start, end| rule.verdict(&listed(&[("start", start), ("end", end)]));
        assert_eq!(range(number(1), number(2)), Verdict::Holds);
        assert_eq!(range(number(2), number(2)), Verdict::Holds);
        assert_eq!(range(number(3), number(2)), Verdict::Breaks("end"));
        assert_eq!(range(Known::Absent, number(2)), Verdict::Holds);
        assert_eq!(range(number(3), Known::Given(None)), Verdict::Unknown);
        assert_eq!(range(Known::Unknown, number(2)), Verdict::Unknown);
    }

    #[test]
    fn every_rule_has_a_name_of_its_own_and_reads_its_type_s_parts() {
        for entry in super::super::STANDARD_TYPES {
            let names = part_names(entry.name);
            let rules = rules(entry.name);
            for (at, rule) in rules.iter().enumerate() {
                assert!(
                    rule.name
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
                    "{}: `{}` is lower_snake_case",
                    entry.name,
                    rule.name
                );
                assert!(
                    !names.contains(&rule.name),
                    "{}: `{}` is a part",
                    entry.name,
                    rule.name
                );
                assert_ne!(rule.name, "invalid");
                assert!(
                    rules[..at].iter().all(|earlier| earlier.name != rule.name),
                    "{}: `{}` twice",
                    entry.name,
                    rule.name
                );
                if let Check::InOrder { orders, at } = rule.check {
                    assert!(names.contains(&at), "{}: `{at}`", entry.name);
                    for order in orders {
                        assert!(names.contains(&order.lower), "{}", entry.name);
                        assert!(names.contains(&order.upper), "{}", entry.name);
                    }
                }
                // Nothing known decides nothing.
                assert_eq!(
                    rule.verdict(&Listed::default()),
                    Verdict::Unknown,
                    "{}: `{}`",
                    entry.name,
                    rule.name
                );
            }
        }
    }
}
