//! A run whose actions make outside calls (area 12, "How a run goes";
//! area 6, "Outside calls hold no transaction"; chunk 7.4b).
//!
//! No call goes out while a transaction is open, and a page's items run
//! inside its transaction, so when `each:`'s action makes outside calls,
//! itself or through an action it calls ([`jadpo_std::batch::BatchCall`]'s
//! `calls`), each item is a batch of its own, run as a subscriber's
//! delivery is:
//!
//! - **The run's place** is read in a statement of its own ([`PEEK`]),
//!   locking nothing.
//! - **The item** is read in a snapshot: a query's next page of one item,
//!   giving that item's own cursor ([`crate::read::PageCursor::Last`]), or
//!   a file's next row, read before anything opens.
//! - **Its action** runs in a boundary of the batch's own, as the system,
//!   its calls keyed from the run and the item's place in the walk (its
//!   cursor, or the row's position in its file), so a repeated item sends
//!   each call under the key it was first sent with. A call closes the
//!   snapshot, and the action's writes after it run in a transaction
//!   opened after the call, held to the row versions the snapshot loaded
//!   (chunk 7.4).
//! - **The run's own statements** follow in that transaction: the run's
//!   row locked and held to the position read, the item's problem, if its
//!   action failed, and the run's next position, committed with the
//!   item's writes. A crash or a deploy before the commit loses only the
//!   item in hand, and a concurrent change runs it again; either way its
//!   calls repeat under their keys.
//!
//! `finished:` whose action makes outside calls runs the same way
//! ([`finished`]), keyed from the run, and the run's end commits with its
//! writes.

use std::ops::ControlFlow;

use jadpo_std::BatchState;
use jadpo_std::batch::{BatchCall, BatchPlan};
use jadpo_std::read::{ReadPlan, Size};
use jadpo_value::Value;

use super::{
    ADVANCE, Batches, Held, Item, LOCK, PROBLEM, PROBLEMS, Ran, Saved, Step, Unread, abandoned,
    arguments, at, batch, end_in, finish, problem, summary, total,
};
use crate::read::{Found, Interrupt, PageCursor};
use crate::runner::Batch;
use crate::{Ctx, Executor, Frame, Outcome, SqlType, SqlValue, StoreError};

use super::BatchStart;

/// Where a run is, read on its own before an item's batch opens, locking
/// nothing: the batch holds the run to it under [`LOCK`] once the item's
/// calls have answered.
pub(crate) const PEEK: &str = "SELECT \"state\", \"position\", \"read_all\", \"items_done\", \"problems\", EXISTS (SELECT 1 FROM \"jadpo_batch_cancels\" WHERE \"run_id\" = $1) FROM \"jadpo_batch_runs\" WHERE \"id\" = $1";

/// The scope a run's calls' keys are derived in (area 11, "Calling").
const SCOPE: &str = "batch";

/// The key a run's call is derived from, beside its scope: the run's id
/// and `place`, the item's place in its walk or `finished`.
fn key(start: &BatchStart, place: &str) -> String {
    format!("{}/{place}", jadpo_value::encoding::to_hex(&start.run))
}

/// Whether the action `call` makes outside calls.
pub(super) fn calls(call: Option<&BatchCall>) -> bool {
    call.is_some_and(|call| call.calls)
}

/// What [`PEEK`] found, and whether a cancel was asked for; or the step
/// the run ends at when it isn't running or its store refused.
async fn peek(ctx: &mut Ctx<'_>, plan: &BatchPlan, id: &SqlValue) -> Result<(Held, bool), Step> {
    let Some(database) = ctx.host().database(&plan.store) else {
        return Err(Step::Ended(Ran::Paused(format!(
            "no store called `{}`",
            plan.store
        ))));
    };
    let rows = match database.query_once(PEEK, std::slice::from_ref(id)).await {
        Ok(rows) => rows,
        Err(error) => {
            return Err(Step::Ended(Ran::Paused(format!(
                "the store refused its batch ({error:?})"
            ))));
        }
    };
    match rows.first().map(Vec::as_slice) {
        Some(
            [
                SqlValue::Text(state),
                position,
                SqlValue::Bool(read_all),
                SqlValue::Int8(done),
                SqlValue::Int8(problems),
                SqlValue::Bool(cancelled),
            ],
        ) if state == BatchState::Running.name() => Ok((
            Held {
                position: match position {
                    SqlValue::Text(text) => Some(text.clone()),
                    _ => None,
                },
                read_all: *read_all,
                done: *done,
                problems: *problems,
            },
            *cancelled,
        )),
        _ => Err(Step::Ended(Ran::NotClaimed)),
    }
}

