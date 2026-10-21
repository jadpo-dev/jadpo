//! A compiled deployable on the native host (roadmap chunk 8.1o;
//! docs/compiler-runtime.md §2.6, "A native deployable"): what the binary
//! `jadpo build` writes runs, from its `main` to its last log line.
//!
//! [`run`] starts the program the generated crate holds the way area 10
//! says an application starts: it reads only the variables its settings
//! name, through `config::load`, and refuses to start, naming each setting
//! and its variable but never a value, when one is missing or invalid. It
//! builds each connection's driver through the factory (chunk 8.1d) and
//! checks every database it keeps migrations in against the migrations the
//! build holds: one not applied, or a database that applied one the build
//! doesn't have, stops the start (area 5, "Migrations"), and a database
//! that can't be reached yet is waited for, since a database that is down
//! is not a configuration error (area 10). Only then does it open its
//! port: `PORT` (8080 when unset) on `HOST` (every IPv4 interface when
//! unset), with a worker per core, each serving and ticking through the
//! runtime's `Serving` on its own `NativeHost`
//! ([`crate::http::AppHandler`]). The
//! first SIGTERM or ctrl-c stops it: it accepts nothing more, lets each
//! connection give the answer it owes and its background work give back
//! what it holds ([`crate::http::Server::stop`]), then ends.
//!
//! What it says is one JSON object a line on standard output, each
//! carrying the build stamp (area 18, "Production builds": the build id,
//! the commit, whether the tree was modified, the compiler, the deployable
//! and the target), so every event names the build it came from:
//! `runtime.startup_failed` with every problem that stopped the start,
//! `runtime.waiting` while a database can't be reached, `runtime.started`
//! with where it listens and each connection whose driver couldn't be
//! built (which the runtime answers `misconfigured`), `operation.failed`
//! for a fault behind a 500 (area 17, "Errors point at the `.jadpo`
//! line"), and `runtime.stopped`.

use std::io::Write;
use std::net::{SocketAddr, TcpListener, ToSocketAddrs};
use std::process::ExitCode;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use jadpo_drivers::factory::{Unbuilt, Why};
use jadpo_runtime::compiled::Program;
use jadpo_runtime::config::{self, ConfigValues, Mode, ProblemKind, StartProblem};
use jadpo_runtime::http::{Request, Response};
use jadpo_runtime::serve::{Answered, Observer};
use jadpo_runtime::{Executor, LocalFuture, MigrateError, StoreError};
use jadpo_std::Unreadable;
use serde_json::{Map, Value, json};

use crate::http::{AppHandler, Handler, MakeHandler, WorkerThread, serve};
use crate::{Drivers, Runtime, supplies};

/// What a generated deployable's `main` hands [`run`].
#[derive(Clone, Copy, Debug)]
pub struct Deployable {
    /// The compiled program on the calling thread: the generated crate's
    /// `program`, which decodes its plan once for the process.
    pub program: fn() -> Result<Program, Unreadable>,
    pub stamp: Stamp,
    /// Each database the project keeps migrations in, with its migrations
    /// in the order they apply, as the project held them when it was built.
    pub stores: &'static [Store],
}

/// The build stamp (area 18, "Production builds"; docs/compiler-runtime.md
/// §2.8): a constant in the binary, and on every event it logs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stamp {
    /// The build id, `b_` and twelve hex digits of a digest of the source
    /// the deployable reaches, the compiler, the target and the lock.
    pub build: &'static str,
    /// The commit it was built from, when the project is in git.
    pub commit: Option<&'static str>,
    /// Whether the tree held changes the commit doesn't, or had no commit.
    pub modified: bool,
    /// The Jadpo version that built it.
    pub compiler: &'static str,
    pub deployable: &'static str,
    /// `native`.
    pub target: &'static str,
}

/// A database the project keeps migrations in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Store {
    /// The store's connection.
    pub name: &'static str,
    pub migrations: &'static [Migration],
}

/// One migration of a store: its folder's name and the digest the store
/// records for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Migration {
    pub name: &'static str,
    pub digest: &'static str,
}

/// The port a deployable listens on when `PORT` doesn't say.
pub const DEFAULT_PORT: u16 = 8080;

/// The longest a deployable waits between tries at a database it can't
/// reach yet as it starts.
const MOST_WAIT: Duration = Duration::from_secs(10);

