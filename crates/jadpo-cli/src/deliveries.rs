//! `jadpo deliveries` (area 12, "What an operator sees"; area 14,
//! question 13; chunk 6.12): the deliveries of events to subscribers, and
//! schedules' runs, that need an operator, and what one does to each,
//! against the databases they are in.
//!
//! - `jadpo deliveries` (or `list`) lists those that failed or whose
//!   outcome is unknown, newest first, 20 a page; `--state` lists those
//!   pending, running, held, dismissed or cancelled instead. Each
//!   connection whose circuit breaker holds deliveries is an alert above
//!   the footer of the list and of the held ones (chunk 7.2b).
//! - `retry <id>` sends a failed delivery again on the app's next
//!   tick, with a fresh retry budget, or asks the app's tick to run a
//!   schedule whose latest run failed, waiting for it as `jadpo schedules
//!   run` does; `dismiss <id>` sets a failed one aside; `cancel <id>`
//!   stops a pending one; `resolve <id> --done|--not-done` settles one
//!   whose outcome is unknown, "not done" retrying it.
//! - An action with several changing calls keeps a record of its steps
//!   (chunk 7.5c): `show <id>` lists each, its operation, whether it was
//!   answered and when, never a request or an answer; a delivery that
//!   `diverged` is listed and retried once a fix is deployed; `retry <id>
//!   --not-done` sends its step in doubt, or the one that failed, again
//!   under its next key generation; `abandon <id>` ends it for good,
//!   keeping its step list 30 days. A route's record whose client's key
//!   has lapsed, a day after its last run, is an item of its own,
//!   `unfinished`, which is shown and abandoned.
//!
//! Each action changes the item only from the state it was read in, and
//! writes, in the same statement, a row of `jadpo_operator_actions` with
//! who did it: the name the machine gives (`USER`), and the database role
//! PostgreSQL authenticated. Nothing of the app runs here, and none of
//! its settings are read beyond its connections' addresses.
//!
//! The databases are those of every store that holds an entity, where
//! deliveries are, and `schedules: { store: }`: each the one `--url`
//! names (with `--store` when there are several), else the address its
//! connection's setting gives, else the one a running `jadpo dev` started.

use std::collections::BTreeMap;
use std::path::PathBuf;

use jadpo_check::schedules::ScheduleModel;
use jadpo_diag::{Diagnostic, rule};
use jadpo_runtime::breaker::Unavailable;
use jadpo_runtime::deliveries::{
    self as runtime, ABANDONED, Action, CANCELLED, DISMISSED, DIVERGED, Done, FAILED, HELD,
    Holds, Item, Kind, NEEDS_AN_OPERATOR, OUTCOME_UNKNOWN, PENDING, RUNNING, UNFINISHED,
};
use jadpo_runtime::unfinished::{self, Record};
use jadpo_runtime::schedules::watch::Declared;
use jadpo_term::{Block, Cell, Fact, Json, Outcome, Screen, Symbol, Table, Token};

use crate::run::{Call, Reply, quoted};
use crate::schedules::{clock, now, plural, store_error, tick_every, url_of};

/// What `jadpo deliveries` does besides listing.
pub const ACTIONS: &[crate::table::Value] = &[
    crate::table::Value {
        name: "list",
        description: "List what needs an operator (the default)",
    },
    crate::table::Value {
        name: "retry",
        description: "Deliver a failed delivery, or run a schedule, again",
    },
    crate::table::Value {
        name: "dismiss",
        description: "Set a failed delivery or schedule run aside",
    },
    crate::table::Value {
        name: "cancel",
        description: "Stop a pending delivery or schedule run",
    },
    crate::table::Value {
        name: "resolve",
        description: "Settle one whose outcome is unknown, with --done or --not-done",
    },
    crate::table::Value {
        name: "show",
        description: "List the steps of one, and what to do",
    },
    crate::table::Value {
        name: "abandon",
        description: "End an action with several changing calls for good",
    },
];

/// The states `--state` lists.
pub const STATES: &[crate::table::Value] = &[
    crate::table::Value {
        name: "failed",
        description: "Gave up, or failed in a way that isn't retried",
    },
    crate::table::Value {
        name: "outcome_unknown",
        description: "An outside call may have happened",
    },
    crate::table::Value {
        name: "diverged",
        description: "A rerun no longer matches the record of its steps",
    },
    crate::table::Value {
        name: "unfinished",
        description: "A route's steps whose client didn't retry within a day",
    },
    crate::table::Value {
        name: "pending",
        description: "Waiting to run, or to retry",
    },
    crate::table::Value {
        name: "running",
        description: "A worker has it",
    },
    crate::table::Value {
        name: "held",
        description: "Held while a provider it calls is unavailable",
    },
    crate::table::Value {
        name: "dismissed",
        description: "Set aside by an operator",
    },
    crate::table::Value {
        name: "cancelled",
        description: "Stopped by an operator before it ran",
    },
    crate::table::Value {
        name: "abandoned",
        description: "Ended for good by an operator, its steps kept",
    },
];

/// How many items a page shows.
pub const PAGE: u64 = 20;

/// The fewest hex digits an id is given by.
const SHORTEST_ID: usize = 8;

pub fn deliveries(call: &Call) -> Reply {
    let action = call.arguments.first().map(String::as_str);
    match action {
        None | Some("list") => {
            if let Some(extra) = call.arguments.get(1) {
                return Reply::failed(arguments(
                    format!("`jadpo deliveries` lists them all, not `{extra}`"),
                    "Listing takes no id; `--state` picks which to list.".to_string(),
                    "Run `jadpo deliveries`, or `jadpo deliveries --state pending`".to_string(),
                ));
            }
            if let Some(reply) =
                only_for(call, "--done", "list").or_else(|| only_for(call, "--not-done", "list"))
            {
                return reply;
            }
            match Project::load(call) {
                Ok(project) => list(call, &project),
                Err(reply) => reply,
            }
        }
        Some(action @ ("retry" | "dismiss" | "cancel" | "resolve" | "show" | "abandon")) => {
            if let Some(extra) = call.arguments.get(2) {
                return Reply::failed(arguments(
                    format!("`jadpo deliveries {action}` takes one id, not `{extra}` too"),
                    "Each acts on one delivery or schedule run, by the id the list shows."
                        .to_string(),
                    format!("Run `jadpo deliveries {action} <id>`"),
                ));
            }
            for flag in ["--state", "--page"] {
                if call.value(flag).is_some() {
                    return Reply::failed(arguments(
                        format!("`{flag}` is for listing, not `{action}`"),
                        "Only the list is filtered and paged.".to_string(),
                        format!("Leave out `{flag}`"),
                    ));
                }
            }
            if action != "resolve"
                && let Some(reply) = only_for(call, "--done", action)
            {
                return reply;
            }
            if !matches!(action, "resolve" | "retry")
                && let Some(reply) = only_for(call, "--not-done", action)
            {
                return reply;
            }
            let action = match action {
                "show" => None,
                "retry" if call.flag("--not-done") => Some(Action::RetryNotDone),
                "retry" => Some(Action::Retry),
                "dismiss" => Some(Action::Dismiss),
                "cancel" => Some(Action::Cancel),
                "abandon" => Some(Action::Abandon),
                _ => match (call.flag("--done"), call.flag("--not-done")) {
                    (true, false) => Some(Action::ResolveDone),
                    (false, true) => Some(Action::ResolveNotDone),
                    (both, _) => {
                        return Reply::failed(arguments(
                            if both {
                                "`jadpo deliveries resolve` takes `--done` or `--not-done`, not both"
                                    .to_string()
                            } else {
                                "`jadpo deliveries resolve` needs `--done` or `--not-done`"
                                    .to_string()
                            },
                            "Its outside call may or may not have happened: check with the provider, then say which. Not done retries it.".to_string(),
                            "Add `--done` or `--not-done`".to_string(),
                        ));
                    }
                },
            };
            let written = match call.arguments.get(1) {
                Some(id) => id.clone(),
                None => {
                    return Reply::failed(arguments(
                        format!(
                            "`jadpo deliveries {}` needs an id",
                            call.arguments[0].as_str()
                        ),
                        "Each acts on one delivery or schedule run, by the id the list shows."
                            .to_string(),
                        "Run `jadpo deliveries` to see them, with their ids".to_string(),
                    ));
                }
            };
            let suffix = match id_suffix(&written) {
                Ok(suffix) => suffix,
                Err(reply) => return reply,
            };
            match (Project::load(call), action) {
                (Ok(project), Some(action)) => act(call, &project, &written, &suffix, action),
                (Ok(project), None) => show(call, &project, &written, &suffix),
                (Err(reply), _) => reply,
            }
        }
        Some(other) => {
            let names: Vec<&str> = ACTIONS.iter().map(|action| action.name).collect();
            let near = jadpo_diag::nearest(other, names.iter().copied());
            let all = crate::run::both(&quoted(names.iter().skip(1)));
            Reply::failed(arguments(
                format!("`{other}` isn't something `jadpo deliveries` does"),
                match near {
                    Some(near) => format!(
                        "`jadpo deliveries` lists what needs an operator, and {all} act on one. Did you mean `{near}`?"
                    ),
                    None => format!(
                        "`jadpo deliveries` lists what needs an operator, and {all} act on one."
                    ),
                },
                match near {
                    Some(near) => format!("Use `{near}` instead of `{other}`"),
                    None => "Run `jadpo deliveries`, or `jadpo deliveries retry <id>`".to_string(),
                },
            ))
        }
    }
}

