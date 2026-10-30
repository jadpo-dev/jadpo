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
use super::facts::{BatchFact as BatchFactRef, Fact, body_facts, facts_at};
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
    /// `batches.each` walking a query, by name (area 12): its run is kept
    /// in the query's store, which its items' writes and its position
    /// commit to together, so starting it writes that store.
    Batch(String),
    /// `batches.cancel`, or with `false` `batches.status`: the runs the
    /// store holding them keeps.
    Runs(bool),
}

/// What `fact` reaches of a store: a `batches.each` call reaches the
/// store its query reads and the actions its run calls, which commit
/// there.
fn reaches_of(fact: &Fact) -> Vec<Reach> {
    vec![match fact {
        Fact::Writes(write) => Reach::Writes(write.entity.clone()),
        Fact::Action(callee) => Reach::Calls(callee.name().to_string()),
        Fact::Gets(entity) => Reach::Reads(entity.clone()),
        Fact::Queries(query) => Reach::Queries(query.clone()),
        Fact::Session(op) if op.reads_only() => Reach::ReadsSessions(op.on_service()),
        Fact::Session(op) => Reach::Sessions(op.on_service()),
        Fact::Batch(batch) => {
            return batch
                .query
                .iter()
                .map(|query| Reach::Batch(query.clone()))
                .chain(
                    [&batch.each, &batch.finished]
                        .into_iter()
                        .flatten()
                        .map(|action| Reach::Calls(action.name().to_string())),
                )
                .collect();
        }
        Fact::Runs(op) => Reach::Runs(*op == jadpo_std::batch::BatchOp::Cancel),
        Fact::Fixture(_) | Fact::Function(_) | Fact::Emits(_) => return Vec::new(),
    }]
}

/// The stores that keep batch runs (area 12; chunk 6.4): each one a
/// `batches.each` call's query reads, or a file helper's run is kept in
/// ([`file_run_stores`]).
#[salsa::tracked(returns(ref))]
pub fn batch_stores(db: &dyn Db, project: Project) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for &file in project.files(db) {
        for callable in callables_of(db, file) {
            let facts = body_facts(db, project, callable);
            for fact in &facts.facts {
                if let Fact::Batch(batch) = fact
                    && let Some(query) = &batch.query
                    && let Some(entity) =
                        query_entity(db, project, Name::new(db, query.clone())).clone()
                    && let Some(store) = entity_store(db, project, Name::new(db, entity)).clone()
                {
                    out.insert(store);
                }
                if let Fact::Batch(BatchFactRef { file: true, .. }) = fact
                    && let Some(key) = &facts.key
                    && let Some(Some(store)) = file_run_stores(db, project).get(key.name())
                {
                    out.insert(store.clone());
                }
            }
        }
    }
    out
}

/// Where each action starting a file helper's run keeps it (chunk 6.4b):
/// a file has no query whose store keeps its run, so it is kept in the
/// one store the action writes, counting what its run's actions write,
/// which area 6's one-store rule makes one; when none of them writes, in
/// the application's only database. `None` when that leaves no one store:
/// the actions write several, which [`store_problems`] reports, or none,
/// in an application with several databases ([`file_run_problems`]).
///
/// It reads what each action writes without the runs `batches.cancel`
/// keeps, so it never waits on [`batch_stores`].
#[salsa::tracked(returns(ref))]
pub fn file_run_stores(db: &dyn Db, project: Project) -> BTreeMap<String, Option<String>> {
    let mut reached: BTreeMap<String, BTreeSet<Reach>> = BTreeMap::new();
    let mut starters: BTreeSet<String> = BTreeSet::new();
    for &file in project.files(db) {
        for (name, found) in store_reaches(db, project, file) {
            reached
                .entry(name.clone())
                .or_default()
                .extend(found.iter().cloned());
        }
        for callable in callables_of(db, file) {
            let facts = body_facts(db, project, callable);
            if facts
                .facts
                .iter()
                .any(|fact| matches!(fact, Fact::Batch(BatchFactRef { file: true, .. })))
                && let Some(key) = &facts.key
            {
                starters.insert(key.name().to_string());
            }
        }
    }
    let databases = crate::data::connections(db, project).databases();
    let mut out = BTreeMap::new();
    for name in starters {
        let mut stores: BTreeSet<String> = BTreeSet::new();
        let mut seen = BTreeSet::from([name.clone()]);
        let mut queue = vec![name.clone()];
        while let Some(current) = queue.pop() {
            for reach in reached.get(&current).into_iter().flatten() {
                match reach {
                    Reach::Calls(callee) => {
                        if seen.insert(callee.clone()) {
                            queue.push(callee.clone());
                        }
                    }
                    Reach::Runs(_) => {}
                    other => {
                        if let Some((store, _)) = store_of(db, project, other, false) {
                            stores.insert(store);
                        }
                    }
                }
            }
        }
        let store = match (stores.len(), databases.as_slice()) {
            (1, _) => stores.into_iter().next(),
            (0, [only]) => Some(only.clone()),
            _ => None,
        };
        out.insert(name, store);
    }
    out
}

