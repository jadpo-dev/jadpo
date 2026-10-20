//! The local fake `jadpo package conform` runs a capability's suite
//! against by default (area 11, "Proving a package works"; area 21, "How a
//! provider fills it"; chunk 5.5v7b): a server on this machine's loopback
//! address, on a thread of its own, that answers every request with the
//! answer the suite set for the case it is running, again on every send
//! until the next is set, so a call the runtime sends again is answered
//! the same. It never reaches a network.
//!
//! Each answer is made from the package's own recording of the operation,
//! its placeholders filled as `jadpo dev`'s fake fills them
//! ([`jadpo_runtime::recorded::filled`]), then marked where the package's
//! mapping says the provider marks what the case needs: the answer's text,
//! a refusal, a cut-off, a stream's pieces and its end, an embedding's
//! vector, the tokens used. A mapping that reads somewhere the provider's
//! answer doesn't put it fails its case, since the answer is still the
//! recording's shape and is read through the package's own types.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use jadpo_std::http::{Codec, CodecId};
use jadpo_value::json::Codecs;
use serde_json::Value as J;

/// One answer: its status, `Content-Type` and body.
#[derive(Clone, Debug)]
pub(crate) struct Answer {
    pub status: u16,
    pub content_type: &'static str,
    pub body: String,
}

impl Answer {
    pub(crate) fn json(body: String) -> Answer {
        Answer {
            status: 200,
            content_type: "application/json",
            body,
        }
    }

    pub(crate) fn events(events: &[J], done: bool) -> Answer {
        let mut body = String::new();
        for event in events {
            body.push_str("data: ");
            body.push_str(&event.to_string());
            body.push_str("\n\n");
        }
        if done {
            body.push_str("data: [DONE]\n\n");
        }
        Answer {
            status: 200,
            content_type: "text/event-stream",
            body,
        }
    }
}

/// The fake, listening until it is dropped.
pub(crate) struct LocalFake {
    /// Where it listens: `http://127.0.0.1:<port>`.
    pub url: String,
    current: Arc<Mutex<Option<Answer>>>,
    stop: Arc<AtomicBool>,
    port: u16,
    thread: Option<JoinHandle<()>>,
}

impl LocalFake {
    pub(crate) fn start() -> std::io::Result<LocalFake> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let port = listener.local_addr()?.port();
        let current = Arc::new(Mutex::new(None::<Answer>));
        let stop = Arc::new(AtomicBool::new(false));
        let thread = {
            let current = Arc::clone(&current);
            let stop = Arc::clone(&stop);
            std::thread::spawn(move || {
                for stream in listener.incoming() {
                    if stop.load(Ordering::SeqCst) {
                        break;
                    }
                    if let Ok(stream) = stream {
                        let answer = current.lock().ok().and_then(|held| held.clone());
                        serve(stream, answer);
                    }
                }
            })
        };
        Ok(LocalFake {
            url: format!("http://127.0.0.1:{port}"),
            current,
            stop,
            port,
            thread: Some(thread),
        })
    }

    /// Answers every request with `answer` from now on.
    pub(crate) fn answer(&self, answer: Answer) {
        if let Ok(mut held) = self.current.lock() {
            *held = Some(answer);
        }
    }
}

impl Drop for LocalFake {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(("127.0.0.1", self.port));
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Reads one request whole, then answers it and closes the connection.
fn serve(stream: TcpStream, answer: Option<Answer>) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let Ok(clone) = stream.try_clone() else {
        return;
    };
    let mut reader = BufReader::new(clone);
    let mut line = String::new();
    if reader.read_line(&mut line).unwrap_or(0) == 0 {
        return;
    }
    let mut length = 0usize;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).unwrap_or(0) == 0 {
            break;
        }
        let header = header.trim_end();
        if header.is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':')
            && name.trim().eq_ignore_ascii_case("content-length")
        {
            length = value.trim().parse().unwrap_or(0);
        }
    }
    let mut body = vec![0u8; length];
    let _ = reader.read_exact(&mut body);
    let answer = answer.unwrap_or(Answer {
        status: 500,
        content_type: "application/json",
        body: "{\"error\":{\"code\":\"no_answer_set\",\"type\":\"no_answer_set\"}}".to_string(),
    });
    let reason = if answer.status == 200 { "OK" } else { "Error" };
    let head = format!(
        "HTTP/1.1 {} {reason}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        answer.status,
        answer.content_type,
        answer.body.len()
    );
    let mut stream = stream;
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(answer.body.as_bytes());
    let _ = stream.flush();
}

// --- marking an answer ------------------------------------------------------

/// A place in a JSON document by spellings on the wire: a member, a list's
/// item by number, or every item of a list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Step {
    Member(String),
    Item(usize),
    Each,
}

/// `path`, dotted by spellings on the wire as a mapping writes a marker's
/// `path:` (`choices.0.finish_reason`), as steps.
pub(crate) fn dotted(path: &str) -> Vec<Step> {
    path.split('.').map(step).collect()
}

/// A path by spellings on the wire, already split.
pub(crate) fn spelled(path: &[String]) -> Vec<Step> {
    path.iter().map(|part| step(part)).collect()
}

