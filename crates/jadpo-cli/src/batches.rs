//! `jadpo batches` (area 14, "`jadpo batches`"; area 12, "What an operator
//! sees" and "What `batches.status` shows"): the app's batch runs, file
//! reads and writes and `batches.each`, from the command line.
//!
//! - `jadpo batches` (or `list`) lists the runs kept, those running first,
//!   then those that ended, newest first, 20 a page: each one's call, its
//!   state and its progress as `batches.status` gives them. `--state`
//!   lists one state's.
//! - `show <id>` shows one run: its progress against its estimated total,
//!   when it started and last moved, when it should finish at the rate so
//!   far, why it failed, and its problems by row, column and rule, 20 a
//!   page, never a value.
//! - `cancel <id>` stops a running one after the batch in hand, as
//!   `batches.cancel` does, logged with who asked; one that has ended, or
//!   was already asked to stop, is left as it is.
//!
//! The databases are those of every store a batch walks, found as `jadpo
//! deliveries` finds its own: `--url` (with `--store` when there are
//! several), else the connection's setting, else a running `jadpo dev`'s.
//! Nothing of the app runs, and nothing a run saved is read.

use std::collections::BTreeMap;
use std::path::PathBuf;

use jadpo_diag::{Diagnostic, rule};
use jadpo_runtime::batches::operator::{
    self as runtime, CANCELLED, FAILED, FINISHED, Kept, Problem, RUNNING, STATES,
};
use jadpo_runtime::deliveries::{Done, hex, short};
use jadpo_term::{Block, Cell, Fact, Json, Outcome, Screen, Symbol, Table};

use crate::database::Address;
use crate::run::{Call, Reply, quoted};
use crate::schedules::{clock, duration, now, plural, unreadable, url_of};

/// What `jadpo batches` does.
pub const ACTIONS: &[crate::table::Value] = &[
    crate::table::Value {
        name: "list",
        description: "List the batch runs kept (the default)",
    },
    crate::table::Value {
        name: "show",
        description: "Show one run's progress and problems",
    },
    crate::table::Value {
        name: "cancel",
        description: "Stop a running one after the batch in hand",
    },
];

/// The states `--state` lists.
pub const STATE_VALUES: &[crate::table::Value] = &[
    crate::table::Value {
        name: "running",
        description: "Still going, or waiting to resume",
    },
    crate::table::Value {
        name: "finished",
        description: "Its last batch and `finished:` committed",
    },
    crate::table::Value {
        name: "failed",
        description: "Stopped for good, saying why",
    },
    crate::table::Value {
        name: "cancelled",
        description: "Stopped by a cancel after the batch in hand",
    },
];

/// How many runs, or problems, a page shows.
const PAGE: u64 = 20;

/// The fewest hex digits an id is given by.
const SHORTEST_ID: usize = 8;

