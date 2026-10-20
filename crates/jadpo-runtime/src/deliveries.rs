//! What an operator sees and does with `jadpo deliveries` (area 12, "What
//! an operator sees"; area 14, question 13; chunk 6.12): each delivery of
//! an event to a subscriber, and each schedule's run, as an item with a
//! state, and the actions that change one, each logged with who did it.
//!
//! - **Deliveries** are their records in `jadpo_deliveries`, in every
//!   store that holds an entity. An operator retries a `failed` one
//!   (`pending` again, due now, with a fresh budget: its attempts counted
//!   from none and its `within:` from the retry, `retried_at`), dismisses
//!   it (`dismissed`), cancels a `pending` one (`cancelled`), and resolves
//!   one whose outcome is unknown as done (`succeeded`) or not done
//!   (retried as above). Each is one statement that changes the record
//!   only from the state it was read in and writes its log row, so two
//!   operators never both act on it, and nothing is changed unlogged.
//! - **Schedule runs** are each schedule's latest run, in `schedules: {
//!   store: }`: one that failed, or whose outcome is unknown, is an item
//!   until a later run ends it or an operator settles it. Dismissing one,
//!   or resolving it as done, is its log row; retrying one, or resolving
//!   it as not done, asks the application's tick to run the schedule
//!   ([`super::schedules::watch::request_run`]), and a run asked for that
//!   way is `pending` until a tick takes it, which cancelling withdraws.
//! - **Parked queue messages** (chunk 7.7c) are their rows in
//!   `jadpo_parked_messages`, in the store a queue's events are kept in:
//!   `failed` until an operator retries one (`pending`, read again by the
//!   app's next tick) or dismisses it (`dismissed`).
//! - **The log** (`jadpo_operator_actions`) is in the store the item is
//!   in: when, what, the item and its state then, the name the operator's
//!   machine gives, and the database role PostgreSQL authenticated, which
//!   the statement reads itself (`session_user`), so it can't be claimed.
//! - **What is kept.** A delivery dismissed or cancelled, and each log row,
//!   is kept 30 days after the operator acted, as run records are; the
//!   tick then deletes it, a log row once its delivery or run is gone
//!   ([`super::schedules`]; chunk 6.13).
//!
//! None of this runs the application's code: the tick's sweep delivers a
//! retried delivery, and its tick runs a schedule.

use std::collections::BTreeMap;

use crate::outbox::micros;
use crate::{Database, Host, SqlType, SqlValue, StoreError};

/// An item's states (area 12's table, and what an operator's action
/// leaves).
pub const PENDING: &str = "pending";
pub const RUNNING: &str = "running";
pub const SUCCEEDED: &str = "succeeded";
pub const FAILED: &str = "failed";
pub const OUTCOME_UNKNOWN: &str = "outcome_unknown";
pub const DISMISSED: &str = "dismissed";
pub const CANCELLED: &str = "cancelled";
/// A schedule's run that isn't its latest, or a skipped tick: nothing
/// to act on.
pub const SUPERSEDED: &str = "superseded";
/// A schedule's run an operator resolved as done.
pub const RESOLVED: &str = "resolved";

/// A delivery a circuit breaker holds while a provider it calls is
/// unavailable (chunk 7.2b).
pub const HELD: &str = crate::breaker::HELD;

/// A delivery whose unkeyed call is on its way out (chunk 7.0c): shown
/// as `running`, as a worker has it.
pub const SENDING: &str = crate::outbox::SENDING;

/// A record's state as an operator is shown it.
fn shown_state(state: &str) -> String {
    if state == SENDING { RUNNING } else { state }.to_string()
}

/// The states an operator is shown by default: those that need one.
pub const NEEDS_AN_OPERATOR: [&str; 2] = [FAILED, OUTCOME_UNKNOWN];

