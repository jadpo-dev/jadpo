//! PostgreSQL over TLS against live servers on this machine (area 5,
//! "PostgreSQL over TLS"; chunk 8.1n): disposable servers this file starts
//! on `127.0.0.1`, one offering TLS with a certificate an authority made
//! here signed, one offering none. A server "elsewhere" is a name that
//! isn't this machine's, which the test's transport sends to one of them,
//! so nothing goes past this machine. `JADPO_SKIP_POSTGRES=1` skips them.

use std::io;
use std::rc::Rc;
use std::sync::OnceLock;

use jadpo_drivers::{Endpoint, Stream, Transport, Trust};
use jadpo_host_native::{NativeTransport, Postgres, Runtime, Security};
use jadpo_runtime::{Database, LocalFuture, SqlValue, StoreError};
use jadpo_test::postgres::Server;

use crate::authority::Authority;

/// The name the server's certificate is for: somewhere that isn't this
/// machine.
const ELSEWHERE: &str = "db.example.test";

/// The authority, and the server offering TLS with a certificate it
/// signed for [`ELSEWHERE`] and `localhost`.
fn secure() -> &'static (Authority, Server) {
    static SECURE: OnceLock<(Authority, Server)> = OnceLock::new();
    SECURE.get_or_init(|| {
        let authority = Authority::signing_for(&[ELSEWHERE, "localhost"]);
        let server = Server::start_on_tcp(Some((&authority.certificate_pem, &authority.key_pem)))
            .unwrap_or_else(|error| panic!("{error}"));
        (authority, server)
    })
}

/// A server offering no TLS.
fn plain() -> &'static Server {
    static PLAIN: OnceLock<Server> = OnceLock::new();
    PLAIN.get_or_init(|| Server::start_on_tcp(None).unwrap_or_else(|error| panic!("{error}")))
}

/// The native transport, sending every connection to a name other than
/// this machine's to `127.0.0.1` at `port`, where the test's server is:
/// the name is still what the certificate is checked for.
struct Elsewhere {
    port: u16,
}

impl Transport for Elsewhere {
    fn connect(&self, endpoint: &Endpoint) -> LocalFuture<'_, io::Result<Box<dyn Stream>>> {
        let local = Endpoint::Tcp {
            host: "127.0.0.1".into(),
            port: self.port,
        };
        let endpoint = match endpoint {
            Endpoint::Tcp { host, .. } if host.ends_with(".test") => local,
            other => other.clone(),
        };
        Box::pin(async move { NativeTransport.connect(&endpoint).await })
    }

    fn upgrade<'a>(
        &'a self,
        stream: Box<dyn Stream>,
        name: &'a str,
        trust: Trust<'a>,
    ) -> LocalFuture<'a, io::Result<Box<dyn Stream>>> {
        NativeTransport.upgrade(stream, name, trust)
    }

    fn spawn(&self, task: LocalFuture<'static, ()>) {
        NativeTransport.spawn(task);
    }

    fn sleep(&self, millis: u64) -> LocalFuture<'_, ()> {
        NativeTransport.sleep(millis)
    }
}

/// A store for `host` on the test's server at `port`, secured as
/// `security` says.
fn store(host: &str, port: u16, security: Security) -> Postgres {
    let url = format!("host={host} port={port} user=jadpo dbname=postgres");
    Postgres::secured(&url, Rc::new(Elsewhere { port }), security).expect("a store")
}

/// Whether the store's connection is over TLS, as the server sees it; the
/// store's refusal otherwise.
fn over_tls(store: &Postgres) -> Result<bool, StoreError> {
    let runtime = Runtime::new().expect("a runtime");
    runtime.block_on(async {
        let rows = store
            .query_once(
                "SELECT ssl FROM pg_stat_ssl WHERE pid = pg_backend_pid()",
                &[],
            )
            .await?;
        match rows.as_slice() {
            [row] => Ok(row.as_slice() == [SqlValue::Bool(true)]),
            other => panic!("pg_stat_ssl gave {other:?}"),
        }
    })
}

