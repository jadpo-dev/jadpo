//! `jadpo seed <name>` (area 5, "Seeds"; area 13, "Seeds and fixtures";
//! area 14; chunk A5): runs the seed in `database/seeds/<name>.jadpo`
//! against the development database, so its data comes through the app's
//! own actions, with their validation, lifecycles and policy.
//!
//! The project is checked and its settings read as `jadpo dev` reads them.
//! Each store's database is the one `--url` names, else the address its
//! connection's setting gives (the environment over `.env.local`), else
//! the one a running `jadpo dev` started for it. Every one of them must be
//! on this machine (a Unix socket, `localhost`, `127.0.0.1` or `::1`):
//! seeds are data for your machine, and a database anywhere else may be
//! production, whose data comes from a migration backfill. Nothing is
//! reached until every store passes. Pending migrations are applied, as
//! `jadpo dev` applies them to its database.
//!
//! The seed runs in the interpreter as a request runs, at one time read
//! from the system clock, with the host's randomness: each action it
//! calls commits on its own, a call passing a caller runs as that caller
//! and one passing none runs without policy (area 13, "Calls with no
//! caller"). Mail lands in the dev inbox and provider calls are faked, as
//! under `jadpo dev`; what its actions emit is delivered before the
//! command ends, as a deployable delivers after answering. A failure or a
//! fault stops it, and what its earlier calls committed stays.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use jadpo_diag::{Diagnostic, Span, rule};
use jadpo_graph::Kind;
use jadpo_host_native::files::DiskFiles;
use jadpo_runtime::{Clock, Ctx, Env, Executor, Host, Outcome};
use jadpo_term::{Block, Fact, Json, Outcome as Shown, Screen};

use crate::database::Address;
use crate::run::{Call, Reply, quoted};

/// Where seeds live in a project (area 1).
pub const FOLDER: &str = "database/seeds";

