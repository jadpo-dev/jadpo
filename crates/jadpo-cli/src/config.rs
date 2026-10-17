//! `jadpo config check` and `jadpo config set <field>` (area 14, with
//! area 10, "Values on your machine and in production"): look at, and fill
//! in, the settings `config.jadpo` declares against the values this
//! machine has, which are the process's environment over `.env.local`.
//! `jadpo config move <constant>` (area 10, "Making a constant a setting",
//! chunk 5.3f) makes a constant a setting through the code the editor's
//! "Make this a setting" runs (`jadpo_check::setting_move`).
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
//!
//! `move`, designed before it was built (impeccable's shape pass against
//! the CLI design language, run unattended, so its assumptions are marked):
//!
//! - *Job.* An agent, or a person reviewing its work, turns a fixed value
//!   into one each deployment sets, and needs to see every place whose
//!   behaviour can now differ, and the variable a deployment may now set.
//!   Mode: operate. The source diff is the full record; the screen is the
//!   summary a reviewer checks it against.
//! - *Success* reads as `jadpo migrate new` does: a change list grouped by
//!   file (the setting's file, the constant's, then each reader's), each
//!   row a `+ ~ -` mark, a subject and a muted note (`+ max_login_attempts
//!   setting, default 5`, `- max_login_attempts  constant`, `~ line 5
//!   reads config.max_login_attempts`), then one status line, `✓ Made
//!   security.max_login_attempts a setting`, with the setting and its
//!   variable on the label grid. Imports are left to the diff: mechanical,
//!   and noise here (assumption). No hint: nothing has to happen next.
//! - *Refused* is each reason as an error in the one shape (a frame at
//!   the constant or at the read, `reason`, `next` with `jadpo config move`
//!   to run again, `rule`), every reason at once so one round fixes them,
//!   then `✗ config move refused  nothing changed`.
//! - *States.* Moved into an existing `config { }` or a new
//!   `config.jadpo`; a library or file left empty and removed; a setting
//!   whose `default:` named the constant filled in (`~`); refused; a result
//!   that wouldn't check (its errors, then `✗ config move stopped`); a
//!   project that doesn't check (its errors, as `check` and `set` show
//!   them); no such constant, or one name for two (`cli.config_arguments`,
//!   with the nearest name or each domain's).

