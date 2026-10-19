//! `jadpo dev` (area 14): runs the application in the interpreter and
//! serves its routes on a local port, checking again on every save and
//! swapping in the new version when it checks cleanly; while a save has
//! errors, the last good version keeps serving. Each request it answers is
//! one line, with its request id (area 17, "The request timeline").
//!
//! The settings `config.jadpo` declares are read first, from the process's
//! environment over `.env.local`, and every one is checked against its type:
//! a missing or invalid value stops the app starting, naming the setting and
//! its variable and never the value, and a save of `.env.local` is checked
//! like a save of source (area 10, "Checking values before the app
//! starts"). Each database store opens at the address its connection's
//! `url:` setting holds; `--url` names the database instead, and a store
//! whose address no variable gives gets a database on a disposable
//! PostgreSQL started for the session, as `jadpo test` starts one; its
//! pending migrations are applied when it starts and after each save that
//! adds one. With `--no-run` it only checks on every save and writes
//! `build/`.
//!
//! The screen follows the CLI design language: a command header, a status
//! line saying where it listens, one line per request (id, method and
//! path, status in its colour, the failure code, the time), a muted rule
//! for each save, and errors drawn as `jadpo check` draws them. With
//! `--json`, every step is one envelope on one line.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::io::Write;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use jadpo_diag::{Diagnostic, rule};
use jadpo_host_native::http::{Handler, MakeHandler, Server};
use jadpo_host_native::{NativeHost, Runtime, postgres};
use jadpo_interp::{Prepared, Program};
use jadpo_runtime::config::{ConfigValues, Mode, Variables};
use jadpo_runtime::http::{Request, Response, Worker, serve_request};
use jadpo_runtime::{Host, LocalFuture};
use jadpo_term::{
    Block, ColorChoice, Environment, Fact, Json, Outcome, Screen, Stream, Style, Token, envelope,
};
use jadpo_test::watch::{EditKind, Poller};

use crate::run::{Call, Reply, Session};

/// The port `jadpo dev` listens on unless `--port` says otherwise.
pub const DEFAULT_PORT: u16 = 3000;

/// What `jadpo dev` was asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DevArgs {
    pub cwd: PathBuf,
    pub port: u16,
    pub url: Option<String>,
    pub no_run: bool,
    pub json: bool,
    pub choice: ColorChoice,
}

pub fn dev(call: &Call) -> Reply {
    let port = match call.value("--port") {
        None => DEFAULT_PORT,
        Some(written) => match written.parse::<u16>() {
            Ok(port) => port,
            Err(_) => {
                return Reply::failed(
                    Diagnostic::new(
                        &rule::CLI_DEV_PORT_INVALID,
                        format!("`{written}` isn't a port"),
                    )
                    .reason("A port is a whole number from 0 to 65535; 0 lets the system choose a free one.")
                    .next(format!("Use a port such as `--port {DEFAULT_PORT}`")),
                );
            }
        },
    };
    let cwd = call.cwd.clone().unwrap_or_else(|| PathBuf::from("."));
    if jadpo_db::find_root(&cwd).is_none() {
        return crate::fmt::project_not_found("dev");
    }
    Reply::Session(Session::Dev(Box::new(DevArgs {
        cwd,
        port,
        url: call.value("--url").map(str::to_string),
        no_run: call.flag("--no-run"),
        json: false,
        choice: ColorChoice::Auto,
    })))
}

/// One request answered, for its line.
struct Line {
    id: String,
    method: String,
    path: String,
    status: u16,
    code: Option<String>,
    route: Option<String>,
    millis: u128,
    /// The address the request counted as (area 10, "Client addresses").
    address: Option<String>,
    /// The fault behind a 500: what it was and where, innermost first.
    fault: Option<(String, Vec<String>)>,
}

/// What every worker reads: the version being served and where its
/// stores are.
struct Shared {
    current: Mutex<(Arc<Prepared>, Arc<ConfigValues>)>,
    generation: AtomicU64,
    urls: BTreeMap<String, String>,
    log: Mutex<Sender<Line>>,
}