fn step(part: &str) -> Step {
    if part == jadpo_std::ai::EACH {
        Step::Each
    } else if let Ok(at) = part.parse::<usize>() {
        Step::Item(at)
    } else {
        Step::Member(part.to_string())
    }
}

/// `path`, by field names in the output type the codec numbered `id`
/// reads (a mapping's `answer:`, `usage:` and `refused_when: { holds: }`),
/// as steps by spellings on the wire.
pub(crate) fn by_fields(codecs: &Codecs<'_>, id: CodecId, path: &[String]) -> Option<Vec<Step>> {
    let Some((first, rest)) = path.split_first() else {
        return Some(Vec::new());
    };
    match codecs.codecs.get(id.0 as usize)? {
        Codec::Nullable(inner) => by_fields(codecs, *inner, path),
        Codec::Wired { contract, .. } => by_fields(codecs, *contract, path),
        Codec::List { item, .. } | Codec::Set { item, .. } => {
            let mut out = vec![step(first)];
            if matches!(out[0], Step::Member(_)) {
                return None;
            }
            out.extend(by_fields(codecs, *item, rest)?);
            Some(out)
        }
        Codec::Object { shape, fields, .. } => {
            let shape = codecs.shapes.get(shape.0 as usize)?;
            let slot = (0..shape.len()).find(|slot| {
                shape
                    .field(*slot)
                    .is_some_and(|(name, _)| name == first.as_str())
            })?;
            let mut out = vec![Step::Member(shape.json_name(slot)?.to_string())];
            out.extend(by_fields(codecs, *fields.get(slot)?, rest)?);
            Some(out)
        }
        _ => None,
    }
}

/// Puts `value` at `steps` in `json`, only where the recording has a
/// member there already, since a recording keeps what the provider sends;
/// under [`Step::Each`], in each item that has it, as a provider's content
/// blocks carry text only in some. Whether it was put anywhere.
pub(crate) fn put(json: &mut J, steps: &[Step], value: &J) -> bool {
    put_in(json, steps, value, false)
}

/// [`put`], the last member made where it is missing: a field a provider
/// leaves out until it has something to say, as a refusal.
pub(crate) fn put_new(json: &mut J, steps: &[Step], value: &J) -> bool {
    put_in(json, steps, value, true)
}

fn put_in(json: &mut J, steps: &[Step], value: &J, make: bool) -> bool {
    let Some((first, rest)) = steps.split_first() else {
        *json = value.clone();
        return true;
    };
    match (first, json) {
        (Step::Member(name), J::Object(members)) => {
            if rest.is_empty() {
                if !make && !members.contains_key(name) {
                    return false;
                }
                members.insert(name.clone(), value.clone());
                return true;
            }
            match members.get_mut(name) {
                Some(inner) => put_in(inner, rest, value, make),
                None => false,
            }
        }
        (Step::Item(at), J::Array(items)) => match items.get_mut(*at) {
            Some(inner) => put_in(inner, rest, value, make),
            None => false,
        },
        (Step::Each, J::Array(items)) => {
            let mut any = false;
            for item in items {
                any |= put_in(item, rest, value, false);
            }
            any
        }
        _ => false,
    }
}

/// The value at `steps` in `json`; under [`Step::Each`], the first item's
/// that has one.
pub(crate) fn get<'j>(json: &'j J, steps: &[Step]) -> Option<&'j J> {
    let Some((first, rest)) = steps.split_first() else {
        return Some(json);
    };
    match (first, json) {
        (Step::Member(name), J::Object(members)) => get(members.get(name)?, rest),
        (Step::Item(at), J::Array(items)) => get(items.get(*at)?, rest),
        (Step::Each, J::Array(items)) => items.iter().find_map(|item| get(item, rest)),
        _ => None,
    }
}

/// The places `prefix` (an operation's `output_at:`, dotted) and `steps`
/// make together.
pub(crate) fn under(prefix: Option<&str>, steps: Vec<Step>) -> Vec<Step> {
    let mut out = prefix.map(dotted).unwrap_or_default();
    out.extend(steps);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_value_is_put_at_a_dotted_place_and_only_where_each_item_has_it() {
        let mut json = serde_json::json!({
            "choices": [{"finish_reason": "stop", "message": {"content": "x"}}],
            "content": [{"type": "thinking"}, {"type": "text", "text": "y"}]
        });
        assert!(put(
            &mut json,
            &dotted("choices.0.finish_reason"),
            &J::from("length")
        ));
        assert!(put(
            &mut json,
            &[
                Step::Member("content".into()),
                Step::Each,
                Step::Member("text".into())
            ],
            &J::from("z")
        ));
        assert_eq!(json["choices"][0]["finish_reason"], "length");
        assert_eq!(json["content"][0].get("text"), None);
        assert_eq!(json["content"][1]["text"], "z");
        assert!(!put(&mut json, &dotted("nowhere.0.x"), &J::Null));
        assert!(!put(
            &mut json,
            &dotted("choices.0.refusal"),
            &J::from("no")
        ));
        assert!(put_new(
            &mut json,
            &dotted("choices.0.refusal"),
            &J::from("no")
        ));
        assert_eq!(json["choices"][0]["refusal"], "no");
    }
}
