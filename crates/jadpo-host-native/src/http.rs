//! Serving HTTP natively (docs/compiler-runtime.md §2.5, "Native accept and
//! one rate-limit count", and §2.6): one acceptor, and one current-thread
//! tokio runtime per worker thread, each with its own handler and its own
//! connections to the stores. The acceptor's thread awaits the listener
//! and hands each connection to the workers in turn (chunk 8.1l); a
//! connection and its requests stay on the worker it was handed to, so the
//! runtime's futures never cross threads. The workers share one table of
//! rate-limit counters ([`WorkerThread::host`]), so a client is counted
//! once whichever workers it reaches. hyper reads and writes HTTP/1.1; the
//! runtime's HTTP component (`jadpo_runtime::http`) answers each request.
//! A client has 10 seconds to send a request's headers and 30 its body,
//! and the server holds at most [`MAX_CONNECTIONS`] connections open. The
//! work a response leaves for after it is sent
//! ([`Handler::take_deferred`]) runs as tasks of their own on the worker's
//! thread, which start only once the response has been handed to hyper and
//! never hold it up (chunk 6.1a). The first worker also runs the server's
//! timer, which starts the runtime's tick ([`Handler::tick`]) once a
//! minute (chunk 6.2). Stopped, the server drains: it hands out what the
//! system had already accepted and accepts nothing more, starts no more
//! ticks, and each worker closes its connections once they have given the
//! answers they owe, then waits for its handler's background work to give
//! back what it holds ([`Handler::drain`]; chunks 6.7 and 8.1l).
//! [`AppHandler`] is the handler that serves an application: the
//! runtime's [`Serving`] on a [`NativeHost`], which a deployable and
//! `jadpo dev` both use (chunk 6.9).

use std::cell::Cell;
use std::io;
use std::net::{SocketAddr, TcpListener};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

use bytes::Bytes;

use http_body_util::{BodyExt, Either, Full};
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper_util::rt::{TokioIo, TokioTimer};
use jadpo_runtime::http::{BODY_LIMIT, Request, Response};
use jadpo_runtime::serve::{Observer, Serving};
use jadpo_runtime::{Executor, LocalFuture, RateLimits};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::tracked::{Tracked, unless};
use crate::{DRAIN_GRACE, NativeHost, Runtime};

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

    /// Stops the handler's background work, which gives back what it holds
    /// and takes nothing more, and waits for it to end: what each worker
    /// does as the server stops (chunk 6.7; [`crate::NativeHost::drain`]).
    fn drain(&self) -> LocalFuture<'static, ()> {
        Box::pin(async {})
    }
}

/// A host a worker serves on: the runtime's `Host`, over the worker's
/// [`NativeHost`], which keeps the work left for after a response and the
/// background work a drain waits for. The native host itself, or it with
/// fakes in front (`jadpo dev`'s, chunk 8.1d).
pub trait Served: jadpo_runtime::Host {
    fn native(&self) -> &NativeHost;
}

impl Served for NativeHost {
    fn native(&self) -> &NativeHost {
        self
    }
}

impl<H: Served> Served for jadpo_runtime::capability::Faked<H> {
    fn native(&self) -> &NativeHost {
        self.host().native()
    }
}

/// The handler that serves an application on one worker (chunk 6.9): the
/// runtime's [`Serving`] on the worker's host, whose work left for after
/// a response, and whose ticks, run as tasks on the worker's thread and
/// are given back as the server drains.
pub struct AppHandler {
    serving: Serving,
    host: Rc<dyn Served>,
}

impl std::fmt::Debug for AppHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppHandler").finish_non_exhaustive()
    }
}

impl AppHandler {
    /// Serves on `host` the program `program` gives each time it is asked.
    pub fn new(
        host: impl Served + 'static,
        program: impl Fn() -> Rc<dyn Executor> + 'static,
    ) -> AppHandler {
        let host: Rc<dyn Served> = Rc::new(host);
        AppHandler {
            serving: Serving::new(Rc::clone(&host) as Rc<dyn jadpo_runtime::Host>, program),
            host,
        }
    }

    /// Tells `observer` about each request answered and each tick.
    pub fn observed_by(mut self, observer: Rc<dyn Observer>) -> AppHandler {
        self.serving = self.serving.observed_by(observer);
        self
    }
}