/// The columns every item is read with: its id, kind, target (the
/// subscriber's or the schedule's identity key), event, state, attempts,
/// when it was last tried (a run's start), its next attempt, its
/// failure's code and kind, its last operator action (when, which, by
/// whom and as which role), and the time it is ordered by; then a
/// delivery's connection and time held, while a circuit breaker holds it
/// (chunk 7.2b).
const DELIVERY_COLUMNS: &str = "\"d\".\"id\", 'delivery', \"d\".\"subscriber\", \"o\".\"event\", \"d\".\"status\", \"d\".\"attempts\", \"d\".\"last_tried_at\", \"d\".\"next_attempt_at\", \"d\".\"last_failure\" ->> 'code', \"d\".\"last_failure\" ->> 'kind', \"a\".\"acted_at\", \"a\".\"action\", \"a\".\"operator\", \"a\".\"role\", COALESCE(\"d\".\"last_tried_at\", \"o\".\"emitted_at\"), \"d\".\"held_for\", \"d\".\"held_since\"";

fn delivery_from() -> &'static str {
    "FROM \"jadpo_deliveries\" AS \"d\" JOIN \"jadpo_outbox\" AS \"o\" ON \"o\".\"id\" = \"d\".\"outbox_id\" LEFT JOIN LATERAL (SELECT \"acted_at\", \"action\", \"operator\", \"role\" FROM \"jadpo_operator_actions\" WHERE \"item\" = \"d\".\"id\" ORDER BY \"acted_at\" DESC, \"id\" DESC LIMIT 1) AS \"a\" ON TRUE"
}

/// A schedule run's state: dismissed or resolved by an operator, else
/// its outcome.
const RUN_COLUMNS: &str = "\"r\".\"id\", 'schedule_run', \"r\".\"schedule\", NULL::text, CASE WHEN \"a\".\"action\" = 'dismiss' THEN 'dismissed' WHEN \"a\".\"action\" = 'resolve_done' THEN 'resolved' ELSE \"r\".\"outcome\" END, NULL::int4, \"r\".\"started_at\", NULL::timestamptz, \"r\".\"failure\" ->> 'code', \"r\".\"failure\" ->> 'kind', \"a\".\"acted_at\", \"a\".\"action\", \"a\".\"operator\", \"a\".\"role\", \"r\".\"started_at\"";

/// The last operator action on run `"r"`.
fn run_action() -> &'static str {
    "LEFT JOIN LATERAL (SELECT \"acted_at\", \"action\", \"operator\", \"role\" FROM \"jadpo_operator_actions\" WHERE \"item\" = \"r\".\"id\" ORDER BY \"acted_at\" DESC, \"id\" DESC LIMIT 1) AS \"a\" ON TRUE"
}

/// The deliveries in states `$1`, most recently tried first, at most
/// `$2`.
pub fn deliveries_sql() -> String {
    format!(
        "SELECT {DELIVERY_COLUMNS} {} WHERE \"d\".\"status\" = ANY($1::text[]) ORDER BY 15 DESC, \"d\".\"id\" DESC LIMIT $2",
        delivery_from()
    )
}

/// The deliveries whose id, as 32 hex digits, ends with `$1`.
pub fn find_delivery_sql() -> String {
    format!(
        "SELECT {DELIVERY_COLUMNS} {} WHERE right(replace(\"d\".\"id\"::text, '-', ''), length($1::text)) = $1::text",
        delivery_from()
    )
}

/// How many deliveries are in each state.
pub const DELIVERY_COUNTS: &str =
    "SELECT \"status\", count(*) FROM \"jadpo_deliveries\" GROUP BY \"status\"";

/// Each schedule's latest run, a skipped tick aside, in states `$1`,
/// unless an operator dismissed it or resolved it as done: most recent
/// first, at most `$2`.
pub fn runs_sql() -> String {
    format!(
        "SELECT {RUN_COLUMNS} FROM (SELECT DISTINCT ON (\"schedule\") * FROM \"jadpo_schedule_runs\" WHERE \"outcome\" <> 'skipped' ORDER BY \"schedule\", \"started_at\" DESC, \"id\" DESC) AS \"r\" {} WHERE \"r\".\"outcome\" = ANY($1::text[]) AND \"a\".\"action\" IS DISTINCT FROM 'dismiss' AND \"a\".\"action\" IS DISTINCT FROM 'resolve_done' ORDER BY 15 DESC, \"r\".\"id\" DESC LIMIT $2",
        run_action()
    )
}

