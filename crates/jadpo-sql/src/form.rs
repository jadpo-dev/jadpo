//! SQL forms (area 19, "SQL forms"): the PostgreSQL 16 expression that
//! gives exactly the result the runtime gives for one standard operation,
//! for every argument, written from its arguments' SQL. A computed
//! backfill whose every operation has one runs as one `UPDATE` per batch
//! (area 5, "Computed backfills"); aggregate queries are their second
//! user, and a query's conditions their third (area 6, "Operations in
//! conditions", chunk 6.3b), which [`crate::condition`] lowers through
//! [`operation`] and indexes where [`indexable`] says PostgreSQL can.
//! `ai.distance` has one more form, pgvector's operator, which area 21
//! decided a query's distance is: the one form here outside area 19's
//! tables, so [`has_form`] doesn't name it and no backfill uses it.
//!
//! Each form here is tested against the runtime on the same inputs, on
//! live PostgreSQL (`crates/jadpo-cli/tests/sql_forms.rs`), and area 19's
//! operation tables list exactly the operations [`has_form`] names (a test
//! below). What every form keeps to (chosen in chunk 5.6):
//!
//! - **It is given values, never `NULL`.** A value that may be `none` is
//!   tested before an operation is called on it, as code must, so a form
//!   never decides what `none` means; one whose result may be `none`
//!   (`range.intersection`) gives `NULL` for it.
//! - **A whole number is `bigint`,** as an `Int` is 64 bits: a whole
//!   number from a narrower column (`smallint`, `integer`) is widened
//!   before arithmetic, and `char_length` and `jsonb_array_length` are
//!   cast, so a result overflows exactly where the runtime's does.
//! - **A count or position is clamped to PostgreSQL's `integer`** where a
//!   function takes one (`left`, `substr`, `lpad`): text is never longer
//!   than a gigabyte, so a larger count means the same as the largest.
//! - **A decimal keeps the places the runtime keeps,** read from the value
//!   with `scale()`, since a `numeric` carries its places as a runtime
//!   `Decimal` does; a result past 38 digits is an error, as it is a defect
//!   in the runtime.
//! - **Text is compared in code points** (`COLLATE "C"`), never by the
//!   database's collation.
//! - **An argument may be written more than once**, since every form is
//!   deterministic; only `temporal.period_between`, which needs its two
//!   dates many times over, names them once in a subquery.

use jadpo_std::{Builtin, Op, Rounding, VectorDistance};

use crate::text_literal;

/// How an argument's value is held in SQL.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Held {
    /// One column's value of a type built on this built-in type, as area
    /// 5's storage types hold it: text as `text`, a date as `date`.
    Builtin(Builtin),
    /// A list, a set or a map, as `jsonb`.
    Json,
    /// A range, in PostgreSQL's range type of its ends, which are built on
    /// this built-in type.
    Range(Builtin),
    /// A pgvector `vector` (area 21, "Storing a vector").
    Vector,
    /// A value of this standard geospatial type (area 20): a geometry in a
    /// PostGIS `geography`, or a `BoundingBox` or a `Circle` given whole.
    Shape(&'static str),
    /// A `double precision` the database works out: the distance pgvector
    /// gives (area 21, chunk 5.5v1b).
    Double,
}

/// One argument of an operation, in the operation's parameter order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arg<'a> {
    /// A value, as a SQL expression, and how it is held.
    Value(&'a str, Held),
    /// A value stored in several columns (an amount of money's minor units
    /// and currency), each column's expression in its entry's order.
    Parts(&'a [String]),
    /// A rounding mode written out (`rounding: Rounding.half_up`).
    Rounding(Rounding),
    /// A whole number written out (`decimal.round`'s `scale:`).
    Whole(i64),
    /// One character written out (`text.pad_start`'s `fill:`).
    Character(char),
    /// How two vectors are compared, which the checker gives
    /// `ai.distance` from their type (area 21, chunk 5.5v1).
    Metric(VectorDistance),
    /// An option the call leaves out.
    Absent,
}

/// What a form gives.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Form {
    /// One SQL expression.
    Value(String),
    /// A value stored in several columns (a `CalendarPeriod`'s years,
    /// months and days), each column's expression in its entry's order.
    Parts(Vec<String>),
}

impl Form {
    /// The one expression, when it is one.
    pub fn value(&self) -> Option<&str> {
        match self {
            Form::Value(sql) => Some(sql),
            Form::Parts(_) => None,
        }
    }
}