/// One item of a query's run whose `each:` action makes outside calls, a
/// batch of its own; or, once every item is done or a cancel was asked
/// for, what any run's last batch does.
pub(super) async fn page(
    executor: &dyn Executor,
    ctx: &mut Ctx<'_>,
    batches: Batches<'_>,
    plan: &BatchPlan,
    start: &BatchStart,
    saved: Saved<'_>,
    (now, lease_until): (i64, i64),
) -> Step {
    let id = SqlValue::Uuid(start.run);
    let (held, cancelled) = match peek(ctx, plan, &id).await {
        Ok(found) => found,
        Err(step) => return step,
    };
    if cancelled || held.read_all {
        return batch(
            executor,
            ctx,
            batches,
            plan,
            start,
            saved,
            (now, lease_until),
        )
        .await;
    }
    let (Some(each), Some(query)) = (plan.each.action(), plan.from.query()) else {
        return Step::Ended(Ran::Failed("a run with no action to call".into()));
    };
    let mut open = match Batch::open_for_calls(ctx, plan.isolation, saved.system) {
        Ok(open) => open,
        Err(error) => {
            return Step::Ended(Ran::Paused(format!(
                "the store refused its batch ({error:?})"
            )));
        }
    };
    let (item, cursor) = match read_one(&mut open, batches, plan, query, saved.values, &held).await
    {
        Ok(Some(found)) => found,
        // Every item done: the run's position says so, and its last
        // batch runs next.
        Ok(None) => {
            open.rollback().await;
            return walked(ctx, batches, plan, start, saved, &held, (now, lease_until)).await;
        }
        Err(Unread::Changed) => {
            open.rollback().await;
            return Step::Changed;
        }
        Err(Unread::Paused(why)) => {
            open.rollback().await;
            return Step::Ended(Ran::Paused(why));
        }
        Err(Unread::Refused(why)) => {
            open.rollback().await;
            super::end(ctx.host(), start, BatchState::Failed, Some(&why)).await;
            return Step::Ended(Ran::Failed(why));
        }
    };
    let args = arguments(&each.args, &item, saved.values);
    // Its calls are keyed from the run and the item's own cursor, which a
    // resumed run reads it at again.
    open.ctx().set_call_key(SCOPE, key(start, &cursor));
    let ran = run(executor, &mut open, each.callable, args).await;
    // Its query's count, as its first item reads it, the total the run
    // estimates (chunk 6.4b3).
    let total = match held.position {
        None => Total::Count(query, batches, saved.values),
        Some(_) => Total::Kept,
    };
    committed(
        open,
        plan,
        &id,
        held.position.as_deref(),
        Some((ran, None)),
        Next {
            position: Some(cursor),
            read_all: false,
            total,
        },
        (now, lease_until),
    )
    .await
}

