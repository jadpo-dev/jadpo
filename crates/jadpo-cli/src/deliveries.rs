//! `jadpo deliveries` (area 12, "What an operator sees"; area 14,
//! question 13; chunk 6.12): the deliveries of events to subscribers, and
//! schedules' runs, that need an operator, and what one does to each,
//! against the databases they are in.
//!
//! - `jadpo deliveries` (or `list`) lists those that failed or whose
//!   outcome is unknown, newest first, 20 a page; `--state` lists those
//!   pending, running, dismissed or cancelled instead.
//! - `retry <id>` delivers a failed delivery again on the app's next
//!   tick, with a fresh retry budget, or asks the app's tick to run a
//!   schedule whose latest run failed, waiting for it as `jadpo schedules
//!   run` does; `dismiss <id>` sets a failed one aside; `cancel <id>`
//!   stops a pending one; `resolve <id> --done|--not-done` settles one
//!   whose outcome is unknown, "not done" retrying it.
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
use jadpo_runtime::deliveries::{
    self as runtime, Action, CANCELLED, DISMISSED, Done, FAILED, Holds, Item, Kind,
    NEEDS_AN_OPERATOR, OUTCOME_UNKNOWN, PENDING, RUNNING,
};
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
        name: "pending",
        description: "Waiting to run, or to retry",
    },
    crate::table::Value {
        name: "running",
        description: "A worker has it",
    },
    crate::table::Value {
        name: "dismissed",
        description: "Set aside by an operator",
    },
    crate::table::Value {
        name: "cancelled",
        description: "Stopped by an operator before it ran",
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
            if let Some(reply) = only_for(call, "--done", "resolve")
                .or_else(|| only_for(call, "--not-done", "resolve"))
            {
                return reply;
            }
            match Project::load(call) {
                Ok(project) => list(call, &project),
                Err(reply) => reply,
            }
        }
        Some(action @ ("retry" | "dismiss" | "cancel" | "resolve")) => {
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
            let action = match action {
                "retry" => Action::Retry,
                "dismiss" => Action::Dismiss,
                "cancel" => Action::Cancel,
                _ => match (call.flag("--done"), call.flag("--not-done")) {
                    (true, false) => Action::ResolveDone,
                    (false, true) => Action::ResolveNotDone,
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
            if action == Action::Retry || action == Action::Dismiss || action == Action::Cancel {
                let name = if action == Action::Retry {
                    "retry"
                } else if action == Action::Dismiss {
                    "dismiss"
                } else {
                    "cancel"
                };
                if let Some(reply) = only_for(call, "--done", name)
                    .or_else(|| only_for(call, "--not-done", name))
                {
                    return reply;
                }
            }
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
            match Project::load(call) {
                Ok(project) => act(call, &project, &written, &suffix, action),
                Err(reply) => reply,
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
        Reply::failed(arguments(
            format!("`{flag}` is for `jadpo deliveries resolve`, not `{action}`"),
            "Only a delivery or schedule run whose outcome is unknown is resolved as done or not done.".to_string(),
            format!("Leave out `{flag}`"),
        ))
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
    if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_hexdigit()) || digits.len() > 32
    {
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
        let stores = schema
            .stores
            .iter()
            .map(|(name, store)| {
                (
                    name.clone(),
                    Holds {
                        deliveries: store.tables.contains_key(deliveries),
                        runs: store.tables.contains_key(runs),
                    },
                )
            })
            .filter(|(_, holds)| holds.deliveries || holds.runs)
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
                            format!("`{store}` isn't a store that keeps deliveries or schedule runs"),
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
    }
}

fn state_cell(state: &str) -> Cell {
    match state {
        FAILED => Cell::state(Symbol::Failure, "failed"),
        OUTCOME_UNKNOWN => Cell::state(Symbol::Decision, "outcome unknown"),
        PENDING => Cell::state(Symbol::Pending, "pending"),
        RUNNING => Cell::state(Symbol::Running, "running"),
        DISMISSED => Cell::state(Symbol::Skipped, "dismissed"),
        CANCELLED => Cell::state(Symbol::Skipped, "cancelled"),
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
        DISMISSED => "was dismissed",
        CANCELLED => "was cancelled",
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
        Some(DISMISSED) => "dismissed (UTC)",
        Some(CANCELLED) => "cancelled (UTC)",
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
        DISMISSED | CANCELLED => match &item.acted {
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
                Kind::ScheduleRun => Json::Null,
            },
        ),
        (
            "schedule",
            match item.kind {
                Kind::ScheduleRun => Json::from(project.schedule_name(&item.target).as_str()),
                Kind::Delivery => Json::Null,
            },
        ),
        ("event", item.event.as_deref().map_or(Json::Null, Json::from)),
        ("state", Json::from(item.state.as_str())),
        (
            "attempts",
            item.attempts.map_or(Json::Null, |n| Json::from(i64::from(n))),
        ),
        ("lastTriedAt", instant(item.last_tried)),
        ("nextAttemptAt", instant(item.next_attempt)),
        ("code", item.code.as_deref().map_or(Json::Null, Json::from)),
        (
            "failureKind",
            item.failure_kind.as_deref().map_or(Json::Null, Json::from),
        ),
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
    for (store, holds, url) in &databases {
        let states = states.clone();
        let read = with_database(store, url, |database| {
            let holds = *holds;
            Box::pin(async move {
                let states: Vec<&str> = states.iter().map(String::as_str).collect();
                let items = runtime::items(database, holds, &states, limit).await?;
                let counts = runtime::counts(database, holds).await?;
                Ok((items, counts))
            })
        });
        let (found, counted) = match read {
            Ok(read) => read,
            Err(reply) => return reply,
        };
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
        (
            "state",
            state.as_deref().map_or(Json::Null, Json::from),
        ),
        ("page", Json::from(i64::try_from(page).unwrap_or(i64::MAX))),
        ("pages", Json::from(i64::try_from(pages).unwrap_or(i64::MAX))),
        ("total", Json::from(i64::try_from(total).unwrap_or(i64::MAX))),
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
    Reply::screen(
        list_screen(project, state.as_deref(), &shown, &counts, total, page, pages, now),
        data,
    )
}

#[allow(clippy::too_many_arguments)]
fn list_screen(
    project: &Project,
    state: Option<&str>,
    shown: &[Item],
    counts: &BTreeMap<String, u64>,
    total: u64,
    page: u64,
    pages: u64,
    now: i64,
) -> Screen {
    let count = |state: &str| counts.get(state).copied().unwrap_or(0);
    let mut screen = Screen::new();
    if shown.is_empty() {
        let (headline, facts) = match state {
            None => (
                "Nothing needs an operator".to_string(),
                [(PENDING, "pending"), (RUNNING, "running")]
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
    let mut facts = Vec::new();
    let (outcome, headline) = match state {
        None => {
            let failed = count(FAILED);
            let unknown = count(OUTCOME_UNKNOWN);
            if failed > 0 {
                if unknown > 0 {
                    facts.push(Fact {
                        text: format!("{unknown} outcome unknown"),
                        tone: Some(Token::Decision),
                    });
                }
                (Outcome::Failure, format!("{failed} failed"))
            } else {
                (Outcome::Failure, format!("{unknown} outcome unknown"))
            }
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
                    "delivers it again, once the cause is fixed",
                ),
                Kind::ScheduleRun => (
                    format!("jadpo deliveries retry {id}"),
                    "runs it again, once the cause is fixed",
                ),
            }),
            PENDING => Some((
                format!("jadpo deliveries cancel {id}"),
                "stops it running",
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
                    format!("jadpo deliveries resolve {} --done", runtime::short(&item.id)),
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

fn act(call: &Call, project: &Project, written: &str, suffix: &str, action: Action) -> Reply {
    let databases = match project.databases(call) {
        Ok(found) => found,
        Err(reply) => return reply,
    };
    let mut found: Vec<(String, String, Item)> = Vec::new();
    for (store, holds, url) in &databases {
        let read = with_database(store, url, |database| {
            let holds = *holds;
            let suffix = suffix.to_string();
            Box::pin(async move { runtime::find(database, holds, &suffix).await })
        });
        match read {
            Ok(items) => found.extend(
                items
                    .into_iter()
                    .map(|item| (store.clone(), url.clone(), item)),
            ),
            Err(reply) => return reply,
        }
    }
    let command = action_command(action);
    let (store, url, item) = match found.as_slice() {
        [] => {
            return Reply::failed(
                Diagnostic::new(
                    &rule::DELIVERIES_NOT_FOUND,
                    format!("No delivery or schedule run has the id `{written}`"),
                )
                .reason(
                    "`jadpo deliveries` lists those that need an operator, and `--state` the others; a delivery whose event every subscriber had is deleted after 24 hours, and a schedule's run after 30 days.",
                )
                .next("Run `jadpo deliveries` to see them, with their ids"),
            );
        }
        [one] => one.clone(),
        many => {
            let ids: Vec<String> = many
                .iter()
                .map(|(_, _, item)| format!("`{}`", runtime::hex(&item.id)))
                .collect();
            return Reply::failed(arguments(
                format!("More than one item has an id ending `{written}`"),
                format!("They are {}.", crate::run::both(&ids)),
                format!("Use more of the id, such as `jadpo deliveries {command} {}`", runtime::hex(&many[0].2.id)),
            ));
        }
    };
    let id = runtime::short(&item.id);
    if !action.applies_to(&item) {
        return Reply::failed(wrong_state(project, &item, action, &id));
    }
    let operator = operator(call);
    let now = now(call);
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
    let done = with_database(&store, &url, |database| {
        let item = item.clone();
        let operator = operator.clone();
        Box::pin(async move {
            let host = jadpo_host_native::NativeHost::new();
            match (item.kind, action) {
                (Kind::Delivery, _) => {
                    runtime::act_on_delivery(database, &host, &item, action, &operator, now).await
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
    let text = match (item.kind, action) {
        (Kind::Delivery, Action::Retry) => format!(
            "Queued `{id}` for retry: the app's next tick delivers {} again, with a fresh retry budget",
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
        (Kind::Delivery, Action::ResolveNotDone) => format!(
            "Resolved `{id}` as not done: the app's next tick delivers {} again",
            delivery_words(&item)
        ),
        (Kind::ScheduleRun, Action::Cancel) => format!(
            "Cancelled `{id}`: the run asked of {subject} won't happen"
        ),
        (Kind::ScheduleRun, Action::ResolveDone) => format!(
            "Resolved `{id}` as done: {subject}'s run took effect"
        ),
        (Kind::ScheduleRun, _) => {
            let from = item
                .last_tried
                .map(|at| format!(" from {}", clock(at, now)))
                .unwrap_or_default();
            format!("Dismissed `{id}`: {subject}'s failed run{from}")
        }
    };
    let screen = Screen::new().with(Block::Status {
        outcome: Outcome::Success,
        text,
        rows: vec![("logged".to_string(), format!("{operator} · role {role}"))],
    });
    Reply::screen(
        screen,
        Json::object([
            ("id", Json::from(id.as_str())),
            ("uuid", Json::from(uuid_text(&item.id).as_str())),
            ("kind", Json::from(item.kind.word())),
            ("store", Json::from(store.as_str())),
            ("action", Json::from(action.word())),
            ("was", Json::from(item.state.as_str())),
            ("operator", Json::from(operator.as_str())),
            ("role", Json::from(role.as_str())),
            ("at", instant(Some(now))),
        ]),
    )
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
    }
}

/// `deliveries.wrong_state`: `action` doesn't apply to `item` as it is,
/// with what does.
fn wrong_state(project: &Project, item: &Item, action: Action, id: &str) -> Diagnostic {
    let what = match item.kind {
        Kind::Delivery => format!("Its delivery of {}", delivery_words(item)),
        Kind::ScheduleRun => format!("This run of {}", subject(project, item)),
    };
    let (message, applies) = match action {
        Action::Retry => (
            format!("`{id}` hasn't failed, so there is nothing to retry"),
            "Retry is for a delivery or a schedule's latest run that failed.",
        ),
        Action::Dismiss => (
            format!("`{id}` hasn't failed, so there is nothing to dismiss"),
            "Dismiss sets aside a delivery or a schedule's latest run that failed.",
        ),
        Action::Cancel => (
            format!("`{id}` isn't pending, so there is nothing to cancel"),
            "Cancel stops a delivery, or a run asked for by hand, that is waiting to run.",
        ),
        Action::ResolveDone | Action::ResolveNotDone => (
            format!("`{id}` isn't in doubt, so there is nothing to resolve"),
            "Resolve is for one whose outside call may or may not have happened.",
        ),
    };
    let next = match item.state.as_str() {
        FAILED if item.latest => format!(
            "Run `jadpo deliveries retry {id}`, or `jadpo deliveries dismiss {id}`"
        ),
        OUTCOME_UNKNOWN => format!(
            "Check with the provider, then run `jadpo deliveries resolve {id} --done` or `--not-done`"
        ),
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
