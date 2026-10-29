//! Comparing two graphs (docs/compiler-runtime.md §2.3, area 18): nodes
//! are matched by key, so a renamed node registered in the identity
//! registry reads as renamed and a moved one as unchanged; then digests,
//! what each node carries and its edges are compared. Each change says
//! which of the decided kinds a person must accept (area 18's table).
//!
//! Areas 1 to 4 decide `DomainChange` (a domain created or renamed).
//! Areas 8 and 11 (chunk 10.3g) decide the package and sign-in changes
//! area 18 lists: `PackageChange` for a used operation removed, changed or
//! failing in a new way, a host or a connection's credential scheme
//! changed, a service's image new or changed, and a package locked from
//! git; `PolicyChange` for a new kind of caller, a principal changed, and
//! a new sign-in method or way of signing in. `ApiChange`, `DataChange`,
//! policy's grants and `ThresholdChange` are classified by their areas as
//! they are built, and "newly reachable" needs deployables (area 14).

use std::collections::BTreeSet;

use crate::json::Value;
use crate::{Detail, EdgeKind, Graph, Key, Kind, Node, NodeId};

/// One edge kind's targets gained and lost by a node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EdgeChange {
    pub kind: EdgeKind,
    pub added: Vec<Key>,
    pub removed: Vec<Key>,
}

/// How a node present on both sides differs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Changed {
    /// Its name before and after, when it was renamed.
    pub renamed: Option<(String, String)>,
    /// Its source changed.
    pub digest: bool,
    /// What it carries changed: a signature, a type's base or rules, a
    /// field's type, a failure's code.
    pub detail: bool,
    /// Its `///` description changed.
    pub doc: bool,
    /// Whether it became visible to other domains, or stopped being.
    pub public: bool,
    pub edges: Vec<EdgeChange>,
}

impl Changed {
    fn any(&self) -> bool {
        self.renamed.is_some()
            || self.digest
            || self.detail
            || self.doc
            || self.public
            || !self.edges.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum What {
    Added,
    Removed,
    Changed(Changed),
}

impl What {
    pub fn word(&self) -> &'static str {
        match self {
            What::Added => "added",
            What::Removed => "removed",
            What::Changed(_) => "changed",
        }
    }
}

/// One changed node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    pub key: Key,
    pub kind: Kind,
    pub what: What,
    /// The kinds of change a person must accept, as `Kind.change`.
    pub needs: Vec<String>,
}

/// Every node that differs between two graphs, by key.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Diff {
    pub changes: Vec<Change>,
}

impl Diff {
    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }

    pub fn get(&self, key: &str) -> Option<&Change> {
        self.changes
            .iter()
            .find(|change| change.key.as_str() == key)
    }

    /// The diff as canonical JSON.
    pub fn to_json(&self) -> Value {
        let mut out = Value::object();
        out.set(
            "changes",
            self.changes
                .iter()
                .map(|change| {
                    let mut entry = Value::object();
                    entry.set("id", change.key.as_str());
                    entry.set("kind", change.kind.word());
                    entry.set("change", change.what.word());
                    entry.set_some(
                        "needs",
                        change
                            .needs
                            .iter()
                            .map(|need| Value::from(need.as_str()))
                            .collect::<Vec<_>>(),
                    );
                    if let What::Changed(changed) = &change.what {
                        if let Some((from, to)) = &changed.renamed {
                            let mut renamed = Value::object();
                            renamed.set("from", from.as_str());
                            renamed.set("to", to.as_str());
                            entry.set("renamed", renamed);
                        }
                        entry.set_some("digest", changed.digest);
                        entry.set_some("detail", changed.detail);
                        entry.set_some("doc", changed.doc);
                        entry.set_some("public", changed.public);
                        let mut edges = Value::object();
                        for edge in &changed.edges {
                            let mut lists = Value::object();
                            lists.set_some("added", keys(&edge.added));
                            lists.set_some("removed", keys(&edge.removed));
                            edges.set(edge.kind.word(), lists);
                        }
                        entry.set_some("edges", edges);
                    }
                    entry
                })
                .collect::<Vec<_>>(),
        );
        out
    }
}

fn keys(keys: &[Key]) -> Vec<Value> {
    keys.iter().map(|key| Value::from(key.as_str())).collect()
}

fn targets(graph: &Graph, id: NodeId, kind: EdgeKind) -> BTreeSet<Key> {
    graph
        .outgoing(id, kind)
        .map(|to| graph.node(to).key.clone())
        .collect()
}

