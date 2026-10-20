//! The provider connections each action calls out through (area 11,
//! "Calling"), itself or through the actions it calls: what a delivery
//! to a subscriber reaches, so the outbox's circuit breaker holds the
//! deliveries that call a provider found unavailable, and only those
//! (area 12, question 18; chunk 7.2b).
//!
//! Each file's actions are read from their typed bodies, by name: the
//! connections their own calls go through, and the actions they call,
//! leaving out those a batch's run calls, which run after the action
//! commits and never in its delivery. The closure is worked out once for
//! the project.

use std::collections::{BTreeMap, BTreeSet};

use jadpo_db::{Db, File, Project};
use jadpo_graph::Kind;
use jadpo_graph::body::Callee;

use super::body::{callables_of, typed_body};
use super::facts::{Fact, body_facts};

/// One action's own calls out: the connections, and the actions it calls.
type Own = (BTreeSet<String>, BTreeSet<String>);

/// Each action in `file`, by name, with the connections its own body
/// calls through and the actions it calls.
#[salsa::tracked(returns(ref))]
fn own_calls(db: &dyn Db, project: Project, file: File) -> BTreeMap<String, Own> {
    let mut out = BTreeMap::new();
    for callable in callables_of(db, file) {
        let facts = body_facts(db, project, callable);
        let Some(key) = facts.key_of(&[Kind::Action]) else {
            continue;
        };
        let connections = typed_body(db, project, callable)
            .body
            .calls()
            .filter_map(|(_, callee)| match callee {
                Callee::Provider { connection, .. } => Some(connection.clone()),
                _ => None,
            })
            .collect();
        let callees = facts
            .facts
            .iter()
            .filter_map(|fact| match fact {
                Fact::Action(callee) => Some(callee.name().to_string()),
                _ => None,
            })
            .collect();
        out.insert(key.name().to_string(), (connections, callees));
    }
    out
}

/// The connections each action calls out through, itself or through
/// every action it calls, by the action's name; an action that calls
/// none isn't listed.
#[salsa::tracked(returns(ref))]
pub fn called_connections(db: &dyn Db, project: Project) -> BTreeMap<String, BTreeSet<String>> {
    let mut own: BTreeMap<String, Own> = BTreeMap::new();
    for &file in project.files(db) {
        for (name, calls) in own_calls(db, project, file) {
            own.insert(name.clone(), calls.clone());
        }
    }
    let mut out = BTreeMap::new();
    for name in own.keys() {
        let mut seen = BTreeSet::from([name.clone()]);
        let mut queue = vec![name.clone()];
        let mut reached = BTreeSet::new();
        while let Some(current) = queue.pop() {
            let Some((connections, callees)) = own.get(&current) else {
                continue;
            };
            reached.extend(connections.iter().cloned());
            for callee in callees {
                if seen.insert(callee.clone()) {
                    queue.push(callee.clone());
                }
            }
        }
        if !reached.is_empty() {
            out.insert(name.clone(), reached);
        }
    }
    out
}
