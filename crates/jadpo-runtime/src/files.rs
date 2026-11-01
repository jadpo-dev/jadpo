//! The `files` capability (area 16, "Files"; chunk 6.4b), on the one
//! outside-call path ([`crate::effects::call`], [`crate::outside`]), sent
//! once since a store has no `retry:`: a store's `put` and a ranged
//! `read` of a stored file, each an outside call through the connection
//! that fills `files`, so each is refused inside an open transaction,
//! logged without its bytes, and recorded and replayed with the other
//! effects.
//!
//! - [`put`]: `store.put(bytes, type)` in an action: the bytes kept under
//!   a key the runtime makes from the invocation's randomness
//!   ([`jadpo_std::files::key_for`]), sent as a keyed call (a repeat
//!   writes the same immutable object again), giving the `File`; a store
//!   with no room is `storage_full`, the condition the call's `attempt`
//!   maps.
//! - [`read`]: a range of a stored file's bytes, what a batch run reads a
//!   page of rows from before its page's transaction opens (chunk 6.4b),
//!   and [`read_whole`], `store.read(file)`, which reads at most
//!   [`WHOLE_READ_LIMIT`] bytes into memory.
//! - [`put_piece`], [`join`] and [`discard`]: an export's pieces, each a
//!   page's rows put once the page's transaction has closed, joined into
//!   the one file once every page is written, or dropped when the export
//!   is cancelled or fails (chunk 6.4b2).
//! - Deleting a file's bytes (area 16, "Deleting and erasing"; chunk B3):
//!   the store whose records hold a store's files keeps `jadpo_files`, a
//!   count of each file's holders that triggers keep as rows change. A
//!   put records its file there at none before sending ([`KEEP_PUT`]);
//!   the end of a commit that left files held by none marks them as being
//!   deleted and leaves a [`FILE_RELEASED`] event for each ([`released`]);
//!   the tick does the same for bytes held by none for a day, and for any
//!   a commit left no event for ([`sweep`]). The event's generated
//!   subscriber sends the capability's [`DELETE`] and records the bytes
//!   gone ([`GONE`]); a file being deleted or gone can't be held again.
//! - [`MemoryFiles`]: the in-memory store tests run on (area 13, "Provider
//!   calls and capabilities in tests"), which can be told to have no room
//!   and to read a few bytes at a time.
//!
//! A `File` is a record of its store, key, name, size and type
//! ([`jadpo_std::files::FILE_FIELDS`]), which only the runtime builds.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;

use jadpo_std::RuleKey;
use jadpo_std::files::{
    DELETE, DISCARD, FILE, FILE_RELEASED, FilesPlan, JOIN, NOT_FOUND, PUT, READ, STORAGE_FULL,
    UNHELD_KEPT_MS, WHOLE_READ_LIMIT, key_for, piece_key,
};
use jadpo_value::Value;

use crate::capability::{Answer, Call, Capability, Repeat, Source};
use crate::effects::{Unanswered, call};
use crate::outbox::{Delivery, Sealed, micros};
use crate::{
    Ctx, Event, Executor, Failure, FaultKind, Isolation, LocalFuture, SqlValue, StoreError,
    Transaction,
};

/// A put's file recorded as held by none, at `$3`, before its bytes are
/// sent, in the store keeping the record of `$1`'s files (chunk B3): a
/// file held already is left as it is, and one whose bytes are gone is
/// kept again, since they are about to be.
pub const KEEP_PUT: &str = "INSERT INTO \"jadpo_files\" (\"store\", \"key\", \"holders\", \"since\") VALUES ($1, $2, 0, $3) ON CONFLICT (\"store\", \"key\") DO UPDATE SET \"holders\" = 0, \"since\" = $3 WHERE \"jadpo_files\".\"holders\" = -2";

/// What a commit leaves, in its own transaction as it ends: each file its
/// changes left held by none, marked as being deleted at `$1`, which no
/// record may hold from then on.
pub const RELEASED: &str = "UPDATE \"jadpo_files\" SET \"holders\" = -1, \"since\" = $1 WHERE \"holders\" = 0 AND \"since\" IS NULL RETURNING \"store\", \"key\"";