/// `flag` given to an action it isn't for.
fn only_for(call: &Call, flag: &str, action: &str) -> Option<Reply> {
    call.flag(flag).then(|| {
        Reply::failed(if flag == "--not-done" {
            arguments(
                format!("`{flag}` is for `jadpo deliveries retry` or `resolve`, not `{action}`"),
                "Not done says an outside call didn't take effect, so it is sent again: a delivery whose outcome is unknown, or whose step failed.".to_string(),
                format!("Leave out `{flag}`"),
            )
        } else {
            arguments(
                format!("`{flag}` is for `jadpo deliveries resolve`, not `{action}`"),
                "Only a delivery or schedule run whose outcome is unknown is resolved as done or not done.".to_string(),
                format!("Leave out `{flag}`"),
            )
        })
    })
}

fn arguments(message: String, reason: String, next: String) -> Diagnostic {
    Diagnostic::new(&rule::CLI_DELIVERIES_ARGUMENTS, message)
        .reason(reason)
        .next(next)
}

/// The hex digits an id ends with, from an id as the list shows it (its
/// last 8), longer, or whole with its dashes.
fn id_suffix(written: &str) -> Result<String, Reply> {
    let digits: String = written
        .chars()
        .filter(|c| *c != '-')
        .collect::<String>()
        .to_ascii_lowercase();
    if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_hexdigit()) || digits.len() > 32 {
        return Err(Reply::failed(arguments(
            format!("`{written}` isn't an id"),
            "An id is the hex digits the list shows, such as `4fa2c09e`.".to_string(),
            "Run `jadpo deliveries` to see them, with their ids".to_string(),
        )));
    }
    if digits.len() < SHORTEST_ID {
        return Err(Reply::failed(arguments(
            format!("`{written}` is too short to be an id"),
            format!(
                "An id is given by at least its last {SHORTEST_ID} hex digits, as the list shows it."
            ),
            "Use the id as the list shows it".to_string(),
        )));
    }
    Ok(digits)
}

/// The project as these commands need it: the stores holding deliveries
/// and schedules' runs, and its schedules.
struct Project {
    root: PathBuf,
    loaded: jadpo_db::Loaded,
    /// Each store, with what it holds.
    stores: Vec<(String, Holds)>,
    schedules: Vec<std::sync::Arc<ScheduleModel>>,
}

impl Project {
    fn load(call: &Call) -> Result<Project, Reply> {
        let cwd = call.cwd.clone().unwrap_or_else(|| PathBuf::from("."));
        let Some(root) = jadpo_db::find_root(&cwd) else {
            return Err(crate::fmt::project_needed("deliveries"));
        };
        let loaded = jadpo_db::load(&root);
        let schema = jadpo_check::data::schema(&loaded.db, loaded.project);
        let deliveries = jadpo_std::system::DELIVERIES.table;
        let runs = jadpo_std::system::SCHEDULE_RUNS.table;
        let steps = jadpo_std::system::STEP_RECORDS.table;
        let stores = schema
            .stores
            .iter()
            .map(|(name, store)| {
                (
                    name.clone(),
                    Holds {
                        deliveries: store.tables.contains_key(deliveries),
                        runs: store.tables.contains_key(runs),
                        steps: store.tables.contains_key(steps),
                    },
                )
            })
            .filter(|(_, holds)| holds.deliveries || holds.runs || holds.steps)
            .collect();
        let schedules = jadpo_check::schedules::schedules(&loaded.db, loaded.project)
            .iter()
            .cloned()
            .collect();
        Ok(Project {
            root,
            loaded,
            stores,
            schedules,
        })
    }

    /// Each store with its database's address: all of them, or with
    /// `--url`, the one it names.
    fn databases(&self, call: &Call) -> Result<Vec<(String, Holds, String)>, Reply> {
        if call.value("--url").is_some() {
            let chosen = match call.value("--store") {
                Some(store) => match self.stores.iter().find(|(name, _)| name == store) {
                    Some(found) => found.clone(),
                    None => {
                        let names: Vec<String> =
                            self.stores.iter().map(|(name, _)| name.clone()).collect();
                        return Err(Reply::failed(arguments(
                            format!(
                                "`{store}` isn't a store that keeps deliveries or schedule runs"
                            ),
                            format!("They are kept in {}.", crate::run::both(&quoted(&names))),
                            format!("Use {}", crate::run::either(&quoted(&names))),
                        )));
                    }
                },
                None => match self.stores.as_slice() {
                    [only] => only.clone(),
                    [] => return Ok(Vec::new()),
                    many => {
                        let names: Vec<String> =
                            many.iter().map(|(name, _)| format!("`{name}`")).collect();
                        return Err(Reply::failed(arguments(
                            "Which store is this database?".to_string(),
                            format!(
                                "Deliveries and schedule runs are kept in {}, and `--url` names one database.",
                                crate::run::both(&names)
                            ),
                            "Add `--store <name>`".to_string(),
                        )));
                    }
                },
            };
            let url = call.value("--url").unwrap_or_default().to_string();
            return Ok(vec![(chosen.0, chosen.1, url)]);
        }
        if let Some(store) = call.value("--store") {
            return Err(Reply::failed(arguments(
                format!("`--store {store}` goes with `--url`"),
                "Without `--url`, every store's database is read, each from its connection's setting.".to_string(),
                "Add `--url postgresql://…`, or leave out `--store`".to_string(),
            )));
        }
        let mut out = Vec::new();
        for (store, holds) in &self.stores {
            match url_of(call, &self.root, &self.loaded, store)? {
                Some(url) => out.push((store.clone(), *holds, url)),
                None => {
                    return Err(Reply::failed(arguments(
                        format!("`jadpo deliveries` needs the address of the database `{store}`"),
                        "No variable gives the address of its connection, and no `jadpo dev` is running with one: a connection reads its address from the setting its `url:` names (area 10), and `--url` names a database directly.".to_string(),
                        "Run `jadpo config set <the connection's setting>`, start `jadpo dev`, or add `--url postgresql://…`".to_string(),
                    )));
                }
            }
        }
        Ok(out)
    }

    /// A schedule as its key names it, for asking the tick to run it.
    fn declared(&self, key: &str) -> Option<Declared> {
        let found = self.schedules.iter().find(|s| s.key.as_str() == key)?;
        Some(Declared {
            key: key.to_string(),
            name: self.schedule_name(key),
            every_ms: found.every_ms,
        })
    }

