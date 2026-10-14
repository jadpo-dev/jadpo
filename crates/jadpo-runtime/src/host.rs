//! What the platform gives the runtime (docs/compiler-runtime.md §2.6):
//! one trait per capability, behind a [`Host`], so the interpreter,
//! compiled code, native hosts, Workers and test fakes all reach a store
//! the same way, and drivers classify their own errors into one fixed set.
//!
//! Built so far (chunk 2.10): the seams the action boundary needs, a
//! store's transactions and delivering an action's events after it
//! commits, with [`MemoryHost`], which holds no store: every transaction
//! begins, commits and rolls back at once. PostgreSQL comes with chunk
//! 3.1, delivery to subscribers with area 12.

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

/// A store's open transaction.
pub trait Transaction {
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
    /// The store actions write to.
    fn database(&self) -> &dyn Database;

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
    fn database(&self) -> &dyn Database {
        &self.store
    }

    fn deliver(&self, _events: Vec<Event>) -> LocalFuture<'_, ()> {
        Box::pin(async {})
    }
}
