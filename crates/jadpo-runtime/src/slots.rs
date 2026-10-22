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
//!
//! On Workers (chunk 8.2j) an isolate's invocations share its set, but no
//! invocation's work may be resumed by another's: Workers settles each
//! invocation's promises in its own context, and a task one invocation
//! wakes is polled in the waker's. So there the set is a looking one
//! ([`Slots::looking`]): a slot given back is handed to the call that has
//! waited longest, waking no one, and that call finds it on its own timer,
//! looking again every [`LOOK_MS`] ([`wait`]). An invocation can end
//! without giving back what it held, since Workers cancels one whose client
//! went away and its futures are never dropped, so a call holding a slot
//! says so every [`BEAT_MS`] ([`keeping`]), and a holder or a waiter not
//! heard from for [`GONE_MS`] is forgotten.

use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::pin::Pin;
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::task::{Context, Poll, Waker};

use crate::Host;

/// How long a delivery given back while its connection's slots are all
/// out waits before it is due again: the next sweep takes it.
pub const WAIT_MS: i64 = 1_000;

/// How often a call waiting in a looking set looks for the slot it may
/// have been handed (chunk 8.2j).
pub const LOOK_MS: u64 = 10;

/// How often a call holding a slot of a looking set says it still holds
/// it.
pub const BEAT_MS: u64 = 1_000;

/// How long a looking set waits to hear from a holder or a waiter before
/// it counts it gone: five beats, and five hundred looks.
pub const GONE_MS: u64 = 5_000;

/// A looking set's clock: milliseconds on the platform's monotonic clock,
/// as the host gives them.
pub type Clock = fn() -> Option<u64>;

/// Each connection's slots in use, and who waits for one.
#[derive(Debug, Default)]
pub struct Slots {
    held: Mutex<Table>,
    /// A looking set's clock; `None` for a set whose waiters are woken.
    looking: Option<Clock>,
}

#[derive(Debug, Default)]
struct Table {
    connections: HashMap<String, Held>,
    /// The number the next holder or waiter of a looking set is known by.
    next: u64,
}

#[derive(Debug, Default)]
struct Held {
    /// The cap the last call through it was made under.
    limit: u32,
    /// How many of its calls are out, where waiters are woken.
    busy: u32,
    /// The calls waiting for a slot, woken when one is given back.
    waiting: Vec<Waker>,
    /// A looking set's calls out, by number, and when each was last heard
    /// from.
    holders: Vec<(u64, Option<u64>)>,
    /// A looking set's calls waiting, longest first.
    queue: VecDeque<Waiter>,
}

/// A call waiting in a looking set.
#[derive(Debug)]
struct Waiter {
    number: u64,
    /// When it last looked.
    looked: Option<u64>,
    /// Whether a slot has been handed to it, which it takes when it looks.
    handed: bool,
}

/// Whether `heard`, when something was last heard from, is longer ago than
/// [`GONE_MS`] at `now`.
fn gone(heard: Option<u64>, now: Option<u64>) -> bool {
    matches!((heard, now), (Some(heard), Some(now)) if heard.saturating_add(GONE_MS) < now)
}

impl Held {
    /// How many slots are out: held, or handed to a waiter that hasn't
    /// taken it yet.
    fn out(&self) -> u32 {
        let held = self.holders.len() + self.queue.iter().filter(|w| w.handed).count();
        u32::try_from(held).unwrap_or(u32::MAX).max(self.busy)
    }

    /// Forgets the holders and waiters not heard from since `GONE_MS`
    /// before `now`.
    fn forget(&mut self, now: Option<u64>) {
        self.holders.retain(|(_, heard)| !gone(*heard, now));
        self.queue.retain(|waiter| !gone(waiter.looked, now));
    }

    /// Hands each free slot to the longest waiter not handed one yet.
    fn hand(&mut self) {
        let mut out = self.out();
        for waiter in &mut self.queue {
            if out >= self.limit {
                break;
            }
            if !waiter.handed {
                waiter.handed = true;
                out += 1;
            }
        }
    }
}

impl Slots {
    pub fn new() -> Slots {
        Slots::default()
    }

