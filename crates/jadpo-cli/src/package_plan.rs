//! `jadpo package update <name> --plan` (area 14, "`jadpo package update`
//! and `remove`"; area 18, "Package upgrades"; chunk R9): the release
//! `update` would take, and what taking it would do to the app, without
//! changing anything.
//!
//! The candidate is fetched and verified as `update` does it, then held in
//! a scratch copy of the project's files, in memory: the package's folder
//! replaced by the candidate's files and its lock line by the one `update`
//! would write, so the app is compiled against it as "Two versions side by
//! side" (area 11) describes, and nothing on disk moves. Each change
//! record entry is followed through the app's graph as it stands, by the
//! walk `jadpo impact` takes ([`jadpo_graph::impact`]), to the actions,
//! routes, jobs and stored fields it reaches. Comparing the two builds
//! gives the errors `jadpo check` would show that it doesn't now, and the
//! schema changes the next migration would hold; the review under `Needs a
//! person` is `update`'s, read from the scratch copy.
//!
//! ```text
//! ✓ Planned Postcodes.v2 from 2.0.0 to 2.1.0, nothing changed
//!     folder   packages/postcodes/v2
//!     source   path ../postcodes
//!     surface  matches its code
//!
//! Change record, 2.0.0 to 2.1.0
//!   ~ retyped  type UkPostcode  min_length: 6, was min_length: 5
//!     actions  save_address, tidy_all
//!     routes   POST /addresses
//!     jobs     schedule nightly_tidy
//!     stored   Address.postcode
//!
//! Migration that would follow
//!   ~ field `Address.postcode` changed  needs a decision: refit
//!
//! ✓ upgrade planned  1 reaches the app · 1 schema change
//!
//! → jadpo package update postcodes  takes 2.1.0
//! ```

use std::collections::BTreeSet;
use std::path::Path;

use jadpo_check::acceptances::upgrades;
use jadpo_db::{FileSet, Locked, Source};
use jadpo_diag::Diagnostic;
use jadpo_graph::impact;
use jadpo_graph::migration::{Needed, NeededKind, decisions_needed};
use jadpo_graph::schema::{Change as SchemaChange, Schema};
use jadpo_graph::{Graph, Kind, Node, NodeId};
use jadpo_term::{Block, ChangeMark, Fact, Json, Outcome, Position, Reach, Screen};

use crate::install::{Verified, folder_name, review_side, source_words, version_of, with_line};
use crate::registry;
use crate::run::Reply;

/// One change record entry, from the registry or worked out here.
struct Entry {
    declaration: String,
    kind: String,
    /// As the registry's protocol names it: `retyped`.
    change: &'static str,
    detail: Option<String>,
    why: Option<String>,
}

/// What an entry reaches in the app's own code, by name.
#[derive(Default)]
struct Reached {
    actions: BTreeSet<String>,
    routes: Vec<String>,
    jobs: Vec<String>,
    stored: BTreeSet<String>,
}

impl Reached {
    fn is_empty(&self) -> bool {
        self.actions.is_empty()
            && self.routes.is_empty()
            && self.jobs.is_empty()
            && self.stored.is_empty()
    }

    fn rows(&self) -> Vec<(String, String)> {
        if self.is_empty() {
            return vec![("reaches".to_string(), "nothing in the app".to_string())];
        }
        let mut rows = Vec::new();
        let joined =
            |names: Vec<&String>| names.into_iter().cloned().collect::<Vec<_>>().join(", ");
        for (label, names) in [
            ("actions", self.actions.iter().collect::<Vec<_>>()),
            ("routes", self.routes.iter().collect()),
            ("jobs", self.jobs.iter().collect()),
            ("stored", self.stored.iter().collect()),
        ] {
            if !names.is_empty() {
                rows.push((label.to_string(), joined(names)));
            }
        }
        rows
    }

    fn json(&self) -> Json {
        let list =
            |names: Vec<&String>| Json::array(names.into_iter().map(|n| Json::from(n.as_str())));
        Json::object([
            ("actions", list(self.actions.iter().collect())),
            ("routes", list(self.routes.iter().collect())),
            ("jobs", list(self.jobs.iter().collect())),
            ("stored", list(self.stored.iter().collect())),
        ])
    }
}

/// An error the app would have against the candidate that it doesn't have
/// now.
struct Error {
    rule: &'static str,
    message: String,
    path: Option<String>,
    line: Option<usize>,
}

