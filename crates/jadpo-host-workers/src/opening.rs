//! The connections one invocation is opening (chunk 8.2j;
//! docs/compiler-runtime.md §2.6, "Six connections an invocation"):
//! Workers lets an invocation have six connections waiting for their
//! answer's head at once, a `fetch()` until its status and headers come
//! and a `connect()` until its socket opens, and holds a seventh back
//! until one of them gets there. The runtime counts them itself, across
//! the transport, `fetch()` and the runtime's own fetches, for everything
//! the invocation does, its `waitUntil` included: a seventh waits here,
//! in the order asked, and its own timeout starts only once it goes out,
//! so a call never runs out of time behind the invocation's others nor
//! times out at the platform not knowing whether it left.
//!
//! An invocation's work all runs in its own context, so a place given back
//! wakes the next waiter directly; [`jadpo_runtime::slots`] is what is
//! shared across invocations, and wakes nobody.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll, Waker};

/// How many connections a Workers invocation may have waiting for their
/// answer's head at once.
pub const MOST: usize = 6;

/// An invocation's places for connections being opened: [`MOST`] of them.
#[derive(Debug, Default)]
pub struct Opening {
    state: RefCell<State>,
}

#[derive(Debug, Default)]
struct State {
    /// Places taken, or handed to a waiter that hasn't taken it yet.
    taken: usize,
    /// The connections waiting for a place, longest first.
    queue: VecDeque<Ticket>,
    /// The number the next waiter is known by.
    next: u64,
}

#[derive(Debug)]
struct Ticket {
    number: u64,
    waker: Option<Waker>,
    handed: bool,
}

impl State {
    /// Hands each free place to the longest waiter without one, waking it.
    fn hand(&mut self) {
        for ticket in &mut self.queue {
            if self.taken >= MOST {
                break;
            }
            if !ticket.handed {
                ticket.handed = true;
                self.taken += 1;
                if let Some(waker) = ticket.waker.take() {
                    waker.wake();
                }
            }
        }
    }
}

impl Opening {
    /// A place for a connection about to be opened: at once while fewer
    /// than [`MOST`] are being opened and none waits, otherwise once one
    /// of them has been answered, in the order asked.
    pub fn place(&self) -> Placing<'_> {
        Placing {
            opening: self,
            number: None,
        }
    }

    /// How many connections hold a place now.
    pub fn taken(&self) -> usize {
        self.state.borrow().taken
    }

    /// How many connections wait for a place now.
    pub fn waiting(&self) -> usize {
        self.state
            .borrow()
            .queue
            .iter()
            .filter(|ticket| !ticket.handed)
            .count()
    }
}

/// A connection's place among its invocation's [`MOST`], given back when
/// it is dropped: once its answer's head has come, or it failed.
#[derive(Debug)]
pub struct Place<'a> {
    opening: &'a Opening,
}

impl Drop for Place<'_> {
    fn drop(&mut self) {
        let mut state = self.opening.state.borrow_mut();
        state.taken = state.taken.saturating_sub(1);
        state.hand();
    }
}

/// Waiting for a place ([`Opening::place`]).
#[derive(Debug)]
pub struct Placing<'a> {
    opening: &'a Opening,
    /// Its ticket, once it waits.
    number: Option<u64>,
}

impl<'a> Future for Placing<'a> {
    type Output = Place<'a>;

    /// Asking hands on any free place first (chunk 8.4b): a place given
    /// back while a waiter handed one before hadn't yet taken it found
    /// nobody to hand it to, and would otherwise sit free while this one
    /// waits behind it.
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Place<'a>> {
        let this = self.get_mut();
        let mut state = this.opening.state.borrow_mut();
        let number = match this.number {
            Some(number) => number,
            None => {
                if state.taken < MOST && state.queue.is_empty() {
                    state.taken += 1;
                    return Poll::Ready(Place {
                        opening: this.opening,
                    });
                }
                let number = state.next;
                state.next += 1;
                state.queue.push_back(Ticket {
                    number,
                    waker: Some(cx.waker().clone()),
                    handed: false,
                });
                this.number = Some(number);
                number
            }
        };
        state.hand();
        let Some(at) = state
            .queue
            .iter()
            .position(|ticket| ticket.number == number)
        else {
            // Never so: its ticket leaves the queue only here and when it
            // is dropped. Waiting again keeps the count whole.
            this.number = None;
            drop(state);
            cx.waker().wake_by_ref();
            return Poll::Pending;
        };
        if state.queue.get(at).is_some_and(|ticket| ticket.handed) {
            state.queue.remove(at);
            this.number = None;
            return Poll::Ready(Place {
                opening: this.opening,
            });
        }
        if let Some(ticket) = state.queue.get_mut(at) {
            ticket.waker = Some(cx.waker().clone());
        }
        Poll::Pending
    }
}

