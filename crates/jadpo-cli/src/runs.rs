//! `jadpo runs unfinished --build <build id>` (area 12, "Deploys"; area
//! 14; chunk 7.5c): run against production before a deploy, it lists the
//! unfinished runs a new build changes, so an operator can let them finish
//! first.
//!
//! - **The new build** is the checkout's: `--build` names the build being
//!   deployed, and the checkout must make that build, since its code is
//!   what the runs are compared with.
//! - **What it reads**, in every store that keeps them: the records of
//!   actions with several changing calls not abandoned
//!   (`jadpo_step_records`), and batch runs still going
//!   (`jadpo_batch_runs`). Never a request, an answer or a key; nothing of
//!   the app runs and none of its settings are read beyond its
//!   connections' addresses.
//! - **What changes.** A record started under another build whose
//!   action's step shape the new build changes stops as `diverged` when it
//!   runs again; one whose action no longer keeps steps would send them
//!   again; a batch run whose call the new build changes, or no longer
//!   has, fails. A record started under the new build, or whose shape is
//!   the same, resumes, as does a batch run whose call is the same.

use std::collections::BTreeMap;
use std::path::PathBuf;

use jadpo_diag::{Diagnostic, rule};
use jadpo_term::{Block, Cell, Fact, Json, Outcome, Screen, Symbol, Table};

use crate::database::Address;
use crate::run::{Call, Reply, quoted};
use crate::schedules::{clock, now, plural, store_error, url_of};

/// What `jadpo runs` does.
pub const ACTIONS: &[crate::table::Value] = &[crate::table::Value {
    name: "unfinished",
    description: "List the unfinished runs a new build changes",
}];

pub fn runs(call: &Call) -> Reply {
    match call.arguments.first().map(String::as_str) {
        Some("unfinished") => {
            if let Some(extra) = call.arguments.get(1) {
                return Reply::failed(arguments(
                    format!("`jadpo runs unfinished` takes no `{extra}`"),
                    "It lists every unfinished run the build changes; `--build` names the build."
                        .to_string(),
                    "Run `jadpo runs unfinished --build <build id>`".to_string(),
                ));
            }
            unfinished(call)
        }
        None => Reply::failed(arguments(
            "`jadpo runs` needs `unfinished`".to_string(),
            "`jadpo runs unfinished --build <build id>` lists the unfinished runs a new build changes, before it is deployed.".to_string(),
            "Run `jadpo runs unfinished --build <build id>`".to_string(),
        )),
        Some(other) => {
            let near = jadpo_diag::nearest(other, ACTIONS.iter().map(|action| action.name));
            Reply::failed(arguments(
                format!("`{other}` isn't something `jadpo runs` does"),
                match near {
                    Some(near) => format!(
                        "`jadpo runs unfinished` lists the unfinished runs a new build changes. Did you mean `{near}`?"
                    ),
                    None => "`jadpo runs unfinished` lists the unfinished runs a new build changes."
                        .to_string(),
                },
                "Run `jadpo runs unfinished --build <build id>`".to_string(),
            ))
        }
    }
}

fn arguments(message: String, reason: String, next: String) -> Diagnostic {
    Diagnostic::new(&rule::CLI_RUNS_ARGUMENTS, message)
        .reason(reason)
        .next(next)
}

/// What a run does under the new build.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Will {
    /// A record whose step shape the build changes: it stops as
    /// `diverged` before sending anything.
    Diverge,
    /// A record whose action the build no longer keeps steps for: a rerun
    /// would send its steps again.
    SendAgain,
    /// A batch run whose call the build changes or no longer has.
    Fail,
}

impl Will {
    fn word(self) -> &'static str {
        match self {
            Will::Diverge => "diverge",
            Will::SendAgain => "send_again",
            Will::Fail => "fail",
        }
    }

    fn cell(self) -> Cell {
        match self {
            Will::Diverge => Cell::state(Symbol::Decision, "stops as diverged"),
            Will::SendAgain => Cell::state(Symbol::Failure, "sends its steps again"),
            Will::Fail => Cell::state(Symbol::Failure, "fails"),
        }
    }
}

