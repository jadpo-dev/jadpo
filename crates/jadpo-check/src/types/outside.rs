//! Outside calls, and the writes that would hold a transaction open over
//! them (area 6, "Several outside calls"; chunk 7.4b). No transaction is
//! open while an outside call is out, so an action's writes come after its
//! last call; a write placed before a later call is
//! `data.write_before_call`, reported at the write and naming the call it
//! must follow, so checked code never reaches the runtime's
//! `call_in_transaction` refusal, which stays as the guard.
//!
//! What each action and fixture does, itself and through every action it
//! calls, is worked out once for the project ([`outside_calls`]): whether
//! it makes an outside call (a provider's operation, a store's `put` or
//! `read`) and whether it writes (an entity's write, a session or key
//! operation that changes one, or starting or cancelling a run, each of
//! which keeps a row). The plan reads it too: an item of a run whose
//! `each:` action makes outside calls is a batch of its own (area 12, "How
//! a run goes").
//!
//! Each body is then walked in the order it runs ([`Walk`]): every way
//! through an `if` or a `match` on its own, a `return`, `reject` or
//! `propagate` ending its way, and a loop's body twice, so a write in one
//! turn meets a call in the next. A run's `each:` and `finished:` are the
//! run's, never the action starting it, as everywhere else (area 12).

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use jadpo_db::{Db, File, Project};
use jadpo_diag::{Diagnostic, Span, rule};
use jadpo_graph::body::{ArmBody, Body, Callee, Expr, Node, SiteId, Stmt};
use jadpo_graph::{Key, Kind};
use jadpo_std::batch::BatchOp;
use jadpo_syntax::ast::{self, AstNode};

use super::body::{callables_of, typed_body};
use crate::tree::{self, node_span};

/// What one action or fixture does that the order of its calls and writes
/// rests on, itself or through any action it calls: each as written where
/// it is first done, `couriers.bookings.create` or `Shipment.update`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Outside {
    /// The outside call it makes.
    pub calls: Option<String>,
    /// The write it makes.
    pub writes: Option<String>,
}

/// What one action or fixture does itself, and the actions and fixtures
/// it calls, in the order it calls them.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Own {
    calls: Option<String>,
    writes: Option<String>,
    callees: Vec<Key>,
}

/// What a site does that matters here.
pub(super) enum Does<'b> {
    /// An outside call.
    Calls,
    /// A write.
    Writes,
    /// A call of an action or fixture, which joins the caller's
    /// transaction and does what it does.
    Joins(&'b Key),
    Nothing,
}

pub(super) fn does(node: &Node) -> Does<'_> {
    match node {
        Node::Expr(Expr::Write { .. }) => Does::Writes,
        Node::Expr(Expr::Call { callee, .. }) => match callee {
            Callee::Provider { .. }
            | Callee::Files { .. }
            | Callee::Generate { .. }
            | Callee::Stream { .. } => Does::Calls,
            Callee::Action(key)
            | Callee::Fixture(key)
            | Callee::PackageAction { action: key, .. } => Does::Joins(key),
            // An entity's other operations are writes, built or not.
            Callee::Entity { operation, .. } if operation != "get" => Does::Writes,
            Callee::Session(op) if !op.reads_only() => Does::Writes,
            // Starting a run keeps its row, and a cancel one of its own.
            Callee::Batch(op) if *op != BatchOp::Status => Does::Writes,
            _ => Does::Nothing,
        },
        _ => Does::Nothing,
    }
}

