//! The approval report (area 17, "Reviewing a change"; area 18,
//! "Comparing two builds"; chunk V2): the difference between the before
//! side's manifest and today's, explained in the order it matters.
//!
//! 1. Each change that needs a person, in plain words, with whether an
//!    acceptance this change adds lists it, and the path that makes it
//!    true ([`paths`]).
//! 2. The declarations that didn't change but are now reached from a new
//!    place, which a line diff never shows.
//! 3. The other behaviour: the API record, the schema and migrations,
//!    failures, events and subscribers, integrations, who can do what
//!    (the permissions report's difference, as `access.jadpo_record`
//!    holds it) and wording not yet accepted.
//! 4. The source, by declaration rather than by file.
//!
//! `jadpo approval` builds the before side, caches its manifest, and
//! writes [`Report::markdown`] and [`Report::json`] to `build/review/`.

pub mod markdown;
pub mod paths;
pub mod view;

use std::collections::{BTreeMap, BTreeSet};

use jadpo_db::record::{self as format, Kind as RecordKind};
use jadpo_db::{Before, Db, Project};
use jadpo_graph::json::Value;

use crate::acceptances::{self, Need, access, files, network};
pub use paths::{Paths, Reachable, Step};
use view::{Node, View};

/// The approval report's format number, in its JSON.
pub const FORMAT: i64 = 1;

/// Where `jadpo approval` writes the report, from the project's root.
pub const MARKDOWN: &str = "build/review/approval.md";
pub const JSON: &str = "build/review/approval.json";

/// The before side as the report names it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Base {
    /// `main at 1a2b3c4`.
    pub described: String,
    /// The whole commit.
    pub commit: String,
}

/// Whether a decision's acceptance exists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Acceptance {
    /// No acceptance this change adds lists it.
    Missing,
    /// One lists it, and has no `///` reason yet.
    NoReason { file: String },
    /// One lists it, with a person's reason.
    Accepted { file: String, reason: String },
}

impl Acceptance {
    /// The state as the report says it.
    pub fn word(&self) -> &'static str {
        match self {
            Acceptance::Missing => "missing",
            Acceptance::NoReason { .. } => "reason missing",
            Acceptance::Accepted { .. } => "accepted",
        }
    }

    pub fn decided(&self) -> bool {
        matches!(self, Acceptance::Accepted { .. })
    }
}

/// One change that needs a person.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decision {
    pub need: Need,
    pub acceptance: Acceptance,
    /// The record line it shows in.
    pub record: (String, Option<i64>),
    pub paths: Paths,
}

/// Added, changed or removed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Mark {
    Added,
    Changed,
    Removed,
}

impl Mark {
    pub fn word(self) -> &'static str {
        match self {
            Mark::Added => "added",
            Mark::Changed => "changed",
            Mark::Removed => "removed",
        }
    }

    fn sign(self) -> char {
        match self {
            Mark::Added => '+',
            Mark::Changed => ' ',
            Mark::Removed => '-',
        }
    }
}

/// One entry of a record that differs, with the lines that differ.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntryDiff {
    /// The record it is in.
    pub record: String,
    pub heading: String,
    pub mark: Mark,
    /// Each line that was added or removed, as the record writes it,
    /// indented.
    pub lines: Vec<(Mark, String)>,
}

/// One other change, said in plain words.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub mark: Mark,
    pub said: String,
    /// What it counts as on the terminal: `entity`, `failure`.
    pub noun: &'static str,
    pub place: Option<(String, i64)>,
    /// Whether it says how things stand, whatever this change did, such
    /// as a type shipped unreviewed: counted without a mark.
    pub stands: bool,
}

/// One part of the other behaviour.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Section {
    /// Its key in the JSON, and its label on the terminal.
    pub key: &'static str,
    pub title: &'static str,
    pub entries: Vec<EntryDiff>,
    pub lines: Vec<Line>,
    /// Said in place of "No change", when there is more to say.
    pub note: Option<String>,
}

impl Section {
    fn new(key: &'static str, title: &'static str) -> Section {
        Section {
            key,
            title,
            entries: Vec::new(),
            lines: Vec::new(),
            note: None,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty() && self.lines.is_empty()
    }

    /// What changed, counted by noun and mark: `1 route added`, `2 types
    /// changed`.
    pub fn counts(&self) -> Vec<String> {
        let mut counts: BTreeMap<(&str, Mark, bool), usize> = BTreeMap::new();
        for entry in &self.entries {
            *counts
                .entry((noun(self.key, &entry.heading), entry.mark, false))
                .or_default() += 1;
        }
        for line in &self.lines {
            *counts
                .entry((line.noun, line.mark, line.stands))
                .or_default() += 1;
        }
        counts
            .into_iter()
            .map(|((noun, mark, stands), n)| {
                if stands {
                    format!("{n} {}", plural(noun, n))
                } else {
                    format!("{n} {} {}", plural(noun, n), mark.word())
                }
            })
            .collect()
    }
}

/// A record entry's noun, from its heading's first word.
fn noun(section: &str, heading: &str) -> &'static str {
    let word = heading.split(' ').next().unwrap_or("");
    if jadpo_std::HttpMethod::ALL.iter().any(|m| m.word() == word) {
        return "route";
    }
    match (section, word) {
        (_, "input" | "output" | "object" | "enum") => "type",
        ("api", "entity") => "type",
        (_, "failure") => "failure",
        (_, "event") => "event",
        (_, "connection") => "connection",
        ("api", "api") => "API setting",
        (_, "entity") => "entity's grants",
        (_, "field") => "field's grants",
        (_, "action" | "query") => "listing",
        (_, "decline") => "decline",
        (_, "role") => "role",
        (_, "sign_in") => "sign-in setting",
        (_, "tenant") => "tenancy",
        _ => "entry",
    }
}

