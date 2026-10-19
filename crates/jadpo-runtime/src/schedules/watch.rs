//! Watching schedules (area 12, "Watching schedules"; area 14; chunk
//! 6.2b): what `jadpo schedules`, `jadpo why` and `jadpo dev` read from a
//! schedule's state row (`jadpo_schedules`) and its run records
//! (`jadpo_schedule_runs`), and what an operator does to one: pause it,
//! resume it, run it now under its lease against `jadpo dev`, or, against
//! a deployment, ask the application's own tick to run it (chunk 6.2d).
//!
//! - **The alerts with no setup**: no successful run within twice the
//!   interval; ticks stopped arriving; a run past 80% of the 15-minute
//!   limit; three failures in a row; a lease held past its expiry. Each
//!   is a condition worked out from the rows at a given time, so any
//!   reader (a command, the tick, `jadpo dev`) sees the same ones. The
//!   tick raises each once ([`raise`]; chunk 6.9): which hold, and since
//!   when, is kept on the schedule's state row (`alerts`), changed by one
//!   instance's tick only, so several processes, or several Workers
//!   invocations, raise an alert once between its starting to hold and
//!   its stopping.
//! - **The metrics**: each run's lateness and duration ([`super::Due`]),
//!   and per schedule over the records kept ([`Metrics`]).

use std::collections::BTreeMap;

use jadpo_std::schedule::SchedulePlan;

use super::{BY_HAND, Claimed, ENSURE, Leased, RUNNING, SKIPPED, Take, run_claimed};
use crate::budget::{Budget, Invocation};
use crate::outbox::micros;
use crate::runner::{self, RUN_LIMIT_MS};
use crate::{Clock, Ctx, Database, Env, Executor, Host, Outcome, SqlType, SqlValue, StoreError};

/// Every schedule's state row.
pub const STATES: &str = "SELECT \"schedule\", \"every_ms\", \"next_run_at\", \"paused\", \"lease_until\", \"run_id\", \"last_tick_at\", \"last_succeeded_at\", \"run_requested\", \"alerts\" FROM \"jadpo_schedules\" ORDER BY \"schedule\"";

/// What a run record is read as: its id, schedule, due, start and end,
/// outcome, reason, failure code and kind, events, trace and the items it
/// counted.
const RUN_COLUMNS: &str = "\"id\", \"schedule\", \"due_slot\", \"started_at\", \"ended_at\", \"outcome\", \"reason\", \"failure\" ->> 'code', \"failure\" ->> 'kind', \"events\", \"traceparent\", \"items\"";

/// The latest run of each of schedules `$1` that wasn't a skipped tick.
pub fn latest_runs_sql() -> String {
    format!(
        "SELECT DISTINCT ON (\"schedule\") {RUN_COLUMNS} FROM \"jadpo_schedule_runs\" WHERE \"schedule\" = ANY($1::text[]) AND \"outcome\" <> 'skipped' ORDER BY \"schedule\", \"started_at\" DESC, \"id\" DESC"
    )
}

/// The run record with id `$1`.
pub fn run_sql() -> String {
    format!("SELECT {RUN_COLUMNS} FROM \"jadpo_schedule_runs\" WHERE \"id\" = $1")
}

/// Schedule `$1`'s records, newest first, `$2` of them after the first
/// `$3`.
pub fn history_sql() -> String {
    format!(
        "SELECT {RUN_COLUMNS} FROM \"jadpo_schedule_runs\" WHERE \"schedule\" = $1 ORDER BY \"started_at\" DESC, \"id\" DESC LIMIT $2 OFFSET $3"
    )
}

/// For each of schedules `$1` with records: its failed runs since its
/// last successful one, and the earliest due time it has a record of.
pub const STREAKS: &str = "SELECT \"r\".\"schedule\", count(*) FILTER (WHERE \"r\".\"outcome\" = 'failed' AND (\"s\".\"at\" IS NULL OR \"r\".\"started_at\" > \"s\".\"at\")), min(\"r\".\"due_slot\") FROM \"jadpo_schedule_runs\" AS \"r\" LEFT JOIN (SELECT \"schedule\", max(\"started_at\") AS \"at\" FROM \"jadpo_schedule_runs\" WHERE \"outcome\" = 'succeeded' GROUP BY \"schedule\") AS \"s\" ON \"s\".\"schedule\" = \"r\".\"schedule\" WHERE \"r\".\"schedule\" = ANY($1::text[]) GROUP BY \"r\".\"schedule\"";