/// A change to the schema the next migration would hold.
struct Migration {
    mark: ChangeMark,
    change: String,
    decision: Option<NeededKind>,
}

impl Error {
    /// Where it is, `path:line`.
    fn at(&self) -> String {
        match (&self.path, self.line) {
            (Some(path), Some(line)) => format!("{path}:{line}"),
            (Some(path), None) => path.clone(),
            _ => "the project".to_string(),
        }
    }
}

/// The plan for taking `verified` in place of `line`, from the project at
/// `root` as it stands; `before` is the installed copy's committed surface
/// and `notes` the registry's, for a package from there. `to` is the
/// release `--to` named, which the hint repeats.
#[allow(clippy::too_many_arguments)]
pub(crate) fn plan(
    root: &Path,
    to: Option<&str>,
    lines: &[Locked],
    line: &Locked,
    verified: &Verified,
    source: &Source,
    before: &str,
    notes: Option<&registry::Notes>,
) -> Reply {
    let folder = jadpo_db::package_folder(&line.package);
    let (_, files) = jadpo_db::read(root);
    let candidate = candidate_files(&files, &folder, lines, line, verified, source);
    let now = jadpo_db::load_files(root.to_path_buf(), &files);
    let then = jadpo_db::load_files(root.to_path_buf(), &candidate);

    // The change record and what each entry reaches, on the app as it is:
    // what the app uses is read on the before side (area 18).
    let record = match notes {
        Some(notes) => notes
            .record
            .iter()
            .map(|entry| Entry {
                declaration: entry.declaration.clone(),
                kind: entry.kind.clone(),
                change: registry_change(entry.change),
                detail: entry.detail.clone(),
                why: entry.why.clone(),
            })
            .collect(),
        None => worked_out(root, &folder, before, verified),
    };
    let graph = jadpo_check::graph::graph(&now.db, now.project);
    let through = jadpo_check::surface::homes(&now.db, now.project)
        .get(&folder)
        .map_or_else(|| line.package.clone(), |home| home.through.clone());
    let reached: Vec<Reached> = record
        .iter()
        .map(|entry| reach(graph, &folder, &through, entry))
        .collect();

    let errors = new_errors(&now, &then, &candidate);
    let migration = migration(
        jadpo_check::data::schema(&now.db, now.project),
        jadpo_check::data::schema(&then.db, then.project),
    );
    let review = upgrades::review(&review_side(root), &candidate_side(&candidate));

    let old_version = version_of(&line.source).unwrap_or("").to_string();
    let name = folder_name(&line.package);
    let mut screen = Screen::new().with(Block::Status {
        outcome: Outcome::Success,
        text: format!(
            "Planned {} from {old_version} to {}, nothing changed",
            line.package, verified.version
        ),
        rows: {
            let mut status = vec![
                ("folder".to_string(), folder.clone()),
                ("source".to_string(), source_words(source)),
            ];
            if let Some(notes) = notes {
                if !notes.released.is_empty() {
                    status.push(("released".to_string(), notes.released.clone()));
                }
                if let Some(urgent) = &notes.urgent {
                    status.push(("urgent".to_string(), urgent.clone()));
                }
            }
            status.push(("surface".to_string(), "matches its code".to_string()));
            status
        },
    });
    if notes.is_some()
        && let Some(facts) = &verified.facts
    {
        screen.push(crate::package_release::facts_block(facts));
    }
    if let Some((was, now)) = notes.and_then(|notes| notes.moved.as_ref()) {
        screen.push(Block::Changes {
            note: None,
            groups: vec![(
                format!("New publisher since {old_version}"),
                vec![(
                    ChangeMark::Changed,
                    vec!["publisher".to_string(), format!("{now}, was {was}")],
                )],
            )],
        });
    }
    if !record.is_empty() {
        screen.push(Block::Reaches {
            heading: format!("Change record, {old_version} to {}", verified.version),
            entries: record
                .iter()
                .zip(&reached)
                .map(|(entry, reached)| Reach {
                    mark: Some(entry_mark(entry.change)),
                    cells: entry_cells(entry),
                    rows: reached.rows(),
                })
                .collect(),
        });
    }
    let (records, errors): (Vec<Error>, Vec<Error>) = errors
        .into_iter()
        .partition(|error| error.rule.starts_with("record."));
    if !errors.is_empty() {
        screen.push(Block::Reaches {
            heading: "Would not compile".to_string(),
            entries: errors
                .iter()
                .map(|error| Reach {
                    mark: None,
                    cells: vec![error.message.clone()],
                    rows: vec![("at".to_string(), error.at())],
                })
                .collect(),
        });
    }
    let records: BTreeSet<String> = records.iter().filter_map(|e| e.path.clone()).collect();
    if !records.is_empty() {
        screen.push(Block::ChangeList {
            heading: "Records out of date".to_string(),
            rows: records
                .iter()
                .map(|path| (ChangeMark::Changed, vec![path.clone()]))
                .collect(),
        });
    }
    if !migration.is_empty() {
        screen.push(Block::ChangeList {
            heading: "Migration that would follow".to_string(),
            rows: migration
                .iter()
                .map(|change| {
                    let mut cells = vec![change.change.clone()];
                    if let Some(decision) = change.decision {
                        cells.push(decision_words(decision).to_string());
                    }
                    (change.mark, cells)
                })
                .collect(),
        });
    }
    if !review.is_empty() {
        screen.push(Block::ChangeList {
            heading: "Needs a person".to_string(),
            rows: review
                .iter()
                .map(|finding| {
                    let mark = match finding.mark() {
                        '+' => ChangeMark::Added,
                        '-' => ChangeMark::Removed,
                        _ => ChangeMark::Changed,
                    };
                    let mut columns = vec![finding.through.clone(), finding.what.clone()];
                    if !finding.detail.is_empty() {
                        columns.push(finding.detail.clone());
                    }
                    (mark, columns)
                })
                .collect(),
        });
    }
    if let Some(notes) = notes
        && !notes.waiting.is_empty()
    {
        screen.push(registry::waiting_block(&notes.waiting));
    }
    verified.alert(&mut screen);
    let reaching = reached.iter().filter(|r| !r.is_empty()).count();
    let mut facts = Vec::new();
    if !record.is_empty() {
        facts.push(Fact::plain(if reaching == 0 {
            "nothing reaches the app".to_string()
        } else {
            format!(
                "{reaching} {} the app",
                if reaching == 1 { "reaches" } else { "reach" }
            )
        }));
    }
    if !errors.is_empty() {
        facts.push(Fact::problem(crate::package::plural(
            errors.len(),
            "error",
            "errors",
        )));
    }
    if !migration.is_empty() {
        facts.push(Fact::plain(crate::package::plural(
            migration.len(),
            "schema change",
            "schema changes",
        )));
    }
    if !review.is_empty() {
        facts.push(Fact::problem(format!(
            "{} {}",
            review.len(),
            if review.len() == 1 {
                "needs a person"
            } else {
                "need a person"
            }
        )));
    }
    screen.push(Block::summary(Outcome::Success, "upgrade planned", facts));
    let mut command = format!("jadpo package update {name}");
    if to.is_some() {
        command.push_str(&format!(" --to {}", verified.version));
    }
    screen.push(Block::hint(command, format!("takes {}", verified.version)));

    let mut data = vec![
        ("package", Json::from(line.package.as_str())),
        ("from", Json::from(old_version.as_str())),
        ("version", Json::from(verified.version.as_str())),
        ("source", Json::from(source_words(source).as_str())),
        ("updated", false.into()),
        (
            "change_record",
            Json::array(record.iter().zip(&reached).map(|(entry, reached)| {
                Json::object([
                    ("declaration", Json::from(entry.declaration.as_str())),
                    ("kind", Json::from(entry.kind.as_str())),
                    ("change", Json::from(entry.change)),
                    (
                        "detail",
                        entry.detail.as_deref().map_or(Json::Null, Json::from),
                    ),
                    ("why", entry.why.as_deref().map_or(Json::Null, Json::from)),
                    ("reaches", reached.json()),
                ])
            })),
        ),
        (
            "errors",
            Json::array(errors.iter().map(|error| {
                Json::object([
                    ("rule", Json::from(error.rule)),
                    ("message", Json::from(error.message.as_str())),
                    ("path", error.path.as_deref().map_or(Json::Null, Json::from)),
                    (
                        "line",
                        error.line.map_or(Json::Null, |line| Json::Int(line as i64)),
                    ),
                ])
            })),
        ),
        (
            "records",
            Json::array(records.iter().map(|path| Json::from(path.as_str()))),
        ),
        (
            "migration",
            Json::array(migration.iter().map(|change| {
                Json::object([
                    ("change", Json::from(change.change.as_str())),
                    (
                        "decision",
                        change
                            .decision
                            .map_or(Json::Null, |kind| Json::from(decision_name(kind))),
                    ),
                ])
            })),
        ),
        (
            "review",
            Json::array(review.iter().map(|finding| {
                Json::object([
                    ("subject", Json::from(finding.subject.as_str())),
                    ("change", Json::from(finding.change.as_str())),
                    ("through", Json::from(finding.through.as_str())),
                    ("said", Json::from(finding.said.as_str())),
                ])
            })),
        ),
    ];
    if let Some(notes) = notes {
        data.push((
            "urgent",
            notes.urgent.as_deref().map_or(Json::Null, Json::from),
        ));
    }
    Reply::screen(screen, Json::object(data))
}

