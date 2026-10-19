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
    /// `timestamp`, without a zone: a `LocalDateTime` (area 19, chunk
    /// 5.5w).
    LocalTimestamp,
    /// `jsonb`.
    Json,
    /// An array of values of the inner type.
    Array(&'static SqlType),
    /// A range of values of the inner type (area 19, "Ranges"):
    /// `daterange`, `tstzrange`, `int8range` or `numrange` (chunk 5.5w).
    Range(&'static SqlType),
    /// A pgvector `vector` (area 21, "Storing a vector"): single-precision
    /// floats, with or without a fixed length. Its type is the extension's,
    /// whose number each database gives it, so a parameter is sent without
    /// one and PostgreSQL infers it from the column (chunk 5.5v1).
    Vector,
    /// A PostGIS `geography` (area 20, "Storage"): a geometry in WGS84.
    /// Its type is the extension's, so a parameter is sent without a
    /// number, as a vector's is, and PostgreSQL infers it (chunk 5.5u4).
    Geography,
    /// `double precision`: only read, as the distance pgvector works out
    /// for a `nearest` query (area 21, chunk 5.5v1b).
    Float8,
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
            SqlType::LocalTimestamp => &SqlType::LocalTimestamp,
            SqlType::Json => &SqlType::Json,
            SqlType::Array(inner) => return SqlType::Array(inner),
            // Arrays of ranges, vectors, doubles and geographies are never
            // sent.
            SqlType::Range(_) | SqlType::Vector | SqlType::Float8 | SqlType::Geography => {
                &SqlType::Text
            }
        })
    }

    /// A range of `self` (chunk 5.5w); `Text` for a type PostgreSQL has no
    /// range of, which is never sent.
    pub fn range(self) -> SqlType {
        SqlType::Range(match self {
            SqlType::Date => &SqlType::Date,
            SqlType::Timestamp => &SqlType::Timestamp,
            SqlType::Int2 | SqlType::Int4 | SqlType::Int8 => &SqlType::Int8,
            SqlType::Numeric => &SqlType::Numeric,
            _ => &SqlType::Text,
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
            "timestamp" => SqlType::LocalTimestamp,
            "date" => SqlType::Date,
            "time" => SqlType::Time,
            // A range (chunk 5.5w).
            "daterange" => SqlType::Range(&SqlType::Date),
            "tstzrange" => SqlType::Range(&SqlType::Timestamp),
            "int8range" => SqlType::Range(&SqlType::Int8),
            "numrange" => SqlType::Range(&SqlType::Numeric),
            "jsonb" => SqlType::Json,
            // A currency's code (area 19).
            "char" => SqlType::Text,
            // A vector, `vector(1536)` (area 21, chunk 5.5v1).
            "vector" => SqlType::Vector,
            // A geometry (area 20, chunk 5.5u4).
            "geography" => SqlType::Geography,
            // An address's lines (area 19, chunk 5.5l).
            "text[]" => SqlType::Text.array(),
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

/// A value sent to a store or read back from one. Drivers turn these into
/// their wire formats; the runtime's read plans turn them into Jadpo
/// values, and [`crate::standard::storage::bind`] makes one from a value,
/// for the runtime and for a literal the compiler writes into a statement
/// (area 19, chunk 5.5w3b). Ordered, so an include's rows group by their
/// key.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SqlValue {
    /// `NULL`, with the type a parameter needs.
    Null(SqlType),
    Bool(bool),
    Int2(i16),
    Int4(i32),
    Int8(i64),
    /// An exact decimal in its canonical text (`-12.50`).
    Numeric(String),
    Text(String),
    Bytes(Vec<u8>),
    Uuid([u8; 16]),
    /// Microseconds since 1970-01-01T00:00:00Z.
    Timestamp(i64),
    /// Days since 1970-01-01.
    Date(i32),
    /// Microseconds since midnight.
    Time(i64),
    /// Microseconds since 1970-01-01T00:00:00 on no zone: a `timestamp`
    /// without a zone, a `LocalDateTime` (area 19, chunk 5.5w).
    LocalTimestamp(i64),
    /// JSON text.
    Json(String),
    /// An array of values of one type, none of them `NULL`: a list a field
    /// is tested `in`, or an include's keys.
    Array(SqlType, Vec<SqlValue>),
    /// A range of values of the element type (area 19, "Ranges", chunk
    /// 5.5w): from its lower end, held, to its upper end, held when
    /// `upper_held`; an end that is `None` is open.
    Range {
        element: SqlType,
        lower: Option<Box<SqlValue>>,
        upper: Option<Box<SqlValue>>,
        upper_held: bool,
    },
    /// A pgvector `vector` (area 21, chunk 5.5v1): each item's
    /// single-precision float by its bits, so a vector is compared and
    /// ordered exactly, as the other values are.
    Vector(Vec<u32>),
    /// A PostGIS `geography` (area 20, chunk 5.5u4): a geometry, each
    /// coordinate the double PostGIS keeps.
    Geography(crate::geography::Shape),
    /// A `double precision` by its bits (chunk 5.5v1b): a distance
    /// pgvector worked out, held exactly, `NaN` included.
    Float8(u64),
}

impl SqlValue {
    /// The type the value is sent as.
    pub fn ty(&self) -> SqlType {
        match self {
            SqlValue::Null(ty) => *ty,
            SqlValue::Bool(_) => SqlType::Bool,
            SqlValue::Int2(_) => SqlType::Int2,
            SqlValue::Int4(_) => SqlType::Int4,
            SqlValue::Int8(_) => SqlType::Int8,
            SqlValue::Numeric(_) => SqlType::Numeric,
            SqlValue::Text(_) => SqlType::Text,
            SqlValue::Bytes(_) => SqlType::Bytes,
            SqlValue::Uuid(_) => SqlType::Uuid,
            SqlValue::Timestamp(_) => SqlType::Timestamp,
            SqlValue::Date(_) => SqlType::Date,
            SqlValue::Time(_) => SqlType::Time,
            SqlValue::LocalTimestamp(_) => SqlType::LocalTimestamp,
            SqlValue::Json(_) => SqlType::Json,
            SqlValue::Array(element, _) => element.array(),
            SqlValue::Range { element, .. } => element.range(),
            SqlValue::Vector(_) => SqlType::Vector,
            SqlValue::Geography(_) => SqlType::Geography,
            SqlValue::Float8(_) => SqlType::Float8,
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
    /// A literal or an enum value the statement's condition compares with,
    /// as the storage codec binds it (area 19, chunk 5.5w3b).
    Stored(SqlValue),
    /// How many rows a read whose conditions are rechecked asks for (chunk
    /// 6.3b; area 20, chunk 5.5u5): the records it keeps at first, and more
    /// each time the recheck leaves it short ([`Recheck`]).
    Fetch,
    /// One side of an envelope a `BoundingBox` filter searches (area 20,
    /// chunk 5.5u5), worked out from the box the read's argument at
    /// `index` (or a field inside it, `path`) holds.
    Envelope {
        index: u32,
        path: Vec<String>,
        part: EnvelopePart,
    },
    /// The double an `ai.distance` condition compares with (area 21, chunk
    /// 5.8c), worked out from the decimal or whole number the read's
    /// argument at `index` (or a field inside it, `path`) holds, so it
    /// meets the comparison as code's decimal does
    /// ([`crate::standard::ai::distance_bound`]); below it when `strict`.
    DistanceBound {
        index: u32,
        path: Vec<String>,
        strict: bool,
    },
}

/// The sides of the two envelopes a `BoundingBox` filter searches, in
/// longitude (area 20, "Queries", chunk 5.5u5): a box that crosses the
/// antimeridian is two, from its west to 180 and from -180 to its east;
/// one that ends on the antimeridian also searches the line at the other
/// end, -180 being 180; any other box is one, given twice.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EnvelopePart {
    FirstWest,
    FirstEast,
    SecondWest,
    SecondEast,
}

impl EnvelopePart {
    /// This side, in degrees, of the envelopes for a box from `west` to
    /// `east`.
    pub fn of(self, west: &crate::Num, east: &crate::Num) -> crate::Num {
        let degrees = crate::Num::whole;
        let crosses = west.compare(*east) == std::cmp::Ordering::Greater;
        let at = |side: &crate::Num, d: i128| side.compare(degrees(d)) == std::cmp::Ordering::Equal;
        let second = if crosses {
            (degrees(-180), *east)
        } else if at(west, -180) {
            (degrees(180), degrees(180))
        } else if at(east, 180) {
            (degrees(-180), degrees(-180))
        } else {
            (*west, *east)
        };
        match self {
            EnvelopePart::FirstWest => *west,
            EnvelopePart::FirstEast if crosses => degrees(180),
            EnvelopePart::FirstEast => *east,
            EnvelopePart::SecondWest => second.0,
            EnvelopePart::SecondEast => second.1,
        }
    }
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
    /// A physical quantity, kept as `numeric` in its kind's base unit at
    /// its scale (area 19).
    Quantity(crate::QuantityKind),
    /// A percentage, kept as `numeric`, the fraction it is: 20% is 0.20
    /// (area 19).
    Percentage,
    /// A plain variant, stored as its name: the enum and its variants in
    /// declared order.
    Enum {
        ty: String,
        variants: Vec<String>,
    },
    /// The first of the columns a compound standard type is stored in
    /// (`Money`'s amount and currency, area 19), by the type's name (a
    /// range's with its item, `Range<Duration>`, chunk 5.5w): the value is
    /// read from it and the [`Column::Part`]s after it, as the type's data
    /// entry lists its columns. `may_be_none` for a field that may be
    /// `none`, which every column `NULL` is; a field that can't is then the
    /// value with none of its parts, a `ProblemDetails` that says nothing.
    Compound {
        ty: String,
        may_be_none: bool,
    },
    /// A later column of a compound type, read by the [`Column::Compound`]
    /// before it and never on its own.
    Part,
    /// A JSON-held standard value (`RichText`, `RichTextBlock`,
    /// `LocalisedText`), stored as its contract's JSON in one `jsonb`
    /// column (area 5's "Storage types"; area 19, chunk 5.5w2b): read with
    /// the codec this number names in the plan's JSON table, as
    /// `json.read`'s site numbers its type, so a stored value is read
    /// exactly as a request's body is.
    Json(u32),
    /// A vector, kept in a pgvector `vector` column (area 21, chunk
    /// 5.5v1): each item read back as the decimal that is its
    /// single-precision float.
    Vector,
    /// A geometry, kept in a PostGIS `geography` column (area 20, chunk
    /// 5.5u4), by the standard type the field holds: one of the six
    /// geometries, or `Geometry` for any of them.
    Geography(String),
    /// The distance pgvector worked out for a `nearest` query (area 21,
    /// chunk 5.5v1b), a `float8`, read as `ai.distance` gives it: the
    /// decimal PostgreSQL writes the double as.
    Distance,
    /// The distance PostGIS measured for a query ordered by `geo.distance`
    /// (area 20, chunk 5.5u5), a `float8` in metres: never a field, read
    /// only by the [`Recheck`] to know when it has fetched enough.
    Measured,
}

impl Column {
    /// How a column holding a value of a type built on `builtin` is read
    /// (area 5, "Storage types"): the one table a query's read and a
    /// backfill worked out in Jadpo share (chunk 5.8b). `None` for a value
    /// held in several columns, as JSON, or not stored.
    pub fn of_builtin(builtin: crate::Builtin) -> Option<Column> {
        use crate::Builtin;
        Some(match builtin {
            Builtin::Bool => Column::Bool,
            Builtin::Int => Column::Int,
            Builtin::Decimal => Column::Decimal,
            Builtin::Text => Column::Text,
            Builtin::Bytes => Column::Bytes,
            Builtin::Uuid => Column::Uuid,
            Builtin::Instant => Column::Instant,
            Builtin::CalendarDate => Column::Date,
            Builtin::Duration => Column::Duration,
            // In its kind's base unit (area 19).
            Builtin::Quantity(kind) => Column::Quantity(kind),
            Builtin::Percentage => Column::Percentage,
            _ => return None,
        })
    }
}

/// Where a record's field comes from.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Field {
    /// The statement's column at this position.
    Column(u32),
    /// The include at this position.
    Include(u32),
    /// A record made from the same row: a `Nearest<T>`'s `item` (area 21,
    /// chunk 5.5v1b).
    Record(Box<Record>),
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
    /// Statements run first, in the read's transaction: a `nearest`
    /// query's `SET LOCAL` of pgvector's search settings (area 21, chunk
    /// 5.5v1b). They take no parameters.
    pub settings: Vec<String>,
    pub cardinality: Cardinality,
    pub select: Select,
    pub page: Option<PageRead>,
    /// For a read of exactly one record: the failure when nothing matches,
    /// the query's `missing:` or the entity's not-found failure for `get`.
    pub missing: Option<MissingPlan>,
    /// For a query whose conditions search an index (`geo.` tests), or
    /// that is ordered by `geo.distance`: what each row the statement found
    /// is held to again, in Jadpo's own code, before it becomes a record
    /// (chunk 6.3b; area 20, chunk 5.5u5).
    pub recheck: Option<Recheck>,
    /// For a map layer (area 6, chunk 5.5u8): the list's records are
    /// `Feature`s, given as one `FeatureCollection`.
    pub layer: bool,
}

/// How far, in metres, the statement of a query with `geo.` conditions
/// searches past what the condition says (area 20, "Queries", chunk
/// 5.5u5): PostGIS measures in doubles, good to some nanometres, and Jadpo
/// exactly, so a record the code finds within a radius, or touching a
/// shape, is always among the rows the index finds; the recheck then
/// decides each. Written into the statement's text as it is here, by
/// `jadpo_sql::condition`.
pub const GEO_MARGIN: &str = "0.001";

/// [`GEO_MARGIN`] as a double, for comparing with a distance PostGIS
/// measured.
pub const GEO_MARGIN_METRES: f64 = 0.001;

/// One condition a row is held to in Jadpo's own code (chunk 6.3b): a
/// standard operation on the row's columns and the read's values, run by
/// `jadpo_value::run`, the code a function body's call of it runs, which
/// must give `true`. A `geo.` test is one (area 20, "Queries", chunk
/// 5.5u5): `geo.within(spot, doorstep, distance: max_trip)` is
/// `GeoWithin` of the column holding `spot`, the read's argument and its
/// other.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Check {
    pub op: crate::Op,
    /// Its arguments, in the operation's parameter order.
    pub args: Vec<Operand>,
}

/// One of a [`Check`]'s arguments.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Operand {
    /// The statement's column at this position: a field's stored value. A
    /// record whose field is `none` meets no condition on it.
    Column(u32),
    /// A value from the read's arguments (a parameter, or a field inside
    /// one), or `clock.now`.
    Value(Source),
    /// A value written in the query, as code builds it.
    Constant(Constant),
    /// An option the call leaves out, given as `none`.
    Absent,
}

