//! HTTP out of the process natively (area 11, "Calling"; chunks 7.2c and
//! 8.1m): [`NativeFetch`], what the package driver sends a provider's call
//! with, and [`Outward`], what the runtime fetches on its own account, each
//! a `Client` of its own per worker: hyper's HTTP/1.1 over tokio's
//! sockets on the worker's thread, HTTPS through rustls with the
//! certificate and the host checked against Mozilla's roots
//! ([`crate::tls`]), its connector made once.
//!
//! - **Connections are kept per host.** An answer read to its end gives
//!   its connection back, and the next call to that host takes it, with at
//!   most [`MOST_IDLE`] waiting for each host, none longer than [`IDLE`].
//!   One the server closed, or said it would close, is let go before
//!   anything is written on it, and a request a kept connection hands back
//!   unsent goes out on a fresh one, so keeping connections never turns a
//!   call that never left into one that may have acted. A server may still
//!   close a kept connection as a request reaches it, which no client can
//!   see coming: a request that may be sent again
//!   ([`Request::repeatable`]) then goes out once more on a fresh
//!   connection, and any other has no answer.
//! - **A call ends with its timeout or its future.** Its timeout ends its
//!   exchange, and so does its future dropped: the connection it was on
//!   is closed at once and never kept, so nothing goes on reading for a
//!   call no one waits for.
//! - **A streamed answer is read as it comes**, each piece waited for at
//!   most the call's timeout; dropping its body closes its connection.
//! - **A name is looked up only when a connection is opened**, with the
//!   system's resolver on tokio's blocking pool (so `/etc/hosts` and the
//!   platform's own configuration count), within the call's timeout.
//! - **No redirect is followed**: a `3xx` is an answer like any other.

use std::cell::RefCell;
use std::collections::HashMap;
use std::io;
use std::net::SocketAddr;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::client::conn::http1;
use hyper::header::{self, HeaderMap};
use hyper_util::rt::TokioIo;
use jadpo_drivers::package::{Fetch, FetchError, Opened};
use jadpo_runtime::LocalFuture;
use jadpo_runtime::provider::{MOST_ANSWER, Request, Response};
use jadpo_runtime::stream::{Chunk, Coming};
use rustls::ClientConfig;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;
use tokio::time::Instant;

/// The longest a connection is waited for, within the call's timeout.
const CONNECT: Duration = Duration::from_secs(5);

/// How long a kept connection waits for its next call before it is let go.
pub const IDLE: Duration = Duration::from_secs(30);

/// The most connections kept waiting for one host, per client.
pub const MOST_IDLE: usize = 8;

/// The package driver's HTTP natively, one per worker
/// ([`crate::supplies`]): HTTPS checked against Mozilla's roots, or plain
/// HTTP, to any address the request names (the package driver keeps plain
/// HTTP to this machine).
#[derive(Clone)]
pub struct NativeFetch {
    client: Client,
}

impl NativeFetch {
    /// A client checking servers against Mozilla's roots.
    pub fn new() -> NativeFetch {
        NativeFetch {
            client: Client::new(crate::tls::config().ok(), Guard::Open),
        }
    }

    /// A client checking servers against `config` alone: a test's own
    /// authority ([`crate::tls::trusting`]).
    pub fn trusting(config: Arc<ClientConfig>) -> NativeFetch {
        NativeFetch {
            client: Client::new(Some(config), Guard::Open),
        }
    }
}

impl Default for NativeFetch {
    fn default() -> NativeFetch {
        NativeFetch::new()
    }
}

impl std::fmt::Debug for NativeFetch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("NativeFetch")
    }
}

impl Fetch for NativeFetch {
    fn fetch<'a>(&'a self, request: &'a Request) -> LocalFuture<'a, Result<Response, FetchError>> {
        Box::pin(async move {
            let deadline = Instant::now() + wait(request.timeout_ms);
            let target = target(&request.url).ok_or_else(unsendable)?;
            let (answer, lease) = self
                .client
                .send(
                    &target,
                    outgoing(request, &target)?,
                    request.repeatable,
                    deadline,
                )
                .await?;
            let (head, body) = answer.into_parts();
            let body = tokio::time::timeout_at(deadline, whole(body, MOST_ANSWER))
                .await
                .map_err(|_| FetchError::TimedOut)??;
            lease.give_back(keeps(&head.headers));
            Ok(Response {
                status: head.status.as_u16(),
                headers: headers(&head.headers),
                body,
            })
        })
    }

