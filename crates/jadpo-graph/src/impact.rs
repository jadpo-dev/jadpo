//! What a change to one declaration reaches (areas 1 and 14, "Seeing a
//! change's reach" and `jadpo impact`), read from the graph's edges
//! backwards: what depends on it directly, then everything that depends on
//! those, as far as the entry points and the tests that reach it.
//!
//! A node depends on another when it calls it, runs it, reads, writes,
//! emits or subscribes to it, raises it, refers to it, includes it or
//! names it (an edge of [`DEPENDS`] from the one to the other). A change
//! to a field, a variant or a rule across fields is a change to the type
//! or entity that holds it, so what depends on that is reached too, both
//! for the node asked about and for every field the walk reaches: a type
//! stored in an entity's field reaches the entity and what uses it. Tests
//! are reached but never counted as users (area 1), and nothing depends on
//! a test.

use std::collections::{BTreeSet, VecDeque};

use crate::{EdgeKind, Graph, Kind, NodeId};

/// The edges along which a change travels, from the node that depends to
/// the node it depends on.
pub const DEPENDS: &[EdgeKind] = &[
    EdgeKind::Calls,
    EdgeKind::Runs,
    EdgeKind::Reads,
    EdgeKind::Writes,
    EdgeKind::Emits,
    EdgeKind::Subscribes,
    EdgeKind::Raises,
    EdgeKind::References,
    EdgeKind::Includes,
    EdgeKind::Uses,
];

/// What a change to one node reaches.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Impact {
    /// The node asked about.
    pub target: NodeId,
    /// The node and what a change to it is a change to: the type or
    /// entity holding a field, variant or rule.
    pub changed: Vec<NodeId>,
    /// What depends on a changed node directly, tests aside, in key order.
    /// For a callable, its callers; for anything else, what names it.
    pub users: Vec<NodeId>,
    /// The routes, schedules and subscribers the change reaches, in key
    /// order.
    pub entry_points: Vec<NodeId>,
    /// The tests the change reaches, in key order.
    pub tests: Vec<NodeId>,
    /// Everything else the change reaches, entry points included, in key
    /// order: what `jadpo package update --plan` sorts into actions,
    /// routes, jobs and stored fields (area 14).
    pub reached: Vec<NodeId>,
}

/// Whether a node of `kind` is where work starts: a route, a schedule or a
/// subscriber.
pub fn is_entry_point(kind: Kind) -> bool {
    matches!(
        kind,
        Kind::Route | Kind::GeneratedRoute | Kind::Schedule | Kind::Subscriber
    )
}

/// What depends on `id` directly, along [`DEPENDS`], in key order.
pub fn dependents(graph: &Graph, id: NodeId) -> BTreeSet<NodeId> {
    DEPENDS
        .iter()
        .flat_map(|&kind| graph.incoming(id, kind))
        .filter(|&from| from != id)
        .collect()
}

/// The type or entity holding a field, variant or rule; `None` for
/// anything else.
pub fn holder(graph: &Graph, id: NodeId) -> Option<NodeId> {
    let node = graph.node(id);
    if !matches!(node.kind, Kind::Field | Kind::Variant | Kind::Rule) {
        return None;
    }
    node.parent.as_ref().and_then(|key| graph.find(key))
}

/// The node and the holders a change to it changes too.
fn changed(graph: &Graph, target: NodeId) -> Vec<NodeId> {
    let mut out = vec![target];
    let mut at = target;
    while let Some(parent) = holder(graph, at) {
        out.push(parent);
        at = parent;
    }
    out
}

/// What a change to `target` reaches.
pub fn impact(graph: &Graph, target: NodeId) -> Impact {
    let changed = changed(graph, target);
    let callable = graph.node(target).kind.is_callable();
    let mut users = BTreeSet::new();
    for &node in &changed {
        let direct: Vec<NodeId> = if callable {
            graph.incoming(node, EdgeKind::Calls).collect()
        } else {
            dependents(graph, node).into_iter().collect()
        };
        users.extend(
            direct
                .into_iter()
                .filter(|id| !changed.contains(id) && graph.node(*id).kind != Kind::Test),
        );
    }
    let mut seen: BTreeSet<NodeId> = changed.iter().copied().collect();
    let mut queue: VecDeque<NodeId> = changed.iter().copied().collect();
    let mut entry_points = BTreeSet::new();
    let mut tests = BTreeSet::new();
    let mut reached = BTreeSet::new();
    while let Some(at) = queue.pop_front() {
        let mut next_nodes = dependents(graph, at);
        // A field reached is a change to what holds it.
        next_nodes.extend(holder(graph, at));
        for next in next_nodes {
            if !seen.insert(next) {
                continue;
            }
            let kind = graph.node(next).kind;
            if kind == Kind::Test {
                tests.insert(next);
                continue;
            }
            if is_entry_point(kind) {
                entry_points.insert(next);
            }
            reached.insert(next);
            queue.push_back(next);
        }
    }
    Impact {
        target,
        changed,
        users: users.into_iter().collect(),
        entry_points: entry_points.into_iter().collect(),
        tests: tests.into_iter().collect(),
        reached: reached.into_iter().collect(),
    }
}
