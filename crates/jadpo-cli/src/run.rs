//! Parsing the command line, dispatching through the table, and writing the
//! result in the stream's mode. Pure: the binary only supplies the
//! environment and prints the [`Output`].

use std::collections::BTreeMap;
use std::path::PathBuf;

use jadpo_diag::{Diagnostic, Severity, rule};
use jadpo_term::{Block, ColorChoice, Environment, Json, Screen, Stream, Style, envelope};

use crate::completion;
use crate::help;
use crate::suggest::nearest;
use crate::table::{self, COLORS, Command, GLOBAL_FLAGS};

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
}

/// Everything a command's `run` function receives.
#[derive(Debug)]
pub struct Call {
    /// Positional arguments after the command name.
    pub arguments: Vec<String>,
    /// The command's own flags that were given (`--check`).
    pub flags: Vec<&'static str>,
    /// The folder the command runs in.
    pub cwd: Option<PathBuf>,
}

impl Call {
    pub fn flag(&self, name: &str) -> bool {
        self.flags.contains(&name)
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
    let code = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity() == Severity::Error)
        .map(exit_code)
        .max()
        .unwrap_or(0);
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

fn finish(
    name: Option<&str>,
    reply: Reply,
    env: &Environment,
    json: bool,
    choice: ColorChoice,
) -> Output {
    let (data, diagnostic, text) = match reply {
        Reply::Session(session) => {
            return Output {
                stdout: String::new(),
                stderr: String::new(),
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
    help: bool,
    version: bool,
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
            let value = match value {
                None if name == "--color"
                    && args.get(i).is_some_and(|next| !next.starts_with('-')) =>
                {
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
                    cwd: env.cwd.clone(),
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
            cwd: env.cwd.clone(),
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
