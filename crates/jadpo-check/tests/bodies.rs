//! Typed bodies (docs/compiler-runtime.md §2.3): what the check decided,
//! in the form both executors walk.

use crate::support;

use jadpo_check::types::{callables_of, typed_body};
use jadpo_db::{Database, Layout};
use jadpo_graph::body::{
    ArmBody, BinaryOp, Body, Callee, Expr, Fallible, Literal, LocalKind, Node, Pattern, RuleArm,
    SiteId, Stmt,
};
use jadpo_graph::{Key, Kind, TypeRef};
use jadpo_std::{Builtin, Op};

const TYPES: &str = "type Coins = Int {\n    min: 0\n}\n\ntype Quantity = Int {\n    min: 1\n    max: 1000\n}\n\ntype Shade = Enum {\n    plain\n    tinted: {\n        level: Coins\n    }\n}\n\ntype Line = Object {\n    price: Coins\n    quantity: Quantity\n}\n\ntype TooMany = Failure {\n    kind: FailureKind.invalid_value\n    client: {\n        limit: Coins\n    }\n}\n\n";

/// The typed bodies of `source`'s callables, in source order.
fn bodies(source: &str) -> Vec<(Option<Key>, Body)> {
    let db = Database::default();
    let text = format!("{TYPES}{source}");
    let project = support::project(&db, Layout::Loose, &[("main.jadpo", text.as_str())]);
    let problems: Vec<&str> = jadpo_check::check(&db, project)
        .iter()
        .map(|d| d.rule.id)
        .collect();
    assert!(problems.is_empty(), "{problems:?}");
    let file = project.files(&db)[0];
    callables_of(&db, file)
        .into_iter()
        .map(|callable| {
            let checked = typed_body(&db, project, callable);
            (checked.key.clone(), Body::clone(&checked.body))
        })
        .collect()
}

fn expr(body: &Body, id: SiteId) -> &Expr {
    match &body.site(id).node {
        Node::Expr(expr) => expr,
        Node::Stmt(stmt) => panic!("a statement: {stmt:?}"),
    }
}

fn stmt(body: &Body, id: SiteId) -> &Stmt {
    match &body.site(id).node {
        Node::Stmt(stmt) => stmt,
        Node::Expr(expr) => panic!("an expression: {expr:?}"),
    }
}

fn key(kind: Kind, name: &str) -> Key {
    Key::new(kind, name)
}

#[test]
fn sites_are_numbered_in_source_order_with_types_and_literal_targets() {
    let found = bodies(
        "function total(line: Line) -> Coins {\n    var sum: Coins = Coins(line.price * line.quantity)\n    return sum\n}\n",
    );
    let (callable, body) = &found[0];
    assert_eq!(callable.as_ref(), Some(&key(Kind::Function, "total")));
    assert_eq!(body.params.len(), 1);
    assert_eq!(body.local(body.params[0]).kind, LocalKind::Param);
    let starts: Vec<u32> = body.sites.iter().map(|site| site.start).collect();
    let mut sorted = starts.clone();
    sorted.sort();
    assert_eq!(starts, sorted, "source order");

    let Stmt::Var {
        local: Some(sum),
        value: Some(value),
    } = stmt(body, body.block[0])
    else {
        panic!("{:?}", body.block);
    };
    assert_eq!(body.local(*sum).name, "sum");
    assert_eq!(body.local(*sum).ty, TypeRef::Node(key(Kind::Type, "Coins")));
    let Expr::Call {
        callee: Callee::Construct(target),
        args,
        fallible,
    } = expr(body, *value)
    else {
        panic!("{:?}", expr(body, *value));
    };
    assert_eq!(*target, TypeRef::Node(key(Kind::Type, "Coins")));
    // `price * quantity` is never negative: building a `Coins` can't fail.
    assert_eq!(*fallible, Fallible::Never);
    let Some(Some(product)) = args.first() else {
        panic!("{args:?}");
    };
    let Expr::Binary {
        op: BinaryOp::Multiply,
        lhs,
        ..
    } = expr(body, *product)
    else {
        panic!();
    };
    assert_eq!(body.site(*product).ty, TypeRef::Builtin(Builtin::Int));
    let Expr::Field { field, .. } = expr(body, *lhs) else {
        panic!();
    };
    assert_eq!(*field, key(Kind::Field, "Line.price"));
}

