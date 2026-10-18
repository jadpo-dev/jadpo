//! Checking a value against its type's rules at runtime (area 2): the same
//! validators `jadpo-check` runs on literals while compiling, from
//! `jadpo-std`, so a value refused at runtime is one a literal would have
//! been refused for.
//!
//! A [`Validator`] holds one type's rules along its chain with their
//! patterns compiled once, and stores a decimal at the type's fixed scale:
//! a computed `Decimal` is rounded half to even into it (area 3, question
//! 7), so `scale:` decides how a value is kept rather than refusing it.

use jadpo_std::{
    Broken, Builtin, Num, Pattern, Rounding, RuleKey, Rules, check_builtin_text, compile_pattern,
};

use crate::{Decimal, Value};

/// Why a value isn't valid for its type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Invalid {
    /// The built-in type itself refuses it, such as text that isn't an
    /// email address for a type built on `Email`: which type, and why.
    Builtin { builtin: Builtin, why: String },
    /// A rule is broken.
    Broken(Broken),
}

impl Invalid {
    /// The rule as written, `max: 1000`, or the built-in type whose own
    /// check refused it, `Email`.
    pub fn rule(&self) -> String {
        match self {
            Invalid::Builtin { builtin, .. } => builtin.name().to_string(),
            Invalid::Broken(broken) => broken.rule.clone(),
        }
    }

    /// The key an `attempt`'s arm names it by: `max` for `max: 1000`, and
    /// `format` for a built-in type's own check (area 2, "Construction
    /// failures by rule").
    pub fn key(&self) -> RuleKey {
        match self {
            Invalid::Builtin { .. } => RuleKey::Format,
            Invalid::Broken(broken) => broken.key,
        }
    }

    /// The rule as a failure's report names it as broken (area 2,
    /// "Construction failures by rule"): `` `max: 1000` ``, or for a field
    /// of an object built with its fields, `` `max: 1000` on `quantity` ``.
    pub fn broken(&self, field: Option<&str>) -> String {
        match field {
            Some(field) => format!("`{}` on `{field}`", self.rule()),
            None => format!("`{}`", self.rule()),
        }
    }

    /// For a rule an object type names across its fields, its name (area
    /// 2, "Rules across fields").
    pub fn name(&self) -> Option<&str> {
        match self {
            Invalid::Broken(broken) if broken.key == RuleKey::Named => Some(&broken.rule),
            _ => None,
        }
    }

    /// What is wrong with the value, completing "The value ...".
    pub fn why(&self) -> &str {
        match self {
            Invalid::Builtin { why, .. } => why,
            Invalid::Broken(broken) => &broken.why,
        }
    }
}

/// One type's rules, most specific first, ready to run.
#[derive(Clone, Debug)]
pub struct Validator {
    builtin: Option<Builtin>,
    rules: Vec<(Rules, Option<Pattern>)>,
    scale: Option<u32>,
    /// An object type's rules across its fields, in the order written
    /// (area 2, "Rules across fields").
    across: Vec<jadpo_std::across::CrossRule>,
}

impl Validator {
    /// The rules of a type built on `builtin`, each declaration's along its
    /// chain, most specific first. A pattern the checker accepted always
    /// compiles.
    pub fn new(builtin: Option<Builtin>, chain: Vec<Rules>) -> Validator {
        let scale = chain.iter().find_map(|rules| rules.scale);
        let rules = chain
            .into_iter()
            .map(|rules| {
                let pattern = rules
                    .pattern
                    .as_deref()
                    .and_then(|pattern| compile_pattern(pattern).ok());
                (rules, pattern)
            })
            .collect();
        Validator {
            builtin,
            rules,
            scale,
            across: Vec::new(),
        }
    }

    /// The same validator, checking an object's rules across its fields
    /// too, once its fields are checked.
    pub fn with_across(mut self, across: Vec<jadpo_std::across::CrossRule>) -> Validator {
        self.across = across;
        self
    }

    /// Whether it checks rules across an object's fields.
    pub fn has_across(&self) -> bool {
        !self.across.is_empty()
    }

    /// The fixed scale its values are kept at, from `scale:` on its chain.
    pub fn scale(&self) -> Option<u32> {
        self.scale
    }

    /// Whether it has anything to check beyond the value's kind.
    pub fn is_empty(&self) -> bool {
        self.across.is_empty()
            && self.rules.iter().all(|(rules, _)| rules.is_empty())
            && !self.builtin.is_some_and(|builtin| {
                matches!(
                    builtin,
                    Builtin::Uuid
                        | Builtin::Email
                        | Builtin::Url
                        | Builtin::IpAddress
                        | Builtin::PostgresUrl
                        | Builtin::RedisUrl
                        | Builtin::SigningKey
                        | Builtin::EncryptionKey
                        | Builtin::PrivateKey
                )
            })
    }