    /// What a schedule is called on screen: its name, or `domain.name`
    /// when another domain has one of the same name.
    fn schedule_name(&self, key: &str) -> String {
        let qualified = key.split_once(' ').map_or(key, |(_, rest)| rest);
        let Some(found) = self.schedules.iter().find(|s| s.key.as_str() == key) else {
            return qualified.to_string();
        };
        let same = self
            .schedules
            .iter()
            .filter(|other| other.name == found.name)
            .count();
        if same > 1 {
            qualified.to_string()
        } else {
            found.name.clone()
        }
    }

    fn period(&self) -> u64 {
        let declared: Vec<Declared> = self
            .schedules
            .iter()
            .map(|s| Declared {
                key: s.key.as_str().to_string(),
                name: s.name.clone(),
                every_ms: s.every_ms,
            })
            .collect();
        tick_every(&declared)
    }
}

/// Runs `work` against the database at `url`; a failure to reach or read
/// it is `deliveries.unavailable`.
fn with_database<T>(
    store: &str,
    url: &str,
    work: impl for<'d> FnOnce(
        &'d dyn jadpo_runtime::Database,
    )
        -> jadpo_runtime::LocalFuture<'d, Result<T, jadpo_runtime::StoreError>>,
) -> Result<T, Reply> {
    let failed = |error: String| {
        Reply::failed(
            Diagnostic::new(
                &rule::DELIVERIES_UNAVAILABLE,
                format!("The database `{store}` couldn't be read"),
            )
            .reason(format!(
                "Reading its deliveries failed: {error}. Its tables come with its migrations."
            ))
            .next("Check the database is running, then run `jadpo migrate apply`"),
        )
    };
    let runtime = jadpo_host_native::Runtime::new().map_err(|error| failed(error.to_string()))?;
    let database = jadpo_host_native::postgres(url).map_err(|error| failed(store_error(&error)))?;
    runtime
        .block_on(work(&database))
        .map_err(|error| failed(store_error(&error)))
}

/// Who the operator is, as the machine names them: `USER`, else
/// `LOGNAME` or `USERNAME`.
fn operator(call: &Call) -> String {
    ["USER", "LOGNAME", "USERNAME"]
        .iter()
        .find_map(|name| {
            call.environment
                .var(name)
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        })
        .unwrap_or_else(|| "unknown".to_string())
}

// --- words ------------------------------------------------------------------

/// A key without its kind and, when it has one, its domain:
/// `subscriber home.ring_bell` is `ring_bell`.
fn bare(key: &str) -> String {
    let qualified = key.split_once(' ').map_or(key, |(_, rest)| rest);
    match qualified.split_once('.') {
        Some((_, name)) if !name.contains('.') => name.to_string(),
        _ => qualified.to_string(),
    }
}

/// An item as a sentence names it: `` `CensusTaken` to `ring_bell` ``,
/// or `` `bell_check` ``.
fn subject(project: &Project, item: &Item) -> String {
    match item.kind {
        Kind::Delivery => format!(
            "`{}` to `{}`",
            item.event.as_deref().map(bare).unwrap_or_default(),
            bare(&item.target)
        ),
        Kind::ScheduleRun => format!("`{}`", project.schedule_name(&item.target)),
        Kind::Request => format!("`{}`'s steps", bare(&item.target)),
    }
}

/// An item as the list's cell shows it.
fn item_cell(project: &Project, item: &Item) -> String {
    match item.kind {
        Kind::Delivery => format!(
            "{} → {}",
            item.event.as_deref().map(bare).unwrap_or_default(),
            bare(&item.target)
        ),
        Kind::ScheduleRun => format!("schedule {}", project.schedule_name(&item.target)),
        Kind::Request => format!("request → {}", bare(&item.target)),
    }
}

fn state_cell(state: &str) -> Cell {
    match state {
        FAILED => Cell::state(Symbol::Failure, "failed"),
        OUTCOME_UNKNOWN => Cell::state(Symbol::Decision, "outcome unknown"),
        PENDING => Cell::state(Symbol::Pending, "pending"),
        RUNNING => Cell::state(Symbol::Running, "running"),
        HELD => Cell::state(Symbol::Paused, "held"),
        DISMISSED => Cell::state(Symbol::Skipped, "dismissed"),
        CANCELLED => Cell::state(Symbol::Skipped, "cancelled"),
        DIVERGED => Cell::state(Symbol::Decision, "diverged"),
        UNFINISHED => Cell::state(Symbol::Decision, "unfinished"),
        ABANDONED => Cell::state(Symbol::Skipped, "abandoned"),
        other => Cell::state(Symbol::Skipped, other.replace('_', " ")),
    }
}

/// A state as a sentence says it: "failed", "is pending".
fn state_words(state: &str) -> &'static str {
    match state {
        FAILED => "failed",
        OUTCOME_UNKNOWN => "is in doubt",
        PENDING => "is pending",
        RUNNING => "is running",
        HELD => "is held while a provider it calls is unavailable",
        DISMISSED => "was dismissed",
        CANCELLED => "was cancelled",
        DIVERGED => "no longer matches the record of its steps",
        UNFINISHED => "ended without finishing, and its client didn't retry within a day",
        ABANDONED => "was abandoned",
        runtime::SUCCEEDED => "succeeded",
        runtime::RESOLVED => "was resolved as done",
        _ => "was followed by a later run",
    }
}

/// The last column's header for a list of `state`.
fn when_header(state: Option<&str>) -> &'static str {
    match state {
        Some(PENDING) => "next attempt (UTC)",
        Some(RUNNING) => "started (UTC)",
        Some(HELD) => "held since (UTC)",
        Some(DISMISSED) => "dismissed (UTC)",
        Some(CANCELLED) => "cancelled (UTC)",
        Some(ABANDONED) => "abandoned (UTC)",
        Some(UNFINISHED) => "last run (UTC)",
        _ => "last tried (UTC)",
    }
}

fn when_cell(item: &Item, now: i64) -> String {
    let code = item.code.as_deref();
    match item.state.as_str() {
        PENDING => match (item.kind, item.next_attempt) {
            (Kind::ScheduleRun, _) => "next tick · asked by hand".to_string(),
            (_, Some(next)) => match code {
                Some(code) => format!("{} · after {code}", clock(next, now)),
                None => clock(next, now),
            },
            (_, None) => "now".to_string(),
        },
        RUNNING => item.last_tried.map(|at| clock(at, now)).unwrap_or_default(),
        HELD => match (item.held_since, &item.held_for) {
            (Some(since), Some(connection)) => format!("{} · {connection}", clock(since, now)),
            (Some(since), None) => clock(since, now),
            (None, Some(connection)) => connection.clone(),
            (None, None) => String::new(),
        },
        DISMISSED | CANCELLED | ABANDONED => match &item.acted {
            Some(acted) => format!("{} · {}", clock(acted.at, now), acted.operator),
            None => String::new(),
        },
        _ => {
            let at = item.last_tried.map(|at| clock(at, now));
            match (at, code) {
                (Some(at), Some(code)) => format!("{at} · {code}"),
                (Some(at), None) => at,
                (None, Some(code)) => code.to_string(),
                (None, None) => "never tried".to_string(),
            }
        }
    }
}

fn instant(at: Option<i64>) -> Json {
    at.map_or(Json::Null, |at| {
        Json::from(jadpo_std::format_instant(at).as_str())
    })
}

