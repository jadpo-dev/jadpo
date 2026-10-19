//! `jadpo records` (area 18): every record the project commits, grouped by
//! the command that writes it, each with its state, so an out-of-date one
//! is found with the command that brings it up to date. `jadpo records
//! update` brings every record up to date at once: each view written from
//! the source, each ledger rewritten as Jadpo writes it (sorted, in the
//! newest format), and records under their old names moved.
//!
//! The screen is a list per writer, a row per record with its state as a
//! symbol and a word, then the summary and the one command to run next:
//!
//! ```text
//! jadpo api update
//! ✗ api/main.jadpo_record                 out of date
//!
//! jadpo translations update
//! ✓ domains/shipping/translations/sources.jadpo_record  up to date
//!
//! ✗ records out of date  1 out of date · 2 records
//! → jadpo api update  brings api/main.jadpo_record up to date
//! ```
//!
//! It fails while a record needs its command, as `jadpo check` does.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use jadpo_check::records::{Fix, Standing, State, standings};
use jadpo_diag::{Diagnostic, rule};
use jadpo_term::{Block, Fact, Json, Outcome, Row, RowState, Screen};

use crate::fmt::project_not_found;
use crate::migrate::blocked;
use crate::run::{Call, Reply};

/// What `jadpo records` does (area 18).
pub const ACTIONS: &[crate::table::Value] = &[crate::table::Value {
    name: "update",
    description: "Bring every record up to date",
}];

pub fn records(call: &Call) -> Reply {
    match call.arguments.first().map(String::as_str) {
        None => list(call),
        Some("update") => match call.arguments.get(1) {
            Some(extra) => Reply::failed(arguments(
                format!("`jadpo records update` takes nothing after it, not `{extra}`"),
                "It brings every record up to date at once.".to_string(),
            )),
            None => update(call),
        },
        Some(other) => Reply::failed(arguments(
            format!("`{other}` isn't something `jadpo records` does"),
            "`jadpo records` lists every record and whether it is up to date; `jadpo records update` brings them all up to date.".to_string(),
        )),
    }
}

fn arguments(message: String, reason: String) -> Diagnostic {
    Diagnostic::new(&rule::CLI_RECORDS_ARGUMENTS, message)
        .reason(reason)
        .next("Run `jadpo records`, or `jadpo records update`")
}

fn load(call: &Call) -> Result<(PathBuf, jadpo_db::Loaded), Reply> {
    let cwd = call.cwd.clone().unwrap_or_else(|| PathBuf::from("."));
    let Some(root) = jadpo_db::find_root(&cwd) else {
        return Err(project_not_found("records"));
    };
    Ok((cwd, jadpo_db::load(&root)))
}

/// What a record's header says writes it, without the backticks:
/// `jadpo api update`.
fn writer(standing: &Standing) -> String {
    standing.kind().update.replace('`', "")
}

