//! A person's export (area 15, "Export"): `User.export(id)`, the same on
//! both executors through [`crate::store`]. It asks first whether the
//! caller may read the person's record, then reads each entity's records
//! of theirs in each store, in a read-only transaction per store so it can
//! read a store the running action doesn't write to, and writes them as
//! one JSON document, one list an entity, each field in the API's form. Someone nothing is held about,
//! or a caller who may not read their record, gets an export of nothing.

use std::rc::Rc;

use jadpo_std::erasure::{ExportKind, ExportPlan, PERSONAL_DATA_EXPORT};
use jadpo_value::Value;

use crate::Ctx;
use crate::read::{Found, Interrupt, JsonCodecs, defect, read_detached};

/// `User.export(id)`, `args[0]` the person's `id`.
pub(crate) async fn export(
    ctx: &mut Ctx<'_>,
    plan: &ExportPlan,
    json: JsonCodecs<'_>,
    args: &[Value],
    site: u32,
) -> Result<Value, Interrupt> {
    let found = read_detached(ctx, &plan.access, json, args, site).await?;
    if !matches!(found, Found::Value(Value::Record(_))) {
        return Ok(built(nothing(), 0));
    }
    // One list an entity, in the order their first records were read.
    let mut lists: Vec<(String, Vec<String>)> = Vec::new();
    let mut records: i64 = 0;
    for part in &plan.parts {
        let Found::Value(Value::List(rows)) =
            read_detached(ctx, &part.read, json, args, site).await?
        else {
            return Err(defect(ctx, "an export's records that aren't a list", site));
        };
        for row in rows.iter() {
            let Value::Record(row) = row else {
                return Err(defect(ctx, "an exported row that isn't a record", site));
            };
            let mut object = String::from("{");
            for (at, (name, number)) in part.fields.iter().enumerate() {
                let Some(codec) = json.codec_of(*number) else {
                    return Err(defect(ctx, "an exported field with no codec", site));
                };
                let Some(value) = row.get(name) else {
                    return Err(defect(
                        ctx,
                        "an exported field the record doesn't hold",
                        site,
                    ));
                };
                if at > 0 {
                    object.push(',');
                }
                object.push_str(&quoted(name));
                object.push(':');
                object.push_str(&json.codecs.write_value(codec, value));
            }
            object.push('}');
            records += 1;
            match lists.iter_mut().find(|(entity, _)| *entity == part.entity) {
                Some((_, rows)) => rows.push(object),
                None => lists.push((part.entity.clone(), vec![object])),
            }
        }
    }
    let mut document = String::from("{\"entities\":{");
    for (at, (entity, rows)) in lists.iter().enumerate() {
        if at > 0 {
            document.push(',');
        }
        document.push_str(&quoted(entity));
        document.push_str(":[");
        document.push_str(&rows.join(","));
        document.push(']');
    }
    document.push_str("},\"kinds\":[");
    let kinds: Vec<String> = plan
        .kinds
        .iter()
        .filter(|kind| lists.iter().any(|(entity, _)| *entity == kind.entity))
        .map(|kind| kind_json(ctx, kind))
        .collect();
    document.push_str(&kinds.join(","));
    document.push_str("]}");
    Ok(built(document, records))
}

/// How long one kind of data is kept and which recipients receive it.
fn kind_json(ctx: &Ctx<'_>, kind: &ExportKind) -> String {
    let retention: Vec<String> = kind
        .retention
        .iter()
        .map(|kept| {
            let field = kept
                .field
                .as_ref()
                .map_or("null".to_string(), |field| quoted(field));
            // In the API's form, as a duration is sent.
            let after = ctx
                .setting(kept.setting)
                .and_then(|value| {
                    let entry = jadpo_value::standard::entry_of(value)?;
                    jadpo_value::standard::canonical(entry, value).ok()
                })
                .map_or("null".to_string(), |text| quoted(&text));
            format!(
                "{{\"field\":{field},\"after\":{after},\"since\":{}}}",
                quoted(&kept.since)
            )
        })
        .collect();
    let recipients: Vec<String> = kind.recipients.iter().map(|name| quoted(name)).collect();
    format!(
        "{{\"entity\":{},\"retention\":[{}],\"recipients\":[{}]}}",
        quoted(&kind.entity),
        retention.join(","),
        recipients.join(",")
    )
}

/// `text` as a JSON string.
fn quoted(text: &str) -> String {
    serde_json::to_string(text).unwrap_or_else(|_| "\"\"".to_string())
}

/// An export holding nothing.
fn nothing() -> String {
    "{\"entities\":{},\"kinds\":[]}".to_string()
}

fn built(document: String, records: i64) -> Value {
    Value::record(
        PERSONAL_DATA_EXPORT,
        vec![
            (Rc::from("json"), Value::text(&document)),
            (Rc::from("records"), Value::Int(records)),
        ],
    )
}

/// How many records the export of the person with `id` holds, as the runtime
/// asks after a test's erasure has run: none, when erasure did what it says.
pub(crate) async fn records_held(
    ctx: &mut Ctx<'_>,
    plan: &ExportPlan,
    json: JsonCodecs<'_>,
    id: &str,
) -> Result<i64, Interrupt> {
    let args = [Value::uuid(id)];
    let value = export(ctx, plan, json, &args, 0).await?;
    match value {
        Value::Record(record) => match record.get("records") {
            Some(Value::Int(records)) => Ok(*records),
            _ => Ok(0),
        },
        _ => Ok(0),
    }
}