pub fn seed(call: &Call) -> Reply {
    let cwd = call.cwd.clone().unwrap_or_else(|| PathBuf::from("."));
    let Some(root) = jadpo_db::find_root(&cwd) else {
        return crate::fmt::project_needed("seed");
    };
    let name = match named(&root, call.arguments.first()) {
        Ok(name) => name,
        Err(reply) => return reply,
    };
    // The project as `jadpo dev` runs it: checked, with its settings.
    let ready = match crate::dev::prepare(&root, &call.environment, true) {
        Ok(ready) => ready,
        Err((problems, sources)) => return blocked(&root, &cwd, problems, sources),
    };
    let path = format!("{FOLDER}/{name}.jadpo");
    let plan = ready.prepared.plan();
    let Some(id) = plan
        .callables()
        .iter()
        .position(|callable| callable.kind == Kind::Seed && callable.file == path)
    else {
        return Reply::failed(arguments(
            format!("`{path}` declares no seed"),
            "A seed's file holds one `seed { }` block, which `jadpo seed` runs.".to_string(),
            format!("Write `seed {{ … }}` in `{path}`"),
        ));
    };
    let id = id as u32;
    let addresses = match databases(call, &root, &ready) {
        Ok(addresses) => addresses,
        Err(reply) => return reply,
    };
    // Every store passes before any is reached.
    for (store, address) in &addresses {
        if let Err(reply) = on_this_machine(store, address) {
            return reply;
        }
    }
    let started = std::time::Instant::now();
    crate::run::say(&format!("Seeding `{name}`"));
    let migrated = match migrate(&addresses, &ready) {
        Ok(migrated) => migrated,
        Err(reply) => return reply,
    };
    let ran = run(&root, &ready, &addresses, id);
    let ms = i64::try_from(started.elapsed().as_millis()).unwrap_or(i64::MAX);
    let stores: Vec<Json> = addresses
        .iter()
        .map(|(store, address)| {
            Json::object([
                ("store", Json::from(store.as_str())),
                ("database", Json::from(shown_address(&address.url).as_str())),
            ])
        })
        .collect();
    let data = |outcome: &str| {
        Json::object([
            ("seed", Json::from(name.as_str())),
            ("outcome", Json::from(outcome)),
            ("stores", Json::array(stores.clone())),
            (
                "migrationsApplied",
                Json::from(migrated.iter().map(|(_, count)| *count as i64).sum::<i64>()),
            ),
            ("ms", Json::from(ms)),
        ])
    };
    match ran {
        Ok(None) => {
            let mut screen = Screen::new();
            for (store, count) in &migrated {
                screen.push(Block::Status {
                    outcome: Shown::Success,
                    text: format!(
                        "Applied {} to `{store}`",
                        crate::schedules::plural(*count as u64, "migration", "migrations")
                    ),
                    rows: Vec::new(),
                });
            }
            screen.push(Block::Status {
                outcome: Shown::Success,
                text: format!("Seeded `{name}` in {}", crate::schedules::duration(ms)),
                rows: addresses
                    .iter()
                    .map(|(store, address)| (store.clone(), shown_address(&address.url)))
                    .collect(),
            });
            Reply::screen(screen, data("succeeded"))
        }
        Ok(Some(stopped)) => {
            let diagnostic = stopped.diagnostic(&name);
            let screen = Screen::new()
                .with(Block::diagnostic_from(
                    &diagnostic,
                    &stopped.sources,
                    jadpo_db::shown_root(&cwd, &root).as_deref(),
                ))
                .with(Block::summary(
                    Shown::Failure,
                    "seed failed",
                    // Its time only when it is slow, as `jadpo check` gives it.
                    [Fact::plain(
                        if started.elapsed() >= crate::check::SHOW_TIME_FROM {
                            format!("`{name}` in {:.1} s", started.elapsed().as_secs_f64())
                        } else {
                            format!("`{name}`")
                        },
                    )],
                ));
            Reply::Report {
                screen,
                data: data("failed"),
                diagnostics: vec![diagnostic],
                sources: stopped.sources,
            }
        }
        Err(error) => Reply::failed(unavailable(
            format!("`{name}` couldn't be run"),
            format!("Reaching its databases failed: {error}."),
        )),
    }
}

/// The seeds in `database/seeds/`, by file name, sorted: what a name
/// may be, and what an unknown one is told.
pub(crate) fn names(root: &Path) -> Vec<String> {
    let mut out: Vec<String> = std::fs::read_dir(root.join(FOLDER))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let file = entry.file_name().into_string().ok()?;
            let stem = file.strip_suffix(".jadpo")?;
            (!stem.ends_with(".test")).then(|| stem.to_string())
        })
        .collect();
    out.sort();
    out
}

/// The seed `name` names, or why it names none.
fn named(root: &Path, name: Option<&String>) -> Result<String, Reply> {
    let names = names(root);
    let listed = || {
        if names.is_empty() {
            "The project has no seeds: each is a file in `database/seeds/`.".to_string()
        } else {
            format!(
                "The project's seeds are {}, in `database/seeds/`.",
                crate::run::both(&quoted(&names))
            )
        }
    };
    let write_one = "Write one in `database/seeds/<name>.jadpo`, as `seed { … }`";
    let Some(name) = name else {
        return Err(Reply::failed(arguments(
            "`jadpo seed` needs the seed to run".to_string(),
            listed(),
            match names.first() {
                Some(first) => format!("Run `jadpo seed {first}`"),
                None => write_one.to_string(),
            },
        )));
    };
    // A path to the file, as Tab in a shell completes one, names it too.
    let bare = name
        .strip_prefix(&format!("{FOLDER}/"))
        .unwrap_or(name)
        .strip_suffix(".jadpo")
        .unwrap_or(name.strip_prefix(&format!("{FOLDER}/")).unwrap_or(name));
    if names.iter().any(|known| known == bare) {
        return Ok(bare.to_string());
    }
    let next = match jadpo_diag::nearest(bare, names.iter().map(String::as_str)) {
        Some(near) => format!("Use `{near}` instead of `{bare}`"),
        None if names.is_empty() => write_one.to_string(),
        None => format!("Use {}", crate::run::either(&quoted(&names))),
    };
    Err(Reply::failed(arguments(
        format!("`{bare}` isn't a seed"),
        listed(),
        next,
    )))
}

