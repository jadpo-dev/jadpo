//! `jadpo rename <name> <new-name>` (area 14, "Commands for looking into
//! and changing code"; area 18, "Stable identity"): renames a declaration
//! or a field everywhere, keeps its stored data by writing the identity
//! registry, and refuses a name in use, through `jadpo_check::rename`,
//! the code the editor's rename is to run.
//!
//! Designed before it was built (impeccable's shape pass against the CLI
//! design language, matching `jadpo fix … rename` and `jadpo config move`,
//! run unattended, so its assumptions are marked):
//!
//! - *Job.* An agent, or the person reviewing its work, gives something a
//!   better name without losing what is stored under the old one. Mode:
//!   operate. The source diff is the full record; the screen is the
//!   summary a reviewer checks it against, and says what was kept.
//! - *Screen.* A change list grouped by file, as `jadpo config move`'s:
//!   the declaration's file first (moved: `- greet.jadpo  file, renamed
//!   welcome.jadpo`, then `+ welcome.jadpo  file, was greet.jadpo`), each
//!   line that changed (`~ line 13  welcome, was greet`), the other files
//!   in path order, then each record written again (`~ record  brought up
//!   to date`) and the registry last, its line by born name (`+ field
//!   Todo.priority  now importance`; `~ … now urgency, was importance`
//!   on a second rename; `- … back to its born name`). `use` lines are
//!   left to the diff, as `config move` leaves them. Then one status line,
//!   `✓ Renamed Todo.priority to Todo.importance`, with a `kept` row when
//!   stored data stays where it is (`kept  column todo.priority`, `kept
//!   table todo`): the one fact the reader most needs to trust the rename
//!   (assumption: rows aren't counted, since that needs a database the
//!   command doesn't open).
//! - *Next.* At most one hint: `jadpo check`, when the project still has
//!   errors it had before (`shows the error the project still has`), or
//!   else when a record a person may need to accept was written again
//!   (`says whether the change needs an acceptance`); none otherwise.
//! - *Refused* (`cli.rename_arguments`, nothing changed): a name the
//!   project doesn't have, with the nearest; a domain, a route or a test,
//!   each saying what renames it; a new name that isn't one for its kind,
//!   the name it has, a built-in type's or a standard namespace's; a field
//!   given a new owner; and a name in use, naming what holds it and its
//!   file with the `in use` list of names that share its words.
//! - *Stopped* (a result with an error the project didn't have): each such
//!   error in the one shape, then `✗ rename stopped  1 error after the
//!   rename · nothing changed`, as `jadpo fix` does. Files that don't parse
//!   stop it first, as in every command that writes source.

use std::path::PathBuf;

use jadpo_check::rename::{self, Plan, Renamed};
use jadpo_diag::{Diagnostic, rule};
use jadpo_graph::Kind;
use jadpo_term::{Block, Fact, Json, Outcome, Screen};

use crate::config::{change_groups, files_json, write_files};
use crate::fmt::project_needed;
use crate::migrate::blocked;
use crate::run::{Call, Reply};