/// The project's files with the candidate in the package's folder and the
/// lock naming it, as `update` would leave them.
fn candidate_files(
    files: &FileSet,
    folder: &str,
    lines: &[Locked],
    line: &Locked,
    verified: &Verified,
    source: &Source,
) -> FileSet {
    let prefix = format!("{folder}/");
    let mut out = FileSet {
        files: files
            .files
            .iter()
            .filter(|(path, _)| !path.starts_with(&prefix))
            .map(|(path, text)| (path.clone(), text.clone()))
            .collect(),
    };
    for (path, bytes) in &verified.files {
        out.insert(
            format!("{prefix}{path}"),
            String::from_utf8(bytes.clone()).map_err(|_| "it isn't UTF-8 text".to_string()),
        );
    }
    let locked = Locked {
        package: line.package.clone(),
        source: source.clone(),
    };
    out.insert(
        jadpo_db::LOCK,
        Ok(jadpo_db::write_lock(&with_line(lines, locked))),
    );
    out
}

/// What an upgrade's review reads of the scratch copy.
fn candidate_side(files: &FileSet) -> upgrades::Side {
    let read: Vec<(&str, &str)> = files
        .files
        .iter()
        .filter(|(path, _)| upgrades::read_for_a_review(path))
        .filter_map(|(path, text)| Some((path.as_str(), text.as_ref().ok()?.as_str())))
        .collect();
    upgrades::Side::read(read)
}

