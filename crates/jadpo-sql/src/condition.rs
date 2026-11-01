//! Conditions (area 6, "Filters"; area 5, "Lifecycle"): the model of a
//! condition on one entity's fields, which the checker reads from a
//! query's `where:` lines and a lifecycle's `visible:` and `when:`, and one
//! renderer for it, which the plan's statements and the predicate of a
//! partial index both use, so each reads the same wherever it lands.
//!
//! A condition keeps Jadpo's meaning where a field is `none`: it meets no
//! comparison but `!=`, and `not` is pushed down to the comparisons, so
//! SQL's three-valued logic never shows. A literal or an enum value is
//! decoded for the field it is compared with (`jadpo_std::constant`) and
//! made the value its column is given by the storage codec
//! (`jadpo_std::standard::storage::bind`), which the runtime binds every
//! parameter with (area 19, chunk 5.5w3b); a statement binds it as a
//! parameter, and a predicate, which takes none, writes it into its text
//! ([`crate::literal`]). Anything else (a parameter, `clock.now`) is a
//! parameter the statement binds, through [`Bind`].
//!
//! A standard operation of a field ([`Condition::Call`]) is lowered one
//! way for every operation (chunk 6.3b), by [`lowering`]: either its SQL
//! form ([`crate::form`]), which gives exactly what the code gives and is
//! compared in the statement (an operation with an exact form, and
//! `ai.distance` as pgvector works it out, area 21), or a search an index
//! serves that never leaves out a record the code keeps, each row it finds
//! then held to the operation by the runtime, in the same code a function
//! body runs (the `geo.` tests, area 20), as the read plan's
//! `jadpo_std::read::Recheck` says.

use std::collections::BTreeSet;

use jadpo_std::read::{EnvelopePart, GEO_MARGIN, SqlType};
use jadpo_std::standard::geo::{BOUNDING_BOX, CIRCLE, POINT};
use jadpo_std::standard::storage::{self, Stored};
use jadpo_std::{Builtin, Literal, Op, Rounding, SqlValue, VectorDistance};

use crate::form::{self, Arg, Held};
use crate::ident;

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

    /// Whether it is written out, fixed while compiling: a literal, an
    /// enum value or a list of those, never a parameter or `clock.now`.
    pub fn written_out(&self) -> bool {
        match self {
            Value::Literal(_) | Value::Variant { .. } => true,
            Value::List(items) => items.iter().all(Value::written_out),
            Value::Param { .. } | Value::Now => false,
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
    /// A standard operation of one of the entity's fields and what it asks
    /// of the result (chunk 6.3b): `ai.distance(Entity.field, value) <=
    /// bound` (area 21), a `geo.` test (area 20), or an operation with an
    /// exact SQL form where a field would stand (area 6, "Operations in
    /// conditions"), each lowered as [`lowering`] says. A record whose
    /// field is `none` never meets one, `not` included, since code can't
    /// call the operation on `none`.
    Call {
        call: Call,
        test: Test,
    },
    /// `Entity matches phrase` (area 16, "Search"; chunk B12): the
    /// record's searchable words meet what a `SearchText` asks for. It
    /// stands on a `where:` line of its own, never under `not`, `or` or
    /// `where_present:`.
    Matches(Search),
}

/// An entity's search, as a `matches` condition tests it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Search {
    /// The fields its words come from, by name now, in `fields:` order:
    /// what the condition tests, so policy proves each one readable.
    pub fields: Vec<String>,
    pub language: jadpo_std::SearchLanguage,
    /// The `SearchText` searched for: a parameter, or a field inside one.
    pub value: Value,
}

/// A standard operation called on one of an entity's fields, its first
/// argument (chunk 6.3b).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Call {
    pub op: Op,
    /// The field it is called on.
    pub field: String,
    /// Its other arguments, in the operation's parameter order.
    pub args: Vec<CallArg>,
    /// How its result is held, which a value compared with it is decoded
    /// and sent for.
    pub result: Held,
}

/// One of a call's arguments after the field.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum CallArg {
    /// A value, held as its parameter's type says: written out (a literal,
    /// an enum value), or given when the query runs (a parameter, or a
    /// field inside one).
    Value(Value, Held),
    /// A rounding mode written out.
    Rounding(Rounding),
    /// A whole number written out (`decimal.round`'s `scale:`).
    Whole(i64),
    /// One character written out (`text.pad_start`'s `fill:`).
    Character(char),
    /// How `ai.distance` compares its vectors, from their type.
    Metric(VectorDistance),
    /// An option the call leaves out.
    Absent,
}

/// What a condition asks of an operation's result.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Test {
    /// That it is `true`: an operation that gives a `Bool`, standing as
    /// the condition itself.
    Holds,
    /// That it compares with a value: `text.length(Book.title) <= 24`.
    Compare { op: Compare, value: Value },
    /// That it is one of a list's values.
    In { value: Value },
}

/// How the database decides a condition on an operation (chunk 6.3b): one
/// of two ways, for every operation a condition may call.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Lowering {
    /// Its SQL form, compared in the statement: area 19's form, which
    /// gives exactly what the code gives, or `ai.distance` as pgvector
    /// works it out, which area 21 decided a query's distance is.
    Exact,
    /// A search an index serves, a margin wider than the condition, so it
    /// never leaves out a record the code keeps; the runtime holds each row
    /// it finds to the operation, run by the code a function body runs, and
    /// asks again when that leaves the read short (area 20, "Queries",
    /// chunk 5.5u5).
    Search,
}

