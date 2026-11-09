//! What a data request leads to (area 15, "Exports", "Erasures" and
//! "Deadlines and events"), the same on both executors, since all of it is
//! here.
//!
//! Each piece of work is a *step*: an outbox row for
//! [`jadpo_std::privacy::STEP_EVENT`] with one delivery, due when the step
//! is, so a step is retried and shown as any delivery is and needs no
//! schedule. Each finds what state its request is in first, so a repeat, or
//! a request cancelled or moved on since, does nothing.
//!
//! - **An export** is made when it is asked for, encrypted under a key
//!   derived from a random token, and kept in the store `exports:` names.
//!   Only the token's digest is kept. The file is in no one's hands but the
//!   link's: the token goes into `PrivacyExportReady`'s link and nowhere
//!   else. Its link works once: the file goes as it is given, or at the
//!   latest as the link ends.
//! - **An erasure** runs when its cooling-off ends, in the person's own
//!   store, with the same statement `User.erase` runs. What the law keeps,
//!   the person's address and the language are read first, and kept only in
//!   the next step's outbox row, since nothing else can say them any more.
//! - **Completing it** waits for each generated delivery of the person's
//!   `PersonErased`, in the store each one reaches, at the provider each
//!   one asks and in the log. One that gave up marks the request `stalled`
//!   and it stays open for an operator.
//! - **The deadline** raises an alert a week before and
//!   `PrivacyRequestOverdue` at the deadline, for what is still open then.

use std::rc::Rc;

use jadpo_std::files::{DELETE, PUT, READ};
use jadpo_std::http::PRIVACY_ALERT_BEFORE_MS;
use jadpo_std::privacy::{
    ERASING, Exports, PrivacySteps, RECEIVED, STALLED, STEP_EVENT, STEPS_SUBSCRIBER, Step,
    export_key,
};
use jadpo_std::read::SqlType;
use jadpo_std::subscriber::Repeat;
use jadpo_std::{PrivacyRequestKind, erasure::ExportPlan};
use jadpo_value::Value;
use serde_json::{Value as Json, json};

use super::magic_links::{format_uuid, millis, new_token, parse_uuid, token_bytes, verifier};
use super::sign_in_events::{self, SignInEvent};
use crate::capability::{Answer, Call};
use crate::outbox::{self, Delivery, Sealed, Trace, micros};
use crate::read::Interrupt;
use crate::{Ctx, Executor, Host, Outcome, SqlValue, StoreError, Transaction};

/// A request's state and what its steps read of it.
pub(crate) const READ_REQUEST: &str = "SELECT \"kind\", \"state\", \"person\", \"locale\", \"deadline\", \"erases_at\", \"export_file\", \"export_expires_at\", \"created_by\" FROM \"jadpo_privacy_requests\" WHERE \"id\" = $1";

/// The same, held until the step commits.
pub(crate) const LOCK_REQUEST: &str = "SELECT \"kind\", \"state\", \"person\", \"locale\", \"deadline\", \"erases_at\", \"export_file\", \"export_expires_at\", \"created_by\" FROM \"jadpo_privacy_requests\" WHERE \"id\" = $1 FOR UPDATE";

/// An export made: its file, its link's verifier and when the link ends.
pub(crate) const EXPORT_READY: &str = "UPDATE \"jadpo_privacy_requests\" SET \"state\" = 'ready', \"open\" = NULL, \"export_file\" = $2, \"export_expires_at\" = $3, \"export_key\" = $4 WHERE \"id\" = $1 AND \"state\" = 'received' RETURNING \"id\"";

/// An export's link ended: the file gone and whatever was still waiting
/// for it with it.
pub(crate) const EXPORT_ENDED: &str = "UPDATE \"jadpo_privacy_requests\" SET \"state\" = CASE WHEN \"state\" = 'ready' THEN 'expired' ELSE \"state\" END, \"export_key\" = NULL, \"export_file\" = NULL WHERE \"id\" = $1 RETURNING \"id\"";

/// A person's open exports, ended as their erasure runs: the files that
/// hold what erasure deletes.
pub(crate) const EXPORTS_OF: &str = "UPDATE \"jadpo_privacy_requests\" SET \"state\" = CASE WHEN \"state\" = 'ready' THEN 'expired' ELSE \"state\" END, \"export_key\" = NULL, \"export_file\" = NULL WHERE \"person\" = $1 AND \"export_file\" IS NOT NULL RETURNING \"export_file\"";

/// An erasure run: waiting for what it asks of others.
pub(crate) const ERASING_NOW: &str = "UPDATE \"jadpo_privacy_requests\" SET \"state\" = 'erasing', \"open\" = NULL, \"erased_at\" = $2, \"cancel_key\" = NULL WHERE \"id\" = $1 AND \"state\" = 'received' RETURNING \"id\"";

/// What is outstanding of `$1`'s generated deliveries: those not yet done
/// and those parked for an operator.
pub(crate) const OUTSTANDING: &str = "SELECT count(*) FILTER (WHERE \"status\" <> 'succeeded')::int4, count(*) FILTER (WHERE \"status\" IN ('failed', 'outcome_unknown', 'diverged'))::int4 FROM \"jadpo_deliveries\" WHERE \"outbox_id\" = ANY($1::uuid[]) AND \"subscriber\" LIKE 'erasure %'";

/// A provider has given up: the request says so, while it still waits.
pub(crate) const STALLED_NOW: &str = "UPDATE \"jadpo_privacy_requests\" SET \"state\" = 'stalled' WHERE \"id\" = $1 AND \"state\" = 'erasing'";

/// Every provider has confirmed.
pub(crate) const COMPLETED_NOW: &str = "UPDATE \"jadpo_privacy_requests\" SET \"state\" = 'completed', \"completed_at\" = $2 WHERE \"id\" = $1 AND \"state\" IN ('erasing', 'stalled') RETURNING \"id\"";

/// What the completion email needs, gone from a step's row once it has
/// moved to the email's own.
pub(crate) const STEP_DONE: &str =
    "UPDATE \"jadpo_outbox\" SET \"payload\" = \"payload\" - 'to' WHERE \"id\" = $1";

/// The payload of the outbox row `$1`, as its email reads it.
pub(crate) const PAYLOAD: &str = "SELECT \"payload\"::text FROM \"jadpo_outbox\" WHERE \"id\" = $1";

/// An email's language, kept with its request.
pub(crate) const LOCALE_OF: &str =
    "SELECT \"locale\" FROM \"jadpo_privacy_requests\" WHERE \"id\" = $1";

