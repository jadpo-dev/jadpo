//! Entities in Redis (area 5, "Entities in Redis"; area 6, "Reading and
//! writing entities in Redis"; chunk Q10): reads and writes of an entity
//! whose `store:` is a Redis connection, run for both executors from the
//! plans `jadpo-plan` decides ([`jadpo_std::redis`]).
//!
//! - **One JSON value a record** ([`jadpo_std::redis::record_key`]): the
//!   record as its codec writes it, with the revision Jadpo keeps on it
//!   and when it expires, `expires_after:` from its last write. A record
//!   past it is hidden from every read and write, even in the moment
//!   before Redis removes it, and so is one this build can't read, such as
//!   one an earlier build wrote with fields it no longer has: both are
//!   harmless to lose.
//! - **A key per unique value and two sets per indexed value**, each named
//!   from a digest of the value, never the value as written.
//! - **An action's writes commit together** ([`Pending`]): each write is
//!   worked out where it is made, against the record as the action sees
//!   it, and kept; the action's commit checks, in one step no other write
//!   comes between, that every record it wrote is still at the revision it
//!   was read at and every unique value it takes is free, then makes all
//!   its changes, or none. A record it loaded that moved meanwhile is a
//!   concurrent change, so the action runs again; an update by `id` that
//!   loaded nothing, a counter's among them, is worked out again against
//!   the record as stored then, so counters racing on one record never run
//!   each other's actions again and no increment is lost.
//! - **Policy is applied in Jadpo's own code** ([`crate::policy::permits`])
//!   to each record once it is read: one the caller may not read is not
//!   found, as policy hides a record in a database.
//!
//! [`Records`] is a store's side of it: a Redis connection's driver, or
//! [`MemoryRecords`], what `jadpo dev` and every test keep records in,
//! which behaves as the driver does.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::rc::Rc;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use jadpo_std::read::{Cardinality, ReadPlan, Size};
use jadpo_std::redis::{Element, Given, Kept, Lookup, RedisEntity, RedisRead, RedisWrite};
use jadpo_std::write::{WriteOp, WritePlan};
use jadpo_value::Value;

use crate::ratelimit::Unasked;
use crate::read::{Found, Interrupt, JsonCodecs, defect, refused, uuid_text};
use crate::write::Written;
use crate::{Ctx, Failure, LocalFuture, SqlValue, StoreError};

/// What one commit holds to before it changes anything.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Check {
    /// The record under `key` is at `revision`: 0 for no record, or one
    /// that has expired or isn't a record this build wrote.
    Revision { key: String, revision: u64 },
    /// `key`, the key of a unique value, is free for the record `id`: it
    /// holds nothing, `id` itself, or a record (under `records`, a record
    /// key's start) there no longer is.
    Free {
        key: String,
        id: String,
        records: String,
    },
}

/// One change a commit makes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Change {
    /// `key` holds `value`, expiring in `ttl_ms`.
    Set {
        key: String,
        value: Vec<u8>,
        ttl_ms: i64,
    },
    /// `key` holds nothing.
    Delete { key: String },
    /// The sorted set `set` holds `member` at `score`.
    Add {
        set: String,
        score: i64,
        member: String,
    },
    /// The sorted set `set` no longer holds `member`.
    Remove { set: String, member: String },
    /// `key` expires in `ttl_ms`.
    Expire { key: String, ttl_ms: i64 },
}

/// What one action's writes to a store come to as it commits: every check,
/// then every change, made as one step or not at all.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Commit {
    pub checks: Vec<Check>,
    pub changes: Vec<Change>,
}

/// A page of an indexed value's records, as a store is asked for one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PageAsk {
    /// The set of the records holding the value, by `id`.
    pub index: String,
    /// The set of when each expires.
    pub expiry: String,
    /// What every record key starts with, an `id` after it.
    pub records: String,
    /// The `id` the page continues after.
    pub after: Option<String>,
    /// How many ids at most.
    pub size: u32,
}

/// A page's ids, each with what its record key holds.
pub type Listed = Vec<(String, Option<Vec<u8>>)>;

/// A store an entity's records are kept in (area 5, "Entities in Redis"):
/// a Redis connection's driver, or [`MemoryRecords`]. Every `now` is the
/// operation's clock, in milliseconds, which a record's expiry is judged
/// by.
pub trait Records {
    /// What each of `keys` holds, in order.
    fn get<'a>(
        &'a self,
        keys: &'a [String],
        now: i64,
    ) -> LocalFuture<'a, Result<Vec<Option<Vec<u8>>>, Unasked>>;

    /// The next `ask.size` ids of `ask.index` after `ask.after`, in order,
    /// each with what its record key holds, once every id `ask.expiry`
    /// says expired by `now` is taken out of both sets.
    fn page<'a>(&'a self, ask: &'a PageAsk, now: i64) -> LocalFuture<'a, Result<Listed, Unasked>>;

    /// How many ids the set `expiry` holds that expire after `now`.
    fn count<'a>(&'a self, expiry: &'a str, now: i64) -> LocalFuture<'a, Result<u64, Unasked>>;

    /// Asks every one of `commit.checks` and, when all hold, makes every
    /// one of `commit.changes`, as one step no other write comes between:
    /// `Ok(None)` once made, `Ok(Some(at))` when the check at `at` didn't
    /// hold, and nothing was changed.
    fn commit<'a>(
        &'a self,
        commit: &'a Commit,
        now: i64,
    ) -> LocalFuture<'a, Result<Option<u32>, Unasked>>;
}

// --- what a key holds ----------------------------------------------------------------

/// How a record's value starts: `{"revision":`.
const HEAD: &[u8] = b"{\"revision\":";

/// The revision and the expiry a record's value starts with; `None` for a
/// value that isn't a record's.
pub fn head(value: &[u8]) -> Option<(u64, i64)> {
    fn digits(bytes: &[u8]) -> Option<(u64, &[u8])> {
        let end = bytes.iter().position(|b| !b.is_ascii_digit())?;
        let (number, rest) = bytes.split_at(end);
        if number.is_empty() {
            return None;
        }
        Some((std::str::from_utf8(number).ok()?.parse().ok()?, rest))
    }
    let rest = value.strip_prefix(HEAD)?;
    let (revision, rest) = digits(rest)?;
    let rest = rest.strip_prefix(b",\"expires\":")?;
    let (expires, rest) = digits(rest)?;
    rest.strip_prefix(b",")?;
    Some((revision, i64::try_from(expires).ok()?))
}

/// The revision of what a record key holds at `now`: 0 for nothing, a
/// record expired by then, or a value that isn't a record's.
pub fn revision_at(value: Option<&[u8]>, now: i64) -> u64 {
    match value.and_then(head) {
        Some((revision, expires)) if expires > now => revision,
        _ => 0,
    }
}

/// Whether `check` holds at `now`, `get` giving what a key holds: what
/// every store asks, so the driver's script, the double and memory agree.
pub fn holds(check: &Check, get: &dyn Fn(&str) -> Option<Vec<u8>>, now: i64) -> bool {
    match check {
        Check::Revision { key, revision } => revision_at(get(key).as_deref(), now) == *revision,
        Check::Free { key, id, records } => match get(key) {
            None => true,
            Some(held) if held == id.as_bytes() => true,
            Some(held) => {
                let holder = format!("{records}{}", String::from_utf8_lossy(&held));
                revision_at(get(&holder).as_deref(), now) == 0
            }
        },
    }
}

// --- in memory ------------------------------------------------------------------------

/// One key's value in memory, and when it expires.
#[derive(Clone, Debug)]
enum Held {
    Text(Vec<u8>, i64),
    /// A sorted set: each member's score.
    Sorted(BTreeMap<String, i64>, i64),
}

#[derive(Debug, Default)]
struct Memory {
    keys: HashMap<String, Held>,
}

impl Memory {
    fn sweep(&mut self, now: i64) {
        self.keys.retain(|_, held| match held {
            Held::Text(_, expires) | Held::Sorted(_, expires) => *expires > now,
        });
    }

    fn text(&self, key: &str) -> Option<Vec<u8>> {
        match self.keys.get(key) {
            Some(Held::Text(value, _)) => Some(value.clone()),
            _ => None,
        }
    }

