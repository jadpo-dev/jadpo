//! `jadpo declines list` and `jadpo declines clear` (area 14; area 1,
//! "Declining a judgement call", question 11, chunk 5.4b): show every
//! `decline` line with its rule, place and reason, and remove them so each
//! finding comes back to be fixed or declined again. Both read and write
//! the lines through `jadpo_check::declines`, the code the editor's
//! "Decline with a reason" and "Remove decline" run.
//!
//! Designed before it was built (impeccable's shape pass against the CLI
//! design language and the screen area 1's question 11 draws, run
//! unattended, so its assumptions are marked):
//!
//! - *Job.* An agent, or a person reviewing its work, sees every judgement
//!   call the code has declined and why, to agree or not; after a compiler
//!   upgrade, a new standard type or a periodic sweep, they reset some so
//!   each is decided again. Mode: operate. Scanning beats reading.
//! - `list` is one list, as `jadpo routes` is: the bold heading `Declines`,
//!   then a row per decline in file order, the rule, its place
//!   (`path:line`, never cut) and the reason. When a reason doesn't fit in
//!   80 columns, every reason goes on the lines under its row, wrapped:
//!   it is prose a reviewer reads whole, and the list keeps one shape. The footer counts the declines, the
//!   rules and the files. No hint: nothing has to happen next.
//! - `clear` is one status line, `✓ Removed 2 declines from 2 files`, and
//!   the one command to run next, `jadpo check`, which shows each finding
//!   again. The removed lines are the diff (assumption: a change list
//!   would repeat `list`, which a reviewer runs first).
//! - `--rule <id>` and a path narrow both; an unknown rule names the
//!   declinable ones (assumption: a rule that can't be declined is named
//!   too, since a project can still hold a line naming one).
//! - *States.* Declines; none at all (`✓ No declines`); none matching the
//!   filters (`✓ No declines` with the filters as facts); a project whose
//!   files don't parse (`clear` stops on the syntax errors, as every
//!   command that writes source does; `list` still lists); a path outside
//!   the project or a missing action (`cli.declines_arguments`).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use jadpo_check::declines::{self, Decline, Filter};
use jadpo_diag::{Diagnostic, rule};
use jadpo_term::{Block, Fact, Json, Outcome, Screen, shown_path};

use crate::fmt::project_not_found;
use crate::migrate::blocked;
use crate::run::{Call, Reply, either, quoted};
use crate::suggest::nearest;

/// What `jadpo declines` does.
pub const ACTIONS: &[crate::table::Value] = &[
    crate::table::Value {
        name: "list",
        description: "List every decline with its rule, place and reason",
    },
    crate::table::Value {
        name: "clear",
        description: "Remove declines, so each finding comes back",
    },
];

/// The widest a list row may be before its reason is cut.
const WIDTH: usize = 80;

