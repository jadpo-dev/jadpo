//! Unfinished records of actions with several changing calls, as an
//! operator sees them (area 12, "What an operator sees"; area 11,
//! "Ending"; chunk 7.5c).
//!
//! A record that ends without committing never disappears quietly: a
//! delivery's stays with its delivery, which is the operator's item
//! (`failed`, `outcome_unknown`, `diverged`), and a route's becomes an
//! item of its own once its client's key has lapsed, [`ROUTE_LAPSES_MS`]
//! after its last run took it. `jadpo deliveries show` lists each step
//! (its operation, whether it was answered, when, and how many times its
//! key was replaced); `retry --not-done` sends the step in doubt again
//! under its next key generation; `abandon` ends it, keeping the step
//! list for tidying up by hand.
//!
//! - **What is read.** Never a request or an answer: a step's operation,
//!   its outcome (`answered`, `failed`, or none while in doubt) and its
//!   times, and a diverged record's problem (the step, its operation and
//!   the fields). A route's record is keyed by its client's key, which is
//!   never read here.
//! - **What is kept.** A record's answers are dropped [`ANSWERS_KEPT_MS`]
//!   after its last run, leaving its step list; an abandoned one goes that
//!   long after it was abandoned, with its steps ([`OLD_ANSWERS`],
//!   [`ABANDONED`]), both on the tick.

use crate::outbox::micros;
use crate::{Database, SqlValue, StoreError};

/// The scope a delivery's record is kept under; its key is the
/// delivery's id in hexadecimal.
pub const DELIVERY_SCOPE: &str = "delivery";

/// How long after its last run a route's record becomes an operator's
/// item: its client's key has lapsed (area 11, "Ending": 24 hours).
pub const ROUTE_LAPSES_MS: i64 = 24 * 60 * 60 * 1000;

/// How long a record keeps its answers after its last run, and an
/// abandoned one is kept after it was abandoned (area 12: 30 days).
pub const ANSWERS_KEPT_MS: i64 = 30 * 24 * 60 * 60 * 1000;

/// A record's columns, never its key: its id, whether it is a delivery's,
/// the delivery's id when it is, its action, build, step shape, first
/// start, state, problem and when its last run took it.
const COLUMNS: &str = "\"id\", \"scope\" = 'delivery', CASE WHEN \"scope\" = 'delivery' THEN \"key\" END, \"action\", \"build\", \"shape\", \"started_at\", \"state\", \"problem\"::text, \"taken_at\"";

/// The record of the delivery whose id, in hexadecimal, is `$1`.
pub fn of_delivery_sql() -> String {
    format!(
        "SELECT {COLUMNS} FROM \"jadpo_step_records\" WHERE \"scope\" = 'delivery' AND \"key\" = $1"
    )
}

/// The record whose id is `$1`.
pub fn by_id_sql() -> String {
    format!("SELECT {COLUMNS} FROM \"jadpo_step_records\" WHERE \"id\" = $1")
}

/// Every record not abandoned, oldest first.
pub fn unfinished_sql() -> String {
    format!(
        "SELECT {COLUMNS} FROM \"jadpo_step_records\" WHERE \"state\" <> 'abandoned' ORDER BY \"started_at\", \"id\""
    )
}

/// Record `$1`'s steps, in order: never a fingerprint or an answer, only
/// whether one is kept.
pub const STEPS: &str = "SELECT \"position\", \"operation\", \"key_generation\", \"dispatched_at\", \"outcome\", \"answer\" IS NOT NULL, \"answered_at\" FROM \"jadpo_steps\" WHERE \"record_id\" = $1 ORDER BY \"position\"";

/// Record `$1` ready for its delivery's retry, from the store it is in
/// when that isn't the delivery's: its generation back to none, since a
/// retry counts its attempts from none; and when `$3` names a step, that
/// step's answer cleared and its key moved to the next generation, sent
/// at `$2` ("not done"). Nothing when it was abandoned.
pub const RETRY: &str = "WITH \"r\" AS (UPDATE \"jadpo_step_records\" SET \"generation\" = 0 WHERE \"id\" = $1 AND \"state\" <> 'abandoned' RETURNING \"id\"), \"s\" AS (UPDATE \"jadpo_steps\" SET \"key_generation\" = \"key_generation\" + 1, \"outcome\" = NULL, \"answer\" = NULL, \"answered_at\" = NULL, \"dispatched_at\" = $2 WHERE \"record_id\" IN (SELECT \"id\" FROM \"r\") AND \"position\" = $3::int4 RETURNING 1) SELECT \"id\" FROM \"r\"";

