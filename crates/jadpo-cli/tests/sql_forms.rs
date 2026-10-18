//! Every SQL form gives what the runtime gives (area 19, "SQL forms";
//! chunk 5.6): each operation `jadpo_sql::form` writes a form for runs in
//! the runtime (`jadpo_value::run`) and as its form in live PostgreSQL on
//! the same inputs, and the two must agree exactly, the places a decimal
//! keeps, a whole number's width and `none` included.
//!
//! Each operation's inputs are stored as a backfill would find them, in a
//! table's columns of the types area 5 gives their fields, written through
//! the storage codec (`jadpo_runtime::to_sql`, `jadpo_sql::literal`), and
//! its form reads those columns. What the form gives is compared with what
//! the runtime gives as the storage codec binds it, or, for a range or a
//! value in several columns, as the runtime reads it back
//! (`jadpo_runtime::field_value`). An input on which the runtime stops
//! with a defect (a whole number past 64 bits, a decimal past 38 digits)
//! must make the statement fail, so a backfill stops there too.
//!
//! Each form is read in a session with unusual settings (a zone whose
//! offset isn't whole hours, dates written day first, escapes read the
//! old way), since a backfill runs in whatever session `jadpo migrate
//! apply` opens.
//!
//! The inputs are the edges each operation has (every rounding mode on
//! halves either side of zero, the largest and smallest whole numbers,
//! decimals of 38 digits, every Unicode white space character, text of
//! four-byte characters and combining marks, month ends and leap days)
//! and many generated besides.
//!
//! The server is the one `JADPO_TEST_DATABASE_URL` names or one started
//! from the machine's PostgreSQL tools; `JADPO_SKIP_POSTGRES=1` skips it.

use std::collections::BTreeMap;

use jadpo_runtime::{Database as _, SqlType, SqlValue};
use jadpo_sql::form::{self, Arg, Held, Operator};
use jadpo_std::read::Column;
use jadpo_std::{Builtin, Op, Rounding};
use jadpo_value::Value;

fn skipped() -> bool {
    let skip = std::env::var("JADPO_SKIP_POSTGRES").is_ok_and(|value| value == "1");
    if skip {
        println!("skipped: JADPO_SKIP_POSTGRES is set");
    }
    skip
}

// --- inputs ------------------------------------------------------------------

/// A column type: as PostgreSQL declares it, and as the codec binds it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Col {
    declared: &'static str,
    bind: SqlType,
}

const BIGINT: Col = Col {
    declared: "bigint",
    bind: SqlType::Int8,
};
const SMALLINT: Col = Col {
    declared: "smallint",
    bind: SqlType::Int2,
};
const INTEGER: Col = Col {
    declared: "integer",
    bind: SqlType::Int4,
};
const NUMERIC: Col = Col {
    declared: "numeric",
    bind: SqlType::Numeric,
};
const TEXT: Col = Col {
    declared: "text",
    bind: SqlType::Text,
};
const BYTEA: Col = Col {
    declared: "bytea",
    bind: SqlType::Bytes,
};
const UUID: Col = Col {
    declared: "uuid",
    bind: SqlType::Uuid,
};
const BOOLEAN: Col = Col {
    declared: "boolean",
    bind: SqlType::Bool,
};
const DATE: Col = Col {
    declared: "date",
    bind: SqlType::Date,
};
const JSONB: Col = Col {
    declared: "jsonb",
    bind: SqlType::Json,
};
const INT8RANGE: Col = Col {
    declared: "int8range",
    bind: SqlType::Range(&SqlType::Int8),
};
const NUMRANGE: Col = Col {
    declared: "numrange",
    bind: SqlType::Range(&SqlType::Numeric),
};
const DATERANGE: Col = Col {
    declared: "daterange",
    bind: SqlType::Range(&SqlType::Date),
};
const TSTZRANGE: Col = Col {
    declared: "tstzrange",
    bind: SqlType::Range(&SqlType::Timestamp),
};
const INSTANT: Col = Col {
    declared: "timestamptz(3)",
    bind: SqlType::Timestamp,
};

/// One argument of a case.
#[derive(Clone, Debug)]
enum In {
    /// A value in one column.
    Val(Value, Col, Held),
    /// A list, a set or a map, as the JSON its column holds.
    Json(Value, String),
    /// An amount of money, in its two columns.
    Money(Value),
    Rounding(Rounding),
    Whole(i64),
    Character(char),
    Absent,
}

impl In {
    /// What the runtime is given.
    fn runtime(&self) -> Value {
        match self {
            In::Val(value, ..) | In::Json(value, _) | In::Money(value) => value.clone(),
            In::Rounding(rounding) => {
                let name = rounding_name(*rounding);
                let ordinal =
                    jadpo_std::variant_ordinal(Rounding::NAME, name).expect("a rounding mode");
                Value::variant(Rounding::NAME, ordinal, name, Vec::new())
            }
            In::Whole(n) => Value::Int(*n),
            In::Character(c) => Value::text(&c.to_string()),
            In::Absent => Value::None,
        }
    }

    /// The columns it is stored in, as PostgreSQL declares them; none for
    /// what is written out.
    fn columns(&self) -> Vec<&'static str> {
        match self {
            In::Val(_, col, _) => vec![col.declared],
            In::Json(..) => vec![JSONB.declared],
            In::Money(_) => vec!["bigint", "char(3)"],
            _ => Vec::new(),
        }
    }

    /// The literal of each of its columns, through the storage codec.
    fn literals(&self) -> Vec<String> {
        let literal = |value: &Value, ty: SqlType| -> String {
            let bound = jadpo_runtime::to_sql(value, ty)
                .unwrap_or_else(|why| panic!("{value:?} as {ty:?}: {why}"));
            jadpo_sql::literal::sql(&bound).unwrap_or_else(|| panic!("{bound:?} as SQL"))
        };
        match self {
            In::Val(value @ Value::Record(_), col, _) => {
                let parts = jadpo_value::standard::to_parts(value).expect("a range's column");
                vec![literal(&parts[0], col.bind)]
            }
            In::Val(value, col, _) => vec![literal(value, col.bind)],
            In::Json(_, json) => {
                vec![jadpo_sql::literal::sql(&SqlValue::Json(json.clone())).expect("JSON")]
            }
            In::Money(value) => {
                let parts = jadpo_value::standard::to_parts(value).expect("money's columns");
                vec![
                    literal(&parts[0], SqlType::Int8),
                    literal(&parts[1], SqlType::Text),
                ]
            }
            _ => Vec::new(),
        }
    }

    /// What decides the table it goes in besides its columns: what is
    /// written out, and how a value is held.
    fn key(&self) -> String {
        match self {
            In::Val(_, col, held) => format!("{}:{held:?}", col.declared),
            In::Json(..) => "json".into(),
            In::Money(_) => "money".into(),
            In::Rounding(rounding) => format!("rounding {rounding:?}"),
            In::Whole(n) => format!("whole {n}"),
            In::Character(c) => format!("character {c:?}"),
            In::Absent => "absent".into(),
        }
    }
}

fn rounding_name(rounding: Rounding) -> &'static str {
    match rounding {
        Rounding::Up => "up",
        Rounding::Down => "down",
        Rounding::HalfUp => "half_up",
        Rounding::HalfEven => "half_even",
    }
}

const ROUNDINGS: [Rounding; 4] = [
    Rounding::Up,
    Rounding::Down,
    Rounding::HalfUp,
    Rounding::HalfEven,
];

fn int(n: i64) -> In {
    In::Val(Value::Int(n), BIGINT, Held::Builtin(Builtin::Int))
}

/// A whole number in a column as narrow as a field whose range is small
/// gets (area 5).
fn narrow(n: i64) -> In {
    let col = if i16::try_from(n).is_ok() {
        SMALLINT
    } else {
        INTEGER
    };
    In::Val(Value::Int(n), col, Held::Builtin(Builtin::Int))
}

fn dec(text: &str) -> In {
    In::Val(decimal(text), NUMERIC, Held::Builtin(Builtin::Decimal))
}