/// At most a hundred files due their bytes' deletion at `$1`, marked as
/// being deleted: held by none since `$2` or before (a day ago), or
/// released by a commit that left no event, or being deleted since then
/// with no store confirming it.
pub const DUE: &str = "WITH \"due\" AS (SELECT \"store\", \"key\" FROM \"jadpo_files\" WHERE (\"holders\" = 0 AND (\"since\" IS NULL OR \"since\" <= $2)) OR (\"holders\" = -1 AND \"since\" <= $2) ORDER BY \"store\", \"key\" LIMIT 100 FOR UPDATE SKIP LOCKED) UPDATE \"jadpo_files\" AS \"f\" SET \"holders\" = -1, \"since\" = $1 FROM \"due\" WHERE \"f\".\"store\" = \"due\".\"store\" AND \"f\".\"key\" = \"due\".\"key\" RETURNING \"f\".\"store\", \"f\".\"key\"";

/// A file's bytes recorded gone once its store confirmed deleting them.
pub const GONE: &str = "UPDATE \"jadpo_files\" SET \"holders\" = -2 WHERE \"store\" = $1 AND \"key\" = $2 AND \"holders\" = -1";

/// The most bytes one read asks a store for: a page of rows is read in
/// windows of this size until it holds its rows (chunk 6.4b).
pub const WINDOW: u64 = 1 << 20;

/// What a store's operation gives the site that called it.
#[derive(Debug)]
pub enum Done {
    Value(Value),
    /// A condition the call's `attempt` maps: `storage_full`.
    Broke(RuleKey),
    /// What the runtime raises in the call's place: `unavailable`,
    /// `misconfigured`.
    Failed(Box<Failure>),
    Fault(FaultKind),
}

/// A stored file's parts the runtime reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stored {
    /// The connection holding its bytes.
    pub store: String,
    pub key: String,
    pub size: u64,
}

/// A `File` (area 16): what an entity field keeps and `store.put` gives.
pub fn file_value(store: &str, key: &str, name: Option<&str>, size: i64, file_type: &str) -> Value {
    let ordinal = jadpo_std::variant_ordinal(jadpo_std::FILE_TYPE, file_type).unwrap_or(0);
    Value::record(
        FILE,
        vec![
            (Rc::from("store"), Value::text(store)),
            (Rc::from("key"), Value::text(key)),
            (Rc::from("name"), name.map_or(Value::None, Value::text)),
            (Rc::from("size"), Value::Int(size)),
            (
                Rc::from("type"),
                Value::variant(jadpo_std::FILE_TYPE, ordinal, file_type, Vec::new()),
            ),
        ],
    )
}

/// The parts of `file`, a `File`; `None` for anything else.
pub fn stored(file: &Value) -> Option<Stored> {
    let Value::Record(record) = file else {
        return None;
    };
    let text = |name: &str| match record.get(name) {
        Some(Value::Text(text)) => Some(text.to_string()),
        _ => None,
    };
    let size = match record.get("size") {
        Some(Value::Int(size)) => u64::try_from(*size).ok()?,
        _ => return None,
    };
    Some(Stored {
        store: text("store")?,
        key: text("key")?,
        size,
    })
}

/// The media type of a `FileType` value.
pub(crate) fn media_type(file_type: &Value) -> Option<&'static str> {
    let Value::Variant(variant) = file_type else {
        return None;
    };
    jadpo_std::FILE_TYPES
        .iter()
        .find(|(name, _)| *name == &*variant.name)
        .map(|(_, media)| *media)
}

/// What an outside call's refusal is at the site that made it.
fn unanswered(refused: Unanswered, what: &str) -> Done {
    match refused {
        Unanswered::Failure(failure) => Done::Failed(failure),
        Unanswered::Fault(kind) => Done::Fault(kind),
        Unanswered::Failed(code) => Done::Fault(FaultKind::Defect(format!(
            "{what} answered `{code}`, which nothing maps"
        ))),
    }
}