    /// The answer's head once it comes, within the call's timeout, and its
    /// body as it comes (chunk 5.5v6): what a model's streamed answer is
    /// read with.
    fn open<'a>(&'a self, request: &'a Request) -> LocalFuture<'a, Result<Opened, FetchError>> {
        Box::pin(async move {
            let wait = wait(request.timeout_ms);
            let target = target(&request.url).ok_or_else(unsendable)?;
            let (answer, lease) = self
                .client
                .send(
                    &target,
                    outgoing(request, &target)?,
                    request.repeatable,
                    Instant::now() + wait,
                )
                .await?;
            let (head, body) = answer.into_parts();
            Ok(Opened {
                status: head.status.as_u16(),
                headers: headers(&head.headers),
                body: Box::new(Body {
                    body,
                    lease: Some(lease),
                    keep: keeps(&head.headers),
                    wait,
                    read: 0,
                }),
            })
        })
    }
}

/// A streamed answer's body, read as it comes. Read to its end, it gives
/// its connection back; dropped before, or broken off, it closes it.
struct Body {
    body: Incoming,
    /// Its connection, until the body ends.
    lease: Option<Lease>,
    /// Whether the server keeps the connection open after it.
    keep: bool,
    /// The longest each piece is waited for.
    wait: Duration,
    /// How much has been read.
    read: usize,
}

impl Coming for Body {
    fn next(&mut self) -> LocalFuture<'_, Chunk> {
        Box::pin(async move {
            if self.lease.is_none() {
                return Chunk::Broken;
            }
            loop {
                let frame = match tokio::time::timeout(self.wait, self.body.frame()).await {
                    Ok(Some(Ok(frame))) => frame,
                    Ok(None) => {
                        if let Some(lease) = self.lease.take() {
                            lease.give_back(self.keep);
                        }
                        return Chunk::End;
                    }
                    Ok(Some(Err(_))) => {
                        self.lease = None;
                        return Chunk::Broken;
                    }
                    Err(_) => {
                        self.lease = None;
                        return Chunk::TimedOut;
                    }
                };
                // Trailers, and empty frames, carry nothing to hand over.
                let Ok(data) = frame.into_data() else {
                    continue;
                };
                if data.is_empty() {
                    continue;
                }
                self.read += data.len();
                if self.read > MOST_ANSWER {
                    self.lease = None;
                    return Chunk::Broken;
                }
                return Chunk::Bytes(data.to_vec());
            }
        })
    }
}

/// What the runtime fetches on its own account natively (area 11,
/// "Receiving from outside"; chunk 7.7b): AWS SNS's signing certificates
/// and subscription confirmations. A `GET` over HTTPS only, the
/// certificate and the host checked by rustls, connected only to an
/// address that is public (never loopback, private, link-local, shared,
/// reserved or multicast, IPv4 inside IPv6 included) and only to the
/// address checked, so a name that resolves again elsewhere can't move
/// it; no redirect followed; at most the bytes asked for. Its connections
/// are kept apart from [`NativeFetch`]'s, which reach any address.
#[derive(Clone)]
pub struct Outward {
    client: Client,
}

impl Default for Outward {
    fn default() -> Outward {
        Outward {
            client: Client::new(crate::tls::config().ok(), Guard::Public { reach: None }),
        }
    }
}

impl std::fmt::Debug for Outward {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Outward")
    }
}

impl Outward {
    /// Requests for `host` go over plain HTTP to `at`, a server on this
    /// machine: a test's stand-in for AWS's SNS host, never built outside
    /// a test.
    #[cfg(any(test, feature = "test-outbound"))]
    pub fn reaching(host: &str, at: SocketAddr) -> Outward {
        Outward {
            client: Client::new(
                crate::tls::config().ok(),
                Guard::Public {
                    reach: Some((host.to_string(), at)),
                },
            ),
        }
    }
}

