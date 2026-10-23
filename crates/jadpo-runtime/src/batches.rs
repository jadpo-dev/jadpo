//! Batch runs (area 12, "Batches: work over many items"; chunk 6.4), on
//! the runner's per-item mode ([`crate::runner::Batch`]).
//!
//! - **Starting.** `batches.each` in an action ([`start`]) writes the run's
//!   row in the action's own transaction: which call it is, the shape it
//!   saved under, its `key:` and the values it saves (each `from:`
//!   argument and each value `each:` and `finished:` capture, as their
//!   types' JSON). With a `key:` another start of the same call finds the
//!   row its key already has and gives that run instead. The action leaves
//!   the run for after its response ([`crate::runner::Deferred::Batch`]);
//!   if it rolls back the row was never kept, and the runner finds nothing
//!   to run.
//! - **Running.** The runner ([`run`]) claims the run with a lease, then
//!   goes a page of `from:` at a time: each page is one transaction, as
//!   the system, that locks the run's row, reads the page after the
//!   position the row keeps, runs `each:`'s action per item (one that
//!   fails is undone alone and becomes a problem, never its value), writes
//!   the problems, the position and the counts, and commits. After the
//!   last page `finished:` runs in a transaction of its own and the run is
//!   finished. A crash or a deploy anywhere loses only the batch in hand:
//!   the lease lapses, and [`resume`] picks the run up from the position
//!   that committed. A runner works within its invocation's budget (chunk
//!   6.7): its lease lasts what is left of the budget and a margin,
//!   renewed with each page it commits, and once the budget is spent it
//!   gives the lease back and hands the run on to a fresh invocation.
//! - **Asking.** `batches.status` reads the row ([`status`]);
//!   `batches.cancel` keeps a row of its own ([`cancel`]), which the next
//!   batch finds, so asking never waits for the batch in hand.
//!
//! What a run reads is the plan's ([`BatchPlan`]): the same for the
//! interpreter and compiled code, which give it through
//! [`crate::Executor::batches`]. A file helper's read (`csv.read` and the
//! others; chunk 6.4b) walks a stored file's rows instead of a query's
//! pages, each page read from the file before its transaction opens
//! ([`file`]); its write (`csv.write` and the others; chunk 6.4b2) walks a
//! query's pages into a stored file, each page's rows put in the store
//! once the page's transaction has closed ([`export`]).

pub(crate) mod calling;
pub(crate) mod export;
pub(crate) mod file;

pub use file::MAX_ROW_BYTES;

use std::ops::ControlFlow;
use std::rc::Rc;

use jadpo_std::BatchState;
use jadpo_std::batch::{BatchArg, BatchFrom, BatchPlan, BatchTenant};
use jadpo_std::types::FailurePlan;
use jadpo_value::Value;

use crate::budget::{Budget, Invocation};
use crate::read::{JsonCodecs, uuid_text};
use crate::runner::{Batch, Deferred};
use crate::{
    Clock, Ctx, Env, Executor, FaultKind, Frame, Host, Outcome, Row, SqlType, SqlValue, StoreError,
    StoreOp, Stored, System,
};

/// What a program's batches run with: each `batches.each` call's plan, the
/// codecs their saved values are kept in, and the failure plans a read
/// raises from.
#[derive(Clone, Copy, Debug)]
pub struct Batches<'a> {
    pub plans: &'a [BatchPlan],
    pub json: JsonCodecs<'a>,
    pub failures: &'a [FailurePlan],
}

impl<'a> Batches<'a> {
    /// The plan of the call a run kept by its name.
    fn plan(&self, name: &str) -> Option<&'a BatchPlan> {
        self.plans.iter().find(|plan| plan.name == name)
    }

    /// The store the program keeps its runs in: the one every batch's
    /// query reads, which the checker holds `batches.status` and
    /// `batches.cancel` to.
    fn store(&self) -> Option<&'a str> {
        self.plans.first().map(|plan| plan.store.as_str())
    }
}

/// A run the runner is to run once the action that started it has
/// committed, or that a sweep found waiting.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BatchStart {
    pub store: String,
    pub run: [u8; 16],
}

/// How many times one batch is tried again after a concurrent change
/// before the runner leaves the run for a later sweep.
const BATCH_ATTEMPTS: u32 = crate::RERUN_ATTEMPTS;

/// The most runs one sweep picks up.
const SWEEP_LIMIT: i64 = 100;

/// How many of a file's rows a file helper's run reads into one batch,
/// unless its host says otherwise ([`crate::Host::rows_per_batch`]): as
/// many as a query's largest page (`PageSize`'s 100).
pub const ROWS_PER_BATCH: u32 = 100;

/// Starts a run, or finds the one its call already started with its
/// `key:` in its tenant (chunk 6.6c), keeping the encoding its saved
/// values are written in (chunk 6.8).
pub(crate) const START: &str = "WITH \"started\" AS (INSERT INTO \"jadpo_batch_runs\" (\"id\", \"batch\", \"shape\", \"key\", \"captured\", \"state\", \"position\", \"read_all\", \"items_done\", \"problems\", \"failure\", \"lease_until\", \"started_at\", \"stepped_at\", \"finished_at\", \"depth\", \"tenant\", \"encoding\") VALUES ($1, $2, $3, $4, $5, 'running', NULL, FALSE, 0, 0, NULL, NULL, $6, $6, NULL, $7, $8, $9) ON CONFLICT (\"batch\", \"tenant\", \"key\") DO NOTHING RETURNING \"id\") SELECT \"id\" FROM \"started\" UNION ALL SELECT \"id\" FROM \"jadpo_batch_runs\" WHERE \"batch\" = $2 AND \"tenant\" = $8 AND \"key\" = $4 AND NOT EXISTS (SELECT 1 FROM \"started\")";

pub(crate) const CLAIM: &str = "UPDATE \"jadpo_batch_runs\" SET \"lease_until\" = $2 WHERE \"id\" = $1 AND \"state\" = 'running' AND (\"lease_until\" IS NULL OR \"lease_until\" < $3) RETURNING \"batch\", \"shape\", \"captured\", \"key\", \"depth\", \"encoding\"";

pub(crate) const LOCK: &str = "SELECT \"state\", \"position\", \"read_all\", \"items_done\", \"problems\", EXISTS (SELECT 1 FROM \"jadpo_batch_cancels\" WHERE \"run_id\" = $1) FROM \"jadpo_batch_runs\" WHERE \"id\" = $1 FOR NO KEY UPDATE";

/// A page's commit: the run's position, its counts and its lease, and,
/// when the page worked one out, the total it estimates (chunk 6.4b3);
/// `NULL` keeps the estimate it had.
pub(crate) const ADVANCE: &str = "UPDATE \"jadpo_batch_runs\" SET \"position\" = $2, \"read_all\" = $3, \"items_done\" = $4, \"problems\" = $5, \"lease_until\" = $6, \"stepped_at\" = $7, \"estimated_total\" = COALESCE($8, \"estimated_total\") WHERE \"id\" = $1";

pub(crate) const PROBLEM: &str = "INSERT INTO \"jadpo_batch_problems\" (\"run_id\", \"row\", \"column\", \"rule\") VALUES ($1, $2, $3, $4)";

pub(crate) const END: &str = "UPDATE \"jadpo_batch_runs\" SET \"state\" = $2, \"failure\" = $3, \"lease_until\" = NULL, \"stepped_at\" = $4, \"finished_at\" = $4 WHERE \"id\" = $1";

pub(crate) const RELEASE: &str = "UPDATE \"jadpo_batch_runs\" SET \"lease_until\" = NULL WHERE \"id\" = $1 AND \"state\" = 'running' RETURNING \"id\"";

