//! `jadpo schedules` and `jadpo why <schedule>` (area 12, "Watching
//! schedules"; area 14; area 17's `jadpo why`; chunk 6.2b): read from each
//! schedule's state row and run records in the database its state is kept
//! in, and act on it there.
//!
//! - `jadpo schedules` lists each schedule with its last run, its next due
//!   time (or that it is paused), the alerts that hold, and a footer; it
//!   fails while a last run failed or an alert holds, so a health check can
//!   run it.
//! - `history <name>` shows its records newest first, 20 a page, grouped by
//!   day, then its metrics over the records kept.
//! - `pause` and `resume` set it on its state row. `run` runs it now in this
//!   process, under its lease, against `jadpo dev`'s own database; against
//!   any other, a deployment's, it asks the application's own tick to run
//!   it and waits for that run's record, running no application code and
//!   reading none of its settings here (chunk 6.2d).
//! - `jadpo why <name>` answers "why didn't it run?" in area 17's shape: the
//!   answer in a sentence, its facts, the `.jadpo` place last.
//!
//! The database is the one `--url` names, else the address the schedules
//! store's connection reads from its setting (area 10), else the one a
//! running `jadpo dev` started for it. Times are UTC, as schedules count
//! them.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use jadpo_check::schedules::ScheduleModel;
use jadpo_diag::{Diagnostic, rule};
use jadpo_runtime::schedules::watch::{
    self, Alert, AlertKind, Declared, Metrics, Paused, Requested, RunNow, RunRecord, Watch, Watched,
};
use jadpo_runtime::schedules::{Due, Ran, why};
use jadpo_term::{Block, Cell, Fact, Json, Outcome, Screen, Symbol, Table};

use crate::database::Address;
use crate::run::{Call, Reply, quoted};

/// What `jadpo schedules` does besides listing.
pub const ACTIONS: &[crate::table::Value] = &[
    crate::table::Value {
        name: "run",
        description: "Run a schedule now, under its lease",
    },
    crate::table::Value {
        name: "pause",
        description: "Stop a schedule's due ticks running it",
    },
    crate::table::Value {
        name: "resume",
        description: "Start a paused schedule again",
    },
    crate::table::Value {
        name: "history",
        description: "Show a schedule's past runs and metrics",
    },
];

/// How many records a page of history shows.
pub const PAGE: u32 = 20;

/// Where `jadpo dev` keeps the address of each database it started for the
/// session, so these commands find it; removed when it stops.
pub const DEV_STORES: &str = "build/dev/stores.json";

pub fn schedules(call: &Call) -> Reply {
    let action = call.arguments.first().map(String::as_str);
    match action {
        None => match Project::load(call, "schedules") {
            Ok(project) => list(call, &project),
            Err(reply) => reply,
        },
        Some(action @ ("run" | "pause" | "resume" | "history")) => {
            if let Some(extra) = call.arguments.get(2) {
                return Reply::failed(arguments(
                    format!("`jadpo schedules {action}` takes one schedule, not `{extra}` too"),
                    "Each acts on one schedule, named as its file names it.".to_string(),
                    format!("Run `jadpo schedules {action} <name>`"),
                ));
            }
            if action != "history" && call.value("--page").is_some() {
                return Reply::failed(arguments(
                    format!("`--page` is for `jadpo schedules history`, not `{action}`"),
                    "Only history comes in pages.".to_string(),
                    format!(
                        "Leave out `--page`, or run `jadpo schedules history <name> --page {}`",
                        call.value("--page").unwrap_or("2")
                    ),
                ));
            }
            let project = match Project::load(call, "schedules") {
                Ok(project) => project,
                Err(reply) => return reply,
            };
            let schedule =
                match project.named(call.arguments.get(1), &format!("schedules {action}")) {
                    Ok(schedule) => schedule,
                    Err(reply) => return reply,
                };
            match action {
                "run" => run(call, &project, &schedule),
                "pause" => pause(call, &project, &schedule, true),
                "resume" => pause(call, &project, &schedule, false),
                _ => history(call, &project, &schedule),
            }
        }
        Some(other) => {
            let names: Vec<&str> = ACTIONS.iter().map(|action| action.name).collect();
            let near = jadpo_diag::nearest(other, names.iter().copied());
            Reply::failed(arguments(
                format!("`{other}` isn't something `jadpo schedules` does"),
                match near {
                    Some(near) => format!(
                        "`jadpo schedules` lists the schedules, and {} act on one. Did you mean `{near}`?",
                        crate::run::both(&quoted(names.iter()))
                    ),
                    None => format!(
                        "`jadpo schedules` lists the schedules, and {} act on one.",
                        crate::run::both(&quoted(names.iter()))
                    ),
                },
                match near {
                    Some(near) => format!("Use `{near}` instead of `{other}`"),
                    None => {
                        "Run `jadpo schedules`, or `jadpo schedules history <name>`".to_string()
                    }
                },
            ))
        }
    }
}

pub fn why(call: &Call) -> Reply {
    let project = match Project::load(call, "why") {
        Ok(project) => project,
        Err(reply) => return reply,
    };
    if let Some(extra) = call.arguments.get(1) {
        return Reply::failed(
            Diagnostic::new(
                &rule::CLI_WHY_ARGUMENTS,
                format!("`jadpo why` explains one thing, not `{extra}` too"),
            )
            .reason("It takes the name of a schedule.")
            .next("Run `jadpo why <schedule>`"),
        );
    }
    let schedule = match project.named(call.arguments.first(), "why") {
        Ok(schedule) => schedule,
        Err(reply) => return reply,
    };
    explain(call, &project, &schedule)
}

fn arguments(message: String, reason: String, next: String) -> Diagnostic {
    Diagnostic::new(&rule::CLI_SCHEDULES_ARGUMENTS, message)
        .reason(reason)
        .next(next)
}

/// The project as these commands need it: its schedules and where their
/// state is kept.
struct Project {
    root: PathBuf,
    loaded: jadpo_db::Loaded,
    schedules: Vec<std::sync::Arc<ScheduleModel>>,
    store: Option<String>,
}

impl Project {
    fn load(call: &Call, command: &str) -> Result<Project, Reply> {
        let cwd = call.cwd.clone().unwrap_or_else(|| PathBuf::from("."));
        let Some(root) = jadpo_db::find_root(&cwd) else {
            return Err(crate::fmt::project_needed(command));
        };
        let loaded = jadpo_db::load(&root);
        let schedules = jadpo_check::schedules::schedules(&loaded.db, loaded.project)
            .iter()
            .cloned()
            .collect();
        let store = jadpo_check::schedules::schedules_settings(&loaded.db, loaded.project)
            .store
            .clone();
        Ok(Project {
            root,
            loaded,
            schedules,
            store,
        })
    }

    /// What a schedule is called on screen: its name, or `domain.name`
    /// when another domain has a schedule of the same name.
    fn shown(&self, schedule: &ScheduleModel) -> String {
        let same = self
            .schedules
            .iter()
            .filter(|other| other.name == schedule.name)
            .count();
        if same > 1 {
            qualified(schedule)
        } else {
            schedule.name.clone()
        }
    }

    fn declared(&self) -> Vec<Declared> {
        self.schedules
            .iter()
            .map(|schedule| Declared {
                key: schedule.key.as_str().to_string(),
                name: self.shown(schedule),
                every_ms: schedule.every_ms,
            })
            .collect()
    }

    /// The schedule `name` names, as its name or `domain.name`.
    fn named(
        &self,
        name: Option<&String>,
        command: &str,
    ) -> Result<std::sync::Arc<ScheduleModel>, Reply> {
        let rule = if command == "why" {
            &rule::CLI_WHY_ARGUMENTS
        } else {
            &rule::CLI_SCHEDULES_ARGUMENTS
        };
        let names: Vec<String> = self.schedules.iter().map(|s| self.shown(s)).collect();
        let Some(name) = name else {
            let (reason, next) = if names.is_empty() {
                (
                    "The project declares no schedules.".to_string(),
                    "Declare one in a domain's `schedules/` folder".to_string(),
                )
            } else {
                (
                    format!(
                        "The project's schedules are {}.",
                        crate::run::both(&quoted(&names))
                    ),
                    format!("Run `jadpo {command} {}`", names[0]),
                )
            };
            return Err(Reply::failed(
                Diagnostic::new(rule, format!("`jadpo {command}` needs a schedule's name"))
                    .reason(reason)
                    .next(next),
            ));
        };
        let found: Vec<_> = self
            .schedules
            .iter()
            .filter(|schedule| schedule.name == *name || qualified(schedule) == *name)
            .collect();
        match found.as_slice() {
            [one] => Ok(std::sync::Arc::clone(one)),
            [] => {
                let near = jadpo_diag::nearest(name, names.iter().map(String::as_str));
                let diagnostic = Diagnostic::new(rule, format!("`{name}` isn't a schedule"));
                Err(Reply::failed(match (near, names.is_empty()) {
                    (_, true) => diagnostic
                        .reason("The project declares no schedules.")
                        .next("Declare one in a domain's `schedules/` folder"),
                    (Some(near), false) => diagnostic
                        .reason(format!(
                            "The project's schedules are {}. Did you mean `{near}`?",
                            crate::run::both(&quoted(&names))
                        ))
                        .next(format!("Use `{near}` instead of `{name}`")),
                    (None, false) => diagnostic
                        .reason(format!(
                            "The project's schedules are {}.",
                            crate::run::both(&quoted(&names))
                        ))
                        .next(format!("Use {}", crate::run::either(&quoted(&names)))),
                }))
            }
            many => {
                let both: Vec<String> = many.iter().map(|s| qualified(s)).collect();
                Err(Reply::failed(
                    Diagnostic::new(rule, format!("Two domains have a schedule called `{name}`"))
                        .reason(format!("They are {}.", crate::run::both(&quoted(&both))))
                        .next(format!("Name it with its domain, such as `{}`", both[0])),
                ))
            }
        }
    }