#[test]
fn calls_reach_what_the_check_resolved() {
    let found = bodies(
        "function double(n: Coins) -> Coins {\n    return Coins(n * 2)\n}\n\nfunction prices(lines: List<Line>) -> List<Coins> {\n    var doubled: List<Coins> = list.map(lines, line -> double(line.price))\n    return doubled\n}\n",
    );
    let (_, body) = &found[1];
    let Stmt::Var {
        value: Some(value), ..
    } = stmt(body, body.block[0])
    else {
        panic!();
    };
    let Expr::Call {
        callee: Callee::Std(Op::ListMap),
        args,
        fallible: Fallible::Never,
    } = expr(body, *value)
    else {
        panic!("{:?}", expr(body, *value));
    };
    assert_eq!(args.len(), 2, "in the table's parameter order");
    let Some(Some(each)) = args.get(1) else {
        panic!();
    };
    let Expr::Inline {
        params,
        body: Some(inline),
    } = expr(body, *each)
    else {
        panic!("{:?}", expr(body, *each));
    };
    let [param] = params.as_slice() else {
        panic!("{params:?}");
    };
    assert_eq!(
        body.local(*param).ty,
        TypeRef::Node(key(Kind::Type, "Line"))
    );
    assert!(matches!(
        expr(body, *inline),
        Expr::Call {
            callee: Callee::Function(called),
            ..
        } if *called == key(Kind::Function, "double")
    ));
}

/// An `else if` chain is one flat list of arms, however long (Phase 1
/// review, finding 18): each condition is a branch, and so is the `else`.
#[test]
fn an_else_if_chain_is_one_flat_if() {
    let chain = " else if (n == 1) {\n        return n\n    }".repeat(5_000);
    let found = bodies(&format!(
        "function f(n: Coins) -> Coins {{\n    if (n == 0) {{\n        return n\n    }}{chain} else {{\n        return n\n    }}\n}}\n"
    ));
    let (_, body) = &found[0];
    let Stmt::If {
        arms, otherwise, ..
    } = stmt(body, body.block[0])
    else {
        panic!();
    };
    assert_eq!(arms.len(), 5_001);
    assert_eq!(otherwise.len(), 1);
    assert_eq!(body.branches.len(), 5_002);
}

#[test]
fn constructions_say_which_rules_break_and_their_failures() {
    let found = bodies(
        "type Refused = Failure {\n    kind: FailureKind.invalid_value\n}\n\nfunction cap(n: Coins) fails TooMany, Refused -> Quantity {\n    if (n > 1000) {\n        reject TooMany {\n            limit: 1000\n        }\n    }\n    return attempt Quantity(n) {\n        invalid: Refused\n    }\n}\n",
    );
    let (_, body) = &found[0];
    assert_eq!(
        body.failures,
        [key(Kind::Type, "Refused"), key(Kind::Type, "TooMany")]
    );
    assert!(!body.open);
    let Stmt::If {
        arms,
        otherwise,
        otherwise_branch,
    } = stmt(body, body.block[0])
    else {
        panic!();
    };
    let [arm] = arms.as_slice() else {
        panic!("one condition");
    };
    let then = &arm.then;
    assert_eq!(body.branches.len(), 2);
    assert_ne!(arm.branch, *otherwise_branch);
    assert!(
        otherwise.is_empty(),
        "no `else`, but a branch past the `if`"
    );
    let Stmt::Expr {
        value: Some(reject),
    } = stmt(body, then[0])
    else {
        panic!();
    };
    let Expr::Reject {
        failure: Some(failure),
        fields,
    } = expr(body, *reject)
    else {
        panic!();
    };
    assert_eq!(*failure, key(Kind::Type, "TooMany"));
    assert_eq!(fields[0].0, key(Kind::Field, "TooMany.limit"));
    assert_eq!(
        body.site(fields[0].1).ty,
        TypeRef::Field(key(Kind::Field, "TooMany.limit")),
        "a literal has the type it goes into"
    );
    assert!(matches!(
        expr(body, fields[0].1),
        Expr::Literal(Literal::Number { decimal: false, .. })
    ));
    let Stmt::Return {
        value: Some(attempt),
    } = stmt(body, body.block[1])
    else {
        panic!();
    };
    let Expr::Attempt {
        inner: Some(inner),
        arms,
    } = expr(body, *attempt)
    else {
        panic!();
    };
    assert_eq!(
        *arms,
        [RuleArm {
            rule: None,
            named: None,
            failure: key(Kind::Type, "Refused"),
            fields: Vec::new(),
            problem: None,
        }],
        "`invalid:` covers what can break"
    );
    // The `if` before it rules out a value above 1000, so only `min` can
    // break (area 2, "Conditions as proof"), and the body records the
    // comparison that proves `max`.
    assert_eq!(
        *expr(body, *inner),
        Expr::Call {
            callee: Callee::Construct(TypeRef::Node(key(Kind::Type, "Quantity"))),
            args: vec![Some(SiteId(inner.0 + 1))],
            fallible: Fallible::Construction {
                rules: vec![jadpo_std::RuleKey::Min],
                named: Vec::new(),
            },
        }
    );
    let Some(condition) = arm.condition else {
        panic!("a condition");
    };
    assert_eq!(
        body.proofs,
        [jadpo_graph::body::Proof {
            site: *inner,
            by: vec![condition],
        }]
    );
}