/// A batch run's row as its runner leases it ([`crate::lease`]; chunk
/// 6.7): claimed once its lease has lapsed, renewed by each page's
/// commit ([`ADVANCE`]), and given back once the runner's budget is
/// spent.
struct Leased {
    id: [u8; 16],
}

impl crate::lease::Leased for Leased {
    fn claim(&self, now: i64, until: i64) -> (&'static str, Vec<SqlValue>) {
        (CLAIM, vec![SqlValue::Uuid(self.id), at(until), at(now)])
    }

    fn release(&self, _now: i64) -> (&'static str, Vec<SqlValue>) {
        (RELEASE, vec![SqlValue::Uuid(self.id)])
    }
}

/// Every run still going, oldest first: its id, call, shape digest and
/// start, which `jadpo runs unfinished` compares with a new build (chunk
/// 7.5c).
pub const UNFINISHED: &str = "SELECT \"id\", \"batch\", \"shape\", \"started_at\" FROM \"jadpo_batch_runs\" WHERE \"state\" = 'running' ORDER BY \"started_at\", \"id\"";

/// A run still going, as [`UNFINISHED`] reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unfinished {
    pub id: [u8; 16],
    /// Which call it is, by its stable name.
    pub batch: String,
    pub shape: String,
    pub started_at: i64,
}

/// Every run of one store still going, oldest first (chunk 7.5c).
pub async fn unfinished(
    database: &dyn crate::Database,
) -> Result<Vec<Unfinished>, crate::StoreError> {
    let rows = database.query_once(UNFINISHED, &[]).await?;
    Ok(rows
        .iter()
        .filter_map(|row| match row.as_slice() {
            [
                SqlValue::Uuid(id),
                SqlValue::Text(batch),
                SqlValue::Text(shape),
                SqlValue::Timestamp(started),
            ] => Some(Unfinished {
                id: *id,
                batch: batch.clone(),
                shape: shape.clone(),
                started_at: started.div_euclid(1000),
            }),
            _ => None,
        })
        .collect())
}

pub(crate) const STATUS: &str = "SELECT \"items_done\", \"problems\", \"state\", \"estimated_total\" FROM \"jadpo_batch_runs\" WHERE \"id\" = $1";

pub(crate) const PROBLEMS: &str = "SELECT \"row\", \"column\", \"rule\" FROM \"jadpo_batch_problems\" WHERE \"run_id\" = $1 ORDER BY \"row\"";

pub(crate) const CANCEL: &str = "INSERT INTO \"jadpo_batch_cancels\" (\"run_id\", \"requested_at\") VALUES ($1, $2) ON CONFLICT (\"run_id\") DO NOTHING";

/// Runs that ended (finished, failed or cancelled) at or before `$1`, at
/// most `$2`, with their cancels; their problems go with them.
pub(crate) const ENDED: &str = "WITH \"gone\" AS (DELETE FROM \"jadpo_batch_runs\" WHERE \"id\" IN (SELECT \"id\" FROM \"jadpo_batch_runs\" WHERE \"state\" <> 'running' AND \"finished_at\" <= $1 ORDER BY \"finished_at\" LIMIT $2) RETURNING \"id\"), \"asked\" AS (DELETE FROM \"jadpo_batch_cancels\" WHERE \"run_id\" IN (SELECT \"id\" FROM \"gone\")) SELECT 1 FROM \"gone\"";

/// Cancels asked for at or before `$1` of runs no store keeps, at most
/// `$2`: a run the caller held after it was deleted.
pub(crate) const UNKEPT_CANCELS: &str = "DELETE FROM \"jadpo_batch_cancels\" WHERE \"run_id\" IN (SELECT \"c\".\"run_id\" FROM \"jadpo_batch_cancels\" AS \"c\" WHERE \"c\".\"requested_at\" <= $1 AND NOT EXISTS (SELECT 1 FROM \"jadpo_batch_runs\" AS \"r\" WHERE \"r\".\"id\" = \"c\".\"run_id\") LIMIT $2) RETURNING 1";

/// How long a run that ended is kept, with its problems and its cancel,
/// before the tick deletes it (chunk 6.6a): as long as a schedule's run
/// records.
pub const ENDED_KEPT_MS: i64 = crate::schedules::RUNS_KEPT_MS;

pub(crate) const WAITING: &str = "SELECT \"id\" FROM \"jadpo_batch_runs\" WHERE \"state\" = 'running' AND (\"lease_until\" IS NULL OR \"lease_until\" < $1) ORDER BY \"started_at\", \"id\" LIMIT $2";

/// A time as a `timestamptz` parameter.
fn at(millis: i64) -> SqlValue {
    SqlValue::Timestamp(millis.saturating_mul(1000))
}

/// The JSON array the values a run saves are kept in: each value's own
/// JSON, through its type's codec, `null` for none.
fn saved(json: JsonCodecs<'_>, plan: &BatchPlan, values: &[Value]) -> Result<String, String> {
    let mut out = String::from("[");
    for (place, (number, value)) in plan.captured.iter().zip(values).enumerate() {
        if place > 0 {
            out.push(',');
        }
        match json.write(*number, value)? {
            SqlValue::Json(text) => out.push_str(&text),
            _ => out.push_str("null"),
        }
    }
    out.push(']');
    Ok(out)
}

/// The values a run saved, read back through their types' codecs with
/// the standard JSON reader (chunk 6.8); why not, never a value.
fn read_saved(json: JsonCodecs<'_>, plan: &BatchPlan, text: &str) -> Result<Vec<Value>, String> {
    json.read_list(&plan.captured, text)
}

/// A run's id from the `BatchRun` a program holds.
fn run_id(run: &Value) -> Option<[u8; 16]> {
    let Value::Record(record) = run else {
        return None;
    };
    match crate::to_sql(record.get("id")?, SqlType::Uuid) {
        Ok(SqlValue::Uuid(bytes)) => Some(bytes),
        _ => None,
    }
}

/// The `BatchRun` a program holds for the run `id`.
fn run_value(id: &[u8; 16]) -> Value {
    Value::record(
        jadpo_std::batch::BATCH_RUN,
        vec![(Rc::from("id"), Value::uuid(&uuid_text(id)))],
    )
}

fn fault(ctx: &mut Ctx<'_>, what: impl Into<String>, site: u32) -> Stored {
    Stored::Fault(ctx.fault(FaultKind::Defect(what.into()), site))
}

/// A store's refusal at `site` of the action calling a batch operation.
fn refused(ctx: &mut Ctx<'_>, error: StoreError, site: u32) -> Stored {
    match crate::read::refused(ctx, error, site) {
        crate::read::Interrupt::Failed(failure) => Stored::Failed(failure),
        crate::read::Interrupt::Fault(fault) => Stored::Fault(fault),
    }
}

