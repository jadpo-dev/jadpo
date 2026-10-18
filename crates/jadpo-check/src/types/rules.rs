//! The rules a valid value follows (area 2): constraints written in a
//! type's block or a field's settings, what each fits, and checking a
//! literal against them while compiling.

use jadpo_syntax::ast::{self, AstNode, Expr};

use jadpo_std::{TextFormat, UuidVersion};

use super::ty::Prim;
use crate::tree::{self, dotted};

pub use jadpo_std::{Broken, Num, Rules, compile_pattern};

/// A literal, or a setting's value, as the checker reads it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Const {
    /// A number; `decimal` when written with a point.
    Number {
        value: Num,
        decimal: bool,
    },
    /// A duration such as `30s` (area 10).
    Duration(String),
    /// A physical quantity such as `2.5kg` or `-40degC`, as written (area
    /// 19, "Physical quantities"): any other suffix than a duration's,
    /// which may name no unit at all (`type.unit_unknown`).
    Quantity(String),
    /// A percentage such as `20%` or `-12.5%`, as written (area 19): the
    /// fraction its number over 100 is.
    Percentage(String),
    /// A string, with its escapes decoded.
    Str(String),
    Bool(bool),
    None,
    /// A name or a dotted name: `email`, `TextFormat.email`.
    Path(Vec<String>),
    List(Vec<Const>),
    /// A number with more digits than an exact number holds (38).
    TooLong,
    /// Anything else: computed, not a constant.
    Other,
}

impl Const {
    /// The literal it is, as typed bodies and the graph hold one; `None`
    /// for a name, a list or anything computed.
    pub fn literal(&self) -> Option<jadpo_graph::body::Literal> {
        use jadpo_graph::body::Literal;
        Some(match self {
            Const::Number { value, decimal } => Literal::Number {
                value: *value,
                decimal: *decimal,
            },
            Const::Duration(written) => Literal::Duration(written.clone()),
            Const::Quantity(written) => Literal::Quantity(written.clone()),
            Const::Percentage(written) => Literal::Percentage(written.clone()),
            Const::Str(text) => Literal::Text(text.clone()),
            Const::Bool(value) => Literal::Bool(*value),
            Const::None => Literal::None,
            Const::Path(_) | Const::List(_) | Const::TooLong | Const::Other => return None,
        })
    }

    /// Reads a member's value or an expression as a constant.
    pub fn from_expr(expr: &Expr) -> Const {
        match expr {
            Expr::Literal(literal) => Const::from_literal(literal, false),
            Expr::PrefixExpr(prefix)
                if prefix.syntax().first_token().map(|t| t.text().to_string())
                    == Some("-".into()) =>
            {
                match prefix.expr() {
                    Some(Expr::Literal(literal)) => Const::from_literal(&literal, true),
                    _ => Const::Other,
                }
            }
            Expr::NameRef(_) | Expr::FieldExpr(_) => match dotted(expr) {
                Some((first, rest)) => std::iter::once(&first)
                    .chain(rest.iter())
                    .map(tree::name_ref_text)
                    .collect::<Option<Vec<_>>>()
                    .map_or(Const::Other, Const::Path),
                None => Const::Other,
            },
            Expr::ListExpr(list) => {
                Const::List(list.exprs().map(|e| Const::from_expr(&e)).collect())
            }
            Expr::ParenExpr(paren) => paren.expr().map_or(Const::Other, |e| Const::from_expr(&e)),
            _ => Const::Other,
        }
    }

