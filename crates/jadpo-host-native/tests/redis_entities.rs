//! Entities kept in Redis by the native host (area 5, "Entities in
//! Redis"; chunk Q10): the host gives its Redis connection as the store of
//! an entity's records, over the native transport, and it keeps them as
//! the runtime's store in memory does; `jadpo dev`'s host keeps them in
//! this process's memory, never asking Redis. The store is the Redis
//! double on a port of this machine, and a real Redis server too when the
//! run has one (`JADPO_TEST_REDIS_URL`, or `redis-server` on `PATH`).

use std::rc::Rc;

use jadpo_drivers::redis::Redis;
use jadpo_drivers::redis::testing::{Double, TcpDouble, real, records_behave};
use jadpo_host_native::{Drivers, NativeHost, NativeTransport, Runtime};
use jadpo_runtime::Host;

/// A server's host, keeping its entities in the Redis connection `drafts`
/// at `url`.
fn server(url: &str) -> NativeHost {
    let store = Redis::new(url, Rc::new(NativeTransport)).expect("a store");
    NativeHost::new().with_drivers(Drivers::default().with_redis("drafts", store))
}

fn kept(url: &str) {
    let host = server(url);
    let runtime = Runtime::new().expect("a runtime");
    let now = 1_800_000_000_000;
    runtime.block_on(async {
        let records = host.records("drafts").expect("the store");
        records_behave(records.as_ref(), now).await;
    });
}

#[test]
fn records_are_kept_in_the_double() {
    let double = TcpDouble::start(Double::new());
    kept(&double.url(None));
    assert_eq!(double.connections(), 1, "one connection");
}

#[test]
fn records_are_kept_in_real_redis() {
    let Some(redis) = real(5) else {
        return;
    };
    kept(&redis.url);
}

/// `jadpo dev`'s host keeps every entity in Redis in this process's
/// memory, whatever its driver.
#[test]
fn jadpo_devs_host_keeps_records_in_memory() {
    let double = TcpDouble::start(Double::new());
    let host = server(&double.url(None)).caching_in_memory();
    let runtime = Runtime::new().expect("a runtime");
    runtime.block_on(async {
        let records = host.records("drafts").expect("the store");
        records_behave(records.as_ref(), 1_800_000_000_000).await;
    });
    assert_eq!(double.connections(), 0, "dev never asks Redis");
    assert!(
        NativeHost::new().records("drafts").is_none(),
        "no driver, no store"
    );
}
