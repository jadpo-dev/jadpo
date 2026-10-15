//! Routes served over real HTTP (area 7): `jadpo dev` runs a project in the
//! interpreter on the native host, against live PostgreSQL, and each test
//! sends requests over TCP as any client would, reading the status, the
//! headers and the JSON: a created record and reading it back, the failure
//! envelope with its `request_id` and `x-request-id`, input problems as
//! `{ field, rule }`, a `GET`'s input from the query string and a header,
//! `HEAD` and `OPTIONS`, unknown routes and methods, the default rate limit,
//! and a save swapping in a new version while the last good one keeps
//! serving through an error. The project's OpenAPI document is compared
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
        input: CreateTodo
        output: TodoView
        status: SuccessStatus.created
        run: create_todo(input)
    }

    /// Todos, by title.
    GET / {
        input: ListTodos
        output: List<TodoView>
        run: list_todos(input)
    }

    /// One todo.
    GET /{todo_id: Todo.id} {
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
            url: Some(url),
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
