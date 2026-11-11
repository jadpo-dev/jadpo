//! A receiver of published events for the `publishing` tests (area 12,
//! "Where events travel"): a localhost HTTP server on its own thread,
//! bound to a port the system chooses and kept for as long as it runs,
//! which keeps every request it is sent and answers each with the next
//! status it was given, then `204`. It never reaches a network.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// One request as it arrived.
#[derive(Clone, Debug)]
pub struct Received {
    pub method: String,
    pub path: String,
    /// Each header's name in lowercase, and its value.
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Received {
    /// The value of header `name`, given in lowercase.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(header, _)| header == name)
            .map(|(_, value)| value.as_str())
    }

    /// As the runtime's webhook check reads a request.
    pub fn as_request(&self) -> jadpo_runtime::http::Request {
        jadpo_runtime::http::Request {
            method: self.method.clone(),
            path: self.path.clone(),
            headers: self.headers.clone(),
            body: self.body.clone(),
            ..jadpo_runtime::http::Request::default()
        }
    }
}

#[derive(Default)]
struct State {
    answers: VecDeque<u16>,
    received: Vec<Received>,
}

pub struct FakeHooks {
    /// Where it listens, `http://127.0.0.1:<port>`.
    pub url: String,
    state: Arc<Mutex<State>>,
    stop: Arc<AtomicBool>,
    port: u16,
    thread: Option<JoinHandle<()>>,
}

impl FakeHooks {
    /// A receiver answering its first requests with `answers`, in order,
    /// and every one after with `204`.
    pub fn start(answers: &[u16]) -> FakeHooks {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a localhost port");
        let port = listener.local_addr().expect("address").port();
        let state = Arc::new(Mutex::new(State {
            answers: answers.iter().copied().collect(),
            received: Vec::new(),
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
        FakeHooks {
            url: format!("http://127.0.0.1:{port}"),
            state,
            stop,
            port,
            thread: Some(thread),
        }
    }

    /// Every request so far, in the order they came.
    pub fn received(&self) -> Vec<Received> {
        self.state.lock().expect("state").received.clone()
    }

    /// Every request, once at least `count` have come or `seconds` have
    /// passed.
    pub fn wait_for(&self, count: usize, seconds: u64) -> Vec<Received> {
        let until = Instant::now() + Duration::from_secs(seconds);
        loop {
            let received = self.received();
            if received.len() >= count || Instant::now() >= until {
                return received;
            }
            std::thread::park_timeout(Duration::from_millis(100));
        }
    }
}

impl Drop for FakeHooks {
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
    let mut headers: Vec<(String, String)> = Vec::new();
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
    let status = {
        let mut state = state.lock().expect("state");
        state.received.push(Received {
            method,
            path,
            headers,
            body,
        });
        state.answers.pop_front().unwrap_or(204)
    };
    let reason = match status {
        204 => "No Content",
        503 => "Service Unavailable",
        _ => "Answer",
    };
    let mut stream = stream;
    let _ = write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\ncontent-length: 0\r\nconnection: close\r\n\r\n"
    );
    let _ = stream.flush();
}