/// A page of requests in the states `$1`, or every one when it is empty,
/// the nearest deadline first, at most `$2` after `$3`: what `jadpo privacy
/// requests` lists.
pub const LIST_BY_DEADLINE: &str = "SELECT \"id\", \"kind\", \"source\", \"state\", \"person\", \"created_by\", \"identity_check\", \"requested_at\", \"deadline\", \"erases_at\", \"export_expires_at\", \"completed_at\" FROM \"jadpo_privacy_requests\" WHERE (cardinality($1::text[]) = 0 OR \"state\" = ANY($1::text[])) ORDER BY \"deadline\", \"id\" LIMIT $2 OFFSET $3";

/// The same, the newest first.
pub const LIST_NEWEST: &str = "SELECT \"id\", \"kind\", \"source\", \"state\", \"person\", \"created_by\", \"identity_check\", \"requested_at\", \"deadline\", \"erases_at\", \"export_expires_at\", \"completed_at\" FROM \"jadpo_privacy_requests\" WHERE (cardinality($1::text[]) = 0 OR \"state\" = ANY($1::text[])) ORDER BY \"requested_at\" DESC, \"id\" LIMIT $2 OFFSET $3";

/// How many requests are in each state and, of those still to finish, how
/// many are past their deadline at `$1` and how many reach it by `$2`.
pub const COUNTS: &str = "SELECT \"state\", count(*)::int4, count(*) FILTER (WHERE \"state\" IN ('received', 'erasing', 'stalled') AND \"deadline\" <= $1)::int4, count(*) FILTER (WHERE \"state\" IN ('received', 'erasing', 'stalled') AND \"deadline\" > $1 AND \"deadline\" <= $2)::int4 FROM \"jadpo_privacy_requests\" GROUP BY \"state\"";

/// How much of a file one read asks a store for.
const WINDOW: usize = 1 << 16;

/// The identities of the steps a new request leaves, made before the
/// transaction is borrowed.
pub(crate) struct Timers {
    ids: Vec<[[u8; 16]; 2]>,
    store: String,
    trace: Option<Trace>,
}

impl Timers {
    pub(crate) fn prepare(ctx: &mut Ctx<'_>, store: &str) -> Option<Timers> {
        let mut ids = Vec::with_capacity(3);
        for _ in 0..3 {
            ids.push([ctx.new_id()?, ctx.new_id()?]);
        }
        Some(Timers {
            ids,
            store: store.to_string(),
            trace: ctx.trace(),
        })
    }

    /// Writes a new request's steps in `transaction`: what it asks for, and
    /// its deadline's alert and event. What is due at once, to deliver once
    /// it has committed.
    pub(crate) async fn write(
        &self,
        transaction: &mut dyn Transaction,
        request: [u8; 16],
        kind: PrivacyRequestKind,
        now: i64,
        deadline: i64,
        erases_at: Option<i64>,
    ) -> Result<Vec<Delivery>, StoreError> {
        let first = match (kind, erases_at) {
            (PrivacyRequestKind::Erasure, Some(at)) => (Step::Erase, at),
            _ => (Step::Export, now),
        };
        let plan = [
            first,
            (
                Step::Nearing,
                deadline.saturating_sub(PRIVACY_ALERT_BEFORE_MS),
            ),
            (Step::Overdue, deadline),
        ];
        let mut due = Vec::new();
        for ([outbox_id, delivery], (step, at)) in self.ids.iter().zip(plan) {
            let written = write_step(
                transaction,
                &self.store,
                [*outbox_id, *delivery],
                step_payload(request, step, None),
                at,
                self.trace,
            )
            .await?;
            if at <= now {
                due.push(written);
            }
        }
        Ok(due)
    }
}

/// `{"request": …, "step": …}` and whatever it carries.
fn step_payload(request: [u8; 16], step: Step, extra: Option<Json>) -> String {
    let mut object = json!({ "request": format_uuid(&request), "step": step.name() });
    if let (Some(extra), Some(object)) = (extra, object.as_object_mut())
        && let Json::Object(extra) = extra
    {
        object.extend(extra);
    }
    object.to_string()
}

/// One step's row and delivery in `transaction`, due at `at`: kept as an
/// event emitted then, so its budget counts from when it is due.
async fn write_step(
    transaction: &mut dyn Transaction,
    store: &str,
    ids: [[u8; 16]; 2],
    payload: String,
    at: i64,
    trace: Option<Trace>,
) -> Result<Delivery, StoreError> {
    let [outbox_id, delivery] = ids;
    let delivery = Delivery {
        store: store.to_string(),
        id: delivery,
        outbox: outbox_id,
        subscriber: STEPS_SUBSCRIBER.to_string(),
        event: STEP_EVENT.to_string(),
        value: step_value(&payload),
        emitted_at: at,
        trace,
        depth: 0,
    };
    let sealed = Sealed {
        outbox: vec![(outbox_id, STEP_EVENT.to_string(), payload)],
        deliveries: vec![delivery.clone()],
    };
    outbox::write(transaction, &sealed, at, trace, 0).await?;
    Ok(delivery)
}

/// A step's value, as its delivery reads it: its payload whole, since it
/// is the runtime's own and no application subscriber hears it.
fn step_value(payload: &str) -> Value {
    Value::record(
        "PrivacyStep",
        vec![(Rc::from("data"), Value::text(payload))],
    )
}

/// A step's value from its outbox payload.
pub(crate) fn read_step(payload: &str) -> Result<Value, String> {
    Ok(step_value(payload))
}

/// What a step read of its request.
struct Current {
    kind: String,
    state: String,
    person: [u8; 16],
    locale: String,
    deadline: i64,
    erases_at: Option<i64>,
    file: Option<String>,
}

fn current(row: &[SqlValue]) -> Option<Current> {
    match row {
        [
            SqlValue::Text(kind),
            SqlValue::Text(state),
            SqlValue::Uuid(person),
            SqlValue::Text(locale),
            deadline,
            erases_at,
            file,
            _export_expires_at,
            _created_by,
        ] => Some(Current {
            kind: kind.clone(),
            state: state.clone(),
            person: *person,
            locale: locale.clone(),
            deadline: millis(deadline)?,
            erases_at: millis(erases_at),
            file: match file {
                SqlValue::Text(file) => Some(file.clone()),
                _ => None,
            },
        }),
        _ => None,
    }
}

/// What a step ended in, as a delivery settles it.
fn nothing() -> Outcome {
    Outcome::Value(Value::Unit)
}

