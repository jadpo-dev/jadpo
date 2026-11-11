//! Batch runs as an operator sees them with `jadpo batches` (area 14,
//! "`jadpo batches`"; area 12, "What an operator sees"): each run kept in a
//! store, its state and counts as `batches.status` gives them, the cancel
//! asked of it and the last thing an operator did to it; its problems a
//! page at a time; and a cancel asked from the command line, logged with
//! who asked.
//!
//! Nothing here runs the application's code, and nothing reads what a run
//! saved: a run's row is shown by its call's name, its counts, its times
//! and why it failed, a sentence the runtime wrote that never holds a
//! value, and a problem by its row, column and rule.

use crate::deliveries::{Acted, Done};
use crate::outbox::micros;
use crate::{Database, Host, SqlType, SqlValue, StoreError};

pub const RUNNING: &str = "running";
pub const FINISHED: &str = "finished";
pub const FAILED: &str = "failed";
pub const CANCELLED: &str = "cancelled";

/// The states a run is kept in, in the order a list counts them.
pub const STATES: [&str; 4] = [RUNNING, FINISHED, FAILED, CANCELLED];

/// The kind an operator's log row names a batch run by.
pub const KIND: &str = "batch_run";

/// A batch run as an operator sees it, times in milliseconds since the
/// epoch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Kept {
    pub id: [u8; 16],
    /// Which call started it, by its stable name (area 12, "Which call
    /// started a run").
    pub batch: String,
    pub state: String,
    pub items_done: i64,
    pub problems: i64,
    /// The total its query's count or its file's size estimates, once its
    /// first page has worked one out.
    pub estimated_total: Option<i64>,
    /// Why it failed, in the runtime's words; never a value.
    pub failure: Option<String>,
    pub started_at: i64,
    /// When its last page committed, or it ended.
    pub stepped_at: i64,
    pub finished_at: Option<i64>,
    /// When a cancel was asked of it, by the app or an operator.
    pub cancel_asked: Option<i64>,
    /// The last thing an operator did to it.
    pub acted: Option<Acted>,
}

impl Kept {
    /// The total `batches.status` gives: once finished, the items it did;
    /// otherwise its estimate, if it has one yet.
    pub fn total(&self) -> Option<i64> {
        if self.state == FINISHED {
            Some(self.items_done)
        } else {
            self.estimated_total
        }
    }

    /// When a run still going should finish, at the rate it has gone so
    /// far: from its start to its last committed page, over the items
    /// still to do. `None` until it has a total and has done some, or once
    /// it has done what it estimated.
    pub fn finish_estimate(&self) -> Option<i64> {
        if self.state != RUNNING {
            return None;
        }
        let total = self.estimated_total?;
        let elapsed = self.stepped_at.checked_sub(self.started_at)?;
        if self.items_done <= 0 || total <= self.items_done || elapsed <= 0 {
            return None;
        }
        let left =
            i128::from(total - self.items_done) * i128::from(elapsed) / i128::from(self.items_done);
        let left = i64::try_from(left).ok()?;
        self.stepped_at.checked_add(left)
    }
}

/// One item's problem: its row as the walk or a spreadsheet numbers it,
/// its column (empty for a query's item) and its rule, never its value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
    pub row: i64,
    pub column: String,
    pub rule: String,
}

/// The runs in states `$1`, those running first, then most recently
/// ended first, at most `$2`: each with the cancel asked of it and the
/// last thing an operator did to it.
pub const LIST: &str = "SELECT \"r\".\"id\", \"r\".\"batch\", \"r\".\"state\", \"r\".\"items_done\", \"r\".\"problems\", \"r\".\"estimated_total\", \"r\".\"failure\", \"r\".\"started_at\", \"r\".\"stepped_at\", \"r\".\"finished_at\", \"c\".\"requested_at\", \"a\".\"acted_at\", \"a\".\"action\", \"a\".\"operator\", \"a\".\"role\" FROM \"jadpo_batch_runs\" AS \"r\" LEFT JOIN \"jadpo_batch_cancels\" AS \"c\" ON \"c\".\"run_id\" = \"r\".\"id\" LEFT JOIN LATERAL (SELECT \"acted_at\", \"action\", \"operator\", \"role\" FROM \"jadpo_operator_actions\" WHERE \"item\" = \"r\".\"id\" ORDER BY \"acted_at\" DESC, \"id\" DESC LIMIT 1) AS \"a\" ON TRUE WHERE \"r\".\"state\" = ANY($1::text[]) ORDER BY \"r\".\"state\" = 'running' DESC, COALESCE(\"r\".\"finished_at\", \"r\".\"started_at\") DESC, \"r\".\"id\" DESC LIMIT $2";

