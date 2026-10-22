//! The drivers' transport on Workers (docs/compiler-runtime.md §2.6, "The
//! Workers host"): a TCP connection through `connect()` from
//! `cloudflare:sockets`, read and written through its streams, upgraded to
//! TLS with `startTls()`, its tasks run beside the work that started them,
//! and Workers' timer and clock.
//!
//! - **Every connection may be upgraded.** Workers lets a socket start
//!   TLS only when it was opened saying it might (`secureTransport:
//!   "starttls"`), and a driver says so only once its conversation has
//!   begun (SMTP's `STARTTLS`, PostgreSQL's answer to its SSL request), so
//!   every connection is opened that way; one that never upgrades is
//!   plain TCP throughout.
//! - **TLS is checked against the platform's roots only.** `startTls()`
//!   takes no authority of a connection's own, so an upgrade trusting one
//!   (`Trust::Authorities`, area 5's `authority:`) is refused as one that
//!   can't be checked here. The certificate is checked for the name the
//!   driver gives, which `expectedServerHostname` passes when it isn't the
//!   name connected to.
//! - **A failed handshake is a failed check.** Workers opens the upgraded
//!   socket at once and checks the server's certificate as the first
//!   bytes go out, telling JavaScript no more than that the socket broke,
//!   so a failure before anything has been read or written over TLS is
//!   `InvalidData`, as a certificate or a name that doesn't check is
//!   natively: a driver reports it rather than trying again.
//! - **A Hyperdrive binding's host is the platform's pooler**
//!   ([`Transport::is_pooler`]; chunk 8.2i): once the binding's host has
//!   been read, `connect()` to it reaches Hyperdrive inside the platform
//!   for the isolate's life, without TLS, which Hyperdrive uses to the
//!   database.
//! - **No Unix sockets**: a Worker has no file system.
//! - **A connection being opened takes one of the invocation's six
//!   places** ([`Opening`]; chunk 8.2j) until its socket opens, or fails
//!   to: a seventh waits for one, within whatever its driver gives the
//!   connection to open.
//! - **A stream holds its streams' locks only while it reads or writes**,
//!   so it can be upgraded between them, and closes its socket when
//!   dropped.

use std::any::Any;
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll};

use jadpo_drivers::{Endpoint, Stream, Transport, Trust};
use jadpo_runtime::LocalFuture;
use js_sys::{Object, Reflect, Uint8Array};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{ReadableStreamDefaultReader, WritableStreamDefaultWriter};

use crate::js;
use crate::opening::Opening;

/// The drivers' transport over Workers' `connect()`, counting the
/// connections it opens among its invocation's six. A default one has six
/// places of its own; an invocation's host gives its drivers one sharing
/// the invocation's ([`crate::WorkersHost::supplies`]).
#[derive(Clone, Debug, Default)]
pub struct WorkersTransport {
    opening: Rc<Opening>,
}

impl WorkersTransport {
    /// A transport whose connections take places among `opening`'s.
    pub fn counted(opening: Rc<Opening>) -> WorkersTransport {
        WorkersTransport { opening }
    }
}

impl Transport for WorkersTransport {
    fn connect(&self, endpoint: &Endpoint) -> LocalFuture<'_, io::Result<Box<dyn Stream>>> {
        let endpoint = endpoint.clone();
        Box::pin(async move {
            let Endpoint::Tcp { host, port } = endpoint else {
                return Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "a Worker has no Unix sockets",
                ));
            };
            // One of the invocation's six until the socket opens.
            let place = self.opening.place().await;
            let socket = open(&host, port)?;
            let opened = JsFuture::from(socket.opened()).await;
            drop(place);
            opened.map_err(|error| {
                io::Error::new(
                    io::ErrorKind::ConnectionRefused,
                    format!("{host}:{port}: {}", js::said(&error)),
                )
            })?;
            Ok(Box::new(WorkersStream::new(socket, host)) as Box<dyn Stream>)
        })
    }

    fn upgrade<'a>(
        &'a self,
        stream: Box<dyn Stream>,
        name: &'a str,
        trust: Trust<'a>,
    ) -> LocalFuture<'a, io::Result<Box<dyn Stream>>> {
        Box::pin(async move {
            if let Trust::Authorities(_) = trust {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "a Worker checks a server's certificate against its platform's roots only",
                ));
            }
            let stream: Box<dyn Any> = stream;
            let Ok(stream) = stream.downcast::<WorkersStream>() else {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "a stream this transport didn't open",
                ));
            };
            stream.start_tls(name).await
        })
    }

    fn spawn(&self, task: LocalFuture<'static, ()>) {
        wasm_bindgen_futures::spawn_local(task);
    }

    fn sleep(&self, millis: u64) -> LocalFuture<'_, ()> {
        Box::pin(crate::timer::sleep(millis))
    }

    fn now_ms(&self) -> Option<i64> {
        crate::wall_clock()
    }

    /// A Hyperdrive binding's host, which this isolate has read.
    fn is_pooler(&self, endpoint: &Endpoint) -> bool {
        match endpoint {
            Endpoint::Tcp { host, port } => crate::hyperdrive::is_pooler(host, *port),
            Endpoint::Unix { .. } => false,
        }
    }
}