fn unavailable() -> Outcome {
    Outcome::Failed(Box::new(crate::unavailable()))
}

fn defect(ctx: &mut Ctx<'_>, what: &str) -> Outcome {
    Outcome::Fault(ctx.fault(
        crate::FaultKind::Defect(what.to_string()),
        crate::Frame::ENTRY,
    ))
}

fn store_failed(ctx: &mut Ctx<'_>, error: StoreError) -> Outcome {
    match error {
        StoreError::Fault(why) => {
            Outcome::Fault(ctx.fault(crate::FaultKind::Store(why), crate::Frame::ENTRY))
        }
        _ => unavailable(),
    }
}

fn interrupted(ctx: &mut Ctx<'_>, stop: Interrupt) -> Outcome {
    let _ = ctx;
    match stop {
        Interrupt::Failed(failure) => Outcome::Failed(failure),
        Interrupt::Fault(fault) => Outcome::Fault(fault),
    }
}

/// Runs the step `delivery` names, how it ended as any delivery's outcome
/// is settled.
pub(crate) async fn deliver(
    executor: &dyn Executor,
    ctx: &mut Ctx<'_>,
    delivery: &Delivery,
) -> Outcome {
    let Some(events) = executor.plan().events() else {
        return nothing();
    };
    let Some(steps) = events.plan.privacy.as_ref() else {
        return nothing();
    };
    let Value::Record(record) = &delivery.value else {
        return defect(ctx, "a data request's step that isn't a record");
    };
    let Some(Value::Text(data)) = record.get("data") else {
        return defect(ctx, "a data request's step with no data");
    };
    let Ok(payload) = serde_json::from_str::<Json>(data) else {
        return defect(ctx, "a data request's step that isn't JSON");
    };
    let text = |name: &str| payload.get(name).and_then(Json::as_str);
    let (Some(request), Some(step)) = (
        text("request").and_then(parse_uuid),
        text("step").and_then(Step::named),
    ) else {
        return defect(ctx, "a data request's step that names none");
    };
    match step {
        Step::Export => Box::pin(export(executor, ctx, steps, request)).await,
        Step::Erase => Box::pin(erase(executor, ctx, steps, request)).await,
        Step::Complete => Box::pin(complete(executor, ctx, steps, request, &payload)).await,
        Step::Expire => Box::pin(expire(ctx, steps, request)).await,
        Step::Nearing => Box::pin(nearing(ctx, steps, request)).await,
        Step::Overdue => Box::pin(overdue(executor, ctx, steps, request)).await,
    }
}

/// The key an export is encrypted under, derived from its link's token.
fn file_key(token: &[u8; 32]) -> [u8; 32] {
    blake3::derive_key("jadpo 2026-11-09 data request export file", token)
}

/// The key an export's file is checked with, derived from the token.
fn tag_key(token: &[u8; 32]) -> [u8; 32] {
    blake3::derive_key("jadpo 2026-11-09 data request export tag", token)
}

/// `document` encrypted under `token`: a tag over what follows, then the
/// text under a key stream, which only the link's token opens.
pub(crate) fn encrypt(token: &[u8; 32], document: &[u8]) -> Vec<u8> {
    let mut stream = vec![0u8; document.len()];
    blake3::Hasher::new_keyed(&file_key(token))
        .finalize_xof()
        .fill(&mut stream);
    let body: Vec<u8> = stream.iter().zip(document).map(|(k, b)| k ^ b).collect();
    let tag = blake3::keyed_hash(&tag_key(token), &body);
    let mut out = Vec::with_capacity(32 + body.len());
    out.extend_from_slice(tag.as_bytes());
    out.extend_from_slice(&body);
    out
}

/// What [`encrypt`] made, when `token` opens it.
pub(crate) fn decrypt(token: &[u8; 32], sealed: &[u8]) -> Option<Vec<u8>> {
    let tag: [u8; 32] = sealed.get(..32)?.try_into().ok()?;
    let body = sealed.get(32..)?;
    if blake3::keyed_hash(&tag_key(token), body) != blake3::Hash::from_bytes(tag) {
        return None;
    }
    let mut stream = vec![0u8; body.len()];
    blake3::Hasher::new_keyed(&file_key(token))
        .finalize_xof()
        .fill(&mut stream);
    Some(stream.iter().zip(body).map(|(k, b)| k ^ b).collect())
}

/// Reads the whole of the file `key` in `store`, `None` when it has gone.
async fn read_all(host: &dyn Host, store: &str, key: &str) -> Result<Option<Vec<u8>>, StoreError> {
    let Some(driver) = host.capability(store) else {
        return Err(StoreError::Unavailable);
    };
    let mut out = Vec::new();
    loop {
        let input = Value::record(
            "FileRead",
            vec![
                (Rc::from("key"), Value::text(key)),
                (Rc::from("from"), Value::Int(out.len() as i64)),
                (Rc::from("length"), Value::Int(WINDOW as i64)),
            ],
        );
        match driver
            .call(&Call::new(store, READ, input, Repeat::Safe))
            .await
        {
            Answer::Value(Value::Bytes(bytes)) => {
                out.extend_from_slice(&bytes);
                if bytes.len() < WINDOW {
                    return Ok(Some(out));
                }
            }
            Answer::Failed(code) if code == jadpo_std::files::NOT_FOUND => return Ok(None),
            _ => return Err(StoreError::Unavailable),
        }
    }
}

/// Deletes the file `key` from `store`, which a repeat does again safely.
async fn delete_file(host: &dyn Host, store: &str, key: &str) -> Result<(), StoreError> {
    let Some(driver) = host.capability(store) else {
        return Err(StoreError::Unavailable);
    };
    let input = Value::record("FileDelete", vec![(Rc::from("key"), Value::text(key))]);
    match driver
        .call(&Call::new(store, DELETE, input, Repeat::Keyed))
        .await
    {
        Answer::Value(_) => Ok(()),
        Answer::Failed(code) if code == jadpo_std::files::NOT_FOUND => Ok(()),
        _ => Err(StoreError::Unavailable),
    }
}

