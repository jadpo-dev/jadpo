//! How a standard object or choice is laid out in its named columns (area
//! 19, "Every standard type meets one contract"; chunk 5.5w). An entry
//! names its columns ([`super::Storage::Columns`]); this module says, from
//! its parts alone, what each column holds, so one codec
//! (`jadpo-value`'s `standard::to_parts` and `from_parts`) reads and
//! writes every type, and a test holds each entry's names to the layout:
//!
//! - an object's parts in order, each in as many columns as it takes: an
//!   amount of money two (minor units and its currency's code), another
//!   standard object or choice its own columns, everything else one;
//! - a choice's variant's name first, then each field name its variants
//!   use once, in the order they first appear, `NULL` where the stored
//!   variant hasn't it;
//! - a type stored natively, in one column named for the field (a suffix
//!   of `""`): a range in PostgreSQL's range type, a `LocalTime` in `time`,
//!   a `LocalDateTime` in `timestamp` without a zone.
//!
//! A part that may be `none` is a column that may be `NULL`, and a
//! quantity's column carries a comment naming the SI unit it holds.

use super::{Holds, Part, StandardType, standard_type};
use crate::Builtin;

/// The column of a choice that holds which variant it is.
pub const KIND: &str = "kind";

/// One column of a standard type's layout.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Laid {
    /// The column's type as PostgreSQL writes it.
    pub sql_type: String,
    /// Whether it may be `NULL` while the value is there.
    pub nullable: bool,
    /// What it holds, for a column holding a quantity: its kind and unit.
    pub comment: Option<String>,
}

/// The columns `part` takes, in order; `None` for a part no column holds
/// (a list of objects), which is stored as JSON.
pub fn part_columns(part: Part) -> Option<Vec<Laid>> {
    let one = |sql_type: &str| {
        Some(vec![Laid {
            sql_type: sql_type.to_string(),
            nullable: false,
            comment: None,
        }])
    };
    match part {
        Part::Optional(inner) => {
            let mut columns = part_columns(*inner)?;
            for column in &mut columns {
                column.nullable = true;
            }
            Some(columns)
        }
        Part::List(inner) => match part_columns(*inner)?.as_slice() {
            [column] if column.sql_type == "text" || column.sql_type == "char(3)" => one("text[]"),
            _ => None,
        },
        Part::Whole(min, max) => one(whole(min, max)),
        Part::Line | Part::Matching(_) | Part::Paragraph => one("text"),
        Part::Float => one("numeric"),
        Part::Of(builtin) | Part::Bounded { of: builtin, .. } => builtin_columns(builtin),
        Part::Standard(name) => {
            let entry = standard_type(name)?;
            if !entry.columns().is_empty() {
                return layout(entry);
            }
            match entry.holds {
                Holds::Builtin(builtin) | Holds::Bounded { of: builtin, .. } => {
                    builtin_columns(builtin)
                }
                Holds::Whole { min, max } => one(whole(min, max)),
                Holds::Enum(_) if name == crate::CURRENCY => one("char(3)"),
                _ => one(entry.column_type()),
            }
        }
    }
}

/// The columns a value built on `builtin` takes.
fn builtin_columns(builtin: Builtin) -> Option<Vec<Laid>> {
    let laid = |sql_type: &str, comment: Option<String>| Laid {
        sql_type: sql_type.to_string(),
        nullable: false,
        comment,
    };
    Some(match builtin {
        Builtin::Money => vec![laid("bigint", None), laid("char(3)", None)],
        Builtin::Quantity(kind) => vec![laid(kind.column_type(), Some(unit_comment(kind)))],
        Builtin::Bool => vec![laid("boolean", None)],
        Builtin::Int => vec![laid("bigint", None)],
        Builtin::Decimal | Builtin::Percentage => vec![laid("numeric", None)],
        Builtin::Bytes => vec![laid("bytea", None)],
        Builtin::Uuid => vec![laid("uuid", None)],
        Builtin::Instant => vec![laid("timestamptz(3)", None)],
        Builtin::CalendarDate => vec![laid("date", None)],
        Builtin::Duration => vec![laid("bigint", None)],
        Builtin::Text | Builtin::Email | Builtin::Url | Builtin::IpAddress => {
            vec![laid("text", None)]
        }
        _ => return None,
    })
}

