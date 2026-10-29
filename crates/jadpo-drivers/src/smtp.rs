//! The SMTP driver (area 16, "Email"; chunk 7.3): the `email` capability's
//! built-in driver, `driver: Driver.smtp`. The runtime hands it a message
//! Jadpo rendered whole ([`jadpo_std::email::MESSAGE_FIELDS`]); the driver
//! writes it as one MIME message ([`compose`]) and hands it to the
//! server in an SMTP conversation ([`relay`]: `EHLO`, `STARTTLS`, `AUTH`,
//! the message) over the host's [`Transport`], which gives only the
//! socket and TLS over it (tokio and rustls natively, Workers' `connect()`
//! there; chunk 8.1b), then classifies what happened into the seam's
//! fixed set:
//!
//! - accepted: sent;
//! - never handed over (nothing listened, a refusal for now, a server that
//!   dropped the connection before the message's end went out): surely
//!   did nothing, so `unavailable`, and the delivery is retried;
//! - the message's end went out and no answer came back: it may have
//!   acted, so `outcome_unknown`, and the delivery is parked for an
//!   operator, never sent again blindly, since SMTP takes no key;
//! - the recipient or the message refused for good: the failure
//!   `recipient_rejected` or `message_rejected`;
//! - the credential, the sender, the server's lack of TLS or its
//!   certificate refused: `misconfigured`.
//!
//! A message is written with CRLF line ends, every part base64, the
//! subject as RFC 2047 encoded words when it isn't plain ASCII, and a
//! `Date:` from the host's clock when it has one; the server adds the
//! `Message-ID:` (RFC 6409, section 8.3).

use std::cell::Cell;
use std::net::IpAddr;
use std::rc::Rc;

use jadpo_runtime::LocalFuture;
use jadpo_runtime::capability::{Answer, Call, Capability};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::{Endpoint, Stream, Transport, within};
use jadpo_std::email::{MESSAGE_FIELDS, RECIPIENT_REJECTED};
use jadpo_value::Value;

pub use jadpo_std::email::MESSAGE_REJECTED;

/// The server a connection with `driver: Driver.smtp` reaches, and how it
/// signs in.
#[derive(Clone, PartialEq, Eq)]
pub struct Server {
    pub host: String,
    pub port: u16,
    /// `credential: { username: password: }`, both or neither.
    pub username: Option<String>,
    pub password: Option<String>,
    /// How long one exchange may take, in milliseconds.
    pub timeout_ms: u64,
}

impl std::fmt::Debug for Server {
    /// Never its password.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Server")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("username", &self.username)
            .finish_non_exhaustive()
    }
}

impl Server {
    /// How long an exchange waits when the connection says nothing: half
    /// a minute.
    pub const TIMEOUT_MS: u64 = 30_000;

    pub fn new(host: impl Into<String>, port: u16) -> Server {
        Server {
            host: host.into(),
            port,
            username: None,
            password: None,
            timeout_ms: Server::TIMEOUT_MS,
        }
    }

    /// Signing in as `username` with `password`.
    pub fn signing_in(
        mut self,
        username: impl Into<String>,
        password: impl Into<String>,
    ) -> Server {
        self.username = Some(username.into());
        self.password = Some(password.into());
        self
    }

    /// Whether it is on this machine, the one place a message and a
    /// credential may go without TLS.
    pub fn is_local(&self) -> bool {
        matches!(
            self.host.trim_start_matches('[').trim_end_matches(']'),
            "localhost" | "127.0.0.1" | "::1"
        )
    }
}

/// One message as it is handed to a server: who it is from and to, for
/// the envelope, and its text, with CRLF line ends.
#[derive(Clone, PartialEq, Eq)]
pub struct Envelope {
    pub from: String,
    pub to: Vec<String>,
    pub data: Vec<u8>,
}

impl std::fmt::Debug for Envelope {
    /// Never its text, nor its addresses, which are personal data.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Envelope")
            .field("to", &self.to.len())
            .field("bytes", &self.data.len())
            .finish_non_exhaustive()
    }
}

/// Where in the exchange a server refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// Signing in, `AUTH`.
    SignIn,
    /// The sender, `MAIL FROM`.
    Sender,
    /// A recipient, `RCPT TO`.
    Rcpt,
    /// The message, after its end went out.
    Message,
}

