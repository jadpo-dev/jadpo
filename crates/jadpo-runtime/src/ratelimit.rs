//! Rate limits counted in memory (area 16, "Rate limits"):
//! what the native host counts with ([`crate::Host::rate_limit`]), fixed
//! windows within a fixed allowance of memory, the oldest counters dropped
//! first. The HTTP component asks the host about every request: 600 a
//! minute per signed-in caller, 120 per address when there is none.
//!
//! One table counts for all of a native server's worker threads (chunk
//! 8.1l; docs/compiler-runtime.md §2.5, "Native accept and one rate-limit
//! count"), so a caller or an address has one allowance a window whichever
//! workers its requests reach. It is plain data behind a lock per shard,
//! held only to find and bump one counter and never across an await.
//! Several servers each count their own, which a Redis connection (area
//! 16) counts once. A route's own limit (`rate_limit:`) has a key of its
//! own and any window from a second to a day.
//!
//! A store shared between servers, a Redis connection's driver, counts
//! through [`CountingStore`] (chunk Q8), and [`counted_by_store`] is how every
//! host asks it: while it can count, its answer stands; when it can't, the
//! host counts in its own memory, as area 16 says, and doesn't ask it
//! again for [`STORE_PAUSE_MS`], so a store that can't be reached holds up
//! one request in that time rather than every one. Each time it stops
//! answering is logged once, as `rate_limits.fallback`, naming the
//! connection and why, never a key.

use std::collections::hash_map::RandomState;
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::hash::BuildHasher;
use std::sync::{Arc, Mutex, PoisonError};

use jadpo_std::http::{RATE_LIMIT_MEMORY, RateLimit};

use crate::LocalFuture;

/// About how much memory one counter takes: its key, held once; its
/// window and count with the map's own overhead; and up to two places in
/// its shard's order.
const ENTRY_BYTES: usize = 128;

/// The most shards a table is split into.
const SHARDS: usize = 16;

/// The fewest counters a shard is given before the table is split again:
/// a small table is one shard, its oldest counter the table's.
const SHARD_COUNTERS: usize = 4096;

/// What a limiter decided about one request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Allowed {
    Yes,
    /// Refused: try again after this many seconds.
    No {
        retry_after: u64,
    },
}

/// One key's window: when it started, in milliseconds, and the requests
/// in it.
#[derive(Debug)]
struct Counter {
    start: i64,
    count: u32,
}

/// The counters of the keys that hash to one shard, within its share of
/// the memory.
#[derive(Debug, Default)]
struct Shard {
    capacity: usize,
    counts: HashMap<Arc<str>, Counter>,
    /// For each window's length a route counts over, each counter's key
    /// with the start of the window it was placed for, in the order the
    /// windows started, oldest first, so the fronts say which window ends
    /// first. A key whose window starts again is placed again at the back,
    /// and its earlier place is passed over.
    order: Vec<Places>,
}

/// The counters of one window's length, by when each started.
#[derive(Debug)]
struct Places {
    window: i64,
    places: VecDeque<(Arc<str>, i64)>,
}

impl Shard {
    fn allow(&mut self, key: &str, limit: RateLimit, now: i64) -> Allowed {
        let window = i64::from(limit.window_ms).max(1);
        let begins = match self.counts.get_key_value(key) {
            None => {
                self.make_room();
                Some(Arc::<str>::from(key))
            }
            // The window has ended, or the clock went back more than a
            // window: a time read a little earlier on another worker
            // counts in the window it falls short of.
            Some((key, counter))
                if now.saturating_sub(counter.start) >= window
                    || counter.start.saturating_sub(now) > window =>
            {
                Some(Arc::clone(key))
            }
            Some(_) => None,
        };
        if let Some(key) = begins {
            self.counts.insert(
                Arc::clone(&key),
                Counter {
                    start: now,
                    count: 0,
                },
            );
            self.place(window, key, now);
        }
        let Some(counter) = self.counts.get_mut(key) else {
            return Allowed::Yes;
        };
        if counter.count >= limit.requests {
            let left = counter
                .start
                .saturating_add(window)
                .saturating_sub(now)
                .clamp(1, window);
            return Allowed::No {
                retry_after: u64::try_from((left + 999) / 1000).unwrap_or(1),
            };
        }
        counter.count += 1;
        Allowed::Yes
    }

