//! "Why didn't it run?" for one schedule (area 12, "Watching schedules";
//! area 17's `jadpo why`; chunk 6.9): the answer decided from the
//! schedule's state row and run records, as data, so `jadpo why`, the
//! console and the MCP server each say the same answer their own way.
//!
//! The answer is the first that holds: paused (and the due times skipped
//! since); no tick since a time; a lease held past its end; a run still
//! holding it; a tick skipped since the last run; the last run failed; it
//! succeeded; not due yet; or no tick has seen it.

use super::watch::{self, Alert, AlertKind, Declared, RunRecord, Watch};
use crate::{Database, StoreError};

/// How many of a schedule's newest records the answer reads.
pub const RECENT: u32 = 20;

/// The answer for one schedule at a time, with what else holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Why {
    pub answer: Answer,
    /// A run an operator asked the application's tick for, which no tick
    /// has taken yet (chunk 6.2d).
    pub requested: bool,
    /// Every alert that holds; the one the answer already says is
    /// [`Answer::restates`].
    pub alerts: Vec<Alert>,
}

impl Why {
    /// The alerts besides the one the answer says.
    pub fn other_alerts(&self) -> impl Iterator<Item = &Alert> {
        let restated = self.answer.restates();
        self.alerts
            .iter()
            .filter(move |alert| Some(alert.id()) != restated)
    }
}

/// Due times a paused schedule has skipped: how many in a row, newest
/// first, and the due time of the first of them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Skips {
    pub count: u64,
    pub since: i64,
}

/// Why a schedule did or didn't run, times in milliseconds since the
/// epoch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Answer {
    /// No tick has seen it: the application hasn't run since it was added.
    NotSeen,
    /// A tick has seen it, and its first run is due at `first_due`.
    NotDue { first_due: i64 },
    /// An operator paused it, so its due ticks are skipped: those skipped
    /// in a row since, and its last run.
    Paused {
        skipped: Option<Skips>,
        last_run: Option<RunRecord>,
    },
    /// No tick has reached it since `since`, for a host that ticks every
    /// `tick_every_ms`; `overdue` is its due time when that has passed.
    TicksStopped {
        since: i64,
        overdue: Option<i64>,
        tick_every_ms: u64,
    },
    /// The run that started at `started` held its lease past `until` and
    /// never gave it back: its worker was lost. The tick due at `next`
    /// records it so and runs it again.
    LeaseExpired {
        started: Option<i64>,
        until: i64,
        next: i64,
    },
    /// The run that started at `started` still holds its lease, until
    /// `lease_until`, and `skipped` due ticks have been skipped for it.
    Running {
        started: i64,
        lease_until: i64,
        skipped: u64,
    },
    /// The tick due at `due` was skipped since its last run started: it was
    /// paused then (`paused`), or the run before was still going.
    Skipped {
        due: i64,
        paused: bool,
        last_run: Option<RunRecord>,
        next: i64,
    },
    /// Its last run failed, after `failures_in_a_row - 1` before it; its
    /// next due time, `next`, is its retry.
    Failed {
        run: RunRecord,
        failures_in_a_row: u32,
        next: i64,
    },
    /// Its last run succeeded; it is next due at `next`.
    Succeeded { run: RunRecord, next: i64 },
}

impl Answer {
    /// The answer's stable id, as `--json` names it.
    pub fn id(&self) -> &'static str {
        match self {
            Answer::NotSeen => "not_seen",
            Answer::NotDue { .. } => "not_due",
            Answer::Paused { .. } => "paused",
            Answer::TicksStopped { .. } => "ticks_stopped",
            Answer::LeaseExpired { .. } => "lease_expired",
            Answer::Running { .. } => "running",
            Answer::Skipped { .. } => "skipped",
            Answer::Failed { .. } => "failed",
            Answer::Succeeded { .. } => "succeeded",
        }
    }

    /// The alert the answer already says, by its id.
    pub fn restates(&self) -> Option<&'static str> {
        match self {
            Answer::TicksStopped { .. } => Some("ticks_stopped"),
            Answer::LeaseExpired { .. } => Some("lease_expired"),
            _ => None,
        }
    }
}

