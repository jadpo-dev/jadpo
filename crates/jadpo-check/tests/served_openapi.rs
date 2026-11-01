//! Each API's own OpenAPI document, served at `<prefix>/openapi.json`,
//! and the `x-jadpo` fields that make it importable (area 7, "OpenAPI";
//! chunk 7.2).

use jadpo_db::{Database, Layout};
use jadpo_graph::json::Value;

use crate::support;

const APP: &str =
    "application {\n    apis: {\n        main: {\n            prefix: /v1\n        }\n    }\n}\n";

const FILES: &[(&str, &str)] = &[
    (
        "domains/planning/types/todo_title.jadpo",
        "type TodoTitle = Text {\n    min_length: 1\n    max_length: 200\n}\n",
    ),
    (
        "domains/planning/entities/todo.jadpo",
        r#"use planning/types { TodoTitle }

entity Todo {
    id: Uuid
    title: TodoTitle
    done: Bool

    policy: {
        create: [Access.public]
        read: [Access.public]
    }
}
"#,
    ),
    (
        "domains/planning/types/todo_view.jadpo",
        "use planning/entities { Todo }\n\ntype TodoView = Output {\n    pick: [Todo.id, Todo.title, Todo.done]\n}\n",
    ),
    (
        "domains/planning/failures/todo_not_found.jadpo",
        r#"use planning/entities { Todo }

type TodoNotFound = Failure {
    kind: FailureKind.not_found
    message: "Todo not found."
    client: {
        todo_id: Todo.id
    }
}
"#,
    ),
    (
        "domains/planning/queries/find_todo.jadpo",
        r#"use planning/entities { Todo }
use planning/types { TodoView }
use planning/failures { TodoNotFound }

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
        "domains/planning/actions/create_todo.jadpo",
        r#"use planning/entities { Todo }
use planning/types { TodoTitle, TodoView }

type CreateTodo = Input {
    title: TodoTitle
}

action create_todo(input: CreateTodo) -> TodoView {
    var todo: Todo = Todo.create {
        set: {
            title: input.title
            done: false
        }
    }
    return TodoView {
        id: todo.id
        title: todo.title
        done: todo.done
    }
}
"#,
    ),
    (
        "domains/planning/routes/v1_todos.jadpo",
        r#"use planning/entities { Todo }
use planning/types { TodoView }
use planning/actions { CreateTodo, create_todo }
use planning/queries { find_todo }

/// Todos: making one and reading one back.
routes V1Todos {
    prefix: /v1/todos

    POST / {
        authentication: RouteAuthentication.none
        input: CreateTodo
        output: TodoView
        status: SuccessStatus.created
        run: create_todo(input)
    }

    GET /{todo_id: Todo.id} {
        authentication: RouteAuthentication.none
        output: TodoView
        run: find_todo(todo_id)
    }
}
"#,
    ),
];

/// The problems, less the record `support::project` doesn't write.
fn problems(files: &[(&str, &str)]) -> Vec<String> {
    support::problems(Layout::Project, files)
        .into_iter()
        .filter(|problem| !problem.contains("record.out_of_date"))
        .collect()
}

fn with_app(app: &str) -> Vec<(&str, &str)> {
    let mut files = vec![("app.jadpo", app)];
    files.extend_from_slice(FILES);
    files
}

fn document(app: &str) -> Value {
    let db = Database::default();
    let project = support::project(&db, Layout::Project, &with_app(app));
    jadpo_check::routes::openapi::document(&db, project, "main", "todos", "1").expect("a document")
}

fn at<'v>(value: &'v Value, path: &[&str]) -> Option<&'v Value> {
    path.iter().try_fold(value, |value, key| value.get(key))
}

#[test]
fn every_api_serves_its_document_under_its_prefix() {
    let db = Database::default();
    let project = support::project(&db, Layout::Project, &with_app(APP));
    let record = jadpo_check::routes::record::record_texts(&db, project);
    let text = record.get("main").expect("a record");
    assert!(
        text.contains("\nGET /v1/openapi.json\n    generated openapi\n    authentication none\n    status 200\n"),
        "{text}"
    );
    let served = document(APP);
    let operation = at(&served, &["paths", "/v1/openapi.json", "get"]).expect("served");
    assert_eq!(operation.get("x-jadpo-generated"), Some(&Value::Bool(true)));
    assert_eq!(operation.get("x-jadpo-changes"), Some(&Value::Bool(false)));
    // Nothing signs anyone in, so every route is open and so is this.
    assert_eq!(operation.get("security"), Some(&Value::Array(Vec::new())));
    assert!(at(operation, &["responses", "401"]).is_none());
}

