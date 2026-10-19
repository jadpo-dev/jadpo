//! One store per action (area 6, "One store per action"): an action writes
//! at most one store, counting the actions it calls, which join its
//! transaction. Moving an entity to another store can break an action that
//! never names it, so the error names the write that brings in the second
//! store, and through which call.
//!
//! Each file's actions and fixtures are read from what their bodies do
//! (`body_facts`, by name and without positions, chunk 4.11): the
//! entities each writes and reads, the queries and the actions it calls.
//! The stores each one writes, and those it uses at all, through every
//! call, are worked out once for the project, and each file reads its own
//! actions' share by name. Only a problem's position reads the typed body
//! again.
//!
//! An action may read another store outside its transaction (area 6), but
//! that isn't built yet: the runtime runs all of an action's store
//! operations in the one transaction its first begins, so a read of a
//! second store is `toolchain.not_built_yet` where it brings that store
//! in, rather than a fault as it runs.

use std::collections::{BTreeMap, BTreeSet};

use jadpo_db::{Db, File, Project};
use jadpo_diag::{Diagnostic, Span, rule};
use jadpo_graph::Kind;

use super::body::{callables_of, typed_body};
use super::facts::{Fact, body_facts, facts_at};
use crate::lookups::Name;

/// What one action or fixture does that reaches a store, in source order:
/// each write's entity, or each action it calls, with where.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Reach {
    Writes(String),
    Calls(String),
    /// `Entity.get`.
    Reads(String),
    /// A query, by name.
    Queries(String),
    /// A write to the store sessions and API tokens are kept in, or with
    /// `true` the store service keys are kept in (area 8):
    /// `sessions.start`, `Service.issue_key` and the like.
    Sessions(bool),
    /// A read of either: `sessions.tokens` or `Service.keys`.
    ReadsSessions(bool),
}

/// What `fact` reaches of a store, if anything.
fn reach_of(fact: &Fact) -> Option<Reach> {
    Some(match fact {
        Fact::Writes(write) => Reach::Writes(write.entity.clone()),
        Fact::Action(callee) => Reach::Calls(callee.name().to_string()),
        Fact::Gets(entity) => Reach::Reads(entity.clone()),
        Fact::Queries(query) => Reach::Queries(query.clone()),
        Fact::Session(op) if op.reads_only() => Reach::ReadsSessions(op.on_service()),
        Fact::Session(op) => Reach::Sessions(op.on_service()),
        Fact::Fixture(_) | Fact::Function(_) | Fact::Emits(_) => return None,
    })
}

/// One action's or fixture's name and what it reaches, each with its
/// site's range.
type Reaches = (String, Vec<(Reach, (u32, u32))>);

/// What each action and fixture in `file` reaches, with where: for a
/// problem's position, so it reads the typed bodies.
fn reaches(db: &dyn Db, project: Project, file: File) -> Vec<Reaches> {
    let mut out = Vec::new();
    for callable in callables_of(db, file) {
        let checked = typed_body(db, project, callable);
        let Some(key) = checked
            .key
            .as_ref()
            .filter(|key| matches!(key.kind(), Some(Kind::Action | Kind::Fixture)))
        else {
            continue;
        };
        let found = facts_at(&checked.body)
            .into_iter()
            .filter_map(|(fact, at)| Some((reach_of(&fact)?, at)))
            .collect();
        out.push((key.name().to_string(), found));
    }
    out
}

/// What each action and fixture in `file` writes and calls, without
/// positions.
#[salsa::tracked(returns(ref))]
pub fn store_reaches(
    db: &dyn Db,
    project: Project,
    file: File,
) -> BTreeMap<String, BTreeSet<Reach>> {
    let mut out = BTreeMap::new();
    for callable in callables_of(db, file) {
        let facts = body_facts(db, project, callable);
        let Some(key) = facts.key_of(&[Kind::Action, Kind::Fixture]) else {
            continue;
        };
        out.insert(
            key.name().to_string(),
            facts.facts.iter().filter_map(reach_of).collect(),
        );
    }
    out
}

