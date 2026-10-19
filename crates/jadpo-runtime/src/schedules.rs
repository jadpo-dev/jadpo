//! Schedules and the runtime's minute tick (area 12, "Schedules",
//! "Watching schedules" and question 16; chunk 6.2).
//!
//! A host calls [`tick`] once a minute (natively its own timer, on
//! Workers the cron trigger, chunk 8.2), more often when a schedule's
//! `every:` is shorter. Duplicate ticks, and ticks from several instances,
//! are harmless: each schedule's state row (`jadpo_schedules`, in
//! `schedules: { store: }`) decides.
//!
//! - **When a schedule runs.** Its first run is due one `every:` after the
//!   first tick that sees it, and each after that one `every:` later, on
//!   the same grid, counted in UTC. A tick at or past its due time runs it
//!   once, however many due times it passed while the application was
//!   down, and moves its next due time to the first one after the tick:
//!   missed ticks become one run, not a backlog.
//! - **One run at a time.** A run takes the schedule's lease in one
//!   conditional statement (its due time unchanged and no live lease), so
//!   two instances never both run it; the lease lasts the 15-minute limit
//!   of a run and a minute, so a worker that dies holding it loses it
//!   then. A due tick that finds it leased, or paused, is skipped, not
//!   queued: recorded with its reason, its due time moved on.
//! - **A run** is its action, called with nothing, as the system with no
//!   tenant (area 9; area 12, "A schedule and tenants"), in a context of
//!   its own whose `clock.now` is the tick's time throughout, in a new
//!   trace, stopped at the 15-minute limit. A failed run is recorded and
//!   not retried: the next due time is its retry. What it emitted is
//!   delivered after it, on the runner.
//! - **Every run is recorded** in `jadpo_schedule_runs`: due, started and
//!   ended, how it ended with its failure's code and kind, never a value,
//!   the events it kept and its trace; skipped ticks too. Each state row
//!   keeps the last tick that reached it. Records are kept 30 days.
//!
//! On each tick the runtime also sweeps its own tables in every store
//! that holds them: deliveries left behind ([`runner::sweep`]), expired
//! idempotency keys (area 7), and events every subscriber has had,
//! [`SETTLED_KEPT_MS`] after they were emitted; then each of
//! [`HOOKS`], where batch runs resume (chunk 6.4).

use std::future::Future;
use std::pin::Pin;
use std::task::Poll;

use jadpo_std::schedule::{SchedulePlan, SchedulesPlan};

use crate::ctx::System;
use crate::outbox::{failure_json, micros};
use crate::race::{First, race};
use crate::runner::{self, Deferred, Job, RUN_LIMIT_MS};
use crate::{
    Clock, Ctx, Env, Executor, FaultKind, Frame, Host, LocalFuture, Outcome, SqlType, SqlValue,
    StoreError,
};

/// How long a run holds its schedule's lease: the 15-minute limit of one
/// run and a minute, as a delivery's claim.
pub const LEASE_MS: i64 = crate::outbox::LEASE_MS;
/// How long a run record is kept after its run started.
pub const RUNS_KEPT_MS: i64 = 30 * 24 * 60 * 60 * 1000;
/// How long an event every subscriber has had is kept after it was
/// emitted, before the tick deletes it with its delivery records.
pub const SETTLED_KEPT_MS: i64 = 24 * 60 * 60 * 1000;
/// How many rows one delete takes, and how many deletes of each kind one
/// tick runs in a store at most, so a tick ends well inside a Workers
/// cron run's limits however much is due; the next tick carries on.
pub const DELETE_BATCH: i64 = 1_000;
pub const DELETE_BATCHES: u32 = 10;

/// A run record's outcomes (area 12, "What an operator sees").
pub const RUNNING: &str = "running";
pub const SUCCEEDED: &str = "succeeded";
pub const FAILED: &str = "failed";
pub const SKIPPED: &str = "skipped";

/// Work the tick runs after the schedules and the runtime's own sweeps,
/// given the program, the host and the tick's time; what it leaves runs on
/// the runner after it.
pub type Hook = for<'a> fn(&'a dyn Executor, &'a dyn Host, i64) -> LocalFuture<'a, Vec<Deferred>>;

/// What the tick runs after its sweeps: batch runs whose runner's lease
/// lapsed are picked up from where they committed (chunk 6.4).
pub const HOOKS: &[Hook] = &[resume_batches];