/// The runs an operator dismissed, most recent first, at most `$1`.
pub fn dismissed_runs_sql() -> String {
    format!(
        "SELECT {RUN_COLUMNS} FROM \"jadpo_schedule_runs\" AS \"r\" {} WHERE \"a\".\"action\" = 'dismiss' ORDER BY 15 DESC, \"r\".\"id\" DESC LIMIT $1",
        run_action()
    )
}

/// The runs asked for by hand that an operator cancelled before a tick
/// took them, which only the log keeps: most recent first, at most `$1`.
pub const CANCELLED_RUNS: &str = "SELECT \"item\", 'schedule_run', \"target\", NULL::text, 'cancelled', NULL::int4, NULL::timestamptz, NULL::timestamptz, NULL::text, NULL::text, \"acted_at\", \"action\", \"operator\", \"role\", \"acted_at\" FROM \"jadpo_operator_actions\" WHERE \"kind\" = 'schedule_run' AND \"action\" = 'cancel' ORDER BY \"acted_at\" DESC, \"id\" DESC LIMIT $1";

/// The runs asked for by hand that no tick has taken yet (chunk 6.2d).
pub const REQUESTED_RUNS: &str = "SELECT \"run_requested\", 'schedule_run', \"schedule\", NULL::text, 'pending', NULL::int4, NULL::timestamptz, NULL::timestamptz, NULL::text, NULL::text, NULL::timestamptz, NULL::text, NULL::text, NULL::text, NULL::timestamptz FROM \"jadpo_schedules\" WHERE \"run_requested\" IS NOT NULL ORDER BY \"schedule\"";

/// The run records whose id, as 32 hex digits, ends with `$1`, each with
/// whether it is its schedule's latest run, a skipped tick aside.
pub fn find_run_sql() -> String {
    format!(
        "SELECT {RUN_COLUMNS}, \"r\".\"outcome\" <> 'skipped' AND \"r\".\"id\" = (SELECT \"l\".\"id\" FROM \"jadpo_schedule_runs\" AS \"l\" WHERE \"l\".\"schedule\" = \"r\".\"schedule\" AND \"l\".\"outcome\" <> 'skipped' ORDER BY \"l\".\"started_at\" DESC, \"l\".\"id\" DESC LIMIT 1) FROM \"jadpo_schedule_runs\" AS \"r\" {} WHERE right(replace(\"r\".\"id\"::text, '-', ''), length($1::text)) = $1::text",
        run_action()
    )
}

/// The runs asked for by hand, not yet taken, whose id ends with `$1`.
pub const FIND_REQUESTED: &str = "SELECT \"run_requested\", 'schedule_run', \"schedule\", NULL::text, 'pending', NULL::int4, NULL::timestamptz, NULL::timestamptz, NULL::text, NULL::text, NULL::timestamptz, NULL::text, NULL::text, NULL::text, NULL::timestamptz FROM \"jadpo_schedules\" WHERE right(replace(\"run_requested\"::text, '-', ''), length($1::text)) = $1::text";

/// Moves delivery `$1` from state `$5` to `$6` at `$2`, due at `$8`, a
/// fresh budget when `$9` (its attempts from none and its `within:` from
/// now), and logs it as action `$7` with id `$3` by `$4`: one statement,
/// so the change and its log row commit together, and only from the
/// state it was read in. The role it was logged as.
pub const ACT_DELIVERY: &str = "WITH \"d\" AS (UPDATE \"jadpo_deliveries\" SET \"status\" = $6, \"next_attempt_at\" = $8, \"attempts\" = CASE WHEN $9 THEN 0 ELSE \"attempts\" END, \"retried_at\" = CASE WHEN $9 THEN $2 ELSE \"retried_at\" END, \"held_ms\" = CASE WHEN $9 THEN NULL ELSE \"held_ms\" END WHERE \"id\" = $1 AND \"status\" = $5 RETURNING \"id\", \"subscriber\", \"outbox_id\") INSERT INTO \"jadpo_operator_actions\" (\"id\", \"acted_at\", \"action\", \"kind\", \"item\", \"target\", \"event\", \"was\", \"operator\", \"role\") SELECT $3, $2, $7, 'delivery', \"d\".\"id\", \"d\".\"subscriber\", \"o\".\"event\", $5, $4, session_user FROM \"d\" JOIN \"jadpo_outbox\" AS \"o\" ON \"o\".\"id\" = \"d\".\"outbox_id\" RETURNING \"role\"";