/// Whether `op` has a SQL form for some arguments: the operations area
/// 19's tables give one.
pub fn has_form(op: Op) -> bool {
    matches!(
        op,
        Op::ListCount
            | Op::SetCount
            | Op::MapCount
            | Op::RangeContains
            | Op::RangeOverlaps
            | Op::RangeIntersection
            | Op::IntegerDivide
            | Op::IntegerAbs
            | Op::IntegerMin
            | Op::IntegerMax
            | Op::IntegerFromDecimal
            | Op::DecimalDivide
            | Op::DecimalAbs
            | Op::DecimalMin
            | Op::DecimalMax
            | Op::DecimalRound
            | Op::MoneyMinor
            | Op::MoneyCurrency
            | Op::TemporalPeriodBetween
            | Op::TemporalYear
            | Op::TemporalMonth
            | Op::TemporalDay
            | Op::TextLength
            | Op::TextTrim
            | Op::TextContains
            | Op::TextStartsWith
            | Op::TextEndsWith
            | Op::TextReplace
            | Op::TextSlice
            | Op::TextKeepFirst
            | Op::TextKeepLast
            | Op::TextDropFirst
            | Op::TextDropLast
            | Op::TextPadStart
            | Op::TextFrom
            | Op::HashSha256
            | Op::EncodingToBase64
            | Op::EncodingToBase64url
            | Op::EncodingToHex
            | Op::EncodingToUtf8
            | Op::WebCidrContains
    )
}