    /// The database the schedules' state is in: `(store, url)`.
    fn database(&self, call: &Call) -> Result<(String, Address), Reply> {
        let Some(store) = self.store.clone() else {
            return Err(Reply::failed(
                Diagnostic::new(
                    &rule::SCHEDULES_UNAVAILABLE,
                    "The schedules have no database to keep their state in",
                )
                .reason("`jadpo check` says why: `schedule.store_required`.")
                .next("Run `jadpo check`, and fix what it reports"),
            ));
        };
        match url_of(call, &self.root, &self.loaded, &store) {
            Ok(Some(url)) => Ok((store, url)),
            Ok(None) => Err(Reply::failed(arguments(
                format!("`jadpo schedules` needs the address of the database `{store}`"),
                "No variable gives the address of its connection, and no `jadpo dev` is running with one: a connection reads its address from the setting its `url:` names (area 10), and `--url` names a database directly.".to_string(),
                "Run `jadpo config set <the connection's setting>`, start `jadpo dev`, or add `--url postgresql://…`".to_string(),
            ))),
            Err(reply) => Err(reply),
        }
    }
}

/// The project's schedules, by file name, for Tab completion: read from
/// the folders, so completion stays quick.
pub(crate) fn names(cwd: &Path) -> Vec<(String, &'static str)> {
    let Some(root) = jadpo_db::find_root(cwd) else {
        return Vec::new();
    };
    let mut out: Vec<(String, &'static str)> = crate::sql::names_in(&root, "schedules")
        .into_iter()
        .map(|name| (name, "Schedule"))
        .collect();
    out.sort();
    out
}

/// `home.chore_census`: a schedule's key without its kind.
fn qualified(schedule: &ScheduleModel) -> String {
    let key = schedule.key.as_str();
    key.strip_prefix("schedule ").unwrap_or(key).to_string()
}

/// The address of `store`'s database: `--url`, else its connection's
/// setting, each secured as its connection says, else the one a running
/// `jadpo dev` started for it.
pub(crate) fn url_of(
    call: &Call,
    root: &Path,
    loaded: &jadpo_db::Loaded,
    store: &str,
) -> Result<Option<Address>, Reply> {
    if let Some(url) = call.value("--url") {
        return Address::of(call, loaded, store, url.to_string()).map(Some);
    }
    if let Some((_, url)) = crate::migrate::configured_url(call, loaded, store.to_string())? {
        return Ok(Some(url));
    }
    Ok(dev_stores(root).remove(store).map(Address::by_default))
}

/// The databases a running `jadpo dev` started, by store.
pub(crate) fn dev_stores(root: &Path) -> BTreeMap<String, String> {
    let Ok(text) = std::fs::read_to_string(root.join(DEV_STORES)) else {
        return BTreeMap::new();
    };
    let Ok(serde_json::Value::Object(document)) = serde_json::from_str(&text) else {
        return BTreeMap::new();
    };
    match document.get("stores") {
        Some(serde_json::Value::Object(stores)) => stores
            .iter()
            .filter_map(|(store, url)| Some((store.clone(), url.as_str()?.to_string())))
            .collect(),
        _ => BTreeMap::new(),
    }
}

/// Now: `SOURCE_DATE_EPOCH` when it is set (the reproducible-builds
/// convention, which tests use), else the system clock.
pub(crate) fn now(call: &Call) -> i64 {
    call.environment
        .var("SOURCE_DATE_EPOCH")
        .and_then(|seconds| seconds.trim().parse::<i64>().ok())
        .map(|seconds| seconds.saturating_mul(1000))
        .unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(0))
        })
}

/// How often the host ticks: once a minute, or as often as the shortest
/// `every:` when that is shorter, at least once a second.
pub(crate) fn tick_every(declared: &[Declared]) -> u64 {
    declared
        .iter()
        .map(|schedule| schedule.every_ms)
        .min()
        .map_or(60_000, |shortest| shortest.clamp(1_000, 60_000))
}

/// Runs `work` against the database at `url` on a runtime of its own; a
/// failure to reach or read it is `schedules.unavailable`.
fn with_database<T>(
    store: &str,
    url: &Address,
    work: impl for<'d> FnOnce(
        &'d dyn jadpo_runtime::Database,
    )
        -> jadpo_runtime::LocalFuture<'d, Result<T, jadpo_runtime::StoreError>>,
) -> Result<T, Reply> {
    let failed = |error: String| {
        Reply::failed(
            Diagnostic::new(
                &rule::SCHEDULES_UNAVAILABLE,
                format!("The database `{store}` couldn't be read"),
            )
            .reason(format!(
                "Reading the schedules' state failed: {error}. Its tables come with the migration that adds the first schedule."
            ))
            .next("Check the database is running, then run `jadpo migrate apply`"),
        )
    };
    let runtime = jadpo_host_native::Runtime::new().map_err(|error| failed(error.to_string()))?;
    let database = url.open().map_err(|error| failed(store_error(&error)))?;
    runtime
        .block_on(work(&database))
        .map_err(|error| failed(store_error(&error)))
}

pub(crate) fn store_error(error: &jadpo_runtime::StoreError) -> String {
    let text = match error {
        jadpo_runtime::StoreError::Fault(text) => text.clone(),
        other => format!("{other:?}"),
    };
    text.trim_end_matches('.').to_string()
}

// --- words ------------------------------------------------------------------

const DAY: i64 = 86_400_000;

pub(crate) fn plural(count: u64, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

/// `12:03:10` on `now`'s day (UTC), `2026-10-18 23:45` on another.
pub(crate) fn clock(at: i64, now: i64) -> String {
    let text = jadpo_std::format_instant(at);
    if at.div_euclid(DAY) == now.div_euclid(DAY) {
        text.get(11..19).unwrap_or_default().to_string()
    } else {
        format!(
            "{} {}",
            text.get(..10).unwrap_or_default(),
            text.get(11..16).unwrap_or_default()
        )
    }
}

/// A due time: `12:15` (seconds only when it has some) on `now`'s day,
/// `2026-10-20 02:00` on another.
fn grid_clock(at: i64, now: i64) -> String {
    let text = jadpo_std::format_instant(at);
    let minutes = text.get(11..16).unwrap_or_default();
    let seconds = text.get(16..19).filter(|s| *s != ":00").unwrap_or("");
    if at.div_euclid(DAY) == now.div_euclid(DAY) {
        format!("{minutes}{seconds}")
    } else {
        format!("{} {minutes}{seconds}", text.get(..10).unwrap_or_default())
    }
}

/// `412 ms`, `1.2 s`, `41 s`, `13 min`, `2 h 5 min`.
pub(crate) fn duration(ms: i64) -> String {
    let ms = ms.max(0);
    if ms < 1_000 {
        format!("{ms} ms")
    } else if ms < 10_000 {
        format!("{}.{} s", ms / 1_000, (ms % 1_000) / 100)
    } else if ms < 60_000 {
        format!("{} s", ms / 1_000)
    } else if ms < 3_600_000 {
        format!("{} min", ms / 60_000)
    } else {
        let minutes = (ms % 3_600_000) / 60_000;
        if minutes == 0 {
            format!("{} h", ms / 3_600_000)
        } else {
            format!("{} h {minutes} min", ms / 3_600_000)
        }
    }
}

/// A period, as a whole number of its unit when it is one: `2 s`, `2 min`.
fn span(ms: u64) -> String {
    let ms = i64::try_from(ms).unwrap_or(i64::MAX);
    if ms % 1_000 == 0 && ms < 60_000 {
        format!("{} s", ms / 1_000)
    } else {
        duration(ms)
    }
}

/// `8 s ago`, `3 min ago`, `2 h ago`, or `2026-10-18 23:45` a day or
/// more before.
fn ago(at: i64, now: i64) -> String {
    let past = now.saturating_sub(at).max(0);
    if past < 60_000 {
        format!("{} s ago", past / 1_000)
    } else if past < 3_600_000 {
        format!("{} min ago", past / 60_000)
    } else if past < DAY {
        format!("{} h ago", past / 3_600_000)
    } else {
        clock(at, now)
    }
}

/// `every:` as the source writes it: `15m`, `1d`, `30s`.
fn every(ms: u64) -> String {
    let seconds = ms / 1_000;
    for (unit, size) in [("d", 86_400), ("h", 3_600), ("m", 60)] {
        if seconds >= size && seconds.is_multiple_of(size) {
            return format!("{}{unit}", seconds / size);
        }
    }
    format!("{seconds}s")
}

fn events(count: Option<i32>) -> String {
    plural(
        u64::try_from(count.unwrap_or(0)).unwrap_or(0),
        "event",
        "events",
    )
}

/// What a run that succeeded did: the records a purge deleted, or the
/// events an action's run emitted.
fn did(record: &RunRecord) -> String {
    match record.items {
        Some(items) => purged(u64::try_from(items).unwrap_or(0)),
        None => events(record.events),
    }
}

fn count_json(count: u64) -> Json {
    Json::Int(i64::try_from(count).unwrap_or(i64::MAX))
}

/// `2 records purged`: what a run of an entity's purge did (chunk 6.2c).
fn purged(items: u64) -> String {
    plural(items, "record purged", "records purged")
}

/// A failure as the screens write it: `provider_unavailable · unavailable`.
fn failure(code: Option<&str>, kind: Option<&str>) -> String {
    let code = code.unwrap_or("internal_fault");
    match kind {
        Some(kind) => format!("{code} · {kind}"),
        None => code.to_string(),
    }
}

fn skip_reason(reason: Option<&str>) -> &'static str {
    match reason {
        Some("paused") => "paused",
        Some("lease_held") => "lease held",
        _ => "skipped",
    }
}

