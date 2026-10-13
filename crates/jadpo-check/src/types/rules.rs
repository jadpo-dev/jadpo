//! The rules a valid value follows (area 2): constraints written in a
//! type's block or a field's settings, what each fits, and checking a
//! literal against them while compiling.

use std::cmp::Ordering;

use jadpo_syntax::ast::{self, AstNode, Expr};

use super::ty::Prim;
use crate::tree::{self, dotted};

/// An exact number as written: `mantissa × 10^-scale`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Num {
    pub mantissa: i128,
    pub scale: u32,
}

impl Num {
    /// Reads `12`, `-3` or `0.50`; `None` past 38 digits.
    pub fn parse(text: &str) -> Option<Num> {
        let (negative, digits) = match text.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, text),
        };
        let digits = digits.replace('_', "");
        let (whole, places) = digits.split_once('.').unwrap_or((&digits, ""));
        let joined = format!("{whole}{places}");
        let mantissa: i128 = joined.parse().ok()?;
        Some(Num {
            mantissa: if negative { -mantissa } else { mantissa },
            scale: u32::try_from(places.len()).ok()?,
        })
    }

    pub fn compare(self, other: Num) -> Ordering {
        let scale = self.scale.max(other.scale);
        let lift = |n: Num| {
            10i128
                .checked_pow(scale - n.scale)
                .and_then(|factor| n.mantissa.checked_mul(factor))
        };
        match (lift(self), lift(other)) {
            (Some(a), Some(b)) => a.cmp(&b),
            _ => self.mantissa.signum().cmp(&other.mantissa.signum()),
        }
    }

    /// Whether it is a whole number that fits an `Int`, a 64-bit integer.
    pub fn fits_int(self) -> bool {
        self.scale == 0 && i64::try_from(self.mantissa).is_ok()
    }

    /// Decimal places, ignoring trailing zeros: `1.50` has one.
    pub fn significant_places(self) -> u32 {
        let mut places = self.scale;
        let mut mantissa = self.mantissa;
        while places > 0 && mantissa % 10 == 0 {
            mantissa /= 10;
            places -= 1;
        }
        places
    }
}

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
    /// A string, with its escapes decoded.
    Str(String),
    Bool(bool),
    None,
    /// A name or a dotted name: `email`, `TextFormat.email`.
    Path(Vec<String>),
    List(Vec<Const>),
    /// Anything else: computed, not a constant.
    Other,
}

impl Const {
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
                Num::parse(&format!("{sign}{text}")).map_or(Const::Other, |value| Const::Number {
                    value,
                    decimal: false,
                })
            }
            jadpo_syntax::SyntaxKind::DECIMAL_NUMBER => Num::parse(&format!("{sign}{text}"))
                .map_or(Const::Other, |value| Const::Number {
                    value,
                    decimal: true,
                }),
            jadpo_syntax::SyntaxKind::UNIT_NUMBER if !negative => Const::Duration(text.to_string()),
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
            Const::Str(_) => "text",
            Const::Bool(_) => "a `Bool`",
            Const::None => "`none`",
            Const::Path(_) => "a name",
            Const::List(_) => "a list",
            Const::Other => "a computed value",
        }
    }
}

/// A string literal's text without quotes, escapes decoded (area 2).
fn unescape(text: &str) -> String {
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

/// The rules one declaration adds: a type's block, or a field's settings.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Rules {
    pub min: Option<Num>,
    pub max: Option<Num>,
    pub min_length: Option<u64>,
    pub max_length: Option<u64>,
    pub pattern: Option<String>,
    pub email: bool,
    pub scale: Option<u32>,
    pub version: Option<u8>,
}

/// What a constraint applies to: a value's representation, or a
/// collection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Subject {
    Prim(Prim),
    Collection,
    /// An object, enum or a type a later area defines: no constraints.
    Other,
}

/// The constraints area 2 defines, in the order the reader learns them.
pub const CONSTRAINTS: [&str; 8] = [
    "min",
    "max",
    "min_length",
    "max_length",
    "pattern",
    "format",
    "scale",
    "version",
];