/// How a condition calling `op` is decided; `None` for an operation no
/// condition may call.
pub fn lowering(op: Op) -> Option<Lowering> {
    match op {
        Op::GeoWithin | Op::GeoContains | Op::GeoIntersects => Some(Lowering::Search),
        Op::AiDistance => Some(Lowering::Exact),
        other if form::has_form(other) => Some(Lowering::Exact),
        _ => None,
    }
}

impl Call {
    /// The value it is compared with, or searched against, first: what a
    /// `geo.` test tests its field against.
    pub fn value(&self) -> Option<&Value> {
        self.args.iter().find_map(|arg| match arg {
            CallArg::Value(value, _) => Some(value),
            _ => None,
        })
    }

    /// Whether every argument but the field is written out, so its
    /// expression is fixed while compiling and an index can hold it.
    pub fn written_out(&self) -> bool {
        self.args.iter().all(|arg| match arg {
            CallArg::Value(value, _) => value.written_out(),
            _ => true,
        })
    }

    /// Whether a `BoundingBox` search, which searches the column cast to
    /// `geometry`, and so an index of its own (area 20, "Indexes").
    pub fn is_box(&self) -> bool {
        self.op == Op::GeoWithin
            && matches!(
                self.args.first(),
                Some(CallArg::Value(_, Held::Shape(BOUNDING_BOX)))
            )
    }

    /// The operation's name in the namespace it is called through:
    /// `length` for `text.length`.
    pub fn name(&self) -> &'static str {
        jadpo_std::OPERATIONS
            .iter()
            .find(|operation| operation.op == self.op)
            .map_or("call", |operation| operation.name)
    }
}

/// `value`, a parameter or a field inside one, with one more field:
/// `view.west`.
fn inside(value: &Value, field: &str) -> Option<Value> {
    match value {
        Value::Param { index, path } => {
            let mut path = path.clone();
            path.push(field.to_string());
            Some(Value::Param {
                index: *index,
                path,
            })
        }
        _ => None,
    }
}

impl Condition {
    /// The fields it tests.
    pub fn fields(&self, out: &mut BTreeSet<String>) {
        match self {
            Condition::Compare { field, .. }
            | Condition::IsNone { field, .. }
            | Condition::In { field, .. }
            | Condition::Call {
                call: Call { field, .. },
                ..
            } => {
                out.insert(field.clone());
            }
            Condition::And(all) | Condition::Or(all) => {
                all.iter().for_each(|c| c.fields(out));
            }
            Condition::Not(inner)
            | Condition::Present {
                condition: inner, ..
            } => inner.fields(out),
            Condition::Matches(search) => out.extend(search.fields.iter().cloned()),
        }
    }

    /// The calls it holds, each with its lowering, anywhere inside.
    pub fn calls<'c>(&'c self, out: &mut Vec<&'c Call>) {
        match self {
            Condition::Call { call, .. } => out.push(call),
            Condition::And(all) | Condition::Or(all) => {
                all.iter().for_each(|c| c.calls(out));
            }
            Condition::Not(inner)
            | Condition::Present {
                condition: inner, ..
            } => inner.calls(out),
            _ => {}
        }
    }

    /// Whether the runtime decides any part of it on the rows a search
    /// finds ([`Lowering::Search`]).
    pub fn searches(&self) -> bool {
        let mut calls = Vec::new();
        self.calls(&mut calls);
        calls
            .iter()
            .any(|call| lowering(call.op) == Some(Lowering::Search))
    }
}

/// The calls a query's lines search for, which the runtime holds each row
/// it finds to: each is a line of its own or a part of an `and`, which is
/// where the checker lets a search stand (area 20, "Queries").
pub fn search_calls(filter: &[Condition]) -> Vec<&Call> {
    let mut out = Vec::new();
    for line in filter {
        match line {
            Condition::Call { call, .. } if lowering(call.op) == Some(Lowering::Search) => {
                out.push(call)
            }
            Condition::And(parts) => out.extend(search_calls(parts)),
            _ => {}
        }
    }
    out
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
    /// The built-in type the field's type is built on, which a literal is
    /// decoded for (`"2026-01-15"` where a `CalendarDate` goes is a date);
    /// `None` for an enum, an object or a container.
    pub builtin: Option<Builtin>,
    /// The places its values keep, for a decimal: `5` where `scale: 2`
    /// goes is `5.00`.
    pub scale: Option<u32>,
}

