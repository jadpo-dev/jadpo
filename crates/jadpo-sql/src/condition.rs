//! Conditions (area 6, "Filters"; area 5, "Lifecycle"): the model of a
//! condition on one entity's fields, which the checker reads from a
//! query's `where:` lines and a lifecycle's `visible:` and `when:`, and one
//! renderer for it, which the plan's statements and the predicate of a
//! partial index both use, so each reads the same wherever it lands.
//!
//! A condition keeps Jadpo's meaning where a field is `none`: it meets no
//! comparison but `!=`, and `not` is pushed down to the comparisons, so
//! SQL's three-valued logic never shows. A literal or an enum value is
//! written into the text; anything else (a parameter, `clock.now`) is a
//! parameter the statement binds, through [`Bind`].

use std::collections::BTreeSet;

use jadpo_std::read::SqlType;
use jadpo_std::{Dur, Literal};

use crate::{NULL, ident, text_literal};

/// A value a condition reads: never computed.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Value {
    /// A parameter, by position, or a field inside it: `filter.status`.
    Param {
        index: u32,
        path: Vec<String>,
    },
    /// A literal, as written, which takes the type of the field it is
    /// compared with.
    Literal(Literal),
    /// A plain variant of an enum: `TodoStatus.open`.
    Variant {
        ty: String,
        variant: String,
    },
    List(Vec<Value>),
    /// `clock.now`: the time the operation started.
    Now,
}

impl Value {
    /// Whether it reads a parameter anywhere inside.
    pub fn reads_param(&self) -> bool {
        match self {
            Value::Param { .. } => true,
            Value::List(items) => items.iter().any(Value::reads_param),
            _ => false,
        }
    }
}

/// A comparison's operator.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Compare {
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
}

impl Compare {
    pub fn is_ordering(self) -> bool {
        !matches!(self, Compare::Equal | Compare::NotEqual)
    }

    /// The SQL operator.
    pub fn sql(self) -> &'static str {
        match self {
            Compare::Equal => "=",
            Compare::NotEqual => "<>",
            Compare::Less => "<",
            Compare::LessEqual => "<=",
            Compare::Greater => ">",
            Compare::GreaterEqual => ">=",
        }
    }

    /// The operator that holds exactly when this one doesn't, for two
    /// values that are both present.
    pub fn negated(self) -> Compare {
        match self {
            Compare::Equal => Compare::NotEqual,
            Compare::NotEqual => Compare::Equal,
            Compare::Less => Compare::GreaterEqual,
            Compare::LessEqual => Compare::Greater,
            Compare::Greater => Compare::LessEqual,
            Compare::GreaterEqual => Compare::Less,
        }
    }
}

/// A condition on one entity's fields, as written: one an index can serve
/// (area 6, "Filters").
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Condition {
    /// `Todo.field <op> value`.
    Compare {
        field: String,
        op: Compare,
        value: Value,
    },
    /// `Todo.field == none`, or `!= none` when `negated`.
    IsNone {
        field: String,
        negated: bool,
    },
    /// `Todo.field in list`.
    In {
        field: String,
        value: Value,
    },
    And(Vec<Condition>),
    Or(Vec<Condition>),
    Not(Box<Condition>),
    /// A `where_present:` line: `condition` applies only when `value` is
    /// present, and reads it as present.
    Present {
        value: Value,
        condition: Box<Condition>,
    },
}

impl Condition {
    /// The fields it tests.
    pub fn fields(&self, out: &mut BTreeSet<String>) {
        match self {
            Condition::Compare { field, .. }
            | Condition::IsNone { field, .. }
            | Condition::In { field, .. } => {
                out.insert(field.clone());
            }
            Condition::And(all) | Condition::Or(all) => {
                all.iter().for_each(|c| c.fields(out));
            }
            Condition::Not(inner)
            | Condition::Present {
                condition: inner, ..
            } => inner.fields(out),
        }
    }
}

/// What a condition needs of the column that holds one field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Column<'a> {
    /// The column's name: the field's born name, which survives a rename.
    pub name: &'a str,
    /// Whether the field may be `none`.
    pub nullable: bool,
    /// The column's type; `None` when the field can't be stored.
    pub ty: Option<SqlType>,
}

impl Column<'_> {
    /// The type a value compared with the column is sent as.
    fn param(&self) -> Option<SqlType> {
        self.ty.map(SqlType::param)
    }
}

