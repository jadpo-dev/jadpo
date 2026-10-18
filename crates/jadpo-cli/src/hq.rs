//! The client side of Jadpo HQ's protocol (area 14, "The protocol"; chunk
//! 5.5y): HTTPS and JSON under `/v1/`, a 5-second connect and a 15-second
//! wait for each answer, and up to three retries with backoff for a failed
//! connection, a timeout, `429` and HQ's own `5xx`, honouring
//! `Retry-After` up to a minute.
//!
//! It sends only what it is given, with `User-Agent: jadpo` and no
//! version, and logs nothing about a request. `http://` is accepted only
//! for a loopback host, which is how the tests reach their fake; anything
//! else is HTTPS through OpenSSL with the certificate and host checked.

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use serde_json::Value;

/// The variable that names HQ, and its default: none until the release
/// that launches HQ, with the registry after the first build, sets the
/// real address here (lead decision, 2026-10-18).
pub const VARIABLE: &str = "JADPO_SUGGEST_HQ";
pub const DEFAULT: Option<&str> = None;

/// HQ's address as written in the environment, or the default; `None`
/// while sharing has nowhere to go.
pub fn configured(environment: &jadpo_term::Environment) -> Option<&str> {
    environment
        .var(VARIABLE)
        .filter(|written| !written.is_empty())
        .or(DEFAULT)
}

const CONNECT: Duration = Duration::from_secs(5);
const ANSWER: Duration = Duration::from_secs(15);
/// The waits before each retry.
const BACKOFF: [Duration; 3] = [
    Duration::from_millis(500),
    Duration::from_secs(1),
    Duration::from_secs(2),
];
/// The longest `Retry-After` worth waiting for, in seconds.
const LONGEST_WAIT: u64 = 60;
/// The most of an answer read.
const MOST: usize = 1 << 20;

/// Where HQ is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Address {
    secure: bool,
    host: String,
    port: u16,
    /// Any path before `/v1/`, without a trailing `/`.
    prefix: String,
}

impl Address {
    /// The full address of one of the protocol's paths.
    pub fn url(&self, path: &str) -> String {
        let scheme = if self.secure { "https" } else { "http" };
        let default = if self.secure { 443 } else { 80 };
        let port = if self.port == default {
            String::new()
        } else {
            format!(":{}", self.port)
        };
        format!("{scheme}://{}{port}{}{path}", self.host, self.prefix)
    }
}

/// HQ's address from what was written, or why it can't be used.
pub fn address(written: &str) -> Result<Address, String> {
    let written = written.trim();
    let (secure, rest) = if let Some(rest) = written.strip_prefix("https://") {
        (true, rest)
    } else if let Some(rest) = written.strip_prefix("http://") {
        (false, rest)
    } else {
        return Err(format!(
            "`{written}` isn't an address: it starts with `https://`."
        ));
    };
    let (authority, path) = match rest.find('/') {
        Some(at) => rest.split_at(at),
        None => (rest, ""),
    };
    if authority.is_empty() || authority.contains('@') || path.contains(['?', '#']) {
        return Err(format!(
            "`{written}` isn't an address Jadpo sends to: a host, an optional port and path, nothing else."
        ));
    }
    let (host, port) = if let Some(inner) = authority.strip_prefix('[') {
        let (host, after) = inner
            .split_once(']')
            .ok_or_else(|| format!("`{written}` has an unclosed `[`."))?;
        (format!("[{host}]"), after.strip_prefix(':'))
    } else {
        match authority.rsplit_once(':') {
            Some((host, port)) => (host.to_string(), Some(port)),
            None => (authority.to_string(), None),
        }
    };
    let port = match port {
        Some(port) => port
            .parse::<u16>()
            .map_err(|_| format!("`{written}` has a port that isn't a number."))?,
        None if secure => 443,
        None => 80,
    };
    let loopback = matches!(
        host.to_lowercase().as_str(),
        "localhost" | "127.0.0.1" | "[::1]"
    );
    if !secure && !loopback {
        return Err(format!(
            "`{written}` is plain `http://`, which Jadpo sends to only on this machine; HQ's address starts with `https://`."
        ));
    }
    Ok(Address {
        secure,
        host,
        port,
        prefix: path.trim_end_matches('/').to_string(),
    })
}

