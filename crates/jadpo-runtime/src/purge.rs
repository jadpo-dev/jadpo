//! Purge (area 5, "Lifecycle"): the permanent deletion of an entity's
//! hidden records once they have been hidden for its lifecycle's
//! `after:`, in small batches, each its own transaction. It is the
//! compiler's own operation, outside every action and policy (area 9);
//! the built-in hourly purge schedule calls [`purge`] for each entity
//! whose lifecycle has `purge:` (area 12, which comes with chunk 6.2 and
//! logs each run's batches). A run stops at its budget of batches, so a
//! Workers cron run can't overrun its limits however much is due; the
//! next run carries on where it stopped.

use jadpo_std::write::{PURGE_BATCH, PurgePlan, PurgeSource};
use jadpo_value::Value;

use crate::read::to_sql;
use crate::{Host, StoreError};

/// How many batches one purge run deletes at most, [`PURGE_BATCH`]
/// records each: what the schedule gives a run, so it ends well inside a
/// Workers cron run's limits (area 12).
pub const PURGE_BATCHES: u32 = 20;

/// What one purge run deleted.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Purged {
    /// The entity's name.
    pub entity: String,
    /// How many records each batch deleted, in order; when the run
    /// finished, the last deleted fewer than a whole batch.
    pub batches: Vec<u64>,
    /// Whether it deleted every record due: `false` when it stopped at its
    /// budget, leaving the rest to the next run.
    pub finished: bool,
}

impl Purged {
    /// How many records the run deleted.
    pub fn removed(&self) -> u64 {
        self.batches.iter().sum()
    }
}

/// Runs `plan` at `now` (the run's time, milliseconds since the epoch):
/// batch after batch until one deletes fewer than a whole batch, or
/// `budget` batches have run ([`PURGE_BATCHES`] for the schedule). A batch
/// that fails is rolled back and ends the run; the batches before it stay
/// deleted, and the next run carries on where it stopped, as it does after
/// a run that used its budget.
pub async fn purge(
    host: &dyn Host,
    plan: &PurgePlan,
    now: i64,
    budget: u32,
) -> Result<Purged, StoreError> {
    let Some(database) = host.database(&plan.store) else {
        return Err(StoreError::Fault(format!(
            "no connection to the store `{}`",
            plan.store
        )));
    };
    let cutoff = now.saturating_sub(plan.after);
    let mut params = Vec::with_capacity(plan.params.len());
    for param in &plan.params {
        let value = match param.source {
            PurgeSource::Cutoff => Value::Instant(cutoff),
            PurgeSource::Now => Value::Instant(now),
        };
        params.push(to_sql(&value, param.ty).map_err(StoreError::Fault)?);
    }
    let mut purged = Purged {
        entity: plan.entity.clone(),
        batches: Vec::new(),
        finished: false,
    };
    for _ in 0..budget {
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
