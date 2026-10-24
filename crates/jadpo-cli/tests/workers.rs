//! Workers deployables, built and served by workerd (roadmap chunks 8.2b,
//! 8.2d, 8.2f, 8.2h, 8.2i and 8.2k; docs/compiler-runtime.md §2.6, "A
//! Workers deployable"; area 14, "A Workers deployable"): the harness the
//! scenarios share, a file a scenario: teaching example 23's kiosk
//! (`kiosk.rs`), the Workers cron on a depot (`depot.rs`), after the
//! response on a news desk (`newsroom.rs`) and Cloudflare Queues' consumer
//! on a warehouse (`queues.rs`).
//!
//! workerd, Cloudflare's own Workers runtime, pinned in `tools/workers/`,
//! runs each Worker's files as its Wrangler file describes them, against a
//! PostgreSQL database on this machine reached through workerd's
//! Hyperdrive binding, which hands `connect()` to the database's server.
//!
//! The projects are kept beside the build's own folders
//! (`target/jadpo-compiled/workers/`) and built into the cache the native
//! deployables' tests use (`target/jadpo-compiled/cache/`, chunk 8.4g), so
//! the runtime compiles once for every scenario and a second run compiles
//! only what changed. The database is the run's when
//! `JADPO_TEST_DATABASE_URL` names one by a URL, as CI's does (`127.0.0.1`), and otherwise one this test
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
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use jadpo_cli::run;
use jadpo_term::Environment;
use jadpo_test::postgres::Server;
use serde_json::Value;

use crate::deployable::{apply, migrations, rows};

/// The Workers cron (chunk 8.2f).
mod depot;
mod external;
/// Teaching example 23's kiosk (chunks 8.2b, 8.2d, 8.2i and 8.4f).
mod kiosk;
/// After the response (chunk 8.2h) and a Worker's limits (chunk 8.4a).
mod newsroom;
/// Cloudflare Queues' consumer (chunk 8.2k).
mod queues;

/// `jadpo` with `args` in `root`, which must succeed.
fn jadpo(root: &Path, args: &[&str]) -> jadpo_cli::Output {
    jadpo_in(root, args, Environment::empty())
}

/// `jadpo` with `args` in `root` and `environment`, which must succeed.
fn jadpo_in(root: &Path, args: &[&str], environment: Environment) -> jadpo_cli::Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let cache = crate::deployable::shared_cache();
    let environment = environment
        .with_cwd(root)
        .with_var("XDG_CACHE_HOME", &cache.to_string_lossy());
    let output = run(&args, &environment);
    assert_eq!(
        output.code, 0,
        "{args:?}: {}{}",
        output.stdout, output.stderr
    );
    output
}

/// The module cargo built for `deployable` of the project at `root`, in
/// the release profile it ships in, before it is bound: its names still
/// in it, which the module the Worker is deployed from leaves out (chunk
/// 8.4f).
fn linked(root: &Path, deployable: &str) -> Vec<u8> {
    let module = root.join(format!(
        "build/rust/{deployable}/jadpo_worker_{deployable}.wasm"
    ));
    fs::read(&module).unwrap_or_else(|error| panic!("{}: {error}", module.display()))
}

/// The names of the custom sections of WebAssembly module `module`.
fn custom_sections(module: &[u8]) -> Vec<String> {
    fn leb(bytes: &[u8], at: &mut usize) -> usize {
        let (mut value, mut shift) = (0usize, 0);
        loop {
            let byte = bytes[*at];
            *at += 1;
            value |= usize::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return value;
            }
            shift += 7;
        }
    }
    let mut sections = Vec::new();
    let mut at = 8;
    while at < module.len() {
        let id = module[at];
        at += 1;
        let size = leb(module, &mut at);
        let end = at + size;
        if id == 0 {
            let length = leb(module, &mut at);
            sections.push(String::from_utf8_lossy(&module[at..at + length]).into_owned());
        }
        at = end;
    }
    sections
}

