//! The audit trail (area 16, "Audit trail"; chunk B11), run the same way
//! for both executors from the plans `jadpo-plan` decides
//! ([`jadpo_std::audit`]).
//!
//! A write of an audited entity reads and locks the record it changes
//! first ([`before`]), then, once its statement has changed the row,
//! adds one `jadpo_audit` row in the same transaction ([`record`]): who
//! made the change, through which action, request and trace, and each
//! changed field's value before and after as canonical text. When
//! something subscribes to `AuditEntryRecorded`, the entry is emitted as
//! that event too, into the action's outbox. Rows are only ever added;
//! the purge schedule deletes them when the entity's audit retention ends,
//! and a person's erasure with every record it deletes.
//!
//! [`history`] reads a page of one record's entries, newest first, for a
//! caller the record's `read` grant lets see it, leaving out the fields
//! whose own `policy:` keeps them from the caller.

use std::rc::Rc;

use jadpo_std::audit::{AuditPlan, HistoryPlan};
use jadpo_std::read::Source;
use jadpo_std::{AuditBy, AuditChange};
use jadpo_value::Value;

use crate::read::{Interrupt, JsonCodecs, cursor, defect, invalid_cursor, refused, to_sql};
use crate::{Ctx, SqlType, SqlValue};

/// Adds one entry, its place the record's next: `$1` the entity's born
/// name, `$2` the record's `id`, then the entry's columns in order.
pub const INSERT_AUDIT: &str = "INSERT INTO \"jadpo_audit\" (\"entity\", \"record_id\", \"seq\", \"at\", \"change\", \"by\", \"user_id\", \"service_id\", \"action\", \"request\", \"trace\", \"changes\", \"encoding\") SELECT $1::text, $2::uuid, COALESCE(MAX(\"seq\"), 0) + 1, $3, $4, $5, $6, $7, $8, $9, $10, $11::jsonb, $12 FROM \"jadpo_audit\" WHERE \"entity\" = $1::text AND \"record_id\" = $2::uuid";

/// One page of a record's entries, newest first: `$1` the entity's born
/// name, `$2` the record's `id`, `$3` the `seq` the page starts before,
/// `NULL` for the first, and `$4` one more than the page holds.
pub const HISTORY_ENTRIES: &str = "SELECT \"seq\", \"at\", \"change\", \"by\", \"user_id\", \"service_id\", \"action\", \"request\", \"trace\", \"changes\"::text, \"encoding\" FROM \"jadpo_audit\" WHERE \"entity\" = $1 AND \"record_id\" = $2 AND ($3::int8 IS NULL OR \"seq\" < $3::int8) ORDER BY \"seq\" DESC LIMIT $4";

/// The record a change starts from, as its write's own returned row is
/// read: `None` when there is no such record, which the write itself
/// then reports.
pub(crate) async fn before(
    ctx: &mut Ctx<'_>,
    plan: &AuditPlan,
    store: &str,
    id: Value,
    json: JsonCodecs<'_>,
    site: u32,
) -> Result<Option<Value>, Interrupt> {
    let Some(before) = &plan.before else {
        return Ok(None);
    };
    read_record(ctx, before, store, id, json, site).await
}

/// The record `read` reads by `id`, as its write's own returned row is
/// read; `None` when there is none.
pub(crate) async fn read_record(
    ctx: &mut Ctx<'_>,
    before: &jadpo_std::audit::AuditBefore,
    store: &str,
    id: Value,
    json: JsonCodecs<'_>,
    site: u32,
) -> Result<Option<Value>, Interrupt> {
    let id = to_sql(&id, SqlType::Uuid).map_err(|what| defect(ctx, &what, site))?;
    let rows = match ctx.transaction(store).await {
        Ok(transaction) => transaction.query(&before.sql, &[id]).await,
        Err(error) => Err(error),
    };
    let rows = rows.map_err(|error| refused(ctx, error, site))?;
    let Some(row) = rows.first() else {
        return Ok(None);
    };
    record_of(ctx, &before.returned, row, json, site).map(Some)
}

