//! `jadpo domain rename`, `split` and `merge` (area 14, "Tools for
//! agents"; area 1, "Domains"; area 18, "Stable identity"): moves a
//! domain's files and updates every call into them, keeping every key
//! through the identity registry, through `jadpo_check::domain_move`.
//!
//! Designed before it was built (impeccable's shape pass against the CLI
//! design language, matching `jadpo rename`, run unattended, so its
//! assumptions are marked):
//!
//! - *Job.* An agent reshapes the domains an app is cut into, or the
//!   person reviewing its work checks what the reshaping did. Mode:
//!   operate. The diff is the full record; the screen is the summary a
//!   reviewer checks it against: where each folder went, each line outside
//!   it that changed, and what the identity registry now says.
//! - *Screen.* `jadpo rename`'s change list, grouped by path. A folder
//!   that moves whole is one row, never its files one by one (assumption:
//!   a domain of 40 files listed file by file buries the edits that
//!   matter): `- planning  folder of 11 files, renamed plans`, then
//!   `+ plans  folder, was planning`; merged, `- reminders  folder of 5
//!   files, merged into planning`. A split lists each file it takes, since
//!   which ones is the decision being checked (`- entities/note.jadpo
//!   moved to notes`), then `+ notes  new domain, 4 files from planning`.
//!   Then each file with a line that changed, by its path after the move
//!   (`~ line 7  plans, was planning`; merged, `~ line 7  add_todo, was
//!   planning.add_todo`), `use` lines left to the diff as `jadpo rename`
//!   leaves them; each record written again; the registry last, by born
//!   name (`+ domain planning  now plans`).
//! - *Status.* `✓ Renamed domain planning to plans`, `✓ Merged domain
//!   reminders into planning`, `✓ Split notes from planning`, with a `kept`
//!   row naming the stored tables that stay where they are, the one fact a
//!   reader most needs to trust the move (assumption: rows aren't counted,
//!   since that needs a database the command doesn't open). A split adds
//!   the seam it cut along, the page's "these 12 entities never reference
//!   these 9": `moved  3 declarations`, `stayed  5, with no reference
//!   across`.
//! - *Next.* At most one hint: `jadpo check` showing the errors the
//!   project still had; else, in a project with a commit, the acceptance a
//!   renamed or new domain needs (`lists the acceptance a renamed domain
//!   needs`); else, when a record a person may need to accept was written
//!   again, `says whether the change needs an acceptance`.
//! - *Refused* (`cli.domain_arguments`, exit 2, nothing changed): no move
//!   or one it doesn't make; a domain the project doesn't have, with the
//!   nearest; a new name that is a domain already, isn't one, or is
//!   generic, reserved or declared; a merge into itself, of two private
//!   names into one domain (the `in use` list naming both) or two files at
//!   one place; a split naming nothing, something of another domain or
//!   the whole domain; and a split across references, naming each and the
//!   command that moves all they reach.
//! - *Stopped* (a result with an error the project didn't have): each such
//!   error in the one shape, then `✗ domain split stopped  1 error after
//!   the move · nothing changed`, as `jadpo rename` stops.

use std::path::PathBuf;

use jadpo_check::domain_move::{self, Move, Moved, Plan};
use jadpo_diag::{Diagnostic, rule};
use jadpo_term::{Block, Fact, Json, Outcome, Screen};

use crate::config::{files_json, mark_of, write_files};
use crate::fmt::project_needed;
use crate::migrate::blocked;
use crate::run::{Call, Reply};

/// The moves `jadpo domain` makes, for help and completion.
pub const ACTIONS: &[crate::table::Value] = &[
    crate::table::Value {
        name: "rename",
        description: "Give a domain a new name",
    },
    crate::table::Value {
        name: "split",
        description: "Move part of a domain to a new one",
    },
    crate::table::Value {
        name: "merge",
        description: "Move a domain into another",
    },
];

/// The project's domains, for completion.
pub(crate) fn names(cwd: &std::path::Path) -> Vec<(String, &'static str)> {
    let Some(root) = jadpo_db::find_root(cwd) else {
        return Vec::new();
    };
    let loaded = jadpo_db::load(&root);
    jadpo_db::symbols(&loaded.db, loaded.project)
        .domains
        .iter()
        .map(|domain| (domain.clone(), "Domain"))
        .collect()
}

