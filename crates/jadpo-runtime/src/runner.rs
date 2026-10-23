//! The runner (docs/compiler-runtime.md §2.6, chunk 6.1a): the one place
//! the runtime drives an executor for work no request is waiting on, so
//! the interpreter now and compiled code later run it the same way.
//!
//! It runs work three ways:
//!
//! - **Deferred**, after the response that set it off is sent: what an
//!   action left once it committed ([`Ctx::defer`]) goes to the host with
//!   [`after_response`], which runs it through [`Host::defer`] (Workers'
//!   `waitUntil`, natively a task on the worker's thread), each piece in a
//!   context of its own under the host's clock.
//! - **While a route waits**, with [`settle_within`]: what a route that
//!   waits for its follow-on work runs before it answers (area 7,
//!   `settle:`; chunk 6.3).
//! - **Per item**, with [`items`]: a callable run once for each of many
//!   items, in order, each one undone alone when it fails inside a batch
//!   ([`Batch`], a transaction of its own committing the items' writes and
//!   the run's position together, so a run resumes after the last batch
//!   that committed). A backfill worked out in Jadpo runs its rows this
//!   way ([`InJadpo`]), and so do batch runs (area 12, chunk 6.4).
//!
//! Each invocation runs within the budget its host gives it
//! ([`crate::budget`]; chunk 6.7): it takes one piece at a time while
//! enough is left, runs each action within its limit, the 15-minute limit
//! of a run or what is left of the budget, never its commit
//! ([`within_budget`]), and gives back what it holds before the budget
//! ends, every claim it takes lasting what is left of the budget and a
//! margin ([`crate::lease`]). What it didn't reach is handed on to a fresh
//! invocation ([`Host::hand_on`]), or left for the minute tick.
//!
//! Work runs as the system ([`System`]), the caller `Access.system` grants
//! (area 9): a schedule's run, or a subscriber's delivery limited to the
//! tenant its event came from. Only the runner makes it, and the HTTP
//! component never calls the runner with a request's context, so no
//! request can act as the system.
//!
//! What it runs is a [`Delivery`] of an event a committed action emitted
//! to one of its subscribers ([`Deferred::Deliveries`], chunk 6.1b), which
//! [`deliver`] claims, runs as the system limited to the event's tenant,
//! and records as succeeded, retried later or failed ([`crate::outbox`]),
//! or a batch run ([`Deferred::Batch`]); [`sweep`] delivers what deferred
//! work dropped. Schedules run their actions here on each tick
//! ([`crate::schedules`], chunk 6.2), and the tick calls [`sweep`].

use std::collections::VecDeque;
use std::ops::ControlFlow;
use std::rc::Rc;

use jadpo_std::Runs;
use jadpo_std::subscriber::Target;
use jadpo_value::{Validator, Value};

pub use crate::budget::RUN_LIMIT_MS;
use crate::budget::{Budget, Cut, Invocation};
use crate::capability::{Call, Repeat};
use crate::ctx::System;
use crate::effects::Unanswered;
pub use crate::outbox::Delivery;
use crate::race::{First, join_all, race};
use crate::{
    Clock, Ctx, Env, Executor, FaultKind, Frame, Host, Isolation, LocalFuture, MemoryHost, Outcome,
    SqlType, SqlValue, StoreError, Transaction,
};

/// A run of callable `callable` with `args`, as the system: a subscriber's
/// delivery or a schedule's run (area 12), in a boundary of its own.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Job {
    pub callable: u32,
    pub args: Vec<Value>,
    pub system: System,
}

/// Work left for after a response (chunk 6.1a): each piece with a record
/// of its own in the store, so work an invocation never reached is never
/// lost (chunk 6.7).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Deferred {
    /// The deliveries of what an action emitted, once it committed, in
    /// the order it emitted it: each event to each of its subscribers
    /// (chunk 6.1b).
    Deliveries(Vec<Delivery>),
    /// A batch run to go on with (area 12, chunk 6.4): one an action
    /// started, left as it was started and found only once that action
    /// has committed, or one a run handed on to carry it further.
    Batch(crate::batches::BatchStart),
}

/// Whether the work a route's action set off settled before it answered
/// (area 7, "Waiting for follow-on work"; chunk 6.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Settled {
    /// Every delivery of what the action emitted, and of what their
    /// subscribers emitted in turn, succeeded, failed for good or was
    /// parked: the route answers with its own status.
    Settled,
    /// Some of it is still to run: a delivery waiting to retry, one
    /// another runner holds, a batch run, or work `within:` cut short.
    /// The route answers 202, and the rest carries on after the response.
    Unsettled,
}

/// Runs `work`, what a route's committed action left, and the work it
/// sets off in turn, in `ctx` before it answers, until all of it has settled or
/// `within` (the route's `within:` timer) finishes first (area 7,
/// "Waiting for follow-on work"; chunk 6.3). Each delivery runs as
/// [`deliver`] runs one, as the system limited to its event's tenant, and
/// whoever ran `ctx` runs it again after. A claim it takes lasts the
/// longest deadline a route has and the margin, since the request waits
/// no longer (chunk 6.7).
///
/// It stops waiting at the first piece that hasn't settled, since the
/// answer is 202 from then on: a delivery left to retry after a backoff,
/// which can't be waited for; one this runner couldn't claim; a batch run,
/// which is long work no route waits on; or the piece whose action's body
/// was running when `within` finished, or the host began to shut down,
/// which is dropped unfinished, nothing it did kept, its claim given back
/// as it was ([`crate::outbox::RELEASE`]). Only a body is raced, never its
/// commit (chunk 6.6a), so work that committed is never given back.
/// Whatever is left goes on `ctx`'s deferred work, for after the
/// response, exactly as it would without `settle:`; a delivery waiting to
/// retry is the sweep's.
pub async fn settle_within(
    executor: &dyn Executor,
    ctx: &mut Ctx<'_>,
    work: Vec<Deferred>,
    within: &mut crate::LocalFuture<'_, ()>,
) -> Settled {
    use crate::outbox::{FAILED, OUTCOME_UNKNOWN, SUCCEEDED};
    let host = ctx.host();
    let budget = Budget::of(host, u64::from(jadpo_std::http::MAX_DEADLINE_MS));
    // A route that waits for follow-on work is served only where it runs:
    // `jadpo check` refuses one in a deployable that doesn't run
    // subscribers (`deployable.settle_unrun`).
    let runs = Runs::EVERYTHING;
    let mut queue: VecDeque<Piece> = VecDeque::new();
    split(runs, work, &mut queue);
    // Work left before it, which isn't this route's to wait for.
    let earlier = ctx.take_deferred();
    let settled = loop {
        let Some(piece) = queue.pop_front() else {
            break Settled::Settled;
        };
        // `within:` may have passed while the last piece's statements ran.
        let passed =
            std::future::poll_fn(|cx| std::task::Poll::Ready(within.as_mut().poll(cx).is_ready()))
                .await;
        if passed || budget.stopping() {
            queue.push_front(piece);
            break Settled::Unsettled;
        }
        let done = match piece {
            Piece::Batch(start) => {
                queue.push_front(Piece::Batch(start));
                false
            }
            Piece::Delivery(delivery) => {
                // Given back if `within:` passes, or the host begins to
                // shut down, while its subscriber's action runs.
                let mut stop: LocalFuture<'_, ()> = Box::pin(async {
                    let _ = race(within.as_mut(), host.stopping()).await;
                });
                let attempted =
                    attempt_delivery(executor, ctx, &budget, delivery.clone(), Some(&mut stop))
                        .await;
                match attempted {
                    Some((_, state))
                        if state == SUCCEEDED || state == FAILED || state == OUTCOME_UNKNOWN =>
                    {
                        true
                    }
                    // A crash `jadpo test` made (chunk 7.5d): delivered
                    // again at once, as the request still waits.
                    Some((Outcome::Fault(fault), GIVEN_BACK))
                        if fault.kind == FaultKind::Crashed =>
                    {
                        queue.push_front(Piece::Delivery(delivery));
                        true
                    }
                    Some((_, GIVEN_BACK)) => {
                        queue.push_front(Piece::Delivery(delivery));
                        false
                    }
                    // Left to retry after a wait: the sweep's.
                    Some(_) => false,
                    // Not claimed: another runner has it, or its store
                    // couldn't be reached. Deferred work tries again.
                    None => {
                        queue.push_front(Piece::Delivery(delivery));
                        false
                    }
                }
            }
        };
        split(runs, ctx.take_deferred(), &mut queue);
        if !done {
            break Settled::Unsettled;
        }
    };
    let left = ctx.take_deferred();
    let background = ctx.background_mut();
    background.defer_all(earlier);
    background.defer_all(left);
    background.defer_all(unsplit(queue));
    settled
}

/// Runs `job` in `ctx` as the system, then puts back whoever ran `ctx`:
/// how a schedule's run and a subscriber's delivery run their actions,
/// and how a test runs one as the system would (area 9).
pub async fn as_system(executor: &dyn Executor, ctx: &mut Ctx<'_>, job: Job) -> Outcome {
    if ctx.in_action() {
        return Outcome::Fault(ctx.fault(
            FaultKind::Defect("the runner's work inside a running action".into()),
            Frame::ENTRY,
        ));
    }
    let saved = ctx.runner();
    ctx.become_system(job.system);
    let outcome = executor.run(ctx, job.callable, job.args).await;
    ctx.restore_runner(saved);
    outcome
}

/// Runs `job` in `ctx` as the system within `budget` (chunk 6.7): the one
/// way the runner runs a subscriber's or a schedule's action. Its body is
/// raced against its limit, the 15-minute limit of a run or what is left
/// of the budget before the time it keeps back, whichever is sooner, and
/// against the host shutting down, never its commit ([`held`]); a body
/// that ends past its limit keeps nothing. Why it was cut, when it was.
pub(crate) async fn within_budget(
    executor: &dyn Executor,
    ctx: &mut Ctx<'_>,
    budget: &Budget<'_>,
    job: Job,
) -> Result<Outcome, Cut> {
    // Work that never waits gives the thread back as it checks this, so
    // the requests the worker answers carry on beside it.
    ctx.set_deadline(Some(budget.limit_ms()));
    let mut cut = budget.cut();
    held(executor, ctx, job, &mut cut, Cut::OutOfTime).await
}

/// Runs `job` in `ctx` as [`as_system`] does with only its action's body
/// raced against `stop`, never its commit (chunk 6.6a), as a route runs
/// its action against its deadline ([`Ctx::hold`]): the transaction is
/// held open until the body has its value, then committed outside the
/// race, so work that committed is never taken for work that didn't. A
/// concurrent change found at that commit runs the body again, as the
/// boundary would, up to [`crate::RERUN_ATTEMPTS`] times. What `stop`
/// gave when it finished first, or `overran` when the body ended past
/// `ctx`'s deadline: what the body had begun is dropped, nothing it did
/// kept, and whoever ran `ctx` runs it again.
async fn held<S: Copy>(
    executor: &dyn Executor,
    ctx: &mut Ctx<'_>,
    job: Job,
    stop: &mut crate::LocalFuture<'_, S>,
    overran: S,
) -> Result<Outcome, S> {
    let runner = ctx.runner();
    // Run again after a concurrent change at the commit, its keyed calls
    // take the steps they took, and an unkeyed change that went out is
    // never sent again (chunk 7.9a).
    let steps = ctx.steps();
    let mark = ctx.capabilities().calls().len();
    let mut attempt = 1;
    loop {
        ctx.set_steps(steps);
        ctx.hold();
        let outcome = match race(as_system(executor, ctx, job.clone()), &mut *stop).await {
            First::Left(outcome) => outcome,
            First::Right(why) => {
                ctx.abandon();
                ctx.restore_runner(runner);
                return Err(why);
            }
        };
        if ctx.out_of_time() {
            ctx.abandon();
            return Err(overran);
        }
        let Some(open) = ctx.take_held() else {
            return Ok(outcome);
        };
        match crate::commit_held(ctx, open, None).await {
            Ok(()) => return Ok(outcome),
            Err(StoreError::ConcurrentChange) if ctx.capabilities().may_have_acted(mark) => {
                return Ok(Outcome::Failed(Box::new(crate::Failure {
                    frames: ctx.frames(),
                    ..crate::outcome_unknown()
                })));
            }
            Err(StoreError::ConcurrentChange) if attempt < crate::RERUN_ATTEMPTS => attempt += 1,
            Err(_) => {
                return Ok(Outcome::Failed(Box::new(crate::Failure {
                    frames: ctx.frames(),
                    ..crate::unavailable()
                })));
            }
        }
    }
}