/// The runs whose id, as 32 hex digits, ends with `$1`, read as
/// [`LIST`] reads them.
pub const FIND: &str = "SELECT \"r\".\"id\", \"r\".\"batch\", \"r\".\"state\", \"r\".\"items_done\", \"r\".\"problems\", \"r\".\"estimated_total\", \"r\".\"failure\", \"r\".\"started_at\", \"r\".\"stepped_at\", \"r\".\"finished_at\", \"c\".\"requested_at\", \"a\".\"acted_at\", \"a\".\"action\", \"a\".\"operator\", \"a\".\"role\" FROM \"jadpo_batch_runs\" AS \"r\" LEFT JOIN \"jadpo_batch_cancels\" AS \"c\" ON \"c\".\"run_id\" = \"r\".\"id\" LEFT JOIN LATERAL (SELECT \"acted_at\", \"action\", \"operator\", \"role\" FROM \"jadpo_operator_actions\" WHERE \"item\" = \"r\".\"id\" ORDER BY \"acted_at\" DESC, \"id\" DESC LIMIT 1) AS \"a\" ON TRUE WHERE right(replace(\"r\".\"id\"::text, '-', ''), length($1::text)) = $1::text";

/// How many runs are in each state.
pub const COUNTS: &str = "SELECT \"state\", count(*) FROM \"jadpo_batch_runs\" GROUP BY \"state\"";

/// Run `$1`'s problems in row order, at most `$2` after the first `$3`.
pub const PROBLEMS_PAGE: &str = "SELECT \"row\", \"column\", \"rule\" FROM \"jadpo_batch_problems\" WHERE \"run_id\" = $1 ORDER BY \"row\" LIMIT $2 OFFSET $3";

/// Asks run `$1` to stop after the batch in hand, at `$2`, while it is
/// running and nothing has asked it yet, as `batches.cancel` does, and
/// logs it with id `$3` by `$4`: one statement, so the cancel and its log
/// row commit together. The role it was logged as; no row when the run
/// had ended or a cancel was already asked.
pub const CANCEL_LOGGED: &str = "WITH \"r\" AS (SELECT \"id\", \"batch\" FROM \"jadpo_batch_runs\" WHERE \"id\" = $1 AND \"state\" = 'running'), \"c\" AS (INSERT INTO \"jadpo_batch_cancels\" (\"run_id\", \"requested_at\") SELECT \"id\", $2 FROM \"r\" ON CONFLICT (\"run_id\") DO NOTHING RETURNING \"run_id\") INSERT INTO \"jadpo_operator_actions\" (\"id\", \"acted_at\", \"action\", \"kind\", \"item\", \"target\", \"event\", \"was\", \"operator\", \"role\") SELECT $3, $2, 'cancel', 'batch_run', \"r\".\"id\", \"r\".\"batch\", NULL, 'running', $4, session_user FROM \"r\" JOIN \"c\" ON \"c\".\"run_id\" = \"r\".\"id\" RETURNING \"role\"";

/// Sets failed run `$1` aside with `jadpo deliveries dismiss`, at `$2`
/// with id `$3` by `$4`, unless an operator already has: its log row, as a
/// schedule's run is dismissed. The role it was logged as.
pub const DISMISS_LOGGED: &str = "INSERT INTO \"jadpo_operator_actions\" (\"id\", \"acted_at\", \"action\", \"kind\", \"item\", \"target\", \"event\", \"was\", \"operator\", \"role\") SELECT $3, $2, 'dismiss', 'batch_run', \"r\".\"id\", \"r\".\"batch\", NULL, 'failed', $4, session_user FROM \"jadpo_batch_runs\" AS \"r\" WHERE \"r\".\"id\" = $1 AND \"r\".\"state\" = 'failed' AND NOT EXISTS (SELECT 1 FROM \"jadpo_operator_actions\" AS \"a\" WHERE \"a\".\"item\" = \"r\".\"id\" AND \"a\".\"action\" = 'dismiss') RETURNING \"role\"";

