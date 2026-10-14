//! Conditions as proof (area 2, "Construction failures by rule": "An `if`
//! condition counts as proof"; chunk 2.6). A comparison in an `if`
//! narrows the range of the number it compares inside the branch where it
//! holds and, when the other way always leaves, after the `if`; a
//! construction those ranges prove takes no `attempt`, and each call site
//! that flips says so with its own error. Nothing here may prove a value
//! that can break its type's rules at runtime.

mod support;

use jadpo_db::{Database, Layout};
use support::{project, rules};

const CLEAN: &[String] = &[];

/// A cake count of 1 to 12, anything else, and the failures for both ends.
const TYPES: &str = "type Count = Int {}\n\ntype Limit = Count {\n    max: 12\n}\n\ntype CakeCount = Int {\n    min: 1\n    max: 12\n}\n\ntype Stock = Int {\n    min: 0\n    max: 500\n}\n\ntype People = Int {\n    min: 1\n}\n\ntype Amount = Decimal {\n    scale: 4\n}\n\ntype Price = Decimal {\n    scale: 2\n    min: 0.00\n    max: 10.00\n}\n\ntype Flag = Bool {}\n\ntype Line = Object {\n    quantity: Count\n}\n\ntype Tag = Text {}\n\ntype Shelf = Object {\n    tags: List<Tag> {\n        max_length: 3\n    }\n}\n\ntype Order = Object {\n    count: Count {\n        min: 1\n        max: 12\n    }\n}\n\ntype OrderEmpty = Failure {\n    kind: FailureKind.invalid_value\n}\n\ntype OrderTooLarge = Failure {\n    kind: FailureKind.invalid_value\n}\n\nfunction is_small(count: CakeCount) -> Flag {\n    return Flag(count < 4)\n}\n\n";

fn with_types(source: &str) -> Vec<String> {
    rules(
        Layout::Loose,
        &[("types.jadpo", TYPES), ("main.jadpo", source)],
    )
}

/// A diagnostic's rule, reason, and each label's source and text.
type Explained = (String, String, Vec<(String, String)>);

/// The diagnostics of `source` beside [`TYPES`], each as its rule, its
/// reason, and its labels' text with the source they point at.
fn explained(source: &str) -> Vec<Explained> {
    let db = Database::default();
    let project = project(
        &db,
        Layout::Loose,
        &[("types.jadpo", TYPES), ("main.jadpo", source)],
    );
    jadpo_check::check(&db, project)
        .iter()
        .map(|d| {
            let labels = d
                .primary
                .iter()
                .chain(d.secondary.iter())
                .map(|label| {
                    let text = &source[label.span.start as usize..label.span.end as usize];
                    (text.to_string(), label.text.clone())
                })
                .collect();
            (
                d.rule.id.to_string(),
                d.reason.clone().unwrap_or_default(),
                labels,
            )
        })
        .collect()
}