/// Schedule `$1`'s metrics over the records kept: records, runs, failed
/// runs and skipped ticks; the median and longest duration of the runs
/// that ended, and the median and greatest lateness of every run, in
/// milliseconds.
pub const METRICS: &str = "SELECT count(*), count(*) FILTER (WHERE \"outcome\" <> 'skipped'), count(*) FILTER (WHERE \"outcome\" = 'failed'), count(*) FILTER (WHERE \"outcome\" = 'skipped'), (percentile_cont(0.5) WITHIN GROUP (ORDER BY extract(epoch FROM \"ended_at\" - \"started_at\") * 1000) FILTER (WHERE \"outcome\" <> 'skipped' AND \"ended_at\" IS NOT NULL))::int8, (max(extract(epoch FROM \"ended_at\" - \"started_at\") * 1000) FILTER (WHERE \"outcome\" <> 'skipped'))::int8, (percentile_cont(0.5) WITHIN GROUP (ORDER BY extract(epoch FROM \"started_at\" - \"due_slot\") * 1000) FILTER (WHERE \"outcome\" <> 'skipped'))::int8, (max(extract(epoch FROM \"started_at\" - \"due_slot\") * 1000) FILTER (WHERE \"outcome\" <> 'skipped'))::int8 FROM \"jadpo_schedule_runs\" WHERE \"schedule\" = $1";

/// Pauses (`$2` true) or resumes schedule `$1`, when it isn't so already;
/// its next due time.
pub const SET_PAUSED: &str = "UPDATE \"jadpo_schedules\" SET \"paused\" = $2 WHERE \"schedule\" = $1 AND \"paused\" <> $2 RETURNING \"next_run_at\"";

/// Takes schedule `$1`'s lease for run `$3` until `$2` at `$4`, whatever
/// its due time and whether it is paused, while no live lease holds it;
/// the run that held it before and that lease's end.
pub const CLAIM_NOW: &str = "WITH \"old\" AS (SELECT \"run_id\", \"lease_until\" FROM \"jadpo_schedules\" WHERE \"schedule\" = $1 FOR UPDATE) UPDATE \"jadpo_schedules\" AS \"s\" SET \"lease_until\" = $2, \"run_id\" = $3 FROM \"old\" WHERE \"s\".\"schedule\" = $1 AND (\"old\".\"lease_until\" IS NULL OR \"old\".\"lease_until\" <= $4) RETURNING \"old\".\"run_id\", \"old\".\"lease_until\"";

/// Asks schedule `$1`'s next tick for run `$2`, while no live lease holds
/// it at `$3`; a request already standing is kept, so both wait for the
/// same run. The run asked for.
pub const REQUEST: &str = "UPDATE \"jadpo_schedules\" SET \"run_requested\" = COALESCE(\"run_requested\", $2) WHERE \"schedule\" = $1 AND (\"run_id\" IS NULL OR \"lease_until\" IS NULL OR \"lease_until\" <= $3) RETURNING \"run_requested\"";

/// Withdraws run `$2` from schedule `$1`'s state row while no tick has
/// taken it.
pub const WITHDRAW: &str = "UPDATE \"jadpo_schedules\" SET \"run_requested\" = NULL WHERE \"schedule\" = $1 AND \"run_requested\" = $2 RETURNING \"schedule\"";

/// Records on schedule `$1`'s state row the alerts that hold for it, `$2`,
/// while what it records is still `$3`: of several instances' ticks, one
/// changes it, and only that one raises what it newly holds.
pub const RAISE: &str = "UPDATE \"jadpo_schedules\" SET \"alerts\" = $2 WHERE \"schedule\" = $1 AND \"alerts\" IS NOT DISTINCT FROM $3 RETURNING \"schedule\"";

/// A schedule as the project declares it: its identity key, its name and
/// its `every:`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Declared {
    pub key: String,
    pub name: String,
    pub every_ms: u64,
}

impl From<&SchedulePlan> for Declared {
    fn from(plan: &SchedulePlan) -> Declared {
        Declared {
            key: plan.key.clone(),
            name: plan.name.clone(),
            every_ms: plan.every_ms,
        }
    }
}

/// A schedule's state row, times in milliseconds since the epoch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StateRow {
    pub schedule: String,
    pub every_ms: i64,
    pub next_run_at: i64,
    pub paused: bool,
    pub lease_until: Option<i64>,
    pub run_id: Option<[u8; 16]>,
    pub last_tick_at: Option<i64>,
    pub last_succeeded_at: Option<i64>,
    /// The run an operator asked the application's tick for, still
    /// waiting (chunk 6.2d).
    pub requested: Option<[u8; 16]>,
    /// The alerts a tick has raised for it that still held at the last
    /// tick, by id, each with when it was raised (chunk 6.9).
    pub raised: BTreeMap<String, i64>,
}

impl StateRow {
    /// Whether a run holds its lease at `now`.
    pub fn leased(&self, now: i64) -> bool {
        self.run_id.is_some() && self.lease_until.is_some_and(|until| until > now)
    }

    /// Whether a run held its lease past its end and never gave it back:
    /// its worker was lost.
    pub fn lease_expired(&self, now: i64) -> bool {
        self.run_id.is_some() && self.lease_until.is_some_and(|until| until <= now)
    }
}