/// Operators' log rows on batch runs acted on at or before `$1` whose run
/// is gone, at most `$2`: what the hourly cleanup deletes once a run has
/// been, as it does a delivery's.
pub const OLD_ACTIONS: &str = "DELETE FROM \"jadpo_operator_actions\" WHERE \"id\" IN (SELECT \"a\".\"id\" FROM \"jadpo_operator_actions\" AS \"a\" WHERE \"a\".\"kind\" = 'batch_run' AND \"a\".\"acted_at\" <= $1 AND NOT EXISTS (SELECT 1 FROM \"jadpo_batch_runs\" AS \"r\" WHERE \"r\".\"id\" = \"a\".\"item\") LIMIT $2) RETURNING 1";

/// The runs of one store in `states`, those running first, then most
/// recently ended first, at most `limit`.
pub async fn list(
    database: &dyn Database,
    states: &[&str],
    limit: i64,
) -> Result<Vec<Kept>, StoreError> {
    let wanted = SqlValue::Array(
        SqlType::Text,
        states
            .iter()
            .map(|s| SqlValue::Text(s.to_string()))
            .collect(),
    );
    let rows = database
        .query_once(LIST, &[wanted, SqlValue::Int8(limit)])
        .await?;
    Ok(rows.iter().filter_map(|row| read_kept(row)).collect())
}

/// The runs of one store whose id, as 32 hex digits, ends with `suffix`.
pub async fn find(database: &dyn Database, suffix: &str) -> Result<Vec<Kept>, StoreError> {
    let rows = database
        .query_once(FIND, &[SqlValue::Text(suffix.to_string())])
        .await?;
    Ok(rows.iter().filter_map(|row| read_kept(row)).collect())
}

/// How many of one store's runs are in each state.
pub async fn counts(database: &dyn Database) -> Result<Vec<(String, u64)>, StoreError> {
    let rows = database.query_once(COUNTS, &[]).await?;
    Ok(rows
        .iter()
        .filter_map(|row| match row.as_slice() {
            [SqlValue::Text(state), SqlValue::Int8(n)] => {
                Some((state.clone(), u64::try_from(*n).unwrap_or(0)))
            }
            _ => None,
        })
        .collect())
}

/// Run `run`'s problems in row order: at most `limit` after the first
/// `skip`.
pub async fn problems(
    database: &dyn Database,
    run: &[u8; 16],
    limit: i64,
    skip: i64,
) -> Result<Vec<Problem>, StoreError> {
    let rows = database
        .query_once(
            PROBLEMS_PAGE,
            &[
                SqlValue::Uuid(*run),
                SqlValue::Int8(limit),
                SqlValue::Int8(skip),
            ],
        )
        .await?;
    Ok(rows
        .iter()
        .filter_map(|row| match row.as_slice() {
            [
                SqlValue::Int8(n),
                SqlValue::Text(column),
                SqlValue::Text(rule),
            ] => Some(Problem {
                row: *n,
                column: column.clone(),
                rule: rule.clone(),
            }),
            _ => None,
        })
        .collect())
}

/// Asks run `run` to stop after the batch in hand at `now` as
/// `operator`, logging it: [`Done::Changed`] when it had ended or a
/// cancel was already asked.
pub async fn cancel(
    database: &dyn Database,
    host: &dyn Host,
    run: &[u8; 16],
    operator: &str,
    now: i64,
) -> Result<Done, StoreError> {
    let rows = database
        .query_once(
            CANCEL_LOGGED,
            &[
                SqlValue::Uuid(*run),
                micros(now),
                SqlValue::Uuid(log_id(host, now)?),
                SqlValue::Text(operator.to_string()),
            ],
        )
        .await?;
    Ok(logged(&rows))
}