/// Why a message wasn't handed over.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RelayError {
    /// It never went: nothing listened, the server refused for now (a
    /// `4xx` reply), or the connection dropped before the message's end
    /// was sent. Safe to send again.
    NotSent(String),
    /// The message's end went out and no answer came back: it may have
    /// been accepted.
    NoAnswer(String),
    /// The server refused for good, with its `5xx` reply's code, at
    /// `phase`.
    Refused { phase: Phase, code: u16 },
    /// It won't go this way: a server that offers no TLS and isn't on this
    /// machine, or a certificate or name that fails the check.
    Insecure(String),
}

/// The port a server speaks TLS on from the start.
pub const IMPLICIT_TLS: u16 = 465;

/// The longest reply line a server is read for.
const MOST_LINE: usize = 4096;

/// The SMTP driver for one connection's server, over the host's
/// [`Transport`]: the host gives the socket, TLS over it, a timer and the
/// clock; the conversation is the driver's.
pub struct Smtp {
    transport: Rc<dyn Transport>,
    server: Server,
}

impl std::fmt::Debug for Smtp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Smtp")
            .field("server", &self.server)
            .finish_non_exhaustive()
    }
}

impl Smtp {
    pub fn new(transport: Rc<dyn Transport>, server: Server) -> Smtp {
        Smtp { transport, server }
    }
}

impl Capability for Smtp {
    fn call<'a>(&'a self, call: &'a Call) -> LocalFuture<'a, Answer> {
        Box::pin(async move {
            if call.operation != jadpo_std::email::SEND {
                return Answer::Misconfigured;
            }
            let envelope = match compose(&call.input, self.transport.now_ms()) {
                Ok(envelope) => envelope,
                // A message no server can take as written: an address
                // only SMTPUTF8 carries.
                Err(Unsendable::Address) => return Answer::Failed(RECIPIENT_REJECTED.to_string()),
                Err(Unsendable::Shape) => return Answer::Misconfigured,
            };
            answer(relay(&*self.transport, &self.server, &envelope).await)
        })
    }
}

/// What happened to a message, as the seam's answer.
fn answer(relayed: Result<(), RelayError>) -> Answer {
    match relayed {
        Ok(()) => Answer::Value(Value::Unit),
        Err(RelayError::NotSent(_)) => Answer::Unavailable,
        Err(RelayError::NoAnswer(_)) => Answer::Unknown,
        Err(RelayError::Refused {
            phase: Phase::Rcpt, ..
        }) => Answer::Failed(RECIPIENT_REJECTED.to_string()),
        Err(RelayError::Refused {
            phase: Phase::Message,
            ..
        }) => Answer::Failed(MESSAGE_REJECTED.to_string()),
        Err(RelayError::Refused { .. } | RelayError::Insecure(_)) => Answer::Misconfigured,
    }
}