/// Runs `deployable` until it is stopped: its exit code, 1 when it
/// couldn't start.
pub fn run(deployable: &Deployable) -> ExitCode {
    let log = Log {
        stamp: deployable.stamp,
    };
    match start(deployable, &log) {
        Ok(()) => ExitCode::SUCCESS,
        Err(problems) => {
            log.event(
                "runtime.startup_failed",
                [("problems", Value::Array(problems))],
            );
            ExitCode::from(1)
        }
    }
}

/// Everything from reading the settings to stopping; the problems that
/// stopped the start, as log objects.
fn start(deployable: &Deployable, log: &Log) -> Result<(), Vec<Value>> {
    let stop = crate::stop_signals().map_err(|error| {
        vec![problem(
            "signals",
            format!("SIGTERM and ctrl-c couldn't be watched: {error}"),
        )]
    })?;
    let program = (deployable.program)().map_err(|error| {
        vec![problem(
            "plan_unreadable",
            format!("The program's plan can't be read: {error}."),
        )]
    })?;
    let environment = |name: &str| std::env::var(name).ok();
    let values = config::load(
        &program.runtime().config,
        program.validators(),
        &environment,
        Mode::Environment,
        &|_: &mut [u8]| false,
    )
    .map_err(|problems| problems.iter().map(setting_problem).collect::<Vec<_>>())?;
    let address = listen_address(environment("HOST"), environment("PORT"))
        .map_err(|why| vec![problem("address_invalid", why)])?;
    let program = program.with_config(&values);
    let unbuilt = check_stores(deployable, &program, &stop, log)?;
    if stop.load(Ordering::SeqCst) {
        log.event("runtime.stopped", []);
        return Ok(());
    }
    let listener = TcpListener::bind(address).map_err(|error| {
        vec![problem(
            "listen_failed",
            format!("It couldn't listen on {address}: {error}."),
        )]
    })?;
    let workers = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get);
    let server = serve(
        listener,
        workers,
        handlers(deployable.program, Arc::new(values), *log),
    )
    .map_err(|error| {
        vec![problem(
            "listen_failed",
            format!("Its workers couldn't start: {error}."),
        )]
    })?;
    log.event(
        "runtime.started",
        [
            ("address", Value::from(server.address().to_string())),
            ("workers", Value::from(workers)),
            (
                "unbuilt",
                Value::Array(
                    unbuilt
                        .iter()
                        .map(|unbuilt| {
                            json!({
                                "connection": unbuilt.connection,
                                "driver": unbuilt.driver,
                                "message": unbuilt.to_string(),
                            })
                        })
                        .collect(),
                ),
            ),
        ],
    );
    while !stop.load(Ordering::SeqCst) {
        std::thread::sleep(Duration::from_millis(50));
    }
    server.stop();
    log.event("runtime.stopped", []);
    Ok(())
}

/// Each worker's handler: the program on the worker's thread with the
/// settings the start read, its drivers built there by the factory, served
/// through `Serving` on its `NativeHost`.
fn handlers(
    program: fn() -> Result<Program, Unreadable>,
    values: Arc<ConfigValues>,
    log: Log,
) -> MakeHandler {
    Arc::new(move |worker: &WorkerThread| -> Box<dyn Handler> {
        // The plan decoded once for the process, which `start` has
        // already done, so this can't fail here.
        let Ok(program) = program() else {
            return Box::new(Unstarted);
        };
        let program = program.with_config(&values);
        let drivers = match program.started() {
            Some(started) => Drivers::build(&program.runtime().connections, &started, &supplies()),
            None => Drivers::default(),
        };
        let program: Rc<dyn Executor> = Rc::new(program);
        let host = worker.host().with_drivers(drivers);
        Box::new(
            AppHandler::new(host, move || Rc::clone(&program)).observed_by(Rc::new(Faults(log))),
        )
    })
}

