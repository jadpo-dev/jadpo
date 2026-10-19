//! Reads (area 6; docs/compiler-runtime.md §2.5: `Ctx` `read` with a plan
//! and parameters), run the same way for both executors from the plans
//! `jadpo-plan` decides ([`jadpo_std::read`]).
//!
//! A read runs inside the action's transaction, begun at the first store
//! operation, so it is current (area 6, "Where a read comes from"); its
//! includes are one statement each, whatever the number of records; and a
//! page continues after an opaque cursor that names the query that made
//! it. A driver's refusal becomes what the runtime raises: `unavailable`,
//! a rerun of the action after a concurrent change, or a fault.

use std::cell::Cell;
use std::collections::BTreeMap;
use std::rc::Rc;

use jadpo_std::policy::CallerPart;
use jadpo_std::read::{
    Cardinality, Check, Column, Field, IncludeShape, Operand, ReadPlan, Recheck, Record, Select,
    Size, Source,
};
use jadpo_value::Value;

use crate::{
    Ctx, Failure, Fault, FaultKind, LocalFuture, Row, SqlType, SqlValue, StoreError, Transaction,
};

/// What a read found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Found {
    Value(Value),
    /// A read for exactly one record found none: [`crate::store`] raises
    /// the read's not-found failure, which its plan names. With `true`,
    /// the record is there and policy hid it from the caller, which the
    /// failure's log says (area 4, "Concealing records").
    Missing(bool),
}

/// The failure policy refusing a caller raises (area 9, "When policy says
/// no"): 403 `not_permitted`, never listed in `fails`.
pub fn not_permitted() -> Failure {
    let mut failure = Failure::new("runtime not_permitted", "NotPermitted", "not_permitted");
    failure.kind = Some("not_permitted".to_string());
    failure.status = Some(403);
    failure.message = Some("Not permitted.".to_string());
    failure
}

/// The caller's parts a statement's policy reads, in [`CallerPart`]'s
/// order.
fn callers(ctx: &Ctx<'_>) -> [SqlValue; 3] {
    [
        ctx.caller_param(CallerPart::User),
        ctx.caller_param(CallerPart::Service),
        ctx.caller_param(CallerPart::Signed),
    ]
}

/// The value of caller part `part` among `callers`.
pub(crate) fn caller_of(callers: &[SqlValue; 3], part: CallerPart) -> SqlValue {
    match part {
        CallerPart::User => callers[0].clone(),
        CallerPart::Service => callers[1].clone(),
        CallerPart::Signed => callers[2].clone(),
    }
}

/// What stops a read or a write before it gives a value.
#[derive(Debug)]
pub(crate) enum Interrupt {
    Failed(Box<Failure>),
    Fault(Box<Fault>),
}

/// What a page holds: its records, and the cursor for the next.
pub const PAGE: &str = "Page";

/// Runs `plan` with `args`, called from site `site` of the frame on top of
/// `ctx`.
pub(crate) async fn read(
    ctx: &mut Ctx<'_>,
    plan: &ReadPlan,
    json: JsonCodecs<'_>,
    args: &[Value],
    site: u32,
) -> Result<Found, Interrupt> {
    let now = ctx.now();
    let mut env = Env {
        args,
        json,
        now,
        size: 0,
        cursor: Vec::new(),
        keys: None,
        callers: callers(ctx),
        unpoliced: ctx.unpoliced(),
        fetch: Cell::new(0),
    };
    if let Some(page) = &plan.page {
        env.size = match &page.size {
            Size::Fixed(n) => i64::try_from(*n).unwrap_or(i64::MAX),
            Size::From(source) => match value_at(source, args) {
                Some(Value::Int(n)) => n,
                _ => return Err(defect(ctx, "a page size that isn't a whole number", site)),
            },
        };
        match value_at(&page.after, args) {
            Some(Value::None) | None => {}
            Some(Value::Text(text)) => match cursor::decode(&text, page.tag, &page.cursor) {
                Some(values) => env.cursor = values,
                None => return Err(Interrupt::Failed(ctx.fail(invalid_cursor(), site))),
            },
            Some(_) => return Err(defect(ctx, "a page cursor that isn't text", site)),
        }
    }
    let result = {
        let transaction = match ctx.transaction(&plan.store).await {
            Ok(transaction) => transaction,
            Err(error) => return Err(refused(ctx, error, site)),
        };
        run_read(transaction, plan, &env).await
    };
    let (rows, values, hidden) = match result {
        Ok(found) => found,
        Err(Problem::Store(error)) => return Err(refused(ctx, error, site)),
        Err(Problem::Defect(what)) => return Err(defect(ctx, &what, site)),
        Err(Problem::Refused) => {
            return Err(Interrupt::Failed(ctx.fail(not_permitted(), site)));
        }
    };
    Ok(match plan.cardinality {
        Cardinality::One => match values.into_iter().next() {
            Some(value) => Found::Value(value),
            // Whether policy hid it: the statement selected the grant of
            // every record it found.
            None => Found::Missing(hidden),
        },
        Cardinality::Optional => Found::Value(values.into_iter().next().unwrap_or(Value::None)),
        // A map layer's features, as one collection (chunk 5.5u8).
        Cardinality::List if plan.layer => Found::Value(Value::record(
            jadpo_std::standard::geo::FEATURE_COLLECTION,
            vec![(
                Rc::from(jadpo_std::standard::geo::FEATURE_COLLECTION_FIELDS[0]),
                Value::list(values),
            )],
        )),
        Cardinality::List => Found::Value(Value::list(values)),
        Cardinality::Page => {
            let size = usize::try_from(env.size).unwrap_or(0);
            let more = values.len() > size;
            let items: Vec<Value> = values.into_iter().take(size).collect();
            let next = match (
                &plan.page,
                more,
                size.checked_sub(1).and_then(|last| rows.get(last)),
            ) {
                (Some(page), true, Some(last)) => {
                    let parts: Vec<SqlValue> = page
                        .cursor
                        .iter()
                        .map(|(at, _)| {
                            widen(
                                last.get(*at as usize)
                                    .cloned()
                                    .unwrap_or(SqlValue::Null(SqlType::Text)),
                            )
                        })
                        .collect();
                    match cursor::encode(page.tag, &parts) {
                        Some(text) => Value::text(&text),
                        None => return Err(defect(ctx, "a page cursor it couldn't write", site)),
                    }
                }
                _ => Value::None,
            };
            Found::Value(Value::record(
                PAGE,
                vec![
                    (Rc::from("items"), Value::list(items)),
                    (Rc::from("next"), next),
                ],
            ))
        }
    })
}