/// Logs run `$1`, while its outcome is `$6` and no operator has settled
/// it, as action `$5` at `$2` with id `$3` by `$4`: a dismissal, or a
/// resolution as done. The role it was logged as.
pub const SETTLE_RUN: &str = "INSERT INTO \"jadpo_operator_actions\" (\"id\", \"acted_at\", \"action\", \"kind\", \"item\", \"target\", \"event\", \"was\", \"operator\", \"role\") SELECT $3, $2, $5, 'schedule_run', \"r\".\"id\", \"r\".\"schedule\", NULL, \"r\".\"outcome\", $4, session_user FROM \"jadpo_schedule_runs\" AS \"r\" WHERE \"r\".\"id\" = $1 AND \"r\".\"outcome\" = $6 AND NOT EXISTS (SELECT 1 FROM \"jadpo_operator_actions\" AS \"a\" WHERE \"a\".\"item\" = \"r\".\"id\" AND \"a\".\"action\" IN ('dismiss', 'resolve_done')) RETURNING \"role\"";

/// Withdraws the run asked for by hand with id `$1`, unless a tick has
/// taken it, and logs it as cancelled at `$2` with id `$3` by `$4`. The
/// role it was logged as.
pub const CANCEL_REQUEST: &str = "WITH \"s\" AS (UPDATE \"jadpo_schedules\" SET \"run_requested\" = NULL WHERE \"run_requested\" = $1 RETURNING \"schedule\") INSERT INTO \"jadpo_operator_actions\" (\"id\", \"acted_at\", \"action\", \"kind\", \"item\", \"target\", \"event\", \"was\", \"operator\", \"role\") SELECT $3, $2, 'cancel', 'schedule_run', $1, \"s\".\"schedule\", NULL, 'pending', $4, session_user FROM \"s\" RETURNING \"role\"";

/// Logs action `$3` on schedule `$5`'s run `$4`, which was `$6`, at `$2`
/// with id `$1` by `$7`: a retry, or a resolution as not done, once the
/// tick has been asked for the run. The role it was logged as.
pub const LOG_RUN: &str = "INSERT INTO \"jadpo_operator_actions\" (\"id\", \"acted_at\", \"action\", \"kind\", \"item\", \"target\", \"event\", \"was\", \"operator\", \"role\") VALUES ($1, $2, $3, 'schedule_run', $4, $5, NULL, $6, $7, session_user) RETURNING \"role\"";

/// A parked queue message's columns (chunk 7.7c): its queue as its
/// target, the event it was read as when it got that far, its state, when
/// it was parked, and why as its code.
const MESSAGE_COLUMNS: &str = "\"m\".\"id\", 'message', \"m\".\"queue\", \"m\".\"event\", \"m\".\"status\", NULL::int4, \"m\".\"parked_at\", NULL::timestamptz, \"m\".\"reason\", NULL::text, \"a\".\"acted_at\", \"a\".\"action\", \"a\".\"operator\", \"a\".\"role\", COALESCE(\"m\".\"retried_at\", \"m\".\"parked_at\")";

fn message_from() -> &'static str {
    "FROM \"jadpo_parked_messages\" AS \"m\" LEFT JOIN LATERAL (SELECT \"acted_at\", \"action\", \"operator\", \"role\" FROM \"jadpo_operator_actions\" WHERE \"item\" = \"m\".\"id\" ORDER BY \"acted_at\" DESC, \"id\" DESC LIMIT 1) AS \"a\" ON TRUE"
}

/// The parked messages in states `$1`, most recent first, at most `$2`.
pub fn messages_sql() -> String {
    format!(
        "SELECT {MESSAGE_COLUMNS} {} WHERE \"m\".\"status\" = ANY($1::text[]) ORDER BY 15 DESC, \"m\".\"id\" DESC LIMIT $2",
        message_from()
    )
}

/// The parked messages whose id, as 32 hex digits, ends with `$1`.
pub fn find_message_sql() -> String {
    format!(
        "SELECT {MESSAGE_COLUMNS} {} WHERE right(replace(\"m\".\"id\"::text, '-', ''), length($1::text)) = $1::text",
        message_from()
    )
}

