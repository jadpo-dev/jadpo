//! The native host (docs/compiler-runtime.md §2.1 and §2.6): what runs a
//! Jadpo program, its tests and its tools on a machine rather than on
//! Workers.
//!
//! - [`Runtime`]: one current-thread tokio runtime with a `LocalSet`, so
//!   the runtime's futures, which are not `Send`, run on the thread that
//!   made them.
//! - [`NativeTransport`]: the drivers' [`Transport`] over tokio's TCP and
//!   Unix sockets, upgraded to TLS through rustls ([`tls`], chunk 8.1b).
//! - [`postgres`]: a PostgreSQL store over that transport.
//! - [`NativeHost`]: the runtime's `Host` for a deployable or `jadpo dev`:
//!   each connection's driver as the factory built it from the plan
//!   ([`supplies`], chunk 8.1d), the operating system's
//!   randomness, tokio's timer and the monotonic clock for deadlines
//!   (chunks 4.2 and 4.9), the system clock, rate limits counted in
//!   bounded memory (chunk 4.10), one table for all of a server's workers
//!   (chunk 8.1l), and the work a response leaves, run as a task on the
//!   worker's thread once the response is handed back (chunk 6.1a); it
//!   drains when the server stops, its background work giving back what
//!   it holds (chunk 6.7).
//! - [`http`]: the acceptor and its worker threads, each a current-thread
//!   runtime answering with its own handler (chunk 4.1), the acceptor
//!   handing them connections in turn (chunk 8.1l).
//! - [`files`]: the disk driver, which keeps `jadpo dev`'s stores of files
//!   under `.jadpo/files/` (chunk 6.4b).
//! - [`fetch`]: HTTP out of the process (chunks 7.2c and 8.1m), the
//!   package driver's and the runtime's own: hyper's client on the
//!   worker's thread, its connections kept per host, HTTPS through rustls.
//! - [`tls`]: rustls with Mozilla's roots, the one TLS stack on the
//!   request path (chunk 8.1b); the SMTP driver's conversation runs over
//!   the transport's upgrade, in `jadpo-drivers`.
//! - [`dns`]: `TXT` lookups for `jadpo email check-domain` (chunk 7.3c2).
//! - [`spamassassin`]: SpamAssassin on this machine, its network tests
//!   off, for the dev inbox's health score (chunk 7.3c3).
//! - [`stop_signals`]: SIGTERM and ctrl-c, which stop a server so that it
//!   drains (chunk 6.7).
//! - [`deployable`]: a compiled deployable from its `main` to its last
//!   log line: its settings read, its databases' migrations checked, its
//!   workers serving and ticking until a stop drains them (chunk 8.1o).
//!
//! Built so far: the runtime and the transport the PostgreSQL driver needs
//! (chunk 3.1), serving HTTP (chunk 4.1), running a response's
//! follow-on work in the background (chunk 6.1a), the timer that ticks
//! the runtime's schedules and sweeps (chunk 6.2), and draining on a stop
//! (chunk 6.7), files on the local disk (chunk 6.4b), SMTP (chunk 7.3)
//! and TLS over the transport (chunk 8.1b).

// A panic aborts a Workers instance, so runtime code reports faults as
// values (docs/compiler-runtime.md §2.6); the native host holds itself to
// the same rule. Tests may still unwrap.
#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing,
        clippy::unreachable,
        clippy::unimplemented
    )
)]

pub mod deployable;
pub mod dns;
pub mod fetch;
pub mod files;
pub mod http;
pub mod passkeys;
mod signals;
pub mod spamassassin;
#[cfg(feature = "test-authenticator")]
pub mod test_authenticator;
pub mod tls;
mod tracked;

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::future::Future;
use std::io;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use jadpo_drivers::{Endpoint, Stream, Transport, Trust};
use jadpo_runtime::{Allowed, Database, Host, LocalFuture, RateLimits, StoreError};
use jadpo_std::http::RateLimit;
use tokio::task::LocalSet;

pub use signals::stop_signals;

pub use jadpo_drivers::factory::{Drivers, Supplies};
pub use jadpo_drivers::package::JsonHttp;
pub use jadpo_drivers::postgres::{Postgres, Security};