fn item_json(project: &Project, store: &str, item: &Item) -> Json {
    Json::object([
        ("id", Json::from(runtime::short(&item.id).as_str())),
        ("uuid", Json::from(uuid_text(&item.id).as_str())),
        ("kind", Json::from(item.kind.word())),
        ("store", Json::from(store)),
        (
            "subscriber",
            match item.kind {
                Kind::Delivery => Json::from(item.target.as_str()),
                Kind::ScheduleRun | Kind::Request => Json::Null,
            },
        ),
        (
            "schedule",
            match item.kind {
                Kind::ScheduleRun => Json::from(project.schedule_name(&item.target).as_str()),
                Kind::Delivery | Kind::Request => Json::Null,
            },
        ),
        (
            "action",
            match item.kind {
                Kind::Request => Json::from(item.target.as_str()),
                Kind::Delivery | Kind::ScheduleRun => Json::Null,
            },
        ),
        (
            "event",
            item.event.as_deref().map_or(Json::Null, Json::from),
        ),
        ("state", Json::from(item.state.as_str())),
        (
            "attempts",
            item.attempts
                .map_or(Json::Null, |n| Json::from(i64::from(n))),
        ),
        ("lastTriedAt", instant(item.last_tried)),
        ("nextAttemptAt", instant(item.next_attempt)),
        ("code", item.code.as_deref().map_or(Json::Null, Json::from)),
        (
            "failureKind",
            item.failure_kind.as_deref().map_or(Json::Null, Json::from),
        ),
        (
            "heldFor",
            item.held_for.as_deref().map_or(Json::Null, Json::from),
        ),
        ("heldSince", instant(item.held_since)),
        (
            "lastAction",
            item.acted.as_ref().map_or(Json::Null, |acted| {
                Json::object([
                    ("action", Json::from(acted.action.as_str())),
                    ("at", instant(Some(acted.at))),
                    ("operator", Json::from(acted.operator.as_str())),
                    ("role", Json::from(acted.role.as_str())),
                ])
            }),
        ),
    ])
}

fn uuid_text(id: &[u8; 16]) -> String {
    let hex = runtime::hex(id);
    format!(
        "{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..]
    )
}

// --- the list ---------------------------------------------------------------

fn list(call: &Call, project: &Project) -> Reply {
    let state = match call.value("--state") {
        None => None,
        Some(state) if runtime::listable(state) => Some(state.to_string()),
        Some(state) => {
            let names: Vec<&str> = STATES.iter().map(|s| s.name).collect();
            let near = jadpo_diag::nearest(state, names.iter().copied());
            return Reply::failed(arguments(
                format!("`{state}` isn't a state"),
                format!(
                    "The states listed are {}.{}",
                    crate::run::both(&quoted(&names)),
                    near.map(|near| format!(" Did you mean `{near}`?"))
                        .unwrap_or_default()
                ),
                match near {
                    Some(near) => format!("Use `--state {near}`"),
                    None => "Use `--state pending`, or leave it out".to_string(),
                },
            ));
        }
    };
    let page = match call.value("--page") {
        None => 1,
        Some(written) => match written.parse::<u64>() {
            Ok(page) if page >= 1 => page,
            _ => {
                return Reply::failed(arguments(
                    format!("`{written}` isn't a page"),
                    "Pages are counted from 1, newest first.".to_string(),
                    "Use a page such as `--page 2`".to_string(),
                ));
            }
        },
    };
    let states: Vec<String> = match &state {
        Some(state) => vec![state.clone()],
        None => NEEDS_AN_OPERATOR.iter().map(|s| s.to_string()).collect(),
    };
    let databases = match project.databases(call) {
        Ok(found) => found,
        Err(reply) => return reply,
    };
    let now = now(call);
    let limit = i64::try_from(page.saturating_mul(PAGE)).unwrap_or(i64::MAX);
    let mut items: Vec<(String, Item)> = Vec::new();
    let mut counts: BTreeMap<String, u64> = BTreeMap::new();
    let mut unavailable: Vec<(String, Unavailable)> = Vec::new();
    for (store, holds, url) in &databases {
        let states = states.clone();
        let read = with_database(store, url, |database| {
            let holds = *holds;
            Box::pin(async move {
                let states: Vec<&str> = states.iter().map(String::as_str).collect();
                let items = runtime::items(database, holds, &states, limit, now).await?;
                let counts = runtime::counts(database, holds, now).await?;
                // The connections whose circuit breakers hold this store's
                // deliveries (chunk 7.2b).
                let unavailable = if holds.deliveries {
                    jadpo_runtime::breaker::unavailable(database).await?
                } else {
                    Vec::new()
                };
                Ok((items, counts, unavailable))
            })
        });
        let (found, counted, open) = match read {
            Ok(read) => read,
            Err(reply) => return reply,
        };
        unavailable.extend(open.into_iter().map(|open| (store.clone(), open)));
        items.extend(found.into_iter().map(|item| (store.clone(), item)));
        for (state, count) in counted {
            *counts.entry(state).or_default() += count;
        }
    }
    // Dismissed and cancelled runs aren't counted by the store; the list
    // is what there is of them.
    let total: u64 = match state.as_deref() {
        Some(state @ (DISMISSED | CANCELLED)) => {
            let runs = items
                .iter()
                .filter(|(_, item)| item.kind == Kind::ScheduleRun)
                .count() as u64;
            counts.get(state).copied().unwrap_or(0) + runs
        }
        _ => states
            .iter()
            .map(|state| counts.get(state.as_str()).copied().unwrap_or(0))
            .sum(),
    };
    let mut all: Vec<Item> = Vec::new();
    let mut stores: BTreeMap<[u8; 16], String> = BTreeMap::new();
    for (store, item) in items {
        stores.insert(item.id, store);
        all.push(item);
    }
    runtime::sort(&mut all);
    let pages = total.div_ceil(PAGE).max(1);
    if page > pages && total > 0 {
        return Reply::failed(arguments(
            format!("There is no page {page}"),
            format!(
                "{} make {}.",
                plural(total, "item", "items"),
                plural(pages, "page", "pages")
            ),
            format!("Use a page from 1 to {pages}"),
        ));
    }
    let shown: Vec<Item> = all
        .into_iter()
        .skip(usize::try_from((page - 1) * PAGE).unwrap_or(usize::MAX))
        .take(PAGE as usize)
        .collect();
    let data = Json::object([
        ("state", state.as_deref().map_or(Json::Null, Json::from)),
        ("page", Json::from(i64::try_from(page).unwrap_or(i64::MAX))),
        (
            "pages",
            Json::from(i64::try_from(pages).unwrap_or(i64::MAX)),
        ),
        (
            "total",
            Json::from(i64::try_from(total).unwrap_or(i64::MAX)),
        ),
        (
            "items",
            Json::array(shown.iter().map(|item| {
                item_json(
                    project,
                    stores.get(&item.id).map(String::as_str).unwrap_or(""),
                    item,
                )
            })),
        ),
        (
            "unavailable",
            Json::array(
                unavailable
                    .iter()
                    .map(|(store, open)| unavailable_json(store, open)),
            ),
        ),
        (
            "counts",
            Json::object(
                counts
                    .iter()
                    .map(|(state, n)| {
                        (
                            state.clone(),
                            Json::from(i64::try_from(*n).unwrap_or(i64::MAX)),
                        )
                    })
                    .collect::<Vec<(String, Json)>>(),
            ),
        ),
    ]);
    let alerts: Vec<String> = match state.as_deref() {
        None | Some(HELD) => unavailable
            .iter()
            .map(|(_, open)| unavailable_text(open, now))
            .collect(),
        Some(_) => Vec::new(),
    };
    Reply::screen(
        list_screen(
            project,
            state.as_deref(),
            &shown,
            &counts,
            &alerts,
            total,
            page,
            pages,
            now,
        ),
        data,
    )
}

/// A connection whose circuit breaker holds deliveries, as an alert's
/// line says it (chunk 7.2b).
fn unavailable_text(open: &Unavailable, now: i64) -> String {
    let since = open
        .since
        .map(|since| format!(" since {}", clock(since, now)))
        .unwrap_or_default();
    let next = if open.probing {
        "one being tried now".to_string()
    } else {
        match open.probe_at {
            Some(at) if at > now => format!("one tried again at {}", clock(at, now)),
            _ => "one tried again on the next tick".to_string(),
        }
    };
    format!(
        "`{}` has been unavailable{since}: {} held, {next}",
        open.connection,
        plural(open.held, "delivery", "deliveries")
    )
}

