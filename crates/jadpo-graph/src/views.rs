//! The views of the graph `jadpo check` writes for tools (area 14, "The
//! `build/` folder"): `build/inventory/`, every route and callable;
//! `build/audit/`, what a review of failures, entities, transactions,
//! configuration and policy reads; and `build/compatibility/`, the public
//! failure codes a client relies on (area 4). Each is JSON, made of the
//! manifest's own entries ([`node_json`]), so no view can disagree with
//! the manifest or another view.

use std::collections::{BTreeMap, BTreeSet};

use crate::json::Value;
use crate::manifest::{FORMAT, name_now, node_json};
use crate::node::Section;
use crate::{Detail, EdgeKind, Graph, Kind, NodeId};

/// Every view, by the path it is written to under the project's root.
pub fn views(graph: &Graph) -> Vec<(String, Value)> {
    vec![
        (
            "build/inventory/routes.json".to_string(),
            document(
                "routes",
                nodes(graph, &[Kind::Api, Kind::Route, Kind::GeneratedRoute]),
            ),
        ),
        (
            "build/inventory/callables.json".to_string(),
            document(
                "callables",
                nodes(
                    graph,
                    &[
                        Kind::Function,
                        Kind::Action,
                        Kind::Query,
                        Kind::Fixture,
                        Kind::Schedule,
                        Kind::Subscriber,
                    ],
                ),
            ),
        ),
        (
            "build/audit/failures.json".to_string(),
            document("failures", failures(graph)),
        ),
        (
            "build/audit/entities.json".to_string(),
            document("entities", entities(graph)),
        ),
        (
            "build/audit/transactions.json".to_string(),
            document("actions", transactions(graph)),
        ),
        (
            "build/audit/configuration.json".to_string(),
            document("settings", configuration(graph)),
        ),
        (
            "build/audit/policy.json".to_string(),
            document(
                "policy",
                nodes(
                    graph,
                    &[
                        Kind::Principal,
                        Kind::AuthenticationMethod,
                        Kind::SignInProvider,
                        Kind::Tenancy,
                        Kind::Grant,
                        Kind::FieldRestriction,
                        Kind::ActionException,
                    ],
                ),
            ),
        ),
        (
            "build/compatibility/public-failure-codes.json".to_string(),
            document("codes", public_codes(graph)),
        ),
    ]
}

fn document(list: &str, entries: Vec<Value>) -> Value {
    let mut out = Value::object();
    out.set("format", FORMAT);
    out.set(list, entries);
    out
}

/// The manifest's entry of each node of `kinds`, in key order.
fn nodes(graph: &Graph, kinds: &[Kind]) -> Vec<Value> {
    graph
        .ids()
        .filter(|&id| kinds.contains(&graph.node(id).kind))
        .map(|id| node_json(graph, id))
        .collect()
}

fn is_failure(graph: &Graph, id: NodeId) -> bool {
    matches!(
        graph.node(id).detail,
        Detail::Type {
            failure: Some(_),
            ..
        }
    )
}

/// The nodes under `top`'s, at any depth, by the top node they are under.
fn under(graph: &Graph, top: &dyn Fn(NodeId) -> bool) -> BTreeMap<NodeId, Vec<NodeId>> {
    let mut out: BTreeMap<NodeId, Vec<NodeId>> = BTreeMap::new();
    for id in graph.ids() {
        let mut at = id;
        for _ in 0..16 {
            if top(at) {
                out.entry(at).or_default().push(id);
                break;
            }
            match graph
                .node(at)
                .parent
                .as_ref()
                .and_then(|key| graph.find(key))
            {
                Some(parent) => at = parent,
                None => break,
            }
        }
    }
    out
}

/// Each failure, with everything declared under it: the fields its
/// client sees and those only its log keeps, and what raises it.
fn failures(graph: &Graph) -> Vec<Value> {
    let found = under(graph, &|id| is_failure(graph, id));
    found
        .into_iter()
        .map(|(failure, members)| {
            let mut entry = node_json(graph, failure);
            let fields: Vec<Value> = members
                .into_iter()
                .filter(|&id| id != failure)
                .map(|id| node_json(graph, id))
                .collect();
            entry.set_some("members", fields);
            entry.set_some("raisedBy", incoming(graph, failure, EdgeKind::Raises));
            entry
        })
        .collect()
}