/// An alert as a line says it.
fn alert_text(alert: &Alert, now: i64) -> String {
    let name = alert.schedule.as_deref().unwrap_or("");
    match &alert.kind {
        AlertKind::TicksStopped { since } => format!(
            "No tick has arrived since {}: the app isn't running",
            clock(*since, now)
        ),
        AlertKind::LeaseExpired { started, until } => match started {
            Some(started) => format!(
                "`{name}`'s run from {} held its lease past {}",
                clock(*started, now),
                clock(*until, now)
            ),
            None => format!("`{name}`'s lease was held past {}", clock(*until, now)),
        },
        AlertKind::LongRun {
            started,
            ms,
            running,
        } => {
            if *running {
                format!(
                    "`{name}`'s run from {} has taken {}, past 80% of the 15-minute limit",
                    clock(*started, now),
                    duration(*ms)
                )
            } else {
                format!(
                    "`{name}`'s last run took {}, past 80% of the 15-minute limit",
                    duration(*ms)
                )
            }
        }
        AlertKind::FailedInARow { count } => {
            format!("`{name}` has failed {count} times in a row")
        }
        AlertKind::NoSuccess {
            since,
            ever,
            every_ms,
        } => {
            let interval = every(u64::try_from(*every_ms).unwrap_or(0));
            if *ever {
                format!(
                    "`{name}` hasn't succeeded since {}, over twice its `{interval}`",
                    clock(*since, now)
                )
            } else {
                format!(
                    "`{name}` hasn't succeeded since it was first due at {}",
                    grid_clock(*since, now)
                )
            }
        }
        AlertKind::ConnectionUnavailable {
            connection,
            since,
            held,
        } => format!(
            "`{connection}` has been unavailable since {}: {} held",
            clock(*since, now),
            plural(*held, "delivery", "deliveries")
        ),
        AlertKind::UnfinishedSteps { count, oldest, .. } => unfinished_text(*count, *oldest, now),
    }
}

/// The alert on unfinished multi-step runs, as a line says it (chunk
/// 7.5f): `3 multi-step runs are unfinished, the oldest since
/// 2026-10-19 08:12`.
pub(crate) fn unfinished_text(count: u64, oldest: i64, now: i64) -> String {
    if count == 1 {
        format!(
            "1 multi-step run has been unfinished since {}",
            clock(oldest, now)
        )
    } else {
        format!(
            "{count} multi-step runs are unfinished, the oldest since {}",
            clock(oldest, now)
        )
    }
}

fn alerts_json(alerts: &[Alert]) -> Json {
    Json::array(alerts.iter().map(|alert| json_of(&alert.event())))
}

/// A `serde_json` value as the envelope's JSON.
fn json_of(value: &serde_json::Value) -> Json {
    match value {
        serde_json::Value::Null => Json::Null,
        serde_json::Value::Bool(b) => Json::from(*b),
        serde_json::Value::Number(n) => match n.as_i64() {
            Some(i) => Json::from(i),
            None => Json::from(n.to_string().as_str()),
        },
        serde_json::Value::String(s) => Json::from(s.as_str()),
        serde_json::Value::Array(items) => Json::array(items.iter().map(json_of)),
        serde_json::Value::Object(map) => Json::object(
            map.iter()
                .map(|(key, value)| (key.clone(), json_of(value)))
                .collect::<Vec<(String, Json)>>(),
        ),
    }
}

fn instant(at: Option<i64>) -> Json {
    at.map_or(Json::Null, |at| {
        Json::from(jadpo_std::format_instant(at).as_str())
    })
}

fn run_json(run: &RunRecord) -> Json {
    Json::object([
        ("outcome", Json::from(run.outcome.as_str())),
        (
            "reason",
            run.reason.as_deref().map_or(Json::Null, Json::from),
        ),
        ("dueSlot", instant(Some(run.due))),
        ("startedAt", instant(Some(run.started))),
        ("endedAt", instant(run.ended)),
        ("code", run.code.as_deref().map_or(Json::Null, Json::from)),
        ("kind", run.kind.as_deref().map_or(Json::Null, Json::from)),
        (
            "events",
            run.events.map_or(Json::Null, |n| Json::from(i64::from(n))),
        ),
        ("items", run.items.map_or(Json::Null, Json::from)),
        (
            "traceparent",
            run.trace.as_deref().map_or(Json::Null, Json::from),
        ),
    ])
}

// --- the list ---------------------------------------------------------------

fn list(call: &Call, project: &Project) -> Reply {
    if project.schedules.is_empty() {
        return Reply::screen(
            Screen::new().with(Block::Answer {
                headline: "The project declares no schedules".to_string(),
                fields: Vec::new(),
            }),
            Json::object([
                ("schedules", Json::array(Vec::<Json>::new())),
                ("alerts", Json::array(Vec::<Json>::new())),
            ]),
        );
    }
    let (store, url) = match project.database(call) {
        Ok(found) => found,
        Err(reply) => return reply,
    };
    let declared = project.declared();
    let now = now(call);
    let period = tick_every(&declared);
    let watched = match with_database(&store, &url, |database| {
        let declared = declared.clone();
        Box::pin(async move { watch::watch(database, &declared, period, now).await })
    }) {
        Ok(watched) => watched,
        Err(reply) => return reply,
    };
    let screen = list_screen(&watched, period, now);
    let data = Json::object([
        ("store", Json::from(store.as_str())),
        ("lastTickAt", instant(watched.last_tick)),
        (
            "schedules",
            Json::array(watched.schedules.iter().map(|schedule| {
                let model = project
                    .schedules
                    .iter()
                    .find(|model| model.key.as_str() == schedule.declared.key);
                schedule_json(schedule, model.map(|m| m.as_ref()))
            })),
        ),
        ("alerts", alerts_json(&watched.alerts)),
    ]);
    Reply::screen(screen, data)
}

fn failed_count(watched: &Watch) -> usize {
    watched
        .schedules
        .iter()
        .filter(|schedule| {
            schedule
                .last_run
                .as_ref()
                .is_some_and(|run| run.outcome == "failed")
        })
        .count()
}

fn schedule_json(schedule: &Watched, model: Option<&ScheduleModel>) -> Json {
    let state = schedule.state.as_ref();
    Json::object([
        ("name", Json::from(schedule.declared.name.as_str())),
        ("key", Json::from(schedule.declared.key.as_str())),
        (
            "everyMs",
            Json::from(i64::try_from(schedule.declared.every_ms).unwrap_or(i64::MAX)),
        ),
        (
            "file",
            model.map_or(Json::Null, |m| Json::from(m.path.as_str())),
        ),
        (
            "line",
            model.map_or(Json::Null, |m| Json::from(i64::from(m.line))),
        ),
        ("paused", Json::from(state.is_some_and(|s| s.paused))),
        ("nextRunAt", instant(state.map(|s| s.next_run_at))),
        ("leaseUntil", instant(state.and_then(|s| s.lease_until))),
        ("lastTickAt", instant(state.and_then(|s| s.last_tick_at))),
        (
            "lastSucceededAt",
            instant(state.and_then(|s| s.last_succeeded_at)),
        ),
        (
            "lastRun",
            schedule.last_run.as_ref().map_or(Json::Null, run_json),
        ),
        (
            "failuresInARow",
            Json::from(i64::from(schedule.failures_in_a_row)),
        ),
        (
            "runRequested",
            Json::from(state.is_some_and(|s| s.requested.is_some())),
        ),
    ])
}

