//! HTTP for the package driver natively (area 11, "Calling"; chunk 7.2c):
//! [`NativeFetch`] sends a provider call's request over a socket of its
//! own, HTTPS through OpenSSL with the certificate and the host checked,
//! or plain HTTP to this machine, which is how a test reaches its server.
//! Each exchange runs on tokio's blocking pool, so the worker's thread
//! goes on serving while a provider answers, and is given up once the
//! call's timeout passes.

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use jadpo_drivers::package::{Fetch, FetchError};
use jadpo_runtime::LocalFuture;
use jadpo_runtime::provider::{MOST_ANSWER, Request, Response};

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
                Err(_) => Err(FetchError::NoAnswer("no answer in time".into())),
            }
        })
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
    let raw = if target.secure {
        let connector = openssl::ssl::SslConnector::builder(openssl::ssl::SslMethod::tls_client())
            .map_err(|error| FetchError::NotSent(error.to_string()))?
            .build();
        let tls = connector
            .connect(bare, stream)
            .map_err(|error| FetchError::NotSent(format!("no secure connection: {error}")))?;
        send(tls, &bytes)?
    } else {
        send(stream, &bytes)?
    };
    parse(&raw).ok_or_else(|| FetchError::NoAnswer("an answer that isn't HTTP".into()))
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

    /// Nothing listening is a call that never left; a server that never
    /// answers is one that may have acted.
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
        assert!(
            matches!(unanswered, Err(FetchError::NoAnswer(_))),
            "{unanswered:?}"
        );
        drop(silent);
    }
}