impl Handler for AppHandler {
    fn handle(&self, request: Request) -> LocalFuture<'_, Response> {
        Box::pin(async move { self.serving.handle(&request).await })
    }

    fn take_deferred(&self) -> Vec<LocalFuture<'static, ()>> {
        self.host.native().take_deferred()
    }

    /// Counted as the host's background work, so stopping waits for it to
    /// give back what it holds (chunk 6.7).
    fn tick(&self) -> Option<LocalFuture<'static, ()>> {
        let tick = self.serving.tick();
        Some(self.host.native().track(Box::pin(async move {
            let _ = tick.await;
        })))
    }

    fn tick_every_ms(&self) -> u64 {
        self.serving.tick_every_ms()
    }

    fn drain(&self) -> LocalFuture<'static, ()> {
        self.host.native().drain()
    }
}

/// One of a server's worker threads, as its handler is made on it: which
/// it is, and the rate-limit table it shares with the others (chunk 8.1l).
#[derive(Debug)]
pub struct WorkerThread {
    index: usize,
    limits: Arc<RateLimits>,
}

impl WorkerThread {
    /// Which of the server's workers it is, from 0; the first runs the
    /// server's timer.
    pub fn index(&self) -> usize {
        self.index
    }

    /// A native host counting rate limits in the table all the server's
    /// workers share, so a client has one allowance whichever workers its
    /// requests reach (area 16, "Rate limits"; chunk 8.1l): what a worker
    /// serving an application starts from.
    pub fn host(&self) -> NativeHost {
        NativeHost::new().with_rate_limits(Arc::clone(&self.limits))
    }
}

/// Makes each worker's handler, on the worker's own thread.
pub type MakeHandler = Arc<dyn Fn(&WorkerThread) -> Box<dyn Handler> + Send + Sync>;

/// A running server: where it listens, and how to stop it.
#[derive(Debug)]
pub struct Server {
    address: SocketAddr,
    stop: tokio::sync::watch::Sender<bool>,
    threads: Vec<JoinHandle<()>>,
}

impl Server {
    /// The address it listens on, with the port the system chose when it
    /// was asked for port 0.
    pub fn address(&self) -> SocketAddr {
        self.address
    }