/// Runs `work` as a deployable does after the response is sent, within
/// the budget its host gives an invocation after a response: each
/// delivery and batch run in a context of its own on `host`, under the
/// host's clock and no seed, as the system, and then the work it set off,
/// while enough of the budget is left. A host with no clock runs nothing:
/// each delivery is left for the sweep. The outcome of each delivery and
/// batch run, in the order they ran; a delivery's record keeps it too
/// (chunk 6.1b). What it didn't reach stays in the store.
pub async fn run_deferred(
    executor: &dyn Executor,
    host: &dyn Host,
    work: Vec<Deferred>,
) -> Vec<Outcome> {
    let budget = Budget::start(host, Invocation::AfterResponse);
    drive(executor, host, &budget, work, None).await.outcomes
}

/// One piece of deferred work, run on its own.
enum Piece {
    Delivery(Delivery),
    Batch(crate::batches::BatchStart),
}

/// `work`'s deliveries and batch runs, each a piece of its own, at the
/// back of `queue`.
///
/// What the program doesn't run is dropped here, the one place work joins
/// a runner (area 14, "Declared deployables"; chunk 8.1o2): a delivery in
/// a deployable that doesn't run `Trigger.subscribers`, and a batch run in
/// one that doesn't run `Trigger.schedules`. Each is committed already, a
/// delivery in its outbox and a batch run on its row, so the tick of a
/// deployable that runs it picks it up.
fn split(runs: Runs, work: Vec<Deferred>, queue: &mut VecDeque<Piece>) {
    for next in work {
        match next {
            Deferred::Deliveries(deliveries) if runs.subscribers => {
                queue.extend(deliveries.into_iter().map(Piece::Delivery));
            }
            Deferred::Batch(start) if runs.schedules => queue.push_back(Piece::Batch(start)),
            Deferred::Deliveries(_) | Deferred::Batch(_) => {}
        }
    }
}

/// `queue`'s pieces as work left for after the response, in order.
fn unsplit(queue: VecDeque<Piece>) -> Vec<Deferred> {
    queue
        .into_iter()
        .map(|piece| match piece {
            Piece::Delivery(delivery) => Deferred::Deliveries(vec![delivery]),
            Piece::Batch(start) => Deferred::Batch(start),
        })
        .collect()
}

/// What one invocation's run of work did.
struct Worked {
    /// The outcome of each piece that ran, in order.
    outcomes: Vec<Outcome>,
    /// What it didn't reach once its budget was spent, a batch run it
    /// handed on among it.
    left: Vec<Deferred>,
    /// Whether it ran any piece.
    ran: bool,
}

/// Runs `work` within `budget`: each piece at the host's time as it
/// starts, never one worked out before (chunk 6.6a), or at `now` on a
/// host with no clock; no piece once the budget is spent (chunk 6.7).
async fn drive(
    executor: &dyn Executor,
    host: &dyn Host,
    budget: &Budget<'_>,
    work: Vec<Deferred>,
    now: Option<i64>,
) -> Worked {
    let runs = executor.plan().runs;
    let mut queue: VecDeque<Piece> = VecDeque::new();
    split(runs, work, &mut queue);
    let mut outcomes = Vec::new();
    let mut ran = false;
    while let Some(piece) = queue.pop_front() {
        // Its budget spent, the invocation takes no more: a delivery waits
        // in the outbox and a batch run on its row, for the next.
        if budget.spent() {
            queue.push_front(piece);
            break;
        }
        let Some(now) = host.now_ms().or(now) else {
            continue;
        };
        ran = true;
        let env = Env {
            seed: None,
            clock: Clock::fixed(now),
        };
        let mut ctx = Ctx::of(executor, host, env);
        let outcome = match piece {
            Piece::Delivery(delivery) => {
                attempt_delivery(executor, &mut ctx, budget, delivery, None)
                    .await
                    .map(|(outcome, _)| outcome)
            }
            // A batch run makes a context per batch, and hands itself on
            // once the budget is spent (chunk 6.7). Its future is boxed,
            // so a runner, and the tick that runs one, never holds it
            // inline.
            Piece::Batch(start) => {
                let (ran, more) =
                    Box::pin(crate::batches::run_within(executor, host, budget, &start)).await;
                split(runs, more, &mut queue);
                Some(match ran {
                    crate::batches::Ran::Failed(why) => Outcome::Fault(Box::new(crate::Fault {
                        kind: FaultKind::Defect(format!("a batch run stopped: {why}")),
                        frames: Vec::new(),
                    })),
                    _ => Outcome::Value(Value::Unit),
                })
            }
        };
        split(runs, ctx.take_deferred(), &mut queue);
        outcomes.extend(outcome);
    }
    Worked {
        outcomes,
        left: unsplit(queue),
        ran,
    }
}

/// Delivers `delivery` in `ctx`, which runs no action (area 12,
/// "Delivery: at least once, safe to repeat"; chunk 6.1b), within the
/// budget its host gives an invocation after a response: claims its
/// record, unless another runner has it or it isn't due; finds whether
/// its subscriber handled it already; runs the subscriber's action as the
/// system limited to the event's tenant, continuing the event's trace,
/// its handled record committing with its writes; then records how it
/// ended. `None` when it wasn't this runner's to deliver, the program
/// has no subscribers to deliver it to, or a circuit breaker held it
/// (chunk 7.2b).
pub async fn deliver(
    executor: &dyn Executor,
    ctx: &mut Ctx<'_>,
    delivery: Delivery,
) -> Option<Outcome> {
    let budget = Budget::start(ctx.host(), Invocation::AfterResponse);
    attempt_delivery(executor, ctx, &budget, delivery, None)
        .await
        .map(|(outcome, _)| outcome)
}

/// The state [`attempt_delivery`] gives a delivery whose subscriber's run
/// was stopped and given back: by a route's `within:`, or by its host
/// shutting down. `pending` again, due at once, its attempt not counted.
const GIVEN_BACK: &str = "given back";

/// [`deliver`] within `budget`, with the state its record was left in:
/// `succeeded`, `failed`, `pending` to be tried again after a wait, or
/// `outcome_unknown`; or [`GIVEN_BACK`]. Its claim lasts what is left of
/// `budget` and the margin ([`crate::lease`]). The subscriber's action, or
/// the capability's operation it targets (chunk 7.0c), runs within
/// `budget` ([`within_budget`]): one its limit stops is an attempt that
/// ran out of time, settled against its `retry:` budget (chunk 6.6a); one
/// its host's shutdown stops is given back. While a route waits for it,
/// `within` races the action's body in place of the budget's limit, and
/// one it stops is given back too. A delivery claimed over its `retry:`
/// budget or past `within:` from its event fails without running (chunk
/// 6.6a). Once a call its run made may have acted outside, and sending it
/// again could act twice, the delivery is neither given back nor retried
/// but parked as `outcome_unknown` for an operator (area 16).
///
/// A host's development call log hears each attempt whose subscriber's
/// action reached steps under a record, with what became of each (chunk
/// 7.5f).
pub(crate) async fn attempt_delivery(
    executor: &dyn Executor,
    ctx: &mut Ctx<'_>,
    budget: &Budget<'_>,
    delivery: Delivery,
    within: Option<&mut crate::LocalFuture<'_, ()>>,
) -> Option<(Outcome, &'static str)> {
    let from = ctx.steps_kept();
    let (id, subscriber) = (delivery.id, delivery.subscriber.clone());
    // Each attempt counts its keyed calls from none under its own key,
    // with none of the models' answers whoever runs `ctx` was given, as
    // its retry in a context of its own does, a route waiting for it
    // (`settle:`) or a test; theirs are given back after (chunk 7.9a).
    let counted = ctx.steps();
    ctx.set_steps(0);
    let call_key = ctx.swap_call_key(None);
    let reused = ctx.swap_reused(Vec::new());
    let attempted = attempt(executor, ctx, budget, delivery, within).await;
    ctx.set_steps(counted);
    ctx.swap_call_key(call_key);
    ctx.swap_reused(reused);
    let steps = ctx.take_steps(from);
    if let (Some((_, state)), false) = (&attempted, steps.is_empty())
        && let Some(log) = ctx.host().call_log()
    {
        let name = executor
            .plan()
            .events()
            .and_then(|events| events.plan.subscriber(&subscriber))
            .map_or(subscriber.as_str(), |plan| plan.name.as_str());
        log.attempt_ended(&crate::call_log::DeliveryAttempt {
            delivery: id,
            subscriber: name,
            state,
            steps: &steps,
        });
    }
    attempted
}