/// The sites a site runs first, in the order it runs them, but an inline
/// function's body, which a run or a standard operation runs, never the
/// site itself.
pub(super) fn children(node: &Node) -> Vec<SiteId> {
    let mut out = Vec::new();
    match node {
        Node::Stmt(stmt) => match stmt {
            Stmt::Var { value, .. }
            | Stmt::Assign { value, .. }
            | Stmt::Return { value }
            | Stmt::Emit { value }
            | Stmt::Assert { value, .. }
            | Stmt::Expr { value } => out.extend(*value),
            Stmt::If {
                arms, otherwise, ..
            } => {
                for arm in arms {
                    out.extend(arm.condition);
                    out.extend(arm.then.iter().copied());
                }
                out.extend(otherwise.iter().copied());
            }
            Stmt::For { list, body, .. } => {
                out.extend(*list);
                out.extend(body.iter().copied());
            }
        },
        Node::Expr(expr) => match expr {
            Expr::Field { receiver, .. } => out.push(*receiver),
            Expr::Call { args, .. } => out.extend(args.iter().flatten().copied()),
            Expr::Build { fields, .. } | Expr::Reject { fields, .. } => {
                out.extend(fields.iter().map(|(_, site)| *site));
            }
            Expr::List { items } => out.extend(items.iter().copied()),
            Expr::Binary { lhs, rhs, .. } => out.extend([*lhs, *rhs]),
            Expr::Unary { operand, .. } => out.push(*operand),
            Expr::Match { subject, arms, .. } => {
                out.extend(*subject);
                for arm in arms {
                    match &arm.body {
                        ArmBody::Value(site) => out.push(*site),
                        ArmBody::Block(block) => out.extend(block.iter().copied()),
                        ArmBody::Empty => {}
                    }
                }
            }
            Expr::Attempt { inner, .. } => out.extend(*inner),
            Expr::Write {
                target,
                set,
                patch,
                when_supplied,
                ..
            } => {
                out.extend(*target);
                out.extend(set.iter().map(|(_, site)| *site));
                out.extend(*patch);
                for (_, fields) in when_supplied {
                    out.extend(fields.iter().map(|(_, site)| *site));
                }
            }
            Expr::Request {
                params,
                input,
                idempotency_key,
                caller,
                ..
            } => {
                out.extend(params.iter().copied());
                out.extend(*input);
                out.extend(*idempotency_key);
                out.extend(*caller);
            }
            Expr::Inline { .. }
            | Expr::Literal(_)
            | Expr::Local(_)
            | Expr::Stored { .. }
            | Expr::Propagate { .. }
            | Expr::Config { .. }
            | Expr::Constant { .. }
            | Expr::Now
            | Expr::Unchecked => {}
        },
    }
    out
}

/// Where each call's and write's name is written in a file, by its site's
/// range: a call's callee (`couriers.bookings.create`, `add_shelf`), a
/// write's entity and operation (`Shipment.update`).
fn heads(source: &ast::SourceFile, path: &str) -> BTreeMap<(u32, u32), (u32, u32)> {
    let mut out = BTreeMap::new();
    let mut keep = |node: &jadpo_syntax::SyntaxNode, head: Option<ast::Expr>| {
        let (Some(head), at) = (head, node_span(path, node)) else {
            return;
        };
        let head = node_span(path, head.syntax());
        out.entry((at.start, at.end))
            .or_insert((head.start, head.end));
    };
    for node in source.syntax().descendants() {
        if let Some(call) = ast::CallExpr::cast(node.clone()) {
            keep(&node, call.expr());
        } else if let Some(record) = ast::RecordExpr::cast(node.clone()) {
            let head = match record.expr() {
                Some(ast::Expr::CallExpr(call)) => call.expr(),
                other => other,
            };
            keep(&node, head);
        }
    }
    out
}

/// A body's sites' names, as its file writes them.
pub(super) struct Names<'f> {
    path: &'f str,
    text: &'f str,
    heads: BTreeMap<(u32, u32), (u32, u32)>,
}