use std::collections::BTreeMap;
use std::io::{BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::{Command as Process, Stdio};

use jadpo_check::setting_move;
use jadpo_diag::{Diagnostic, rule};
use jadpo_host_native::NativeHost;
use jadpo_interp::Prepared;
use jadpo_runtime::Host;
use jadpo_runtime::config::{Mode, ProblemKind, StartProblem, State, Variables};
use jadpo_term::{
    Block, ChangeMark, ColorChoice, Environment, Fact, Json, Outcome, Screen, Stream, Style, Token,
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
        name: "move",
        description: "Make a constant a setting, rewriting every read",
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
        "move" => move_command(call),
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

// --- move --------------------------------------------------------------------

fn move_command(call: &Call) -> Reply {
    let cwd = call.cwd.clone().unwrap_or_else(|| PathBuf::from("."));
    let Some(written) = call.arguments.get(1) else {
        return Reply::failed(
            Diagnostic::new(
                &rule::CLI_CONFIG_ARGUMENTS,
                "`jadpo config move` needs a constant",
            )
            .reason("It makes one constant a setting, so it needs to know which.")
            .next("Run `jadpo config move <constant>`, naming it as code reads it: `name`, or `library.name` for a library's"),
        );
    };
    let Some(root) = jadpo_db::find_root(&cwd) else {
        return project_not_found("config");
    };
    let mut loaded = jadpo_db::load(&root);
    let id = match setting_move::find(&loaded.db, loaded.project, written) {
        Ok(id) => id,
        Err(problem) => return Reply::failed(*problem),
    };
    // A project that doesn't check can't be moved, except for a
    // `default:` naming this constant, which the move answers.
    let problems: Vec<Diagnostic> = jadpo_check::check(&loaded.db, loaded.project)
        .into_iter()
        .filter(|problem| !setting_move::answered(&loaded.db, loaded.project, &id, problem))
        .collect();
    if !problems.is_empty() {
        return blocked(&loaded, &cwd, problems, "config stopped");
    }
    let sources = jadpo_db::source_texts(&loaded.db, loaded.project);
    let shown_root = jadpo_db::shown_root(&cwd, &loaded.root);
    let plan = match setting_move::plan(&loaded.db, loaded.project, &id) {
        Ok(plan) => plan,
        Err(refusals) => {
            let mut screen = Screen::new();
            for problem in &refusals {
                screen.push(Block::diagnostic_from(
                    problem,
                    &sources,
                    shown_root.as_deref(),
                ));
            }
            screen.push(Block::summary(
                Outcome::Failure,
                "config move refused",
                [Fact::plain("nothing changed")],
            ));
            return Reply::Report {
                screen,
                data: Json::object([("moved", false.into())]),
                diagnostics: refusals,
                sources,
            };
        }
    };
    let moved = match setting_move::apply(&mut loaded, &plan) {
        Ok(moved) => moved,
        Err(stopped) => {
            let mut screen = Screen::new();
            for problem in &stopped.problems {
                screen.push(Block::diagnostic_from(
                    problem,
                    &stopped.sources,
                    shown_root.as_deref(),
                ));
            }
            let errors = stopped.problems.len();
            screen.push(Block::summary(
                Outcome::Failure,
                "config move stopped",
                [
                    Fact::problem(format!(
                        "{errors} {} after the move",
                        if errors == 1 { "error" } else { "errors" }
                    )),
                    Fact::plain("nothing changed"),
                ],
            ));
            return Reply::Report {
                screen,
                data: Json::object([("moved", false.into())]),
                diagnostics: stopped.problems,
                sources: stopped.sources,
            };
        }
    };
    if let Err(problem) = write_moved(&root, &moved) {
        return Reply::failed(*problem);
    }
    Reply::screen(moved_screen(&plan, &moved), moved_data(&plan, &moved))
}

/// Writes what the move changed, deleting a file it emptied.
fn write_moved(root: &Path, moved: &setting_move::Moved) -> Result<(), Box<Diagnostic>> {
    for change in &moved.files {
        let path = root.join(&change.path);
        let written = match &change.after {
            Some(text) => path
                .parent()
                .map_or(Ok(()), std::fs::create_dir_all)
                .and_then(|()| std::fs::write(&path, text)),
            None => std::fs::remove_file(&path).map(|()| {
                // A folder the file leaves empty goes too, as git sees it.
                if let Some(folder) = path.parent().filter(|folder| *folder != root) {
                    let _ = std::fs::remove_dir(folder);
                }
            }),
        };
        if let Err(error) = written {
            return Err(Box::new(
                Diagnostic::new(
                    &rule::TOOLCHAIN_ARTIFACT_WRITE_FAILED,
                    format!("`{}` couldn't be written", change.path),
                )
                .reason(format!("Writing it failed: {error}. The files before it in the move were written."))
                .next_for(
                    "Check the folder can be written, then restore the files with git and run `jadpo config move` again",
                    jadpo_diag::Decider::Human,
                ),
            ));
        }
    }
    Ok(())
}

fn mark_of(mark: setting_move::Mark) -> ChangeMark {
    match mark {
        setting_move::Mark::Added => ChangeMark::Added,
        setting_move::Mark::Changed => ChangeMark::Changed,
        setting_move::Mark::Removed => ChangeMark::Removed,
    }
}

fn mark_word(mark: setting_move::Mark) -> &'static str {
    match mark {
        setting_move::Mark::Added => "added",
        setting_move::Mark::Changed => "changed",
        setting_move::Mark::Removed => "removed",
    }
}

/// The change list, file by file, then the one status line.
fn moved_screen(plan: &setting_move::Plan, moved: &setting_move::Moved) -> Screen {
    let groups = moved
        .files
        .iter()
        .filter(|change| !change.rows.is_empty())
        .map(|change| {
            (
                change.path.clone(),
                change
                    .rows
                    .iter()
                    .map(|row| {
                        (
                            mark_of(row.mark),
                            vec![row.subject.clone(), row.note.clone()],
                        )
                    })
                    .collect(),
            )
        })
        .collect();
    Screen::new()
        .with(Block::Changes { note: None, groups })
        .with(Block::Status {
            outcome: Outcome::Success,
            text: format!("Made {} a setting", plan.shown),
            rows: vec![
                ("setting".to_string(), format!("config.{}", plan.setting)),
                ("variable".to_string(), plan.variable.clone()),
            ],
        })
}

fn moved_data(plan: &setting_move::Plan, moved: &setting_move::Moved) -> Json {
    let files: Vec<Json> = moved
        .files
        .iter()
        .map(|change| {
            let state = match (&change.before, &change.after) {
                (None, _) => "added",
                (_, None) => "removed",
                _ => "changed",
            };
            let rows: Vec<Json> = change
                .rows
                .iter()
                .map(|row| {
                    Json::object([
                        ("change", Json::from(mark_word(row.mark))),
                        ("subject", Json::from(row.subject.as_str())),
                        ("note", Json::from(row.note.as_str())),
                    ])
                })
                .collect();
            Json::object([
                ("path", Json::from(change.path.as_str())),
                ("change", Json::from(state)),
                ("rows", Json::array(rows)),
            ])
        })
        .collect();
    let reads: Vec<Json> = plan
        .reads
        .iter()
        .map(|read| {
            Json::object([
                ("path", Json::from(read.path.as_str())),
                ("line", Json::from(i64::from(read.line))),
            ])
        })
        .collect();
    Json::object([
        ("moved", true.into()),
        ("constant", Json::from(plan.shown.as_str())),
        ("setting", Json::from(plan.setting.as_str())),
        ("variable", Json::from(plan.variable.as_str())),
        ("default", Json::from(plan.default.as_str())),
        ("reads", Json::array(reads)),
        ("files", Json::array(files)),
    ])
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