#[test]
fn serve_openapi_false_turns_it_off() {
    let app = "application {\n    apis: {\n        main: {\n            prefix: /v1\n            serve_openapi: false\n        }\n    }\n}\n";
    assert_eq!(problems(&with_app(app)), Vec::<String>::new());
    let db = Database::default();
    let project = support::project(&db, Layout::Project, &with_app(app));
    let record = jadpo_check::routes::record::record_texts(&db, project);
    assert!(!record.get("main").expect("a record").contains("openapi"));
    assert!(at(&document(app), &["paths", "/v1/openapi.json"]).is_none());
}

#[test]
fn serve_openapi_takes_true_or_false() {
    let app = "application {\n    apis: {\n        main: {\n            prefix: /v1\n            serve_openapi: \"no\"\n        }\n    }\n}\n";
    assert_eq!(
        problems(&with_app(app)),
        vec!["app.jadpo api.serve_openapi_invalid: `\"no\"` isn't `true` or `false`".to_string()]
    );
}

#[test]
fn a_route_where_the_document_is_served_never_runs() {
    let mut files = with_app(APP);
    files.push((
        "domains/planning/queries/all_todos.jadpo",
        r#"use planning/entities { Todo }
use planning/types { TodoView }

query all_todos() -> List<TodoView> {
    from: Todo
}
"#,
    ));
    files.push((
        "domains/planning/routes/v1.jadpo",
        r#"use planning/queries { all_todos }
use planning/types { TodoView }

routes V1 {
    prefix: /v1

    GET /openapi.json {
        authentication: RouteAuthentication.none
        output: List<TodoView>
        run: all_todos()
    }
}
"#,
    ));
    let problems = problems(&files);
    assert!(
        problems.iter().any(|problem| problem
            == "domains/planning/routes/v1.jadpo route.path_overlap: `GET /v1/openapi.json` is where API `main` serves its OpenAPI document"),
        "{problems:#?}"
    );
}

#[test]
fn the_document_carries_the_x_jadpo_fields() {
    let document = document(APP);
    let create = at(&document, &["paths", "/v1/todos", "post"]).expect("create");
    assert_eq!(create.get("x-jadpo-changes"), Some(&Value::Bool(true)));
    assert_eq!(
        create.get("tags"),
        Some(&Value::Array(vec![Value::from("V1Todos")]))
    );
    let idempotency = create.get("x-jadpo-idempotency").expect("idempotency");
    assert_eq!(
        idempotency.get("header"),
        Some(&Value::from("Idempotency-Key"))
    );
    assert_eq!(idempotency.get("required"), Some(&Value::Bool(false)));
    assert_eq!(idempotency.get("keeps"), Some(&Value::from("response")));
    assert_eq!(idempotency.get("window"), Some(&Value::from("1d")));
    assert_eq!(idempotency.get("in_use"), Some(&Value::from("conflict")));
    assert_eq!(
        idempotency.get("reused"),
        Some(&Value::from("idempotency_key_reused"))
    );
    let find = at(&document, &["paths", "/v1/todos/{todo_id}", "get"]).expect("find");
    assert_eq!(find.get("x-jadpo-changes"), Some(&Value::Bool(false)));
    assert!(find.get("x-jadpo-idempotency").is_none());
    let placeholder = find
        .get("parameters")
        .and_then(Value::as_array)
        .and_then(|all| all.first())
        .and_then(|first| first.get("schema"))
        .and_then(|schema| schema.get("x-jadpo-constraints"));
    assert_eq!(placeholder, Some(&Value::from("Uuid")));
    let failure = at(
        &document,
        &["components", "schemas", "TodoNotFound", "x-jadpo-failure"],
    )
    .expect("the failure's code and kind");
    assert_eq!(failure.get("code"), Some(&Value::from("todo_not_found")));
    assert_eq!(failure.get("kind"), Some(&Value::from("not_found")));
    assert_eq!(
        at(
            &document,
            &[
                "components",
                "schemas",
                "CreateTodo",
                "properties",
                "title",
                "x-jadpo-constraints"
            ]
        ),
        Some(&Value::from("Text { min_length: 1, max_length: 200 }"))
    );
    let tags = document
        .get("tags")
        .and_then(Value::as_array)
        .expect("tags");
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].get("name"), Some(&Value::from("V1Todos")));
    assert_eq!(
        tags[0].get("description"),
        Some(&Value::from("Todos: making one and reading one back."))
    );
}

