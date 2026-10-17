//! `jadpo fix` (area 14; area 19, question 4; chunk 5.5a2): lists the
//! findings it can fix, and applies the fix the author picks for one,
//! through the code the editor's quick fix runs (`jadpo_check::fixes`).
//! The findings are the standard types': a declaration a release's new
//! standard type now names (`type.standard_clash`), swapped or renamed,
//! and a type with a standard type's shape (`type.standard_exists`),
//! swapped.
//!
//! Designed before it was built (impeccable's shape pass against the CLI
//! design language, matching `jadpo declines` and `jadpo config move`,
//! run unattended, so its assumptions are marked):
//!
//! - *Job.* After a Jadpo upgrade adds a standard type, an agent, or the
//!   person reviewing its work, makes each finding go the way the author
//!   chose: use the standard type, or keep the application's under a name
//!   of its own. Mode: operate. The source diff is the full record; the
//!   screen is the summary a reviewer checks it against.
//! - `jadpo fix` alone is one list, as `jadpo declines list` is: the bold
//!   heading `Fixes`, a row per finding in file order with the name, its
//!   place (`path:line`, never cut), the fixes it takes (`swap`, `rename`)
//!   and the rule. The footer counts findings and files, and the hint is
//!   the first finding's first fix as a command to copy (assumption: the
//!   swap is the usual answer, so it leads).
//! - `jadpo fix <name> swap` and `jadpo fix <name> rename <NewName>` read
//!   as `jadpo config move` does: a change list grouped by file, the
//!   declaration's first (`- Booking  type, the standard InstantRange
//!   replaces it`, `+ room_reference.jadpo  file, was uuid.jadpo`), then
//!   each line that changed (`~ line 6  start, was starts_at`), then one
//!   status line, `✓ Swapped Booking for the standard InstantRange` or
//!   `✓ Renamed Uuid to RoomReference`. Imports are left to the diff, as
//!   `config move` leaves them. When the project still has errors (every
//!   one of which it had before), the hint is `jadpo check`, which shows
//!   them; otherwise there is none.
//! - *Refused* (a name with nothing to fix, a fix the finding doesn't
//!   take, a new name that is taken or not a type's) is
//!   `cli.fix_arguments`, with the nearest name or the fixes it takes.
//!   *Stopped* (a result with an error the project didn't have) is each
//!   such error in the one shape, then `✗ fix stopped  1 error after the
//!   fix · nothing changed`.
//! - *States.* Nothing to fix (`✓ Nothing to fix`); findings; a swap,
//!   with the file left empty removed; a rename, with the file named
//!   after the declaration moved; refused; stopped; files that don't parse
//!   (stopped on the syntax errors, as every command that writes source
//!   is); a name two domains declare (refused, naming both places).

use std::path::PathBuf;

use jadpo_check::fixes::{self, Finding, Fix, FixKind};
use jadpo_diag::{Diagnostic, rule};
use jadpo_term::{Block, Fact, Json, Outcome, Screen, shown_path};

use crate::config::{change_groups, files_json, write_files};
use crate::fmt::project_needed;
use crate::migrate::blocked;
use crate::run::{Call, Reply, either, quoted};
use crate::suggest::nearest;

/// The fixes, as the command's second argument takes them.
pub const FIXES: &[crate::table::Value] = &[
    crate::table::Value {
        name: "swap",
        description: "Use the standard type in its place",
    },
    crate::table::Value {
        name: "rename",
        description: "Keep it under a new name",
    },
];