impl jadpo_runtime::Outbound for Outward {
    fn get<'a>(
        &'a self,
        url: &'a str,
        limit: usize,
        timeout_ms: u64,
    ) -> LocalFuture<'a, Result<(u16, Vec<u8>), String>> {
        Box::pin(async move {
            let deadline = Instant::now() + wait(timeout_ms);
            let target = target(url)
                .filter(|target| target.secure)
                .ok_or("not HTTPS")?;
            let request = hyper::Request::get(target.path.as_str())
                .header(header::HOST, target.authority())
                .header(header::ACCEPT, "*/*")
                .body(Full::default())
                .map_err(|error| format!("a request it can't send: {error}"))?;
            // A `GET` changes nothing: it may be sent again.
            let (answer, lease) = self
                .client
                .send(&target, request, true, deadline)
                .await
                .map_err(said)?;
            let (head, body) = answer.into_parts();
            let body = tokio::time::timeout_at(deadline, whole(body, limit.min(MOST_ANSWER)))
                .await
                .map_err(|_| said(FetchError::TimedOut))?
                .map_err(said)?;
            lease.give_back(keeps(&head.headers));
            Ok((head.status.as_u16(), body))
        })
    }

    /// A sign-in provider's token endpoint (area 8, "Providers"; chunk
    /// A7b): the same guards as a `GET`, sent once, since a code works
    /// once.
    fn post_form<'a>(
        &'a self,
        url: &'a str,
        form: &'a str,
        limit: usize,
        timeout_ms: u64,
    ) -> LocalFuture<'a, Result<(u16, Vec<u8>), String>> {
        Box::pin(async move {
            let deadline = Instant::now() + wait(timeout_ms);
            let target = target(url)
                .filter(|target| target.secure)
                .ok_or("not HTTPS")?;
            let request = hyper::Request::post(target.path.as_str())
                .header(header::HOST, target.authority())
                .header(header::ACCEPT, "application/json")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Full::new(hyper::body::Bytes::from(form.as_bytes().to_vec())))
                .map_err(|error| format!("a request it can't send: {error}"))?;
            let (answer, lease) = self
                .client
                .send(&target, request, false, deadline)
                .await
                .map_err(said)?;
            let (head, body) = answer.into_parts();
            let body = tokio::time::timeout_at(deadline, whole(body, limit.min(MOST_ANSWER)))
                .await
                .map_err(|_| said(FetchError::TimedOut))?
                .map_err(said)?;
            lease.give_back(keeps(&head.headers));
            Ok((head.status.as_u16(), body))
        })
    }
}

/// Which addresses a client connects to.
#[derive(Clone, Debug)]
enum Guard {
    /// Any the request's address names.
    Open,
    /// Public ones only ([`public`]); `reach` a test's server standing in
    /// for one host, over plain HTTP, which only a test sets.
    Public { reach: Option<(String, SocketAddr)> },
}

impl Guard {
    /// The test's server for `host`, when it is the one reached.
    fn reached(&self, host: &str) -> Option<SocketAddr> {
        match self {
            Guard::Public {
                reach: Some((reached, at)),
            } if reached == host => Some(*at),
            _ => None,
        }
    }
}

/// HTTP/1.1 with its connections kept per host, on one worker's thread.
#[derive(Clone)]
struct Client(Rc<Shared>);

struct Shared {
    /// rustls with its roots, made once; `None` when no configuration
    /// could be made, and then HTTPS is refused.
    tls: Option<tokio_rustls::TlsConnector>,
    guard: Guard,
    /// The connections waiting for their next call, oldest first.
    idle: RefCell<HashMap<Key, Vec<Idle>>>,
}

/// Which connections can carry a call: those to the same host and port,
/// over the same scheme, driven by the same runtime.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Key {
    runtime: tokio::runtime::Id,
    secure: bool,
    host: String,
    port: u16,
}

/// A connection waiting for its next call, and since when.
struct Idle {
    connection: Connection,
    since: Instant,
}

/// One connection: its side for sending requests, and the task reading
/// and writing it, which is ended (closing its socket) when it is dropped.
struct Connection {
    sender: http1::SendRequest<Full<Bytes>>,
    task: tokio::task::AbortHandle,
}

impl Drop for Connection {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// A connection out for one exchange: given back once its answer is read
/// to the end, and closed if dropped before.
struct Lease {
    client: Client,
    key: Key,
    connection: Connection,
}

impl Lease {
    /// Keeps the connection for the host's next call, unless `keep` is
    /// false (the server said it would close it), or it has closed.
    fn give_back(self, keep: bool) {
        if keep {
            self.client.keep(self.key, self.connection);
        }
    }
}

impl Client {
    fn new(tls: Option<Arc<ClientConfig>>, guard: Guard) -> Client {
        Client(Rc::new(Shared {
            tls: tls.map(tokio_rustls::TlsConnector::from),
            guard,
            idle: RefCell::new(HashMap::new()),
        }))
    }