/// The last run's cell: its state, then when it started, how long it took
/// and what it emitted or why it failed.
fn last_run_cell(schedule: &Watched, now: i64) -> Cell {
    let Some(run) = &schedule.last_run else {
        return Cell::state(Symbol::Skipped, "none yet");
    };
    let by_hand = if run.by_hand_run() { " · by hand" } else { "" };
    match run.outcome.as_str() {
        "running" => Cell::state(Symbol::Running, "running").then(format!(
            "{} · {} so far{by_hand}",
            clock(run.started, now),
            duration(run.duration_ms(now))
        )),
        "succeeded" => Cell::state(Symbol::Success, "succeeded").then(format!(
            "{} · {} · {}{by_hand}",
            clock(run.started, now),
            duration(run.duration_ms(now)),
            did(run)
        )),
        _ => {
            let streak = if schedule.failures_in_a_row > 1 {
                format!(" · {} in a row", schedule.failures_in_a_row)
            } else {
                String::new()
            };
            Cell::state(Symbol::Failure, "failed").then(format!(
                "{} · {}{streak}",
                clock(run.started, now),
                run.code.as_deref().unwrap_or("internal_fault")
            ))
        }
    }
}

/// The next cell: when it is next due, that it is paused, that it is late
/// because no tick has taken it, or nothing before a tick has seen it.
fn next_cell(schedule: &Watched, period: u64, now: i64) -> Cell {
    let Some(state) = &schedule.state else {
        return Cell::state(Symbol::Skipped, "");
    };
    if state.paused {
        return Cell::state(Symbol::Paused, "paused");
    }
    let missed = now.saturating_sub(state.next_run_at)
        > i64::try_from(period).unwrap_or(i64::MAX).saturating_mul(2);
    if missed && !state.leased(now) {
        return Cell::state(
            Symbol::Warning,
            format!("not run since {}", grid_clock(state.next_run_at, now)),
        );
    }
    Cell::text(grid_clock(state.next_run_at, now))
}

fn list_screen(watched: &Watch, period: u64, now: i64) -> Screen {
    let mut table = Table::new(["schedule", "last run (UTC)", "next"]).cut(1);
    for schedule in &watched.schedules {
        table = table.row(vec![
            Cell::text(schedule.declared.name.as_str()),
            last_run_cell(schedule, now),
            next_cell(schedule, period, now),
        ]);
    }
    let mut screen = Screen::new().with(Block::Table(table));
    if !watched.alerts.is_empty() {
        screen.push(Block::Alerts(
            watched
                .alerts
                .iter()
                .map(|alert| alert_text(alert, now))
                .collect(),
        ));
    }
    let failed = failed_count(watched);
    let total = plural(watched.schedules.len() as u64, "schedule", "schedules");
    let running = watched
        .schedules
        .iter()
        .filter(|s| s.last_run.as_ref().is_some_and(RunRecord::is_running))
        .count();
    let paused = watched
        .schedules
        .iter()
        .filter(|s| s.state.as_ref().is_some_and(|state| state.paused))
        .count();
    let mut facts = Vec::new();
    let (outcome, headline) = if failed > 0 {
        facts.push(Fact::plain(total));
        (Outcome::Failure, format!("{failed} failed"))
    } else if !watched.alerts.is_empty() {
        facts.push(Fact::plain(total));
        (
            Outcome::Failure,
            plural(watched.alerts.len() as u64, "alert", "alerts"),
        )
    } else {
        (Outcome::Success, total)
    };
    if failed > 0 && !watched.alerts.is_empty() {
        facts.push(Fact::problem(plural(
            watched.alerts.len() as u64,
            "alert",
            "alerts",
        )));
    }
    if running > 0 {
        facts.push(Fact::plain(format!("{running} running")));
    }
    if paused > 0 {
        facts.push(Fact::plain(format!("{paused} paused")));
    }
    facts.push(Fact::plain(match watched.last_tick {
        Some(at) => format!("last tick {}", ago(at, now)),
        None => "no tick yet".to_string(),
    }));
    screen.push(
        Block::summary(outcome, headline, facts).with_plain_headline(
            if outcome == Outcome::Failure {
                "schedules need attention"
            } else {
                "schedules"
            },
        ),
    );
    // The one next step: why the first schedule with a problem isn't
    // running as it should.
    let troubled = watched.schedules.iter().find(|schedule| {
        schedule
            .last_run
            .as_ref()
            .is_some_and(|run| run.outcome == "failed")
            || watched
                .alerts
                .iter()
                .any(|alert| alert.schedule.as_deref() == Some(schedule.declared.name.as_str()))
    });
    if let Some(schedule) = troubled {
        screen.push(Block::hint(
            format!("jadpo why {}", schedule.declared.name),
            "explains what went wrong",
        ));
    } else if watched
        .alerts
        .iter()
        .any(|alert| matches!(alert.kind, AlertKind::TicksStopped { .. }))
        && let Some(first) = watched.schedules.first()
    {
        screen.push(Block::hint(
            format!("jadpo why {}", first.declared.name),
            "explains why it hasn't run",
        ));
    }
    screen
}

// --- history ----------------------------------------------------------------

fn history(call: &Call, project: &Project, schedule: &ScheduleModel) -> Reply {
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
    let (store, url) = match project.database(call) {
        Ok(found) => found,
        Err(reply) => return reply,
    };
    let name = project.shown(schedule);
    let key = schedule.key.as_str().to_string();
    let declared = vec![Declared {
        key: key.clone(),
        name: name.clone(),
        every_ms: schedule.every_ms,
    }];
    let now = now(call);
    let period = tick_every(&declared);
    let read = with_database(&store, &url, |database| {
        let key = key.clone();
        let declared = declared.clone();
        Box::pin(async move {
            let metrics = watch::metrics(database, &key).await?;
            let records =
                watch::history(database, &key, PAGE, (page - 1) * u64::from(PAGE)).await?;
            let watched = watch::watch(database, &declared, period, now).await?;
            Ok((metrics, records, watched))
        })
    });
    let (metrics, records, watched) = match read {
        Ok(read) => read,
        Err(reply) => return reply,
    };
    let pages = metrics.records.div_ceil(u64::from(PAGE)).max(1);
    if metrics.records == 0 {
        let (headline, answer) = not_run(&watched, &name, now);
        return Reply::screen(
            Screen::new().with(Block::Answer {
                headline: headline.clone(),
                fields: Vec::new(),
            }),
            Json::object([
                ("schedule", Json::from(name.as_str())),
                ("answer", Json::from(answer)),
                ("records", Json::array(Vec::<Json>::new())),
            ]),
        );
    }
    if page > pages {
        return Reply::failed(arguments(
            format!("`{name}` has no page {page}"),
            format!(
                "Its {} make {}.",
                plural(metrics.records, "record", "records"),
                plural(pages, "page", "pages")
            ),
            format!("Use a page from 1 to {pages}"),
        ));
    }
    let mut screen = Screen::new();
    let mut days: Vec<(String, Vec<&RunRecord>)> = Vec::new();
    for record in &records {
        let day = jadpo_std::format_instant(record.started)
            .get(..10)
            .unwrap_or_default()
            .to_string();
        match days.last_mut() {
            Some((last, rows)) if *last == day => rows.push(record),
            _ => days.push((day, vec![record])),
        }
    }
    for (day, rows) in &days {
        let mut table = Table::new(["due (UTC)", "started", "took", "outcome"]).right(2);
        table.heading = Some(day.clone());
        for record in rows {
            table = table.row(history_row(record, now));
        }
        screen.push(Block::Table(table));
    }
    screen.push(Block::fields(metric_fields(&metrics)));
    if page < pages {
        screen.push(Block::hint(
            format!("jadpo schedules history {name} --page {}", page + 1),
            format!(
                "shows the {} before these",
                PAGE.min(u32::try_from(metrics.records - page * u64::from(PAGE)).unwrap_or(PAGE))
            ),
        ));
    }
    Reply::screen(
        screen,
        Json::object([
            ("schedule", Json::from(name.as_str())),
            ("page", Json::from(i64::try_from(page).unwrap_or(i64::MAX))),
            (
                "pages",
                Json::from(i64::try_from(pages).unwrap_or(i64::MAX)),
            ),
            ("records", Json::array(records.iter().map(run_json))),
            ("metrics", metrics_json(&metrics)),
        ]),
    )
}

