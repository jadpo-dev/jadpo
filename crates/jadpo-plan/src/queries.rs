//! The SQL each query, `get` and `create` runs on PostgreSQL (area 6;
//! docs/compiler-runtime.md §2.1: "SQL per engine with policy compiled
//! in"), as [`jadpo_std::read`] plans both executors run.
//!
//! - A query is one `SELECT` of only the columns its result needs, plus its
//!   order's columns when it pages and the keys its includes match;
//!   conditions keep Jadpo's meaning where a field is `none` (it meets no
//!   comparison but `!=`), with `not` pushed down to the comparisons so
//!   SQL's three-valued logic never shows; `where_present:` and a page's
//!   cursor are `($n IS NULL OR ...)`, so the text never changes with the
//!   arguments.
//! - Order is the written fields with `id` added as the tie-break; text
//!   sorts by code point (`COLLATE "C"`), as Jadpo compares it. A page is
//!   keyset pagination: the rows after the cursor's values, one more than
//!   the page's size to know whether another page follows.
//! - Each include is one more statement however many records it is
//!   included in: a parent or a unique child by `= ANY($1)` of the keys,
//!   children through a `LATERAL` subquery that orders and limits each
//!   record's list. Never one statement per record.
//! - The conditions an entity's lifecycle (`visible:`, chunk 3.4) and
//!   policy (chunk 4.4) add join every statement's `WHERE` at one place,
//!   [`Planner::access`], which adds none yet.

use jadpo_check::data::queries::{
    self as model, Bound, Compare, Condition, Include, IncludeKind, OrderField, QueryModel,
    Source as From, Value,
};
use jadpo_check::data::{DataModel, EntityModel, FieldModel};
use jadpo_check::lookups::Lookup;
use jadpo_check::types::{Const, Def, Prim, Ty, TypeView};
use jadpo_graph::{Key, Kind};
use jadpo_std::read::{
    Cardinality, Column, CreatePlan, Field, Given, IncludeRead, IncludeShape, PageRead, Param,
    ReadPlan, Record, Select, Size, Source, SqlType,
};

use crate::constant::Constant;
use crate::types::{FailureId, Types};

/// A query's plan.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueryPlan {
    pub key: Key,
    pub name: String,
    pub read: ReadPlan,
    /// For a query returning exactly one record: the failure when nothing
    /// matches.
    pub missing: Option<MissingPlan>,
}

/// The failure a read raises when nothing matches, with its fields.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MissingPlan {
    pub failure: FailureId,
    /// Each field given: its place in the failure's fields, and its value.
    pub fields: Vec<(Option<u32>, MissingValue)>,
}

/// Where a `missing:` failure's field gets its value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MissingValue {
    /// The read's argument, or a field inside it.
    Arg {
        index: u32,
        path: Vec<String>,
    },
    Constant(Constant),
    Now,
}

/// What an entity's operations run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntityPlan {
    pub key: Key,
    pub name: String,
    /// `Entity.get(id)`.
    pub get: ReadPlan,
    /// The not-found failure `get` raises, each of its fields given the
    /// `id` it looked for; `None` when the domain declares none, which the
    /// checker reports.
    pub not_found: Option<MissingPlan>,
    /// `Entity.create`, given its settable fields in [`EntityPlan::settable`]'s
    /// order.
    pub create: CreatePlan,
    /// The fields a create gives, in declared order: all but `id` and the
    /// generated ones.
    pub settable: Vec<Key>,
}

/// Builds the plans of a project's queries and entities.
pub(crate) struct Planner<'a> {
    pub model: &'a DataModel,
    pub lookup: &'a Lookup<'a>,
    pub types: &'a mut Types,
}

/// One statement being written: its parameters, deduplicated, so a value
/// used twice is sent once.
#[derive(Default)]
struct Statement {
    params: Vec<Param>,
}