/// A whole number's column: the narrowest that holds its range.
fn whole(min: i64, max: i64) -> &'static str {
    if min >= i64::from(i16::MIN) && max <= i64::from(i16::MAX) {
        "smallint"
    } else if min >= i64::from(i32::MIN) && max <= i64::from(i32::MAX) {
        "integer"
    } else {
        "bigint"
    }
}

/// The comment on a quantity's column: its kind and the SI unit it holds,
/// `Length in metres (m)`.
pub fn unit_comment(kind: crate::QuantityKind) -> String {
    let unit = kind.base_unit();
    format!("{} in {} ({})", kind.name(), unit.meaning, unit.symbol)
}

/// Each field a choice's columns hold after its kind: every field name its
/// variants use, once, with the part the first variant using it gives it,
/// in the order they first appear.
pub fn choice_fields(
    variants: &'static [(&'static str, &'static [(&'static str, Part)])],
) -> Vec<(&'static str, Part)> {
    let mut out: Vec<(&'static str, Part)> = Vec::new();
    for (_, fields) in variants {
        for (name, part) in *fields {
            if !out.iter().any(|(seen, _)| seen == name) {
                out.push((name, *part));
            }
        }
    }
    out
}

/// How many columns `part` takes.
pub fn width(part: Part) -> usize {
    part_columns(part).map_or(1, |columns| columns.len())
}

/// The columns an entry's value is laid out in, from its parts, in order;
/// `None` for a type no named columns hold.
pub fn layout(entry: &StandardType) -> Option<Vec<Laid>> {
    let native = |sql_type: &str| {
        Some(vec![Laid {
            sql_type: sql_type.to_string(),
            nullable: false,
            comment: None,
        }])
    };
    // A type stored natively, in one column of PostgreSQL's own type.
    if let [("", sql_type)] = entry.columns() {
        return native(sql_type);
    }
    match entry.holds {
        Holds::Object(fields) => {
            let mut out = Vec::new();
            for (_, part) in fields {
                out.extend(part_columns(*part)?);
            }
            Some(out)
        }
        Holds::Choice(variants) => {
            let mut out = vec![Laid {
                sql_type: "text".to_string(),
                nullable: false,
                comment: None,
            }];
            for (name, _) in choice_fields(variants) {
                // Each variant using the field: where it holds a quantity,
                // the comment names the unit for each.
                let users: Vec<(&str, Part)> = variants
                    .iter()
                    .filter_map(|(variant, fields)| {
                        fields
                            .iter()
                            .find(|(field, _)| *field == name)
                            .map(|(_, part)| (*variant, *part))
                    })
                    .collect();
                let (_, first) = users.first()?;
                let mut columns = part_columns(*first)?;
                // A quantity of each kind in one column: the finest scale
                // any of them keeps.
                for (_, part) in &users {
                    for (column, other) in columns.iter_mut().zip(part_columns(*part)?) {
                        if other.sql_type > column.sql_type
                            && column.sql_type.starts_with("numeric(38, ")
                        {
                            column.sql_type = other.sql_type;
                        }
                    }
                }
                let comments: Vec<String> = users
                    .iter()
                    .filter_map(|(variant, part)| {
                        let laid = part_columns(*part)?;
                        let comment = laid.first()?.comment.clone()?;
                        Some(format!("{comment} when {KIND} is {variant}"))
                    })
                    .collect();
                for column in &mut columns {
                    column.nullable = true;
                    if column.comment.is_some() && users.len() > 1 {
                        column.comment = Some(comments.join("; "));
                    }
                }
                out.extend(columns);
            }
            Some(out)
        }
        Holds::Builtin(Builtin::Money) => builtin_columns(Builtin::Money),
        Holds::Builtin(Builtin::ZonedTime) => Some(vec![
            Laid {
                sql_type: "timestamptz(3)".to_string(),
                nullable: false,
                comment: None,
            },
            Laid {
                sql_type: "text".to_string(),
                nullable: false,
                comment: None,
            },
        ]),
        _ => None,
    }
}

// --- The storage codec (chunk 5.5w3b) -----------------------------------------

/// A value as the storage codec takes it: what a runtime value and a value
/// known while compiling (a literal, an enum value) both are, so one
/// function, [`bind`], makes what a column is given from either, and a
/// literal the compiler writes into a statement is the value the runtime
/// binds (area 19, "Every boundary through the contract"; chunk 5.5w3b).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Stored<'a> {
    None,
    Bool(bool),
    Int(i64),
    /// `mantissa × 10^-scale`, at the places it keeps.
    Decimal {
        mantissa: i128,
        scale: u32,
    },
    Text(&'a str),
    Uuid(&'a str),
    Bytes(&'a [u8]),
    /// Milliseconds since 1970-01-01T00:00:00Z.
    Instant(i64),
    /// Days since 1970-01-01.
    Date(i32),
    /// Milliseconds.
    Duration(i64),
    /// In its kind's base unit, a mantissa at the kind's scale.
    Quantity {
        kind: crate::QuantityKind,
        amount: i128,
    },
    /// The fraction it is: `20%` is `mantissa` 20 at `scale` 2.
    Percentage {
        mantissa: i128,
        scale: u32,
    },
    /// A variant without fields of the enum `ty`.
    Variant {
        ty: &'a str,
        name: &'a str,
    },
    /// A list's items, or a range's ends and whether it holds its upper
    /// one, as the runtime holds a range's columns.
    List(Vec<Stored<'a>>),
    /// A set's items.
    Set(Vec<Stored<'a>>),
}

impl Stored<'_> {
    /// What kind of value it is, never what it holds, as a fault names it
    /// (chunk 5.5w2).
    pub fn described(&self) -> String {
        let kind = match self {
            Stored::None => "`none`",
            Stored::Bool(_) => "a `Bool`",
            Stored::Int(_) => "an `Int`",
            Stored::Decimal { .. } => "a `Decimal`",
            Stored::Text(_) => "text",
            Stored::Bytes(_) => "bytes",
            Stored::Uuid(_) => "a `Uuid`",
            Stored::Instant(_) => "an `Instant`",
            Stored::Date(_) => "a `CalendarDate`",
            Stored::Duration(_) => "a `Duration`",
            Stored::Quantity { .. } => "a quantity",
            Stored::Percentage { .. } => "a `Percentage`",
            Stored::Variant { ty, .. } => return format!("a `{ty}`"),
            Stored::List(_) => "a list",
            Stored::Set(_) => "a set",
        };
        kind.to_string()
    }
}

/// `value` as a parameter of a column of type `ty`, as the contract's
/// storage holds it: a duration as its milliseconds, a quantity in its
/// kind's base unit, a percentage as the fraction it is, an enum's value by
/// its code where its column holds codes, an instant as microseconds. The
/// one encoder the runtime binds every parameter with and the compiler
/// writes every literal with (chunk 5.5w3b). `Err` names what the value is
/// and the rule, never the value (chunk 5.5w2).
pub fn bind(value: &Stored<'_>, ty: crate::SqlType) -> Result<crate::SqlValue, String> {
    use crate::{SqlType, SqlValue};
    Ok(match (value, ty) {
        (Stored::None, ty) => SqlValue::Null(ty),
        (Stored::Bool(b), SqlType::Bool) => SqlValue::Bool(*b),
        (Stored::Int(n), SqlType::Int8) => SqlValue::Int8(*n),
        (Stored::Int(n), SqlType::Int4) => SqlValue::Int4(
            i32::try_from(*n).map_err(|_| "a whole number too large for its column")?,
        ),
        (Stored::Int(n), SqlType::Int2) => SqlValue::Int2(
            i16::try_from(*n).map_err(|_| "a whole number too large for its column")?,
        ),
        (Stored::Int(n), SqlType::Numeric) => SqlValue::Numeric(n.to_string()),
        (Stored::Decimal { mantissa, scale }, SqlType::Numeric) => SqlValue::Numeric(
            crate::Num {
                mantissa: *mantissa,
                scale: *scale,
            }
            .to_string(),
        ),
        (Stored::Text(text), SqlType::Text) => SqlValue::Text(text.to_string()),
        (Stored::Uuid(text) | Stored::Text(text), SqlType::Uuid) => {
            SqlValue::Uuid(uuid_bytes(text).ok_or("text that isn't a UUID")?)
        }
        (Stored::Uuid(text), SqlType::Text) => SqlValue::Text(text.to_string()),
        (Stored::Bytes(bytes), SqlType::Bytes) => SqlValue::Bytes(bytes.to_vec()),
        (Stored::Instant(millis), SqlType::Timestamp) => {
            SqlValue::Timestamp(millis.checked_mul(1000).ok_or("an instant out of range")?)
        }
        (Stored::Date(day), SqlType::Date) => SqlValue::Date(*day),
        (Stored::Duration(millis), SqlType::Int8) => SqlValue::Int8(*millis),
        // In its kind's base unit at its scale (area 19).
        (Stored::Quantity { kind, amount }, SqlType::Numeric) => {
            SqlValue::Numeric(crate::quantity::number_text(*amount, kind.scale()))
        }
        // The fraction it is (area 19).
        (Stored::Percentage { mantissa, scale }, SqlType::Numeric) => SqlValue::Numeric(
            crate::Num {
                mantissa: *mantissa,
                scale: *scale,
            }
            .to_string(),
        ),
        // By its code where its column holds codes: `Currency`'s (area 19).
        (Stored::Variant { ty, name }, SqlType::Text) => {
            SqlValue::Text(super::stored_variant(ty, name).to_string())
        }
        // A `LocalTime` and a `LocalDateTime`, as microseconds (chunk 5.5w).
        (Stored::Int(micros), SqlType::Time) => SqlValue::Time(*micros),
        (Stored::Int(micros), SqlType::LocalTimestamp) => SqlValue::LocalTimestamp(*micros),
        // A range's ends and whether it holds its upper one (chunk 5.5w).
        (Stored::List(ends), SqlType::Range(element)) => match &ends[..] {
            // One whose start is its end holds nothing, and PostgreSQL keeps
            // such a range as `empty`, without its ends, so it wouldn't
            // read back.
            [lower, upper, Stored::Bool(false)] if *lower != Stored::None && lower == upper => {
                return Err(
                    "a range whose start is its end, which PostgreSQL keeps without its ends"
                        .into(),
                );
            }
            [lower, upper, Stored::Bool(upper_held)] => {
                let end = |value: &Stored<'_>| -> Result<Option<Box<SqlValue>>, String> {
                    Ok(match value {
                        Stored::None => None,
                        value => Some(Box::new(bind(value, *element)?)),
                    })
                };
                SqlValue::Range {
                    element: *element,
                    lower: end(lower)?,
                    upper: end(upper)?,
                    upper_held: *upper_held,
                }
            }
            _ => return Err("a range without its two ends".into()),
        },
        (Stored::List(items) | Stored::Set(items), SqlType::Array(inner)) => SqlValue::Array(
            *inner,
            items
                .iter()
                .map(|item| bind(item, *inner))
                .collect::<Result<Vec<_>, _>>()?,
        ),
        (value, ty) => {
            return Err(format!(
                "{} that a `{ty:?}` column can't hold",
                value.described()
            ));
        }
    })
}

/// A UUID's sixteen bytes from its text, hyphens anywhere.
pub fn uuid_bytes(text: &str) -> Option<[u8; 16]> {
    let hex: Vec<u8> = text.bytes().filter(|b| *b != b'-').collect();
    if hex.len() != 32 {
        return None;
    }
    let mut out = [0u8; 16];
    for (byte, pair) in out.iter_mut().zip(hex.chunks(2)) {
        let [high, low] = pair else { return None };
        let high = (*high as char).to_digit(16)?;
        let low = (*low as char).to_digit(16)?;
        *byte = u8::try_from(high * 16 + low).ok()?;
    }
    Some(out)
}