/// `batches.each`, at `site` of the running action: keeps the run's row
/// in the action's transaction and leaves the run for after its response.
/// `values` are what the call saves, in the plan's order, then its `key:`.
pub async fn start(
    ctx: &mut Ctx<'_>,
    batches: Batches<'_>,
    plan: &BatchPlan,
    values: &[Value],
    site: u32,
) -> Stored {
    let Some((captured, key)) = values.split_at_checked(plan.captured.len()) else {
        return fault(ctx, "a batch started without the values it saves", site);
    };
    // The tenant it is limited to, which its `key:` is unique within
    // (chunk 6.6c).
    let tenant =
        tenant_of(plan, captured, key_value(plan, key)).unwrap_or(crate::outbox::NO_TENANT);
    let text = match saved(batches.json, plan, captured) {
        Ok(text) => text,
        Err(why) => {
            return fault(
                ctx,
                format!("a batch's saved values couldn't be kept: {why}"),
                site,
            );
        }
    };
    let key = match (plan.key, key.first()) {
        (Some(number), Some(value)) if !matches!(value, Value::None) => {
            match batches.json.write(number, value) {
                Ok(SqlValue::Json(text)) => SqlValue::Text(text),
                Ok(_) => SqlValue::Null(SqlType::Text),
                Err(why) => {
                    return fault(ctx, format!("a batch's key couldn't be kept: {why}"), site);
                }
            }
        }
        _ => SqlValue::Null(SqlType::Text),
    };
    let Some(id) = ctx.new_id() else {
        return fault(
            ctx,
            "a new run's identity with no randomness to make it",
            site,
        );
    };
    let now = ctx.now();
    let params = [
        SqlValue::Uuid(id),
        SqlValue::Text(plan.name.clone()),
        SqlValue::Text(shape_digest(plan)),
        key,
        SqlValue::Json(text),
        at(now),
        // How deep in a chain of events it was started, which its items
        // run at (chunk 6.6a).
        SqlValue::Int4(i32::try_from(ctx.chain()).unwrap_or(i32::MAX)),
        SqlValue::Uuid(tenant),
        // The encoding its saved values and its key are written in (chunk
        // 6.8).
        crate::read::encoding_value(),
    ];
    let transaction = match ctx.transaction(&plan.store).await {
        Ok(transaction) => transaction,
        Err(error) => return refused(ctx, error, site),
    };
    let rows = match transaction.query(START, &params).await {
        Ok(rows) => rows,
        Err(error) => return refused(ctx, error, site),
    };
    // A start with the same key that committed after this action's
    // snapshot is found by neither half: the action runs again and finds
    // it.
    let found = match rows.first().and_then(|row| row.first()) {
        Some(SqlValue::Uuid(found)) => *found,
        _ => return refused(ctx, StoreError::ConcurrentChange, site),
    };
    ctx.defer(Deferred::Batch(BatchStart {
        store: plan.store.clone(),
        run: found,
    }));
    Stored::Value(run_value(&found))
}

/// The system a run works as (area 12, "A batch's tenant"; chunk 6.4c):
/// in an application with a tenant, limited to the one tenant its saved
/// values holding a tenant's id name, `values` and its `key:` as kept,
/// and otherwise to none.
fn run_system(
    json: JsonCodecs<'_>,
    plan: &BatchPlan,
    values: &[Value],
    key: Option<&str>,
) -> Result<System, String> {
    let key = match (plan.key, key) {
        (Some(number), Some(text)) => json.read(number, text)?,
        _ => Value::None,
    };
    Ok(System {
        tenant: tenant_of(plan, values, key),
    })
}

/// The tenant a run saving `values` with `key:` `key` is limited to: the
/// one its saved values holding a tenant's id name, and `None` in an
/// application with no tenant.
fn tenant_of(plan: &BatchPlan, values: &[Value], key: Value) -> Option<[u8; 16]> {
    let BatchTenant::Saved {
        captured,
        key: by_key,
    } = &plan.tenant
    else {
        return None;
    };
    let mut held: Vec<Value> = captured
        .iter()
        .map(|at| values.get(*at as usize).cloned().unwrap_or(Value::None))
        .collect();
    if *by_key {
        held.push(key);
    }
    Some(crate::outbox::named_tenant(held.iter()))
}

/// The `key:` a call passes, of the values it gives after what it saves.
fn key_value(plan: &BatchPlan, key: &[Value]) -> Value {
    match (plan.key, key.first()) {
        (Some(_), Some(value)) => value.clone(),
        _ => Value::None,
    }
}

/// What a run's `shape` column keeps: a digest of the plan's shape, so a
/// deploy that changes what the call saves or calls is found.
pub fn shape_digest(plan: &BatchPlan) -> String {
    blake3::hash(plan.shape.as_bytes()).to_hex().to_string()
}

/// `batches.each`, a file helper's read or write, `batches.status` or
/// `batches.cancel`, called with `args` at `site` of the running action
/// (area 12): the one entry point both executors call (chunk 8.1h). A
/// call that starts a run is given its plan, `plan`, and `args` are the
/// values it saves, then its `key:`; the others are given the run. A
/// store's refusal is the failure it raised, never a `conflict:` arm.
pub async fn call(
    ctx: &mut Ctx<'_>,
    batches: Option<Batches<'_>>,
    op: jadpo_std::batch::BatchOp,
    plan: Option<&BatchPlan>,
    args: &[Value],
    site: u32,
) -> Stored {
    use jadpo_std::batch::BatchOp;
    let Some(batches) = batches else {
        return fault(ctx, "a batch in a program that runs none", site);
    };
    let stored = match (op, args) {
        (BatchOp::Each | BatchOp::Read(_) | BatchOp::Write(_), _) => match plan {
            Some(plan) => start(ctx, batches, plan, args, site).await,
            None => return fault(ctx, "a batch with no plan", site),
        },
        (BatchOp::Status, [run]) => status(ctx, batches, run, site).await,
        (BatchOp::Cancel, [run]) => cancel(ctx, batches, run, site).await,
        _ => {
            return fault(
                ctx,
                format!("`batches.{}` called with the wrong arguments", op.name()),
                site,
            );
        }
    };
    match stored {
        Stored::Conflict { cause, .. } => Stored::Failed(cause),
        stored => stored,
    }
}

/// What a file helper's write's row (chunk 6.4b2) runs with: each local
/// of the action that started the run it reads, `row.item` given `item`
/// and every other the value the run saved for it at its place in `saved`
/// (`none` past the end), as either executor sets them before working
/// the row out.
pub fn row_locals(
    row: &jadpo_std::batch::BatchRow,
    item: Value,
    saved: &[Value],
) -> Vec<(u32, Value)> {
    let mut locals = vec![(row.item, item)];
    for (local, at) in &row.locals {
        locals.push((
            *local,
            saved.get(*at as usize).cloned().unwrap_or(Value::None),
        ));
    }
    locals
}

/// `batches.status(run)`, at `site` of the running action: the run's
/// `BatchProgress`, read in the action's transaction.
pub async fn status(ctx: &mut Ctx<'_>, batches: Batches<'_>, run: &Value, site: u32) -> Stored {
    let (Some(id), Some(store)) = (run_id(run), batches.store()) else {
        return fault(
            ctx,
            "a batch's status with no run or no store keeping it",
            site,
        );
    };
    let transaction = match ctx.read_transaction(store).await {
        Ok(transaction) => transaction,
        Err(error) => return refused(ctx, error, site),
    };
    let rows = match transaction.query(STATUS, &[SqlValue::Uuid(id)]).await {
        Ok(rows) => rows,
        Err(error) => return refused(ctx, error, site),
    };
    match rows.first().map(Vec::as_slice) {
        Some(
            [
                SqlValue::Int8(done),
                SqlValue::Int8(problems),
                SqlValue::Text(state),
                likely_total,
            ],
        ) => {
            let Some(state) = BatchState::from_name(state) else {
                return fault(ctx, "a batch run in a state the runtime doesn't keep", site);
            };
            // Once finished, the total is known: the items it did.
            let likely_total = match (state, likely_total) {
                (BatchState::Finished, _) => Some(*done),
                (_, SqlValue::Int8(total)) => Some(*total),
                _ => None,
            };
            Stored::Value(progress(*done, *problems, state, likely_total))
        }
        Some(_) => fault(ctx, "a batch run's row the runtime didn't write", site),
        None => fault(ctx, "no batch run is kept with this `BatchRun`'s id", site),
    }
}

