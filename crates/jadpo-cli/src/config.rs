//! `jadpo config check` and `jadpo config set <field>` (area 14, with
//! area 10, "Values on your machine and in production"): look at, and fill
//! in, the settings `config.jadpo` declares against the values this
//! machine has, which are the process's environment over `.env.local`.
//!
//! Design (the brief, before the code; the CLI design language is the
//! visual authority, the screens drawn in it):
//!
//! - *Job.* A person, or the agent working for them, learns what the app
//!   still needs to start, and a person types a secret where it cannot
//!   reach a chat, a shell history or a log. Mode: operate. Scanning
//!   beats reading, and a value never appears anywhere.
//! - `check` is one list: a setting, the word for how it stands (`set`,
//!   `default`, `missing`, `invalid`) and the variable it is read from.
//!   Every problem is then an error of its own, the shape agents read
//!   (`rule:` and `next:`), and the one command to run next ends the
//!   screen. A passing run is the list and one line.
//! - `set` is one prompt and one status line: `? Enter MAIL_API_KEY`
//!   (hidden for a secret, shown for anything else), then `✓ mail_api_key
//!   configured locally`. What was typed is checked against the setting's
//!   type before anything is written, and a refusal names the rule it
//!   breaks, never the value. It needs a terminal and refuses `--json`,
//!   naming `jadpo config check`, which does the reading.
//! - *States.* Everything set; defaults only; some missing; some invalid;
//!   no `config { }` at all (a note, not an error); a project that does
//!   not check yet (its errors, as every command shows them).

use std::collections::BTreeMap;
use std::io::{BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::{Command as Process, Stdio};

use jadpo_diag::{Diagnostic, rule};
use jadpo_host_native::NativeHost;
use jadpo_interp::Prepared;
use jadpo_runtime::Host;
use jadpo_runtime::config::{Mode, ProblemKind, StartProblem, State, Variables};
use jadpo_term::{
    Block, ColorChoice, Environment, Fact, Json, Outcome, Screen, Stream, Style, Token,
};

use crate::fmt::project_not_found;
use crate::local;
use crate::migrate::blocked;
use crate::run::{Call, Reply, Session, both, either, quoted};
use crate::suggest::nearest;

/// What `jadpo config set` was asked for, checked before it prompts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetArgs {
    pub root: PathBuf,
    pub field: String,
    pub choice: ColorChoice,
}

/// The two things the command does.
pub const ACTIONS: &[crate::table::Value] = &[
    crate::table::Value {
        name: "check",
        description: "List each setting as set, default, missing or invalid",
    },
    crate::table::Value {
        name: "set",
        description: "Store one setting's value in .env.local",
    },
];

pub fn config(call: &Call) -> Reply {
    let Some(action) = call.arguments.first() else {
        return Reply::failed(
            Diagnostic::new(
                &rule::CLI_CONFIG_ARGUMENTS,
                "`jadpo config` needs an action",
            )
            .reason(format!(
                "It does {}.",
                either(&quoted(ACTIONS.iter().map(|a| a.name)))
            ))
            .next("Run `jadpo config check`, or `jadpo config set <field>`"),
        );
    };
    match action.as_str() {
        "check" => check(call),
        "set" => set_command(call),
        other => {
            let names: Vec<&str> = ACTIONS.iter().map(|a| a.name).collect();
            let diagnostic = Diagnostic::new(
                &rule::CLI_CONFIG_ARGUMENTS,
                format!("`{other}` isn't something `jadpo config` does"),
            );
            Reply::failed(match nearest(other, names.iter().copied()) {
                Some(near) => diagnostic
                    .reason(format!(
                        "`jadpo config` does {}. Did you mean `{near}`?",
                        both(&quoted(&names))
                    ))
                    .next(format!("Use `{near}` instead of `{other}`")),
                None => diagnostic
                    .reason(format!("`jadpo config` does {}.", both(&quoted(&names))))
                    .next("Run `jadpo config check`, or `jadpo config set <field>`"),
            })
        }
    }
}

