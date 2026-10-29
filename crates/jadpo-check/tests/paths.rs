//! Path numbers (docs/compiler-runtime.md §2.3, "Bodies"; roadmap chunk
//! 9.3): each way through a typed body numbered when it is compiled, every
//! way a number of its own from 0, a `for`'s and an inline function's
//! body regions of their own.

use std::collections::BTreeSet;

use crate::support;

use jadpo_check::types::{callables_of, typed_body};
use jadpo_db::{Database, Layout};
use jadpo_graph::body::{ArmBody, Body, Expr, Node, SiteId, Stmt};
use jadpo_graph::paths::{self, Edge, Paths};

const TYPES: &str =
    "type Count = Int {}\n\ntype Shade = Enum {\n    red\n    green\n    blue\n}\n\n";

/// The typed body of the first callable in `source`, after the types
/// above.
fn body(source: &str) -> Body {
    let db = Database::default();
    let text = format!("{TYPES}{source}");
    let project = support::project(&db, Layout::Loose, &[("main.jadpo", text.as_str())]);
    let problems: Vec<String> = jadpo_check::check(&db, project)
        .iter()
        .map(|d| format!("{}: {}", d.rule.id, d.message))
        .collect();
    assert!(problems.is_empty(), "{problems:?}");
    let file = project.files(&db)[0];
    let callable = callables_of(&db, file)
        .into_iter()
        .next()
        .expect("a callable");
    Body::clone(&typed_body(&db, project, callable).body)
}

/// Every way through region 0's statements as the executors take them,
/// worked out by trying each choice rather than from the ways' counts:
/// the sum of what each way adds. A `for` is one step, left through a
/// `return` inside it when one can.
fn sums(body: &Body, numbered: &Paths) -> Vec<u64> {
    let mut ends = Vec::new();
    let mut left = Vec::new();
    walk(
        body,
        numbered,
        &body.block,
        0,
        &mut |sum| ends.push(sum),
        &mut |sum| left.push(sum),
    );
    ends.extend(left);
    ends
}

/// Walks `stmts` from `sum`: `done` for each way that reaches their end,
/// `left` for each that leaves by a `return`.
fn walk(
    body: &Body,
    numbered: &Paths,
    stmts: &[SiteId],
    sum: u64,
    done: &mut dyn FnMut(u64),
    left: &mut dyn FnMut(u64),
) {
    let Some((&first, rest)) = stmts.split_first() else {
        done(sum);
        return;
    };
    let mut on = |sum: u64, left: &mut dyn FnMut(u64)| walk(body, numbered, rest, sum, done, left);
    let add = |sum: u64, edge: Option<Edge>| sum + edge.map_or(0, |edge| edge.increment);
    match &body.site(first).node {
        Node::Stmt(Stmt::Return { .. }) => left(sum),
        Node::Stmt(Stmt::If {
            arms,
            otherwise,
            otherwise_branch,
        }) => {
            let mut ways: Vec<(u64, &[SiteId])> = arms
                .iter()
                .map(|arm| (add(sum, numbered.branch(arm.branch)), arm.then.as_slice()))
                .collect();
            ways.push((add(sum, numbered.branch(*otherwise_branch)), otherwise));
            for (sum, block) in ways {
                let mut ends = Vec::new();
                walk(body, numbered, block, sum, &mut |sum| ends.push(sum), left);
                for sum in ends {
                    on(sum, left);
                }
            }
        }
        Node::Stmt(Stmt::For { .. }) => {
            if let Some(Edge { increment, .. }) = returns_out(body, numbered, first) {
                left(sum + increment);
            }
            on(sum, left);
        }
        Node::Stmt(
            Stmt::Var {
                value: Some(value), ..
            }
            | Stmt::Expr { value: Some(value) },
        ) if matches!(body.site(*value).node, Node::Expr(Expr::Match { .. })) => {
            let Node::Expr(Expr::Match { arms, .. }) = &body.site(*value).node else {
                unreachable!()
            };
            for arm in arms {
                let sum = add(sum, numbered.branch(arm.branch));
                match &arm.body {
                    ArmBody::Block(block) => {
                        let mut ends = Vec::new();
                        walk(body, numbered, block, sum, &mut |sum| ends.push(sum), left);
                        for sum in ends {
                            on(sum, left);
                        }
                    }
                    _ => on(sum, left),
                }
            }
        }
        _ => on(sum, left),
    }
}

