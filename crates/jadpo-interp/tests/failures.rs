//! Area 4, failures: the page's examples, run. What a failure carries
//! (code, kind, fields, where it was raised, the path it travelled, its
//! cause) and the faults the runtime raises itself.

mod support;

use jadpo_interp::{FaultKind, Value};
use support::{failure, fault, int, main, program, text, value, where_fault};

/// Each frame's `file:line`, innermost first.
fn lines(program: &jadpo_interp::Program, frames: &[jadpo_interp::Frame]) -> Vec<String> {
    frames
        .iter()
        .filter_map(|frame| program.places().place(*frame))
        .map(|place| format!("{}:{}", place.file, place.line))
        .collect()
}

#[test]
fn reject_raises_a_declared_failure_with_its_fields() {
    let program = program(&[
        (
            "types.jadpo",
            "type Quantity = Int {\n    min: 1\n    max: 1000\n}\n\nentity Product {\n    id: Uuid\n    available: Quantity\n}\n\ntype Reservation = Object {\n    product_id: Product.id\n    quantity: Quantity\n}\n",
        ),
        (
            "failures.jadpo",
            "type QuantityUnavailable = Failure {\n    kind: FailureKind.conflict\n    message: \"The requested quantity is unavailable.\"\n    client: {\n        available: Quantity\n    }\n    internal: {\n        product_id: Product.id\n        requested: Quantity\n    }\n}\n",
        ),
        (
            "main.jadpo",
            "action reserve(product: Product, requested: Quantity) fails QuantityUnavailable -> Reservation {\n    if (requested > product.available) {\n        reject QuantityUnavailable {\n            available: product.available\n            product_id: product.id\n            requested: requested\n        }\n    }\n    return Reservation {\n        product_id: product.id\n        quantity: requested\n    }\n}\n",
        ),
    ]);
    let id = Value::uuid("0192f3a4-3b5e-7c1a-9d2e-4f6a8b0c1d2e");
    let product = program
        .object("Product", vec![("id", id.clone()), ("available", int(5))])
        .expect("Product");
    assert_eq!(
        value(&program, "reserve", vec![product.clone(), int(2)]).to_string(),
        "Reservation { product_id: \"0192f3a4-3b5e-7c1a-9d2e-4f6a8b0c1d2e\", quantity: 2 }"
    );
    let unavailable = failure(&program, "reserve", vec![product, int(12)]);
    assert_eq!(unavailable.name, "QuantityUnavailable");
    assert_eq!(unavailable.code, "quantity_unavailable");
    assert_eq!(unavailable.kind.as_deref(), Some("conflict"));
    assert_eq!(unavailable.status, Some(409));
    assert_eq!(
        unavailable.message.as_deref(),
        Some("The requested quantity is unavailable.")
    );
    assert_eq!(unavailable.client, [("available".to_string(), int(5))]);
    assert_eq!(
        unavailable.internal,
        [
            ("product_id".to_string(), id),
            ("requested".to_string(), int(12))
        ]
    );
    assert_eq!(lines(&program, &unavailable.frames), ["main.jadpo:3"]);
    assert_eq!(unavailable.cause, None);
}

#[test]
fn a_construction_the_ranges_prove_safe_never_fails_and_one_they_dont_can() {
    // Question 6.
    let program = program(&[
        (
            "types.jadpo",
            "type Pence = Int {\n    min: 0\n}\n\ntype Quantity = Int {\n    min: 1\n    max: 1000\n}\n\ntype QuantityTooLarge = Failure {\n    kind: FailureKind.invalid_value\n}\n",
        ),
        (
            "main.jadpo",
            "function line_total(price: Pence, quantity: Quantity) -> Pence {\n    var total: Pence = Pence(price * quantity)  // both at least 0, Pence has no maximum\n    return total\n}\n\nfunction doubled(quantity: Quantity) fails QuantityTooLarge -> Quantity {\n    var result: Quantity = attempt Quantity(quantity * 2) {  // can exceed 1000\n        max: QuantityTooLarge\n    }\n    return result\n}\n",
        ),
    ]);
    assert_eq!(value(&program, "line_total", vec![int(0), int(7)]), int(0));
    assert_eq!(value(&program, "doubled", vec![int(500)]), int(1000));
    let too_large = failure(&program, "doubled", vec![int(501)]);
    assert_eq!(too_large.name, "QuantityTooLarge");
    assert_eq!(too_large.broken.as_deref(), Some("`max: 1000`"));
    assert_eq!(lines(&program, &too_large.frames), ["main.jadpo:7"]);
}

