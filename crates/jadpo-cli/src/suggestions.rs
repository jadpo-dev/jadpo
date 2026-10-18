//! `jadpo suggest` (area 14, "Suggestions"; chunk 5.5x): where code could
//! use a standard type or operation, read through `jadpo_check::suggest`,
//! and the agent's routine around it: `--catalogue` to review the rest,
//! `ignore` to keep a finding quiet, `found` to teach a miss, `share` to
//! send what was learned (roadmap 5.5y). It hints and never edits source,
//! apart from formatting it first as `jadpo fmt` does.
//!
//! Designed before it was built (impeccable's shape pass against the CLI
//! design language, matching `jadpo fix` and `jadpo declines`, run
//! unattended, so its assumptions are marked):
//!
//! - *Job.* An agent that has written code finds where a standard type or
//!   operation would do the job, makes the edits itself, and tells Jadpo
//!   what it found or chose to leave. Mode: review. The agent reads every
//!   line, so the screen is a list it can act on, then the routine.
//! - The scan is one list, as `jadpo fix` is: the bold heading
//!   `Suggestions`, a row per finding in file order, its number (`S1`), its
//!   place (`path:line`, never cut), what it suggests, the signal, and the
//!   snippet; when a snippet doesn't fit in 80 columns, every snippet goes
//!   on the line under its row, as `jadpo declines list` does with long
//!   reasons, cut with `…` only past that line. Then the bold heading `What
//!   to do` and the routine, one instruction per line, each command whole.
//!   The footer counts the suggestions and files, and the findings taken
//!   since the last run (assumption: a count, since the findings
//!   themselves are gone). No hint: the routine says what to do.
//! - *States.* Suggestions; none (`✓ No suggestions`, the routine kept,
//!   since reviewing the rest still applies, without the ignore line);
//!   files that don't parse (stopped on the syntax errors, as every
//!   command that writes source is); a hand-edited record
//!   (`suggest.record_invalid`).
//! - `ignore` and `found` are each one status line with rows, as `jadpo
//!   config set` is: `✓ Ignored S1, Money conversion` with the snippet,
//!   place and reason; `✓ Recorded a missed Money at <place>` with the
//!   snippet and reason, and the hint `jadpo suggest`, which finds the
//!   pattern elsewhere.
//! - `--catalogue` is one list headed `Catalogue`: each type or operation,
//!   the signal and what it looks for, wrapped under its row when long.
//! - *Refused* (a missing or unknown number, a place with nothing to read,
//!   an unknown type or operation, no reason) is `cli.suggest_arguments`,
//!   with the nearest name.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use jadpo_check::suggest::{self, Finding, Signal, Snippet, Taught, record};
use jadpo_diag::{Diagnostic, rule};
use jadpo_term::{Block, Fact, Json, Outcome, Screen, shown_path};
use serde_json::Value;

use crate::fmt::project_needed;
use crate::migrate::blocked;
use crate::run::{Call, Reply, either, quoted};
use crate::suggest::nearest;

/// What `jadpo suggest` does besides scanning.
pub const ACTIONS: &[crate::table::Value] = &[
    crate::table::Value {
        name: "ignore",
        description: "Keep a suggestion quiet until its code changes",
    },
    crate::table::Value {
        name: "found",
        description: "Record something the scan missed, before changing it",
    },
    crate::table::Value {
        name: "share",
        description: "Send what was learned to Jadpo HQ",
    },
];

/// The local, untracked folder, and the files in it.
const LOCAL: &str = ".jadpo";
const LAST: &str = ".jadpo/suggest_last";
const PENDING: &str = ".jadpo/suggest_pending";
const SIGNALS: &str = ".jadpo/suggest_signals";

/// The widest a list row may be.
const WIDTH: usize = 80;

