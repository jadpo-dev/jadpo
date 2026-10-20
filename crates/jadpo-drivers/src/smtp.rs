//! The SMTP driver (area 16, "Email"; chunk 7.3): the `email` capability's
//! built-in driver, `driver: Driver.smtp`. The runtime hands it a message
//! Jadpo rendered whole ([`jadpo_std::email::MESSAGE_FIELDS`]); the driver
//! writes it as one MIME message ([`compose`]) and has the host's
//! [`Relay`] hand it to the server (tokio and OpenSSL natively, Workers'
//! `connect()` there), then classifies what happened into the seam's
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
//! - the credential, the sender or the server's lack of TLS refused:
//!   `misconfigured`.
//!
//! A message is written with CRLF line ends, every part base64, the
//! subject as RFC 2047 encoded words when it isn't plain ASCII, and a
//! `Date:` from the host's clock when it has one; the server adds the
//! `Message-ID:` (RFC 6409, section 8.3).

use jadpo_runtime::LocalFuture;
use jadpo_runtime::capability::{Answer, Call, Capability};
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
    /// The host won't send it this way: a server that offers no TLS and
    /// isn't on this machine, or no TLS where it was asked for.
    Insecure(String),
}

/// How a host hands a message to an SMTP server: what the SMTP driver
/// needs of it, as [`crate::package::Fetch`] is what the package driver
/// needs.
pub trait Relay {
    /// Hands `envelope` to `server`, giving up after `server.timeout_ms`.
    fn relay<'a>(
        &'a self,
        server: &'a Server,
        envelope: &'a Envelope,
    ) -> LocalFuture<'a, Result<(), RelayError>>;

    /// The time now, in milliseconds since 1970-01-01T00:00:00Z, for the
    /// message's `Date:`; `None` on a host with no clock.
    fn now_ms(&self) -> Option<i64> {
        None
    }
}

/// The SMTP driver over `R`, for one connection's server.
#[derive(Debug)]
pub struct Smtp<R> {
    relay: R,
    server: Server,
}

impl<R: Relay> Smtp<R> {
    pub fn new(relay: R, server: Server) -> Smtp<R> {
        Smtp { relay, server }
    }
}

impl<R: Relay> Capability for Smtp<R> {
    fn call<'a>(&'a self, call: &'a Call) -> LocalFuture<'a, Answer> {
        Box::pin(async move {
            if call.operation != jadpo_std::email::SEND {
                return Answer::Misconfigured;
            }
            let envelope = match compose(&call.input, self.relay.now_ms()) {
                Ok(envelope) => envelope,
                // A message no server can take as written: an address
                // only SMTPUTF8 carries.
                Err(Unsendable::Address) => return Answer::Failed(RECIPIENT_REJECTED.to_string()),
                Err(Unsendable::Shape) => return Answer::Misconfigured,
            };
            match self.relay.relay(&self.server, &envelope).await {
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
        })
    }
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
    use std::cell::RefCell;
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

    /// A relay answering from a script, keeping what it was handed.
    struct Scripted {
        answer: RefCell<Option<Result<(), RelayError>>>,
        sent: RefCell<Vec<Envelope>>,
    }

    impl Relay for Scripted {
        fn relay<'a>(
            &'a self,
            _: &'a Server,
            envelope: &'a Envelope,
        ) -> LocalFuture<'a, Result<(), RelayError>> {
            self.sent.borrow_mut().push(envelope.clone());
            let answer = self.answer.borrow_mut().take().unwrap_or(Ok(()));
            Box::pin(async move { answer })
        }
    }

    fn answered(answer: Result<(), RelayError>, input: Value) -> Answer {
        let relay = Scripted {
            answer: RefCell::new(Some(answer)),
            sent: RefCell::new(Vec::new()),
        };
        let smtp = Smtp::new(relay, Server::new("127.0.0.1", 2525));
        let call = Call::new(
            "mail",
            "send",
            input,
            jadpo_std::subscriber::Repeat::Unkeyed,
        );
        let mut answered = None;
        let future = smtp.call(&call);
        let mut future = std::pin::pin!(future);
        let waker = std::task::Waker::noop();
        if let std::task::Poll::Ready(answer) = future
            .as_mut()
            .poll(&mut std::task::Context::from_waker(waker))
        {
            answered = Some(answer);
        }
        answered.expect("answered at once")
    }

    #[test]
    fn what_a_server_did_is_classified() {
        let input = || message(None, Some("Hi"), Vec::new());
        assert_eq!(answered(Ok(()), input()), Answer::Value(Value::Unit));
        assert_eq!(
            answered(Err(RelayError::NotSent("refused".into())), input()),
            Answer::Unavailable
        );
        assert_eq!(
            answered(Err(RelayError::NoAnswer("dropped".into())), input()),
            Answer::Unknown
        );
        assert_eq!(
            answered(
                Err(RelayError::Refused {
                    phase: Phase::Rcpt,
                    code: 550
                }),
                input()
            ),
            Answer::Failed(RECIPIENT_REJECTED.into())
        );
        assert_eq!(
            answered(
                Err(RelayError::Refused {
                    phase: Phase::Message,
                    code: 554
                }),
                input()
            ),
            Answer::Failed(MESSAGE_REJECTED.into())
        );
        assert_eq!(
            answered(
                Err(RelayError::Refused {
                    phase: Phase::SignIn,
                    code: 535
                }),
                input()
            ),
            Answer::Misconfigured
        );
        assert_eq!(
            answered(Err(RelayError::Insecure("no TLS".into())), input()),
            Answer::Misconfigured
        );
        assert_eq!(answered(Ok(()), Value::Unit), Answer::Misconfigured);
    }
}
