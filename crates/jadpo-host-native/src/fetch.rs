//! HTTP for the package driver natively (area 11, "Calling"; chunk 7.2c):
//! [`NativeFetch`] sends a provider call's request over a socket of its
//! own, HTTPS through rustls with the certificate and the host checked
//! against Mozilla's roots ([`crate::tls`], since chunk 8.1b),
//! or plain HTTP to this machine, which is how a test reaches its server.
//! Each exchange runs on tokio's blocking pool, so the worker's thread
//! goes on serving while a provider answers, and is given up once the
//! call's timeout passes.

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use jadpo_drivers::package::{Fetch, FetchError, Opened};
use jadpo_runtime::LocalFuture;
use jadpo_runtime::provider::{MOST_ANSWER, Request, Response};
use jadpo_runtime::stream::{Chunk, Coming};

/// The longest a connection is waited for, within the call's timeout.
const CONNECT: Duration = Duration::from_secs(5);

/// The package driver's HTTP natively.
#[derive(Clone, Copy, Debug, Default)]
pub struct NativeFetch;

impl Fetch for NativeFetch {
    fn fetch<'a>(&'a self, request: &'a Request) -> LocalFuture<'a, Result<Response, FetchError>> {
        let request = request.clone();
        Box::pin(async move {
            let timeout = Duration::from_millis(request.timeout_ms.max(1));
            let exchange = tokio::task::spawn_blocking(move || exchange(&request, timeout));
            match tokio::time::timeout(timeout, exchange).await {
                Ok(Ok(answered)) => answered,
                Ok(Err(_)) => Err(FetchError::NoAnswer("the exchange stopped".into())),
                Err(_) => Err(FetchError::TimedOut),
            }
        })
    }

    /// The answer's head once it comes, its body read on a thread of its
    /// own and handed over as it arrives (chunk 5.5v6): what a model's
    /// streamed answer is read with. Dropping the body closes the
    /// connection at once, so a client that went away stops the reading.
    fn open<'a>(&'a self, request: &'a Request) -> LocalFuture<'a, Result<Opened, FetchError>> {
        let request = request.clone();
        Box::pin(async move {
            let timeout = Duration::from_millis(request.timeout_ms.max(1));
            let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
            let (closer_sender, closer) = std::sync::mpsc::channel::<TcpStream>();
            std::thread::spawn(move || streamed(&request, timeout, &sender, &closer_sender));
            let head = tokio::time::timeout(timeout, receiver.recv()).await;
            let closing = closer.try_recv().ok();
            match head {
                Ok(Some(Part::Head(status, headers))) => Ok(Opened {
                    status,
                    headers,
                    body: Box::new(Body { receiver, closing }),
                }),
                Ok(Some(Part::Failed(error))) => Err(error),
                Ok(_) => Err(FetchError::NoAnswer("the exchange stopped".into())),
                Err(_) => {
                    if let Some(stream) = closing {
                        let _ = stream.shutdown(std::net::Shutdown::Both);
                    }
                    Err(FetchError::TimedOut)
                }
            }
        })
    }
}

/// What the thread reading a streamed answer hands over.
enum Part {
    Head(u16, Vec<(String, String)>),
    Failed(FetchError),
    Chunk(Chunk),
}

/// A streamed answer's body as its thread hands it over; dropped, it
/// shuts its connection, which ends the thread's read.
struct Body {
    receiver: tokio::sync::mpsc::UnboundedReceiver<Part>,
    closing: Option<TcpStream>,
}

impl Coming for Body {
    fn next(&mut self) -> LocalFuture<'_, Chunk> {
        Box::pin(async move {
            match self.receiver.recv().await {
                Some(Part::Chunk(chunk)) => chunk,
                _ => Chunk::Broken,
            }
        })
    }
}

impl Drop for Body {
    fn drop(&mut self) {
        if let Some(stream) = self.closing.take() {
            let _ = stream.shutdown(std::net::Shutdown::Both);
        }
    }
}