/// `route`, `routes`; `entity's grants`, `entities' grants`.
pub fn plural(noun: &str, n: usize) -> String {
    if n == 1 {
        return noun.to_string();
    }
    match noun {
        "entity" => "entities".to_string(),
        "entity's grants" => "entities' grants".to_string(),
        "field's grants" => "fields' grants".to_string(),
        "tenancy" => "tenancies".to_string(),
        "lock entry" => "lock entries".to_string(),
        "entry" => "entries".to_string(),
        "`fails` entry" => "`fails` entries".to_string(),
        "translation not yet accepted" => "translations not yet accepted".to_string(),
        "type shipped unreviewed" => "types shipped unreviewed".to_string(),
        "call not confirmed" => "calls not confirmed".to_string(),
        noun => format!("{noun}s"),
    }
}

/// One declaration whose source changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceChange {
    pub kind: String,
    pub name: String,
    pub domain: Option<String>,
    pub mark: Mark,
    pub file: String,
    pub lines: (i64, i64),
    /// Its name before, when it was renamed.
    pub renamed_from: Option<String>,
}

/// The approval report.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Report {
    pub base: Base,
    /// How many errors the before side has with this compiler: a graph
    /// built with errors leaves out what couldn't be resolved.
    pub base_errors: i64,
    pub decisions: Vec<Decision>,
    /// The records the before side doesn't have, whose changes are
    /// compared with nothing (area 18, "Comparing two builds").
    pub not_judged: Vec<String>,
    pub reachable: Vec<Reachable>,
    pub other: Vec<Section>,
    pub source: Vec<SourceChange>,
}

impl Report {
    /// The decisions still waiting for an acceptance, and for a reason.
    pub fn waiting(&self) -> (usize, usize) {
        let missing = self
            .decisions
            .iter()
            .filter(|d| d.acceptance == Acceptance::Missing)
            .count();
        let reason = self
            .decisions
            .iter()
            .filter(|d| matches!(d.acceptance, Acceptance::NoReason { .. }))
            .count();
        (missing, reason)
    }

    /// How many files the changed declarations are in.
    pub fn source_files(&self) -> usize {
        self.source
            .iter()
            .map(|change| change.file.as_str())
            .collect::<BTreeSet<_>>()
            .len()
    }

    pub fn markdown(&self) -> String {
        markdown::render(self)
    }

    pub fn json(&self) -> Value {
        json(self)
    }
}

/// The report of the change from `before`, the base's manifest, to
/// `after`, today's, for the project whose before side `jadpo check`
/// read from git.
pub fn report(
    db: &dyn Db,
    project: Project,
    base: Base,
    before: &Value,
    before_declines: &Value,
    after: &Value,
) -> Report {
    let was = View::of(before);
    let now = View::of(after);
    let side = match project.before(db) {
        Before::At(side) => Some(side),
        _ => None,
    };
    let records = Records::read(db, project, side);
    let public_now = records.public_routes(false);
    let public_was = records.public_routes(true);
    let mut decisions = Vec::new();
    let added: Vec<files::Acceptance> = files::acceptances(db, project)
        .into_iter()
        .filter(|acceptance| side.is_none_or(|side| side.text(&acceptance.path).is_none()))
        .collect();
    for need in acceptances::needs(db, project) {
        let mut found = paths::paths(&now, need, &public_now);
        if found.shown.is_empty() {
            found = paths::paths(&was, need, &public_was);
        }
        decisions.push(Decision {
            acceptance: acceptance_of(need, &added),
            record: (
                need.path.clone(),
                need.heading
                    .as_deref()
                    .and_then(|heading| records.line_of(&need.path, heading)),
            ),
            paths: found,
            need: need.clone(),
        });
    }
    let other = vec![
        records.section("api", "The API record", |path| path.starts_with("api/")),
        schema(&was, &now),
        failures(&was, &now),
        {
            let mut section = records.section("events", "Events and subscribers", |path| {
                crate::events::record::domain_of(path).is_some()
            });
            section.lines = events(&was, &now);
            section
        },
        {
            let mut section =
                records.section("integrations", "Integrations", |path| path == network::PATH);
            // A connection whose hosts changed shows in the record's diff.
            let shown: BTreeSet<&str> = section
                .entries
                .iter()
                .filter_map(|entry| entry.heading.strip_prefix("connection "))
                .collect();
            section.lines = integrations(&was, &now, &shown);
            section.lines.extend(shipped_unchecked(db, project));
            section
        },
        {
            let mut section =
                records.section("access", "Who can do what", |path| path == access::PATH);
            section.lines = new_reach(&was, &now);
            section
        },
        declines(db, project, before_declines),
        prompts(&was, &now),
        wording(db, project),
    ];
    Report {
        base,
        base_errors: was.errors,
        decisions,
        not_judged: acceptances::introduced(db, project)
            .into_iter()
            .map(str::to_string)
            .collect(),
        reachable: paths::newly_reachable(&was, &now),
        other,
        source: source(&was, &now),
    }
}