/// The SQL form of `op` on `args`, its parameters in table order; `None`
/// when it has none, or none for these arguments (a rounding mode that
/// isn't written out, a time given to `text.from`).
pub fn operation(op: Op, args: &[Arg<'_>]) -> Option<Form> {
    use Arg::{Absent, Character, Parts, Value, Whole};
    let one = |sql: String| Some(Form::Value(sql));
    match (op, args) {
        (Op::ListCount | Op::SetCount, [Value(x, Held::Json)]) => {
            one(format!("jsonb_array_length({x})::bigint"))
        }
        (Op::MapCount, [Value(x, Held::Json)]) => {
            one(format!("(SELECT count(*) FROM jsonb_object_keys({x}))"))
        }
        (Op::RangeContains, [Value(x, Held::Range(end)), Value(a, _)]) => {
            one(format!("(({x}) @> ({a})::{})", range_end(*end)?))
        }
        (Op::RangeOverlaps, [Value(x, Held::Range(end)), Value(a, Held::Range(_))])
            if range_end(*end).is_some() =>
        {
            one(format!("(({x}) && ({a}))"))
        }
        (Op::RangeIntersection, [Value(x, Held::Range(end)), Value(a, Held::Range(_))])
            if range_end(*end).is_some() =>
        {
            one(format!("NULLIF(({x}) * ({a}), 'empty')"))
        }
        (Op::IntegerDivide, [Value(x, _), Value(a, _), Arg::Rounding(rounding)]) => {
            one(integer_divide(&wide(x), &wide(a), *rounding))
        }
        (Op::IntegerAbs, [Value(x, _)]) => one(format!("abs({})", wide(x))),
        (Op::IntegerMin, [Value(x, _), Value(a, _)]) => {
            one(format!("LEAST({}, {})", wide(x), wide(a)))
        }
        (Op::IntegerMax, [Value(x, _), Value(a, _)]) => {
            one(format!("GREATEST({}, {})", wide(x), wide(a)))
        }
        (Op::DecimalAbs, [Value(x, _)]) => one(format!("abs({x})")),
        (Op::DecimalMin, [Value(x, _), Value(a, _)]) => one(format!("LEAST({x}, {a})")),
        (Op::DecimalMax, [Value(x, _), Value(a, _)]) => one(format!("GREATEST({x}, {a})")),
        (Op::IntegerFromDecimal, [Value(x, _), Arg::Rounding(rounding)]) => {
            one(format!("({})::bigint", round_whole(x, *rounding)))
        }
        (Op::DecimalDivide, [Value(x, _), Value(a, _), Arg::Rounding(rounding)]) => {
            one(decimal_divide(x, a, *rounding))
        }
        (Op::DecimalRound, [Value(x, _), Whole(scale), Arg::Rounding(rounding)])
            if (0..=38).contains(scale) =>
        {
            one(decimal_round(x, *scale as u32, *rounding))
        }
        (Op::MoneyMinor, [Parts([amount, _])]) => one(amount.clone()),
        (Op::MoneyCurrency, [Parts([_, currency])]) => one(format!("({currency})::text")),
        (Op::TemporalPeriodBetween, [Value(start, _), Value(end, _)]) => {
            Some(Form::Parts(period_between(start, end)))
        }
        // A date's parts (chunk 5.6c, an aggregate query's time buckets).
        // PostgreSQL counts no year 0: its 1 BC, whose `extract` gives -1,
        // is the runtime's year 0, as ISO 8601 counts.
        (Op::TemporalYear, [Value(x, Held::Builtin(builtin))])
            if builtin.representation() == Builtin::CalendarDate =>
        {
            let year = format!("extract(year FROM {x})");
            one(format!(
                "(CASE WHEN {year} < 0 THEN {year} + 1 ELSE {year} END)::bigint"
            ))
        }
        (Op::TemporalMonth, [Value(x, Held::Builtin(builtin))])
            if builtin.representation() == Builtin::CalendarDate =>
        {
            one(format!("extract(month FROM {x})::bigint"))
        }
        (Op::TemporalDay, [Value(x, Held::Builtin(builtin))])
            if builtin.representation() == Builtin::CalendarDate =>
        {
            one(format!("extract(day FROM {x})::bigint"))
        }
        (Op::TextLength, [Value(x, held)]) if text(held) => {
            one(format!("char_length({x})::bigint"))
        }
        (Op::TextTrim, [Value(x, held)]) if text(held) => {
            one(format!("btrim({x}, {})", text_literal(TRIMMED_CHARACTERS)))
        }
        (Op::TextContains, [Value(x, held), Value(a, _)]) if text(held) => {
            one(format!("(strpos({x}, {a}) > 0)"))
        }
        (Op::TextStartsWith, [Value(x, held), Value(a, _)]) if text(held) => {
            one(format!("starts_with({x}, {a})"))
        }
        (Op::TextEndsWith, [Value(x, held), Value(a, _)]) if text(held) => one(format!(
            "(right({x}, char_length({a})) = ({a}) COLLATE \"C\")"
        )),
        (Op::TextReplace, [Value(x, held), Value(a, _), Value(b, _)]) if text(held) => {
            one(format!("replace({x}, {a}, {b})"))
        }
        (Op::TextSlice, [Value(x, held), Value(from, _), Value(count, _)]) if text(held) => {
            one(format!(
                "substr({x}, {} + 1, {})",
                clamped(from, 1),
                clamped(count, 0)
            ))
        }
        (Op::TextKeepFirst, [Value(x, held), Value(n, _)]) if text(held) => {
            one(format!("left({x}, {})", clamped(n, 0)))
        }
        (Op::TextKeepLast, [Value(x, held), Value(n, _)]) if text(held) => {
            one(format!("right({x}, {})", clamped(n, 0)))
        }
        (Op::TextDropFirst, [Value(x, held), Value(n, _)]) if text(held) => {
            one(format!("substr({x}, {} + 1)", clamped(n, 1)))
        }
        (Op::TextDropLast, [Value(x, held), Value(n, _)]) if text(held) => one(format!(
            "(CASE WHEN ({n}) = 0 THEN {x} ELSE left({x}, -{}) END)",
            clamped(n, 0)
        )),
        (Op::TextPadStart, [Value(x, held), Value(length, _), Character(fill)])
            if text(held) && *fill != '\0' =>
        {
            one(format!(
                "(CASE WHEN char_length({x}) >= ({length}) THEN {x} ELSE lpad({x}, {}, {}) END)",
                clamped(length, 0),
                text_literal(&fill.to_string())
            ))
        }
        (Op::TextFrom, [Value(x, Held::Builtin(builtin)), Absent])
            if matches!(
                builtin.representation(),
                Builtin::Int | Builtin::Decimal | Builtin::Uuid | Builtin::Bool
            ) =>
        {
            one(format!("({x})::text"))
        }
        (Op::HashSha256, [Value(x, held)]) if text(held) => {
            one(format!("sha256(convert_to({x}, 'UTF8'))"))
        }
        (Op::HashSha256, [Value(x, Held::Builtin(Builtin::Bytes))]) => one(format!("sha256({x})")),
        (Op::EncodingToBase64, [Value(x, _)]) => {
            one(format!("translate(encode({x}, 'base64'), E'\\n', '')"))
        }
        (Op::EncodingToBase64url, [Value(x, _)]) => one(format!(
            "rtrim(translate(encode({x}, 'base64'), E'+/\\n', '-_'), '=')"
        )),
        (Op::EncodingToHex, [Value(x, _)]) => one(format!("encode({x}, 'hex')")),
        (Op::EncodingToUtf8, [Value(x, held)]) if text(held) => {
            one(format!("convert_to({x}, 'UTF8')"))
        }
        (Op::WebCidrContains, [Value(x, _), Value(a, _)]) => {
            one(format!("(({x})::cidr >>= ({a})::inet)"))
        }
        // pgvector's distance by the vectors' type's metric (area 21,
        // "The nearest-neighbour query"): a `double precision`, `NULL`
        // for a `NULL` vector, as every operator of pgvector's is strict.
        (
            Op::AiDistance,
            [
                Value(x, Held::Vector),
                Value(a, Held::Vector),
                Arg::Metric(metric),
            ],
        ) => one(format!("{x} {} {a}", metric.operator())),
        _ => None,
    }
}

/// Whether PostgreSQL can index `op`'s form on a first argument held as
/// `first`, its other arguments written out (area 6, "Operations in
/// conditions", chunk 6.3b): an index's expression may call only
/// immutable functions and may not read a subquery. `map.count` and
/// `temporal.period_between` read a subquery; `hash.sha256` and
/// `encoding.to_utf8` of text convert it with `convert_to`, which
/// PostgreSQL marks stable, since it reads the database's encoding; a
/// range's operations take a range written out, which a query can't
/// write. A live test builds an index on every other form
/// (`crates/jadpo-cli/tests/operation_conditions.rs`).
pub fn indexable(op: Op, first: Held) -> bool {
    match op {
        Op::MapCount | Op::TemporalPeriodBetween => false,
        Op::RangeContains | Op::RangeOverlaps | Op::RangeIntersection => false,
        Op::HashSha256 => {
            matches!(first, Held::Builtin(builtin) if builtin.representation() == Builtin::Bytes)
        }
        Op::EncodingToUtf8 | Op::AiDistance => false,
        other => has_form(other),
    }
}

/// An operator (area 3, "Expressions and operators"), as its SQL form
/// takes it (chunk 5.6b).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Operator {
    And,
    Or,
    Not,
    Negate,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
}