pub fn batches(call: &Call) -> Reply {
    let action = call.arguments.first().map(String::as_str);
    match action {
        None | Some("list") => {
            if let Some(extra) = call.arguments.get(1) {
                return Reply::failed(arguments(
                    format!("`jadpo batches` lists them all, not `{extra}`"),
                    "Listing takes no id; `--state` picks which to list.".to_string(),
                    "Run `jadpo batches`, or `jadpo batches show <id>` for one".to_string(),
                ));
            }
            let state = match state(call) {
                Ok(state) => state,
                Err(reply) => return reply,
            };
            let page = match page(call) {
                Ok(page) => page,
                Err(reply) => return reply,
            };
            match Project::load(call) {
                Ok(project) => list(call, &project, state, page),
                Err(reply) => reply,
            }
        }
        Some(action @ ("show" | "cancel")) => {
            if let Some(extra) = call.arguments.get(2) {
                return Reply::failed(arguments(
                    format!("`jadpo batches {action}` takes one id, not `{extra}` too"),
                    "Each acts on one batch run, by the id the list shows.".to_string(),
                    format!("Run `jadpo batches {action} <id>`"),
                ));
            }
            if call.value("--state").is_some() {
                return Reply::failed(arguments(
                    format!("`--state` is for listing, not `{action}`"),
                    "Only the list is filtered by state.".to_string(),
                    "Leave out `--state`".to_string(),
                ));
            }
            if action == "cancel" && call.value("--page").is_some() {
                return Reply::failed(arguments(
                    "`--page` is for listing and `show`, not `cancel`".to_string(),
                    "The list and a run's problems are paged; a cancel acts on one run."
                        .to_string(),
                    "Leave out `--page`".to_string(),
                ));
            }
            let page = match page(call) {
                Ok(page) => page,
                Err(reply) => return reply,
            };
            let Some(written) = call.arguments.get(1).cloned() else {
                return Reply::failed(arguments(
                    format!("`jadpo batches {action}` needs an id"),
                    "Each acts on one batch run, by the id the list shows.".to_string(),
                    "Run `jadpo batches` to see them, with their ids".to_string(),
                ));
            };
            let suffix = match id_suffix(&written) {
                Ok(suffix) => suffix,
                Err(reply) => return reply,
            };
            let project = match Project::load(call) {
                Ok(project) => project,
                Err(reply) => return reply,
            };
            let databases = match project.databases(call) {
                Ok(databases) => databases,
                Err(reply) => return reply,
            };
            let (store, url, kept) = match locate(&databases, &written, &suffix, action) {
                Ok(found) => found,
                Err(reply) => return reply,
            };
            if action == "show" {
                show(call, &store, &url, &kept, page)
            } else {
                cancel(call, &store, &url, &kept)
            }
        }
        Some(other) => {
            let names: Vec<&str> = ACTIONS.iter().map(|action| action.name).collect();
            let near = jadpo_diag::nearest(other, names.iter().copied());
            Reply::failed(arguments(
                format!("`{other}` isn't something `jadpo batches` does"),
                format!(
                    "`jadpo batches` lists the app's batch runs, and `show` and `cancel` act on one.{}",
                    near.map(|near| format!(" Did you mean `{near}`?"))
                        .unwrap_or_default()
                ),
                match near {
                    Some(near) => format!("Use `{near}` instead of `{other}`"),
                    None => "Run `jadpo batches`, or `jadpo batches show <id>`".to_string(),
                },
            ))
        }
    }
}

fn arguments(message: String, reason: String, next: String) -> Diagnostic {
    Diagnostic::new(&rule::CLI_BATCHES_ARGUMENTS, message)
        .reason(reason)
        .next(next)
}

/// `--state`, when it names a state a run is kept in.
fn state(call: &Call) -> Result<Option<&'static str>, Reply> {
    let Some(written) = call.value("--state") else {
        return Ok(None);
    };
    if let Some(state) = STATES.iter().find(|state| **state == written) {
        return Ok(Some(state));
    }
    let near = jadpo_diag::nearest(written, STATES.iter().copied());
    Err(Reply::failed(arguments(
        format!("`{written}` isn't a state"),
        format!(
            "A batch run is {}.{}",
            crate::run::either(&quoted(STATES)),
            near.map(|near| format!(" Did you mean `{near}`?"))
                .unwrap_or_default()
        ),
        match near {
            Some(near) => format!("Use `--state {near}`"),
            None => "Use `--state running`, or leave it out".to_string(),
        },
    )))
}

