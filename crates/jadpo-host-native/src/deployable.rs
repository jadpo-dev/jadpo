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
//! built (which the runtime answers `misconfigured`), `runtime.ticked`
//! after each tick, its own timer's or the `tick` command's, with each
//! store it couldn't work in, `operation.failed` for a fault behind a 500
//! (area 17, "Errors point at the `.jadpo` line"), and `runtime.stopped`.
//! How it starts, compares its stores and logs a fault or a tick is the
//! runtime's, shared with the Workers host
//! (`jadpo_runtime::deployable`, chunk 8.4c).
//!
//! Its one argument is a command: none, or `serve`, serves as above; `tick`
//! runs one tick and ends ([`tick`]; area 12, question 16, Richard,
//! 2026-10-21; chunk 8.2g), what another scheduler runs each minute when
//! `schedules: { trigger: ScheduleTrigger.external }` says the server's
//! own timer never ticks. Anything else is `runtime.startup_failed`,
//! ending with code 2.

use std::cell::RefCell;
use std::ffi::OsString;
use std::io::Write;
use std::net::{SocketAddr, TcpListener, ToSocketAddrs};
use std::process::ExitCode;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use jadpo_drivers::factory::Unbuilt;
use jadpo_runtime::compiled::Program;
use jadpo_runtime::config::ConfigValues;
use jadpo_runtime::deployable::{
    Compared, Log, Logged, Then, compare_store, compare_stores, problem, unbuilt, waiting,
};
use jadpo_runtime::http::address::Target;
use jadpo_runtime::http::{Request, Response};
use jadpo_runtime::schedules::Ticked;
use jadpo_runtime::serve::Serving;
use jadpo_runtime::{Database, Executor, Host, LocalFuture};
use jadpo_std::{ScheduleTrigger, Unreadable};
use serde_json::{Value, json};

use crate::http::{AppHandler, Handler, MakeHandler, WorkerThread, serve};
use crate::tracked::unless;
use crate::{DRAIN_GRACE, Drivers, NativeHost, Runtime, supplies};

/// What a generated deployable's `main` hands [`run`]: the program, its
/// build stamp and each store's migrations, shared with the Workers host
/// (chunk 8.2b).
pub use jadpo_runtime::deployable::{Deployable, Migration, Stamp, Store};

/// The port a deployable listens on when `PORT` doesn't say.
pub const DEFAULT_PORT: u16 = 8080;

/// The longest a deployable waits between tries at a database it can't
/// reach yet as it starts.
const MOST_WAIT: Duration = Duration::from_secs(10);

/// What the binary was asked to do, by its one argument.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    /// None, or `serve`: listen until stopped.
    Serve,
    /// `tick`: one tick, then end.
    Tick,
}

impl Command {
    /// The command `arguments` (those after the program's name) name, or
    /// `None` when they name none.
    pub fn read(arguments: &[OsString]) -> Option<Command> {
        match arguments {
            [] => Some(Command::Serve),
            [command] if command == "serve" => Some(Command::Serve),
            [command] if command == "tick" => Some(Command::Tick),
            _ => None,
        }
    }
}

/// Runs `deployable` as its arguments say: serving until it is stopped, or
/// one tick. Its exit code: 1 when it couldn't start or a tick couldn't
/// work in a store, 2 when its arguments name no command.
pub fn run(deployable: &Deployable) -> ExitCode {
    let log = Log::new(deployable.stamp, standard_output);
    let arguments: Vec<OsString> = std::env::args_os().skip(1).collect();
    let outcome = match Command::read(&arguments) {
        Some(Command::Serve) => start(deployable, &log).map(|()| ExitCode::SUCCESS),
        Some(Command::Tick) => tick(deployable, &log),
        None => {
            // What was given isn't repeated: an argument may hold anything.
            log.event(
                "runtime.startup_failed",
                [(
                    "problems",
                    json!([problem(
                        "command_unknown",
                        format!(
                            "`{}` takes one command or none: `serve`, which it does with none, or `tick`, which runs one tick and ends.",
                            deployable.stamp.deployable
                        ),
                    )]),
                )],
            );
            return ExitCode::from(2);
        }
    };
    outcome.unwrap_or_else(|problems| {
        log.event(
            "runtime.startup_failed",
            [("problems", Value::Array(problems))],
        );
        ExitCode::from(1)
    })
}

/// The program, reading its settings from the environment, or what stops
/// it: what serving and a tick both start with, as a Worker starts
/// (`jadpo_runtime::deployable::start`).
fn configured(deployable: &Deployable) -> Result<(Program, ConfigValues), Vec<Value>> {
    let environment = |name: &str| std::env::var(name).ok();
    jadpo_runtime::deployable::start(deployable, &environment, Target::Native)
}

/// What ticks the program: the server's own timer unless
/// `schedules: { trigger: }` leaves it to another scheduler.
fn trigger(program: &Program) -> ScheduleTrigger {
    program
        .plan()
        .schedules()
        .map(|plan| plan.trigger)
        .unwrap_or_default()
}