#[test]
fn an_api_that_signs_callers_in_needs_one_to_read_its_document() {
    let config = (
        "config.jadpo",
        "config {\n    session_signing_key: SigningKey\n}\n",
    );
    let app = r#"use planning/entities { Todo }

application {
    apis: {
        main: {
            prefix: /v1
        }
    }
}

principal {
    service: {
        subject: Todo.title
    }
}
"#;
    let authentication = (
        "authentication.jadpo",
        "authentication {\n    service_key: {\n        signing_key: config.session_signing_key\n    }\n}\n",
    );
    let mut files = with_app(app);
    files.push(config);
    files.push(authentication);
    let db = Database::default();
    let project = support::project(&db, Layout::Project, &files);
    let record = jadpo_check::routes::record::record_texts(&db, project);
    let text = record.get("main").expect("a record");
    assert!(
        text.contains("\nGET /v1/openapi.json\n    generated openapi\n    status 200\n"),
        "{text}"
    );
    let document = jadpo_check::routes::openapi::document(&db, project, "main", "todos", "1")
        .expect("a document");
    let operation = at(&document, &["paths", "/v1/openapi.json", "get"]).expect("served");
    assert!(operation.get("security").is_none());
    assert!(at(operation, &["responses", "401"]).is_some());
}

/// A field's own rules, written inline (`reply: Text { max_length: 2000 }`),
/// reach its schema as a named type's do: the schema's keywords and its
/// `x-jadpo-constraints` (chunk 7.7d).
#[test]
fn a_field_s_inline_rules_reach_its_schema() {
    let mut files = with_app(APP);
    files.push((
        "domains/planning/actions/answer_todo.jadpo",
        r#"type Answer = Input {
    reply: Text {
        min_length: 1
        max_length: 2000
    }
    code: Text {
        pattern: "[A-Z]{3}"
    }
    stars: Int {
        min: 1
        max: 5
    }
    tags: List<Text> {
        max_length: 4
    }
    note: Text? {
        max_length: 50
    }
}

type Answered = Output {
    reply: Text {
        min_length: 1
        max_length: 2000
    }
}

action answer_todo(input: Answer) -> Answered {
    return Answered {
        reply: input.reply
    }
}
"#,
    ));
    files.push((
        "domains/planning/routes/v1_answers.jadpo",
        r#"use planning/actions { Answer, Answered, answer_todo }

routes V1Answers {
    prefix: /v1/answers

    POST / {
        authentication: RouteAuthentication.none
        input: Answer
        output: Answered
        run: answer_todo(input)
    }
}
"#,
    ));
    files.push((
        "policy.jadpo",
        "use planning\n\npolicy {\n    actions: {\n        planning.answer_todo: [Access.public]\n    }\n}\n",
    ));
    assert_eq!(problems(&files), Vec::<String>::new());
    let db = Database::default();
    let project = support::project(&db, Layout::Project, &files);
    let document = jadpo_check::routes::openapi::document(&db, project, "main", "todos", "1")
        .expect("a document");
    let field = |name: &str| {
        at(
            &document,
            &["components", "schemas", "Answer", "properties", name],
        )
        .cloned()
        .unwrap_or_else(|| panic!("`{name}` in {document:#?}"))
    };
    let reply = field("reply");
    assert_eq!(
        reply.get("maxLength"),
        Some(&Value::from(2000u32)),
        "{reply:#?}"
    );
    assert_eq!(
        reply.get("minLength"),
        Some(&Value::from(1u32)),
        "{reply:#?}"
    );
    assert_eq!(
        reply.get("x-jadpo-constraints"),
        Some(&Value::from("Text { min_length: 1, max_length: 2000 }"))
    );
    let code = field("code");
    assert_eq!(
        code.get("pattern"),
        Some(&Value::from("^(?:[A-Z]{3})$")),
        "{code:#?}"
    );
    let stars = field("stars");
    assert_eq!(stars.get("minimum"), Some(&Value::from(1i64)), "{stars:#?}");
    assert_eq!(stars.get("maximum"), Some(&Value::from(5i64)), "{stars:#?}");
    let tags = field("tags");
    assert_eq!(tags.get("maxItems"), Some(&Value::from(4u32)), "{tags:#?}");
    let answered = at(
        &document,
        &["components", "schemas", "Answered", "properties", "reply"],
    )
    .unwrap_or_else(|| panic!("{document:#?}"));
    assert_eq!(
        answered.get("maxLength"),
        Some(&Value::from(2000u32)),
        "{answered:#?}"
    );
    let note = field("note");
    let some = note
        .get("anyOf")
        .and_then(Value::as_array)
        .and_then(|options| options.first())
        .unwrap_or_else(|| panic!("{note:#?}"));
    assert_eq!(
        some.get("maxLength"),
        Some(&Value::from(50u32)),
        "{note:#?}"
    );
}