/// One run record, times in milliseconds since the epoch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunRecord {
    pub id: [u8; 16],
    pub schedule: String,
    pub due: i64,
    pub started: i64,
    pub ended: Option<i64>,
    /// `running`, `succeeded`, `failed` or `skipped`.
    pub outcome: String,
    /// Why a tick skipped it (`paused`, `lease_held`), or `by_hand` for a
    /// run an operator started.
    pub reason: Option<String>,
    pub code: Option<String>,
    pub kind: Option<String>,
    pub events: Option<i32>,
    pub trace: Option<String>,
    /// The items it processed, for a run that counts them: the records a
    /// purge deleted (chunk 6.2c).
    pub items: Option<i64>,
}

impl RunRecord {
    pub fn is_running(&self) -> bool {
        self.outcome == RUNNING
    }

    pub fn is_skipped(&self) -> bool {
        self.outcome == SKIPPED
    }

    pub fn by_hand_run(&self) -> bool {
        self.reason.as_deref() == Some(BY_HAND)
    }

    /// How late it started after it was due.
    pub fn delay_ms(&self) -> i64 {
        self.started.saturating_sub(self.due).max(0)
    }

    /// How long it took, or has taken by `now` while it runs.
    pub fn duration_ms(&self, now: i64) -> i64 {
        self.ended
            .unwrap_or(now)
            .saturating_sub(self.started)
            .max(0)
    }
}

/// One schedule as a reader sees it: as declared, its state row if a
/// tick has seen it, its latest run, its failures since its last success
/// and the earliest due time it has a record of.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Watched {
    pub declared: Declared,
    pub state: Option<StateRow>,
    pub last_run: Option<RunRecord>,
    pub failures_in_a_row: u32,
    pub first_due: Option<i64>,
}

/// Every declared schedule, the last tick any of them received, and the
/// alerts that hold at the time it was read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Watch {
    pub schedules: Vec<Watched>,
    pub last_tick: Option<i64>,
    pub alerts: Vec<Alert>,
}

/// A schedule's metrics over the records kept (30 days).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Metrics {
    /// Records: runs and skipped ticks.
    pub records: u64,
    pub runs: u64,
    pub failed: u64,
    pub skipped: u64,
    pub median_ms: Option<i64>,
    pub longest_ms: Option<i64>,
    pub median_delay_ms: Option<i64>,
    pub most_delay_ms: Option<i64>,
}

/// An alert (area 12, "Watching schedules": "Alerts with no setup").
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Alert {
    /// The schedule's name; none for ticks, which every schedule shares.
    pub schedule: Option<String>,
    pub kind: AlertKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AlertKind {
    /// No tick has arrived since `since`, for more than twice the period
    /// the host ticks at.
    TicksStopped { since: i64 },
    /// The run that started at `started` held its lease past `until` and
    /// never gave it back.
    LeaseExpired { started: Option<i64>, until: i64 },
    /// The run that started at `started` has taken, or took, `ms`: past
    /// 80% of the 15-minute limit.
    LongRun {
        started: i64,
        ms: i64,
        running: bool,
    },
    /// `count` failed runs since the last that succeeded, three or more.
    FailedInARow { count: u32 },
    /// No successful run since `since` (the last one's start, or the first
    /// due time when none has succeeded), over twice the interval.
    NoSuccess {
        since: i64,
        ever: bool,
        every_ms: i64,
    },
}

impl Alert {
    /// The alert's stable id, as its log event names it.
    pub fn id(&self) -> &'static str {
        match self.kind {
            AlertKind::TicksStopped { .. } => "ticks_stopped",
            AlertKind::LeaseExpired { .. } => "lease_expired",
            AlertKind::LongRun { .. } => "long_run",
            AlertKind::FailedInARow { .. } => "failed_in_a_row",
            AlertKind::NoSuccess { .. } => "no_success",
        }
    }

    /// What identifies it while it holds: its id and schedule.
    pub fn identity(&self) -> String {
        format!("{} {}", self.id(), self.schedule.as_deref().unwrap_or(""))
    }

    /// Its log event (area 14, question 13: alerts are log events the
    /// hosting platform's alerting picks up), times as RFC 3339 text,
    /// never a value from the app's data.
    pub fn event(&self) -> serde_json::Value {
        let at = |millis: i64| serde_json::Value::from(jadpo_std::format_instant(millis));
        let mut object = serde_json::Map::new();
        object.insert("event".into(), "schedule.alert".into());
        object.insert("alert".into(), self.id().into());
        object.insert(
            "schedule".into(),
            self.schedule
                .as_deref()
                .map_or(serde_json::Value::Null, serde_json::Value::from),
        );
        match &self.kind {
            AlertKind::TicksStopped { since } => {
                object.insert("since".into(), at(*since));
            }
            AlertKind::LeaseExpired { started, until } => {
                if let Some(started) = started {
                    object.insert("started".into(), at(*started));
                }
                object.insert("until".into(), at(*until));
            }
            AlertKind::LongRun {
                started,
                ms,
                running,
            } => {
                object.insert("started".into(), at(*started));
                object.insert("ms".into(), (*ms).into());
                object.insert("running".into(), (*running).into());
            }
            AlertKind::FailedInARow { count } => {
                object.insert("failures".into(), (*count).into());
            }
            AlertKind::NoSuccess {
                since,
                ever,
                every_ms,
            } => {
                object.insert("since".into(), at(*since));
                object.insert("succeededBefore".into(), (*ever).into());
                object.insert("everyMs".into(), (*every_ms).into());
            }
        }
        serde_json::Value::Object(object)
    }
}