    fn change(&mut self, change: &Change, now: i64) {
        match change {
            Change::Set { key, value, ttl_ms } => {
                self.keys.insert(
                    key.clone(),
                    Held::Text(value.clone(), now.saturating_add(*ttl_ms)),
                );
            }
            Change::Delete { key } => {
                self.keys.remove(key);
            }
            Change::Add { set, score, member } => match self.keys.get_mut(set) {
                Some(Held::Sorted(members, _)) => {
                    members.insert(member.clone(), *score);
                }
                // A new set, which keeps until it is given an expiry.
                _ => {
                    let members = BTreeMap::from([(member.clone(), *score)]);
                    self.keys
                        .insert(set.clone(), Held::Sorted(members, i64::MAX));
                }
            },
            Change::Remove { set, member } => {
                if let Some(Held::Sorted(members, _)) = self.keys.get_mut(set) {
                    members.remove(member);
                    if members.is_empty() {
                        self.keys.remove(set);
                    }
                }
            }
            Change::Expire { key, ttl_ms } => match self.keys.get_mut(key) {
                Some(Held::Text(_, expires) | Held::Sorted(_, expires)) => {
                    *expires = now.saturating_add(*ttl_ms);
                }
                None => {}
            },
        }
    }
}

/// An entity's records in this process's memory, which behave as the
/// Redis driver keeps them: `jadpo dev`'s, one per connection for all its
/// workers ([`MemoryRecords::process`]), and a test's, one of its own per
/// test, empty at the start of each (area 13).
#[derive(Clone, Debug, Default)]
pub struct MemoryRecords {
    memory: Arc<Mutex<Memory>>,
}

impl MemoryRecords {
    /// An empty store of its own.
    pub fn new() -> MemoryRecords {
        MemoryRecords::default()
    }

    /// Connection `connection`'s records in this process, which every
    /// worker thread shares.
    pub fn process(connection: &str) -> MemoryRecords {
        static STORES: OnceLock<Mutex<BTreeMap<String, MemoryRecords>>> = OnceLock::new();
        let stores = STORES.get_or_init(Mutex::default);
        let mut stores = stores.lock().unwrap_or_else(PoisonError::into_inner);
        stores.entry(connection.to_string()).or_default().clone()
    }

    fn memory(&self) -> std::sync::MutexGuard<'_, Memory> {
        self.memory.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Every key it holds at `now`, in order: what a test reads to see
    /// what is kept, and that no value is kept as written.
    pub fn keys(&self, now: i64) -> Vec<String> {
        let mut memory = self.memory();
        memory.sweep(now);
        let mut out: Vec<String> = memory.keys.keys().cloned().collect();
        out.sort();
        out
    }

    /// What `key` holds at `now`, as text.
    pub fn text(&self, key: &str, now: i64) -> Option<String> {
        let mut memory = self.memory();
        memory.sweep(now);
        memory
            .text(key)
            .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
    }
}

impl Records for MemoryRecords {
    fn get<'a>(
        &'a self,
        keys: &'a [String],
        now: i64,
    ) -> LocalFuture<'a, Result<Vec<Option<Vec<u8>>>, Unasked>> {
        let mut memory = self.memory();
        memory.sweep(now);
        let out = keys.iter().map(|key| memory.text(key)).collect();
        Box::pin(async move { Ok(out) })
    }

    fn page<'a>(&'a self, ask: &'a PageAsk, now: i64) -> LocalFuture<'a, Result<Listed, Unasked>> {
        let mut memory = self.memory();
        memory.sweep(now);
        let expired: Vec<String> = match memory.keys.get(&ask.expiry) {
            Some(Held::Sorted(members, _)) => members
                .iter()
                .filter(|(_, score)| **score <= now)
                .map(|(member, _)| member.clone())
                .collect(),
            _ => Vec::new(),
        };
        for member in &expired {
            for set in [&ask.index, &ask.expiry] {
                memory.change(
                    &Change::Remove {
                        set: set.clone(),
                        member: member.clone(),
                    },
                    now,
                );
            }
        }
        let ids: Vec<String> = match memory.keys.get(&ask.index) {
            Some(Held::Sorted(members, _)) => members
                .keys()
                .filter(|id| ask.after.as_ref().is_none_or(|after| *id > after))
                .take(ask.size as usize)
                .cloned()
                .collect(),
            _ => Vec::new(),
        };
        let out = ids
            .into_iter()
            .map(|id| {
                let record = memory.text(&format!("{}{id}", ask.records));
                (id, record)
            })
            .collect();
        Box::pin(async move { Ok(out) })
    }

    fn count<'a>(&'a self, expiry: &'a str, now: i64) -> LocalFuture<'a, Result<u64, Unasked>> {
        let mut memory = self.memory();
        memory.sweep(now);
        let count = match memory.keys.get(expiry) {
            Some(Held::Sorted(members, _)) => {
                members.values().filter(|score| **score > now).count() as u64
            }
            _ => 0,
        };
        Box::pin(async move { Ok(count) })
    }

    fn commit<'a>(
        &'a self,
        commit: &'a Commit,
        now: i64,
    ) -> LocalFuture<'a, Result<Option<u32>, Unasked>> {
        let mut memory = self.memory();
        memory.sweep(now);
        let failed = {
            let get = |key: &str| memory.text(key);
            commit
                .checks
                .iter()
                .position(|check| !holds(check, &get, now))
        };
        if failed.is_none() {
            for change in &commit.changes {
                memory.change(change, now);
            }
        }
        let failed = failed.map(|at| at as u32);
        Box::pin(async move { Ok(failed) })
    }
}

/// A host's stores of records, by connection, each the same however often
/// it is asked: what a test's and `jadpo dev`'s hosts keep every entity in
/// Redis in.
#[derive(Clone, Debug, Default)]
pub struct MemoryStores {
    stores: Rc<std::cell::RefCell<BTreeMap<String, MemoryRecords>>>,
}

impl MemoryStores {
    /// Connection `connection`'s store, made empty the first time it is
    /// asked for.
    pub fn of(&self, connection: &str) -> MemoryRecords {
        self.stores
            .borrow_mut()
            .entry(connection.to_string())
            .or_default()
            .clone()
    }
}

// --- one record ---------------------------------------------------------------------

/// A record as the store holds it, read back.
#[derive(Clone, Debug, PartialEq)]
struct Stored {
    revision: u64,
    record: Value,
}

/// A record's value as it is kept: its revision, when it expires and the
/// record as its codec writes it.
fn kept_value(revision: u64, expires: i64, record: &str) -> Vec<u8> {
    format!("{{\"revision\":{revision},\"expires\":{expires},\"record\":{record}}}").into_bytes()
}

/// What a record key's value reads as at `now`, by `entity`'s codec:
/// `None` for nothing, a record expired by then, or one this build can't
/// read (area 5: harmless to lose).
fn read_kept(
    json: &JsonCodecs<'_>,
    entity: &RedisEntity,
    bytes: Option<&[u8]>,
    now: i64,
) -> Option<Stored> {
    let bytes = bytes?;
    let (revision, expires) = head(bytes)?;
    if expires <= now {
        return None;
    }
    let codec = json.codec_of(entity.codec)?;
    let (document, numbers) = jadpo_value::json::parse(bytes)?;
    let record = document.get("record")?;
    let path =
        jadpo_value::json::Path::default().numbered(&numbers.within(&["record".to_string()]));
    let mut problems = Vec::new();
    let record = json.codecs.read(codec, record, &path, &mut problems)?;
    problems.is_empty().then_some(Stored { revision, record })
}

/// `record` as its codec writes it.
fn written(json: &JsonCodecs<'_>, entity: &RedisEntity, record: &Value) -> Option<String> {
    let codec = json.codec_of(entity.codec)?;
    let mut out = String::new();
    json.codecs.write(codec, record, &mut out);
    Some(out)
}

/// The digest a value of a keyed field is kept under: 32 hex digits of
/// BLAKE3 of its JSON; `None` for `none`, which no key holds.
fn digest(json: &JsonCodecs<'_>, codec: u32, value: &Value) -> Option<String> {
    if matches!(value, Value::None) {
        return None;
    }
    let codec = json.codec_of(codec)?;
    let mut out = String::new();
    json.codecs.write(codec, value, &mut out);
    let hex = blake3::hash(out.as_bytes()).to_hex();
    Some(hex.get(..32).unwrap_or(hex.as_str()).to_string())
}

fn field<'v>(record: &'v Value, name: &str) -> Option<&'v Value> {
    match record {
        Value::Record(record) => record.get(name),
        _ => None,
    }
}

/// A record's `id`, as text.
fn id_of(record: &Value) -> Option<String> {
    match field(record, "id") {
        Some(Value::Uuid(id)) => Some(id.to_string()),
        _ => None,
    }
}

// --- an action's writes ---------------------------------------------------------------

