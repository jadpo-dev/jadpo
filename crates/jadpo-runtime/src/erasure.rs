//! Erasure past the person's own transaction (area 15, "What erasure
//! reaches" and "What erasure doesn't rewrite"; chunk P4c).
//!
//! - **`PersonErased`** ([`emit`]): an erasure's statement returns a row for
//!   each person it erased, and each is emitted in its transaction, holding
//!   the person entity's name, their `id` and the time, and nothing else.
//! - **Other stores** ([`apply_delivery`]): the generated subscriber `erasure
//!   in <store>` runs that store's statement after the commit, in a
//!   transaction of its own there, with the events of whoever else it
//!   erased and of each file it let go of. It is safe to repeat, since it
//!   finds nothing a second time, so it needs no handled record: a store
//!   that can't do it yet leaves the delivery `pending`, durably, in the
//!   person's store, and one that keeps failing leaves it `failed` for an
//!   operator (area 12, "What an operator sees").
//! - **The log** ([`log_write`]): the generated subscriber `erasure log`
//!   keeps an entry after the commit, in a database that holds no person's
//!   records or in a store of files, the first time written kept.
//! - **The replay** ([`replay`]): every start, before the app serves
//!   anything, reads the log a page at a time and asks each store which of
//!   those people it still holds anything of, then runs the same statement
//!   for each. A restored backup is erased, whichever store it restored,
//!   and a run that finds nothing changes nothing. What the replay emits is
//!   left for the sweep, as a dropped delivery is.

use std::rc::Rc;

use jadpo_std::erasure::{
    ErasureLog, ErasurePlan, LogEntry, PERSON_ERASED, PERSON_ERASED_KEY, REPLAY_PAGE, StoreErasure,
    log_key, log_key_parts,
};
use jadpo_std::files::{LIST, PUT, READ};
use jadpo_std::subscriber::Repeat;
use jadpo_std::write::{WriteParam, WriteSource};
use jadpo_value::Value;

use crate::capability::{Answer, Call};
use crate::outbox::{self, Delivery, Sealed, Trace, micros};
use crate::read::uuid_text;
use crate::{Ctx, Event, Host, Isolation, Outcome, Row, SqlType, SqlValue, StoreError};

/// Keeps a person's entry, the first time written kept.
pub const INSERT_LOG: &str = "INSERT INTO \"jadpo_erasure_log\" (\"entity\", \"person_id\", \"erased_at\") VALUES ($1, $2, $3) ON CONFLICT (\"entity\", \"person_id\") DO NOTHING";

/// A page of the log after `($1, $2)`, in order, at most `$3`.
pub const LOG_PAGE: &str = "SELECT \"entity\", \"person_id\", \"erased_at\" FROM \"jadpo_erasure_log\" WHERE (\"entity\", \"person_id\") > ($1, $2) ORDER BY \"entity\", \"person_id\" LIMIT $3";

/// `PersonErased` for the person of the entity called `entity`, as it is
/// delivered and kept: its value and its payload, the runtime's own JSON.
fn event(entity: &str, id: &str, at: i64, subscribers: Vec<String>) -> Event {
    Event {
        key: PERSON_ERASED_KEY.to_string(),
        value: value(entity, id, at),
        payload: serde_json::json!({ "entity": entity, "id": id, "erased_at": at }).to_string(),
        subscribers,
    }
}

fn value(entity: &str, id: &str, at: i64) -> Value {
    Value::record(
        PERSON_ERASED,
        vec![
            (Rc::from("entity"), Value::text(entity)),
            (Rc::from("id"), Value::uuid(id)),
            (Rc::from("erased_at"), Value::Instant(at)),
        ],
    )
}

/// The value a `PersonErased` event's payload holds.
pub(crate) fn read_payload(payload: &str) -> Result<Value, String> {
    let not = || "not the payload of `PersonErased`".to_string();
    let object: serde_json::Value = serde_json::from_str(payload).map_err(|_| not())?;
    let text = |name: &str| object.get(name).and_then(serde_json::Value::as_str);
    match (
        text("entity"),
        text("id"),
        object.get("erased_at").and_then(serde_json::Value::as_i64),
    ) {
        (Some(entity), Some(id), Some(at)) => Ok(value(entity, id, at)),
        _ => Err(not()),
    }
}

