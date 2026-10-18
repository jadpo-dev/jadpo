//! The rules a valid value follows (area 2) and the validators that check
//! them: bounds and scale on numbers, lengths counted in characters or
//! items, whole-text patterns, formats and UUID versions, and what the
//! validated built-in types require of their text. The checker runs these
//! on literals while compiling; the runtime runs the same functions on
//! values.

use std::cmp::Ordering;

use crate::{Builtin, Dur, Num, Rounding, TextFormat, UuidVersion};

/// The rules one declaration adds: a type's block, or a field's settings.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Rules {
    pub min: Option<Num>,
    pub max: Option<Num>,
    pub min_length: Option<u64>,
    pub max_length: Option<u64>,
    /// A regular expression the whole text must match.
    pub pattern: Option<String>,
    pub format: Option<TextFormat>,
    /// The most decimal places a number may have.
    pub scale: Option<u32>,
    pub version: Option<UuidVersion>,
    /// `min:` and `max:` on a `Duration` type.
    pub min_duration: Option<Dur>,
    pub max_duration: Option<Dur>,
    /// `currency:` on a `Money` type (area 19, "Money and payments"): the
    /// one currency its amounts are in, by its place among `Currency`'s
    /// variants. `min:` and `max:` on a `Money` type count minor units.
    pub currency: Option<u32>,
    /// `rounding:` on a `Money` type: how `*` and `/` round an amount to a
    /// whole minor unit, `half_even` when not declared.
    pub rounding: Option<Rounding>,
    /// The standard type held as text whose own check the text passes
    /// (area 19, "Groups with the baseline only"): its pattern and its
    /// named validator, an IBAN's mod-97, broken as `format`, as an
    /// `Email`'s check is. Only the standard library sets it; no block
    /// can write it.
    pub standard: Option<&'static str>,
}

/// Which rule a value breaks, by the key an `attempt`'s arm names it with
/// (area 2, "Construction failures by rule"): `max` for `max: 1000`.
/// `min:` and `max:` on a duration are `min` and `max` too, and a
/// validated built-in type's own check (`Email`'s) is `format`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RuleKey {
    Min,
    Max,
    MinLength,
    MaxLength,
    Pattern,
    Format,
    Scale,
    Version,
    /// An amount of money in another currency than a `Money` type's
    /// `currency:` (area 19, "Money and payments").
    Currency,
    /// A local time a temporal operation makes falls in a daylight-saving
    /// gap or overlap, and its `dst:` didn't say which instant to take
    /// (area 10, "Zones and local times").
    Dst,
    /// A date a temporal operation makes has no such day in its month, and
    /// its `invalid_day:` didn't say what to do instead.
    InvalidDay,
    /// Text `text.parse` reads isn't the type it was asked for at all
    /// (area 19, "Every standard type meets one contract"), or text or
    /// bytes an `encoding` or `json` operation decodes aren't that format
    /// at all (area 19, "Encodings" and "Data formats").
    Malformed,
    /// A value a `decimal` operation has no result for, such as `ln` of
    /// a value at or below 0 (area 19, "Numbers").
    OutOfRange,
    /// Two amounts of money in different currencies, added, subtracted or
    /// ordered, where only the running program knows their currencies
    /// (area 19, "The standard operator table").
    CurrencyMismatch,
    /// A decimal amount with more places than its currency has, made into
    /// `Money` by `money.of` (area 19, "Money and payments").
    TooManyPlaces,
    /// A JSON document `json.read` decodes isn't the type `as:` names: a
    /// field missing, unknown or breaking a rule (area 19, "Data formats").
    Mismatch,
    /// Text `text.parse` reads as a physical quantity has a number and a
    /// unit, but the unit isn't one of the quantity's kind (area 19,
    /// "Physical quantities"): `5 kg` read as a `Length`.
    UnknownUnit,
}

