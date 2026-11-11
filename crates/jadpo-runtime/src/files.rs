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
    COMPLETE, DELETE, DISCARD, FILE, FILE_RELEASED, FilesPlan, HEAD_BYTES, JOIN, NOT_FOUND,
    PARTS_MISSING, PARTS_UNEQUAL, PUT, PartsRefused, READ, STAT, STORAGE_FULL, StoreFails,
    UNHELD_KEPT_MS, UPLOADED, WHOLE_READ_LIMIT, WHOLE_STORE, key_for, parts_fit, piece_key,
    type_of,
};
use jadpo_value::Value;

use crate::capability::{Answer, Call, Capability, Kept, Receiving, Refused, Repeat, Source};
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

/// A file marked as being deleted at `$3` in one of several databases
/// counting its store's files (chunk B14), as long as none of this
/// database's records holds it: `RETURNING` a row only when it was
/// marked. A row it had none of is made, so no record here may hold it
/// from then on.
pub const MARK: &str = "INSERT INTO \"jadpo_files\" (\"store\", \"key\", \"holders\", \"since\") VALUES ($1, $2, -1, $3) ON CONFLICT (\"store\", \"key\") DO UPDATE SET \"holders\" = -1, \"since\" = $3 WHERE \"jadpo_files\".\"holders\" IN (0, -1) RETURNING 1";

/// How many of a database's records hold a file, or that its bytes are
/// being deleted (`-1`) or gone (`-2`).
pub const HOLDERS: &str =
    "SELECT \"holders\" FROM \"jadpo_files\" WHERE \"store\" = $1 AND \"key\" = $2";

/// A mark [`MARK`] made taken back, as another database's records still
/// hold the file: the row goes, as one no record here holds.
pub const UNMARK: &str =
    "DELETE FROM \"jadpo_files\" WHERE \"store\" = $1 AND \"key\" = $2 AND \"holders\" = -1";

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

/// The hash of `bytes` as a `File` keeps it (area 16, "Downloading"): what
/// its ETag is made from.
pub fn hash_of(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

/// A file's hash made as its bytes go by, a window at a time, as
/// [`hash_of`] makes it from them whole (`jadpo files show`).
#[derive(Debug, Default)]
pub struct Hashing(blake3::Hasher);

impl Hashing {
    pub fn new() -> Hashing {
        Hashing::default()
    }

    pub fn update(&mut self, bytes: &[u8]) {
        self.0.update(bytes);
    }

    pub fn finish(&self) -> String {
        self.0.finalize().to_hex().to_string()
    }
}

/// A `File` (area 16): what an entity field keeps and `store.put` gives.
pub fn file_value(
    store: &str,
    key: &str,
    name: Option<&str>,
    size: i64,
    file_type: &str,
    hash: Option<&str>,
) -> Value {
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
            (Rc::from("hash"), hash.map_or(Value::None, Value::text)),
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

/// What a file of `file_type` ends in (area 16, "Attachments"), when it is
/// one `FileType` holds.
pub(crate) fn extension(file_type: &Value) -> Option<&'static str> {
    let Value::Variant(variant) = file_type else {
        return None;
    };
    jadpo_std::files::extension(&variant.name)
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
    let hash = hash_of(held);
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
                    Done::Value(file_value(
                        connection,
                        &key,
                        None,
                        size,
                        &variant.name,
                        Some(&hash),
                    ))
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
    if let Err(refused) = keep_unheld(ctx, connection, &key).await {
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
        Ok(_) => Done::Value(file_value(
            connection,
            &key,
            None,
            size,
            &variant.name,
            Some(&hash),
        )),
        Err(Unanswered::Failed(code)) if code == STORAGE_FULL => Done::Broke(RuleKey::StorageFull),
        Err(refused) => unanswered(refused, &format!("`{connection}.put`")),
    }
}

/// The failure a `FileUpload` an attach can't take raises (area 16,
/// "Uploading"): `invalid_value`, why only in its log.
fn upload_refused(why: &str) -> Failure {
    let mut failure = Failure::new("runtime invalid_value", "InvalidValue", "invalid_value");
    failure.kind = Some("invalid_value".to_string());
    failure.status = Some(422);
    failure.message = Some("This upload can't be attached.".to_string());
    failure
        .internal
        .push(("reason".to_string(), Value::text(why)));
    failure
}

/// The failure a slot's parts raise when joining them would take a disk
/// below its floor (area 16, "Large uploads go in parts"): the client's
/// file is whole, but there is no room to keep it joined.
fn no_room_to_join() -> Failure {
    let mut failure = Failure::new("runtime storage_full", "StorageFull", "storage_full");
    failure.kind = Some("unavailable".to_string());
    failure.status = Some(503);
    failure.message = Some("There is no room to keep this file.".to_string());
    failure
}

/// Asks the store what the upload `token` names left, and gives the
/// `File` it is, or stops the write as `invalid_value` (area 16,
/// "Uploading"): a token for another field, or a file that isn't there,
/// is too large or isn't a type the field takes. An outside call, so it
/// ends the reads before it, as any call does; only the runtime's refusal
/// of one once the action has written is left, which the compiler rules
/// out.
pub(crate) fn attach<'a, 'p>(
    ctx: &'a mut Ctx<'p>,
    check: &'a jadpo_std::write::FileCheck,
    token: &'a str,
    site: u32,
) -> LocalFuture<'a, Result<Value, crate::read::Interrupt>> {
    Box::pin(attached(ctx, check, token, site))
}