/// `store.put(bytes, type)` through the `files` connection `connection`
/// (area 16): the bytes kept under a key the runtime makes, giving the
/// `File`.
pub async fn put(ctx: &mut Ctx<'_>, connection: &str, bytes: &Value, file_type: &Value) -> Done {
    let Value::Bytes(held) = bytes else {
        return Done::Fault(FaultKind::Defect("a put given no bytes".into()));
    };
    let (Some(media), Value::Variant(variant)) = (media_type(file_type), file_type) else {
        return Done::Fault(FaultKind::Defect("a put given no file type".into()));
    };
    let size = i64::try_from(held.len()).unwrap_or(i64::MAX);
    // In an action with several changing calls a `put` is a step like a
    // provider's call (area 12; chunk 7.5b): its object's key is derived
    // from its place, so a rerun writes the same object rather than a
    // second one, and an answered one is given back without being sent.
    let operation = format!("{connection}.{PUT}");
    let comes = crate::outside::comes_from(
        ctx,
        crate::outside::Asking {
            step: Some(crate::steps::Proposed {
                operation: &operation,
                fields: vec![
                    ("type".to_string(), media.as_bytes().to_vec()),
                    ("bytes".to_string(), held.to_vec()),
                ],
                // Sent again, it writes the same object under the same key.
                repeat: Repeat::Safe,
                changes: true,
                window_ms: None,
                did_nothing: &[],
            }),
            rerun: None,
        },
    )
    .await;
    let step = match comes {
        Ok(crate::outside::Comes::Send(place)) => place.step,
        Ok(crate::outside::Comes::Recorded { answered, at }) => {
            let key = at
                .and_then(|at| crate::steps::object_id(ctx, at))
                .map(|id| key_for(&id));
            return match (answered, key) {
                (crate::steps::Answered::Failed(code), _) if code == STORAGE_FULL => {
                    Done::Broke(RuleKey::StorageFull)
                }
                (crate::steps::Answered::Done, Some(key)) => {
                    let input = Value::record("FilePut", Vec::new());
                    let mut made = Call::new(connection, PUT, input, Repeat::Keyed);
                    // Logged as the step it is (chunk 7.5f).
                    made.step = at;
                    crate::outside::given_back(ctx, &made, &Answer::Value(Value::Unit));
                    Done::Value(file_value(connection, &key, None, size, &variant.name))
                }
                _ => Done::Fault(FaultKind::Defect(format!(
                    "`{operation}`'s recorded answer isn't one a store gives"
                ))),
            };
        }
        Ok(_) => {
            return Done::Fault(FaultKind::Defect(format!(
                "`{operation}` given back a model's answer"
            )));
        }
        Err(refused) => return unanswered(refused, &format!("`{operation}`")),
    };
    let id = match step.and_then(|at| crate::steps::object_id(ctx, at)) {
        Some(id) => id,
        None => {
            let Some(id) = ctx.new_id() else {
                return Done::Fault(FaultKind::Defect(
                    "a file's key with no randomness to make it".into(),
                ));
            };
            id
        }
    };
    let key = key_for(&id);
    // Recorded as held by none before the bytes go, so bytes no record
    // ever holds are found and deleted after a day, a crash between the
    // two included (chunk B3).
    if let Err(refused) = keep_put(ctx, connection, &key).await {
        return match refused {
            StoreError::Unavailable | StoreError::ConcurrentChange => {
                Done::Failed(Box::new(Failure {
                    frames: ctx.frames(),
                    ..crate::unavailable()
                }))
            }
            other => Done::Fault(FaultKind::Defect(format!(
                "the record of held files refused a put: {other:?}"
            ))),
        };
    }
    let input = Value::record(
        "FilePut",
        vec![
            (Rc::from("key"), Value::text(&key)),
            (Rc::from("type"), Value::text(media)),
            (Rc::from("bytes"), bytes.clone()),
        ],
    );
    let mut made = Call::new(connection, PUT, input, Repeat::Keyed);
    made.step = step;
    let sent = call(ctx, made).await;
    let kept = match (&sent, step) {
        (Ok(_), Some(at)) => crate::steps::answered(ctx, at, crate::steps::Answered::Done).await,
        (Err(Unanswered::Failed(code)), Some(at)) if code == STORAGE_FULL => {
            crate::steps::answered(ctx, at, crate::steps::Answered::Failed(code.clone())).await
        }
        _ => Ok(()),
    };
    if let Err(refused) = kept {
        return unanswered(refused, &format!("`{operation}`"));
    }
    match sent {
        Ok(_) => Done::Value(file_value(connection, &key, None, size, &variant.name)),
        Err(Unanswered::Failed(code)) if code == STORAGE_FULL => Done::Broke(RuleKey::StorageFull),
        Err(refused) => unanswered(refused, &format!("`{connection}.put`")),
    }
}

/// Records the file a put keeps under `key` in `connection` as held by
/// none, in the store keeping the record of its files, if one does.
async fn keep_put(ctx: &mut Ctx<'_>, connection: &str, key: &str) -> Result<(), StoreError> {
    let Some(ledger) = ctx.started().files().ledger(connection).map(str::to_string) else {
        return Ok(());
    };
    let now = crate::steps::wall(ctx);
    let database = ctx
        .capabilities()
        .database(&ledger)
        .ok_or_else(|| StoreError::Fault(format!("no store called `{ledger}`")))?;
    database
        .query_once(
            KEEP_PUT,
            &[
                SqlValue::Text(connection.to_string()),
                SqlValue::Text(key.to_string()),
                micros(now),
            ],
        )
        .await
        .map(|_| ())
}

