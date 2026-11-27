//! The request timeline and `jadpo trace` (area 17, "The request timeline
//! in `jadpo dev`"; chunk E0), against live PostgreSQL. A todo list:
//! adding a todo writes it and emits `TodoAdded`, which `note_added` hears;
//! listing the open ones runs a query no index serves; the board runs two
//! queries; and finishing a todo twice fails the second time.
//!
//! `jadpo dev` keeps each request's timeline in `build/dev/`, flags a
//! request that ran several queries or one no index serves on its line,
//! and `jadpo trace` shows a request's route, caller, actions, queries and
//! writes with their SQL, the failure and where it was raised, and the
//! outcome with the events emitted and the subscribers they ran. Text is
//! never kept as it was sent, only as its keyed hash. And `jadpo test
//! --keep-failed` keeps a failed test's trace, which its report names.
//!
//! The server is the one `JADPO_TEST_DATABASE_URL` names or one started
//! from the machine's PostgreSQL tools; `JADPO_SKIP_POSTGRES=1` skips them.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::http::{Dev, jadpo, tempdir, write};

/// A title no kept timeline may hold as it was sent.
const TITLE: &str = "Buy oat milk";

const FILES: &[(&str, &str)] = &[
    ("app.jadpo", "application {}\n"),
    (
        "domains/planning/entities/todo.jadpo",
        r#"use planning/types { TodoStatus, TodoTitle }

entity Todo {
    id: Uuid
    title: TodoTitle
    status: TodoStatus
    policy: {
        create: [Access.public]
        read: [Access.public]
        update: [Access.public]
        delete: [Access.public]
    }
}
"#,
    ),
    (
        "domains/planning/types/todo_status.jadpo",
        "/// Where a todo is.\ntype TodoStatus = Enum {\n    open\n    done\n}\n",
    ),
    (
        "domains/planning/types/todo_title.jadpo",
        "/// What a todo says.\ntype TodoTitle = Text {\n    min_length: 1\n    max_length: 200\n}\n",
    ),
    (
        "domains/planning/types/todo_view.jadpo",
        "use planning/entities { Todo }\n\n/// A todo as clients see it.\ntype TodoView = Output {\n    pick: [Todo.id, Todo.title, Todo.status]\n}\n",
    ),
    (
        "domains/planning/types/board.jadpo",
        "use planning/types { TodoView }\n\n/// The open and the done todos.\ntype Board = Output {\n    open: List<TodoView>\n    done: List<TodoView>\n}\n",
    ),
    (
        "domains/planning/types/new_todo.jadpo",
        "use planning/types { TodoTitle }\n\n/// What adding a todo takes.\ntype NewTodo = Input {\n    title: TodoTitle\n}\n",
    ),
    (
        "domains/planning/events/todo_added.jadpo",
        "use planning/entities { Todo }\n\n/// A todo was added.\ntype TodoAdded = Event {\n    todo_id: Todo.id\n}\n",
    ),
    (
        "domains/planning/failures/todo_not_found.jadpo",
        "use planning/entities { Todo }\n\n/// No todo has that id.\ntype TodoNotFound = Failure {\n    kind: FailureKind.not_found\n    message: \"Todo not found.\"\n    client: {\n        todo_id: Todo.id\n    }\n}\n",
    ),
    (
        "domains/planning/failures/todo_already_done.jadpo",
        "/// The todo is done already.\ntype TodoAlreadyDone = Failure {\n    kind: FailureKind.conflict\n    message: \"That todo is done already.\"\n}\n",
    ),
    (
        "domains/planning/queries/find_todo.jadpo",
        r#"use planning/entities { Todo }
use planning/types { TodoView }
use planning/failures { TodoNotFound }

/// One todo.
query find_todo(todo_id: Todo.id) fails TodoNotFound -> TodoView {
    from: Todo
    where: Todo.id == todo_id
    missing: TodoNotFound {
        todo_id: todo_id
    }
}
"#,
    ),
    (
        "domains/planning/queries/open_todos.jadpo",
        r#"use planning/entities { Todo }
use planning/types { TodoStatus, TodoView }

/// The open todos, by title.
query open_todos() -> List<TodoView> {
    from: Todo
    where: Todo.status == TodoStatus.open
    order: [Todo.title asc]
    limit: 50
}
"#,
    ),
    (
        "domains/planning/queries/done_todos.jadpo",
        r#"use planning/entities { Todo }
use planning/types { TodoStatus, TodoView }

/// The done todos, by title.
query done_todos() -> List<TodoView> {
    from: Todo
    where: Todo.status == TodoStatus.done
    order: [Todo.title asc]
    limit: 50
}
"#,
    ),
    (
        "domains/planning/actions/add_todo.jadpo",
        r#"use planning/entities { Todo }
use planning/events { TodoAdded }
use planning/types { NewTodo, TodoStatus, TodoView }

/// Adds an open todo.
action add_todo(input: NewTodo) -> TodoView {
    var todo: Todo = Todo.create {
        set: {
            title: input.title
            status: TodoStatus.open
        }
    }
    emit TodoAdded {
        todo_id: todo.id
    }
    return TodoView {
        id: todo.id
        title: todo.title
        status: todo.status
    }
}
"#,
    ),
    (
        "domains/planning/actions/board.jadpo",
        r#"use planning/queries { done_todos, open_todos }
use planning/types { Board }

/// The open and the done todos.
action board() -> Board {
    return Board {
        open: open_todos()
        done: done_todos()
    }
}
"#,
    ),
    (
        "domains/planning/actions/complete_todo.jadpo",
        r#"use planning/entities { Todo }
use planning/failures { TodoAlreadyDone, TodoNotFound }
use planning/queries { find_todo }
use planning/types { TodoStatus, TodoView }

/// Marks a todo done.
action complete_todo(todo_id: Todo.id) fails TodoNotFound, TodoAlreadyDone -> TodoView {
    var todo: TodoView = attempt find_todo(todo_id)
    if (todo.status == TodoStatus.done) {
        reject TodoAlreadyDone
    }
    var done: Todo = attempt Todo.update(todo_id) {
        set: {
            status: TodoStatus.done
        }
    }
    return TodoView {
        id: done.id
        title: done.title
        status: done.status
    }
}
"#,
    ),
    (
        "domains/planning/actions/note_added.jadpo",
        r#"use planning/entities { Todo }
use planning/failures { TodoNotFound }
use planning/queries { find_todo }
use planning/types { TodoView }

/// Reads the todo that was added.
action note_added(todo_id: Todo.id) fails TodoNotFound -> Unit {
    var todo: TodoView = attempt find_todo(todo_id)
}
"#,
    ),
    (
        "domains/planning/subscribers/note_added.jadpo",
        "use planning/actions { note_added }\nuse planning/events { TodoAdded }\n\n/// Reads each todo added.\nsubscriber note_added {\n    on: TodoAdded\n    run: note_added(event.todo_id)\n}\n",
    ),
    (
        "domains/planning/routes/todos.jadpo",
        r#"use planning/entities { Todo }
use planning/actions { add_todo, board, complete_todo }
use planning/queries { open_todos }
use planning/types { Board, NewTodo, TodoView }

/// The todo list.
routes Todos {
    prefix: /todos

    /// Adds a todo.
    POST / {
        authentication: RouteAuthentication.none
        input: NewTodo
        output: TodoView
        run: add_todo(input)
    }

    /// The open todos.
    GET /open {
        authentication: RouteAuthentication.none
        output: List<TodoView>
        run: open_todos()
    }

    /// The board.
    GET /board {
        authentication: RouteAuthentication.none
        output: Board
        run: board()
    }

    /// Marks a todo done.
    POST /{todo_id: Todo.id}/done {
        authentication: RouteAuthentication.none
        output: TodoView
        run: complete_todo(todo_id)
    }
}
"#,
    ),
    (
        "policy.jadpo",
        "use planning\n\npolicy {\n    actions: {\n        planning.add_todo: [Access.public]\n        planning.board: [Access.public]\n        planning.complete_todo: [Access.public]\n        planning.note_added: [Access.public]\n    }\n}\n",
    ),
    (
        "domains/planning/actions/add_todo.test.jadpo",
        r#"use planning/types { NewTodo, TodoStatus, TodoView }

test "a new todo starts done" {
    var todo: TodoView = add_todo(NewTodo {
        title: "Walk the dog"
    })
    assert todo.status == TodoStatus.done
}
"#,
    ),
];