/// `connect()` to `host:port`, ready to be upgraded.
fn open(host: &str, port: u16) -> io::Result<js::Socket> {
    let address = Object::new();
    let options = Object::new();
    let set = |object: &Object, key: &str, value: JsValue| {
        Reflect::set(object, &JsValue::from_str(key), &value)
            .map(|_| ())
            .map_err(|error| io::Error::other(js::said(&error)))
    };
    set(&address, "hostname", JsValue::from_str(host))?;
    set(&address, "port", JsValue::from(port))?;
    set(&options, "secureTransport", JsValue::from_str("starttls"))?;
    js::connect(&address, &options).map_err(|error| {
        io::Error::new(
            io::ErrorKind::ConnectionRefused,
            format!("{host}:{port}: {}", js::said(&error)),
        )
    })
}

/// A connection `connect()` opened, as a driver's byte stream.
pub struct WorkersStream {
    /// `None` once it has been upgraded, when the upgraded stream has it.
    socket: Option<js::Socket>,
    /// The name it connected to.
    host: String,
    reading: Reading,
    writing: Option<Writing>,
    closing: Option<JsFuture>,
    /// The name TLS was started for, until a read or a write over it has
    /// gone through, which proves its handshake.
    checking: Option<String>,
}

impl std::fmt::Debug for WorkersStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkersStream")
            .field("host", &self.host)
            .finish_non_exhaustive()
    }
}

/// Where a read is.
#[derive(Default)]
enum Reading {
    #[default]
    Idle,
    /// Waiting on the readable stream, whose reader holds its lock.
    Waiting(JsFuture, ReadableStreamDefaultReader),
    /// Bytes read that the caller had no room for yet.
    Held(Vec<u8>),
    /// The stream ended.
    Ended,
}

/// A write under way, whose writer holds the writable stream's lock, and
/// how many bytes it carries.
struct Writing(JsFuture, WritableStreamDefaultWriter, usize);

impl WorkersStream {
    fn new(socket: js::Socket, host: String) -> WorkersStream {
        WorkersStream {
            socket: Some(socket),
            host,
            reading: Reading::Idle,
            writing: None,
            closing: None,
            checking: None,
        }
    }

    /// An I/O error of `kind`, or the failed check it is when nothing has
    /// gone through since TLS started.
    fn failed(&self, kind: io::ErrorKind, error: &JsValue) -> io::Error {
        match &self.checking {
            Some(name) => io::Error::new(
                io::ErrorKind::InvalidData,
                format!("TLS with {name}: {}", js::said(error)),
            ),
            None => failed(kind, error),
        }
    }

    fn socket(&self) -> io::Result<&js::Socket> {
        self.socket
            .as_ref()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotConnected, "the stream was upgraded"))
    }

    /// This stream upgraded to TLS, checked for `name`.
    async fn start_tls(mut self: Box<Self>, name: &str) -> io::Result<Box<dyn Stream>> {
        if !matches!(self.reading, Reading::Idle) || self.writing.is_some() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "a stream is upgraded only between reads and writes",
            ));
        }
        let socket = self.socket.take().ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotConnected, "the stream was upgraded")
        })?;
        let options = Object::new();
        let refused = |error: JsValue| {
            let _ = socket.close();
            io::Error::new(io::ErrorKind::InvalidInput, js::said(&error))
        };
        if !name.eq_ignore_ascii_case(&self.host) {
            Reflect::set(
                &options,
                &JsValue::from_str("expectedServerHostname"),
                &JsValue::from_str(name),
            )
            .map_err(refused)?;
        }
        let secure = socket.start_tls(&options).map_err(refused)?;
        // From here the upgraded stream owns the connection, and closes it
        // when dropped, failed or not.
        let mut upgraded = WorkersStream::new(secure, self.host.clone());
        upgraded.checking = Some(name.to_string());
        JsFuture::from(upgraded.socket()?.opened())
            .await
            .map_err(|error| upgraded.failed(io::ErrorKind::InvalidData, &error))?;
        Ok(Box::new(upgraded) as Box<dyn Stream>)
    }
}

impl Drop for WorkersStream {
    fn drop(&mut self) {
        if let Some(socket) = self.socket.take() {
            let _ = socket.close();
        }
    }
}