    /// Drops the counters whose windows end soonest until there is room
    /// for one more: one whose window has ended goes before any still
    /// counting, whatever each one's length.
    fn make_room(&mut self) {
        while self.counts.len() >= self.capacity {
            let soonest = self
                .order
                .iter_mut()
                .filter_map(|order| {
                    let (_, start) = order.places.front()?;
                    Some((start.saturating_add(order.window), order))
                })
                .min_by_key(|(ends, _)| *ends);
            let Some((key, start)) = soonest.and_then(|(_, order)| order.places.pop_front()) else {
                break;
            };
            if self
                .counts
                .get(&key)
                .is_some_and(|counter| counter.start == start)
            {
                self.counts.remove(&key);
            }
        }
    }

    /// Places `key`'s window, begun at `start`, at the back of its length's
    /// order, leaving out the places passed over once they outnumber the
    /// counters, so the orders hold at most twice the shard's counters.
    fn place(&mut self, window: i64, key: Arc<str>, start: i64) {
        let place = (key, start);
        match self.order.iter_mut().find(|order| order.window == window) {
            Some(order) => order.places.push_back(place),
            None => self.order.push(Places {
                window,
                places: VecDeque::from([place]),
            }),
        }
        if self
            .order
            .iter()
            .map(|order| order.places.len())
            .sum::<usize>()
            > 2 * self.capacity
        {
            let counts = &self.counts;
            for order in &mut self.order {
                order.places.retain(|(key, start)| {
                    counts
                        .get(key)
                        .is_some_and(|counter| counter.start == *start)
                });
            }
        }
    }
}

/// In-memory counters, fixed windows, bounded by memory, shared by every
/// thread that holds the table.
#[derive(Debug)]
pub struct RateLimits {
    shards: Box<[Mutex<Shard>]>,
    /// Chooses a key's shard, with keys random per process, so no one can
    /// aim their keys at one shard.
    hasher: RandomState,
}

impl Default for RateLimits {
    fn default() -> RateLimits {
        RateLimits::new(RATE_LIMIT_MEMORY)
    }
}

impl RateLimits {
    /// Counters within `memory` bytes, shared equally between its shards.
    pub fn new(memory: usize) -> RateLimits {
        let capacity = (memory / ENTRY_BYTES).max(1);
        let count = (capacity / SHARD_COUNTERS).clamp(1, SHARDS);
        let shards = (0..count)
            .map(|at| {
                Mutex::new(Shard {
                    capacity: capacity / count + usize::from(at < capacity % count),
                    ..Shard::default()
                })
            })
            .collect();
        RateLimits {
            shards,
            hasher: RandomState::new(),
        }
    }

    /// Counts a request from `key` at `now` (milliseconds) against `limit`
    /// and says whether it may go ahead.
    pub fn allow(&self, key: &str, limit: RateLimit, now: i64) -> Allowed {
        let shards = self.shards.len() as u64;
        let at = usize::try_from(self.hasher.hash_one(key) % shards.max(1)).unwrap_or(0);
        let Some(shard) = self.shards.get(at) else {
            return Allowed::Yes;
        };
        shard
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .allow(key, limit, now)
    }

    /// How many counters it holds, and how many places their order.
    #[cfg(test)]
    fn held(&self) -> (usize, usize) {
        self.shards
            .iter()
            .fold((0, 0), |(counters, places), shard| {
                let shard = shard.lock().unwrap();
                let held: usize = shard.order.iter().map(|order| order.places.len()).sum();
                (counters + shard.counts.len(), places + held)
            })
    }
}

/// Why a store shared between servers couldn't count a request.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Unasked {
    /// It couldn't be reached, or the connection to it was lost.
    Unreachable,
    /// It didn't answer in time.
    TimedOut,
    /// It answered with something other than a count: a refusal, such as
    /// a password it doesn't take, or what isn't its protocol.
    Failed,
}

impl Unasked {
    /// How `rate_limits.fallback` names it.
    pub fn word(self) -> &'static str {
        match self {
            Unasked::Unreachable => "unreachable",
            Unasked::TimedOut => "timed_out",
            Unasked::Failed => "failed",
        }
    }
}