/// [`crate::batches::resume`] as a [`Hook`]: what the runs it picked up
/// left for after.
fn resume_batches<'a>(
    executor: &'a dyn Executor,
    host: &'a dyn Host,
    now: i64,
) -> LocalFuture<'a, Vec<Deferred>> {
    Box::pin(async move { crate::batches::resume(executor, host, now).await.1 })
}

/// A state row for each schedule that has none, its first run due one
/// interval after this tick: names, intervals and first due times
/// unnested.
pub const ENSURE: &str = "INSERT INTO \"jadpo_schedules\" (\"schedule\", \"every_ms\", \"next_run_at\", \"paused\") SELECT \"s\".\"schedule\", \"s\".\"every_ms\", \"s\".\"next_run_at\", FALSE FROM unnest($1::text[], $2::int8[], $3::timestamptz[]) AS \"s\"(\"schedule\", \"every_ms\", \"next_run_at\") ON CONFLICT (\"schedule\") DO NOTHING";

/// Records tick `$1` on each of schedules `$2`, giving each one's state.
pub const TICKED: &str = "UPDATE \"jadpo_schedules\" SET \"last_tick_at\" = GREATEST(COALESCE(\"last_tick_at\", $1), $1) WHERE \"schedule\" = ANY($2::text[]) RETURNING \"schedule\", \"every_ms\", \"next_run_at\", \"paused\", \"lease_until\", \"run_id\"";

/// Schedule `$1`'s `every:` changed to `$2`: its next due time is the
/// earlier of the one kept and `$3`, one new interval after the tick.
pub const REGRID: &str = "UPDATE \"jadpo_schedules\" SET \"every_ms\" = $2, \"next_run_at\" = LEAST(\"next_run_at\", $3) WHERE \"schedule\" = $1 AND \"every_ms\" <> $2 RETURNING \"next_run_at\"";

/// Takes schedule `$1`'s lease at tick `$6` for run `$5` until `$4`,
/// moving its next due time from `$2` to `$3`: only while that due time
/// stands, it isn't paused and no live lease holds it, so one claimer wins.
pub const CLAIM: &str = "UPDATE \"jadpo_schedules\" SET \"next_run_at\" = $3, \"lease_until\" = $4, \"run_id\" = $5 WHERE \"schedule\" = $1 AND \"next_run_at\" = $2 AND NOT \"paused\" AND (\"lease_until\" IS NULL OR \"lease_until\" <= $6) RETURNING \"schedule\"";

/// Moves schedule `$1`'s next due time from `$2` to `$3` past a tick that
/// skips it: once, by whichever instance gets there first.
pub const SKIP: &str = "UPDATE \"jadpo_schedules\" SET \"next_run_at\" = $3 WHERE \"schedule\" = $1 AND \"next_run_at\" = $2 RETURNING \"schedule\"";

/// Gives schedule `$1`'s lease back once run `$2` has ended.
pub const RELEASE: &str = "UPDATE \"jadpo_schedules\" SET \"lease_until\" = NULL, \"run_id\" = NULL WHERE \"schedule\" = $1 AND \"run_id\" = $2 RETURNING \"schedule\"";

/// A run record: its id, schedule, due, start and end, outcome, the reason
/// a tick skipped it, its failure, its events and its trace.
pub const RECORD: &str = "INSERT INTO \"jadpo_schedule_runs\" (\"id\", \"schedule\", \"due_slot\", \"started_at\", \"ended_at\", \"outcome\", \"reason\", \"failure\", \"events\", \"traceparent\") VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)";

/// How run `$1` ended at `$2`: its outcome `$3`, failure `$4` and events
/// `$5`, while it is still recorded as running.
pub const ENDED: &str = "UPDATE \"jadpo_schedule_runs\" SET \"ended_at\" = $2, \"outcome\" = $3, \"failure\" = $4, \"events\" = $5 WHERE \"id\" = $1 AND \"outcome\" = 'running' RETURNING \"id\"";

/// Expired idempotency keys (area 7, "Expired idempotency keys"): a batch
/// of those past their `kept_until` at `$1`, at most `$2`.
pub const EXPIRED_KEYS: &str = "DELETE FROM \"jadpo_idempotency_keys\" WHERE (\"scope\", \"key\") IN (SELECT \"scope\", \"key\" FROM \"jadpo_idempotency_keys\" WHERE \"kept_until\" <= $1 LIMIT $2) RETURNING 1";