/// What a `PersonErased` value holds: the person entity's name, their `id`
/// and when.
fn parts(event: &Value) -> Option<(String, Value, i64)> {
    let Value::Record(record) = event else {
        return None;
    };
    match (
        record.get("entity"),
        record.get("id"),
        record.get("erased_at"),
    ) {
        (Some(Value::Text(entity)), Some(id @ Value::Uuid(_)), Some(Value::Instant(at))) => {
            Some((entity.to_string(), id.clone(), *at))
        }
        _ => None,
    }
}

/// The person each row of an erasure's statement names: their entity's name
/// and their `id` in its text form.
fn erased_people<'p>(people: &'p [(i32, String)], rows: &[Row]) -> Vec<(&'p str, String)> {
    rows.iter()
        .filter_map(|row| match (row.first(), row.get(1)) {
            (Some(SqlValue::Int4(place)), Some(SqlValue::Uuid(id))) => people
                .iter()
                .find(|(at, _)| at == place)
                .map(|(_, name)| (name.as_str(), uuid_text(id))),
            _ => None,
        })
        .collect()
}

/// Queues `PersonErased` for each person `rows` names, in the running
/// action's outbox, so it commits with the erasure. With nothing listening,
/// as in a project that names no log and has no subscriber, it isn't
/// emitted.
pub(crate) fn emit(
    ctx: &mut Ctx<'_>,
    people: &[(i32, String)],
    rows: &[Row],
    now: i64,
) -> Result<(), &'static str> {
    if people.is_empty() {
        return Ok(());
    }
    let started = ctx.started();
    for (entity, id) in erased_people(people, rows) {
        ctx.erasing(entity, &id);
        let subscribers = started
            .erasure()
            .person(entity)
            .map(|person| person.subscribers.clone())
            .unwrap_or_default();
        if subscribers.is_empty() {
            continue;
        }
        ctx.emit(event(entity, &id, now, subscribers))
            .map_err(|refused| match refused {
                crate::EmitRefused::OutsideAction => "an erasure outside an action",
                crate::EmitRefused::TooMany => "more than 1,000 people erased by one action",
                crate::EmitRefused::TooLarge => "a `PersonErased` event past 64 KB",
            })?;
    }
    Ok(())
}

/// One store's share of an erasure, about to run.
struct Applying<'a> {
    plan: &'a ErasurePlan,
    /// Whether the store keeps a record of files, whose commit releases them.
    keeps_files: bool,
    part: &'a StoreErasure,
    /// The person's `id`.
    id: &'a SqlValue,
    /// When they were erased.
    at: i64,
    /// The host's time.
    now: i64,
    trace: Option<Trace>,
    depth: u32,
}

/// Runs one store's share of an erasure, in a transaction of its own at
/// repeatable read as an erasure's action runs: with the `PersonErased` of
/// anyone else it erased and the event of each file it let go of, in that
/// store's outbox. What it leaves for the runner.
async fn apply(
    host: &dyn Host,
    applying: Applying<'_>,
    new_id: &mut dyn FnMut() -> Option<[u8; 16]>,
) -> Result<Vec<Delivery>, StoreError> {
    let Applying {
        plan,
        keeps_files,
        part,
        id,
        at,
        now,
        trace,
        depth,
    } = applying;
    let database = host.database(&part.store).ok_or(StoreError::Unavailable)?;
    let params = bound(&part.params, id, at)?;
    let mut transaction = database.begin_at(Isolation::RepeatableRead).await?;
    let done = async {
        let rows = transaction.query(&part.sql, &params).await?;
        let mut events: Vec<Event> = erased_people(&part.people, &rows)
            .into_iter()
            .filter_map(|(entity, id)| {
                let person = plan.person(entity)?;
                (!person.subscribers.is_empty())
                    .then(|| event(entity, &id, at, person.subscribers.clone()))
            })
            .collect();
        if keeps_files {
            events.extend(crate::files::released(&mut *transaction, now).await?);
        }
        let mut sealed = Sealed {
            outbox: Vec::with_capacity(events.len()),
            deliveries: Vec::new(),
        };
        let fresh = |new_id: &mut dyn FnMut() -> Option<[u8; 16]>| {
            new_id().ok_or_else(|| StoreError::Fault("no random bytes for an identity".into()))
        };
        for event in events {
            let kept = fresh(new_id)?;
            for subscriber in &event.subscribers {
                sealed.deliveries.push(Delivery {
                    store: part.store.clone(),
                    id: fresh(new_id)?,
                    outbox: kept,
                    subscriber: subscriber.clone(),
                    event: event.key.clone(),
                    value: event.value.clone(),
                    emitted_at: now,
                    trace,
                    depth,
                });
            }
            sealed.outbox.push((kept, event.key, event.payload));
        }
        outbox::write(&mut *transaction, &sealed, now, trace, depth).await?;
        Ok::<_, StoreError>(sealed.deliveries)
    }
    .await;
    match done {
        Ok(deliveries) => {
            transaction.commit().await?;
            Ok(deliveries)
        }
        Err(error) => {
            let _ = transaction.rollback().await;
            Err(error)
        }
    }
}

