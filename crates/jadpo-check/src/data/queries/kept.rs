//! Conditions and orders on a field kept in several columns or whole as
//! its JSON (area 5, "A stored value is checked when it is read" and
//! "Structured values"; area 6, "Filters"; area 19, "SQL literals"): the
//! database compares what Jadpo wrote, so a condition holds for a record
//! exactly when code's `==` or `<` would.
//!
//! - **Named columns.** A standard type stored in named columns (`Money`,
//!   `PostalAddress`, `YearMonth`, a range) is equal when every column is,
//!   a part that may be `none` compared with `IS NOT DISTINCT FROM`. An
//!   ordered type whose columns are all numbers, dates and times, never
//!   `NULL`, orders by them in turn (`jadpo_sql::orders_by_parts`).
//!   Amounts of money order by their minor units only where the field's
//!   type and the value's both fix one currency, the proof code needs
//!   without an `attempt`. A geometry inside one has no exact `=` in
//!   PostGIS, so such a type isn't compared.
//! - **JSON.** A structured value or a standard collection is compared as
//!   the JSON the codec keeps, with `==`, `!=` and `in`, never ordered.
//!   That is exact only where the codec writes one spelling for each value:
//!   a decimal without `scale:` keeps its places (`1.5` and `1.50` are
//!   `==`), so a type holding one isn't compared. It is compared with a
//!   parameter or an enum value, which the codec writes when the query
//!   runs, never a list written out.

use super::operations::compare_written;
use super::*;
use jadpo_sql::Kept;
use jadpo_std::read::SqlType;

