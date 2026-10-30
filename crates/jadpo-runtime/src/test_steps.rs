//! What a test's steps emit, delivered between them (area 13, "Caches and
//! events"; area 12, question 15; chunk 6.11). `jadpo test` turns it on
//! for a test with a database ([`Ctx::deliver_between_steps`]), and the
//! executor calls [`after_step`] each time a test's step comes back: a
//! call of an action or fixture, a route test's request, a schedule's
//! run, or `clock.advance`. It delivers what is due in the test's outbox
//! at the test's clock, as the runner delivers any delivery
//! ([`crate::runner`]), as the system limited to its event's tenant,
//! then what those deliveries' subscribers emit in turn, until none is
//! due, before the test's next statement runs.
//!
//! - **Order.** Each delivery is drawn from all those due at that moment
//!   in an order the run's printed seed decides, mixed with the test's
//!   own identity, so a test that relies on the order subscribers run in
//!   fails under some seed, and `--seed` reproduces it.
//! - **Retries.** A delivery whose subscriber fails in a way worth trying
//!   again waits its backoff in full, with no jitter (30 seconds, then a
//!   minute, and so on), as the test's host has no randomness: the
//!   test's clock doesn't move by itself, so it is tried again once
//!   `clock.advance` reaches it.
//! - **Failing for good.** A delivery that fails for good, a failure not
//!   worth retrying, its last attempt, or a fault, ends the test as
//!   `test.subscriber_failed` with that failure and the delivery it
//!   happened in ([`Undelivered`]); so does one that stops as `diverged`,
//!   held for an operator (chunk 7.5d). What a route's `settle:` delivers
//!   while its request waits is the request's, and its response says how
//!   it ended.
//!
//! - **Crashes.** A delivery `jadpo test` crashed at one of its steps
//!   ([`crate::crashes`]) is given back, due at once, and delivered again
//!   as the next draw, each attempt counting its keyed calls from none
//!   under the delivery's key, as a fresh runner's does.
//! - **Batches.** A batch run a step starts, or a delivery or an item
//!   starts in turn, runs to its end among the deliveries (area 13,
//!   "Batches"): a batch at a time, each drawn as a delivery is, at the
//!   test's clock, by a fresh runner as after a restart
//!   ([`crate::batches::run_a_batch`]), two items a batch on a test's
//!   host. An item that fails is the run's problem, and a `finished:`
//!   that fails leaves the run `failed`, which `batches.status` reads; the
//!   test goes on. A run whose batch couldn't commit waits, as it would
//!   for the tick, until the test's clock passes its lease.

use std::collections::BTreeSet;

use crate::batches::BatchStart;
use crate::budget::{Budget, Invocation};
use crate::outbox::{DIVERGED, Delivery, FAILED};
use crate::runner::Deferred;
use crate::test_run::AtClock;
use jadpo_value::Value;

use crate::{Ctx, Executor, Fault, FaultKind, Outcome, SqlValue};

/// A test's delivery between its steps: the state of the order its
/// deliveries are drawn in, the delivery that failed for good, once one
/// has, and how many random draws its batch runs have made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BetweenSteps {
    order: u64,
    undelivered: Option<Undelivered>,
    drawn: u64,
}

impl BetweenSteps {
    /// The next draw (SplitMix64).
    fn draw(&mut self) -> u64 {
        self.order = self.order.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.order;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
}

/// The delivery that failed for good between a test's steps, which ended
/// the test (`test.subscriber_failed`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Undelivered {
    /// The event's type, as written.
    pub event: String,
    /// The subscriber, as written.
    pub subscriber: String,
    /// The attempt that failed, from 1.
    pub attempt: u32,
    /// How many its `retry:` allows.
    pub attempts: u32,
    /// Whether it stopped as `diverged`, its rerun no longer matching the
    /// record of its steps, rather than failing (chunk 7.5d): held for an
    /// operator, it never finishes in the test.
    pub diverged: bool,
}

impl Ctx<'_> {
    /// Delivers what each of the test's steps emits before its next
    /// statement ([`after_step`]), drawing each delivery in the order
    /// `order` decides: what `jadpo test` turns on for a test with a
    /// database.
    pub fn deliver_between_steps(&mut self, order: u64) {
        self.background_mut().set_between_steps(BetweenSteps {
            order,
            undelivered: None,
            drawn: 0,
        });
    }

    /// The delivery that failed for good between the test's steps, which
    /// ended it; `None` when none did.
    pub fn undelivered(&self) -> Option<&Undelivered> {
        self.background()
            .between_steps()
            .and_then(|between| between.undelivered.as_ref())
    }
}

