//! A literal as written in source (area 2): a number, a duration, text, a
//! flag or `none`, before the type it goes into decides how the runtime
//! holds it. Typed bodies hold one at each literal site, a field's default
//! and a query's condition each hold one, and the SQL renderer writes one
//! into a statement, so all read the same form.

use crate::Num;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Literal {
    Number {
        value: Num,
        decimal: bool,
    },
    /// A duration as written: `15m`.
    Duration(String),
    /// A physical quantity as written, with its unit's suffix: `2.5kg`,
    /// `-40degC` (area 19, "Physical quantities").
    Quantity(String),
    /// A percentage as written, `20%` (area 19): the fraction 0.20.
    Percentage(String),
    Text(String),
    Bool(bool),
    None,
}

impl Literal {
    /// As Jadpo writes it: `25`, `1.50`, `"text"`, `15m`, `none`.
    pub fn written(&self) -> String {
        match self {
            Literal::Number { value, .. } => value.to_string(),
            Literal::Duration(written)
            | Literal::Quantity(written)
            | Literal::Percentage(written) => written.clone(),
            Literal::Text(text) => format!("{text:?}"),
            Literal::Bool(value) => value.to_string(),
            Literal::None => "none".to_string(),
        }
    }
}
