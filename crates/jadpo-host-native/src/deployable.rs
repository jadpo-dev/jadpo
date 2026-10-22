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
use std::time::{Duration, Instant};

use jadpo_drivers::factory::{Unbuilt, Why};
use jadpo_runtime::compiled::Program;
use jadpo_runtime::config::{self, ConfigValues, Mode};
use jadpo_runtime::deployable::{
    Compared, Faults, Log, Then, compare, problem, setting_problem, store_error,
};
use jadpo_runtime::http::{Request, Response};
use jadpo_runtime::schedules::{Ran, Ticked};
use jadpo_runtime::serve::Serving;
use jadpo_runtime::{Executor, Host, LocalFuture};
use jadpo_std::schedule::SchedulesPlan;
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
/// it: what serving and a tick both start with.
fn configured(deployable: &Deployable) -> Result<(Program, ConfigValues), Vec<Value>> {
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
    let program = program.with_config(&values);
    Ok((program, values))
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
            ("unbuilt", unbuilt_list(&unbuilt)),
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

/// Each connection whose driver wasn't built, as the log lists it.
fn unbuilt_list(unbuilt: &[Unbuilt]) -> Value {
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
    )
}

/// One tick, `<deployable> tick` (area 12, question 16: Richard,
/// 2026-10-21; chunk 8.2g): the settings read and each store compared with
/// the build's migrations as serving does, except that a database that
/// can't be reached isn't waited for, since the scheduler's next run is
/// the next try and the ticks missed meanwhile become one run; then the
/// runtime's tick through `Serving` on a `NativeHost`, logged as
/// `runtime.ticked`. Code 0, or 1 when the tick couldn't work in a store,
/// panicked or outlasted the drain ([`tick_code`]);
/// a schedule's failed run is recorded, alerted and retried at its next
/// due time, as a server's is. The first SIGTERM or ctrl-c drains it as a
/// server drains its background work, giving back what it holds, and it
/// ends with `runtime.stopped`.
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
    let mut problems = Vec::new();
    for store in deployable.stores {
        let Some(database) = drivers.database(store.name) else {
            problems.push(unaddressed(store.name, drivers.unbuilt()));
            continue;
        };
        match runtime.block_on(compare(database, store, Then::NextTick)) {
            Compared::Current => {}
            Compared::Refused(problem) => problems.push(problem),
            Compared::Unreachable => problems.push(json!({
                "problem": "store_unreachable",
                "store": store.name,
                "message": format!(
                    "`{}` can't be reached: nothing was ticked, and the next tick tries again.",
                    store.name
                ),
            })),
        }
    }
    if !problems.is_empty() {
        return Err(problems);
    }
    let unbuilt = drivers.unbuilt().to_vec();
    let host = Rc::new(NativeHost::new().with_drivers(drivers));
    let program: Rc<Program> = Rc::new(program);
    let serving = {
        let program = Rc::clone(&program);
        Serving::new(Rc::clone(&host) as Rc<dyn Host>, move || {
            Rc::clone(&program) as Rc<dyn Executor>
        })
    };
    let began = Instant::now();
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
    let code = tick_code(ticked.as_ref());
    let ticked = ticked.unwrap_or_default();
    let ms = u64::try_from(began.elapsed().as_millis()).unwrap_or(u64::MAX);
    log.event(
        "runtime.ticked",
        ticked_fields(program.plan().schedules(), &ticked, ms, &unbuilt),
    );
    if stopped {
        log.event("runtime.stopped", []);
    }
    Ok(ExitCode::from(code))
}

/// The code `<deployable> tick` ends with for what its tick gave: 0 once
/// it ran to its end and could work in every store, 1 when it couldn't,
/// and 1 when it gave nothing, having panicked or outlasted the drain
/// (chunk 8.4b), so the scheduler sees it didn't finish.
fn tick_code(ticked: Option<&Ticked>) -> u8 {
    match ticked {
        Some(ticked) if ticked.problems.is_empty() => 0,
        _ => 1,
    }
}

