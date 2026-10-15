//! Purge (area 5, "Lifecycle"): the permanent deletion of an entity's
//! hidden records once they have been hidden for its lifecycle's
//! `after:`, in small batches, each its own transaction. It is the
//! compiler's own operation, outside every action and policy (area 9);
//! the built-in hourly purge schedule calls [`purge`] for each entity
//! whose lifecycle has `purge:` (area 12, which comes with chunk 6.2 and
//! logs each run's batches).

use jadpo_std::write::{PURGE_BATCH, PurgePlan, PurgeSource};
use jadpo_value::Value;

use crate::read::to_sql;
use crate::{Host, StoreError};

/// What one purge run deleted.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Purged {
    /// The entity's name.
    pub entity: String,
    /// How many records each batch deleted, in order; the last deleted
    /// fewer than a whole batch.
    pub batches: Vec<u64>,
}

impl Purged {
    /// How many records the run deleted.
    pub fn removed(&self) -> u64 {
        self.batches.iter().sum()
    }
}

/// Runs `plan` at `now` (the run's time, milliseconds since the epoch):
/// batch after batch until one deletes fewer than a whole batch. A batch
/// that fails is rolled back and ends the run; the batches before it stay
/// deleted, and the next run carries on where it stopped.
pub async fn purge(host: &dyn Host, plan: &PurgePlan, now: i64) -> Result<Purged, StoreError> {
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
    };
    loop {
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
            return Ok(purged);
        }
    }
}
