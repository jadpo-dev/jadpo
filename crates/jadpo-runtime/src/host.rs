//! What the platform gives the runtime (docs/compiler-runtime.md §2.6):
//! one trait per capability, behind a [`Host`], so the interpreter,
//! compiled code, native hosts, Workers and test fakes all reach a store
//! the same way, and drivers classify their own errors into one fixed set.
//!
//! Built so far: the seams the action boundary needs, a store's
//! transactions, with [`MemoryHost`], which holds no store (chunk 2.10);
//! since chunk 3.1 a transaction runs SQL with [`SqlValue`] parameters and
//! rows, and a host gives each store by its connection's name, so the
//! PostgreSQL driver in `jadpo-drivers` plugs in; since chunk 3.12 a host
//! gives random bytes for a deployable's new identities; since chunk 4.10
//! its wall clock and rate limits are capabilities too; since chunk 6.1a
//! it runs work after a response is sent ([`Host::defer`]), which
//! replaced delivering a committed action's events in the request; since
//! chunk 6.7 it gives each invocation of background work its budget
//! ([`Host::budget_ms`]), says when it is shutting down
//! ([`Host::stopping`]), and runs work handed on ([`Host::hand_on`]); since
//! chunk 7.0c it gives the driver of each outside capability a connection
//! fills ([`Host::capability`]), which only the capability seam calls.

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

/// A value sent to a store or read back from one: the shared leaf's, so
/// a literal the compiler writes into a statement is the value the runtime
/// binds (area 19, chunk 5.5w3b).
pub use jadpo_std::SqlValue;

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

    /// Runs the only application statement of a fresh standalone read
    /// transaction. Only `commit` or `rollback` may follow. Drivers may
    /// queue its ending with the read, keeping any end error for that call;
    /// the ordinary fallback keeps the transaction open until then.
    fn query_last_read<'a>(
        &'a mut self,
        sql: &'a str,
        params: &'a [SqlValue],
    ) -> LocalFuture<'a, Result<Vec<Row>, StoreError>> {
        self.query(sql, params)
    }

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
/// commits (area 12), when it is written to the outbox with a delivery
/// record for each of its subscribers (chunk 6.1b).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
    /// The event type's identity key.
    pub key: String,
    pub value: Value,
    /// Its payload, its type's contract JSON (area 12, "A payload is the
    /// contract's JSON"), as the outbox keeps it.
    pub payload: String,
    /// The identity keys of its subscribers, each delivered to on its own.
    pub subscribers: Vec<String>,
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

/// A software authenticator (area 8, "In development and tests"; chunk
/// 5.5z2): what a browser and a device do between a passkey route's
/// options and the answer posted back, so `test_passkeys` drives the
/// generated routes for real. Only a test's host has one: a `TestPasskey`
/// exists nowhere else.
pub trait TestAuthenticators {
    /// A new device in a browser at `origin`, making a passkey from the
    /// registration `options` (`navigator.credentials.create`): the
    /// device's number and its answer, as WebAuthn's `toJSON()` writes it.
    fn create(&self, origin: &str, options: &str) -> Result<(u32, String), String>;

    /// Device `device`'s answer to the sign-in `options`
    /// (`navigator.credentials.get`).
    fn get(&self, device: u32, options: &str) -> Result<String, String>;
}

/// What the runtime fetches from outside on its own account (area 11,
/// "Receiving from outside"; chunk 7.7b): AWS SNS's signing certificate
/// and its subscription confirmation. The runtime decides which addresses
/// it asks for; the host keeps every request from reaching a private
/// address.
pub trait Outbound {
    /// `GET`s `url`, an HTTPS address the runtime has checked, and gives
    /// its status and body: no redirect followed, no connection made to a
    /// loopback, private, link-local or otherwise non-public address, at
    /// most `limit` bytes read and `timeout_ms` waited. Why not, for the
    /// host's log, when it can't.
    fn get<'a>(
        &'a self,
        url: &'a str,
        limit: usize,
        timeout_ms: u64,
    ) -> LocalFuture<'a, Result<(u16, Vec<u8>), String>>;
}

