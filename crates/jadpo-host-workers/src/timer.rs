//! A Worker's timer: a promise `setTimeout` settles, its timeout cleared
//! when the wait is dropped first, so nothing is left to fire for work no
//! one waits for, and the promise settled then instead (chunk 8.4e): a
//! promise that never settles keeps what waits on it (wasm-bindgen's
//! callbacks, which hold each other until one is called) for the isolate's
//! life, and every request drops several waits that its work outran.

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use js_sys::{Function, Promise};
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::JsFuture;

use crate::js;

/// Finishes `millis` milliseconds after it was made, or at once when no
/// timer could be set.
pub(crate) struct Sleep {
    waiting: Option<JsFuture>,
    timeout: Option<JsValue>,
    /// The promise's `resolve`, which a wait dropped first calls.
    resolve: Option<Function>,
}

impl std::fmt::Debug for Sleep {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Sleep")
    }
}

/// A wait of `millis` milliseconds on `setTimeout`.
pub(crate) fn sleep(millis: u64) -> Sleep {
    let mut timeout = None;
    let mut resolves = None;
    // Up to 2^53 milliseconds is exact as a JavaScript number.
    let millis = millis.min(1 << 53) as f64;
    let promise = Promise::new(&mut |resolve, _reject| {
        timeout = js::set_timeout(&resolve, millis).ok();
        resolves = Some(resolve);
    });
    let waiting = timeout.is_some().then(|| JsFuture::from(promise));
    Sleep {
        waiting,
        timeout,
        resolve: resolves,
    }
}

impl Future for Sleep {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let Some(waiting) = self.waiting.as_mut() else {
            return Poll::Ready(());
        };
        match Pin::new(waiting).poll(cx) {
            Poll::Ready(_) => {
                self.waiting = None;
                self.timeout = None;
                self.resolve = None;
                Poll::Ready(())
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

impl Drop for Sleep {
    /// Clears a timeout that hasn't fired, and settles its promise, which
    /// the timer no longer will.
    fn drop(&mut self) {
        if let Some(timeout) = self.timeout.take() {
            let _ = js::clear_timeout(&timeout);
            if let Some(resolve) = self.resolve.take() {
                let _ = resolve.call0(&JsValue::UNDEFINED);
            }
        }
    }
}

/// `work`'s output, or `None` when `millis` passed first on a Worker's
/// timer.
pub(crate) async fn within<T>(millis: u64, work: impl Future<Output = T>) -> Option<T> {
    let mut work = std::pin::pin!(work);
    let mut timer = sleep(millis);
    std::future::poll_fn(|cx| {
        if let Poll::Ready(done) = work.as_mut().poll(cx) {
            return Poll::Ready(Some(done));
        }
        if Pin::new(&mut timer).poll(cx).is_ready() {
            return Poll::Ready(None);
        }
        Poll::Pending
    })
    .await
}
