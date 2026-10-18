//! A literal takes the type it goes into (area 2, question 15): where the
//! expected type is known, a literal must be the right kind of value for
//! it and follow every rule on it and the types it builds on, checked while
//! compiling.

use jadpo_diag::{Diagnostic, Span, rule};

use super::rules::{Broken, Const, check_builtin, check_rules};
use super::table::{Def, TypeView};
use super::ty::{Prim, Ty};

/// Why a literal can't be the expected type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Problem {
    /// The wrong kind of value, with a replacement when one is obvious.
    Kind { fix: Option<String> },
    /// The kind is right, but the built-in type refuses it.
    Builtin(String),
    /// A rule is broken: which type declares it, and how.
    Broken(String, Broken),
    /// A bare number where a quantity of this kind goes, which needs its
    /// unit (area 19, `type.unit_required`).
    UnitRequired(jadpo_std::QuantityKind),
}

/// Checks a constant against the type it goes into.
pub fn check(table: &dyn TypeView, expected: &Ty, value: &Const) -> Result<(), Problem> {
    if expected.has_error() || matches!(expected, Ty::Var(_) | Ty::Opaque(..)) {
        return Ok(());
    }
    match value {
        Const::Other | Const::Path(_) => return Ok(()),
        Const::None => {
            return if table.is_nullable(expected) {
                Ok(())
            } else {
                Err(Problem::Kind { fix: None })
            };
        }
        Const::List(items) => {
            let structure = table.structure(expected);
            let element = match &structure {
                Ty::List(element) | Ty::Set(element) => element,
                // `[]` where a map is expected is an empty map (area 19,
                // question 8); a map literal with entries is still area
                // 2's to design.
                Ty::Map(..) if items.is_empty() => &Ty::Error,
                _ => {
                    return Err(Problem::Kind { fix: None });
                }
            };
            for item in items {
                check(table, element, item)?;
            }
            if matches!(structure, Ty::Set(_))
                && let Some(twice) = repeated(items)
            {
                return Err(Problem::Builtin(format!(
                    "has {twice} twice, and a set holds each value once"
                )));
            }
            for (host, rules) in table.rule_chain(expected) {
                check_rules(rules, value).map_err(|broken| Problem::Broken(host, broken))?;
            }
            return Ok(());
        }
        _ => {}
    }
    let Some(base) = table.base_prim(expected) else {
        let fix = match (value, expected.strip_nullable()) {
            (Const::Str(text), Ty::Named(name)) => table
                .variants(name)
                .and_then(|variants| variants.iter().find(|v| v.name == *text))
                .map(|variant| format!("{name}.{}", variant.name)),
            _ => None,
        };
        return Err(Problem::Kind { fix });
    };
    let representation = base.representation();
    // A quantity is always made with its unit (area 19): a bare number is
    // the mistake its type exists to prevent. A suffix that names no unit
    // is reported where the literal is read.
    if let Prim::Quantity(kind) = representation {
        match value {
            Const::Number { .. } | Const::TooLong => return Err(Problem::UnitRequired(kind)),
            Const::Quantity(_) => match value.quantity() {
                None => return Ok(()),
                Some((unit, _)) if unit.kind != kind => return Err(Problem::Kind { fix: None }),
                Some(_) => {}
            },
            _ => return Err(Problem::Kind { fix: None }),
        }
        for (host, rules) in table.rule_chain(expected) {
            check_rules(rules, value).map_err(|broken| Problem::Broken(host, broken))?;
        }
        return Ok(());
    }
    let fits = match value {
        Const::Number { decimal: false, .. } => representation == Prim::Int,
        Const::Number { decimal: true, .. } => representation == Prim::Decimal,
        // Instants and dates are written in ISO 8601, durations only with
        // a unit (area 10), and a `ZonedTime` is never a literal. Text
        // written in source may stand for text for a person, which a test
        // compares formatted text with (area 10).
        Const::Str(_) => matches!(
            representation,
            Prim::Text | Prim::Uuid | Prim::Instant | Prim::CalendarDate | Prim::PresentationText
        ),
        Const::Duration(_) => representation == Prim::Duration,
        Const::TooLong => representation.is_number(),
        Const::Bool(_) => representation == Prim::Bool,
        _ => false,
    };
    if !fits {
        let fix = match value {
            Const::Number {
                value,
                decimal: false,
            } if representation == Prim::Decimal => {
                let places = table
                    .rule_chain(expected)
                    .iter()
                    .find_map(|(_, rules)| rules.scale)
                    .unwrap_or(1)
                    .max(1);
                Some(format!(
                    "{}.{}",
                    value.mantissa,
                    "0".repeat(places as usize)
                ))
            }
            _ => None,
        };
        return Err(Problem::Kind { fix });
    }
    check_builtin(base, value).map_err(Problem::Builtin)?;
    if representation != base {
        check_builtin(representation, value).map_err(Problem::Builtin)?;
    }
    if representation == Prim::Int {
        check_builtin(Prim::Int, value).map_err(Problem::Builtin)?;
    }
    for (host, rules) in table.rule_chain(expected) {
        check_rules(rules, value).map_err(|broken| Problem::Broken(host, broken))?;
    }
    Ok(())
}