pub fn rename(call: &Call) -> Reply {
    let cwd = call.cwd.clone().unwrap_or_else(|| PathBuf::from("."));
    let Some(root) = jadpo_db::find_root(&cwd) else {
        return project_needed("rename");
    };
    let mut loaded = jadpo_db::load(&root);
    // Editing a file that doesn't parse could change the wrong text, so
    // syntax errors stop it, as in every command that writes source.
    let syntax: Vec<Diagnostic> = jadpo_check::check(&loaded.db, loaded.project)
        .into_iter()
        .filter(|d| d.rule.id.starts_with("syntax."))
        .collect();
    if !syntax.is_empty() {
        return blocked(&loaded, &cwd, syntax, "rename stopped");
    }
    let (Some(name), Some(new)) = (call.arguments.first(), call.arguments.get(1)) else {
        return Reply::failed(
            Diagnostic::new(
                &rule::CLI_RENAME_ARGUMENTS,
                "`jadpo rename` needs a name and its new name",
            )
            .reason("It renames one declaration or field, so it needs to know which and what to call it.")
            .next("Run `jadpo rename <name> <new-name>`; `jadpo inspect` lists the project's declarations"),
        );
    };
    let (db, project) = (&loaded.db, loaded.project);
    let graph = std::sync::Arc::clone(jadpo_check::graph::graph(db, project));
    if jadpo_db::symbols(db, project)
        .domains
        .contains(name.as_str())
        && !graph
            .nodes()
            .iter()
            .any(|node| node.name == *name && rename::renames(node.kind))
    {
        return Reply::failed(not_renamed(
            format!("`{name}` is a domain"),
            "A domain's name is its folder's, and renaming one moves the folder and every call into it, which `jadpo domain rename` does.".to_string(),
            &format!("Run `jadpo domain rename {name} {new}`"),
        ));
    }
    let id = match crate::inspect::resolve(
        &graph,
        name,
        "rename",
        &rule::CLI_RENAME_ARGUMENTS,
        &format!(" {new}"),
    ) {
        Ok(id) => id,
        Err(problem) => return Reply::Failed(problem),
    };
    let node = graph.node(id);
    if !rename::renames(node.kind) {
        return Reply::failed(match node.kind {
            Kind::Route | Kind::GeneratedRoute => not_renamed(
                format!("`{name}` is a route"),
                "A route goes by its method and path, written in its `routes` block.".to_string(),
                "Change its method or path where its `routes` block writes them",
            ),
            Kind::Test => not_renamed(
                format!("`{name}` is a test"),
                "A test goes by the file it sits beside and its sentence, written where the test is.".to_string(),
                "Change its sentence where the test is written",
            ),
            kind => not_renamed(
                format!("`{name}` is {} {}", a_or_an(kind.word()), kind.word().replace('_', " ")),
                "`jadpo rename` renames entities, types, functions, constants, actions, queries, fixtures, seeds, schedules, subscribers and libraries, and their fields and variants.".to_string(),
                "Change it where it is written",
            ),
        });
    }
    let key = node.key.clone();
    let plan = match rename::plan(db, project, &key, new) {
        Ok(plan) => plan,
        Err(problem) => return Reply::Failed(problem),
    };
    let shown_root = jadpo_db::shown_root(&cwd, &loaded.root);
    let renamed = match rename::apply(&mut loaded, &plan) {
        Ok(renamed) => renamed,
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
                "rename stopped",
                [
                    Fact::problem(format!(
                        "{errors} {} after the rename",
                        if errors == 1 { "error" } else { "errors" }
                    )),
                    Fact::plain("nothing changed"),
                ],
            ));
            return Reply::Report {
                screen,
                data: Json::object([("renamed", false.into())]),
                diagnostics: stopped.problems,
                sources: stopped.sources,
            };
        }
    };
    let command = format!("jadpo rename {name} {new}");
    if let Err(problem) = write_files(&root, &renamed.files, &command) {
        return Reply::failed(*problem);
    }
    Reply::screen(screen(&plan, &renamed), data(&plan, &renamed))
}

fn a_or_an(word: &str) -> &'static str {
    if word.starts_with(['a', 'e', 'i', 'o', 'u']) {
        "an"
    } else {
        "a"
    }
}

/// `cli.rename_arguments` for something `jadpo rename` doesn't rename.
fn not_renamed(message: String, reason: String, next: &str) -> Diagnostic {
    Diagnostic::new(&rule::CLI_RENAME_ARGUMENTS, message)
        .reason(reason)
        .next(next)
}

/// The change list, file by file, the status line with what was kept, and
/// the one hint.
fn screen(plan: &Plan, renamed: &Renamed) -> Screen {
    let mut screen = Screen::new()
        .with(Block::Changes {
            note: None,
            groups: change_groups(&renamed.files),
        })
        .with(Block::Status {
            outcome: Outcome::Success,
            text: format!("Renamed {} to {}", plan.name, plan.to),
            rows: plan
                .kept
                .iter()
                .map(|kept| ("kept".to_string(), kept.clone()))
                .collect(),
        });
    if renamed.errors_left > 0 {
        screen.push(Block::hint(
            "jadpo check",
            if renamed.errors_left == 1 {
                "shows the error the project still has".to_string()
            } else {
                format!(
                    "shows the {} errors the project still has",
                    renamed.errors_left
                )
            },
        ));
    } else if renamed.records {
        screen.push(Block::hint(
            "jadpo check",
            "says whether the change needs an acceptance",
        ));
    }
    screen
}

fn data(plan: &Plan, renamed: &Renamed) -> Json {
    Json::object([
        ("renamed", true.into()),
        ("kind", Json::from(plan.kind.word())),
        ("name", Json::from(plan.name.as_str())),
        ("to", Json::from(plan.to.as_str())),
        ("id", Json::from(plan.key.as_str())),
        ("kept", plan.kept.as_deref().map_or(Json::Null, Json::from)),
        ("files", Json::array(files_json(&renamed.files))),
        ("errorsLeft", renamed.errors_left.into()),
    ])
}
