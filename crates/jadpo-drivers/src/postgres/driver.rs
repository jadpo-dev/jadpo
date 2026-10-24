//! [`Postgres`]: the runtime's `Database` over tokio-postgres.

use std::cell::RefCell;
use std::error::Error as _;
use std::fmt;
use std::future::Future;
use std::rc::Rc;
use std::task::{Poll, Waker};

use bytes::BytesMut;
use jadpo_runtime::{Database, Isolation, LocalFuture, Row, SqlValue, StoreError, Transaction};
use tokio_postgres::config::Host;
use tokio_postgres::types::{FromSql, IsNull, Kind, ToSql, Type, to_sql_checked};
use tokio_postgres::{Client, Config, NoTls};

use super::classify::{Cause, classify};
use super::wire;
use crate::{Endpoint, Stream, Transport, Trust, within};

/// How many connections a store keeps open between transactions.
const MAX_IDLE: usize = 8;

/// The most connections a store has open at once, idle or in a
/// transaction; a transaction that would open one more waits for one to
/// end. Natively each worker thread has its stores, so a server opens at
/// most this many per worker.
pub const MAX_OPEN: usize = 10;

/// The connections a store may open: a count, and who is waiting for one.
#[derive(Default)]
struct Slots {
    open: std::cell::Cell<usize>,
    waiting: RefCell<Vec<Waker>>,
}

/// One open connection's place among [`MAX_OPEN`], given back when it is
/// dropped, with its connection.
struct Slot(Rc<Slots>);

impl Drop for Slot {
    fn drop(&mut self) {
        self.0.open.set(self.0.open.get().saturating_sub(1));
        self.0.wake();
    }
}

impl Slots {
    /// Wakes every transaction waiting for a connection, now that one has
    /// gone idle or closed: the first to look takes it, and the rest wait
    /// again. Waking them all means one that gave up waiting (its request
    /// ran out of time) never holds the others back.
    fn wake(&self) {
        let waiting: Vec<Waker> = self.waiting.borrow_mut().drain(..).collect();
        for next in waiting {
            next.wake();
        }
    }
}

/// How many bytes a connection has written to its stream, counted as they
/// go ([`Counted`]).
type Written = Rc<std::cell::Cell<u64>>;

/// A connection kept between transactions: its client, what it has
/// written, and its place among [`MAX_OPEN`].
type Idle = (Client, Written, Slot);

/// What a transaction begins on: an idle connection, or the place for a
/// new one.
enum Got {
    Idle(Idle),
    New(Slot),
}

/// An idle connection, or a place for a new one once fewer than
/// [`MAX_OPEN`] are open, waiting until there is either.
async fn next(slots: &Rc<Slots>, idle: &RefCell<Vec<Idle>>) -> Got {
    std::future::poll_fn(|cx| {
        if let Some(kept) = idle.borrow_mut().pop() {
            return Poll::Ready(Got::Idle(kept));
        }
        if slots.open.get() < MAX_OPEN {
            slots.open.set(slots.open.get() + 1);
            return Poll::Ready(Got::New(Slot(Rc::clone(slots))));
        }
        slots.waiting.borrow_mut().push(cx.waker().clone());
        Poll::Pending
    })
    .await
}

/// PostgreSQL's default port.
const DEFAULT_PORT: u16 = 5432;

/// Begins a transaction at `isolation`: the level is the transaction's
/// own, never the session's, since Hyperdrive pools by transaction (§2.6).
fn begin_sql(isolation: Isolation) -> &'static str {
    match isolation {
        Isolation::ReadCommitted => "BEGIN ISOLATION LEVEL READ COMMITTED",
        Isolation::RepeatableRead => "BEGIN ISOLATION LEVEL REPEATABLE READ",
    }
}

/// How a store's connections are secured (area 5, "PostgreSQL over TLS";
/// chunk 8.1n): its connection in `app.jadpo` decides, never its address.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Security {
    /// TLS, the server's certificate checked against the host's roots,
    /// but to this machine (a Unix socket, `localhost`, `127.0.0.1`,
    /// `::1`), which is reached without it.
    #[default]
    Roots,
    /// TLS everywhere but a Unix socket, the server's certificate checked
    /// against only these authorities, each in DER: `authority:
    /// config.<name>`.
    Authorities(Vec<Vec<u8>>),
    /// No TLS anywhere: `tls: false`.
    Plain,
}

impl Security {
    /// Whom the upgrade to `endpoint` trusts, or `None` when it is reached
    /// without TLS.
    fn trust(&self, endpoint: &Endpoint) -> Option<Trust<'_>> {
        let Endpoint::Tcp { host, .. } = endpoint else {
            // PostgreSQL offers no TLS over a Unix socket, which is this
            // machine.
            return None;
        };
        match self {
            Security::Plain => None,
            Security::Roots if jadpo_std::database_tls::is_this_machine(host) => None,
            Security::Roots => Some(Trust::Roots),
            Security::Authorities(authorities) => Some(Trust::Authorities(authorities)),
        }
    }
}

/// How long one attempt at a server may take, from opening the socket to
/// its being ready for statements, when the address sets no
/// `connect_timeout` (or sets it to 0, which libpq reads as forever).
const CONNECT_MS: u64 = 10_000;

/// How long a server has to answer the SSL request and finish the TLS
/// handshake.
const HANDSHAKE_MS: u64 = 10_000;

/// PostgreSQL's SSL request: its length, 8, and the code 80877103.
const SSL_REQUEST: [u8; 8] = [0, 0, 0, 8, 0x04, 0xd2, 0x16, 0x2f];

/// `stream` to `host` as TLS: PostgreSQL's SSL request, then, once the
/// server answers `S`, the host's upgrade, checking its certificate
/// against `trust` and that it names `host`. A server that offers no TLS,
/// or a certificate the check refuses, is a fault naming the server, never
/// a store that is down for now, since trying again changes nothing; a
/// connection that broke off is unavailable.
async fn secured(
    transport: &dyn Transport,
    mut stream: Box<dyn Stream>,
    host: &str,
    trust: Trust<'_>,
) -> Result<Box<dyn Stream>, StoreError> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let asked = within(transport, HANDSHAKE_MS, async {
        stream.write_all(&SSL_REQUEST).await?;
        stream.flush().await?;
        // One byte, so nothing the server sends past it is read as
        // anything but TLS.
        let mut answer = [0u8; 1];
        stream.read_exact(&mut answer).await?;
        let [byte] = answer;
        Ok::<u8, std::io::Error>(byte)
    })
    .await;
    match asked {
        Some(Ok(b'S')) => {}
        Some(Ok(_)) => {
            return Err(StoreError::Fault(format!(
                "the database server at `{host}` offers no TLS, and a database elsewhere than this machine is reached only over TLS unless its connection in `app.jadpo` says `tls: false`"
            )));
        }
        Some(Err(_)) | None => return Err(StoreError::Unavailable),
    }
    use std::io::ErrorKind;
    match within(
        transport,
        HANDSHAKE_MS,
        transport.upgrade(stream, host, trust),
    )
    .await
    {
        Some(Ok(secure)) => Ok(secure),
        Some(Err(error))
            if matches!(
                error.kind(),
                ErrorKind::InvalidData | ErrorKind::InvalidInput | ErrorKind::Unsupported
            ) =>
        {
            Err(failed_check(host, trust, &error))
        }
        Some(Err(_)) | None => Err(StoreError::Unavailable),
    }
}