/// The SQL form of operator `op` on its operands (one for `not` and `-`,
/// two otherwise), each a value held in one column of a built-in type: an
/// enum without data is held as text. `None` where it has none: `in` and
/// `matches`, the operators on money, quantities, percentages, periods
/// and times moved by a duration, a decimal's `*` and `/`, and operands
/// of different kinds. What each keeps to (chosen in chunk 5.6b):
///
/// - **`and` and `or` stop early, as the runtime does,** so an operand
///   that would stop the statement (`x / y` after `y != 0 and`) is never
///   worked out: they are `CASE`, whose branches PostgreSQL works out
///   only when taken.
/// - **Whole numbers are `bigint`,** widened first, so `+`, `-`, `*`,
///   `/` and unary `-` overflow exactly where an `Int`'s do, and `/` and
///   `%` drop the remainder toward zero, as Rust's do.
/// - **A decimal's `+` and `-` keep the larger operand's places and stop
///   where the runtime's do:** past 38 digits, or where either operand
///   lifted to those places is past the 128 bits the runtime works in.
/// - **Text is compared in code points** (`COLLATE "C"`), and `+` joins.
pub fn operator(op: Operator, args: &[Arg<'_>]) -> Option<String> {
    use Operator::*;
    let held = |arg: &Arg<'_>| match arg {
        Arg::Value(sql, Held::Builtin(builtin)) => Some((sql.to_string(), kind(*builtin)?)),
        _ => None,
    };
    match (op, args) {
        (Not, [a]) => {
            let (a, Kind::Bool) = held(a)? else {
                return None;
            };
            Some(format!("(NOT {a})"))
        }
        (Negate, [a]) => match held(a)? {
            (a, Kind::Whole) => Some(format!("(-{})", wide(&a))),
            (a, Kind::Decimal) => Some(format!("(-({a}))")),
            _ => None,
        },
        (_, [a, b]) => binary(op, held(a)?, held(b)?),
        _ => None,
    }
}

/// Whether a form reads a value of a type built on `builtin` held in one
/// column: what a backfill's expression may hold (chunk 5.6b).
pub fn reads(builtin: Builtin) -> bool {
    kind(builtin).is_some()
}

/// What an operator does with a built-in type's values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Bool,
    Whole,
    Decimal,
    Text,
    /// Compared only: a UUID, bytes.
    Same,
    /// Compared and ordered: a date, an instant.
    Ordered,
    /// A duration, in milliseconds: ordered, and added to another.
    Duration,
}

fn kind(builtin: Builtin) -> Option<Kind> {
    Some(match builtin.representation() {
        Builtin::Bool => Kind::Bool,
        Builtin::Int => Kind::Whole,
        Builtin::Decimal => Kind::Decimal,
        Builtin::Text => Kind::Text,
        Builtin::Uuid | Builtin::Bytes => Kind::Same,
        Builtin::CalendarDate | Builtin::Instant => Kind::Ordered,
        Builtin::Duration => Kind::Duration,
        _ => return None,
    })
}

fn binary(op: Operator, (a, ka): (String, Kind), (b, kb): (String, Kind)) -> Option<String> {
    use Operator::*;
    let number = |k: Kind| matches!(k, Kind::Whole | Kind::Decimal);
    let numbers = number(ka) && number(kb);
    let decimal = numbers && (ka == Kind::Decimal || kb == Kind::Decimal);
    let whole = ka == Kind::Whole && kb == Kind::Whole;
    let same = ka == kb;
    let compare = |sign: &str| -> Option<String> {
        if whole {
            return Some(format!("({} {sign} {})", wide(&a), wide(&b)));
        }
        if numbers || (same && ka != Kind::Text) {
            return Some(format!("(({a}) {sign} ({b}))"));
        }
        None
    };
    match op {
        And if ka == Kind::Bool && kb == Kind::Bool => {
            Some(format!("(CASE WHEN {a} THEN {b} ELSE FALSE END)"))
        }
        Or if ka == Kind::Bool && kb == Kind::Bool => {
            Some(format!("(CASE WHEN {a} THEN TRUE ELSE {b} END)"))
        }
        Equal | NotEqual => {
            let sign = if op == Equal { "=" } else { "<>" };
            if ka == Kind::Text && kb == Kind::Text {
                return Some(format!("(({a}) {sign} ({b}) COLLATE \"C\")"));
            }
            compare(sign)
        }
        Less | LessEqual | Greater | GreaterEqual
            if numbers || (same && matches!(ka, Kind::Ordered | Kind::Duration)) =>
        {
            compare(match op {
                Less => "<",
                LessEqual => "<=",
                Greater => ">",
                _ => ">=",
            })
        }
        Add | Subtract if decimal => Some(decimal_sum(&a, &b, op == Add)),
        Add | Subtract | Multiply | Divide | Remainder
            if whole || (same && ka == Kind::Duration && matches!(op, Add | Subtract)) =>
        {
            let sign = match op {
                Add => "+",
                Subtract => "-",
                Multiply => "*",
                Divide => "/",
                _ => "%",
            };
            Some(format!("({} {sign} {})", wide(&a), wide(&b)))
        }
        Add if ka == Kind::Text && kb == Kind::Text => Some(format!("(({a}) || ({b}))")),
        _ => None,
    }
}