/// [`attach`]'s work, boxed so a write's own frame stays small.
async fn attached(
    ctx: &mut Ctx<'_>,
    check: &jadpo_std::write::FileCheck,
    token: &str,
    site: u32,
) -> Result<Value, crate::read::Interrupt> {
    let refused = |ctx: &mut Ctx<'_>, why: &str| {
        crate::read::Interrupt::Failed(ctx.fail(upload_refused(why), site))
    };
    let Some(slot) = crate::http::uploads::token_slot(ctx.host(), &check.store, token) else {
        return Err(refused(ctx, "a token this field's store didn't sign"));
    };
    if slot.field != check.field {
        return Err(refused(ctx, "a token for another field"));
    }
    // A slot in parts: joined here, once every part is there, so what is
    // checked below is the whole file (area 16, "Large uploads go in
    // parts").
    if let Some(part_size) = slot.part_size {
        let input = Value::record(
            "FileComplete",
            vec![
                (Rc::from("key"), Value::text(&slot.key)),
                (
                    Rc::from("part_size"),
                    Value::Int(i64::try_from(part_size).unwrap_or(i64::MAX)),
                ),
                (
                    Rc::from("max_size"),
                    Value::Int(i64::try_from(slot.max_size).unwrap_or(i64::MAX)),
                ),
            ],
        );
        match call(ctx, Call::new(&check.store, COMPLETE, input, Repeat::Safe)).await {
            Ok(_) => {}
            Err(Unanswered::Failed(code)) if code == PARTS_MISSING || code == NOT_FOUND => {
                return Err(refused(ctx, "a part of this slot hasn't been sent"));
            }
            Err(Unanswered::Failed(code)) if code == PARTS_UNEQUAL => {
                return Err(refused(ctx, "this slot's parts aren't equal"));
            }
            Err(Unanswered::Failed(code)) if code == STORAGE_FULL => {
                return Err(crate::read::Interrupt::Failed(
                    ctx.fail(no_room_to_join(), site),
                ));
            }
            Err(Unanswered::Failed(code)) => {
                return Err(crate::read::Interrupt::Fault(ctx.fault(
                    FaultKind::Defect(format!("`{}.complete` answered `{code}`", check.store)),
                    site,
                )));
            }
            Err(Unanswered::Failure(failure)) => {
                return Err(crate::read::Interrupt::Failed(ctx.fail(*failure, site)));
            }
            Err(Unanswered::Fault(kind)) => {
                return Err(crate::read::Interrupt::Fault(ctx.fault(kind, site)));
            }
        }
    }
    let input = Value::record(
        "FileStat",
        vec![
            (Rc::from("key"), Value::text(&slot.key)),
            (
                Rc::from("max_size"),
                Value::Int(i64::try_from(slot.max_size).unwrap_or(i64::MAX)),
            ),
        ],
    );
    let answer = match call(ctx, Call::new(&check.store, STAT, input, Repeat::Safe)).await {
        Ok(answer) => answer,
        Err(Unanswered::Failed(code)) if code == NOT_FOUND => {
            return Err(refused(ctx, "no file was uploaded to this slot"));
        }
        Err(Unanswered::Failed(code)) if code == jadpo_std::files::UPLOAD_COMPRESSED => {
            return Err(refused(ctx, "the file was sent compressed"));
        }
        Err(Unanswered::Failure(failure)) => {
            return Err(crate::read::Interrupt::Failed(ctx.fail(*failure, site)));
        }
        Err(Unanswered::Fault(kind)) => {
            return Err(crate::read::Interrupt::Fault(ctx.fault(kind, site)));
        }
        Err(Unanswered::Failed(code)) => {
            return Err(crate::read::Interrupt::Fault(ctx.fault(
                FaultKind::Defect(format!("`{}.stat` answered `{code}`", check.store)),
                site,
            )));
        }
    };
    let Value::Record(about) = &answer else {
        return Err(crate::read::Interrupt::Fault(ctx.fault(
            FaultKind::Defect("a store's answer about an upload isn't a record".into()),
            site,
        )));
    };
    let text = |name: &str| match about.get(name) {
        Some(Value::Text(text)) => Some(text.to_string()),
        _ => None,
    };
    let size = match about.get("size") {
        Some(Value::Int(size)) => u64::try_from(*size).unwrap_or(u64::MAX),
        _ => u64::MAX,
    };
    if size > check.max_size {
        return Err(refused(ctx, "the file is over what the field holds"));
    }
    let head = match about.get("head") {
        Some(Value::Bytes(head)) => head.to_vec(),
        _ => Vec::new(),
    };
    let claimed = text("claimed");
    let Some(file_type) = type_of(&head, claimed.as_deref(), &check.types) else {
        return Err(refused(
            ctx,
            "the file's first bytes aren't a type the field takes",
        ));
    };
    Ok(file_value(
        &check.store,
        &slot.key,
        text("name").as_deref(),
        i64::try_from(size).unwrap_or(i64::MAX),
        file_type,
        text("hash").as_deref(),
    ))
}