/// The values this machine has: the process's environment, then
/// `.env.local`. Only the variables the settings read are looked at.
pub(crate) struct Layered {
    process: Environment,
    file: BTreeMap<String, String>,
}

impl std::fmt::Debug for Layered {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Layered")
    }
}

impl Layered {
    pub(crate) fn new(root: &Path, process: &Environment) -> Result<Layered, Box<Diagnostic>> {
        Ok(Layered {
            process: process.clone(),
            file: local::read(root)?.values(),
        })
    }
}

impl Variables for Layered {
    fn variable(&self, name: &str) -> Option<String> {
        self.process
            .var(name)
            .filter(|value| !value.is_empty())
            .or_else(|| self.file.get(name).map(String::as_str))
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    }
}

/// Random bytes from the operating system.
pub(crate) fn random(bytes: &mut [u8]) -> bool {
    NativeHost::new().random(bytes)
}

// --- check -------------------------------------------------------------------

fn check(call: &Call) -> Reply {
    let cwd = call.cwd.clone().unwrap_or_else(|| PathBuf::from("."));
    let Some(root) = jadpo_db::find_root(&cwd) else {
        return project_not_found("config");
    };
    let loaded = jadpo_db::load(&root);
    let prepared = match Prepared::load(&loaded.db, loaded.project) {
        Ok(prepared) => prepared,
        Err(problems) => return blocked(&loaded, &cwd, problems, "config stopped"),
    };
    let layered = match Layered::new(&root, &call.environment) {
        Ok(layered) => layered,
        Err(problem) => return Reply::failed(*problem),
    };
    let plan = prepared.plan().config();
    if plan.is_empty() {
        let screen = Screen::new().with(Block::Status {
            outcome: Outcome::Success,
            text: "No settings declared".to_string(),
            rows: vec![(
                "config.jadpo".to_string(),
                "holds no `config { }`, so the app reads no variables".to_string(),
            )],
        });
        return Reply::screen(
            screen,
            Json::object([
                ("ready", true.into()),
                ("settings", Json::array(Vec::<Json>::new())),
            ]),
        );
    }
    let standing = prepared.config_standing(&layered, Mode::Environment, &random);
    let model = jadpo_check::config::config(&loaded.db, loaded.project);
    let mut lines = Vec::new();
    let mut data = Vec::new();
    let mut diagnostics = Vec::new();
    let widest = standing
        .iter()
        .map(|setting| setting.field.chars().count())
        .max()
        .unwrap_or(0);
    let widest_state = ["missing", "invalid", "default", "set"]
        .iter()
        .map(|word| word.len())
        .max()
        .unwrap_or(0);
    for setting in &standing {
        let word = state_word(&setting.state);
        lines.push(format!(
            "{:widest$}  {:widest_state$}  {}",
            setting.field, word, setting.variable
        ));
        let ty = model
            .setting(&setting.field)
            .map_or_else(String::new, |s| s.ty.to_string());
        let mut entry = vec![
            ("name", Json::from(setting.field.as_str())),
            ("variable", Json::from(setting.variable.as_str())),
            ("secret", Json::from(setting.secret)),
            ("state", Json::from(word)),
        ];
        match &setting.state {
            State::Missing => diagnostics.push(value_diagnostic(
                &setting.field,
                &setting.variable,
                setting.secret,
                &ty,
                &ProblemKind::Missing,
            )),
            State::Invalid { rule, variable } => {
                entry.push(("rule", Json::from(rule.as_str())));
                diagnostics.push(value_diagnostic(
                    &setting.field,
                    variable,
                    setting.secret,
                    &ty,
                    &ProblemKind::Invalid { rule: rule.clone() },
                ));
            }
            _ => {}
        }
        data.push(Json::object(entry));
    }
    let count = standing.len();
    let mut screen = Screen::new().with(Block::Section {
        heading: "Values".to_string(),
        lines,
    });
    let ready = diagnostics.is_empty();
    for problem in &diagnostics {
        screen.push(Block::diagnostic(problem, &()));
    }
    let settings = if count == 1 {
        "1 setting".to_string()
    } else {
        format!("{count} settings")
    };
    if ready {
        screen.push(Block::summary(
            Outcome::Success,
            "config ready",
            [Fact::plain(settings)],
        ));
    } else {
        let problems = diagnostics.len();
        screen.push(Block::summary(
            Outcome::Failure,
            "config incomplete",
            [
                Fact::problem(format!(
                    "{problems} {}",
                    if problems == 1 { "problem" } else { "problems" }
                )),
                Fact::plain(settings),
            ],
        ));
        if let Some(first) = standing.iter().find(|setting| setting.state.is_problem()) {
            screen.push(Block::hint(
                format!("jadpo config set {}", first.field),
                "stores it in .env.local",
            ));
        }
    }
    Reply::Report {
        screen,
        data: Json::object([("ready", ready.into()), ("settings", Json::array(data))]),
        diagnostics,
        sources: BTreeMap::new(),
    }
}

