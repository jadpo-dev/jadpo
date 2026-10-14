//! What the platform gives the runtime (docs/compiler-runtime.md §2.6):
//! one trait per capability, behind a [`Host`], so the interpreter,
//! compiled code, native hosts, Workers and test fakes all reach a store
//! the same way, and drivers classify their own errors into one fixed set.
//!
//! Built so far: the seams the action boundary needs, a store's
//! transactions and delivering an action's events after it commits, with
//! [`MemoryHost`], which holds no store (chunk 2.10); since chunk 3.1 a
//! transaction runs SQL with [`SqlValue`] parameters and rows, and a host
//! gives each store by its connection's name, so the PostgreSQL driver in
//! `jadpo-drivers` plugs in. Delivery to subscribers comes with area 12.

use std::future::Future;
use std::pin::Pin;

use jadpo_value::Value;

/// A future that stays on the thread that made it: the runtime is
/// single-threaded by design (§2.6), so nothing it awaits needs `Send`.
pub type LocalFuture<'a, T> = Pin<Box<dyn Future<Output = T> + 'a>>;

/// Why a store refused, as every driver classifies it (§2.6), so
/// executors and targets agree on what happened.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StoreError {
    /// A unique or foreign-key constraint, by name.
    Conflict { constraint: String },
    /// Another transaction changed what this one read: the action reruns
    /// (area 6, "All-or-nothing actions and concurrent changes").
    ConcurrentChange,
    /// The store can't be reached now.
    Unavailable,
    /// Anything else the driver can't classify.
    Fault(String),
}

pub use jadpo_std::SqlType;

/// A value sent to a store or read back from one. Drivers turn these into
/// their wire formats; the read plans' codecs ([`crate::read`]) turn them
/// into Jadpo values. Ordered, so an include's rows group by their key.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SqlValue {
    /// `NULL`, with the type a parameter needs.
    Null(SqlType),
    Bool(bool),
    Int2(i16),
    Int4(i32),
    Int8(i64),
    /// An exact decimal in its canonical text (`-12.50`).
    Numeric(String),
    Text(String),
    Bytes(Vec<u8>),
    Uuid([u8; 16]),
    /// Microseconds since 1970-01-01T00:00:00Z.
    Timestamp(i64),
    /// Days since 1970-01-01.
    Date(i32),
    /// Microseconds since midnight.
    Time(i64),
    /// JSON text.
    Json(String),
    /// An array of values of one type, none of them `NULL`: a list a field
    /// is tested `in`, or an include's keys.
    Array(SqlType, Vec<SqlValue>),
}

impl SqlValue {
    /// The type the value is sent as.
    pub fn ty(&self) -> SqlType {
        match self {
            SqlValue::Null(ty) => *ty,
            SqlValue::Bool(_) => SqlType::Bool,
            SqlValue::Int2(_) => SqlType::Int2,
            SqlValue::Int4(_) => SqlType::Int4,
            SqlValue::Int8(_) => SqlType::Int8,
            SqlValue::Numeric(_) => SqlType::Numeric,
            SqlValue::Text(_) => SqlType::Text,
            SqlValue::Bytes(_) => SqlType::Bytes,
            SqlValue::Uuid(_) => SqlType::Uuid,
            SqlValue::Timestamp(_) => SqlType::Timestamp,
            SqlValue::Date(_) => SqlType::Date,
            SqlValue::Time(_) => SqlType::Time,
            SqlValue::Json(_) => SqlType::Json,
            SqlValue::Array(element, _) => element.array(),
        }
    }
}

/// One row a query gave, its columns in the query's order.
pub type Row = Vec<SqlValue>;