/// What the runtime raises when a page's cursor was made by another query,
/// or isn't a cursor at all (area 6: "rejected as invalid input"; area 7:
/// 422 `invalid_input`).
pub fn invalid_cursor() -> Failure {
    let mut failure = Failure::new("runtime invalid_input", "InvalidInput", "invalid_input");
    failure.kind = Some("invalid_value".to_string());
    failure.status = Some(422);
    failure.internal.push((
        "reason".to_string(),
        Value::text("the page cursor wasn't made by this query"),
    ));
    failure
}

/// A store's refusal as what the runtime raises.
pub(crate) fn refused(ctx: &mut Ctx<'_>, error: StoreError, site: u32) -> Interrupt {
    match error {
        StoreError::Unavailable => Interrupt::Failed(ctx.fail(crate::unavailable(), site)),
        StoreError::ConcurrentChange => {
            // The boundary runs the whole action again; what this run
            // gives is thrown away.
            ctx.concurrent_change();
            Interrupt::Fault(ctx.fault(FaultKind::Defect("a concurrent change".into()), site))
        }
        // A rule nothing maps: the write's plan maps every rule the
        // checker knows it can break.
        StoreError::Conflict { constraint } => Interrupt::Fault(ctx.fault(
            FaultKind::Store(format!(
                "the store refused a write that broke `{constraint}`, which nothing maps"
            )),
            site,
        )),
        StoreError::Statement(what) => Interrupt::Fault(ctx.fault(
            FaultKind::Defect(format!(
                "a statement Jadpo wrote that the store refused ({what})"
            )),
            site,
        )),
        StoreError::Fault(what) => Interrupt::Fault(ctx.fault(FaultKind::Store(what), site)),
    }
}

pub(crate) fn defect(ctx: &mut Ctx<'_>, what: &str, site: u32) -> Interrupt {
    Interrupt::Fault(ctx.fault(FaultKind::Defect(what.to_string()), site))
}

// --- running statements --------------------------------------------------------

/// Where a read's parameters come from while it runs.
struct Env<'a> {
    args: &'a [Value],
    /// The codecs a JSON-held column is read with.
    json: JsonCodecs<'a>,
    now: i64,
    size: i64,
    /// The values of the cursor a page continues after.
    cursor: Vec<SqlValue>,
    /// An include's keys.
    keys: Option<SqlValue>,
    /// The caller's parts its policy reads.
    callers: [SqlValue; 3],
    /// Whether it runs its statements without their policy.
    unpoliced: bool,
    /// How many rows a rechecked read's statement asks for this time
    /// ([`Source::Fetch`]).
    fetch: Cell<i64>,
}

enum Problem {
    Store(StoreError),
    Defect(String),
    /// A required reference's record that policy hides from the caller.
    Refused,
}

/// One parameter's value.
fn bind(param: &jadpo_std::read::Param, env: &Env<'_>) -> Result<SqlValue, Problem> {
    Ok(match &param.source {
        Source::Arg { .. } => {
            let value = value_at(&param.source, env.args)
                .ok_or_else(|| Problem::Defect("a parameter the read wasn't given".into()))?;
            to_sql(&value, param.ty).map_err(Problem::Defect)?
        }
        Source::Now => SqlValue::Timestamp(env.now.saturating_mul(1000)),
        Source::Keys => env
            .keys
            .clone()
            .ok_or_else(|| Problem::Defect("an include without keys".into()))?,
        Source::Cursor(i) => env
            .cursor
            .get(*i as usize)
            .cloned()
            .unwrap_or(SqlValue::Null(param.ty)),
        Source::SizePlusOne => SqlValue::Int8(env.size.saturating_add(1)),
        Source::Caller(part) => caller_of(&env.callers, *part),
        // A literal or an enum value, as the storage codec bound it when
        // the plan was made (chunk 5.5w3b).
        Source::Stored(value) => value.clone(),
        Source::Fetch => SqlValue::Int8(env.fetch.get()),
        // A side of a box filter's envelopes (area 20, chunk 5.5u5).
        Source::Envelope { index, path, part } => {
            let bounds = value_at(
                &Source::Arg {
                    index: *index,
                    path: path.clone(),
                },
                env.args,
            );
            let side = |name: &str| match &bounds {
                Some(Value::Record(record)) => match record.get(name) {
                    Some(Value::Decimal(value)) => Some(jadpo_std::Num {
                        mantissa: value.mantissa,
                        scale: value.scale,
                    }),
                    _ => None,
                },
                _ => None,
            };
            let (Some(west), Some(east)) = (side("west"), side("east")) else {
                return Err(Problem::Defect("a box without its sides".into()));
            };
            SqlValue::Numeric(part.of(&west, &east).to_string())
        }
        // The double a distance is compared with (chunk 5.8c).
        Source::DistanceBound {
            index,
            path,
            strict,
        } => {
            let bound = value_at(
                &Source::Arg {
                    index: *index,
                    path: path.clone(),
                },
                env.args,
            );
            let written = match &bound {
                Some(Value::Decimal(decimal)) => decimal.to_string(),
                Some(Value::Int(n)) => n.to_string(),
                _ => {
                    return Err(Problem::Defect(
                        "a distance bound that isn't a number".into(),
                    ));
                }
            };
            let found = jadpo_std::standard::ai::distance_bound(&written, *strict)
                .ok_or_else(|| Problem::Defect("a distance bound that isn't a number".into()))?;
            SqlValue::Float8(found.to_bits())
        }
    })
}

