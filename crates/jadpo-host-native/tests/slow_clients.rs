//! Slow clients against the native HTTP server,
//! `jadpo_host_native::http::serve` (docs/compiler-runtime.md §2.6,
//! "Deadlines and request limits"): a 10-second header timeout, 30 seconds
//! for a body, 30 for a write to wait for the client to read, and at most
//! `MAX_CONNECTIONS` connections, so that an idle client, or one that never
//! reads its answers, can't hold the server (security audit S1).

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::time::{Duration, Instant};

use jadpo_host_native::http::{Handler, MakeHandler, WorkerThread, serve};
use jadpo_runtime::LocalFuture;
use jadpo_runtime::http::{Request, Response};

/// Answers every request with a small JSON body, as any route does.
struct Small;

impl Handler for Small {
    fn handle(&self, _request: Request) -> LocalFuture<'_, Response> {
        Box::pin(async {
            Response {
                status: 200,
                headers: vec![("content-type".into(), "application/json".into())],
                body: vec![b' '; 512],
                ..Response::default()
            }
        })
    }
}

fn server() -> jadpo_host_native::http::Server {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
    let make: MakeHandler = Arc::new(|_: &WorkerThread| Box::new(Small) as Box<dyn Handler>);
    serve(listener, 2, make).expect("serving")
}

/// A connection whose receive buffer is as small as the system allows, so
/// a few answers fill it; the client never reads it.
fn stalled(address: SocketAddr, pipelined: usize) -> TcpStream {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime");
    let stream = runtime.block_on(async {
        let socket = tokio::net::TcpSocket::new_v4().expect("a socket");
        let _ = socket.set_recv_buffer_size(1024);
        socket.connect(address).await.expect("connected")
    });
    let stream = stream.into_std().expect("a std stream");
    stream.set_nonblocking(false).expect("blocking");
    stream
        .set_write_timeout(Some(Duration::from_millis(200)))
        .expect("a write timeout");
    let one = b"GET /x HTTP/1.1\r\nHost: a\r\n\r\n";
    let all: Vec<u8> = one
        .iter()
        .copied()
        .cycle()
        .take(one.len() * pipelined)
        .collect();
    let mut written = 0;
    // Written until the server stops reading; what isn't sent doesn't
    // matter, the connection is already stuck.
    while written < all.len() {
        match (&stream).write(&all[written..]) {
            Ok(0) | Err(_) => break,
            Ok(n) => written += n,
        }
    }
    stream
}

/// Every answer still to come on `stream`, read until it ends or goes
/// quiet: how many there were.
fn answers_left(mut stream: TcpStream) -> usize {
    // A connection the server has reset refuses the option on some
    // systems; reading it then ends at once.
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let mut seen = Vec::new();
    let mut buffer = [0u8; 65_536];
    loop {
        match stream.read(&mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(n) => seen.extend_from_slice(&buffer[..n]),
        }
    }
    String::from_utf8_lossy(&seen)
        .matches("HTTP/1.1 200")
        .count()
}

/// An idle connection that sends nothing is closed by the header timeout.
#[test]
fn an_idle_connection_is_closed_by_the_header_timeout() {
    let server = server();
    let mut idle = TcpStream::connect(server.address()).expect("connected");
    idle.set_read_timeout(Some(Duration::from_secs(20)))
        .expect("a timeout");
    let began = Instant::now();
    let mut buffer = [0u8; 16];
    let read = idle.read(&mut buffer);
    assert!(
        matches!(read, Ok(0) | Err(_)) && began.elapsed() < Duration::from_secs(15),
        "an idle connection is closed within the header timeout ({:?})",
        began.elapsed()
    );
    server.stop();
}

/// A client that pipelines requests and never reads the answers is given
/// up once a write has waited `WRITE_TIMEOUT` for it: its connection, and
/// its place under `MAX_CONNECTIONS`, are let go, so a few hundred such
/// connections can't turn every other client away.
#[test]
fn a_client_that_never_reads_is_given_up() {
    let server = server();
    let address = server.address();
    const PIPELINED: usize = 3000;
    let stalled = stalled(address, PIPELINED);
    std::thread::sleep(jadpo_host_native::http::WRITE_TIMEOUT + Duration::from_secs(6));
    let answered = answers_left(stalled);
    server.stop();
    assert!(
        answered < PIPELINED,
        "a connection that read nothing for longer than the write timeout is closed; it was \
         kept, and all {answered} of its {PIPELINED} requests were answered once it read"
    );
}