/// The actions and fixtures in `file` whose own body starts or ends a
/// session or issues an API token or a service key (area 8).
#[salsa::tracked(returns(ref))]
fn session_effects(db: &dyn Db, project: Project, file: File) -> BTreeSet<String> {
    use jadpo_std::http::SessionOp;
    let mut out = BTreeSet::new();
    for callable in callables_of(db, file) {
        let facts = body_facts(db, project, callable);
        let Some(key) = facts.key.as_ref() else {
            continue;
        };
        let affects = facts.facts.iter().any(|fact| {
            matches!(
                fact,
                Fact::Session(
                    SessionOp::Start | SessionOp::End | SessionOp::IssueToken | SessionOp::IssueKey
                )
            )
        });
        if affects {
            out.insert(key.name().to_string());
        }
    }
    out
}

/// The actions whose response is never kept for an idempotent repeat:
/// each one that starts or ends a session, or issues a token or a key,
/// itself or through an action it calls (area 7, "Idempotency keys").
#[salsa::tracked(returns(ref))]
pub fn unkept_responses(db: &dyn Db, project: Project) -> BTreeSet<String> {
    let mut calls: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut direct: BTreeSet<String> = BTreeSet::new();
    for &file in project.files(db) {
        direct.extend(session_effects(db, project, file).iter().cloned());
        for (name, found) in store_reaches(db, project, file) {
            let callees = calls.entry(name.clone()).or_default();
            for reach in found {
                if let Reach::Calls(callee) = reach {
                    callees.insert(callee.clone());
                }
            }
        }
    }
    calls
        .keys()
        .filter(|name| {
            let mut seen = BTreeSet::from([(*name).clone()]);
            let mut queue = vec![(*name).clone()];
            while let Some(current) = queue.pop() {
                if direct.contains(&current) {
                    return true;
                }
                for callee in calls.get(&current).into_iter().flatten() {
                    if seen.insert(callee.clone()) {
                        queue.push(callee.clone());
                    }
                }
            }
            false
        })
        .cloned()
        .collect()
}

/// The stores each action and fixture writes, through every action it
/// calls: each store with an entity it writes there.
#[salsa::tracked(returns(ref))]
pub fn written_stores(db: &dyn Db, project: Project) -> BTreeMap<String, BTreeMap<String, String>> {
    stores_reached(db, project, false)
}

/// The stores each action and fixture uses at all, reading or writing,
/// through every action it calls: each store with an entity it uses there.
#[salsa::tracked(returns(ref))]
pub fn used_stores(db: &dyn Db, project: Project) -> BTreeMap<String, BTreeMap<String, String>> {
    stores_reached(db, project, true)
}

/// The store the entity called `entity` lives in.
#[salsa::tracked(returns(ref))]
fn entity_store<'db>(db: &'db dyn Db, project: Project, entity: Name<'db>) -> Option<String> {
    crate::data::entity_model(db, project, entity)
        .as_ref()?
        .store
        .clone()
}

/// The entity the query called `query` reads.
#[salsa::tracked(returns(ref))]
fn query_entity<'db>(db: &'db dyn Db, project: Project, query: Name<'db>) -> Option<String> {
    crate::data::queries::query_model(db, project, query)
        .as_ref()
        .map(|query| query.entity.clone())
}

