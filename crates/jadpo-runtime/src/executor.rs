//! What runs a program's callables, behind one trait the test runner
//! calls (docs/compiler-runtime.md §2.5 and §2.7): the interpreter is its
//! first implementation and compiled code its second (Phase 8), so the
//! runner hands both the same context and reads the same [`Outcome`].
//!
//! [`block_on`] drives one invocation's future on the thread that made it.
//! Nothing the runtime awaits yet waits on I/O, so it needs no reactor;
//! the native host's current-thread runtime per core (§2.6) takes over
//! when the store does (chunk 3.1).

use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Context, Poll, Wake, Waker};

use jadpo_value::Value;

use crate::{Ctx, LocalFuture, Outcome, Places};

/// Runs a program's callables.
pub trait Executor {
    /// Where every frame is: the callables' numbers, labels, files and
    /// sites.
    fn places(&self) -> &Places;

    /// The number of the callable with identity key `key`.
    fn callable(&self, key: &str) -> Option<u32>;

    /// Runs callable `callable` with `args`, one per parameter, against
    /// `ctx`, whose stack is empty: an action or fixture inside the
    /// runtime's action boundary, anything else as it is.
    fn run<'a, 'p: 'a>(
        &'a self,
        ctx: &'a mut Ctx<'p>,
        callable: u32,
        args: Vec<Value>,
    ) -> LocalFuture<'a, Outcome>;
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
        if let Poll::Ready(output) = future.as_mut().poll(&mut context) {
            return Ok(output);
        }
        if !flag.0.swap(false, Ordering::SeqCst) {
            return Err(Stalled);
        }
    }
}

#[cfg(test)]
mod tests {
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