pub fn suggest(call: &Call) -> Reply {
    if call.flag("--catalogue") {
        if !call.arguments.is_empty() {
            return Reply::failed(
                Diagnostic::new(
                    &rule::CLI_SUGGEST_ARGUMENTS,
                    "`--catalogue` lists the signals on its own",
                )
                .reason("The catalogue is the same for every project, so it takes no action.")
                .next("Run `jadpo suggest --catalogue`"),
            );
        }
        return catalogue();
    }
    let cwd = call.cwd.clone().unwrap_or_else(|| PathBuf::from("."));
    let Some(root) = jadpo_db::find_root(&cwd) else {
        return project_needed("suggest");
    };
    match call.arguments.first().map(String::as_str) {
        None => scan(call, &cwd, &root),
        Some("ignore") => ignore(call, &root),
        Some("found") => found(call, &cwd, &root),
        Some("share") => share(),
        Some(other) => {
            let names: Vec<&str> = ACTIONS.iter().map(|action| action.name).collect();
            let diagnostic = Diagnostic::new(
                &rule::CLI_SUGGEST_ARGUMENTS,
                format!("`{other}` isn't something `jadpo suggest` does"),
            );
            Reply::failed(match nearest(other, names.iter().copied()) {
                Some(near) => diagnostic
                    .reason(format!(
                        "`jadpo suggest` scans, or can {}. Did you mean `{near}`?",
                        either(&quoted(&names))
                    ))
                    .next(format!("Use `jadpo suggest {near}`")),
                None => diagnostic
                    .reason(format!(
                        "`jadpo suggest` scans, or can {}.",
                        either(&quoted(&names))
                    ))
                    .next("Run `jadpo suggest` to scan the project"),
            })
        }
    }
}

fn arguments(message: impl Into<String>, reason: impl Into<String>, next: impl Into<String>) -> Reply {
    Reply::failed(
        Diagnostic::new(&rule::CLI_SUGGEST_ARGUMENTS, message)
            .reason(reason)
            .next(next),
    )
}

fn count(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

// --- local files --------------------------------------------------------

/// The JSON objects of a JSON-lines file; nothing when it is missing, and
/// a line that isn't one is skipped, since the file is local state.
fn read_lines(root: &Path, file: &str) -> Vec<serde_json::Map<String, Value>> {
    let Ok(text) = std::fs::read_to_string(root.join(file)) else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|line| match serde_json::from_str(line) {
            Ok(Value::Object(object)) => Some(object),
            _ => None,
        })
        .collect()
}

/// Makes `.jadpo/` and keeps it out of git.
fn local_folder(root: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(root.join(LOCAL))?;
    let path = root.join(".gitignore");
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let ignored = text
        .lines()
        .any(|line| matches!(line.trim(), ".jadpo" | ".jadpo/" | "/.jadpo" | "/.jadpo/"));
    if !ignored {
        let mut added = text;
        if !added.is_empty() && !added.ends_with('\n') {
            added.push('\n');
        }
        added.push_str(".jadpo/\n");
        std::fs::write(&path, added)?;
    }
    Ok(())
}

fn write_lines(root: &Path, file: &str, lines: &[Value]) -> Result<(), Box<Diagnostic>> {
    let mut text = String::new();
    for line in lines {
        text.push_str(&line.to_string());
        text.push('\n');
    }
    local_folder(root)
        .and_then(|()| std::fs::write(root.join(file), text))
        .map_err(|error| write_failed(file, &error))
}

fn append_lines(root: &Path, file: &str, lines: &[Value]) -> Result<(), Box<Diagnostic>> {
    if lines.is_empty() {
        return Ok(());
    }
    let mut kept: Vec<Value> = read_lines(root, file).into_iter().map(Value::Object).collect();
    kept.extend(lines.iter().cloned());
    write_lines(root, file, &kept)
}

fn write_failed(file: &str, error: &std::io::Error) -> Box<Diagnostic> {
    Box::new(
        Diagnostic::new(
            &rule::TOOLCHAIN_ARTIFACT_WRITE_FAILED,
            format!("`{file}` couldn't be written"),
        )
        .reason(format!("{error}."))
        .next("Check that the project's folder is writable, then run the command again"),
    )
}