fn history_row(record: &RunRecord, now: i64) -> Vec<Cell> {
    let same_day = |at: i64| {
        let text = jadpo_std::format_instant(at);
        text.get(11..19).unwrap_or_default().to_string()
    };
    let due = if record.by_hand_run() {
        Cell::state(Symbol::Skipped, "")
    } else {
        let text = jadpo_std::format_instant(record.due);
        let minutes = text.get(11..16).unwrap_or_default();
        let seconds = text.get(16..19).filter(|s| *s != ":00").unwrap_or("");
        Cell::text(format!("{minutes}{seconds}"))
    };
    let by_hand = if record.by_hand_run() {
        " · by hand"
    } else {
        ""
    };
    match record.outcome.as_str() {
        "skipped" => vec![
            due,
            Cell::state(Symbol::Skipped, ""),
            Cell::state(Symbol::Skipped, ""),
            Cell::state(Symbol::Skipped, "skipped").then(skip_reason(record.reason.as_deref())),
        ],
        "running" => vec![
            due,
            Cell::muted(same_day(record.started)),
            Cell::muted(format!("{} so far", duration(record.duration_ms(now)))),
            Cell::state(Symbol::Running, "running").then(by_hand.trim_start_matches(" · ")),
        ],
        "succeeded" => vec![
            due,
            Cell::muted(same_day(record.started)),
            Cell::muted(duration(record.duration_ms(now))),
            Cell::state(Symbol::Success, "succeeded").then(format!("{}{by_hand}", did(record))),
        ],
        _ => vec![
            due,
            Cell::muted(same_day(record.started)),
            Cell::muted(duration(record.duration_ms(now))),
            Cell::state(Symbol::Failure, "failed").then(format!(
                "{}{by_hand}",
                failure(record.code.as_deref(), record.kind.as_deref())
            )),
        ],
    }
}

fn metric_fields(metrics: &Metrics) -> Vec<(String, String)> {
    let mut runs = format!("{} in the last 30 days", metrics.runs);
    if metrics.failed > 0 {
        runs.push_str(&format!(" · {} failed", metrics.failed));
    }
    if metrics.skipped > 0 {
        runs.push_str(&format!(
            " · {}",
            plural(metrics.skipped, "skipped tick", "skipped ticks")
        ));
    }
    let mut fields = vec![("runs".to_string(), runs)];
    if let (Some(median), Some(longest)) = (metrics.median_ms, metrics.longest_ms) {
        fields.push((
            "took".to_string(),
            format!(
                "median {} · longest {}",
                duration(median),
                duration(longest)
            ),
        ));
    }
    if let (Some(median), Some(most)) = (metrics.median_delay_ms, metrics.most_delay_ms) {
        fields.push((
            "delay".to_string(),
            format!("median {} · at most {}", duration(median), duration(most)),
        ));
    }
    fields
}

fn metrics_json(metrics: &Metrics) -> Json {
    let n = |v: u64| Json::from(i64::try_from(v).unwrap_or(i64::MAX));
    let ms = |v: Option<i64>| v.map_or(Json::Null, Json::from);
    Json::object([
        ("records", n(metrics.records)),
        ("runs", n(metrics.runs)),
        ("failed", n(metrics.failed)),
        ("skipped", n(metrics.skipped)),
        ("medianMs", ms(metrics.median_ms)),
        ("longestMs", ms(metrics.longest_ms)),
        ("medianDelayMs", ms(metrics.median_delay_ms)),
        ("mostDelayMs", ms(metrics.most_delay_ms)),
    ])
}

/// What to say of a schedule with no records, and its answer's id.
fn not_run(watched: &Watch, name: &str, now: i64) -> (String, &'static str) {
    match watched.schedules.first().and_then(|s| s.state.as_ref()) {
        Some(state) => (
            format!(
                "`{name}` hasn't been due yet: its first run is due at {}",
                grid_clock(state.next_run_at, now)
            ),
            "not_due",
        ),
        None => (
            format!("No tick has seen `{name}` yet: the app hasn't run since it was added"),
            "not_seen",
        ),
    }
}

// --- pause and resume -------------------------------------------------------

fn pause(call: &Call, project: &Project, schedule: &ScheduleModel, paused: bool) -> Reply {
    let (store, url) = match project.database(call) {
        Ok(found) => found,
        Err(reply) => return reply,
    };
    let name = project.shown(schedule);
    let declared = Declared {
        key: schedule.key.as_str().to_string(),
        name: name.clone(),
        every_ms: schedule.every_ms,
    };
    let now = now(call);
    let period = tick_every(std::slice::from_ref(&declared));
    let done = with_database(&store, &url, |database| {
        let declared = declared.clone();
        Box::pin(async move {
            let changed = watch::set_paused(database, &declared, paused, now).await?;
            let watched =
                watch::watch(database, std::slice::from_ref(&declared), period, now).await?;
            Ok((changed, watched))
        })
    });
    let (changed, watched) = match done {
        Ok(done) => done,
        Err(reply) => return reply,
    };
    let state = watched.schedules.first().and_then(|s| s.state.clone());
    let running = watched
        .schedules
        .first()
        .and_then(|s| s.last_run.clone())
        .filter(|run| run.is_running() && state.as_ref().is_some_and(|s| s.leased(now)));
    let mut screen = Screen::new();
    let (text, did) = match (changed, paused) {
        (Paused::Already, true) => (format!("`{name}` was already paused"), false),
        (Paused::Already, false) => (format!("`{name}` wasn't paused"), false),
        (Paused::Changed { .. }, true) => {
            let mut text =
                format!("Paused `{name}`: its due ticks are skipped until it is resumed");
            if let Some(run) = &running {
                text.push_str(&format!(
                    "; its run from {} carries on",
                    clock(run.started, now)
                ));
            }
            (text, true)
        }
        (Paused::Changed { next_run_at }, false) => (
            match next_run_at {
                Some(at) if at > now => {
                    format!(
                        "Resumed `{name}`: its next run is at {}",
                        grid_clock(at, now)
                    )
                }
                _ => format!("Resumed `{name}`: its next run is due now"),
            },
            true,
        ),
    };
    screen.push(Block::Status {
        outcome: Outcome::Success,
        text,
        rows: Vec::new(),
    });
    if paused {
        screen.push(Block::hint(
            format!("jadpo schedules resume {name}"),
            "starts it again",
        ));
    }
    Reply::screen(
        screen,
        Json::object([
            ("schedule", Json::from(name.as_str())),
            ("paused", Json::from(paused)),
            ("changed", Json::from(did)),
            ("nextRunAt", instant(state.map(|s| s.next_run_at))),
        ]),
    )
}

// --- run --------------------------------------------------------------------

fn run(call: &Call, project: &Project, schedule: &ScheduleModel) -> Reply {
    let name = project.shown(schedule);
    let (state_store, state_url) = match project.database(call) {
        Ok(found) => found,
        Err(reply) => return reply,
    };
    // Only `jadpo dev`'s own database, which it notes for the session, is
    // run against here; any other is a deployment's, whose own server
    // runs it (area 12, decided 2026-10-19).
    if dev_stores(&project.root).get(&state_store) != Some(&state_url.url) {
        return request(call, project, schedule, &state_store, &state_url);
    }
    crate::run::say(&format!("Running `{name}`"));
    // The project as `jadpo dev` runs it: checked, with its settings.
    let ready = match crate::dev::prepare(&project.root, &call.environment, true) {
        Ok(ready) => ready,
        Err((problems, _)) => {
            let cwd = call.cwd.clone().unwrap_or_else(|| PathBuf::from("."));
            return crate::migrate::blocked(
                &project.loaded,
                &cwd,
                problems,
                "schedules run stopped",
            );
        }
    };
    let mut stores: Vec<String> = ready.stores.clone();
    if !stores.contains(&state_store) {
        stores.push(state_store.clone());
    }
    let mut urls = BTreeMap::new();
    for store in &stores {
        let url = if call.value("--url").is_some() && store != &state_store && stores.len() > 1 {
            None
        } else if let Some(configured) = ready.configured.get(store) {
            match Address::of(call, &project.loaded, store, configured.clone()) {
                Ok(address) => Some(address),
                Err(reply) => return reply,
            }
        } else {
            match url_of(call, &project.root, &project.loaded, store) {
                Ok(url) => url,
                Err(reply) => return reply,
            }
        };
        match url {
            Some(url) => {
                urls.insert(store.clone(), url);
            }
            None => {
                return Reply::failed(arguments(
                    format!("`jadpo schedules run` needs the address of the database `{store}`"),
                    "Running a schedule runs its action against every database it reads and writes, and no variable gives this one's address.".to_string(),
                    "Run `jadpo config set <the connection's setting>`, or start `jadpo dev`".to_string(),
                ));
            }
        }
    }
    let key = schedule.key.as_str().to_string();
    let now = now(call);
    let prepared = std::sync::Arc::clone(&ready.prepared);
    let config = std::sync::Arc::clone(&ready.config);
    let store = state_store.clone();
    let ran = jadpo_interp::on_large_stack(move || {
        let program = jadpo_interp::Program::new(prepared).with_config(&config);
        let mut host = jadpo_host_native::NativeHost::new();
        for (store, url) in &urls {
            let database = url.open()?;
            host = host.with_store(store.clone(), database);
        }
        let runtime = jadpo_host_native::Runtime::new()
            .map_err(|error| jadpo_runtime::StoreError::Fault(error.to_string()))?;
        let plan = program
            .plan()
            .schedules()
            .schedule(&key)
            .cloned()
            .ok_or_else(|| {
                jadpo_runtime::StoreError::Fault(format!("no schedule `{key}` to run"))
            })?;
        // What crosses back to this thread: how the run went, never a
        // value of the app's.
        Ok(
            match runtime.block_on(watch::run_now(&program, &host, &store, &plan, now))? {
                RunNow::Ran { due, .. } => Ok(due),
                RunNow::LeaseHeld { started, until } => Err((started, until)),
            },
        )
    });
    let ran = match ran {
        Ok(Ok(ran)) => ran,
        Ok(Err(error)) => {
            return Reply::failed(
                Diagnostic::new(
                    &rule::SCHEDULES_UNAVAILABLE,
                    format!("`{name}` couldn't be run"),
                )
                .reason(format!(
                    "Reaching its databases failed: {}.",
                    store_error(&error)
                ))
                .next("Check the databases are running, then run `jadpo migrate apply`"),
            );
        }
        Err(error) => {
            return Reply::failed(
                Diagnostic::new(
                    &rule::SCHEDULES_UNAVAILABLE,
                    format!("`{name}` couldn't be run"),
                )
                .reason(format!("Starting it failed: {error}."))
                .next(format!("Run `jadpo schedules run {name}` again")),
            );
        }
    };
    match ran {
        Err((started, until)) => lease_held(&name, started, until, now),
        Ok(due) => ran_screen(&name, &due, false),
    }
}

