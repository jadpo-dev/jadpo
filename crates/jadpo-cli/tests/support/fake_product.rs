//! A monitoring product's side of the manifest protocol, for `jadpo
//! manifest send` and `jadpo manifest protocol` (area 16, "The manifest
//! protocol"; chunk O3): a localhost HTTP server on its own thread that
//! keeps each build's manifest as it was first sent, reads the credential
//! header it is given, and records every request so a test sees exactly
//! what was sent. Its faults are switches, so the conformance test can be
//! shown each way a product can differ. It never reaches a network.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use jadpo_cli::manifest_protocol;
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

    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }
}

/// The ways a product can differ from the protocol, each off by default.
#[derive(Clone, Copy, Debug, Default)]
pub struct Faults {
    /// A build sent again replaces the one held.
    pub replaces: bool,
    /// Answers without reading the credential.
    pub open: bool,
    /// Keeps a manifest whose `build` isn't the path's.
    pub lenient: bool,
}

#[derive(Default)]
struct State {
    held: BTreeMap<String, String>,
    requests: Vec<Request>,
}

pub struct FakeProduct {
    /// The telemetry endpoint the product serves the protocol below.
    pub url: String,
    state: Arc<Mutex<State>>,
    stop: Arc<AtomicBool>,
    port: u16,
    thread: Option<JoinHandle<()>>,
}

impl FakeProduct {
    /// A product that conforms, reading `credential` (a header's name and
    /// value) when one is given.
    pub fn start(credential: Option<(&str, &str)>) -> FakeProduct {
        FakeProduct::with_faults(credential, Faults::default())
    }

    pub fn with_faults(credential: Option<(&str, &str)>, faults: Faults) -> FakeProduct {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
        let port = listener.local_addr().expect("an address").port();
        let state = Arc::new(Mutex::new(State::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let credential = credential.map(|(name, value)| (name.to_lowercase(), value.to_string()));
        let (kept, stopping) = (Arc::clone(&state), Arc::clone(&stop));
        let thread = std::thread::spawn(move || {
            for stream in listener.incoming() {
                if stopping.load(Ordering::SeqCst) {
                    break;
                }
                let Ok(stream) = stream else { continue };
                let (kept, credential) = (Arc::clone(&kept), credential.clone());
                std::thread::spawn(move || serve(stream, &kept, credential.as_ref(), faults));
            }
        });
        FakeProduct {
            url: format!("http://127.0.0.1:{port}"),
            state,
            stop,
            port,
            thread: Some(thread),
        }
    }

    pub fn requests(&self) -> Vec<Request> {
        self.state.lock().expect("state").requests.clone()
    }

    /// The manifest held for `build`, as it was sent.
    pub fn held(&self, build: &str) -> Option<Value> {
        let state = self.state.lock().expect("state");
        state
            .held
            .get(build)
            .and_then(|text| serde_json::from_str(text).ok())
    }
}

impl Drop for FakeProduct {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(("127.0.0.1", self.port));
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn serve(
    stream: TcpStream,
    state: &Mutex<State>,
    credential: Option<&(String, String)>,
    faults: Faults,
) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
    let Some(request) = read(&stream) else { return };
    state.lock().expect("state").requests.push(request.clone());
    let (status, body) = answer(&request, state, credential, faults);
    let text = body.to_string();
    let mut stream = stream;
    let _ = write!(
        stream,
        "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
        text.len()
    );
}

fn refused(status: u16, code: &str, message: &str) -> (u16, Value) {
    (
        status,
        json!({ "error": { "code": code, "message": message } }),
    )
}

fn answer(
    request: &Request,
    state: &Mutex<State>,
    credential: Option<&(String, String)>,
    faults: Faults,
) -> (u16, Value) {
    if let Some((name, value)) = credential
        && !faults.open
        && request.header(name) != Some(value.as_str())
    {
        return refused(401, "unauthorised", "The credential is missing or wrong.");
    }
    let Some(build) = request.path.strip_prefix("/jadpo/v1/builds/") else {
        return refused(404, "not_found", "Nothing is served here.");
    };
    let build = build.to_string();
    match request.method.as_str() {
        "PUT" => {
            if request.body.len() > manifest_protocol::MOST {
                return refused(413, "too_large", "The manifest is over 16 MiB.");
            }
            let body = request.json();
            // A lenient product checks the manifest against the build it
            // names, not the path's.
            let against = match body["build"].as_str() {
                Some(named) if faults.lenient => named.to_string(),
                _ => build.clone(),
            };
            if let Err(why) = manifest_protocol::check_manifest(&body, &against) {
                return refused(422, "manifest_invalid", &why);
            }
            let mut state = state.lock().expect("state");
            let held = state.held.contains_key(&build);
            if !held || faults.replaces {
                state.held.insert(build.clone(), request.body.clone());
            }
            (if held { 200 } else { 201 }, json!({ "build": build }))
        }
        "GET" => match state.lock().expect("state").held.get(&build) {
            Some(text) => (200, serde_json::from_str(text).unwrap_or(Value::Null)),
            None => refused(
                404,
                "build_not_found",
                "No manifest is held for that build.",
            ),
        },
        _ => refused(405, "method_not_allowed", "Only PUT and GET are served."),
    }
}

fn read(stream: &TcpStream) -> Option<Request> {
    let mut reader = BufReader::new(stream.try_clone().ok()?);
    let mut line = String::new();
    reader.read_line(&mut line).ok()?;
    let mut parts = line.split_whitespace();
    let method = parts.next()?.to_string();
    let path = parts.next()?.to_string();
    let mut headers = Vec::new();
    let mut length = 0;
    loop {
        let mut header = String::new();
        reader.read_line(&mut header).ok()?;
        let header = header.trim_end();
        if header.is_empty() {
            break;
        }
        let (name, value) = header.split_once(':')?;
        let (name, value) = (name.trim().to_lowercase(), value.trim().to_string());
        if name == "content-length" {
            length = value.parse().ok()?;
        }
        headers.push((name, value));
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body).ok()?;
    Some(Request {
        method,
        path,
        headers,
        body: String::from_utf8(body).ok()?,
    })
}