/// How many parked messages are in each state.
pub const MESSAGE_COUNTS: &str =
    "SELECT \"status\", count(*) FROM \"jadpo_parked_messages\" GROUP BY \"status\"";

/// Moves parked message `$1` from state `$5` to `$6` at `$2`, and logs it
/// as action `$7` with id `$3` by `$4`: a retry, `pending` for the app's
/// next tick, or a dismissal. The role it was logged as.
pub const ACT_MESSAGE: &str = "WITH \"m\" AS (UPDATE \"jadpo_parked_messages\" SET \"status\" = $6, \"retried_at\" = CASE WHEN $6 = 'pending' THEN $2 ELSE \"retried_at\" END WHERE \"id\" = $1 AND \"status\" = $5 RETURNING \"id\", \"queue\", \"event\") INSERT INTO \"jadpo_operator_actions\" (\"id\", \"acted_at\", \"action\", \"kind\", \"item\", \"target\", \"event\", \"was\", \"operator\", \"role\") SELECT $3, $2, $7, 'message', \"m\".\"id\", \"m\".\"queue\", \"m\".\"event\", $5, $4, session_user FROM \"m\" RETURNING \"role\"";

/// What an item is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    Delivery,
    ScheduleRun,
    /// A queue message parked for an operator (chunk 7.7c).
    Message,
}

impl Kind {
    pub fn word(self) -> &'static str {
        match self {
            Kind::Delivery => "delivery",
            Kind::ScheduleRun => "schedule_run",
            Kind::Message => "message",
        }
    }
}

/// The last thing an operator did to an item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Acted {
    pub at: i64,
    pub action: String,
    pub operator: String,
    pub role: String,
}

/// A delivery or a schedule's run as an operator sees it, times in
/// milliseconds since the epoch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    pub id: [u8; 16],
    pub kind: Kind,
    /// The subscriber's identity key, or the schedule's.
    pub target: String,
    /// A delivery's event type's identity key.
    pub event: Option<String>,
    pub state: String,
    /// A delivery's attempts.
    pub attempts: Option<i32>,
    /// When a delivery was last tried, or a run started.
    pub last_tried: Option<i64>,
    /// A pending delivery's next attempt.
    pub next_attempt: Option<i64>,
    pub code: Option<String>,
    pub failure_kind: Option<String>,
    pub acted: Option<Acted>,
    /// The time it is ordered by, most recent first.
    pub at: Option<i64>,
    /// Whether a run is its schedule's latest; always true of a delivery.
    pub latest: bool,
    /// A held delivery's connection, whose circuit breaker holds it, and
    /// since when (chunk 7.2b).
    pub held_for: Option<String>,
    pub held_since: Option<i64>,
}

/// The states [`items`] lists.
pub fn listable(state: &str) -> bool {
    [
        PENDING,
        RUNNING,
        HELD,
        FAILED,
        OUTCOME_UNKNOWN,
        DISMISSED,
        CANCELLED,
    ]
    .contains(&state)
}

/// What a store holds: deliveries, when it holds an entity, and
/// schedules' runs, when it is `schedules: { store: }`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Holds {
    pub deliveries: bool,
    pub runs: bool,
    /// Parked queue messages, when it is the store queues' events are
    /// kept in (chunk 7.7c).
    pub messages: bool,
}

