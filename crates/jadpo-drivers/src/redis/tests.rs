//! The Redis driver against the in-process double ([`super::testing`]),
//! over pipes on one thread: what goes over the wire and what comes back,
//! signing in, TLS, many callers on one connection, and a connection that
//! breaks, closes or never answers.

use std::cell::{Cell, RefCell};
use std::io;
use std::rc::Rc;

use jadpo_runtime::{Allowed, LocalFuture};
use jadpo_std::http::RateLimit;
use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream};

use super::testing::{Double, Session, commands, encode};
use super::*;
use crate::{Endpoint, Stream, Transport, Trust};

/// How a played server treats each connection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Plays {
    /// Answers every command.
    Answers,
    /// Reads every command and answers none.
    Silent,
    /// Answers one batch of commands, then closes the connection.
    ClosesAfterOne,
}

/// A host whose connections are pipes to one double, played on the same
/// thread, on a clock the test moves.
struct Doubled {
    double: Rc<RefCell<Double>>,
    clock: Rc<Cell<i64>>,
    plays: Cell<Plays>,
    /// Where each connection went.
    connects: RefCell<Vec<Endpoint>>,
    /// Each upgrade asked for: the name, and whether anything had been
    /// written before it.
    upgrades: RefCell<Vec<String>>,
    /// What an upgrade does: succeed, or fail with this.
    upgrade_fails: Cell<Option<io::ErrorKind>>,
    /// Whether nothing listens.
    refused: Cell<bool>,
    /// How many connections' servers have seen their client go.
    goodbyes: Rc<Cell<usize>>,
    /// Whether the timer fires after a few turns of the thread, or never.
    timer_fires: Cell<bool>,
}

impl Doubled {
    fn new(double: Double) -> Rc<Doubled> {
        Rc::new(Doubled {
            double: Rc::new(RefCell::new(double)),
            clock: Rc::new(Cell::new(1_000_000)),
            plays: Cell::new(Plays::Answers),
            connects: RefCell::default(),
            upgrades: RefCell::default(),
            upgrade_fails: Cell::new(None),
            refused: Cell::new(false),
            goodbyes: Rc::default(),
            timer_fires: Cell::new(false),
        })
    }

    /// Another host whose connections reach the same double, as another
    /// server's would.
    fn beside(&self) -> Rc<Doubled> {
        let other = Doubled::new(Double::new());
        Rc::new(Doubled {
            double: Rc::clone(&self.double),
            clock: Rc::clone(&self.clock),
            ..Rc::into_inner(other).expect("one")
        })
    }
}

impl Transport for Doubled {
    fn connect(&self, endpoint: &Endpoint) -> LocalFuture<'_, io::Result<Box<dyn Stream>>> {
        self.connects.borrow_mut().push(endpoint.clone());
        if self.refused.get() {
            return Box::pin(async { Err(io::Error::from(io::ErrorKind::ConnectionRefused)) });
        }
        let (client, server) = tokio::io::duplex(1 << 16);
        tokio::task::spawn_local(play(
            server,
            Rc::clone(&self.double),
            Rc::clone(&self.clock),
            self.plays.get(),
            Rc::clone(&self.goodbyes),
        ));
        Box::pin(async move { Ok(Box::new(client) as Box<dyn Stream>) })
    }

    fn upgrade<'a>(
        &'a self,
        stream: Box<dyn Stream>,
        name: &'a str,
        trust: Trust<'a>,
    ) -> LocalFuture<'a, io::Result<Box<dyn Stream>>> {
        assert_eq!(trust, Trust::Roots, "the host's roots");
        self.upgrades.borrow_mut().push(name.to_string());
        let fails = self.upgrade_fails.get();
        Box::pin(async move {
            match fails {
                Some(kind) => Err(io::Error::from(kind)),
                None => Ok(stream),
            }
        })
    }

    fn spawn(&self, task: LocalFuture<'static, ()>) {
        tokio::task::spawn_local(task);
    }

    fn sleep(&self, _: u64) -> LocalFuture<'_, ()> {
        if self.timer_fires.get() {
            Box::pin(async {
                for _ in 0..50 {
                    tokio::task::yield_now().await;
                }
            })
        } else {
            Box::pin(std::future::pending())
        }
    }
}

