//! `jadpo package new <Name> --library` or `--provider` (area 11,
//! "Developing and publishing a package"; area 14, "`jadpo package new`"):
//! starts a package in its own repository, a folder named as the registry
//! will name it. It writes the package format with one small working
//! example, `recipes/`, a `sample/` app, `GUIDE.md` and the release's
//! `CHANGELOG.md` and `surface.jadpo_record`, so it passes `jadpo package
//! check` as written. What it writes is compiled before it reports, and a
//! name it can't work under leaves nothing behind.
//!
//! The screen is `jadpo new`'s: one status line with what was made, then the
//! one command to run next.
//!
//! ```text
//! ✓ Created postcodes
//!     package    Postcodes.v1, library
//!     publisher  @acme
//!     licence    proprietary, publishing needs an open one
//!     jadpo      X.Y
//!     files      16 written
//!
//! → cd postcodes && jadpo package check  runs the publishing bar on it
//! ```
//!
//! Without `--publisher` it asks on a terminal, and leaves the key out
//! elsewhere (`publisher  left out, publishing needs it`). The licence is
//! always `Licence.proprietary` to start with: which rights to give others is
//! the author's to say, so it isn't chosen for them.

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

use jadpo_check::jadpo_version::this_line;
use jadpo_check::packages::is_publisher_handle;
use jadpo_check::publishing;
use jadpo_diag::{Diagnostic, rule};
use jadpo_registry::names::{Handle, ScopedName};
use jadpo_term::{
    Block, ColorChoice, Environment, Json, Mode, Outcome, Screen, Stream, Style, Symbol,
};

use crate::new::{Written, inspect, shell_word};
use crate::package::arguments;
use crate::package_recipe::in_sample;
use crate::run::{Call, Reply, Session};

// Each scaffold's files: `PACKAGE_COMMON`, `PACKAGE_LIBRARY` and
// `PACKAGE_PROVIDER`, each path with its text, from `package_scaffold/`
// (build.rs). `{name}`, `{snake}`, `{kebab}`, `{line}` and `{publisher}` are
// filled in as they are written.
include!(concat!(env!("OUT_DIR"), "/package_common.rs"));
include!(concat!(env!("OUT_DIR"), "/package_library.rs"));
include!(concat!(env!("OUT_DIR"), "/package_provider.rs"));

/// What a new package is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Library,
    Provider,
}

impl Kind {
    fn word(self) -> &'static str {
        match self {
            Kind::Library => "library",
            Kind::Provider => "provider",
        }
    }

    fn files(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Kind::Library => PACKAGE_LIBRARY,
            Kind::Provider => PACKAGE_PROVIDER,
        }
    }
}

/// A package to start, once its arguments are known to be good.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewArgs {
    /// Its name in code, `AcmeCouriers`.
    pub code: String,
    /// The folder it goes in, as the registry names it: `acme-couriers`.
    pub folder: String,
    /// That folder, whole.
    pub root: PathBuf,
    pub kind: Kind,
    pub choice: ColorChoice,
}

const USAGE: &str =
    "Run `jadpo package new <Name> --library` or `jadpo package new <Name> --provider`";

/// A handle no package is named with, for asking what a name comes to.
const ANY_HANDLE: &str = "@any";

pub(crate) fn new(call: &Call) -> Reply {
    let code = match call.arguments.as_slice() {
        [_, code] => code.as_str(),
        [_] => {
            return Reply::failed(arguments(
                "`jadpo package new` needs a name for the package".to_string(),
                "A package is named in UpperCamelCase, such as `AcmeCouriers`, and it makes a folder for it.".to_string(),
                USAGE.to_string(),
            ));
        }
        [_, _, extra, ..] => {
            return Reply::failed(arguments(
                format!("`jadpo package new` takes one name, not `{extra}` as well"),
                "It starts one package, in a folder of its own.".to_string(),
                USAGE.to_string(),
            ));
        }
        [] => unreachable!("`new` is the first argument"),
    };
    let kind = match (call.flag("--library"), call.flag("--provider")) {
        (true, false) => Kind::Library,
        (false, true) => Kind::Provider,
        (true, true) => {
            return Reply::failed(arguments(
                "A package can't be both `--library` and `--provider`".to_string(),
                "A package that describes an API is a provider; any other is a library."
                    .to_string(),
                USAGE.to_string(),
            ));
        }
        (false, false) => {
            return Reply::failed(arguments(
                "`jadpo package new` needs `--library` or `--provider`".to_string(),
                "A package that describes an API is a provider; any other is a library, code an app calls.".to_string(),
                USAGE.to_string(),
            ));
        }
    };
    let Some(folder) = folder_of(code) else {
        return Reply::failed(arguments(
            format!("`{code}` isn't a package's name"),
            "A package is named in UpperCamelCase, words of a capital and lower-case letters and digits, such as `AcmeCouriers`, in at most 39 letters and dashes as the registry writes it. It starts at `v1`, so the name goes without a version.".to_string(),
            USAGE.to_string(),
        ));
    };
    let publisher = call.value("--publisher");
    if let Some(handle) = publisher
        && !is_publisher_handle(handle)
    {
        return Reply::failed(arguments(
            format!("`{handle}` isn't a publisher's handle"),
            HANDLE.to_string(),
            format!(
                "Run `jadpo package new {code} --{} --publisher @<your handle>`",
                kind.word()
            ),
        ));
    }
    let cwd = call.cwd.clone().unwrap_or_else(|| PathBuf::from("."));
    let root = cwd.join(&folder);
    if let Err(diagnostic) = inspect("package new", "package", &folder, &root) {
        return Reply::failed(*diagnostic);
    }
    let args = NewArgs {
        code: code.to_string(),
        folder,
        root,
        kind,
        choice: ColorChoice::Auto,
    };
    let environment = &call.environment;
    if publisher.is_none() && environment.stdin_terminal && environment.stderr_terminal {
        return Reply::Session(Session::PackageNew(Box::new(args)));
    }
    write(&args, publisher)
}