impl Drop for Placing<'_> {
    /// A connection that stops waiting, as when its call's deadline passes,
    /// leaves the queue, and a place handed to it goes to the next.
    fn drop(&mut self) {
        let Some(number) = self.number else {
            return;
        };
        let mut state = self.opening.state.borrow_mut();
        if let Some(at) = state
            .queue
            .iter()
            .position(|ticket| ticket.number == number)
            && let Some(ticket) = state.queue.remove(at)
            && ticket.handed
        {
            state.taken = state.taken.saturating_sub(1);
        }
        state.hand();
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

    /// Six connections are opened at once; the seventh and eighth wait,
    /// and each goes, in the order asked, as one of the six is answered.
    #[test]
    fn a_seventh_waits_for_one_of_six_to_be_answered() {
        let opening = Opening::default();
        let woken = Arc::new(Count(AtomicUsize::new(0)));
        let waker = Waker::from(Arc::clone(&woken));
        let mut six: Vec<Place<'_>> = (0..MOST)
            .map(|_| match poll(&mut opening.place(), &waker) {
                Poll::Ready(place) => place,
                Poll::Pending => panic!("a free place"),
            })
            .collect();
        let mut seventh = opening.place();
        let mut eighth = opening.place();
        assert!(poll(&mut seventh, &waker).is_pending());
        assert!(poll(&mut eighth, &waker).is_pending());
        assert_eq!((opening.taken(), opening.waiting()), (6, 2));
        six.pop();
        assert_eq!(woken.0.load(Ordering::SeqCst), 1, "the seventh woken");
        assert!(
            poll(&mut eighth, &waker).is_pending(),
            "the eighth waits on"
        );
        let Poll::Ready(seventh) = poll(&mut seventh, &waker) else {
            panic!("the place handed to the seventh");
        };
        // A later connection waits behind the eighth.
        let mut ninth = opening.place();
        assert!(poll(&mut ninth, &waker).is_pending());
        drop(seventh);
        assert!(poll(&mut ninth, &waker).is_pending());
        assert!(poll(&mut eighth, &waker).is_ready());
        assert_eq!(opening.taken(), 6);
    }

    /// A place free when a connection asks for one is handed to it, even
    /// while a waiter handed a place before it hasn't taken it yet (chunk
    /// 8.4b; 8.2j's reproduction): the eighth never sits behind a free
    /// place.
    #[test]
    fn a_free_place_is_handed_on_when_a_place_is_asked_for() {
        let opening = Opening::default();
        let woken = Arc::new(Count(AtomicUsize::new(0)));
        let waker = Waker::from(Arc::clone(&woken));
        let mut six: Vec<Place<'_>> = (0..MOST)
            .map(|_| match poll(&mut opening.place(), &waker) {
                Poll::Ready(place) => place,
                Poll::Pending => panic!("a free place"),
            })
            .collect();
        let mut seventh = opening.place();
        assert!(poll(&mut seventh, &waker).is_pending());
        // The first place given back is handed to the seventh; the second
        // is free, nobody waiting without one.
        six.pop();
        six.pop();
        let mut eighth = opening.place();
        let eighth_place = match poll(&mut eighth, &waker) {
            Poll::Ready(place) => Some(place),
            Poll::Pending => None,
        };
        let Poll::Ready(_seventh) = poll(&mut seventh, &waker) else {
            panic!("the place handed to the seventh");
        };
        let eighth_place = eighth_place.or_else(|| match poll(&mut eighth, &waker) {
            Poll::Ready(place) => Some(place),
            Poll::Pending => None,
        });
        assert!(eighth_place.is_some(), "the eighth waits behind a free place");
        assert_eq!((opening.taken(), opening.waiting()), (MOST, 0));
    }

    /// A connection that stops waiting hands the place it was given on.
    #[test]
    fn a_waiter_that_goes_hands_its_place_on() {
        let opening = Opening::default();
        let waker = Waker::from(Arc::new(Count(AtomicUsize::new(0))));
        let mut six: Vec<Place<'_>> = (0..MOST)
            .filter_map(|_| match poll(&mut opening.place(), &waker) {
                Poll::Ready(place) => Some(place),
                Poll::Pending => None,
            })
            .collect();
        let mut seventh = opening.place();
        let mut eighth = opening.place();
        assert!(poll(&mut seventh, &waker).is_pending());
        assert!(poll(&mut eighth, &waker).is_pending());
        six.pop();
        drop(seventh);
        assert!(poll(&mut eighth, &waker).is_ready());
        drop(six);
        assert_eq!((opening.taken(), opening.waiting()), (0, 0));
    }
}
