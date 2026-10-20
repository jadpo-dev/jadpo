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

use jadpo_diag::{Diagnostic, rule};
use jadpo_host_native::files::DiskFiles;
use jadpo_host_native::http::{AppHandler, Handler, MakeHandler, Server};
use jadpo_host_native::{NativeHost, Runtime, postgres};
use jadpo_interp::{Prepared, Program};
use jadpo_runtime::Host;
use jadpo_runtime::config::{ConfigValues, Mode, Variables};
use jadpo_runtime::schedules::Ticked;
use jadpo_runtime::serve::{Answered, Application, Observer};
use jadpo_term::{
    Block, ColorChoice, Environment, Fact, Json, Outcome, Screen, Stream, Style, Token, envelope,
};
use jadpo_test::watch::{EditKind, Poller};

use crate::run::{Call, Reply, Session};

/// Where `jadpo dev` keeps every store of files, a folder each (area 16,
/// "Files"; chunk 6.4b).
pub(crate) const DEV_FILES: &str = ".jadpo/files";

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
    /// `--fake <connection>`: keeps all its provider calls faked for this
    /// session (chunk 7.2d).
    pub fake: Vec<String>,
    /// `--sandbox <operation>`: lets it through to the sandbox for this
    /// session.
    pub sandbox: Vec<String>,
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
        fake: call.all("--fake").into_iter().map(str::to_string).collect(),
        sandbox: call
            .all("--sandbox")
            .into_iter()
            .map(str::to_string)
            .collect(),
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
    /// The calls it made outside the process (chunk 7.2d).
    calls: Vec<jadpo_runtime::capability::Logged>,
    /// Why a sign-in it answered was refused (chunk 7.3b4).
    refused: Option<jadpo_runtime::SignInRefused>,
    /// The steps its action reached under a record (chunk 7.5f).
    steps: Vec<jadpo_runtime::steps::RunSteps>,
}

/// What the session prints, as it happens: a request answered, a
/// schedule's run or skipped tick (area 12, "Watching schedules"), or an
/// alert that has just started.
enum Event {
    Request(Line),
    Schedule {
        name: String,
        due: jadpo_runtime::schedules::Due,
    },
    Alert {
        alert: jadpo_runtime::schedules::watch::Alert,
        now: i64,
    },
    /// A mail the inbox caught (chunk 7.3c2): its id, its email and its
    /// address, never its subject or words, which may hold a sign-in code.
    Mail {
        id: String,
        email: String,
        to: String,
    },
    /// A delivery whose action reached steps under a record, as an attempt
    /// at it ended (chunk 7.5f).
    Delivery(StepsLine),
}

/// A delivery's attempt with steps, for its line (chunk 7.5f).
struct StepsLine {
    id: [u8; 16],
    subscriber: String,
    state: String,
    steps: Vec<jadpo_runtime::steps::RunSteps>,
}

/// `jadpo dev`'s call log: each call kept in the log file, and each
/// delivery whose action reached steps under a record sent to the screen
/// (chunk 7.5f).
struct DevCalls {
    log: crate::dev_calls::DevLog,
    shared: Arc<Shared>,
}

impl jadpo_runtime::call_log::CallLog for DevCalls {
    fn seen(&self, seen: &jadpo_runtime::call_log::Seen<'_>) {
        self.log.seen(seen);
    }

    fn attempt_ended(&self, attempt: &jadpo_runtime::call_log::DeliveryAttempt<'_>) {
        if let Ok(log) = self.shared.log.lock() {
            let _ = log.send(Event::Delivery(StepsLine {
                id: attempt.delivery,
                subscriber: attempt.subscriber.to_string(),
                state: attempt.state.to_string(),
                steps: attempt.steps.to_vec(),
            }));
        }
    }
}

/// What every worker reads: the version being served and where its
/// stores are.
struct Shared {
    current: Mutex<(Arc<Prepared>, Arc<ConfigValues>)>,
    generation: AtomicU64,
    urls: BTreeMap<String, String>,
    log: Mutex<Sender<Event>>,
}

/// A worker's view of the program: the version it last served, rebuilt
/// when a save swaps in a new one.
struct Current {
    shared: Arc<Shared>,
    program: RefCell<Option<(u64, Rc<Program>)>>,
}

impl Current {
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

/// What `jadpo dev` hears from the runtime's server handler (chunk 6.9),
/// which serves the application as a deployable does: a line for each
/// request it answers, each schedule's run or skipped tick, and each alert
/// the tick raised.
struct Lines {
    shared: Arc<Shared>,
    /// The schedules whose paused skip was printed, so a paused one prints
    /// once, not on every tick, until it runs again.
    paused: RefCell<std::collections::BTreeSet<String>>,
}

impl Lines {
    fn send(&self, events: Vec<Event>) {
        if let Ok(log) = self.shared.log.lock() {
            for event in events {
                let _ = log.send(event);
            }
        }
    }
}

impl Observer for Lines {
    fn answered(&self, answered: &Answered<'_>) {
        let (request, response) = (answered.request, answered.response);
        self.send(vec![Event::Request(Line {
            id: response
                .header("x-request-id")
                .unwrap_or_default()
                .to_string(),
            method: request.method.clone(),
            path: request.path.clone(),
            status: response.status,
            code: response.code.clone(),
            route: response.route.clone(),
            millis: u128::from(answered.ms),
            address: response.client.clone(),
            fault: response.fault.as_ref().map(|fault| {
                (
                    fault.kind.classification().to_string(),
                    fault.stack(answered.program.places()),
                )
            }),
            calls: response.calls.clone(),
            refused: response.refused.clone(),
            steps: response.steps.clone(),
        })]);
    }