pub fn declines(call: &Call) -> Reply {
    let action = match call.arguments.first().map(String::as_str) {
        Some(action @ ("list" | "clear")) => action,
        Some(other) => {
            let names: Vec<&str> = ACTIONS.iter().map(|action| action.name).collect();
            let diagnostic = Diagnostic::new(
                &rule::CLI_DECLINES_ARGUMENTS,
                format!("`{other}` isn't something `jadpo declines` does"),
            );
            return Reply::failed(match nearest(other, names.iter().copied()) {
                Some(near) => diagnostic
                    .reason(format!(
                        "`jadpo declines` can {}. Did you mean `{near}`?",
                        either(&quoted(&names))
                    ))
                    .next(format!("Use `jadpo declines {near}`")),
                None => diagnostic
                    .reason(format!("`jadpo declines` can {}.", either(&quoted(&names))))
                    .next("Run `jadpo declines list` to see every decline"),
            });
        }
        None => {
            return Reply::failed(
                Diagnostic::new(
                    &rule::CLI_DECLINES_ARGUMENTS,
                    "`jadpo declines` needs to know what to do",
                )
                .reason("`list` shows every decline with its reason; `clear` removes them, so each finding comes back.")
                .next("Run `jadpo declines list`"),
            );
        }
    };
    let cwd = call.cwd.clone().unwrap_or_else(|| PathBuf::from("."));
    let Some(root) = jadpo_db::find_root(&cwd) else {
        return project_not_found("declines");
    };
    let loaded = jadpo_db::load(&root);
    let path = match call.arguments.get(1) {
        None => None,
        Some(written) => match within(&loaded, &cwd, written) {
            Some(path) => Some(path),
            None => {
                return Reply::failed(
                    Diagnostic::new(
                        &rule::CLI_DECLINES_ARGUMENTS,
                        format!("`{written}` isn't in this project"),
                    )
                    .reason(format!(
                        "`jadpo declines {action}` looks at the files of the project `app.jadpo` is in, and `{written}` is outside it."
                    ))
                    .next(format!(
                        "Give a file or folder inside the project, or run `jadpo declines {action}` without one"
                    )),
                );
            }
        },
    };
    let rule = call.value("--rule").map(str::to_string);
    if let Some(id) = &rule
        && jadpo_diag::find(id).is_none()
    {
        let declinable = declines::declinable_rules();
        let diagnostic = Diagnostic::new(
            &rule::CLI_DECLINES_ARGUMENTS,
            format!("`{id}` isn't a rule"),
        );
        return Reply::failed(match nearest(id, declinable.iter().copied()) {
            Some(near) => diagnostic
                .reason(format!("Jadpo has no rule `{id}`. Did you mean `{near}`?"))
                .next(format!("Use `--rule {near}`")),
            None => diagnostic
                .reason(format!(
                    "Jadpo has no rule `{id}`. The rules that can be declined are {}.",
                    either(
                        &declinable
                            .iter()
                            .map(|id| format!("`{id}`"))
                            .collect::<Vec<_>>()
                    )
                ))
                .next("Give `--rule` a rule's id, as its error's `rule:` line writes it"),
        });
    }
    let filter = Filter { rule, path };
    match action {
        "list" => list(&loaded, &cwd, &filter),
        _ => clear(&loaded, &cwd, &filter),
    }
}