/// The main statement's rows, the values they become, and whether it found
/// a record the caller may not read, which is left out before anything
/// else reads it.
async fn run_read(
    transaction: &mut dyn Transaction,
    plan: &ReadPlan,
    env: &Env<'_>,
) -> Result<(Vec<Row>, Vec<Value>, bool), Problem> {
    // A `nearest` query's search settings, for the rest of the
    // transaction (area 21, chunk 5.5v1b).
    for setting in &plan.settings {
        transaction.batch(setting).await.map_err(Problem::Store)?;
    }
    let mut rows = match &plan.recheck {
        Some(recheck) => rechecked(transaction, plan, recheck, env).await?,
        None => run(transaction, &plan.select, env).await?,
    };
    let mut hidden = false;
    if let Some(at) = plan.select.permitted
        && !env.unpoliced
    {
        let found = rows.len();
        rows.retain(|row| matches!(row.get(at as usize), Some(SqlValue::Bool(true))));
        hidden = rows.len() < found;
    }
    let values = records(transaction, &plan.select, &rows, env).await?;
    Ok((rows, values, hidden))
}

/// The rows of a read whose conditions search an index (chunk 6.3b; area
/// 20, "Queries", chunk 5.5u5): those its statement finds that pass each
/// check as Jadpo's own code works it out, sorted by the code's result when
/// the read is ordered by an operation's, as many as the read keeps. The
/// statement searches a margin wider than the conditions, so it never
/// leaves out a row the code keeps; it asks for as many rows as the read
/// keeps, and again for four times as many while the checks leave the read
/// short, or while a row past those fetched could, by the database's
/// measure less the margin, still come before the last kept.
async fn rechecked(
    transaction: &mut dyn Transaction,
    plan: &ReadPlan,
    recheck: &Recheck,
    env: &Env<'_>,
) -> Result<Vec<Row>, Problem> {
    let keep = match (plan.cardinality, &recheck.limit) {
        (Cardinality::Page, _) => Some(env.size.saturating_add(1)),
        (_, Some(Size::Fixed(n))) => Some(i64::try_from(*n).unwrap_or(i64::MAX)),
        (_, Some(Size::From(source))) => match value_at(source, env.args) {
            Some(Value::Int(n)) => Some(n),
            _ => return Err(Problem::Defect("a limit that isn't a whole number".into())),
        },
        (_, None) => None,
    };
    let keep = keep.map(|n| usize::try_from(n.max(0)).unwrap_or(usize::MAX));
    let mut fetch = keep.unwrap_or(0).max(1);
    loop {
        env.fetch.set(i64::try_from(fetch).unwrap_or(i64::MAX));
        let rows = run(&mut *transaction, &plan.select, env).await?;
        let found = rows.len();
        let last_measured = recheck.order.as_ref().and_then(|order| {
            match rows.last().and_then(|row| row.get(order.measured as usize)) {
                Some(SqlValue::Float8(bits)) => Some(f64::from_bits(*bits)),
                _ => None,
            }
        });
        let mut kept = Vec::new();
        for row in rows {
            if let Some(key) = passes(recheck, &plan.select, &row, env)? {
                kept.push((key, row));
            }
        }
        if let Some(order) = &recheck.order {
            let id = order.id as usize;
            kept.sort_by(|(a, x), (b, y)| a.cmp(b).then_with(|| x.get(id).cmp(&y.get(id))));
        }
        let Some(keep) = keep else {
            return Ok(kept.into_iter().map(|(_, row)| row).collect());
        };
        let exhausted = found < fetch;
        let enough = kept.len() >= keep
            && match (&recheck.order, last_measured) {
                // The last row kept by the code's result is nearer than
                // any row not fetched could be.
                (Some(_), Some(measured)) => kept
                    .get(keep.saturating_sub(1))
                    .and_then(|(key, _)| *key)
                    .is_some_and(|(amount, scale)| {
                        (amount as f64) / 10f64.powi(scale as i32)
                            < measured - jadpo_std::read::GEO_MARGIN_METRES
                    }),
                (Some(_), None) => false,
                (None, _) => true,
            };
        if exhausted || enough {
            kept.truncate(keep);
            return Ok(kept.into_iter().map(|(_, row)| row).collect());
        }
        fetch = fetch.saturating_mul(4);
    }
}

/// Whether `row` passes each of the read's checks as Jadpo's code works
/// them out: `None` when it doesn't, and when it does, the code's result
/// for a read ordered by one, a physical quantity's amount and its kind's
/// scale.
fn passes(
    recheck: &Recheck,
    select: &Select,
    row: &Row,
    env: &Env<'_>,
) -> Result<Option<Option<(i128, u32)>>, Problem> {
    let mut key = None;
    if let Some(order) = &recheck.order {
        let Some(found) = checked(&order.key, select, row, env)? else {
            return Ok(None);
        };
        match found {
            Value::Quantity(quantity) => key = Some((quantity.amount, quantity.kind.scale())),
            _ => return Err(Problem::Defect("an order that isn't a quantity".into())),
        }
    }
    for check in &recheck.checks {
        if checked(check, select, row, env)? != Some(Value::Bool(true)) {
            return Ok(None);
        }
    }
    Ok(Some(key))
}