/// How a setting stands, in one word.
fn state_word(state: &State) -> &'static str {
    match state {
        State::Set => "set",
        State::Default => "default",
        State::Test => "test",
        State::Generated => "generated",
        State::Missing => "missing",
        State::Invalid { .. } => "invalid",
        State::Absent => "absent",
    }
}

/// The error for a setting that stops the application starting, as
/// `jadpo config check` and `jadpo dev` show it: the setting and the
/// variable, never the value.
pub(crate) fn value_diagnostic(
    field: &str,
    variable: &str,
    secret: bool,
    ty: &str,
    kind: &ProblemKind,
) -> Diagnostic {
    match kind {
        ProblemKind::Missing => Diagnostic::new(
            &rule::CONFIGURATION_VALUE_MISSING,
            format!("`{variable}` isn't set"),
        )
        .reason(format!(
            "`{field}` is required: it has no `default:`, so the app can't start without a value for it."
        ))
        .next_for(
            format!(
                "Run `jadpo config set {field}` to store it in `{}`, or set `{variable}` in the environment",
                local::FILE
            ),
            jadpo_diag::Decider::Human,
        ),
        ProblemKind::Invalid { rule } => {
            let of = if ty.is_empty() {
                String::new()
            } else {
                format!(" of type `{ty}`")
            };
            Diagnostic::new(
                &rule::CONFIGURATION_VALUE_INVALID,
                format!("`{variable}` isn't a valid value for `{field}`"),
            )
            .reason(format!(
                "`{field}`{of} reads `{variable}`, and the value there breaks `{rule}`. The value isn't shown{}.",
                if secret { ", as it is secret" } else { ", so it can't leak into a log" }
            ))
            .next_for(
                format!(
                    "Run `jadpo config set {field}` to store a valid one, or fix `{variable}` in the environment"
                ),
                jadpo_diag::Decider::Human,
            )
        }
    }
}

/// The errors for the settings that stop an app starting.
pub(crate) fn start_diagnostics(
    problems: &[StartProblem],
    ty: &dyn Fn(&str) -> String,
) -> Vec<Diagnostic> {
    problems
        .iter()
        .map(|problem| {
            value_diagnostic(
                &problem.field,
                &problem.variable,
                problem.secret,
                &ty(&problem.field),
                &problem.kind,
            )
        })
        .collect()
}

// --- set ---------------------------------------------------------------------

