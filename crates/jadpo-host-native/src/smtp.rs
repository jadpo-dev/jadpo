//! SMTP for the email capability natively (area 16, "Email"; chunk 7.3):
//! [`NativeRelay`] hands a message to its server over a socket of its own,
//! upgraded with `STARTTLS` through OpenSSL with the certificate and the
//! host checked, or with TLS from the start on port 465. Without TLS it
//! sends only to this machine, which is how a test reaches its server; a
//! credential goes nowhere else in the clear. Each exchange runs on
//! tokio's blocking pool, so the worker's thread goes on serving while a
//! server answers, and is given up once the connection's time passes.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use jadpo_drivers::smtp::{Envelope, Phase, Relay, RelayError, Server, base64, dot_stuffed};
use jadpo_runtime::LocalFuture;

/// The port a server speaks TLS on from the start.
const IMPLICIT_TLS: u16 = 465;

/// The longest reply line a server is read for.
const MOST_LINE: usize = 4096;

/// SMTP natively.
#[derive(Clone, Copy, Debug, Default)]
pub struct NativeRelay;

impl Relay for NativeRelay {
    fn relay<'a>(
        &'a self,
        server: &'a Server,
        envelope: &'a Envelope,
    ) -> LocalFuture<'a, Result<(), RelayError>> {
        let server = server.clone();
        let envelope = envelope.clone();
        Box::pin(async move {
            let timeout = Duration::from_millis(server.timeout_ms.max(1));
            // Set once the message's end may have left: from then on, no
            // answer means it may have been accepted.
            let ended = Arc::new(AtomicBool::new(false));
            let marked = Arc::clone(&ended);
            let exchange =
                tokio::task::spawn_blocking(move || exchange(&server, &envelope, timeout, &marked));
            let lost = |what: &str| {
                if ended.load(Ordering::SeqCst) {
                    RelayError::NoAnswer(what.to_string())
                } else {
                    RelayError::NotSent(what.to_string())
                }
            };
            match tokio::time::timeout(timeout.saturating_mul(4), exchange).await {
                Ok(Ok(done)) => done,
                Ok(Err(_)) => Err(lost("the exchange stopped")),
                Err(_) => Err(lost("no answer in time")),
            }
        })
    }

    fn now_ms(&self) -> Option<i64> {
        let since = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?;
        i64::try_from(since.as_millis()).ok()
    }
}

/// A connection, in the clear or over TLS.
enum Link {
    Plain(TcpStream),
    Tls(Box<openssl::ssl::SslStream<TcpStream>>),
}

impl Read for Link {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Link::Plain(stream) => stream.read(buf),
            Link::Tls(stream) => stream.read(buf),
        }
    }
}

impl Write for Link {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            Link::Plain(stream) => stream.write(buf),
            Link::Tls(stream) => stream.write(buf),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Link::Plain(stream) => stream.flush(),
            Link::Tls(stream) => stream.flush(),
        }
    }
}

/// A server's reply: its code and its lines' text.
struct Reply {
    code: u16,
    lines: Vec<String>,
}

impl Reply {
    fn is(&self, class: u16) -> bool {
        self.code / 100 == class
    }

    /// What a refusal at `phase` is: for now, or for good.
    fn refused(&self, phase: Phase) -> RelayError {
        if self.is(5) {
            RelayError::Refused {
                phase,
                code: self.code,
            }
        } else {
            RelayError::NotSent(format!("the server answered {}", self.code))
        }
    }
}

/// One SMTP conversation, its reads and writes each bounded by `timeout`.
struct Talk {
    link: Link,
}

impl Talk {
    fn send(&mut self, line: &str) -> Result<(), RelayError> {
        self.link
            .write_all(line.as_bytes())
            .and_then(|()| self.link.write_all(b"\r\n"))
            .and_then(|()| self.link.flush())
            .map_err(|error| RelayError::NotSent(error.to_string()))
    }

    /// The next reply, its lines joined: `250-...` lines continue it.
    fn reply(&mut self) -> std::io::Result<Reply> {
        let mut lines = Vec::new();
        loop {
            let line = self.line()?;
            let code = line
                .get(..3)
                .and_then(|code| code.parse::<u16>().ok())
                .ok_or_else(|| std::io::Error::other("a reply that isn't SMTP"))?;
            let more = line.as_bytes().get(3) == Some(&b'-');
            lines.push(line.get(4..).unwrap_or_default().to_string());
            if !more {
                return Ok(Reply { code, lines });
            }
        }
    }

    /// One line, read a byte at a time so nothing past it is taken from a
    /// connection that may be upgraded to TLS next.
    fn line(&mut self) -> std::io::Result<String> {
        let mut bytes = Vec::new();
        let mut byte = [0u8; 1];
        loop {
            match self.link.read(&mut byte)? {
                0 => return Err(std::io::Error::other("the server closed the connection")),
                _ => bytes.push(byte[0]),
            }
            if bytes.ends_with(b"\r\n") {
                bytes.truncate(bytes.len() - 2);
                return Ok(String::from_utf8_lossy(&bytes).into_owned());
            }
            if bytes.len() > MOST_LINE {
                return Err(std::io::Error::other("a reply line too long"));
            }
        }
    }

