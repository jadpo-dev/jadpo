//! A run over a provider's list operation, walked by `page.next` (area
//! 12, "Walking records or a provider's list: `batches.each`"; chunk J2).
//! A file helper's write over one asks for its pages with [`ask`] too, and
//! writes them as an export does (chunk J4).
//!
//! No outside call goes out while a transaction is open, so each page is
//! asked for first, as the system and with no transaction, with the
//! run's saved input and, after the first page, the cursor the page
//! before gave put in the input's `PageCursor` field. Then the batch's
//! transaction opens, holds the run's row to the position the page was
//! asked after, runs the page's items, and commits their writes with the
//! problems and the page's `next` cursor as the run's position. A page is
//! one batch, whatever its size: the provider, not the run, says where a
//! page ends, and a page asked for again is asked with the same cursor.
//! A run whose page couldn't be had waits for the tick; the list's own
//! failure, or an answer that isn't a page, ends it `failed`.

use jadpo_std::BatchState;
use jadpo_std::batch::{BatchPlan, Listed};
use jadpo_value::Value;

use super::{Batches, Fetched, Ran, Saved, Step, batch, batch_of, with_at};
use crate::provider::{Calling, Done, Site};
use crate::{Ctx, Executor, SqlValue};

/// The codes of a list's failure that pass: the run waits for the tick.
const PASSING: [&str; 3] = ["unavailable", "rate_limited", "timed_out"];

/// One page of a run over a provider's list, or, once its last page has
/// committed or a cancel was asked for, what any run's last batch does.
#[allow(clippy::too_many_arguments)]
pub(super) async fn page(
    executor: &dyn Executor,
    ctx: &mut Ctx<'_>,
    batches: Batches<'_>,
    plan: &BatchPlan,
    listed: &Listed,
    start: &super::BatchStart,
    saved: Saved<'_>,
    times: (i64, i64),
) -> Step {
    let id = SqlValue::Uuid(start.run);
    let (held, cancelled) = match super::calling::peek(ctx, plan, &id).await {
        Ok(found) => found,
        Err(step) => return step,
    };
    if cancelled || held.read_all {
        return batch(executor, ctx, batches, plan, start, saved, times).await;
    }
    let (items, next) = match ask(
        executor,
        ctx,
        listed,
        start,
        saved,
        held.position.as_deref(),
    )
    .await
    {
        Ok(page) => page,
        Err(step) => return step,
    };
    let fetched = Fetched {
        after: held.position,
        items,
        next,
    };
    batch_of(
        executor,
        ctx,
        batches,
        plan,
        start,
        saved,
        times,
        Some(fetched),
    )
    .await
}

/// Asks `listed` for the page after `cursor`, the first when there is
/// none, with the run's saved input and the cursor put in its
/// `PageCursor` field, as the system and with no transaction open: its
/// items and the cursor of the page after it. A list that can't give it
/// now pauses the run for the tick; its own failure, a fault or an
/// answer that isn't a page ends the run `failed`. A file helper's write
/// over a provider's list asks for its pages here too (chunk J4).
pub(super) async fn ask(
    executor: &dyn Executor,
    ctx: &mut Ctx<'_>,
    listed: &Listed,
    start: &super::BatchStart,
    saved: Saved<'_>,
    cursor: Option<&str>,
) -> Result<(Vec<Value>, Option<String>), Step> {
    let mut input = saved.values.first().cloned().unwrap_or(Value::Unit);
    if let Some(cursor) = cursor {
        input = with_at(&input, &listed.cursor, Value::text(cursor));
    }
    let site = jadpo_std::provider::CallSite {
        called: listed.called,
        ..jadpo_std::provider::CallSite::default()
    };
    let calling = Calling {
        plan: executor.plan(),
        site: Site {
            call: &site,
            functions: &crate::receiving::NoFunctions,
            at: 0,
        },
    };
    let operation = listed.operation.as_str();
    match crate::provider::call(ctx, calling, Some(input)).await {
        Done::Value(Value::Record(record)) => Ok((
            match record.get("items") {
                Some(Value::List(items)) => items.to_vec(),
                _ => Vec::new(),
            },
            match record.get("next") {
                Some(Value::Text(cursor)) => Some(cursor.to_string()),
                _ => None,
            },
        )),
        Done::Value(_) => {
            Err(failed(ctx, start, &format!("`{operation}` answered with no page")).await)
        }
        Done::Failed(failure) if PASSING.contains(&failure.code.as_str()) => {
            Err(Step::Ended(Ran::Paused(format!(
                "`{operation}` couldn't give its page (`{}`)",
                failure.code
            ))))
        }
        Done::Failed(failure) => {
            let why = format!("`{operation}` refused its page (`{}`)", failure.code);
            Err(failed(ctx, start, &why).await)
        }
        Done::Fault(kind) => {
            let why = format!(
                "asking `{operation}` for its page faulted with `{}`",
                kind.classification()
            );
            Err(failed(ctx, start, &why).await)
        }
    }
}

/// Ends the run `start` names `failed`, saying `why`.
async fn failed(ctx: &mut Ctx<'_>, start: &super::BatchStart, why: &str) -> Step {
    super::end(ctx.host(), start, BatchState::Failed, Some(why)).await;
    Step::Ended(Ran::Failed(why.to_string()))
}
