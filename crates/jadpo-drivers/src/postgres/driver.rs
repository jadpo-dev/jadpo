//! [`Postgres`]: the runtime's `Database` over tokio-postgres.

use std::cell::RefCell;
use std::error::Error as _;
use std::fmt;
use std::rc::Rc;

use bytes::BytesMut;
use jadpo_runtime::{Database, LocalFuture, Row, SqlValue, StoreError, Transaction};
use tokio_postgres::config::Host;
use tokio_postgres::types::{FromSql, IsNull, ToSql, Type, to_sql_checked};
use tokio_postgres::{Client, Config, NoTls};

use super::classify::{Cause, classify};
use super::wire;
use crate::{Endpoint, Transport};

/// How many connections a store keeps open between transactions.
const MAX_IDLE: usize = 8;

/// PostgreSQL's default port.
const DEFAULT_PORT: u16 = 5432;

const BEGIN: &str = "BEGIN ISOLATION LEVEL REPEATABLE READ";

/// A PostgreSQL store, reached through the host's [`Transport`]. It keeps
/// a few connections open between transactions and begins each one at
/// repeatable read (area 6).
///
/// Connections are unencrypted: TLS is not built yet, so a URL that
/// requires it (`sslmode=require`) is refused when the first transaction
/// connects. Natively that suits a server on the same machine or network;
/// on Workers, Hyperdrive encrypts the connection to the database.
pub struct Postgres {
    config: Config,
    endpoints: Vec<Endpoint>,
    transport: Rc<dyn Transport>,
    idle: Rc<RefCell<Vec<Client>>>,
}

impl fmt::Debug for Postgres {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Never the URL: it may hold a password.
        f.debug_struct("Postgres")
            .field("endpoints", &self.endpoints)
            .field("idle", &self.idle.borrow().len())
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
            config,
            endpoints,
            transport,
            idle: Rc::new(RefCell::new(Vec::new())),
        })
    }

    /// Where the store connects, in the order it tries them.
    pub fn endpoints(&self) -> &[Endpoint] {
        &self.endpoints
    }

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
            let client = self.connect().await?;
            client
                .batch_execute(sql)
                .await
                .map_err(|error| store_error(&error))
        })
    }

    fn open(&self, client: Client) -> Box<dyn Transaction> {
        Box::new(PgTransaction {
            client,
            idle: Rc::clone(&self.idle),
            failed: None,
            saved: Vec::new(),
        })
    }
}

impl Database for Postgres {
    fn begin(&self) -> LocalFuture<'_, Result<Box<dyn Transaction>, StoreError>> {
        Box::pin(async move {
            // An idle connection may have been closed by the server since;
            // one that can't begin is dropped and the next one tried.
            loop {
                let Some(client) = self.idle.borrow_mut().pop() else {
                    break;
                };
                if client.is_closed() {
                    continue;
                }
                match client.batch_execute(BEGIN).await {
                    Ok(()) => return Ok(self.open(client)),
                    Err(error) => match store_error(&error) {
                        StoreError::Unavailable => continue,
                        other => return Err(other),
                    },
                }
            }
            let client = self.connect().await?;
            client
                .batch_execute(BEGIN)
                .await
                .map_err(|error| store_error(&error))?;
            Ok(self.open(client))
        })
    }
}

/// An open transaction on one connection. Dropped without a commit or a
/// rollback, it drops its connection, and closing the connection aborts
/// the transaction on the server.
struct PgTransaction {
    client: Client,
    idle: Rc<RefCell<Vec<Client>>>,
    /// The first error a statement gave: PostgreSQL aborts the transaction
    /// then, and answers a later `COMMIT` by rolling back.
    failed: Option<StoreError>,
    /// What `failed` was at each savepoint still open, latest last: going
    /// back to one undoes the error after it.
    saved: Vec<Option<StoreError>>,
}

impl PgTransaction {
    fn note<T>(&mut self, result: Result<T, tokio_postgres::Error>) -> Result<T, StoreError> {
        result.map_err(|error| {
            let error = store_error(&error);
            if self.failed.is_none() {
                self.failed = Some(error.clone());
            }
            error
        })
    }

    /// Ends the transaction with `sql`, keeping the connection for the next
    /// one if it is still sound.
    async fn end(self, sql: &str) -> Result<(), StoreError> {
        let PgTransaction { client, idle, .. } = self;
        // A connection that couldn't end its transaction is dropped.
        client
            .batch_execute(sql)
            .await
            .map_err(|error| store_error(&error))?;
        let mut idle = idle.borrow_mut();
        if !client.is_closed() && idle.len() < MAX_IDLE {
            idle.push(client);
        }
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
            let rows = self.client.query_typed(sql, &typed).await;
            let rows = self.note(rows)?;
            rows.iter().map(read_row).collect()
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
            let count = self.client.execute_typed(sql, &typed).await;
            self.note(count)
        })
    }

    fn batch<'a>(&'a mut self, sql: &'a str) -> LocalFuture<'a, Result<(), StoreError>> {
        Box::pin(async move {
            let done = self.client.batch_execute(sql).await;
            self.note(done)
        })
    }

    fn savepoint(&mut self) -> LocalFuture<'_, Result<(), StoreError>> {
        Box::pin(async move {
            let done = self.client.batch_execute(jadpo_runtime::SAVEPOINT).await;
            self.note(done)?;
            self.saved.push(self.failed.clone());
            Ok(())
        })
    }

    fn release_savepoint(&mut self) -> LocalFuture<'_, Result<(), StoreError>> {
        Box::pin(async move {
            let done = self.client.batch_execute(jadpo_runtime::RELEASE_SAVEPOINT).await;
            self.note(done)?;
            self.saved.pop();
            Ok(())
        })
    }

    fn rollback_to_savepoint(&mut self) -> LocalFuture<'_, Result<(), StoreError>> {
        Box::pin(async move {
            let done = self
                .client
                .batch_execute(jadpo_runtime::ROLLBACK_TO_SAVEPOINT)
                .await;
            self.note(done)?;
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
