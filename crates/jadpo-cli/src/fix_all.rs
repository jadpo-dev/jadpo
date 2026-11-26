//! `jadpo fix` alone (area 14, "`jadpo fix`"; chunk 10.9a): applies every
//! edit `jadpo check`'s errors carry that is an agent's to make and leaves
//! no choice, then checks again, until none is left. Anything that needs a
//! person is never applied: an acceptance, a widening of who may read or
//! write, a migration's decision ([`jadpo_diag::Diagnostic::automatic`]).
//! A finding with a real choice (swap or rename) stays a command of its
//! own.
//!
//! Designed before it was built (impeccable's shape pass against the CLI
//! design language, as `jadpo fix <name> swap` was, run unattended, so its
//! assumptions are marked):
//!
//! - *Job.* An agent whose check failed with errors that carry their own
//!   repair applies them all in one call, sees what changed, and learns
//!   how many errors are left without reading them again. Mode: operate.
//!   The source diff is the full record; the screen is the summary.
//! - The change list reads as `jadpo fix <name> swap`'s does: grouped by
//!   file in the order written, a row per error fixed, `~ line 20` (`-`
//!   when the edit only removes, `+` when it only adds), then the step it
//!   made, cut before any "or …" alternative it didn't take.
//! - Then one status line, `✓ Fixed 3 errors in 2 files`, and the hint:
//!   `jadpo check` with the errors left (assumption: their text is worth
//!   reading only once the automatic ones are gone, so they are counted,
//!   not shown), or the first standard-type finding's command, or none.
//! - *States.* Nothing automatic: the standard-type findings as before, or
//!   `✓ Nothing to fix`. Files that don't parse: only the parser's own
//!   edits are applied. An edit that would break a file's parse is put
//!   back and the run stops there.

use std::collections::BTreeMap;
use std::path::Path;

use jadpo_diag::{Diagnostic, rule};
use jadpo_term::{Block, ChangeMark, Json, Outcome, Screen};

use crate::run::{Call, Reply};

/// The most rounds of applying and checking again: a fix can bring the
/// next to light (`attempt` written, then the `fails` list it needs).
const ROUNDS: usize = 12;

/// One file's rows in the change list.
type Group = (String, Vec<(ChangeMark, Vec<String>)>);

/// One error fixed: the file, the line its first edit was on, the mark,
/// the step it made and the rule.
struct Fixed {
    path: String,
    line: usize,
    mark: ChangeMark,
    step: String,
    rule: &'static str,
}

/// Applies every automatic edit under `root`, round by round; `None`
/// when there was none, so the caller lists the findings instead.
pub(crate) fn fix_all(call: &Call, root: &Path) -> Option<Reply> {
    let mut fixed: Vec<Fixed> = Vec::new();
    for _ in 0..ROUNDS {
        let mut loaded = jadpo_db::load(root);
        crate::check::judge_acceptances(call, &mut loaded);
        let problems = jadpo_check::check(&loaded.db, loaded.project);
        let syntax = problems
            .iter()
            .filter(|p| p.rule.id.starts_with("syntax."))
            .count();
        let picked = picked(&problems, syntax > 0);
        if picked.is_empty() {
            break;
        }
        let texts = jadpo_db::source_texts(&loaded.db, loaded.project);
        let mut by_file: BTreeMap<String, Vec<(u32, u32, String)>> = BTreeMap::new();
        let mut round = Vec::new();
        for problem in &picked {
            let Some(next) = &problem.next else { continue };
            let mut seen: Vec<&str> = Vec::new();
            for edit in &next.edits {
                by_file.entry(edit.span.path.clone()).or_default().push((
                    edit.span.start,
                    edit.span.end,
                    edit.replacement.clone(),
                ));
                if seen.contains(&edit.span.path.as_str()) {
                    continue;
                }
                seen.push(&edit.span.path);
                let of_file: Vec<&jadpo_diag::Edit> = next
                    .edits
                    .iter()
                    .filter(|e| e.span.path == edit.span.path)
                    .collect();
                let text = texts.get(&edit.span.path).map(String::as_str).unwrap_or("");
                let mark = mark_of(&of_file);
                // A line added after another is named by its own number.
                let below =
                    usize::from(mark == ChangeMark::Added && edit.replacement.starts_with('\n'));
                round.push(Fixed {
                    path: edit.span.path.clone(),
                    line: line_of(text, edit.span.start) + below,
                    mark,
                    step: step_of(&next.text),
                    rule: problem.rule.id,
                });
            }
        }
        let mut written: Vec<(String, Option<String>)> = Vec::new();
        for (path, edits) in by_file {
            let before = texts.get(&path).cloned();
            let text = before.clone().unwrap_or_default();
            let after = jadpo_check::apply_edits(&text, edits);
            if write_one(root, &path, &after).is_err() {
                restore(root, &written);
                return Some(Reply::failed(
                    Diagnostic::new(
                        &rule::TOOLCHAIN_ARTIFACT_WRITE_FAILED,
                        format!("`{path}` couldn't be written"),
                    )
                    .reason("Writing it failed, so the fixes of this round were put back.")
                    .next("Check the folder can be written, then run `jadpo fix` again"),
                ));
            }
            written.push((path, before));
        }
        // A round that leaves more files unparsed than it found is put
        // back, and nothing more is tried.
        let after = jadpo_db::load(root);
        let broken = jadpo_check::check(&after.db, after.project)
            .iter()
            .filter(|p| p.rule.id.starts_with("syntax."))
            .count();
        if broken > syntax {
            restore(root, &written);
            break;
        }
        fixed.extend(round);
    }
    if fixed.is_empty() {
        return None;
    }
    Some(report(call, root, fixed))
}