/// One streamed exchange, blocking, on its own thread: connect, send the
/// request whole, hand over the head, then each part of the body as it is
/// read, a chunked body's framing taken off, until it ends or no one is
/// reading.
fn streamed(
    request: &Request,
    timeout: Duration,
    out: &tokio::sync::mpsc::UnboundedSender<Part>,
    closer: &std::sync::mpsc::Sender<TcpStream>,
) {
    let failed = |error: FetchError| {
        let _ = out.send(Part::Failed(error));
    };
    let Some(target) = target(&request.url) else {
        return failed(FetchError::Refused("an address it can't send to".into()));
    };
    let bare = target.host.trim_start_matches('[').trim_end_matches(']');
    let Ok(addresses) = (bare, target.port).to_socket_addrs() else {
        return failed(FetchError::NotSent(format!(
            "`{}` couldn't be found",
            target.host
        )));
    };
    let mut stream = None;
    let mut last = String::from("nothing to connect to");
    for address in addresses {
        match TcpStream::connect_timeout(&address, CONNECT.min(timeout)) {
            Ok(connected) => {
                stream = Some(connected);
                break;
            }
            Err(error) => last = error.to_string(),
        }
    }
    let Some(stream) = stream else {
        return failed(FetchError::NotSent(format!("no connection: {last}")));
    };
    let _ = stream.set_nodelay(true);
    if let Err(error) = stream
        .set_read_timeout(Some(timeout))
        .and_then(|()| stream.set_write_timeout(Some(timeout)))
    {
        return failed(FetchError::NotSent(error.to_string()));
    }
    if let Ok(copy) = stream.try_clone() {
        let _ = closer.send(copy);
    }
    let bytes = request_bytes(request, &target);
    let reading = if target.secure {
        match crate::tls::blocking(stream, bare) {
            Ok(tls) => read_streamed(tls, &bytes, out),
            Err(error) => {
                return failed(FetchError::NotSent(format!(
                    "no secure connection: {error}"
                )));
            }
        }
    } else {
        read_streamed(stream, &bytes, out)
    };
    if let Err(error) = reading {
        failed(error);
    }
}

/// Writes `request` and hands over the answer's head, then its body as it
/// is read. `Err` only before the head came.
fn read_streamed<S: Read + Write>(
    mut stream: S,
    request: &[u8],
    out: &tokio::sync::mpsc::UnboundedSender<Part>,
) -> Result<(), FetchError> {
    stream
        .write_all(request)
        .and_then(|()| stream.flush())
        .map_err(|error| FetchError::NoAnswer(error.to_string()))?;
    let mut raw = Vec::new();
    let mut buffer = [0; 8192];
    let start = loop {
        if let Some((start, _)) = head(&raw) {
            break start;
        }
        match stream.read(&mut buffer) {
            Ok(0) => return Err(FetchError::NoAnswer("an answer that isn't HTTP".into())),
            Ok(n) => raw.extend_from_slice(buffer.get(..n).unwrap_or_default()),
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                return Err(FetchError::TimedOut);
            }
            Err(error) => return Err(FetchError::NoAnswer(error.to_string())),
        }
        if raw.len() > MOST_ANSWER {
            return Err(FetchError::NoAnswer("an answer too long".into()));
        }
    };
    let Some((_, lines)) = head(&raw) else {
        return Err(FetchError::NoAnswer("an answer that isn't HTTP".into()));
    };
    let mut status_line = lines
        .first()
        .into_iter()
        .flat_map(|line| line.split_whitespace());
    let status = match (status_line.next(), status_line.next()) {
        (Some(version), Some(code)) if version.starts_with("HTTP/1.") => code.parse().ok(),
        _ => None,
    }
    .ok_or_else(|| FetchError::NoAnswer("an answer that isn't HTTP".into()))?;
    let headers: Vec<(String, String)> = lines
        .iter()
        .skip(1)
        .filter_map(|line| {
            let (name, value) = line.split_once(':')?;
            Some((name.trim().to_ascii_lowercase(), value.trim().to_string()))
        })
        .collect();
    let chunked =
        header(&lines, "transfer-encoding").is_some_and(|v| v.eq_ignore_ascii_case("chunked"));
    let mut left = header(&lines, "content-length").and_then(|v| v.parse::<usize>().ok());
    if out.send(Part::Head(status, headers)).is_err() {
        return Ok(());
    }
    let mut body = Chunking::new(chunked);
    let mut pending: Vec<u8> = raw.get(start..).unwrap_or_default().to_vec();
    let mut total = 0usize;
    loop {
        if !pending.is_empty() {
            let mut data = body.take(&mut pending);
            if let Some(length) = left.as_mut() {
                data.truncate(*length);
                *length -= data.len();
            }
            total += data.len();
            if total > MOST_ANSWER {
                let _ = out.send(Part::Chunk(Chunk::Broken));
                return Ok(());
            }
            if !data.is_empty() && out.send(Part::Chunk(Chunk::Bytes(data))).is_err() {
                return Ok(());
            }
            if body.ended || left == Some(0) {
                let _ = out.send(Part::Chunk(Chunk::End));
                return Ok(());
            }
        }
        match stream.read(&mut buffer) {
            Ok(0) => {
                let ended = !chunked && left.is_none();
                let _ = out.send(Part::Chunk(if ended { Chunk::End } else { Chunk::Broken }));
                return Ok(());
            }
            Ok(n) => pending.extend_from_slice(buffer.get(..n).unwrap_or_default()),
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                let _ = out.send(Part::Chunk(Chunk::TimedOut));
                return Ok(());
            }
            Err(_) => {
                let _ = out.send(Part::Chunk(Chunk::Broken));
                return Ok(());
            }
        }
    }
}

