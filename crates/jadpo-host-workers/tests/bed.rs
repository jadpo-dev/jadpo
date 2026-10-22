//! A test bed for the Workers host under workerd, Cloudflare's own Workers
//! runtime, on this machine (docs/compiler-runtime.md §2.6, "The Workers
//! host"; chunk 8.2a): the probe Worker (examples/probe.rs) built for
//! wasm32 and bound by wasm-bindgen's library at the lock's release, run by
//! the workerd pinned in tools/workers/ (installed there by `npm ci` the
//! first time a bed starts, never globally), beside the servers it
//! reaches: an echo over TCP, the same behind TLS signed by an authority
//! workerd is told to trust, and again by one it isn't, HTTP over plain
//! TCP and over TLS, and PostgreSQL's side of a connection over TLS,
//! signed by each authority (chunk 8.2i); and a Hyperdrive binding whose
//! service is the echo. One bed per test process, started by the first
//! test that asks for it and stopped, with its folder removed, once the
//! process has ended.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::authority::Authority;

/// The probe under workerd, and the servers it reaches.
pub struct Bed {
    /// workerd's port, which serves the probe.
    port: u16,
    /// An echo over plain TCP.
    pub echo: u16,
    /// The echo behind TLS, its certificate for `localhost` signed by the
    /// authority workerd trusts.
    pub echo_tls: u16,
    /// The echo behind TLS signed by an authority workerd doesn't trust.
    pub echo_untrusted: u16,
    /// HTTP over plain TCP.
    pub http: u16,
    /// HTTP over TLS, for `localhost`, signed by the trusted authority.
    pub https: u16,
    /// PostgreSQL's answer to an SSL request, then TLS for
    /// `db.example.test` signed by the trusted authority, a startup and
    /// each simple query answered.
    pub postgres_tls: u16,
    /// The same, signed by the authority workerd doesn't trust.
    pub postgres_untrusted: u16,
    /// workerd's output.
    log: PathBuf,
    _workerd: Mutex<Child>,
}

/// The process's bed, started on first use.
pub fn bed() -> &'static Bed {
    static BED: OnceLock<Bed> = OnceLock::new();
    BED.get_or_init(Bed::start)
}

