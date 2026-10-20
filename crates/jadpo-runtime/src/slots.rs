//! Calls at once through a connection (area 11, "Calling"; chunk 7.6c):
//! a provider package's `max_concurrent:`, or a connection's lower one,
//! caps how many of its calls are out at once per app instance. A call
//! takes one of the connection's slots before it goes out and gives it
//! back once answered; a further call waits for one, and a subscriber's
//! delivery that would wait is given back to run later
//! ([`Slots::full`]).
//!
//! An app instance is a process natively, whose worker threads share one
//! set ([`Slots::process`], what a host gives unless it keeps its own),
//! and an isolate on Workers. A test's host keeps its own, so tests that
//! run side by side never wait on each other.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::task::{Context, Poll, Waker};

/// How long a delivery given back while its connection's slots are all
/// out waits before it is due again: the next sweep takes it.
pub const WAIT_MS: i64 = 1_000;

/// Each connection's slots in use, and who waits for one.
#[derive(Debug, Default)]
pub struct Slots {
    held: Mutex<HashMap<String, Held>>,
}

#[derive(Debug, Default)]
struct Held {
    /// The cap the last call through it was made under.
    limit: u32,
    /// How many of its calls are out.
    busy: u32,
    /// The calls waiting for a slot, woken when one is given back.
    waiting: Vec<Waker>,
}

impl Slots {
    pub fn new() -> Slots {
        Slots::default()
    }

    /// The process's own set, which every host without its own shares.
    pub fn process() -> &'static Slots {
        static PROCESS: OnceLock<Slots> = OnceLock::new();
        PROCESS.get_or_init(Slots::new)
    }

    fn held(&self) -> MutexGuard<'_, HashMap<String, Held>> {
        // A thread that panicked while it held the lock left the counts
        // whole: each change is one statement.
        self.held
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// A slot of `connection`'s, at most `limit` of them out at once:
    /// given at once while one is free, otherwise once one is given back.
    pub fn take<'a>(&'a self, connection: &str, limit: u32) -> Take<'a> {
        Take {
            slots: self,
            connection: connection.to_string(),
            limit: limit.max(1),
        }
    }

    /// A slot of `connection`'s now, while one of `limit` is free.
    pub fn try_take(&self, connection: &str, limit: u32) -> Option<Slot<'_>> {
        let mut held = self.held();
        let entry = held.entry(connection.to_string()).or_default();
        entry.limit = limit.max(1);
        (entry.busy < entry.limit).then(|| {
            entry.busy += 1;
            Slot {
                slots: self,
                connection: connection.to_string(),
            }
        })
    }

    /// Whether every slot of `connection`'s is out now, under the cap its
    /// last call was made with: a call through it would wait.
    pub fn full(&self, connection: &str) -> bool {
        self.held()
            .get(connection)
            .is_some_and(|entry| entry.busy >= entry.limit)
    }

    /// How many of `connection`'s calls are out now.
    pub fn busy(&self, connection: &str) -> u32 {
        self.held().get(connection).map_or(0, |entry| entry.busy)
    }

    fn give_back(&self, connection: &str) {
        let waiting = {
            let mut held = self.held();
            let Some(entry) = held.get_mut(connection) else {
                return;
            };
            entry.busy = entry.busy.saturating_sub(1);
            std::mem::take(&mut entry.waiting)
        };
        // Every waiter looks again: one takes the slot, the rest wait
        // again, and one that stopped waiting is woken for nothing.
        for waker in waiting {
            waker.wake();
        }
    }
}

/// A call's slot, given back when it is dropped.
#[derive(Debug)]
pub struct Slot<'a> {
    slots: &'a Slots,
    connection: String,
}

impl Drop for Slot<'_> {
    fn drop(&mut self) {
        self.slots.give_back(&self.connection);
    }
}

/// Waiting for a slot ([`Slots::take`]).
#[derive(Debug)]
pub struct Take<'a> {
    slots: &'a Slots,
    connection: String,
    limit: u32,
}

impl<'a> Future for Take<'a> {
    type Output = Slot<'a>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Slot<'a>> {
        let this = self.get_mut();
        let mut held = this.slots.held();
        let entry = held.entry(this.connection.clone()).or_default();
        entry.limit = this.limit;
        if entry.busy < entry.limit {
            entry.busy += 1;
            return Poll::Ready(Slot {
                slots: this.slots,
                connection: this.connection.clone(),
            });
        }
        entry.waiting.push(cx.waker().clone());
        Poll::Pending
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::task::Wake;

    use super::*;

    struct Count(AtomicUsize);

    impl Wake for Count {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn poll<F: Future + Unpin>(future: &mut F, waker: &Waker) -> Poll<F::Output> {
        Pin::new(future).poll(&mut Context::from_waker(waker))
    }

    /// Two slots: the third call waits, and goes once one is given back;
    /// another connection's calls never wait on these.
    #[test]
    fn a_call_past_the_cap_waits_for_a_slot_given_back() {
        let slots = Slots::new();
        let woken = Arc::new(Count(AtomicUsize::new(0)));
        let waker = Waker::from(Arc::clone(&woken));
        let Poll::Ready(first) = poll(&mut slots.take("fuel", 2), &waker) else {
            panic!("a free slot");
        };
        let Poll::Ready(_second) = poll(&mut slots.take("fuel", 2), &waker) else {
            panic!("a free slot");
        };
        assert!(slots.full("fuel"));
        assert!(!slots.full("couriers"));
        assert!(slots.try_take("couriers", 1).is_some());
        let mut third = slots.take("fuel", 2);
        assert!(poll(&mut third, &waker).is_pending());
        assert!(slots.try_take("fuel", 2).is_none());
        assert_eq!(woken.0.load(Ordering::SeqCst), 0);
        drop(first);
        assert_eq!(woken.0.load(Ordering::SeqCst), 1);
        assert!(!slots.full("fuel"));
        let Poll::Ready(third) = poll(&mut third, &waker) else {
            panic!("the slot given back");
        };
        assert_eq!(slots.busy("fuel"), 2);
        drop(third);
        assert_eq!(slots.busy("fuel"), 1);
    }

    /// A cap lowered since a call went out holds further calls until the
    /// calls out are under it.
    #[test]
    fn the_cap_a_call_is_made_under_holds_from_then_on() {
        let slots = Slots::new();
        let a = slots.try_take("fuel", 3).expect("a slot");
        let b = slots.try_take("fuel", 3).expect("a slot");
        assert!(slots.try_take("fuel", 1).is_none());
        assert!(slots.full("fuel"));
        drop(a);
        assert!(slots.full("fuel"));
        drop(b);
        assert!(slots.try_take("fuel", 1).is_some());
    }
}