impl Column<'_> {
    /// The type a value compared with the column is sent as.
    fn param(&self) -> Option<SqlType> {
        self.ty.map(SqlType::param)
    }

    /// How an operation's form is given the column's value: the built-in
    /// type it is built on, text for an enum, `jsonb` for a list, a set or
    /// a map, and a vector.
    fn held(&self) -> Option<Held> {
        Some(match (self.ty?, self.builtin) {
            (SqlType::Json, _) => Held::Json,
            (SqlType::Vector, _) => Held::Vector,
            (_, Some(builtin)) => Held::Builtin(builtin),
            (SqlType::Text, None) => Held::Builtin(Builtin::Text),
            _ => return None,
        })
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

    /// `$n` for a literal or an enum value, as the storage codec made it:
    /// a parameter bound to exactly `value`, or, where the statement takes
    /// no parameters, `value` written into its text ([`crate::literal`]).
    /// `None` when neither can hold it.
    fn stored(&mut self, value: SqlValue) -> Option<String>;

    /// `$n` for one side of the envelopes a `BoundingBox` filter searches,
    /// worked out from the box `bounds` when the statement runs (area 20,
    /// chunk 5.5u5). `None` when the statement can't take it.
    fn envelope(&mut self, _bounds: &Value, _part: EnvelopePart) -> Option<String> {
        None
    }

    /// `$n` for the double a distance is compared with, worked out from the
    /// decimal or whole number `bound` holds when the statement runs
    /// ([`jadpo_std::standard::ai::distance_bound`], chunk 5.8c). `None`
    /// when the statement can't take it.
    fn distance_bound(&mut self, _bound: &Value, _strict: bool) -> Option<String> {
        None
    }

    /// `$n` for what a `matches` condition searches for, the `SearchText`
    /// `value` holds as `websearch_to_tsquery` is given it (area 16,
    /// chunk B12). `None` when the statement can't take it.
    fn search_phrase(&mut self, _value: &Value) -> Option<String> {
        None
    }
}

/// For text that takes no parameters: a partial index's predicate, which
/// must mean the same at every moment, and an index's expression. A literal
/// is written into it.
#[derive(Debug)]
pub struct Fixed;

impl Bind for Fixed {
    fn bind(&mut self, _value: &Value, _ty: SqlType) -> Option<String> {
        None
    }

    fn stored(&mut self, value: SqlValue) -> Option<String> {
        crate::literal::sql(&value)
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
            let rendered = self::value(bind, value, &field)?;
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
            let text = among(bind, &column, value, &field, negated)?;
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
        Condition::Call { call, test } => {
            let field = entity.column(&call.field)?;
            let column = ident(field.name);
            match lowering(call.op)? {
                // No index serves the negation, and the code's test is
                // worked out again anyway: the checker keeps a search out
                // from under `not`.
                Lowering::Search => {
                    if negated || *test != Test::Holds {
                        return None;
                    }
                    search(bind, &column, call)?
                }
                Lowering::Exact => {
                    let expression = expression(bind, entity, call)?;
                    let text = tested(bind, &expression, call, test, negated)?;
                    // A record whose field is `none` meets nothing, `not`
                    // included. pgvector's operators are strict, so its
                    // distance of `NULL` is `NULL` and meets nothing
                    // already; a form is given values, never `NULL`.
                    if field.nullable && call.op != Op::AiDistance {
                        format!("({column} IS NOT NULL AND {text})")
                    } else {
                        text
                    }
                }
            }
        }
        // Never negated: the checker keeps it out from under `not`.
        Condition::Matches(search) => {
            if negated {
                return None;
            }
            crate::search::matches(&search_query(bind, search)?)
        }
        Condition::Present { value, condition } => {
            let ty = match condition.as_ref() {
                Condition::Compare { field: name, .. } => entity.column(name)?.param()?,
                Condition::In { field: name, .. } => entity.column(name)?.param()?.array(),
                Condition::Call {
                    call,
                    test: Test::Compare { .. },
                } => result_column(call).param()?,
                Condition::Call {
                    call,
                    test: Test::In { .. },
                } => result_column(call).param()?.array(),
                _ => return None,
            };
            let n = bind.bind(value, ty)?;
            let inner = self::condition(bind, entity, condition, negated)?;
            format!("({n} IS NULL OR {inner})")
        }
    })
}

/// What `search` asks for, as SQL: its phrase bound as a parameter.
pub fn search_query(bind: &mut dyn Bind, search: &Search) -> Option<String> {
    let phrase = bind.search_phrase(&search.value)?;
    Some(crate::search::query(search.language, &phrase))
}

