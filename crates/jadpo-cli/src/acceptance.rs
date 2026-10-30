//! `jadpo acceptance new <purpose>` (area 18, "Acceptances"; chunk A8):
//! drafts `acceptances/<stamp>_<purpose>.jadpo` listing every change since
//! the before side that needs a person and no added acceptance lists yet,
//! with no reason. An agent may run it, then stops: the missing reason
//! keeps `jadpo check` failing until a person writes one.
//!
//! ```text
//! ✓ Wrote acceptances/20261030T1400_remove_todo_priority.jadpo
//!     main.TodoSummary.priority  ApiChange.output_field_removed
//!     main.TodoView.priority     ApiChange.output_field_removed
//!
//! ◆ decision  2 changes wait for a person's reason
//!   at        acceptances/20261030T1400_remove_todo_priority.jadpo:1
//!   next      Stop and hand over: a person reads the changes and writes why
//!             they are accepted on a `///` line above `acceptance`. (human)
//! ```

use std::path::PathBuf;

use jadpo_check::acceptances::{self, Need};
use jadpo_db::{Before, Layout};
use jadpo_diag::{Diagnostic, rule};
use jadpo_term::{Block, Json, Outcome, Screen};

use crate::fmt::project_not_found;
use crate::run::{Call, Reply};

/// What `jadpo acceptance` does.
pub const ACTIONS: &[crate::table::Value] = &[crate::table::Value {
    name: "new",
    description: "Draft an acceptance of every change waiting for one",
}];

fn arguments(message: String, reason: &str, next: String) -> Diagnostic {
    Diagnostic::new(&rule::CLI_ACCEPTANCE_ARGUMENTS, message)
        .reason(reason)
        .next(next)
}