fn arguments(message: String, reason: String, next: String) -> Diagnostic {
    Diagnostic::new(&rule::CLI_SEED_ARGUMENTS, message)
        .reason(reason)
        .next(next)
}

fn unavailable(message: String, reason: String) -> Diagnostic {
    Diagnostic::new(&rule::SEED_UNAVAILABLE, message)
        .reason(reason)
        .next("Check the database is running, then run `jadpo seed` again")
}

/// The project's errors, drawn as `jadpo check` draws them.
fn blocked(
    root: &Path,
    cwd: &Path,
    problems: Vec<Diagnostic>,
    sources: BTreeMap<String, String>,
) -> Reply {
    let shown = jadpo_db::shown_root(cwd, root);
    let mut screen = Screen::new();
    for problem in &problems {
        screen.push(Block::diagnostic_from(problem, &sources, shown.as_deref()));
    }
    let errors = problems.len();
    screen.push(Block::summary(
        Shown::Failure,
        "seed stopped",
        [Fact::problem(crate::schedules::plural(
            errors as u64,
            "error",
            "errors",
        ))],
    ));
    Reply::Report {
        screen,
        data: Json::object([("errors", Json::from(errors as i64))]),
        diagnostics: problems,
        sources,
    }
}

/// Each store's database: `--url` for a project with one store, else the
/// address its connection's setting gives, else the one a running `jadpo
/// dev` started for it; each secured as its connection says.
fn databases(
    call: &Call,
    root: &Path,
    ready: &crate::dev::Ready,
) -> Result<BTreeMap<String, Address>, Reply> {
    let secured = crate::database::securities(ready.prepared.plan().connections(), &ready.config);
    let address = |store: &str, url: String| Address {
        url,
        security: secured.get(store).cloned().unwrap_or_default(),
    };
    let mut out = BTreeMap::new();
    if let Some(url) = call.value("--url") {
        if ready.stores.len() > 1 {
            return Err(Reply::failed(arguments(
                "`--url` names one database, and the project has several stores".to_string(),
                format!(
                    "The stores {} each need a database, and each connection takes its address from a setting: `url: config.<name>` in `app.jadpo`, with the value in `.env.local`.",
                    crate::run::both(&quoted(&ready.stores))
                ),
                "Leave out `--url`, and give each connection's setting its address with `jadpo config set`".to_string(),
            )));
        }
        for store in &ready.stores {
            out.insert(store.clone(), address(store, url.to_string()));
        }
        return Ok(out);
    }
    let mut dev = crate::schedules::dev_stores(root);
    for store in &ready.stores {
        let url = ready
            .configured
            .get(store)
            .cloned()
            .or_else(|| dev.remove(store));
        let Some(url) = url else {
            return Err(Reply::failed(arguments(
                format!("`jadpo seed` needs the address of the database `{store}`"),
                "No variable gives the address of its connection, and no `jadpo dev` is running with one: a connection reads its address from the setting its `url:` names (area 10), and `--url` names a database directly.".to_string(),
                "Run `jadpo config set <the connection's setting>`, start `jadpo dev`, or add `--url postgresql://localhost/…`".to_string(),
            )));
        };
        out.insert(store.clone(), address(store, url));
    }
    Ok(out)
}

