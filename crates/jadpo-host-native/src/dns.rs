//! DNS `TXT` lookups, for `jadpo email check-domain` (area 16, "A mail
//! health score"; chunk 7.3c2): one question to the system's name server,
//! over UDP, asked again over TCP when the answer was cut short. The
//! command reads records through its own seam, so its tests answer from a
//! fake resolver and never reach a name server; this module's own tests
//! answer from a server on this machine.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream, UdpSocket};
use std::time::Duration;

/// How long each lookup waits for an answer.
pub const TIMEOUT: Duration = Duration::from_secs(5);

/// The record type `TXT`.
const TXT: u16 = 16;

/// Why a lookup gave no answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DnsError {
    /// No name server to ask: none in `/etc/resolv.conf`.
    NoServer,
    /// The name server couldn't be reached or didn't answer in time.
    Unreachable(String),
    /// It answered with a failure (`SERVFAIL`, `REFUSED`), by its code.
    Failed(u8),
    /// Its answer couldn't be read.
    Malformed,
}

impl std::fmt::Display for DnsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DnsError::NoServer => f.write_str("no name server is configured"),
            DnsError::Unreachable(why) => write!(f, "the name server didn't answer: {why}"),
            DnsError::Failed(2) => f.write_str("the name server failed to answer (SERVFAIL)"),
            DnsError::Failed(5) => f.write_str("the name server refused the question"),
            DnsError::Failed(code) => write!(f, "the name server answered with error {code}"),
            DnsError::Malformed => f.write_str("the name server's answer couldn't be read"),
        }
    }
}

/// The first name server `/etc/resolv.conf` names, on port 53.
pub fn system_server() -> Option<SocketAddr> {
    let text = std::fs::read_to_string("/etc/resolv.conf").ok()?;
    server_in(&text)
}

/// The first `nameserver` line of a `resolv.conf`.
fn server_in(text: &str) -> Option<SocketAddr> {
    text.lines().find_map(|line| {
        let mut words = line.split_whitespace();
        (words.next()? == "nameserver")
            .then(|| words.next())
            .flatten()
            .and_then(|address| {
                let address = address.split('%').next()?;
                address
                    .parse::<std::net::IpAddr>()
                    .ok()
                    .map(|ip| SocketAddr::new(ip, 53))
            })
    })
}

/// Every `TXT` record at `name`, each one's strings joined, as `server`
/// answers: none when the name doesn't exist or has none.
pub fn txt(server: SocketAddr, name: &str) -> Result<Vec<String>, DnsError> {
    let id = question_id(name);
    let query = query(id, name, TXT).ok_or(DnsError::Malformed)?;
    let bind = if server.is_ipv4() {
        "0.0.0.0:0"
    } else {
        "[::]:0"
    };
    let unreachable = |error: std::io::Error| DnsError::Unreachable(error.to_string());
    let socket = UdpSocket::bind(bind).map_err(unreachable)?;
    socket
        .set_read_timeout(Some(TIMEOUT))
        .map_err(unreachable)?;
    socket.connect(server).map_err(unreachable)?;
    socket.send(&query).map_err(unreachable)?;
    let mut buffer = vec![0u8; 4096];
    let answer = loop {
        let read = socket.recv(&mut buffer).map_err(unreachable)?;
        let answer = buffer.get(..read).ok_or(DnsError::Malformed)?;
        // An answer to another question is someone else's; wait on.
        if answer.get(..2) == Some(&id.to_be_bytes()[..]) {
            break answer.to_vec();
        }
    };
    match read_answer(&answer, id)? {
        Reply::Records(records) => Ok(records),
        Reply::Truncated => over_tcp(server, &query, id),
    }
}