/// What leaving the body through a `return` inside the `for` at `site`
/// adds to region 0.
fn returns_out(body: &Body, numbered: &Paths, site: SiteId) -> Option<Edge> {
    let held = body.site(site);
    body.sites.iter().enumerate().find_map(|(at, inner)| {
        let at = SiteId(u32::try_from(at).ok()?);
        let inside = inner.start >= held.start && inner.end <= held.end && at != site;
        (inside && matches!(inner.node, Node::Stmt(Stmt::Return { .. })))
            .then(|| {
                numbered
                    .returning(at)
                    .iter()
                    .find(|edge| edge.region == 0)
                    .copied()
            })
            .flatten()
    })
}

/// Region 0's ways, each its own number from 0 to how many there are.
fn assert_numbered(body: &Body) -> Paths {
    let numbered = paths::number(body);
    let sums = sums(body, &numbered);
    let distinct: BTreeSet<u64> = sums.iter().copied().collect();
    assert_eq!(
        distinct.len(),
        sums.len(),
        "two ways share a number: {sums:?}"
    );
    assert_eq!(
        distinct,
        (0..numbered.ways()).collect::<BTreeSet<u64>>(),
        "{numbered:?}"
    );
    numbered
}

#[test]
fn a_body_without_a_choice_has_one_way() {
    let body = body("function same(count: Count) -> Count {\n    return count\n}\n");
    let numbered = assert_numbered(&body);
    assert_eq!(numbered.ways(), 1);
    assert!(numbered.branches.iter().all(Option::is_none));
    assert!(numbered.turns.is_empty());
}

#[test]
fn an_if_without_an_else_is_two_ways() {
    let body = body(
        "function capped(count: Count) -> Count {\n    var mut out: Count = count\n    if (count > 10) {\n        out = 10\n    }\n    return out\n}\n",
    );
    let numbered = assert_numbered(&body);
    assert_eq!(numbered.ways(), 2);
    // The way past the `if` comes first and adds nothing.
    assert_eq!(numbered.branches.iter().flatten().count(), 1);
}

/// Choices one after another multiply, an `else if` chain and a `match`
/// adding a way per arm, and every way still has a number of its own.
#[test]
fn ways_multiply_and_each_has_a_number_of_its_own() {
    let body = body(
        "function mixed(count: Count, shade: Shade) -> Count {\n    var mut out: Count = count\n    if (count > 10) {\n        out = 10\n    } else if (count > 5) {\n        out = 5\n    }\n    if (count == 0) {\n        out = 1\n    }\n    var picked: Count = match shade {\n        Shade.red -> 1\n        Shade.green -> 2\n        Shade.blue -> 3\n    }\n    return Count(out + picked)\n}\n",
    );
    let numbered = assert_numbered(&body);
    assert_eq!(numbered.ways(), 3 * 2 * 3);
}

/// A `return` ends its way: the ways after it are only those that didn't
/// take it.
#[test]
fn a_return_ends_its_way() {
    let body = body(
        "function early(count: Count) -> Count {\n    if (count == 0) {\n        return 0\n    }\n    var mut out: Count = count\n    if (count > 10) {\n        out = 10\n    }\n    return out\n}\n",
    );
    let numbered = assert_numbered(&body);
    assert_eq!(numbered.ways(), 1 + 2);
}

