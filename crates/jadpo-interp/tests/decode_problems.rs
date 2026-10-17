//! Area 19, "Data formats" (chunk 5.5g2), run: a failure mapped from
//! `json.read`'s `mismatch` that declares `problem: DecodeProblem`
//! receives the first problem, the field's path with list positions as
//! numbers and the rule, in its client or internal fields and in its
//! report, and a `match` arm reads it; no value from the document ever
//! reaches it.

use std::rc::Rc;

use crate::support;

use jadpo_interp::Value;
use support::{failure, main, report, text, value};

const SOURCE: &str = "type Sku = Text {\n    max_length: 20\n}\n\ntype Quantity = Int {\n    min: 1\n    max: 1000\n}\n\ntype OrderLine = Input {\n    sku: Sku\n    quantity: Quantity\n}\n\ntype OrderImport = Input {\n    reference: Sku\n    lines: List<OrderLine>\n}\n\ntype SupplierMessage = Text {\n    max_length: 100000\n}\n\ntype Shown = Text {}\n\ntype ImportUnreadable = Failure {\n    kind: FailureKind.invalid_value\n}\n\ntype ImportInvalid = Failure {\n    kind: FailureKind.invalid_value\n    client: {\n        problem: DecodeProblem\n    }\n}\n\ntype ImportRefused = Failure {\n    kind: FailureKind.invalid_value\n    client: {\n        reference: Sku\n    }\n    internal: {\n        problem: DecodeProblem\n    }\n}\n\nfunction imported(message: SupplierMessage) fails ImportUnreadable, ImportInvalid -> OrderImport {\n    return attempt json.read(message, as: OrderImport) {\n        malformed: ImportUnreadable\n        mismatch: ImportInvalid\n    }\n}\n\nfunction refused(message: SupplierMessage, reference: Sku) fails ImportUnreadable, ImportRefused -> OrderImport {\n    return attempt json.read(message, as: OrderImport) {\n        malformed: ImportUnreadable\n        invalid: ImportRefused {\n            reference: reference\n        }\n    }\n}\n\nfunction problem_shown(message: SupplierMessage) -> List<Shown> {\n    return match imported(message) {\n        success -> []\n        ImportUnreadable -> []\n        ImportInvalid(invalid) -> [Shown(invalid.problem.field), Shown(invalid.problem.rule)]\n    }\n}\n";

/// The third line's quantity is over its `max`.
const OVER: &str = r#"{"reference": "A-12", "lines": [{"sku": "rye", "quantity": 2}, {"sku": "spelt", "quantity": 1}, {"sku": "oat", "quantity": 1001}]}"#;

/// A secret-looking value that breaks `max_length`, beside one that is
/// valid.
const SECRET: &str = "sk_live_51HxQ9eSECRETv4lue";

fn problem(field: &str, rule: &str) -> Value {
    Value::record(
        "DecodeProblem",
        vec![
            (Rc::from("field"), text(field)),
            (Rc::from("rule"), text(rule)),
        ],
    )
}

#[test]
fn the_mapped_failure_receives_the_first_problem() {
    let program = main(SOURCE);
    let found = failure(&program, "imported", vec![text(OVER)]);
    assert_eq!(found.name, "ImportInvalid");
    assert_eq!(
        found.client,
        [("problem".to_string(), problem("lines.2.quantity", "max"))]
    );
    assert!(found.internal.is_empty());
    // A `match` arm reads it.
    assert_eq!(
        value(&program, "problem_shown", vec![text(OVER)]),
        Value::list(vec![text("lines.2.quantity"), text("max")])
    );
    // An unknown field, a missing one, and a document of the wrong kind,
    // whose path is empty.
    for (document, field, rule) in [
        (
            r#"{"reference": "A-12", "lines": [], "colour": "red"}"#,
            "colour",
            "unknown_field",
        ),
        (r#"{"reference": "A-12"}"#, "lines", "required"),
        ("[1, 2]", "", "type"),
    ] {
        assert_eq!(
            failure(&program, "imported", vec![text(document)]).client,
            [("problem".to_string(), problem(field, rule))],
            "{document}"
        );
    }
    // Text that isn't JSON has no problem to give.
    assert_eq!(
        failure(&program, "imported", vec![text("{")]).name,
        "ImportUnreadable"
    );
}

#[test]
fn an_internal_problem_sits_beside_the_fields_the_arm_gives() {
    let program = main(SOURCE);
    // `invalid:` covers `mismatch` alone here.
    let found = failure(&program, "refused", vec![text(OVER), text("A-12")]);
    assert_eq!(found.name, "ImportRefused");
    assert_eq!(found.client, [("reference".to_string(), text("A-12"))]);
    assert_eq!(
        found.internal,
        [("problem".to_string(), problem("lines.2.quantity", "max"))]
    );
}

#[test]
fn no_value_from_the_document_reaches_the_problem_or_the_report() {
    let program = main(SOURCE);
    let document = format!(
        r#"{{"reference": "{SECRET}", "lines": [{{"sku": "sk_test_valid", "quantity": 2}}]}}"#
    );
    let found = failure(&program, "imported", vec![text(&document)]);
    assert_eq!(
        found.client,
        [("problem".to_string(), problem("reference", "max_length"))]
    );
    assert!(!format!("{found:?}").contains("sk_"), "{found:?}");
    let shown = report(&program, "imported", vec![text(&document)]);
    assert!(
        shown.contains(
            "client: { problem: DecodeProblem { field: \"reference\", rule: \"max_length\" } }"
        ),
        "{shown}"
    );
    assert!(!shown.contains("sk_"), "{shown}");
    let shown = report(&program, "imported", vec![text(OVER)]);
    assert!(
        shown.contains(
            "client: { problem: DecodeProblem { field: \"lines.2.quantity\", rule: \"max\" } }"
        ),
        "{shown}"
    );
    assert!(!shown.contains("1001"), "{shown}");
}
