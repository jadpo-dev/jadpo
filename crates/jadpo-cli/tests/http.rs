//! Routes served over real HTTP (area 7): `jadpo dev` runs a project in the
//! interpreter on the native host, against live PostgreSQL, and each test
//! sends requests over TCP as any client would, reading the status, the
//! headers and the JSON: a created record and reading it back, the failure
//! envelope with its `request_id` and `x-request-id`, input problems as
//! `{ field, rule }`, a `GET`'s input from the query string and a header,
//! `HEAD` and `OPTIONS`, unknown routes and methods, the default rate limit,
//! a save swapping in a new version while the last good one keeps
//! serving through an error, and the request guarantees (chunk 4.2):
//! health, the body limit, a retried write answered from its
//! `Idempotency-Key`, and a write held past its deadline by another
//! transaction's lock, which commits nothing. The project's OpenAPI document is compared
//! with its golden copy (`JADPO_BLESS=1` rewrites it).
//!
//! The server is the one `JADPO_TEST_DATABASE_URL` names or one started
//! from the machine's PostgreSQL tools; `JADPO_SKIP_POSTGRES=1` skips the
//! live tests.

use std::io::{Read, Write as _};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use jadpo_cli::dev::{DevArgs, serve_to};
use jadpo_cli::{Output, run};
use jadpo_term::{ColorChoice, Environment};

fn skipped() -> bool {
    let skip = std::env::var("JADPO_SKIP_POSTGRES").is_ok_and(|value| value == "1");
    if skip {
        println!("skipped: JADPO_SKIP_POSTGRES is set");
    }
    skip
}

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    std::fs::create_dir_all(path.parent().expect("parent")).expect("folders");
    std::fs::write(path, text).expect("write");
}

fn jadpo(cwd: &Path, args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let env = Environment::empty()
        .with_var("SOURCE_DATE_EPOCH", "1792504800")
        .with_cwd(cwd);
    run(&args, &env)
}

const FILES: &[(&str, &str)] = &[
    (
        "app.jadpo",
        r#"application {}
"#,
    ),
    (
        "domains/planning/actions/create_todo.jadpo",
        r#"use planning/entities { Todo }
use planning/types { TodoStatus, TodoTitle, TodoView }

action create_todo(input: CreateTodo) -> TodoView {
    var todo: Todo = Todo.create {
        set: {
            title: input.title
            status: TodoStatus.open
        }
    }
    return TodoView {
        id: todo.id
        title: todo.title
        status: todo.status
    }
}

/// What creating a todo takes.
type CreateTodo = Input {
    title: TodoTitle
}
"#,
    ),
    (
        "domains/planning/entities/todo.jadpo",
        r#"use planning/types { TodoStatus, TodoTitle }

entity Todo {
    id: Uuid
    title: TodoTitle
    status: TodoStatus
}
"#,
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
        "domains/planning/queries/list_todos.jadpo",
        r#"use planning/entities { Todo }
use planning/types { TodoLimit, TodoStatus, TodoTitle, TodoView }

/// Todos, by title.
query list_todos(input: ListTodos) -> List<TodoView> {
    from: Todo
    where_present: Todo.status in input.status
    order: [Todo.title asc]
    limit: input.limit
}

/// Which todos to list.
type ListTodos = Input {
    status: List<TodoStatus> {
        optional: true
    }
    limit: TodoLimit {
        default: 20
    }
    request_tag: TodoTitle {
        header: "X-Request-Tag"
        optional: true
    }
}
"#,
    ),
    (
        "domains/planning/routes/todos.jadpo",
        r#"use planning/entities { Todo }
use planning/types { TodoView }
use planning/actions { CreateTodo, create_todo }
use planning/queries { ListTodos, find_todo, list_todos }

/// Todos: making one and reading them back.
routes Todos {
    prefix: /todos

    /// Creates a todo.
    POST / {
        authentication: RouteAuthentication.none
        input: CreateTodo
        output: TodoView
        status: SuccessStatus.created
        run: create_todo(input)
    }

    /// Todos, by title.
    GET / {
        authentication: RouteAuthentication.none
        input: ListTodos
        output: List<TodoView>
        run: list_todos(input)
    }

    /// One todo.
    GET /{todo_id: Todo.id} {
        authentication: RouteAuthentication.none
        output: TodoView
        run: find_todo(todo_id)
    }
}
"#,
    ),
    (
        "domains/planning/types/todo_limit.jadpo",
        r#"type TodoLimit = Int {
    min: 1
    max: 50
}
"#,
    ),
    (
        "domains/planning/types/todo_status.jadpo",
        r#"/// Where a todo is.
type TodoStatus = Enum {
    open
    done
}
"#,
    ),
    (
        "domains/planning/types/todo_title.jadpo",
        r#"type TodoTitle = Text {
    min_length: 1
    max_length: 200
}
"#,
    ),
    (
        "domains/planning/types/todo_view.jadpo",
        r#"use planning/entities { Todo }

/// A todo as clients see it.
type TodoView = Output {
    pick: [Todo.id, Todo.title, Todo.status]
}
"#,
    ),
];

/// The project, migrated and recorded, checking cleanly.
fn project() -> (tempdir::Dir, PathBuf) {
    let dir = tempdir::Dir::new("http");
    let root = dir.path().to_path_buf();
    for (path, text) in FILES {
        write(&root, path, text);
    }
    let migrated = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(migrated.code, 0, "{}{}", migrated.stdout, migrated.stderr);
    let recorded = jadpo(&root, &["api", "update"]);
    assert_eq!(recorded.code, 0, "{}{}", recorded.stdout, recorded.stderr);
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    (dir, root)
}

/// A folder removed when the test ends.
mod tempdir {
    use std::path::{Path, PathBuf};