/// `written`, relative to `cwd`, as a project-relative path; `.` for the
/// root. `None` outside the project.
fn within(loaded: &jadpo_db::Loaded, cwd: &Path, written: &str) -> Option<String> {
    let joined = cwd.join(written);
    let absolute = joined.canonicalize().unwrap_or(joined);
    let root = loaded.root.canonicalize().unwrap_or(loaded.root.clone());
    let relative = absolute.strip_prefix(&root).ok()?;
    let shown = relative
        .components()
        .map(|part| part.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/");
    Some(if shown.is_empty() {
        ".".to_string()
    } else {
        shown
    })
}

fn count(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// The filters as facts, for a footer that found nothing.
fn filter_facts(filter: &Filter) -> Vec<Fact> {
    let mut facts = Vec::new();
    if let Some(rule) = &filter.rule {
        facts.push(Fact::plain(format!("rule {rule}")));
    }
    if let Some(path) = filter.path.as_deref().filter(|path| *path != ".") {
        facts.push(Fact::plain(format!("in {path}")));
    }
    facts
}

fn decline_json(decline: &Decline) -> Json {
    Json::object([
        ("rule", Json::from(decline.rule.as_str())),
        ("path", Json::from(decline.path.as_str())),
        ("line", Json::from(decline.line as usize)),
        ("reason", decline.reason.as_deref().into()),
        ("on", decline.subject.as_deref().into()),
    ])
}

/// `text` in lines of at most `width` characters, broken between words.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > width {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

fn list(loaded: &jadpo_db::Loaded, cwd: &Path, filter: &Filter) -> Reply {
    let shown_root = jadpo_db::shown_root(cwd, &loaded.root);
    let found: Vec<Decline> = declines::declines(&loaded.db, loaded.project)
        .into_iter()
        .filter(|decline| filter.matches(decline))
        .collect();
    let places: Vec<String> = found
        .iter()
        .map(|decline| {
            format!(
                "{}:{}",
                shown_path(shown_root.as_deref(), &decline.path),
                decline.line
            )
        })
        .collect();
    let rule_width = found.iter().map(|d| d.rule.len()).max().unwrap_or(0);
    let place_width = places.iter().map(String::len).max().unwrap_or(0);
    let rows: Vec<String> = found
        .iter()
        .zip(&places)
        .map(|(decline, place)| {
            let reason = decline.reason.as_deref().unwrap_or("");
            format!("{:rule_width$}  {place:place_width$}  {reason}", decline.rule)
                .trim_end()
                .to_string()
        })
        .collect();
    // When any reason doesn't fit beside its row, every reason goes on the
    // lines under its row, wrapped, so the list keeps one shape.
    let lines: Vec<String> = if rows.iter().all(|row| row.chars().count() <= WIDTH - 2) {
        rows
    } else {
        let mut lines = Vec::new();
        for (decline, place) in found.iter().zip(&places) {
            lines.push(format!("{:rule_width$}  {place}", decline.rule));
            let reason = decline.reason.as_deref().unwrap_or("");
            lines.extend(wrap(reason, WIDTH - 4).into_iter().map(|line| format!("  {line}")));
        }
        lines
    };
    let rules: BTreeSet<&str> = found.iter().map(|d| d.rule.as_str()).collect();
    let files: BTreeSet<&str> = found.iter().map(|d| d.path.as_str()).collect();
    let mut screen = Screen::new();
    if found.is_empty() {
        screen.push(Block::summary(
            Outcome::Success,
            "No declines",
            filter_facts(filter),
        ));
    } else {
        screen.push(Block::Section {
            heading: "Declines".to_string(),
            lines,
        });
        screen.push(Block::summary(
            Outcome::Success,
            count(found.len(), "decline", "declines"),
            [
                Fact::plain(count(rules.len(), "rule", "rules")),
                Fact::plain(count(files.len(), "file", "files")),
            ],
        ));
    }
    let data = Json::object([
        ("declines", Json::array(found.iter().map(decline_json))),
        ("rules", rules.len().into()),
        ("files", files.len().into()),
    ]);
    Reply::screen(screen, data)
}

fn clear(loaded: &jadpo_db::Loaded, cwd: &Path, filter: &Filter) -> Reply {
    let db = &loaded.db;
    // Removing a line from a file that doesn't parse could take the wrong
    // one, so syntax errors stop it, as in every command that writes source.
    let syntax: Vec<Diagnostic> = jadpo_check::check(db, loaded.project)
        .into_iter()
        .filter(|d| d.rule.id.starts_with("syntax."))
        .collect();
    if !syntax.is_empty() {
        return blocked(loaded, cwd, syntax, "declines stopped");
    }
    let found: Vec<Decline> = declines::declines(db, loaded.project)
        .into_iter()
        .filter(|decline| filter.matches(decline))
        .collect();
    let texts: BTreeMap<String, String> = jadpo_db::source_texts(db, loaded.project);
    let mut by_file: BTreeMap<&str, Vec<&Decline>> = BTreeMap::new();
    for decline in &found {
        by_file
            .entry(decline.path.as_str())
            .or_default()
            .push(decline);
    }
    for (path, lines) in &by_file {
        let Some(text) = texts.get(*path) else {
            continue;
        };
        let after = declines::without(text, lines);
        if let Err(error) = std::fs::write(loaded.root.join(path), after) {
            return Reply::failed(
                Diagnostic::new(
                    &rule::TOOLCHAIN_ARTIFACT_WRITE_FAILED,
                    format!("`{path}` couldn't be written"),
                )
                .reason(format!(
                    "Writing it failed: {error}. The files before it were written."
                ))
                .next_for(
                    "Check the folder can be written, then run `jadpo declines clear` again",
                    jadpo_diag::Decider::Human,
                ),
            );
        }
    }
    let data = Json::object([
        ("removed", Json::array(found.iter().map(decline_json))),
        ("files", Json::array(by_file.keys().copied())),
    ]);
    if found.is_empty() {
        let screen = Screen::new().with(Block::summary(
            Outcome::Success,
            "No declines to remove",
            filter_facts(filter),
        ));
        return Reply::screen(screen, data);
    }
    let screen = Screen::new()
        .with(Block::Status {
            outcome: Outcome::Success,
            text: format!(
                "Removed {} from {}",
                count(found.len(), "decline", "declines"),
                count(by_file.len(), "file", "files")
            ),
            rows: Vec::new(),
        })
        .with(Block::hint(
            "jadpo check",
            "shows each finding again, to fix or decline",
        ));
    Reply::screen(screen, data)
}
