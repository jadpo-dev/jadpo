//! Serving HTTP natively (docs/compiler-runtime.md §2.6): one listener, and
//! one current-thread tokio runtime per worker thread, each with its own
//! handler, its own connections to the stores and its own rate-limit
//! counters. The listener's thread hands each connection to a worker
//! chosen by the client's address, so one address is always counted on
//! one thread and the runtime's futures never cross threads. hyper reads
//! and writes HTTP/1.1; the runtime's HTTP component
//! (`jadpo_runtime::http`) answers each request. A client has 10 seconds
//! to send a request's headers and 30 its body, and the server holds at
//! most [`MAX_CONNECTIONS`] connections open. The work a response leaves
//! for after it is sent ([`Handler::take_deferred`]) runs as tasks of
//! their own on the worker's thread, which start only once the response
//! has been handed to hyper and never hold it up (chunk 6.1a). The first
//! worker also runs the server's timer, which starts the runtime's tick
//! ([`Handler::tick`]) once a minute (chunk 6.2).

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io;
use std::net::{SocketAddr, TcpListener};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper_util::rt::{TokioIo, TokioTimer};
use jadpo_runtime::LocalFuture;
use jadpo_runtime::http::{BODY_LIMIT, Request, Response};

use crate::Runtime;

/// What answers requests on one worker thread. It lives on that thread,
/// so it may hold the runtime's `Rc` values and its stores.
pub trait Handler {
    fn handle(&self, request: Request) -> LocalFuture<'_, Response>;

    /// The work the response it just gave left for after it is sent:
    /// what its host was given ([`crate::NativeHost::take_deferred`]),
    /// taken as the response is handed back, so it holds only that
    /// response's work.
    fn take_deferred(&self) -> Vec<LocalFuture<'static, ()>> {
        Vec::new()
    }

    /// The runtime's tick, which the server's own timer starts on the
    /// first worker's thread at each whole [`Handler::tick_every_ms`] of
    /// UTC (area 12, "Schedules"; chunk 6.2): `None` for a handler with
    /// nothing to tick. Each tick runs as a task of its own, so a long one
    /// holds up neither the requests nor the next tick.
    fn tick(&self) -> Option<LocalFuture<'static, ()>> {
        None
    }

    /// How often the timer ticks, in milliseconds: once a minute unless a
    /// schedule runs more often.
    fn tick_every_ms(&self) -> u64 {
        60_000
    }
}

/// Makes each worker's handler, on the worker's own thread.
pub type MakeHandler = Arc<dyn Fn() -> Box<dyn Handler> + Send + Sync>;

/// A running server: where it listens, and how to stop it.
#[derive(Debug)]
pub struct Server {
    address: SocketAddr,
    stop: Arc<AtomicBool>,
    threads: Vec<JoinHandle<()>>,
}

impl Server {
    /// The address it listens on, with the port the system chose when it
    /// was asked for port 0.
    pub fn address(&self) -> SocketAddr {
        self.address
    }

