//! Every generated record and how it stands (area 18, "What is committed,
//! and why"): `jadpo check` regenerates each view and fails while one
//! disagrees with the source (`record.out_of_date`), reads each ledger and
//! fails while one isn't exactly what Jadpo writes (`record.edited`), and
//! refuses a record a newer Jadpo wrote (`record.format_newer`). `jadpo
//! records` lists the same standings, and `jadpo records update` applies
//! each one's fix.
//!
//! A view is compared byte for byte with what its writer gives today. A
//! difference is a hand edit when the file isn't one Jadpo could have
//! written (a comment, a fact outside the kind's list, a layout Jadpo
//! doesn't write); otherwise the record is out of date, the source having
//! moved on. Telling a careful hand edit of a fact from a stale record
//! needs the base commit's build, which comes with `jadpo approval`.
//!
//! A ledger is parsed and compared with its own canonical rewrite: sorted,
//! one line per entry, each key once, so a `union` merge that left lines
//! out of order is put right by `jadpo records update`, and two lines for
//! one key are left for a person.
//!
//! New kinds join here: 6.1c's event records as a view beside the API
//! record ([`crate::events::record`]), 7.1's package surfaces as views
//! and the lock as a ledger.

use std::collections::BTreeMap;

use jadpo_db::record::{self as format, Change, HeaderProblem, Kind, Shape};
use jadpo_db::{Db, Project};
use jadpo_diag::{Diagnostic, Span, rule};

use crate::routes::{record as api, routes_clean};

/// How one record stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum State {
    UpToDate,
    /// The source has something to record and there is no file.
    Missing,
    /// The source has moved on.
    OutOfDate,
    /// The file is under its name from before the record format.
    OldName,
    /// The file isn't one Jadpo writes.
    Edited,
    /// A newer Jadpo wrote it.
    FormatNewer,
    /// Not compared: the source it is generated from has errors.
    NotChecked,
}

impl State {
    /// The state as `jadpo records` says it.
    pub fn words(self) -> &'static str {
        match self {
            State::UpToDate => "up to date",
            State::Missing => "missing",
            State::OutOfDate => "out of date",
            State::OldName => "old name",
            State::Edited => "edited by hand",
            State::FormatNewer => "newer format",
            State::NotChecked => "not checked",
        }
    }

    /// The state as `--json` names it.
    pub fn id(self) -> &'static str {
        match self {
            State::UpToDate => "up_to_date",
            State::Missing => "missing",
            State::OutOfDate => "out_of_date",
            State::OldName => "old_name",
            State::Edited => "edited",
            State::FormatNewer => "format_newer",
            State::NotChecked => "not_checked",
        }
    }

    /// Whether the record needs its command.
    pub fn is_problem(self) -> bool {
        !matches!(self, State::UpToDate | State::NotChecked)
    }
}

/// What `jadpo records update` does to one file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fix {
    Write(String),
    Remove,
}

/// One record: where it is, its kind, how it stands, the problem `jadpo
/// check` reports for it and the files `jadpo records update` writes or
/// removes to bring it up to date.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Standing {
    pub path: String,
    pub kind: &'static str,
    pub state: State,
    pub problem: Option<Diagnostic>,
    pub fixes: Vec<(String, Fix)>,
}

impl Standing {
    pub fn kind(&self) -> &'static Kind {
        format::KINDS
            .iter()
            .find(|kind| kind.word == self.kind)
            .copied()
            .unwrap_or(&format::API)
    }

    /// The command that brings it up to date, when one does.
    pub fn command(&self) -> Option<&'static str> {
        if !self.state.is_problem() || self.state == State::FormatNewer {
            return None;
        }
        if self.state == State::Edited && self.fixes.is_empty() {
            return None;
        }
        Some(match self.kind().shape {
            Shape::View => self.kind().command,
            Shape::History => "jadpo records update",
        })
    }
}

