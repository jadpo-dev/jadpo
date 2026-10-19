//! The runner (docs/compiler-runtime.md §2.6, chunk 6.1a): the one place
//! the runtime drives an executor for work no request is waiting on, so
//! the interpreter now and compiled code later run it the same way.
//!
//! It runs work three ways:
//!
//! - **Deferred**, after the response that set it off is sent: what an
//!   action left once it committed ([`Ctx::defer`]) goes to the host with
//!   [`after_response`], which runs it with [`run_deferred`] through
//!   [`Host::defer`] (Workers' `waitUntil`, natively a task on the
//!   worker's thread), each run in a context of its own under the host's
//!   clock and the 15-minute limit of one run (area 12, "Limits").
//! - **Inline**, in the invocation that set it off, with [`settle`]: what a
//!   route that waits for its follow-on work runs (area 7, `settle:`;
//!   chunk 6.3), and a test's delivery after each commit (area 13).
//! - **Per item**, with [`items`]: a callable run once for each of many
//!   items, in order, each one undone alone when it fails inside a batch
//!   ([`Batch`], a transaction of its own committing the items' writes and
//!   the run's position together, so a run resumes after the last batch
//!   that committed). A backfill worked out in Jadpo runs its rows this
//!   way ([`InJadpo`]), and batches (area 12, chunk 6.4) will.
//!
//! Work runs as the system ([`System`]), the caller `Access.system` grants
//! (area 9): a schedule's run, or a subscriber's delivery limited to the
//! tenant its event came from. Only the runner makes it, and the HTTP
//! component never calls the runner with a request's context, so no
//! request can act as the system.
//!
//! What it runs is a [`Job`], or a [`Delivery`] of an event a committed
//! action emitted to one of its subscribers ([`Deferred::Deliveries`],
//! chunk 6.1b), which [`deliver`] claims, runs as the system limited to
//! the event's tenant, and records as succeeded, retried later or failed
//! ([`crate::outbox`]); [`sweep`] delivers what deferred work dropped.
//! Schedules (chunk 6.2) hand their runs here on each tick, and the tick
//! calls [`sweep`].

use std::collections::VecDeque;
use std::ops::ControlFlow;
use std::rc::Rc;

use jadpo_value::{Validator, Value};

use crate::ctx::System;
pub use crate::outbox::Delivery;
use crate::race::{First, race};
use crate::{
    Clock, Ctx, Env, Executor, FaultKind, Frame, Host, Isolation, MemoryHost, Outcome, SqlType,
    SqlValue, StoreError, Transaction,
};

/// The longest one run of a subscriber or a schedule takes before it is
/// stopped as timed out (area 12, "Limits and runtime faults").
pub const RUN_LIMIT_MS: u64 = 15 * 60 * 1000;

/// A run of callable `callable` with `args`, as the system: a subscriber's
/// delivery or a schedule's run (area 12), in a boundary of its own.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Job {
    pub callable: u32,
    pub args: Vec<Value>,
    pub system: System,
}

/// Work left for after a response (chunk 6.1a).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Deferred {
    /// The deliveries of what an action emitted, once it committed, in
    /// the order it emitted it: each event to each of its subscribers
    /// (chunk 6.1b).
    Deliveries(Vec<Delivery>),
    /// A run of a callable as the system.
    Run(Job),
}

/// Runs `work`, and the work it sets off in turn, in `ctx`, the
/// invocation that left it, before that invocation answers: inline, as a
/// route that waits for its follow-on work does (chunk 6.3). Each job runs
/// as the system in a boundary of its own; whoever ran `ctx` before runs
/// it again after. The outcome of each job, in the order they ran.
pub async fn settle(
    executor: &dyn Executor,
    ctx: &mut Ctx<'_>,
    work: Vec<Deferred>,
) -> Vec<Outcome> {
    let mut queue = VecDeque::from(work);
    let mut outcomes = Vec::new();
    while let Some(next) = queue.pop_front() {
        let before = ctx.deferred.len();
        match next {
            Deferred::Run(job) => outcomes.push(as_system(executor, ctx, job).await),
            Deferred::Deliveries(deliveries) => {
                for delivery in deliveries {
                    outcomes.extend(deliver(executor, ctx, delivery).await);
                }
            }
        }
        let after = before.min(ctx.deferred.len());
        queue.extend(ctx.deferred.split_off(after));
    }
    outcomes
}

