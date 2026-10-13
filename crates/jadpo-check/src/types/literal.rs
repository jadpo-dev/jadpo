//! A literal takes the type it goes into (area 2, question 15): where the
//! expected type is known, a literal must be the right kind of value for
//! it and follow every rule on it and the types it builds on, checked while
//! compiling.

use jadpo_diag::{Diagnostic, Span, rule};

use super::rules::{Broken, Const, check_builtin, check_rules};
use super::table::{Def, TypeTable};
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
}

/// Checks a constant against the type it goes into.
pub fn check(table: &TypeTable, expected: &Ty, value: &Const) -> Result<(), Problem> {
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
            let element = match expected.strip_nullable() {
                Ty::List(element) | Ty::Set(element) => element,
                _ => {
                    return Err(Problem::Kind { fix: None });
                }
            };
            for item in items {
                check(table, element, item)?;
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
    let fits = match value {
        Const::Number { decimal: false, .. } => representation == Prim::Int,
        Const::Number { decimal: true, .. } => representation == Prim::Decimal,
        Const::Str(_) => matches!(
            representation,
            Prim::Text | Prim::Uuid | Prim::Instant | Prim::CalendarDate | Prim::Time | Prim::Duration
        ),
        Const::Duration(_) => representation == Prim::Duration,
        Const::Bool(_) => representation == Prim::Bool,
        _ => false,
    };
    if !fits {
        let fix = match value {
            Const::Number {
                value,
                decimal: false,
            } if representation == Prim::Decimal => Some(format!("{}.0", value.mantissa)),
            _ => None,
        };
        return Err(Problem::Kind { fix });
    }
    check_builtin(base, value).map_err(Problem::Builtin)?;
    if representation == Prim::Int {
        check_builtin(Prim::Int, value).map_err(Problem::Builtin)?;
    }
    for (host, rules) in table.rule_chain(expected) {
        check_rules(rules, value).map_err(|broken| Problem::Broken(host, broken))?;
    }
    Ok(())
}

/// What a type is, for a reason line: "built on `Int`", "an enum".
pub fn what(table: &TypeTable, ty: &Ty) -> String {
    match ty.strip_nullable() {
        Ty::Prim(prim) if prim.is_representation() => "a built-in type".to_string(),
        Ty::Prim(prim) => format!("built on `{}`", prim.representation().name()),
        Ty::List(_) => "a list".to_string(),
        Ty::Set(_) => "a set".to_string(),
        Ty::Map(..) => "a map".to_string(),
        Ty::Named(name) => match table.def(name) {
            Some(Def::Enum { .. }) => "an enum".to_string(),
            Some(Def::Fields { .. }) => "an object with fields".to_string(),
            _ => match table.base_prim(ty) {
                Some(prim) => format!("built on `{}`", prim.name()),
                None => "a named type".to_string(),
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
    table: &TypeTable,
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
    } else if written.ends_with(|c: char| c.is_ascii_alphabetic()) {
        "a duration"
    } else {
        "a whole number"
    }
}