/// `--page`, from 1.
fn page(call: &Call) -> Result<u64, Reply> {
    match call.value("--page") {
        None => Ok(1),
        Some(written) => match written.parse::<u64>() {
            Ok(page) if page >= 1 => Ok(page),
            _ => Err(Reply::failed(arguments(
                format!("`{written}` isn't a page"),
                "Pages are counted from 1.".to_string(),
                "Use a page such as `--page 2`".to_string(),
            ))),
        },
    }
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
            "Run `jadpo batches` to see them, with their ids".to_string(),
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

// --- the project and its databases --------------------------------------------

/// The stores a batch walks, which keep its runs.
struct Project {
    root: PathBuf,
    loaded: jadpo_db::Loaded,
    stores: Vec<String>,
}

impl Project {
    fn load(call: &Call) -> Result<Project, Reply> {
        let cwd = call.cwd.clone().unwrap_or_else(|| PathBuf::from("."));
        let Some(root) = jadpo_db::find_root(&cwd) else {
            return Err(crate::fmt::project_needed("batches"));
        };
        let loaded = jadpo_db::load(&root);
        let schema = jadpo_check::data::schema(&loaded.db, loaded.project);
        let runs = jadpo_std::system::BATCH_RUNS.table;
        let stores = schema
            .stores
            .iter()
            .filter(|(_, store)| store.tables.contains_key(runs))
            .map(|(name, _)| name.clone())
            .collect();
        Ok(Project {
            root,
            loaded,
            stores,
        })
    }

    /// Each store keeping batch runs, with its database's address: all of
    /// them, or with `--url`, the one it names.
    fn databases(&self, call: &Call) -> Result<Vec<(String, Address)>, Reply> {
        if let Some(url) = call.value("--url") {
            let chosen = match (call.value("--store"), self.stores.as_slice()) {
                (Some(store), _) if self.stores.iter().any(|name| name == store) => {
                    store.to_string()
                }
                (Some(store), []) => {
                    return Err(Reply::failed(arguments(
                        format!("`{store}` isn't a store that keeps batch runs"),
                        "The app starts no batch, so no store keeps runs.".to_string(),
                        "Leave out `--store`".to_string(),
                    )));
                }
                (Some(store), _) => {
                    return Err(Reply::failed(arguments(
                        format!("`{store}` isn't a store that keeps batch runs"),
                        format!(
                            "They are kept in {}.",
                            crate::run::both(&quoted(&self.stores))
                        ),
                        format!("Use {}", crate::run::either(&quoted(&self.stores))),
                    )));
                }
                (None, [only]) => only.clone(),
                (None, []) => return Ok(Vec::new()),
                (None, _) => {
                    return Err(Reply::failed(arguments(
                        "Which store is this database?".to_string(),
                        format!(
                            "Batch runs are kept in {}, and `--url` names one database.",
                            crate::run::both(&quoted(&self.stores))
                        ),
                        "Add `--store <name>`".to_string(),
                    )));
                }
            };
            let url = Address::of(call, &self.loaded, &chosen, url.to_string())?;
            return Ok(vec![(chosen, url)]);
        }
        if let Some(store) = call.value("--store") {
            return Err(Reply::failed(arguments(
                format!("`--store {store}` goes with `--url`"),
                "Without `--url`, every store's database is read, each from its connection's setting.".to_string(),
                "Add `--url postgresql://…`, or leave out `--store`".to_string(),
            )));
        }
        let mut out = Vec::new();
        for store in &self.stores {
            match url_of(call, &self.root, &self.loaded, store)? {
                Some(url) => out.push((store.clone(), url)),
                None => {
                    return Err(Reply::failed(arguments(
                        format!("`jadpo batches` needs the address of the database `{store}`"),
                        "No variable gives the address of its connection, and no `jadpo dev` is running with one: a connection reads its address from the setting its `url:` names (area 10), and `--url` names a database directly.".to_string(),
                        "Run `jadpo config set <the connection's setting>`, start `jadpo dev`, or add `--url postgresql://…`".to_string(),
                    )));
                }
            }
        }
        Ok(out)
    }
}

/// Runs `work` against the database at `url`; a failure to reach, read
/// or write it is `batches.unavailable`.
fn with_database<T>(
    store: &str,
    url: &Address,
    work: impl for<'d> FnOnce(
        &'d dyn jadpo_runtime::Database,
    )
        -> jadpo_runtime::LocalFuture<'d, Result<T, jadpo_runtime::StoreError>>,
) -> Result<T, Reply> {
    let failed = |error: jadpo_runtime::StoreError| {
        Reply::failed(unreadable(
            &rule::BATCHES_UNAVAILABLE,
            store,
            "batch runs",
            &error,
        ))
    };
    let runtime = jadpo_host_native::Runtime::new()
        .map_err(|error| failed(jadpo_runtime::StoreError::Fault(error.to_string())))?;
    let database = url.open().map_err(failed)?;
    runtime.block_on(work(&database)).map_err(failed)
}

