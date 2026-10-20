//! What runs a program's callables, behind one trait the test runner
//! calls (docs/compiler-runtime.md §2.5 and §2.7): the interpreter is its
//! first implementation and compiled code its second (Phase 8), so the
//! runner hands both the same context and reads the same [`Outcome`].
//!
//! [`block_on`] drives one invocation's future on the thread that made it,
//! with no reactor: for one that waits on nothing outside it, such as a
//! function, or an action on a host with no store. One that waits on a
//! store's I/O runs inside the native host's current-thread runtime
//! instead (§2.6, chunk 3.1), which drives it.

use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Context, Wake, Waker};

use jadpo_value::Value;

use crate::{Ctx, LocalFuture, Outcome, Places};

/// Runs a program's callables.
pub trait Executor {
    /// Where every frame is: the callables' numbers, labels, files and
    /// sites.
    fn places(&self) -> &Places;

    /// The number of the callable with identity key `key`.
    fn callable(&self, key: &str) -> Option<u32>;

    /// The values callable `callable` gives some settings itself, by their
    /// numbers: a test's `config:` (area 13). None for anything else.
    fn settings(&self, callable: u32) -> Vec<(u32, Value)> {
        let _ = callable;
        Vec::new()
    }

    /// Where callable `callable`'s clock starts, in milliseconds since
    /// 1970-01-01T00:00:00Z: a test's `clock:` (area 13). None for anything
    /// else, which runs at its host's time or the test default.
    fn clock(&self, callable: u32) -> Option<i64> {
        let _ = callable;
        None
    }

    /// What the program's batches run with (area 12, chunk 6.4): each
    /// `batches.each` call's plan and the codecs its saved values are
    /// kept in. None for a program that starts none.
    fn batches(&self) -> Option<crate::batches::Batches<'_>> {
        None
    }

    /// The program's subscribers and the codecs its events' payloads are
    /// read with (area 12, chunk 6.1b): what the runner delivers an event
    /// with. `None` for a program that has none to deliver to.
    fn events(&self) -> Option<crate::outbox::Events<'_>> {
        None
    }

    /// The program's schedules, where their state is kept, and the stores
    /// the tick sweeps (area 12, chunk 6.2). `None` for a program the tick
    /// has nothing to do for.
    fn schedules(&self) -> Option<&jadpo_std::schedule::SchedulesPlan> {
        None
    }

    /// The purge of the entity called `entity` (area 5, "Lifecycle"), which
    /// its built-in schedule runs (chunk 6.2c), with how long it waits, in
    /// milliseconds: its `after:`, or the value the program started with
    /// for the setting it reads; `None` when that setting holds no
    /// duration. `None` for an entity that isn't purged.
    fn purge(&self, entity: &str) -> Option<(&jadpo_std::write::PurgePlan, Option<i64>)> {
        let _ = entity;
        None
    }

    /// The connections the program keeps files in (area 16; chunk 6.4b):
    /// what a test's host gives its store of files for, and no other
    /// connection (chunk 7.1c).
    fn file_stores(&self) -> &[jadpo_std::files::FilesConnection] {
        &[]
    }

    /// The program's connections to provider packages (area 11; chunk
    /// 7.2d): what a test's host gives a package's fake for.
    fn provider_connections(&self) -> Vec<&str> {
        Vec::new()
    }

    /// Reads the recording at `file` of one of the app's own integrations
    /// as its operation's output, through the package's own decoders run
    /// at `ctx` (area 11, "Provenance and confirmation"; chunk 7.2d2):
    /// what `jadpo test` runs for each. `None` when the program holds no
    /// such recording.
    fn recording(
        &self,
        ctx: &mut Ctx<'_>,
        file: &str,
    ) -> Option<Result<(), crate::recorded::Unread>> {
        let _ = (ctx, file);
        None
    }

    /// Runs callable `callable` with `args`, one per parameter, against
    /// `ctx`, whose stack is empty: an action or fixture inside the
    /// runtime's action boundary, anything else as it is.
    fn run<'a, 'p: 'a>(
        &'a self,
        ctx: &'a mut Ctx<'p>,
        callable: u32,
        args: Vec<Value>,
    ) -> LocalFuture<'a, Outcome>;

    /// A file helper's write's row for `item` (area 12, "Batches"; chunk
    /// 6.4b2): the inline function `row` names, in the body of the action
    /// that started the run, its parameter given `item` and each local of
    /// the action it reads given the value at its place in `saved`, the
    /// run's saved values. Pure, so it never waits; a fault is the item's
    /// problem. An executor that runs no writes gives a defect.
    fn row<'a, 'p: 'a>(
        &'a self,
        ctx: &'a mut Ctx<'p>,
        row: &'a jadpo_std::batch::BatchRow,
        item: Value,
        saved: &'a [Value],
    ) -> LocalFuture<'a, Outcome> {
        let _ = (row, item, saved);
        Box::pin(async move {
            Outcome::Fault(ctx.fault(
                crate::FaultKind::Defect("a row written by an executor that runs no writes".into()),
                crate::Frame::ENTRY,
            ))
        })
    }
}

/// Gives the thread back once: pending the first time it is polled, after
/// asking to be polled again, so whatever else the thread runs (other
/// requests a worker answers, the route's own deadline) has a turn. What
/// an executor awaits each time it checks a request's deadline while
/// running work that never waits (area 7, "Time limits"; chunk 4.10).
pub fn yield_now() -> YieldNow {
    YieldNow(false)
}

/// The future [`yield_now`] gives.
#[derive(Debug)]
pub struct YieldNow(bool);

impl Future for YieldNow {
    type Output = ();

    fn poll(mut self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> std::task::Poll<()> {
        if self.0 {
            return std::task::Poll::Ready(());
        }
        self.0 = true;
        cx.waker().wake_by_ref();
        std::task::Poll::Pending
    }
}

/// A future [`block_on`] gave up on: it waited for something nothing will
/// wake it for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stalled;

/// Notes that the future asked to be polled again.
struct Flag(AtomicBool);

impl Wake for Flag {
    fn wake(self: Arc<Self>) {
        self.0.store(true, Ordering::SeqCst);
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.0.store(true, Ordering::SeqCst);
    }
}

/// Drives `future` to its end on this thread: polled again whenever it
/// wakes itself, and [`Stalled`] when it waits with nothing to wake it.
pub fn block_on<F: Future>(future: F) -> Result<F::Output, Stalled> {
    let flag = Arc::new(Flag(AtomicBool::new(false)));
    let waker = Waker::from(Arc::clone(&flag));
    let mut context = Context::from_waker(&waker);
    let mut future = std::pin::pin!(future);
    loop {
        let mut output = None;
        let _ = future
            .as_mut()
            .poll(&mut context)
            .map(|value| output = Some(value));
        if let Some(output) = output {
            return Ok(output);
        }
        if !flag.0.swap(false, Ordering::SeqCst) {
            return Err(Stalled);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::task::Poll;

    use super::*;

    /// Pending once, after waking itself.
    struct Yield(bool);

    impl Future for Yield {
        type Output = u8;

        fn poll(mut self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<u8> {
            if self.0 {
                return Poll::Ready(7);
            }
            self.0 = true;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }

    #[test]
    fn a_future_runs_to_its_end_or_stalls() {
        assert_eq!(block_on(async { 1 + 1 }), Ok(2));
        assert_eq!(block_on(Yield(false)), Ok(7));
        assert_eq!(block_on(std::future::pending::<()>()), Err(Stalled));
    }
}