/// A statement's parameters: the person's `id`, and the time they were
/// erased wherever a generated field or the time noted takes it, so a
/// repeat or a replay notes the erasure's own.
fn bound(params: &[WriteParam], id: &SqlValue, at: i64) -> Result<Vec<SqlValue>, StoreError> {
    params
        .iter()
        .map(|param| match &param.source {
            WriteSource::Input(0) => Ok(id.clone()),
            WriteSource::Now => Ok(micros(at)),
            WriteSource::Stored(value) => Ok(value.clone()),
            _ => Err(StoreError::Fault(
                "an erasure's parameter it can't bind".into(),
            )),
        })
        .collect()
}

/// The generated subscriber `erasure in <store>`: the erasure `delivery`
/// names, applied in `store`, how it ended as any subscriber's outcome is
/// settled. A store that can't be reached, or a concurrent change there, is
/// worth trying again, and a person entity this build doesn't have has
/// nothing to do.
pub(crate) async fn apply_delivery(ctx: &mut Ctx<'_>, store: &str, delivery: &Delivery) -> Outcome {
    let Some((entity, id, at)) = parts(&delivery.value) else {
        return Outcome::Fault(ctx.fault(
            crate::FaultKind::Defect("a person's erasure with no person".into()),
            crate::Frame::ENTRY,
        ));
    };
    let started = ctx.started();
    let plan = started.erasure();
    let Some(part) = plan
        .person(&entity)
        .and_then(|person| person.elsewhere(store))
    else {
        return Outcome::Value(Value::Unit);
    };
    let Ok(id) = crate::to_sql(&id, SqlType::Uuid) else {
        return Outcome::Fault(ctx.fault(
            crate::FaultKind::Defect("a person's erasure whose `id` isn't one".into()),
            crate::Frame::ENTRY,
        ));
    };
    let host = ctx.host();
    let now = host.now_ms().unwrap_or_else(|| ctx.now());
    let keeps_files = started.files().keeps(store);
    let (trace, depth) = (delivery.trace, delivery.depth.saturating_add(1));
    let mut new_id = || ctx.new_id();
    let applying = Applying {
        plan,
        keeps_files,
        part,
        id: &id,
        at,
        now,
        trace,
        depth,
    };
    match apply(host, applying, &mut new_id).await {
        Ok(deliveries) => {
            crate::boundary_committed(ctx, deliveries);
            Outcome::Value(Value::Unit)
        }
        Err(StoreError::Fault(why)) => {
            Outcome::Fault(ctx.fault(crate::FaultKind::Store(why), crate::Frame::ENTRY))
        }
        Err(_) => Outcome::Failed(Box::new(crate::unavailable())),
    }
}

/// What a person's entry in the log takes.
pub(crate) enum LogWrite {
    /// [`INSERT_LOG`] in this store, with these parameters.
    Statement(String, Vec<SqlValue>),
    /// A file, which a repeat writes again the same.
    File(Box<Call>),
}

