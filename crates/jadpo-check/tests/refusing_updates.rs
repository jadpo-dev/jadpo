//! A one-step update that refuses (area 6; chunk Q2b): a construction an
//! update by `id`'s `set:` works out from stored fields maps the rules it
//! can break, `Stock(Product.stock - ordered) { min: OutOfStock }`, each
//! to a `conflict` failure the write can fail with, and the statement's
//! `WHERE` checks them. A rule left unmapped, one the database can't check
//! as Jadpo does, an arm for a rule that can't break and a failure of
//! another kind are errors; the action loads no record, so it runs at read
//! committed; and the arms reach the graph after the write's `conflict:`
//! arms, each naming its construction and rule.

use crate::support;

use jadpo_check::types::{callables_of, isolation, typed_body};
use jadpo_db::{Database, Layout};
use jadpo_graph::body::{Callee, Expr, Node};
use jadpo_std::{Isolation, RuleKey};

const NAMES: &str = "type Stock = Int {\n    min: 0\n}\n\ntype Shelf = Int {\n    min: 0\n    max: 1000\n}\n\ntype Note = Text {\n    max_length: 20\n}\n\ntype Code = Text {\n    pattern: \"[A-Z]+\"\n}\n\ntype Balance = Money {\n    currency: Currency.gbp\n    min: 0\n}\n\ntype Wait = Duration {\n    min: 0s\n}\n\ntype Count = Int {\n    min: 0\n}\n\ntype OutOfStock = Failure {\n    kind: FailureKind.conflict\n}\n\ntype ShelfFull = Failure {\n    kind: FailureKind.conflict\n}\n\ntype TooShort = Failure {\n    kind: FailureKind.invalid_value\n}\n\ntype Refused = Failure {\n    kind: FailureKind.conflict\n    internal: {\n        product_id: Uuid\n    }\n}\n\ntype ProductNotFound = Failure {\n    kind: FailureKind.not_found\n}\n\nentity Product {\n    id: Uuid\n    stock: Stock\n    shelf: Shelf\n    note: Note\n    code: Code\n    balance: Balance\n    wait: Wait\n    visits: Count\n}\n\n";

fn project(source: &str) -> (Database, jadpo_db::Project) {
    let db = Database::default();
    let project = support::project(
        &db,
        Layout::Loose,
        &[("main.jadpo", &format!("{NAMES}{source}"))],
    );
    (db, project)
}

fn rules(source: &str) -> Vec<String> {
    let (db, project) = project(source);
    jadpo_check::check(&db, project)
        .iter()
        .map(|d| d.rule.id.to_string())
        .collect()
}

/// `take`, an update by `id` of a product setting `set`, failing with
/// `fails`, its parameters `params`.
fn take(params: &str, fails: &str, set: &str) -> String {
    format!(
        "action take(product_id: Product.id{params}) fails {fails} -> Product {{\n    return attempt Product.update(product_id) {{\n        set: {{\n{set}        }}\n    }}\n}}\n"
    )
}

const NONE: &[&str] = &[];

#[test]
fn a_construction_over_stored_fields_maps_the_rules_it_can_break() {
    // The stock can fall below its `min: 0`, which the arm maps.
    assert_eq!(
        rules(&take(
            ", ordered: Stock",
            "ProductNotFound, OutOfStock",
            "            stock: Stock(Product.stock - ordered) {\n                min: OutOfStock\n            }\n",
        )),
        NONE
    );
    // `invalid:` covers every rule without an arm of its own.
    assert_eq!(
        rules(&take(
            ", more: Shelf",
            "ProductNotFound, ShelfFull",
            "            shelf: Shelf(Product.shelf + more) {\n                invalid: ShelfFull\n            }\n",
        )),
        NONE,
        "invalid: covers the rest"
    );
    // A failure's fields, from the action's parameters.
    assert_eq!(
        rules(&take(
            ", ordered: Stock",
            "ProductNotFound, Refused",
            "            stock: Stock(Product.stock - ordered) {\n                min: Refused {\n                    product_id: product_id\n                }\n            }\n",
        )),
        NONE,
        "an arm with fields"
    );
    // Text's lengths, an amount's and a duration's bounds.
    assert_eq!(
        rules(&take(
            ", more: Note",
            "ProductNotFound, OutOfStock",
            "            note: Note(Product.note + more) {\n                max_length: OutOfStock\n            }\n",
        )),
        NONE,
        "text's length"
    );
    assert_eq!(
        rules(&take(
            ", paid: Balance",
            "ProductNotFound, OutOfStock",
            "            balance: Balance(Product.balance - paid) {\n                min: OutOfStock\n            }\n",
        )),
        NONE,
        "an amount's minor units"
    );
    assert_eq!(
        rules(&take(
            ", spent: Wait",
            "ProductNotFound, OutOfStock",
            "            wait: Wait(Product.wait - spent) {\n                min: OutOfStock\n            }\n",
        )),
        NONE,
        "a duration"
    );
}

#[test]
fn what_the_arms_map_is_what_the_update_fails_with() {
    // The mapped failure leaves through the update's `attempt`, so `fails`
    // lists it.
    assert_eq!(
        rules(&take(
            ", ordered: Stock",
            "ProductNotFound",
            "            stock: Stock(Product.stock - ordered) {\n                min: OutOfStock\n            }\n",
        )),
        ["failure.undeclared_propagation"]
    );
    // An update not under `attempt` can fail with it.
    assert_eq!(
        rules(
            "action take(product_id: Product.id, ordered: Stock) fails ProductNotFound, OutOfStock -> Product {\n    return Product.update(product_id) {\n        set: {\n            stock: Stock(Product.stock - ordered) {\n                min: OutOfStock\n            }\n        }\n    }\n}\n"
        )
        .first()
        .map(String::as_str),
        Some("failure.attempt_required")
    );
}

