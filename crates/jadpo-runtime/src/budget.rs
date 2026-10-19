//! What one invocation of background work may spend (chunk 6.7, from the
//! Phase 6 architecture review).
//!
//! Each invocation that runs background work (the work a response left,
//! the minute tick, a run carried on) is given a budget by its host
//! ([`Host::budget_ms`]): Workers ends a `waitUntil` 30 seconds after the
//! response and a cron after 15 minutes, and a native server is stopped
//! when it is deployed again. Within it the runner takes one piece of work
//! at a time while enough is left ([`Budget::spent`]), runs each piece
//! until its limit, the 15-minute limit of a run or what is left of the
//! budget, whichever is sooner, or until the host shuts down
//! ([`Budget::cut`], [`Budget::run`]), and gives back what it holds before
//! the budget ends, so every lease it takes lasts what is left of the
//! budget and a margin ([`crate::lease`]).

use std::cell::{Cell, RefCell};
use std::future::Future;
use std::task::{Context, Waker};

use crate::host::{Host, LocalFuture};
use crate::race::{First, race};

/// The longest one run of a subscriber or a schedule takes before it is
/// stopped as timed out (area 12, "Limits and runtime faults"), and the
/// longest budget any invocation is given.
pub const RUN_LIMIT_MS: u64 = 15 * 60 * 1000;

/// What a host gives each invocation unless it says otherwise: the whole
/// run limit. What the native host and `jadpo dev` give.
pub const GENEROUS_MS: u64 = RUN_LIMIT_MS;

/// How long before its budget ends the runner stops at most: giving back
/// what it holds takes a statement or two. A budget keeps back a tenth of
/// itself, up to this.
pub const RESERVE_MS: u64 = 5_000;

/// The kind of invocation background work runs in, which its host gives
/// a budget for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Invocation {
    /// The work a response left, run once it has been handed back
    /// ([`Host::defer`]): on Workers a `waitUntil`, 30 seconds.
    AfterResponse,
    /// The minute tick: on Workers the cron trigger, 15 minutes.
    Tick,
    /// Work an invocation whose budget was spent handed on, run as a
    /// fresh invocation ([`Host::hand_on`]).
    HandedOn,
}

/// Why a piece of work was stopped before it ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cut {
    /// Its limit passed: the 15-minute limit of a run, or what was left of
    /// the invocation's budget. A run that ran out of time.
    OutOfTime,
    /// The host is shutting down: what it held is given back as it was.
    ShuttingDown,
}

/// One invocation's budget: how long it has, measured from when it began
/// on the host's monotonic clock, and whether its host is shutting down.
pub struct Budget<'h> {
    host: &'h dyn Host,
    total: u64,
    /// When it began on the host's monotonic clock; `None` on a host with
    /// none, whose budget never runs down.
    began: Option<u64>,
    /// The host's shutdown, looked at without waiting.
    stopping: RefCell<LocalFuture<'h, ()>>,
    stopped: Cell<bool>,
}

impl std::fmt::Debug for Budget<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Budget")
            .field("total", &self.total)
            .field("left", &self.left_ms())
            .field("stopped", &self.stopped.get())
            .finish()
    }
}

impl<'h> Budget<'h> {
    /// The budget `host` gives an invocation of `invocation`, from now.
    pub fn start(host: &'h dyn Host, invocation: Invocation) -> Budget<'h> {
        Budget::of(host, host.budget_ms(invocation))
    }

    /// A budget of `total_ms` from now, at most [`RUN_LIMIT_MS`]: what
    /// a request waiting for its follow-on work has, its deadline (area 7,
    /// `settle:`).
    pub fn of(host: &'h dyn Host, total_ms: u64) -> Budget<'h> {
        Budget {
            host,
            total: total_ms.min(RUN_LIMIT_MS),
            began: host.monotonic_ms(),
            stopping: RefCell::new(host.stopping()),
            stopped: Cell::new(false),
        }
    }

    /// The host it runs on.
    pub fn host(&self) -> &'h dyn Host {
        self.host
    }

    /// What is left of it, in milliseconds: all of it on a host with no
    /// monotonic clock.
    pub fn left_ms(&self) -> u64 {
        match (self.began, self.host.monotonic_ms()) {
            (Some(began), Some(now)) => self.total.saturating_sub(now.saturating_sub(began)),
            _ => self.total,
        }
    }

    /// How long before its end the runner stops: a tenth of it, at most
    /// [`RESERVE_MS`].
    fn reserve_ms(&self) -> u64 {
        (self.total / 10).min(RESERVE_MS)
    }

    /// Whether the host is shutting down.
    pub fn stopping(&self) -> bool {
        if self.stopped.get() {
            return true;
        }
        let mut cx = Context::from_waker(Waker::noop());
        let stopped = self.stopping.borrow_mut().as_mut().poll(&mut cx).is_ready();
        self.stopped.set(stopped);
        stopped
    }

    /// Whether the runner takes no more work in this invocation: the host
    /// is shutting down, or no more is left than the time kept back for
    /// giving back what it holds.
    pub fn spent(&self) -> bool {
        self.stopping() || self.left_ms() <= self.reserve_ms()
    }

    /// How long a piece starting now may run: what is left before the time
    /// kept back, at most the run limit.
    pub fn limit_ms(&self) -> u64 {
        self.left_ms()
            .saturating_sub(self.reserve_ms())
            .min(RUN_LIMIT_MS)
    }

    /// A future that finishes when a piece starting now must stop, saying
    /// why: the run limit or what is left of the budget passes, or the
    /// host begins to shut down.
    pub fn cut(&self) -> LocalFuture<'h, Cut> {
        // Each timer counts from now, not from when the future is first
        // waited on.
        let time = race(
            self.host.sleep(RUN_LIMIT_MS),
            self.host.sleep(self.limit_ms()),
        );
        let stopping = self.host.stopping();
        Box::pin(async move {
            match race(stopping, time).await {
                First::Left(()) => Cut::ShuttingDown,
                First::Right(_) => Cut::OutOfTime,
            }
        })
    }