/// The settings a field of an `Input` may have beside its constraints.
pub const FIELD_SETTINGS: [&str; 2] = ["optional", "default"];

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
                // Duration bounds are checked when area 10 is built.
                (Subject::Prim(Prim::Duration), Const::Duration(_)) => None,
                (Subject::Prim(Prim::Duration), _) => {
                    return Err(Problem::Value("a duration, such as `30s`".into(), None));
                }
                _ => return Err(Problem::Base("`Int`, `Decimal` and `Duration` types")),
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
                || subject == Subject::Collection)
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
            match value {
                Const::Path(path)
                    if path.len() == 2 && path[0] == "TextFormat" && path[1] == "email" =>
                {
                    rules.email = true;
                }
                Const::Path(path) if path.len() == 1 && path[0] == "email" => {
                    return Err(Problem::Value(
                        "a `TextFormat` value, such as `TextFormat.email`".into(),
                        Some("TextFormat.email".into()),
                    ));
                }
                _ => {
                    return Err(Problem::Value(
                        "a `TextFormat` value; `TextFormat.email` is the only one".into(),
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
            let version = match value {
                Const::Path(path) if path.len() == 2 && path[0] == "UuidVersion" => path[1]
                    .strip_prefix('v')
                    .and_then(|n| n.parse::<u8>().ok())
                    .filter(|n| (1..=8).contains(n)),
                _ => None,
            };
            match (version, value) {
                (Some(version), _) => rules.version = Some(version),
                (None, Const::Path(path)) if path.len() == 1 && path[0].starts_with('v') => {
                    return Err(Problem::Value(
                        "a `UuidVersion` value, such as `UuidVersion.v4`".into(),
                        Some(format!("UuidVersion.{}", path[0])),
                    ));
                }
                _ => {
                    return Err(Problem::Value(
                        "a `UuidVersion` value, such as `UuidVersion.v4`".into(),
                        None,
                    ));
                }
            }
        }
        _ => return Err(Problem::Unknown),
    }
    Ok(())
}

/// Compiles a pattern the way every target runs it: it must match the
/// whole text, and backreferences and lookaround are refused so it runs
/// in linear time (area 2, question 9).
pub fn compile_pattern(pattern: &str) -> Result<regex::Regex, String> {
    let bytes = pattern.as_bytes();
    let mut escaped = false;
    for (i, &b) in bytes.iter().enumerate() {
        if escaped {
            escaped = false;
            if b.is_ascii_digit() && b != b'0' || b == b'k' {
                return Err("backreferences are not allowed".into());
            }
            continue;
        }
        if b == b'\\' {
            escaped = true;
        } else if b == b'(' && bytes.get(i + 1) == Some(&b'?') {
            let rest = &bytes[i + 2..];
            if rest.starts_with(b"=")
                || rest.starts_with(b"!")
                || rest.starts_with(b"<=")
                || rest.starts_with(b"<!")
            {
                return Err("lookahead and lookbehind are not allowed".into());
            }
        }
    }
    regex::Regex::new(&format!("^(?:{pattern})$")).map_err(|error| {
        let detail = error.to_string();
        let last = detail
            .lines()
            .rev()
            .find(|line| line.starts_with("error:"))
            .map(|line| line.trim_start_matches("error:").trim().to_string());
        last.unwrap_or_else(|| "it isn't a valid regular expression".into())
    })
}

/// Why a literal breaks a rule.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Broken {
    /// The rule as written, such as `min: 1`.
    pub rule: String,
    /// What is wrong with the value, completing "The value ...".
    pub why: String,
}

/// Checks a constant against one declaration's rules.
pub fn check_rules(rules: &Rules, value: &Const) -> Result<(), Broken> {
    let broken = |rule: String, why: String| Err(Broken { rule, why });
    if let Const::Number { value, .. } = value {
        if let Some(min) = rules.min
            && value.compare(min) == Ordering::Less
        {
            return broken(
                format!("min: {}", show(min)),
                format!("is below {}", show(min)),
            );
        }
        if let Some(max) = rules.max
            && value.compare(max) == Ordering::Greater
        {
            return broken(
                format!("max: {}", show(max)),
                format!("is above {}", show(max)),
            );
        }
        if let Some(scale) = rules.scale
            && value.significant_places() > scale
        {
            return broken(
                format!("scale: {scale}"),
                format!("has {} decimal places", value.significant_places()),
            );
        }
    }
    let length = match value {
        Const::Str(text) => Some(text.chars().count() as u64),
        Const::List(items) => Some(items.len() as u64),
        _ => None,
    };
    if let Some(length) = length {
        let unit = if matches!(value, Const::List(_)) {
            "items"
        } else {
            "characters"
        };
        if let Some(min) = rules.min_length
            && length < min
        {
            return broken(format!("min_length: {min}"), format!("has {length} {unit}"));
        }
        if let Some(max) = rules.max_length
            && length > max
        {
            return broken(format!("max_length: {max}"), format!("has {length} {unit}"));
        }
    }
    if let Const::Str(text) = value {
        if let Some(pattern) = &rules.pattern
            && let Ok(compiled) = compile_pattern(pattern)
            && !compiled.is_match(text)
        {
            return broken(
                format!("pattern: \"{pattern}\""),
                "doesn't match the whole pattern".into(),
            );
        }
        if rules.email && !is_email(text) {
            return broken(
                "format: TextFormat.email".into(),
                "isn't an email address".into(),
            );
        }
        if let Some(version) = rules.version
            && uuid_version(text) != Some(version)
        {
            return broken(
                format!("version: UuidVersion.v{version}"),
                format!("isn't a version {version} UUID"),
            );
        }
    }
    Ok(())
}