/// Plays the double for one connection, as `plays` says, until the
/// client goes.
async fn play(
    mut server: DuplexStream,
    double: Rc<RefCell<Double>>,
    clock: Rc<Cell<i64>>,
    plays: Plays,
    goodbyes: Rc<Cell<usize>>,
) {
    let mut session = Session::default();
    let mut input = Vec::new();
    let mut buffer = [0u8; 4096];
    while let Ok(read) = server.read(&mut buffer).await {
        if read == 0 {
            break;
        }
        input.extend_from_slice(&buffer[..read]);
        let sent = commands(&mut input).expect("commands");
        if sent.is_empty() {
            continue;
        }
        let mut out = Vec::new();
        for words in sent {
            let reply = double.borrow_mut().answer(&mut session, words, clock.get());
            out.extend(encode(&reply));
        }
        if plays == Plays::Silent {
            continue;
        }
        if server.write_all(&out).await.is_err() {
            break;
        }
        if plays == Plays::ClosesAfterOne {
            // Closing its end, as a server that times idle clients out.
            return;
        }
    }
    goodbyes.set(goodbyes.get() + 1);
}

/// Runs `work` on a thread of its own, with what it spawns.
fn on_one_thread<T>(work: impl AsyncFnOnce() -> T) -> T {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("a runtime");
    let local = tokio::task::LocalSet::new();
    local.block_on(&runtime, async {
        let done = work().await;
        // What the drivers spawned has a turn to end.
        for _ in 0..100 {
            tokio::task::yield_now().await;
        }
        done
    })
}

fn store(url: &str, host: &Rc<Doubled>) -> Redis {
    Redis::new(url, Rc::clone(host) as Rc<dyn Transport>).expect("a store")
}

const THREE_A_MINUTE: RateLimit = RateLimit {
    requests: 3,
    window_ms: 60_000,
};

/// Two servers counting in one store share one count and one window,
/// timed by the store's clock: the fourth request between them is
/// refused until the window the first began has ended (area 16, "Where
/// the counts are kept").
#[test]
fn two_servers_counting_in_one_store_share_one_count() {
    let first = Doubled::new(Double::new());
    let second = first.beside();
    let (one, two) = (
        store("redis://cache", &first),
        store("redis://cache", &second),
    );
    let key = "route POST /releases address 203.0.113.9";
    let answers = on_one_thread(async || {
        let mut answers = Vec::new();
        answers.push(one.count(key, THREE_A_MINUTE).await);
        answers.push(two.count(key, THREE_A_MINUTE).await);
        first.clock.set(first.clock.get() + 15_000);
        answers.push(one.count(key, THREE_A_MINUTE).await);
        answers.push(two.count(key, THREE_A_MINUTE).await);
        answers.push(one.count("address 198.51.100.4", THREE_A_MINUTE).await);
        first.clock.set(first.clock.get() + 45_000);
        answers.push(two.count(key, THREE_A_MINUTE).await);
        answers
    });
    assert_eq!(
        answers,
        [
            Ok(Allowed::Yes),
            Ok(Allowed::Yes),
            Ok(Allowed::Yes),
            Ok(Allowed::No { retry_after: 45 }),
            Ok(Allowed::Yes),
            Ok(Allowed::Yes),
        ]
    );
    // Each count is kept under a hash of what it counts, never the
    // address as written.
    let keys = first.double.borrow().keys(0, first.clock.get());
    assert_eq!(keys.len(), 2, "{keys:?}");
    assert!(keys.iter().all(|key| key.starts_with(RATE_LIMIT_PREFIX)
        && key.len() == RATE_LIMIT_PREFIX.len() + 32
        && !key.contains("203.0.113.9")));
    assert!(keys.contains(&rate_limit_key(key)));
    assert_eq!(
        first.connects.borrow().as_slice(),
        [Endpoint::Tcp {
            host: "cache".into(),
            port: 6379
        }]
    );
}

