//! Parsing the command line, dispatching through the table, and writing the
//! result in the stream's mode. Pure: the binary only supplies the
//! environment and prints the [`Output`].

use std::collections::BTreeMap;
use std::path::PathBuf;

use jadpo_diag::{Diagnostic, rule};
use jadpo_term::{Block, ColorChoice, Environment, Json, Screen, Stream, Style, envelope};

use crate::completion;
use crate::help;
use crate::suggest::nearest;
use crate::table::{self, COLORS, Command, Flag, GLOBAL_FLAGS};
use crate::watch::WatchArgs;

/// What one run of `jadpo` writes and how it exits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Output {
    pub stdout: String,
    pub stderr: String,
    pub code: u8,
    /// A session the binary runs on the real streams after writing the
    /// output, such as the language server.
    pub session: Option<Session>,
}

/// A long-running command that owns standard input and output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Session {
    /// `jadpo lsp`, started in this folder.
    Lsp(PathBuf),
    /// `jadpo test --watch`, until it is stopped.
    Watch(Box<WatchArgs>),
    /// `jadpo dev`, until it is stopped.
    Dev(Box<crate::dev::DevArgs>),
    /// `jadpo config set`, which asks for the value at a prompt.
    ConfigSet(Box<crate::config::SetArgs>),
}

/// Everything a command's `run` function receives.
#[derive(Debug)]
pub struct Call {
    /// Positional arguments after the command name.
    pub arguments: Vec<String>,
    /// The command's own flags that were given (`--check`).
    pub flags: Vec<&'static str>,
    /// The command's own flags that take a value, with the value given
    /// (`--seed 48213`); each is given at most once.
    pub values: Vec<(&'static str, String)>,
    /// The folder the command runs in.
    pub cwd: Option<PathBuf>,
    /// What it runs under: the process's variables and terminals.
    pub environment: Environment,
}

/// Whether the command runs in CI: the `CI` variable set to anything but
/// empty, as CI services set it (area 14). A package linked to a live
/// checkout is refused there (area 11).
pub(crate) fn in_ci(environment: &Environment) -> bool {
    environment.var("CI").is_some_and(|value| !value.is_empty())
}

impl Call {
    pub fn flag(&self, name: &str) -> bool {
        self.flags.contains(&name)
    }

    /// The value a flag that takes one was given.
    pub fn value(&self, name: &str) -> Option<&str> {
        self.values
            .iter()
            .rev()
            .find(|(flag, _)| *flag == name)
            .map(|(_, value)| value.as_str())
    }
}

/// What a command produced.
#[derive(Debug)]
pub enum Reply {
    /// Success: text for people (a screen, or exact bytes such as a script)
    /// and data for `--json`.
    Done {
        text: Text,
        data: Json,
    },
    Failed(Box<Diagnostic>),
    /// A result about the project's files: a screen of diagnostics and a
    /// summary, the data for `--json`, and the diagnostics with the sources
    /// their frames and line numbers come from. It fails when any
    /// diagnostic is an error.
    Report {
        screen: Screen,
        data: Json,
        diagnostics: Vec<Diagnostic>,
        sources: BTreeMap<String, String>,
    },
    /// A session that takes over standard input and output.
    Session(Session),
}

#[derive(Debug)]
pub enum Text {
    Screen(Screen),
    Raw(String),
}

impl Reply {
    pub fn screen(screen: Screen, data: Json) -> Reply {
        Reply::Done {
            text: Text::Screen(screen),
            data,
        }
    }

    pub fn raw(text: impl Into<String>, data: Json) -> Reply {
        Reply::Done {
            text: Text::Raw(text.into()),
            data,
        }
    }