/// An entity's fields as the columns that hold them, by each field's name
/// now.
pub trait Columns {
    fn column(&self, field: &str) -> Option<Column<'_>>;
}

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

/// A condition on `entity`'s fields as SQL, `negated` when a `not` above
/// it was pushed down.
pub fn condition(
    bind: &mut dyn Bind,
    entity: &dyn Columns,
    condition: &Condition,
    negated: bool,
) -> Option<String> {
    Some(match condition {
        Condition::Compare {
            field: name,
            op,
            value,
        } => {
            let field = entity.column(name)?;
            let column = ident(field.name);
            let ty = field.param()?;
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
            let field = entity.column(name)?;
            let is_not = *written != negated;
            format!(
                "{} IS {}NULL",
                ident(field.name),
                if is_not { "NOT " } else { "" }
            )
        }
        Condition::In { field: name, value } => {
            let field = entity.column(name)?;
            let column = ident(field.name);
            let ty = field.param()?;
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
                Condition::Compare { field: name, .. } => entity.column(name)?.param()?,
                Condition::In { field: name, .. } => entity.column(name)?.param()?.array(),
                _ => return None,
            };
            let n = bind.bind(value, ty)?;
            let inner = self::condition(bind, entity, condition, negated)?;
            format!("({n} IS NULL OR {inner})")
        }
    })
}

/// A value as SQL, sent as `ty`: a literal or a variant written into the
/// text, anything else bound.
pub fn value(bind: &mut dyn Bind, value: &Value, ty: SqlType) -> Option<String> {
    Some(match value {
        Value::Literal(literal) => self::literal(literal, ty)?,
        // As its column holds it: `Currency`'s by its code (area 19).
        Value::Variant { ty, variant } => {
            text_literal(jadpo_std::standard::stored_variant(ty, variant))
        }
        Value::List(_) => return None,
        other => bind.bind(other, ty)?,
    })
}

/// A literal written into a statement for a column of type `ty`; `none`
/// is `NULL`, and a duration is its milliseconds in a `bigint`.
pub fn literal(literal: &Literal, ty: SqlType) -> Option<String> {
    Some(match literal {
        Literal::Text(text) => text_literal(text),
        Literal::Number { value, .. } => value.to_string(),
        Literal::Bool(true) => "TRUE".to_string(),
        Literal::Bool(false) => "FALSE".to_string(),
        Literal::None => NULL.to_string(),
        Literal::Duration(written) if ty == SqlType::Int8 => {
            Dur::parse(written)?.millis.to_string()
        }
        Literal::Duration(_) => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A table of one optional text field, `title`, born `name`.
    struct Notes;

    impl Columns for Notes {
        fn column(&self, field: &str) -> Option<Column<'_>> {
            (field == "title").then_some(Column {
                name: "name",
                nullable: true,
                ty: Some(SqlType::Text),
            })
        }
    }

    /// Binds every value as the next parameter.
    #[derive(Default)]
    struct Params(usize);

    impl Bind for Params {
        fn bind(&mut self, _value: &Value, _ty: SqlType) -> Option<String> {
            self.0 += 1;
            Some(format!("${}", self.0))
        }
    }

    #[test]
    fn none_meets_only_not_equal_and_not_is_pushed_down() {
        let equal = Condition::Compare {
            field: "title".into(),
            op: Compare::Equal,
            value: Value::Literal(Literal::Text("it's".into())),
        };
        assert_eq!(
            condition(&mut Fixed, &Notes, &equal, false).as_deref(),
            Some("\"name\" = 'it''s'")
        );
        assert_eq!(
            condition(&mut Fixed, &Notes, &Condition::Not(Box::new(equal)), false).as_deref(),
            Some("(\"name\" <> 'it''s' OR \"name\" IS NULL)")
        );
        let present = Condition::Present {
            value: Value::Param {
                index: 0,
                path: Vec::new(),
            },
            condition: Box::new(Condition::In {
                field: "title".into(),
                value: Value::Param {
                    index: 0,
                    path: Vec::new(),
                },
            }),
        };
        assert_eq!(
            condition(&mut Params::default(), &Notes, &present, false).as_deref(),
            Some("($1 IS NULL OR \"name\" = ANY($2))")
        );
        assert_eq!(
            condition(&mut Fixed, &Notes, &present, false),
            None,
            "a predicate takes no parameters"
        );
    }
}