/// The entry for the person `event` names: kept by their entity's born
/// name, so a rename finds it.
pub(crate) fn log_write(
    ctx: &Ctx<'_>,
    log: &ErasureLog,
    event: &Value,
) -> Result<LogWrite, &'static str> {
    let (entity, id, at) = parts(event).ok_or("a person's erasure with no person")?;
    let started = ctx.started();
    let born = started
        .erasure()
        .person(&entity)
        .map_or(entity.as_str(), |person| person.born.as_str())
        .to_string();
    let id = crate::to_sql(&id, SqlType::Uuid).map_err(|_| "a person's `id` that isn't one")?;
    let SqlValue::Uuid(bytes) = &id else {
        return Err("a person's `id` that isn't one");
    };
    Ok(match log {
        ErasureLog::Database { store } => LogWrite::Statement(
            store.clone(),
            vec![SqlValue::Text(born), id.clone(), micros(at)],
        ),
        ErasureLog::Files { connection } => {
            let id = uuid_text(bytes);
            let entry = LogEntry {
                entity: born.clone(),
                id: id.clone(),
                erased_at: at,
            };
            let bytes = serde_json::to_vec(&entry).map_err(|_| "an entry that can't be written")?;
            let input = Value::record(
                "FilePut",
                vec![
                    (Rc::from("key"), Value::text(&log_key(&born, &id))),
                    (Rc::from("type"), Value::text("application/json")),
                    (Rc::from("bytes"), Value::Bytes(Rc::from(bytes))),
                ],
            );
            LogWrite::File(Box::new(Call::new(connection, PUT, input, Repeat::Keyed)))
        }
    })
}

/// What a replay did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Replayed {
    /// How many entries the log holds.
    pub logged: u64,
    /// How many times it found someone logged still there and erased them
    /// again, a person and each other store counted apart.
    pub erased: u64,
}

/// Why a replay couldn't finish: the start waits, or stops, naming where.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplayStopped {
    /// The connection it couldn't work in.
    pub store: String,
    /// Whether trying again may help: a store that can't be reached, or a
    /// concurrent change.
    pub waiting: bool,
    pub why: String,
}

fn stopped(store: &str, error: &StoreError) -> ReplayStopped {
    ReplayStopped {
        store: store.to_string(),
        waiting: matches!(
            error,
            StoreError::Unavailable | StoreError::ConcurrentChange
        ),
        why: crate::deployable::store_error(error),
    }
}

/// One entry as a page gives it. A file's time is read only if it is needed.
struct Entry {
    born: String,
    id: [u8; 16],
    at: Option<i64>,
}

/// Replays the log `plan` names on `host`: every person logged who is still
/// there erased again, in each store `host` has a database for, before the
/// app serves anything (area 15, "What erasure doesn't rewrite"). A person
/// entity no longer declared has nothing to erase. What it emits is left for
/// the sweep. Asked of a plan with no log it does nothing.
pub async fn replay(
    host: &dyn Host,
    plan: &ErasurePlan,
    files: &jadpo_std::files::FilesPlan,
) -> Result<Replayed, ReplayStopped> {
    let mut out = Replayed::default();
    let Some(log) = &plan.log else {
        return Ok(out);
    };
    let page = REPLAY_PAGE as usize;
    match log {
        ErasureLog::Database { store } => {
            let database = host.database(store).ok_or_else(|| ReplayStopped {
                store: store.clone(),
                waiting: false,
                why: "it has no driver".to_string(),
            })?;
            let mut after = (String::new(), [0u8; 16]);
            loop {
                let rows = database
                    .query_once(
                        LOG_PAGE,
                        &[
                            SqlValue::Text(after.0.clone()),
                            SqlValue::Uuid(after.1),
                            SqlValue::Int4(REPLAY_PAGE as i32),
                        ],
                    )
                    .await
                    .map_err(|error| stopped(store, &error))?;
                let entries: Vec<Entry> = rows.iter().filter_map(database_entry).collect();
                let Some(last) = entries.last() else {
                    return Ok(out);
                };
                after = (last.born.clone(), last.id);
                let full = rows.len() >= page;
                replay_page(host, plan, files, log, &entries, &mut out).await?;
                if !full {
                    return Ok(out);
                }
            }
        }
        ErasureLog::Files { connection } => {
            let keys = listed(host, connection).await?;
            for chunk in keys.chunks(page) {
                let entries: Vec<Entry> = chunk
                    .iter()
                    .filter_map(|key| {
                        let (born, id) = log_key_parts(key)?;
                        Some(Entry {
                            born: born.to_string(),
                            id: crate::http::magic_links::parse_uuid(id)?,
                            at: None,
                        })
                    })
                    .collect();
                replay_page(host, plan, files, log, &entries, &mut out).await?;
            }
            Ok(out)
        }
    }
}

fn database_entry(row: &Row) -> Option<Entry> {
    match (row.first(), row.get(1), row.get(2)) {
        (
            Some(SqlValue::Text(born)),
            Some(SqlValue::Uuid(id)),
            Some(SqlValue::Timestamp(micros)),
        ) => Some(Entry {
            born: born.clone(),
            id: *id,
            at: Some(micros.div_euclid(1000)),
        }),
        _ => None,
    }
}

