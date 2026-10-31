//! Fetching a file over HTTP/1.1 for `jadpo build`'s toolchain (roadmap
//! chunk 9.2c) and the releases the toolchain launcher runs (chunk D14):
//! HTTPS through rustls with the certificate and the host checked against
//! Mozilla's roots compiled in, the configuration a native deployable's
//! connections use (`jadpo_host_native::tls`), so a released `jadpo`,
//! whose OpenSSL is built in and finds no CA bundle of the machine's,
//! trusts the same servers on every machine; or plain HTTP to this
//! machine only, which is how a test serves its stand-in. An answer is
//! read by its `Content-Length`, or to the connection's end, a redirect
//! followed up to five times, and what arrives handed on as it comes, so
//! a large file is never held whole.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

const CONNECT: Duration = Duration::from_secs(10);
/// How long a read may wait for the next bytes.
const STALLED: Duration = Duration::from_secs(60);
const REDIRECTS: usize = 5;

/// Where a URL points.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Url {
    secure: bool,
    host: String,
    port: u16,
    /// From the first `/`, with any query.
    path: String,
}

impl Url {
    /// `url` read, or why it can't be fetched: HTTPS, or HTTP to a
    /// loopback host.
    pub(crate) fn parse(url: &str) -> Result<Url, String> {
        let (secure, rest) = if let Some(rest) = url.strip_prefix("https://") {
            (true, rest)
        } else if let Some(rest) = url.strip_prefix("http://") {
            (false, rest)
        } else {
            return Err(format!("`{url}` isn't an `https://` address."));
        };
        let (authority, path) = match rest.find('/') {
            Some(at) => rest.split_at(at),
            None => (rest, "/"),
        };
        if authority.is_empty() || authority.contains('@') {
            return Err(format!("`{url}` has no host Jadpo fetches from."));
        }
        let (host, port) = match authority.strip_prefix('[') {
            Some(inner) => {
                let (host, after) = inner
                    .split_once(']')
                    .ok_or_else(|| format!("`{url}` has an unclosed `[`."))?;
                (format!("[{host}]"), after.strip_prefix(':'))
            }
            None => match authority.rsplit_once(':') {
                Some((host, port)) => (host.to_string(), Some(port)),
                None => (authority.to_string(), None),
            },
        };
        let port = match port {
            Some(port) => port
                .parse()
                .map_err(|_| format!("`{url}` has a port that isn't a number."))?,
            None if secure => 443,
            None => 80,
        };
        let loopback = matches!(
            host.to_lowercase().as_str(),
            "localhost" | "127.0.0.1" | "[::1]"
        );
        if !secure && !loopback {
            return Err(format!(
                "`{url}` is plain `http://`, which Jadpo fetches from only on this machine."
            ));
        }
        Ok(Url {
            secure,
            host,
            port,
            path: path.to_string(),
        })
    }
}

/// An answer's status and length, as it starts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Started {
    /// Its `Content-Length`, when it gives one.
    pub(crate) length: Option<u64>,
}

/// Fetches `url`, giving `started` the answer's length once its head has
/// come and `each` every piece of its body as it arrives; an `Err` from
/// `each` stops it. A status other than 200 is a failure naming it.
pub(crate) fn get(
    url: &str,
    started: &mut dyn FnMut(Started),
    each: &mut dyn FnMut(&[u8]) -> std::io::Result<()>,
) -> Result<(), String> {
    let mut url = url.to_string();
    for _ in 0..=REDIRECTS {
        let parsed = Url::parse(&url)?;
        match once(&parsed, started, each)? {
            None => return Ok(()),
            Some(location) => {
                url = if location.starts_with('/') {
                    let scheme = if parsed.secure { "https" } else { "http" };
                    format!("{scheme}://{}:{}{location}", parsed.host, parsed.port)
                } else {
                    location
                };
            }
        }
    }
    Err(format!("`{url}` redirects more than {REDIRECTS} times."))
}

/// Reads all of `url` into memory, refusing more than `most` bytes.
pub(crate) fn get_all(url: &str, most: usize) -> Result<Vec<u8>, String> {
    let mut body = Vec::new();
    get(url, &mut |_| {}, &mut |piece| {
        if body.len() + piece.len() > most {
            return Err(std::io::Error::other("the answer is too long"));
        }
        body.extend_from_slice(piece);
        Ok(())
    })?;
    Ok(body)
}

/// One request: `Some` with where a redirect points, else the body handed
/// on.
fn once(
    url: &Url,
    started: &mut dyn FnMut(Started),
    each: &mut dyn FnMut(&[u8]) -> std::io::Result<()>,
) -> Result<Option<String>, String> {
    let host = url.host.trim_start_matches('[').trim_end_matches(']');
    let named = format!("`{}`", url.host);
    let targets = (host, url.port)
        .to_socket_addrs()
        .map_err(|_| format!("{named} couldn't be found."))?;
    let mut stream = None;
    let mut last = String::from("nothing to connect to");
    for target in targets {
        match TcpStream::connect_timeout(&target, CONNECT) {
            Ok(connected) => {
                stream = Some(connected);
                break;
            }
            Err(error) => last = crate::hq::not_connected(&error, CONNECT),
        }
    }
    let stream = stream.ok_or_else(|| format!("No connection to {named}: {last}."))?;
    stream
        .set_read_timeout(Some(STALLED))
        .and_then(|()| stream.set_write_timeout(Some(STALLED)))
        .map_err(|error| error.to_string())?;
    let request = format!(
        "GET {} HTTP/1.1\r\nHost: {}\r\nUser-Agent: jadpo\r\nAccept-Encoding: identity\r\nConnection: close\r\n\r\n",
        url.path, url.host
    );
    let broke = |error: std::io::Error| format!("The download from {named} broke off: {error}.");
    if url.secure {
        let config = jadpo_host_native::tls::config().map_err(|error| error.to_string())?;
        let name = rustls::pki_types::ServerName::try_from(host.to_string())
            .map_err(|_| format!("{named} isn't a host name."))?;
        let connection = rustls::ClientConnection::new(config, name)
            .map_err(|error| format!("No secure connection to {named}: {error}."))?;
        let tls = rustls::StreamOwned::new(connection, stream);
        exchange(tls, &request, &named, started, each).map_err(|error| {
            if error.kind() == std::io::ErrorKind::InvalidData {
                format!("No secure connection to {named}: {error}.")
            } else {
                broke(error)
            }
        })?
    } else {
        exchange(stream, &request, &named, started, each).map_err(broke)?
    }
}