/// The store `reach` uses directly, with the entity it uses there: a
/// write's, or with `reads`, a `get`'s or a query's.
fn store_of(db: &dyn Db, project: Project, reach: &Reach, reads: bool) -> Option<(String, String)> {
    let name = |text: &str| Name::new(db, text.to_string());
    let keys = match reach {
        Reach::Sessions(keys) => Some(*keys),
        Reach::ReadsSessions(keys) if reads => Some(*keys),
        Reach::ReadsSessions(_) => return None,
        _ => None,
    };
    if let Some(keys) = keys {
        let identity = crate::identity::identity(db, project);
        return Some(if keys {
            (
                identity.keys_store()?.to_string(),
                crate::data::SERVICE_KEYS.to_string(),
            )
        } else {
            (
                identity.sessions_store()?.to_string(),
                crate::data::SESSIONS.to_string(),
            )
        });
    }
    let entity = match reach {
        Reach::Writes(entity) => entity.clone(),
        Reach::Reads(entity) if reads => entity.clone(),
        Reach::Queries(query) if reads => query_entity(db, project, name(query)).clone()?,
        _ => return None,
    };
    let store = entity_store(db, project, name(&entity)).clone()?;
    Some((store, entity))
}

fn stores_reached(
    db: &dyn Db,
    project: Project,
    reads: bool,
) -> BTreeMap<String, BTreeMap<String, String>> {
    let mut reached: BTreeMap<String, BTreeSet<Reach>> = BTreeMap::new();
    for &file in project.files(db) {
        for (name, found) in store_reaches(db, project, file) {
            reached
                .entry(name.clone())
                .or_default()
                .extend(found.iter().cloned());
        }
    }
    let mut out = BTreeMap::new();
    for name in reached.keys() {
        let mut stores: BTreeMap<String, String> = BTreeMap::new();
        let mut seen = BTreeSet::from([name.clone()]);
        let mut queue = vec![name.clone()];
        while let Some(current) = queue.pop() {
            for reach in reached.get(&current).into_iter().flatten() {
                if let Reach::Calls(callee) = reach {
                    if seen.insert(callee.clone()) {
                        queue.push(callee.clone());
                    }
                } else if let Some((store, entity)) = store_of(db, project, reach, reads) {
                    stores.entry(store).or_insert(entity);
                }
            }
        }
        out.insert(name.clone(), stores);
    }
    out
}

/// The stores the action or fixture called `name` writes, and those it
/// uses at all, through every action it calls: what a file's checks read
/// of the project's stores, one action at a time.
#[salsa::tracked(returns(ref))]
fn stores_of<'db>(db: &'db dyn Db, project: Project, name: Name<'db>) -> Stores {
    let name = name.text(db);
    Stores {
        written: written_stores(db, project)
            .get(name)
            .cloned()
            .unwrap_or_default(),
        used: used_stores(db, project)
            .get(name)
            .cloned()
            .unwrap_or_default(),
    }
}

/// What one action or fixture reaches: each store with an entity it
/// writes there, and each with an entity it uses there.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
struct Stores {
    written: BTreeMap<String, String>,
    used: BTreeMap<String, String>,
}

/// Whether any action or fixture uses more than one store.
#[salsa::tracked(returns(copy))]
fn any_spans_stores(db: &dyn Db, project: Project) -> bool {
    used_stores(db, project)
        .values()
        .any(|stores| stores.len() > 1)
}