/// How often the wait for a requested run reads its record.
const POLL_MS: u64 = 200;

/// How a wait for the run asked of the application's tick ended.
enum Waited {
    /// Its record ended.
    Ended(RunRecord),
    /// A run holds the lease, so nothing was asked: when it started, if
    /// its record says, and the lease's end.
    LeaseHeld { started: Option<i64>, until: i64 },
    /// No tick took it in time, so it was withdrawn; whether a run held
    /// the lease then.
    NoTick { lease_held: bool },
    /// A tick took it, but its run held the lease past its end and never
    /// ended: its worker was lost.
    Lost(RunRecord),
}

/// `jadpo schedules run` against a deployment (area 12, decided
/// 2026-10-19; chunk 6.2d): asks the application's own tick to run the
/// schedule by hand, under its lease, and waits for that run's record.
/// Nothing of the application runs here, and none of its settings are read.
fn request(
    call: &Call,
    project: &Project,
    schedule: &ScheduleModel,
    store: &str,
    url: &Address,
) -> Reply {
    let name = project.shown(schedule);
    let declared = Declared {
        key: schedule.key.as_str().to_string(),
        name: name.clone(),
        every_ms: schedule.every_ms,
    };
    // The application ticks for all its schedules at once.
    let period = tick_every(&project.declared());
    ask_tick(&declared, period, now(call), store, url)
}

/// Asks the application's next tick, ticking every `period`, to run
/// `declared` by hand, and waits for that run's record, as `jadpo
/// schedules run` does against a deployment and `jadpo deliveries retry`
/// does for a schedule's failed run (chunk 6.12).
pub(crate) fn ask_tick(
    declared: &Declared,
    period: u64,
    now: i64,
    store: &str,
    url: &Address,
) -> Reply {
    let name = declared.name.clone();
    crate::run::say(&format!("Asking the app's next tick to run `{name}`"));
    let waited = with_database(store, url, |database| {
        let declared = declared.clone();
        Box::pin(async move { wait_for(database, &declared, period, now).await })
    });
    let waited = match waited {
        Ok(waited) => waited,
        Err(reply) => return reply,
    };
    let wait = span(period.saturating_mul(2));
    match waited {
        Waited::Ended(record) => ran_screen(&name, &due_of(&record), true),
        Waited::LeaseHeld { started, until } => lease_held(&name, started, until, now),
        Waited::NoTick { lease_held } => Reply::failed(
            Diagnostic::new(
                &rule::SCHEDULES_NO_TICK,
                format!("No tick picked up `{name}`'s run in {wait}"),
            )
            .reason(if lease_held {
                "The app's own server runs it on a tick once no run holds its lease, and a run held the lease throughout; the request was taken back, and nothing ran.".to_string()
            } else {
                format!(
                    "The app's own server runs it on its next tick, which comes every {} while the app runs; none came, so the request was taken back, and nothing ran.",
                    span(period)
                )
            })
            .next(if lease_held {
                format!("See the run with `jadpo why {name}`, then run `jadpo schedules run {name}` again")
            } else {
                format!("Check the app is running, then run `jadpo schedules run {name}` again")
            }),
        ),
        Waited::Lost(record) => Reply::failed(
            Diagnostic::new(
                &rule::SCHEDULES_RUN_FAILED,
                format!("`{name}`'s run on the app never ended"),
            )
            .reason(format!(
                "It started at {} and held its lease past its end, so its worker was lost; the next tick records it as lease_expired.",
                clock(record.started, now)
            ))
            .next(format!("See it with `jadpo why {name}`")),
        ),
    }
}

/// Asks `schedule`'s next tick for a run and waits for its record:
/// until a tick takes it, at most twice the period the application ticks
/// at, or longer while another run holds the lease, up to the lease's
/// length; then until it ends, up to the lease's length again. Each is
/// measured on this machine's own clock, so its clock never has to agree
/// with the application's.
async fn wait_for(
    database: &dyn jadpo_runtime::Database,
    schedule: &Declared,
    period: u64,
    now: i64,
) -> Result<Waited, jadpo_runtime::StoreError> {
    use std::time::{Duration, Instant};
    let host = jadpo_host_native::NativeHost::new();
    let run = match watch::request_run(database, &host, schedule, now).await? {
        Requested::Waiting { run, .. } => run,
        Requested::LeaseHeld { started, until } => return Ok(Waited::LeaseHeld { started, until }),
    };
    crate::run::say(&format!(
        "Waiting for the app's next tick to run `{}`",
        schedule.name
    ));
    let tick_wait = Duration::from_millis(period.saturating_mul(2));
    // The longest a deployment's lease lasts, whatever budget its host
    // gives a tick (chunk 6.7).
    let lease = Duration::from_millis(u64::try_from(jadpo_runtime::lease::LONGEST_MS).unwrap_or(0));
    let began = Instant::now();
    let mut deadline = began + tick_wait;
    let mut taken: Option<Instant> = None;
    loop {
        if let Some(record) = watch::run(database, run).await? {
            if !record.is_running() {
                return Ok(Waited::Ended(record));
            }
            let since = *taken.get_or_insert_with(Instant::now);
            crate::run::say(&format!("Running `{}` on the app", schedule.name));
            if since.elapsed() > lease + tick_wait {
                return Ok(Waited::Lost(record));
            }
        } else {
            let state = watch::states(database)
                .await?
                .into_iter()
                .find(|state| state.schedule == schedule.key);
            let lease_held = state.as_ref().is_some_and(|state| state.run_id.is_some());
            let waiting = state
                .as_ref()
                .is_some_and(|state| state.requested == Some(run));
            if waiting && lease_held {
                // Another run holds the lease: the request waits for it,
                // and for a tick after it.
                deadline =
                    deadline.max((Instant::now() + tick_wait).min(began + lease + tick_wait));
            }
            // Taken by a tick, its record comes next; gone without one,
            // another wait withdrew it.
            if Instant::now() >= deadline
                && (!waiting || watch::withdraw(database, &schedule.key, run).await?)
            {
                return Ok(Waited::NoTick { lease_held });
            }
        }
        jadpo_runtime::Host::sleep(&host, POLL_MS).await;
    }
}

/// A requested run's record as the screens show a run.
fn due_of(record: &RunRecord) -> Due {
    Due {
        schedule: record.schedule.clone(),
        due_slot: record.due,
        ran: match record.outcome.as_str() {
            "succeeded" => Ran::Succeeded {
                events: u32::try_from(record.events.unwrap_or(0)).unwrap_or(0),
            },
            _ => Ran::Failed {
                code: record
                    .code
                    .clone()
                    .unwrap_or_else(|| "internal_fault".to_string()),
                kind: record.kind.clone(),
            },
        },
        started: record.started,
        ended: record.ended.unwrap_or(record.started),
        trace: record.trace.clone(),
        items: record.items.map(|items| u64::try_from(items).unwrap_or(0)),
        batches: Vec::new(),
        by_hand: record.by_hand_run(),
    }
}