/// A record an action wrote, as it commits.
#[derive(Clone, Debug)]
struct Touched {
    entity: Rc<RedisEntity>,
    id: String,
    /// The record as the store held it when the action first read or wrote
    /// it: what the commit checks it still holds, and what its keys are
    /// changed from.
    before: Option<Stored>,
    /// Whether the action read it, so its writes depend on what it read
    /// and the commit holds it to that revision. One it didn't, an update
    /// by `id`, is worked out again against what is stored as it commits.
    held: bool,
    /// What it holds once the action commits; `None` once deleted.
    now: Option<Value>,
    /// For one not held: each write made to it, to work out again
    /// against what is stored.
    replay: Vec<Replayed>,
}

/// A write to a record an action didn't read, worked out again at the
/// commit when what is stored moved.
#[derive(Clone, Debug)]
enum Replayed {
    /// An update, with its site's inputs.
    Update(Rc<RedisWrite>, Vec<Value>),
    Delete,
}

/// One store's share of what the running action has read and written in
/// Redis, kept in its boundary until it commits.
#[derive(Clone, Debug, Default)]
pub(crate) struct Pending {
    /// Each record it wrote, by key, in the order first written.
    touched: Vec<(String, Touched)>,
    /// Each record it read, by key, with the revision read: what a write
    /// to it later in the action is held to.
    loaded: BTreeMap<String, u64>,
}

impl Pending {
    fn touched(&self, key: &str) -> Option<&Touched> {
        self.touched
            .iter()
            .find(|(held, _)| held == key)
            .map(|(_, touched)| touched)
    }

    fn touched_mut(&mut self, key: &str) -> Option<&mut Touched> {
        self.touched
            .iter_mut()
            .find(|(held, _)| held == key)
            .map(|(_, touched)| touched)
    }

    /// Whether it wrote anything, which then commits.
    pub(crate) fn writes(&self) -> bool {
        !self.touched.is_empty()
    }
}

/// The running action's pending share of `store`, made empty when first
/// asked for.
fn pending<'c>(ctx: &'c mut Ctx<'_>, store: &str) -> Option<&'c mut Pending> {
    Some(
        ctx.boundary_mut()?
            .redis_mut()
            .entry(store.to_string())
            .or_default(),
    )
}

/// The store `entity` is kept in, as the host gives it.
fn store_of(
    ctx: &mut Ctx<'_>,
    entity: &RedisEntity,
    site: u32,
) -> Result<Rc<dyn Records>, Interrupt> {
    match ctx.host().records(&entity.store) {
        Some(store) => Ok(store),
        None => Err(refused(
            ctx,
            StoreError::Fault(format!(
                "there is no Redis connection called `{}` to keep `{}` in",
                entity.store, entity.entity
            )),
            site,
        )),
    }
}

fn unreachable(ctx: &mut Ctx<'_>, site: u32) -> Interrupt {
    refused(ctx, StoreError::Unavailable, site)
}

/// The record `id` of `entity` as the running action sees it at `now`:
/// what it wrote, or what the store holds, noted as loaded at its revision
/// when `load` says so.
async fn record(
    ctx: &mut Ctx<'_>,
    store: &dyn Records,
    json: &JsonCodecs<'_>,
    entity: &RedisEntity,
    id: &str,
    load: bool,
    site: u32,
) -> Result<Option<Value>, Interrupt> {
    let key = jadpo_std::redis::record_key(&entity.keyspace, id);
    let now = ctx.now();
    if let Some(pending) = pending(ctx, &entity.store)
        && let Some(touched) = pending.touched_mut(&key)
    {
        // Read back, what it holds now decides what the action does next.
        if load {
            touched.held = true;
        }
        return Ok(touched.now.clone());
    }
    let found = match store.get(std::slice::from_ref(&key), now).await {
        Ok(found) => found,
        Err(_) => return Err(unreachable(ctx, site)),
    };
    let stored = read_kept(json, entity, found.first().and_then(Option::as_deref), now);
    if load && let Some(pending) = pending(ctx, &entity.store) {
        pending
            .loaded
            .entry(key)
            .or_insert(stored.as_ref().map_or(0, |stored| stored.revision));
    }
    Ok(stored.map(|stored| stored.record))
}