fn plural(count: usize, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

fn list(call: &Call) -> Reply {
    let (_, loaded) = match load(call) {
        Ok(found) => found,
        Err(reply) => return reply,
    };
    let all = standings(&loaded.db, loaded.project);
    let mut screen = Screen::new();
    let mut groups: BTreeMap<String, Vec<&Standing>> = BTreeMap::new();
    for standing in all {
        groups.entry(writer(standing)).or_default().push(standing);
    }
    let mut data = Vec::new();
    for (heading, rows) in &groups {
        let rows: Vec<Row> = rows
            .iter()
            .map(|standing| {
                let state = match standing.state {
                    State::UpToDate => RowState::Done,
                    State::NotChecked => RowState::Skipped,
                    _ => RowState::Failed,
                };
                data.push(Json::object([
                    ("path", Json::from(standing.path.as_str())),
                    ("kind", Json::from(standing.kind)),
                    ("written_by", Json::from(heading.as_str())),
                    ("state", Json::from(standing.state.id())),
                    ("command", standing.command().map_or(Json::Null, Json::from)),
                ]));
                Row::new(standing.path.as_str(), standing.state.words()).state(state)
            })
            .collect();
        screen.push(Block::list(heading.as_str(), rows));
    }
    let problems: Vec<&Standing> = all.iter().filter(|s| s.state.is_problem()).collect();
    let unchecked = all.iter().filter(|s| s.state == State::NotChecked).count();
    let total = plural(all.len(), "record", "records");
    if all.is_empty() {
        screen.push(Block::summary(
            Outcome::Success,
            "no records",
            [Fact::plain("nothing in the project is recorded yet")],
        ));
    } else if problems.is_empty() && unchecked == 0 {
        screen.push(Block::summary(
            Outcome::Success,
            "records up to date",
            [Fact::plain(total)],
        ));
    } else {
        let mut counts: BTreeMap<State, usize> = BTreeMap::new();
        for standing in all.iter().filter(|s| s.state != State::UpToDate) {
            *counts.entry(standing.state).or_default() += 1;
        }
        let mut facts: Vec<Fact> = counts
            .iter()
            .map(|(state, count)| Fact::problem(format!("{count} {}", state.words())))
            .collect();
        facts.push(Fact::plain(total));
        let headline = if problems.is_empty() {
            "records not checked"
        } else {
            "records out of date"
        };
        screen.push(Block::summary(Outcome::Failure, headline, facts));
        let commands: BTreeSet<&str> = problems.iter().filter_map(|s| s.command()).collect();
        let fixable: Vec<&&Standing> = problems.iter().filter(|s| s.command().is_some()).collect();
        let hint = match (commands.len(), fixable.as_slice()) {
            (0, _) => (
                "jadpo check".to_string(),
                "says what is wrong with each".to_string(),
            ),
            (1, [one]) => (
                one.command().unwrap_or_default().to_string(),
                format!("brings {} up to date", one.path),
            ),
            (1, many) => (
                commands
                    .iter()
                    .next()
                    .copied()
                    .unwrap_or_default()
                    .to_string(),
                format!(
                    "brings {} up to date",
                    plural(many.len(), "record", "records")
                ),
            ),
            _ => (
                "jadpo records update".to_string(),
                "brings every record up to date".to_string(),
            ),
        };
        screen.push(Block::hint(hint.0, hint.1));
    }
    let diagnostics: Vec<Diagnostic> = all
        .iter()
        .filter_map(|standing| standing.problem.clone())
        .collect();
    let data = Json::object([("records", Json::array(data))]);
    if diagnostics.is_empty() && unchecked == 0 {
        return Reply::screen(screen, data);
    }
    // Records that couldn't be compared fail on the source's own errors,
    // which `jadpo check` shows.
    let diagnostics = if diagnostics.is_empty() {
        jadpo_check::check(&loaded.db, loaded.project)
    } else {
        diagnostics
    };
    Reply::Report {
        screen,
        data,
        diagnostics,
        sources: BTreeMap::new(),
    }
}

fn update(call: &Call) -> Reply {
    let (cwd, loaded) = match load(call) {
        Ok(found) => found,
        Err(reply) => return reply,
    };
    let (db, project) = (&loaded.db, loaded.project);
    let all = standings(db, project);
    // A view is written from source that reads cleanly; a record with no
    // fix (a newer format, a line only a person can put back) stops it
    // too, so nothing is half written.
    if all.iter().any(|s| s.state == State::NotChecked) {
        let problems: Vec<Diagnostic> = jadpo_check::check(db, project)
            .into_iter()
            .filter(|d| d.rule.area != 18)
            .collect();
        return blocked(&loaded, &cwd, problems, "records update stopped");
    }
    let stuck: Vec<Diagnostic> = all
        .iter()
        .filter(|s| s.state.is_problem() && s.fixes.is_empty())
        .filter_map(|s| s.problem.clone())
        .collect();
    if !stuck.is_empty() {
        return blocked(&loaded, &cwd, stuck, "records update stopped");
    }
    let mut screen = Screen::new();
    let mut entries = Vec::new();
    let (mut wrote, mut removed) = (0usize, 0usize);
    for standing in all {
        for (path, fix) in &standing.fixes {
            let full = loaded.root.join(path);
            let done = match fix {
                Fix::Write(text) => {
                    let result = full
                        .parent()
                        .map_or(Ok(()), std::fs::create_dir_all)
                        .and_then(|()| std::fs::write(&full, text));
                    wrote += 1;
                    result.map(|()| ("Wrote", "written"))
                }
                Fix::Remove => {
                    removed += 1;
                    std::fs::remove_file(&full).map(|()| ("Removed", "removed"))
                }
            };
            let (verb, change) = match done {
                Ok(done) => done,
                Err(error) => {
                    return Reply::failed(
                        Diagnostic::new(
                            &rule::TOOLCHAIN_ARTIFACT_WRITE_FAILED,
                            format!("`{path}` couldn't be written"),
                        )
                        .reason(format!("Writing it failed: {error}."))
                        .next("Check that the folder can be written, then run `jadpo records update` again"),
                    );
                }
            };
            screen.push(Block::Status {
                outcome: Outcome::Success,
                text: format!("{verb} {path}"),
                rows: Vec::new(),
            });
            entries.push(Json::object([
                ("path", Json::from(path.as_str())),
                ("change", Json::from(change)),
            ]));
        }
    }
    let total = plural(all.len(), "record", "records");
    if wrote + removed == 0 {
        screen.push(Block::summary(
            Outcome::Success,
            "records up to date",
            [Fact::plain(total)],
        ));
    } else {
        let mut facts = Vec::new();
        if wrote > 0 {
            facts.push(Fact::plain(format!("{wrote} written")));
        }
        if removed > 0 {
            facts.push(Fact::plain(format!("{removed} removed")));
        }
        facts.push(Fact::plain(total));
        screen.push(Block::summary(Outcome::Success, "records updated", facts));
        screen.push(Block::hint(
            "git diff",
            "shows what changed, to commit with the code",
        ));
    }
    Reply::screen(screen, Json::object([("records", Json::array(entries))]))
}