pub fn acceptance(call: &Call) -> Reply {
    match call.arguments.first().map(String::as_str) {
        Some("new") => {}
        Some(other) => {
            return Reply::failed(arguments(
                format!("`{other}` isn't something `jadpo acceptance` does"),
                "`jadpo acceptance new <purpose>` drafts an acceptance of every change waiting for a person.",
                "Run `jadpo acceptance new <purpose>`".to_string(),
            ));
        }
        None => {
            return Reply::failed(arguments(
                "`jadpo acceptance` needs `new <purpose>`".to_string(),
                "`jadpo acceptance new <purpose>` drafts an acceptance of every change waiting for a person.",
                "Run `jadpo acceptance new <purpose>`, such as `jadpo acceptance new open_book_list`".to_string(),
            ));
        }
    }
    let Some(purpose) = call.arguments.get(1) else {
        return Reply::failed(arguments(
            "`jadpo acceptance new` needs a purpose".to_string(),
            "An acceptance's file is named after the decision it records, so `acceptances/` reads as a list of decisions.",
            "Add one, such as `jadpo acceptance new open_book_list`".to_string(),
        ));
    };
    if let Some(extra) = call.arguments.get(2) {
        return Reply::failed(arguments(
            format!("`jadpo acceptance new` takes one purpose, not `{extra}` too"),
            "The purpose is one lower_snake_case name.",
            format!("Run `jadpo acceptance new {purpose}`"),
        ));
    }
    if !crate::migrate::valid_purpose(purpose) {
        return Reply::failed(arguments(
            format!("`{purpose}` isn't a purpose a file can be named after"),
            "A purpose is lower_snake_case words, such as `open_book_list`.",
            format!("Write it as `{}`", crate::migrate::snake(purpose)),
        ));
    }
    let cwd = call.cwd.clone().unwrap_or_else(|| PathBuf::from("."));
    let Some(root) = jadpo_db::find_root(&cwd) else {
        return project_not_found("acceptance");
    };
    let checked = match crate::check::checked_before(call, &root) {
        Ok(checked) => checked,
        Err(reply) => return *reply,
    };
    let loaded = &checked.loaded;
    let db = &loaded.db;
    let project = loaded.project;
    if project.layout(db) != Layout::Project {
        return project_not_found("acceptance");
    }
    // What changed can only be worked out from a source without errors.
    let blocking: Vec<Diagnostic> = checked
        .diagnostics
        .iter()
        .filter(|d| !d.rule.id.starts_with("acceptance.") && !d.rule.id.starts_with("record."))
        .cloned()
        .collect();
    if !blocking.is_empty() {
        return crate::migrate::blocked(loaded, &cwd, blocking, "acceptance not written");
    }
    match project.before(db) {
        Before::At(_) => {}
        Before::Missing { base } => {
            return Reply::failed(
                Diagnostic::new(
                    &rule::ACCEPTANCE_BASE_MISSING,
                    format!("The base to compare with, {base}, isn't in this clone"),
                )
                .reason("What needs a person's acceptance is worked out against the default branch at its merge base, and this clone was made without that commit.")
                .next("Fetch the history with `git fetch --unshallow`, then run the command again"),
            );
        }
        Before::Nothing(reason) => return Reply::failed(nothing_to_compare(reason)),
        Before::NotRead => {
            return Reply::failed(nothing_to_compare("not a git repository"));
        }
    }
    let mut needs: Vec<Need> = acceptances::unaccepted(db, project);
    if needs.is_empty() {
        return Reply::failed(arguments(
            "Nothing waits for a person's acceptance".to_string(),
            "Every change since the before side that needs a person is listed in an acceptance this change adds already, or none needs one.",
            "Run `jadpo check` to see what is left".to_string(),
        ));
    }
    needs.sort_by(|a, b| (&a.subject, &a.change).cmp(&(&b.subject, &b.change)));
    needs.dedup_by(|a, b| a.subject == b.subject && a.change == b.change);
    let name = format!("{}_{purpose}", stamp(&loaded.root));
    let relative = format!("{}/{name}.jadpo", acceptances::files::FOLDER);
    let text = draft(&needs);
    let folder = loaded.root.join(acceptances::files::FOLDER);
    let written = std::fs::create_dir_all(&folder)
        .and_then(|()| std::fs::write(loaded.root.join(&relative), &text));
    if let Err(error) = written {
        return Reply::failed(
            Diagnostic::new(
                &rule::TOOLCHAIN_ARTIFACT_WRITE_FAILED,
                format!("`{relative}` couldn't be written"),
            )
            .reason(format!("Writing it failed: {error}."))
            .next("Check the folder can be written to, then run the command again"),
        );
    }
    let mut screen = Screen::new();
    screen.push(Block::Status {
        outcome: Outcome::Success,
        text: format!("Wrote {relative}"),
        rows: needs
            .iter()
            .map(|need| (need.subject.clone(), need.change.clone()))
            .collect(),
    });
    let count = needs.len();
    screen.push(Block::Decision {
        subject: format!(
            "{count} {} for a person's reason",
            if count == 1 { "change waits" } else { "changes wait" }
        ),
        location: Some(format!("{relative}:1")),
        next: "Stop and hand over: a person reads the changes and writes why they are accepted on a `///` line above `acceptance`.".to_string(),
    });
    let data = Json::object([
        ("file", relative.as_str().into()),
        (
            "changes",
            Json::array(needs.iter().map(|need| {
                Json::object([
                    ("subject", need.subject.as_str().into()),
                    ("change", need.change.as_str().into()),
                ])
            })),
        ),
        ("reason", false.into()),
    ]);
    Reply::screen(screen, data)
}

fn nothing_to_compare(reason: &str) -> Diagnostic {
    arguments(
        format!("Nothing to compare with: {reason}"),
        "An acceptance lists the changes since the before side, the default branch at its merge base, and there is none.",
        "Commit the project to git with a default branch, or name a base with `--before <branch or commit>`".to_string(),
    )
}

/// The drafted file: every change, sorted, and no reason.
pub fn draft(needs: &[Need]) -> String {
    let mut text = String::from("acceptance {\n    changes: {\n");
    for need in needs {
        text.push_str(&format!("        {}: {}\n", need.subject, need.change));
    }
    text.push_str("    }\n}\n");
    text
}

/// `20261030T1400`, now in UTC, or at `SOURCE_DATE_EPOCH` when it is set,
/// a minute later while a file of that minute exists.
fn stamp(root: &std::path::Path) -> String {
    let millis = std::env::var("SOURCE_DATE_EPOCH")
        .ok()
        .and_then(|seconds| seconds.trim().parse::<i64>().ok())
        .map(|seconds| seconds.saturating_mul(1000))
        .unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(0))
        });
    let format = |millis: i64| -> String {
        jadpo_std::format_instant(millis)
            .chars()
            .take(16)
            .filter(|c| *c != '-' && *c != ':')
            .collect()
    };
    let taken: Vec<String> = std::fs::read_dir(root.join(acceptances::files::FOLDER))
        .map(|entries| {
            entries
                .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
                .filter_map(|name| name.get(..13).map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let mut at = millis - millis.rem_euclid(60_000);
    while taken.contains(&format(at)) {
        at += 60_000;
    }
    format(at)
}