/// One current-thread tokio runtime and the `LocalSet` its tasks run in.
/// Everything a store connects is driven by [`Runtime::block_on`], so a
/// store is used inside it, on the thread that made it.
#[derive(Debug)]
pub struct Runtime {
    runtime: tokio::runtime::Runtime,
    local: LocalSet,
}

impl Runtime {
    /// A runtime with tokio's I/O driver and timer, on this thread.
    pub fn new() -> io::Result<Runtime> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .enable_time()
            .build()?;
        Ok(Runtime {
            runtime,
            local: LocalSet::new(),
        })
    }

    /// Runs `future` to its end on this thread, with every task the
    /// transport spawned running alongside it.
    pub fn block_on<F: Future>(&self, future: F) -> F::Output {
        self.local.block_on(&self.runtime, future)
    }
}

/// The drivers' transport over tokio's sockets: TCP with Nagle's algorithm
/// off, and Unix sockets on Unix, upgraded to TLS through rustls against
/// Mozilla's roots ([`tls`]). It connects, spawns and sleeps only inside
/// [`Runtime::block_on`] (or another tokio `LocalSet`), where its I/O
/// driver, its timer and its tasks run.
#[derive(Clone, Copy, Debug, Default)]
pub struct NativeTransport;

impl Transport for NativeTransport {
    fn connect(&self, endpoint: &Endpoint) -> LocalFuture<'_, io::Result<Box<dyn Stream>>> {
        let endpoint = endpoint.clone();
        Box::pin(async move {
            match &endpoint {
                Endpoint::Tcp { host, port } => {
                    let stream = tokio::net::TcpStream::connect((host.as_str(), *port)).await?;
                    stream.set_nodelay(true)?;
                    Ok(Box::new(stream) as Box<dyn Stream>)
                }
                Endpoint::Unix { .. } => unix(&endpoint).await,
            }
        })
    }

    fn upgrade<'a>(
        &'a self,
        stream: Box<dyn Stream>,
        name: &'a str,
        trust: Trust<'a>,
    ) -> LocalFuture<'a, io::Result<Box<dyn Stream>>> {
        Box::pin(async move {
            let config = match trust {
                Trust::Roots => tls::config()?,
                Trust::Authorities(authorities) => tls::trusting_all(authorities)?,
            };
            tls::upgrade(config, stream, name).await
        })
    }

    fn spawn(&self, task: LocalFuture<'static, ()>) {
        tokio::task::spawn_local(task);
    }

    fn sleep(&self, millis: u64) -> LocalFuture<'_, ()> {
        Box::pin(tokio::time::sleep(Duration::from_millis(millis)))
    }

    fn now_ms(&self) -> Option<i64> {
        let since = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?;
        i64::try_from(since.as_millis()).ok()
    }

    fn local_address(&self, stream: &dyn Stream) -> Option<std::net::IpAddr> {
        let stream: &dyn std::any::Any = stream;
        let tcp = stream.downcast_ref::<tokio::net::TcpStream>()?;
        tcp.local_addr().ok().map(|address| address.ip())
    }
}

#[cfg(unix)]
async fn unix(endpoint: &Endpoint) -> io::Result<Box<dyn Stream>> {
    let path = endpoint.socket_path().unwrap_or_default();
    let stream = tokio::net::UnixStream::connect(path).await?;
    Ok(Box::new(stream))
}

#[cfg(not(unix))]
async fn unix(endpoint: &Endpoint) -> io::Result<Box<dyn Stream>> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        format!("{endpoint} is a Unix socket, and this host has none"),
    ))
}

/// A PostgreSQL store for `url` over [`NativeTransport`]. It connects when
/// its first transaction begins, inside [`Runtime::block_on`], over TLS
/// checked against Mozilla's roots to any server but this machine (chunk
/// 8.1n).
pub fn postgres(url: &str) -> Result<Postgres, StoreError> {
    Postgres::new(url, Rc::new(NativeTransport))
}