/// [`attempt_delivery`]'s attempt.
async fn attempt(
    executor: &dyn Executor,
    ctx: &mut Ctx<'_>,
    budget: &Budget<'_>,
    delivery: Delivery,
    within: Option<&mut crate::LocalFuture<'_, ()>>,
) -> Option<(Outcome, &'static str)> {
    use crate::outbox::{
        self, After, CHAIN_LIMIT, DIVERGED, FAILED, OUTCOME_UNKNOWN, PENDING, SUCCEEDED,
        failure_json, micros,
    };
    if ctx.in_action() {
        return Some((
            Outcome::Fault(ctx.fault(
                FaultKind::Defect("a delivery inside a running action".into()),
                Frame::ENTRY,
            )),
            PENDING,
        ));
    }
    let events = executor.plan().events()?;
    let host = ctx.host();
    let database = host.database(&delivery.store)?;
    // Its lease, and its next attempt, count from when it is claimed: the
    // host's time now, or the context's on a host with no clock.
    let now = host.now_ms().unwrap_or_else(|| ctx.now());
    let id = SqlValue::Uuid(delivery.id);
    let mut lease = outbox::Leased {
        id: delivery.id,
        attempt: 0,
    };
    let claimed = crate::lease::claim(database, budget, &lease, now)
        .await
        .ok()??;
    let attempt = match claimed.first() {
        Some(SqlValue::Int4(n)) => u32::try_from(*n).unwrap_or(u32::MAX),
        _ => return None,
    };
    lease.attempt = attempt;
    let counted = SqlValue::Int4(i32::try_from(attempt).unwrap_or(i32::MAX));
    let settle = |status: &'static str, next: Option<i64>, failure: Option<SqlValue>| {
        let params = [
            id.clone(),
            counted.clone(),
            SqlValue::Text(status.to_string()),
            next.map_or(SqlValue::Null(SqlType::Timestamp), micros),
            failure.unwrap_or(SqlValue::Null(SqlType::Json)),
        ];
        async move {
            let _ = database.query_once(outbox::SETTLE, &params).await;
        }
    };
    let Some(subscriber) = events.plan.subscriber(&delivery.subscriber) else {
        settle(FAILED, None, Some(failure_json("subscriber_removed", None))).await;
        return Some((
            Outcome::Fault(ctx.fault(
                FaultKind::Defect(format!(
                    "a delivery to `{}`, which this build doesn't have",
                    delivery.subscriber
                )),
                Frame::ENTRY,
            )),
            FAILED,
        ));
    };
    if delivery.depth >= CHAIN_LIMIT {
        settle(FAILED, None, Some(failure_json("event_chain_limit", None))).await;
        let mut failure = crate::Failure::new(
            "runtime event_chain_limit",
            "EventChainLimit",
            "event_chain_limit",
        );
        failure.kind = Some("rejected".to_string());
        return Some((Outcome::Failed(Box::new(failure)), FAILED));
    }
    // Its budget: how many attempts, and until when from its event.
    let within_ms = delivery
        .emitted_at
        .saturating_add(i64::try_from(subscriber.within_ms).unwrap_or(i64::MAX));
    // An attempt that ended in a failure worth retrying is tried again
    // after a wait, while its budget lasts, and fails after.
    let retried = |code: &str, kind: Option<&str>| {
        let next = now.saturating_add(outbox::backoff(attempt, jitter(host)));
        let failure = Some(failure_json(code, kind));
        let again = attempt < subscriber.attempts && next <= within_ms;
        async move {
            if again {
                settle(PENDING, Some(next), failure).await;
                PENDING
            } else {
                settle(FAILED, None, failure).await;
                FAILED
            }
        }
    };
    // A repeated delivery its subscriber handled already does nothing.
    if let Some(store) = &subscriber.store {
        let handled = match host.database(store) {
            Some(own) => {
                own.query_once(outbox::FIND_HANDLED, std::slice::from_ref(&id))
                    .await
            }
            None => Err(StoreError::Unavailable),
        };
        match handled {
            Ok(rows) if !rows.is_empty() => {
                settle(SUCCEEDED, None, None).await;
                return Some((Outcome::Value(Value::Unit), SUCCEEDED));
            }
            Ok(_) => {}
            Err(_) => {
                let state = retried("unavailable", Some("unavailable")).await;
                return Some((Outcome::Failed(Box::new(crate::unavailable())), state));
            }
        }
    }
    // Claimed again over its budget, or past `within:`, as a lost worker
    // or a dropped run leaves it: it fails without running.
    if attempt > subscriber.attempts || now > within_ms {
        let _ = database
            .query_once(
                outbox::GIVE_UP,
                &[
                    id.clone(),
                    counted.clone(),
                    failure_json(FaultKind::DeadlineExceeded.classification(), None),
                ],
            )
            .await;
        return Some((
            Outcome::Fault(ctx.fault(FaultKind::DeadlineExceeded, Frame::ENTRY)),
            FAILED,
        ));
    }
    // Given back to run later, its attempt not counted, while a connection
    // it calls out through has every slot out (area 11, `max_concurrent:`;
    // chunk 7.6c): it waits in the outbox rather than in the worker.
    let through = subscriber.calls_through();
    if through
        .iter()
        .any(|connection| host.slots().full(connection))
    {
        let later = now.saturating_add(crate::slots::WAIT_MS);
        crate::lease::release(database, &lease, later).await;
        return None;
    }
    // Held, its attempt given back, while a provider it calls out to is
    // unavailable and isn't due a probe (chunk 7.2b).
    let watch =
        match crate::breaker::gate(database, &through, delivery.id, attempt, budget, now).await {
            crate::breaker::Gate::Held => return None,
            crate::breaker::Gate::Run(watch) => watch,
        };
    // What the run sends outside is logged from here (chunk 7.0c).
    let mark = ctx.capabilities().calls().len();
    // A provider call's key is derived from the delivery's id, which every
    // attempt shares (area 12, "Keys"; chunk 7.2c).
    ctx.set_call_key("delivery", jadpo_value::encoding::to_hex(&delivery.id));
    // Its key survives a retry, so an action with several changing calls
    // keeps a record of its steps, held by this attempt (area 12, "One
    // runner per record"; chunk 7.5b).
    ctx.set_resumable(Some(i64::from(attempt)));
    // A sign-in email is made with a link and a code no attempt before
    // had, which end the last attempt's: so each attempt's calls go under
    // keys of its own, and a provider that replays a key's first answer
    // still sends the link that works. Its key doesn't survive a retry,
    // so no record of its steps is kept (chunk 7.9a).
    if subscriber.event == jadpo_std::email::MAGIC_LINK_EMAIL {
        ctx.set_resumable(None);
        let Some(fresh) = ctx.new_id() else {
            let state = retried("unavailable", Some("unavailable")).await;
            return Some((Outcome::Failed(Box::new(crate::unavailable())), state));
        };
        ctx.set_call_key("sign-in link", jadpo_value::encoding::to_hex(&fresh));
    }
    let ran = match &subscriber.target {
        // Another deployable's to deliver (chunk 8.1o3): one that runs no
        // subscribers delivers nothing, so this is a defect.
        Target::Elsewhere => Ok(Outcome::Fault(ctx.fault(
            FaultKind::Defect("a subscriber another deployable runs".into()),
            Frame::ENTRY,
        ))),
        // A capability's operation is handed the event whole (chunk 7.0c).
        Target::Capability {
            connection,
            operation,
            repeat,
        } => {
            // An unkeyed send is marked as going out first, so a worker
            // lost while it is out leaves it for an operator, never sends
            // it again: the sweep parks it once its lease has passed.
            if *repeat == Repeat::Unkeyed {
                let marked = database
                    .query_once(outbox::MARK_SENDING, &[id.clone(), counted.clone()])
                    .await;
                if !matches!(marked, Ok(rows) if !rows.is_empty()) {
                    let state = retried("unavailable", Some("unavailable")).await;
                    ctx.set_resumable(None);
                    return Some((Outcome::Failed(Box::new(crate::unavailable())), state));
                }
            }
            let call = Call::new(
                connection.clone(),
                operation.clone(),
                delivery.value.clone(),
                *repeat,
            );
            send(ctx, call, budget, within).await
        }
        // An email, rendered whole and sent with no key (area 16; chunk
        // 7.3): what can't be rendered is settled as a subscriber's
        // outcome is, before anything goes out.
        Target::Email { connection } => {
            let emails = &events.plan.emails;
            let rendered = match emails.email(&subscriber.event) {
                Some(email) => {
                    crate::email::message(executor, ctx, emails, email, &delivery.value).await
                }
                None => Err(Outcome::Fault(ctx.fault(
                    FaultKind::Defect("an email this build doesn't have".into()),
                    Frame::ENTRY,
                ))),
            };
            match rendered {
                Err(outcome) => Ok(outcome),
                Ok(message) => {
                    let marked = database
                        .query_once(outbox::MARK_SENDING, &[id.clone(), counted.clone()])
                        .await;
                    if !matches!(marked, Ok(rows) if !rows.is_empty()) {
                        let state = retried("unavailable", Some("unavailable")).await;
                        ctx.set_resumable(None);
                        return Some((Outcome::Failed(Box::new(crate::unavailable())), state));
                    }
                    // A mail package's operation, as its mapping builds
                    // its input (chunk 7.3c), unless the host catches the
                    // mail in its place, as a test's does.
                    let caught = ctx
                        .host()
                        .capability(connection)
                        .is_some_and(|driver| driver.catches_mail());
                    match (&emails.package, caught) {
                        (Some(package), false) => {
                            let sending = crate::email::send_package(
                                executor, ctx, connection, package, &message,
                            );
                            match within {
                                Some(stop) => match race(sending, stop).await {
                                    First::Left(outcome) => Ok(outcome),
                                    First::Right(()) => Err(Cut::ShuttingDown),
                                },
                                None => match race(sending, budget.cut()).await {
                                    First::Left(outcome) => Ok(outcome),
                                    First::Right(cut) => Err(cut),
                                },
                            }
                        }
                        _ => {
                            let mut call = Call::new(
                                connection.clone(),
                                jadpo_std::email::SEND,
                                message,
                                Repeat::Unkeyed,
                            );
                            call.mail = Some(Box::new(crate::capability::MailSent {
                                // Its identity key, `type members.WelcomeEmail`,
                                // as people name it.
                                email: subscriber
                                    .event
                                    .split_once(' ')
                                    .map_or(subscriber.event.as_str(), |(_, name)| name)
                                    .to_string(),
                                // The trace of the work that emitted it.
                                trace: delivery.trace.or_else(|| ctx.trace()).map(|trace| {
                                    trace
                                        .header()
                                        .split('-')
                                        .nth(1)
                                        .unwrap_or_default()
                                        .to_string()
                                }),
                            }));
                            send(ctx, call, budget, within).await
                        }
                    }
                }
            }
        }
        // The sign-in email the application sends itself (area 16,
        // "Built-in emails", level 5): its action is given the
        // `MagicLinkRequest` made now, with a link and a code no attempt
        // before had, or nothing runs when the request was used or
        // replaced since.
        Target::Action { callable, .. }
            if subscriber.event == jadpo_std::email::MAGIC_LINK_EMAIL =>
        {
            let plan = events
                .plan
                .emails
                .email(&subscriber.event)
                .and_then(|email| email.builtin.as_ref())
                .map(|jadpo_std::email::BuiltinEmail::MagicLink(plan)| plan.clone());
            let made = match &plan {
                Some(plan) => {
                    crate::http::magic_links::request_value(ctx, plan, &delivery.value).await
                }
                None => Err("a sign-in email this build doesn't have"),
            };
            match made {
                Ok(None) => Ok(Outcome::Value(Value::Unit)),
                Err("unavailable") => Ok(Outcome::Failed(Box::new(crate::unavailable()))),
                Err(what) => Ok(Outcome::Fault(
                    ctx.fault(FaultKind::Defect(what.into()), Frame::ENTRY),
                )),
                Ok(Some(request)) => {
                    // The tenant of its delivery, as a subscriber's: the
                    // sign-in email names none (chunk 7.9b).
                    let job = Job {
                        callable: *callable,
                        args: vec![request],
                        system: System {
                            tenant: outbox::tenant(subscriber, &delivery.value),
                        },
                    };
                    run_job(executor, ctx, &delivery, subscriber, job, budget, within).await
                }
            }
        }
        Target::Action { callable, args } => {
            let Some(args) = outbox::arguments(args, &delivery.value) else {
                settle(FAILED, None, Some(failure_json("internal_fault", None))).await;
                ctx.set_resumable(None);
                return Some((
                    Outcome::Fault(ctx.fault(
                        FaultKind::Defect(
                            "an event without the fields its subscriber reads".into(),
                        ),
                        Frame::ENTRY,
                    )),
                    FAILED,
                ));
            };
            let job = Job {
                callable: *callable,
                args,
                system: System {
                    tenant: outbox::tenant(subscriber, &delivery.value),
                },
            };
            // A package's own subscriber runs its package's action with
            // the API reached through the connection its webhook came in
            // on (chunk 7.6b).
            if let Some(connection) = &subscriber.through {
                ctx.enter_package(connection);
            }
            let ran = run_job(executor, ctx, &delivery, subscriber, job, budget, within).await;
            if subscriber.through.is_some() {
                ctx.leave_package();
            }
            ran
        }
    };
    ctx.set_resumable(None);
    // Whether a call the run made may have acted outside in a way sending
    // it again would repeat: then the delivery is never given back nor
    // retried, but parked for an operator (area 16).
    let acted = ctx.capabilities().may_have_acted(mark);
    // How its calls went counts towards their connections' breakers.
    let calls = ctx
        .capabilities()
        .calls()
        .get(mark..)
        .unwrap_or_default()
        .to_vec();
    let ended = host.now_ms().unwrap_or(now);
    crate::breaker::record(database, &watch, delivery.id, &calls, ended).await;
    let outcome = match ran {
        Ok(outcome) => outcome,
        // What the subscriber's action had begun is dropped, nothing it
        // did kept, and the delivery is given back as it was, for
        // deferred work or the sweep to run.
        Err(Cut::ShuttingDown) if !acted => {
            crate::lease::release(database, &lease, now).await;
            return Some((
                Outcome::Fault(ctx.fault(FaultKind::DeadlineExceeded, Frame::ENTRY)),
                GIVEN_BACK,
            ));
        }
        // Its time ran out: nothing it did kept, and the attempt is
        // settled as one that timed out.
        Err(_) => Outcome::Fault(ctx.fault(FaultKind::DeadlineExceeded, Frame::ENTRY)),
    };
    // A crash `jadpo test` made at a step (chunk 7.5d): nothing the run
    // did was kept, and the delivery is given back, due now, as a
    // restart's sweep would find it, without counting the attempt.
    if matches!(&outcome, Outcome::Fault(fault) if fault.kind == FaultKind::Crashed) {
        crate::lease::release(database, &lease, now).await;
        return Some((outcome, GIVEN_BACK));
    }
    let after = outbox::after(&outcome);
    let after = if acted { after.once_acted() } else { after };
    let state = match after {
        After::Succeeded => {
            settle(SUCCEEDED, None, None).await;
            SUCCEEDED
        }
        After::Retry { code, kind } => retried(&code, kind.as_deref()).await,
        After::Fail { code, kind } => {
            settle(FAILED, None, Some(failure_json(&code, kind.as_deref()))).await;
            FAILED
        }
        After::Park { cause } => {
            settle(
                OUTCOME_UNKNOWN,
                None,
                Some(outbox::parked_json(Some(&cause))),
            )
            .await;
            OUTCOME_UNKNOWN
        }
        After::Diverge { problem } => {
            settle(DIVERGED, None, Some(outbox::diverged_json(&problem))).await;
            DIVERGED
        }
    };
    Some((outcome, state))
}