    /// Runs `work` until it ends or [`Budget::cut`] stops it, dropping it
    /// unfinished then: the one way background work that isn't an action
    /// runs within a budget (a schedule's purge, a batch run's page).
    pub async fn run<T>(&self, work: impl Future<Output = T>) -> Result<T, Cut> {
        match race(work, self.cut()).await {
            First::Left(done) => Ok(done),
            First::Right(cut) => Err(cut),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MemoryHost, block_on};

    /// A host whose monotonic clock the test moves, and which shuts down
    /// when it says so; its timers finish once the clock reaches them.
    struct Paced {
        now: Cell<u64>,
        budget: u64,
        stopping: Cell<bool>,
    }

    impl Host for Paced {
        fn database(&self, _store: &str) -> Option<&dyn crate::Database> {
            None
        }

        fn random(&self, _bytes: &mut [u8]) -> bool {
            false
        }

        fn monotonic_ms(&self) -> Option<u64> {
            Some(self.now.get())
        }

        fn budget_ms(&self, _invocation: Invocation) -> u64 {
            self.budget
        }

        fn sleep(&self, millis: u64) -> LocalFuture<'_, ()> {
            let until = self.now.get().saturating_add(millis);
            Box::pin(std::future::poll_fn(move |_| {
                if self.now.get() >= until {
                    std::task::Poll::Ready(())
                } else {
                    std::task::Poll::Pending
                }
            }))
        }

        fn stopping(&self) -> LocalFuture<'_, ()> {
            Box::pin(std::future::poll_fn(move |_| {
                if self.stopping.get() {
                    std::task::Poll::Ready(())
                } else {
                    std::task::Poll::Pending
                }
            }))
        }
    }

    #[test]
    fn a_budget_runs_down_on_the_monotonic_clock_and_keeps_a_tenth_back() {
        let host = Paced {
            now: Cell::new(1_000),
            budget: 30_000,
            stopping: Cell::new(false),
        };
        let budget = Budget::start(&host, Invocation::AfterResponse);
        assert_eq!(budget.left_ms(), 30_000);
        assert_eq!(budget.limit_ms(), 27_000, "3 s kept back");
        host.now.set(20_000);
        assert_eq!(budget.left_ms(), 11_000);
        assert!(!budget.spent());
        host.now.set(28_000);
        assert!(budget.spent(), "no more than the 3 s kept back is left");
        // A long budget keeps back 5 s, and none is longer than a run.
        let host = Paced {
            now: Cell::new(0),
            budget: 60 * 60 * 1000,
            stopping: Cell::new(false),
        };
        let budget = Budget::start(&host, Invocation::Tick);
        assert_eq!(budget.left_ms(), RUN_LIMIT_MS);
        assert_eq!(budget.limit_ms(), RUN_LIMIT_MS - RESERVE_MS);
    }

    #[test]
    fn a_host_shutting_down_spends_the_budget_and_cuts_what_runs() {
        let host = Paced {
            now: Cell::new(0),
            budget: 30_000,
            stopping: Cell::new(false),
        };
        let budget = Budget::start(&host, Invocation::AfterResponse);
        let mut cut = budget.cut();
        let mut cx = Context::from_waker(Waker::noop());
        assert!(cut.as_mut().poll(&mut cx).is_pending());
        host.stopping.set(true);
        assert!(budget.spent());
        assert_eq!(
            cut.as_mut().poll(&mut cx),
            std::task::Poll::Ready(Cut::ShuttingDown)
        );
        // Out of time when what is left before the reserve passes.
        let host = Paced {
            now: Cell::new(0),
            budget: 30_000,
            stopping: Cell::new(false),
        };
        let budget = Budget::start(&host, Invocation::AfterResponse);
        let mut cut = budget.cut();
        host.now.set(26_999);
        assert!(cut.as_mut().poll(&mut cx).is_pending());
        host.now.set(27_000);
        assert_eq!(
            cut.as_mut().poll(&mut cx),
            std::task::Poll::Ready(Cut::OutOfTime)
        );
    }

    #[test]
    fn a_host_with_no_clock_never_runs_its_budget_down() {
        let host = MemoryHost::new();
        let budget = Budget::start(&host, Invocation::AfterResponse);
        assert_eq!(budget.left_ms(), GENEROUS_MS);
        assert!(!budget.spent());
        let ran = block_on(budget.run(async { 7 })).expect("nothing waits");
        assert_eq!(ran, Ok(7));
    }
}
