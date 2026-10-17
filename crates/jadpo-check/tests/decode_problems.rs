//! Area 19, "Data formats" (chunk 5.5g2): a failure mapped from
//! `json.read`'s `mismatch` that declares `problem: DecodeProblem`, in its
//! `client` or `internal` block, receives the first problem, so the arm
//! gives every other field and never `problem`; nothing else fills one, so
//! such a failure raised anywhere else is missing it; only the runtime
//! builds a `DecodeProblem`, and a program reads its `field` and `rule`.

use crate::support;

use jadpo_db::Layout;
use support::{problems, rules};

const TYPES: &str = "type Note = Text {\n    max_length: 200\n}\n\ntype Shown = Text {}\n\ntype Quantity = Int {\n    min: 1\n    max: 1000\n}\n\ntype ImportLine = Input {\n    sku: Note\n    quantity: Quantity\n}\n\ntype OrderImport = Input {\n    reference: Note\n    lines: List<ImportLine>\n}\n\ntype Unreadable = Failure {\n    kind: FailureKind.invalid_value\n}\n\ntype Invalid = Failure {\n    kind: FailureKind.invalid_value\n    client: {\n        problem: DecodeProblem\n    }\n}\n\ntype Logged = Failure {\n    kind: FailureKind.invalid_value\n    client: {\n        source: Note\n    }\n    internal: {\n        problem: DecodeProblem\n    }\n}\n\n";

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
fn a_failure_mapped_from_mismatch_receives_the_problem() {
    // In `client`, or in `internal` beside fields the arm gives.
    assert_eq!(
        check(
            "function f(note: Note) fails Unreadable, Invalid -> OrderImport {\n    return attempt json.read(note, as: OrderImport) {\n        malformed: Unreadable\n        mismatch: Invalid\n    }\n}\n\nfunction g(note: Note) fails Unreadable, Logged -> OrderImport {\n    return attempt json.read(note, as: OrderImport) {\n        malformed: Unreadable\n        mismatch: Logged {\n            source: note\n        }\n    }\n}\n"
        ),
        NONE
    );
    // `invalid:` covering `mismatch` alone receives it as well.
    assert_eq!(
        check(
            "function f(note: Note) fails Unreadable, Invalid -> OrderImport {\n    return attempt json.read(note, as: OrderImport) {\n        malformed: Unreadable\n        invalid: Invalid\n    }\n}\n"
        ),
        NONE
    );
    // A field called `problem` of another type is an ordinary field.
    assert_eq!(
        check(
            "type Other = Failure {\n    kind: FailureKind.invalid_value\n    client: {\n        problem: Note\n    }\n}\n\nfunction f(note: Note) fails Unreadable, Other -> OrderImport {\n    return attempt json.read(note, as: OrderImport) {\n        malformed: Unreadable\n        mismatch: Other\n    }\n}\n"
        ),
        ["failure.missing_context_field"]
    );
}

#[test]
fn nothing_else_fills_a_decode_problem() {
    // `malformed` has no problem to give.
    assert_eq!(
        check(
            "function f(note: Note) fails Invalid -> OrderImport {\n    return attempt json.read(note, as: OrderImport) {\n        malformed: Invalid\n        mismatch: Invalid\n    }\n}\n"
        ),
        ["failure.missing_context_field"]
    );
    // Nor does `invalid:` covering `malformed` too.
    assert_eq!(
        check(
            "function f(note: Note) fails Invalid -> OrderImport {\n    return attempt json.read(note, as: OrderImport) {\n        invalid: Invalid\n    }\n}\n"
        ),
        ["failure.missing_context_field"]
    );
    // Nor a construction's rule, nor `reject`.
    assert_eq!(
        check(
            "type Short = Text {\n    max_length: 5\n}\n\nfunction f(note: Note) fails Invalid -> Short {\n    return attempt Short(note) {\n        max_length: Invalid\n    }\n}\n"
        ),
        ["failure.missing_context_field"]
    );
    let said = shown("function f() fails Invalid -> Shown {\n    reject Invalid\n}\n");
    assert_eq!(said.len(), 1, "{said:?}");
    assert!(
        said[0].contains("failure.missing_context_field: `Invalid` is missing `problem`"),
        "{said:?}"
    );
}

#[test]
fn the_received_problem_is_never_given_as_well() {
    assert_eq!(
        check(
            "function f(note: Note) fails Unreadable, Invalid -> OrderImport {\n    return attempt json.read(note, as: OrderImport) {\n        malformed: Unreadable\n        mismatch: Invalid {\n            problem: note\n        }\n    }\n}\n"
        ),
        ["failure.duplicate_context_field"]
    );
    let said = shown(
        "function f(note: Note) fails Unreadable, Invalid -> OrderImport {\n    return attempt json.read(note, as: OrderImport) {\n        malformed: Unreadable\n        mismatch: Invalid {\n            problem: note\n        }\n    }\n}\n",
    );
    assert!(
        said[0].contains("`problem` is filled in by `mismatch`"),
        "{said:?}"
    );
}

#[test]
fn only_the_runtime_builds_a_decode_problem() {
    // So a problem never holds a value a program wrote.
    assert_eq!(
        check(
            "function f(note: Note) fails Invalid -> Shown {\n    reject Invalid {\n        problem: DecodeProblem {\n            field: note\n            rule: \"max\"\n        }\n    }\n}\n"
        ),
        ["type.not_record"]
    );
    // It has no JSON form of its own, either way.
    assert_eq!(
        check(
            "function f(problem: DecodeProblem) -> Shown {\n    return Shown(json.write(problem))\n}\n"
        ),
        ["type.mismatch"]
    );
    // And its name is the language's.
    assert_eq!(
        rules(
            Layout::Loose,
            &[(
                "main.jadpo",
                "type DecodeProblem = Object {\n    field: Note\n}\n\ntype Note = Text {}\n"
            )]
        ),
        ["semantic.duplicate_declaration"]
    );
}

#[test]
fn a_program_reads_the_problem_s_field_and_rule() {
    assert_eq!(
        check(
            "function f(note: Note) -> List<Shown> {\n    return match imported(note) {\n        success -> []\n        Unreadable -> []\n        Invalid(invalid) -> [Shown(invalid.problem.field), Shown(invalid.problem.rule)]\n    }\n}\n\nfunction imported(note: Note) fails Unreadable, Invalid -> OrderImport {\n    return attempt json.read(note, as: OrderImport) {\n        malformed: Unreadable\n        mismatch: Invalid\n    }\n}\n"
        ),
        NONE
    );
    assert_eq!(
        check(
            "function f(note: Note) -> Shown {\n    return match imported(note) {\n        success -> \"\"\n        Unreadable -> \"\"\n        Invalid(invalid) -> Shown(invalid.problem.value)\n    }\n}\n\nfunction imported(note: Note) fails Unreadable, Invalid -> OrderImport {\n    return attempt json.read(note, as: OrderImport) {\n        malformed: Unreadable\n        mismatch: Invalid\n    }\n}\n"
        ),
        ["type.unknown_field"]
    );
}