/// Whether `ip` is an address on the public internet, what an
/// [`Outbound`] reaches: not loopback, private, link-local, shared
/// (100.64/10), reserved, documentation, benchmarking, unspecified,
/// broadcast or multicast; IPv4 inside IPv6 judged as itself: mapped
/// (`::ffff:0:0/96`), compatible, NAT64's (`64:ff9b::/96`), 6to4's
/// (`2002::/16`) and both of Teredo's (`2001::/32`, its server and its
/// client), chunk 8.4d. Site-local (`fec0::/10`), NAT64's local-use
/// prefix (`64:ff9b:1::/48`) and the rest of the IETF's protocol block
/// (`2001::/23`) are never public. One answer for every host's guard (the
/// native host's since chunk 7.7b, moved here for the Workers host's in
/// chunk 8.2a).
pub fn public_address(ip: std::net::IpAddr) -> bool {
    use std::net::{IpAddr, Ipv4Addr};
    match ip {
        IpAddr::V4(v4) => {
            let [a, b, c, _] = v4.octets();
            !(v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_documentation()
                || v4.is_unspecified()
                || v4.is_multicast()
                || a == 0
                || a >= 240
                || (a == 100 && (64..128).contains(&b))
                || (a == 192 && b == 0 && c == 0)
                || (a == 198 && (18..20).contains(&b)))
        }
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped().or_else(|| v6.to_ipv4()) {
                return public_address(IpAddr::V4(v4));
            }
            let s = v6.segments();
            let four = |high: u16, low: u16| {
                IpAddr::V4(Ipv4Addr::from((u32::from(high) << 16) | u32::from(low)))
            };
            // NAT64's well-known prefix reaches the IPv4 address in its
            // last 32 bits.
            if s[..6] == [0x0064, 0xff9b, 0, 0, 0, 0] {
                return public_address(four(s[6], s[7]));
            }
            // 6to4 carries its IPv4 address in the 32 bits after 2002.
            if s[0] == 0x2002 {
                return public_address(four(s[1], s[2]));
            }
            // Teredo carries its server's address, and its client's with
            // every bit flipped.
            if s[0] == 0x2001 && s[1] == 0 {
                return public_address(four(s[2], s[3])) && public_address(four(!s[6], !s[7]));
            }
            !(v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || s[0] & 0xfe00 == 0xfc00
                || s[0] & 0xffc0 == 0xfe80
                || s[0] & 0xffc0 == 0xfec0
                || s[0] == 0x2001 && s[1] < 0x0200
                || s[0] == 0x2001 && s[1] == 0x0db8
                || s[0] == 0x0064 && s[1] == 0xff9b)
        }
    }
}

/// The platform: the capabilities one invocation runs against.
pub trait Host {
    /// The driver of the outside capability connection `connection` fills
    /// (area 11; chunk 7.0c): sending an email, putting a file, a
    /// provider's operation. Only the capability seam calls it
    /// ([`crate::effects::call`]), which logs, records and guards every
    /// call. `None` when the host has none by that name, as every host
    /// until a capability's driver is built; a test gives fakes in their
    /// place ([`crate::capability::Faked`]).
    fn capability(&self, connection: &str) -> Option<&dyn crate::capability::Capability> {
        let _ = connection;
        None
    }

    /// The development call log (area 17; chunk 7.2d), which keeps each
    /// provider call with its values: only `jadpo dev`'s host has one.
    fn call_log(&self) -> Option<&dyn crate::call_log::CallLog> {
        None
    }

    /// The outcome-and-effect record each outside call is kept in (chunk
    /// 8.1c): only a test's host keeps one ([`crate::capability::Faked`]),
    /// so everywhere else a call pays only this asking. Only with the
    /// `testing` feature (chunk 8.1e).
    #[cfg(feature = "testing")]
    fn record(&self) -> Option<&crate::record::Record> {
        None
    }

    /// The passkey ceremonies (area 8, "Passkeys"): `None` on a host that
    /// carries no WebAuthn implementation, whose passkey routes answer
    /// `authentication_misconfigured`.
    fn passkeys(&self) -> Option<&dyn Passkeys> {
        None
    }

    /// The software authenticator `test_passkeys` drives the passkey
    /// routes with (chunk 5.5z2): only a test's host has one.
    fn test_authenticators(&self) -> Option<&dyn TestAuthenticators> {
        None
    }