/// Events every subscriber has had, emitted at or before `$1`, at most
/// `$2`: their delivery records go with them. An event with a delivery
/// still to make, or that failed, stays for the sweep or an operator.
pub const SETTLED: &str = "DELETE FROM \"jadpo_outbox\" WHERE \"id\" IN (SELECT \"o\".\"id\" FROM \"jadpo_outbox\" AS \"o\" WHERE \"o\".\"emitted_at\" <= $1 AND NOT EXISTS (SELECT 1 FROM \"jadpo_deliveries\" AS \"d\" WHERE \"d\".\"outbox_id\" = \"o\".\"id\" AND \"d\".\"status\" <> 'succeeded') ORDER BY \"o\".\"emitted_at\" LIMIT $2) RETURNING 1";

/// Run records of runs started at or before `$1`, at most `$2`.
pub const OLD_RUNS: &str = "DELETE FROM \"jadpo_schedule_runs\" WHERE \"id\" IN (SELECT \"id\" FROM \"jadpo_schedule_runs\" WHERE \"started_at\" <= $1 AND \"outcome\" <> 'running' LIMIT $2) RETURNING 1";

/// Why a due tick didn't run a schedule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Skipped {
    /// An operator paused it.
    Paused,
    /// A run started earlier still holds its lease.
    LeaseHeld,
}

impl Skipped {
    /// As its run record names it.
    pub fn word(self) -> &'static str {
        match self {
            Skipped::Paused => "paused",
            Skipped::LeaseHeld => "lease_held",
        }
    }
}

/// How a due schedule went on one tick.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ran {
    /// Its action succeeded, keeping this many events.
    Succeeded { events: u32 },
    /// Its action failed or faulted, with the code and kind its record
    /// keeps; the next due time is its retry.
    Failed { code: String, kind: Option<String> },
    /// The tick skipped it.
    Skipped(Skipped),
}

/// One due schedule on one tick.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Due {
    /// The schedule's identity key.
    pub schedule: String,
    /// The due time the tick took, in milliseconds since the epoch.
    pub due_slot: i64,
    pub ran: Ran,
}

/// What one tick did.
#[derive(Clone, Debug, Default)]
pub struct Ticked {
    /// Each schedule that was due, in the order of the plan, and how it
    /// went; a schedule another instance took on this tick isn't listed.
    pub due: Vec<Due>,
    /// The outcome of each delivery the tick ran: what the runs emitted,
    /// then what the sweep found left behind.
    pub deliveries: Vec<Outcome>,
    /// What the tick deleted, across the stores.
    pub deleted: Deleted,
    /// Each store the tick couldn't do its work in, with why: it carries
    /// on with the rest, and the next tick tries again.
    pub problems: Vec<(String, StoreError)>,
}

/// How many rows a tick deleted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Deleted {
    pub keys: u64,
    pub events: u64,
    pub runs: u64,
}

/// Runs one tick at `now` on `host`: the schedules due, the runtime's own
/// sweeps, then [`HOOKS`]. What it did.
pub async fn tick(executor: &dyn Executor, host: &dyn Host, now: i64) -> Ticked {
    let mut ticked = Ticked::default();
    let Some(plan) = executor.schedules() else {
        return ticked;
    };
    if let Some(store) = &plan.store
        && !plan.schedules.is_empty()
    {
        match due(plan, host, store, now).await {
            Ok(states) => {
                let runs = states
                    .into_iter()
                    .map(|(schedule, state)| take(executor, host, store, schedule, state, now));
                let mut left = Vec::new();
                for (due, more) in join_all(runs.collect()).await.into_iter().flatten() {
                    ticked.due.push(due);
                    left.extend(more);
                }
                ticked.deliveries = runner::run_deferred(executor, host, left).await;
            }
            Err(error) => ticked.problems.push((store.clone(), error)),
        }
        match delete(host, store, OLD_RUNS, now.saturating_sub(RUNS_KEPT_MS)).await {
            Ok(count) => ticked.deleted.runs += count,
            Err(error) => ticked.problems.push((store.clone(), error)),
        }
    }
    for store in &plan.stores {
        match runner::sweep(executor, host, store, now).await {
            Ok(outcomes) => ticked.deliveries.extend(outcomes),
            Err(error) => {
                ticked.problems.push((store.clone(), error));
                continue;
            }
        }
        match delete(host, store, EXPIRED_KEYS, now).await {
            Ok(count) => ticked.deleted.keys += count,
            Err(error) => ticked.problems.push((store.clone(), error)),
        }
        match delete(host, store, SETTLED, now.saturating_sub(SETTLED_KEPT_MS)).await {
            Ok(count) => ticked.deleted.events += count,
            Err(error) => ticked.problems.push((store.clone(), error)),
        }
    }
    for hook in HOOKS {
        let left = hook(executor, host, now).await;
        ticked
            .deliveries
            .extend(runner::run_deferred(executor, host, left).await);
    }
    ticked
}