/// Runs `job` in `ctx` as the system, then puts back whoever ran `ctx`.
async fn as_system(executor: &dyn Executor, ctx: &mut Ctx<'_>, job: Job) -> Outcome {
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

/// Runs `work` as a deployable does after the response is sent: each job
/// and delivery in a context of its own on `host`, under the host's clock
/// and no seed, as the system, within [`RUN_LIMIT_MS`], and then the work
/// it set off. A host with no clock runs nothing: each job is a fault
/// saying so, and each delivery is left for the sweep. The outcome of
/// each job and delivery, in the order they ran; a delivery's record
/// keeps it too (chunk 6.1b).
pub async fn run_deferred(
    executor: &dyn Executor,
    host: &dyn Host,
    work: Vec<Deferred>,
) -> Vec<Outcome> {
    drive(executor, host, work, None).await
}

/// One piece of deferred work, run on its own.
enum Piece {
    Job(Job),
    Delivery(Delivery),
}

/// Runs `work` as [`run_deferred`] does, at `now` when it is given and
/// at the host's time otherwise.
async fn drive(
    executor: &dyn Executor,
    host: &dyn Host,
    work: Vec<Deferred>,
    now: Option<i64>,
) -> Vec<Outcome> {
    let mut queue: VecDeque<Piece> = VecDeque::new();
    let split = |work: Vec<Deferred>, queue: &mut VecDeque<Piece>| {
        for next in work {
            match next {
                Deferred::Run(job) => queue.push_back(Piece::Job(job)),
                Deferred::Deliveries(deliveries) => {
                    queue.extend(deliveries.into_iter().map(Piece::Delivery));
                }
            }
        }
    };
    split(work, &mut queue);
    let mut outcomes = Vec::new();
    while let Some(piece) = queue.pop_front() {
        let Some(now) = now.or_else(|| host.now_ms()) else {
            if let Piece::Job(_) = piece {
                outcomes.push(Outcome::Fault(Box::new(crate::Fault {
                    kind: FaultKind::Defect("the host has no clock".into()),
                    frames: Vec::new(),
                })));
            }
            continue;
        };
        let env = Env {
            seed: None,
            clock: Clock::fixed(now),
        };
        let mut ctx = Ctx::new(executor.places(), host, env);
        // Work that never waits gives the thread back as it checks this,
        // so the requests the worker answers carry on beside it.
        ctx.set_deadline(Some(RUN_LIMIT_MS));
        let limit = host.sleep(RUN_LIMIT_MS);
        let outcome = match piece {
            Piece::Job(job) => match race(as_system(executor, &mut ctx, job), limit).await {
                First::Left(outcome) if !ctx.out_of_time() => Some(outcome),
                First::Left(_) | First::Right(()) => {
                    ctx.abandon();
                    Some(Outcome::Fault(
                        ctx.fault(FaultKind::DeadlineExceeded, Frame::ENTRY),
                    ))
                }
            },
            // A delivery that runs out of time keeps its claim until its
            // lease passes, and the sweep tries it again.
            Piece::Delivery(delivery) => {
                match race(deliver(executor, &mut ctx, delivery), limit).await {
                    First::Left(outcome) => outcome,
                    First::Right(()) => {
                        ctx.abandon();
                        Some(Outcome::Fault(
                            ctx.fault(FaultKind::DeadlineExceeded, Frame::ENTRY),
                        ))
                    }
                }
            }
        };
        split(ctx.take_deferred(), &mut queue);
        outcomes.extend(outcome);
    }
    outcomes
}

/// Delivers `delivery` in `ctx`, which runs no action (area 12,
/// "Delivery: at least once, safe to repeat"; chunk 6.1b): claims its
/// record, unless another runner has it or it isn't due; finds whether
/// its subscriber handled it already; runs the subscriber's action as the
/// system limited to the event's tenant, continuing the event's trace,
/// its handled record committing with its writes; then records how it
/// ended. `None` when it wasn't this runner's to deliver, or the program
/// has no subscribers to deliver it to.
pub async fn deliver(
    executor: &dyn Executor,
    ctx: &mut Ctx<'_>,
    delivery: Delivery,
) -> Option<Outcome> {
    use crate::outbox::{
        self, After, CHAIN_LIMIT, FAILED, LEASE_MS, PENDING, SUCCEEDED, failure_json, micros,
    };
    if ctx.in_action() {
        return Some(Outcome::Fault(ctx.fault(
            FaultKind::Defect("a delivery inside a running action".into()),
            Frame::ENTRY,
        )));
    }
    let events = executor.events()?;
    let host = ctx.host();
    let database = host.database(&delivery.store)?;
    let now = ctx.now();
    let id = SqlValue::Uuid(delivery.id);
    let claimed = database
        .query_once(
            outbox::CLAIM,
            &[
                id.clone(),
                micros(now),
                micros(now.saturating_add(LEASE_MS)),
            ],
        )
        .await
        .ok()?;
    let attempt = match claimed.first().and_then(|row| row.first()) {
        Some(SqlValue::Int4(n)) => u32::try_from(*n).unwrap_or(u32::MAX),
        _ => return None,
    };
    let settle = |status: &'static str, next: Option<i64>, failure: Option<SqlValue>| {
        let params = [
            SqlValue::Uuid(delivery.id),
            SqlValue::Int4(i32::try_from(attempt).unwrap_or(i32::MAX)),
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
        return Some(Outcome::Fault(ctx.fault(
            FaultKind::Defect(format!(
                "a delivery to `{}`, which this build doesn't have",
                delivery.subscriber
            )),
            Frame::ENTRY,
        )));
    };
    if delivery.depth >= CHAIN_LIMIT {
        settle(FAILED, None, Some(failure_json("event_chain_limit", None))).await;
        let mut failure = crate::Failure::new(
            "runtime event_chain_limit",
            "EventChainLimit",
            "event_chain_limit",
        );
        failure.kind = Some("rejected".to_string());
        return Some(Outcome::Failed(Box::new(failure)));
    }
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
                return Some(Outcome::Value(Value::Unit));
            }
            Ok(_) => {}
            Err(_) => {
                let next = now.saturating_add(outbox::backoff(attempt, jitter(host)));
                settle(
                    PENDING,
                    Some(next),
                    Some(failure_json("unavailable", Some("unavailable"))),
                )
                .await;
                return Some(Outcome::Failed(Box::new(crate::unavailable())));
            }
        }
    }
    let Some(args) = outbox::arguments(subscriber, &delivery.value) else {
        settle(FAILED, None, Some(failure_json("internal_fault", None))).await;
        return Some(Outcome::Fault(ctx.fault(
            FaultKind::Defect("an event without the fields its subscriber reads".into()),
            Frame::ENTRY,
        )));
    };
    let job = Job {
        callable: subscriber.callable,
        args,
        system: System {
            tenant: outbox::tenant(subscriber, &delivery.value),
        },
    };
    let (trace, chain) = (ctx.trace(), ctx.chain);
    let continued = match delivery.trace {
        Some(parent) => ctx.new_span(parent),
        None => ctx.new_trace(),
    };
    ctx.set_trace(Some(continued));
    ctx.chain = delivery.depth.saturating_add(1);
    ctx.handled = subscriber.store.clone().map(|store| crate::ctx::Handled {
        store,
        id: delivery.id,
    });
    let outcome = as_system(executor, ctx, job).await;
    ctx.handled = None;
    ctx.set_trace(trace);
    ctx.chain = chain;
    match outbox::after(&outcome) {
        After::Succeeded => settle(SUCCEEDED, None, None).await,
        After::Retry { code, kind } => {
            let next = now.saturating_add(outbox::backoff(attempt, jitter(host)));
            let within = delivery
                .emitted_at
                .saturating_add(i64::try_from(subscriber.within_ms).unwrap_or(i64::MAX));
            let failure = Some(failure_json(&code, kind.as_deref()));
            if attempt < subscriber.attempts && next <= within {
                settle(PENDING, Some(next), failure).await;
            } else {
                settle(FAILED, None, failure).await;
            }
        }
        After::Fail { code, kind } => {
            settle(FAILED, None, Some(failure_json(&code, kind.as_deref()))).await;
        }
    }
    Some(outcome)
}

