//! What a read runs (area 6; docs/compiler-runtime.md §2.1 and
//! §2.5): the SQL text, where each parameter comes from, how each column
//! becomes a value and how a row becomes a record, and the failure when a
//! read of one record finds none, decided once by
//! `jadpo-plan` and run by `jadpo-runtime` for both executors, so the
//! interpreter and compiled code send the same statements and build the
//! same values.
//!
//! Every statement's text is fixed when the project is compiled: a
//! condition that applies only when its value is present (`where_present:`)
//! and a page's cursor are written as `($n IS NULL OR ...)`, so the same
//! query always sends the same text, and PostgreSQL plans an unnamed
//! statement with its parameters' values, where that test folds away.

use crate::Constant;
use crate::types::FailureId;

/// The type of a SQL parameter or column, as the runtime knows it: one per
/// column type area 5 maps a field to ("Storage types"), plus the widths of
/// whole numbers and arrays of the others, for `in` and includes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SqlType {
    Bool,
    Int2,
    Int4,
    Int8,
    Numeric,
    Text,
    Bytes,
    Uuid,
    /// `timestamptz`.
    Timestamp,
    Date,
    /// `time`.
    Time,
    /// `jsonb`.
    Json,
    /// An array of values of the inner type.
    Array(&'static SqlType),
}

impl SqlType {
    /// An array of `self`; `self` itself when it is an array already.
    pub fn array(self) -> SqlType {
        SqlType::Array(match self {
            SqlType::Bool => &SqlType::Bool,
            SqlType::Int2 => &SqlType::Int2,
            SqlType::Int4 => &SqlType::Int4,
            SqlType::Int8 => &SqlType::Int8,
            SqlType::Numeric => &SqlType::Numeric,
            SqlType::Text => &SqlType::Text,
            SqlType::Bytes => &SqlType::Bytes,
            SqlType::Uuid => &SqlType::Uuid,
            SqlType::Timestamp => &SqlType::Timestamp,
            SqlType::Date => &SqlType::Date,
            SqlType::Time => &SqlType::Time,
            SqlType::Json => &SqlType::Json,
            SqlType::Array(inner) => return SqlType::Array(inner),
        })
    }

    /// The type of a column declared as `column` (area 5's column types:
    /// `uuid`, `integer`, `numeric(10, 2)`, `timestamptz(3)` ...).
    pub fn of_column(column: &str) -> Option<SqlType> {
        let base = column.split('(').next().unwrap_or(column).trim();
        Some(match base {
            "boolean" => SqlType::Bool,
            "smallint" => SqlType::Int2,
            "integer" => SqlType::Int4,
            "bigint" => SqlType::Int8,
            "numeric" => SqlType::Numeric,
            "text" => SqlType::Text,
            "bytea" => SqlType::Bytes,
            "uuid" => SqlType::Uuid,
            "timestamptz" => SqlType::Timestamp,
            "date" => SqlType::Date,
            "time" => SqlType::Time,
            "jsonb" => SqlType::Json,
            // A currency's code (area 19).
            "char" => SqlType::Text,
            _ => return None,
        })
    }

    /// The type a parameter compared with a column of this type is sent
    /// as: whole numbers go as `bigint`, which PostgreSQL compares with
    /// and assigns to every width.
    pub fn param(self) -> SqlType {
        match self {
            SqlType::Int2 | SqlType::Int4 => SqlType::Int8,
            SqlType::Array(inner) => inner.param().array(),
            other => other,
        }
    }
}

/// How many records a read gives (area 6, "Named queries"), from a
/// query's result type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Cardinality {
    /// `T`: exactly one, or the read's not-found failure.
    One,
    /// `T?`: at most one, `none` when nothing matches.
    Optional,
    /// `List<T>`: up to `limit:`.
    List,
    /// `Page<T>`: one page and a cursor for the next.
    Page,
}

impl Cardinality {
    /// As the manifest and `jadpo sql` write it: `one`.
    pub fn word(self) -> &'static str {
        match self {
            Cardinality::One => "one",
            Cardinality::Optional => "optional",
            Cardinality::List => "list",
            Cardinality::Page => "page",
        }
    }
}

/// Where a parameter's value comes from when the statement runs.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Source {
    /// The read's argument at `index`, or a field inside it: `filter.status`
    /// is argument 1, path `["status"]`. An optional field left out is
    /// `NULL`.
    Arg { index: u32, path: Vec<String> },
    /// `clock.now`: the time the operation started.
    Now,
    /// An include: the distinct values of the including rows' key column.
    Keys,
    /// A page: the `index`th value of the cursor it continues after, or
    /// `NULL` on the first page.
    Cursor(u32),
    /// A page: one more than its size, to see whether a next page exists.
    SizePlusOne,
    /// Who is calling, for the policy compiled into the statement (area
    /// 9).
    Caller(crate::policy::CallerPart),
}