const CLIENT_FLAG: &str = r#"flags {
    /// The one-page checkout, shown by the browser.
    new_checkout: Bool {
        default: false
        owner: "payments"
        expires: "2099-01-31"
        client: true
    }
}
"#;

#[test]
fn each_api_answers_its_client_flags_at_get_prefix_flags() {
    let mut files = with_app(APP);
    files.push(("flags.jadpo", CLIENT_FLAG));
    let db = Database::default();
    let project = support::project(&db, Layout::Project, &files);
    let record = jadpo_check::routes::record::record_texts(&db, project);
    let text = record.get("main").expect("a record");
    assert!(
        text.contains("\nGET /v1/flags\n    generated flags\n    authentication none\n    status 200\n    flag new_checkout: Bool\n"),
        "{text}"
    );
    let document = jadpo_check::routes::openapi::document(&db, project, "main", "todos", "1")
        .expect("a document");
    let operation = at(&document, &["paths", "/v1/flags", "get"]).expect("the flags");
    assert_eq!(operation.get("x-jadpo-generated"), Some(&Value::Bool(true)));
    let schema = at(
        operation,
        &["responses", "200", "content", "application/json", "schema"],
    )
    .expect("a schema");
    assert_eq!(
        at(schema, &["properties", "new_checkout", "type"]),
        Some(&Value::from("boolean"))
    );
    // A flag without `client: true` never leaves the server.
    let quiet = CLIENT_FLAG.replace("        client: true\n", "");
    let mut files = with_app(APP);
    files.push(("flags.jadpo", quiet.as_str()));
    let db = Database::default();
    let project = support::project(&db, Layout::Project, &files);
    let record = jadpo_check::routes::record::record_texts(&db, project);
    assert!(!record.get("main").expect("a record").contains("flags"));
}

#[test]
fn a_route_where_the_flags_are_answered_never_runs() {
    let mut files = with_app(APP);
    files.push(("flags.jadpo", CLIENT_FLAG));
    files.push((
        "domains/planning/queries/all_todos.jadpo",
        r#"use planning/entities { Todo }
use planning/types { TodoView }

query all_todos() -> List<TodoView> {
    from: Todo
}
"#,
    ));
    files.push((
        "domains/planning/routes/v1.jadpo",
        r#"use planning/queries { all_todos }
use planning/types { TodoView }

routes V1 {
    prefix: /v1

    GET /flags {
        authentication: RouteAuthentication.none
        output: List<TodoView>
        run: all_todos()
    }
}
"#,
    ));
    let problems = problems(&files);
    assert!(
        problems.iter().any(|problem| problem
            == "domains/planning/routes/v1.jadpo route.path_overlap: `GET /v1/flags` is where API `main` answers its flags"),
        "{problems:#?}"
    );
}