/// What `given` is for a read with `args`.
fn given_value(ctx: &Ctx<'_>, given: &Given, args: &[Value]) -> Option<Value> {
    match given {
        Given::Arg { index, path } => {
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
        Given::Constant(constant) => Some(Value::constant(constant)),
        Given::Now => Some(Value::Instant(ctx.now())),
    }
}

/// What a record read becomes.
fn element(element: &Element, record: Value) -> Value {
    match element {
        Element::Record => record,
        Element::Picked { ty, fields } => Value::record(
            ty,
            fields
                .iter()
                .map(|(name, from)| {
                    (
                        Rc::from(name.as_str()),
                        field(&record, from).cloned().unwrap_or(Value::None),
                    )
                })
                .collect(),
        ),
    }
}

// --- reads ----------------------------------------------------------------------------

/// Runs `plan`, a read of an entity in Redis, with `args`, from site `site`.
pub(crate) async fn read(
    ctx: &mut Ctx<'_>,
    plan: &ReadPlan,
    redis: &RedisRead,
    json: JsonCodecs<'_>,
    args: &[Value],
    site: u32,
) -> Result<Found, Interrupt> {
    let entity = &redis.entity;
    let store = store_of(ctx, entity, site)?;
    let grants = &entity.policy.read;
    // What a read of at most one record gives, for each result type.
    let one = |found: Option<Value>, concealed: bool| -> Found {
        let found = found.map(|value| element(&redis.element, value));
        match (plan.cardinality, found) {
            (Cardinality::List, found) => Found::Value(Value::list(found.into_iter().collect())),
            (Cardinality::Page, found) => Found::Value(Value::record(
                crate::read::PAGE,
                vec![
                    (Rc::from("items"), Value::list(found.into_iter().collect())),
                    (Rc::from("next"), Value::None),
                ],
            )),
            (_, Some(value)) => Found::Value(value),
            (Cardinality::One, None) => Found::Missing(concealed),
            (Cardinality::Optional, None) => Found::Value(Value::None),
        }
    };
    match &redis.lookup {
        Lookup::Id(given) => {
            let id = match given_value(ctx, given, args) {
                Some(Value::Uuid(id)) => id.to_string(),
                Some(Value::None) => return Ok(one(None, false)),
                _ => {
                    return Err(defect(
                        ctx,
                        "a Redis record looked up by what isn't an `id`",
                        site,
                    ));
                }
            };
            let found = record(ctx, store.as_ref(), &json, entity, &id, true, site).await?;
            Ok(match found {
                Some(record) if crate::policy::permits(ctx, grants, &record) => {
                    one(Some(record), false)
                }
                Some(_) => one(None, true),
                None => one(None, false),
            })
        }
        Lookup::Unique { field: at, value } => {
            let Some(keyed) = entity.unique.get(*at as usize) else {
                return Err(defect(ctx, "a unique field the plan doesn't list", site));
            };
            let Some(value) = given_value(ctx, value, args) else {
                return Err(defect(ctx, "a Redis lookup without its value", site));
            };
            let found = by_unique(
                ctx,
                store.as_ref(),
                &json,
                entity,
                keyed,
                &value,
                None,
                site,
            )
            .await?;
            let found = match found {
                Some(id) => record(ctx, store.as_ref(), &json, entity, &id, true, site).await?,
                None => None,
            };
            Ok(match found {
                Some(record) if crate::policy::permits(ctx, grants, &record) => {
                    one(Some(record), false)
                }
                Some(_) => one(None, true),
                None => one(None, false),
            })
        }
        Lookup::Index { field: at, value } => {
            let Some(keyed) = entity.indexed.get(*at as usize) else {
                return Err(defect(ctx, "an indexed field the plan doesn't list", site));
            };
            let Some(value) = given_value(ctx, value, args) else {
                return Err(defect(ctx, "a Redis lookup without its value", site));
            };
            listed(
                ctx,
                plan,
                redis,
                store.as_ref(),
                &json,
                keyed,
                &value,
                args,
                site,
            )
            .await
        }
        Lookup::Count {
            field: at,
            value,
            ty,
            total,
        } => {
            let Some(keyed) = entity.indexed.get(*at as usize) else {
                return Err(defect(ctx, "an indexed field the plan doesn't list", site));
            };
            let Some(value) = given_value(ctx, value, args) else {
                return Err(defect(ctx, "a Redis count without its value", site));
            };
            let count = counted(ctx, store.as_ref(), &json, entity, keyed, &value, site).await?;
            Ok(Found::Value(Value::record(
                ty,
                vec![(
                    Rc::from(total.as_str()),
                    Value::Int(i64::try_from(count).unwrap_or(i64::MAX)),
                )],
            )))
        }
    }
}

/// The `id` of the record of `entity` whose unique field `keyed` holds
/// `value`, as the running action sees it, but `except`'s.
#[allow(clippy::too_many_arguments)]
async fn by_unique(
    ctx: &mut Ctx<'_>,
    store: &dyn Records,
    json: &JsonCodecs<'_>,
    entity: &RedisEntity,
    keyed: &jadpo_std::redis::KeyedField,
    value: &Value,
    except: Option<&str>,
    site: u32,
) -> Result<Option<String>, Interrupt> {
    let Some(digest) = digest(json, keyed.codec, value) else {
        return Ok(None);
    };
    let now = ctx.now();
    // What the action itself wrote decides for the records it wrote.
    let mine: Vec<(String, bool)> = pending(ctx, &entity.store)
        .map(|pending| {
            pending
                .touched
                .iter()
                .filter(|(_, touched)| touched.entity.keyspace == entity.keyspace)
                .map(|(_, touched)| {
                    let holds = touched
                        .now
                        .as_ref()
                        .and_then(|record| field(record, &keyed.field))
                        == Some(value);
                    (touched.id.clone(), holds)
                })
                .collect()
        })
        .unwrap_or_default();
    if let Some((id, _)) = mine
        .iter()
        .find(|(id, holds)| *holds && Some(id.as_str()) != except)
    {
        return Ok(Some(id.clone()));
    }
    let key = jadpo_std::redis::unique_key(&entity.keyspace, &keyed.born, &digest);
    let held = match store.get(std::slice::from_ref(&key), now).await {
        Ok(found) => found.into_iter().next().flatten(),
        Err(_) => return Err(unreachable(ctx, site)),
    };
    let Some(held) = held.and_then(|bytes| String::from_utf8(bytes).ok()) else {
        return Ok(None);
    };
    if Some(held.as_str()) == except || mine.iter().any(|(id, _)| *id == held) {
        return Ok(None);
    }
    // The record it names still holds the value: a key outlives nothing.
    let record_key = jadpo_std::redis::record_key(&entity.keyspace, &held);
    let found = match store.get(std::slice::from_ref(&record_key), now).await {
        Ok(found) => found,
        Err(_) => return Err(unreachable(ctx, site)),
    };
    let stored = read_kept(json, entity, found.first().and_then(Option::as_deref), now);
    Ok(stored
        .filter(|stored| field(&stored.record, &keyed.field) == Some(value))
        .map(|_| held))
}

/// The records of `entity` whose indexed field `keyed` holds `value` that
/// the caller may read, as the running action sees them, in `id` order
/// after `after`: up to `want`, or every one.
#[allow(clippy::too_many_arguments)]
async fn holding(
    ctx: &mut Ctx<'_>,
    store: &dyn Records,
    json: &JsonCodecs<'_>,
    entity: &RedisEntity,
    keyed: &jadpo_std::redis::KeyedField,
    value: &Value,
    after: Option<String>,
    want: Option<usize>,
    site: u32,
) -> Result<Vec<(String, Value)>, Interrupt> {
    /// How many ids one ask for a page fetches at least.
    const BATCH: u32 = 100;
    let Some(digest) = digest(json, keyed.codec, value) else {
        return Ok(Vec::new());
    };
    let now = ctx.now();
    let grants = &entity.policy.read;
    // The records the action wrote: what it holds of each decides.
    let mine: Vec<(String, Option<Value>)> = pending(ctx, &entity.store)
        .map(|pending| {
            pending
                .touched
                .iter()
                .filter(|(_, touched)| touched.entity.keyspace == entity.keyspace)
                .map(|(_, touched)| (touched.id.clone(), touched.now.clone()))
                .collect()
        })
        .unwrap_or_default();
    let mut found: Vec<(String, Value)> = Vec::new();
    let mut ask = PageAsk {
        index: jadpo_std::redis::index_key(&entity.keyspace, &keyed.born, &digest),
        expiry: jadpo_std::redis::expiry_key(&entity.keyspace, &keyed.born, &digest),
        records: jadpo_std::redis::record_prefix(&entity.keyspace),
        after: after.clone(),
        size: want.map_or(BATCH, |want| {
            u32::try_from(want).unwrap_or(u32::MAX).max(BATCH)
        }),
    };
    loop {
        let batch = match store.page(&ask, now).await {
            Ok(batch) => batch,
            Err(_) => return Err(unreachable(ctx, site)),
        };
        let full = batch.len() >= ask.size as usize;
        let last = batch.last().map(|(id, _)| id.clone());
        for (id, bytes) in batch {
            if mine.iter().any(|(mine, _)| *mine == id) {
                continue;
            }
            let Some(stored) = read_kept(json, entity, bytes.as_deref(), now) else {
                continue;
            };
            if field(&stored.record, &keyed.field) != Some(value)
                || !crate::policy::permits(ctx, grants, &stored.record)
            {
                continue;
            }
            if let Some(pending) = pending(ctx, &entity.store) {
                pending
                    .loaded
                    .entry(jadpo_std::redis::record_key(&entity.keyspace, &id))
                    .or_insert(stored.revision);
            }
            found.push((id, stored.record));
        }
        if !full || want.is_some_and(|want| found.len() >= want) {
            break;
        }
        ask.after = last;
    }
    for (id, now) in mine {
        let Some(record) = now else {
            continue;
        };
        if field(&record, &keyed.field) == Some(value)
            && after.as_ref().is_none_or(|after| id > *after)
            && crate::policy::permits(ctx, grants, &record)
        {
            found.push((id, record));
        }
    }
    found.sort_by(|a, b| a.0.cmp(&b.0));
    if let Some(want) = want {
        found.truncate(want);
    }
    Ok(found)
}

/// A list or a page of the records holding `value` in `keyed`.
#[allow(clippy::too_many_arguments)]
async fn listed(
    ctx: &mut Ctx<'_>,
    plan: &ReadPlan,
    redis: &RedisRead,
    store: &dyn Records,
    json: &JsonCodecs<'_>,
    keyed: &jadpo_std::redis::KeyedField,
    value: &Value,
    args: &[Value],
    site: u32,
) -> Result<Found, Interrupt> {
    let size = |size: &Size, ctx: &mut Ctx<'_>| -> Result<usize, Interrupt> {
        match size {
            Size::Fixed(n) => Ok(usize::try_from(*n).unwrap_or(usize::MAX)),
            Size::From(source) => match crate::read::value_at(source, args) {
                Some(Value::Int(n)) => Ok(usize::try_from(n).unwrap_or(0)),
                _ => Err(defect(ctx, "a size that isn't a whole number", site)),
            },
        }
    };
    let entity = &redis.entity;
    match (&plan.page, plan.cardinality) {
        (Some(page), Cardinality::Page) => {
            let size = size(&page.size, ctx)?;
            let after = match crate::read::value_at(&page.after, args) {
                Some(Value::None) | None => None,
                Some(Value::Text(text)) => {
                    match crate::read::cursor::decode(&text, page.tag, &page.cursor).as_deref() {
                        Some([SqlValue::Uuid(id)]) => Some(uuid_text(id)),
                        _ => {
                            return Err(Interrupt::Failed(
                                ctx.fail(crate::read::invalid_cursor(), site),
                            ));
                        }
                    }
                }
                Some(_) => return Err(defect(ctx, "a page cursor that isn't text", site)),
            };
            let mut found = holding(
                ctx,
                store,
                json,
                entity,
                keyed,
                value,
                after,
                Some(size.saturating_add(1)),
                site,
            )
            .await?;
            let more = found.len() > size;
            found.truncate(size);
            let next = match (more, found.last()) {
                (true, Some((id, _))) => {
                    let bytes = match parse_uuid(id) {
                        Some(bytes) => bytes,
                        None => return Err(defect(ctx, "a record `id` that isn't one", site)),
                    };
                    match crate::read::cursor::encode(page.tag, &[SqlValue::Uuid(bytes)]) {
                        Some(text) => Value::text(&text),
                        None => return Err(defect(ctx, "a page cursor it couldn't write", site)),
                    }
                }
                _ => Value::None,
            };
            let items = found
                .into_iter()
                .map(|(_, record)| element(&redis.element, record))
                .collect();
            Ok(Found::Value(Value::record(
                crate::read::PAGE,
                vec![
                    (Rc::from("items"), Value::list(items)),
                    (Rc::from("next"), next),
                ],
            )))
        }
        // A read of one record finds the first, in creation order.
        (_, Cardinality::One | Cardinality::Optional) => {
            let found =
                holding(ctx, store, json, entity, keyed, value, None, Some(1), site).await?;
            Ok(match (found.into_iter().next(), plan.cardinality) {
                (Some((_, record)), _) => Found::Value(element(&redis.element, record)),
                (None, Cardinality::One) => Found::Missing(false),
                (None, _) => Found::Value(Value::None),
            })
        }
        _ => {
            let limit = match &redis.limit {
                Some(limit) => Some(size(limit, ctx)?),
                None => None,
            };
            let found = holding(ctx, store, json, entity, keyed, value, None, limit, site).await?;
            Ok(Found::Value(Value::list(
                found
                    .into_iter()
                    .map(|(_, record)| element(&redis.element, record))
                    .collect(),
            )))
        }
    }
}