/// The export of the person's data: its document, kept encrypted, then
/// the request marked ready and `PrivacyExportReady` left for whoever
/// delivers it.
async fn export(
    executor: &dyn Executor,
    ctx: &mut Ctx<'_>,
    steps: &PrivacySteps,
    request: [u8; 16],
) -> Outcome {
    let Some(exports) = steps.exports.clone() else {
        return defect(ctx, "an export with no `exports:`");
    };
    let host = ctx.host();
    let Some(database) = host.database(&steps.store) else {
        return unavailable();
    };
    let id = SqlValue::Uuid(request);
    let found = match database
        .query_once(READ_REQUEST, std::slice::from_ref(&id))
        .await
    {
        Ok(rows) => rows,
        Err(_) => return unavailable(),
    };
    let Some(current) = found.first().and_then(|row| current(row)) else {
        return nothing();
    };
    if current.state != RECEIVED || current.kind != PrivacyRequestKind::Export.name() {
        return nothing();
    }
    let program = executor.plan();
    let Some(plan) = program
        .exports
        .iter()
        .find(|plan| plan.entity == steps.person)
    else {
        return defect(ctx, "an export of someone with no plan to read them");
    };
    let person = Value::uuid(&format_uuid(&current.person));
    let document =
        match crate::export::for_request(ctx, plan, program.json, &person, exports.keep_ms).await {
            Ok(document) => document,
            Err(stop) => return interrupted(ctx, stop),
        };
    let Some(token) = new_token(ctx) else {
        return defect(ctx, "an export with no randomness");
    };
    let Some(bytes) = token_bytes(&token) else {
        return defect(ctx, "an export token that isn't one");
    };
    let file = export_key(&format_uuid(&request));
    let sealed = encrypt(&bytes, document.as_bytes());
    let Some(driver) = ctx.host().capability(&exports.store) else {
        return unavailable();
    };
    let input = Value::record(
        "FilePut",
        vec![
            (Rc::from("key"), Value::text(&file)),
            (Rc::from("type"), Value::text("application/octet-stream")),
            (Rc::from("bytes"), Value::Bytes(Rc::from(sealed))),
        ],
    );
    match driver
        .call(&Call::new(&exports.store, PUT, input, Repeat::Keyed))
        .await
    {
        Answer::Value(_) => {}
        Answer::Failed(code) if code == jadpo_std::files::STORAGE_FULL => return unavailable(),
        _ => return unavailable(),
    }
    let origin = origin_of(ctx, steps);
    let Some(origin) = origin else {
        return defect(ctx, "an export whose origin setting has no value");
    };
    let now = ctx.now();
    let expires = now.saturating_add(exports.keep_ms);
    let link = format!("{origin}{}?token={token}", exports.link);
    let Some(ids) = pair(ctx) else {
        return defect(ctx, "an export with no randomness");
    };
    let trace = ctx.trace();
    let Ok(mut transaction) = database.begin().await else {
        return unavailable();
    };
    let written = async {
        let rows = transaction
            .query(
                EXPORT_READY,
                &[
                    id.clone(),
                    SqlValue::Text(file.clone()),
                    micros(expires),
                    SqlValue::Text(verifier(token.as_bytes())),
                ],
            )
            .await?;
        if rows.is_empty() {
            return Ok(None);
        }
        write_step(
            transaction.as_mut(),
            &steps.store,
            ids,
            step_payload(request, Step::Expire, None),
            expires,
            trace,
        )
        .await?;
        Ok::<_, StoreError>(Some(()))
    }
    .await;
    let happened = match written {
        Ok(Some(())) => true,
        Ok(None) => false,
        Err(error) => {
            let _ = transaction.rollback().await;
            return store_failed(ctx, error);
        }
    };
    if !happened {
        let _ = transaction.rollback().await;
        return nothing();
    }
    let events = [SignInEvent::PrivacyExportReady {
        request,
        user: current.person,
        download_link: link,
        expires_at: expires,
    }];
    if let Err(error) =
        sign_in_events::commit_with(ctx, executor, &steps.store, transaction, &events).await
    {
        return store_failed(ctx, error);
    }
    nothing()
}

/// The browser origin a link opens.
fn origin_of(ctx: &Ctx<'_>, steps: &PrivacySteps) -> Option<String> {
    let at = steps.origin?;
    match ctx.setting(at) {
        Some(Value::Text(origin)) => super::signed::origin_of(origin),
        _ => None,
    }
}

fn pair(ctx: &mut Ctx<'_>) -> Option<[[u8; 16]; 2]> {
    Some([ctx.new_id()?, ctx.new_id()?])
}

/// Why a download gave no file.
pub(crate) enum NotDownloaded {
    /// The link has been used, has ended or isn't one.
    Link,
    /// A store couldn't answer: nothing was used up.
    Unavailable,
}

/// The export a link's `token` names, used up as it is given: read and
/// opened first, so a store that can't answer uses nothing up, then the
/// link ended, then the file gone.
pub(crate) async fn download(
    ctx: &mut Ctx<'_>,
    steps: &PrivacySteps,
    token: &str,
) -> Result<String, NotDownloaded> {
    let Some(bytes) = token_bytes(token) else {
        return Err(NotDownloaded::Link);
    };
    let Some(exports) = steps.exports.as_ref() else {
        return Err(NotDownloaded::Unavailable);
    };
    let host = ctx.host();
    let Some(database) = host.database(&steps.store) else {
        return Err(NotDownloaded::Unavailable);
    };
    let now = ctx.now();
    let key = verifier(token.as_bytes());
    let rows = database
        .query_once(
            super::privacy::EXPORT_BY_KEY,
            &[SqlValue::Text(key.clone()), micros(now)],
        )
        .await
        .map_err(|_| NotDownloaded::Unavailable)?;
    let Some([SqlValue::Uuid(request), SqlValue::Uuid(person), file]) =
        rows.first().map(Vec::as_slice)
    else {
        return Err(NotDownloaded::Link);
    };
    let SqlValue::Text(file) = file else {
        return Err(NotDownloaded::Link);
    };
    let (request, person, file) = (*request, *person, file.clone());
    let sealed = match read_all(host, &exports.store, &file).await {
        Ok(Some(sealed)) => sealed,
        Ok(None) => return Err(NotDownloaded::Link),
        Err(_) => return Err(NotDownloaded::Unavailable),
    };
    let Some(document) = decrypt(&bytes, &sealed).and_then(|plain| String::from_utf8(plain).ok())
    else {
        return Err(NotDownloaded::Link);
    };
    let used = database
        .query_once(
            super::privacy::EXPORT_USED,
            &[SqlValue::Uuid(request), micros(now), SqlValue::Text(key)],
        )
        .await
        .map_err(|_| NotDownloaded::Unavailable)?;
    if used.is_empty() {
        return Err(NotDownloaded::Link);
    }
    // It is used now, and what is left of the file goes; a store that
    // can't delete it now has the link's end to try again at.
    let _ = delete_file(host, &exports.store, &file).await;
    crate::logging::privacy_downloaded(ctx, &format_uuid(&request), &format_uuid(&person));
    Ok(document)
}

