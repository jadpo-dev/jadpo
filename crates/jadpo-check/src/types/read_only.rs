//! A `GET` that runs an action (area 7, "What a route runs"; chunk G2,
//! decided 2026-11-03): an action may answer a `GET` when the graph proves
//! it writes nothing, emits nothing and makes no changing call, itself and
//! through every action it calls. So it can work a value out, raise several
//! failures and call a model, and repeating it is still safe, as HTTP
//! promises of a `GET`.
//!
//! What counts is what the other checks already say, read from the same
//! places rather than worked out again:
//!
//! - **A write** is [`Does::Writes`]: an entity's write, a counter and a
//!   transition among them; `sessions.start`, `sessions.end` and the other
//!   session, key and magic link operations that change a row; and starting
//!   or cancelling a run, each of which keeps one. `Entity.get`, `history`,
//!   a query, `sessions.tokens`, `<Service>.keys` and `batches.status` read.
//! - **An emit** is `emit`, of an event or an email.
//! - **A changing call** is what [`super::steps`] says: a provider
//!   operation with `changes: true`, idempotent or not, and a store's `put`.
//!   A provider operation with `changes: false`, a store's `read` and the
//!   `ai` calls (area 21: asking a model changes nothing at the provider)
//!   aren't.
//!
//! A function can do none of these (`effect.function_persistence`,
//! `effect.function_calls_action`), so the proof stops at a function call,
//! and a package's action is followed like any other. Every site of a body
//! counts, on any way through it, inline functions' included, so the proof
//! fails safe: a call or an operation it can't read isn't proved.
//!
//! The runtime keeps its own books beside an action, and those are never
//! the action's: a model's budget, which writes `jadpo_ai_usage` in a
//! statement of its own, a provider's circuit breaker and the default rate
//! limits. None of them joins the action's transaction, which on a `GET` is
//! `READ ONLY`, so the proof and the store agree.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use jadpo_db::{Db, File, ItemKind, Project, item_tree};
use jadpo_diag::{Diagnostic, Span, rule};
use jadpo_graph::Key;
use jadpo_graph::body::{Body, Node, SiteId, Stmt};
use jadpo_std::HttpMethod;

use super::outside::{Does, Names, bodies, does};
use super::steps::Calls;
use crate::lookups::{Lookup, Name};

/// What an action does that a `GET` can't repeat safely.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Change {
    /// A write, as written where it is made: `Note.create`.
    Writes(String),
    /// `emit`, of the event or email named.
    Emits(String),
    /// A changing call, as written where it is made:
    /// `couriers.bookings.create`.
    Calls(String),
    /// A call of an action no body was found for, so nothing is proved.
    Unread(String),
}

impl Change {
    /// As a sentence ends: `writes with `Note.create``.
    pub fn describe(&self) -> String {
        match self {
            Change::Writes(what) => format!("writes with `{what}`"),
            Change::Emits(what) => format!("emits `{what}`"),
            Change::Calls(what) => format!("makes the changing call `{what}`"),
            Change::Unread(what) => format!("calls `{what}`, which can't be read"),
        }
    }

    /// What it names, as written.
    fn written(&self) -> &str {
        match self {
            Change::Writes(what)
            | Change::Emits(what)
            | Change::Calls(what)
            | Change::Unread(what) => what,
        }
    }
}

/// Why an action isn't read-only: the nearest thing it does, and where.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotReadOnly {
    pub change: Change,
    /// Where it is done, which may be in another file.
    pub at: Span,
    /// The actions called on the way to it, nearest first; empty when the
    /// action does it itself.
    pub through: Vec<String>,
}

/// What one action or fixture does itself: its first change in source
/// order, and each action it calls.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Own {
    change: Option<(Change, Span)>,
    callees: Vec<(Key, Span)>,
}

/// What `node` does that isn't read-only, given how the body names it.
fn change_of(
    body: &Body,
    names: &Names<'_>,
    id: SiteId,
    calls: &Calls,
    package: Option<&str>,
) -> Option<(Change, Span)> {
    let node = &body.site(id).node;
    if let Node::Stmt(Stmt::Emit { value }) = node {
        let value = (*value)?;
        return Some((
            Change::Emits(names.name(body, value)),
            names.span(body, value),
        ));
    }
    let name = || names.name(body, id);
    match does(node) {
        Does::Writes => Some((Change::Writes(name()), names.span(body, id))),
        Does::Calls if !calls.reads(node, package) => {
            Some((Change::Calls(name()), names.span(body, id)))
        }
        Does::Calls | Does::Joins(_) | Does::Nothing => None,
    }
}

/// What each action and fixture does itself, by key. It is read only when
/// a `GET` runs an action, and carries positions for the message, so
/// nothing else depends on it.
#[salsa::tracked(returns(ref))]
fn own_changes(db: &dyn Db, project: Project) -> BTreeMap<Key, Own> {
    let calls = Calls::new(db, project);
    let mut out = BTreeMap::new();
    for &file in project.files(db) {
        let Some(names) = Names::of(db, file) else {
            continue;
        };
        let package = super::steps::package_of(db, project, file);
        for (key, body) in bodies(db, project, file) {
            let mut own = Own::default();
            let mut sites: Vec<(u32, SiteId)> = (0..body.sites.len() as u32)
                .map(|at| (body.site(SiteId(at)).start, SiteId(at)))
                .collect();
            sites.sort();
            for (_, id) in sites {
                if let Does::Joins(callee) = does(&body.site(id).node) {
                    own.callees.push((callee.clone(), names.span(&body, id)));
                } else if own.change.is_none() {
                    own.change = change_of(&body, &names, id, &calls, package.as_deref());
                }
            }
            out.entry(key).or_insert(own);
        }
    }
    out
}