fn text(object: &serde_json::Map<String, Value>, key: &str) -> String {
    object
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// A pending entry to share (area 14, "What is kept, and where").
fn pending(kind: &str, signal: Signal, suggestion: &str, snippet: &Snippet, reason: Option<&str>, canonical: bool) -> Value {
    let mut entry = serde_json::json!({
        "kind": kind,
        "signal": signal.word(),
        "suggestion": suggestion,
        "hash": snippet.hash,
        "textHash": snippet.text_hash,
        "text": snippet.text,
    });
    if canonical {
        entry["canonical"] = Value::from(snippet.canonical.clone());
    }
    if let Some(reason) = reason {
        entry["reason"] = Value::from(reason);
    }
    entry
}

/// A finding as the last scan's note keeps it.
fn noted(number: usize, finding: &Finding) -> Value {
    let snippet = &finding.snippet;
    serde_json::json!({
        "number": format!("S{number}"),
        "path": snippet.path,
        "line": snippet.line,
        "start": snippet.start,
        "end": snippet.end,
        "signal": finding.signal.word(),
        "suggestion": finding.suggestion,
        "reason": finding.reason,
        "text": snippet.text,
        "shown": snippet.shown,
        "canonical": snippet.canonical,
        "hash": snippet.hash,
        "textHash": snippet.text_hash,
    })
}

/// A snippet back from the note.
fn snippet_of(object: &serde_json::Map<String, Value>) -> Snippet {
    let number = |key: &str| object.get(key).and_then(Value::as_u64).unwrap_or(0) as u32;
    Snippet {
        path: text(object, "path"),
        line: number("line"),
        start: number("start"),
        end: number("end"),
        text: text(object, "text"),
        shown: text(object, "shown"),
        canonical: text(object, "canonical"),
        hash: text(object, "hash"),
        text_hash: text(object, "textHash"),
    }
}

fn taught(root: &Path) -> Vec<Taught> {
    read_lines(root, SIGNALS)
        .iter()
        .map(|object| Taught {
            hash: text(object, "hash"),
            canonical: text(object, "canonical"),
            suggestion: text(object, "suggestion"),
            reason: text(object, "reason"),
        })
        .collect()
}

/// The ignored suggestions, or why the record can't be read.
fn ignores(root: &Path) -> Result<(Vec<record::Ignore>, bool), Box<Diagnostic>> {
    let path = root.join(record::FILE);
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Ok((Vec::new(), false));
    };
    record::parse(&text).map(|entries| (entries, true)).map_err(|(line, why)| {
        Box::new(
            Diagnostic::new(
                &rule::SUGGEST_RECORD_INVALID,
                format!("`{}` can't be read", record::FILE),
            )
            .reason(format!(
                "Line {}: {why}. The record is written by `jadpo suggest ignore`, never by hand.",
                line + 1
            ))
            .next(format!(
                "Restore `{}` from git, then ignore suggestions again with `jadpo suggest ignore`",
                record::FILE
            )),
        )
    })
}

fn write_record(root: &Path, entries: &[record::Ignore]) -> Result<(), Box<Diagnostic>> {
    std::fs::write(root.join(record::FILE), record::render(entries))
        .map_err(|error| write_failed(record::FILE, &error))
}

// --- the scan -----------------------------------------------------------

/// The routine the scan ends with: the agent's instruction.
fn routine(any: bool) -> Vec<String> {
    let mut lines = vec![
        "Review the rest of the code against `jadpo suggest --catalogue`.".to_string(),
        "Before changing code for something it missed, run".to_string(),
        "`jadpo suggest found <file>:<line> <Type or operation> \"<why>\"`.".to_string(),
    ];
    if any {
        lines.push("To keep one quiet, run `jadpo suggest ignore <number> \"<why>\"`.".to_string());
    }
    lines.push("When done, run `jadpo suggest share`.".to_string());
    lines
}

fn cut(text: &str, width: usize) -> String {
    let one_line = text.replace('\n', " ");
    if one_line.chars().count() <= width {
        return one_line;
    }
    let kept: String = one_line.chars().take(width.saturating_sub(1)).collect();
    format!("{}…", kept.trim_end())
}