/// What `check` gives on `row`, run by the code a function body's call of
/// its operation runs; `None` when a column it reads is `none`, which no
/// condition on an operation of a field meets.
fn checked(
    check: &Check,
    select: &Select,
    row: &Row,
    env: &Env<'_>,
) -> Result<Option<Value>, Problem> {
    let mut args = Vec::with_capacity(check.args.len());
    for operand in &check.args {
        args.push(match operand {
            Operand::Column(at) => {
                let value =
                    field_value(row, &select.columns, *at, env.json).map_err(Problem::Defect)?;
                if value == Value::None {
                    return Ok(None);
                }
                value
            }
            Operand::Value(source) => value_at(source, env.args)
                .ok_or_else(|| Problem::Defect("a value the read wasn't given".into()))?,
            Operand::Constant(constant) => Value::constant(constant),
            Operand::Absent => Value::None,
        });
    }
    jadpo_value::run(check.op, &args, None, &mut |_| None)
        .map(Some)
        .map_err(|fault| Problem::Defect(format!("{fault:?}")))
}

async fn run(
    transaction: &mut dyn Transaction,
    select: &Select,
    env: &Env<'_>,
) -> Result<Vec<Row>, Problem> {
    let mut params = Vec::with_capacity(select.params.len());
    for param in &select.params {
        params.push(bind(param, env)?);
    }
    let sql = match (&select.unpoliced, env.unpoliced) {
        (Some(unpoliced), true) => unpoliced,
        _ => &select.sql,
    };
    transaction
        .query(sql, &params)
        .await
        .map_err(Problem::Store)
}

/// The records `rows` of `select` become, with their includes: one more
/// statement per include, matching all the rows' keys at once.
fn records<'a>(
    transaction: &'a mut dyn Transaction,
    select: &'a Select,
    rows: &'a [Row],
    env: &'a Env<'a>,
) -> LocalFuture<'a, Result<Vec<Value>, Problem>> {
    Box::pin(async move {
        let mut found: Vec<BTreeMap<SqlValue, Vec<Value>>> = Vec::new();
        for include in &select.includes {
            let mut keys: Vec<SqlValue> = Vec::new();
            for row in rows {
                if let Some(key) = row.get(include.key as usize)
                    && !matches!(key, SqlValue::Null(_))
                    && !keys.contains(key)
                {
                    keys.push(key.clone());
                }
            }
            let mut matched: BTreeMap<SqlValue, Vec<Value>> = BTreeMap::new();
            if let Some(first) = keys.first() {
                let element = first.ty();
                let inner = Env {
                    args: env.args,
                    json: env.json,
                    now: env.now,
                    size: env.size,
                    cursor: Vec::new(),
                    keys: Some(SqlValue::Array(element, keys)),
                    callers: env.callers.clone(),
                    unpoliced: env.unpoliced,
                    fetch: Cell::new(0),
                };
                let child_rows = run(&mut *transaction, &include.select, &inner).await?;
                let values =
                    records(&mut *transaction, &include.select, &child_rows, &inner).await?;
                for (row, value) in child_rows.iter().zip(values) {
                    let key = row.first().cloned().unwrap_or(SqlValue::Null(element));
                    matched.entry(key).or_default().push(value);
                }
            }
            found.push(matched);
        }
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            out.push(record(&select.record, row, select, &found, env)?);
        }
        Ok(out)
    })
}

/// The value `row` of `select` becomes as `shape`: each field from its
/// column, its include's records, or a record of its own from the same
/// row (a `Nearest<T>`'s `item`, chunk 5.5v1b).
fn record(
    shape: &Record,
    row: &Row,
    select: &Select,
    found: &[BTreeMap<SqlValue, Vec<Value>>],
    env: &Env<'_>,
) -> Result<Value, Problem> {
    let mut fields = Vec::with_capacity(shape.fields.len());
    for (name, field) in &shape.fields {
        let value = match field {
            Field::Column(at) => {
                field_value(row, &select.columns, *at, env.json).map_err(Problem::Defect)?
            }
            Field::Record(inner) => record(inner, row, select, found, env)?,
            Field::Include(at) => {
                let (Some(include), Some(matched)) =
                    (select.includes.get(*at as usize), found.get(*at as usize))
                else {
                    return Err(Problem::Defect("an include the plan doesn't have".into()));
                };
                let records = row
                    .get(include.key as usize)
                    .and_then(|key| matched.get(key))
                    .cloned()
                    .unwrap_or_default();
                match include.shape {
                    IncludeShape::List => Value::list(records),
                    IncludeShape::Optional => records.into_iter().next().unwrap_or(Value::None),
                    IncludeShape::One => match records.into_iter().next() {
                        Some(value) => value,
                        // The reference holds, so policy hid its record
                        // from the caller (area 9).
                        None if include.select.unpoliced.is_some() && !env.unpoliced => {
                            return Err(Problem::Refused);
                        }
                        None => {
                            return Err(Problem::Defect(
                                "a required reference whose record isn't there".into(),
                            ));
                        }
                    },
                }
            }
        };
        fields.push((Rc::from(name.as_str()), value));
    }
    Ok(Value::record(&shape.ty, fields))
}