/// The fault a certificate check that failed is: the server named, and
/// whom it was checked against, since trying again changes nothing.
fn failed_check(host: &str, trust: Trust<'_>, error: &std::io::Error) -> StoreError {
    let whom = match trust {
        Trust::Roots => "the public authorities Jadpo trusts",
        Trust::Authorities(_) => "the authority its connection's `authority:` names",
    };
    StoreError::Fault(format!(
        "the database server at `{host}` failed the certificate check against {whom}: {error}"
    ))
}

/// A stream just upgraded to TLS, keeping the certificate check its first
/// read or write failed, if one did: Workers opens an upgraded socket at
/// once and checks the server's certificate as the first bytes go out,
/// saying so only then (chunk 8.2i), where a native host's upgrade has
/// already refused it.
struct Checked {
    inner: Box<dyn Stream>,
    /// Whether anything has gone through, after which a failure is the
    /// connection's.
    through: bool,
    refused: Rc<RefCell<Option<std::io::Error>>>,
}

impl Checked {
    fn note<T>(
        &mut self,
        polled: Poll<std::io::Result<T>>,
        moved: impl Fn(&T) -> bool,
    ) -> Poll<std::io::Result<T>> {
        match &polled {
            Poll::Ready(Ok(done)) if moved(done) => self.through = true,
            Poll::Ready(Err(error))
                if !self.through && error.kind() == std::io::ErrorKind::InvalidData =>
            {
                *self.refused.borrow_mut() =
                    Some(std::io::Error::new(error.kind(), error.to_string()));
            }
            _ => {}
        }
        polled
    }
}

impl tokio::io::AsyncRead for Checked {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let before = buf.filled().len();
        let polled = std::pin::Pin::new(&mut self.inner).poll_read(cx, buf);
        let read = buf.filled().len() > before;
        self.note(polled, |_| read)
    }
}

impl tokio::io::AsyncWrite for Checked {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        bytes: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let polled = std::pin::Pin::new(&mut self.inner).poll_write(cx, bytes);
        self.note(polled, |written| *written > 0)
    }

    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<std::io::Result<()>> {
        let polled = std::pin::Pin::new(&mut self.inner).poll_flush(cx);
        self.note(polled, |()| false)
    }

    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

/// What a store's transactions share: where it connects and the
/// connections it keeps.
struct Pool {
    config: Config,
    endpoints: Vec<Endpoint>,
    security: Security,
    transport: Rc<dyn Transport>,
    /// How long one attempt at an endpoint may take: the address's
    /// `connect_timeout`, or [`CONNECT_MS`].
    connect_ms: u64,
    idle: RefCell<Vec<Idle>>,
    slots: Rc<Slots>,
}

impl Pool {
    /// Opens a connection, trying each endpoint in turn, over TLS where
    /// its security says to; the platform's own pooler (Hyperdrive, on
    /// Workers) without, since the pooler uses TLS to the database (chunk
    /// 8.2i).
    async fn connect(&self) -> Result<(Client, Written), StoreError> {
        let mut last = StoreError::Unavailable;
        for endpoint in &self.endpoints {
            // Each attempt within the address's `connect_timeout`, or ten
            // seconds, from opening the socket to the server being ready,
            // so a server that never answers is the next one's turn
            // (chunk 8.4d).
            match within(&*self.transport, self.connect_ms, self.connect_to(endpoint)).await {
                Some(Ok(connected)) => return Ok(connected),
                Some(Err(Attempt::Next(error))) => last = error,
                Some(Err(Attempt::Stop(error))) => return Err(error),
                None => last = StoreError::Unavailable,
            }
        }
        Err(last)
    }

    /// One attempt at `endpoint`.
    async fn connect_to(&self, endpoint: &Endpoint) -> Result<(Client, Written), Attempt> {
        let Ok(stream) = self.transport.connect(endpoint).await else {
            return Err(Attempt::Next(StoreError::Unavailable));
        };
        let trust = if self.transport.is_pooler(endpoint) {
            None
        } else {
            self.security.trust(endpoint)
        };
        // A check Workers reports at the first write over TLS.
        let refused = Rc::new(RefCell::new(None));
        let stream = match (trust, endpoint) {
            (Some(trust), Endpoint::Tcp { host, .. }) => {
                match secured(&*self.transport, stream, host, trust).await {
                    Ok(secure) => Box::new(Checked {
                        inner: secure,
                        through: false,
                        refused: Rc::clone(&refused),
                    }) as Box<dyn Stream>,
                    Err(StoreError::Unavailable) => {
                        return Err(Attempt::Next(StoreError::Unavailable));
                    }
                    Err(refused) => return Err(Attempt::Stop(refused)),
                }
            }
            _ => stream,
        };
        let written = Written::default();
        let stream = Counted {
            inner: stream,
            written: Rc::clone(&written),
        };
        let connected = self.config.connect_raw(stream, NoTls).await;
        if let (Err(_), Some(error), Some(trust), Endpoint::Tcp { host, .. }) =
            (&connected, refused.borrow_mut().take(), trust, endpoint)
        {
            return Err(Attempt::Stop(failed_check(host, trust, &error)));
        }
        match connected {
            Ok((client, connection)) => {
                self.transport.spawn(Box::pin(async move {
                    // The connection ends when its client is dropped or
                    // the server goes; either way its client sees it.
                    let _ = connection.await;
                }));
                Ok((client, written))
            }
            Err(error) => {
                let error = store_error(&error);
                if error == StoreError::Unavailable {
                    Err(Attempt::Next(error))
                } else {
                    Err(Attempt::Stop(error))
                }
            }
        }
    }

    /// A connection to run on: an idle one, which may have been closed by
    /// the server since (`reused`), or a new one; with none idle and
    /// [`MAX_OPEN`] open, it waits for one to end.
    async fn take(&self) -> Result<Conn, StoreError> {
        loop {
            match next(&self.slots, &self.idle).await {
                Got::Idle((client, written, slot)) => {
                    if client.is_closed() {
                        continue;
                    }
                    return Ok(Conn {
                        client,
                        written,
                        slot,
                        reused: true,
                    });
                }
                Got::New(slot) => {
                    let (client, written) = self.connect().await?;
                    return Ok(Conn {
                        client,
                        written,
                        slot,
                        reused: false,
                    });
                }
            }
        }
    }

    /// Keeps a connection for the next transaction, if it is still sound
    /// and fewer than [`MAX_IDLE`] are kept.
    fn give_back(&self, conn: Conn) {
        let Conn {
            client,
            written,
            slot,
            ..
        } = conn;
        let mut kept = self.idle.borrow_mut();
        if !client.is_closed() && kept.len() < MAX_IDLE {
            kept.push((client, written, slot));
            drop(kept);
            self.slots.wake();
        }
    }
}