/// Every record in the project and how it stands, by path.
#[salsa::tracked(returns(ref))]
pub fn standings(db: &dyn Db, project: Project) -> Vec<Standing> {
    if project.layout(db) != jadpo_db::Layout::Project {
        return Vec::new();
    }
    let written: BTreeMap<&str, &Result<String, String>> = project
        .records(db)
        .iter()
        .map(|record| (record.path.as_str(), &record.text))
        .collect();
    let mut out = Vec::new();
    views(db, project, &written, &mut out);
    event_views(db, project, &written, &mut out);
    for (path, text) in &written {
        if let Some((kind, from)) = format::kind_of(path)
            && kind.shape == Shape::History
        {
            out.push(history(path, kind, &from, text));
        }
    }
    if let Some(text) = written.get(jadpo_db::LEGACY_IDENTITIES) {
        out.push(legacy_identities(
            text,
            written.contains_key(jadpo_db::IDENTITIES),
        ));
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

/// `record.out_of_date`, `record.edited` and `record.format_newer`: each
/// record's problem.
#[salsa::tracked(returns(ref))]
pub fn record_problems(db: &dyn Db, project: Project) -> Vec<Diagnostic> {
    standings(db, project)
        .iter()
        .filter_map(|standing| standing.problem.clone())
        .collect()
}

/// The API records: one per API with routes, compared with what `jadpo api
/// update` writes; a record of an API with no routes, and one under its
/// old name, `api/<api>.api`.
fn views(
    db: &dyn Db,
    project: Project,
    written: &BTreeMap<&str, &Result<String, String>>,
    out: &mut Vec<Standing>,
) {
    let clean = routes_clean(db, project);
    let expected = api::record_texts(db, project);
    let legacy: BTreeMap<String, &str> = written
        .keys()
        .filter_map(|path| {
            let now = format::renamed(path)?;
            format::kind_of(&now)
                .is_some_and(|(kind, _)| kind.word == format::API.word)
                .then_some((now, *path))
        })
        .collect();
    for (api_name, text) in expected {
        let path = api::path(api_name);
        let mut fixes = vec![(path.clone(), Fix::Write(text.clone()))];
        if let Some(old) = legacy.get(&path) {
            fixes.push((old.to_string(), Fix::Remove));
        }
        let standing = |state, problem| Standing {
            path: match (state, legacy.get(&path)) {
                (State::OldName, Some(old)) => old.to_string(),
                _ => path.clone(),
            },
            kind: format::API.word,
            state,
            problem,
            fixes: if state == State::UpToDate {
                Vec::new()
            } else {
                fixes.clone()
            },
        };
        match written.get(path.as_str()) {
            None => {
                let problem = match legacy.get(&path) {
                    Some(old) => old_name(old, &path, "API records", &format::API),
                    None => Diagnostic::new(&rule::RECORD_OUT_OF_DATE, format!("`{path}` is missing"))
                        .at(file(&path), "no record")
                        .reason(format!(
                            "Each API with routes has a committed record of its routes, fields and failure codes, so every change to API `{api_name}` shows as a diff of one file."
                        ))
                        .next("Run `jadpo api update`, and commit the record with the code"),
                };
                let state = if legacy.contains_key(&path) {
                    State::OldName
                } else {
                    State::Missing
                };
                out.push(standing(state, Some(problem)));
            }
            Some(Err(error)) => out.push(standing(
                State::OutOfDate,
                Some(unreadable(&path, error, &format::API)),
            )),
            Some(Ok(written)) => {
                let (state, problem) = compare(
                    &path,
                    written,
                    text,
                    &format::API,
                    &api::from(api_name),
                    |changes| {
                        format!(
                            "The record is a promise to API `{api_name}`'s clients, and the routes now say otherwise: {changes}."
                        )
                    },
                );
                out.push(standing(state, problem));
                if let Some(old) = legacy.get(&path) {
                    out.push(Standing {
                        path: old.to_string(),
                        kind: format::API.word,
                        state: State::OldName,
                        problem: Some(old_name(old, &path, "API records", &format::API)),
                        fixes: vec![(old.to_string(), Fix::Remove)],
                    });
                }
            }
        }
    }
    // Records no API needs, judged only once the routes read cleanly.
    for path in written.keys() {
        if !format::kind_of(path).is_some_and(|(kind, _)| kind.word == format::API.word) {
            continue;
        }
        let api_name = path
            .trim_start_matches("api/")
            .trim_end_matches(format::EXTENSION);
        if expected.contains_key(api_name) {
            continue;
        }
        let (state, problem) = if !clean {
            (State::NotChecked, None)
        } else {
            (
                State::OutOfDate,
                Some(
                    Diagnostic::new(&rule::RECORD_OUT_OF_DATE, format!("`{path}` records no routes"))
                        .at(file(path), "nothing to record")
                        .reason(format!(
                            "No route belongs to API `{api_name}` any more, so its record promises routes that are gone."
                        ))
                        .next("Run `jadpo api update`, which removes it"),
                ),
            )
        };
        out.push(Standing {
            path: path.to_string(),
            kind: format::API.word,
            state,
            problem,
            fixes: vec![(path.to_string(), Fix::Remove)],
        });
    }
    for (now, old) in &legacy {
        let api_name = now
            .trim_start_matches("api/")
            .trim_end_matches(format::EXTENSION);
        if expected.contains_key(api_name) {
            continue;
        }
        out.push(Standing {
            path: old.to_string(),
            kind: format::API.word,
            state: if clean {
                State::OldName
            } else {
                State::NotChecked
            },
            problem: clean.then(|| old_name(old, now, "API records", &format::API)),
            fixes: vec![(old.to_string(), Fix::Remove)],
        });
    }
}

/// The event records: one per domain that declares an event, compared
/// with what `jadpo records update` writes, and a record of a domain that
/// declares none (chunk 6.1c).
fn event_views(
    db: &dyn Db,
    project: Project,
    written: &BTreeMap<&str, &Result<String, String>>,
    out: &mut Vec<Standing>,
) {
    use crate::events::record as events;
    let kind = &format::EVENTS;
    let clean = events::events_clean(db, project);
    let expected = events::record_texts(db, project);
    for (domain, text) in expected {
        let path = events::path(domain);
        let fixes = vec![(path.clone(), Fix::Write(text.clone()))];
        let standing = |state, problem: Option<Diagnostic>| Standing {
            path: path.clone(),
            kind: kind.word,
            state,
            problem,
            fixes: if state == State::UpToDate {
                Vec::new()
            } else {
                fixes.clone()
            },
        };
        match written.get(path.as_str()) {
            None => out.push(standing(
                State::Missing,
                Some(
                    Diagnostic::new(&rule::RECORD_OUT_OF_DATE, format!("`{path}` is missing"))
                        .at(file(&path), "no record")
                        .reason(format!(
                            "Each domain with events has a committed record of what each event carries, so every change to the events of domain `{domain}` shows as a diff of one file."
                        ))
                        .next("Run `jadpo records update`, and commit the record with the code"),
                ),
            )),
            Some(Err(error)) => out.push(standing(
                State::OutOfDate,
                Some(unreadable(&path, error, kind)),
            )),
            Some(Ok(written)) => {
                let (state, problem) = compare(
                    &path,
                    written,
                    text,
                    kind,
                    &events::from(domain),
                    |changes| {
                        format!(
                            "The record is a promise to whatever subscribes to the events of domain `{domain}`, and to the events already waiting in the outbox, and the events now say otherwise: {changes}."
                        )
                    },
                );
                out.push(standing(state, problem));
            }
        }
    }
    // Records of domains that declare no event, judged only once the
    // events read cleanly.
    for path in written.keys() {
        let Some(domain) = events::domain_of(path) else {
            continue;
        };
        if expected.contains_key(domain) {
            continue;
        }
        let (state, problem) = if !clean {
            (State::NotChecked, None)
        } else {
            (
                State::OutOfDate,
                Some(
                    Diagnostic::new(&rule::RECORD_OUT_OF_DATE, format!("`{path}` records no events"))
                        .at(file(path), "nothing to record")
                        .reason(format!(
                            "Domain `{domain}` declares no event any more, so its record promises events that are gone."
                        ))
                        .next("Run `jadpo records update`, which removes it"),
                ),
            )
        };
        out.push(Standing {
            path: path.to_string(),
            kind: kind.word,
            state,
            problem,
            fixes: vec![(path.to_string(), Fix::Remove)],
        });
    }
}

/// How a view's file stands against what its writer gives today.
fn compare(
    path: &str,
    written: &str,
    expected: &str,
    kind: &Kind,
    from: &str,
    reason: impl Fn(&str) -> String,
) -> (State, Option<Diagnostic>) {
    let written = format::normalise(written);
    if written == expected {
        return (State::UpToDate, None);
    }
    if let Err(problem) = format::read_header(&written, kind, from) {
        return header_problem(path, &written, kind, &problem);
    }
    if let Err((line, why)) = format::canonical(&written, kind, from) {
        return (
            State::Edited,
            Some(edited(path, &written, kind, line, &why, true)),
        );
    }
    let before = format::read(&written, kind).unwrap_or_default();
    let after = format::read(expected, kind).unwrap_or_default();
    let found = format::changes(&before, &after);
    let said: Vec<String> = found
        .iter()
        .map(|(change, heading)| format!("{} `{heading}`", change.word()))
        .collect();
    let (at, label) = first_change(&written, &before, &after, &found);
    let changes = if said.is_empty() {
        "its text differs".to_string()
    } else {
        crate::and_list(&said)
    };
    (
        State::OutOfDate,
        Some(
            Diagnostic::new(
                &rule::RECORD_OUT_OF_DATE,
                format!("`{path}` is out of date"),
            )
            .at(line_span(path, &written, at), label)
            .reason(reason(&changes))
            .next(format!(
                "Run `{}`, and commit the record with the code",
                kind.command
            )),
        ),
    )
}

/// Where a stale view first differs, and what to say there: the heading
/// of the first entry changed or removed, else the entry an added one
/// goes before (or the last entry, when it goes at the end).
fn first_change(
    written: &str,
    before: &[format::Entry],
    after: &[format::Entry],
    found: &[(Change, String)],
) -> (usize, String) {
    let line_of = |heading: &str| {
        written
            .lines()
            .position(|line| line == heading)
            .unwrap_or(0)
    };
    let gone: Vec<(usize, Change)> = found
        .iter()
        .filter(|(change, _)| *change != Change::Added)
        .map(|(change, heading)| (line_of(heading), *change))
        .collect();
    if let Some((at, change)) = gone.into_iter().min() {
        let label = match change {
            Change::Removed => "the source no longer has this",
            _ => "the source says otherwise",
        };
        return (at, label.to_string());
    }
    let Some((_, added)) = found.first() else {
        return (0, "the source says otherwise".to_string());
    };
    let next = after
        .iter()
        .skip_while(|entry| entry.heading != *added)
        .find(|entry| before.iter().any(|old| old.heading == entry.heading));
    match next {
        Some(entry) => (
            line_of(&entry.heading),
            format!("`{added}` goes before this"),
        ),
        None => (
            before.last().map_or(0, |entry| line_of(&entry.heading)),
            format!("`{added}` goes after this"),
        ),
    }
}

/// The standing of a record whose header isn't the one this Jadpo writes.
fn header_problem(
    path: &str,
    text: &str,
    kind: &Kind,
    problem: &HeaderProblem,
) -> (State, Option<Diagnostic>) {
    match problem {
        HeaderProblem::Newer(found) => (
            State::FormatNewer,
            Some(
                Diagnostic::new(
                    &rule::RECORD_FORMAT_NEWER,
                    format!("`{path}` was written by a newer Jadpo"),
                )
                .at(line_span(path, text, 1), "newer format")
                .reason(format!(
                    "It is in format {found} of the `{}` record, and this Jadpo reads format {} and older. Reading it anyway could lose what it holds.",
                    kind.word, kind.format
                ))
                .next("Upgrade Jadpo to the version the project uses, then run the command again"),
            ),
        ),
        HeaderProblem::Older(found) => (
            State::OutOfDate,
            Some(
                Diagnostic::new(&rule::RECORD_OUT_OF_DATE, format!("`{path}` is out of date"))
                    .at(line_span(path, text, 1), "older format")
                    .reason(format!(
                        "It is in format {found} of the `{}` record, and this Jadpo writes format {}.",
                        kind.word, kind.format
                    ))
                    .next(format!(
                        "Run `{}`, and commit the record with the upgrade",
                        kind.command
                    )),
            ),
        ),
        other => (
            State::Edited,
            Some(edited(
                path,
                text,
                kind,
                other.line(),
                &other.describe(kind),
                true,
            )),
        ),
    }
}

/// `record.edited` at `line` (from 0): the file isn't one Jadpo writes.
/// `rewritable` when its command writes it again from what it holds.
fn edited(
    path: &str,
    text: &str,
    kind: &Kind,
    line: usize,
    why: &str,
    rewritable: bool,
) -> Diagnostic {
    let command = match kind.shape {
        Shape::View => kind.command,
        Shape::History => "jadpo records update",
    };
    let next = if rewritable {
        format!("Run `{command}`, which writes it again")
    } else {
        format!(
            "Put line {} back as Jadpo wrote it, with `git restore {path}` or by hand",
            line + 1
        )
    };
    Diagnostic::new(
        &rule::RECORD_EDITED,
        format!("`{path}` was edited by hand: {why}"),
    )
    .at(line_span(path, text, line), "not what Jadpo writes")
    .reason(format!(
        "Jadpo writes this record with {}, and a record is never edited by hand, so its diff always means what the source or a command changed.",
        kind.update
    ))
    .next(next)
}

/// `record.out_of_date` for a record still under its name from before
/// the record format.
fn old_name(old: &str, now: &str, what: &str, kind: &Kind) -> Diagnostic {
    let command = match kind.shape {
        Shape::View => kind.command,
        Shape::History => "jadpo records update",
    };
    Diagnostic::new(
        &rule::RECORD_OUT_OF_DATE,
        format!("`{old}` is the record's old name"),
    )
    .at(file(old), "old name")
    .reason(format!(
        "{what} are now written in the record format, as `{now}`, so they read alike and say what writes them."
    ))
    .next(format!("Run `{command}`, which writes `{now}` and removes `{old}`"))
}

fn unreadable(path: &str, error: &str, kind: &Kind) -> Diagnostic {
    Diagnostic::new(&rule::RECORD_OUT_OF_DATE, format!("`{path}` can't be read"))
        .at(file(path), "unreadable")
        .reason(format!("Reading it failed: {error}."))
        .next(format!("Run `{}` to write it again", kind.command))
}

/// A ledger's standing: its header, then each line, then whether it is in
/// the order and layout Jadpo writes.
fn history(path: &str, kind: &'static Kind, from: &str, text: &Result<String, String>) -> Standing {
    let standing = |state, problem, fixes| Standing {
        path: path.to_string(),
        kind: kind.word,
        state,
        problem,
        fixes,
    };
    let text = match text {
        Ok(text) => format::normalise(text).into_owned(),
        Err(error) => {
            return standing(
                State::Edited,
                Some(
                    Diagnostic::new(&rule::RECORD_EDITED, format!("`{path}` can't be read"))
                        .at(file(path), "unreadable")
                        .reason(format!("Reading it failed: {error}."))
                        .next(format!("Restore it with `git restore {path}`")),
                ),
                Vec::new(),
            );
        }
    };
    let found = match format::read_header(&text, kind, from) {
        Ok(found) => found,
        Err(problem) => {
            let (state, diagnostic) = header_problem(path, &text, kind, &problem);
            return standing(state, diagnostic, Vec::new());
        }
    };
    let rewritten = match rewrite(kind, &text) {
        Some(Ok(rewritten)) => rewritten,
        Some(Err((line, why))) => {
            return standing(
                State::Edited,
                Some(edited(path, &text, kind, line, &why, false)),
                Vec::new(),
            );
        }
        // Its own check reads it (`translation.sources_invalid`).
        None => return standing(State::NotChecked, None, Vec::new()),
    };
    if rewritten == text {
        return standing(State::UpToDate, None, Vec::new());
    }
    let fixes = vec![(path.to_string(), Fix::Write(rewritten.clone()))];
    // A ledger in an older format is history this Jadpo still reads; `jadpo
    // records update` rewrites it in the newest.
    if found < kind.format {
        return standing(State::UpToDate, None, fixes);
    }
    let (line, why) = match format::canonical(&text, kind, from) {
        Err(problem) => problem,
        Ok(_) => (
            rewritten
                .lines()
                .zip(text.lines())
                .position(|(a, b)| a != b)
                .unwrap_or_else(|| rewritten.lines().count().min(text.lines().count())),
            "its lines aren't in the order Jadpo writes them".to_string(),
        ),
    };
    standing(
        State::Edited,
        Some(edited(path, &text, kind, line, &why, true)),
        fixes,
    )
}

/// A ledger rewritten as Jadpo writes it from what it holds, or the line
/// that can't be read; `None` for a kind whose own check reads it.
fn rewrite(kind: &Kind, text: &str) -> Option<Result<String, (usize, String)>> {
    use crate::suggest::{record as ignored, taught};
    Some(match kind.word {
        "identities" => {
            jadpo_db::parse_identities(text).map(|lines| jadpo_db::write_identities(&lines))
        }
        "lock" => jadpo_db::parse_lock(text).map(|packages| jadpo_db::write_lock(&packages)),
        "ignored_suggestions" => ignored::parse(text).map(|entries| ignored::render(&entries)),
        "taught_suggestions" => taught::parse(text).map(|entries| taught::render(&entries)),
        "translations" => match crate::translations::sources::parse(text) {
            Ok(entries) => Ok(crate::translations::sources::render(&entries)),
            Err(_) => return None,
        },
        _ => return None,
    })
}

/// `database/identities.json`, the registry before the record format: out
/// of date until `jadpo records update` rewrites it, or left over beside
/// the new file.
fn legacy_identities(text: &Result<String, String>, has_new: bool) -> Standing {
    let old = jadpo_db::LEGACY_IDENTITIES;
    let now = jadpo_db::IDENTITIES;
    let mut fixes = Vec::new();
    if !has_new
        && let Ok(text) = text
        && let Ok(lines) = jadpo_db::parse_legacy_identities(text)
    {
        fixes.push((
            now.to_string(),
            Fix::Write(jadpo_db::write_identities(&lines)),
        ));
    }
    fixes.push((old.to_string(), Fix::Remove));
    Standing {
        path: old.to_string(),
        kind: format::IDENTITIES.word,
        state: State::OldName,
        problem: Some(old_name(
            old,
            now,
            "The identity registry's renames",
            &format::IDENTITIES,
        )),
        fixes,
    }
}

fn file(path: &str) -> Span {
    crate::tree::file_span(path)
}

/// The span of line `line` (from 0) of `text`.
fn line_span(path: &str, text: &str, line: usize) -> Span {
    let start: usize = text.lines().take(line).map(|l| l.len() + 1).sum();
    let end = start + text.lines().nth(line).map_or(0, str::len);
    Span::new(path, start as u32, end.max(start + 1) as u32)
}

/// Whether two record texts differ only by what `changes` lists; used by
/// the review of a record diff.
pub fn entry_changes(kind: &Kind, before: &str, after: &str) -> Vec<(Change, String)> {
    let before = format::read(&format::normalise(before), kind).unwrap_or_default();
    let after = format::read(&format::normalise(after), kind).unwrap_or_default();
    format::changes(&before, &after)
}
