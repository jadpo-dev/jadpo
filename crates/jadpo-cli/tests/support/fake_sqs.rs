//! A local Amazon SQS for the `receiving` tests (chunk 7.7c): a localhost
//! HTTP server on its own thread that speaks SQS's JSON protocol for the
//! two operations the SQS package maps, `ReceiveMessage` and
//! `DeleteMessage`, and refuses any request whose Signature Version 4
//! doesn't verify against its one credential, as SQS does. It never reaches
//! a network.
//!
//! A received message is in flight, hidden from the next receive, until it
//! is deleted.

use std::collections::{BTreeMap, VecDeque};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

/// One request as it arrived.
#[derive(Clone, Debug)]
pub struct Seen {
    /// Its `X-Amz-Target`, `AmazonSQS.ReceiveMessage`.
    pub target: String,
    pub content_type: String,
    /// Whether its signature verified.
    pub signed: bool,
    /// Its `Authorization` header, which carries the key id, never the
    /// secret.
    pub authorization: String,
    pub body: serde_json::Value,
}

#[derive(Clone, Debug)]
struct Message {
    id: String,
    body: String,
    /// The handle it was last received with, while it is in flight.
    handle: Option<String>,
}

#[derive(Default)]
struct State {
    queues: BTreeMap<String, VecDeque<Message>>,
    seen: Vec<Seen>,
    handles: u64,
    ids: u64,
}

pub struct FakeSqs {
    /// Its endpoint, `http://127.0.0.1:<port>`.
    pub url: String,
    state: Arc<Mutex<State>>,
    stop: Arc<AtomicBool>,
    port: u16,
    thread: Option<JoinHandle<()>>,
}

impl FakeSqs {
    /// A server holding one empty queue, `<url>/000000000000/<queue>`,
    /// that takes requests signed by `key_id` and `secret`.
    pub fn start(queue: &str, key_id: &str, secret: &str) -> FakeSqs {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a localhost port");
        let port = listener.local_addr().expect("address").port();
        let url = format!("http://127.0.0.1:{port}");
        let state = Arc::new(Mutex::new(State::default()));
        state
            .lock()
            .expect("state")
            .queues
            .insert(format!("{url}/000000000000/{queue}"), VecDeque::new());
        let stop = Arc::new(AtomicBool::new(false));
        let thread = {
            let state = Arc::clone(&state);
            let stop = Arc::clone(&stop);
            let (key_id, secret) = (key_id.to_string(), secret.to_string());
            std::thread::spawn(move || {
                for stream in listener.incoming() {
                    if stop.load(Ordering::SeqCst) {
                        break;
                    }
                    if let Ok(stream) = stream {
                        serve(stream, &state, &key_id, &secret);
                    }
                }
            })
        };
        FakeSqs {
            url,
            state,
            stop,
            port,
            thread: Some(thread),
        }
    }

    /// The address of queue `queue`.
    pub fn queue_url(&self, queue: &str) -> String {
        format!("{}/000000000000/{queue}", self.url)
    }

    /// Sends `body` to queue `queue`.
    pub fn send(&self, queue: &str, body: &str) {
        let mut state = self.state.lock().expect("state");
        state.ids += 1;
        let id = format!("sqs-{}", state.ids);
        state
            .queues
            .get_mut(&self.queue_url(queue))
            .expect("a queue")
            .push_back(Message {
                id,
                body: body.to_string(),
                handle: None,
            });
    }

    /// The messages still on queue `queue`, in flight or not, by body.
    pub fn left(&self, queue: &str) -> Vec<String> {
        self.state.lock().expect("state").queues[&self.queue_url(queue)]
            .iter()
            .map(|message| message.body.clone())
            .collect()
    }

    /// Every request so far.
    pub fn seen(&self) -> Vec<Seen> {
        self.state.lock().expect("state").seen.clone()
    }
}