/// The largest 128-bit whole number, the most a runtime decimal's
/// mantissa is worked out in.
const I128_MAX: &str = "170141183460469231731687303715884105727";

/// A decimal's `+` or `-`, as the runtime works it out: both mantissas
/// lifted to the larger scale in 128 bits, then added, the result held
/// to 38 digits. Named once in a subquery, since each operand is read
/// several times; where the runtime stops, a cast to `numeric(38, 0)`
/// stops the statement.
fn decimal_sum(a: &str, b: &str, add: bool) -> String {
    let sign = if add { "+" } else { "-" };
    let lift = |x: &str| format!("abs({x}) * power(10::numeric, v.s) <= {I128_MAX}");
    format!(
        "(SELECT CASE WHEN {} AND {} THEN round(((v.a {sign} v.b) * power(10::numeric, v.s))::numeric(38, 0) * power(10::numeric, -v.s), v.s) ELSE (v.a * 0 + 1e38)::numeric(38, 0) END FROM (SELECT ({a})::numeric AS a, ({b})::numeric AS b, greatest(scale(({a})::numeric), scale(({b})::numeric)) AS s) AS v)",
        lift("v.a"),
        lift("v.b"),
    )
}

/// A conditional (area 3): each condition in turn and the value it gives,
/// and the value when none holds, as `CASE`, which works out only the
/// branch it takes, so a value that would stop the statement where its
/// condition doesn't hold never runs. An `if`'s chain, and a `match`
/// whose last arm is the `else` (chunk 5.6b).
pub fn case(arms: &[(String, String)], otherwise: &str) -> String {
    if arms.is_empty() {
        return otherwise.to_string();
    }
    let mut out = String::from("(CASE");
    for (condition, value) in arms {
        out.push_str(&format!(" WHEN {condition} THEN {value}"));
    }
    out.push_str(&format!(" ELSE {otherwise} END)"));
    out
}

/// Whether a value that may be `none` is: `x == none`, or with `negated`
/// `x != none`.
pub fn none_check(x: &str, negated: bool) -> String {
    if negated {
        format!("(({x}) IS NOT NULL)")
    } else {
        format!("(({x}) IS NULL)")
    }
}

/// Every character Unicode gives the `White_Space` property, which Rust's
/// `str::trim` removes and `text.trim` with it.
pub const TRIMMED_CHARACTERS: &str = "\u{9}\u{a}\u{b}\u{c}\u{d}\u{20}\u{85}\u{a0}\u{1680}\u{2000}\u{2001}\u{2002}\u{2003}\u{2004}\u{2005}\u{2006}\u{2007}\u{2008}\u{2009}\u{200a}\u{2028}\u{2029}\u{202f}\u{205f}\u{3000}";

/// A whole number as `bigint`: a field whose range is narrow is stored
/// in a `smallint` or an `integer` (area 5), whose arithmetic overflows
/// where an `Int`'s doesn't (`abs` of -32768).
fn wide(x: &str) -> String {
    format!("({x})::bigint")
}

/// Whether a value is held as text.
fn text(held: &Held) -> bool {
    matches!(held, Held::Builtin(builtin) if builtin.representation() == Builtin::Text)
}

/// The type of a range's ends, for a range PostgreSQL holds in a range
/// type of its own (area 5): dates, instants, whole numbers, durations and
/// decimals. A value tested against a range is cast to it, since a whole
/// number in a narrower column isn't an `int8range`'s `bigint`.
fn range_end(end: Builtin) -> Option<&'static str> {
    Some(match end.representation() {
        Builtin::CalendarDate => "date",
        Builtin::Instant => "timestamptz",
        Builtin::Int | Builtin::Duration => "bigint",
        Builtin::Decimal => "numeric",
        _ => return None,
    })
}

/// A count that is never negative as PostgreSQL's `integer`, which its
/// text functions take, no larger than leaves `room` to add to it: text is
/// never longer than a gigabyte, so a larger count means the same.
fn clamped(n: &str, room: i64) -> String {
    format!("LEAST({n}, {})::integer", i64::from(i32::MAX) - room)
}

/// `+1` when the exact quotient of `n` by `d` is positive, `-1` when it is
/// negative; never asked when it is 0.
fn quotient_sign(n: &str, d: &str) -> String {
    format!("(CASE WHEN (({n}) < 0) = (({d}) < 0) THEN 1 ELSE -1 END)")
}