    /// A looking set on `clock` (chunk 8.2j): what a Workers isolate's
    /// invocations share, where a slot given back is handed to the longest
    /// waiter and woken for no one, and a holder or a waiter not heard from
    /// for [`GONE_MS`] is forgotten. Its calls wait through [`wait`] and
    /// hold a slot through [`keeping`].
    pub fn looking(clock: Clock) -> Slots {
        Slots {
            held: Mutex::default(),
            looking: Some(clock),
        }
    }

    /// The process's own set, which every host without its own shares.
    pub fn process() -> &'static Slots {
        static PROCESS: OnceLock<Slots> = OnceLock::new();
        PROCESS.get_or_init(Slots::new)
    }

    /// Whether this is a looking set.
    pub fn is_looking(&self) -> bool {
        self.looking.is_some()
    }

    fn now(&self) -> Option<u64> {
        self.looking.and_then(|clock| clock())
    }

    fn held(&self) -> MutexGuard<'_, Table> {
        // A thread that panicked while it held the lock left the counts
        // whole: each change is one statement.
        self.held
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// A slot of `connection`'s, at most `limit` of them out at once:
    /// given at once while one is free, otherwise once one is given back.
    /// A looking set's is waited for through [`wait`], which looks again
    /// on the host's timer, since nobody wakes it.
    pub fn take<'a>(&'a self, connection: &str, limit: u32) -> Take<'a> {
        Take {
            slots: self,
            connection: connection.to_string(),
            limit: limit.max(1),
            number: None,
        }
    }

    /// A slot of `connection`'s now, while one of `limit` is free and, in
    /// a looking set, nobody waits for one.
    pub fn try_take(&self, connection: &str, limit: u32) -> Option<Slot<'_>> {
        let now = self.now();
        let mut table = self.held();
        let number = table.next;
        let entry = table.connections.entry(connection.to_string()).or_default();
        entry.limit = limit.max(1);
        entry.forget(now);
        entry.hand();
        if entry.out() >= entry.limit || entry.queue.iter().any(|waiter| !waiter.handed) {
            return None;
        }
        if self.is_looking() {
            entry.holders.push((number, now));
            table.next += 1;
        } else {
            entry.busy += 1;
        }
        Some(Slot {
            slots: self,
            connection: connection.to_string(),
            number,
        })
    }

    /// Whether every slot of `connection`'s is out now, under the cap its
    /// last call was made with: a call through it would wait. In a looking
    /// set, a holder gone quiet doesn't count.
    pub fn full(&self, connection: &str) -> bool {
        let now = self.now();
        let mut table = self.held();
        let Some(entry) = table.connections.get_mut(connection) else {
            return false;
        };
        entry.forget(now);
        entry.hand();
        entry.out() >= entry.limit
    }

    /// How many of `connection`'s calls are out now.
    pub fn busy(&self, connection: &str) -> u32 {
        self.held().connections.get(connection).map_or(0, Held::out)
    }

    fn give_back(&self, connection: &str, number: u64) {
        let waiting = {
            let mut table = self.held();
            let Some(entry) = table.connections.get_mut(connection) else {
                return;
            };
            if self.is_looking() {
                entry.holders.retain(|(held, _)| *held != number);
                entry.hand();
                return;
            }
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
    /// What a looking set knows it by.
    number: u64,
}

impl Slot<'_> {
    /// Tells a looking set its holder is still there, so it isn't
    /// forgotten as one whose invocation ended ([`keeping`]).
    pub fn beat(&self) {
        let now = self.slots.now();
        let mut table = self.slots.held();
        if let Some(entry) = table.connections.get_mut(&self.connection)
            && let Some((_, heard)) = entry
                .holders
                .iter_mut()
                .find(|(held, _)| *held == self.number)
        {
            *heard = now;
        }
    }
}

impl Drop for Slot<'_> {
    fn drop(&mut self) {
        self.slots.give_back(&self.connection, self.number);
    }
}

/// Waiting for a slot ([`Slots::take`]).
#[derive(Debug)]
pub struct Take<'a> {
    slots: &'a Slots,
    connection: String,
    limit: u32,
    /// Its place in a looking set's queue, once it has looked.
    number: Option<u64>,
}