    pub fn from_literal(literal: &ast::Literal, negative: bool) -> Const {
        let Some(token) = literal.value() else {
            return Const::Other;
        };
        let text = token.text();
        let sign = if negative { "-" } else { "" };
        match token.kind() {
            jadpo_syntax::SyntaxKind::INT_NUMBER => {
                Num::parse(&format!("{sign}{text}")).map_or(Const::TooLong, |value| Const::Number {
                    value,
                    decimal: false,
                })
            }
            jadpo_syntax::SyntaxKind::DECIMAL_NUMBER => Num::parse(&format!("{sign}{text}"))
                .map_or(Const::TooLong, |value| Const::Number {
                    value,
                    decimal: true,
                }),
            // A number and `%` is a percentage, which may be negative
            // (area 19).
            jadpo_syntax::SyntaxKind::UNIT_NUMBER if text.ends_with('%') => {
                Const::Percentage(format!("{sign}{text}"))
            }
            // A duration's suffix (`ms`, `s`, `m`, `h`, `d`) makes a
            // duration; any other a quantity, which may be negative
            // (`-40degC`), area 19.
            jadpo_syntax::SyntaxKind::UNIT_NUMBER
                if is_duration_suffix(jadpo_std::quantity::suffix_of(text)) =>
            {
                if negative {
                    Const::Other
                } else {
                    Const::Duration(text.to_string())
                }
            }
            jadpo_syntax::SyntaxKind::UNIT_NUMBER => Const::Quantity(format!("{sign}{text}")),
            jadpo_syntax::SyntaxKind::STRING if !negative => Const::Str(unescape(text)),
            jadpo_syntax::SyntaxKind::TRUE_KW if !negative => Const::Bool(true),
            jadpo_syntax::SyntaxKind::FALSE_KW if !negative => Const::Bool(false),
            jadpo_syntax::SyntaxKind::NONE_KW if !negative => Const::None,
            _ => Const::Other,
        }
    }

    /// What kind of value it is, in the reader's words.
    pub fn describe(&self) -> &'static str {
        match self {
            Const::Number { decimal: true, .. } => "a decimal number",
            Const::Number { .. } => "a whole number",
            Const::Duration(_) => "a duration",
            Const::Quantity(_) => "a quantity",
            Const::Percentage(_) => "a percentage",
            Const::Str(_) => "text",
            Const::Bool(_) => "a `Bool`",
            Const::None => "`none`",
            Const::Path(_) => "a name",
            Const::List(_) => "a list",
            Const::TooLong => "a number",
            Const::Other => "a computed value",
        }
    }
}

/// Whether `suffix` is a duration's unit (area 10): `5m` is minutes.
pub fn is_duration_suffix(suffix: &str) -> bool {
    matches!(suffix, "ms" | "s" | "m" | "h" | "d")
}

impl Const {
    /// The unit and the value in its kind's base unit of a quantity
    /// literal whose suffix names a unit.
    pub fn quantity(&self) -> Option<(&'static jadpo_std::QuantityUnit, i128)> {
        match self {
            Const::Quantity(text) => jadpo_std::quantity::parse_literal(text).ok(),
            _ => None,
        }
    }

    /// The fraction a percentage literal is: `20%` is 0.20.
    pub fn percentage(&self) -> Option<Num> {
        match self {
            Const::Percentage(text) => jadpo_std::percentage::parse_literal(text),
            _ => None,
        }
    }
}

/// A string literal's text without quotes, escapes decoded (area 2).
pub(crate) fn unescape(text: &str) -> String {
    let inner = text
        .strip_prefix('"')
        .and_then(|t| t.strip_suffix('"'))
        .unwrap_or(text);
    let mut out = String::new();
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some(other) => out.push(other),
            None => {}
        }
    }
    out
}

/// What a constraint applies to: a value's representation, or a
/// collection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Subject {
    Prim(Prim),
    Collection,
    /// The standard `ExchangeRate`, whose currencies `from:` and `to:`
    /// fix (area 19, chunk 5.5h4).
    ExchangeRate,
    /// The standard `Vector`, whose length `dimensions:` fixes (area 19,
    /// chunk 5.5s2), a list as its lengths go.
    Vector,
    /// An object, enum or a type a later area defines: no constraints.
    Other,
}

/// The constraints area 2 defines, in the order the reader learns them.
pub const CONSTRAINTS: [&str; 14] = [
    "min",
    "max",
    "min_length",
    "max_length",
    "pattern",
    "format",
    "scale",
    "version",
    "currency",
    "rounding",
    "from",
    "to",
    "dimensions",
    "distance",
];

/// The settings a field of an `Input` may have beside its constraints.
pub const FIELD_SETTINGS: [&str; 3] = ["optional", "default", "header"];

/// Why a constraint can't be used.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Problem {
    /// Not a constraint at all.
    Unknown,
    /// A constraint, but not for this kind of value: which kinds it fits.
    Base(&'static str),
    /// The right constraint with the wrong kind of value: what it takes,
    /// and a replacement when one is obvious.
    Value(String, Option<String>),
    /// A pattern Jadpo can't use, and why.
    Pattern(String),
    /// A bare number as a quantity type's bound, which needs its unit
    /// (area 19, `type.unit_required`).
    UnitRequired,
}