fn decimal(text: &str) -> Value {
    Value::decimal(text).unwrap_or_else(|| panic!("{text} as a decimal"))
}

fn text(value: &str) -> In {
    In::Val(Value::text(value), TEXT, Held::Builtin(Builtin::Text))
}

fn bytes(value: &[u8]) -> In {
    In::Val(
        Value::Bytes(std::rc::Rc::from(value)),
        BYTEA,
        Held::Builtin(Builtin::Bytes),
    )
}

fn day(text: &str) -> i32 {
    jadpo_std::date_number(text).unwrap_or_else(|| panic!("{text} as a date"))
}

fn date(text: &str) -> In {
    In::Val(
        Value::Date(day(text)),
        DATE,
        Held::Builtin(Builtin::CalendarDate),
    )
}

/// A `Range<T>` of `ends`' values, held in `col`, `None` an open end.
fn range(start: Option<Value>, end: Option<Value>, col: Col, item: Builtin) -> In {
    In::Val(
        jadpo_value::range::range_of(start, end),
        col,
        Held::Range(item),
    )
}

/// A `DateRange`, from `start` to `end`, both held.
fn date_range(start: &str, end: &str) -> In {
    In::Val(
        Value::record(
            jadpo_std::DATE_RANGE,
            vec![
                ("start".into(), Value::Date(day(start))),
                ("end".into(), Value::Date(day(end))),
            ],
        ),
        DATERANGE,
        Held::Range(Builtin::CalendarDate),
    )
}

/// An `InstantRange` from `start` up to `end`, in milliseconds.
fn instant_range(start: i64, end: i64) -> In {
    In::Val(
        jadpo_value::temporal::range(start, end),
        TSTZRANGE,
        Held::Range(Builtin::Instant),
    )
}

/// Pseudo-random numbers, the same every run.
struct Numbers(u64);

impl Numbers {
    fn next(&mut self) -> u64 {
        // A 64-bit linear congruential generator (Knuth's MMIX).
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 11
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }

    /// A whole number of a random number of digits and sign.
    fn whole(&mut self) -> i64 {
        let digits = self.below(19) as u32;
        let magnitude = (self.next() % 10u64.pow(digits).max(1)) as i64;
        if self.below(2) == 0 {
            magnitude
        } else {
            -magnitude
        }
    }

    /// A decimal of a random number of digits, places and sign.
    fn decimal(&mut self) -> String {
        let places = self.below(9) as usize;
        let whole = self.below(12) as u32;
        let digits = (self.next() % 10u64.pow(whole).max(1)).to_string();
        let fraction: String = (0..places)
            .map(|_| char::from(b'0' + self.below(10) as u8))
            .collect();
        let sign = if self.below(2) == 0 { "" } else { "-" };
        if places == 0 {
            format!("{sign}{digits}")
        } else {
            format!("{sign}{digits}.{fraction}")
        }
    }
}

const WHOLE_EDGES: &[i64] = &[
    0,
    1,
    -1,
    2,
    -2,
    3,
    -3,
    4,
    -4,
    5,
    -5,
    7,
    -7,
    10,
    -10,
    25,
    -25,
    100,
    -100,
    i64::MAX,
    i64::MIN,
    i64::MAX - 1,
    i64::MIN + 1,
    1 << 62,
    -(1 << 62),
    (1 << 62) + 1,
    1 << 31,
    -(1 << 31),
    (1 << 31) - 1,
    4_611_686_018_427_387_903,
    3_037_000_499,
];

fn wholes(count: usize) -> Vec<i64> {
    let mut out = WHOLE_EDGES.to_vec();
    let mut numbers = Numbers(7);
    out.extend((0..count).map(|_| numbers.whole()));
    out
}

const DECIMAL_EDGES: &[&str] = &[
    "0",
    "0.0",
    "0.00",
    "0.5",
    "-0.5",
    "1.5",
    "-1.5",
    "2.5",
    "-2.5",
    "3.5",
    "0.05",
    "-0.05",
    "0.15",
    "0.25",
    "-0.25",
    "1.25",
    "-1.25",
    "1.35",
    "2.675",
    "0.005",
    "-0.005",
    "1.4999999",
    "-1.5000001",
    "10.00",
    "3",
    "-3",
    "7.000",
    "123.456",
    "-123.456",
    "0.0000000000000000000000000000000000001",
    "12345678901234567890.123456789012345678",
    "-99999999999999999999999999999999999999",
    "99999999999999999999999999999999999.999",
    "9223372036854775807.4",
    "9223372036854775807.5",
    "-9223372036854775808.5",
    "-9223372036854775808.4",
    "9223372036854775806.5",
];

fn decimals(count: usize) -> Vec<String> {
    let mut out: Vec<String> = DECIMAL_EDGES.iter().map(|d| d.to_string()).collect();
    let mut numbers = Numbers(11);
    out.extend((0..count).map(|_| numbers.decimal()));
    out
}

/// Text as people and programs write it: empty, ASCII, every Unicode white
/// space character, characters that look like white space and aren't,
/// four-byte characters, combining marks, right-to-left text, quotes and
/// backslashes, and characters `LIKE` would read as patterns.
fn texts() -> Vec<String> {
    let mut out: Vec<String> = [
        "",
        "a",
        "ab",
        "abc",
        "hello world",
        "  padded  ",
        "\t\n\r tabs \u{b}\u{c}",
        "\u{85}\u{a0}nbsp\u{a0}\u{85}",
        "\u{1680}\u{2000}\u{200a}ogham\u{2028}\u{2029}\u{202f}\u{205f}\u{3000}",
        "\u{200b}zero width\u{200b}",
        "\u{feff}bom\u{feff}",
        "\u{180e}mongolian\u{180e}",
        "café",
        "cafe\u{301}",
        "naïve résumé",
        "İstanbul",
        "ß",
        "日本語のテキスト",
        "𝄞 music 𝄞",
        "👍🏽👨‍👩‍👧",
        "שלום עולם",
        "it's \"quoted\"",
        "back\\slash",
        "100%_off",
        "a-b-c-a-b-c",
        "aaa",
        "aaaa",
        "x\u{0}y",
    ]
    .iter()
    .map(|t| t.to_string())
    .collect();
    // PostgreSQL text can't hold the NUL character, so no column does.
    out.retain(|t| !t.contains('\0'));
    out.push(form::TRIMMED_CHARACTERS.to_string());
    out.push(format!(
        "{}word{}",
        form::TRIMMED_CHARACTERS,
        form::TRIMMED_CHARACTERS
    ));
    out.push("x".repeat(300));
    out
}

// --- running -----------------------------------------------------------------

/// How what the form gives is compared with what the runtime gives.
#[derive(Clone, Debug)]
enum Out {
    /// One column: the runtime's value as the codec binds it.
    Bound(SqlType),
    /// Several columns: the runtime's parts as the codec binds them.
    Parts(Vec<SqlType>),
    /// A value read back from its column as the runtime reads a field.
    Read(Column),
}

struct Forms {
    runtime: jadpo_host_native::Runtime,
    store: jadpo_host_native::Postgres,
    tables: usize,
    checked: BTreeMap<Op, usize>,
    /// How many inputs each operator was checked on (chunk 5.6b).
    operators: BTreeMap<Operator, usize>,
    /// How many inputs the runtime stopped on, and the form with it.
    stopped: usize,
}

impl Forms {
    fn new() -> Forms {
        let server = jadpo_test::database::server().expect("a PostgreSQL server");
        let url = server.create_database("sql-forms").expect("a database");
        Forms {
            runtime: jadpo_host_native::Runtime::new().expect("a runtime"),
            store: jadpo_host_native::postgres(&url).expect("a store"),
            tables: 0,
            checked: BTreeMap::new(),
            operators: BTreeMap::new(),
            stopped: 0,
        }
    }

