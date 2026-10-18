//! Area 19, "Randomness" (chunk 5.5q, question 19): the five `random`
//! operations check where the generator may be read, in actions and tests;
//! a function, an inline function and a constant are pure, so a call there
//! is `type.random_context`, as `clock.now` is `type.clock_context`; a
//! code's length is proved from 4 to 32 and a sample's count not negative;
//! a shuffle and a sample keep the list's item type.

use crate::support;

use jadpo_db::Layout;
use support::{problems, rules};

const TYPES: &str = "type BookingCode = Text {\n    min_length: 4\n    max_length: 32\n}\n\ntype CancelToken = Text {\n    min_length: 43\n    max_length: 43\n}\n\ntype CodeLength = Int {\n    min: 4\n    max: 32\n}\n\ntype AnyLength = Int {}\n\ntype Seat = Text {\n    max_length: 4\n}\n\ntype Pick = Int {\n    min: 0\n}\n\ntype Offset = Int {}\n\ntype Shown = Text {}\n\ntype Roll = Int {}\n\ntype Unlucky = Failure {\n    kind: FailureKind.invalid_value\n}\n\n";

fn check(source: &str) -> Vec<String> {
    rules(
        Layout::Loose,
        &[("main.jadpo", &format!("{TYPES}{source}"))],
    )
}

fn shown(source: &str) -> Vec<String> {
    problems(
        Layout::Loose,
        &[("main.jadpo", &format!("{TYPES}{source}"))],
    )
}

const NONE: &[&str] = &[];

#[test]
fn every_operation_checks_in_an_action() {
    assert_eq!(
        check(
            "action book(seats: List<Seat>, length: CodeLength, pick: Pick) fails Unlucky -> List<Shown> {\n    var token: Text = random.token()\n    var cancel: CancelToken = attempt CancelToken(token) {\n        invalid: Unlucky\n    }\n    var code: BookingCode = attempt BookingCode(random.code(length)) {\n        invalid: Unlucky\n    }\n    var short: Text = random.code(6)\n    var roll: Int = random.integer(1, 6)\n    var shuffled: List<Seat> = random.shuffle(seats)\n    var some: List<Seat> = random.sample(seats, pick)\n    var two: List<Seat> = random.sample(seats, 2)\n    return [Shown(cancel), Shown(code), Shown(short), Shown(text.from(roll)), Shown(text.join(shuffled, \",\")), Shown(text.join(some, \",\")), Shown(text.join(two, \",\"))]\n}\n"
        ),
        NONE
    );
}

#[test]
fn a_test_reads_the_generator_too() {
    assert_eq!(
        rules(
            Layout::Loose,
            &[
                ("main.jadpo", TYPES),
                (
                    "main.test.jadpo",
                    "test \"a token is 43 characters\" {\n    assert text.length(random.token()) == 43\n}\n"
                ),
            ]
        ),
        NONE
    );
}

#[test]
fn a_pure_body_cant_read_the_generator() {
    for call in [
        "random.token()",
        "random.code(8)",
        "text.from(random.integer(1, 6))",
    ] {
        assert_eq!(
            check(&format!(
                "function f() -> Shown {{\n    return Shown({call})\n}}\n"
            )),
            ["type.random_context"],
            "{call}"
        );
    }
    assert_eq!(
        check(
            "function f(seats: List<Seat>) -> List<Seat> {\n    return random.shuffle(seats)\n}\n"
        ),
        ["type.random_context"]
    );
    // An inline function is pure, even inside an action.
    assert_eq!(
        check(
            "action f(seats: List<Seat>) -> List<Roll> {\n    return list.map(seats, seat -> Roll(random.integer(1, 6)))\n}\n"
        ),
        ["type.random_context"]
    );
    let shown = shown("function f() -> Shown {\n    return Shown(random.token())\n}\n");
    assert_eq!(shown.len(), 1, "{shown:?}");
    assert!(
        shown[0].contains("A function can't call `random.token`"),
        "{shown:?}"
    );
}

#[test]
fn a_codes_length_is_proved_from_4_to_32() {
    let with = |length: &str| {
        check(&format!(
            "action f(length: CodeLength, any: AnyLength) -> Shown {{\n    return Shown(random.code({length}))\n}}\n"
        ))
    };
    assert_eq!(with("length"), NONE);
    assert_eq!(with("4"), NONE);
    assert_eq!(with("32"), NONE);
    for length in ["3", "33", "any"] {
        assert_eq!(with(length), ["type.mismatch"], "`{length}`");
    }
}

#[test]
fn a_samples_count_is_never_negative() {
    let with = |count: &str| {
        check(&format!(
            "action f(seats: List<Seat>, pick: Pick, offset: Offset) -> List<Seat> {{\n    return random.sample(seats, {count})\n}}\n"
        ))
    };
    assert_eq!(with("pick"), NONE);
    assert_eq!(with("0"), NONE);
    assert_eq!(with("-1"), ["type.argument_negative"]);
    assert_eq!(with("offset"), ["type.argument_negative"]);
}

#[test]
fn a_shuffle_keeps_the_items_type_and_takes_only_a_list() {
    assert_eq!(
        check("action f(seats: List<Seat>) -> List<Roll> {\n    return random.shuffle(seats)\n}\n"),
        ["type.mismatch"]
    );
    assert_eq!(
        check("action f(seat: Seat) -> List<Seat> {\n    return random.shuffle(seat)\n}\n"),
        ["type.mismatch"]
    );
    assert_eq!(
        check("action f() -> Shown {\n    return Shown(random.token(16))\n}\n").len(),
        1,
        "a token takes nothing"
    );
}

#[test]
fn a_drawn_number_is_proved_within_its_bounds() {
    let with = |target: &str, call: &str| {
        check(&format!(
            "type Die = Int {{\n    min: 1\n    max: 6\n}}\n\naction f(pick: Pick, length: CodeLength) -> {target} {{\n    return {target}({call})\n}}\n"
        ))
    };
    assert_eq!(with("Die", "random.integer(1, 6)"), NONE);
    assert_eq!(with("Die", "random.integer(2, 5)"), NONE);
    assert_eq!(
        with("Die", "random.integer(0, 6)"),
        ["failure.attempt_required"]
    );
    assert_eq!(
        with("Die", "random.integer(1, 7)"),
        ["failure.attempt_required"]
    );
    assert_eq!(with("Pick", "random.integer(0, length)"), NONE);
    assert_eq!(with("Die", "random.integer(min: 1, max: 6)"), NONE);
}
