//! Schedules and the runtime's minute tick (area 12, "Schedules",
//! "Watching schedules" and question 16; chunk 6.2).
//!
//! A host calls [`tick`] once a minute (natively its own timer, on
//! Workers the cron trigger, chunk 8.2f), more often when a schedule's
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
//!   two instances never both run it; the lease lasts what is left of the
//!   tick's budget and a minute ([`crate::lease`]; chunk 6.7), so a worker
//!   that dies holding it loses it then. A due tick that finds it leased,
//!   or paused, is skipped, not queued: recorded with its reason, its due
//!   time moved on.
//! - **A run** is its action, called with nothing, as the system with no
//!   tenant (area 9; area 12, "A schedule and tenants"), in a context of
//!   its own whose `clock.now` is the tick's time throughout, in a new
//!   trace, run within the tick's budget as the runner runs any action
//!   ([`runner::within_budget`]): stopped at the 15-minute limit, or
//!   before the budget ends, its commit never raced. A failed run is
//!   recorded and not retried: the next due time is its retry. What it
//!   emitted is delivered after it, on the runner.
//! - **Every run is recorded** in `jadpo_schedule_runs`: due, started and
//!   ended, how it ended with its failure's code and kind, never a value,
//!   the events it kept and its trace; skipped ticks too. Each state row
//!   keeps the last tick that reached it. Records are kept 30 days.
//! - **A run an operator asked for** (`jadpo schedules run` against a
//!   deployment; chunk 6.2d) waits on the state row as `run_requested`,
//!   the id its record will have. A tick runs it by hand under the lease,
//!   paused or not, its grid left as it was, when no run holds the lease
//!   and the schedule's due run didn't take it on the same tick: one run
//!   of a schedule per tick, so a request waits for the next tick then.
//!
//! On each tick the runtime also sweeps its own tables in every store
//! that holds them: deliveries left behind ([`runner::sweep`]), expired
//! idempotency keys (area 7), events every subscriber has had,
//! [`SETTLED_KEPT_MS`] after they were emitted, with their delivery
//! records and then the handled records of those deliveries, wherever
//! each is ([`settle`]), and ended browser sessions, API tokens and
//! service keys in the store each is in (area 8; chunk 6.10). What an
//! operator settled goes after [`RUNS_KEPT_MS`] (chunk 6.13): a delivery
//! dismissed or cancelled that long after the operator's last action on
//! it, with its handled records ([`settle_by_hand`]), and so its event,
//! once no delivery of it is left that could run; and the operator's log
//! rows that long after they were written, once their delivery or run is
//! gone ([`OLD_DELIVERY_ACTIONS`], [`OLD_RUN_ACTIONS`]); then each of
//! [`HOOKS`], where batch runs resume (chunk 6.4) and ended ones are
//! deleted after 30 days (chunk 6.6a). The whole tick runs within the
//! budget its host gives a tick (chunk 6.7): once that is spent it starts
//! nothing more, and the next tick carries on.
//!
//! - **Alerts are raised on the tick** (chunk 6.9): once its schedules
//!   have run, it works out the alerts that hold for each and raises those
//!   that have just started, recorded once on the schedule's state row
//!   ([`watch::raise`]), so of several instances' ticks one raises each.
//!
//! What an operator reads and does with them, `jadpo schedules`, is
//! [`watch`] (chunk 6.2b); `jadpo why`'s answer is [`why`] (chunk 6.9).

pub mod watch;
pub mod why;

use crate::race::join_all;

use jadpo_std::schedule::{SchedulePlan, ScheduleRun, SchedulesPlan};

use crate::budget::{Budget, Invocation};
use crate::ctx::System;
use crate::outbox::{failure_json, micros};
use crate::runner::{self, Deferred, Job};
use crate::{
    Clock, Ctx, Database, Env, Executor, FaultKind, Frame, Host, LocalFuture, Outcome, SqlType,
    SqlValue, StoreError,
};
/// How long a run record is kept after its run started; and a delivery
/// an operator dismissed or cancelled, and an operator's log row, after
/// the operator acted (chunk 6.13).
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
/// The reason a run record gives for a run an operator started
/// (`jadpo schedules run`), rather than a tick.
pub const BY_HAND: &str = "by_hand";

/// Work the tick runs after the schedules and the runtime's own sweeps,
/// given the program, the host, the tick's budget and its time; what it
/// leaves runs on the runner after it, within the same budget.
pub type Hook = for<'a, 'h> fn(
    &'a dyn Executor,
    &'a dyn Host,
    &'a Budget<'h>,
    i64,
) -> LocalFuture<'a, Vec<Deferred>>;

/// What the tick runs after its sweeps: batch runs whose runner's lease
/// lapsed are picked up from where they committed (chunk 6.4), and runs
/// that ended 30 days ago are deleted (chunk 6.6a).
pub const HOOKS: &[Hook] = &[resume_batches, clean_batches];

/// [`crate::batches::resume`] as a [`Hook`], within the tick's budget:
/// what the runs it picked up left for after.
fn resume_batches<'a, 'h>(
    executor: &'a dyn Executor,
    host: &'a dyn Host,
    budget: &'a Budget<'h>,
    now: i64,
) -> LocalFuture<'a, Vec<Deferred>> {
    Box::pin(async move {
        crate::batches::resume_within(executor, host, budget, now)
            .await
            .1
    })
}

/// [`crate::batches::clean`] as a [`Hook`]: it leaves nothing for after.
fn clean_batches<'a, 'h>(
    executor: &'a dyn Executor,
    host: &'a dyn Host,
    _budget: &'a Budget<'h>,
    now: i64,
) -> LocalFuture<'a, Vec<Deferred>> {
    Box::pin(async move {
        crate::batches::clean(executor, host, now).await;
        Vec::new()
    })
}