#[test]
fn matches_bind_locals_and_number_their_arms() {
    let found = bodies(
        "function level(shade: Shade) -> Coins {\n    return match (shade) {\n        Shade.plain -> 0\n        Shade.tinted(tint) -> tint.level\n    }\n}\n",
    );
    let (_, body) = &found[0];
    let Stmt::Return { value: Some(value) } = stmt(body, body.block[0]) else {
        panic!();
    };
    let Expr::Match {
        outcome: false,
        arms,
        ..
    } = expr(body, *value)
    else {
        panic!();
    };
    assert_eq!(
        arms[0].pattern,
        Pattern::Variant(key(Kind::Variant, "Shade.plain"))
    );
    assert_eq!(
        arms[1].pattern,
        Pattern::Variant(key(Kind::Variant, "Shade.tinted"))
    );
    let binding = arms[1].binding.expect("a binding");
    assert_eq!(body.local(binding).kind, LocalKind::Binding);
    let ArmBody::Value(read) = &arms[1].body else {
        panic!();
    };
    assert!(matches!(
        expr(body, *read),
        Expr::Field { field, .. } if *field == key(Kind::Field, "Shade.tinted.level")
    ));
    assert_eq!(body.branches.len(), 2);
}

#[test]
fn a_bodys_names_are_resolved_in_its_own_walk() {
    // Chunk 2.1: `resolve_file` resolves declarations; each body's names
    // are resolved by the walk that types it, so its name errors and its
    // unused variables come from one walk over one set of scopes.
    let db = Database::default();
    let text = "type Count = Int {}\n\nfunction tally(Total: Count) -> Count {\n    var spare: Cuont = 1\n    return missing(spare)\n}\n\nfunction other(n: Cuont) -> Count {\n    return 1\n}\n";
    let project = support::project(&db, Layout::Loose, &[("main.jadpo", text)]);
    let file = project.files(&db)[0];
    let rules = |problems: &[jadpo_diag::Diagnostic]| {
        let mut found: Vec<&str> = problems.iter().map(|d| d.rule.id).collect();
        found.sort_unstable();
        found
    };
    let callables = callables_of(&db, file);
    assert_eq!(
        rules(&typed_body(&db, project, callables[0]).problems),
        [
            "semantic.name_case",
            "semantic.unknown_callee",
            "semantic.unknown_name",
        ],
        "the parameter's case, the variable's type and the call, from the body's walk"
    );
    assert_eq!(
        rules(jadpo_check::resolve_file(&db, project, file)),
        ["semantic.unknown_name"],
        "only `other`'s signature, a declaration"
    );
}

#[test]
fn sites_that_can_pause_are_effects_and_calls_into_actions() {
    let found = bodies(
        "type At = Instant {}\n\naction stamp() -> At {\n    return At(clock.now)\n}\n\naction bump(n: Coins) -> Coins {\n    return Coins(n + 1)\n}\n\naction twice(n: Coins) -> Coins {\n    var once: Coins = bump(n)\n    return bump(once)\n}\n\nfunction plain(n: Coins) -> Coins {\n    return Coins(n + 1)\n}\n",
    );
    let pausing = |name: &str| -> (bool, Vec<String>) {
        let (_, body) = found
            .iter()
            .find(|(key, _)| key.as_ref().is_some_and(|key| key.name() == name))
            .unwrap_or_else(|| panic!("{name}"));
        let sites = body
            .sites
            .iter()
            .filter(|site| site.pauses)
            .map(|site| match &site.node {
                Node::Expr(Expr::Call { callee, .. }) => format!("call {callee:?}"),
                Node::Expr(Expr::Now) => "effect".to_string(),
                other => format!("{other:?}"),
            })
            .collect();
        (body.pauses, sites)
    };
    assert_eq!(pausing("stamp"), (true, vec!["effect".to_string()]));
    let (pauses, sites) = pausing("twice");
    assert!(pauses);
    assert_eq!(sites.len(), 2, "both calls to `bump`: {sites:?}");
    assert!(sites.iter().all(|site| site.starts_with("call Action")));
    assert_eq!(
        pausing("bump"),
        (false, Vec::new()),
        "computing never pauses"
    );
    assert_eq!(pausing("plain"), (false, Vec::new()));
}