/// Hands `envelope` to `server` over `transport`: connect, greet, secure,
/// sign in, and hand the message over. Each wait is bounded by the
/// server's timeout, and the whole conversation by four of them.
pub async fn relay(
    transport: &dyn Transport,
    server: &Server,
    envelope: &Envelope,
) -> Result<(), RelayError> {
    let timeout = server.timeout_ms.max(1);
    // Set once the message's end may have left: from then on, no answer
    // means it may have been accepted.
    let ended = Cell::new(false);
    let talked = within(
        transport,
        timeout.saturating_mul(4),
        conversation(transport, server, envelope, timeout, &ended),
    )
    .await;
    match talked {
        Some(done) => done,
        None if ended.get() => Err(RelayError::NoAnswer("no answer in time".into())),
        None => Err(RelayError::NotSent("no answer in time".into())),
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

/// One SMTP conversation, its reads and writes each bounded by `timeout`
/// milliseconds.
struct Talk<'t> {
    transport: &'t dyn Transport,
    timeout: u64,
    link: Box<dyn Stream>,
}

impl Talk<'_> {
    fn late(what: &str) -> std::io::Error {
        std::io::Error::new(std::io::ErrorKind::TimedOut, format!("no {what} in time"))
    }

    /// Writes `bytes` whole.
    async fn write(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        let (transport, timeout) = (self.transport, self.timeout);
        let link = &mut self.link;
        within(transport, timeout, async move {
            link.write_all(bytes).await?;
            link.flush().await
        })
        .await
        .unwrap_or_else(|| Err(Talk::late("write")))
    }

    async fn send(&mut self, line: &str) -> Result<(), RelayError> {
        let mut bytes = Vec::with_capacity(line.len() + 2);
        bytes.extend_from_slice(line.as_bytes());
        bytes.extend_from_slice(b"\r\n");
        self.write(&bytes)
            .await
            .map_err(|error| RelayError::NotSent(error.to_string()))
    }

    /// The next reply, its lines joined: `250-...` lines continue it.
    async fn reply(&mut self) -> std::io::Result<Reply> {
        let (transport, timeout) = (self.transport, self.timeout);
        let link = &mut self.link;
        within(transport, timeout, async move {
            let mut lines = Vec::new();
            loop {
                let line = Talk::line(link).await?;
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
        })
        .await
        .unwrap_or_else(|| Err(Talk::late("answer")))
    }

    /// One line, read a byte at a time so nothing past it is taken from a
    /// connection that may be upgraded to TLS next: whatever a server (or
    /// someone between) sends after its `STARTTLS` reply is read as TLS,
    /// never as the conversation.
    async fn line(link: &mut Box<dyn Stream>) -> std::io::Result<String> {
        let mut bytes = Vec::new();
        let mut byte = [0u8; 1];
        loop {
            if link.read(&mut byte).await? == 0 {
                return Err(std::io::Error::other("the server closed the connection"));
            }
            bytes.extend_from_slice(&byte);
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
    async fn command(&mut self, line: &str, class: u16, phase: Phase) -> Result<Reply, RelayError> {
        self.send(line).await?;
        let reply = self
            .reply()
            .await
            .map_err(|error| RelayError::NotSent(error.to_string()))?;
        if reply.is(class) {
            Ok(reply)
        } else {
            Err(reply.refused(phase))
        }
    }
}

/// `stream` as TLS to `name`, through the host: a certificate or name the
/// check refuses is [`RelayError::Insecure`]; a connection that broke off
/// took nothing.
async fn secured(
    transport: &dyn Transport,
    stream: Box<dyn Stream>,
    name: &str,
    timeout: u64,
) -> Result<Box<dyn Stream>, RelayError> {
    use std::io::ErrorKind;
    match within(
        transport,
        timeout,
        transport.upgrade(stream, name, crate::Trust::Roots),
    )
    .await
    {
        Some(Ok(secure)) => Ok(secure),
        Some(Err(error))
            if matches!(
                error.kind(),
                ErrorKind::InvalidData | ErrorKind::InvalidInput | ErrorKind::Unsupported
            ) =>
        {
            Err(RelayError::Insecure(format!(
                "no secure connection: {error}"
            )))
        }
        Some(Err(error)) => Err(RelayError::NotSent(format!(
            "no secure connection: {error}"
        ))),
        None => Err(RelayError::NotSent("no secure connection in time".into())),
    }
}

/// The conversation itself (area 16, "The SMTP driver"): TLS always, by
/// `STARTTLS` or from the start on port 465, but to this machine, where
/// `STARTTLS` isn't asked for; a credential never in the clear elsewhere.
async fn conversation(
    transport: &dyn Transport,
    server: &Server,
    envelope: &Envelope,
    timeout: u64,
    ended: &Cell<bool>,
) -> Result<(), RelayError> {
    let name = server.host.trim_start_matches('[').trim_end_matches(']');
    let endpoint = Endpoint::Tcp {
        host: name.to_string(),
        port: server.port,
    };
    let stream = within(transport, timeout, transport.connect(&endpoint))
        .await
        .ok_or_else(|| RelayError::NotSent("no connection in time".into()))?
        .map_err(|error| RelayError::NotSent(format!("no connection: {error}")))?;
    // EHLO names this end by its address (RFC 5321, section 4.1.3).
    let me = match transport.local_address(&*stream) {
        Some(IpAddr::V4(ip)) => format!("[{ip}]"),
        Some(IpAddr::V6(ip)) => format!("[IPv6:{ip}]"),
        None => "[127.0.0.1]".to_string(),
    };
    let mut secure = server.port == IMPLICIT_TLS;
    let link = if secure {
        secured(transport, stream, name, timeout).await?
    } else {
        stream
    };
    let mut talk = Talk {
        transport,
        timeout,
        link,
    };
    let opening = talk
        .reply()
        .await
        .map_err(|error| RelayError::NotSent(error.to_string()))?;
    if !opening.is(2) {
        return Err(RelayError::NotSent(format!(
            "the server answered {}",
            opening.code
        )));
    }
    let ehlo = talk
        .command(&format!("EHLO {me}"), 2, Phase::Sender)
        .await?;
    // A server on this machine is spoken to as it is, as PostgreSQL's is:
    // a stock Postfix offers `STARTTLS` with a self-signed certificate,
    // which no check would pass.
    if !secure && !server.is_local() {
        let offers_tls = ehlo
            .lines
            .iter()
            .any(|line| line.eq_ignore_ascii_case("STARTTLS"));
        if offers_tls {
            talk.command("STARTTLS", 2, Phase::Sender).await?;
            talk.link = secured(transport, talk.link, name, timeout).await?;
            secure = true;
            talk.command(&format!("EHLO {me}"), 2, Phase::Sender)
                .await?;
        } else {
            let _ = talk.send("QUIT").await;
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
        talk.command(&format!("AUTH PLAIN {plain}"), 2, Phase::SignIn)
            .await?;
    }
    talk.command(&format!("MAIL FROM:<{}>", envelope.from), 2, Phase::Sender)
        .await?;
    for to in &envelope.to {
        talk.command(&format!("RCPT TO:<{to}>"), 2, Phase::Rcpt)
            .await?;
    }
    talk.command("DATA", 3, Phase::Message).await?;
    let data = dot_stuffed(&envelope.data);
    // Once the message's end may have left, a lost answer may be a
    // message accepted.
    ended.set(true);
    talk.write(&data)
        .await
        .map_err(|error| RelayError::NoAnswer(error.to_string()))?;
    let accepted = talk
        .reply()
        .await
        .map_err(|error| RelayError::NoAnswer(error.to_string()))?;
    if !accepted.is(2) {
        return Err(accepted.refused(Phase::Message));
    }
    let _ = talk.send("QUIT").await;
    Ok(())
}

/// Why a message can't be written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unsendable {
    /// An address with characters plain SMTP can't carry.
    Address,
    /// Not a message the runtime makes.
    Shape,
}

/// `message`, the record the runtime sends, as one MIME message: its
/// envelope's addresses and its text. `now_ms` dates it.
pub fn compose(message: &Value, now_ms: Option<i64>) -> Result<Envelope, Unsendable> {
    let Value::Record(record) = message else {
        return Err(Unsendable::Shape);
    };
    let text = |name: &str| match record.get(name) {
        Some(Value::Text(text)) => Some(text.to_string()),
        _ => None,
    };
    let [from, to, subject, html, plain, attachments] = MESSAGE_FIELDS;
    let from = text(from).ok_or(Unsendable::Shape)?;
    let to = text(to).ok_or(Unsendable::Shape)?;
    for address in [&from, &to] {
        if !address.is_ascii()
            || address
                .chars()
                .any(|c| c.is_control() || c.is_whitespace() || c == '<' || c == '>')
        {
            return Err(Unsendable::Address);
        }
    }
    let subject = text(subject).unwrap_or_default();
    let html = text(html);
    let plain = text(plain);
    let attachments: Vec<(String, String, Vec<u8>)> = match record.get(attachments) {
        Some(Value::List(items)) => items
            .iter()
            .map(|item| {
                let Value::Record(item) = item else {
                    return Err(Unsendable::Shape);
                };
                let name = match item.get("name") {
                    Some(Value::Text(name)) => name.to_string(),
                    _ => return Err(Unsendable::Shape),
                };
                let kind = match item.get("content_type") {
                    Some(Value::Text(kind)) => kind.to_string(),
                    _ => return Err(Unsendable::Shape),
                };
                let bytes = match item.get("content") {
                    Some(Value::Bytes(bytes)) => bytes.to_vec(),
                    _ => return Err(Unsendable::Shape),
                };
                Ok((name, kind, bytes))
            })
            .collect::<Result<_, _>>()?,
        _ => Vec::new(),
    };
    let mut out = String::new();
    if let Some(now) = now_ms {
        out.push_str(&format!("Date: {}\r\n", date(now)));
    }
    out.push_str(&format!("From: {from}\r\n"));
    out.push_str(&format!("To: {to}\r\n"));
    out.push_str(&format!("Subject: {}\r\n", encoded_words(&subject)));
    out.push_str("MIME-Version: 1.0\r\n");
    let seed = &[
        subject.as_str(),
        html.as_deref().unwrap_or(""),
        plain.as_deref().unwrap_or(""),
    ];
    let mut boundaries = Boundaries::new(seed, &attachments);
    let body = |out: &mut String, boundaries: &mut Boundaries| match (&plain, &html) {
        (Some(plain), Some(html)) => {
            let boundary = boundaries.next();
            out.push_str(&format!(
                "Content-Type: multipart/alternative; boundary=\"{boundary}\"\r\n\r\n"
            ));
            out.push_str(&format!("--{boundary}\r\n"));
            part(out, "text/plain; charset=utf-8", None, plain.as_bytes());
            out.push_str(&format!("--{boundary}\r\n"));
            part(out, "text/html; charset=utf-8", None, html.as_bytes());
            out.push_str(&format!("--{boundary}--\r\n"));
        }
        (Some(plain), None) => part(out, "text/plain; charset=utf-8", None, plain.as_bytes()),
        (None, Some(html)) => part(out, "text/html; charset=utf-8", None, html.as_bytes()),
        (None, None) => part(out, "text/plain; charset=utf-8", None, b""),
    };
    if attachments.is_empty() {
        body(&mut out, &mut boundaries);
    } else {
        let boundary = boundaries.next();
        out.push_str(&format!(
            "Content-Type: multipart/mixed; boundary=\"{boundary}\"\r\n\r\n"
        ));
        out.push_str(&format!("--{boundary}\r\n"));
        body(&mut out, &mut boundaries);
        for (name, kind, bytes) in &attachments {
            out.push_str(&format!("--{boundary}\r\n"));
            part(&mut out, kind, Some(name), bytes);
        }
        out.push_str(&format!("--{boundary}--\r\n"));
    }
    Ok(Envelope {
        from,
        to: vec![to],
        data: out.into_bytes(),
    })
}

/// One part: its headers, a blank line and its bytes in base64, 76
/// characters to a line.
fn part(out: &mut String, kind: &str, file: Option<&str>, bytes: &[u8]) {
    let kind: String = kind.chars().filter(|c| !c.is_control()).collect();
    out.push_str(&format!("Content-Type: {kind}\r\n"));
    out.push_str("Content-Transfer-Encoding: base64\r\n");
    if let Some(file) = file {
        out.push_str(&format!(
            "Content-Disposition: attachment; filename*=utf-8''{}\r\n",
            percent(file)
        ));
    }
    out.push_str("\r\n");
    let encoded = base64(bytes);
    for line in encoded.as_bytes().chunks(76) {
        out.push_str(std::str::from_utf8(line).unwrap_or_default());
        out.push_str("\r\n");
    }
}

/// Boundaries that appear nowhere in the message's own text.
struct Boundaries {
    texts: Vec<String>,
    count: u32,
}

impl Boundaries {
    fn new(texts: &[&str], attachments: &[(String, String, Vec<u8>)]) -> Boundaries {
        let mut all: Vec<String> = texts.iter().map(|text| text.to_string()).collect();
        all.extend(
            attachments
                .iter()
                .map(|(name, kind, _)| format!("{name}{kind}")),
        );
        Boundaries {
            texts: all,
            count: 0,
        }
    }

    fn next(&mut self) -> String {
        loop {
            self.count += 1;
            let boundary = format!("jadpo-part-{:04}", self.count);
            if !self.texts.iter().any(|text| text.contains(&boundary)) {
                return boundary;
            }
        }
    }
}

/// A subject as RFC 2047 encoded words when it isn't printable ASCII; on
/// one line either way.
pub fn encoded_words(subject: &str) -> String {
    let line: String = subject
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    if line.chars().all(|c| c.is_ascii() && !c.is_control()) && !line.contains("=?") {
        return line;
    }
    // At most 45 bytes in each word, so each stays within 75 characters,
    // never splitting a character.
    let mut words = Vec::new();
    let mut chunk = String::new();
    for c in line.chars() {
        if chunk.len() + c.len_utf8() > 45 {
            words.push(format!("=?utf-8?B?{}?=", base64(chunk.as_bytes())));
            chunk.clear();
        }
        chunk.push(c);
    }
    if !chunk.is_empty() {
        words.push(format!("=?utf-8?B?{}?=", base64(chunk.as_bytes())));
    }
    words.join("\r\n ")
}

/// A file name as RFC 2231 writes it: every byte outside a few safe ones
/// percent-encoded.
fn percent(name: &str) -> String {
    let mut out = String::new();
    for byte in name.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_') {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// Standard base64, padded.
pub fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk.first().copied().unwrap_or(0),
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        let at = |shift: u32| {
            char::from(
                TABLE
                    .get(((n >> shift) & 63) as usize)
                    .copied()
                    .unwrap_or(b'A'),
            )
        };
        out.push(at(18));
        out.push(at(12));
        out.push(if chunk.len() > 1 { at(6) } else { '=' });
        out.push(if chunk.len() > 2 { at(0) } else { '=' });
    }
    out
}

/// RFC 5322's date, in UTC: `Tue, 20 Oct 2026 09:00:00 +0000`.
pub fn date(ms: i64) -> String {
    let seconds = ms.div_euclid(1000);
    let days = seconds.div_euclid(86_400);
    let of_day = seconds.rem_euclid(86_400);
    // Howard Hinnant's civil-from-days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    const DAYS: [&str; 7] = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"];
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let weekday = DAYS
        .get(days.rem_euclid(7) as usize)
        .copied()
        .unwrap_or("Thu");
    let month_name = MONTHS.get((month - 1) as usize).copied().unwrap_or("Jan");
    format!(
        "{weekday}, {day:02} {month_name} {year} {:02}:{:02}:{:02} +0000",
        of_day / 3600,
        (of_day / 60) % 60,
        of_day % 60
    )
}

/// The lines of `data` as they go after `DATA`: each line starting with a
/// `.` given another (RFC 5321, section 4.5.2), every line ending CRLF,
/// then the line holding only `.`.
pub fn dot_stuffed(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + 8);
    let mut at_start = true;
    let mut previous = 0u8;
    for &byte in data {
        if byte == b'\n' && previous != b'\r' {
            out.push(b'\r');
        }
        if at_start && byte == b'.' {
            out.push(b'.');
        }
        out.push(byte);
        at_start = byte == b'\n';
        previous = byte;
    }
    if !out.ends_with(b"\r\n") {
        out.extend_from_slice(b"\r\n");
    }
    out.extend_from_slice(b".\r\n");
    out
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use super::*;

    fn message(html: Option<&str>, text: Option<&str>, attachments: Vec<Value>) -> Value {
        let given = |value: Option<&str>| value.map_or(Value::None, Value::text);
        Value::record(
            "EmailMessage",
            vec![
                (Rc::from("from"), Value::text("hello@acme.test")),
                (Rc::from("to"), Value::text("ana@example.test")),
                (Rc::from("subject"), Value::text("Welcome, Ana")),
                (Rc::from("html"), given(html)),
                (Rc::from("text"), given(text)),
                (Rc::from("attachments"), Value::List(attachments.into())),
            ],
        )
    }

    fn written(envelope: &Envelope) -> String {
        String::from_utf8(envelope.data.clone()).expect("text")
    }

    #[test]
    fn base64_and_dates_are_the_standard_ones() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        // 2026-10-20T09:00:00Z, a Tuesday.
        assert_eq!(date(1_792_486_800_000), "Tue, 20 Oct 2026 09:00:00 +0000");
        assert_eq!(date(0), "Thu, 01 Jan 1970 00:00:00 +0000");
    }

    #[test]
    fn a_message_with_both_bodies_is_one_alternative() {
        let envelope = compose(&message(Some("<p>Hi</p>"), Some("Hi"), Vec::new()), Some(0))
            .expect("composed");
        assert_eq!(envelope.from, "hello@acme.test");
        assert_eq!(envelope.to, vec!["ana@example.test".to_string()]);
        let text = written(&envelope);
        assert!(text.starts_with(
            "Date: Thu, 01 Jan 1970 00:00:00 +0000\r\nFrom: hello@acme.test\r\nTo: ana@example.test\r\nSubject: Welcome, Ana\r\nMIME-Version: 1.0\r\nContent-Type: multipart/alternative; boundary=\"jadpo-part-0001\"\r\n"
        ), "{text}");
        assert!(text.contains("Content-Type: text/plain; charset=utf-8\r\nContent-Transfer-Encoding: base64\r\n\r\nSGk=\r\n"));
        assert!(text.contains(&format!("\r\n{}\r\n", base64(b"<p>Hi</p>"))));
        assert!(text.ends_with("--jadpo-part-0001--\r\n"));
        assert!(
            !text.contains('\n')
                || text
                    .split('\n')
                    .all(|line| line.is_empty() || line.ends_with('\r'))
        );
    }

    #[test]
    fn attachments_go_beside_the_body() {
        let attachment = Value::record(
            "EmailAttachment",
            vec![
                (Rc::from("name"), Value::text("receipt 1.pdf")),
                (Rc::from("content_type"), Value::text("application/pdf")),
                (Rc::from("content"), Value::Bytes(Rc::from(&b"%PDF"[..]))),
            ],
        );
        let text = written(
            &compose(&message(None, Some("Hi"), vec![attachment]), None).expect("composed"),
        );
        assert!(!text.starts_with("Date:"));
        assert!(text.contains("Content-Type: multipart/mixed; boundary=\"jadpo-part-0001\""));
        assert!(
            text.contains("Content-Disposition: attachment; filename*=utf-8''receipt%201.pdf\r\n")
        );
        assert!(text.contains(&base64(b"%PDF")));
    }

    #[test]
    fn a_subject_outside_ascii_is_encoded_words() {
        assert_eq!(encoded_words("Hello"), "Hello");
        assert_eq!(
            encoded_words("Café"),
            format!("=?utf-8?B?{}?=", base64("Café".as_bytes()))
        );
        assert_eq!(encoded_words("a\r\nBcc: x"), "a  Bcc: x");
        let long = "é".repeat(40);
        let words = encoded_words(&long);
        assert!(words.split("\r\n ").all(|word| word.len() <= 75), "{words}");
    }

    #[test]
    fn data_is_dot_stuffed_and_ended() {
        assert_eq!(
            dot_stuffed(b".a\r\nb\n.c"),
            b"..a\r\nb\r\n..c\r\n.\r\n".to_vec()
        );
    }

    #[test]
    fn what_a_server_did_is_classified() {
        assert_eq!(answer(Ok(())), Answer::Value(Value::Unit));
        assert_eq!(
            answer(Err(RelayError::NotSent("refused".into()))),
            Answer::Unavailable
        );
        assert_eq!(
            answer(Err(RelayError::NoAnswer("dropped".into()))),
            Answer::Unknown
        );
        assert_eq!(
            answer(Err(RelayError::Refused {
                phase: Phase::Rcpt,
                code: 550
            })),
            Answer::Failed(RECIPIENT_REJECTED.into())
        );
        assert_eq!(
            answer(Err(RelayError::Refused {
                phase: Phase::Message,
                code: 554
            })),
            Answer::Failed(MESSAGE_REJECTED.into())
        );
        assert_eq!(
            answer(Err(RelayError::Refused {
                phase: Phase::SignIn,
                code: 535
            })),
            Answer::Misconfigured
        );
        assert_eq!(
            answer(Err(RelayError::Insecure("no TLS".into()))),
            Answer::Misconfigured
        );
    }

    /// A host where nothing listens and time never passes.
    struct Nowhere;

    impl Transport for Nowhere {
        fn connect(&self, _: &Endpoint) -> LocalFuture<'_, std::io::Result<Box<dyn Stream>>> {
            Box::pin(async { Err(std::io::Error::from(std::io::ErrorKind::ConnectionRefused)) })
        }

        fn spawn(&self, _: LocalFuture<'static, ()>) {}

        fn sleep(&self, _: u64) -> LocalFuture<'_, ()> {
            Box::pin(std::future::pending())
        }
    }

    fn sent(input: Value) -> Answer {
        let smtp = Smtp::new(Rc::new(Nowhere), Server::new("127.0.0.1", 2525));
        let call = Call::new(
            "mail",
            "send",
            input,
            jadpo_std::subscriber::Repeat::Unkeyed,
        );
        let future = smtp.call(&call);
        let mut future = std::pin::pin!(future);
        match future
            .as_mut()
            .poll(&mut std::task::Context::from_waker(std::task::Waker::noop()))
        {
            std::task::Poll::Ready(answer) => answer,
            std::task::Poll::Pending => panic!("answered at once"),
        }
    }

    #[test]
    fn a_server_that_cant_be_reached_took_nothing_and_a_wrong_shape_is_misconfigured() {
        assert_eq!(
            sent(message(None, Some("Hi"), Vec::new())),
            Answer::Unavailable
        );
        assert_eq!(sent(Value::Unit), Answer::Misconfigured);
    }
}