fn fault(result: Result<bool, StoreError>) -> String {
    match result {
        Err(StoreError::Fault(why)) => why,
        other => panic!("a fault, not {other:?}"),
    }
}

fn authorities(authority: &Authority) -> Security {
    Security::Authorities(
        jadpo_std::database_tls::certificates(&authority.root_pem).expect("the authority's PEM"),
    )
}

#[test]
fn a_server_elsewhere_is_reached_only_over_tls() {
    if !jadpo_test::live() {
        return;
    }
    let port = plain().tcp_port().expect("a port");
    let why = fault(over_tls(&store(ELSEWHERE, port, Security::Roots)));
    assert!(why.contains("`db.example.test` offers no TLS"), "{why}");
    assert!(why.contains("`tls: false`"), "{why}");
}

#[test]
fn tls_false_reaches_a_plain_server_elsewhere() {
    if !jadpo_test::live() {
        return;
    }
    let port = plain().tcp_port().expect("a port");
    assert_eq!(
        over_tls(&store(ELSEWHERE, port, Security::Plain)),
        Ok(false)
    );
}

#[test]
fn this_machine_is_reached_without_tls() {
    if !jadpo_test::live() {
        return;
    }
    let port = plain().tcp_port().expect("a port");
    for host in ["localhost", "127.0.0.1"] {
        assert_eq!(
            over_tls(&store(host, port, Security::Roots)),
            Ok(false),
            "{host}"
        );
    }
    // The secure server is reached without TLS too, though it offers it.
    let port = secure().1.tcp_port().expect("a port");
    assert_eq!(
        over_tls(&store("127.0.0.1", port, Security::Roots)),
        Ok(false)
    );
}

#[test]
fn a_private_authority_named_by_the_connection_is_trusted() {
    if !jadpo_test::live() {
        return;
    }
    let (authority, server) = secure();
    let port = server.tcp_port().expect("a port");
    assert_eq!(
        over_tls(&store(ELSEWHERE, port, authorities(authority))),
        Ok(true)
    );
    // Named, it secures a connection to this machine too.
    assert_eq!(
        over_tls(&store("localhost", port, authorities(authority))),
        Ok(true)
    );
}

#[test]
fn an_authority_mozilla_doesnt_list_is_refused() {
    if !jadpo_test::live() {
        return;
    }
    let port = secure().1.tcp_port().expect("a port");
    let why = fault(over_tls(&store(ELSEWHERE, port, Security::Roots)));
    assert!(
        why.contains(
            "`db.example.test` failed the certificate check against the public authorities"
        ),
        "{why}"
    );
    // Nor is one the connection doesn't name.
    let other = Authority::signing(ELSEWHERE);
    let why = fault(over_tls(&store(ELSEWHERE, port, authorities(&other))));
    assert!(
        why.contains(
            "failed the certificate check against the authority its connection's `authority:` names"
        ),
        "{why}"
    );
}

#[test]
fn a_certificate_for_another_name_is_refused() {
    if !jadpo_test::live() {
        return;
    }
    let (authority, server) = secure();
    let port = server.tcp_port().expect("a port");
    let why = fault(over_tls(&store(
        "other.example.test",
        port,
        authorities(authority),
    )));
    assert!(
        why.contains("`other.example.test` failed the certificate check"),
        "{why}"
    );
}

#[test]
fn an_address_that_sets_tls_itself_is_refused() {
    let url = "postgresql://jadpo@db.example.test/postgres?sslmode=require";
    match jadpo_host_native::postgres(url) {
        Err(StoreError::Fault(why)) => assert!(why.contains("sets `sslmode`"), "{why}"),
        other => panic!("{other:?}"),
    }
    assert!(
        jadpo_std::check_builtin_text(jadpo_std::Builtin::PostgresUrl, url)
            .is_err_and(|why| why.contains("sets `sslmode`")),
    );
}