/// The one run whose id ends with `suffix`, with its store and address:
/// `cli.batches_arguments` when none has it, or several do.
fn locate(
    databases: &[(String, Address)],
    written: &str,
    suffix: &str,
    command: &str,
) -> Result<(String, Address, Kept), Reply> {
    let mut found: Vec<(String, Address, Kept)> = Vec::new();
    for (store, url) in databases {
        let runs = with_database(store, url, |database| {
            let suffix = suffix.to_string();
            Box::pin(async move { runtime::find(database, &suffix).await })
        })?;
        found.extend(
            runs.into_iter()
                .map(|run| (store.clone(), url.clone(), run)),
        );
    }
    match found.as_slice() {
        [] => Err(Reply::failed(arguments(
            format!("No batch run has the id `{written}`"),
            "A run is kept until 30 days after it ends; `jadpo batches` lists those kept, with their ids.".to_string(),
            "Run `jadpo batches` to see them, with their ids".to_string(),
        ))),
        [one] => Ok(one.clone()),
        many => {
            let ids: Vec<String> = many
                .iter()
                .map(|(_, _, run)| format!("`{}`", hex(&run.id)))
                .collect();
            Err(Reply::failed(arguments(
                format!("More than one batch run has an id ending `{written}`"),
                format!("They are {}.", crate::run::both(&ids)),
                format!(
                    "Use more of the id, such as `jadpo batches {command} {}`",
                    hex(&many[0].2.id)
                ),
            )))
        }
    }
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

/// A run's call as a screen names it: the callable that started it, bare,
/// with its place when one callable starts the same call twice, and the
/// helper that started it, from its stable name (`action
/// stock.import_products reads csv ProductRow each action
/// stock.save_product` is `import_products` and `csv.read`).
pub(crate) fn call_words(name: &str) -> (String, String) {
    let (name, place) = match name.rsplit_once(" #") {
        Some((rest, n)) if !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()) => {
            (rest, Some(n))
        }
        _ => (name, None),
    };
    let cut = [" from ", " reads ", " each ", " writes "]
        .iter()
        .filter_map(|word| name.find(word))
        .min()
        .unwrap_or(name.len());
    let starter = &name[..cut];
    let qualified = starter.split_once(' ').map_or(starter, |(_, rest)| rest);
    let mut call = qualified
        .rsplit_once('.')
        .map_or(qualified, |(_, bare)| bare)
        .to_string();
    if let Some(place) = place {
        call.push_str(&format!(" #{place}"));
    }
    let helper = |word: &str, verb: &str| {
        name.split_once(word)
            .and_then(|(_, rest)| rest.split(' ').next())
            .map(|format| format!("{format}.{verb}"))
    };
    let helper = helper(" reads ", "read")
        .or_else(|| helper(" writes ", "write"))
        .unwrap_or_else(|| "batches.each".to_string());
    (call, helper)
}

/// How far a run is: `412 of about 1200`, `21 of 21` once finished, or
/// the items done alone before it has a total.
pub(crate) fn progress_words(done: i64, total: Option<i64>, finished: bool) -> String {
    match total {
        Some(total) if finished => format!("{done} of {total}"),
        Some(total) => format!("{done} of about {total}"),
        None => done.to_string(),
    }
}

fn state_cell(state: &str) -> Cell {
    match state {
        RUNNING => Cell::state(Symbol::Running, "running"),
        FINISHED => Cell::state(Symbol::Success, "finished"),
        FAILED => Cell::state(Symbol::Failure, "failed"),
        CANCELLED => Cell::state(Symbol::Skipped, "cancelled"),
        other => Cell::state(Symbol::Skipped, other.replace('_', " ")),
    }
}

fn instant(at: Option<i64>) -> Json {
    at.map_or(Json::Null, |at| {
        Json::from(jadpo_std::format_instant(at).as_str())
    })
}

fn uuid_text(id: &[u8; 16]) -> String {
    let hex = hex(id);
    format!(
        "{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..]
    )
}

fn count(n: i64) -> Json {
    Json::from(n)
}

