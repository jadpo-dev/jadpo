//! What a deployable's generated code hands its host, and what both hosts
//! start it with (roadmap chunks 8.1o, 8.2b and 8.2d;
//! docs/compiler-runtime.md §2.6, "A native deployable" and "A Workers
//! deployable"): the compiled program, the build stamp and each store's
//! migrations, as constants ([`Deployable`]). The native host's `main` and
//! the Workers host's handlers take the same [`Deployable`], so both start
//! a program the same way and stamp every event alike:
//!
//! - [`Log`]: each event one JSON object a line, its keys sorted, carrying
//!   the stamp, through the host's sink (standard output natively, the
//!   console on Workers, which Workers Logs reads);
//! - the problems that stop a start, each with its `problem` code and a
//!   `message` naming a setting by its variable and never its value
//!   ([`setting_problem`], [`unaddressed`], [`client_address_refused`]);
//! - each store compared with the migrations the build holds
//!   ([`compare`]), one behind, ahead or changed a problem
//!   ([`pending`], [`diverged`]) and one it can't reach told apart, which
//!   the native host waits for and a Worker checks again at its next
//!   request;
//! - [`Faults`]: each fault behind a 500 logged with its `.jadpo` frames
//!   (areas 4 and 17).

use std::fmt;

use jadpo_std::Unreadable;
use serde_json::{Map, Value, json};

use crate::compiled::Program;
use crate::config::{ProblemKind, StartProblem};
use crate::http::address::NotAccepted;
use crate::serve::{Answered, Observer};
use crate::{Database, MigrateError, StoreError};

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

/// Logs each fault behind a 500 with its `.jadpo` frames (areas 4 and 17)
/// as `operation.failed`.
#[derive(Debug)]
pub struct Faults(pub Log);

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

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use jadpo_std::ClientAddressSource;
    use jadpo_std::connections::Target;

    use super::*;

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
            target: Target::Workers,
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
}
