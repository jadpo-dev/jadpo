//! The SQL each query and `get` runs on PostgreSQL (area 6;
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
    self as model, Bound, Condition, Include, IncludeKind, OrderField, QueryModel, Source as From,
    Value,
};
use jadpo_check::data::{DataModel, EntityModel, FieldModel};
use jadpo_check::lookups::Lookup;
use jadpo_check::types::{Def, Prim, Ty, TypeView};
use jadpo_graph::{Key, Kind};
use jadpo_sql::Bind;
use jadpo_sql::statement::{self as text, After, Order};
use jadpo_std::read::{
    Cardinality, Column, Field, IncludeRead, IncludeShape, MissingPlan, MissingValue, PageRead,
    Param, ReadPlan, Record, Select, Size, Source, SqlType,
};

use crate::constant::Constant;
use crate::policy::Row;
use crate::types::Types;
use jadpo_check::policy::Effect;

/// A query's plan: its read, with the failure when a query for exactly
/// one record finds none (`ReadPlan::missing`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueryPlan {
    pub key: Key,
    pub name: String,
    pub read: ReadPlan,
    /// The parameter whose type is a `Principal`, when it has one: the
    /// caller a test's, fixture's or seed's call of it runs as (area 9).
    pub caller: Option<u32>,
}

/// What an entity's operations run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntityPlan {
    pub key: Key,
    pub name: String,
    /// `Entity.get(id)`, with the not-found failure it raises, each of its
    /// fields given the `id` it looked for: the domain's, or the one the
    /// compiler provides.
    pub get: ReadPlan,
}

/// How a statement reaches an entity, which decides what [`Planner::access`]
/// adds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Reach {
    /// A query, `get`, or the children an include reads.
    Read,
    /// The record a reference points at, through an include.
    Parent,
    /// An update, a delete or a transition.
    Write,
}

/// Builds the plans of a project's queries and entities.
pub(crate) struct Planner<'a> {
    pub model: &'a DataModel,
    pub lookup: &'a Lookup<'a>,
    pub types: &'a mut Types,
    /// Every entity's policy (area 9).
    pub access: &'a jadpo_check::policy::Access,
    /// The action whose writes are being planned, whose line under root
    /// `policy { actions: }` replaces its entities' grants on them.
    pub action: Option<String>,
}

/// One statement being written: its parameters, deduplicated, so a value
/// used twice is sent once.
#[derive(Default)]
pub(crate) struct Statement {
    params: Vec<Param>,
}

impl Statement {
    /// Its parameters, in order.
    pub(crate) fn into_params(self) -> Vec<Param> {
        self.params
    }

    /// `$n` for `source` sent as `ty`.
    pub(crate) fn param(&mut self, source: Source, ty: SqlType) -> String {
        let param = Param { source, ty };
        let at = match self.params.iter().position(|p| *p == param) {
            Some(at) => at,
            None => {
                self.params.push(param);
                self.params.len() - 1
            }
        };
        jadpo_sql::placeholder(at)
    }
}

impl Bind for Statement {
    fn bind(&mut self, value: &Value, ty: SqlType) -> Option<String> {
        let source = match value {
            Value::Param { index, path } => Source::Arg {
                index: *index,
                path: path.clone(),
            },
            Value::Now => Source::Now,
            _ => return None,
        };
        Some(self.param(source, ty))
    }
}

/// The columns a statement selects, each a field of its entity once, with
/// what it becomes.
#[derive(Default)]
struct Columns {
    names: Vec<String>,
    kinds: Vec<Column>,
}

/// How a later column of a compound standard type (`Money`, area 19) is
/// named among a statement's columns, after its field's name and before
/// its place among the type's columns: `total#part1`.
const PART_COLUMN: &str = "#part";

