//! Work that runs on one thread until it ends or is told to stop, counted
//! so that whoever stops it can wait for the last of it: a host's
//! background work (chunk 6.7) and a server worker's connections (chunk
//! 8.1l).

use std::cell::{Cell, RefCell};
use std::future::Future;
use std::rc::Rc;
use std::task::{Poll, Waker};
use std::time::Duration;

use jadpo_runtime::LocalFuture;

/// The work running, and whether it has been told to stop.
pub(crate) struct Tracked {
    stop: tokio::sync::watch::Sender<bool>,
    /// What each piece's look at the stop is cloned from.
    stopping: tokio::sync::watch::Receiver<bool>,
    running: Cell<usize>,
    /// Who waits for the last of it to end.
    idle: RefCell<Vec<Waker>>,
}

impl Default for Tracked {
    fn default() -> Tracked {
        let (stop, stopping) = tokio::sync::watch::channel(false);
        Tracked {
            stop,
            stopping,
            running: Cell::new(0),
            idle: RefCell::new(Vec::new()),
        }
    }
}

impl std::fmt::Debug for Tracked {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tracked")
            .field("stopping", &*self.stop.borrow())
            .field("running", &self.running.get())
            .finish()
    }
}

/// One piece of work, counted until it ends or is dropped.
struct Counted(Rc<Tracked>);

impl Drop for Counted {
    fn drop(&mut self) {
        let left = self.0.running.get().saturating_sub(1);
        self.0.running.set(left);
        if left == 0 {
            for waiting in self.0.idle.borrow_mut().drain(..) {
                waiting.wake();
            }
        }
    }
}

impl Tracked {
    /// `work` counted until it ends or is dropped.
    pub(crate) fn track(
        self: &Rc<Tracked>,
        work: impl Future<Output = ()> + 'static,
    ) -> LocalFuture<'static, ()> {
        self.running.set(self.running.get() + 1);
        let counted = Counted(Rc::clone(self));
        Box::pin(async move {
            let _counted = counted;
            work.await;
        })
    }

    /// Whether it has been told to stop.
    pub(crate) fn stopped(&self) -> bool {
        *self.stop.borrow()
    }

    /// Finishes once it is told to stop.
    pub(crate) fn stopping(&self) -> LocalFuture<'static, ()> {
        let mut stop = self.stopping.clone();
        Box::pin(async move {
            let _ = stop.wait_for(|stopping| *stopping).await;
        })
    }

    /// Tells its work to stop, and waits at most `grace` for the last of
    /// it to end.
    pub(crate) fn stop(self: &Rc<Tracked>, grace: Duration) -> LocalFuture<'static, ()> {
        self.stop.send_replace(true);
        let tracked = Rc::clone(self);
        Box::pin(async move {
            let idle = std::future::poll_fn(|cx| {
                if tracked.running.get() == 0 {
                    Poll::Ready(())
                } else {
                    tracked.idle.borrow_mut().push(cx.waker().clone());
                    Poll::Pending
                }
            });
            let _ = tokio::time::timeout(grace, idle).await;
        })
    }
}

/// `work`'s output, or `None` when `stop` finishes first.
pub(crate) async fn unless<T>(
    work: impl Future<Output = T>,
    stop: impl Future<Output = ()>,
) -> Option<T> {
    let mut work = std::pin::pin!(work);
    let mut stop = std::pin::pin!(stop);
    std::future::poll_fn(|cx| {
        if let Poll::Ready(done) = work.as_mut().poll(cx) {
            return Poll::Ready(Some(done));
        }
        if stop.as_mut().poll(cx).is_ready() {
            return Poll::Ready(None);
        }
        Poll::Pending
    })
    .await
}
