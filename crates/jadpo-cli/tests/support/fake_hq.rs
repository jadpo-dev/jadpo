//! A fake of Jadpo HQ for `jadpo suggest share`'s tests (area 14, "The
//! protocol"; chunk 5.5y): a localhost HTTP server on its own thread that
//! speaks the protocol the real HQ will, counting distinct project tokens
//! for each hash, asking for a hash's text only once enough projects sent
//! it, recomputing each snippet's hash, and keeping every request it got
//! so a test can see exactly what was sent. It never reaches a network.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use serde_json::{Value, json};

/// One request as it arrived.
#[derive(Clone, Debug)]
pub struct Request {
    pub method: String,
    pub path: String,
    /// Header names in lower case.
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl Request {
    pub fn json(&self) -> Value {
        serde_json::from_str(&self.body).unwrap_or(Value::Null)
    }
}

/// A canned answer, served before the protocol's own.
type Canned = (u16, Vec<(String, String)>, String);

#[derive(Default)]
struct State {
    threshold: usize,
    /// For each hash, kind, signal and suggestion, the tokens it came from.
    seen: BTreeMap<(String, String, String, String), BTreeSet<String>>,
    /// The snippets kept, by hash and kind.
    texts: BTreeMap<(String, String), Vec<Value>>,
    requests: Vec<Request>,
    canned: VecDeque<Canned>,
}

impl State {
    /// How many distinct projects sent this hash with this kind.
    fn projects(&self, hash: &str, kind: &str) -> usize {
        self.seen
            .iter()
            .filter(|((h, k, _, _), _)| h == hash && k == kind)
            .flat_map(|(_, tokens)| tokens.iter())
            .collect::<BTreeSet<_>>()
            .len()
    }

    /// Whether HQ wants this hash's text with this kind: enough projects
    /// sent it, and it doesn't hold it yet (an ignored one: a few reasons).
    fn wants(&self, hash: &str, kind: &str) -> bool {
        let held = self
            .texts
            .get(&(hash.to_string(), kind.to_string()))
            .map_or(0, Vec::len);
        let enough = if kind == "ignored" { 3 } else { 1 };
        self.projects(hash, kind) >= self.threshold && held < enough
    }
}

pub struct FakeHq {
    pub url: String,
    state: Arc<Mutex<State>>,
    stop: Arc<AtomicBool>,
    port: u16,
    thread: Option<JoinHandle<()>>,
}

fn is_hex(text: &str, digits: usize) -> bool {
    text.len() == digits
        && text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn error(status: u16, code: &str, message: &str) -> (u16, Vec<(String, String)>, String) {
    (
        status,
        Vec::new(),
        json!({ "error": { "code": code, "message": message } }).to_string(),
    )
}

impl FakeHq {
    /// A fake that wants a hash's text once `threshold` distinct projects
    /// have sent it.
    pub fn start(threshold: usize) -> FakeHq {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a localhost port");
        let port = listener.local_addr().expect("address").port();
        let state = Arc::new(Mutex::new(State {
            threshold,
            ..State::default()
        }));
        let stop = Arc::new(AtomicBool::new(false));
        let thread = {
            let state = Arc::clone(&state);
            let stop = Arc::clone(&stop);
            std::thread::spawn(move || {
                for stream in listener.incoming() {
                    if stop.load(Ordering::SeqCst) {
                        break;
                    }
                    if let Ok(stream) = stream {
                        serve(stream, &state);
                    }
                }
            })
        };
        FakeHq {
            url: format!("http://127.0.0.1:{port}"),
            state,
            stop,
            port,
            thread: Some(thread),
        }
    }

    /// Every request so far.
    pub fn requests(&self) -> Vec<Request> {
        self.state.lock().expect("state").requests.clone()
    }

    /// The snippets HQ kept.
    pub fn snippets(&self) -> Vec<Value> {
        self.state
            .lock()
            .expect("state")
            .texts
            .values()
            .flatten()
            .cloned()
            .collect()
    }

    /// Pretend other projects already sent this hash.
    pub fn seed(&self, hash: &str, kind: &str, signal: &str, suggestion: &str, tokens: &[&str]) {
        let mut state = self.state.lock().expect("state");
        let entry = state
            .seen
            .entry((
                hash.to_string(),
                kind.to_string(),
                signal.to_string(),
                suggestion.to_string(),
            ))
            .or_default();
        entry.extend(tokens.iter().map(|t| t.to_string()));
    }

    /// How many distinct projects sent this hash with this kind.
    pub fn projects(&self, hash: &str, kind: &str) -> usize {
        self.state.lock().expect("state").projects(hash, kind)
    }

    /// Answer the next request with this instead of the protocol.
    pub fn answer_next(&self, status: u16, headers: &[(&str, &str)], body: &str) {
        self.state.lock().expect("state").canned.push_back((
            status,
            headers
                .iter()
                .map(|(n, v)| (n.to_string(), v.to_string()))
                .collect(),
            body.to_string(),
        ));
    }
}

impl Drop for FakeHq {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(("127.0.0.1", self.port));
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn serve(stream: TcpStream, state: &Mutex<State>) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let mut reader = BufReader::new(match stream.try_clone() {
        Ok(clone) => clone,
        Err(_) => return,
    });
    let mut line = String::new();
    if reader.read_line(&mut line).unwrap_or(0) == 0 {
        return;
    }
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let path = parts.next().unwrap_or_default().to_string();
    let mut headers = Vec::new();
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).unwrap_or(0) == 0 {
            break;
        }
        let header = header.trim_end();
        if header.is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':') {
            headers.push((name.trim().to_lowercase(), value.trim().to_string()));
        }
    }
    let length = headers
        .iter()
        .find(|(name, _)| name == "content-length")
        .and_then(|(_, value)| value.parse::<usize>().ok())
        .unwrap_or(0);
    let mut body = vec![0; length];
    if reader.read_exact(&mut body).is_err() {
        return;
    }
    let request = Request {
        method,
        path,
        headers,
        body: String::from_utf8_lossy(&body).into_owned(),
    };
    let (status, extra, reply) = {
        let mut state = state.lock().expect("state");
        state.requests.push(request.clone());
        match state.canned.pop_front() {
            Some(canned) => canned,
            None => answer(&mut state, &request),
        }
    };
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        410 => "Gone",
        429 => "Too Many Requests",
        503 => "Service Unavailable",
        _ => "Status",
    };
    let mut response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n",
        reply.len()
    );
    for (name, value) in extra {
        response.push_str(&format!("{name}: {value}\r\n"));
    }
    response.push_str("\r\n");
    response.push_str(&reply);
    let mut stream = stream;
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

