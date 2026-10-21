//! A native deployable, built and run against live PostgreSQL (roadmap
//! chunk 8.1o; docs/compiler-runtime.md §2.6, "A native deployable"):
//! `jadpo build` writes teaching example 23, the repair café, as one
//! binary in `build/target/app/`, holding none of the testing machinery.
//! Started with its settings from the environment and nothing else, it
//! refuses to start without its database's address and against a database
//! that hasn't applied its last migration, naming each in a
//! `runtime.startup_failed` event that carries the build stamp; once the
//! database is up to date it starts, books a repair through `POST
//! /repairs`, waiting for the subscribers that find it a bench, answers its
//! health check, and on SIGTERM stops accepting, drains and ends.
//!
//! The build is kept beside the build's own folders
//! (`target/jadpo-compiled/deployable/`), so a second run compiles only
//! what changed. The database is the one `JADPO_TEST_DATABASE_URL` names
//! or one started from the machine's PostgreSQL tools;
//! `JADPO_SKIP_POSTGRES=1` skips it. Nothing leaves this machine.

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

use jadpo_cli::run;
use jadpo_term::Environment;
use serde_json::Value;

/// Copies the folder `from` into `to`, over what is there, but `build/`
/// and `.jadpo/`, which a run keeps.
fn copy_folder(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("folders");
    for entry in fs::read_dir(from).expect("a folder") {
        let entry = entry.expect("an entry");
        let path = entry.path();
        let target = to.join(entry.file_name());
        if path.is_dir() {
            if entry.file_name() != ".jadpo" && entry.file_name() != "build" {
                copy_folder(&path, &target);
            }
        } else if fs::read(&target).ok() != fs::read(&path).ok() {
            fs::copy(&path, &target).expect("a copy");
        }
    }
}

/// Teaching example 23, copied where its build is kept between runs.
fn repair_cafe() -> PathBuf {
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let target = Path::new(env!("CARGO_TARGET_TMPDIR"));
    let root = target
        .parent()
        .unwrap_or(target)
        .join("jadpo-compiled/deployable/23-repair-cafe");
    copy_folder(&examples.join("23-repair-cafe"), &root);
    root
}

/// A connection the test server gives, as the URL a `PostgresUrl` holds:
/// the disposable server answers with `host=… dbname=…` pairs, an
/// external one with a URL.
fn as_url(connection: &str) -> String {
    if connection.starts_with("postgres") {
        return connection.to_string();
    }
    let part = |key: &str| {
        connection
            .split_whitespace()
            .find_map(|pair| pair.strip_prefix(&format!("{key}=")))
            .map(|value| value.trim_matches('\'').to_string())
            .unwrap_or_default()
    };
    format!(
        "postgresql://{}@/{}?host={}&port={}",
        part("user"),
        part("dbname"),
        part("host"),
        part("port")
    )
}

/// The migrations of the project at `root`, store `main`'s, in order.
fn migrations(root: &Path) -> Vec<jadpo_std::Migration> {
    let loaded = jadpo_db::load(root);
    let steps = jadpo_check::data::migrations::store_steps(
        jadpo_check::data::migrations::migrations(&loaded.db, loaded.project),
    );
    steps.get("main").cloned().expect("store main's migrations")
}

/// Applies `migrations` to the database at `connection`.
fn apply(connection: &str, migrations: &[jadpo_std::Migration]) {
    let runtime = jadpo_host_native::Runtime::new().expect("a runtime");
    let database = jadpo_host_native::postgres(connection).expect("a store");
    runtime
        .block_on(jadpo_runtime::migrate(&database, migrations))
        .expect("applied");
}

/// The deployable at `binary` started with only `variables` in its
/// environment, and its log lines as they come.
fn start(binary: &Path, variables: &[(&str, &str)]) -> (Child, Receiver<Value>) {
    let mut command = Command::new(binary);
    command
        .env_clear()
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    for (name, value) in variables {
        command.env(name, value);
    }
    let mut child = command.spawn().expect("it starts");
    let stdout = child.stdout.take().expect("its output");
    let (lines, read) = channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            let event: Value = serde_json::from_str(&line)
                .unwrap_or_else(|_| panic!("a log line is one JSON object: {line}"));
            if lines.send(event).is_err() {
                break;
            }
        }
    });
    (child, read)
}