/// A `BatchProgress`, with the total its run estimates, if it has one
/// yet.
fn progress(done: i64, problems: i64, state: BatchState, likely_total: Option<i64>) -> Value {
    let ordinal = jadpo_std::variant_ordinal(BatchState::NAME, state.name()).unwrap_or(0);
    Value::record(
        jadpo_std::batch::BATCH_PROGRESS,
        vec![
            (Rc::from("items_done"), Value::Int(done)),
            (
                Rc::from("estimated_total"),
                likely_total.map_or(Value::None, Value::Int),
            ),
            (Rc::from("problems"), Value::Int(problems)),
            (
                Rc::from("state"),
                Value::variant(BatchState::NAME, ordinal, state.name(), Vec::new()),
            ),
        ],
    )
}

/// `batches.cancel(run)`, at `site` of the running action: asks the run
/// to stop after the batch in hand, in the action's transaction.
pub async fn cancel(ctx: &mut Ctx<'_>, batches: Batches<'_>, run: &Value, site: u32) -> Stored {
    let (Some(id), Some(store)) = (run_id(run), batches.store()) else {
        return fault(
            ctx,
            "a batch cancelled with no run or no store keeping it",
            site,
        );
    };
    let now = ctx.now();
    let transaction = match ctx.transaction(store).await {
        Ok(transaction) => transaction,
        Err(error) => return refused(ctx, error, site),
    };
    match transaction
        .execute(CANCEL, &[SqlValue::Uuid(id), at(now)])
        .await
    {
        Ok(_) => Stored::Value(Value::Unit),
        Err(error) => refused(ctx, error, site),
    }
}

/// How a runner left a run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ran {
    /// Its last batch and `finished:` committed.
    Finished,
    /// `batches.cancel` stopped it after the batch in hand.
    Cancelled,
    /// It stopped for good, for this reason, never a value.
    Failed(String),
    /// Another runner holds it, or it isn't running: nothing was done.
    NotClaimed,
    /// It stopped for now, for this reason (a store that couldn't be
    /// reached, changes that kept meeting its batch, its time running
    /// out): what committed stands, and it is carried on later.
    Paused(String),
    /// Its invocation's budget was spent (chunk 6.7): what committed
    /// stands, its lease was given back, and it goes on in a fresh
    /// invocation.
    HandedOn,
}

/// The host's clock now, or `None` for a host with none.
fn now_on(host: &dyn Host) -> Option<i64> {
    host.now_ms()
}

/// Runs the run `start` names to its end, or as far as one runner goes,
/// on `host` with `executor`'s batches, within the budget `host` gives an
/// invocation after a response: claimed with a lease, then a page of
/// `from:` at a time, each page a transaction as the system. What the
/// runs' actions left for after (events, other runs) is given back with
/// how the run was left.
pub async fn run(
    executor: &dyn Executor,
    host: &dyn Host,
    start: &BatchStart,
) -> (Ran, Vec<Deferred>) {
    let budget = Budget::start(host, Invocation::AfterResponse);
    run_within(executor, host, &budget, start).await
}

/// [`run`] within `budget` (chunk 6.7): the run's lease lasts what is left
/// of it and a margin, and once it is spent, between pages or by cutting
/// the page in hand short, the lease is given back and the run, if it got
/// further in this invocation, handed on ([`Ran::HandedOn`], the run left
/// with what the actions left for after) to a fresh one. A run that
/// couldn't commit a page in a whole budget is left for the minute tick
/// instead.
pub async fn run_within(
    executor: &dyn Executor,
    host: &dyn Host,
    budget: &Budget<'_>,
    start: &BatchStart,
) -> (Ran, Vec<Deferred>) {
    let mut left = Vec::new();
    let ran = run_inner(executor, host, budget, start, &mut left).await;
    (ran, left)
}

async fn run_inner(
    executor: &dyn Executor,
    host: &dyn Host,
    budget: &Budget<'_>,
    start: &BatchStart,
    left: &mut Vec<Deferred>,
) -> Ran {
    let Some(batches) = executor.plan().batches() else {
        return Ran::Failed("the program runs no batches".into());
    };
    let Some(database) = host.database(&start.store) else {
        return Ran::Paused(format!("no store called `{}`", start.store));
    };
    let Some(now) = now_on(host) else {
        return Ran::Paused("the host has no clock".into());
    };
    let lease = Leased { id: start.run };
    let row = match crate::lease::claim(database, budget, &lease, now).await {
        Ok(Some(row)) => row,
        Ok(None) => return Ran::NotClaimed,
        Err(error) => return Ran::Paused(format!("the store refused its claim ({error:?})")),
    };
    let (name, shape, captured, key, depth, encoding) = match row.as_slice() {
        [
            SqlValue::Text(name),
            SqlValue::Text(shape),
            SqlValue::Json(captured),
            key,
            depth,
            encoding,
        ] => (
            name.clone(),
            shape.clone(),
            captured.clone(),
            match key {
                SqlValue::Text(key) => Some(key.clone()),
                _ => None,
            },
            // A run kept before its depth was counts from the start.
            match depth {
                SqlValue::Int4(depth) => u32::try_from(*depth).unwrap_or(0),
                _ => 0,
            },
            encoding.clone(),
        ),
        _ => return Ran::Failed("its row isn't one the runtime wrote".into()),
    };
    // What a later release saved is never guessed at (chunk 6.8).
    if let Err(why) = crate::read::kept_encoding(&encoding) {
        let why = format!("its saved values were {why}");
        end(host, start, BatchState::Failed, Some(&why)).await;
        return Ran::Failed(why);
    }
    // A deploy that changed what the call saves or what it calls stops
    // the run rather than reading its saved values as something else.
    let Some(plan) = batches
        .plan(&name)
        .filter(|plan| shape_digest(plan) == shape)
    else {
        // A run kept before its call was named by a stable key and its
        // shape written in canonical type text, which no call now has
        // (chunk 6.8).
        let why = if matches!(encoding, SqlValue::Null(_)) {
            "it was started by a release that named its `batches.each` call another way"
        } else {
            "a deploy changed the `batches.each` call it was started by"
        }
        .to_string();
        end(host, start, BatchState::Failed, Some(&why)).await;
        return Ran::Failed(why);
    };
    let values = match read_saved(batches.json, plan, &captured) {
        Ok(values) => values,
        Err(why) => {
            let why = format!("its saved values no longer read: {why}");
            end(host, start, BatchState::Failed, Some(&why)).await;
            return Ran::Failed(why);
        }
    };
    let system = match run_system(batches.json, plan, &values, key.as_deref()) {
        Ok(system) => system,
        Err(why) => {
            let why = format!("its key no longer reads: {why}");
            end(host, start, BatchState::Failed, Some(&why)).await;
            return Ran::Failed(why);
        }
    };
    let mut pages = 0;
    let mut attempts = 0;
    // A file helper's file, opened at its first page in this invocation.
    let mut opened: Option<file::Opened> = None;
    loop {
        if budget.spent() {
            return handed_on(database, budget, start, &lease, pages, left).await;
        }
        let saved = Saved {
            values: &values,
            system,
            depth,
        };
        // The page in hand is cut short when the budget ends: dropped,
        // nothing it did kept.
        let stepped = budget
            .run(step(
                executor,
                budget,
                batches,
                plan,
                start,
                saved,
                (&mut opened, left),
            ))
            .await;
        match stepped {
            Ok(Step::Next) => {
                pages += 1;
                attempts = 0;
            }
            Ok(Step::Changed) if attempts + 1 < BATCH_ATTEMPTS => attempts += 1,
            Ok(Step::Changed) => {
                return Ran::Paused("changes by others kept meeting its batch".into());
            }
            // An item that ran out of the budget's time pauses its page:
            // the run is handed on as at any budget's end.
            Ok(Step::Ended(Ran::Paused(_))) if budget.spent() => {}
            Ok(Step::Ended(ran)) => return ran,
            Err(_) => return handed_on(database, budget, start, &lease, pages, left).await,
        }
    }
}