    /// Sends `request` to `target` on a kept connection or a new one, and
    /// gives its answer's head, with the connection to give back once its
    /// body is read. Before the request may have left, a failure is
    /// [`FetchError::NotSent`]; after, [`FetchError::NoAnswer`] or, past
    /// `deadline`, [`FetchError::TimedOut`]. A kept connection dropped
    /// under it before any answer sends it once more, on a fresh
    /// connection, when it is `repeatable`.
    async fn send(
        &self,
        target: &Target,
        mut request: hyper::Request<Full<Bytes>>,
        repeatable: bool,
        deadline: Instant,
    ) -> Result<(hyper::Response<Incoming>, Lease), FetchError> {
        let runtime = tokio::runtime::Handle::try_current()
            .map_err(|_| FetchError::Refused("no runtime to send it on".into()))?;
        let key = Key {
            runtime: runtime.id(),
            secure: target.secure,
            host: target.host.clone(),
            port: target.port,
        };
        // Once a request has been written on a kept connection that was
        // dropped, it goes out on a fresh one only.
        let mut fresh = false;
        loop {
            let kept = if fresh { None } else { self.take(&key) };
            let reused = kept.is_some();
            let mut connection = match kept {
                Some(connection) => connection,
                None => {
                    let by = deadline.min(Instant::now() + CONNECT);
                    tokio::time::timeout_at(by, self.connect(target, &runtime))
                        .await
                        .map_err(|_| FetchError::NotSent("no connection in time".into()))??
                }
            };
            // A kept connection the server has closed since is let go
            // before anything is written on it.
            match tokio::time::timeout_at(deadline, connection.sender.ready()).await {
                Ok(Ok(())) => {}
                Ok(Err(_)) if reused => continue,
                Ok(Err(error)) => {
                    return Err(FetchError::NotSent(format!("no connection: {error}")));
                }
                Err(_) => return Err(FetchError::NotSent("no connection in time".into())),
            }
            let again = (reused && repeatable).then(|| copy(&request));
            let sent = connection.sender.try_send_request(request);
            match tokio::time::timeout_at(deadline, sent).await {
                Ok(Ok(answer)) => {
                    let lease = Lease {
                        client: self.clone(),
                        key,
                        connection,
                    };
                    return Ok((answer, lease));
                }
                Ok(Err(mut error)) => match error.take_message() {
                    // Handed back unwritten: a kept connection that closed
                    // as the request reached it. It never left.
                    Some(unsent) if reused => request = unsent,
                    Some(_) => {
                        return Err(FetchError::NotSent(format!(
                            "no connection: {}",
                            error.error()
                        )));
                    }
                    // Written on a kept connection the server closed or
                    // reset before any answer: the server let it go as
                    // the request reached it.
                    None => match again {
                        Some(copied) if dropped(error.error()) => {
                            request = copied;
                            fresh = true;
                        }
                        _ => return Err(no_answer(error.error())),
                    },
                },
                Err(_) => return Err(FetchError::TimedOut),
            }
        }
    }

    /// A new connection to `target`, its task reading and writing it on
    /// `runtime`.
    async fn connect(
        &self,
        target: &Target,
        runtime: &tokio::runtime::Handle,
    ) -> Result<Connection, FetchError> {
        let (addresses, secure) = self.addresses(target).await?;
        let mut last = String::from("nothing to connect to");
        let mut opened = None;
        for address in addresses {
            match TcpStream::connect(address).await {
                Ok(stream) => {
                    opened = Some(stream);
                    break;
                }
                Err(error) => last = error.to_string(),
            }
        }
        let stream = opened.ok_or_else(|| FetchError::NotSent(format!("no connection: {last}")))?;
        let _ = stream.set_nodelay(true);
        let io = if secure {
            let unsecured =
                |error: io::Error| FetchError::NotSent(format!("no secure connection: {error}"));
            let tls = self
                .0
                .tls
                .clone()
                .ok_or_else(|| FetchError::Refused("no TLS configuration".into()))?;
            let name = crate::tls::server_name(&target.host).map_err(unsecured)?;
            Io::Secure(Box::new(
                tls.connect(name, stream).await.map_err(unsecured)?,
            ))
        } else {
            Io::Plain(stream)
        };
        let (sender, connection) = http1::Builder::new()
            .title_case_headers(true)
            .handshake(TokioIo::new(io))
            .await
            .map_err(|error| FetchError::NotSent(format!("no connection: {error}")))?;
        let task = runtime.spawn(connection).abort_handle();
        Ok(Connection { sender, task })
    }