#[test]
fn a_condition_that_leaves_proves_the_construction_after_it() {
    let clean = [
        // The decision's own example: after the check, `count * 2` is at
        // most 12, and at least 2 because `count` is at least 1.
        "function doubled_checked(count: CakeCount) fails OrderTooLarge -> CakeCount {\n    if (count * 2 > 12) {\n        reject OrderTooLarge\n    }\n    return CakeCount(count * 2)\n}\n",
        // One check for each end.
        "function from_count(n: Count) fails OrderEmpty, OrderTooLarge -> CakeCount {\n    if (n < 1) {\n        reject OrderEmpty\n    }\n    if (n > 12) {\n        reject OrderTooLarge\n    }\n    return CakeCount(n)\n}\n",
        // Both ends in one condition: after `a or b` leaves, neither holds.
        "function from_count(n: Count) fails OrderTooLarge -> CakeCount {\n    if (n < 1 or n > 12) {\n        reject OrderTooLarge\n    }\n    return CakeCount(n)\n}\n",
        // `not` turns a condition round.
        "function from_count(n: Count) fails OrderTooLarge -> CakeCount {\n    if (not (n >= 1 and n <= 12)) {\n        reject OrderTooLarge\n    }\n    return CakeCount(n)\n}\n",
        // `return` leaves as `reject` does.
        "function or_one(n: Count) -> CakeCount {\n    if (n < 1 or n > 12) {\n        return 1\n    }\n    return CakeCount(n)\n}\n",
        // A bound on a part carries through the arithmetic: `count` is at
        // most 6, so `count * 2` is at most 12.
        "function doubled_small(count: CakeCount) fails OrderTooLarge -> CakeCount {\n    if (count > 6) {\n        reject OrderTooLarge\n    }\n    return CakeCount(count * 2)\n}\n",
        // Compared with another value, the bound is that value's range.
        "function within(n: Count, limit: Limit) fails OrderEmpty, OrderTooLarge -> CakeCount {\n    if (n < 1) {\n        reject OrderEmpty\n    }\n    if (n > limit) {\n        reject OrderTooLarge\n    }\n    return CakeCount(n)\n}\n",
        // A whole number that is not 0, with 0 its lowest value, is at
        // least 1.
        "function share(stock: Stock) fails OrderEmpty -> People {\n    if (stock == 0) {\n        reject OrderEmpty\n    }\n    return People(stock)\n}\n",
        // A field of a value that never changes.
        "function from_line(line: Line) fails OrderTooLarge -> CakeCount {\n    if (line.quantity < 1 or line.quantity > 12) {\n        reject OrderTooLarge\n    }\n    return CakeCount(line.quantity)\n}\n",
        // A standard-library call on such values.
        "function counted(items: List<Tag>) fails OrderEmpty, OrderTooLarge -> CakeCount {\n    if (collection.count(items) == 0) {\n        reject OrderEmpty\n    }\n    if (collection.count(items) > 12) {\n        reject OrderTooLarge\n    }\n    return CakeCount(collection.count(items))\n}\n",
        // A count bounds the list's own length.
        "function stock_shelf(tags: List<Tag>) fails OrderTooLarge -> Shelf {\n    if (collection.count(tags) > 3) {\n        reject OrderTooLarge\n    }\n    return Shelf {\n        tags: tags\n    }\n}\n",
        // An object literal's field is proved the same way.
        "function build_order(n: Count) fails OrderTooLarge -> Order {\n    if (n < 1 or n > 12) {\n        reject OrderTooLarge\n    }\n    return Order {\n        count: n\n    }\n}\n",
        // Decimals: the value is rounded to the target's scale before its
        // rules are checked, so 10.004 is kept as 10.00.
        "function priced(amount: Amount) fails OrderTooLarge -> Price {\n    if (amount < 0.00 or amount > 10.004) {\n        reject OrderTooLarge\n    }\n    return Price(amount)\n}\n",
        // A `var mut` keeps its bound until it is assigned.
        "function kept(start: Count) fails OrderTooLarge -> CakeCount {\n    var mut n: Count = start\n    if (n < 1 or n > 12) {\n        reject OrderTooLarge\n    }\n    return CakeCount(n)\n}\n",
        // A loop that doesn't assign a value keeps its bound.
        "function each(n: Count, lines: List<Line>) fails OrderTooLarge -> Count {\n    if (n < 1 or n > 12) {\n        reject OrderTooLarge\n    }\n    var mut total: Count = 0\n    for line in lines {\n        var size: CakeCount = CakeCount(n)\n        total = Count(total * size + line.quantity)\n    }\n    return total\n}\n",
    ];
    for source in clean {
        assert_eq!(with_types(source), CLEAN, "for:\n{source}");
    }
}

#[test]
fn a_condition_proves_the_branch_where_it_holds() {
    let clean = [
        "function from_count(n: Count) fails OrderTooLarge -> CakeCount {\n    if (n >= 1 and n <= 12) {\n        return CakeCount(n)\n    }\n    reject OrderTooLarge\n}\n",
        // An `else if` knows every earlier condition is false, and so does
        // the `else`.
        "function sized(n: Count) fails OrderEmpty, OrderTooLarge -> CakeCount {\n    if (n < 1) {\n        reject OrderEmpty\n    } else if (n > 12) {\n        reject OrderTooLarge\n    } else {\n        return CakeCount(n)\n    }\n}\n",
        "function sized(n: Count) fails OrderEmpty, OrderTooLarge -> CakeCount {\n    if (n < 1) {\n        reject OrderEmpty\n    } else if (n <= 12) {\n        return CakeCount(n)\n    }\n    reject OrderTooLarge\n}\n",
        // The right side of `and` runs only where the left side holds.
        "function small(n: Count) -> Flag {\n    if (n >= 1 and n <= 12 and is_small(CakeCount(n))) {\n        return true\n    }\n    return false\n}\n",
        // The right side of `or` runs only where the left side doesn't.
        "function small(n: Count) -> Flag {\n    if (n < 1 or n > 12 or is_small(CakeCount(n))) {\n        return true\n    }\n    return false\n}\n",
    ];
    for source in clean {
        assert_eq!(with_types(source), CLEAN, "for:\n{source}");
    }
}