impl Drop for FakeSqs {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(("127.0.0.1", self.port));
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn serve(stream: TcpStream, state: &Mutex<State>, key_id: &str, secret: &str) {
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
    let header = |name: &str| {
        headers
            .iter()
            .find(|(header, _)| header == name)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let length = header("content-length").parse::<usize>().unwrap_or(0);
    let mut body = vec![0; length];
    if reader.read_exact(&mut body).is_err() {
        return;
    }
    let signed = verified(&method, &path, &headers, &body, key_id, secret);
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null);
    let target = header("x-amz-target");
    let (status, reply) = {
        let mut state = state.lock().expect("state");
        state.seen.push(Seen {
            target: target.clone(),
            content_type: header("content-type"),
            signed,
            authorization: header("authorization"),
            body: json.clone(),
        });
        if !signed {
            (
                403,
                r#"{"__type":"com.amazon.coral.service#SignatureDoesNotMatch","message":"The request signature we calculated does not match the signature you provided."}"#.to_string(),
            )
        } else {
            answer(&mut state, &target, &json)
        }
    };
    let response = format!(
        "HTTP/1.1 {status} Status\r\nContent-Type: application/x-amz-json-1.0\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",
        reply.len()
    );
    let mut stream = stream;
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

fn answer(state: &mut State, target: &str, json: &serde_json::Value) -> (u16, String) {
    let missing = (
        400,
        r#"{"__type":"com.amazonaws.sqs#QueueDoesNotExist","message":"The specified queue does not exist."}"#
            .to_string(),
    );
    let queue_url = json["QueueUrl"].as_str().unwrap_or_default().to_string();
    match target {
        "AmazonSQS.ReceiveMessage" => {
            let most = json["MaxNumberOfMessages"].as_u64().unwrap_or(1) as usize;
            state.handles += 1;
            let round = state.handles;
            let Some(queue) = state.queues.get_mut(&queue_url) else {
                return missing;
            };
            let mut messages = Vec::new();
            for (at, message) in queue
                .iter_mut()
                .filter(|message| message.handle.is_none())
                .take(most)
                .enumerate()
            {
                let handle = format!("handle-{round}-{at}");
                message.handle = Some(handle.clone());
                messages.push(serde_json::json!({
                    "MessageId": message.id,
                    "ReceiptHandle": handle,
                    "MD5OfBody": "not checked",
                    "Body": message.body,
                }));
            }
            if messages.is_empty() {
                (200, "{}".to_string())
            } else {
                (200, serde_json::json!({ "Messages": messages }).to_string())
            }
        }
        "AmazonSQS.DeleteMessage" => {
            let handle = json["ReceiptHandle"].as_str().unwrap_or_default();
            let Some(queue) = state.queues.get_mut(&queue_url) else {
                return missing;
            };
            match queue
                .iter()
                .position(|message| message.handle.as_deref() == Some(handle))
            {
                Some(at) => {
                    queue.remove(at);
                    (200, "{}".to_string())
                }
                None => (
                    400,
                    r#"{"__type":"com.amazonaws.sqs#ReceiptHandleIsInvalid","message":"The input receipt handle is invalid."}"#
                        .to_string(),
                ),
            }
        }
        _ => (
            400,
            r#"{"__type":"com.amazonaws.sqs#InvalidAction","message":"Unknown action."}"#
                .to_string(),
        ),
    }
}

/// Whether the request's `Authorization` is the Signature Version 4 of
/// its method, path, signed headers and body under the one credential,
/// signed for us-east-1's `sqs` at its `X-Amz-Date`.
fn verified(
    method: &str,
    path: &str,
    headers: &[(String, String)],
    body: &[u8],
    key_id: &str,
    secret: &str,
) -> bool {
    let header = |name: &str| {
        headers
            .iter()
            .find(|(header, _)| header == name)
            .map(|(_, value)| value.as_str())
    };
    let (Some(authorization), Some(date), Some(host)) = (
        header("authorization"),
        header("x-amz-date"),
        header("host"),
    ) else {
        return false;
    };
    let Some(signed) = authorization
        .split(", ")
        .find_map(|part| part.strip_prefix("SignedHeaders="))
    else {
        return false;
    };
    let Some(at) = date_ms(date) else {
        return false;
    };
    let given: Vec<(String, String)> = signed
        .split(';')
        .filter(|name| *name != "host" && *name != "x-amz-date")
        .filter_map(|name| Some((name.to_string(), header(name)?.to_string())))
        .collect();
    let request = jadpo_runtime::sigv4::Unsigned {
        method,
        url: &format!("http://{host}{path}"),
        headers: &given,
        body,
    };
    let signer = jadpo_runtime::sigv4::Signer {
        key_id,
        secret,
        region: "us-east-1",
        service: "sqs",
    };
    jadpo_runtime::sigv4::sign(&request, &signer, at)
        .is_some_and(|[_, (_, expected)]| expected == authorization)
}

/// `YYYYMMDDTHHMMSSZ` in milliseconds since 1970.
fn date_ms(stamp: &str) -> Option<i64> {
    let digits = |from: usize, to: usize| stamp.get(from..to)?.parse::<i64>().ok();
    let (year, month, day) = (digits(0, 4)?, digits(4, 6)?, digits(6, 8)?);
    let (hour, minute, second) = (digits(9, 11)?, digits(11, 13)?, digits(13, 15)?);
    // Howard Hinnant's `days_from_civil`.
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(((days * 24 + hour) * 60 + minute) * 60_000 + second * 1000)
}