/// A PostgreSQL store for `url` over [`NativeTransport`], secured as its
/// connection in `app.jadpo` says (area 5, "PostgreSQL over TLS"; chunk
/// 8.1n).
pub fn postgres_secured(url: &str, security: Security) -> Result<Postgres, StoreError> {
    Postgres::secured(url, Rc::new(NativeTransport), security)
}

/// What a native host gives the driver factory (chunk 8.1d): tokio's
/// sockets with rustls ([`NativeTransport`]) and HTTP with its connections
/// kept per host ([`fetch::NativeFetch`], chunk 8.1m), one of each per
/// worker, since each worker builds its own. It gives no disk: a deployable's disk driver
/// at a connection's `path:` comes with the rest of the files capability,
/// and `jadpo dev` keeps its stores of files on its own
/// ([`NativeHost::with_files`]).
pub fn supplies() -> Supplies {
    Supplies::on(jadpo_runtime::http::address::Target::Native)
        .with_transport(Rc::new(NativeTransport))
        .with_fetch(Rc::new(fetch::NativeFetch::new()))
}

/// The runtime's host natively, one per worker thread: each connection's
/// driver as the factory built it ([`NativeHost::with_drivers`]), a store
/// given in place of one, the operating system's randomness for new
/// identities and request ids, the system clock, rate-limit counters in
/// bounded memory, the native default until a Redis connection is named
/// (area 16), shared by all of a server's workers
/// ([`http::WorkerThread::host`]; chunk 8.1l) and a host's own otherwise,
/// and the work responses leave for after they are sent
/// ([`Host::defer`]), which the HTTP worker takes as each response is
/// handed back ([`NativeHost::take_deferred`]) and runs as tasks of its
/// own on the worker's thread (chunk 6.1a). Each invocation of background
/// work has the whole run limit as its budget, the runtime's default, and
/// what it hands on runs as a task of its own; once the host drains
/// ([`NativeHost::drain`]) its background work stops and gives back what
/// it holds (chunk 6.7).
#[derive(Debug, Default)]
pub struct NativeHost {
    /// Each connection's driver, as the factory built it from the plan
    /// (chunk 8.1d).
    drivers: Drivers,
    /// Stores given in place of the factory's: `jadpo dev`'s databases,
    /// which it chooses itself, and a test's.
    stores: BTreeMap<String, Postgres>,
    /// Every store of files, on the local disk (chunk 6.4b), and which
    /// connections are stores of files (chunk 7.1c).
    files: Option<(files::DiskFiles, FileStores)>,
    /// `jadpo dev`'s call log, which keeps each provider call with its
    /// values (chunk 7.2d).
    call_log: Option<CallLog>,
    /// The rate-limit counters: the server's one table, which its workers
    /// share (chunk 8.1l).
    limits: Arc<RateLimits>,
    deferred: Deferred,
    /// The background work it has running (deferred work, ticks, work
    /// handed on), and whether it is shutting down (chunk 6.7).
    background: Rc<tracked::Tracked>,
    /// What the runtime fetches on its own account (chunk 7.7b).
    outbound: fetch::Outward,
}

/// Which connections are stores of files: what a host gives the disk
/// driver for, and nothing else (chunk 7.1c).
#[derive(Clone)]
struct FileStores(Rc<dyn Fn(&str) -> bool>);

impl std::fmt::Debug for FileStores {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("FileStores")
    }
}

/// A development call log, held for the host.
#[derive(Clone)]
struct CallLog(Rc<dyn jadpo_runtime::call_log::CallLog>);

impl std::fmt::Debug for CallLog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CallLog")
    }
}

/// How long a host draining waits for its background work to give back
/// what it holds before the process ends without it: each piece stops at
/// once, so this only bounds the statements that give its claims back.
/// A stopping server's worker waits as long for its connections to give
/// the answers they owe first (chunk 8.1l).
pub const DRAIN_GRACE: Duration = Duration::from_secs(10);

/// Work given to run after a response, until the worker takes it.
#[derive(Default)]
struct Deferred(RefCell<Vec<LocalFuture<'static, ()>>>);

impl std::fmt::Debug for Deferred {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Deferred({})", self.0.borrow().len())
    }
}

