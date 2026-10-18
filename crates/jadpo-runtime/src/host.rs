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
//! `jadpo-drivers` plugs in; since chunk 3.12 a host gives random bytes
//! for a deployable's new identities; since chunk 4.10 its wall clock and
//! rate limits are capabilities too. Delivery to subscribers comes with
//! area 12.

use std::future::Future;
use std::pin::Pin;

use jadpo_std::Isolation;
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
    /// The store refused a statement Jadpo wrote as malformed or as
    /// breaking a rule the plan should have kept to (a missing operator, a
    /// negative `LIMIT`, `NULL` in a required column): a mistake in Jadpo,
    /// not in the program or the database. It holds the server's code and
    /// message.
    Statement(String),
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
    /// Microseconds since 1970-01-01T00:00:00 on no zone: a `timestamp`
    /// without a zone, a `LocalDateTime` (area 19, chunk 5.5w).
    LocalTimestamp(i64),
    /// JSON text.
    Json(String),
    /// An array of values of one type, none of them `NULL`: a list a field
    /// is tested `in`, or an include's keys.
    Array(SqlType, Vec<SqlValue>),
    /// A range of values of the element type (area 19, "Ranges", chunk
    /// 5.5w): from its lower end, held, to its upper end, held when
    /// `upper_held`; an end that is `None` is open.
    Range {
        element: SqlType,
        lower: Option<Box<SqlValue>>,
        upper: Option<Box<SqlValue>>,
        upper_held: bool,
    },
    /// A pgvector `vector` (area 21, chunk 5.5v1): each item's
    /// single-precision float by its bits, so a vector is compared and
    /// ordered exactly, as the other values are.
    Vector(Vec<u32>),
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
            SqlValue::LocalTimestamp(_) => SqlType::LocalTimestamp,
            SqlValue::Json(_) => SqlType::Json,
            SqlValue::Array(element, _) => element.array(),
            SqlValue::Range { element, .. } => element.range(),
            SqlValue::Vector(_) => SqlType::Vector,
        }
    }
}

/// One row a query gave, its columns in the query's order.
pub type Row = Vec<SqlValue>;

/// The savepoint a called action whose failure its caller handles runs
/// after (area 6). Savepoints of one name stack, so actions called inside
/// such an action take their own.
pub const SAVEPOINT: &str = "SAVEPOINT \"jadpo_call\"";
/// Forgets it once the action succeeded.
pub const RELEASE_SAVEPOINT: &str = "RELEASE SAVEPOINT \"jadpo_call\"";
/// Returns to it, and forgets it, once the action failed.
pub const ROLLBACK_TO_SAVEPOINT: &str =
    "ROLLBACK TO SAVEPOINT \"jadpo_call\"; RELEASE SAVEPOINT \"jadpo_call\"";

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

    /// Marks where the transaction returns to if the action about to run
    /// fails: one whose failure its caller handles and carries on from
    /// (area 6).
    fn savepoint(&mut self) -> LocalFuture<'_, Result<(), StoreError>> {
        self.batch(SAVEPOINT)
    }

    /// Forgets the latest savepoint: what ran after it stands.
    fn release_savepoint(&mut self) -> LocalFuture<'_, Result<(), StoreError>> {
        self.batch(RELEASE_SAVEPOINT)
    }

    /// Returns to the latest savepoint and forgets it: everything after
    /// it, a refused statement among them, is undone, and the transaction
    /// carries on.
    fn rollback_to_savepoint(&mut self) -> LocalFuture<'_, Result<(), StoreError>> {
        self.batch(ROLLBACK_TO_SAVEPOINT)
    }

    fn commit(self: Box<Self>) -> LocalFuture<'static, Result<(), StoreError>>;
    fn rollback(self: Box<Self>) -> LocalFuture<'static, Result<(), StoreError>>;
}

/// A store that runs transactions: PostgreSQL from chunk 3.1.
pub trait Database {
    /// Begins a transaction at `isolation`: an action's own, decided from
    /// what it does (area 6, "Counters and other one-step updates"). The
    /// level is set by the transaction's own `BEGIN`, never for the
    /// session, so nothing outlives the transaction (§2.6).
    fn begin_at(
        &self,
        isolation: Isolation,
    ) -> LocalFuture<'_, Result<Box<dyn Transaction>, StoreError>>;