/// One parameter: `$1` is the first.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Param {
    pub source: Source,
    pub ty: SqlType,
}

/// What a column's value becomes.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Column {
    Bool,
    Int,
    Decimal,
    Text,
    Bytes,
    Uuid,
    Instant,
    Date,
    /// Milliseconds, kept as `bigint`.
    Duration,
    /// A plain variant, stored as its name: the enum and its variants in
    /// declared order.
    Enum {
        ty: String,
        variants: Vec<String>,
    },
    /// The first of the columns a compound standard type is stored in
    /// (`Money`'s amount and currency, area 19), by the type's name: the
    /// value is read from it and the [`Column::Part`]s after it, as the
    /// type's data entry lists its columns.
    Compound(String),
    /// A later column of a compound type, read by the [`Column::Compound`]
    /// before it and never on its own.
    Part,
}

/// Where a record's field comes from.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Field {
    /// The statement's column at this position.
    Column(u32),
    /// The include at this position.
    Include(u32),
}

/// How a row becomes a value: the type's name and its fields in the order
/// the type declares them.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Record {
    pub ty: String,
    pub fields: Vec<(String, Field)>,
}

/// One statement and what its rows become.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Select {
    pub sql: String,
    /// The statement without the policy compiled into `sql`, when policy
    /// limits it: what a test's call without a caller runs (area 9). It
    /// takes the same parameters.
    pub unpoliced: Option<String>,
    /// For a read of exactly one record that policy limits: the column of
    /// `sql` that says whether the caller may read each row. Such a
    /// statement filters only on what the read asks and the record's
    /// visibility, and selects the grant rather than testing it, so one
    /// statement tells a record policy hid from one that isn't there (area
    /// 4, "Concealing records"); a row the caller may not read is dropped
    /// before anything else reads it. A call without a caller reads every
    /// row, as `unpoliced` would.
    pub permitted: Option<u32>,
    pub params: Vec<Param>,
    /// What each selected column becomes, in order.
    pub columns: Vec<Column>,
    pub record: Record,
    pub includes: Vec<IncludeRead>,
}

/// How many records an include gives each record it is included in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IncludeShape {
    /// A required reference's record: always there.
    One,
    /// An optional reference's record, or the one record a unique
    /// reference to this one gives: maybe `none`.
    Optional,
    /// The records another entity's reference to this one gives.
    List,
}

/// An include: one more statement, however many records it is included
/// in (area 6, "Children and parents").
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct IncludeRead {
    pub shape: IncludeShape,
    /// The column of the including statement whose values this include
    /// matches: the reference for a parent, the `id` for children.
    pub key: u32,
    /// Its statement, run once with the keys as `Source::Keys`; its first
    /// column is the key each of its rows matches.
    pub select: Select,
}

/// How many records a page holds.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Size {
    /// Written in the query.
    Fixed(u64),
    /// A value whose type has a `max:`.
    From(Source),
}

/// A page's parts.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PageRead {
    pub size: Size,
    /// The cursor it continues after, which may be absent.
    pub after: Source,
    /// The positions of the order's columns, which a cursor holds, and
    /// their types.
    pub cursor: Vec<(u32, SqlType)>,
    /// Which query made a cursor: a cursor from another is refused.
    pub tag: u64,
}

/// A read: a query, or an entity's `get`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ReadPlan {
    /// The store it reads (a connection's name).
    pub store: String,
    pub cardinality: Cardinality,
    pub select: Select,
    pub page: Option<PageRead>,
    /// For a read of exactly one record: the failure when nothing matches,
    /// the query's `missing:` or the entity's not-found failure for `get`.
    pub missing: Option<MissingPlan>,
}

/// The failure a read or a write raises when the record it names isn't
/// there, with its fields.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct MissingPlan {
    pub failure: FailureId,
    /// Each field given: its place in the failure's fields, and its value.
    pub fields: Vec<(Option<u32>, MissingValue)>,
}

/// Where a missing record's failure's field gets its value.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum MissingValue {
    /// The operation's argument (a write's input), or a field inside it.
    Arg {
        index: u32,
        path: Vec<String>,
    },
    Constant(Constant),
    /// The operation's clock.
    Now,
}