/// `schedules.lease_held`: a run that started at `started` holds the
/// lease until `until`.
fn lease_held(name: &str, started: Option<i64>, until: i64, now: i64) -> Reply {
    let holder = match started {
        Some(started) => format!("A run that started at {}", clock(started, now)),
        None => "Another run".to_string(),
    };
    Reply::Failed(Box::new(
        Diagnostic::new(
            &rule::SCHEDULES_LEASE_HELD,
            format!("`{name}` is already running"),
        )
        .reason(format!(
            "{holder} holds its lease until {}, and only one run of a schedule happens at a time.",
            clock(until, now)
        ))
        .next(format!(
            "Wait for it to end, or see it with `jadpo why {name}`"
        )),
    ))
}

/// How a run went: here, against `jadpo dev`, or on the application's
/// tick, against a deployment (`on_app`).
fn ran_screen(name: &str, due: &Due, on_app: bool) -> Reply {
    let data = Json::object([
        ("schedule", Json::from(name)),
        (
            "outcome",
            Json::from(match due.ran {
                Ran::Succeeded { .. } => "succeeded",
                _ => "failed",
            }),
        ),
        ("ms", Json::from(due.duration_ms())),
        (
            "events",
            match due.ran {
                Ran::Succeeded { events } => Json::from(i64::from(events)),
                _ => Json::from(0),
            },
        ),
        ("items", due.items.map_or(Json::Null, count_json)),
        (
            "code",
            match &due.ran {
                Ran::Failed { code, .. } => Json::from(code.as_str()),
                _ => Json::Null,
            },
        ),
        (
            "kind",
            match &due.ran {
                Ran::Failed {
                    kind: Some(kind), ..
                } => Json::from(kind.as_str()),
                _ => Json::Null,
            },
        ),
        (
            "traceparent",
            due.trace.as_deref().map_or(Json::Null, Json::from),
        ),
        ("requested", Json::from(on_app)),
    ]);
    let ran = if on_app {
        format!("The app's tick ran `{name}`")
    } else {
        format!("Ran `{name}`")
    };
    match &due.ran {
        Ran::Failed { code, kind } => {
            let screen = Screen::new()
                .with(Block::Failed {
                    subject: format!("`{name}`"),
                    fields: vec![
                        ("failure".to_string(), failure(Some(code), kind.as_deref())),
                        ("took".to_string(), duration(due.duration_ms())),
                    ],
                })
                .with(Block::hint(format!("jadpo why {name}"), "explains the run"));
            // The screen says it; the diagnostic gives `--json` and the
            // exit code the failure.
            let diagnostic = Diagnostic::new(
                &rule::SCHEDULES_RUN_FAILED,
                format!("`{name}` failed with {code}"),
            )
            .reason(format!(
                "Its action ended with {}{}; the run is recorded, and its next due time is its retry.",
                failure(Some(code), kind.as_deref()),
                if on_app { " on the app's tick" } else { "" }
            ))
            .next(format!("Fix the cause, then run `jadpo schedules run {name}` again"));
            Reply::Report {
                screen,
                data,
                diagnostics: vec![diagnostic],
                sources: BTreeMap::new(),
            }
        }
        Ran::Succeeded { events: count } => Reply::screen(
            Screen::new().with(Block::Status {
                outcome: Outcome::Success,
                text: match due.items {
                    Some(items) => format!(
                        "{ran} in {}: {}",
                        duration(due.duration_ms()),
                        purged(items)
                    ),
                    None => format!(
                        "{ran} in {}, emitting {}",
                        duration(due.duration_ms()),
                        plural(u64::from(*count), "event", "events")
                    ),
                },
                rows: Vec::new(),
            }),
            data,
        ),
        Ran::Skipped(_) => Reply::screen(Screen::new(), data),
    }
}

// --- why --------------------------------------------------------------------

fn explain(call: &Call, project: &Project, schedule: &ScheduleModel) -> Reply {
    let (store, url) = match project.database(call) {
        Ok(found) => found,
        Err(reply) => return reply,
    };
    let name = project.shown(schedule);
    let declared = Declared {
        key: schedule.key.as_str().to_string(),
        name: name.clone(),
        every_ms: schedule.every_ms,
    };
    let now = now(call);
    let period = tick_every(std::slice::from_ref(&declared));
    // The answer is the runtime's, as data (chunk 6.9); this says it.
    let why = match with_database(&store, &url, |database| {
        Box::pin(async move { why::why(database, &declared, period, now).await })
    }) {
        Ok(why) => why,
        Err(reply) => return reply,
    };
    let place = format!("{}:{}", schedule.path, schedule.line);
    let said = say(&why.answer, &name, now);
    let mut fields = said.fields;
    // A run asked of the application's tick that none has taken yet
    // (chunk 6.2d).
    if why.requested {
        fields.push((
            "request".to_string(),
            "a run by hand, waiting for the app's next tick".to_string(),
        ));
    }
    fields.push(("schedule".to_string(), place.clone()));
    let mut screen = Screen::new().with(Block::Answer {
        headline: said.headline.clone(),
        fields: fields.clone(),
    });
    let alerts: Vec<&Alert> = why.other_alerts().collect();
    if !alerts.is_empty() {
        screen.push(Block::Alerts(
            alerts.iter().map(|alert| alert_text(alert, now)).collect(),
        ));
    }
    if let Some((command, reason)) = &said.hint {
        screen.push(Block::hint(command.as_str(), reason.as_str()));
    }
    Reply::screen(
        screen,
        Json::object([
            ("schedule", Json::from(name.as_str())),
            ("answer", Json::from(why.answer.id())),
            ("headline", Json::from(said.headline.as_str())),
            (
                "facts",
                Json::object(
                    fields
                        .iter()
                        .map(|(label, value)| (label.clone(), Json::from(value.as_str())))
                        .collect::<Vec<(String, Json)>>(),
                ),
            ),
            ("place", Json::from(place.as_str())),
            ("alerts", alerts_json(&why.alerts)),
        ]),
    )
}

/// `jadpo why`'s answer in words: its headline, its facts and a hint.
struct Said {
    headline: String,
    fields: Vec<(String, String)>,
    hint: Option<(String, String)>,
}