/// How one attempt at an endpoint ended, when it didn't connect: the next
/// endpoint's turn, or an answer that trying another doesn't change.
enum Attempt {
    Next(StoreError),
    Stop(StoreError),
}

/// A connection's stream, counting the bytes written to it: once any of a
/// statement's have gone, the server may have run it.
struct Counted {
    inner: Box<dyn Stream>,
    written: Written,
}

impl tokio::io::AsyncRead for Counted {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl tokio::io::AsyncWrite for Counted {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        bytes: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let polled = std::pin::Pin::new(&mut self.inner).poll_write(cx, bytes);
        if let Poll::Ready(Ok(written)) = &polled {
            let total = self.written.get().saturating_add(*written as u64);
            self.written.set(total);
        }
        polled
    }

    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

/// One connection taken from the pool.
struct Conn {
    client: Client,
    /// What it has written, so a statement is sent again only when none of
    /// it went.
    written: Written,
    /// Its place among the store's [`MAX_OPEN`].
    slot: Slot,
    /// It was idle, so the server may have closed it meanwhile: a first
    /// round trip that finds it gone is sent again on another.
    reused: bool,
}

/// Runs `a` and `b` together, `a` polled first, until both finish: what
/// sends a transaction's `BEGIN` and its first statement in one round
/// trip, since each sends its messages when first polled and the
/// connection writes them out together.
async fn both<A: Future, B: Future>(a: A, b: B) -> (A::Output, B::Output) {
    let mut a = std::pin::pin!(a);
    let mut b = std::pin::pin!(b);
    let mut left = None;
    let mut right = None;
    std::future::poll_fn(|cx| {
        if left.is_none()
            && let Poll::Ready(value) = a.as_mut().poll(cx)
        {
            left = Some(value);
        }
        if right.is_none()
            && let Poll::Ready(value) = b.as_mut().poll(cx)
        {
            right = Some(value);
        }
        match (left.take(), right.take()) {
            (Some(a), Some(b)) => Poll::Ready((a, b)),
            (a, b) => {
                left = a;
                right = b;
                Poll::Pending
            }
        }
    })
    .await
}

/// A statement as a transaction or [`Database::query_once`] sends it.
enum Op<'a> {
    Query(&'a str, &'a [(&'a (dyn ToSql + Sync), Type)]),
    Execute(&'a str, &'a [(&'a (dyn ToSql + Sync), Type)]),
    Batch(&'a str),
}

/// What a statement gave back.
enum Done {
    Rows(Vec<tokio_postgres::Row>),
    Count(u64),
    Ran,
}

async fn send(client: &Client, op: &Op<'_>) -> Result<Done, tokio_postgres::Error> {
    match op {
        Op::Query(sql, params) => client.query_typed(sql, params).await.map(Done::Rows),
        Op::Execute(sql, params) => client.execute_typed(sql, params).await.map(Done::Count),
        Op::Batch(sql) => client.batch_execute(sql).await.map(|()| Done::Ran),
    }
}

/// A PostgreSQL store, reached through the host's [`Transport`]. It keeps
/// a few connections open between transactions and begins each one at the
/// level it is asked for: read committed for an action that loads no
/// record, repeatable read for any other (area 6). A transaction's `BEGIN`
/// goes with its first statement, in the same round trip, and the
/// runtime's own short steps run as one statement of their own
/// ([`Database::query_once`]), so neither waits on a `BEGIN` or a
/// `COMMIT` of its own (chunk 4.10).
///
/// Its connections are secured as its [`Security`] says (chunk 8.1n):
/// by default TLS, checked against the host's roots, to any server but
/// this machine, after PostgreSQL's SSL request, through the transport's
/// upgrade. An address that sets TLS itself (`sslmode`) is refused, since
/// the connection decides.
pub struct Postgres {
    pool: Rc<Pool>,
}

impl fmt::Debug for Postgres {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Never the URL: it may hold a password.
        f.debug_struct("Postgres")
            .field("endpoints", &self.pool.endpoints)
            .field("idle", &self.pool.idle.borrow().len())
            .finish_non_exhaustive()
    }
}

impl Postgres {
    /// A store for `url`, in either of libpq's forms
    /// (`postgresql://user@host:port/db` or `host=… dbname=…`); a host
    /// starting with `/` is the folder of a Unix socket. Nothing connects
    /// until the first transaction begins.
    pub fn new(url: &str, transport: Rc<dyn Transport>) -> Result<Postgres, StoreError> {
        Postgres::secured(url, transport, Security::Roots)
    }

    /// A store for `url`, its connections secured as `security` says
    /// (chunk 8.1n). An address that sets TLS itself (`sslmode`,
    /// `sslrootcert` and the rest) is refused.
    pub fn secured(
        url: &str,
        transport: Rc<dyn Transport>,
        security: Security,
    ) -> Result<Postgres, StoreError> {
        if let Some(key) = jadpo_std::database_tls::tls_parameter(url) {
            return Err(StoreError::Fault(format!(
                "the database URL sets `{key}`, which its connection in `app.jadpo` decides: TLS always, but to this machine"
            )));
        }
        let mut config: Config = url.parse().map_err(|error| {
            StoreError::Fault(format!(
                "the database URL isn't valid: {}",
                describe(&error)
            ))
        })?;
        // TLS, where there is any, is set up before tokio-postgres sees
        // the stream.
        config.ssl_mode(tokio_postgres::config::SslMode::Disable);
        let endpoints = endpoints(&config)?;
        let connect_ms = config.get_connect_timeout().map_or(CONNECT_MS, |timeout| {
            u64::try_from(timeout.as_millis()).unwrap_or(u64::MAX)
        });
        Ok(Postgres {
            pool: Rc::new(Pool {
                config,
                endpoints,
                security,
                transport,
                connect_ms,
                idle: RefCell::new(Vec::new()),
                slots: Rc::new(Slots::default()),
            }),
        })
    }

    /// Where the store connects, in the order it tries them.
    pub fn endpoints(&self) -> &[Endpoint] {
        &self.pool.endpoints
    }

    /// Runs statements without parameters outside any transaction, on a
    /// connection of their own: for what PostgreSQL refuses inside one,
    /// such as `CREATE DATABASE`. Tools and the test harness use it;
    /// actions never do, since everything they run is in their
    /// transaction.
    pub fn batch_outside_transaction<'a>(
        &'a self,
        sql: &'a str,
    ) -> LocalFuture<'a, Result<(), StoreError>> {
        Box::pin(async move {
            let (client, _) = self.pool.connect().await?;
            client
                .batch_execute(sql)
                .await
                .map_err(|error| store_error(&error))
        })
    }
}

impl Database for Postgres {
    /// Takes a connection, opening one when none is idle, and leaves the
    /// `BEGIN` to go with the first statement.
    fn begin_at(
        &self,
        isolation: Isolation,
    ) -> LocalFuture<'_, Result<Box<dyn Transaction>, StoreError>> {
        Box::pin(async move {
            let conn = self.pool.take().await?;
            Ok(Box::new(PgTransaction {
                conn: Some(conn),
                pool: Rc::clone(&self.pool),
                begin: Some(begin_sql(isolation)),
                failed: None,
                saved: Vec::new(),
                read_end: ReadEnd::Ordinary,
            }) as Box<dyn Transaction>)
        })
    }

    /// The statement on an idle connection, or a new one, outside any
    /// transaction, so PostgreSQL commits it as it runs. A connection the
    /// server closed while it was idle never ran it, so the statement is
    /// sent again on another, but only when none of it was written: once
    /// it went, the server may have committed it, and it is never sent
    /// twice (chunk 8.4d).
    fn query_once<'a>(
        &'a self,
        sql: &'a str,
        params: &'a [SqlValue],
    ) -> LocalFuture<'a, Result<Vec<Row>, StoreError>> {
        Box::pin(async move {
            let bound = bind(params)?;
            let typed = typed(&bound);
            loop {
                let conn = self.pool.take().await?;
                let before = conn.written.get();
                match conn.client.query_typed(sql, &typed).await {
                    Ok(rows) => {
                        self.pool.give_back(conn);
                        return rows.iter().map(read_row).collect();
                    }
                    Err(error) => {
                        let error = store_error(&error);
                        if conn.reused
                            && error == StoreError::Unavailable
                            && conn.written.get() == before
                        {
                            continue;
                        }
                        self.pool.give_back(conn);
                        return Err(error);
                    }
                }
            }
        })
    }
}

enum ReadEnd {
    Ordinary,
    /// The future owns the connection: cancelling it drops that connection.
    Finalizing,
    /// All three protocol operations finished; consumed by commit/rollback.
    Completed(Result<(), StoreError>),
}

/// An open transaction on one connection. Dropped without a commit or a
/// rollback, it drops its connection, and closing the connection aborts
/// the transaction on the server.
struct PgTransaction {
    /// Its connection, except while the terminal-read future owns it or
    /// after that future was cancelled or the connection was lost.
    conn: Option<Conn>,
    pool: Rc<Pool>,
    /// The `BEGIN` its first statement sends with it, until it is sent.
    begin: Option<&'static str>,
    /// The first error a statement gave: PostgreSQL aborts the transaction
    /// then, and answers a later `COMMIT` by rolling back.
    failed: Option<StoreError>,
    /// What `failed` was at each savepoint still open, latest last: going
    /// back to one undoes the error after it.
    saved: Vec<Option<StoreError>>,
    read_end: ReadEnd,
}

impl PgTransaction {
    fn note<T>(&mut self, result: Result<T, StoreError>) -> Result<T, StoreError> {
        if let Err(error) = &result
            && self.failed.is_none()
        {
            self.failed = Some(error.clone());
        }
        result
    }