/// Checks each store's migrations against the database, waiting for one
/// that can't be reached yet: the connections whose drivers weren't built,
/// for the start to report, or what stopped it.
fn check_stores(
    deployable: &Deployable,
    program: &Program,
    stop: &AtomicBool,
    log: &Log,
) -> Result<Vec<Unbuilt>, Vec<Value>> {
    let runtime = Runtime::new().map_err(|error| {
        vec![problem(
            "runtime",
            format!("Its runtime couldn't start: {error}."),
        )]
    })?;
    let drivers = match program.started() {
        Some(started) => Drivers::build(&program.runtime().connections, &started, &supplies()),
        None => Drivers::default(),
    };
    let mut problems = Vec::new();
    for store in deployable.stores {
        let Some(database) = drivers.database(store.name) else {
            let why = drivers
                .unbuilt()
                .iter()
                .find(|unbuilt| unbuilt.connection == store.name);
            problems.push(unaddressed(store.name, why));
            continue;
        };
        let migrations: Vec<jadpo_std::Migration> = store
            .migrations
            .iter()
            .map(|migration| jadpo_std::Migration {
                name: migration.name.to_string(),
                digest: migration.digest.to_string(),
                sql: String::new(),
            })
            .collect();
        let mut wait = Duration::from_secs(1);
        let mut said = false;
        loop {
            match runtime.block_on(jadpo_runtime::standing(database, &migrations)) {
                Ok(standing) => {
                    if !standing.pending.is_empty() {
                        problems.push(pending(store.name, &standing.pending));
                    }
                    break;
                }
                Err(MigrateError::Store(StoreError::Unavailable)) => {
                    if !said {
                        log.event(
                            "runtime.waiting",
                            [
                                ("store", Value::from(store.name)),
                                (
                                    "message",
                                    Value::from(format!(
                                        "`{}` can't be reached yet: the app starts once it can check the database's migrations.",
                                        store.name
                                    )),
                                ),
                            ],
                        );
                        said = true;
                    }
                    if waited(stop, wait) {
                        return Ok(drivers.unbuilt().to_vec());
                    }
                    wait = (wait * 2).min(MOST_WAIT);
                }
                Err(error) => {
                    problems.push(diverged(store.name, &error));
                    break;
                }
            }
        }
    }
    if problems.is_empty() {
        Ok(drivers.unbuilt().to_vec())
    } else {
        Err(problems)
    }
}

/// Sleeps `wait`, a little at a time: whether a stop came meanwhile.
fn waited(stop: &AtomicBool, wait: Duration) -> bool {
    let step = Duration::from_millis(50);
    let mut slept = Duration::ZERO;
    while slept < wait {
        if stop.load(Ordering::SeqCst) {
            return true;
        }
        std::thread::sleep(step);
        slept += step;
    }
    stop.load(Ordering::SeqCst)
}

/// Where the deployable listens: `host` (every IPv4 interface when unset)
/// and `port` ([`DEFAULT_PORT`] when unset), as `HOST` and `PORT` give
/// them.
pub fn listen_address(host: Option<String>, port: Option<String>) -> Result<SocketAddr, String> {
    let host = host
        .filter(|host| !host.is_empty())
        .unwrap_or_else(|| "0.0.0.0".to_string());
    let port = match port.filter(|port| !port.is_empty()) {
        None => DEFAULT_PORT,
        Some(port) => port.parse::<u16>().map_err(|_| {
            "`PORT` isn't a port: it is a whole number from 0 to 65535.".to_string()
        })?,
    };
    // An IPv6 address is written in brackets beside a port.
    let bare = host.trim_start_matches('[').trim_end_matches(']');
    if let Ok(ip) = bare.parse::<std::net::IpAddr>() {
        return Ok(SocketAddr::new(ip, port));
    }
    (host.as_str(), port)
        .to_socket_addrs()
        .ok()
        .and_then(|mut found| found.next())
        .ok_or_else(|| format!("`HOST` names no address this machine can listen on: `{host}`."))
}

/// A problem with its code and what to say about it.
fn problem(code: &str, message: impl Into<String>) -> Value {
    json!({ "problem": code, "message": message.into() })
}

/// A setting that stops the start, named with its variable, never its
/// value (area 10).
fn setting_problem(found: &StartProblem) -> Value {
    let (code, message) = match &found.kind {
        ProblemKind::Missing => (
            "setting_missing",
            format!(
                "`{}` isn't set: `{}` is required, so the app can't start without it.",
                found.variable, found.field
            ),
        ),
        ProblemKind::Invalid { rule } => (
            "setting_invalid",
            format!(
                "`{}` isn't a valid `{}`: it breaks `{rule}`.",
                found.variable, found.field
            ),
        ),
    };
    let mut out = Map::new();
    out.insert("problem".into(), code.into());
    out.insert("setting".into(), found.field.as_str().into());
    out.insert("variable".into(), found.variable.as_str().into());
    if let ProblemKind::Invalid { rule } = &found.kind {
        out.insert("rule".into(), rule.as_str().into());
    }
    out.insert("message".into(), message.into());
    Value::Object(out)
}

