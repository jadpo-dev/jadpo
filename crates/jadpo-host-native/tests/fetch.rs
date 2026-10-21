//! The package driver's HTTP natively (area 11, "Calling"; chunk 8.1m),
//! against servers on this machine: a connection kept per host and used
//! again, in the clear and over TLS; one the server closed or said it
//! would close never used again; a call's timeout, or a call dropped,
//! closing its connection at once; and a streamed answer read as it is
//! written. Nothing here reaches a network.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use jadpo_drivers::package::{Fetch, FetchError};
use jadpo_host_native::fetch::NativeFetch;
use jadpo_host_native::{Runtime, tls};
use jadpo_runtime::provider::{Reading, Request};
use jadpo_runtime::stream::Chunk;
use jadpo_std::HttpMethod;

use crate::authority::Authority;

/// How a server answers each request on a connection.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Answers {
    /// `200 hello`, the connection kept open for the next.
    Kept,
    /// `200 hello`, then the connection closed without saying so first,
    /// as a server whose keep-alive ran out.
    ThenClosed,
    /// `200 hello` with `Connection: close`, then closed.
    Closing,
    /// `200 hello`, the connection kept open; the next request on it is
    /// read and the connection closed with no answer: a server whose
    /// keep-alive ran out as the request reached it, every time.
    ClosedOnNext,
    /// Never: it reads the request, then waits for the client to close.
    Never,
    /// A head and `first`, then `second` once told to go on, then the end.
    Streamed,
}

/// What a server saw.
#[derive(Default)]
struct Seen {
    accepted: AtomicUsize,
    requests: AtomicUsize,
    /// Each request's head as it came.
    heads: Mutex<Vec<String>>,
}

/// A server on this machine, each connection on a thread of its own and
/// its requests answered in turn.
struct Server {
    port: u16,
    seen: Arc<Seen>,
    /// Told when a client closed a connection it was left waiting on.
    closed: mpsc::Receiver<Instant>,
    /// Lets a streamed answer go on past its first piece.
    go_on: mpsc::Sender<()>,
}

impl Server {
    fn start(answers: Answers) -> Server {
        Server::serving(answers, None)
    }

    /// The same over TLS, with `authority`'s certificate.
    fn secure(answers: Answers, authority: &Authority) -> Server {
        Server::serving(answers, Some(authority.server_config()))
    }

    fn serving(answers: Answers, tls: Option<Arc<rustls::ServerConfig>>) -> Server {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
        let port = listener.local_addr().expect("its address").port();
        let seen = Arc::new(Seen::default());
        let (closed_sender, closed) = mpsc::channel();
        let (go_on, told) = mpsc::channel::<()>();
        let told = Arc::new(Mutex::new(told));
        let serving = Arc::clone(&seen);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                serving.accepted.fetch_add(1, Ordering::SeqCst);
                let seen = Arc::clone(&serving);
                let closed = closed_sender.clone();
                let told = Arc::clone(&told);
                let tls = tls.clone();
                std::thread::spawn(move || {
                    // Short, so a wait notices a closed connection at once.
                    let _ = stream.set_read_timeout(Some(TICK));
                    match tls {
                        Some(config) => {
                            let Ok(connection) = rustls::ServerConnection::new(config) else {
                                return;
                            };
                            let link = rustls::StreamOwned::new(connection, stream);
                            serve(link, answers, &seen, &closed, &told);
                        }
                        None => serve(stream, answers, &seen, &closed, &told),
                    }
                });
            }
        });
        Server {
            port,
            seen,
            closed,
            go_on,
        }
    }

    fn url(&self, scheme: &str, host: &str) -> String {
        format!("{scheme}://{host}:{}/greeting", self.port)
    }

    fn accepted(&self) -> usize {
        self.seen.accepted.load(Ordering::SeqCst)
    }

    fn requests(&self) -> usize {
        self.seen.requests.load(Ordering::SeqCst)
    }
}