/// The errors whose edits this round applies: each automatic, in the
/// order `jadpo check` reports them, none touching another's place; with
/// files that don't parse, only the parser's own.
fn picked(problems: &[Diagnostic], syntax: bool) -> Vec<&Diagnostic> {
    let mut taken: Vec<(&str, u32, u32)> = Vec::new();
    let mut out = Vec::new();
    for problem in problems {
        if !problem.automatic() || (syntax && !problem.rule.id.starts_with("syntax.")) {
            continue;
        }
        let Some(next) = &problem.next else { continue };
        let touches = next.edits.iter().any(|edit| {
            taken.iter().any(|(path, start, end)| {
                *path == edit.span.path && edit.span.start <= *end && *start <= edit.span.end
            })
        });
        if touches {
            continue;
        }
        for edit in &next.edits {
            taken.push((&edit.span.path, edit.span.start, edit.span.end));
        }
        out.push(problem);
    }
    out
}

/// Writes `text` at `path` under `root`; a source file left empty, as a
/// move leaves its old home, is removed.
fn write_one(root: &Path, path: &str, text: &str) -> std::io::Result<()> {
    let file = root.join(path);
    if text.trim().is_empty() && path.ends_with(".jadpo") {
        return match std::fs::remove_file(&file) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error),
            _ => Ok(()),
        };
    }
    if let Some(folder) = file.parent() {
        std::fs::create_dir_all(folder)?;
    }
    std::fs::write(file, text)
}

/// Puts each file written back as it was.
fn restore(root: &Path, written: &[(String, Option<String>)]) {
    for (path, before) in written {
        let file = root.join(path);
        let _ = match before {
            Some(text) => std::fs::write(file, text),
            None => std::fs::remove_file(file),
        };
    }
}

fn line_of(text: &str, offset: u32) -> usize {
    let end = (offset as usize).min(text.len());
    text.as_bytes()[..end]
        .iter()
        .filter(|b| **b == b'\n')
        .count()
        + 1
}

fn mark_of(edits: &[&jadpo_diag::Edit]) -> ChangeMark {
    if edits.iter().all(|e| e.replacement.is_empty()) {
        ChangeMark::Removed
    } else if edits.iter().all(|e| e.span.start == e.span.end) {
        ChangeMark::Added
    } else {
        ChangeMark::Changed
    }
}

/// The step an error's fix made, without the alternative it didn't take.
fn step_of(next: &str) -> String {
    let step = next
        .split_once(", or ")
        .map_or(next, |(first, _)| first)
        .trim_end_matches('.');
    step.to_string()
}

fn report(call: &Call, root: &Path, fixed: Vec<Fixed>) -> Reply {
    let mut loaded = jadpo_db::load(root);
    crate::check::judge_acceptances(call, &mut loaded);
    let problems = jadpo_check::check(&loaded.db, loaded.project);
    // What `jadpo check` writes itself isn't left for anyone.
    let migration_written = crate::migrate::written_once_fixed(&loaded, &problems);
    let left = problems
        .iter()
        .filter(|p| {
            p.rule.id != rule::RECORD_OUT_OF_DATE.id
                && !(migration_written && p.rule.id == rule::MIGRATION_OUT_OF_DATE.id)
        })
        .count();
    let mut groups: Vec<Group> = Vec::new();
    let mut fixed = fixed;
    // Each file's rows in line order, the files in the order first fixed.
    let order: Vec<String> = fixed.iter().fold(Vec::new(), |mut seen, fix| {
        if !seen.contains(&fix.path) {
            seen.push(fix.path.clone());
        }
        seen
    });
    fixed.sort_by_key(|fix| (order.iter().position(|p| *p == fix.path), fix.line));
    for fix in &fixed {
        let row = (
            fix.mark,
            vec![format!("line {}", fix.line), fix.step.clone()],
        );
        match groups.iter_mut().find(|(path, _)| *path == fix.path) {
            Some((_, rows)) => rows.push(row),
            None => groups.push((fix.path.clone(), vec![row])),
        }
    }
    let files = groups.len();
    let count =
        |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
    let mut screen = Screen::new()
        .with(Block::Changes { note: None, groups })
        .with(Block::Status {
            outcome: Outcome::Success,
            text: format!(
                "Fixed {} in {}",
                count(fixed.len(), "error", "errors"),
                count(files, "file", "files")
            ),
            rows: Vec::new(),
        });
    let findings = jadpo_check::fixes::findings(&loaded.db, loaded.project);
    if left > 0 {
        screen.push(Block::hint(
            "jadpo check",
            if left == 1 {
                "shows the error left".to_string()
            } else {
                format!("shows the {left} errors left")
            },
        ));
    } else if let Some(first) = findings.first() {
        screen.push(Block::hint(
            format!("jadpo fix {} swap", first.name),
            format!("uses the standard {} in its place", first.standard),
        ));
    }
    let data = Json::object([
        (
            "fixed",
            Json::array(fixed.iter().map(|fix| {
                Json::object([
                    ("path", Json::from(fix.path.as_str())),
                    ("line", Json::from(fix.line)),
                    ("rule", Json::from(fix.rule)),
                    ("change", Json::from(fix.step.as_str())),
                ])
            })),
        ),
        ("errorsLeft", left.into()),
    ]);
    Reply::screen(screen, data)
}