/// A row as the record `returned` says it becomes.
pub(crate) fn record_of(
    ctx: &mut Ctx<'_>,
    returned: &jadpo_std::write::Returned,
    row: &[SqlValue],
    json: JsonCodecs<'_>,
    site: u32,
) -> Result<Value, Interrupt> {
    let mut fields = Vec::with_capacity(returned.record.fields.len());
    let started = ctx.started();
    let ring = started.field_ring();
    for (name, field) in &returned.record.fields {
        let jadpo_std::read::Field::Column(at) = field else {
            return Err(defect(ctx, "an audited record with an include", site));
        };
        match crate::read::stored_value(row, &returned.columns, &returned.held, *at, json, ring) {
            Ok(value) => fields.push((Rc::from(name.as_str()), value)),
            Err(what) => return Err(defect(ctx, &what, site)),
        }
    }
    Ok(Value::record(&returned.record.ty, fields))
}

/// One changed field: its name and its text before and after.
type Changed = (String, Option<String>, Option<String>);

/// Adds the entry for a change from `before` to `after` (either `none`
/// for a create or a delete) in `store`'s open transaction, and emits it
/// as `AuditEntryRecorded` when something listens.
pub(crate) async fn record(
    ctx: &mut Ctx<'_>,
    plan: &AuditPlan,
    store: &str,
    before: Option<&Value>,
    after: Option<&Value>,
    json: JsonCodecs<'_>,
    site: u32,
) -> Result<(), Interrupt> {
    let change = match (before, after) {
        (None, Some(_)) => AuditChange::Created,
        (Some(_), Some(_)) => AuditChange::Updated,
        (Some(_), None) => AuditChange::Deleted,
        (None, None) => return Err(defect(ctx, "an audited change with no record", site)),
    };
    let id = match after.or(before) {
        Some(Value::Record(record)) => record.get("id").cloned().unwrap_or(Value::None),
        _ => Value::None,
    };
    let mut changed: Vec<Changed> = Vec::new();
    for field in &plan.fields {
        let of = |value: Option<&Value>| match value {
            Some(Value::Record(record)) => record.get(&field.name).cloned().unwrap_or(Value::None),
            _ => Value::None,
        };
        // A sealed field's history says that it changed, never what it
        // holds, and a history kept in clear couldn't say more (area 16;
        // chunk P9).
        if field.sealed {
            if of(before) != of(after) {
                changed.push((field.name.clone(), None, None));
            }
            continue;
        }
        let was =
            text_of(&of(before), field.json, json).map_err(|what| defect(ctx, &what, site))?;
        let now = text_of(&of(after), field.json, json).map_err(|what| defect(ctx, &what, site))?;
        if was != now {
            changed.push((field.name.clone(), was, now));
        }
    }
    let (by, user, service) = who(ctx);
    let action = action_of(ctx);
    let request = ctx.request_id().map(str::to_string);
    let trace = ctx.trace().map(|trace| hex(&trace.trace_id));
    let at = ctx.now();
    let changes = serde_json::Value::Array(
        changed
            .iter()
            .map(|(field, was, now)| {
                serde_json::json!({ "field": field, "before": was, "after": now })
            })
            .collect(),
    )
    .to_string();
    let record_id = to_sql(&id, SqlType::Uuid).map_err(|what| defect(ctx, &what, site))?;
    let text = |value: &Option<String>| match value {
        Some(text) => SqlValue::Text(text.clone()),
        None => SqlValue::Null(SqlType::Text),
    };
    let uuid = |value: Option<[u8; 16]>| match value {
        Some(id) => SqlValue::Uuid(id),
        None => SqlValue::Null(SqlType::Uuid),
    };
    let params = [
        SqlValue::Text(plan.born.clone()),
        record_id,
        crate::outbox::micros(at),
        SqlValue::Text(change.name().to_string()),
        SqlValue::Text(by.name().to_string()),
        uuid(user),
        uuid(service),
        text(&action),
        text(&request),
        text(&trace),
        SqlValue::Text(changes.clone()),
        crate::read::encoding_value(),
    ];
    let written = match ctx.transaction(store).await {
        Ok(transaction) => transaction.execute(INSERT_AUDIT, &params).await,
        Err(error) => Err(error),
    };
    written.map_err(|error| refused(ctx, error, site))?;
    if plan.subscribers.is_empty() {
        return Ok(());
    }
    let entry = Entry {
        at,
        change,
        by,
        user,
        service,
        action,
        request,
        trace,
        fields: changed,
    };
    let payload = serde_json::json!({
        "entity": plan.entity,
        "record_id": match &id { Value::Uuid(text) => text.to_string(), _ => String::new() },
        "entry": entry.json(),
    })
    .to_string();
    let mut value = vec![
        (Rc::from("entity"), Value::text(&plan.entity)),
        (Rc::from("record_id"), id),
    ];
    value.extend(entry.fields_of());
    let event = crate::Event {
        key: jadpo_std::audit::AUDIT_EVENT_KEY.to_string(),
        value: Value::record(jadpo_std::audit::AUDIT_ENTRY_RECORDED, value),
        payload,
        subscribers: plan.subscribers.clone(),
    };
    ctx.emit(event).map_err(|refused| {
        let what = match refused {
            crate::EmitRefused::OutsideAction => "an audited change outside an action",
            crate::EmitRefused::TooMany => "more than 1,000 events emitted by one action",
            crate::EmitRefused::TooLarge => "an audit entry's event past 64 KB",
        };
        defect(ctx, what, site)
    })
}