/// A state row for each schedule that has none, its first run due one
/// interval after this tick: names, intervals and first due times
/// unnested.
pub const ENSURE: &str = "INSERT INTO \"jadpo_schedules\" (\"schedule\", \"every_ms\", \"next_run_at\", \"paused\") SELECT \"s\".\"schedule\", \"s\".\"every_ms\", \"s\".\"next_run_at\", FALSE FROM unnest($1::text[], $2::int8[], $3::timestamptz[]) AS \"s\"(\"schedule\", \"every_ms\", \"next_run_at\") ON CONFLICT (\"schedule\") DO NOTHING";

/// Records tick `$1` on each of schedules `$2`, giving each one's state
/// and the run an operator asked for, if any.
pub const TICKED: &str = "UPDATE \"jadpo_schedules\" SET \"last_tick_at\" = GREATEST(COALESCE(\"last_tick_at\", $1), $1) WHERE \"schedule\" = ANY($2::text[]) RETURNING \"schedule\", \"every_ms\", \"next_run_at\", \"paused\", \"lease_until\", \"run_id\", \"run_requested\"";

/// Schedule `$1`'s `every:` changed to `$2`: its next due time is the
/// earlier of the one kept and `$3`, one new interval after the tick.
pub const REGRID: &str = "UPDATE \"jadpo_schedules\" SET \"every_ms\" = $2, \"next_run_at\" = LEAST(\"next_run_at\", $3) WHERE \"schedule\" = $1 AND \"every_ms\" <> $2 RETURNING \"next_run_at\"";

/// Takes schedule `$1`'s lease at tick `$6` for run `$5` until `$4`,
/// moving its next due time from `$2` to `$3`: only while that due time
/// stands, it isn't paused and no live lease holds it, so one claimer wins.
pub const CLAIM: &str = "UPDATE \"jadpo_schedules\" SET \"next_run_at\" = $3, \"lease_until\" = $4, \"run_id\" = $5 WHERE \"schedule\" = $1 AND \"next_run_at\" = $2 AND NOT \"paused\" AND (\"lease_until\" IS NULL OR \"lease_until\" <= $6) RETURNING \"schedule\"";

/// Takes schedule `$1`'s lease at tick `$4` until `$3` for run `$2`, the
/// one an operator asked for, which no longer waits: only while that is
/// the request standing and no live lease holds it, paused or not, its
/// due time left as it is.
pub const CLAIM_REQUESTED: &str = "UPDATE \"jadpo_schedules\" SET \"lease_until\" = $3, \"run_id\" = $2, \"run_requested\" = NULL WHERE \"schedule\" = $1 AND \"run_requested\" = $2 AND (\"lease_until\" IS NULL OR \"lease_until\" <= $4) RETURNING \"schedule\"";

/// Moves schedule `$1`'s next due time from `$2` to `$3` past a tick that
/// skips it: once, by whichever instance gets there first.
pub const SKIP: &str = "UPDATE \"jadpo_schedules\" SET \"next_run_at\" = $3 WHERE \"schedule\" = $1 AND \"next_run_at\" = $2 RETURNING \"schedule\"";

/// Gives schedule `$1`'s lease back once run `$2` has ended, keeping
/// `$3`, the start of a run that succeeded, as its last success (`NULL`
/// keeps the one before).
pub const RELEASE: &str = "UPDATE \"jadpo_schedules\" SET \"lease_until\" = NULL, \"run_id\" = NULL, \"last_succeeded_at\" = COALESCE($3, \"last_succeeded_at\") WHERE \"schedule\" = $1 AND \"run_id\" = $2 RETURNING \"schedule\"";

/// A run record: its id, schedule, due, start and end, outcome, the reason
/// a tick skipped it, its failure, its events and its trace; the items it
/// counts come when it ends.
pub const RECORD: &str = "INSERT INTO \"jadpo_schedule_runs\" (\"id\", \"schedule\", \"due_slot\", \"started_at\", \"ended_at\", \"outcome\", \"reason\", \"failure\", \"events\", \"traceparent\") VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)";

/// How run `$1` ended at `$2`: its outcome `$3`, failure `$4`, events
/// `$5` and the items it processed `$6` (a purge's deleted records), while
/// it is still recorded as running.
pub const ENDED: &str = "UPDATE \"jadpo_schedule_runs\" SET \"ended_at\" = $2, \"outcome\" = $3, \"failure\" = $4, \"events\" = $5, \"items\" = $6 WHERE \"id\" = $1 AND \"outcome\" = 'running' RETURNING \"id\"";

/// Expired idempotency keys (area 7, "Expired idempotency keys"): a batch
/// of those past their `kept_until` at `$1`, at most `$2`.
pub const EXPIRED_KEYS: &str = "DELETE FROM \"jadpo_idempotency_keys\" WHERE (\"scope\", \"key\") IN (SELECT \"scope\", \"key\" FROM \"jadpo_idempotency_keys\" WHERE \"kept_until\" <= $1 LIMIT $2) RETURNING 1";

/// Events every subscriber has had, emitted at or before `$1`, at most
/// `$2`: their delivery records go with them, and so do the handled
/// records of those deliveries kept in this store, in the same statement,
/// since with its delivery record gone nothing can repeat a delivery
/// (chunk 6.10). An event with a delivery still to make, or that failed,
/// stays for the sweep or an operator; one an operator dismissed or
/// cancelled holds it until [`SETTLED_BY_HAND`] deletes that delivery,
/// so the event is listed beside it while it is kept (chunk 6.13). A row for each event and each of
/// its deliveries: the event's id, the delivery's (`NULL` for an event
/// nothing subscribes to), and whether its handled record was here.
pub const SETTLED: &str = "WITH \"gone\" AS (DELETE FROM \"jadpo_outbox\" WHERE \"id\" IN (SELECT \"o\".\"id\" FROM \"jadpo_outbox\" AS \"o\" WHERE \"o\".\"emitted_at\" <= $1 AND NOT EXISTS (SELECT 1 FROM \"jadpo_deliveries\" AS \"d\" WHERE \"d\".\"outbox_id\" = \"o\".\"id\" AND \"d\".\"status\" <> 'succeeded') ORDER BY \"o\".\"emitted_at\" LIMIT $2) RETURNING \"id\"), \"their_deliveries\" AS (SELECT \"d\".\"id\", \"d\".\"outbox_id\" FROM \"jadpo_deliveries\" AS \"d\" WHERE \"d\".\"outbox_id\" IN (SELECT \"id\" FROM \"gone\")), \"handled\" AS (DELETE FROM \"jadpo_handled_deliveries\" WHERE \"id\" IN (SELECT \"id\" FROM \"their_deliveries\") RETURNING \"id\") SELECT \"g\".\"id\", \"v\".\"id\", \"h\".\"id\" IS NOT NULL FROM \"gone\" AS \"g\" LEFT JOIN \"their_deliveries\" AS \"v\" ON \"v\".\"outbox_id\" = \"g\".\"id\" LEFT JOIN \"handled\" AS \"h\" ON \"h\".\"id\" = \"v\".\"id\"";