/// The next log event, within `seconds`.
fn next(lines: &Receiver<Value>, seconds: u64) -> Value {
    lines
        .recv_timeout(Duration::from_secs(seconds))
        .expect("a log line in time")
}

/// Waits for `child` to end, within `seconds`: its exit code.
fn ended(child: &mut Child, seconds: u64) -> Option<i32> {
    for _ in 0..seconds * 20 {
        if let Some(status) = child.try_wait().expect("its status") {
            return status.code();
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let _ = child.kill();
    panic!("the deployable didn't end within {seconds} s");
}

/// One HTTP/1.1 exchange with the server at `address`: the status and the
/// body.
fn exchange(address: &str, method: &str, path: &str, body: &str) -> (u16, String) {
    let mut stream = TcpStream::connect(address).expect("it accepts");
    stream
        .set_read_timeout(Some(Duration::from_secs(20)))
        .expect("a timeout");
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: {address}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .expect("sent");
    let mut answer = String::new();
    stream.read_to_string(&mut answer).expect("an answer");
    let status = answer
        .split(' ')
        .nth(1)
        .and_then(|code| code.parse().ok())
        .unwrap_or_else(|| panic!("an HTTP answer: {answer}"));
    let body = answer
        .split_once("\r\n\r\n")
        .map_or(String::new(), |(_, body)| body.to_string());
    (status, body)
}

/// What a symbol naming `names`, a path in a crate, holds, legacy and v0
/// mangling alike: each name its length, then its text (as
/// `tools/deployable.py` reads them).
fn path(names: &[&str]) -> Vec<u8> {
    names
        .iter()
        .map(|name| format!("{}{name}", name.len()))
        .collect::<String>()
        .into_bytes()
}

fn holds(binary: &[u8], symbol: &[u8]) -> bool {
    binary.windows(symbol.len()).any(|window| window == symbol)
}

/// The stamp's part of an event, as the build said it.
fn stamped(event: &Value, built: &Value) {
    for key in [
        "build",
        "commit",
        "modified",
        "compiler",
        "deployable",
        "target",
    ] {
        assert_eq!(event[key], built[key], "{key} in {event}");
    }
}

#[cfg(unix)]
#[test]
fn a_teaching_example_is_built_refuses_to_start_out_of_date_serves_and_drains() {
    if !jadpo_test::live() {
        return;
    }
    let root = repair_cafe();
    let output = run(
        &["build".to_string(), "--json".to_string()],
        &Environment::empty().with_cwd(&root),
    );
    assert_eq!(output.code, 0, "{}{}", output.stdout, output.stderr);
    let document: Value = serde_json::from_str(&output.stdout).expect("JSON");
    let built = &document["data"];
    assert_eq!(built["deployable"], "app", "{built}");
    assert_eq!(built["target"], "native", "{built}");
    assert_eq!(built["compiler"], env!("CARGO_PKG_VERSION"), "{built}");
    let id = built["build"].as_str().expect("a build id");
    assert!(
        id.len() == 14 && id.starts_with("b_") && id[2..].chars().all(|c| c.is_ascii_hexdigit()),
        "{id}"
    );
    assert_eq!(built["binary"], "build/target/app/app", "{built}");
    assert_eq!(built["settings"], serde_json::json!(["DATABASE_URL"]));
    let binary = root.join("build/target/app/app");
    assert!(
        root.join("build/target/app/openapi/main.json").is_file(),
        "the API's document is beside the binary"
    );
    assert!(root.join("build/rust/rust-toolchain.toml").is_file());

    // Built without the `testing` feature: it reaches none of the fakes,
    // recordings, crash points, records or a test's steps, but it is a
    // deployable, with the runtime's HTTP component and the driver factory.
    let bytes = fs::read(&binary).expect("the binary");
    for absent in [
        path(&["jadpo_runtime", "fakes"]),
        path(&["ai", "fake"]),
        path(&["capability", "Faked"]),
        path(&["jadpo_runtime", "recorded"]),
        path(&["jadpo_runtime", "prompt_recordings"]),
        path(&["jadpo_runtime", "crashes"]),
        path(&["jadpo_runtime", "record"]),
        path(&["jadpo_runtime", "test_steps"]),
    ] {
        assert!(
            !holds(&bytes, &absent),
            "it holds {}",
            String::from_utf8_lossy(&absent)
        );
    }
    for present in [
        path(&["jadpo_runtime", "http"]),
        path(&["jadpo_drivers", "factory"]),
    ] {
        assert!(
            holds(&bytes, &present),
            "it lacks {}",
            String::from_utf8_lossy(&present)
        );
    }

    // Without its database's address it doesn't start, naming the
    // setting and its variable.
    let (mut child, lines) = start(&binary, &[("HOST", "127.0.0.1"), ("PORT", "0")]);
    let failed = next(&lines, 30);
    assert_eq!(failed["event"], "runtime.startup_failed", "{failed}");
    stamped(&failed, built);
    assert_eq!(
        failed["problems"][0]["problem"], "setting_missing",
        "{failed}"
    );
    assert_eq!(
        failed["problems"][0]["variable"], "DATABASE_URL",
        "{failed}"
    );
    assert_eq!(ended(&mut child, 30), Some(1));

    // Against a database that hasn't applied the last migration, it
    // doesn't start either, naming it.
    let server = jadpo_test::postgres::Server::start().expect("a server");
    let connection = server.create_database("deployable").expect("a database");
    let url = as_url(&connection);
    let migrations = migrations(&root);
    let (last, earlier) = migrations.split_last().expect("migrations");
    apply(&connection, earlier);
    let settings = [
        ("DATABASE_URL", url.as_str()),
        ("HOST", "127.0.0.1"),
        ("PORT", "0"),
    ];
    let (mut child, lines) = start(&binary, &settings);
    let failed = next(&lines, 60);
    assert_eq!(failed["event"], "runtime.startup_failed", "{failed}");
    let problem = &failed["problems"][0];
    assert_eq!(problem["problem"], "migrations_pending", "{failed}");
    assert_eq!(problem["store"], "main", "{failed}");
    assert_eq!(
        problem["pending"],
        serde_json::json!([last.name]),
        "{failed}"
    );
    assert_eq!(ended(&mut child, 30), Some(1));

    // Up to date, it starts, serves and ticks.
    apply(&connection, &migrations);
    let (mut child, lines) = start(&binary, &settings);
    let started = next(&lines, 60);
    assert_eq!(started["event"], "runtime.started", "{started}");
    stamped(&started, built);
    assert_eq!(started["unbuilt"], serde_json::json!([]), "{started}");
    let address = started["address"]
        .as_str()
        .expect("its address")
        .to_string();
    let (status, body) = exchange(
        &address,
        "POST",
        "/repairs",
        r#"{"item":"lamp","symptom":"flickers when the shade is touched"}"#,
    );
    assert!(status == 201 || status == 202, "{status}: {body}");
    let ticket: Value = serde_json::from_str(&body).expect("a ticket");
    assert_eq!(ticket["item"], "lamp", "{ticket}");
    let (status, body) = exchange(&address, "GET", "/health/ready", "");
    assert_eq!(status, 200, "{body}");

    // SIGTERM while a booking is on its way: the request it is reading
    // is still answered, then it drains and ends.
    let body = r#"{"item":"bike","symptom":"the chain slips on every hill"}"#;
    let (head, rest) = body.split_at(20);
    let mut in_flight = TcpStream::connect(&address).expect("it accepts");
    in_flight
        .set_read_timeout(Some(Duration::from_secs(20)))
        .expect("a timeout");
    write!(
        in_flight,
        "POST /repairs HTTP/1.1\r\nHost: {address}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{head}",
        body.len()
    )
    .expect("sent");
    in_flight.flush().expect("sent");
    std::thread::sleep(Duration::from_millis(300));
    let stopped = Command::new("kill")
        .args(["-TERM", &child.id().to_string()])
        .status()
        .expect("kill runs");
    assert!(stopped.success());
    std::thread::sleep(Duration::from_millis(300));
    in_flight.write_all(rest.as_bytes()).expect("the rest sent");
    let mut answer = String::new();
    in_flight.read_to_string(&mut answer).expect("an answer");
    assert!(
        answer.starts_with("HTTP/1.1 201") || answer.starts_with("HTTP/1.1 202"),
        "{answer}"
    );
    let last = next(&lines, 30);
    assert_eq!(last["event"], "runtime.stopped", "{last}");
    stamped(&last, built);
    assert_eq!(ended(&mut child, 30), Some(0));
    assert!(
        TcpStream::connect(&address).is_err(),
        "nothing listens once it has stopped"
    );
}