fn set_command(call: &Call) -> Reply {
    let cwd = call.cwd.clone().unwrap_or_else(|| PathBuf::from("."));
    let Some(field) = call.arguments.get(1) else {
        return Reply::failed(
            Diagnostic::new(
                &rule::CLI_CONFIG_ARGUMENTS,
                "`jadpo config set` needs a setting",
            )
            .reason("It stores one setting's value at a prompt, so it needs to know which.")
            .next("Run `jadpo config set <field>`, naming a field of `config { }`"),
        );
    };
    let Some(root) = jadpo_db::find_root(&cwd) else {
        return project_not_found("config");
    };
    let loaded = jadpo_db::load(&root);
    let prepared = match Prepared::load(&loaded.db, loaded.project) {
        Ok(prepared) => prepared,
        Err(problems) => return blocked(&loaded, &cwd, problems, "config stopped"),
    };
    let plan = prepared.plan().config();
    if plan.field(field).is_none() {
        let names: Vec<&str> = plan.fields.iter().map(|f| f.name.as_str()).collect();
        let diagnostic = Diagnostic::new(
            &rule::CLI_CONFIG_ARGUMENTS,
            format!("`{field}` isn't a setting"),
        );
        return Reply::failed(match nearest(field, names.iter().copied()) {
            Some(near) => diagnostic
                .reason(format!(
                    "`config.jadpo` declares no setting called `{field}`. Did you mean `{near}`?"
                ))
                .next(format!("Use `{near}` instead of `{field}`")),
            None if names.is_empty() => diagnostic
                .reason("The project declares no `config { }`.")
                .next("Declare the setting in `config.jadpo` first"),
            None => diagnostic
                .reason(format!("The settings are {}.", both(&quoted(&names))))
                .next("Use one of them"),
        });
    }
    if !call.environment.stdin_terminal {
        return Reply::failed(
            Diagnostic::new(
                &rule::CLI_CONFIG_TTY_REQUIRED,
                "`jadpo config set` needs a terminal",
            )
            .reason("The value is typed at a prompt, never given as an argument or piped in, so it can't land in shell history, a log or a chat; `jadpo config check` is the command that works without a terminal.")
            .next_for(
                format!("Run `jadpo config set {field}` yourself in a terminal"),
                jadpo_diag::Decider::Human,
            ),
        );
    }
    Reply::Session(Session::ConfigSet(Box::new(SetArgs {
        root,
        field: field.clone(),
        choice: ColorChoice::Auto,
    })))
}

/// Runs the prompt; the exit code.
pub fn set(args: &SetArgs, env: &Environment) -> u8 {
    let style = Style::detect(env, false, args.choice, Stream::Stdout);
    let err_style = Style::detect(env, false, args.choice, Stream::Stderr);
    let fail = |problem: Diagnostic| {
        let screen = Screen::new().with(Block::diagnostic(&problem, &()));
        let _ = std::io::stderr().write_all(screen.render(&err_style).as_bytes());
        1
    };
    let loaded = jadpo_db::load(&args.root);
    let prepared = match Prepared::load(&loaded.db, loaded.project) {
        Ok(prepared) => prepared,
        Err(_) => {
            return fail(
                Diagnostic::new(&rule::CLI_CONFIG_ARGUMENTS, "The project has errors now")
                    .reason("It checked a moment ago, and a save since then broke it.")
                    .next("Run `jadpo check`, then `jadpo config set` again"),
            );
        }
    };
    let plan = prepared.plan().config();
    let Some((_, field)) = plan.field(&args.field) else {
        return fail(Diagnostic::new(
            &rule::CLI_CONFIG_ARGUMENTS,
            format!("`{}` isn't a setting", args.field),
        ));
    };
    let paint = |token: Token, text: &str| match err_style.color {
        Some(depth) => format!("{}{text}\x1b[0m", token.sgr(depth, err_style.theme)),
        None => text.to_string(),
    };
    let question = format!("Enter {}", field.variable);
    let hint = if field.secret { "hidden" } else { "visible" };
    let mut err = std::io::stderr();
    let _ = write!(
        err,
        "{} {} {}  ",
        paint(Token::Accent, "?"),
        question,
        paint(Token::Muted, hint)
    );
    let _ = err.flush();
    let typed = match read_value(field.secret) {
        Ok(typed) => typed,
        Err(why) => {
            let _ = writeln!(err);
            return fail(
                Diagnostic::new(
                    &rule::CLI_CONFIG_PROMPT_FAILED,
                    format!("The prompt for `{}` failed", field.variable),
                )
                .reason(why)
                .next_for(
                    "Run the command again in a terminal that can hide what you type",
                    jadpo_diag::Decider::Human,
                ),
            );
        }
    };
    let _ = writeln!(err);
    if typed.is_empty() {
        return fail(
            Diagnostic::new(
                &rule::CLI_CONFIG_PROMPT_FAILED,
                format!("Nothing was typed for `{}`", field.variable),
            )
            .reason("An empty answer would leave the setting unset, which `jadpo config check` already shows.")
            .next_for(
                format!("Run `jadpo config set {}` and type the value", field.name),
                jadpo_diag::Decider::Human,
            ),
        );
    }
    let screen = match store(args, &typed) {
        Ok(screen) => screen,
        Err(problem) => return fail(*problem),
    };
    let _ = std::io::stdout().write_all(screen.render(&style).as_bytes());
    0
}