/// The handled records, among deliveries `$1`, that a subscriber writing
/// another store kept here, once [`SETTLED`] has deleted those deliveries
/// with their event in the event's store (chunk 6.10).
pub const HANDLED: &str =
    "DELETE FROM \"jadpo_handled_deliveries\" WHERE \"id\" = ANY($1::uuid[]) RETURNING 1";

/// Deliveries an operator dismissed, cancelled or abandoned (chunk 7.5c),
/// with no action on them logged after `$1`, at most `$2` (chunk 6.13):
/// none of those states is ever left, so none could run again. Their handled records kept in this
/// store go in the same statement, as [`SETTLED`]'s do. A row for each:
/// its id, and whether its handled record was here.
pub const SETTLED_BY_HAND: &str = "WITH \"gone\" AS (DELETE FROM \"jadpo_deliveries\" WHERE \"id\" IN (SELECT \"d\".\"id\" FROM \"jadpo_deliveries\" AS \"d\" WHERE \"d\".\"status\" IN ('dismissed', 'cancelled', 'abandoned') AND NOT EXISTS (SELECT 1 FROM \"jadpo_operator_actions\" AS \"a\" WHERE \"a\".\"item\" = \"d\".\"id\" AND \"a\".\"acted_at\" > $1) LIMIT $2) AND \"status\" IN ('dismissed', 'cancelled', 'abandoned') RETURNING \"id\"), \"handled\" AS (DELETE FROM \"jadpo_handled_deliveries\" WHERE \"id\" IN (SELECT \"id\" FROM \"gone\") RETURNING \"id\") SELECT \"g\".\"id\", \"h\".\"id\" IS NOT NULL FROM \"gone\" AS \"g\" LEFT JOIN \"handled\" AS \"h\" ON \"h\".\"id\" = \"g\".\"id\"";

/// An operator's log rows on deliveries, written at or before `$1`, whose
/// delivery is gone, at most `$2` (chunk 6.13): a delivery still on
/// record keeps the last thing done to it.
pub const OLD_DELIVERY_ACTIONS: &str = "DELETE FROM \"jadpo_operator_actions\" WHERE \"id\" IN (SELECT \"a\".\"id\" FROM \"jadpo_operator_actions\" AS \"a\" WHERE \"a\".\"kind\" = 'delivery' AND \"a\".\"acted_at\" <= $1 AND NOT EXISTS (SELECT 1 FROM \"jadpo_deliveries\" AS \"d\" WHERE \"d\".\"id\" = \"a\".\"item\") LIMIT $2) RETURNING 1";

/// An operator's log rows on schedules' runs, written at or before `$1`,
/// whose run record is gone (a run asked for and cancelled never had
/// one), at most `$2` (chunk 6.13): a dismissal is how a run is read as
/// settled, so it stays while its run does.
pub const OLD_RUN_ACTIONS: &str = "DELETE FROM \"jadpo_operator_actions\" WHERE \"id\" IN (SELECT \"a\".\"id\" FROM \"jadpo_operator_actions\" AS \"a\" WHERE \"a\".\"kind\" = 'schedule_run' AND \"a\".\"acted_at\" <= $1 AND NOT EXISTS (SELECT 1 FROM \"jadpo_schedule_runs\" AS \"r\" WHERE \"r\".\"id\" = \"a\".\"item\") LIMIT $2) RETURNING 1";

/// Browser sessions and API tokens that ended at or before `$1`, at most
/// `$2` (area 8; chunk 6.10): a lookup already ignores them.
pub const ENDED_SESSIONS: &str = "DELETE FROM \"jadpo_sessions\" WHERE \"id\" IN (SELECT \"id\" FROM \"jadpo_sessions\" WHERE \"ends_at\" <= $1 LIMIT $2) RETURNING 1";

/// Service keys that expired at or before `$1`, at most `$2` (area 8;
/// chunk 6.10).
pub const ENDED_SERVICE_KEYS: &str = "DELETE FROM \"jadpo_service_keys\" WHERE \"id\" IN (SELECT \"id\" FROM \"jadpo_service_keys\" WHERE \"ends_at\" <= $1 LIMIT $2) RETURNING 1";

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

/// One due schedule on one tick, or a run an operator started.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Due {
    /// The schedule's identity key.
    pub schedule: String,
    /// The due time the tick took, in milliseconds since the epoch.
    pub due_slot: i64,
    pub ran: Ran,
    /// When the run started and ended; a skipped tick's are the tick's
    /// time. Its lateness is `started - due_slot`, its duration
    /// `ended - started` (area 12, "Watching schedules": the metrics).
    pub started: i64,
    pub ended: i64,
    /// The run's `traceparent`; none for a skipped tick.
    pub trace: Option<String>,
    /// The items a run that counts them processed: the records a purge
    /// deleted (area 5; chunk 6.2c). None for an action's run.
    pub items: Option<u64>,
    /// How many records each of a purge's batches deleted, in order, which
    /// its log line carries (area 5); empty for any other run.
    pub batches: Vec<u64>,
    /// Whether an operator started it (`jadpo schedules run`), rather than
    /// its due time: its record's reason is `by_hand` (chunk 6.2d).
    pub by_hand: bool,
}