impl NativeHost {
    pub fn new() -> NativeHost {
        NativeHost::default()
    }

    /// Gives each connection the driver `drivers` built for it from the
    /// plan's connections table (chunk 8.1d): what a deployable and `jadpo
    /// dev` run on, `jadpo dev` with its fakes in front.
    pub fn with_drivers(mut self, drivers: Drivers) -> NativeHost {
        self.drivers = drivers;
        self
    }

    /// Adds store `name`, a PostgreSQL database, in place of the one the
    /// factory built for it: `jadpo dev`'s, which it chooses itself, and a
    /// test's.
    pub fn with_store(mut self, name: impl Into<String>, store: Postgres) -> NativeHost {
        self.stores.insert(name.into(), store);
        self
    }

    /// Keeps every store of files on the local disk, as `files` says
    /// (chunk 6.4b): the connections `stores` says are stores of files,
    /// which `jadpo dev` reads from the program it runs now, and no other
    /// (chunk 7.1c).
    pub fn with_files(
        mut self,
        files: files::DiskFiles,
        stores: impl Fn(&str) -> bool + 'static,
    ) -> NativeHost {
        self.files = Some((files, FileStores(Rc::new(stores))));
        self
    }

    /// Counts rate limits in `limits`: the table every worker of a server
    /// shares (chunk 8.1l).
    pub(crate) fn with_rate_limits(mut self, limits: Arc<RateLimits>) -> NativeHost {
        self.limits = limits;
        self
    }

    /// Keeps each provider call with its values in `log` (chunk 7.2d).
    pub fn with_call_log(mut self, log: Rc<dyn jadpo_runtime::call_log::CallLog>) -> NativeHost {
        self.call_log = Some(CallLog(log));
        self
    }

    /// The work it was given to run after the response being answered,
    /// for whoever sends that response to run once it has been handed
    /// back: the HTTP worker, as a task on its thread. Each is counted as
    /// the host's background work, which a drain waits for.
    pub fn take_deferred(&self) -> Vec<LocalFuture<'static, ()>> {
        std::mem::take(&mut *self.deferred.0.borrow_mut())
            .into_iter()
            .map(|work| self.track(work))
            .collect()
    }

    /// `work` counted as the host's background work until it ends or is
    /// dropped, so a drain waits for it: what the runtime's tick runs as.
    pub fn track(&self, work: LocalFuture<'static, ()>) -> LocalFuture<'static, ()> {
        self.background.track(work)
    }

    /// Shuts its background work down (chunk 6.7): every budget it runs
    /// within ends at once, so the piece each is running stops and gives
    /// back what it held, and it takes nothing more. The future waits for
    /// that work to end, at most [`DRAIN_GRACE`].
    pub fn drain(&self) -> LocalFuture<'static, ()> {
        self.background.stop(DRAIN_GRACE)
    }
}

/// The passkey ceremonies every native host carries.
static PASSKEYS: passkeys::WebauthnPasskeys = passkeys::WebauthnPasskeys;

impl Host for NativeHost {
    /// The disk driver given for a store of files (chunk 6.4b; narrowed
    /// to those in chunk 7.1c), else the driver the factory built for the
    /// connection (chunk 8.1d); none for any other.
    fn capability(&self, connection: &str) -> Option<&dyn jadpo_runtime::capability::Capability> {
        if let Some((files, stores)) = &self.files
            && (stores.0)(connection)
        {
            return Some(files as &dyn jadpo_runtime::capability::Capability);
        }
        self.drivers.capability(connection)
    }

    fn call_log(&self) -> Option<&dyn jadpo_runtime::call_log::CallLog> {
        self.call_log.as_ref().map(|log| log.0.as_ref())
    }

    /// `webauthn-rs` (area 8, "Passkeys").
    fn passkeys(&self) -> Option<&dyn jadpo_runtime::Passkeys> {
        Some(&PASSKEYS)
    }

    /// A store given in its place, else the one the factory built.
    fn database(&self, store: &str) -> Option<&dyn Database> {
        self.stores
            .get(store)
            .or_else(|| self.drivers.database(store))
            .map(|store| store as &dyn Database)
    }