/// One constraint read and checked against what it constrains.
pub fn constraint(
    subject: Subject,
    name: &str,
    value: &Const,
    rules: &mut Rules,
) -> Result<(), Problem> {
    let text_like = matches!(subject, Subject::Prim(p) if p.representation() == Prim::Text);
    match name {
        "min" | "max" => {
            let parsed = match (subject, value) {
                (
                    Subject::Prim(Prim::Int),
                    Const::Number {
                        value,
                        decimal: false,
                    },
                ) => Some(*value),
                (Subject::Prim(Prim::Int), _) => {
                    return Err(Problem::Value("a whole number, such as `0`".into(), None));
                }
                (
                    Subject::Prim(Prim::Decimal),
                    Const::Number {
                        value,
                        decimal: true,
                    },
                ) => Some(*value),
                (
                    Subject::Prim(Prim::Decimal),
                    Const::Number {
                        value,
                        decimal: false,
                    },
                ) => {
                    return Err(Problem::Value(
                        "a decimal number, such as `0.0`".into(),
                        Some(format!("{}.0", value.mantissa)),
                    ));
                }
                (Subject::Prim(Prim::Decimal), _) => {
                    return Err(Problem::Value(
                        "a decimal number, such as `0.0`".into(),
                        None,
                    ));
                }
                (Subject::Prim(Prim::Duration), Const::Duration(text)) => {
                    let Some(bound) = jadpo_std::Dur::parse(text) else {
                        return Err(Problem::Value("a duration, such as `30s`".into(), None));
                    };
                    if name == "min" {
                        rules.min_duration = Some(bound);
                    } else {
                        rules.max_duration = Some(bound);
                    }
                    return Ok(());
                }
                (Subject::Prim(Prim::Duration), _) => {
                    return Err(Problem::Value("a duration, such as `30s`".into(), None));
                }
                // An amount's bounds count its minor units (area 19).
                (
                    Subject::Prim(Prim::Money),
                    Const::Number {
                        value,
                        decimal: false,
                    },
                ) => Some(*value),
                (Subject::Prim(Prim::Money), _) => {
                    return Err(Problem::Value(
                        "a whole number of minor units, such as `0`".into(),
                        None,
                    ));
                }
                // A percentage's bounds are written with `%` and held as
                // fractions, as its values are (area 19).
                (Subject::Prim(Prim::Percentage), found @ Const::Percentage(_)) => {
                    match found.percentage() {
                        Some(fraction) => Some(fraction),
                        None => {
                            return Err(Problem::Value(
                                "a percentage written with `%`, such as `0%`".into(),
                                None,
                            ));
                        }
                    }
                }
                // A bare number could be a ratio or a percentage (0.2 or
                // 20), so no edit guesses which.
                (Subject::Prim(Prim::Percentage), _) => {
                    return Err(Problem::Value(
                        "a percentage written with `%`, such as `0%`".into(),
                        None,
                    ));
                }
                // A quantity's bounds are written with a unit and held in
                // its kind's base unit at its scale, as its values are
                // (area 19).
                (Subject::Prim(Prim::Quantity(kind)), found) => match found.quantity() {
                    Some((unit, amount)) if unit.kind == kind => Some(Num {
                        mantissa: amount,
                        scale: kind.scale(),
                    }),
                    _ if matches!(found, Const::Number { .. }) => {
                        return Err(Problem::UnitRequired);
                    }
                    _ => {
                        let base = kind.base_unit();
                        let example = if base.suffix {
                            format!("`0{}`", base.code)
                        } else {
                            format!("`units.of(0, {}.{})`", kind.unit_enum(), base.name)
                        };
                        return Err(Problem::Value(
                            format!(
                                "a `{}` written with its unit, such as {example}",
                                kind.name()
                            ),
                            None,
                        ));
                    }
                },
                _ => {
                    return Err(Problem::Base(
                        "`Int`, `Decimal`, `Duration`, `Money`, `Percentage` and quantity types",
                    ));
                }
            };
            if name == "min" {
                rules.min = parsed;
            } else {
                rules.max = parsed;
            }
        }
        "min_length" | "max_length" => {
            if !(text_like
                || subject == Subject::Prim(Prim::Bytes)
                || subject == Subject::Collection
                || subject == Subject::Vector)
            {
                return Err(Problem::Base("`Text` and `Bytes` types and collections"));
            }
            let length = match value {
                Const::Number {
                    value,
                    decimal: false,
                } if value.mantissa >= 0 => u64::try_from(value.mantissa).ok(),
                _ => None,
            };
            let Some(length) = length else {
                return Err(Problem::Value("a whole number of 0 or more".into(), None));
            };
            if name == "min_length" {
                rules.min_length = Some(length);
            } else {
                rules.max_length = Some(length);
            }
        }
        "pattern" => {
            if !text_like {
                return Err(Problem::Base("`Text` types"));
            }
            let Const::Str(pattern) = value else {
                return Err(Problem::Value(
                    "a regular expression in a string, such as `\"[a-z]+\"`".into(),
                    None,
                ));
            };
            compile_pattern(pattern).map_err(Problem::Pattern)?;
            rules.pattern = Some(pattern.clone());
        }
        "format" => {
            if !text_like {
                return Err(Problem::Base("`Text` types"));
            }
            let named = |path: &[String]| match path {
                [host, variant] if host == TextFormat::NAME => TextFormat::from_name(variant),
                _ => None,
            };
            match value {
                Const::Path(path) if named(path).is_some() => {
                    rules.format = named(path);
                }
                Const::Path(path)
                    if path.len() == 1 && TextFormat::from_name(&path[0]).is_some() =>
                {
                    let written = format!("{}.{}", TextFormat::NAME, path[0]);
                    return Err(Problem::Value(
                        format!("a `{}` value, such as `{written}`", TextFormat::NAME),
                        Some(written),
                    ));
                }
                _ => {
                    let listed: Vec<String> = TextFormat::NAMES
                        .iter()
                        .map(|name| format!("`{}.{name}`", TextFormat::NAME))
                        .collect();
                    return Err(Problem::Value(
                        match listed.as_slice() {
                            [only] => {
                                format!("a `{}` value; {only} is the only one", TextFormat::NAME)
                            }
                            _ => format!(
                                "a `{}` value, one of {}",
                                TextFormat::NAME,
                                listed.join(", ")
                            ),
                        },
                        None,
                    ));
                }
            }
        }
        "scale" => {
            if subject != Subject::Prim(Prim::Decimal) {
                return Err(Problem::Base("`Decimal` types"));
            }
            match value {
                Const::Number {
                    value,
                    decimal: false,
                } if (0..=18).contains(&value.mantissa) => {
                    rules.scale = u32::try_from(value.mantissa).ok();
                }
                _ => {
                    return Err(Problem::Value(
                        "a whole number of decimal places from 0 to 18".into(),
                        None,
                    ));
                }
            }
        }
        "version" => {
            if subject != Subject::Prim(Prim::Uuid) {
                return Err(Problem::Base("`Uuid` types"));
            }
            let example = format!(
                "a `{}` value, such as `{}.{}`",
                UuidVersion::NAME,
                UuidVersion::NAME,
                UuidVersion::V4.name()
            );
            match value {
                Const::Path(path) if path.len() == 2 && path[0] == UuidVersion::NAME => {
                    match UuidVersion::from_name(&path[1]) {
                        Some(version) => rules.version = Some(version),
                        None => return Err(Problem::Value(example, None)),
                    }
                }
                Const::Path(path)
                    if path.len() == 1 && UuidVersion::from_name(&path[0]).is_some() =>
                {
                    return Err(Problem::Value(
                        example,
                        Some(format!("{}.{}", UuidVersion::NAME, path[0])),
                    ));
                }
                _ => return Err(Problem::Value(example, None)),
            }
        }
        // The one currency a `Money` type's amounts are in (area 19).
        "currency" => {
            if subject != Subject::Prim(Prim::Money) {
                return Err(Problem::Base("`Money` types"));
            }
            rules.currency = Some(currency_value(value)?);
        }
        // The currencies a type built on `ExchangeRate` converts from and
        // to (area 19, chunk 5.5h4).
        "from" | "to" => {
            if subject != Subject::ExchangeRate {
                return Err(Problem::Base("`ExchangeRate` types"));
            }
            let code = Some(currency_value(value)?);
            if name == "from" {
                rules.rate_from = code;
            } else {
                rules.rate_to = code;
            }
        }
        // How many items a type built on `Vector` has (area 19, chunk
        // 5.5s2): one number, as many as a vector can hold.
        "dimensions" => {
            if subject != Subject::Vector {
                return Err(Problem::Base("`Vector` types"));
            }
            let most = jadpo_std::standard::ai::MOST_DIMENSIONS;
            let count = match value {
                Const::Number {
                    value,
                    decimal: false,
                } if value.mantissa >= 1 => {
                    u64::try_from(value.mantissa).ok().filter(|n| *n <= most)
                }
                _ => None,
            };
            let Some(count) = count else {
                return Err(Problem::Value(
                    format!("a whole number from 1 to {most}"),
                    None,
                ));
            };
            rules.dimensions = Some(count);
        }
        // How a type built on `Vector` compares its vectors (area 21,
        // decision 14): a `VectorDistance` variant.
        "distance" => {
            if subject != Subject::Vector {
                return Err(Problem::Base("`Vector` types"));
            }
            let name = jadpo_std::VectorDistance::NAME;
            let example = format!("a `{name}` value, such as `{name}.cosine`");
            match value {
                Const::Path(path) if path.len() == 2 && path[0] == name => {
                    match jadpo_std::VectorDistance::from_name(&path[1]) {
                        Some(distance) => rules.distance = Some(distance),
                        None => return Err(Problem::Value(example, None)),
                    }
                }
                Const::Path(path)
                    if path.len() == 1
                        && jadpo_std::VectorDistance::from_name(&path[0]).is_some() =>
                {
                    return Err(Problem::Value(example, Some(format!("{name}.{}", path[0]))));
                }
                _ => return Err(Problem::Value(example, None)),
            }
        }
        // How `*` and `/` round a `Money` type's amounts (area 19).
        "rounding" => {
            if subject != Subject::Prim(Prim::Money) {
                return Err(Problem::Base("`Money` types"));
            }
            let named = |path: &[String]| match path {
                [host, variant] if host == jadpo_std::Rounding::NAME => {
                    jadpo_std::Rounding::from_name(variant)
                }
                _ => None,
            };
            match value {
                Const::Path(path) if named(path).is_some() => rules.rounding = named(path),
                Const::Path(path)
                    if path.len() == 1 && jadpo_std::Rounding::from_name(&path[0]).is_some() =>
                {
                    let written = format!("{}.{}", jadpo_std::Rounding::NAME, path[0]);
                    return Err(Problem::Value(
                        format!(
                            "a `{}` value, such as `{written}`",
                            jadpo_std::Rounding::NAME
                        ),
                        Some(written),
                    ));
                }
                _ => {
                    return Err(Problem::Value(
                        format!(
                            "a `{}` value, such as `{}.half_up`",
                            jadpo_std::Rounding::NAME,
                            jadpo_std::Rounding::NAME
                        ),
                        None,
                    ));
                }
            }
        }
        _ => return Err(Problem::Unknown),
    }
    Ok(())
}