    /// Sends `op`, with the transaction's `BEGIN` before it in the same
    /// round trip when it is the first. A connection that was idle and
    /// turns out closed began nothing and ran nothing, so both go again
    /// on another. `BEGIN` fails only when its connection does, and then
    /// the statement after it does too.
    async fn run(&mut self, op: Op<'_>) -> Result<Done, StoreError> {
        if !matches!(self.read_end, ReadEnd::Ordinary) {
            return Err(StoreError::Fault(
                "a statement after a terminal read".into(),
            ));
        }
        let Some(begin) = self.begin.take() else {
            let done = match &self.conn {
                Some(conn) => send(&conn.client, &op)
                    .await
                    .map_err(|error| store_error(&error)),
                None => Err(StoreError::Unavailable),
            };
            return self.note(done);
        };
        loop {
            let Some(conn) = &self.conn else {
                return self.note(Err(StoreError::Unavailable));
            };
            let (begun, done) =
                both(conn.client.batch_execute(begin), send(&conn.client, &op)).await;
            let error = match begun {
                Ok(()) => return self.note(done.map_err(|error| store_error(&error))),
                Err(error) => store_error(&error),
            };
            if conn.reused && error == StoreError::Unavailable {
                // Its place goes back before another is taken.
                self.conn = None;
                match self.pool.take().await {
                    Ok(conn) => {
                        self.conn = Some(conn);
                        continue;
                    }
                    Err(error) => return self.note(Err(error)),
                }
            }
            return self.note(Err(error));
        }
    }

    /// A proven final read: queue the whole transaction in order, await
    /// every response and keep the ending separate from the read's result.
    async fn last_read(&mut self, sql: &str, params: &[SqlValue]) -> Result<Vec<Row>, StoreError> {
        if !matches!(self.read_end, ReadEnd::Ordinary)
            || self.begin.is_none()
            || self.failed.is_some()
            || !self.saved.is_empty()
            || params
                .iter()
                .any(|value| wire::param_oid(value.ty()) == wire::oid::UNSPECIFIED)
        {
            return self.query(sql, params).await;
        }
        let bound = bind(params)?;
        let typed = typed(&bound);
        let Some(begin) = self.begin.take() else {
            return self.query(sql, params).await;
        };
        self.read_end = ReadEnd::Finalizing;
        let mut connection = self.conn.take();
        loop {
            let Some(conn) = connection.take() else {
                self.read_end = ReadEnd::Completed(Err(StoreError::Unavailable));
                return self.note(Err(StoreError::Unavailable));
            };
            let ((begun, read), ended) = both(
                both(
                    conn.client.batch_execute(begin),
                    conn.client.query_typed(sql, &typed),
                ),
                conn.client.batch_execute("COMMIT"),
            )
            .await;
            let begun = begun.map_err(|error| store_error(&error));
            if begun == Err(StoreError::Unavailable) && conn.reused {
                // As for an ordinary first statement, a stale idle connection
                // can be replaced. No retry after a successful BEGIN/read.
                drop(conn);
                match self.pool.take().await {
                    Ok(conn) => {
                        connection = Some(conn);
                        continue;
                    }
                    Err(error) => {
                        self.read_end = ReadEnd::Completed(Err(error.clone()));
                        return self.note(Err(error));
                    }
                }
            }
            let ended = ended.map_err(|error| store_error(&error));
            if ended.is_ok() {
                self.conn = Some(conn);
            }
            self.read_end = ReadEnd::Completed(ended);
            let rows = self.note(begun.and(read.map_err(|error| store_error(&error))))?;
            // Decoding errors were never statement errors in the ordinary
            // path; keep the same classification and boundary precedence.
            return rows.iter().map(read_row).collect();
        }
    }

