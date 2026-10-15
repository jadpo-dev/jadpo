//! Serving HTTP natively (docs/compiler-runtime.md §2.6): one listener, and
//! one current-thread tokio runtime per worker thread, each with its own
//! handler, its own connections to the stores and its own rate-limit
//! counters. The listener's thread hands each connection to a worker
//! chosen by the client's address, so one address is always counted on
//! one thread and the runtime's futures never cross threads. hyper reads
//! and writes HTTP/1.1; the runtime's HTTP component
//! (`jadpo_runtime::http`) answers each request.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io;
use std::net::{SocketAddr, TcpListener};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use jadpo_runtime::LocalFuture;
use jadpo_runtime::http::{BODY_LIMIT, Request, Response};

use crate::Runtime;

/// What answers requests on one worker thread. It lives on that thread,
/// so it may hold the runtime's `Rc` values and its stores.
pub trait Handler {
    fn handle(&self, request: Request) -> LocalFuture<'_, Response>;
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
    let workers = workers.max(1);
    let mut senders = Vec::with_capacity(workers);
    let mut threads = Vec::with_capacity(workers + 1);
    for index in 0..workers {
        let (sender, receiver) = tokio::sync::mpsc::unbounded_channel::<std::net::TcpStream>();
        senders.push(sender);
        let make = Arc::clone(&make);
        let thread = std::thread::Builder::new()
            .name(format!("jadpo-http-{index}"))
            .spawn(move || worker(receiver, make))?;
        threads.push(thread);
    }
    let accepting = Arc::clone(&stop);
    let acceptor = std::thread::Builder::new()
        .name("jadpo-http-accept".into())
        .spawn(move || {
            while !accepting.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, peer)) => {
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

fn worker(
    mut receiver: tokio::sync::mpsc::UnboundedReceiver<std::net::TcpStream>,
    make: MakeHandler,
) {
    let Ok(runtime) = Runtime::new() else {
        return;
    };
    let handler: Rc<dyn Handler> = Rc::from(make());
    runtime.block_on(async move {
        while let Some(stream) = receiver.recv().await {
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
                let service = service_fn(move |request: hyper::Request<Incoming>| {
                    let handler = Rc::clone(&handler);
                    async move {
                        let request = read(request, peer).await;
                        let response = match request {
                            Some(request) => handler.handle(request).await,
                            None => Response {
                                status: 400,
                                ..Response::default()
                            },
                        };
                        Ok::<_, std::convert::Infallible>(write(response))
                    }
                });
                let _ = http1::Builder::new()
                    .keep_alive(true)
                    .serve_connection(TokioIo::new(stream), service)
                    .await;
            });
        }
    });
}

/// A hyper request as the runtime's, its body read whole, or only until
/// it passes the runtime's limit (area 7, question 5), which the runtime
/// answers with 413, so a larger body is never held.
async fn read(request: hyper::Request<Incoming>, peer: Option<SocketAddr>) -> Option<Request> {
    let (parts, mut body) = request.into_parts();
    let mut bytes = Vec::new();
    let mut too_large = false;
    let declared = parts
        .headers
        .get(hyper::header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<usize>().ok());
    if declared.is_some_and(|length| length > BODY_LIMIT) {
        too_large = true;
    } else {
        while let Some(frame) = body.frame().await {
            if let Ok(data) = frame.ok()?.into_data() {
                bytes.extend_from_slice(&data);
                if bytes.len() > BODY_LIMIT {
                    too_large = true;
                    break;
                }
            }
        }
    }
    let headers = parts
        .headers
        .iter()
        .filter_map(|(name, value)| {
            Some((name.as_str().to_string(), value.to_str().ok()?.to_string()))
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
