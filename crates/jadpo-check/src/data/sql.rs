//! Conditions as PostgreSQL (area 6, "Filters"; area 5, "Lifecycle"):
//! one renderer for a query's `where:` lines, a lifecycle's `visible:` and
//! `when:`, and the predicate of a partial index, so each reads the same
//! wherever it lands.
//!
//! A condition keeps Jadpo's meaning where a field is `none`: it meets no
//! comparison but `!=`, and `not` is pushed down to the comparisons, so
//! SQL's three-valued logic never shows. A literal or an enum value is
//! written into the text; anything else (a parameter, `clock.now`) is a
//! parameter the statement binds, through [`Bind`].

use jadpo_std::read::SqlType;

use super::queries::{Compare, Condition, Value};
use super::{EntityModel, FieldModel};
use crate::types::Const;

/// Where a statement's parameters go.
pub trait Bind {
    /// `$n` for `value`, sent as `ty`: a parameter or `clock.now`. `None`
    /// when the statement can't take it, as an index's predicate can't.
    fn bind(&mut self, value: &Value, ty: SqlType) -> Option<String>;
}

/// For text that takes no parameters: a partial index's predicate, which
/// must mean the same at every moment.
#[derive(Debug)]
pub struct Fixed;

impl Bind for Fixed {
    fn bind(&mut self, _value: &Value, _ty: SqlType) -> Option<String> {
        None
    }
}

/// A condition as SQL, `negated` when a `not` above it was pushed
/// down.
pub fn condition(
    bind: &mut dyn Bind,
    entity: &EntityModel,
    condition: &Condition,
    negated: bool,
) -> Option<String> {
    Some(match condition {
        Condition::Compare {
            field: name,
            op,
            value,
        } => {
            let field = field(entity, name)?;
            let column = quoted(&field.born);
            let ty = column_type(field)?.param();
            let rendered = self::value(bind, value, ty)?;
            // Jadpo's meaning when the field is `none`: it meets no
            // comparison but `!=`, and so meets the negation of every
            // other.
            let none_meets = (*op == Compare::NotEqual) != negated;
            let op = if negated { op.negated() } else { *op };
            let text = format!("{column} {} {rendered}", op.sql());
            if field.nullable && none_meets {
                format!("({text} OR {column} IS NULL)")
            } else {
                text
            }
        }
        Condition::IsNone {
            field: name,
            negated: written,
        } => {
            let field = field(entity, name)?;
            let is_not = *written != negated;
            format!(
                "{} IS {}NULL",
                quoted(&field.born),
                if is_not { "NOT " } else { "" }
            )
        }
        Condition::In { field: name, value } => {
            let field = field(entity, name)?;
            let column = quoted(&field.born);
            let ty = column_type(field)?.param();
            let text = match value {
                Value::List(items) => {
                    if items.is_empty() {
                        return Some(if negated { "TRUE" } else { "FALSE" }.to_string());
                    }
                    let mut rendered = Vec::new();
                    for item in items {
                        rendered.push(self::value(bind, item, ty)?);
                    }
                    format!(
                        "{column} {}IN ({})",
                        if negated { "NOT " } else { "" },
                        rendered.join(", ")
                    )
                }
                other => {
                    let n = bind.bind(other, ty.array())?;
                    if negated {
                        format!("{column} <> ALL({n})")
                    } else {
                        format!("{column} = ANY({n})")
                    }
                }
            };
            if negated && field.nullable {
                format!("({text} OR {column} IS NULL)")
            } else {
                text
            }
        }
        Condition::And(parts) | Condition::Or(parts) => {
            let and = matches!(condition, Condition::And(_)) != negated;
            let mut rendered = Vec::new();
            for part in parts {
                rendered.push(self::condition(bind, entity, part, negated)?);
            }
            format!("({})", rendered.join(if and { " AND " } else { " OR " }))
        }
        Condition::Not(inner) => {
            return self::condition(bind, entity, inner, !negated);
        }
        Condition::Present { value, condition } => {
            let ty = match condition.as_ref() {
                Condition::Compare { field: name, .. } => {
                    column_type(field(entity, name)?)?.param()
                }
                Condition::In { field: name, .. } => {
                    column_type(field(entity, name)?)?.param().array()
                }
                _ => return None,
            };
            let n = bind.bind(value, ty)?;
            let inner = self::condition(bind, entity, condition, negated)?;
            format!("({n} IS NULL OR {inner})")
        }
    })
}

/// A value as SQL: a literal written into the text, anything else bound.
pub fn value(bind: &mut dyn Bind, value: &Value, ty: SqlType) -> Option<String> {
    Some(match value {
        Value::Literal(constant) => literal(constant, ty)?,
        Value::Variant { variant, .. } => text_literal(variant),
        Value::List(_) => return None,
        other => bind.bind(other, ty)?,
    })
}

pub fn field<'e>(entity: &'e EntityModel, name: &str) -> Option<&'e FieldModel> {
    entity.fields.iter().find(|f| f.name == name)
}

/// A field's column type, as a parameter or column is sent.
pub fn column_type(field: &FieldModel) -> Option<SqlType> {
    SqlType::of_column(field.sql_type.as_deref()?)
}

/// An identifier, always quoted: a table called `user` or `order` is a
/// keyword unquoted.
pub fn quoted(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

pub fn text_literal(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}

/// A literal written into a statement for a column of type `ty`; `none`
/// is `NULL`.
pub fn literal(constant: &Const, ty: SqlType) -> Option<String> {
    Some(match constant {
        Const::Str(text) => text_literal(text),
        Const::Number { value, .. } => value.to_string(),
        Const::Bool(true) => "TRUE".to_string(),
        Const::Bool(false) => "FALSE".to_string(),
        Const::None => "NULL".to_string(),
        Const::Duration(written) if ty == SqlType::Int8 => {
            jadpo_std::Dur::parse(written)?.millis.to_string()
        }
        _ => return None,
    })
}