/// The project, migrated and recorded.
fn project() -> (tempdir::Dir, PathBuf) {
    let dir = tempdir::Dir::new("request-timeline");
    let root = dir.path().to_path_buf();
    for (path, text) in FILES {
        write(&root, path, text);
    }
    for args in [&["migrate", "new", "initial"][..], &["records", "update"]] {
        let done = jadpo(&root, args);
        assert_eq!(done.code, 0, "{args:?}: {}{}", done.stdout, done.stderr);
    }
    (dir, root)
}

/// `jadpo <args>`, which must succeed: its standard output.
fn ok(root: &Path, args: &[&str]) -> String {
    let done = jadpo(root, args);
    assert_eq!(done.code, 0, "{args:?}: {}{}", done.stdout, done.stderr);
    done.stdout
}

/// `jadpo <args> --json`'s data.
fn data(root: &Path, args: &[&str]) -> serde_json::Value {
    let mut args = args.to_vec();
    args.push("--json");
    let text = ok(root, &args);
    serde_json::from_str::<serde_json::Value>(&text).expect("json")["data"].clone()
}

/// Waits up to 30 seconds for `ready`.
fn eventually(what: &str, ready: impl Fn() -> bool) {
    let started = Instant::now();
    while !ready() {
        assert!(started.elapsed() < Duration::from_secs(30), "{what}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn jadpo_trace_shows_what_a_request_did_and_dev_flags_its_line() {
    if !jadpo_test::live() {
        return;
    }
    let (_dir, root) = project();
    let dev = Dev::start(&root);
    let added = dev.send("POST", "/todos", &[], &format!(r#"{{"title":"{TITLE}"}}"#));
    assert_eq!(added.status, 200, "{added:?}");
    let todo = added.json()["id"].as_str().expect("an id").to_string();
    let added_id = added.header("x-request-id").expect("an id").to_string();
    let done = dev.send("POST", &format!("/todos/{todo}/done"), &[], "");
    assert_eq!(done.status, 200, "{done:?}");
    let again = dev.send("POST", &format!("/todos/{todo}/done"), &[], "");
    assert_eq!(again.status, 409, "{again:?}");
    let again_id = again.header("x-request-id").expect("an id").to_string();
    let open = dev.send("GET", "/todos/open", &[], "");
    assert_eq!(open.status, 200, "{open:?}");
    let open_id = open.header("x-request-id").expect("an id").to_string();
    let board = dev.send("GET", "/todos/board", &[], "");
    assert_eq!(board.status, 200, "{board:?}");
    let board_id = board.header("x-request-id").expect("an id").to_string();

    // The subscriber's run joins the request that set it off.
    eventually("the subscriber ran", || {
        std::fs::read_to_string(root.join("build/dev/deliveries.jsonl"))
            .is_ok_and(|text| text.contains("succeeded"))
    });

    // Adding: the route, the action, the write with its SQL, and what it
    // committed and set off, by any start of its id.
    let short: String = added_id.chars().take(8).collect();
    let shown = ok(&root, &["trace", &short]);
    assert!(shown.starts_with("POST /todos  200"), "{shown}");
    assert!(shown.contains("not signed in"), "{shown}");
    assert!(
        shown.contains("route     domains/planning/routes/todos.jadpo:"),
        "{shown}"
    );
    assert!(shown.contains("action    add_todo"), "{shown}");
    assert!(
        shown.contains("domains/planning/actions/add_todo.jadpo:"),
        "{shown}"
    );
    assert!(shown.contains("write   Todo.create"), "{shown}");
    assert!(shown.contains("INSERT INTO \"todos\""), "{shown}");
    assert!(shown.contains("committed; emitted TodoAdded"), "{shown}");
    assert!(shown.contains("then      note_added"), "{shown}");
    assert!(shown.contains("succeeded"), "{shown}");
    assert!(!shown.contains(TITLE), "a value was shown: {shown}");

    // Finishing it twice: the failure, where it was raised, rolled back.
    let failed = ok(&root, &["trace", &again_id]);
    assert!(
        failed.starts_with(&format!("POST /todos/{}…/done  409 todo_already_done", &todo[..4])),
        "{failed}"
    );
    assert!(failed.contains("query   find_todo"), "{failed}");
    assert!(failed.contains("SELECT … FROM \"todos\""), "{failed}");
    assert!(failed.contains("1 row"), "{failed}");
    assert!(failed.contains(&format!("$1 {}…", &todo[..4])), "{failed}");
    assert!(failed.contains("fail    TodoAlreadyDone"), "{failed}");
    assert!(
        failed.contains("domains/planning/actions/complete_todo.jadpo:10"),
        "{failed}"
    );
    assert!(failed.contains("no events emitted"), "{failed}");
    let json = data(&root, &["trace", &again_id]);
    assert_eq!(json["schemaVersion"], 1, "{json}");
    assert_eq!(json["status"], 409, "{json}");
    assert_eq!(json["failure"]["name"], "TodoAlreadyDone", "{json}");
    assert_eq!(json["spans"][0]["kind"], "action", "{json}");
    assert_eq!(json["spans"][1]["name"], "find_todo", "{json}");
    assert_eq!(
        json["spans"][1]["statements"][0]["params"][0],
        todo.as_str(),
        "an identifier is shown as its value: {json}"
    );

    // The open todos: one query, which no index serves.
    let listed = ok(&root, &["trace", &open_id]);
    assert!(listed.contains("query   open_todos"), "{listed}");
    assert!(listed.contains("no index"), "{listed}");
    assert!(listed.contains("wrote nothing"), "{listed}");
    // The board: two queries.
    let both = data(&root, &["trace", &board_id]);
    assert_eq!(both["flags"]["queries"], 2, "{both}");
    // `jadpo dev`'s lines carry the flags.
    eventually("the board's line", || dev.log().contains(&board_id));
    let log = dev.log();
    let line = |id: &str| -> serde_json::Value {
        let text = log
            .lines()
            .find(|line| line.contains(&format!("\"requestId\":\"{id}\"")))
            .expect("its line");
        serde_json::from_str::<serde_json::Value>(text).expect("json")["data"].clone()
    };
    assert_eq!(line(&board_id)["flags"]["queries"], 2, "{log}");
    assert_eq!(
        line(&open_id)["flags"]["unindexed"],
        serde_json::json!(["open_todos"]),
        "{log}"
    );
    assert_eq!(line(&added_id)["flags"]["queries"], 0, "{log}");

    // Text is kept only as its keyed hash.
    let kept = std::fs::read_to_string(root.join("build/dev/timelines.jsonl")).expect("kept");
    assert!(!kept.contains(TITLE), "a title was kept");
    assert!(kept.contains("hmac-sha256:"), "{kept}");

    // An id no request has.
    let missing = jadpo(&root, &["trace", "req_ffffffff"]);
    assert_eq!(missing.code, 1, "{}", missing.stdout);
    assert!(
        missing.stdout.contains("cli.trace_arguments")
            || missing.stderr.contains("cli.trace_arguments"),
        "{}{}",
        missing.stdout,
        missing.stderr
    );
    drop(dev);
}

#[test]
fn keep_failed_keeps_a_failed_tests_trace_for_jadpo_trace() {
    if !jadpo_test::live() {
        return;
    }
    let (_dir, root) = project();
    let ran = jadpo(&root, &["test", "--keep-failed"]);
    assert_eq!(ran.code, 1, "{}{}", ran.stdout, ran.stderr);
    let report = format!("{}{}", ran.stdout, ran.stderr);
    let id = report
        .split("jadpo trace ")
        .nth(1)
        .and_then(|rest| rest.split_whitespace().next())
        .unwrap_or_else(|| panic!("no trace named: {report}"))
        .to_string();
    assert!(id.starts_with("t-"), "{report}");
    let shown = ok(&root, &["trace", &id]);
    assert!(
        shown.starts_with("test \"a new todo starts done\""),
        "{shown}"
    );
    assert!(shown.contains("action    add_todo"), "{shown}");
    assert!(shown.contains("write   Todo.create"), "{shown}");
    assert!(
        shown.contains("domains/planning/actions/add_todo.test.jadpo:"),
        "{shown}"
    );
    let ran = jadpo(&root, &["test", "--keep-failed", "--json"]);
    let json: serde_json::Value = serde_json::from_str(&ran.stdout).expect("json");
    let trace = json["data"]["tests"][0]["failure"]["trace"]
        .as_str()
        .expect("a trace");
    assert!(trace.starts_with("t-"), "{json}");
}