/// `clock.advance(by)` in a test (area 13, "Controlling time"): its one
/// argument, a duration, moves the test's clock on, never back
/// (`clock_backward`), at site `at`. A step, after which what has come
/// due is delivered ([`after_step`]).
pub fn advance_clock(ctx: &mut Ctx<'_>, args: &[Value], at: u32) -> Result<(), Box<Fault>> {
    let [Value::Duration(by)] = args else {
        return Err(ctx.fault(
            FaultKind::Defect("a clock moved by something that isn't a duration".into()),
            at,
        ));
    };
    if !ctx.advance_clock(*by) {
        return Err(ctx.fault(FaultKind::ClockBackward, at));
    }
    Ok(())
}

/// The name `key`, an identity key, is written with: `RepairBooked` for
/// `type cafe.RepairBooked`.
fn written(key: &str) -> &str {
    let name = key.split_once(' ').map_or(key, |(_, name)| name);
    name.rsplit('.').next().unwrap_or(name)
}

/// One piece of what runs between a test's steps.
enum Piece {
    Delivery(Delivery),
    Batch(BatchStart),
}

/// Puts `work`, what a step, a delivery or a batch left, among `pool`'s
/// pieces: each delivery, and each batch run not there already.
fn pool_up(pool: &mut Vec<Piece>, work: Vec<Deferred>) {
    for work in work {
        match work {
            Deferred::Deliveries(deliveries) => {
                pool.extend(deliveries.into_iter().map(Piece::Delivery));
            }
            Deferred::Batch(start) => add_run(pool, start),
        }
    }
}

/// Puts the run `start` names among `pool`'s pieces, unless it is there.
fn add_run(pool: &mut Vec<Piece>, start: BatchStart) {
    let there = pool
        .iter()
        .any(|piece| matches!(piece, Piece::Batch(held) if held.run == start.run));
    if !there {
        pool.push(Piece::Batch(start));
    }
}