/// A read ordered by an operation's result, nearest first (area 20,
/// question 4: `geo.distance(field, point) asc`): records by the result as
/// the code works it out, then by `id`. The statement orders the rows it
/// finds by the database's own measure of it, which may differ from the
/// code's by up to [`GEO_MARGIN_METRES`], the margin a search keeps.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct RecheckOrder {
    /// The operation whose result orders the records: a physical quantity,
    /// compared in its kind's base unit with the database's measure.
    pub key: Check,
    /// The statement's column holding the database's measure, which it is
    /// ordered by.
    pub measured: u32,
    /// The statement's column holding the record's `id`.
    pub id: u32,
}

/// What a read whose conditions search an index works out again in
/// Jadpo's own code (chunk 6.3b; area 20, "Queries", chunk 5.5u5). Its
/// statement finds, through the index, every row the code could keep and
/// perhaps a few more; each row is then held to each check, and a read
/// ordered by an operation's result is sorted by the code's results. The
/// statement asks for as many rows as the read keeps ([`Source::Fetch`]),
/// and asks again for four times as many while the checks leave it short,
/// or while a row it didn't fetch could still come before the last it
/// keeps.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Recheck {
    pub checks: Vec<Check>,
    pub order: Option<RecheckOrder>,
    /// How many records a list keeps: its `limit:`. A page keeps its size
    /// and one more; a read of one record, found by a unique field, keeps
    /// every row that passes.
    pub limit: Option<Size>,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Num;

    fn sides(west: i128, east: i128) -> [String; 4] {
        let (west, east) = (Num::whole(west), Num::whole(east));
        [
            EnvelopePart::FirstWest,
            EnvelopePart::FirstEast,
            EnvelopePart::SecondWest,
            EnvelopePart::SecondEast,
        ]
        .map(|part| part.of(&west, &east).to_string())
    }

    #[test]
    fn a_box_s_envelopes_reach_round_the_antimeridian() {
        assert_eq!(sides(-10, 10), ["-10", "10", "-10", "10"], "one, twice");
        assert_eq!(sides(170, -170), ["170", "180", "-180", "-170"], "across");
        assert_eq!(
            sides(-180, -170),
            ["-180", "-170", "180", "180"],
            "from -180"
        );
        assert_eq!(sides(170, 180), ["170", "180", "-180", "-180"], "to 180");
        assert_eq!(sides(-180, 180), ["-180", "180", "180", "180"], "all of it");
    }
}
