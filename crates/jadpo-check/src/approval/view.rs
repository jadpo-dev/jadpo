//! A manifest read back for comparing (area 18, "Comparing two builds"):
//! its nodes by id, with their edges both ways. Both sides of a review are
//! read as manifests, the before side's from its cache in
//! `build/before/<commit>/`, so a review compares exactly what each build
//! wrote.

use std::collections::BTreeMap;

use jadpo_graph::json::Value;

/// One node of a manifest.
#[derive(Clone, Debug)]
pub struct Node<'m> {
    pub id: &'m str,
    pub kind: &'m str,
    pub name: &'m str,
    pub value: &'m Value,
}

impl<'m> Node<'m> {
    pub fn text(&self, key: &str) -> Option<&'m str> {
        self.value.get(key).and_then(Value::as_str)
    }

    pub fn file(&self) -> Option<&'m str> {
        self.text("file")
    }

    pub fn domain(&self) -> Option<&'m str> {
        self.text("domain")
    }

    pub fn digest(&self) -> Option<&'m str> {
        self.text("digest")
    }

    /// Its first and last lines.
    pub fn lines(&self) -> Option<(i64, i64)> {
        let lines = self.value.get("lines")?.as_array()?;
        match lines {
            [Value::Int(first), Value::Int(last)] => Some((*first, *last)),
            _ => None,
        }
    }

    /// Where it starts: `domains/lending/actions/borrow.jadpo` and its
    /// first line.
    pub fn place(&self) -> Option<(String, i64)> {
        Some((self.file()?.to_string(), self.lines()?.0))
    }

    /// The ids its edges of `kind` lead to.
    pub fn edges(&self, kind: &str) -> Vec<&'m str> {
        self.value
            .get(kind)
            .and_then(Value::as_array)
            .map(|items| items.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default()
    }

    /// Its name as the app's acceptances and reports say it: a
    /// callable's or an entity's with its domain (`lending.borrow`).
    pub fn shown(&self) -> String {
        match (self.kind, self.domain()) {
            ("action" | "query" | "function" | "fixture" | "constant" | "entity", Some(domain)) => {
                format!("{domain}.{}", self.name)
            }
            _ => self.name.to_string(),
        }
    }
}

/// A manifest's nodes by id, and each node's incoming edges by kind.
#[derive(Debug, Default)]
pub struct View<'m> {
    pub nodes: BTreeMap<&'m str, Node<'m>>,
    /// By the node an edge reaches: each edge's kind and where it starts.
    incoming: BTreeMap<&'m str, Vec<(&'static str, &'m str)>>,
    /// How many errors the check that wrote it found.
    pub errors: i64,
}

/// The edge kinds a node's lists are written under (area 18's manifest).
const EDGES: &[&str] = &[
    "calls",
    "reads",
    "writes",
    "emits",
    "subscribes",
    "runs",
    "raises",
    "references",
    "includes",
    "grants",
    "scopes",
    "uses",
    "provides",
];

impl<'m> View<'m> {
    pub fn of(manifest: &'m Value) -> View<'m> {
        let mut view = View {
            errors: match manifest.get("errors") {
                Some(Value::Int(errors)) => *errors,
                _ => 0,
            },
            ..View::default()
        };
        for value in manifest
            .get("nodes")
            .and_then(Value::as_array)
            .unwrap_or_default()
        {
            let (Some(id), Some(kind)) = (
                value.get("id").and_then(Value::as_str),
                value.get("kind").and_then(Value::as_str),
            ) else {
                continue;
            };
            let name = value.get("name").and_then(Value::as_str).unwrap_or(id);
            view.nodes.insert(
                id,
                Node {
                    id,
                    kind,
                    name,
                    value,
                },
            );
        }
        for node in view.nodes.values() {
            for edge in EDGES {
                for to in node.edges(edge) {
                    view.incoming.entry(to).or_default().push((edge, node.id));
                }
            }
        }
        view
    }

    pub fn get(&self, id: &str) -> Option<&Node<'m>> {
        self.nodes.get(id)
    }

    /// The ids of the nodes whose edges of `kind` reach `to`.
    pub fn incoming(&self, to: &str, kind: &str) -> Vec<&'m str> {
        self.incoming
            .get(to)
            .map(|edges| {
                edges
                    .iter()
                    .filter(|(edge, _)| *edge == kind)
                    .map(|(_, from)| *from)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The nodes of `kind`, in id order.
    pub fn of_kind(&self, kind: &str) -> Vec<&Node<'m>> {
        self.nodes
            .values()
            .filter(|node| node.kind == kind)
            .collect()
    }

    /// The node of `kind` named `name` in `domain`, or in any domain when
    /// none is given.
    pub fn find(&self, kind: &str, domain: Option<&str>, name: &str) -> Option<&Node<'m>> {
        let named: Vec<&Node<'m>> = self
            .nodes
            .values()
            .filter(|node| node.kind == kind && node.name == name)
            .collect();
        named
            .iter()
            .find(|node| domain.is_none_or(|domain| node.domain() == Some(domain)))
            .or(named.first())
            .copied()
    }

    /// A type by the name a signature or route writes, `List<BookView>`
    /// read as `BookView`.
    pub fn type_named(&self, written: &str, domain: Option<&str>) -> Option<&Node<'m>> {
        self.find("type", domain, unwrapped(written))
    }
}

/// The type a written type holds: `List<BookView>`, `Page<T>` and `T?`
/// read as the type inside.
pub fn unwrapped(written: &str) -> &str {
    let mut text = written.trim().trim_end_matches('?');
    while let Some(open) = text.find('<') {
        let inner = &text[open + 1..];
        text = inner.strip_suffix('>').unwrap_or(inner);
        // `Map<K, V>` holds its values.
        if let Some((_, value)) = text.rsplit_once(',') {
            text = value.trim();
        }
    }
    text.trim_end_matches('?')
}

#[cfg(test)]
mod tests {
    use super::unwrapped;

    #[test]
    fn a_written_type_is_read_as_the_type_it_holds() {
        assert_eq!(unwrapped("List<BookView>"), "BookView");
        assert_eq!(unwrapped("Page<LoanView>?"), "LoanView");
        assert_eq!(unwrapped("StockBook"), "StockBook");
    }
}