impl Due {
    /// How late it started after it was due, in milliseconds.
    pub fn delay_ms(&self) -> i64 {
        self.started.saturating_sub(self.due_slot).max(0)
    }

    /// How long the run took, in milliseconds.
    pub fn duration_ms(&self) -> i64 {
        self.ended.saturating_sub(self.started).max(0)
    }
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
    /// The alerts this tick raised: those that have started to hold since
    /// the last tick, which no other instance's tick raised first (chunk
    /// 6.9), and each circuit breaker's that opened (chunk 7.2b). A host
    /// reports each, once.
    pub raised: Vec<watch::Alert>,
    /// What it received from the app's queues, when it received anything
    /// (chunk 7.7c).
    pub received: Option<Tally>,
    /// How many polls it called, and how many of those saw a change, when
    /// it called any (chunk 7.6c).
    pub polled: Option<(u64, u64)>,
    /// The metric on the unfinished records of actions with several
    /// changing calls, for each store that keeps them: how many, and when
    /// the oldest started (chunk 7.5f).
    pub unfinished: Vec<(String, crate::unfinished::Metric)>,
}

/// How many rows a tick deleted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Deleted {
    /// Expired idempotency keys.
    pub keys: u64,
    pub events: u64,
    pub runs: u64,
    /// Subscribers' handled records, in every store (chunk 6.10).
    pub handled: u64,
    /// Ended browser sessions and API tokens.
    pub sessions: u64,
    /// Expired service keys.
    pub service_keys: u64,
    /// Deliveries an operator dismissed or cancelled (chunk 6.13); those
    /// deleted with their event are counted with it.
    pub deliveries: u64,
    /// An operator's log rows (chunk 6.13).
    pub actions: u64,
    /// The ids of events received from outside, once no sender would send
    /// one again (chunk 7.7).
    pub received: u64,
    /// Steps of unfinished records of several changing calls whose
    /// answers were dropped after 30 days (chunk 7.5c).
    pub answers: u64,
    /// Records of several changing calls an operator abandoned, 30 days
    /// on (chunk 7.5c).
    pub step_records: u64,
    /// Parked queue messages an operator dismissed 30 days before (chunk
    /// 7.7c).
    pub parked: u64,
    /// Counts of what `ai` calls used on days long past (chunk 5.5v5).
    pub ai_usage: u64,
}

/// How many queue messages one tick kept as events, dropped as repeats,
/// parked for an operator and left for the queue to send again (chunk
/// 7.7c).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Tally {
    pub kept: u64,
    pub repeats: u64,
    pub parked: u64,
    pub left: u64,
}