    pub struct Dir(PathBuf);

    impl Dir {
        pub fn new(stem: &str) -> Dir {
            static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let path =
                std::env::temp_dir().join(format!("jadpo-{stem}-{}-{n}", std::process::id()));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("temp folder");
            Dir(path)
        }

        pub fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

/// What `jadpo dev` writes, shared with the test.
#[derive(Clone, Default)]
struct Buffer(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for Buffer {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().expect("buffer").extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Buffer {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().expect("buffer")).into_owned()
    }
}

/// A running `jadpo dev --json` on a database of its own.
struct Dev {
    port: u16,
    /// The database it serves.
    url: String,
    out: Buffer,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<u8>>,
}

impl Dev {
    fn start(root: &Path) -> Dev {
        let server = jadpo_test::database::server().expect("a PostgreSQL server");
        let url = server.create_database("http").expect("a database");
        let out = Buffer::default();
        let stop = Arc::new(AtomicBool::new(false));
        let args = DevArgs {
            cwd: root.to_path_buf(),
            port: 0,
            url: Some(url.clone()),
            no_run: false,
            json: true,
            choice: ColorChoice::Never,
        };
        let thread = {
            let mut out = out.clone();
            let stop = Arc::clone(&stop);
            std::thread::spawn(move || {
                let mut err = Buffer::default();
                serve_to(&args, &Environment::empty(), &mut out, &mut err, &|| {
                    stop.load(Ordering::SeqCst)
                })
            })
        };
        let started = Instant::now();
        let port = loop {
            let text = out.text();
            if let Some(line) = text.lines().find(|line| line.contains("\"listening\"")) {
                let document: serde_json::Value = serde_json::from_str(line).expect("json");
                let url = document["data"]["url"].as_str().expect("a url").to_string();
                break url
                    .rsplit(':')
                    .next()
                    .expect("a port")
                    .parse()
                    .expect("a number");
            }
            assert!(
                started.elapsed() < Duration::from_secs(60),
                "jadpo dev didn't start:\n{text}"
            );
            std::thread::sleep(Duration::from_millis(20));
        };
        Dev {
            port,
            url,
            out,
            stop,
            thread: Some(thread),
        }
    }