/// Answers each request on one connection, as `answers` says, until the
/// client closes it.
fn serve<S: Read + Write>(
    mut link: S,
    answers: Answers,
    seen: &Seen,
    closed: &mpsc::Sender<Instant>,
    told: &Mutex<mpsc::Receiver<()>>,
) {
    let mut pending = Vec::new();
    let mut answered = 0;
    while let Some(head) = next_request(&mut link, &mut pending) {
        seen.requests.fetch_add(1, Ordering::SeqCst);
        seen.heads.lock().expect("heads").push(head);
        match answers {
            Answers::ClosedOnNext if answered > 0 => return,
            Answers::ClosedOnNext => {
                answered += 1;
                let answer = "HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello";
                if link.write_all(answer.as_bytes()).is_err() || link.flush().is_err() {
                    return;
                }
            }
            Answers::Kept | Answers::ThenClosed | Answers::Closing => {
                let close = if answers == Answers::Closing {
                    "Connection: close\r\n"
                } else {
                    ""
                };
                let answer = format!("HTTP/1.1 200 OK\r\nContent-Length: 5\r\n{close}\r\nhello");
                if link.write_all(answer.as_bytes()).is_err() || link.flush().is_err() {
                    return;
                }
                if answers != Answers::Kept {
                    return;
                }
            }
            Answers::Never => return wait_for_close(&mut link, closed),
            Answers::Streamed => {
                let first = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nfirst\r\n";
                if link.write_all(first.as_bytes()).is_err() || link.flush().is_err() {
                    return;
                }
                // Told to go on, or the client closed the connection.
                let until = Instant::now() + LONGEST;
                loop {
                    if told.lock().expect("told").try_recv().is_ok() {
                        break;
                    }
                    match read_some(&mut link, &mut [0u8; 64], Instant::now() + TICK) {
                        Ok(0) => {
                            let _ = closed.send(Instant::now());
                            return;
                        }
                        Err(_) if Instant::now() < until => {}
                        Ok(_) => {}
                        Err(_) => return,
                    }
                }
                let _ = link.write_all(b"6\r\nsecond\r\n0\r\n\r\n");
                let _ = link.flush();
            }
        }
    }
}

/// Waits for the client to close the connection, and says when it did.
fn wait_for_close<S: Read>(link: &mut S, closed: &mpsc::Sender<Instant>) {
    let mut rest = [0u8; 64];
    loop {
        match read_some(link, &mut rest, Instant::now() + LONGEST) {
            Ok(0) => break,
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::TimedOut => return,
            Err(_) => break,
        }
    }
    let _ = closed.send(Instant::now());
}

/// How long a server's read waits before looking again.
const TICK: Duration = Duration::from_millis(10);

/// The longest a server waits for anything.
const LONGEST: Duration = Duration::from_secs(10);

/// One read, waiting until `until` for something to come.
fn read_some<S: Read>(link: &mut S, buffer: &mut [u8], until: Instant) -> std::io::Result<usize> {
    loop {
        match link.read(buffer) {
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                if Instant::now() >= until {
                    return Err(std::io::ErrorKind::TimedOut.into());
                }
            }
            read => return read,
        }
    }
}

/// The next request's head, its body read past: `None` once the client
/// closes the connection.
fn next_request<S: Read>(link: &mut S, pending: &mut Vec<u8>) -> Option<String> {
    let mut buffer = [0u8; 4096];
    let end = loop {
        if let Some(end) = pending.windows(4).position(|w| w == b"\r\n\r\n") {
            break end + 4;
        }
        match read_some(link, &mut buffer, Instant::now() + LONGEST) {
            Ok(0) | Err(_) => return None,
            Ok(n) => pending.extend_from_slice(&buffer[..n]),
        }
    };
    let head = String::from_utf8_lossy(&pending[..end]).into_owned();
    let length = head
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())?
        })
        .unwrap_or(0);
    while pending.len() < end + length {
        match read_some(link, &mut buffer, Instant::now() + LONGEST) {
            Ok(0) | Err(_) => return None,
            Ok(n) => pending.extend_from_slice(&buffer[..n]),
        }
    }
    pending.drain(..end + length);
    Some(head)
}

/// A `GET` that changes nothing, so may be sent again.
fn get(url: String, timeout_ms: u64) -> Request {
    Request {
        method: HttpMethod::Get,
        url,
        headers: vec![("Accept".into(), "application/json".into())],
        body: None,
        timeout_ms,
        reading: Reading::default(),
        repeatable: true,
    }
}