impl Statement {
    /// `$n` for `source` sent as `ty`.
    fn param(&mut self, source: Source, ty: SqlType) -> String {
        let param = Param { source, ty };
        let at = match self.params.iter().position(|p| *p == param) {
            Some(at) => at,
            None => {
                self.params.push(param);
                self.params.len() - 1
            }
        };
        format!("${}", at + 1)
    }
}

/// The columns a statement selects, each a field of its entity once, with
/// what it becomes.
#[derive(Default)]
struct Columns {
    names: Vec<String>,
    kinds: Vec<Column>,
}

impl Columns {
    fn add(&mut self, field: &str, kind: Column) -> u32 {
        let at = match self.names.iter().position(|n| n == field) {
            Some(at) => at,
            None => {
                self.names.push(field.to_string());
                self.kinds.push(kind);
                self.names.len() - 1
            }
        };
        at as u32
    }
}

impl Planner<'_> {
    // --- queries -----------------------------------------------------------------

    pub fn query(&mut self, query: &QueryModel) -> Option<QueryPlan> {
        let entity = self.model.entity(&query.entity)?;
        let store = entity.store.clone()?;
        let cardinality = match query.cardinality {
            model::Cardinality::One => Cardinality::One,
            model::Cardinality::Optional => Cardinality::Optional,
            model::Cardinality::List => Cardinality::List,
            model::Cardinality::Page => Cardinality::Page,
        };
        let page = query.cardinality == model::Cardinality::Page;
        let mut statement = Statement::default();
        let mut columns = Columns::default();
        // The record's own columns first, in its type's order.
        let record = self.record(entity, &query.element, &query.includes, &mut columns)?;
        let mut cursor = Vec::new();
        if page {
            for order in &query.order {
                let field = field(entity, &order.field)?;
                let kind = self.column(entity, field)?;
                let at = columns.add(&field.name, kind);
                cursor.push((at, column_type(field)?.param()));
            }
        }
        let mut conditions: Vec<String> = Vec::new();
        for line in &query.filter {
            conditions.push(self.condition(&mut statement, entity, line, false)?);
        }
        conditions.extend(self.access(entity));
        let size = match &query.limit {
            Some(Bound::Value(value)) => Some(self.source(value)?),
            _ => None,
        };
        let after = match &query.after {
            Some(value) => Some(self.source(value)?),
            None => None,
        };
        if page {
            conditions.push(self.after(&mut statement, entity, &query.order)?);
        }
        let mut sql = format!(
            "SELECT {}\nFROM {}",
            columns
                .names
                .iter()
                .map(|f| quoted(&born(entity, f)))
                .collect::<Vec<_>>()
                .join(", "),
            quoted(&entity.table)
        );
        where_clause(&mut sql, &conditions, "");
        if !query.order.is_empty() {
            sql.push_str(&format!(
                "\nORDER BY {}",
                self.order_by(entity, &query.order, "")?
            ));
        }
        match (&query.limit, page) {
            (Some(Bound::Literal(n)), true) => sql.push_str(&format!("\nLIMIT {}", n + 1)),
            (Some(Bound::Value(_)), true) => {
                let n = statement.param(Source::SizePlusOne, SqlType::Int8);
                sql.push_str(&format!("\nLIMIT {n}"));
            }
            (Some(Bound::Literal(n)), false) => sql.push_str(&format!("\nLIMIT {n}")),
            (Some(Bound::Value(value)), false) => {
                let source = self.source(value)?;
                let n = statement.param(source, SqlType::Int8);
                sql.push_str(&format!("\nLIMIT {n}"));
            }
            (None, _) => {}
        }
        let includes = self.includes(entity, &query.includes, &columns)?;
        let select = Select {
            sql,
            params: statement.params,
            columns: columns.kinds,
            record,
            includes,
        };
        let page = if page {
            let size = match (&query.limit, size) {
                (_, Some(source)) => Size::From(source),
                (Some(Bound::Literal(n)), None) => Size::Fixed(*n),
                _ => return None,
            };
            Some(PageRead {
                size,
                after: after?,
                cursor,
                tag: tag(&query.key),
            })
        } else {
            None
        };
        let missing = match &query.missing {
            Some(missing) => Some(self.missing(missing)?),
            None => None,
        };
        Some(QueryPlan {
            key: query.key.clone(),
            name: query.name.clone(),
            read: ReadPlan {
                store,
                cardinality,
                select,
                page,
            },
            missing,
        })
    }

    /// The record a row becomes: each field from its column, or from an
    /// include, and the include keys selected.
    fn record(
        &mut self,
        entity: &EntityModel,
        element: &model::Element,
        includes: &[Include],
        columns: &mut Columns,
    ) -> Option<Record> {
        let mut fields = Vec::new();
        for (name, source) in &element.fields {
            match source {
                From::Column(column) => {
                    let field = field(entity, column)?;
                    let kind = self.column(entity, field)?;
                    let at = columns.add(&field.name, kind);
                    fields.push((name.clone(), Field::Column(at)));
                }
                From::Include(at) => fields.push((name.clone(), Field::Include(*at as u32))),
            }
        }
        // The keys the includes match.
        for include in includes {
            let key = match include.kind {
                IncludeKind::Parent => include.via.as_str(),
                IncludeKind::Children | IncludeKind::Child => "id",
            };
            let field = field(entity, key)?;
            let kind = self.column(entity, field)?;
            columns.add(&field.name, kind);
        }
        Some(Record {
            ty: element.ty.clone(),
            fields,
        })
    }

    /// One statement per include, matching the including statement's keys.
    fn includes(
        &mut self,
        entity: &EntityModel,
        includes: &[Include],
        outer: &Columns,
    ) -> Option<Vec<IncludeRead>> {
        let mut out = Vec::new();
        for include in includes {
            let target = self.model.entity(&include.entity)?;
            let (key_field, shape) = match include.kind {
                IncludeKind::Parent => (
                    include.via.as_str(),
                    if include.optional {
                        IncludeShape::Optional
                    } else {
                        IncludeShape::One
                    },
                ),
                IncludeKind::Child => ("id", IncludeShape::Optional),
                IncludeKind::Children => ("id", IncludeShape::List),
            };
            let key = outer.names.iter().position(|n| n == key_field)? as u32;
            let parent_key = field(entity, key_field)?;
            let key_type = column_type(parent_key)?.param();
            let mut statement = Statement::default();
            let keys = statement.param(Source::Keys, key_type.array());
            let mut columns = Columns::default();
            // The column each row matches comes first.
            let match_field = match include.kind {
                IncludeKind::Parent => "id",
                _ => include.via.as_str(),
            };
            let matched = field(target, match_field)?;
            columns.add(&matched.name, self.column(target, matched)?);
            let record = self.record(target, &include.element, &include.includes, &mut columns)?;
            let sql = match include.kind {
                IncludeKind::Parent | IncludeKind::Child => {
                    let mut conditions = vec![format!("{} = ANY({keys})", quoted(&matched.born))];
                    for line in &include.filter {
                        conditions.push(self.condition(&mut statement, target, line, false)?);
                    }
                    conditions.extend(self.access(target));
                    let mut sql = format!(
                        "SELECT {}\nFROM {}",
                        columns
                            .names
                            .iter()
                            .map(|f| quoted(&born(target, f)))
                            .collect::<Vec<_>>()
                            .join(", "),
                        quoted(&target.table)
                    );
                    where_clause(&mut sql, &conditions, "");
                    sql
                }
                IncludeKind::Children => {
                    // Each record's list ordered and limited on its own.
                    for order in &include.order {
                        let f = field(target, &order.field)?;
                        columns.add(&f.name, self.column(target, f)?);
                    }
                    let mut conditions =
                        vec![format!("{} = \"key\".\"id\"", quoted(&matched.born))];
                    for line in &include.filter {
                        conditions.push(self.condition(&mut statement, target, line, false)?);
                    }
                    conditions.extend(self.access(target));
                    let inner_columns: Vec<String> = columns
                        .names
                        .iter()
                        .map(|f| quoted(&born(target, f)))
                        .collect();
                    let mut inner = format!(
                        "  SELECT {}\n  FROM {}",
                        inner_columns.join(", "),
                        quoted(&target.table)
                    );
                    where_clause(&mut inner, &conditions, "  ");
                    inner.push_str(&format!(
                        "\n  ORDER BY {}",
                        self.order_by(target, &include.order, "")?
                    ));
                    match &include.limit {
                        Some(Bound::Literal(n)) => inner.push_str(&format!("\n  LIMIT {n}")),
                        Some(Bound::Value(value)) => {
                            let source = self.source(value)?;
                            let n = statement.param(source, SqlType::Int8);
                            inner.push_str(&format!("\n  LIMIT {n}"));
                        }
                        None => return None,
                    }
                    // The outer statement gives the key, then each of the
                    // inner columns but the first, which is the key again.
                    let outer_columns: Vec<String> = std::iter::once("\"key\".\"id\"".to_string())
                        .chain(
                            columns
                                .names
                                .iter()
                                .skip(1)
                                .map(|f| format!("\"row\".{}", quoted(&born(target, f)))),
                        )
                        .collect();
                    format!(
                        "SELECT {}\nFROM unnest({keys}) AS \"key\"(\"id\")\nCROSS JOIN LATERAL (\n{inner}\n) AS \"row\"\nORDER BY \"key\".\"id\", {}",
                        outer_columns.join(", "),
                        self.order_by(target, &include.order, "\"row\".")?
                    )
                }
            };
            let includes = self.includes(target, &include.includes, &columns)?;
            out.push(IncludeRead {
                shape,
                key,
                select: Select {
                    sql,
                    params: statement.params,
                    columns: columns.kinds,
                    record,
                    includes,
                },
            });
        }
        Some(out)
    }

    /// The conditions an entity's lifecycle and policy add to every
    /// statement that reads it. Lifecycles (`visible:`) come with chunk
    /// 3.4 and policy grants with chunk 4.4, which compile them here, so
    /// every query, include and `get` gets them at once.
    fn access(&self, _entity: &EntityModel) -> Vec<String> {
        Vec::new()
    }

    // --- conditions ----------------------------------------------------------------

    /// A condition as SQL, `negated` when a `not` above it was pushed
    /// down.
    fn condition(
        &mut self,
        statement: &mut Statement,
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
                let rendered = self.value(statement, value, ty)?;
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
                            rendered.push(self.value(statement, item, ty)?);
                        }
                        format!(
                            "{column} {}IN ({})",
                            if negated { "NOT " } else { "" },
                            rendered.join(", ")
                        )
                    }
                    other => {
                        let source = self.source(other)?;
                        let n = statement.param(source, ty.array());
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
                    rendered.push(self.condition(statement, entity, part, negated)?);
                }
                format!("({})", rendered.join(if and { " AND " } else { " OR " }))
            }
            Condition::Not(inner) => {
                return self.condition(statement, entity, inner, !negated);
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
                let source = self.source(value)?;
                let n = statement.param(source, ty);
                let inner = self.condition(statement, entity, condition, negated)?;
                format!("({n} IS NULL OR {inner})")
            }
        })
    }

    /// A value as SQL: a literal written into the text, anything else a
    /// parameter.
    fn value(&mut self, statement: &mut Statement, value: &Value, ty: SqlType) -> Option<String> {
        Some(match value {
            Value::Literal(constant) => literal(constant, ty)?,
            Value::Variant { variant, .. } => text_literal(variant),
            Value::List(_) => return None,
            other => {
                let source = self.source(other)?;
                statement.param(source, ty)
            }
        })
    }

    /// Where a value that isn't written into the text comes from.
    fn source(&self, value: &Value) -> Option<Source> {
        Some(match value {
            Value::Param { index, path } => Source::Arg {
                index: *index,
                path: path.clone(),
            },
            Value::Now => Source::Now,
            _ => return None,
        })
    }

    /// The condition that starts a page after its cursor: the rows past
    /// the cursor's values of the order fields, or every row when there is
    /// no cursor.
    fn after(
        &mut self,
        statement: &mut Statement,
        entity: &EntityModel,
        order: &[OrderField],
    ) -> Option<String> {
        let mut parts = Vec::new();
        for (i, order) in order.iter().enumerate() {
            let field = field(entity, &order.field)?;
            let ty = column_type(field)?.param();
            let n = statement.param(Source::Cursor(i as u32), ty);
            let collate = if self.is_text(entity, field) {
                " COLLATE \"C\""
            } else {
                ""
            };
            parts.push((
                format!("{}{collate}", quoted(&field.born)),
                format!("{n}{collate}"),
                order.descending,
                n,
            ));
        }
        let first = parts.first()?.3.clone();
        let same = parts.iter().all(|p| p.2 == parts[0].2);
        let past = if same {
            let op = if parts[0].2 { "<" } else { ">" };
            if parts.len() == 1 {
                format!("{} {op} {}", parts[0].0, parts[0].1)
            } else {
                format!(
                    "({}) {op} ({})",
                    parts
                        .iter()
                        .map(|p| p.0.clone())
                        .collect::<Vec<_>>()
                        .join(", "),
                    parts
                        .iter()
                        .map(|p| p.1.clone())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
        } else {
            // Mixed directions: past on the first field, or equal on it and
            // past on the next, and so on.
            let mut alternatives = Vec::new();
            for i in 0..parts.len() {
                let mut terms: Vec<String> = parts[..i]
                    .iter()
                    .map(|p| format!("{} = {}", p.0, p.1))
                    .collect();
                let op = if parts[i].2 { "<" } else { ">" };
                terms.push(format!("{} {op} {}", parts[i].0, parts[i].1));
                alternatives.push(if terms.len() == 1 {
                    terms.remove(0)
                } else {
                    format!("({})", terms.join(" AND "))
                });
            }
            format!("({})", alternatives.join(" OR "))
        };
        Some(format!("({first} IS NULL OR {past})"))
    }

    fn order_by(&self, entity: &EntityModel, order: &[OrderField], prefix: &str) -> Option<String> {
        let mut out = Vec::new();
        for order in order {
            let field = field(entity, &order.field)?;
            let collate = if self.is_text(entity, field) {
                " COLLATE \"C\""
            } else {
                ""
            };
            out.push(format!(
                "{prefix}{}{collate} {}",
                quoted(&field.born),
                if order.descending { "DESC" } else { "ASC" }
            ));
        }
        Some(out.join(", "))
    }

    fn is_text(&self, entity: &EntityModel, field: &FieldModel) -> bool {
        self.column(entity, field) == Some(Column::Text)
    }

    /// What a field's column becomes.
    fn column(&self, entity: &EntityModel, field: &FieldModel) -> Option<Column> {
        let ty = Ty::Field(entity.name.clone(), vec![field.name.clone()]);
        if let Some(prim) = self.lookup.representation(&ty) {
            return Some(match prim {
                Prim::Bool => Column::Bool,
                Prim::Int => Column::Int,
                Prim::Decimal => Column::Decimal,
                Prim::Text => Column::Text,
                Prim::Bytes => Column::Bytes,
                Prim::Uuid => Column::Uuid,
                Prim::Instant => Column::Instant,
                Prim::CalendarDate => Column::Date,
                Prim::Time => Column::Time,
                Prim::Duration => Column::Duration,
                _ => return None,
            });
        }
        match self.lookup.structure(&ty) {
            Ty::Named(name) => match self.lookup.def(&name) {
                Some(Def::Enum { variants }) if variants.iter().all(|v| v.fields.is_none()) => {
                    Some(Column::Enum {
                        ty: name.clone(),
                        variants: variants.iter().map(|v| v.name.clone()).collect(),
                    })
                }
                _ => None,
            },
            _ => None,
        }
    }

    /// A `missing:` failure with its fields.
    fn missing(&mut self, missing: &model::Missing) -> Option<MissingPlan> {
        let key = self.types.named(&missing.failure)?;
        let failure = self.types.failure_id(&key)?;
        let mut fields = Vec::new();
        for (name, value) in &missing.fields {
            let field_key = self
                .types
                .graph()
                .named(Kind::Field, &format!("{}.{name}", missing.failure))
                .map(|id| self.types.graph().node(id).key.clone())?;
            let slot = self
                .types
                .failure(failure)
                .and_then(|plan| plan.slot(&field_key));
            let value = match value {
                Value::Param { index, path } => MissingValue::Arg {
                    index: *index,
                    path: path.clone(),
                },
                Value::Now => MissingValue::Now,
                Value::Literal(constant) => {
                    let literal = constant.literal()?;
                    MissingValue::Constant(
                        self.types
                            .literal(&literal, &jadpo_graph::TypeRef::Field(field_key))?,
                    )
                }
                Value::Variant { ty, variant } => {
                    let shape = self
                        .types
                        .shape_plan(&self.types.variant_named(ty, variant))?;
                    let plan = shape.variant?;
                    MissingValue::Constant(Constant::Variant {
                        ty: ty.clone(),
                        ordinal: plan.ordinal,
                        name: plan.name,
                        fields: Vec::new(),
                    })
                }
                Value::List(_) => return None,
            };
            fields.push((slot, value));
        }
        Some(MissingPlan { failure, fields })
    }

    // --- entities -----------------------------------------------------------------

    pub fn entity(&mut self, entity: &EntityModel) -> Option<EntityPlan> {
        let store = entity.store.clone()?;
        let key = self.types.named(&entity.name)?;
        // `get`: every field, by `id`.
        let mut columns = Columns::default();
        let mut fields = Vec::new();
        for field in &entity.fields {
            let kind = self.column(entity, field)?;
            let at = columns.add(&field.name, kind);
            fields.push((field.name.clone(), Field::Column(at)));
        }
        let mut statement = Statement::default();
        let id = statement.param(
            Source::Arg {
                index: 0,
                path: Vec::new(),
            },
            SqlType::Uuid,
        );
        let mut conditions = vec![format!("\"id\" = {id}")];
        conditions.extend(self.access(entity));
        let mut sql = format!(
            "SELECT {}\nFROM {}",
            columns
                .names
                .iter()
                .map(|f| quoted(&born(entity, f)))
                .collect::<Vec<_>>()
                .join(", "),
            quoted(&entity.table)
        );
        where_clause(&mut sql, &conditions, "");
        let get = ReadPlan {
            store: store.clone(),
            cardinality: Cardinality::One,
            select: Select {
                sql,
                params: statement.params,
                columns: columns.kinds,
                record: Record {
                    ty: entity.name.clone(),
                    fields: fields.clone(),
                },
                includes: Vec::new(),
            },
            page: None,
        };
        let not_found = self.not_found(entity);
        // `create`: every column, from the fields given, a new identity and
        // the clock.
        let mut params = Vec::new();
        let mut names = Vec::new();
        let mut settable = Vec::new();
        for field in &entity.fields {
            let ty = column_type(field)?.param();
            let given = if field.name == "id" {
                Given::NewId
            } else if field.generated.is_some() {
                Given::Now
            } else {
                let at = settable.len() as u32;
                let field_key = self
                    .types
                    .graph()
                    .named(Kind::Field, &format!("{}.{}", entity.name, field.name))
                    .map(|id| self.types.graph().node(id).key.clone())?;
                settable.push(field_key);
                Given::Set(at)
            };
            params.push((given, ty));
            names.push(quoted(&field.born));
        }
        let placeholders: Vec<String> = (1..=names.len()).map(|n| format!("${n}")).collect();
        let create = CreatePlan {
            store,
            sql: format!(
                "INSERT INTO {} ({})\nVALUES ({})",
                quoted(&entity.table),
                names.join(", "),
                placeholders.join(", ")
            ),
            params,
            record: Record {
                ty: entity.name.clone(),
                fields: (0..entity.fields.len() as u32)
                    .zip(&entity.fields)
                    .map(|(at, field)| (field.name.clone(), Field::Column(at)))
                    .collect(),
            },
        };
        Some(EntityPlan {
            key,
            name: entity.name.clone(),
            get,
            not_found,
            create,
            settable,
        })
    }

    /// `<Entity>NotFound`, each field typed `<Entity>.id` given the `id`
    /// looked for.
    fn not_found(&mut self, entity: &EntityModel) -> Option<MissingPlan> {
        let name = format!("{}NotFound", entity.name);
        let key = self.types.named(&name)?;
        let failure = self.types.failure_id(&key)?;
        let plan = self.types.failure(failure)?.clone();
        let fields = plan
            .fields
            .iter()
            .enumerate()
            .map(|(at, _)| {
                (
                    Some(at as u32),
                    MissingValue::Arg {
                        index: 0,
                        path: Vec::new(),
                    },
                )
            })
            .collect();
        Some(MissingPlan { failure, fields })
    }
}

// --- text -----------------------------------------------------------------------

fn field<'e>(entity: &'e EntityModel, name: &str) -> Option<&'e FieldModel> {
    entity.fields.iter().find(|f| f.name == name)
}

