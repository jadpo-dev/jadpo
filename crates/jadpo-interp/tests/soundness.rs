//! The Phase 2 review's soundness findings, run (roadmap chunk 2.7): each
//! value the interpreter once stored unchecked, each wrong result, each
//! failure caught by the wrong `match`, now behaves as the pages decide.

use crate::support;

use jadpo_interp::{Outcome, Value};
use support::{decimal, failure, int, main, run, text, value};

// --- finding 1: the built-in text types check their text -----------------

const ADDRESSES: &str = "type Raw = Text {}\n\ntype WorkEmail = Email {\n    pattern: \".*@acme[.]com\"\n}\n\ntype Link = Url {}\n\ntype Host = IpAddress {}\n\ntype Contact = Object {\n    email: Email\n}\n\ntype NotWork = Failure {\n    kind: FailureKind.invalid_value\n}\n\ntype NotAnAddress = Failure {\n    kind: FailureKind.invalid_value\n}\n\nfunction work(t: Raw) fails NotWork, NotAnAddress -> WorkEmail {\n    var e: WorkEmail = attempt WorkEmail(t) {\n        pattern: NotWork\n        format: NotAnAddress\n    }\n    return e\n}\n\nfunction work_from(address: Email) fails NotWork -> WorkEmail {\n    var e: WorkEmail = attempt WorkEmail(address) {\n        pattern: NotWork\n    }\n    return e\n}\n\nfunction link(t: Raw) fails NotAnAddress -> Link {\n    var l: Link = attempt Link(t) {\n        invalid: NotAnAddress\n    }\n    return l\n}\n\nfunction host(t: Raw) fails NotAnAddress -> Host {\n    var h: Host = attempt Host(t) {\n        format: NotAnAddress\n    }\n    return h\n}\n\nfunction contact(t: Raw) fails NotAnAddress -> Contact {\n    var email: Contact.email = attempt Contact.email(t) {\n        format: NotAnAddress\n    }\n    return Contact {\n        email: email\n    }\n}\n";

#[test]
fn text_built_into_an_address_type_is_checked_as_its_format() {
    let program = main(ADDRESSES);
    let fails_with = |name: &str, input: &str| failure(&program, name, vec![text(input)]).name;
    assert_eq!(fails_with("work", "junk"), "NotAnAddress");
    assert_eq!(fails_with("work", "sam@example.com"), "NotWork");
    assert_eq!(
        value(&program, "work", vec![text("sam@acme.com")]),
        text("sam@acme.com")
    );
    // An `Email` is already an address: only the pattern can break.
    assert_eq!(
        failure(&program, "work_from", vec![text("sam@example.com")]).name,
        "NotWork"
    );
    assert_eq!(fails_with("link", "junk"), "NotAnAddress");
    assert_eq!(
        value(&program, "link", vec![text("https://example.com")]),
        text("https://example.com")
    );
    assert_eq!(fails_with("host", "junk"), "NotAnAddress");
    assert_eq!(
        value(&program, "host", vec![text("10.0.0.1")]),
        text("10.0.0.1")
    );
    assert_eq!(fails_with("contact", "junk"), "NotAnAddress");
}

#[test]
fn a_broken_address_records_the_format_rule() {
    let program = main(ADDRESSES);
    let failed = failure(&program, "work", vec![text("junk")]);
    let broken = failed.broken.as_deref().unwrap_or_default();
    assert!(
        broken.contains("Email"),
        "the failure names the check that broke: {broken}"
    );
}

// --- finding 2: decimals compare exactly, whatever their scales -----------

#[test]
fn a_decimal_bound_far_apart_in_scale_is_compared_exactly() {
    // 1.0 is far above `Tiny`'s maximum: building one can fail, so the
    // arm is needed, and it runs.
    let program = main(
        "type Big = Decimal {\n    scale: 1\n    min: 0.0\n    max: 100000000000000000000000000000.0\n}\n\ntype Tiny = Decimal {\n    scale: 10\n    min: 0.0\n    max: 0.0000000005\n}\n\ntype TooBig = Failure {\n    kind: FailureKind.invalid_value\n}\n\nfunction shrink(b: Big) fails TooBig -> Tiny {\n    var t: Tiny = attempt Tiny(b) {\n        max: TooBig\n    }\n    return t\n}\n",
    );
    assert_eq!(
        failure(&program, "shrink", vec![decimal("1.0")]).name,
        "TooBig"
    );
    assert_eq!(
        value(&program, "shrink", vec![decimal("0.0")]),
        decimal("0")
    );
}

#[test]
fn comparing_a_large_decimal_with_a_fine_one_is_exact() {
    let program = main(
        "type Amount = Decimal {\n    scale: 2\n    min: 0.01\n}\n\ntype Flag = Bool {}\n\nfunction above_a_third(big: Amount, one: Amount, three: Amount) -> Flag {\n    return Flag(big > one / three)\n}\n",
    );
    assert_eq!(
        value(
            &program,
            "above_a_third",
            vec![
                decimal("10000000000000000000.00"),
                decimal("1.00"),
                decimal("3.00")
            ]
        ),
        Value::Bool(true)
    );
}

// --- finding 3: `/` keeps 18 places past its operands ---------------------