pub fn fix(call: &Call) -> Reply {
    let cwd = call.cwd.clone().unwrap_or_else(|| PathBuf::from("."));
    let Some(root) = jadpo_db::find_root(&cwd) else {
        return project_needed("fix");
    };
    let mut loaded = jadpo_db::load(&root);
    // Editing a file that doesn't parse could change the wrong text, so
    // syntax errors stop it, as in every command that writes source.
    let syntax: Vec<Diagnostic> = jadpo_check::check(&loaded.db, loaded.project)
        .into_iter()
        .filter(|d| d.rule.id.starts_with("syntax."))
        .collect();
    if !syntax.is_empty() {
        return blocked(&loaded, &cwd, syntax, "fix stopped");
    }
    let Some(written) = call.arguments.first() else {
        return list(&loaded, &cwd);
    };
    let finding = match fixes::find(&loaded.db, loaded.project, written) {
        Ok(finding) => finding,
        Err(problem) => return Reply::failed(*problem),
    };
    let fix = match chosen(call, &finding) {
        Ok(fix) => fix,
        Err(problem) => return Reply::failed(*problem),
    };
    let plan = match fixes::plan(&loaded.db, loaded.project, &finding, &fix) {
        Ok(plan) => plan,
        Err(mut refusals) if !refusals.is_empty() => return Reply::failed(refusals.remove(0)),
        Err(_) => {
            return Reply::failed(
                Diagnostic::new(
                    &rule::CLI_FIX_ARGUMENTS,
                    format!("`{}` has nothing to fix", finding.name),
                )
                .reason("Its declaration changed while the fix was planned.")
                .next(format!("Run `jadpo fix {}` again", finding.name)),
            );
        }
    };
    let shown_root = jadpo_db::shown_root(&cwd, &loaded.root);
    let fixed = match fixes::apply(&mut loaded, &plan) {
        Ok(fixed) => fixed,
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
                "fix stopped",
                [
                    Fact::problem(format!(
                        "{errors} {} after the fix",
                        if errors == 1 { "error" } else { "errors" }
                    )),
                    Fact::plain("nothing changed"),
                ],
            ));
            return Reply::Report {
                screen,
                data: Json::object([("fixed", false.into())]),
                diagnostics: stopped.problems,
                sources: stopped.sources,
            };
        }
    };
    if let Err(problem) = write_files(&root, &fixed.files, &command(&finding, &fix)) {
        return Reply::failed(*problem);
    }
    let text = match &fix {
        Fix::Swap if finding.name == finding.standard => format!(
            "Swapped the application's {} for the standard one",
            finding.name
        ),
        Fix::Swap => format!(
            "Swapped {} for the standard {}",
            finding.name, finding.standard
        ),
        Fix::Rename(new) => format!("Renamed {} to {new}", finding.name),
    };
    let mut screen = Screen::new()
        .with(Block::Changes {
            note: None,
            groups: change_groups(&fixed.files),
        })
        .with(Block::Status {
            outcome: Outcome::Success,
            text,
            rows: Vec::new(),
        });
    if fixed.errors_left > 0 {
        screen.push(Block::hint(
            "jadpo check",
            if fixed.errors_left == 1 {
                "shows the error the project still has".to_string()
            } else {
                format!(
                    "shows the {} errors the project still has",
                    fixed.errors_left
                )
            },
        ));
    }
    let data = Json::object([
        ("fixed", true.into()),
        ("name", Json::from(finding.name.as_str())),
        ("rule", Json::from(finding.rule.id)),
        ("fix", Json::from(fix.kind().word())),
        ("standard", Json::from(finding.standard)),
        (
            "renamed",
            match &fix {
                Fix::Rename(new) => Json::from(new.as_str()),
                Fix::Swap => Json::Null,
            },
        ),
        ("files", Json::array(files_json(&fixed.files))),
        ("errorsLeft", fixed.errors_left.into()),
    ]);
    Reply::screen(screen, data)
}

/// The fix the command line chose for `finding`.
fn chosen(call: &Call, finding: &Finding) -> Result<Fix, Box<Diagnostic>> {
    let name = &finding.name;
    let offered: Vec<&str> = finding.fixes.iter().map(|fix| fix.word()).collect();
    let commands = || -> Vec<String> {
        finding
            .fixes
            .iter()
            .map(|fix| match fix {
                FixKind::Swap => format!("`jadpo fix {name} swap`"),
                FixKind::Rename => format!("`jadpo fix {name} rename <NewName>`"),
            })
            .collect()
    };
    let Some(word) = call.arguments.get(1) else {
        return Err(Box::new(
            Diagnostic::new(
                &rule::CLI_FIX_ARGUMENTS,
                format!("`jadpo fix {name}` needs a fix"),
            )
            .reason(format!(
                "`{name}` can be fixed with {}: the author picks which.",
                either(&quoted(&offered))
            ))
            .next(format!("Run {}", either(&commands()))),
        ));
    };
    let kind = match FixKind::from_word(word) {
        Some(kind) => kind,
        None => {
            let words: Vec<&str> = FixKind::ALL.iter().map(|fix| fix.word()).collect();
            let diagnostic =
                Diagnostic::new(&rule::CLI_FIX_ARGUMENTS, format!("`{word}` isn't a fix"));
            return Err(Box::new(match nearest(word, words.iter().copied()) {
                Some(near) => diagnostic
                    .reason(format!(
                        "`jadpo fix` makes {}. Did you mean `{near}`?",
                        either(&quoted(&words))
                    ))
                    .next(format!("Use `{near}` instead of `{word}`")),
                None => diagnostic
                    .reason(format!("`jadpo fix` makes {}.", either(&quoted(&words))))
                    .next(format!("Run {}", either(&commands()))),
            }));
        }
    };
    match kind {
        FixKind::Swap => Ok(Fix::Swap),
        FixKind::Rename => match call.arguments.get(2) {
            Some(new) => Ok(Fix::Rename(new.clone())),
            None => Err(Box::new(
                Diagnostic::new(
                    &rule::CLI_FIX_ARGUMENTS,
                    format!("`jadpo fix {name} rename` needs the new name"),
                )
                .reason(format!(
                    "A rename writes a new name at `{name}`'s declaration and every reference to it, so it needs to know the name."
                ))
                .next(format!("Run `jadpo fix {name} rename <NewName>`")),
            )),
        },
    }
}