/// The value a source names among a read's arguments.
fn value_at(source: &Source, args: &[Value]) -> Option<Value> {
    let Source::Arg { index, path } = source else {
        return None;
    };
    let mut value = args.get(*index as usize)?.clone();
    for name in path {
        value = match &value {
            Value::Record(record) => record.get(name)?.clone(),
            Value::None => return Some(Value::None),
            _ => return None,
        };
    }
    Some(value)
}

// --- values and columns ------------------------------------------------------------

/// The codecs a JSON-held standard value is written to its `jsonb` column
/// and read back with (area 19, chunk 5.5w2b): the plan's JSON table,
/// numbered as `json.read`'s sites number theirs, with the program's
/// validators and shapes, as `json.read` is given them, so a stored value
/// is written and read exactly as a response's and a request's body are.
#[derive(Clone, Copy, Debug)]
pub struct JsonCodecs<'a> {
    pub codecs: jadpo_value::json::Codecs<'a>,
    /// Each numbered type's codec in `codecs`.
    pub types: &'a [jadpo_std::http::CodecId],
}

impl JsonCodecs<'static> {
    /// No codecs, for a program that holds no JSON-held value.
    pub const NONE: JsonCodecs<'static> = JsonCodecs {
        codecs: jadpo_value::json::Codecs {
            codecs: &[],
            validators: &[],
            shapes: &[],
        },
        types: &[],
    };
}

impl JsonCodecs<'_> {
    fn codec(&self, number: u32) -> Result<jadpo_std::http::CodecId, String> {
        self.types
            .get(number as usize)
            .copied()
            .ok_or_else(|| "a JSON-held column the plan gave no codec".to_string())
    }

    /// `value` as the JSON its column keeps; `NULL` for no value.
    pub fn write(&self, number: u32, value: &Value) -> Result<SqlValue, String> {
        if matches!(value, Value::None) {
            return Ok(SqlValue::Null(SqlType::Json));
        }
        Ok(SqlValue::Json(
            self.codecs.write_value(self.codec(number)?, value),
        ))
    }

    /// The value the JSON `stored` holds, read as a request's body is.
    /// What doesn't read back is named by its field's path and the rule,
    /// never the value (chunk 5.5w2).
    pub fn read(&self, number: u32, stored: &str) -> Result<Value, String> {
        let codec = self.codec(number)?;
        self.codecs
            .read_value(codec, &Value::text(stored))
            .map_err(|unread| match unread {
                jadpo_value::json::Unread::Malformed => {
                    "a JSON-held column that isn't one JSON document".to_string()
                }
                jadpo_value::json::Unread::Mismatch(problem) if problem.field.is_empty() => {
                    format!(
                        "a JSON-held column that isn't its type (`{}`)",
                        problem.rule
                    )
                }
                jadpo_value::json::Unread::Mismatch(problem) => format!(
                    "a JSON-held column that isn't its type (`{}` on `{}`)",
                    problem.rule, problem.field
                ),
            })
    }
}

/// A value as a parameter of type `ty`, through the contract's storage
/// codec (`jadpo_std::standard::storage::bind`), which the compiler writes
/// every literal in a statement with too (chunk 5.5w3b). A fault names what
/// the value is, never the value itself, which may be personal or secret
/// and would reach a log through it (chunk 5.5w2).
pub fn to_sql(value: &Value, ty: SqlType) -> Result<SqlValue, String> {
    match value.stored() {
        Some(stored) => jadpo_std::standard::storage::bind(&stored, ty),
        None => Err(format!(
            "{} that a `{ty:?}` column can't hold",
            value.described()
        )),
    }
}

/// A column's value as the runtime holds it.
pub fn from_sql(sql: &SqlValue, column: &Column) -> Result<Value, String> {
    Ok(match (sql, column) {
        (SqlValue::Null(_), _) => Value::None,
        (SqlValue::Bool(b), Column::Bool) => Value::Bool(*b),
        (SqlValue::Int2(n), Column::Int) => Value::Int(i64::from(*n)),
        (SqlValue::Int4(n), Column::Int) => Value::Int(i64::from(*n)),
        (SqlValue::Int8(n), Column::Int) => Value::Int(*n),
        (SqlValue::Int8(n), Column::Duration) => Value::Duration(*n),
        (SqlValue::Numeric(text), Column::Decimal) => {
            Value::decimal(text).ok_or("a decimal with more than 38 digits")?
        }
        // Held in its kind's base unit at its scale, which the column's
        // type keeps (area 19).
        (SqlValue::Numeric(text), Column::Quantity(kind)) => {
            let number = jadpo_std::Num::parse(text).ok_or("a quantity that isn't a number")?;
            let amount = number
                .at_scale(kind.scale())
                .ok_or("a quantity with more than 38 digits")?
                .mantissa;
            Value::Quantity(jadpo_value::Quantity {
                kind: *kind,
                amount,
            })
        }
        // The fraction it is: 20% is 0.20 (area 19).
        (SqlValue::Numeric(text), Column::Percentage) => match Value::decimal(text) {
            Some(Value::Decimal(fraction)) => Value::Percentage(fraction),
            _ => return Err("a percentage with more than 38 digits".to_string()),
        },
        (SqlValue::Text(text), Column::Text) => Value::text(text),
        (SqlValue::Bytes(bytes), Column::Bytes) => Value::Bytes(Rc::from(bytes.as_slice())),
        (SqlValue::Uuid(bytes), Column::Uuid) => Value::uuid(&uuid_text(bytes)),
        (SqlValue::Timestamp(micros), Column::Instant) => Value::Instant(micros.div_euclid(1000)),
        (SqlValue::Date(day), Column::Date) => Value::Date(*day),
        (SqlValue::Text(stored), Column::Enum { ty, variants }) => {
            let name = &jadpo_std::standard::variant_stored(ty, stored).to_string();
            let ordinal = variants
                .iter()
                .position(|variant| variant == name)
                .ok_or_else(|| format!("a stored code that isn't a variant of `{ty}`"))?;
            Value::variant(ty, ordinal as u32, name, Vec::new())
        }
        // Each item the decimal its float is (area 21, chunk 5.5v1).
        (SqlValue::Vector(bits), Column::Vector) => vector_value(bits)?,
        // The geometry its type holds, its rings as they were stored (area
        // 20, chunk 5.5u4).
        (SqlValue::Geography(shape), Column::Geography(ty)) => {
            jadpo_value::geo::from_shape(ty, shape)?
        }
        // As `ai.distance` gives it (area 21, chunk 5.5v1b).
        (SqlValue::Float8(bits), Column::Distance) => distance_value(f64::from_bits(*bits))?,
        (sql, column) => {
            return Err(format!("a `{:?}` column read as {column:?}", sql.ty()));
        }
    })
}