/// What one count sends: one transaction, in one write.
#[test]
fn a_count_is_one_transaction() {
    let host = Doubled::new(Double::new());
    let redis = store("redis://cache", &host);
    let answered = on_one_thread(async || redis.count("user 7", THREE_A_MINUTE).await);
    assert_eq!(answered, Ok(Allowed::Yes));
    assert_eq!(
        host.double.borrow().sent,
        ["MULTI", "SET", "INCR", "PTTL", "EXEC"]
    );
    let count = host
        .double
        .borrow()
        .get(0, &rate_limit_key("user 7"), host.clock.get());
    assert_eq!(count.as_deref(), Some("1"));
}

/// A connection signs in and chooses its database before anything else,
/// written ahead of the first count rather than waited for.
#[test]
fn a_connection_signs_in_and_chooses_its_database_first() {
    let suffix = std::process::id();
    let host = Doubled::new(Double::signing_in(Some("app"), &format!("p@ss{suffix}")));
    let redis = store(&format!("redis://app:p%40ss{suffix}@cache:6380/3"), &host);
    let answered = on_one_thread(async || redis.count("user 7", THREE_A_MINUTE).await);
    assert_eq!(answered, Ok(Allowed::Yes));
    let double = host.double.borrow();
    assert_eq!(
        double.sent,
        ["AUTH", "SELECT", "MULTI", "SET", "INCR", "PTTL", "EXEC"]
    );
    assert_eq!(double.keys(3, host.clock.get()), [rate_limit_key("user 7")]);
    assert!(double.keys(0, host.clock.get()).is_empty());
    assert_eq!(
        host.connects.borrow().as_slice(),
        [Endpoint::Tcp {
            host: "cache".into(),
            port: 6380
        }]
    );
}

/// A password the server doesn't take is its refusal, and the
/// connection is done: the next ask opens another.
#[test]
fn a_refused_sign_in_is_the_servers_refusal_and_ends_the_connection() {
    let host = Doubled::new(Double::signing_in(
        None,
        &format!("right{}", std::process::id()),
    ));
    let redis = store("redis://:wrong@cache", &host);
    let (first, second) = on_one_thread(async || {
        let first = redis.count("user 7", THREE_A_MINUTE).await;
        let second = redis.count("user 7", THREE_A_MINUTE).await;
        (first, second)
    });
    for answered in [&first, &second] {
        assert!(
            matches!(answered, Err(Error::Refused(message)) if message.starts_with("WRONGPASS")),
            "{answered:?}"
        );
    }
    assert_eq!(host.connects.borrow().len(), 2);
    assert_eq!(first.unwrap_err().reason(), "failed");
}

/// `rediss://` is TLS from the first byte, for the host's name against
/// the host's roots; a certificate that doesn't check is reported, not
/// tried again in the clear.
#[test]
fn rediss_is_tls_from_the_first_byte() {
    let host = Doubled::new(Double::new());
    let redis = store("rediss://default:pw@eu1-cosmic-ant.upstash.io", &host);
    let answered = on_one_thread(async || redis.count("user 7", THREE_A_MINUTE).await);
    // The double takes no password, so it refuses the `AUTH`: what matters
    // here is that it came over the upgraded stream.
    assert!(matches!(answered, Err(Error::Refused(_))), "{answered:?}");
    assert_eq!(
        host.upgrades.borrow().as_slice(),
        ["eu1-cosmic-ant.upstash.io"]
    );

    let host = Doubled::new(Double::new());
    host.upgrade_fails.set(Some(io::ErrorKind::InvalidData));
    let redis = store("rediss://cache", &host);
    let answered = on_one_thread(async || redis.count("user 7", THREE_A_MINUTE).await);
    assert_eq!(answered, Err(Error::Insecure));
    assert_eq!(answered.unwrap_err().reason(), "failed");
    assert!(
        host.double.borrow().sent.is_empty(),
        "nothing sent in the clear"
    );

    let plain = Doubled::new(Double::new());
    let redis = store("redis://cache", &plain);
    let _ = on_one_thread(async || redis.count("user 7", THREE_A_MINUTE).await);
    assert!(plain.upgrades.borrow().is_empty());
}