/// What a handle is, for whoever typed something else.
const HANDLE: &str = "A handle is `@` and lower-case letters and digits, starting with a letter, with single `-`s between them.";

/// The folder a package named `code` goes in, as the registry writes the name;
/// `None` when `code` is no package's name.
fn folder_of(code: &str) -> Option<String> {
    let upper_camel = code.starts_with(|first: char| first.is_ascii_uppercase())
        && code.chars().all(|letter| letter.is_ascii_alphanumeric())
        && jadpo_db::to_upper_camel(&jadpo_db::to_snake(code)) == code;
    if !upper_camel {
        return None;
    }
    let handle = Handle::parse(ANY_HANDLE).ok()?;
    let name = ScopedName::of_code(&handle, code)?;
    Some(name.name().to_string())
}

/// `jadpo package new` on a terminal: asks who publishes the package, then
/// writes it.
pub fn serve(args: &NewArgs, env: &Environment) -> u8 {
    let style = Style::detect(env, false, args.choice, Stream::Stderr);
    let publisher = ask_publisher(&mut std::io::stdin().lock(), &mut std::io::stderr(), &style);
    let output = crate::run::finish(
        Some("package"),
        write(args, publisher.as_deref()),
        env,
        false,
        args.choice,
    );
    let _ = std::io::stdout().write_all(output.stdout.as_bytes());
    let _ = std::io::stderr().write_all(output.stderr.as_bytes());
    output.code
}

/// Asks who publishes the package until it has a handle or nothing: enter, or
/// the input ending, leaves the key out. Answered, the prompt collapses to
/// `✓ question  answer` where the cursor can move.
pub fn ask_publisher(
    input: &mut impl BufRead,
    output: &mut impl Write,
    style: &Style,
) -> Option<String> {
    const QUESTION: &str = "Who publishes this package?";
    let paint = |symbol: Symbol, text: &str| match (style.color, symbol.token()) {
        (Some(depth), Some(token)) => {
            format!("\x1b[{}m{text}\x1b[0m", token.sgr(depth, style.theme))
        }
        _ => text.to_string(),
    };
    let muted = |text: &str| paint(Symbol::Separator, text);
    let glyph = |symbol: Symbol| paint(symbol, symbol.text(style.symbols));
    let question = if style.bold {
        format!("\x1b[1m{QUESTION}\x1b[0m")
    } else {
        QUESTION.to_string()
    };
    let answer = loop {
        let _ = write!(
            output,
            "{} {question}  {}  ",
            glyph(Symbol::Prompt),
            muted("a handle such as @acme; enter leaves it out")
        );
        let _ = output.flush();
        let mut line = String::new();
        match input.read_line(&mut line) {
            Ok(0) | Err(_) => {
                let _ = writeln!(output);
                break None;
            }
            Ok(_) => {}
        }
        let typed = line.trim();
        if typed.is_empty() {
            break None;
        }
        if is_publisher_handle(typed) {
            break Some(typed.to_string());
        }
        let _ = writeln!(
            output,
            "{} `{typed}` isn't a publisher's handle. {HANDLE}",
            glyph(Symbol::Failure)
        );
    };
    if style.mode == Mode::Rich {
        // The line up, wiped, and the record of what was answered in its place.
        let _ = writeln!(
            output,
            "\x1b[1A\r\x1b[2K{} {QUESTION}  {}",
            glyph(Symbol::Success),
            answer.as_deref().unwrap_or("left out")
        );
    }
    answer
}