    /// The value as the type keeps it: a decimal at the type's fixed scale,
    /// rounded half to even, and `Html` made safe; `None` past 38 digits.
    pub fn store(&self, value: Value) -> Option<Value> {
        // A ratio becomes a percentage by construction, `Percentage(0.25)`
        // being 25% (area 19).
        if self.builtin == Some(Builtin::Percentage) {
            return Some(match value {
                Value::Decimal(ratio) => Value::Percentage(ratio),
                value => value,
            });
        }
        // `Html` is made safe as it is built, whatever text it is built
        // from (area 19, "Text formats"): its own check is then what the
        // sanitiser keeps, which it always passes.
        if let Value::Text(text) = &value
            && self.rules.iter().any(|(rules, _)| {
                rules
                    .standard
                    .is_some_and(jadpo_std::standard::markup::sanitises)
            })
        {
            return Some(Value::text(&jadpo_std::standard::markup::sanitise(text)));
        }
        // A secret value is held so that it is never written out (area 19,
        // chunk 5.5r2), whatever text it is built from: a literal, a
        // request's JSON, `json.read`.
        if let Value::Text(text) = &value
            && let Some(kind) = self.secret()
        {
            return Some(Value::secret(kind, text));
        }
        // A `Vector`'s items are kept as the nearest single-precision
        // floats, whatever decimals it is built from (chunk 5.5s2).
        if let Value::List(items) = &value
            && self
                .rules
                .iter()
                .any(|(rules, _)| rules.standard.is_some_and(jadpo_std::standard::ai::rounds))
        {
            let mut kept = Vec::with_capacity(items.len());
            for item in items.iter() {
                let Value::Decimal(decimal) = item else {
                    return None;
                };
                let text = jadpo_std::standard::ai::float(&decimal.to_string())?;
                kept.push(Value::decimal(&text)?);
            }
            return Some(Value::list(kept));
        }
        match (value, self.scale) {
            (Value::Decimal(d), Some(scale)) if d.scale != scale => {
                d.rescale(scale, Rounding::HalfEven).map(Value::Decimal)
            }
            (Value::Int(n), Some(scale)) => Decimal::from_int(n)
                .rescale(scale, Rounding::HalfEven)
                .map(Value::Decimal),
            (value, _) => Some(value),
        }
    }

    /// The standard type its chain names, if any: one held as text with
    /// its own check, or held as a whole number with its range (chunk
    /// 5.5n), whose canonical text a place that holds text reads.
    pub fn standard(&self) -> Option<&'static str> {
        self.rules.iter().find_map(|(rules, _)| rules.standard)
    }

    /// The secret value standard type its chain is built on, if any: a
    /// `Password`, or a type built on one (chunk 5.5r2).
    pub fn secret(&self) -> Option<&'static str> {
        self.rules.iter().find_map(|(rules, _)| {
            rules
                .standard
                .and_then(jadpo_std::standard::security::secret_value)
        })
    }

    /// Checks `value` against the built-in type and every rule.
    pub fn check(&self, value: &Value) -> Result<(), Invalid> {
        if let (Some(builtin), Value::Text(text) | Value::Uuid(text)) = (self.builtin, value) {
            check_builtin_text(builtin, text).map_err(|why| Invalid::Builtin { builtin, why })?;
        }
        for (rules, pattern) in &self.rules {
            match value {
                Value::Int(n) => rules.check_number(Num::whole(i128::from(*n))),
                Value::Decimal(d) => rules.check_number(Num {
                    mantissa: d.mantissa,
                    scale: d.scale,
                }),
                Value::Text(text) | Value::Uuid(text) => {
                    rules.check_text_with(text, pattern.as_ref())
                }
                // A broken rule names the rule, never the text (chunk
                // 5.5r2).
                Value::Secret(secret) => rules.check_text_with(secret.expose(), pattern.as_ref()),
                // Its length, and the check of a standard type built on
                // `Bytes` (chunk 5.5r3).
                Value::Bytes(bytes) => rules.check_bytes(bytes),
                Value::List(items) | Value::Set(items) => {
                    rules.check_length(items.len() as u64, "items")
                }
                Value::Map(entries) => rules.check_length(entries.len() as u64, "items"),
                Value::Duration(millis) => rules.check_duration(jadpo_std::Dur {
                    millis: i128::from(*millis),
                }),
                Value::Money(money) => rules.check_money(money.minor, money.currency),
                // A quantity type's bounds are held in its base unit at its
                // kind's scale, as the value is (area 19).
                Value::Quantity(quantity) => rules.check_quantity(quantity.kind, quantity.amount),
                // A percentage type's bounds are held as fractions, as its
                // values are: `max: 100%` is 1.00 (area 19).
                Value::Percentage(fraction) => rules.check_percentage(Num {
                    mantissa: fraction.mantissa,
                    scale: fraction.scale,
                }),
                // A rate type's currencies (area 19, chunk 5.5h4).
                Value::Record(record) if rules.rate_from.is_some() || rules.rate_to.is_some() => {
                    match (record.get("from"), record.get("to")) {
                        (Some(Value::Variant(from)), Some(Value::Variant(to))) => {
                            rules.check_rate(from.ordinal, to.ordinal)
                        }
                        _ => Ok(()),
                    }
                }
                _ => Ok(()),
            }
            .map_err(Invalid::Broken)?;
        }
        // The rules across an object's fields, in the order written: the
        // first broken one is the one reported (area 2, "Rules across
        // fields").
        if let (false, Value::Record(record)) = (self.across.is_empty(), value) {
            let fields: Vec<Value> = record.fields.iter().map(|(_, v)| v.clone()).collect();
            if let Some(rule) = self
                .across
                .iter()
                .find(|rule| !crate::across::holds(rule, &fields))
            {
                return Err(Invalid::Broken(Broken {
                    key: RuleKey::Named,
                    rule: rule.name.clone(),
                    why: format!("doesn't follow its type's rule `{}`", rule.name),
                }));
            }
        }
        Ok(())
    }
}

