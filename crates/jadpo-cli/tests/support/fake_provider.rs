//! A provider's API for the package driver's tests (area 11, "Calling";
//! chunk 7.2c): a localhost HTTP server on its own thread that answers
//! each request with the next answer scripted for it, `200 {}` when none
//! is, after the wait scripted with it (chunk 7.2c2), and keeps every
//! request it got, so a test sees exactly what was sent. It never reaches
//! a network.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

/// One request as it arrived.
#[derive(Clone, Debug)]
pub struct Request {
    pub method: String,
    /// Its path and query.
    pub path: String,
    /// Header names in lower case.
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl Request {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(header, _)| header == name)
            .map(|(_, value)| value.as_str())
    }

    pub fn json(&self) -> serde_json::Value {
        serde_json::from_str(&self.body).unwrap_or(serde_json::Value::Null)
    }
}

/// What a test runs while a request is held, before it is answered.
type Hold = Arc<dyn Fn(&Request) + Send + Sync>;

#[derive(Default)]
struct State {
    requests: Vec<Request>,
    /// Each answer's status, body and how long it waits first, in
    /// milliseconds.
    answers: VecDeque<(u16, String, u64)>,
    hold: Option<Hold>,
}

pub struct FakeProvider {
    /// Where it listens: `http://127.0.0.1:<port>`.
    pub url: String,
    state: Arc<Mutex<State>>,
    stop: Arc<AtomicBool>,
    port: u16,
    thread: Option<JoinHandle<()>>,
}

impl FakeProvider {
    pub fn start() -> FakeProvider {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a localhost port");
        let port = listener.local_addr().expect("address").port();
        let state = Arc::new(Mutex::new(State::default()));
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
        FakeProvider {
            url: format!("http://127.0.0.1:{port}"),
            state,
            stop,
            port,
            thread: Some(thread),
        }
    }

    /// Answers the next request with `status` and `body`, after the
    /// answers scripted already.
    pub fn answer(&self, status: u16, body: &str) {
        self.answer_after(0, status, body);
    }

    /// The same, answered only after `millis` milliseconds: longer than a
    /// call's timeout, it is never answered in time.
    pub fn answer_after(&self, millis: u64, status: u16, body: &str) {
        self.state
            .lock()
            .expect("state")
            .answers
            .push_back((status, body.to_string(), millis));
    }

    /// Runs `hold` with each request as it arrives, on the server's
    /// thread, and answers it only once `hold` returns: what a test does
    /// while a call is out (chunk 7.4).
    pub fn hold(&self, hold: impl Fn(&Request) + Send + Sync + 'static) {
        self.state.lock().expect("state").hold = Some(Arc::new(hold));
    }

    /// Every request so far.
    pub fn requests(&self) -> Vec<Request> {
        self.state.lock().expect("state").requests.clone()
    }
}

impl Drop for FakeProvider {
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
    let hold = {
        let mut state = state.lock().expect("state");
        state.requests.push(request.clone());
        state.hold.clone()
    };
    if let Some(hold) = hold {
        hold(&request);
    }
    let (status, reply, wait) = state
        .lock()
        .expect("state")
        .answers
        .pop_front()
        .unwrap_or_else(|| (200, "{}".to_string(), 0));
    if wait > 0 {
        std::thread::sleep(Duration::from_millis(wait));
    }
    let response = format!(
        "HTTP/1.1 {status} Status\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",
        reply.len()
    );
    let mut stream = stream;
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}