/// Runs one tick at `now` on `host`, within the budget its host gives a
/// tick (chunk 6.7): the schedules due, the runtime's own sweeps, then
/// [`HOOKS`], each only while the budget isn't spent; the next tick
/// carries on with what this one didn't reach. What it did.
pub async fn tick(executor: &dyn Executor, host: &dyn Host, now: i64) -> Ticked {
    let mut ticked = Ticked::default();
    let Some(plan) = executor.plan().schedules() else {
        return ticked;
    };
    let budget = Budget::start(host, Invocation::Tick);
    if budget.spent() {
        return ticked;
    }
    // A declared deployable runs only the background work it says it
    // runs (area 14, "Declared deployables"; chunk 8.1o2); the deletions
    // below are every instance's.
    let runs = executor.plan().runs;
    if let Some(store) = &plan.store
        && !plan.schedules.is_empty()
        && runs.schedules
    {
        match due(plan, host, store, now).await {
            Ok(states) => {
                let runs = states.into_iter().map(|(schedule, state)| {
                    take(executor, host, &budget, store, schedule, state, now)
                });
                let mut left = Vec::new();
                for (due, more) in join_all(runs.collect()).await.into_iter().flatten() {
                    ticked.due.push(due);
                    left.extend(more);
                }
                ticked.deliveries = runner::run_within(executor, host, &budget, left).await;
                if !budget.spent() {
                    match raise(plan, host, store, now).await {
                        Ok(raised) => ticked.raised = raised,
                        Err(error) => ticked.problems.push((store.clone(), error)),
                    }
                }
            }
            Err(error) => ticked.problems.push((store.clone(), error)),
        }
        match delete(host, store, OLD_RUNS, now.saturating_sub(RUNS_KEPT_MS)).await {
            Ok(count) => ticked.deleted.runs += count,
            Err(error) => ticked.problems.push((store.clone(), error)),
        }
        match delete(
            host,
            store,
            OLD_RUN_ACTIONS,
            now.saturating_sub(RUNS_KEPT_MS),
        )
        .await
        {
            Ok(count) => ticked.deleted.actions += count,
            Err(error) => ticked.problems.push((store.clone(), error)),
        }
    }
    for store in &plan.stores {
        if budget.spent() {
            return ticked;
        }
        if runs.subscribers {
            match runner::sweep_within(executor, host, &budget, store, now).await {
                Ok(outcomes) => ticked.deliveries.extend(outcomes),
                Err(error) => {
                    ticked.problems.push((store.clone(), error));
                    continue;
                }
            }
            // A circuit breaker that opened raises its alert once (chunk
            // 7.2b).
            match crate::breaker::raise(host, store, now).await {
                Ok(raised) => ticked.raised.extend(raised),
                Err(error) => ticked.problems.push((store.clone(), error)),
            }
        }
        match delete(host, store, EXPIRED_KEYS, now).await {
            Ok(count) => ticked.deleted.keys += count,
            Err(error) => ticked.problems.push((store.clone(), error)),
        }
        let kept = now.saturating_sub(RUNS_KEPT_MS);
        if let Err(problem) =
            settle_by_hand(host, &plan.stores, store, kept, &mut ticked.deleted).await
        {
            ticked.problems.push(problem);
        }
        let before = now.saturating_sub(SETTLED_KEPT_MS);
        if let Err(problem) = settle(host, &plan.stores, store, before, &mut ticked.deleted).await {
            ticked.problems.push(problem);
        }
        match delete(host, store, OLD_DELIVERY_ACTIONS, kept).await {
            Ok(count) => ticked.deleted.actions += count,
            Err(error) => ticked.problems.push((store.clone(), error)),
        }
        if plan.sessions.as_ref() == Some(store) {
            match delete(host, store, ENDED_SESSIONS, now).await {
                Ok(count) => ticked.deleted.sessions += count,
                Err(error) => ticked.problems.push((store.clone(), error)),
            }
        }
        if plan.keys.as_ref() == Some(store) {
            match delete(host, store, ENDED_SERVICE_KEYS, now).await {
                Ok(count) => ticked.deleted.service_keys += count,
                Err(error) => ticked.problems.push((store.clone(), error)),
            }
        }
        if plan.received.as_ref() == Some(store) {
            let before = now.saturating_sub(crate::webhooks::RECEIVED_KEPT_MS);
            match delete(host, store, crate::webhooks::OLD_RECEIVED, before).await {
                Ok(count) => ticked.deleted.received += count,
                Err(error) => ticked.problems.push((store.clone(), error)),
            }
        }
        // Parked queue messages an operator dismissed, and the log rows of
        // those gone (chunk 7.7c).
        if plan.parked.as_ref() == Some(store) {
            let before = now.saturating_sub(crate::receiving::DISMISSED_KEPT_MS);
            match delete(host, store, crate::receiving::OLD_DISMISSED, before).await {
                Ok(count) => ticked.deleted.parked += count,
                Err(error) => ticked.problems.push((store.clone(), error)),
            }
            match delete(host, store, crate::receiving::OLD_MESSAGE_ACTIONS, kept).await {
                Ok(count) => ticked.deleted.actions += count,
                Err(error) => ticked.problems.push((store.clone(), error)),
            }
        }
    }
    // The counts of what `ai` calls used, once `jadpo ai usage` no
    // longer shows their day (chunk 5.5v5).
    if let Some(store) = &plan.ai_usage
        && !budget.spent()
    {
        let before = now.saturating_sub(crate::ai_budget::KEPT_MS);
        match delete(host, store, crate::ai_budget::OLD_DAYS, before).await {
            Ok(count) => ticked.deleted.ai_usage += count,
            Err(error) => ticked.problems.push((store.clone(), error)),
        }
    }
    // Each queue's messages, kept as events and delivered within what is
    // left of the budget (chunk 7.7c).
    if !budget.spent() && runs.subscribers {
        let received = crate::receiving::receive(executor, &budget, now).await;
        ticked.problems.extend(received.problems);
        ticked
            .deliveries
            .extend(runner::run_within(executor, host, &budget, received.deferred).await);
        ticked.received = Some(Tally {
            kept: received.kept,
            repeats: received.repeats,
            parked: received.parked,
            left: received.left,
        })
        .filter(|tally| *tally != Tally::default());
    }
    // Each poll that is due, its changes kept as events and delivered
    // within what is left of the budget (chunk 7.6c).
    if !budget.spent() && runs.schedules {
        let polled = crate::polls::poll(executor, &budget, now).await;
        ticked.problems.extend(polled.problems);
        ticked
            .deliveries
            .extend(runner::run_within(executor, host, &budget, polled.deferred).await);
        ticked.polled = Some((polled.called, polled.changed)).filter(|&(called, _)| called > 0);
    }
    // Unfinished records of several changing calls keep their answers 30
    // days after their last run, then only their step list; an abandoned
    // one goes 30 days after it was abandoned (chunk 7.5c).
    let kept = now.saturating_sub(crate::unfinished::ANSWERS_KEPT_MS);
    for store in &plan.steps {
        if budget.spent() {
            return ticked;
        }
        // The metric on its unfinished records, and their alert once one
        // has been unfinished a day, raised once (chunk 7.5f).
        let raised = match host.database(store) {
            Some(database) => crate::unfinished::raise(database, store, now).await,
            None => Err(StoreError::Fault(format!("no store called `{store}`"))),
        };
        match raised {
            Ok((metric, alert)) => {
                ticked.unfinished.push((store.clone(), metric));
                ticked.raised.extend(alert);
            }
            Err(error) => ticked.problems.push((store.clone(), error)),
        }
        match delete(host, store, crate::unfinished::OLD_ANSWERS, kept).await {
            Ok(count) => ticked.deleted.answers += count,
            Err(error) => ticked.problems.push((store.clone(), error)),
        }
        match delete(host, store, crate::unfinished::ABANDONED, kept).await {
            Ok(count) => ticked.deleted.step_records += count,
            Err(error) => ticked.problems.push((store.clone(), error)),
        }
        match delete(host, store, crate::unfinished::OLD_ACTIONS, kept).await {
            Ok(count) => ticked.deleted.actions += count,
            Err(error) => ticked.problems.push((store.clone(), error)),
        }
    }
    // Batch runs are picked up again where schedules run.
    for hook in HOOKS {
        if budget.spent() || !runs.schedules {
            break;
        }
        let left = hook(executor, host, &budget, now).await;
        ticked
            .deliveries
            .extend(runner::run_within(executor, host, &budget, left).await);
    }
    ticked
}

/// Raises at `now` the alerts that have started to hold for `plan`'s
/// schedules, whose state is in `store` ([`watch::raise`]).
async fn raise(
    plan: &SchedulesPlan,
    host: &dyn Host,
    store: &str,
    now: i64,
) -> Result<Vec<watch::Alert>, StoreError> {
    let database = host
        .database(store)
        .ok_or_else(|| StoreError::Fault(format!("no store called `{store}`")))?;
    let declared: Vec<watch::Declared> = plan.schedules.iter().map(watch::Declared::from).collect();
    watch::raise(database, &declared, tick_every_ms(Some(plan)), now).await
}