/// The file helpers' reads in `file` whose run no one store keeps: their
/// actions write nothing, in an application with several databases
/// (chunk 6.4b).
#[salsa::tracked(returns(ref))]
pub fn file_run_problems(db: &dyn Db, project: Project, file: File) -> Vec<Diagnostic> {
    let path = file.path(db);
    let mut out = Vec::new();
    let runs = file_run_stores(db, project);
    for callable in callables_of(db, file) {
        let checked = typed_body(db, project, callable);
        let Some(key) = checked.key.as_ref() else {
            continue;
        };
        if !matches!(runs.get(key.name()), Some(None)) {
            continue;
        }
        if written_stores(db, project)
            .get(key.name())
            .is_some_and(|stores| stores.len() > 1)
        {
            continue;
        }
        for site in &checked.body.sites {
            let jadpo_graph::body::Node::Expr(jadpo_graph::body::Expr::Call {
                callee: jadpo_graph::body::Callee::Batch(jadpo_std::batch::BatchOp::Read(format)),
                ..
            }) = &site.node
            else {
                continue;
            };
            let written = format!("{}.read", format.namespace());
            let databases = crate::data::connections(db, project).databases();
            let listed: Vec<String> = databases.iter().map(|d| format!("`{d}`")).collect();
            out.push(
                Diagnostic::new(
                    &rule::TOOLCHAIN_NOT_BUILT_YET,
                    format!("`{written}` here keeps its run in no one store"),
                )
                .at(Span::new(path, site.start, site.end), "which store keeps the run?")
                .reason(format!(
                    "A file has no query whose store keeps its run, so it is kept in the store its actions write; `{}` and the actions its run calls write nothing, and the application's databases are {}.",
                    key.name(),
                    crate::and_list(&listed)
                ))
                .next(format!(
                    "Have `{written}`'s `each:` action write the records it reads, for now"
                )),
            );
        }
    }
    out
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
            .flat_map(|(fact, at)| reaches_of(&fact).into_iter().map(move |reach| (reach, at)))
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
            facts.facts.iter().flat_map(reaches_of).collect(),
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
    // The runs live in the one store batches keep them in; with none, or
    // more than one, nothing pins `batches.status` and `batches.cancel` to
    // a store, which their call reports.
    if let Reach::Runs(writes) = reach {
        if !writes && !reads {
            return None;
        }
        let stores = batch_stores(db, project);
        let [store] = stores.iter().collect::<Vec<_>>()[..] else {
            return None;
        };
        return Some((store.clone(), jadpo_std::batch::RUNS.to_string()));
    }
    let entity = match reach {
        Reach::Writes(entity) => entity.clone(),
        Reach::Batch(query) => query_entity(db, project, name(query)).clone()?,
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
                Reach::Sessions(_) | Reach::Runs(true) => (
                    store_of(db, project, &reach, false).into_iter().collect(),
                    "writes".to_string(),
                ),
                Reach::Batch(query) => (
                    store_of(db, project, &reach, false).into_iter().collect(),
                    format!("starts a batch over `{query}`, whose run is kept with"),
                ),
                Reach::Reads(_)
                | Reach::Queries(_)
                | Reach::ReadsSessions(_)
                | Reach::Runs(false) => {
                    continue;
                }
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

/// The calls of `batches.status` and `batches.cancel` in `file` that no
/// one store pins (area 12, chunk 6.4): a run is found by its `id` alone,
/// in the store the application keeps its runs in, so an application
/// whose batches keep them in more than one store, or that starts none,
/// has nowhere to look.
#[salsa::tracked(returns(ref))]
pub fn batch_store_problems(db: &dyn Db, project: Project, file: File) -> Vec<Diagnostic> {
    let stores = batch_stores(db, project);
    if stores.len() == 1 {
        return Vec::new();
    }
    let path = file.path(db);
    let mut out = Vec::new();
    for (_, found) in reaches(db, project, file) {
        for (reach, (start, end)) in found {
            let Reach::Runs(cancels) = reach else {
                continue;
            };
            let written = if cancels {
                "batches.cancel"
            } else {
                "batches.status"
            };
            let reason = if stores.is_empty() {
                format!(
                    "`{written}` finds a run in the store the application keeps its runs in, beside the store its query reads, and nothing in this application starts one with `batches.each`."
                )
            } else {
                let listed: Vec<String> = stores.iter().map(|s| format!("`{s}`")).collect();
                format!(
                    "`{written}` finds a run by its `id` alone, in the store the application keeps its runs in, and this application's batches keep them in {}; this compiler doesn't look in more than one yet.",
                    crate::and_list(&listed)
                )
            };
            out.push(
                Diagnostic::new(
                    &rule::TOOLCHAIN_NOT_BUILT_YET,
                    format!("`{written}` here isn't built yet"),
                )
                .at(Span::new(path, start, end), "no one store keeps the runs")
                .reason(reason)
                .next(format!(
                    "Start the runs with `batches.each` over queries of one store, or do without `{written}` for now"
                )),
            );
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
            Reach::Writes(_) | Reach::Sessions(_) | Reach::Runs(true) => (
                store_of(db, project, reach, true).into_iter().collect(),
                "writes".to_string(),
            ),
            Reach::Batch(query) => (
                store_of(db, project, reach, true).into_iter().collect(),
                format!("starts a batch over `{query}`, whose run is kept with"),
            ),
            Reach::Reads(_) | Reach::Queries(_) | Reach::ReadsSessions(_) | Reach::Runs(false) => (
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
                )),
            );
        }
    }
    None
}
