//! The test database (area 13, "The test database"; docs/compiler-runtime.md
//! §2.7): each test that can touch data gets its own database at the
//! latest migration, empty except for what its fixtures create.
//!
//! The server is the one `JADPO_TEST_DATABASE_URL` names or a disposable
//! one ([`crate::postgres`]), started the first time a test needs a
//! database and kept for the rest of the process. Each store's migrations
//! build one template database, named after their digest, so a server that
//! already has it (one `JADPO_TEST_DATABASE_URL` names, across runs) builds
//! it once. Each test gets a copy (`CREATE DATABASE … TEMPLATE`), and
//! copies are prepared in the background by a few workers, a few ahead of
//! the tests that will take them, so a test seldom waits for one. A test's
//! copy is emptied when it ends (its tables truncated and its sequences
//! started again) and used for a later test, also in the background, unless
//! it failed and the run keeps failed tests' databases. A template whose
//! migrations left rows or moved a sequence can't be put back that way, so
//! its copies are dropped instead. Every name a process creates carries a
//! tag of its own ([`crate::postgres::process_tag`]), so processes sharing
//! one server never collide.

use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::thread::JoinHandle;

use jadpo_runtime::{MigrateError, Migration};

use crate::postgres::{Server, ServerError};

/// How many copies are kept ready beyond the ones tests hold.
const AHEAD: usize = 4;

/// How many databases wait to be dropped before they are: drops that run at
/// once share the checkpoint each waits for.
const DROP_BATCH: usize = 16;

/// How many threads make, empty and drop copies at once: PostgreSQL makes
/// a copy in tens of milliseconds, a few at a time.
const WORKERS: usize = 4;

/// The server tests in this process use, started the first time one is
/// needed and kept until the process ends.
pub fn server() -> Result<Arc<Server>, String> {
    static SERVER: OnceLock<Result<Arc<Server>, String>> = OnceLock::new();
    SERVER
        .get_or_init(|| {
            Server::start()
                .map(Arc::new)
                .map_err(|error| error.to_string())
        })
        .clone()
}

/// The digest naming a store's template: its migrations' names and
/// digests, in order.
pub fn template_digest(steps: &[Migration]) -> String {
    let mut hasher = blake3::Hasher::new();
    for step in steps {
        hasher.update(step.name.as_bytes());
        hasher.update(b"\0");
        hasher.update(step.digest.as_bytes());
        hasher.update(b"\0");
    }
    hasher.finalize().to_hex()[..16].to_string()
}

/// Why a store's template couldn't be built.
#[derive(Clone, Debug)]
enum Unmade {
    /// Its migrations were refused: an extension the server lacks, say.
    Refused(MigrateError),
    /// Anything else, as the test that needed it reports it.
    Failed(String),
}

impl From<String> for Unmade {
    fn from(error: String) -> Unmade {
        Unmade::Failed(error)
    }
}

impl std::fmt::Display for Unmade {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Unmade::Refused(error) => write!(f, "applying the migrations failed: {error:?}"),
            Unmade::Failed(error) => f.write_str(error),
        }
    }
}

/// The template of one store: built once per process and digest.
fn template(server: &Server, store: &str, steps: &[Migration]) -> Result<String, Unmade> {
    static BUILT: OnceLock<Mutex<BTreeMap<String, Result<String, Unmade>>>> = OnceLock::new();
    let name = format!("jadpo_tpl_{}_{}", short(store), template_digest(steps));
    let built = BUILT.get_or_init(|| Mutex::new(BTreeMap::new()));
    let mut built = built
        .lock()
        .map_err(|_| "the template lock was poisoned".to_string())?;
    if let Some(done) = built.get(&name) {
        return done.clone();
    }
    let done = build_template(server, &name, steps);
    built.insert(name, done.clone());
    done
}

/// A store's name as part of a database's name.
fn short(store: &str) -> String {
    store
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .take(20)
        .collect()
}

