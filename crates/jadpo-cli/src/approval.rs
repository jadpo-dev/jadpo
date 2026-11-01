//! `jadpo approval [--before <ref>]` (area 17, "Reviewing a change"; area
//! 18, "Comparing two builds"; chunk V2): checks today's source as `jadpo
//! check` does, builds the before side, the default branch at its merge
//! base or the branch or commit `--before` names, keeping its manifest in
//! `build/before/<commit>/manifest.json` so each base is built once, and
//! writes the difference of the two manifests to `build/review/approval.md`
//! and `.json`, decisions first.
//!
//! The terminal says the decisions in full, each with its state and the
//! path that makes it true, then counts the rest:
//!
//! ```text
//! 5 decisions need a person
//! ◆ anyone can now read `lending.Book` records          acceptance missing
//!     anyone → GET /books → lending.catalogue → List<BookView>
//! …
//!
//! 1 declaration newly reachable
//!   lending.catalogue     now reached from GET /books/shelf
//!
//!   before    main at 1a2b3c4
//!   api       1 route added, 2 routes changed, 1 type changed
//!   access    1 decline added, 1 entity's grants changed
//!   source    8 declarations in 7 files
//!
//! ✓ Wrote build/review/approval.md
//! ◆ decision  5 changes wait for a person's acceptance
//!   next      A person accepts them: `jadpo acceptance new <purpose>`
//!             writes the draft for them. (human)
//! ```

use std::path::{Path, PathBuf};

use jadpo_check::approval::{self, Acceptance, Base, Report};
use jadpo_db::{Before, Layout};
use jadpo_diag::{Diagnostic, rule};
use jadpo_graph::json::{self, Value};
use jadpo_graph::manifest::{self, Source, Stamp};
use jadpo_term::{Block, DecisionRow, Outcome, Row, Screen};

use crate::check::{JudgedOn, checked_before_as, judged_day};
use crate::fmt::project_needed;
use crate::help::VERSION;
use crate::run::{Call, Reply};

/// The most newly reachable declarations the terminal lists; the report
/// holds them all.
const MOST_LISTED: usize = 10;

fn arguments(message: String, reason: &str, next: &str) -> Diagnostic {
    Diagnostic::new(&rule::CLI_APPROVAL_ARGUMENTS, message)
        .reason(reason)
        .next(next)
}

pub fn approval(call: &Call) -> Reply {
    let cwd = call.cwd.clone().unwrap_or_else(|| PathBuf::from("."));
    let Some(root) = jadpo_db::find_root(&cwd) else {
        return project_needed("approval");
    };
    let checked =
        match checked_before_as(call, &root, JudgedOn::Today, &rule::CLI_APPROVAL_ARGUMENTS) {
            Ok(checked) => checked,
            Err(reply) => return *reply,
        };
    let loaded = &checked.loaded;
    let (db, project) = (&loaded.db, loaded.project);
    if project.layout(db) != Layout::Project {
        return project_needed("approval");
    }
    let side = match project.before(db) {
        Before::At(side) => side,
        Before::Missing { base } => {
            return Reply::failed(
                Diagnostic::new(
                    &rule::ACCEPTANCE_BASE_MISSING,
                    format!("The base to compare with, {base}, isn't in this clone"),
                )
                .reason("The approval report compares the change with the default branch at its merge base, and this clone was made without that commit.")
                .next("Fetch the history with `git fetch --unshallow`, then run the command again"),
            );
        }
        Before::Nothing(reason) => return Reply::failed(nothing_to_compare(reason)),
        Before::NotRead => return Reply::failed(nothing_to_compare("not a git repository")),
    };
    // What changed can only be worked out from a source without errors;
    // what needs a person and the records out of date are what the report
    // explains.
    let blocking: Vec<Diagnostic> = checked
        .diagnostics
        .iter()
        .filter(|d| !d.rule.id.starts_with("acceptance.") && !d.rule.id.starts_with("record."))
        .cloned()
        .collect();
    if !blocking.is_empty() {
        return crate::migrate::blocked(loaded, &cwd, blocking, "approval not written");
    }
    let Some(after) = read_manifest(&loaded.root.join("build/manifest.json")) else {
        return Reply::failed(not_written("build/manifest.json"));
    };
    let day = judged_day(call, &loaded.root, JudgedOn::Today);
    crate::run::say(&format!("Building the before side, {}", side.described));
    let (commit, before, before_declines) = match base_build(&loaded.root, &side.commit, day) {
        Ok(found) => found,
        Err(problem) => return Reply::failed(*problem),
    };
    let report = approval::report(
        db,
        project,
        Base {
            described: side.described.clone(),
            commit,
        },
        &before,
        &before_declines,
        &after,
    );
    let mut data = report.json();
    for (path, text) in [
        (approval::MARKDOWN, report.markdown()),
        (approval::JSON, data.to_document()),
    ] {
        if let Err(problem) = crate::derived::put(&loaded.root, path, &text) {
            return Reply::failed(*problem);
        }
    }
    data.set("file", approval::MARKDOWN);
    Reply::screen(screen(&report), crate::help::from_graph(&data))
}

