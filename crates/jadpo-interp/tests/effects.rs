//! Effects later areas build stop where they are reached, with the
//! `not_built_yet` fault naming them at their `.jadpo` line, never a panic;
//! `clock.now` reads the invocation's clock;
//! a read or write on a host with no store stops with the store's fault at
//! its line; the pure parts of actions and tests run; and the interpreter
//! runs every operation with effects through the runtime's one entry point
//! for them (chunk 7.0b).

use crate::support;

use jadpo_interp::{AssertionDetail, FaultKind, Got, Outcome, Value};
use support::{failure, fault, int, main, program, run, value, where_fault};

#[test]
fn a_read_without_a_store_faults_at_its_line() {
    // Area 3, "Statements": `archive_todo` reads a stored todo first.
    let program = program(&[
        (
            "todos.jadpo",
            "type TodoStatus = Enum {\n    open\n    archived\n}\n\nentity Todo {\n    id: Uuid\n    status: TodoStatus\n}\n\ntype TodoNotFound = Failure {\n    kind: FailureKind.not_found\n}\n\ntype Archived = Bool {}\n\nfunction already_archived(todo_id: Todo.id) -> Archived {\n    return false\n}\n\naction notify_owner(todo_id: Todo.id) -> Unit {}\n",
        ),
        (
            "main.jadpo",
            "action archive_todo(todo_id: Todo.id) fails TodoNotFound -> Unit {\n    attempt Todo.get(todo_id)  // fails or carries on; the value is not needed\n    if (already_archived(todo_id)) {\n        return  // leaves a Unit action early\n    }\n    notify_owner(todo_id)  // a Unit action called for its effect\n}\n",
        ),
    ]);
    let id = Value::uuid("01908f6e-1c2a-7b3c-9d4e-5f6a7b8c9d0e");
    let stopped = fault(&program, "archive_todo", vec![id.clone()]);
    assert_eq!(
        where_fault(&program, &stopped),
        (
            FaultKind::Store("no store holds any data".to_string()),
            vec!["main.jadpo:2".to_string()]
        )
    );
    assert_eq!(stopped.kind.classification(), "internal_fault");
    // The pure parts around it run.
    assert_eq!(
        value(&program, "already_archived", vec![id.clone()]),
        Value::Bool(false)
    );
    assert_eq!(value(&program, "notify_owner", vec![id]), Value::Unit);
}

#[test]
fn clock_now_reads_the_invocations_clock() {
    let program = program(&[
        (
            "types.jadpo",
            "type DueAt = Instant {}\n\ntype DueDateInPast = Failure {\n    kind: FailureKind.invalid_value\n}\n\ntype DueDateTooFar = Failure {\n    kind: FailureKind.invalid_value\n}\n",
        ),
        (
            "main.jadpo",
            "action check_due(due_at: DueAt, latest_due_at: DueAt) fails DueDateInPast, DueDateTooFar -> Unit {\n    if (due_at <= clock.now) {\n        reject DueDateInPast\n    } else if (due_at > latest_due_at) {\n        reject DueDateTooFar\n    }\n}\n",
        ),
    ]);
    // The default environment's clock stands at 2000-01-01T00:00:00Z, as a
    // test's does until it moves it (area 13).
    let due = program
        .codec()
        .text("DueAt", "2026-10-25T08:30:00Z")
        .expect("an instant");
    assert_eq!(
        value(&program, "check_due", vec![due.clone(), due]),
        Value::Unit
    );
    let past = program
        .codec()
        .text("DueAt", "1999-12-31T23:59:59Z")
        .expect("an instant");
    assert_eq!(
        failure(&program, "check_due", vec![past.clone(), past]).name,
        "DueDateInPast"
    );
}

#[test]
fn time_values_compare_in_order() {
    let program = main(
        "type DueAt = Instant {}\n\ntype Late = Bool {}\n\nfunction late(due_at: DueAt, done_at: DueAt) -> Late {\n    return Late(done_at > due_at)\n}\n\nfunction deadline() -> DueAt {\n    return \"2026-10-25T09:30:00+01:00\"\n}\n",
    );
    let at = |text: &str| program.codec().text("DueAt", text).expect("an instant");
    assert_eq!(
        value(
            &program,
            "late",
            vec![at("2026-10-25T08:30:00Z"), at("2026-10-25T08:31:00Z")]
        ),
        Value::Bool(true)
    );
    assert_eq!(
        value(&program, "deadline", vec![]),
        at("2026-10-25T08:30:00Z"),
        "the offset applied"
    );
    assert_eq!(
        value(&program, "deadline", vec![]).to_string(),
        "\"2026-10-25T08:30:00Z\""
    );
}

const SIZES: &str = "type Count = Int {\n    min: 0\n}\n\ntype TooBig = Failure {\n    kind: FailureKind.rejected\n    message: \"Too big.\"\n}\n\ntype TooSmall = Failure {\n    kind: FailureKind.rejected\n}\n\naction check_size(n: Count) fails TooBig, TooSmall -> Count {\n    if (n > 10) {\n        reject TooBig\n    }\n    if (n == 0) {\n        reject TooSmall\n    }\n    return n\n}\n";