/// A store servers share their rate limits' counts in (area 16, "Where
/// the counts are kept"): a Redis connection's driver.
pub trait CountingStore {
    /// Counts one more request for `key` against `limit`, or says why it
    /// couldn't.
    fn count<'a>(
        &'a self,
        key: &'a str,
        limit: RateLimit,
    ) -> LocalFuture<'a, Result<Allowed, Unasked>>;
}

/// How long a server, or a Worker's instance, counts in its own memory
/// once its store couldn't count a request, before it asks the store
/// again: 10 seconds.
pub const STORE_PAUSE_MS: u64 = 10_000;

/// Whether each store is being asked, as one server or one Worker's
/// instance sees it: shared by a server's workers, like its table.
#[derive(Debug, Default)]
pub struct StoreHealth {
    stores: Mutex<BTreeMap<String, Health>>,
}

/// One store's health.
#[derive(Clone, Copy, Debug, Default)]
struct Health {
    /// Until when, on the monotonic clock, it isn't asked.
    paused_until: Option<u64>,
    /// Whether its fallback has been logged since it last answered.
    logged: bool,
}

impl StoreHealth {
    /// Whether `store` is asked at `now`, the monotonic clock's
    /// milliseconds: always on a host with no such clock.
    fn asks(&self, store: &str, now: Option<u64>) -> bool {
        let stores = self.stores.lock().unwrap_or_else(PoisonError::into_inner);
        match (
            stores.get(store).and_then(|health| health.paused_until),
            now,
        ) {
            (Some(until), Some(now)) => now >= until,
            _ => true,
        }
    }

    /// `store` answered: it is asked from now on, and its next fallback is
    /// logged.
    fn answered(&self, store: &str) {
        let mut stores = self.stores.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(health) = stores.get_mut(store) {
            *health = Health::default();
        }
    }

    /// `store` couldn't count at `now`: it isn't asked again for
    /// [`STORE_PAUSE_MS`]. Whether this is the first time since it last
    /// answered, which is logged.
    fn failed(&self, store: &str, now: Option<u64>) -> bool {
        let mut stores = self.stores.lock().unwrap_or_else(PoisonError::into_inner);
        let health = stores.entry(store.to_string()).or_default();
        health.paused_until = now.map(|now| now.saturating_add(STORE_PAUSE_MS));
        !std::mem::replace(&mut health.logged, true)
    }
}

/// Counts a request for `key` against `limit` in `store`, connection
/// `connection`, unless `health` says it isn't asked at `now` (the
/// monotonic clock's milliseconds): its answer, or `None` when it couldn't
/// count, and the host counts in its own memory instead. Each time it
/// stops answering, `log` is given the `rate_limits.fallback` event's
/// fields once: the connection, why (`unreachable`, `timed_out` or
/// `failed`) and what that means, never a key.
pub async fn counted_by_store(
    health: &StoreHealth,
    connection: &str,
    store: &dyn CountingStore,
    (key, limit): (&str, RateLimit),
    now: Option<u64>,
    log: impl FnOnce(serde_json::Map<String, serde_json::Value>),
) -> Option<Allowed> {
    if !health.asks(connection, now) {
        return None;
    }
    match store.count(key, limit).await {
        Ok(allowed) => {
            health.answered(connection);
            Some(allowed)
        }
        Err(why) => {
            if health.failed(connection, now) {
                log(fallback(connection, why));
            }
            None
        }
    }
}