/// Runs a delivery's action, `job`, as the system in `ctx`: its trace
/// continuing the event's, one deeper in its chain, and its handled record
/// kept where its subscriber keeps it (chunk 6.1b). Raced against `within`
/// while a route waits for it, or else against `budget`'s limit.
async fn run_job(
    executor: &dyn Executor,
    ctx: &mut Ctx<'_>,
    delivery: &crate::outbox::Delivery,
    subscriber: &jadpo_std::subscriber::SubscriberPlan,
    job: Job,
    budget: &Budget<'_>,
    within: Option<&mut crate::LocalFuture<'_, ()>>,
) -> Result<Outcome, Cut> {
    let (trace, chain) = (ctx.trace(), ctx.chain());
    let continued = match delivery.trace {
        Some(parent) => ctx.new_span(parent),
        None => ctx.new_trace(),
    };
    ctx.set_trace(Some(continued));
    let background = ctx.background_mut();
    background.set_chain(delivery.depth.saturating_add(1));
    background.set_handled(subscriber.store.clone().map(|store| crate::ctx::Handled {
        store,
        id: delivery.id,
    }));
    let ran = match within {
        // A route stopping its wait gives the delivery back, as the
        // host shutting down does.
        Some(stop) => held(executor, ctx, job, stop, ())
            .await
            .map_err(|()| Cut::ShuttingDown),
        None => within_budget(executor, ctx, budget, job).await,
    };
    let background = ctx.background_mut();
    background.set_handled(None);
    background.set_trace(trace);
    background.set_chain(chain);
    ran
}

/// Hands `call`, a capability's delivery, to the capability seam in `ctx`
/// (chunk 7.0c), raced against `within` while a route waits for it, or
/// else against `budget`'s limit, as a subscriber's action is: its outcome,
/// or why it was cut while the call was out.
async fn send(
    ctx: &mut Ctx<'_>,
    call: Call,
    budget: &Budget<'_>,
    within: Option<&mut crate::LocalFuture<'_, ()>>,
) -> Result<Outcome, Cut> {
    let name = call.name();
    let sent = {
        let sending = crate::effects::call(ctx, call);
        match within {
            Some(stop) => match race(sending, stop).await {
                First::Left(sent) => sent,
                First::Right(()) => return Err(Cut::ShuttingDown),
            },
            None => match race(sending, budget.cut()).await {
                First::Left(sent) => sent,
                First::Right(cut) => return Err(cut),
            },
        }
    };
    Ok(match sent {
        Ok(value) => Outcome::Value(value),
        Err(Unanswered::Failed(code)) => {
            Outcome::Failed(Box::new(crate::Failure::new(name, code.clone(), code)))
        }
        Err(Unanswered::Failure(failure)) => Outcome::Failed(failure),
        Err(Unanswered::Fault(kind)) => Outcome::Fault(ctx.fault(kind, Frame::ENTRY)),
    })
}

/// Eight random bytes for a retry's jitter, from the host; none from a
/// host with no randomness, as a test's, whose waits are their ceilings.
fn jitter(host: &dyn Host) -> Option<[u8; 8]> {
    let mut bytes = [0u8; 8];
    host.random(&mut bytes).then_some(bytes)
}

/// Delivers what is due in `store`'s outbox at `now` (area 12, "Where
/// events travel"; chunk 6.1b), within the budget its host gives a tick:
/// every delivery still `pending` past its next attempt, or `running`
/// past its lease, which a crash between the commit and the delivery, a
/// host that dropped deferred work, or a worker that died holding a lease
/// left; at most [`SWEEP_BATCH`] of them, selected oldest first and run
/// in at most four concurrent chains (no completion-order promise), each as
/// [`run_deferred`] runs one, at the host's time as it starts rather than
/// `now` (chunk 6.6a), and the work they set off. A payload that no
/// longer reads as its type is parked with its problem, never dropped.
/// The once-a-minute tick that calls it is chunk 6.2's. The outcome of
/// each delivery that ran.
///
/// [`SWEEP_BATCH`]: crate::outbox::SWEEP_BATCH
pub async fn sweep(
    executor: &dyn Executor,
    host: &dyn Host,
    store: &str,
    now: i64,
) -> Result<Vec<Outcome>, StoreError> {
    let budget = Budget::start(host, Invocation::Tick);
    sweep_within(executor, host, &budget, store, now).await
}

/// A sweep's maximum concurrently driven delivery chains (chunk 9.1b1).
/// Below Workers' six opening slots; all connections and provider calls
/// still take their own host guards. No delivery ordering is promised.
/// Static round-robin lanes bound descendants and batches too; a lane
/// never runs two batches of one batch run at once.
const SWEEP_CONCURRENCY: usize = 4;

/// [`sweep`] within `budget`: the tick's own.
pub(crate) async fn sweep_within(
    executor: &dyn Executor,
    host: &dyn Host,
    budget: &Budget<'_>,
    store: &str,
    now: i64,
) -> Result<Vec<Outcome>, StoreError> {
    let Some(events) = executor.plan().events() else {
        return Ok(Vec::new());
    };
    let work = due(&events, host, store, now).await?;
    // Four independent lanes share this invocation's host and budget.
    // Each lane still uses the ordinary driver: its contexts, descendants
    // and each batch run remain serial; no action context crosses lanes.
    let mut lanes: Vec<Vec<Delivery>> = (0..SWEEP_CONCURRENCY).map(|_| Vec::new()).collect();
    let mut due: VecDeque<_> = work.into();
    while !due.is_empty() {
        for lane in &mut lanes {
            if let Some(delivery) = due.pop_front() {
                lane.push(delivery);
            }
        }
    }
    let pending: Vec<_> = lanes
        .into_iter()
        .filter(|lane| !lane.is_empty())
        .map(|lane| {
            drive(
                executor,
                host,
                budget,
                vec![Deferred::Deliveries(lane)],
                Some(now),
            )
        })
        .collect();
    // Drain every admitted lane, including after shutdown or a spent
    // budget: the ordinary driver settles in-flight work and leaves
    // unstarted work durable for the next tick.
    Ok(join_all(pending)
        .await
        .into_iter()
        .flat_map(|worked| worked.outcomes)
        .collect())
}

/// The deliveries due in `store`'s outbox at `now`, at most
/// [`SWEEP_BATCH`] of them, oldest first, each with its event read as its
/// type; one whose payload no longer reads is parked with its problem,
/// never dropped. What [`sweep`] delivers, and a test between its steps
/// ([`crate::test_steps`]; chunk 6.11).
///
/// [`SWEEP_BATCH`]: crate::outbox::SWEEP_BATCH
pub(crate) async fn due(
    events: &crate::outbox::Events<'_>,
    host: &dyn Host,
    store: &str,
    now: i64,
) -> Result<Vec<Delivery>, StoreError> {
    use crate::outbox::{DUE, SWEEP_BATCH, micros};
    let database = host
        .database(store)
        .ok_or_else(|| StoreError::Fault(format!("no store called `{store}`")))?;
    // What a circuit breaker holds and should no longer, and each due
    // probe, is released first (chunk 7.2b).
    crate::breaker::wake(database, now).await?;
    // A delivery whose worker was lost while its unkeyed call was out is
    // parked for an operator, never claimed again (chunk 7.0c).
    database
        .query_once(
            crate::outbox::PARK_LOST,
            &[
                micros(now),
                crate::outbox::parked_json(Some("lease_expired")),
            ],
        )
        .await?;
    let rows = database
        .query_once(DUE, &[micros(now), SqlValue::Int8(i64::from(SWEEP_BATCH))])
        .await?;
    Ok(deliveries(events, database, store, rows).await)
}

/// The deliveries `rows` hold, read as [`crate::outbox::DUE`] reads them
/// from `store`, each with its event read as its type; one whose payload
/// no longer reads is parked with its problem, never dropped.
async fn deliveries(
    events: &crate::outbox::Events<'_>,
    database: &dyn crate::Database,
    store: &str,
    rows: Vec<crate::Row>,
) -> Vec<Delivery> {
    use crate::outbox::{PARK, Trace, failure_json};
    let mut work = Vec::new();
    for row in rows {
        let (
            Some(SqlValue::Uuid(id)),
            Some(SqlValue::Uuid(outbox_id)),
            Some(SqlValue::Text(subscriber)),
            Some(SqlValue::Text(event)),
            Some(SqlValue::Text(payload)),
            Some(SqlValue::Timestamp(emitted)),
        ) = (
            row.first(),
            row.get(1),
            row.get(2),
            row.get(3),
            row.get(4),
            row.get(5),
        )
        else {
            continue;
        };
        let trace = match row.get(6) {
            Some(SqlValue::Text(header)) => Trace::parse(header),
            _ => None,
        };
        let depth = match row.get(7) {
            Some(SqlValue::Int4(depth)) => u32::try_from(*depth).unwrap_or(0),
            _ => 0,
        };
        // The encoding its payload was written in: `NULL` for one kept
        // before encodings were recorded (chunk 6.8).
        let encoding = row
            .get(8)
            .cloned()
            .unwrap_or(SqlValue::Null(crate::SqlType::Int2));
        match events.read(event, payload, &encoding) {
            Ok(value) => work.push(Delivery {
                store: store.to_string(),
                id: *id,
                outbox: *outbox_id,
                subscriber: subscriber.clone(),
                event: event.clone(),
                value,
                emitted_at: emitted.div_euclid(1000),
                trace,
                depth,
            }),
            Err(problem) => {
                let mut failure = match failure_json("payload_unreadable", None) {
                    SqlValue::Json(text) => {
                        serde_json::from_str::<serde_json::Value>(&text).unwrap_or_default()
                    }
                    _ => serde_json::Value::Null,
                };
                if let Some(object) = failure.as_object_mut() {
                    object.insert("problem".into(), problem.into());
                }
                let _ = database
                    .query_once(
                        PARK,
                        &[SqlValue::Uuid(*id), SqlValue::Json(failure.to_string())],
                    )
                    .await;
            }
        }
    }
    work
}

/// Runs `work` and what it sets off within `budget`, as the tick runs
/// what its schedules and hooks left (chunk 6.7): what it didn't reach
/// stays in the store for the next tick. The outcome of each piece that
/// ran.
pub(crate) async fn run_within(
    executor: &dyn Executor,
    host: &dyn Host,
    budget: &Budget<'_>,
    work: Vec<Deferred>,
) -> Vec<Outcome> {
    drive(executor, host, budget, work, None).await.outcomes
}

/// Gives `work` to `host` to run after the response it belongs to is
/// sent ([`Host::defer`]), within the budget the host gives an invocation
/// after a response: what a deployable and `jadpo dev` do with the work a
/// response left (`Response::deferred`). What it didn't reach is handed
/// on into a fresh invocation ([`Host::hand_on`]). `executor` and `host`
/// are kept until it has run.
pub fn after_response(executor: Rc<dyn Executor>, host: Rc<dyn Host>, work: Vec<Deferred>) {
    if work.is_empty() {
        return;
    }
    let runs = Rc::clone(&host);
    host.defer(invocation(executor, runs, work, Invocation::AfterResponse));
}