/// `column` among the values `value` holds: a list written out, or a
/// parameter holding one.
fn among(
    bind: &mut dyn Bind,
    column: &str,
    value: &Value,
    field: &Column<'_>,
    negated: bool,
) -> Option<String> {
    let ty = field.param()?;
    Some(match value {
        Value::List(items) => {
            if items.is_empty() {
                return Some(if negated { "TRUE" } else { "FALSE" }.to_string());
            }
            let mut rendered = Vec::new();
            for item in items {
                rendered.push(self::value(bind, item, field)?);
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
    })
}

/// The column a call's result is compared as: of the type its result is
/// held in, the literal it is compared with decoded for that type.
fn result_column(call: &Call) -> Column<'static> {
    let (ty, builtin) = match call.result {
        Held::Builtin(builtin) => (
            storage::builtin_column(builtin.representation()),
            Some(builtin),
        ),
        Held::Double => (Some(SqlType::Float8), None),
        Held::Json => (Some(SqlType::Json), None),
        Held::Vector => (Some(SqlType::Vector), None),
        Held::Range(_) | Held::Shape(_) => (None, None),
    };
    Column {
        name: "",
        nullable: false,
        ty,
        builtin,
        scale: None,
    }
}

/// What `test` asks of a call's `expression`, `negated` when a `not` above
/// it was pushed down: the expression is never `NULL` here, the field
/// being present, so `not` turns each comparison round.
fn tested(
    bind: &mut dyn Bind,
    expression: &str,
    call: &Call,
    test: &Test,
    negated: bool,
) -> Option<String> {
    Some(match test {
        // `IS FALSE` rather than `NOT`: PostgreSQL turns `NOT (a = b)` into
        // `a <> b` before it looks for an index, which then no longer
        // matches the index on `(a = b)`; it serves `IS FALSE` as it serves
        // the expression itself. The expression is never `NULL` here.
        Test::Holds => {
            if negated {
                format!("({expression}) IS FALSE")
            } else {
                expression.to_string()
            }
        }
        Test::Compare { op, value } if call.result == Held::Double => {
            distance(bind, expression, call, *op, value, negated)?
        }
        Test::Compare { op, value } => {
            let op = if negated { op.negated() } else { *op };
            let rendered = self::value(bind, value, &result_column(call))?;
            format!("{expression} {} {rendered}", op.sql())
        }
        Test::In { value } => among(bind, expression, value, &result_column(call), negated)?,
    })
}

/// A distance pgvector worked out compared with a bound (area 21, chunk
/// 5.8c): each comparison is "at most" or "above" the double that agrees
/// with code's decimal comparison, sent as that double so PostgreSQL
/// rounds nothing; `NaN` (a cosine from a vector of zeros) and an infinity
/// meet none of them, as in code, where neither is a decimal.
fn distance(
    bind: &mut dyn Bind,
    distance: &str,
    call: &Call,
    op: Compare,
    bound: &Value,
    negated: bool,
) -> Option<String> {
    // `none` never meets it, so a pushed-down `not` only flips the
    // comparison.
    let op = if negated { op.negated() } else { op };
    let (above, strict) = match op {
        Compare::LessEqual => (false, false),
        Compare::Less => (false, true),
        Compare::Greater => (true, false),
        Compare::GreaterEqual => (true, true),
        Compare::Equal | Compare::NotEqual => return None,
    };
    let bound = match bound {
        Value::Literal(Literal::Number { value, .. }) => bind.stored(SqlValue::Float8(
            jadpo_std::standard::ai::distance_bound(&value.to_string(), strict)?.to_bits(),
        ))?,
        Value::Param { .. } => bind.distance_bound(bound, strict)?,
        _ => return None,
    };
    let metric = call.args.iter().find_map(|arg| match arg {
        CallArg::Metric(metric) => Some(*metric),
        _ => None,
    })?;
    // PostgreSQL sorts `NaN` above infinity, and infinity above every
    // number: neither meets any comparison.
    Some(if above {
        format!("({distance} > {bound} AND {distance} < 'Infinity'::float8)")
    } else if metric == VectorDistance::InnerProduct {
        format!("({distance} <= {bound} AND {distance} > '-Infinity'::float8)")
    } else {
        format!("{distance} <= {bound}")
    })
}

/// A call's SQL form on its field's column, its other arguments as SQL: a
/// value given when the query runs bound as a parameter, and one written
/// out written into the text, so an index on the expression and the
/// statement's own are the same text (chunk 6.3b). `None` when the
/// operation has no form for these arguments.
pub fn expression(bind: &mut dyn Bind, entity: &dyn Columns, call: &Call) -> Option<String> {
    let field = entity.column(&call.field)?;
    let column = ident(field.name);
    let mut rendered: Vec<Option<String>> = Vec::with_capacity(call.args.len());
    for arg in &call.args {
        rendered.push(match arg {
            CallArg::Value(value, held) => Some(argument(bind, value, *held)?),
            _ => None,
        });
    }
    let mut args = vec![Arg::Value(&column, field.held()?)];
    for (arg, sql) in call.args.iter().zip(&rendered) {
        args.push(match (arg, sql) {
            (CallArg::Value(_, held), Some(sql)) => Arg::Value(sql, *held),
            (CallArg::Rounding(rounding), _) => Arg::Rounding(*rounding),
            (CallArg::Whole(n), _) => Arg::Whole(*n),
            (CallArg::Character(c), _) => Arg::Character(*c),
            (CallArg::Metric(metric), _) => Arg::Metric(*metric),
            _ => Arg::Absent,
        });
    }
    form::operation(call.op, &args)?.value().map(str::to_string)
}

/// An argument after the field as SQL: written out, as the storage codec
/// makes it, when it is fixed while compiling; bound when it is given as
/// the query runs.
fn argument(bind: &mut dyn Bind, value: &Value, held: Held) -> Option<String> {
    let ty = match held {
        Held::Builtin(builtin) => storage::builtin_column(builtin.representation())?,
        Held::Vector => SqlType::Vector,
        Held::Shape(_) => SqlType::Geography,
        Held::Json => SqlType::Json,
        Held::Double => SqlType::Float8,
        Held::Range(_) => return None,
    };
    let builtin = match held {
        Held::Builtin(builtin) => Some(builtin),
        _ => None,
    };
    let fixed = |stored: SqlValue| crate::literal::sql(&stored);
    match value {
        Value::Literal(literal) => {
            let decoded = jadpo_std::constant::literal(literal, builtin, None)?;
            fixed(storage::bind(&decoded.stored()?, ty).ok()?)
        }
        Value::Variant {
            ty: enum_ty,
            variant,
        } => fixed(
            storage::bind(
                &Stored::Variant {
                    ty: enum_ty,
                    name: variant,
                },
                ty,
            )
            .ok()?,
        ),
        Value::List(_) => None,
        other => bind.bind(other, ty),
    }
}

/// The expression an index on a call holds (area 6, "Operations in
/// conditions", chunk 6.3b): its SQL form on its field's column, every
/// other argument written out. `None` for a call an index can't hold: a
/// search, which its own index serves, one given a value when the query
/// runs, or one whose form PostgreSQL won't index ([`form::indexable`]).
pub fn index_expression(entity: &dyn Columns, call: &Call) -> Option<String> {
    if lowering(call.op)? != Lowering::Exact || !call.written_out() {
        return None;
    }
    let field = entity.column(&call.field)?;
    if !form::indexable(call.op, field.held()?) {
        return None;
    }
    expression(&mut Fixed, entity, call)
}

/// The index a search reads (area 20, "Indexes", chunk 5.5u5): a GiST
/// index on the field's column, cast to `geometry` for a box, whose
/// envelopes compare longitude and latitude directly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SearchIndex {
    /// The index's method.
    pub method: &'static str,
    /// The type the column is cast to, when it is.
    pub cast: Option<&'static str>,
}

