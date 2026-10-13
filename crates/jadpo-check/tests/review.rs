//! The Phase 1 review's reproducers (roadmap chunk 1.9): each mistake the
//! review found the checker accepting is now reported, once, with its rule.

mod support;

use jadpo_db::Layout;
use support::rules;

fn check(source: &str) -> Vec<String> {
    rules(Layout::Loose, &[("main.jadpo", source)])
}

const CLEAN: &[String] = &[];

const COUNTS: &str = "type Count = Int {\n    min: 0\n}\n\ntype Quantity = Int {\n    min: 1\n    max: 1000\n}\n\ntype DeliveryState = Enum {\n    pending\n    shipped\n}\n\ntype Refused = Failure {\n    kind: FailureKind.rejected\n}\n\ntype TooBig = Failure {\n    kind: FailureKind.invalid_value\n}\n\n";

fn with_counts(source: &str) -> Vec<String> {
    check(&format!("{COUNTS}{source}"))
}

// --- finding 2: `reject` and `invalid:` name failures ----------------------

#[test]
fn reject_names_only_a_failure() {
    assert_eq!(
        with_counts("function f(n: Count) -> Count {\n    reject FailureKind.rejected\n}\n"),
        ["semantic.wrong_name_kind"]
    );
    assert_eq!(
        with_counts(
            "function f(n: Count) -> Count {\n    reject DeliveryState.pending {\n        n: n\n    }\n}\n"
        ),
        ["semantic.wrong_name_kind"]
    );
}

#[test]
fn a_mapping_names_only_a_failure() {
    assert_eq!(
        with_counts(
            "function f(n: Count) -> Quantity {\n    var q: Quantity = attempt Quantity(n * 2) invalid: DeliveryState.pending\n    return q\n}\n"
        ),
        ["semantic.wrong_name_kind"]
    );
}

// --- finding 6: `reject` starts a statement or an arm ----------------------

#[test]
fn reject_is_never_a_value() {
    assert_eq!(
        with_counts(
            "action f() fails Refused -> Count {\n    var x: Count = reject Refused\n    return x\n}\n"
        ),
        ["failure.reject_placement"]
    );
    assert_eq!(
        with_counts(
            "function g(n: Count) -> Count {\n    return n\n}\n\nfunction f() fails Refused -> Count {\n    return g(reject Refused)\n}\n"
        ),
        ["failure.reject_placement"]
    );
    assert_eq!(
        with_counts(
            "function f(n: Count) fails Refused -> Count {\n    if (reject Refused) {\n        return n\n    }\n    return n\n}\n"
        ),
        ["failure.reject_placement"]
    );
    assert_eq!(
        with_counts(
            "function f(ns: List<Count>) fails Refused -> List<Count> {\n    return collection.map(ns, n -> reject Refused)\n}\n"
        ),
        ["failure.reject_placement"]
    );
}

#[test]
fn reject_still_stands_alone_and_ends_an_arm() {
    assert_eq!(
        with_counts(
            "function f(n: Count) fails Refused -> Count {\n    if (n > 3) {\n        reject Refused\n    }\n    return match n {\n        0 -> reject Refused\n        _ -> n\n    }\n}\n"
        ),
        CLEAN
    );
}

// --- finding 7: the standard library's divisions -------------------------

#[test]
fn a_standard_division_needs_a_divisor_that_excludes_zero() {
    assert_eq!(
        with_counts(
            "function f(a: Count, b: Count) -> Count {\n    return Count(integer.divide(a, b, rounding: Rounding.up))\n}\n"
        ),
        ["type.divisor_zero"]
    );
    assert_eq!(
        with_counts(
            "function f(a: Count) -> Count {\n    return Count(integer.divide(a, 0, rounding: Rounding.up))\n}\n"
        ),
        ["type.divisor_zero"]
    );
    assert_eq!(
        with_counts(
            "function f(a: Count, b: Quantity) fails TooBig -> Count {\n    return attempt Count(integer.divide(a, b, rounding: Rounding.up)) invalid: TooBig\n}\n"
        ),
        CLEAN
    );
}

// --- finding 4: functions never read the clock ---------------------------

const AT: &str = "type ReminderCutoff = Instant {}\n\n";

#[test]
fn only_actions_read_the_clock() {
    assert_eq!(
        check(&format!(
            "{AT}function cutoff() -> ReminderCutoff {{\n    return ReminderCutoff(clock.now)\n}}\n"
        )),
        ["type.clock_context"]
    );
    assert_eq!(
        check(&format!(
            "{AT}library dates {{\n    function cutoff() -> ReminderCutoff {{\n        return ReminderCutoff(clock.now)\n    }}\n}}\n"
        )),
        ["type.clock_context"]
    );
    assert_eq!(
        check(&format!(
            "{AT}type Flag = Bool {{}}\n\nfunction late(cutoffs: List<ReminderCutoff>) -> List<ReminderCutoff> {{\n    return collection.filter(cutoffs, c -> c < clock.now)\n}}\n"
        )),
        ["type.clock_context"]
    );
    assert_eq!(
        check(&format!(
            "{AT}action cutoff() -> ReminderCutoff {{\n    return ReminderCutoff(clock.now)\n}}\n"
        )),
        CLEAN
    );
}

#[test]
fn a_generated_namespace_member_not_built_yet_is_reported() {
    assert_eq!(
        check(&format!(
            "{AT}action cutoff() -> ReminderCutoff {{\n    return ReminderCutoff(clock.whenever)\n}}\n"
        )),
        ["toolchain.not_built_yet"]
    );
    assert_eq!(
        check(
            "type Count = Int {}\n\naction limit() -> Count {\n    var x: Count = config.no_such_setting\n    return x\n}\n"
        ),
        ["toolchain.not_built_yet"]
    );
    assert_eq!(
        check(&format!(
            "{AT}action cutoff() -> ReminderCutoff {{\n    return ReminderCutoff(clock.later())\n}}\n"
        )),
        ["toolchain.not_built_yet"]
    );
}