impl Bed {
    fn start() -> Bed {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .expect("the repository");
        let dir = std::env::temp_dir().join(format!("jadpo-workerd-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the bed's folder");

        let trusted = Authority::signing_for(&["localhost", "db.example.test"]);
        let untrusted = Authority::signing_for(&["localhost", "db.example.test"]);
        let plain = serve(None, echo);
        let echo_tls = serve(Some(trusted.server_config()), echo);
        let echo_untrusted = serve(Some(untrusted.server_config()), echo);
        let http = serve(None, answer);
        let https = serve(Some(trusted.server_config()), answer);
        let postgres_tls = serve_postgres(trusted.server_config());
        let postgres_untrusted = serve_postgres(untrusted.server_config());

        let wasm = probe(&root);
        let mut bindgen = wasm_bindgen_cli_support::Bindgen::new();
        bindgen
            .input_path(&wasm)
            .web(true)
            .expect("wasm-bindgen's web target")
            .out_name("probe");
        bindgen
            .generate(&dir)
            .expect("wasm-bindgen binds the probe");
        std::fs::write(dir.join("worker.mjs"), SHIM).expect("the shim");
        std::fs::write(dir.join("authority.pem"), &trusted.root_pem).expect("the authority");
        std::fs::copy(
            root.join("tools/workers").join("ratelimit-binding.mjs"),
            dir.join("ratelimit-binding.mjs"),
        )
        .expect("the stand-in rate-limiting binding");
        let port = free_port();
        std::fs::write(dir.join("config.capnp"), config(port, https, plain)).expect("the config");

        let log = dir.join("workerd.log");
        let out = std::fs::File::create(&log).expect("workerd's log");
        let child = Command::new(workerd())
            .args(["serve", "config.capnp"])
            .current_dir(&dir)
            .stdin(Stdio::null())
            .stdout(out.try_clone().expect("the log"))
            .stderr(out)
            .spawn()
            .expect("workerd starts");
        watchdog(child.id(), &dir);
        let bed = Bed {
            port,
            echo: plain,
            echo_tls,
            echo_untrusted,
            http,
            https,
            postgres_tls,
            postgres_untrusted,
            log,
            _workerd: Mutex::new(child),
        };
        bed.wait_until_listening();
        bed
    }

    fn wait_until_listening(&self) {
        let until = Instant::now() + Duration::from_secs(60);
        while TcpStream::connect(("127.0.0.1", self.port)).is_err() {
            if let Ok(mut child) = self._workerd.lock()
                && let Ok(Some(status)) = child.try_wait()
            {
                panic!("workerd ended ({status}):\n{}", self.log());
            }
            assert!(
                Instant::now() < until,
                "workerd didn't listen within a minute:\n{}",
                self.log()
            );
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// What workerd has printed.
    pub fn log(&self) -> String {
        std::fs::read_to_string(&self.log).unwrap_or_default()
    }

    /// The probe's answer to `GET path`, which must be a 200.
    pub fn get(&self, path: &str) -> String {
        let (status, body) = request(self.port, path);
        assert_eq!(
            status,
            200,
            "GET {path} answered {status}: {body}\nworkerd said:\n{}",
            self.log()
        );
        body
    }
}

/// workerd's configuration: the probe, served on `port`, reaching this
/// machine through a network that trusts the test's authority, given two
/// text bindings, a Hyperdrive binding whose service is the echo on
/// `echo`, as a database's would be its server, and two stand-ins for
/// Cloudflare's rate-limiting binding (tools/workers/ratelimit-binding.mjs):
/// one allowing a request a minute, and one that throws.
fn config(port: u16, https: u16, echo: u16) -> String {
    format!(
        r#"using Workerd = import "/workerd/workerd.capnp";

const config :Workerd.Config = (
  services = [
    (name = "probe", worker = .probe),
    (name = "internet", network = (
      allow = ["public", "private", "local"],
      tlsOptions = (trustedCertificates = [embed "authority.pem"]),
    )),
    (name = "pooled", external = (address = "127.0.0.1:{echo}", tcp = ())),
  ],
  sockets = [(name = "http", address = "127.0.0.1:{port}", http = (), service = "probe")],
  extensions = [(modules = [
    (name = "jadpo-test:ratelimit", internal = true, esModule = embed "ratelimit-binding.mjs"),
  ])],
);

const probe :Workerd.Worker = (
  modules = [
    (name = "worker.mjs", esModule = embed "worker.mjs"),
    (name = "probe.js", esModule = embed "probe.js"),
    (name = "probe_bg.wasm", wasm = embed "probe_bg.wasm"),
  ],
  compatibilityDate = "2026-09-01",
  bindings = [
    (name = "GREETING", text = "hello"),
    (name = "REACH", text = "https://localhost:{https}"),
    (name = "DATABASE", hyperdrive = (
      designator = "pooled",
      database = "repair cafe",
      user = "kiosk",
      password = "p@ss/word!",
      scheme = "postgresql",
    )),
    (name = "LIMITS_2_PER_60S", wrapped = (moduleName = "jadpo-test:ratelimit", innerBindings = [
      (name = "limit", json = "1"),
      (name = "period", json = "60"),
    ])),
    (name = "BROKEN_2_PER_60S", wrapped = (moduleName = "jadpo-test:ratelimit", innerBindings = [
      (name = "limit", json = "1"),
      (name = "period", json = "60"),
      (name = "broken", json = "true"),
    ])),
  ],
  globalOutbound = "internet",
);
"#
    )
}

/// The module workerd runs first: the probe's module instantiated once per
/// isolate, and its `fetch` the Worker's.
const SHIM: &str = r#"import module from "./probe_bg.wasm";
import { initSync, fetch as handle } from "./probe.js";

initSync({ module });

export default {
  fetch(request, env, ctx) {
    return handle(request, env, ctx);
  },
};
"#;

/// The probe built for wasm32 with `test-outbound` and `postgres`, through the cargo that
/// runs this test: its module's path.
fn probe(root: &Path) -> PathBuf {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let output = Command::new(cargo)
        .current_dir(root)
        .args([
            "build",
            "--locked",
            "--target",
            "wasm32-unknown-unknown",
            "-p",
            "jadpo-host-workers",
            "--example",
            "probe",
            "--features",
            "test-outbound,postgres",
            "--message-format=json-render-diagnostics",
        ])
        .output()
        .expect("cargo runs");
    assert!(
        output.status.success(),
        "the probe doesn't build for wasm32:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|message| {
            message["reason"] == "compiler-artifact" && message["target"]["name"] == "probe"
        })
        .flat_map(|message| message["filenames"].as_array().cloned().unwrap_or_default())
        .filter_map(|file| file.as_str().map(PathBuf::from))
        .find(|file| {
            file.extension()
                .is_some_and(|extension| extension == "wasm")
        })
        .expect("cargo names the probe's module")
}

/// The one installer of workerd every test that runs a Worker shares,
/// which installs it once across processes.
#[path = "../../jadpo-test/src/workerd_install.rs"]
mod workerd_install;

/// workerd: `JADPO_WORKERD`, else the one pinned in tools/workers/,
/// installed there by `npm ci` when it isn't yet, one process at a time.
fn workerd() -> PathBuf {
    workerd_install::workerd().unwrap_or_else(|why| panic!("{why}"))
}

/// A process that stops workerd and removes the bed's folder once this
/// process has ended, however it ended: the bed lives in a static, which
/// is never dropped.
fn watchdog(workerd: u32, dir: &Path) {
    let script = r#"trap '' INT HUP
while kill -0 "$0" 2>/dev/null; do sleep 1; done
kill "$1" 2>/dev/null
sleep 1
kill -9 "$1" 2>/dev/null
rm -rf "$2""#;
    let _ = Command::new("/bin/sh")
        .arg("-c")
        .arg(script)
        .arg(std::process::id().to_string())
        .arg(workerd.to_string())
        .arg(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .and_then(|listener| listener.local_addr())
        .expect("a port on this machine")
        .port()
}

/// A stream a server answers on, plain or TLS.
trait Link: Read + Write + Send {
    /// Its connection.
    fn tcp(&self) -> &TcpStream;
}

impl Link for TcpStream {
    fn tcp(&self) -> &TcpStream {
        self
    }
}

impl Link for rustls::StreamOwned<rustls::ServerConnection, TcpStream> {
    fn tcp(&self) -> &TcpStream {
        &self.sock
    }
}

/// How long after each tagged request reached the server its client let
/// the connection go, by the request's `tag`.
static CLOSES: Mutex<Vec<(String, Duration)>> = Mutex::new(Vec::new());

/// How long after the request tagged `tag` reached the HTTP server its
/// client closed the connection, waiting up to three seconds to hear;
/// `None` when it held it open past that.
pub fn closed(tag: &str) -> Option<Duration> {
    let until = Instant::now() + Duration::from_secs(3);
    loop {
        let found = CLOSES.lock().ok().and_then(|closes| {
            closes
                .iter()
                .find(|(seen, _)| seen == tag)
                .map(|(_, at)| *at)
        });
        if found.is_some() || Instant::now() > until {
            return found;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// What the gate saw of a tag's requests: how many waited for their
/// answer's head at once at most, and how many came.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Gate {
    pub most: usize,
    pub came: usize,
    waiting: usize,
}

/// Each tag's requests at the gate.
static GATES: Mutex<Vec<(String, Gate)>> = Mutex::new(Vec::new());

/// A request of `tag` arriving at the gate (`1`) or answered or gone
/// (`-1`).
fn gate_arrived(tag: &str, change: isize) {
    let Ok(mut gates) = GATES.lock() else {
        return;
    };
    let at = match gates.iter().position(|(seen, _)| seen == tag) {
        Some(at) => at,
        None => {
            gates.push((tag.to_string(), Gate::default()));
            gates.len() - 1
        }
    };
    let gate = &mut gates[at].1;
    if change > 0 {
        gate.waiting += 1;
        gate.came += 1;
        gate.most = gate.most.max(gate.waiting);
    } else {
        gate.waiting = gate.waiting.saturating_sub(1);
    }
}

/// What the gate has seen of `tag`'s requests.
pub fn gate(tag: &str) -> Gate {
    GATES
        .lock()
        .ok()
        .and_then(|gates| {
            gates
                .iter()
                .find(|(seen, _)| seen == tag)
                .map(|(_, gate)| *gate)
        })
        .unwrap_or_default()
}

/// Waits up to `most` for the client to close `link`, keeping how long
/// that took under `tag`: whether it did.
fn watch_close(link: &mut dyn Link, tag: &str, began: Instant, most: Duration) -> bool {
    let _ = link.tcp().set_read_timeout(Some(Duration::from_millis(20)));
    let mut byte = [0u8; 1];
    while began.elapsed() < most {
        match link.read(&mut byte) {
            Ok(0) => break,
            Ok(_) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(_) => break,
        }
    }
    let gone = began.elapsed() < most;
    if gone && let Ok(mut closes) = CLOSES.lock() {
        closes.push((tag.to_string(), began.elapsed()));
    }
    gone
}

/// A server on this machine answering each connection with `handle`,
/// behind TLS when given a configuration: its port, on `127.0.0.1` and,
/// where the machine has it, on `::1` too, so `localhost` reaches it
/// whichever of its addresses workerd tries.
fn serve(tls: Option<Arc<rustls::ServerConfig>>, handle: fn(&mut dyn Link)) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a port on this machine");
    let port = listener.local_addr().expect("its address").port();
    let listeners = [Some(listener), TcpListener::bind(("::1", port)).ok()];
    for listener in listeners.into_iter().flatten() {
        let tls = tls.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let tls = tls.clone();
                std::thread::spawn(move || {
                    let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
                    match tls {
                        None => handle(&mut { stream }),
                        Some(config) => {
                            let Ok(connection) = rustls::ServerConnection::new(config) else {
                                return;
                            };
                            handle(&mut rustls::StreamOwned::new(connection, stream));
                        }
                    }
                });
            }
        });
    }
    port
}

/// Answers a line with `pong:` and the line.
fn echo(link: &mut dyn Link) {
    let mut line = String::new();
    if BufReader::new(&mut *link).read_line(&mut line).is_err() {
        return;
    }
    let _ = link.write_all(format!("pong:{}\n", line.trim_end()).as_bytes());
    let _ = link.flush();
}

/// PostgreSQL's side of a connection over TLS, on a port of this machine
/// as [`serve`] gives one: `S` to the SSL request, then TLS with `tls`, the
/// startup answered as a server that asks for no password, and each simple
/// query answered as done, until the client says it is leaving.
fn serve_postgres(tls: Arc<rustls::ServerConfig>) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a port on this machine");
    let port = listener.local_addr().expect("its address").port();
    let listeners = [Some(listener), TcpListener::bind(("::1", port)).ok()];
    for listener in listeners.into_iter().flatten() {
        let tls = tls.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let tls = tls.clone();
                std::thread::spawn(move || {
                    let _ = postgres(stream, tls);
                });
            }
        });
    }
    port
}