/// How many records holding `value` in `keyed` the caller may read, as the
/// running action sees them.
async fn counted(
    ctx: &mut Ctx<'_>,
    store: &dyn Records,
    json: &JsonCodecs<'_>,
    entity: &RedisEntity,
    keyed: &jadpo_std::redis::KeyedField,
    value: &Value,
    site: u32,
) -> Result<u64, Interrupt> {
    let grants = &entity.policy.read;
    if !crate::policy::same_for_every_record(grants) {
        let found = holding(ctx, store, json, entity, keyed, value, None, None, site).await?;
        return Ok(found.len() as u64);
    }
    // Every record or none: anyone it lets in counts what the set holds.
    if !crate::policy::permits(ctx, grants, &Value::None) {
        return Ok(0);
    }
    let Some(digest) = digest(json, keyed.codec, value) else {
        return Ok(0);
    };
    let now = ctx.now();
    let expiry = jadpo_std::redis::expiry_key(&entity.keyspace, &keyed.born, &digest);
    let stored = match store.count(&expiry, now).await {
        Ok(count) => count,
        Err(_) => return Err(unreachable(ctx, site)),
    };
    // What the action wrote moves the count by what it changed.
    let holds = |record: Option<&Value>| {
        record.is_some_and(|record| field(record, &keyed.field) == Some(value))
    };
    let mut count = i64::try_from(stored).unwrap_or(i64::MAX);
    if let Some(pending) = pending(ctx, &entity.store) {
        for (_, touched) in &pending.touched {
            if touched.entity.keyspace != entity.keyspace {
                continue;
            }
            let was = holds(touched.before.as_ref().map(|stored| &stored.record));
            let is = holds(touched.now.as_ref());
            count += i64::from(is) - i64::from(was);
        }
    }
    Ok(u64::try_from(count.max(0)).unwrap_or(0))
}

/// A UUID's 16 bytes from its text.
fn parse_uuid(text: &str) -> Option<[u8; 16]> {
    let hex: String = text.chars().filter(|c| *c != '-').collect();
    if hex.len() != 32 {
        return None;
    }
    let mut out = [0u8; 16];
    for (at, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(hex.get(at * 2..at * 2 + 2)?, 16).ok()?;
    }
    Some(out)
}

// --- writes ---------------------------------------------------------------------------

/// The value `kept` gives with `inputs`, against `stored`, the record as
/// it is stored as the write is made.
fn kept_value_of(
    ctx: &mut Ctx<'_>,
    kept: &Kept,
    inputs: &[Value],
    stored: Option<&Value>,
    name: &str,
) -> Result<Value, String> {
    let stored_field = |name: &str| -> Result<Value, String> {
        stored
            .and_then(|record| field(record, name))
            .cloned()
            .ok_or_else(|| format!("an update of `{name}` with no record stored"))
    };
    let whole = |value: Value| -> Result<i64, String> {
        match value {
            Value::Int(n) => Ok(n),
            _ => Err("a counter of what isn't a whole number".to_string()),
        }
    };
    Ok(match kept {
        Kept::Input(at) => inputs
            .get(*at as usize)
            .cloned()
            .ok_or("a write without one of its inputs")?,
        Kept::Patched { input, field: from } => match inputs.get(*input as usize) {
            Some(Value::Record(patch)) if patch.supplied(from) => {
                patch.get(from).cloned().unwrap_or(Value::None)
            }
            Some(Value::Record(_)) => stored_field(name)?,
            _ => return Err("a patch that isn't a record".to_string()),
        },
        Kept::WhenSupplied {
            input,
            fields,
            value,
        } => match inputs.get(*input as usize) {
            Some(Value::Record(patch)) if fields.iter().any(|field| patch.supplied(field)) => {
                kept_value_of(ctx, value, inputs, stored, name)?
            }
            Some(Value::Record(_)) => stored_field(name)?,
            _ => return Err("a patch that isn't a record".to_string()),
        },
        Kept::NewId => match ctx.new_id() {
            Some(id) => Value::uuid(&uuid_text(&id)),
            None => return Err("a new identity on a host with no randomness and no seed".into()),
        },
        Kept::Now => Value::Instant(ctx.now()),
        Kept::Stored(from) => stored_field(from)?,
        Kept::Add(lhs, rhs) => {
            let lhs = whole(kept_value_of(ctx, lhs, inputs, stored, name)?)?;
            let rhs = whole(kept_value_of(ctx, rhs, inputs, stored, name)?)?;
            Value::Int(lhs.checked_add(rhs).ok_or("a counter past 64 bits")?)
        }
        Kept::Subtract(lhs, rhs) => {
            let lhs = whole(kept_value_of(ctx, lhs, inputs, stored, name)?)?;
            let rhs = whole(kept_value_of(ctx, rhs, inputs, stored, name)?)?;
            Value::Int(lhs.checked_sub(rhs).ok_or("a counter past 64 bits")?)
        }
        Kept::Negate(inner) => {
            let inner = whole(kept_value_of(ctx, inner, inputs, stored, name)?)?;
            Value::Int(inner.checked_neg().ok_or("a counter past 64 bits")?)
        }
    })
}

/// `record` with the fields `set` gives, worked out with `inputs`.
fn changed(
    ctx: &mut Ctx<'_>,
    write: &RedisWrite,
    record: &Value,
    inputs: &[Value],
) -> Result<Value, String> {
    let Value::Record(old) = record else {
        return Err("a stored record that isn't one".to_string());
    };
    let mut fields = old.fields.clone();
    for (name, kept) in &write.set {
        let value = kept_value_of(ctx, kept, inputs, Some(record), name)?;
        match fields.iter_mut().find(|(held, _)| held.as_ref() == name) {
            Some((_, slot)) => *slot = value,
            None => return Err(format!("a field `{name}` the record doesn't have")),
        }
    }
    Ok(Value::record(&old.ty, fields))
}

