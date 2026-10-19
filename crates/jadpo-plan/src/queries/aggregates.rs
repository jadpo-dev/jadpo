//! The one statement an aggregate query runs (area 6, "Counts, sums and
//! groups"; chunk 5.6c): a `SELECT` of each group and aggregate, the
//! query's conditions with the lifecycle's and policy's, `GROUP BY` the
//! groups, its order and its limit.
//!
//! Each aggregate gives what code working from the records gives:
//!
//! - `count()` is `count(*)`, a `bigint`.
//! - `sum` is PostgreSQL's exact `sum`, zero for no values, at the field's
//!   places for a decimal; a whole number, a duration and money's minor
//!   units are cast back to `bigint`, so a sum past 64 bits stops the
//!   read, as it is a defect in code. Money's currency is the one its type
//!   fixes, sent as a parameter.
//! - `min` and `max` are PostgreSQL's, text compared by code point
//!   (`COLLATE "C"`).
//! - `average` is `decimal.divide`'s SQL form (area 19) of the sum, at
//!   `scale:` places, by the count of values, `NULL` when there are none.
//! - A time bucket is its `temporal` operation's SQL form.
//!
//! A group sorts in Jadpo's own order: text by code point, an enum's
//! variants in their declared order (`array_position` of the codes they
//! are stored as), the rest as PostgreSQL orders them.

use super::*;
use jadpo_check::data::queries::{AggregateKind, Entry, GroupKey, Grouping};
use jadpo_sql::aggregate;
use jadpo_sql::form::{self, Arg, Held};
use jadpo_std::{Builtin, Op};

/// One entry's SQL: what the grouped rows select for it, each with the
/// name it is selected as; what the outer statement selects from those,
/// the first part's value what its first column becomes; the expression
/// it is grouped by, for a group; and how rows sort by it, of the grouped
/// rows' names.
struct Selected {
    inner: Vec<(String, String)>,
    outer: Vec<(String, String)>,
    column: Column,
    /// For a group by a field, and a field's `min` or `max`: a stored
    /// value of the field, held to its rules when read (chunk 5.9d). A
    /// count, a sum, an average and a bucket are worked out, and hold none.
    held: Option<FieldRules>,
    group: Option<String>,
    sort: String,
}