/// The document a person's export is, as a `PersonalDataExport`: how many
/// records it holds counted from its `entities`.
pub(crate) fn document_value(document: &str) -> Value {
    let records = serde_json::from_str::<Json>(document)
        .ok()
        .and_then(|parsed| {
            parsed
                .get("entities")
                .and_then(Json::as_object)
                .map(|entities| {
                    entities
                        .values()
                        .map(|rows| rows.as_array().map_or(0, Vec::len))
                        .sum::<usize>()
                })
        })
        .unwrap_or(0);
    Value::record(
        jadpo_std::erasure::PERSONAL_DATA_EXPORT,
        vec![
            (Rc::from("json"), Value::text(document)),
            (
                Rc::from("records"),
                Value::Int(i64::try_from(records).unwrap_or(i64::MAX)),
            ),
        ],
    )
}

/// An export's link has ended, used or not: its file goes.
async fn expire(ctx: &mut Ctx<'_>, steps: &PrivacySteps, request: [u8; 16]) -> Outcome {
    let Some(exports) = steps.exports.as_ref() else {
        return nothing();
    };
    let host = ctx.host();
    let Some(database) = host.database(&steps.store) else {
        return unavailable();
    };
    let id = SqlValue::Uuid(request);
    let rows = match database
        .query_once(READ_REQUEST, std::slice::from_ref(&id))
        .await
    {
        Ok(rows) => rows,
        Err(_) => return unavailable(),
    };
    let Some(current) = rows.first().and_then(|row| current(row)) else {
        return nothing();
    };
    if let Some(file) = &current.file
        && delete_file(host, &exports.store, file).await.is_err()
    {
        return unavailable();
    }
    match database.query_once(EXPORT_ENDED, &[id]).await {
        Ok(_) => nothing(),
        Err(_) => unavailable(),
    }
}

/// The alert for a request nearing its deadline, raised once.
async fn nearing(ctx: &mut Ctx<'_>, steps: &PrivacySteps, request: [u8; 16]) -> Outcome {
    let host = ctx.host();
    let Some(database) = host.database(&steps.store) else {
        return unavailable();
    };
    let rows = match database
        .query_once(READ_REQUEST, &[SqlValue::Uuid(request)])
        .await
    {
        Ok(rows) => rows,
        Err(_) => return unavailable(),
    };
    let Some(current) = rows.first().and_then(|row| current(row)) else {
        return nothing();
    };
    if still_open(&current) {
        crate::logging::privacy_alert(
            ctx,
            "request_nearing_deadline",
            &format_uuid(&request),
            &format_uuid(&current.person),
            &current.kind,
            current.deadline,
        );
    }
    nothing()
}

/// Whether a request is still to be finished.
fn still_open(current: &Current) -> bool {
    matches!(current.state.as_str(), RECEIVED | ERASING | STALLED)
}

/// `PrivacyRequestOverdue` for a request past its deadline, emitted once.
async fn overdue(
    executor: &dyn Executor,
    ctx: &mut Ctx<'_>,
    steps: &PrivacySteps,
    request: [u8; 16],
) -> Outcome {
    let host = ctx.host();
    let Some(database) = host.database(&steps.store) else {
        return unavailable();
    };
    let rows = match database
        .query_once(READ_REQUEST, &[SqlValue::Uuid(request)])
        .await
    {
        Ok(rows) => rows,
        Err(_) => return unavailable(),
    };
    let Some(current) = rows.first().and_then(|row| current(row)) else {
        return nothing();
    };
    if !still_open(&current) {
        return nothing();
    }
    crate::logging::privacy_alert(
        ctx,
        "request_overdue",
        &format_uuid(&request),
        &format_uuid(&current.person),
        &current.kind,
        current.deadline,
    );
    let Some(kind) = PrivacyRequestKind::ALL
        .iter()
        .find(|known| known.name() == current.kind)
    else {
        return defect(ctx, "a request of a kind this build doesn't have");
    };
    let events = [SignInEvent::PrivacyRequestOverdue {
        request,
        user: current.person,
        kind: kind.name(),
        deadline: current.deadline,
    }];
    let Ok(mut transaction) = database.begin().await else {
        return unavailable();
    };
    let written =
        sign_in_events::write(ctx, executor, &steps.store, transaction.as_mut(), &events).await;
    match written {
        Ok(()) => match transaction.commit().await {
            Ok(()) => nothing(),
            Err(error) => {
                let _ = ctx.take_deferred();
                store_failed(ctx, error)
            }
        },
        Err(error) => {
            let _ = transaction.rollback().await;
            let _ = ctx.take_deferred();
            store_failed(ctx, error)
        }
    }
}

/// The records of each entity kept by law, each as how the completion
/// email lists it: the entity's name and the last end of its retention.
async fn kept_records(
    ctx: &mut Ctx<'_>,
    steps: &PrivacySteps,
    plan: &ExportPlan,
    json: crate::read::JsonCodecs<'_>,
    person: &Value,
) -> Result<Vec<(String, i64)>, Interrupt> {
    let mut out = Vec::new();
    for kept in &steps.kept {
        let Some(retention) = plan
            .kinds
            .iter()
            .find(|kind| kind.entity == kept.entity)
            .and_then(|kind| kind.retention.iter().find(|held| held.field.is_none()))
        else {
            continue;
        };
        let after = match ctx.setting(retention.setting) {
            Some(Value::Duration(after)) => *after,
            _ => 0,
        };
        let mut last: Option<i64> = None;
        for part in plan.parts.iter().filter(|part| part.entity == kept.entity) {
            let found =
                crate::read::read_detached(ctx, &part.read, json, std::slice::from_ref(person), 0)
                    .await?;
            let crate::read::Found::Value(Value::List(rows)) = found else {
                continue;
            };
            for row in rows.iter() {
                let Value::Record(row) = row else {
                    continue;
                };
                if let Some(Value::Instant(since)) = row.get(&retention.since) {
                    last = Some(last.map_or(*since, |held| held.max(*since)));
                }
            }
        }
        if let Some(since) = last {
            out.push((kept.entity.clone(), since.saturating_add(after)));
        }
    }
    Ok(out)
}