/// Runs `plan`, a write of an entity in Redis, with `inputs`, the values
/// its site worked out, from site `site`.
pub(crate) async fn write(
    ctx: &mut Ctx<'_>,
    plan: &WritePlan,
    redis: &RedisWrite,
    json: JsonCodecs<'_>,
    inputs: &[Value],
    site: u32,
) -> Result<Written, Interrupt> {
    let entity = &redis.entity;
    let store = store_of(ctx, entity, site)?;
    match plan.op {
        WriteOp::Create => {
            let mut fields: Vec<(Rc<str>, Value)> = Vec::with_capacity(redis.set.len());
            for (name, kept) in &redis.set {
                match kept_value_of(ctx, kept, inputs, None, name) {
                    Ok(value) => fields.push((Rc::from(name.as_str()), value)),
                    Err(what) => return Err(defect(ctx, &what, site)),
                }
            }
            let record = Value::record(&entity.entity, fields);
            if !crate::policy::permits(ctx, &entity.policy.create, &record) {
                return Ok(Written::NotPermitted);
            }
            let Some(id) = id_of(&record) else {
                return Err(defect(ctx, "a Redis record made without its `id`", site));
            };
            if let Some(refused) = taken(
                ctx,
                store.as_ref(),
                &json,
                plan,
                redis,
                None,
                &record,
                &id,
                site,
            )
            .await?
            {
                return Ok(refused);
            }
            let key = jadpo_std::redis::record_key(&entity.keyspace, &id);
            if let Some(pending) = pending(ctx, &entity.store) {
                pending.touched.push((
                    key,
                    Touched {
                        entity: Rc::new(entity.clone()),
                        id,
                        before: None,
                        held: true,
                        now: Some(record.clone()),
                        replay: Vec::new(),
                    },
                ));
            }
            Ok(Written::Value(record))
        }
        WriteOp::Update | WriteOp::Delete => {
            let target = redis.target.and_then(|at| inputs.get(at as usize)).cloned();
            let id = match (plan.by_id, &target) {
                (true, Some(Value::Uuid(id))) => id.to_string(),
                (false, Some(record)) => match id_of(record) {
                    Some(id) => id,
                    None => return Err(defect(ctx, "a write without the record it names", site)),
                },
                _ => return Err(defect(ctx, "a write without the record it names", site)),
            };
            let key = jadpo_std::redis::record_key(&entity.keyspace, &id);
            let now = ctx.now();
            // What the action sees of it: its own write, else the store's.
            let seen = match pending(ctx, &entity.store).and_then(|p| p.touched(&key).cloned()) {
                Some(touched) => Some((touched.now.clone(), touched.before.clone(), true)),
                None => None,
            };
            let (current, before) = match seen {
                Some((current, before, _)) => (current, before),
                None => {
                    let found = match store.get(std::slice::from_ref(&key), now).await {
                        Ok(found) => found,
                        Err(_) => return Err(unreachable(ctx, site)),
                    };
                    let stored =
                        read_kept(&json, entity, found.first().and_then(Option::as_deref), now);
                    (stored.as_ref().map(|s| s.record.clone()), stored)
                }
            };
            let loaded =
                pending(ctx, &entity.store).and_then(|pending| pending.loaded.get(&key).copied());
            let Some(current) = current else {
                if plan.by_id {
                    return Ok(Written::Missing(false));
                }
                // A record it loaded, gone since: another write came
                // between, so the action runs again.
                ctx.concurrent_change();
                return Err(defect(ctx, "a concurrent change", site));
            };
            let grants = if plan.op == WriteOp::Delete {
                &entity.policy.delete
            } else {
                &entity.policy.update
            };
            if !crate::policy::permits(ctx, grants, &current) {
                return Ok(if plan.by_id {
                    Written::Missing(true)
                } else {
                    Written::NotPermitted
                });
            }
            // A record loaded earlier that moved since is a concurrent
            // change now, as the commit would find it.
            if let (Some(loaded), Some(before)) = (loaded, &before)
                && loaded != before.revision
            {
                ctx.concurrent_change();
                return Err(defect(ctx, "a concurrent change", site));
            }
            let after = if plan.op == WriteOp::Delete {
                None
            } else {
                match changed(ctx, redis, &current, inputs) {
                    Ok(record) => Some(record),
                    Err(what) => return Err(defect(ctx, &what, site)),
                }
            };
            if let Some(record) = &after
                && let Some(refused) = taken(
                    ctx,
                    store.as_ref(),
                    &json,
                    plan,
                    redis,
                    Some(&current),
                    record,
                    &id,
                    site,
                )
                .await?
            {
                return Ok(refused);
            }
            // Held to what was read when the action read it, or wrote it
            // without an `id`; worked out again at the commit otherwise.
            let held = !plan.by_id || loaded.is_some();
            if let Some(pending) = pending(ctx, &entity.store) {
                match pending.touched_mut(&key) {
                    Some(touched) => {
                        touched.held |= held;
                        touched.now = after.clone();
                        if !touched.held {
                            touched.replay.push(replayed(plan, redis, inputs));
                        }
                    }
                    None => pending.touched.push((
                        key,
                        Touched {
                            entity: Rc::new(entity.clone()),
                            id,
                            before,
                            held,
                            now: after.clone(),
                            replay: if held {
                                Vec::new()
                            } else {
                                vec![replayed(plan, redis, inputs)]
                            },
                        },
                    )),
                }
            }
            Ok(Written::Value(after.unwrap_or(Value::Unit)))
        }
    }
}

/// What a write to a record the action didn't read is worked out again
/// from.
fn replayed(plan: &WritePlan, redis: &RedisWrite, inputs: &[Value]) -> Replayed {
    match plan.op {
        WriteOp::Delete => Replayed::Delete,
        _ => Replayed::Update(Rc::new(redis.clone()), inputs.to_vec()),
    }
}

/// The refusal a write of `record` meets when a unique value it takes is
/// another record's, as the running action sees it; `None` when every one
/// is free. `old` is the record before an update.
#[allow(clippy::too_many_arguments)]
async fn taken(
    ctx: &mut Ctx<'_>,
    store: &dyn Records,
    json: &JsonCodecs<'_>,
    plan: &WritePlan,
    redis: &RedisWrite,
    old: Option<&Value>,
    record: &Value,
    id: &str,
    site: u32,
) -> Result<Option<Written>, Interrupt> {
    let entity = &redis.entity;
    for (at, keyed) in entity.unique.iter().enumerate() {
        let Some(value) = field(record, &keyed.field) else {
            continue;
        };
        if matches!(value, Value::None)
            || old.and_then(|old| field(old, &keyed.field)) == Some(value)
        {
            continue;
        }
        if by_unique(ctx, store, json, entity, keyed, value, Some(id), site)
            .await?
            .is_none()
        {
            continue;
        }
        let Some((_, refusal)) = redis
            .conflicts
            .iter()
            .find(|(field, _)| *field as usize == at)
        else {
            return Err(refused(
                ctx,
                StoreError::Conflict {
                    constraint: format!("{}.{}", entity.entity, keyed.field),
                },
                site,
            ));
        };
        let constraint = plan
            .refusals
            .get(*refusal as usize)
            .map(|refusal| refusal.constraint.clone())
            .unwrap_or_default();
        let mut cause = Failure::new("runtime refused", "ConstraintRefused", "constraint_refused");
        cause.kind = Some("conflict".to_string());
        cause.status = Some(409);
        cause
            .internal
            .push(("constraint".to_string(), Value::text(&constraint)));
        let cause = ctx.fail(cause, site);
        return Ok(Some(Written::Refused {
            at: *refusal,
            cause,
        }));
    }
    Ok(None)
}

// --- the commit -----------------------------------------------------------------------

/// How an action's writes to one store ended as it committed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Committed {
    /// Made, every one.
    Made,
    /// A record it read moved, or a unique value it takes was taken,
    /// meanwhile: none was made, and the action runs again.
    Changed,
    /// The store couldn't be reached: none was made.
    Unavailable,
}

/// How many times a commit works the updates by `id` it holds out again
/// when what is stored moved under them, before the action runs again.
const RECOMMITS: u32 = 8;

/// The checks and changes `touched` comes to, its keys from what it held
/// before to what it holds now, each written expiring `expires_after:`
/// from `now`.
fn commit_of(json: &JsonCodecs<'_>, touched: &[(String, Touched)], now: i64) -> Option<Commit> {
    let mut commit = Commit::default();
    for (key, touched) in touched {
        let entity = &touched.entity;
        let ttl = entity.expires_ms;
        let expires = now.saturating_add(ttl);
        let base = touched.before.as_ref().map_or(0, |stored| stored.revision);
        commit.checks.push(Check::Revision {
            key: key.clone(),
            revision: base,
        });
        let old = touched.before.as_ref().map(|stored| &stored.record);
        let new = touched.now.as_ref();
        match new {
            Some(record) => {
                let text = written(json, entity, record)?;
                commit.changes.push(Change::Set {
                    key: key.clone(),
                    value: kept_value(base.saturating_add(1), expires, &text),
                    ttl_ms: ttl,
                });
            }
            None => commit.changes.push(Change::Delete { key: key.clone() }),
        }
        let records = jadpo_std::redis::record_prefix(&entity.keyspace);
        for keyed in &entity.unique {
            let was = old.and_then(|record| field(record, &keyed.field));
            let is = new.and_then(|record| field(record, &keyed.field));
            let was = was.and_then(|value| digest(json, keyed.codec, value));
            let is = is.and_then(|value| digest(json, keyed.codec, value));
            if let Some(was) = &was
                && is.as_ref() != Some(was)
            {
                commit.changes.push(Change::Delete {
                    key: jadpo_std::redis::unique_key(&entity.keyspace, &keyed.born, was),
                });
            }
            if let Some(is) = &is {
                let unique = jadpo_std::redis::unique_key(&entity.keyspace, &keyed.born, is);
                if was.as_ref() != Some(is) {
                    commit.checks.push(Check::Free {
                        key: unique.clone(),
                        id: touched.id.clone(),
                        records: records.clone(),
                    });
                }
                commit.changes.push(Change::Set {
                    key: unique,
                    value: touched.id.clone().into_bytes(),
                    ttl_ms: ttl,
                });
            }
        }
        for keyed in &entity.indexed {
            let was = old
                .and_then(|record| field(record, &keyed.field))
                .and_then(|value| digest(json, keyed.codec, value));
            let is = new
                .and_then(|record| field(record, &keyed.field))
                .and_then(|value| digest(json, keyed.codec, value));
            if let Some(was) = &was
                && is.as_ref() != Some(was)
            {
                commit.changes.push(Change::Remove {
                    set: jadpo_std::redis::index_key(&entity.keyspace, &keyed.born, was),
                    member: touched.id.clone(),
                });
                commit.changes.push(Change::Remove {
                    set: jadpo_std::redis::expiry_key(&entity.keyspace, &keyed.born, was),
                    member: touched.id.clone(),
                });
            }
            if let Some(is) = &is {
                let index = jadpo_std::redis::index_key(&entity.keyspace, &keyed.born, is);
                let expiry = jadpo_std::redis::expiry_key(&entity.keyspace, &keyed.born, is);
                commit.changes.push(Change::Add {
                    set: index.clone(),
                    score: 0,
                    member: touched.id.clone(),
                });
                commit.changes.push(Change::Add {
                    set: expiry.clone(),
                    score: expires,
                    member: touched.id.clone(),
                });
                commit.changes.push(Change::Expire {
                    key: index,
                    ttl_ms: ttl,
                });
                commit.changes.push(Change::Expire {
                    key: expiry,
                    ttl_ms: ttl,
                });
            }
        }
    }
    Some(commit)
}

