//! Hourly, per-store cleanup after prompt work, under a fenced lease.
use super::*;
use crate::lease::{self, Leased};

pub const EVERY_MS: i64 = 60 * 60 * 1000;
/// One conditional upsert: the first tick inserts the singleton; later
/// ticks claim it only after a completed hour and any former lease.
pub const CLAIM: &str = "INSERT INTO \"jadpo_housekeeping\" (\"slot\", \"lease_id\", \"lease_until\") VALUES (0, $1, $2) ON CONFLICT (\"slot\") DO UPDATE SET \"lease_id\" = $1, \"lease_until\" = $2 WHERE (\"jadpo_housekeeping\".\"lease_until\" IS NULL OR \"jadpo_housekeeping\".\"lease_until\" <= $3) AND (\"jadpo_housekeeping\".\"last_cleaned_at\" IS NULL OR \"jadpo_housekeeping\".\"last_cleaned_at\" <= $4) RETURNING \"slot\"";
/// Only the matching lease identity may release or complete a pass. A partial pass keeps
/// the previous completion time, so the next tick can resume at once.
pub const RELEASE: &str = "UPDATE \"jadpo_housekeeping\" SET \"lease_id\" = NULL, \"lease_until\" = NULL, \"last_cleaned_at\" = COALESCE($2, \"last_cleaned_at\") WHERE \"slot\" = 0 AND \"lease_id\" = $1 RETURNING \"slot\"";

struct Pass {
    lease_id: [u8; 16],
    completed: bool,
}
impl Leased for Pass {
    fn claim(&self, now: i64, until: i64) -> (&'static str, Vec<SqlValue>) {
        (
            CLAIM,
            vec![
                SqlValue::Uuid(self.lease_id),
                micros(until),
                micros(now),
                micros(now.saturating_sub(EVERY_MS)),
            ],
        )
    }
    fn release(&self, now: i64) -> (&'static str, Vec<SqlValue>) {
        (
            RELEASE,
            vec![
                SqlValue::Uuid(self.lease_id),
                if self.completed {
                    micros(now)
                } else {
                    SqlValue::Null(SqlType::Timestamp)
                },
            ],
        )
    }
}

/// The union includes stores which hold no outbox (notably AI-only and
/// schedule-only stores). Never run an outbox statement against those.
fn stores(plan: &SchedulesPlan) -> Vec<String> {
    let mut stores = plan.stores.clone();
    stores.extend(plan.cleanup_schedules.iter().cloned());
    stores.extend(plan.cleanup_batches.iter().cloned());
    stores.extend(plan.sessions.iter().cloned());
    stores.extend(plan.keys.iter().cloned());
    stores.extend(plan.received.iter().cloned());
    stores.extend(plan.parked.iter().cloned());
    stores.extend(plan.ai_usage.iter().cloned());
    stores.extend(plan.steps.iter().cloned());
    stores.sort();
    stores.dedup();
    stores
}

