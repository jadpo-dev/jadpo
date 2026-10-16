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
use tokio_postgres::types::{FromSql, IsNull, ToSql, Type, to_sql_checked};
use tokio_postgres::{Client, Config, NoTls};

use super::classify::{Cause, classify};
use super::wire;
use crate::{Endpoint, Transport};

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

/// What a transaction begins on: an idle connection, or the place for a
/// new one.
enum Got {
    Idle(Client, Slot),
    New(Slot),
}

/// An idle connection, or a place for a new one once fewer than
/// [`MAX_OPEN`] are open, waiting until there is either.
async fn next(slots: &Rc<Slots>, idle: &RefCell<Vec<(Client, Slot)>>) -> Got {
    std::future::poll_fn(|cx| {
        if let Some((client, slot)) = idle.borrow_mut().pop() {
            return Poll::Ready(Got::Idle(client, slot));
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

/// What a store's transactions share: where it connects and the
/// connections it keeps.
struct Pool {
    config: Config,
    endpoints: Vec<Endpoint>,
    transport: Rc<dyn Transport>,
    idle: RefCell<Vec<(Client, Slot)>>,
    slots: Rc<Slots>,
}

impl Pool {
    /// Opens a connection, trying each endpoint in turn.
    async fn connect(&self) -> Result<Client, StoreError> {
        let mut last = StoreError::Unavailable;
        for endpoint in &self.endpoints {
            let Ok(stream) = self.transport.connect(endpoint).await else {
                last = StoreError::Unavailable;
                continue;
            };
            match self.config.connect_raw(stream, NoTls).await {
                Ok((client, connection)) => {
                    self.transport.spawn(Box::pin(async move {
                        // The connection ends when its client is dropped or
                        // the server goes; either way its client sees it.
                        let _ = connection.await;
                    }));
                    return Ok(client);
                }
                Err(error) => {
                    last = store_error(&error);
                    if last != StoreError::Unavailable {
                        return Err(last);
                    }
                }
            }
        }
        Err(last)
    }

    /// A connection to run on: an idle one, which may have been closed by
    /// the server since (`reused`), or a new one; with none idle and
    /// [`MAX_OPEN`] open, it waits for one to end.
    async fn take(&self) -> Result<Conn, StoreError> {
        loop {
            match next(&self.slots, &self.idle).await {
                Got::Idle(client, slot) => {
                    if client.is_closed() {
                        continue;
                    }
                    return Ok(Conn {
                        client,
                        slot,
                        reused: true,
                    });
                }
                Got::New(slot) => {
                    let client = self.connect().await?;
                    return Ok(Conn {
                        client,
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
        let Conn { client, slot, .. } = conn;
        let mut kept = self.idle.borrow_mut();
        if !client.is_closed() && kept.len() < MAX_IDLE {
            kept.push((client, slot));
            drop(kept);
            self.slots.wake();
        }
    }
}

/// One connection taken from the pool.
struct Conn {
    client: Client,
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
/// Connections are unencrypted: TLS is not built yet, so a URL that
/// requires it (`sslmode=require`) is refused when the first transaction
/// connects. Natively that suits a server on the same machine or network;
/// on Workers, Hyperdrive encrypts the connection to the database.
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
        let config: Config = url.parse().map_err(|error| {
            StoreError::Fault(format!(
                "the database URL isn't valid: {}",
                describe(&error)
            ))
        })?;
        let endpoints = endpoints(&config)?;
        Ok(Postgres {
            pool: Rc::new(Pool {
                config,
                endpoints,
                transport,
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
            let client = self.pool.connect().await?;
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
            }) as Box<dyn Transaction>)
        })
    }

    /// The statement on an idle connection, or a new one, outside any
    /// transaction, so PostgreSQL commits it as it runs. A connection the
    /// server closed while it was idle never ran it, so the statement is
    /// sent again on another.
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
                match conn.client.query_typed(sql, &typed).await {
                    Ok(rows) => {
                        self.pool.give_back(conn);
                        return rows.iter().map(read_row).collect();
                    }
                    Err(error) => {
                        let error = store_error(&error);
                        if conn.reused && error == StoreError::Unavailable {
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

/// An open transaction on one connection. Dropped without a commit or a
/// rollback, it drops its connection, and closing the connection aborts
/// the transaction on the server.
struct PgTransaction {
    /// Its connection: `None` only once it was lost finding another.
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

    /// Ends the transaction with `sql`, keeping the connection for the next
    /// one if it is still sound. One that never began sends nothing.
    async fn end(self, sql: &str) -> Result<(), StoreError> {
        let PgTransaction {
            conn, pool, begin, ..
        } = self;
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
        Ok(Cell(wire::decode(ty.oid(), raw)?))
    }

    fn from_sql_null(ty: &Type) -> Result<Cell, Box<dyn std::error::Error + Sync + Send>> {
        match wire::column_type(ty.oid()) {
            Some(sql_type) => Ok(Cell(SqlValue::Null(sql_type))),
            None => Err(format!("the runtime doesn't read type {}", ty.name()).into()),
        }
    }

    fn accepts(ty: &Type) -> bool {
        wire::column_type(ty.oid()).is_some()
    }
}

fn read_row(row: &tokio_postgres::Row) -> Result<Row, StoreError> {
    let mut out = Vec::with_capacity(row.len());
    for (index, column) in row.columns().iter().enumerate() {
        let ty = column.type_();
        if wire::column_type(ty.oid()).is_none() {
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