fn kept_json(store: &str, run: &Kept) -> Json {
    let (call, helper) = call_words(&run.batch);
    Json::object([
        ("id", Json::from(short(&run.id).as_str())),
        ("uuid", Json::from(uuid_text(&run.id).as_str())),
        ("store", Json::from(store)),
        ("batch", Json::from(run.batch.as_str())),
        ("call", Json::from(call.as_str())),
        ("helper", Json::from(helper.as_str())),
        ("state", Json::from(run.state.as_str())),
        ("itemsDone", count(run.items_done)),
        ("estimatedTotal", run.total().map_or(Json::Null, count)),
        ("problems", count(run.problems)),
        ("startedAt", instant(Some(run.started_at))),
        ("lastPageAt", instant(Some(run.stepped_at))),
        ("endedAt", instant(run.finished_at)),
        ("estimatedFinishAt", instant(run.likely_finish())),
        (
            "failure",
            run.failure.as_deref().map_or(Json::Null, Json::from),
        ),
        ("cancelAskedAt", instant(run.cancel_asked)),
        (
            "lastAction",
            run.acted.as_ref().map_or(Json::Null, |acted| {
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

// --- the list ---------------------------------------------------------------

fn list(call: &Call, project: &Project, state: Option<&'static str>, page: u64) -> Reply {
    let databases = match project.databases(call) {
        Ok(databases) => databases,
        Err(reply) => return reply,
    };
    let states: Vec<&'static str> = match state {
        Some(state) => vec![state],
        None => STATES.to_vec(),
    };
    let limit = i64::try_from(page.saturating_mul(PAGE)).unwrap_or(i64::MAX);
    let mut runs: Vec<(String, Kept)> = Vec::new();
    let mut counts: BTreeMap<String, u64> = BTreeMap::new();
    for (store, url) in &databases {
        let states = states.clone();
        let read = with_database(store, url, |database| {
            Box::pin(async move {
                let runs = runtime::list(database, &states, limit).await?;
                let counts = runtime::counts(database).await?;
                Ok((runs, counts))
            })
        });
        let (found, counted) = match read {
            Ok(read) => read,
            Err(reply) => return reply,
        };
        runs.extend(found.into_iter().map(|run| (store.clone(), run)));
        for (state, n) in counted {
            *counts.entry(state).or_default() += n;
        }
    }
    // Those running first, then those that ended, newest first, across
    // the stores as within each.
    runs.sort_by(|(_, a), (_, b)| {
        (b.state == RUNNING)
            .cmp(&(a.state == RUNNING))
            .then(
                b.finished_at
                    .unwrap_or(b.started_at)
                    .cmp(&a.finished_at.unwrap_or(a.started_at)),
            )
            .then(b.id.cmp(&a.id))
    });
    let count = |state: &str| counts.get(state).copied().unwrap_or(0);
    let total: u64 = states.iter().map(|state| count(state)).sum();
    let pages = total.div_ceil(PAGE).max(1);
    if page > pages && total > 0 {
        return Reply::failed(arguments(
            format!("There is no page {page}"),
            format!(
                "{} make {}.",
                plural(total, "run", "runs"),
                plural(pages, "page", "pages")
            ),
            format!("Use a page from 1 to {pages}"),
        ));
    }
    let shown: Vec<(String, Kept)> = runs
        .into_iter()
        .skip(usize::try_from((page - 1) * PAGE).unwrap_or(usize::MAX))
        .take(PAGE as usize)
        .collect();
    let data = Json::object([
        ("state", state.map_or(Json::Null, Json::from)),
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
            "counts",
            Json::object(
                STATES
                    .iter()
                    .map(|state| {
                        (
                            state.to_string(),
                            Json::from(i64::try_from(count(state)).unwrap_or(i64::MAX)),
                        )
                    })
                    .collect::<Vec<(String, Json)>>(),
            ),
        ),
        (
            "runs",
            Json::array(shown.iter().map(|(store, run)| kept_json(store, run))),
        ),
    ]);
    let mut screen = Screen::new();
    if shown.is_empty() {
        let headline = match state {
            None => "No batch run is kept",
            Some(RUNNING) => "Nothing is running",
            Some(FINISHED) => "Nothing has finished",
            Some(FAILED) => "Nothing failed",
            Some(_) => "Nothing was cancelled",
        };
        screen.push(Block::summary(
            Outcome::Success,
            headline.to_string(),
            Vec::new(),
        ));
        return Reply::screen(screen, data);
    }
    let mut table = Table::new(["id", "batch", "state", "progress", "problems"])
        .cut(1)
        .right(4);
    for (_, run) in &shown {
        let (call, helper) = call_words(&run.batch);
        table = table.row(vec![
            Cell::text(short(&run.id)),
            Cell::text(call).then(helper),
            state_cell(&run.state),
            Cell::text(progress_words(
                run.items_done,
                run.total(),
                run.state == FINISHED,
            )),
            Cell::text(run.problems.to_string()),
        ]);
    }
    screen.push(Block::Table(table));
    // The headline is what needs a look first: a failed run, else those
    // going, else those that finished; the other states are facts.
    let words = [
        (FAILED, "failed"),
        (RUNNING, "running"),
        (FINISHED, "finished"),
        (CANCELLED, "cancelled"),
    ];
    let listed: Vec<(&str, u64)> = words
        .iter()
        .filter(|(state, _)| states.contains(state))
        .map(|(state, word)| (*word, count(state)))
        .filter(|(_, n)| *n > 0)
        .collect();
    let (outcome, headline, rest) = match listed.split_first() {
        Some(((word, n), rest)) => (
            match *word {
                "failed" => Outcome::Failure,
                "running" => Outcome::Running,
                _ => Outcome::Success,
            },
            format!("{n} {word}"),
            rest.to_vec(),
        ),
        None => (Outcome::Success, "0 runs".to_string(), Vec::new()),
    };
    let mut facts: Vec<Fact> = rest
        .iter()
        .map(|(word, n)| Fact::plain(format!("{n} {word}")))
        .collect();
    if pages > 1 {
        facts.push(Fact::plain(format!("page {page} of {pages}")));
    }
    screen.push(Block::summary(outcome, headline, facts).with_plain_headline("batch runs"));
    let first = |wanted: &dyn Fn(&Kept) -> bool| {
        shown
            .iter()
            .find(|(_, run)| wanted(run))
            .map(|(_, run)| short(&run.id))
    };
    let hint = first(&|run| run.state == FAILED)
        .map(|id| (format!("jadpo batches show {id}"), "says why it failed"))
        .or_else(|| {
            first(&|run| run.problems > 0)
                .map(|id| (format!("jadpo batches show {id}"), "lists its problems"))
        })
        .or_else(|| {
            first(&|run| run.state == RUNNING).map(|id| {
                (
                    format!("jadpo batches show {id}"),
                    "says when it should finish",
                )
            })
        });
    if let Some((command, reason)) = hint {
        screen.push(Block::hint(command, reason));
    }
    Reply::screen(screen, data)
}

// --- one run ----------------------------------------------------------------

/// `jadpo batches show <id>`: one run, and a page of its problems.
fn show(call: &Call, store: &str, url: &Address, run: &Kept, page: u64) -> Reply {
    let problems_total = u64::try_from(run.problems).unwrap_or(0);
    let pages = problems_total.div_ceil(PAGE).max(1);
    if page > pages {
        return Reply::failed(arguments(
            format!("There is no page {page} of its problems"),
            format!(
                "{} make {}.",
                plural(problems_total, "problem", "problems"),
                plural(pages, "page", "pages")
            ),
            format!("Use a page from 1 to {pages}"),
        ));
    }
    let skip = i64::try_from((page - 1) * PAGE).unwrap_or(i64::MAX);
    let problems = if problems_total == 0 {
        Vec::new()
    } else {
        let id = run.id;
        match with_database(store, url, |database| {
            Box::pin(async move { runtime::problems(database, &id, PAGE as i64, skip).await })
        }) {
            Ok(problems) => problems,
            Err(reply) => return reply,
        }
    };
    let (screen, data) = show_screen(store, run, &problems, (page, pages), now(call));
    Reply::screen(screen, data)
}

/// One run's screen and its `--json`: what `jadpo batches show` and `jadpo
/// deliveries show` give for it.
pub(crate) fn show_screen(
    store: &str,
    run: &Kept,
    problems: &[Problem],
    (page, pages): (u64, u64),
    now: i64,
) -> (Screen, Json) {
    let id = short(&run.id);
    let (call, helper) = call_words(&run.batch);
    let mut fields = vec![("batch".to_string(), format!("{call} · {helper}"))];
    let mut progress = progress_words(run.items_done, run.total(), run.state == FINISHED);
    if run.state == RUNNING
        && let Some(total) = run.total().filter(|total| *total > 0)
    {
        let percent = (i128::from(run.items_done) * 100 / i128::from(total)).clamp(0, 99);
        progress.push_str(&format!(" · {percent}%"));
    }
    fields.push(("progress".to_string(), progress));
    fields.push(("started".to_string(), clock(run.started_at, now)));
    match run.finished_at {
        Some(ended) => fields.push((
            "ended".to_string(),
            format!(
                "{} · took {}",
                clock(ended, now),
                duration(ended - run.started_at)
            ),
        )),
        None => fields.push(("updated".to_string(), clock(run.stepped_at, now))),
    }
    let finish = run.likely_finish().filter(|at| *at > now);
    if let Some(at) = finish {
        fields.push(("finishes".to_string(), format!("about {}", clock(at, now))));
    }
    if run.state == RUNNING
        && let Some(asked) = run.cancel_asked
    {
        let by = run
            .acted
            .as_ref()
            .filter(|acted| acted.action == "cancel")
            .map(|acted| format!(" by {}", acted.operator))
            .unwrap_or_default();
        fields.push((
            "cancel".to_string(),
            format!(
                "asked at {}{by}; it stops after the batch in hand",
                clock(asked, now)
            ),
        ));
    }
    if let Some(why) = &run.failure {
        fields.push(("reason".to_string(), why.clone()));
    }
    if let Some(acted) = &run.acted {
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
    let mut screen = Screen::new().with(Block::Fields(fields));
    if !problems.is_empty() {
        let mut table = Table::new(["row", "column", "rule"]).right(0).cut(1);
        for problem in problems {
            table = table.row(vec![
                Cell::text(problem.row.to_string()),
                if problem.column.is_empty() {
                    Cell::muted("–")
                } else {
                    Cell::text(problem.column.clone())
                },
                Cell::text(problem.rule.clone()),
            ]);
        }
        screen.push(Block::Table(table));
    }
    let mut facts = Vec::new();
    if run.problems > 0 {
        facts.push(Fact::problem(plural(
            u64::try_from(run.problems).unwrap_or(0),
            "problem",
            "problems",
        )));
    }
    if pages > 1 {
        facts.push(Fact::plain(format!("page {page} of {pages}")));
    }
    let outcome = match run.state.as_str() {
        RUNNING => Outcome::Running,
        FAILED => Outcome::Failure,
        _ => Outcome::Success,
    };
    screen.push(
        Block::summary(outcome, state_cell(&run.state).text, facts)
            .with_plain_headline("batch run"),
    );
    let hint = if page < pages {
        Some((
            format!("jadpo batches show {id} --page {}", page + 1),
            "lists the next problems",
        ))
    } else if run.state == RUNNING && run.cancel_asked.is_none() {
        Some((
            format!("jadpo batches cancel {id}"),
            "stops it after the batch in hand",
        ))
    } else {
        None
    };
    if let Some((command, reason)) = hint {
        screen.push(Block::hint(command, reason));
    }
    let data = Json::object([
        ("run", kept_json(store, run)),
        ("page", Json::from(i64::try_from(page).unwrap_or(i64::MAX))),
        (
            "pages",
            Json::from(i64::try_from(pages).unwrap_or(i64::MAX)),
        ),
        (
            "problems",
            Json::array(problems.iter().map(|problem| {
                Json::object([
                    ("row", count(problem.row)),
                    (
                        "column",
                        if problem.column.is_empty() {
                            Json::Null
                        } else {
                            Json::from(problem.column.as_str())
                        },
                    ),
                    ("rule", Json::from(problem.rule.as_str())),
                ])
            })),
        ),
    ]);
    (screen, data)
}

/// The problems of run `id` in `store`, a page of them, for `jadpo
/// deliveries show`.
pub(crate) fn first_problems(
    store: &str,
    url: &Address,
    run: &Kept,
) -> Result<Vec<Problem>, Reply> {
    if run.problems == 0 {
        return Ok(Vec::new());
    }
    let id = run.id;
    with_database(store, url, |database| {
        Box::pin(async move { runtime::problems(database, &id, PAGE as i64, 0).await })
    })
}

/// The run `id` names in `store`, for `jadpo deliveries`.
pub(crate) fn kept(store: &str, url: &Address, id: &[u8; 16]) -> Result<Option<Kept>, Reply> {
    let suffix = hex(id);
    let found = with_database(store, url, |database| {
        Box::pin(async move { runtime::find(database, &suffix).await })
    })?;
    Ok(found.into_iter().next())
}

/// The pages a run's problems make.
pub(crate) fn problem_pages(run: &Kept) -> u64 {
    u64::try_from(run.problems)
        .unwrap_or(0)
        .div_ceil(PAGE)
        .max(1)
}

// --- cancelling ---------------------------------------------------------------

/// `jadpo batches cancel <id>`: asks a running run to stop after the
/// batch in hand, logged with who asked. A run that has ended, or was
/// already asked to stop, is left as it is, and says so.
fn cancel(call: &Call, store: &str, url: &Address, run: &Kept) -> Reply {
    let now = now(call);
    let id = short(&run.id);
    let operator = operator(call);
    let done = if run.state == RUNNING && run.cancel_asked.is_none() {
        let run_id = run.id;
        let who = operator.clone();
        match with_database(store, url, |database| {
            Box::pin(async move {
                let host = jadpo_host_native::NativeHost::new();
                runtime::cancel(database, &host, &run_id, &who, now).await
            })
        }) {
            Ok(done) => done,
            Err(reply) => return reply,
        }
    } else {
        Done::Changed
    };
    let (call_name, _) = call_words(&run.batch);
    match done {
        Done::Logged { role } => {
            let screen = Screen::new()
                .with(Block::Status {
                    outcome: Outcome::Success,
                    text: format!(
                        "Asked `{id}` to stop: `{call_name}` stops after the batch in hand"
                    ),
                    rows: vec![("logged".to_string(), format!("{operator} · role {role}"))],
                })
                .with(Block::hint(
                    format!("jadpo batches show {id}"),
                    "shows how far it got",
                ));
            Reply::screen(
                screen,
                Json::object([
                    ("id", Json::from(id.as_str())),
                    ("uuid", Json::from(uuid_text(&run.id).as_str())),
                    ("store", Json::from(store)),
                    ("cancelled", Json::from(true)),
                    ("was", Json::from(RUNNING)),
                    ("operator", Json::from(operator.as_str())),
                    ("role", Json::from(role.as_str())),
                    ("at", instant(Some(now))),
                ]),
            )
        }
        Done::Changed => {
            // Read it again: it may have ended, or been asked to stop,
            // since it was found.
            let now_kept = match kept(store, url, &run.id) {
                Ok(found) => found.unwrap_or_else(|| run.clone()),
                Err(reply) => return reply,
            };
            let text = match now_kept.state.as_str() {
                RUNNING => match now_kept.cancel_asked {
                    Some(asked) => format!(
                        "`{id}` is already stopping: a cancel was asked at {}",
                        clock(asked, now)
                    ),
                    None => format!("`{id}` is still running"),
                },
                FINISHED => format!("`{id}` had already finished, so there is nothing to cancel"),
                FAILED => format!("`{id}` had already failed, so there is nothing to cancel"),
                _ => format!("`{id}` was already cancelled"),
            };
            let screen = Screen::new().with(Block::Status {
                outcome: Outcome::Success,
                text,
                rows: Vec::new(),
            });
            Reply::screen(
                screen,
                Json::object([
                    ("id", Json::from(id.as_str())),
                    ("uuid", Json::from(uuid_text(&run.id).as_str())),
                    ("store", Json::from(store)),
                    ("cancelled", Json::from(false)),
                    ("was", Json::from(now_kept.state.as_str())),
                    ("cancelAskedAt", instant(now_kept.cancel_asked)),
                ]),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A run's call is named by the callable that started it and the
    /// helper, from its stable name.
    #[test]
    fn a_runs_call_is_named_by_its_starter_and_helper() {
        assert_eq!(
            call_words(
                "action stock.check_stock from query stock.products_to_check each action stock.check_product"
            ),
            ("check_stock".to_string(), "batches.each".to_string())
        );
        assert_eq!(
            call_words(
                "action catalogue.import_products reads csv ProductRow each action catalogue.save_product"
            ),
            ("import_products".to_string(), "csv.read".to_string())
        );
        assert_eq!(
            call_words(
                "action catalogue.export_products from query catalogue.products_for_export writes json_lines ProductExportRow #2"
            ),
            (
                "export_products #2".to_string(),
                "json_lines.write".to_string()
            )
        );
    }

    #[test]
    fn progress_says_how_far_against_the_total() {
        assert_eq!(progress_words(412, Some(1200), false), "412 of about 1200");
        assert_eq!(progress_words(21, Some(21), true), "21 of 21");
        assert_eq!(progress_words(3, None, false), "3");
    }
}