    /// Stops accepting, lets each worker's connections give the answers
    /// they owe and its background work drain, and waits for them
    /// (chunks 6.7 and 8.1l).
    pub fn stop(mut self) {
        self.stop.send_replace(true);
        for thread in self.threads.drain(..) {
            let _ = thread.join();
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.send_replace(true);
    }
}

/// How long a connection that has sent no request yet is given for its
/// first once the server stops: a client sends one as it connects, so
/// this is a round trip with room to spare, and a connection opened ahead
/// of need closes after it.
pub const FIRST_REQUEST: Duration = Duration::from_secs(1);

/// How long the acceptor waits before accepting again when the system
/// has no descriptors or memory to give a connection, which waits in the
/// system's queue meanwhile.
const ACCEPT_PAUSE: Duration = Duration::from_millis(100);

/// Serves `listener` with `workers` threads, each answering with a handler
/// `make` gives it, and an acceptor handing them connections in turn.
pub fn serve(listener: TcpListener, workers: usize, make: MakeHandler) -> io::Result<Server> {
    let address = listener.local_addr()?;
    listener.set_nonblocking(true)?;
    let (stop, stopped) = tokio::sync::watch::channel(false);
    let limits = Arc::new(RateLimits::default());
    let workers = workers.max(1);
    let mut senders = Vec::with_capacity(workers);
    let mut threads = Vec::with_capacity(workers + 1);
    for index in 0..workers {
        let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
        senders.push(sender);
        let thread = WorkerThread {
            index,
            limits: Arc::clone(&limits),
        };
        let make = Arc::clone(&make);
        let thread = std::thread::Builder::new()
            .name(format!("jadpo-http-{index}"))
            .spawn(move || worker(&thread, receiver, &make))?;
        threads.push(thread);
    }
    let handing = Handing {
        workers: senders,
        next: 0,
        open: Arc::new(AtomicUsize::new(0)),
    };
    let acceptor = std::thread::Builder::new()
        .name("jadpo-http-accept".into())
        .spawn(move || accept(listener, handing, stopped))?;
    threads.push(acceptor);
    Ok(Server {
        address,
        stop,
        threads,
    })
}

/// The acceptor: awaits the listener on a current-thread runtime of its
/// own, handing each connection on as it arrives, until the server stops;
/// then it hands on what the system had already accepted and closes the
/// listener.
fn accept(
    listener: TcpListener,
    mut handing: Handing,
    mut stopped: tokio::sync::watch::Receiver<bool>,
) {
    let Ok(runtime) = Runtime::new() else {
        return;
    };
    runtime.block_on(async move {
        let Ok(listener) = tokio::net::TcpListener::from_std(listener) else {
            return;
        };
        loop {
            let stop = async {
                let _ = stopped.wait_for(|stop| *stop).await;
            };
            let Some(accepted) = unless(listener.accept(), stop).await else {
                break;
            };
            match accepted {
                Ok((stream, peer)) => {
                    if let Ok(stream) = stream.into_std() {
                        handing.hand(stream, peer);
                    }
                }
                // One its client gave up on before it was taken.
                Err(error) if abandoned(&error) => {}
                // No descriptor or memory for it: it waits in the system's
                // queue a moment.
                Err(_) => {
                    let stop = async {
                        let _ = stopped.wait_for(|stop| *stop).await;
                    };
                    if unless(tokio::time::sleep(ACCEPT_PAUSE), stop)
                        .await
                        .is_none()
                    {
                        break;
                    }
                }
            }
        }
        // Stopping: what the system has accepted already is handed on
        // too, at most a cap's worth, so none is dropped unread.
        if let Ok(listener) = listener.into_std() {
            for _ in 0..MAX_CONNECTIONS {
                match listener.accept() {
                    Ok((stream, peer)) => handing.hand(stream, peer),
                    Err(error) if abandoned(&error) => {}
                    Err(_) => break,
                }
            }
        }
        // Dropping the listener closes it; dropping `handing` ends each
        // worker's loop once it has taken what it was handed.
    });
}

/// Whether an accept failed only because its client gave up first.
fn abandoned(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::ConnectionAborted
            | io::ErrorKind::ConnectionReset
            | io::ErrorKind::Interrupted
    )
}

/// A connection on its way to a worker: its stream, its peer's address,
/// and its place under [`MAX_CONNECTIONS`].
struct Accepted {
    stream: std::net::TcpStream,
    peer: SocketAddr,
    open: Open,
}

/// Hands connections to the workers in turn (chunk 8.1l).
struct Handing {
    workers: Vec<UnboundedSender<Accepted>>,
    next: usize,
    /// The connections open across the workers.
    open: Arc<AtomicUsize>,
}

impl Handing {
    /// Hands `stream` to the next worker in turn, passing over any whose
    /// thread has ended. Past the cap, or with no worker left, it is
    /// closed at once, so a flood of idle connections can't exhaust the
    /// process.
    fn hand(&mut self, stream: std::net::TcpStream, peer: SocketAddr) {
        if self.open.load(Ordering::SeqCst) >= MAX_CONNECTIONS {
            return;
        }
        let mut accepted = Accepted {
            stream,
            peer,
            open: Open::counting(&self.open),
        };
        for _ in 0..self.workers.len() {
            let Some(at) = self.next.checked_rem(self.workers.len()) else {
                return;
            };
            self.next = self.next.wrapping_add(1);
            let Some(worker) = self.workers.get(at) else {
                return;
            };
            match worker.send(accepted) {
                Ok(()) => return,
                Err(tokio::sync::mpsc::error::SendError(back)) => accepted = back,
            }
        }
    }
}

/// One open connection, counted against [`MAX_CONNECTIONS`] until it is
/// dropped.
struct Open(Arc<AtomicUsize>);

impl Open {
    fn counting(open: &Arc<AtomicUsize>) -> Open {
        open.fetch_add(1, Ordering::SeqCst);
        Open(Arc::clone(open))
    }
}