/// The person erased, once their cooling-off is over: in one transaction
/// the erasure, its events and the step that waits for what it asked of
/// others. What the email needs is read first and kept only in that step.
async fn erase(
    executor: &dyn Executor,
    ctx: &mut Ctx<'_>,
    steps: &PrivacySteps,
    request: [u8; 16],
) -> Outcome {
    let host = ctx.host();
    let Some(database) = host.database(&steps.store) else {
        return unavailable();
    };
    let id = SqlValue::Uuid(request);
    let rows = match database
        .query_once(READ_REQUEST, std::slice::from_ref(&id))
        .await
    {
        Ok(rows) => rows,
        Err(_) => return unavailable(),
    };
    let Some(state) = rows.first().and_then(|row| current(row)) else {
        return nothing();
    };
    let now = ctx.now();
    if state.state != RECEIVED
        || state.kind != PrivacyRequestKind::Erasure.name()
        || state.erases_at.is_none_or(|at| at > now)
    {
        return nothing();
    }
    let program = executor.plan();
    let Some(plan) = program
        .exports
        .iter()
        .find(|plan| plan.entity == steps.person)
    else {
        return defect(ctx, "an erasure of someone with no plan to read them");
    };
    let person = Value::uuid(&format_uuid(&state.person));
    let kept = match kept_records(ctx, steps, plan, program.json, &person).await {
        Ok(kept) => kept,
        Err(stop) => return interrupted(ctx, stop),
    };
    let started = ctx.started();
    let erasure = started.erasure();
    let Some(person_plan) = erasure.person(&steps.person).cloned() else {
        return defect(ctx, "an erasure of someone with no plan to erase them");
    };
    let keeps_files = started.files().keeps(&person_plan.own.store);
    let Some(ids) = pair(ctx) else {
        return defect(ctx, "an erasure with no randomness");
    };
    let trace = ctx.trace();
    let (trace_of, depth) = (trace, 0);
    let mut new_id = || ctx.new_id();
    let done = async {
        let mut transaction = database
            .begin_at(crate::Isolation::RepeatableRead)
            .await?;
        let held = transaction
            .query(LOCK_REQUEST, std::slice::from_ref(&id))
            .await?;
        let Some(locked) = held.first().and_then(|row| current(row)) else {
            let _ = transaction.rollback().await;
            return Ok(None);
        };
        if locked.state != RECEIVED {
            let _ = transaction.rollback().await;
            return Ok(None);
        }
        // Read before it goes: only the outbox holds it after this.
        let address = match &steps.address {
            Some(sql) => {
                let rows = transaction
                    .query(sql, &[SqlValue::Uuid(state.person)])
                    .await?;
                match rows.first().and_then(|row| row.first()) {
                    Some(SqlValue::Text(address)) => Some(address.clone()),
                    _ => None,
                }
            }
            None => None,
        };
        let applied = crate::erasure::apply_in(
            transaction.as_mut(),
            crate::erasure::Applying {
                plan: erasure,
                keeps_files,
                part: &person_plan.own,
                id: &SqlValue::Uuid(state.person),
                at: now,
                now,
                trace: trace_of,
                depth,
            },
            &mut new_id,
        )
        .await?;
        let ended = transaction
            .query(EXPORTS_OF, &[SqlValue::Uuid(state.person)])
            .await?;
        let files: Vec<String> = ended
            .iter()
            .filter_map(|row| match row.first() {
                Some(SqlValue::Text(file)) => Some(file.clone()),
                _ => None,
            })
            .collect();
        transaction
            .query(ERASING_NOW, &[id.clone(), micros(now)])
            .await?;
        let waits_for: Vec<String> = applied
            .outboxes
            .iter()
            .map(format_uuid)
            .collect();
        let extra = json!({
            "to": address,
            "locale": locked.locale,
            "kept": kept.iter().map(|(entity, until)| json!({ "entity": entity, "until": until })).collect::<Vec<_>>(),
            "waits_for": waits_for,
            "user": format_uuid(&state.person),
        });
        let completing = write_step(
            transaction.as_mut(),
            &steps.store,
            ids,
            step_payload(request, Step::Complete, Some(extra)),
            now,
            trace_of,
        )
        .await?;
        transaction.commit().await?;
        Ok::<_, StoreError>(Some((applied.deliveries, completing, files)))
    }
    .await;
    let (mut deliveries, completing, files) = match done {
        Ok(Some(applied)) => applied,
        Ok(None) => return nothing(),
        Err(error) => return store_failed(ctx, error),
    };
    if let Some(exports) = &steps.exports {
        for file in &files {
            let _ = delete_file(ctx.host(), &exports.store, file).await;
        }
    }
    ctx.erasing(&steps.person, &format_uuid(&state.person));
    // The step that waits goes last, so what it waits for has been tried.
    deliveries.push(completing);
    crate::boundary_committed(ctx, deliveries);
    nothing()
}

/// The erasure finished: every generated delivery of the person's
/// `PersonErased` has done its work. One that gave up marks the request
/// `stalled`, and this step is tried again until an operator has it
/// delivered.
async fn complete(
    executor: &dyn Executor,
    ctx: &mut Ctx<'_>,
    steps: &PrivacySteps,
    request: [u8; 16],
    payload: &Json,
) -> Outcome {
    let host = ctx.host();
    let Some(database) = host.database(&steps.store) else {
        return unavailable();
    };
    let id = SqlValue::Uuid(request);
    let waits_for: Vec<SqlValue> = payload
        .get("waits_for")
        .and_then(Json::as_array)
        .map(|list| {
            list.iter()
                .filter_map(Json::as_str)
                .filter_map(parse_uuid)
                .map(SqlValue::Uuid)
                .collect()
        })
        .unwrap_or_default();
    let outstanding = match database
        .query_once(OUTSTANDING, &[SqlValue::Array(SqlType::Uuid, waits_for)])
        .await
    {
        Ok(rows) => rows,
        Err(_) => return unavailable(),
    };
    let (waiting, parked) = match outstanding.first().map(Vec::as_slice) {
        Some([SqlValue::Int4(waiting), SqlValue::Int4(parked)]) => (*waiting, *parked),
        _ => (0, 0),
    };
    if waiting > 0 {
        if parked > 0 {
            let _ = database
                .query_once(STALLED_NOW, std::slice::from_ref(&id))
                .await;
        }
        return unavailable();
    }
    let now = ctx.now();
    let Some(user) = payload
        .get("user")
        .and_then(Json::as_str)
        .and_then(parse_uuid)
    else {
        return defect(ctx, "a completed erasure with no person");
    };
    let to = payload.get("to").and_then(Json::as_str).map(str::to_string);
    let locale = payload
        .get("locale")
        .and_then(Json::as_str)
        .unwrap_or("en_gb")
        .to_string();
    let kept = payload
        .get("kept")
        .cloned()
        .unwrap_or(Json::Array(Vec::new()));
    let Ok(mut transaction) = database.begin().await else {
        return unavailable();
    };
    let written = async {
        let rows = transaction
            .query(COMPLETED_NOW, &[id.clone(), micros(now)])
            .await?;
        Ok::<_, StoreError>(!rows.is_empty())
    }
    .await;
    match written {
        Ok(true) => {}
        Ok(false) => {
            let _ = transaction.rollback().await;
            return nothing();
        }
        Err(error) => {
            let _ = transaction.rollback().await;
            return store_failed(ctx, error);
        }
    }
    let events = [SignInEvent::PrivacyErasureCompleted {
        request,
        user,
        completed_at: now,
        to,
        locale,
        kept,
    }];
    if let Err(error) =
        sign_in_events::commit_with(ctx, executor, &steps.store, transaction, &events).await
    {
        return store_failed(ctx, error);
    }
    nothing()
}