/// A folder or git package's change record, worked out as the verifier
/// does from the two surfaces and the two releases' facts (area 11, "How
/// a release's facts, change record and fingerprints are worked out").
/// The previous release's tests aren't run, so it has no "behaviour
/// changed" entries: a registry release's record has them.
fn worked_out(root: &Path, folder: &str, before: &str, verified: &Verified) -> Vec<Entry> {
    let mut entries = jadpo_check::release::surface_changes(before, &verified.surface_text);
    if let Some(now) = &verified.facts {
        let was = crate::package::compile(&root.join(folder), "package update")
            .ok()
            .and_then(|compiled| {
                jadpo_check::release::facts(
                    &compiled.loaded.db,
                    compiled.loaded.project,
                    &compiled.mount,
                )
            });
        entries.extend(jadpo_check::release::personal_data_changes(
            was.as_ref(),
            now,
        ));
    }
    entries
        .into_iter()
        .map(|entry| Entry {
            declaration: entry.declaration,
            kind: entry.kind,
            change: entry.change.name(),
            detail: entry.detail,
            why: entry.why,
        })
        .collect()
}

fn registry_change(change: jadpo_registry::protocol::Change) -> &'static str {
    use jadpo_registry::protocol::Change;
    match change {
        Change::Added => "added",
        Change::Removed => "removed",
        Change::Retyped => "retyped",
        Change::NewFailure => "new_failure",
        Change::NewHost => "new_host",
        Change::PersonalDataSent => "personal_data_sent",
        Change::BehaviourChanged => "behaviour_changed",
    }
}

/// An entry's mark, as `jadpo package update` lists the registry's record.
fn entry_mark(change: &str) -> ChangeMark {
    match change {
        "added" | "new_host" | "personal_data_sent" => ChangeMark::Added,
        "removed" => ChangeMark::Removed,
        _ => ChangeMark::Changed,
    }
}