/// The first value a list literal gives twice, as written for a reason
/// line (area 2, question 5).
pub fn repeated(items: &[Const]) -> Option<String> {
    let same = |a: &Const, b: &Const| match (a, b) {
        (Const::Number { value: x, .. }, Const::Number { value: y, .. }) => x.compare(*y).is_eq(),
        (Const::Other | Const::TooLong, _) | (_, Const::Other | Const::TooLong) => false,
        _ => a == b,
    };
    items.iter().enumerate().find_map(|(i, item)| {
        items[..i]
            .iter()
            .any(|earlier| same(earlier, item))
            .then(|| match item {
                Const::Number { value, .. } => format!("`{value}`"),
                Const::Str(text) => format!("`\"{text}\"`"),
                Const::Bool(value) => format!("`{value}`"),
                Const::Duration(text) => format!("`{text}`"),
                Const::Path(names) => format!("`{}`", names.join(".")),
                _ => "a value".to_string(),
            })
    })
}

/// What a type is, for a reason line: "built on `Int`", "an enum".
pub fn what(table: &dyn TypeView, ty: &Ty) -> String {
    match ty.strip_nullable() {
        Ty::Prim(prim) if prim.is_representation() => "a built-in type".to_string(),
        Ty::Prim(prim) => format!("built on `{}`", prim.representation().name()),
        Ty::List(_) => "a list".to_string(),
        Ty::Set(_) => "a set".to_string(),
        Ty::Map(..) => "a map".to_string(),
        Ty::Named(name) => match table.def(name) {
            Some(Def::Enum { .. }) => "an enum".to_string(),
            Some(Def::Fields { .. }) => "an object with fields".to_string(),
            _ => match (table.base_prim(ty), table.structure(ty)) {
                (Some(prim), _) => format!("built on `{}`", prim.name()),
                (None, Ty::List(_)) => "a list".to_string(),
                (None, Ty::Set(_)) => "a set".to_string(),
                (None, Ty::Map(..)) => "a map".to_string(),
                (None, _) => "a named type".to_string(),
            },
        },
        _ => match table.base_prim(ty) {
            Some(prim) => format!("built on `{}`", prim.name()),
            None => "a different type".to_string(),
        },
    }
}

/// The error for a literal that can't be `expected`: `type.mismatch` for
/// the wrong kind of value, `type.invalid_literal` for a broken rule.
/// `constructor` names the type being constructed, whose wrong-kind error is
/// `type.constructor_input`.
pub fn diagnostic(
    table: &dyn TypeView,
    problem: Problem,
    written: &str,
    expected: &Ty,
    span: Span,
    constructor: bool,
) -> Diagnostic {
    let value = describe(written);
    match problem {
        Problem::Kind { fix } => {
            let reason = if written == "none" {
                format!("`{expected}` can't be `none`; only a type written with `?` can.")
            } else {
                format!("This is {value}, and `{expected}` is {}.", what(table, expected))
            };
            let headline = if constructor {
                Diagnostic::new(
                    &rule::TYPE_CONSTRUCTOR_INPUT,
                    format!("`{expected}` can't be built from `{written}`"),
                )
            } else {
                Diagnostic::new(
                    &rule::TYPE_MISMATCH,
                    format!("`{written}` can't be used where `{expected}` is required"),
                )
            };
            let problem = headline.at(span.clone(), "the wrong kind of value").reason(reason);
            match fix {
                Some(fix) => problem
                    .next(format!("Write `{fix}`"))
                    .edit(span, fix),
                None => problem.next(format!("Use a `{expected}` value here")),
            }
        }
        Problem::UnitRequired(kind) => unit_required(kind, written, &expected.to_string(), span),
        Problem::Builtin(why) => {
            let base = table
                .base_prim(expected)
                .map_or_else(|| expected.to_string(), |prim| prim.name().to_string());
            Diagnostic::new(
                &rule::TYPE_INVALID_LITERAL,
                format!("`{written}` isn't a valid `{expected}`"),
            )
            .at(span, format!("not a valid `{base}`"))
            .reason(format!(
                "The value {why}; a literal is checked while compiling."
            ))
            .next(format!("Write a valid `{base}` value"))
        }
        Problem::Broken(host, broken) => Diagnostic::new(
            &rule::TYPE_INVALID_LITERAL,
            format!("`{written}` isn't a valid `{expected}`"),
        )
        .at(span, format!("breaks `{}`", broken.rule))
        .reason(format!(
            "`{host}` declares `{}`, and the value {}. A literal is checked against every rule while compiling.",
            broken.rule, broken.why
        ))
        .next(format!("Use a value that `{expected}` allows")),
    }
}