/// A worker's handler: its view of the program, rebuilt when a save
/// swaps in a new version, its host (its stores and rate-limit counters,
/// and the work each response leaves for after it is sent, which runs on
/// the version that left it) and what the runtime keeps between its
/// requests.
struct DevHandler {
    shared: Arc<Shared>,
    program: RefCell<Option<(u64, Rc<Program>)>>,
    host: Rc<NativeHost>,
    worker: Worker,
}

impl DevHandler {
    fn program(&self) -> Rc<Program> {
        let generation = self.shared.generation.load(Ordering::SeqCst);
        if let Some((seen, program)) = &*self.program.borrow()
            && *seen == generation
        {
            return Rc::clone(program);
        }
        let (prepared, values) = match self.shared.current.lock() {
            Ok(current) => (Arc::clone(&current.0), Arc::clone(&current.1)),
            Err(poisoned) => {
                let current = poisoned.into_inner();
                (Arc::clone(&current.0), Arc::clone(&current.1))
            }
        };
        let program = Rc::new(Program::new(prepared).with_config(&values));
        *self.program.borrow_mut() = Some((generation, Rc::clone(&program)));
        program
    }
}

impl Handler for DevHandler {
    fn handle(&self, request: Request) -> LocalFuture<'_, Response> {
        Box::pin(async move {
            let started = Instant::now();
            let program = self.program();
            let app = program.app();
            let mut response = serve_request(
                &app,
                program.as_ref(),
                self.host.as_ref(),
                &self.worker,
                &request,
            )
            .await;
            // What the response left runs after it is sent (chunk 6.1a).
            jadpo_runtime::runner::after_response(
                Rc::clone(&program) as Rc<dyn jadpo_runtime::Executor>,
                Rc::clone(&self.host) as Rc<dyn Host>,
                std::mem::take(&mut response.deferred),
            );
            if let Ok(log) = self.shared.log.lock() {
                let _ = log.send(Line {
                    id: response
                        .header("x-request-id")
                        .unwrap_or_default()
                        .to_string(),
                    method: request.method.clone(),
                    path: request.path.clone(),
                    status: response.status,
                    code: response.code.clone(),
                    route: response.route.clone(),
                    millis: started.elapsed().as_millis(),
                    address: response.client.clone(),
                    fault: response.fault.as_ref().map(|fault| {
                        (
                            fault.kind.classification().to_string(),
                            fault.stack(program.places()),
                        )
                    }),
                });
            }
            response
        })
    }

    fn take_deferred(&self) -> Vec<LocalFuture<'static, ()>> {
        self.host.take_deferred()
    }
}

/// Writes to the session's streams.
struct Out<'a> {
    out: &'a mut dyn Write,
    err: &'a mut dyn Write,
    style: Style,
    err_style: Style,
    json: bool,
}

