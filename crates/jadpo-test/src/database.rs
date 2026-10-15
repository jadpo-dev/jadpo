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

use jadpo_check::data::migrations::Step;

use crate::postgres::{Server, ServerError};

/// How many copies are kept ready beyond the ones tests hold.
const AHEAD: usize = 4;

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
pub fn template_digest(steps: &[Step]) -> String {
    let mut hasher = blake3::Hasher::new();
    for step in steps {
        hasher.update(step.name.as_bytes());
        hasher.update(b"\0");
        hasher.update(step.digest.as_bytes());
        hasher.update(b"\0");
    }
    hasher.finalize().to_hex()[..16].to_string()
}

/// The template of one store: built once per process and digest.
fn template(server: &Server, store: &str, steps: &[Step]) -> Result<String, String> {
    static BUILT: OnceLock<Mutex<BTreeMap<String, Result<String, String>>>> = OnceLock::new();
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

fn build_template(server: &Server, name: &str, steps: &[Step]) -> Result<String, String> {
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
                Err(error)
            }
        }
    }
}

/// Applies `steps` to the database at `url`, on a runtime of its own that
/// is gone, with its connections, when this returns.
fn migrate(url: &str, steps: &[Step]) -> Result<(), String> {
    let url = url.to_string();
    let migrations: Vec<jadpo_runtime::Migration> = steps
        .iter()
        .map(|step| jadpo_runtime::Migration {
            name: step.name.clone(),
            digest: step.digest.clone(),
            sql: step.sql.clone(),
        })
        .collect();
    std::thread::spawn(move || -> Result<(), String> {
        let runtime = jadpo_host_native::Runtime::new().map_err(|e| e.to_string())?;
        let store = jadpo_host_native::postgres(&url).map_err(|e| format!("{e:?}"))?;
        runtime
            .block_on(jadpo_runtime::migrate(&store, &migrations))
            .map(|_| ())
            .map_err(|e| format!("applying the migrations failed: {e:?}"))
    })
    .join()
    .map_err(|_| "the migrating thread panicked".to_string())?
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
    steps: BTreeMap<String, Vec<Step>>,
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

/// The server and each store's template, by store.
type Started = (Arc<Server>, BTreeMap<String, Template>);

#[derive(Default)]
struct State {
    /// The server and each store's template, once the first test asked.
    ready: Option<Result<Started, String>>,
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
    /// The run is over: the workers drop what is left and stop.
    done: bool,
}

impl State {
    /// Whether the pool, with what is on its way, has fewer copies than
    /// the tests waiting and the few kept ahead of them.
    fn short(&self) -> bool {
        !self.done && self.pool.len() + self.making < self.wanted + AHEAD
    }
}

impl TestDatabases {
    /// The databases for a run whose stores have `steps`; nothing starts
    /// until a test asks for its database.
    pub fn new(steps: BTreeMap<String, Vec<Step>>, keep_failed: bool) -> TestDatabases {
        TestDatabases {
            shared: Arc::new(Shared {
                steps,
                keep_failed,
                state: Mutex::new(State::default()),
                changed: Condvar::new(),
            }),
            worker: Mutex::new(None),
        }
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
            let reusable = matches!(&state.ready, Some(Ok((_, templates)))
                if templates.values().all(|template| template.reset.is_some()));
            if reusable && !state.done {
                state.used.push_back(copy);
            } else {
                state.drop.extend(copy.names());
            }
            self.shared.changed.notify_all();
        }
        Vec::new()
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
    let ready = (|| -> Result<Started, String> {
        let server = server()?;
        let mut templates = BTreeMap::new();
        for (store, steps) in &shared.steps {
            let name = template(&server, store, steps)?;
            templates.insert(store.clone(), Template { name, reset: None });
        }
        Ok((server, templates))
    })();
    let (server, mut templates) = match ready {
        Ok(started) => started,
        Err(error) => {
            if let Ok(mut state) = shared.state.lock() {
                state.ready = Some(Err(error));
                shared.changed.notify_all();
            }
            return;
        }
    };
    // The first copy tells how to empty one again: a copy is the template
    // as it was migrated, so it is read before any test writes to it.
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
    Stop,
}

/// One worker: keeps copies ready, emptying the ones tests gave back
/// before making new ones, and drops what is no longer wanted; once the
/// run is over, drops everything left and stops.
fn work(shared: &Shared, server: &Server, templates: &BTreeMap<String, Template>) {
    // Emptying a copy runs on this worker's own runtime.
    let runtime = jadpo_host_native::Runtime::new().ok();
    loop {
        let job = {
            let Ok(mut state) = shared.state.lock() else {
                return;
            };
            loop {
                if let Some(name) = state.drop.pop() {
                    break Job::Drop(name);
                }
                if state.done {
                    let state = &mut *state;
                    let left = state.pool.drain(..).chain(state.used.drain(..));
                    state.drop.extend(left.flat_map(Copy::names));
                    match state.drop.pop() {
                        Some(name) => break Job::Drop(name),
                        None => break Job::Stop,
                    }
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
        let made = match job {
            Job::Stop => return,
            Job::Drop(name) => {
                let _ = drop_database(server, &name);
                continue;
            }
            Job::Make => make_copy(server, templates)
                .map_err(|error| format!("copying a template failed: {error}")),
            Job::Empty(copy) => match runtime
                .as_ref()
                .map(|runtime| empty(runtime, &copy, templates))
            {
                Some(Ok(())) => Ok(copy),
                // A copy that won't empty is dropped, and a new one made
                // in its place.
                _ => {
                    let made = make_copy(server, templates)
                        .map_err(|error| format!("copying a template failed: {error}"));
                    if let Ok(mut state) = shared.state.lock() {
                        state.drop.extend(copy.names());
                    }
                    made
                }
            },
        };
        let Ok(mut state) = shared.state.lock() else {
            return;
        };
        state.making = state.making.saturating_sub(1);
        match made {
            Ok(copy) if state.done => state.drop.extend(copy.names()),
            Ok(copy) => state.pool.push_back(copy),
            Err(error) => state.ready = Some(Err(error)),
        }
        shared.changed.notify_all();
    }
}

/// A new copy of each store's template, under names no other process
/// sharing the server uses.
fn make_copy(server: &Server, templates: &BTreeMap<String, Template>) -> Result<Copy, String> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let mut stores: BTreeMap<String, (String, String)> = BTreeMap::new();
    for (store, template) in templates {
        let name = format!(
            "jadpo_test_{}_{}",
            crate::postgres::process_tag(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
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
            return Err(format!("`{store}` can't be emptied"));
        };
        if reset.is_empty() {
            continue;
        }
        let database = jadpo_host_native::postgres(url).map_err(|e| format!("{e:?}"))?;
        runtime.block_on(async {
            use jadpo_runtime::Database;
            let mut transaction = database.begin().await.map_err(|e| format!("{e:?}"))?;
            transaction
                .batch(reset)
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
}

enum Held {
    Holding {
        copy: Copy,
        stores: BTreeMap<String, jadpo_host_native::Postgres>,
    },
    /// No database could be had: every store says why.
    Failed(Unready),
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
        }
    }

    fn held(&self) -> &Held {
        self.held.get_or_init(|| match self.databases.acquire() {
            Ok(copy) => {
                let mut stores = BTreeMap::new();
                for (store, (_, url)) in &copy.stores {
                    match jadpo_host_native::postgres(url) {
                        Ok(database) => {
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

impl jadpo_runtime::Host for TestHost<'_> {
    fn database(&self, store: &str) -> Option<&dyn jadpo_runtime::Database> {
        match self.held() {
            Held::Holding { stores, .. } => stores
                .get(store)
                .map(|database| database as &dyn jadpo_runtime::Database),
            Held::Failed(unready) => Some(unready),
        }
    }

    fn deliver(&self, _events: Vec<jadpo_runtime::Event>) -> jadpo_runtime::LocalFuture<'_, ()> {
        Box::pin(async {})
    }
}
