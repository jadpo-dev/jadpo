//! One store per action (area 6, "One store per action"): an action writes
//! at most one store, counting the actions it calls, which join its
//! transaction. Moving an entity to another store can break an action that
//! never names it, so the error names the write that brings in the second
//! store, and through which call.
//!
//! Each file's actions and fixtures are read from their typed bodies, by
//! name and without positions: the entities each writes and the actions it
//! calls. The stores each one writes, through every call, are worked out
//! once for the project.

use std::collections::{BTreeMap, BTreeSet};

use jadpo_db::{Db, File, Project};
use jadpo_diag::{Diagnostic, Span, rule};
use jadpo_graph::Kind;
use jadpo_graph::body::{Callee, Node};

use super::body::{callables_of, typed_body};

/// What one action or fixture does that reaches a store, in source order:
/// each write's entity, or each action it calls, with where.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Reach {
    Writes(String),
    Calls(String),
}

/// One action's or fixture's name and what it reaches, each with its
/// site's range.
type Reaches = (String, Vec<(Reach, (u32, u32))>);

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
        let mut found = Vec::new();
        for site in &checked.body.sites {
            let reach = match &site.node {
                Node::Expr(jadpo_graph::body::Expr::Write { entity, .. }) => {
                    Reach::Writes(entity.name().to_string())
                }
                Node::Expr(jadpo_graph::body::Expr::Call {
                    callee: Callee::Action(callee),
                    ..
                }) => Reach::Calls(callee.name().to_string()),
                _ => continue,
            };
            found.push((reach, (site.start, site.end)));
        }
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
    reaches(db, project, file)
        .into_iter()
        .map(|(name, found)| (name, found.into_iter().map(|(reach, _)| reach).collect()))
        .collect()
}

/// The stores each action and fixture writes, through every action it
/// calls: each store with an entity it writes there.
#[salsa::tracked(returns(ref))]
pub fn written_stores(db: &dyn Db, project: Project) -> BTreeMap<String, BTreeMap<String, String>> {
    let model = crate::data::data_model(db, project);
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
                match reach {
                    Reach::Writes(entity) => {
                        if let Some(store) = model.entity(entity).and_then(|e| e.store.clone()) {
                            stores.entry(store).or_insert_with(|| entity.clone());
                        }
                    }
                    Reach::Calls(callee) => {
                        if seen.insert(callee.clone()) {
                            queue.push(callee.clone());
                        }
                    }
                }
            }
        }
        out.insert(name.clone(), stores);
    }
    out
}

/// The actions and fixtures in `file` that write more than one store,
/// each reported at the write or call that brings in a second.
#[salsa::tracked(returns(ref))]
pub fn store_problems(db: &dyn Db, project: Project, file: File) -> Vec<Diagnostic> {
    let written = written_stores(db, project);
    if written.values().all(|stores| stores.len() <= 1) {
        return Vec::new();
    }
    let model = crate::data::data_model(db, project);
    let path = file.path(db);
    let mut out = Vec::new();
    for (name, found) in reaches(db, project, file) {
        if written.get(&name).is_none_or(|stores| stores.len() <= 1) {
            continue;
        }
        // The first store it reaches, and the first reach into another.
        let mut first: Option<(String, String)> = None;
        for (reach, (start, end)) in found {
            let (stores, how): (BTreeMap<String, String>, String) = match &reach {
                Reach::Writes(entity) => {
                    let store = model.entity(entity).and_then(|e| e.store.clone());
                    (
                        store.into_iter().map(|s| (s, entity.clone())).collect(),
                        "writes".to_string(),
                    )
                }
                Reach::Calls(callee) => (
                    written.get(callee).cloned().unwrap_or_default(),
                    format!("calls `{callee}`, which writes"),
                ),
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