/// How often a host's own timer ticks for `plan`: once a minute, or as
/// often as its shortest `every:` when that is shorter, at least once a
/// second.
pub fn tick_every_ms(plan: Option<&SchedulesPlan>) -> u64 {
    let minute = 60_000;
    // A queue is received on the tick, so an app that receives one ticks
    // at least every 10 seconds (chunk 7.7c).
    let most = if plan.is_some_and(|plan| plan.parked.is_some()) {
        QUEUE_TICK_MS
    } else {
        minute
    };
    plan.and_then(SchedulesPlan::shortest_every)
        .map_or(most, |shortest| shortest.clamp(1_000, most))
}

/// How often at least a host ticks while its app receives a queue.
pub const QUEUE_TICK_MS: u64 = 10_000;

/// The state of a schedule's row, as a tick reads it.
#[derive(Clone, Copy, Debug)]
struct State {
    every_ms: i64,
    next_run_at: i64,
    paused: bool,
    lease_until: Option<i64>,
    run: Option<[u8; 16]>,
    /// The run an operator asked for, still waiting for a tick.
    requested: Option<[u8; 16]>,
}

/// Records the tick on every schedule's row, making the rows it lacks,
/// and gives each schedule due at `now`, or with a run an operator asked
/// for, with its state, interval changes applied.
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
        if state.next_run_at <= now || state.requested.is_some() {
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
        requested: match row.get(6) {
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

/// Takes `schedule` at `now` within `budget`: its due run, then the run an
/// operator asked for when the due run left the lease free (skipped as
/// paused, or not due), so a tick runs a schedule once at most. How each
/// went and the work its run left; nothing for one another instance took
/// first.
async fn take(
    executor: &dyn Executor,
    host: &dyn Host,
    budget: &Budget<'_>,
    store: &str,
    schedule: &SchedulePlan,
    state: State,
    now: i64,
) -> Vec<(Due, Vec<Deferred>)> {
    let mut out = Vec::new();
    let mut free = !state.lease_until.is_some_and(|until| until > now);
    if state.next_run_at <= now {
        let taken = take_due(executor, host, budget, store, schedule, state, now).await;
        free = free
            && matches!(
                taken.as_ref().map(|(due, _)| &due.ran),
                Some(Ran::Skipped(Skipped::Paused))
            );
        out.extend(taken);
    }
    if let Some(run) = state.requested
        && free
    {
        out.extend(serve(executor, budget, store, schedule, state, run, now).await);
    }
    out
}

/// Runs by hand, at `now` under the lease, the run `run` an operator asked
/// `schedule`'s tick for (`jadpo schedules run` against a deployment;
/// chunk 6.2d), paused or not, its due time left as it is: recorded with
/// `run` as its id and `by_hand` as its reason, so the command waiting for
/// it finds it. `None` when another instance took it first.
async fn serve(
    executor: &dyn Executor,
    budget: &Budget<'_>,
    store: &str,
    schedule: &SchedulePlan,
    state: State,
    run: [u8; 16],
    now: i64,
) -> Option<(Due, Vec<Deferred>)> {
    let host = budget.host();
    let database = host.database(store)?;
    let lease = Leased {
        key: schedule.key.clone(),
        run,
        take: Take::Requested,
        succeeded: None,
    };
    crate::lease::claim(database, budget, &lease, now)
        .await
        .ok()??;
    let env = Env {
        seed: None,
        clock: Clock::fixed(now),
    };
    let ctx = Ctx::of(executor, host, env);
    let claimed = Claimed {
        lease,
        due_slot: now,
        lost: state.run.map(|lost| (lost, state.lease_until)),
        reason: Some(BY_HAND),
    };
    Some(run_claimed(executor, budget, database, schedule, claimed, ctx, now).await)
}

/// Takes due `schedule` at `now`: skipped while paused or leased, run
/// once its lease is this tick's. `None` when another instance took it
/// first; otherwise how it went and the work its run left.
async fn take_due(
    executor: &dyn Executor,
    host: &dyn Host,
    budget: &Budget<'_>,
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
    let mut ctx = Ctx::of(executor, host, env);
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
                started: now,
                ended: now,
                trace: None,
                items: None,
                batches: Vec::new(),
                by_hand: false,
            },
            Vec::new(),
        ));
    }
    let lease = Leased {
        key: schedule.key.clone(),
        run: ctx.new_id()?,
        take: Take::Due {
            slot: due_slot,
            next,
        },
        succeeded: None,
    };
    crate::lease::claim(database, budget, &lease, now)
        .await
        .ok()??;
    let lost = state.run.map(|lost| (lost, state.lease_until));
    let claimed = Claimed {
        lease,
        due_slot,
        lost,
        reason: None,
    };
    Some(run_claimed(executor, budget, database, schedule, claimed, ctx, now).await)
}

/// Who ran `ctx` before [`test_run`] made it the system, to put back.
#[derive(Debug)]
pub struct TestRun((Option<crate::Caller>, Option<System>, bool));

/// Makes `ctx` the system with no tenant, as a schedule's run is, for a
/// test's `schedule.run()` (area 13, "Kinds of test"; chunk 6.2c): the
/// test's clock stays as it is, fixed for the run. Only a test's executor
/// calls this; [`test_run_ended`] puts back who ran `ctx` before.
pub fn test_run(ctx: &mut Ctx<'_>) -> TestRun {
    let saved = ctx.runner();
    ctx.become_system(System { tenant: None });
    TestRun(saved)
}

/// Ends a test's run of a schedule: whoever ran `ctx` before runs it again.
pub fn test_run_ended(ctx: &mut Ctx<'_>, saved: TestRun) {
    ctx.restore_runner(saved.0);
}

/// A schedule's state row as a run leases it ([`crate::lease`]; chunk
/// 6.7): taken for run `run` as `take` says, and given back once the run
/// has ended, keeping `succeeded`, the start of a run that succeeded, as
/// its last success.
pub(crate) struct Leased {
    pub key: String,
    pub run: [u8; 16],
    pub take: Take,
    pub succeeded: Option<i64>,
}