/// The value of the field whose column is at `at` of `row`: the column's
/// own, for a compound standard type (`Money`, area 19) its value made from
/// that column and the parts after it, and for a JSON-held one its JSON
/// read by `json`'s codec (chunk 5.5w2b).
pub fn field_value(
    row: &[SqlValue],
    columns: &[Column],
    at: u32,
    json: JsonCodecs<'_>,
) -> Result<Value, String> {
    let at = at as usize;
    let (Some(sql), Some(column)) = (row.get(at), columns.get(at)) else {
        return Err("a row without a column it needs".into());
    };
    if let Column::Json(number) = column {
        return match sql {
            SqlValue::Null(_) => Ok(Value::None),
            SqlValue::Json(stored) => json.read(*number, stored),
            other => Err(format!("a `{:?}` column read as JSON", other.ty())),
        };
    }
    let Column::Compound { ty, may_be_none } = column else {
        return from_sql(sql, column);
    };
    // A range's type names its item, `Range<Duration>` (chunk 5.5w).
    let name = ty.split('<').next().unwrap_or(ty);
    let entry =
        jadpo_std::standard_type(name).ok_or_else(|| format!("`{ty}` isn't a standard type"))?;
    let count = entry.columns().len();
    let parts = row
        .get(at..at + count)
        .ok_or_else(|| format!("a `{ty}` without its {count} columns"))?;
    if *may_be_none && parts.iter().all(|part| matches!(part, SqlValue::Null(_))) {
        return Ok(Value::None);
    }
    fn part_value(part: &SqlValue, ty: &str) -> Result<Value, String> {
        match part {
            SqlValue::Null(_) => Ok(Value::None),
            SqlValue::Bool(flag) => Ok(Value::Bool(*flag)),
            SqlValue::Bytes(bytes) => Ok(Value::Bytes(Rc::from(bytes.as_slice()))),
            SqlValue::Uuid(bytes) => Ok(Value::uuid(&uuid_text(bytes))),
            SqlValue::Timestamp(micros) => Ok(Value::Instant(micros.div_euclid(1000))),
            // A `LocalTime` and a `LocalDateTime`, as microseconds, and a
            // range as its ends and whether it holds its upper one (chunk
            // 5.5w).
            SqlValue::Time(micros) | SqlValue::LocalTimestamp(micros) => Ok(Value::Int(*micros)),
            SqlValue::Range {
                lower,
                upper,
                upper_held,
                ..
            } => {
                let end = |end: &Option<Box<SqlValue>>| match end {
                    Some(end) => part_value(end, ty),
                    None => Ok(Value::None),
                };
                Ok(Value::list(vec![
                    end(lower)?,
                    end(upper)?,
                    Value::Bool(*upper_held),
                ]))
            }
            SqlValue::Int2(n) => Ok(Value::Int(i64::from(*n))),
            SqlValue::Int4(n) => Ok(Value::Int(i64::from(*n))),
            SqlValue::Int8(n) => Ok(Value::Int(*n)),
            SqlValue::Text(text) => Ok(Value::text(text)),
            // Projected coordinates' easting and northing (chunk 5.5s).
            SqlValue::Numeric(text) => Value::decimal(text)
                .ok_or_else(|| format!("a part of a `{ty}` that isn't a decimal")),
            // An identity document's expiry (chunk 5.5i2) and an event's
            // days (chunk 5.5k3).
            SqlValue::Date(day) => Ok(Value::Date(*day)),
            // An address's lines (chunk 5.5l).
            SqlValue::Array(_, items) => items
                .iter()
                .map(|item| part_value(item, ty))
                .collect::<Result<Vec<Value>, String>>()
                .map(Value::list),
            // An embedding's vector (chunk 5.5v1).
            SqlValue::Vector(bits) => vector_value(bits),
            // A circle's centre, the point it holds (area 20, chunk 5.5u4).
            SqlValue::Geography(shape) => {
                jadpo_value::geo::from_shape(shape.standard_type(), shape)
            }
            other => Err(format!("a `{:?}` column of a `{ty}`", other.ty())),
        }
    }
    let values = parts
        .iter()
        .map(|part| part_value(part, ty))
        .collect::<Result<Vec<Value>, String>>()?;
    let value = jadpo_value::standard::from_parts_of(ty, &values)
        .map_err(|_| format!("columns that don't make a `{ty}`"))?;
    // A value read from the store is made like any other, so its rules
    // across its parts hold (area 2, "Rules across fields"; chunk 5.8c):
    // Jadpo writes none that breaks one, so a row that does was written
    // around it, and reading it is a defect, named by the rule alone.
    if let Some(rule) = jadpo_value::across::broken_standard(&value) {
        return Err(format!("a stored `{ty}` that breaks its rule `{rule}`"));
    }
    Ok(value)
}