/// `type.unit_required` (area 19): `written`, a bare number, where a
/// quantity of `kind` goes, as type `expected`. Never declinable.
pub fn unit_required(
    kind: jadpo_std::QuantityKind,
    written: &str,
    expected: &str,
    span: Span,
) -> Diagnostic {
    let base = kind.base_unit();
    let literal = written.parse::<f64>().is_ok() || written.starts_with('-');
    let made = format!("units.of({written}, {}.{})", kind.unit_enum(), base.name);
    let next = if literal && base.suffix {
        format!(
            "Write its unit: `{written}{}`, or `{made}` for another unit",
            base.code
        )
    } else {
        format!(
            "Make it with its unit: `{made}`, or another of `{}`'s",
            kind.unit_enum()
        )
    };
    let an = if kind.name().starts_with(['A', 'E', 'I', 'O', 'U']) {
        "An"
    } else {
        "A"
    };
    Diagnostic::new(
        &rule::TYPE_UNIT_REQUIRED,
        format!("`{expected}` needs a unit, and `{written}` is a bare number"),
    )
    .at(span, "no unit")
    .reason(format!(
        "{an} `{}` is always made with its unit, so a number alone could be any {}: a number without its unit is the mistake the type exists to prevent.",
        kind.name(),
        kind.name().to_lowercase()
    ))
    .next(next)
}

/// `type.unit_unknown` (area 19): a number written with a suffix that is
/// no duration's or quantity's unit, naming each unit it misses only by
/// case (`5mw`, between `mW` and `MW`).
pub fn unit_unknown(written: &str, span: Span) -> Diagnostic {
    let suffix = jadpo_std::quantity::suffix_of(written);
    let near = jadpo_std::quantity::near_misses(suffix);
    let number = written.strip_suffix(suffix).unwrap_or(written);
    if near.is_empty() {
        return Diagnostic::new(
            &rule::TYPE_UNIT_UNKNOWN,
            format!("`{suffix}` isn't a unit, so `{written}` is no value"),
        )
        .at(span, "no such unit")
        .reason(format!(
            "A number's suffix is a duration's unit (`ms`, `s`, `m`, `h`, `d`) or a quantity's, as SI writes it and case matters; no unit is written `{suffix}`. Metres are `metre`, since `m` is minutes, and inches `inch`, since `in` is a keyword."
        ))
        .next("Write a unit Jadpo knows, as the editor offers after the number, or make the value with `units.of`");
    }
    let named: Vec<String> = near
        .iter()
        .map(|unit| format!("`{number}{}` ({})", unit.code, unit.meaning))
        .collect();
    let mut problem = Diagnostic::new(
        &rule::TYPE_UNIT_UNKNOWN,
        format!(
            "`{written}` has no unit `{suffix}`: did you mean {}?",
            crate::or_list(&named)
        ),
    )
    .at(span.clone(), "unit in the wrong case")
    .reason(format!(
        "Units are written as SI writes them, and case matters: {}.",
        crate::and_list(
            &near
                .iter()
                .map(|unit| format!("`{}` is {}", unit.code, unit.meaning))
                .collect::<Vec<_>>()
        )
    ));
    problem = match near.as_slice() {
        [only] => problem
            .next(format!("Write `{number}{}`", only.code))
            .edit(span, format!("{number}{}", only.code)),
        _ => problem.next(format!(
            "Write {}, whichever is meant",
            crate::or_list(&named)
        )),
    };
    problem
}

/// A literal in the reader's words.
fn describe(written: &str) -> &'static str {
    if written == "none" {
        "`none`"
    } else if written.starts_with('"') {
        "text"
    } else if written == "true" || written == "false" {
        "a `Bool`"
    } else if written.starts_with('[') {
        "a list"
    } else if written.contains('.') {
        "a decimal number"
    } else if written.ends_with(|c: char| c.is_ascii_alphabetic())
        && super::rules::is_duration_suffix(jadpo_std::quantity::suffix_of(written))
    {
        "a duration"
    } else if written.ends_with(|c: char| c.is_ascii_alphabetic()) {
        "a quantity"
    } else {
        "a whole number"
    }
}