/// Whether a truncated quotient `q` with remainder `r` by `d` (`r` not 0)
/// takes one step away from zero under `rounding`: `r`, `d` and `q`
/// compared as `numeric`, so doubling a remainder never overflows.
fn steps(rounding: Rounding, q: &str, r: &str, d: &str) -> String {
    let twice = format!("2 * abs(({r})::numeric)");
    let whole = format!("abs(({d})::numeric)");
    match rounding {
        Rounding::Up => "TRUE".to_string(),
        Rounding::Down => "FALSE".to_string(),
        Rounding::HalfUp => format!("{twice} >= {whole}"),
        Rounding::HalfEven => {
            format!("({twice} > {whole} OR ({twice} = {whole} AND mod(({q})::numeric, 2) <> 0))")
        }
    }
}

/// `integer.divide`: PostgreSQL's `/` drops the remainder toward zero, as
/// `Rounding.down` does; every other mode steps once away from zero when
/// the remainder says so.
fn integer_divide(x: &str, a: &str, rounding: Rounding) -> String {
    let q = format!("(({x}) / ({a}))");
    if rounding == Rounding::Down {
        return q;
    }
    let r = format!("(({x}) % ({a}))");
    format!(
        "({q} + CASE WHEN {r} = 0 THEN 0 WHEN {} THEN {} ELSE 0 END)",
        steps(rounding, &q, &r, a),
        quotient_sign(x, a)
    )
}

/// A decimal rounded to a whole number as `rounding` says, still a
/// `numeric`: `round` rounds half away from zero, as `half_up` does, and
/// `trunc` toward zero, as `down` does.
fn round_whole(x: &str, rounding: Rounding) -> String {
    let t = format!("trunc({x})");
    let away = format!("(CASE WHEN ({x}) > 0 THEN 1 ELSE -1 END)");
    match rounding {
        Rounding::HalfUp => format!("round({x})"),
        Rounding::Down => t,
        Rounding::Up => format!("({t} + CASE WHEN ({x}) = {t} THEN 0 ELSE {away} END)"),
        Rounding::HalfEven => format!(
            "(CASE WHEN abs(({x}) - {t}) = 0.5 THEN {t} + CASE WHEN mod({t}, 2) = 0 THEN 0 ELSE {away} END ELSE round({x}) END)"
        ),
    }
}

/// `decimal.divide`: the quotient at the dividend's places, worked out
/// exactly, as the runtime does, from the two mantissas: `x / a` at `x`'s
/// scale `s` has the mantissa `x × 10^s ÷ a` rounded to a whole number,
/// which is `x × 10^(s + t) ÷ (a × 10^t)` with `t` the divisor's scale, so
/// both are whole and `div` and `mod` are exact. The mantissa is held to
/// 38 digits by `numeric(38, 0)`, as the runtime's decimal is.
fn decimal_divide(x: &str, a: &str, rounding: Rounding) -> String {
    let s = format!("scale(({x})::numeric)");
    let t = format!("scale(({a})::numeric)");
    let n = format!("trunc(({x})::numeric * power(10::numeric, {s} + {t}))");
    let d = format!("trunc(({a})::numeric * power(10::numeric, {t}))");
    let q = format!("div({n}, {d})");
    let r = format!("mod({n}, {d})");
    let mantissa = if rounding == Rounding::Down {
        q
    } else {
        format!(
            "({q} + CASE WHEN {r} = 0 THEN 0 WHEN {} THEN {} ELSE 0 END)",
            steps(rounding, &q, &r, &d),
            quotient_sign(&n, &d)
        )
    };
    format!("round(({mantissa})::numeric(38, 0) * power(10::numeric, -{s}), {s})")
}

/// `decimal.round`: a value with at most `scale` places is kept as it is,
/// places and all, where PostgreSQL's `round` would add zeros; otherwise
/// rounded to `scale` places as `rounding` says.
fn decimal_round(x: &str, scale: u32, rounding: Rounding) -> String {
    // One unit in the last place kept, and half of one, as literals with
    // exactly that many places.
    let unit = place_literal(scale, "1");
    let half = place_literal(scale + 1, "5");
    let t = format!("trunc({x}, {scale})");
    let away = format!("(CASE WHEN ({x}) > 0 THEN {unit} ELSE -{unit} END)");
    let rounded = match rounding {
        Rounding::HalfUp => format!("round({x}, {scale})"),
        Rounding::Down => t,
        Rounding::Up => format!("({t} + CASE WHEN ({x}) = {t} THEN 0 ELSE {away} END)"),
        Rounding::HalfEven => format!(
            "(CASE WHEN abs(({x}) - {t}) = {half} THEN {t} + CASE WHEN mod({t} / {unit}, 2) = 0 THEN 0 ELSE {away} END ELSE round({x}, {scale}) END)"
        ),
    };
    format!("(CASE WHEN scale({x}) <= {scale} THEN {x} ELSE {rounded} END)")
}