/// `work` run as one invocation of kind `kind` on `host`, within the
/// budget the host gives it (chunk 6.7). What it didn't reach once the
/// budget was spent, a batch run handed on by time among it, goes on into
/// a fresh invocation, unless the host is shutting down, or the
/// invocation ran nothing, so work that can't start in a whole budget
/// waits for the tick rather than hands itself on for ever.
fn invocation(
    executor: Rc<dyn Executor>,
    host: Rc<dyn Host>,
    work: Vec<Deferred>,
    kind: Invocation,
) -> LocalFuture<'static, ()> {
    Box::pin(async move {
        let budget = Budget::start(host.as_ref(), kind);
        let worked = drive(executor.as_ref(), host.as_ref(), &budget, work, None).await;
        let stopping = budget.stopping();
        drop(budget);
        if stopping || !worked.ran || worked.left.is_empty() {
            return;
        }
        let fresh = Rc::clone(&host);
        host.hand_on(HandOn {
            executor,
            host: fresh,
            work: worked.left,
        });
    })
}

/// What an invocation whose budget was spent hands on to a fresh one
/// (chunk 6.7; area 12, "Running background work"): the deliveries it
/// didn't reach and the batch runs it handed on by time, each with a
/// record of its own in the store. Its host runs it as a fresh invocation
/// with a budget of its own ([`Host::hand_on`]): natively on the host that
/// handed it on, [`HandOn::run`]; on Workers in a fresh invocation of the
/// Worker, which is sent [`HandOn::message`], each piece named by its
/// record, and reads it back ([`carry_on`]; chunk 8.2h).
pub struct HandOn {
    executor: Rc<dyn Executor>,
    host: Rc<dyn Host>,
    work: Vec<Deferred>,
}

impl std::fmt::Debug for HandOn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HandOn")
            .field("work", &self.work)
            .finish_non_exhaustive()
    }
}

impl HandOn {
    /// The work handed on.
    pub fn work(&self) -> &[Deferred] {
        &self.work
    }

    /// It run as a fresh invocation on the host that handed it on, with a
    /// budget of its own, handing on in turn what it doesn't reach.
    pub fn run(self) -> LocalFuture<'static, ()> {
        invocation(self.executor, self.host, self.work, Invocation::HandedOn)
    }

    /// It as JSON naming each piece by its record and store, in order, for
    /// a fresh invocation elsewhere to read back ([`carry_on`]): a
    /// delivery by its id, a batch run by its run's. It holds no event's
    /// value, so nothing an application keeps travels in it.
    pub fn message(&self) -> String {
        let mut pieces = Vec::new();
        for piece in &self.work {
            match piece {
                Deferred::Deliveries(deliveries) => {
                    pieces.extend(deliveries.iter().map(|delivery| {
                        serde_json::json!({
                            "store": delivery.store,
                            "delivery": crate::deliveries::hex(&delivery.id),
                        })
                    }));
                }
                Deferred::Batch(start) => pieces.push(serde_json::json!({
                    "store": start.store,
                    "batch": crate::deliveries::hex(&start.run),
                })),
            }
        }
        serde_json::json!({ "handed_on": pieces }).to_string()
    }
}

/// One piece of work a [`HandOn::message`] names.
enum Named {
    Delivery { store: String, id: [u8; 16] },
    Batch(crate::batches::BatchStart),
}

/// The pieces `message` names, in order; `None` when it isn't a
/// [`HandOn::message`].
fn named(message: &str) -> Option<Vec<Named>> {
    let document: serde_json::Value = serde_json::from_str(message).ok()?;
    let mut pieces = Vec::new();
    for piece in document.get("handed_on")?.as_array()? {
        let store = piece.get("store")?.as_str()?.to_string();
        let id = |key: &str| piece.get(key).and_then(|id| id.as_str()).map(id_from_hex);
        pieces.push(match (id("delivery"), id("batch")) {
            (Some(id), None) => Named::Delivery { store, id: id? },
            (None, Some(run)) => Named::Batch(crate::batches::BatchStart { store, run: run? }),
            _ => return None,
        });
    }
    Some(pieces)
}

/// An id from its 32 hexadecimal digits.
fn id_from_hex(text: &str) -> Option<[u8; 16]> {
    if text.len() != 32 {
        return None;
    }
    let mut id = [0u8; 16];
    for (at, byte) in id.iter_mut().enumerate() {
        *byte = u8::from_str_radix(text.get(at * 2..at * 2 + 2)?, 16).ok()?;
    }
    Some(id)
}

/// The work `message` names ([`HandOn::message`]) run on `host` as a fresh
/// invocation with a budget of its own, as [`HandOn::run`] runs it: each
/// delivery read back from its record, as the sweep reads one, unless it
/// has run or been given up since, and each batch run from its row; what
/// it doesn't reach is handed on in turn. `None` when `message` isn't one,
/// what it names left for the tick.
pub fn carry_on(
    executor: Rc<dyn Executor>,
    host: Rc<dyn Host>,
    message: &str,
) -> Option<LocalFuture<'static, ()>> {
    let pieces = named(message)?;
    Some(Box::pin(async move {
        let work = from_records(executor.as_ref(), host.as_ref(), pieces).await;
        if work.is_empty() {
            return;
        }
        invocation(executor, host, work, Invocation::HandedOn).await;
    }))
}

/// The work `pieces` name as the runner runs it, in their order: each
/// store's deliveries read in one statement; one a store can't give is
/// left for the tick.
async fn from_records(
    executor: &dyn Executor,
    host: &dyn Host,
    pieces: Vec<Named>,
) -> Vec<Deferred> {
    let mut wanted: Vec<(&str, Vec<SqlValue>)> = Vec::new();
    for piece in &pieces {
        if let Named::Delivery { store, id } = piece {
            match wanted.iter_mut().find(|(named, _)| named == store) {
                Some((_, ids)) => ids.push(SqlValue::Uuid(*id)),
                None => wanted.push((store, vec![SqlValue::Uuid(*id)])),
            }
        }
    }
    let mut found: Vec<Delivery> = Vec::new();
    if let Some(events) = executor.plan().events() {
        for (store, ids) in wanted {
            let Some(database) = host.database(store) else {
                continue;
            };
            let Ok(rows) = database
                .query_once(
                    crate::outbox::HANDED_ON,
                    &[SqlValue::Array(crate::SqlType::Uuid, ids)],
                )
                .await
            else {
                continue;
            };
            found.extend(deliveries(&events, database, store, rows).await);
        }
    }
    let mut work = Vec::new();
    for piece in pieces {
        match piece {
            Named::Delivery { store, id } => {
                if let Some(at) = found
                    .iter()
                    .position(|delivery| delivery.id == id && delivery.store == store)
                {
                    work.push(Deferred::Deliveries(vec![found.swap_remove(at)]));
                }
            }
            Named::Batch(start) => work.push(Deferred::Batch(start)),
        }
    }
    work
}

/// Runs `callable` once for each of `items`, its arguments, in order,
/// against `ctx`, handing each outcome to `each` with the item's index,
/// which says whether to go on: the runner's per-item mode. Inside a
/// [`Batch`] each item's action joins the batch's transaction and is
/// undone alone when it fails, as an action whose failure its caller
/// handles is (area 6), so one bad item never undoes the rest (area 12,
/// "Batches"); and, an action of its own, it has its own allowance of
/// events, never what the items before it in the batch left (chunk
/// 6.6a). How many items ran.
pub async fn items(
    executor: &dyn Executor,
    ctx: &mut Ctx<'_>,
    callable: u32,
    items: Vec<Vec<Value>>,
    each: &mut dyn FnMut(usize, Outcome) -> ControlFlow<()>,
) -> usize {
    let mut ran = 0;
    for (at, args) in items.into_iter().enumerate() {
        ran += 1;
        let outcome = if ctx.in_action() {
            if let Some(boundary) = ctx.boundary_mut() {
                boundary.own_allowance();
            }
            match crate::boundary::Undo::take(ctx).await {
                Ok(undo) => {
                    let outcome = executor.run(ctx, callable, args).await;
                    undo.settle(ctx, outcome).await
                }
                Err(outcome) => outcome,
            }
        } else {
            executor.run(ctx, callable, args).await
        };
        if each(at, outcome).is_break() {
            break;
        }
    }
    ran
}

/// One batch of a run over many items (area 12, "How a run goes"): a
/// transaction of its own in one store, begun as the batch opens, which
/// every item's action joins, run as the system. The run writes its
/// position in the same transaction ([`Batch::transaction`]) before
/// [`Batch::commit`], so the items' writes and how far the run got commit
/// together, and a run stopped anywhere resumes after the last batch that
/// committed, writing nothing twice. What the items emitted is left for
/// the runner once the batch commits.
#[derive(Debug)]
pub struct Batch<'c, 'p> {
    ctx: &'c mut Ctx<'p>,
    saved: (Option<crate::Caller>, Option<System>, bool),
}

impl<'c, 'p> Batch<'c, 'p> {
    /// Opens a batch in `store` at `isolation`, its items run as `system`.
    /// `ctx` must be running no action.
    pub async fn open(
        ctx: &'c mut Ctx<'p>,
        store: &str,
        isolation: Isolation,
        system: System,
    ) -> Result<Batch<'c, 'p>, StoreError> {
        if ctx.in_action() {
            return Err(StoreError::Fault(
                "a batch opened inside a running action".into(),
            ));
        }
        ctx.open_boundary(isolation);
        if let Err(error) = ctx.transaction(store).await {
            ctx.close_boundary();
            return Err(error);
        }
        let saved = ctx.runner();
        ctx.become_system(system);
        Ok(Batch { ctx, saved })
    }

    /// Opens a batch whose transaction begins at its first store operation
    /// rather than at once: one item whose action makes outside calls, a
    /// batch of its own (area 12, "How a run goes"; chunk 7.4b). What it
    /// reads before a call is a snapshot the call closes, and the run's
    /// own statements ([`Batch::writing`]) run in the transaction after
    /// it, with the item's writes. `ctx` must be running no action.
    pub fn open_for_calls(
        ctx: &'c mut Ctx<'p>,
        isolation: Isolation,
        system: System,
    ) -> Result<Batch<'c, 'p>, StoreError> {
        if ctx.in_action() {
            return Err(StoreError::Fault(
                "a batch opened inside a running action".into(),
            ));
        }
        ctx.open_boundary(isolation);
        let saved = ctx.runner();
        ctx.become_system(system);
        Ok(Batch { ctx, saved })
    }

    /// The batch's transaction in `store`, begun now if nothing has begun
    /// one, for the run's own statements, which write (chunk 7.4b).
    pub async fn writing(&mut self, store: &str) -> Result<&mut dyn Transaction, StoreError> {
        self.ctx.transaction(store).await
    }

    /// Runs `callable` for each of `items`, as [`items`] does.
    pub async fn items(
        &mut self,
        executor: &dyn Executor,
        callable: u32,
        list: Vec<Vec<Value>>,
        each: &mut dyn FnMut(usize, Outcome) -> ControlFlow<()>,
    ) -> usize {
        items(executor, self.ctx, callable, list, each).await
    }

    /// The context the batch runs in, for a read in its transaction (a
    /// batch run's page, chunk 6.4).
    pub fn ctx(&mut self) -> &mut Ctx<'p> {
        self.ctx
    }

    /// Whether an item or a read found a concurrent change: then nothing
    /// the batch did may be kept, and the run runs it again.
    pub fn changed(&self) -> bool {
        self.ctx
            .boundary()
            .is_some_and(crate::boundary::Boundary::changed)
    }

    /// The batch's transaction, for the run's own statements.
    pub fn transaction(&mut self) -> Option<&mut dyn Transaction> {
        self.ctx.boundary_mut()?.open_transaction()
    }

    /// Commits the batch: its items' writes and whatever the run wrote in
    /// its transaction, together. A concurrent change found by an item or
    /// at the commit is `ConcurrentChange`, and nothing is kept: the run
    /// runs the batch again.
    pub async fn commit(self) -> Result<(), StoreError> {
        let Batch { ctx, saved } = self;
        let written = crate::boundary::seal(ctx, None).await;
        let mut boundary = ctx.close_boundary();
        if let Err(error) = written {
            ctx.restore_runner(saved);
            if let Some(transaction) = boundary.take_transaction() {
                let _ = transaction.rollback().await;
            }
            return Err(error);
        }
        ctx.restore_runner(saved);
        let Some(transaction) = boundary.take_transaction() else {
            return Err(StoreError::Fault("a batch without its transaction".into()));
        };
        if boundary.changed() {
            let _ = transaction.rollback().await;
            return Err(StoreError::ConcurrentChange);
        }
        transaction.commit().await?;
        crate::boundary::committed(ctx, boundary.take_sealed());
        Ok(())
    }

    /// Gives the batch up: nothing it wrote is kept.
    pub async fn rollback(self) {
        let Batch { ctx, saved } = self;
        let mut boundary = ctx.close_boundary();
        ctx.restore_runner(saved);
        if let Some(transaction) = boundary.take_transaction() {
            let _ = transaction.rollback().await;
        }
    }
}