/// How often a host's own timer ticks for `plan`: once a minute, or as
/// often as its shortest `every:` when that is shorter, at least once a
/// second.
pub fn tick_every_ms(plan: Option<&SchedulesPlan>) -> u64 {
    let minute = 60_000;
    plan.and_then(SchedulesPlan::shortest_every)
        .map_or(minute, |shortest| shortest.clamp(1_000, minute))
}

/// The state of a schedule's row, as a tick reads it.
#[derive(Clone, Copy, Debug)]
struct State {
    every_ms: i64,
    next_run_at: i64,
    paused: bool,
    lease_until: Option<i64>,
    run: Option<[u8; 16]>,
}

/// Records the tick on every schedule's row, making the rows it lacks,
/// and gives each schedule due at `now` with its state, interval changes
/// applied.
async fn due<'p>(
    plan: &'p SchedulesPlan,
    host: &dyn Host,
    store: &str,
    now: i64,
) -> Result<Vec<(&'p SchedulePlan, State)>, StoreError> {
    let database = host
        .database(store)
        .ok_or_else(|| StoreError::Fault(format!("no store called `{store}`")))?;
    let keys: Vec<SqlValue> = plan
        .schedules
        .iter()
        .map(|schedule| SqlValue::Text(schedule.key.clone()))
        .collect();
    let every: Vec<SqlValue> = plan
        .schedules
        .iter()
        .map(|schedule| SqlValue::Int8(every_of(schedule)))
        .collect();
    let first: Vec<SqlValue> = plan
        .schedules
        .iter()
        .map(|schedule| micros(now.saturating_add(every_of(schedule))))
        .collect();
    database
        .query_once(
            ENSURE,
            &[
                SqlValue::Array(SqlType::Text, keys.clone()),
                SqlValue::Array(SqlType::Int8, every),
                SqlValue::Array(SqlType::Timestamp, first),
            ],
        )
        .await?;
    let rows = database
        .query_once(TICKED, &[micros(now), SqlValue::Array(SqlType::Text, keys)])
        .await?;
    let mut out = Vec::new();
    for schedule in &plan.schedules {
        let Some(mut state) = rows.iter().find_map(|row| match row.first() {
            Some(SqlValue::Text(key)) if *key == schedule.key => read_state(row),
            _ => None,
        }) else {
            continue;
        };
        let every = every_of(schedule);
        if state.every_ms != every {
            let moved = database
                .query_once(
                    REGRID,
                    &[
                        SqlValue::Text(schedule.key.clone()),
                        SqlValue::Int8(every),
                        micros(now.saturating_add(every)),
                    ],
                )
                .await?;
            if let Some(SqlValue::Timestamp(at)) = moved.first().and_then(|row| row.first()) {
                state.next_run_at = at.div_euclid(1000);
            }
            state.every_ms = every;
        }
        if state.next_run_at <= now {
            out.push((schedule, state));
        }
    }
    Ok(out)
}

fn every_of(schedule: &SchedulePlan) -> i64 {
    i64::try_from(schedule.every_ms).unwrap_or(i64::MAX).max(1)
}

fn read_state(row: &[SqlValue]) -> Option<State> {
    let millis = |value: Option<&SqlValue>| match value {
        Some(SqlValue::Timestamp(at)) => Some(at.div_euclid(1000)),
        _ => None,
    };
    let every_ms = match row.get(1) {
        Some(SqlValue::Int8(every)) => *every,
        _ => return None,
    };
    Some(State {
        every_ms,
        next_run_at: millis(row.get(2))?,
        paused: matches!(row.get(3), Some(SqlValue::Bool(true))),
        lease_until: millis(row.get(4)),
        run: match row.get(5) {
            Some(SqlValue::Uuid(id)) => Some(*id),
            _ => None,
        },
    })
}

/// The first due time on `due`'s grid of `every` after `now`.
pub fn next_due(due: i64, every: i64, now: i64) -> i64 {
    let every = every.max(1);
    let passed = now.saturating_sub(due).max(0) / every;
    due.saturating_add(passed.saturating_add(1).saturating_mul(every))
}