impl Planner<'_> {
    /// An aggregate query's plan.
    pub(super) fn aggregate(
        &mut self,
        query: &QueryModel,
        grouping: &Grouping,
    ) -> Option<QueryPlan> {
        let entity = self.model.entity(&query.entity)?;
        let store = entity.store.clone()?;
        let mut statement = Statement::default();
        let mut selected = Vec::new();
        for (name, entry) in &grouping.entries {
            selected.push(self.entry(&mut statement, entity, name, entry)?);
        }
        let mut conditions: Vec<String> = Vec::new();
        for line in &query.filter {
            conditions.push(jadpo_sql::condition(&mut statement, entity, line, false)?);
        }
        conditions.extend(self.access(&mut statement, entity, Reach::Read)?);
        let policy = self.policy(&mut statement, entity, Effect::Read, &Row::Table)?;
        let limit = match &query.limit {
            Some(Bound::Literal(n)) => Some(n.to_string()),
            Some(Bound::Value(value)) => {
                let source = self.source(value)?;
                Some(statement.param(source, SqlType::Int8))
            }
            None => None,
        };
        // The grouped rows' columns, then the outer statement's, each
        // entry's parts in turn, and where each entry's value starts.
        let inner: Vec<(String, String)> = selected
            .iter()
            .flat_map(|s| s.inner.iter().cloned())
            .collect();
        let mut rows: Vec<(String, String)> = Vec::new();
        let mut kinds: Vec<Column> = Vec::new();
        let mut held: Vec<Option<FieldRules>> = Vec::new();
        let mut starts: Vec<u32> = Vec::new();
        for entry in &selected {
            starts.push(u32::try_from(rows.len()).ok()?);
            for (at, part) in entry.outer.iter().enumerate() {
                rows.push(part.clone());
                kinds.push(if at == 0 {
                    entry.column.clone()
                } else {
                    Column::Part
                });
                held.push(if at == 0 { entry.held.clone() } else { None });
            }
        }
        let groups: Vec<String> = selected.iter().filter_map(|s| s.group.clone()).collect();
        let order: Vec<(String, bool)> = grouping
            .order
            .iter()
            .filter_map(|o| Some((selected.get(o.entry)?.sort.clone(), o.descending)))
            .collect();
        let write = |conditions: &[String]| {
            text::grouped(
                &inner,
                &entity.table,
                conditions,
                &groups,
                &rows,
                &order,
                limit.as_deref(),
            )
        };
        let (sql, unpoliced) = both(&write, conditions, policy);
        let mut fields = Vec::new();
        for (name, source) in &query.element.fields {
            let model::Source::Entry { at, .. } = source else {
                return None;
            };
            fields.push((name.clone(), Field::Column(*starts.get(*at)?)));
        }
        let caller = query
            .params
            .iter()
            .position(|(_, ty)| crate::holds_caller_ty(ty))
            .and_then(|at| u32::try_from(at).ok());
        Some(QueryPlan {
            key: query.key.clone(),
            name: query.name.clone(),
            caller,
            read: ReadPlan {
                store,
                settings: Vec::new(),
                cardinality: query.cardinality,
                select: Select {
                    sql,
                    unpoliced,
                    permitted: None,
                    params: statement.into_params(),
                    columns: kinds,
                    held,
                    record: Record {
                        ty: query.element.ty.clone(),
                        fields,
                    },
                    includes: Vec::new(),
                },
                page: None,
                missing: None,
                recheck: None,
                layer: false,
            },
        })
    }

    /// One group's or aggregate's SQL.
    fn entry(
        &mut self,
        statement: &mut Statement,
        entity: &EntityModel,
        name: &str,
        entry: &Entry,
    ) -> Option<Selected> {
        let alias = jadpo_sql::ident(name);
        let field = match entry.field() {
            Some(field_name) => Some(field(entity, field_name)?),
            None => None,
        };
        let column = field.map(|f| jadpo_sql::ident(&f.born));
        // Most entries are one expression, selected by name and passed
        // out as it is.
        let one = |expr: String, kind: Column, group: bool| Selected {
            inner: vec![(expr.clone(), name.to_string())],
            outer: vec![(alias.clone(), name.to_string())],
            column: kind,
            held: None,
            group: group.then_some(expr),
            sort: alias.clone(),
        };
        Some(match entry {
            Entry::Group(GroupKey::Field(_)) => {
                let (field, column) = (field?, column?);
                let kind = self.column(entity, field)?;
                let sort = match &kind {
                    Column::Text => aggregate::text_order(&alias),
                    Column::Enum { ty, variants } => {
                        let codes: Vec<&str> = variants
                            .iter()
                            .map(|variant| jadpo_std::standard::stored_variant(ty, variant))
                            .collect();
                        aggregate::enum_order(&alias, &codes)
                    }
                    _ => alias.clone(),
                };
                let held =
                    (!matches!(kind, Column::Json(_))).then(|| self.held_field(entity, field));
                Selected {
                    sort,
                    held,
                    ..one(column, kind, true)
                }
            }
            Entry::Group(GroupKey::Bucket { op, .. }) => {
                let (field, column) = (field?, column?);
                let builtin = field.builtin.unwrap_or(Builtin::CalendarDate);
                let form = form::operation(*op, &[Arg::Value(&column, Held::Builtin(builtin))])?;
                one(form.value()?.to_string(), Column::Int, true)
            }
            Entry::Aggregate {
                kind: AggregateKind::Count,
                ..
            } => one(aggregate::count(), Column::Int, false),
            Entry::Aggregate {
                kind: AggregateKind::Sum,
                ..
            } => {
                let (field, column) = (field?, column?);
                let kind = self.column(entity, field)?;
                match &kind {
                    Column::Int | Column::Duration => {
                        one(aggregate::sum_whole(&column), kind, false)
                    }
                    Column::Decimal => one(
                        aggregate::sum_at(&column, field.scale.unwrap_or(0)),
                        kind,
                        false,
                    ),
                    Column::Quantity(quantity) => {
                        one(aggregate::sum_at(&column, quantity.scale()), kind, false)
                    }
                    // Money: its minor units added up, and the currency its
                    // type fixes.
                    Column::Compound { .. } => {
                        let (_, code) = field.fixed.iter().find(|(part, _)| *part == "currency")?;
                        let amount = jadpo_sql::ident(&field.columns().first()?.0);
                        let currency = statement.stored(jadpo_std::SqlValue::Text(code.clone()))?;
                        let part = format!("{name}_currency");
                        Selected {
                            inner: vec![
                                (aggregate::sum_whole(&amount), name.to_string()),
                                (aggregate::as_text(&currency), part.clone()),
                            ],
                            outer: vec![
                                (alias.clone(), name.to_string()),
                                (jadpo_sql::ident(&part), part),
                            ],
                            column: Column::Compound {
                                ty: "Money".to_string(),
                                may_be_none: false,
                            },
                            held: None,
                            group: None,
                            sort: alias.clone(),
                        }
                    }
                    _ => return None,
                }
            }
            Entry::Aggregate {
                kind: kind @ (AggregateKind::Min | AggregateKind::Max),
                ..
            } => {
                let (field, column) = (field?, column?);
                let read = self.column(entity, field)?;
                let max = *kind == AggregateKind::Max;
                let text = read == Column::Text;
                let held =
                    (!matches!(read, Column::Json(_))).then(|| self.held_field(entity, field));
                Selected {
                    held,
                    ..one(aggregate::extreme(&column, max, text), read, false)
                }
            }
            // The sum at `scale:` places and how many values there are,
            // grouped; outside, `decimal.divide`'s form of the one by the
            // other, `NULL` when there are no values, so it never divides
            // by zero.
            Entry::Aggregate {
                kind: AggregateKind::Average { scale, rounding },
                ..
            } => {
                let column = column?;
                let (sum, count) = (format!("{name}_sum"), format!("{name}_count"));
                let dividend = jadpo_sql::ident(&sum);
                let divisor = aggregate::nonzero(&count);
                let form = form::operation(
                    Op::DecimalDivide,
                    &[
                        Arg::Value(&dividend, Held::Builtin(Builtin::Decimal)),
                        Arg::Value(&divisor, Held::Builtin(Builtin::Int)),
                        Arg::Rounding(*rounding),
                    ],
                )?;
                Selected {
                    inner: vec![
                        (aggregate::sum_rounded(&column, *scale), sum),
                        (aggregate::count_of(&column), count),
                    ],
                    outer: vec![(form.value()?.to_string(), name.to_string())],
                    column: Column::Decimal,
                    held: None,
                    group: None,
                    sort: alias.clone(),
                }
            }
        })
    }
}