/// Records the file a put or an upload keeps under `key` in `connection`
/// as held by none, in the store keeping the record of its files, if one
/// does (area 16, "Uploading"), before its bytes land.
pub(crate) async fn keep_unheld(
    ctx: &Ctx<'_>,
    connection: &str,
    key: &str,
) -> Result<(), StoreError> {
    let Some(counter) = ctx
        .started()
        .files()
        .counter(connection)
        .map(str::to_string)
    else {
        return Ok(());
    };
    let now = crate::steps::wall(ctx);
    let database = ctx
        .capabilities()
        .database(&counter)
        .ok_or_else(|| StoreError::Fault(format!("no store called `{counter}`")))?;
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

/// Whether a file's bytes may be deleted now, as its subscriber is about
/// to (chunk B14).
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Decided {
    /// No database's records hold it, and none may from now on.
    Delete,
    /// Another database's records still hold it: its mark here is taken
    /// back, and that database deletes the bytes once its records let go.
    Held,
}

/// Decides whether the bytes of `key` in store of files `connection` may
/// be deleted, when fields in entities of several databases name the
/// store (area 16, "Deleting and erasing"; chunk B14). Each database is
/// marked as deleting the file, which refuses any record there holding
/// it from then on, unless one of its records already does: then every
/// database's mark is taken back, any another commit left included, and
/// the bytes stay; whichever database lets go last decides again. The
/// first database's mark
/// is held in a transaction until the others are done, so two deletions
/// of one file are decided one at a time. With one database, the commit
/// that let go of the file marked it already.
pub(crate) async fn decide(
    ctx: &Ctx<'_>,
    connection: &str,
    key: &str,
    now: i64,
) -> Result<Decided, StoreError> {
    let counting = ctx.started().files().counting(connection).to_vec();
    let Some((first, rest)) = counting.split_first() else {
        return Ok(Decided::Delete);
    };
    if rest.is_empty() {
        return Ok(Decided::Delete);
    }
    let database = |store: &str| {
        ctx.capabilities()
            .database(store)
            .ok_or_else(|| StoreError::Fault(format!("no store called `{store}`")))
    };
    let named = [
        SqlValue::Text(connection.to_string()),
        SqlValue::Text(key.to_string()),
    ];
    let marking = [named[0].clone(), named[1].clone(), micros(now)];
    // Whether a database's records hold the file, once a mark finds them
    // doing so; a file gone already is deleted again, which does nothing.
    let held = |rows: &[crate::Row]| {
        rows.first()
            .and_then(|row| row.first())
            .is_some_and(|holders| matches!(holders, SqlValue::Int4(n) if *n > 0))
    };
    let mut lead = database(first)?.begin_at(Isolation::ReadCommitted).await?;
    let marked = match lead.query(MARK, &marking).await {
        Ok(rows) => rows,
        Err(error) => {
            let _ = lead.rollback().await;
            return Err(error);
        }
    };
    let mut decided = Decided::Delete;
    if marked.is_empty() {
        match lead.query(HOLDERS, &named).await {
            Ok(rows) if held(&rows) => decided = Decided::Held,
            Ok(_) => {}
            Err(error) => {
                let _ = lead.rollback().await;
                return Err(error);
            }
        }
    }
    for store in rest {
        if decided == Decided::Held {
            break;
        }
        let found = async {
            let other = database(store)?;
            if !other.query_once(MARK, &marking).await?.is_empty() {
                return Ok(false);
            }
            Ok::<bool, StoreError>(held(&other.query_once(HOLDERS, &named).await?))
        }
        .await;
        match found {
            Ok(false) => {}
            Ok(true) => decided = Decided::Held,
            Err(error) => {
                // Each mark made stays: a later attempt marks them again.
                let _ = lead.rollback().await;
                return Err(error);
            }
        }
    }
    // Every mark taken back, those made here and any a commit left in
    // another database: its own delivery decides again, after this one.
    if decided == Decided::Held {
        for store in rest {
            if let Err(error) = database(store)?.query_once(UNMARK, &named).await {
                let _ = lead.rollback().await;
                return Err(error);
            }
        }
        if let Err(error) = lead.query(UNMARK, &named).await {
            let _ = lead.rollback().await;
            return Err(error);
        }
    }
    lead.commit().await?;
    Ok(decided)
}

/// Records the bytes of `key` in `connection` gone in every database
/// counting its files, once its store confirmed deleting them, so none
/// of their records may hold it again.
pub(crate) async fn record_gone(
    ctx: &Ctx<'_>,
    connection: &str,
    key: &str,
) -> Result<(), StoreError> {
    let named = [
        SqlValue::Text(connection.to_string()),
        SqlValue::Text(key.to_string()),
    ];
    for store in ctx.started().files().counting(connection) {
        let database = ctx
            .capabilities()
            .database(store)
            .ok_or_else(|| StoreError::Fault(format!("no store called `{store}`")))?;
        database.query_once(GONE, &named).await?;
    }
    Ok(())
}

/// What a tick's sweep of the records of held files did.
#[derive(Debug, Default)]
pub struct Sweep {
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
pub async fn sweep(executor: &dyn Executor, budget: &crate::budget::Budget<'_>, now: i64) -> Sweep {
    let mut out = Sweep::default();
    let Some(events) = executor.plan().events() else {
        return out;
    };
    let host = budget.host();
    for counter in events.plan.files.stores() {
        if budget.spent() {
            break;
        }
        let mut ctx = crate::receiving::system_ctx(executor, host, now);
        match sweeping(&mut ctx, counter, now).await {
            Ok(deliveries) => {
                out.due += deliveries.len() as u64;
                if !deliveries.is_empty() {
                    out.deferred
                        .push(crate::runner::Deferred::Deliveries(deliveries));
                }
            }
            Err(error) => out.problems.push((counter.to_string(), error)),
        }
    }
    out
}

/// One store's sweep.
async fn sweeping(ctx: &mut Ctx<'_>, counter: &str, now: i64) -> Result<Vec<Delivery>, StoreError> {
    let database = ctx
        .capabilities()
        .database(counter)
        .ok_or_else(|| StoreError::Fault(format!("no store called `{counter}`")))?;
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
                store: counter.to_string(),
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

/// What a store answers [`jadpo_std::files::KEYS`] with: `keys`, each a
/// key and its size, and `next`, where the following page starts, or
/// `none` after the last.
pub fn keys_answer(keys: &[(String, u64)], next: Option<&str>) -> Value {
    let listed = keys
        .iter()
        .map(|(key, size)| {
            Value::record(
                jadpo_std::files::FILE_KEY,
                vec![
                    (Rc::from("key"), Value::text(key)),
                    (
                        Rc::from("size"),
                        Value::Int(i64::try_from(*size).unwrap_or(i64::MAX)),
                    ),
                ],
            )
        })
        .collect();
    Value::record(
        jadpo_std::files::FILE_KEYS,
        vec![
            (Rc::from("keys"), Value::list(listed)),
            (Rc::from("next"), next.map_or(Value::None, Value::text)),
        ],
    )
}

/// One page of [`jadpo_std::files::KEYS`] from `held`, every key a store
/// holds with its size, in any order: those after `after` that
/// [`jadpo_std::files::is_listed`] allows, in order.
pub fn keys_page(held: impl IntoIterator<Item = (String, u64)>, after: &str) -> Value {
    let mut keys: Vec<(String, u64)> = held
        .into_iter()
        .filter(|(key, _)| key.as_str() > after && jadpo_std::files::is_listed(key))
        .collect();
    keys.sort();
    let more = keys.len() > jadpo_std::files::KEYS_PAGE;
    keys.truncate(jadpo_std::files::KEYS_PAGE);
    let next = more
        .then(|| keys.last().map(|(key, _)| key.clone()))
        .flatten();
    keys_answer(&keys, next.as_deref())
}

/// One page of a store's keys, each with its size, and where the next
/// page starts.
pub type KeysPage = (Vec<(String, u64)>, Option<String>);

/// What a [`jadpo_std::files::KEYS`] answer holds: its keys with their
/// sizes, and where the next page starts.
pub fn keys_of(answer: &Value) -> Option<KeysPage> {
    let Value::Record(record) = answer else {
        return None;
    };
    let Some(Value::List(listed)) = record.get("keys") else {
        return None;
    };
    let mut keys = Vec::new();
    for item in listed.iter() {
        let Value::Record(item) = item else {
            return None;
        };
        let (Some(Value::Text(key)), Some(Value::Int(size))) = (item.get("key"), item.get("size"))
        else {
            return None;
        };
        keys.push((key.to_string(), u64::try_from(*size).unwrap_or(0)));
    }
    let next = match record.get("next") {
        Some(Value::Text(next)) => Some(next.to_string()),
        _ => None,
    };
    Some((keys, next))
}

/// The `after` a [`jadpo_std::files::KEYS`] call is given, or empty text.
pub fn keys_after(input: &jadpo_value::Record) -> String {
    match input.get("after") {
        Some(Value::Text(after)) => after.to_string(),
        _ => String::new(),
    }
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
) -> Result<Option<String>, Unanswered> {
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
        .map(|answer| match answer {
            Value::Text(hash) => Some(hash.to_string()),
            _ => None,
        })
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

/// Whether `value` holds a stored `File`, however deep.
pub(crate) fn holds_stored(value: &Value) -> bool {
    let mut found = Vec::new();
    stored_in(value, &mut found);
    !found.is_empty()
}

/// Each stored `File` `value` holds, however deep, in the order it holds
/// them, once each: a record's and a variant's fields, a list's, a set's
/// and a map's values.
fn stored_in(value: &Value, out: &mut Vec<Value>) {
    match value {
        Value::Record(record) if &*record.ty == jadpo_std::files::FILE => {
            if !out.contains(value) {
                out.push(value.clone());
            }
        }
        Value::Record(record) => record
            .fields
            .iter()
            .for_each(|(_, field)| stored_in(field, out)),
        Value::Variant(variant) => variant
            .fields
            .iter()
            .for_each(|(_, field)| stored_in(field, out)),
        Value::List(items) | Value::Set(items) => {
            items.iter().for_each(|item| stored_in(item, out))
        }
        Value::Map(map) => map.values().for_each(|item| stored_in(item, out)),
        _ => {}
    }
}

/// `value` with each `File` among `read` in its place as the bytes given
/// with it.
fn with_read(value: &Value, read: &[(Value, Value)]) -> Value {
    match value {
        Value::Record(record) if &*record.ty == jadpo_std::files::FILE => read
            .iter()
            .find(|(file, _)| file == value)
            .map_or_else(|| value.clone(), |(_, bytes)| bytes.clone()),
        Value::Record(record) => {
            let mut out = (**record).clone();
            out.fields = record
                .fields
                .iter()
                .map(|(name, field)| (name.clone(), with_read(field, read)))
                .collect();
            Value::Record(Rc::new(out))
        }
        Value::Variant(variant) => {
            let mut out = (**variant).clone();
            out.fields = variant
                .fields
                .iter()
                .map(|(name, field)| (name.clone(), with_read(field, read)))
                .collect();
            Value::Variant(Rc::new(out))
        }
        Value::List(items) => Value::List(items.iter().map(|item| with_read(item, read)).collect()),
        // A set's order is its values', so bytes in place of files are
        // put in order again.
        Value::Set(items) => {
            let mut out: Vec<Value> = items.iter().map(|item| with_read(item, read)).collect();
            out.sort();
            out.dedup();
            Value::Set(out.into())
        }
        Value::Map(map) => Value::Map(Rc::new(
            map.iter()
                .map(|(key, item)| (key.clone(), with_read(item, read)))
                .collect(),
        )),
        other => other.clone(),
    }
}

/// `value` with each stored `File` it holds, however deep, as its bytes,
/// each read whole from its store as `store.read` reads it: what an
/// operation's `input:` and a webhook's `reply:` send (area 11, "Types";
/// chunk B14). A file over [`WHOLE_READ_LIMIT`], or whose bytes are gone,
/// is a fault, as it is for `store.read`.
pub(crate) async fn with_bytes(ctx: &mut Ctx<'_>, value: &Value) -> Done {
    let mut files = Vec::new();
    stored_in(value, &mut files);
    let mut read = Vec::with_capacity(files.len());
    for file in files {
        match Box::pin(read_whole(ctx, &file)).await {
            Done::Value(bytes) => read.push((file, bytes)),
            other => return other,
        }
    }
    Done::Value(with_read(value, &read))
}

/// The `files` capability in memory (area 13, "Provider calls and
/// capabilities in tests"): each key's bytes kept in a map, read `read_at_most`
/// bytes at a time at most, as a store reading in pieces would; told to,
/// it answers a put `storage_full`.
#[derive(Debug, Default)]
pub struct MemoryFiles {
    files: RefCell<BTreeMap<String, Vec<u8>>>,
    /// What each upload left beside its bytes.
    uploads: RefCell<BTreeMap<String, MemoryUpload>>,
    /// The parts of each upload in parts not yet joined, and what its
    /// first part said of the file.
    parts: RefCell<BTreeMap<String, (MemoryParts, Option<MemoryUpload>)>>,
    read_at_most: Option<usize>,
    full: Cell<bool>,
    reads: Cell<usize>,
    kept: Cell<u64>,
    /// What a test's `fakes:` told each connection's store to answer
    /// (area 13, "Files"), by the connection's name.
    told: RefCell<BTreeMap<String, StoreFails>>,
    /// The project folder a test's fixture files are read from, below
    /// each domain's `fixtures/files/`; `None` where none can be read.
    fixtures: Option<std::path::PathBuf>,
}

impl MemoryFiles {
    pub fn new() -> MemoryFiles {
        MemoryFiles::default()
    }

    /// Reads at most `bytes` at a time.
    /// Joins the parts of the upload `key`, as the disk driver does: part
    /// after part, once every one is there and they are equal.
    fn complete(&self, key: &str, input: &jadpo_value::Record) -> Answer {
        let Some(Value::Int(part_size)) = input.get("part_size") else {
            return Answer::Misconfigured;
        };
        if self.files.borrow().contains_key(key) {
            return Answer::Value(Value::Unit);
        }
        let mut uploads = self.parts.borrow_mut();
        let Some((held, about)) = uploads.get(key) else {
            return Answer::Failed(PARTS_MISSING.into());
        };
        let mut sizes = Vec::new();
        let mut gaps = false;
        for (expected, (number, bytes)) in (1u64..).zip(held) {
            if *number != expected {
                gaps = true;
                break;
            }
            sizes.push(bytes.len() as u64);
        }
        match parts_fit(&sizes, gaps, u64::try_from(*part_size).unwrap_or(0)) {
            Ok(()) => {}
            Err(PartsRefused::Missing) => return Answer::Failed(PARTS_MISSING.into()),
            Err(PartsRefused::Unequal) => return Answer::Failed(PARTS_UNEQUAL.into()),
        }
        if self.full.get() {
            return Answer::Failed(STORAGE_FULL.into());
        }
        let about = about.as_ref();
        let whole: Vec<u8> = held.values().flatten().copied().collect();
        let upload = MemoryUpload {
            hash: hash_of(&whole),
            name: about.and_then(|about| about.name.clone()),
            claimed: about.and_then(|about| about.claimed.clone()),
        };
        self.uploads.borrow_mut().insert(key.to_string(), upload);
        self.files.borrow_mut().insert(key.to_string(), whole);
        uploads.remove(key);
        Answer::Value(Value::Unit)
    }

    pub fn reading_at_most(mut self, bytes: usize) -> MemoryFiles {
        self.read_at_most = Some(bytes.max(1));
        self
    }

    /// Reads a test's fixture files from the project at `root`.
    pub fn fixtures_under(mut self, root: impl Into<std::path::PathBuf>) -> MemoryFiles {
        self.fixtures = Some(root.into());
        self
    }

    /// Answers every call through `connection` as `fails` says, from now
    /// on (area 13, "Files").
    pub fn tell(&self, connection: &str, fails: StoreFails) {
        self.told.borrow_mut().insert(connection.to_string(), fails);
    }

    /// Why an upload to `connection` is refused, as a test told its store.
    fn refuses(&self, connection: &str) -> Result<(), Refused> {
        match self.told.borrow().get(connection) {
            Some(StoreFails::Unavailable) => Err(Refused::Unavailable),
            Some(StoreFails::StorageFull) => Err(Refused::Full),
            None => Ok(()),
        }
    }

    /// The fixture file at `path`, relative to the project, kept as a
    /// file of the `FileType` variant `file_type` named `name` in
    /// connection `store` (area 13, "Files"): the `File`, or why it
    /// couldn't be read. Only a regular file inside the project is read,
    /// so a link can't bring a file from elsewhere on the machine into a
    /// test.
    fn fixture_at(
        &self,
        store: &str,
        path: &str,
        file_type: &str,
        name: &str,
    ) -> Result<Value, String> {
        let Some(root) = &self.fixtures else {
            return Err(format!(
                "`{path}` can't be read here: fixture files are read by `jadpo test` from the project's folder"
            ));
        };
        let unread = |why: String| format!("the fixture file `{path}` couldn't be read: {why}");
        let within = std::fs::canonicalize(root).map_err(|error| unread(error.to_string()))?;
        let found =
            std::fs::canonicalize(root.join(path)).map_err(|error| unread(error.to_string()))?;
        if !found.starts_with(&within) {
            return Err(unread("it links to a file outside the project".to_string()));
        }
        if !std::fs::metadata(&found).is_ok_and(|meta| meta.is_file()) {
            return Err(unread("it isn't a regular file".to_string()));
        }
        let bytes = std::fs::read(&found).map_err(|error| unread(error.to_string()))?;
        Ok(self.keep_named(store, &bytes, file_type, Some(name)))
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
        self.parts.borrow_mut().clear();
    }

    /// `bytes` kept as a file of the `FileType` variant `file_type` in
    /// connection `store`, as a test's fixture file is (area 13): the
    /// `File` an action is given.
    pub fn keep(&self, store: &str, bytes: &[u8], file_type: &str) -> Value {
        self.keep_named(store, bytes, file_type, None)
    }

    /// [`MemoryFiles::keep`], the file named `name`.
    fn keep_named(&self, store: &str, bytes: &[u8], file_type: &str, name: Option<&str>) -> Value {
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
            name,
            i64::try_from(bytes.len()).unwrap_or(i64::MAX),
            file_type,
            Some(&hash_of(bytes)),
        )
    }

    fn answer(&self, call: &Call) -> Answer {
        let Value::Record(input) = &call.input else {
            return Answer::Misconfigured;
        };
        // What a test told this connection's store (area 13, "Files").
        match self.told.borrow().get(&call.connection) {
            Some(StoreFails::Unavailable) => return Answer::Unavailable,
            Some(StoreFails::StorageFull)
                if matches!(call.operation.as_str(), PUT | JOIN | COMPLETE) =>
            {
                return Answer::Failed(STORAGE_FULL.into());
            }
            _ => {}
        }
        // Every key it keeps, a page at a time (area 14, `jadpo files`).
        if call.operation == jadpo_std::files::KEYS {
            let files = self.files.borrow();
            return Answer::Value(keys_page(
                files
                    .iter()
                    .map(|(key, bytes)| (key.clone(), bytes.len() as u64)),
                &keys_after(input),
            ));
        }
        // Every key of the erasure log kept here (area 15; chunk P4c).
        if call.operation == jadpo_std::files::LIST {
            let mut keys: Vec<Value> = self
                .files
                .borrow()
                .keys()
                .filter(|key| jadpo_std::erasure::log_key_parts(key).is_some())
                .map(|key| Value::text(key))
                .collect();
            keys.sort_by_key(|key| match key {
                Value::Text(text) => text.to_string(),
                _ => String::new(),
            });
            return Answer::Value(Value::list(keys));
        }
        let Some(Value::Text(key)) = input.get("key") else {
            return Answer::Misconfigured;
        };
        let logged = jadpo_std::erasure::log_key_parts(key).is_some()
            && matches!(call.operation.as_str(), PUT | READ);
        let export = jadpo_std::privacy::is_export_key(key)
            && matches!(call.operation.as_str(), PUT | READ | DELETE);
        if !jadpo_std::files::is_key(key) && !logged && !export {
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
                    let hash = hash_of(&whole);
                    files.insert(key.to_string(), whole);
                    return Answer::Value(Value::text(&hash));
                }
                // Joined already: the pieces went as the file was kept.
                if keys.iter().all(|piece| !files.contains_key(piece))
                    && let Some(whole) = files.get(&**key)
                {
                    return Answer::Value(Value::text(&hash_of(whole)));
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
                self.uploads.borrow_mut().remove(&**key);
                self.parts.borrow_mut().remove(&**key);
                Answer::Value(Value::Unit)
            }
            COMPLETE => self.complete(key, input),
            STAT => {
                let uploads = self.uploads.borrow();
                let (Some(about), Some(bytes)) = (
                    uploads.get(&**key),
                    self.files.borrow().get(&**key).cloned(),
                ) else {
                    return Answer::Failed(NOT_FOUND.into());
                };
                Answer::Value(uploaded(
                    bytes.get(..bytes.len().min(HEAD_BYTES)).unwrap_or_default(),
                    bytes.len() as u64,
                    Some(&about.hash),
                    about.name.as_deref(),
                    about.claimed.as_deref(),
                ))
            }
            _ => Answer::Misconfigured,
        }
    }
}

/// What an upload left beside its bytes, in a test's store.
/// The parts of an upload a test's store is keeping, by number.
type MemoryParts = BTreeMap<u64, Vec<u8>>;

#[derive(Debug)]
struct MemoryUpload {
    hash: String,
    name: Option<String>,
    claimed: Option<String>,
}

/// The [`UPLOADED`] answering a store's [`STAT`]: what a store keeps of an
/// upload, and the first of its bytes.
pub fn uploaded(
    head: &[u8],
    size: u64,
    hash: Option<&str>,
    name: Option<&str>,
    claimed: Option<&str>,
) -> Value {
    Value::record(
        UPLOADED,
        vec![
            (
                Rc::from("size"),
                Value::Int(i64::try_from(size).unwrap_or(i64::MAX)),
            ),
            (Rc::from("hash"), hash.map_or(Value::None, Value::text)),
            (Rc::from("name"), name.map_or(Value::None, Value::text)),
            (
                Rc::from("claimed"),
                claimed.map_or(Value::None, Value::text),
            ),
            (Rc::from("head"), Value::Bytes(Rc::from(head))),
        ],
    )
}

/// An upload a test's store is keeping.
struct MemoryReceiving<'a> {
    store: &'a MemoryFiles,
    key: String,
    bytes: Vec<u8>,
    /// The part it is, when it is one.
    part: Option<u64>,
}

impl Receiving for MemoryReceiving<'_> {
    fn write(&mut self, bytes: &[u8]) -> Result<(), Refused> {
        if self.store.full.get() {
            return Err(Refused::Full);
        }
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }

    fn finish(self: Box<Self>, kept: &Kept<'_>) -> Result<(), Refused> {
        let mut files = self.store.files.borrow_mut();
        if files.contains_key(&self.key) {
            return Err(Refused::Taken);
        }
        if let Some(part) = self.part {
            let mut uploads = self.store.parts.borrow_mut();
            let (held, about) = uploads.entry(self.key).or_default();
            if part == 1 {
                *about = Some(MemoryUpload {
                    hash: kept.hash.to_string(),
                    name: kept.name.map(str::to_string),
                    claimed: kept.claimed.map(str::to_string),
                });
            }
            held.insert(part, self.bytes);
            return Ok(());
        }
        self.store.uploads.borrow_mut().insert(
            self.key.clone(),
            MemoryUpload {
                hash: kept.hash.to_string(),
                name: kept.name.map(str::to_string),
                claimed: kept.claimed.map(str::to_string),
            },
        );
        files.insert(self.key, self.bytes);
        Ok(())
    }
}

