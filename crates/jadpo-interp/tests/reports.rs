//! What a person reads when a call stops: the rule or kind, the `.jadpo`
//! file and line of the site with its code frame, then the frames (area 4,
//! "Faults" and "Logging"; area 17, "Errors point at the `.jadpo` line"),
//! on the grid every Jadpo error shares (CLI design language). These are
//! the plain forms, exactly.

use crate::support;

use jadpo_interp::on_large_stack;
use support::{int, main, program, report, text};

const LINE_TOTAL: &str = "type Pence = Int {\n    min: 0\n}\n\ntype Quantity = Int {\n    min: 1\n    max: 1000\n}\n\nfunction line_total(price: Pence, quantity: Quantity) -> Pence {\n    var total: Pence = Pence(price * quantity)\n    return total\n}\n\naction price_line(price: Pence, quantity: Quantity) -> Pence {\n    var line: Pence = line_total(price, quantity)\n    return line\n}\n";

#[test]
fn an_overflow_names_its_site_and_every_frame() {
    let program = main(LINE_TOTAL);
    assert_eq!(
        report(&program, "price_line", vec![int(i64::MAX / 2), int(3)]),
        "\
fault: int_overflow in function line_total
 --> main.jadpo:11:30
   |
11 |     var total: Pence = Pence(price * quantity)
   |                              ^^^^^^^^^^^^^^^^ out of range
  reason: `price * quantity` gives a whole number outside `Int`'s range, -9223372036854775808 to 9223372036854775807; Jadpo stops rather than wrap around.
  next: Bound the values that feed it with `max`, so the result always fits (agent)
  stack: main.jadpo:11  function line_total
         main.jadpo:16  action price_line
"
    );
}

/// `n` isn't 0 past the `if`, so `Count(n - 1)` can't fail (area 2,
/// "Conditions as proof").
const COUNTDOWN: &str = "type Count = Int {\n    min: 0\n}\n\nfunction countdown(n: Count) -> Count {\n    if (n == 0) {\n        return n\n    }\n    var next: Count = Count(n - 1)\n    return countdown(next)\n}\n\naction start(n: Count) -> Count {\n    return countdown(n)\n}\n";

#[test]
fn the_recursion_limit_shows_a_repeated_frame_once() {
    let shown = on_large_stack(|| {
        let program = main(COUNTDOWN);
        report(&program, "start", vec![int(5000)])
    })
    .expect("a thread");
    assert_eq!(
        shown,
        "\
fault: recursion_limit in function countdown
 --> main.jadpo:10:12
   |
10 |     return countdown(next)
   |            ^^^^^^^^^^^^^^^ 1000 calls deep
  reason: The call stack reached 1000 frames, the most Jadpo allows, so the recursion stopped here instead of exhausting the stack.
  next: Make the recursion end sooner, or recurse over smaller values (agent)
  stack: main.jadpo:10  function countdown (999 times)
         main.jadpo:14  action start
"
    );
}