/// A field's column name: its born name, which survives a rename.
fn born(entity: &EntityModel, name: &str) -> String {
    field(entity, name).map_or_else(|| name.to_string(), |f| f.born.clone())
}

fn column_type(field: &FieldModel) -> Option<SqlType> {
    SqlType::of_column(field.sql_type.as_deref()?)
}

/// An identifier, always quoted: a table called `user` or `order` is a
/// keyword unquoted.
pub fn quoted(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

fn text_literal(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}

/// A literal written into a statement for a column of type `ty`.
fn literal(constant: &Const, ty: SqlType) -> Option<String> {
    Some(match constant {
        Const::Str(text) => text_literal(text),
        Const::Number { value, .. } => value.to_string(),
        Const::Bool(true) => "TRUE".to_string(),
        Const::Bool(false) => "FALSE".to_string(),
        Const::Duration(written) if ty == SqlType::Int8 => {
            jadpo_std::Dur::parse(written)?.millis.to_string()
        }
        _ => return None,
    })
}

fn where_clause(sql: &mut String, conditions: &[String], indent: &str) {
    for (i, condition) in conditions.iter().enumerate() {
        let word = if i == 0 { "WHERE" } else { "  AND" };
        sql.push_str(&format!("\n{indent}{word} {condition}"));
    }
}

/// Which query made a cursor: the first eight bytes of its key's digest,
/// so a cursor survives a rename of the query but not another query.
fn tag(key: &Key) -> u64 {
    let digest = blake3::hash(key.as_str().as_bytes());
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&digest.as_bytes()[..8]);
    u64::from_be_bytes(bytes)
}

/// Every query's plan and every entity's, by key.
pub(crate) fn plans(
    db: &dyn jadpo_db::Db,
    project: jadpo_db::Project,
    types: &mut Types,
) -> (Vec<QueryPlan>, Vec<EntityPlan>) {
    let model = jadpo_check::data::data_model(db, project);
    let queries = jadpo_check::data::queries::query_models(db, project);
    let lookup = Lookup::new(db, project);
    let mut planner = Planner {
        model,
        lookup: &lookup,
        types,
    };
    let mut query_plans: Vec<QueryPlan> = queries
        .queries
        .iter()
        .filter_map(|query| planner.query(query))
        .collect();
    query_plans.sort_by(|a, b| a.key.cmp(&b.key));
    let mut entity_plans: Vec<EntityPlan> = model
        .entities
        .iter()
        .filter_map(|entity| planner.entity(entity))
        .collect();
    entity_plans.sort_by(|a, b| a.key.cmp(&b.key));
    (query_plans, entity_plans)
}
