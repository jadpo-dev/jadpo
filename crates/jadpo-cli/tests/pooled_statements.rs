//! A column of an extension's type read through a pooler (chunk 8.2n). The
//! real deploy read `vector` through Hyperdrive to Neon and got
//! `internal_fault` in 8 of 12 runs: tokio-postgres looks up a type it
//! doesn't know through a *named* statement, and a pooler that hands out a
//! server connection a transaction at a time may run its `Bind` where its
//! `Parse` never ran. So the driver names no statement, ever.
//!
//! [`Pooler`] is what the driver would meet: it stands between the driver
//! and one PostgreSQL connection and, each time that connection goes idle,
//! deallocates every prepared statement on it, as if the next statement had
//! landed on another server connection. It passes the client's own
//! authentication through. Local runs use a server of the test's own on TCP,
//! since the pooler speaks TCP.

use std::collections::VecDeque;
use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use jadpo_runtime::{Database, SqlValue, StoreError};
use jadpo_test::postgres::Server;

/// A database on TCP: its host's port and its name, and how to address it.
struct OnTcp {
    /// Its address with `{}` where its `host:port` goes.
    url: String,
    upstream: String,
    _server: Server,
}

fn on_tcp() -> OnTcp {
    let named = std::env::var(jadpo_test::postgres::DATABASE_URL_VARIABLE)
        .is_ok_and(|url| url.starts_with("postgres://") || url.starts_with("postgresql://"));
    let server = if named {
        Server::start()
    } else {
        Server::start_on_tcp(None)
    }
    .unwrap_or_else(|error| panic!("{error}"));
    let native = server.create_database("pooled").expect("a database");
    let (url, upstream) = match server.tcp_port() {
        // The pairs a disposable server gives, as a URL over TCP.
        Some(port) => {
            let database = native
                .split_whitespace()
                .rev()
                .find_map(|pair| pair.strip_prefix("dbname="))
                .map(|name| name.trim_matches('\''))
                .expect("a database's name");
            (
                format!("postgresql://jadpo@{{}}/{database}"),
                format!("127.0.0.1:{port}"),
            )
        }
        None => {
            let rest = native.split_once("://").expect("a URL").1;
            let (who, rest) = rest.rsplit_once('@').expect("a user");
            let (upstream, database) = rest.split_once('/').expect("a database");
            let scheme = native.split_once("://").expect("a URL").0;
            (
                format!("{scheme}://{who}@{{}}/{database}"),
                upstream.to_string(),
            )
        }
    };
    OnTcp {
        url,
        upstream,
        _server: server,
    }
}

/// What the pooler saw.
#[derive(Default)]
struct Seen {
    /// `Parse` messages naming a statement.
    named: AtomicUsize,
    /// Times it deallocated, as a move to another server connection.
    moved: AtomicUsize,
}

struct Pooler {
    port: u16,
    seen: Arc<Seen>,
}

/// What each `ReadyForQuery` the server sends answers.
enum Asked {
    Client,
    Deallocate,
}

/// What the client's side and the server's side share.
struct Upstream {
    stream: TcpStream,
    asked: VecDeque<Asked>,
}

impl Pooler {
    fn start(upstream: &str) -> Pooler {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
        let port = listener.local_addr().expect("an address").port();
        let seen = Arc::new(Seen::default());
        let upstream = upstream.to_string();
        let seen_by = Arc::clone(&seen);
        std::thread::spawn(move || {
            for client in listener.incoming() {
                let Ok(client) = client else { return };
                let upstream = upstream.clone();
                let seen = Arc::clone(&seen_by);
                std::thread::spawn(move || Pooler::serve(client, &upstream, &seen));
            }
        });
        Pooler { port, seen }
    }

    fn serve(mut client: TcpStream, upstream: &str, seen: &Arc<Seen>) {
        let server = TcpStream::connect(upstream).expect("the server");
        let shared = Arc::new(Mutex::new(Upstream {
            stream: server.try_clone().expect("a clone"),
            asked: VecDeque::new(),
        }));
        let from_server = {
            let shared = Arc::clone(&shared);
            let seen = Arc::clone(seen);
            let mut to_client = client.try_clone().expect("a clone");
            std::thread::spawn(move || {
                let mut server = server;
                Pooler::server_to_client(&mut server, &mut to_client, &shared, &seen);
                let _ = to_client.shutdown(Shutdown::Both);
            })
        };
        Pooler::client_to_server(&mut client, &shared, seen);
        let _ = shared
            .lock()
            .expect("the upstream")
            .stream
            .shutdown(Shutdown::Both);
        let _ = from_server.join();
    }

    /// The client's messages, passed on whole, each `Sync` or `Query`
    /// noted as asking for a `ReadyForQuery`.
    fn client_to_server(client: &mut TcpStream, shared: &Mutex<Upstream>, seen: &Seen) {
        // The startup message has no type byte.
        let Some(startup) = read_length_prefixed(client) else {
            return;
        };
        if shared
            .lock()
            .expect("the upstream")
            .stream
            .write_all(&startup)
            .is_err()
        {
            return;
        }
        while let Some(message) = read_typed(client) {
            let mut upstream = shared.lock().expect("the upstream");
            if message[0] == b'P' {
                // Its statement's name comes straight after its length.
                if message[5] != 0 {
                    seen.named.fetch_add(1, Ordering::SeqCst);
                }
            }
            if matches!(message[0], b'S' | b'Q') {
                upstream.asked.push_back(Asked::Client);
            }
            if upstream.stream.write_all(&message).is_err() {
                return;
            }
        }
    }