/// Record `$1` abandoned at `$2`, from the store it is in when that isn't
/// its delivery's.
pub const ABANDON: &str = "UPDATE \"jadpo_step_records\" SET \"state\" = 'abandoned', \"taken_at\" = $2 WHERE \"id\" = $1 RETURNING \"id\"";

/// A route's record `$1` abandoned at `$2`, while it is what was read
/// (last taken at `$5`, not abandoned), logged as action id `$3` by `$4`,
/// which was in state `$6`: one statement. The role it was logged as.
pub const ABANDON_REQUEST: &str = "WITH \"r\" AS (UPDATE \"jadpo_step_records\" SET \"state\" = 'abandoned', \"taken_at\" = $2 WHERE \"id\" = $1 AND \"state\" <> 'abandoned' AND \"taken_at\" = $5 RETURNING \"id\", \"action\") INSERT INTO \"jadpo_operator_actions\" (\"id\", \"acted_at\", \"action\", \"kind\", \"item\", \"target\", \"event\", \"was\", \"operator\", \"role\") SELECT $3, $2, 'abandon', 'request', \"r\".\"id\", \"r\".\"action\", NULL, $6, $4, session_user FROM \"r\" RETURNING \"role\"";

/// Records whose last run took them at or before `$1` dropping their
/// answers, a batch of at most `$2` steps: what is left is the step list.
pub const OLD_ANSWERS: &str = "UPDATE \"jadpo_steps\" SET \"answer\" = NULL WHERE (\"record_id\", \"position\") IN (SELECT \"s\".\"record_id\", \"s\".\"position\" FROM \"jadpo_steps\" AS \"s\" JOIN \"jadpo_step_records\" AS \"r\" ON \"r\".\"id\" = \"s\".\"record_id\" WHERE \"r\".\"taken_at\" <= $1 AND \"s\".\"answer\" IS NOT NULL LIMIT $2) RETURNING 1";

/// Records abandoned at or before `$1`, at most `$2`, their steps with
/// them.
pub const ABANDONED: &str = "DELETE FROM \"jadpo_step_records\" WHERE \"id\" IN (SELECT \"id\" FROM \"jadpo_step_records\" WHERE \"state\" = 'abandoned' AND \"taken_at\" <= $1 LIMIT $2) RETURNING 1";

/// An operator's log rows on routes' records, written at or before `$1`,
/// whose record is gone, at most `$2`.
pub const OLD_ACTIONS: &str = "DELETE FROM \"jadpo_operator_actions\" WHERE \"id\" IN (SELECT \"a\".\"id\" FROM \"jadpo_operator_actions\" AS \"a\" WHERE \"a\".\"kind\" = 'request' AND \"a\".\"acted_at\" <= $1 AND NOT EXISTS (SELECT 1 FROM \"jadpo_step_records\" AS \"r\" WHERE \"r\".\"id\" = \"a\".\"item\") LIMIT $2) RETURNING 1";

/// One step as an operator sees it: never its request or its answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
    /// From 1.
    pub position: u32,
    /// `pay.payments.create`, `sheets.put`.
    pub operation: String,
    /// How many times its key was replaced: by the provider's
    /// `new_key_on:`, or an operator's "not done".
    pub key_generation: u32,
    /// When it was last sent, in milliseconds since the epoch.
    pub sent_at: i64,
    /// `answered` or `failed`; `None` while in doubt.
    pub outcome: Option<String>,
    /// Whether its answer is still kept: dropped after 30 days.
    pub kept: bool,
    pub answered_at: Option<i64>,
}

impl Step {
    /// Answered once, its answer since dropped.
    pub fn dropped(&self) -> bool {
        self.outcome.is_some() && !self.kept
    }
}

/// A record as an operator sees it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    pub id: [u8; 16],
    /// The delivery whose record it is; `None` for a route's.
    pub delivery: Option<[u8; 16]>,
    /// The identity key of the action that is run.
    pub action: String,
    /// The build it started under and a digest of its step shape.
    pub build: String,
    pub shape: String,
    pub started_at: i64,
    /// `running`, `diverged` or `abandoned`.
    pub state: String,
    /// What differed, when it diverged: the step, its operation and the
    /// fields, never a value.
    pub problem: Option<String>,
    /// When its last run, or the operator who abandoned it, took it.
    pub taken_at: i64,
    pub steps: Vec<Step>,
}