impl Out<'_> {
    fn screen(&mut self, screen: Screen) {
        let _ = self.out.write_all(screen.render(&self.style).as_bytes());
        let _ = self.out.flush();
    }

    fn json(&mut self, data: Json, diagnostics: &[Diagnostic], sources: &BTreeMap<String, String>) {
        let _ = self.out.write_all(
            envelope(Some("dev"), Some(data), diagnostics, sources)
                .to_line()
                .as_bytes(),
        );
        let _ = self.out.flush();
    }

    fn paint(&self, token: Token, text: &str) -> String {
        match self.style.color {
            Some(depth) => format!("{}{text}\x1b[0m", token.sgr(depth, self.style.theme)),
            None => text.to_string(),
        }
    }

    fn line(&mut self, line: &Line) {
        if self.json {
            let data = Json::object([
                ("event", "request".into()),
                ("requestId", Json::from(line.id.as_str())),
                ("method", Json::from(line.method.as_str())),
                ("path", Json::from(line.path.as_str())),
                (
                    "route",
                    line.route.as_deref().map_or(Json::Null, Json::from),
                ),
                ("status", Json::from(line.status as usize)),
                ("code", line.code.as_deref().map_or(Json::Null, Json::from)),
                ("ms", Json::from(line.millis as usize)),
                (
                    "address",
                    line.address.as_deref().map_or(Json::Null, Json::from),
                ),
                (
                    "fault",
                    line.fault.as_ref().map_or(Json::Null, |(kind, frames)| {
                        Json::object([
                            ("classification", Json::from(kind.as_str())),
                            ("frames", Json::array(frames.iter().map(String::as_str))),
                        ])
                    }),
                ),
            ]);
            self.json(data, &[], &BTreeMap::new());
            return;
        }
        let short_id: String = line.id.chars().take(8).collect();
        let request = format!("{} {}", line.method, shorten(&line.path));
        let token = match line.status {
            200..=399 => Token::Success,
            400..=499 => Token::Warning,
            _ => Token::Failure,
        };
        let status = self.paint(token, &line.status.to_string());
        let code = line
            .code
            .as_deref()
            .map_or_else(String::new, |code| format!(" {code}"));
        let time = self.paint(Token::Muted, &format!("{} ms", line.millis));
        let address = line.address.as_deref().map_or_else(String::new, |address| {
            format!("   {}", self.paint(Token::Muted, address))
        });
        let mut text = format!("{short_id}  {request:<34} {status}{code}   {time}{address}\n");
        if let Some((kind, frames)) = &line.fault {
            text.push_str(&format!(
                "  {} {kind}\n",
                self.paint(Token::Failure, "fault")
            ));
            for frame in frames {
                text.push_str(&format!("    {}\n", self.paint(Token::Muted, frame)));
            }
        }
        let _ = self.out.write_all(text.as_bytes());
        let _ = self.out.flush();
    }
}