/// The value of the event that deletes the bytes of `key` in `store`.
fn released_value(store: &str, key: &str) -> Value {
    Value::record(
        FILE_RELEASED,
        vec![
            (Rc::from("store"), Value::text(store)),
            (Rc::from("key"), Value::text(key)),
        ],
    )
}

/// Its payload, the runtime's own JSON.
fn released_payload(store: &str, key: &str) -> String {
    serde_json::json!({ "store": store, "key": key }).to_string()
}

/// The value a [`FILE_RELEASED`] event's payload holds.
pub(crate) fn read_released(payload: &str) -> Result<Value, String> {
    let Ok(serde_json::Value::Object(object)) = serde_json::from_str::<serde_json::Value>(payload)
    else {
        return Err("not a released file's payload".to_string());
    };
    let field = |name: &str| object.get(name).and_then(serde_json::Value::as_str);
    match (field("store"), field("key")) {
        (Some(store), Some(key)) if jadpo_std::files::is_key(key) => Ok(released_value(store, key)),
        _ => Err("not a released file's payload".to_string()),
    }
}

/// The store of files and key a [`FILE_RELEASED`] event names.
pub(crate) fn released_parts(value: &Value) -> Option<(String, String)> {
    let Value::Record(record) = value else {
        return None;
    };
    match (record.get("store"), record.get("key")) {
        (Some(Value::Text(store)), Some(Value::Text(key))) => {
            Some((store.to_string(), key.to_string()))
        }
        _ => None,
    }
}

/// The capability's `delete` of `key`, which its subscriber sends.
pub(crate) fn delete_call(connection: &str, key: &str) -> Call {
    let input = Value::record("FileDelete", vec![(Rc::from("key"), Value::text(key))]);
    Call::new(connection, DELETE, input, Repeat::Keyed)
}

/// The events deleting the bytes of `released`, each `(store, key)`, in
/// order: each handed to its store's generated subscriber.
fn released_events(released: &mut [(String, String)]) -> Vec<Event> {
    released.sort();
    released
        .iter()
        .map(|(store, key)| Event {
            key: FILE_RELEASED.to_string(),
            value: released_value(store, key),
            payload: released_payload(store, key),
            subscribers: vec![FilesPlan::subscriber(store)],
        })
        .collect()
}

/// The `(store, key)` of each row `rows` gave.
fn named(rows: Vec<crate::Row>) -> Vec<(String, String)> {
    rows.into_iter()
        .filter_map(|row| match (row.first(), row.get(1)) {
            (Some(SqlValue::Text(store)), Some(SqlValue::Text(key))) => {
                Some((store.clone(), key.clone()))
            }
            _ => None,
        })
        .collect()
}

/// As a commit in `transaction` ends (chunk B3): each file its changes
/// left held by none, marked as being deleted at `now`, with the event
/// that deletes its bytes, which commits with them.
pub(crate) async fn released(
    transaction: &mut dyn Transaction,
    now: i64,
) -> Result<Vec<Event>, StoreError> {
    let rows = transaction.query(RELEASED, &[micros(now)]).await?;
    Ok(released_events(&mut named(rows)))
}

/// What a tick's sweep of the records of held files did.
#[derive(Debug, Default)]
pub struct Swept {
    /// The deliveries of the events it kept, to make within the tick.
    pub deferred: Vec<crate::runner::Deferred>,
    pub problems: Vec<(String, StoreError)>,
    /// How many files' bytes it set out to delete.
    pub due: u64,
}

/// The tick's sweep of each store keeping a record of held files, at
/// `now` (chunk B3): at most a hundred files due their bytes' deletion in
/// each ([`DUE`]), marked as being deleted, with the event that deletes
/// them, in one transaction.
pub async fn sweep(executor: &dyn Executor, budget: &crate::budget::Budget<'_>, now: i64) -> Swept {
    let mut out = Swept::default();
    let Some(events) = executor.plan().events() else {
        return out;
    };
    let mut ledgers: Vec<&String> = events.plan.files.ledgers.values().collect();
    ledgers.sort();
    ledgers.dedup();
    let host = budget.host();
    for ledger in ledgers {
        if budget.spent() {
            break;
        }
        let mut ctx = crate::receiving::system_ctx(executor, host, now);
        match swept(&mut ctx, ledger, now).await {
            Ok(deliveries) => {
                out.due += deliveries.len() as u64;
                if !deliveries.is_empty() {
                    out.deferred
                        .push(crate::runner::Deferred::Deliveries(deliveries));
                }
            }
            Err(error) => out.problems.push((ledger.clone(), error)),
        }
    }
    out
}