/// An entry's columns: what happened, to what, and the detail.
fn entry_cells(entry: &Entry) -> Vec<String> {
    let word = match entry.change {
        "new_failure" => "new failure",
        "new_host" => "new host",
        "personal_data_sent" => "personal data",
        "behaviour_changed" => "behaviour changed",
        other => other,
    };
    let what = match entry.change {
        "added" | "removed" | "retyped" if entry.kind != "provider" => {
            format!("{} {}", entry.kind, entry.declaration)
        }
        _ => entry.declaration.clone(),
    };
    let mut detail = entry.detail.clone().unwrap_or_default();
    if let Some(why) = &entry.why {
        if !detail.is_empty() {
            detail.push_str(", ");
        }
        detail.push_str(&format!("a fix: {why}"));
    }
    vec![word.to_string(), what, detail]
}

/// The nodes of the package in `folder` an entry names: the declaration
/// itself, or for the package's or provider's own entry every operation
/// and capability it offers, since a host or a raised `jadpo` reaches
/// every call through it.
fn starts(graph: &Graph, folder: &str, through: &str, entry: &Entry) -> Vec<NodeId> {
    let prefix = format!("{folder}/");
    let in_package = |node: &Node| {
        node.place
            .as_ref()
            .is_some_and(|place| place.file.starts_with(&prefix))
    };
    let whole = matches!(entry.kind.as_str(), "provider" | "package");
    let wanted = format!("{through}.{}", entry.declaration);
    graph
        .ids()
        .filter(|&id| {
            let node = graph.node(id);
            if !in_package(node) {
                return false;
            }
            if whole {
                return matches!(
                    node.kind,
                    Kind::Operation | Kind::CapabilityMapping | Kind::Function
                ) && node.public;
            }
            let kind = match entry.kind.as_str() {
                "function" => Kind::Function,
                "operation" => Kind::Operation,
                "type" => Kind::Type,
                _ => return false,
            };
            node.kind == kind && node.key.name() == wanted
        })
        .collect()
}

/// What an entry reaches in the app's own code: the walk from each node it
/// names, sorted into actions, routes, jobs (schedules and subscribers)
/// and the entities' fields that store it.
fn reach(graph: &Graph, folder: &str, through: &str, entry: &Entry) -> Reached {
    let packages = format!("{}/", jadpo_db::PACKAGES);
    let mut out = Reached::default();
    if entry.change == "added" {
        // Nothing the app has can reach what it didn't have.
        return out;
    }
    let mut nodes: BTreeSet<NodeId> = BTreeSet::new();
    let mut entry_points: BTreeSet<NodeId> = BTreeSet::new();
    for start in starts(graph, folder, through, entry) {
        let found = impact::impact(graph, start);
        nodes.extend(found.reached);
        entry_points.extend(found.entry_points);
    }
    let own = |node: &Node| {
        node.place
            .as_ref()
            .is_some_and(|place| !place.file.starts_with(&packages))
    };
    for &id in &nodes {
        let node = graph.node(id);
        if !own(node) {
            continue;
        }
        match node.kind {
            Kind::Action => {
                out.actions.insert(node.name.clone());
            }
            Kind::Field => {
                let held_by_entity = impact::holder(graph, id)
                    .is_some_and(|holder| graph.node(holder).kind == Kind::Entity);
                if held_by_entity {
                    out.stored.insert(node.name.clone());
                }
            }
            _ => {}
        }
    }
    let mut routes: Vec<&Node> = Vec::new();
    let mut jobs: Vec<&Node> = Vec::new();
    for &id in &entry_points {
        let node = graph.node(id);
        if !own(node) {
            continue;
        }
        match node.kind {
            Kind::Route | Kind::GeneratedRoute => routes.push(node),
            _ => jobs.push(node),
        }
    }
    routes.sort_by(|a, b| a.name.cmp(&b.name));
    jobs.sort_by_key(|node| (node.kind != Kind::Schedule, node.name.clone()));
    out.routes = routes.iter().map(|node| node.name.clone()).collect();
    out.jobs = jobs
        .iter()
        .map(|node| format!("{} {}", node.kind.word(), node.name))
        .collect();
    out
}