/// A path as a request line shows it: a long segment, such as an id, cut
/// to its first four characters (`/todos/0192…/complete`).
fn shorten(path: &str) -> String {
    path.split('/')
        .map(|segment| {
            if segment.chars().count() > 12 {
                let head: String = segment.chars().take(4).collect();
                format!("{head}…")
            } else {
                segment.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// Runs the session on the process's streams until it is stopped with
/// ctrl-c; the exit code when it can't start.
pub fn serve(args: &DevArgs, env: &Environment) -> u8 {
    serve_to(
        args,
        env,
        &mut std::io::stdout(),
        &mut std::io::stderr(),
        &|| false,
    )
}

/// [`serve`] on any streams until `stop` says so.
pub fn serve_to(
    args: &DevArgs,
    env: &Environment,
    out: &mut dyn Write,
    err: &mut dyn Write,
    stop: &dyn Fn() -> bool,
) -> u8 {
    let mut out = Out {
        out,
        err,
        style: Style::detect(env, args.json, args.choice, Stream::Stdout),
        err_style: Style::detect(env, args.json, args.choice, Stream::Stderr),
        json: args.json,
    };
    let Some(root) = jadpo_db::find_root(&args.cwd) else {
        return 1;
    };
    let started = crate::watch::time_now();
    if !args.json {
        out.screen(Screen::new().with(Block::header("jadpo dev", [format!("started {started}")])));
    }
    // The first version that checks cleanly; until then, what's wrong.
    let mut poller = Poller::new(&root, true).watching(crate::local::FILE);
    let first = loop {
        match prepare(&root, env, !args.no_run) {
            Ok(ready) => break ready,
            Err((problems, sources)) => {
                report_errors(&mut out, &root, &args.cwd, &problems, &sources, false);
                if poller.wait(stop).is_none() {
                    return 1;
                }
            }
        }
    };
    if args.no_run {
        report_ready(&mut out, None, &first, Databases::Disposable);
        return watch_only(&mut out, &root, &args.cwd, env, &mut poller, stop);
    }
    // Every request is answered under the system clock, so without one
    // the server doesn't start, rather than serve as though it were 1970
    // (chunk 5.8a).
    if NativeHost::new().now_ms().is_none() {
        report_failure(
            &mut out,
            Diagnostic::new(&rule::CLI_DEV_START_FAILED, "The system clock can't be read")
                .reason("Every request reads the time, and the clock reads before 1970, so the server won't answer with a wrong one.")
                .next("Set the system clock, then run `jadpo dev` again"),
        );
        return 1;
    }
    // The stores, and their migrations applied.
    let mut disposable = None;
    let urls = match stores(&first, args.url.as_deref(), &mut disposable) {
        Ok(urls) => urls,
        Err(problem) => {
            report_failure(&mut out, *problem);
            return 1;
        }
    };
    let mut applied = match migrate_all(&urls, &first.steps, &first.prepared) {
        Ok(applied) => applied,
        Err(problem) => {
            report_failure(&mut out, *problem);
            return 1;
        }
    };
    // The listener.
    let listener = match TcpListener::bind(("127.0.0.1", args.port)) {
        Ok(listener) => listener,
        Err(error) => {
            report_failure(
                &mut out,
                Diagnostic::new(
                    &rule::CLI_DEV_START_FAILED,
                    format!("Port {} can't be listened on", args.port),
                )
                .reason(format!("Listening failed: {error}."))
                .next(format!(
                    "Stop what is using port {}, or choose another with `--port`",
                    args.port
                )),
            );
            return 1;
        }
    };
    let port = listener
        .local_addr()
        .map_or(args.port, |address| address.port());
    let (sender, receiver): (Sender<Line>, Receiver<Line>) = channel();
    let shared = Arc::new(Shared {
        current: Mutex::new((Arc::clone(&first.prepared), Arc::clone(&first.config))),
        generation: AtomicU64::new(0),
        urls: urls.clone(),
        log: Mutex::new(sender),
    });
    let make_shared = Arc::clone(&shared);
    let make: MakeHandler = Arc::new(move || {
        let mut host = NativeHost::new();
        for (store, url) in &make_shared.urls {
            if let Ok(database) = postgres(url) {
                host = host.with_store(store.clone(), database);
            }
        }
        Box::new(DevHandler {
            shared: Arc::clone(&make_shared),
            program: RefCell::new(None),
            host: Rc::new(host),
            worker: Worker::default(),
        }) as Box<dyn Handler>
    });
    let workers = std::thread::available_parallelism().map_or(2, |n| n.get().min(8));
    let server: Server = match jadpo_host_native::http::serve(listener, workers, make) {
        Ok(server) => server,
        Err(error) => {
            report_failure(
                &mut out,
                Diagnostic::new(&rule::CLI_DEV_START_FAILED, "The server couldn't start")
                    .reason(format!("Starting it failed: {error}."))
                    .next("Run `jadpo dev` again"),
            );
            return 1;
        }
    };
    let url = format!("http://localhost:{port}");
    let databases = match (
        args.url.is_some(),
        disposable.is_some(),
        first.configured.is_empty(),
    ) {
        (true, ..) => Databases::Url,
        (false, true, false) => Databases::Mixed,
        (false, true, true) => Databases::Disposable,
        (false, false, _) => Databases::Configured,
    };
    report_ready(&mut out, Some(&url), &first, databases);
    let mut current = first;
    // Request lines are written as they come, between looks for saves.
    loop {
        let drain = |out: &mut Out<'_>| {
            while let Ok(line) = receiver.try_recv() {
                out.line(&line);
            }
        };
        let edits = {
            let out_cell = RefCell::new(&mut out);
            poller.wait(&|| {
                if let Ok(mut out) = out_cell.try_borrow_mut() {
                    drain(&mut out);
                }
                stop()
            })
        };
        drain(&mut out);
        let Some(edits) = edits else {
            server.stop();
            return 0;
        };
        let time = crate::watch::time_now();
        let saved: Vec<String> = edits
            .iter()
            .map(|edit| {
                format!(
                    "{} {}",
                    match edit.kind {
                        EditKind::Added => "added",
                        EditKind::Saved => "saved",
                        EditKind::Removed => "removed",
                    },
                    edit.path
                )
            })
            .collect();
        match prepare(&root, env, true) {
            Err((problems, sources)) => {
                rule_line(&mut out, &time, &saved, "not reloaded");
                report_errors(&mut out, &root, &args.cwd, &problems, &sources, true);
            }
            Ok(ready) => {
                if ready.steps != current.steps {
                    match migrate_all(&urls, &ready.steps, &ready.prepared) {
                        Ok(ran) => applied += ran,
                        Err(problem) => {
                            rule_line(&mut out, &time, &saved, "not reloaded");
                            report_failure(&mut out, *problem);
                            continue;
                        }
                    }
                }
                if let Ok(mut slot) = shared.current.lock() {
                    *slot = (Arc::clone(&ready.prepared), Arc::clone(&ready.config));
                }
                shared.generation.fetch_add(1, Ordering::SeqCst);
                rule_line(&mut out, &time, &saved, "reloaded");
                if out.json {
                    let data = Json::object([
                        ("event", "reloaded".into()),
                        ("routes", Json::from(ready.routes)),
                        ("migrationsApplied", Json::from(applied)),
                    ]);
                    out.json(data, &[], &BTreeMap::new());
                }
                current = ready;
            }
        }
    }
}

/// `--no-run`: checks on every save and writes `build/`, serving nothing.
fn watch_only(
    out: &mut Out<'_>,
    root: &Path,
    cwd: &Path,
    env: &Environment,
    poller: &mut Poller,
    stop: &dyn Fn() -> bool,
) -> u8 {
    while let Some(edits) = poller.wait(stop) {
        let time = crate::watch::time_now();
        let saved: Vec<String> = edits
            .iter()
            .map(|edit| format!("saved {}", edit.path))
            .collect();
        match prepare(root, env, false) {
            Ok(_) => rule_line(out, &time, &saved, "checked"),
            Err((problems, sources)) => {
                rule_line(out, &time, &saved, "errors");
                report_errors(out, root, cwd, &problems, &sources, false);
            }
        }
    }
    0
}

/// A version of the project ready to serve.
struct Ready {
    prepared: Arc<Prepared>,
    /// Each store's migrations.
    steps: BTreeMap<String, Vec<jadpo_std::Migration>>,
    /// The stores its entities live in.
    stores: Vec<String>,
    routes: usize,
    /// The settings it starts with (area 10).
    config: Arc<ConfigValues>,
    /// The address each database store's connection reads, where a
    /// variable gives one.
    configured: BTreeMap<String, String>,
}

/// Loads and checks the project, writes `build/`, and prepares it to run;
/// the problems that stop it otherwise.
fn prepare(
    root: &Path,
    env: &Environment,
    run: bool,
) -> Result<Ready, (Vec<Diagnostic>, BTreeMap<String, String>)> {
    let loaded = jadpo_db::load(root);
    let (db, project) = (&loaded.db, loaded.project);
    let diagnostics = jadpo_check::check(db, project);
    let errors = diagnostics.len();
    let graph = jadpo_check::graph::graph(db, project);
    let openapi = crate::derived::openapi(db, project, &crate::check::title(root));
    let permissions = jadpo_check::policy::policy_clean(db, project)
        .then(|| jadpo_check::policy::report::report(db, project));
    let _ = crate::derived::write(root, graph, errors, &openapi, permissions.as_ref());
    let blocking: Vec<Diagnostic> = diagnostics
        .into_iter()
        .filter(jadpo_check::blocks_running)
        .collect();
    if !blocking.is_empty() {
        let sources = jadpo_db::source_texts(db, project);
        return Err((blocking, sources));
    }
    let set = jadpo_check::data::migrations::migrations(db, project);
    let steps = jadpo_check::data::migrations::store_steps(set);
    let model = jadpo_check::data::data_model(db, project);
    let mut stores: Vec<String> = model
        .entities
        .iter()
        .filter_map(|entity| entity.store.clone())
        .collect();
    stores.extend(steps.keys().cloned());
    stores.sort();
    stores.dedup();
    // With the latest migration's backfills worked out in Jadpo, which
    // applying its migrations runs (chunk 5.6b2).
    let prepared = Prepared::checked_with_backfills(db, project);
    let routes = prepared.plan().routes().routes.len();
    let (config, configured) = if run {
        let model = jadpo_check::config::config(db, project);
        let types = |field: &str| {
            model
                .setting(field)
                .map_or_else(String::new, |setting| setting.ty.to_string())
        };
        start_config(&prepared, root, env, &stores, &types)
            .map_err(|problems| (problems, BTreeMap::new()))?
    } else {
        (Arc::new(ConfigValues::default()), BTreeMap::new())
    };
    Ok(Ready {
        prepared,
        steps,
        stores,
        routes,
        config,
        configured,
    })
}

/// The settings the app starts with and the address each database store's
/// connection reads, where a variable gives one.
type StartConfig = (Arc<ConfigValues>, BTreeMap<String, String>);

/// The settings the app starts with, every one checked: from the
/// environment over `.env.local`. A database store's address that no
/// variable gives is not a problem here: `jadpo dev` gives that store a
/// disposable database, which is what a project with a fresh clone wants
/// (area 14); any other missing or invalid value is, and stops the app
/// starting.
fn start_config(
    prepared: &Prepared,
    root: &Path,
    env: &Environment,
    stores: &[String],
    types: &dyn Fn(&str) -> String,
) -> Result<StartConfig, Vec<Diagnostic>> {
    let plan = prepared.plan().config();
    if plan.is_empty() {
        return Ok((Arc::new(ConfigValues::default()), BTreeMap::new()));
    }
    let layered = crate::config::Layered::new(root, env).map_err(|problem| vec![*problem])?;
    // The addresses of the stores this project uses that a variable doesn't
    // give: a placeholder stands in so the rest can be checked, and the
    // store gets a database of its own.
    let wanted: Vec<_> = plan
        .connections
        .iter()
        .filter(|connection| stores.contains(&connection.connection))
        .filter_map(|connection| {
            let field = plan.fields.get(connection.field as usize)?;
            let missing = field.default.is_none() && layered.variable(&field.variable).is_none();
            Some((
                connection.connection.clone(),
                field.variable.clone(),
                missing,
            ))
        })
        .collect();
    let placeholders: BTreeMap<String, String> = wanted
        .iter()
        .filter(|(_, _, missing)| *missing)
        .map(|(_, variable, _)| {
            (
                variable.clone(),
                "postgres://disposable.invalid/dev".to_string(),
            )
        })
        .collect();
    let stand_in = |name: &str| {
        layered
            .variable(name)
            .or_else(|| placeholders.get(name).cloned())
    };
    let values = prepared
        .config(&stand_in, Mode::Environment, &crate::config::random)
        .map_err(|problems| crate::config::start_diagnostics(&problems, types))?;
    let configured = wanted
        .iter()
        .filter(|(_, _, missing)| !*missing)
        .filter_map(|(store, ..)| {
            let connection = plan
                .connections
                .iter()
                .find(|connection| &connection.connection == store)?;
            let text = values.text_at(connection.field)?;
            Some((store.clone(), text.to_string()))
        })
        .collect();
    Ok((Arc::new(values), configured))
}

/// Each store's database URL: `--url` for the one store, else the address
/// its connection's setting holds, else a database of its own on a
/// disposable server.
fn stores(
    ready: &Ready,
    url: Option<&str>,
    disposable: &mut Option<jadpo_test::postgres::Server>,
) -> Result<BTreeMap<String, String>, Box<Diagnostic>> {
    let mut out = BTreeMap::new();
    if ready.stores.is_empty() {
        return Ok(out);
    }
    if let Some(url) = url {
        if ready.stores.len() > 1 {
            return Err(Box::new(
                Diagnostic::new(
                    &rule::CLI_DEV_ARGUMENTS,
                    "`--url` names one database, and the project has several stores",
                )
                .reason(format!(
                    "The stores {} each need a database, and each connection takes its address from a setting: `url: config.<name>` in `app.jadpo`, with the value in `.env.local`.",
                    crate::run::both(&ready.stores)
                ))
                .next("Leave out `--url`, and give each connection a `url:`; stores with none get a disposable PostgreSQL"),
            ));
        }
        for store in &ready.stores {
            out.insert(store.clone(), url.to_string());
        }
        return Ok(out);
    }
    for store in &ready.stores {
        if let Some(configured) = ready.configured.get(store) {
            out.insert(store.clone(), configured.clone());
        }
    }
    let wanting: Vec<&String> = ready
        .stores
        .iter()
        .filter(|store| !out.contains_key(*store))
        .collect();
    if wanting.is_empty() {
        return Ok(out);
    }
    let server = jadpo_test::postgres::Server::start().map_err(|error| {
        Box::new(
            Diagnostic::new(&rule::CLI_DEV_START_FAILED, "No PostgreSQL to run the app on")
                .reason(format!(
                    "`jadpo dev` starts a disposable PostgreSQL for a store whose address no setting gives and `--url` doesn't name, and starting one failed: {error}."
                ))
                .next("Install PostgreSQL, set the connection's address with `jadpo config set`, or name a database with `--url`"),
        )
    })?;
    for store in wanting {
        let url = server
            .create_database(&format!("dev_{store}"))
            .map_err(|error| {
                Box::new(
                    Diagnostic::new(
                        &rule::CLI_DEV_START_FAILED,
                        format!("No database for store `{store}`"),
                    )
                    .reason(format!("Creating it failed: {error}."))
                    .next("Run `jadpo dev` again"),
                )
            })?;
        out.insert(store.clone(), url);
    }
    *disposable = Some(server);
    Ok(out)
}

/// Applies each store's pending migrations, working out in Jadpo with
/// `prepared` a backfill that has no SQL form (chunk 5.6b2); how many ran.
fn migrate_all(
    urls: &BTreeMap<String, String>,
    steps: &BTreeMap<String, Vec<jadpo_std::Migration>>,
    prepared: &Arc<Prepared>,
) -> Result<usize, Box<Diagnostic>> {
    let program = Program::new(Arc::clone(prepared));
    crate::migrate::with_backfills(Some(&program), |backfills| {
        migrate_stores(urls, steps, backfills)
    })
}

/// Applies each store's pending migrations with `backfills`.
fn migrate_stores(
    urls: &BTreeMap<String, String>,
    steps: &BTreeMap<String, Vec<jadpo_std::Migration>>,
    backfills: &dyn jadpo_runtime::Backfills,
) -> Result<usize, Box<Diagnostic>> {
    let runtime = Runtime::new().map_err(|error| {
        Box::new(
            Diagnostic::new(&rule::CLI_DEV_START_FAILED, "The runtime couldn't start")
                .reason(format!("Starting it failed: {error}."))
                .next("Run `jadpo dev` again"),
        )
    })?;
    let mut ran = 0;
    for (store, url) in urls {
        let Some(migrations) = steps.get(store) else {
            continue;
        };
        let database = postgres(url).map_err(|error| {
            Box::new(
                Diagnostic::new(
                    &rule::CLI_DEV_START_FAILED,
                    format!("Store `{store}` can't be reached"),
                )
                .reason(format!("Connecting failed: {error:?}."))
                .next("Check the database URL, then run `jadpo dev` again"),
            )
        })?;
        let done = runtime
            .block_on(jadpo_runtime::migrate_with(
                &database,
                migrations,
                backfills,
                &mut |_| std::ops::ControlFlow::Continue(()),
            ))
            .map_err(|error| Box::new(crate::migrate::migrate_failed(store, error)))?;
        // The start-up check: the store has every extension its
        // migrations created, pgvector's among them (area 21).
        runtime
            .block_on(jadpo_runtime::has_extensions(&database, migrations))
            .map_err(|error| Box::new(crate::migrate::migrate_failed(store, error)))?;
        ran += done.len();
    }
    Ok(ran)
}

/// Where the databases the app runs on came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Databases {
    /// The one `--url` names.
    Url,
    /// The addresses the connections' settings hold.
    Configured,
    /// A disposable PostgreSQL, for every store.
    Disposable,
    /// Some stores configured, the rest on a disposable PostgreSQL.
    Mixed,
}

impl Databases {
    fn word(self) -> &'static str {
        match self {
            Databases::Url => "url",
            Databases::Configured => "config",
            Databases::Disposable => "disposable",
            Databases::Mixed => "mixed",
        }
    }

    fn describe(self) -> &'static str {
        match self {
            Databases::Url => "the one --url names",
            Databases::Configured => "from the connections' settings",
            Databases::Disposable => "disposable PostgreSQL, gone when dev stops",
            Databases::Mixed => "from settings, and a disposable PostgreSQL for the rest",
        }
    }
}

