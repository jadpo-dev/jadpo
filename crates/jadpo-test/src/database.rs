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
//! copies are prepared in the background, a few ahead of the tests that
//! will take them, so a test seldom waits for one. A test's copy is dropped
//! when it ends, also in the background, unless it failed and the run
//! keeps failed tests' databases.

use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::thread::JoinHandle;

use jadpo_check::data::migrations::Step;

use crate::postgres::{Server, ServerError};

/// How many copies are kept ready beyond the ones tests hold.
const AHEAD: usize = 4;

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
        std::process::id(),
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

/// The server and each store's template, by store.
type Started = (Arc<Server>, BTreeMap<String, String>);

#[derive(Default)]
struct State {
    /// The server and each store's template, once the first test asked.
    ready: Option<Result<Started, String>>,
    /// Copies prepared and not yet taken.
    pool: VecDeque<Copy>,
    /// How many copies tests want ready.
    wanted: usize,
    /// Databases to drop.
    drop: Vec<String>,
    /// The run is over: the worker drops what is left and stops.
    done: bool,
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
        self.start_worker();
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

    /// Gives a test's copy back: dropped in the background, unless the
    /// test failed and the run keeps failed tests' databases, when its
    /// names are given back to report.
    pub fn release(&self, copy: Copy, failed: bool) -> Vec<String> {
        let names: Vec<String> = copy.stores.values().map(|(name, _)| name.clone()).collect();
        if failed && self.shared.keep_failed {
            return names;
        }
        if let Ok(mut state) = self.shared.state.lock() {
            state.drop.extend(names);
            self.shared.changed.notify_all();
        }
        Vec::new()
    }

    fn start_worker(&self) {
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

/// The background worker: starts the server and builds the templates,
/// then keeps copies ready and drops the ones tests gave back, until the
/// run is over.
fn prepare(shared: &Shared) {
    let ready = (|| -> Result<Started, String> {
        let server = server()?;
        let mut templates = BTreeMap::new();
        for (store, steps) in &shared.steps {
            templates.insert(store.clone(), template(&server, store, steps)?);
        }
        Ok((server, templates))
    })();
    let (server, templates) = match &ready {
        Ok((server, templates)) => (Arc::clone(server), templates.clone()),
        Err(_) => {
            if let Ok(mut state) = shared.state.lock() {
                state.ready = Some(ready);
                shared.changed.notify_all();
            }
            return;
        }
    };
    if let Ok(mut state) = shared.state.lock() {
        state.ready = Some(ready);
        shared.changed.notify_all();
    }
    static NEXT: AtomicU64 = AtomicU64::new(0);
    loop {
        let (work, done) = {
            let Ok(mut state) = shared.state.lock() else {
                return;
            };
            loop {
                let short = state.pool.len() < state.wanted + AHEAD && !state.done;
                if short || !state.drop.is_empty() || state.done {
                    break;
                }
                state = match shared.changed.wait(state) {
                    Ok(state) => state,
                    Err(_) => return,
                };
            }
            let drop = std::mem::take(&mut state.drop);
            let make = state.pool.len() < state.wanted + AHEAD && !state.done;
            ((drop, make), state.done)
        };
        let (drop, make) = work;
        if make {
            let mut stores = BTreeMap::new();
            let mut failed = None;
            for (store, template) in &templates {
                let name = format!(
                    "jadpo_test_{}_{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                );
                match admin_retrying(
                    &server,
                    &format!("CREATE DATABASE \"{name}\" TEMPLATE \"{template}\""),
                ) {
                    Ok(()) => {
                        stores.insert(store.clone(), (name.clone(), server.url(&name)));
                    }
                    Err(error) => {
                        failed = Some(error);
                        break;
                    }
                }
            }
            let Ok(mut state) = shared.state.lock() else {
                return;
            };
            match failed {
                None => state.pool.push_back(Copy { stores }),
                Some(error) => {
                    state
                        .drop
                        .extend(stores.into_values().map(|(name, _)| name));
                    state.ready = Some(Err(format!("copying a template failed: {error}")));
                }
            }
            shared.changed.notify_all();
        }
        for name in drop {
            let _ = drop_database(&server, &name);
        }
        if done {
            let left: Vec<String> = match shared.state.lock() {
                Ok(mut state) => {
                    let mut left: Vec<String> = state
                        .pool
                        .drain(..)
                        .flat_map(|copy| copy.stores.into_values().map(|(name, _)| name))
                        .collect();
                    left.append(&mut state.drop);
                    left
                }
                Err(_) => Vec::new(),
            };
            for name in left {
                let _ = drop_database(&server, &name);
            }
            return;
        }
    }
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