fn build_template(server: &Server, name: &str, steps: &[Migration]) -> Result<String, Unmade> {
    if exists(server, name)? {
        return Ok(name.to_string());
    }
    // Built under a name of its own, then renamed: a server shared across
    // runs never holds half a template under the real name.
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let building = format!(
        "jadpo_build_{}_{}",
        crate::postgres::process_tag(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    );
    server
        .admin(&format!("CREATE DATABASE \"{building}\""))
        .map_err(|e| e.to_string())?;
    let migrated = migrate(&server.url(&building), steps);
    if let Err(error) = migrated {
        let _ = drop_database(server, &building);
        return Err(error);
    }
    match rename(server, &building, name) {
        Ok(()) => Ok(name.to_string()),
        Err(error) => {
            let _ = drop_database(server, &building);
            // Another run built it meanwhile.
            if exists(server, name).unwrap_or(false) {
                Ok(name.to_string())
            } else {
                Err(error.into())
            }
        }
    }
}

/// Applies `steps` to the database at `url`, on a runtime of its own that
/// is gone, with its connections, when this returns.
fn migrate(url: &str, steps: &[Migration]) -> Result<(), Unmade> {
    let url = url.to_string();
    let migrations = steps.to_vec();
    std::thread::spawn(move || -> Result<(), Unmade> {
        let runtime = jadpo_host_native::Runtime::new().map_err(|e| e.to_string())?;
        let store = jadpo_host_native::postgres(&url).map_err(|e| format!("{e:?}"))?;
        runtime
            .block_on(jadpo_runtime::migrate(&store, &migrations))
            .map(|_| ())
            .map_err(Unmade::Refused)
    })
    .join()
    .map_err(|_| Unmade::Failed("the migrating thread panicked".to_string()))?
}

/// Whether the server has a database called `name`.
fn exists(server: &Server, name: &str) -> Result<bool, String> {
    let url = server.url(crate::postgres::ADMIN_DATABASE);
    let name = name.to_string();
    std::thread::spawn(move || -> Result<bool, String> {
        let runtime = jadpo_host_native::Runtime::new().map_err(|e| e.to_string())?;
        let store = jadpo_host_native::postgres(&url).map_err(|e| format!("{e:?}"))?;
        runtime.block_on(async {
            use jadpo_runtime::{Database, SqlValue};
            let mut transaction = store.begin().await.map_err(|e| format!("{e:?}"))?;
            let rows = transaction
                .query(
                    "SELECT count(*)::int4 FROM pg_database WHERE datname = $1",
                    &[SqlValue::Text(name)],
                )
                .await
                .map_err(|e| format!("{e:?}"))?;
            let _ = transaction.rollback().await;
            Ok(matches!(rows.first().and_then(|r| r.first()), Some(SqlValue::Int4(n)) if *n > 0))
        })
    })
    .join()
    .map_err(|_| "a server query's thread panicked".to_string())?
}

/// The names of the server's databases that start with `prefix`.
fn databases_starting(server: &Server, prefix: &str) -> Result<Vec<String>, String> {
    let url = server.url(crate::postgres::ADMIN_DATABASE);
    let prefix = prefix.to_string();
    std::thread::spawn(move || -> Result<Vec<String>, String> {
        let runtime = jadpo_host_native::Runtime::new().map_err(|e| e.to_string())?;
        let store = jadpo_host_native::postgres(&url).map_err(|e| format!("{e:?}"))?;
        runtime.block_on(async {
            use jadpo_runtime::{Database, SqlValue};
            let mut transaction = store.begin().await.map_err(|e| format!("{e:?}"))?;
            let rows = transaction
                .query(
                    "SELECT datname::text FROM pg_database WHERE starts_with(datname, $1) ORDER BY 1",
                    &[SqlValue::Text(prefix)],
                )
                .await
                .map_err(|e| format!("{e:?}"))?;
            let _ = transaction.rollback().await;
            Ok(rows
                .into_iter()
                .filter_map(|row| match row.into_iter().next() {
                    Some(SqlValue::Text(name)) => Some(name),
                    _ => None,
                })
                .collect())
        })
    })
    .join()
    .map_err(|_| "a server query's thread panicked".to_string())?
}

/// Runs an admin statement, waiting out a template another session is
/// still leaving: PostgreSQL refuses to copy a database anyone is
/// connected to, and a connection just closed can take a moment to go.
fn admin_retrying(server: &Server, sql: &str) -> Result<(), String> {
    let mut wait = std::time::Duration::from_millis(10);
    for _ in 0..12 {
        match server.admin(sql) {
            Ok(()) => return Ok(()),
            Err(ServerError::Failed { detail, .. })
                if detail.contains("is being accessed by other users") =>
            {
                std::thread::sleep(wait);
                wait = (wait * 2).min(std::time::Duration::from_millis(500));
            }
            Err(error) => return Err(error.to_string()),
        }
    }
    server.admin(sql).map_err(|e| e.to_string())
}

fn rename(server: &Server, from: &str, to: &str) -> Result<(), String> {
    admin_retrying(
        server,
        &format!("ALTER DATABASE \"{from}\" RENAME TO \"{to}\""),
    )
}

fn drop_database(server: &Server, name: &str) -> Result<(), String> {
    admin_retrying(
        server,
        &format!("DROP DATABASE IF EXISTS \"{name}\" WITH (FORCE)"),
    )
}

/// One test's databases: a copy of each store's template.
#[derive(Debug)]
pub struct Copy {
    /// Each store's database name and URL.
    pub stores: BTreeMap<String, (String, String)>,
}

impl Copy {
    fn names(self) -> impl Iterator<Item = String> {
        self.stores.into_values().map(|(name, _)| name)
    }
}

/// The test databases of one run.
pub struct TestDatabases {
    shared: Arc<Shared>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl std::fmt::Debug for TestDatabases {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TestDatabases").finish_non_exhaustive()
    }
}

struct Shared {
    steps: BTreeMap<String, Vec<Migration>>,
    keep_failed: bool,
    state: Mutex<State>,
    changed: Condvar,
}

/// One store's template, and how to empty a copy of it again, when one
/// can be: the statements that put it back as the template has it.
#[derive(Clone)]
struct Template {
    name: String,
    reset: Option<String>,
}

impl Template {
    /// The start of the names of the emptied copies of this template that
    /// runs leave on the server for later runs to take.
    fn free(&self) -> String {
        let hash = blake3::hash(self.name.as_bytes()).to_hex();
        format!("jadpo_free_{}_", &hash[..12])
    }
}

/// The server and each store's template, by store.
type Started = (Arc<Server>, BTreeMap<String, Template>);

#[derive(Default)]
struct State {
    /// The server and each store's template, once the first test asked.
    ready: Option<Result<Started, String>>,
    /// The store whose migrations the server refused, and why, when that
    /// is why `ready` failed.
    refused: Option<(String, MigrateError)>,
    /// Copies prepared and not yet taken.
    pool: VecDeque<Copy>,
    /// How many copies tests want ready.
    wanted: usize,
    /// How many copies workers are making or emptying for the pool.
    making: usize,
    /// Copies tests gave back, to empty and use again.
    used: VecDeque<Copy>,
    /// Databases to drop.
    drop: Vec<String>,
    /// How many tests may still ask for a copy: no more are prepared.
    remaining: usize,
    /// How many copies this run has made or taken from earlier runs.
    copies: usize,
    /// Whether copies can be emptied and used again: every template's
    /// migrations left its tables empty.
    reusable: bool,
    /// The run is over: the workers leave what is left for later runs, or
    /// drop it, and stop.
    done: bool,
}

impl State {
    /// Whether the pool, with what is on its way, has fewer copies than
    /// the tests waiting and the few kept ahead of them.
    fn short(&self) -> bool {
        let target = (self.wanted + AHEAD).min(self.remaining).max(self.wanted);
        !self.done && self.pool.len() + self.making < target
    }
}

impl TestDatabases {
    /// The databases for a run whose stores have `steps`; nothing starts
    /// until a test asks for its database.
    pub fn new(steps: BTreeMap<String, Vec<Migration>>, keep_failed: bool) -> TestDatabases {
        TestDatabases {
            shared: Arc::new(Shared {
                steps,
                keep_failed,
                state: Mutex::new(State {
                    remaining: usize::MAX,
                    ..State::default()
                }),
                changed: Condvar::new(),
            }),
            worker: Mutex::new(None),
        }
    }

    /// The databases for a run of `tests` tests that may use a store: no
    /// copies are prepared beyond what they can use.
    pub fn for_tests(self, tests: usize) -> TestDatabases {
        if let Ok(mut state) = self.shared.state.lock() {
            state.remaining = tests;
        }
        self
    }

    /// How many copies the run has made, or taken from earlier runs, so
    /// far.
    pub fn copies(&self) -> usize {
        self.shared.state.lock().map_or(0, |state| state.copies)
    }

    /// The stores the databases are for.
    pub fn stores(&self) -> Vec<String> {
        self.shared.steps.keys().cloned().collect()
    }

    /// A copy of each store's template for one test, prepared in the
    /// background where it can be. It waits, the first time, for the
    /// server and the templates.
    pub fn acquire(&self) -> Result<Copy, String> {
        self.start_workers();
        let shared = &self.shared;
        let mut state = shared.state.lock().map_err(|_| poisoned())?;
        state.wanted += 1;
        shared.changed.notify_all();
        loop {
            if let Some(Err(error)) = &state.ready {
                return Err(error.clone());
            }
            if let Some(copy) = state.pool.pop_front() {
                state.wanted = state.wanted.saturating_sub(1);
                state.remaining = state.remaining.saturating_sub(1);
                shared.changed.notify_all();
                return Ok(copy);
            }
            state = shared.changed.wait(state).map_err(|_| poisoned())?;
        }
    }

    /// Gives a test's copy back: emptied in the background and used again
    /// where its templates allow, otherwise dropped; unless the test
    /// failed and the run keeps failed tests' databases, when its names
    /// are given back to report.
    pub fn release(&self, copy: Copy, failed: bool) -> Vec<String> {
        if failed && self.shared.keep_failed {
            return copy.names().collect();
        }
        if let Ok(mut state) = self.shared.state.lock() {
            if state.reusable {
                state.used.push_back(copy);
            } else {
                state.drop.extend(copy.names());
            }
            self.shared.changed.notify_all();
        }
        Vec::new()
    }

    /// Starts the server and builds each store's template, unless a test
    /// already has, and gives the store whose migrations the server
    /// refused, and why: `jadpo test` stops on that before any test runs,
    /// as `jadpo migrate apply` does (areas 20 and 21). Any other trouble
    /// is left for the tests that need a database to report.
    pub fn refused(&self) -> Option<(String, MigrateError)> {
        self.start_workers();
        let mut state = self.shared.state.lock().ok()?;
        while state.ready.is_none() {
            state = self.shared.changed.wait(state).ok()?;
        }
        state.refused.clone()
    }

    fn start_workers(&self) {
        let Ok(mut worker) = self.worker.lock() else {
            return;
        };
        if worker.is_some() {
            return;
        }
        let shared = Arc::clone(&self.shared);
        *worker = std::thread::Builder::new()
            .name("jadpo-test-databases".into())
            .spawn(move || prepare(&shared))
            .ok();
    }
}

impl Drop for TestDatabases {
    fn drop(&mut self) {
        if let Ok(mut state) = self.shared.state.lock() {
            state.done = true;
            self.shared.changed.notify_all();
        }
        if let Ok(mut worker) = self.worker.lock()
            && let Some(handle) = worker.take()
        {
            let _ = handle.join();
        }
    }
}

fn poisoned() -> String {
    "the test databases' lock was poisoned".to_string()
}

/// The first worker: starts the server and builds the templates, then
/// starts the other workers and works beside them until the run is over.
fn prepare(shared: &Arc<Shared>) {
    let ready = (|| -> Result<Started, (Option<String>, Unmade)> {
        let server = server().map_err(|error| (None, error.into()))?;
        let mut templates = BTreeMap::new();
        for (store, steps) in &shared.steps {
            let name =
                template(&server, store, steps).map_err(|error| (Some(store.clone()), error))?;
            templates.insert(store.clone(), Template { name, reset: None });
        }
        Ok((server, templates))
    })();
    let (server, mut templates) = match ready {
        Ok(started) => started,
        Err((store, error)) => {
            if let Ok(mut state) = shared.state.lock() {
                if let (Some(store), Unmade::Refused(refused)) = (store, &error) {
                    state.refused = Some((store, refused.clone()));
                }
                state.ready = Some(Err(error.to_string()));
                shared.changed.notify_all();
            }
            return;
        }
    };
    // The first copy tells how to empty one again: a copy is the template
    // as it was migrated (or one a run left emptied, as only copies of
    // templates that empty that way are left), so it is read before any
    // test writes to it.
    let first = make_copy(&server, &templates).and_then(|copy| {
        for (store, (_, url)) in &copy.stores {
            if let Some(template) = templates.get_mut(store) {
                template.reset = how_to_reset(url)?;
            }
        }
        Ok(copy)
    });
    let Ok(mut state) = shared.state.lock() else {
        return;
    };
    match first {
        Ok(copy) => {
            state.pool.push_back(copy);
            state.copies += 1;
            state.reusable = templates.values().all(|template| template.reset.is_some());
            state.ready = Some(Ok((Arc::clone(&server), templates.clone())));
        }
        Err(error) => {
            state.ready = Some(Err(format!("copying a template failed: {error}")));
            shared.changed.notify_all();
            return;
        }
    }
    shared.changed.notify_all();
    drop(state);
    let others: Vec<JoinHandle<()>> = (1..WORKERS)
        .filter_map(|_| {
            let shared = Arc::clone(shared);
            let server = Arc::clone(&server);
            let templates = templates.clone();
            std::thread::Builder::new()
                .name("jadpo-test-databases".into())
                .spawn(move || work(&shared, &server, &templates))
                .ok()
        })
        .collect();
    work(shared, &server, &templates);
    for other in others {
        let _ = other.join();
    }
}

/// What a worker does next.
enum Job {
    Drop(String),
    Make,
    Empty(Copy),
    /// Leaves a copy for later runs, emptying it first if a test used it.
    Leave(Copy, bool),
    Stop,
}

/// One worker: keeps copies ready, emptying the ones tests gave back
/// before making new ones, and drops what is no longer wanted; once the
/// run is over, leaves the copies left, emptied, for later runs (or drops
/// them, where they can't be emptied) and stops.
fn work(shared: &Shared, server: &Server, templates: &BTreeMap<String, Template>) {
    // Emptying a copy runs on this worker's own runtime.
    let runtime = jadpo_host_native::Runtime::new().ok();
    loop {
        let job = {
            let Ok(mut state) = shared.state.lock() else {
                return;
            };
            loop {
                // Every drop waits for a checkpoint of the whole server,
                // so drops wait until there are several, or the run is
                // over, and go together.
                if (state.done || state.drop.len() >= DROP_BATCH)
                    && let Some(name) = state.drop.pop()
                {
                    break Job::Drop(name);
                }
                if state.done {
                    if let Some(copy) = state.pool.pop_front() {
                        break Job::Leave(copy, false);
                    }
                    if let Some(copy) = state.used.pop_front() {
                        break Job::Leave(copy, true);
                    }
                    break Job::Stop;
                }
                if state.short() {
                    state.making += 1;
                    match state.used.pop_front() {
                        Some(copy) if runtime.is_some() => break Job::Empty(copy),
                        Some(copy) => {
                            state.drop.extend(copy.names());
                            break Job::Make;
                        }
                        None => break Job::Make,
                    }
                }
                state = match shared.changed.wait(state) {
                    Ok(state) => state,
                    Err(_) => return,
                };
            }
        };
        // The copy for the pool, and whether it is a new one.
        let (made, new) = match job {
            Job::Stop => return,
            Job::Drop(name) => {
                let _ = drop_database(server, &name);
                continue;
            }
            Job::Leave(copy, used) => {
                let clean = !used
                    || runtime
                        .as_ref()
                        .is_some_and(|runtime| empty(runtime, &copy, templates).is_ok());
                // A copy no test used is the template as it was migrated,
                // whatever its migrations did, so it can be left too.
                if clean && (!used || state_reusable(shared)) {
                    leave(server, copy, templates);
                } else {
                    for name in copy.names() {
                        let _ = drop_database(server, &name);
                    }
                }
                continue;
            }
            Job::Make => (
                make_copy(server, templates)
                    .map_err(|error| format!("copying a template failed: {error}")),
                true,
            ),
            Job::Empty(copy) => match runtime
                .as_ref()
                .map(|runtime| empty(runtime, &copy, templates))
            {
                Some(Ok(())) => (Ok(copy), false),
                // A copy that won't empty is dropped, and a new one made
                // in its place.
                _ => {
                    let made = make_copy(server, templates)
                        .map_err(|error| format!("copying a template failed: {error}"));
                    if let Ok(mut state) = shared.state.lock() {
                        state.drop.extend(copy.names());
                    }
                    (made, true)
                }
            },
        };
        let Ok(mut state) = shared.state.lock() else {
            return;
        };
        state.making = state.making.saturating_sub(1);
        match made {
            Ok(copy) => {
                state.copies += usize::from(new);
                state.pool.push_back(copy);
            }
            Err(error) => state.ready = Some(Err(error)),
        }
        shared.changed.notify_all();
    }
}

fn state_reusable(shared: &Shared) -> bool {
    shared.state.lock().is_ok_and(|state| state.reusable)
}

/// A name for a database this process creates or takes, which no other
/// process sharing the server uses.
fn own_name(stem: &str) -> String {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    format!(
        "jadpo_{stem}_{}_{}",
        crate::postgres::process_tag(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )
}

/// A statement ending every other session connected to `database` (an
/// SQL expression giving its name).
fn others_ended(database: &str) -> String {
    format!(
        "SELECT pg_terminate_backend(pid) FROM pg_stat_activity \
         WHERE datname = {database} AND pid <> pg_backend_pid()"
    )
}

/// Leaves an emptied copy on the server for a later run to take, renamed
/// as its template's free copies are; one that can't be renamed is
/// dropped. Renaming waits for every session on it to end, and the run is
/// over, so any still connected is ended.
fn leave(server: &Server, copy: Copy, templates: &BTreeMap<String, Template>) {
    for (store, (name, _)) in copy.stores {
        let left = templates.get(&store).is_some_and(|template| {
            let free = format!("{}{}", template.free(), own_name("left"));
            let _ = server.admin(&others_ended(&format!("'{name}'")));
            rename(server, &name, &free).is_ok()
        });
        if !left {
            let _ = drop_database(server, &name);
        }
    }
}

/// One of `template`'s free copies, taken under a name of this process's
/// own: renaming it succeeds for one process only.
fn take_free(server: &Server, template: &Template, name: &str) -> bool {
    let Ok(free) = databases_starting(server, &template.free()) else {
        return false;
    };
    free.iter().any(|left| {
        server
            .admin(&format!("ALTER DATABASE \"{left}\" RENAME TO \"{name}\""))
            .is_ok()
    })
}

/// A copy of each store's template, under names no other process sharing
/// the server uses: one an earlier run left emptied where there is one,
/// else a new one.
fn make_copy(server: &Server, templates: &BTreeMap<String, Template>) -> Result<Copy, String> {
    let mut stores: BTreeMap<String, (String, String)> = BTreeMap::new();
    for (store, template) in templates {
        let name = own_name("test");
        if take_free(server, template, &name) {
            stores.insert(store.clone(), (name.clone(), server.url(&name)));
            continue;
        }
        if let Err(error) = admin_retrying(
            server,
            &format!("CREATE DATABASE \"{name}\" TEMPLATE \"{}\"", template.name),
        ) {
            for (made, _) in stores.into_values() {
                let _ = drop_database(server, &made);
            }
            return Err(error);
        }
        stores.insert(store.clone(), (name.clone(), server.url(&name)));
    }
    Ok(Copy { stores })
}

/// Puts a used copy back as its templates have it.
fn empty(
    runtime: &jadpo_host_native::Runtime,
    copy: &Copy,
    templates: &BTreeMap<String, Template>,
) -> Result<(), String> {
    for (store, (_, url)) in &copy.stores {
        let Some(reset) = templates.get(store).and_then(|t| t.reset.as_ref()) else {
            return Err(format!("no way to reset `{store}`"));
        };
        // A connection the test left open (its runtime no longer drives
        // it, say) is ended first, so nothing holds a lock the reset needs;
        // a lock still held after a while gives the copy up.
        let sql = format!(
            "SET LOCAL lock_timeout = '10s';\n{};\n{reset}",
            others_ended("current_database()")
        );
        let database = jadpo_host_native::postgres(url).map_err(|e| format!("{e:?}"))?;
        runtime.block_on(async {
            use jadpo_runtime::Database;
            let mut transaction = database.begin().await.map_err(|e| format!("{e:?}"))?;
            transaction
                .batch(&sql)
                .await
                .map_err(|e| format!("{e:?}"))?;
            transaction.commit().await.map_err(|e| format!("{e:?}"))
        })?;
    }
    Ok(())
}

/// How to put a used copy back as the fresh copy at `url` is: empty its
/// tables (all but the migrations' record) and start its sequences again.
/// `None` when that wouldn't give the template back: a table the
/// migrations put rows in, a sequence they moved on, or a materialized
/// view; such copies are dropped after each test instead.
fn how_to_reset(url: &str) -> Result<Option<String>, String> {
    let url = url.to_string();
    std::thread::spawn(move || -> Result<Option<String>, String> {
        let runtime = jadpo_host_native::Runtime::new().map_err(|e| e.to_string())?;
        let store = jadpo_host_native::postgres(&url).map_err(|e| format!("{e:?}"))?;
        runtime.block_on(async {
            use jadpo_runtime::{Database, SqlValue};
            let mut transaction = store.begin().await.map_err(|e| format!("{e:?}"))?;
            let mut read = async |sql: &str| -> Result<Vec<Vec<SqlValue>>, String> {
                transaction
                    .query(sql, &[])
                    .await
                    .map_err(|e| format!("{e:?}"))
            };
            let user = "schemaname NOT IN ('pg_catalog', 'information_schema')";
            let tables = read(&format!(
                "SELECT format('%I.%I', schemaname, tablename) FROM pg_tables \
                 WHERE {user} AND tablename <> '{}' ORDER BY 1",
                jadpo_runtime::MIGRATIONS_TABLE
            ))
            .await?;
            let sequences = read(&format!(
                "SELECT format('%I.%I', schemaname, sequencename), last_value IS NULL \
                 FROM pg_sequences WHERE {user} ORDER BY 1"
            ))
            .await?;
            let views = read(&format!(
                "SELECT count(*)::int4 FROM pg_matviews WHERE {user}"
            ))
            .await?;
            let text = |row: &Vec<SqlValue>| match row.first() {
                Some(SqlValue::Text(name)) => Ok(name.clone()),
                other => Err(format!("a catalogue query gave {other:?}")),
            };
            let tables = tables
                .iter()
                .map(text)
                .collect::<Result<Vec<String>, String>>()?;
            if !matches!(
                views.first().and_then(|row| row.first()),
                Some(SqlValue::Int4(0))
            ) {
                return Ok(None);
            }
            for table in &tables {
                let rows = read(&format!("SELECT 1 FROM {table} LIMIT 1")).await?;
                if !rows.is_empty() {
                    return Ok(None);
                }
            }
            let mut reset = Vec::new();
            if !tables.is_empty() {
                reset.push(format!(
                    "TRUNCATE {} RESTART IDENTITY CASCADE",
                    tables.join(", ")
                ));
            }
            for row in &sequences {
                if !matches!(row.get(1), Some(SqlValue::Bool(true))) {
                    return Ok(None);
                }
                reset.push(format!("ALTER SEQUENCE {} RESTART", text(row)?));
            }
            drop(read);
            let _ = transaction.rollback().await;
            Ok(Some(reset.join(";\n")))
        })
    })
    .join()
    .map_err(|_| "a server query's thread panicked".to_string())?
}

/// The host an action test runs on when the project has tables: its own
/// copy of each store's template, taken from the run's [`TestDatabases`]
/// the first time the test uses a store, and given back when it ends.
#[allow(missing_debug_implementations)]
pub struct TestHost<'d> {
    databases: &'d TestDatabases,
    held: std::cell::OnceCell<Held>,
    /// The in-memory store every `files` connection is in a test (area
    /// 13, "Provider calls and capabilities in tests"; chunk 6.4b).
    files: jadpo_runtime::files::MemoryFiles,
    /// The connections that keep files, which it is given for (chunk
    /// 7.1c).
    file_stores: Vec<String>,
    /// Every email a test sends, caught rather than sent (area 16,
    /// "Email"; chunk 7.3): what `email.sent` will read (chunk 7.3c).
    mail: jadpo_runtime::capability::Fake,
    /// The connection `email:` names, which it is given for.
    mail_connection: Option<String>,
    /// The authenticators `test_passkeys.register` made, each holding one
    /// passkey, by number (area 8; chunk 5.5z2).
    authenticators: TestPasskeyAuthenticators,
    /// The record its stores keep each statement in, when the run keeps
    /// one (chunk 8.1c).
    record: Option<std::rc::Rc<jadpo_runtime::record::Record>>,
}

/// A test's software authenticators: one device per passkey
/// `test_passkeys.register` adds, which only a test's host has.
#[derive(Default)]
struct TestPasskeyAuthenticators(
    std::cell::RefCell<Vec<jadpo_host_native::test_authenticator::TestAuthenticator>>,
);

impl jadpo_runtime::TestAuthenticators for TestPasskeyAuthenticators {
    fn create(&self, origin: &str, options: &str) -> Result<(u32, String), String> {
        let mut authenticators = self.0.borrow_mut();
        let number = u32::try_from(authenticators.len()).map_err(|e| e.to_string())?;
        // Its passkeys the same every run, so a test's record is (chunk
        // 8.1c); each device of a test its own.
        let mut device = jadpo_host_native::test_authenticator::TestAuthenticator::seeded(
            origin,
            u64::from(number),
        );
        let answer = device.create(options)?;
        authenticators.push(device);
        Ok((number, answer))
    }

    fn get(&self, device: u32, options: &str) -> Result<String, String> {
        let mut authenticators = self.0.borrow_mut();
        let device = authenticators
            .get_mut(device as usize)
            .ok_or("no such device in this test")?;
        device.get(options)
    }
}

enum Held {
    Holding {
        copy: Copy,
        stores: BTreeMap<String, Store>,
    },
    /// No database could be had: every store says why.
    Failed(Unready),
}

/// One of a test's stores: its own copy, whose statements are kept in the
/// run's record when it keeps one (chunk 8.1c).
enum Store {
    Plain(jadpo_host_native::Postgres),
    Recorded(jadpo_runtime::record::Recorded<jadpo_host_native::Postgres>),
}

impl Store {
    fn database(&self) -> &dyn jadpo_runtime::Database {
        match self {
            Store::Plain(database) => database,
            Store::Recorded(recorded) => recorded,
        }
    }

    /// The copy itself, whose statements no record keeps.
    fn raw(&self) -> &jadpo_host_native::Postgres {
        match self {
            Store::Plain(database) => database,
            Store::Recorded(recorded) => recorded.inner(),
        }
    }
}

/// What a test's database holds when it ends (chunk 8.1c): the rows of
/// each of the application's and the runtime's tables, but the migrations'
/// own and an extension's, by table and in column order, as JSON; a
/// `bytea` column by its length only, since it holds what is sealed, hashed
/// or encrypted. The tables in name order, each table's rows in their
/// text's order, so the same data always reads the same.
const TABLES: &str = "SELECT c.relname::text, a.attname::text, format_type(a.atttypid, a.atttypmod)::text \
    FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
    JOIN pg_attribute a ON a.attrelid = c.oid AND a.attnum > 0 AND NOT a.attisdropped \
    WHERE n.nspname = 'public' AND c.relkind IN ('r', 'p') AND NOT c.relispartition \
    AND c.relname <> 'jadpo_migrations' \
    AND NOT EXISTS (SELECT 1 FROM pg_depend d WHERE d.classid = 'pg_class'::regclass \
    AND d.objid = c.oid AND d.deptype = 'e') \
    ORDER BY c.relname, a.attnum";

/// Each row `database` holds, as [`TABLES`] reads them: `<store>.<table>
/// <row as JSON>`.
async fn rows_of(
    store: &str,
    database: &dyn jadpo_runtime::Database,
) -> Result<Vec<String>, String> {
    use jadpo_runtime::SqlValue;
    let mut transaction = database.begin().await.map_err(|e| format!("{e:?}"))?;
    // Times read the same whatever zone the server is in.
    transaction
        .batch("SET LOCAL TIME ZONE 'UTC'")
        .await
        .map_err(|e| format!("{e:?}"))?;
    let columns = transaction
        .query(TABLES, &[])
        .await
        .map_err(|e| format!("{e:?}"))?;
    let mut tables: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for column in &columns {
        let [
            SqlValue::Text(table),
            SqlValue::Text(name),
            SqlValue::Text(ty),
        ] = column.as_slice()
        else {
            continue;
        };
        let quoted = format!("\"{}\"", name.replace('"', "\"\""));
        let read = if ty == "bytea" {
            format!("'<' || octet_length({quoted}) || ' bytes>' AS {quoted}")
        } else {
            quoted
        };
        tables.entry(table.clone()).or_default().push(read);
    }
    let mut out = Vec::new();
    for (table, columns) in &tables {
        let sql = format!(
            "SELECT row_to_json(r)::text FROM (SELECT {} FROM \"{}\") r",
            columns.join(", "),
            table.replace('"', "\"\"")
        );
        let read = transaction
            .query(&sql, &[])
            .await
            .map_err(|e| format!("{e:?}"))?;
        let mut rows: Vec<String> = read
            .into_iter()
            .filter_map(|row| match row.into_iter().next() {
                Some(SqlValue::Text(json)) => Some(json),
                _ => None,
            })
            .collect();
        rows.sort();
        out.extend(rows.into_iter().map(|row| format!("{store}.{table} {row}")));
    }
    let _ = transaction.rollback().await;
    Ok(out)
}

/// A store that refuses with the reason the test's database couldn't be
/// had, so the test reports it as a fault at the place that used it.
struct Unready(String);

impl jadpo_runtime::Database for Unready {
    fn begin_at(
        &self,
        _isolation: jadpo_runtime::Isolation,
    ) -> jadpo_runtime::LocalFuture<
        '_,
        Result<Box<dyn jadpo_runtime::Transaction>, jadpo_runtime::StoreError>,
    > {
        let reason = self.0.clone();
        Box::pin(async move { Err(jadpo_runtime::StoreError::Fault(reason)) })
    }
}

impl<'d> TestHost<'d> {
    pub fn new(databases: &'d TestDatabases) -> TestHost<'d> {
        TestHost {
            databases,
            held: std::cell::OnceCell::new(),
            files: jadpo_runtime::files::MemoryFiles::new(),
            file_stores: Vec::new(),
            mail: jadpo_runtime::capability::Fake::new(),
            mail_connection: None,
            authenticators: TestPasskeyAuthenticators::default(),
            record: None,
        }
    }

    /// Keeps each statement its stores run in `record` (chunk 8.1c).
    pub fn with_record(
        mut self,
        record: Option<std::rc::Rc<jadpo_runtime::record::Record>>,
    ) -> TestHost<'d> {
        self.record = record;
        self
    }

    /// The rows its stores hold now, each `<store>.<table> <row as
    /// JSON>`, tables in name order and each table's rows in order (chunk
    /// 8.1c); none when the test used no store. A store that can't be read
    /// says why in a row of its own.
    pub fn rows(&self, runtime: &jadpo_host_native::Runtime) -> Vec<String> {
        let Some(Held::Holding { stores, .. }) = self.held.get() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for (name, store) in stores {
            match runtime.block_on(rows_of(name, store.raw())) {
                Ok(rows) => out.extend(rows),
                Err(error) => out.push(format!("{name} unread {error}")),
            }
        }
        out
    }

    /// Catches every email sent through `connection`, the one `email:`
    /// names, rather than sending it (area 16; chunk 7.3).
    pub fn with_mail(mut self, connection: Option<String>) -> TestHost<'d> {
        self.mail_connection = connection;
        self
    }

    /// Gives its store of files for the connections in `stores`, and
    /// nothing for any other (chunk 7.1c).
    pub fn with_file_stores<'s>(
        mut self,
        stores: impl IntoIterator<Item = &'s str>,
    ) -> TestHost<'d> {
        self.file_stores = stores.into_iter().map(str::to_string).collect();
        self
    }

    fn held(&self) -> &Held {
        self.held.get_or_init(|| match self.databases.acquire() {
            Ok(copy) => {
                let mut stores = BTreeMap::new();
                for (store, (_, url)) in &copy.stores {
                    match jadpo_host_native::postgres(url) {
                        Ok(database) => {
                            let database = match &self.record {
                                Some(record) => {
                                    Store::Recorded(jadpo_runtime::record::Recorded::new(
                                        database,
                                        std::rc::Rc::clone(record),
                                    ))
                                }
                                None => Store::Plain(database),
                            };
                            stores.insert(store.clone(), database);
                        }
                        Err(error) => {
                            return Held::Failed(Unready(format!(
                                "the test database for `{store}` can't be used: {error:?}"
                            )));
                        }
                    }
                }
                Held::Holding { copy, stores }
            }
            Err(error) => Held::Failed(Unready(format!(
                "the test database couldn't be made: {error}"
            ))),
        })
    }

    /// Ends the test: its copy goes back, and the names of the databases
    /// kept for a failed test come back to report.
    pub fn finish(self, failed: bool) -> Vec<String> {
        match self.held.into_inner() {
            Some(Held::Holding { copy, stores }) => {
                drop(stores);
                self.databases.release(copy, failed)
            }
            _ => Vec::new(),
        }
    }
}