/// Refuses `address` unless every server it names is on this machine
/// (area 5, "Seeds": a seed refuses a production database).
fn on_this_machine(store: &str, address: &Address) -> Result<(), Reply> {
    let database = address.open().map_err(|error| {
        Reply::failed(unavailable(
            format!("The address of the database `{store}` can't be read"),
            format!(
                "Reading it failed: {}.",
                crate::schedules::store_error(&error)
            ),
        ))
    })?;
    let elsewhere: Vec<String> = database
        .endpoints()
        .iter()
        .filter_map(|endpoint| match endpoint {
            jadpo_drivers::Endpoint::Tcp { host, .. }
                if !jadpo_std::database_tls::is_this_machine(host) =>
            {
                Some(host.clone())
            }
            _ => None,
        })
        .collect();
    let Some(host) = elsewhere.first() else {
        return Ok(());
    };
    Err(Reply::failed(
        Diagnostic::new(
            &rule::SEED_PRODUCTION_REFUSED,
            format!("The database `{store}` at {host} isn't on this machine"),
        )
        .reason(
            "A seed is data for development, so `jadpo seed` runs only against a database on this machine: a Unix socket, `localhost`, `127.0.0.1` or `::1`. One anywhere else may be production, whose data comes from a migration backfill. Nothing was run.",
        )
        .next_for(
            "Give the connection's setting a database on this machine with `jadpo config set`, or start `jadpo dev`, which starts one",
            jadpo_diag::Decider::Human,
        ),
    ))
}

/// `postgresql://ada:secret@localhost:5432/library?sslmode=…` as
/// `localhost:5432/library`: where the data went, never a credential. A
/// socket named in the query (`postgresql://ada@/library?host=/tmp`), and
/// libpq's `host=… dbname=…` form, give `/tmp/library`.
pub(crate) fn shown_address(url: &str) -> String {
    let pairs: Vec<(&str, &str)>;
    let mut place = "";
    let mut name = "";
    match url.split_once("://") {
        Some((_, rest)) => {
            let rest = rest.split('#').next().unwrap_or(rest);
            let (path, query) = rest.split_once('?').unwrap_or((rest, ""));
            let path = path.rsplit_once('@').map_or(path, |(_, after)| after);
            let (authority, database) = path.split_once('/').unwrap_or((path, ""));
            place = authority;
            name = database;
            pairs = query
                .split('&')
                .filter_map(|pair| pair.split_once('='))
                .collect();
        }
        None => {
            pairs = url
                .split_whitespace()
                .filter_map(|pair| pair.split_once('='))
                .collect()
        }
    }
    for (key, value) in pairs {
        let value = value.trim_matches('\'');
        match key {
            "host" if place.is_empty() => place = value,
            "dbname" if name.is_empty() => name = value,
            _ => {}
        }
    }
    if place.is_empty() {
        place = "localhost";
    }
    format!("{place}/{name}")
}

/// Applies each store's pending migrations; how many each took, for the
/// stores that took any.
fn migrate(
    addresses: &BTreeMap<String, Address>,
    ready: &crate::dev::Ready,
) -> Result<Vec<(String, usize)>, Reply> {
    let program = jadpo_interp::Program::new(Arc::clone(&ready.prepared));
    crate::migrate::with_backfills(Some(&program), |backfills| {
        let runtime = jadpo_host_native::Runtime::new().map_err(|error| {
            Reply::failed(unavailable(
                "The runtime couldn't start".to_string(),
                format!("Starting it failed: {error}."),
            ))
        })?;
        let mut out = Vec::new();
        for (store, address) in addresses {
            let Some(migrations) = ready.steps.get(store) else {
                continue;
            };
            let database = address.open().map_err(|error| {
                Reply::failed(unavailable(
                    format!("The database `{store}` can't be reached"),
                    format!(
                        "Connecting failed: {}.",
                        crate::schedules::store_error(&error)
                    ),
                ))
            })?;
            let done = runtime
                .block_on(jadpo_runtime::migrate_with(
                    &database,
                    migrations,
                    backfills,
                    &mut |_| std::ops::ControlFlow::Continue(()),
                ))
                .map_err(|error| {
                    Reply::failed(match error {
                        // Not reached at all: the seed's own error, whose
                        // next step is the seed again (area 5, "Seeds").
                        jadpo_runtime::MigrateError::Store(
                            jadpo_runtime::StoreError::Unavailable,
                        ) => unavailable(
                            format!("The database `{store}` can't be reached"),
                            "Connecting to it failed, so nothing was run.".to_string(),
                        ),
                        other => crate::migrate::migrate_failed(store, other),
                    })
                })?;
            if !done.is_empty() {
                out.push((store.clone(), done.len()));
            }
        }
        Ok(out)
    })
}