fn unavailable_json(store: &str, open: &Unavailable) -> Json {
    Json::object([
        ("connection", Json::from(open.connection.as_str())),
        ("store", Json::from(store)),
        ("probing", Json::from(open.probing)),
        ("since", instant(open.since)),
        ("nextProbeAt", instant(open.probe_at)),
        (
            "held",
            Json::from(i64::try_from(open.held).unwrap_or(i64::MAX)),
        ),
    ])
}

#[allow(clippy::too_many_arguments)]
fn list_screen(
    project: &Project,
    state: Option<&str>,
    shown: &[Item],
    counts: &BTreeMap<String, u64>,
    alerts: &[String],
    total: u64,
    page: u64,
    pages: u64,
    now: i64,
) -> Screen {
    let count = |state: &str| counts.get(state).copied().unwrap_or(0);
    let mut screen = Screen::new();
    if shown.is_empty() {
        if !alerts.is_empty() {
            screen.push(Block::Alerts(alerts.to_vec()));
        }
        let (headline, facts) = match state {
            None => (
                "Nothing needs an operator".to_string(),
                [(HELD, "held"), (PENDING, "pending"), (RUNNING, "running")]
                    .into_iter()
                    .filter(|(state, _)| count(state) > 0)
                    .map(|(state, word)| Fact::plain(format!("{} {word}", count(state))))
                    .collect::<Vec<_>>(),
            ),
            Some(state) => (
                match state {
                    FAILED => "Nothing failed",
                    OUTCOME_UNKNOWN => "Nothing is in doubt",
                    PENDING => "Nothing is pending",
                    RUNNING => "Nothing is running",
                    HELD => "Nothing is held",
                    DIVERGED => "Nothing diverged",
                    UNFINISHED => "Nothing is unfinished",
                    ABANDONED => "Nothing was abandoned",
                    DISMISSED => "Nothing was dismissed",
                    _ => "Nothing was cancelled",
                }
                .to_string(),
                Vec::new(),
            ),
        };
        screen.push(Block::summary(Outcome::Success, headline, facts));
        return screen;
    }
    // Every item listed is in one state, or the default's two.
    let mut table = Table::new(["id", "item", "state", when_header(state)]).cut(1);
    for item in shown {
        table = table.row(vec![
            Cell::text(runtime::short(&item.id)),
            Cell::text(item_cell(project, item)),
            state_cell(&item.state),
            Cell::muted(when_cell(item, now)),
        ]);
    }
    screen.push(Block::Table(table));
    if !alerts.is_empty() {
        screen.push(Block::Alerts(alerts.to_vec()));
    }
    let mut facts = Vec::new();
    let (outcome, headline) = match state {
        None => {
            // The first state that holds any is the headline, the others
            // facts in the decision colour, as each needs a person.
            let needing: Vec<(u64, &str)> = [
                (FAILED, "failed"),
                (OUTCOME_UNKNOWN, "outcome unknown"),
                (DIVERGED, "diverged"),
                (UNFINISHED, "unfinished"),
            ]
            .into_iter()
            .map(|(state, word)| (count(state), word))
            .filter(|(n, _)| *n > 0)
            .collect();
            let headline = match needing.split_first() {
                Some(((n, word), rest)) => {
                    for (n, word) in rest {
                        facts.push(Fact {
                            text: format!("{n} {word}"),
                            tone: Some(Token::Decision),
                        });
                    }
                    (Outcome::Failure, format!("{n} {word}"))
                }
                None => (Outcome::Failure, "0 failed".to_string()),
            };
            let held = count(HELD);
            if held > 0 {
                facts.push(Fact::plain(format!("{held} held")));
            }
            headline
        }
        Some(state) => (
            Outcome::Success,
            format!(
                "{total} {}",
                match state {
                    OUTCOME_UNKNOWN => "outcome unknown",
                    other => other,
                }
            ),
        ),
    };
    if pages > 1 {
        facts.push(Fact::plain(format!("page {page} of {pages}")));
    }
    screen.push(
        Block::summary(outcome, headline, facts).with_plain_headline(match state {
            None => "deliveries need attention",
            Some(_) => "deliveries",
        }),
    );
    // The one next step, for the first item it applies to.
    let hint = shown.iter().find_map(|item| {
        let id = runtime::short(&item.id);
        match item.state.as_str() {
            FAILED => Some(match item.kind {
                Kind::Delivery => (
                    format!("jadpo deliveries retry {id}"),
                    "sends it again, once the cause is fixed",
                ),
                Kind::ScheduleRun => (
                    format!("jadpo deliveries retry {id}"),
                    "runs it again, once the cause is fixed",
                ),
                Kind::Request => (
                    format!("jadpo deliveries show {id}"),
                    "lists what took effect at each step",
                ),
            }),
            PENDING => Some((format!("jadpo deliveries cancel {id}"), "stops it running")),
            DIVERGED => Some((
                format!("jadpo deliveries show {id}"),
                "says which step differs from its record",
            )),
            UNFINISHED => Some((
                format!("jadpo deliveries show {id}"),
                "lists what took effect at each step",
            )),
            _ => None,
        }
    });
    let hint = hint.or_else(|| {
        shown
            .iter()
            .find(|item| item.state == OUTCOME_UNKNOWN)
            .map(|item| {
                (
                    format!(
                        "jadpo deliveries resolve {} --done",
                        runtime::short(&item.id)
                    ),
                    "once the provider shows it happened",
                )
            })
    });
    if let Some((command, reason)) = hint {
        screen.push(Block::hint(command, reason));
    }
    screen
}

// --- actions ----------------------------------------------------------------

/// Each store's database, with what it holds and its address.
type Databases = Vec<(String, Holds, String)>;

/// The one item whose id ends with `suffix` at `now`, with the store it
/// is in and its database's address, and every store's database:
/// `deliveries.not_found` when none has it, `cli.deliveries_arguments`
/// when several do.
fn locate(
    call: &Call,
    project: &Project,
    written: &str,
    suffix: &str,
    command: &str,
    now: i64,
) -> Result<(Databases, String, String, Item), Reply> {
    let databases = project.databases(call)?;
    let mut found: Vec<(String, String, Item)> = Vec::new();
    for (store, holds, url) in &databases {
        let items = with_database(store, url, |database| {
            let holds = *holds;
            let suffix = suffix.to_string();
            Box::pin(async move { runtime::find(database, holds, &suffix, now).await })
        })?;
        found.extend(
            items
                .into_iter()
                .map(|item| (store.clone(), url.clone(), item)),
        );
    }
    match found.as_slice() {
        [] => Err(Reply::failed(
            Diagnostic::new(
                &rule::DELIVERIES_NOT_FOUND,
                format!("No delivery or schedule run has the id `{written}`"),
            )
            .reason(
                "`jadpo deliveries` lists those that need an operator, and `--state` the others; a delivery whose event every subscriber had is deleted after 24 hours, and a schedule's run after 30 days.",
            )
            .next("Run `jadpo deliveries` to see them, with their ids"),
        )),
        [one] => {
            let (store, url, item) = one.clone();
            Ok((databases, store, url, item))
        }
        many => {
            let ids: Vec<String> = many
                .iter()
                .map(|(_, _, item)| format!("`{}`", runtime::hex(&item.id)))
                .collect();
            Err(Reply::failed(arguments(
                format!("More than one item has an id ending `{written}`"),
                format!("They are {}.", crate::run::both(&ids)),
                format!(
                    "Use more of the id, such as `jadpo deliveries {command} {}`",
                    runtime::hex(&many[0].2.id)
                ),
            )))
        }
    }
}