    pub fn failed(diagnostic: Diagnostic) -> Reply {
        Reply::Failed(Box::new(diagnostic))
    }
}

/// Exit codes: 0 done, 1 the command ran and failed, 2 the command line
/// itself was wrong.
fn exit_code(diagnostic: &Diagnostic) -> u8 {
    if diagnostic.rule.id.starts_with("cli.") {
        2
    } else {
        1
    }
}

/// Runs `jadpo` with `args` (without the program name).
pub fn run(args: &[String], env: &Environment) -> Output {
    if args.first().map(String::as_str) == Some(completion::CALLBACK) {
        let words = match args.get(1).map(String::as_str) {
            Some("--") => &args[2..],
            _ => &args[1..],
        };
        return Output {
            stdout: completion::candidates(words, env.cwd.as_deref()),
            stderr: String::new(),
            code: 0,
            session: None,
        };
    }
    let (json, choice) = presentation(args);
    let (name, reply) = match parse(args) {
        Ok(parsed) => dispatch(parsed, env),
        Err(diagnostic) => (None, Reply::Failed(diagnostic)),
    };
    finish(name, reply, env, json, choice)
}

fn finish_report(
    name: Option<&str>,
    report: (Screen, Json, Vec<Diagnostic>, BTreeMap<String, String>),
    env: &Environment,
    json: bool,
    choice: ColorChoice,
) -> Output {
    let (screen, data, diagnostics, sources) = report;
    let code = diagnostics.iter().map(exit_code).max().unwrap_or(0);
    let stdout = if json {
        envelope(name, Some(data), &diagnostics, &sources).to_document()
    } else {
        screen.render(&Style::detect(env, false, choice, Stream::Stdout))
    };
    Output {
        stdout,
        stderr: String::new(),
        code,
        session: None,
    }
}

pub(crate) fn finish(
    name: Option<&str>,
    reply: Reply,
    env: &Environment,
    json: bool,
    choice: ColorChoice,
) -> Output {
    let (data, diagnostic, text) = match reply {
        // The session writes everything itself, in the form asked for.
        Reply::Session(Session::Watch(mut args)) => {
            args.json = json;
            args.choice = choice;
            return Output {
                stdout: String::new(),
                stderr: String::new(),
                code: 0,
                session: Some(Session::Watch(args)),
            };
        }
        Reply::Session(Session::ConfigSet(mut args)) => {
            if json {
                // A prompt has no JSON form: `config check` does the reading.
                let refusal = Diagnostic::new(
                    &rule::CLI_CONFIG_ARGUMENTS,
                    "`jadpo config set` can't write JSON",
                )
                .reason("The value is typed at a prompt, and nothing about it may be printed, so there is no document to write.")
                .next("Leave out `--json`; `jadpo config check --json` lists how each setting stands");
                return Output {
                    stdout: envelope(name, None, &[refusal], &()).to_document(),
                    stderr: String::new(),
                    code: 2,
                    session: None,
                };
            }
            args.choice = choice;
            return Output {
                stdout: String::new(),
                stderr: String::new(),
                code: 0,
                session: Some(Session::ConfigSet(args)),
            };
        }
        Reply::Session(Session::Dev(mut args)) => {
            args.json = json;
            args.choice = choice;
            return Output {
                stdout: String::new(),
                stderr: String::new(),
                code: 0,
                session: Some(Session::Dev(args)),
            };
        }
        Reply::Session(session) => {
            // A person who starts it by hand learns what it is waiting for;
            // an editor, which pipes standard input, sees nothing.
            let stderr = if env.stdin_terminal {
                let header = Block::header(
                    format!("jadpo {}", name.unwrap_or("lsp")),
                    ["waiting for an editor on standard input", "ctrl-c stops it"],
                );
                Screen::new().with(header).render(&Style::detect(
                    env,
                    false,
                    choice,
                    Stream::Stderr,
                ))
            } else {
                String::new()
            };
            return Output {
                stdout: String::new(),
                stderr,
                code: 0,
                session: Some(session),
            };
        }
        Reply::Done { text, data } => (Some(data), None, Some(text)),
        Reply::Failed(diagnostic) => (None, Some(*diagnostic), None),
        Reply::Report {
            screen,
            data,
            diagnostics,
            sources,
        } => {
            return finish_report(
                name,
                (screen, data, diagnostics, sources),
                env,
                json,
                choice,
            );
        }
    };
    let code = diagnostic.as_ref().map_or(0, exit_code);
    if json {
        let diagnostics: Vec<Diagnostic> = diagnostic.into_iter().collect();
        return Output {
            stdout: envelope(name, data, &diagnostics, &()).to_document(),
            stderr: String::new(),
            code,
            session: None,
        };
    }
    let mut output = Output {
        stdout: String::new(),
        stderr: String::new(),
        code,
        session: None,
    };
    match text {
        Some(Text::Raw(text)) => output.stdout = text,
        Some(Text::Screen(screen)) => {
            output.stdout = screen.render(&Style::detect(env, false, choice, Stream::Stdout))
        }
        None => {}
    }
    if let Some(diagnostic) = diagnostic {
        // A command-line mistake is not a result, so it goes to stderr;
        // `--json` keeps everything on stdout.
        let screen = Screen::new().with(Block::diagnostic(&diagnostic, &()));
        output.stderr = screen.render(&Style::detect(env, false, choice, Stream::Stderr));
    }
    output
}

/// The live line a slow command shows on stderr while it works: `check`
/// and `test` (not `--watch`, which draws its own, nor `--help` or
/// `--json`), and only where stderr animates.
pub fn spinner(args: &[String], env: &Environment) -> jadpo_term::Spinner {
    let (json, choice) = presentation(args);
    let style = Style::detect(env, json, choice, Stream::Stderr);
    let mut words = args.iter().take_while(|arg| *arg != "--").peekable();
    let mut command = None;
    let mut action = None;
    let mut quiet = json;
    while let Some(arg) = words.next() {
        match arg.as_str() {
            "--color" => {
                words.next_if(|value| ColorChoice::parse(value).is_some());
            }
            "--watch" | "--help" | "-h" => quiet = true,
            word if !word.starts_with('-') && command.is_none() => {
                command = table::find(word).map(|command| command.name);
            }
            word if !word.starts_with('-') && action.is_none() => action = Some(word.to_string()),
            _ => {}
        }
    }
    let doing = match (command, action.as_deref()) {
        (Some("test"), _) => "Running tests",
        (Some("check"), _) => "Checking",
        // A backfill's batches say how far they have got (chunk 5.6b).
        (Some("migrate"), Some("apply")) => "Applying migrations",
        // Against a deployment it waits for the app's tick (chunk 6.2d).
        (Some("schedules"), Some("run")) => "Running the schedule",
        // A schedule's run retried waits for the app's tick (chunk 6.12).
        (Some("deliveries"), Some("retry" | "resolve")) => "Acting on it",
        _ => "",
    };
    if quiet || doing.is_empty() {
        return jadpo_term::Spinner::start(&Style::new(jadpo_term::Mode::Plain), "");
    }
    let spinner = jadpo_term::Spinner::start(&style, doing);
    if let Ok(mut live) = LIVE.lock() {
        *live = Some(spinner.words());
    }
    spinner
}

/// What the running command's live line says, when it shows one.
static LIVE: std::sync::Mutex<Option<std::sync::Arc<std::sync::Mutex<String>>>> =
    std::sync::Mutex::new(None);

/// Changes what the live line says, when there is one: how far a slow
/// step has got.
pub(crate) fn say(words: &str) {
    let live = LIVE.lock().ok().and_then(|live| live.clone());
    if let Some(live) = live
        && let Ok(mut now) = live.lock()
    {
        *now = words.to_string();
    }
}

/// `--json` and `--color` read before anything else, so even a mistake on
/// the command line is reported in the form that was asked for.
fn presentation(args: &[String]) -> (bool, ColorChoice) {
    let mut json = false;
    let mut choice = ColorChoice::Auto;
    let mut words = args.iter().take_while(|arg| *arg != "--").peekable();
    while let Some(arg) = words.next() {
        if arg == "--json" {
            json = true;
        } else if let Some(value) = arg.strip_prefix("--color=") {
            choice = ColorChoice::parse(value).unwrap_or(choice);
        } else if arg == "--color"
            && let Some(value) = words.peek().and_then(|value| ColorChoice::parse(value))
        {
            choice = value;
        }
    }
    (json, choice)
}

#[derive(Debug)]
struct Parsed {
    command: Option<&'static Command>,
    arguments: Vec<String>,
    flags: Vec<&'static str>,
    values: Vec<(&'static str, String)>,
    help: bool,
    version: bool,
}

/// The flag of `command` called `name` that takes a value.
fn value_flag(command: Option<&'static Command>, name: &str) -> Option<&'static Flag> {
    command?
        .flags
        .iter()
        .find(|flag| flag.name == name && flag.value.is_some())
}

fn parse(args: &[String]) -> Result<Parsed, Box<Diagnostic>> {
    let mut words: Vec<String> = Vec::new();
    let mut flags: Vec<(String, Option<String>)> = Vec::new();
    let mut rest = false;
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        i += 1;
        if rest || !arg.starts_with('-') || arg == "-" {
            words.push(arg.clone());
        } else if arg == "--" {
            rest = true;
        } else {
            let (name, value) = match arg.split_once('=') {
                Some((name, value)) => (name.to_string(), Some(value.to_string())),
                None => (arg.clone(), None),
            };
            let name = if name == "-h" {
                "--help".to_string()
            } else {
                name
            };
            let current = words.first().and_then(|word| table::find(word));
            let value = match value {
                None if name == "--color"
                    && args.get(i).is_some_and(|next| !next.starts_with('-')) =>
                {
                    i += 1;
                    Some(args[i - 1].clone())
                }
                // A command's own flag that takes a value takes the next
                // word whatever it is, so `--name "-1 is rejected"` works.
                None if value_flag(current, &name).is_some() && i < args.len() => {
                    i += 1;
                    Some(args[i - 1].clone())
                }
                value => value,
            };
            flags.push((name, value));
        }
    }
    let command = match words.first() {
        Some(word) => Some(table::find(word).ok_or_else(|| Box::new(unknown_command(word)))?),
        None => None,
    };
    let mut parsed = Parsed {
        command,
        arguments: words.into_iter().skip(1).collect(),
        flags: Vec::new(),
        values: Vec::new(),
        help: false,
        version: false,
    };
    for (name, value) in flags {
        match (name.as_str(), value) {
            ("--json", None) => {}
            ("--help", None) => parsed.help = true,
            ("--version", None) if command.is_none() => parsed.version = true,
            ("--color", Some(value)) => {
                if ColorChoice::parse(&value).is_none() {
                    return Err(Box::new(bad_color(&value)));
                }
            }
            ("--color", None) => return Err(Box::new(missing_color())),
            (name, None)
                if let Some(flag) = command.and_then(|command| {
                    command
                        .flags
                        .iter()
                        .find(|flag| flag.name == name && flag.value.is_none())
                }) =>
            {
                parsed.flags.push(flag.name);
            }
            (name, Some(value)) if let Some(flag) = value_flag(command, name) => {
                if let (Some(command), Some((_, first))) = (
                    command,
                    parsed.values.iter().find(|(given, _)| *given == flag.name),
                ) {
                    return Err(Box::new(repeated_value(command, flag, first, &value)));
                }
                parsed.values.push((flag.name, value));
            }
            (name, None)
                if let (Some(command), Some(flag)) = (command, value_flag(command, name)) =>
            {
                return Err(Box::new(missing_value(command, flag)));
            }
            (name, _)
                if let Some(planned) =
                    command.and_then(|command| table::planned(command.name, name)) =>
            {
                return Err(Box::new(not_built_flag(planned)));
            }
            (_, value) => {
                let written = value.map_or_else(|| name.clone(), |value| format!("{name}={value}"));
                return Err(Box::new(unknown_flag(&written, &name, command)));
            }
        }
    }
    Ok(parsed)
}

fn dispatch(parsed: Parsed, env: &Environment) -> (Option<&'static str>, Reply) {
    let Some(command) = parsed.command else {
        if parsed.version && !parsed.help {
            let version = table::find("version").expect("version is built");
            return (
                Some(version.name),
                (version.run)(&Call {
                    arguments: Vec::new(),
                    flags: Vec::new(),
                    values: Vec::new(),
                    cwd: env.cwd.clone(),
                    environment: env.clone(),
                }),
            );
        }
        return (Some("help"), help::reference());
    };
    if parsed.help {
        // `jadpo help <command> --help` explains the command it names.
        let target = match (command.name, parsed.arguments.first()) {
            ("help", Some(word)) => match table::find(word) {
                Some(target) => target,
                None => return (Some("help"), Reply::failed(unknown_command(word))),
            },
            _ => command,
        };
        return (Some("help"), help::command(target));
    }
    if parsed.arguments.len() > command.arguments.len() {
        return (
            Some(command.name),
            Reply::failed(too_many(command, &parsed.arguments)),
        );
    }
    (
        Some(command.name),
        (command.run)(&Call {
            arguments: parsed.arguments,
            flags: parsed.flags,
            values: parsed.values,
            cwd: env.cwd.clone(),
            environment: env.clone(),
        }),
    )
}

/// `a`, `a or b`, `a, b or c`.
pub fn either(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [only] => only.clone(),
        [init @ .., last] => format!("{} or {last}", init.join(", ")),
    }
}

/// `a and b`, `a, b and c`.
pub fn both(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [only] => only.clone(),
        [init @ .., last] => format!("{} and {last}", init.join(", ")),
    }
}