/// Which touched record the check at `at` of [`commit_of`]'s commit asks
/// of, and whether it is its revision.
fn checked(touched: &[(String, Touched)], json: &JsonCodecs<'_>, at: u32) -> Option<(usize, bool)> {
    let mut seen = 0u32;
    for (place, (_, record)) in touched.iter().enumerate() {
        if seen == at {
            return Some((place, true));
        }
        seen += 1;
        let entity = &record.entity;
        for keyed in &entity.unique {
            let was = record
                .before
                .as_ref()
                .and_then(|stored| field(&stored.record, &keyed.field))
                .and_then(|value| digest(json, keyed.codec, value));
            let is = record
                .now
                .as_ref()
                .and_then(|record| field(record, &keyed.field))
                .and_then(|value| digest(json, keyed.codec, value));
            if is.is_some() && is != was {
                if seen == at {
                    return Some((place, false));
                }
                seen += 1;
            }
        }
    }
    None
}

/// Commits `pending`, the running action's writes to `store`, at `now`.
pub(crate) async fn commit(
    ctx: &mut Ctx<'_>,
    store: &str,
    mut pending: Pending,
    json: JsonCodecs<'_>,
) -> Committed {
    if !pending.writes() {
        return Committed::Made;
    }
    let Some(records) = ctx.host().records(store) else {
        return Committed::Unavailable;
    };
    let now = ctx.now();
    for _ in 0..RECOMMITS {
        let Some(asked) = commit_of(&json, &pending.touched, now) else {
            return Committed::Unavailable;
        };
        let failed = match records.commit(&asked, now).await {
            Ok(None) => return Committed::Made,
            Ok(Some(at)) => at,
            Err(_) => return Committed::Unavailable,
        };
        // An update by `id` that read nothing is worked out again against
        // what is stored now; anything else moved under what the action
        // read.
        let Some((place, true)) = checked(&pending.touched, &json, failed) else {
            return Committed::Changed;
        };
        let Some((key, touched)) = pending.touched.get(place).cloned() else {
            return Committed::Changed;
        };
        if touched.held {
            return Committed::Changed;
        }
        let found = match records.get(std::slice::from_ref(&key), now).await {
            Ok(found) => found,
            Err(_) => return Committed::Unavailable,
        };
        let Some(stored) = read_kept(
            &json,
            &touched.entity,
            found.first().and_then(Option::as_deref),
            now,
        ) else {
            return Committed::Changed;
        };
        let mut record = Some(stored.record.clone());
        for replayed in &touched.replay {
            record = match (replayed, record) {
                (Replayed::Update(write, inputs), Some(record)) => {
                    match changed(ctx, write, &record, inputs) {
                        Ok(next) => Some(next),
                        Err(_) => return Committed::Changed,
                    }
                }
                _ => None,
            };
        }
        if let Some((_, slot)) = pending.touched.get_mut(place) {
            slot.before = Some(stored);
            slot.now = record;
        }
    }
    Committed::Changed
}

/// The stores the running action wrote in Redis, taken from its boundary
/// as it ends, committed in name order: the first that doesn't commit
/// says how it ended.
pub(crate) async fn commit_all(
    ctx: &mut Ctx<'_>,
    pending: BTreeMap<String, Pending>,
    json: JsonCodecs<'_>,
) -> Committed {
    for (store, pending) in pending {
        match commit(ctx, &store, pending, json).await {
            Committed::Made => {}
            ended => return ended,
        }
    }
    Committed::Made
}

