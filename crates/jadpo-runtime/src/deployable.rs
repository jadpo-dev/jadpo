//! What a deployable's generated code hands its host, and what both hosts
//! start it with (roadmap chunks 8.1o, 8.2b, 8.2d and 8.4c;
//! docs/compiler-runtime.md §2.6, "A native deployable" and "A Workers
//! deployable"): the compiled program, the build stamp and each store's
//! migrations, as constants ([`Deployable`]). The native host's `main` and
//! the Workers host's handlers take the same [`Deployable`] and start it
//! through this module, so both start a program the same way and log
//! alike:
//!
//! - [`start`]: the program made, its settings read from what the host
//!   gives (the environment natively, the bindings on Workers) and its
//!   `client_address:` source accepted on the target, or every problem
//!   that stops the start, each with its `problem` code and a `message`
//!   naming a setting by its variable and never its value
//!   ([`setting_problem`], [`client_address_refused`]);
//! - [`compare_stores`]: each store compared with the migrations the
//!   build holds ([`compare_store`]), one behind, ahead or changed, or
//!   with no driver, a problem ([`pending`], [`diverged`], [`unaddressed`])
//!   and one it can't reach told apart, which the native host waits for
//!   ([`waiting`]), a Worker checks again at its next request and a tick
//!   doesn't wait for;
//! - [`unbuilt`]: each connection whose driver wasn't built
//!   ([`NotBuilt`]), as `runtime.started` and `runtime.ticked` list it;
//! - [`Log`]: each event one JSON object a line, its keys sorted,
//!   carrying the stamp, through the host's sink (standard output
//!   natively, the console on Workers, which Workers Logs reads);
//! - [`Logged`]: what `Serving` does, logged: each fault behind a 500 with
//!   its `.jadpo` frames (areas 4 and 17), and each tick's result as
//!   `runtime.ticked` ([`ticked_fields`]), so a store a tick can't work in
//!   is logged wherever it ticks: a native server's timer, its `tick`
//!   command and a Worker's cron.

use std::fmt;

use jadpo_std::Unreadable;
use jadpo_std::schedule::SchedulesPlan;
use serde_json::{Map, Value, json};

use crate::compiled::Program;
use crate::config::{self, ConfigValues, Mode, ProblemKind, StartProblem, Variables};
use crate::http::address::{ClientAddress, NotAccepted, Target};
use crate::schedules::{Ran, Ticked};
use crate::serve::{Answered, Observer};
use crate::{Database, Executor, MigrateError, StoreError};

/// What a generated deployable hands its host.
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
/// §2.8): a constant in the deployable, and on every event it logs.
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
    /// `native` or `wasm`, as `target:` names it.
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

/// Where a deployable's lines go: standard output natively, the console
/// on Workers.
pub type Sink = fn(&str);

/// A deployable's log: one JSON object a line, each carrying the build
/// stamp.
#[derive(Clone, Copy)]
pub struct Log {
    stamp: Stamp,
    sink: Sink,
}

impl fmt::Debug for Log {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Log")
            .field("stamp", &self.stamp)
            .finish_non_exhaustive()
    }
}

impl Log {
    /// The log of the deployable stamped `stamp`, its lines given to
    /// `sink`.
    pub fn new(stamp: Stamp, sink: Sink) -> Log {
        Log { stamp, sink }
    }

    /// Logs `event` with `fields`.
    pub fn event<const N: usize>(&self, event: &str, fields: [(&str, Value); N]) {
        (self.sink)(&self.line(event, fields));
    }

