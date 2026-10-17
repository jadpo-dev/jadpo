//! What a body does beyond its own values (docs/compiler-runtime.md §2.4,
//! chunk 4.11): the records it reads with `get`, creates, changes and
//! deletes, with the fields each write sets, the queries, actions,
//! fixtures and functions it calls, and the session and key operations it
//! runs, in source order and without positions.
//!
//! It is one query per callable, read from its typed body once. Isolation
//! ([`super::loading`]), the one-store rule ([`super::stores`]), policy's
//! reach (`policy::reach`) and the lines the flow check reads under root
//! `policy { actions: }` all build on it, so a body edit that keeps these
//! facts stops invalidation here and a comment never gets past it. A check
//! that reports where a fact is written reads the sites again with
//! [`facts_at`], which keeps the same facts in the same order.

use jadpo_db::{Db, ItemKind, Project};
use jadpo_graph::Key;
use jadpo_graph::body::{Body, Callee, Expr, Node};
use jadpo_std::http::SessionOp;
use jadpo_std::write::WriteOp;

use super::body::{Callable, typed_body};
use crate::lookups::{Lookup, Name, NameView, file_at};

/// One thing a body does that reaches past its own values.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Fact {
    /// `Entity.get`: the entity's name.
    Gets(String),
    /// A query, by name.
    Queries(String),
    /// A call to an action.
    Action(Key),
    /// A call to a fixture (area 13).
    Fixture(Key),
    /// A call to a function, a library's or not.
    Function(Key),
    /// A create, update, delete or transition.
    Writes(Write),
    /// A session or key operation (area 8).
    Session(SessionOp),
}

/// One write, without positions.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Write {
    /// The entity's name.
    pub entity: String,
    pub op: WriteOp,
    /// The lifecycle transition it runs.
    pub transition: Option<Key>,
    /// The fields its `set:` gives, by name, as written.
    pub set: Vec<String>,
    /// Whether it changes a record it was given, rather than one it finds
    /// by `id`, so it rests on a copy loaded earlier.
    pub given: bool,
}

/// Everything one callable's body does past its own values.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct BodyFacts {
    /// Its key in the graph.
    pub key: Option<Key>,
    /// In source order.
    pub facts: Vec<Fact>,
}

impl BodyFacts {
    /// The key, when the callable is one of `kinds`.
    pub fn key_of(&self, kinds: &[jadpo_graph::Kind]) -> Option<&Key> {
        self.key
            .as_ref()
            .filter(|key| key.kind().is_some_and(|kind| kinds.contains(&kind)))
    }
}

/// The fact a site holds, if any.
pub fn fact_of(node: &Node) -> Option<Fact> {
    let Node::Expr(expr) = node else {
        return None;
    };
    Some(match expr {
        Expr::Call { callee, .. } => match callee {
            Callee::Entity { entity, operation } if operation == "get" => {
                Fact::Gets(entity.name().to_string())
            }
            Callee::Query(query) => Fact::Queries(query.name().to_string()),
            Callee::Action(key) => Fact::Action(key.clone()),
            Callee::Fixture(key) => Fact::Fixture(key.clone()),
            Callee::Function(key) | Callee::LibraryFunction(key) => Fact::Function(key.clone()),
            Callee::Session(op) => Fact::Session(*op),
            _ => return None,
        },
        Expr::Write {
            entity,
            op,
            transition,
            target,
            by_id,
            set,
            ..
        } => Fact::Writes(Write {
            entity: entity.name().to_string(),
            op: *op,
            transition: transition.clone(),
            set: set
                .iter()
                .map(|(key, _)| {
                    key.name()
                        .rsplit('.')
                        .next()
                        .unwrap_or_default()
                        .to_string()
                })
                .collect(),
            given: target.is_some() && !by_id,
        }),
        _ => return None,
    })
}

/// Each of `body`'s facts with its site's range, in source order: the
/// same facts as [`body_facts`], for a check that reports where one is.
pub fn facts_at(body: &Body) -> Vec<(Fact, (u32, u32))> {
    body.sites
        .iter()
        .filter_map(|site| Some((fact_of(&site.node)?, (site.start, site.end))))
        .collect()
}

/// What one callable's body does past its own values, without positions.
#[salsa::tracked(returns(ref))]
pub fn body_facts<'db>(db: &'db dyn Db, project: Project, callable: Callable<'db>) -> BodyFacts {
    let checked = typed_body(db, project, callable);
    BodyFacts {
        key: checked.key.clone(),
        facts: checked
            .body
            .sites
            .iter()
            .filter_map(|site| fact_of(&site.node))
            .collect(),
    }
}

/// The callables declared as `name`, in path order: an action, fixture or
/// function, or with `library.function`, a library's function. Read by
/// name, so the caller depends on that name's declarations alone.
pub fn named<'db>(db: &'db dyn Db, project: Project, name: &str) -> Vec<Callable<'db>> {
    let lookup = Lookup::new(db, project);
    let (library, own) = match name.split_once('.') {
        Some((library, function)) => (Some(library), function),
        None => (None, name),
    };
    // A function's key carries its domain, `billing.format_address` (area
    // 18, question 8, amended 2026-10-16); a domain is never a library's
    // name.
    let (domain, library) = match library {
        Some(domain) if lookup.is_domain(domain) => (Some(domain), None),
        other => (None, other),
    };
    let mut out = Vec::new();
    for symbol in lookup.facts(own).declarations.iter() {
        if domain.is_some() && symbol.ownership.domain() != domain {
            continue;
        }
        let function = match (&symbol.library, library) {
            (None, None)
                if matches!(
                    symbol.kind,
                    ItemKind::Action | ItemKind::Fixture | ItemKind::Function
                ) =>
            {
                None
            }
            (Some((held, index)), Some(library)) if held == library => Some(*index),
            _ => continue,
        };
        if let Some(file) = file_at(db, project, Name::new(db, symbol.path.clone())) {
            out.push(Callable::new(db, file, symbol.index, function));
        }
    }
    out
}

/// The callable a call to `key` runs: of those declared with its name,
/// the last whose key it is.
pub fn resolve<'db>(db: &'db dyn Db, project: Project, key: &Key) -> Option<Callable<'db>> {
    named(db, project, key.name())
        .into_iter()
        .rfind(|callable| body_facts(db, project, *callable).key.as_ref() == Some(key))
}