/// Every key `commit` names, in order, for a test to read.
pub fn keys_named(commit: &Commit) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for check in &commit.checks {
        match check {
            Check::Revision { key, .. } | Check::Free { key, .. } => {
                out.insert(key.clone());
            }
        }
    }
    for change in &commit.changes {
        match change {
            Change::Set { key, .. } | Change::Delete { key } | Change::Expire { key, .. } => {
                out.insert(key.clone());
            }
            Change::Add { set, .. } | Change::Remove { set, .. } => {
                out.insert(set.clone());
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run<T>(future: impl std::future::Future<Output = T>) -> T {
        crate::block_on(future).expect("records in memory never wait")
    }

    #[test]
    fn a_record_s_head_is_its_revision_and_expiry() {
        assert_eq!(head(&kept_value(3, 99, "{}")), Some((3, 99)));
        assert_eq!(head(b"{\"revision\":x"), None);
        assert_eq!(head(b"not a record"), None);
        assert_eq!(revision_at(Some(&kept_value(3, 99, "{}")), 98), 3);
        assert_eq!(
            revision_at(Some(&kept_value(3, 99, "{}")), 99),
            0,
            "expired"
        );
        assert_eq!(revision_at(None, 0), 0);
    }

    #[test]
    fn a_commit_makes_nothing_unless_every_check_holds() {
        let store = MemoryRecords::new();
        let set = |key: &str, value: &[u8]| Change::Set {
            key: key.to_string(),
            value: value.to_vec(),
            ttl_ms: 1_000,
        };
        let first = Commit {
            checks: vec![Check::Revision {
                key: "r".into(),
                revision: 0,
            }],
            changes: vec![set("r", &kept_value(1, 1_000, "{}")), set("u", b"a")],
        };
        assert_eq!(run(store.commit(&first, 0)), Ok(None));
        // The same again finds the record at revision 1, not 0.
        assert_eq!(run(store.commit(&first, 0)), Ok(Some(0)));
        let free = |id: &str| Check::Free {
            key: "u".into(),
            id: id.into(),
            records: "".into(),
        };
        let taking = Commit {
            checks: vec![free("b")],
            changes: vec![set("u", b"b")],
        };
        // `u` names `a`, whose record `a` there isn't: free.
        assert_eq!(run(store.commit(&taking, 0)), Ok(None));
        let again = Commit {
            checks: vec![free("c")],
            changes: vec![set("other", b"x")],
        };
        run(store.commit(
            &Commit {
                checks: Vec::new(),
                changes: vec![set("b", &kept_value(1, 1_000, "{}"))],
            },
            0,
        ))
        .expect("kept");
        assert_eq!(run(store.commit(&again, 0)), Ok(Some(0)), "`b` holds it");
        assert_eq!(store.text("other", 0), None, "nothing changed");
        // Once `b` expires, it is free.
        assert_eq!(run(store.commit(&again, 1_000)), Ok(None));
    }

    /// A store of records for a test's host: `counts`, in memory.
    struct KeptIn {
        records: MemoryRecords,
    }

    impl crate::Host for KeptIn {
        fn database(&self, _store: &str) -> Option<&dyn crate::Database> {
            None
        }

        fn random(&self, bytes: &mut [u8]) -> bool {
            bytes.fill(7);
            true
        }

        fn records(&self, _connection: &str) -> Option<Rc<dyn Records>> {
            Some(Rc::new(self.records.clone()))
        }
    }

    const ID: &str = "0192f0c4-3b5e-7c1a-9d2e-4f6a8b0c1d2e";

    /// The `Counter` entity, `{ id: Uuid, count: Int }`, kept in `counts`.
    fn counter() -> RedisEntity {
        RedisEntity {
            entity: "Counter".into(),
            store: "counts".into(),
            keyspace: "counter".into(),
            codec: 0,
            expires_ms: 60_000,
            unique: Vec::new(),
            indexed: Vec::new(),
            policy: jadpo_std::redis::RecordPolicy::default(),
        }
    }

    /// `Counter.update(id) { set: { count: Counter.count + n } }`.
    fn counting() -> WritePlan {
        WritePlan {
            store: "counts".into(),
            op: WriteOp::Update,
            sql: String::new(),
            unpoliced: None,
            policy: None,
            params: Vec::new(),
            returns: None,
            by_id: true,
            refusals: Vec::new(),
            guard: None,
            missing: None,
            kept_in_tenant: false,
            row: None,
            files: Vec::new(),
            audit: None,
            erased: Vec::new(),
            clears: Vec::new(),
            redis: Some(Box::new(RedisWrite {
                entity: counter(),
                target: Some(0),
                set: vec![(
                    "count".into(),
                    Kept::Add(
                        Box::new(Kept::Stored("count".into())),
                        Box::new(Kept::Input(1)),
                    ),
                )],
                conflicts: Vec::new(),
            })),
        }
    }

    /// `Counter.get(id)`.
    fn getting() -> ReadPlan {
        ReadPlan {
            store: "counts".into(),
            settings: Vec::new(),
            cardinality: Cardinality::One,
            select: jadpo_std::read::Select {
                sql: String::new(),
                unpoliced: None,
                permitted: None,
                params: Vec::new(),
                columns: Vec::new(),
                held: Vec::new(),
                record: jadpo_std::read::Record {
                    ty: "Counter".into(),
                    fields: Vec::new(),
                },
                includes: Vec::new(),
                versions: None,
                sensitive: None,
            },
            page: None,
            missing: None,
            recheck: None,
            layer: false,
            decided: None,
            gate: None,
            redis: Some(Box::new(RedisRead {
                entity: counter(),
                lookup: Lookup::Id(Given::Arg {
                    index: 0,
                    path: Vec::new(),
                }),
                element: Element::Record,
                limit: None,
            })),
        }
    }

    /// The test clock's time, 2000-01-01T00:00:00Z.
    const NOW: i64 = 946_684_800_000;

    /// What the counter holds at revision `revision`.
    fn stored(revision: u64, count: i64) -> Vec<u8> {
        kept_value(
            revision,
            NOW + 60_000,
            &format!("{{\"id\":\"{ID}\",\"count\":{count}}}"),
        )
    }

    /// An action counting one more, reading the counter first when
    /// `loads`; on its first run another server's count of 4 commits
    /// between its write and its commit.
    struct Counts<'j> {
        json: JsonCodecs<'j>,
        write: &'j WritePlan,
        read: &'j ReadPlan,
        loads: bool,
        records: MemoryRecords,
        runs: u32,
    }

    impl<'p> crate::ActionBody<'p> for Counts<'_> {
        fn isolation(&self) -> jadpo_std::Isolation {
            jadpo_std::Isolation::RepeatableRead
        }

        fn json(&self) -> JsonCodecs<'_> {
            self.json
        }

        fn run<'c>(&'c mut self, ctx: &'c mut Ctx<'p>) -> LocalFuture<'c, crate::Outcome> {
            Box::pin(async move {
                self.runs += 1;
                let id = [Value::uuid(ID)];
                if self.loads {
                    let read =
                        crate::store(ctx, crate::StoreOp::Read(self.read), &[], self.json, &id, 0);
                    if !matches!(read.await, crate::Stored::Value(_)) {
                        return crate::Outcome::Value(Value::text("not read"));
                    }
                }
                let written = crate::store(
                    ctx,
                    crate::StoreOp::Write(self.write),
                    &[],
                    self.json,
                    &[Value::uuid(ID), Value::Int(1)],
                    0,
                )
                .await;
                if self.runs == 1 {
                    let other = Commit {
                        checks: Vec::new(),
                        changes: vec![Change::Set {
                            key: jadpo_std::redis::record_key("counter", ID),
                            value: stored(2, 5),
                            ttl_ms: 60_000,
                        }],
                    };
                    let _ = self.records.commit(&other, NOW).await;
                }
                match written {
                    crate::Stored::Value(value) => crate::Outcome::Value(value),
                    _ => crate::Outcome::Value(Value::text("not written")),
                }
            })
        }
    }

    /// A counter's update by `id` that loaded nothing is worked out again
    /// against what another server committed meanwhile, without running
    /// its action again, so no count is lost; one that loaded the record
    /// first runs its action again, on what it reads then (chunk Q10).
    #[test]
    fn a_counter_is_worked_out_again_and_a_loaded_record_runs_its_action_again() {
        use jadpo_std::http::{Codec, CodecId};
        use jadpo_std::types::{FieldPlan, ShapeId, ShapePlan, ValidatorId};
        let field = |name: &str, validator: u32| FieldPlan {
            key: format!("field Counter.{name}"),
            name: name.into(),
            validator: ValidatorId(validator),
            absent: None,
            omittable: false,
            none_left_out: false,
            section: None,
            hashed: false,
            wire: None,
        };
        let shape = ShapePlan {
            ty: "Counter".into(),
            variant: None,
            fields: vec![field("id", 0), field("count", 1)],
            open: false,
        };
        let shapes = [jadpo_value::shape::Shape::new(&shape)];
        let validators = [
            jadpo_value::Validator::new(Some(jadpo_std::Builtin::Uuid), vec![]),
            jadpo_value::Validator::new(Some(jadpo_std::Builtin::Int), vec![]),
        ];
        let codecs = [
            Codec::Scalar {
                builtin: jadpo_std::Builtin::Uuid,
                validator: ValidatorId(0),
                int_as_number: false,
            },
            Codec::Scalar {
                builtin: jadpo_std::Builtin::Int,
                validator: ValidatorId(1),
                int_as_number: true,
            },
            Codec::Object {
                shape: ShapeId(0),
                fields: vec![CodecId(0), CodecId(1)],
                validator: None,
            },
        ];
        let types = [CodecId(2)];
        let json = JsonCodecs {
            codecs: jadpo_value::json::Codecs {
                codecs: &codecs,
                validators: &validators,
                shapes: &shapes,
                coder: None,
            },
            types: &types,
        };
        let (write, read) = (counting(), getting());
        for loads in [false, true] {
            let records = MemoryRecords::new();
            let first = Commit {
                checks: Vec::new(),
                changes: vec![Change::Set {
                    key: jadpo_std::redis::record_key("counter", ID),
                    value: stored(1, 1),
                    ttl_ms: 60_000,
                }],
            };
            run(records.commit(&first, NOW)).expect("kept");
            let host = KeptIn {
                records: records.clone(),
            };
            let places = crate::Places::new();
            let mut ctx = Ctx::new(&places, &host, crate::Env::default());
            ctx.set_unpoliced();
            let mut body = Counts {
                json,
                write: &write,
                read: &read,
                loads,
                records: records.clone(),
                runs: 0,
            };
            let outcome = run(crate::action(&mut ctx, &mut body));
            assert!(
                matches!(outcome, crate::Outcome::Value(Value::Record(_))),
                "{outcome:?}"
            );
            assert_eq!(body.runs, if loads { 2 } else { 1 }, "loads: {loads}");
            let kept = records.text(&jadpo_std::redis::record_key("counter", ID), NOW);
            assert_eq!(
                kept.as_deref(),
                Some(format!("{{\"revision\":3,\"expires\":{},\"record\":{{\"id\":\"{ID}\",\"count\":6}}}}", NOW + 60_000).as_str()),
                "loads: {loads}"
            );
        }
    }

    #[test]
    fn a_page_reads_ids_in_order_and_leaves_out_what_expired() {
        let store = MemoryRecords::new();
        let add = |set: &str, score: i64, member: &str| Change::Add {
            set: set.to_string(),
            score,
            member: member.to_string(),
        };
        run(store.commit(
            &Commit {
                checks: Vec::new(),
                changes: vec![
                    add("i", 0, "b"),
                    add("i", 0, "a"),
                    add("i", 0, "c"),
                    add("e", 50, "a"),
                    add("e", 500, "b"),
                    add("e", 500, "c"),
                ],
            },
            0,
        ))
        .expect("kept");
        let ask = PageAsk {
            index: "i".into(),
            expiry: "e".into(),
            records: "r:".into(),
            after: None,
            size: 2,
        };
        let ids = |found: Vec<(String, Option<Vec<u8>>)>| -> Vec<String> {
            found.into_iter().map(|(id, _)| id).collect()
        };
        assert_eq!(ids(run(store.page(&ask, 0)).expect("page")), ["a", "b"]);
        assert_eq!(run(store.count("e", 0)), Ok(3));
        assert_eq!(ids(run(store.page(&ask, 100)).expect("page")), ["b", "c"]);
        assert_eq!(run(store.count("e", 100)), Ok(2));
        let after = PageAsk {
            after: Some("b".into()),
            ..ask
        };
        assert_eq!(ids(run(store.page(&after, 100)).expect("page")), ["c"]);
    }
}