impl<'a> Take<'a> {
    /// In a looking set, the slot handed to this call, if one has been:
    /// the first look joins the queue behind every call waiting already,
    /// and each says the call is still waiting.
    pub fn look(&mut self) -> Option<Slot<'a>> {
        let slots = self.slots;
        let now = slots.now();
        let mut table = slots.held();
        let number = match self.number {
            Some(number) => number,
            None => {
                let number = table.next;
                table.next += 1;
                self.number = Some(number);
                number
            }
        };
        let entry = table
            .connections
            .entry(self.connection.clone())
            .or_default();
        entry.limit = self.limit;
        match entry
            .queue
            .iter_mut()
            .find(|waiter| waiter.number == number)
        {
            Some(waiter) => waiter.looked = now,
            // Its first look, or one after it was counted gone, which puts
            // it at the back.
            None => entry.queue.push_back(Waiter {
                number,
                looked: now,
                handed: false,
            }),
        }
        entry.forget(now);
        entry.hand();
        let at = entry
            .queue
            .iter()
            .position(|waiter| waiter.number == number && waiter.handed)?;
        entry.queue.remove(at);
        entry.holders.push((number, now));
        self.number = None;
        Some(Slot {
            slots,
            connection: self.connection.clone(),
            number,
        })
    }
}

impl<'a> Future for Take<'a> {
    type Output = Slot<'a>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Slot<'a>> {
        let this = self.get_mut();
        if this.slots.is_looking() {
            // Nobody wakes a looking set's waiter: [`wait`] looks again.
            return match this.look() {
                Some(slot) => Poll::Ready(slot),
                None => Poll::Pending,
            };
        }
        let mut table = this.slots.held();
        let entry = table
            .connections
            .entry(this.connection.clone())
            .or_default();
        entry.limit = this.limit;
        if entry.busy < entry.limit {
            entry.busy += 1;
            return Poll::Ready(Slot {
                slots: this.slots,
                connection: this.connection.clone(),
                number: 0,
            });
        }
        entry.waiting.push(cx.waker().clone());
        Poll::Pending
    }
}

impl Drop for Take<'_> {
    /// A looking set's waiter that stops waiting leaves the queue, and a
    /// slot handed to it goes to the next.
    fn drop(&mut self) {
        let Some(number) = self.number else {
            return;
        };
        let mut table = self.slots.held();
        if let Some(entry) = table.connections.get_mut(&self.connection) {
            entry.queue.retain(|waiter| waiter.number != number);
            entry.hand();
        }
    }
}

/// A slot of `connection`'s for a call on `host`, at most `limit` out at
/// once: natively woken once one is given back; in a looking set, as on
/// Workers, looking again every [`LOOK_MS`] on the host's timer, in the
/// call's own invocation, until one has been handed to it (chunk 8.2j).
pub async fn wait<'h>(host: &'h dyn Host, connection: &str, limit: u32) -> Slot<'h> {
    let slots = host.slots();
    let mut take = slots.take(connection, limit);
    if !slots.is_looking() {
        return take.await;
    }
    loop {
        if let Some(slot) = take.look() {
            return slot;
        }
        host.sleep(LOOK_MS).await;
    }
}

