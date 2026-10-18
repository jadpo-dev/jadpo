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