/// Whether the module's names hold a path in crate `krate` starting with
/// `path`, as rustc writes them: `jadpo_runtime[9f2c…]::fakes` or
/// `_ZN13jadpo_runtime5fakes…`.
fn names(module: &[u8], krate: &str, path: &str) -> bool {
    let mangled: String = std::iter::once(krate)
        .chain(path.trim_end_matches(':').split("::"))
        .map(|name| format!("{}{name}", name.len()))
        .collect();
    if module
        .windows(mangled.len())
        .any(|window| window == mangled.as_bytes())
    {
        return true;
    }
    let text = String::from_utf8_lossy(module);
    let pattern = regex::Regex::new(&format!(
        r"{}\[[0-9a-f]+\]::{}",
        regex::escape(krate),
        regex::escape(path)
    ))
    .expect("a pattern");
    pattern.is_match(&text)
}

/// The time on this machine, in milliseconds since 1970.
fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_millis() as i64)
}

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
/// when dropped, and beside it a Worker of the test's own on another port
/// that runs the built Worker's `scheduled` handler as Cloudflare's cron
/// trigger does, through a service binding's `scheduled()` (workerd's
/// `service_binding_extra_handlers`, which needs `--experimental`).
struct Workerd {
    child: Child,
    port: u16,
    cron: u16,
    log: PathBuf,
}

/// The test's cron: each request runs the bound Worker's `scheduled`
/// handler for the time its `?at=` gives, in milliseconds, and answers
/// with how it ended; a request to `/queue` hands the bound Worker's
/// `queue` handler the batch its body gives, as Cloudflare Queues does
/// (chunk 8.2k, `queues.rs`), and answers with how each message was; and
/// a request to `/memory` answers with the size of the bound Worker's
/// linear memory, through the test's own entrypoint ([`MEMORY_MAIN`]).
const CRON: &str = "export default {\n  async fetch(request, env) {\n    if (new URL(request.url).pathname === \"/memory\") {\n      return Response.json({ bytes: await env.MEMORY.bytes() });\n    }\n    if (new URL(request.url).pathname === \"/queue\") {\n      const { queue, messages } = await request.json();\n      const answered = await env.WORKER.queue(queue, messages.map((message) => ({ ...message, timestamp: new Date() })));\n      return Response.json(answered);\n    }\n    const at = Number(new URL(request.url).searchParams.get(\"at\"));\n    const ended = await env.WORKER.scheduled({ scheduledTime: new Date(at), cron: \"* * * * *\" });\n    return Response.json(ended);\n  },\n};\n";

/// The built Worker's main module as the test serves it (chunk 8.4e): the
/// shim `jadpo build` wrote, re-exported whole, with one entrypoint more,
/// which only the test's cron reaches, giving the size of the module's
/// linear memory from the exports wasm-bindgen's `initSync` hands back once
/// the shim has instantiated the module. Requests reach the shim's
/// handlers as they would on Cloudflare.
const MEMORY_MAIN: &str = "export * from \"./{main}\";\nexport { default } from \"./{main}\";\nimport { WorkerEntrypoint } from \"cloudflare:workers\";\nimport { initSync } from \"./{glue}\";\n\nexport class JadpoTestMemory extends WorkerEntrypoint {\n  bytes() {\n    return initSync().memory.buffer.byteLength;\n  }\n}\n";

/// The test's main module's name, beside the Worker's own files.
const TEST_MAIN: &str = "jadpo-test-main.mjs";

impl Workerd {
    /// The Worker in `out` as `wrangler` describes it: its main module and
    /// the modules beside it by the file's rules, its compatibility date
    /// and its rate-limiting bindings, each a stand-in for Cloudflare's
    /// (tools/workers/ratelimit-binding.mjs), given `bindings`, its
    /// outbound network reaching this machine. A binding the Wrangler file
    /// names as Hyperdrive's is given as workerd's Hyperdrive binding for
    /// the database at that address, which hands `connect()` to its server,
    /// as `wrangler dev` does with a local connection string; any other is
    /// text.
    fn start(out: &Path, wrangler: &Value, bindings: &[(&str, &str)]) -> Workerd {
        Self::start_with_provider(out, wrangler, bindings, None)
    }

