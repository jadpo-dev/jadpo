//! Caches (area 5, "Caches"; area 6, "Where a read comes from"; area 7,
//! "Write revisions"; chunks Q9 and Q9b): a route's display read of a
//! query with `cache:` is served from its cache, one of a record by `id`
//! from its entity's record cache, and every committed change clears what
//! it could have made stale.
//!
//! - **What an entry is kept under.** The query's key and its read plan's
//!   digest ([`RouteCache::key`]), every argument the route passes it and
//!   every part of the caller its policy can bind, hashed with BLAKE3, so
//!   one caller's results are never another's and no argument is kept as
//!   written. An argument holding a secret is never hashed: that read goes
//!   to the store.
//! - **One JSON value an entry**: the generations it was read under, when
//!   its read began, and the answer as the API sends it, written and read
//!   back by the route's output codec.
//! - **A generation for each entity, and a list of the entries that read
//!   it.** A read asks for the generations of the entities its statements
//!   read and for its entry in one ask; an entry read under other
//!   generations is a miss. A miss reads the store, keeps the answer, and
//!   asks the generations again: one that moved since its read began means
//!   a change committed meanwhile, and the entry is deleted. A clear adds
//!   one to each generation and deletes every entry each list names, so a
//!   change leaves no copy behind (area 15, "What erasure reaches").
//! - **A record cache** ([`records`]): each record is kept whole, but for
//!   the fields it leaves out, under a key worked out from its entity and
//!   its `id`, one copy for every caller, listed under its entity alone.
//!   Each time a copy is read it is held to the entity's `read:` grants in
//!   Jadpo's own code ([`crate::policy::permits`]), as a record in Redis
//!   is, so a caller policy doesn't let in finds nothing, then becomes what
//!   the query gives. A miss reads the record without policy, as
//!   `visible:` lets reads see it, and keeps it.
//! - **A revision** ([`revision_of`]): a request that carries one skips an
//!   entry whose read began before it, and keeps what it reads instead.
//! - **A cache that can't answer never fails a request.** A read goes to
//!   the store, and the server leaves that cache alone for
//!   [`STORE_PAUSE_MS`]; a clear that fails is kept ([`CacheHealth`]) and
//!   tried again after the response and before the server's next use of
//!   the cache, and until it succeeds the server reads the store for every
//!   entry it would clear. Each outage is logged once, as
//!   `caches.fallback`.
//!
//! [`Cache`] is a store's side of it: a Redis connection's driver, or
//! [`MemoryCache`], what `Driver.memory`, `jadpo dev` and every test keep a
//! cache in, which behaves as Redis does.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::rc::Rc;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use jadpo_std::caches::{Clear, RecordRead, RouteCache};
use jadpo_std::http::RoutePlan;
use jadpo_std::read::{Cardinality, ReadPlan};
use jadpo_std::types::FailurePlan;
use jadpo_value::Value;

use crate::http::{App, Request};
use crate::ratelimit::{STORE_PAUSE_MS, Unasked};
use crate::read::JsonCodecs;
use crate::{Ctx, FaultKind, Frame, Host, LocalFuture, Outcome, StoreOp, Stored};

/// What a store said of an entry and the generations it was asked for.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Looked {
    /// Each entity's generation, in the order asked; 0 for one never
    /// cleared.
    pub generations: Vec<u64>,
    /// The entry's bytes, when it is there.
    pub entry: Option<Vec<u8>>,
}

/// A cache's store (area 11, the `cache` capability): a Redis
/// connection's driver, or [`MemoryCache`].
pub trait Cache {
    /// The generations of `tags` and the entry under `key`, in one ask, at
    /// `now` (milliseconds since the epoch, which only a store in memory
    /// reads its expiry by).
    fn look<'a>(
        &'a self,
        key: &'a str,
        tags: &'a [String],
        now: i64,
    ) -> LocalFuture<'a, Result<Looked, Unasked>>;

    /// Keeps `entry` under `key` for `keep_ms`, listed under each of
    /// `tags`, then gives the generations of `tags` again.
    fn keep<'a>(
        &'a self,
        key: &'a str,
        entry: &'a [u8],
        keep_ms: u64,
        tags: &'a [String],
        now: i64,
    ) -> LocalFuture<'a, Result<Vec<u64>, Unasked>>;

    /// Deletes the entry under `key`.
    fn forget<'a>(&'a self, key: &'a str) -> LocalFuture<'a, Result<(), Unasked>>;

    /// Clears every entry that reads any of `tags`: one more on each
    /// one's generation, and each entry its list names deleted.
    fn clear<'a>(&'a self, tags: &'a [String], now: i64) -> LocalFuture<'a, Result<(), Unasked>>;
}

/// One entry kept in memory, and when it expires.
#[derive(Clone, Debug)]
struct Kept {
    bytes: Vec<u8>,
    expires: i64,
}

/// What a [`MemoryCache`] holds.
#[derive(Debug, Default)]
struct Memory {
    entries: HashMap<String, Kept>,
    generations: HashMap<String, u64>,
    /// Each entity's entries, and when the list expires.
    listings: HashMap<String, (BTreeSet<String>, i64)>,
}

impl Memory {
    fn sweep(&mut self, now: i64) {
        self.entries.retain(|_, kept| kept.expires > now);
        self.listings.retain(|_, (_, expires)| *expires > now);
    }

    fn generations(&self, tags: &[String]) -> Vec<u64> {
        tags.iter()
            .map(|tag| self.generations.get(tag).copied().unwrap_or(0))
            .collect()
    }
}

/// A cache in this process's memory, which behaves as a Redis store does:
/// `Driver.memory`'s, one per connection for all of a server's workers
/// ([`MemoryCache::process`]), and `jadpo dev`'s; and a test's, one of its
/// own per test, emptied between tests (area 13, "Events and caches in
/// tests").
#[derive(Clone, Debug, Default)]
pub struct MemoryCache {
    memory: Arc<Mutex<Memory>>,
}

impl MemoryCache {
    /// An empty cache of its own.
    pub fn new() -> MemoryCache {
        MemoryCache::default()
    }

    /// Connection `connection`'s cache in this process, which every worker
    /// thread shares: each server's own memory (area 16, "`Driver.memory`").
    pub fn process(connection: &str) -> MemoryCache {
        static CACHES: OnceLock<Mutex<BTreeMap<String, MemoryCache>>> = OnceLock::new();
        let caches = CACHES.get_or_init(Mutex::default);
        let mut caches = caches.lock().unwrap_or_else(PoisonError::into_inner);
        caches.entry(connection.to_string()).or_default().clone()
    }

