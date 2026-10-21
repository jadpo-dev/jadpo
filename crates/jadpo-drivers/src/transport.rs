//! How a driver reaches a server: the host opens the connection, secures
//! it, runs the tasks a connection needs and keeps the time, so the same
//! driver runs over tokio's sockets and rustls natively and over
//! `connect()` sockets on Workers (docs/compiler-runtime.md §2.6).

use std::any::Any;
use std::fmt;
use std::future::Future;
use std::io;
use std::net::IpAddr;
use std::task::Poll;

use jadpo_runtime::LocalFuture;
use tokio::io::{AsyncRead, AsyncWrite};

/// A connected byte stream the host opened. `Any`, so the host that made
/// one can read what only it knows of it ([`Transport::local_address`]).
pub trait Stream: AsyncRead + AsyncWrite + Unpin + Any {}

impl<T: AsyncRead + AsyncWrite + Unpin + Any> Stream for T {}

/// Where a server listens.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Endpoint {
    /// A host name or address, and a port.
    Tcp { host: String, port: u16 },
    /// A Unix socket: the file `<dir>/.s.PGSQL.<port>`, as PostgreSQL
    /// names it.
    Unix { dir: String, port: u16 },
}

impl Endpoint {
    /// The socket file of a Unix endpoint.
    pub fn socket_path(&self) -> Option<String> {
        match self {
            Endpoint::Tcp { .. } => None,
            Endpoint::Unix { dir, port } => {
                Some(format!("{}/.s.PGSQL.{port}", dir.trim_end_matches('/')))
            }
        }
    }
}

impl fmt::Display for Endpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Endpoint::Tcp { host, port } => write!(f, "{host}:{port}"),
            Endpoint::Unix { .. } => match self.socket_path() {
                Some(path) => f.write_str(&path),
                None => Ok(()),
            },
        }
    }
}

/// Whom a TLS upgrade trusts to vouch for the server's certificate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trust<'a> {
    /// The host's roots: natively Mozilla's, compiled in.
    Roots,
    /// Only these authorities, each certificate in DER: the private one a
    /// database connection names (area 5, "PostgreSQL over TLS"; chunk
    /// 8.1n).
    Authorities(&'a [Vec<u8>]),
}

/// What a host gives a driver: connections, TLS over them, somewhere to
/// run the task each connection needs, a timer and the clock. Nothing
/// here is `Send`: the runtime is single-threaded by design.
pub trait Transport {
    /// Opens a stream to `endpoint`.
    fn connect(&self, endpoint: &Endpoint) -> LocalFuture<'_, io::Result<Box<dyn Stream>>>;

    /// Turns `stream` into TLS, as its client, checking the server's
    /// certificate against whom `trust` names (the host's roots, or a
    /// connection's own authority since chunk 8.1n) and that it was
    /// issued for `name`, a host name or an address (chunk 8.1b). Nothing
    /// may be
    /// waiting to be read on `stream`: a protocol upgrades where its
    /// conversation says to (SMTP's `STARTTLS`, PostgreSQL's answer to its
    /// SSL request), or before anything is written for one that speaks TLS
    /// from the start. A certificate or name that fails the check is
    /// [`io::ErrorKind::InvalidData`], a name or an authority that can't
    /// be checked [`io::ErrorKind::InvalidInput`], and a host with no TLS
    /// [`io::ErrorKind::Unsupported`]; anything else is the connection's.
    fn upgrade<'a>(
        &'a self,
        stream: Box<dyn Stream>,
        name: &'a str,
        trust: Trust<'a>,
    ) -> LocalFuture<'a, io::Result<Box<dyn Stream>>> {
        let _ = (stream, name, trust);
        Box::pin(async {
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "this host has no TLS",
            ))
        })
    }

    /// Runs `task` alongside the caller, on the same thread, until it ends.
    fn spawn(&self, task: LocalFuture<'static, ()>);

    /// Wakes after `millis` milliseconds: the timer a driver bounds its
    /// waits with.
    fn sleep(&self, millis: u64) -> LocalFuture<'_, ()>;

    /// The time now, in milliseconds since 1970-01-01T00:00:00Z, for what
    /// a driver dates (an email's `Date:`); `None` on a host with no clock.
    fn now_ms(&self) -> Option<i64> {
        None
    }

    /// The address this end of `stream`, one this transport opened, is
    /// at, when the host knows it: what SMTP's `EHLO` names.
    fn local_address(&self, stream: &dyn Stream) -> Option<IpAddr> {
        let _ = stream;
        None
    }

    /// Whether `endpoint` is the platform's own database pooler, which the
    /// host reaches inside the platform rather than over a network:
    /// Cloudflare's Hyperdrive, whose binding gave the address, on Workers
    /// (chunk 8.2i). A driver reaches it without TLS, which the pooler
    /// itself uses to the database. None natively.
    fn is_pooler(&self, endpoint: &Endpoint) -> bool {
        let _ = endpoint;
        false
    }
}

/// `work`'s output, or `None` when `millis` passed first on `transport`'s
/// timer.
pub async fn within<T>(
    transport: &dyn Transport,
    millis: u64,
    work: impl Future<Output = T>,
) -> Option<T> {
    let mut work = std::pin::pin!(work);
    let mut timer = transport.sleep(millis);
    std::future::poll_fn(|cx| {
        if let Poll::Ready(done) = work.as_mut().poll(cx) {
            return Poll::Ready(Some(done));
        }
        if timer.as_mut().poll(cx).is_ready() {
            return Poll::Ready(None);
        }
        Poll::Pending
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_unix_endpoint_names_postgresqls_socket_file() {
        let endpoint = Endpoint::Unix {
            dir: "/tmp/pg/".into(),
            port: 5432,
        };
        assert_eq!(
            endpoint.socket_path().as_deref(),
            Some("/tmp/pg/.s.PGSQL.5432")
        );
        assert_eq!(endpoint.to_string(), "/tmp/pg/.s.PGSQL.5432");
        let tcp = Endpoint::Tcp {
            host: "db".into(),
            port: 6543,
        };
        assert_eq!(tcp.socket_path(), None);
        assert_eq!(tcp.to_string(), "db:6543");
    }
}