/// Eight random bytes for a retry's jitter, from the host; none from a
/// host with no randomness, as a test's, whose waits are their ceilings.
fn jitter(host: &dyn Host) -> Option<[u8; 8]> {
    let mut bytes = [0u8; 8];
    host.random(&mut bytes).then_some(bytes)
}

/// Delivers what is due in `store`'s outbox at `now` (area 12, "Where
/// events travel"; chunk 6.1b): every delivery still `pending` past its
/// next attempt, or `running` past its lease, which a crash between the
/// commit and the delivery, a host that dropped deferred work, or a
/// worker that died holding a lease left; at most [`SWEEP_BATCH`] of
/// them, oldest first, each as [`run_deferred`] runs one, and the work
/// they set off. A payload that no longer reads as its type is parked
/// with its problem, never dropped. The once-a-minute tick that calls it
/// is chunk 6.2's. The outcome of each delivery that ran.
///
/// [`SWEEP_BATCH`]: crate::outbox::SWEEP_BATCH
pub async fn sweep(
    executor: &dyn Executor,
    host: &dyn Host,
    store: &str,
    now: i64,
) -> Result<Vec<Outcome>, StoreError> {
    use crate::outbox::{DUE, PARK, SWEEP_BATCH, Trace, failure_json, micros};
    let Some(events) = executor.events() else {
        return Ok(Vec::new());
    };
    let database = host
        .database(store)
        .ok_or_else(|| StoreError::Fault(format!("no store called `{store}`")))?;
    let rows = database
        .query_once(DUE, &[micros(now), SqlValue::Int8(i64::from(SWEEP_BATCH))])
        .await?;
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
        match events.read(event, payload) {
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
    Ok(drive(executor, host, vec![Deferred::Deliveries(work)], Some(now)).await)
}

/// Gives `work` to `host` to run after the response it belongs to is
/// sent ([`Host::defer`]), with [`run_deferred`]: what a deployable and
/// `jadpo dev` do with the work a response left (`Response::deferred`).
/// `executor` and `host` are kept until it has run.
pub fn after_response(executor: Rc<dyn Executor>, host: Rc<dyn Host>, work: Vec<Deferred>) {
    if work.is_empty() {
        return;
    }
    let runs = Rc::clone(&host);
    host.defer(Box::pin(async move {
        run_deferred(executor.as_ref(), runs.as_ref(), work).await;
    }));
}

/// Runs `callable` once for each of `items`, its arguments, in order,
/// against `ctx`, handing each outcome to `each` with the item's index,
/// which says whether to go on: the runner's per-item mode. Inside a
/// [`Batch`] each item's action joins the batch's transaction and is
/// undone alone when it fails, as an action whose failure its caller
/// handles is (area 6), so one bad item never undoes the rest (area 12,
/// "Batches"). How many items ran.
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
        ctx.boundary = Some(crate::ctx::Boundary {
            isolation,
            ..Default::default()
        });
        if let Err(error) = ctx.transaction(store).await {
            ctx.boundary = None;
            return Err(error);
        }
        let saved = ctx.runner();
        ctx.become_system(system);
        Ok(Batch { ctx, saved })
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

    /// The batch's transaction, for the run's own statements.
    pub fn transaction(&mut self) -> Option<&mut dyn Transaction> {
        let boundary = self.ctx.boundary.as_mut()?;
        match &mut boundary.transaction {
            Some(transaction) => Some(transaction.as_mut()),
            None => None,
        }
    }

    /// Commits the batch: its items' writes and whatever the run wrote in
    /// its transaction, together. A concurrent change found by an item or
    /// at the commit is `ConcurrentChange`, and nothing is kept: the run
    /// runs the batch again.
    pub async fn commit(self) -> Result<(), StoreError> {
        let Batch { ctx, saved } = self;
        let written = crate::boundary::seal(ctx, None).await;
        let mut boundary = ctx.boundary.take().unwrap_or_default();
        if let Err(error) = written {
            ctx.restore_runner(saved);
            if let Some(transaction) = boundary.transaction.take() {
                let _ = transaction.rollback().await;
            }
            return Err(error);
        }
        ctx.restore_runner(saved);
        let Some(transaction) = boundary.transaction else {
            return Err(StoreError::Fault("a batch without its transaction".into()));
        };
        if boundary.concurrent_change {
            let _ = transaction.rollback().await;
            return Err(StoreError::ConcurrentChange);
        }
        transaction.commit().await?;
        crate::boundary::committed(ctx, boundary.sealed);
        Ok(())
    }

    /// Gives the batch up: nothing it wrote is kept.
    pub async fn rollback(self) {
        let Batch { ctx, saved } = self;
        let boundary = ctx.boundary.take().unwrap_or_default();
        ctx.restore_runner(saved);
        if let Some(transaction) = boundary.transaction {
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
        let mut ctx = Ctx::new(self.executor.places(), &host, Env::default());
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
    use std::cell::RefCell;
    use std::rc::Rc;

    use jadpo_std::policy::CallerPart;

    use super::*;
    use crate::{ActionBody, Caller, Database, Event, Failure, LocalFuture, Places, Row, block_on};

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
    /// when given one, and the work it was given to run after a response.
    #[derive(Default)]
    struct Fake {
        log: Rc<Log>,
        clock: Option<i64>,
        deferred: RefCell<Vec<LocalFuture<'static, ()>>>,
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
    }

    /// What each callable does: an action that writes `write <name>` and
    /// what its caller is, then ends as it says, or a function.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Does {
        /// Writes, emits `type Wrote`, succeeds.
        Writes,
        /// Writes, emits, then fails `not_found`.
        Fails,
        /// Writes, and leaves callable 3 to run after it.
        SetsOff,
        /// A function giving its argument back.
        Echoes,
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
                Deferred::Run(_) => Vec::new(),
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
                let _ = ctx.emit(Event {
                    key: format!("type Wrote {name}"),
                    value: Value::Unit,
                    payload: "null".into(),
                    subscribers: vec!["subscriber s".into()],
                });
                match self.does {
                    Does::Fails => {
                        let mut failure = Failure::new("type Gone", "Gone", "gone");
                        failure.kind = Some("not_found".into());
                        Outcome::Failed(Box::new(failure))
                    }
                    Does::SetsOff => {
                        ctx.defer(Deferred::Run(Job {
                            callable: 3,
                            args: vec![Value::text("follow-on")],
                            system: System::default(),
                        }));
                        Outcome::Value(Value::Unit)
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

        fn callable(&self, _key: &str) -> Option<u32> {
            None
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

    fn job(callable: u32, name: &str, tenant: Option<u8>) -> Deferred {
        Deferred::Run(Job {
            callable,
            args: vec![Value::text(name)],
            system: System {
                tenant: tenant.map(|byte| [byte; 16]),
            },
        })
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

    /// Inline: the work runs in the invocation that left it, each job as
    /// the system in a boundary of its own, the work it sets off after it,
    /// and whoever ran the invocation runs it again afterwards.
    #[test]
    fn inline_work_runs_as_the_system_then_gives_the_caller_back() {
        let program = Program::new(&[Does::Writes, Does::Fails, Does::SetsOff, Does::Writes]);
        let host = Fake::default();
        let mut ctx = Ctx::new(&program.places, &host, Env::default());
        signed_in(&mut ctx);
        let work = vec![
            Deferred::Deliveries(vec![delivery("type Placed")]),
            job(2, "order", Some(9)),
            job(1, "refused", None),
        ];
        let outcomes = block_on(settle(&program, &mut ctx, work)).expect("nothing waits");
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
                "begin repeatable_read",
                "write follow-on as system",
                "outbox",
                "deliveries",
                "commit",
            ],
            "the follow-on runs after the work queued before it"
        );
        assert_eq!(outcomes.len(), 3);
        assert!(matches!(&outcomes[1], Outcome::Failed(failure) if failure.code == "gone"));
        assert_eq!(ctx.system(), None);
        assert!(ctx.caller().is_some(), "the request's caller again");
        // What the jobs emitted was settled too: nothing is left over.
        assert!(ctx.take_deferred().is_empty());
    }

    /// Deferred: the host is given the work to run once the response is
    /// handed back, and it runs nothing before; then each job runs in a
    /// context of its own, under the host's clock, as the system.
    #[test]
    fn deferred_work_runs_only_when_the_host_runs_it_after_the_response() {
        let program: Rc<dyn Executor> = Rc::new(Program::new(&[
            Does::Writes,
            Does::Fails,
            Does::SetsOff,
            Does::Writes,
        ]));
        let fake = Rc::new(Fake {
            clock: Some(1_800_000_000_000),
            ..Fake::default()
        });
        let host: Rc<dyn Host> = Rc::clone(&fake) as Rc<dyn Host>;
        after_response(Rc::clone(&program), Rc::clone(&host), Vec::new());
        assert!(
            fake.deferred.borrow().is_empty(),
            "nothing to run, nothing handed over"
        );
        after_response(
            Rc::clone(&program),
            Rc::clone(&host),
            vec![job(2, "order", None)],
        );
        assert_eq!(fake.deferred.borrow().len(), 1);
        assert!(
            fake.log.take().is_empty(),
            "nothing runs until the host runs it"
        );
        let work: Vec<_> = fake.deferred.borrow_mut().drain(..).collect();
        for work in work {
            block_on(work).expect("nothing waits");
        }
        assert_eq!(
            fake.log.take(),
            [
                "begin repeatable_read",
                "write order as system",
                "at 1800000000000",
                "outbox",
                "deliveries",
                "commit",
                "begin repeatable_read",
                "write follow-on as system",
                "at 1800000000000",
                "outbox",
                "deliveries",
                "commit",
            ]
        );
        // A host with no clock runs nothing.
        let clockless = Fake::default();
        let outcomes = block_on(run_deferred(
            program.as_ref(),
            &clockless,
            vec![job(0, "x", None)],
        ))
        .expect("nothing waits");
        assert!(matches!(
            &outcomes[..],
            [Outcome::Fault(fault)] if fault.kind == FaultKind::Defect("the host has no clock".into())
        ));
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