    /// Ends the transaction with `sql`, keeping the connection for the next
    /// one if it is still sound. One that never began sends nothing.
    async fn end(self, sql: &str) -> Result<(), StoreError> {
        let PgTransaction {
            conn,
            pool,
            begin,
            read_end,
            ..
        } = self;
        match read_end {
            ReadEnd::Finalizing => return Err(StoreError::Unavailable),
            ReadEnd::Completed(result) => {
                result?;
                let conn = conn.ok_or(StoreError::Unavailable)?;
                pool.give_back(conn);
                return Ok(());
            }
            ReadEnd::Ordinary => {}
        }
        let Some(conn) = conn else {
            return Err(StoreError::Unavailable);
        };
        if begin.is_none() {
            // A connection that couldn't end its transaction is dropped.
            conn.client
                .batch_execute(sql)
                .await
                .map_err(|error| store_error(&error))?;
        }
        pool.give_back(conn);
        Ok(())
    }
}

impl Transaction for PgTransaction {
    fn query_last_read<'a>(
        &'a mut self,
        sql: &'a str,
        params: &'a [SqlValue],
    ) -> LocalFuture<'a, Result<Vec<Row>, StoreError>> {
        Box::pin(async move { self.last_read(sql, params).await })
    }

    fn query<'a>(
        &'a mut self,
        sql: &'a str,
        params: &'a [SqlValue],
    ) -> LocalFuture<'a, Result<Vec<Row>, StoreError>> {
        Box::pin(async move {
            let bound = bind(params)?;
            let typed = typed(&bound);
            match self.run(Op::Query(sql, &typed)).await? {
                Done::Rows(rows) => rows.iter().map(read_row).collect(),
                Done::Count(_) | Done::Ran => Ok(Vec::new()),
            }
        })
    }

    fn execute<'a>(
        &'a mut self,
        sql: &'a str,
        params: &'a [SqlValue],
    ) -> LocalFuture<'a, Result<u64, StoreError>> {
        Box::pin(async move {
            let bound = bind(params)?;
            let typed = typed(&bound);
            match self.run(Op::Execute(sql, &typed)).await? {
                Done::Count(count) => Ok(count),
                Done::Rows(rows) => Ok(rows.len() as u64),
                Done::Ran => Ok(0),
            }
        })
    }

    fn batch<'a>(&'a mut self, sql: &'a str) -> LocalFuture<'a, Result<(), StoreError>> {
        Box::pin(async move { self.run(Op::Batch(sql)).await.map(|_| ()) })
    }

    fn savepoint(&mut self) -> LocalFuture<'_, Result<(), StoreError>> {
        Box::pin(async move {
            self.run(Op::Batch(jadpo_runtime::SAVEPOINT)).await?;
            self.saved.push(self.failed.clone());
            Ok(())
        })
    }

    fn release_savepoint(&mut self) -> LocalFuture<'_, Result<(), StoreError>> {
        Box::pin(async move {
            self.run(Op::Batch(jadpo_runtime::RELEASE_SAVEPOINT))
                .await?;
            self.saved.pop();
            Ok(())
        })
    }

    fn rollback_to_savepoint(&mut self) -> LocalFuture<'_, Result<(), StoreError>> {
        Box::pin(async move {
            self.run(Op::Batch(jadpo_runtime::ROLLBACK_TO_SAVEPOINT))
                .await?;
            // The refused statement is undone, and with it its error.
            self.failed = self.saved.pop().flatten();
            Ok(())
        })
    }

    fn commit(self: Box<Self>) -> LocalFuture<'static, Result<(), StoreError>> {
        Box::pin(async move {
            let this = *self;
            match this.failed.clone() {
                // PostgreSQL would roll back anyway; say why.
                Some(error) => {
                    this.end("ROLLBACK").await?;
                    Err(error)
                }
                None => this.end("COMMIT").await,
            }
        })
    }

    fn rollback(self: Box<Self>) -> LocalFuture<'static, Result<(), StoreError>> {
        Box::pin(async move { (*self).end("ROLLBACK").await })
    }
}

/// A parameter, sent in its binary format with the type its value names.
#[derive(Debug)]
struct Param<'a> {
    value: &'a SqlValue,
    ty: Type,
}

fn bind(params: &[SqlValue]) -> Result<Vec<Param<'_>>, StoreError> {
    params
        .iter()
        .map(|value| {
            let oid = wire::param_oid(value.ty());
            // A vector's and a geography's types are their extensions',
            // whose numbers differ in each database: sent with none,
            // PostgreSQL infers each from the column it meets (chunks 5.5v1
            // and 5.5u4).
            if oid == wire::oid::UNSPECIFIED {
                let ty = Type::new("unknown".into(), oid, Kind::Pseudo, "pg_catalog".into());
                return Ok(Param { value, ty });
            }
            let ty = Type::from_oid(oid).ok_or_else(|| {
                StoreError::Fault(format!("PostgreSQL client: no built-in type has OID {oid}"))
            })?;
            Ok(Param { value, ty })
        })
        .collect()
}

fn typed<'p>(bound: &'p [Param<'_>]) -> Vec<(&'p (dyn ToSql + Sync), Type)> {
    bound
        .iter()
        .map(|param| (param as &(dyn ToSql + Sync), param.ty.clone()))
        .collect()
}

impl ToSql for Param<'_> {
    fn to_sql(
        &self,
        _ty: &Type,
        out: &mut BytesMut,
    ) -> Result<IsNull, Box<dyn std::error::Error + Sync + Send>> {
        let mut bytes = Vec::new();
        if wire::encode(self.value, &mut bytes)? {
            out.extend_from_slice(&bytes);
            Ok(IsNull::No)
        } else {
            Ok(IsNull::Yes)
        }
    }

    fn accepts(_ty: &Type) -> bool {
        // The type sent is the value's own, chosen in `bind`.
        true
    }

    to_sql_checked!();
}

/// A column's value as the runtime reads it.
struct Cell(SqlValue);

impl<'a> FromSql<'a> for Cell {
    fn from_sql(
        ty: &Type,
        raw: &'a [u8],
    ) -> Result<Cell, Box<dyn std::error::Error + Sync + Send>> {
        if is_vector(ty) {
            return Ok(Cell(wire::vector_from_wire(raw)?));
        }
        if is_geography(ty) {
            return Ok(Cell(wire::geography_from_wire(raw)?));
        }
        Ok(Cell(wire::decode(ty.oid(), raw)?))
    }

    fn from_sql_null(ty: &Type) -> Result<Cell, Box<dyn std::error::Error + Sync + Send>> {
        match column_type(ty) {
            Some(sql_type) => Ok(Cell(SqlValue::Null(sql_type))),
            None => Err(format!("the runtime doesn't read type {}", ty.name()).into()),
        }
    }

    fn accepts(ty: &Type) -> bool {
        column_type(ty).is_some()
    }
}

/// The runtime's type for a column of type `ty`: a built-in one by its
/// number, pgvector's and PostGIS's by their names (chunks 5.5v1 and
/// 5.5u4).
fn column_type(ty: &Type) -> Option<jadpo_runtime::SqlType> {
    if is_vector(ty) {
        return Some(jadpo_runtime::SqlType::Vector);
    }
    if is_geography(ty) {
        return Some(jadpo_runtime::SqlType::Geography);
    }
    wire::column_type(ty.oid())
}

/// Whether `ty` is PostGIS's `geography`, which an extension adds, so it
/// has no built-in number (area 20, chunk 5.5u4).
fn is_geography(ty: &Type) -> bool {
    wire::column_type(ty.oid()).is_none() && ty.name() == wire::GEOGRAPHY
}

/// Whether `ty` is pgvector's `vector`, which an extension adds, so it has
/// no built-in number.
fn is_vector(ty: &Type) -> bool {
    wire::column_type(ty.oid()).is_none() && ty.name() == wire::VECTOR
}