/// The acceptance this change adds that lists `need`, one with a reason
/// first.
fn acceptance_of(need: &Need, added: &[files::Acceptance]) -> Acceptance {
    let listing: Vec<&files::Acceptance> = added
        .iter()
        .filter(|acceptance| {
            acceptance
                .entries
                .iter()
                .any(|entry| entry.subject == need.subject && entry.change == need.change)
        })
        .collect();
    if let Some(with) = listing.iter().find(|a| a.reason.is_some()) {
        return Acceptance::Accepted {
            file: with.path.clone(),
            reason: with.reason.clone().unwrap_or_default(),
        };
    }
    match listing.first() {
        Some(without) => Acceptance::NoReason {
            file: without.path.clone(),
        },
        None => Acceptance::Missing,
    }
}

// --- records ----------------------------------------------------------------

/// Each record on both sides, by path: the API records, the event records,
/// `access.jadpo_record` and `network.jadpo_record`.
struct Records {
    /// Path, its kind, its text before and now.
    texts: Vec<(String, &'static RecordKind, Option<String>, Option<String>)>,
}

impl Records {
    fn read(db: &dyn Db, project: Project, side: Option<&jadpo_db::BeforeSide>) -> Records {
        let before = |path: &str| side.and_then(|side| side.text(path)).map(str::to_string);
        let mut texts = Vec::new();
        let mut apis: BTreeMap<String, Option<String>> = BTreeMap::new();
        if crate::routes::routes_clean(db, project) {
            for (api, text) in crate::routes::record::record_texts(db, project) {
                apis.insert(
                    format!("api/{api}{}", format::EXTENSION),
                    Some(text.clone()),
                );
            }
        }
        let mut events: BTreeMap<String, Option<String>> = BTreeMap::new();
        if *crate::events::record::events_clean(db, project) {
            for (domain, text) in crate::events::record::record_texts(db, project) {
                events.insert(
                    format!("domains/{domain}/events/{}", format::EVENTS_FILE),
                    Some(text.clone()),
                );
            }
        }
        if let Some(side) = side {
            for path in side.files.keys() {
                if let Some((kind, _)) = format::kind_of(path) {
                    if kind.word == format::API.word {
                        apis.entry(path.clone()).or_insert(None);
                    } else if kind.word == format::EVENTS.word {
                        events.entry(path.clone()).or_insert(None);
                    }
                }
            }
        }
        for (path, now) in apis {
            texts.push((path.clone(), &format::API, before(&path), now));
        }
        for (path, now) in events {
            texts.push((path.clone(), &format::EVENTS, before(&path), now));
        }
        if let Some(now) = access::record_text(db, project) {
            texts.push((
                access::PATH.to_string(),
                &format::ACCESS,
                before(access::PATH),
                now.clone(),
            ));
        }
        if let Some(now) = network::record_text(db, project) {
            texts.push((
                network::PATH.to_string(),
                &format::NETWORK,
                before(network::PATH),
                now.clone(),
            ));
        }
        Records { texts }
    }

    /// The line of `path` that is `heading`, from 1: today's record, else
    /// the before side's.
    fn line_of(&self, path: &str, heading: &str) -> Option<i64> {
        let (_, _, was, now) = self.texts.iter().find(|(p, ..)| p == path)?;
        [now, was].into_iter().flatten().find_map(|text| {
            text.lines()
                .position(|line| line == heading)
                .map(|at| at as i64 + 1)
        })
    }

    /// The routes anyone may call, by API and route, before or now.
    fn public_routes(&self, before: bool) -> paths::Public {
        let mut out = paths::Public::new();
        for (path, kind, was, now) in &self.texts {
            if kind.word != format::API.word {
                continue;
            }
            let Some(text) = (if before { was } else { now }) else {
                continue;
            };
            let api = path
                .trim_start_matches("api/")
                .trim_end_matches(format::EXTENSION)
                .to_string();
            for entry in acceptances::compare::entries(text, kind) {
                if entry
                    .members
                    .iter()
                    .any(|m| m.line == "authentication none")
                {
                    out.insert((api.clone(), paths::route_name(&entry.heading)));
                }
            }
        }
        out
    }