/// How a streamed body is framed, and how far its chunks have been read.
struct Chunking {
    chunked: bool,
    /// Bytes left of the chunk being read, or `None` before a size line.
    in_chunk: Option<usize>,
    ended: bool,
}

impl Chunking {
    fn new(chunked: bool) -> Chunking {
        Chunking {
            chunked,
            in_chunk: None,
            ended: false,
        }
    }

    /// The body's data in `pending`, its chunks' framing taken off; what
    /// can't be read yet stays.
    fn take(&mut self, pending: &mut Vec<u8>) -> Vec<u8> {
        if !self.chunked {
            return std::mem::take(pending);
        }
        let mut data = Vec::new();
        loop {
            match self.in_chunk {
                Some(0) => {
                    // The line ending after a chunk's data.
                    if pending.len() < 2 {
                        return data;
                    }
                    pending.drain(..2);
                    self.in_chunk = None;
                }
                Some(left) => {
                    if pending.is_empty() {
                        return data;
                    }
                    let n = left.min(pending.len());
                    data.extend(pending.drain(..n));
                    self.in_chunk = Some(left - n);
                }
                None => {
                    let Some(end) = pending.windows(2).position(|w| w == b"\r\n") else {
                        return data;
                    };
                    let size = std::str::from_utf8(pending.get(..end).unwrap_or_default())
                        .ok()
                        .and_then(|line| line.split(';').next())
                        .and_then(|size| usize::from_str_radix(size.trim(), 16).ok());
                    pending.drain(..end + 2);
                    match size {
                        Some(0) | None => {
                            self.ended = true;
                            pending.clear();
                            return data;
                        }
                        Some(size) => self.in_chunk = Some(size),
                    }
                }
            }
        }
    }
}

/// What the runtime fetches on its own account natively (area 11,
/// "Receiving from outside"; chunk 7.7b): AWS SNS's signing certificates
/// and subscription confirmations. A `GET` over HTTPS only, the
/// certificate and the host checked by rustls, connected only to an
/// address that is public (never loopback, private, link-local, shared,
/// reserved or multicast, IPv4 inside IPv6 included) and only to the
/// address checked, so a name that resolves again elsewhere can't move
/// it; no redirect followed; at most the bytes asked for.
#[derive(Clone, Debug, Default)]
pub struct Outward {
    /// A test's server on this machine, reached over plain HTTP for the
    /// one host named: what a test reaches AWS's host with, never built
    /// outside a test.
    #[cfg(feature = "test-outbound")]
    reach: Option<(String, std::net::SocketAddr)>,
}