impl RuleKey {
    /// Every key: a type's rules in the order its block lists them, then
    /// the conditions a temporal operation can fail on, each named by the
    /// option that would settle it.
    pub const ALL: [RuleKey; 17] = [
        RuleKey::Min,
        RuleKey::Max,
        RuleKey::MinLength,
        RuleKey::MaxLength,
        RuleKey::Pattern,
        RuleKey::Format,
        RuleKey::Scale,
        RuleKey::Version,
        RuleKey::Currency,
        RuleKey::Dst,
        RuleKey::InvalidDay,
        RuleKey::Malformed,
        RuleKey::OutOfRange,
        RuleKey::CurrencyMismatch,
        RuleKey::TooManyPlaces,
        RuleKey::Mismatch,
        RuleKey::UnknownUnit,
    ];

    /// The key as written: `max`, `min_length`.
    pub fn word(self) -> &'static str {
        match self {
            RuleKey::Min => "min",
            RuleKey::Max => "max",
            RuleKey::MinLength => "min_length",
            RuleKey::MaxLength => "max_length",
            RuleKey::Pattern => "pattern",
            RuleKey::Format => "format",
            RuleKey::Scale => "scale",
            RuleKey::Version => "version",
            RuleKey::Currency => "currency",
            RuleKey::Dst => "dst",
            RuleKey::InvalidDay => "invalid_day",
            RuleKey::Malformed => "malformed",
            RuleKey::OutOfRange => "out_of_range",
            RuleKey::CurrencyMismatch => "currency_mismatch",
            RuleKey::TooManyPlaces => "too_many_places",
            RuleKey::Mismatch => "mismatch",
            RuleKey::UnknownUnit => "unknown_unit",
        }
    }

    pub fn from_word(word: &str) -> Option<RuleKey> {
        RuleKey::ALL.into_iter().find(|key| key.word() == word)
    }
}

/// Why a value breaks a rule.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Broken {
    /// The rule's key, so nothing reads it back out of the text.
    pub key: RuleKey,
    /// The rule as written, such as `min: 1`.
    pub rule: String,
    /// What is wrong with the value, completing "The value ...".
    pub why: String,
}

fn broken(key: RuleKey, rule: String, why: String) -> Result<(), Broken> {
    Err(Broken { key, rule, why })
}

impl Rules {
    pub fn is_empty(&self) -> bool {
        *self == Rules::default()
    }

    /// A number against `min`, `max` and `scale`.
    pub fn check_number(&self, value: Num) -> Result<(), Broken> {
        if let Some(min) = self.min
            && value.compare(min) == Ordering::Less
        {
            return broken(
                RuleKey::Min,
                format!("min: {min}"),
                format!("is below {min}"),
            );
        }
        if let Some(max) = self.max
            && value.compare(max) == Ordering::Greater
        {
            return broken(
                RuleKey::Max,
                format!("max: {max}"),
                format!("is above {max}"),
            );
        }
        if let Some(scale) = self.scale
            && value.significant_places() > scale
        {
            return broken(
                RuleKey::Scale,
                format!("scale: {scale}"),
                format!("has {} decimal places", value.significant_places()),
            );
        }
        Ok(())
    }

    /// A duration against `min` and `max`.
    pub fn check_duration(&self, value: Dur) -> Result<(), Broken> {
        if let Some(min) = self.min_duration
            && value < min
        {
            return broken(
                RuleKey::Min,
                format!("min: {min}"),
                format!("is shorter than {min}"),
            );
        }
        if let Some(max) = self.max_duration
            && value > max
        {
            return broken(
                RuleKey::Max,
                format!("max: {max}"),
                format!("is longer than {max}"),
            );
        }
        Ok(())
    }

    /// An amount of money against `currency`, then `min` and `max`, which
    /// count its minor units; `currency` is its currency's place among
    /// `Currency`'s variants.
    pub fn check_money(&self, minor: i64, currency: u32) -> Result<(), Broken> {
        if let Some(fixed) = self.currency
            && fixed != currency
        {
            let code = |ordinal: u32| crate::money::code(ordinal).unwrap_or("?");
            let variant = crate::money::variant(fixed).unwrap_or("?");
            return broken(
                RuleKey::Currency,
                format!("currency: {}.{variant}", crate::CURRENCY),
                format!("is in {}, not {}", code(currency), code(fixed)),
            );
        }
        self.check_number(Num::whole(i128::from(minor)))
    }