/// One request, as `jadpo privacy requests` lists it: nothing about the
/// person but their `id`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Listed {
    pub id: String,
    pub kind: String,
    pub source: String,
    pub state: String,
    pub person: String,
    pub created_by: Option<String>,
    pub identity_check: Option<String>,
    pub requested_at: i64,
    pub deadline: i64,
    pub erases_at: Option<i64>,
    pub link_ends_at: Option<i64>,
    pub completed_at: Option<i64>,
}

impl Listed {
    /// An id as a list shows it: its last 8 hex digits.
    pub fn short_id(&self) -> String {
        let digits: String = self.id.chars().filter(|c| *c != '-').collect();
        digits
            .get(digits.len().saturating_sub(8)..)
            .unwrap_or_default()
            .to_string()
    }
}

/// How many requests there are, by state, and how many are late.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    pub states: Vec<(String, u64)>,
    /// Still to finish, and past their deadline.
    pub overdue: u64,
    /// Still to finish, and due within the alert's week.
    pub nearing: u64,
}

impl Counts {
    /// How many are in `state`.
    pub fn of(&self, state: &str) -> u64 {
        self.states
            .iter()
            .find(|(name, _)| name == state)
            .map_or(0, |(_, count)| *count)
    }

    /// How many there are.
    pub fn total(&self) -> u64 {
        self.states.iter().map(|(_, count)| count).sum()
    }

    /// How many are still to finish.
    pub fn open(&self) -> u64 {
        self.of(RECEIVED) + self.of(ERASING) + self.of(STALLED)
    }

    /// How many are held up by a provider that gave up.
    pub fn stalled(&self) -> u64 {
        self.of(STALLED)
    }
}

/// A page of the requests in `states`, or all of them when it is empty.
pub async fn list(
    database: &dyn crate::Database,
    states: &[String],
    latest_first: bool,
    limit: i64,
    offset: i64,
) -> Result<Vec<Listed>, StoreError> {
    let rows = database
        .query_once(
            if latest_first {
                LIST_NEWEST
            } else {
                LIST_BY_DEADLINE
            },
            &[
                SqlValue::Array(
                    SqlType::Text,
                    states.iter().cloned().map(SqlValue::Text).collect(),
                ),
                SqlValue::Int8(limit),
                SqlValue::Int8(offset),
            ],
        )
        .await?;
    let text = |value: &SqlValue| match value {
        SqlValue::Text(text) => Some(text.clone()),
        _ => None,
    };
    Ok(rows
        .iter()
        .filter_map(|row| match row.as_slice() {
            [
                SqlValue::Uuid(id),
                SqlValue::Text(kind),
                SqlValue::Text(source),
                SqlValue::Text(state),
                SqlValue::Uuid(person),
                created_by,
                identity_check,
                requested_at,
                deadline,
                erases_at,
                link_ends_at,
                completed_at,
            ] => Some(Listed {
                id: format_uuid(id),
                kind: kind.clone(),
                source: source.clone(),
                state: state.clone(),
                person: format_uuid(person),
                created_by: text(created_by),
                identity_check: text(identity_check),
                requested_at: millis(requested_at)?,
                deadline: millis(deadline)?,
                erases_at: millis(erases_at),
                link_ends_at: millis(link_ends_at),
                completed_at: millis(completed_at),
            }),
            _ => None,
        })
        .collect())
}

/// How many requests there are, by state, and how many are late at `now`.
pub async fn counts(database: &dyn crate::Database, now: i64) -> Result<Counts, StoreError> {
    let rows = database
        .query_once(
            COUNTS,
            &[
                micros(now),
                micros(now.saturating_add(PRIVACY_ALERT_BEFORE_MS)),
            ],
        )
        .await?;
    let mut out = Counts::default();
    let count = |value: &SqlValue| match value {
        SqlValue::Int4(n) => u64::try_from(*n).unwrap_or(0),
        _ => 0,
    };
    for row in &rows {
        if let [SqlValue::Text(state), all, overdue, nearing] = row.as_slice() {
            out.states.push((state.clone(), count(all)));
            out.overdue += count(overdue);
            out.nearing += count(nearing);
        }
    }
    Ok(out)
}

/// What an email of a data request is written from, read as it is sent.
pub(crate) struct Mail {
    /// The address it goes to.
    pub address: String,
    /// The language it is in, as a `Locale` value.
    pub locale: Value,
    /// Its `Locale` tag: `en-GB`.
    pub tag: &'static str,
    /// Its place among `Locale`'s variants.
    pub ordinal: u32,
    /// `kept`, for the completion email: each entity and the end of its
    /// retention, in milliseconds.
    pub kept: Vec<(String, i64)>,
}

/// The value of a language by `Locale`'s name, with its tag and place.
fn locale_of(name: &str) -> Option<(Value, &'static str, u32)> {
    let ordinal = jadpo_std::locale_ordinal(name)?;
    let tag = jadpo_std::LOCALES
        .get(ordinal as usize)
        .map_or("en-GB", |(_, tag)| *tag);
    Some((jadpo_value::locale::value(ordinal), tag, ordinal))
}