/// One unfinished run the build changes.
struct Changed {
    /// The id an operator acts on: a delivery's, else the record's or the
    /// batch run's.
    id: [u8; 16],
    kind: &'static str,
    /// The action, or the batch's starting callable, by its identity key.
    name: String,
    store: String,
    started_at: i64,
    will: Will,
}

/// A key without its kind and domain: `action billing.subscribe` is
/// `subscribe`.
fn bare(key: &str) -> String {
    let qualified = key.split_once(' ').map_or(key, |(_, rest)| rest);
    qualified
        .rsplit_once('.')
        .map_or(qualified, |(_, name)| name)
        .to_string()
}

fn unfinished(call: &Call) -> Reply {
    let Some(build) = call.value("--build").map(str::to_string) else {
        return Reply::failed(arguments(
            "`jadpo runs unfinished` needs `--build <build id>`".to_string(),
            "It compares the unfinished runs with the build about to be deployed, which the checkout makes.".to_string(),
            "Add `--build <build id>`".to_string(),
        ));
    };
    let cwd = call.cwd.clone().unwrap_or_else(|| PathBuf::from("."));
    let Some(root) = jadpo_db::find_root(&cwd) else {
        return crate::fmt::project_needed("runs");
    };
    let loaded = jadpo_db::load(&root);
    let prepared = match jadpo_interp::Prepared::load(&loaded.db, loaded.project) {
        Ok(prepared) => prepared,
        Err(_) => {
            return Reply::failed(arguments(
                "The project has errors, so the build it makes isn't known".to_string(),
                "`jadpo runs unfinished` compares the runs with the build the checkout makes, which needs the project to check cleanly.".to_string(),
                "Run `jadpo check`, fix what it reports, then run this again".to_string(),
            ));
        }
    };
    let plan = prepared.plan();
    if plan.build_id() != build {
        return Reply::failed(arguments(
            format!("The checkout makes build `{}`, not `{build}`", plan.build_id()),
            "The runs are compared with the checkout's code, so it must be the build being deployed.".to_string(),
            format!(
                "Check out the commit being deployed, or use `--build {}`",
                plan.build_id()
            ),
        ));
    }
    // The new build's step shape for each action that keeps steps, and
    // its batch calls' shapes.
    let shapes: BTreeMap<&str, &str> = plan
        .callables()
        .iter()
        .filter_map(|callable| {
            Some((
                callable.key.as_str(),
                callable.record.as_ref()?.shape.as_str(),
            ))
        })
        .collect();
    let batches: BTreeMap<&str, String> = plan
        .batches()
        .iter()
        .map(|batch| {
            (
                batch.name.as_str(),
                jadpo_runtime::batches::shape_digest(batch),
            )
        })
        .collect();
    let databases = match databases(call, &root, &loaded) {
        Ok(found) => found,
        Err(reply) => return reply,
    };
    let now = now(call);
    let mut changed: Vec<Changed> = Vec::new();
    let mut total = 0u64;
    for (store, url, keeps) in &databases {
        let keeps = *keeps;
        let read = with_database(store, url, |database| {
            Box::pin(async move {
                // A store keeps one of the two tables or both.
                let records = if keeps.steps {
                    jadpo_runtime::unfinished::unfinished(database).await?
                } else {
                    Vec::new()
                };
                let runs = if keeps.batches {
                    jadpo_runtime::batches::unfinished(database).await?
                } else {
                    Vec::new()
                };
                Ok((records, runs))
            })
        });
        let (records, runs) = match read {
            Ok(read) => read,
            Err(reply) => return reply,
        };
        total += (records.len() + runs.len()) as u64;
        for record in records {
            if record.build == build {
                continue;
            }
            let will = match shapes.get(record.action.as_str()) {
                Some(shape) if *shape == record.shape => continue,
                Some(_) => Will::Diverge,
                None => Will::SendAgain,
            };
            changed.push(Changed {
                id: record.delivery.unwrap_or(record.id),
                kind: if record.delivery.is_some() {
                    "delivery"
                } else {
                    "request"
                },
                name: record.action.clone(),
                store: store.clone(),
                started_at: record.started_at,
                will,
            });
        }
        for run in runs {
            if batches.get(run.batch.as_str()) == Some(&run.shape) {
                continue;
            }
            changed.push(Changed {
                id: run.id,
                kind: "batch",
                name: run
                    .batch
                    .split_once(" from ")
                    .map_or(run.batch.as_str(), |(start, _)| start)
                    .to_string(),
                store: store.clone(),
                started_at: run.started_at,
                will: Will::Fail,
            });
        }
    }
    changed.sort_by_key(|run| (run.started_at, run.id));
    let data = Json::object([
        ("build", Json::from(build.as_str())),
        (
            "unfinished",
            Json::from(i64::try_from(total).unwrap_or(i64::MAX)),
        ),
        (
            "changed",
            Json::array(changed.iter().map(|run| {
                Json::object([
                    (
                        "id",
                        Json::from(jadpo_runtime::deliveries::short(&run.id).as_str()),
                    ),
                    ("kind", Json::from(run.kind)),
                    ("name", Json::from(run.name.as_str())),
                    ("store", Json::from(run.store.as_str())),
                    (
                        "startedAt",
                        Json::from(jadpo_std::format_instant(run.started_at).as_str()),
                    ),
                    ("will", Json::from(run.will.word())),
                ])
            })),
        ),
    ]);
    let mut screen = Screen::new();
    let resume = total - changed.len() as u64;
    if changed.is_empty() {
        let headline = if total == 0 {
            "Nothing is unfinished".to_string()
        } else {
            "The build changes no unfinished run".to_string()
        };
        let facts = if total == 0 {
            Vec::new()
        } else {
            vec![Fact::plain(format!(
                "{} {}",
                plural(total, "unfinished run", "unfinished runs"),
                if total == 1 { "resumes" } else { "resume" }
            ))]
        };
        screen.push(Block::summary(Outcome::Success, headline, facts));
        return Reply::screen(screen, data);
    }
    let mut table = Table::new(["id", "run", "started (UTC)", "under this build"]).cut(1);
    for run in &changed {
        let what = match run.kind {
            "batch" => format!("batch from {}", bare(&run.name)),
            "request" => format!("request → {}", bare(&run.name)),
            _ => format!("steps of {}", bare(&run.name)),
        };
        table = table.row(vec![
            Cell::text(jadpo_runtime::deliveries::short(&run.id)),
            Cell::text(what),
            Cell::muted(clock(run.started_at, now)),
            run.will.cell(),
        ]);
    }
    screen.push(Block::Table(table));
    let mut facts = vec![Fact::plain(format!("{total} unfinished"))];
    if resume > 0 {
        facts.push(Fact::plain(format!(
            "{resume} {}",
            if resume == 1 { "resumes" } else { "resume" }
        )));
    }
    screen.push(
        Block::summary(
            Outcome::Failure,
            plural(changed.len() as u64, "run changes", "runs change"),
            facts,
        )
        .with_plain_headline("runs the build changes"),
    );
    let first = &changed[0];
    let id = jadpo_runtime::deliveries::short(&first.id);
    let (command, reason) = match first.kind {
        "batch" => (
            "jadpo runs unfinished --build ".to_string() + &build,
            "again once they have finished, before deploying",
        ),
        _ => (
            format!("jadpo deliveries show {id}"),
            "lists its steps, to let it finish before deploying",
        ),
    };
    screen.push(Block::hint(command, reason));
    Reply::screen(screen, data)
}