impl<'f> Names<'f> {
    pub(super) fn of(db: &'f dyn Db, file: File) -> Option<Names<'f>> {
        let source = tree::source(db, file)?;
        let path = file.path(db).as_str();
        Some(Names {
            path,
            text: file.text(db).as_str(),
            heads: heads(&source, path),
        })
    }

    /// Where the site's name is written: its head, or the site itself.
    pub(super) fn span(&self, body: &Body, id: SiteId) -> Span {
        let site = body.site(id);
        let (start, end) = self
            .heads
            .get(&(site.start, site.end))
            .copied()
            .unwrap_or((site.start, site.end));
        Span::new(self.path, start, end)
    }

    /// The site's name, as written.
    pub(super) fn name(&self, body: &Body, id: SiteId) -> String {
        let span = self.span(body, id);
        let written = self
            .text
            .get(span.start as usize..span.end as usize)
            .unwrap_or_default();
        // A site with no head of its own, up to its arguments or block.
        written
            .split(['(', '{'])
            .next()
            .unwrap_or_default()
            .trim()
            .to_string()
    }
}

/// The actions and fixtures `file` declares, each with its typed body.
pub(super) fn bodies(
    db: &dyn Db,
    project: Project,
    file: File,
) -> Vec<(Key, std::sync::Arc<Body>)> {
    callables_of(db, file)
        .into_iter()
        .filter_map(|callable| {
            let checked = typed_body(db, project, callable);
            let key = checked
                .key
                .as_ref()
                .filter(|key| matches!(key.kind(), Some(Kind::Action | Kind::Fixture)))?;
            Some((key.clone(), std::sync::Arc::clone(&checked.body)))
        })
        .collect()
}

/// What each action and fixture in `file` does itself, by key.
#[salsa::tracked(returns(ref))]
pub fn file_outside(db: &dyn Db, project: Project, file: File) -> BTreeMap<Key, Own> {
    let mut out = BTreeMap::new();
    let Some(names) = Names::of(db, file) else {
        return out;
    };
    for (key, body) in bodies(db, project, file) {
        let mut own = Own::default();
        let mut queue: VecDeque<SiteId> = body.block.iter().copied().collect();
        let mut seen: BTreeSet<SiteId> = BTreeSet::new();
        // Every site the body runs, in the order its sites are numbered.
        while let Some(id) = queue.pop_front() {
            if !seen.insert(id) {
                continue;
            }
            queue.extend(children(&body.site(id).node));
        }
        for id in seen {
            match does(&body.site(id).node) {
                Does::Calls => {
                    own.calls.get_or_insert_with(|| names.name(&body, id));
                }
                Does::Writes => {
                    own.writes.get_or_insert_with(|| names.name(&body, id));
                }
                Does::Joins(callee) => {
                    if !own.callees.contains(callee) {
                        own.callees.push(callee.clone());
                    }
                }
                Does::Nothing => {}
            }
        }
        out.insert(key, own);
    }
    out
}

/// What each action and fixture does, itself or through the actions it
/// calls, by key: the outside call it makes and the write, each the one it
/// makes itself, or else the nearest an action it calls makes.
#[salsa::tracked(returns(ref))]
pub fn outside_calls(db: &dyn Db, project: Project) -> BTreeMap<Key, Outside> {
    let mut own: BTreeMap<&Key, &Own> = BTreeMap::new();
    for &file in project.files(db) {
        for (key, found) in file_outside(db, project, file) {
            own.entry(key).or_insert(found);
        }
    }
    let nearest = |start: &Key, part: &dyn Fn(&Own) -> Option<String>| -> Option<String> {
        let mut seen = BTreeSet::new();
        let mut queue = VecDeque::from([start]);
        while let Some(key) = queue.pop_front() {
            if !seen.insert(key) {
                continue;
            }
            let Some(found) = own.get(key) else {
                continue;
            };
            if let Some(done) = part(found) {
                return Some(done);
            }
            queue.extend(found.callees.iter());
        }
        None
    };
    own.keys()
        .map(|&key| {
            (
                key.clone(),
                Outside {
                    calls: nearest(key, &|own| own.calls.clone()),
                    writes: nearest(key, &|own| own.writes.clone()),
                },
            )
        })
        .collect()
}

/// One way through a body: whether it is still running, and the writes
/// that may have been made along it.
#[derive(Clone, Debug, Default)]
struct Way {
    live: bool,
    written: BTreeSet<SiteId>,
}

impl Way {
    fn start() -> Way {
        Way {
            live: true,
            written: BTreeSet::new(),
        }
    }

    /// A way that ended: a `return`, a `reject` or a `propagate`.
    fn ended() -> Way {
        Way::default()
    }

    /// Where two ways meet again.
    fn join(mut self, other: Way) -> Way {
        match (self.live, other.live) {
            (false, _) => other,
            (_, false) => self,
            _ => {
                self.written.extend(other.written);
                self
            }
        }
    }
}

/// A body walked in the order it runs, finding each write some way runs
/// before an outside call.
struct Walk<'a> {
    body: &'a Body,
    outside: &'a BTreeMap<Key, Outside>,
    /// Each write, with the calls that may come after it.
    found: BTreeMap<SiteId, BTreeSet<SiteId>>,
}