/// Delivers what is due in the test's outbox once a step of it has come
/// back, and runs the batch runs it started, and what those set off,
/// until none is due (see the module): when the step left deliveries or
/// batch runs, or moved the clock (`advanced`), so a delivery waiting to
/// retry, or a run waiting for its lease to lapse, may be due. Nothing
/// when `ctx` isn't a test's that delivers between its steps, or is
/// inside an action. The outcome to end the test with when a delivery
/// failed for good, which [`Ctx::undelivered`] then names, the test's
/// frame at `at`, the step's site.
pub async fn after_step(
    executor: &dyn Executor,
    ctx: &mut Ctx<'_>,
    advanced: bool,
    at: u32,
) -> Result<(), Outcome> {
    if ctx.background().between_steps().is_none() || ctx.in_action() {
        return Ok(());
    }
    let events = executor
        .plan()
        .events()
        .filter(|events| !events.plan.subscribers.is_empty());
    let host = ctx.host();
    let mut pool: Vec<Piece> = Vec::new();
    let mut emitted = false;
    for work in ctx.take_deferred() {
        match work {
            // Read again from the outbox, with any older one now due.
            Deferred::Deliveries(deliveries) => emitted |= !deliveries.is_empty(),
            Deferred::Batch(start) => add_run(&mut pool, start),
        }
    }
    // The tick picks up a run whose lease lapsed as the clock moved.
    if advanced {
        let clocked = AtClock::new(host, ctx.now());
        for start in crate::batches::waiting(executor, &clocked, ctx.now()).await {
            add_run(&mut pool, start);
        }
    }
    if pool.is_empty() && (events.is_none() || !emitted && !advanced) {
        return Ok(());
    }
    let stores = executor
        .plan()
        .schedules()
        .map(|plan| plan.stores.clone())
        .unwrap_or_default();
    let budget = Budget::start(host, Invocation::AfterResponse);
    // Deliveries this runner couldn't claim, never drawn again.
    let mut unclaimed: BTreeSet<[u8; 16]> = BTreeSet::new();
    loop {
        let delivering = pool.iter().any(|piece| matches!(piece, Piece::Delivery(_)));
        if let Some(events) = &events
            && !delivering
        {
            for store in &stores {
                match crate::runner::due(events, host, store, ctx.now()).await {
                    Ok(due) => pool.extend(
                        due.into_iter()
                            .filter(|delivery| !unclaimed.contains(&delivery.id))
                            .map(Piece::Delivery),
                    ),
                    Err(error) => {
                        return Err(Outcome::Fault(ctx.fault(
                            FaultKind::Defect(format!(
                                "the test's outbox in `{store}` couldn't be read: {error:?}"
                            )),
                            at,
                        )));
                    }
                }
            }
        }
        if pool.is_empty() {
            return Ok(());
        }
        let draw = ctx
            .background_mut()
            .between_steps_mut()
            .map_or(0, BetweenSteps::draw);
        match pool.remove((draw % pool.len() as u64) as usize) {
            Piece::Delivery(delivery) => {
                let Some(events) = &events else {
                    continue;
                };
                // Each attempt counts its keyed calls from none under the
                // delivery's own key, as a runner's fresh context does, so
                // an attempt after a crash sends the keys the first one
                // sent (chunk 7.5d); the test's own are given back after
                // (the attempt's own doing since chunk 7.9a).
                let attempted =
                    crate::runner::attempt_delivery(executor, ctx, &budget, delivery.clone(), None)
                        .await;
                match attempted {
                    None => {
                        unclaimed.insert(delivery.id);
                    }
                    Some((mut outcome, state)) if state == FAILED || state == DIVERGED => {
                        let mut undelivered = undelivered(events, host, &delivery).await;
                        undelivered.diverged = state == DIVERGED;
                        if let Some(between) = ctx.background_mut().between_steps_mut() {
                            between.undelivered = Some(undelivered);
                        }
                        // Its subscriber's action ran from the test, which
                        // stood at the step.
                        ctx.at(at);
                        let frames = match &mut outcome {
                            Outcome::Failed(failure) => Some(&mut failure.frames),
                            Outcome::Fault(fault) => Some(&mut fault.frames),
                            Outcome::Assertion(assertion) => Some(&mut assertion.frames),
                            Outcome::Value(_) => None,
                        };
                        if let Some(test) = frames.and_then(|frames| frames.last_mut()) {
                            test.site = at;
                        }
                        return Err(outcome);
                    }
                    Some(_) => {}
                }
                // What its subscriber's action emitted or started, due now
                // too.
                let work = ctx.take_deferred();
                pool_up(&mut pool, work);
            }
            Piece::Batch(start) => {
                // A batch at a time, by a fresh runner each, at the test's
                // clock, with randomness that carries on from the last.
                let drawn = ctx
                    .background()
                    .between_steps()
                    .map_or(0, |between| between.drawn);
                let clocked = AtClock::new(host, ctx.now()).after(drawn);
                let batch_budget = Budget::start(&clocked, Invocation::AfterResponse);
                let (_, left) = Box::pin(crate::batches::run_a_batch(
                    executor,
                    &clocked,
                    &batch_budget,
                    &start,
                ))
                .await;
                if let Some(between) = ctx.background_mut().between_steps_mut() {
                    between.drawn = clocked.drawn();
                }
                // A run handed on is among what it left; one that ended,
                // or that waits for its lease to lapse, isn't.
                pool_up(&mut pool, left);
            }
        }
    }
}

/// What a test's report says of `delivery`, which failed for good: its
/// event and subscriber as written, and the attempt it failed on, as its
/// record keeps it.
async fn undelivered(
    events: &crate::outbox::Events<'_>,
    host: &dyn crate::Host,
    delivery: &Delivery,
) -> Undelivered {
    let subscriber = events.plan.subscriber(&delivery.subscriber);
    let attempt = match host.database(&delivery.store) {
        Some(database) => database
            .query_once(
                "SELECT \"attempts\" FROM \"jadpo_deliveries\" WHERE \"id\" = $1",
                &[SqlValue::Uuid(delivery.id)],
            )
            .await
            .ok()
            .and_then(|rows| match rows.first().and_then(|row| row.first()) {
                Some(SqlValue::Int4(n)) => u32::try_from(*n).ok(),
                _ => None,
            }),
        None => None,
    };
    Undelivered {
        event: written(&delivery.event).to_string(),
        subscriber: subscriber.map_or_else(
            || written(&delivery.subscriber).to_string(),
            |plan| plan.name.clone(),
        ),
        attempt: attempt.unwrap_or(1).max(1),
        attempts: subscriber.map_or(1, |plan| plan.attempts),
        diverged: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_seed_draws_the_same_order() {
        let draws = |order| {
            let mut between = BetweenSteps {
                order,
                undelivered: None,
                drawn: 0,
            };
            (0..8).map(|_| between.draw() % 5).collect::<Vec<_>>()
        };
        assert_eq!(draws(7), draws(7));
        assert_ne!(draws(7), draws(8));
    }

    #[test]
    fn an_identity_key_is_shown_as_written() {
        assert_eq!(written("type cafe.RepairBooked"), "RepairBooked");
        assert_eq!(written("type RepairBooked"), "RepairBooked");
        assert_eq!(written("subscriber find_a_bench"), "find_a_bench");
    }
}