/// The answer for the one schedule `watch` was read for, given its newest
/// records, `recent`, at `now`.
pub fn answer(watch: &Watch, recent: &[RunRecord], now: i64) -> Answer {
    let Some(schedule) = watch.schedules.first() else {
        return Answer::NotSeen;
    };
    let Some(state) = &schedule.state else {
        return Answer::NotSeen;
    };
    let last_run = schedule.last_run.as_ref();
    if state.paused {
        let skips: Vec<&RunRecord> = recent
            .iter()
            .take_while(|record| record.is_skipped() && record.reason.as_deref() == Some("paused"))
            .collect();
        return Answer::Paused {
            skipped: skips.last().map(|first| Skips {
                count: skips.len() as u64,
                since: first.due,
            }),
            last_run: last_run.cloned(),
        };
    }
    if let Some(since) = watch.alerts.iter().find_map(|alert| match alert.kind {
        AlertKind::TicksStopped { since } => Some(since),
        _ => None,
    }) {
        return Answer::TicksStopped {
            since,
            overdue: (state.next_run_at <= now).then_some(state.next_run_at),
            tick_every_ms: tick_every_ms(watch),
        };
    }
    if state.lease_expired(now) {
        return Answer::LeaseExpired {
            started: last_run
                .filter(|run| Some(run.id) == state.run_id)
                .map(|run| run.started),
            until: state.lease_until.unwrap_or(now),
            next: state.next_run_at.max(now),
        };
    }
    if state.leased(now)
        && let Some(run) = last_run.filter(|run| run.is_running())
    {
        return Answer::Running {
            started: run.started,
            lease_until: state.lease_until.unwrap_or(now),
            skipped: recent
                .iter()
                .take_while(|record| record.is_skipped())
                .filter(|record| record.reason.as_deref() == Some("lease_held"))
                .count() as u64,
        };
    }
    // A tick skipped since the last run started: it was still going then.
    if let Some(skip) = recent.first().filter(|record| {
        record.is_skipped() && last_run.is_none_or(|run| record.started > run.started)
    }) {
        return Answer::Skipped {
            due: skip.due,
            paused: skip.reason.as_deref() == Some("paused"),
            last_run: last_run.cloned(),
            next: state.next_run_at,
        };
    }
    match last_run {
        Some(run) if run.outcome == super::FAILED => Answer::Failed {
            run: run.clone(),
            failures_in_a_row: schedule.failures_in_a_row,
            next: state.next_run_at,
        },
        Some(run) => Answer::Succeeded {
            run: run.clone(),
            next: state.next_run_at,
        },
        None => Answer::NotDue {
            first_due: state.next_run_at,
        },
    }
}

/// How often the host ticks for the schedules `watch` was read for: once a
/// minute, or as often as the shortest `every:` when that is shorter, at
/// least once a second.
fn tick_every_ms(watch: &Watch) -> u64 {
    watch
        .schedules
        .iter()
        .map(|watched| watched.declared.every_ms)
        .min()
        .map_or(60_000, |shortest| shortest.clamp(1_000, 60_000))
}