fn scan(call: &Call, cwd: &Path, root: &Path) -> Reply {
    // Formatting first, as `jadpo fmt` does, so a snippet's text is the
    // formatter's.
    let _ = crate::fmt::fmt(&Call {
        arguments: Vec::new(),
        flags: Vec::new(),
        values: Vec::new(),
        cwd: Some(root.to_path_buf()),
        environment: call.environment.clone(),
    });
    let loaded = jadpo_db::load(root);
    let syntax: Vec<Diagnostic> = jadpo_check::check(&loaded.db, loaded.project)
        .into_iter()
        .filter(|d| d.rule.id.starts_with("syntax."))
        .collect();
    if !syntax.is_empty() {
        return blocked(&loaded, cwd, syntax, "suggest stopped");
    }
    let (mut ignored, recorded) = match ignores(root) {
        Ok(read) => read,
        Err(problem) => return Reply::failed(*problem),
    };
    let all = suggest::scan(&loaded.db, loaded.project, &taught(root));
    // An ignore no finding has any more is stale: its code changed or went.
    let before = ignored.len();
    ignored.retain(|ignore| {
        all.iter()
            .any(|f| ignore.covers(&f.snippet.hash, f.signal, &f.suggestion))
    });
    let stale = before - ignored.len();
    if recorded && stale > 0
        && let Err(problem) = write_record(root, &ignored)
    {
        return Reply::failed(*problem);
    }
    let (quiet, findings): (Vec<Finding>, Vec<Finding>) = all.into_iter().partition(|f| {
        ignored
            .iter()
            .any(|ignore| ignore.covers(&f.snippet.hash, f.signal, &f.suggestion))
    });
    // A finding of the last scan that is gone was taken.
    let mut accepted = Vec::new();
    for object in read_lines(root, LAST) {
        let snippet = snippet_of(&object);
        let Some(signal) = Signal::from_word(&text(&object, "signal")) else {
            continue;
        };
        let suggestion = text(&object, "suggestion");
        let still = findings
            .iter()
            .chain(&quiet)
            .any(|f| f.snippet.hash == snippet.hash && f.signal == signal && f.suggestion == suggestion);
        let now_ignored = ignored
            .iter()
            .any(|ignore| ignore.covers(&snippet.hash, signal, &suggestion));
        if !still && !now_ignored {
            accepted.push(pending("accepted", signal, &suggestion, &snippet, None, false));
        }
    }
    if let Err(problem) = append_lines(root, PENDING, &accepted) {
        return Reply::failed(*problem);
    }
    let note: Vec<Value> = findings
        .iter()
        .enumerate()
        .map(|(i, finding)| noted(i + 1, finding))
        .collect();
    if let Err(problem) = write_lines(root, LAST, &note) {
        return Reply::failed(*problem);
    }

    let shown_root = jadpo_db::shown_root(cwd, root);
    let numbers: Vec<String> = (1..=findings.len()).map(|n| format!("S{n}")).collect();
    let places: Vec<String> = findings
        .iter()
        .map(|f| format!("{}:{}", shown_path(shown_root.as_deref(), &f.snippet.path), f.snippet.line))
        .collect();
    let width = |items: &mut dyn Iterator<Item = usize>| items.max().unwrap_or(0);
    let number_width = width(&mut numbers.iter().map(String::len));
    let place_width = width(&mut places.iter().map(String::len));
    let suggestion_width = width(&mut findings.iter().map(|f| f.suggestion.chars().count()));
    let signal_width = width(&mut findings.iter().map(|f| f.signal.word().len()));
    let prefixes: Vec<String> = findings
        .iter()
        .zip(numbers.iter().zip(&places))
        .map(|(finding, (number, place))| {
            format!(
                "{number:number_width$}  {place:place_width$}  {:suggestion_width$}  {:signal_width$}",
                finding.suggestion,
                finding.signal.word()
            )
        })
        .collect();
    // When a snippet doesn't fit beside its row, every snippet goes on the
    // line under its row, so the list keeps one shape and a snippet is
    // read whole wherever it can be.
    let fits = findings.iter().zip(&prefixes).all(|(finding, prefix)| {
        2 + prefix.chars().count() + 2 + finding.snippet.shown.chars().count() <= WIDTH
    });
    let mut lines = Vec::new();
    for (finding, prefix) in findings.iter().zip(&prefixes) {
        if fits {
            lines.push(format!("{prefix}  {}", finding.snippet.shown));
        } else {
            lines.push(prefix.clone());
            let indent = number_width + 2;
            lines.push(format!(
                "{:indent$}{}",
                "",
                cut(&finding.snippet.shown, WIDTH - 2 - indent)
            ));
        }
    }
    let files: BTreeSet<&str> = findings.iter().map(|f| f.snippet.path.as_str()).collect();
    let mut facts = Vec::new();
    if !findings.is_empty() {
        facts.push(Fact::plain(count(files.len(), "file", "files")));
    }
    if !accepted.is_empty() {
        facts.push(Fact::plain(format!("{} since the last run", count(accepted.len(), "taken", "taken"))));
    }
    if !quiet.is_empty() {
        facts.push(Fact::plain(format!("{} ignored", quiet.len())));
    }
    if stale > 0 {
        facts.push(Fact::plain(count(stale, "stale ignore dropped", "stale ignores dropped")));
    }
    let instruction = routine(!findings.is_empty());
    let mut screen = Screen::new();
    if !findings.is_empty() {
        screen.push(Block::Section {
            heading: "Suggestions".to_string(),
            lines,
        });
    }
    screen.push(Block::Section {
        heading: "What to do".to_string(),
        lines: instruction.clone(),
    });
    screen.push(Block::summary(
        Outcome::Success,
        if findings.is_empty() {
            "No suggestions".to_string()
        } else {
            count(findings.len(), "suggestion", "suggestions")
        },
        facts,
    ));
    let data = Json::object([
        (
            "findings",
            Json::array(findings.iter().zip(&numbers).map(|(f, number)| finding_json(number, f))),
        ),
        ("accepted", accepted.len().into()),
        ("ignored", quiet.len().into()),
        ("staleIgnores", stale.into()),
        ("instruction", Json::array(instruction.iter().map(|line| Json::from(line.as_str())))),
    ]);
    Reply::screen(screen, data)
}