/// Many callers at once share one connection, each given its own
/// replies, in the order it asked.
#[test]
fn callers_at_once_share_one_connection() {
    let host = Doubled::new(Double::new());
    let redis = store("redis://cache", &host);
    let limit = RateLimit {
        requests: 5,
        window_ms: 10_000,
    };
    let answers = on_one_thread(async || {
        let asks: Vec<_> = (0..8).map(|_| redis.count("user 7", limit)).collect();
        let mut answers = Vec::new();
        for ask in futures_in_turn(asks).await {
            answers.push(ask);
        }
        answers
    });
    let allowed = answers
        .iter()
        .filter(|answer| **answer == Ok(Allowed::Yes))
        .count();
    let refused = answers
        .iter()
        .filter(|answer| matches!(answer, Ok(Allowed::No { .. })))
        .count();
    assert_eq!((allowed, refused), (5, 3), "{answers:?}");
    assert_eq!(host.connects.borrow().len(), 1);
}

/// Polls every future in `asks` together until all have answered.
async fn futures_in_turn<T>(asks: Vec<impl Future<Output = T>>) -> Vec<T> {
    let mut asks: Vec<_> = asks.into_iter().map(Box::pin).collect();
    let mut done: Vec<Option<T>> = asks.iter().map(|_| None).collect();
    std::future::poll_fn(|cx| {
        for (ask, slot) in asks.iter_mut().zip(done.iter_mut()) {
            if slot.is_none()
                && let std::task::Poll::Ready(answer) = ask.as_mut().poll(cx)
            {
                *slot = Some(answer);
            }
        }
        if done.iter().all(Option::is_some) {
            std::task::Poll::Ready(())
        } else {
            std::task::Poll::Pending
        }
    })
    .await;
    done.into_iter().flatten().collect()
}

/// A server that reads and never answers is given up on when the ask's
/// time is up, and the next ask opens another connection rather than
/// queue behind it.
#[test]
fn a_server_that_never_answers_times_out_and_the_next_ask_opens_another() {
    let host = Doubled::new(Double::new());
    host.plays.set(Plays::Silent);
    host.timer_fires.set(true);
    let redis = store("redis://cache", &host);
    let (first, second) = on_one_thread(async || {
        let first = redis.count("user 7", THREE_A_MINUTE).await;
        host.plays.set(Plays::Answers);
        let second = redis.count("user 7", THREE_A_MINUTE).await;
        (first, second)
    });
    assert_eq!(first, Err(Error::TimedOut));
    assert_eq!(first.unwrap_err().reason(), "timed_out");
    assert_eq!(second, Ok(Allowed::Yes));
    assert_eq!(host.connects.borrow().len(), 2);
}

/// A server that closes an idle connection, as services that time idle
/// clients out do, is seen at once: the next ask opens another rather
/// than fail on the one that closed.
#[test]
fn a_connection_closed_while_idle_is_opened_again() {
    let host = Doubled::new(Double::new());
    host.plays.set(Plays::ClosesAfterOne);
    let redis = store("redis://cache", &host);
    let answers = on_one_thread(async || {
        let first = redis.count("user 7", THREE_A_MINUTE).await;
        for _ in 0..20 {
            tokio::task::yield_now().await;
        }
        let second = redis.count("user 7", THREE_A_MINUTE).await;
        [first, second]
    });
    assert_eq!(answers, [Ok(Allowed::Yes), Ok(Allowed::Yes)]);
    assert_eq!(host.connects.borrow().len(), 2);
}