/// Writes the package, naming `publisher` when there is one.
pub fn write(args: &NewArgs, publisher: Option<&str>) -> Reply {
    let (major, minor) = this_line();
    let line = format!("{major}.{minor}");
    let snake = jadpo_db::to_snake(&args.code);
    let key = publisher.map_or_else(String::new, |handle| {
        format!("    publisher: \"{handle}\"\n")
    });
    let fill = |template: &str| {
        template
            .replace("{name}", &args.code)
            .replace("{snake}", &snake)
            .replace("{kebab}", &args.folder)
            .replace("{line}", &line)
            .replace("{publisher}", &key)
    };
    let mut written = Written {
        files: Vec::new(),
        folders: Vec::new(),
        root: args.root.clone(),
    };
    let refuse = |written: &Written, diagnostic: Diagnostic| {
        written.undo();
        Reply::failed(diagnostic)
    };
    let write_failed = |path: &str, error: &dyn std::fmt::Display| {
        Diagnostic::new(
            &rule::TOOLCHAIN_SCAFFOLD_WRITE_FAILED,
            format!("`{path}` couldn't be written"),
        )
        .reason(format!(
            "Writing it failed: {error}. `jadpo package new` removed what it had written, so `{}` is as it was.",
            args.folder
        ))
        .next(format!(
            "Check that `{}` can be written, then run `jadpo package new` again",
            args.folder
        ))
    };
    if let Err(error) = written.folder(&args.root) {
        return refuse(&written, write_failed(&args.folder, &error));
    }
    for (path, template) in PACKAGE_COMMON.iter().chain(args.kind.files()) {
        let mut text = fill(template);
        // How a line breaks depends on how long the name is. What doesn't
        // parse is turned down below, by compiling it.
        if path.ends_with(".jadpo")
            && let Ok(formatted) = jadpo_fmt::format_file(&text)
        {
            text = formatted;
        }
        if let Err(error) = written.file(path, &text) {
            return refuse(&written, write_failed(path, &error));
        }
    }
    let surface = match surface_of(&args.root, &args.code) {
        Ok(surface) => surface,
        Err(why) => return refuse(&written, unusable(&args.code, &why)),
    };
    if let Err(error) = written.file(jadpo_db::record::SURFACE_FILE, &surface) {
        return refuse(
            &written,
            write_failed(jadpo_db::record::SURFACE_FILE, &error),
        );
    }
    done(args, publisher, &line, &written)
}

/// The surface of the package just written, or the first thing wrong with it.
fn surface_of(root: &Path, code: &str) -> Result<String, String> {
    let compiled = match crate::package::compile(root, "package new") {
        Ok(compiled) => compiled,
        Err(_) => return Err(format!("`{code}` couldn't be compiled")),
    };
    if let Some(problem) = compiled.problems.first() {
        return Err(problem.message.clone());
    }
    if let Some(problem) =
        in_sample(root, &compiled).and_then(|ran| ran.problems.into_iter().next())
    {
        return Err(problem.message);
    }
    let surface = compiled
        .surface()
        .ok_or_else(|| format!("`{code}` has no surface"))?;
    let bump = publishing::version(&compiled.label, None, &surface.text(None))
        .map_err(|_| "its first version couldn't be worked out".to_string())?;
    Ok(surface.text(Some(&bump.version)))
}

/// A name the package can't compile under, such as a standard type's.
fn unusable(code: &str, why: &str) -> Diagnostic {
    arguments(
        format!("`{code}` can't name a package"),
        format!(
            "The package it writes doesn't compile under that name: {}. It wrote nothing.",
            why.trim_end_matches('.')
        ),
        "Choose another name".to_string(),
    )
}

fn done(args: &NewArgs, publisher: Option<&str>, line: &str, written: &Written) -> Reply {
    let mut files = written.files.clone();
    files.sort();
    let package = format!("{}.v1", args.code);
    let mut screen = Screen::new();
    screen.push(Block::Status {
        outcome: Outcome::Success,
        text: format!("Created {}", args.folder),
        rows: vec![
            (
                "package".to_string(),
                format!("{package}, {}", args.kind.word()),
            ),
            (
                "publisher".to_string(),
                publisher.map_or_else(
                    || "left out, publishing needs it".to_string(),
                    str::to_string,
                ),
            ),
            (
                "licence".to_string(),
                "proprietary, publishing needs an open one".to_string(),
            ),
            ("jadpo".to_string(), line.to_string()),
            ("files".to_string(), format!("{} written", files.len())),
        ],
    });
    screen.push(Block::hint(
        format!("cd {} && jadpo package check", shell_word(&args.folder)),
        "runs the publishing bar on it",
    ));
    Reply::screen(
        screen,
        Json::object([
            ("path", Json::from(args.folder.as_str())),
            (
                "root",
                Json::from(
                    args.root
                        .canonicalize()
                        .unwrap_or_else(|_| args.root.clone())
                        .to_string_lossy()
                        .as_ref(),
                ),
            ),
            ("package", Json::from(package.as_str())),
            ("kind", Json::from(args.kind.word())),
            ("publisher", publisher.map_or(Json::Null, Json::from)),
            ("licence", Json::from("proprietary")),
            ("jadpo", Json::from(line)),
            (
                "files",
                Json::array(files.iter().map(|file| Json::from(file.as_str()))),
            ),
        ]),
    )
}