fn nothing_to_compare(reason: &str) -> Diagnostic {
    arguments(
        format!("Nothing to compare with: {reason}"),
        "The approval report is the difference between this change and its before side, the default branch at its merge base, and there is none.",
        "Commit the project to git with a default branch, or name a base with `--before <branch or commit>`",
    )
}

fn not_written(path: &str) -> Diagnostic {
    Diagnostic::new(
        &rule::TOOLCHAIN_ARTIFACT_WRITE_FAILED,
        format!("`{path}` couldn't be read back"),
    )
    .reason("`jadpo check` writes the manifest the report compares, and it isn't there.")
    .next("Check that `build/` can be written, then run the command again")
}

fn read_manifest(path: &Path) -> Option<Value> {
    json::parse(&std::fs::read_to_string(path).ok()?).ok()
}

/// The before side at `commit`: the whole commit, its manifest and its
/// declines, kept in `build/before/<commit>/` once built, and used from
/// there while this compiler wrote them, since the same source and
/// compiler always build the same graph (area 18, "Comparing two
/// builds").
fn base_build(
    root: &Path,
    commit: &str,
    day: i32,
) -> Result<(String, Value, Value), Box<Diagnostic>> {
    let full = crate::before::full_commit(root, commit).unwrap_or_else(|| commit.to_string());
    let kept = format!("build/before/{full}/manifest.json");
    let kept_declines = format!("build/before/{full}/declines.json");
    if let (Some(value), Some(declines)) = (
        read_manifest(&root.join(&kept)),
        read_manifest(&root.join(&kept_declines)),
    ) && value.get("compiler").and_then(Value::as_str) == Some(VERSION)
        && value.get("format") == Some(&Value::Int(manifest::FORMAT))
    {
        return Ok((full, value, declines));
    }
    let files = crate::before::tree(root, &full);
    let mut loaded = jadpo_db::load_files(root.to_path_buf(), &files);
    loaded.set_judged_on(Some(day));
    let errors = jadpo_check::check(&loaded.db, loaded.project).len();
    let graph = jadpo_check::graph::graph(&loaded.db, loaded.project);
    let value = manifest::manifest(
        graph,
        &Stamp {
            compiler: VERSION.to_string(),
            errors,
            source: Some(Source {
                commit: Some(full.clone()),
                modified: false,
            }),
            deployables: Vec::new(),
        },
    );
    let declines = approval::declines_json(&loaded.db, loaded.project);
    crate::derived::put(root, &kept, &value.to_document())?;
    crate::derived::put(root, &kept_declines, &declines.to_document())?;
    Ok((full, value, declines))
}

fn count(n: usize, one: &str) -> String {
    format!("{n} {}", approval::plural(one, n))
}