/// A `Currency` variant written out, `Currency.gbp`, by its place among
/// `Currency`'s variants: a `Money` type's `currency:`, and a rate type's
/// `from:` and `to:`.
fn currency_value(value: &Const) -> Result<u32, Problem> {
    let named = |path: &[String]| match path {
        [host, code] if host == jadpo_std::CURRENCY => jadpo_std::money::ordinal_of_variant(code),
        _ => None,
    };
    match value {
        Const::Path(path) if let Some(code) = named(path) => Ok(code),
        Const::Path(path)
            if path.len() == 1 && jadpo_std::money::ordinal_of_variant(&path[0]).is_some() =>
        {
            let written = format!("{}.{}", jadpo_std::CURRENCY, path[0]);
            Err(Problem::Value(
                format!("a `{}` value, such as `{written}`", jadpo_std::CURRENCY),
                Some(written),
            ))
        }
        _ => Err(Problem::Value(
            format!(
                "a `{}` value, such as `{}.gbp`",
                jadpo_std::CURRENCY,
                jadpo_std::CURRENCY
            ),
            None,
        )),
    }
}

/// Checks a constant against one declaration's rules, with the
/// validators the runtime runs on values (`jadpo-std`).
pub fn check_rules(rules: &Rules, value: &Const) -> Result<(), Broken> {
    match value {
        Const::Number { value, .. } => rules.check_number(*value),
        Const::Duration(text) => match jadpo_std::Dur::parse(text) {
            Some(value) => rules.check_duration(value),
            None => Ok(()),
        },
        Const::Str(text) => rules.check_text(text),
        Const::Quantity(_) => match value.quantity() {
            Some((unit, amount)) => rules.check_quantity(unit.kind, amount),
            None => Ok(()),
        },
        Const::Percentage(_) => match value.percentage() {
            Some(fraction) => rules.check_percentage(fraction),
            None => Ok(()),
        },
        Const::List(items) => rules.check_length(items.len() as u64, "items"),
        _ => Ok(()),
    }
}