/// The items in `states` in one store, most recent first, at most
/// `limit` of each kind.
pub async fn items(
    database: &dyn Database,
    holds: Holds,
    states: &[&str],
    limit: i64,
) -> Result<Vec<Item>, StoreError> {
    let mut out = Vec::new();
    // A delivery `sending` its unkeyed call (chunk 7.0c) is running.
    let wanted = SqlValue::Array(
        SqlType::Text,
        states
            .iter()
            .flat_map(|s| {
                if *s == RUNNING {
                    vec![RUNNING, SENDING]
                } else {
                    vec![*s]
                }
            })
            .map(|s| SqlValue::Text(s.to_string()))
            .collect(),
    );
    if holds.deliveries {
        let rows = database
            .query_once(&deliveries_sql(), &[wanted.clone(), SqlValue::Int8(limit)])
            .await?;
        out.extend(rows.iter().filter_map(|row| read_item(row)));
    }
    if holds.messages {
        let parked: Vec<SqlValue> = states
            .iter()
            .filter(|s| [FAILED, PENDING, DISMISSED].contains(s))
            .map(|s| SqlValue::Text(s.to_string()))
            .collect();
        if !parked.is_empty() {
            let rows = database
                .query_once(
                    &messages_sql(),
                    &[
                        SqlValue::Array(SqlType::Text, parked),
                        SqlValue::Int8(limit),
                    ],
                )
                .await?;
            out.extend(rows.iter().filter_map(|row| read_item(row)));
        }
    }
    if holds.runs {
        let by_outcome: Vec<SqlValue> = states
            .iter()
            .filter(|s| [FAILED, OUTCOME_UNKNOWN, RUNNING].contains(s))
            .map(|s| SqlValue::Text(s.to_string()))
            .collect();
        if !by_outcome.is_empty() {
            let rows = database
                .query_once(
                    &runs_sql(),
                    &[
                        SqlValue::Array(SqlType::Text, by_outcome),
                        SqlValue::Int8(limit),
                    ],
                )
                .await?;
            out.extend(rows.iter().filter_map(|row| read_item(row)));
        }
        if states.contains(&DISMISSED) {
            let rows = database
                .query_once(&dismissed_runs_sql(), &[SqlValue::Int8(limit)])
                .await?;
            out.extend(rows.iter().filter_map(|row| read_item(row)));
        }
        if states.contains(&CANCELLED) {
            let rows = database
                .query_once(CANCELLED_RUNS, &[SqlValue::Int8(limit)])
                .await?;
            out.extend(rows.iter().filter_map(|row| read_item(row)));
        }
        if states.contains(&PENDING) {
            let rows = database.query_once(REQUESTED_RUNS, &[]).await?;
            out.extend(rows.iter().filter_map(|row| read_item(row)));
        }
    }
    sort(&mut out);
    Ok(out)
}

/// Most recent first; a run asked for by hand, which has no time yet,
/// first of all.
pub fn sort(items: &mut [Item]) {
    items.sort_by(|a, b| b.at.cmp(&a.at).then(b.id.cmp(&a.id)));
    items.sort_by_key(|item| item.at.is_some());
}

/// How many of one store's deliveries are in each state; with its
/// schedules' runs, how many schedules' latest runs need an operator or
/// run, and how many runs asked for by hand wait.
pub async fn counts(
    database: &dyn Database,
    holds: Holds,
) -> Result<BTreeMap<String, u64>, StoreError> {
    let mut out = BTreeMap::new();
    if holds.deliveries {
        for row in database.query_once(DELIVERY_COUNTS, &[]).await? {
            if let (Some(SqlValue::Text(state)), Some(SqlValue::Int8(n))) =
                (row.first(), row.get(1))
            {
                *out.entry(shown_state(state)).or_default() += u64::try_from(*n).unwrap_or(0);
            }
        }
    }
    if holds.messages {
        for row in database.query_once(MESSAGE_COUNTS, &[]).await? {
            if let (Some(SqlValue::Text(state)), Some(SqlValue::Int8(n))) =
                (row.first(), row.get(1))
            {
                *out.entry(state.clone()).or_default() += u64::try_from(*n).unwrap_or(0);
            }
        }
    }
    if holds.runs {
        for item in items(
            database,
            Holds {
                deliveries: false,
                runs: true,
                messages: false,
            },
            &[FAILED, OUTCOME_UNKNOWN, RUNNING, PENDING],
            10_000,
        )
        .await?
        {
            *out.entry(item.state).or_default() += 1;
        }
    }
    Ok(out)
}