/// Takes due `schedule` at `now`: skipped while paused or leased, run
/// once its lease is this tick's. `None` when another instance took it
/// first; otherwise how it went and the work its run left.
async fn take(
    executor: &dyn Executor,
    host: &dyn Host,
    store: &str,
    schedule: &SchedulePlan,
    state: State,
    now: i64,
) -> Option<(Due, Vec<Deferred>)> {
    let database = host.database(store)?;
    let key = SqlValue::Text(schedule.key.clone());
    let due_slot = state.next_run_at;
    let next = next_due(due_slot, state.every_ms, now);
    let env = Env {
        seed: None,
        clock: Clock::fixed(now),
    };
    let mut ctx = Ctx::new(executor.places(), host, env);
    let skipped = if state.paused {
        Some(Skipped::Paused)
    } else if state.lease_until.is_some_and(|until| until > now) {
        Some(Skipped::LeaseHeld)
    } else {
        None
    };
    if let Some(why) = skipped {
        let moved = database
            .query_once(SKIP, &[key.clone(), micros(due_slot), micros(next)])
            .await
            .ok()?;
        moved.first()?;
        let id = ctx.new_id()?;
        let _ = database
            .query_once(
                RECORD,
                &[
                    SqlValue::Uuid(id),
                    key,
                    micros(due_slot),
                    micros(now),
                    micros(now),
                    SqlValue::Text(SKIPPED.into()),
                    SqlValue::Text(why.word().into()),
                    SqlValue::Null(SqlType::Json),
                    SqlValue::Null(SqlType::Int4),
                    SqlValue::Null(SqlType::Text),
                ],
            )
            .await;
        return Some((
            Due {
                schedule: schedule.key.clone(),
                due_slot,
                ran: Ran::Skipped(why),
            },
            Vec::new(),
        ));
    }
    let run = ctx.new_id()?;
    let claimed = database
        .query_once(
            CLAIM,
            &[
                key.clone(),
                micros(due_slot),
                micros(next),
                micros(now.saturating_add(LEASE_MS)),
                SqlValue::Uuid(run),
                micros(now),
            ],
        )
        .await
        .ok()?;
    claimed.first()?;
    // A run that held the lease before and never ended was lost with its
    // worker: its record says so.
    if let Some(lost) = state.run {
        let ended_at = state.lease_until.unwrap_or(now).min(now);
        let _ = database
            .query_once(
                ENDED,
                &[
                    SqlValue::Uuid(lost),
                    micros(ended_at),
                    SqlValue::Text(FAILED.into()),
                    failure_json("lease_expired", None),
                    SqlValue::Null(SqlType::Int4),
                ],
            )
            .await;
    }
    let trace = ctx.new_trace();
    let _ = database
        .query_once(
            RECORD,
            &[
                SqlValue::Uuid(run),
                key.clone(),
                micros(due_slot),
                micros(now),
                SqlValue::Null(SqlType::Timestamp),
                SqlValue::Text(RUNNING.into()),
                SqlValue::Null(SqlType::Text),
                SqlValue::Null(SqlType::Json),
                SqlValue::Null(SqlType::Int4),
                SqlValue::Text(trace.header()),
            ],
        )
        .await;
    ctx.set_trace(Some(trace));
    // Work that never waits gives the thread back as it checks this.
    ctx.set_deadline(Some(RUN_LIMIT_MS));
    let job = Job {
        callable: schedule.callable,
        args: Vec::new(),
        system: System { tenant: None },
    };
    let limit = host.sleep(RUN_LIMIT_MS);
    let outcome = match race(runner::as_system(executor, &mut ctx, job), limit).await {
        First::Left(outcome) if !ctx.out_of_time() => outcome,
        First::Left(_) | First::Right(()) => {
            ctx.abandon();
            Outcome::Fault(ctx.fault(FaultKind::DeadlineExceeded, Frame::ENTRY))
        }
    };
    let ran = match &outcome {
        Outcome::Value(_) => Ran::Succeeded {
            events: ctx.emitted,
        },
        Outcome::Failed(failure) => Ran::Failed {
            code: failure.code.clone(),
            kind: failure.kind.clone(),
        },
        Outcome::Fault(fault) => Ran::Failed {
            code: fault.kind.classification().to_string(),
            kind: None,
        },
        Outcome::Assertion(_) => Ran::Failed {
            code: "internal_fault".to_string(),
            kind: None,
        },
    };
    let ended_at = host.now_ms().unwrap_or(now).max(now);
    let (status, failure, events) = match &ran {
        Ran::Succeeded { events } => (
            SUCCEEDED,
            SqlValue::Null(SqlType::Json),
            SqlValue::Int4(i32::try_from(*events).unwrap_or(i32::MAX)),
        ),
        Ran::Failed { code, kind } => (
            FAILED,
            failure_json(code, kind.as_deref()),
            SqlValue::Int4(0),
        ),
        Ran::Skipped(_) => (SKIPPED, SqlValue::Null(SqlType::Json), SqlValue::Int4(0)),
    };
    let _ = database
        .query_once(
            ENDED,
            &[
                SqlValue::Uuid(run),
                micros(ended_at),
                SqlValue::Text(status.into()),
                failure,
                events,
            ],
        )
        .await;
    let _ = database
        .query_once(RELEASE, &[key, SqlValue::Uuid(run)])
        .await;
    Some((
        Due {
            schedule: schedule.key.clone(),
            due_slot,
            ran,
        },
        ctx.take_deferred(),
    ))
}