#[test]
fn each_broken_rule_raises_its_arms_failure() {
    // Area 2, "Construction failures by rule": the rule that broke picks
    // the arm, and `invalid:` covers every rule without its own.
    let program = main(
        "type Count = Int {}\n\ntype CakeCount = Int {\n    min: 1\n    max: 12\n}\n\ntype Label = Text {\n    max_length: 40\n}\n\ntype Code = Text {\n    min_length: 3\n    max_length: 8\n    pattern: \"[A-Z]+\"\n}\n\ntype OrderEmpty = Failure {\n    kind: FailureKind.invalid_value\n}\n\ntype OrderTooLarge = Failure {\n    kind: FailureKind.invalid_value\n}\n\ntype CodeTooLong = Failure {\n    kind: FailureKind.invalid_value\n}\n\ntype CodeRejected = Failure {\n    kind: FailureKind.invalid_value\n}\n\nfunction cakes(n: Count) fails OrderEmpty, OrderTooLarge -> CakeCount {\n    var count: CakeCount = attempt CakeCount(n) {\n        min: OrderEmpty\n        max: OrderTooLarge\n    }\n    return count\n}\n\nfunction code(label: Label) fails CodeTooLong, CodeRejected -> Code {\n    var code: Code = attempt Code(label) {\n        max_length: CodeTooLong\n        invalid: CodeRejected\n    }\n    return code\n}\n",
    );
    assert_eq!(value(&program, "cakes", vec![int(6)]), int(6));
    assert_eq!(failure(&program, "cakes", vec![int(0)]).name, "OrderEmpty");
    let large = failure(&program, "cakes", vec![int(13)]);
    assert_eq!(large.name, "OrderTooLarge");
    assert_eq!(large.broken.as_deref(), Some("`max: 12`"));
    assert_eq!(value(&program, "code", vec![text("ABCD")]), text("ABCD"));
    assert_eq!(
        failure(&program, "code", vec![text("ABCDEFGHIJ")]).name,
        "CodeTooLong"
    );
    // `min_length` and `pattern` have no arm of their own.
    assert_eq!(failure(&program, "code", vec![text("AB")]).name, "CodeRejected");
    let rejected = failure(&program, "code", vec![text("abcd")]);
    assert_eq!(rejected.name, "CodeRejected");
    assert_eq!(rejected.broken.as_deref(), Some("`pattern: \"[A-Z]+\"`"));
}

#[test]
fn a_construction_mapping_gives_its_failure_fields() {
    // An arm names its failure as `reject` does, fields and all
    // (question 7: every mapping is `condition: Failure`).
    let program = main(
        "type Quantity = Int {\n    min: 1\n    max: 1000\n}\n\ntype QuantityTooLarge = Failure {\n    kind: FailureKind.invalid_value\n    client: {\n        largest: Quantity\n    }\n    internal: {\n        asked: Quantity\n    }\n}\n\nfunction doubled(quantity: Quantity) fails QuantityTooLarge -> Quantity {\n    var result: Quantity = attempt Quantity(quantity * 2) {\n        max: QuantityTooLarge {\n            largest: 1000\n            asked: quantity\n        }\n    }\n    return result\n}\n",
    );
    assert_eq!(value(&program, "doubled", vec![int(500)]), int(1000));
    let too_large = failure(&program, "doubled", vec![int(501)]);
    assert_eq!(too_large.client, [("largest".to_string(), int(1000))]);
    assert_eq!(too_large.internal, [("asked".to_string(), int(501))]);
    assert_eq!(too_large.broken.as_deref(), Some("`max: 1000`"));
}

const LOOKUP: &str = "type LookupCode = Text {}\n\ntype LookupResult = Text {}\n\ntype LookupMissing = Failure {\n    kind: FailureKind.not_found\n}\n\ntype LookupRefused = Failure {\n    kind: FailureKind.rejected\n    internal: {\n        reason: LookupResult\n    }\n}\n\ntype LookupRejected = Failure {\n    kind: FailureKind.rejected\n}\n\nfunction lookup(key: LookupCode) fails LookupMissing, LookupRefused -> LookupResult {\n    if (key == \"missing\") {\n        reject LookupMissing\n    }\n    if (key == \"refused\") {\n        reject LookupRefused {\n            reason: \"on the deny list\"\n        }\n    }\n    return \"found\"\n}\n";

#[test]
fn a_mapped_failure_keeps_the_original_as_its_cause() {
    // Question 8.
    let program = program(&[
        ("lookup.jadpo", LOOKUP),
        (
            "main.jadpo",
            "function lookup_or_default(key: LookupCode) fails LookupRejected -> LookupResult {\n    return match lookup(key) {\n        success(value) -> value\n        LookupMissing -> \"not found\"\n        LookupRefused -> reject LookupRejected\n    }\n}\n",
        ),
    ]);
    let rejected = failure(&program, "lookup_or_default", vec![text("refused")]);
    assert_eq!(rejected.name, "LookupRejected");
    assert_eq!(lines(&program, &rejected.frames), ["main.jadpo:5"]);
    let cause = rejected.cause.as_deref().expect("a cause");
    assert_eq!(cause.name, "LookupRefused");
    assert_eq!(
        cause.internal,
        [("reason".to_string(), text("on the deny list"))]
    );
    assert_eq!(
        lines(&program, &cause.frames),
        ["lookup.jadpo:25", "main.jadpo:2"]
    );
}