    fn memory(&self) -> std::sync::MutexGuard<'_, Memory> {
        self.memory.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// How many entries it holds at `now`: what a test reads to see that a
    /// change left none behind.
    pub fn entries(&self, now: i64) -> usize {
        let mut memory = self.memory();
        memory.sweep(now);
        memory.entries.len()
    }

    /// Each entry's bytes it holds at `now`, as text.
    pub fn texts(&self, now: i64) -> Vec<String> {
        let mut memory = self.memory();
        memory.sweep(now);
        let mut out: Vec<String> = memory
            .entries
            .values()
            .map(|kept| String::from_utf8_lossy(&kept.bytes).into_owned())
            .collect();
        out.sort();
        out
    }
}

impl Cache for MemoryCache {
    fn look<'a>(
        &'a self,
        key: &'a str,
        tags: &'a [String],
        now: i64,
    ) -> LocalFuture<'a, Result<Looked, Unasked>> {
        let mut memory = self.memory();
        memory.sweep(now);
        let looked = Looked {
            generations: memory.generations(tags),
            entry: memory.entries.get(key).map(|kept| kept.bytes.clone()),
        };
        Box::pin(async move { Ok(looked) })
    }

    fn keep<'a>(
        &'a self,
        key: &'a str,
        entry: &'a [u8],
        keep_ms: u64,
        tags: &'a [String],
        now: i64,
    ) -> LocalFuture<'a, Result<Vec<u64>, Unasked>> {
        let mut memory = self.memory();
        memory.sweep(now);
        let expires = now.saturating_add(i64::try_from(keep_ms).unwrap_or(i64::MAX));
        memory.entries.insert(
            key.to_string(),
            Kept {
                bytes: entry.to_vec(),
                expires,
            },
        );
        let listed = now
            .saturating_add(i64::try_from(jadpo_std::caches::LISTING_KEEP_MS).unwrap_or(i64::MAX));
        for tag in tags {
            let (keys, expires) = memory.listings.entry(tag.clone()).or_default();
            keys.insert(key.to_string());
            *expires = listed;
        }
        let generations = memory.generations(tags);
        Box::pin(async move { Ok(generations) })
    }

    fn forget<'a>(&'a self, key: &'a str) -> LocalFuture<'a, Result<(), Unasked>> {
        self.memory().entries.remove(key);
        Box::pin(async { Ok(()) })
    }

    fn clear<'a>(&'a self, tags: &'a [String], now: i64) -> LocalFuture<'a, Result<(), Unasked>> {
        let mut memory = self.memory();
        memory.sweep(now);
        for tag in tags {
            *memory.generations.entry(tag.clone()).or_default() += 1;
            if let Some((keys, _)) = memory.listings.remove(tag) {
                for key in keys {
                    memory.entries.remove(&key);
                }
            }
        }
        Box::pin(async { Ok(()) })
    }
}

/// Whether each cache is being asked, and the clears each owes, as one
/// server, or one Worker's instance, sees it: shared by a server's
/// workers.
#[derive(Debug, Default)]
pub struct CacheHealth {
    stores: Mutex<BTreeMap<String, Health>>,
}

/// One cache's health.
#[derive(Clone, Debug, Default)]
struct Health {
    /// Until when, on the monotonic clock, it isn't asked.
    paused_until: Option<u64>,
    /// Whether its fallback has been logged since it last answered.
    logged: bool,
    /// The entities a clear that failed owes: until it is cleared, no
    /// entry reading one is served by this server.
    owed: BTreeSet<String>,
}