/// Checks `typed` against setting `args.field`'s type, the same reading the
/// app starts with, so what is stored is what will be accepted, and stores
/// it in `.env.local`. The screen that says so; never the value.
pub fn store(args: &SetArgs, typed: &str) -> Result<Screen, Box<Diagnostic>> {
    let loaded = jadpo_db::load(&args.root);
    let prepared = Prepared::load(&loaded.db, loaded.project).map_err(|_| {
        Box::new(
            Diagnostic::new(&rule::CLI_CONFIG_ARGUMENTS, "The project has errors now")
                .reason("It checked a moment ago, and a save since then broke it.")
                .next("Run `jadpo check`, then `jadpo config set` again"),
        )
    })?;
    let plan = prepared.plan().config();
    let Some((_, field)) = plan.field(&args.field) else {
        return Err(Box::new(Diagnostic::new(
            &rule::CLI_CONFIG_ARGUMENTS,
            format!("`{}` isn't a setting", args.field),
        )));
    };
    if let Err(rule) = jadpo_runtime::config::decode(plan, prepared.validators(), field, typed) {
        let ty = jadpo_check::config::config(&loaded.db, loaded.project)
            .setting(&field.name)
            .map_or_else(String::new, |s| s.ty.to_string());
        return Err(Box::new(value_diagnostic(
            &field.name,
            &field.variable,
            field.secret,
            &ty,
            &ProblemKind::Invalid { rule },
        )));
    }
    let mut local = local::read(&args.root)?;
    local.set(&field.variable, typed);
    let ignored = local::write(&args.root, &local)?;
    let mut rows = vec![
        ("variable".to_string(), field.variable.clone()),
        ("file".to_string(), local::FILE.to_string()),
    ];
    if ignored {
        rows.push((
            "gitignore".to_string(),
            format!("added {}, so it isn't committed", local::FILE),
        ));
    }
    Ok(Screen::new().with(Block::Status {
        outcome: Outcome::Success,
        text: format!("{} configured locally", field.name),
        rows,
    }))
}

/// One line typed at the terminal, without echo for a secret. The terminal
/// is put back as it was whatever happens.
fn read_value(secret: bool) -> Result<String, String> {
    let stdin = std::io::stdin();
    if !stdin.is_terminal() {
        return Err("Standard input isn't a terminal.".to_string());
    }
    let saved = if secret { Some(hide_echo()?) } else { None };
    let mut line = String::new();
    let read = stdin.lock().read_line(&mut line);
    if let Some(saved) = saved {
        restore_echo(&saved);
    }
    read.map_err(|error| format!("Reading what you typed failed: {error}."))?;
    Ok(line.trim_end_matches(['\n', '\r']).to_string())
}

/// Turns off the terminal's echo with `stty`, giving back the settings to
/// restore. A secret is never typed where it shows, so if the echo can't be
/// turned off nothing is read.
fn hide_echo() -> Result<String, String> {
    let cannot = || {
        "The terminal wouldn't hide what you type, so Jadpo won't read a secret from it."
            .to_string()
    };
    let saved = Process::new("stty")
        .arg("-g")
        .stdin(Stdio::inherit())
        .output()
        .map_err(|_| cannot())?;
    if !saved.status.success() {
        return Err(cannot());
    }
    let saved = String::from_utf8_lossy(&saved.stdout).trim().to_string();
    let hidden = Process::new("stty")
        .arg("-echo")
        .stdin(Stdio::inherit())
        .status()
        .map_err(|_| cannot())?;
    if !hidden.success() {
        return Err(cannot());
    }
    Ok(saved)
}

fn restore_echo(saved: &str) {
    let _ = Process::new("stty")
        .arg(saved)
        .stdin(Stdio::inherit())
        .status();
}