/// Each entity with everything declared under it, and the runtime's own
/// tables.
fn entities(graph: &Graph) -> Vec<Value> {
    let found = under(graph, &|id| graph.node(id).kind == Kind::Entity);
    let mut out: Vec<Value> = found
        .into_iter()
        .map(|(entity, members)| {
            let mut entry = node_json(graph, entity);
            entry.set_some(
                "members",
                members
                    .into_iter()
                    .filter(|&id| id != entity)
                    .map(|id| node_json(graph, id))
                    .collect::<Vec<_>>(),
            );
            entry
        })
        .collect();
    out.extend(nodes(graph, &[Kind::SystemTable]));
    out
}

/// Each action and fixture as a transaction: the level it runs at, the
/// entities it writes, the events it emits and what it calls, outside
/// calls included, which hold no transaction (area 6).
fn transactions(graph: &Graph) -> Vec<Value> {
    const KEPT: [&str; 10] = [
        "id",
        "kind",
        "name",
        "file",
        "lines",
        "domain",
        "isolation",
        "writes",
        "emits",
        "calls",
    ];
    graph
        .ids()
        .filter(|&id| matches!(graph.node(id).kind, Kind::Action | Kind::Fixture))
        .map(|id| {
            let mut entry = Value::object();
            let whole = node_json(graph, id);
            for key in KEPT {
                if let Some(value) = whole.get(key) {
                    entry.set(key, value.clone());
                }
            }
            entry
        })
        .collect()
}

/// Each setting and connection, with what reads it.
fn configuration(graph: &Graph) -> Vec<Value> {
    graph
        .ids()
        .filter(|&id| matches!(graph.node(id).kind, Kind::ConfigField | Kind::Connection))
        .map(|id| {
            let mut entry = node_json(graph, id);
            entry.set_some("usedBy", incoming(graph, id, EdgeKind::Uses));
            entry
        })
        .collect()
}

/// The keys of the nodes whose `kind` edges reach `to`.
fn incoming(graph: &Graph, to: NodeId, kind: EdgeKind) -> Vec<Value> {
    graph
        .incoming(to, kind)
        .map(|from| Value::from(graph.node(from).key.as_str()))
        .collect()
}

/// Each code a client can be answered with: a declared failure raised by
/// what a route runs, with its status, message and client fields, any
/// change to which breaks a client (area 4). Jadpo's own codes, such as
/// `invalid_input`, are the language's and are not listed.
fn public_codes(graph: &Graph) -> Vec<Value> {
    let mut answered: BTreeMap<NodeId, BTreeSet<String>> = BTreeMap::new();
    for route in graph.ids().filter(|&id| graph.node(id).kind == Kind::Route) {
        for target in graph.outgoing(route, EdgeKind::Runs) {
            for failure in graph.outgoing(target, EdgeKind::Raises) {
                if is_failure(graph, failure) {
                    answered
                        .entry(failure)
                        .or_default()
                        .insert(graph.node(route).key.as_str().to_string());
                }
            }
        }
    }
    let mut out: Vec<(String, Value)> = answered
        .into_iter()
        .filter_map(|(id, routes)| {
            let node = graph.node(id);
            let Detail::Type {
                failure: Some(failure),
                ..
            } = &node.detail
            else {
                return None;
            };
            let mut entry = Value::object();
            entry.set("code", failure.code.as_str());
            entry.set("failure", node.key.as_str());
            entry.set_some("kind", failure.kind.as_deref());
            entry.set_some("status", failure.status);
            entry.set_some("message", failure.message.as_deref());
            entry.set(
                "client",
                graph
                    .fields(id)
                    .into_iter()
                    .filter_map(|field| {
                        let child = graph.node(field);
                        let Detail::Field {
                            ty,
                            optional,
                            section: Some(Section::Client),
                            ..
                        } = &child.detail
                        else {
                            return None;
                        };
                        let short = child
                            .name
                            .rsplit_once('.')
                            .map_or(child.name.as_str(), |(_, short)| short);
                        let mut out = Value::object();
                        out.set("name", short);
                        out.set("type", ty.render(&|key| name_now(graph, key)));
                        out.set_some("optional", *optional);
                        Some(out)
                    })
                    .collect::<Vec<_>>(),
            );
            entry.set("routes", routes.into_iter().collect::<Vec<_>>());
            Some((failure.code.clone(), entry))
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out.into_iter().map(|(_, entry)| entry).collect()
}