    /// The server's messages, passed on but for the answers to its own
    /// `DEALLOCATE`, which it sends each time the connection goes idle.
    fn server_to_client(
        server: &mut TcpStream,
        client: &mut TcpStream,
        shared: &Mutex<Upstream>,
        seen: &Seen,
    ) {
        let mut started = false;
        while let Some(message) = read_typed(server) {
            let deallocating = matches!(
                shared.lock().expect("the upstream").asked.front(),
                Some(Asked::Deallocate)
            );
            if !deallocating && client.write_all(&message).is_err() {
                return;
            }
            if message[0] != b'Z' {
                continue;
            }
            if !started {
                // Authenticated and ready: not a statement's answer.
                started = true;
                continue;
            }
            let mut upstream = shared.lock().expect("the upstream");
            upstream.asked.pop_front();
            if !deallocating && message[5] == b'I' {
                let sql = b"DEALLOCATE ALL\0";
                let mut query = vec![b'Q'];
                query.extend_from_slice(&((sql.len() + 4) as u32).to_be_bytes());
                query.extend_from_slice(sql);
                upstream.asked.push_back(Asked::Deallocate);
                if upstream.stream.write_all(&query).is_err() {
                    return;
                }
                seen.moved.fetch_add(1, Ordering::SeqCst);
            }
        }
    }
}

fn read_length_prefixed(stream: &mut TcpStream) -> Option<Vec<u8>> {
    let mut head = [0u8; 4];
    stream.read_exact(&mut head).ok()?;
    let length = u32::from_be_bytes(head) as usize;
    let mut message = head.to_vec();
    message.resize(length, 0);
    stream.read_exact(&mut message[4..]).ok()?;
    Some(message)
}

fn read_typed(stream: &mut TcpStream) -> Option<Vec<u8>> {
    let mut head = [0u8; 5];
    stream.read_exact(&mut head).ok()?;
    let length = u32::from_be_bytes([head[1], head[2], head[3], head[4]]) as usize;
    let mut message = head.to_vec();
    message.resize(length + 1, 0);
    stream.read_exact(&mut message[5..]).ok()?;
    Some(message)
}

/// The vector the table holds, as the runtime reads it.
fn bits(items: &[f32]) -> SqlValue {
    SqlValue::Vector(items.iter().map(|item| item.to_bits()).collect())
}

#[test]
fn a_vector_is_read_through_a_pooler_that_moves_between_statements() {
    if !jadpo_test::live() {
        return;
    }
    let database = on_tcp();
    let direct = jadpo_host_native::postgres(&database.url.replace("{}", &database.upstream))
        .expect("a store");
    let runtime = jadpo_host_native::Runtime::new().expect("a runtime");
    runtime.block_on(async {
        let mut setup = direct.begin().await.expect("begin");
        for sql in [
            "CREATE EXTENSION IF NOT EXISTS \"vector\"",
            "CREATE TYPE mood AS ENUM ('calm')",
            "CREATE TABLE meaning (id int PRIMARY KEY, felt mood, meaning vector(3))",
            "INSERT INTO meaning VALUES (1, 'calm', '[0.25,0.5,0.75]'), (2, NULL, NULL)",
        ] {
            setup
                .batch(sql)
                .await
                .unwrap_or_else(|e| panic!("{sql}: {e:?}"));
        }
        setup.commit().await.expect("commit");
    });

    let pooler = Pooler::start(&database.upstream);
    let through = database
        .url
        .replace("{}", &format!("127.0.0.1:{}", pooler.port));
    let store = jadpo_host_native::postgres(&through).expect("a store");
    runtime.block_on(async {
        // A statement on its own, as a read runs and the runtime's short
        // steps do: the lookup that fails on a pool has nothing holding it
        // to one server connection.
        for _ in 0..3 {
            let rows = store
                .query_once("SELECT meaning FROM meaning ORDER BY id", &[])
                .await
                .unwrap_or_else(|e| panic!("on its own: {e:?}"));
            assert_eq!(rows[0], vec![bits(&[0.25, 0.5, 0.75])]);
            assert_eq!(
                rows[1],
                vec![SqlValue::Null(jadpo_runtime::SqlType::Vector)]
            );
        }
        // In a transaction, as an action's `create` reads what it wrote.
        let mut transaction = store.begin().await.expect("begin");
        let rows = transaction
            .query(
                "INSERT INTO meaning (id, meaning) VALUES (3, '[1,2,3]') RETURNING meaning",
                &[],
            )
            .await
            .unwrap_or_else(|e| panic!("in a transaction: {e:?}"));
        assert_eq!(rows[0], vec![bits(&[1.0, 2.0, 3.0])]);
        transaction.commit().await.expect("commit");

        // A type the runtime doesn't read is still named by its fault.
        let refused = store
            .query_once("SELECT felt FROM meaning WHERE id = 1", &[])
            .await
            .expect_err("a type the runtime doesn't read");
        let StoreError::Fault(why) = &refused else {
            panic!("{refused:?}");
        };
        assert!(why.contains("mood") && why.contains("felt"), "{why}");
    });

    assert!(
        pooler.seen.moved.load(Ordering::SeqCst) > 0,
        "it never moved"
    );
    assert_eq!(
        pooler.seen.named.load(Ordering::SeqCst),
        0,
        "a named statement went through the pooler"
    );
}