    /// The addresses `target` may be reached at, as the guard allows, and
    /// whether to secure the connection.
    async fn addresses(&self, target: &Target) -> Result<(Vec<SocketAddr>, bool), FetchError> {
        let bare = target.bare();
        if let Some(at) = self.0.guard.reached(bare) {
            return Ok((vec![at], false));
        }
        let found: Vec<SocketAddr> = tokio::net::lookup_host((bare, target.port))
            .await
            .map_err(|_| FetchError::NotSent(format!("`{bare}` couldn't be found")))?
            .collect();
        match self.0.guard {
            Guard::Open => Ok((found, target.secure)),
            Guard::Public { .. } => {
                let public: Vec<SocketAddr> = found
                    .into_iter()
                    .filter(|address| public(address.ip()))
                    .collect();
                if public.is_empty() {
                    return Err(FetchError::Refused(format!(
                        "`{bare}` has no public address"
                    )));
                }
                Ok((public, target.secure))
            }
        }
    }

    /// The newest connection kept for `key` that is still open, letting go
    /// of any that closed or waited too long.
    fn take(&self, key: &Key) -> Option<Connection> {
        let mut idle = self.0.idle.borrow_mut();
        let waiting = idle.get_mut(key)?;
        let now = Instant::now();
        let mut found = None;
        while let Some(kept) = waiting.pop() {
            if fresh(&kept, now) {
                found = Some(kept.connection);
                break;
            }
        }
        if waiting.is_empty() {
            idle.remove(key);
        }
        found
    }

    /// Keeps `connection` for `key`'s next call, letting go of the oldest
    /// past [`MOST_IDLE`], and of every one that closed or waited too long.
    fn keep(&self, key: Key, connection: Connection) {
        if connection.sender.is_closed() {
            return;
        }
        let now = Instant::now();
        let mut idle = self.0.idle.borrow_mut();
        idle.retain(|_, waiting| {
            waiting.retain(|kept| fresh(kept, now));
            !waiting.is_empty()
        });
        let waiting = idle.entry(key).or_default();
        if waiting.len() >= MOST_IDLE {
            waiting.remove(0);
        }
        waiting.push(Idle {
            connection,
            since: now,
        });
    }
}

/// Whether a kept connection may still carry a call.
fn fresh(kept: &Idle, now: Instant) -> bool {
    !kept.connection.sender.is_closed() && now.duration_since(kept.since) < IDLE
}

/// A connection's socket, in the clear or over TLS.
enum Io {
    Plain(TcpStream),
    Secure(Box<tokio_rustls::client::TlsStream<TcpStream>>),
}

impl AsyncRead for Io {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Io::Plain(stream) => Pin::new(stream).poll_read(cx, buf),
            // A peer that closes without TLS's notice has still sent what
            // it sent: hyper decides whether that is a whole answer, as it
            // does by an answer's length or its chunks.
            Io::Secure(stream) => match Pin::new(stream.as_mut()).poll_read(cx, buf) {
                Poll::Ready(Err(error)) if error.kind() == io::ErrorKind::UnexpectedEof => {
                    Poll::Ready(Ok(()))
                }
                read => read,
            },
        }
    }
}

impl AsyncWrite for Io {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        match self.get_mut() {
            Io::Plain(stream) => Pin::new(stream).poll_write(cx, buf),
            Io::Secure(stream) => Pin::new(stream.as_mut()).poll_write(cx, buf),
        }
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[io::IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        match self.get_mut() {
            Io::Plain(stream) => Pin::new(stream).poll_write_vectored(cx, bufs),
            Io::Secure(stream) => Pin::new(stream.as_mut()).poll_write_vectored(cx, bufs),
        }
    }

    fn is_write_vectored(&self) -> bool {
        match self {
            Io::Plain(stream) => stream.is_write_vectored(),
            Io::Secure(stream) => stream.is_write_vectored(),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Io::Plain(stream) => Pin::new(stream).poll_flush(cx),
            Io::Secure(stream) => Pin::new(stream.as_mut()).poll_flush(cx),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Io::Plain(stream) => Pin::new(stream).poll_shutdown(cx),
            Io::Secure(stream) => Pin::new(stream.as_mut()).poll_shutdown(cx),
        }
    }
}

/// A call's timeout, never none.
fn wait(timeout_ms: u64) -> Duration {
    Duration::from_millis(timeout_ms.max(1))
}

fn unsendable() -> FetchError {
    FetchError::Refused("an address it can't send to".into())
}

/// A copy of `request` to send again: its method, address, version,
/// headers and body.
fn copy(request: &hyper::Request<Full<Bytes>>) -> hyper::Request<Full<Bytes>> {
    let mut copied = hyper::Request::new(request.body().clone());
    *copied.method_mut() = request.method().clone();
    *copied.uri_mut() = request.uri().clone();
    *copied.version_mut() = request.version();
    *copied.headers_mut() = request.headers().clone();
    copied
}