/// How many failed runs in a row raise an alert.
pub const FAILURES_ALERT: u32 = 3;
/// A run past this many milliseconds is past 80% of the 15-minute limit.
pub const LONG_RUN_MS: i64 = (RUN_LIMIT_MS as i64) * 4 / 5;

/// The alerts that hold at `now` for `schedules`, given the last tick any
/// of them received and the period the host ticks at: ticks first, then
/// each schedule's in the order given.
pub fn alerts(
    schedules: &[Watched],
    last_tick: Option<i64>,
    tick_every_ms: u64,
    now: i64,
) -> Vec<Alert> {
    let mut out = Vec::new();
    let period = i64::try_from(tick_every_ms).unwrap_or(i64::MAX).max(1_000);
    if let Some(since) = last_tick
        && now.saturating_sub(since) > period.saturating_mul(2)
    {
        out.push(Alert {
            schedule: None,
            kind: AlertKind::TicksStopped { since },
        });
    }
    for watched in schedules {
        let name = || Some(watched.declared.name.clone());
        let Some(state) = &watched.state else {
            continue;
        };
        if state.lease_expired(now) {
            out.push(Alert {
                schedule: name(),
                kind: AlertKind::LeaseExpired {
                    started: watched
                        .last_run
                        .as_ref()
                        .filter(|run| Some(run.id) == state.run_id)
                        .map(|run| run.started),
                    until: state.lease_until.unwrap_or(now),
                },
            });
        }
        if let Some(run) = &watched.last_run {
            let ms = run.duration_ms(now);
            if ms >= LONG_RUN_MS && (run.ended.is_some() || !state.lease_expired(now)) {
                out.push(Alert {
                    schedule: name(),
                    kind: AlertKind::LongRun {
                        started: run.started,
                        ms,
                        running: run.ended.is_none(),
                    },
                });
            }
        }
        if watched.failures_in_a_row >= FAILURES_ALERT {
            out.push(Alert {
                schedule: name(),
                kind: AlertKind::FailedInARow {
                    count: watched.failures_in_a_row,
                },
            });
        }
        // A paused schedule isn't meant to run.
        if !state.paused {
            let every = state.every_ms.max(1);
            let since = state.last_succeeded_at.map(|at| (at, true)).or_else(|| {
                let first = watched
                    .first_due
                    .map_or(state.next_run_at, |due| due.min(state.next_run_at));
                Some((first, false))
            });
            if let Some((since, ever)) = since
                && now.saturating_sub(since) > every.saturating_mul(2)
            {
                out.push(Alert {
                    schedule: name(),
                    kind: AlertKind::NoSuccess {
                        since,
                        ever,
                        every_ms: every,
                    },
                });
            }
        }
    }
    out
}

fn millis(value: Option<&SqlValue>) -> Option<i64> {
    match value {
        Some(SqlValue::Timestamp(at)) => Some(at.div_euclid(1000)),
        _ => None,
    }
}

fn text(value: Option<&SqlValue>) -> Option<String> {
    match value {
        Some(SqlValue::Text(text)) => Some(text.clone()),
        _ => None,
    }
}

fn int(value: Option<&SqlValue>) -> Option<i64> {
    match value {
        Some(SqlValue::Int8(n)) => Some(*n),
        Some(SqlValue::Int4(n)) => Some(i64::from(*n)),
        _ => None,
    }
}

fn uuid(value: Option<&SqlValue>) -> Option<[u8; 16]> {
    match value {
        Some(SqlValue::Uuid(id)) => Some(*id),
        _ => None,
    }
}

fn read_state(row: &[SqlValue]) -> Option<StateRow> {
    Some(StateRow {
        schedule: text(row.first())?,
        every_ms: int(row.get(1))?,
        next_run_at: millis(row.get(2))?,
        paused: matches!(row.get(3), Some(SqlValue::Bool(true))),
        lease_until: millis(row.get(4)),
        run_id: uuid(row.get(5)),
        last_tick_at: millis(row.get(6)),
        last_succeeded_at: millis(row.get(7)),
        requested: uuid(row.get(8)),
        raised: match row.get(9) {
            Some(SqlValue::Json(text)) => raised_from(text),
            _ => BTreeMap::new(),
        },
    })
}

