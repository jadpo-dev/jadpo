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
//! 16) counts once.

use std::collections::hash_map::RandomState;
use std::collections::{HashMap, VecDeque};
use std::hash::BuildHasher;
use std::sync::{Arc, Mutex, PoisonError};

use jadpo_std::http::{RATE_LIMIT_MEMORY, RateLimit};

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
    /// Each counter's key with the start of the window it was placed for,
    /// in the order the windows started, oldest first. A key whose window
    /// starts again is placed again at the back, and its earlier place is
    /// passed over.
    order: VecDeque<(Arc<str>, i64)>,
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
            self.place(key, now);
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

    /// Drops the counters whose windows started longest ago until there
    /// is room for one more: one whose window has ended goes before any
    /// still counting.
    fn make_room(&mut self) {
        while self.counts.len() >= self.capacity {
            let Some((key, start)) = self.order.pop_front() else {
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

    /// Places `key`'s window, begun at `start`, at the back of the order,
    /// leaving out the places passed over once they outnumber the
    /// counters, so the order holds at most twice the shard's counters.
    fn place(&mut self, key: Arc<str>, start: i64) {
        self.order.push_back((key, start));
        if self.order.len() > 2 * self.capacity {
            let counts = &self.counts;
            self.order.retain(|(key, start)| {
                counts
                    .get(key)
                    .is_some_and(|counter| counter.start == *start)
            });
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
                (counters + shard.counts.len(), places + shard.order.len())
            })
    }
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
}