/// The question asked again over TCP, for an answer UDP cut short.
fn over_tcp(server: SocketAddr, query: &[u8], id: u16) -> Result<Vec<String>, DnsError> {
    let unreachable = |error: std::io::Error| DnsError::Unreachable(error.to_string());
    let mut stream = TcpStream::connect_timeout(&server, TIMEOUT).map_err(unreachable)?;
    stream
        .set_read_timeout(Some(TIMEOUT))
        .map_err(unreachable)?;
    let length = u16::try_from(query.len()).map_err(|_| DnsError::Malformed)?;
    let mut framed = length.to_be_bytes().to_vec();
    framed.extend_from_slice(query);
    stream.write_all(&framed).map_err(unreachable)?;
    let mut size = [0u8; 2];
    stream.read_exact(&mut size).map_err(unreachable)?;
    let mut answer = vec![0u8; usize::from(u16::from_be_bytes(size))];
    stream.read_exact(&mut answer).map_err(unreachable)?;
    match read_answer(&answer, id)? {
        Reply::Records(records) => Ok(records),
        Reply::Truncated => Err(DnsError::Malformed),
    }
}

/// A question's id: from the name and the clock, so two lookups at once
/// differ; a name server echoes it, which ties its answer to the question.
fn question_id(name: &str) -> u16 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    name.hash(&mut hasher);
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.subsec_nanos())
        .unwrap_or_default()
        .hash(&mut hasher);
    (hasher.finish() & 0xffff) as u16
}

/// A query for `name`'s records of type `kind`, recursion desired.
fn query(id: u16, name: &str, kind: u16) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(32 + name.len());
    out.extend_from_slice(&id.to_be_bytes());
    out.extend_from_slice(&[0x01, 0x00, 0, 1, 0, 0, 0, 0, 0, 0]);
    for label in name.trim_end_matches('.').split('.') {
        let length = u8::try_from(label.len())
            .ok()
            .filter(|n| (1..64).contains(n))?;
        out.push(length);
        out.extend_from_slice(label.as_bytes());
    }
    out.push(0);
    out.extend_from_slice(&kind.to_be_bytes());
    out.extend_from_slice(&1u16.to_be_bytes());
    Some(out)
}

/// What an answer said.
#[derive(Debug, PartialEq, Eq)]
enum Reply {
    Records(Vec<String>),
    /// It was cut short to fit UDP: ask over TCP.
    Truncated,
}

/// The `TXT` records in `answer`, the reply to question `id`.
fn read_answer(answer: &[u8], id: u16) -> Result<Reply, DnsError> {
    let byte = |at: usize| answer.get(at).copied().ok_or(DnsError::Malformed);
    let word = |at: usize| Ok(u16::from_be_bytes([byte(at)?, byte(at + 1)?]));
    if word(0)? != id || byte(2)? & 0x80 == 0 {
        return Err(DnsError::Malformed);
    }
    if byte(2)? & 0x02 != 0 {
        return Ok(Reply::Truncated);
    }
    match byte(3)? & 0x0f {
        0 => {}
        // The name doesn't exist: it has no records.
        3 => return Ok(Reply::Records(Vec::new())),
        code => return Err(DnsError::Failed(code)),
    }
    let questions = word(4)?;
    let answers = word(6)?;
    let mut at = 12;
    for _ in 0..questions {
        at = skip_name(answer, at)? + 4;
    }
    let mut records = Vec::new();
    for _ in 0..answers {
        at = skip_name(answer, at)?;
        let kind = word(at)?;
        let length = usize::from(word(at + 8)?);
        let data = answer
            .get(at + 10..at + 10 + length)
            .ok_or(DnsError::Malformed)?;
        at += 10 + length;
        if kind != TXT {
            continue;
        }
        let mut text = Vec::new();
        let mut piece = 0;
        while let Some(&size) = data.get(piece) {
            let size = usize::from(size);
            text.extend_from_slice(
                data.get(piece + 1..piece + 1 + size)
                    .ok_or(DnsError::Malformed)?,
            );
            piece += 1 + size;
        }
        records.push(String::from_utf8_lossy(&text).into_owned());
    }
    Ok(Reply::Records(records))
}