/// Gives back the lease of the run `start` names once `budget` is spent,
/// and hands the run on to a fresh invocation when it committed `pages`
/// in this one and its host isn't shutting down: otherwise it waits for
/// the minute tick, so a run that can't get a page done in a whole budget
/// doesn't hand itself on for ever.
async fn handed_on(
    database: &dyn crate::Database,
    budget: &Budget<'_>,
    start: &BatchStart,
    lease: &Leased,
    pages: u32,
    left: &mut Vec<Deferred>,
) -> Ran {
    let now = budget.host().now_ms().unwrap_or(0);
    crate::lease::release(database, lease, now).await;
    if budget.stopping() {
        return Ran::Paused("its host began to shut down".into());
    }
    if pages == 0 {
        return Ran::Paused("its invocation's budget ended before a page committed".into());
    }
    left.push(Deferred::Batch(start.clone()));
    Ran::HandedOn
}

/// How one batch went.
enum Step {
    /// It committed: on to the next.
    Next,
    /// A concurrent change undid it: try it again.
    Changed,
    Ended(Ran),
}

/// What a run saved, read back, the system its batches work as, and how
/// deep in a chain of events it was started.
#[derive(Clone, Copy)]
struct Saved<'a> {
    values: &'a [Value],
    system: System,
    depth: u32,
}

/// One transaction of the run: the next page with its items, or, once
/// every page is read, `finished:`; or the stop a cancel asked for.
async fn step(
    executor: &dyn Executor,
    budget: &Budget<'_>,
    batches: Batches<'_>,
    plan: &BatchPlan,
    start: &BatchStart,
    saved: Saved<'_>,
    (opened, left): (&mut Option<file::Opened>, &mut Vec<Deferred>),
) -> Step {
    let host = budget.host();
    let Some(now) = now_on(host) else {
        return Step::Ended(Ran::Paused("the host has no clock".into()));
    };
    // Each batch's items read the clock as the batch began.
    let env = Env {
        seed: None,
        clock: Clock::fixed(now),
    };
    let mut ctx = Ctx::of(executor, host, env);
    // Its items run as deep in a chain of events as the action that
    // started the run, so what they emit is one deeper than the event
    // that set it off, and a loop through a batch stops as any does
    // (chunk 6.6a).
    ctx.background_mut().set_chain(saved.depth);
    // Work that never waits gives the thread back as it checks this, and
    // stops once the budget is spent (chunk 6.7).
    ctx.set_deadline(Some(budget.limit_ms()));
    let lease_until = crate::lease::until(budget, now);
    // Each kind of page's future is boxed (chunk 6.4b3): a file's reads,
    // its unpacking and a query's count make them large, and whatever
    // runs a batch, the tick among them, would otherwise hold each inline
    // and could run out of a thread's stack.
    let stepped = match (&plan.from, &plan.to) {
        (BatchFrom::Query(_), Some(write)) => {
            Box::pin(export::page(
                executor,
                &mut ctx,
                batches,
                plan,
                write,
                start,
                saved,
                (now, lease_until),
            ))
            .await
        }
        // An item whose action makes outside calls is a batch of its own
        // (chunk 7.4b).
        (BatchFrom::Query(_), None) if calling::calls(plan.each.action()) => {
            Box::pin(calling::page(
                executor,
                &mut ctx,
                batches,
                plan,
                start,
                saved,
                (now, lease_until),
            ))
            .await
        }
        (BatchFrom::Query(_), None) => {
            Box::pin(batch(
                executor,
                &mut ctx,
                batches,
                plan,
                start,
                saved,
                (now, lease_until),
            ))
            .await
        }
        (BatchFrom::File(rows), _) => {
            Box::pin(file::page(
                executor,
                &mut ctx,
                batches,
                plan,
                rows,
                start,
                saved,
                opened,
                (now, lease_until),
            ))
            .await
        }
    };
    left.extend(ctx.take_deferred());
    stepped
}

/// What a batch found on the run's row, which it holds locked.
struct Held {
    position: Option<String>,
    read_all: bool,
    done: i64,
    problems: i64,
}

/// One batch of the run at `now`, its lease renewed until `lease_until`
/// as its position commits.
async fn batch(
    executor: &dyn Executor,
    ctx: &mut Ctx<'_>,
    batches: Batches<'_>,
    plan: &BatchPlan,
    start: &BatchStart,
    saved: Saved<'_>,
    (now, lease_until): (i64, i64),
) -> Step {
    let Saved { values, system, .. } = saved;
    let id = SqlValue::Uuid(start.run);
    let mut open = match Batch::open(ctx, &plan.store, plan.isolation, system).await {
        Ok(open) => open,
        Err(StoreError::ConcurrentChange) => return Step::Changed,
        Err(error) => {
            return Step::Ended(Ran::Paused(format!(
                "the store refused its batch ({error:?})"
            )));
        }
    };
    let locked = match open.transaction() {
        Some(transaction) => transaction.query(LOCK, std::slice::from_ref(&id)).await,
        None => Err(StoreError::Fault("a batch without its transaction".into())),
    };
    let held = match locked {
        Ok(rows) => match rows.first().map(Vec::as_slice) {
            Some(
                [
                    SqlValue::Text(state),
                    position,
                    SqlValue::Bool(read_all),
                    SqlValue::Int8(done),
                    SqlValue::Int8(problems),
                    SqlValue::Bool(cancelled),
                ],
            ) => {
                if state != BatchState::Running.name() {
                    open.rollback().await;
                    return Step::Ended(Ran::NotClaimed);
                }
                if *cancelled {
                    let ended = end_in(&mut open, &id, BatchState::Cancelled, None, now).await;
                    return finish(open, ended, Ran::Cancelled).await;
                }
                Held {
                    position: match position {
                        SqlValue::Text(cursor) => Some(cursor.clone()),
                        _ => None,
                    },
                    read_all: *read_all,
                    done: *done,
                    problems: *problems,
                }
            }
            _ => {
                open.rollback().await;
                return Step::Ended(Ran::NotClaimed);
            }
        },
        Err(StoreError::ConcurrentChange) => {
            open.rollback().await;
            return Step::Changed;
        }
        Err(error) => {
            open.rollback().await;
            return Step::Ended(Ran::Paused(format!(
                "the store refused its batch ({error:?})"
            )));
        }
    };
    if held.read_all {
        // A `finished:` whose action makes outside calls is a batch of its
        // own (chunk 7.4b).
        if calling::calls(plan.finished.as_ref()) {
            open.rollback().await;
            return calling::finished(executor, ctx, plan, start, saved, &held, now, None).await;
        }
        return finished(executor, open, plan, &id, values, &held, now, None).await;
    }
    let Some(each) = plan.each.action() else {
        open.rollback().await;
        return Step::Ended(Ran::Failed("a run with no action to call".into()));
    };
    // The query's count, as its first page reads it, the total the run
    // estimates (chunk 6.4b3).
    let likely_total = if held.position.is_none() {
        match counted(&mut open, batches, plan, values).await {
            Ok(likely_total) => likely_total,
            Err(unread) => {
                open.rollback().await;
                return match unread {
                    Unread::Changed => Step::Changed,
                    Unread::Paused(why) | Unread::Refused(why) => Step::Ended(Ran::Paused(why)),
                };
            }
        }
    } else {
        None
    };
    // The page after the position that committed, as the system.
    let (items, next) =
        match read_page(&mut open, batches, plan, values, held.position.as_deref()).await {
            Ok(page) => page,
            Err(Unread::Changed) => {
                open.rollback().await;
                return Step::Changed;
            }
            Err(Unread::Paused(why)) => {
                open.rollback().await;
                return Step::Ended(Ran::Paused(why));
            }
            Err(Unread::Refused(why)) => {
                let ended = end_in(&mut open, &id, BatchState::Failed, Some(&why), now).await;
                return finish(open, ended, Ran::Failed(why)).await;
            }
        };
    let calls: Vec<Vec<Value>> = items
        .iter()
        .map(|item| arguments(&each.args, item, values))
        .collect();
    let mut problems: Vec<(i64, String)> = Vec::new();
    let mut paused: Option<String> = None;
    let base = held.done;
    open.items(
        executor,
        each.callable,
        calls,
        &mut |at, outcome| match problem(&outcome) {
            Item::Done => ControlFlow::Continue(()),
            Item::Problem(rule) => {
                problems.push((base + at as i64 + 1, rule));
                ControlFlow::Continue(())
            }
            Item::Pause(why) => {
                paused = Some(why);
                ControlFlow::Break(())
            }
        },
    )
    .await;
    if open.changed() {
        open.rollback().await;
        return Step::Changed;
    }
    if let Some(why) = paused {
        open.rollback().await;
        return Step::Ended(Ran::Paused(why));
    }
    let done = base + items.len() as i64;
    let had = held.problems + problems.len() as i64;
    let Some(transaction) = open.transaction() else {
        return Step::Ended(Ran::Paused("a batch without its transaction".into()));
    };
    for (row, rule) in &problems {
        let written = transaction
            .execute(
                PROBLEM,
                &[
                    id.clone(),
                    SqlValue::Int8(*row),
                    SqlValue::Text(String::new()),
                    SqlValue::Text(rule.clone()),
                ],
            )
            .await;
        if let Err(error) = written {
            return abandoned(open, error).await;
        }
    }
    let advanced = transaction
        .execute(
            ADVANCE,
            &[
                id.clone(),
                next.clone()
                    .map_or(SqlValue::Null(SqlType::Text), SqlValue::Text),
                SqlValue::Bool(next.is_none()),
                SqlValue::Int8(done),
                SqlValue::Int8(had),
                at(lease_until),
                at(now),
                total(likely_total),
            ],
        )
        .await;
    if let Err(error) = advanced {
        return abandoned(open, error).await;
    }
    match open.commit().await {
        Ok(()) => Step::Next,
        Err(StoreError::ConcurrentChange) => Step::Changed,
        Err(error) => Step::Ended(Ran::Paused(format!(
            "its batch couldn't commit ({error:?})"
        ))),
    }
}