/// Why an exchange failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Failure {
    /// Nothing answered, or not in time, after every try.
    Unreachable(String),
    /// HQ answered with an error.
    Refused {
        status: u16,
        code: String,
        message: String,
    },
    /// HQ answered with something that isn't the protocol.
    Garbled(String),
}

/// One answer.
struct Answer {
    status: u16,
    retry_after: Option<u64>,
    body: String,
}

/// Posts `body` to `path` and reads HQ's JSON answer, retrying what can
/// be retried.
pub fn post(address: &Address, path: &str, body: &str) -> Result<Value, Failure> {
    let mut tries = 0;
    loop {
        let wait = match once(address, path, body) {
            Ok(answer) if answer.status == 200 => {
                return serde_json::from_str(&answer.body)
                    .map_err(|_| Failure::Garbled("Jadpo HQ's answer isn't JSON.".to_string()));
            }
            Ok(answer) => {
                let retryable = matches!(answer.status, 429 | 500 | 502 | 503 | 504);
                let refused = refusal(&answer);
                if !retryable || tries == BACKOFF.len() {
                    return Err(refused);
                }
                match answer.retry_after {
                    Some(seconds) if seconds > LONGEST_WAIT => return Err(refused),
                    Some(seconds) => Duration::from_secs(seconds),
                    None => BACKOFF[tries],
                }
            }
            Err(why) => {
                if tries == BACKOFF.len() {
                    return Err(Failure::Unreachable(why));
                }
                BACKOFF[tries]
            }
        };
        tries += 1;
        std::thread::sleep(wait);
    }
}

/// HQ's error envelope as a failure.
fn refusal(answer: &Answer) -> Failure {
    let error = serde_json::from_str::<Value>(&answer.body)
        .ok()
        .and_then(|v| v.get("error").cloned());
    let field = |name: &str| {
        error
            .as_ref()
            .and_then(|e| e.get(name))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    Failure::Refused {
        status: answer.status,
        code: field("code"),
        message: field("message"),
    }
}

/// One try: connect, send, read the whole answer.
fn once(address: &Address, path: &str, body: &str) -> Result<Answer, String> {
    let host = address.host.trim_start_matches('[').trim_end_matches(']');
    let targets = (host, address.port)
        .to_socket_addrs()
        .map_err(|_| format!("`{}` couldn't be found.", address.host))?;
    let mut stream = None;
    let mut last = String::from("nothing to connect to");
    for target in targets {
        match TcpStream::connect_timeout(&target, CONNECT) {
            Ok(connected) => {
                stream = Some(connected);
                break;
            }
            Err(error) => last = error.to_string(),
        }
    }
    let stream = stream.ok_or_else(|| format!("No connection to `{}`: {last}.", address.host))?;
    stream
        .set_read_timeout(Some(ANSWER))
        .and_then(|()| stream.set_write_timeout(Some(ANSWER)))
        .map_err(|error| error.to_string())?;
    let port = if (address.secure && address.port == 443) || (!address.secure && address.port == 80)
    {
        String::new()
    } else {
        format!(":{}", address.port)
    };
    let request = format!(
        "POST {}{path} HTTP/1.1\r\nHost: {}{port}\r\nContent-Type: application/json\r\nAccept: application/json\r\nUser-Agent: jadpo\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        address.prefix,
        address.host,
        body.len()
    );
    let raw = if address.secure {
        let connector = openssl::ssl::SslConnector::builder(openssl::ssl::SslMethod::tls_client())
            .map_err(|error| error.to_string())?
            .build();
        let tls = connector
            .connect(host, stream)
            .map_err(|error| format!("No secure connection to `{}`: {error}.", address.host))?;
        exchange(tls, request.as_bytes())
    } else {
        exchange(stream, request.as_bytes())
    }
    .map_err(|error| format!("The exchange with `{}` broke off: {error}.", address.host))?;
    parse(&raw).ok_or_else(|| "Jadpo HQ's answer isn't HTTP.".to_string())
}

fn exchange<S: Read + Write>(mut stream: S, request: &[u8]) -> std::io::Result<Vec<u8>> {
    stream.write_all(request)?;
    stream.flush()?;
    let mut raw = Vec::new();
    let mut buffer = [0; 8192];
    loop {
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => {
                raw.extend_from_slice(&buffer[..n]);
                if raw.len() > MOST {
                    return Err(std::io::Error::other("the answer is too long"));
                }
                if complete(&raw) {
                    break;
                }
            }
            // A TLS peer that closes without notice has still sent what
            // it sent; parsing decides whether that is a whole answer.
            Err(_) if !raw.is_empty() => break,
            Err(error) => return Err(error),
        }
    }
    Ok(raw)
}