/// The alerts a state row records as raised: an object of each alert's id
/// and when it was raised, in milliseconds since the epoch.
fn raised_from(text: &str) -> BTreeMap<String, i64> {
    match serde_json::from_str::<serde_json::Value>(text) {
        Ok(serde_json::Value::Object(map)) => map
            .into_iter()
            .filter_map(|(id, at)| Some((id, at.as_i64()?)))
            .collect(),
        _ => BTreeMap::new(),
    }
}

/// What a state row keeps for `raised`: nothing when none is.
fn raised_json(raised: &BTreeMap<String, i64>) -> SqlValue {
    if raised.is_empty() {
        return SqlValue::Null(SqlType::Json);
    }
    let object: serde_json::Map<String, serde_json::Value> = raised
        .iter()
        .map(|(id, at)| (id.clone(), serde_json::Value::from(*at)))
        .collect();
    SqlValue::Json(serde_json::Value::Object(object).to_string())
}

/// What a tick records as raised for a schedule whose row records
/// `recorded`, given the alerts `holding` for it at `now`: each that holds,
/// since when it was first raised, kept while it goes on holding. `None`
/// when that is what is recorded already.
pub fn to_record(
    recorded: &BTreeMap<String, i64>,
    holding: &[Alert],
    now: i64,
) -> Option<BTreeMap<String, i64>> {
    let record: BTreeMap<String, i64> = holding
        .iter()
        .map(|alert| {
            let id = alert.id().to_string();
            let since = recorded.get(&id).copied().unwrap_or(now);
            (id, since)
        })
        .collect();
    (record != *recorded).then_some(record)
}

/// Raises at `now` the alerts that have started to hold for `declared`
/// since the last tick, for a host ticking every `tick_every_ms`: each
/// schedule's are recorded on its state row in one conditional statement
/// ([`RAISE`]), so the tick that changed the record raises them and every
/// other instance's, finding it changed, raises nothing. The alerts it
/// raised, in [`alerts`]' order. Ticks stopping is never raised here: a
/// tick arriving is the proof that they haven't.
pub async fn raise(
    database: &dyn Database,
    declared: &[Declared],
    tick_every_ms: u64,
    now: i64,
) -> Result<Vec<Alert>, StoreError> {
    let read = watch(database, declared, tick_every_ms, now).await?;
    let mut raised = Vec::new();
    for watched in &read.schedules {
        let Some(state) = &watched.state else {
            continue;
        };
        let holding = alerts(std::slice::from_ref(watched), None, tick_every_ms, now);
        let Some(record) = to_record(&state.raised, &holding, now) else {
            continue;
        };
        let changed = database
            .query_once(
                RAISE,
                &[
                    SqlValue::Text(state.schedule.clone()),
                    raised_json(&record),
                    raised_json(&state.raised),
                ],
            )
            .await?;
        if changed.is_empty() {
            continue;
        }
        raised.extend(
            holding
                .into_iter()
                .filter(|alert| !state.raised.contains_key(alert.id())),
        );
    }
    Ok(raised)
}

fn read_run(row: &[SqlValue]) -> Option<RunRecord> {
    Some(RunRecord {
        id: uuid(row.first())?,
        schedule: text(row.get(1))?,
        due: millis(row.get(2))?,
        started: millis(row.get(3))?,
        ended: millis(row.get(4)),
        outcome: text(row.get(5))?,
        reason: text(row.get(6)),
        code: text(row.get(7)),
        kind: text(row.get(8)),
        events: int(row.get(9)).and_then(|n| i32::try_from(n).ok()),
        trace: text(row.get(10)),
        items: int(row.get(11)),
    })
}

fn keys(declared: &[Declared]) -> SqlValue {
    SqlValue::Array(
        SqlType::Text,
        declared
            .iter()
            .map(|schedule| SqlValue::Text(schedule.key.clone()))
            .collect(),
    )
}

/// Every state row in `database`.
pub async fn states(database: &dyn Database) -> Result<Vec<StateRow>, StoreError> {
    let rows = database.query_once(STATES, &[]).await?;
    Ok(rows.iter().filter_map(|row| read_state(row)).collect())
}

/// Reads `declared` from `database` at `now`, and the alerts that hold
/// for a host ticking every `tick_every_ms`.
pub async fn watch(
    database: &dyn Database,
    declared: &[Declared],
    tick_every_ms: u64,
    now: i64,
) -> Result<Watch, StoreError> {
    let states = states(database).await?;
    let runs: Vec<RunRecord> = database
        .query_once(&latest_runs_sql(), &[keys(declared)])
        .await?
        .iter()
        .filter_map(|row| read_run(row))
        .collect();
    let streaks: BTreeMap<String, (u32, Option<i64>)> = database
        .query_once(STREAKS, &[keys(declared)])
        .await?
        .iter()
        .filter_map(|row| {
            Some((
                text(row.first())?,
                (
                    u32::try_from(int(row.get(1)).unwrap_or(0)).unwrap_or(u32::MAX),
                    millis(row.get(2)),
                ),
            ))
        })
        .collect();
    let schedules: Vec<Watched> = declared
        .iter()
        .map(|declared| {
            let (failures_in_a_row, first_due) =
                streaks.get(&declared.key).copied().unwrap_or((0, None));
            Watched {
                declared: declared.clone(),
                state: states
                    .iter()
                    .find(|state| state.schedule == declared.key)
                    .cloned(),
                last_run: runs
                    .iter()
                    .find(|run| run.schedule == declared.key)
                    .cloned(),
                failures_in_a_row,
                first_due,
            }
        })
        .collect();
    let last_tick = schedules
        .iter()
        .filter_map(|watched| watched.state.as_ref()?.last_tick_at)
        .max();
    let alerts = alerts(&schedules, last_tick, tick_every_ms, now);
    Ok(Watch {
        schedules,
        last_tick,
        alerts,
    })
}