/// The record of `item`'s steps, when its action makes several changing
/// calls (chunk 7.5c), with the store it is in and its address: a
/// delivery's in whichever store keeps it, a route's in its own.
fn record_of(
    databases: &Databases,
    store: &str,
    url: &str,
    item: &Item,
) -> Result<Option<(String, String, Record)>, Reply> {
    match item.kind {
        Kind::ScheduleRun => Ok(None),
        Kind::Request => {
            let id = item.id;
            let found = with_database(store, url, |database| {
                Box::pin(async move { unfinished::by_id(database, &id).await })
            })?;
            Ok(found.map(|record| (store.to_string(), url.to_string(), record)))
        }
        Kind::Delivery => {
            for (other, holds, other_url) in databases {
                if !holds.steps {
                    continue;
                }
                let id = item.id;
                let found = with_database(other, other_url, |database| {
                    Box::pin(async move { unfinished::of_delivery(database, &id).await })
                })?;
                if let Some(record) = found {
                    return Ok(Some((other.clone(), other_url.clone(), record)));
                }
            }
            Ok(None)
        }
    }
}

/// `deliveries.wrong_state` for an action a record of steps rules out, or
/// that needs one (chunk 7.5c), with what applies instead.
fn record_rules_out(item: &Item, action: Action, record: Option<&Record>, id: &str) -> Option<Diagnostic> {
    let refused = |message: String, reason: String, next: String| {
        Some(
            Diagnostic::new(&rule::DELIVERIES_WRONG_STATE, message)
                .reason(reason)
                .next(next),
        )
    };
    match (record, action) {
        (Some(record), Action::Dismiss | Action::ResolveDone) => {
            let answered = record
                .steps
                .iter()
                .filter(|step| step.outcome.is_some())
                .count() as u64;
            refused(
                format!(
                    "`{id}` made changing calls that may have taken effect, so it is abandoned, not {}",
                    if action == Action::Dismiss {
                        "dismissed"
                    } else {
                        "resolved as done"
                    }
                ),
                format!(
                    "Its action, `{}`, keeps a record of {} ({} answered) and never committed its writes. Abandoning it ends it for good and keeps that list 30 days, for tidying up at the provider by hand.",
                    bare(&record.action),
                    plural(record.steps.len() as u64, "step", "steps"),
                    answered
                ),
                format!("Run `jadpo deliveries show {id}`, then `jadpo deliveries abandon {id}`"),
            )
        }
        (None, Action::Abandon) => refused(
            format!("`{id}` keeps no record of steps, so there is nothing to abandon"),
            "Abandon ends an action with several changing calls whose steps are recorded; this delivery's action makes at most one.".to_string(),
            match item.state.as_str() {
                OUTCOME_UNKNOWN => format!(
                    "Check with the provider, then run `jadpo deliveries resolve {id} --done` or `--not-done`"
                ),
                _ => format!("Run `jadpo deliveries dismiss {id}`"),
            },
        ),
        (Some(record), Action::Retry | Action::RetryNotDone | Action::ResolveNotDone)
            if record.dropped() =>
        {
            refused(
                format!("`{id}`'s answers were dropped after 30 days, so it can't be retried"),
                format!(
                    "A retry replays each answered step of `{}` from its record, and those answers are kept 30 days after its last run; only the step list is left.",
                    bare(&record.action)
                ),
                format!("Run `jadpo deliveries show {id}`, then `jadpo deliveries abandon {id}`"),
            )
        }
        _ => None,
    }
}

fn act(call: &Call, project: &Project, written: &str, suffix: &str, action: Action) -> Reply {
    let now = now(call);
    let command = action_command(action);
    let (databases, store, url, item) =
        match locate(call, project, written, suffix, command, now) {
            Ok(found) => found,
            Err(reply) => return reply,
        };
    let id = runtime::short(&item.id);
    if !action.fits(&item) {
        return Reply::failed(wrong_state(project, &item, action, &id));
    }
    let record = match record_of(&databases, &store, &url, &item) {
        Ok(record) => record,
        Err(reply) => return reply,
    };
    if let Some(refused) =
        record_rules_out(&item, action, record.as_ref().map(|(_, _, record)| record), &id)
    {
        return Reply::failed(refused);
    }
    // "Not done" with no record is what it was before records: a retry of
    // a failed delivery, or a resolution as not done of one in doubt.
    let action = match (action, &record) {
        (Action::RetryNotDone, None) if item.state == OUTCOME_UNKNOWN => Action::ResolveNotDone,
        (Action::RetryNotDone, None) => Action::Retry,
        _ => action,
    };
    let operator = operator(call);
    // A schedule's run asked of the app's tick: logged, then asked for
    // and waited for, as `jadpo schedules run` against a deployment.
    if item.kind == Kind::ScheduleRun && action.asks_for_a_run() {
        let Some(declared) = project.declared(&item.target) else {
            return Reply::failed(
                Diagnostic::new(
                    &rule::DELIVERIES_WRONG_STATE,
                    format!("`{}` isn't a schedule this project declares", bare(&item.target)),
                )
                .reason("Its run is recorded, but the project no longer has it, so nothing can run it again.")
                .next(format!("Run `jadpo deliveries dismiss {id}`")),
            );
        };
        let logged = with_database(&store, &url, |database| {
            let item = item.clone();
            let operator = operator.clone();
            Box::pin(async move {
                let host = jadpo_host_native::NativeHost::new();
                runtime::log_run(database, &host, &item, action, &operator, now).await
            })
        });
        if let Err(reply) = logged {
            return reply;
        }
        return crate::schedules::ask_tick(&declared, project.period(), now, &store, &url);
    }
    // What the action does to the record of its steps (chunk 7.5c).
    let sends_again = match (action, &record) {
        (Action::RetryNotDone | Action::ResolveNotDone, Some((_, _, record))) => {
            record.in_doubt().cloned()
        }
        _ => None,
    };
    let change = match (action, &record) {
        (Action::Retry, Some((_, _, record))) => Some(unfinished::Change::Retry {
            record: record.id,
            not_done: None,
        }),
        (Action::RetryNotDone | Action::ResolveNotDone, Some((_, _, record))) => {
            Some(unfinished::Change::Retry {
                record: record.id,
                not_done: sends_again.as_ref().map(|step| step.position),
            })
        }
        (Action::Abandon, Some((_, _, record))) => Some(unfinished::Change::Abandon {
            record: record.id,
        }),
        _ => None,
    };
    // A record kept in another store than its delivery's is changed
    // first, on its own; in the same store, with the delivery, in one
    // statement.
    let alone = change.filter(|_| {
        item.kind == Kind::Delivery
            && record
                .as_ref()
                .is_some_and(|(record_store, _, _)| *record_store != store)
    });
    if let (Some(change), Some((record_store, record_url, _))) = (alone, &record) {
        let changed = with_database(record_store, record_url, |database| {
            Box::pin(async move { unfinished::change_alone(database, change, now).await })
        });
        if let Err(reply) = changed {
            return reply;
        }
    }
    let done = with_database(&store, &url, |database| {
        let item = item.clone();
        let operator = operator.clone();
        let together = if alone.is_some() { None } else { change };
        Box::pin(async move {
            let host = jadpo_host_native::NativeHost::new();
            match (item.kind, action) {
                (Kind::Request, _) => {
                    unfinished::abandon_request(database, &host, &item, &operator, now).await
                }
                (Kind::Delivery, _) => {
                    runtime::act_on_delivery_and_record(
                        database, &host, &item, action, &operator, now, together,
                    )
                    .await
                }
                (Kind::ScheduleRun, Action::Cancel) => {
                    runtime::cancel_request(database, &host, &item, &operator, now).await
                }
                (Kind::ScheduleRun, _) => {
                    runtime::settle_run(database, &host, &item, action, &operator, now).await
                }
            }
        })
    });
    let role = match done {
        Ok(Done::Logged { role }) => role,
        Ok(Done::Changed) => {
            return Reply::failed(
                Diagnostic::new(
                    &rule::DELIVERIES_WRONG_STATE,
                    format!("`{id}` changed while it was being read"),
                )
                .reason(format!(
                    "It {} when it was read, and the app or another operator changed it first, so nothing was done.",
                    state_words(&item.state)
                ))
                .next("Run `jadpo deliveries --state <state>` to see where it is now"),
            );
        }
        Err(reply) => return reply,
    };
    let subject = subject(project, &item);
    let kept_until = jadpo_std::format_instant(now.saturating_add(unfinished::ANSWERS_KEPT_MS));
    let kept_until = kept_until.get(..10).unwrap_or_default();
    let text = match (item.kind, action) {
        (_, Action::Abandon) => format!(
            "Abandoned `{id}`: `{}` won't finish, and its steps are listed until {kept_until}",
            record
                .as_ref()
                .map(|(_, _, record)| bare(&record.action))
                .unwrap_or_else(|| bare(&item.target))
        ),
        (Kind::Delivery, Action::RetryNotDone | Action::ResolveNotDone)
            if record.is_some() =>
        {
            match &sends_again {
                Some(step) => format!(
                    "Queued `{id}` for retry: step {}, `{}`, goes again under a new key on the app's next tick",
                    step.position, step.operation
                ),
                None => format!(
                    "Queued `{id}` for retry: every step was answered, so the app's next tick replays them and finishes {}",
                    delivery_words(&item)
                ),
            }
        }
        (Kind::Delivery, Action::Retry | Action::RetryNotDone) => format!(
            "Queued `{id}` for retry: the app's next tick sends {} again, with a fresh retry budget",
            delivery_words(&item)
        ),
        (Kind::Delivery, Action::Dismiss) => format!(
            "Dismissed `{id}`: `{}` won't receive `{}`",
            bare(&item.target),
            item.event.as_deref().map(bare).unwrap_or_default()
        ),
        (Kind::Delivery, Action::Cancel) => format!(
            "Cancelled `{id}`: `{}` won't receive `{}`",
            bare(&item.target),
            item.event.as_deref().map(bare).unwrap_or_default()
        ),
        (Kind::Delivery, Action::ResolveDone) => format!(
            "Resolved `{id}` as done: `{}` took effect, so it isn't retried",
            bare(&item.target)
        ),
        (Kind::Delivery | Kind::Request, Action::ResolveNotDone) => format!(
            "Resolved `{id}` as not done: the app's next tick sends {} again",
            delivery_words(&item)
        ),
        (Kind::ScheduleRun, Action::Cancel) => {
            format!("Cancelled `{id}`: the run asked of {subject} won't happen")
        }
        (Kind::ScheduleRun, Action::ResolveDone) => {
            format!("Resolved `{id}` as done: {subject}'s run took effect")
        }
        (Kind::ScheduleRun, _) => {
            let from = item
                .last_tried
                .map(|at| format!(" from {}", clock(at, now)))
                .unwrap_or_default();
            format!("Dismissed `{id}`: {subject}'s failed run{from}")
        }
        // Only abandon reaches a route's record.
        (Kind::Request, _) => format!("Acted on `{id}`"),
    };
    let mut screen = Screen::new().with(Block::Status {
        outcome: Outcome::Success,
        text,
        rows: vec![("logged".to_string(), format!("{operator} · role {role}"))],
    });
    if action == Action::Abandon {
        screen.push(Block::hint(
            format!("jadpo deliveries show {id}"),
            "lists what took effect, to tidy up at the provider",
        ));
    }
    Reply::screen(
        screen,
        Json::object([
            ("id", Json::from(id.as_str())),
            ("uuid", Json::from(uuid_text(&item.id).as_str())),
            ("kind", Json::from(item.kind.word())),
            ("store", Json::from(store.as_str())),
            ("action", Json::from(action.word())),
            ("was", Json::from(item.state.as_str())),
            (
                "sendsAgain",
                sends_again.as_ref().map_or(Json::Null, |step| {
                    Json::object([
                        ("step", Json::from(i64::from(step.position))),
                        ("operation", Json::from(step.operation.as_str())),
                        (
                            "keyGeneration",
                            Json::from(i64::from(step.key_generation) + 1),
                        ),
                    ])
                }),
            ),
            ("operator", Json::from(operator.as_str())),
            ("role", Json::from(role.as_str())),
            ("at", instant(Some(now))),
        ]),
    )
}

