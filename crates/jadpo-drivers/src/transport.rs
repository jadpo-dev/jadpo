//! How a driver reaches a server: the host opens the connection and runs
//! the tasks a connection needs, so the same driver runs over tokio's
//! sockets natively and over `connect()` sockets on Workers
//! (docs/compiler-runtime.md §2.6).

use std::fmt;

use jadpo_runtime::LocalFuture;
use tokio::io::{AsyncRead, AsyncWrite};

/// A connected byte stream the host opened.
pub trait Stream: AsyncRead + AsyncWrite + Unpin {}

impl<T: AsyncRead + AsyncWrite + Unpin> Stream for T {}

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

/// What a host gives a driver: connections, and somewhere to run the task
/// each connection needs. Nothing here is `Send`: the runtime is
/// single-threaded by design.
pub trait Transport {
    /// Opens a stream to `endpoint`.
    fn connect(&self, endpoint: &Endpoint) -> LocalFuture<'_, std::io::Result<Box<dyn Stream>>>;

    /// Runs `task` alongside the caller, on the same thread, until it ends.
    fn spawn(&self, task: LocalFuture<'static, ()>);
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