pub fn quoted(items: impl IntoIterator<Item = impl AsRef<str>>) -> Vec<String> {
    items
        .into_iter()
        .map(|item| format!("`{}`", item.as_ref()))
        .collect()
}

pub fn unknown_command(word: &str) -> Diagnostic {
    let diagnostic = Diagnostic::new(
        &rule::CLI_UNKNOWN_COMMAND,
        format!("`{word}` isn't a jadpo command"),
    );
    match nearest(word, table::all_names()) {
        Some(near) => diagnostic
            .reason(format!(
                "Jadpo has no command with this name. Did you mean `{near}`?"
            ))
            .next(format!("Use `{near}` instead of `{word}`")),
        None => diagnostic
            .reason("Jadpo has no command with this name.")
            .next("Run `jadpo help` to see every command"),
    }
}

fn unknown_flag(written: &str, name: &str, command: Option<&Command>) -> Diagnostic {
    let flags = GLOBAL_FLAGS
        .iter()
        .chain(command.map_or(&[][..], |command| command.flags))
        .map(|flag| flag.name);
    let (message, subject, help) = match command {
        Some(command) => (
            format!("`{written}` isn't a flag of `jadpo {}`", command.name),
            format!("`jadpo {}`", command.name),
            format!("Run `jadpo {} --help` to see every flag", command.name),
        ),
        None => (
            format!("`{written}` isn't a jadpo flag"),
            "Jadpo".to_string(),
            "Run `jadpo --help` to see every flag".to_string(),
        ),
    };
    let diagnostic = Diagnostic::new(&rule::CLI_UNKNOWN_FLAG, message);
    match nearest(name, flags) {
        Some(near) => diagnostic
            .reason(format!(
                "{subject} has no flag with this name. Did you mean `{near}`?"
            ))
            .next(format!("Use `{near}` instead of `{written}`")),
        None => diagnostic
            .reason(format!("{subject} has no flag with this name."))
            .next(help),
    }
}