impl Drop for Open {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

fn worker(thread: &WorkerThread, mut receiver: UnboundedReceiver<Accepted>, make: &MakeHandler) {
    let Ok(runtime) = Runtime::new() else {
        return;
    };
    let handler: Rc<dyn Handler> = Rc::from(make(thread));
    let first = thread.index == 0;
    runtime.block_on(async move {
        let draining = Rc::new(Cell::new(false));
        let connections = Rc::new(Tracked::default());
        if first {
            tokio::task::spawn_local(ticker(Rc::clone(&handler), Rc::clone(&draining)));
        }
        while let Some(accepted) = receiver.recv().await {
            let serving = connection(accepted, Rc::clone(&handler), connections.stopping());
            tokio::task::spawn_local(connections.track(serving));
        }
        // The server is stopping: no more ticks, each connection closes
        // once it has given the answer it owes, and then the background
        // work gives back what it holds, before the worker ends.
        draining.set(true);
        connections.stop(DRAIN_GRACE).await;
        handler.drain().await;
    });
}

/// Serves one connection until it closes, or, once `stopping` finishes,
/// until it has given the answer it owes (hyper's graceful shutdown): one
/// kept open with no request in it closes at once, one mid-request after
/// its answer, and one that has sent no request yet is given
/// [`FIRST_REQUEST`] for one, which is then answered, so a request on its
/// way as the server stops isn't cut off unread.
async fn connection(
    accepted: Accepted,
    handler: Rc<dyn Handler>,
    stopping: LocalFuture<'static, ()>,
) {
    let Accepted {
        stream,
        peer,
        open: _open,
    } = accepted;
    if stream.set_nonblocking(true).is_err() {
        return;
    }
    let Ok(stream) = tokio::net::TcpStream::from_std(stream) else {
        return;
    };
    let _ = stream.set_nodelay(true);
    // Told of each request as hyper has read its head.
    let begun = Rc::new(tokio::sync::Notify::new());
    let begins = Rc::clone(&begun);
    let service = service_fn(move |request: hyper::Request<Incoming>| {
        begins.notify_one();
        let handler = Rc::clone(&handler);
        async move {
            // A body that can't be read in time, or at all, ends the
            // connection without an answer.
            let Some(request) = read(request, Some(peer)).await else {
                return Err(Unread);
            };
            let response = handler.handle(request).await;
            // Started as tasks of their own, which run only once this one,
            // the connection's, has handed the response on and waits:
            // after the response, never before it.
            for work in handler.take_deferred() {
                tokio::task::spawn_local(work);
            }
            Ok::<_, Unread>(write(response))
        }
    });
    let serving = http1::Builder::new()
        .keep_alive(true)
        .timer(TokioTimer::new())
        .header_read_timeout(HEADER_TIMEOUT)
        .serve_connection(TokioIo::new(stream), service);
    let mut serving = std::pin::pin!(serving);
    if unless(serving.as_mut(), stopping).await.is_some() {
        return;
    }
    // hyper closes a connection that has read nothing yet at once, so
    // one whose first request is still on its way is given a moment.
    let first = async {
        let _ = tokio::time::timeout(FIRST_REQUEST, begun.notified()).await;
    };
    if unless(serving.as_mut(), first).await.is_some() {
        return;
    }
    serving.as_mut().graceful_shutdown();
    let _ = serving.await;
}

/// The server's own timer (area 12, question 16: `ScheduleTrigger.native`,
/// an in-process timer): at each whole period of UTC it starts the
/// handler's tick as a task of its own, until the server begins to drain.
/// The lease each schedule's state row holds decides who runs it, so ticks
/// from several processes, or a late one, are harmless.
async fn ticker(handler: Rc<dyn Handler>, draining: Rc<Cell<bool>>) {
    loop {
        let every = handler.tick_every_ms().max(1_000);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_millis());
        let wait = u128::from(every) - now % u128::from(every);
        tokio::time::sleep(Duration::from_millis(u64::try_from(wait).unwrap_or(every))).await;
        if draining.get() {
            return;
        }
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

/// A response's body: whole, or a model's answer sent event by event as
/// it is read (area 7, "Streaming a model's answer"; chunk 5.5v6).
type Body = Either<Full<Bytes>, Pumped>;

/// A body hyper sends a piece at a time as the task reading the stream
/// hands each over; dropped, as hyper drops it when the client goes away,
/// it ends that task, which drops the stream and with it the provider's
/// connection.
struct Pumped(tokio::sync::mpsc::Receiver<Bytes>);

impl hyper::body::Body for Pumped {
    type Data = Bytes;
    type Error = std::convert::Infallible;

    fn poll_frame(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Result<hyper::body::Frame<Bytes>, Self::Error>>> {
        self.0
            .poll_recv(cx)
            .map(|bytes| bytes.map(|bytes| Ok(hyper::body::Frame::data(bytes))))
    }
}

/// Reads `stream` on the worker's thread, handing each event to `sender`
/// as it comes, until the last or until the client has gone, whichever is
/// first: waiting on the model stops as soon as no one is listening.
async fn pump(
    stream: jadpo_runtime::stream::EventStream,
    sender: tokio::sync::mpsc::Sender<Bytes>,
) {
    loop {
        let next = unless(stream.next_written(), sender.closed()).await;
        let Some(event) = next.flatten() else {
            return;
        };
        if sender.send(Bytes::from(event)).await.is_err() {
            return;
        }
    }
}

/// The runtime's response as hyper's.
fn write(mut response: Response) -> hyper::Response<Body> {
    let body = match response.stream.take() {
        Some(stream) => {
            let (sender, receiver) = tokio::sync::mpsc::channel(1);
            tokio::task::spawn_local(pump(stream, sender));
            Either::Right(Pumped(receiver))
        }
        None => Either::Left(Full::new(Bytes::from(response.body))),
    };
    let mut out = hyper::Response::new(body);
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
        let make: MakeHandler = Arc::new(move |_: &WorkerThread| {
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
        let make: MakeHandler = Arc::new(move |_: &WorkerThread| {
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

    /// An application with no routes, which the runtime answers itself.
    struct Bare {
        places: jadpo_runtime::Places,
    }

    impl Bare {
        fn new() -> Bare {
            Bare {
                places: jadpo_runtime::Places::default(),
            }
        }
    }

    impl jadpo_runtime::Executor for Bare {
        fn places(&self) -> &jadpo_runtime::Places {
            &self.places
        }

        fn callable(&self, _key: &str) -> Option<u32> {
            None
        }

        fn run<'a, 'p: 'a>(
            &'a self,
            _ctx: &'a mut jadpo_runtime::Ctx<'p>,
            _callable: u32,
            _args: Vec<jadpo_value::Value>,
        ) -> LocalFuture<'a, jadpo_runtime::Outcome> {
            Box::pin(async { jadpo_runtime::Outcome::Value(jadpo_value::Value::Unit) })
        }
    }

    /// Hears each request answered and each tick.
    struct Heard(Arc<Mutex<mpsc::Sender<String>>>);

    impl Observer for Heard {
        fn answered(&self, answered: &jadpo_runtime::serve::Answered<'_>) {
            let line = format!(
                "{} {} {}",
                answered.request.method, answered.request.path, answered.response.status
            );
            let _ = self.0.lock().unwrap().send(line);
        }

        fn ticked(
            &self,
            _program: &dyn Executor,
            ticked: &jadpo_runtime::schedules::Ticked,
            _now: i64,
        ) {
            let _ = self
                .0
                .lock()
                .unwrap()
                .send(format!("tick {}", ticked.due.len()));
        }
    }

    /// The handler that serves an application needs nothing from the CLI:
    /// given a program and a host, it answers requests through the
    /// runtime, and an observer hears each one and each tick (chunk 6.9).
    #[test]
    fn the_application_handler_serves_without_the_cli() {
        let (said, heard) = mpsc::channel();
        let said = Arc::new(Mutex::new(said));
        let heard_by = Arc::clone(&said);
        let make: MakeHandler = Arc::new(move |_: &WorkerThread| {
            let said = &heard_by;
            let program: Rc<dyn Executor> = Rc::new(Bare::new());
            Box::new(
                AppHandler::new(NativeHost::new(), move || Rc::clone(&program))
                    .observed_by(Rc::new(Heard(Arc::clone(said)))),
            ) as Box<dyn Handler>
        });
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let server = serve(listener, 2, make).unwrap();
        let get = |path: &str| {
            let mut stream = std::net::TcpStream::connect(server.address()).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            let request =
                format!("GET {path} HTTP/1.1\r\nHost: jadpo\r\nConnection: close\r\n\r\n");
            stream.write_all(request.as_bytes()).unwrap();
            let mut answer = String::new();
            stream.read_to_string(&mut answer).unwrap();
            answer
        };
        let live = get("/health/live");
        assert!(live.starts_with("HTTP/1.1 200"), "{live}");
        assert!(live.contains("x-request-id"), "{live}");
        let missing = get("/nothing/here");
        assert!(missing.starts_with("HTTP/1.1 404"), "{missing}");
        assert!(missing.contains("route_not_found"), "{missing}");
        let mut lines = Vec::new();
        while lines.len() < 2 {
            let line = heard.recv_timeout(Duration::from_secs(10)).unwrap();
            if !line.starts_with("tick") {
                lines.push(line);
            }
        }
        assert_eq!(lines, ["GET /health/live 200", "GET /nothing/here 404"]);
        // The server's own timer runs the runtime's tick, once a minute
        // for a program with no schedules, which a test can't wait for:
        // the handler's tick is the one the timer starts.
        let handler = make_tick(&said);
        let runtime = Runtime::new().unwrap();
        runtime.block_on(async {
            handler.tick().unwrap().await;
        });
        let mut ticked = false;
        while let Ok(line) = heard.recv_timeout(Duration::from_secs(10)) {
            if line == "tick 0" {
                ticked = true;
                break;
            }
        }
        assert!(ticked, "the tick was heard");
        assert_eq!(handler.tick_every_ms(), 60_000);
        server.stop();
    }

    /// An application handler for [`Bare`] heard by `said`.
    fn make_tick(said: &Arc<Mutex<mpsc::Sender<String>>>) -> AppHandler {
        let program: Rc<dyn Executor> = Rc::new(Bare::new());
        AppHandler::new(NativeHost::new(), move || Rc::clone(&program))
            .observed_by(Rc::new(Heard(Arc::clone(said))))
    }

    /// The name of the thread it runs on.
    fn thread_name() -> String {
        std::thread::current()
            .name()
            .unwrap_or_default()
            .to_string()
    }

    /// Answers each request with the name of the worker's thread.
    struct Named;

    impl Handler for Named {
        fn handle(&self, _request: Request) -> LocalFuture<'_, Response> {
            Box::pin(async {
                Response {
                    status: 200,
                    body: thread_name().into_bytes(),
                    ..Response::default()
                }
            })
        }
    }

    /// One request on a connection of its own: the whole answer.
    fn get(address: SocketAddr, path: &str) -> String {
        let mut stream = std::net::TcpStream::connect(address).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let request = format!("GET {path} HTTP/1.1\r\nHost: jadpo\r\nConnection: close\r\n\r\n");
        stream.write_all(request.as_bytes()).unwrap();
        let mut answer = String::new();
        stream.read_to_string(&mut answer).unwrap();
        answer
    }

    /// An answer's body.
    fn body(answer: &str) -> &str {
        answer.split_once("\r\n\r\n").map_or("", |(_, body)| body)
    }

    /// One answer on a connection kept open: its head and as much body as
    /// its `content-length` says.
    fn read_answer(stream: &mut std::net::TcpStream) -> String {
        let mut read = Vec::new();
        let mut byte = [0u8; 1];
        while !read.ends_with(b"\r\n\r\n") {
            assert_eq!(stream.read(&mut byte).unwrap(), 1, "the head ended early");
            read.push(byte[0]);
        }
        let head = String::from_utf8(read).unwrap();
        let length = head
            .lines()
            .find_map(|line| line.strip_prefix("content-length: "))
            .map_or(0, |length| length.trim().parse::<usize>().unwrap());
        let mut body = vec![0u8; length];
        stream.read_exact(&mut body).unwrap();
        head + &String::from_utf8(body).unwrap()
    }

    /// Connections go to the workers in turn, whoever sends them: one
    /// client's eight connections reach each of four workers twice
    /// (chunk 8.1l).
    #[test]
    fn connections_are_handed_to_the_workers_in_turn() {
        let make: MakeHandler = Arc::new(|_: &WorkerThread| Box::new(Named) as Box<dyn Handler>);
        let server = serve(TcpListener::bind("127.0.0.1:0").unwrap(), 4, make).unwrap();
        let answered: Vec<String> = (0..8)
            .map(|_| body(&get(server.address(), "/")).to_string())
            .collect();
        let round: std::collections::BTreeSet<&String> = answered[..4].iter().collect();
        assert_eq!(round.len(), 4, "each worker once in four: {answered:?}");
        assert_eq!(answered[..4], answered[4..], "then again in the same turn");
        server.stop();
    }

    /// Hears which worker's thread answered each request.
    struct Threads(Arc<Mutex<Vec<String>>>);

    impl Observer for Threads {
        fn answered(&self, _answered: &jadpo_runtime::serve::Answered<'_>) {
            self.0.lock().unwrap().push(thread_name());
        }
    }

    /// An address has one allowance however many workers its connections
    /// reach (area 16, "Rate limits"; chunk 8.1l): of 125 requests from
    /// one address, each on a connection of its own spread over four
    /// workers, 120 are answered and 5 refused.
    #[test]
    fn a_client_reaching_several_workers_gets_one_allowance() {
        let threads = Arc::new(Mutex::new(Vec::new()));
        let heard = Arc::clone(&threads);
        let make: MakeHandler = Arc::new(move |worker: &WorkerThread| {
            let program: Rc<dyn Executor> = Rc::new(Bare::new());
            Box::new(
                AppHandler::new(worker.host(), move || Rc::clone(&program))
                    .observed_by(Rc::new(Threads(Arc::clone(&heard)))),
            ) as Box<dyn Handler>
        });
        let server = serve(TcpListener::bind("127.0.0.1:0").unwrap(), 4, make).unwrap();
        let mut statuses = std::collections::BTreeMap::new();
        for _ in 0..125 {
            let answer = get(server.address(), "/nothing/here");
            let status = answer.get(9..12).unwrap_or_default().to_string();
            *statuses.entry(status).or_insert(0) += 1;
        }
        server.stop();
        let reached: std::collections::BTreeSet<String> =
            threads.lock().unwrap().iter().cloned().collect();
        assert_eq!(reached.len(), 4, "it reached every worker: {reached:?}");
        assert_eq!(
            statuses,
            [("404".to_string(), 120), ("429".to_string(), 5)].into(),
            "one allowance of 120 a minute, not one per worker"
        );
    }

    /// A burst of connections arriving together is answered in full, and
    /// shared out evenly: none is dropped, and each of four workers takes
    /// a quarter (chunk 8.1l).
    #[test]
    fn a_burst_of_connections_is_answered_in_full() {
        const BURST: usize = 100;
        let make: MakeHandler = Arc::new(|_: &WorkerThread| Box::new(Named) as Box<dyn Handler>);
        let server = serve(TcpListener::bind("127.0.0.1:0").unwrap(), 4, make).unwrap();
        let address = server.address();
        let start = Arc::new(std::sync::Barrier::new(BURST));
        let clients: Vec<_> = (0..BURST)
            .map(|_| {
                let start = Arc::clone(&start);
                std::thread::spawn(move || {
                    start.wait();
                    get(address, "/")
                })
            })
            .collect();
        let mut by_worker = std::collections::BTreeMap::<String, usize>::new();
        for client in clients {
            let answer = client.join().unwrap();
            assert!(answer.starts_with("HTTP/1.1 200"), "{answer}");
            *by_worker.entry(body(&answer).to_string()).or_default() += 1;
        }
        server.stop();
        assert_eq!(
            by_worker.values().copied().collect::<Vec<_>>(),
            [BURST / 4; 4],
            "{by_worker:?}"
        );
    }

    /// Answers the first request once the test lets it, saying when it
    /// has begun, and every other at once.
    struct Gated {
        gate: Arc<Mutex<Option<tokio::sync::oneshot::Receiver<()>>>>,
        said: Arc<Mutex<mpsc::Sender<&'static str>>>,
    }

    impl Handler for Gated {
        fn handle(&self, request: Request) -> LocalFuture<'_, Response> {
            let gate = self.gate.lock().unwrap().take();
            let said = Arc::clone(&self.said);
            Box::pin(async move {
                if let Some(gate) = gate {
                    let _ = said.lock().unwrap().send("begun");
                    let _ = gate.await;
                }
                Response {
                    status: 200,
                    body: format!("answered {}", request.path).into_bytes(),
                    ..Response::default()
                }
            })
        }
    }

    /// Stopped, the server accepts nothing more and drains (chunk 8.1l):
    /// a connection kept open with no request in it closes at once, one
    /// accepted before the stop whose request comes after it is still
    /// answered, and a request in flight is answered in full before
    /// `stop` returns, each connection closed after its answer.
    #[test]
    fn stopping_answers_what_is_in_flight_and_accepts_nothing_more() {
        let (release, gate) = tokio::sync::oneshot::channel::<()>();
        let gate = Arc::new(Mutex::new(Some(gate)));
        let (said, heard) = mpsc::channel();
        let said = Arc::new(Mutex::new(said));
        let make: MakeHandler = Arc::new(move |_: &WorkerThread| {
            Box::new(Gated {
                gate: Arc::clone(&gate),
                said: Arc::clone(&said),
            }) as Box<dyn Handler>
        });
        let server = serve(TcpListener::bind("127.0.0.1:0").unwrap(), 2, make).unwrap();
        let address = server.address();
        let open = || {
            let stream = std::net::TcpStream::connect(address).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            stream
        };
        // Accepted, its request not yet sent. Accepted before the request
        // in flight, which is begun only once everything ahead of it in
        // the system's queue has been taken.
        let mut fresh = open();
        // In flight: its answer waits for the gate.
        let mut in_flight = open();
        in_flight
            .write_all(b"GET /in-flight HTTP/1.1\r\nHost: jadpo\r\n\r\n")
            .unwrap();
        assert_eq!(heard.recv_timeout(Duration::from_secs(10)), Ok("begun"));
        // Kept open after its answer, with nothing in it.
        let mut idle = open();
        idle.write_all(b"GET /idle HTTP/1.1\r\nHost: jadpo\r\n\r\n")
            .unwrap();
        assert!(read_answer(&mut idle).ends_with("answered /idle"));
        let (stopped, stop_returned) = mpsc::channel();
        let stopping = std::thread::spawn(move || {
            server.stop();
            let _ = stopped.send(());
        });
        // The idle connection closes at once.
        let mut rest = Vec::new();
        assert_eq!(
            idle.read_to_end(&mut rest).unwrap(),
            0,
            "closed, unanswered"
        );
        // Nothing more is accepted: refused, or (should another test have
        // been given the port since) never answered by this server.
        if let Ok(mut late) = std::net::TcpStream::connect(address) {
            late.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
            let _ = late.write_all(b"GET /late HTTP/1.1\r\nHost: jadpo\r\n\r\n");
            let mut answer = String::new();
            let _ = late.read_to_string(&mut answer);
            assert!(!answer.contains("answered /late"), "{answer}");
        }
        // The connection accepted before the stop is answered once, then
        // closed.
        fresh
            .write_all(b"GET /fresh HTTP/1.1\r\nHost: jadpo\r\n\r\n")
            .unwrap();
        let mut answer = String::new();
        fresh.read_to_string(&mut answer).unwrap();
        assert!(answer.starts_with("HTTP/1.1 200"), "{answer}");
        assert!(answer.ends_with("answered /fresh"), "{answer}");
        // `stop` waits for the request in flight, which is answered whole.
        assert!(
            stop_returned
                .recv_timeout(Duration::from_millis(300))
                .is_err(),
            "stop waits for the request in flight"
        );
        release.send(()).unwrap();
        let mut answer = String::new();
        in_flight.read_to_string(&mut answer).unwrap();
        assert!(answer.starts_with("HTTP/1.1 200"), "{answer}");
        assert!(answer.contains("connection: close"), "{answer}");
        assert!(answer.ends_with("answered /in-flight"), "{answer}");
        assert_eq!(stop_returned.recv_timeout(Duration::from_secs(10)), Ok(()));
        stopping.join().unwrap();
    }
}