    /// The section of the records `wanted` names, each entry that differs.
    fn section(
        &self,
        key: &'static str,
        title: &'static str,
        wanted: impl Fn(&str) -> bool,
    ) -> Section {
        let mut section = Section::new(key, title);
        for (path, kind, was, now) in &self.texts {
            if wanted(path) {
                section
                    .entries
                    .extend(record_diff(path, kind, was.as_deref(), now.as_deref()));
            }
        }
        section
    }
}

/// An entry's lines as its record writes them, below its heading.
fn member_lines(entry: &format::Entry) -> Vec<String> {
    let mut out = Vec::new();
    for member in &entry.members {
        out.push(format!("    {}", member.line));
        for nested in &member.nested {
            out.push(format!("        {nested}"));
        }
    }
    out
}

/// Each entry that differs between two texts of one record, by heading,
/// in the order today's record writes them, then the removed ones.
fn record_diff(
    path: &str,
    kind: &RecordKind,
    before: Option<&str>,
    after: Option<&str>,
) -> Vec<EntryDiff> {
    let read = |text: Option<&str>| {
        text.map(|text| acceptances::compare::entries(text, kind))
            .unwrap_or_default()
    };
    let was = read(before);
    let now = read(after);
    let old: BTreeMap<&str, &format::Entry> = was
        .iter()
        .map(|entry| (entry.heading.as_str(), entry))
        .collect();
    let new: BTreeSet<&str> = now.iter().map(|entry| entry.heading.as_str()).collect();
    let mut out = Vec::new();
    for entry in &now {
        let lines = member_lines(entry);
        match old.get(entry.heading.as_str()) {
            None => out.push(EntryDiff {
                record: path.to_string(),
                heading: entry.heading.clone(),
                mark: Mark::Added,
                lines: lines.into_iter().map(|line| (Mark::Added, line)).collect(),
            }),
            Some(previous) => {
                let before_lines = member_lines(previous);
                if before_lines == lines {
                    continue;
                }
                let kept: BTreeSet<&String> = lines.iter().collect();
                let had: BTreeSet<&String> = before_lines.iter().collect();
                let mut diff: Vec<(Mark, String)> = before_lines
                    .iter()
                    .filter(|line| !kept.contains(line))
                    .map(|line| (Mark::Removed, line.clone()))
                    .collect();
                diff.extend(
                    lines
                        .iter()
                        .filter(|line| !had.contains(line))
                        .map(|line| (Mark::Added, line.clone())),
                );
                out.push(EntryDiff {
                    record: path.to_string(),
                    heading: entry.heading.clone(),
                    mark: Mark::Changed,
                    lines: diff,
                });
            }
        }
    }
    for entry in &was {
        if !new.contains(entry.heading.as_str()) {
            out.push(EntryDiff {
                record: path.to_string(),
                heading: entry.heading.clone(),
                mark: Mark::Removed,
                lines: member_lines(entry)
                    .into_iter()
                    .map(|line| (Mark::Removed, line))
                    .collect(),
            });
        }
    }
    out
}

// --- the manifests ------------------------------------------------------------

/// The nodes of `kind` on each side, by id.
fn pairs<'v, 'm>(
    was: &'v View<'m>,
    now: &'v View<'m>,
    kind: &'v str,
) -> impl Iterator<Item = (Option<&'v Node<'m>>, Option<&'v Node<'m>>)> + 'v {
    let ids: BTreeSet<&str> = was
        .of_kind(kind)
        .into_iter()
        .chain(now.of_kind(kind))
        .map(|node| node.id)
        .collect();
    ids.into_iter().map(move |id| (was.get(id), now.get(id)))
}

/// What a node carries, its place, digest and description aside, as
/// canonical JSON: whether that differs says its meaning changed.
fn carried(node: &Node<'_>) -> String {
    let Value::Object(entries) = node.value else {
        return String::new();
    };
    let kept: BTreeMap<String, Value> = entries
        .iter()
        .filter(|(key, _)| !["lines", "digest", "doc", "file", "order"].contains(&key.as_str()))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    Value::Object(kept).to_document()
}

/// A thing added, changed or removed: `Added entity `lending.Loan``.
fn line(mark: Mark, noun: &'static str, said: String, node: &Node<'_>) -> Line {
    let word = mark.word();
    sentence(
        mark,
        noun,
        format!("{}{} {said}", word[..1].to_uppercase(), &word[1..]),
        node,
    )
}

/// A change said as a sentence of its own.
fn sentence(mark: Mark, noun: &'static str, said: String, node: &Node<'_>) -> Line {
    Line {
        mark,
        said,
        noun,
        place: node.place(),
        stands: false,
    }
}