/// A decided flag that isn't built yet.
fn not_built_flag(planned: &table::Planned) -> Diagnostic {
    Diagnostic::new(
        &rule::TOOLCHAIN_NOT_BUILT_YET,
        format!("{} aren't built yet", planned.what),
    )
    .reason(planned.reason)
    .next(format!(
        "Leave out `{}`; `jadpo {} --help` lists what is built",
        planned.name, planned.command
    ))
    .help_at(planned.anchor)
}

/// A flag that takes one value, given more than once: which one was meant
/// can't be known, so neither is guessed.
fn repeated_value(command: &Command, flag: &Flag, first: &str, second: &str) -> Diagnostic {
    Diagnostic::new(
        command.arguments_rule,
        format!("`{}` is given more than once", flag.name),
    )
    .reason(format!(
        "`{}` takes one value, and this command line gives it `{first}` and then `{second}`.",
        flag.name
    ))
    .next(format!("Keep one `{}`", flag.name))
}

/// What a flag's value is called in a sentence: "some text", "a number".
fn value_noun(placeholder: &str) -> String {
    match placeholder {
        "text" => "some text".to_string(),
        "n" => "a number".to_string(),
        word if word.starts_with(['a', 'e', 'i', 'o', 'u']) => format!("an {word}"),
        word => format!("a {word}"),
    }
}

