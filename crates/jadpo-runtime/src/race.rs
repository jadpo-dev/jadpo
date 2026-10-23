//! Waiting for whichever of two futures finishes first, on the thread that
//! made them: what a route's deadline and the readiness check need from a
//! timer, without an executor of the runtime's own. Also drains a group
//! of local futures completely, as schedules and bounded delivery sweeps need.

use std::future::Future;
use std::task::Poll;

/// Which future finished first, with what it gave.
#[derive(Debug)]
pub(crate) enum First<A, B> {
    Left(A),
    Right(B),
}

/// Runs `left` and `right` together until one finishes, `left` polled
/// first; the other is dropped unfinished.
pub(crate) async fn race<A: Future, B: Future>(left: A, right: B) -> First<A::Output, B::Output> {
    let mut left = std::pin::pin!(left);
    let mut right = std::pin::pin!(right);
    std::future::poll_fn(|cx| {
        if let Poll::Ready(value) = left.as_mut().poll(cx) {
            return Poll::Ready(First::Left(value));
        }
        if let Poll::Ready(value) = right.as_mut().poll(cx) {
            return Poll::Ready(First::Right(value));
        }
        Poll::Pending
    })
    .await
}

/// Runs `futures` together on this thread until every one has finished,
/// giving their outputs in order: each due schedule's run goes on beside
/// the others, so a long one holds none of them up.
pub(crate) async fn join_all<F: Future>(futures: Vec<F>) -> Vec<F::Output> {
    let mut pending: Vec<std::pin::Pin<Box<F>>> = futures.into_iter().map(Box::pin).collect();
    let mut done: Vec<Option<F::Output>> = pending.iter().map(|_| None).collect();
    std::future::poll_fn(|cx| {
        let mut all = true;
        for (future, out) in pending.iter_mut().zip(done.iter_mut()) {
            if out.is_some() {
                continue;
            }
            match future.as_mut().poll(cx) {
                Poll::Ready(value) => *out = Some(value),
                Poll::Pending => all = false,
            }
        }
        if all { Poll::Ready(()) } else { Poll::Pending }
    })
    .await;
    done.into_iter().flatten().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block_on;

    #[test]
    fn the_first_to_finish_wins() {
        let won = block_on(race(async { 1 }, std::future::pending::<()>())).unwrap();
        assert!(matches!(won, First::Left(1)));
        let won = block_on(race(std::future::pending::<()>(), async { 2 })).unwrap();
        assert!(matches!(won, First::Right(2)));
    }
}