/// A value as its history keeps it: its canonical text, as `text.from`
/// gives it; its JSON for a JSON-held field; a variant's name for an
/// enum the application declares; `None` for no value.
fn text_of(
    value: &Value,
    codec: Option<u32>,
    json: JsonCodecs<'_>,
) -> Result<Option<String>, String> {
    if matches!(value, Value::None) {
        return Ok(None);
    }
    if let Some(codec) = codec {
        return match json.write(codec, value)? {
            SqlValue::Json(text) => Ok(Some(text)),
            _ => Ok(None),
        };
    }
    match jadpo_value::standard::from(value) {
        Ok(Value::Text(text)) => Ok(Some(text.to_string())),
        _ => match value {
            Value::Variant(variant) if variant.fields.is_empty() => {
                Ok(Some(variant.name.to_string()))
            }
            _ => Err("an audited field's value with no text form".to_string()),
        },
    }
}

/// Who is making the change: the signed-in user or service, the
/// application for a schedule's run or a subscriber's delivery, or no one.
fn who(ctx: &Ctx<'_>) -> (AuditBy, Option<[u8; 16]>, Option<[u8; 16]>) {
    use jadpo_std::policy::CallerPart;
    if let SqlValue::Uuid(id) = ctx.caller_param(CallerPart::User) {
        return (AuditBy::User, Some(id), None);
    }
    if let SqlValue::Uuid(id) = ctx.caller_param(CallerPart::Service) {
        return (AuditBy::Service, None, Some(id));
    }
    if ctx.caller_param(CallerPart::System) == SqlValue::Bool(true) {
        return (AuditBy::Application, None, None);
    }
    (AuditBy::NoCaller, None, None)
}