impl Outward {
    /// Requests for `host` go over plain HTTP to `at`, a server on this
    /// machine: a test's stand-in for AWS's SNS host.
    #[cfg(feature = "test-outbound")]
    pub fn reaching(host: &str, at: std::net::SocketAddr) -> Outward {
        Outward {
            reach: Some((host.to_string(), at)),
        }
    }

    /// The test's server for `host`, when it is the one reached.
    fn reached(&self, host: &str) -> Option<std::net::SocketAddr> {
        #[cfg(feature = "test-outbound")]
        if let Some((reached, at)) = &self.reach
            && reached == host
        {
            return Some(*at);
        }
        let _ = host;
        None
    }
}

impl jadpo_runtime::Outbound for Outward {
    fn get<'a>(
        &'a self,
        url: &'a str,
        limit: usize,
        timeout_ms: u64,
    ) -> LocalFuture<'a, Result<(u16, Vec<u8>), String>> {
        let url = url.to_string();
        let this = self.clone();
        Box::pin(async move {
            let timeout = Duration::from_millis(timeout_ms.max(1));
            let exchange = tokio::task::spawn_blocking(move || this.exchange(&url, limit, timeout));
            match tokio::time::timeout(timeout, exchange).await {
                Ok(Ok(answered)) => answered,
                Ok(Err(_)) => Err("the exchange stopped".into()),
                Err(_) => Err("no answer in time".into()),
            }
        })
    }
}

impl Outward {
    fn exchange(
        &self,
        url: &str,
        limit: usize,
        timeout: Duration,
    ) -> Result<(u16, Vec<u8>), String> {
        let target = target(url)
            .filter(|target| target.secure)
            .ok_or("not HTTPS")?;
        let bare = target.host.trim_start_matches('[').trim_end_matches(']');
        let reached = self.reached(bare);
        let address = match reached {
            Some(at) => at,
            None => (bare, target.port)
                .to_socket_addrs()
                .map_err(|_| format!("`{bare}` couldn't be found"))?
                .find(|address| public(address.ip()))
                .ok_or_else(|| format!("`{bare}` has no public address"))?,
        };
        let stream = TcpStream::connect_timeout(&address, CONNECT.min(timeout))
            .map_err(|error| format!("no connection: {error}"))?;
        stream
            .set_read_timeout(Some(timeout))
            .and_then(|()| stream.set_write_timeout(Some(timeout)))
            .map_err(|error| error.to_string())?;
        let head = format!(
            "GET {} HTTP/1.1\r\nHost: {}\r\nAccept: */*\r\nConnection: close\r\n\r\n",
            target.path, target.host
        );
        let raw = if reached.is_some() {
            send(stream, head.as_bytes())
        } else {
            let tls = crate::tls::blocking(stream, bare)
                .map_err(|error| format!("no secure connection: {error}"))?;
            send(tls, head.as_bytes())
        }
        .map_err(|error| format!("{error:?}"))?;
        let answer = parse(&raw).ok_or("an answer that isn't HTTP")?;
        if answer.body.len() > limit {
            return Err("an answer too long".into());
        }
        Ok((answer.status, answer.body))
    }
}

/// Whether `ip` is an address on the public internet: not loopback,
/// private, link-local, shared (100.64/10), reserved, documentation,
/// benchmarking, unspecified, broadcast or multicast; IPv4 inside IPv6
/// judged as itself.
pub fn public(ip: std::net::IpAddr) -> bool {
    use std::net::IpAddr;
    match ip {
        IpAddr::V4(v4) => {
            let [a, b, c, _] = v4.octets();
            !(v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_documentation()
                || v4.is_unspecified()
                || v4.is_multicast()
                || a == 0
                || a >= 240
                || (a == 100 && (64..128).contains(&b))
                || (a == 192 && b == 0 && c == 0)
                || (a == 198 && (18..20).contains(&b)))
        }
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped().or_else(|| v6.to_ipv4()) {
                return public(IpAddr::V4(v4));
            }
            let first = v6.segments()[0];
            !(v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || first & 0xfe00 == 0xfc00
                || first & 0xffc0 == 0xfe80
                || first == 0x2001 && v6.segments()[1] == 0x0db8
                || first == 0x0064 && v6.segments()[1] == 0xff9b)
        }
    }
}