/// The index `call`'s search reads; `None` for a call decided in SQL.
pub fn search_index(call: &Call) -> Option<SearchIndex> {
    (lowering(call.op)? == Lowering::Search).then(|| SearchIndex {
        method: "gist",
        cast: call.is_box().then_some("geometry"),
    })
}

/// A `geo.` test on `column` as a condition an index serves, which never
/// leaves out a record the code's test keeps (area 20, "Queries", chunk
/// 5.5u5): a distance searched [`GEO_MARGIN`] further, a shape searched
/// for anything within [`GEO_MARGIN`] of it (whatever is inside it or on
/// its edge, and whatever `ST_Covers` would wrongly leave out), and a box
/// by its envelopes on the column cast to `geometry`.
fn search(bind: &mut dyn Bind, column: &str, call: &Call) -> Option<String> {
    let value = call.value()?;
    let reach = match call.args.get(1) {
        Some(CallArg::Value(reach, held)) => Some((reach, *held)),
        _ => None,
    };
    let shape = match call.args.first() {
        Some(CallArg::Value(_, Held::Shape(shape))) => *shape,
        _ => return None,
    };
    Some(match (call.op, shape, reach) {
        (Op::GeoWithin, POINT, Some((distance, held))) => {
            let length = Column {
                name: column,
                nullable: false,
                ty: Some(SqlType::Numeric),
                builtin: match held {
                    Held::Builtin(builtin) => Some(builtin),
                    _ => None,
                },
                scale: None,
            };
            let point = bind.bind(value, SqlType::Geography)?;
            let distance = self::value(bind, distance, &length)?;
            format!("ST_DWithin({column}, {point}, {distance} + {GEO_MARGIN})")
        }
        (Op::GeoWithin, CIRCLE, None) => {
            let centre = bind.bind(&inside(value, "centre")?, SqlType::Geography)?;
            let radius = bind.bind(&inside(value, "radius")?, SqlType::Numeric)?;
            format!("ST_DWithin({column}, {centre}, {radius} + {GEO_MARGIN})")
        }
        (Op::GeoWithin, BOUNDING_BOX, None) => {
            let south = bind.bind(&inside(value, "south")?, SqlType::Numeric)?;
            let north = bind.bind(&inside(value, "north")?, SqlType::Numeric)?;
            let mut envelopes = Vec::new();
            for (west, east) in [
                (EnvelopePart::FirstWest, EnvelopePart::FirstEast),
                (EnvelopePart::SecondWest, EnvelopePart::SecondEast),
            ] {
                let west = bind.envelope(value, west)?;
                let east = bind.envelope(value, east)?;
                envelopes.push(format!(
                    "{column}::geometry && ST_MakeEnvelope({west}, {south}, {east}, {north}, 4326)"
                ));
            }
            format!("({})", envelopes.join(" OR "))
        }
        (Op::GeoWithin | Op::GeoContains | Op::GeoIntersects, _, None) => {
            let shape = bind.bind(value, SqlType::Geography)?;
            format!("ST_DWithin({column}, {shape}, {GEO_MARGIN})")
        }
        _ => return None,
    })
}

/// PostGIS's distance from `point` to `column`'s point, which a read
/// ordered by `geo.distance` orders the rows it finds by before the
/// runtime sorts them by the code's (area 20, question 4; chunk 5.5u5).
pub fn measured(bind: &mut dyn Bind, column: &str, point: &Value) -> Option<String> {
    let from = bind.bind(point, SqlType::Geography)?;
    Some(format!("ST_Distance({}, {from})", ident(column)))
}

/// `column` holding a value: a nearest-neighbour read leaves out a record
/// whose vector is `none`, which is near nothing (area 21).
pub fn present(column: &str) -> String {
    format!("{} IS NOT NULL", ident(column))
}