/// A backfill worked out in Jadpo (area 5, "Computed backfills"; chunk
/// 5.6b2), as the runner runs it: the callable working out one row's
/// value from the stored fields it reads, how each of their columns is
/// read, the column type its value is written as, and its field's
/// validator, through which each value is kept and checked as a value of
/// the field's type is (chunk 6.1a).
#[derive(Clone, Debug)]
pub struct Backfill<'v> {
    /// The migration's folder name.
    pub migration: String,
    /// `Todo.priority`.
    pub field: String,
    pub callable: u32,
    pub reads: Vec<jadpo_std::read::Column>,
    /// The field each value read is of and its validator, in the order of
    /// `reads`, which a value read is held to (chunk 5.9d): `None` for a
    /// field the migration retires.
    pub held: Vec<Option<(String, &'v Validator)>>,
    pub write: SqlType,
    /// The field's rules, its type's included: a decimal is kept at the
    /// type's places, rounded half to even, and a value its rules refuse
    /// stops the backfill.
    pub validator: Option<&'v Validator>,
}

/// Works out the latest migration's backfills that have no SQL form with
/// `executor` (chunk 5.6b2; moved into the runtime in chunk 6.1a), each
/// batch's rows on the runner's per-item mode: what `jadpo migrate apply`
/// hands [`crate::migrate_with`].
pub struct InJadpo<'e> {
    executor: &'e dyn Executor,
    backfills: Vec<Backfill<'e>>,
    /// The codecs a structured field's stored JSON is read and written
    /// with, and each such field the latest migration reads or writes, by
    /// migration and field, with its type's number (chunk 5.9c).
    json: Option<crate::JsonCodecs<'e>>,
    stored: Vec<StoredJson>,
}

/// A structured field the latest migration checks, rewrites or fills as
/// JSON (chunk 5.9c): its type's number in the JSON table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredJson {
    pub migration: String,
    /// `Parcel.label`.
    pub field: String,
    pub number: u32,
}

impl std::fmt::Debug for InJadpo<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InJadpo")
            .field("backfills", &self.backfills)
            .finish()
    }
}

impl<'e> InJadpo<'e> {
    pub fn new(executor: &'e dyn Executor, backfills: Vec<Backfill<'e>>) -> InJadpo<'e> {
        InJadpo {
            executor,
            backfills,
            json: None,
            stored: Vec::new(),
        }
    }

    /// With the codecs a structured field's stored JSON is read and
    /// written with, and the fields the latest migration reads or writes
    /// so (chunk 5.9c).
    pub fn with_json(mut self, codecs: crate::JsonCodecs<'e>, stored: Vec<StoredJson>) -> Self {
        self.json = Some(codecs);
        self.stored = stored;
        self
    }

    /// The codecs and the number a structured field's JSON is read and
    /// written with in `migration`.
    fn json_of(&self, migration: &str, field: &str) -> Option<(crate::JsonCodecs<'e>, u32)> {
        let number = self
            .stored
            .iter()
            .find(|stored| stored.migration == migration && stored.field == field)?
            .number;
        Some((self.json?, number))
    }
}

impl crate::Backfills for InJadpo<'_> {
    fn work_out(
        &self,
        migration: &str,
        field: &str,
        rows: &[&[SqlValue]],
    ) -> Result<Vec<SqlValue>, crate::Unworked> {
        use crate::Unworked;
        let backfill = self
            .backfills
            .iter()
            .find(|backfill| backfill.migration == migration && backfill.field == field)
            .ok_or(Unworked::Unavailable)?;
        let mut args = Vec::with_capacity(rows.len());
        for (at, row) in rows.iter().enumerate() {
            let stopped = |reason: String| Unworked::Stopped { at, reason };
            if row.len() != backfill.reads.len() {
                return Err(stopped("its row didn't hold every column it reads".into()));
            }
            let values = row
                .iter()
                .zip(&backfill.reads)
                .map(|(sql, column)| crate::from_sql(sql, column))
                .collect::<Result<Vec<_>, String>>()
                .map_err(|why| stopped(format!("a column it reads held {why}")))?;
            // Each stored value read keeps its field's rules, as a query's
            // read holds it to them (chunk 5.9d).
            for (value, held) in values.iter().zip(&backfill.held) {
                if let Some((field, validator)) = held {
                    crate::kept_by(field, validator, value)
                        .map_err(|why| stopped(format!("it read {why}")))?;
                }
            }
            args.push(values);
        }
        // A backfill is a function: it waits on nothing, so it runs to its
        // end here, on a host with no store.
        // A structured value is written through its type's route codec
        // (chunk 5.9c).
        let json = match backfill.write {
            SqlType::Json => Some(
                self.json_of(migration, field)
                    .ok_or(Unworked::Unavailable)?,
            ),
            _ => None,
        };
        let host = MemoryHost::new();
        let mut ctx = Ctx::of(self.executor, &host, Env::default());
        let mut out: Vec<SqlValue> = Vec::with_capacity(rows.len());
        let mut stop: Option<Unworked> = None;
        let places = self.executor.places();
        let ran = crate::block_on(items(
            self.executor,
            &mut ctx,
            backfill.callable,
            args,
            &mut |at, outcome| match written(backfill, outcome, places, json) {
                Ok(value) => {
                    out.push(value);
                    ControlFlow::Continue(())
                }
                Err(reason) => {
                    stop = Some(Unworked::Stopped { at, reason });
                    ControlFlow::Break(())
                }
            },
        ));
        if let Some(stop) = stop {
            return Err(stop);
        }
        if ran.is_err() || out.len() != rows.len() {
            return Err(Unworked::Stopped {
                at: out.len(),
                reason: "it waited for something a backfill never waits for".into(),
            });
        }
        Ok(out)
    }

    fn unreadable(
        &self,
        migration: &str,
        field: &str,
        values: &[&SqlValue],
    ) -> Result<Vec<Option<String>>, crate::Unworked> {
        let (codecs, number) = self
            .json_of(migration, field)
            .ok_or(crate::Unworked::Unavailable)?;
        Ok(values
            .iter()
            .map(|value| match value {
                SqlValue::Json(stored) => codecs.unread(number, stored),
                other => Some(format!("a `{:?}` column read as JSON", other.ty())),
            })
            .collect())
    }
}