impl Record {
    /// The step "not done" sends again: its last, while it has no answer
    /// or failed.
    pub fn in_doubt(&self) -> Option<&Step> {
        self.steps
            .last()
            .filter(|step| step.outcome.as_deref() != Some("answered"))
    }

    /// Whether a step's answer was dropped after 30 days, so a rerun
    /// can't replay it.
    pub fn dropped(&self) -> bool {
        self.steps.iter().any(Step::dropped)
    }

    /// What differed, said as a delivery's record says it.
    pub fn problem_text(&self) -> Option<String> {
        crate::steps::problem_text(self.problem.as_deref()?, &self.action)
    }
}

fn millis(value: Option<&SqlValue>) -> Option<i64> {
    match value {
        Some(SqlValue::Timestamp(micros)) => Some(micros.div_euclid(1000)),
        _ => None,
    }
}

fn text(value: Option<&SqlValue>) -> Option<String> {
    match value {
        Some(SqlValue::Text(text)) => Some(text.clone()),
        _ => None,
    }
}

/// A delivery's id from its record's key, 32 hex digits.
fn delivery_id(key: &str) -> Option<[u8; 16]> {
    let bytes = jadpo_value::encoding::from_hex(key)?;
    bytes.try_into().ok()
}

async fn read(
    database: &dyn Database,
    sql: &str,
    params: &[SqlValue],
) -> Result<Vec<Record>, StoreError> {
    let rows = database.query_once(sql, params).await?;
    let mut out = Vec::new();
    for row in rows {
        let Some(SqlValue::Uuid(id)) = row.first() else {
            continue;
        };
        let delivery = match (row.get(1), text(row.get(2))) {
            (Some(SqlValue::Bool(true)), Some(key)) => delivery_id(&key),
            _ => None,
        };
        let (Some(action), Some(state)) = (text(row.get(3)), text(row.get(7))) else {
            continue;
        };
        let steps = database
            .query_once(STEPS, &[SqlValue::Uuid(*id)])
            .await?
            .iter()
            .filter_map(|step| {
                let position = match step.first() {
                    Some(SqlValue::Int4(n)) => u32::try_from(*n).ok()?,
                    _ => return None,
                };
                Some(Step {
                    position,
                    operation: text(step.get(1))?,
                    key_generation: match step.get(2) {
                        Some(SqlValue::Int4(n)) => u32::try_from(*n).unwrap_or(0),
                        _ => 0,
                    },
                    sent_at: millis(step.get(3))?,
                    outcome: text(step.get(4)),
                    kept: matches!(step.get(5), Some(SqlValue::Bool(true))),
                    answered_at: millis(step.get(6)),
                })
            })
            .collect();
        out.push(Record {
            id: *id,
            delivery,
            action,
            build: text(row.get(4)).unwrap_or_default(),
            shape: text(row.get(5)).unwrap_or_default(),
            started_at: millis(row.get(6)).unwrap_or(0),
            state,
            problem: text(row.get(8)),
            taken_at: millis(row.get(9)).unwrap_or(0),
            steps,
        });
    }
    Ok(out)
}

/// The record of delivery `delivery` in one store, when it has one there.
pub async fn of_delivery(
    database: &dyn Database,
    delivery: &[u8; 16],
) -> Result<Option<Record>, StoreError> {
    let key = SqlValue::Text(jadpo_value::encoding::to_hex(delivery));
    Ok(read(database, &of_delivery_sql(), &[key])
        .await?
        .into_iter()
        .next())
}

/// The record whose id is `id` in one store.
pub async fn by_id(database: &dyn Database, id: &[u8; 16]) -> Result<Option<Record>, StoreError> {
    Ok(read(database, &by_id_sql(), &[SqlValue::Uuid(*id)])
        .await?
        .into_iter()
        .next())
}

/// Every record of one store not abandoned, oldest first: what `jadpo runs
/// unfinished` compares with a new build.
pub async fn unfinished(database: &dyn Database) -> Result<Vec<Record>, StoreError> {
    read(database, &unfinished_sql(), &[]).await
}