/// What the terminal shows of `report`.
fn screen(report: &Report) -> Screen {
    let mut screen = Screen::new();
    let decisions = report.decisions.len();
    if decisions == 0 {
        screen.push(Block::Section {
            heading: "No decision needs a person".to_string(),
            lines: Vec::new(),
        });
    } else {
        screen.push(Block::Decisions {
            heading: format!(
                "{} {} a person",
                count(decisions, "decision"),
                if decisions == 1 { "needs" } else { "need" }
            ),
            rows: report
                .decisions
                .iter()
                .map(|decision| DecisionRow {
                    decided: decision.acceptance.decided(),
                    text: decision.need.said.clone(),
                    state: match decision.acceptance {
                        Acceptance::Missing => "acceptance missing".to_string(),
                        Acceptance::NoReason { .. } => "reason missing".to_string(),
                        Acceptance::Accepted { .. } => "accepted".to_string(),
                    },
                    path: decision
                        .paths
                        .shown
                        .first()
                        .map(|path| path.iter().map(|step| step.name.clone()).collect())
                        .unwrap_or_default(),
                })
                .collect(),
        });
    }
    if !report.reachable.is_empty() {
        let mut rows: Vec<Row> = report
            .reachable
            .iter()
            .take(MOST_LISTED)
            .map(|reachable| {
                Row::new(
                    reachable.name.clone(),
                    format!("now reached from {}", reachable.from.join(", ")),
                )
            })
            .collect();
        if report.reachable.len() > MOST_LISTED {
            rows.push(Row::new(
                "…",
                format!(
                    "{} more in {}",
                    report.reachable.len() - MOST_LISTED,
                    approval::MARKDOWN
                ),
            ));
        }
        screen.push(Block::List {
            heading: format!(
                "{} newly reachable",
                count(report.reachable.len(), "declaration")
            ),
            rows,
        });
    }
    let mut before = report.base.described.clone();
    if report.base_errors > 0 {
        before.push_str(&format!(
            ", which has {} with this compiler",
            count(report.base_errors as usize, "error")
        ));
    }
    for path in &report.not_judged {
        before.push_str(&format!(
            "; `{path}` is new, so the changes in it aren't judged"
        ));
    }
    let mut fields = vec![("before".to_string(), before)];
    let label = |key: &'static str| -> &'static str {
        match key {
            "integrations" => "outside",
            key => key,
        }
    };
    let mut other = false;
    for section in &report.other {
        if section.is_empty() {
            continue;
        }
        other = true;
        fields.push((label(section.key).to_string(), section.counts().join(", ")));
    }
    if !other {
        fields.push(("other".to_string(), "no other change".to_string()));
    }
    fields.push((
        "source".to_string(),
        if report.source.is_empty() {
            "no change".to_string()
        } else {
            format!(
                "{} in {}",
                count(report.source.len(), "declaration"),
                count(report.source_files(), "file")
            )
        },
    ));
    screen.push(Block::Fields(fields));
    screen.push(Block::Status {
        outcome: Outcome::Success,
        text: format!("Wrote {}", approval::MARKDOWN),
        rows: Vec::new(),
    });
    let (missing, reason) = report.waiting();
    if missing > 0 {
        screen.push(Block::Decision {
            subject: format!("{} for a person's acceptance", waiting_changes(missing)),
            location: None,
            next:
                "A person accepts them: `jadpo acceptance new <purpose>` writes the draft for them."
                    .to_string(),
        });
    } else if reason > 0 {
        let file = report.decisions.iter().find_map(|d| match &d.acceptance {
            Acceptance::NoReason { file } => Some(format!("{file}:1")),
            _ => None,
        });
        screen.push(Block::Decision {
            subject: format!("{} for a person's reason", waiting_changes(reason)),
            location: file,
            next: "A person reads the changes and writes why they are accepted on a `///` line above `acceptance`.".to_string(),
        });
    }
    screen
}

fn waiting_changes(n: usize) -> String {
    if n == 1 {
        "1 change waits".to_string()
    } else {
        format!("{n} changes wait")
    }
}