/// A total a run estimates, as [`ADVANCE`] keeps it: `NULL` keeps the
/// one it had.
fn total(likely_total: Option<i64>) -> SqlValue {
    likely_total.map_or(SqlValue::Null(SqlType::Int8), SqlValue::Int8)
}

/// How many items `from:`'s query holds, counted in `open`'s transaction
/// as the system, as its first page reads them (chunk 6.4b3).
async fn counted(
    open: &mut Batch<'_, '_>,
    batches: Batches<'_>,
    plan: &BatchPlan,
    values: &[Value],
) -> Result<Option<i64>, Unread> {
    let Some(query) = plan.from.query() else {
        return Ok(None);
    };
    let args: Vec<Value> = values.get(..plan.from_args).unwrap_or_default().to_vec();
    match crate::read::count(open.ctx(), query, batches.json, &args).await {
        Ok(likely_total) => Ok(likely_total),
        Err(_) if open.changed() => Err(Unread::Changed),
        Err(StoreError::ConcurrentChange) => Err(Unread::Changed),
        Err(StoreError::Unavailable) => Err(Unread::Paused("its store couldn't be reached".into())),
        Err(error) => Err(Unread::Paused(format!(
            "counting its query failed ({error:?})"
        ))),
    }
}

/// Why a run's next page wasn't read.
enum Unread {
    /// A concurrent change met it: the batch runs again.
    Changed,
    /// Something passing: the run waits.
    Paused(String),
    /// Its query refused the page it had reached: the run can't go on.
    Refused(String),
}

/// The items of the page of `from:` after `position`, the cursor that
/// committed, and the cursor of the page after it, read in `open`'s
/// transaction as the system.
async fn read_page(
    open: &mut Batch<'_, '_>,
    batches: Batches<'_>,
    plan: &BatchPlan,
    values: &[Value],
    position: Option<&str>,
) -> Result<(Vec<Value>, Option<String>), Unread> {
    let Some(query) = plan.from.query() else {
        return Err(Unread::Paused("a file's run read as a query's".into()));
    };
    let mut args: Vec<Value> = values.get(..plan.from_args).unwrap_or_default().to_vec();
    if let Some(page) = &query.page
        && let jadpo_std::read::Source::Arg { index, path } = &page.after
    {
        let cursor = position.map_or(Value::None, Value::text);
        if let Some(arg) = args.get_mut(*index as usize) {
            *arg = with_at(arg, path, cursor);
        }
    }
    let read = crate::store(
        open.ctx(),
        StoreOp::Read(query),
        batches.failures,
        batches.json,
        &args,
        Frame::ENTRY,
    )
    .await;
    let page = match read {
        Stored::Value(page) => page,
        _ if open.changed() => return Err(Unread::Changed),
        Stored::Failed(failure) if failure.code == "unavailable" => {
            return Err(Unread::Paused("its store couldn't be reached".into()));
        }
        // A cursor its query no longer takes: a deploy changed the
        // query's order.
        Stored::Failed(failure) | Stored::Conflict { cause: failure, .. } => {
            return Err(Unread::Refused(format!(
                "its query refused the page it had reached (`{}`)",
                failure.code
            )));
        }
        Stored::Fault(fault) => {
            return Err(Unread::Paused(format!(
                "reading its page faulted with `{}`",
                fault.kind.classification()
            )));
        }
    };
    Ok(match &page {
        Value::Record(record) => (
            match record.get("items") {
                Some(Value::List(items)) => items.to_vec(),
                _ => Vec::new(),
            },
            match record.get("next") {
                Some(Value::Text(cursor)) => Some(cursor.to_string()),
                _ => None,
            },
        ),
        _ => (Vec::new(), None),
    })
}

/// A statement of the run's own the store refused: nothing of the batch
/// is kept.
async fn abandoned(open: Batch<'_, '_>, error: StoreError) -> Step {
    open.rollback().await;
    match error {
        StoreError::ConcurrentChange => Step::Changed,
        error => Step::Ended(Ran::Paused(format!(
            "the store refused the run's own statement ({error:?})"
        ))),
    }
}

