//! A Workers deployable, built and served by workerd (roadmap chunks 8.2b
//! and 8.2d; docs/compiler-runtime.md §2.6, "A Workers deployable"; area
//! 14, "A Workers deployable"): teaching example 23, the repair café,
//! declared as one kiosk built for `Target.wasm`. `jadpo build` writes it in
//! `build/target/kiosk/` as a WebAssembly module bound by wasm-bindgen's
//! library, the JavaScript that binds it, Jadpo's module shim and a
//! Wrangler file naming them, the setting it needs and the compatibility
//! date; its stamp's target is `wasm`, the screen shows the module's size
//! raw and gzipped, and the module links the PostgreSQL driver the café's
//! database needs, the runtime's HTTP component, and none of the testing
//! machinery.
//!
//! workerd, Cloudflare's own Workers runtime, pinned in `tools/workers/`,
//! then runs the files as the Wrangler file describes them, against a
//! PostgreSQL database on this machine reached through `connect()`.
//! Without its database's address the Worker answers 503 and logs
//! `runtime.startup_failed` naming the setting's variable, once for its
//! isolate; against a database that hasn't applied its last migration it
//! answers 503 and logs the migration, and checks again at the next
//! request, so once the migration is applied the same isolate starts
//! serving: it books a repair through `POST /repairs`, the subscribers
//! that find it a bench and print its ticket delivered while the request
//! waits, and answers with the ticket read back from the database; a thank
//! you's note is pinned up after its answer, in `waitUntil`; its readiness
//! check reaches the database; and each address `CF-Connecting-IP` gives
//! has its own allowance of 120 requests a minute. Every event it logs is
//! one JSON line carrying the build stamp.
//!
//! The build is kept beside the build's own folders
//! (`target/jadpo-compiled/workers/`), so a second run compiles only what
//! changed. The database is the run's when `JADPO_TEST_DATABASE_URL` names
//! one by a URL, as CI's does (`127.0.0.1`), and otherwise one this test
//! starts on `127.0.0.1` from the machine's PostgreSQL tools, since the
//! one a local run of the gate starts listens on a Unix socket, which a
//! Worker can't open; `JADPO_SKIP_POSTGRES=1` skips the serving part.
//! Nothing leaves this machine: workerd is installed into `tools/workers/`
//! by `npm ci --ignore-scripts` the first time (`JADPO_WORKERD` names
//! another), and the Worker reaches nothing but this machine.

use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use jadpo_cli::run;
use jadpo_term::Environment;
use jadpo_test::postgres::Server;
use serde_json::Value;

use crate::deployable::{apply, copy_folder, migrations, rows};

/// The café declares one kiosk, built for Workers, serving its API and
/// running its subscribers and schedules, its rate limits counted in
/// Cloudflare's rate-limiting binding (chunk 8.2m2).
const DECLARED: &str = "application {\n    connections: {\n        limits: {\n            driver: Driver.cloudflare_rate_limits\n            namespace: 1001\n        }\n        main: {\n            driver: Driver.postgres\n            url: config.database_url\n        }\n    }\n    events: {\n        store: main\n    }\n    rate_limits: {\n        store: limits\n    }\n    deployables: {\n        kiosk: {\n            serves: [main]\n            runs: [Trigger.subscribers, Trigger.schedules]\n            target: Target.wasm\n        }\n    }\n}\n";

/// Teaching example 23 declared as a kiosk on Workers, copied where its
/// build is kept between runs.
fn repair_cafe() -> PathBuf {
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let target = Path::new(env!("CARGO_TARGET_TMPDIR"));
    let root = target
        .parent()
        .unwrap_or(target)
        .join("jadpo-compiled/workers/23-repair-cafe");
    copy_folder(&examples.join("23-repair-cafe"), &root);
    if fs::read_to_string(root.join("app.jadpo")).ok().as_deref() != Some(DECLARED) {
        fs::write(root.join("app.jadpo"), DECLARED).expect("written");
    }
    root
}