/// The `rate_limits.fallback` event's fields for store `connection`,
/// which couldn't count for `why`.
pub fn fallback(connection: &str, why: Unasked) -> serde_json::Map<String, serde_json::Value> {
    let said = match why {
        Unasked::Unreachable => format!("`{connection}` couldn't be reached"),
        Unasked::TimedOut => format!("`{connection}` didn't answer in time"),
        Unasked::Failed => format!("`{connection}` answered with something other than a count"),
    };
    let mut fields = serde_json::Map::new();
    fields.insert("connection".into(), connection.into());
    fields.insert("reason".into(), why.word().into());
    fields.insert(
        "message".into(),
        format!(
            "{said}: each instance counts its rate limits in its own memory, asking again every {} seconds.",
            STORE_PAUSE_MS / 1000
        )
        .into(),
    );
    fields
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_address_gets_its_allowance_a_window() {
        let limits = RateLimits::new(RATE_LIMIT_MEMORY);
        let two = RateLimit {
            requests: 2,
            window_ms: 60_000,
        };
        assert_eq!(limits.allow("a", two, 0), Allowed::Yes);
        assert_eq!(limits.allow("a", two, 10), Allowed::Yes);
        assert_eq!(
            limits.allow("a", two, 1_000),
            Allowed::No { retry_after: 59 }
        );
        assert_eq!(
            limits.allow("b", two, 1_000),
            Allowed::Yes,
            "each address counts alone"
        );
        assert_eq!(limits.allow("a", two, 60_000), Allowed::Yes, "a new window");
    }

    #[test]
    fn the_oldest_counters_go_first_when_memory_runs_out() {
        let limits = RateLimits::new(ENTRY_BYTES * 2);
        let one = RateLimit {
            requests: 1,
            window_ms: 60_000,
        };
        assert_eq!(limits.allow("a", one, 0), Allowed::Yes);
        assert_eq!(limits.allow("b", one, 0), Allowed::Yes);
        assert_eq!(limits.allow("c", one, 0), Allowed::Yes);
        // `a`'s counter was dropped to make room, so it starts again.
        assert_eq!(limits.allow("a", one, 0), Allowed::Yes);
        assert!(matches!(limits.allow("c", one, 0), Allowed::No { .. }));
    }

    const ONE: RateLimit = RateLimit {
        requests: 1,
        window_ms: 60_000,
    };

    /// When memory runs out, the counter whose window started longest ago
    /// goes: one whose window has ended before one still counting, so a
    /// client still being counted isn't given a fresh allowance to make
    /// room (chunk 8.1l).
    #[test]
    fn a_counter_still_counting_outlives_an_idle_one() {
        let limits = RateLimits::new(ENTRY_BYTES * 2);
        assert_eq!(limits.allow("a", ONE, 0), Allowed::Yes);
        assert_eq!(limits.allow("b", ONE, 0), Allowed::Yes);
        // `a` counts again in a new window; `b` has gone quiet.
        assert_eq!(limits.allow("a", ONE, 60_000), Allowed::Yes);
        assert_eq!(limits.allow("c", ONE, 60_000), Allowed::Yes);
        assert!(
            matches!(limits.allow("a", ONE, 60_001), Allowed::No { .. }),
            "`a` is still counted"
        );
        assert!(matches!(limits.allow("c", ONE, 60_001), Allowed::No { .. }));
    }

    /// A route's own limit can count over a window of any length (chunk
    /// G4): a short window that has ended goes before a long one still
    /// counting that started earlier, so an hour's limit isn't handed a
    /// fresh allowance while a minute's dead counter stays.
    #[test]
    fn a_long_window_still_counting_outlives_a_short_one_that_ended() {
        let hour = RateLimit {
            requests: 1,
            window_ms: 3_600_000,
        };
        let limits = RateLimits::new(ENTRY_BYTES * 2);
        assert_eq!(limits.allow("hourly", hour, 0), Allowed::Yes);
        assert_eq!(limits.allow("minutely", ONE, 1_000), Allowed::Yes);
        assert_eq!(limits.allow("next", ONE, 120_000), Allowed::Yes);
        assert!(
            matches!(limits.allow("hourly", hour, 120_001), Allowed::No { .. }),
            "the hour's counter is still counted"
        );
    }

    /// Workers read the clock apart, so a time a little before a
    /// counter's window counts in that window; one more than a window
    /// back means the clock went back, and the window starts again.
    #[test]
    fn a_time_read_a_little_earlier_counts_in_the_same_window() {
        let limits = RateLimits::new(RATE_LIMIT_MEMORY);
        assert_eq!(limits.allow("a", ONE, 100_000), Allowed::Yes);
        assert_eq!(
            limits.allow("a", ONE, 99_999),
            Allowed::No { retry_after: 60 }
        );
        assert_eq!(limits.allow("a", ONE, 39_999), Allowed::Yes);
    }

    /// One table counts every thread's requests (chunk 8.1l): eight
    /// threads asking for one key together are let through 100 times in
    /// all, the allowance, not 100 each.
    #[test]
    fn a_key_is_counted_once_across_threads() {
        let limits = std::sync::Arc::new(RateLimits::new(RATE_LIMIT_MEMORY));
        let hundred = RateLimit {
            requests: 100,
            window_ms: 60_000,
        };
        let threads: Vec<_> = (0..8)
            .map(|_| {
                let limits = std::sync::Arc::clone(&limits);
                std::thread::spawn(move || {
                    (0..50)
                        .filter(|_| limits.allow("address 127.0.0.1", hundred, 0) == Allowed::Yes)
                        .count()
                })
            })
            .collect();
        let allowed: usize = threads.into_iter().map(|t| t.join().unwrap()).sum();
        assert_eq!(allowed, 100);
    }

    /// However many keys come, the table holds no more counters than its
    /// memory allows, nor more than twice as many places in its order,
    /// spread over its shards.
    #[test]
    fn the_table_stays_within_its_memory() {
        let limits = RateLimits::new(ENTRY_BYTES * 20_000);
        for at in 0..200_000i64 {
            let _ = limits.allow(&format!("address {}", at % 50_000), ONE, at * 10);
        }
        let (counters, places) = limits.held();
        assert!(counters <= 20_000, "{counters} counters");
        assert!(places <= 2 * 20_000 + 16, "{places} places");
    }

    /// A store answering as scripted, counting how often it was asked.
    struct Scripted {
        answers: std::cell::RefCell<std::collections::VecDeque<Result<Allowed, Unasked>>>,
        asked: std::cell::Cell<usize>,
    }

    impl CountingStore for Scripted {
        fn count<'a>(
            &'a self,
            _: &'a str,
            _: RateLimit,
        ) -> LocalFuture<'a, Result<Allowed, Unasked>> {
            self.asked.set(self.asked.get() + 1);
            let answer = self
                .answers
                .borrow_mut()
                .pop_front()
                .unwrap_or(Ok(Allowed::Yes));
            Box::pin(async move { answer })
        }
    }

    fn now<F: std::future::Future>(future: F) -> F::Output {
        let mut future = std::pin::pin!(future);
        match future
            .as_mut()
            .poll(&mut std::task::Context::from_waker(std::task::Waker::noop()))
        {
            std::task::Poll::Ready(answer) => answer,
            std::task::Poll::Pending => panic!("answered at once"),
        }
    }

    /// A store's answer stands while it answers; once it can't count, the
    /// host counts in memory and doesn't ask it again for 10 seconds, and
    /// each time it stops answering is logged once (chunk Q8).
    #[test]
    fn a_store_that_cant_count_is_left_alone_for_a_while_and_logged_once() {
        let store = Scripted {
            answers: std::cell::RefCell::new(
                [
                    Ok(Allowed::No { retry_after: 7 }),
                    Err(Unasked::TimedOut),
                    Err(Unasked::Unreachable),
                    Ok(Allowed::Yes),
                    Err(Unasked::Failed),
                ]
                .into(),
            ),
            asked: std::cell::Cell::new(0),
        };
        let health = StoreHealth::default();
        let logged = std::cell::RefCell::new(Vec::new());
        let ask = |at: u64| {
            now(counted_by_store(
                &health,
                "cache",
                &store,
                ("address 203.0.113.9", ONE),
                Some(at),
                |fields| logged.borrow_mut().push(fields),
            ))
        };
        assert_eq!(ask(0), Some(Allowed::No { retry_after: 7 }));
        assert_eq!(ask(1), None, "timed out: counted in memory");
        assert_eq!(ask(5_000), None, "not asked while it rests");
        assert_eq!(store.asked.get(), 2);
        assert_eq!(ask(11_001), None, "asked again, and still down");
        assert_eq!(store.asked.get(), 3);
        assert_eq!(logged.borrow().len(), 1, "one outage, one line");
        assert_eq!(ask(21_001), Some(Allowed::Yes), "back");
        assert_eq!(ask(21_002), None, "down again");
        let logged = logged.borrow();
        assert_eq!(logged.len(), 2, "a second outage, a second line");
        assert_eq!(logged[0]["connection"], "cache");
        assert_eq!(logged[0]["reason"], "timed_out");
        assert_eq!(logged[1]["reason"], "failed");
        assert!(
            logged.iter().all(|fields| !fields
                .values()
                .any(|value| value.to_string().contains("203.0.113.9"))),
            "never a key: {logged:?}"
        );
    }
}