/// The value a backfill's run gave, kept and checked as its field keeps
/// a value, bound for its column, through `json`'s codec for a structured
/// value (chunk 5.9c); why not, never the value.
fn written(
    backfill: &Backfill<'_>,
    outcome: Outcome,
    places: &crate::Places,
    json: Option<(crate::JsonCodecs<'_>, u32)>,
) -> Result<SqlValue, String> {
    let value = match outcome {
        Outcome::Value(value) => value,
        Outcome::Fault(fault) => {
            let place = fault
                .stack(places)
                .into_iter()
                .next()
                .map(|place| format!(" at {place}"))
                .unwrap_or_default();
            let what = match &fault.kind {
                FaultKind::Defect(what) | FaultKind::NotBuiltYet(what) | FaultKind::Store(what) => {
                    format!(" ({what})")
                }
                _ => String::new(),
            };
            return Err(format!(
                "it faulted with `{}`{what}{place}",
                fault.kind.classification()
            ));
        }
        Outcome::Failed(_) | Outcome::Assertion(_) => {
            return Err("it failed, which the checker rules out".into());
        }
    };
    let value = match backfill.validator {
        Some(validator) => {
            let kept = validator
                .store(value)
                .ok_or_else(|| "its value has more than 38 digits".to_string())?;
            validator
                .check(&kept)
                .map_err(|invalid| format!("its value breaks {}", invalid.broken(None)))?;
            kept
        }
        None => value,
    };
    match json {
        Some((codecs, number)) => codecs.write(number, &value),
        None => crate::to_sql(&value, backfill.write),
    }
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    use jadpo_std::policy::CallerPart;

    use super::*;
    use crate::{ActionBody, Caller, Database, Event, Failure, Places, Row, block_on};

    /// A message handed on names each piece by its record and store, in
    /// order (chunk 8.2h); anything else names nothing, so what it would
    /// have named is left for the tick.
    #[test]
    fn a_message_handed_on_names_each_piece_by_its_record() {
        let delivery = "0192f7a8c3d14e5f8a6b7c8d9e0f1a2b";
        let run = "00112233445566778899aabbccddeeff";
        let message = format!(
            r#"{{"handed_on":[{{"store":"main","delivery":"{delivery}"}},{{"store":"archive","batch":"{run}"}}]}}"#
        );
        let pieces = named(&message).expect("a message");
        assert!(matches!(
            pieces.as_slice(),
            [
                Named::Delivery { store, id },
                Named::Batch(crate::batches::BatchStart { store: batch_store, run: batch_run }),
            ] if store == "main"
                && crate::deliveries::hex(id) == delivery
                && batch_store == "archive"
                && crate::deliveries::hex(batch_run) == run
        ));
        for refused in [
            "",
            "{}",
            r#"{"handed_on":[{"delivery":"0192f7a8c3d14e5f8a6b7c8d9e0f1a2b"}]}"#,
            r#"{"handed_on":[{"store":"main","delivery":"0192f7a8"}]}"#,
            r#"{"handed_on":[{"store":"main","delivery":"zz92f7a8c3d14e5f8a6b7c8d9e0f1a2b"}]}"#,
            r#"{"handed_on":[{"store":"main","delivery":"0192f7a8c3d14e5f8a6b7c8d9e0f1a2b","batch":"00112233445566778899aabbccddeeff"}]}"#,
        ] {
            assert!(named(refused).is_none(), "{refused}");
        }
    }

    /// Work joins a runner only where its deployable runs it (chunk
    /// 8.1o2): a delivery where subscribers run, a batch run where
    /// schedules do.
    #[test]
    fn a_program_runs_only_the_background_work_its_deployable_runs() {
        let work = || {
            vec![
                Deferred::Deliveries(vec![crate::outbox::Delivery {
                    store: "main".to_string(),
                    id: [1; 16],
                    outbox: [2; 16],
                    subscriber: "subscriber pin_it".to_string(),
                    event: "type NoteAdded".to_string(),
                    value: Value::Unit,
                    emitted_at: 0,
                    trace: None,
                    depth: 0,
                }]),
                Deferred::Batch(crate::batches::BatchStart {
                    store: "main".to_string(),
                    run: [3; 16],
                }),
            ]
        };
        let kinds = |runs: Runs| {
            let mut queue = VecDeque::new();
            split(runs, work(), &mut queue);
            queue
                .iter()
                .map(|piece| match piece {
                    Piece::Delivery(_) => "delivery",
                    Piece::Batch(_) => "batch",
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(kinds(Runs::EVERYTHING), ["delivery", "batch"]);
        assert_eq!(
            kinds(Runs {
                schedules: true,
                subscribers: false
            }),
            ["batch"]
        );
        assert_eq!(
            kinds(Runs {
                schedules: false,
                subscribers: true
            }),
            ["delivery"]
        );
        assert!(
            kinds(Runs {
                schedules: false,
                subscribers: false
            })
            .is_empty()
        );
    }

    /// What reached the store, and in which order.
    #[derive(Default)]
    struct Log {
        lines: RefCell<Vec<String>>,
        /// Commits to come that find a concurrent change.
        changes: RefCell<u32>,
    }

    impl Log {
        fn line(&self, line: impl Into<String>) {
            self.lines.borrow_mut().push(line.into());
        }

        fn take(&self) -> Vec<String> {
            std::mem::take(&mut *self.lines.borrow_mut())
        }
    }

    struct Tx(Rc<Log>);

    impl Transaction for Tx {
        fn query<'a>(
            &'a mut self,
            sql: &'a str,
            _params: &'a [SqlValue],
        ) -> LocalFuture<'a, Result<Vec<Row>, StoreError>> {
            self.0.line(sql);
            Box::pin(async { Ok(Vec::new()) })
        }

        fn execute<'a>(
            &'a mut self,
            sql: &'a str,
            _params: &'a [SqlValue],
        ) -> LocalFuture<'a, Result<u64, StoreError>> {
            let line = match sql {
                crate::outbox::INSERT_OUTBOX => "outbox",
                crate::outbox::INSERT_DELIVERIES => "deliveries",
                other => other,
            };
            self.0.line(line);
            Box::pin(async { Ok(1) })
        }

        fn batch<'a>(&'a mut self, sql: &'a str) -> LocalFuture<'a, Result<(), StoreError>> {
            let line = match sql {
                crate::SAVEPOINT => "savepoint",
                crate::RELEASE_SAVEPOINT => "release",
                crate::ROLLBACK_TO_SAVEPOINT => "back to savepoint",
                other => other,
            };
            self.0.line(line);
            Box::pin(async { Ok(()) })
        }

        fn commit(self: Box<Self>) -> LocalFuture<'static, Result<(), StoreError>> {
            let log = Rc::clone(&self.0);
            Box::pin(async move {
                let mut changes = log.changes.borrow_mut();
                if *changes > 0 {
                    *changes -= 1;
                    drop(changes);
                    log.line("changed");
                    return Err(StoreError::ConcurrentChange);
                }
                drop(changes);
                log.line("commit");
                Ok(())
            })
        }

        fn rollback(self: Box<Self>) -> LocalFuture<'static, Result<(), StoreError>> {
            let log = Rc::clone(&self.0);
            Box::pin(async move {
                log.line("rollback");
                Ok(())
            })
        }
    }

    /// A host with one store that writes down what reaches it, a clock
    /// when given one, a budget and a monotonic clock that moves `step` on
    /// every look when given them, and the work it was given to run after
    /// a response or to hand on.
    #[derive(Default)]
    struct Fake {
        log: Rc<Log>,
        clock: Option<i64>,
        paced: Option<(Cell<u64>, u64)>,
        budget: Option<u64>,
        deferred: RefCell<Vec<LocalFuture<'static, ()>>>,
        handed: RefCell<Vec<LocalFuture<'static, ()>>>,
    }

    impl Database for Fake {
        fn begin_at(
            &self,
            isolation: Isolation,
        ) -> LocalFuture<'_, Result<Box<dyn Transaction>, StoreError>> {
            self.log.line(format!("begin {}", isolation.word()));
            let log = Rc::clone(&self.log);
            Box::pin(async move { Ok(Box::new(Tx(log)) as Box<dyn Transaction>) })
        }
    }

    impl Host for Fake {
        fn database(&self, _store: &str) -> Option<&dyn Database> {
            Some(self)
        }

        fn random(&self, bytes: &mut [u8]) -> bool {
            bytes.fill(7);
            true
        }

        fn now_ms(&self) -> Option<i64> {
            self.clock
        }

        fn defer(&self, work: LocalFuture<'static, ()>) {
            self.deferred.borrow_mut().push(work);
        }

        fn hand_on(&self, work: HandOn) {
            self.handed.borrow_mut().push(work.run());
        }

        fn monotonic_ms(&self) -> Option<u64> {
            let (now, step) = self.paced.as_ref()?;
            let then = now.get();
            now.set(then + step);
            Some(then)
        }

        fn budget_ms(&self, invocation: crate::budget::Invocation) -> u64 {
            let _ = invocation;
            self.budget.unwrap_or(crate::budget::GENEROUS_MS)
        }
    }

    /// What each callable does: an action that writes `write <name>` and
    /// what its caller is, then ends as it says, or a function.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Does {
        /// Writes, emits `type Wrote`, succeeds.
        Writes,
        /// Writes, emits, then fails `not_found`.
        Fails,
        /// A function giving its argument back.
        Echoes,
        /// Writes, emits this many events, and faults when one is refused.
        Emits(usize),
    }

    struct Program {
        places: Places,
        does: Vec<Does>,
    }

    impl Program {
        fn new(does: &[Does]) -> Program {
            Program {
                places: Places::new(),
                does: does.to_vec(),
            }
        }
    }

    struct Body {
        callable: u32,
        does: Does,
        args: Vec<Value>,
    }

    /// A delivery of an event of type `key` to `subscriber s`.
    fn delivery(key: &str) -> Delivery {
        Delivery {
            store: "main".into(),
            id: [1; 16],
            outbox: [2; 16],
            subscriber: "subscriber s".into(),
            event: key.into(),
            value: Value::Unit,
            emitted_at: Clock::TEST.now(),
            trace: None,
            depth: 0,
        }
    }

    /// The event types of the deliveries `work` holds, in order.
    fn deferred_events(work: Vec<Deferred>) -> Vec<String> {
        work.into_iter()
            .flat_map(|work| match work {
                Deferred::Deliveries(deliveries) => {
                    deliveries.into_iter().map(|d| d.event).collect()
                }
                Deferred::Batch(_) => Vec::new(),
            })
            .collect()
    }

    /// Who runs the action, as policy binds it.
    fn who(ctx: &Ctx<'_>) -> String {
        let system = ctx.caller_param(CallerPart::System) == SqlValue::Bool(true);
        let tenant = match ctx.caller_param(CallerPart::SystemTenant) {
            SqlValue::Uuid(tenant) => format!(" in {}", tenant[0]),
            _ => String::new(),
        };
        let signed = ctx.caller_param(CallerPart::Signed) == SqlValue::Bool(true);
        match (system, signed) {
            (true, _) => format!("system{tenant}"),
            (false, true) => "signed in".into(),
            (false, false) => "no one".into(),
        }
    }

    impl<'p> ActionBody<'p> for Body {
        fn run<'c>(&'c mut self, ctx: &'c mut Ctx<'p>) -> LocalFuture<'c, Outcome> {
            Box::pin(async move {
                let name = match self.args.first() {
                    Some(Value::Text(text)) => text.to_string(),
                    _ => format!("{}", self.callable),
                };
                let line = format!("write {name} as {}", who(ctx));
                let now = ctx.now();
                let Ok(transaction) = ctx.transaction("main").await else {
                    return Outcome::Value(Value::Unit);
                };
                let _ = transaction.execute(&line, &[]).await;
                if now != Clock::TEST.now() {
                    let _ = transaction.execute(&format!("at {now}"), &[]).await;
                }
                let emits = match self.does {
                    Does::Emits(count) => count,
                    _ => 1,
                };
                for _ in 0..emits {
                    let kept = ctx.emit(Event {
                        key: format!("type Wrote {name}"),
                        value: Value::Unit,
                        payload: "null".into(),
                        subscribers: vec!["subscriber s".into()],
                    });
                    if kept.is_err() && matches!(self.does, Does::Emits(_)) {
                        return Outcome::Fault(ctx.fault(
                            FaultKind::Defect("past the events one action emits".into()),
                            Frame::ENTRY,
                        ));
                    }
                }
                match self.does {
                    Does::Fails => {
                        let mut failure = Failure::new("type Gone", "Gone", "gone");
                        failure.kind = Some("not_found".into());
                        Outcome::Failed(Box::new(failure))
                    }
                    _ => Outcome::Value(Value::Unit),
                }
            })
        }

        fn isolation(&self) -> Isolation {
            Isolation::RepeatableRead
        }
    }

    impl Executor for Program {
        fn places(&self) -> &Places {
            &self.places
        }

        fn run<'a, 'p: 'a>(
            &'a self,
            ctx: &'a mut Ctx<'p>,
            callable: u32,
            args: Vec<Value>,
        ) -> LocalFuture<'a, Outcome> {
            let does = self
                .does
                .get(callable as usize)
                .copied()
                .unwrap_or(Does::Writes);
            Box::pin(async move {
                if does == Does::Echoes {
                    return Outcome::Value(args.into_iter().next().unwrap_or(Value::None));
                }
                let mut body = Body {
                    callable,
                    does,
                    args,
                };
                crate::action(ctx, &mut body).await
            })
        }
    }

    fn job(callable: u32, name: &str, tenant: Option<u8>) -> Job {
        Job {
            callable,
            args: vec![Value::text(name)],
            system: System {
                tenant: tenant.map(|byte| [byte; 16]),
            },
        }
    }

    fn signed_in(ctx: &mut Ctx<'_>) {
        ctx.set_caller(Some(Caller {
            value: Value::record(
                "Principal.user",
                vec![(
                    "id".into(),
                    Value::uuid("0192f0c4-3b5e-7c1a-9d2e-4f6a8b0c1d2e"),
                )],
            ),
            key: "user 1".into(),
            session: None,
        }));
    }

    /// A job runs as the system in a boundary of its own, limited to the
    /// tenant it names, and whoever ran the context runs it again after.
    #[test]
    fn a_job_runs_as_the_system_then_gives_the_caller_back() {
        let program = Program::new(&[Does::Writes, Does::Fails]);
        let host = Fake::default();
        let mut ctx = Ctx::new(&program.places, &host, Env::default());
        signed_in(&mut ctx);
        let wrote = block_on(as_system(&program, &mut ctx, job(0, "order", Some(9))))
            .expect("nothing waits");
        let refused = block_on(as_system(&program, &mut ctx, job(1, "refused", None)))
            .expect("nothing waits");
        assert_eq!(
            host.log.take(),
            [
                "begin repeatable_read",
                "write order as system in 9",
                "outbox",
                "deliveries",
                "commit",
                "begin repeatable_read",
                "write refused as system",
                "rollback",
            ]
        );
        assert!(matches!(wrote, Outcome::Value(_)));
        assert!(matches!(&refused, Outcome::Failed(failure) if failure.code == "gone"));
        assert_eq!(ctx.system(), None);
        assert!(ctx.caller().is_some(), "the request's caller again");
        assert_eq!(
            deferred_events(ctx.take_deferred()),
            ["type Wrote order"],
            "what the failed job emitted is undone with it"
        );
    }

    /// A route waiting for its follow-on work (chunk 6.3): work that leaves
    /// nothing to wait for has settled; a delivery that can't be claimed
    /// (this program has no subscribers) hasn't, and it and what follows
    /// are left for after the response; once `within:` has passed nothing
    /// more runs. Work left before isn't this route's to wait for, and the
    /// request's caller is given back for the answer.
    #[test]
    fn waiting_for_follow_on_work_stops_at_what_has_not_settled() {
        let program = Program::new(&[Does::Writes]);
        let host = Fake::default();
        let mut ctx = Ctx::new(&program.places, &host, Env::default());
        signed_in(&mut ctx);
        let earlier = Deferred::Deliveries(vec![delivery("type Earlier")]);
        ctx.defer(earlier.clone());
        let mut within: LocalFuture<'_, ()> = Box::pin(std::future::pending());
        let settled = block_on(settle_within(&program, &mut ctx, Vec::new(), &mut within))
            .expect("nothing waits");
        assert_eq!(settled, Settled::Settled);
        assert_eq!(ctx.take_deferred(), [earlier]);
        let work = vec![Deferred::Deliveries(vec![
            delivery("type Placed"),
            delivery("type Paid"),
        ])];
        let settled =
            block_on(settle_within(&program, &mut ctx, work, &mut within)).expect("nothing waits");
        assert_eq!(settled, Settled::Unsettled);
        assert_eq!(
            deferred_events(ctx.take_deferred()),
            ["type Placed", "type Paid"]
        );
        assert!(ctx.caller().is_some(), "the request's caller again");
        assert_eq!(ctx.system(), None);
        // Once `within:` has passed, nothing more runs.
        let mut passed: LocalFuture<'_, ()> = Box::pin(async {});
        let settled = block_on(settle_within(
            &program,
            &mut ctx,
            vec![Deferred::Deliveries(vec![delivery("type Late")])],
            &mut passed,
        ))
        .expect("nothing waits");
        assert_eq!(settled, Settled::Unsettled);
        assert!(host.log.take().is_empty());
        assert_eq!(deferred_events(ctx.take_deferred()), ["type Late"]);
        assert!(ctx.caller().is_some());
    }

    /// Deferred: the host is given the work to run once the response is
    /// handed back, and nothing runs before (chunk 6.1a). Run, it works
    /// within the budget its host gives it, and once that is spent hands
    /// what it didn't reach on into a fresh invocation, which carries it
    /// to its end (chunk 6.7); an invocation that could run nothing hands
    /// nothing on, and a host with no clock runs nothing.
    #[test]
    fn deferred_work_runs_after_the_response_and_hands_on_what_its_budget_left() {
        let program: Rc<dyn Executor> = Rc::new(Program::new(&[Does::Writes]));
        // Ten seconds, and each look at the clock takes five.
        let fake = Rc::new(Fake {
            clock: Some(1_800_000_000_000),
            paced: Some((Cell::new(0), 5_000)),
            budget: Some(10_000),
            ..Fake::default()
        });
        let host: Rc<dyn Host> = Rc::clone(&fake) as Rc<dyn Host>;
        after_response(Rc::clone(&program), Rc::clone(&host), Vec::new());
        assert!(
            fake.deferred.borrow().is_empty(),
            "nothing to run, nothing handed over"
        );
        let work = vec![Deferred::Deliveries(vec![
            delivery("type First"),
            delivery("type Second"),
        ])];
        after_response(Rc::clone(&program), Rc::clone(&host), work);
        assert_eq!(fake.deferred.borrow().len(), 1);
        assert!(fake.handed.borrow().is_empty());
        let work: Vec<_> = fake.deferred.borrow_mut().drain(..).collect();
        for work in work {
            block_on(work).expect("nothing waits");
        }
        // It took the first delivery; then its budget was spent, and the
        // second went on into a fresh invocation, which took it and left
        // nothing.
        let handed: Vec<_> = fake.handed.borrow_mut().drain(..).collect();
        assert_eq!(handed.len(), 1);
        for work in handed {
            block_on(work).expect("nothing waits");
        }
        assert!(fake.handed.borrow().is_empty(), "nothing more to hand on");
        // A budget spent before it could take anything hands nothing on.
        let spent = Rc::new(Fake {
            clock: Some(1_800_000_000_000),
            paced: Some((Cell::new(0), 10_000)),
            budget: Some(10_000),
            ..Fake::default()
        });
        after_response(
            Rc::clone(&program),
            Rc::clone(&spent) as Rc<dyn Host>,
            vec![Deferred::Deliveries(vec![delivery("type Placed")])],
        );
        let work: Vec<_> = spent.deferred.borrow_mut().drain(..).collect();
        for work in work {
            block_on(work).expect("nothing waits");
        }
        assert!(spent.handed.borrow().is_empty());
        // A host with no clock runs nothing.
        let clockless = Fake::default();
        let outcomes = block_on(run_deferred(
            program.as_ref(),
            &clockless,
            vec![Deferred::Deliveries(vec![delivery("type Placed")])],
        ))
        .expect("nothing waits");
        assert!(outcomes.is_empty(), "{outcomes:?}");
        assert!(clockless.log.take().is_empty());
    }

    /// Per item, in a batch: one transaction for the batch, each item's
    /// action joining it and undone alone when it fails, the run's position
    /// written in it before it commits, and only then what the items that
    /// stood emitted left for the runner.
    #[test]
    fn a_batch_commits_its_items_and_position_together_and_undoes_a_failed_item_alone() {
        let program = Program::new(&[Does::Writes, Does::Fails]);
        let host = Fake::default();
        let mut ctx = Ctx::new(&program.places, &host, Env::default());
        let mut seen = Vec::new();
        let ran = block_on(async {
            let mut batch = Batch::open(
                &mut ctx,
                "main",
                Isolation::ReadCommitted,
                System::default(),
            )
            .await
            .expect("opens");
            let ran = batch
                .items(
                    &program,
                    0,
                    vec![vec![Value::text("a")], vec![Value::text("b")]],
                    &mut |at, outcome| {
                        seen.push((at, matches!(outcome, Outcome::Value(_))));
                        ControlFlow::Continue(())
                    },
                )
                .await;
            let failed = batch
                .items(
                    &program,
                    1,
                    vec![vec![Value::text("c")]],
                    &mut |at, outcome| {
                        seen.push((at + 2, matches!(outcome, Outcome::Value(_))));
                        ControlFlow::Continue(())
                    },
                )
                .await;
            let transaction = batch.transaction().expect("open");
            let _ = transaction.execute("position 3", &[]).await;
            batch.commit().await.expect("commits");
            ran + failed
        })
        .expect("nothing waits");
        assert_eq!(ran, 3);
        assert_eq!(seen, [(0, true), (1, true), (2, false)]);
        assert_eq!(
            host.log.take(),
            [
                "begin read_committed",
                "savepoint",
                "write a as system",
                "release",
                "savepoint",
                "write b as system",
                "release",
                "savepoint",
                "write c as system",
                "back to savepoint",
                "position 3",
                "outbox",
                "deliveries",
                "commit",
            ]
        );
        assert_eq!(
            deferred_events(ctx.take_deferred()),
            ["type Wrote a", "type Wrote b"],
            "the failed item's event is undone with it"
        );
        assert_eq!(ctx.system(), None, "the batch's caller is given back");
        // A concurrent change at the commit keeps nothing, for the run to
        // run the batch again.
        *host.log.changes.borrow_mut() = 1;
        let again = block_on(async {
            let mut batch = Batch::open(
                &mut ctx,
                "main",
                Isolation::ReadCommitted,
                System::default(),
            )
            .await
            .expect("opens");
            batch
                .items(&program, 0, vec![vec![Value::text("d")]], &mut |_, _| {
                    ControlFlow::Continue(())
                })
                .await;
            batch.commit().await
        })
        .expect("nothing waits");
        assert_eq!(again, Err(StoreError::ConcurrentChange));
        assert!(ctx.take_deferred().is_empty());
    }

    /// The 1,000 events one action may emit are each item's action's own,
    /// not shared by the page its batch is (chunk 6.6a): two items of 600
    /// each both stand, while one item past 1,000 is still refused.
    #[test]
    fn each_items_action_has_its_own_allowance_of_events() {
        let program = Program::new(&[Does::Emits(600), Does::Emits(1_001)]);
        let host = Fake::default();
        let mut ctx = Ctx::new(&program.places, &host, Env::default());
        let mut seen = Vec::new();
        block_on(async {
            let mut batch = Batch::open(
                &mut ctx,
                "main",
                Isolation::ReadCommitted,
                System::default(),
            )
            .await
            .expect("opens");
            for (callable, name) in [(0, "a"), (0, "b"), (1, "c"), (0, "d")] {
                batch
                    .items(
                        &program,
                        callable,
                        vec![vec![Value::text(name)]],
                        &mut |_, outcome| {
                            seen.push((name, matches!(outcome, Outcome::Value(_))));
                            ControlFlow::Continue(())
                        },
                    )
                    .await;
            }
            batch.commit().await.expect("commits");
        })
        .expect("nothing waits");
        assert_eq!(
            seen,
            [("a", true), ("b", true), ("c", false), ("d", true)],
            "only the item past 1,000 of its own is refused"
        );
        assert_eq!(deferred_events(ctx.take_deferred()).len(), 1_800);
    }

    /// Per item, outside a batch: each item runs in order until told to
    /// stop, as a backfill's rows do.
    #[test]
    fn items_run_in_order_until_told_to_stop() {
        let program = Program::new(&[Does::Echoes]);
        let host = Fake::default();
        let mut ctx = Ctx::new(&program.places, &host, Env::default());
        let mut got = Vec::new();
        let ran = block_on(items(
            &program,
            &mut ctx,
            0,
            (1..=5).map(|n| vec![Value::Int(n)]).collect(),
            &mut |_, outcome| {
                got.push(outcome);
                if got.len() == 3 {
                    ControlFlow::Break(())
                } else {
                    ControlFlow::Continue(())
                }
            },
        ))
        .expect("nothing waits");
        assert_eq!(ran, 3);
        assert_eq!(got, [1, 2, 3].map(|n| Outcome::Value(Value::Int(n))),);
        assert!(host.log.take().is_empty(), "a function reaches no store");
    }

    /// A backfill worked out in Jadpo keeps and checks each value through
    /// its field's validator, as a value of the field's type is kept: a
    /// decimal at the type's places, rounded half to even, and one its
    /// rules refuse stops it, naming the rule, never the value.
    #[test]
    fn a_backfill_writes_each_value_through_its_fields_validator() {
        use crate::{Backfills, Unworked};
        let program = Program::new(&[Does::Echoes]);
        let cost = Validator::new(
            None,
            vec![jadpo_std::Rules {
                scale: Some(2),
                max: jadpo_std::Num::parse("100"),
                ..jadpo_std::Rules::default()
            }],
        );
        let in_jadpo = InJadpo::new(
            &program,
            vec![Backfill {
                migration: "20261019T1200_add_cost".into(),
                field: "Order.cost".into(),
                callable: 0,
                reads: vec![jadpo_std::read::Column::Decimal],
                held: vec![None],
                write: SqlType::Numeric,
                validator: Some(&cost),
            }],
        );
        let rows: Vec<Vec<SqlValue>> = ["2.345", "2.355", "7"]
            .iter()
            .map(|n| vec![SqlValue::Numeric((*n).into())])
            .collect();
        let rows: Vec<&[SqlValue]> = rows.iter().map(Vec::as_slice).collect();
        assert_eq!(
            in_jadpo.work_out("20261019T1200_add_cost", "Order.cost", &rows),
            Ok(vec![
                SqlValue::Numeric("2.34".into()),
                SqlValue::Numeric("2.36".into()),
                SqlValue::Numeric("7.00".into()),
            ])
        );
        let over = [SqlValue::Numeric("1.00".into())];
        let past = [SqlValue::Numeric("100.004".into())];
        let broken = [SqlValue::Numeric("100.01".into())];
        assert_eq!(
            in_jadpo.work_out(
                "20261019T1200_add_cost",
                "Order.cost",
                &[&over, &past, &broken]
            ),
            Err(Unworked::Stopped {
                at: 2,
                reason: "its value breaks `max: 100`".into()
            }),
            "100.004 is kept as 100.00, which its rules allow"
        );
        assert_eq!(
            in_jadpo.work_out("20261019T1200_add_cost", "Order.total", &[&over]),
            Err(Unworked::Unavailable)
        );
    }

    /// Each stored value a backfill worked out in Jadpo reads is held to
    /// its field's rules, as a query's read is (chunk 5.9d): a row written
    /// around Jadpo that breaks one stops it, naming the field and the
    /// rule, never the value.
    #[test]
    fn a_backfill_reading_a_value_that_breaks_its_rule_stops() {
        use crate::{Backfills, Unworked};
        let program = Program::new(&[Does::Echoes]);
        let price = Validator::new(
            Some(jadpo_std::Builtin::Decimal),
            vec![jadpo_std::Rules {
                max: jadpo_std::Num::parse("100"),
                ..jadpo_std::Rules::default()
            }],
        );
        let in_jadpo = InJadpo::new(
            &program,
            vec![Backfill {
                migration: "20261019T1200_add_cost".into(),
                field: "Order.cost".into(),
                callable: 0,
                reads: vec![jadpo_std::read::Column::Decimal],
                held: vec![Some(("Order.price".into(), &price))],
                write: SqlType::Numeric,
                validator: None,
            }],
        );
        let kept = [SqlValue::Numeric("12.50".into())];
        let broken = [SqlValue::Numeric("123.45".into())];
        let stopped = in_jadpo.work_out("20261019T1200_add_cost", "Order.cost", &[&kept, &broken]);
        assert_eq!(
            stopped,
            Err(Unworked::Stopped {
                at: 1,
                reason: "it read a stored `Order.price` that breaks `max: 100`".into()
            })
        );
    }
}