/// What an operator's action does to a delivery's record, beside its
/// delivery.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    /// Ready for the delivery's retry; with a step, that step sent again
    /// under its next key ("not done").
    Retry { record: [u8; 16], not_done: Option<u32> },
    /// Abandoned with its delivery.
    Abandon { record: [u8; 16] },
}

impl Change {
    pub fn record(self) -> [u8; 16] {
        match self {
            Change::Retry { record, .. } | Change::Abandon { record } => record,
        }
    }
}

/// Does `change` to a record in a store other than its delivery's, at
/// `now`, before the delivery is changed. Whether the record was there to
/// change.
pub async fn change_alone(
    database: &dyn Database,
    change: Change,
    now: i64,
) -> Result<bool, StoreError> {
    let rows = match change {
        Change::Retry { record, not_done } => {
            database
                .query_once(
                    RETRY,
                    &[
                        SqlValue::Uuid(record),
                        micros(now),
                        position(not_done),
                    ],
                )
                .await?
        }
        Change::Abandon { record } => {
            database
                .query_once(ABANDON, &[SqlValue::Uuid(record), micros(now)])
                .await?
        }
    };
    Ok(!rows.is_empty())
}

/// A step's position as a statement's parameter, or none.
pub(crate) fn position(at: Option<u32>) -> SqlValue {
    match at {
        Some(at) => SqlValue::Int4(i32::try_from(at).unwrap_or(i32::MAX)),
        None => SqlValue::Null(crate::SqlType::Int4),
    }
}

/// Abandons a route's record `item` (its id and when it was last taken,
/// as read), which was `was`, at `now` as `operator`, logging it.
pub async fn abandon_request(
    database: &dyn Database,
    host: &dyn crate::Host,
    item: &crate::deliveries::Item,
    operator: &str,
    now: i64,
) -> Result<crate::deliveries::Done, StoreError> {
    let log = crate::ctx::random_id(host, now)
        .ok_or_else(|| StoreError::Fault("no random bytes for a log row's id".into()))?;
    let rows = database
        .query_once(
            ABANDON_REQUEST,
            &[
                SqlValue::Uuid(item.id),
                micros(now),
                SqlValue::Uuid(log),
                SqlValue::Text(operator.to_string()),
                item.last_tried
                    .map_or(SqlValue::Null(crate::SqlType::Timestamp), micros),
                SqlValue::Text(item.state.clone()),
            ],
        )
        .await?;
    Ok(match rows.first().and_then(|row| row.first()) {
        Some(SqlValue::Text(role)) => crate::deliveries::Done::Logged { role: role.clone() },
        _ => crate::deliveries::Done::Changed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(outcome: Option<&str>, kept: bool) -> Step {
        Step {
            position: 1,
            operation: "pay.customers.create".into(),
            key_generation: 0,
            sent_at: 0,
            outcome: outcome.map(str::to_string),
            kept,
            answered_at: None,
        }
    }

    fn record(steps: Vec<Step>) -> Record {
        Record {
            id: [1; 16],
            delivery: None,
            action: "action subscribe".into(),
            build: String::new(),
            shape: String::new(),
            started_at: 0,
            state: "running".into(),
            problem: None,
            taken_at: 0,
            steps,
        }
    }

    #[test]
    fn the_step_in_doubt_is_the_last_unless_it_was_answered() {
        let mut steps = vec![step(Some("answered"), true), step(None, false)];
        steps[1].position = 2;
        assert_eq!(record(steps.clone()).in_doubt().map(|s| s.position), Some(2));
        steps[1].outcome = Some("failed".into());
        assert_eq!(record(steps.clone()).in_doubt().map(|s| s.position), Some(2));
        steps[1].outcome = Some("answered".into());
        assert_eq!(record(steps).in_doubt(), None);
    }

    #[test]
    fn an_answer_dropped_after_thirty_days_is_told_from_one_never_given() {
        assert!(record(vec![step(Some("answered"), false)]).dropped());
        assert!(!record(vec![step(None, false)]).dropped());
        assert!(!record(vec![step(Some("failed"), true)]).dropped());
    }

    #[test]
    fn a_deliverys_record_key_reads_back_as_its_id() {
        let id = [0xab; 16];
        assert_eq!(delivery_id(&jadpo_value::encoding::to_hex(&id)), Some(id));
        assert_eq!(delivery_id("k-ada-gold"), None);
    }
}