/// A number as written, with its decimal places.
pub fn show(n: Num) -> String {
    if n.scale == 0 {
        return n.mantissa.to_string();
    }
    let digits = n.mantissa.unsigned_abs().to_string();
    let places = n.scale as usize;
    let padded = format!("{digits:0>width$}", width = places + 1);
    let (whole, frac) = padded.split_at(padded.len() - places);
    let sign = if n.mantissa < 0 { "-" } else { "" };
    format!("{sign}{whole}.{frac}")
}

/// What a built-in type itself requires of a literal, beyond its kind.
pub fn check_builtin(prim: Prim, value: &Const) -> Result<(), String> {
    match (prim, value) {
        (Prim::Int, Const::Number { value, .. }) if !value.fits_int() => {
            Err("is outside `Int`'s range, a 64-bit integer".into())
        }
        (Prim::Uuid, Const::Str(text)) if uuid_version(text).is_none() => {
            Err("isn't a UUID, written as 32 hexadecimal digits in groups of 8-4-4-4-12".into())
        }
        (Prim::Email, Const::Str(text)) if !is_email(text) => Err("isn't an email address".into()),
        (Prim::Url, Const::Str(text)) if !is_url(text) => {
            Err("isn't a URL with a scheme and a host, such as `https://example.com`".into())
        }
        (Prim::IpAddress, Const::Str(text)) if text.parse::<std::net::IpAddr>().is_err() => {
            Err("isn't an IPv4 or IPv6 address".into())
        }
        _ => Ok(()),
    }
}

/// The version digit of a UUID in its textual form.
fn uuid_version(text: &str) -> Option<u8> {
    let groups: Vec<&str> = text.split('-').collect();
    let sizes = [8, 4, 4, 4, 12];
    if groups.len() != 5
        || groups.iter().zip(sizes).any(|(group, size)| {
            group.len() != size || !group.chars().all(|c| c.is_ascii_hexdigit())
        })
    {
        return None;
    }
    groups[2]
        .chars()
        .next()
        .and_then(|c| c.to_digit(16))
        .and_then(|d| u8::try_from(d).ok())
}

/// An address with one `@`, something before it, and a domain with a dot
/// after it, and no spaces.
fn is_email(text: &str) -> bool {
    let Some((local, domain)) = text.split_once('@') else {
        return false;
    };
    !local.is_empty()
        && !domain.contains('@')
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && !text.chars().any(char::is_whitespace)
}

fn is_url(text: &str) -> bool {
    let Some((scheme, rest)) = text.split_once("://") else {
        return false;
    };
    !scheme.is_empty()
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
        && rest
            .split(['/', '?', '#'])
            .next()
            .is_some_and(|host| !host.is_empty())
        && !text.chars().any(char::is_whitespace)
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
    fn numbers_compare_exactly() {
        let a = Num::parse("1.50").expect("a");
        let b = Num::parse("1.5").expect("b");
        assert_eq!(a.compare(b), Ordering::Equal);
        assert_eq!(Num::parse("-3").expect("c").compare(a), Ordering::Less);
        assert_eq!(a.significant_places(), 1);
        assert_eq!(show(Num::parse("0.05").expect("d")), "0.05");
    }

    #[test]
    fn constraints_fit_their_base() {
        let mut rules = Rules::default();
        assert_eq!(
            constraint(Subject::Prim(Prim::Text), "min", &number("1"), &mut rules),
            Err(Problem::Base("`Int`, `Decimal` and `Duration` types"))
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
    fn patterns_match_the_whole_text_in_linear_time() {
        assert!(compile_pattern("(a+)+\\1").is_err());
        assert!(compile_pattern("(?=a)a").is_err());
        assert!(compile_pattern("[a-z").is_err());
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
    }
}