/// Where a request goes.
struct Target {
    secure: bool,
    host: String,
    port: u16,
    /// Its path and query.
    path: String,
}

fn target(url: &str) -> Option<Target> {
    let (secure, rest) = if let Some(rest) = url.strip_prefix("https://") {
        (true, rest)
    } else {
        (false, url.strip_prefix("http://")?)
    };
    let (authority, path) = match rest.find(['/', '?']) {
        Some(at) => rest.split_at(at),
        None => (rest, "/"),
    };
    let path = if path.starts_with('?') {
        format!("/{path}")
    } else {
        path.to_string()
    };
    let (host, port) = if let Some(inner) = authority.strip_prefix('[') {
        let (host, after) = inner.split_once(']')?;
        (format!("[{host}]"), after.strip_prefix(':'))
    } else {
        match authority.rsplit_once(':') {
            Some((host, port)) => (host.to_string(), Some(port)),
            None => (authority.to_string(), None),
        }
    };
    let port = match port {
        Some(port) => port.parse().ok()?,
        None if secure => 443,
        None => 80,
    };
    Some(Target {
        secure,
        host,
        port,
        path,
    })
}

/// One exchange, blocking: connect, send the request whole, read the
/// whole answer.
fn exchange(request: &Request, timeout: Duration) -> Result<Response, FetchError> {
    let Some(target) = target(&request.url) else {
        return Err(FetchError::Refused("an address it can't send to".into()));
    };
    let bare = target.host.trim_start_matches('[').trim_end_matches(']');
    let addresses = (bare, target.port)
        .to_socket_addrs()
        .map_err(|_| FetchError::NotSent(format!("`{}` couldn't be found", target.host)))?;
    let mut stream = None;
    let mut last = String::from("nothing to connect to");
    for address in addresses {
        match TcpStream::connect_timeout(&address, CONNECT.min(timeout)) {
            Ok(connected) => {
                stream = Some(connected);
                break;
            }
            Err(error) => last = error.to_string(),
        }
    }
    let stream = stream.ok_or_else(|| FetchError::NotSent(format!("no connection: {last}")))?;
    let _ = stream.set_nodelay(true);
    stream
        .set_read_timeout(Some(timeout))
        .and_then(|()| stream.set_write_timeout(Some(timeout)))
        .map_err(|error| FetchError::NotSent(error.to_string()))?;
    let bytes = request_bytes(request, &target);
    let raw = if target.secure {
        let tls = crate::tls::blocking(stream, bare)
            .map_err(|error| FetchError::NotSent(format!("no secure connection: {error}")))?;
        send(tls, &bytes)?
    } else {
        send(stream, &bytes)?
    };
    parse(&raw).ok_or_else(|| FetchError::NoAnswer("an answer that isn't HTTP".into()))
}

/// `request` as it is written to `target`: its head, then its body.
fn request_bytes(request: &Request, target: &Target) -> Vec<u8> {
    let default = if target.secure { 443 } else { 80 };
    let port = if target.port == default {
        String::new()
    } else {
        format!(":{}", target.port)
    };
    let body = request.body.as_deref().unwrap_or_default();
    let mut head = format!(
        "{} {} HTTP/1.1\r\nHost: {}{port}\r\n",
        request.method.word(),
        target.path,
        target.host
    );
    for (name, value) in &request.headers {
        head.push_str(name);
        head.push_str(": ");
        head.push_str(value);
        head.push_str("\r\n");
    }
    if request.body.is_some() {
        head.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }
    head.push_str("Connection: close\r\n\r\n");
    let mut bytes = head.into_bytes();
    bytes.extend_from_slice(body.as_bytes());
    bytes
}

