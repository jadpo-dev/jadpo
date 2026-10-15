//! Literals decoded once (area 2, question 15): a literal takes the type
//! it goes into, so `"0192F0C4-…"` written where a `Uuid` goes is the
//! lowercase UUID, `"2026-01-15T12:00:00Z"` where an `Instant` goes is its
//! milliseconds, and `5` where a `Decimal` with `scale: 2` goes is `5.00`.
//! The plan decodes every literal site and field default once; the
//! interpreter turns each into a value per thread and the code generator
//! writes it into the program.

use jadpo_graph::body::Literal;
use jadpo_std::{Builtin, Dur, Num};

/// A value known while compiling, as plain data the runtime reads too.
pub use jadpo_std::Constant;

/// Text as a literal of a type built on `builtin` holds it; `None` when
/// the text isn't one, which the checker rules out.
pub fn text(builtin: Option<Builtin>, text: &str) -> Option<Constant> {
    Some(match builtin.map(Builtin::representation) {
        Some(Builtin::Uuid) => Constant::Uuid(text.to_ascii_lowercase()),
        Some(Builtin::Instant) => Constant::Instant(jadpo_std::instant_millis(text)?),
        Some(Builtin::CalendarDate) => Constant::Date(jadpo_std::date_number(text)?),
        Some(Builtin::Time) => Constant::Time(jadpo_std::time_millis(text)?),
        _ => Constant::Text(text.to_string()),
    })
}

/// `literal` as a value of a type built on `builtin` (none for an object,
/// an enum or a container) whose values keep `scale` places; `None` for a
/// literal that type can't hold, which the checker rules out.
pub fn literal(
    literal: &Literal,
    builtin: Option<Builtin>,
    scale: Option<u32>,
) -> Option<Constant> {
    match literal {
        Literal::Bool(value) => Some(Constant::Bool(*value)),
        Literal::None => Some(Constant::None),
        Literal::Text(written) => text(builtin, written),
        Literal::Duration(written) => Dur::parse(written)
            .and_then(|dur| i64::try_from(dur.millis).ok())
            .map(Constant::Duration),
        Literal::Number { value, decimal } => {
            let decimal = match builtin {
                Some(builtin) => builtin.representation() == Builtin::Decimal,
                None => *decimal,
            };
            if decimal {
                let kept: Num = match scale {
                    Some(scale) => value.at_scale(scale)?,
                    None => value.at_scale(value.scale)?,
                };
                Some(Constant::Decimal {
                    mantissa: kept.mantissa,
                    scale: kept.scale,
                })
            } else if value.scale == 0 {
                i64::try_from(value.mantissa).ok().map(Constant::Int)
            } else {
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn number(text: &str, decimal: bool) -> Literal {
        Literal::Number {
            value: Num::parse(text).expect("a number"),
            decimal,
        }
    }

    #[test]
    fn literals_take_the_type_they_go_into() {
        assert_eq!(
            literal(&number("5", false), Some(Builtin::Decimal), Some(2)),
            Some(Constant::Decimal {
                mantissa: 500,
                scale: 2
            })
        );
        assert_eq!(
            literal(&number("12", false), Some(Builtin::Int), None),
            Some(Constant::Int(12))
        );
        assert_eq!(
            literal(&number("1.5", true), Some(Builtin::Int), None),
            None
        );
        assert_eq!(
            literal(
                &Literal::Text("0192F0C4-3B5E-7C1A-9D2E-4F6A8B0C1D2E".into()),
                Some(Builtin::Uuid),
                None
            ),
            Some(Constant::Uuid(
                "0192f0c4-3b5e-7c1a-9d2e-4f6a8b0c1d2e".into()
            ))
        );
        assert_eq!(
            literal(
                &Literal::Text("1970-01-01T00:00:01Z".into()),
                Some(Builtin::Instant),
                None
            ),
            Some(Constant::Instant(1000))
        );
        assert_eq!(
            literal(&Literal::Text("plain".into()), Some(Builtin::Email), None),
            Some(Constant::Text("plain".into())),
            "an email address is text"
        );
        assert_eq!(
            literal(&Literal::Duration("15m".into()), None, None),
            Some(Constant::Duration(900_000))
        );
    }
}