fn finding_json(number: &str, finding: &Finding) -> Json {
    let snippet = &finding.snippet;
    Json::object([
        ("number", Json::from(number)),
        ("path", Json::from(snippet.path.as_str())),
        ("line", Json::from(snippet.line as usize)),
        ("start", Json::from(snippet.start as usize)),
        ("end", Json::from(snippet.end as usize)),
        ("suggestion", Json::from(finding.suggestion.as_str())),
        ("signal", Json::from(finding.signal.word())),
        ("reason", Json::from(finding.reason.as_str())),
        ("text", Json::from(snippet.text.as_str())),
        ("canonical", Json::from(snippet.canonical.as_str())),
        ("hash", Json::from(snippet.hash.as_str())),
        ("textHash", Json::from(snippet.text_hash.as_str())),
    ])
}

// --- ignore ---------------------------------------------------------------

fn ignore(call: &Call, root: &Path) -> Reply {
    let note = read_lines(root, LAST);
    let numbers: Vec<String> = note.iter().map(|object| text(object, "number")).collect();
    let Some(written) = call.arguments.get(1) else {
        return arguments(
            "`jadpo suggest ignore` needs a suggestion's number",
            "It keeps one finding of the last scan quiet, by the number the scan gave it.",
            "Run `jadpo suggest`, then `jadpo suggest ignore <number> \"<why>\"`",
        );
    };
    let wanted = written.to_uppercase();
    let wanted = if wanted.starts_with('S') { wanted } else { format!("S{wanted}") };
    let Some(object) = note.iter().find(|object| text(object, "number") == wanted) else {
        let reason = if numbers.is_empty() {
            "The last scan found nothing to ignore, or `jadpo suggest` hasn't run here yet.".to_string()
        } else {
            format!("The last scan numbered its suggestions {}.", either(&numbers))
        };
        return arguments(
            format!("`{written}` isn't a suggestion of the last scan"),
            reason,
            "Run `jadpo suggest` to see the numbers",
        );
    };
    let reason = call.arguments.get(2).map(|why| why.trim().to_string()).unwrap_or_default();
    if reason.is_empty() {
        return arguments(
            format!("`jadpo suggest ignore {wanted}` needs a reason"),
            "A reason tells the team, and Jadpo when it is shared, why the suggestion is wrong here.",
            format!("Run `jadpo suggest ignore {wanted} \"<why>\"`"),
        );
    }
    let Some(signal) = Signal::from_word(&text(object, "signal")) else {
        return arguments(
            format!("`{wanted}` can't be read from the last scan"),
            "The scan's note is local state that has been changed.",
            "Run `jadpo suggest` again",
        );
    };
    let suggestion = text(object, "suggestion");
    let snippet = snippet_of(object);
    let (mut entries, _) = match ignores(root) {
        Ok(read) => read,
        Err(problem) => return Reply::failed(*problem),
    };
    entries.retain(|entry| !entry.covers(&snippet.hash, signal, &suggestion));
    entries.push(record::Ignore {
        hash: snippet.hash.clone(),
        signal,
        suggestion: suggestion.clone(),
        reason: reason.clone(),
    });
    if let Err(problem) = write_record(root, &entries) {
        return Reply::failed(*problem);
    }
    let entry = pending("ignored", signal, &suggestion, &snippet, Some(&reason), false);
    if let Err(problem) = append_lines(root, PENDING, &[entry]) {
        return Reply::failed(*problem);
    }
    let place = format!("{}:{}", snippet.path, snippet.line);
    let screen = Screen::new().with(Block::Status {
        outcome: Outcome::Success,
        text: format!("Ignored {wanted}, {suggestion} {}", signal.word()),
        rows: vec![
            ("snippet".to_string(), cut(&snippet.shown, 60)),
            ("place".to_string(), place.clone()),
            ("reason".to_string(), reason.clone()),
        ],
    });
    let data = Json::object([
        ("number", Json::from(wanted.as_str())),
        ("suggestion", Json::from(suggestion.as_str())),
        ("signal", Json::from(signal.word())),
        ("hash", Json::from(snippet.hash.as_str())),
        ("path", Json::from(snippet.path.as_str())),
        ("line", Json::from(snippet.line as usize)),
        ("reason", Json::from(reason.as_str())),
        ("record", Json::from(record::FILE)),
    ]);
    Reply::screen(screen, data)
}