/// Why `action` isn't read-only: the nearest change it makes, itself or
/// through an action it calls; `None` when the graph proves it read-only.
pub fn not_read_only(db: &dyn Db, project: Project, action: &Key) -> Option<NotReadOnly> {
    let owns = own_changes(db, project);
    let mut seen = BTreeSet::from([action.clone()]);
    let mut queue: VecDeque<(&Key, Vec<String>)> = VecDeque::from([(action, Vec::new())]);
    while let Some((key, through)) = queue.pop_front() {
        let Some(own) = owns.get(key) else {
            continue;
        };
        if let Some((change, at)) = &own.change {
            return Some(NotReadOnly {
                change: change.clone(),
                at: at.clone(),
                through,
            });
        }
        for (callee, at) in &own.callees {
            if !owns.contains_key(callee) {
                return Some(NotReadOnly {
                    change: Change::Unread(callee.name().to_string()),
                    at: at.clone(),
                    through,
                });
            }
            if seen.insert(callee.clone()) {
                let mut next = through.clone();
                next.push(callee.name().to_string());
                queue.push_back((callee, next));
            }
        }
    }
    None
}

/// `route.run_kind` at each `GET` in `file` running an action that isn't
/// read-only, naming what the action does and where.
#[salsa::tracked(returns(ref))]
pub fn read_only_problems(db: &dyn Db, project: Project, file: File) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    let lookup = Lookup::new(db, project);
    let mut seen = BTreeSet::new();
    for item in &item_tree(db, file).items {
        if item.kind != ItemKind::Routes || !seen.insert(item.name.as_str()) {
            continue;
        }
        let Some(read) = crate::routes::group_read(db, project, Name::new(db, item.name.clone()))
        else {
            continue;
        };
        if read.path != *file.path(db) {
            continue;
        }
        let Some(group) = &read.model else {
            continue;
        };
        for route in &group.routes {
            if route.method != HttpMethod::Get || route.run.query || route.run.generated.is_some() {
                continue;
            }
            let label = route.label();
            let Some((_, span)) = read.run_spans.iter().find(|(known, _)| *known == label) else {
                continue;
            };
            let why = match crate::graph::declaration_key(&lookup, &route.run.name) {
                Some(key) => not_read_only(db, project, &key),
                // Nothing to read, so nothing is proved.
                None => Some(NotReadOnly {
                    change: Change::Unread(route.run.name.clone()),
                    at: span.clone(),
                    through: Vec::new(),
                }),
            };
            if let Some(why) = why {
                let at = where_written(db, project, &why.at, file.path(db));
                out.push(problem(&label, route, &why, at.as_deref(), span.clone()));
            }
        }
    }
    out
}

/// `at` as `path:line`, when it isn't in the file `here`, whose message
/// shows it as a label; a report shows only that file's lines.
fn where_written(db: &dyn Db, project: Project, at: &Span, here: &str) -> Option<String> {
    if at.path == here {
        return None;
    }
    let file = project
        .files(db)
        .iter()
        .find(|file| *file.path(db) == at.path)?;
    let line = file
        .text(db)
        .bytes()
        .take(at.start as usize)
        .filter(|byte| *byte == b'\n')
        .count()
        + 1;
    Some(format!("{}:{line}", at.path))
}

/// The message for `route`, whose action `why` says isn't read-only; `at`
/// where, when that is in another file.
fn problem(
    label: &str,
    route: &crate::routes::RouteModel,
    why: &NotReadOnly,
    at: Option<&str>,
    span: Span,
) -> Diagnostic {
    let action = &route.run.name;
    let mut does = why.change.describe();
    if let Some(at) = at {
        does.push_str(&format!(" at `{at}`"));
    }
    let chain = why
        .through
        .iter()
        .fold(format!("`{action}`"), |sentence, callee| {
            format!("{sentence} calls `{callee}`, and `{callee}`")
        });
    let brief = why.change.describe();
    let message = if why.through.is_empty() {
        format!("`{label}` runs action `{action}`, which {brief}")
    } else {
        format!(
            "`{label}` runs action `{action}`, which {brief} through `{}`",
            why.through[0]
        )
    };
    Diagnostic::new(&rule::ROUTE_RUN_KIND, message)
        .at(span, "not read-only")
        .also(why.at.clone(), "here")
        .reason(format!(
            "A `GET` is safe to repeat, so it runs a named query, or an action the graph proves writes nothing, emits nothing and makes no changing call; {chain} {does}."
        ))
        .next(format!(
            "Answer `{}` with another method, or leave `{}` out of what `{action}` does",
            route.path(),
            why.change.written()
        ))
}