/// A store whose database has no driver: its connection names no address,
/// or the factory couldn't build it.
fn unaddressed(store: &str, why: Option<&Unbuilt>) -> Value {
    let message = match why.map(|unbuilt| &unbuilt.why) {
        Some(Why::Unset(None)) | None => format!(
            "`{store}` names no address: give its connection in `app.jadpo` a `url:` read from a setting."
        ),
        Some(_) => why.map_or_else(String::new, |unbuilt| format!("{unbuilt}.")),
    };
    json!({ "problem": "store_unbuilt", "store": store, "message": message })
}

/// A store whose database hasn't applied every migration the build holds.
fn pending(store: &str, names: &[String]) -> Value {
    let count = names.len();
    json!({
        "problem": "migrations_pending",
        "store": store,
        "pending": names,
        "message": format!(
            "`{store}` hasn't applied {} the app was built with: run `jadpo migrate apply` against it, then start the app again.",
            if count == 1 { "1 migration".to_string() } else { format!("{count} migrations") }
        ),
    })
}

/// A store whose database doesn't match the build's migrations, or
/// couldn't be read.
fn diverged(store: &str, error: &MigrateError) -> Value {
    let (code, message) = match error {
        MigrateError::Unknown { name } => (
            "migrations_diverged",
            format!(
                "`{store}` applied `{name}`, which this build doesn't have: it was built from older code than the database runs."
            ),
        ),
        MigrateError::Changed { name } => (
            "migrations_diverged",
            format!(
                "`{store}` applied `{name}` as it was before it changed: the migration was edited after it ran."
            ),
        ),
        MigrateError::OutOfOrder { name, missing } => (
            "migrations_diverged",
            format!("`{store}` applied `{name}` but not `{missing}`, which comes before it."),
        ),
        MigrateError::Store(error) => (
            "store_refused",
            format!("`{store}` couldn't be read: {}.", store_error(error)),
        ),
        other => (
            "store_refused",
            format!("`{store}`'s migrations couldn't be checked: {other:?}."),
        ),
    };
    json!({ "problem": code, "store": store, "message": message })
}

/// A store's error in words, never an address.
fn store_error(error: &StoreError) -> String {
    match error {
        StoreError::Fault(why) | StoreError::Statement(why) => why.clone(),
        StoreError::Unavailable => "it can't be reached".into(),
        StoreError::ConcurrentChange => "another change got there first".into(),
        StoreError::Conflict { constraint } => format!("`{constraint}` refused it"),
    }
}

/// The log: one JSON object a line on standard output, each carrying the
/// build stamp.
#[derive(Clone, Copy, Debug)]
struct Log {
    stamp: Stamp,
}

impl Log {
    fn event<const N: usize>(&self, event: &str, fields: [(&str, Value); N]) {
        let line = self.line(event, fields);
        let mut out = std::io::stdout().lock();
        let _ = writeln!(out, "{line}");
        let _ = out.flush();
    }

    /// `event` with the stamp, then `fields`, as one line of JSON.
    fn line<const N: usize>(&self, event: &str, fields: [(&str, Value); N]) -> String {
        let mut out = Map::new();
        out.insert("event".into(), event.into());
        out.insert("build".into(), self.stamp.build.into());
        out.insert(
            "commit".into(),
            self.stamp.commit.map_or(Value::Null, Value::from),
        );
        out.insert("modified".into(), self.stamp.modified.into());
        out.insert("compiler".into(), self.stamp.compiler.into());
        out.insert("deployable".into(), self.stamp.deployable.into());
        out.insert("target".into(), self.stamp.target.into());
        for (key, value) in fields {
            out.insert(key.into(), value);
        }
        Value::Object(out).to_string()
    }
}

/// Logs each fault behind a 500 with its `.jadpo` frames (areas 4 and 17).
struct Faults(Log);