    /// The store named `store` (a connection in `app.jadpo`, area 11), or
    /// `None` when the host has none by that name.
    fn database(&self, store: &str) -> Option<&dyn Database>;

    /// What the runtime fetches from outside on its own account (chunk
    /// 7.7b): `None` on a host that reaches nothing, where an SNS webhook
    /// is answered `503` until its certificate can be fetched.
    fn outbound(&self) -> Option<&dyn Outbound> {
        None
    }

    /// Runs `work` once the response being answered has been handed back,
    /// without holding that response up: what the runner gives it to run
    /// a committed action's follow-on work in the background (area 12,
    /// "Where events travel"; chunk 6.1a, [`crate::runner::after_response`]).
    /// Workers' `waitUntil`, natively a task on the worker's own thread.
    /// A host with nothing to run it on, as a test's or a tool's, drops it:
    /// what an event sets off is kept in the outbox, which the minute
    /// tick sweeps (chunk 6.1b).
    fn defer(&self, work: LocalFuture<'static, ()>) {
        drop(work);
    }

    /// How long one invocation of background work of kind `invocation`
    /// may run on this host, in milliseconds from when it starts (chunk
    /// 6.7): the runner takes no more work and gives back what it holds
    /// before it ends, and every lease it takes lasts what is left of it
    /// and a margin ([`crate::budget`], [`crate::lease`]). At most the
    /// 15-minute limit of a run. Natively and under `jadpo dev`, and on a
    /// host that says nothing, that limit; on Workers its platform's, 30
    /// seconds after a response and 15 minutes for the cron (chunk 8.2a).
    fn budget_ms(&self, invocation: crate::budget::Invocation) -> u64 {
        let _ = invocation;
        crate::budget::GENEROUS_MS
    }

    /// How many of a file's rows a file helper's run reads into one batch
    /// (area 12, "How a run is kept"; chunk 6.4b): a batch's transaction
    /// commits that many rows' writes and the run's position together. A
    /// test's host gives fewer, so every import resumes between batches
    /// (area 13, "Provider calls and capabilities in tests").
    fn rows_per_batch(&self) -> u32 {
        crate::batches::ROWS_PER_BATCH
    }

    /// A future that finishes once the host begins to shut down: natively
    /// on SIGTERM, or ctrl-c under `jadpo dev` (chunk 6.7). Background
    /// work's budget ends there: the piece running is stopped and what it
    /// held given back as it was. A host that never shuts its work down so
    /// gives one that never finishes.
    fn stopping(&self) -> LocalFuture<'_, ()> {
        Box::pin(std::future::pending())
    }

    /// Runs `work` as a fresh invocation, with a budget of its own: what an
    /// invocation whose budget was spent hands on, a batch run carried on
    /// and the work it hadn't reached (chunk 6.7). Natively a task of its
    /// own on the worker's thread ([`HandOn::run`]); on Workers a fresh
    /// invocation of the Worker, a request to itself through a service
    /// binding sent [`HandOn::message`] (chunk 8.2h). A host with nothing
    /// to run it on, as a test's or a tool's, drops it: what it held was
    /// given back, and the minute tick takes it up.
    ///
    /// [`HandOn::run`]: crate::runner::HandOn::run
    /// [`HandOn::message`]: crate::runner::HandOn::message
    fn hand_on(&self, work: crate::runner::HandOn) {
        drop(work);
    }

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
    /// the counters are in bounded memory, one table for all of a server's
    /// workers ([`crate::RateLimits`]; chunk 8.1l); on Workers the deployable counts in a Redis
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

    /// The slots that cap each connection's calls at once (area 11,
    /// `max_concurrent:`; chunk 7.6c): the app instance's own. Natively
    /// the process's, which every worker thread shares, and an isolate's
    /// on Workers ([`crate::slots::Slots::process`]); a test's host keeps
    /// its own, so tests side by side never wait on each other.
    fn slots(&self) -> &crate::slots::Slots {
        crate::slots::Slots::process()
    }
}

/// A host borrowed is the host: what a wrapper such as
/// [`crate::capability::Faked`] holds when it doesn't own the host it
/// wraps.
impl<H: Host + ?Sized> Host for &H {
    fn capability(&self, connection: &str) -> Option<&dyn crate::capability::Capability> {
        (**self).capability(connection)
    }