/// The outermost action running: `pay_invoice`.
fn action_of(ctx: &Ctx<'_>) -> Option<String> {
    let places = ctx.places();
    ctx.frames().into_iter().find_map(|frame| {
        places
            .label(frame.callable)
            .strip_prefix("action ")
            .map(str::to_string)
    })
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// One entry as `history` and `AuditEntryRecorded` give it.
struct Entry {
    at: i64,
    change: AuditChange,
    by: AuditBy,
    user: Option<[u8; 16]>,
    service: Option<[u8; 16]>,
    action: Option<String>,
    request: Option<String>,
    trace: Option<String>,
    fields: Vec<Changed>,
}

impl Entry {
    /// Its fields in `AuditEntry`'s order.
    fn fields_of(&self) -> Vec<(Rc<str>, Value)> {
        let text = |value: &Option<String>| value.as_deref().map_or(Value::None, Value::text);
        let uuid = |value: &Option<[u8; 16]>| {
            value.map_or(Value::None, |id| Value::uuid(&crate::read::uuid_text(&id)))
        };
        let changes = self
            .fields
            .iter()
            .map(|(field, was, now)| {
                Value::record(
                    jadpo_std::audit::AUDIT_FIELD_CHANGE,
                    vec![
                        (Rc::from("field"), Value::text(field)),
                        (Rc::from("before"), text(was)),
                        (Rc::from("after"), text(now)),
                    ],
                )
            })
            .collect();
        vec![
            (Rc::from("at"), Value::Instant(self.at)),
            (
                Rc::from("change"),
                variant(AuditChange::NAME, self.change.name()),
            ),
            (Rc::from("by"), variant(AuditBy::NAME, self.by.name())),
            (Rc::from("user_id"), uuid(&self.user)),
            (Rc::from("service_id"), uuid(&self.service)),
            (Rc::from("action"), text(&self.action)),
            (Rc::from("request"), text(&self.request)),
            (Rc::from("trace"), text(&self.trace)),
            (Rc::from("fields"), Value::list(changes)),
        ]
    }

    /// Its value as `history` gives it.
    fn value(&self) -> Value {
        Value::record(jadpo_std::audit::AUDIT_ENTRY, self.fields_of())
    }

    /// The JSON an `AuditEntryRecorded`'s payload keeps it as.
    fn json(&self) -> serde_json::Value {
        let uuid = |value: &Option<[u8; 16]>| value.map(|id| crate::read::uuid_text(&id));
        serde_json::json!({
            "at": self.at,
            "change": self.change.name(),
            "by": self.by.name(),
            "user_id": uuid(&self.user),
            "service_id": uuid(&self.service),
            "action": self.action,
            "request": self.request,
            "trace": self.trace,
            "fields": self.fields.iter().map(|(field, was, now)| {
                serde_json::json!({ "field": field, "before": was, "after": now })
            }).collect::<Vec<_>>(),
        })
    }

    /// An entry from the JSON [`Entry::json`] writes.
    fn from_json(object: &serde_json::Value) -> Option<Entry> {
        let text = |name: &str| {
            object
                .get(name)
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        };
        let uuid = |name: &str| text(name).and_then(|id| crate::http::magic_links::parse_uuid(&id));
        let fields = object
            .get("fields")?
            .as_array()?
            .iter()
            .map(|field| {
                let part = |name: &str| {
                    field
                        .get(name)
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_string)
                };
                Some((part("field")?, part("before"), part("after")))
            })
            .collect::<Option<Vec<_>>>()?;
        Some(Entry {
            at: object.get("at")?.as_i64()?,
            change: AuditChange::from_name(&text("change")?)?,
            by: AuditBy::from_name(&text("by")?)?,
            user: uuid("user_id"),
            service: uuid("service_id"),
            action: text("action"),
            request: text("request"),
            trace: text("trace"),
            fields,
        })
    }
}

fn variant(ty: &str, name: &str) -> Value {
    let ordinal = jadpo_std::variant_ordinal(ty, name).unwrap_or_default();
    Value::variant(ty, ordinal, name, Vec::new())
}

