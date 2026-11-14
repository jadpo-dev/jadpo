//! Caches kept in Redis by the native host (area 5, "Caches"; chunk Q9):
//! two servers, each its own host and its own connection over the native
//! transport, share one cache, so an entry one keeps the other reads, and
//! a clear one makes deletes it for both; `Driver.memory` keeps a cache in
//! this process alone, which every host of the process shares; and
//! `jadpo dev`'s host keeps every cache in memory, never asking Redis.
//! The store is the Redis double on a port of this machine, and a real
//! Redis server too when the run has one (`JADPO_TEST_REDIS_URL`, or
//! `redis-server` on `PATH`).

use std::rc::Rc;

use jadpo_drivers::redis::Redis;
use jadpo_drivers::redis::testing::{Double, TcpDouble, real};
use jadpo_host_native::{Drivers, NativeHost, NativeTransport, Runtime};
use jadpo_runtime::Host;
use jadpo_runtime::caches::Looked;

/// A server's host, keeping its caches in the Redis connection `cache` at
/// `url`.
fn server(url: &str) -> NativeHost {
    let store = Redis::new(url, Rc::new(NativeTransport)).expect("a store");
    NativeHost::new().with_drivers(Drivers::default().with_redis("cache", store))
}

fn tags(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| name.to_string()).collect()
}

/// One server keeps an entry, the other reads it; the other clears what
/// reads products, and neither reads it again, while an entry reading
/// only categories stays.
fn shared_cache(url: &str) {
    let (one, two) = (server(url), server(url));
    let runtime = Runtime::new().expect("a runtime");
    runtime.block_on(async {
        let (first, second) = (
            one.cache("cache").expect("one's cache"),
            two.cache("cache").expect("two's cache"),
        );
        let product = tags(&["Product"]);
        let category = tags(&["Category"]);
        let kept = first
            .keep("jadpo:cache:entry:shelf", b"{\"v\":1}", 60_000, &product, 0)
            .await;
        assert_eq!(kept, Ok(vec![0]));
        first
            .keep(
                "jadpo:cache:entry:aisle",
                b"{\"v\":2}",
                60_000,
                &category,
                0,
            )
            .await
            .expect("kept");
        let looked = second.look("jadpo:cache:entry:shelf", &product, 0).await;
        assert_eq!(
            looked,
            Ok(Looked {
                generations: vec![0],
                entry: Some(b"{\"v\":1}".to_vec())
            })
        );
        second.clear(&product, 0).await.expect("cleared");
        let looked = first.look("jadpo:cache:entry:shelf", &product, 0).await;
        assert_eq!(
            looked,
            Ok(Looked {
                generations: vec![1],
                entry: None
            })
        );
        let aisle = first
            .look("jadpo:cache:entry:aisle", &category, 0)
            .await
            .expect("looked");
        assert_eq!(aisle.entry.as_deref(), Some(&b"{\"v\":2}"[..]));
    });
}

#[test]
fn two_servers_share_one_cache_in_the_double() {
    let double = TcpDouble::start(Double::new());
    shared_cache(&double.url(None));
    assert_eq!(double.connections(), 2, "a connection each");
}

#[test]
fn two_servers_share_one_cache_in_real_redis() {
    let Some(redis) = real(4) else {
        return;
    };
    shared_cache(&redis.url);
}

/// `jadpo dev`'s host keeps every cache in this process's memory, whatever
/// its driver, so nothing reaches the Redis its address names.
#[test]
fn jadpo_devs_host_keeps_every_cache_in_memory() {
    let double = TcpDouble::start(Double::new());
    let host = server(&double.url(None)).caching_in_memory();
    let runtime = Runtime::new().expect("a runtime");
    runtime.block_on(async {
        let cache = host.cache("cache").expect("a cache");
        cache
            .keep(
                "jadpo:cache:entry:dev",
                b"{}",
                60_000,
                &tags(&["Product"]),
                0,
            )
            .await
            .expect("kept");
        let other = server(&double.url(None)).caching_in_memory();
        let looked = other
            .cache("cache")
            .expect("a cache")
            .look("jadpo:cache:entry:dev", &tags(&["Product"]), 0)
            .await
            .expect("looked");
        assert_eq!(
            looked.entry.as_deref(),
            Some(&b"{}"[..]),
            "one process, one cache"
        );
    });
    assert_eq!(double.connections(), 0, "dev never asks Redis");
    assert!(
        NativeHost::new().cache("cache").is_none(),
        "no driver, no cache"
    );
}