    /// A line for each run, and for each alert the tick raised (chunk
    /// 6.2b; raised by the tick since chunk 6.9).
    fn ticked(&self, program: &dyn Application, ticked: &Ticked, now: i64) {
        let Some(plan) = program.schedules() else {
            return;
        };
        let name = |key: &str| {
            plan.schedule(key)
                .map_or_else(|| key.to_string(), |schedule| schedule.name.clone())
        };
        let mut events = Vec::new();
        for due in &ticked.due {
            // A paused schedule's skipped ticks print once.
            let skipped_paused = matches!(
                due.ran,
                jadpo_runtime::schedules::Ran::Skipped(jadpo_runtime::schedules::Skipped::Paused)
            );
            if skipped_paused && !self.paused.borrow_mut().insert(due.schedule.clone()) {
                continue;
            }
            if !skipped_paused {
                self.paused.borrow_mut().remove(&due.schedule);
            }
            events.push(Event::Schedule {
                name: name(&due.schedule),
                due: due.clone(),
            });
        }
        for alert in &ticked.raised {
            events.push(Event::Alert {
                alert: alert.clone(),
                now,
            });
        }
        self.send(events);
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

    fn event(&mut self, event: &Event) {
        match event {
            Event::Request(line) => self.line(line),
            Event::Schedule { name, due } => self.schedule(name, due),
            Event::Mail { id, email, to } => self.mail(id, email, to),
            Event::Alert { alert, now } => {
                let (text, data) = crate::schedules::dev_alert(alert, *now);
                if self.json {
                    self.json(data, &[], &BTreeMap::new());
                } else {
                    self.screen(Screen::new().with(Block::Alerts(vec![text])));
                }
            }
            Event::Delivery(line) => self.delivery(line),
        }
    }

    /// A delivery whose action reached steps under a record (chunk 7.5f):
    /// its id where a request has its request id, the subscriber, how the
    /// attempt left it as a symbol and a word in its colour, then each
    /// step, and the command that shows it when it needs an operator.
    fn delivery(&mut self, line: &StepsLine) {
        let id = jadpo_runtime::deliveries::short(&line.id);
        let (symbol, token, word) = attempt_state(&line.state);
        if self.json {
            let data = Json::object([
                ("event", "delivery".into()),
                (
                    "delivery",
                    Json::from(jadpo_value::encoding::to_hex(&line.id)),
                ),
                ("subscriber", Json::from(line.subscriber.as_str())),
                ("state", Json::from(line.state.as_str())),
                ("steps", steps_json(&line.steps)),
            ]);
            self.json(data, &[], &BTreeMap::new());
            return;
        }
        let state = if self.style.mode == jadpo_term::Mode::Rich {
            self.paint(
                token,
                &format!("{} {word}", symbol.text(self.style.symbols)),
            )
        } else {
            self.paint(token, word)
        };
        let label = format!("subscriber {}", line.subscriber);
        let mut text = format!("{id}  {label:<34} {state}\n");
        let paint = |token: Token, text: &str| self.paint(token, text);
        for run in &line.steps {
            for shown in &run.steps {
                text.push_str(&step_line(shown, &paint));
            }
        }
        if matches!(
            line.state.as_str(),
            "failed" | "outcome_unknown" | "diverged"
        ) {
            let command = format!("jadpo deliveries show {id}");
            text.push_str(&if self.style.mode == jadpo_term::Mode::Rich {
                format!(
                    "  {}\n",
                    self.paint(
                        Token::Accent,
                        &format!(
                            "{} {command}",
                            jadpo_term::Symbol::Next.text(self.style.symbols)
                        )
                    )
                )
            } else {
                format!("  next: {command}\n")
            });
        }
        let _ = self.out.write_all(text.as_bytes());
        let _ = self.out.flush();
    }

    /// A schedule's run: its trace's id where a request has its request
    /// id, the schedule where a request has its method and path, its
    /// outcome as a symbol and a word in its colour where a request has
    /// its status, what it emitted or why it failed, then its time.
    fn schedule(&mut self, name: &str, due: &jadpo_runtime::schedules::Due) {
        use jadpo_runtime::schedules::Ran;
        let (detail, data) = crate::schedules::dev_line(name, due);
        if self.json {
            self.json(data, &[], &BTreeMap::new());
            return;
        }
        let id: String = due
            .trace
            .as_deref()
            .and_then(|trace| trace.get(3..11))
            .map_or_else(|| " ".repeat(8), str::to_string);
        let (symbol, token, word) = match due.ran {
            Ran::Succeeded { .. } => (jadpo_term::Symbol::Success, Token::Success, "succeeded"),
            Ran::Failed { .. } => (jadpo_term::Symbol::Failure, Token::Failure, "failed"),
            Ran::Skipped(_) => (jadpo_term::Symbol::Skipped, Token::Muted, "skipped"),
        };
        let state = if self.style.mode == jadpo_term::Mode::Rich {
            self.paint(
                token,
                &format!("{} {word}", symbol.text(self.style.symbols)),
            )
        } else {
            self.paint(token, word)
        };
        let label = format!("schedule {name}");
        let time = match due.ran {
            Ran::Skipped(_) => String::new(),
            _ => format!(
                "   {}",
                self.paint(
                    Token::Muted,
                    &crate::schedules::dev_duration(due.duration_ms())
                )
            ),
        };
        let text = format!("{id}  {label:<34} {state} {detail}{time}\n");
        let _ = self.out.write_all(text.as_bytes());
        let _ = self.out.flush();
    }

    /// A caught mail: its id where a request has its request id, its
    /// email where a request has its method and path, then where it went.
    fn mail(&mut self, id: &str, email: &str, to: &str) {
        if self.json {
            let data = Json::object([
                ("event", "mail".into()),
                ("id", Json::from(id)),
                ("email", Json::from(email)),
                ("to", Json::from(to)),
            ]);
            self.json(data, &[], &BTreeMap::new());
            return;
        }
        let state = if self.style.mode == jadpo_term::Mode::Rich {
            self.paint(
                Token::Success,
                &format!(
                    "{} caught",
                    jadpo_term::Symbol::Success.text(self.style.symbols)
                ),
            )
        } else {
            self.paint(Token::Success, "caught")
        };
        let label = format!("email {email}");
        let to = self.paint(Token::Muted, &format!("to {to}"));
        let text = format!("{id:<8}  {label:<34} {state}   {to}\n");
        let _ = self.out.write_all(text.as_bytes());
        let _ = self.out.flush();
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
                ("steps", steps_json(&line.steps)),
                (
                    "calls",
                    Json::array(line.calls.iter().map(|call| {
                        Json::object([
                            (
                                "call",
                                Json::from(format!("{}.{}", call.connection, call.operation)),
                            ),
                            ("source", Json::from(call.source.word())),
                            ("ended", Json::from(ended_word(&call.ended))),
                        ])
                    })),
                ),
                (
                    "refused",
                    line.refused.as_ref().map_or(Json::Null, |refused| {
                        Json::object([
                            ("reason", Json::from(refused.reason)),
                            (
                                "failure",
                                refused.failure.as_deref().map_or(Json::Null, Json::from),
                            ),
                        ])
                    }),
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
        // Each call it made, as a muted line of its trace (chunk 7.2d),
        // only a failed one in its colour; a step's call as its step, below.
        for call in line.calls.iter().filter(|call| call.step.is_none()) {
            let ended = ended_word(&call.ended);
            let word = match &call.ended {
                jadpo_runtime::capability::Ended::Answered => self.paint(Token::Muted, &ended),
                _ => self.paint(Token::Failure, &ended),
            };
            text.push_str(&format!(
                "  {} {}\n",
                self.paint(
                    Token::Muted,
                    &format!(
                        "└─ {}.{} · {} ·",
                        call.connection,
                        call.operation,
                        call.source.word()
                    )
                ),
                word
            ));
        }
        // Each step its action reached under a record, with what became of
        // it (chunk 7.5f).
        for run in &line.steps {
            for shown in &run.steps {
                text.push_str(&step_line(shown, &|token, text| self.paint(token, text)));
            }
        }
        // Why a sign-in was refused, which the person was never told
        // (area 8, "Events"): a muted line of its trace, the reason in
        // the colour of the refusal's status.
        if let Some(refused) = &line.refused {
            text.push_str(&refusal_line(refused, &|token, text| {
                self.paint(token, text)
            }));
        }
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

/// A sign-in refusal's line under its request: `└─ sign-in refused ·
/// refused_by_app · SignUpClosed`, the reason painted as a warning.
fn refusal_line(
    refused: &jadpo_runtime::SignInRefused,
    paint: &dyn Fn(Token, &str) -> String,
) -> String {
    let failure = refused
        .failure
        .as_deref()
        .map_or_else(String::new, |failure| {
            format!(" {}", paint(Token::Muted, &format!("· {failure}")))
        });
    format!(
        "  {} {}{failure}\n",
        paint(Token::Muted, "└─ sign-in refused ·"),
        paint(Token::Warning, refused.reason),
    )
}

/// A step under its request or delivery (chunk 7.5f): `└─ step 2 ·
/// pay.subscriptions.create · generated · in doubt`, muted but for what
/// went wrong, in its colour. Its place, operation, where its answer came
/// from and what became of it; never its request, answer or key.
fn step_line(shown: &jadpo_runtime::steps::Shown, paint: &dyn Fn(Token, &str) -> String) -> String {
    use jadpo_runtime::capability::Source;
    use jadpo_runtime::steps::StepState;
    let mut trace = format!("└─ step {} · {} ·", shown.position, shown.operation);
    if let Some(source) = shown.source.filter(|source| *source != Source::Replayed)
        && shown.state != StepState::Replayed
    {
        trace.push_str(&format!(" {} ·", source.word()));
    }
    let word = match &shown.state {
        StepState::Replayed => paint(Token::Muted, "replayed"),
        StepState::Answered => paint(Token::Muted, "answered"),
        StepState::Failed(code) => paint(Token::Failure, code),
        StepState::InDoubt => paint(Token::Warning, "in doubt"),
        StepState::Diverged => paint(Token::Decision, "diverged"),
        StepState::OutcomeUnknown => paint(Token::Decision, "outcome unknown"),
    };
    format!("  {} {word}\n", paint(Token::Muted, &trace))
}

/// The steps runs reached, as `--json` gives them (chunk 7.5f).
fn steps_json(runs: &[jadpo_runtime::steps::RunSteps]) -> Json {
    Json::array(runs.iter().flat_map(|run| {
        run.steps.iter().map(|shown| {
            Json::object([
                ("action", Json::from(run.action.as_str())),
                ("step", Json::from(shown.position as usize)),
                ("operation", Json::from(shown.operation.as_str())),
                ("state", Json::from(shown.state.word())),
                (
                    "code",
                    match &shown.state {
                        jadpo_runtime::steps::StepState::Failed(code) => Json::from(code.as_str()),
                        _ => Json::Null,
                    },
                ),
                (
                    "source",
                    shown
                        .source
                        .map_or(Json::Null, |source| Json::from(source.word())),
                ),
            ])
        })
    }))
}

/// How a delivery's line says the state an attempt left it in: its
/// symbol, colour and word (chunk 7.5f).
fn attempt_state(state: &str) -> (jadpo_term::Symbol, Token, &'static str) {
    use jadpo_term::Symbol;
    match state {
        "succeeded" => (Symbol::Success, Token::Success, "succeeded"),
        "failed" => (Symbol::Failure, Token::Failure, "failed"),
        "outcome_unknown" => (Symbol::Decision, Token::Decision, "outcome unknown"),
        "diverged" => (Symbol::Decision, Token::Decision, "diverged"),
        // Tried again after a wait, or given back to run again at once.
        "pending" => (Symbol::Pending, Token::Warning, "retrying"),
        _ => (Symbol::Pending, Token::Muted, "pending"),
    }
}

/// How a call ended, as a request line says it.
fn ended_word(ended: &jadpo_runtime::capability::Ended) -> String {
    use jadpo_runtime::capability::Ended;
    match ended {
        Ended::InFlight => "in flight".to_string(),
        Ended::Answered => "answered".to_string(),
        Ended::Failed(code) => code.clone(),
        Ended::Unavailable => "unavailable".to_string(),
        Ended::Unknown => "outcome unknown".to_string(),
        Ended::Misconfigured => "misconfigured".to_string(),
        Ended::Refused => "refused".to_string(),
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
/// ctrl-c or SIGTERM, when the server drains, its background work giving
/// back what it holds (chunk 6.7); the exit code when it can't start. A
/// second signal ends it at once.
pub fn serve(args: &DevArgs, env: &Environment) -> u8 {
    let stop = jadpo_host_native::stop_signals().ok();
    serve_to(
        args,
        env,
        &mut std::io::stdout(),
        &mut std::io::stderr(),
        &|| {
            stop.as_ref()
                .is_some_and(|stop| stop.load(Ordering::SeqCst))
        },
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
        report_ready(&mut out, None, &first, Databases::Disposable, None, None);
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
    // What each provider connection's calls do (chunk 7.2d): faked, or
    // let through to the sandbox, never live.
    let session = crate::dev_calls::Session {
        fake: args.fake.clone(),
        sandbox: args.sandbox.clone(),
    };
    let setup =
        match crate::dev_calls::setup(&first.prepared, &first.config, &first.unset, &session) {
            Ok(setup) => Arc::new(Mutex::new(setup)),
            Err(problem) => {
                report_failure(&mut out, *problem);
                return 1;
            }
        };
    let call_log = Arc::new(Mutex::new(crate::dev_calls::LogFile::open(&root)));
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
    let (sender, receiver): (Sender<Event>, Receiver<Event>) = channel();
    let shared = Arc::new(Shared {
        current: Mutex::new((Arc::clone(&first.prepared), Arc::clone(&first.config))),
        generation: AtomicU64::new(0),
        urls: urls.clone(),
        log: Mutex::new(sender),
    });
    let make_shared = Arc::clone(&shared);
    // Every store of files on this machine's disk, under `.jadpo/files/`
    // (area 16, "Files"; chunk 6.4b), so nothing leaves it.
    let files = root.join(DEV_FILES);
    if !first.prepared.plan().file_stores().is_empty() {
        let _ = crate::suggestions::local_folder(&root);
    }
    let make_setup = Arc::clone(&setup);
    let make_log = Arc::clone(&call_log);
    let make_root = root.clone();
    let make_inbox_lock = Arc::new(Mutex::new(()));
    let make: MakeHandler = Arc::new(move || {
        // Only the connections the program running now keeps files in.
        let stores = Arc::clone(&make_shared);
        let mut host = NativeHost::new().with_files(DiskFiles::under(&files), move |connection| {
            let current = match stores.current.lock() {
                Ok(current) => Arc::clone(&current.0),
                Err(poisoned) => Arc::clone(&poisoned.into_inner().0),
            };
            current
                .plan()
                .file_stores()
                .iter()
                .any(|store| store.name == connection)
        });
        // Every provider connection the program running now has: faked,
        // or let through to the sandbox, each call logged (chunk 7.2d).
        let providers = Arc::clone(&make_shared);
        let fakes: Rc<dyn jadpo_runtime::capability::Capability> = Rc::new(
            crate::dev_calls::DevProviders::new(&make_root, Arc::clone(&make_setup)),
        );
        // Every mail, through the connection `email:` names, caught in the
        // inbox in place of being sent (chunk 7.3c2).
        let mail = Arc::clone(&make_shared);
        let told = Arc::clone(&make_shared);
        host = host.with_mail(
            Rc::new(crate::dev_inbox::Catcher {
                root: make_root.clone(),
                lock: Arc::clone(&make_inbox_lock),
                caught: Arc::new(move |caught: &crate::dev_inbox::Mail| {
                    if let Ok(log) = told.log.lock() {
                        let _ = log.send(Event::Mail {
                            id: caught.id.clone(),
                            email: caught.email.clone(),
                            to: caught.to.clone(),
                        });
                    }
                }),
                other: Some(Rc::clone(&fakes)),
                // SpamAssassin, when installed, on this machine only
                // (chunk 7.3c3).
                spam: Rc::new(crate::spam::SystemSpamAssassin),
            }),
            move |connection| {
                let current = match mail.current.lock() {
                    Ok(current) => Arc::clone(&current.0),
                    Err(poisoned) => Arc::clone(&poisoned.into_inner().0),
                };
                current.plan().events().emails.connection.as_deref() == Some(connection)
            },
        );
        host = host
            .with_providers(fakes, move |connection| {
                let current = match providers.current.lock() {
                    Ok(current) => Arc::clone(&current.0),
                    Err(poisoned) => Arc::clone(&poisoned.into_inner().0),
                };
                current
                    .plan()
                    .providers()
                    .connections
                    .iter()
                    .any(|known| known.name == connection)
            })
            .with_call_log(Rc::new(DevCalls {
                log: crate::dev_calls::DevLog(Arc::clone(&make_log)),
                shared: Arc::clone(&make_shared),
            }));
        // The model `ai:` names, through the built-in driver, a server on
        // this machine with no key at all (area 21; chunk 5.5v3).
        let models = Arc::clone(&make_shared);
        host = host.with_models(move |connection| {
            let current = match models.current.lock() {
                Ok(current) => Arc::clone(&current.0),
                Err(poisoned) => Arc::clone(&poisoned.into_inner().0),
            };
            // A package filling `ai` is one of the provider connections
            // above, faked or let through to its sandbox (chunk 5.5v3b).
            current.plan().ai().is_some_and(|ai| {
                ai.connection == connection
                    && matches!(ai.driver, jadpo_std::ai::AiDriver::OpenAiCompatible(_))
            })
        });
        for (store, url) in &make_shared.urls {
            if let Ok(database) = postgres(url) {
                host = host.with_store(store.clone(), database);
            }
        }
        let current = Current {
            shared: Arc::clone(&make_shared),
            program: RefCell::new(None),
        };
        let lines = Lines {
            shared: Arc::clone(&make_shared),
            paused: RefCell::new(Default::default()),
        };
        Box::new(
            AppHandler::new(host, move || current.program() as Rc<dyn Application>)
                .observed_by(Rc::new(lines)),
        ) as Box<dyn Handler>
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
    // The inbox's page, on its own port on this machine (chunk 7.3c2):
    // the next one up, or one the system chooses when that is taken.
    let inbox = serve_inbox(&root, port);
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
    // The databases started for this session, so `jadpo schedules` and
    // `jadpo why` in another terminal find them (chunk 6.2b).
    if args.url.is_none() {
        let started: BTreeMap<String, String> = urls
            .iter()
            .filter(|(store, _)| !first.configured.contains_key(*store))
            .map(|(store, url)| (store.clone(), url.clone()))
            .collect();
        remember_stores(&root, &started);
    }
    let inbox_url = inbox
        .as_ref()
        .map(|server| format!("http://localhost:{}", server.address().port()));
    report_ready(
        &mut out,
        Some(&url),
        &first,
        databases,
        Some(&lock(&setup)),
        inbox_url.as_deref(),
    );
    let mut current = first;
    // Request lines are written as they come, between looks for saves.
    loop {
        let drain = |out: &mut Out<'_>| {
            while let Ok(event) = receiver.try_recv() {
                out.event(&event);
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
            if let Some(inbox) = inbox {
                inbox.stop();
            }
            forget_stores(&root);
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
                let reset = match crate::dev_calls::setup(
                    &ready.prepared,
                    &ready.config,
                    &ready.unset,
                    &session,
                ) {
                    Ok(reset) => reset,
                    Err(problem) => {
                        rule_line(&mut out, &time, &saved, "not reloaded");
                        report_failure(&mut out, *problem);
                        continue;
                    }
                };
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
                let changed = *lock(&setup) != reset;
                *lock(&setup) = reset;
                shared.generation.fetch_add(1, Ordering::SeqCst);
                rule_line(&mut out, &time, &saved, "reloaded");
                if changed {
                    report_providers(&mut out, &lock(&setup));
                }
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

/// Writes the address of each database started for the session to
/// [`crate::schedules::DEV_STORES`]; nothing when there are none.
fn remember_stores(root: &Path, started: &BTreeMap<String, String>) {
    let path = root.join(crate::schedules::DEV_STORES);
    if started.is_empty() {
        let _ = std::fs::remove_file(&path);
        return;
    }
    let stores: serde_json::Map<String, serde_json::Value> = started
        .iter()
        .map(|(store, url)| (store.clone(), serde_json::Value::from(url.as_str())))
        .collect();
    let document = serde_json::json!({
        "pid": std::process::id(),
        "stores": stores,
    });
    if let Some(folder) = path.parent() {
        let _ = std::fs::create_dir_all(folder);
    }
    let _ = std::fs::write(&path, format!("{document}\n"));
}

/// Removes what [`remember_stores`] wrote, when the session stops.
fn forget_stores(root: &Path) {
    let _ = std::fs::remove_file(root.join(crate::schedules::DEV_STORES));
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
pub(crate) struct Ready {
    pub(crate) prepared: Arc<Prepared>,
    /// Each store's migrations.
    steps: BTreeMap<String, Vec<jadpo_std::Migration>>,
    /// The stores its entities live in.
    pub(crate) stores: Vec<String>,
    routes: usize,
    /// The settings it starts with (area 10).
    pub(crate) config: Arc<ConfigValues>,
    /// The address each database store's connection reads, where a
    /// variable gives one.
    pub(crate) configured: BTreeMap<String, String>,
    /// The settings each provider connection reads that nothing gives
    /// (chunk 7.2d).
    pub(crate) unset: crate::dev_calls::Unset,
}

/// Loads and checks the project, writes `build/`, and prepares it to run;
/// the problems that stop it otherwise.
pub(crate) fn prepare(
    root: &Path,
    env: &Environment,
    run: bool,
) -> Result<Ready, (Vec<Diagnostic>, BTreeMap<String, String>)> {
    let loaded = jadpo_db::load(root);
    let (db, project) = (&loaded.db, loaded.project);
    let diagnostics = jadpo_check::check(db, project);
    let errors = diagnostics.len();
    let graph = jadpo_check::graph::graph(db, project);
    let openapi = crate::derived::openapi(
        db,
        project,
        &jadpo_check::routes::openapi::title(db, project),
    );
    let permissions = jadpo_check::policy::policy_clean(db, project)
        .then(|| jadpo_check::policy::report::report(db, project));
    let mut surfaces = crate::derived::surfaces(db, project);
    surfaces.extend(crate::derived::network(db, project));
    let _ = crate::derived::write(
        root,
        graph,
        errors,
        &openapi,
        permissions.as_ref(),
        &surfaces,
    );
    let mut blocking: Vec<Diagnostic> = diagnostics
        .into_iter()
        .filter(jadpo_check::blocks_running)
        .collect();
    if !blocking.is_empty() {
        // A linked package's problems show in its checkout (area 11).
        let mut sources = jadpo_db::source_texts(db, project);
        jadpo_db::show_in_checkouts(&loaded.links, &mut blocking, &mut sources);
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
    let (config, configured, unset) = if run {
        let model = jadpo_check::config::config(db, project);
        let types = |field: &str| {
            model
                .setting(field)
                .map_or_else(String::new, |setting| setting.ty.to_string())
        };
        start_config(&prepared, root, env, &stores, &types)
            .map_err(|problems| (problems, BTreeMap::new()))?
    } else {
        (
            Arc::new(ConfigValues::default()),
            BTreeMap::new(),
            BTreeMap::new(),
        )
    };
    Ok(Ready {
        prepared,
        steps,
        stores,
        routes,
        config,
        configured,
        unset,
    })
}

/// The settings the app starts with and the address each database store's
/// connection reads, where a variable gives one.
type StartConfig = (
    Arc<ConfigValues>,
    BTreeMap<String, String>,
    crate::dev_calls::Unset,
);

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
        return Ok((
            Arc::new(ConfigValues::default()),
            BTreeMap::new(),
            BTreeMap::new(),
        ));
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
    let mut placeholders: BTreeMap<String, String> = wanted
        .iter()
        .filter(|(_, _, missing)| *missing)
        .map(|(_, variable, _)| {
            (
                variable.clone(),
                "postgres://disposable.invalid/dev".to_string(),
            )
        })
        .collect();
    // A provider connection's settings that nothing gives: a stand-in
    // lets the app start, and the connection's calls stay faked, so no
    // credential is needed (area 11, "Provider calls in development").
    let mut unset: crate::dev_calls::Unset = BTreeMap::new();
    for (connection, read) in crate::dev_calls::provider_settings(prepared) {
        for at in read {
            let Some(field) = plan.fields.get(at as usize) else {
                continue;
            };
            if field.default.is_some() || layered.variable(&field.variable).is_some() {
                continue;
            }
            placeholders.insert(
                field.variable.clone(),
                crate::dev_calls::stand_in(&types(&field.name)),
            );
            unset
                .entry(connection.clone())
                .or_default()
                .push(field.variable.clone());
        }
    }
    // An SMTP connection's settings that nothing gives: the inbox catches
    // every mail, so its server is never reached (chunk 7.3c2).
    for smtp in &prepared.plan().events().emails.smtp {
        let host = match smtp.host {
            jadpo_std::email::SmtpHost::Setting(at) => Some(at),
            jadpo_std::email::SmtpHost::Written(_) => None,
        };
        for at in [host, smtp.username, smtp.password].into_iter().flatten() {
            let Some(field) = plan.fields.get(at as usize) else {
                continue;
            };
            if field.default.is_some() || layered.variable(&field.variable).is_some() {
                continue;
            }
            placeholders.insert(
                field.variable.clone(),
                crate::dev_calls::stand_in(&types(&field.name)),
            );
        }
    }
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
    Ok((Arc::new(values), configured, unset))
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

fn report_ready(
    out: &mut Out<'_>,
    url: Option<&str>,
    ready: &Ready,
    databases: Databases,
    providers: Option<&crate::dev_calls::Setup>,
    inbox: Option<&str>,
) {
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
            ("providers", providers.map_or(Json::Null, providers_json)),
            ("inbox", inbox.map_or(Json::Null, Json::from)),
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
    let providers = providers.filter(|setup| !setup.connections.is_empty());
    if let Some(setup) = providers {
        rows.push(("providers".to_string(), providers_row(setup)));
    }
    // The inbox, once the app sends mail.
    if let Some(inbox) = inbox
        && ready.prepared.plan().events().emails.connection.is_some()
    {
        rows.push((
            "inbox".to_string(),
            format!("{inbox} · mail caught, never sent"),
        ));
    }
    let text = match url {
        Some(url) => format!("listening on {url}"),
        None => "checked; serving nothing (--no-run)".to_string(),
    };
    let mut screen = Screen::new().with(Block::Status {
        outcome: Outcome::Success,
        text,
        rows,
    });
    if let Some(alerts) = providers
        .map(providers_alerts)
        .filter(|alerts| !alerts.is_empty())
    {
        screen.push(Block::Alerts(alerts));
    }
    out.screen(screen);
    let keys = Screen::new().with(Block::KeyBar {
        label: "serving".to_string(),
        facts: vec![Fact::plain("saves reload")],
        keys: vec![("ctrl-c".to_string(), "quit".to_string())],
    });
    let _ = out.err.write_all(keys.render(&out.err_style).as_bytes());
    let _ = out.err.flush();
}

/// Serves the inbox's page on this machine, on the port after `port`, or
/// one the system chooses when that is taken; `None` when neither can be
/// listened on, and the inbox is still read with `jadpo email`.
fn serve_inbox(root: &Path, port: u16) -> Option<Server> {
    let listener = port
        .checked_add(1)
        .filter(|_| port != 0)
        .and_then(|next| TcpListener::bind(("127.0.0.1", next)).ok())
        .or_else(|| TcpListener::bind(("127.0.0.1", 0)).ok())?;
    let root = root.to_path_buf();
    let make: MakeHandler =
        Arc::new(move || Box::new(InboxHandler(root.clone())) as Box<dyn Handler>);
    jadpo_host_native::http::serve(listener, 1, make).ok()
}

/// Answers the inbox's page.
struct InboxHandler(PathBuf);

impl Handler for InboxHandler {
    fn handle(
        &self,
        request: jadpo_runtime::http::Request,
    ) -> jadpo_runtime::LocalFuture<'_, jadpo_runtime::http::Response> {
        let response = crate::inbox_page::answer(&self.0, &request);
        Box::pin(async move { response })
    }
}

fn lock<T>(held: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    match held.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// What the provider connections' calls do (chunk 7.2d): one line for
/// those let through to the sandbox, and one alert for each that stays
/// faked because a setting its sandbox needs has no value.
fn report_providers(out: &mut Out<'_>, setup: &crate::dev_calls::Setup) {
    if setup.connections.is_empty() {
        return;
    }
    if out.json {
        let data = Json::object([
            ("event", "providers".into()),
            ("providers", providers_json(setup)),
        ]);
        out.json(data, &[], &BTreeMap::new());
        return;
    }
    let mut screen = Screen::new().with(Block::Status {
        outcome: Outcome::Success,
        text: "Provider calls changed".to_string(),
        rows: vec![("providers".to_string(), providers_row(setup))],
    });
    let alerts = providers_alerts(setup);
    if !alerts.is_empty() {
        screen.push(Block::Alerts(alerts));
    }
    out.screen(screen);
}

/// Each provider connection, the operations let through to its sandbox
/// and the variables that keep it faked.
fn providers_json(setup: &crate::dev_calls::Setup) -> Json {
    Json::array(setup.connections.iter().map(|connection| {
        Json::object([
            ("connection", Json::from(connection.name.as_str())),
            (
                "sandbox",
                Json::array(connection.sandbox.iter().map(String::as_str)),
            ),
            (
                "unset",
                Json::array(connection.unset.iter().map(String::as_str)),
            ),
        ])
    }))
}

/// What each provider connection's calls do, as one row says it:
/// `couriers faked · ledger: bookings.get to the sandbox`.
fn providers_row(setup: &crate::dev_calls::Setup) -> String {
    setup
        .connections
        .iter()
        .map(|connection| {
            if connection.sandbox.is_empty() {
                format!("{} faked", connection.name)
            } else {
                format!(
                    "{}: {} to the sandbox",
                    connection.name,
                    connection.sandbox.join(", ")
                )
            }
        })
        .collect::<Vec<_>>()
        .join(" · ")
}

/// An alert for each connection that stays faked because a setting its
/// sandbox needs has no value, naming the variable to set.
fn providers_alerts(setup: &crate::dev_calls::Setup) -> Vec<String> {
    setup
        .connections
        .iter()
        .filter(|connection| !connection.unset.is_empty())
        .map(|connection| {
            format!(
                "`{}` stays faked: set {} in `.env.local` for its sandbox",
                connection.name,
                connection
                    .unset
                    .iter()
                    .map(|variable| format!("`{variable}`"))
                    .collect::<Vec<_>>()
                    .join(" and ")
            )
        })
        .collect()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refused_sign_in_shows_its_reason_under_its_request() {
        let plain = |_: Token, text: &str| text.to_string();
        let refused = jadpo_runtime::SignInRefused {
            reason: "refused_by_app",
            failure: Some("SignUpClosed".to_string()),
        };
        assert_eq!(
            refusal_line(&refused, &plain),
            "  └─ sign-in refused · refused_by_app · SignUpClosed\n"
        );
        let refused = jadpo_runtime::SignInRefused {
            reason: "code_invalid",
            failure: None,
        };
        assert_eq!(
            refusal_line(&refused, &plain),
            "  └─ sign-in refused · code_invalid\n"
        );
    }

    fn shown(
        position: u32,
        operation: &str,
        state: jadpo_runtime::steps::StepState,
        source: Option<jadpo_runtime::capability::Source>,
    ) -> jadpo_runtime::steps::Shown {
        jadpo_runtime::steps::Shown {
            position,
            operation: operation.to_string(),
            state,
            source,
        }
    }

    /// Each step a run reached is a line of its trace: its place, its
    /// operation, where its answer came from and what became of it; a
    /// step given back from the record says only that (chunk 7.5f).
    #[test]
    fn a_step_shows_its_place_operation_source_and_state() {
        use jadpo_runtime::capability::Source;
        use jadpo_runtime::steps::StepState;
        let plain = |_: Token, text: &str| text.to_string();
        let cases = [
            (
                shown(
                    1,
                    "pay.customers.create",
                    StepState::Replayed,
                    Some(Source::Replayed),
                ),
                "  └─ step 1 · pay.customers.create · replayed\n",
            ),
            (
                shown(2, "sheets.put", StepState::Answered, Some(Source::Live)),
                "  └─ step 2 · sheets.put · live · answered\n",
            ),
            (
                shown(
                    3,
                    "pay.subscriptions.create",
                    StepState::InDoubt,
                    Some(Source::Generated),
                ),
                "  └─ step 3 · pay.subscriptions.create · generated · in doubt\n",
            ),
            (
                shown(
                    3,
                    "pay.subscriptions.create",
                    StepState::Failed("card_declined".into()),
                    Some(Source::Scripted),
                ),
                "  └─ step 3 · pay.subscriptions.create · scripted · card_declined\n",
            ),
            (
                shown(2, "sheets.put", StepState::Diverged, None),
                "  └─ step 2 · sheets.put · diverged\n",
            ),
            (
                shown(
                    3,
                    "pay.subscriptions.create",
                    StepState::OutcomeUnknown,
                    None,
                ),
                "  └─ step 3 · pay.subscriptions.create · outcome unknown\n",
            ),
        ];
        for (step, line) in cases {
            assert_eq!(step_line(&step, &plain), line);
        }
    }

    fn attempt(state: &str) -> StepsLine {
        use jadpo_runtime::capability::Source;
        use jadpo_runtime::steps::{RunSteps, StepState};
        let mut id = [0u8; 16];
        id[12..].copy_from_slice(&[0x41, 0xd7, 0xe0, 0x9a]);
        StepsLine {
            id,
            subscriber: "billing.start_subscription".into(),
            state: state.into(),
            steps: vec![RunSteps {
                action: "subscribe".into(),
                steps: vec![
                    shown(
                        1,
                        "pay.customers.create",
                        StepState::Replayed,
                        Some(Source::Replayed),
                    ),
                    shown(2, "pay.subscriptions.create", StepState::Diverged, None),
                ],
            }],
        }
    }

    /// Writes `event` as a terminal with colour off shows it, or as
    /// `--json`.
    fn written(event: &Event, json: bool) -> String {
        let env = Environment::empty()
            .with_terminals(true, true, true)
            .with_var("TERM", "xterm-256color")
            .with_var("NO_COLOR", "1");
        let (mut out, mut err) = (Vec::new(), Vec::new());
        {
            let mut screen = Out {
                out: &mut out,
                err: &mut err,
                style: Style::detect(&env, json, ColorChoice::Auto, Stream::Stdout),
                err_style: Style::detect(&env, json, ColorChoice::Auto, Stream::Stderr),
                json,
            };
            screen.event(event);
        }
        String::from_utf8(out).expect("text")
    }

    /// A delivery whose action reached steps is a line of its own, as a
    /// schedule's run is, with each step under it and, when it needs an
    /// operator, the command that shows it (chunk 7.5f).
    #[test]
    fn a_delivery_with_steps_is_a_line_with_its_steps_and_what_to_do() {
        assert_eq!(
            written(&Event::Delivery(attempt("diverged")), false),
            "41d7e09a  subscriber billing.start_subscription ◆ diverged\n  └─ step 1 · pay.customers.create · replayed\n  └─ step 2 · pay.subscriptions.create · diverged\n  → jadpo deliveries show 41d7e09a\n"
        );
        let succeeded = written(&Event::Delivery(attempt("succeeded")), false);
        assert!(
            succeeded.starts_with("41d7e09a  subscriber billing.start_subscription ✓ succeeded\n"),
            "{succeeded}"
        );
        assert!(!succeeded.contains("jadpo deliveries show"), "{succeeded}");
        let retrying = written(&Event::Delivery(attempt("pending")), false);
        assert!(retrying.contains("○ retrying\n"), "{retrying}");

        let json = written(&Event::Delivery(attempt("diverged")), true);
        let document: serde_json::Value = serde_json::from_str(json.trim()).expect("JSON");
        assert_eq!(document["data"]["event"], "delivery");
        assert_eq!(document["data"]["state"], "diverged");
        assert_eq!(
            document["data"]["delivery"],
            "00000000000000000000000041d7e09a"
        );
        assert_eq!(
            document["data"]["steps"],
            serde_json::json!([
                { "action": "subscribe", "step": 1, "operation": "pay.customers.create", "state": "replayed", "code": null, "source": "replayed" },
                { "action": "subscribe", "step": 2, "operation": "pay.subscriptions.create", "state": "diverged", "code": null, "source": null },
            ])
        );
    }

    /// A request whose action kept a record shows its steps in place of
    /// their calls; its other calls stay as they were (chunk 7.5f).
    #[test]
    fn a_requests_steps_show_in_place_of_their_calls() {
        use jadpo_runtime::capability::{Ended, Logged, Repeat, Source};
        use jadpo_runtime::steps::{RunSteps, StepState};
        let call = |operation: &str, step: Option<u32>| Logged {
            connection: "pay".into(),
            operation: operation.into(),
            repeat: Repeat::Keyed,
            source: Source::Generated,
            at: 0,
            took_ms: None,
            ended: Ended::Answered,
            step,
        };
        let line = Line {
            id: "9c1f0a2b-0000".into(),
            method: "POST".into(),
            path: "/partners/subscriptions".into(),
            status: 204,
            code: None,
            route: None,
            millis: 38,
            address: None,
            fault: None,
            calls: vec![call("plans.get", None), call("customers.create", Some(1))],
            refused: None,
            steps: vec![RunSteps {
                action: "subscribe_now".into(),
                steps: vec![shown(
                    1,
                    "pay.customers.create",
                    StepState::Answered,
                    Some(Source::Generated),
                )],
            }],
        };
        assert_eq!(
            written(&Event::Request(line), false),
            "9c1f0a2b  POST /partners/subs…               204   38 ms\n  └─ pay.plans.get · generated · answered\n  └─ step 1 · pay.customers.create · generated · answered\n"
        );
    }
}