/// Writes `request` and reads the answer until it is whole or the other
/// end closes.
fn send<S: Read + Write>(mut stream: S, request: &[u8]) -> Result<Vec<u8>, FetchError> {
    // Once a byte may have left, a failure is an answer that may have
    // acted.
    stream
        .write_all(request)
        .and_then(|()| stream.flush())
        .map_err(|error| FetchError::NoAnswer(error.to_string()))?;
    let mut raw = Vec::new();
    let mut buffer = [0; 8192];
    loop {
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => {
                raw.extend_from_slice(buffer.get(..n).unwrap_or_default());
                if raw.len() > MOST_ANSWER {
                    return Err(FetchError::NoAnswer("an answer too long".into()));
                }
                if complete(&raw) {
                    break;
                }
            }
            // A TLS peer that closes without notice has still sent what
            // it sent; parsing decides whether that is a whole answer.
            Err(_) if !raw.is_empty() => break,
            // The socket's own timeout, the call's: no answer in time.
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                return Err(FetchError::TimedOut);
            }
            Err(error) => return Err(FetchError::NoAnswer(error.to_string())),
        }
    }
    Ok(raw)
}

/// The end of the head, and its lines.
fn head(raw: &[u8]) -> Option<(usize, Vec<String>)> {
    let end = raw.windows(4).position(|w| w == b"\r\n\r\n")?;
    let text = std::str::from_utf8(raw.get(..end)?).ok()?;
    Some((end + 4, text.split("\r\n").map(str::to_string).collect()))
}

fn header<'a>(lines: &'a [String], name: &str) -> Option<&'a str> {
    lines.iter().skip(1).find_map(|line| {
        let (key, value) = line.split_once(':')?;
        key.trim()
            .eq_ignore_ascii_case(name)
            .then_some(value.trim())
    })
}

/// Whether the whole answer has arrived, by its `Content-Length`.
fn complete(raw: &[u8]) -> bool {
    let Some((start, lines)) = head(raw) else {
        return false;
    };
    match header(&lines, "content-length").and_then(|v| v.parse::<usize>().ok()) {
        Some(length) => raw.len() >= start + length,
        None => false,
    }
}

fn parse(raw: &[u8]) -> Option<Response> {
    let (start, lines) = head(raw)?;
    let mut status_line = lines.first()?.split_whitespace();
    if !status_line.next()?.starts_with("HTTP/1.") {
        return None;
    }
    let status = status_line.next()?.parse().ok()?;
    let rest = raw.get(start..)?;
    let body =
        if header(&lines, "transfer-encoding").is_some_and(|v| v.eq_ignore_ascii_case("chunked")) {
            unchunk(rest)?
        } else if let Some(length) =
            header(&lines, "content-length").and_then(|v| v.parse::<usize>().ok())
        {
            rest.get(..length)?.to_vec()
        } else {
            rest.to_vec()
        };
    let headers = lines
        .iter()
        .skip(1)
        .filter_map(|line| {
            let (name, value) = line.split_once(':')?;
            Some((name.trim().to_ascii_lowercase(), value.trim().to_string()))
        })
        .collect();
    Some(Response {
        status,
        headers,
        body,
    })
}