fn report_ready(out: &mut Out<'_>, url: Option<&str>, ready: &Ready, databases: Databases) {
    if out.json {
        let data = Json::object([
            ("event", "listening".into()),
            ("url", url.map_or(Json::Null, Json::from)),
            ("routes", Json::from(ready.routes)),
            (
                "database",
                if ready.stores.is_empty() {
                    Json::Null
                } else {
                    databases.word().into()
                },
            ),
            ("settings", Json::from(ready.config.len())),
        ]);
        out.json(data, &[], &BTreeMap::new());
        return;
    }
    let routes = if ready.routes == 1 {
        "1 route".to_string()
    } else {
        format!("{} routes", ready.routes)
    };
    let mut rows = vec![("routes".to_string(), routes)];
    if !ready.config.is_empty() {
        rows.push((
            "settings".to_string(),
            format!("{} checked", ready.config.len()),
        ));
    }
    if !ready.stores.is_empty() && url.is_some() {
        rows.push(("database".to_string(), databases.describe().to_string()));
    }
    let text = match url {
        Some(url) => format!("listening on {url}"),
        None => "checked; serving nothing (--no-run)".to_string(),
    };
    out.screen(Screen::new().with(Block::Status {
        outcome: Outcome::Success,
        text,
        rows,
    }));
    let keys = Screen::new().with(Block::KeyBar {
        label: "serving".to_string(),
        facts: vec![Fact::plain("saves reload")],
        keys: vec![("ctrl-c".to_string(), "quit".to_string())],
    });
    let _ = out.err.write_all(keys.render(&out.err_style).as_bytes());
    let _ = out.err.flush();
}