/// The errors `then` has that `now` doesn't, outside the migration's,
/// which the plan lists as the schema changes they are, and the
/// acceptances', which are the review's.
fn new_errors(now: &jadpo_db::Loaded, then: &jadpo_db::Loaded, candidate: &FileSet) -> Vec<Error> {
    let text = |path: &str| -> Option<&str> {
        candidate
            .files
            .get(path)
            .and_then(|text| text.as_ref().ok())
            .map(String::as_str)
    };
    let key = |d: &Diagnostic, text: &dyn Fn(&str) -> Option<String>| {
        let at = d.primary.as_ref().map(|label| {
            let line =
                text(&label.span.path).map(|text| Position::of(&text, label.span.start).line);
            (label.span.path.clone(), line)
        });
        (d.rule.id, at, d.message.clone())
    };
    let before_files = jadpo_db::source_texts(&now.db, now.project);
    let before_text = |path: &str| before_files.get(path).cloned();
    let seen: BTreeSet<_> = jadpo_check::check(&now.db, now.project)
        .iter()
        .map(|d| key(d, &before_text))
        .collect();
    let after_text = |path: &str| text(path).map(str::to_string);
    let mut out = Vec::new();
    let mut listed = BTreeSet::new();
    for diagnostic in jadpo_check::check(&then.db, then.project) {
        let id = diagnostic.rule.id;
        if id.starts_with("migration.") || id.starts_with("acceptance.") {
            continue;
        }
        let found = key(&diagnostic, &after_text);
        if seen.contains(&found) || !listed.insert(found.clone()) {
            continue;
        }
        let (_, at, message) = found;
        let (path, line) = match at {
            Some((path, line)) => (Some(path), line),
            None => (None, None),
        };
        out.push(Error {
            rule: id,
            message,
            path,
            line,
        });
    }
    out
}

/// The schema changes from what the app asks for now to what it would ask
/// for against the candidate, each with the decision the migration needs.
fn migration(now: &Schema, then: &Schema) -> Vec<Migration> {
    let needed: Vec<Needed> = decisions_needed(now, then);
    let decision = |store: &str, table: &str, column: Option<&str>| {
        needed
            .iter()
            .find(|n| n.store == store && n.table == table && column.is_none_or(|c| n.column == c))
            .map(|n| n.kind)
    };
    jadpo_check::data::migrations::readable_changes(now, then)
        .iter()
        .map(|change| {
            let (store, table) = change.table();
            let column = match change {
                SchemaChange::AddColumn { column, .. }
                | SchemaChange::RemoveColumn { column, .. }
                | SchemaChange::AlterColumn { column, .. } => Some(column.as_str()),
                _ => None,
            };
            let mark = match change {
                SchemaChange::AddStore { .. }
                | SchemaChange::AddTable { .. }
                | SchemaChange::AddColumn { .. }
                | SchemaChange::AddUnique { .. }
                | SchemaChange::AddForeignKey { .. }
                | SchemaChange::AddIndex { .. }
                | SchemaChange::AddCheck { .. } => ChangeMark::Added,
                SchemaChange::AlterColumn { .. } | SchemaChange::ReviveTable { .. } => {
                    ChangeMark::Changed
                }
                _ => ChangeMark::Removed,
            };
            Migration {
                mark,
                change: change.describe(now, then),
                decision: table.and_then(|table| {
                    column.and_then(|column| decision(store, table, Some(column)))
                }),
            }
        })
        .collect()
}

/// The decision a migration needs, as `changes.jadpo` names it.
fn decision_name(kind: NeededKind) -> &'static str {
    match kind {
        NeededKind::Backfill => "backfill",
        NeededKind::Retire => "retire",
        NeededKind::Refit => "refit",
        NeededKind::Unreadable => "rewrite",
        NeededKind::DropField => "drop_field",
        NeededKind::RetireEntity => "retire_entity",
        NeededKind::DropEntity => "drop_entity",
    }
}

/// What the migration needs, in a few words: a decision a person writes,
/// or a check it makes itself.
fn decision_words(kind: NeededKind) -> String {
    if kind.optional() {
        return "checks the stored values".to_string();
    }
    format!("needs a decision: {}", decision_name(kind))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_entry_reads_as_update_lists_the_registrys_record() {
        let entry = Entry {
            declaration: "UkPostcode".to_string(),
            kind: "type".to_string(),
            change: "retyped",
            detail: Some("max_length: 7, was max_length: 8".to_string()),
            why: None,
        };
        assert_eq!(
            entry_cells(&entry),
            [
                "retyped",
                "type UkPostcode",
                "max_length: 7, was max_length: 8"
            ]
        );
        assert_eq!(entry_mark("new_failure"), ChangeMark::Changed);
        assert_eq!(entry_mark("new_host"), ChangeMark::Added);
    }

    #[test]
    fn nothing_reached_says_so() {
        assert_eq!(
            Reached::default().rows(),
            [("reaches".to_string(), "nothing in the app".to_string())]
        );
    }
}