/// The entities and their fields, and the migrations added.
fn schema(was: &View<'_>, now: &View<'_>) -> Section {
    let mut section = Section::new("schema", "Schema and migrations");
    for (old, new) in pairs(was, now, "migration_snapshot") {
        if let (None, Some(node)) = (old, new) {
            section.lines.push(line(
                Mark::Added,
                "migration",
                format!("migration `{}`", node.name),
                node,
            ));
        }
    }
    for (old, new) in pairs(was, now, "entity") {
        match (old, new) {
            (None, Some(node)) => section.lines.push(line(
                Mark::Added,
                "entity",
                format!("entity `{}`", node.shown()),
                node,
            )),
            (Some(node), None) => section.lines.push(line(
                Mark::Removed,
                "entity",
                format!("entity `{}`", node.shown()),
                node,
            )),
            (Some(old), Some(node)) if old.text("store") != node.text("store") => {
                section.lines.push(sentence(
                    Mark::Changed,
                    "entity",
                    format!(
                        "Entity `{}` moved from store `{}` to `{}`",
                        node.shown(),
                        old.text("store").unwrap_or(""),
                        node.text("store").unwrap_or("")
                    ),
                    node,
                ));
            }
            _ => {}
        }
    }
    let of_entity = |node: &Node<'_>| {
        node.value
            .get("parent")
            .and_then(Value::as_str)
            .is_some_and(|parent| parent.starts_with("entity "))
    };
    for (old, new) in pairs(was, now, "field") {
        match (old, new) {
            (None, Some(node)) if of_entity(node) => {
                // A new entity's fields come with it.
                let parent = node.text("parent").unwrap_or("");
                if was.get(parent).is_some() {
                    section.lines.push(line(
                        Mark::Added,
                        "field",
                        format!(
                            "field `{}`: `{}`",
                            node.name,
                            node.text("type").unwrap_or("")
                        ),
                        node,
                    ));
                }
            }
            (Some(node), None) if of_entity(node) => {
                let parent = node.text("parent").unwrap_or("");
                if now.get(parent).is_some() {
                    section.lines.push(line(
                        Mark::Removed,
                        "field",
                        format!("field `{}`", node.name),
                        node,
                    ));
                }
            }
            (Some(old), Some(node)) if of_entity(node) && carried(old) != carried(node) => {
                section
                    .lines
                    .push(match (old.text("type"), node.text("type")) {
                        (Some(was), Some(now)) if was != now => sentence(
                            Mark::Changed,
                            "field",
                            format!("Field `{}` is now `{now}`, where it was `{was}`", node.name),
                            node,
                        ),
                        _ => line(
                            Mark::Changed,
                            "field",
                            format!("field `{}`", node.name),
                            node,
                        ),
                    });
            }
            _ => {}
        }
    }
    section
}

fn failure_of<'m>(node: &Node<'m>) -> Option<&'m Value> {
    node.value.get("failure")
}

/// The failures declared, and the callables that can fail in a new way.
fn failures(was: &View<'_>, now: &View<'_>) -> Section {
    let mut section = Section::new("failures", "Failures");
    let said = |node: &Node<'_>| {
        let failure = failure_of(node);
        let code = failure
            .and_then(|f| f.get("code"))
            .and_then(Value::as_str)
            .unwrap_or("");
        let status = match failure.and_then(|f| f.get("status")) {
            Some(Value::Int(status)) => status.to_string(),
            _ => String::new(),
        };
        format!("failure `{}`, code `{code}`, status {status}", node.name)
    };
    for (old, new) in pairs(was, now, "type") {
        match (old, new) {
            (None, Some(node)) if failure_of(node).is_some() => {
                section
                    .lines
                    .push(line(Mark::Added, "failure", said(node), node));
            }
            (Some(node), None) if failure_of(node).is_some() => {
                section
                    .lines
                    .push(line(Mark::Removed, "failure", said(node), node));
            }
            (Some(old), Some(node))
                if failure_of(node).is_some() && failure_of(old) != failure_of(node) =>
            {
                section.lines.push(line(
                    Mark::Changed,
                    "failure",
                    format!(
                        "{}, where it was {}",
                        said(node),
                        said(old).split_once(", ").map_or("", |(_, rest)| rest)
                    ),
                    node,
                ));
            }
            _ => {}
        }
    }
    for kind in ["action", "query", "function"] {
        for (old, new) in pairs(was, now, kind) {
            let (Some(old), Some(node)) = (old, new) else {
                continue;
            };
            let before: BTreeSet<&str> = old.edges("raises").into_iter().collect();
            let after: BTreeSet<&str> = node.edges("raises").into_iter().collect();
            let name = |id: &str| {
                now.get(id)
                    .or_else(|| was.get(id))
                    .map_or_else(|| id.to_string(), |n| n.name.to_string())
            };
            for id in after.difference(&before) {
                section.lines.push(sentence(
                    Mark::Added,
                    "`fails` entry",
                    format!("`{}` can now fail with `{}`", node.shown(), name(id)),
                    node,
                ));
            }
            for id in before.difference(&after) {
                section.lines.push(sentence(
                    Mark::Removed,
                    "`fails` entry",
                    format!("`{}` no longer fails with `{}`", node.shown(), name(id)),
                    node,
                ));
            }
        }
    }
    section
}