pub(super) async fn run(
    host: &dyn Host,
    budget: &Budget<'_>,
    plan: &SchedulesPlan,
    now: i64,
    ticked: &mut Ticked,
) {
    for store in stores(plan) {
        if budget.spent() {
            break;
        }
        let database = match open(host, &store) {
            Ok(db) => db,
            Err(problem) => {
                ticked.problems.push(problem);
                continue;
            }
        };
        let mut lease_id = [0; 16];
        if !host.random(&mut lease_id) {
            ticked.problems.push((
                store.clone(),
                StoreError::Fault("housekeeping requires random lease ownership".into()),
            ));
            continue;
        }
        let mut pass = Pass {
            lease_id,
            completed: false,
        };
        match lease::claim(database, budget, &pass, now).await {
            Ok(Some(_)) => {}
            Ok(None) => continue,
            Err(error) => {
                ticked.problems.push((store.clone(), error));
                continue;
            }
        }
        let mut complete = true;
        let kept = now.saturating_sub(RUNS_KEPT_MS);
        // Retention work is every deployable's, regardless of which
        // triggers it executes. Delivery deletion precedes event deletion.
        if plan.stores.contains(&store) {
            match delete(host, budget, &store, EXPIRED_KEYS, now).await {
                Ok((n, done)) => {
                    ticked.deleted.keys += n;
                    complete &= done;
                }
                Err(error) => {
                    complete = false;
                    ticked.problems.push((store.clone(), error));
                }
            }
            if !budget.spent() {
                match settle_by_hand(
                    host,
                    budget,
                    &plan.stores,
                    &store,
                    kept,
                    &mut ticked.deleted,
                )
                .await
                {
                    Ok(done) => complete &= done,
                    Err(problem) => {
                        complete = false;
                        ticked.problems.push(problem);
                    }
                }
            } else {
                complete = false;
            }
            if !budget.spent() {
                match settle(
                    host,
                    budget,
                    &plan.stores,
                    &store,
                    now.saturating_sub(SETTLED_KEPT_MS),
                    &mut ticked.deleted,
                )
                .await
                {
                    Ok(done) => complete &= done,
                    Err(problem) => {
                        complete = false;
                        ticked.problems.push(problem);
                    }
                }
            } else {
                complete = false;
            }
        }
        let mut tasks: Vec<(&str, i64, &mut u64)> = Vec::new();
        let Deleted {
            actions,
            runs,
            sessions,
            service_keys,
            received,
            parked,
            ai_usage,
            answers,
            step_records,
            ..
        } = &mut ticked.deleted;
        // Counts sharing one tally are applied sequentially below.
        let mut action_tasks = Vec::new();
        if plan.stores.contains(&store) {
            action_tasks.push(OLD_DELIVERY_ACTIONS);
        }
        if plan.cleanup_schedules.contains(&store) {
            tasks.push((OLD_RUNS, kept, runs));
            action_tasks.push(OLD_RUN_ACTIONS);
        }
        if plan.sessions.as_ref() == Some(&store) {
            tasks.push((ENDED_SESSIONS, now, sessions));
        }
        if plan.keys.as_ref() == Some(&store) {
            tasks.push((ENDED_SERVICE_KEYS, now, service_keys));
        }
        if plan.received.as_ref() == Some(&store) {
            tasks.push((
                crate::webhooks::OLD_RECEIVED,
                now.saturating_sub(crate::webhooks::RECEIVED_KEPT_MS),
                received,
            ));
        }
        if plan.parked.as_ref() == Some(&store) {
            tasks.push((
                crate::receiving::OLD_DISMISSED,
                now.saturating_sub(crate::receiving::DISMISSED_KEPT_MS),
                parked,
            ));
            action_tasks.push(crate::receiving::OLD_MESSAGE_ACTIONS);
        }
        if plan.ai_usage.as_ref() == Some(&store) {
            tasks.push((
                crate::ai_budget::OLD_DAYS,
                now.saturating_sub(crate::ai_budget::KEPT_MS),
                ai_usage,
            ));
        }
        if plan.steps.contains(&store) {
            let before = now.saturating_sub(crate::unfinished::ANSWERS_KEPT_MS);
            tasks.push((crate::unfinished::OLD_ANSWERS, before, answers));
            tasks.push((crate::unfinished::ABANDONED, before, step_records));
            action_tasks.push(crate::unfinished::OLD_ACTIONS);
        }
        for (sql, before, count) in tasks {
            if budget.spent() {
                complete = false;
                break;
            }
            match delete(host, budget, &store, sql, before).await {
                Ok((n, done)) => {
                    *count += n;
                    complete &= done;
                }
                Err(error) => {
                    complete = false;
                    ticked.problems.push((store.clone(), error));
                }
            }
        }
        for sql in action_tasks {
            if budget.spent() {
                complete = false;
                break;
            }
            match delete(host, budget, &store, sql, kept).await {
                Ok((n, done)) => {
                    *actions += n;
                    complete &= done;
                }
                Err(error) => {
                    complete = false;
                    ticked.problems.push((store.clone(), error));
                }
            }
        }
        if plan.cleanup_batches.contains(&store) {
            for sql in [crate::batches::ENDED, crate::batches::UNKEPT_CANCELS] {
                if budget.spent() {
                    complete = false;
                    break;
                }
                match delete(
                    host,
                    budget,
                    &store,
                    sql,
                    now.saturating_sub(crate::batches::ENDED_KEPT_MS),
                )
                .await
                {
                    Ok((_, done)) => complete &= done,
                    Err(error) => {
                        complete = false;
                        ticked.problems.push((store.clone(), error));
                    }
                }
            }
        }
        pass.completed = complete && !budget.spent();
        // Release is cleanup of owned work, allowed after admission closes.
        let (sql, params) = pass.release(now);
        if let Err(error) = database.query_once(sql, &params).await {
            ticked.problems.push((store, error));
        }
    }
}