/// One store's sweep.
async fn swept(ctx: &mut Ctx<'_>, ledger: &str, now: i64) -> Result<Vec<Delivery>, StoreError> {
    let database = ctx
        .capabilities()
        .database(ledger)
        .ok_or_else(|| StoreError::Fault(format!("no store called `{ledger}`")))?;
    let mut transaction = database.begin_at(Isolation::ReadCommitted).await?;
    let rows = match transaction
        .query(
            DUE,
            &[micros(now), micros(now.saturating_sub(UNHELD_KEPT_MS))],
        )
        .await
    {
        Ok(rows) => rows,
        Err(error) => {
            let _ = transaction.rollback().await;
            return Err(error);
        }
    };
    let events = released_events(&mut named(rows));
    if events.is_empty() {
        transaction.rollback().await?;
        return Ok(Vec::new());
    }
    let mut sealed = Sealed {
        outbox: Vec::with_capacity(events.len()),
        deliveries: Vec::with_capacity(events.len()),
    };
    for event in events {
        let (Some(outbox), Some(delivery)) = (ctx.new_id(), ctx.new_id()) else {
            let _ = transaction.rollback().await;
            return Err(StoreError::Fault(
                "no random bytes for an event's identity".into(),
            ));
        };
        for subscriber in &event.subscribers {
            sealed.deliveries.push(Delivery {
                store: ledger.to_string(),
                id: delivery,
                outbox,
                subscriber: subscriber.clone(),
                event: event.key.clone(),
                value: event.value.clone(),
                emitted_at: now,
                trace: None,
                depth: 0,
            });
        }
        sealed.outbox.push((outbox, event.key, event.payload));
    }
    if let Err(error) = crate::outbox::write(transaction.as_mut(), &sealed, now, None, 0).await {
        let _ = transaction.rollback().await;
        return Err(error);
    }
    transaction.commit().await?;
    Ok(sealed.deliveries)
}

/// The media type of the `FileType` variant called `name`.
pub fn media_of(name: &str) -> Option<&'static str> {
    jadpo_std::FILE_TYPES
        .iter()
        .find(|(variant, _)| *variant == name)
        .map(|(_, media)| *media)
}

/// Piece `piece` of the file whose new id is `id`, `bytes` of type
/// `media`, kept in the `files` connection `store` (chunk 6.4b2): a keyed
/// put, so putting it again keeps the bytes given last.
pub async fn put_piece(
    ctx: &mut Ctx<'_>,
    store: &str,
    (id, piece): (&[u8; 16], u64),
    media: &str,
    bytes: Vec<u8>,
) -> Result<(), Unanswered> {
    let input = Value::record(
        "FilePut",
        vec![
            (Rc::from("key"), Value::text(&piece_key(id, piece))),
            (Rc::from("type"), Value::text(media)),
            (Rc::from("bytes"), Value::Bytes(Rc::from(bytes))),
        ],
    );
    call(ctx, Call::new(store, PUT, input, Repeat::Keyed))
        .await
        .map(|_| ())
}

/// The file whose new id is `id`, its `pieces` pieces joined into it in
/// order in `store`, as `media` (chunk 6.4b2): a repeat once it is whole
/// does nothing.
pub async fn join(
    ctx: &mut Ctx<'_>,
    store: &str,
    id: &[u8; 16],
    pieces: u64,
    media: &str,
) -> Result<(), Unanswered> {
    let input = Value::record(
        "FileJoin",
        vec![
            (Rc::from("key"), Value::text(&key_for(id))),
            (
                Rc::from("pieces"),
                Value::Int(i64::try_from(pieces).unwrap_or(i64::MAX)),
            ),
            (Rc::from("type"), Value::text(media)),
        ],
    );
    call(ctx, Call::new(store, JOIN, input, Repeat::Keyed))
        .await
        .map(|_| ())
}