/// The passkey ceremonies a test's requests run, as a native host's do.
static PASSKEYS: jadpo_host_native::passkeys::WebauthnPasskeys =
    jadpo_host_native::passkeys::WebauthnPasskeys;

impl jadpo_runtime::Host for TestHost<'_> {
    /// `webauthn-rs`, as natively (area 8, "Passkeys").
    fn passkeys(&self) -> Option<&dyn jadpo_runtime::Passkeys> {
        Some(&PASSKEYS)
    }

    /// The authenticators `test_passkeys` makes (chunk 5.5z2).
    fn test_authenticators(&self) -> Option<&dyn jadpo_runtime::TestAuthenticators> {
        Some(&self.authenticators)
    }

    fn database(&self, store: &str) -> Option<&dyn jadpo_runtime::Database> {
        match self.held() {
            Held::Holding { stores, .. } => stores.get(store).map(Store::database),
            Held::Failed(unready) => Some(unready),
        }
    }

    /// None: a test runs under its seed, so it reproduces.
    fn random(&self, _bytes: &mut [u8]) -> bool {
        false
    }

    /// A store of files' connection in memory (chunk 6.4b), and only
    /// one (chunk 7.1c), and the email connection's mail caught (chunk
    /// 7.3): nothing in a test leaves the process, and a package's
    /// connection has no driver until provider calls are built (chunk
    /// 7.2c), where a test fakes it.
    fn capability(&self, connection: &str) -> Option<&dyn jadpo_runtime::capability::Capability> {
        if self.mail_connection.as_deref() == Some(connection) {
            return Some(&self.mail);
        }
        self.file_stores
            .iter()
            .any(|store| store == connection)
            .then_some(&self.files as &dyn jadpo_runtime::capability::Capability)
    }

    /// Two rows a batch, so every file a test reads resumes between
    /// batches (area 13, "Provider calls and capabilities in tests").
    fn rows_per_batch(&self) -> u32 {
        2
    }
}