    /// `event` with the stamp, then `fields`, as one line of JSON, its
    /// keys sorted.
    pub fn line<const N: usize>(&self, event: &str, fields: [(&str, Value); N]) -> String {
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

/// What a deployable logs of what `Serving` does, on either host (chunk
/// 8.4c): each fault behind a 500 with its `.jadpo` frames (areas 4 and
/// 17) as `operation.failed`, and each tick's result as `runtime.ticked`
/// ([`ticked_fields`]), naming the connections whose drivers the host
/// serving it didn't build.
#[derive(Debug)]
pub struct Logged {
    log: Log,
    unbuilt: Value,
}

impl Logged {
    /// Logs to `log`, on a host whose factory didn't build `unbuilt`.
    pub fn new<U: NotBuilt>(log: Log, unbuilt: &[U]) -> Logged {
        Logged {
            log,
            unbuilt: self::unbuilt(unbuilt),
        }
    }
}

impl Observer for Logged {
    fn answered(&self, answered: &Answered<'_>) {
        let Some(fault) = &answered.response.fault else {
            return;
        };
        self.log.event(
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

    fn ticked(&self, program: &dyn Executor, ticked: &Ticked, _now: i64, ms: u64) {
        self.log.event(
            "runtime.ticked",
            ticked_fields(program.plan().schedules(), ticked, ms, self.unbuilt.clone()),
        );
    }
}

/// What `runtime.ticked` says of a tick that took `ms`: each schedule it
/// ran or skipped by name, how many deliveries it ran, what it received
/// and polled, how many rows it deleted, the alerts it raised as their
/// own events say them, each store it couldn't work in, and each
/// connection whose driver wasn't built (`unbuilt`, as [`unbuilt`] lists
/// them). Never a value from the app's data.
pub fn ticked_fields(
    plan: Option<&SchedulesPlan>,
    ticked: &Ticked,
    ms: u64,
    unbuilt: Value,
) -> [(&'static str, Value); 9] {
    let name = |key: &str| {
        plan.and_then(|plan| plan.schedule(key))
            .map_or_else(|| key.to_string(), |schedule| schedule.name.clone())
    };
    let ran = ticked
        .due
        .iter()
        .map(|due| {
            let mut run = Map::new();
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
        ("unbuilt", unbuilt),
    ]
}

/// The program `deployable` holds, started as both hosts start it (chunk
/// 8.4c): its plan read, its settings read from `variables` (the
/// environment natively, the bindings on Workers) and checked (area 10),
/// and its `client_address:` source one `target` accepts; with the values
/// read, which a native server's workers each start their program with.
/// `Err` with every problem that stops the start.
pub fn start(
    deployable: &Deployable,
    variables: &dyn Variables,
    target: Target,
) -> Result<(Program, ConfigValues), Vec<Value>> {
    let program = (deployable.program)().map_err(|error| {
        vec![problem(
            "plan_unreadable",
            format!("The program's plan can't be read: {error}."),
        )]
    })?;
    let values = config::load(
        &program.runtime().config,
        program.validators(),
        variables,
        Mode::Environment,
        &|_: &mut [u8]| false,
    )
    .map_err(|problems| problems.iter().map(setting_problem).collect::<Vec<_>>())?;
    let refused = ClientAddress::read(
        program
            .plan()
            .routes
            .and_then(|routes| routes.client_address.as_ref()),
        &values,
        target,
    )
    .err();
    if let Some(refused) = refused {
        return Err(vec![client_address_refused(&refused)]);
    }
    let program = program.with_config_on(&values, target);
    Ok((program, values))
}

/// A connection whose driver a host's factory didn't build
/// (`jadpo_drivers::factory::Unbuilt`), as a deployable reports it: the
/// runtime answers it `misconfigured` when it is used.
pub trait NotBuilt: fmt::Display {
    /// The connection's name.
    fn connection(&self) -> &str;

    /// Its driver's name.
    fn driver(&self) -> &str;

    /// Whether its connection names no address at all, rather than one
    /// the factory couldn't build a driver from.
    fn unaddressed(&self) -> bool;
}

/// Each connection whose driver wasn't built, as `runtime.started` and
/// `runtime.ticked` list them: its name, its driver and why, in words.
pub fn unbuilt<U: NotBuilt>(unbuilt: &[U]) -> Value {
    Value::Array(
        unbuilt
            .iter()
            .map(|unbuilt| {
                json!({
                    "connection": unbuilt.connection(),
                    "driver": unbuilt.driver(),
                    "message": unbuilt.to_string(),
                })
            })
            .collect(),
    )
}

/// A problem with its code and what to say about it.
pub fn problem(code: &str, message: impl Into<String>) -> Value {
    json!({ "problem": code, "message": message.into() })
}

/// A setting that stops the start, named with its variable, never its
/// value (area 10).
pub fn setting_problem(found: &StartProblem) -> Value {
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

/// A `client_address:` source the deployable's target doesn't accept
/// (area 10, "Client addresses"): on Workers, anything but `cloudflare`.
pub fn client_address_refused(refused: &NotAccepted) -> Value {
    json!({
        "problem": "client_address_refused",
        "source": refused.source.name(),
        "message": format!(
            "`client_address:` reads `{}`, but {refused}: use `ClientAddressSource.cloudflare`, or leave `client_address:` out.",
            refused.source.name()
        ),
    })
}

/// A store whose database has no driver: its connection names no address
/// (`why` is `None`), or the factory couldn't build it, for `why`.
pub fn unaddressed(store: &str, why: Option<&dyn fmt::Display>) -> Value {
    let message = match why {
        None => format!(
            "`{store}` names no address: give its connection in `app.jadpo` a `url:` read from a setting."
        ),
        Some(why) => format!("{why}."),
    };
    json!({ "problem": "store_unbuilt", "store": store, "message": message })
}

/// What follows `jadpo migrate apply` before a deployable serves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Then {
    /// A process that refused to start is started again.
    Restart,
    /// A Worker checks again at its next request.
    NextRequest,
    /// A native deployable's `tick` command runs again at the scheduler's
    /// next run (chunk 8.2g).
    NextTick,
}

impl Then {
    fn words(self) -> &'static str {
        match self {
            Then::Restart => "then start the app again",
            Then::NextRequest => "and the Worker serves from its next request",
            Then::NextTick => "and the next tick runs",
        }
    }
}

/// A store whose database hasn't applied every migration the build holds.
pub fn pending(store: &str, names: &[String], then: Then) -> Value {
    let count = names.len();
    json!({
        "problem": "migrations_pending",
        "store": store,
        "pending": names,
        "message": format!(
            "`{store}` hasn't applied {} the app was built with: run `jadpo migrate apply` against it, {}.",
            if count == 1 { "1 migration".to_string() } else { format!("{count} migrations") },
            then.words()
        ),
    })
}

/// A store whose database doesn't match the build's migrations, or
/// couldn't be read.
pub fn diverged(store: &str, error: &MigrateError) -> Value {
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
pub fn store_error(error: &StoreError) -> String {
    match error {
        StoreError::Fault(why) | StoreError::Statement(why) => why.clone(),
        StoreError::Unavailable => "it can't be reached".into(),
        StoreError::ConcurrentChange => "another change got there first".into(),
        StoreError::Conflict { constraint } => format!("`{constraint}` refused it"),
    }
}

/// What comparing a store's database with the build's migrations found.
#[derive(Clone, Debug, PartialEq)]
pub enum Compared {
    /// Every migration the build holds is applied, and nothing else.
    Current,
    /// The database is behind, ahead or changed, or refused the check:
    /// the problem, which stops the start.
    Refused(Value),
    /// The database can't be reached yet, which is no configuration error
    /// (area 10): the native host waits for it, a Worker answers 503 and
    /// checks again at its next request.
    Unreachable,
}

/// Compares `database` with `store`'s migrations (area 5, "Migrations"),
/// a deployable that finds it behind serving once it is current, `then`.
pub async fn compare(database: &dyn Database, store: &Store, then: Then) -> Compared {
    let migrations: Vec<jadpo_std::Migration> = store
        .migrations
        .iter()
        .map(|migration| jadpo_std::Migration {
            name: migration.name.to_string(),
            digest: migration.digest.to_string(),
            sql: String::new(),
        })
        .collect();
    match crate::standing(database, &migrations).await {
        Ok(standing) if standing.pending.is_empty() => Compared::Current,
        Ok(standing) => Compared::Refused(pending(store.name, &standing.pending, then)),
        Err(MigrateError::Store(StoreError::Unavailable)) => Compared::Unreachable,
        Err(error) => Compared::Refused(diverged(store.name, &error)),
    }
}

/// Store `store` compared with the migrations the build holds through
/// `database`, its driver: `None` when the host's factory didn't build it,
/// `unbuilt` saying why, which stops the start as a problem.
pub async fn compare_store<U: NotBuilt>(
    database: Option<&dyn Database>,
    unbuilt: &[U],
    store: &Store,
    then: Then,
) -> Compared {
    let Some(database) = database else {
        let why = unbuilt
            .iter()
            .find(|unbuilt| unbuilt.connection() == store.name)
            .filter(|unbuilt| !unbuilt.unaddressed());
        return Compared::Refused(unaddressed(
            store.name,
            why.map(|why| why as &dyn fmt::Display),
        ));
    };
    compare(database, store, then).await
}

/// What comparing each of a deployable's stores found.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stores {
    /// What stops the start, as `runtime.startup_failed` lists them.
    pub problems: Vec<Value>,
    /// The stores it can't reach yet, which a server waits for and a
    /// Worker checks again at its next request.
    pub unreachable: Vec<&'static str>,
}

impl Stores {
    /// Whether every store is the build's.
    pub fn current(&self) -> bool {
        self.problems.is_empty() && self.unreachable.is_empty()
    }
}

/// Each of `deployable`'s stores compared ([`compare_store`]), `database`
/// giving each store's driver. A tick ([`Then::NextTick`]) waits for no
/// database: one it can't reach is a problem, `store_unreachable`, since
/// the scheduler's next run is the next try.
pub async fn compare_stores<'d, U: NotBuilt>(
    deployable: &Deployable,
    database: impl Fn(&str) -> Option<&'d dyn Database>,
    unbuilt: &[U],
    then: Then,
) -> Stores {
    let mut stores = Stores::default();
    for store in deployable.stores {
        match compare_store(database(store.name), unbuilt, store, then).await {
            Compared::Current => {}
            Compared::Refused(problem) => stores.problems.push(problem),
            Compared::Unreachable if then == Then::NextTick => {
                stores.problems.push(json!({
                    "problem": "store_unreachable",
                    "store": store.name,
                    "message": format!(
                        "`{}` can't be reached: nothing was ticked, and the next tick tries again.",
                        store.name
                    ),
                }));
            }
            Compared::Unreachable => stores.unreachable.push(store.name),
        }
    }
    stores
}

/// Logs `runtime.waiting` for store `store`, which can't be reached yet:
/// the deployable serves once it can check the database's migrations,
/// `then` saying when it tries again.
pub fn waiting(log: &Log, store: &str, then: Then) {
    let message = match then {
        Then::NextRequest => format!(
            "`{store}` can't be reached yet: the Worker serves once it can check the database's migrations, trying again at each request."
        ),
        Then::Restart | Then::NextTick => format!(
            "`{store}` can't be reached yet: the app starts once it can check the database's migrations."
        ),
    };
    log.event(
        "runtime.waiting",
        [
            ("store", Value::from(store)),
            ("message", Value::from(message)),
        ],
    );
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use jadpo_std::ClientAddressSource;
    use jadpo_std::schedule::{SchedulePlan, ScheduleRun};
    use jadpo_value::Value as Held;

    use super::*;
    use crate::serve::Serving;
    use crate::{Ctx, Host, Isolation, LocalFuture, Outcome, Places, Plan, Transaction, block_on};

    const STAMP: Stamp = Stamp {
        build: "b_3c41d07a9e12",
        commit: Some("8d113e6"),
        modified: false,
        compiler: "0.1.0",
        deployable: "app",
        target: "native",
    };

    thread_local! {
        static LINES: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    }

    fn kept(line: &str) {
        LINES.with(|lines| lines.borrow_mut().push(line.to_string()));
    }

    #[test]
    fn every_event_carries_the_stamp() {
        let line = Log::new(STAMP, kept).line("runtime.stopped", []);
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
        let unstamped = Log::new(
            Stamp {
                commit: None,
                modified: true,
                ..STAMP
            },
            kept,
        )
        .line("runtime.waiting", [("store", Value::from("main"))]);
        let read: Value = serde_json::from_str(&unstamped).unwrap();
        assert_eq!(read["commit"], Value::Null);
        assert_eq!(read["modified"], Value::Bool(true));
        assert_eq!(read["store"], "main");
    }

    #[test]
    fn an_event_goes_to_the_sink_as_one_line_its_keys_sorted() {
        Log::new(STAMP, kept).event("runtime.started", [("unbuilt", json!([]))]);
        let lines = LINES.with(|lines| lines.borrow().clone());
        assert_eq!(
            lines,
            vec![
                "{\"build\":\"b_3c41d07a9e12\",\"commit\":\"8d113e6\",\"compiler\":\"0.1.0\",\"deployable\":\"app\",\"event\":\"runtime.started\",\"modified\":false,\"target\":\"native\",\"unbuilt\":[]}"
                    .to_string()
            ]
        );
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
        let one = pending(
            "main",
            &["20261020T0626_hold_deliveries".to_string()],
            Then::Restart,
        );
        assert_eq!(one["pending"], json!(["20261020T0626_hold_deliveries"]));
        assert_eq!(
            one["message"],
            "`main` hasn't applied 1 migration the app was built with: run `jadpo migrate apply` against it, then start the app again."
        );
        let two = pending(
            "main",
            &["one".to_string(), "two".to_string()],
            Then::NextRequest,
        );
        assert_eq!(
            two["message"],
            "`main` hasn't applied 2 migrations the app was built with: run `jadpo migrate apply` against it, and the Worker serves from its next request."
        );
        let unnamed = unaddressed("main", None);
        assert!(
            unnamed["message"]
                .as_str()
                .unwrap()
                .contains("give its connection in `app.jadpo` a `url:`")
        );
        let unbuilt = unaddressed("main", Some(&"`main`'s address isn't one PostgreSQL reads"));
        assert_eq!(
            unbuilt["message"],
            "`main`'s address isn't one PostgreSQL reads."
        );
    }

    #[test]
    fn a_source_workers_refuses_names_the_one_it_takes() {
        let refused = client_address_refused(&NotAccepted {
            source: ClientAddressSource::ForwardedFor,
            target: jadpo_std::connections::Target::Workers,
        });
        assert_eq!(refused["problem"], "client_address_refused");
        assert_eq!(refused["source"], "forwarded_for");
        let message = refused["message"].as_str().unwrap();
        assert!(
            message.contains("`CF-Connecting-IP`")
                && message.contains("ClientAddressSource.cloudflare"),
            "{message}"
        );
    }

    /// A connection a host's factory didn't build.
    struct Unbuilt {
        connection: &'static str,
        unaddressed: bool,
    }

    impl fmt::Display for Unbuilt {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(
                f,
                "`{}`'s postgres reads a setting that has no value",
                self.connection
            )
        }
    }

    impl NotBuilt for Unbuilt {
        fn connection(&self) -> &str {
            self.connection
        }

        fn driver(&self) -> &str {
            "postgres"
        }

        fn unaddressed(&self) -> bool {
            self.unaddressed
        }
    }

    /// A database that can't be reached.
    struct Down;

    impl Database for Down {
        fn begin_at(
            &self,
            _isolation: Isolation,
        ) -> LocalFuture<'_, Result<Box<dyn Transaction>, StoreError>> {
            Box::pin(async { Err(StoreError::Unavailable) })
        }
    }

    /// A host with a clock whose one database, `main`, can't be reached.
    struct Unreachable;

    impl Host for Unreachable {
        fn database(&self, _store: &str) -> Option<&dyn Database> {
            Some(&Down)
        }

        fn random(&self, bytes: &mut [u8]) -> bool {
            bytes.fill(1);
            true
        }

        fn now_ms(&self) -> Option<i64> {
            Some(1_792_486_800_000)
        }
    }

    /// A program whose tick sweeps `main` and runs no schedule.
    struct Sweeping {
        places: Places,
        schedules: SchedulesPlan,
    }

    impl Executor for Sweeping {
        fn places(&self) -> &Places {
            &self.places
        }

        fn plan(&self) -> Plan<'_> {
            Plan {
                schedules: Some(&self.schedules),
                runs: jadpo_std::Runs {
                    schedules: false,
                    subscribers: false,
                },
                ..Plan::NONE
            }
        }

        fn run<'a, 'p: 'a>(
            &'a self,
            _ctx: &'a mut Ctx<'p>,
            _callable: u32,
            _args: Vec<Held>,
        ) -> LocalFuture<'a, Outcome> {
            Box::pin(async { Outcome::Value(Held::Unit) })
        }
    }

    /// What `sink` has been given, read as JSON, and cleared.
    fn logged() -> Vec<Value> {
        LINES.with(|lines| {
            lines
                .borrow_mut()
                .drain(..)
                .map(|line| serde_json::from_str(&line).expect("JSON"))
                .collect()
        })
    }

    /// Each tick is logged as `runtime.ticked` by what serves on either
    /// host (chunk 8.4c), so a store a tick can't work in is logged
    /// wherever it ticks, with the connections that host didn't build.
    #[test]
    fn a_tick_that_cant_work_in_a_store_is_logged_where_it_ticks() {
        logged();
        let program: Rc<dyn Executor> = Rc::new(Sweeping {
            places: Places::new(),
            schedules: SchedulesPlan {
                stores: vec!["main".into()],
                ..SchedulesPlan::default()
            },
        });
        let unbuilt = [Unbuilt {
            connection: "archive",
            unaddressed: false,
        }];
        let serving = Serving::new(Rc::new(Unreachable), move || Rc::clone(&program))
            .observed_by(Rc::new(Logged::new(Log::new(STAMP, kept), &unbuilt)));
        let ticked = block_on(serving.tick())
            .expect("nothing waits")
            .expect("a clock");
        assert!(!ticked.problems.is_empty(), "{ticked:?}");
        let lines = logged();
        let [line] = lines.as_slice() else {
            panic!("{lines:?}")
        };
        assert_eq!(line["event"], "runtime.ticked", "{line}");
        assert_eq!(line["build"], STAMP.build, "{line}");
        let problems = line["problems"].as_array().expect("its problems");
        assert_eq!(problems.len(), ticked.problems.len(), "{line}");
        assert_eq!(
            problems[0],
            json!({
                "problem": "store_failed",
                "store": "main",
                "message": "`main` couldn't be ticked: it can't be reached. The next tick tries again.",
            })
        );
        assert_eq!(
            line["unbuilt"],
            json!([{
                "connection": "archive",
                "driver": "postgres",
                "message": "`archive`'s postgres reads a setting that has no value",
            }])
        );
        assert_eq!(line["ran"], json!([]), "{line}");
    }

    #[test]
    fn a_tick_is_logged_by_its_schedules_names_never_a_value() {
        use crate::schedules::watch::{Alert, AlertKind};
        use crate::schedules::{Deleted, Due, Skipped, Tally};
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
            problems: vec![("archive".into(), StoreError::Unavailable)],
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
        let fields: Map<String, Value> =
            ticked_fields(Some(&plan), &ticked, 412, unbuilt::<Unbuilt>(&[]))
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

    /// Both hosts compare their stores the same way: one with no driver a
    /// problem naming why, one out of reach waited for, except by a tick,
    /// which waits for nothing.
    #[test]
    fn each_store_is_compared_alike_on_either_host() {
        const MAIN: Store = Store {
            name: "main",
            migrations: &[],
        };
        let deployable = Deployable {
            program: || Err(Unreadable::NotThisFormat),
            stamp: STAMP,
            stores: &[MAIN],
        };
        fn none(_: &str) -> Option<&'static dyn Database> {
            None
        }
        fn down(_: &str) -> Option<&'static dyn Database> {
            Some(&Down)
        }
        let unset = [Unbuilt {
            connection: "main",
            unaddressed: true,
        }];
        let unbuildable = [Unbuilt {
            connection: "main",
            unaddressed: false,
        }];
        let stores = block_on(compare_stores(&deployable, none, &unset, Then::Restart))
            .expect("nothing waits");
        assert!(!stores.current());
        assert_eq!(stores.problems[0]["problem"], "store_unbuilt");
        assert!(
            stores.problems[0]["message"]
                .as_str()
                .unwrap()
                .contains("give its connection in `app.jadpo` a `url:`"),
            "{stores:?}"
        );
        let stores = block_on(compare_stores(
            &deployable,
            none,
            &unbuildable,
            Then::Restart,
        ))
        .expect("nothing waits");
        assert_eq!(
            stores.problems[0]["message"],
            "`main`'s postgres reads a setting that has no value."
        );
        let stores = block_on(compare_stores(
            &deployable,
            down,
            &[] as &[Unbuilt],
            Then::NextRequest,
        ))
        .expect("nothing waits");
        assert_eq!(
            stores,
            Stores {
                problems: Vec::new(),
                unreachable: vec!["main"],
            }
        );
        let stores = block_on(compare_stores(
            &deployable,
            down,
            &[] as &[Unbuilt],
            Then::NextTick,
        ))
        .expect("nothing waits");
        assert!(stores.unreachable.is_empty());
        assert_eq!(
            stores.problems,
            vec![json!({
                "problem": "store_unreachable",
                "store": "main",
                "message": "`main` can't be reached: nothing was ticked, and the next tick tries again.",
            })]
        );
    }

    /// A plan that can't be read stops the start on either host.
    #[test]
    fn a_plan_that_cant_be_read_stops_the_start() {
        let deployable = Deployable {
            program: || Err(Unreadable::NotThisFormat),
            stamp: STAMP,
            stores: &[],
        };
        let variables = std::collections::BTreeMap::new();
        let Err(problems) = start(&deployable, &variables, Target::Workers) else {
            panic!("it started")
        };
        assert_eq!(problems[0]["problem"], "plan_unreadable");
    }
}