    fn send(&self, method: &str, path: &str, headers: &[(&str, &str)], body: &str) -> Reply {
        send(self.port, method, path, headers, body)
    }
}

impl Drop for Dev {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// A response as a client reads it.
#[derive(Debug)]
struct Reply {
    status: u16,
    headers: Vec<(String, String)>,
    body: String,
}

impl Reply {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(header, _)| header.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    fn json(&self) -> serde_json::Value {
        serde_json::from_str(&self.body).unwrap_or_else(|_| panic!("not JSON: {}", self.body))
    }
}

/// Sends one HTTP/1.1 request and reads the whole response.
fn send(port: u16, method: &str, path: &str, headers: &[(&str, &str)], body: &str) -> Reply {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connects");
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .expect("timeout");
    let mut request = format!(
        "{method} {path} HTTP/1.1\r\nhost: localhost\r\nconnection: close\r\ncontent-length: {}\r\n",
        body.len()
    );
    for (name, value) in headers {
        request.push_str(&format!("{name}: {value}\r\n"));
    }
    request.push_str("\r\n");
    request.push_str(body);
    stream.write_all(request.as_bytes()).expect("sends");
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).expect("reads");
    let text = String::from_utf8_lossy(&raw).into_owned();
    let (head, body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
    let mut lines = head.lines();
    let status = lines
        .next()
        .and_then(|line| line.split(' ').nth(1))
        .and_then(|code| code.parse().ok())
        .expect("a status line");
    let headers = lines
        .filter_map(|line| line.split_once(": "))
        .map(|(name, value)| (name.to_string(), value.to_string()))
        .collect();
    Reply {
        status,
        headers,
        body: body.to_string(),
    }
}

#[test]
fn a_created_record_reads_back_and_failures_come_in_the_envelope() {
    if skipped() {
        return;
    }
    let (_dir, root) = project();
    let dev = Dev::start(&root);

    let created = dev.send("POST", "/todos", &[], r#"{"title":"Buy milk"}"#);
    assert_eq!(created.status, 201, "{created:?}");
    assert_eq!(created.header("content-type"), Some("application/json"));
    let id = created.header("x-request-id").expect("a request id");
    assert!(id.starts_with("req_") && id.len() == 20, "{id}");
    let todo = created.json();
    assert_eq!(todo["title"], "Buy milk");
    assert_eq!(todo["status"], "open");
    let todo_id = todo["id"].as_str().expect("an id").to_string();

    let read = dev.send("GET", &format!("/todos/{todo_id}"), &[], "");
    assert_eq!(read.status, 200, "{read:?}");
    assert_eq!(read.json(), todo);

    let missing = "0192f0c4-3b5e-7c1a-9d2e-4f6a8b0c1d2e";
    let not_found = dev.send("GET", &format!("/todos/{missing}"), &[], "");
    assert_eq!(not_found.status, 404);
    let error = &not_found.json()["error"];
    assert_eq!(error["code"], "todo_not_found");
    assert_eq!(error["message"], "Todo not found.");
    assert_eq!(error["details"]["todo_id"], missing);
    assert_eq!(
        error["request_id"].as_str(),
        not_found.header("x-request-id"),
        "the body and the header carry one id"
    );

    let invalid = dev.send("POST", "/todos", &[], r#"{"title":"","colour":"red"}"#);
    assert_eq!(invalid.status, 422);
    let error = &invalid.json()["error"];
    assert_eq!(error["code"], "invalid_input");
    assert_eq!(
        error["details"]["problems"],
        serde_json::json!([
            { "field": "title", "rule": "min_length" },
            { "field": "colour", "rule": "unknown_field" }
        ])
    );
    let bad_id = dev.send("GET", "/todos/not-an-id", &[], "");
    assert_eq!(bad_id.status, 422);
    assert_eq!(
        bad_id.json()["error"]["details"]["problems"][0],
        serde_json::json!({ "field": "todo_id", "rule": "format" })
    );
    let unreadable = dev.send("POST", "/todos", &[], "{not json");
    assert_eq!(unreadable.status, 400);
    assert_eq!(unreadable.json()["error"]["code"], "invalid_request");

    let nowhere = dev.send("GET", "/nowhere", &[], "");
    assert_eq!(nowhere.status, 404);
    assert_eq!(nowhere.json()["error"]["code"], "route_not_found");
    let wrong_method = dev.send("DELETE", "/todos", &[], "");
    assert_eq!(wrong_method.status, 405);
    assert_eq!(
        wrong_method.header("allow"),
        Some("GET, HEAD, OPTIONS, POST")
    );
    let options = dev.send("OPTIONS", "/todos", &[], "");
    assert_eq!(options.status, 204);
    assert_eq!(options.header("allow"), Some("GET, HEAD, OPTIONS, POST"));
    let head = dev.send("HEAD", &format!("/todos/{todo_id}"), &[], "");
    assert_eq!((head.status, head.body.as_str()), (200, ""));

    // Each request is a line of `--json`, with its id.
    let lines = dev.out.text();
    assert!(
        lines.contains(&format!("\"requestId\":\"{id}\"")),
        "{lines}"
    );
}

#[test]
fn a_get_reads_its_input_from_the_query_string_and_its_headers() {
    if skipped() {
        return;
    }
    let (_dir, root) = project();
    let dev = Dev::start(&root);
    for title in ["Walk the dog", "Buy milk", "Call home"] {
        let created = dev.send("POST", "/todos", &[], &format!(r#"{{"title":"{title}"}}"#));
        assert_eq!(created.status, 201, "{created:?}");
    }
    let listed = dev.send("GET", "/todos?status=open&status=done&limit=2", &[], "");
    assert_eq!(listed.status, 200, "{listed:?}");
    let titles: Vec<String> = listed
        .json()
        .as_array()
        .expect("a list")
        .iter()
        .map(|todo| todo["title"].as_str().unwrap_or_default().to_string())
        .collect();
    assert_eq!(titles.len(), 2);
    assert_eq!(titles[0], "Buy milk", "ordered by title, two of them");

    let tagged = dev.send("GET", "/todos", &[("X-Request-Tag", "from a header")], "");
    assert_eq!(tagged.status, 200, "{tagged:?}");
    assert_eq!(
        tagged.json().as_array().map(Vec::len),
        Some(3),
        "the default limit"
    );

    let problems =
        |path: &str| dev.send("GET", path, &[], "").json()["error"]["details"]["problems"].clone();
    assert_eq!(
        problems("/todos?limit=0"),
        serde_json::json!([{ "field": "limit", "rule": "min" }])
    );
    assert_eq!(
        problems("/todos?status=closed"),
        serde_json::json!([{ "field": "status.0", "rule": "variant" }])
    );
    assert_eq!(
        problems("/todos?request_tag=x"),
        serde_json::json!([{ "field": "request_tag", "rule": "unknown_field" }]),
        "a header field never comes in the query string"
    );
}

#[test]
fn an_address_is_limited_to_its_allowance() {
    if skipped() {
        return;
    }
    let (_dir, root) = project();
    let dev = Dev::start(&root);
    let mut statuses = Vec::new();
    for _ in 0..121 {
        statuses.push(dev.send("GET", "/todos", &[], "").status);
    }
    assert!(
        statuses[..120].iter().all(|status| *status == 200),
        "{statuses:?}"
    );
    let refused = dev.send("GET", "/todos", &[], "");
    assert_eq!(refused.status, 429);
    assert_eq!(refused.json()["error"]["code"], "rate_limited");
    let retry: u64 = refused
        .header("retry-after")
        .expect("Retry-After")
        .parse()
        .expect("seconds");
    assert!((1..=60).contains(&retry), "{retry}");
}

#[test]
fn a_save_swaps_in_the_new_version_and_errors_keep_the_last_good_one() {
    if skipped() {
        return;
    }
    let (_dir, root) = project();
    let dev = Dev::start(&root);
    let routes = root.join("domains/planning/routes/todos.jadpo");
    let text = std::fs::read_to_string(&routes).expect("the routes");
    // An error: the last good version keeps answering.
    std::fs::write(&routes, text.replace("SuccessStatus.created", "201")).expect("save");
    let errors = wait_for(&dev, "\"errors\"");
    assert!(errors.contains("route.status_invalid"), "{errors}");
    let created = dev.send("POST", "/todos", &[], r#"{"title":"Still served"}"#);
    assert_eq!(created.status, 201);
    // Fixed differently: the new version answers.
    std::fs::write(
        &routes,
        text.replace("SuccessStatus.created", "SuccessStatus.accepted"),
    )
    .expect("save");
    wait_for(&dev, "\"reloaded\"");
    let accepted = dev.send("POST", "/todos", &[], r#"{"title":"Reloaded"}"#);
    assert_eq!(accepted.status, 202);
}

/// Waits for `jadpo dev` to write a line holding `needle`.
fn wait_for(dev: &Dev, needle: &str) -> String {
    let started = Instant::now();
    loop {
        let text = dev.out.text();
        if let Some(line) = text.lines().find(|line| line.contains(needle)) {
            return line.to_string();
        }
        assert!(
            started.elapsed() < Duration::from_secs(30),
            "no {needle} line:\n{text}"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn the_openapi_document_matches_its_golden_copy() {
    let dir = tempdir::Dir::new("openapi");
    let root = dir.path().join("todos");
    for (path, text) in FILES {
        write(&root, path, text);
    }
    let migrated = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(migrated.code, 0, "{}", migrated.stdout);
    assert_eq!(jadpo(&root, &["api", "update"]).code, 0);
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}", checked.stdout);
    let document =
        std::fs::read_to_string(root.join("build/openapi/main.json")).expect("the document");
    let record = std::fs::read_to_string(root.join("api/main.api")).expect("the record");
    let goldens = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/goldens");
    for (name, text) in [
        ("todos.openapi.json", &document),
        ("todos.main.api", &record),
    ] {
        let path = goldens.join(name);
        if std::env::var_os("JADPO_BLESS").is_some() {
            std::fs::create_dir_all(&goldens).expect("goldens folder");
            std::fs::write(&path, text).expect("bless");
            continue;
        }
        let saved = std::fs::read_to_string(&path).unwrap_or_default();
        assert_eq!(
            &saved, text,
            "{name} changed; JADPO_BLESS=1 cargo test -p jadpo-cli --test integration http:: rewrites it"
        );
    }
}

#[test]
fn health_is_the_runtimes_and_a_body_past_the_limit_is_refused() {
    if skipped() {
        return;
    }
    let (_dir, root) = project();
    let dev = Dev::start(&root);
    let live = dev.send("GET", "/health/live", &[], "");
    assert_eq!(live.status, 200, "{live:?}");
    assert_eq!(live.json(), serde_json::json!({ "status": "live" }));
    let ready = dev.send("GET", "/health/ready", &[], "");
    assert_eq!(ready.status, 200, "{ready:?}");
    assert_eq!(
        ready.json(),
        serde_json::json!({ "status": "ready", "stores": { "main": "ready" }, "integrations": {} })
    );
    let big = format!(r#"{{"title":"{}"}}"#, "x".repeat(1024 * 1024));
    let refused = dev.send("POST", "/todos", &[], &big);
    assert_eq!(refused.status, 413, "{}", refused.body);
    assert_eq!(refused.json()["error"]["code"], "request_too_large");
    let listed = dev.send("GET", "/todos", &[], "");
    assert_eq!(listed.json(), serde_json::json!([]));
}

#[test]
fn a_retried_write_is_answered_from_its_idempotency_key() {
    if skipped() {
        return;
    }
    let (_dir, root) = project();
    let dev = Dev::start(&root);
    let key = [("Idempotency-Key", "order-1")];
    let first = dev.send("POST", "/todos", &key, r#"{"title":"Buy milk"}"#);
    assert_eq!(first.status, 201, "{first:?}");
    let again = dev.send("POST", "/todos", &key, r#"{"title":"Buy milk"}"#);
    assert_eq!(again.status, 201, "{again:?}");
    assert_eq!(again.body, first.body, "the first response, as it was sent");
    assert_ne!(
        again.header("x-request-id"),
        first.header("x-request-id"),
        "a request of its own"
    );
    let listed = dev.send("GET", "/todos", &[], "");
    assert_eq!(listed.json().as_array().map(Vec::len), Some(1), "one todo");
    let other = dev.send("POST", "/todos", &key, r#"{"title":"Buy eggs"}"#);
    assert_eq!(other.status, 422, "{other:?}");
    assert_eq!(other.json()["error"]["code"], "idempotency_key_reused");
    let bad = dev.send(
        "POST",
        "/todos",
        &[("Idempotency-Key", "")],
        r#"{"title":"x"}"#,
    );
    assert_eq!(bad.status, 400);
    // A `GET` ignores a key.
    let read = dev.send("GET", "/todos", &key, "");
    assert_eq!(read.status, 200);
}

const NOTES: &[(&str, &str)] = &[
    ("app.jadpo", "application {}\n"),
    (
        "domains/notes/entities/note.jadpo",
        "use notes/types { NoteText }\n\nentity Note {\n    id: Uuid\n    text: NoteText\n}\n",
    ),
    (
        "domains/notes/types/note_text.jadpo",
        "type NoteText = Text {\n    min_length: 1\n    max_length: 200\n}\n",
    ),
    (
        "domains/notes/types/note_input.jadpo",
        "use notes/types { NoteText }\n\n/// What a note says.\ntype NoteInput = Input {\n    text: NoteText\n}\n",
    ),
    (
        "domains/notes/types/note_view.jadpo",
        "use notes/entities { Note }\n\n/// A note as clients see it.\ntype NoteView = Output {\n    pick: [Note.id, Note.text]\n}\n",
    ),
    (
        "domains/notes/actions/add_note.jadpo",
        "use notes/entities { Note }\nuse notes/types { NoteInput, NoteView }\n\n/// Adds a note.\naction add_note(input: NoteInput) -> NoteView {\n    var note: Note = Note.create {\n        set: {\n            text: input.text\n        }\n    }\n    return NoteView {\n        id: note.id\n        text: note.text\n    }\n}\n",
    ),
    (
        "domains/notes/actions/edit_note.jadpo",
        "use notes/entities { Note, NoteNotFound }\nuse notes/types { NoteInput, NoteView }\n\n/// Rewrites a note in one step.\naction edit_note(note_id: Note.id, input: NoteInput) fails NoteNotFound -> NoteView {\n    var note: Note = attempt Note.update(note_id) {\n        set: {\n            text: input.text\n        }\n    }\n    return NoteView {\n        id: note.id\n        text: note.text\n    }\n}\n",
    ),
    (
        "domains/notes/routes/notes.jadpo",
        "use notes/entities { Note }\nuse notes/types { NoteInput, NoteView }\nuse notes/actions { add_note, edit_note }\n\n/// Notes.\nroutes Notes {\n    prefix: /notes\n\n    /// Adds a note.\n    POST / {\n        authentication: RouteAuthentication.none\n        input: NoteInput\n        output: NoteView\n        status: SuccessStatus.created\n        run: add_note(input)\n    }\n\n    /// Rewrites a note, within a second.\n    PUT /{note_id: Note.id} {\n        authentication: RouteAuthentication.none\n        input: NoteInput\n        output: NoteView\n        deadline: 1s\n        run: edit_note(note_id, input)\n    }\n}\n",
    ),
];

/// A UUID's text as its bytes.
fn uuid(text: &str) -> [u8; 16] {
    let hex: String = text.chars().filter(|c| *c != '-').collect();
    let mut out = [0u8; 16];
    for (at, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[at * 2..at * 2 + 2], 16).expect("hex");
    }
    out
}

/// Runs `sql` with `params` on a connection of its own, giving the rows.
fn query(url: &str, sql: &str, params: Vec<jadpo_runtime::SqlValue>) -> Vec<jadpo_runtime::Row> {
    use jadpo_runtime::Database as _;
    let runtime = jadpo_host_native::Runtime::new().expect("a runtime");
    let store = jadpo_host_native::postgres(url).expect("a store");
    runtime.block_on(async {
        let mut transaction = store.begin().await.expect("begins");
        let rows = transaction.query(sql, &params).await.expect("runs");
        transaction.commit().await.expect("commits");
        rows
    })
}

#[test]
fn a_write_past_its_deadline_commits_nothing_and_answers_504() {
    if skipped() {
        return;
    }
    let dir = tempdir::Dir::new("deadline");
    let root = dir.path().to_path_buf();
    for (path, text) in NOTES {
        write(&root, path, text);
    }
    for args in [
        &["migrate", "new", "initial"][..],
        &["api", "update"],
        &["check"],
    ] {
        let done = jadpo(&root, args);
        assert_eq!(done.code, 0, "{}{}", done.stdout, done.stderr);
    }
    let dev = Dev::start(&root);
    let created = dev.send("POST", "/notes", &[], r#"{"text":"first"}"#);
    assert_eq!(created.status, 201, "{created:?}");
    let id = created.json()["id"].as_str().expect("an id").to_string();

    // Another transaction holds the note's row until the test lets go.
    let (locked_tx, locked) = std::sync::mpsc::channel::<()>();
    let (release, released) = std::sync::mpsc::channel::<()>();
    let holder = {
        let url = dev.url.clone();
        let id = uuid(&id);
        std::thread::spawn(move || {
            use jadpo_runtime::Database as _;
            let runtime = jadpo_host_native::Runtime::new().expect("a runtime");
            let store = jadpo_host_native::postgres(&url).expect("a store");
            runtime.block_on(async {
                let mut transaction = store.begin().await.expect("begins");
                transaction
                    .execute(
                        "UPDATE \"note\" SET \"text\" = 'held' WHERE \"id\" = $1",
                        &[jadpo_runtime::SqlValue::Uuid(id)],
                    )
                    .await
                    .expect("locks the row");
                locked_tx.send(()).expect("says so");
                released.recv().expect("let go");
                transaction.rollback().await.expect("rolls back");
            });
        })
    };
    locked.recv().expect("the row is locked");

    let path = format!("/notes/{id}");
    let key = [("Idempotency-Key", "edit-1")];
    let started = Instant::now();
    let late = {
        let port = dev.port;
        let path = path.clone();
        std::thread::spawn(move || {
            send(
                port,
                "PUT",
                &path,
                &[("Idempotency-Key", "edit-1")],
                r#"{"text":"late"}"#,
            )
        })
    };
    std::thread::sleep(Duration::from_millis(300));
    let busy = dev.send("PUT", &path, &key, r#"{"text":"late"}"#);
    assert_eq!(busy.status, 409, "{busy:?}");
    assert_eq!(busy.json()["error"]["code"], "conflict");
    let late = late.join().expect("answered");
    assert_eq!(late.status, 504, "{late:?}");
    assert_eq!(late.json()["error"]["code"], "deadline_exceeded");
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "answered at the deadline, not when the lock went: {:?}",
        started.elapsed()
    );
    release.send(()).expect("lets go");
    holder.join().expect("the holder ends");

    let text = || {
        let rows = query(
            &dev.url,
            "SELECT \"text\" FROM \"note\" WHERE \"id\" = $1",
            vec![jadpo_runtime::SqlValue::Uuid(uuid(&id))],
        );
        match rows.first().and_then(|row| row.first()) {
            Some(jadpo_runtime::SqlValue::Text(text)) => text.clone(),
            other => panic!("{other:?}"),
        }
    };
    assert_eq!(text(), "first", "the late write committed nothing");
    // Its key was given up, so a retry runs.
    let retried = dev.send("PUT", &path, &key, r#"{"text":"late"}"#);
    assert_eq!(retried.status, 200, "{retried:?}");
    assert_eq!(text(), "late");
}

#[test]
fn an_api_that_requires_keys_says_so_in_its_record_and_openapi() {
    let dir = tempdir::Dir::new("required");
    let root = dir.path().join("notes");
    for (path, text) in NOTES {
        write(&root, path, text);
    }
    write(
        &root,
        "app.jadpo",
        "application {\n    apis: {\n        main: {\n            prefix: /\n            idempotency: Idempotency.required\n        }\n    }\n}\n",
    );
    for args in [
        &["migrate", "new", "initial"][..],
        &["api", "update"],
        &["check"],
    ] {
        let done = jadpo(&root, args);
        assert_eq!(done.code, 0, "{}{}", done.stdout, done.stderr);
    }
    let record = std::fs::read_to_string(root.join("api/main.api")).expect("the record");
    assert!(
        record.contains("version: 1\n\nidempotency: required\n\nPOST /notes\n"),
        "{record}"
    );
    let document: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(root.join("build/openapi/main.json")).expect("the document"),
    )
    .expect("JSON");
    for (path, method) in [("/notes", "post"), ("/notes/{note_id}", "put")] {
        let operation = &document["paths"][path][method];
        let header = operation["parameters"]
            .as_array()
            .and_then(|parameters| {
                parameters
                    .iter()
                    .find(|parameter| parameter["name"] == "Idempotency-Key")
            })
            .unwrap_or_else(|| panic!("{method} {path} lists the key"));
        assert_eq!(header["in"], "header");
        assert_eq!(header["required"], true);
        for status in ["400", "409", "422", "504"] {
            assert!(
                operation["responses"].get(status).is_some(),
                "{method} {path} answers {status}"
            );
        }
    }
}

#[test]
fn a_route_on_a_health_path_is_refused() {
    let dir = tempdir::Dir::new("health-path");
    let root = dir.path().join("notes");
    for (path, text) in NOTES {
        write(&root, path, text);
    }
    write(
        &root,
        "domains/notes/routes/health.jadpo",
        "use notes/types { NoteInput, NoteView }\nuse notes/actions { add_note }\n\n/// Not the runtime's.\nroutes Health {\n    prefix: /health\n\n    POST /ready {\n        authentication: RouteAuthentication.none\n        input: NoteInput\n        output: NoteView\n        run: add_note(input)\n    }\n}\n",
    );
    let checked = jadpo(&root, &["check", "--color", "never"]);
    assert_eq!(checked.code, 1, "{}", checked.stdout);
    assert!(
        checked
            .stdout
            .contains("error: `/health/ready` is the runtime's health check"),
        "{}",
        checked.stdout
    );
    assert!(checked.stdout.contains("rule: route.path_overlap"));
}
/// A project that signs members up and keeps their notes (area 8).
const SIGN_IN: &[(&str, &str)] = &[
    (
        "app.jadpo",
        r#"use notes/entities { Member }
use notes/types { MemberStatus }

application {}

principal {
    user: {
        subject: Member.handle
        active: Member.status == MemberStatus.active
    }
}
"#,
    ),
    (
        "authentication.jadpo",
        r#"authentication {
    browser_session: {
        cookie: "notes_session"
    }
}
"#,
    ),
    (
        "domains/notes/actions/register.jadpo",
        r#"use notes/entities { Member }
use notes/types { Handle, MemberStatus }
use notes/failures { HandleTaken }

action register(handle: Handle) fails HandleTaken -> Member {
    return attempt Member.create {
        set: {
            handle: handle
            status: MemberStatus.active
        }
        conflict: {
            Member.handle: HandleTaken
        }
    }
}
"#,
    ),
    (
        "domains/notes/actions/sign_out.jadpo",
        r#"/// Ends the session the request came with.
action sign_out() -> Unit {
    sessions.end()
}
"#,
    ),
    (
        "domains/notes/actions/sign_up.jadpo",
        r#"use notes/entities { Member }
use notes/types { Handle }
use notes/failures { HandleTaken }
use notes/actions { register }

/// Registers a member and signs them in.
action sign_up(input: SignUp) fails HandleTaken -> Unit {
    var member: Member = attempt register(input.handle)
    sessions.start(member.id)
}

/// What signing up takes.
type SignUp = Input {
    handle: Handle
}
"#,
    ),
    (
        "domains/notes/actions/suspend.jadpo",
        r#"use notes/entities { Member, MemberNotFound }
use notes/types { MemberStatus }

action suspend(member_id: Member.id) fails MemberNotFound -> Member {
    return attempt Member.update(member_id) {
        set: {
            status: MemberStatus.suspended
        }
    }
}
"#,
    ),
    (
        "domains/notes/actions/write_note.jadpo",
        r#"use notes/entities { Note }
use notes/types { NoteText, NoteView }

/// Writes a note as the signed-in member.
action write_note(input: WriteNote, author: Principal.user) -> NoteView {
    var note: Note = Note.create {
        set: {
            author_id: author.id
            text: input.text
        }
    }
    return NoteView {
        text: note.text
    }
}

/// What writing a note takes.
type WriteNote = Input {
    text: NoteText
}
"#,
    ),
    (
        "domains/notes/entities/member.jadpo",
        r#"use notes/types { Handle, MemberStatus }

entity Member {
    id: Uuid
    handle: Handle {
        unique: true
    }
    status: MemberStatus
}
"#,
    ),
    (
        "domains/notes/entities/note.jadpo",
        r#"use notes/entities { Member }
use notes/types { NoteText }

entity Note {
    id: Uuid
    author_id: Member.id {
        on_delete: OnDelete.cascade
    }
    text: NoteText
}
"#,
    ),
    (
        "domains/notes/failures/handle_taken.jadpo",
        r#"type HandleTaken = Failure {
    kind: FailureKind.conflict
    message: "That handle is taken."
}
"#,
    ),
    (
        "domains/notes/routes/account.jadpo",
        r#"use notes/actions { SignUp, sign_out, sign_up }

/// Signing up and out.
routes Account {
    prefix: /account

    /// Anyone may sign up; the response sets the session's cookies.
    POST /sign-up {
        authentication: RouteAuthentication.none
        input: SignUp
        status: SuccessStatus.no_content
        run: sign_up(input)
    }

    /// Ends the session and clears its cookies.
    POST /sign-out {
        status: SuccessStatus.no_content
        run: sign_out()
    }
}
"#,
    ),
    (
        "domains/notes/routes/notes.jadpo",
        r#"use notes/types { NoteView }
use notes/actions { WriteNote, write_note }

/// The signed-in member's notes.
routes Notes {
    prefix: /notes

    POST / {
        input: WriteNote
        output: NoteView
        status: SuccessStatus.created
        run: write_note(input, current_principal.user)
    }
}
"#,
    ),
    (
        "domains/notes/types/handle.jadpo",
        r#"type Handle = Text {
    min_length: 3
    max_length: 40
}
"#,
    ),
    (
        "domains/notes/types/member_status.jadpo",
        r#"type MemberStatus = Enum {
    active
    suspended
}
"#,
    ),
    (
        "domains/notes/types/note_text.jadpo",
        r#"type NoteText = Text {
    min_length: 1
    max_length: 500
}
"#,
    ),
    (
        "domains/notes/types/note_view.jadpo",
        r#"use notes/types { NoteText }

type NoteView = Output {
    text: NoteText
}
"#,
    ),
];

/// The sign-in project, migrated and recorded, checking cleanly.
fn sign_in_project() -> (tempdir::Dir, PathBuf) {
    let dir = tempdir::Dir::new("sign-in");
    let root = dir.path().join("notes");
    for (path, text) in SIGN_IN {
        write(&root, path, text);
    }
    for args in [
        &["migrate", "new", "initial"][..],
        &["api", "update"],
        &["check"],
    ] {
        let done = jadpo(&root, args);
        assert_eq!(done.code, 0, "{}{}", done.stdout, done.stderr);
    }
    (dir, root)
}

/// The session's cookies a response sets: the session's token and its
/// CSRF token, with each one's attributes.
fn session_cookies(reply: &Reply) -> (String, String, Vec<String>) {
    let set: Vec<String> = reply
        .headers
        .iter()
        .filter(|(name, _)| name.eq_ignore_ascii_case("set-cookie"))
        .map(|(_, value)| value.clone())
        .collect();
    let value = |name: &str| {
        set.iter()
            .find_map(|cookie| cookie.strip_prefix(&format!("{name}=")))
            .and_then(|rest| rest.split(';').next())
            .unwrap_or_default()
            .to_string()
    };
    (value("notes_session"), value("notes_session_csrf"), set)
}

#[test]
fn a_browser_session_signs_in_writes_with_its_csrf_token_and_signs_out() {
    if skipped() {
        return;
    }
    let (_dir, root) = sign_in_project();
    let dev = Dev::start(&root);
    let json = [("content-type", "application/json")];

    // Signing up is public, and its response sets both cookies.
    let signed = dev.send("POST", "/account/sign-up", &json, r#"{"handle":"ann"}"#);
    assert_eq!(signed.status, 204, "{}", signed.body);
    let (token, csrf, set) = session_cookies(&signed);
    assert_eq!(token.len(), 64, "{set:?}");
    assert_eq!(csrf.len(), 64, "{set:?}");
    let session = set
        .iter()
        .find(|cookie| cookie.starts_with("notes_session="))
        .expect("the session's cookie");
    for attribute in [
        "HttpOnly",
        "Secure",
        "SameSite=Strict",
        "Path=/",
        "Max-Age=2592000",
    ] {
        assert!(session.contains(attribute), "{session}");
    }
    let readable = set
        .iter()
        .find(|cookie| cookie.starts_with("notes_session_csrf="))
        .expect("the CSRF token's cookie");
    assert!(!readable.contains("HttpOnly"), "{readable}");

    // Only verifiers are kept.
    let rows = query(
        &dev.url,
        "SELECT \"verifier\", \"csrf_verifier\" FROM \"jadpo_sessions\"",
        Vec::new(),
    );
    assert_eq!(rows.len(), 1);
    for column in &rows[0] {
        let jadpo_runtime::SqlValue::Text(kept) = column else {
            panic!("{column:?}");
        };
        assert_ne!(kept, &token);
        assert_ne!(kept, &csrf);
    }

    let note = r#"{"text":"Buy milk"}"#;
    let cookie = format!("notes_session={token}; notes_session_csrf={csrf}");

    // No credential.
    let anonymous = dev.send("POST", "/notes", &json, note);
    assert_eq!(anonymous.status, 401);
    assert_eq!(anonymous.json()["error"]["code"], "authentication_required");

    // The cookie alone: a write needs the server's origin and the token.
    let forged = dev.send("POST", "/notes", &[json[0], ("cookie", &cookie)], note);
    assert_eq!(forged.status, 403);
    assert_eq!(forged.json()["error"]["code"], "csrf_rejected");
    let elsewhere = dev.send(
        "POST",
        "/notes",
        &[
            json[0],
            ("cookie", &cookie),
            ("origin", "https://elsewhere.test"),
            ("x-jadpo-csrf", &csrf),
        ],
        note,
    );
    assert_eq!(elsewhere.json()["error"]["code"], "csrf_rejected");

    // Signed in, from this server, with the session's token.
    let signed_in = [
        json[0],
        ("cookie", cookie.as_str()),
        ("origin", "http://localhost"),
        ("x-jadpo-csrf", csrf.as_str()),
    ];
    let written = dev.send("POST", "/notes", &signed_in, note);
    assert_eq!(written.status, 201, "{}", written.body);
    assert_eq!(written.json()["text"], "Buy milk");

    // A cookie and a bearer token are two credentials.
    let both = dev.send(
        "POST",
        "/notes",
        &[
            signed_in[0],
            signed_in[1],
            signed_in[2],
            signed_in[3],
            ("authorization", "Bearer x"),
        ],
        note,
    );
    assert_eq!(both.status, 401);
    assert_eq!(both.json()["error"]["code"], "ambiguous_credentials");

    // A session nobody started is refused, and its cookies cleared.
    let unknown = format!("notes_session={}", "0".repeat(64));
    let stale = dev.send("POST", "/notes", &[json[0], ("cookie", &unknown)], note);
    assert_eq!(stale.status, 401);
    assert_eq!(stale.json()["error"]["code"], "invalid_credentials");
    let (_, _, cleared) = session_cookies(&stale);
    assert!(
        cleared.iter().all(|cookie| cookie.contains("Max-Age=0")),
        "{cleared:?}"
    );
    assert_eq!(cleared.len(), 2, "{cleared:?}");

    // Signing out ends the session and clears its cookies.
    let out = dev.send("POST", "/account/sign-out", &signed_in[1..], "");
    assert_eq!(out.status, 204, "{}", out.body);
    let (_, _, cleared) = session_cookies(&out);
    assert!(
        cleared.iter().all(|cookie| cookie.contains("Max-Age=0")),
        "{cleared:?}"
    );
    let after = dev.send("POST", "/notes", &signed_in, note);
    assert_eq!(after.status, 401);
    assert_eq!(after.json()["error"]["code"], "invalid_credentials");
}

#[test]
fn a_signed_in_callers_idempotency_keys_are_their_own() {
    if skipped() {
        return;
    }
    let (_dir, root) = sign_in_project();
    let dev = Dev::start(&root);
    let json = ("content-type", "application/json");
    let sign_up = |handle: &str| {
        let reply = dev.send(
            "POST",
            "/account/sign-up",
            &[json],
            &format!(r#"{{"handle":"{handle}"}}"#),
        );
        assert_eq!(reply.status, 204, "{}", reply.body);
        let (token, csrf, _) = session_cookies(&reply);
        (format!("notes_session={token}"), csrf)
    };
    let (ann, ann_csrf) = sign_up("ann");
    let (bob, bob_csrf) = sign_up("bob");
    let write = |cookie: &str, csrf: &str, text: &str| {
        dev.send(
            "POST",
            "/notes",
            &[
                json,
                ("cookie", cookie),
                ("origin", "http://localhost"),
                ("x-jadpo-csrf", csrf),
                ("idempotency-key", "note-1"),
            ],
            &format!(r#"{{"text":"{text}"}}"#),
        )
    };
    let first = write(&ann, &ann_csrf, "Ann's");
    assert_eq!(first.status, 201, "{}", first.body);
    // The same key from another caller is another request.
    let other = write(&bob, &bob_csrf, "Bob's");
    assert_eq!(other.status, 201, "{}", other.body);
    assert_eq!(other.json()["text"], "Bob's");
    // Ann's repeat is answered from her key.
    let repeat = write(&ann, &ann_csrf, "Ann's");
    assert_eq!(repeat.status, 201);
    assert_eq!(repeat.body, first.body);
    let rows = query(&dev.url, "SELECT count(*) FROM \"note\"", Vec::new());
    assert_eq!(rows[0][0], jadpo_runtime::SqlValue::Int8(2));
    let scopes = query(
        &dev.url,
        "SELECT DISTINCT \"scope\" FROM \"jadpo_idempotency_keys\" ORDER BY 1",
        Vec::new(),
    );
    assert_eq!(scopes.len(), 2);
    for scope in scopes {
        let jadpo_runtime::SqlValue::Text(scope) = &scope[0] else {
            panic!("{scope:?}");
        };
        assert!(scope.starts_with("user "), "{scope}");
    }
}

#[test]
fn openapi_shows_the_session_and_what_signing_in_answers() {
    let (_dir, root) = sign_in_project();
    let document: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(root.join("build/openapi/main.json")).expect("the document"),
    )
    .expect("JSON");
    let scheme = &document["components"]["securitySchemes"]["browser_session"];
    assert_eq!(scheme["type"], "apiKey");
    assert_eq!(scheme["in"], "cookie");
    assert_eq!(scheme["name"], "notes_session");
    assert_eq!(
        document["security"][0]["browser_session"],
        serde_json::json!([])
    );
    let sign_up = &document["paths"]["/account/sign-up"]["post"];
    assert_eq!(sign_up["security"], serde_json::json!([]));
    assert!(sign_up["responses"].get("401").is_none());
    let write = &document["paths"]["/notes"]["post"]["responses"];
    for status in ["401", "403", "503"] {
        assert!(write.get(status).is_some(), "POST /notes answers {status}");
    }
    let refused = write["403"]["content"]["application/json"]["schema"]["oneOf"]
        .as_array()
        .expect("both refusals");
    assert_eq!(refused.len(), 2);
    let record = std::fs::read_to_string(root.join("api/main.api")).expect("the record");
    assert!(
        record.contains("POST /account/sign-up\n    authentication: none\n"),
        "{record}"
    );
    assert!(record.contains("POST /notes\n    input:"), "{record}");
    let listed = jadpo(&root, &["routes", "--json"]);
    assert_eq!(listed.code, 0, "{}{}", listed.stdout, listed.stderr);
    let listed: serde_json::Value = serde_json::from_str(&listed.stdout).expect("JSON");
    let shown: Vec<(String, String)> = listed["data"]["routes"]
        .as_array()
        .expect("routes")
        .iter()
        .map(|route| {
            (
                format!(
                    "{} {}",
                    route["method"].as_str().unwrap_or(""),
                    route["path"].as_str().unwrap_or("")
                ),
                route["authentication"].as_str().unwrap_or("").to_string(),
            )
        })
        .collect();
    assert!(
        shown.contains(&("POST /account/sign-up".into(), "none".into())),
        "{shown:?}"
    );
    assert!(
        shown.contains(&("POST /notes".into(), "required".into())),
        "{shown:?}"
    );
}
