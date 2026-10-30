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

/// The route the starter domain answers, as the screen and `--json` name it.
const ROUTE: &str = "POST /hello";

/// The project's files, in the order they are written, each with its
/// text; `{line}`, `{name}` and `{suggestions}` are filled in.
const FILES: &[(&str, &str)] = &[
    (".gitattributes", GITATTRIBUTES),
    (".gitignore", "build/\n"),
    ("AGENTS.md", AGENTS),
    ("app.jadpo", APP),
    ("config.jadpo", CONFIG),
    ("policy.jadpo", POLICY),
    ("domains/hello/types/visitor.jadpo", VISITOR),
    ("domains/hello/types/greeting.jadpo", GREETING),
    ("domains/hello/actions/greet.jadpo", GREET),
    ("domains/hello/routes/hello.jadpo", ROUTES),
    ("domains/hello/routes/hello.test.jadpo", ROUTES_TEST),
];

/// The fixed root's folders that start empty.
const EMPTY_FOLDERS: &[&str] = &["database", "tests"];

/// Area 18's lines: each record's diff stays expanded in a pull request,
/// and the records two branches both add to merge with git's own union
/// driver.
const GITATTRIBUTES: &str = "\
*.jadpo_record                       text eol=lf linguist-generated=false
jadpo.lock                           text eol=lf linguist-generated=false merge=union
database/identities.jadpo_record     merge=union
**/translations/sources.jadpo_record merge=union
";

const AGENTS: &str = "\
# {name}

{name} is a Jadpo app. Jadpo is a programming language for web backends,
written mainly by coding agents and reviewed by people. One compiler checks
the whole app: its types, stored data, routes, sign-in, and who may see and
change what.

## Working on it

- The app is written for Jadpo {line}: `app.jadpo` says `jadpo: \"{line}\"`.
  `jadpo --version`, run in this folder, gives the exact release.
- Read the primer once before you write Jadpo:
  https://jadpo.dev/docs/primer.md. For anything else, pick topics by title
  from https://jadpo.dev/docs/index.json and read only their `.md`. The
  same paths under `https://jadpo.dev/v/<version>/` are the docs of one
  release.
- Every error names its rule, and its page is
  `https://jadpo.dev/docs/errors/<rule.id>.md`. An error's `next:` ending
  `(human)` is a decision for the person you work with: stop and ask them.
- Run `jadpo check` after every change, `jadpo fmt` after editing a file and
  `jadpo test` before you finish. After changing a route, run
  `jadpo api update` and commit `api/` with the code.
";

const APP: &str = "\
application {
    jadpo: \"{line}\"
}
{suggestions}";

const CONFIG: &str = "\
// The settings the app reads from its environment, one field each.

config {}
";

const POLICY: &str = "\
use hello

policy {
    actions: {
        hello.greet: [Access.public]
    }
}
";

const VISITOR: &str = "\
/// Who `POST /hello` greets.
type Visitor = Input {
    name: Text
}
";

const GREETING: &str = "\
/// What `POST /hello` sends back.
type Greeting = Output {
    message: Text
    name: Text
}
";

const GREET: &str = "\
use hello/types { Greeting, Visitor }

/// Greets a visitor by name.
action greet(input: Visitor) -> Greeting {
    return Greeting {
        message: \"Hello\"
        name: input.name
    }
}
";

const ROUTES: &str = "\
use hello/types { Greeting, Visitor }
use hello/actions { greet }

/// A first route, to start from.
routes Hello {
    prefix: /hello

    /// Greets whoever calls it, signed in or not.
    POST / {
        authentication: RouteAuthentication.none
        input: Visitor
        output: Greeting
        run: greet(input)
    }
}
";

const ROUTES_TEST: &str = "\
use hello/types { Greeting, Visitor }

test \"greets the visitor by name\" {
    var response: Response = request {
        method: HttpMethod.post
        path: \"/hello\"
        input: Visitor {
            name: \"Ann\"
        }
    }
    assert response.status == 200
    assert response.output == Greeting {
        message: \"Hello\"
        name: \"Ann\"
    }
}
";

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
    let Some(name) = project_name(typed, &destination) else {
        return Reply::failed(
            Diagnostic::new(
                &rule::TOOLCHAIN_SCAFFOLD_NAME_INVALID,
                format!("`{typed}` doesn't name a folder for the project"),
            )
            .reason("`jadpo new` makes the project in the folder its argument names, and names the project after that folder, so the argument ends in a folder's name.")
            .next("Give the project's name, for example `jadpo new todo-app`"),
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
        Ok(()) => done(typed, &destination, &line, &written),
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
        .next("Run `jadpo new <name>`, for example `jadpo new todo-app`")
}

/// The project's name: the last folder of the destination as typed, or,
/// for `.`, the name of the folder it runs in.
fn project_name(typed: &str, destination: &Path) -> Option<String> {
    match Path::new(typed).components().next_back()? {
        Component::Normal(name) => name.to_str().map(str::to_string),
        Component::CurDir => destination
            .canonicalize()
            .ok()?
            .file_name()?
            .to_str()
            .map(str::to_string),
        _ => None,
    }
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
) -> Result<(), (String, std::io::Error)> {
    // A token is written whenever `share` first needs one, so a machine
    // whose generator fails still gets a project, without it.
    let suggestions = crate::share::new_token()
        .map(|token| format!("\nsuggestions {{\n    project_token: \"{token}\"\n}}\n"))
        .unwrap_or_default();
    written
        .folder(root)
        .map_err(|error| (root.display().to_string(), error))?;
    for (path, template) in FILES {
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
    for (api, text) in record::record_texts(&loaded.db, loaded.project) {
        let path = record::path(api);
        written
            .file(&path, text)
            .map_err(|error| (path.clone(), error))?;
    }
    Ok(())
}

fn done(typed: &str, root: &Path, line: &str, written: &Written) -> Reply {
    let mut files = written.files.clone();
    files.sort();
    let mut screen = Screen::new();
    screen.push(Block::Status {
        outcome: Outcome::Success,
        text: format!("Created {typed}"),
        rows: vec![
            ("jadpo".to_string(), line.to_string()),
            ("route".to_string(), ROUTE.to_string()),
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
            ("routes", Json::array([Json::from(ROUTE)])),
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