/// A store's open transaction. Statements are unnamed, so nothing outlives
/// the transaction on the server (§2.6: Hyperdrive pools by transaction).
pub trait Transaction {
    /// Runs one statement with `params` bound to `$1`, `$2`…, giving its
    /// rows.
    fn query<'a>(
        &'a mut self,
        sql: &'a str,
        params: &'a [SqlValue],
    ) -> LocalFuture<'a, Result<Vec<Row>, StoreError>>;

    /// Runs one statement with `params`, giving how many rows it changed.
    fn execute<'a>(
        &'a mut self,
        sql: &'a str,
        params: &'a [SqlValue],
    ) -> LocalFuture<'a, Result<u64, StoreError>>;

    /// Runs statements without parameters, one after another, as a
    /// migration's SQL is written.
    fn batch<'a>(&'a mut self, sql: &'a str) -> LocalFuture<'a, Result<(), StoreError>>;

    fn commit(self: Box<Self>) -> LocalFuture<'static, Result<(), StoreError>>;
    fn rollback(self: Box<Self>) -> LocalFuture<'static, Result<(), StoreError>>;
}

/// A store that runs transactions: PostgreSQL from chunk 3.1.
pub trait Database {
    /// Begins a transaction, at repeatable read (area 6).
    fn begin(&self) -> LocalFuture<'_, Result<Box<dyn Transaction>, StoreError>>;
}

/// An event an action emitted, waiting in its outbox until the action
/// commits (area 12).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
    /// The event type's identity key.
    pub key: String,
    pub value: Value,
}

/// The platform: the capabilities one invocation runs against.
pub trait Host {
    /// The store named `store` (a connection in `app.jadpo`, area 11), or
    /// `None` when the host has none by that name.
    fn database(&self, store: &str) -> Option<&dyn Database>;

    /// Delivers what a committed action emitted, in the order it emitted
    /// it. Never called for an action that rolled back.
    fn deliver(&self, events: Vec<Event>) -> LocalFuture<'_, ()>;
}

/// A store that holds nothing: its transactions begin, commit and roll back
/// at once. What runs while no store is built.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoStore;

struct NoTransaction;

impl Transaction for NoTransaction {
    fn query<'a>(
        &'a mut self,
        _sql: &'a str,
        _params: &'a [SqlValue],
    ) -> LocalFuture<'a, Result<Vec<Row>, StoreError>> {
        Box::pin(async { Err(StoreError::Fault("no store holds any data".into())) })
    }

    fn execute<'a>(
        &'a mut self,
        _sql: &'a str,
        _params: &'a [SqlValue],
    ) -> LocalFuture<'a, Result<u64, StoreError>> {
        Box::pin(async { Err(StoreError::Fault("no store holds any data".into())) })
    }

    fn batch<'a>(&'a mut self, _sql: &'a str) -> LocalFuture<'a, Result<(), StoreError>> {
        Box::pin(async { Err(StoreError::Fault("no store holds any data".into())) })
    }

    fn commit(self: Box<Self>) -> LocalFuture<'static, Result<(), StoreError>> {
        Box::pin(async { Ok(()) })
    }

    fn rollback(self: Box<Self>) -> LocalFuture<'static, Result<(), StoreError>> {
        Box::pin(async { Ok(()) })
    }
}

impl Database for NoStore {
    fn begin(&self) -> LocalFuture<'_, Result<Box<dyn Transaction>, StoreError>> {
        Box::pin(async { Ok(Box::new(NoTransaction) as Box<dyn Transaction>) })
    }
}

/// A host with no store and no subscribers, in memory: what `jadpo test`
/// and the interpreter run on until the test database and events come
/// with their phases.
#[derive(Clone, Copy, Debug, Default)]
pub struct MemoryHost {
    store: NoStore,
}

impl MemoryHost {
    pub fn new() -> MemoryHost {
        MemoryHost::default()
    }
}

impl Host for MemoryHost {
    fn database(&self, _store: &str) -> Option<&dyn Database> {
        Some(&self.store)
    }

    fn deliver(&self, _events: Vec<Event>) -> LocalFuture<'_, ()> {
        Box::pin(async {})
    }
}