fn usage() -> Diagnostic {
    Diagnostic::new(
        &rule::CLI_DOMAIN_ARGUMENTS,
        "`jadpo domain` needs `rename`, `split` or `merge`",
    )
    .reason("It moves a domain's files and updates every call into them, and needs to know which move.")
    .next("Run `jadpo domain rename <domain> <new-name>`, `jadpo domain split <domain> <new-domain> <name>...` or `jadpo domain merge <domain> <into>`")
}

/// The move the arguments ask for.
fn asked(arguments: &[String]) -> Result<Move, Box<Diagnostic>> {
    let word = arguments.first().map(String::as_str);
    let domain = arguments.get(1).cloned();
    let to = arguments.get(2).cloned();
    match (word, domain, to) {
        (Some("rename"), Some(domain), Some(to)) if arguments.len() == 3 => {
            Ok(Move::Rename { domain, to })
        }
        (Some("merge"), Some(domain), Some(into)) if arguments.len() == 3 => {
            Ok(Move::Merge { domain, into })
        }
        (Some("split"), Some(domain), Some(to)) => Ok(Move::Split {
            domain,
            to,
            names: arguments[3..].to_vec(),
        }),
        (Some("rename"), ..) => Err(Box::new(
            Diagnostic::new(
                &rule::CLI_DOMAIN_ARGUMENTS,
                "`jadpo domain rename` needs a domain and its new name",
            )
            .reason("It gives one domain a new name, moving its folder and every call into it.")
            .next("Run `jadpo domain rename <domain> <new-name>`"),
        )),
        (Some("merge"), ..) => Err(Box::new(
            Diagnostic::new(
                &rule::CLI_DOMAIN_ARGUMENTS,
                "`jadpo domain merge` needs a domain and the domain it goes into",
            )
            .reason("It moves every file of one domain into another, and every call into it.")
            .next("Run `jadpo domain merge <domain> <into>`"),
        )),
        (Some("split"), ..) => Err(Box::new(
            Diagnostic::new(
                &rule::CLI_DOMAIN_ARGUMENTS,
                "`jadpo domain split` needs a domain, the new domain's name and what moves",
            )
            .reason("It moves the declarations you name, with their tests, to a new domain, and leaves the rest where it is.")
            .next("Run `jadpo domain split <domain> <new-domain> <name>...`"),
        )),
        _ => Err(Box::new(usage())),
    }
}

pub fn domain(call: &Call) -> Reply {
    let cwd = call.cwd.clone().unwrap_or_else(|| PathBuf::from("."));
    let what = match asked(&call.arguments) {
        Ok(what) => what,
        Err(problem) => return Reply::Failed(problem),
    };
    let Some(root) = jadpo_db::find_root(&cwd) else {
        return project_needed("domain");
    };
    let mut loaded = jadpo_db::load(&root);
    let headline = format!("domain {} stopped", what.word());
    // Editing a file that doesn't parse could change the wrong text, so
    // syntax errors stop it, as in every command that writes source.
    let syntax: Vec<Diagnostic> = jadpo_check::check(&loaded.db, loaded.project)
        .into_iter()
        .filter(|d| d.rule.id.starts_with("syntax."))
        .collect();
    if !syntax.is_empty() {
        return blocked(&loaded, &cwd, syntax, &headline);
    }
    let plan = match domain_move::plan(&loaded.db, loaded.project, &what) {
        Ok(plan) => plan,
        Err(problem) => return Reply::Failed(problem),
    };
    let committed = loaded.project.committed(&loaded.db);
    let shown_root = jadpo_db::shown_root(&cwd, &loaded.root);
    let moved = match domain_move::apply(&mut loaded, &plan) {
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
                &headline,
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
    let command = format!("jadpo domain {}", call.arguments.join(" "));
    if let Err(problem) = move_others(&root, &moved, &command) {
        return Reply::failed(*problem);
    }
    if let Err(problem) = write_files(&root, &moved.files, &command) {
        return Reply::failed(*problem);
    }
    // The folders the move emptied go too, as git sees them.
    prune(&root.join("domains").join(plan.what.domain()));
    Reply::screen(screen(&plan, &moved, committed), data(&plan, &moved))
}

/// Removes the empty folders in `folder`, and `folder` itself once it is
/// empty.
fn prune(folder: &std::path::Path) {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return;
    };
    for entry in entries.flatten() {
        if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            prune(&entry.path());
        }
    }
    let _ = std::fs::remove_dir(folder);
}