/// Who a privacy email goes to and in what language, read now: the person's
/// address by their `id` and the request's language, or, once they have
/// been erased, what the completion's step kept. `None` when nothing can
/// say where it goes, and nothing is sent.
pub(crate) async fn mail_for(
    ctx: &Ctx<'_>,
    steps: &PrivacySteps,
    value: &Value,
    outbox_row: Option<[u8; 16]>,
) -> Result<Option<Mail>, Outcome> {
    let Value::Record(record) = value else {
        return Ok(None);
    };
    let host = ctx.host();
    let Some(database) = host.database(&steps.store) else {
        return Err(unavailable());
    };
    if let Some(row) = outbox_row {
        let rows = database
            .query_once(PAYLOAD, &[SqlValue::Uuid(row)])
            .await
            .map_err(|_| unavailable())?;
        let Some([SqlValue::Text(payload)]) = rows.first().map(Vec::as_slice) else {
            return Ok(None);
        };
        let Ok(payload) = serde_json::from_str::<Json>(payload) else {
            return Ok(None);
        };
        let Some(address) = payload.get("to").and_then(Json::as_str) else {
            return Ok(None);
        };
        let Some((locale, tag, ordinal)) = payload
            .get("locale")
            .and_then(Json::as_str)
            .and_then(locale_of)
        else {
            return Ok(None);
        };
        let kept = payload
            .get("kept")
            .and_then(Json::as_array)
            .map(|list| {
                list.iter()
                    .filter_map(|item| {
                        Some((
                            item.get("entity")?.as_str()?.to_string(),
                            item.get("until")?.as_i64()?,
                        ))
                    })
                    .collect()
            })
            .unwrap_or_default();
        return Ok(Some(Mail {
            address: address.to_string(),
            locale,
            tag,
            ordinal,
            kept,
        }));
    }
    let id_of = |name: &str| match record.get(name) {
        Some(Value::Uuid(id)) => parse_uuid(id),
        _ => None,
    };
    let (Some(request), Some(user)) = (id_of("request_id"), id_of("user_id")) else {
        return Ok(None);
    };
    let Some(sql) = &steps.address else {
        return Ok(None);
    };
    let rows = database
        .query_once(sql, &[SqlValue::Uuid(user)])
        .await
        .map_err(|_| unavailable())?;
    let Some(SqlValue::Text(address)) = rows.first().and_then(|row| row.first()) else {
        return Ok(None);
    };
    let address = address.clone();
    let rows = database
        .query_once(LOCALE_OF, &[SqlValue::Uuid(request)])
        .await
        .map_err(|_| unavailable())?;
    let name = match rows.first().and_then(|row| row.first()) {
        Some(SqlValue::Text(name)) => name.clone(),
        _ => "en_gb".to_string(),
    };
    let Some((locale, tag, ordinal)) = locale_of(&name) else {
        return Ok(None);
    };
    Ok(Some(Mail {
        address,
        locale,
        tag,
        ordinal,
        kept: Vec::new(),
    }))
}

/// The address gone from the completion email's row, once it has gone out.
pub(crate) async fn forget_address(host: &dyn Host, store: &str, outbox_row: [u8; 16]) {
    if let Some(database) = host.database(store) {
        let _ = database
            .query_once(STEP_DONE, &[SqlValue::Uuid(outbox_row)])
            .await;
    }
}

/// How long an export's link lasts, as its email says it.
pub(crate) fn keep_ms(steps: &PrivacySteps) -> Option<i64> {
    steps
        .exports
        .as_ref()
        .map(|exports: &Exports| exports.keep_ms)
}

/// A date as an email says it, in its language.
pub(crate) fn date_words(at: i64, tag: &str) -> Result<String, jadpo_value::Fault> {
    const DAY_MS: i64 = 86_400_000;
    let date = i32::try_from(at.div_euclid(DAY_MS)).map_err(|_| jadpo_value::Fault::Shape)?;
    let moment = jadpo_value::present::Moment {
        date,
        clock: 0,
        zone: None,
    };
    let spec = jadpo_std::TimeFormatSpec::Builtin(
        jadpo_std::BuiltinFormat::from_name("date_long").ok_or(jadpo_value::Fault::Shape)?,
    );
    jadpo_value::present::format(moment, tag, &spec)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_export_opens_only_for_its_token() {
        let token = [7u8; 32];
        let sealed = encrypt(&token, b"{\"entities\":{}}");
        assert_eq!(
            decrypt(&token, &sealed).as_deref(),
            Some(&b"{\"entities\":{}}"[..])
        );
        assert_eq!(decrypt(&[8u8; 32], &sealed), None);
        let mut altered = sealed.clone();
        if let Some(last) = altered.last_mut() {
            *last ^= 1;
        }
        assert_eq!(decrypt(&token, &altered), None);
        assert_eq!(decrypt(&token, b"short"), None);
    }

    #[test]
    fn an_export_is_read_back_whole_and_gone_once_deleted() {
        use crate::capability::{Capability, Faked};
        let host = crate::MemoryHost::new();
        let files = Rc::new(crate::files::MemoryFiles::new());
        let faked = Faked::new(&host).with("vault", Rc::clone(&files) as Rc<dyn Capability>);
        let file = export_key("0190a2b3-c4d5-7e6f-8a9b-0c1d2e3f4a5b");
        // Longer than a window, so reading it takes more than one.
        let bytes = vec![7u8; WINDOW + 10];
        let input = Value::record(
            "FilePut",
            vec![
                (Rc::from("key"), Value::text(&file)),
                (Rc::from("type"), Value::text("application/octet-stream")),
                (Rc::from("bytes"), Value::Bytes(Rc::from(bytes.clone()))),
            ],
        );
        let put = Call::new("vault", PUT, input, Repeat::Keyed);
        assert!(matches!(
            crate::block_on(files.call(&put)).unwrap(),
            Answer::Value(_)
        ));
        assert_eq!(
            crate::block_on(read_all(&faked, "vault", &file))
                .unwrap()
                .unwrap(),
            Some(bytes)
        );
        crate::block_on(delete_file(&faked, "vault", &file))
            .unwrap()
            .unwrap();
        assert_eq!(
            crate::block_on(read_all(&faked, "vault", &file))
                .unwrap()
                .unwrap(),
            None
        );
        // Deleting it again changes nothing.
        crate::block_on(delete_file(&faked, "vault", &file))
            .unwrap()
            .unwrap();
    }

    #[test]
    fn a_steps_payload_says_what_it_is_and_carries_what_it_needs() {
        let payload = step_payload(
            [1; 16],
            Step::Complete,
            Some(json!({ "to": "ada@example.com" })),
        );
        let parsed: Json = serde_json::from_str(&payload).expect("JSON");
        assert_eq!(parsed["step"], "complete");
        assert_eq!(parsed["to"], "ada@example.com");
        assert_eq!(
            parse_uuid(parsed["request"].as_str().expect("id")),
            Some([1; 16])
        );
    }
}