/// Schedule `key`'s records, newest first: `limit` of them after the
/// first `offset`.
pub async fn history(
    database: &dyn Database,
    key: &str,
    limit: u32,
    offset: u64,
) -> Result<Vec<RunRecord>, StoreError> {
    let rows = database
        .query_once(
            &history_sql(),
            &[
                SqlValue::Text(key.to_string()),
                SqlValue::Int8(i64::from(limit)),
                SqlValue::Int8(i64::try_from(offset).unwrap_or(i64::MAX)),
            ],
        )
        .await?;
    Ok(rows.iter().filter_map(|row| read_run(row)).collect())
}

/// Schedule `key`'s metrics over the records kept.
pub async fn metrics(database: &dyn Database, key: &str) -> Result<Metrics, StoreError> {
    let rows = database
        .query_once(METRICS, &[SqlValue::Text(key.to_string())])
        .await?;
    let Some(row) = rows.first() else {
        return Ok(Metrics::default());
    };
    let count = |i: usize| u64::try_from(int(row.get(i)).unwrap_or(0)).unwrap_or(0);
    Ok(Metrics {
        records: count(0),
        runs: count(1),
        failed: count(2),
        skipped: count(3),
        median_ms: int(row.get(4)),
        longest_ms: int(row.get(5)),
        median_delay_ms: int(row.get(6)),
        most_delay_ms: int(row.get(7)),
    })
}

/// Makes `schedule`'s state row if no tick has yet, its first run due one
/// interval after `now`, as the first tick to see it would.
async fn ensure(database: &dyn Database, schedule: &Declared, now: i64) -> Result<(), StoreError> {
    let every = i64::try_from(schedule.every_ms).unwrap_or(i64::MAX).max(1);
    database
        .query_once(
            ENSURE,
            &[
                SqlValue::Array(SqlType::Text, vec![SqlValue::Text(schedule.key.clone())]),
                SqlValue::Array(SqlType::Int8, vec![SqlValue::Int8(every)]),
                SqlValue::Array(SqlType::Timestamp, vec![micros(now.saturating_add(every))]),
            ],
        )
        .await?;
    Ok(())
}

/// What pausing or resuming did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Paused {
    /// It changed; the schedule's next due time.
    Changed { next_run_at: Option<i64> },
    /// It was so already.
    Already,
}

/// Pauses `schedule` (`paused` true) or resumes it at `now`: kept on its
/// state row, made if no tick has seen it yet, so every instance's next
/// tick sees it.
pub async fn set_paused(
    database: &dyn Database,
    schedule: &Declared,
    paused: bool,
    now: i64,
) -> Result<Paused, StoreError> {
    ensure(database, schedule, now).await?;
    let rows = database
        .query_once(
            SET_PAUSED,
            &[SqlValue::Text(schedule.key.clone()), SqlValue::Bool(paused)],
        )
        .await?;
    Ok(match rows.first() {
        Some(row) => Paused::Changed {
            next_run_at: millis(row.first()),
        },
        None => Paused::Already,
    })
}

/// How a run started now went.
#[derive(Clone, Debug)]
pub enum RunNow {
    /// It ran: how, and the outcome of each delivery of what it emitted.
    Ran {
        due: super::Due,
        deliveries: Vec<Outcome>,
    },
    /// A run that started at `started` holds the lease until `until`.
    LeaseHeld { started: Option<i64>, until: i64 },
}

