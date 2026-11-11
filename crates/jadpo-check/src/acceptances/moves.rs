//! What the before side's domains and the names under them are called now
//! (area 18, "Stable identity"): a domain renamed, split or merged moves
//! what its records name, so the before side's records are read in today's
//! names before they are compared, and a move reads as a move, never as
//! something removed and something added.
//!
//! A domain is followed through its key: the before side's registry gives
//! its born name, and today's graph the domain with that key. A name under
//! it (`planning.TodoAdded`, `planning.Todo.title`) is followed through the
//! key of what it names: a type, function or constant carries its domain
//! in its key, and anything else is unique across the project, so today's
//! graph says which domain it is in.

use std::collections::BTreeSet;

use jadpo_graph::{Graph, Kind, Registry};

/// The before side's domains, and what today's graph knows them by.
#[derive(Debug)]
pub struct Moves<'a> {
    before: Registry,
    domains: &'a BTreeSet<String>,
    graph: &'a Graph,
}

impl<'a> Moves<'a> {
    /// The moves between the before side, whose registry is `identities`'
    /// text and whose domains are `domains`, and today's `graph`.
    pub fn new(
        identities: Option<&str>,
        domains: &'a BTreeSet<String>,
        graph: &'a Graph,
    ) -> Moves<'a> {
        let lines = identities
            .and_then(|text| jadpo_db::parse_identities(text).ok())
            .unwrap_or_default();
        Moves {
            before: Registry::from_lines(&lines),
            domains,
            graph,
        }
    }

    /// The domain's born name on the before side.
    pub fn born(&self, domain: &str) -> String {
        self.before.born(Kind::Domain, domain, None)
    }

    /// What the before side's domain `domain` is called now; `None` when
    /// it is gone, merged into another.
    pub fn domain_now(&self, domain: &str) -> Option<String> {
        let key = self.before.key(Kind::Domain, domain, None);
        self.graph
            .get(&key)
            .filter(|node| node.kind == Kind::Domain)
            .map(|node| node.name.clone())
    }

    /// `subject`, a before side's dotted name under a domain
    /// (`planning.Todo.title`), as today names it (`plans.Todo.title`); a
    /// name under no domain of the before side is as it was.
    pub fn now(&self, subject: &str) -> String {
        let Some((domain, rest)) = subject.split_once('.') else {
            return subject.to_string();
        };
        if !self.domains.contains(domain) {
            return subject.to_string();
        }
        let first = rest.split('.').next().unwrap_or(rest);
        let tail = &rest[first.len()..];
        // What carries its domain in its key.
        for kind in [Kind::Type, Kind::Function, Kind::Constant] {
            let key = self.before.key(kind, &format!("{domain}.{first}"), None);
            if let Some(node) = self.graph.get(&key)
                && let Some(now) = &node.domain
            {
                return format!("{now}.{}{tail}", node.name);
            }
        }
        // A name unique across the project.
        let found: BTreeSet<&str> = self
            .graph
            .nodes()
            .iter()
            .filter(|node| {
                node.name == first
                    && node.parent.is_none()
                    && !matches!(node.kind, Kind::Domain | Kind::File | Kind::Test)
            })
            .filter_map(|node| node.domain.as_deref())
            .collect();
        if found.len() == 1
            && let Some(now) = found.first()
        {
            return format!("{now}.{rest}");
        }
        match self.domain_now(domain) {
            Some(now) => format!("{now}.{rest}"),
            None => subject.to_string(),
        }
    }

    /// A record's text with every name under a before side's domain given
    /// as today names it, word by word.
    pub fn text_now(&self, text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        for line in text.split_inclusive('\n') {
            if line.starts_with("//") {
                out.push_str(line);
                continue;
            }
            let mut word = String::new();
            for c in line.chars() {
                if c.is_alphanumeric() || c == '_' || c == '.' {
                    word.push(c);
                } else {
                    out.push_str(&self.now(&word));
                    word.clear();
                    out.push(c);
                }
            }
            out.push_str(&self.now(&word));
        }
        out
    }
}