fn exchange<S: Read + Write>(
    mut stream: S,
    request: &str,
    named: &str,
    started: &mut dyn FnMut(Started),
    each: &mut dyn FnMut(&[u8]) -> std::io::Result<()>,
) -> std::io::Result<Result<Option<String>, String>> {
    stream.write_all(request.as_bytes())?;
    stream.flush()?;
    let mut reader = BufReader::new(stream);
    let mut status = String::new();
    reader.read_line(&mut status)?;
    let code: u16 = status
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .unwrap_or(0);
    let mut length = None;
    let mut location = None;
    let mut chunked = false;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 || line == "\r\n" || line == "\n" {
            break;
        }
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();
        match name.trim().to_ascii_lowercase().as_str() {
            "content-length" => length = value.parse().ok(),
            "location" => location = Some(value.to_string()),
            "transfer-encoding" => chunked = value.eq_ignore_ascii_case("chunked"),
            _ => {}
        }
    }
    if matches!(code, 301 | 302 | 303 | 307 | 308) {
        return Ok(match location {
            Some(location) => Ok(Some(location)),
            None => Err(format!("{named} redirected without saying where.")),
        });
    }
    if code != 200 {
        return Ok(Err(match code {
            0 => format!("{named}'s answer isn't HTTP."),
            404 => format!("{named} has no such file (404)."),
            code => format!("{named} answered {code}."),
        }));
    }
    started(Started { length });
    let mut buffer = vec![0; 1 << 16];
    let mut read = 0u64;
    if chunked {
        loop {
            let mut size = String::new();
            reader.read_line(&mut size)?;
            let size = u64::from_str_radix(size.split(';').next().unwrap_or_default().trim(), 16)
                .map_err(|_| std::io::Error::other("a chunk without its size"))?;
            if size == 0 {
                return Ok(Ok(None));
            }
            let mut left = size;
            while left > 0 {
                let want = left.min(buffer.len() as u64) as usize;
                reader.read_exact(&mut buffer[..want])?;
                each(&buffer[..want])?;
                left -= want as u64;
            }
            let mut end = String::new();
            reader.read_line(&mut end)?;
        }
    }
    loop {
        let want = match length {
            Some(length) if read >= length => return Ok(Ok(None)),
            Some(length) => (length - read).min(buffer.len() as u64) as usize,
            None => buffer.len(),
        };
        let got = reader.read(&mut buffer[..want])?;
        if got == 0 {
            return Ok(match length {
                Some(length) => Err(format!(
                    "{named} sent {read} of the {length} bytes it promised."
                )),
                None => Ok(None),
            });
        }
        each(&buffer[..got])?;
        read += got as u64;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_address_is_https_or_on_this_machine() {
        let dist = Url::parse("https://static.rust-lang.org/dist/x.toml").expect("https");
        assert_eq!(
            (
                dist.secure,
                dist.host.as_str(),
                dist.port,
                dist.path.as_str()
            ),
            (true, "static.rust-lang.org", 443, "/dist/x.toml")
        );
        let local = Url::parse("http://127.0.0.1:4100/dist").expect("loopback");
        assert_eq!((local.secure, local.port), (false, 4100));
        assert!(Url::parse("http://[::1]:9/").is_ok());
        assert!(Url::parse("http://mirror.example.test/dist").is_err());
        assert!(Url::parse("https://user@mirror.example.test/").is_err());
        assert!(Url::parse("ftp://mirror.example.test/").is_err());
    }

    #[test]
    fn a_file_is_handed_on_as_it_arrives_and_a_short_one_refused() {
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
        let port = listener.local_addr().expect("an address").port();
        std::thread::spawn(move || {
            for (at, stream) in listener.incoming().flatten().enumerate() {
                let mut stream = stream;
                let mut head = [0; 1024];
                let _ = stream.read(&mut head);
                let answer: &[u8] = match at {
                    0 => b"HTTP/1.1 302 Found\r\nLocation: /file\r\nContent-Length: 0\r\n\r\n",
                    1 => b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello",
                    2 => b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nhel\r\n2\r\nlo\r\n0\r\n\r\n",
                    3 => b"HTTP/1.1 200 OK\r\nContent-Length: 9\r\n\r\nhello",
                    _ => b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n",
                };
                let _ = stream.write_all(answer);
            }
        });
        let url = format!("http://127.0.0.1:{port}/start");
        assert_eq!(get_all(&url, 100).expect("fetched"), b"hello");
        assert_eq!(
            get_all(&url.replace("start", "file"), 100).expect("fetched"),
            b"hello"
        );
        let short = get_all(&url, 100).expect_err("short");
        assert!(short.contains("5 of the 9 bytes"), "{short}");
        let missing = get_all(&url, 100).expect_err("missing");
        assert!(missing.contains("404"), "{missing}");
    }
}