/// Runs `schedule` at `now`, outside the grid, as `jadpo schedules run`
/// does: under its lease, paused or not, so only one run of it happens at
/// a time; its record says an operator started it (`by_hand`), and its
/// next due time is left as it is. It runs within the budget its host
/// gives a tick, as the tick's runs do (chunk 6.7).
pub async fn run_now(
    executor: &dyn Executor,
    host: &dyn Host,
    store: &str,
    schedule: &SchedulePlan,
    now: i64,
) -> Result<RunNow, StoreError> {
    let database = host
        .database(store)
        .ok_or_else(|| StoreError::Fault(format!("no store called `{store}`")))?;
    ensure(database, &Declared::from(schedule), now).await?;
    let env = Env {
        seed: None,
        clock: Clock::fixed(now),
    };
    let mut ctx = Ctx::new(executor.places(), host, env);
    let run = ctx
        .new_id()
        .ok_or_else(|| StoreError::Fault("no random bytes for a run's id".into()))?;
    let budget = Budget::start(host, Invocation::Tick);
    let lease = Leased {
        key: schedule.key.clone(),
        run,
        take: Take::Now,
        succeeded: None,
    };
    let Some(row) = crate::lease::claim(database, &budget, &lease, now).await? else {
        let (started, until) = holder(database, &schedule.key, now).await?;
        return Ok(RunNow::LeaseHeld { started, until });
    };
    let lost = uuid(row.first()).map(|lost| (lost, millis(row.get(1))));
    let claimed = Claimed {
        lease,
        due_slot: now,
        lost,
        reason: Some(BY_HAND),
    };
    let (due, left) = run_claimed(executor, &budget, database, schedule, claimed, ctx, now).await;
    let deliveries = runner::run_within(executor, host, &budget, left).await;
    Ok(RunNow::Ran { due, deliveries })
}

/// The run holding schedule `key`'s lease: when it started, if its
/// record says, and the lease's end.
async fn holder(
    database: &dyn Database,
    key: &str,
    now: i64,
) -> Result<(Option<i64>, i64), StoreError> {
    let state = states(database)
        .await?
        .into_iter()
        .find(|state| state.schedule == key);
    let until = state
        .as_ref()
        .and_then(|state| state.lease_until)
        .unwrap_or(now);
    let started = match state.and_then(|state| state.run_id) {
        Some(id) => run(database, id).await?.map(|run| run.started),
        None => None,
    };
    Ok((started, until))
}

/// The run record with id `id`, if there is one.
pub async fn run(database: &dyn Database, id: [u8; 16]) -> Result<Option<RunRecord>, StoreError> {
    let rows = database
        .query_once(&run_sql(), &[SqlValue::Uuid(id)])
        .await?;
    Ok(rows.iter().find_map(|row| read_run(row)))
}

/// What asking a schedule's tick for a run did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Requested {
    /// The run `run` waits for the application's next tick; `joined` when
    /// it was asked for already and this request waits for the same run.
    Waiting { run: [u8; 16], joined: bool },
    /// A run that started at `started` holds the lease until `until`, so
    /// nothing was asked.
    LeaseHeld { started: Option<i64>, until: i64 },
}

/// Asks `schedule`'s next tick to run it by hand, as `jadpo schedules run`
/// does against a deployment (area 12, decided 2026-10-19; chunk 6.2d):
/// the request is kept on its state row, made if no tick has seen it yet,
/// as the id its run's record will have, so the application's own server
/// runs it and the operator's machine runs no application code. Refused
/// while a run holds the lease at `now`; a request already standing is
/// joined.
pub async fn request_run(
    database: &dyn Database,
    host: &dyn Host,
    schedule: &Declared,
    now: i64,
) -> Result<Requested, StoreError> {
    ensure(database, schedule, now).await?;
    let id = crate::ctx::random_id(host, now)
        .ok_or_else(|| StoreError::Fault("no random bytes for a run's id".into()))?;
    let rows = database
        .query_once(
            REQUEST,
            &[
                SqlValue::Text(schedule.key.clone()),
                SqlValue::Uuid(id),
                micros(now),
            ],
        )
        .await?;
    match rows.first().and_then(|row| uuid(row.first())) {
        Some(run) => Ok(Requested::Waiting {
            run,
            joined: run != id,
        }),
        None => {
            let (started, until) = holder(database, &schedule.key, now).await?;
            Ok(Requested::LeaseHeld { started, until })
        }
    }
}