fn read_row(row: &tokio_postgres::Row) -> Result<Row, StoreError> {
    let mut out = Vec::with_capacity(row.len());
    for (index, column) in row.columns().iter().enumerate() {
        let ty = column.type_();
        if column_type(ty).is_none() {
            return Err(StoreError::Fault(format!(
                "column `{}` has type {}, which the runtime doesn't read",
                column.name(),
                ty.name()
            )));
        }
        let cell: Cell = row.try_get(index).map_err(|error| store_error(&error))?;
        out.push(cell.0);
    }
    Ok(out)
}

/// Where `config` says to connect: each host with its port, as libpq pairs
/// them.
fn endpoints(config: &Config) -> Result<Vec<Endpoint>, StoreError> {
    let hosts = config.get_hosts();
    let ports = config.get_ports();
    if hosts.is_empty() {
        return Err(StoreError::Fault(
            "the database URL names no host".to_string(),
        ));
    }
    hosts
        .iter()
        .enumerate()
        .map(|(index, host)| {
            let port = match ports {
                [] => DEFAULT_PORT,
                [port] => *port,
                _ => *ports.get(index).ok_or_else(|| {
                    StoreError::Fault("the database URL names more hosts than ports".to_string())
                })?,
            };
            Ok(match host {
                Host::Tcp(name) if name.starts_with('/') => Endpoint::Unix {
                    dir: name.clone(),
                    port,
                },
                Host::Tcp(name) => Endpoint::Tcp {
                    host: name.clone(),
                    port,
                },
                #[cfg(unix)]
                Host::Unix(path) => Endpoint::Unix {
                    dir: path.to_string_lossy().into_owned(),
                    port,
                },
            })
        })
        .collect()
}

/// The runtime's reading of a tokio-postgres error.
fn store_error(error: &tokio_postgres::Error) -> StoreError {
    if let Some(db) = error.as_db_error() {
        return classify(Cause::Server {
            code: db.code().code(),
            constraint: db.constraint(),
            message: db.message(),
        });
    }
    let io = error
        .source()
        .is_some_and(|source| source.is::<std::io::Error>());
    if error.is_closed() || io {
        return classify(Cause::Lost);
    }
    classify(Cause::Other(&describe(error)))
}

