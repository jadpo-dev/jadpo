//! Effects later areas build stop where they are reached, with the
//! `not_built_yet` fault naming them at their `.jadpo` line, never a panic;
//! the pure parts of actions and tests run.

mod support;

use jadpo_interp::{AssertionDetail, FaultKind, Got, Outcome, Value};
use support::{fault, int, main, program, run, value, where_fault};

#[test]
fn an_entity_operation_is_not_built_yet() {
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
            FaultKind::NotBuiltYet("Todo.get".to_string()),
            vec!["main.jadpo:2".to_string()]
        )
    );
    assert_eq!(stopped.kind.classification(), "not_built_yet");
    // The pure parts around it run.
    assert_eq!(
        value(&program, "already_archived", vec![id.clone()]),
        Value::Bool(false)
    );
    assert_eq!(value(&program, "notify_owner", vec![id]), Value::Unit);
}

#[test]
fn reading_the_clock_is_not_built_yet() {
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
    let due = program
        .codec()
        .text("DueAt", "2026-10-25T08:30:00Z")
        .expect("an instant");
    let stopped = fault(&program, "check_due", vec![due.clone(), due]);
    assert_eq!(
        where_fault(&program, &stopped),
        (
            FaultKind::NotBuiltYet("clock.now".to_string()),
            vec!["main.jadpo:2".to_string()]
        )
    );
}

#[test]
fn emitting_an_event_is_not_built_yet() {
    let program = main(
        "type Note = Text {\n    max_length: 40\n}\n\naction record(note: Note) -> Unit {\n    emit note\n}\n",
    );
    let stopped = fault(&program, "record", vec![Value::text("hi")]);
    assert_eq!(stopped.kind, FaultKind::NotBuiltYet("emit".to_string()));
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
fn an_entity_write_is_named_by_its_operation() {
    let program = main(
        "type TodoStatus = Enum {\n    open\n    done\n}\n\nentity Todo {\n    id: Uuid\n    status: TodoStatus\n}\n\naction finish(todo: Todo) -> Todo {\n    var done: Todo = Todo.update(todo) {\n        set: {\n            status: TodoStatus.done\n        }\n    }\n    return done\n}\n",
    );
    let todo = program
        .codec()
        .object(
            "Todo",
            vec![
                ("id", Value::uuid("01908f6e-1c2a-7b3c-9d4e-5f6a7b8c9d0e")),
                (
                    "status",
                    program
                        .codec()
                        .variant("TodoStatus", "open", vec![])
                        .expect("open"),
                ),
            ],
        )
        .expect("Todo");
    let stopped = fault(&program, "finish", vec![todo]);
    assert_eq!(
        where_fault(&program, &stopped),
        (
            FaultKind::NotBuiltYet("Todo.update".to_string()),
            vec!["main.jadpo:12".to_string()]
        )
    );
}
