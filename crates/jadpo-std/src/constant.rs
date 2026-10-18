//! A value known while compiling (area 2, question 15), in the
//! representation the runtime holds it in (`jadpo-value`'s `Value`), as
//! plain data: a literal decoded for the type it goes into, a field's
//! default, or a value a plan fills a failure's field with. `jadpo-plan`
//! decodes each once; the interpreter turns each into a value per thread
//! and the code generator writes it into the program.

use crate::{Builtin, Dur, Literal, Num};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Constant {
    Unit,
    None,
    Bool(bool),
    Int(i64),
    /// `mantissa × 10^-scale`, at the scale its type keeps.
    Decimal {
        mantissa: i128,
        scale: u32,
    },
    Text(String),
    /// Lowercase.
    Uuid(String),
    /// Milliseconds since 1970-01-01T00:00:00Z.
    Instant(i64),
    /// Days since 1970-01-01.
    Date(i32),
    /// Milliseconds.
    Duration(i64),
    /// A physical quantity: its kind and its amount in the kind's base
    /// unit, a mantissa at the kind's scale (area 19).
    Quantity {
        kind: crate::QuantityKind,
        amount: i128,
    },
    /// A percentage, as the fraction it is: `20%` is `mantissa` 20 at
    /// `scale` 2 (area 19).
    Percentage {
        mantissa: i128,
        scale: u32,
    },
    /// A variant of an enum, the `ordinal`th it declares, with a payload's
    /// fields in their declared order: a field's default.
    Variant {
        ty: String,
        ordinal: u32,
        name: String,
        fields: Vec<(String, Constant)>,
    },
    /// A secret value written where its type goes (area 19, chunk 5.5r2),
    /// in a test or a fixture: the standard type it is (`Password`) and
    /// its text, which is held so it is never printed.
    Secret {
        kind: &'static str,
        text: Hidden,
    },
}

/// A secret value's text in a plan, printed by `{:?}` as `Hidden(..)` so
/// a plan's dump never shows it (chunk 5.5r2).
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Hidden(pub String);

impl std::fmt::Debug for Hidden {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Hidden(..)")
    }
}

/// Text as a literal of a type built on `builtin` holds it; `None` when
/// the text isn't one, which the checker rules out.
pub fn text(builtin: Option<Builtin>, text: &str) -> Option<Constant> {
    Some(match builtin.map(Builtin::representation) {
        Some(Builtin::Uuid) => Constant::Uuid(text.to_ascii_lowercase()),
        Some(Builtin::Instant) => Constant::Instant(crate::instant_millis(text)?),
        Some(Builtin::CalendarDate) => Constant::Date(crate::date_number(text)?),
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
        Literal::Quantity(written) => {
            crate::quantity::parse_literal(written)
                .ok()
                .map(|(unit, amount)| Constant::Quantity {
                    kind: unit.kind,
                    amount,
                })
        }
        Literal::Percentage(written) => {
            crate::percentage::parse_literal(written).map(|fraction| Constant::Percentage {
                mantissa: fraction.mantissa,
                scale: fraction.scale,
            })
        }
        // A ratio built into a percentage, `Percentage(0.25)`, is the
        // fraction it is (area 19).
        Literal::Number { value, .. } if builtin == Some(Builtin::Percentage) => {
            Some(Constant::Percentage {
                mantissa: value.mantissa,
                scale: value.scale,
            })
        }
        // Only the zero `units.sum` is given for no items, which the
        // checker fills: a written number never stands for a quantity
        // (area 19, `type.unit_required`).
        Literal::Number { value, .. } if matches!(builtin, Some(Builtin::Quantity(_))) => {
            let Some(Builtin::Quantity(kind)) = builtin else {
                return None;
            };
            let kept = value.at_scale(kind.scale())?;
            Some(Constant::Quantity {
                kind,
                amount: kept.mantissa,
            })
        }
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

impl Constant {
    /// As the storage codec takes it (chunk 5.5w3b); `None` for a value no
    /// column holds as itself: `Unit`, a variant with fields, a secret.
    pub fn stored(&self) -> Option<crate::standard::storage::Stored<'_>> {
        use crate::standard::storage::Stored;
        Some(match self {
            Constant::None => Stored::None,
            Constant::Bool(value) => Stored::Bool(*value),
            Constant::Int(n) => Stored::Int(*n),
            Constant::Decimal { mantissa, scale } => Stored::Decimal {
                mantissa: *mantissa,
                scale: *scale,
            },
            Constant::Text(text) => Stored::Text(text),
            Constant::Uuid(text) => Stored::Uuid(text),
            Constant::Instant(millis) => Stored::Instant(*millis),
            Constant::Date(day) => Stored::Date(*day),
            Constant::Duration(millis) => Stored::Duration(*millis),
            Constant::Quantity { kind, amount } => Stored::Quantity {
                kind: *kind,
                amount: *amount,
            },
            Constant::Percentage { mantissa, scale } => Stored::Percentage {
                mantissa: *mantissa,
                scale: *scale,
            },
            Constant::Variant {
                ty, name, fields, ..
            } if fields.is_empty() => Stored::Variant { ty, name },
            Constant::Unit | Constant::Variant { .. } | Constant::Secret { .. } => return None,
        })
    }
}