/// Where a seed stopped: a file's path, its bytes, and the caret's words.
type At = (String, Option<(u32, u32)>, String);

/// Why a seed stopped, as its report has it.
struct Stopped {
    /// `failed` or `fault`.
    word: &'static str,
    subject: String,
    /// Where, in its file: its path, its bytes and the caret's words.
    at: Option<At>,
    /// The report's other lines, by label.
    fields: Vec<(&'static str, Vec<String>)>,
    sources: BTreeMap<String, String>,
}

impl Stopped {
    /// The report as one diagnostic: its sentences as the reason, the
    /// place it stopped as the frame, and what stays said last.
    fn diagnostic(&self, name: &str) -> Diagnostic {
        let field = |label: &str| {
            self.fields
                .iter()
                .find(|(has, _)| *has == label)
                .map(|(_, lines)| lines.join(" "))
        };
        let mut said: Vec<String> = Vec::new();
        if let Some(reason) = field("reason") {
            said.push(reason);
        }
        // A failure's code, kind and message, as a client would get them.
        if let Some(code) = field("code") {
            let kind = field("kind").map_or_else(String::new, |kind| format!(", {kind}"));
            let message =
                field("message").map_or_else(String::new, |message| format!(": \"{message}\""));
            let stop = if message.ends_with(".\"") { "" } else { "." };
            said.push(format!("Its code is `{code}`{kind}{message}{stop}"));
        }
        // An assertion's values.
        for label in ["expected", "actual", "left", "right", "compared"] {
            if let Some(value) = field(label) {
                said.push(format!("{label}: {value}."));
            }
        }
        if let Some(path) = field("path") {
            said.push(format!("The calls were {path}."));
        }
        said.push("What the seed's earlier calls committed stays in the database.".to_string());
        let subject = match self.word {
            "failed" => self.subject.clone(),
            word => format!("{word} {}", self.subject),
        };
        let next = field("next")
            .unwrap_or_else(|| format!("Fix what it reports, then run `jadpo seed {name}` again"));
        let mut diagnostic = Diagnostic::new(
            &rule::SEED_FAILED,
            format!("Seed `{name}` stopped with {subject}"),
        )
        .reason(said.join(" "))
        .next(next);
        if let Some((path, Some((start, end)), label)) = &self.at {
            diagnostic = diagnostic.at(Span::new(path.clone(), *start, *end), label.clone());
        }
        diagnostic
    }
}

/// Runs seed `id` against `addresses` in the interpreter, on a stack large
/// enough for it, then what its actions set off: `None` when it ran to its
/// end, else why it stopped.
fn run(
    root: &Path,
    ready: &crate::dev::Ready,
    addresses: &BTreeMap<String, Address>,
    id: u32,
) -> Result<Option<Stopped>, String> {
    let prepared = Arc::clone(&ready.prepared);
    let config = Arc::clone(&ready.config);
    let unset = ready.unset.clone();
    let addresses = addresses.clone();
    let root = root.to_path_buf();
    let ran = jadpo_interp::on_large_stack(move || -> Result<Option<Stopped>, String> {
        let program = jadpo_interp::Program::new(Arc::clone(&prepared)).with_config(&config);
        let runtime = jadpo_host_native::Runtime::new().map_err(|error| error.to_string())?;
        // Each connection's driver, as `jadpo dev` builds them; files on
        // this machine's disk, under `.jadpo/files/`.
        let drivers = jadpo_host_native::Drivers::build(
            program.runtime_plan().connections,
            program.started(),
            &jadpo_host_native::supplies(),
        );
        let file_stores: Vec<String> = jadpo_std::connections::files(prepared.plan().connections())
            .map(|(name, _, _)| name.to_string())
            .collect();
        let mut native = jadpo_host_native::NativeHost::new()
            .with_drivers(drivers)
            .with_files(
                DiskFiles::under(root.join(crate::dev::DEV_FILES)),
                move |connection| file_stores.iter().any(|name| name == connection),
            );
        for (store, address) in &addresses {
            let database = address
                .open()
                .map_err(|error| crate::schedules::store_error(&error))?;
            native = native.with_store(store.clone(), database);
        }
        let now = native
            .now_ms()
            .ok_or_else(|| "the system clock can't be read".to_string())?;
        // Mail caught in the dev inbox, and every provider connection's
        // calls faked, as `jadpo dev` does them (chunks 7.2d and 7.3c2).
        let setup = crate::dev_calls::setup(
            &prepared,
            &config,
            &unset,
            &crate::dev_calls::Session::default(),
        )
        .map_err(|problem| problem.message.clone())?;
        let fakes: Rc<dyn jadpo_runtime::capability::Capability> = Rc::new(
            crate::dev_calls::DevProviders::new(&root, Arc::new(Mutex::new(setup))),
        );
        let catcher: Rc<dyn jadpo_runtime::capability::Capability> =
            Rc::new(crate::dev_inbox::Catcher {
                root: root.clone(),
                lock: Arc::new(Mutex::new(())),
                caught: Arc::new(|_: &crate::dev_inbox::Mail| {}),
                other: Some(Rc::clone(&fakes)),
                spam: Rc::new(crate::spam::SystemSpamAssassin),
            });
        let plan = prepared.plan();
        let mail = plan.events().emails.connection.clone();
        let mut host = jadpo_runtime::capability::Faked::new(native);
        if let Some(mail) = &mail {
            host = host.with(mail, Rc::clone(&catcher));
        }
        for (_, connection, _) in jadpo_std::connections::packages(plan.connections()) {
            if Some(connection) != mail.as_deref() {
                host = host.with(connection, Rc::clone(&fakes));
            }
        }
        let env = Env {
            seed: None,
            clock: Clock::fixed(now),
        };
        let mut ctx = Ctx::of(&program, &host, env);
        let outcome = runtime.block_on(program.run(&mut ctx, id, Vec::new()));
        let deferred = ctx.take_deferred();
        drop(ctx);
        if !deferred.is_empty() {
            runtime.block_on(jadpo_runtime::runner::run_deferred(
                &program, &host, deferred,
            ));
        }
        Ok(stopped(&program, &outcome))
    });
    ran.map_err(|error| error.to_string())?
}

/// Why `outcome` stopped the seed, or `None` when it ran to its end.
fn stopped(program: &jadpo_interp::Program, outcome: &Outcome) -> Option<Stopped> {
    let report = program.report(outcome)?;
    let at = report
        .location
        .as_ref()
        .map(|location| (location.path.clone(), location.span, location.label.clone()));
    let sources = program
        .plan()
        .files()
        .iter()
        .map(|(path, text)| (path.clone(), text.clone()))
        .collect();
    Some(Stopped {
        word: report.word,
        subject: report.subject.clone(),
        at,
        fields: report.fields().into_iter().collect(),
        sources,
    })
}

#[cfg(test)]
mod tests {
    use super::shown_address;

    #[test]
    fn an_address_is_shown_without_its_credentials() {
        assert_eq!(
            shown_address("postgresql://ada:secret@localhost:5432/library?sslmode=disable"),
            "localhost:5432/library"
        );
        assert_eq!(
            shown_address("postgres://localhost/library"),
            "localhost/library"
        );
        assert_eq!(
            shown_address("host=/tmp dbname=library user=ada password=secret"),
            "/tmp/library"
        );
        assert_eq!(
            shown_address("postgresql://ada@/library?host=/tmp&port=5433"),
            "/tmp/library"
        );
    }
}