#[test]
fn nothing_else_counts_as_proof() {
    let cases = [
        // The branch where the value is too large.
        "function wrong_way(count: CakeCount) fails OrderTooLarge -> CakeCount {\n    if (count * 2 > 12) {\n        return CakeCount(count * 2)\n    }\n    reject OrderTooLarge\n}\n",
        // A branch that doesn't leave proves nothing after the `if`.
        "action note(n: Count) -> Unit {}\n\naction noted(n: Count) -> CakeCount {\n    if (n < 1 or n > 12) {\n        note(n)\n    }\n    return CakeCount(n)\n}\n",
        // An assignment is not a condition.
        "function capped(n: Count) -> CakeCount {\n    var mut size: Count = n\n    if (n > 12 or n < 1) {\n        size = 12\n    }\n    return CakeCount(size)\n}\n",
        // A `var mut` assigned after the check loses its bound.
        "function moved(start: Count) fails OrderTooLarge -> CakeCount {\n    var mut n: Count = start\n    if (n < 1 or n > 12) {\n        reject OrderTooLarge\n    }\n    n = Count(n + 1)\n    return CakeCount(n)\n}\n",
        // A loop that assigns a value loses its bound inside the loop...
        "function looped(start: Count, lines: List<Line>) fails OrderTooLarge -> Count {\n    var mut n: Count = start\n    if (n < 1 or n > 12) {\n        reject OrderTooLarge\n    }\n    for line in lines {\n        var size: CakeCount = CakeCount(n)\n        n = Count(size * line.quantity)\n    }\n    return n\n}\n",
        // ...and after it.
        "function looped(start: Count, lines: List<Line>) fails OrderTooLarge -> CakeCount {\n    var mut n: Count = start\n    if (n < 1 or n > 12) {\n        reject OrderTooLarge\n    }\n    for line in lines {\n        n = Count(n + line.quantity)\n    }\n    return CakeCount(n)\n}\n",
        // A check inside a branch that goes on stays in that branch.
        "function nested(n: Count, flag: Flag) fails OrderTooLarge -> CakeCount {\n    if (flag) {\n        if (n < 1 or n > 12) {\n            reject OrderTooLarge\n        }\n    }\n    return CakeCount(n)\n}\n",
        // ...and inside a `match` arm.
        "function matched(n: Count, flag: Flag) fails OrderTooLarge -> CakeCount {\n    match flag {\n        true -> {\n            if (n < 1 or n > 12) {\n                reject OrderTooLarge\n            }\n        }\n        false -> {}\n    }\n    return CakeCount(n)\n}\n",
        // The `else` of `a and b` knows only that one of them is false.
        "function other_way(n: Count) fails OrderTooLarge -> CakeCount {\n    if (n >= 1 and n <= 12) {\n        reject OrderTooLarge\n    } else {\n        return CakeCount(n)\n    }\n}\n",
        // Inside `a or b`, either may hold.
        "function either(n: Count) fails OrderTooLarge -> CakeCount {\n    if (n < 1 or n > 12) {\n        return CakeCount(n)\n    }\n    reject OrderTooLarge\n}\n",
        // An expression is compared as written: `2 * count` is not
        // `count * 2`.
        "function turned(count: CakeCount) fails OrderTooLarge -> CakeCount {\n    if (2 * count > 12) {\n        reject OrderTooLarge\n    }\n    return CakeCount(count * 2)\n}\n",
        // A variable's starting value is not a condition: `doubled` is an
        // `Int`, so only its checked end is known.
        "function named(count: CakeCount) fails OrderTooLarge -> CakeCount {\n    var doubled: Int = count * 2\n    if (doubled > 12) {\n        reject OrderTooLarge\n    }\n    return CakeCount(doubled)\n}\n",
        // A bound on the whole says nothing about its parts.
        "function parts(count: Count) fails OrderTooLarge -> CakeCount {\n    if (count * 2 > 12 or count * 2 < 2) {\n        reject OrderTooLarge\n    }\n    return CakeCount(count)\n}\n",
        // A decimal that isn't 0 can still be as close to 0 as it likes.
        "function nonzero(amount: Amount) fails OrderTooLarge -> Price {\n    if (amount < 0.00 or amount > 10.00 or amount == 0.00) {\n        reject OrderTooLarge\n    }\n    var priced: Price = Price(amount)\n    return Price(priced - 0.01)\n}\n",
    ];
    for source in cases {
        assert_eq!(
            with_types(source),
            ["failure.attempt_required"],
            "for:\n{source}"
        );
    }
}