    /// HTTPS to public addresses only ([`fetch::Outward`]).
    fn outbound(&self) -> Option<&dyn jadpo_runtime::Outbound> {
        Some(&self.outbound)
    }

    /// Kept for the HTTP worker, which runs it once the response is handed
    /// back ([`NativeHost::take_deferred`]).
    fn defer(&self, work: LocalFuture<'static, ()>) {
        self.deferred.0.borrow_mut().push(work);
    }

    /// A task of its own on this thread, counted as background work; none
    /// once the host is draining, since what it would run waits in the
    /// store for the next process. Called only from work this thread's
    /// `LocalSet` runs, as everything a worker defers is.
    fn hand_on(&self, work: LocalFuture<'static, ()>) {
        if self.background.stopped() {
            return;
        }
        tokio::task::spawn_local(self.track(work));
    }

    /// Finishes once [`NativeHost::drain`] begins.
    fn stopping(&self) -> LocalFuture<'_, ()> {
        self.background.stopping()
    }

    /// The process's environment.
    fn variable(&self, name: &str) -> Option<String> {
        std::env::var(name).ok()
    }

    /// The operating system's cryptographic generator, through getrandom:
    /// `false` when it can't give, never a weaker source.
    fn random(&self, bytes: &mut [u8]) -> bool {
        getrandom::fill(bytes).is_ok()
    }

    /// tokio's timer, which runs inside [`Runtime::block_on`].
    fn sleep(&self, millis: u64) -> LocalFuture<'_, ()> {
        Box::pin(tokio::time::sleep(std::time::Duration::from_millis(millis)))
    }

    /// tokio's yield, which wakes the task only after the I/O driver is
    /// polled: a task that wakes itself is polled again first, up to a
    /// `LocalSet`'s whole tick, so other requests' replies would wait.
    fn yield_now(&self) -> LocalFuture<'_, ()> {
        Box::pin(tokio::task::yield_now())
    }

    /// The process's monotonic clock, from when it was first read.
    fn monotonic_ms(&self) -> Option<u64> {
        static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
        let start = START.get_or_init(std::time::Instant::now);
        u64::try_from(start.elapsed().as_millis()).ok()
    }

    /// The system clock.
    fn now_ms(&self) -> Option<i64> {
        let elapsed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?;
        i64::try_from(elapsed.as_millis()).ok()
    }

    /// Counted in memory: the server's one table, which its workers share
    /// (chunk 8.1l).
    fn rate_limit<'a>(
        &'a self,
        key: &'a str,
        limit: RateLimit,
        now: i64,
    ) -> LocalFuture<'a, Allowed> {
        let allowed = self.limits.allow(key, limit, now);
        Box::pin(async move { allowed })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jadpo_runtime::Database;

    #[test]
    fn a_server_that_isnt_there_is_unavailable() {
        let runtime = Runtime::new().unwrap();
        let dir = std::env::temp_dir().join(format!("jadpo-nothing-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = postgres(&format!(
            "host={} user=jadpo dbname=postgres",
            dir.display()
        ))
        .unwrap();
        let begun = runtime.block_on(store.begin());
        assert_eq!(begun.err(), Some(StoreError::Unavailable));
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn random_bytes_come_from_the_operating_systems_generator() {
        let host = NativeHost::new();
        let (mut a, mut b) = ([0u8; 32], [0u8; 32]);
        assert!(host.random(&mut a) && host.random(&mut b));
        assert_ne!(a, b, "each fill is new");
        assert_ne!(a, [0u8; 32]);
    }

    #[test]
    fn a_url_that_isnt_one_is_a_fault() {
        for url in ["postgresql://h:notaport/db", "dbname=nowhere"] {
            assert!(matches!(postgres(url), Err(StoreError::Fault(_))), "{url}");
        }
        let store = postgres("postgresql://jadpo@db.internal:6543/app").unwrap();
        assert_eq!(
            store.endpoints().first(),
            Some(&Endpoint::Tcp {
                host: "db.internal".into(),
                port: 6543
            })
        );
    }
}