    fn call_log(&self) -> Option<&dyn crate::call_log::CallLog> {
        (**self).call_log()
    }

    #[cfg(feature = "testing")]
    fn record(&self) -> Option<&crate::record::Record> {
        (**self).record()
    }

    fn passkeys(&self) -> Option<&dyn Passkeys> {
        (**self).passkeys()
    }

    fn test_authenticators(&self) -> Option<&dyn TestAuthenticators> {
        (**self).test_authenticators()
    }

    fn database(&self, store: &str) -> Option<&dyn Database> {
        (**self).database(store)
    }

    fn outbound(&self) -> Option<&dyn Outbound> {
        (**self).outbound()
    }

    fn defer(&self, work: LocalFuture<'static, ()>) {
        (**self).defer(work);
    }

    fn budget_ms(&self, invocation: crate::budget::Invocation) -> u64 {
        (**self).budget_ms(invocation)
    }

    fn rows_per_batch(&self) -> u32 {
        (**self).rows_per_batch()
    }

    fn stopping(&self) -> LocalFuture<'_, ()> {
        (**self).stopping()
    }

    fn hand_on(&self, work: crate::runner::HandOn) {
        (**self).hand_on(work);
    }

    fn variable(&self, name: &str) -> Option<String> {
        (**self).variable(name)
    }

    fn random(&self, bytes: &mut [u8]) -> bool {
        (**self).random(bytes)
    }

    fn sleep(&self, millis: u64) -> LocalFuture<'_, ()> {
        (**self).sleep(millis)
    }

    fn yield_now(&self) -> LocalFuture<'_, ()> {
        (**self).yield_now()
    }

    fn monotonic_ms(&self) -> Option<u64> {
        (**self).monotonic_ms()
    }

    fn now_ms(&self) -> Option<i64> {
        (**self).now_ms()
    }

    fn rate_limit<'a>(
        &'a self,
        key: &'a str,
        limit: jadpo_std::http::RateLimit,
        now: i64,
    ) -> LocalFuture<'a, crate::Allowed> {
        (**self).rate_limit(key, limit, now)
    }

    fn slots(&self) -> &crate::slots::Slots {
        (**self).slots()
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

/// A host with no store, in memory, that runs nothing in the background:
/// what the interpreter runs a function on, and `jadpo test` a test that
/// uses no store.
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

    /// None: what runs on it runs under a seed.
    fn random(&self, _bytes: &mut [u8]) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::public_address;

    fn public(address: &str) -> bool {
        public_address(address.parse().expect("an address"))
    }

    /// IPv4 carried inside IPv6 is judged as itself, however it is carried
    /// (chunk 8.4d).
    #[test]
    fn ipv4_inside_ipv6_is_judged_as_itself() {
        // NAT64's well-known prefix.
        assert!(public("64:ff9b::808:808"));
        assert!(public("64:ff9b::8.8.8.8"));
        assert!(!public("64:ff9b::a00:1"));
        assert!(!public("64:ff9b::127.0.0.1"));
        assert!(!public("64:ff9b::169.254.169.254"));
        assert!(!public("64:ff9b:1::808:808"), "NAT64's local-use prefix");
        // 6to4.
        assert!(public("2002:808:808::1"));
        assert!(!public("2002:a00:1::1"));
        assert!(!public("2002:7f00:1::1"));
        assert!(!public("2002:a9fe:a9fe::1"));
        // Teredo: the server's address, then the client's flipped.
        assert!(public("2001:0:808:808::f7f7:f7f7"));
        assert!(!public("2001:0:808:808::f5ff:fffe"), "a client at 10.0.0.1");
        assert!(!public("2001:0:a00:1::f7f7:f7f7"), "a server at 10.0.0.1");
        // Site-local, and the rest of the IETF's protocol block.
        assert!(!public("fec0::1"));
        assert!(!public("feff::1"));
        assert!(!public("2001:2::1"));
        assert!(!public("2001:10::1"));
        // Public IPv6 stays public.
        assert!(public("2600:1f18::1"));
        assert!(public("2001:4860:4860::8888"));
    }
}