#[test]
fn a_failure_shows_its_code_fields_cause_and_path() {
    let program = program(&[
        (
            "lookup.jadpo",
            "type LookupCode = Text {}\n\ntype LookupResult = Text {}\n\ntype LookupMissing = Failure {\n    kind: FailureKind.not_found\n}\n\ntype LookupRefused = Failure {\n    kind: FailureKind.rejected\n    internal: {\n        reason: LookupResult\n    }\n}\n\ntype LookupRejected = Failure {\n    kind: FailureKind.rejected\n    message: \"The lookup was refused.\"\n    client: {\n        code: LookupCode\n    }\n}\n\nfunction lookup(key: LookupCode) fails LookupMissing, LookupRefused -> LookupResult {\n    if (key == \"missing\") {\n        reject LookupMissing\n    }\n    if (key == \"refused\") {\n        reject LookupRefused {\n            reason: \"on the deny list\"\n        }\n    }\n    return \"found\"\n}\n",
        ),
        (
            "main.jadpo",
            "function lookup_or_default(key: LookupCode) fails LookupRejected -> LookupResult {\n    return match lookup(key) {\n        success(value) -> value\n        LookupMissing -> \"not found\"\n        LookupRefused -> reject LookupRejected {\n            code: key\n        }\n    }\n}\n",
        ),
    ]);
    assert_eq!(
        report(&program, "lookup_or_default", vec![text("refused")]),
        "\
failed: `LookupRejected` from function lookup_or_default
 --> main.jadpo:5:26
  |
5 |         LookupRefused -> reject LookupRejected {
  |                          ^^^^^^^^^^^^^^^^^^^^^^^ rejected here
  code: lookup_rejected
  kind: rejected (422)
  message: The lookup was refused.
  client: { code: \"refused\" }
  cause: `LookupRefused` from function lookup, lookup.jadpo:29 (internal: reason)
  path: function lookup_or_default
"
    );
}

#[test]
fn a_broken_construction_shows_the_rule_never_the_value() {
    let program = main(
        "type Quantity = Int {\n    min: 1\n    max: 1000\n}\n\ntype QuantityTooLarge = Failure {\n    kind: FailureKind.invalid_value\n}\n\nfunction doubled(quantity: Quantity) fails QuantityTooLarge -> Quantity {\n    var result: Quantity = attempt Quantity(quantity * 2) {  // can exceed 1000\n        max: QuantityTooLarge\n    }\n    return result\n}\n\naction reorder(quantity: Quantity) fails QuantityTooLarge -> Quantity {\n    return attempt doubled(quantity)\n}\n",
    );
    assert_eq!(
        report(&program, "reorder", vec![int(501)]),
        "\
failed: `QuantityTooLarge` from function doubled
 --> main.jadpo:11:36
   |
11 |     var result: Quantity = attempt Quantity(quantity * 2) {  // can exceed 1000
   |                                    ^^^^^^^^^^^^^^^^^^^^^^ breaks `max: 1000`
  reason: The value built here breaks `max: 1000`, so `attempt` raises the failure its block maps that rule to.
  code: quantity_too_large
  kind: invalid_value (422)
  path: action reorder -> function doubled
"
    );
}

#[test]
fn a_failed_assertion_shows_both_sides() {
    let program = program(&[
        (
            "sizes.jadpo",
            "type Count = Int {\n    min: 0\n}\n\ntype TooBig = Failure {\n    kind: FailureKind.rejected\n}\n\naction check_size(n: Count) fails TooBig -> Count {\n    if (n > 10) {\n        reject TooBig\n    }\n    return n\n}\n",
        ),
        (
            "sizes.test.jadpo",
            "test \"a wrong expectation fails\" {\n    var size: Count = attempt check_size(4)\n    assert size == 3\n}\n\ntest \"a size that passes isn't rejected\" {\n    assert check_size(3) fails TooBig\n}\n",
        ),
    ]);
    assert_eq!(
        report(&program, "test sizes \"a wrong expectation fails\"", vec![]),
        "\
failed: an assertion in test \"a wrong expectation fails\"
 --> sizes.test.jadpo:3:5
  |
3 |     assert size == 3
  |     ^^^^^^^^^^^^^^^^ doesn't hold
  expected: 3
  actual: 4
"
    );
    assert_eq!(
        report(
            &program,
            "test sizes \"a size that passes isn't rejected\"",
            vec![]
        ),
        "\
failed: an assertion in test \"a size that passes isn't rejected\"
 --> sizes.test.jadpo:7:5
  |
7 |     assert check_size(3) fails TooBig
  |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ didn't fail as expected
  expected: fails with `TooBig`
  actual: gave 3
"
    );
}

#[test]
fn a_value_has_no_report() {
    let program = main(LINE_TOTAL);
    let outcome = program
        .call("line_total", vec![int(2), int(3)])
        .expect("runs");
    assert_eq!(program.report(&outcome), None);
}
