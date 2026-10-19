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

use std::collections::BTreeSet;

use jadpo_std::read::{EnvelopePart, GEO_MARGIN, SqlType};
use jadpo_std::standard::storage::{self, Stored};
use jadpo_std::{Builtin, Literal, SqlValue};

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
    /// `ai.distance(Entity.field, value) <op> bound` (area 21, chunk
    /// 5.5v1b): the distance the field's vector type measures by, as
    /// pgvector's operator for `metric`, compared with a number. A record
    /// whose vector is `none` never meets it, whichever way it is compared.
    Distance {
        field: String,
        metric: jadpo_std::VectorDistance,
        value: Value,
        op: Compare,
        bound: Value,
    },
    /// A `geo.` condition on a geometry field (area 20, "Queries", chunk
    /// 5.5u5), written as a test an index serves that never leaves out a
    /// record the code's test keeps; the read works the code's test out
    /// again on each row it finds (`jadpo_std::read::Recheck`). It stands
    /// on its own `where:` line or joined by `and`, never under `not` or
    /// `or`, and a record whose field is `none` never meets it.
    Geo {
        field: String,
        test: GeoTest,
    },
}

/// What a `geo.` condition tests its field against (area 20, "Queries").
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum GeoTest {
    /// `geo.within(F, point, distance: d)`: a `Point` field within `d`, a
    /// `Length`, of a point; `measure` is the built-in type the checker
    /// found `d` to be, which a literal is decoded for.
    Near {
        point: Value,
        distance: Value,
        measure: Builtin,
    },
    /// `geo.within(F, area)`: inside a `Polygon` or a `MultiPolygon`.
    Within { area: Value },
    /// `geo.within(F, view)`: a `Point` field inside a `BoundingBox`.
    InBox { bounds: Value },
    /// `geo.within(F, circle)`: a `Point` field inside a `Circle`.
    InCircle { circle: Value },
    /// `geo.contains(F, shape)`: no point of a geometry outside the field.
    Contains { shape: Value },
    /// `geo.intersects(F, shape)`: the field and a geometry share a point.
    Intersects { shape: Value },
}

impl GeoTest {
    /// The value the field is compared with, as the code's test takes it.
    pub fn value(&self) -> &Value {
        match self {
            GeoTest::Near { point: value, .. }
            | GeoTest::Within { area: value }
            | GeoTest::InBox { bounds: value }
            | GeoTest::InCircle { circle: value }
            | GeoTest::Contains { shape: value }
            | GeoTest::Intersects { shape: value } => value,
        }
    }