/// A vector read back from pgvector: each float as the decimal a vector
/// keeps it as (chunk 5.5s2), so what was written reads back unchanged.
fn vector_value(bits: &[u32]) -> Result<Value, String> {
    let floats: Vec<f32> = bits.iter().copied().map(f32::from_bits).collect();
    jadpo_value::vector::from_floats(&floats)
        .ok_or_else(|| "a vector with an item past a decimal's 38 digits".to_string())
}

/// A distance pgvector worked out, as `ai.distance` gives the same double:
/// the decimal PostgreSQL writes it as. A cosine from a vector of zeros,
/// which pgvector answers with `NaN`, has no direction, and is a fault as
/// it is in code.
fn distance_value(found: f64) -> Result<Value, String> {
    if found.is_nan() {
        return Err("a cosine distance from a vector of zeros, which has no direction".into());
    }
    if !found.is_finite() {
        return Err("a distance past a decimal's range".into());
    }
    Value::decimal(&jadpo_std::standard::ai::distance_text(found))
        .ok_or_else(|| "a distance past a decimal's range".to_string())
}

/// A whole number of any width as a cursor holds it.
fn widen(value: SqlValue) -> SqlValue {
    match value {
        SqlValue::Int2(n) => SqlValue::Int8(i64::from(n)),
        SqlValue::Int4(n) => SqlValue::Int8(i64::from(n)),
        other => other,
    }
}