impl Walk<'_> {
    fn block(&mut self, block: &[SiteId], mut way: Way) -> Way {
        for &id in block {
            if !way.live {
                break;
            }
            way = self.site(id, way);
        }
        way
    }

    fn all(&mut self, sites: impl IntoIterator<Item = SiteId>, mut way: Way) -> Way {
        for id in sites {
            way = self.site(id, way);
        }
        way
    }

    fn site(&mut self, id: SiteId, way: Way) -> Way {
        if !way.live {
            return way;
        }
        let body = self.body;
        let node = &body.site(id).node;
        match node {
            Node::Stmt(Stmt::Return { value }) => {
                self.all(*value, way);
                Way::ended()
            }
            Node::Stmt(Stmt::If {
                arms, otherwise, ..
            }) => {
                let mut way = way;
                let mut out = Way::ended();
                for arm in arms {
                    way = self.all(arm.condition, way);
                    out = out.join(self.block(&arm.then, way.clone()));
                }
                out.join(self.block(otherwise, way))
            }
            // A loop's body runs any number of times: a write in one turn
            // comes before a call in the next.
            Node::Stmt(Stmt::For { list, body, .. }) => {
                let entry = self.all(*list, way);
                let once = self.block(body, entry.clone());
                let twice = self.block(body, entry.clone().join(once));
                entry.join(twice)
            }
            Node::Expr(Expr::Match { subject, arms, .. }) if !arms.is_empty() => {
                let way = self.all(*subject, way);
                let mut out = Way::ended();
                for arm in arms {
                    out = out.join(match &arm.body {
                        ArmBody::Value(site) => self.site(*site, way.clone()),
                        ArmBody::Block(block) => self.block(block, way.clone()),
                        ArmBody::Empty => way.clone(),
                    });
                }
                out
            }
            Node::Expr(Expr::Reject { .. } | Expr::Propagate { .. }) => {
                self.all(children(node), way);
                Way::ended()
            }
            _ => {
                let mut way = self.all(children(node), way);
                match does(node) {
                    Does::Calls => self.call(id, &way),
                    Does::Writes => {
                        way.written.insert(id);
                    }
                    Does::Joins(callee) => {
                        let outside = self.outside.get(callee).cloned().unwrap_or_default();
                        if outside.calls.is_some() {
                            self.call(id, &way);
                        }
                        if outside.writes.is_some() {
                            way.written.insert(id);
                        }
                    }
                    Does::Nothing => {}
                }
                way
            }
        }
    }

    /// An outside call at `id`: after each write `way` may have made.
    fn call(&mut self, id: SiteId, way: &Way) {
        for write in &way.written {
            self.found.entry(*write).or_default().insert(id);
        }
    }
}

/// Each write in `file`'s actions and fixtures that some way runs before
/// an outside call: one error at the write, naming the last call it comes
/// before.
#[salsa::tracked(returns(ref))]
pub fn call_order_problems(db: &dyn Db, project: Project, file: File) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    let own = file_outside(db, project, file);
    if own
        .values()
        .all(|own| own.calls.is_none() && own.callees.is_empty())
    {
        return out;
    }
    let outside = outside_calls(db, project);
    let Some(names) = Names::of(db, file) else {
        return out;
    };
    for (_, body) in bodies(db, project, file) {
        let mut walk = Walk {
            body: &body,
            outside,
            found: BTreeMap::new(),
        };
        walk.block(&body.block, Way::start());
        let mut found: Vec<(SiteId, SiteId)> = walk
            .found
            .iter()
            .filter_map(|(write, calls)| {
                let last = calls
                    .iter()
                    .max_by_key(|call| (body.site(**call).start, call.0))?;
                Some((*write, *last))
            })
            .collect();
        found.sort_by_key(|(write, _)| (body.site(*write).start, write.0));
        for (write, call) in found {
            out.push(problem(&names, &body, outside, write, call));
        }
    }
    out
}

/// The error for `write`, which comes before `call`.
fn problem(
    names: &Names<'_>,
    body: &Body,
    outside: &BTreeMap<Key, Outside>,
    write: SiteId,
    call: SiteId,
) -> Diagnostic {
    let written = names.name(body, write);
    let called = names.name(body, call);
    // An action called that makes the call, and the call it makes.
    let through = match does(&body.site(call).node) {
        Does::Joins(key) => outside.get(key).and_then(|outside| outside.calls.clone()),
        _ => None,
    };
    let (message, label, next) = match &through {
        Some(made) => (
            format!("`{written}` writes before `{called}` calls `{made}`"),
            format!("calls `{made}`"),
            format!("Move `{written}` after the call to `{called}`"),
        ),
        None => (
            format!("`{written}` writes before the call to `{called}`"),
            "the call it must follow".to_string(),
            format!("Move `{written}` after `{called}`"),
        ),
    };
    let made = through.as_deref().unwrap_or(&called);
    Diagnostic::new(&rule::DATA_WRITE_BEFORE_CALL, message)
        .at(names.span(body, write), "writes here")
        .also(names.span(body, call), label)
        .reason(format!(
            "No transaction is open while an outside call is out, so an action writes only after its last call; run as written, `{made}` would be refused."
        ))
        .next(next)
}