/// How a run takes its schedule's lease.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Take {
    /// The tick's due run, moving its due time from `slot` to `next`.
    Due { slot: i64, next: i64 },
    /// The run an operator asked the tick for ([`CLAIM_REQUESTED`]).
    Requested,
    /// A run `jadpo schedules run` starts now ([`watch::CLAIM_NOW`]).
    Now,
}

impl crate::lease::Leased for Leased {
    fn claim(&self, now: i64, until: i64) -> (&'static str, Vec<SqlValue>) {
        let key = SqlValue::Text(self.key.clone());
        let run = SqlValue::Uuid(self.run);
        match self.take {
            Take::Due { slot, next } => (
                CLAIM,
                vec![
                    key,
                    micros(slot),
                    micros(next),
                    micros(until),
                    run,
                    micros(now),
                ],
            ),
            Take::Requested => (CLAIM_REQUESTED, vec![key, run, micros(until), micros(now)]),
            Take::Now => (watch::CLAIM_NOW, vec![key, micros(until), run, micros(now)]),
        }
    }

    fn release(&self, _now: i64) -> (&'static str, Vec<SqlValue>) {
        (
            RELEASE,
            vec![
                SqlValue::Text(self.key.clone()),
                SqlValue::Uuid(self.run),
                self.succeeded
                    .map_or(SqlValue::Null(SqlType::Timestamp), micros),
            ],
        )
    }
}

/// A run whose lease has been taken: the lease, the due time it runs for,
/// the run that held the lease before it and never ended, with that
/// lease's end, and the reason its record gives (`by_hand` for an
/// operator's).
pub(crate) struct Claimed {
    pub lease: Leased,
    pub due_slot: i64,
    pub lost: Option<([u8; 16], Option<i64>)>,
    pub reason: Option<&'static str>,
}

/// Runs `schedule` under the lease `claimed` holds, at `now`, within
/// `budget`: its record written as running, the action run as the system
/// with no tenant in a new trace, then its record ended and the lease
/// given back. How it went and the work its run left.
pub(crate) async fn run_claimed(
    executor: &dyn Executor,
    budget: &Budget<'_>,
    database: &dyn crate::Database,
    schedule: &SchedulePlan,
    claimed: Claimed,
    mut ctx: Ctx<'_>,
    now: i64,
) -> (Due, Vec<Deferred>) {
    let Claimed {
        mut lease,
        due_slot,
        lost,
        reason,
    } = claimed;
    let host = budget.host();
    let run = lease.run;
    let key = SqlValue::Text(schedule.key.clone());
    // A run that held the lease before and never ended was lost with its
    // worker: its record says so.
    if let Some((lost, lease_until)) = lost {
        let ended_at = lease_until.unwrap_or(now).min(now);
        let _ = database
            .query_once(
                ENDED,
                &[
                    SqlValue::Uuid(lost),
                    micros(ended_at),
                    SqlValue::Text(FAILED.into()),
                    failure_json("lease_expired", None),
                    SqlValue::Null(SqlType::Int4),
                    SqlValue::Null(SqlType::Int4),
                ],
            )
            .await;
    }
    let began = host.monotonic_ms();
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
                reason.map_or(SqlValue::Null(SqlType::Text), |reason| {
                    SqlValue::Text(reason.into())
                }),
                SqlValue::Null(SqlType::Json),
                SqlValue::Null(SqlType::Int4),
                SqlValue::Text(trace.header()),
            ],
        )
        .await;
    ctx.set_trace(Some(trace));
    let (ran, purged) = match &schedule.run {
        ScheduleRun::Action(callable) => (
            run_action(executor, budget, &mut ctx, *callable).await,
            None,
        ),
        ScheduleRun::Purge(entity) => run_purge(executor, budget, entity, now).await,
    };
    // Measured on the monotonic clock from the run's own time, so the
    // record's duration is how long it took whatever the wall clock did;
    // a host with no timer reads its wall clock.
    let ended_at = match (began, host.monotonic_ms()) {
        (Some(began), Some(end)) => {
            now.saturating_add(i64::try_from(end.saturating_sub(began)).unwrap_or(i64::MAX))
        }
        _ => host.now_ms().unwrap_or(now).max(now),
    };
    let items = purged.as_ref().map(crate::purge::Purged::removed);
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
                items.map_or(SqlValue::Null(SqlType::Int4), |items| {
                    SqlValue::Int4(i32::try_from(items).unwrap_or(i32::MAX))
                }),
            ],
        )
        .await;
    lease.succeeded = match &ran {
        Ran::Succeeded { .. } => Some(now),
        _ => None,
    };
    crate::lease::release(database, &lease, now).await;
    (
        Due {
            schedule: schedule.key.clone(),
            due_slot,
            ran,
            started: now,
            ended: ended_at,
            trace: Some(trace.header()),
            items,
            batches: purged.map(|purged| purged.batches).unwrap_or_default(),
            by_hand: reason == Some(BY_HAND),
        },
        ctx.take_deferred(),
    )
}

