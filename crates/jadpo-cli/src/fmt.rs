//! `jadpo fmt`: formats every `.jadpo` file in the project, or in the file
//! or folder named, and with `--check` reports instead of writing. The
//! project is found by walking up to `app.jadpo` (area 14). Paths in output
//! are relative to the project, or to the folder named when there is no
//! project around it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use jadpo_diag::{Decider, Diagnostic, Span, rule};
use jadpo_term::{Block, Fact, Json, Outcome, Screen};

use crate::run::{Call, Reply};

/// What happened to one file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Status {
    /// Rewritten in the canonical layout.
    Formatted,
    Unchanged,
    /// Differs from the canonical layout (`--check`).
    Unformatted,
    SyntaxErrors,
    /// The formatter couldn't format it safely (`format.unsafe`).
    Refused,
    Unreadable,
    Unwritable,
}

impl Status {
    fn word(self) -> &'static str {
        match self {
            Status::Formatted => "formatted",
            Status::Unchanged => "unchanged",
            Status::Unformatted => "unformatted",
            Status::SyntaxErrors => "syntaxErrors",
            Status::Refused => "refused",
            Status::Unreadable => "unreadable",
            Status::Unwritable => "unwritable",
        }
    }
}

pub fn fmt(call: &Call) -> Reply {
    let check = call.flag("--check");
    let cwd = call.cwd.clone().unwrap_or_else(|| PathBuf::from("."));
    let (root, files) = match call.arguments.first() {
        Some(argument) => match named(&cwd, argument) {
            Ok(found) => found,
            Err(diagnostic) => return Reply::failed(*diagnostic),
        },
        None => match jadpo_db::find_root(&cwd) {
            Some(root) => {
                let files = jadpo_files(&root);
                (root, files)
            }
            None => return project_not_found("fmt"),
        },
    };
    let mut diagnostics = Vec::new();
    let mut sources = BTreeMap::new();
    let mut results = Vec::new();
    for file in files {
        let shown = display(&root, &file);
        let status = format_one(&file, &shown, check, &mut diagnostics, &mut sources);
        results.push((shown, status));
    }
    report(check, results, diagnostics, sources)
}

/// The root paths are shown from, and the files to format, for a named
/// file or folder.
fn named(cwd: &Path, argument: &str) -> Result<(PathBuf, Vec<PathBuf>), Box<Diagnostic>> {
    let path = cwd.join(argument);
    if path.is_dir() {
        let root = jadpo_db::find_root(&path).unwrap_or_else(|| path.clone());
        let files = jadpo_files(&path);
        return Ok((root, files));
    }
    if !path.exists() {
        return Err(Box::new(
            Diagnostic::new(
                &rule::CLI_FMT_ARGUMENTS,
                format!("`{argument}` doesn't exist"),
            )
            .reason("There is no file or folder at this path.")
            .next("Give the path of a `.jadpo` file or a folder"),
        ));
    }
    if path.extension().and_then(|e| e.to_str()) != Some("jadpo") {
        return Err(Box::new(
            Diagnostic::new(
                &rule::CLI_FMT_ARGUMENTS,
                format!("`{argument}` isn't a Jadpo file"),
            )
            .reason("`jadpo fmt` formats files whose names end in `.jadpo`.")
            .next("Give a `.jadpo` file, or a folder to format every one in it"),
        ));
    }
    let folder = path
        .parent()
        .map_or_else(|| cwd.to_path_buf(), Path::to_path_buf);
    let root = jadpo_db::find_root(&folder).unwrap_or(folder);
    Ok((root, vec![path]))
}

/// Every `.jadpo` file under `folder`, in path order, leaving out hidden
/// folders and the generated `build/`.
fn jadpo_files(folder: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![folder.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with('.') {
                continue;
            }
            let path = entry.path();
            match entry.file_type() {
                Ok(kind) if kind.is_dir() => {
                    if name != "build" {
                        pending.push(path);
                    }
                }
                Ok(_) if name.ends_with(".jadpo") => found.push(path),
                _ => {}
            }
        }
    }
    found.sort();
    found
}