/// The item after the position `held` read, and its own cursor, read in
/// `open`'s snapshot as the system: `None` once every item is done.
async fn read_one(
    open: &mut Batch<'_, '_>,
    batches: Batches<'_>,
    plan: &BatchPlan,
    query: &ReadPlan,
    values: &[Value],
    held: &Held,
) -> Result<Option<(Value, String)>, Unread> {
    // A page of one item, giving its own cursor.
    let mut one = query.clone();
    if let Some(page) = &mut one.page {
        page.size = Size::Fixed(1);
    }
    let mut args: Vec<Value> = values.get(..plan.from_args).unwrap_or_default().to_vec();
    if let Some(page) = &query.page
        && let jadpo_std::read::Source::Arg { index, path } = &page.after
    {
        let cursor = held.position.as_deref().map_or(Value::None, Value::text);
        if let Some(arg) = args.get_mut(*index as usize) {
            *arg = super::with_at(arg, path, cursor);
        }
    }
    // A listed query's line, as a read through the store checks it (area
    // 9, "Listing a query"): it never lets the system in.
    if let Some(gate) = &query.gate
        && let crate::Stored::Failed(failure) = crate::admit(open.ctx(), gate, Frame::ENTRY).await
    {
        return Err(Unread::Refused(format!(
            "its query's line under `actions:` refused it (`{}`)",
            failure.code
        )));
    }
    let read = crate::read::read_as(
        open.ctx(),
        &one,
        batches.json,
        &args,
        Frame::ENTRY,
        PageCursor::Last,
    )
    .await;
    let page = match read {
        Ok(Found::Value(page)) => page,
        _ if open.changed() => return Err(Unread::Changed),
        Ok(Found::Missing(_)) => {
            return Err(Unread::Paused("its page read as a single record".into()));
        }
        Err(Interrupt::Failed(failure)) if failure.code == "unavailable" => {
            return Err(Unread::Paused("its store couldn't be reached".into()));
        }
        // A cursor its query no longer takes: a deploy changed the query's
        // order.
        Err(Interrupt::Failed(failure)) => {
            return Err(Unread::Refused(format!(
                "its query refused the page it had reached (`{}`)",
                failure.code
            )));
        }
        Err(Interrupt::Fault(fault)) => {
            return Err(Unread::Paused(format!(
                "reading its page faulted with `{}`",
                fault.kind.classification()
            )));
        }
    };
    let Value::Record(page) = &page else {
        return Ok(None);
    };
    let item = match page.get("items") {
        Some(Value::List(items)) => items.first().cloned(),
        _ => None,
    };
    match (item, page.get("next")) {
        (Some(item), Some(Value::Text(cursor))) => Ok(Some((item, cursor.to_string()))),
        // An item with no place to key its calls from is never run.
        (Some(_), _) => Err(Unread::Paused("its item came with no cursor".into())),
        (None, _) => Ok(None),
    }
}

/// Every item of a query's run done: its position kept, and `read_all`,
/// so its last batch runs next.
async fn walked(
    ctx: &mut Ctx<'_>,
    batches: Batches<'_>,
    plan: &BatchPlan,
    start: &BatchStart,
    saved: Saved<'_>,
    held: &Held,
    (now, lease_until): (i64, i64),
) -> Step {
    let id = SqlValue::Uuid(start.run);
    let open = match Batch::open_for_calls(ctx, plan.isolation, saved.system) {
        Ok(open) => open,
        Err(error) => {
            return Step::Ended(Ran::Paused(format!(
                "the store refused its batch ({error:?})"
            )));
        }
    };
    let total = match (held.position.is_none(), plan.from.query()) {
        (true, Some(query)) => Total::Count(query, batches, saved.values),
        _ => Total::Kept,
    };
    committed(
        open,
        plan,
        &id,
        held.position.as_deref(),
        None,
        Next {
            position: held.position.clone(),
            read_all: true,
            total,
        },
        (now, lease_until),
    )
    .await
}

/// One row of a file's run whose `each:` action makes outside calls, a
/// batch of its own (chunk 7.4b): `args` its action's arguments, `place`
/// where it begins in its file, which keys its calls; `position` where the
/// run was when the row was read, and `next` where it will be after it.
#[allow(clippy::too_many_arguments)]
pub(super) async fn row(
    executor: &dyn Executor,
    ctx: &mut Ctx<'_>,
    plan: &BatchPlan,
    start: &BatchStart,
    saved: Saved<'_>,
    each: &BatchCall,
    args: Vec<Value>,
    (place, row): (&str, i64),
    position: Option<String>,
    next: Next<'_>,
    (now, lease_until): (i64, i64),
) -> Step {
    let id = SqlValue::Uuid(start.run);
    let mut open = match Batch::open_for_calls(ctx, plan.isolation, saved.system) {
        Ok(open) => open,
        Err(error) => {
            return Step::Ended(Ran::Paused(format!(
                "the store refused its batch ({error:?})"
            )));
        }
    };
    open.ctx().set_call_key(SCOPE, key(start, place));
    let ran = run(executor, &mut open, each.callable, args).await;
    committed(
        open,
        plan,
        &id,
        position.as_deref(),
        Some((ran, Some(row))),
        next,
        (now, lease_until),
    )
    .await
}

