//! Default rate limits (area 16, "Rate limits", question 6), counted in
//! memory: with no Redis store named, each instance counts on its own,
//! within a fixed allowance of memory, the oldest counters dropped first.
//! Every route has the default limits: 600 requests a minute per signed-in
//! caller, 120 per address when there is none. Sign-in comes with area 8,
//! so for now every request counts against its address.
//!
//! A limiter is one thread's: the runtime is single-threaded (§2.6), and
//! the native host sends every connection from one address to the same
//! thread, so an address is counted in one place without a lock.

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};

use jadpo_std::http::{ADDRESS_LIMIT, RATE_LIMIT_MEMORY, RateLimit};

/// About how much memory one counter takes: its key, its window and count,
/// and its place in the eviction queue.
const ENTRY_BYTES: usize = 96;

/// What a limiter decided about one request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Allowed {
    Yes,
    /// Refused: try again after this many seconds.
    No {
        retry_after: u64,
    },
}

#[derive(Debug, Default)]
struct Counters {
    /// By key: the window's start, in milliseconds, and the requests in it.
    counts: HashMap<String, (i64, u32)>,
    /// Keys in the order their counters were made, oldest first.
    order: VecDeque<String>,
}

/// In-memory counters, fixed windows, bounded by memory.
#[derive(Debug)]
pub struct RateLimits {
    limit: RateLimit,
    capacity: usize,
    counters: RefCell<Counters>,
}

impl Default for RateLimits {
    fn default() -> RateLimits {
        RateLimits::new(ADDRESS_LIMIT, RATE_LIMIT_MEMORY)
    }
}

impl RateLimits {
    /// Counters for `limit`, within `memory` bytes.
    pub fn new(limit: RateLimit, memory: usize) -> RateLimits {
        RateLimits {
            limit,
            capacity: (memory / ENTRY_BYTES).max(1),
            counters: RefCell::new(Counters::default()),
        }
    }

    /// Counts a request from `key` at `now` (milliseconds) and says
    /// whether it may go ahead.
    pub fn allow(&self, key: &str, now: i64) -> Allowed {
        let window = i64::from(self.limit.window_ms);
        let mut counters = self.counters.borrow_mut();
        if !counters.counts.contains_key(key) {
            while counters.counts.len() >= self.capacity {
                let Some(oldest) = counters.order.pop_front() else {
                    break;
                };
                counters.counts.remove(&oldest);
            }
            counters.order.push_back(key.to_string());
        }
        let entry = counters.counts.entry(key.to_string()).or_insert((now, 0));
        if now - entry.0 >= window || now < entry.0 {
            *entry = (now, 0);
        }
        if entry.1 >= self.limit.requests {
            let left = (entry.0 + window - now).max(1);
            return Allowed::No {
                retry_after: u64::try_from((left + 999) / 1000).unwrap_or(1),
            };
        }
        entry.1 += 1;
        Allowed::Yes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_address_gets_its_allowance_a_window() {
        let limits = RateLimits::new(
            RateLimit {
                requests: 2,
                window_ms: 60_000,
            },
            RATE_LIMIT_MEMORY,
        );
        assert_eq!(limits.allow("a", 0), Allowed::Yes);
        assert_eq!(limits.allow("a", 10), Allowed::Yes);
        assert_eq!(limits.allow("a", 1_000), Allowed::No { retry_after: 59 });
        assert_eq!(
            limits.allow("b", 1_000),
            Allowed::Yes,
            "each address counts alone"
        );
        assert_eq!(limits.allow("a", 60_000), Allowed::Yes, "a new window");
    }

    #[test]
    fn the_oldest_counters_go_first_when_memory_runs_out() {
        let limits = RateLimits::new(
            RateLimit {
                requests: 1,
                window_ms: 60_000,
            },
            ENTRY_BYTES * 2,
        );
        assert_eq!(limits.allow("a", 0), Allowed::Yes);
        assert_eq!(limits.allow("b", 0), Allowed::Yes);
        assert_eq!(limits.allow("c", 0), Allowed::Yes);
        // `a`'s counter was dropped to make room, so it starts again.
        assert_eq!(limits.allow("a", 0), Allowed::Yes);
        assert!(matches!(limits.allow("c", 0), Allowed::No { .. }));
    }
}