/// The items in one store whose id, as 32 hex digits, ends with
/// `suffix`.
pub async fn find(
    database: &dyn Database,
    holds: Holds,
    suffix: &str,
) -> Result<Vec<Item>, StoreError> {
    let suffix = SqlValue::Text(suffix.to_string());
    let mut out = Vec::new();
    if holds.deliveries {
        let rows = database
            .query_once(&find_delivery_sql(), std::slice::from_ref(&suffix))
            .await?;
        out.extend(rows.iter().filter_map(|row| read_item(row)));
    }
    if holds.messages {
        let rows = database
            .query_once(&find_message_sql(), std::slice::from_ref(&suffix))
            .await?;
        out.extend(rows.iter().filter_map(|row| read_item(row)));
    }
    if holds.runs {
        let rows = database
            .query_once(&find_run_sql(), std::slice::from_ref(&suffix))
            .await?;
        out.extend(rows.iter().filter_map(|row| {
            let mut item = read_item(row)?;
            item.latest = matches!(row.get(15), Some(SqlValue::Bool(true)));
            if !item.latest && item.state != RUNNING {
                item.state = SUPERSEDED.to_string();
            }
            Some(item)
        }));
        let rows = database
            .query_once(FIND_REQUESTED, std::slice::from_ref(&suffix))
            .await?;
        out.extend(rows.iter().filter_map(|row| read_item(row)));
    }
    Ok(out)
}

/// What an operator does to an item.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Retry,
    Dismiss,
    Cancel,
    ResolveDone,
    ResolveNotDone,
}

impl Action {
    /// Its name in the log.
    pub fn word(self) -> &'static str {
        match self {
            Action::Retry => "retry",
            Action::Dismiss => "dismiss",
            Action::Cancel => "cancel",
            Action::ResolveDone => "resolve_done",
            Action::ResolveNotDone => "resolve_not_done",
        }
    }

    /// The state it acts on.
    pub fn from_state(self) -> &'static str {
        match self {
            Action::Retry | Action::Dismiss => FAILED,
            Action::Cancel => PENDING,
            Action::ResolveDone | Action::ResolveNotDone => OUTCOME_UNKNOWN,
        }
    }

    /// Whether it may act on `item` as it was read: in its state, and,
    /// for a schedule's run, its latest. A parked message is only retried
    /// or dismissed.
    pub fn fits(self, item: &Item) -> bool {
        item.state == self.from_state()
            && item.latest
            && (item.kind != Kind::Message || matches!(self, Action::Retry | Action::Dismiss))
    }

    /// Whether it asks the application's tick for a schedule's run.
    pub fn asks_for_a_run(self) -> bool {
        matches!(self, Action::Retry | Action::ResolveNotDone)
    }
}

/// How an action on an item went.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Done {
    /// Done, and logged as the database role it names.
    Logged { role: String },
    /// The item was no longer in the state it was read in: another
    /// operator, or the app, changed it first.
    Changed,
}

/// Does `action` to delivery `item` at `now` as `operator`, logging it,
/// in the state it was read in.
pub async fn act_on_delivery(
    database: &dyn Database,
    host: &dyn Host,
    item: &Item,
    action: Action,
    operator: &str,
    now: i64,
) -> Result<Done, StoreError> {
    let (to, due, fresh) = match action {
        Action::Retry | Action::ResolveNotDone => (PENDING, Some(now), true),
        Action::Dismiss => (DISMISSED, None, false),
        Action::Cancel => (CANCELLED, None, false),
        Action::ResolveDone => (SUCCEEDED, None, false),
    };
    let rows = database
        .query_once(
            ACT_DELIVERY,
            &[
                SqlValue::Uuid(item.id),
                micros(now),
                SqlValue::Uuid(log_id(host, now)?),
                SqlValue::Text(operator.to_string()),
                SqlValue::Text(action.from_state().to_string()),
                SqlValue::Text(to.to_string()),
                SqlValue::Text(action.word().to_string()),
                due.map_or(SqlValue::Null(SqlType::Timestamp), micros),
                SqlValue::Bool(fresh),
            ],
        )
        .await?;
    Ok(logged(&rows))
}

/// Retries or dismisses parked message `item` at `now` as `operator`,
/// logging it, in the state it was read in (chunk 7.7c).
pub async fn act_on_message(
    database: &dyn Database,
    host: &dyn Host,
    item: &Item,
    action: Action,
    operator: &str,
    now: i64,
) -> Result<Done, StoreError> {
    let to = match action {
        Action::Retry => PENDING,
        _ => DISMISSED,
    };
    let rows = database
        .query_once(
            ACT_MESSAGE,
            &[
                SqlValue::Uuid(item.id),
                micros(now),
                SqlValue::Uuid(log_id(host, now)?),
                SqlValue::Text(operator.to_string()),
                SqlValue::Text(action.from_state().to_string()),
                SqlValue::Text(to.to_string()),
                SqlValue::Text(action.word().to_string()),
            ],
        )
        .await?;
    Ok(logged(&rows))
}