/// How the run's total is kept as an item commits.
pub(super) enum Total<'a> {
    /// A query's first item counts its query, as [`batch`] does.
    Count(&'a ReadPlan, Batches<'a>, &'a [Value]),
    /// A file's works it out from the items done once this one is.
    From(&'a dyn Fn(i64) -> Option<i64>),
    /// The one it has.
    Kept,
}

/// Where the run goes once an item commits.
pub(super) struct Next<'a> {
    pub position: Option<String>,
    pub read_all: bool,
    pub total: Total<'a>,
}

/// Runs `callable` with `args` as one item of `open`, its own batch: what
/// becomes of it.
async fn run(
    executor: &dyn Executor,
    open: &mut Batch<'_, '_>,
    callable: u32,
    args: Vec<Value>,
) -> Item {
    let mut ran = Item::Done;
    open.items(executor, callable, vec![args], &mut |_, outcome| {
        ran = match problem(&outcome) {
            // The item's call or its store: nothing the item did.
            Item::Pause(_) if unavailable(&outcome) => {
                Item::Pause("an item's call or its store couldn't be reached".into())
            }
            other => other,
        };
        ControlFlow::Continue(())
    })
    .await;
    ran
}

fn unavailable(outcome: &Outcome) -> bool {
    matches!(outcome, Outcome::Failed(failure) if failure.code == "unavailable")
}

