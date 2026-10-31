//! Purge (area 5, "Lifecycle"): the permanent deletion of an entity's
//! hidden records once they have been hidden for its lifecycle's
//! `after:`, in small batches, each its own transaction. It is the
//! compiler's own operation, outside every action and policy (area 9);
//! the built-in hourly purge schedule calls [`purge_all`] for each entity
//! whose lifecycle has `purge:` (area 12, which comes with chunk 6.2 and
//! logs each run's batches). Retention (area 15, "Retention"; chunk P2)
//! runs the same way: an entity's deletes its records once their time is
//! up, with their sign-in state when they sign in, and a field's clears
//! its value, each a [`PurgePlan`] of the entity's that the same run
//! applies. A run stops at its budget of batches, so a Workers cron run
//! can't overrun its limits however much is due; the next run carries on
//! where it stopped.

use jadpo_std::write::{PURGE_BATCH, PurgePlan, PurgeSource};
use jadpo_value::Value;

use crate::read::to_sql;
use crate::{Host, StoreError};

/// How many batches one purge run deletes at most, [`PURGE_BATCH`]
/// records each: what the schedule gives a run, so it ends well inside a
/// Workers cron run's limits (area 12).
pub const PURGE_BATCHES: u32 = 20;

/// What one purge run deleted, or a retention's cleared.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Purged {
    /// The entity's name.
    pub entity: String,
    /// How many records each batch deleted, or cleared a value of, in
    /// order; when a plan finished, its last did fewer than a whole batch.
    pub batches: Vec<u64>,
    /// Whether it deleted every record due: `false` when it stopped at its
    /// budget, leaving the rest to the next run.
    pub finished: bool,
}

impl Purged {
    /// How many records the run deleted, or cleared a value of.
    pub fn removed(&self) -> u64 {
        self.batches.iter().sum()
    }
}

/// Runs each of `plans`, an entity's purge schedule's in order with the
/// period each waits ([`crate::Plan::purges_of`]), at `now`, sharing
/// `batches` batches among them: each runs until a batch comes back
/// short, then the next, until the budget is spent. What they did, as one
/// run; `finished` only when every plan finished. A plan that fails ends
/// the run, the batches before it staying done.
pub async fn purge_all(
    host: &dyn Host,
    plans: &[(&PurgePlan, Option<i64>)],
    now: i64,
    batches: u32,
) -> Result<Purged, StoreError> {
    let mut out = Purged {
        entity: plans
            .first()
            .map(|(plan, _)| plan.entity.clone())
            .unwrap_or_default(),
        batches: Vec::new(),
        finished: true,
    };
    let mut left = batches;
    for (plan, after) in plans {
        if left == 0 {
            out.finished = false;
            break;
        }
        let ran = purge_after(host, plan, now, left, after.unwrap_or(0)).await?;
        left = left.saturating_sub(u32::try_from(ran.batches.len()).unwrap_or(u32::MAX));
        out.batches.extend(ran.batches);
        if !ran.finished {
            out.finished = false;
            break;
        }
    }
    Ok(out)
}

/// Runs `plan` at `now` (the run's time, milliseconds since the epoch):
/// batch after batch until one deletes fewer than a whole batch, or
/// `batches` batches have run ([`PURGE_BATCHES`] for the schedule). A batch
/// that fails is rolled back and ends the run; the batches before it stay
/// deleted, and the next run carries on where it stopped, as it does after
/// a run that used its budget.
pub async fn purge(
    host: &dyn Host,
    plan: &PurgePlan,
    now: i64,
    batches: u32,
) -> Result<Purged, StoreError> {
    if plan.after_setting.is_some() {
        return Err(StoreError::Fault(format!(
            "{} waits as long as a setting says, which only `purge_after` is given",
            plan.what()
        )));
    }
    purge_after(host, plan, now, batches, plan.after).await
}

/// [`purge`] for a plan whose period a setting gives (`after_setting`):
/// `after` is that setting's value, in milliseconds, which the application
/// started with. A period that is not above zero would delete a record the
/// moment it is hidden, so it deletes nothing.
pub async fn purge_after(
    host: &dyn Host,
    plan: &PurgePlan,
    now: i64,
    batches: u32,
    after: i64,
) -> Result<Purged, StoreError> {
    if after <= 0 {
        return Err(StoreError::Fault(format!(
            "{} waits for a period above zero",
            plan.what()
        )));
    }
    let Some(database) = host.database(&plan.store) else {
        return Err(StoreError::Fault(format!(
            "no connection to the store `{}`",
            plan.store
        )));
    };
    let cutoff = now.saturating_sub(after);
    let mut params = Vec::with_capacity(plan.params.len());
    for param in &plan.params {
        let value = match &param.source {
            PurgeSource::Cutoff => Value::Instant(cutoff),
            PurgeSource::Now => Value::Instant(now),
            // A literal or an enum value `visible:` compares with, as the
            // storage codec bound it when the plan was made (chunk 5.5w3b).
            PurgeSource::Stored(value) => {
                params.push(value.clone());
                continue;
            }
        };
        params.push(to_sql(&value, param.ty).map_err(StoreError::Fault)?);
    }
    let mut purged = Purged {
        entity: plan.entity.clone(),
        batches: Vec::new(),
        finished: false,
    };
    for _ in 0..batches {
        let mut transaction = database.begin().await?;
        let rows = match transaction.query(&plan.sql, &params).await {
            Ok(rows) => rows,
            Err(error) => {
                let _ = transaction.rollback().await;
                return Err(error);
            }
        };
        transaction.commit().await?;
        let removed = rows.len() as u64;
        purged.batches.push(removed);
        if removed < u64::from(PURGE_BATCH) {
            purged.finished = true;
            return Ok(purged);
        }
    }
    Ok(purged)
}
