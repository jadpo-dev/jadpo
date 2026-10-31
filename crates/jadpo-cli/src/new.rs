//! `jadpo new <name>` (area 14, "`jadpo new`"): writes a project into a
//! folder that doesn't exist yet or is empty. The project is area 1's
//! fixed root, on this jadpo's own release line, with one small domain:
//! a route, the action it runs, its line in root policy, its API record
//! and a route test, so it passes `check`, `fmt --check` and `test` as
//! soon as it is written. The record is written by the same code as
//! `jadpo api update`, from the routes just written, so it can't drift
//! from them.
//!
//! The screen is one status line with what was made, then the one
//! command to run next. A folder that holds anything is refused, and a
//! write that fails partway removes what this run wrote.

use std::fs;
use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};

use jadpo_check::jadpo_version::this_line;
use jadpo_check::routes::record;
use jadpo_diag::{Diagnostic, rule};
use jadpo_term::{Block, Json, Outcome, Screen};

use crate::run::{Call, Reply};

// The project's files: `SCAFFOLD`, each project path with its text, from
// `scaffold/` (build.rs). `{line}`, `{name}` and `{suggestions}` are
// filled in as it is written.
include!(concat!(env!("OUT_DIR"), "/scaffold.rs"));

/// The fixed root's folders that start empty.
const EMPTY_FOLDERS: &[&str] = &["database", "tests"];

pub fn new(call: &Call) -> Reply {
    let typed = match call.arguments.as_slice() {
        [typed] => typed.as_str(),
        [] => {
            return Reply::failed(arguments(
                "`jadpo new` needs a name for the project".to_string(),
                "It makes a folder of that name holding the project.".to_string(),
            ));
        }
        [_, extra, ..] => {
            return Reply::failed(arguments(
                format!("`jadpo new` takes one name, not `{extra}` as well"),
                "It makes one project, in the folder its name gives.".to_string(),
            ));
        }
    };
    let cwd = call.cwd.clone().unwrap_or_else(|| PathBuf::from("."));
    let destination = cwd.join(typed);
    let Some(name) = folder_name(typed, &destination) else {
        return Reply::failed(
            Diagnostic::new(
                &rule::TOOLCHAIN_SCAFFOLD_NAME_INVALID,
                format!("`{typed}` doesn't name a folder for the project"),
            )
            .reason("`jadpo new` makes the project in the folder its argument names, and names the project after that folder, so the argument ends in a folder's name.")
            .next("Give the project's name, for example `jadpo new my-app`"),
        );
    };
    if let Err(diagnostic) = inspect(typed, &destination) {
        return Reply::failed(*diagnostic);
    }
    let line = {
        let (major, minor) = this_line();
        format!("{major}.{minor}")
    };
    let mut written = Written {
        files: Vec::new(),
        folders: Vec::new(),
        root: destination.clone(),
    };
    match write_project(&destination, &name, &line, &mut written) {
        Ok(routes) => done(typed, &destination, &line, &routes, &written),
        Err((path, error)) => {
            written.undo();
            Reply::failed(
                Diagnostic::new(
                    &rule::TOOLCHAIN_SCAFFOLD_WRITE_FAILED,
                    format!("`{path}` couldn't be written"),
                )
                .reason(format!(
                    "Writing it failed: {error}. `jadpo new` removed what it had written, so `{typed}` is as it was."
                ))
                .next(format!(
                    "Check that `{typed}` can be written, then run `jadpo new {}` again",
                    shell_word(typed)
                )),
            )
        }
    }
}

fn arguments(message: String, reason: String) -> Diagnostic {
    Diagnostic::new(&rule::CLI_NEW_ARGUMENTS, message)
        .reason(reason)
        .next("Run `jadpo new <name>`, for example `jadpo new my-app`")
}