    /// Begins a transaction at repeatable read: what runs outside an
    /// action, such as migrations, purge and tools.
    fn begin(&self) -> LocalFuture<'_, Result<Box<dyn Transaction>, StoreError>> {
        self.begin_at(Isolation::RepeatableRead)
    }

    /// Runs one statement on its own, outside any transaction, so the store
    /// keeps what it did as it runs: one round trip, with no `BEGIN` or
    /// `COMMIT` to wait on. What the runtime's own short steps use, each a
    /// single statement already: the sign-in lookup, claiming and giving
    /// up an idempotency key, the response a failed action keeps, and the
    /// readiness check (chunk 4.10). A driver that can't run a statement
    /// on its own runs it in a transaction of its own, as this does.
    fn query_once<'a>(
        &'a self,
        sql: &'a str,
        params: &'a [SqlValue],
    ) -> LocalFuture<'a, Result<Vec<Row>, StoreError>> {
        Box::pin(async move {
            let mut transaction = self.begin_at(Isolation::ReadCommitted).await?;
            match transaction.query(sql, params).await {
                Ok(rows) => {
                    transaction.commit().await?;
                    Ok(rows)
                }
                Err(error) => {
                    let _ = transaction.rollback().await;
                    Err(error)
                }
            }
        })
    }
}

/// An event an action emitted, waiting in its outbox until the action
/// commits (area 12).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
    /// The event type's identity key.
    pub key: String,
    pub value: Value,
}

/// What is kept of a passkey for its ceremonies (area 8, "Passkeys"):
/// `WebAuthnCredential`'s parts that `webauthn-rs` reads (area 19, chosen
/// in chunk 5.5r3), never a secret.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PasskeyParts {
    pub credential_id: Vec<u8>,
    /// The COSE_Key (RFC 9052 §7) in CBOR.
    pub public_key: Vec<u8>,
    pub sign_count: u32,
    /// WebAuthn's names: `usb`, `nfc`, `ble`, `internal`, `hybrid`.
    pub transports: Vec<String>,
    pub backup_eligible: bool,
    pub backed_up: bool,
}

/// What an answer from a device says before it is checked: the challenge
/// it answers, the passkey it is from, and on signing in the user handle
/// it gives.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PasskeyAnswer {
    pub challenge: Vec<u8>,
    pub credential_id: Vec<u8>,
    pub user_handle: Option<Vec<u8>>,
}

/// Why a passkey ceremony couldn't go on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PasskeyError {
    /// The answer doesn't check: origin, relying party, presence,
    /// verification, signature or counter. Which isn't said.
    Rejected,
    /// The origin can't be a relying party, or the state kept is no
    /// longer readable.
    Misconfigured,
}

/// The passkey ceremonies (area 8, "Passkeys"), on an audited WebAuthn
/// implementation the host carries: `webauthn-rs` natively. `origin` is
/// the browser session's configured origin, whose host is the
/// relying-party ID; nothing here reads the request. A ceremony's state
/// is JSON the runtime keeps server-side, used once.
pub trait Passkeys {
    /// The options for adding a passkey, as WebAuthn's
    /// `PublicKeyCredentialCreationOptionsJSON` inside `publicKey`, and
    /// the state to check the answer with: a discoverable passkey that
    /// verifies the person, with no attestation, none of `exclude` again.
    fn start_registration(
        &self,
        origin: &str,
        timeout_ms: u32,
        user_handle: [u8; 16],
        account: &str,
        exclude: &[Vec<u8>],
    ) -> Result<(String, String), PasskeyError>;

    /// The new passkey, once its answer checks against `state`.
    fn finish_registration(
        &self,
        origin: &str,
        timeout_ms: u32,
        state: &str,
        credential: &str,
    ) -> Result<PasskeyParts, PasskeyError>;

    /// The options for signing in with any of the person's discoverable
    /// passkeys, and the state to check the answer with.
    fn start_sign_in(
        &self,
        origin: &str,
        timeout_ms: u32,
    ) -> Result<(String, String), PasskeyError>;

    /// What `credential`, a device's answer as WebAuthn's `toJSON()`
    /// writes it, says before it is checked; `None` when it can't be read.
    fn read_answer(&self, credential: &str) -> Option<PasskeyAnswer>;

    /// `stored` after signing in with it: the answer checked against
    /// `state`, the user handle `user_handle` and the stored key, its
    /// counter, backup state and eligibility updated. A counter that
    /// didn't go up, when either is above zero, is `Rejected`.
    fn finish_sign_in(
        &self,
        origin: &str,
        timeout_ms: u32,
        state: &str,
        credential: &str,
        stored: &PasskeyParts,
        user_handle: [u8; 16],
    ) -> Result<PasskeyParts, PasskeyError>;
}