fn unchunk(mut rest: &[u8]) -> Option<Vec<u8>> {
    let mut body = Vec::new();
    loop {
        let end = rest.windows(2).position(|w| w == b"\r\n")?;
        let size = std::str::from_utf8(rest.get(..end)?).ok()?;
        let size = usize::from_str_radix(size.split(';').next()?.trim(), 16).ok()?;
        rest = rest.get(end + 2..)?;
        if size == 0 {
            return Some(body);
        }
        body.extend_from_slice(rest.get(..size)?);
        rest = rest.get(size + 2..)?;
    }
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead, BufReader};
    use std::net::TcpListener;

    use jadpo_runtime::provider::Reading;
    use jadpo_std::HttpMethod;

    use super::*;

    /// A server on this machine that answers one request with `answer`,
    /// handing back what it was sent.
    fn serve_once(answer: &'static str) -> (String, std::thread::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let thread = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut seen = String::new();
            let mut length = 0;
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = value.trim().parse().unwrap();
                }
                seen.push_str(&line);
                if line == "\r\n" {
                    break;
                }
            }
            let mut body = vec![0; length];
            reader.read_exact(&mut body).unwrap();
            seen.push_str(&String::from_utf8(body).unwrap());
            let mut stream = stream;
            stream.write_all(answer.as_bytes()).unwrap();
            seen
        });
        (format!("http://127.0.0.1:{port}"), thread)
    }

    fn request(url: String) -> Request {
        Request {
            method: HttpMethod::Post,
            url,
            headers: vec![("Authorization".into(), "Bearer test-token".into())],
            body: Some("{\"slot\":\"am\"}".into()),
            timeout_ms: 5000,
            reading: Reading::default(),
        }
    }

    /// A request goes out whole, its headers and body as made, and the
    /// answer comes back by its length or in chunks.
    #[test]
    fn a_request_is_sent_and_its_answer_read() {
        let runtime = crate::Runtime::new().unwrap();
        let (base, server) = serve_once(
            "HTTP/1.1 201 Created\r\nContent-Type: application/json\r\nContent-Length: 11\r\n\r\n{\"id\":\"b1\"}",
        );
        let sent = request(format!("{base}/v1/bookings?x=1"));
        let answer = runtime.block_on(NativeFetch.fetch(&sent)).unwrap();
        assert_eq!(answer.status, 201);
        assert_eq!(answer.body, b"{\"id\":\"b1\"}");
        let seen = server.join().unwrap();
        assert!(
            seen.starts_with("POST /v1/bookings?x=1 HTTP/1.1\r\n"),
            "{seen}"
        );
        assert!(
            seen.contains("Authorization: Bearer test-token\r\n"),
            "{seen}"
        );
        assert!(seen.contains("Content-Length: 13\r\n"), "{seen}");
        assert!(seen.ends_with("{\"slot\":\"am\"}"), "{seen}");

        let (base, _server) = serve_once(
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n4\r\n{\"a\"\r\n3\r\n:1}\r\n0\r\n\r\n",
        );
        let answer = runtime.block_on(NativeFetch.fetch(&request(base))).unwrap();
        assert_eq!(answer.body, b"{\"a\":1}");
    }

    /// Only a public address is reached on the runtime's own account.
    #[test]
    fn only_a_public_address_is_reached() {
        for private in [
            "127.0.0.1",
            "10.1.2.3",
            "172.16.0.1",
            "192.168.1.1",
            "169.254.169.254",
            "100.64.0.1",
            "0.0.0.0",
            "224.0.0.1",
            "255.255.255.255",
            "::1",
            "fe80::1",
            "fd00::1",
            "::ffff:10.0.0.1",
            "::ffff:127.0.0.1",
        ] {
            assert!(!public(private.parse().unwrap()), "{private}");
        }
        for open in ["52.94.236.248", "2600:1f18::1"] {
            assert!(public(open.parse().unwrap()), "{open}");
        }
        // Plain HTTP and a name with only a private address are refused.
        let runtime = crate::Runtime::new().unwrap();
        use jadpo_runtime::Outbound;
        let plain = runtime.block_on(Outward::default().get("http://example.com/a.pem", 100, 1000));
        assert!(plain.is_err(), "{plain:?}");
        let local = runtime.block_on(Outward::default().get("https://localhost/a.pem", 100, 1000));
        assert_eq!(local, Err("`localhost` has no public address".to_string()));
    }

    /// Nothing listening is a call that never left; a server that never
    /// answers is one that may have acted, and timed out.
    #[test]
    fn a_call_that_never_left_differs_from_one_never_answered() {
        let runtime = crate::Runtime::new().unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let refused =
            runtime.block_on(NativeFetch.fetch(&request(format!("http://127.0.0.1:{port}"))));
        assert!(
            matches!(refused, Err(FetchError::NotSent(_))),
            "{refused:?}"
        );

        let silent = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = silent.local_addr().unwrap().port();
        let mut slow = request(format!("http://127.0.0.1:{port}"));
        slow.timeout_ms = 200;
        let unanswered = runtime.block_on(NativeFetch.fetch(&slow));
        // No answer within its timeout is told apart (chunk 7.2c2).
        assert!(
            matches!(unanswered, Err(FetchError::TimedOut)),
            "{unanswered:?}"
        );
        drop(silent);
    }
}
