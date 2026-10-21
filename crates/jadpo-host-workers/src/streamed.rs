//! A route's streamed answer on Workers (chunk 8.2h; area 7, "Streaming a
//! model's answer"; docs/compiler-runtime.md §2.6, "Serving on Workers"):
//! the runtime's `EventStream` sent as the body of Workers' `Response`, a
//! `ReadableStream` read as the client takes it, rather than read whole
//! first.
//!
//! Each time Workers pulls on the stream, the next event is read on the
//! host the request was served on (held to the call's `timeout:` and the
//! route's deadline on that host's clock, as natively) and handed over as
//! its bytes; after the last, the stream closes. A client that goes away
//! ends the request, and Workers ends the model's `fetch()` with it
//! (workerd once it next writes to the client). Where the platform
//! cancels the stream instead, the read in hand is dropped and with it
//! the model's answer, whose `fetch()` is aborted, as natively when hyper
//! drops the body.

use std::cell::{Cell, RefCell};
use std::future::Future;
use std::rc::Rc;
use std::task::{Poll, Waker};

use jadpo_runtime::stream::EventStream;
use js_sys::{Object, Promise, Reflect, Uint8Array};
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{ReadableStream, ReadableStreamDefaultController};

/// The underlying source's `pull` and `cancel`, kept until the stream ends.
type Callbacks = (
    Closure<dyn FnMut(JsValue) -> Promise>,
    Closure<dyn FnMut(JsValue)>,
);

/// `stream` as a body Workers sends piece by piece.
pub(crate) fn body(stream: EventStream) -> Result<ReadableStream, JsValue> {
    let source = Rc::new(Source {
        stream: RefCell::new(Some(stream)),
        cancelled: Cell::new(false),
        waker: RefCell::new(None),
        callbacks: RefCell::new(None),
    });
    let pull = {
        let source = Rc::clone(&source);
        Closure::<dyn FnMut(JsValue) -> Promise>::new(move |controller: JsValue| {
            let source = Rc::clone(&source);
            wasm_bindgen_futures::future_to_promise(async move {
                source.pull(controller.unchecked_into()).await;
                Ok(JsValue::UNDEFINED)
            })
        })
    };
    let cancel = {
        let source = Rc::clone(&source);
        Closure::<dyn FnMut(JsValue)>::new(move |_reason: JsValue| source.cancel())
    };
    let underlying = Object::new();
    let made = Reflect::set(&underlying, &JsValue::from_str("pull"), pull.as_ref())
        .and_then(|_| Reflect::set(&underlying, &JsValue::from_str("cancel"), cancel.as_ref()))
        .and_then(|_| ReadableStream::new_with_underlying_source(&underlying));
    *source.callbacks.borrow_mut() = Some((pull, cancel));
    if made.is_err() {
        source.finish();
    }
    made
}

/// What a stream's pulls read from: the runtime's stream until it ends or
/// is cancelled, and the callbacks Workers calls, which hold it, so both
/// are let go together once it is done.
struct Source {
    stream: RefCell<Option<EventStream>>,
    /// Whether the client went away.
    cancelled: Cell<bool>,
    /// The pull waiting on the next event, woken when the client goes away.
    waker: RefCell<Option<Waker>>,
    callbacks: RefCell<Option<Callbacks>>,
}

impl Source {
    /// Hands `controller` the next event, or closes the stream after the
    /// last; a stream cancelled meanwhile is let go.
    async fn pull(self: Rc<Self>, controller: ReadableStreamDefaultController) {
        let Some(stream) = self.stream.borrow().clone() else {
            return;
        };
        let next = unless_cancelled(&self, stream.next_written()).await;
        // No pull waits now: a client going away from here lets it go at
        // once.
        drop(self.waker.borrow_mut().take());
        drop(stream);
        match next {
            Some(Some(event)) => {
                if controller
                    .enqueue_with_chunk(&Uint8Array::from(event.as_slice()))
                    .is_err()
                {
                    self.finish();
                }
            }
            Some(None) => {
                let _ = controller.close();
                self.finish();
            }
            None => self.finish(),
        }
    }

    /// The client went away: the read in hand stops, and the stream is let
    /// go.
    fn cancel(&self) {
        self.cancelled.set(true);
        let waker = self.waker.borrow_mut().take();
        match waker {
            Some(waker) => waker.wake(),
            None => self.finish(),
        }
    }

    /// Lets go of the stream, and of the callbacks once the call in hand
    /// has returned.
    fn finish(&self) {
        drop(self.stream.borrow_mut().take());
        if let Some(callbacks) = self.callbacks.borrow_mut().take() {
            wasm_bindgen_futures::spawn_local(async move { drop(callbacks) });
        }
    }
}

/// What `next` gives, unless `source` is cancelled first: then `None`, and
/// `next` is dropped unfinished.
async fn unless_cancelled<T>(source: &Source, next: impl Future<Output = T>) -> Option<T> {
    let mut next = std::pin::pin!(next);
    std::future::poll_fn(|cx| {
        if source.cancelled.get() {
            return Poll::Ready(None);
        }
        if let Poll::Ready(value) = next.as_mut().poll(cx) {
            return Poll::Ready(Some(value));
        }
        *source.waker.borrow_mut() = Some(cx.waker().clone());
        Poll::Pending
    })
    .await
}