/// Every key of the log the store of files `connection` keeps.
async fn listed(host: &dyn Host, connection: &str) -> Result<Vec<String>, ReplayStopped> {
    let driver = host.capability(connection).ok_or_else(|| ReplayStopped {
        store: connection.to_string(),
        waiting: false,
        why: "it has no driver".to_string(),
    })?;
    let call = Call::new(
        connection,
        LIST,
        Value::record("FileList", Vec::new()),
        Repeat::Safe,
    );
    match driver.call(&call).await {
        Answer::Value(Value::List(keys)) => Ok(keys
            .iter()
            .filter_map(|key| match key {
                Value::Text(text) => Some(text.to_string()),
                _ => None,
            })
            .collect()),
        answer => Err(ReplayStopped {
            store: connection.to_string(),
            waiting: matches!(answer, Answer::Unavailable),
            why: "its files couldn't be listed".to_string(),
        }),
    }
}

/// When the person of `entry` was erased: what a page gave, or what their
/// file holds.
async fn time_of(host: &dyn Host, log: &ErasureLog, entry: &Entry) -> Result<i64, ReplayStopped> {
    if let Some(at) = entry.at {
        return Ok(at);
    }
    let connection = log.connection();
    let unread = |why: &str| ReplayStopped {
        store: connection.to_string(),
        waiting: false,
        why: why.to_string(),
    };
    let driver = host
        .capability(connection)
        .ok_or_else(|| unread("it has no driver"))?;
    let input = Value::record(
        "FileRead",
        vec![
            (
                Rc::from("key"),
                Value::text(&log_key(&entry.born, &uuid_text(&entry.id))),
            ),
            (Rc::from("from"), Value::Int(0)),
            (Rc::from("length"), Value::Int(4096)),
        ],
    );
    let call = Call::new(connection, READ, input, Repeat::Safe);
    let bytes = match driver.call(&call).await {
        Answer::Value(Value::Bytes(bytes)) => bytes,
        Answer::Unavailable => {
            return Err(ReplayStopped {
                waiting: true,
                ..unread("an entry couldn't be read")
            });
        }
        _ => return Err(unread("an entry couldn't be read")),
    };
    let read: LogEntry =
        serde_json::from_slice(&bytes).map_err(|_| unread("an entry doesn't read"))?;
    Ok(read.erased_at)
}

/// Asks each store which of `entries` it still holds anything of, then
/// erases each of those there.
async fn replay_page(
    host: &dyn Host,
    plan: &ErasurePlan,
    files: &jadpo_std::files::FilesPlan,
    log: &ErasureLog,
    entries: &[Entry],
    out: &mut Replayed,
) -> Result<(), ReplayStopped> {
    out.logged += entries.len() as u64;
    let now = host.now_ms().unwrap_or_default();
    let mut new_id = || {
        let mut id = [0u8; 16];
        host.random(&mut id).then_some(id)
    };
    for person in &plan.people {
        let ids: Vec<&Entry> = entries
            .iter()
            .filter(|entry| entry.born == person.born)
            .collect();
        if ids.is_empty() {
            continue;
        }
        for part in std::iter::once(&person.own).chain(&person.elsewhere) {
            // A deployable that holds no connection to it replays nothing
            // there; the one that does replays it as it starts.
            let Some(database) = host.database(&part.store) else {
                continue;
            };
            let asked = SqlValue::Array(
                SqlType::Uuid,
                ids.iter().map(|entry| SqlValue::Uuid(entry.id)).collect(),
            );
            let rows = database
                .query_once(&part.present, &[asked])
                .await
                .map_err(|error| stopped(&part.store, &error))?;
            for row in rows {
                let Some(SqlValue::Uuid(id)) = row.first() else {
                    continue;
                };
                let Some(entry) = ids.iter().find(|entry| entry.id == *id) else {
                    continue;
                };
                let at = time_of(host, log, entry).await?;
                let applying = Applying {
                    plan,
                    keeps_files: files.keeps(&part.store),
                    part,
                    id: &SqlValue::Uuid(*id),
                    at,
                    now,
                    trace: None,
                    depth: 0,
                };
                apply(host, applying, &mut new_id)
                    .await
                    .map_err(|error| stopped(&part.store, &error))?;
                out.erased += 1;
            }
        }
    }
    Ok(())
}