/// A JavaScript error as an I/O error of `kind`.
fn failed(kind: io::ErrorKind, error: &JsValue) -> io::Error {
    io::Error::new(kind, js::said(error))
}

/// Copies what fits of `bytes` into `buf`, keeping the rest.
fn copy_into(buf: &mut ReadBuf<'_>, mut bytes: Vec<u8>) -> Reading {
    let fits = buf.remaining().min(bytes.len());
    let rest = bytes.split_off(fits);
    buf.put_slice(&bytes);
    if rest.is_empty() {
        Reading::Idle
    } else {
        Reading::Held(rest)
    }
}

impl AsyncRead for WorkersStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        loop {
            match std::mem::take(&mut self.reading) {
                Reading::Ended => {
                    self.reading = Reading::Ended;
                    return Poll::Ready(Ok(()));
                }
                Reading::Held(bytes) => {
                    self.reading = copy_into(buf, bytes);
                    return Poll::Ready(Ok(()));
                }
                Reading::Idle => {
                    let readable = self.socket()?.readable();
                    let reader = ReadableStreamDefaultReader::new(&readable)
                        .map_err(|error| failed(io::ErrorKind::Other, &error))?;
                    let read = JsFuture::from(reader.read());
                    self.reading = Reading::Waiting(read, reader);
                }
                Reading::Waiting(mut read, reader) => {
                    let Poll::Ready(result) = Pin::new(&mut read).poll(cx) else {
                        self.reading = Reading::Waiting(read, reader);
                        return Poll::Pending;
                    };
                    reader.release_lock();
                    let result = result
                        .map_err(|error| self.failed(io::ErrorKind::ConnectionReset, &error))?;
                    let done = Reflect::get(&result, &JsValue::from_str("done"))
                        .map(|done| done.is_truthy())
                        .unwrap_or(true);
                    if done {
                        self.reading = Reading::Ended;
                        return Poll::Ready(Ok(()));
                    }
                    let value = Reflect::get(&result, &JsValue::from_str("value"))
                        .map_err(|error| failed(io::ErrorKind::InvalidData, &error))?;
                    let Some(bytes) = value.dyn_ref::<Uint8Array>() else {
                        return Poll::Ready(Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "a socket read gave something other than bytes",
                        )));
                    };
                    // An empty read carries nothing: read again.
                    if bytes.length() > 0 {
                        self.checking = None;
                        self.reading = copy_into(buf, bytes.to_vec());
                        return Poll::Ready(Ok(()));
                    }
                }
            }
        }
    }
}

impl WorkersStream {
    /// Waits for the write under way, if any: how many bytes it carried.
    fn poll_written(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<Option<usize>>> {
        let Some(Writing(mut write, writer, length)) = self.writing.take() else {
            return Poll::Ready(Ok(None));
        };
        match Pin::new(&mut write).poll(cx) {
            Poll::Pending => {
                self.writing = Some(Writing(write, writer, length));
                Poll::Pending
            }
            Poll::Ready(result) => {
                writer.release_lock();
                Poll::Ready(match result {
                    Ok(_) => {
                        self.checking = None;
                        Ok(Some(length))
                    }
                    Err(error) => Err(self.failed(io::ErrorKind::BrokenPipe, &error)),
                })
            }
        }
    }
}

impl AsyncWrite for WorkersStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        // A write started earlier, whose bytes were these, ends here.
        if let Some(written) = std::task::ready!(self.poll_written(cx))? {
            return Poll::Ready(Ok(written));
        }
        let writable = self.socket()?.writable();
        let writer = WritableStreamDefaultWriter::new(&writable)
            .map_err(|error| failed(io::ErrorKind::Other, &error))?;
        let chunk = Uint8Array::from(bytes);
        let write = JsFuture::from(writer.write_with_chunk(&chunk));
        self.writing = Some(Writing(write, writer, bytes.len()));
        match self.poll_written(cx) {
            Poll::Ready(Ok(Some(written))) => Poll::Ready(Ok(written)),
            Poll::Ready(Ok(None)) => Poll::Ready(Ok(0)),
            Poll::Ready(Err(error)) => Poll::Ready(Err(error)),
            Poll::Pending => Poll::Pending,
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.poll_written(cx).map_ok(|_| ())
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        std::task::ready!(self.poll_written(cx))?;
        let mut closing = match self.closing.take() {
            Some(closing) => closing,
            None => JsFuture::from(self.socket()?.writable().close()),
        };
        match Pin::new(&mut closing).poll(cx) {
            Poll::Pending => {
                self.closing = Some(closing);
                Poll::Pending
            }
            Poll::Ready(result) => Poll::Ready(
                result
                    .map(|_| ())
                    .map_err(|error| failed(io::ErrorKind::BrokenPipe, &error)),
            ),
        }
    }
}