/// The subscribers, and the events each callable emits.
fn events(was: &View<'_>, now: &View<'_>) -> Vec<Line> {
    let mut out = Vec::new();
    for (old, new) in pairs(was, now, "subscriber") {
        let said = |node: &Node<'_>| {
            let event = node
                .edges("subscribes")
                .first()
                .map(|id| id.rsplit(['.', ' ']).next().unwrap_or(id).to_string())
                .unwrap_or_default();
            format!("subscriber `{}` to `{event}`", node.name)
        };
        match (old, new) {
            (None, Some(node)) => out.push(line(Mark::Added, "subscriber", said(node), node)),
            (Some(node), None) => out.push(line(Mark::Removed, "subscriber", said(node), node)),
            (Some(old), Some(node)) if old.digest() != node.digest() => {
                out.push(line(Mark::Changed, "subscriber", said(node), node));
            }
            _ => {}
        }
    }
    for kind in ["action", "query", "function"] {
        for (old, new) in pairs(was, now, kind) {
            let (Some(old), Some(node)) = (old, new) else {
                continue;
            };
            let before: BTreeSet<&str> = old.edges("emits").into_iter().collect();
            let after: BTreeSet<&str> = node.edges("emits").into_iter().collect();
            let name = |id: &str| id.rsplit(['.', ' ']).next().unwrap_or(id).to_string();
            for id in after.difference(&before) {
                out.push(sentence(
                    Mark::Added,
                    "event emitted",
                    format!("`{}` now emits `{}`", node.shown(), name(id)),
                    node,
                ));
            }
            for id in before.difference(&after) {
                out.push(sentence(
                    Mark::Removed,
                    "event emitted",
                    format!("`{}` no longer emits `{}`", node.shown(), name(id)),
                    node,
                ));
            }
        }
    }
    out
}

/// The connections, the packages locked and the operations the app calls.
fn integrations(was: &View<'_>, now: &View<'_>, shown: &BTreeSet<&str>) -> Vec<Line> {
    let mut out = Vec::new();
    for (kind, noun) in [("connection", "connection"), ("lock_entry", "package")] {
        for (old, new) in pairs(was, now, kind) {
            let said = |node: &Node<'_>| format!("{} `{}`", noun, node.name);
            if kind == "connection" && old.or(new).is_some_and(|node| shown.contains(node.name)) {
                continue;
            }
            match (old, new) {
                (None, Some(node)) => out.push(line(Mark::Added, noun, said(node), node)),
                (Some(node), None) => out.push(line(Mark::Removed, noun, said(node), node)),
                (Some(old), Some(node)) if carried(old) != carried(node) => {
                    out.push(line(Mark::Changed, noun, said(node), node));
                }
                _ => {}
            }
        }
    }
    let called = |view: &View<'_>, id: &str| {
        view.incoming(id, "calls")
            .iter()
            .any(|caller| view.get(caller).is_some_and(|n| n.kind != "test"))
    };
    for (old, new) in pairs(was, now, "operation") {
        let said = |node: &Node<'_>| format!("operation `{}`", node.name);
        match (old, new) {
            (Some(node), None) if called(was, node.id) => {
                out.push(line(Mark::Removed, "operation", said(node), node));
            }
            (Some(old), Some(node)) if called(now, node.id) && carried(old) != carried(node) => {
                out.push(line(Mark::Changed, "operation", said(node), node));
            }
            _ => {}
        }
    }
    out
}

/// What ships without the checks an imported or written integration asks
/// for (area 11, "Mapping imported types to standard types"): the imported
/// types the app's calls reach still to review, and the calls no recording
/// confirms.
fn shipped_unchecked(db: &dyn Db, project: Project) -> Vec<Line> {
    let mut out = Vec::new();
    for review in crate::review::review(db, project) {
        for name in review.outstanding() {
            out.push(Line {
                mark: Mark::Added,
                said: format!("`{name}` of `{}` ships unreviewed", review.package),
                noun: "type shipped unreviewed",
                place: None,
                stands: true,
            });
        }
        for call in review.unconfirmed() {
            out.push(Line {
                mark: Mark::Added,
                said: format!("`{}` has no recording to confirm it", call.call),
                noun: "call not confirmed",
                place: None,
                stands: true,
            });
        }
    }
    out
}

/// What widens access without changing a grant (area 9, "Protecting
/// policy"): an output gaining a field, an action or query reading or
/// writing an entity it didn't. Policy has proved each caller may; the
/// reviewer sees the new way there.
fn new_reach(was: &View<'_>, now: &View<'_>) -> Vec<Line> {
    let mut out = Vec::new();
    let gained = |old: &Node<'_>, node: &Node<'_>, edge: &str| -> Vec<String> {
        let before: BTreeSet<&str> = old.edges(edge).into_iter().collect();
        node.edges(edge)
            .into_iter()
            .filter(|id| !before.contains(id))
            .map(|id| id.split_once(' ').map_or(id, |(_, name)| name).to_string())
            .collect()
    };
    for kind in ["action", "query", "function"] {
        for (old, new) in pairs(was, now, kind) {
            let (Some(old), Some(node)) = (old, new) else {
                continue;
            };
            for name in gained(old, node, "reads") {
                out.push(sentence(
                    Mark::Added,
                    "path",
                    format!("`{}` now reads `{name}` records", node.shown()),
                    node,
                ));
            }
            for name in gained(old, node, "writes") {
                out.push(sentence(
                    Mark::Added,
                    "path",
                    format!("`{}` now changes `{name}` records", node.shown()),
                    node,
                ));
            }
        }
    }
    for (old, new) in pairs(was, now, "type") {
        let (Some(old), Some(node)) = (old, new) else {
            continue;
        };
        if node.text("base") != Some("Output") {
            continue;
        }
        for name in gained(old, node, "uses") {
            if now.get(&format!("field {name}")).is_some() {
                out.push(sentence(
                    Mark::Added,
                    "path",
                    format!("`{}` now carries `{name}`", node.name),
                    node,
                ));
            }
        }
    }
    out
}