/// Drops the first `pieces` pieces of the file whose new id is `id`, and
/// the file if they were joined: a cancelled export's (chunk 6.4b2).
pub async fn discard(
    ctx: &mut Ctx<'_>,
    store: &str,
    id: &[u8; 16],
    pieces: u64,
) -> Result<(), Unanswered> {
    let input = Value::record(
        "FileDiscard",
        vec![
            (Rc::from("key"), Value::text(&key_for(id))),
            (
                Rc::from("pieces"),
                Value::Int(i64::try_from(pieces).unwrap_or(i64::MAX)),
            ),
        ],
    );
    call(ctx, Call::new(store, DISCARD, input, Repeat::Keyed))
        .await
        .map(|_| ())
}

/// At most `length` of `file`'s bytes from `from`, through its store:
/// fewer only at its end, or when the store reads less at once.
pub async fn read(
    ctx: &mut Ctx<'_>,
    file: &Stored,
    from: u64,
    length: u64,
) -> Result<Vec<u8>, Unanswered> {
    let input = Value::record(
        "FileRead",
        vec![
            (Rc::from("key"), Value::text(&file.key)),
            (
                Rc::from("from"),
                Value::Int(i64::try_from(from).unwrap_or(i64::MAX)),
            ),
            (
                Rc::from("length"),
                Value::Int(i64::try_from(length).unwrap_or(i64::MAX)),
            ),
        ],
    );
    match call(ctx, Call::new(&file.store, READ, input, Repeat::Safe)).await? {
        Value::Bytes(bytes) => Ok(bytes.to_vec()),
        _ => Err(Unanswered::Fault(FaultKind::Defect(format!(
            "`{}.read` answered with no bytes",
            file.store
        )))),
    }
}

/// `store.read(file)` (area 16): the file's bytes whole, as
/// `FileContents`, read in windows; a file over [`WHOLE_READ_LIMIT`] is
/// refused before anything is read.
pub async fn read_whole(ctx: &mut Ctx<'_>, file: &Value) -> Done {
    let Some(stored) = stored(file) else {
        return Done::Fault(FaultKind::Defect("a read given no stored file".into()));
    };
    if stored.size > WHOLE_READ_LIMIT {
        return Done::Fault(FaultKind::Defect(format!(
            "a whole read of a file over {}, which only the format helpers and `batches.each` read",
            jadpo_std::files::shown_size(WHOLE_READ_LIMIT)
        )));
    }
    let mut bytes: Vec<u8> = Vec::with_capacity(usize::try_from(stored.size).unwrap_or(0));
    while (bytes.len() as u64) < stored.size {
        let from = bytes.len() as u64;
        match read(ctx, &stored, from, (stored.size - from).min(WINDOW)).await {
            Ok(more) if more.is_empty() => {
                return Done::Fault(FaultKind::Defect(
                    "a stored file shorter than its size".into(),
                ));
            }
            Ok(more) => bytes.extend_from_slice(&more),
            Err(Unanswered::Failed(code)) if code == NOT_FOUND => {
                return Done::Fault(FaultKind::Defect(
                    "a stored file whose bytes are no longer in its store".into(),
                ));
            }
            Err(refused) => return unanswered(refused, &format!("`{}.read`", stored.store)),
        }
    }
    Done::Value(Value::Bytes(Rc::from(bytes)))
}

/// The `files` capability in memory (area 13, "Provider calls and
/// capabilities in tests"): each key's bytes kept in a map, read `read_at_most`
/// bytes at a time at most, as a store reading in pieces would; told to,
/// it answers a put `storage_full`.
#[derive(Debug, Default)]
pub struct MemoryFiles {
    files: RefCell<BTreeMap<String, Vec<u8>>>,
    read_at_most: Option<usize>,
    full: Cell<bool>,
    reads: Cell<usize>,
    kept: Cell<u64>,
}

impl MemoryFiles {
    pub fn new() -> MemoryFiles {
        MemoryFiles::default()
    }

    /// Reads at most `bytes` at a time.
    pub fn reading_at_most(mut self, bytes: usize) -> MemoryFiles {
        self.read_at_most = Some(bytes.max(1));
        self
    }

    /// Has no room from now on: every put answers `storage_full`.
    pub fn fill(&self) {
        self.full.set(true);
    }

    /// The bytes kept under `key`.
    pub fn bytes(&self, key: &str) -> Option<Vec<u8>> {
        self.files.borrow().get(key).cloned()
    }

    /// How many reads it has answered.
    pub fn reads(&self) -> usize {
        self.reads.get()
    }

    /// The keys it holds bytes under, in order.
    pub fn keys(&self) -> Vec<String> {
        self.files.borrow().keys().cloned().collect()
    }

    /// Forgets every file it holds.
    pub fn clear(&self) {
        self.files.borrow_mut().clear();
    }

