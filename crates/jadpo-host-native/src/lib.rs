//! The native host (docs/compiler-runtime.md §2.1 and §2.6): what runs a
//! Jadpo program, its tests and its tools on a machine rather than on
//! Workers.
//!
//! - [`Runtime`]: one current-thread tokio runtime with a `LocalSet`, so
//!   the runtime's futures, which are not `Send`, run on the thread that
//!   made them.
//! - [`NativeTransport`]: the drivers' [`Transport`] over tokio's TCP and
//!   Unix sockets.
//! - [`postgres`]: a PostgreSQL store over that transport.
//! - [`NativeHost`]: the runtime's `Host` for a deployable or `jadpo dev`:
//!   each store by its connection's name, the operating system's
//!   randomness, and tokio's timer and the monotonic clock for deadlines
//!   (chunks 4.2 and 4.9).
//! - [`http`]: the listener and its worker threads, each a current-thread
//!   runtime answering with its own handler (chunk 4.1).
//!
//! Built so far: the runtime and the transport the PostgreSQL driver needs
//! (chunk 3.1), and serving HTTP (chunk 4.1). Delivery workers, local disk
//! and inbox come with their areas.

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

pub mod http;

use std::collections::BTreeMap;
use std::future::Future;
use std::io;
use std::io::Read as _;
use std::rc::Rc;

use jadpo_drivers::{Endpoint, Stream, Transport};
use jadpo_runtime::{Database, Event, Host, LocalFuture, StoreError};
use tokio::task::LocalSet;

pub use jadpo_drivers::postgres::Postgres;

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
/// off, and Unix sockets on Unix. It connects and spawns only inside
/// [`Runtime::block_on`] (or another tokio `LocalSet`), where its I/O
/// driver and its tasks run.
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

    fn spawn(&self, task: LocalFuture<'static, ()>) {
        tokio::task::spawn_local(task);
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
/// its first transaction begins, inside [`Runtime::block_on`].
pub fn postgres(url: &str) -> Result<Postgres, StoreError> {
    Postgres::new(url, Rc::new(NativeTransport))
}

/// The runtime's host natively: each store by its connection's name, and
/// the operating system's randomness for new identities and request ids.
/// Events are delivered by area 12's workers; until then a committed
/// action's events go nowhere.
#[derive(Debug, Default)]
pub struct NativeHost {
    stores: BTreeMap<String, Postgres>,
}

impl NativeHost {
    pub fn new() -> NativeHost {
        NativeHost::default()
    }

    /// Adds store `name`, a PostgreSQL database.
    pub fn with_store(mut self, name: impl Into<String>, store: Postgres) -> NativeHost {
        self.stores.insert(name.into(), store);
        self
    }
}

impl Host for NativeHost {
    fn database(&self, store: &str) -> Option<&dyn Database> {
        self.stores.get(store).map(|store| store as &dyn Database)
    }

    fn deliver(&self, _events: Vec<Event>) -> LocalFuture<'_, ()> {
        Box::pin(async {})
    }

    fn random(&self, bytes: &mut [u8]) -> bool {
        std::fs::File::open("/dev/urandom")
            .and_then(|mut source| source.read_exact(bytes))
            .is_ok()
    }

    /// tokio's timer, which runs inside [`Runtime::block_on`].
    fn sleep(&self, millis: u64) -> LocalFuture<'_, ()> {
        Box::pin(tokio::time::sleep(std::time::Duration::from_millis(millis)))
    }

    /// The process's monotonic clock, from when it was first read.
    fn monotonic_ms(&self) -> Option<u64> {
        static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
        let start = START.get_or_init(std::time::Instant::now);
        u64::try_from(start.elapsed().as_millis()).ok()
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
