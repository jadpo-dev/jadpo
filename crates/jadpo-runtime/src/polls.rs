//! Polls (area 11, "Polls"; area 12, "Events from outside"; chunk 7.6c):
//! the tick calls each poll whose event the app subscribes to through its
//! connection, at its interval, and keeps the event its answer makes only
//! when that changed.
//!
//! - **Due, then claimed once.** What each poll last saw is a row of
//!   `jadpo_polls` in the store its events are kept in, by its name, `poll
//!   <connection>.<poll>`. One statement takes a poll that is due, or has
//!   never run, and moves it to its next time, so of several instances
//!   ticking only one calls it. A poll whose interval was shortened since
//!   it was claimed, the interval kept on its row, is due at once.
//! - **Called as the runtime's own work**: its operation, which takes
//!   nothing, through its connection with its credential and host, as the
//!   system, with no request, logged as any call is and capped by the
//!   connection's `max_concurrent:`.
//! - **Only changes kept.** The event is made from the answer, each of its
//!   fields taken from the answer's field of its name, and written with
//!   its codec; the digest of what is written is compared with the last
//!   one kept. The same is nothing; a change, or the first answer, keeps
//!   the event in the outbox with a delivery for each subscriber, and the
//!   new digest, in one transaction, so a change is never lost or kept
//!   twice. Fields the event doesn't carry (when the answer was fetched)
//!   never count.
//! - **A failed call** keeps nothing and is tried again a minute later
//!   ([`AGAIN_MS`]); the tick reports it, naming the operation and never a
//!   value.

use std::rc::Rc;

use jadpo_std::provider::PollPlan;
use jadpo_value::Value;

use crate::budget::Budget;
use crate::outbox::{Delivery, micros};
use crate::runner::Deferred;
use crate::webhooks::{Kept, keep_claimed};
use crate::{Ctx, Executor, SqlValue, StoreError};

/// How long after a failed call a poll is tried again.
pub const AGAIN_MS: i64 = 60_000;

/// Takes poll `$1` at `$2` when it is due, or has never run, moving it to
/// `$3`, its next time, under interval `$4`; and when it was claimed
/// under another interval and is due later than `$3`, its interval
/// shortened since. A claim under the same interval is due only at its
/// time, so neither a second tick nor an instance whose clock is behind
/// takes it again (chunk 7.9c). The digest of the event it last kept.
pub const CLAIM: &str = "INSERT INTO \"jadpo_polls\" AS \"p\" (\"poll\", \"next_at\", \"every_ms\") VALUES ($1, $3, $4::bigint) ON CONFLICT (\"poll\") DO UPDATE SET \"next_at\" = $3, \"every_ms\" = $4::bigint WHERE \"p\".\"next_at\" <= $2 OR (\"p\".\"every_ms\" IS DISTINCT FROM $4::bigint AND \"p\".\"next_at\" > $3) RETURNING \"p\".\"digest\"";

/// Poll `$1`, whose call failed, due again at `$2`.
pub const AGAIN: &str = "UPDATE \"jadpo_polls\" SET \"next_at\" = $2 WHERE \"poll\" = $1";

/// Records digest `$2` for poll `$1` at `$3`, unless it is the one kept
/// already: a row when the answer changed.
pub const CHANGED: &str = "UPDATE \"jadpo_polls\" SET \"digest\" = $2, \"last_change\" = $3 WHERE \"poll\" = $1 AND \"digest\" IS DISTINCT FROM $2 RETURNING \"poll\"";

/// What one tick's polls did.
#[derive(Debug, Default)]
pub struct Polled {
    /// The deliveries of the changes kept.
    pub deferred: Vec<Deferred>,
    /// How many polls were called, and how many of those saw a change.
    pub called: u64,
    pub changed: u64,
    /// Each poll or store the tick couldn't poll, with why.
    pub problems: Vec<(String, StoreError)>,
}

