//! An SMTP server for the SMTP driver's tests (area 16, "Email"; chunk
//! 7.3): a localhost server on its own thread that speaks enough SMTP to
//! take a message, signs in with `AUTH PLAIN`, and keeps every message it
//! accepted, so a test sees exactly what was sent. Each conversation goes
//! as the next scripted [`Behaviour`] says, or takes the message when none
//! is. It never reaches a network.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

/// How one conversation goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Behaviour {
    /// The message is taken.
    Accept,
    /// The recipient is refused for good: `550`.
    RejectRecipient,
    /// The connection is dropped once the message's end has arrived, with
    /// no answer: the message may or may not have been taken.
    DropAfterData,
}

/// One message as it arrived.
#[derive(Clone, Debug)]
pub struct Received {
    pub from: String,
    pub to: Vec<String>,
    /// `AUTH PLAIN`'s user name and password, when it signed in.
    pub signed_in: Option<(String, String)>,
    /// Its text, dot-stuffing undone, CRLF line ends.
    pub data: String,
}

#[derive(Default)]
struct State {
    received: Vec<Received>,
    behaviours: VecDeque<Behaviour>,
    conversations: usize,
}

pub struct FakeSmtp {
    pub port: u16,
    state: Arc<Mutex<State>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl FakeSmtp {
    pub fn start() -> FakeSmtp {
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
        FakeSmtp {
            port,
            state,
            stop,
            thread: Some(thread),
        }
    }

    /// The next conversation goes as `behaviour`, after those scripted
    /// already.
    pub fn next(&self, behaviour: Behaviour) {
        self.state
            .lock()
            .expect("state")
            .behaviours
            .push_back(behaviour);
    }

    /// Every message it took.
    pub fn received(&self) -> Vec<Received> {
        self.state.lock().expect("state").received.clone()
    }

    /// How many conversations it had.
    pub fn conversations(&self) -> usize {
        self.state.lock().expect("state").conversations
    }
}

impl Drop for FakeSmtp {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(("127.0.0.1", self.port));
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn decode_base64(text: &str) -> Vec<u8> {
    let value = |c: u8| match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    };
    let digits: Vec<u8> = text.bytes().filter_map(value).collect();
    let mut out = Vec::new();
    for chunk in digits.chunks(4) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, d)| n | (u32::from(*d) << (18 - 6 * i)));
        out.push((n >> 16) as u8);
        if chunk.len() > 2 {
            out.push((n >> 8) as u8);
        }
        if chunk.len() > 3 {
            out.push(n as u8);
        }
    }
    out
}

/// Base64 back to text, for a test reading a message's parts.
pub fn base64_text(text: &str) -> String {
    String::from_utf8(decode_base64(text)).expect("text")
}

fn serve(stream: TcpStream, state: &Mutex<State>) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let behaviour = {
        let mut state = state.lock().expect("state");
        state.conversations += 1;
        state.behaviours.pop_front().unwrap_or(Behaviour::Accept)
    };
    let mut writer = match stream.try_clone() {
        Ok(clone) => clone,
        Err(_) => return,
    };
    let mut reader = BufReader::new(stream);
    let mut say = |line: &str| {
        let _ = writer.write_all(line.as_bytes());
        let _ = writer.write_all(b"\r\n");
        let _ = writer.flush();
    };
    say("220 localhost ESMTP fake");
    let mut from = String::new();
    let mut to = Vec::new();
    let mut signed_in = None;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 {
            return;
        }
        let line = line.trim_end().to_string();
        let upper = line.to_ascii_uppercase();
        if upper.starts_with("EHLO") {
            say("250-localhost");
            say("250 AUTH PLAIN");
        } else if let Some(plain) = line.strip_prefix("AUTH PLAIN ") {
            let decoded = decode_base64(plain);
            let parts: Vec<String> = decoded
                .split(|byte| *byte == 0)
                .map(|part| String::from_utf8_lossy(part).into_owned())
                .collect();
            if let [_, user, password] = parts.as_slice() {
                signed_in = Some((user.clone(), password.clone()));
            }
            say("235 2.7.0 Authentication successful");
        } else if upper.starts_with("MAIL FROM:") {
            from = line["MAIL FROM:".len()..]
                .trim_matches(|c| c == '<' || c == '>')
                .to_string();
            say("250 2.1.0 Ok");
        } else if upper.starts_with("RCPT TO:") {
            if behaviour == Behaviour::RejectRecipient {
                say("550 5.1.1 No such user");
                continue;
            }
            to.push(
                line["RCPT TO:".len()..]
                    .trim_matches(|c| c == '<' || c == '>')
                    .to_string(),
            );
            say("250 2.1.5 Ok");
        } else if upper == "DATA" {
            say("354 End data with <CR><LF>.<CR><LF>");
            let mut data = String::new();
            loop {
                let mut part = String::new();
                if reader.read_line(&mut part).unwrap_or(0) == 0 {
                    return;
                }
                if part == ".\r\n" {
                    break;
                }
                let part = part.strip_prefix('.').map_or(part.clone(), str::to_string);
                data.push_str(&part);
            }
            if behaviour == Behaviour::DropAfterData {
                return;
            }
            state.lock().expect("state").received.push(Received {
                from: from.clone(),
                to: to.clone(),
                signed_in: signed_in.clone(),
                data,
            });
            say("250 2.0.0 Ok: queued");
        } else if upper == "QUIT" {
            say("221 2.0.0 Bye");
            return;
        } else {
            say("502 5.5.2 Not implemented");
        }
    }
}