    /// A command's reply, which must be in `class`, or why not, at `phase`.
    fn command(&mut self, line: &str, class: u16, phase: Phase) -> Result<Reply, RelayError> {
        self.send(line)?;
        let reply = self
            .reply()
            .map_err(|error| RelayError::NotSent(error.to_string()))?;
        if reply.is(class) {
            Ok(reply)
        } else {
            Err(reply.refused(phase))
        }
    }
}

/// The TLS connector every upgrade uses, checking the server's
/// certificate and name.
fn tls(host: &str, stream: TcpStream) -> Result<Link, RelayError> {
    let connector = openssl::ssl::SslConnector::builder(openssl::ssl::SslMethod::tls_client())
        .map_err(|error| RelayError::NotSent(error.to_string()))?
        .build();
    connector
        .connect(host, stream)
        .map(|stream| Link::Tls(Box::new(stream)))
        .map_err(|error| RelayError::Insecure(format!("no secure connection: {error}")))
}

/// One exchange, blocking: connect, greet, secure, sign in, and hand the
/// message over.
fn exchange(
    server: &Server,
    envelope: &Envelope,
    timeout: Duration,
    ended: &AtomicBool,
) -> Result<(), RelayError> {
    let bare = server.host.trim_start_matches('[').trim_end_matches(']');
    let addresses: Vec<SocketAddr> = (bare, server.port)
        .to_socket_addrs()
        .map_err(|_| RelayError::NotSent(format!("`{}` couldn't be found", server.host)))?
        .collect();
    let mut stream = None;
    let mut last = String::from("nothing to connect to");
    for address in addresses {
        match TcpStream::connect_timeout(&address, timeout) {
            Ok(connected) => {
                stream = Some(connected);
                break;
            }
            Err(error) => last = error.to_string(),
        }
    }
    let stream = stream.ok_or_else(|| RelayError::NotSent(format!("no connection: {last}")))?;
    let _ = stream.set_nodelay(true);
    stream
        .set_read_timeout(Some(timeout))
        .and_then(|()| stream.set_write_timeout(Some(timeout)))
        .map_err(|error| RelayError::NotSent(error.to_string()))?;
    // EHLO names this end by its address (RFC 5321, section 4.1.3).
    let me = match stream.local_addr() {
        Ok(SocketAddr::V4(at)) => format!("[{}]", at.ip()),
        Ok(SocketAddr::V6(at)) => format!("[IPv6:{}]", at.ip()),
        Err(_) => "[127.0.0.1]".to_string(),
    };
    let mut secure = server.port == IMPLICIT_TLS;
    let link = if secure {
        tls(
            bare,
            stream
                .try_clone()
                .map_err(|e| RelayError::NotSent(e.to_string()))?,
        )?
    } else {
        Link::Plain(
            stream
                .try_clone()
                .map_err(|e| RelayError::NotSent(e.to_string()))?,
        )
    };
    let mut talk = Talk { link };
    let opening = talk
        .reply()
        .map_err(|error| RelayError::NotSent(error.to_string()))?;
    if !opening.is(2) {
        return Err(RelayError::NotSent(format!(
            "the server answered {}",
            opening.code
        )));
    }
    let ehlo = talk.command(&format!("EHLO {me}"), 2, Phase::Sender)?;
    if !secure {
        let offers_tls = ehlo
            .lines
            .iter()
            .any(|line| line.eq_ignore_ascii_case("STARTTLS"));
        if offers_tls {
            talk.command("STARTTLS", 2, Phase::Sender)?;
            talk = Talk {
                link: tls(bare, stream)?,
            };
            secure = true;
            talk.command(&format!("EHLO {me}"), 2, Phase::Sender)?;
        } else if !server.is_local() {
            let _ = talk.send("QUIT");
            return Err(RelayError::Insecure(format!(
                "`{}` offers no TLS, and a message goes in the clear only to this machine",
                server.host
            )));
        }
    }
    if let (Some(username), Some(password)) = (&server.username, &server.password) {
        if !secure && !server.is_local() {
            return Err(RelayError::Insecure(
                "a credential sent in the clear".into(),
            ));
        }
        let plain = base64(format!("\0{username}\0{password}").as_bytes());
        talk.command(&format!("AUTH PLAIN {plain}"), 2, Phase::SignIn)?;
    }
    talk.command(&format!("MAIL FROM:<{}>", envelope.from), 2, Phase::Sender)?;
    for to in &envelope.to {
        talk.command(&format!("RCPT TO:<{to}>"), 2, Phase::Rcpt)?;
    }
    talk.command("DATA", 3, Phase::Message)?;
    let data = dot_stuffed(&envelope.data);
    // Once the message's end may have left, a lost answer may be a
    // message accepted.
    ended.store(true, Ordering::SeqCst);
    talk.link
        .write_all(&data)
        .and_then(|()| talk.link.flush())
        .map_err(|error| RelayError::NoAnswer(error.to_string()))?;
    let accepted = talk
        .reply()
        .map_err(|error| RelayError::NoAnswer(error.to_string()))?;
    if !accepted.is(2) {
        return Err(accepted.refused(Phase::Message));
    }
    let _ = talk.send("QUIT");
    Ok(())
}