/// `jadpo` with `args` in `root`, which must succeed.
fn jadpo(root: &Path, args: &[&str]) -> jadpo_cli::Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let output = run(&args, &Environment::empty().with_cwd(root));
    assert_eq!(
        output.code, 0,
        "{args:?}: {}{}",
        output.stdout, output.stderr
    );
    output
}

/// Whether the module's names hold a path in crate `krate` starting with
/// `path`, as wasm-bindgen writes them: `jadpo_runtime[9f2c…]::fakes`.
fn names(module: &[u8], krate: &str, path: &str) -> bool {
    let text = String::from_utf8_lossy(module);
    let pattern = regex::Regex::new(&format!(
        r"{}\[[0-9a-f]+\]::{}",
        regex::escape(krate),
        regex::escape(path)
    ))
    .expect("a pattern");
    pattern.is_match(&text)
}

#[test]
fn a_teaching_example_is_built_for_workers_and_served_by_workerd() {
    let root = repair_cafe();
    let output = jadpo(&root, &["build", "--json"]);
    let document: Value = serde_json::from_str(&output.stdout).expect("JSON");
    let built = &document["data"];
    let kiosk = &built["deployables"][0];
    assert_eq!(
        built["deployables"].as_array().map(Vec::len),
        Some(1),
        "{built}"
    );
    assert_eq!(kiosk["deployable"], "kiosk", "{built}");
    assert_eq!(kiosk["target"], "wasm", "{built}");
    assert_eq!(kiosk["binary"], Value::Null, "{built}");
    assert_eq!(kiosk["wrangler"], "build/target/kiosk/wrangler.json");
    assert_eq!(kiosk["module"]["path"], "build/target/kiosk/kiosk.wasm");
    assert_eq!(
        kiosk["openapi"],
        serde_json::json!(["build/target/kiosk/openapi/main.json"])
    );
    let id = kiosk["build"].as_str().expect("a build id");
    assert!(
        id.len() == 14 && id.starts_with("b_") && id[2..].chars().all(|c| c.is_ascii_hexdigit()),
        "{id}"
    );
    let manifest: Value =
        serde_json::from_str(&fs::read_to_string(root.join("build/manifest.json")).expect("read"))
            .expect("JSON");
    assert_eq!(manifest["deployables"]["kiosk"]["build"], id, "{manifest}");
    assert_eq!(manifest["deployables"]["kiosk"]["target"], "wasm");

    // The module, its size as the build gives it, raw and gzipped.
    let out = root.join("build/target/kiosk");
    let module = fs::read(out.join("kiosk.wasm")).expect("the module");
    assert_eq!(kiosk["module"]["bytes"], module.len(), "{built}");
    let gzipped = kiosk["module"]["gzip_bytes"].as_u64().expect("gzipped");
    assert!(
        gzipped > 0 && gzipped < module.len() as u64,
        "{gzipped} of {}",
        module.len()
    );
    assert!(module.starts_with(b"\0asm"), "a WebAssembly module");
    println!(
        "measured: the kiosk's module {} bytes, {gzipped} gzipped",
        module.len()
    );

    // The Wrangler file: the shim as the main module, uploaded with the
    // files beside it as they are, the compatibility date, and the setting
    // the kiosk needs, by its variable.
    let wrangler: Value = serde_json::from_str(
        &fs::read_to_string(out.join("wrangler.json")).expect("the Wrangler file"),
    )
    .expect("JSON");
    assert_eq!(wrangler["name"], "kiosk", "{wrangler}");
    assert_eq!(wrangler["main"], "worker.mjs", "{wrangler}");
    assert_eq!(wrangler["no_bundle"], true, "{wrangler}");
    assert_eq!(
        wrangler["compatibility_date"],
        jadpo_codegen::workers::COMPATIBILITY_DATE
    );
    assert_eq!(
        wrangler["secrets"]["required"],
        serde_json::json!(["DATABASE_URL"]),
        "{wrangler}"
    );
    // A rate-limiting binding for each limit the kiosk counts, numbered
    // from `limits`' namespace.
    assert_eq!(
        wrangler["ratelimits"],
        serde_json::json!([
            { "name": "LIMITS_120_PER_60S", "namespace_id": "1001", "simple": { "limit": 120, "period": 60 } },
            { "name": "LIMITS_600_PER_60S", "namespace_id": "1002", "simple": { "limit": 600, "period": 60 } },
        ]),
        "{wrangler}"
    );
    let shim = fs::read_to_string(out.join("worker.mjs")).expect("the shim");
    assert!(shim.contains("from \"./kiosk.js\""), "{shim}");
    let js = fs::read_to_string(out.join("kiosk.js")).expect("the module's JavaScript");
    // The module's `fetch`, renamed where it would hide the global one the
    // module itself calls.
    assert!(
        js.contains("export function fetch(")
            || regex::Regex::new(r"export \{ \w+ as fetch \}")
                .expect("a pattern")
                .is_match(&js),
        "{js}"
    );
    assert!(js.contains("initSync"), "{js}");

    // It links the PostgreSQL driver, which the café's database needs, and
    // no SMTP driver, since it sends no email; the runtime's HTTP component
    // and the runner that serve a request; none of the fakes, recordings,
    // crash points, records or a test's steps.
    let crate_manifest =
        fs::read_to_string(root.join("build/rust/kiosk/deployable/Cargo.toml")).expect("read");
    assert!(
        crate_manifest.contains("features = [\"postgres\"]"),
        "{crate_manifest}"
    );
    for (krate, path) in [
        ("jadpo_runtime", "fakes"),
        ("jadpo_runtime", "ai::fake"),
        ("jadpo_runtime", "capability::Faked"),
        ("jadpo_runtime", "recorded"),
        ("jadpo_runtime", "prompt_recordings"),
        ("jadpo_runtime", "crashes"),
        ("jadpo_runtime", "record::"),
        ("jadpo_runtime", "test_steps"),
    ] {
        assert!(!names(&module, krate, path), "it holds {krate}::{path}");
    }
    for (krate, path) in [
        ("jadpo_runtime", "store"),
        ("jadpo_runtime", "http::trie::Routes"),
        ("jadpo_runtime", "serve::Serving"),
        ("jadpo_runtime", "runner"),
        ("jadpo_drivers", "postgres"),
        ("jadpo_program_kiosk", "cafe"),
        ("jadpo_host_workers", "deployable::fetch"),
    ] {
        assert!(names(&module, krate, path), "it lacks {krate}::{path}");
    }

    // The screen names the Worker's files and the module's size, and no
    // longer says the Worker answers nothing.
    let screen = jadpo(&root, &["build", "kiosk"]).stdout;
    for shown in [
        "worker: build/target/kiosk/wrangler.json",
        "module: build/target/kiosk/kiosk.wasm",
        "MB gzipped",
        "build: b_",
        "settings: DATABASE_URL",
        "built: kiosk, wasm in ",
    ] {
        assert!(screen.contains(shown), "{shown} isn't in:\n{screen}");
    }
    assert!(!screen.contains("503"), "{screen}");

    if !jadpo_test::live() {
        return;
    }
    let database = Database::on_tcp();
    let migrations = migrations(&root);
    let (last, earlier) = migrations.split_last().expect("migrations");
    apply(&database.native, earlier);

    // Without its database's address it serves nothing: every request is
    // 503, and its isolate logs why once, naming the setting's variable.
    let worker = Workerd::start(&out, &wrangler, &[]);
    for _ in 0..2 {
        let (status, body) = worker.send("POST", "/repairs", &[], LAMP);
        assert_eq!(status, 503, "{body}\n{}", worker.log());
        let answer: Value = serde_json::from_str(&body).expect("JSON");
        assert_eq!(answer["error"]["code"], "unavailable", "{answer}");
        assert!(
            answer["error"]["request_id"]
                .as_str()
                .is_some_and(|id| id.starts_with("req_")),
            "{answer}"
        );
    }
    let failed = worker.events("runtime.startup_failed");
    assert_eq!(failed.len(), 1, "{}", worker.log());
    stamped(&failed[0], built, kiosk);
    let problem = &failed[0]["problems"][0];
    assert_eq!(problem["problem"], "setting_missing", "{problem}");
    assert_eq!(problem["variable"], "DATABASE_URL", "{problem}");
    drop(worker);

    // Against a database that hasn't applied its last migration it serves
    // nothing either, naming the migration.
    let worker = Workerd::start(&out, &wrangler, &[("DATABASE_URL", &database.worker)]);
    let (status, body) = worker.send("POST", "/repairs", &[], LAMP);
    assert_eq!(status, 503, "{body}\n{}", worker.log());
    let failed = worker.events("runtime.startup_failed");
    assert_eq!(failed.len(), 1, "{}", worker.log());
    stamped(&failed[0], built, kiosk);
    let problem = &failed[0]["problems"][0];
    assert_eq!(problem["problem"], "migrations_pending", "{failed:?}");
    assert_eq!(problem["store"], "main", "{failed:?}");
    assert_eq!(problem["pending"], serde_json::json!([last.name]));
    assert!(
        !worker.log().contains(&database.worker),
        "the log holds the database's address:\n{}",
        worker.log()
    );

    // Up to date, the same isolate checks again and serves: the booking's
    // subscribers find it the staffed bench and print its ticket while the
    // request waits, and the answer is the ticket read back.
    apply(&database.native, &migrations);
    rows(
        &database.native,
        "INSERT INTO bench (id, item, fixer) VALUES ('6f0c4d5e-2b1a-4c3d-9e8f-7a6b5c4d3e2f', 'lamp', 'Ada')",
    );
    let (status, body) = worker.send("POST", "/repairs", &[], LAMP);
    assert_eq!(status, 201, "{body}\n{}", worker.log());
    let ticket: Value = serde_json::from_str(&body).expect("a ticket");
    assert_eq!(ticket["item"], "lamp", "{ticket}");
    assert_eq!(
        ticket["bench_id"], "6f0c4d5e-2b1a-4c3d-9e8f-7a6b5c4d3e2f",
        "{ticket}"
    );
    assert!(ticket["ticket_printed_at"].is_string(), "{ticket}");
    let repair = ticket["id"].as_str().expect("its id").to_string();
    assert_eq!(
        rows(
            &database.native,
            &format!("SELECT item, bench_id::text FROM repair WHERE id = '{repair}'"),
        ),
        vec![vec![
            "lamp".to_string(),
            "6f0c4d5e-2b1a-4c3d-9e8f-7a6b5c4d3e2f".to_string()
        ]]
    );
    let started = worker.events("runtime.started");
    assert_eq!(started.len(), 1, "{}", worker.log());
    stamped(&started[0], built, kiosk);
    assert_eq!(started[0]["unbuilt"], serde_json::json!([]), "{started:?}");

    // A thank-you is answered at once, its note pinned up after the
    // answer, in `waitUntil`.
    let (status, body) = worker.send(
        "POST",
        &format!("/repairs/{repair}/thanks"),
        &[],
        r#"{"note":"Good as new, thank you!"}"#,
    );
    assert_eq!(status, 202, "{body}\n{}", worker.log());
    let mut pinned = Vec::new();
    for _ in 0..100 {
        pinned = rows(
            &database.native,
            &format!("SELECT note FROM thank_you_note WHERE repair_id = '{repair}'"),
        );
        if !pinned.is_empty() {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    assert_eq!(
        pinned,
        vec![vec!["Good as new, thank you!".to_string()]],
        "{}",
        worker.log()
    );

    // Its readiness check reaches the database.
    let (status, body) = worker.send("GET", "/health/ready", &[], "");
    assert_eq!(status, 200, "{body}\n{}", worker.log());

    // Each address `CF-Connecting-IP` gives is counted on its own, in the
    // rate-limiting binding the Wrangler file names: 120 requests a
    // minute, then 429, while another address still has its allowance.
    let one = [("CF-Connecting-IP", "203.0.113.9")];
    for _ in 0..120 {
        let (status, body) = worker.send("GET", "/nowhere", &one, "");
        assert_eq!(status, 404, "{body}");
    }
    let (status, body) = worker.send("GET", "/nowhere", &one, "");
    assert_eq!(status, 429, "{body}");
    assert_eq!(
        worker
            .log()
            .matches("rate-limiting binding: address 203.0.113.9 ")
            .count(),
        121,
        "{}",
        worker.log()
    );
    let (status, body) = worker.send(
        "GET",
        "/nowhere",
        &[("CF-Connecting-IP", "203.0.113.10")],
        "",
    );
    assert_eq!(status, 404, "{body}");

    // Every line it logged is one JSON object carrying the stamp.
    for event in worker.events("") {
        stamped(&event, built, kiosk);
    }
}

/// A booking for a lamp.
const LAMP: &str = r#"{"item":"lamp","symptom":"flickers when the shade is touched"}"#;

/// The stamp's part of an event, as the build said it of `deployable`.
fn stamped(event: &Value, built: &Value, deployable: &Value) {
    for key in ["commit", "modified", "compiler"] {
        assert_eq!(event[key], built[key], "{key} in {event}");
    }
    for key in ["build", "deployable", "target"] {
        assert_eq!(event[key], deployable[key], "{key} in {event}");
    }
}

/// A database a Worker reaches over TCP on this machine, and the server
/// it is on.
struct Database {
    /// Its address as the native client opens it.
    native: String,
    /// Its address as the Worker's `DATABASE_URL`: TCP to `127.0.0.1`.
    worker: String,
    _server: Server,
}

impl Database {
    /// A new database on the run's server when `JADPO_TEST_DATABASE_URL`
    /// names it by a URL, and otherwise on a server of the test's own
    /// listening on `127.0.0.1`.
    fn on_tcp() -> Database {
        let named = std::env::var(jadpo_test::postgres::DATABASE_URL_VARIABLE)
            .is_ok_and(|url| url.starts_with("postgres://") || url.starts_with("postgresql://"));
        let server = if named {
            Server::start()
        } else {
            Server::start_on_tcp(None)
        }
        .unwrap_or_else(|error| panic!("{error}"));
        let native = server.create_database("workers").expect("a database");
        let worker = match server.tcp_port() {
            // The pairs a disposable server gives, as a URL over TCP: its
            // database is the last `dbname=`, as libpq reads it.
            Some(port) => {
                let database = native
                    .split_whitespace()
                    .rev()
                    .find_map(|pair| pair.strip_prefix("dbname="))
                    .map(|name| name.trim_matches('\''))
                    .expect("a database's name");
                format!("postgresql://jadpo@127.0.0.1:{port}/{database}")
            }
            None => native.clone(),
        };
        Database {
            native,
            worker,
            _server: server,
        }
    }
}

/// workerd serving a built Worker on a port of this machine, stopped
/// when dropped.
struct Workerd {
    child: Child,
    port: u16,
    log: PathBuf,
}

impl Workerd {
    /// The Worker in `out` as `wrangler` describes it: its main module and
    /// the modules beside it by the file's rules, its compatibility date
    /// and its rate-limiting bindings, each a stand-in for Cloudflare's
    /// (tools/workers/ratelimit-binding.mjs), given the text bindings
    /// `bindings`, its outbound network reaching this machine.
    fn start(out: &Path, wrangler: &Value, bindings: &[(&str, &str)]) -> Workerd {
        let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("workers-workerd");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("the folder");
        let main = wrangler["main"].as_str().expect("a main module");
        let mut modules = vec![format!("(name = \"{main}\", esModule = embed \"{main}\")")];
        for entry in fs::read_dir(out).expect("the Worker's folder") {
            let path = entry.expect("an entry").path();
            let name = path
                .file_name()
                .expect("a name")
                .to_string_lossy()
                .to_string();
            if !path.is_file() || name == main {
                continue;
            }
            let kind = match path.extension().and_then(|extension| extension.to_str()) {
                Some("js" | "mjs") => "esModule",
                Some("wasm") => "wasm",
                _ => continue,
            };
            modules.push(format!("(name = \"{name}\", {kind} = embed \"{name}\")"));
        }
        for module in fs::read_dir(out).expect("the Worker's folder") {
            let path = module.expect("an entry").path();
            if path.is_file() {
                fs::copy(&path, dir.join(path.file_name().expect("a name"))).expect("copied");
            }
        }
        let mut bindings: Vec<String> = bindings
            .iter()
            .map(|(name, value)| format!("(name = \"{name}\", text = \"{value}\")"))
            .collect();
        let tools = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/workers");
        fs::copy(
            tools.join("ratelimit-binding.mjs"),
            dir.join("ratelimit-binding.mjs"),
        )
        .expect("the stand-in rate-limiting binding");
        for binding in wrangler["ratelimits"].as_array().into_iter().flatten() {
            bindings.push(format!(
                "(name = \"{}\", wrapped = (moduleName = \"jadpo-test:ratelimit\", innerBindings = [(name = \"limit\", json = \"{}\"), (name = \"period\", json = \"{}\"), (name = \"log\", json = \"true\")]))",
                binding["name"].as_str().expect("a binding's name"),
                binding["simple"]["limit"],
                binding["simple"]["period"],
            ));
        }
        let port = TcpListener::bind("127.0.0.1:0")
            .and_then(|listener| listener.local_addr())
            .expect("a port")
            .port();
        let date = wrangler["compatibility_date"].as_str().expect("a date");
        fs::write(
            dir.join("config.capnp"),
            format!(
                "using Workerd = import \"/workerd/workerd.capnp\";\n\n\
                 const config :Workerd.Config = (\n\
                 \x20 services = [\n\
                 \x20   (name = \"kiosk\", worker = .kiosk),\n\
                 \x20   (name = \"machine\", network = (allow = [\"public\", \"private\", \"local\"])),\n\
                 \x20 ],\n\
                 \x20 sockets = [(name = \"http\", address = \"127.0.0.1:{port}\", http = (), service = \"kiosk\")],\n\
                 \x20 extensions = [(modules = [(name = \"jadpo-test:ratelimit\", internal = true, esModule = embed \"ratelimit-binding.mjs\")])],\n\
                 );\n\n\
                 const kiosk :Workerd.Worker = (\n\
                 \x20 modules = [{}],\n\
                 \x20 compatibilityDate = \"{date}\",\n\
                 \x20 bindings = [{}],\n\
                 \x20 globalOutbound = \"machine\",\n\
                 );\n",
                modules.join(", "),
                bindings.join(", "),
            ),
        )
        .expect("the config");
        let log = dir.join("workerd.log");
        let file = fs::File::create(&log).expect("the log");
        let child = Command::new(workerd())
            .args(["serve", "config.capnp"])
            .current_dir(&dir)
            .stdin(Stdio::null())
            .stdout(file.try_clone().expect("the log"))
            .stderr(file)
            .spawn()
            .expect("workerd starts");
        let mut worker = Workerd { child, port, log };
        let until = Instant::now() + Duration::from_secs(60);
        while TcpStream::connect(("127.0.0.1", port)).is_err() {
            if let Ok(Some(status)) = worker.child.try_wait() {
                panic!("workerd ended ({status}):\n{}", worker.log());
            }
            assert!(
                Instant::now() < until,
                "workerd didn't listen within a minute:\n{}",
                worker.log()
            );
            std::thread::sleep(Duration::from_millis(50));
        }
        worker
    }

    fn log(&self) -> String {
        fs::read_to_string(&self.log).unwrap_or_default()
    }

    /// The events the Worker has logged whose name starts with `event`,
    /// each a line of JSON; every line that names an event is one.
    fn events(&self, event: &str) -> Vec<Value> {
        self.log()
            .lines()
            .filter(|line| line.starts_with("{\"") && line.contains("\"event\""))
            .map(|line| {
                serde_json::from_str::<Value>(line)
                    .unwrap_or_else(|_| panic!("a log line is one JSON object: {line}"))
            })
            .filter(|logged| {
                logged["event"]
                    .as_str()
                    .is_some_and(|name| name.starts_with(event))
            })
            .collect()
    }

    /// One HTTP/1.1 exchange: the status and the body, read as far as the
    /// answer goes rather than until workerd closes the connection, which
    /// it may do only once the invocation's work after the answer has
    /// ended.
    fn send(
        &self,
        method: &str,
        path: &str,
        headers: &[(&str, &str)],
        body: &str,
    ) -> (u16, String) {
        let mut stream = TcpStream::connect(("127.0.0.1", self.port)).expect("workerd listens");
        stream
            .set_read_timeout(Some(Duration::from_secs(60)))
            .expect("a timeout");
        let mut head = format!(
            "{method} {path} HTTP/1.1\r\nHost: kiosk\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n",
            body.len()
        );
        for (name, value) in headers {
            head.push_str(&format!("{name}: {value}\r\n"));
        }
        write!(stream, "{head}\r\n{body}").expect("sent");
        let mut raw = Vec::new();
        let mut byte = [0u8; 1];
        while !raw.ends_with(b"\r\n\r\n") && stream.read(&mut byte).unwrap_or(0) == 1 {
            raw.push(byte[0]);
        }
        let head = String::from_utf8_lossy(&raw).to_ascii_lowercase();
        let status = head
            .split_whitespace()
            .nth(1)
            .and_then(|status| status.parse().ok())
            .unwrap_or_else(|| panic!("an HTTP answer: {head}\n{}", self.log()));
        let length = head.lines().find_map(|line| {
            line.strip_prefix("content-length:")
                .and_then(|length| length.trim().parse::<usize>().ok())
        });
        let mut body = Vec::new();
        match length {
            Some(length) => {
                body.resize(length, 0);
                stream.read_exact(&mut body).expect("the body");
            }
            None if head.contains("transfer-encoding: chunked") => {
                while !body.ends_with(b"0\r\n\r\n") && stream.read(&mut byte).unwrap_or(0) == 1 {
                    body.push(byte[0]);
                }
                return (status, dechunk(&String::from_utf8_lossy(&body)));
            }
            None => {
                let _ = stream.read_to_end(&mut body);
            }
        }
        (status, String::from_utf8_lossy(&body).into_owned())
    }
}

impl Drop for Workerd {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// A chunked body's text.
fn dechunk(mut body: &str) -> String {
    let mut out = String::new();
    while let Some((size, rest)) = body.split_once("\r\n") {
        let size = usize::from_str_radix(size.trim(), 16).unwrap_or(0);
        if size == 0 || rest.len() < size {
            break;
        }
        out.push_str(&rest[..size]);
        body = rest[size..].trim_start_matches("\r\n");
    }
    out
}

/// workerd: `JADPO_WORKERD`, else the one pinned in `tools/workers/`,
/// installed there by `npm ci` when it isn't yet (as the Workers host's
/// tests install it).
fn workerd() -> PathBuf {
    if let Ok(path) = std::env::var("JADPO_WORKERD") {
        return PathBuf::from(path);
    }
    let tools = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/workers");
    if let Some(found) = installed(&tools) {
        return found;
    }
    // No install scripts: the platform's package carries the binary.
    let output = Command::new("npm")
        .args(["ci", "--ignore-scripts", "--no-audit", "--no-fund"])
        .current_dir(&tools)
        .output()
        .expect("npm runs: workerd is installed with Node's npm, into tools/workers/");
    assert!(
        output.status.success(),
        "`npm ci` in tools/workers/ failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    installed(&tools).expect("npm installed workerd for this platform")
}

/// The binary of the platform's own workerd package, rather than npm's
/// wrapper, which would run it as a child of its own.
fn installed(tools: &Path) -> Option<PathBuf> {
    fs::read_dir(tools.join("node_modules/@cloudflare"))
        .ok()?
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().starts_with("workerd-"))
        .map(|entry| entry.path().join("bin/workerd"))
        .find(|binary| binary.is_file())
}