/// Whether an exchange ended with its connection closed or reset before
/// any answer came, rather than with an answer that isn't HTTP.
fn dropped(error: &hyper::Error) -> bool {
    use std::error::Error as _;
    error.is_incomplete_message()
        || error
            .source()
            .is_some_and(|source| source.downcast_ref::<io::Error>().is_some())
}

/// Why a request that may have left got no answer.
fn no_answer(error: &hyper::Error) -> FetchError {
    if error.is_parse() {
        FetchError::NoAnswer("an answer that isn't HTTP".into())
    } else {
        FetchError::NoAnswer(error.to_string())
    }
}

/// A failure as [`Outward`] reports it, for the host's log.
fn said(error: FetchError) -> String {
    match error {
        FetchError::NotSent(why) | FetchError::NoAnswer(why) | FetchError::Refused(why) => why,
        FetchError::TimedOut => "no answer in time".into(),
    }
}

/// `request` as hyper sends it to `target`: its headers as made, `Host`
/// first, and `Content-Length` with a body.
fn outgoing(request: &Request, target: &Target) -> Result<hyper::Request<Full<Bytes>>, FetchError> {
    let mut built = hyper::Request::builder()
        .method(request.method.word())
        .uri(target.path.as_str())
        .header(header::HOST, target.authority());
    for (name, value) in &request.headers {
        built = built.header(name.as_str(), value.as_str());
    }
    let body = request.body.clone().unwrap_or_default();
    if request.body.is_some() {
        built = built.header(header::CONTENT_LENGTH, body.len());
    }
    built
        .body(Full::new(Bytes::from(body)))
        .map_err(|error| FetchError::Refused(format!("a request it can't send: {error}")))
}

/// The whole of a body, at most `limit` bytes.
async fn whole(mut body: Incoming, limit: usize) -> Result<Vec<u8>, FetchError> {
    let mut read = Vec::new();
    while let Some(frame) = body.frame().await {
        let frame = frame.map_err(|error| FetchError::NoAnswer(error.to_string()))?;
        if let Some(data) = frame.data_ref() {
            if read.len() + data.len() > limit {
                return Err(FetchError::NoAnswer("an answer too long".into()));
            }
            read.extend_from_slice(data);
        }
    }
    Ok(read)
}

/// Each header's name in lowercase, and its value.
fn headers(map: &HeaderMap) -> Vec<(String, String)> {
    map.iter()
        .map(|(name, value)| {
            (
                name.as_str().to_string(),
                String::from_utf8_lossy(value.as_bytes()).trim().to_string(),
            )
        })
        .collect()
}

/// Whether the server keeps the connection open after this answer: not
/// when it says `Connection: close`.
fn keeps(headers: &HeaderMap) -> bool {
    !headers.get_all(header::CONNECTION).iter().any(|value| {
        value.to_str().is_ok_and(|value| {
            value
                .split(',')
                .any(|token| token.trim().eq_ignore_ascii_case("close"))
        })
    })
}

/// Whether `ip` is an address on the public internet: the runtime's one
/// answer, which the Workers host's guard asks too (chunk 8.2a).
pub use jadpo_runtime::public_address as public;

/// Where a request goes.
#[derive(Debug, PartialEq, Eq)]
struct Target {
    secure: bool,
    /// As the address writes it, an IPv6 address in its brackets.
    host: String,
    port: u16,
    /// Its path and query.
    path: String,
}

impl Target {
    /// The host without an IPv6 address's brackets.
    fn bare(&self) -> &str {
        self.host.trim_start_matches('[').trim_end_matches(']')
    }