#[test]
fn propagate_passes_the_handled_failure_on_unchanged() {
    let program = program(&[
        ("lookup.jadpo", LOOKUP),
        (
            "main.jadpo",
            "function lookup_or_refused(key: LookupCode) fails LookupMissing -> LookupResult {\n    return match lookup(key) {\n        success(value) -> value\n        LookupMissing -> propagate\n        LookupRefused -> \"refused\"\n    }\n}\n",
        ),
    ]);
    assert_eq!(
        value(&program, "lookup_or_refused", vec![text("refused")]),
        text("refused")
    );
    let missing = failure(&program, "lookup_or_refused", vec![text("missing")]);
    assert_eq!(missing.name, "LookupMissing");
    assert_eq!(missing.cause, None);
    assert_eq!(
        lines(&program, &missing.frames),
        ["lookup.jadpo:22", "main.jadpo:2"],
        "still where it was raised"
    );
}

#[test]
fn a_failure_arm_can_read_the_failures_fields() {
    let program = program(&[
        ("lookup.jadpo", LOOKUP),
        (
            "main.jadpo",
            "function refusal(key: LookupCode) fails LookupMissing -> LookupResult {\n    return match lookup(key) {\n        success(value) -> value\n        LookupMissing -> propagate\n        LookupRefused(refused) -> refused.reason\n    }\n}\n",
        ),
    ]);
    assert_eq!(
        value(&program, "refusal", vec![text("refused")]),
        text("on the deny list")
    );
}

#[test]
fn attempt_passes_failures_on_through_every_caller() {
    let program = main(
        "type Pence = Int {\n    min: 0\n}\n\ntype Quantity = Int {\n    min: 1\n    max: 1000\n}\n\ntype QuantityTooLarge = Failure {\n    kind: FailureKind.invalid_value\n}\n\nfunction line_total(price: Pence, quantity: Quantity) -> Pence {\n    return Pence(price * quantity)\n}\n\nfunction bulk_quantity(quantity: Quantity) fails QuantityTooLarge -> Quantity {\n    if (quantity > 100) {\n        reject QuantityTooLarge\n    }\n    return quantity\n}\n\naction price_line(price: Pence, quantity: Quantity) fails QuantityTooLarge -> Pence {\n    var checked: Quantity = attempt bulk_quantity(quantity)\n    return line_total(price, checked)\n}\n",
    );
    assert_eq!(
        value(&program, "price_line", vec![int(250), int(4)]),
        int(1000)
    );
    let too_large = failure(&program, "price_line", vec![int(250), int(101)]);
    assert_eq!(
        lines(&program, &too_large.frames),
        ["main.jadpo:20", "main.jadpo:26"]
    );
    assert_eq!(
        too_large.trace(program.places()),
        ["function bulk_quantity", "action price_line"],
        "the path it travelled, as area 4's log lists it"
    );
}

#[test]
fn overflow_is_a_defect_with_the_frames_that_reached_it() {
    // Question 9.
    let program = main(
        "type Pence = Int {\n    min: 0\n}\n\ntype Quantity = Int {\n    min: 1\n    max: 1000\n}\n\nfunction line_total(price: Pence, quantity: Quantity) -> Pence {\n    var total: Pence = Pence(price * quantity)\n    return total\n}\n\naction price_line(price: Pence, quantity: Quantity) -> Pence {\n    var line: Pence = line_total(price, quantity)\n    return line\n}\n",
    );
    let overflow = fault(&program, "price_line", vec![int(i64::MAX / 2), int(3)]);
    assert_eq!(
        where_fault(&program, &overflow),
        (
            FaultKind::IntOverflow,
            vec!["main.jadpo:11".to_string(), "main.jadpo:16".to_string()]
        )
    );
    assert_eq!(overflow.kind.classification(), "int_overflow");
    assert_eq!(
        overflow.stack(program.places()),
        [
            "main.jadpo:11  function line_total",
            "main.jadpo:16  action price_line"
        ],
        "as area 17's log prints them"
    );
}

#[test]
fn a_failure_reaching_a_test_names_itself() {
    let program = main(
        "type Count = Int {\n    min: 0\n}\n\ntype TooBig = Failure {\n    kind: FailureKind.rejected\n    message: \"Too big.\"\n}\n\naction check_size(n: Count) fails TooBig -> Count {\n    if (n > 10) {\n        reject TooBig\n    }\n    return n\n}\n",
    );
    let too_big = failure(&program, "check_size", vec![int(11)]);
    assert_eq!(too_big.kind.as_deref(), Some("rejected"));
    assert_eq!(too_big.status, Some(422));
}
