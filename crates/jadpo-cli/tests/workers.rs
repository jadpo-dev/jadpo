//! A Workers deployable, built and loaded by workerd (roadmap chunk 8.2b;
//! docs/compiler-runtime.md §2.6, "A Workers deployable"; area 14, "A
//! Workers deployable"): teaching example 23, the repair café, declared as
//! one kiosk built for `Target.wasm`. `jadpo build` writes it in
//! `build/target/kiosk/` as a WebAssembly module bound by wasm-bindgen's
//! library, the JavaScript that binds it, Jadpo's module shim and a
//! Wrangler file naming them, the setting it needs and the compatibility
//! date; its stamp's target is `wasm`, the screen shows the module's size
//! raw and gzipped, and the module links the PostgreSQL driver the café's
//! database needs and none of the testing machinery. workerd, Cloudflare's
//! own Workers runtime, pinned in `tools/workers/`, then runs the files as
//! the Wrangler file describes them: the shim instantiates the module and
//! the program's plan is read inside it, and the Worker answers.
//!
//! The build is kept beside the build's own folders
//! (`target/jadpo-compiled/workers/`), so a second run compiles only what
//! changed. Nothing leaves this machine: workerd is installed into
//! `tools/workers/` by `npm ci --ignore-scripts` the first time
//! (`JADPO_WORKERD` names another), and the Worker reaches nothing.

use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use jadpo_cli::run;
use jadpo_term::Environment;
use serde_json::Value;

use crate::deployable::copy_folder;

/// The café declares one kiosk, built for Workers, serving its API and
/// running its subscribers and schedules.
const DECLARED: &str = "application {\n    connections: {\n        main: {\n            driver: Driver.postgres\n            url: config.database_url\n        }\n    }\n    events: {\n        store: main\n    }\n    deployables: {\n        kiosk: {\n            serves: [main]\n            runs: [Trigger.subscribers, Trigger.schedules]\n            target: Target.wasm\n        }\n    }\n}\n";

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
fn a_teaching_example_is_built_for_workers_and_loaded_by_workerd() {
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
    let shim = fs::read_to_string(out.join("worker.mjs")).expect("the shim");
    assert!(shim.contains("from \"./kiosk.js\""), "{shim}");
    let js = fs::read_to_string(out.join("kiosk.js")).expect("the module's JavaScript");
    assert!(js.contains("export function fetch("), "{js}");
    assert!(js.contains("initSync"), "{js}");

    // It links the PostgreSQL driver, which the café's database needs, and
    // no SMTP driver, since it sends no email; none of the fakes,
    // recordings, crash points, records or a test's steps.
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
        ("jadpo_program_kiosk", "cafe"),
        ("jadpo_host_workers", "deployable::fetch"),
    ] {
        assert!(names(&module, krate, path), "it lacks {krate}::{path}");
    }

    // The screen names the Worker's files and the module's size, and says
    // the Worker answers nothing yet.
    let screen = jadpo(&root, &["build", "kiosk"]).stdout;
    for shown in [
        "worker: build/target/kiosk/wrangler.json",
        "module: build/target/kiosk/kiosk.wasm",
        "MB gzipped",
        "build: b_",
        "settings: DATABASE_URL",
        "alert: `kiosk` answers 503 to every request: serving on Workers is chunk 8.2d",
        "built: kiosk, wasm in ",
    ] {
        assert!(screen.contains(shown), "{shown} isn't in:\n{screen}");
    }

    // workerd runs the files as the Wrangler file names them: the shim
    // instantiates the module, the program's plan is read inside it, and
    // the Worker answers that it serves nothing yet.
    let worker = Workerd::start(&out, &wrangler);
    let (status, body) = worker.get("/repairs");
    assert_eq!(status, 503, "{body}\n{}", worker.log());
    let answer: Value = serde_json::from_str(&body).expect("JSON");
    assert_eq!(answer["error"]["code"], "unavailable", "{answer}");
    assert_eq!(
        answer["error"]["message"], "This Worker doesn't answer requests yet.",
        "{answer}"
    );
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
    /// and a value for each setting it requires.
    fn start(out: &Path, wrangler: &Value) -> Workerd {
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
        let bindings: Vec<String> = wrangler["secrets"]["required"]
            .as_array()
            .map(|required| {
                required
                    .iter()
                    .filter_map(Value::as_str)
                    .map(|name| {
                        format!(
                            "(name = \"{name}\", text = \"postgresql://kiosk@127.0.0.1:1/kiosk\")"
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
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
                 \x20 services = [(name = \"kiosk\", worker = .kiosk)],\n\
                 \x20 sockets = [(name = \"http\", address = \"127.0.0.1:{port}\", http = (), service = \"kiosk\")],\n\
                 );\n\n\
                 const kiosk :Workerd.Worker = (\n\
                 \x20 modules = [{}],\n\
                 \x20 compatibilityDate = \"{date}\",\n\
                 \x20 bindings = [{}],\n\
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

    /// `GET path`: the status and the body.
    fn get(&self, path: &str) -> (u16, String) {
        let mut stream = TcpStream::connect(("127.0.0.1", self.port)).expect("workerd listens");
        stream
            .set_read_timeout(Some(Duration::from_secs(60)))
            .expect("a timeout");
        write!(
            stream,
            "GET {path} HTTP/1.1\r\nHost: kiosk\r\nConnection: close\r\n\r\n"
        )
        .expect("sent");
        let mut answer = String::new();
        stream.read_to_string(&mut answer).expect("an answer");
        let status = answer
            .split(' ')
            .nth(1)
            .and_then(|code| code.parse().ok())
            .unwrap_or_else(|| panic!("an HTTP answer: {answer}\n{}", self.log()));
        let (head, body) = answer.split_once("\r\n\r\n").unwrap_or((&answer, ""));
        let body = if head
            .to_ascii_lowercase()
            .contains("transfer-encoding: chunked")
        {
            dechunk(body)
        } else {
            body.to_string()
        };
        (status, body)
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