#[test]
fn a_quotient_of_large_decimals_is_exact_to_eighteen_places() {
    let program = main(
        "type Amount = Decimal {\n    scale: 2\n    min: 0.01\n}\n\ntype Share = Decimal {\n    scale: 2\n}\n\nfunction share(total: Amount, people: Amount) -> Share {\n    return Share(total / people)\n}\n",
    );
    let shared = value(
        &program,
        "share",
        vec![
            decimal("100000000499999999999.99"),
            decimal("300000000000000.00"),
        ],
    );
    assert_eq!(shared.to_string(), "333333.33");
}

// --- finding 4: an outcome `match` catches only its call's failures --------

const OUTCOMES: &str = "type Count = Int {}\n\ntype Boom = Failure {\n    kind: FailureKind.rejected\n}\n\ntype Answer = Text {}\n\nfunction explode(n: Count) fails Boom -> Count {\n    reject Boom\n}\n\nfunction checked(n: Count) fails Boom -> Count {\n    if (n > 5) {\n        reject Boom\n    }\n    return n\n}\n\nfunction outer(n: Count) fails Boom -> Answer {\n    var a: Answer = match checked(match explode(n) {\n        success(v) -> v\n        Boom -> propagate\n    }) {\n        success -> \"ok\"\n        Boom -> \"caught by the outer match\"\n    }\n    return a\n}\n\ntest \"an argument's failure leaves the test\" {\n    var n: Count = 1\n    assert checked(match explode(n) {\n        success(v) -> v\n        Boom -> propagate\n    }) fails Boom\n}\n";

#[test]
fn a_failure_in_an_outcome_matchs_arguments_leaves_the_callable() {
    let program = main(OUTCOMES);
    assert_eq!(failure(&program, "outer", vec![int(1)]).name, "Boom");
}

#[test]
fn a_failure_in_an_asserted_calls_arguments_leaves_the_test() {
    let program = main(OUTCOMES);
    let test = "test main \"an argument's failure leaves the test\"";
    match run(&program, test, vec![]) {
        Outcome::Failed(failure) => assert_eq!(failure.name, "Boom"),
        other => panic!("the test passed or ended otherwise: {other:?}"),
    }
}

// --- finding 5: a set literal in a named set type is a set -----------------

#[test]
fn a_set_literal_in_a_named_set_type_holds_each_value_once() {
    let program = main(
        "type Name = Text {}\n\ntype Tags = Set<Name> {}\n\ntype Same = Bool {}\n\nfunction tags(a: Name, b: Name) -> Tags {\n    return [b, a]\n}\n\nfunction same(a: Name, b: Name) -> Same {\n    var x: Tags = [b, a]\n    var y: Tags = [a, b]\n    return Same(x == y)\n}\n",
    );
    assert_eq!(
        value(&program, "same", vec![text("a"), text("b")]),
        Value::Bool(true)
    );
    assert_eq!(
        value(&program, "tags", vec![text("a"), text("a")]),
        Value::set(vec![text("a")])
    );
}

// --- finding 6: copied fields keep their declared order --------------------

#[test]
fn included_fields_follow_the_order_their_type_declares() {
    let program = main(
        "type Word = Text {}\n\ntype Base = Object {\n    zeta: Word\n    alpha: Word\n}\n\ntype Copy = Object {\n    include: [Base]\n}\n\ntype Mixed = Object {\n    first: Word\n    include: [Base]\n    last: Word\n}\n\ntype Picked = Object {\n    pick: [Base.alpha, Base.zeta]\n}\n\nfunction copy(z: Base.zeta, a: Base.alpha) -> Copy {\n    return Copy {\n        alpha: a\n        zeta: z\n    }\n}\n\nfunction mixed(z: Base.zeta, a: Base.alpha) -> Mixed {\n    return Mixed {\n        last: \"w\"\n        alpha: a\n        first: \"w\"\n        zeta: z\n    }\n}\n\nfunction picked(z: Base.zeta, a: Base.alpha) -> Picked {\n    return Picked {\n        zeta: z\n        alpha: a\n    }\n}\n",
    );
    let shown = |name: &str| value(&program, name, vec![text("w"), text("w")]).to_string();
    assert_eq!(shown("copy"), "Copy { zeta: \"w\", alpha: \"w\" }");
    assert_eq!(
        shown("mixed"),
        "Mixed { first: \"w\", zeta: \"w\", alpha: \"w\", last: \"w\" }"
    );
    assert_eq!(shown("picked"), "Picked { alpha: \"w\", zeta: \"w\" }");
}

// --- finding 7: the smallest `Int` has a remainder ------------------------

#[test]
fn the_smallest_int_divided_by_minus_one_leaves_no_remainder() {
    let program = main(
        "type Count = Int {}\n\ntype Negative = Int {\n    max: -1\n}\n\nfunction remainder(a: Count, b: Negative) -> Count {\n    return Count(a % b)\n}\n",
    );
    assert_eq!(
        value(&program, "remainder", vec![int(i64::MIN), int(-1)]),
        int(0)
    );
    assert_eq!(
        value(&program, "remainder", vec![int(-7), int(-2)]),
        int(-1)
    );
}
