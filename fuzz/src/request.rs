//! The request edge (area 7, "What is built"; docs/compiler-runtime.md
//! §2.6, "Deadlines and request limits"): a request as the native host
//! hands it to the runtime, answered by teaching examples served by the
//! interpreter. Whatever its method, path, query string, headers and body,
//! the runtime answers it, never a panic; and the input reader of the
//! route its path finds reads it, as it would for a signed-in caller.
//!
//! An input's first byte picks the application ([`APPS`]); the rest is the
//! request as text: `METHOD TARGET`, then `name: value` lines, a blank
//! line, and the body. Only what hyper could hand over reaches the
//! runtime: the method, target, names and values are read with the `http`
//! crate's own rules, a value's bytes made text as the host makes them,
//! and a body over the limit is cut off as the host cuts it.
//!
//! The host has no store, so whatever a route runs that reads or writes
//! one ends in a fault, which is an answer like any other; its clock is
//! fixed and its randomness repeats, so a crash repeats. The interpreter
//! runs requests on a thread with the runtime's stack, as a deployable's
//! workers do.

use std::cell::Cell;
use std::sync::mpsc;
use std::sync::{Mutex, OnceLock};

use jadpo_interp::Program;
use jadpo_runtime::http::{App, BODY_LIMIT, Found, Request, Worker, read_request, serve_request};
use jadpo_runtime::{Database, Host, NoStore, block_on};

/// The teaching examples requests are sent to: every one with routes or
/// webhooks.
pub const APPS: &[&str] = &[
    "11-cake-shop",
    "12-recipe-book",
    "13-lettings-office",
    "21-veg-box-rounds",
    "23-repair-cafe",
    "26-delivery-desk",
    "27-guest-house",
    "28-lost-property",
];

/// The connection's address: the client's own, which it doesn't choose.
const PEER: &str = "203.0.113.7";

/// 2026-09-21T14:13:20Z.
const NOW_MS: i64 = 1_790_000_000_000;

pub fn run(data: &[u8]) {
    let Some((&app, rest)) = data.split_first() else {
        return;
    };
    let Some(request) = request(rest) else {
        return;
    };
    let app = usize::from(app) % APPS.len();
    served().send((app, request));
}

/// A request read from `text` as the host would hand it over, or `None`
/// when hyper wouldn't.
pub fn request(text: &[u8]) -> Option<Request> {
    let (head, body) = match find(text, b"\n\n")
        .map(|at| (at, 2))
        .or_else(|| find(text, b"\r\n\r\n").map(|at| (at, 4)))
    {
        Some((at, skip)) => (&text[..at], &text[at + skip..]),
        None => (text, &[][..]),
    };
    let mut lines = head
        .split(|&byte| byte == b'\n')
        .map(|line| line.strip_suffix(b"\r").unwrap_or(line));
    let first = lines.next()?;
    let space = first.iter().position(|&byte| byte == b' ')?;
    let method = http::Method::from_bytes(&first[..space]).ok()?;
    let uri = http::Uri::try_from(&first[space + 1..]).ok()?;
    let mut headers = Vec::new();
    for line in lines {
        let colon = line.iter().position(|&byte| byte == b':')?;
        let name = http::HeaderName::from_bytes(&line[..colon]).ok()?;
        let value = trim(&line[colon + 1..]);
        let value = http::HeaderValue::from_bytes(value).ok()?;
        headers.push((
            name.as_str().to_string(),
            String::from_utf8_lossy(value.as_bytes()).into_owned(),
        ));
    }
    let declared = headers
        .iter()
        .find(|(name, _)| name == "content-length")
        .and_then(|(_, value)| value.parse::<usize>().ok());
    let too_large = body.len() > BODY_LIMIT || declared.is_some_and(|length| length > BODY_LIMIT);
    Some(Request {
        method: method.as_str().to_string(),
        path: uri.path().to_string(),
        query: uri.query().map(str::to_string),
        headers,
        body: if too_large { Vec::new() } else { body.to_vec() },
        address: Some(PEER.to_string()),
        too_large,
    })
}

fn find(text: &[u8], needle: &[u8]) -> Option<usize> {
    text.windows(needle.len())
        .position(|window| window == needle)
}

fn trim(value: &[u8]) -> &[u8] {
    let start = value
        .iter()
        .position(|byte| !matches!(byte, b' ' | b'\t'))
        .unwrap_or(value.len());
    let end = value
        .iter()
        .rposition(|byte| !matches!(byte, b' ' | b'\t'))
        .map_or(start, |end| end + 1);
    &value[start..end]
}

/// The thread that serves: requests go in, and it answers when it has
/// answered each. A panic there aborts the process, as libFuzzer needs.
struct Served {
    to: mpsc::Sender<(usize, Request)>,
    done: mpsc::Receiver<()>,
}

impl Served {
    fn send(&self, request: (usize, Request)) {
        self.to
            .send(request)
            .expect("the serving thread is running");
        self.done.recv().expect("the serving thread answered");
    }
}