/// The runtime's answer for schedule `name` as `jadpo why` says it at
/// `now`.
fn say(answer: &why::Answer, name: &str, now: i64) -> Said {
    use why::Answer;
    let field = |label: &str, value: String| (label.to_string(), value);
    let said = |headline: String, fields: Vec<(String, String)>| Said {
        headline,
        fields,
        hint: None,
    };
    match answer {
        Answer::NotSeen => said(
            format!("No tick has seen `{name}` yet: the app hasn't run since it was added"),
            Vec::new(),
        ),
        Answer::NotDue { first_due } => said(
            format!(
                "`{name}` hasn't been due yet: its first run is due at {}",
                grid_clock(*first_due, now)
            ),
            Vec::new(),
        ),
        Answer::Paused { skipped, last_run } => {
            let mut fields = Vec::new();
            if let Some(skips) = skipped {
                fields.push(field(
                    "skipped",
                    format!(
                        "{} since {}",
                        plural(skips.count, "due time", "due times"),
                        grid_clock(skips.since, now)
                    ),
                ));
            }
            if let Some(run) = last_run {
                fields.push(field("last run", run_words(run, now)));
            }
            Said {
                headline: format!("`{name}` is paused, so its due ticks are skipped"),
                fields,
                hint: Some((
                    format!("jadpo schedules resume {name}"),
                    "starts it again".to_string(),
                )),
            }
        }
        Answer::TicksStopped {
            since,
            missed_due,
            tick_every_ms,
        } => {
            let mut fields = Vec::new();
            if let Some(due) = missed_due {
                fields.push(field("due", format!("{}, not run", grid_clock(*due, now))));
            }
            fields.push(field(
                "ticks",
                format!(
                    "none since {}; the running app ticks every {}",
                    clock(*since, now),
                    duration(i64::try_from(*tick_every_ms).unwrap_or(60_000))
                ),
            ));
            said(
                format!("No tick has reached `{name}` since {}", clock(*since, now)),
                fields,
            )
        }
        Answer::LeaseExpired {
            started,
            until,
            next,
        } => {
            let headline = match started {
                Some(started) => format!(
                    "`{name}`'s run from {} held its lease past {}: its worker was lost",
                    clock(*started, now),
                    clock(*until, now)
                ),
                None => format!(
                    "`{name}`'s lease was held past {}: its worker was lost",
                    clock(*until, now)
                ),
            };
            said(
                headline,
                vec![field(
                    "next",
                    format!(
                        "{}: the next due tick records it as lease_expired and runs again",
                        grid_clock(*next, now)
                    ),
                )],
            )
        }
        Answer::Running {
            started,
            lease_until,
            skipped,
        } => {
            let mut fields = vec![field(
                "lease",
                format!(
                    "held until {}, the 15-minute limit and a minute",
                    clock(*lease_until, now)
                ),
            )];
            if *skipped > 0 {
                fields.push(field(
                    "skipped",
                    plural(*skipped, "due time so far", "due times so far"),
                ));
            }
            said(
                format!(
                    "`{name}`'s run from {} is still going, so its due ticks are skipped until it ends",
                    clock(*started, now)
                ),
                fields,
            )
        }
        Answer::Skipped {
            due,
            paused,
            last_run,
            next,
        } => {
            let mut fields = Vec::new();
            if let Some(run) = last_run {
                fields.push(field("last run", run_words(run, now)));
            }
            fields.push(field("next", grid_clock(*next, now)));
            said(
                format!(
                    "`{name}` skipped its tick due at {}: {}",
                    grid_clock(*due, now),
                    if *paused {
                        "it was paused then"
                    } else {
                        "its run before was still going"
                    }
                ),
                fields,
            )
        }
        Answer::Failed {
            run,
            failures_in_a_row,
            next,
        } => {
            let mut fields = vec![field(
                "run",
                format!(
                    "due {} · took {}{}",
                    grid_clock(run.due, now),
                    duration(run.duration_ms(now)),
                    if run.by_hand_run() { " · by hand" } else { "" }
                ),
            )];
            if let Some(kind) = &run.kind {
                fields.push(field("kind", kind.clone()));
            }
            if *failures_in_a_row > 1 {
                fields.push(field(
                    "before",
                    format!(
                        "{} in a row before this one",
                        plural(
                            u64::from(failures_in_a_row - 1),
                            "failed run",
                            "failed runs"
                        )
                    ),
                ));
            }
            fields.push(field(
                "next",
                format!(
                    "{}: a schedule's next run is its retry",
                    grid_clock(*next, now)
                ),
            ));
            if let Some(trace) = &run.trace {
                fields.push(field("trace", trace.clone()));
            }
            Said {
                headline: format!(
                    "`{name}` failed at {} with {}",
                    clock(run.started, now),
                    run.code.as_deref().unwrap_or("internal_fault")
                ),
                fields,
                hint: Some((
                    format!("jadpo schedules run {name}"),
                    "runs it again now, once the cause is fixed".to_string(),
                )),
            }
        }
        Answer::Succeeded { run, next } => said(
            format!("`{name}` ran at {} and succeeded", clock(run.started, now)),
            vec![
                field(
                    "run",
                    format!(
                        "due {} · took {} · {}{}",
                        grid_clock(run.due, now),
                        duration(run.duration_ms(now)),
                        did(run),
                        if run.by_hand_run() { " · by hand" } else { "" }
                    ),
                ),
                field("next", grid_clock(*next, now)),
            ],
        ),
    }
}

/// A run in words: `succeeded at 11:00:00`.
fn run_words(run: &RunRecord, now: i64) -> String {
    match run.outcome.as_str() {
        "running" => format!("running since {}", clock(run.started, now)),
        "succeeded" => format!("succeeded at {}", clock(run.started, now)),
        _ => format!(
            "failed at {} with {}",
            clock(run.started, now),
            run.code.as_deref().unwrap_or("internal_fault")
        ),
    }
}

/// A schedule's run in `jadpo dev`'s output and its `--json` envelope.
pub(crate) fn dev_line(name: &str, due: &Due) -> (String, Json) {
    let (word, detail) = match &due.ran {
        Ran::Succeeded { events } => (
            "succeeded",
            match due.items {
                Some(items) => purged(items),
                None => plural(u64::from(*events), "event", "events"),
            },
        ),
        Ran::Failed { code, kind } => ("failed", failure(Some(code), kind.as_deref())),
        Ran::Skipped(why) => ("skipped", skip_reason(Some(why.word())).to_string()),
    };
    let detail = if due.by_hand {
        format!("{detail} · by hand")
    } else {
        detail
    };
    let json = Json::object([
        ("event", Json::from("schedule")),
        ("schedule", Json::from(name)),
        ("outcome", Json::from(word)),
        ("detail", Json::from(detail.as_str())),
        ("dueSlot", instant(Some(due.due_slot))),
        ("startedAt", instant(Some(due.started))),
        ("ms", Json::from(due.duration_ms())),
        ("delayMs", Json::from(due.delay_ms())),
        (
            "traceparent",
            due.trace.as_deref().map_or(Json::Null, Json::from),
        ),
    ]);
    // A run an operator asked for says so (chunk 6.2d).
    let json = match json {
        Json::Object(mut fields) if due.by_hand => {
            fields.push(("reason".to_string(), Json::from("by_hand")));
            Json::Object(fields)
        }
        json => json,
    };
    // A purge's log line says how many records each batch deleted (area
    // 5, "Lifecycle"; chunk 6.2c).
    let json = match (json, due.items) {
        (Json::Object(mut fields), Some(items)) => {
            fields.push(("items".to_string(), count_json(items)));
            fields.push((
                "batches".to_string(),
                Json::Array(due.batches.iter().copied().map(count_json).collect()),
            ));
            Json::Object(fields)
        }
        (json, _) => json,
    };
    (detail, json)
}

/// An alert's line in `jadpo dev`'s output, its times in the local time
/// zone as the session's own lines are, and its log event, in UTC.
pub(crate) fn dev_alert(alert: &Alert, now: i64) -> (String, Json) {
    let shift = crate::watch::local_offset().saturating_mul(1000);
    let at = |millis: i64| millis.saturating_add(shift);
    let kind = match &alert.kind {
        AlertKind::TicksStopped { since } => AlertKind::TicksStopped { since: at(*since) },
        AlertKind::LeaseExpired { started, until } => AlertKind::LeaseExpired {
            started: started.map(at),
            until: at(*until),
        },
        AlertKind::LongRun {
            started,
            ms,
            running,
        } => AlertKind::LongRun {
            started: at(*started),
            ms: *ms,
            running: *running,
        },
        AlertKind::FailedInARow { count } => AlertKind::FailedInARow { count: *count },
        AlertKind::NoSuccess {
            since,
            ever,
            every_ms,
        } => AlertKind::NoSuccess {
            since: at(*since),
            ever: *ever,
            every_ms: *every_ms,
        },
        AlertKind::ConnectionUnavailable {
            connection,
            since,
            held,
        } => AlertKind::ConnectionUnavailable {
            connection: connection.clone(),
            since: at(*since),
            held: *held,
        },
        AlertKind::UnfinishedSteps {
            store,
            count,
            oldest,
        } => AlertKind::UnfinishedSteps {
            store: store.clone(),
            count: *count,
            oldest: at(*oldest),
        },
    };
    let local = Alert {
        schedule: alert.schedule.clone(),
        kind,
    };
    (alert_text(&local, at(now)), json_of(&alert.event()))
}

/// `1.2 s`, as the screens write a duration.
pub(crate) fn dev_duration(ms: i64) -> String {
    duration(ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: i64 = 1_792_404_230_000; // 2026-10-19T10:03:50Z

    #[test]
    fn a_run_in_jadpo_dev_is_one_json_event() {
        let due = Due {
            schedule: "schedule census".into(),
            due_slot: T - 2_000,
            ran: Ran::Failed {
                code: "bell_missing".into(),
                kind: Some("unavailable".into()),
            },
            started: T,
            ended: T + 41,
            trace: Some("00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01".into()),
            items: None,
            batches: Vec::new(),
            by_hand: false,
        };
        let (detail, json) = dev_line("census", &due);
        assert_eq!(detail, "bell_missing · unavailable");
        assert_eq!(
            json.to_line(),
            "{\"event\":\"schedule\",\"schedule\":\"census\",\"outcome\":\"failed\",\"detail\":\"bell_missing · unavailable\",\"dueSlot\":\"2026-10-19T10:03:48Z\",\"startedAt\":\"2026-10-19T10:03:50Z\",\"ms\":41,\"delayMs\":2000,\"traceparent\":\"00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01\"}\n"
        );
    }

    #[test]
    fn durations_and_times_read_as_the_design_language_writes_them() {
        assert_eq!(duration(412), "412 ms");
        assert_eq!(duration(1_234), "1.2 s");
        assert_eq!(duration(41_000), "41 s");
        assert_eq!(duration(13 * 60_000 + 5_000), "13 min");
        assert_eq!(duration(2 * 3_600_000 + 5 * 60_000), "2 h 5 min");
        assert_eq!(clock(T, T), "10:03:50");
        assert_eq!(clock(T - DAY, T), "2026-10-18 10:03");
        assert_eq!(grid_clock(T - 50_000, T), "10:03");
        assert_eq!(grid_clock(T, T), "10:03:50");
        assert_eq!(ago(T - 8_000, T), "8 s ago");
        assert_eq!(every(15 * 60_000), "15m");
        assert_eq!(every(86_400_000), "1d");
        assert_eq!(every(30_000), "30s");
    }
}