fn rule_line(out: &mut Out<'_>, time: &str, saved: &[String], what: &str) {
    if out.json {
        return;
    }
    let mut long = vec![time.to_string()];
    long.extend(saved.iter().cloned());
    long.push(what.to_string());
    let short = vec![
        time.to_string(),
        format!("{} files changed", saved.len()),
        what.to_string(),
    ];
    out.screen(Screen::new().with(Block::Rule {
        long,
        short,
        changes: Vec::new(),
        plain: "dev".to_string(),
    }));
}

fn report_errors(
    out: &mut Out<'_>,
    root: &Path,
    cwd: &Path,
    problems: &[Diagnostic],
    sources: &BTreeMap<String, String>,
    serving: bool,
) {
    if out.json {
        let data = Json::object([("event", "errors".into()), ("serving", serving.into())]);
        out.json(data, problems, sources);
        return;
    }
    let shown = jadpo_db::shown_root(cwd, root);
    let mut screen = Screen::new();
    for problem in problems {
        screen.push(Block::diagnostic_from(problem, sources, shown.as_deref()));
    }
    let errors = problems.len();
    let mut facts = vec![Fact::problem(format!(
        "{errors} {}",
        if errors == 1 { "error" } else { "errors" }
    ))];
    facts.push(Fact::plain(if serving {
        "still serving the last good version"
    } else {
        "waiting for a save that fixes them"
    }));
    screen.push(Block::summary(
        Outcome::Failure,
        "not running this version",
        facts,
    ));
    out.screen(screen);
}

fn report_failure(out: &mut Out<'_>, problem: Diagnostic) {
    if out.json {
        out.json(
            Json::object([("event", "stopped".into())]),
            &[problem],
            &BTreeMap::new(),
        );
        return;
    }
    let screen = Screen::new().with(Block::diagnostic(
        &problem,
        &BTreeMap::<String, String>::new(),
    ));
    let _ = out.err.write_all(screen.render(&out.err_style).as_bytes());
    let _ = out.err.flush();
}