fn served() -> std::sync::MutexGuard<'static, Served> {
    static SERVED: OnceLock<Mutex<Served>> = OnceLock::new();
    SERVED
        .get_or_init(|| {
            let (to, from) = mpsc::channel::<(usize, Request)>();
            let (answered, done) = mpsc::channel();
            std::thread::Builder::new()
                .name("jadpo-fuzz-request".into())
                .stack_size(jadpo_runtime::STACK_BYTES)
                .spawn(move || {
                    let programs = programs();
                    while let Ok((app, request)) = from.recv() {
                        answer(&programs[app], &request);
                        if answered.send(()).is_err() {
                            break;
                        }
                    }
                })
                .expect("a thread to serve on");
            Mutex::new(Served { to, done })
        })
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Every app in [`APPS`], loaded and prepared once.
pub fn programs() -> Vec<Program> {
    APPS.iter()
        .map(|name| {
            let loaded = jadpo_db::load(&crate::root().join("examples").join(name));
            Program::load(&loaded.db, loaded.project)
                .unwrap_or_else(|errors| panic!("example {name} doesn't check: {errors:?}"))
                .with_proof_checks()
        })
        .collect()
}

/// Answers `request` as a deployable does, then reads it with the input
/// reader of the route its path finds.
pub fn answer(program: &Program, request: &Request) {
    let host = FuzzHost::default();
    let worker = Worker::default();
    let app = program.app();
    let _ = block_on(serve_request(&app, program, &host, &worker, request));
    read(&app, request);
}

/// The route `request`'s path and method find, read as it would be once
/// its caller signed in.
fn read(app: &App<'_>, request: &Request) {
    if let Found::Route {
        route: Some(number),
        ..
    } = app.routes.find(&request.method, &request.path)
    {
        let _ = read_request(app, number, request);
    }
}

/// A host with no store, a fixed clock and randomness that repeats.
#[derive(Default)]
struct FuzzHost {
    store: NoStore,
    drawn: Cell<u64>,
}

impl Host for FuzzHost {
    fn database(&self, _store: &str) -> Option<&dyn Database> {
        Some(&self.store)
    }

    fn random(&self, bytes: &mut [u8]) -> bool {
        for byte in bytes {
            let mut z = self.drawn.get().wrapping_add(0x9E37_79B9_7F4A_7C15);
            self.drawn.set(z);
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            *byte = (z ^ (z >> 31)) as u8;
        }
        true
    }

    fn now_ms(&self) -> Option<i64> {
        Some(NOW_MS)
    }

    fn monotonic_ms(&self) -> Option<u64> {
        Some(0)
    }
}

/// Seed requests for app `app`: each route's, its placeholders and input
/// made from their types as a fake's answer is, then health's, `OPTIONS`
/// and `HEAD`.
pub fn seeds(app: u8, program: &Program) -> Vec<Vec<u8>> {
    let app_plan = program.app();
    let mut uuids = 0u64;
    let mut uuid = || {
        uuids += 1;
        format!("00000000-0000-4000-8000-{uuids:012x}")
    };
    let mut making = jadpo_runtime::fakes::Making {
        now: NOW_MS,
        uuid: &mut uuid,
    };
    let mut out = Vec::new();
    for route in &app_plan.table.routes {
        let mut path = String::new();
        for segment in &route.segments {
            path.push('/');
            match segment {
                jadpo_std::http::Segment::Fixed(text) => path.push_str(text),
                jadpo_std::http::Segment::Param { codec, .. } => {
                    let value =
                        jadpo_runtime::fakes::generated(&app_plan.codecs, *codec, &mut making);
                    path.push_str(&encode(&plain(&value)));
                }
            }
        }
        if path.is_empty() {
            path.push('/');
        }
        let method = format!("{:?}", route.method).to_ascii_uppercase();
        let mut headers = vec!["host: app.example".to_string()];
        let mut body = String::new();
        if let Some(input) = &route.input {
            let value = jadpo_runtime::fakes::generated(&app_plan.codecs, input.codec, &mut making);
            if input.query {
                if let serde_json::Value::Object(members) = &value {
                    let query: Vec<String> = members
                        .iter()
                        .filter(|(_, value)| !value.is_null())
                        .map(|(key, value)| format!("{}={}", encode(key), encode(&plain(value))))
                        .collect();
                    if !query.is_empty() {
                        path = format!("{path}?{}", query.join("&"));
                    }
                }
            } else {
                headers.push("content-type: application/json".to_string());
                body = value.to_string();
            }
            for (_, header) in &input.headers {
                headers.push(format!("{header}: x"));
            }
        }
        if method != "GET" {
            headers.push("idempotency-key: 0123456789abcdef".to_string());
        }
        out.push(seed(app, &method, &path, &headers, &body));
    }
    for (method, path) in [
        ("GET", "/health/live"),
        ("GET", "/health/ready"),
        ("OPTIONS", "/"),
        ("HEAD", "/health/live"),
    ] {
        out.push(seed(
            app,
            method,
            path,
            &["host: app.example".to_string()],
            "",
        ));
    }
    out
}

fn seed(app: u8, method: &str, target: &str, headers: &[String], body: &str) -> Vec<u8> {
    let mut out = vec![app];
    out.extend_from_slice(format!("{method} {target}\n").as_bytes());
    for header in headers {
        out.extend_from_slice(header.as_bytes());
        out.push(b'\n');
    }
    out.push(b'\n');
    out.extend_from_slice(body.as_bytes());
    out
}

/// A value as a placeholder or a query string writes it.
fn plain(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(text) => text.clone(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn encode(text: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}