/// What changed from `before` to `after`. Sites and branches are numbered
/// per build (area 18): only declarations and fields are recognised across
/// builds, so a change inside a callable shows as that callable's change.
pub fn diff(before: &Graph, after: &Graph) -> Diff {
    let mut changes = Vec::new();
    for id in before.ids() {
        let node = before.node(id);
        if node.kind.per_build() {
            continue;
        }
        if after.find(&node.key).is_none() {
            changes.push(Change {
                key: node.key.clone(),
                kind: node.kind,
                what: What::Removed,
                needs: removed_needs(before, id),
            });
        }
    }
    for id in after.ids() {
        let node = after.node(id);
        if node.kind.per_build() {
            continue;
        }
        let Some(old_id) = before.find(&node.key) else {
            let needs = added_needs(before, node);
            changes.push(Change {
                key: node.key.clone(),
                kind: node.kind,
                what: What::Added,
                needs,
            });
            continue;
        };
        let old = before.node(old_id);
        let mut changed = Changed {
            renamed: (old.name != node.name).then(|| (old.name.clone(), node.name.clone())),
            digest: old.digest != node.digest,
            detail: old.detail != node.detail,
            doc: old.doc != node.doc,
            public: old.public != node.public,
            edges: Vec::new(),
        };
        for &kind in EdgeKind::ALL {
            let was = targets(before, old_id, kind);
            let now = targets(after, id, kind);
            if was != now {
                changed.edges.push(EdgeChange {
                    kind,
                    added: now.difference(&was).cloned().collect(),
                    removed: was.difference(&now).cloned().collect(),
                });
            }
        }
        if changed.any() {
            let needs = changed_needs(after, id, old, node, &changed);
            changes.push(Change {
                key: node.key.clone(),
                kind: node.kind,
                what: What::Changed(changed),
                needs,
            });
        }
    }
    changes.sort_by(|a, b| a.key.cmp(&b.key));
    Diff { changes }
}

fn need(kind: &str) -> Vec<String> {
    vec![kind.to_string()]
}

/// Whether anything but a test calls `id`: an operation the app uses
/// (area 18, "Package upgrades").
fn called(graph: &Graph, id: NodeId) -> bool {
    graph
        .incoming(id, EdgeKind::Calls)
        .any(|caller| graph.node(caller).kind != Kind::Test)
}

/// A lock entry from git: a package not from the registry, whose publisher
/// nothing verifies (area 11, "Publisher trust"). Telling the app's own
/// company's repository from a third party's needs a setting the pages
/// don't decide, so every git package needs a person until they do.
fn from_git(node: &Node) -> bool {
    matches!(&node.detail, Detail::LockEntry { source, .. } if source == "git")
}

fn service(node: &Node) -> Option<&str> {
    match &node.detail {
        Detail::Host { service, .. } => service.as_deref(),
        _ => None,
    }
}

/// What a node added on the `after` side needs.
fn added_needs(before: &Graph, node: &Node) -> Vec<String> {
    match node.kind {
        Kind::Domain => need("DomainChange.created"),
        Kind::LockEntry if from_git(node) => need("PackageChange.git_source"),
        Kind::Host if service(node).is_some() => need("PackageChange.service_image"),
        // A new connection's hosts appear without a person; a host added
        // to a connection the app already had is its host changed.
        Kind::Host
            if node
                .parent
                .as_ref()
                .is_some_and(|connection| before.find(connection).is_some()) =>
        {
            need("PackageChange.host_changed")
        }
        Kind::Principal => need("PolicyChange.principal_added"),
        Kind::AuthenticationMethod => need("PolicyChange.sign_in_method_added"),
        Kind::SignInProvider => need("PolicyChange.sign_in_added"),
        _ => Vec::new(),
    }
}

/// What a node gone from the `before` side needs: an operation the app
/// called (area 18, "Package upgrades").
fn removed_needs(before: &Graph, id: NodeId) -> Vec<String> {
    match before.node(id).kind {
        Kind::Operation if called(before, id) => need("PackageChange.operation_removed"),
        _ => Vec::new(),
    }
}

/// What a node on both sides that differs needs.
fn changed_needs(
    after: &Graph,
    id: NodeId,
    old: &Node,
    node: &Node,
    changed: &Changed,
) -> Vec<String> {
    match node.kind {
        Kind::Domain if changed.renamed.is_some() => need("DomainChange.renamed"),
        Kind::Operation if called(after, id) => {
            let mut out = Vec::new();
            if changed.detail {
                out.push("PackageChange.operation_changed".to_string());
            }
            if changed
                .edges
                .iter()
                .any(|edge| edge.kind == EdgeKind::Raises && !edge.added.is_empty())
            {
                out.push("PackageChange.failure_added".to_string());
            }
            out
        }
        Kind::Host
            if changed.detail && service(node).is_some() && service(old) != service(node) =>
        {
            need("PackageChange.service_image")
        }
        Kind::Host if changed.detail => need("PackageChange.host_changed"),
        Kind::Connection => match (&old.detail, &node.detail) {
            (
                Detail::Connection {
                    credential: was, ..
                },
                Detail::Connection {
                    credential: now, ..
                },
            ) if was != now => need("PackageChange.credential_changed"),
            _ => Vec::new(),
        },
        Kind::LockEntry if from_git(node) && changed.detail => need("PackageChange.git_source"),
        Kind::Principal if changed.detail => need("PolicyChange.principal_changed"),
        _ => Vec::new(),
    }
}