// --- found ----------------------------------------------------------------

fn found(call: &Call, cwd: &Path, root: &Path) -> Reply {
    let usage = "Run `jadpo suggest found <file>:<line> <Type or operation> \"<why>\"`";
    let Some(place) = call.arguments.get(1) else {
        return arguments(
            "`jadpo suggest found` needs the place it missed",
            "It records the snippet at a line, before the code changes, so its text survives the fix.",
            usage,
        );
    };
    let Some((file, line)) = place
        .rsplit_once(':')
        .and_then(|(file, line)| Some((file, line.parse::<u32>().ok().filter(|n| *n > 0)?)))
    else {
        return arguments(
            format!("`{place}` isn't a place"),
            "A place is a file and a line, such as `domains/shop/libraries/pricing.jadpo:9`.",
            usage,
        );
    };
    let Some(path) = project_path(cwd, root, file) else {
        return arguments(
            format!("`{file}` isn't a file of this project"),
            "The place names a `.jadpo` file inside the project.",
            usage,
        );
    };
    let Some(suggestion) = call.arguments.get(2) else {
        return arguments(
            format!("`jadpo suggest found {place}` needs the type or operation"),
            "It records what the code should use, such as `Money` or `list.sum`.",
            usage,
        );
    };
    let known = suggest::suggestions();
    if !known.contains(suggestion) {
        let diagnostic = Diagnostic::new(
            &rule::CLI_SUGGEST_ARGUMENTS,
            format!("`{suggestion}` isn't a standard type or operation"),
        );
        return Reply::failed(match nearest(suggestion, known.iter().map(String::as_str)) {
            Some(near) => diagnostic
                .reason(format!("A suggestion names a standard type or operation. Did you mean `{near}`?"))
                .next(format!("Use `{near}` instead of `{suggestion}`")),
            None => diagnostic
                .reason("A suggestion names a standard type, such as `Money`, or an operation, such as `list.sum`.")
                .next("Run `jadpo suggest --catalogue` to see the types and operations it knows"),
        });
    }
    let reason = call.arguments.get(3).map(|why| why.trim().to_string()).unwrap_or_default();
    if reason.is_empty() {
        return arguments(
            format!("`jadpo suggest found {place} {suggestion}` needs a reason"),
            "A reason says why the code should use it, for the team and for Jadpo when it is shared.",
            format!("Run `jadpo suggest found {place} {suggestion} \"<why>\"`"),
        );
    }
    let loaded = jadpo_db::load(root);
    let Some(snippet) = suggest::snippet_at(&loaded.db, loaded.project, &path, line) else {
        return arguments(
            format!("`{path}:{line}` has nothing `jadpo suggest` can record"),
            "The line holds no statement, field or parameter: name the line where the code that should change starts.",
            usage,
        );
    };
    let mut signals: Vec<Value> = read_lines(root, SIGNALS)
        .into_iter()
        .filter(|object| !(text(object, "hash") == snippet.hash && text(object, "suggestion") == *suggestion))
        .map(Value::Object)
        .collect();
    signals.push(serde_json::json!({
        "hash": snippet.hash,
        "canonical": snippet.canonical,
        "suggestion": suggestion,
        "reason": reason,
        "text": snippet.text,
    }));
    if let Err(problem) = write_lines(root, SIGNALS, &signals) {
        return Reply::failed(*problem);
    }
    let entry = pending("found", Signal::Taught, suggestion, &snippet, Some(&reason), true);
    if let Err(problem) = append_lines(root, PENDING, &[entry]) {
        return Reply::failed(*problem);
    }
    let screen = Screen::new()
        .with(Block::Status {
            outcome: Outcome::Success,
            text: format!("Recorded a missed {suggestion} at {path}:{line}"),
            rows: vec![
                ("snippet".to_string(), cut(&snippet.shown, 60)),
                ("reason".to_string(), reason.clone()),
            ],
        })
        .with(Block::hint("jadpo suggest", "finds the same pattern elsewhere"));
    let data = Json::object([
        ("path", Json::from(path.as_str())),
        ("line", Json::from(line as usize)),
        ("suggestion", Json::from(suggestion.as_str())),
        ("reason", Json::from(reason.as_str())),
        ("text", Json::from(snippet.text.as_str())),
        ("canonical", Json::from(snippet.canonical.as_str())),
        ("hash", Json::from(snippet.hash.as_str())),
        ("textHash", Json::from(snippet.text_hash.as_str())),
    ]);
    Reply::screen(screen, data)
}