/// One connection of [`serve_postgres`].
fn postgres(mut stream: TcpStream, tls: Arc<rustls::ServerConfig>) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    let mut request = [0u8; 8];
    stream.read_exact(&mut request)?;
    stream.write_all(b"S")?;
    let connection = rustls::ServerConnection::new(tls).map_err(std::io::Error::other)?;
    let mut link = rustls::StreamOwned::new(connection, stream);
    // The startup message: its length, then the rest.
    let mut length = [0u8; 4];
    link.read_exact(&mut length)?;
    let mut rest = vec![0u8; (u32::from_be_bytes(length) as usize).saturating_sub(4)];
    link.read_exact(&mut rest)?;
    // Signed in, and ready for a query.
    link.write_all(&[b'R', 0, 0, 0, 8, 0, 0, 0, 0, b'Z', 0, 0, 0, 5, b'I'])?;
    link.flush()?;
    loop {
        let mut head = [0u8; 5];
        link.read_exact(&mut head)?;
        let [kind, length @ ..] = head;
        let mut body = vec![0u8; (u32::from_be_bytes(length) as usize).saturating_sub(4)];
        link.read_exact(&mut body)?;
        if kind != b'Q' {
            return Ok(());
        }
        let tag = b"SELECT 1\0";
        let mut done = vec![b'C'];
        done.extend_from_slice(&(4 + tag.len() as u32).to_be_bytes());
        done.extend_from_slice(tag);
        done.extend_from_slice(&[b'Z', 0, 0, 0, 5, b'I']);
        link.write_all(&done)?;
        link.flush()?;
    }
}