/// Runs every poll of `executor`'s that is due at `now`, within `budget`.
pub async fn poll(executor: &dyn Executor, budget: &Budget<'_>, now: i64) -> Polled {
    let mut out = Polled::default();
    let Some(polls) = executor.plan().polls() else {
        return out;
    };
    let host = budget.host();
    for plan in polls {
        if budget.spent() {
            break;
        }
        let Some(database) = host.database(&plan.store) else {
            out.problems.push((
                plan.store.clone(),
                StoreError::Fault(format!("no store called `{}`", plan.store)),
            ));
            continue;
        };
        let every = i64::try_from(plan.every_ms).unwrap_or(i64::MAX);
        let claimed = database
            .query_once(
                CLAIM,
                &[
                    SqlValue::Text(plan.key()),
                    micros(now),
                    micros(now.saturating_add(every)),
                    SqlValue::Int8(every),
                ],
            )
            .await;
        let last = match claimed {
            Ok(rows) => match rows.first() {
                // Not due: another instance took it, or it ran lately.
                None => continue,
                Some(row) => match row.first() {
                    Some(SqlValue::Text(digest)) => Some(digest.clone()),
                    _ => None,
                },
            },
            Err(error) => {
                out.problems.push((plan.store.clone(), error));
                continue;
            }
        };
        out.called += 1;
        let mut ctx = crate::receiving::system_ctx(executor, host, now);
        match taken(executor, plan, &mut ctx, last).await {
            Ok(Some(deliveries)) => {
                out.changed += 1;
                if !deliveries.is_empty() {
                    out.deferred.push(Deferred::Deliveries(deliveries));
                }
            }
            Ok(None) => {}
            Err(error) => {
                let _ = database
                    .query_once(
                        AGAIN,
                        &[
                            SqlValue::Text(plan.key()),
                            micros(now.saturating_add(AGAIN_MS)),
                        ],
                    )
                    .await;
                out.problems.push((plan.key(), error));
            }
        }
    }
    out
}

/// Calls `plan`'s operation in `ctx` and keeps the event its answer
/// makes when it isn't the one `last` digests: its deliveries, or `None`
/// for an answer that changed nothing.
async fn taken(
    executor: &dyn Executor,
    plan: &PollPlan,
    ctx: &mut Ctx<'_>,
    last: Option<String>,
) -> Result<Option<Vec<Delivery>>, StoreError> {
    let Some(events) = executor.plan().events() else {
        return Err(StoreError::Fault("a program with no events".into()));
    };
    let answer =
        crate::receiving::call_through(executor, (&plan.connection, &plan.operation), ctx, None)
            .await?;
    let named = format!("`{}.{}`", plan.connection, plan.operation);
    let Value::Record(record) = &answer else {
        return Err(StoreError::Fault(format!("{named} gave no object")));
    };
    let fields = plan
        .fields
        .iter()
        .map(|name| {
            (
                Rc::from(name.as_str()),
                record.get(name).cloned().unwrap_or(Value::None),
            )
        })
        .collect();
    let made = Value::record(&plan.event, fields);
    let json = events.json;
    let payload = match json.write(plan.number, &made) {
        Ok(SqlValue::Json(text)) => text,
        Ok(_) => return Err(StoreError::Fault(format!("{named} made no event"))),
        Err(why) => return Err(StoreError::Fault(format!("{named}'s event: {why}"))),
    };
    if payload.len() > crate::outbox::MAX_EVENT_BYTES {
        return Err(StoreError::Fault(format!("{named}'s event is past 64 KB")));
    }
    let value = json
        .read(plan.number, &payload)
        .map_err(|why| StoreError::Fault(format!("{named}'s event: {why}")))?;
    let digest: String = jadpo_value::hash::sha256(payload.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    if last.as_deref() == Some(digest.as_str()) {
        return Ok(None);
    }
    let trace = ctx.new_trace();
    ctx.set_trace(Some(trace));
    let now = ctx.now();
    let kept = Kept {
        store: &plan.store,
        source: plan.key(),
        event: &plan.event,
        connection: None,
    };
    let claim = vec![
        SqlValue::Text(plan.key()),
        SqlValue::Text(digest),
        micros(now),
    ];
    keep_claimed(executor, &kept, ctx, (CHANGED, claim), value, payload).await
}