    /// The host as `Host` names it: its port only when it isn't the
    /// scheme's own.
    fn authority(&self) -> String {
        let default = if self.secure { 443 } else { 80 };
        if self.port == default {
            self.host.clone()
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }
}

fn target(url: &str) -> Option<Target> {
    let (secure, rest) = if let Some(rest) = url.strip_prefix("https://") {
        (true, rest)
    } else {
        (false, url.strip_prefix("http://")?)
    };
    let (authority, path) = match rest.find(['/', '?']) {
        Some(at) => rest.split_at(at),
        None => (rest, "/"),
    };
    let path = if path.starts_with('?') {
        format!("/{path}")
    } else {
        path.to_string()
    };
    let (host, port) = if let Some(inner) = authority.strip_prefix('[') {
        let (host, after) = inner.split_once(']')?;
        (format!("[{host}]"), after.strip_prefix(':'))
    } else {
        match authority.rsplit_once(':') {
            Some((host, port)) => (host.to_string(), Some(port)),
            None => (authority.to_string(), None),
        }
    };
    let port = match port {
        Some(port) => port.parse().ok()?,
        None if secure => 443,
        None => 80,
    };
    Some(Target {
        secure,
        host,
        port,
        path,
    })
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;

    use jadpo_runtime::Outbound;
    use jadpo_runtime::provider::Reading;
    use jadpo_std::HttpMethod;

    use super::*;

    /// A server on this machine that answers one request with `answer`,
    /// handing back what it was sent.
    fn serve_once(answer: String) -> (String, std::thread::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let thread = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut seen = String::new();
            let mut length = 0;
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = value.trim().parse().unwrap();
                }
                seen.push_str(&line);
                if line == "\r\n" {
                    break;
                }
            }
            let mut body = vec![0; length];
            reader.read_exact(&mut body).unwrap();
            seen.push_str(&String::from_utf8(body).unwrap());
            let mut stream = stream;
            stream.write_all(answer.as_bytes()).unwrap();
            seen
        });
        (format!("http://127.0.0.1:{port}"), thread)
    }

    fn request(url: String) -> Request {
        Request {
            method: HttpMethod::Post,
            url,
            headers: vec![("Authorization".into(), "Bearer test-token".into())],
            body: Some("{\"slot\":\"am\"}".into()),
            timeout_ms: 5000,
            reading: Reading::default(),
            repeatable: false,
        }
    }

    /// A request goes out whole, its headers and body as made, and the
    /// answer comes back by its length or in chunks.
    #[test]
    fn a_request_is_sent_and_its_answer_read() {
        let runtime = crate::Runtime::new().unwrap();
        let fetch = NativeFetch::new();
        let (base, server) = serve_once(
            "HTTP/1.1 201 Created\r\nContent-Type: application/json\r\nContent-Length: 11\r\n\r\n{\"id\":\"b1\"}".into(),
        );
        let sent = request(format!("{base}/v1/bookings?x=1"));
        let answer = runtime.block_on(fetch.fetch(&sent)).unwrap();
        assert_eq!(answer.status, 201);
        assert_eq!(answer.body, b"{\"id\":\"b1\"}");
        assert!(
            answer
                .headers
                .contains(&("content-type".into(), "application/json".into())),
            "{:?}",
            answer.headers
        );
        let seen = server.join().unwrap();
        assert!(
            seen.starts_with("POST /v1/bookings?x=1 HTTP/1.1\r\n"),
            "{seen}"
        );
        assert!(
            seen.contains("Authorization: Bearer test-token\r\n"),
            "{seen}"
        );
        assert!(seen.contains("Content-Length: 13\r\n"), "{seen}");
        assert!(seen.ends_with("{\"slot\":\"am\"}"), "{seen}");

        let (base, _server) = serve_once(
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n4\r\n{\"a\"\r\n3\r\n:1}\r\n0\r\n\r\n".into(),
        );
        let answer = runtime.block_on(fetch.fetch(&request(base))).unwrap();
        assert_eq!(answer.body, b"{\"a\":1}");

        // An answer that isn't HTTP may have acted.
        let (base, _server) = serve_once("SMTP ready\r\n\r\n".into());
        let garbled = runtime.block_on(fetch.fetch(&request(base)));
        assert!(
            matches!(&garbled, Err(FetchError::NoAnswer(why)) if why == "an answer that isn't HTTP"),
            "{garbled:?}"
        );
    }

    /// An address is split into where it goes, an IPv6 address in its
    /// brackets, and `Host` names a port only when it isn't the scheme's.
    #[test]
    fn an_address_is_read_as_where_it_goes() {
        let ipv6 = target("http://[::1]:8080?a=1").unwrap();
        assert_eq!(
            (ipv6.bare(), ipv6.port, ipv6.path.as_str()),
            ("::1", 8080, "/?a=1")
        );
        assert_eq!(ipv6.authority(), "[::1]:8080");
        let named = target("https://api.example.com/v1/x").unwrap();
        assert_eq!(
            (named.port, named.authority()),
            (443, "api.example.com".to_string())
        );
        assert_eq!(target("ftp://example.com/"), None);
        assert_eq!(target("https://example.com:port/"), None);
    }

    /// The runtime's own fetches reach only public addresses, over HTTPS,
    /// and follow no redirect; a test's stand-in for AWS is reached in its
    /// place over plain HTTP.
    #[test]
    fn the_runtimes_own_fetches_keep_to_https_and_public_addresses() {
        for private in [
            "127.0.0.1",
            "10.1.2.3",
            "172.16.0.1",
            "192.168.1.1",
            "169.254.169.254",
            "100.64.0.1",
            "0.0.0.0",
            "224.0.0.1",
            "255.255.255.255",
            "::1",
            "fe80::1",
            "fd00::1",
            "::ffff:10.0.0.1",
            "::ffff:127.0.0.1",
        ] {
            assert!(!public(private.parse().unwrap()), "{private}");
        }
        for open in ["52.94.236.248", "2600:1f18::1"] {
            assert!(public(open.parse().unwrap()), "{open}");
        }
        let runtime = crate::Runtime::new().unwrap();
        let outward = Outward::default();
        // Plain HTTP, and a name or an address that is only private, are
        // refused, nothing connected to.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let port = listener.local_addr().unwrap().port();
        let plain = runtime.block_on(outward.get("http://example.com/a.pem", 100, 1000));
        assert_eq!(plain, Err("not HTTPS".to_string()));
        let local = runtime.block_on(outward.get("https://localhost/a.pem", 100, 1000));
        assert_eq!(local, Err("`localhost` has no public address".to_string()));
        let loopback = format!("https://127.0.0.1:{port}/a.pem");
        let refused = runtime.block_on(outward.get(&loopback, 100, 1000));
        assert_eq!(
            refused,
            Err("`127.0.0.1` has no public address".to_string())
        );
        let ipv6 = format!("https://[::1]:{port}/");
        let refused = runtime.block_on(outward.get(&ipv6, 100, 1000));
        assert_eq!(refused, Err("`::1` has no public address".to_string()));
        assert!(listener.accept().is_err(), "nothing connected");

        // A redirect is an answer, not followed: the place it names is
        // never reached.
        let elsewhere = TcpListener::bind("127.0.0.1:0").unwrap();
        elsewhere.set_nonblocking(true).unwrap();
        let (base, server) = serve_once(format!(
            "HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:{}/a.pem\r\nContent-Length: 0\r\n\r\n",
            elsewhere.local_addr().unwrap().port()
        ));
        let at = base.strip_prefix("http://").unwrap().parse().unwrap();
        let aws = Outward::reaching("sns.eu-west-2.amazonaws.com", at);
        let answer =
            runtime.block_on(aws.get("https://sns.eu-west-2.amazonaws.com/cert.pem", 100, 1000));
        assert_eq!(answer, Ok((302, Vec::new())));
        let seen = server.join().unwrap();
        assert!(seen.starts_with("GET /cert.pem HTTP/1.1\r\n"), "{seen}");
        assert!(
            seen.contains("Host: sns.eu-west-2.amazonaws.com\r\n"),
            "{seen}"
        );
        assert!(elsewhere.accept().is_err(), "the redirect wasn't followed");

        // At most the bytes asked for.
        let (base, _server) =
            serve_once("HTTP/1.1 200 OK\r\nContent-Length: 10\r\n\r\n0123456789".into());
        let at = base.strip_prefix("http://").unwrap().parse().unwrap();
        let aws = Outward::reaching("sns.eu-west-2.amazonaws.com", at);
        let long = runtime.block_on(aws.get("https://sns.eu-west-2.amazonaws.com/a", 4, 1000));
        assert_eq!(long, Err("an answer too long".to_string()));
    }

    /// Nothing listening is a call that never left; a server that never
    /// answers is one that may have acted, and timed out.
    #[test]
    fn a_call_that_never_left_differs_from_one_never_answered() {
        let runtime = crate::Runtime::new().unwrap();
        let fetch = NativeFetch::new();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let refused = runtime.block_on(fetch.fetch(&request(format!("http://127.0.0.1:{port}"))));
        assert!(
            matches!(refused, Err(FetchError::NotSent(_))),
            "{refused:?}"
        );

        let silent = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = silent.local_addr().unwrap().port();
        let mut slow = request(format!("http://127.0.0.1:{port}"));
        slow.timeout_ms = 200;
        let unanswered = runtime.block_on(fetch.fetch(&slow));
        // No answer within its timeout is told apart (chunk 7.2c2).
        assert!(
            matches!(unanswered, Err(FetchError::TimedOut)),
            "{unanswered:?}"
        );
        drop(silent);
    }
}