/// Where the name at `at` ends: its labels, or a pointer to them.
fn skip_name(answer: &[u8], mut at: usize) -> Result<usize, DnsError> {
    loop {
        let length = *answer.get(at).ok_or(DnsError::Malformed)?;
        match length {
            0 => return Ok(at + 1),
            pointer if pointer & 0xc0 == 0xc0 => return Ok(at + 2),
            label => at += 1 + usize::from(label),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An answer to `query` with `records`, each one string at `name`.
    fn answer(query: &[u8], records: &[&str], truncated: bool, code: u8) -> Vec<u8> {
        let mut out = query.to_vec();
        out[2] = 0x81 | if truncated { 0x02 } else { 0 };
        out[3] = 0x80 | code;
        out[7] = u8::try_from(records.len()).expect("few");
        for record in records {
            // A pointer to the question's name, then TXT, IN, a TTL.
            out.extend_from_slice(&[0xc0, 12, 0, 16, 0, 1, 0, 0, 1, 0]);
            let pieces: Vec<&[u8]> = record.as_bytes().chunks(255).collect();
            let length: usize = pieces.iter().map(|piece| piece.len() + 1).sum();
            out.extend_from_slice(&u16::try_from(length).expect("short").to_be_bytes());
            for piece in pieces {
                out.push(u8::try_from(piece.len()).expect("short"));
                out.extend_from_slice(piece);
            }
        }
        out
    }

    #[test]
    fn records_are_read_and_long_ones_joined() {
        let query = query(7, "notes.example", TXT).expect("a name");
        let long = format!("v=spf1 {} -all", "include:a.example ".repeat(20));
        let reply = answer(&query, &["v=spf1 -all", &long], false, 0);
        assert_eq!(
            read_answer(&reply, 7),
            Ok(Reply::Records(vec!["v=spf1 -all".to_string(), long]))
        );
        assert_eq!(
            read_answer(&answer(&query, &[], false, 3), 7),
            Ok(Reply::Records(Vec::new()))
        );
        assert_eq!(
            read_answer(&answer(&query, &[], false, 2), 7),
            Err(DnsError::Failed(2))
        );
        assert_eq!(
            read_answer(&answer(&query, &[], true, 0), 7),
            Ok(Reply::Truncated)
        );
        assert_eq!(read_answer(&reply, 8), Err(DnsError::Malformed));
        assert_eq!(read_answer(&reply[..20], 7), Err(DnsError::Malformed));
    }

    #[test]
    fn a_name_too_long_for_a_label_asks_nothing() {
        assert!(query(1, &"a".repeat(64), TXT).is_none());
        assert!(query(1, "a..b", TXT).is_none());
    }

    #[test]
    fn the_first_name_server_is_read() {
        assert_eq!(
            server_in("# comment\nsearch lan\nnameserver 192.0.2.1\nnameserver 192.0.2.2\n"),
            Some("192.0.2.1:53".parse().expect("an address"))
        );
        assert_eq!(
            server_in("nameserver fe80::1%en0\n"),
            Some("[fe80::1]:53".parse().expect("an address"))
        );
        assert_eq!(server_in("search lan\n"), None);
    }

    #[test]
    fn a_server_on_this_machine_is_asked_over_udp_then_tcp() {
        // Another test can take the UDP port's number over TCP first; try again.
        let (udp, server, tcp) = (0..50)
            .find_map(|_| {
                let udp = UdpSocket::bind("127.0.0.1:0").ok()?;
                let server = udp.local_addr().ok()?;
                let tcp = std::net::TcpListener::bind(server).ok()?;
                Some((udp, server, tcp))
            })
            .expect("one port free over UDP and TCP");
        let records = ["v=DMARC1; p=reject", "x".repeat(300).leak() as &str];
        let answering = std::thread::spawn(move || {
            let mut buffer = [0u8; 512];
            let (read, from) = udp.recv_from(&mut buffer).expect("a question");
            // Cut short over UDP.
            udp.send_to(&answer(&buffer[..read], &[], true, 0), from)
                .expect("sent");
            let (mut stream, _) = tcp.accept().expect("a connection");
            let mut size = [0u8; 2];
            stream.read_exact(&mut size).expect("its length");
            let mut asked = vec![0u8; usize::from(u16::from_be_bytes(size))];
            stream.read_exact(&mut asked).expect("the question");
            let reply = answer(&asked, &records, false, 0);
            let mut framed = u16::try_from(reply.len())
                .expect("short")
                .to_be_bytes()
                .to_vec();
            framed.extend_from_slice(&reply);
            stream.write_all(&framed).expect("answered");
        });
        let found = txt(server, "_dmarc.notes.example").expect("an answer");
        answering.join().expect("the server");
        assert_eq!(found, records.map(str::to_string).to_vec());
    }
}