fn answer(state: &mut State, request: &Request) -> Canned {
    if request.method != "POST" {
        return error(405, "invalid_request", "Only POST is answered");
    }
    let Ok(Value::Object(body)) = serde_json::from_str::<Value>(&request.body) else {
        return error(400, "invalid_request", "The body isn't a JSON object");
    };
    match request.path.as_str() {
        "/v1/suggestions/hashes" => {
            let Some(token) = body.get("token").and_then(Value::as_str) else {
                return error(400, "invalid_request", "token is missing");
            };
            if !is_hex(token, 32) {
                return error(400, "invalid_request", "token isn't a project token");
            }
            let Some(entries) = body.get("entries").and_then(Value::as_array) else {
                return error(400, "invalid_request", "entries is missing");
            };
            if entries.len() > 500 {
                return error(400, "invalid_request", "entries has more than 500");
            }
            let mut asked = Vec::new();
            for entry in entries {
                let field = |name: &str| entry.get(name).and_then(Value::as_str).unwrap_or("");
                let (hash, kind) = (field("hash"), field("kind"));
                if !is_hex(hash, 16)
                    || !matches!(kind, "found" | "accepted" | "ignored")
                    || field("signal").is_empty()
                    || field("suggestion").is_empty()
                    || entry.as_object().map_or(0, |o| o.len()) != 4
                {
                    return error(
                        400,
                        "invalid_request",
                        "an entry isn't a hash, kind, signal and suggestion",
                    );
                }
                state
                    .seen
                    .entry((
                        hash.to_string(),
                        kind.to_string(),
                        field("signal").to_string(),
                        field("suggestion").to_string(),
                    ))
                    .or_default()
                    .insert(token.to_string());
                if !asked.contains(&(hash.to_string(), kind.to_string())) {
                    asked.push((hash.to_string(), kind.to_string()));
                }
            }
            let wanted: Vec<Value> = asked
                .iter()
                .filter(|(hash, kind)| state.wants(hash, kind))
                .take(20)
                .map(|(hash, kind)| json!({ "hash": hash, "kind": kind }))
                .collect();
            (200, Vec::new(), json!({ "wanted": wanted }).to_string())
        }
        "/v1/suggestions/snippets" => {
            if body.contains_key("token") {
                return error(
                    400,
                    "invalid_request",
                    "a snippet is never tied to a project",
                );
            }
            let Some(snippets) = body.get("snippets").and_then(Value::as_array) else {
                return error(400, "invalid_request", "snippets is missing");
            };
            let (mut received, mut refused) = (0, 0);
            for snippet in snippets {
                let field = |name: &str| snippet.get(name).and_then(Value::as_str).unwrap_or("");
                let kind = field("kind");
                if snippet.get("reason").is_some() && kind != "ignored" {
                    return error(
                        400,
                        "invalid_request",
                        "only an ignored snippet has a reason",
                    );
                }
                let hash = jadpo_check::suggest::hash(field("text"));
                if state.wants(&hash, kind) {
                    state
                        .texts
                        .entry((hash, kind.to_string()))
                        .or_default()
                        .push(snippet.clone());
                    received += 1;
                } else {
                    refused += 1;
                }
            }
            (
                200,
                Vec::new(),
                json!({ "received": received, "refused": refused }).to_string(),
            )
        }
        _ => error(404, "invalid_request", "No such path"),
    }
}