#[test]
fn a_tests_assertions_run() {
    // Area 13, "Assertions".
    let program = program(&[
        ("sizes.jadpo", SIZES),
        (
            "sizes.test.jadpo",
            "test \"a large size is rejected\" {\n    assert check_size(11) fails TooBig\n}\n\ntest \"a small size passes\" {\n    var size: Count = attempt check_size(3)\n    assert size == 3\n}\n\ntest \"a wrong expectation fails\" {\n    var size: Count = attempt check_size(4)\n    assert size == 3\n}\n\ntest \"a size that passes isn't rejected\" {\n    assert check_size(3) fails TooBig\n}\n\ntest \"another failure isn't the one expected\" {\n    assert check_size(0) fails TooBig\n}\n\ntest \"a failure with attempt ends the test\" {\n    var size: Count = attempt check_size(12)\n    assert size == 12\n}\n",
        ),
    ]);
    let test = |name: &str| format!("test sizes \"{name}\"");
    assert_eq!(
        run(&program, &test("a large size is rejected"), vec![]),
        Outcome::Value(Value::Unit)
    );
    assert_eq!(
        run(&program, &test("a small size passes"), vec![]),
        Outcome::Value(Value::Unit)
    );
    let Outcome::Assertion(wrong) = run(&program, &test("a wrong expectation fails"), vec![])
    else {
        panic!("the assertion holds");
    };
    assert_eq!(
        wrong.detail,
        AssertionDetail::False {
            comparison: Some(("==", int(4), int(3)))
        }
    );
    let Outcome::Assertion(passes) =
        run(&program, &test("a size that passes isn't rejected"), vec![])
    else {
        panic!("the assertion holds");
    };
    assert_eq!(
        passes.detail,
        AssertionDetail::Fails {
            expected: "TooBig".to_string(),
            got: Got::Value(int(3))
        }
    );
    let Outcome::Assertion(other) = run(
        &program,
        &test("another failure isn't the one expected"),
        vec![],
    ) else {
        panic!("the assertion holds");
    };
    assert!(matches!(
        other.detail,
        AssertionDetail::Fails { got: Got::Failure(ref failure), .. } if failure.name == "TooSmall"
    ));
    let Outcome::Failed(failed) = run(
        &program,
        &test("a failure with attempt ends the test"),
        vec![],
    ) else {
        panic!("the call succeeds");
    };
    assert_eq!(failed.name, "TooBig");
    assert_eq!(
        failed.trace(program.places()),
        [
            "action check_size",
            "test \"a failure with attempt ends the test\""
        ]
    );
}

#[test]
fn a_write_without_a_store_faults_at_its_line() {
    let program = main(
        "type TodoStatus = Enum {\n    open\n    done\n}\n\nentity Todo {\n    id: Uuid\n    status: TodoStatus\n}\n\naction start() -> Todo {\n    var todo: Todo = Todo.create {\n        set: {\n            status: TodoStatus.open\n        }\n    }\n    return todo\n}\n",
    );
    let stopped = fault(&program, "start", vec![]);
    assert_eq!(
        where_fault(&program, &stopped),
        (
            FaultKind::Store("no store holds any data".to_string()),
            vec!["main.jadpo:12".to_string()]
        )
    );
}

/// What runs an operation with effects itself, each with the effect it
/// gives away: the runtime's entry point (`jadpo_runtime::effects`) names
/// every one, and no interpreter source may.
const EFFECTS_ONLY: &[(&str, &str)] = &[
    (".now()", "reads the invocation's clock: `clock.now`"),
    ("random_bytes", "takes the invocation's random bytes"),
    ("random::run", "runs `random`"),
    ("reads_generator", "tells `random` and a salt apart"),
    ("crypto::run", "runs `crypto`"),
    ("is_crypto", "tells `crypto` apart"),
    ("reads_clock", "gives `verify_jwt` the clock"),
    ("KeyRing", "holds the key a `crypto` call's `key:` names"),
    (
        "verify_jwt_as",
        "reads a token's claims with the plan's codec",
    ),
    ("Op::Crypto", "runs a `crypto` operation of its own"),
    ("Op::Json", "runs `json.read` or `json.write` of its own"),
    ("read_value", "reads JSON with the plan's codec"),
    ("ctx.emit(", "keeps an event in the outbox"),
    ("EmitRefused", "says why an `emit` wasn't kept"),
    ("subscribers_of", "finds an event's subscribers"),
];

/// A source file's code, without its comments and its tests: what it runs.
fn code_of(text: &str) -> String {
    let text = text
        .split("#[cfg(test)]\nmod tests")
        .next()
        .unwrap_or_default();
    text.lines()
        .map(|line| line.split("//").next().unwrap_or_default())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn the_interpreter_runs_effects_only_through_the_runtime() {
    let crates = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let entry = code_of(
        &std::fs::read_to_string(crates.join("jadpo-runtime/src/effects.rs"))
            .expect("the runtime's entry point for effects"),
    );
    let mut sources: Vec<_> = std::fs::read_dir(crates.join("jadpo-interp/src"))
        .expect("the interpreter's sources")
        .map(|entry| entry.expect("a source").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "rs"))
        .collect();
    sources.sort();
    assert!(sources.len() > 3, "the interpreter's sources: {sources:?}");
    let mut found = Vec::new();
    for path in &sources {
        let code = code_of(&std::fs::read_to_string(path).expect("a source file"));
        let file = path.file_name().unwrap_or_default().to_string_lossy();
        for (name, what) in EFFECTS_ONLY {
            if code.contains(name) {
                found.push(format!("{file}: `{name}` {what}"));
            }
        }
        // `clock.now` is the graph's own node, never told by its text.
        if code.contains("\"clock.now\"") {
            found.push(format!("{file}: tells `clock.now` by its source text"));
        }
    }
    assert!(
        found.is_empty(),
        "the interpreter runs an operation with effects of its own; call \
         `jadpo_runtime::effects::run` instead, which compiled code calls too:\n{}",
        found.join("\n")
    );
    // Each name is still the entry point's, so the list can't go stale.
    for (name, _) in EFFECTS_ONLY {
        assert!(
            entry.contains(name),
            "`{name}` is no longer the runtime's entry point's: update EFFECTS_ONLY"
        );
    }
}