    /// `bytes` kept as a file of the `FileType` variant `file_type` in
    /// connection `store`, as a test's fixture file is (area 13): the
    /// `File` an action is given.
    pub fn keep(&self, store: &str, bytes: &[u8], file_type: &str) -> Value {
        self.kept.set(self.kept.get() + 1);
        let mut id = [0xf0u8; 16];
        for (slot, byte) in id.iter_mut().rev().zip(self.kept.get().to_le_bytes()) {
            *slot = byte;
        }
        let key = key_for(&id);
        self.files.borrow_mut().insert(key.clone(), bytes.to_vec());
        file_value(
            store,
            &key,
            None,
            i64::try_from(bytes.len()).unwrap_or(i64::MAX),
            file_type,
        )
    }

    fn answer(&self, call: &Call) -> Answer {
        let Value::Record(input) = &call.input else {
            return Answer::Misconfigured;
        };
        let Some(Value::Text(key)) = input.get("key") else {
            return Answer::Misconfigured;
        };
        if !jadpo_std::files::is_key(key) {
            return Answer::Failed(NOT_FOUND.into());
        }
        match call.operation.as_str() {
            PUT => {
                if self.full.get() {
                    return Answer::Failed(STORAGE_FULL.into());
                }
                let Some(Value::Bytes(bytes)) = input.get("bytes") else {
                    return Answer::Misconfigured;
                };
                self.files
                    .borrow_mut()
                    .insert(key.to_string(), bytes.to_vec());
                Answer::Value(Value::Unit)
            }
            READ => {
                self.reads.set(self.reads.get() + 1);
                let (Some(Value::Int(from)), Some(Value::Int(length))) =
                    (input.get("from"), input.get("length"))
                else {
                    return Answer::Misconfigured;
                };
                let files = self.files.borrow();
                let Some(bytes) = files.get(&**key) else {
                    return Answer::Failed(NOT_FOUND.into());
                };
                let from = usize::try_from(*from)
                    .unwrap_or(usize::MAX)
                    .min(bytes.len());
                let mut length = usize::try_from(*length).unwrap_or(0);
                if let Some(most) = self.read_at_most {
                    length = length.min(most);
                }
                let to = from.saturating_add(length).min(bytes.len());
                Answer::Value(Value::Bytes(Rc::from(
                    bytes.get(from..to).unwrap_or_default(),
                )))
            }
            JOIN => {
                let Some(Value::Int(pieces)) = input.get("pieces") else {
                    return Answer::Misconfigured;
                };
                let Some(id) = jadpo_std::files::key_id(key) else {
                    return Answer::Failed(NOT_FOUND.into());
                };
                let mut files = self.files.borrow_mut();
                let count = u64::try_from(*pieces).unwrap_or(0);
                let keys: Vec<String> = (0..count).map(|piece| piece_key(&id, piece)).collect();
                if keys.iter().all(|piece| files.contains_key(piece)) {
                    if self.full.get() {
                        return Answer::Failed(STORAGE_FULL.into());
                    }
                    let mut whole = Vec::new();
                    for piece in &keys {
                        if let Some(bytes) = files.remove(piece) {
                            whole.extend(bytes);
                        }
                    }
                    files.insert(key.to_string(), whole);
                    return Answer::Value(Value::Unit);
                }
                // Joined already: the pieces went as the file was kept.
                if files.contains_key(&**key) && keys.iter().all(|piece| !files.contains_key(piece))
                {
                    return Answer::Value(Value::Unit);
                }
                Answer::Failed(NOT_FOUND.into())
            }
            DISCARD => {
                let Some(Value::Int(pieces)) = input.get("pieces") else {
                    return Answer::Misconfigured;
                };
                let Some(id) = jadpo_std::files::key_id(key) else {
                    return Answer::Failed(NOT_FOUND.into());
                };
                let mut files = self.files.borrow_mut();
                for piece in 0..u64::try_from(*pieces).unwrap_or(0) {
                    files.remove(&piece_key(&id, piece));
                }
                files.remove(&**key);
                Answer::Value(Value::Unit)
            }
            DELETE => {
                self.files.borrow_mut().remove(&**key);
                Answer::Value(Value::Unit)
            }
            _ => Answer::Misconfigured,
        }
    }
}

impl Capability for MemoryFiles {
    fn call<'a>(&'a self, call: &'a Call) -> LocalFuture<'a, Answer> {
        let answer = self.answer(call);
        Box::pin(async move { answer })
    }