/// Checks `value`, of a type built on `builtin`, against each declaration's
/// rules along its chain, most specific first.
pub fn check<'r>(
    value: &Value,
    builtin: Builtin,
    chain: impl IntoIterator<Item = &'r Rules>,
) -> Result<(), Invalid> {
    Validator::new(Some(builtin), chain.into_iter().cloned().collect()).check(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_follow_the_rules_literals_follow() {
        let quantity = Rules {
            min: Num::parse("1"),
            max: Num::parse("1000"),
            ..Rules::default()
        };
        assert!(check(&Value::Int(5), Builtin::Int, [&quantity]).is_ok());
        assert!(matches!(
            check(&Value::Int(0), Builtin::Int, [&quantity]),
            Err(Invalid::Broken(broken)) if broken.rule == "min: 1"
        ));
        let name = Rules {
            max_length: Some(3),
            ..Rules::default()
        };
        assert!(check(&Value::text("héé"), Builtin::Text, [&name]).is_ok());
        assert!(matches!(
            check(&Value::text("not an address"), Builtin::Email, []),
            Err(Invalid::Builtin {
                builtin: Builtin::Email,
                ..
            })
        ));
        let refused = check(&Value::text("junk"), Builtin::Url, []).expect_err("not a URL");
        assert_eq!(
            (refused.key(), refused.rule().as_str()),
            (RuleKey::Format, "Url")
        );
        let sku = Validator::new(
            Some(Builtin::Text),
            vec![Rules {
                pattern: Some("[A-Z]{3}-[0-9]{4}".into()),
                ..Rules::default()
            }],
        );
        assert!(sku.check(&Value::text("ABC-1234")).is_ok());
        assert_eq!(
            sku.check(&Value::text("ABC-1234x"))
                .map_err(|invalid| invalid.rule()),
            Err("pattern: \"[A-Z]{3}-[0-9]{4}\"".to_string())
        );
    }

    #[test]
    fn a_fixed_scale_rounds_half_to_even_where_it_is_stored() {
        let price = Validator::new(
            Some(Builtin::Decimal),
            vec![Rules {
                scale: Some(2),
                min: Num::parse("0.00"),
                ..Rules::default()
            }],
        );
        let stored = price.store(Value::decimal("1.255").expect("a decimal"));
        assert_eq!(stored, Value::decimal("1.26"));
        assert_eq!(
            stored.map(|value| value.to_string()).as_deref(),
            Some("1.26")
        );
        assert_eq!(
            price.store(Value::decimal("0.125").expect("a decimal")),
            Value::decimal("0.12")
        );
        assert_eq!(
            price
                .store(Value::decimal("1.5").expect("a decimal"))
                .map(|value| value.to_string())
                .as_deref(),
            Some("1.50")
        );
        assert!(
            price
                .check(&Value::decimal("-0.01").expect("a decimal"))
                .is_err()
        );
    }
}