// --- one item ----------------------------------------------------------------

/// `jadpo deliveries show <id>`: one item, and when its action makes
/// several changing calls, each recorded step (chunk 7.5c): its
/// operation, whether and when it was answered, and how many times its
/// key was replaced, never its request or its answer; then what to do.
fn show(call: &Call, project: &Project, written: &str, suffix: &str) -> Reply {
    let now = now(call);
    let (databases, store, url, item) = match locate(call, project, written, suffix, "show", now)
    {
        Ok(found) => found,
        Err(reply) => return reply,
    };
    let record = match record_of(&databases, &store, &url, &item) {
        Ok(record) => record.map(|(_, _, record)| record),
        Err(reply) => return reply,
    };
    let id = runtime::short(&item.id);
    let mut screen = Screen::new();
    let mut fields = vec![("item".to_string(), item_cell(project, &item))];
    if let Some(record) = &record {
        fields.push(("action".to_string(), bare(&record.action)));
    }
    let mut state = state_cell(&item.state).text;
    if let Some(attempts) = item.attempts.filter(|n| *n > 0) {
        state.push_str(&format!(
            " · {}",
            plural(u64::try_from(attempts).unwrap_or(0), "attempt", "attempts")
        ));
    }
    fields.push(("state".to_string(), state));
    if let Some(at) = item.last_tried {
        let label = if item.kind == Kind::Request {
            "last run"
        } else {
            "tried"
        };
        fields.push((
            label.to_string(),
            match item.code.as_deref() {
                Some(code) if item.kind != Kind::Request => {
                    format!("{} · {code}", clock(at, now))
                }
                _ => clock(at, now),
            },
        ));
    }
    if let Some(record) = &record {
        fields.push(("started".to_string(), clock(record.started_at, now)));
        if let Some(problem) = record.problem_text() {
            fields.push(("differs".to_string(), problem));
        }
    }
    if let Some(acted) = &item.acted {
        fields.push((
            "logged".to_string(),
            format!(
                "{} · {} · {}",
                acted.action.replace('_', " "),
                acted.operator,
                clock(acted.at, now)
            ),
        ));
    }
    screen.push(Block::Fields(fields));
    if let Some(record) = &record {
        let mut table = Table::new(["step", "operation", "state", "sent (UTC)"]).cut(1);
        for step in &record.steps {
            let state = match step.outcome.as_deref() {
                Some("answered") if step.kept => Cell::state(Symbol::Success, "answered"),
                Some("failed") if step.kept => Cell::state(Symbol::Failure, "failed"),
                Some(_) => Cell::state(Symbol::Skipped, "answer dropped"),
                None => Cell::state(Symbol::Decision, "no answer"),
            };
            let mut sent = clock(step.sent_at, now);
            if step.key_generation > 0 {
                sent.push_str(&format!(" · key {}", step.key_generation + 1));
            }
            table = table.row(vec![
                Cell::text(step.position.to_string()),
                Cell::text(step.operation.clone()),
                state,
                Cell::muted(sent),
            ]);
        }
        screen.push(Block::Table(table));
    }
    let answered = record.as_ref().map_or(0, |record| {
        record
            .steps
            .iter()
            .filter(|step| step.outcome.as_deref() == Some("answered"))
            .count() as u64
    });
    let mut facts = Vec::new();
    if let Some(record) = &record {
        facts.push(Fact::plain(plural(
            record.steps.len() as u64,
            "step recorded",
            "steps recorded",
        )));
        facts.push(Fact::plain(format!("{answered} answered")));
    }
    let doubt = record.as_ref().and_then(Record::in_doubt);
    let dropped = record.as_ref().is_some_and(Record::dropped);
    match (item.state.as_str(), &record) {
        // Something only a person can find out: whether a step took effect.
        (OUTCOME_UNKNOWN | UNFINISHED, Some(record)) if !dropped => {
            let subject = match doubt {
                Some(step) => format!(
                    "Check whether step {}, `{}`, took effect at the provider",
                    step.position, step.operation
                ),
                None => format!(
                    "Check what `{}`'s {} did at the provider",
                    bare(&record.action),
                    plural(record.steps.len() as u64, "step", "steps")
                ),
            };
            let next = if item.kind == Kind::Request {
                format!(
                    "Its client can still retry under the same key; otherwise run `jadpo deliveries abandon {id}` and tidy up what took effect"
                )
            } else {
                format!(
                    "If it didn't, run `jadpo deliveries retry {id} --not-done`; if it did, run `jadpo deliveries abandon {id}` and tidy up what took effect"
                )
            };
            screen.push(Block::Decision {
                subject,
                location: None,
                next,
            });
        }
        (state, _) => {
            let failing = [FAILED, OUTCOME_UNKNOWN, DIVERGED, UNFINISHED].contains(&state);
            screen.push(
                Block::summary(
                    if failing {
                        Outcome::Failure
                    } else {
                        Outcome::Success
                    },
                    state_cell(state).text,
                    facts,
                )
                .with_plain_headline("delivery"),
            );
            let hint = match (state, &record) {
                (_, Some(_)) if dropped && state != ABANDONED => Some((
                    format!("jadpo deliveries abandon {id}"),
                    "its answers were dropped after 30 days, so it can't be retried",
                )),
                (DIVERGED, Some(_)) if item.kind == Kind::Request => Some((
                    format!("jadpo deliveries abandon {id}"),
                    "unless its client retries under the same key once a fix is deployed",
                )),
                (DIVERGED, Some(_)) => Some((
                    format!("jadpo deliveries retry {id}"),
                    "once a fix that sends what the record shows is deployed",
                )),
                (FAILED, Some(_)) if doubt.is_some() => Some((
                    format!("jadpo deliveries retry {id} --not-done"),
                    "sends the step that failed again under a new key, once its cause is fixed",
                )),
                (FAILED, _) => Some((
                    format!("jadpo deliveries retry {id}"),
                    "sends it again, once the cause is fixed",
                )),
                (OUTCOME_UNKNOWN, None) => Some((
                    format!("jadpo deliveries resolve {id} --done"),
                    "once the provider shows it happened",
                )),
                (PENDING, _) if item.kind == Kind::Delivery => Some((
                    format!("jadpo deliveries cancel {id}"),
                    "stops it running",
                )),
                _ => None,
            };
            if let Some((command, reason)) = hint {
                screen.push(Block::hint(command, reason));
            }
        }
    }
    let data = Json::object([
        ("item", item_json(project, &store, &item)),
        (
            "record",
            record.as_ref().map_or(Json::Null, |record| record_json(record)),
        ),
    ]);
    Reply::screen(screen, data)
}