/// Sets failed run `run` aside at `now` as `operator`, logging it:
/// [`Done::Changed`] when it isn't failed or was already set aside.
pub async fn dismiss(
    database: &dyn Database,
    host: &dyn Host,
    run: &[u8; 16],
    operator: &str,
    now: i64,
) -> Result<Done, StoreError> {
    let rows = database
        .query_once(
            DISMISS_LOGGED,
            &[
                SqlValue::Uuid(*run),
                micros(now),
                SqlValue::Uuid(log_id(host, now)?),
                SqlValue::Text(operator.to_string()),
            ],
        )
        .await?;
    Ok(logged(&rows))
}

fn log_id(host: &dyn Host, now: i64) -> Result<[u8; 16], StoreError> {
    crate::ctx::random_id(host, now)
        .ok_or_else(|| StoreError::Fault("no random bytes for a log row's id".into()))
}

fn logged(rows: &[crate::Row]) -> Done {
    match rows.first().and_then(|row| row.first()) {
        Some(SqlValue::Text(role)) => Done::Logged { role: role.clone() },
        _ => Done::Changed,
    }
}

fn read_kept(row: &[SqlValue]) -> Option<Kept> {
    let text = |at: usize| match row.get(at) {
        Some(SqlValue::Text(text)) => Some(text.clone()),
        _ => None,
    };
    let millis = |at: usize| match row.get(at) {
        Some(SqlValue::Timestamp(micros)) => Some(micros.div_euclid(1000)),
        _ => None,
    };
    let count = |at: usize| match row.get(at) {
        Some(SqlValue::Int8(n)) => Some(*n),
        _ => None,
    };
    let id = match row.first() {
        Some(SqlValue::Uuid(id)) => *id,
        _ => return None,
    };
    let acted = match (millis(11), text(12), text(13), text(14)) {
        (Some(at), Some(action), Some(operator), Some(role)) => Some(Acted {
            at,
            action,
            operator,
            role,
        }),
        _ => None,
    };
    Some(Kept {
        id,
        batch: text(1)?,
        state: text(2)?,
        items_done: count(3)?,
        problems: count(4)?,
        estimated_total: count(5),
        failure: text(6),
        started_at: millis(7)?,
        stepped_at: millis(8)?,
        finished_at: millis(9),
        cancel_asked: millis(10),
        acted,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kept(state: &str, done: i64, total: Option<i64>, started: i64, stepped: i64) -> Kept {
        Kept {
            id: [0; 16],
            batch: "action import_products reads csv ProductRow each action save_product"
                .to_string(),
            state: state.to_string(),
            items_done: done,
            problems: 0,
            estimated_total: total,
            failure: None,
            started_at: started,
            stepped_at: stepped,
            finished_at: None,
            cancel_asked: None,
            acted: None,
        }
    }

    /// A run still going finishes at the rate it has gone so far: 400 of
    /// about 1,200 in 4 minutes leaves 800, another 8 minutes.
    #[test]
    fn a_running_run_finishes_at_the_rate_it_has_gone() {
        let run = kept(RUNNING, 400, Some(1_200), 0, 240_000);
        assert_eq!(run.finish_estimate(), Some(240_000 + 480_000));
        assert_eq!(run.total(), Some(1_200));
    }

    /// No estimate before a total or any item, past its estimate, or once
    /// it has ended; a finished run's total is what it did.
    #[test]
    fn no_estimate_without_a_rate_or_once_ended() {
        assert_eq!(kept(RUNNING, 0, Some(10), 0, 1).finish_estimate(), None);
        assert_eq!(kept(RUNNING, 5, None, 0, 1).finish_estimate(), None);
        assert_eq!(kept(RUNNING, 12, Some(10), 0, 1).finish_estimate(), None);
        assert_eq!(kept(RUNNING, 5, Some(10), 7, 7).finish_estimate(), None);
        let finished = kept(FINISHED, 21, Some(30), 0, 9);
        assert_eq!(finished.finish_estimate(), None);
        assert_eq!(finished.total(), Some(21));
    }
}