/// A small HTTP/1.1 server's answers, by path, each closing its connection:
///
/// - `/answer`: `200` with `x-test: yes` and `{"ok":true}`;
/// - `/redirect`: `302` to `/answer`;
/// - `/hold`: `200` three seconds later, unless the client has gone;
/// - `/pieces`: `one`, then `two` 200 ms later, chunked;
/// - `/hang`: `first`, then nothing for five seconds, unless the client
///   has gone;
/// - `/echo`: the method, the content type and the body sent;
/// - `/big`: 4,096 bytes;
/// - `/gate/<ms>`: `200` `open` once `<ms>` milliseconds have passed,
///   unless the client has gone, counting the requests of its `tag`
///   waiting for their answer's head ([`gate`]; chunk 8.2j);
/// - anything else `404`.
fn answer(link: &mut dyn Link) {
    let mut reader = BufReader::new(&mut *link);
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() {
        return;
    }
    let began = Instant::now();
    let mut words = line.split_whitespace();
    let method = words.next().unwrap_or_default().to_string();
    let target = words.next().unwrap_or_default().to_string();
    let (path, tag) = target.split_once("?tag=").unwrap_or((&target, ""));
    let (path, tag) = (path.to_string(), tag.to_string());
    let (mut length, mut kind) = (0usize, String::new());
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).is_err() || header.trim().is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':') {
            match name.trim().to_ascii_lowercase().as_str() {
                "content-length" => length = value.trim().parse().unwrap_or(0),
                "content-type" => kind = value.trim().to_string(),
                _ => {}
            }
        }
    }
    let mut body = vec![0u8; length];
    if reader.read_exact(&mut body).is_err() {
        return;
    }
    drop(reader);
    let whole = |status: &str, headers: &str, body: &str| {
        format!(
            "HTTP/1.1 {status}\r\n{headers}content-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        )
    };
    let chunked = "HTTP/1.1 200 OK\r\ntransfer-encoding: chunked\r\nconnection: close\r\n\r\n";
    let piece = |text: &str| format!("{:x}\r\n{text}\r\n", text.len());
    let _ = match path.as_str() {
        "/answer" => link.write_all(
            whole(
                "200 OK",
                "x-test: yes\r\ncontent-type: application/json\r\n",
                "{\"ok\":true}",
            )
            .as_bytes(),
        ),
        "/redirect" => {
            link.write_all(whole("302 Found", "location: /answer\r\n", "moved").as_bytes())
        }
        "/hold" => {
            if watch_close(link, &tag, began, Duration::from_secs(3)) {
                return;
            }
            link.write_all(whole("200 OK", "", "late").as_bytes())
        }
        "/pieces" => {
            let _ = link.write_all(format!("{chunked}{}", piece("one")).as_bytes());
            let _ = link.flush();
            std::thread::sleep(Duration::from_millis(200));
            link.write_all(format!("{}0\r\n\r\n", piece("two")).as_bytes())
        }
        "/hang" => {
            let _ = link.write_all(format!("{chunked}{}", piece("first")).as_bytes());
            let _ = link.flush();
            if watch_close(link, &tag, began, Duration::from_secs(5)) {
                return;
            }
            link.write_all(b"0\r\n\r\n")
        }
        "/echo" => link.write_all(
            whole(
                "200 OK",
                "",
                &format!("{method} {kind} {}", String::from_utf8_lossy(&body)),
            )
            .as_bytes(),
        ),
        "/big" => link.write_all(whole("200 OK", "", &"x".repeat(4096)).as_bytes()),
        held if held.starts_with("/gate/") => {
            let ms = held.trim_start_matches("/gate/").parse().unwrap_or(0);
            gate_arrived(&tag, 1);
            let gone = watch_close(link, &tag, began, Duration::from_millis(ms));
            gate_arrived(&tag, -1);
            if gone {
                return;
            }
            link.write_all(whole("200 OK", "", "open").as_bytes())
        }
        _ => link.write_all(whole("404 Not Found", "", "none").as_bytes()),
    };
    let _ = link.flush();
}

/// `GET path` from workerd on `port`: its status and body, read as far as
/// the answer goes rather than until workerd closes the connection, which
/// it may do only once the invocation's other work has ended.
fn request(port: u16, path: &str) -> (u16, String) {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("workerd listens");
    let _ = stream.set_read_timeout(Some(Duration::from_secs(60)));
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: probe\r\nConnection: close\r\n\r\n"
    )
    .expect("the request is sent");
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
        .unwrap_or(0);
    let length = head.lines().find_map(|line| {
        line.strip_prefix("content-length:")
            .and_then(|length| length.trim().parse::<usize>().ok())
    });
    let mut body = Vec::new();
    match length {
        Some(length) => {
            body.resize(length, 0);
            let _ = stream.read_exact(&mut body);
        }
        None if head.contains("transfer-encoding: chunked") => {
            while !body.ends_with(b"0\r\n\r\n") && stream.read(&mut byte).unwrap_or(0) == 1 {
                body.push(byte[0]);
            }
            let text = dechunk(&String::from_utf8_lossy(&body));
            return (status, text);
        }
        None => {
            let _ = stream.read_to_end(&mut body);
        }
    }
    (status, String::from_utf8_lossy(&body).into_owned())
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