/// `finished:`, once every page is read, in a transaction of its own:
/// its action given the run's summary, an `ExportSummary` holding `file`
/// for a write, then the run finished. One that fails is undone, and the
/// run is failed with its code.
#[allow(clippy::too_many_arguments)]
async fn finished(
    executor: &dyn Executor,
    mut open: Batch<'_, '_>,
    plan: &BatchPlan,
    id: &SqlValue,
    values: &[Value],
    held: &Held,
    now: i64,
    file: Option<Value>,
) -> Step {
    let mut failure: Option<String> = None;
    if let Some(call) = &plan.finished {
        let Some(transaction) = open.transaction() else {
            return Step::Ended(Ran::Paused("a batch without its transaction".into()));
        };
        let rows = match transaction.query(PROBLEMS, std::slice::from_ref(id)).await {
            Ok(rows) => rows,
            Err(error) => return abandoned(open, error).await,
        };
        let summary = summary(held, &rows, file);
        let args = arguments(&call.args, &summary, values);
        let mut paused: Option<String> = None;
        open.items(executor, call.callable, vec![args], &mut |_, outcome| {
            match problem(&outcome) {
                Item::Done => {}
                Item::Problem(rule) => {
                    failure = Some(format!("its `finished:` failed with `{rule}`"))
                }
                Item::Pause(why) => paused = Some(why),
            }
            ControlFlow::Continue(())
        })
        .await;
        if open.changed() {
            open.rollback().await;
            return Step::Changed;
        }
        if let Some(why) = paused {
            open.rollback().await;
            return Step::Ended(Ran::Paused(why));
        }
    }
    let state = if failure.is_some() {
        BatchState::Failed
    } else {
        BatchState::Finished
    };
    let ended = end_in(&mut open, id, state, failure.as_deref(), now).await;
    let ran = match failure {
        Some(why) => Ran::Failed(why),
        None => Ran::Finished,
    };
    finish(open, ended, ran).await
}

/// The `ImportSummary` `finished:` receives: the counts and each problem,
/// in row order; or with the `file` a write wrote, the `ExportSummary`.
fn summary(held: &Held, rows: &[Row], file: Option<Value>) -> Value {
    let problems: Vec<Value> = rows
        .iter()
        .filter_map(|row| match row.as_slice() {
            [
                SqlValue::Int8(row),
                SqlValue::Text(column),
                SqlValue::Text(rule),
            ] => Some(Value::record(
                jadpo_std::batch::ROW_PROBLEM,
                vec![
                    (Rc::from("row"), Value::Int(*row)),
                    (Rc::from("column"), Value::text(column)),
                    (Rc::from("rule"), Value::text(rule)),
                ],
            )),
            _ => None,
        })
        .collect();
    let mut fields = vec![
        (Rc::from("items"), Value::Int(held.done)),
        (Rc::from("items_with_problems"), Value::Int(held.problems)),
        (Rc::from("problems"), Value::list(problems)),
    ];
    let name = match file {
        Some(file) => {
            fields.push((Rc::from("file"), file));
            jadpo_std::batch::EXPORT_SUMMARY
        }
        None => jadpo_std::batch::IMPORT_SUMMARY,
    };
    Value::record(name, fields)
}

/// Writes the run's end in the batch's transaction.
async fn end_in(
    open: &mut Batch<'_, '_>,
    id: &SqlValue,
    state: BatchState,
    why: Option<&str>,
    now: i64,
) -> Result<(), StoreError> {
    let Some(transaction) = open.transaction() else {
        return Err(StoreError::Fault("a batch without its transaction".into()));
    };
    transaction
        .execute(
            END,
            &[
                id.clone(),
                SqlValue::Text(state.name().to_string()),
                why.map_or(SqlValue::Null(SqlType::Text), |why| {
                    SqlValue::Text(why.to_string())
                }),
                at(now),
            ],
        )
        .await
        .map(|_| ())
}

/// Commits a batch that ends the run, giving `ran` once it has.
async fn finish(open: Batch<'_, '_>, ended: Result<(), StoreError>, ran: Ran) -> Step {
    if let Err(error) = ended {
        return abandoned(open, error).await;
    }
    match open.commit().await {
        Ok(()) => Step::Ended(ran),
        Err(StoreError::ConcurrentChange) => Step::Changed,
        Err(error) => Step::Ended(Ran::Paused(format!("its end couldn't commit ({error:?})"))),
    }
}

/// Ends a run in one statement of its own: a run that can't go on.
async fn end(host: &dyn Host, start: &BatchStart, state: BatchState, why: Option<&str>) {
    let (Some(database), Some(now)) = (host.database(&start.store), now_on(host)) else {
        return;
    };
    let _ = database
        .query_once(
            END,
            &[
                SqlValue::Uuid(start.run),
                SqlValue::Text(state.name().to_string()),
                why.map_or(SqlValue::Null(SqlType::Text), |why| {
                    SqlValue::Text(why.to_string())
                }),
                at(now),
            ],
        )
        .await;
}

/// What an item's outcome makes of it.
enum Item {
    Done,
    /// A problem, by the rule's code: a failure's own code, or a fault's
    /// classification, never a value.
    Problem(String),
    /// Something passing that no item caused: the batch stops for now.
    Pause(String),
}

fn problem(outcome: &Outcome) -> Item {
    match outcome {
        Outcome::Value(_) => Item::Done,
        // The store couldn't be reached, or changes kept meeting it: the
        // batch, not the item.
        Outcome::Failed(failure) if failure.code == "unavailable" => {
            Item::Pause("its store couldn't be reached".into())
        }
        Outcome::Failed(failure) => Item::Problem(failure.code.clone()),
        Outcome::Fault(fault) => match &fault.kind {
            FaultKind::Store(_) | FaultKind::DeadlineExceeded => Item::Pause(format!(
                "an item's action faulted with `{}`",
                fault.kind.classification()
            )),
            kind => Item::Problem(kind.classification().to_string()),
        },
        Outcome::Assertion(_) => Item::Problem("internal_fault".into()),
    }
}

/// The arguments of a batch's call for `item` (or the summary): each the
/// item or a field inside it, or a value the run saved.
fn arguments(args: &[BatchArg], item: &Value, values: &[Value]) -> Vec<Value> {
    args.iter()
        .map(|arg| match arg {
            BatchArg::Item(path) => {
                let mut value = item.clone();
                for name in path {
                    value = match &value {
                        Value::Record(record) => record.get(name).cloned().unwrap_or(Value::None),
                        Value::Variant(variant) => {
                            variant.get(name).cloned().unwrap_or(Value::None)
                        }
                        _ => Value::None,
                    };
                }
                value
            }
            BatchArg::Captured(at) => values.get(*at as usize).cloned().unwrap_or(Value::None),
        })
        .collect()
}

/// `value` with what is at `path` inside it replaced by `new`: a page's
/// cursor in its query's argument.
fn with_at(value: &Value, path: &[String], new: Value) -> Value {
    let Some((first, rest)) = path.split_first() else {
        return new;
    };
    let Value::Record(record) = value else {
        return value.clone();
    };
    let mut record = (**record).clone();
    let at = record.fields.iter().position(|(name, _)| &**name == first);
    if let Some(at) = at
        && let Some((_, slot)) = record.fields.get_mut(at)
    {
        *slot = with_at(slot, rest, new);
        // Given now, not left out.
        if let Some(omitted) = &record.omitted {
            let kept: Vec<u32> = omitted
                .iter()
                .copied()
                .filter(|place| *place as usize != at)
                .collect();
            record.omitted = Some(Rc::from(kept));
        }
    }
    Value::Record(Rc::new(record))
}

/// The runs waiting in each store the program keeps runs in, whose
/// runner's lease has lapsed by `now`: a crash or a deploy left them, or
/// no host ran them after their action's response. At most a hundred
/// from each store, oldest first.
pub async fn waiting(executor: &dyn Executor, host: &dyn Host, now: i64) -> Vec<BatchStart> {
    let mut out = Vec::new();
    let Some(batches) = executor.plan().batches() else {
        return out;
    };
    let mut stores: Vec<&str> = batches
        .plans
        .iter()
        .map(|plan| plan.store.as_str())
        .collect();
    stores.sort_unstable();
    stores.dedup();
    for store in stores {
        let Some(database) = host.database(store) else {
            continue;
        };
        let Ok(rows) = database
            .query_once(WAITING, &[at(now), SqlValue::Int8(SWEEP_LIMIT)])
            .await
        else {
            continue;
        };
        out.extend(
            rows.into_iter()
                .filter_map(|row| match row.into_iter().next() {
                    Some(SqlValue::Uuid(run)) => Some(BatchStart {
                        store: store.to_string(),
                        run,
                    }),
                    _ => None,
                }),
        );
    }
    out
}