    fn source(&self) -> Source {
        Source::Fake
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::Faked;
    use crate::{Clock, Env, MemoryHost, Places};

    fn env() -> Env {
        Env {
            seed: Some(1),
            clock: Clock::TEST,
        }
    }

    /// A put keeps the bytes under a key the runtime makes and gives the
    /// `File`; a ranged read gives them back in pieces, and a whole read
    /// all of them; a full store answers `storage_full`, which the call's
    /// `attempt` maps.
    #[test]
    fn a_put_keeps_bytes_a_read_gives_them_back_and_a_full_store_refuses() {
        let files = Rc::new(MemoryFiles::new().reading_at_most(3));
        let host =
            Faked::new(MemoryHost::new()).with("imports", Rc::clone(&files) as Rc<dyn Capability>);
        let places = Places::new();
        let mut ctx = Ctx::new(&places, &host, env());
        let csv = Value::variant(
            jadpo_std::FILE_TYPE,
            jadpo_std::variant_ordinal(jadpo_std::FILE_TYPE, "csv").unwrap_or(0),
            "csv",
            Vec::new(),
        );
        let bytes = Value::Bytes(Rc::from(&b"a,b\n1,2\n"[..]));
        let kept = crate::block_on(put(&mut ctx, "imports", &bytes, &csv)).expect("no wait");
        let Done::Value(file) = kept else {
            panic!("a file: {kept:?}");
        };
        let kept = stored(&file).expect("a stored file");
        assert_eq!(kept.store, "imports");
        assert_eq!(kept.size, 8);
        assert!(jadpo_std::files::is_key(&kept.key), "{}", kept.key);
        assert_eq!(files.bytes(&kept.key).as_deref(), Some(&b"a,b\n1,2\n"[..]));

        let piece = crate::block_on(read(&mut ctx, &kept, 2, 100))
            .expect("no wait")
            .expect("bytes");
        assert_eq!(piece, b"b\n1", "three at a time");
        let whole = crate::block_on(read_whole(&mut ctx, &file)).expect("no wait");
        assert!(
            matches!(&whole, Done::Value(Value::Bytes(all)) if &**all == b"a,b\n1,2\n"),
            "{whole:?}"
        );
        // Logged without their bytes.
        let shown = format!("{:?}", ctx.capabilities().calls());
        assert!(!shown.contains("1,2"), "{shown}");

        files.fill();
        let full = crate::block_on(put(&mut ctx, "imports", &bytes, &csv)).expect("no wait");
        assert!(
            matches!(full, Done::Broke(RuleKey::StorageFull)),
            "{full:?}"
        );

        let large = file_value(
            "imports",
            &kept.key,
            None,
            WHOLE_READ_LIMIT as i64 + 1,
            "csv",
        );
        let refused = crate::block_on(read_whole(&mut ctx, &large)).expect("no wait");
        assert!(
            matches!(&refused, Done::Fault(FaultKind::Defect(what)) if what.contains("10 MB")),
            "{refused:?}"
        );
    }

    /// The capability's `delete` leaves nothing under its key, and a
    /// repeat does nothing more (chunk B3); the event that sends it reads
    /// back from its payload, which names only a key the runtime makes.
    #[test]
    fn a_delete_leaves_nothing_and_its_event_reads_back() {
        let files = MemoryFiles::new();
        let kept = files.keep("sheets", b"flour,1.20", "csv");
        let key = stored(&kept).expect("a stored file").key;
        let call = delete_call("sheets", &key);
        assert_eq!(call.operation, DELETE);
        assert_eq!(call.repeat, Repeat::Keyed);
        for _ in 0..2 {
            let answer = crate::block_on(files.call(&call)).expect("no wait");
            assert_eq!(answer, Answer::Value(Value::Unit));
            assert_eq!(files.bytes(&key), None);
        }
        let mut released = vec![("sheets".to_string(), key.clone())];
        let events = released_events(&mut released);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].key, FILE_RELEASED);
        assert_eq!(events[0].subscribers, ["files sheets"]);
        let read = read_released(&events[0].payload).expect("its payload reads");
        assert_eq!(read, events[0].value);
        assert_eq!(
            released_parts(&read),
            Some(("sheets".to_string(), key.clone()))
        );
        for wrong in [
            "{}",
            "[]",
            r#"{"store":"sheets","key":"../../etc/passwd"}"#,
            r#"{"store":"sheets"}"#,
        ] {
            assert!(read_released(wrong).is_err(), "{wrong}");
        }
    }
}