/// An error with its cause, which tokio-postgres's own message leaves out.
fn describe(error: &tokio_postgres::Error) -> String {
    match error.source() {
        Some(source) => format!("{error}: {source}"),
        None => error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::io;

    use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream};

    use super::*;

    /// What the scripted host saw a store do.
    #[derive(Debug, Default)]
    struct Seen {
        connected: Vec<Endpoint>,
        /// Each upgrade's name, and whether it trusted a connection's own
        /// authority rather than the host's roots.
        upgraded: Vec<(String, bool)>,
    }

    /// A host whose every connection is one end of a pipe the test holds
    /// the other end of, and whose upgrade fails the check when told to:
    /// at once, as natively, or at the first write over it, as Workers
    /// reports one.
    struct Scripted {
        seen: RefCell<Seen>,
        server: RefCell<Option<DuplexStream>>,
        refuse_certificate: bool,
        refuse_at_first_write: bool,
        /// The host its platform's pooler is at, if any.
        pooler: Option<&'static str>,
    }

    impl Scripted {
        fn new(refuse_certificate: bool) -> Rc<Scripted> {
            Rc::new(Scripted {
                seen: RefCell::default(),
                server: RefCell::new(None),
                refuse_certificate,
                refuse_at_first_write: false,
                pooler: None,
            })
        }
    }

    /// An upgraded stream whose certificate check fails as the first bytes
    /// go out, as Workers' `startTls()` reports one.
    struct RefusedAtFirstWrite;

    impl tokio::io::AsyncRead for RefusedAtFirstWrite {
        fn poll_read(
            self: std::pin::Pin<&mut Self>,
            _: &mut std::task::Context<'_>,
            _: &mut tokio::io::ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            Poll::Ready(Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "TLS with db.example.test: the socket broke",
            )))
        }
    }

    impl tokio::io::AsyncWrite for RefusedAtFirstWrite {
        fn poll_write(
            self: std::pin::Pin<&mut Self>,
            _: &mut std::task::Context<'_>,
            _: &[u8],
        ) -> Poll<io::Result<usize>> {
            Poll::Ready(Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "TLS with db.example.test: the socket broke",
            )))
        }

        fn poll_flush(
            self: std::pin::Pin<&mut Self>,
            _: &mut std::task::Context<'_>,
        ) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }

        fn poll_shutdown(
            self: std::pin::Pin<&mut Self>,
            _: &mut std::task::Context<'_>,
        ) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }
    }

    impl Transport for Scripted {
        fn connect(&self, endpoint: &Endpoint) -> LocalFuture<'_, io::Result<Box<dyn Stream>>> {
            self.seen.borrow_mut().connected.push(endpoint.clone());
            let (client, server) = tokio::io::duplex(1024);
            *self.server.borrow_mut() = Some(server);
            Box::pin(async move { Ok(Box::new(client) as Box<dyn Stream>) })
        }

        fn upgrade<'a>(
            &'a self,
            stream: Box<dyn Stream>,
            name: &'a str,
            trust: Trust<'a>,
        ) -> LocalFuture<'a, io::Result<Box<dyn Stream>>> {
            self.seen
                .borrow_mut()
                .upgraded
                .push((name.to_string(), matches!(trust, Trust::Authorities(_))));
            let refuse = self.refuse_certificate;
            let at_first_write = self.refuse_at_first_write;
            Box::pin(async move {
                if refuse {
                    Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "invalid peer certificate: UnknownIssuer",
                    ))
                } else if at_first_write {
                    Ok(Box::new(RefusedAtFirstWrite) as Box<dyn Stream>)
                } else {
                    Ok(stream)
                }
            })
        }

        fn spawn(&self, _: LocalFuture<'static, ()>) {}

        fn sleep(&self, _: u64) -> LocalFuture<'_, ()> {
            Box::pin(std::future::pending())
        }

        fn is_pooler(&self, endpoint: &Endpoint) -> bool {
            matches!((endpoint, self.pooler), (Endpoint::Tcp { host, .. }, Some(pooler)) if host == pooler)
        }
    }

    /// What the store's first connection sent first, the server answering
    /// an SSL request with `answer` and then hanging up.
    fn first_words(
        url: &str,
        security: Security,
        answer: u8,
        refuse_certificate: bool,
    ) -> (Result<(), StoreError>, [u8; 8], Seen) {
        first_words_on(Scripted::new(refuse_certificate), url, security, answer)
    }

    /// [`first_words`] over `host`.
    fn first_words_on(
        host: Rc<Scripted>,
        url: &str,
        security: Security,
        answer: u8,
    ) -> (Result<(), StoreError>, [u8; 8], Seen) {
        let store = Postgres::secured(url, Rc::clone(&host) as Rc<dyn Transport>, security)
            .expect("a store");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("a runtime");
        let mut first = [0u8; 8];
        let (done, ()) =
            runtime.block_on(both(store.batch_outside_transaction("SELECT 1"), async {
                // The connection opens when the store is first polled.
                let mut server = loop {
                    if let Some(server) = host.server.borrow_mut().take() {
                        break server;
                    }
                    tokio::task::yield_now().await;
                };
                server
                    .read_exact(&mut first)
                    .await
                    .expect("its first words");
                if first == SSL_REQUEST {
                    server.write_all(&[answer]).await.expect("an answer");
                }
            }));
        drop(store);
        let seen = std::mem::take(&mut *host.seen.borrow_mut());
        (done, first, seen)
    }

    fn fault(result: &Result<(), StoreError>) -> &str {
        match result {
            Err(StoreError::Fault(why)) => why,
            other => panic!("a fault, not {other:?}"),
        }
    }

    #[test]
    fn a_server_elsewhere_is_asked_for_tls_and_refused_without_it() {
        let (done, first, seen) = first_words(
            "postgres://app@db.example.test:6543/app",
            Security::Roots,
            b'N',
            false,
        );
        assert_eq!(first, SSL_REQUEST);
        assert!(seen.upgraded.is_empty());
        assert!(
            fault(&done).contains("`db.example.test` offers no TLS"),
            "{done:?}"
        );
    }

    #[test]
    fn a_server_elsewhere_is_upgraded_against_the_hosts_roots_and_a_refused_certificate_is_a_fault()
    {
        let (done, first, seen) = first_words(
            "postgres://app@db.example.test/app",
            Security::Roots,
            b'S',
            true,
        );
        assert_eq!(first, SSL_REQUEST);
        assert_eq!(seen.upgraded, [("db.example.test".to_string(), false)]);
        assert!(
            fault(&done).contains("failed the certificate check against the public authorities"),
            "{done:?}"
        );
    }

    #[test]
    fn this_machine_is_reached_without_tls_unless_an_authority_is_named() {
        for url in [
            "postgres://app@localhost/app",
            "postgres://app@127.0.0.1/app",
            "postgres://app@[::1]/app",
        ] {
            let (_, first, seen) = first_words(url, Security::Roots, b'S', false);
            assert_ne!(first, SSL_REQUEST, "{url}");
            assert!(seen.upgraded.is_empty(), "{url}");
        }
        let (_, first, seen) = first_words(
            "postgres://app@localhost/app",
            Security::Authorities(vec![vec![0x30]]),
            b'S',
            false,
        );
        assert_eq!(first, SSL_REQUEST);
        assert_eq!(seen.upgraded, [("localhost".to_string(), true)]);
    }

    #[test]
    fn a_private_authority_is_what_the_upgrade_trusts() {
        let (done, _, seen) = first_words(
            "postgres://app@db.internal.example/app",
            Security::Authorities(vec![vec![0x30]]),
            b'S',
            true,
        );
        assert_eq!(seen.upgraded, [("db.internal.example".to_string(), true)]);
        assert!(
            fault(&done).contains("the authority its connection's `authority:` names"),
            "{done:?}"
        );
    }

    /// Workers checks the certificate as the first bytes go out over TLS,
    /// so its upgrade succeeds and the startup's first write fails: the
    /// same fault naming the server as natively, never a store down for
    /// now (chunk 8.2i).
    #[test]
    fn a_check_that_fails_at_the_first_write_after_the_upgrade_is_a_fault() {
        let host = Rc::new(Scripted {
            refuse_at_first_write: true,
            ..Rc::into_inner(Scripted::new(false)).expect("one")
        });
        let (done, first, seen) = first_words_on(
            host,
            "postgres://app@db.example.test/app",
            Security::Roots,
            b'S',
        );
        assert_eq!(first, SSL_REQUEST);
        assert_eq!(seen.upgraded, [("db.example.test".to_string(), false)]);
        assert!(
            fault(&done).contains(
                "the database server at `db.example.test` failed the certificate check against the public authorities"
            ),
            "{done:?}"
        );
    }

    /// The platform's own pooler (Hyperdrive, on Workers) is reached
    /// without TLS, whatever the connection says, since the pooler is what
    /// uses TLS to the database (chunk 8.2i).
    #[test]
    fn the_platforms_pooler_is_reached_without_tls() {
        for security in [
            Security::Roots,
            Security::Authorities(vec![vec![0x30]]),
            Security::Plain,
        ] {
            let host = Rc::new(Scripted {
                pooler: Some("5f0c9a7e2b1d4c3e8f6a7b9c0d1e2f3a.hyperdrive.local"),
                ..Rc::into_inner(Scripted::new(false)).expect("one")
            });
            let (_, first, seen) = first_words_on(
                host,
                "postgresql://app@5f0c9a7e2b1d4c3e8f6a7b9c0d1e2f3a.hyperdrive.local:5432/app",
                security.clone(),
                b'S',
            );
            assert_ne!(first, SSL_REQUEST, "{security:?}");
            assert!(seen.upgraded.is_empty(), "{security:?}");
        }
    }

    #[test]
    fn tls_false_reaches_a_server_elsewhere_without_tls() {
        let (_, first, seen) = first_words(
            "postgres://app@db.example.test/app",
            Security::Plain,
            b'S',
            false,
        );
        assert_ne!(first, SSL_REQUEST);
        assert!(seen.upgraded.is_empty());
        assert_eq!(
            seen.connected,
            [Endpoint::Tcp {
                host: "db.example.test".into(),
                port: 5432
            }]
        );
    }

    #[test]
    fn an_address_that_sets_tls_itself_is_refused() {
        for url in [
            "postgres://app@db.example.test/app?sslmode=require",
            "postgresql://app@localhost/app?sslmode=disable",
            "host=db.example.test sslrootcert=/ca.pem dbname=app",
        ] {
            let refused = Postgres::new(url, Scripted::new(false)).map(|_| ());
            match refused {
                Err(StoreError::Fault(why)) => {
                    assert!(
                        why.contains("its connection in `app.jadpo` decides"),
                        "{why}"
                    );
                    assert!(!why.contains("example"), "never the address: {why}");
                }
                other => panic!("{url}: {other:?}"),
            }
        }
    }

    /// A host whose connections never open, and whose timer goes off at
    /// once, saying how long it was asked to wait.
    #[derive(Default)]
    struct Unreachable {
        connected: RefCell<Vec<Endpoint>>,
        waits: RefCell<Vec<u64>>,
    }

    impl Transport for Unreachable {
        fn connect(&self, endpoint: &Endpoint) -> LocalFuture<'_, io::Result<Box<dyn Stream>>> {
            self.connected.borrow_mut().push(endpoint.clone());
            Box::pin(std::future::pending())
        }

        fn spawn(&self, _: LocalFuture<'static, ()>) {}

        fn sleep(&self, millis: u64) -> LocalFuture<'_, ()> {
            self.waits.borrow_mut().push(millis);
            Box::pin(std::future::ready(()))
        }
    }

    /// Each attempt at a server is bounded by the address's
    /// `connect_timeout`, or ten seconds, then the next server is tried
    /// (chunk 8.4d), so a start against a server that never answers ends.
    #[test]
    fn an_unreachable_server_is_given_up_on_after_the_connect_timeout() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("a runtime");
        for (url, wait) in [
            (
                "postgres://app@db.example.test/app?connect_timeout=3",
                3_000,
            ),
            ("host=db.example.test dbname=app connect_timeout=7", 7_000),
            ("postgres://app@db.example.test/app", CONNECT_MS),
            (
                "postgres://app@db.example.test/app?connect_timeout=0",
                CONNECT_MS,
            ),
        ] {
            let host = Rc::new(Unreachable::default());
            let store = Postgres::new(url, Rc::clone(&host) as Rc<dyn Transport>).expect("a store");
            let done = runtime.block_on(store.batch_outside_transaction("SELECT 1"));
            assert_eq!(done, Err(StoreError::Unavailable), "{url}");
            assert_eq!(*host.waits.borrow(), [wait], "{url}");
        }
        let host = Rc::new(Unreachable::default());
        let store = Postgres::new(
            "postgres://app@one.example.test,two.example.test/app?connect_timeout=2",
            Rc::clone(&host) as Rc<dyn Transport>,
        )
        .expect("a store");
        let done = runtime.block_on(store.batch_outside_transaction("SELECT 1"));
        assert_eq!(done, Err(StoreError::Unavailable));
        assert_eq!(host.connected.borrow().len(), 2, "each server tried");
        assert_eq!(*host.waits.borrow(), [2_000, 2_000]);
    }

    /// A host whose connections are pipes to a PostgreSQL server the test
    /// plays on the same thread ([`play`]), running each connection's task.
    #[derive(Default)]
    struct Played {
        servers: RefCell<Vec<DuplexStream>>,
        connects: std::cell::Cell<usize>,
    }

    impl Transport for Played {
        fn connect(&self, _: &Endpoint) -> LocalFuture<'_, io::Result<Box<dyn Stream>>> {
            self.connects.set(self.connects.get() + 1);
            let (client, server) = tokio::io::duplex(1 << 16);
            self.servers.borrow_mut().push(server);
            Box::pin(async move { Ok(Box::new(client) as Box<dyn Stream>) })
        }

        fn spawn(&self, task: LocalFuture<'static, ()>) {
            tokio::task::spawn_local(task);
        }

        fn sleep(&self, _: u64) -> LocalFuture<'_, ()> {
            Box::pin(std::future::pending())
        }
    }

    /// One message from the client: its type and its body.
    async fn message(server: &mut DuplexStream) -> Option<(u8, Vec<u8>)> {
        let mut kind = [0u8; 1];
        server.read_exact(&mut kind).await.ok()?;
        let mut length = [0u8; 4];
        server.read_exact(&mut length).await.ok()?;
        let mut body = vec![0u8; (u32::from_be_bytes(length) as usize).checked_sub(4)?];
        server.read_exact(&mut body).await.ok()?;
        Some((kind[0], body))
    }

    /// The start-up message, answered as a server that asks for no
    /// password.
    async fn started(server: &mut DuplexStream) -> Option<()> {
        let mut length = [0u8; 4];
        server.read_exact(&mut length).await.ok()?;
        let mut body = vec![0u8; (u32::from_be_bytes(length) as usize).checked_sub(4)?];
        server.read_exact(&mut body).await.ok()?;
        server
            .write_all(b"R\0\0\0\x08\0\0\0\0Z\0\0\0\x05I")
            .await
            .ok()
    }

    /// One statement read to its `Sync`.
    async fn statement(server: &mut DuplexStream) -> Option<()> {
        loop {
            let (kind, _) = message(server).await?;
            if kind == b'S' {
                return Some(());
            }
        }
    }

    /// The answer to a statement that gives no rows.
    const ANSWERED: &[u8] = b"1\0\0\0\x042\0\0\0\x04n\0\0\0\x04C\0\0\0\x0dSELECT 0\0Z\0\0\0\x05I";

    /// How the first connection a played server takes hangs up, once it
    /// has answered its first statement.
    #[derive(Clone, Copy)]
    enum HangUp {
        /// While idle, before another statement comes.
        Idle,
        /// Once it has read the next statement, before answering it.
        Sent,
    }

    /// Plays the server for every connection `host` opens, counting the
    /// statements it reads: the first connection hangs up after its first
    /// statement as `hang_up` says; every other answers each.
    async fn play(host: Rc<Played>, hang_up: HangUp, statements: Rc<std::cell::Cell<usize>>) {
        let mut opened = 0;
        loop {
            let next = host.servers.borrow_mut().pop();
            let Some(mut server) = next else {
                tokio::task::yield_now().await;
                continue;
            };
            opened += 1;
            let first = opened == 1;
            let statements = Rc::clone(&statements);
            tokio::task::spawn_local(async move {
                if started(&mut server).await.is_none() {
                    return;
                }
                let mut read = 0;
                while statement(&mut server).await.is_some() {
                    read += 1;
                    statements.set(statements.get() + 1);
                    if first && read == 2 {
                        return;
                    }
                    if server.write_all(ANSWERED).await.is_err() {
                        return;
                    }
                    if first && matches!(hang_up, HangUp::Idle) {
                        return;
                    }
                }
            });
        }
    }

    /// Runs `work` against a store on `host`, its server played alongside.
    fn against<T>(
        host: &Rc<Played>,
        hang_up: HangUp,
        work: impl AsyncFnOnce(&Postgres) -> T,
    ) -> (T, usize) {
        let store = Postgres::new(
            "postgres://app@localhost/app",
            Rc::clone(host) as Rc<dyn Transport>,
        )
        .expect("a store");
        let statements = Rc::new(std::cell::Cell::new(0));
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("a runtime");
        let local = tokio::task::LocalSet::new();
        let done = local.block_on(&runtime, async {
            let server =
                tokio::task::spawn_local(play(Rc::clone(host), hang_up, Rc::clone(&statements)));
            let done = work(&store).await;
            server.abort();
            done
        });
        (done, statements.get())
    }

    /// An autocommitted statement that went out on a reused connection
    /// which then dropped may have been committed, so it is never sent
    /// again (chunk 8.4d): the store says it is unavailable.
    #[test]
    fn a_statement_sent_on_a_reused_connection_that_drops_is_not_sent_again() {
        let host = Rc::new(Played::default());
        let ((first, second), statements) =
            against(&host, HangUp::Sent, async |store: &Postgres| {
                let first = store.query_once("INSERT INTO t VALUES (1)", &[]).await;
                let second = store.query_once("INSERT INTO t VALUES (2)", &[]).await;
                (first, second)
            });
        assert_eq!(first, Ok(Vec::new()));
        assert_eq!(second, Err(StoreError::Unavailable));
        assert_eq!(statements, 2, "the second statement went once");
        assert_eq!(
            host.connects.get(),
            1,
            "no connection opened to send it again"
        );
    }

    /// A statement still goes through when the connection it was given had
    /// closed while idle, on another.
    #[test]
    fn a_connection_closed_while_idle_sends_its_statement_on_another() {
        let host = Rc::new(Played::default());
        let ((first, second), statements) =
            against(&host, HangUp::Idle, async |store: &Postgres| {
                let first = store.query_once("INSERT INTO t VALUES (1)", &[]).await;
                let second = store.query_once("INSERT INTO t VALUES (2)", &[]).await;
                (first, second)
            });
        assert_eq!(first, Ok(Vec::new()));
        assert_eq!(second, Ok(Vec::new()));
        assert_eq!(statements, 2);
        assert_eq!(host.connects.get(), 2);
    }

    mod terminal;
}