impl Columns {
    /// The field's column, added once; a compound type's every column.
    fn add(&mut self, field: &str, kind: Column) -> u32 {
        let at = match self.names.iter().position(|n| n == field) {
            Some(at) => at,
            None => {
                let at = self.names.len();
                let parts = match &kind {
                    Column::Compound(ty) => {
                        jadpo_std::standard_type(ty).map_or(1, |entry| entry.columns().len())
                    }
                    _ => 1,
                };
                self.names.push(field.to_string());
                self.kinds.push(kind);
                for part in 1..parts {
                    self.names.push(format!("{field}{PART_COLUMN}{part}"));
                    self.kinds.push(Column::Part);
                }
                at
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
        let cardinality = query.cardinality;
        let page = cardinality == Cardinality::Page;
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
        conditions.extend(self.access(&mut statement, entity, Reach::Read)?);
        let policy = self.policy(&mut statement, entity, Effect::Read, &Row::Table)?;
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
        let order = if query.order.is_empty() {
            None
        } else {
            Some(self.order(entity, &query.order)?)
        };
        let limit = match (&query.limit, page) {
            (Some(Bound::Literal(n)), true) => Some((n + 1).to_string()),
            (Some(Bound::Value(_)), true) => {
                Some(statement.param(Source::SizePlusOne, SqlType::Int8))
            }
            (Some(Bound::Literal(n)), false) => Some(n.to_string()),
            (Some(Bound::Value(value)), false) => {
                let source = self.source(value)?;
                Some(statement.param(source, SqlType::Int8))
            }
            (None, _) => None,
        };
        // The statement with its policy, and without, which a test's call
        // without a caller runs.
        let write = |conditions: &[String]| {
            let mut sql = text::select(&borns(entity, &columns.names), &entity.table, "");
            text::where_clause(&mut sql, conditions, "");
            if let Some(order) = &order {
                text::order_by(&mut sql, order, "");
            }
            if let Some(n) = &limit {
                text::limit(&mut sql, n, "");
            }
            sql
        };
        // A read of exactly one record selects its grant, so the one
        // statement tells a hidden record from one that isn't there.
        let (sql, unpoliced, permitted) = if cardinality == Cardinality::One && limit.is_none() {
            granted(
                &borns(entity, &columns.names),
                entity,
                conditions,
                policy,
                &|sql| {
                    if let Some(order) = &order {
                        text::order_by(sql, order, "");
                    }
                },
            )
        } else {
            let (sql, unpoliced) = both(&write, conditions, policy);
            (sql, unpoliced, None)
        };
        let includes = self.includes(entity, &query.includes, &columns)?;
        let mut kinds = columns.kinds;
        if permitted.is_some() {
            kinds.push(Column::Bool);
        }
        let select = Select {
            sql,
            unpoliced,
            permitted,
            params: statement.params,
            columns: kinds,
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
                cardinality,
                select,
                page,
                missing,
            },
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
            let (sql, unpoliced) = match include.kind {
                IncludeKind::Parent | IncludeKind::Child => {
                    let mut conditions = vec![text::any(&matched.born, &keys)];
                    for line in &include.filter {
                        conditions.push(self.condition(&mut statement, target, line, false)?);
                    }
                    let reach = if include.kind == IncludeKind::Parent {
                        Reach::Parent
                    } else {
                        Reach::Read
                    };
                    conditions.extend(self.access(&mut statement, target, reach)?);
                    let policy = self.policy(&mut statement, target, Effect::Read, &Row::Table)?;
                    let write = |conditions: &[String]| {
                        let mut sql =
                            text::select(&borns(target, &columns.names), &target.table, "");
                        text::where_clause(&mut sql, conditions, "");
                        sql
                    };
                    both(&write, conditions, policy)
                }
                IncludeKind::Children => {
                    // Each record's list ordered and limited on its own.
                    for order in &include.order {
                        let f = field(target, &order.field)?;
                        columns.add(&f.name, self.column(target, f)?);
                    }
                    let mut conditions = vec![text::matches_key(&matched.born)];
                    for line in &include.filter {
                        conditions.push(self.condition(&mut statement, target, line, false)?);
                    }
                    conditions.extend(self.access(&mut statement, target, Reach::Read)?);
                    let policy = self.policy(&mut statement, target, Effect::Read, &Row::Table)?;
                    let inner_columns = borns(target, &columns.names);
                    let order = self.order(target, &include.order)?;
                    let limit = match &include.limit {
                        Some(Bound::Literal(n)) => n.to_string(),
                        Some(Bound::Value(value)) => {
                            let source = self.source(value)?;
                            statement.param(source, SqlType::Int8)
                        }
                        None => return None,
                    };
                    // The outer statement gives the key, then each of the
                    // inner columns but the first, which is the key again.
                    let rest: Vec<&str> =
                        inner_columns.iter().skip(1).map(String::as_str).collect();
                    let write = |conditions: &[String]| {
                        let mut inner = text::select(&inner_columns, &target.table, "  ");
                        text::where_clause(&mut inner, conditions, "  ");
                        text::order_by(&mut inner, &order, "  ");
                        text::limit(&mut inner, &limit, "  ");
                        text::children(&keys, &rest, &inner, &order)
                    };
                    both(&write, conditions, policy)
                }
            };
            let includes = self.includes(target, &include.includes, &columns)?;
            out.push(IncludeRead {
                shape,
                key,
                select: Select {
                    sql,
                    unpoliced,
                    permitted: None,
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
    /// statement that reads or writes it, at this one place: the
    /// lifecycle's `visible:` (area 5), so a hidden record is in no query,
    /// `get`, update or delete, and policy grants with chunk 4.4. A parent
    /// a record refers to is read even when its lifecycle hides it, since
    /// the reference still holds (area 6, question 10).
    pub(crate) fn access(
        &self,
        bind: &mut dyn Bind,
        entity: &EntityModel,
        reach: Reach,
    ) -> Option<Vec<String>> {
        let mut out = Vec::new();
        if reach != Reach::Parent
            && let Some(visible) = entity.lifecycle.as_ref().and_then(|l| l.visible.as_ref())
        {
            out.push(jadpo_sql::condition(bind, entity, visible, false)?);
        }
        Some(out)
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
        jadpo_sql::condition(statement, entity, condition, negated)
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
            let param = statement.param(Source::Cursor(i as u32), ty);
            parts.push(After {
                column: &field.born,
                param,
                collate: self.is_text(entity, field),
                descending: order.descending,
            });
        }
        text::after(&parts)
    }

    /// An order's fields as their columns, text sorted by code point.
    fn order<'e>(&self, entity: &'e EntityModel, order: &[OrderField]) -> Option<Vec<Order<'e>>> {
        let mut out = Vec::new();
        for order in order {
            let field = field(entity, &order.field)?;
            out.push(Order {
                column: &field.born,
                collate: self.is_text(entity, field),
                descending: order.descending,
            });
        }
        Some(out)
    }

    fn is_text(&self, entity: &EntityModel, field: &FieldModel) -> bool {
        self.column(entity, field) == Some(Column::Text)
    }

    /// What a field's column becomes.
    pub(crate) fn column(&self, entity: &EntityModel, field: &FieldModel) -> Option<Column> {
        // A standard object stored in a column per part (area 19, chunk
        // 5.5l), read from its several columns as `Money` is.
        if let Some(entry) = field
            .compound_entry()
            .filter(|entry| matches!(entry.holds, jadpo_std::standard::Holds::Object(_)))
        {
            return Some(Column::Compound(entry.name.to_string()));
        }
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
                Prim::Duration => Column::Duration,
                // In its kind's base unit (area 19).
                Prim::Quantity(kind) => Column::Quantity(kind),
                Prim::Percentage => Column::Percentage,
                // A compound standard type, read from its several columns
                // (area 19).
                prim if let Some(entry) = jadpo_std::standard::of_builtin(prim)
                    .filter(|entry| !entry.columns().is_empty()) =>
                {
                    Column::Compound(entry.name.to_string())
                }
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
                .and_then(|plan| plan.slot(field_key.as_str()));
            let value = match value {
                Value::Param { index, path } => MissingValue::Arg {
                    index: *index,
                    path: path.clone(),
                },
                Value::Now => MissingValue::Now,
                Value::Literal(literal) => MissingValue::Constant(
                    self.types
                        .literal(literal, &jadpo_graph::TypeRef::Field(field_key))?,
                ),
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
        let mut conditions = vec![text::equal("id", &id)];
        conditions.extend(self.access(&mut statement, entity, Reach::Read)?);
        let policy = self.policy(&mut statement, entity, Effect::Read, &Row::Table)?;
        let (sql, unpoliced, permitted) = granted(
            &borns(entity, &columns.names),
            entity,
            conditions,
            policy,
            &|_| {},
        );
        let mut kinds = columns.kinds;
        if permitted.is_some() {
            kinds.push(Column::Bool);
        }
        let not_found = self.not_found(
            entity,
            MissingValue::Arg {
                index: 0,
                path: Vec::new(),
            },
        );
        let get = ReadPlan {
            store: store.clone(),
            cardinality: Cardinality::One,
            select: Select {
                sql,
                unpoliced,
                permitted,
                params: statement.params,
                columns: kinds,
                record: Record {
                    ty: entity.name.clone(),
                    fields: fields.clone(),
                },
                includes: Vec::new(),
            },
            page: None,
            missing: not_found,
        };
        Some(EntityPlan {
            key,
            name: entity.name.clone(),
            get,
        })
    }

    /// `<Entity>NotFound`, each field (typed `<Entity>.id`, which the
    /// checker holds it to) given `value`, the `id` looked for.
    pub(crate) fn not_found(
        &mut self,
        entity: &EntityModel,
        value: MissingValue,
    ) -> Option<MissingPlan> {
        let name = format!("{}NotFound", entity.name);
        let key = self.types.named(&name)?;
        let failure = self.types.failure_id(&key)?;
        let plan = self.types.failure(failure)?.clone();
        let fields = plan
            .fields
            .iter()
            .enumerate()
            .map(|(at, _)| (Some(at as u32), value.clone()))
            .collect();
        Some(MissingPlan { failure, fields })
    }
}

/// A statement written by `write` with `policy` among its `conditions`,
/// and without it, which a test's call without a caller runs; just the
/// one when policy lets anyone in.
pub(crate) fn both(
    write: &dyn Fn(&[String]) -> String,
    mut conditions: Vec<String>,
    policy: String,
) -> (String, Option<String>) {
    if policy == jadpo_sql::policy::ANYONE {
        return (write(&conditions), None);
    }
    let unpoliced = write(&conditions);
    conditions.push(policy);
    (write(&conditions), Some(unpoliced))
}

/// The statement of a read of exactly one record: `columns` where
/// `conditions` hold, then what `finish` adds (its order), with `policy`
/// selected as the last column, whose position comes back, rather than
/// tested, unless policy lets anyone in. A call without a caller runs the
/// same statement, so there is no text without policy.
fn granted(
    columns: &[String],
    entity: &EntityModel,
    conditions: Vec<String>,
    policy: String,
    finish: &dyn Fn(&mut String),
) -> (String, Option<String>, Option<u32>) {
    let mut sql = if policy == jadpo_sql::policy::ANYONE {
        text::select(columns, &entity.table, "")
    } else {
        text::select_granted(columns, &policy, &entity.table)
    };
    text::where_clause(&mut sql, &conditions, "");
    finish(&mut sql);
    let permitted =
        (policy != jadpo_sql::policy::ANYONE).then(|| u32::try_from(columns.len()).unwrap_or(0));
    (sql, None, permitted)
}

// --- columns -------------------------------------------------------------------

pub(crate) use jadpo_check::data::{field, sql_type as column_type};

/// Fields' column names: each its born name, which survives a rename.
fn borns(entity: &EntityModel, names: &[String]) -> Vec<String> {
    names
        .iter()
        .map(|name| {
            if let Some((name, part)) = name.split_once(PART_COLUMN)
                && let Some(field) = field(entity, name)
            {
                let part: usize = part.parse().unwrap_or(0);
                return field
                    .columns()
                    .get(part)
                    .map_or_else(|| field.born.clone(), |(column, _)| column.clone());
            }
            match field(entity, name) {
                Some(field) => field
                    .columns()
                    .first()
                    .map_or_else(|| field.born.clone(), |(column, _)| column.clone()),
                None => name.to_string(),
            }
        })
        .collect()
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
) -> (
    Vec<QueryPlan>,
    Vec<EntityPlan>,
    Option<jadpo_std::http::IdentityPlan>,
) {
    let model = jadpo_check::data::data_model(db, project);
    let queries = jadpo_check::data::queries::query_models(db, project);
    let lookup = Lookup::new(db, project);
    let access = jadpo_check::policy::access(db, project);
    let mut planner = Planner {
        model,
        lookup: &lookup,
        types,
        access,
        action: None,
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
    let identity = planner.identity(db, project);
    (query_plans, entity_plans, identity)
}