/// Withdraws the request for run `run` from schedule `key`'s state row,
/// unless a tick has taken it: whether it was withdrawn.
pub async fn withdraw(
    database: &dyn Database,
    key: &str,
    run: [u8; 16],
) -> Result<bool, StoreError> {
    let rows = database
        .query_once(
            WITHDRAW,
            &[SqlValue::Text(key.to_string()), SqlValue::Uuid(run)],
        )
        .await?;
    Ok(!rows.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINUTE: i64 = 60_000;

    fn watched(every: i64) -> Watched {
        Watched {
            declared: Declared {
                key: "schedule home.census".into(),
                name: "census".into(),
                every_ms: every as u64,
            },
            state: Some(StateRow {
                schedule: "schedule home.census".into(),
                every_ms: every,
                next_run_at: 60 * MINUTE,
                paused: false,
                lease_until: None,
                run_id: None,
                last_tick_at: Some(50 * MINUTE),
                last_succeeded_at: Some(45 * MINUTE),
                requested: None,
                raised: BTreeMap::new(),
            }),
            last_run: None,
            failures_in_a_row: 0,
            first_due: Some(0),
        }
    }

    fn ids(alerts: &[Alert]) -> Vec<&'static str> {
        alerts.iter().map(Alert::id).collect()
    }

    #[test]
    fn a_healthy_schedule_raises_nothing() {
        let all = alerts(
            &[watched(15 * MINUTE)],
            Some(50 * MINUTE),
            60_000,
            51 * MINUTE,
        );
        assert!(all.is_empty(), "{all:?}");
    }

    #[test]
    fn ticks_stopped_after_twice_the_tick_period() {
        let list = [watched(15 * MINUTE)];
        assert!(alerts(&list, Some(50 * MINUTE), 60_000, 52 * MINUTE).is_empty());
        let late = alerts(&list, Some(50 * MINUTE), 60_000, 52 * MINUTE + 1);
        assert_eq!(ids(&late), ["ticks_stopped"]);
        assert_eq!(late[0].schedule, None);
    }

    #[test]
    fn no_success_within_twice_the_interval_unless_paused() {
        let mut one = watched(15 * MINUTE);
        let at = 45 * MINUTE + 30 * MINUTE;
        assert!(alerts(std::slice::from_ref(&one), None, 60_000, at).is_empty());
        assert_eq!(
            ids(&alerts(std::slice::from_ref(&one), None, 60_000, at + 1)),
            ["no_success"]
        );
        one.state.as_mut().unwrap().paused = true;
        assert!(alerts(&[one], None, 60_000, at + 1).is_empty());
        // Never succeeded: counted from the first due time.
        let mut never = watched(15 * MINUTE);
        never.state.as_mut().unwrap().last_succeeded_at = None;
        never.first_due = Some(15 * MINUTE);
        let raised = alerts(&[never], None, 60_000, 45 * MINUTE + 1);
        assert_eq!(
            raised[0].kind,
            AlertKind::NoSuccess {
                since: 15 * MINUTE,
                ever: false,
                every_ms: 15 * MINUTE
            }
        );
    }

    #[test]
    fn three_failures_a_long_run_and_a_lost_lease() {
        let mut one = watched(15 * MINUTE);
        one.failures_in_a_row = 3;
        let run = RunRecord {
            id: [7; 16],
            schedule: "schedule home.census".into(),
            due: 45 * MINUTE,
            started: 45 * MINUTE,
            ended: None,
            outcome: "running".into(),
            reason: None,
            code: None,
            kind: None,
            events: None,
            trace: None,
            items: None,
        };
        one.last_run = Some(run);
        let state = one.state.as_mut().unwrap();
        state.run_id = Some([7; 16]);
        state.lease_until = Some(61 * MINUTE);
        state.last_succeeded_at = Some(44 * MINUTE);
        // 12 minutes in: past 80% of the limit.
        let at = 57 * MINUTE;
        assert_eq!(
            ids(&alerts(std::slice::from_ref(&one), None, 60_000, at)),
            ["long_run", "failed_in_a_row"]
        );
        // Past the lease's end the worker was lost: that, not a long run.
        let lost = alerts(&[one], None, 60_000, 62 * MINUTE);
        assert_eq!(ids(&lost), ["lease_expired", "failed_in_a_row"]);
        assert_eq!(
            lost[0].kind,
            AlertKind::LeaseExpired {
                started: Some(45 * MINUTE),
                until: 61 * MINUTE
            }
        );
    }

    #[test]
    fn an_alert_is_recorded_once_until_it_stops_holding() {
        let alert = Alert {
            schedule: Some("census".into()),
            kind: AlertKind::FailedInARow { count: 3 },
        };
        // Newly holding: recorded since now.
        let none = BTreeMap::new();
        let first = to_record(&none, std::slice::from_ref(&alert), 10).unwrap();
        assert_eq!(first, BTreeMap::from([("failed_in_a_row".to_string(), 10)]));
        // Still holding, its count grown: nothing to record.
        let more = Alert {
            kind: AlertKind::FailedInARow { count: 4 },
            ..alert.clone()
        };
        assert_eq!(to_record(&first, &[more], 20), None);
        // Stopped holding: the record is emptied, and nothing is kept.
        assert_eq!(to_record(&first, &[], 30), Some(BTreeMap::new()));
        assert_eq!(to_record(&none, &[], 30), None);
        // A record survives a row read back.
        assert_eq!(
            raised_from(&format!(
                "{}",
                match raised_json(&first) {
                    SqlValue::Json(text) => text,
                    other => panic!("{other:?}"),
                }
            )),
            first
        );
        assert_eq!(raised_json(&none), SqlValue::Null(SqlType::Json));
    }

    #[test]
    fn an_alert_is_a_log_event() {
        let alert = Alert {
            schedule: Some("census".into()),
            kind: AlertKind::FailedInARow { count: 3 },
        };
        assert_eq!(
            alert.event().to_string(),
            r#"{"alert":"failed_in_a_row","event":"schedule.alert","failures":3,"schedule":"census"}"#
        );
    }
}