impl CacheHealth {
    /// This process's: every native worker's, and a Worker's instance's.
    pub fn process() -> &'static CacheHealth {
        static HEALTH: OnceLock<CacheHealth> = OnceLock::new();
        HEALTH.get_or_init(CacheHealth::default)
    }

    fn stores(&self) -> std::sync::MutexGuard<'_, BTreeMap<String, Health>> {
        self.stores.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Whether `store` is asked at `now`, the monotonic clock's
    /// milliseconds: always on a host with no such clock.
    fn asks(&self, store: &str, now: Option<u64>) -> bool {
        match (
            self.stores()
                .get(store)
                .and_then(|health| health.paused_until),
            now,
        ) {
            (Some(until), Some(now)) => now >= until,
            _ => true,
        }
    }

    /// What `store` owes from clears that failed.
    fn owed(&self, store: &str) -> Vec<String> {
        self.stores()
            .get(store)
            .map(|health| health.owed.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Whether `store` owes a clear of any of `tags`.
    fn owes_any(&self, store: &str, tags: &[String]) -> bool {
        self.stores()
            .get(store)
            .is_some_and(|health| tags.iter().any(|tag| health.owed.contains(tag)))
    }

    /// `store` answered: asked from now on, its next fallback logged, and
    /// `cleared` no longer owed.
    fn answered(&self, store: &str, cleared: &[String]) {
        let mut stores = self.stores();
        if let Some(health) = stores.get_mut(store) {
            health.paused_until = None;
            health.logged = false;
            for tag in cleared {
                health.owed.remove(tag);
            }
        }
    }

    /// `store`, left alone for now, owes a clear of `owed` too.
    fn owe(&self, store: &str, owed: &[String]) {
        self.stores()
            .entry(store.to_string())
            .or_default()
            .owed
            .extend(owed.iter().cloned());
    }

    /// `store` couldn't answer at `now`, owing a clear of `owed`: not
    /// asked again for [`STORE_PAUSE_MS`]. Whether this is the first time
    /// since it last answered, which is logged.
    fn failed(&self, store: &str, now: Option<u64>, owed: &[String]) -> bool {
        let mut stores = self.stores();
        let health = stores.entry(store.to_string()).or_default();
        health.paused_until = now.map(|now| now.saturating_add(STORE_PAUSE_MS));
        health.owed.extend(owed.iter().cloned());
        !std::mem::replace(&mut health.logged, true)
    }
}

/// The `caches.fallback` event's fields for cache `connection`, which
/// couldn't answer for `why`: never a key.
pub fn fallback(connection: &str, why: Unasked) -> serde_json::Map<String, serde_json::Value> {
    let said = match why {
        Unasked::Unreachable => format!("`{connection}` couldn't be reached"),
        Unasked::TimedOut => format!("`{connection}` didn't answer in time"),
        Unasked::Failed => format!("`{connection}` answered with something other than a cache's"),
    };
    let mut fields = serde_json::Map::new();
    fields.insert("connection".into(), connection.into());
    fields.insert("reason".into(), why.word().into());
    fields.insert(
        "message".into(),
        format!(
            "{said}: its reads go to the database, and a clear it missed is tried again, asking it again in {} seconds.",
            STORE_PAUSE_MS / 1000
        )
        .into(),
    );
    fields
}

/// The revision a request carries (area 7, "Write revisions"): its
/// `Jadpo-Revision` header, else its cookie; the later when it has both.
pub fn revision_of(request: &Request) -> Option<i64> {
    let header = request
        .header(jadpo_std::caches::REVISION_HEADER)
        .and_then(|value| value.trim().parse::<i64>().ok());
    let cookie = crate::http::identity::cookie_values(request, jadpo_std::caches::REVISION_COOKIE)
        .into_iter()
        .filter_map(|value| value.parse::<i64>().ok())
        .max();
    header.max(cookie)
}

/// The headers a write's response carries its revision, `revision`, in:
/// `Jadpo-Revision`, and a cookie lasting `keep_ms`, the longest `keep:`,
/// after which no entry it could skip is left.
pub fn revision_headers(revision: i64, keep_ms: u64) -> Vec<(String, String)> {
    let age = keep_ms.div_ceil(1000);
    vec![
        (
            jadpo_std::caches::REVISION_HEADER.to_string(),
            revision.to_string(),
        ),
        (
            "set-cookie".to_string(),
            format!(
                "{}={revision}; Path=/; Max-Age={age}; HttpOnly; Secure; SameSite=Strict",
                jadpo_std::caches::REVISION_COOKIE
            ),
        ),
    ]
}

/// The key `route`'s entry for `args` and the caller in `ctx` is kept
/// under; `None` when an argument holds a secret, which is never hashed.
fn entry_key(ctx: &Ctx<'_>, cache: &RouteCache, args: &[Value]) -> Option<String> {
    let mut hasher = blake3::Hasher::new();
    hasher.update(cache.key.as_bytes());
    hasher.update(&[0]);
    for arg in args {
        hashed(&mut hasher, arg)?;
    }
    hasher.update(format!("{:?}", crate::read::callers(ctx)).as_bytes());
    hasher.update(&[u8::from(ctx.unpoliced())]);
    let digest = hasher.finalize().to_hex();
    Some(jadpo_std::caches::entry_key(
        digest.get(..32).unwrap_or(digest.as_str()),
    ))
}

/// Feeds `value` into `hasher`, each part tagged with what it is; `None`
/// for a secret or a key.
fn hashed(hasher: &mut blake3::Hasher, value: &Value) -> Option<()> {
    fn word(hasher: &mut blake3::Hasher, tag: u8, bytes: &[u8]) {
        hasher.update(&[tag]);
        hasher.update(&(bytes.len() as u64).to_le_bytes());
        hasher.update(bytes);
    }
    match value {
        Value::Secret(_) | Value::Key(_) => return None,
        Value::Text(text) | Value::Uuid(text) => word(hasher, 1, text.as_bytes()),
        Value::Bytes(bytes) => word(hasher, 2, bytes),
        Value::List(items) | Value::Set(items) => {
            word(hasher, 3, &(items.len() as u64).to_le_bytes());
            for item in items.iter() {
                hashed(hasher, item)?;
            }
        }
        Value::Map(map) => {
            word(hasher, 4, &(map.len() as u64).to_le_bytes());
            for (key, item) in map.iter() {
                hashed(hasher, key)?;
                hashed(hasher, item)?;
            }
        }
        Value::Record(record) => {
            word(hasher, 5, record.ty.as_bytes());
            for (name, field) in &record.fields {
                word(hasher, 6, name.as_bytes());
                hashed(hasher, field)?;
            }
        }
        Value::Variant(variant) => {
            word(hasher, 7, variant.ty.as_bytes());
            word(hasher, 8, variant.name.as_bytes());
            for (name, field) in &variant.fields {
                word(hasher, 6, name.as_bytes());
                hashed(hasher, field)?;
            }
        }
        // Everything else holds no other value and no secret: its
        // debugging form says all of it.
        other => word(hasher, 9, format!("{other:?}").as_bytes()),
    }
    Some(())
}

/// One entry as it is kept: the generations its read began under, when it
/// began, and the answer as the API sends it, written by `write`.
fn entry(generations: &[u64], filled: i64, write: impl FnOnce(&mut String)) -> Vec<u8> {
    let mut out = String::from("{\"generations\":[");
    for (at, generation) in generations.iter().enumerate() {
        if at > 0 {
            out.push(',');
        }
        out.push_str(&generation.to_string());
    }
    out.push_str("],\"filled\":");
    out.push_str(&filled.to_string());
    out.push_str(",\"value\":");
    write(&mut out);
    out.push('}');
    out.into_bytes()
}

/// An entry read back: its generations, when its read began, and its
/// answer, read by `read`; `None` when it doesn't read.
fn read_entry(
    bytes: &[u8],
    read: impl FnOnce(&serde_json::Value, &jadpo_value::json::Numbers) -> Option<Value>,
) -> Option<(Vec<u64>, i64, Value)> {
    let (json, numbers) = jadpo_value::json::parse(bytes)?;
    let generations = json
        .get("generations")?
        .as_array()?
        .iter()
        .map(serde_json::Value::as_u64)
        .collect::<Option<Vec<u64>>>()?;
    let filled = json.get("filled")?.as_i64()?;
    let value = read(json.get("value")?, &numbers.within(&["value".to_string()]))?;
    Some((generations, filled, value))
}

/// The milliseconds a cache's entries count their age and expiry by: the
/// host's wall clock, or a test's.
fn now(ctx: &Ctx<'_>) -> i64 {
    crate::steps::wall(ctx)
}

/// What a route's display read of `plan` gives, served from `cache` when
/// it holds an entry this request may read (area 6, "Where a read comes
/// from"), and kept there when it didn't. `revision` is the one the
/// request carries.
pub(crate) async fn read(
    ctx: &mut Ctx<'_>,
    app: &App<'_>,
    route: &RoutePlan,
    (cache, plan): (&RouteCache, &ReadPlan),
    args: &[Value],
    revision: Option<i64>,
) -> Outcome {
    let store = ctx.host().cache(&cache.store);
    let health = ctx.host().cache_health();
    let (Some(store), Some(codec), Some(key)) = (store, route.output, entry_key(ctx, cache, args))
    else {
        return queried(ctx, app, plan, args).await;
    };
    let monotonic = ctx.host().monotonic_ms();
    if !owed_cleared(ctx, &cache.store, store.as_ref(), health).await
        || health.owes_any(&cache.store, &cache.tags)
    {
        return queried(ctx, app, plan, args).await;
    }
    let began = now(ctx);
    let looked = match store.look(&key, &cache.tags, began).await {
        Ok(looked) => looked,
        Err(why) => {
            fell_back(ctx, &cache.store, why, health, monotonic, &[]);
            return queried(ctx, app, plan, args).await;
        }
    };
    if let Some(bytes) = &looked.entry {
        let found = read_entry(bytes, |json, numbers| {
            let mut problems = Vec::new();
            let path = jadpo_value::json::Path::default().numbered(numbers);
            app.codecs.read(codec, json, &path, &mut problems)
        });
        match found {
            Some((generations, filled, value))
                if generations == looked.generations
                    && revision.is_none_or(|revision| filled > revision) =>
            {
                health.answered(&cache.store, &[]);
                return Outcome::Value(value);
            }
            _ => {}
        }
    }
    let outcome = queried(ctx, app, plan, args).await;
    if let Outcome::Value(value) = &outcome {
        let kept = entry(&looked.generations, began, |out| {
            app.codecs.write(codec, value, out);
        });
        match store
            .keep(&key, &kept, cache.keep_ms, &cache.tags, began)
            .await
        {
            // A change committed while it read: what it kept may be
            // older than that change, so it isn't kept.
            Ok(after) if after != looked.generations => {
                if let Err(why) = store.forget(&key).await {
                    fell_back(ctx, &cache.store, why, health, monotonic, &cache.tags);
                }
            }
            Ok(_) => health.answered(&cache.store, &[]),
            Err(why) => fell_back(ctx, &cache.store, why, health, monotonic, &cache.tags),
        }
    }
    outcome
}

/// What reading `plan` from its store gives.
async fn queried(ctx: &mut Ctx<'_>, app: &App<'_>, plan: &ReadPlan, args: &[Value]) -> Outcome {
    match crate::store(
        ctx,
        StoreOp::Read(plan),
        app.failures,
        app.json,
        args,
        Frame::ENTRY,
    )
    .await
    {
        Stored::Value(value) => Outcome::Value(value),
        Stored::Failed(failure) => Outcome::Failed(failure),
        Stored::Conflict { cause, .. } => Outcome::Failed(cause),
        Stored::Fault(fault) => Outcome::Fault(fault),
    }
}

/// What a route's display read of one record by `id` gives through its
/// entity's record cache (area 5, "A record cache"; chunk Q9b): the copy
/// `cache` keeps, or the record read and kept there, held to the entity's
/// `read:` grants each time, then what the query gives. `revision` is the
/// one the request carries.
pub(crate) async fn records(
    ctx: &mut Ctx<'_>,
    app: &App<'_>,
    read: (&RouteCache, &RecordRead, &ReadPlan),
    args: &[Value],
    revision: Option<i64>,
) -> Outcome {
    record_read(ctx, (app.failures, app.json), read, args, revision).await
}

/// [`records`], with the program's failures and JSON codecs.
async fn record_read(
    ctx: &mut Ctx<'_>,
    (failures, json): (&[FailurePlan], JsonCodecs<'_>),
    (cache, records, plan): (&RouteCache, &RecordRead, &ReadPlan),
    args: &[Value],
    revision: Option<i64>,
) -> Outcome {
    // A listed query's line, checked as it starts, as its statement would
    // be (area 9, "Listing a query"): a copy is every caller's.
    if let Some(gate) = &plan.gate {
        match crate::admit(ctx, gate, Frame::ENTRY).await {
            Stored::Value(_) => {}
            refused => return outcome(refused),
        }
    }
    let found = match crate::redis::given_value(ctx, &records.id, args) {
        Some(Value::Uuid(id)) => {
            match kept_record(ctx, (failures, json), cache, records, &id, revision).await {
                Ok(found) => found,
                Err(stopped) => return stopped,
            }
        }
        Some(Value::None) => None,
        _ => {
            return Outcome::Fault(ctx.fault(
                FaultKind::Defect("a cached record looked up by what isn't an `id`".into()),
                Frame::ENTRY,
            ));
        }
    };
    // Policy on the copy, by what the record holds: one the caller may not
    // read is not found, as policy hides a record in the store.
    let (found, concealed) = match found {
        Some(record) if crate::policy::permits(ctx, &records.grants, &record) => {
            (Some(crate::redis::element(&records.element, record)), false)
        }
        Some(_) => (None, true),
        None => (None, false),
    };
    match (plan.cardinality, found) {
        (Cardinality::List | Cardinality::Page, _) => Outcome::Fault(ctx.fault(
            FaultKind::Defect("a record cache serving a read of many records".into()),
            Frame::ENTRY,
        )),
        (_, Some(value)) => Outcome::Value(value),
        (Cardinality::Optional, None) => Outcome::Value(Value::None),
        (Cardinality::One, None) => outcome(crate::store::not_found(
            ctx,
            plan,
            failures,
            args,
            concealed,
            Frame::ENTRY,
        )),
    }
}

/// What a store operation's end is as a route's.
fn outcome(stored: Stored) -> Outcome {
    match stored {
        Stored::Value(value) => Outcome::Value(value),
        Stored::Failed(failure) => Outcome::Failed(failure),
        Stored::Conflict { cause, .. } => Outcome::Failed(cause),
        Stored::Fault(fault) => Outcome::Fault(fault),
    }
}

/// Record `id` of `records`' entity, as `cache` keeps a copy of it, or as
/// its store holds it, kept once read; `None` when there is none to read.
async fn kept_record(
    ctx: &mut Ctx<'_>,
    (failures, json): (&[FailurePlan], JsonCodecs<'_>),
    cache: &RouteCache,
    records: &RecordRead,
    id: &str,
    revision: Option<i64>,
) -> Result<Option<Value>, Outcome> {
    let Some(store) = ctx.host().cache(&cache.store) else {
        return filled(ctx, (failures, json), records, id).await;
    };
    let health = ctx.host().cache_health();
    let monotonic = ctx.host().monotonic_ms();
    if !owed_cleared(ctx, &cache.store, store.as_ref(), health).await
        || health.owes_any(&cache.store, &cache.tags)
    {
        return filled(ctx, (failures, json), records, id).await;
    }
    let key = record_key(&cache.key, id);
    let began = now(ctx);
    let looked = match store.look(&key, &cache.tags, began).await {
        Ok(looked) => looked,
        Err(why) => {
            fell_back(ctx, &cache.store, why, health, monotonic, &[]);
            return filled(ctx, (failures, json), records, id).await;
        }
    };
    if let Some(bytes) = &looked.entry
        && let Some((generations, filled_at, record)) = read_entry(bytes, |value, numbers| {
            read_record(&json, records, value, numbers)
        })
        && generations == looked.generations
        && revision.is_none_or(|revision| filled_at > revision)
    {
        health.answered(&cache.store, &[]);
        return Ok(Some(record));
    }
    let found = filled(ctx, (failures, json), records, id).await?;
    if let Some(text) = found
        .as_ref()
        .and_then(|record| written_record(&json, records, record))
    {
        let kept = entry(&looked.generations, began, |out| out.push_str(&text));
        match store
            .keep(&key, &kept, cache.keep_ms, &cache.tags, began)
            .await
        {
            // A change committed while it read: what it kept may be older
            // than that change, so it isn't kept.
            Ok(after) if after != looked.generations => {
                if let Err(why) = store.forget(&key).await {
                    fell_back(ctx, &cache.store, why, health, monotonic, &cache.tags);
                }
            }
            Ok(_) => health.answered(&cache.store, &[]),
            Err(why) => fell_back(ctx, &cache.store, why, health, monotonic, &cache.tags),
        }
    }
    Ok(found)
}

/// Record `id` as its store holds it, read with no policy, which each read
/// of a copy applies.
async fn filled(
    ctx: &mut Ctx<'_>,
    (failures, json): (&[FailurePlan], JsonCodecs<'_>),
    records: &RecordRead,
    id: &str,
) -> Result<Option<Value>, Outcome> {
    let stored = crate::store(
        ctx,
        StoreOp::Read(&records.fill),
        failures,
        json,
        &[Value::uuid(id)],
        Frame::ENTRY,
    )
    .await;
    match stored {
        Stored::Value(Value::None) => Ok(None),
        Stored::Value(record) => Ok(Some(record)),
        other => Err(outcome(other)),
    }
}

/// The key a copy of record `id` is kept under, from `key`, its record
/// cache's: never the `id` as written.
fn record_key(key: &str, id: &str) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(key.as_bytes());
    hasher.update(&[0]);
    hasher.update(id.as_bytes());
    let digest = hasher.finalize().to_hex();
    jadpo_std::caches::entry_key(digest.get(..32).unwrap_or(digest.as_str()))
}

/// A copy of `record` as it is kept: each field `records` keeps, under the
/// name it was born with, as its codec writes it; `None` for a record
/// that doesn't hold them all.
fn written_record(json: &JsonCodecs<'_>, records: &RecordRead, record: &Value) -> Option<String> {
    let Value::Record(record) = record else {
        return None;
    };
    let mut out = String::from("{");
    for (at, kept) in records.fields.iter().enumerate() {
        let codec = json.codec_of(kept.codec)?;
        let value = record.get(&kept.field)?;
        if at > 0 {
            out.push(',');
        }
        out.push_str(&serde_json::to_string(&kept.born).ok()?);
        out.push(':');
        json.codecs.write(codec, value, &mut out);
    }
    out.push('}');
    Some(out)
}

/// A copy read back as a record of `records`' entity; `None` for one this
/// build can't read, which is then read from the store again.
fn read_record(
    json: &JsonCodecs<'_>,
    records: &RecordRead,
    value: &serde_json::Value,
    numbers: &jadpo_value::json::Numbers,
) -> Option<Value> {
    let members = value.as_object()?;
    let mut fields = Vec::with_capacity(records.fields.len());
    for kept in &records.fields {
        let codec = json.codec_of(kept.codec)?;
        let member = members.get(&kept.born)?;
        let path = jadpo_value::json::Path::default()
            .numbered(&numbers.within(std::slice::from_ref(&kept.born)));
        let mut problems = Vec::new();
        let read = json.codecs.read(codec, member, &path, &mut problems)?;
        if !problems.is_empty() {
            return None;
        }
        fields.push((Rc::from(kept.field.as_str()), read));
    }
    Some(Value::record(&records.entity, fields))
}

/// Whether `cache`, store `store`, may be read: it is being asked, and
/// every clear it owes has now been made.
async fn owed_cleared(ctx: &Ctx<'_>, store: &str, cache: &dyn Cache, health: &CacheHealth) -> bool {
    let monotonic = ctx.host().monotonic_ms();
    if !health.asks(store, monotonic) {
        return false;
    }
    let owed = health.owed(store);
    if owed.is_empty() {
        return true;
    }
    match cache.clear(&owed, now(ctx)).await {
        Ok(()) => {
            health.answered(store, &owed);
            true
        }
        Err(why) => {
            fell_back(ctx, store, why, health, monotonic, &owed);
            false
        }
    }
}

/// Notes that `store` couldn't answer for `why`, owing a clear of `owed`,
/// and logs its fallback the first time.
fn fell_back(
    ctx: &Ctx<'_>,
    store: &str,
    why: Unasked,
    health: &CacheHealth,
    monotonic: Option<u64>,
    owed: &[String],
) {
    if health.failed(store, monotonic, owed) {
        ctx.host().log("caches.fallback", fallback(store, why));
    }
}

/// The clears `clears` asks for, merged by store, each store's entities
/// once and in name order.
pub(crate) fn merged<'c>(
    clears: impl IntoIterator<Item = &'c Clear>,
) -> BTreeMap<String, Vec<String>> {
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for clear in clears {
        out.entry(clear.store.clone())
            .or_default()
            .extend(clear.tags.iter().cloned());
    }
    out.into_iter()
        .map(|(store, tags)| (store, tags.into_iter().collect()))
        .collect()
}

/// Clears, on `host` at `now`, what a committed change asks (area 5, "The
/// running app keeps caches current"): each store's entries reading any
/// of its entities, with what an earlier clear left owed. A cache that
/// can't answer is owed them, and tried again once the response has gone.
pub async fn clear(host: &dyn Host, clears: &BTreeMap<String, Vec<String>>, now: i64) {
    let health = host.cache_health();
    let monotonic = host.monotonic_ms();
    for (store, tags) in clears {
        let Some(cache) = host.cache(store) else {
            continue;
        };
        if !health.asks(store, monotonic) {
            health.owe(store, tags);
            continue;
        }
        let mut all: BTreeSet<String> = health.owed(store).into_iter().collect();
        all.extend(tags.iter().cloned());
        let all: Vec<String> = all.into_iter().collect();
        match cache.clear(&all, now).await {
            Ok(()) => health.answered(store, &all),
            Err(why) => {
                if health.failed(store, monotonic, &all) {
                    host.log("caches.fallback", fallback(store, why));
                }
                // Tried once more after the response; until it succeeds,
                // this server serves nothing it would have cleared.
                let store = store.clone();
                host.defer(Box::pin(async move {
                    let health = CacheHealth::process();
                    if let Ok(()) = cache.clear(&all, now).await {
                        health.answered(&store, &all);
                    }
                }));
            }
        }
    }
}

/// A host's caches, by connection, each the same cache however often it
/// is asked: what a test's and `jadpo dev`'s hosts keep every cache in.
#[derive(Clone, Debug, Default)]
pub struct MemoryCaches {
    caches: Rc<std::cell::RefCell<BTreeMap<String, MemoryCache>>>,
}

impl MemoryCaches {
    /// Connection `connection`'s cache, made empty the first time it is
    /// asked for.
    pub fn of(&self, connection: &str) -> MemoryCache {
        self.caches
            .borrow_mut()
            .entry(connection.to_string())
            .or_default()
            .clone()
    }

    /// Every cache asked for so far, by connection.
    pub fn all(&self) -> Vec<(String, MemoryCache)> {
        self.caches
            .borrow()
            .iter()
            .map(|(name, cache)| (name.clone(), cache.clone()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| name.to_string()).collect()
    }

    fn run<T>(future: impl std::future::Future<Output = T>) -> T {
        crate::block_on(future).expect("a cache in memory never waits")
    }

    #[test]
    fn an_entry_is_read_back_under_the_generations_it_was_kept_under() {
        let cache = MemoryCache::new();
        let product = tags(&["Product"]);
        let looked = run(cache.look("k", &product, 0)).expect("looked");
        assert_eq!(
            looked,
            Looked {
                generations: vec![0],
                entry: None
            }
        );
        let after = run(cache.keep("k", b"{}", 60_000, &product, 0)).expect("kept");
        assert_eq!(after, vec![0]);
        let looked = run(cache.look("k", &product, 1)).expect("looked");
        assert_eq!(looked.entry.as_deref(), Some(&b"{}"[..]));
    }

    #[test]
    fn a_clear_moves_the_generation_and_deletes_each_entry_listed() {
        let cache = MemoryCache::new();
        run(cache.keep("one", b"1", 60_000, &tags(&["Product"]), 0)).expect("kept");
        run(cache.keep("two", b"2", 60_000, &tags(&["Category", "Product"]), 0)).expect("kept");
        run(cache.keep("three", b"3", 60_000, &tags(&["Category"]), 0)).expect("kept");
        run(cache.clear(&tags(&["Product"]), 1)).expect("cleared");
        assert_eq!(cache.entries(1), 1, "only what reads no product is left");
        let looked = run(cache.look("three", &tags(&["Category", "Product"]), 1)).expect("looked");
        assert_eq!(looked.generations, vec![0, 1]);
    }

    #[test]
    fn an_entry_expires_after_its_keep() {
        let cache = MemoryCache::new();
        run(cache.keep("k", b"1", 1_000, &tags(&["Product"]), 0)).expect("kept");
        assert_eq!(cache.entries(999), 1);
        assert_eq!(cache.entries(1_000), 0);
    }

    #[test]
    fn an_entry_holds_its_generations_its_time_and_its_answer() {
        let bytes = entry(&[3, 0], 42, |out| out.push_str("{\"name\":\"tea\"}"));
        assert_eq!(
            String::from_utf8_lossy(&bytes),
            "{\"generations\":[3,0],\"filled\":42,\"value\":{\"name\":\"tea\"}}"
        );
        let (generations, filled, value) =
            read_entry(&bytes, |json, _| Some(Value::text(json["name"].as_str()?)))
                .expect("read back");
        assert_eq!(generations, vec![3, 0]);
        assert_eq!(filled, 42);
        assert_eq!(value, Value::text("tea"));
    }

    #[test]
    fn a_secret_argument_is_never_hashed() {
        let mut hasher = blake3::Hasher::new();
        let secret = Value::Secret(jadpo_value::secret::Secret::new("Password", "hunter2"));
        assert!(hashed(&mut hasher, &secret).is_none());
        let inside = Value::List(Rc::from(vec![Value::Int(1), secret]));
        assert!(hashed(&mut hasher, &inside).is_none());
    }

    #[test]
    fn different_arguments_hash_apart() {
        let digest = |value: &Value| {
            let mut hasher = blake3::Hasher::new();
            hashed(&mut hasher, value).expect("hashed");
            hasher.finalize()
        };
        assert_ne!(digest(&Value::text("ab")), digest(&Value::text("a")));
        assert_ne!(
            digest(&Value::List(Rc::from(vec![
                Value::text("a"),
                Value::text("b")
            ]))),
            digest(&Value::List(Rc::from(vec![Value::text("ab")]))),
        );
    }

    #[test]
    fn a_revision_comes_from_the_header_or_the_cookie() {
        let request = |headers: Vec<(&str, &str)>| Request {
            method: "GET".into(),
            path: "/".into(),
            headers: headers
                .into_iter()
                .map(|(name, value)| (name.to_string(), value.to_string()))
                .collect(),
            ..Request::default()
        };
        assert_eq!(revision_of(&request(vec![])), None);
        assert_eq!(
            revision_of(&request(vec![("Jadpo-Revision", "17")])),
            Some(17)
        );
        assert_eq!(
            revision_of(&request(vec![("cookie", "a=b; jadpo_revision=20")])),
            Some(20)
        );
        assert_eq!(
            revision_of(&request(vec![
                ("Jadpo-Revision", "30"),
                ("cookie", "jadpo_revision=20")
            ])),
            Some(30)
        );
    }

    #[test]
    fn a_write_carries_its_revision_as_a_header_and_a_cookie() {
        assert_eq!(
            revision_headers(1_700_000_000_000, 600_000),
            vec![
                ("jadpo-revision".to_string(), "1700000000000".to_string()),
                (
                    "set-cookie".to_string(),
                    "jadpo_revision=1700000000000; Path=/; Max-Age=600; HttpOnly; Secure; SameSite=Strict"
                        .to_string()
                ),
            ]
        );
    }

    /// A cache that can't be reached.
    struct Down;

    impl Cache for Down {
        fn look<'a>(
            &'a self,
            _: &'a str,
            _: &'a [String],
            _: i64,
        ) -> LocalFuture<'a, Result<Looked, Unasked>> {
            Box::pin(async { Err(Unasked::Unreachable) })
        }

        fn keep<'a>(
            &'a self,
            _: &'a str,
            _: &'a [u8],
            _: u64,
            _: &'a [String],
            _: i64,
        ) -> LocalFuture<'a, Result<Vec<u64>, Unasked>> {
            Box::pin(async { Err(Unasked::Unreachable) })
        }

        fn forget<'a>(&'a self, _: &'a str) -> LocalFuture<'a, Result<(), Unasked>> {
            Box::pin(async { Err(Unasked::Unreachable) })
        }

        fn clear<'a>(&'a self, _: &'a [String], _: i64) -> LocalFuture<'a, Result<(), Unasked>> {
            Box::pin(async { Err(Unasked::TimedOut) })
        }
    }

    /// A host whose one cache is down, keeping what it logs and defers.
    #[derive(Default)]
    struct Hosting {
        health: CacheHealth,
        logged: std::cell::RefCell<Vec<(String, serde_json::Map<String, serde_json::Value>)>>,
        deferred: std::cell::Cell<usize>,
    }

    impl Host for Hosting {
        fn database(&self, _: &str) -> Option<&dyn crate::Database> {
            None
        }

        fn random(&self, _: &mut [u8]) -> bool {
            false
        }

        fn cache(&self, _: &str) -> Option<Rc<dyn Cache>> {
            Some(Rc::new(Down))
        }

        fn cache_health(&self) -> &CacheHealth {
            &self.health
        }

        fn log(&self, event: &str, fields: serde_json::Map<String, serde_json::Value>) {
            self.logged.borrow_mut().push((event.to_string(), fields));
        }

        fn defer(&self, work: LocalFuture<'static, ()>) {
            drop(work);
            self.deferred.set(self.deferred.get() + 1);
        }
    }

    /// A clear a cache can't make never fails the change that asked for it:
    /// it is owed, tried again after the response, and logged once, naming
    /// the connection and why.
    #[test]
    fn a_clear_a_cache_cant_make_is_owed_and_logged_once() {
        let host = Hosting::default();
        let clears = merged(&[
            Clear {
                store: "cache".into(),
                tags: tags(&["Product"]),
            },
            Clear {
                store: "cache".into(),
                tags: tags(&["Category", "Product"]),
            },
        ]);
        assert_eq!(clears["cache"], tags(&["Category", "Product"]));
        run(clear(&host, &clears, 0));
        run(clear(&host, &clears, 0));
        assert!(host.health.owes_any("cache", &tags(&["Category"])));
        assert_eq!(host.deferred.get(), 2, "tried again after each response");
        let logged = host.logged.borrow();
        assert_eq!(logged.len(), 1, "{logged:?}");
        assert_eq!(logged[0].0, "caches.fallback");
        assert_eq!(logged[0].1["connection"], "cache");
        assert_eq!(logged[0].1["reason"], "timed_out");
    }

    #[test]
    fn a_cache_that_failed_is_owed_its_clear_and_left_alone() {
        let health = CacheHealth::default();
        assert!(health.asks("cache", Some(0)));
        assert!(health.failed("cache", Some(0), &tags(&["Product"])));
        assert!(!health.failed("cache", Some(1), &[]), "logged once");
        assert!(!health.asks("cache", Some(STORE_PAUSE_MS)));
        assert!(health.asks("cache", Some(STORE_PAUSE_MS + 1)));
        assert!(health.owes_any("cache", &tags(&["Product", "Category"])));
        health.answered("cache", &tags(&["Product"]));
        assert!(!health.owes_any("cache", &tags(&["Product"])));
        // A clear while it is left alone is owed, and leaves the pause as
        // it was.
        assert!(health.failed("cache", Some(100), &[]));
        health.owe("cache", &tags(&["Category"]));
        assert!(health.owes_any("cache", &tags(&["Category"])));
        assert!(health.asks("cache", Some(100 + STORE_PAUSE_MS)));
    }

    // --- record caches (chunk Q9b) ---------------------------------------------------

    const NOTE: &str = "0192f0c4-3b5e-7c1a-9d2e-4f6a8b0c1d2e";
    const ADA: &str = "0192f0c4-3b5e-7c1a-9d2e-4f6a8b0c1d2f";
    const BOB: &str = "0192f0c4-3b5e-7c1a-9d2e-4f6a8b0c1d30";

    fn bytes(id: &str) -> [u8; 16] {
        let hex: String = id.chars().filter(|c| *c != '-').collect();
        let mut out = [0u8; 16];
        for (at, byte) in out.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&hex[at * 2..at * 2 + 2], 16).expect("hex");
        }
        out
    }

    /// The one note a store holds, Ada's, and how many reads it answered.
    #[derive(Default)]
    struct Notes {
        title: std::cell::RefCell<String>,
        reads: std::cell::Cell<usize>,
    }

    struct Reading(Rc<Notes>);

    impl crate::Transaction for Reading {
        fn query<'a>(
            &'a mut self,
            _sql: &'a str,
            _params: &'a [crate::SqlValue],
        ) -> LocalFuture<'a, Result<Vec<crate::Row>, crate::StoreError>> {
            self.0.reads.set(self.0.reads.get() + 1);
            let row = vec![
                crate::SqlValue::Uuid(bytes(NOTE)),
                crate::SqlValue::Uuid(bytes(ADA)),
                crate::SqlValue::Text(self.0.title.borrow().clone()),
            ];
            Box::pin(async move { Ok(vec![row]) })
        }

        fn execute<'a>(
            &'a mut self,
            _sql: &'a str,
            _params: &'a [crate::SqlValue],
        ) -> LocalFuture<'a, Result<u64, crate::StoreError>> {
            Box::pin(async { Ok(0) })
        }

        fn batch<'a>(
            &'a mut self,
            _sql: &'a str,
        ) -> LocalFuture<'a, Result<(), crate::StoreError>> {
            Box::pin(async { Ok(()) })
        }

        fn commit(self: Box<Self>) -> LocalFuture<'static, Result<(), crate::StoreError>> {
            Box::pin(async { Ok(()) })
        }

        fn rollback(self: Box<Self>) -> LocalFuture<'static, Result<(), crate::StoreError>> {
            Box::pin(async { Ok(()) })
        }
    }

    struct NoteStore(Rc<Notes>);

    impl crate::Database for NoteStore {
        fn begin_at(
            &self,
            _isolation: crate::Isolation,
        ) -> LocalFuture<'_, Result<Box<dyn crate::Transaction>, crate::StoreError>> {
            let notes = Rc::clone(&self.0);
            Box::pin(async move { Ok(Box::new(Reading(notes)) as Box<dyn crate::Transaction>) })
        }
    }

    /// A host whose store holds the note and whose cache is in memory.
    struct Shelved {
        store: NoteStore,
        cache: MemoryCache,
        health: CacheHealth,
    }

    impl Host for Shelved {
        fn database(&self, _: &str) -> Option<&dyn crate::Database> {
            Some(&self.store)
        }

        fn random(&self, _: &mut [u8]) -> bool {
            false
        }

        fn cache(&self, _: &str) -> Option<Rc<dyn Cache>> {
            Some(Rc::new(self.cache.clone()))
        }

        fn cache_health(&self) -> &CacheHealth {
            &self.health
        }
    }

    /// `Note { id, owner_id, title }`, read by its owner alone, kept in
    /// `cache` for a minute, as a route's read of one by `id` giving a
    /// `NoteView { id, title }` reads it.
    fn note_cache() -> (RouteCache, RecordRead, ReadPlan) {
        use jadpo_std::caches::KeptField;
        use jadpo_std::policy::{CallerPart, Narrowed, RecordGrant, RecordGrants};
        use jadpo_std::read::{Column, Field, Param, Record, Select, Source, SqlType};
        use jadpo_std::redis::{Element, Given};
        let kept = |field: &str, codec: u32| KeptField {
            field: field.into(),
            born: field.into(),
            codec,
        };
        let read = |cardinality: Cardinality| ReadPlan {
            store: "main".into(),
            settings: Vec::new(),
            cardinality,
            select: Select {
                sql: "SELECT \"id\", \"owner_id\", \"title\" FROM \"note\" WHERE \"id\" = $1"
                    .into(),
                unpoliced: None,
                permitted: None,
                params: vec![Param {
                    source: Source::Arg {
                        index: 0,
                        path: Vec::new(),
                    },
                    ty: SqlType::Uuid,
                }],
                columns: vec![Column::Uuid, Column::Uuid, Column::Text],
                held: Vec::new(),
                record: Record {
                    ty: "Note".into(),
                    fields: vec![
                        ("id".into(), Field::Column(0)),
                        ("owner_id".into(), Field::Column(1)),
                        ("title".into(), Field::Column(2)),
                    ],
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
            redis: None,
            every: None,
        };
        let records = RecordRead {
            entity: "Note".into(),
            id: Given::Arg {
                index: 0,
                path: Vec::new(),
            },
            fields: vec![kept("id", 0), kept("owner_id", 0), kept("title", 1)],
            grants: RecordGrants {
                grants: vec![RecordGrant::Caller {
                    field: "owner_id".into(),
                    part: CallerPart::User,
                }],
                narrowed: Narrowed::Untouched,
            },
            element: Element::Picked {
                ty: "NoteView".into(),
                fields: vec![("id".into(), "id".into()), ("title".into(), "title".into())],
            },
            fill: Box::new(read(Cardinality::Optional)),
        };
        let cache = RouteCache {
            store: "cache".into(),
            keep_ms: 60_000,
            tags: tags(&["Note"]),
            key: "Note records test".into(),
            records: None,
        };
        (cache, records, read(Cardinality::Optional))
    }

    fn view(title: &str) -> Value {
        Value::record(
            "NoteView",
            vec![
                (Rc::from("id"), Value::uuid(NOTE)),
                (Rc::from("title"), Value::text(title)),
            ],
        )
    }

    /// One copy of a record serves every caller, held to policy each time
    /// it is read: its owner reads it from the cache, a change behind the
    /// app's back unseen, someone else finds nothing, from the same copy
    /// and without asking the store, and once its entity is cleared the
    /// next read keeps the record as the store holds it now (area 5, "A
    /// record cache"; chunk Q9b).
    #[test]
    fn one_copy_serves_every_caller_held_to_policy_as_it_is_read() {
        use jadpo_std::http::{Codec, CodecId};
        use jadpo_std::types::ValidatorId;
        let validators = [
            jadpo_value::Validator::new(Some(jadpo_std::Builtin::Uuid), vec![]),
            jadpo_value::Validator::new(Some(jadpo_std::Builtin::Text), vec![]),
        ];
        let codecs = [
            Codec::Scalar {
                builtin: jadpo_std::Builtin::Uuid,
                validator: ValidatorId(0),
                int_as_number: false,
            },
            Codec::Scalar {
                builtin: jadpo_std::Builtin::Text,
                validator: ValidatorId(1),
                int_as_number: false,
            },
        ];
        let types = [CodecId(0), CodecId(1)];
        let json = JsonCodecs {
            codecs: jadpo_value::json::Codecs {
                codecs: &codecs,
                validators: &validators,
                shapes: &[],
                coder: None,
            },
            types: &types,
        };
        let notes = Rc::new(Notes::default());
        *notes.title.borrow_mut() = "Buy tea".into();
        let host = Shelved {
            store: NoteStore(Rc::clone(&notes)),
            cache: MemoryCache::new(),
            health: CacheHealth::default(),
        };
        let (cache, records, plan) = note_cache();
        let places = crate::Places::new();
        let read_as = |who: &str, revision: Option<i64>| {
            let mut ctx = Ctx::new(&places, &host, crate::Env::default());
            let caller = crate::http::identity::principal(
                jadpo_std::http::PrincipalKind::User,
                Value::uuid(who),
                Value::text("someone"),
            );
            crate::policy::call_as(&mut ctx, Some(&caller));
            let args = [Value::uuid(NOTE)];
            let outcome = run(record_read(
                &mut ctx,
                (&[], json),
                (&cache, &records, &plan),
                &args,
                revision,
            ));
            match outcome {
                Outcome::Value(value) => value,
                other => panic!("{other:?}"),
            }
        };
        assert_eq!(read_as(ADA, None), view("Buy tea"));
        assert_eq!(notes.reads.get(), 1);
        let now = {
            let ctx = Ctx::new(&places, &host, crate::Env::default());
            now(&ctx)
        };
        // The copy holds the record by the names its fields were born
        // with, never under its `id` as written.
        let kept = host.cache.texts(now);
        assert_eq!(
            kept,
            [format!(
                "{{\"generations\":[0],\"filled\":{now},\"value\":{{\"id\":\"{NOTE}\",\"owner_id\":\"{ADA}\",\"title\":\"Buy tea\"}}}}"
            )]
        );
        assert!(!record_key(&cache.key, NOTE).contains(NOTE));
        // A change behind the app's back is unseen, and someone else finds
        // nothing in the same copy, the store unasked.
        *notes.title.borrow_mut() = "Buy green tea".into();
        assert_eq!(read_as(ADA, None), view("Buy tea"));
        assert_eq!(read_as(BOB, None), Value::None);
        assert_eq!(notes.reads.get(), 1);
        // A request carrying a later revision reads the store.
        assert_eq!(read_as(ADA, Some(now)), view("Buy green tea"));
        assert_eq!(notes.reads.get(), 2);
        // A change to the entity clears every copy.
        *notes.title.borrow_mut() = "Buy oolong".into();
        run(clear(
            &host,
            &BTreeMap::from([("cache".to_string(), tags(&["Note"]))]),
            now,
        ));
        assert_eq!(host.cache.entries(now), 0);
        assert_eq!(read_as(ADA, None), view("Buy oolong"));
        assert_eq!(read_as(BOB, None), Value::None);
        assert_eq!(notes.reads.get(), 3);
    }
}