/// A UUID's 16 bytes from its text.
/// A UUID as text: lowercase hexadecimal in groups of 8-4-4-4-12.
pub fn uuid_text(bytes: &[u8; 16]) -> String {
    let mut out = String::with_capacity(36);
    for (i, byte) in bytes.iter().enumerate() {
        if matches!(i, 4 | 6 | 8 | 10) {
            out.push('-');
        }
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// The opaque page cursor: the query's tag and the last record's values of
/// the order's fields, as URL-safe base64. It names the query that made
/// it, so another query refuses it.
pub mod cursor {
    use crate::{SqlType, SqlValue};

    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

    pub fn encode(tag: u64, values: &[SqlValue]) -> Option<String> {
        let mut bytes = tag.to_be_bytes().to_vec();
        for value in values {
            match value {
                SqlValue::Int8(n) => {
                    bytes.push(1);
                    bytes.extend_from_slice(&n.to_be_bytes());
                }
                SqlValue::Text(text) => {
                    bytes.push(2);
                    let length = u32::try_from(text.len()).ok()?;
                    bytes.extend_from_slice(&length.to_be_bytes());
                    bytes.extend_from_slice(text.as_bytes());
                }
                SqlValue::Uuid(uuid) => {
                    bytes.push(3);
                    bytes.extend_from_slice(uuid);
                }
                SqlValue::Timestamp(micros) => {
                    bytes.push(4);
                    bytes.extend_from_slice(&micros.to_be_bytes());
                }
                SqlValue::Date(day) => {
                    bytes.push(5);
                    bytes.extend_from_slice(&day.to_be_bytes());
                }
                SqlValue::Time(micros) => {
                    bytes.push(6);
                    bytes.extend_from_slice(&micros.to_be_bytes());
                }
                SqlValue::Numeric(text) => {
                    bytes.push(7);
                    let length = u32::try_from(text.len()).ok()?;
                    bytes.extend_from_slice(&length.to_be_bytes());
                    bytes.extend_from_slice(text.as_bytes());
                }
                _ => return None,
            }
        }
        Some(base64(&bytes))
    }

    /// The values of a cursor made with `tag` for an order of `types`;
    /// `None` when it isn't one.
    pub fn decode(text: &str, tag: u64, types: &[(u32, SqlType)]) -> Option<Vec<SqlValue>> {
        let bytes = unbase64(text)?;
        let (head, mut rest) = bytes.split_at_checked(8)?;
        if u64::from_be_bytes(head.try_into().ok()?) != tag {
            return None;
        }
        let mut take = |n: usize| -> Option<Vec<u8>> {
            let (piece, after) = rest.split_at_checked(n)?;
            rest = after;
            Some(piece.to_vec())
        };
        let mut out = Vec::with_capacity(types.len());
        for (_, ty) in types {
            let kind = *take(1)?.first()?;
            let value = match (kind, ty) {
                (1, SqlType::Int8) => SqlValue::Int8(i64::from_be_bytes(take(8)?.try_into().ok()?)),
                (2, SqlType::Text) => {
                    let length = u32::from_be_bytes(take(4)?.try_into().ok()?);
                    SqlValue::Text(String::from_utf8(take(usize::try_from(length).ok()?)?).ok()?)
                }
                (3, SqlType::Uuid) => SqlValue::Uuid(take(16)?.try_into().ok()?),
                (4, SqlType::Timestamp) => {
                    SqlValue::Timestamp(i64::from_be_bytes(take(8)?.try_into().ok()?))
                }
                (5, SqlType::Date) => SqlValue::Date(i32::from_be_bytes(take(4)?.try_into().ok()?)),
                (6, SqlType::Time) => SqlValue::Time(i64::from_be_bytes(take(8)?.try_into().ok()?)),
                (7, SqlType::Numeric) => {
                    let length = u32::from_be_bytes(take(4)?.try_into().ok()?);
                    SqlValue::Numeric(String::from_utf8(take(usize::try_from(length).ok()?)?).ok()?)
                }
                _ => return None,
            };
            out.push(value);
        }
        rest.is_empty().then_some(out)
    }

    fn base64(bytes: &[u8]) -> String {
        let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
        for chunk in bytes.chunks(3) {
            let n = chunk
                .iter()
                .enumerate()
                .fold(0u32, |acc, (i, b)| acc | (u32::from(*b) << (16 - 8 * i)));
            for i in 0..=chunk.len() {
                if let Some(digit) = ALPHABET.get(((n >> (18 - 6 * i)) & 63) as usize) {
                    out.push(*digit as char);
                }
            }
        }
        out
    }

    fn unbase64(text: &str) -> Option<Vec<u8>> {
        let mut out = Vec::with_capacity(text.len() * 3 / 4);
        let digits: Vec<u32> = text
            .bytes()
            .map(|c| ALPHABET.iter().position(|a| *a == c).map(|p| p as u32))
            .collect::<Option<_>>()?;
        for chunk in digits.chunks(4) {
            if chunk.len() == 1 {
                return None;
            }
            let n = chunk
                .iter()
                .enumerate()
                .fold(0u32, |acc, (i, d)| acc | (d << (18 - 6 * i)));
            for i in 0..chunk.len() - 1 {
                out.push(((n >> (16 - 8 * i)) & 255) as u8);
            }
        }
        Some(out)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn a_cursor_reads_back_only_for_its_own_query() {
            let values = vec![
                SqlValue::Timestamp(1_760_000_000_000_000),
                SqlValue::Uuid([7; 16]),
                SqlValue::Text("naïve".into()),
                SqlValue::Int8(-3),
            ];
            let types = [
                (0, SqlType::Timestamp),
                (1, SqlType::Uuid),
                (2, SqlType::Text),
                (3, SqlType::Int8),
            ];
            let text = encode(42, &values).unwrap();
            assert!(text.bytes().all(|b| ALPHABET.contains(&b)), "{text}");
            assert_eq!(decode(&text, 42, &types), Some(values));
            assert_eq!(decode(&text, 43, &types), None, "another query's cursor");
            assert_eq!(decode("not a cursor!", 42, &types), None);
            assert_eq!(decode(&text[..text.len() - 2], 42, &types), None);
            for n in 0..10 {
                let bytes: Vec<u8> = (0..n).collect();
                assert_eq!(unbase64(&base64(&bytes)), Some(bytes));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_statement_the_store_refused_is_jadpos_mistake() {
        let places = crate::Places::new();
        let host = crate::MemoryHost::new();
        let mut ctx = Ctx::new(&places, &host, crate::Env::default());
        let stopped = refused(
            &mut ctx,
            StoreError::Statement("42883: operator does not exist: text + text".into()),
            0,
        );
        let Interrupt::Fault(fault) = stopped else {
            panic!("a fault");
        };
        assert_eq!(
            fault.kind,
            FaultKind::Defect(
                "a statement Jadpo wrote that the store refused (42883: operator does not exist: text + text)"
                    .into()
            )
        );
    }

    /// A value a column can't hold, or a stored value that isn't one of
    /// its column's, is a fault named by what it is and the rule, never
    /// the value, which may be personal or secret and would reach a log
    /// (chunk 5.5w2), as a `DecodeProblem` names the field and the rule.
    #[test]
    fn a_fault_never_carries_the_value() {
        let secret = "hunter2-correct-horse";
        for (value, ty) in [
            (Value::text(secret), SqlType::Int8),
            (Value::text(secret), SqlType::Uuid),
            (Value::Int(9_876_543_210), SqlType::Int4),
            (Value::Int(9_876_543_210), SqlType::Text),
            (
                Value::record("Card", vec![("number".into(), Value::text(secret))]),
                SqlType::Text,
            ),
            (
                Value::list(vec![Value::text(secret)]),
                SqlType::Int8.array(),
            ),
        ] {
            let why = to_sql(&value, ty).expect_err("refused");
            assert!(
                !why.contains(secret) && !why.contains("9876543210"),
                "{why}"
            );
        }
        let column = Column::Enum {
            ty: "TodoStatus".into(),
            variants: vec!["open".into(), "done".into()],
        };
        let why = from_sql(&SqlValue::Text(secret.into()), &column).expect_err("not a variant");
        assert!(!why.contains(secret), "{why}");
    }

    #[test]
    fn values_and_columns_round_trip() {
        let uuid = Value::uuid("0192f0c4-3b5e-7c1a-9d2e-4f6a8b0c1d2e");
        let sql = to_sql(&uuid, SqlType::Uuid).unwrap();
        assert_eq!(from_sql(&sql, &Column::Uuid).unwrap(), uuid);
        let instant = Value::Instant(1_760_000_000_123);
        let sql = to_sql(&instant, SqlType::Timestamp).unwrap();
        assert_eq!(from_sql(&sql, &Column::Instant).unwrap(), instant);
        let open = Value::variant("TodoStatus", 0, "open", Vec::new());
        let sql = to_sql(&open, SqlType::Text).unwrap();
        let column = Column::Enum {
            ty: "TodoStatus".into(),
            variants: vec!["open".into(), "done".into()],
        };
        assert_eq!(from_sql(&sql, &column).unwrap(), open);
        assert_eq!(
            to_sql(&Value::list(vec![Value::Int(1)]), SqlType::Int8.array()).unwrap(),
            SqlValue::Array(SqlType::Int8, vec![SqlValue::Int8(1)])
        );
        assert_eq!(
            to_sql(&Value::None, SqlType::Text).unwrap(),
            SqlValue::Null(SqlType::Text)
        );
        assert!(from_sql(&SqlValue::Text("gone".into()), &column).is_err());
    }
}