/// A record as `--json` gives it: never a request, an answer or a key.
fn record_json(record: &Record) -> Json {
    Json::object([
        ("action", Json::from(record.action.as_str())),
        ("build", Json::from(record.build.as_str())),
        ("state", Json::from(record.state.as_str())),
        ("startedAt", instant(Some(record.started_at))),
        ("lastRunAt", instant(Some(record.taken_at))),
        (
            "problem",
            record
                .problem_text()
                .map_or(Json::Null, |text| Json::from(text.as_str())),
        ),
        (
            "steps",
            Json::array(record.steps.iter().map(|step| {
                Json::object([
                    ("step", Json::from(i64::from(step.position))),
                    ("operation", Json::from(step.operation.as_str())),
                    (
                        "outcome",
                        step.outcome.as_deref().map_or(Json::Null, Json::from),
                    ),
                    ("answerKept", Json::from(step.kept)),
                    ("sentAt", instant(Some(step.sent_at))),
                    ("answeredAt", instant(step.answered_at)),
                    ("keyGeneration", Json::from(i64::from(step.key_generation))),
                ])
            })),
        ),
    ])
}

/// `` `CensusTaken` to `ring_bell` ``.
fn delivery_words(item: &Item) -> String {
    format!(
        "`{}` to `{}`",
        item.event.as_deref().map(bare).unwrap_or_default(),
        bare(&item.target)
    )
}

fn action_command(action: Action) -> &'static str {
    match action {
        Action::Retry => "retry",
        Action::Dismiss => "dismiss",
        Action::Cancel => "cancel",
        Action::ResolveDone | Action::ResolveNotDone => "resolve",
        Action::RetryNotDone => "retry",
        Action::Abandon => "abandon",
    }
}

/// `deliveries.wrong_state`: `action` doesn't apply to `item` as it is,
/// with what does.
fn wrong_state(project: &Project, item: &Item, action: Action, id: &str) -> Diagnostic {
    let what = match item.kind {
        Kind::Delivery => format!("Its delivery of {}", delivery_words(item)),
        Kind::ScheduleRun => format!("This run of {}", subject(project, item)),
        Kind::Request => format!("The steps of `{}`", bare(&item.target)),
    };
    let (message, applies) = match (item.kind, action) {
        (Kind::Request, _) => (
            format!(
                "`{id}` is a route's steps, which only its client can {}",
                if matches!(action, Action::Retry | Action::RetryNotDone) {
                    "retry"
                } else {
                    "settle"
                }
            ),
            "A route's steps resume when its client retries under the same `Idempotency-Key`; an operator can only abandon them.",
        ),
        (_, Action::Retry) => (
            format!("`{id}` hasn't failed, so there is nothing to retry"),
            "Retry is for a delivery that failed or diverged, or a schedule's latest run that failed.",
        ),
        (_, Action::Dismiss) => (
            format!("`{id}` hasn't failed, so there is nothing to dismiss"),
            "Dismiss sets aside a delivery or a schedule's latest run that failed.",
        ),
        (_, Action::Cancel) => (
            format!("`{id}` isn't pending, so there is nothing to cancel"),
            "Cancel stops a delivery, or a run asked for by hand, that is waiting to run.",
        ),
        (_, Action::ResolveDone | Action::ResolveNotDone) => (
            format!("`{id}` isn't in doubt, so there is nothing to resolve"),
            "Resolve is for one whose outside call may or may not have happened.",
        ),
        (_, Action::RetryNotDone) => (
            format!("`{id}` isn't in doubt and hasn't failed, so there is nothing to send again"),
            "Not done is for a delivery whose outcome is unknown, or that failed.",
        ),
        (_, Action::Abandon) => (
            format!("`{id}` isn't unfinished, so there is nothing to abandon"),
            "Abandon ends an action with several changing calls whose delivery failed, diverged or is in doubt, or a route's unfinished steps.",
        ),
    };
    let next = match item.state.as_str() {
        UNFINISHED | DIVERGED if item.kind == Kind::Request => {
            format!("Run `jadpo deliveries abandon {id}`, unless its client retries")
        }
        FAILED if item.latest => {
            format!("Run `jadpo deliveries retry {id}`, or `jadpo deliveries dismiss {id}`")
        }
        OUTCOME_UNKNOWN => format!(
            "Check with the provider, then run `jadpo deliveries resolve {id} --done` or `--not-done`"
        ),
        DIVERGED => format!(
            "Deploy a fix, then run `jadpo deliveries retry {id}`, or `jadpo deliveries abandon {id}`"
        ),
        ABANDONED => format!("Run `jadpo deliveries show {id}` for what took effect"),
        PENDING => format!("Wait for it to run, or run `jadpo deliveries cancel {id}`"),
        RUNNING => "Wait for it to end, then run `jadpo deliveries` again".to_string(),
        _ => "Run `jadpo deliveries` to see what needs an operator".to_string(),
    };
    let mut reason = format!("{what} {}. {applies}", state_words(&item.state));
    if item.state == OUTCOME_UNKNOWN && action == Action::Retry {
        reason.push_str(" Retrying it blindly could repeat the outside call.");
    }
    Diagnostic::new(&rule::DELIVERIES_WRONG_STATE, message)
        .reason(reason)
        .next(next)
}