/// The end of the head, and the head's lines.
fn head(raw: &[u8]) -> Option<(usize, Vec<String>)> {
    let end = raw.windows(4).position(|w| w == b"\r\n\r\n")?;
    let text = std::str::from_utf8(&raw[..end]).ok()?;
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

fn parse(raw: &[u8]) -> Option<Answer> {
    let (start, lines) = head(raw)?;
    let mut status_line = lines.first()?.split_whitespace();
    if !status_line.next()?.starts_with("HTTP/1.") {
        return None;
    }
    let status = status_line.next()?.parse().ok()?;
    let rest = &raw[start..];
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
    Some(Answer {
        status,
        retry_after: header(&lines, "retry-after").and_then(|v| v.parse().ok()),
        body: String::from_utf8(body).ok()?,
    })
}

fn unchunk(mut rest: &[u8]) -> Option<Vec<u8>> {
    let mut body = Vec::new();
    loop {
        let end = rest.windows(2).position(|w| w == b"\r\n")?;
        let size = std::str::from_utf8(&rest[..end]).ok()?;
        let size = usize::from_str_radix(size.split(';').next()?.trim(), 16).ok()?;
        rest = &rest[end + 2..];
        if size == 0 {
            return Some(body);
        }
        body.extend_from_slice(rest.get(..size)?);
        rest = rest.get(size + 2..)?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_address_is_https_or_on_this_machine() {
        let hq = address("https://hq.example.test").expect("an https address");
        assert_eq!(
            hq.url("/v1/suggestions/hashes"),
            "https://hq.example.test/v1/suggestions/hashes"
        );
        let local = address("http://127.0.0.1:4100/").expect("loopback");
        assert_eq!(local.url("/v1/x"), "http://127.0.0.1:4100/v1/x");
        assert!(address("http://[::1]:9/").is_ok());
        assert!(address("http://localhost").is_ok());
        assert!(address("http://hq.example.test").is_err());
        assert!(address("https://user@hq.example.test").is_err());
        assert!(address("hq.example.test").is_err());
        assert!(address("https://hq.example.test:x").is_err());
    }

    #[test]
    fn an_answer_is_read_by_length_or_in_chunks() {
        let raw =
            b"HTTP/1.1 429 Too Many Requests\r\nRetry-After: 7\r\nContent-Length: 2\r\n\r\n{}";
        let answer = parse(raw).expect("parsed");
        assert_eq!(answer.status, 429);
        assert_eq!(answer.retry_after, Some(7));
        assert_eq!(answer.body, "{}");
        let chunked =
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\n{\"a\r\n4\r\n\":1}\r\n0\r\n\r\n";
        assert_eq!(parse(chunked).expect("parsed").body, "{\"a\":1}");
        assert!(parse(b"nonsense").is_none());
    }
}