/// The declines of a project as `jadpo approval` keeps them for the next
/// comparison: each one's rule, subject and reason.
pub fn declines_json(db: &dyn Db, project: Project) -> Value {
    let mut out = Value::object();
    out.set(
        "declines",
        crate::declines::declines(db, project)
            .iter()
            .map(|decline| {
                let mut entry = Value::object();
                entry.set("rule", decline.rule.as_str());
                entry.set("subject", decline_subject(decline));
                entry.set("reason", decline.reason.as_deref().unwrap_or(""));
                entry
            })
            .collect::<Vec<_>>(),
    );
    out
}

/// What a decline stands above, with its domain, or its file when it
/// names nothing.
fn decline_subject(decline: &crate::declines::Decline) -> String {
    match &decline.subject {
        Some(subject) => access::qualify(&decline.path, subject),
        None => decline.path.clone(),
    }
}

/// Every decline this change adds or edits, with its reason, so declining
/// by reflex shows in review (area 1, "Declining a judgement call").
fn declines(db: &dyn Db, project: Project, before: &Value) -> Section {
    let mut section = Section::new("declines", "Declines");
    let was: BTreeMap<(&str, &str), &str> = before
        .get("declines")
        .and_then(Value::as_array)
        .unwrap_or_default()
        .iter()
        .filter_map(|entry| {
            Some((
                (
                    entry.get("rule")?.as_str()?,
                    entry.get("subject")?.as_str()?,
                ),
                entry.get("reason")?.as_str()?,
            ))
        })
        .collect();
    for decline in crate::declines::declines(db, project) {
        let subject = decline_subject(&decline);
        let reason = decline.reason.as_deref().unwrap_or("");
        let (mark, verb) = match was.get(&(decline.rule.as_str(), subject.as_str())) {
            None => (Mark::Added, "declines"),
            Some(before) if *before != reason => {
                (Mark::Changed, "gives a new reason for declining")
            }
            Some(_) => continue,
        };
        section.lines.push(Line {
            mark,
            said: format!("`{subject}` {verb} `{}`: {reason}", decline.rule),
            noun: "decline",
            place: Some((decline.path.clone(), i64::from(decline.line))),
            stands: false,
        });
    }
    section
}

/// Each prompt whose text changed, as a change of its own (area 21,
/// "Prompts").
fn prompts(was: &View<'_>, now: &View<'_>) -> Section {
    let mut section = Section::new("prompts", "Prompts");
    for (old, new) in pairs(was, now, "prompt") {
        let said = |node: &Node<'_>| format!("prompt `{}`", node.name);
        match (old, new) {
            (None, Some(node)) => {
                section
                    .lines
                    .push(line(Mark::Added, "prompt", said(node), node));
            }
            (Some(node), None) => {
                section
                    .lines
                    .push(line(Mark::Removed, "prompt", said(node), node));
            }
            (Some(old), Some(node)) if old.digest() != node.digest() => {
                section
                    .lines
                    .push(line(Mark::Changed, "prompt", said(node), node));
            }
            _ => {}
        }
    }
    section
}

/// Wording not yet accepted, when the app reviews its translations.
fn wording(db: &dyn Db, project: Project) -> Section {
    let mut section = Section::new("wording", "Wording not yet accepted");
    if !crate::translations::sources::review(db, project).required {
        section.note = Some(
            "Not reviewed: the app doesn't set `translations: { review: TranslationReview.required }`."
                .to_string(),
        );
        return section;
    }
    for waiting in crate::translations::sources::unaccepted(db, project) {
        section.lines.push(Line {
            mark: Mark::Added,
            said: format!(
                "`{}` in `{}`, in `{}`: {}",
                waiting.member, waiting.tag, waiting.path, waiting.note
            ),
            noun: "translation not yet accepted",
            place: None,
            stands: true,
        });
    }
    if section.lines.is_empty() {
        section.note = Some("None: every translation has been accepted.".to_string());
    }
    section
}

// --- the source --------------------------------------------------------------

/// The kinds of node the source is listed by, in the order a declaration
/// sharing its place with another is kept: an entity before the failure
/// the compiler provides for it.
const DECLARATIONS: &[&str] = &[
    "entity",
    "type",
    "function",
    "constant",
    "action",
    "query",
    "fixture",
    "seed",
    "test",
    "route",
    "schedule",
    "subscriber",
    "config_field",
    "connection",
    "principal",
    "authentication_method",
    "sign_in_provider",
    "translation_key",
    "prompt",
    "flag",
    "rate_limit",
    "locale_set",
];