/// Reads `schedule`'s rows from `database` and answers for it at `now`,
/// for a host ticking every `tick_every_ms`.
pub async fn why(
    database: &dyn Database,
    schedule: &Declared,
    tick_every_ms: u64,
    now: i64,
) -> Result<Why, StoreError> {
    let declared = std::slice::from_ref(schedule);
    let read = watch::watch(database, declared, tick_every_ms, now).await?;
    let recent = watch::history(database, &schedule.key, RECENT, 0).await?;
    let requested = read
        .schedules
        .first()
        .and_then(|watched| watched.state.as_ref())
        .is_some_and(|state| state.requested.is_some());
    Ok(Why {
        answer: answer(&read, &recent, now),
        requested,
        alerts: read.alerts,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::schedules::watch::{StateRow, Watched, alerts};

    const MINUTE: i64 = 60_000;

    fn run(outcome: &str, due: i64, started: i64, ended: Option<i64>) -> RunRecord {
        RunRecord {
            id: [7; 16],
            schedule: "schedule bell_check".into(),
            due,
            started,
            ended,
            outcome: outcome.into(),
            reason: None,
            code: (outcome == "failed").then(|| "bell_missing".into()),
            kind: (outcome == "failed").then(|| "unavailable".into()),
            events: Some(0),
            trace: None,
            items: None,
        }
    }

    fn skip(reason: &str, due: i64) -> RunRecord {
        RunRecord {
            reason: Some(reason.into()),
            ..run("skipped", due, due, Some(due))
        }
    }

    /// `bell_check`, every hour, as read at `now` with `last_run` and
    /// `failures` in a row.
    fn read(last_run: Option<RunRecord>, failures: u32, now: i64) -> Watch {
        let watched = Watched {
            declared: Declared {
                key: "schedule bell_check".into(),
                name: "bell_check".into(),
                every_ms: 60 * MINUTE as u64,
            },
            state: Some(StateRow {
                schedule: "schedule bell_check".into(),
                every_ms: 60 * MINUTE,
                next_run_at: 120 * MINUTE,
                paused: false,
                lease_until: None,
                run_id: None,
                last_tick_at: Some(now),
                last_succeeded_at: None,
                requested: None,
                raised: BTreeMap::new(),
            }),
            last_run,
            failures_in_a_row: failures,
            first_due: Some(60 * MINUTE),
        };
        let schedules = vec![watched];
        let alerts = alerts(&schedules, Some(now), 60_000, now);
        Watch {
            schedules,
            last_tick: Some(now),
            alerts,
        }
    }

    #[test]
    fn a_failed_run_is_the_answer_with_its_retry() {
        let now = 60 * MINUTE + 20_000;
        let failed = run("failed", 60 * MINUTE, 60 * MINUTE, Some(60 * MINUTE));
        let watch = read(Some(failed.clone()), 1, now);
        let answer = answer(&watch, std::slice::from_ref(&failed), now);
        assert_eq!(answer.id(), "failed");
        assert_eq!(
            answer,
            Answer::Failed {
                run: failed,
                failures_in_a_row: 1,
                next: 120 * MINUTE,
            }
        );
        assert_eq!(answer.restates(), None);
    }

    #[test]
    fn paused_counts_the_due_times_skipped_since() {
        let now = 200 * MINUTE;
        let mut watch = read(None, 0, now);
        watch.schedules[0].state.as_mut().unwrap().paused = true;
        let recent = [skip("paused", 180 * MINUTE), skip("paused", 120 * MINUTE)];
        assert_eq!(
            answer(&watch, &recent, now),
            Answer::Paused {
                skipped: Some(Skips {
                    count: 2,
                    since: 120 * MINUTE
                }),
                last_run: None,
            }
        );
    }

    #[test]
    fn ticks_stopping_comes_before_the_last_run_and_is_restated() {
        let failed = run("failed", 60 * MINUTE, 60 * MINUTE, Some(60 * MINUTE));
        let mut watch = read(Some(failed.clone()), 3, 60 * MINUTE);
        let now = 125 * MINUTE;
        watch.alerts = alerts(&watch.schedules, Some(60 * MINUTE), 60_000, now);
        let answer = answer(&watch, &[failed], now);
        assert_eq!(
            answer,
            Answer::TicksStopped {
                since: 60 * MINUTE,
                overdue: Some(120 * MINUTE),
                tick_every_ms: 60_000,
            }
        );
        let why = Why {
            answer,
            requested: false,
            alerts: watch.alerts.clone(),
        };
        let others: Vec<&str> = why.other_alerts().map(Alert::id).collect();
        assert_eq!(others, ["failed_in_a_row"]);
    }

    #[test]
    fn a_running_run_a_lost_one_and_a_schedule_never_due() {
        let now = 125 * MINUTE;
        let running = run("running", 120 * MINUTE, 120 * MINUTE, None);
        let mut watch = read(Some(running.clone()), 0, now);
        {
            let state = watch.schedules[0].state.as_mut().unwrap();
            state.run_id = Some([7; 16]);
            state.lease_until = Some(137 * MINUTE);
            state.next_run_at = 180 * MINUTE;
        }
        assert_eq!(
            answer(&watch, &[running.clone()], now),
            Answer::Running {
                started: 120 * MINUTE,
                lease_until: 137 * MINUTE,
                skipped: 0
            }
        );
        let later = 140 * MINUTE;
        assert_eq!(
            answer(&watch, &[running], later),
            Answer::LeaseExpired {
                started: Some(120 * MINUTE),
                until: 137 * MINUTE,
                next: 180 * MINUTE,
            }
        );
        let fresh = read(None, 0, MINUTE);
        assert_eq!(
            answer(&fresh, &[], MINUTE),
            Answer::NotDue {
                first_due: 120 * MINUTE
            }
        );
        let mut unseen = fresh.clone();
        unseen.schedules[0].state = None;
        assert_eq!(answer(&unseen, &[], MINUTE), Answer::NotSeen);
    }
}
