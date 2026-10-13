//! Checking a value against its type's rules at runtime (area 2): the same
//! validators `jadpo-check` runs on literals while compiling, from
//! `jadpo-std`, so a value refused at runtime is one a literal would have
//! been refused for.

use jadpo_std::{Broken, Builtin, Num, Rules, check_builtin_text};

use crate::Value;

/// Why a value isn't valid for its type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Invalid {
    /// The built-in type itself refuses it: why.
    Builtin(String),
    /// A rule is broken.
    Broken(Broken),
}

/// Checks `value`, of a type built on `builtin`, against each declaration's
/// rules along its chain, most specific first.
pub fn check<'r>(
    value: &Value,
    builtin: Builtin,
    chain: impl IntoIterator<Item = &'r Rules>,
) -> Result<(), Invalid> {
    if let Value::Text(text) | Value::Uuid(text) = value {
        check_builtin_text(builtin, text).map_err(Invalid::Builtin)?;
    }
    for rules in chain {
        match value {
            Value::Int(n) => rules.check_number(Num::whole(i128::from(*n))),
            Value::Decimal(d) => rules.check_number(Num {
                mantissa: d.mantissa,
                scale: d.scale,
            }),
            Value::Text(text) | Value::Uuid(text) => rules.check_text(text),
            Value::Bytes(bytes) => rules.check_length(bytes.len() as u64, "bytes"),
            Value::List(items) | Value::Set(items) => {
                rules.check_length(items.len() as u64, "items")
            }
            Value::Map(entries) => rules.check_length(entries.len() as u64, "items"),
            _ => Ok(()),
        }
        .map_err(Invalid::Broken)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Decimal;

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
        let price = Rules {
            scale: Some(2),
            ..Rules::default()
        };
        assert!(
            check(
                &Value::Decimal(Decimal::new(1255, 3)),
                Builtin::Decimal,
                [&price]
            )
            .is_err()
        );
        let name = Rules {
            max_length: Some(3),
            ..Rules::default()
        };
        assert!(check(&Value::Text("héé".into()), Builtin::Text, [&name]).is_ok());
        assert!(matches!(
            check(&Value::Text("not an address".into()), Builtin::Email, []),
            Err(Invalid::Builtin(_))
        ));
    }
}