/// Moves the files that are neither source nor a record, as they are on
/// disk.
fn move_others(
    root: &std::path::Path,
    moved: &Moved,
    command: &str,
) -> Result<(), Box<Diagnostic>> {
    for (from, to) in &moved.others {
        let target = root.join(to);
        let done = target
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::rename(root.join(from), &target));
        if let Err(error) = done {
            return Err(Box::new(
                Diagnostic::new(
                    &rule::TOOLCHAIN_ARTIFACT_WRITE_FAILED,
                    format!("`{from}` couldn't be moved"),
                )
                .reason(format!("Moving it to `{to}` failed: {error}. The files before it were moved."))
                .next_for(
                    format!("Check the folder can be written, then restore the files with git and run `{command}` again"),
                    jadpo_diag::Decider::Human,
                ),
            ));
        }
    }
    Ok(())
}

/// The change list, the status line with what was kept, and the one hint.
fn screen(plan: &Plan, moved: &Moved, committed: bool) -> Screen {
    let domain = plan.what.domain();
    let to = plan.what.to();
    let groups = moved
        .groups
        .iter()
        .map(|(path, rows)| {
            (
                path.clone(),
                rows.iter()
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
    let (text, mut rows) = match &plan.what {
        Move::Rename { .. } => (format!("Renamed domain {domain} to {to}"), Vec::new()),
        Move::Merge { .. } => (format!("Merged domain {domain} into {to}"), Vec::new()),
        Move::Split { .. } => (
            format!("Split {to} from {domain}"),
            vec![
                ("moved".to_string(), declarations(plan.moved.len())),
                (
                    "stayed".to_string(),
                    format!("{}, with no reference across", plan.stayed),
                ),
            ],
        ),
    };
    if !plan.kept.is_empty() {
        let tables: Vec<&str> = plan
            .kept
            .iter()
            .filter_map(|kept| kept.strip_prefix("table "))
            .collect();
        rows.push((
            "kept".to_string(),
            match tables.as_slice() {
                [one] => format!("table {one}"),
                many => format!("tables {}", many.join(", ")),
            },
        ));
    }
    let mut screen = Screen::new()
        .with(Block::Changes { note: None, groups })
        .with(Block::Status {
            outcome: Outcome::Success,
            text,
            rows,
        });
    let acceptance = match &plan.what {
        Move::Rename { .. } => Some("lists the acceptance a renamed domain needs"),
        Move::Split { .. } => Some("lists the acceptance a new domain needs"),
        Move::Merge { .. } => None,
    };
    if moved.errors_left > 0 {
        screen.push(Block::hint(
            "jadpo check",
            if moved.errors_left == 1 {
                "shows the error the project still has".to_string()
            } else {
                format!(
                    "shows the {} errors the project still has",
                    moved.errors_left
                )
            },
        ));
    } else if let Some(acceptance) = acceptance.filter(|_| committed) {
        screen.push(Block::hint("jadpo check", acceptance));
    } else if moved.records {
        screen.push(Block::hint(
            "jadpo check",
            "says whether the change needs an acceptance",
        ));
    }
    screen
}

fn declarations(count: usize) -> String {
    if count == 1 {
        "1 declaration".to_string()
    } else {
        format!("{count} declarations")
    }
}

fn data(plan: &Plan, moved: &Moved) -> Json {
    let mut fields = vec![
        ("moved", true.into()),
        ("action", Json::from(plan.what.word())),
        ("domain", Json::from(plan.what.domain())),
        ("to", Json::from(plan.what.to())),
        (
            "kept",
            Json::array(plan.kept.iter().map(|kept| Json::from(kept.as_str()))),
        ),
    ];
    if let Move::Split { .. } = plan.what {
        fields.push((
            "declarations",
            Json::array(plan.moved.iter().map(|name| Json::from(name.as_str()))),
        ));
        fields.push(("stayed", plan.stayed.into()));
    }
    fields.push(("files", Json::array(files_json(&moved.files))));
    fields.push(("errorsLeft", moved.errors_left.into()));
    Json::object(fields)
}