impl Reader<'_> {
    /// Whether `field`, kept in several columns or as JSON, can be compared
    /// with `value` (of type `ty`) by `compare` (`None` for `in`),
    /// reporting why not.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn kept_comparison(
        &mut self,
        scope: &Scope<'_>,
        entity: &EntityModel,
        field: &FieldModel,
        compare: Option<Compare>,
        value: &Value,
        ty: &Ty,
        whole: &Expr,
    ) -> bool {
        let written = format!("{}.{}", entity.name, field.name);
        let field_ty = Ty::Field(entity.name.clone(), vec![field.name.clone()]);
        match field.column().kept {
            Kept::One => true,
            Kept::Json(_) => {
                let held = field.json.clone().unwrap_or_default();
                if let Some(op) = compare.filter(|op| op.is_ordering()) {
                    self.unsupported(
                        scope,
                        whole,
                        format!("`{}` orders numbers, times, dates, durations and quantities, as it does in a body; `{written}` is a `{held}`, kept whole as its JSON, which has no order.", compare_written(op)),
                        format!("Compare `{written}` with `==`, `!=` or `in`"),
                    );
                    return false;
                }
                if let Some(decimal) = self.spelled_two_ways(&field_ty) {
                    self.unsupported(
                        scope,
                        whole,
                        format!("`{written}` is kept as its JSON, which the database compares as JSON, and it holds {decimal}, whose JSON keeps its places: `1.5` and `1.50` are `==` in code but written differently, so the database couldn't compare it as code does."),
                        "Give the decimal's type a `scale:`, or filter the records after the query".to_string(),
                    );
                    return false;
                }
                let given =
                    |value: &Value| matches!(value, Value::Param { .. } | Value::Variant { .. });
                let fine = match (compare, value) {
                    (None, Value::List(items)) => items.iter().all(given),
                    (_, value) => given(value),
                };
                if !fine {
                    self.unsupported(
                        scope,
                        whole,
                        format!("`{written}` is kept as its JSON, and is compared with a value the codec writes as the column keeps it: a parameter, a field of one, or an enum value; a list written out isn't one."),
                        format!("Pass the value as a parameter, or test `list.count({written})`"),
                    );
                    return false;
                }
                true
            }
            Kept::Parts(name) => {
                let Some(entry) = jadpo_std::standard_type(name) else {
                    return true;
                };
                let shown_type = field.compound_type();
                if !jadpo_sql::compares_parts(entry) {
                    self.unsupported(
                        scope,
                        whole,
                        format!("`{written}` is a `{shown_type}`, which holds a geometry or a vector, and the database's `=` doesn't compare those as Jadpo's `==` does: PostGIS compares geometries by their bounding boxes."),
                        "Filter the records after the query".to_string(),
                    );
                    return false;
                }
                // A standard type's value is given, never an enum value.
                let variant = |value: &Value| matches!(value, Value::Variant { .. });
                let given_variant = match value {
                    Value::List(items) => items.iter().any(variant),
                    value => variant(value),
                };
                if given_variant {
                    self.value_unsupported(scope, whole);
                    return false;
                }
                match compare {
                    Some(op) if op.is_ordering() => {
                        self.kept_ordering(scope, entity, field, entry, op, ty, whole)
                    }
                    // A list given as a parameter is sent a part at a time,
                    // each part as an array, which holds no `NULL`.
                    None if !matches!(value, Value::List(_)) => {
                        let sent = field.column().parts().is_some_and(|parts| {
                            parts.iter().all(|part| !part.nullable && sendable(part.ty))
                        });
                        if !sent {
                            self.unsupported(
                                scope,
                                whole,
                                format!("`{written}` is a `{shown_type}`, kept in several columns, and a list given as a parameter is compared a part at a time; a part of a `{shown_type}` may be `none`, or can't be sent as a list."),
                                format!("Write the values out, `{written} in [first, second]`"),
                            );
                            return false;
                        }
                        true
                    }
                    _ => true,
                }
            }
        }
    }

    /// Whether `field`, a standard type in named columns, can be ordered
    /// by `op` against a value of type `ty`.
    #[allow(clippy::too_many_arguments)]
    fn kept_ordering(
        &mut self,
        scope: &Scope<'_>,
        entity: &EntityModel,
        field: &FieldModel,
        entry: &jadpo_std::StandardType,
        op: Compare,
        ty: &Ty,
        whole: &Expr,
    ) -> bool {
        let written = format!("{}.{}", entity.name, field.name);
        let shown_type = field.compound_type();
        if entry.name == MONEY {
            let field_ty = Ty::Field(entity.name.clone(), vec![field.name.clone()]);
            // Two types of money are compared only where one widens to the
            // other, as in code, so the value's type fixes the field's
            // currency whenever both fix one.
            let field_currency = self.fixed_currency(&field_ty);
            if field_currency.is_some() && field_currency == self.fixed_currency(ty) {
                return true;
            }
            let which = if field_currency.is_none() {
                format!("`{written}`'s type doesn't fix its currency")
            } else {
                "the value's type doesn't fix its currency".to_string()
            };
            self.unsupported(
                scope,
                whole,
                format!("`{}` orders amounts of money in one currency, which code proves from their types or maps with `currency_mismatch`, and a query can't map a failure for a record; {which}.", compare_written(op)),
                "Give the field's type a `currency:`, or compare `money.minor` of the field".to_string(),
            );
            return false;
        }
        if jadpo_sql::orders_by_parts(entry) {
            return true;
        }
        let reason = if entry.ordered {
            format!(
                "`{written}` is a `{shown_type}`, kept in several columns that don't sort as it does: a `SemanticVersion` orders by precedence, which its pre-release's text doesn't keep."
            )
        } else {
            format!(
                "`{}` orders numbers, times, dates, durations and quantities, and the standard types the contract orders, as it does in a body; `{written}` is a `{shown_type}`, which has no order.",
                compare_written(op)
            )
        };
        self.unsupported(
            scope,
            whole,
            reason,
            format!("Compare `{written}` with `==`, `!=` or `in`"),
        );
        false
    }

    /// The currency an amount's type fixes, by its place among
    /// `Currency`'s variants, when it fixes one.
    fn fixed_currency(&self, ty: &Ty) -> Option<u32> {
        self.lookup
            .rule_chain(ty.strip_nullable())
            .into_iter()
            .find_map(|(_, rules)| rules.currency)
    }

    /// Whether `order:` can sort by `field`, kept in several columns: an
    /// ordered type sorted by its columns in turn, or an amount of money
    /// whose type fixes its currency, sorted by its minor units.
    pub(super) fn kept_sortable(&self, field: &FieldModel) -> bool {
        match field.compound_entry() {
            Some(entry) if entry.name == MONEY => {
                field.fixed.iter().any(|(suffix, _)| *suffix == "currency")
            }
            Some(entry) => jadpo_sql::orders_by_parts(entry),
            None => false,
        }
    }

    /// What `ty`, kept as JSON, holds that JSON can write two ways for one
    /// value ([`spelled_two_ways`]).
    fn spelled_two_ways(&self, ty: &Ty) -> Option<String> {
        spelled_two_ways(self.lookup, ty)
    }
}