/// Runs delete `sql` in `store` with cut-off `before`, batch after batch
/// until one comes back short or [`DELETE_BATCHES`] have run. How many
/// rows it deleted.
async fn delete(host: &dyn Host, store: &str, sql: &str, before: i64) -> Result<u64, StoreError> {
    let database = host
        .database(store)
        .ok_or_else(|| StoreError::Fault(format!("no store called `{store}`")))?;
    let mut deleted = 0u64;
    for _ in 0..DELETE_BATCHES {
        let rows = database
            .query_once(sql, &[micros(before), SqlValue::Int8(DELETE_BATCH)])
            .await?;
        deleted += rows.len() as u64;
        if (rows.len() as i64) < DELETE_BATCH {
            break;
        }
    }
    Ok(deleted)
}

/// Runs `futures` together on this thread until every one has finished,
/// giving their outputs in order: each due schedule's run goes on beside
/// the others, so a long one holds none of them up.
async fn join_all<F: Future>(futures: Vec<F>) -> Vec<F::Output> {
    let mut pending: Vec<Pin<Box<F>>> = futures.into_iter().map(Box::pin).collect();
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

    #[test]
    fn missed_due_slots_become_one_run_on_the_same_grid() {
        let minute = 60_000;
        // Due at 10:00 every 15 minutes; the tick at 10:00 moves it to 10:15.
        assert_eq!(next_due(0, 15 * minute, 0), 15 * minute);
        // A tick a little late still moves it to 10:15.
        assert_eq!(next_due(0, 15 * minute, 40_000), 15 * minute);
        // Down from 10:00 to 13:02: one run, next due 13:15, not a backlog.
        let down = (3 * 60 + 2) * minute;
        assert_eq!(next_due(0, 15 * minute, down), (3 * 60 + 15) * minute);
        // Exactly on a later due time: the next one after it.
        assert_eq!(next_due(0, 15 * minute, 30 * minute), 45 * minute);
    }

    #[test]
    fn a_host_ticks_once_a_minute_or_as_often_as_the_shortest_schedule() {
        let plan = |every: &[u64]| SchedulesPlan {
            schedules: every
                .iter()
                .map(|&every_ms| SchedulePlan {
                    key: format!("schedule s{every_ms}"),
                    name: "s".into(),
                    every_ms,
                    callable: 0,
                    file: String::new(),
                    line: 1,
                })
                .collect(),
            store: Some("main".into()),
            stores: vec!["main".into()],
        };
        assert_eq!(tick_every_ms(None), 60_000);
        assert_eq!(tick_every_ms(Some(&plan(&[]))), 60_000);
        assert_eq!(tick_every_ms(Some(&plan(&[3_600_000, 900_000]))), 60_000);
        assert_eq!(tick_every_ms(Some(&plan(&[30_000, 900_000]))), 30_000);
    }

    #[test]
    fn several_futures_finish_together_in_order() {
        let joined = crate::block_on(join_all(vec![
            Box::pin(async { 1 }) as Pin<Box<dyn Future<Output = i32>>>,
            Box::pin(async {
                crate::yield_now().await;
                2
            }),
        ]))
        .unwrap();
        assert_eq!(joined, [1, 2]);
    }
}