/// Waits on the runtime, so the connection a dropped call held can close,
/// for the server to say a client closed it: `None` after `within`.
async fn closed_within(server: &Server, within: Duration) -> Option<Instant> {
    let until = Instant::now() + within;
    while Instant::now() < until {
        if let Ok(at) = server.closed.try_recv() {
            return Some(at);
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    None
}

/// Calls to one host take turns on one connection, a streamed answer
/// read to its end giving its connection back too; another host has its
/// own.
#[test]
fn a_connection_is_kept_per_host_and_used_again() {
    let one = Server::start(Answers::Kept);
    let two = Server::start(Answers::Kept);
    let fetch = NativeFetch::new();
    let runtime = Runtime::new().expect("a runtime");
    runtime.block_on(async {
        for _ in 0..3 {
            let answer = fetch
                .fetch(&get(one.url("http", "127.0.0.1"), 5000))
                .await
                .expect("an answer");
            assert_eq!(
                (answer.status, answer.body.as_slice()),
                (200, &b"hello"[..])
            );
        }
        let mut opened = fetch
            .open(&get(one.url("http", "127.0.0.1"), 5000))
            .await
            .expect("an answer");
        assert_eq!(opened.body.next().await, Chunk::Bytes(b"hello".to_vec()));
        assert_eq!(opened.body.next().await, Chunk::End);
        fetch
            .fetch(&get(one.url("http", "127.0.0.1"), 5000))
            .await
            .expect("an answer");
        for _ in 0..2 {
            fetch
                .fetch(&get(two.url("http", "127.0.0.1"), 5000))
                .await
                .expect("an answer");
        }
    });
    assert_eq!((one.accepted(), one.requests()), (1, 5));
    assert_eq!((two.accepted(), two.requests()), (1, 2));
    // Kept alive by HTTP/1.1's default: nothing asks for it closed.
    let heads = one.seen.heads.lock().expect("heads");
    assert!(
        heads[0].starts_with("GET /greeting HTTP/1.1\r\n"),
        "{}",
        heads[0]
    );
    assert!(
        heads[0].contains(&format!("Host: 127.0.0.1:{}\r\n", one.port)),
        "{}",
        heads[0]
    );
    assert!(
        !heads[0].to_ascii_lowercase().contains("connection:"),
        "{}",
        heads[0]
    );
}

/// HTTPS connections are kept as well: one handshake for every call.
#[test]
fn an_https_connection_is_kept_and_used_again() {
    let authority = Authority::signing("localhost");
    let server = Server::secure(Answers::Kept, &authority);
    let fetch = NativeFetch::trusting(tls::trusting(&authority.root).expect("the authority"));
    let runtime = Runtime::new().expect("a runtime");
    runtime.block_on(async {
        for _ in 0..3 {
            let answer = fetch
                .fetch(&get(server.url("https", "localhost"), 5000))
                .await
                .expect("an answer");
            assert_eq!(answer.body, b"hello");
        }
    });
    assert_eq!((server.accepted(), server.requests()), (1, 3));
    // Mozilla's roots don't vouch for the test's authority.
    let unvouched = Server::secure(Answers::Kept, &authority);
    let refused =
        runtime.block_on(NativeFetch::new().fetch(&get(unvouched.url("https", "localhost"), 5000)));
    assert!(
        matches!(&refused, Err(FetchError::NotSent(why)) if why.contains("no secure connection")),
        "{refused:?}"
    );
    assert_eq!(unvouched.requests(), 0);
}

/// A connection the server closed after answering, or said it would
/// close, is never used again: each call opens its own and is answered.
#[test]
fn a_connection_the_server_closes_is_not_used_again() {
    for answers in [Answers::ThenClosed, Answers::Closing] {
        let server = Server::start(answers);
        let fetch = NativeFetch::new();
        let runtime = Runtime::new().expect("a runtime");
        runtime.block_on(async {
            for _ in 0..3 {
                let answer = fetch
                    .fetch(&get(server.url("http", "127.0.0.1"), 5000))
                    .await
                    .expect("an answer");
                assert_eq!(answer.body, b"hello");
            }
        });
        assert_eq!((server.accepted(), server.requests()), (3, 3));
    }
}

/// A kept connection the server closes as the next request reaches it,
/// the race every kept connection runs, has written a request that got no
/// answer. One that may be sent again (it changes nothing, or goes under
/// its key) goes out again on a fresh connection and is answered; a change
/// with no key may have acted, and is no answer.
#[test]
fn a_repeatable_request_a_kept_connection_drops_is_sent_on_a_fresh_one() {
    let server = Server::start(Answers::ClosedOnNext);
    let fetch = NativeFetch::new();
    let runtime = Runtime::new().expect("a runtime");
    let url = server.url("http", "127.0.0.1");
    let post = |key: Option<&str>| {
        let mut request = get(url.clone(), 5000);
        request.method = HttpMethod::Post;
        request.body = Some("{}".into());
        request.repeatable = key.is_some();
        if let Some(key) = key {
            request.headers.push(("Idempotency-Key".into(), key.into()));
        }
        request
    };
    runtime.block_on(async {
        // A `GET` on a fresh connection, then on the kept one, which the
        // server drops: sent again on a fresh one.
        for _ in 0..2 {
            let answer = fetch.fetch(&get(url.clone(), 5000)).await;
            assert_eq!(answer.expect("an answer").body, b"hello");
        }
        assert_eq!((server.accepted(), server.requests()), (2, 3));
        // A `POST` with no key, dropped on the kept connection: it may
        // have acted, so it isn't sent again.
        let unkeyed = fetch.fetch(&post(None)).await;
        assert!(
            matches!(unkeyed, Err(FetchError::NoAnswer(_))),
            "{unkeyed:?}"
        );
        assert_eq!((server.accepted(), server.requests()), (2, 4));
        // One with a key, dropped on the kept connection: sent again.
        fetch
            .fetch(&get(url.clone(), 5000))
            .await
            .expect("an answer");
        let keyed = fetch.fetch(&post(Some("k-1"))).await;
        assert_eq!(keyed.expect("an answer").body, b"hello");
        assert_eq!((server.accepted(), server.requests()), (4, 7));
    });
}

/// A call's timeout ends its exchange and closes its connection; so does
/// a call given up before its timeout, its future dropped, whether it
/// was reading a whole answer or a streamed one.
#[test]
fn a_timeout_or_a_dropped_call_closes_its_connection_at_once() {
    let server = Server::start(Answers::Never);
    let fetch = NativeFetch::new();
    let runtime = Runtime::new().expect("a runtime");
    runtime.block_on(async {
        let started = Instant::now();
        let timed_out = fetch
            .fetch(&get(server.url("http", "127.0.0.1"), 200))
            .await;
        assert!(
            matches!(timed_out, Err(FetchError::TimedOut)),
            "{timed_out:?}"
        );
        let closed = closed_within(&server, Duration::from_secs(2))
            .await
            .expect("the connection closed");
        assert!(
            closed - started < Duration::from_secs(1),
            "{:?}",
            closed - started
        );

        // Given up after 100 ms of a 10 second timeout.
        let given_up = Instant::now();
        let dropped = tokio::time::timeout(
            Duration::from_millis(100),
            fetch.fetch(&get(server.url("http", "127.0.0.1"), 10_000)),
        )
        .await;
        assert!(dropped.is_err(), "{dropped:?}");
        let closed = closed_within(&server, Duration::from_secs(2))
            .await
            .expect("the dropped call's connection closed");
        assert!(
            closed - given_up < Duration::from_secs(1),
            "{:?}",
            closed - given_up
        );

        let given_up = Instant::now();
        let dropped = tokio::time::timeout(
            Duration::from_millis(100),
            fetch.open(&get(server.url("http", "127.0.0.1"), 10_000)),
        )
        .await;
        assert!(dropped.is_err(), "the head never came");
        let closed = closed_within(&server, Duration::from_secs(2))
            .await
            .expect("the dropped stream's connection closed");
        assert!(
            closed - given_up < Duration::from_secs(1),
            "{:?}",
            closed - given_up
        );
    });
    assert_eq!(server.accepted(), 3);
}

/// A streamed answer's first piece arrives while the server is still
/// writing; dropping the body part-way closes its connection.
#[test]
fn a_streamed_answer_is_read_as_it_comes() {
    let server = Server::start(Answers::Streamed);
    let fetch = NativeFetch::new();
    let runtime = Runtime::new().expect("a runtime");
    runtime.block_on(async {
        let mut opened = fetch
            .open(&get(server.url("http", "127.0.0.1"), 5000))
            .await
            .expect("its head");
        assert_eq!(opened.status, 200);
        assert!(
            opened
                .headers
                .contains(&("content-type".to_string(), "text/event-stream".to_string())),
            "{:?}",
            opened.headers
        );
        let first = tokio::time::timeout(Duration::from_secs(3), opened.body.next()).await;
        assert_eq!(
            first,
            Ok(Chunk::Bytes(b"first".to_vec())),
            "before the server goes on"
        );
        server.go_on.send(()).expect("go on");
        assert_eq!(opened.body.next().await, Chunk::Bytes(b"second".to_vec()));
        assert_eq!(opened.body.next().await, Chunk::End);

        let mut opened = fetch
            .open(&get(server.url("http", "127.0.0.1"), 5000))
            .await
            .expect("its head");
        assert_eq!(opened.body.next().await, Chunk::Bytes(b"first".to_vec()));
        let dropped = Instant::now();
        drop(opened);
        let closed = closed_within(&server, Duration::from_secs(5))
            .await
            .expect("the stream's connection closed");
        assert!(
            closed - dropped < Duration::from_secs(1),
            "{:?}",
            closed - dropped
        );
    });
    // The first stream, read to its end, gave its connection back.
    assert_eq!((server.accepted(), server.requests()), (1, 2));
}

/// Nothing listening is a call that never left.
#[test]
fn a_host_with_nothing_listening_is_not_sent_to() {
    let port = TcpListener::bind("127.0.0.1:0")
        .expect("a port")
        .local_addr()
        .expect("its address")
        .port();
    let runtime = Runtime::new().expect("a runtime");
    let refused =
        runtime.block_on(NativeFetch::new().fetch(&get(format!("http://127.0.0.1:{port}/"), 2000)));
    assert!(
        matches!(refused, Err(FetchError::NotSent(_))),
        "{refused:?}"
    );
    // A connection was never made, so none is left waiting.
    assert!(TcpStream::connect(("127.0.0.1", port)).is_err());
}
