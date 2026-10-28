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
//! - **A stream reads and writes through one reader and one writer for
//!   its socket's life**, each taken at its first read or
//!   write and released only to upgrade it, between them: taking a reader
//!   or a writer and releasing its lock for every read and write was about
//!   half of what workerd spent on a booking (§2.6, "The Workers host"). It closes its socket
//!   when dropped.
//! - **A connection ends within the invocation that opened it**.
//!   workerd never settles a promise of an invocation that has
//!   ended, and what waits on one is never given back: before, each
//!   booking's connection was left waiting for its goodbye to go, some
//!   20 KB of the module's memory kept for the isolate's life (§2.6, "The Workers host"). So
//!   the transport counts the connections' tasks it has started
//!   ([`Tasks`]), which the invocation's host, once dropped, keeps the
//!   invocation for ([`crate::WorkersHost`]); and shutting a stream down
//!   closes its writable side after the driver's goodbye (PostgreSQL's
//!   `Terminate`), then, with a read still waiting, the socket, and waits
//!   for that read to settle, so nothing is left waiting once the task has
//!   ended.

use std::any::Any;
use std::cell::{Cell, RefCell};
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

use jadpo_drivers::{Endpoint, Stream, Transport, Trust};
use jadpo_runtime::LocalFuture;
use js_sys::{Object, Reflect, Uint8Array};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{ReadableStreamDefaultReader, ReadableStreamReadResult, WritableStreamDefaultWriter};

use crate::js;
use crate::opening::Opening;

/// The drivers' transport over Workers' `connect()`, counting the
/// connections it opens among its invocation's six and the tasks it
/// starts for them. A default one has six places and its count of its
/// own; an invocation's host gives its drivers one sharing the
/// invocation's ([`crate::WorkersHost::supplies`]).
#[derive(Clone, Debug, Default)]
pub struct WorkersTransport {
    opening: Rc<Opening>,
    tasks: Rc<Tasks>,
}

impl WorkersTransport {
    /// A transport whose connections take places among `opening`'s.
    pub fn counted(opening: Rc<Opening>) -> WorkersTransport {
        WorkersTransport {
            opening,
            tasks: Rc::default(),
        }
    }

    /// Counts the tasks it starts in `tasks` (chunk 8.4e).
    pub fn tracking(mut self, tasks: Rc<Tasks>) -> WorkersTransport {
        self.tasks = tasks;
        self
    }
}

/// The connections' tasks a transport has started that haven't ended
/// (chunk 8.4e): what an invocation's host, once dropped, keeps the
/// invocation for, since dropping its drivers is what starts each
/// connection's goodbye.
#[derive(Debug, Default)]
pub struct Tasks {
    running: Cell<usize>,
    /// Who waits for the last to end.
    waiting: RefCell<Option<Waker>>,
}

impl Tasks {
    /// How many are running now.
    pub fn running(&self) -> usize {
        self.running.get()
    }

    /// Settles once none is running.
    pub async fn ended(self: Rc<Self>) {
        std::future::poll_fn(|cx| {
            if self.running.get() == 0 {
                return Poll::Ready(());
            }
            *self.waiting.borrow_mut() = Some(cx.waker().clone());
            Poll::Pending
        })
        .await;
    }
}

/// One running task's place in its [`Tasks`], given up when it ends or is
/// dropped.
struct Running(Rc<Tasks>);