/// `work`, run while its call holds `slot`: in a looking set, telling it
/// every [`BEAT_MS`] on the host's timer that the call's invocation still
/// holds it, so a slot an ended invocation never gave back is forgotten
/// rather than held for the isolate's life (chunk 8.2j).
pub async fn keeping<T>(
    host: &dyn Host,
    slot: Option<&Slot<'_>>,
    work: impl Future<Output = T>,
) -> T {
    let Some(slot) = slot.filter(|slot| slot.slots.is_looking()) else {
        return work.await;
    };
    let mut work = std::pin::pin!(work);
    let mut beat = Some(host.sleep(BEAT_MS));
    std::future::poll_fn(|cx| {
        if let Poll::Ready(done) = work.as_mut().poll(cx) {
            return Poll::Ready(done);
        }
        if let Some(timer) = beat.as_mut()
            && timer.as_mut().poll(cx).is_ready()
        {
            slot.beat();
            let mut next = host.sleep(BEAT_MS);
            // A timer that finishes at once can't pace the beats.
            beat = next.as_mut().poll(cx).is_pending().then_some(next);
        }
        Poll::Pending
    })
    .await
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

    thread_local! {
        /// The looking set's clock in these tests.
        static NOW: std::cell::Cell<u64> = const { std::cell::Cell::new(1_000) };
    }

    fn clock() -> Option<u64> {
        Some(NOW.with(std::cell::Cell::get))
    }

    fn at(now: u64) {
        NOW.with(|clock| clock.set(now));
    }

    /// On Workers a slot given back is handed to the call that has waited
    /// longest, waking no one: it finds the slot when it looks again, and a
    /// call that comes later waits behind it (chunk 8.2j).
    #[test]
    fn a_looking_set_hands_a_slot_given_back_to_the_longest_waiter_waking_no_one() {
        at(1_000);
        let slots = Slots::looking(clock);
        let woken = Arc::new(Count(AtomicUsize::new(0)));
        let waker = Waker::from(Arc::clone(&woken));
        let mut first = slots.take("fuel", 1);
        let Poll::Ready(held) = poll(&mut first, &waker) else {
            panic!("a free slot");
        };
        let mut second = slots.take("fuel", 1);
        let mut third = slots.take("fuel", 1);
        assert!(second.look().is_none());
        assert!(third.look().is_none());
        assert!(slots.full("fuel"));
        drop(held);
        assert_eq!(woken.0.load(Ordering::SeqCst), 0, "nobody is woken");
        assert!(slots.full("fuel"), "the slot is the second call's now");
        assert!(slots.try_take("fuel", 1).is_none());
        let mut later = slots.take("fuel", 1);
        assert!(later.look().is_none(), "a later call waits behind");
        assert!(third.look().is_none(), "the third waits behind the second");
        let second = second.look().expect("the slot handed to the second");
        assert_eq!(slots.busy("fuel"), 1);
        drop(second);
        let third = third.look().expect("then the third's");
        drop(third);
        assert!(later.look().is_some());
    }

    /// A waiter that stops waiting before it looks again hands the slot it
    /// was handed on to the next.
    #[test]
    fn a_waiter_that_goes_hands_its_slot_on() {
        at(1_000);
        let slots = Slots::looking(clock);
        let held = slots.try_take("fuel", 1).expect("a slot");
        let mut second = slots.take("fuel", 1);
        let mut third = slots.take("fuel", 1);
        assert!(second.look().is_none());
        assert!(third.look().is_none());
        drop(held);
        drop(second);
        assert!(third.look().is_some(), "handed on");
    }

    /// An invocation can end without giving back what it held, and
    /// without looking again for what it waited for: a holder not heard
    /// from for `GONE_MS` is forgotten, as is a waiter that hasn't looked
    /// for as long, and a holder that beats is kept.
    #[test]
    fn a_holder_or_a_waiter_not_heard_from_is_forgotten() {
        at(1_000);
        let slots = Slots::looking(clock);
        let gone = slots.try_take("fuel", 1).expect("a slot");
        std::mem::forget(gone);
        let mut waiting = slots.take("fuel", 1);
        assert!(waiting.look().is_none());
        at(1_000 + GONE_MS);
        assert!(waiting.look().is_none(), "not gone yet");
        at(1_001 + GONE_MS);
        let kept = waiting.look().expect("the gone holder's slot");
        // A holder that says it is still there keeps its slot.
        let mut next = slots.take("fuel", 1);
        assert!(next.look().is_none());
        at(2_000 + GONE_MS * 2);
        kept.beat();
        assert!(next.look().is_none(), "the holder beat");
        // A waiter handed a slot that never looks again is forgotten, and
        // its slot handed on.
        let mut after = slots.take("fuel", 1);
        assert!(after.look().is_none());
        drop(kept);
        std::mem::forget(next);
        at(2_001 + GONE_MS * 3);
        assert!(after.look().is_some(), "the gone waiter passed over");
    }

    /// A delivery reads a looking set as full while a call holds its one
    /// slot, and not once that holder is gone.
    #[test]
    fn a_looking_set_is_full_only_while_a_live_call_holds_the_slot() {
        at(1_000);
        let slots = Slots::looking(clock);
        std::mem::forget(slots.try_take("fuel", 1).expect("a slot"));
        assert!(slots.full("fuel"));
        at(1_001 + GONE_MS);
        assert!(!slots.full("fuel"));
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