/// A `for`'s body is a region of its own, numbered for each turn, and a
/// `return` inside it leaves the body through a way of its own.
#[test]
fn a_loop_body_is_a_region_of_its_own_left_through_its_returns() {
    let body = body(
        "function first_big(counts: List<Count>) -> Count {\n    var mut total: Count = 0\n    for count in counts {\n        if (count > 3) {\n            return count\n        }\n        total = Count(total + count)\n    }\n    return total\n}\n",
    );
    let numbered = assert_numbered(&body);
    let Node::Stmt(Stmt::For { .. }) = body.site(body.block[1]).node else {
        panic!("a `for`");
    };
    let region = paths::region_of(body.block[1]);
    assert_eq!(numbered.turn(body.block[1]), Some(region));
    assert_eq!(numbered.regions[&region].ways, 2);
    // Run to its end, or left through the `return`.
    assert_eq!(numbered.ways(), 2);
    let (site, edges) = numbered.returns.iter().next().expect("a return");
    assert!(matches!(
        body.site(*site).node,
        Node::Stmt(Stmt::Return { .. })
    ));
    assert_eq!(
        edges,
        &[Edge {
            region: 0,
            increment: 1
        }]
    );
}

/// A loop whose body has one way opens no region per turn.
#[test]
fn a_loop_body_with_one_way_opens_no_turns() {
    let body = body(
        "function total(counts: List<Count>) -> Count {\n    var mut total: Count = 0\n    for count in counts {\n        total = Count(total + count)\n    }\n    return total\n}\n",
    );
    let numbered = assert_numbered(&body);
    assert!(numbered.turns.is_empty());
}

/// An inline function's body is a region of its own, keyed by its site.
#[test]
fn an_inline_function_is_a_region_of_its_own() {
    let body = body(
        "function sized(counts: List<Count>) -> List<Count> {\n    return list.map(counts, count -> match count > 3 {\n        true -> Count(1)\n        false -> Count(2)\n    })\n}\n",
    );
    let numbered = assert_numbered(&body);
    assert_eq!(numbered.ways(), 1);
    let (site, region) = numbered.turns.iter().next().expect("a turn");
    assert_eq!(*region, paths::region_of(*site));
    assert_eq!(numbered.regions[region].ways, 2);
    assert!(matches!(
        body.site(*site).node,
        Node::Expr(Expr::Match { .. })
    ));
}

/// An `and` whose right side holds a choice is a choice itself: worked out
/// or not.
#[test]
fn an_and_whose_right_side_chooses_is_a_choice() {
    let body = body(
        "function both(count: Count, shade: Shade) -> Count {\n    if (count > 1 and match shade {\n        Shade.red -> true\n        Shade.green -> false\n        Shade.blue -> count > 5\n    }) {\n        return 1\n    }\n    return 2\n}\n",
    );
    let numbered = paths::number(&body);
    // Skipped, or one of three arms; then the `if` or past it.
    assert_eq!(numbered.ways(), (1 + 3) * 2);
    assert_eq!(numbered.rights.len(), 1);
}

/// A chain of five thousand conditions is five thousand and two ways,
/// each condition's and the `else`'s.
#[test]
fn a_long_chain_has_a_way_per_arm() {
    let chain = " else if (count == 1) {\n        return count\n    }".repeat(5_000);
    let body = body(&format!(
        "function chained(count: Count) -> Count {{\n    if (count == 0) {{\n        return count\n    }}{chain} else {{\n        return count\n    }}\n}}\n"
    ));
    let numbered = assert_numbered(&body);
    assert_eq!(numbered.ways(), 5_002);
}

/// A body with more ways than a `u64` holds is cut after a statement into
/// segments, each numbered from 0.
#[test]
fn a_body_with_too_many_ways_is_cut_into_segments() {
    let checks: String = (0..70)
        .map(|n| format!("    if (count == {n}) {{\n        out = Count(out + {n})\n    }}\n"))
        .collect();
    let body = body(&format!(
        "function many(count: Count) -> Count {{\n    var mut out: Count = 0\n{checks}    return out\n}}\n"
    ));
    let numbered = paths::number(&body);
    assert!(!numbered.cuts.is_empty(), "{numbered:?}");
    assert!(numbered.ways() > 1);
    for (region, ways) in numbered.cuts.values() {
        assert_eq!(*region, 0);
        assert!(*ways > 1);
    }
    // The first segment's ways are the product of the choices before the
    // first cut.
    assert!(numbered.ways().is_power_of_two(), "{}", numbered.ways());
}