/// The actions and fixtures in `file` that write more than one store,
/// each reported at the write or call that brings in a second.
#[salsa::tracked(returns(ref))]
pub fn store_problems(db: &dyn Db, project: Project, file: File) -> Vec<Diagnostic> {
    if !any_spans_stores(db, project) {
        return Vec::new();
    }
    let reached = |name: &str| stores_of(db, project, Name::new(db, name.to_string()));
    let path = file.path(db);
    let mut out = Vec::new();
    for (name, found) in reaches(db, project, file) {
        let own = reached(&name);
        if own.written.len() <= 1 && own.used.len() > 1 {
            out.extend(second_store_read(db, project, path, &name, &found));
        }
    }
    for (name, found) in reaches(db, project, file) {
        if reached(&name).written.len() <= 1 {
            continue;
        }
        // The first store it reaches, and the first reach into another.
        let mut first: Option<(String, String)> = None;
        for (reach, (start, end)) in found {
            let (stores, how): (BTreeMap<String, String>, String) = match &reach {
                Reach::Writes(entity) => {
                    let store = entity_store(db, project, Name::new(db, entity.clone())).clone();
                    (
                        store.into_iter().map(|s| (s, entity.clone())).collect(),
                        "writes".to_string(),
                    )
                }
                Reach::Calls(callee) => (
                    reached(callee).written.clone(),
                    format!("calls `{callee}`, which writes"),
                ),
                Reach::Sessions(_) => (
                    store_of(db, project, &reach, false).into_iter().collect(),
                    "writes".to_string(),
                ),
                Reach::Reads(_) | Reach::Queries(_) | Reach::ReadsSessions(_) => continue,
            };
            let Some((store, entity)) = stores
                .iter()
                .find(|(store, _)| first.as_ref().is_none_or(|(s, _)| s != *store))
                .or_else(|| stores.iter().next())
            else {
                continue;
            };
            let Some((first_store, first_entity)) = &first else {
                first = Some((store.clone(), entity.clone()));
                continue;
            };
            if store == first_store {
                continue;
            }
            let what = format!("{how} `{entity}` (store `{store}`)");
            out.push(
                Diagnostic::new(
                    &rule::STORE_ACTION_SPANS_STORES,
                    format!("`{name}` writes two stores"),
                )
                .at(Span::new(path, start, end), format!("store `{store}`"))
                .reason(format!(
                    "An action's writes, and those of the actions it calls, commit as one, in one store's transaction; `{name}` writes `{first_entity}` (store `{first_store}`), and here it {what}."
                ))
                .next(format!(
                    "Write `{entity}` in a subscriber to an event `{name}` emits, or keep `{entity}` in store `{first_store}`"
                )),
            );
            break;
        }
    }
    out
}

/// Where action `name`, which writes at most one store, first uses
/// another: the store it writes is its own, or, writing none, the first it
/// uses.
fn second_store_read(
    db: &dyn Db,
    project: Project,
    path: &str,
    name: &str,
    found: &[(Reach, (u32, u32))],
) -> Option<Diagnostic> {
    let reached = |name: &str| stores_of(db, project, Name::new(db, name.to_string()));
    let mut own: Option<(String, String)> = reached(name)
        .written
        .iter()
        .next()
        .map(|(store, entity)| (store.clone(), entity.clone()));
    for (reach, (start, end)) in found {
        let (stores, how): (Vec<(String, String)>, String) = match reach {
            Reach::Calls(callee) => (
                reached(callee)
                    .used
                    .iter()
                    .map(|(s, e)| (s.clone(), e.clone()))
                    .collect(),
                format!("calls `{callee}`, which uses"),
            ),
            Reach::Writes(_) | Reach::Sessions(_) => (
                store_of(db, project, reach, true).into_iter().collect(),
                "writes".to_string(),
            ),
            Reach::Reads(_) | Reach::Queries(_) | Reach::ReadsSessions(_) => (
                store_of(db, project, reach, true).into_iter().collect(),
                "reads".to_string(),
            ),
        };
        for (store, entity) in stores {
            let Some((own_store, own_entity)) = &own else {
                own = Some((store, entity));
                continue;
            };
            if store == *own_store {
                continue;
            }
            return Some(
                Diagnostic::new(
                    &rule::TOOLCHAIN_NOT_BUILT_YET,
                    format!("Reading a second store in `{name}` isn't built yet"),
                )
                .at(Span::new(path, *start, *end), format!("store `{store}`"))
                .reason(format!(
                    "An action may read another store outside its transaction (area 6), but this compiler runs all of an action's store operations in the one transaction its first begins; `{name}` uses `{own_entity}` (store `{own_store}`), and here it {how} `{entity}` (store `{store}`)."
                ))
                .next(format!(
                    "Read `{entity}` before `{name}` runs, and pass in what it needs, for now"
                ))
                .help_at("06-reads-and-writes.html#one-store-per-action"),
            );
        }
    }
    None
}