/// What `ty`, kept as JSON, holds that JSON can write two ways for one
/// value: a decimal or a percentage whose places its type doesn't fix,
/// anywhere inside, a standard type's parts included. The database tells
/// two such values apart where code's `==` doesn't, so a query compares
/// none and a unique rule covers none (chunk Q2).
pub(crate) fn spelled_two_ways(lookup: &dyn crate::types::TypeView, ty: &Ty) -> Option<String> {
    let reads = crate::data::columns::json_reads(lookup, ty);
    let mut seen = BTreeSet::new();
    let mut found = None;
    two_ways(&reads.shape, &reads.types, &mut seen, &mut found);
    found
}

/// The standard type an amount of money is.
const MONEY: &str = "Money";

/// Whether a part's type can be sent as an array, whose items are never
/// `NULL`.
fn sendable(ty: SqlType) -> bool {
    matches!(
        ty,
        SqlType::Bool
            | SqlType::Int2
            | SqlType::Int4
            | SqlType::Int8
            | SqlType::Numeric
            | SqlType::Text
            | SqlType::Bytes
            | SqlType::Uuid
            | SqlType::Timestamp
            | SqlType::Date
            | SqlType::Time
            | SqlType::Json
    )
}

/// Walks a JSON-held value's shape for what [`Reader::spelled_two_ways`]
/// looks for, each named type once.
fn two_ways(
    shape: &jadpo_graph::stored_json::Shape,
    types: &BTreeMap<String, jadpo_graph::stored_json::Named>,
    seen: &mut BTreeSet<String>,
    found: &mut Option<String>,
) {
    use jadpo_graph::stored_json::{Named, Shape};
    if found.is_some() {
        return;
    }
    match shape {
        Shape::Value(scalar) => {
            if scalar.scale.is_none() && holds_places(&scalar.ty) {
                *found = Some(format!("a `{}`", scalar.ty));
            }
        }
        Shape::Optional(inner) => two_ways(inner, types, seen, found),
        Shape::List { item, .. } => two_ways(item, types, seen, found),
        Shape::Map { key, value, .. } => {
            two_ways(key, types, seen, found);
            two_ways(value, types, seen, found);
        }
        Shape::Named(name) => {
            if !seen.insert(name.clone()) {
                return;
            }
            match types.get(name) {
                Some(Named::Object { fields, .. }) => {
                    for shape in fields.values() {
                        two_ways(shape, types, seen, found);
                    }
                }
                Some(Named::Enum { variants }) => {
                    for fields in variants.values().flatten() {
                        for shape in fields.values() {
                            two_ways(shape, types, seen, found);
                        }
                    }
                }
                None => {}
            }
        }
    }
}

/// Whether a value of the built-in or standard type written `ty`
/// (`Decimal`, `Range<Decimal>`, `Coordinates`) keeps places JSON writes:
/// a decimal, a percentage or a float, or a standard type holding one.
fn holds_places(ty: &str) -> bool {
    ty.split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .any(|word| match word {
            "Decimal" | "Percentage" => true,
            other => jadpo_std::standard_type(other).is_some_and(standard_places),
        })
}

/// Whether a standard type's parts hold a decimal, a percentage or a
/// float, its own value included.
fn standard_places(entry: &jadpo_std::StandardType) -> bool {
    use jadpo_std::standard::Holds;
    match entry.holds {
        Holds::Builtin(builtin) | Holds::Bounded { of: builtin, .. } => {
            matches!(
                builtin.representation(),
                jadpo_std::Builtin::Decimal | jadpo_std::Builtin::Percentage
            )
        }
        Holds::Object(fields) => fields.iter().any(|(_, part)| part_places(*part)),
        Holds::Choice(variants) => variants
            .iter()
            .any(|(_, fields)| fields.iter().any(|(_, part)| part_places(*part))),
        _ => false,
    }
}

/// [`standard_places`] of one part.
fn part_places(part: jadpo_std::Part) -> bool {
    use jadpo_std::Part;
    match part {
        Part::Optional(inner) | Part::List(inner) | Part::Set(inner) => part_places(*inner),
        Part::Float => true,
        Part::Of(builtin) | Part::Bounded { of: builtin, .. } => matches!(
            builtin.representation(),
            jadpo_std::Builtin::Decimal | jadpo_std::Builtin::Percentage
        ),
        Part::Standard(name) => jadpo_std::standard_type(name).is_some_and(standard_places),
        _ => false,
    }
}