/// The platform: the capabilities one invocation runs against.
pub trait Host {
    /// The passkey ceremonies (area 8, "Passkeys"): `None` on a host that
    /// carries no WebAuthn implementation, whose passkey routes answer
    /// `authentication_misconfigured`.
    fn passkeys(&self) -> Option<&dyn Passkeys> {
        None
    }

    /// The store named `store` (a connection in `app.jadpo`, area 11), or
    /// `None` when the host has none by that name.
    fn database(&self, store: &str) -> Option<&dyn Database>;

    /// Delivers what a committed action emitted, in the order it emitted
    /// it. Never called for an action that rolled back.
    fn deliver(&self, events: Vec<Event>) -> LocalFuture<'_, ()>;

    /// The value of the environment variable `name` (area 10): the
    /// platform's environment, or on Workers a binding. `None` when it
    /// isn't set, or for a host with no environment, as a test's, whose
    /// settings come from `test:` values instead. Read only when the
    /// application starts ([`crate::config::load`]).
    fn variable(&self, name: &str) -> Option<String> {
        let _ = name;
        None
    }

    /// Fills `bytes` from the platform's cryptographically secure source
    /// of randomness (the operating system's, or the Web Crypto API on
    /// Workers): what a deployable's new identities take (area 5). `false`
    /// when it has none to give, as a test's host, whose invocations run
    /// under a seed instead ([`crate::Env::seed`]).
    fn random(&self, bytes: &mut [u8]) -> bool;

    /// A future that finishes `millis` milliseconds after it is made, on
    /// the platform's timer: what a route's deadline and the readiness
    /// check wait with (area 7, "Time limits" and "Health"). A host with no
    /// timer, as a test's, gives one that never finishes, so nothing runs
    /// out of time there.
    fn sleep(&self, millis: u64) -> LocalFuture<'_, ()> {
        let _ = millis;
        Box::pin(std::future::pending())
    }

    /// Gives the thread back once, so whatever else it runs (other requests
    /// a worker answers, their I/O) has a turn: what work that never waits
    /// awaits as it checks its deadline (area 7, "Time limits"). The
    /// default wakes itself at once ([`crate::yield_now`]); a host whose
    /// scheduler polls its I/O only when nothing is ready gives one that
    /// waits for that poll.
    fn yield_now(&self) -> LocalFuture<'_, ()> {
        Box::pin(crate::yield_now())
    }

    /// Milliseconds on the platform's monotonic clock, from any start it
    /// likes: what a route's deadline is checked against while work runs
    /// that never waits (area 7, "Time limits"), and how old the last
    /// readiness check is. `None` for a host with no timer, as a test's, so
    /// nothing runs out of time there.
    fn monotonic_ms(&self) -> Option<u64> {
        None
    }

    /// The platform's wall clock, in milliseconds since
    /// 1970-01-01T00:00:00Z: what a request a deployable answers reads as
    /// `clock.now` ([`crate::http::serve_request`]). `None` for a host with
    /// no clock, as a test's, whose clock is the test's own (area 13).
    fn now_ms(&self) -> Option<i64> {
        None
    }

    /// Counts a request against `limit` for `key` (`address <ip>`, `user
    /// <id>` or `service <id>`) at `now`, milliseconds since the epoch,
    /// and says whether it may go ahead (area 16, "Rate limits"). Natively
    /// the counters are in bounded memory, each worker's own
    /// ([`crate::RateLimits`]); on Workers the deployable counts in a Redis
    /// connection or Cloudflare's rate-limiting binding, and in the
    /// isolate's memory only while that can't be reached (area 16,
    /// question 6). A host that counts nothing, as a test's, lets every
    /// request through.
    fn rate_limit<'a>(
        &'a self,
        key: &'a str,
        limit: jadpo_std::http::RateLimit,
        now: i64,
    ) -> LocalFuture<'a, crate::Allowed> {
        let _ = (key, limit, now);
        Box::pin(async { crate::Allowed::Yes })
    }
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
    fn begin_at(
        &self,
        _isolation: Isolation,
    ) -> LocalFuture<'_, Result<Box<dyn Transaction>, StoreError>> {
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

    /// None: what runs on it runs under a seed.
    fn random(&self, _bytes: &mut [u8]) -> bool {
        false
    }
}
