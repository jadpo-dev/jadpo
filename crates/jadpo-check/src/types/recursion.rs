//! Recursion (area 3, question 14): a function may call itself or form a
//! cycle with other functions; an action in a call cycle is an error,
//! because repeating effects has no data reason and an unbounded cycle
//! never finishes. Queries have no statement bodies, so they call nothing
//! (area 6).
//!
//! Each file's action-to-action calls are collected by name, without
//! positions, so a body edit that keeps its calls stops invalidation
//! there; the cycles are found once for the project, and each file reports
//! the calls in it that close one.

use std::collections::{BTreeMap, BTreeSet};

use jadpo_db::{Db, File, ItemKind, Project, Symbols, symbols};
use jadpo_diag::{Diagnostic, rule};
use jadpo_syntax::ast::{self, AstNode, Decl, Expr};

use crate::tree::{self, dotted, node_span};

/// One action calling another, by name.
type Call = (String, String);

/// The calls that close a cycle, each with the cycle it closes, from the
/// caller round to the caller again.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Cycles {
    pub closing: BTreeMap<Call, Vec<String>>,
}

/// The action a call names, if it names one: `reserve(...)` or
/// `inventory.reserve(...)`.
fn called_action(call: &ast::CallExpr, symbols: &Symbols) -> Option<String> {
    let callee = call.expr()?;
    let (first, rest) = dotted(&callee)?;
    let name = match rest.as_slice() {
        [] => tree::name_ref_text(&first)?,
        [member] => {
            let qualifier = tree::name_ref_text(&first)?;
            if !symbols.domains.contains(&qualifier) {
                return None;
            }
            tree::name_ref_text(member)?
        }
        _ => return None,
    };
    let is_action = symbols.declarations.get(&name).is_some_and(|found| {
        found
            .iter()
            .any(|symbol| symbol.library.is_none() && symbol.kind == ItemKind::Action)
    });
    is_action.then_some(name)
}

/// Each action declared in a file, with its body.
fn actions(source: &ast::SourceFile) -> Vec<(String, ast::Block)> {
    source
        .decls()
        .filter_map(|decl| match decl {
            Decl::ActionDecl(action) => Some((
                action.name().as_ref().and_then(tree::name_text)?,
                action.block()?,
            )),
            _ => None,
        })
        .collect()
}

fn calls_in(block: &ast::Block, symbols: &Symbols) -> Vec<(String, ast::CallExpr)> {
    block
        .syntax()
        .descendants()
        .filter_map(ast::CallExpr::cast)
        .filter_map(|call| Some((called_action(&call, symbols)?, call)))
        .collect()
}

/// The actions each action in `file` calls.
#[salsa::tracked(returns(ref))]
pub fn action_calls(db: &dyn Db, project: Project, file: File) -> BTreeSet<Call> {
    let Some(source) = tree::source(db, file) else {
        return BTreeSet::new();
    };
    let symbols = symbols(db, project);
    let mut out = BTreeSet::new();
    for (caller, block) in actions(&source) {
        for (callee, _) in calls_in(&block, symbols) {
            out.insert((caller.clone(), callee));
        }
    }
    out
}

/// The project's action cycles: one closing call for each.
#[salsa::tracked(returns(ref))]
pub fn action_cycles(db: &dyn Db, project: Project) -> Cycles {
    let mut graph: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for &file in project.files(db) {
        for (caller, callee) in action_calls(db, project, file) {
            graph
                .entry(caller.clone())
                .or_default()
                .insert(callee.clone());
        }
    }
    let mut closing = BTreeMap::new();
    let mut done: BTreeSet<String> = BTreeSet::new();
    // In name order, each action not yet in a reported cycle: the first
    // call back to it closes its cycle.
    for start in graph.keys() {
        if done.contains(start) {
            continue;
        }
        let Some(path) = path_back(&graph, start) else {
            continue;
        };
        done.extend(path.iter().cloned());
        let last = path[path.len() - 2].clone();
        closing.insert((last, start.clone()), path);
    }
    Cycles { closing }
}

/// The shortest cycle from `start` back to itself, as the names along it,
/// `start` first and last.
fn path_back(graph: &BTreeMap<String, BTreeSet<String>>, start: &str) -> Option<Vec<String>> {
    let mut previous: BTreeMap<String, String> = BTreeMap::new();
    let mut seen = BTreeSet::from([start.to_string()]);
    let mut queue = std::collections::VecDeque::from([start.to_string()]);
    while let Some(node) = queue.pop_front() {
        for next in graph.get(&node).into_iter().flatten() {
            if next == start {
                let mut path = vec![node.clone()];
                let mut current = node.clone();
                while let Some(before) = previous.get(&current) {
                    path.push(before.clone());
                    current = before.clone();
                }
                path.reverse();
                path.push(start.to_string());
                return Some(path);
            }
            if seen.insert(next.clone()) {
                previous.insert(next.clone(), node.clone());
                queue.push_back(next.clone());
            }
        }
    }
    None
}

/// The calls in `file` that close an action cycle.
#[salsa::tracked(returns(ref))]
pub fn recursion_problems(db: &dyn Db, project: Project, file: File) -> Vec<Diagnostic> {
    let cycles = action_cycles(db, project);
    if cycles.closing.is_empty() {
        return Vec::new();
    }
    let Some(source) = tree::source(db, file) else {
        return Vec::new();
    };
    let symbols = symbols(db, project);
    let path = file.path(db);
    let mut out = Vec::new();
    for (caller, block) in actions(&source) {
        let mut reported = BTreeSet::new();
        for (callee, call) in calls_in(&block, symbols) {
            let key = (caller.clone(), callee.clone());
            let Some(cycle) = cycles.closing.get(&key) else {
                continue;
            };
            if !reported.insert(key) {
                continue;
            }
            let span = call.expr().map_or_else(
                || node_span(path, call.syntax()),
                |e: Expr| node_span(path, e.syntax()),
            );
            let shown: Vec<String> = cycle.iter().map(|name| format!("`{name}`")).collect();
            let message = if cycle.len() <= 2 {
                format!("Action `{caller}` calls itself")
            } else {
                format!(
                    "Action `{caller}` is in a call cycle: {}",
                    shown.join(" → ")
                )
            };
            out.push(
                Diagnostic::new(&rule::EFFECT_RECURSIVE_ACTION, message)
                    .at(span, "calls back round")
                    .reason("Only functions may call themselves; an action in a call cycle repeats its effects with no data reason, and an unbounded one never finishes.")
                    .next("Move the repeated logic into a function, or loop over a list with `for`"),
            );
        }
    }
    out
}
