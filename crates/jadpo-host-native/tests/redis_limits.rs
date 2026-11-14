//! Rate limits counted in Redis by the native host (area 16, "Where the
//! counts are kept"; chunk Q8): two servers, each its own host and its own
//! connection over the native transport, share one count in one store;
//! a host told to count nowhere counts alone in memory; and a store that
//! can't be reached leaves each server counting in memory, logged once,
//! without holding up the requests that follow. The store is the Redis
//! double on a port of this machine, and a real Redis server too when the
//! run has one (`JADPO_TEST_REDIS_URL`, or `redis-server` on `PATH`).

use std::rc::Rc;
use std::sync::Mutex;
use std::time::Instant;

use jadpo_drivers::redis::Redis;
use jadpo_drivers::redis::testing::{Double, TcpDouble, real};
use jadpo_host_native::{Drivers, NativeHost, NativeTransport, Runtime};
use jadpo_runtime::deployable::{Log, Stamp};
use jadpo_runtime::{Allowed, Host};
use jadpo_std::http::RateLimit;

const TWO_A_MINUTE: RateLimit = RateLimit {
    requests: 2,
    window_ms: 60_000,
};

/// A server's host, counting in the Redis connection `cache` at `url`.
fn server(url: &str) -> NativeHost {
    let store = Redis::new(url, Rc::new(NativeTransport)).expect("a store");
    NativeHost::new()
        .with_drivers(Drivers::default().with_redis("cache", store))
        .counting_in(Some("cache"))
}

/// Two servers' answers to requests that alternate between them, all for
/// one key.
fn alternating(one: &NativeHost, two: &NativeHost, key: &str, requests: usize) -> Vec<Allowed> {
    let runtime = Runtime::new().expect("a runtime");
    runtime.block_on(async {
        let mut answers = Vec::new();
        for at in 0..requests {
            let host = if at % 2 == 0 { one } else { two };
            answers.push(host.rate_limit(key, TWO_A_MINUTE, 0).await);
        }
        answers
    })
}

fn shared_count(url: &str) {
    let (one, two) = (server(url), server(url));
    let answers = alternating(&one, &two, "address 203.0.113.9", 3);
    assert_eq!(&answers[..2], [Allowed::Yes, Allowed::Yes]);
    assert!(
        matches!(answers[2], Allowed::No { retry_after } if (1..=60).contains(&retry_after)),
        "{answers:?}"
    );
}

#[test]
fn two_servers_share_one_count_in_the_double() {
    let double = TcpDouble::start(Double::new());
    shared_count(&double.url(None));
    assert_eq!(double.keys().len(), 1, "{:?}", double.keys());
    assert_eq!(double.connections(), 2, "a connection each");
}

#[test]
fn two_servers_share_one_count_in_real_redis() {
    let Some(redis) = real(1) else {
        return;
    };
    shared_count(&redis.url);
}

/// A host told to count nowhere counts in its own memory: the two servers
/// each give the whole allowance.
#[test]
fn hosts_counting_nowhere_count_alone() {
    let double = TcpDouble::start(Double::new());
    let store = || Redis::new(&double.url(None), Rc::new(NativeTransport)).expect("a store");
    let one = NativeHost::new().with_drivers(Drivers::default().with_redis("cache", store()));
    let two = NativeHost::new().with_drivers(Drivers::default().with_redis("cache", store()));
    let answers = alternating(&one, &two, "address 203.0.113.9", 4);
    assert_eq!(answers, [Allowed::Yes; 4]);
    assert!(double.keys().is_empty());
}

static LINES: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn kept(line: &str) {
    LINES.lock().expect("the lines").push(line.to_string());
}

const STAMP: Stamp = Stamp {
    build: "b_000000000000",
    commit: None,
    modified: false,
    compiler: "0.0.0",
    deployable: "desk",
    target: "native",
};

/// A store that doesn't answer leaves the server counting in its own
/// memory: the request that found it out waited at most its second, the
/// ones after it aren't held up by asking again, and the fallback is
/// logged once, naming the connection and why, never a key.
#[test]
fn a_store_that_doesnt_answer_leaves_the_server_counting_in_memory() {
    let double = TcpDouble::start(Double::new());
    double.go_silent(true);
    let host = server(&double.url(None)).with_log(Log::new(STAMP, kept));
    let runtime = Runtime::new().expect("a runtime");
    let (answers, first, rest) = runtime.block_on(async {
        let began = Instant::now();
        let first = host
            .rate_limit("address 203.0.113.9", TWO_A_MINUTE, 0)
            .await;
        let found_out = began.elapsed();
        let began = Instant::now();
        let mut answers = vec![first];
        for _ in 0..3 {
            answers.push(
                host.rate_limit("address 203.0.113.9", TWO_A_MINUTE, 0)
                    .await,
            );
        }
        (answers, found_out, began.elapsed())
    });
    assert_eq!(&answers[..2], [Allowed::Yes, Allowed::Yes]);
    assert!(matches!(answers[2], Allowed::No { .. }), "{answers:?}");
    assert!(
        first.as_millis() >= 900 && first.as_millis() < 3_000,
        "{first:?}"
    );
    assert!(rest.as_millis() < 200, "not asked again: {rest:?}");
    assert_eq!(double.connections(), 1);
    let lines = LINES.lock().expect("the lines");
    let fallbacks: Vec<serde_json::Value> = lines
        .iter()
        .map(|line| serde_json::from_str(line).expect("JSON"))
        .filter(|line: &serde_json::Value| line["event"] == "rate_limits.fallback")
        .collect();
    assert_eq!(fallbacks.len(), 1, "{lines:?}");
    assert_eq!(fallbacks[0]["connection"], "cache");
    assert_eq!(fallbacks[0]["reason"], "timed_out");
    assert!(
        !lines.iter().any(|line| line.contains("203.0.113.9")),
        "{lines:?}"
    );
}
