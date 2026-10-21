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
use jadpo_runtime::config::{self, ConfigValues, Mode};
use jadpo_runtime::deployable::{Compared, Faults, Log, Then, compare, problem, setting_problem};
use jadpo_runtime::http::{Request, Response};
use jadpo_runtime::{Executor, LocalFuture};
use jadpo_std::Unreadable;
use serde_json::{Value, json};

use crate::http::{AppHandler, Handler, MakeHandler, WorkerThread, serve};
use crate::{Drivers, Runtime, supplies};

/// What a generated deployable's `main` hands [`run`]: the program, its
/// build stamp and each store's migrations, shared with the Workers host
/// (chunk 8.2b).
pub use jadpo_runtime::deployable::{Deployable, Migration, Stamp, Store};

/// The port a deployable listens on when `PORT` doesn't say.
pub const DEFAULT_PORT: u16 = 8080;

/// The longest a deployable waits between tries at a database it can't
/// reach yet as it starts.
const MOST_WAIT: Duration = Duration::from_secs(10);

/// Runs `deployable` until it is stopped: its exit code, 1 when it
/// couldn't start.
pub fn run(deployable: &Deployable) -> ExitCode {
    let log = Log::new(deployable.stamp, standard_output);
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
            problems.push(unaddressed(store.name, drivers.unbuilt()));
            continue;
        };
        let mut wait = Duration::from_secs(1);
        let mut said = false;
        loop {
            match runtime.block_on(compare(database, store, Then::Restart)) {
                Compared::Current => break,
                Compared::Refused(problem) => {
                    problems.push(problem);
                    break;
                }
                Compared::Unreachable => {
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

/// A store whose database has no driver, given the connections the
/// factory couldn't build: its connection names no address, or the
/// factory says why not.
fn unaddressed(store: &str, unbuilt: &[Unbuilt]) -> Value {
    let why = unbuilt
        .iter()
        .find(|unbuilt| unbuilt.connection == store)
        .filter(|unbuilt| !matches!(unbuilt.why, Why::Unset(None)));
    jadpo_runtime::deployable::unaddressed(store, why.map(|why| why as &dyn std::fmt::Display))
}

/// Writes `line` to standard output, at once.
fn standard_output(line: &str) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{line}");
    let _ = out.flush();
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
}