impl Capability for MemoryFiles {
    fn call<'a>(&'a self, call: &'a Call) -> LocalFuture<'a, Answer> {
        let answer = self.answer(call);
        Box::pin(async move { answer })
    }

    fn receive(
        &self,
        connection: &str,
        key: &str,
        _expected: u64,
    ) -> Result<Box<dyn Receiving + '_>, Refused> {
        self.refuses(connection)?;
        if self.full.get() {
            return Err(Refused::Full);
        }
        if self.files.borrow().contains_key(key) {
            return Err(Refused::Taken);
        }
        Ok(Box::new(MemoryReceiving {
            store: self,
            key: key.to_string(),
            bytes: Vec::new(),
            part: None,
        }))
    }

    fn receive_part(
        &self,
        connection: &str,
        key: &str,
        part: u64,
        _expected: u64,
    ) -> Result<Box<dyn Receiving + '_>, Refused> {
        self.refuses(connection)?;
        if self.full.get() {
            return Err(Refused::Full);
        }
        if self.files.borrow().contains_key(key) {
            return Err(Refused::Taken);
        }
        Ok(Box::new(MemoryReceiving {
            store: self,
            key: key.to_string(),
            bytes: Vec::new(),
            part: Some(part),
        }))
    }

    /// A test's `fakes: { <store>: unavailable }` or `storage_full`
    /// (area 13, "Files"): the store as a whole answers so from now on.
    fn script(
        &self,
        connection: &str,
        operation: &str,
        answer: Answer,
        _from: crate::Frame,
    ) -> bool {
        if operation != WHOLE_STORE {
            return false;
        }
        let fails = match answer {
            Answer::Unavailable => StoreFails::Unavailable,
            Answer::Failed(code) if code == STORAGE_FULL => StoreFails::StorageFull,
            _ => return false,
        };
        self.tell(connection, fails);
        true
    }

    fn fixture(
        &self,
        connection: &str,
        path: &str,
        file_type: &str,
        name: &str,
    ) -> Option<Result<Value, String>> {
        Some(self.fixture_at(connection, path, file_type, name))
    }

    fn link_key(&self, _connection: &str, _make: bool) -> Option<[u8; 32]> {
        // A test's store's: its links are good for that store alone, by
        // the store's name in what signs them.
        Some([0x4c; 32])
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
            None,
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
