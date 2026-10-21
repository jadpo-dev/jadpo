//! The graph itself: nodes in key order, each addressed in memory by a
//! typed index, and edges between them. A graph is built once and never
//! changed; the next check builds a new one. Indexes are positions in this
//! graph only and are never written out: the manifest names nodes by key.

use crate::{Detail, EdgeKind, Key, Kind, Node};

/// A node's position in one graph.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(u32);

impl NodeId {
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// One edge: `from` `kind` `to`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Edge {
    pub from: NodeId,
    pub kind: EdgeKind,
    pub to: NodeId,
}

/// The application graph.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Graph {
    /// Sorted by key; a key appears once.
    nodes: Vec<Node>,
    /// Sorted by `from`, kind, then `to`; no repeats.
    edges: Vec<Edge>,
    /// The same edges sorted by `to`, kind, then `from`.
    incoming: Vec<Edge>,
}

impl Graph {
    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// A digest of every node's identity key and content digest (chunk
    /// 7.5b), 32 hex digits: the same source always gives the same one,
    /// and a change to any node's content another. What a record of
    /// several changing calls keeps as its build under `jadpo dev`, and
    /// what area 18's build id digests as the source (chunk 8.1o).
    pub fn digest(&self) -> String {
        let mut nodes: Vec<(&str, String)> = self
            .nodes
            .iter()
            .map(|node| {
                let digest = node.digest.as_ref().map(|d| d.short()).unwrap_or_default();
                (node.key.as_str(), digest)
            })
            .collect();
        nodes.sort();
        let mut hasher = blake3::Hasher::new();
        for (key, digest) in &nodes {
            for part in [*key, digest.as_str()] {
                hasher.update(&(part.len() as u64).to_le_bytes());
                hasher.update(part.as_bytes());
            }
        }
        hasher.finalize().to_hex()[..32].to_string()
    }

    pub fn ids(&self) -> impl Iterator<Item = NodeId> + '_ {
        (0..self.nodes.len()).map(|i| NodeId(i as u32))
    }

    pub fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id.index()]
    }

    /// The node with `key`.
    pub fn find(&self, key: &Key) -> Option<NodeId> {
        self.nodes
            .binary_search_by(|node| node.key.cmp(key))
            .ok()
            .map(|i| NodeId(i as u32))
    }

    pub fn get(&self, key: &Key) -> Option<&Node> {
        self.find(key).map(|id| self.node(id))
    }

    /// The node of `kind` whose name now is `name`.
    pub fn named(&self, kind: Kind, name: &str) -> Option<NodeId> {
        self.find(&Key::new(kind, name)).or_else(|| {
            self.ids().find(|&id| {
                let node = self.node(id);
                node.kind == kind && node.name == name
            })
        })
    }

    pub fn edges(&self) -> &[Edge] {
        &self.edges
    }

    /// Where `from`'s edges of `kind` lead, in key order.
    pub fn outgoing(&self, from: NodeId, kind: EdgeKind) -> impl Iterator<Item = NodeId> + '_ {
        let start = self
            .edges
            .partition_point(|e| (e.from, e.kind) < (from, kind));
        self.edges[start..]
            .iter()
            .take_while(move |e| e.from == from && e.kind == kind)
            .map(|e| e.to)
    }

    /// Where the edges of `kind` that reach `to` start, in key order.
    pub fn incoming(&self, to: NodeId, kind: EdgeKind) -> impl Iterator<Item = NodeId> + '_ {
        let start = self
            .incoming
            .partition_point(|e| (e.to, e.kind) < (to, kind));
        self.incoming[start..]
            .iter()
            .take_while(move |e| e.to == to && e.kind == kind)
            .map(|e| e.from)
    }

    /// Every edge kind `from` has, each with where it leads.
    pub fn edges_from(&self, from: NodeId) -> Vec<(EdgeKind, Vec<NodeId>)> {
        let start = self.edges.partition_point(|e| e.from < from);
        let mut out: Vec<(EdgeKind, Vec<NodeId>)> = Vec::new();
        for edge in self.edges[start..].iter().take_while(|e| e.from == from) {
            match out.last_mut() {
                Some((kind, targets)) if *kind == edge.kind => targets.push(edge.to),
                _ => out.push((edge.kind, vec![edge.to])),
            }
        }
        out
    }

    /// The nodes whose parent is `parent`, in key order.
    pub fn children(&self, parent: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        let key = &self.node(parent).key;
        self.ids()
            .filter(move |&id| self.node(id).parent.as_ref() == Some(key))
    }

    /// The fields of a type, entity, variant or failure, in the order they
    /// are declared (each field's `order`), never their keys' order.
    pub fn fields(&self, parent: NodeId) -> Vec<NodeId> {
        let mut fields: Vec<NodeId> = self
            .children(parent)
            .filter(|&id| self.node(id).kind == Kind::Field)
            .collect();
        fields.sort_by_key(|&id| match &self.node(id).detail {
            Detail::Field { order, .. } => *order,
            _ => u32::MAX,
        });
        fields
    }
}

/// Collects nodes and edges in any order and builds a [`Graph`].
#[derive(Clone, Debug, Default)]
pub struct Builder {
    nodes: Vec<Node>,
    edges: Vec<(Key, EdgeKind, Key)>,
}

impl Builder {
    pub fn new() -> Builder {
        Builder::default()
    }

    /// Adds a node. When two have one key, the first added is kept: a
    /// name declared twice is an error the checker reports, and callers
    /// add nodes in file and source order.
    pub fn node(&mut self, node: Node) {
        self.nodes.push(node);
    }

    /// Adds an edge by key. An edge whose ends are not both nodes when the
    /// graph is built is dropped: it names something the checker reports
    /// as unknown.
    pub fn edge(&mut self, from: Key, kind: EdgeKind, to: Key) {
        self.edges.push((from, kind, to));
    }

    pub fn build(self) -> Graph {
        let mut nodes = self.nodes;
        // Stable, so the first node added for a key stays first.
        nodes.sort_by(|a, b| a.key.cmp(&b.key));
        nodes.dedup_by(|later, earlier| later.key == earlier.key);
        let find = |key: &Key| {
            nodes
                .binary_search_by(|node| node.key.cmp(key))
                .ok()
                .map(|i| NodeId(i as u32))
        };
        let mut edges: Vec<Edge> = self
            .edges
            .iter()
            .filter_map(|(from, kind, to)| {
                Some(Edge {
                    from: find(from)?,
                    kind: *kind,
                    to: find(to)?,
                })
            })
            .collect();
        edges.sort();
        edges.dedup();
        let mut incoming = edges.clone();
        incoming.sort_by_key(|e| (e.to, e.kind, e.from));
        Graph {
            nodes,
            edges,
            incoming,
        }
    }
}