/// The command a fix runs, to run again.
fn command(finding: &Finding, fix: &Fix) -> String {
    match fix {
        Fix::Swap => format!("jadpo fix {} swap", finding.name),
        Fix::Rename(new) => format!("jadpo fix {} rename {new}", finding.name),
    }
}

fn count(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

fn list(loaded: &jadpo_db::Loaded, cwd: &std::path::Path) -> Reply {
    let shown_root = jadpo_db::shown_root(cwd, &loaded.root);
    let found = fixes::findings(&loaded.db, loaded.project);
    let data = Json::object([(
        "findings",
        Json::array(found.iter().map(|finding| {
            Json::object([
                ("name", Json::from(finding.name.as_str())),
                ("rule", Json::from(finding.rule.id)),
                ("path", Json::from(finding.path.as_str())),
                ("line", Json::from(finding.line as usize)),
                ("standard", Json::from(finding.standard)),
                ("release", Json::from(finding.release)),
                (
                    "fixes",
                    Json::array(finding.fixes.iter().map(|fix| fix.word())),
                ),
            ])
        })),
    )]);
    let Some(first) = found.first() else {
        let screen = Screen::new().with(Block::summary(Outcome::Success, "Nothing to fix", []));
        return Reply::screen(screen, data);
    };
    let places: Vec<String> = found
        .iter()
        .map(|finding| {
            format!(
                "{}:{}",
                shown_path(shown_root.as_deref(), &finding.path),
                finding.line
            )
        })
        .collect();
    let takes: Vec<String> = found
        .iter()
        .map(|finding| {
            finding
                .fixes
                .iter()
                .map(|fix| fix.word())
                .collect::<Vec<_>>()
                .join(", ")
        })
        .collect();
    let name_width = found.iter().map(|f| f.name.len()).max().unwrap_or(0);
    let place_width = places.iter().map(String::len).max().unwrap_or(0);
    let takes_width = takes.iter().map(String::len).max().unwrap_or(0);
    let lines: Vec<String> = found
        .iter()
        .zip(places.iter().zip(&takes))
        .map(|(finding, (place, takes))| {
            format!(
                "{:name_width$}  {place:place_width$}  {takes:takes_width$}  {}",
                finding.name, finding.rule.id
            )
        })
        .collect();
    let files: std::collections::BTreeSet<&str> =
        found.iter().map(|finding| finding.path.as_str()).collect();
    let next = match first.fixes.first() {
        Some(FixKind::Rename) => format!("jadpo fix {} rename <NewName>", first.name),
        _ => format!("jadpo fix {} swap", first.name),
    };
    let reason = match first.fixes.first() {
        Some(FixKind::Rename) => "keeps it under a name of its own".to_string(),
        _ => format!("uses the standard {} in its place", first.standard),
    };
    let screen = Screen::new()
        .with(Block::Section {
            heading: "Fixes".to_string(),
            lines,
        })
        .with(Block::summary(
            Outcome::Failure,
            count(found.len(), "finding to fix", "findings to fix"),
            [Fact::plain(count(files.len(), "file", "files"))],
        ))
        .with(Block::hint(next, reason));
    Reply::screen(screen, data)
}