#[test]
fn rounding_to_the_targets_scale_is_part_of_the_proof() {
    // `Fine` stops at 10.07 and `Coarse` keeps one decimal place, so 10.07
    // is kept as 10.1, past `max: 10.07`: the construction can fail even
    // though no value passes the maximum before it is rounded.
    let source = "type Coarse = Decimal {\n    scale: 1\n    max: 10.07\n}\n\ntype Fine = Decimal {\n    scale: 2\n    max: 10.07\n}\n\nfunction coarse(value: Fine) -> Coarse {\n    return Coarse(value)\n}\n";
    assert_eq!(
        rules(Layout::Loose, &[("main.jadpo", source)]),
        ["failure.attempt_required"]
    );
}

#[test]
fn each_call_site_that_flips_says_why() {
    // The check proves both rules, so `attempt` has nothing to pass on,
    // and the error names the condition that proves it.
    let infallible = explained(
        "function doubled_checked(count: CakeCount) fails OrderTooLarge -> CakeCount {\n    if (count * 2 > 12) {\n        reject OrderTooLarge\n    }\n    return attempt CakeCount(count * 2) {\n        max: OrderTooLarge\n    }\n}\n",
    );
    assert_eq!(infallible.len(), 1, "{infallible:#?}");
    let (rule, reason, labels) = &infallible[0];
    assert_eq!(rule, "failure.attempt_infallible");
    assert_eq!(
        reason,
        "The declared ranges and the condition `count * 2 > 12` prove `CakeCount(...)` always follows its type's rules, so building it never fails; `attempt` marks only what can."
    );
    assert!(
        labels.contains(&("count * 2 > 12".to_string(), "checked here".to_string())),
        "{labels:?}"
    );
    // One rule proved by a condition, the other still able to break.
    let arm = explained(
        "function from_count(n: Count) fails OrderEmpty, OrderTooLarge -> CakeCount {\n    if (n > 12) {\n        reject OrderTooLarge\n    }\n    return attempt CakeCount(n) {\n        min: OrderEmpty\n        max: OrderTooLarge\n    }\n}\n",
    );
    assert_eq!(arm.len(), 1, "{arm:#?}");
    let (rule, reason, labels) = &arm[0];
    assert_eq!(rule, "failure.rule_cannot_break");
    assert_eq!(
        reason,
        "The declared ranges and the condition `n > 12` prove the value always follows `max: 12`, so this arm could never run."
    );
    assert!(
        labels.contains(&("n > 12".to_string(), "checked here".to_string())),
        "{labels:?}"
    );
    // `invalid:` left covering nothing once a condition proves the rest.
    assert_eq!(
        with_types(
            "function from_count(n: Count) fails OrderEmpty, OrderTooLarge -> CakeCount {\n    if (n > 12) {\n        reject OrderTooLarge\n    }\n    return attempt CakeCount(n) {\n        min: OrderEmpty\n        invalid: OrderTooLarge\n    }\n}\n"
        ),
        ["failure.invalid_covers_nothing"]
    );
    // Without the check, the plain construction flips the other way, and
    // the error says a condition could prove it.
    let required = explained(
        "function doubled_checked(count: CakeCount) -> CakeCount {\n    return CakeCount(count * 2)\n}\n",
    );
    assert_eq!(required.len(), 1, "{required:#?}");
    let (rule, reason, _) = &required[0];
    assert_eq!(rule, "failure.attempt_required");
    assert_eq!(
        reason,
        "The value is only known when the program runs, and neither the declared ranges nor a condition before it prove it follows `max: 12`, so building it can fail."
    );
}

#[test]
fn the_typed_body_records_what_each_condition_proves() {
    let db = Database::default();
    let source = "function doubled_checked(count: CakeCount) fails OrderTooLarge -> CakeCount {\n    if (count * 2 > 12) {\n        reject OrderTooLarge\n    }\n    return CakeCount(count * 2)\n}\n";
    let project = project(
        &db,
        Layout::Loose,
        &[("types.jadpo", TYPES), ("main.jadpo", source)],
    );
    let file = project
        .files(&db)
        .iter()
        .copied()
        .find(|file| file.path(&db) == "main.jadpo")
        .expect("main.jadpo");
    let callable = jadpo_check::types::callables_of(&db, file)[0];
    let body = &jadpo_check::types::typed_body(&db, project, callable).body;
    assert_eq!(body.proofs.len(), 1, "{:#?}", body.proofs);
    let proof = &body.proofs[0];
    let site = body.site(proof.site);
    assert_eq!(
        &source[site.start as usize..site.end as usize],
        "CakeCount(count * 2)"
    );
    let by: Vec<&str> = proof
        .by
        .iter()
        .map(|id| {
            let site = body.site(*id);
            &source[site.start as usize..site.end as usize]
        })
        .collect();
    assert_eq!(by, ["count * 2 > 12"]);
}