/// A number as written, with its decimal places.
pub fn show(n: Num) -> String {
    n.to_string()
}

/// What a built-in type itself requires of a literal, beyond its kind.
pub fn check_builtin(prim: Prim, value: &Const) -> Result<(), String> {
    match (prim, value) {
        (Prim::Int, Const::Number { value, .. }) if !value.fits_int() => {
            Err("is outside `Int`'s range, a 64-bit integer".into())
        }
        (prim, Const::Str(text)) => jadpo_std::check_builtin_text(prim, text),
        (Prim::Duration, Const::Duration(text)) if jadpo_std::Dur::parse(text).is_none() => {
            Err("isn't a duration, written with a unit: `500ms`, `30s`, `5m`, `2h` or `30d`".into())
        }
        (_, Const::TooLong) => Err("has more digits than any Jadpo number holds".into()),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn number(text: &str) -> Const {
        Const::Number {
            value: Num::parse(text).expect("a number"),
            decimal: text.contains('.'),
        }
    }

    #[test]
    fn constraints_fit_their_base() {
        let mut rules = Rules::default();
        assert_eq!(
            constraint(Subject::Prim(Prim::Text), "min", &number("1"), &mut rules),
            Err(Problem::Base(
                "`Int`, `Decimal`, `Duration`, `Money`, `Percentage` and quantity types"
            ))
        );
        assert!(
            constraint(
                Subject::Prim(Prim::Email),
                "max_length",
                &number("80"),
                &mut rules
            )
            .is_ok()
        );
        assert!(matches!(
            constraint(Subject::Prim(Prim::Decimal), "min", &number("0"), &mut rules),
            Err(Problem::Value(_, Some(fix))) if fix == "0.0"
        ));
        assert_eq!(
            constraint(
                Subject::Prim(Prim::Int),
                "maxlength",
                &number("1"),
                &mut rules
            ),
            Err(Problem::Unknown)
        );
    }

    #[test]
    fn literals_are_checked_with_the_runtimes_validators() {
        let rules = Rules {
            pattern: Some("[A-Z]{3}-[0-9]{4}".into()),
            ..Rules::default()
        };
        assert!(check_rules(&rules, &Const::Str("ABC-1234".into())).is_ok());
        assert!(check_rules(&rules, &Const::Str("ABC-1234x".into())).is_err());
    }

    #[test]
    fn literals_break_the_rule_they_break() {
        let rules = Rules {
            min: Num::parse("1"),
            max: Num::parse("1000"),
            ..Rules::default()
        };
        let broken = check_rules(&rules, &number("0")).expect_err("below min");
        assert_eq!(broken.rule, "min: 1");
        let scaled = Rules {
            scale: Some(2),
            ..Rules::default()
        };
        assert!(check_rules(&scaled, &number("1.250")).is_ok());
        assert!(check_rules(&scaled, &number("1.255")).is_err());
        assert!(check_builtin(Prim::Uuid, &Const::Str("not a uuid".into())).is_err());
        assert!(
            check_builtin(
                Prim::Uuid,
                &Const::Str("0192f0c4-3b5e-7c1a-9d2e-4f6a8b0c1d2e".into())
            )
            .is_ok()
        );
        assert!(check_builtin(Prim::IpAddress, &Const::Str("10.0.0.1".into())).is_ok());
        let mut rules = Rules::default();
        let path = |names: &[&str]| Const::Path(names.iter().map(|n| n.to_string()).collect());
        assert!(
            constraint(
                Subject::Prim(Prim::Text),
                "format",
                &path(&["TextFormat", "email"]),
                &mut rules
            )
            .is_ok()
        );
        assert_eq!(rules.format, Some(jadpo_std::TextFormat::Email));
        assert!(
            constraint(
                Subject::Prim(Prim::Uuid),
                "version",
                &path(&["UuidVersion", "v7"]),
                &mut rules
            )
            .is_ok()
        );
        assert_eq!(rules.version, Some(UuidVersion::V7));
    }
}
