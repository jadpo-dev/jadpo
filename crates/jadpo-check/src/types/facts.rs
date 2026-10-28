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
use std::collections::BTreeSet;

use jadpo_graph::body::{Body, Callee, Expr, Node, SiteId};
use jadpo_std::batch::BatchOp;
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
    /// `emit`: the event type's key (area 12).
    Emits(Key),
    /// `batches.each` (area 12): a run, started once the action commits,
    /// walking a query and calling an action per item, and another once
    /// it finishes. The calls are the run's, as the system, never the
    /// action's own.
    Batch(BatchFact),
    /// `batches.status` or `batches.cancel` (area 12), which read or write
    /// the runs a store keeps.
    Runs(BatchOp),
}

/// A batch an action starts, without positions.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct BatchFact {
    /// The query `from:` walks, by name.
    pub query: Option<String>,
    /// Whether it reads a stored file's rows instead (`csv.read` and the
    /// other file helpers; chunk 6.4b), keeping its run where its actions
    /// write.
    pub file: bool,
    /// The action `each:` calls per item.
    pub each: Option<Key>,
    /// The action `finished:` calls once.
    pub finished: Option<Key>,
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

/// The fact site `site` of `body` holds, if any: an `emit` names the
/// event type its value is.
fn fact_in(body: &Body, site: &jadpo_graph::body::Site) -> Option<Fact> {
    if let Node::Stmt(jadpo_graph::body::Stmt::Emit { value: Some(value) }) = &site.node {
        return match &body.site(*value).ty {
            jadpo_graph::TypeRef::Node(key) => Some(Fact::Emits(key.clone())),
            _ => None,
        };
    }
    fact_of(&site.node)
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
            Callee::Batch(op @ (BatchOp::Status | BatchOp::Cancel)) => Fact::Runs(*op),
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

/// Where a call starting a run has `from:`, `each:` and `finished:`
/// among its arguments: `batches.each`'s first; a file helper's read has
/// its file and `as:` in place of `from:` (chunk 6.4b), and its write its
/// store, `name:` and `format:` before it (chunk 6.4b2).
pub fn run_places(op: BatchOp) -> (Option<usize>, usize, usize) {
    match op {
        BatchOp::Read(_) => (None, 2, 3),
        BatchOp::Write(_) => (Some(3), 4, 5),
        _ => (Some(0), 1, 2),
    }
}

/// The batch a call starting a run starts, and the sites inside it that
/// are the run's rather than the action's: `from:`'s query call, and the
/// action `each:` and `finished:` each call.
fn batch_of(body: &Body, args: &[Option<SiteId>], op: BatchOp) -> (BatchFact, Vec<SiteId>) {
    let node = |site: Option<SiteId>| site.map(|site| (site, &body.site(site).node));
    let mut skip = Vec::new();
    let file = matches!(op, BatchOp::Read(_));
    let mut fact = BatchFact {
        query: None,
        file,
        each: None,
        finished: None,
    };
    let (from, calls, _) = run_places(op);
    if let Some((
        site,
        Node::Expr(Expr::Call {
            callee: Callee::Query(query),
            ..
        }),
    )) = node(from.and_then(|at| args.get(at).copied().flatten()))
    {
        fact.query = Some(query.name().to_string());
        skip.push(site);
    }
    let called = |at: Option<SiteId>, skip: &mut Vec<SiteId>| match node(at) {
        Some((
            _,
            Node::Expr(Expr::Inline {
                body: Some(inner), ..
            }),
        )) => match node(Some(*inner)) {
            Some((
                site,
                Node::Expr(Expr::Call {
                    callee: Callee::Action(key),
                    ..
                }),
            )) => {
                skip.push(site);
                Some(key.clone())
            }
            _ => None,
        },
        _ => None,
    };
    fact.each = called(args.get(calls).copied().flatten(), &mut skip);
    fact.finished = called(args.get(calls + 1).copied().flatten(), &mut skip);
    (fact, skip)
}

/// Each of `body`'s facts with the site it is at, in source order: a
/// `batches.each` call's own fact in place of the run's calls inside it.
pub(super) fn facts_of(body: &Body) -> Vec<(Fact, SiteId)> {
    let mut batches: Vec<(SiteId, BatchFact)> = Vec::new();
    let mut skip: BTreeSet<SiteId> = BTreeSet::new();
    for (at, site) in body.sites.iter().enumerate() {
        if let Node::Expr(Expr::Call {
            callee: Callee::Batch(op),
            args,
            ..
        }) = &site.node
            && op.starts()
        {
            let (fact, inside) = batch_of(body, args, *op);
            skip.extend(inside);
            batches.push((SiteId(at as u32), fact));
        }
    }
    body.sites
        .iter()
        .enumerate()
        .filter_map(|(at, site)| {
            let at = SiteId(at as u32);
            if skip.contains(&at) {
                return None;
            }
            if let Some((_, fact)) = batches.iter().find(|(site, _)| *site == at) {
                return Some((Fact::Batch(fact.clone()), at));
            }
            Some((fact_in(body, site)?, at))
        })
        .collect()
}

/// Each of `body`'s facts with its site's range, in source order: the
/// same facts as [`body_facts`], for a check that reports where one is.
pub fn facts_at(body: &Body) -> Vec<(Fact, (u32, u32))> {
    facts_of(body)
        .into_iter()
        .map(|(fact, at)| {
            let site = body.site(at);
            (fact, (site.start, site.end))
        })
        .collect()
}

/// What one callable's body does past its own values, without positions.
#[salsa::tracked(returns(ref))]
pub fn body_facts<'db>(db: &'db dyn Db, project: Project, callable: Callable<'db>) -> BodyFacts {
    let checked = typed_body(db, project, callable);
    BodyFacts {
        key: checked.key.clone(),
        facts: facts_of(&checked.body)
            .into_iter()
            .map(|(fact, _)| fact)
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
    // 18, "Stable identity", amended 2026-10-16); a domain is never a library's
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