/// Commits an item that ran as a batch of its own, `item` what became of
/// it and, for a file's row, the row its problem is kept at (a query's is
/// its place in the walk); `None` when no item ran, every one being done.
/// In the transaction its writes are in, or one begun now, the run's row
/// is locked and held to `position`, where it was when the item was read;
/// then the item's problem, if its action failed, and the run's next
/// position commit with its writes.
async fn committed(
    mut open: Batch<'_, '_>,
    plan: &BatchPlan,
    id: &SqlValue,
    position: Option<&str>,
    item: Option<(Item, Option<i64>)>,
    next: Next<'_>,
    (now, lease_until): (i64, i64),
) -> Step {
    let (ran, rule, row) = match item {
        Some((Item::Pause(why), _)) => {
            open.rollback().await;
            return Step::Ended(Ran::Paused(why));
        }
        Some((Item::Problem(rule), row)) => (true, Some(rule), row),
        Some((Item::Done, row)) => (true, None, row),
        None => (false, None, None),
    };
    if open.changed() {
        open.rollback().await;
        return Step::Changed;
    }
    let locked = match open.writing(&plan.store).await {
        Ok(transaction) => transaction.query(LOCK, std::slice::from_ref(id)).await,
        Err(error) => Err(error),
    };
    let (done, had) = match locked {
        Ok(rows) => match rows.first().map(Vec::as_slice) {
            Some(
                [
                    SqlValue::Text(state),
                    held,
                    SqlValue::Bool(false),
                    SqlValue::Int8(done),
                    SqlValue::Int8(problems),
                    SqlValue::Bool(_),
                ],
            ) if state == BatchState::Running.name() => {
                let held = match held {
                    SqlValue::Text(text) => Some(text.as_str()),
                    _ => None,
                };
                // Another runner moved it on since the item was read.
                if held != position {
                    open.rollback().await;
                    return Step::Changed;
                }
                (*done, *problems)
            }
            _ => {
                open.rollback().await;
                return Step::Ended(Ran::NotClaimed);
            }
        },
        Err(error) => return abandoned(open, error).await,
    };
    let done = if ran { done + 1 } else { done };
    let likely_total = match next.total {
        Total::Count(query, batches, values) => {
            let args: Vec<Value> = values.get(..plan.from_args).unwrap_or_default().to_vec();
            match crate::read::count(open.ctx(), query, batches.json, &args).await {
                Ok(likely_total) => likely_total,
                Err(_) if open.changed() => {
                    open.rollback().await;
                    return Step::Changed;
                }
                Err(error) => return abandoned(open, error).await,
            }
        }
        Total::From(likely) => likely(done),
        Total::Kept => None,
    };
    let Some(transaction) = open.transaction() else {
        return Step::Ended(Ran::Paused("a batch without its transaction".into()));
    };
    let mut problems = had;
    if let Some(rule) = rule {
        let written = transaction
            .execute(
                PROBLEM,
                &[
                    id.clone(),
                    SqlValue::Int8(row.unwrap_or(done)),
                    // An action's problem has no column (area 12).
                    SqlValue::Text(String::new()),
                    SqlValue::Text(rule),
                ],
            )
            .await;
        if let Err(error) = written {
            return abandoned(open, error).await;
        }
        problems += 1;
    }
    let advanced = transaction
        .execute(
            ADVANCE,
            &[
                id.clone(),
                next.position
                    .clone()
                    .map_or(SqlValue::Null(SqlType::Text), SqlValue::Text),
                SqlValue::Bool(next.read_all),
                SqlValue::Int8(done),
                SqlValue::Int8(problems),
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
        Err(error) => Step::Ended(Ran::Paused(format!("its item couldn't commit ({error:?})"))),
    }
}

/// Whether the run `id` names has a cancel asked for, as [`PEEK`] reads
/// it on its own: a file's row whose action makes calls is never run once
/// one has been. Anything else it finds is the batch's own lock's to say.
pub(super) async fn cancelled(ctx: &mut Ctx<'_>, plan: &BatchPlan, id: &SqlValue) -> bool {
    !matches!(peek(ctx, plan, id).await, Ok((_, false)))
}

/// `finished:` whose action makes outside calls, once every item is done
/// (chunk 7.4b): a batch of its own, as an item is, its calls keyed from
/// the run, and its writes and the run's end committed together once they
/// have answered. `held` is what the run's row held when its last batch
/// found every item done, and `file` the file an export wrote.
#[allow(clippy::too_many_arguments)]
pub(super) async fn finished(
    executor: &dyn Executor,
    ctx: &mut Ctx<'_>,
    plan: &BatchPlan,
    start: &BatchStart,
    saved: Saved<'_>,
    held: &Held,
    now: i64,
    file: Option<Value>,
) -> Step {
    let id = SqlValue::Uuid(start.run);
    let Some(call) = &plan.finished else {
        return Step::Ended(Ran::Failed("a run with no `finished:` to call".into()));
    };
    let mut open = match Batch::open_for_calls(ctx, plan.isolation, saved.system) {
        Ok(open) => open,
        Err(error) => {
            return Step::Ended(Ran::Paused(format!(
                "the store refused its batch ({error:?})"
            )));
        }
    };
    // The problems, read in the snapshot its calls close.
    let problems = match open.ctx().read_transaction(&plan.store).await {
        Ok(transaction) => transaction.query(PROBLEMS, std::slice::from_ref(&id)).await,
        Err(error) => Err(error),
    };
    let problems = match problems {
        Ok(rows) => rows,
        Err(error) => return abandoned(open, error).await,
    };
    let args = arguments(&call.args, &summary(held, &problems, file), saved.values);
    open.ctx().set_call_key(SCOPE, key(start, "finished"));
    let failure = match run(executor, &mut open, call.callable, args).await {
        Item::Pause(why) => {
            open.rollback().await;
            return Step::Ended(Ran::Paused(why));
        }
        Item::Problem(rule) => Some(format!("its `finished:` failed with `{rule}`")),
        Item::Done => None,
    };
    if open.changed() {
        open.rollback().await;
        return Step::Changed;
    }
    let locked = match open.writing(&plan.store).await {
        Ok(transaction) => transaction.query(LOCK, std::slice::from_ref(&id)).await,
        Err(error) => Err(error),
    };
    match locked {
        Ok(rows) => match rows.first().map(Vec::as_slice) {
            Some([SqlValue::Text(state), position, SqlValue::Bool(true), ..])
                if state == BatchState::Running.name() =>
            {
                let position = match position {
                    SqlValue::Text(text) => Some(text.as_str()),
                    _ => None,
                };
                if position != held.position.as_deref() {
                    open.rollback().await;
                    return Step::Changed;
                }
            }
            _ => {
                open.rollback().await;
                return Step::Ended(Ran::NotClaimed);
            }
        },
        Err(error) => return abandoned(open, error).await,
    }
    let state = if failure.is_some() {
        BatchState::Failed
    } else {
        BatchState::Finished
    };
    let ended = end_in(&mut open, &id, state, failure.as_deref(), now).await;
    let ran = match failure {
        Some(why) => Ran::Failed(why),
        None => Ran::Finished,
    };
    finish(open, ended, ran).await
}