/// A decimal as a type with a fixed `scale` keeps it (area 2, "A fixed
/// `scale` rounds first"; chunk 5.8b): exactly `scale` places, rounded
/// half to even, as the runtime keeps a value it builds or stores, so
/// 2.50 kept with no places is 2 and 3.50 is 4; a whole number becomes a
/// decimal with that many places. Where the runtime stops, past 38 digits,
/// the cast to `numeric(38, scale)` stops the statement. The value is
/// named once in a subquery, since rounding reads it several times.
pub fn kept_at_scale(x: &str, scale: u32) -> Option<String> {
    (scale <= 38).then(|| {
        let rounded = decimal_round("\"k\".\"x\"", scale, Rounding::HalfEven);
        format!(
            "(SELECT ({rounded})::numeric(38, {scale}) FROM (SELECT ({x})::numeric AS \"x\") AS \"k\")"
        )
    })
}

/// `digit` in the `places`th place after the point, as a literal:
/// `place_literal(2, "1")` is `0.01`, and with no places `1`.
fn place_literal(places: u32, digit: &str) -> String {
    if places == 0 {
        return digit.to_string();
    }
    format!("0.{}{digit}", "0".repeat(places as usize - 1))
}

/// `temporal.period_between(start, end)` as its three columns, worked out
/// as the runtime does rather than with `age`, which borrows days from the
/// start's month (29 January to 1 March 2028 is `age`'s 1 month 3 days,
/// the runtime's `P1M1D`): the months between the two months, one fewer
/// when the end's day of the month hasn't reached the start's (going
/// forward) or one more when it has passed it (going back); then one step
/// back toward zero where adding them to the start, years first and then
/// months, each clamped to a month's last day as `+` adds a period, would
/// pass the end; the days are what is left. The runtime steps until it no
/// longer passes; one step is always enough, since the clamped day is
/// never past the start's day of the month.
fn period_between(start: &str, end: &str) -> Vec<String> {
    let reach = |months: &str| {
        format!(
            "(v.s + make_interval(years => (({months}) / 12)::integer) + make_interval(months => (({months}) % 12)::integer))::date"
        )
    };
    // A month's number since the year 0, which PostgreSQL counts as 1 BC
    // and `extract` gives as -1.
    let month = |d: &str| {
        format!(
            "((extract(year FROM {d})::bigint + CASE WHEN extract(year FROM {d}) < 0 THEN 1 ELSE 0 END) * 12 + extract(month FROM {d})::bigint)"
        )
    };
    let m0 = format!(
        "({} - {} + CASE WHEN v.e >= v.s AND extract(day FROM v.e) < extract(day FROM v.s) THEN -1 WHEN v.e < v.s AND extract(day FROM v.e) > extract(day FROM v.s) THEN 1 ELSE 0 END)",
        month("v.e"),
        month("v.s")
    );
    let m1 = format!(
        "CASE WHEN v.e >= v.s AND {reached} > v.e THEN m.m0 - 1 WHEN v.e < v.s AND {reached} < v.e THEN m.m0 + 1 ELSE m.m0 END",
        reached = reach("m.m0")
    );
    let from = format!(
        "FROM (SELECT ({start})::date AS s, ({end})::date AS e) AS v, LATERAL (SELECT {m0} AS m0) AS m, LATERAL (SELECT {m1} AS m1) AS n"
    );
    vec![
        format!("(SELECT n.m1 / 12 {from})"),
        format!("(SELECT n.m1 % 12 {from})"),
        format!("(SELECT (v.e - {})::bigint {from})", reach("n.m1")),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use jadpo_std::OPERATIONS;

    fn value(sql: &str, op: Op, args: &[Arg<'_>]) {
        assert_eq!(
            operation(op, args).as_ref().and_then(Form::value),
            Some(sql),
            "{op:?}"
        );
    }

    #[test]
    fn a_form_is_written_from_its_arguments() {
        let text = Held::Builtin(Builtin::Text);
        value(
            "char_length(\"name\")::bigint",
            Op::TextLength,
            &[Arg::Value("\"name\"", text)],
        );
        value(
            "(((\"a\")::bigint) / ((\"b\")::bigint))",
            Op::IntegerDivide,
            &[
                Arg::Value("\"a\"", Held::Builtin(Builtin::Int)),
                Arg::Value("\"b\"", Held::Builtin(Builtin::Int)),
                Arg::Rounding(Rounding::Down),
            ],
        );
        value(
            "(\"price\")::text",
            Op::TextFrom,
            &[
                Arg::Value("\"price\"", Held::Builtin(Builtin::Decimal)),
                Arg::Absent,
            ],
        );
        // A date's month, for an aggregate query's time bucket; a local
        // time's month has none (chunk 5.6c).
        value(
            "extract(month FROM \"made_on\")::bigint",
            Op::TemporalMonth,
            &[Arg::Value(
                "\"made_on\"",
                Held::Builtin(Builtin::CalendarDate),
            )],
        );
        assert_eq!(
            operation(
                Op::TemporalYear,
                &[Arg::Value("x", Held::Builtin(Builtin::ZonedTime))]
            ),
            None
        );
        // A time has no form for `text.from`, nor a rounding mode that
        // isn't written out, nor an operation the page gives none.
        assert_eq!(
            operation(
                Op::TextFrom,
                &[
                    Arg::Value("x", Held::Builtin(Builtin::Instant)),
                    Arg::Absent
                ]
            ),
            None
        );
        assert_eq!(
            operation(
                Op::IntegerDivide,
                &[
                    Arg::Value("a", Held::Builtin(Builtin::Int)),
                    Arg::Value("b", Held::Builtin(Builtin::Int)),
                    Arg::Value("r", Held::Builtin(Builtin::Text)),
                ]
            ),
            None
        );
        assert_eq!(
            operation(Op::TextLower, &[Arg::Value("x", text), Arg::Absent]),
            None
        );
        assert_eq!(
            operation(
                Op::MoneyMinor,
                &[Arg::Parts(&[
                    "\"p_amount\"".into(),
                    "\"p_currency\"".into()
                ])]
            ),
            Some(Form::Value("\"p_amount\"".into()))
        );
        let int = Held::Builtin(Builtin::Int);
        assert_eq!(
            operator(
                Operator::Add,
                &[Arg::Value("\"a\"", int), Arg::Value("\"b\"", int)]
            )
            .as_deref(),
            Some("((\"a\")::bigint + (\"b\")::bigint)")
        );
        let flag = Held::Builtin(Builtin::Bool);
        assert_eq!(
            operator(
                Operator::And,
                &[Arg::Value("\"p\"", flag), Arg::Value("\"q\"", flag)]
            )
            .as_deref(),
            Some("(CASE WHEN \"p\" THEN \"q\" ELSE FALSE END)")
        );
        // Text compares in code points; a decimal's `*` has no form.
        assert_eq!(
            operator(
                Operator::Equal,
                &[Arg::Value("\"t\"", text), Arg::Value("'x'", text)]
            )
            .as_deref(),
            Some("((\"t\") = ('x') COLLATE \"C\")")
        );
        let decimal = Held::Builtin(Builtin::Decimal);
        assert_eq!(
            operator(
                Operator::Multiply,
                &[Arg::Value("\"d\"", decimal), Arg::Value("\"e\"", decimal)]
            ),
            None
        );
        assert_eq!(
            operator(
                Operator::Less,
                &[Arg::Value("\"t\"", text), Arg::Value("'x'", text)]
            ),
            None
        );
        assert_eq!(
            case(&[("\"p\"".into(), "1".into())], "2"),
            "(CASE WHEN \"p\" THEN 1 ELSE 2 END)"
        );
        assert_eq!(place_literal(0, "1"), "1");
        assert_eq!(place_literal(3, "5"), "0.005");
        assert!(TRIMMED_CHARACTERS.chars().all(char::is_whitespace));
        assert_eq!(
            (0..=0x10ffff_u32)
                .filter_map(char::from_u32)
                .filter(|c| c.is_whitespace())
                .count(),
            TRIMMED_CHARACTERS.chars().count()
        );
    }

    /// The operations area 19's tables give a SQL form are those
    /// [`has_form`] names: a row whose SQL form isn't `none` names each
    /// of its operations in its first cell, with its namespace or under a
    /// heading that gives it.
    #[test]
    fn the_page_and_the_renderer_name_the_same_operations() {
        let page = include_str!("../../../docs/language/19-standard-library.md");
        let heading_namespace = |heading: &str| -> Option<&'static str> {
            Some(match heading {
                "Lists" => "list",
                "Sets" => "set",
                "Maps" => "map",
                "Text" => "text",
                "Encodings" => "encoding",
                "Hashing and cryptography" => "hash",
                _ => return None,
            })
        };
        let mut namespace: Option<&'static str> = None;
        let mut in_table = false;
        let mut listed: Vec<Op> = Vec::new();
        for line in page.lines() {
            if let Some(heading) = line.strip_prefix("## ") {
                namespace = heading_namespace(heading.trim());
                continue;
            }
            if line.starts_with("| Operation | Gives | Fails | SQL form |") {
                in_table = true;
                continue;
            }
            if !line.starts_with('|') {
                in_table = false;
                continue;
            }
            if !in_table || line.starts_with("|---") {
                continue;
            }
            let cells: Vec<&str> = line.trim_matches('|').split('|').map(str::trim).collect();
            let form = cells.last().copied().unwrap_or("none");
            if form == "none" {
                continue;
            }
            for call in cells[0].split('`').skip(1).step_by(2) {
                let name = call.split('(').next().unwrap_or(call);
                let (space, name) = match name.split_once('.') {
                    Some((space, name)) => (space, name),
                    None => (namespace.unwrap_or_else(|| panic!("{line}")), name),
                };
                let found = OPERATIONS
                    .iter()
                    .find(|o| o.namespace == space && o.name == name)
                    .unwrap_or_else(|| panic!("`{space}.{name}` on the page isn't an operation"));
                listed.push(found.op);
            }
        }
        listed.sort();
        listed.dedup();
        let mut built: Vec<Op> = OPERATIONS
            .iter()
            .map(|o| o.op)
            .filter(|op| has_form(*op))
            .collect();
        built.sort();
        built.dedup();
        assert_eq!(listed, built);
    }
}