/// A schedule's action, called with nothing as the system with no tenant
/// in `ctx`, within `budget` as the runner runs any action: how it went.
/// One its limit or its host's shutdown stopped ran out of time
/// (`deadline_exceeded`), nothing it did kept.
async fn run_action(
    executor: &dyn Executor,
    budget: &Budget<'_>,
    ctx: &mut Ctx<'_>,
    callable: u32,
) -> Ran {
    let job = Job {
        callable,
        args: Vec::new(),
        system: System { tenant: None },
    };
    let outcome = match runner::within_budget(executor, ctx, budget, job).await {
        Ok(outcome) => outcome,
        Err(_) => Outcome::Fault(ctx.fault(FaultKind::DeadlineExceeded, Frame::ENTRY)),
    };
    match &outcome {
        Outcome::Value(_) => Ran::Succeeded {
            events: ctx.background().emitted(),
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
    }
}

/// The built-in purge of `entity` at `now` (area 5, "Lifecycle"; chunk
/// 6.2c): the compiler's own operation, outside every action and policy,
/// [`crate::PURGE_BATCHES`] batches at most, run within `budget`: stopped
/// at the limit of a run, or before the budget ends. How it went and what
/// it deleted. A store that can't be reached is `unavailable`, as for an
/// action; anything else the store refuses is an `internal_fault`, the
/// batches before it staying deleted.
async fn run_purge(
    executor: &dyn Executor,
    budget: &Budget<'_>,
    entity: &str,
    now: i64,
) -> (Ran, Option<crate::purge::Purged>) {
    let host = budget.host();
    let failed = |code: &str, kind: Option<&str>| Ran::Failed {
        code: code.to_string(),
        kind: kind.map(str::to_string),
    };
    // How long it waits, as the program started with it.
    let started = executor.started().unwrap_or_else(crate::started::unstarted);
    let Some((plan, after)) = executor.plan().purge(entity, &started) else {
        return (failed("internal_fault", None), None);
    };
    let run = crate::purge::purge_after(host, plan, now, crate::PURGE_BATCHES, after.unwrap_or(0));
    match budget.run(run).await {
        Ok(Ok(purged)) => (Ran::Succeeded { events: 0 }, Some(purged)),
        Ok(Err(StoreError::Unavailable)) => (failed("unavailable", Some("unavailable")), None),
        Ok(Err(_)) => (failed("internal_fault", None), None),
        Err(_) => (failed("deadline_exceeded", None), None),
    }
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

/// Deletes in `store` the events every subscriber had, emitted at or
/// before `before`, with their delivery records and the handled records
/// this store kept of them ([`SETTLED`]), then the handled records of
/// those deliveries in each other store of `stores` ([`HANDLED`]): a
/// subscriber's handled record goes only once its delivery's record has,
/// since nothing repeats a delivery then (chunk 6.10). [`DELETE_BATCH`]
/// events at a time, at most [`DELETE_BATCHES`] times. A store that fails
/// stops it, named with why; the next tick carries on.
async fn settle(
    host: &dyn Host,
    stores: &[String],
    store: &str,
    before: i64,
    deleted: &mut Deleted,
) -> Result<(), (String, StoreError)> {
    let database = open(host, store)?;
    for _ in 0..DELETE_BATCHES {
        let rows = database
            .query_once(SETTLED, &[micros(before), SqlValue::Int8(DELETE_BATCH)])
            .await
            .map_err(|error| (store.to_string(), error))?;
        let mut events = std::collections::BTreeSet::new();
        let mut elsewhere = Vec::new();
        for row in &rows {
            let [SqlValue::Uuid(event), delivery, SqlValue::Bool(here)] = row.as_slice() else {
                continue;
            };
            events.insert(*event);
            match delivery {
                SqlValue::Uuid(_) if *here => deleted.handled += 1,
                SqlValue::Uuid(id) => elsewhere.push(SqlValue::Uuid(*id)),
                _ => {}
            }
        }
        deleted.events += events.len() as u64;
        deleted.handled += handled_elsewhere(host, stores, store, &elsewhere).await?;
        if (events.len() as i64) < DELETE_BATCH {
            break;
        }
    }
    Ok(())
}

/// Deletes in `store` the deliveries an operator dismissed or cancelled
/// with no action on them logged after `before`, with the handled records
/// this store kept of them ([`SETTLED_BY_HAND`]), then those of them in
/// each other store of `stores` ([`HANDLED`]) (chunk 6.13). Their events
/// stay for [`settle`], which takes each once no delivery of it is left
/// that could run. [`DELETE_BATCH`] deliveries at a time, at most
/// [`DELETE_BATCHES`] times; a store that fails stops it, named with why.
async fn settle_by_hand(
    host: &dyn Host,
    stores: &[String],
    store: &str,
    before: i64,
    deleted: &mut Deleted,
) -> Result<(), (String, StoreError)> {
    let database = open(host, store)?;
    for _ in 0..DELETE_BATCHES {
        let rows = database
            .query_once(
                SETTLED_BY_HAND,
                &[micros(before), SqlValue::Int8(DELETE_BATCH)],
            )
            .await
            .map_err(|error| (store.to_string(), error))?;
        let mut elsewhere = Vec::new();
        for row in &rows {
            match row.as_slice() {
                [SqlValue::Uuid(_), SqlValue::Bool(true)] => deleted.handled += 1,
                [SqlValue::Uuid(id), _] => elsewhere.push(SqlValue::Uuid(*id)),
                _ => continue,
            }
        }
        deleted.deliveries += rows.len() as u64;
        deleted.handled += handled_elsewhere(host, stores, store, &elsewhere).await?;
        if (rows.len() as i64) < DELETE_BATCH {
            break;
        }
    }
    Ok(())
}

/// Deletes the handled records of deliveries `ids`, whose records `store`
/// has just deleted, in each other store of `stores` ([`HANDLED`]): how
/// many there were.
async fn handled_elsewhere(
    host: &dyn Host,
    stores: &[String],
    store: &str,
    ids: &[SqlValue],
) -> Result<u64, (String, StoreError)> {
    let mut gone = 0;
    if ids.is_empty() {
        return Ok(gone);
    }
    for other in stores.iter().filter(|other| other.as_str() != store) {
        let database = open(host, other)?;
        for ids in ids.chunks(DELETE_BATCH as usize) {
            let rows = database
                .query_once(HANDLED, &[SqlValue::Array(SqlType::Uuid, ids.to_vec())])
                .await
                .map_err(|error| (other.clone(), error))?;
            gone += rows.len() as u64;
        }
    }
    Ok(gone)
}

/// The store called `name`, or why not, named.
fn open<'h>(host: &'h dyn Host, name: &str) -> Result<&'h dyn Database, (String, StoreError)> {
    host.database(name).ok_or_else(|| {
        (
            name.to_string(),
            StoreError::Fault(format!("no store called `{name}`")),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::Future;
    use std::pin::Pin;

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
                    run: ScheduleRun::Action(0),
                    file: String::new(),
                    line: 1,
                })
                .collect(),
            store: Some("main".into()),
            stores: vec!["main".into()],
            ..SchedulesPlan::default()
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