/// Each declaration added, removed or whose source changed, grouped by
/// domain, outside installed packages.
fn source(was: &View<'_>, now: &View<'_>) -> Vec<SourceChange> {
    let mut out: Vec<SourceChange> = Vec::new();
    let mut places: BTreeMap<(String, (i64, i64)), usize> = BTreeMap::new();
    for kind in DECLARATIONS {
        for (old, new) in pairs(was, now, kind) {
            let (mark, node) = match (old, new) {
                (None, Some(node)) => (Mark::Added, node),
                (Some(node), None) => (Mark::Removed, node),
                (Some(old), Some(node))
                    if old.digest() != node.digest() || old.name != node.name =>
                {
                    (Mark::Changed, node)
                }
                _ => continue,
            };
            let (Some(file), Some(lines)) = (node.file(), node.lines()) else {
                continue;
            };
            if file.starts_with("packages/") {
                continue;
            }
            // One place is one declaration, listed once.
            let at = (file.to_string(), lines);
            if places.contains_key(&at) {
                continue;
            }
            places.insert(at, out.len());
            out.push(SourceChange {
                kind: kind.to_string(),
                name: node.name.to_string(),
                domain: node.domain().map(str::to_string),
                mark,
                file: file.to_string(),
                lines,
                renamed_from: old
                    .filter(|old| old.name != node.name)
                    .map(|old| old.name.to_string()),
            });
        }
    }
    out.sort_by(|a, b| {
        (&a.domain, &a.file, a.lines.0, &a.name).cmp(&(&b.domain, &b.file, b.lines.0, &b.name))
    });
    out
}

// --- JSON --------------------------------------------------------------------

fn place_json(entry: &mut Value, place: &Option<(String, i64)>) {
    if let Some((file, line)) = place {
        entry.set("file", file.as_str());
        entry.set("line", *line);
    }
}

fn json(report: &Report) -> Value {
    let mut out = Value::object();
    out.set("format", FORMAT);
    let mut before = Value::object();
    before.set("described", report.base.described.as_str());
    before.set("commit", report.base.commit.as_str());
    before.set("errors", report.base_errors);
    before.set_some(
        "not_judged",
        report
            .not_judged
            .iter()
            .map(|path| Value::from(path.as_str()))
            .collect::<Vec<_>>(),
    );
    out.set("before", before);
    out.set(
        "decisions",
        report
            .decisions
            .iter()
            .map(|decision| {
                let mut entry = Value::object();
                entry.set("subject", decision.need.subject.as_str());
                entry.set("change", decision.need.change.as_str());
                entry.set("said", decision.need.said.as_str());
                entry.set("acceptance", decision.acceptance.word());
                match &decision.acceptance {
                    Acceptance::Missing => {}
                    Acceptance::NoReason { file } => entry.set("file", file.as_str()),
                    Acceptance::Accepted { file, reason } => {
                        entry.set("file", file.as_str());
                        entry.set("reason", reason.as_str());
                    }
                }
                let mut record = Value::object();
                record.set("file", decision.record.0.as_str());
                if let Some(line) = decision.record.1 {
                    record.set("line", line);
                }
                entry.set("record", record);
                entry.set(
                    "paths",
                    decision
                        .paths
                        .shown
                        .iter()
                        .map(|path| {
                            Value::Array(
                                path.iter()
                                    .map(|step| {
                                        let mut item = Value::object();
                                        item.set("name", step.name.as_str());
                                        item.set("kind", step.kind.as_str());
                                        place_json(&mut item, &step.place);
                                        item
                                    })
                                    .collect(),
                            )
                        })
                        .collect::<Vec<_>>(),
                );
                entry.set_some("more_paths", decision.paths.more);
                entry
            })
            .collect::<Vec<_>>(),
    );
    out.set(
        "newly_reachable",
        report
            .reachable
            .iter()
            .map(|reachable| {
                let mut entry = Value::object();
                entry.set("name", reachable.name.as_str());
                entry.set("kind", reachable.kind.as_str());
                place_json(&mut entry, &reachable.place);
                entry.set(
                    "from",
                    reachable
                        .from
                        .iter()
                        .map(|from| Value::from(from.as_str()))
                        .collect::<Vec<_>>(),
                );
                entry
            })
            .collect::<Vec<_>>(),
    );
    let mut other = Value::object();
    for section in &report.other {
        let mut items: Vec<Value> = section
            .entries
            .iter()
            .map(|entry| {
                let mut item = Value::object();
                item.set("record", entry.record.as_str());
                item.set("heading", entry.heading.as_str());
                item.set("change", entry.mark.word());
                item.set(
                    "lines",
                    entry
                        .lines
                        .iter()
                        .map(|(mark, line)| Value::from(format!("{}{line}", mark.sign())))
                        .collect::<Vec<_>>(),
                );
                item
            })
            .collect();
        items.extend(section.lines.iter().map(|line| {
            let mut item = Value::object();
            item.set("said", line.said.as_str());
            item.set("change", line.mark.word());
            place_json(&mut item, &line.place);
            item
        }));
        other.set(section.key, items);
    }
    out.set("other", other);
    out.set(
        "source",
        report
            .source
            .iter()
            .map(|change| {
                let mut entry = Value::object();
                entry.set("name", change.name.as_str());
                entry.set("kind", change.kind.as_str());
                entry.set("change", change.mark.word());
                entry.set_some("domain", change.domain.as_deref());
                entry.set("file", change.file.as_str());
                entry.set(
                    "lines",
                    vec![Value::from(change.lines.0), Value::from(change.lines.1)],
                );
                entry.set_some("renamed_from", change.renamed_from.as_deref());
                entry
            })
            .collect::<Vec<_>>(),
    );
    out
}