fn missing_value(command: &Command, flag: &Flag) -> Diagnostic {
    let placeholder = flag.value.unwrap_or("value");
    let noun = value_noun(placeholder);
    let values: Vec<String> = flag
        .values
        .iter()
        .map(|value| value.name.to_string())
        .collect();
    let reason = if values.is_empty() {
        format!("`{}` takes {noun}.", flag.name)
    } else {
        format!("`{}` takes {}.", flag.name, either(&quoted(&values)))
    };
    Diagnostic::new(
        command.arguments_rule,
        format!("`{}` needs {noun}", flag.name),
    )
    .reason(reason)
    .next(format!(
        "Give it after the flag: `jadpo {} {} <{placeholder}>`",
        command.name, flag.name
    ))
}

fn color_values() -> (String, String) {
    let names: Vec<String> = COLORS.iter().map(|value| value.name.to_string()).collect();
    let flags = quoted(COLORS.iter().map(|value| format!("--color={}", value.name)));
    (either(&names), either(&flags))
}

fn bad_color(value: &str) -> Diagnostic {
    let (names, flags) = color_values();
    let diagnostic = Diagnostic::new(
        &rule::CLI_PRESENTATION_ARGUMENTS,
        format!("`{value}` isn't a colour setting"),
    );
    match nearest(value, COLORS.iter().map(|value| value.name)) {
        Some(near) => diagnostic
            .reason(format!("`--color` takes {names}. Did you mean `{near}`?"))
            .next(format!("Use `--color={near}`")),
        None => diagnostic
            .reason(format!("`--color` takes {names}."))
            .next(format!("Use {flags}")),
    }
}

fn missing_color() -> Diagnostic {
    let (names, flags) = color_values();
    Diagnostic::new(&rule::CLI_PRESENTATION_ARGUMENTS, "`--color` needs a value")
        .reason(format!("`--color` takes {names}."))
        .next(format!("Use {flags}"))
}

fn too_many(command: &Command, arguments: &[String]) -> Diagnostic {
    let takes = match command.arguments.len() {
        0 => "takes no arguments".to_string(),
        1 => "takes one argument".to_string(),
        n => format!("takes {n} arguments"),
    };
    let kept: Vec<&str> = arguments
        .iter()
        .take(command.arguments.len())
        .map(String::as_str)
        .collect();
    let run = std::iter::once(format!("jadpo {}", command.name))
        .chain(kept.iter().map(|word| word.to_string()))
        .collect::<Vec<_>>()
        .join(" ");
    Diagnostic::new(
        command.arguments_rule,
        format!("`jadpo {}` {takes}", command.name),
    )
    .reason(format!("It was given `{}`.", arguments.join(" ")))
    .next(format!("Run `{run}`"))
}