/// Everything from reading the settings to stopping; the problems that
/// stopped the start, as log objects.
fn start(deployable: &Deployable, log: &Log) -> Result<(), Vec<Value>> {
    let stop = watched()?;
    let (program, values) = configured(deployable)?;
    let environment = |name: &str| std::env::var(name).ok();
    let address = listen_address(environment("HOST"), environment("PORT"))
        .map_err(|why| vec![problem("address_invalid", why)])?;
    let trigger = trigger(&program);
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
        handlers(
            deployable.program,
            Arc::new(values),
            *log,
            trigger == ScheduleTrigger::Native,
        ),
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
            ("trigger", Value::from(trigger.name())),
            ("unbuilt", self::unbuilt(&unbuilt)),
        ],
    );
    while !stop.load(Ordering::SeqCst) {
        std::thread::sleep(Duration::from_millis(50));
    }
    server.stop();
    log.event("runtime.stopped", []);
    Ok(())
}

/// SIGTERM and ctrl-c, watched.
fn watched() -> Result<Arc<AtomicBool>, Vec<Value>> {
    crate::stop_signals().map_err(|error| {
        vec![problem(
            "signals",
            format!("SIGTERM and ctrl-c couldn't be watched: {error}"),
        )]
    })
}

/// One tick, `<deployable> tick` (area 12, question 16: Richard,
/// 2026-10-21; chunk 8.2g): the settings read and each store compared with
/// the build's migrations as serving does, except that a database that
/// can't be reached isn't waited for, since the scheduler's next run is
/// the next try and the ticks missed meanwhile become one run; then the
/// runtime's tick through `Serving` on a `NativeHost`, which logs it as
/// `runtime.ticked` as it logs a server's (chunk 8.4c). Code 0, or 1 when
/// the tick couldn't work in a store; a schedule's failed run is
/// recorded, alerted and retried at its next due time, as a server's is.
/// The first SIGTERM or ctrl-c drains it as a server drains its
/// background work, giving back what it holds, and it ends with
/// `runtime.stopped`.
fn tick(deployable: &Deployable, log: &Log) -> Result<ExitCode, Vec<Value>> {
    let stop = watched()?;
    let (program, _) = configured(deployable)?;
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
    let stores = runtime.block_on(compare_stores(
        deployable,
        |name| {
            drivers
                .database(name)
                .map(|database| database as &dyn Database)
        },
        drivers.unbuilt(),
        Then::NextTick,
    ));
    if !stores.problems.is_empty() {
        return Err(stores.problems);
    }
    let logged = Rc::new(Logged::new(*log, drivers.unbuilt()));
    let host = Rc::new(NativeHost::new().with_drivers(drivers));
    let program: Rc<dyn Executor> = Rc::new(program);
    let serving = Serving::new(Rc::clone(&host) as Rc<dyn Host>, move || {
        Rc::clone(&program)
    })
    .observed_by(logged);
    let (ticked, stopped) = runtime.block_on(async {
        let slot: Rc<RefCell<Option<Ticked>>> = Rc::default();
        let tick = serving.tick();
        let filled = Rc::clone(&slot);
        let work = host.track(Box::pin(async move {
            *filled.borrow_mut() = tick.await;
        }));
        let mut task = std::pin::pin!(tokio::task::spawn_local(work));
        let stopping = async {
            while !stop.load(Ordering::SeqCst) {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        };
        let stopped = unless(task.as_mut(), stopping).await.is_none();
        if stopped {
            // Every budget it runs within ends, so the piece running
            // gives back what it holds.
            host.drain().await;
            let _ = tokio::time::timeout(DRAIN_GRACE, task).await;
        }
        (slot.borrow_mut().take(), stopped)
    });
    let ticked = ticked.unwrap_or_default();
    let failed = !ticked.problems.is_empty();
    if stopped {
        log.event("runtime.stopped", []);
    }
    Ok(if failed {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    })
}

/// Each worker's handler: the program on the worker's thread with the
/// settings the start read, its drivers built there by the factory, served
/// through `Serving` on its `NativeHost`, and ticked by the server's own
/// timer when `timed`.
fn handlers(
    program: fn() -> Result<Program, Unreadable>,
    values: Arc<ConfigValues>,
    log: Log,
    timed: bool,
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
        let logged = Rc::new(Logged::new(log, drivers.unbuilt()));
        let program: Rc<dyn Executor> = Rc::new(program);
        let host = worker.host().with_drivers(drivers);
        let handler = AppHandler::new(host, move || Rc::clone(&program)).observed_by(logged);
        Box::new(if timed { handler } else { handler.untimed() })
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
        let database = drivers
            .database(store.name)
            .map(|database| database as &dyn Database);
        let mut wait = Duration::from_secs(1);
        let mut said = false;
        loop {
            match runtime.block_on(compare_store(
                database,
                drivers.unbuilt(),
                store,
                Then::Restart,
            )) {
                Compared::Current => break,
                Compared::Refused(problem) => {
                    problems.push(problem);
                    break;
                }
                Compared::Unreachable => {
                    if !said {
                        waiting(log, store.name, Then::Restart);
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
    fn its_one_argument_is_serve_or_tick() {
        let read = |arguments: &[&str]| {
            Command::read(
                &arguments
                    .iter()
                    .map(OsString::from)
                    .collect::<Vec<OsString>>(),
            )
        };
        assert_eq!(read(&[]), Some(Command::Serve));
        assert_eq!(read(&["serve"]), Some(Command::Serve));
        assert_eq!(read(&["tick"]), Some(Command::Tick));
        assert_eq!(read(&["ticks"]), None);
        assert_eq!(read(&["tick", "now"]), None);
        assert_eq!(read(&["--tick"]), None);
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
}