    /// Stops accepting, lets the workers finish and waits for them.
    pub fn stop(mut self) {
        self.stop.store(true, Ordering::SeqCst);
        for thread in self.threads.drain(..) {
            let _ = thread.join();
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

/// How often the listener looks for a stop between connections.
const POLL: Duration = Duration::from_millis(20);

/// Serves `listener` with `workers` threads, each answering with a handler
/// `make` gives it.
pub fn serve(listener: TcpListener, workers: usize, make: MakeHandler) -> io::Result<Server> {
    let address = listener.local_addr()?;
    listener.set_nonblocking(true)?;
    let stop = Arc::new(AtomicBool::new(false));
    let open = Arc::new(AtomicUsize::new(0));
    let workers = workers.max(1);
    let mut senders = Vec::with_capacity(workers);
    let mut threads = Vec::with_capacity(workers + 1);
    for index in 0..workers {
        let (sender, receiver) = tokio::sync::mpsc::unbounded_channel::<std::net::TcpStream>();
        senders.push(sender);
        let make = Arc::clone(&make);
        let open = Arc::clone(&open);
        let thread = std::thread::Builder::new()
            .name(format!("jadpo-http-{index}"))
            .spawn(move || worker(receiver, make, open, index == 0))?;
        threads.push(thread);
    }
    let accepting = Arc::clone(&stop);
    let acceptor = std::thread::Builder::new()
        .name("jadpo-http-accept".into())
        .spawn(move || {
            while !accepting.load(Ordering::SeqCst) {
                match listener.accept() {
                    // Past the cap a connection is closed at once, so a
                    // flood of idle ones can't exhaust the process.
                    Ok(_) if open.load(Ordering::SeqCst) >= MAX_CONNECTIONS => {}
                    Ok((stream, peer)) => {
                        open.fetch_add(1, Ordering::SeqCst);
                        let mut hasher = DefaultHasher::new();
                        peer.ip().hash(&mut hasher);
                        let at = (hasher.finish() % senders.len() as u64) as usize;
                        if let Some(sender) = senders.get(at) {
                            let _ = sender.send(stream);
                        }
                    }
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                        std::thread::sleep(POLL);
                    }
                    Err(_) => std::thread::sleep(POLL),
                }
            }
            // Dropping the senders ends each worker's loop.
        })?;
    threads.push(acceptor);
    Ok(Server {
        address,
        stop,
        threads,
    })
}

/// One open connection, counted against [`MAX_CONNECTIONS`] until it is
/// dropped.
struct Open(Arc<AtomicUsize>);

impl Drop for Open {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

fn worker(
    mut receiver: tokio::sync::mpsc::UnboundedReceiver<std::net::TcpStream>,
    make: MakeHandler,
    open: Arc<AtomicUsize>,
    first: bool,
) {
    let Ok(runtime) = Runtime::new() else {
        return;
    };
    let handler: Rc<dyn Handler> = Rc::from(make());
    runtime.block_on(async move {
        if first {
            tokio::task::spawn_local(ticker(Rc::clone(&handler)));
        }
        while let Some(stream) = receiver.recv().await {
            let counted = Open(Arc::clone(&open));
            let peer = stream.peer_addr().ok();
            if stream.set_nonblocking(true).is_err() {
                continue;
            }
            let Ok(stream) = tokio::net::TcpStream::from_std(stream) else {
                continue;
            };
            let _ = stream.set_nodelay(true);
            let handler = Rc::clone(&handler);
            tokio::task::spawn_local(async move {
                let _counted = counted;
                let service = service_fn(move |request: hyper::Request<Incoming>| {
                    let handler = Rc::clone(&handler);
                    async move {
                        // A body that can't be read in time, or at all,
                        // ends the connection without an answer.
                        let Some(request) = read(request, peer).await else {
                            return Err(Unread);
                        };
                        let response = handler.handle(request).await;
                        // Started as tasks of their own, which run only once
                        // this one, the connection's, has handed the response
                        // on and waits: after the response, never before it.
                        for work in handler.take_deferred() {
                            tokio::task::spawn_local(work);
                        }
                        Ok::<_, Unread>(write(response))
                    }
                });
                let _ = http1::Builder::new()
                    .keep_alive(true)
                    .timer(TokioTimer::new())
                    .header_read_timeout(HEADER_TIMEOUT)
                    .serve_connection(TokioIo::new(stream), service)
                    .await;
            });
        }
    });
}

/// The server's own timer (area 12, question 16: `ScheduleTrigger.native`,
/// an in-process timer): at each whole period of UTC it starts the
/// handler's tick as a task of its own. The lease each schedule's state
/// row holds decides who runs it, so ticks from several processes, or a
/// late one, are harmless.
async fn ticker(handler: Rc<dyn Handler>) {
    loop {
        let every = handler.tick_every_ms().max(1_000);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_millis());
        let wait = u128::from(every) - now % u128::from(every);
        tokio::time::sleep(Duration::from_millis(u64::try_from(wait).unwrap_or(every))).await;
        if let Some(work) = handler.tick() {
            tokio::task::spawn_local(work);
        }
    }
}

/// A request whose body wasn't read: hyper closes its connection.
#[derive(Debug)]
struct Unread;

impl std::fmt::Display for Unread {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the request's body couldn't be read")
    }
}

impl std::error::Error for Unread {}

/// How much of a body past the runtime's limit is read and dropped before
/// answering 413.
const DRAIN_LIMIT: usize = 16 * BODY_LIMIT;

/// How long a client has to send a request's headers, from the first byte
/// (or from the last response on a kept-alive connection).
const HEADER_TIMEOUT: Duration = Duration::from_secs(10);

/// How long a client has to send a request's whole body.
const BODY_TIMEOUT: Duration = Duration::from_secs(30);

/// The most connections the server holds open at once, across its
/// workers; one past it is closed as it is accepted.
pub const MAX_CONNECTIONS: usize = 1024;

/// A hyper request as the runtime's, its body read whole, or, once it
/// passes the runtime's limit (area 7, question 5), marked too large and
/// never held: the runtime answers it with 413.
async fn read(request: hyper::Request<Incoming>, peer: Option<SocketAddr>) -> Option<Request> {
    let (parts, mut body) = request.into_parts();
    let mut bytes = Vec::new();
    let declared = parts
        .headers
        .get(hyper::header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<usize>().ok());
    // Past the limit the rest is read and thrown away, up to a bound, so a
    // client still sending reads the 413 rather than a broken connection;
    // past the bound the connection is closed.
    let mut seen = 0usize;
    if declared.is_none_or(|length| length <= DRAIN_LIMIT) {
        // A client that stops sending part-way is given up on, its
        // connection closed without an answer.
        let reading = async {
            while let Some(frame) = body.frame().await {
                if let Ok(data) = frame.ok()?.into_data() {
                    seen = seen.saturating_add(data.len());
                    if seen > DRAIN_LIMIT {
                        break;
                    }
                    if seen <= BODY_LIMIT {
                        bytes.extend_from_slice(&data);
                    }
                }
            }
            Some(())
        };
        tokio::time::timeout(BODY_TIMEOUT, reading).await.ok()??;
    }
    let too_large = seen > BODY_LIMIT || declared.is_some_and(|length| length > BODY_LIMIT);
    if too_large {
        bytes.clear();
    }
    // Every header reaches the runtime, a value that isn't printable ASCII
    // as it came, so the runtime refuses it rather than never seeing it.
    let headers = parts
        .headers
        .iter()
        .map(|(name, value)| {
            (
                name.as_str().to_string(),
                String::from_utf8_lossy(value.as_bytes()).into_owned(),
            )
        })
        .collect();
    Some(Request {
        method: parts.method.as_str().to_string(),
        path: parts.uri.path().to_string(),
        query: parts.uri.query().map(str::to_string),
        headers,
        body: bytes,
        address: peer.map(|peer| peer.ip().to_string()),
        too_large,
    })
}

/// The runtime's response as hyper's.
fn write(response: Response) -> hyper::Response<Full<Bytes>> {
    let mut out = hyper::Response::new(Full::new(Bytes::from(response.body)));
    *out.status_mut() = hyper::StatusCode::from_u16(response.status)
        .unwrap_or(hyper::StatusCode::INTERNAL_SERVER_ERROR);
    for (name, value) in response.headers {
        if let (Ok(name), Ok(value)) = (
            hyper::header::HeaderName::from_bytes(name.as_bytes()),
            hyper::header::HeaderValue::from_str(&value),
        ) {
            out.headers_mut().append(name, value);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::sync::Mutex;
    use std::sync::mpsc;

    use jadpo_runtime::Host;

    use super::*;
    use crate::NativeHost;

    /// Answers every request at once, giving its host work for after the
    /// response that waits for the test to let it go.
    struct Deferring {
        host: NativeHost,
        gate: Arc<Mutex<Option<tokio::sync::oneshot::Receiver<()>>>>,
        said: Arc<Mutex<mpsc::Sender<&'static str>>>,
    }

    impl Handler for Deferring {
        fn handle(&self, _request: Request) -> LocalFuture<'_, Response> {
            let gate = self.gate.lock().unwrap().take();
            let said = Arc::clone(&self.said);
            self.host.defer(Box::pin(async move {
                let _ = said.lock().unwrap().send("started");
                if let Some(gate) = gate {
                    let _ = gate.await;
                }
                let _ = said.lock().unwrap().send("finished");
            }));
            Box::pin(async {
                Response {
                    status: 200,
                    body: b"answered".to_vec(),
                    ..Response::default()
                }
            })
        }

        fn take_deferred(&self) -> Vec<LocalFuture<'static, ()>> {
            self.host.take_deferred()
        }
    }

    /// The work a response leaves runs on the worker's thread after the
    /// response is sent, and never holds it up: the client has its whole
    /// answer while that work is still waiting, and the work then finishes
    /// on its own (chunk 6.1a).
    #[test]
    fn work_left_for_after_a_response_runs_without_holding_it_up() {
        let (release, gate) = tokio::sync::oneshot::channel::<()>();
        let gate = Arc::new(Mutex::new(Some(gate)));
        let (said, heard) = mpsc::channel();
        let said = Arc::new(Mutex::new(said));
        let make: MakeHandler = Arc::new(move || {
            Box::new(Deferring {
                host: NativeHost::new(),
                gate: Arc::clone(&gate),
                said: Arc::clone(&said),
            }) as Box<dyn Handler>
        });
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let server = serve(listener, 1, make).unwrap();
        let mut stream = std::net::TcpStream::connect(server.address()).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        stream
            .write_all(b"GET / HTTP/1.1\r\nHost: jadpo\r\nConnection: close\r\n\r\n")
            .unwrap();
        let mut answer = String::new();
        stream.read_to_string(&mut answer).unwrap();
        assert!(answer.starts_with("HTTP/1.1 200"), "{answer}");
        assert!(answer.ends_with("answered"), "{answer}");
        // The work started after the response, and is still waiting.
        assert_eq!(heard.recv_timeout(Duration::from_secs(10)), Ok("started"));
        assert!(heard.try_recv().is_err(), "it hasn't finished");
        release.send(()).unwrap();
        assert_eq!(heard.recv_timeout(Duration::from_secs(10)), Ok("finished"));
        server.stop();
    }

    /// Ticks every second, saying which thread each tick ran on.
    struct Ticking {
        said: Arc<Mutex<mpsc::Sender<String>>>,
    }

    impl Handler for Ticking {
        fn handle(&self, _request: Request) -> LocalFuture<'_, Response> {
            Box::pin(async { Response::default() })
        }

        fn tick(&self) -> Option<LocalFuture<'static, ()>> {
            let said = Arc::clone(&self.said);
            Some(Box::pin(async move {
                let thread = std::thread::current()
                    .name()
                    .unwrap_or_default()
                    .to_string();
                let _ = said.lock().unwrap().send(thread);
            }))
        }

        fn tick_every_ms(&self) -> u64 {
            1_000
        }
    }

    /// The server's own timer starts the handler's tick at each whole
    /// period, on the first worker's thread only, so one process ticks
    /// once however many workers it has (chunk 6.2).
    #[test]
    fn the_timer_ticks_on_the_first_worker_only() {
        let (said, heard) = mpsc::channel();
        let said = Arc::new(Mutex::new(said));
        let make: MakeHandler = Arc::new(move || {
            Box::new(Ticking {
                said: Arc::clone(&said),
            }) as Box<dyn Handler>
        });
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let server = serve(listener, 3, make).unwrap();
        for _ in 0..2 {
            assert_eq!(
                heard.recv_timeout(Duration::from_secs(5)).as_deref(),
                Ok("jadpo-http-0")
            );
        }
        server.stop();
    }
}