/// What `runtime.ticked` says of a tick that took `ms`: each schedule it
/// ran or skipped by name, how many deliveries it ran, what it received
/// and polled, how many rows it deleted, the alerts it raised as their
/// own events say them, each store it couldn't work in, and each
/// connection whose driver wasn't built. Never a value from the app's
/// data.
pub fn ticked_fields(
    plan: Option<&SchedulesPlan>,
    ticked: &Ticked,
    ms: u64,
    unbuilt: &[Unbuilt],
) -> [(&'static str, Value); 9] {
    let name = |key: &str| {
        plan.and_then(|plan| plan.schedule(key))
            .map_or_else(|| key.to_string(), |schedule| schedule.name.clone())
    };
    let ran = ticked
        .due
        .iter()
        .map(|due| {
            let mut run = serde_json::Map::new();
            run.insert("schedule".into(), name(&due.schedule).into());
            run.insert("by_hand".into(), due.by_hand.into());
            run.insert("ms".into(), due.duration_ms().into());
            match &due.ran {
                Ran::Succeeded { events } => {
                    run.insert("outcome".into(), "succeeded".into());
                    run.insert("events".into(), (*events).into());
                }
                Ran::Failed { code, kind } => {
                    run.insert("outcome".into(), "failed".into());
                    run.insert("code".into(), code.as_str().into());
                    run.insert(
                        "kind".into(),
                        kind.as_deref().map_or(Value::Null, Value::from),
                    );
                }
                Ran::Skipped(why) => {
                    run.insert("outcome".into(), "skipped".into());
                    run.insert("reason".into(), why.word().into());
                }
            }
            if let Some(items) = due.items {
                run.insert("items".into(), items.into());
            }
            Value::Object(run)
        })
        .collect();
    let deleted = ticked.deleted;
    let rows = [
        deleted.keys,
        deleted.events,
        deleted.runs,
        deleted.handled,
        deleted.sessions,
        deleted.service_keys,
        deleted.deliveries,
        deleted.actions,
        deleted.received,
        deleted.answers,
        deleted.step_records,
        deleted.parked,
        deleted.ai_usage,
    ]
    .iter()
    .fold(0u64, |sum, rows| sum.saturating_add(*rows));
    let problems = ticked
        .problems
        .iter()
        .map(|(store, error)| {
            json!({
                "problem": "store_failed",
                "store": store,
                "message": format!(
                    "`{store}` couldn't be ticked: {}. The next tick tries again.",
                    store_error(error)
                ),
            })
        })
        .collect();
    [
        ("ms", ms.into()),
        ("ran", Value::Array(ran)),
        ("deliveries", ticked.deliveries.len().into()),
        (
            "received",
            ticked.received.map_or(Value::Null, |tally| {
                json!({
                    "kept": tally.kept,
                    "repeats": tally.repeats,
                    "parked": tally.parked,
                    "left": tally.left,
                })
            }),
        ),
        (
            "polled",
            ticked.polled.map_or(
                Value::Null,
                |(called, changed)| json!({ "called": called, "changed": changed }),
            ),
        ),
        ("deleted", rows.into()),
        (
            "alerts",
            Value::Array(ticked.raised.iter().map(|alert| alert.event()).collect()),
        ),
        ("problems", Value::Array(problems)),
        ("unbuilt", unbuilt_list(unbuilt)),
    ]
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
        let program: Rc<dyn Executor> = Rc::new(program);
        let host = worker.host().with_drivers(drivers);
        let handler =
            AppHandler::new(host, move || Rc::clone(&program)).observed_by(Rc::new(Faults(log)));
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

    /// A tick ends 0 only once it ran to its end and worked in every
    /// store: one that panicked or outlasted the drain gave nothing, and
    /// ends 1 as one that couldn't work in a store does (chunk 8.4b).
    #[test]
    fn a_tick_that_didnt_finish_ends_with_code_1() {
        let finished = Ticked::default();
        assert_eq!(tick_code(Some(&finished)), 0);
        let failed = Ticked {
            problems: vec![(
                "main".to_string(),
                jadpo_runtime::StoreError::Unavailable,
            )],
            ..Ticked::default()
        };
        assert_eq!(tick_code(Some(&failed)), 1);
        assert_eq!(tick_code(None), 1);
    }

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
    fn a_tick_is_logged_by_its_schedules_names_never_a_value() {
        use jadpo_runtime::schedules::watch::{Alert, AlertKind};
        use jadpo_runtime::schedules::{Deleted, Due, Skipped, Tally};
        use jadpo_std::schedule::{SchedulePlan, ScheduleRun};
        let plan = SchedulesPlan {
            schedules: vec![SchedulePlan {
                key: "schedule cafe.sweep".into(),
                name: "sweep".into(),
                every_ms: 60_000,
                run: ScheduleRun::Action(0),
                file: "domains/cafe/schedules/sweep.jadpo".into(),
                line: 4,
            }],
            store: Some("main".into()),
            ..SchedulesPlan::default()
        };
        let due = |ran: Ran| Due {
            schedule: "schedule cafe.sweep".into(),
            due_slot: 60_000,
            ran,
            started: 61_000,
            ended: 61_250,
            trace: None,
            items: None,
            batches: Vec::new(),
            by_hand: false,
        };
        let ticked = Ticked {
            due: vec![
                due(Ran::Succeeded { events: 2 }),
                due(Ran::Failed {
                    code: "out_of_brooms".into(),
                    kind: Some("unavailable".into()),
                }),
                due(Ran::Skipped(Skipped::Paused)),
            ],
            deleted: Deleted {
                keys: 3,
                events: 4,
                ..Deleted::default()
            },
            problems: vec![("archive".into(), jadpo_runtime::StoreError::Unavailable)],
            raised: vec![Alert {
                schedule: Some("sweep".into()),
                kind: AlertKind::FailedInARow { count: 3 },
            }],
            received: Some(Tally {
                kept: 1,
                ..Tally::default()
            }),
            ..Ticked::default()
        };
        let fields: serde_json::Map<String, Value> = ticked_fields(Some(&plan), &ticked, 412, &[])
            .into_iter()
            .map(|(key, value)| (key.to_string(), value))
            .collect();
        let fields = Value::Object(fields);
        assert_eq!(fields["ms"], 412);
        assert_eq!(
            fields["ran"],
            json!([
                { "schedule": "sweep", "outcome": "succeeded", "events": 2, "by_hand": false, "ms": 250 },
                { "schedule": "sweep", "outcome": "failed", "code": "out_of_brooms", "kind": "unavailable", "by_hand": false, "ms": 250 },
                { "schedule": "sweep", "outcome": "skipped", "reason": "paused", "by_hand": false, "ms": 250 },
            ])
        );
        assert_eq!(fields["deliveries"], 0);
        assert_eq!(fields["deleted"], 7);
        assert_eq!(
            fields["received"],
            json!({ "kept": 1, "repeats": 0, "parked": 0, "left": 0 })
        );
        assert_eq!(fields["polled"], Value::Null);
        assert_eq!(fields["alerts"][0]["alert"], "failed_in_a_row");
        assert_eq!(
            fields["problems"],
            json!([{
                "problem": "store_failed",
                "store": "archive",
                "message": "`archive` couldn't be ticked: it can't be reached. The next tick tries again.",
            }])
        );
        assert_eq!(fields["unbuilt"], json!([]));
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