/// Dismisses schedule run `item`, or resolves it as done, at `now` as
/// `operator`: its log row.
pub async fn settle_run(
    database: &dyn Database,
    host: &dyn Host,
    item: &Item,
    action: Action,
    operator: &str,
    now: i64,
) -> Result<Done, StoreError> {
    let rows = database
        .query_once(
            SETTLE_RUN,
            &[
                SqlValue::Uuid(item.id),
                micros(now),
                SqlValue::Uuid(log_id(host, now)?),
                SqlValue::Text(operator.to_string()),
                SqlValue::Text(action.word().to_string()),
                SqlValue::Text(action.from_state().to_string()),
            ],
        )
        .await?;
    Ok(logged(&rows))
}

/// Cancels the run asked for by hand, `item`, unless a tick has taken it,
/// at `now` as `operator`.
pub async fn cancel_request(
    database: &dyn Database,
    host: &dyn Host,
    item: &Item,
    operator: &str,
    now: i64,
) -> Result<Done, StoreError> {
    let rows = database
        .query_once(
            CANCEL_REQUEST,
            &[
                SqlValue::Uuid(item.id),
                micros(now),
                SqlValue::Uuid(log_id(host, now)?),
                SqlValue::Text(operator.to_string()),
            ],
        )
        .await?;
    Ok(logged(&rows))
}

/// Logs `action` on schedule run `item` at `now` as `operator`, once the
/// tick has been asked for a run.
pub async fn log_run(
    database: &dyn Database,
    host: &dyn Host,
    item: &Item,
    action: Action,
    operator: &str,
    now: i64,
) -> Result<Done, StoreError> {
    let rows = database
        .query_once(
            LOG_RUN,
            &[
                SqlValue::Uuid(log_id(host, now)?),
                micros(now),
                SqlValue::Text(action.word().to_string()),
                SqlValue::Uuid(item.id),
                SqlValue::Text(item.target.clone()),
                SqlValue::Text(item.state.clone()),
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

fn read_item(row: &[SqlValue]) -> Option<Item> {
    let text = |at: usize| match row.get(at) {
        Some(SqlValue::Text(text)) => Some(text.clone()),
        _ => None,
    };
    let millis = |at: usize| match row.get(at) {
        Some(SqlValue::Timestamp(micros)) => Some(micros.div_euclid(1000)),
        _ => None,
    };
    let id = match row.first() {
        Some(SqlValue::Uuid(id)) => *id,
        _ => return None,
    };
    let kind = match text(1)?.as_str() {
        "delivery" => Kind::Delivery,
        "message" => Kind::Message,
        _ => Kind::ScheduleRun,
    };
    let acted = match (millis(10), text(11), text(12), text(13)) {
        (Some(at), Some(action), Some(operator), Some(role)) => Some(Acted {
            at,
            action,
            operator,
            role,
        }),
        _ => None,
    };
    Some(Item {
        id,
        kind,
        target: text(2)?,
        event: text(3),
        state: shown_state(&text(4)?),
        attempts: match row.get(5) {
            Some(SqlValue::Int4(n)) => Some(*n),
            _ => None,
        },
        last_tried: millis(6),
        next_attempt: millis(7),
        code: text(8),
        failure_kind: text(9),
        acted,
        at: millis(14),
        latest: true,
        held_for: if kind == Kind::Delivery {
            text(15)
        } else {
            None
        },
        held_since: if kind == Kind::Delivery {
            millis(16)
        } else {
            None
        },
    })
}

/// An id as 32 lower-case hex digits.
pub fn hex(id: &[u8; 16]) -> String {
    id.iter().map(|b| format!("{b:02x}")).collect()
}

/// The id an operator is shown: its last 8 hex digits, which are random
/// in a version 7 UUID, where the first are its time.
pub fn short(id: &[u8; 16]) -> String {
    let full = hex(id);
    full.get(24..).unwrap_or_default().to_string()
}