    fn start_with_provider(
        out: &Path,
        wrangler: &Value,
        bindings: &[(&str, &str)],
        provider: Option<&crate::external_tokens::Provider>,
    ) -> Workerd {
        // A folder of its own, config and log, since this module's tests
        // run side by side, each with its workerd.
        static STARTED: AtomicUsize = AtomicUsize::new(0);
        let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!(
            "workers-workerd/{}-{}",
            wrangler["name"].as_str().unwrap_or("worker"),
            STARTED.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("the folder");
        let main = wrangler["main"].as_str().expect("a main module");
        // The test's main module first, which workerd takes as the main.
        let mut modules = vec![format!(
            "(name = \"{TEST_MAIN}\", esModule = embed \"{TEST_MAIN}\")"
        )];
        let mut glue = Vec::new();
        for entry in fs::read_dir(out).expect("the Worker's folder") {
            let path = entry.expect("an entry").path();
            let name = path
                .file_name()
                .expect("a name")
                .to_string_lossy()
                .to_string();
            if !path.is_file() {
                continue;
            }
            let kind = match path.extension().and_then(|extension| extension.to_str()) {
                Some("js") => {
                    glue.push(name.clone());
                    "esModule"
                }
                Some("mjs") => "esModule",
                Some("wasm") => "wasm",
                _ => continue,
            };
            modules.push(format!("(name = \"{name}\", {kind} = embed \"{name}\")"));
        }
        // wasm-bindgen's JavaScript, which the shim instantiates the module
        // through: the one `.js` beside it.
        let [glue] = glue.as_slice() else {
            panic!("one module's JavaScript beside the shim: {glue:?}");
        };
        fs::write(
            dir.join(TEST_MAIN),
            MEMORY_MAIN.replace("{main}", main).replace("{glue}", glue),
        )
        .expect("the test's main module");
        for module in fs::read_dir(out).expect("the Worker's folder") {
            let path = module.expect("an entry").path();
            if path.is_file() {
                fs::copy(&path, dir.join(path.file_name().expect("a name"))).expect("copied");
            }
        }
        let pooled: Vec<&str> = wrangler["hyperdrive"]
            .as_array()
            .map(|bindings| {
                bindings
                    .iter()
                    .filter_map(|binding| binding["binding"].as_str())
                    .collect()
            })
            .unwrap_or_default();
        let mut services = Vec::new();
        let mut bindings: Vec<String> = bindings
            .iter()
            .map(|(name, value)| {
                if !pooled.contains(name) {
                    return format!("(name = \"{name}\", text = \"{value}\")");
                }
                let (user, password, server, database) = parts(value);
                let service = format!("hyperdrive-{}", name.to_ascii_lowercase());
                services.push(format!(
                    "(name = \"{service}\", external = (address = \"{server}\", tcp = ())),"
                ));
                format!(
                    "(name = \"{name}\", hyperdrive = (designator = \"{service}\", database = \"{database}\", user = \"{user}\", password = \"{password}\", scheme = \"postgresql\"))"
                )
            })
            .collect();
        // Work is handed on through `ctx.exports`, which workerd gives
        // each entrypoint the Worker exports (chunk 8.4b): no binding.
        assert!(wrangler.get("services").is_none(), "{wrangler}");
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
        let free = || {
            TcpListener::bind("127.0.0.1:0")
                .and_then(|listener| listener.local_addr())
                .expect("a port")
                .port()
        };
        let (port, cron) = (free(), free());
        let date = wrangler["compatibility_date"].as_str().expect("a date");
        fs::write(dir.join("cron.mjs"), CRON).expect("the cron");
        fs::write(
            dir.join("config.capnp"),
            format!(
                "using Workerd = import \"/workerd/workerd.capnp\";\n\n\
                 const config :Workerd.Config = (\n\
                 \x20 services = [\n\
                 \x20   (name = \"kiosk\", worker = .kiosk),\n\
                 \x20   (name = \"cron\", worker = .cron),\n\
                 \x20   (name = \"machine\", network = (allow = [\"public\", \"private\", \"local\"])),\n\
                 \x20   {}\n\
                 \x20 ],\n\
                 \x20 sockets = [\n\
                 \x20   (name = \"http\", address = \"127.0.0.1:{port}\", http = (), service = \"kiosk\"),\n\
                 \x20   (name = \"cron\", address = \"127.0.0.1:{cron}\", http = (), service = \"cron\"),\n\
                 \x20 ],\n\
                 \x20 extensions = [(modules = [(name = \"jadpo-test:ratelimit\", internal = true, esModule = embed \"ratelimit-binding.mjs\")])],\n\
                 );\n\n\
                 const kiosk :Workerd.Worker = (\n\
                 \x20 modules = [{}],\n\
                 \x20 compatibilityDate = \"{date}\",\n\
                 \x20 bindings = [{}],\n\
                 \x20 globalOutbound = \"machine\",\n\
                 );\n\n\
                 const cron :Workerd.Worker = (\n\
                 \x20 modules = [(name = \"cron.mjs\", esModule = embed \"cron.mjs\")],\n\
                 \x20 compatibilityDate = \"{date}\",\n\
                 \x20 compatibilityFlags = [\"service_binding_extra_handlers\"],\n\
                 \x20 bindings = [(name = \"WORKER\", service = \"kiosk\"), (name = \"MEMORY\", service = (name = \"kiosk\", entrypoint = \"JadpoTestMemory\"))],\n\
                 );\n",
                services.join("\n    "),
                modules.join(", "),
                bindings.join(", "),
            ),
        )
        .expect("the config");
        // A test-only outbound service maps two public fixture hosts to
        // local TLS. The compiled production Worker and its public-fetch
        // flag are unchanged; the fixture CA still verifies the connection.
        let config_path = dir.join("config.capnp");
        let mut configuration = fs::read_to_string(&config_path).expect("config");
        if let Some(flags) = wrangler
            .get("compatibility_flags")
            .and_then(Value::as_array)
        {
            let flags: Vec<String> = flags
                .iter()
                .map(|flag| serde_json::to_string(flag).expect("flag"))
                .collect();
            configuration = configuration.replace(
                &format!("compatibilityDate = \"{date}\",\n"),
                &format!(
                    "compatibilityDate = \"{date}\",\n  compatibilityFlags = [{}],\n",
                    flags.join(", ")
                ),
            );
        }
        if let Some(provider) = provider {
            fs::write(
                dir.join("provider-authority.pem"),
                &provider.authority.root_pem,
            )
            .expect("fixture CA");
            let proxy = format!(
                r#"export default {{ async fetch(request) {{
              const url = new URL(request.url);
              if (!["identity.example", "keys.example"].includes(url.hostname)) return new Response("unknown fixture host", {{status:502}});
              url.hostname = "localhost"; url.port = "{}";
              return fetch(new Request(url, request));
            }} }};"#,
                provider.port
            );
            fs::write(dir.join("provider.mjs"), proxy).expect("fixture proxy");
            configuration = configuration.replace("(name = \"cron\", worker = .cron),", "(name = \"cron\", worker = .cron),\n    (name = \"provider\", worker = .provider),");
            configuration = configuration.replace("network = (allow = [\"public\", \"private\", \"local\"])", "network = (allow = [\"public\", \"private\", \"local\"], tlsOptions = (trustedCertificates = [embed \"provider-authority.pem\"]))");
            configuration = configuration.replace(
                "globalOutbound = \"machine\"",
                "globalOutbound = \"provider\"",
            );
            configuration.push_str(&format!("\nconst provider :Workerd.Worker = (modules = [(name = \"provider.mjs\", esModule = embed \"provider.mjs\")], compatibilityDate = \"{date}\", globalOutbound = \"machine\");\n"));
        }
        fs::write(&config_path, configuration).expect("configured fixture");
        let log = dir.join("workerd.log");
        let file = fs::File::create(&log).expect("the log");
        let child = Command::new(workerd())
            .args(["serve", "--experimental", "config.capnp"])
            .current_dir(&dir)
            .stdin(Stdio::null())
            .stdout(file.try_clone().expect("the log"))
            .stderr(file)
            .spawn()
            .expect("workerd starts");
        let mut worker = Workerd {
            child,
            port,
            cron,
            log,
        };
        let until = Instant::now() + Duration::from_secs(60);
        while TcpStream::connect(("127.0.0.1", port)).is_err()
            || TcpStream::connect(("127.0.0.1", cron)).is_err()
        {
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

    /// Runs the Worker's `scheduled` handler as the cron trigger would for
    /// the minute at `at`, in milliseconds since 1970, waiting for the
    /// invocation to end: how it ended, `ok` or `exception`.
    fn scheduled(&self, at: i64) -> String {
        let ended = self.ask_cron(&format!("/?at={at}"));
        ended["outcome"].as_str().unwrap_or_default().to_string()
    }

    /// The built Worker's WebAssembly linear memory now, in bytes, read
    /// through the test's own entrypoint (chunk 8.4e).
    fn linear_memory(&self) -> u64 {
        let answer = self.ask_cron("/memory");
        answer["bytes"]
            .as_u64()
            .unwrap_or_else(|| panic!("the memory's size: {answer}\n{}", self.log()))
    }

    /// The test's cron's JSON answer to a `GET` of `path`.
    fn ask_cron(&self, path: &str) -> Value {
        let mut stream = TcpStream::connect(("127.0.0.1", self.cron)).expect("the cron listens");
        stream
            .set_read_timeout(Some(Duration::from_secs(180)))
            .expect("a timeout");
        write!(
            stream,
            "GET {path} HTTP/1.1\r\nHost: cron\r\nConnection: close\r\n\r\n"
        )
        .expect("sent");
        let mut answer = String::new();
        let _ = stream.read_to_string(&mut answer);
        let body = answer
            .split_once("\r\n\r\n")
            .map(|(head, body)| {
                if head
                    .to_ascii_lowercase()
                    .contains("transfer-encoding: chunked")
                {
                    dechunk(body)
                } else {
                    body.to_string()
                }
            })
            .unwrap_or_else(|| panic!("an HTTP answer: {answer}\n{}", self.log()));
        serde_json::from_str(&body)
            .unwrap_or_else(|_| panic!("the cron's answer: {answer}\n{}", self.log()))
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

/// A PostgreSQL address's user, password, server (`host:port`) and
/// database, each percent-decoded, as libpq reads a URL: a `dbname=` in
/// its query names the database over its path, the last one winning, as
/// the run's server's addresses do (`…/postgres?dbname=jadpo_…`).
fn parts(url: &str) -> (String, String, String, String) {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let (rest, query) = rest.split_once('?').unwrap_or((rest, ""));
    let (user_info, rest) = rest.rsplit_once('@').unwrap_or(("", rest));
    let (user, password) = user_info.split_once(':').unwrap_or((user_info, ""));
    let (server, path) = rest.split_once('/').unwrap_or((rest, ""));
    let database = query
        .split('&')
        .rev()
        .find_map(|pair| pair.strip_prefix("dbname="))
        .unwrap_or(path);
    (
        decoded(user),
        decoded(password),
        server.to_string(),
        decoded(database),
    )
}

/// `text` with each `%XX` read as its byte.
fn decoded(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        let hex = text
            .get(at + 1..at + 3)
            .and_then(|hex| u8::from_str_radix(hex, 16).ok());
        match (bytes[at], hex) {
            (b'%', Some(byte)) => {
                out.push(byte);
                at += 3;
            }
            (byte, _) => {
                out.push(byte);
                at += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[test]
fn an_address_is_read_into_its_parts_as_libpq_reads_it() {
    assert_eq!(
        parts("postgresql://jadpo:jadpo@127.0.0.1:5432/postgres?dbname=jadpo_1_2_workers"),
        (
            "jadpo".to_string(),
            "jadpo".to_string(),
            "127.0.0.1:5432".to_string(),
            "jadpo_1_2_workers".to_string()
        )
    );
    assert_eq!(
        parts("postgres://app:p%40ss@db.test:6543/repair%20cafe?sslmode=x&dbname=a&dbname=b"),
        (
            "app".to_string(),
            "p@ss".to_string(),
            "db.test:6543".to_string(),
            "b".to_string()
        )
    );
    assert_eq!(
        parts("postgresql://jadpo@127.0.0.1:55432/jadpo_workers"),
        (
            "jadpo".to_string(),
            String::new(),
            "127.0.0.1:55432".to_string(),
            "jadpo_workers".to_string()
        )
    );
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
/// installed there by `npm ci` when it isn't yet, one process at a time
/// (`jadpo_test::workerd_install`).
fn workerd() -> PathBuf {
    jadpo_test::workerd_install::workerd().unwrap_or_else(|why| panic!("{why}"))
}