    /// A physical quantity of `kind` against `min` and `max`, both held in
    /// its base unit at its scale as the value is (area 19), and written
    /// as code writes a quantity, `min: 1metre`.
    pub fn check_quantity(&self, kind: crate::QuantityKind, amount: i128) -> Result<(), Broken> {
        let value = Num {
            mantissa: amount,
            scale: kind.scale(),
        };
        let shown = |bound: Num| {
            bound.at_scale(kind.scale()).map_or_else(
                || bound.to_string(),
                |at| crate::quantity::written(kind, at.mantissa),
            )
        };
        if let Some(min) = self.min
            && value.compare(min) == Ordering::Less
        {
            let min = shown(min);
            return broken(
                RuleKey::Min,
                format!("min: {min}"),
                format!("is below {min}"),
            );
        }
        if let Some(max) = self.max
            && value.compare(max) == Ordering::Greater
        {
            let max = shown(max);
            return broken(
                RuleKey::Max,
                format!("max: {max}"),
                format!("is above {max}"),
            );
        }
        Ok(())
    }

    /// A percentage, the fraction it is, against `min` and `max`, held
    /// as fractions too and written as percentages: `max: 100%` (area 19).
    pub fn check_percentage(&self, fraction: Num) -> Result<(), Broken> {
        let shown = |bound: Num| crate::percentage::canonical(bound.mantissa, bound.scale);
        if let Some(min) = self.min
            && fraction.compare(min) == Ordering::Less
        {
            let min = shown(min);
            return broken(
                RuleKey::Min,
                format!("min: {min}"),
                format!("is below {min}"),
            );
        }
        if let Some(max) = self.max
            && fraction.compare(max) == Ordering::Greater
        {
            let max = shown(max);
            return broken(
                RuleKey::Max,
                format!("max: {max}"),
                format!("is above {max}"),
            );
        }
        Ok(())
    }

    /// A length against `min_length` and `max_length`; `unit` names what
    /// was counted, `characters` or `items`.
    pub fn check_length(&self, length: u64, unit: &str) -> Result<(), Broken> {
        if let Some(min) = self.min_length
            && length < min
        {
            return broken(
                RuleKey::MinLength,
                format!("min_length: {min}"),
                format!("has {length} {unit}"),
            );
        }
        if let Some(max) = self.max_length
            && length > max
        {
            return broken(
                RuleKey::MaxLength,
                format!("max_length: {max}"),
                format!("has {length} {unit}"),
            );
        }
        Ok(())
    }

    /// Text against its length in characters, `pattern`, `format` and
    /// `version`.
    pub fn check_text(&self, text: &str) -> Result<(), Broken> {
        let compiled = self
            .pattern
            .as_deref()
            .and_then(|pattern| compile_pattern(pattern).ok());
        self.check_text_with(text, compiled.as_ref())
    }

    /// [`Rules::check_text`] with `pattern` already compiled by
    /// [`compile_pattern`], as the runtime keeps it, so a value is never
    /// checked by compiling its pattern again.
    pub fn check_text_with(&self, text: &str, compiled: Option<&Pattern>) -> Result<(), Broken> {
        self.check_length(text.chars().count() as u64, "characters")?;
        if let Some(pattern) = &self.pattern
            && let Some(compiled) = compiled
            && !compiled.is_match(text)
        {
            return broken(
                RuleKey::Pattern,
                format!("pattern: \"{pattern}\""),
                "doesn't match the whole pattern".into(),
            );
        }
        if let Some(format) = self.format {
            let fits = match format {
                TextFormat::Email => is_email(text),
            };
            if !fits {
                return broken(
                    RuleKey::Format,
                    format!("format: {}.{}", TextFormat::NAME, format.name()),
                    "isn't an email address".into(),
                );
            }
        }
        if let Some(version) = self.version
            && uuid_version(text) != Some(version.digit())
        {
            return broken(
                RuleKey::Version,
                format!("version: {}.{}", UuidVersion::NAME, version.name()),
                format!("isn't a version {} UUID", version.digit()),
            );
        }
        if let Some(name) = self.standard
            && !crate::standard::standard_type(name).is_some_and(|entry| entry.accepts(text))
        {
            return broken(
                RuleKey::Format,
                name.to_string(),
                format!("isn't a valid `{name}`"),
            );
        }
        Ok(())
    }
}