impl Observer for Faults {
    fn answered(&self, answered: &Answered<'_>) {
        let Some(fault) = &answered.response.fault else {
            return;
        };
        self.0.event(
            "operation.failed",
            [
                ("classification", fault.kind.classification().into()),
                (
                    "route",
                    answered
                        .response
                        .route
                        .as_deref()
                        .map_or(Value::Null, Value::from),
                ),
                (
                    "request_id",
                    answered
                        .response
                        .header("x-request-id")
                        .map_or(Value::Null, Value::from),
                ),
                (
                    "stack",
                    Value::Array(
                        fault
                            .stack(answered.program.places())
                            .into_iter()
                            .map(Value::from)
                            .collect(),
                    ),
                ),
            ],
        );
    }
}

/// A worker whose program couldn't be made, which never happens once the
/// start has made it: it answers 503.
struct Unstarted;

impl Handler for Unstarted {
    fn handle(&self, _request: Request) -> LocalFuture<'_, Response> {
        Box::pin(async {
            Response {
                status: 503,
                ..Response::default()
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STAMP: Stamp = Stamp {
        build: "b_3c41d07a9e12",
        commit: Some("8d113e6"),
        modified: false,
        compiler: "0.1.0",
        deployable: "app",
        target: "native",
    };

    #[test]
    fn every_event_carries_the_stamp() {
        let line = Log { stamp: STAMP }.line("runtime.stopped", []);
        let read: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(
            read,
            json!({
                "event": "runtime.stopped",
                "build": "b_3c41d07a9e12",
                "commit": "8d113e6",
                "modified": false,
                "compiler": "0.1.0",
                "deployable": "app",
                "target": "native",
            })
        );
        let unstamped = Log {
            stamp: Stamp {
                commit: None,
                modified: true,
                ..STAMP
            },
        }
        .line("runtime.waiting", [("store", Value::from("main"))]);
        let read: Value = serde_json::from_str(&unstamped).unwrap();
        assert_eq!(read["commit"], Value::Null);
        assert_eq!(read["modified"], Value::Bool(true));
        assert_eq!(read["store"], "main");
    }

    #[test]
    fn it_listens_where_host_and_port_say() {
        assert_eq!(
            listen_address(None, None),
            Ok("0.0.0.0:8080".parse().unwrap())
        );
        assert_eq!(
            listen_address(Some("127.0.0.1".into()), Some("0".into())),
            Ok("127.0.0.1:0".parse().unwrap())
        );
        assert_eq!(
            listen_address(Some("::1".into()), Some("9000".into())),
            Ok("[::1]:9000".parse().unwrap())
        );
        assert_eq!(
            listen_address(Some(String::new()), Some(String::new())),
            Ok("0.0.0.0:8080".parse().unwrap())
        );
        assert!(
            listen_address(None, Some("eighty".into()))
                .unwrap_err()
                .contains("`PORT` isn't a port")
        );
        assert!(listen_address(None, Some("70000".into())).is_err());
    }

    #[test]
    fn a_setting_is_named_by_its_variable_never_its_value() {
        let missing = setting_problem(&StartProblem {
            field: "database_url".into(),
            variable: "DATABASE_URL".into(),
            secret: true,
            kind: ProblemKind::Missing,
        });
        assert_eq!(missing["problem"], "setting_missing");
        assert_eq!(missing["variable"], "DATABASE_URL");
        let invalid = setting_problem(&StartProblem {
            field: "database_url".into(),
            variable: "DATABASE_URL".into(),
            secret: true,
            kind: ProblemKind::Invalid {
                rule: "format".into(),
            },
        });
        assert_eq!(invalid["rule"], "format");
        assert_eq!(
            invalid["message"],
            "`DATABASE_URL` isn't a valid `database_url`: it breaks `format`."
        );
    }

    #[test]
    fn a_store_behind_names_its_pending_migrations() {
        let one = pending("main", &["20261020T0626_hold_deliveries".to_string()]);
        assert_eq!(one["pending"], json!(["20261020T0626_hold_deliveries"]));
        assert!(
            one["message"]
                .as_str()
                .unwrap()
                .starts_with("`main` hasn't applied 1 migration the app was built with")
        );
        let unnamed = unaddressed("main", None);
        assert!(
            unnamed["message"]
                .as_str()
                .unwrap()
                .contains("give its connection in `app.jadpo` a `url:`")
        );
    }
}