/// Which of the tables a store keeps.
#[derive(Clone, Copy, Debug)]
struct Keeps {
    steps: bool,
    batches: bool,
}

/// Each store keeping records of several changing calls or batch runs,
/// with its database's address and what it keeps: all of them, or with
/// `--url`, the one it names.
fn databases(
    call: &Call,
    root: &std::path::Path,
    loaded: &jadpo_db::Loaded,
) -> Result<Vec<(String, Address, Keeps)>, Reply> {
    let schema = jadpo_check::data::schema(&loaded.db, loaded.project);
    let kept: BTreeMap<String, Keeps> = schema
        .stores
        .iter()
        .map(|(name, store)| {
            (
                name.clone(),
                Keeps {
                    steps: store
                        .tables
                        .contains_key(jadpo_std::system::STEP_RECORDS.table),
                    batches: store
                        .tables
                        .contains_key(jadpo_std::system::BATCH_RUNS.table),
                },
            )
        })
        .filter(|(_, keeps)| keeps.steps || keeps.batches)
        .collect();
    let stores: Vec<String> = kept.keys().cloned().collect();
    if let Some(url) = call.value("--url") {
        let chosen = match (call.value("--store"), stores.as_slice()) {
            (Some(store), _) if stores.iter().any(|name| name == store) => store.to_string(),
            (Some(store), _) => {
                return Err(Reply::failed(arguments(
                    format!("`{store}` isn't a store that keeps unfinished runs"),
                    format!("They are kept in {}.", crate::run::both(&quoted(&stores))),
                    format!("Use {}", crate::run::either(&quoted(&stores))),
                )));
            }
            (None, [only]) => only.clone(),
            (None, []) => return Ok(Vec::new()),
            (None, _) => {
                return Err(Reply::failed(arguments(
                    "Which store is this database?".to_string(),
                    format!(
                        "Unfinished runs are kept in {}, and `--url` names one database.",
                        crate::run::both(&quoted(&stores))
                    ),
                    "Add `--store <name>`".to_string(),
                )));
            }
        };
        let keeps = kept[&chosen];
        let url = Address::of(call, loaded, &chosen, url.to_string())?;
        return Ok(vec![(chosen, url, keeps)]);
    }
    if let Some(store) = call.value("--store") {
        return Err(Reply::failed(arguments(
            format!("`--store {store}` goes with `--url`"),
            "Without `--url`, every store's database is read, each from its connection's setting."
                .to_string(),
            "Add `--url postgresql://…`, or leave out `--store`".to_string(),
        )));
    }
    let mut out = Vec::new();
    for store in stores {
        match url_of(call, root, loaded, &store)? {
            Some(url) => {
                let keeps = kept[&store];
                out.push((store, url, keeps));
            }
            None => {
                return Err(Reply::failed(arguments(
                    format!("`jadpo runs unfinished` needs the address of the database `{store}`"),
                    "No variable gives the address of its connection, and no `jadpo dev` is running with one: a connection reads its address from the setting its `url:` names (area 10), and `--url` names a database directly.".to_string(),
                    "Run `jadpo config set <the connection's setting>`, or add `--url postgresql://…`".to_string(),
                )));
            }
        }
    }
    Ok(out)
}

/// Runs `work` against the database at `url`; a failure to reach or read
/// it is `runs.unavailable`.
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
                &rule::RUNS_UNAVAILABLE,
                format!("The database `{store}` couldn't be read"),
            )
            .reason(format!(
                "Reading its unfinished runs failed: {error}. Its tables come with its migrations."
            ))
            .next("Check the database is running and its address is right"),
        )
    };
    let runtime = jadpo_host_native::Runtime::new().map_err(|error| failed(error.to_string()))?;
    let database = url.open().map_err(|error| failed(store_error(&error)))?;
    runtime
        .block_on(work(&database))
        .map_err(|error| failed(store_error(&error)))
}