impl Running {
    fn start(tasks: Rc<Tasks>) -> Running {
        tasks.running.set(tasks.running.get() + 1);
        Running(tasks)
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        let tasks = &self.0;
        tasks.running.set(tasks.running.get().saturating_sub(1));
        if tasks.running.get() == 0
            && let Some(waiting) = tasks.waiting.borrow_mut().take()
        {
            waiting.wake();
        }
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

    /// Beside the work that started it, counted among its [`Tasks`] until
    /// it ends.
    fn spawn(&self, task: LocalFuture<'static, ()>) {
        let running = Running::start(Rc::clone(&self.tasks));
        wasm_bindgen_futures::spawn_local(async move {
            let _running = running;
            task.await;
        });
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
    /// The socket's reader and writer, each taken at its first read or
    /// write and kept, holding its stream's lock, until the stream is
    /// upgraded or dropped.
    reader: Option<ReadableStreamDefaultReader>,
    writer: Option<WritableStreamDefaultWriter>,
    reading: Reading,
    writing: Option<Writing>,
    /// Closing the writable side, until it has closed.
    closing: Option<JsFuture>,
    /// Whether the writable side has closed.
    shut: bool,
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
    /// Waiting on the reader's read.
    Waiting(JsFuture),
    /// Bytes read that the caller had no room for yet.
    Held(Vec<u8>),
    /// The stream ended.
    Ended,
}

/// A write under way, and how many bytes it carries.
struct Writing(JsFuture, usize);

impl WorkersStream {
    fn new(socket: js::Socket, host: String) -> WorkersStream {
        WorkersStream {
            socket: Some(socket),
            host,
            reader: None,
            writer: None,
            reading: Reading::Idle,
            writing: None,
            closing: None,
            shut: false,
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

    /// The socket's reader, taken at the first read.
    fn reader(&mut self) -> io::Result<&ReadableStreamDefaultReader> {
        let reader = match self.reader.take() {
            Some(reader) => reader,
            None => ReadableStreamDefaultReader::new(&self.socket()?.readable())
                .map_err(|error| failed(io::ErrorKind::Other, &error))?,
        };
        Ok(self.reader.insert(reader))
    }

    /// The socket's writer, taken at the first write.
    fn writer(&mut self) -> io::Result<&WritableStreamDefaultWriter> {
        let writer = match self.writer.take() {
            Some(writer) => writer,
            None => WritableStreamDefaultWriter::new(&self.socket()?.writable())
                .map_err(|error| failed(io::ErrorKind::Other, &error))?,
        };
        Ok(self.writer.insert(writer))
    }

    /// This stream upgraded to TLS, checked for `name`.
    async fn start_tls(mut self: Box<Self>, name: &str) -> io::Result<Box<dyn Stream>> {
        if !matches!(self.reading, Reading::Idle) || self.writing.is_some() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "a stream is upgraded only between reads and writes",
            ));
        }
        // `startTls()` takes over the socket's streams, which no reader or
        // writer may hold then.
        if let Some(reader) = self.reader.take() {
            reader.release_lock();
        }
        if let Some(writer) = self.writer.take() {
            writer.release_lock();
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

/// Copies `bytes` read from the socket into `buf`: straight from
/// JavaScript's memory when they fit, as they mostly do, else through a
/// copy of them whose rest is kept.
fn read_into(buf: &mut ReadBuf<'_>, bytes: &Uint8Array) -> Reading {
    let length = bytes.length() as usize;
    if length > buf.remaining() {
        return copy_into(buf, bytes.to_vec());
    }
    bytes.copy_to(buf.initialize_unfilled_to(length));
    buf.advance(length);
    Reading::Idle
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
                    let read = JsFuture::from(self.reader()?.read());
                    self.reading = Reading::Waiting(read);
                }
                Reading::Waiting(mut read) => {
                    let Poll::Ready(result) = Pin::new(&mut read).poll(cx) else {
                        self.reading = Reading::Waiting(read);
                        return Poll::Pending;
                    };
                    let result = result
                        .map_err(|error| self.failed(io::ErrorKind::ConnectionReset, &error))?
                        .unchecked_into::<ReadableStreamReadResult>();
                    if result.get_done().unwrap_or(true) {
                        self.reading = Reading::Ended;
                        return Poll::Ready(Ok(()));
                    }
                    let value = result.get_value();
                    let Some(bytes) = value.dyn_ref::<Uint8Array>() else {
                        return Poll::Ready(Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "a socket read gave something other than bytes",
                        )));
                    };
                    // An empty read carries nothing: read again.
                    if bytes.length() > 0 {
                        self.checking = None;
                        self.reading = read_into(buf, bytes);
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
        let Some(Writing(mut write, length)) = self.writing.take() else {
            return Poll::Ready(Ok(None));
        };
        match Pin::new(&mut write).poll(cx) {
            Poll::Pending => {
                self.writing = Some(Writing(write, length));
                Poll::Pending
            }
            Poll::Ready(result) => Poll::Ready(match result {
                Ok(_) => {
                    self.checking = None;
                    Ok(Some(length))
                }
                Err(error) => Err(self.failed(io::ErrorKind::BrokenPipe, &error)),
            }),
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
        let chunk = Uint8Array::from(bytes);
        let write = JsFuture::from(self.writer()?.write_with_chunk(&chunk));
        self.writing = Some(Writing(write, bytes.len()));
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

    /// Closes the writable side once the writes under way have gone; then,
    /// with a read still waiting, closes the socket and waits for that
    /// read to settle, which it does only while the invocation lasts
    /// (chunk 8.4e).
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        std::task::ready!(self.poll_written(cx))?;
        if !self.shut {
            // Through the writer, which holds the writable stream's lock.
            let mut closing = match self.closing.take() {
                Some(closing) => closing,
                None => JsFuture::from(self.writer()?.close()),
            };
            let Poll::Ready(closed) = Pin::new(&mut closing).poll(cx) else {
                self.closing = Some(closing);
                return Poll::Pending;
            };
            closed.map_err(|error| failed(io::ErrorKind::BrokenPipe, &error))?;
            self.shut = true;
            if let (Reading::Waiting(_), Some(socket)) = (&self.reading, &self.socket) {
                let _ = socket.close();
            }
        }
        if let Reading::Waiting(read) = &mut self.reading {
            // Whatever it gives, nothing more is read.
            let _ = std::task::ready!(Pin::new(read).poll(cx));
            self.reading = Reading::Ended;
        }
        Poll::Ready(Ok(()))
    }
}