/// Picks up the runs [`waiting`] finds and runs each as [`run`] does,
/// within the budget `host` gives a tick: what area 12's resuming after
/// crashes and deploys is. The runtime's minute tick calls it (chunk 6.2).
/// How each was left, with what their actions left for after.
pub async fn resume(
    executor: &dyn Executor,
    host: &dyn Host,
    now: i64,
) -> (Vec<(BatchStart, Ran)>, Vec<Deferred>) {
    let budget = Budget::start(host, Invocation::Tick);
    resume_within(executor, host, &budget, now).await
}

/// [`resume`] within `budget`, the tick's: once it is spent, the runs not
/// reached wait for the next tick (chunk 6.7).
pub async fn resume_within(
    executor: &dyn Executor,
    host: &dyn Host,
    budget: &Budget<'_>,
    now: i64,
) -> (Vec<(BatchStart, Ran)>, Vec<Deferred>) {
    let mut out = Vec::new();
    let mut left = Vec::new();
    for start in waiting(executor, host, now).await {
        if budget.spent() {
            break;
        }
        let (ran, more) = run_within(executor, host, budget, &start).await;
        left.extend(more);
        out.push((start, ran));
    }
    (out, left)
}

/// Deletes, in each store the program keeps runs in, the runs that ended
/// [`ENDED_KEPT_MS`] or more before `now`, with their problems and
/// cancels, and the cancels as old of runs no store keeps (chunk 6.6a):
/// the retention work run by hourly housekeeping, a thousand rows at a time, at
/// most ten times a tick, as its other deletes. A run still going is
/// never deleted. How many runs and cancels it deleted.
pub async fn clean(executor: &dyn Executor, host: &dyn Host, now: i64) -> u64 {
    use crate::schedules::{DELETE_BATCH, DELETE_BATCHES};
    let budget = Budget::start(host, Invocation::Tick);
    let Some(batches) = executor.plan().batches() else {
        return 0;
    };
    let mut stores: Vec<&str> = batches
        .plans
        .iter()
        .map(|plan| plan.store.as_str())
        .collect();
    stores.sort_unstable();
    stores.dedup();
    let before = at(now.saturating_sub(ENDED_KEPT_MS));
    let mut deleted = 0;
    for store in stores {
        if budget.spent() {
            break;
        }
        let Some(database) = host.database(store) else {
            continue;
        };
        for sql in [ENDED, UNKEPT_CANCELS] {
            for _ in 0..DELETE_BATCHES {
                if budget.spent() {
                    break;
                }
                let Ok(rows) = database
                    .query_once(sql, &[before.clone(), SqlValue::Int8(DELETE_BATCH)])
                    .await
                else {
                    break;
                };
                deleted += rows.len() as u64;
                if (rows.len() as i64) < DELETE_BATCH {
                    break;
                }
            }
        }
    }
    deleted
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A run's saved values are read with the standard JSON reader
    /// (chunk 6.8): commas and brackets inside a value's text are its own,
    /// a number keeps the digits it was written with, `null` is `none`,
    /// and what doesn't read names its value's place and rule, never the
    /// value.
    #[test]
    fn saved_values_are_read_with_the_standard_json_reader() {
        use jadpo_std::http::{Codec, CodecId};
        use jadpo_std::types::ValidatorId;
        let text = jadpo_value::Validator::new(Some(jadpo_std::Builtin::Text), vec![]);
        let count = jadpo_value::Validator::new(
            Some(jadpo_std::Builtin::Int),
            vec![jadpo_std::Rules {
                max: jadpo_std::Num::parse("100"),
                ..jadpo_std::Rules::default()
            }],
        );
        let validators = [text, count];
        let codecs = [
            Codec::Scalar {
                builtin: jadpo_std::Builtin::Text,
                validator: ValidatorId(0),
                int_as_number: false,
            },
            Codec::Scalar {
                builtin: jadpo_std::Builtin::Int,
                validator: ValidatorId(1),
                int_as_number: true,
            },
        ];
        let types = [CodecId(0), CodecId(1)];
        let json = JsonCodecs {
            codecs: jadpo_value::json::Codecs {
                codecs: &codecs,
                validators: &validators,
                shapes: &[],
                coder: None,
            },
            types: &types,
        };
        assert_eq!(
            json.read_list(&[0, 1, 0], r#"["a,b]\"", 42, null]"#),
            Ok(vec![Value::text("a,b]\""), Value::Int(42), Value::None])
        );
        assert_eq!(json.read_list(&[], "[]"), Ok(Vec::new()));
        let why = json
            .read_list(&[0, 1], r#"["kept", 777]"#)
            .expect_err("past its max");
        assert_eq!(why, "value 2 isn't its type (`max`)");
        assert!(!why.contains("777"), "{why}");
        assert!(json.read_list(&[0], r#"{"0": "a"}"#).is_err());
        assert!(json.read_list(&[0], r#"["a"] trailing"#).is_err());
        assert_eq!(
            json.read_list(&[0, 0], r#"["a"]"#),
            Err("1 values kept, where 2 are read".into())
        );
    }

    #[test]
    fn a_cursor_goes_into_its_argument_and_counts_as_given() {
        let input = Value::Record(Rc::new(jadpo_value::Record {
            ty: Rc::from("RecountPage"),
            fields: vec![
                (Rc::from("page_size"), Value::Int(25)),
                (Rc::from("after"), Value::None),
            ],
            omitted: Some(Rc::from(vec![1u32])),
        }));
        let moved = with_at(&input, &["after".to_string()], Value::text("abc"));
        let Value::Record(record) = &moved else {
            panic!("a record");
        };
        assert_eq!(record.get("after"), Some(&Value::text("abc")));
        assert!(record.supplied("after"));
        assert_eq!(with_at(&input, &[], Value::Int(1)), Value::Int(1));
    }

    #[test]
    fn an_items_arguments_are_its_fields_and_the_saved_values() {
        let item = Value::record("Product", vec![(Rc::from("sku"), Value::text("A-1"))]);
        let args = arguments(
            &[
                BatchArg::Item(Vec::new()),
                BatchArg::Item(vec!["sku".into()]),
                BatchArg::Captured(1),
            ],
            &item,
            &[Value::Int(0), Value::Int(7)],
        );
        assert_eq!(args, [item.clone(), Value::text("A-1"), Value::Int(7)]);
    }

    #[test]
    fn a_failure_is_a_problem_and_an_outage_pauses_the_batch() {
        let mut gone = crate::Failure::new("type Gone", "Gone", "gone");
        gone.kind = Some("not_found".into());
        assert!(matches!(
            problem(&Outcome::Failed(Box::new(gone))),
            Item::Problem(rule) if rule == "gone"
        ));
        assert!(matches!(
            problem(&Outcome::Failed(Box::new(crate::unavailable()))),
            Item::Pause(_)
        ));
        let overflow = crate::Fault {
            kind: FaultKind::IntOverflow,
            frames: Vec::new(),
        };
        assert!(matches!(
            problem(&Outcome::Fault(Box::new(overflow))),
            Item::Problem(rule) if rule == "int_overflow"
        ));
        let store = crate::Fault {
            kind: FaultKind::Store("gone away".into()),
            frames: Vec::new(),
        };
        assert!(matches!(
            problem(&Outcome::Fault(Box::new(store))),
            Item::Pause(_)
        ));
    }
}