/// A value as SQL, given to `column`: a literal or an enum value as the
/// storage codec makes it, then bound or written by `bind`; anything else
/// bound.
pub fn value(bind: &mut dyn Bind, value: &Value, column: &Column<'_>) -> Option<String> {
    let ty = column.param()?;
    match value {
        Value::Literal(literal) => bind.stored(self::literal(literal, column)?),
        Value::Variant {
            ty: enum_ty,
            variant,
        } => bind.stored(
            storage::bind(
                &Stored::Variant {
                    ty: enum_ty,
                    name: variant,
                },
                ty,
            )
            .ok()?,
        ),
        Value::List(_) => None,
        other => bind.bind(other, ty),
    }
}

/// A literal given to `column`, as the runtime would bind it: decoded for
/// the field's type, as the plan decodes a body's literal, and made its
/// column's value by the storage codec. `None` for a literal the column
/// can't hold, which the checker rules out.
pub fn literal(literal: &Literal, column: &Column<'_>) -> Option<SqlValue> {
    let decoded = jadpo_std::constant::literal(literal, column.builtin, column.scale)?;
    storage::bind(&decoded.stored()?, column.param()?).ok()
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
                builtin: Some(Builtin::Text),
                scale: None,
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

        fn stored(&mut self, _value: SqlValue) -> Option<String> {
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

    /// A table of one field of each kind a literal is written for.
    struct Kept;

    impl Columns for Kept {
        fn column(&self, field: &str) -> Option<Column<'_>> {
            let (name, ty, builtin, scale) = match field {
                "at" => ("at", SqlType::Timestamp, Some(Builtin::Instant), None),
                "on" => ("on", SqlType::Date, Some(Builtin::CalendarDate), None),
                "wait" => ("wait", SqlType::Int8, Some(Builtin::Duration), None),
                "price" => ("price", SqlType::Numeric, Some(Builtin::Decimal), Some(2)),
                "share" => ("share", SqlType::Numeric, Some(Builtin::Percentage), None),
                "shelf" => (
                    "shelf",
                    SqlType::Numeric,
                    Some(Builtin::Quantity(jadpo_std::QuantityKind::Length)),
                    None,
                ),
                "currency" => ("currency", SqlType::Text, None, None),
                "rank" => ("rank", SqlType::Int2, Some(Builtin::Int), None),
                "key" => ("key", SqlType::Uuid, Some(Builtin::Uuid), None),
                _ => return None,
            };
            Some(Column {
                name,
                nullable: false,
                ty: Some(ty),
                builtin,
                scale,
            })
        }
    }

    fn equal(field: &str, value: Value) -> Condition {
        Condition::Compare {
            field: field.into(),
            op: Compare::Equal,
            value,
        }
    }

    fn number(text: &str) -> Literal {
        Literal::Number {
            value: jadpo_std::Num::parse(text).expect("a number"),
            decimal: text.contains('.'),
        }
    }

    /// Binds every literal as the next parameter, keeping its value.
    #[derive(Default)]
    struct Collected(Vec<SqlValue>);

    impl Bind for Collected {
        fn bind(&mut self, _value: &Value, _ty: SqlType) -> Option<String> {
            None
        }

        fn stored(&mut self, value: SqlValue) -> Option<String> {
            self.0.push(value);
            Some(format!("${}", self.0.len()))
        }
    }

    #[test]
    fn a_literal_is_bound_as_the_storage_codec_makes_it() {
        let cases: Vec<(&str, Value, SqlValue, &str)> = vec![
            (
                "at",
                Value::Literal(Literal::Text("2026-10-25T09:30:00Z".into())),
                SqlValue::Timestamp(1_792_920_600_000_000),
                "'2026-10-25 09:30:00.000000+00'::timestamptz",
            ),
            (
                "on",
                Value::Literal(Literal::Text("2026-10-25".into())),
                SqlValue::Date(20_751),
                "'2026-10-25'::date",
            ),
            (
                "wait",
                Value::Literal(Literal::Duration("15m".into())),
                SqlValue::Int8(900_000),
                "900000",
            ),
            (
                "price",
                Value::Literal(number("5")),
                SqlValue::Numeric("5.00".into()),
                "5.00",
            ),
            (
                "share",
                Value::Literal(Literal::Percentage("20%".into())),
                SqlValue::Numeric("0.20".into()),
                "0.20",
            ),
            (
                "shelf",
                Value::Literal(Literal::Quantity("2.5km".into())),
                SqlValue::Numeric("2500".into()),
                "2500",
            ),
            (
                "currency",
                Value::Variant {
                    ty: "Currency".into(),
                    variant: "gbp".into(),
                },
                SqlValue::Text("GBP".into()),
                "'GBP'",
            ),
            ("rank", Value::Literal(number("3")), SqlValue::Int8(3), "3"),
            (
                "key",
                Value::Literal(Literal::Text("0192F0C4-3B5E-7C1A-9D2E-4F6A8B0C1D2E".into())),
                SqlValue::Uuid([
                    0x01, 0x92, 0xf0, 0xc4, 0x3b, 0x5e, 0x7c, 0x1a, 0x9d, 0x2e, 0x4f, 0x6a, 0x8b,
                    0x0c, 0x1d, 0x2e,
                ]),
                "'0192f0c4-3b5e-7c1a-9d2e-4f6a8b0c1d2e'::uuid",
            ),
        ];
        for (field, value, bound, written) in cases {
            let condition = equal(field, value);
            let mut params = Collected::default();
            assert_eq!(
                self::condition(&mut params, &Kept, &condition, false).as_deref(),
                Some(format!("\"{field}\" = $1").as_str()),
                "{field}: a statement binds it"
            );
            assert_eq!(params.0, vec![bound], "{field}");
            assert_eq!(
                self::condition(&mut Fixed, &Kept, &condition, false),
                Some(format!("\"{field}\" = {written}")),
                "{field}: a predicate writes it"
            );
        }
    }

    #[test]
    fn text_in_a_predicate_cant_end_its_literal() {
        for (text, written) in [
            ("x' OR TRUE --", "'x'' OR TRUE --'"),
            ("x\\' OR TRUE --", "E'x\\\\'' OR TRUE --'"),
            ("ünï ☕", "'ünï ☕'"),
        ] {
            let condition = Condition::Compare {
                field: "title".into(),
                op: Compare::Equal,
                value: Value::Literal(Literal::Text(text.into())),
            };
            assert_eq!(
                self::condition(&mut Fixed, &Notes, &condition, false),
                Some(format!("\"name\" = {written}")),
                "{text}"
            );
        }
        let nul = Condition::Compare {
            field: "title".into(),
            op: Compare::Equal,
            value: Value::Literal(Literal::Text("a\0b".into())),
        };
        assert_eq!(
            self::condition(&mut Fixed, &Notes, &nul, false),
            None,
            "PostgreSQL text can't hold NUL, so no predicate is written"
        );
    }

    /// A table of one optional vector field, `meaning`.
    /// Binds every value as the next parameter, keeping the stored ones.
    #[derive(Default)]
    struct Sent(Vec<SqlValue>, usize);

    impl Bind for Sent {
        fn bind(&mut self, _value: &Value, _ty: SqlType) -> Option<String> {
            self.1 += 1;
            Some(format!("${}", self.1))
        }

        fn stored(&mut self, value: SqlValue) -> Option<String> {
            self.0.push(value);
            self.1 += 1;
            Some(format!("${}", self.1))
        }
    }

    struct Meanings;

    impl Columns for Meanings {
        fn column(&self, field: &str) -> Option<Column<'_>> {
            (field == "meaning").then_some(Column {
                name: "meaning",
                nullable: true,
                ty: Some(SqlType::Vector),
                builtin: None,
                scale: None,
            })
        }
    }

    /// `ai.distance(Meanings.meaning, $1) <op> bound`.
    fn distance(metric: jadpo_std::VectorDistance, op: Compare, bound: &str) -> Condition {
        Condition::Call {
            call: Call {
                op: Op::AiDistance,
                field: "meaning".into(),
                args: vec![
                    CallArg::Value(
                        Value::Param {
                            index: 0,
                            path: Vec::new(),
                        },
                        Held::Vector,
                    ),
                    CallArg::Metric(metric),
                ],
                result: Held::Double,
            },
            test: Test::Compare {
                op,
                value: Value::Literal(Literal::Number {
                    value: jadpo_std::Num::parse(bound).expect("a number"),
                    decimal: true,
                }),
            },
        }
    }

    #[test]
    fn a_distance_is_pgvector_s_operator_and_none_never_meets_it() {
        let near = distance(
            jadpo_std::VectorDistance::InnerProduct,
            Compare::LessEqual,
            "0.6",
        );
        // An inner product can be minus infinity, which no bound meets
        // (chunk 5.8c).
        assert_eq!(
            condition(&mut Params::default(), &Meanings, &near, false).as_deref(),
            Some("(\"meaning\" <#> $1 <= $2 AND \"meaning\" <#> $1 > '-Infinity'::float8)")
        );
        // `not` flips the comparison, with no `IS NULL`: a record without
        // a vector meets neither, and nor does an infinite or `NaN`
        // distance, which PostgreSQL sorts above every number.
        assert_eq!(
            condition(
                &mut Params::default(),
                &Meanings,
                &Condition::Not(Box::new(near.clone())),
                false
            )
            .as_deref(),
            Some("(\"meaning\" <#> $1 > $2 AND \"meaning\" <#> $1 < 'Infinity'::float8)")
        );
        // A cosine at most a bound can't be `NaN` or infinite, since both
        // sort above it; at least one can.
        let cosine =
            |op: Compare| distance(jadpo_std::VectorDistance::Cosine, op, "0.60000000000000005");
        let mut sent = Sent::default();
        assert_eq!(
            condition(&mut sent, &Meanings, &cosine(Compare::LessEqual), false).as_deref(),
            Some("\"meaning\" <=> $1 <= $2")
        );
        assert_eq!(
            condition(&mut sent, &Meanings, &cosine(Compare::GreaterEqual), false).as_deref(),
            Some("(\"meaning\" <=> $3 > $4 AND \"meaning\" <=> $3 < 'Infinity'::float8)")
        );
        // The bound is sent as the double code compares the same, never a
        // decimal PostgreSQL rounds: 0.6's own for at most, and the one
        // below it for at least, since `>=` is "not below".
        assert_eq!(
            sent.0,
            [
                SqlValue::Float8(0.6f64.to_bits()),
                SqlValue::Float8(0.6f64.to_bits())
            ]
        );
        // Below the bound: 0.6 writes as less than it too.
        let mut sent = Sent::default();
        condition(&mut sent, &Meanings, &cosine(Compare::Less), false);
        assert_eq!(sent.0, [SqlValue::Float8(0.6f64.to_bits())]);
        assert_eq!(
            condition(&mut Fixed, &Meanings, &near, false),
            None,
            "a predicate takes no parameters"
        );
    }

    /// A table of one optional point, `spot`.
    struct Spots;

    impl Columns for Spots {
        fn column(&self, field: &str) -> Option<Column<'_>> {
            (field == "spot").then_some(Column {
                name: "spot",
                nullable: true,
                ty: Some(SqlType::Geography),
                builtin: None,
                scale: None,
            })
        }
    }

    #[test]
    fn a_geo_condition_searches_a_margin_wider_and_never_under_not() {
        let param = |index: u32| Value::Param {
            index,
            path: Vec::new(),
        };
        let geo = |op: Op, args: Vec<CallArg>| Condition::Call {
            call: Call {
                op,
                field: "spot".into(),
                args,
                result: Held::Builtin(Builtin::Bool),
            },
            test: Test::Holds,
        };
        let near = geo(
            Op::GeoWithin,
            vec![
                CallArg::Value(param(0), Held::Shape(POINT)),
                CallArg::Value(param(1), Held::Builtin(Builtin::Decimal)),
            ],
        );
        let mut params = Collected::default();
        assert_eq!(
            condition(&mut Params::default(), &Spots, &near, false).as_deref(),
            Some("ST_DWithin(\"spot\", $1, $2 + 0.001)")
        );
        assert_eq!(
            condition(&mut params, &Spots, &near, false),
            None,
            "the point is a parameter, which `Collected` doesn't bind"
        );
        assert_eq!(
            condition(&mut Params::default(), &Spots, &near, true),
            None,
            "no index serves the negation"
        );
        let inside = geo(
            Op::GeoContains,
            vec![CallArg::Value(param(0), Held::Shape(POINT))],
        );
        assert_eq!(
            condition(&mut Params::default(), &Spots, &inside, false).as_deref(),
            Some("ST_DWithin(\"spot\", $1, 0.001)")
        );
        assert_eq!(
            condition(&mut Fixed, &Spots, &inside, false),
            None,
            "a predicate takes no parameters"
        );
    }

    /// An operation with an exact form stands where a field would (chunk
    /// 6.3b): its form compared in the statement, `not` turning the
    /// comparison round, a record whose field is `none` meeting nothing.
    #[test]
    fn an_operation_s_form_is_compared_and_indexed() {
        let length = |op: Compare| Condition::Call {
            call: Call {
                op: Op::TextLength,
                field: "title".into(),
                args: Vec::new(),
                result: Held::Builtin(Builtin::Int),
            },
            test: Test::Compare {
                op,
                value: Value::Literal(number("24")),
            },
        };
        let mut params = Collected::default();
        assert_eq!(
            condition(&mut params, &Notes, &length(Compare::LessEqual), false).as_deref(),
            Some("(\"name\" IS NOT NULL AND char_length(\"name\")::bigint <= $1)")
        );
        assert_eq!(params.0, [SqlValue::Int8(24)], "the bound as an `Int`");
        assert_eq!(
            condition(
                &mut Collected::default(),
                &Notes,
                &Condition::Not(Box::new(length(Compare::LessEqual))),
                false
            )
            .as_deref(),
            Some("(\"name\" IS NOT NULL AND char_length(\"name\")::bigint > $1)"),
            "`none` meets neither"
        );
        let starts = |value: Value| Condition::Call {
            call: Call {
                op: Op::TextStartsWith,
                field: "title".into(),
                args: vec![CallArg::Value(value, Held::Builtin(Builtin::Text))],
                result: Held::Builtin(Builtin::Bool),
            },
            test: Test::Holds,
        };
        let the = starts(Value::Literal(Literal::Text("The ".into())));
        assert_eq!(
            condition(
                &mut Params::default(),
                &Notes,
                &Condition::Not(Box::new(the.clone())),
                false
            )
            .as_deref(),
            Some("(\"name\" IS NOT NULL AND (starts_with(\"name\", 'The ')) IS FALSE)"),
            "an argument written out is written into the expression"
        );
        let Condition::Call { call, .. } = &the else {
            unreachable!()
        };
        assert_eq!(
            index_expression(&Notes, call).as_deref(),
            Some("starts_with(\"name\", 'The ')"),
            "the index holds the same expression"
        );
        let given = starts(Value::Param {
            index: 0,
            path: Vec::new(),
        });
        let Condition::Call { call, .. } = &given else {
            unreachable!()
        };
        assert_eq!(
            index_expression(&Notes, call),
            None,
            "a value given as the query runs isn't indexed"
        );
        assert_eq!(lowering(Op::TextLower), None, "no form");
        assert_eq!(lowering(Op::GeoWithin), Some(Lowering::Search));
        assert_eq!(lowering(Op::AiDistance), Some(Lowering::Exact));
        assert!(!form::indexable(Op::MapCount, Held::Json));
        assert!(!form::indexable(
            Op::HashSha256,
            Held::Builtin(Builtin::Text)
        ));
        assert!(form::indexable(
            Op::HashSha256,
            Held::Builtin(Builtin::Bytes)
        ));
    }
}