/// The value of an `AuditEntryRecorded` from its outbox payload, as the
/// sweep reads a delivery back; `None` when `key` isn't its.
pub(crate) fn read_payload(key: &str, payload: &str) -> Option<Result<Value, String>> {
    if key != jadpo_std::audit::AUDIT_EVENT_KEY {
        return None;
    }
    let not = || "not the payload of `AuditEntryRecorded`".to_string();
    let read = || -> Option<Value> {
        let object: serde_json::Value = serde_json::from_str(payload).ok()?;
        let entity = object.get("entity")?.as_str()?;
        let record = object.get("record_id")?.as_str()?;
        let entry = Entry::from_json(object.get("entry")?)?;
        let mut fields = vec![
            (Rc::from("entity"), Value::text(entity)),
            (Rc::from("record_id"), Value::uuid(record)),
        ];
        fields.extend(entry.fields_of());
        Some(Value::record(
            jadpo_std::audit::AUDIT_ENTRY_RECORDED,
            fields,
        ))
    };
    Some(read().ok_or_else(not))
}

/// `Entity.history(id)`, or `history(id, after: cursor)` with `args[1]`:
/// a page of the record's entries, newest first.
pub(crate) async fn history(
    ctx: &mut Ctx<'_>,
    plan: &HistoryPlan,
    args: &[Value],
    site: u32,
) -> Result<Value, Interrupt> {
    let Some(id) = args.first() else {
        return Err(defect(ctx, "a history without its record's `id`", site));
    };
    let id = to_sql(id, SqlType::Uuid).map_err(|what| defect(ctx, &what, site))?;
    let start = match args.get(1) {
        None | Some(Value::None) => SqlValue::Null(SqlType::Int8),
        Some(Value::Text(text)) => {
            match cursor::decode(text, plan.tag, &[(0, jadpo_std::audit::CURSOR)]) {
                Some(values) => values
                    .into_iter()
                    .next()
                    .unwrap_or(SqlValue::Null(SqlType::Int8)),
                None => return Err(Interrupt::Failed(ctx.fail(invalid_cursor(), site))),
            }
        }
        Some(_) => return Err(defect(ctx, "a page cursor that isn't text", site)),
    };
    // Whether the caller may read the record, and which of its restricted
    // fields; without a caller, every entry and field.
    let mut readable: Vec<bool> = vec![true; plan.restricted.len()];
    let policed = !ctx.unpoliced();
    if policed {
        let now = ctx.now();
        let mut params = Vec::with_capacity(plan.params.len());
        for param in &plan.params {
            params.push(match &param.source {
                Source::Arg { index: 0, path } if path.is_empty() => id.clone(),
                Source::Caller(part) => ctx.caller_param(*part),
                Source::Stored(value) => value.clone(),
                Source::Now => SqlValue::Timestamp(now.saturating_mul(1000)),
                _ => return Err(defect(ctx, "a history parameter it can't bind", site)),
            });
        }
        let rows = match ctx.read_transaction(&plan.store).await {
            Ok(transaction) => transaction.query(&plan.access, &params).await,
            Err(error) => Err(error),
        };
        let rows = rows.map_err(|error| refused(ctx, error, site))?;
        let Some(row) = rows.first() else {
            return Ok(page(Vec::new(), None));
        };
        for (at, may) in readable.iter_mut().enumerate() {
            *may = row.get(at + 1) == Some(&SqlValue::Bool(true));
        }
    }
    let size = i64::from(jadpo_std::audit::HISTORY_PAGE);
    let params = [
        SqlValue::Text(plan.born.clone()),
        id,
        start,
        SqlValue::Int8(size + 1),
    ];
    let rows = match ctx.read_transaction(&plan.store).await {
        Ok(transaction) => transaction.query(HISTORY_ENTRIES, &params).await,
        Err(error) => Err(error),
    };
    let rows = rows.map_err(|error| refused(ctx, error, site))?;
    let more = rows.len() as i64 > size;
    let mut items = Vec::new();
    let mut last = None;
    for row in rows.iter().take(size as usize) {
        let Some(entry) = entry_of(row, plan, &readable, policed) else {
            return Err(defect(ctx, "an audit entry that doesn't read", site));
        };
        last = row.first().cloned();
        items.push(entry.value());
    }
    let next = match (more, last) {
        (true, Some(seq)) => cursor::encode(plan.tag, &[seq]),
        _ => None,
    };
    Ok(page(items, next))
}

