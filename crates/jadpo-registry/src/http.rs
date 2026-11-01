//! Plain HTTP/1.1 on this machine, for the local registry and for reading
//! a served OpenAPI document: one request per connection, closed after
//! the answer. It is never used to reach another machine: [`fetch`] only
//! takes `http://` addresses on the loopback interface or `localhost`.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

use crate::protocol::Method;

/// The most a request's or answer's head may be.
const HEAD_LIMIT: usize = 64 * 1024;

/// The most a request's body may be, as a Jadpo application reads one.
const BODY_LIMIT: usize = 1024 * 1024;

/// The most an answer's body may be.
const ANSWER_LIMIT: usize = 256 * 1024 * 1024;

const TIMEOUT: Duration = Duration::from_secs(30);

/// A request the server read.
#[derive(Clone, Debug)]
pub struct Incoming {
    pub method: String,
    pub target: String,
    pub body: Vec<u8>,
}

/// An answer: its status, content type and body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fetched {
    pub status: u16,
    pub content_type: Option<String>,
    pub body: Vec<u8>,
}

/// What answers each request.
pub type Handler = Arc<dyn Fn(Incoming) -> Fetched + Send + Sync>;

/// A server on `127.0.0.1`, stopped when dropped.
#[derive(Debug)]
pub struct Server {
    address: SocketAddr,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Server {
    /// Serves `handler` on a port the system picks.
    pub fn start(handler: Handler) -> std::io::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let address = listener.local_addr()?;
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let thread = std::thread::spawn(move || {
            for stream in listener.incoming() {
                if stopping.load(Ordering::SeqCst) {
                    break;
                }
                let Ok(stream) = stream else { continue };
                let handler = handler.clone();
                std::thread::spawn(move || {
                    let _ = answer(stream, &handler);
                });
            }
        });
        Ok(Self {
            address,
            stop,
            thread: Some(thread),
        })
    }

    pub fn address(&self) -> SocketAddr {
        self.address
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        // Wakes the accepting thread so it sees the flag.
        let _ = TcpStream::connect(self.address);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn answer(stream: TcpStream, handler: &Handler) -> std::io::Result<()> {
    stream.set_read_timeout(Some(TIMEOUT))?;
    stream.set_write_timeout(Some(TIMEOUT))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let reply = match read_request(&mut reader) {
        Ok(incoming) => handler(incoming),
        Err(why) => Fetched {
            status: 400,
            content_type: Some("text/plain".into()),
            body: why.into_bytes(),
        },
    };
    let mut stream = stream;
    let head = format!(
        "HTTP/1.1 {} {}\r\ncontent-length: {}\r\n{}connection: close\r\n\r\n",
        reply.status,
        reason(reply.status),
        reply.body.len(),
        reply
            .content_type
            .as_ref()
            .map(|t| format!("content-type: {t}\r\n"))
            .unwrap_or_default(),
    );
    stream.write_all(head.as_bytes())?;
    stream.write_all(&reply.body)?;
    stream.flush()?;
    stream.shutdown(Shutdown::Both)
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        202 => "Accepted",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        413 => "Content Too Large",
        422 => "Unprocessable Content",
        _ => "Status",
    }
}

/// Reads the head's lines, up to the empty one.
fn read_head(reader: &mut impl BufRead) -> Result<Vec<String>, String> {
    let mut lines = Vec::new();
    let mut total = 0;
    loop {
        let mut line = Vec::new();
        let read = reader
            .take((HEAD_LIMIT - total + 1) as u64)
            .read_until(b'\n', &mut line)
            .map_err(|e| e.to_string())?;
        if read == 0 {
            return Err("the connection closed in the head".into());
        }
        total += read;
        if total > HEAD_LIMIT {
            return Err("the head is too large".into());
        }
        let line = String::from_utf8(line).map_err(|_| "the head isn't UTF-8".to_string())?;
        let line = line.trim_end_matches(['\r', '\n']).to_string();
        if line.is_empty() {
            return Ok(lines);
        }
        lines.push(line);
    }
}

fn header<'a>(lines: &'a [String], name: &str) -> Option<&'a str> {
    lines.iter().skip(1).find_map(|line| {
        let (key, value) = line.split_once(':')?;
        key.trim().eq_ignore_ascii_case(name).then(|| value.trim())
    })
}