/// A path as the reader sees it: relative to the root, with `/`.
fn display(root: &Path, file: &Path) -> String {
    let relative = file.strip_prefix(root).unwrap_or(file);
    relative
        .components()
        .map(|part| part.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

fn format_one(
    file: &Path,
    shown: &str,
    check: bool,
    diagnostics: &mut Vec<Diagnostic>,
    sources: &mut BTreeMap<String, String>,
) -> Status {
    // A pipe or a device named `.jadpo` would block or never end.
    let regular = std::fs::metadata(file)
        .map_err(|error| error.to_string())
        .and_then(|meta| {
            if meta.is_file() {
                Ok(())
            } else {
                Err("it isn't a regular file".to_string())
            }
        });
    let text = match regular.and_then(|()| std::fs::read_to_string(file).map_err(|e| e.to_string()))
    {
        Ok(text) => text,
        Err(error) => {
            diagnostics.push(
                Diagnostic::new(
                    &rule::TOOLCHAIN_SOURCE_READ_FAILED,
                    format!("`{shown}` couldn't be read"),
                )
                .at(Span::new(shown, 0, 0), "")
                .reason(format!("Reading it failed: {error}."))
                .next("Check the file can be read, then run `jadpo fmt` again"),
            );
            return Status::Unreadable;
        }
    };
    sources.insert(shown.to_string(), text.clone());
    let formatted = match jadpo_fmt::format_file(&text) {
        Ok(formatted) => formatted,
        Err(errors) => {
            let refused = errors
                .iter()
                .any(|error| error.rule.id == jadpo_diag::rule::FORMAT_UNSAFE.id);
            diagnostics.extend(errors.iter().map(|error| error.to_diagnostic(shown)));
            return if refused {
                Status::Refused
            } else {
                Status::SyntaxErrors
            };
        }
    };
    if formatted == text {
        return Status::Unchanged;
    }
    if check {
        diagnostics.push(unformatted(shown, &text, &formatted));
        return Status::Unformatted;
    }
    match std::fs::write(file, &formatted) {
        Ok(()) => Status::Formatted,
        Err(error) => {
            diagnostics.push(
                Diagnostic::new(
                    &rule::FORMAT_WRITE_FAILED,
                    format!("`{shown}` couldn't be written"),
                )
                .at(Span::new(shown, 0, 0), "")
                .reason(format!("Writing the formatted file failed: {error}."))
                .next("Check the file can be written, then run `jadpo fmt` again"),
            );
            Status::Unwritable
        }
    }
}

/// `format.changes_required`, pointing at the first character the
/// formatter would change, with the whole formatted file as the edit.
fn unformatted(shown: &str, text: &str, formatted: &str) -> Diagnostic {
    let mut at = text
        .bytes()
        .zip(formatted.bytes())
        .position(|(a, b)| a != b)
        .unwrap_or_else(|| text.len().min(formatted.len()));
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    // Point at the next thing written on the line rather than at a space.
    let rest = &text[at..];
    let skip = rest.len() - rest.trim_start_matches([' ', '\t']).len();
    if text[at + skip..].starts_with(|c: char| c != '\n' && c != '\r') {
        at += skip;
    }
    let width = text[at..].chars().next().map_or(0, char::len_utf8);
    Diagnostic::new(&rule::FORMAT_CHANGES_REQUIRED, "This file isn't formatted")
        .at(
            Span::new(shown, at as u32, (at + width) as u32),
            "the formatter changes this line",
        )
        .reason("`jadpo fmt` lays this file out differently from here on.")
        .next("Run `jadpo fmt` to format it")
        .edit(Span::new(shown, 0, text.len() as u32), formatted)
}

/// No `app.jadpo` at or above the folder a command runs in.
pub(crate) fn project_not_found(command: &str) -> Reply {
    let diagnostic = Diagnostic::new(&rule::TOOLCHAIN_PROJECT_NOT_FOUND, "No Jadpo project here")
        .reason(
            "Jadpo looks for `app.jadpo` in this folder and every folder above it, and found none.",
        )
        .next_for(
            format!(
                "Run `jadpo {command}` inside a project, or give a file or folder: `jadpo {command} <path>`"
            ),
            Decider::Agent,
        );
    let screen = Screen::new().with(Block::diagnostic(&diagnostic, &()));
    Reply::Report {
        screen,
        data: Json::Null,
        diagnostics: vec![diagnostic],
        sources: BTreeMap::new(),
    }
}

fn count(n: usize, what: &str) -> String {
    format!("{n} {what}")
}

fn report(
    check: bool,
    results: Vec<(String, Status)>,
    diagnostics: Vec<Diagnostic>,
    sources: BTreeMap<String, String>,
) -> Reply {
    let tally = |status: Status| results.iter().filter(|(_, s)| *s == status).count();
    let problems = [
        (Status::SyntaxErrors, "with syntax errors"),
        (Status::Refused, "refused by the formatter"),
        (Status::Unreadable, "couldn't be read"),
        (Status::Unwritable, "couldn't be written"),
        (Status::Unformatted, "not formatted"),
    ];
    let mut facts: Vec<Fact> = problems
        .iter()
        .filter(|(status, _)| tally(*status) > 0)
        .map(|(status, what)| Fact::problem(count(tally(*status), what)))
        .collect();
    let failed = !facts.is_empty();
    let changed = tally(Status::Formatted);
    let unchanged = tally(Status::Unchanged);
    if changed > 0 {
        facts.push(Fact::plain(count(changed, "changed")));
    }
    let (outcome, headline) = if results.is_empty() {
        facts.push(Fact::plain("no .jadpo files"));
        (Outcome::Success, "nothing to format")
    } else if failed {
        if unchanged > 0 {
            let what = if check { "formatted" } else { "unchanged" };
            facts.push(Fact::plain(count(unchanged, what)));
        }
        (Outcome::Failure, "fmt failed")
    } else {
        if check {
            let files = if results.len() == 1 { "file" } else { "files" };
            facts.push(Fact::plain(count(results.len(), files)));
        } else if unchanged > 0 {
            facts.push(Fact::plain(count(unchanged, "unchanged")));
        }
        (Outcome::Success, "formatted")
    };
    let mut screen = Screen::new();
    for diagnostic in &diagnostics {
        screen.push(Block::diagnostic(diagnostic, &sources));
    }
    screen.push(Block::summary(outcome, headline, facts));
    let data = Json::object([
        ("check", check.into()),
        (
            "files",
            Json::array(results.iter().map(|(path, status)| {
                Json::object([("path", Json::from(path)), ("status", status.word().into())])
            })),
        ),
    ]);
    Reply::Report {
        screen,
        data,
        diagnostics,
        sources,
    }
}