#[test]
fn every_rule_that_can_break_is_mapped_and_checked_by_the_database() {
    // Only `min:` is mapped; the value can pass `max: 1000` too.
    assert_eq!(
        rules(&take(
            ", more: Shelf, less: Shelf",
            "ProductNotFound, ShelfFull",
            "            shelf: Shelf(Product.shelf + more - less) {\n                min: ShelfFull\n            }\n",
        )),
        ["data.stored_value_unproven"]
    );
    // Without a block, as before.
    assert_eq!(
        rules(&take(
            ", ordered: Stock",
            "ProductNotFound",
            "            stock: Stock(Product.stock - ordered)\n",
        )),
        ["data.stored_value_unproven"]
    );
    // PostgreSQL's regular expressions aren't Jadpo's.
    assert_eq!(
        rules(&take(
            ", more: Code",
            "ProductNotFound, OutOfStock",
            "            code: Code(Product.code + more) {\n                pattern: OutOfStock\n            }\n",
        )),
        ["data.stored_value_unproven"]
    );
}

#[test]
fn an_arm_maps_a_rule_that_can_break_to_a_conflict() {
    // A count of at least 0 plus 1 is at least 0.
    assert_eq!(
        rules(&take(
            "",
            "ProductNotFound",
            "            visits: Count(Product.visits + 1) {\n                min: OutOfStock\n            }\n",
        )),
        ["failure.rule_cannot_break"]
    );
    // What refuses it is the stored value, not the request.
    assert_eq!(
        rules(&take(
            ", ordered: Stock",
            "ProductNotFound, TooShort",
            "            stock: Stock(Product.stock - ordered) {\n                min: TooShort\n            }\n",
        )),
        ["failure.mapping_kind"]
    );
    assert_eq!(
        rules(&take(
            ", ordered: Stock",
            "ProductNotFound, OutOfStock",
            "            stock: Stock(Product.stock - ordered) {\n                minimum: OutOfStock\n            }\n",
        )),
        ["failure.unknown_mapping"]
    );
    assert_eq!(
        rules(&take(
            ", ordered: Stock",
            "ProductNotFound, OutOfStock",
            "            stock: Stock(Product.stock - ordered) {\n                min: OutOfStock\n                min: OutOfStock\n            }\n",
        )),
        ["failure.duplicate_mapping"]
    );
    assert_eq!(
        rules(&take(
            ", ordered: Stock",
            "ProductNotFound, OutOfStock",
            "            stock: Stock(Product.stock - ordered) {\n                min: OutOfStock\n                invalid: OutOfStock\n            }\n",
        )),
        ["failure.invalid_covers_nothing"]
    );
}

#[test]
fn a_refusing_update_loads_nothing_and_lists_its_arms_after_its_conflicts() {
    let (db, project) = project(&take(
        ", ordered: Stock, more: Shelf, less: Shelf",
        "ProductNotFound, OutOfStock, ShelfFull",
        "            stock: Stock(Product.stock - ordered) {\n                min: OutOfStock\n            }\n            shelf: Shelf(Product.shelf + more - less) {\n                max: ShelfFull\n                min: OutOfStock\n            }\n",
    ));
    assert_eq!(
        jadpo_check::check(&db, project)
            .iter()
            .map(|d| d.rule.id.to_string())
            .collect::<Vec<_>>(),
        NONE
    );
    // It loads nothing, so it runs at read committed: buyers racing on a
    // product don't rerun each other.
    let decided: Vec<(String, Isolation)> = isolation(&db, project)
        .iter()
        .map(|(key, decided)| (key.as_str().to_string(), decided.isolation))
        .collect();
    assert_eq!(
        decided,
        [("action take".to_string(), Isolation::ReadCommitted)]
    );
    let file = project.files(&db)[0];
    let bodies: Vec<_> = callables_of(&db, file)
        .into_iter()
        .map(|callable| typed_body(&db, project, callable).body.clone())
        .collect();
    let (body, conflicts) = bodies
        .iter()
        .find_map(|body| {
            body.sites.iter().find_map(|site| match &site.node {
                Node::Expr(Expr::Write { conflicts, .. }) => Some((body, conflicts)),
                _ => None,
            })
        })
        .expect("a write");
    let arms: Vec<(String, Option<RuleKey>, String)> = conflicts
        .iter()
        .map(|arm| {
            let stored = arm.stored.as_ref().expect("a stored value's rule");
            (
                arm.rule.name().to_string(),
                stored.rule,
                arm.failure.name().to_string(),
            )
        })
        .collect();
    assert_eq!(
        arms,
        [
            (
                "Product.stock".to_string(),
                Some(RuleKey::Min),
                "OutOfStock".to_string()
            ),
            (
                "Product.shelf".to_string(),
                Some(RuleKey::Max),
                "ShelfFull".to_string()
            ),
            (
                "Product.shelf".to_string(),
                Some(RuleKey::Min),
                "OutOfStock".to_string()
            ),
        ]
    );
    // Each arm names the construction it follows.
    let constructions: Vec<_> = conflicts
        .iter()
        .filter_map(|arm| arm.stored.as_ref().map(|stored| stored.construction))
        .collect();
    assert_eq!(constructions[1], constructions[2]);
    assert_ne!(constructions[0], constructions[1]);
    for site in constructions {
        assert!(matches!(
            body.site(site).node,
            Node::Expr(Expr::Call {
                callee: Callee::Construct(_),
                ..
            })
        ));
    }
}