/// The project's name: the last folder of the destination as typed, or,
/// for `.`, the name of the folder it runs in.
fn folder_name(typed: &str, destination: &Path) -> Option<String> {
    let path = Path::new(typed);
    let named = if path.components().next_back()? == Component::CurDir {
        destination.canonicalize().ok()?.file_name()?.to_os_string()
    } else {
        path.file_name()?.to_os_string()
    };
    named.to_str().map(str::to_string)
}

/// Whether the destination can take a project: it doesn't exist yet, or
/// is an empty folder.
fn inspect(typed: &str, destination: &Path) -> Result<(), Box<Diagnostic>> {
    let unreadable = |error: std::io::Error| {
        Box::new(
            Diagnostic::new(
                &rule::TOOLCHAIN_SCAFFOLD_READ_FAILED,
                format!("`{typed}` couldn't be looked at"),
            )
            .reason(format!(
                "`jadpo new` checks that the folder is empty before it writes, and reading it failed: {error}. It wrote nothing."
            ))
            .next(format!("Check that `{typed}` can be read, then run `jadpo new` again")),
        )
    };
    match fs::metadata(destination) {
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(unreadable(error)),
        Ok(found) if !found.is_dir() => Err(Box::new(
            Diagnostic::new(
                &rule::TOOLCHAIN_SCAFFOLD_DESTINATION_EXISTS,
                format!("`{typed}` is a file"),
            )
            .reason("`jadpo new` makes a folder holding the project, and a file already has this name. It wrote nothing.")
            .next(format!("Name a folder that doesn't exist yet, or move `{typed}` first")),
        )),
        Ok(_) => {
            let mut entries = fs::read_dir(destination).map_err(unreadable)?;
            match entries.next() {
                None => Ok(()),
                Some(Err(error)) => Err(unreadable(error)),
                Some(Ok(_)) => Err(Box::new(
                    Diagnostic::new(
                        &rule::TOOLCHAIN_SCAFFOLD_DESTINATION_NOT_EMPTY,
                        format!("`{typed}` already has files in it"),
                    )
                    .reason("`jadpo new` writes a whole project, so it writes only into a folder that doesn't exist yet or is empty. It wrote nothing.")
                    .next(format!(
                        "Name a folder that doesn't exist yet, or empty `{typed}` first"
                    )),
                )),
            }
        }
    }
}

/// What this run made, so a failure can take it away again.
struct Written {
    /// Project-relative paths of the files written, in order.
    files: Vec<String>,
    /// Folders this run made, outermost first.
    folders: Vec<PathBuf>,
    root: PathBuf,
}

impl Written {
    fn folder(&mut self, path: &Path) -> std::io::Result<()> {
        let mut missing = Vec::new();
        let mut at = Some(path);
        while let Some(folder) = at {
            if folder.exists() {
                break;
            }
            missing.push(folder.to_path_buf());
            at = folder.parent();
        }
        for folder in missing.into_iter().rev() {
            fs::create_dir(&folder)?;
            self.folders.push(folder);
        }
        Ok(())
    }

    fn file(&mut self, relative: &str, text: &str) -> std::io::Result<()> {
        let path = self.root.join(relative);
        if let Some(parent) = path.parent() {
            self.folder(parent)?;
        }
        fs::write(&path, text)?;
        self.files.push(relative.to_string());
        Ok(())
    }

    fn undo(&self) {
        for file in self.files.iter().rev() {
            let _ = fs::remove_file(self.root.join(file));
        }
        for folder in self.folders.iter().rev() {
            let _ = fs::remove_dir(folder);
        }
    }
}