/// `file` as the project names it, when it is one of its `.jadpo` files.
fn project_path(cwd: &Path, root: &Path, file: &str) -> Option<String> {
    let full = std::fs::canonicalize(cwd.join(file)).ok()?;
    let root = std::fs::canonicalize(root).ok()?;
    let relative = full.strip_prefix(&root).ok()?;
    let shown = relative
        .components()
        .map(|part| part.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/");
    (shown.ends_with(".jadpo") && full.is_file()).then_some(shown)
}

// --- share and the catalogue ---------------------------------------------

fn share() -> Reply {
    Reply::failed(
        Diagnostic::new(
            &rule::TOOLCHAIN_NOT_BUILT_YET,
            "`jadpo suggest share` isn't built yet",
        )
        .reason(format!(
            "Sending what `jadpo suggest` learned to Jadpo HQ is roadmap chunk 5.5y. Nothing was sent, and `{PENDING}` keeps every entry until then."
        ))
        .next("Carry on: the entries wait for sharing to be built"),
    )
}

/// `text` wrapped to `width` columns.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split(' ') {
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

fn catalogue() -> Reply {
    let entries = suggest::catalogue();
    let name_width = entries.iter().map(|e| e.suggestion.chars().count()).max().unwrap_or(0);
    let signal_width = entries.iter().map(|e| e.signal.word().len()).max().unwrap_or(0);
    let indent = name_width + 2 + signal_width + 2;
    let room = WIDTH.saturating_sub(2 + indent).max(20);
    let mut lines = Vec::new();
    for entry in &entries {
        let wrapped = wrap(&entry.looks_for, room);
        for (i, part) in wrapped.iter().enumerate() {
            if i == 0 {
                lines.push(format!(
                    "{:name_width$}  {:signal_width$}  {part}",
                    entry.suggestion,
                    entry.signal.word()
                ));
            } else {
                lines.push(format!("{:indent$}{part}", ""));
            }
        }
    }
    let named: BTreeSet<&str> = entries.iter().map(|e| e.suggestion.as_str()).collect();
    let screen = Screen::new()
        .with(Block::Section {
            heading: "Catalogue".to_string(),
            lines,
        })
        .with(Block::summary(
            Outcome::Success,
            count(entries.len(), "signal", "signals"),
            [Fact::plain(count(named.len(), "type or operation", "types and operations"))],
        ));
    let data = Json::object([(
        "signals",
        Json::array(entries.iter().map(|entry| {
            Json::object([
                ("suggestion", Json::from(entry.suggestion.as_str())),
                ("signal", Json::from(entry.signal.word())),
                ("looksFor", Json::from(entry.looks_for.as_str())),
                ("values", Json::array(entry.values.iter().map(|v| Json::from(v.as_str())))),
            ])
        })),
    )]);
    Reply::screen(screen, data)
}