fn read_request(reader: &mut impl BufRead) -> Result<Incoming, String> {
    let lines = read_head(reader)?;
    let mut parts = lines[0].split(' ');
    let (Some(method), Some(target), Some(_version), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err("the request line can't be read".into());
    };
    if header(&lines, "transfer-encoding").is_some() {
        return Err("a chunked body isn't read".into());
    }
    let length: usize = match header(&lines, "content-length") {
        Some(length) => length
            .parse()
            .map_err(|_| "content-length can't be read".to_string())?,
        None => 0,
    };
    if length > BODY_LIMIT {
        return Err("the body is over 1 MB".into());
    }
    let mut body = vec![0u8; length];
    reader.read_exact(&mut body).map_err(|e| e.to_string())?;
    Ok(Incoming {
        method: method.to_string(),
        target: target.to_string(),
        body,
    })
}

/// Sends `method` to an `http://` address on this machine and reads the
/// answer.
pub fn fetch(method: Method, url: &str, body: Option<&[u8]>) -> Result<Fetched, String> {
    let rest = url
        .strip_prefix("http://")
        .ok_or_else(|| format!("`{url}` isn't an http:// address on this machine"))?;
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let host = match authority.rsplit_once(':') {
        Some((host, _)) => host,
        None => authority,
    };
    if !matches!(host, "127.0.0.1" | "localhost" | "[::1]") {
        return Err(format!("`{url}` isn't on this machine"));
    }
    let authority = if authority.contains(':') && !authority.ends_with(']') {
        authority.to_string()
    } else {
        format!("{authority}:80")
    };
    let mut stream = TcpStream::connect(authority.as_str()).map_err(|e| format!("{url}: {e}"))?;
    stream
        .set_read_timeout(Some(TIMEOUT))
        .map_err(|e| e.to_string())?;
    let body = body.unwrap_or_default();
    let mut request = format!(
        "{method} {path} HTTP/1.1\r\nhost: {authority}\r\naccept: */*\r\nconnection: close\r\ncontent-length: {}\r\n",
        body.len()
    );
    if !body.is_empty() {
        request.push_str("content-type: application/json\r\n");
    }
    request.push_str("\r\n");
    stream
        .write_all(request.as_bytes())
        .map_err(|e| e.to_string())?;
    stream.write_all(body).map_err(|e| e.to_string())?;
    let mut reader = BufReader::new(stream);
    let lines = read_head(&mut reader)?;
    let status: u16 = lines[0]
        .split(' ')
        .nth(1)
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| format!("`{}` isn't a status line", lines[0]))?;
    let content_type = header(&lines, "content-type")
        .map(|t| t.split(';').next().unwrap_or(t).trim().to_ascii_lowercase());
    let body =
        if header(&lines, "transfer-encoding").is_some_and(|t| t.eq_ignore_ascii_case("chunked")) {
            read_chunked(&mut reader)?
        } else if let Some(length) = header(&lines, "content-length") {
            let length: usize = length
                .parse()
                .map_err(|_| "content-length can't be read".to_string())?;
            if length > ANSWER_LIMIT {
                return Err("the answer is too large".into());
            }
            let mut body = vec![0u8; length];
            reader.read_exact(&mut body).map_err(|e| e.to_string())?;
            body
        } else {
            let mut body = Vec::new();
            reader
                .take(ANSWER_LIMIT as u64 + 1)
                .read_to_end(&mut body)
                .map_err(|e| e.to_string())?;
            if body.len() > ANSWER_LIMIT {
                return Err("the answer is too large".into());
            }
            body
        };
    Ok(Fetched {
        status,
        content_type,
        body,
    })
}

fn read_chunked(reader: &mut impl BufRead) -> Result<Vec<u8>, String> {
    let mut body = Vec::new();
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).map_err(|e| e.to_string())?;
        let size = line.trim().split(';').next().unwrap_or("");
        let size = usize::from_str_radix(size, 16)
            .map_err(|_| "a chunk's size can't be read".to_string())?;
        if body.len() + size > ANSWER_LIMIT {
            return Err("the answer is too large".into());
        }
        if size == 0 {
            // The trailer, up to its empty line.
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).map_err(|e| e.to_string())? == 0
                    || line.trim().is_empty()
                {
                    return Ok(body);
                }
            }
        }
        let start = body.len();
        body.resize(start + size, 0);
        reader
            .read_exact(&mut body[start..])
            .map_err(|e| e.to_string())?;
        let mut end = [0u8; 2];
        reader.read_exact(&mut end).map_err(|e| e.to_string())?;
    }
}