fn page(items: Vec<Value>, next: Option<String>) -> Value {
    Value::record(
        crate::read::PAGE,
        vec![
            (Rc::from("items"), Value::list(items)),
            (
                Rc::from("next"),
                next.map_or(Value::None, |text| Value::text(&text)),
            ),
        ],
    )
}

/// One row of [`HISTORY_ENTRIES`] as an entry, without the restricted
/// fields `readable` says the caller may not read, or, when `policed`, a
/// field retired or dropped since, whose grant is gone (chunk V3).
fn entry_of(
    row: &[SqlValue],
    plan: &HistoryPlan,
    readable: &[bool],
    policed: bool,
) -> Option<Entry> {
    let text = |at: usize| match row.get(at) {
        Some(SqlValue::Text(text)) => Some(text.clone()),
        _ => None,
    };
    let uuid = |at: usize| match row.get(at) {
        Some(SqlValue::Uuid(id)) => Some(*id),
        _ => None,
    };
    if let Some(encoding) = row.get(10) {
        crate::read::kept_encoding(encoding).ok()?;
    }
    let at = match row.get(1) {
        Some(SqlValue::Timestamp(micros)) => micros.div_euclid(1000),
        _ => return None,
    };
    let changes: serde_json::Value = serde_json::from_str(&text(9)?).ok()?;
    let hidden = |field: &str| {
        (policed && !plan.fields.iter().any(|known| known == field))
            || plan
                .restricted
                .iter()
                .zip(readable)
                .any(|(restricted, may)| restricted == field && !may)
    };
    let fields = changes
        .as_array()?
        .iter()
        .filter_map(|change| {
            let part = |name: &str| {
                change
                    .get(name)
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string)
            };
            let field = part("field")?;
            (!hidden(&field)).then(|| (field, part("before"), part("after")))
        })
        .collect();
    Some(Entry {
        at,
        change: AuditChange::from_name(&text(2)?)?,
        by: AuditBy::from_name(&text(3)?)?,
        user: uuid(4),
        service: uuid(5),
        action: text(6),
        request: text(7),
        trace: text(8),
        fields,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A field retired since an entry was written has no grant any more, so
    /// a caller is never given what the entry holds of it (chunk V3).
    #[test]
    fn a_callers_history_leaves_out_a_field_retired_since() {
        let plan = HistoryPlan {
            store: "main".into(),
            born: "Invoice".into(),
            access: String::new(),
            params: Vec::new(),
            restricted: vec!["discount".into()],
            fields: vec!["number".into(), "discount".into()],
            tag: 0,
        };
        let row = vec![
            SqlValue::Int8(1),
            SqlValue::Timestamp(0),
            SqlValue::Text("updated".into()),
            SqlValue::Text("no_caller".into()),
            SqlValue::Null(SqlType::Uuid),
            SqlValue::Null(SqlType::Uuid),
            SqlValue::Null(SqlType::Text),
            SqlValue::Null(SqlType::Text),
            SqlValue::Null(SqlType::Text),
            SqlValue::Text(
                r#"[{"field":"number"},{"field":"discount"},{"field":"margin","before":"12"}]"#
                    .into(),
            ),
            crate::read::encoding_value(),
        ];
        let names = |entry: Entry| -> Vec<String> {
            entry.fields.into_iter().map(|(name, ..)| name).collect()
        };
        let shown = entry_of(&row, &plan, &[true], true).expect("an entry");
        assert_eq!(names(shown), ["number", "discount"]);
        let hidden = entry_of(&row, &plan, &[false], true).expect("an entry");
        assert_eq!(names(hidden), ["number"]);
        // Without a caller, every field, as a test's call reads them.
        let every = entry_of(&row, &plan, &[true], false).expect("an entry");
        assert_eq!(names(every), ["number", "discount", "margin"]);
    }
}