    /// Whether only a `BoundingBox` test, which searches the column cast to
    /// `geometry`, and so its own index (area 20, "Indexes").
    pub fn is_box(&self) -> bool {
        matches!(self, GeoTest::InBox { .. })
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
            | Condition::Distance { field, .. }
            | Condition::Geo { field, .. } => {
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
}

/// For text that takes no parameters: a partial index's predicate, which
/// must mean the same at every moment. A literal is written into it.
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
            let ty = field.param()?;
            let text = match value {
                Value::List(items) => {
                    if items.is_empty() {
                        return Some(if negated { "TRUE" } else { "FALSE" }.to_string());
                    }
                    let mut rendered = Vec::new();
                    for item in items {
                        rendered.push(self::value(bind, item, &field)?);
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
        Condition::Distance {
            field: name,
            metric,
            value,
            op,
            bound,
        } => {
            let field = entity.column(name)?;
            let vector = match value {
                Value::Param { .. } => bind.bind(value, SqlType::Vector)?,
                _ => return None,
            };
            // The number it is compared with, as a decimal is sent; `none`
            // never meets it, so a pushed-down `not` only flips the
            // comparison.
            let number = Column {
                name: field.name,
                nullable: false,
                ty: Some(SqlType::Numeric),
                builtin: Some(Builtin::Decimal),
                scale: None,
            };
            let bound = self::value(bind, bound, &number)?;
            let op = if negated { op.negated() } else { *op };
            format!(
                "{} {} {vector} {} {bound}",
                ident(field.name),
                metric.operator(),
                op.sql()
            )
        }
        Condition::Geo { field: name, test } => {
            // No index serves the negation, and the code's test is worked
            // out again anyway: the checker keeps `geo.` conditions out
            // from under `not`.
            if negated {
                return None;
            }
            let field = entity.column(name)?;
            geo(bind, &ident(field.name), test)?
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

/// A `geo.` test on `column` as a condition an index serves, which never
/// leaves out a record the code's test keeps (area 20, "Queries", chunk
/// 5.5u5): a distance searched [`GEO_MARGIN`] further, a shape searched
/// for anything within [`GEO_MARGIN`] of it (whatever is inside it or on
/// its edge, and whatever `ST_Covers` would wrongly leave out), and a box
/// by its envelopes on the column cast to `geometry`.
fn geo(bind: &mut dyn Bind, column: &str, test: &GeoTest) -> Option<String> {
    Some(match test {
        GeoTest::Near {
            point,
            distance,
            measure,
        } => {
            let length = Column {
                name: column,
                nullable: false,
                ty: Some(SqlType::Numeric),
                builtin: Some(*measure),
                scale: None,
            };
            let point = bind.bind(point, SqlType::Geography)?;
            let distance = self::value(bind, distance, &length)?;
            format!("ST_DWithin({column}, {point}, {distance} + {GEO_MARGIN})")
        }
        GeoTest::InCircle { circle } => {
            let centre = bind.bind(&inside(circle, "centre")?, SqlType::Geography)?;
            let radius = bind.bind(&inside(circle, "radius")?, SqlType::Numeric)?;
            format!("ST_DWithin({column}, {centre}, {radius} + {GEO_MARGIN})")
        }
        GeoTest::Within { area: shape }
        | GeoTest::Contains { shape }
        | GeoTest::Intersects { shape } => {
            let shape = bind.bind(shape, SqlType::Geography)?;
            format!("ST_DWithin({column}, {shape}, {GEO_MARGIN})")
        }
        GeoTest::InBox { bounds } => {
            let south = bind.bind(&inside(bounds, "south")?, SqlType::Numeric)?;
            let north = bind.bind(&inside(bounds, "north")?, SqlType::Numeric)?;
            let mut envelopes = Vec::new();
            for (west, east) in [
                (EnvelopePart::FirstWest, EnvelopePart::FirstEast),
                (EnvelopePart::SecondWest, EnvelopePart::SecondEast),
            ] {
                let west = bind.envelope(bounds, west)?;
                let east = bind.envelope(bounds, east)?;
                envelopes.push(format!(
                    "{column}::geometry && ST_MakeEnvelope({west}, {south}, {east}, {north}, 4326)"
                ));
            }
            format!("({})", envelopes.join(" OR "))
        }
    })
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

    #[test]
    fn a_distance_is_pgvector_s_operator_and_none_never_meets_it() {
        let near = Condition::Distance {
            field: "meaning".into(),
            metric: jadpo_std::VectorDistance::InnerProduct,
            value: Value::Param {
                index: 0,
                path: Vec::new(),
            },
            op: Compare::LessEqual,
            bound: Value::Literal(Literal::Number {
                value: jadpo_std::Num::parse("0.6").expect("a number"),
                decimal: true,
            }),
        };
        assert_eq!(
            condition(&mut Params::default(), &Meanings, &near, false).as_deref(),
            Some("\"meaning\" <#> $1 <= $2")
        );
        // `not` flips the comparison, with no `IS NULL`: a record without
        // a vector meets neither.
        assert_eq!(
            condition(
                &mut Params::default(),
                &Meanings,
                &Condition::Not(Box::new(near.clone())),
                false
            )
            .as_deref(),
            Some("\"meaning\" <#> $1 > $2")
        );
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
        let near = Condition::Geo {
            field: "spot".into(),
            test: GeoTest::Near {
                point: param(0),
                distance: param(1),
                measure: Builtin::Decimal,
            },
        };
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
        let inside = Condition::Geo {
            field: "spot".into(),
            test: GeoTest::Contains { shape: param(0) },
        };
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
}