/// Nothing listening is `unreachable`, which the host counts in memory
/// for; the driver tries again at the next ask.
#[test]
fn a_server_nothing_listens_at_is_unreachable() {
    let host = Doubled::new(Double::new());
    host.refused.set(true);
    let redis = store("redis://cache", &host);
    let answers = on_one_thread(async || {
        [
            redis.count("user 7", THREE_A_MINUTE).await,
            redis.count("user 7", THREE_A_MINUTE).await,
        ]
    });
    assert_eq!(answers, [Err(Error::Unreachable), Err(Error::Unreachable)]);
    assert_eq!(answers[0].clone().unwrap_err().reason(), "unreachable");
    assert_eq!(host.connects.borrow().len(), 2);
}

/// Once the driver is dropped and nothing waits, its connection's task
/// says goodbye and ends, so a Worker's invocation isn't kept for it.
#[test]
fn a_dropped_driver_closes_its_connection() {
    let host = Doubled::new(Double::new());
    on_one_thread(async || {
        let redis = store("redis://cache", &host);
        assert_eq!(
            redis.count("user 7", THREE_A_MINUTE).await,
            Ok(Allowed::Yes)
        );
        drop(redis);
    });
    assert_eq!(host.goodbyes.get(), 1);
}

/// Any commands go the same way, their replies in order, a refusal among
/// them as it came.
#[test]
fn any_commands_are_run_in_one_write() {
    let host = Doubled::new(Double::new());
    let redis = store("redis://cache", &host);
    let words = |words: &[&str]| -> Vec<Vec<u8>> {
        words.iter().map(|word| word.as_bytes().to_vec()).collect()
    };
    let replies = on_one_thread(async || {
        redis
            .run(&[
                words(&["SET", "greeting", "kept"]),
                words(&["GET", "greeting"]),
                words(&["INCR", "greeting"]),
                words(&["PING"]),
            ])
            .await
    });
    assert_eq!(
        replies,
        Ok(vec![
            Reply::Status("OK".into()),
            Reply::Bulk(Some(b"kept".to_vec())),
            Reply::Error("ERR value is not an integer or out of range".into()),
            Reply::Status("PONG".into()),
        ])
    );
}

/// The double answers as Redis does where the driver relies on it.
#[test]
fn the_double_answers_as_redis_does() {
    let mut double = Double::new();
    let mut session = Session::default();
    let mut say = |words: &[&str], now: i64| {
        double.answer(
            &mut session,
            words.iter().map(|word| word.as_bytes().to_vec()).collect(),
            now,
        )
    };
    assert_eq!(
        say(&["SET", "k", "0", "PX", "1000", "NX"], 0),
        Reply::Status("OK".into())
    );
    assert_eq!(
        say(&["SET", "k", "0", "PX", "1000", "NX"], 1),
        Reply::Bulk(None)
    );
    assert_eq!(say(&["INCR", "k"], 2), Reply::Integer(1));
    assert_eq!(say(&["PTTL", "k"], 400), Reply::Integer(600));
    assert_eq!(say(&["PTTL", "k"], 1_000), Reply::Integer(-2), "expired");
    assert_eq!(
        say(&["EXEC"], 0),
        Reply::Error("ERR EXEC without MULTI".into())
    );
    assert_eq!(say(&["MULTI"], 0), Reply::Status("OK".into()));
    assert_eq!(say(&["INCR", "n"], 0), Reply::Status("QUEUED".into()));
    assert_eq!(
        say(&["EXEC"], 0),
        Reply::Array(Some(vec![Reply::Integer(1)]))
    );
}