    /// Runs `op` on each case in the runtime and as its form, and fails
    /// with every case on which they disagree.
    fn check(&mut self, op: Op, out: Out, cases: Vec<Vec<In>>) {
        assert!(form::has_form(op), "{op:?} has no form");
        let count = cases.len();
        self.agree(
            &format!("{op:?}"),
            &|args| form::operation(op, args),
            &|values| jadpo_value::run(op, values, None, &mut |_| None),
            out,
            cases,
        );
        *self.checked.entry(op).or_default() += count;
    }

    /// Runs operator `op` on each case in the runtime and as its form
    /// (chunk 5.6b).
    fn check_operator(
        &mut self,
        op: Operator,
        run: &dyn Fn(&[Value]) -> Result<Value, jadpo_value::Fault>,
        out: Out,
        cases: Vec<Vec<In>>,
    ) {
        let count = cases.len();
        self.agree(
            &format!("{op:?}"),
            &|args| form::operator(op, args).map(form::Form::Value),
            run,
            out,
            cases,
        );
        *self.operators.entry(op).or_default() += count;
    }

    /// Runs `render`, a form written from its arguments, and `run`, what
    /// the runtime does, on each case, and fails with every case on which
    /// they disagree.
    fn agree(
        &mut self,
        label: &str,
        render: &dyn Fn(&[Arg<'_>]) -> Option<form::Form>,
        run: &dyn Fn(&[Value]) -> Result<Value, jadpo_value::Fault>,
        out: Out,
        cases: Vec<Vec<In>>,
    ) {
        let mut groups: BTreeMap<String, Vec<Vec<In>>> = BTreeMap::new();
        for case in cases {
            let key: Vec<String> = case.iter().map(In::key).collect();
            groups.entry(key.join(" | ")).or_default().push(case);
        }
        let mut wrong = Vec::new();
        for (_, cases) in groups {
            wrong.extend(self.check_group(label, render, run, &out, &cases));
        }
        assert!(
            wrong.is_empty(),
            "{label}: the form and the runtime disagree on {} cases:\n{}",
            wrong.len(),
            wrong
                .iter()
                .take(25)
                .cloned()
                .collect::<Vec<String>>()
                .join("\n")
        );
    }

    fn check_group(
        &mut self,
        op: &str,
        render: &dyn Fn(&[Arg<'_>]) -> Option<form::Form>,
        run: &dyn Fn(&[Value]) -> Result<Value, jadpo_value::Fault>,
        out: &Out,
        cases: &[Vec<In>],
    ) -> Vec<String> {
        self.tables += 1;
        let table = format!("f{}", self.tables);
        // The columns, and each argument as its form takes it.
        let first = &cases[0];
        let mut declared = vec!["k integer PRIMARY KEY".to_string()];
        let mut names: Vec<Vec<String>> = Vec::new();
        for (at, arg) in first.iter().enumerate() {
            let columns = arg.columns();
            let mut these = Vec::new();
            for (part, ty) in columns.iter().enumerate() {
                let name = if columns.len() == 1 {
                    format!("c{at}")
                } else {
                    format!("c{at}_{part}")
                };
                declared.push(format!("\"{name}\" {ty}"));
                these.push(format!("\"{name}\""));
            }
            names.push(these);
        }
        let args: Vec<Arg<'_>> = first
            .iter()
            .zip(&names)
            .map(|(arg, names)| match arg {
                In::Val(_, _, held) => Arg::Value(&names[0], *held),
                In::Json(..) => Arg::Value(&names[0], Held::Json),
                In::Money(_) => Arg::Parts(names),
                In::Rounding(rounding) => Arg::Rounding(*rounding),
                In::Whole(n) => Arg::Whole(*n),
                In::Character(c) => Arg::Character(*c),
                In::Absent => Arg::Absent,
            })
            .collect();
        let written = render(&args).unwrap_or_else(|| panic!("{op} has no form for {args:?}"));
        let selected = match &written {
            form::Form::Value(sql) => sql.clone(),
            form::Form::Parts(parts) => parts.join(", "),
        };
        // What the runtime gives each case.
        let given: Vec<Result<Value, jadpo_value::Fault>> = cases
            .iter()
            .map(|case| {
                let values: Vec<Value> = case.iter().map(In::runtime).collect();
                run(&values)
            })
            .collect();
        // The cases the runtime gives a value for go in one table, read
        // in one statement; those it stops on in another, read one by one.
        let row = |k: usize| {
            let mut values = vec![k.to_string()];
            values.extend(cases[k].iter().flat_map(In::literals));
            format!("({})", values.join(", "))
        };
        let fine: Vec<String> = (0..cases.len())
            .filter(|k| given[*k].is_ok())
            .map(row)
            .collect();
        let stops: Vec<String> = (0..cases.len())
            .filter(|k| given[*k].is_err())
            .map(row)
            .collect();
        let stopped = format!("{table}_stops");
        let describe = |k: usize| {
            let values: Vec<String> = cases[k]
                .iter()
                .map(|arg| match arg {
                    In::Val(value, ..) | In::Json(value, _) | In::Money(value) => {
                        format!("{value:?}")
                    }
                    other => format!("{other:?}"),
                })
                .collect();
            values.join(", ")
        };
        let store = &self.store;
        let mut wrong = Vec::new();
        self.runtime.block_on(async {
            let mut transaction = store.begin().await.expect("begins");
            for (name, rows) in [(&table, &fine), (&stopped, &stops)] {
                transaction
                    .batch(&format!(
                        "CREATE TABLE \"{name}\" ({})",
                        declared.join(", ")
                    ))
                    .await
                    .expect("a table");
                for chunk in rows.chunks(500) {
                    transaction
                        .batch(&format!(
                            "INSERT INTO \"{name}\" VALUES {}",
                            chunk.join(", ")
                        ))
                        .await
                        .unwrap_or_else(|error| panic!("{op}: inserting ({error:?})"));
                }
            }
            transaction.commit().await.expect("commits");
            if !fine.is_empty() {
                let mut transaction = store.begin().await.expect("begins");
                transaction.batch(UNUSUAL).await.expect("settings");
                let sql = format!("SELECT k, {selected} FROM \"{table}\" ORDER BY k");
                let found = transaction.query(&sql, &[]).await;
                let _ = transaction.rollback().await;
                let found = match found {
                    Ok(found) => found,
                    Err(error) => {
                        wrong.push(format!("{op}: the form failed ({error:?}):\n{sql}"));
                        return;
                    }
                };
                for row in found {
                    let Some(SqlValue::Int4(k)) = row.first() else {
                        panic!("a row without its key");
                    };
                    let k = *k as usize;
                    let Ok(value) = &given[k] else {
                        continue;
                    };
                    if let Some(problem) = compare(out, value, &row[1..]) {
                        wrong.push(format!("  {}: {problem}", describe(k)));
                    }
                }
            }
            // Each case the runtime stops on with a defect stops the
            // statement too.
            for (k, given) in given.iter().enumerate() {
                let Err(fault) = given else {
                    continue;
                };
                let mut transaction = store.begin().await.expect("begins");
                transaction.batch(UNUSUAL).await.expect("settings");
                let found = transaction
                    .query(
                        &format!("SELECT {selected} FROM \"{stopped}\" WHERE k = {k}"),
                        &[],
                    )
                    .await;
                let _ = transaction.rollback().await;
                if let Ok(found) = found {
                    wrong.push(format!(
                        "  {}: the runtime stops ({fault:?}) but the form gives {found:?}",
                        describe(k)
                    ));
                }
            }
        });
        self.stopped += stops.len();
        wrong
    }
}

/// Settings a session may have that no form may depend on: a zone far
/// from UTC whose offset isn't whole hours, dates written day first,
/// intervals in the SQL standard's style, and fewer float digits.
const UNUSUAL: &str = "SET LOCAL TimeZone = 'Pacific/Chatham'; SET LOCAL DateStyle = 'SQL, DMY'; SET LOCAL IntervalStyle = 'sql_standard'; SET LOCAL extra_float_digits = -15; SET LOCAL standard_conforming_strings = off";

/// Why `found`, what the form gave, isn't `value`, what the runtime gave.
fn compare(out: &Out, value: &Value, found: &[SqlValue]) -> Option<String> {
    let bound = |value: &Value, ty: SqlType| match value {
        Value::None => SqlValue::Null(ty),
        value => jadpo_runtime::to_sql(value, ty)
            .unwrap_or_else(|why| panic!("the runtime's {value:?} as {ty:?}: {why}")),
    };
    // `NULL` read back is untyped; compare it as `none`.
    let same = |expected: &SqlValue, found: &SqlValue| match (expected, found) {
        (SqlValue::Null(_), SqlValue::Null(_)) => true,
        (expected, found) => expected == found,
    };
    match out {
        Out::Bound(ty) => {
            let expected = bound(value, *ty);
            (!same(&expected, &found[0]))
                .then(|| format!("the runtime gives {expected:?}, the form {:?}", found[0]))
        }
        Out::Parts(types) => {
            let parts = jadpo_value::standard::to_parts(value).expect("parts");
            let expected: Vec<SqlValue> = parts
                .iter()
                .zip(types)
                .map(|(part, ty)| bound(part, *ty))
                .collect();
            let agree = expected.len() == found.len()
                && expected.iter().zip(found).all(|(e, f)| same(e, f));
            (!agree).then(|| format!("the runtime gives {expected:?}, the form {found:?}"))
        }
        Out::Read(column) => {
            let read = jadpo_runtime::field_value(
                found,
                std::slice::from_ref(column),
                0,
                jadpo_runtime::JsonCodecs::NONE,
            )
            .unwrap_or_else(|why| panic!("reading {found:?}: {why}"));
            // Equal, and with the same places: a decimal end keeps its own.
            (read != *value || format!("{read:?}") != format!("{value:?}"))
                .then(|| format!("the runtime gives {value:?}, the form {read:?}"))
        }
    }
}

// --- the operations ------------------------------------------------------------

fn pairs<T: Clone>(items: &[T]) -> Vec<(T, T)> {
    let mut out = Vec::new();
    for a in items {
        for b in items {
            out.push((a.clone(), b.clone()));
        }
    }
    out
}

fn numbers(forms: &mut Forms) {
    let whole = wholes(60);
    let mut divide = Vec::new();
    for (x, a) in pairs(&whole) {
        if a == 0 {
            continue;
        }
        for rounding in ROUNDINGS {
            divide.push(vec![int(x), int(a), In::Rounding(rounding)]);
        }
    }
    // The smallest whole numbers of the narrower columns, whose
    // arithmetic there would overflow.
    let narrower: Vec<i64> = vec![
        i64::from(i16::MIN),
        i64::from(i16::MAX),
        -1,
        1,
        2,
        3,
        -7,
        i64::from(i32::MIN),
        i64::from(i32::MAX),
    ];
    for (x, a) in pairs(&narrower) {
        for rounding in ROUNDINGS {
            divide.push(vec![narrow(x), narrow(a), In::Rounding(rounding)]);
        }
    }
    forms.check(Op::IntegerDivide, Out::Bound(SqlType::Int8), divide);
    let mut ones: Vec<Vec<In>> = whole.iter().map(|n| vec![int(*n)]).collect();
    ones.extend(narrower.iter().map(|n| vec![narrow(*n)]));
    forms.check(Op::IntegerAbs, Out::Bound(SqlType::Int8), ones);
    for op in [Op::IntegerMin, Op::IntegerMax] {
        let mut cases: Vec<Vec<In>> = pairs(&whole[..40])
            .into_iter()
            .map(|(a, b)| vec![int(a), int(b)])
            .collect();
        cases.extend(
            pairs(&narrower)
                .into_iter()
                .map(|(a, b)| vec![narrow(a), narrow(b)]),
        );
        forms.check(op, Out::Bound(SqlType::Int8), cases);
    }
    let decimal = decimals(150);
    let mut from = Vec::new();
    for d in &decimal {
        for rounding in ROUNDINGS {
            from.push(vec![dec(d), In::Rounding(rounding)]);
        }
    }
    forms.check(Op::IntegerFromDecimal, Out::Bound(SqlType::Int8), from);
    let mut divide = Vec::new();
    let divisors: Vec<String> = decimal[..70].to_vec();
    for x in &decimal[..90] {
        for a in &divisors {
            if decimal_of(a).is_zero() {
                continue;
            }
            for rounding in ROUNDINGS {
                divide.push(vec![dec(x), dec(a), In::Rounding(rounding)]);
            }
        }
    }
    // A whole number divided, or dividing, as a decimal of no places.
    for (x, a) in [(10, 4), (-7, 2), (1, 3), (i64::MAX, 7)] {
        for rounding in ROUNDINGS {
            divide.push(vec![int(x), dec("0.3"), In::Rounding(rounding)]);
            divide.push(vec![dec("12.5"), int(a), In::Rounding(rounding)]);
        }
    }
    forms.check(Op::DecimalDivide, Out::Bound(SqlType::Numeric), divide);
    let ones: Vec<Vec<In>> = decimal.iter().map(|d| vec![dec(d)]).collect();
    forms.check(Op::DecimalAbs, Out::Bound(SqlType::Numeric), ones);
    for op in [Op::DecimalMin, Op::DecimalMax] {
        let mut cases: Vec<Vec<In>> = pairs(&decimal[..45])
            .into_iter()
            .map(|(a, b)| vec![dec(&a), dec(&b)])
            .collect();
        // Equal values of different places: the first is given back.
        cases.push(vec![dec("1.5"), dec("1.50")]);
        cases.push(vec![dec("1.50"), dec("1.5")]);
        forms.check(op, Out::Bound(SqlType::Numeric), cases);
    }
    let mut round = Vec::new();
    for d in &decimal {
        for scale in [0, 1, 2, 3, 4, 7, 10, 18] {
            for rounding in ROUNDINGS {
                round.push(vec![dec(d), In::Whole(scale), In::Rounding(rounding)]);
            }
        }
    }
    forms.check(Op::DecimalRound, Out::Bound(SqlType::Numeric), round);
}

fn decimal_of(text: &str) -> jadpo_value::Decimal {
    match decimal(text) {
        Value::Decimal(d) => d,
        _ => unreachable!(),
    }
}

fn texts_and_counts(forms: &mut Forms) {
    let all = texts();
    let ones: Vec<Vec<In>> = all.iter().map(|t| vec![text(t)]).collect();
    forms.check(Op::TextLength, Out::Bound(SqlType::Int8), ones.clone());
    forms.check(Op::TextTrim, Out::Bound(SqlType::Text), ones);
    let mut parts: Vec<String> = all.clone();
    parts.extend(
        [
            "c", "b", "ab", "bc", "abc", "abcd", "a-b", "-", "%", "_", "\\", "'", "e\u{301}", "é",
            "𝄞", "👍", "aa", " ", "\u{a0}", "world", "İ", "i",
        ]
        .iter()
        .map(|t| t.to_string()),
    );
    for op in [Op::TextContains, Op::TextStartsWith, Op::TextEndsWith] {
        let mut cases = Vec::new();
        for value in &all {
            for part in &parts {
                cases.push(vec![text(value), text(part)]);
            }
        }
        forms.check(op, Out::Bound(SqlType::Bool), cases);
    }
    let finds = ["a", "aa", "b", "-", "%", "\\", "é", "𝄞", " ", "abc", "x"];
    let withs = ["", "Z", "aa", "\\\\", "'", "𝄞𝄞"];
    let mut cases = Vec::new();
    for value in &all {
        for find in finds {
            for with in withs {
                cases.push(vec![text(value), text(find), text(with)]);
            }
        }
    }
    forms.check(Op::TextReplace, Out::Bound(SqlType::Text), cases);
    let counts: Vec<i64> = vec![
        0,
        1,
        2,
        3,
        5,
        11,
        299,
        300,
        301,
        i64::from(i32::MAX) - 2,
        i64::from(i32::MAX) - 1,
        i64::from(i32::MAX),
        i64::from(i32::MAX) + 1,
        1 << 40,
        i64::MAX - 1,
        i64::MAX,
    ];
    let mut cases = Vec::new();
    for value in &all {
        for from in &counts {
            for count in &counts {
                cases.push(vec![text(value), int(*from), int(*count)]);
            }
        }
    }
    forms.check(Op::TextSlice, Out::Bound(SqlType::Text), cases);
    for op in [
        Op::TextKeepFirst,
        Op::TextKeepLast,
        Op::TextDropFirst,
        Op::TextDropLast,
    ] {
        let mut cases = Vec::new();
        for value in &all {
            for n in &counts {
                cases.push(vec![text(value), int(*n)]);
            }
        }
        forms.check(op, Out::Bound(SqlType::Text), cases);
    }
    let mut cases = Vec::new();
    for value in &all {
        for length in [i64::MIN, -5, 0, 1, 2, 3, 5, 8, 12, 40, 301] {
            for fill in ['0', ' ', '\u{a0}', 'é', '𝄞', '\'', '\\', '\n'] {
                cases.push(vec![text(value), int(length), In::Character(fill)]);
            }
        }
    }
    forms.check(Op::TextPadStart, Out::Bound(SqlType::Text), cases);
    // `text.from`: whole numbers, decimals, UUIDs and flags.
    let mut cases: Vec<Vec<In>> = wholes(40)
        .into_iter()
        .map(|n| vec![int(n), In::Absent])
        .collect();
    cases.extend(
        [i64::from(i16::MIN), 7, i64::from(i32::MAX)]
            .iter()
            .map(|n| vec![narrow(*n), In::Absent]),
    );
    cases.extend(decimals(60).iter().map(|d| vec![dec(d), In::Absent]));
    cases.extend(
        ["-0.00", "-0", "0.000000"]
            .iter()
            .map(|d| vec![dec(d), In::Absent]),
    );
    for uuid in [
        "0192f0c4-3b5e-7c1a-9d2e-4f6a8b0c1d2e",
        "00000000-0000-0000-0000-000000000000",
        "ffffffff-ffff-ffff-ffff-ffffffffffff",
    ] {
        cases.push(vec![
            In::Val(Value::uuid(uuid), UUID, Held::Builtin(Builtin::Uuid)),
            In::Absent,
        ]);
    }
    for flag in [true, false] {
        cases.push(vec![
            In::Val(Value::Bool(flag), BOOLEAN, Held::Builtin(Builtin::Bool)),
            In::Absent,
        ]);
    }
    forms.check(Op::TextFrom, Out::Bound(SqlType::Text), cases);
    // Counts of lists, sets and maps, held as JSON.
    let mut lists = Vec::new();
    let mut sets = Vec::new();
    let mut maps = Vec::new();
    for length in [0usize, 1, 2, 3, 10, 57, 200] {
        let items: Vec<Value> = (0..length as i64).map(Value::Int).collect();
        let json = format!(
            "[{}]",
            (0..length)
                .map(|n| n.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );
        lists.push(vec![In::Json(Value::list(items.clone()), json.clone())]);
        sets.push(vec![In::Json(Value::set(items.clone()), json)]);
        let entries: BTreeMap<Value, Value> = (0..length)
            .map(|n| (Value::text(&format!("k{n}")), Value::Int(n as i64)))
            .collect();
        let json = format!(
            "{{{}}}",
            (0..length)
                .map(|n| format!("\"k{n}\": {n}"))
                .collect::<Vec<_>>()
                .join(", ")
        );
        maps.push(vec![In::Json(Value::Map(std::rc::Rc::new(entries)), json)]);
    }
    forms.check(Op::ListCount, Out::Bound(SqlType::Int8), lists);
    forms.check(Op::SetCount, Out::Bound(SqlType::Int8), sets);
    forms.check(Op::MapCount, Out::Bound(SqlType::Int8), maps);
}

fn bytes_inputs() -> Vec<Vec<u8>> {
    let mut out: Vec<Vec<u8>> = vec![
        Vec::new(),
        vec![0],
        vec![0xff],
        vec![0xfb, 0xff],
        vec![0xfb, 0xff, 0xbf],
        (0..=255).collect(),
        b"hello".to_vec(),
    ];
    let mut numbers = Numbers(13);
    for length in [1usize, 2, 3, 4, 56, 57, 58, 75, 76, 77, 114, 200, 1000] {
        out.push((0..length).map(|_| numbers.below(256) as u8).collect());
    }
    out
}

fn hashes_and_encodings(forms: &mut Forms) {
    let all = bytes_inputs();
    let mut cases: Vec<Vec<In>> = all.iter().map(|b| vec![bytes(b)]).collect();
    cases.extend(texts().iter().map(|t| vec![text(t)]));
    forms.check(Op::HashSha256, Out::Bound(SqlType::Bytes), cases);
    let ones: Vec<Vec<In>> = all.iter().map(|b| vec![bytes(b)]).collect();
    for op in [
        Op::EncodingToBase64,
        Op::EncodingToBase64url,
        Op::EncodingToHex,
    ] {
        forms.check(op, Out::Bound(SqlType::Text), ones.clone());
    }
    let cases = texts().iter().map(|t| vec![text(t)]).collect();
    forms.check(Op::EncodingToUtf8, Out::Bound(SqlType::Bytes), cases);
}

fn money(forms: &mut Forms) {
    let mut cases = Vec::new();
    for code in ["GBP", "USD", "JPY", "BHD", "EUR"] {
        let currency = jadpo_std::money::ordinal_of_code(code).expect("a currency");
        for minor in [0, 1, -1, 1250, -99_999, i64::MAX, i64::MIN] {
            cases.push(vec![In::Money(jadpo_value::money::value(currency, minor))]);
        }
    }
    forms.check(Op::MoneyMinor, Out::Bound(SqlType::Int8), cases.clone());
    forms.check(Op::MoneyCurrency, Out::Bound(SqlType::Text), cases);
}

fn ranges(forms: &mut Forms) {
    let n = |n: i64| Some(Value::Int(n));
    let int_ranges = vec![
        range(n(1), n(5), INT8RANGE, Builtin::Int),
        range(n(5), n(10), INT8RANGE, Builtin::Int),
        range(n(-3), n(2), INT8RANGE, Builtin::Int),
        range(n(4), n(6), INT8RANGE, Builtin::Int),
        range(n(i64::MIN), n(i64::MAX), INT8RANGE, Builtin::Int),
        range(n(10), None, INT8RANGE, Builtin::Int),
        range(None, n(1), INT8RANGE, Builtin::Int),
        range(None, None, INT8RANGE, Builtin::Int),
    ];
    let int_values = [
        i64::MIN,
        -4,
        -3,
        0,
        1,
        4,
        5,
        6,
        9,
        10,
        11,
        i64::MAX - 1,
        i64::MAX,
    ];
    let d = |text: &str| Some(decimal(text));
    let decimal_ranges = vec![
        range(d("1.50"), d("3.00"), NUMRANGE, Builtin::Decimal),
        range(d("1.5"), d("2.25"), NUMRANGE, Builtin::Decimal),
        range(d("3.0"), None, NUMRANGE, Builtin::Decimal),
        range(None, d("-0.001"), NUMRANGE, Builtin::Decimal),
        range(d("-10"), d("1.500"), NUMRANGE, Builtin::Decimal),
    ];
    let decimal_values = [
        "1.5", "1.49", "1.50", "2.999", "3.00", "3", "-0.001", "-0.0011", "100",
    ];
    let date_ranges = vec![
        date_range("2026-01-01", "2026-01-31"),
        date_range("2026-01-31", "2026-02-28"),
        date_range("2026-02-01", "2026-02-01"),
        date_range("2024-02-28", "2024-03-01"),
        date_range("0000-01-01", "9999-12-31"),
    ];
    let dates = [
        "2025-12-31",
        "2026-01-01",
        "2026-01-31",
        "2026-02-01",
        "2026-02-28",
        "2026-03-01",
        "2024-02-29",
        "0000-01-01",
        "9999-12-31",
    ];
    let day_ranges = vec![
        range(
            Some(Value::Date(day("2026-01-01"))),
            Some(Value::Date(day("2026-02-01"))),
            DATERANGE,
            Builtin::CalendarDate,
        ),
        range(
            Some(Value::Date(day("2026-01-15"))),
            None,
            DATERANGE,
            Builtin::CalendarDate,
        ),
    ];
    let hour = 3_600_000;
    let instant_ranges = vec![
        instant_range(9 * hour, 12 * hour),
        instant_range(11 * hour, 13 * hour),
        instant_range(12 * hour, 17 * hour),
        instant_range(-hour, 1),
    ];
    let instants = [
        -hour,
        0,
        1,
        9 * hour,
        12 * hour - 1,
        12 * hour,
        13 * hour,
        17 * hour,
    ];
    let duration_ranges = vec![
        range(
            Some(Value::Duration(0)),
            Some(Value::Duration(60_000)),
            INT8RANGE,
            Builtin::Duration,
        ),
        range(
            Some(Value::Duration(30_000)),
            None,
            INT8RANGE,
            Builtin::Duration,
        ),
    ];
    let mut contains = Vec::new();
    for r in &int_ranges {
        for v in int_values {
            contains.push(vec![r.clone(), int(v)]);
        }
        for v in [-4, 0, 5, 10, i64::from(i16::MAX), i64::from(i32::MIN)] {
            contains.push(vec![r.clone(), narrow(v)]);
        }
    }
    for r in &decimal_ranges {
        for v in decimal_values {
            contains.push(vec![r.clone(), dec(v)]);
        }
    }
    for r in date_ranges.iter().chain(&day_ranges) {
        for v in dates {
            contains.push(vec![r.clone(), date(v)]);
        }
    }
    for r in &instant_ranges {
        for v in instants {
            contains.push(vec![
                r.clone(),
                In::Val(Value::Instant(v), INSTANT, Held::Builtin(Builtin::Instant)),
            ]);
        }
    }
    for r in &duration_ranges {
        for v in [0, 29_999, 30_000, 59_999, 60_000, i64::MAX] {
            contains.push(vec![
                r.clone(),
                In::Val(Value::Duration(v), BIGINT, Held::Builtin(Builtin::Duration)),
            ]);
        }
    }
    forms.check(Op::RangeContains, Out::Bound(SqlType::Bool), contains);
    let groups: Vec<(&Vec<In>, &str)> = vec![
        (&int_ranges, "Range<Int>"),
        (&decimal_ranges, "Range<Decimal>"),
        (&date_ranges, jadpo_std::DATE_RANGE),
        (&day_ranges, "Range<CalendarDate>"),
        (&instant_ranges, jadpo_std::INSTANT_RANGE),
        (&duration_ranges, "Range<Duration>"),
    ];
    let mut overlaps = Vec::new();
    for (group, _) in &groups {
        for (a, b) in pairs(group) {
            overlaps.push(vec![a, b]);
        }
    }
    forms.check(Op::RangeOverlaps, Out::Bound(SqlType::Bool), overlaps);
    for (group, ty) in groups {
        let cases = pairs(group).into_iter().map(|(a, b)| vec![a, b]).collect();
        let column = Column::Compound {
            ty: ty.to_string(),
            may_be_none: true,
        };
        forms.check(Op::RangeIntersection, Out::Read(column), cases);
    }
}

/// Days from `from` to `to`, both included, as dates' text.
fn days_between(from: &str, to: &str) -> Vec<String> {
    (day(from)..=day(to))
        .map(|n| {
            let (y, m, d) = jadpo_std::civil_date(n);
            format!("{y:04}-{m:02}-{d:02}")
        })
        .collect()
}

fn periods(forms: &mut Forms) {
    // Every pair of days across a leap February and the months either
    // side, and of days at the ends of months and years far apart.
    let window = days_between("2027-12-25", "2028-04-05");
    let mut ends = vec![
        "0000-01-01",
        "0000-02-29",
        "0001-03-01",
        "1900-02-28",
        "1900-03-01",
        "2000-02-29",
        "2023-01-31",
        "2024-01-29",
        "2024-01-30",
        "2024-01-31",
        "2024-02-29",
        "2024-03-31",
        "2025-02-28",
        "2025-03-28",
        "2025-03-29",
        "2025-10-25",
        "2026-03-29",
        "2026-10-25",
        "2028-02-29",
        "2028-03-01",
        "9999-12-31",
    ]
    .into_iter()
    .map(String::from)
    .collect::<Vec<_>>();
    for year in [1999, 2001, 2100] {
        for month in 1..=12 {
            let last = jadpo_std::days_in_month(year, month);
            ends.push(format!("{year:04}-{month:02}-{last:02}"));
            ends.push(format!("{year:04}-{month:02}-01"));
        }
    }
    let mut cases = Vec::new();
    for (a, b) in pairs(&window) {
        cases.push(vec![date(&a), date(&b)]);
    }
    // And pairs anywhere from the year 0 to 9999.
    let (first, last) = (day("0000-01-01"), day("9999-12-31"));
    let span = u64::try_from(last - first).expect("a span");
    let mut numbers = Numbers(17);
    for _ in 0..4000 {
        let mut pick = || first + i32::try_from(numbers.below(span + 1)).expect("a day");
        cases.push(vec![
            In::Val(
                Value::Date(pick()),
                DATE,
                Held::Builtin(Builtin::CalendarDate),
            ),
            In::Val(
                Value::Date(pick()),
                DATE,
                Held::Builtin(Builtin::CalendarDate),
            ),
        ]);
    }
    for (a, b) in pairs(&ends) {
        cases.push(vec![date(&a), date(&b)]);
    }
    forms.check(
        Op::TemporalPeriodBetween,
        Out::Parts(vec![SqlType::Int8; 3]),
        cases,
    );
}

fn addresses(forms: &mut Forms) {
    let ranges = [
        "192.0.2.0/24",
        "198.51.100.7/32",
        "0.0.0.0/0",
        "10.0.0.0/8",
        "2001:db8::/32",
        "2001:db8:0:1::/64",
        "::/0",
        "::ffff:0.0.0.0/96",
        "fe80::/10",
    ];
    let addresses = [
        "192.0.2.0",
        "192.0.2.255",
        "192.0.3.0",
        "198.51.100.7",
        "198.51.100.8",
        "10.255.255.255",
        "11.0.0.0",
        "0.0.0.0",
        "255.255.255.255",
        "2001:db8::1",
        "2001:DB8:0:1::abcd",
        "2001:db9::",
        "::",
        "::1",
        "::ffff:192.0.2.1",
        "fe80::1",
        "febf:ffff::",
        "fec0::",
    ];
    let mut cases = Vec::new();
    for r in ranges {
        for a in addresses {
            cases.push(vec![
                In::Val(Value::text(r), TEXT, Held::Builtin(Builtin::Text)),
                In::Val(Value::text(a), TEXT, Held::Builtin(Builtin::IpAddress)),
            ]);
        }
    }
    forms.check(Op::WebCidrContains, Out::Bound(SqlType::Bool), cases);
}

// --- operators and conditionals (chunk 5.6b) ------------------------------------

fn boolean(value: bool) -> In {
    In::Val(Value::Bool(value), BOOLEAN, Held::Builtin(Builtin::Bool))
}

fn instant(ms: i64) -> In {
    In::Val(Value::Instant(ms), INSTANT, Held::Builtin(Builtin::Instant))
}

fn duration(ms: i64) -> In {
    In::Val(
        Value::Duration(ms),
        BIGINT,
        Held::Builtin(Builtin::Duration),
    )
}

fn uuid(text: &str) -> In {
    In::Val(Value::uuid(text), UUID, Held::Builtin(Builtin::Uuid))
}

/// An operator on two values, as the runtime runs it.
type Binary = fn(&Value, &Value) -> Result<Value, jadpo_value::Fault>;

fn truth(value: &Value) -> bool {
    matches!(value, Value::Bool(true))
}

fn ordering(
    wanted: fn(std::cmp::Ordering) -> bool,
) -> impl Fn(&[Value]) -> Result<Value, jadpo_value::Fault> {
    move |v: &[Value]| jadpo_value::ops::order(&v[0], &v[1]).map(|o| Value::Bool(wanted(o)))
}

/// Every operator with a form, on the operands each takes, and the
/// conditionals, which must work out only the branch they take: a
/// division by zero or an overflow in a branch not taken stops neither.
fn operators(forms: &mut Forms) {
    use jadpo_value::ops;
    let whole = wholes(40);
    let whole_pairs: Vec<Vec<In>> = pairs(&whole)
        .into_iter()
        .map(|(a, b)| vec![int(a), int(b)])
        .collect();
    let narrower: Vec<i64> = vec![
        i64::from(i16::MIN),
        i64::from(i16::MAX),
        -1,
        0,
        1,
        2,
        -7,
        i64::from(i32::MIN),
        i64::from(i32::MAX),
    ];
    let mut wide_pairs = whole_pairs.clone();
    wide_pairs.extend(
        pairs(&narrower)
            .into_iter()
            .map(|(a, b)| vec![narrow(a), narrow(b)]),
    );
    let arithmetic: [(Operator, Binary); 5] = [
        (Operator::Add, ops::add),
        (Operator::Subtract, ops::subtract),
        (Operator::Multiply, ops::multiply),
        (Operator::Divide, ops::divide),
        (Operator::Remainder, ops::remainder),
    ];
    for (op, run) in arithmetic {
        forms.check_operator(
            op,
            &|v| run(&v[0], &v[1]),
            Out::Bound(SqlType::Int8),
            wide_pairs.clone(),
        );
    }
    let mut ones: Vec<Vec<In>> = whole.iter().map(|n| vec![int(*n)]).collect();
    ones.extend(narrower.iter().map(|n| vec![narrow(*n)]));
    forms.check_operator(
        Operator::Negate,
        &|v| ops::negate(&v[0]),
        Out::Bound(SqlType::Int8),
        ones,
    );
    // Decimals: added and taken away at the larger scale, held to 38
    // digits and to the runtime's 128 bits; compared by value.
    let mut decimal = decimals(60);
    decimal.extend(
        [
            "17014118346046923173168730371588410573",
            "-17014118346046923173168730371588410573",
            "-9999999999999999999999999999999999999.9",
            "9999999999999999999999999999999999999.9",
            "0.00000000000000000000000000000000000001",
            "1.0000000000000000000000000000000000000",
        ]
        .iter()
        .map(|d| d.to_string()),
    );
    let decimal_pairs: Vec<Vec<In>> = pairs(&decimal)
        .into_iter()
        .map(|(a, b)| vec![dec(&a), dec(&b)])
        .collect();
    let mut mixed: Vec<Vec<In>> = Vec::new();
    for d in &decimal[..50] {
        for n in &whole[..30] {
            mixed.push(vec![dec(d), int(*n)]);
            mixed.push(vec![int(*n), dec(d)]);
        }
    }
    for (op, run) in [
        (Operator::Add, ops::add as Binary),
        (Operator::Subtract, ops::subtract),
    ] {
        let mut cases = decimal_pairs.clone();
        cases.extend(mixed.clone());
        forms.check_operator(
            op,
            &|v| run(&v[0], &v[1]),
            Out::Bound(SqlType::Numeric),
            cases,
        );
    }
    forms.check_operator(
        Operator::Negate,
        &|v| ops::negate(&v[0]),
        Out::Bound(SqlType::Numeric),
        decimal.iter().map(|d| vec![dec(d)]).collect(),
    );
    // Comparisons, on every kind that is compared or ordered.
    let mut compared = wide_pairs.clone();
    compared.extend(decimal_pairs.iter().take(900).cloned());
    compared.extend(mixed.clone());
    let days = [
        "0000-01-01",
        "1969-12-31",
        "1970-01-01",
        "2024-02-29",
        "2026-10-18",
        "9999-12-31",
    ];
    compared.extend(
        pairs(&days)
            .into_iter()
            .map(|(a, b)| vec![date(a), date(b)]),
    );
    let instants = [
        -62_167_219_200_000,
        -1,
        0,
        1,
        999,
        1_792_504_800_000,
        253_402_300_799_999,
    ];
    compared.extend(
        pairs(&instants)
            .into_iter()
            .map(|(a, b)| vec![instant(a), instant(b)]),
    );
    let durations = [i64::MIN, -1, 0, 1, 60_000, i64::MAX];
    compared.extend(
        pairs(&durations)
            .into_iter()
            .map(|(a, b)| vec![duration(a), duration(b)]),
    );
    for (op, wanted) in [
        (
            Operator::Less,
            std::cmp::Ordering::is_lt as fn(std::cmp::Ordering) -> bool,
        ),
        (Operator::LessEqual, std::cmp::Ordering::is_le),
        (Operator::Greater, std::cmp::Ordering::is_gt),
        (Operator::GreaterEqual, std::cmp::Ordering::is_ge),
    ] {
        forms.check_operator(
            op,
            &ordering(wanted),
            Out::Bound(SqlType::Bool),
            compared.clone(),
        );
    }
    let mut equal = compared.clone();
    let text = texts();
    equal.extend(
        pairs(&text[..20])
            .into_iter()
            .map(|(a, b)| vec![self::text(&a), self::text(&b)]),
    );
    // Text that is canonically equivalent but not the same code points
    // is not equal, whatever the database's collation.
    equal.push(vec![self::text("café"), self::text("cafe\u{301}")]);
    equal.push(vec![self::text("a"), self::text("A")]);
    let ids = [
        "0192f0c4-3b5e-7c1a-9d2e-4f6a8b0c1d2e",
        "00000000-0000-0000-0000-000000000000",
        "ffffffff-ffff-ffff-ffff-ffffffffffff",
    ];
    equal.extend(pairs(&ids).into_iter().map(|(a, b)| vec![uuid(a), uuid(b)]));
    equal.extend(
        pairs(&[true, false])
            .into_iter()
            .map(|(a, b)| vec![boolean(a), boolean(b)]),
    );
    forms.check_operator(
        Operator::Equal,
        &|v| Ok(Value::Bool(ops::equal(&v[0], &v[1]))),
        Out::Bound(SqlType::Bool),
        equal.clone(),
    );
    forms.check_operator(
        Operator::NotEqual,
        &|v| Ok(Value::Bool(!ops::equal(&v[0], &v[1]))),
        Out::Bound(SqlType::Bool),
        equal,
    );
    // Durations added and taken away, overflowing where an `Int` does.
    let duration_pairs: Vec<Vec<In>> = pairs(&durations)
        .into_iter()
        .map(|(a, b)| vec![duration(a), duration(b)])
        .collect();
    for (op, run) in [
        (Operator::Add, ops::add as Binary),
        (Operator::Subtract, ops::subtract),
    ] {
        forms.check_operator(
            op,
            &|v| run(&v[0], &v[1]),
            Out::Bound(SqlType::Int8),
            duration_pairs.clone(),
        );
    }
    // Text joined.
    forms.check_operator(
        Operator::Add,
        &|v| ops::add(&v[0], &v[1]),
        Out::Bound(SqlType::Text),
        pairs(&text)
            .into_iter()
            .map(|(a, b)| vec![self::text(&a), self::text(&b)])
            .collect(),
    );
    // Logic.
    let truths: Vec<Vec<In>> = pairs(&[true, false])
        .into_iter()
        .map(|(a, b)| vec![boolean(a), boolean(b)])
        .collect();
    forms.check_operator(
        Operator::And,
        &|v| Ok(Value::Bool(truth(&v[0]) && truth(&v[1]))),
        Out::Bound(SqlType::Bool),
        truths.clone(),
    );
    forms.check_operator(
        Operator::Or,
        &|v| Ok(Value::Bool(truth(&v[0]) || truth(&v[1]))),
        Out::Bound(SqlType::Bool),
        truths,
    );
    forms.check_operator(
        Operator::Not,
        &|v| Ok(Value::Bool(!truth(&v[0]))),
        Out::Bound(SqlType::Bool),
        vec![vec![boolean(true)], vec![boolean(false)]],
    );
    conditionals(forms, &whole);
}

/// `and`, `or`, an `if` chain and a `match` work out only what the
/// runtime works out: `y != 0 and x / y > 1`, `y == 0 or x / y > 1`,
/// `if (y != 0) { return x / y } return 0`, and a `match` whose arms
/// divide only where their pattern holds.
fn conditionals(forms: &mut Forms, whole: &[i64]) {
    use jadpo_value::ops;
    let int = Held::Builtin(Builtin::Int);
    let bool_held = Held::Builtin(Builtin::Bool);
    let mut cases: Vec<Vec<In>> = Vec::new();
    for (x, y) in pairs(&whole[..30]) {
        cases.push(vec![self::int(x), self::int(y)]);
    }
    for y in [0, -1, 1] {
        cases.push(vec![self::int(i64::MIN), self::int(y)]);
    }
    let divide = |args: &[Arg<'_>]| -> Option<String> {
        form::operator(Operator::Divide, &[args[0], args[1]])
    };
    let nonzero = |args: &[Arg<'_>]| -> Option<String> {
        let Arg::Value(y, _) = args[1] else {
            return None;
        };
        form::operator(
            Operator::NotEqual,
            &[Arg::Value(y, int), Arg::Value("0", int)],
        )
    };
    let quotient_over_one = |args: &[Arg<'_>]| -> Option<String> {
        let q = divide(args)?;
        form::operator(
            Operator::Greater,
            &[Arg::Value(&q, int), Arg::Value("1", int)],
        )
    };
    let run_quotient = |v: &[Value]| ops::divide(&v[0], &v[1]);
    let over_one = |v: &[Value]| -> Result<bool, jadpo_value::Fault> {
        Ok(matches!(run_quotient(v)?, Value::Int(q) if q > 1))
    };
    forms.agree(
        "and stops early",
        &|args| {
            let (p, q) = (nonzero(args)?, quotient_over_one(args)?);
            form::operator(
                Operator::And,
                &[Arg::Value(&p, bool_held), Arg::Value(&q, bool_held)],
            )
            .map(form::Form::Value)
        },
        &|v| {
            if matches!(v[1], Value::Int(0)) {
                return Ok(Value::Bool(false));
            }
            over_one(v).map(Value::Bool)
        },
        Out::Bound(SqlType::Bool),
        cases.clone(),
    );
    forms.agree(
        "or stops early",
        &|args| {
            let Arg::Value(y, _) = args[1] else {
                return None;
            };
            let p = form::operator(Operator::Equal, &[Arg::Value(y, int), Arg::Value("0", int)])?;
            let q = quotient_over_one(args)?;
            form::operator(
                Operator::Or,
                &[Arg::Value(&p, bool_held), Arg::Value(&q, bool_held)],
            )
            .map(form::Form::Value)
        },
        &|v| {
            if matches!(v[1], Value::Int(0)) {
                return Ok(Value::Bool(true));
            }
            over_one(v).map(Value::Bool)
        },
        Out::Bound(SqlType::Bool),
        cases.clone(),
    );
    forms.agree(
        "an if chain takes one branch",
        &|args| {
            Some(form::Form::Value(form::case(
                &[(nonzero(args)?, divide(args)?)],
                "0::bigint",
            )))
        },
        &|v| {
            if matches!(v[1], Value::Int(0)) {
                return Ok(Value::Int(0));
            }
            run_quotient(v)
        },
        Out::Bound(SqlType::Int8),
        cases.clone(),
    );
    // A match on a variant held as text: `Size.small` divides by the
    // whole number, `Size.large` multiplies, anything else is 0.
    let sizes = ["small", "large", "medium"];
    let text_held = Held::Builtin(Builtin::Text);
    let mut matched: Vec<Vec<In>> = Vec::new();
    for size in sizes {
        for (x, y) in pairs(&whole[..12]) {
            matched.push(vec![self::text(size), self::int(x), self::int(y)]);
        }
        matched.push(vec![self::text(size), self::int(i64::MIN), self::int(-1)]);
        matched.push(vec![self::text(size), self::int(i64::MAX), self::int(2)]);
    }
    forms.agree(
        "a match takes one arm",
        &|args| {
            let Arg::Value(size, _) = args[0] else {
                return None;
            };
            let is = |variant: &str| {
                form::operator(
                    Operator::Equal,
                    &[
                        Arg::Value(size, text_held),
                        Arg::Value(&jadpo_sql::text_literal(variant), text_held),
                    ],
                )
            };
            let small = is("small")?;
            let large = is("large")?;
            let divided = form::operator(Operator::Divide, &[args[1], args[2]])?;
            let multiplied = form::operator(Operator::Multiply, &[args[1], args[2]])?;
            Some(form::Form::Value(form::case(
                &[(small, divided), (large, multiplied)],
                "0::bigint",
            )))
        },
        &|v| match &v[0] {
            Value::Text(size) if &**size == "small" => ops::divide(&v[1], &v[2]),
            Value::Text(size) if &**size == "large" => ops::multiply(&v[1], &v[2]),
            _ => Ok(Value::Int(0)),
        },
        Out::Bound(SqlType::Int8),
        matched,
    );
    // `x == none` and `x != none` on a value that may be `none`.
    let mut maybe: Vec<Vec<In>> = whole[..10].iter().map(|n| vec![self::int(*n)]).collect();
    maybe.push(vec![In::Val(Value::None, BIGINT, int)]);
    for negated in [false, true] {
        forms.agree(
            if negated { "!= none" } else { "== none" },
            &|args| {
                let Arg::Value(x, _) = args[0] else {
                    return None;
                };
                Some(form::Form::Value(form::none_check(x, negated)))
            },
            &|v| Ok(Value::Bool(ops::equal(&v[0], &Value::None) != negated)),
            Out::Bound(SqlType::Bool),
            maybe.clone(),
        );
    }
}

#[test]
fn every_sql_form_gives_what_the_runtime_gives() {
    if skipped() {
        return;
    }
    let mut forms = Forms::new();
    numbers(&mut forms);
    texts_and_counts(&mut forms);
    hashes_and_encodings(&mut forms);
    money(&mut forms);
    ranges(&mut forms);
    periods(&mut forms);
    addresses(&mut forms);
    operators(&mut forms);
    // Every operation with a form was checked.
    let missing: Vec<Op> = jadpo_std::OPERATIONS
        .iter()
        .map(|o| o.op)
        .filter(|op| form::has_form(*op) && !forms.checked.contains_key(op))
        .collect();
    assert_eq!(
        missing,
        Vec::<Op>::new(),
        "operations with a form not checked"
    );
    // Every operator with a form was checked (chunk 5.6b).
    let unchecked: Vec<Operator> = [
        Operator::And,
        Operator::Or,
        Operator::Not,
        Operator::Negate,
        Operator::Equal,
        Operator::NotEqual,
        Operator::Less,
        Operator::LessEqual,
        Operator::Greater,
        Operator::GreaterEqual,
        Operator::Add,
        Operator::Subtract,
        Operator::Multiply,
        Operator::Divide,
        Operator::Remainder,
    ]
    .into_iter()
    .filter(|op| !forms.operators.contains_key(op))
    .collect();
    assert_eq!(unchecked, Vec::<Operator>::new(), "operators not checked");
    let total: usize =
        forms.checked.values().sum::<usize>() + forms.operators.values().sum::<usize>();
    println!(
        "{} operations' and {} operators' SQL forms agree with the runtime on {total} inputs, {} of which stop both",
        forms.checked.len(),
        forms.operators.len(),
        forms.stopped
    );
    assert!(
        forms.stopped > 0,
        "no input the runtime stops on was checked"
    );
}