/// A `pattern:` rule compiled to run: it matches the whole text, in linear
/// time.
pub type Pattern = regex::Regex;

/// Compiles a pattern the way every target runs it: it must match the
/// whole text, and backreferences and lookaround are refused so it runs in
/// linear time (area 2, question 9).
pub fn compile_pattern(pattern: &str) -> Result<Pattern, String> {
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
            let rest = bytes.get(i + 2..).unwrap_or_default();
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

/// What a validated built-in type requires of its text, beyond being text:
/// the reason it refuses `text`, completing "The value ...".
pub fn check_builtin_text(builtin: Builtin, text: &str) -> Result<(), String> {
    match builtin {
        Builtin::Uuid if uuid_version(text).is_none() => {
            Err("isn't a UUID, written as 32 hexadecimal digits in groups of 8-4-4-4-12".into())
        }
        Builtin::Email if !is_email(text) => Err("isn't an email address".into()),
        Builtin::Url if !is_url(text) => Err(
            "isn't an `http://` or `https://` URL with a host, such as `https://example.com`"
                .into(),
        ),
        Builtin::PostgresUrl if !is_connection_url(text, &["postgres", "postgresql"]) => {
            Err("isn't a PostgreSQL address, which starts `postgres://` or `postgresql://`".into())
        }
        Builtin::RedisUrl if !is_connection_url(text, &["redis", "rediss"]) => {
            Err("isn't a Redis address, which starts `redis://` or `rediss://`".into())
        }
        Builtin::SigningKey if !is_signing_key(text) => Err(format!(
            "isn't a signing key, which is at least {} characters with no spaces",
            crate::builtin::SIGNING_KEY_MIN
        )),
        Builtin::IpAddress if !is_ip_address(text) => Err("isn't an IPv4 or IPv6 address".into()),
        Builtin::Instant => crate::time::check_instant(text),
        Builtin::CalendarDate => crate::time::check_calendar_date(text),
        _ => Ok(()),
    }
}

/// The version digit of a UUID in its textual form; `None` when it isn't
/// one.
pub fn uuid_version(text: &str) -> Option<u8> {
    let groups: Vec<&str> = text.split('-').collect();
    let sizes = [8, 4, 4, 4, 12];
    if groups.len() != 5
        || groups.iter().zip(sizes).any(|(group, size)| {
            group.len() != size || !group.chars().all(|c| c.is_ascii_hexdigit())
        })
    {
        return None;
    }
    groups
        .get(2)?
        .chars()
        .next()
        .and_then(|c| c.to_digit(16))
        .and_then(|d| u8::try_from(d).ok())
}

/// An address with one `@`, something before it, and a domain with a dot
/// after it, and no spaces.
pub fn is_email(text: &str) -> bool {
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

/// An `http://` or `https://` URL with a host, and no spaces (area 10:
/// `Url` holds a web address, so a database address has types of its own).
pub fn is_url(text: &str) -> bool {
    is_connection_url(text, &["http", "https"])
}

/// A URL whose scheme is one of `schemes` (compared in lower case), with
/// something after `://` and no spaces. A database address may have no
/// host (`postgres:///app` is a local socket), so only its rest is
/// required to be there; a web address's host must be.
fn is_connection_url(text: &str, schemes: &[&str]) -> bool {
    let Some((scheme, rest)) = text.split_once("://") else {
        return false;
    };
    let web = schemes.contains(&"http");
    schemes.contains(&scheme.to_ascii_lowercase().as_str())
        && !rest.is_empty()
        && (!web
            || rest
                .split(['/', '?', '#'])
                .next()
                .is_some_and(|host| !host.is_empty()))
        && !text.chars().any(char::is_whitespace)
}

/// At least [`crate::builtin::SIGNING_KEY_MIN`] visible characters.
pub fn is_signing_key(text: &str) -> bool {
    text.chars().count() >= crate::builtin::SIGNING_KEY_MIN
        && text.chars().all(|c| c.is_ascii_graphic())
}

pub fn is_ip_address(text: &str) -> bool {
    text.parse::<std::net::IpAddr>().is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn num(text: &str) -> Num {
        Num::parse(text).expect("a number")
    }

    #[test]
    fn numbers_break_the_rule_they_break() {
        let rules = Rules {
            min: Some(num("1")),
            max: Some(num("1000")),
            ..Rules::default()
        };
        let broken = rules.check_number(num("0")).expect_err("below min");
        assert_eq!(broken.rule, "min: 1");
        assert_eq!(broken.key, RuleKey::Min);
        let scaled = Rules {
            scale: Some(2),
            ..Rules::default()
        };
        assert!(scaled.check_number(num("1.250")).is_ok());
        assert!(scaled.check_number(num("1.255")).is_err());
    }

    #[test]
    fn text_is_counted_in_characters_and_matched_whole() {
        let rules = Rules {
            max_length: Some(3),
            ..Rules::default()
        };
        assert!(rules.check_text("héé").is_ok(), "three characters");
        assert!(rules.check_text("abcd").is_err());
        let rules = Rules {
            pattern: Some("[A-Z]{3}-[0-9]{4}".into()),
            ..Rules::default()
        };
        assert!(rules.check_text("ABC-1234").is_ok());
        assert!(rules.check_text("ABC-1234x").is_err());
        let rules = Rules {
            version: Some(UuidVersion::V7),
            ..Rules::default()
        };
        assert!(
            rules
                .check_text("0192f0c4-3b5e-7c1a-9d2e-4f6a8b0c1d2e")
                .is_ok()
        );
        assert_eq!(
            rules
                .check_text("0192f0c4-3b5e-4c1a-9d2e-4f6a8b0c1d2e")
                .expect_err("v4")
                .rule,
            "version: UuidVersion.v7"
        );
    }

    #[test]
    fn rule_keys_read_back_by_word() {
        for key in RuleKey::ALL {
            assert_eq!(RuleKey::from_word(key.word()), Some(key));
        }
        assert_eq!(RuleKey::from_word("invalid"), None);
        let rules = Rules {
            min_length: Some(2),
            ..Rules::default()
        };
        assert_eq!(
            rules.check_text("a").expect_err("short").key,
            RuleKey::MinLength
        );
    }

    #[test]
    fn patterns_run_in_linear_time() {
        assert!(compile_pattern("(a+)+\\1").is_err());
        assert!(compile_pattern("(?=a)a").is_err());
        assert!(compile_pattern("[a-z").is_err());
    }

    #[test]
    fn validated_builtins_check_their_text() {
        assert!(check_builtin_text(Builtin::Uuid, "not a uuid").is_err());
        assert!(check_builtin_text(Builtin::Uuid, "0192f0c4-3b5e-7c1a-9d2e-4f6a8b0c1d2e").is_ok());
        assert!(check_builtin_text(Builtin::IpAddress, "10.0.0.1").is_ok());
        assert!(check_builtin_text(Builtin::Email, "a@b").is_err());
        assert!(check_builtin_text(Builtin::Url, "https://example.com").is_ok());
        assert!(check_builtin_text(Builtin::Text, "anything").is_ok());
    }
}