fn write_project(
    root: &Path,
    name: &str,
    line: &str,
    written: &mut Written,
) -> Result<Vec<String>, (String, std::io::Error)> {
    // A token is written whenever `share` first needs one, so a machine
    // whose generator fails still gets a project, without it.
    let suggestions = crate::share::new_token()
        .map(|token| format!("\nsuggestions {{\n    project_token: \"{token}\"\n}}\n"))
        .unwrap_or_default();
    written
        .folder(root)
        .map_err(|error| (root.display().to_string(), error))?;
    for (path, template) in SCAFFOLD {
        let text = template
            .replace("{name}", name)
            .replace("{line}", line)
            .replace("{suggestions}", &suggestions);
        written
            .file(path, &text)
            .map_err(|error| (path.to_string(), error))?;
    }
    for folder in EMPTY_FOLDERS {
        written
            .folder(&root.join(folder))
            .map_err(|error| (format!("{folder}/"), error))?;
    }
    // The API record, as `jadpo api update` writes it from these routes.
    let loaded = jadpo_db::load(root);
    let mut routes = Vec::new();
    for (api, text) in record::record_texts(&loaded.db, loaded.project) {
        let path = record::path(api);
        written
            .file(&path, text)
            .map_err(|error| (path.clone(), error))?;
        routes.extend(own_routes(text));
    }
    // Who can do what and the outside hosts, as `jadpo records update`
    // writes them (area 18, chunk A8).
    for (path, text) in [
        (
            jadpo_check::acceptances::access::PATH,
            jadpo_check::acceptances::access::record_text(&loaded.db, loaded.project),
        ),
        (
            jadpo_check::acceptances::network::PATH,
            jadpo_check::acceptances::network::record_text(&loaded.db, loaded.project),
        ),
    ] {
        if let Some(Some(text)) = text {
            written
                .file(path, text)
                .map_err(|error| (path.to_string(), error))?;
        }
    }
    Ok(routes)
}

/// The routes a record lists that the project declares: each entry
/// headed by a method and a path, less those Jadpo generates.
fn own_routes(record: &str) -> Vec<String> {
    const METHODS: [&str; 5] = ["GET ", "POST ", "PUT ", "PATCH ", "DELETE "];
    let lines: Vec<&str> = record.lines().collect();
    lines
        .iter()
        .enumerate()
        .filter(|(_, line)| METHODS.iter().any(|method| line.starts_with(method)))
        .filter(|(i, _)| {
            !lines
                .get(i + 1)
                .is_some_and(|next| next.trim_start().starts_with("generated "))
        })
        .map(|(_, line)| line.to_string())
        .collect()
}

fn done(typed: &str, root: &Path, line: &str, routes: &[String], written: &Written) -> Reply {
    let mut files = written.files.clone();
    files.sort();
    let mut screen = Screen::new();
    screen.push(Block::Status {
        outcome: Outcome::Success,
        text: format!("Created {typed}"),
        rows: vec![
            ("jadpo".to_string(), line.to_string()),
            (
                if routes.len() == 1 { "route" } else { "routes" }.to_string(),
                routes.join(", "),
            ),
            ("files".to_string(), format!("{} written", files.len())),
        ],
    });
    let here = Path::new(typed)
        .components()
        .all(|part| part == Component::CurDir);
    let command = if here {
        "jadpo dev".to_string()
    } else {
        format!("cd {} && jadpo dev", shell_word(typed))
    };
    screen.push(Block::hint(command, "serves it on http://localhost:3000"));
    Reply::screen(
        screen,
        Json::object([
            ("path", Json::from(typed)),
            (
                "root",
                Json::from(
                    root.canonicalize()
                        .unwrap_or_else(|_| root.to_path_buf())
                        .to_string_lossy()
                        .as_ref(),
                ),
            ),
            ("jadpo", Json::from(line)),
            (
                "routes",
                Json::array(routes.iter().map(|route| Json::from(route.as_str()))),
            ),
            (
                "files",
                Json::array(files.iter().map(|file| Json::from(file.as_str()))),
            ),
        ]),
    )
}

/// `word` as a shell reads it back: as it is when it holds only letters,
/// digits and `-_./`, otherwise in single quotes.
fn shell_word(word: &str) -> String {
    let plain = !word.is_empty()
        && !word.starts_with('-')
        && word
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./".contains(c));
    if plain {
        word.to_string()
    } else {
        format!("'{}'", word.replace('\'', r"'\''"))
    }
}
