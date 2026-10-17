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

use std::collections::BTreeMap;
use std::rc::Rc;

use jadpo_std::policy::CallerPart;
use jadpo_std::read::{Cardinality, Column, Field, IncludeShape, ReadPlan, Select, Size, Source};
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
    args: &[Value],
    site: u32,
) -> Result<Found, Interrupt> {
    let now = ctx.now();
    let mut env = Env {
        args,
        now,
        size: 0,
        cursor: Vec::new(),
        keys: None,
        callers: callers(ctx),
        unpoliced: ctx.unpoliced(),
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
    let mut rows = run(transaction, &plan.select, env).await?;
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
                    now: env.now,
                    size: env.size,
                    cursor: Vec::new(),
                    keys: Some(SqlValue::Array(element, keys)),
                    callers: env.callers.clone(),
                    unpoliced: env.unpoliced,
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
            let mut fields = Vec::with_capacity(select.record.fields.len());
            for (name, field) in &select.record.fields {
                let value = match field {
                    Field::Column(at) => {
                        field_value(row, &select.columns, *at).map_err(Problem::Defect)?
                    }
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
                            IncludeShape::Optional => {
                                records.into_iter().next().unwrap_or(Value::None)
                            }
                            IncludeShape::One => match records.into_iter().next() {
                                Some(value) => value,
                                // The reference holds, so policy hid its
                                // record from the caller (area 9).
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
            out.push(Value::record(&select.record.ty, fields));
        }
        Ok(out)
    })
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

/// A value as a parameter of type `ty`.
pub fn to_sql(value: &Value, ty: SqlType) -> Result<SqlValue, String> {
    Ok(match (value, ty) {
        (Value::None, ty) => SqlValue::Null(ty),
        (Value::Bool(b), SqlType::Bool) => SqlValue::Bool(*b),
        (Value::Int(n), SqlType::Int8) => SqlValue::Int8(*n),
        (Value::Int(n), SqlType::Int4) => SqlValue::Int4(
            i32::try_from(*n).map_err(|_| "a whole number too large for its column")?,
        ),
        (Value::Int(n), SqlType::Int2) => SqlValue::Int2(
            i16::try_from(*n).map_err(|_| "a whole number too large for its column")?,
        ),
        (Value::Int(n), SqlType::Numeric) => SqlValue::Numeric(n.to_string()),
        (Value::Decimal(d), SqlType::Numeric) => SqlValue::Numeric(d.to_string()),
        (Value::Text(text), SqlType::Text) => SqlValue::Text(text.to_string()),
        (Value::Uuid(text) | Value::Text(text), SqlType::Uuid) => {
            SqlValue::Uuid(uuid_bytes(text).ok_or("text that isn't a UUID")?)
        }
        (Value::Uuid(text), SqlType::Text) => SqlValue::Text(text.to_string()),
        (Value::Bytes(bytes), SqlType::Bytes) => SqlValue::Bytes(bytes.to_vec()),
        (Value::Instant(millis), SqlType::Timestamp) => {
            SqlValue::Timestamp(millis.checked_mul(1000).ok_or("an instant out of range")?)
        }
        (Value::Date(day), SqlType::Date) => SqlValue::Date(*day),
        (Value::Duration(millis), SqlType::Int8) => SqlValue::Int8(*millis),
        (Value::Variant(variant), SqlType::Text) if variant.fields.is_empty() => {
            SqlValue::Text(variant.name.to_string())
        }
        (Value::List(items) | Value::Set(items), SqlType::Array(inner)) => SqlValue::Array(
            *inner,
            items
                .iter()
                .map(|item| to_sql(item, *inner))
                .collect::<Result<Vec<_>, _>>()?,
        ),
        (value, ty) => {
            return Err(format!(
                "a value ({value}) that a `{ty:?}` column can't hold"
            ));
        }
    })
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
        (SqlValue::Text(text), Column::Text) => Value::text(text),
        (SqlValue::Bytes(bytes), Column::Bytes) => Value::Bytes(Rc::from(bytes.as_slice())),
        (SqlValue::Uuid(bytes), Column::Uuid) => Value::uuid(&uuid_text(bytes)),
        (SqlValue::Timestamp(micros), Column::Instant) => Value::Instant(micros.div_euclid(1000)),
        (SqlValue::Date(day), Column::Date) => Value::Date(*day),
        (SqlValue::Text(name), Column::Enum { ty, variants }) => {
            let ordinal = variants
                .iter()
                .position(|variant| variant == name)
                .ok_or_else(|| format!("`{name}`, which isn't a variant of `{ty}`"))?;
            Value::variant(ty, ordinal as u32, name, Vec::new())
        }
        (sql, column) => {
            return Err(format!("a `{:?}` column read as {column:?}", sql.ty()));
        }
    })
}

/// The value of the field whose column is at `at` of `row`: the column's
/// own, or for an amount of money, its minor units and the currency code
/// in the column after them (area 19, "Money and payments").
pub fn field_value(row: &[SqlValue], columns: &[Column], at: u32) -> Result<Value, String> {
    let at = at as usize;
    let (Some(sql), Some(column)) = (row.get(at), columns.get(at)) else {
        return Err("a row without a column it needs".into());
    };
    if *column != Column::Money {
        return from_sql(sql, column);
    }
    match (sql, row.get(at + 1)) {
        (SqlValue::Null(_), Some(SqlValue::Null(_))) => Ok(Value::None),
        (SqlValue::Int8(minor), Some(SqlValue::Text(code))) => {
            let currency = jadpo_std::money::ordinal_of_code(code.trim())
                .ok_or_else(|| format!("`{code}`, which isn't a currency this release knows"))?;
            Ok(jadpo_value::money::value(currency, *minor))
        }
        _ => Err("an amount of money without its two columns".into()),
    }
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
fn uuid_bytes(text: &str) -> Option<[u8; 16]> {
    let hex: Vec<u8> = text.bytes().filter(|b| *b != b'-').collect();
    if hex.len() != 32 {
        return None;
    }
    let mut out = [0u8; 16];
    for (byte, pair) in out.iter_mut().zip(hex.chunks(2)) {
        let [high, low] = pair else { return None };
        let high = (*high as char).to_digit(16)?;
        let low = (*low as char).to_digit(16)?;
        *byte = u8::try_from(high * 16 + low).ok()?;
    }
    Some(out)
}

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
