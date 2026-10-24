//! The outbox's circuit breaker (area 12, question 18; chunk 7.2b): per
//! connection, after a run of deliveries whose call through it found its
//! provider unavailable, the deliveries that call it are held rather than
//! attempted; after a pause one of them is tried, the probe, and its
//! success releases the rest, while its failure holds them for a longer
//! pause. A held delivery keeps its retry budget: holding it gives back
//! the attempt its claim counted, and the time it is held doesn't count
//! towards its `within:`.
//!
//! - **Its state** is a row of `jadpo_circuit_breakers` in the store whose
//!   deliveries it holds, so it holds across processes and on Workers,
//!   every change one conditional statement.
//! - **What counts.** A delivery attempt counts once per connection its
//!   run called through, by how the last such call ended, and only a call
//!   to the connection's own driver ([`Source::Live`]): one that found the
//!   provider unavailable (it surely did nothing, or no answer came) is a
//!   failure in the run; one the provider answered, refusals included,
//!   ends the run. Calls made outside deliveries, a route's, don't count.
//!   The connection's own `retry:` resends are calls of the same attempt:
//!   it counts once, by how the last of them ended.
//! - **Which deliveries.** A subscriber's, when its action calls the
//!   connection, itself or through an action it calls (the plan's
//!   [`SubscriberPlan::calls_through`]), or when the capability it hands
//!   its event to fills it. It is held at its claim, before it runs.
//! - **The probe.** Once the pause has passed, the first delivery claimed
//!   for the connection takes the probe, its claim's lease bounding it;
//!   the tick's sweep wakes the longest-held delivery as one ([`WAKE`]),
//!   so a probe goes even when nothing new arrives. A probe whose run
//!   doesn't call the connection gives the probe back.
//! - **The alert** is raised by the tick, once per outage, recorded on
//!   the breaker's row as a schedule's are on its state (chunk 6.9).
//!
//! Chosen where the pages leave it open (area 12, "Chosen in chunk
//! 7.2b"): [`OPEN_AFTER`] failures in a row, a first pause of
//! [`PAUSE_MS`] doubling after each failed probe up to [`PAUSE_CAP_MS`].
//!
//! [`Source::Live`]: crate::capability::Source::Live
//! [`SubscriberPlan::calls_through`]: jadpo_std::subscriber::SubscriberPlan::calls_through

use std::collections::BTreeMap;

use crate::budget::Budget;
use crate::capability::{Ended, Logged, Source};
use crate::outbox::micros;
use crate::{Database, Host, SqlType, SqlValue, StoreError};

/// How many delivery attempts in a row must find a provider unavailable
/// before its connection's deliveries are held.
pub const OPEN_AFTER: u32 = 5;
/// How long deliveries are held before the first probe.
pub const PAUSE_MS: i64 = 60_000;
/// The longest pause: each failed probe doubles it up to this.
pub const PAUSE_CAP_MS: i64 = 10 * 60_000;

/// A breaker's states.
pub const CLOSED: &str = "closed";
pub const OPEN: &str = "open";
pub const PROBING: &str = "probing";

/// The state a delivery is left in while a breaker holds it.
pub const HELD: &str = "held";

/// The breakers of connections `$1`.
pub const READ: &str = "SELECT \"connection\", \"state\", \"failures\", \"probe_at\", \"probe_until\", \"pauses\" FROM \"jadpo_circuit_breakers\" WHERE \"connection\" = ANY($1::text[])";

/// Makes delivery `$2` the probe of connection `$1`'s breaker at `$4`,
/// its claim lasting until `$3`: once its pause has passed, or the last
/// probe's claim has. The pauses so far.
pub const TAKE_PROBE: &str = "UPDATE \"jadpo_circuit_breakers\" SET \"state\" = 'probing', \"probe_id\" = $2, \"probe_until\" = $3 WHERE \"connection\" = $1 AND ((\"state\" = 'open' AND \"probe_at\" <= $4) OR (\"state\" = 'probing' AND (\"probe_until\" IS NULL OR \"probe_until\" <= $4))) RETURNING \"pauses\"";

/// Holds attempt `$2` of delivery `$1`, while it is still the claimer's,
/// for connection `$3` from `$4`, while its breaker isn't closed: `held`,
/// not due, the attempt its claim counted given back.
pub const HOLD: &str = "UPDATE \"jadpo_deliveries\" SET \"status\" = 'held', \"attempts\" = \"attempts\" - 1, \"next_attempt_at\" = NULL, \"held_for\" = $3, \"held_since\" = $4 WHERE \"id\" = $1 AND \"status\" = 'running' AND \"attempts\" = $2 AND EXISTS (SELECT 1 FROM \"jadpo_circuit_breakers\" AS \"b\" WHERE \"b\".\"connection\" = $3 AND \"b\".\"state\" <> 'closed') RETURNING \"id\"";

/// Counts a delivery attempt at `$2` that found connection `$1`'s
/// provider unavailable: one more in the run, opening the breaker, its
/// probe due at `$4`, once the run reaches `$3`; or, when delivery `$5`
/// was its probe, opening it again with its probe due at `$6`. Its state
/// and the run's length.
pub const FAILED: &str = "INSERT INTO \"jadpo_circuit_breakers\" AS \"b\" (\"connection\", \"state\", \"failures\", \"opened_at\", \"pauses\", \"probe_at\", \"probe_id\", \"probe_until\", \"alerted_at\") VALUES ($1, 'closed', 1, NULL, 0, NULL, NULL, NULL, NULL) ON CONFLICT (\"connection\") DO UPDATE SET \"failures\" = \"b\".\"failures\" + 1, \"state\" = CASE WHEN \"b\".\"state\" = 'closed' AND \"b\".\"failures\" + 1 >= $3 THEN 'open' WHEN \"b\".\"state\" = 'probing' AND \"b\".\"probe_id\" = $5::uuid THEN 'open' ELSE \"b\".\"state\" END, \"opened_at\" = CASE WHEN \"b\".\"state\" = 'closed' AND \"b\".\"failures\" + 1 >= $3 THEN $2 ELSE \"b\".\"opened_at\" END, \"pauses\" = CASE WHEN \"b\".\"state\" = 'closed' AND \"b\".\"failures\" + 1 >= $3 THEN 1 WHEN \"b\".\"state\" = 'probing' AND \"b\".\"probe_id\" = $5::uuid THEN \"b\".\"pauses\" + 1 ELSE \"b\".\"pauses\" END, \"probe_at\" = CASE WHEN \"b\".\"state\" = 'closed' AND \"b\".\"failures\" + 1 >= $3 THEN $4 WHEN \"b\".\"state\" = 'probing' AND \"b\".\"probe_id\" = $5::uuid THEN $6 ELSE \"b\".\"probe_at\" END, \"probe_id\" = CASE WHEN \"b\".\"state\" = 'probing' AND \"b\".\"probe_id\" = $5::uuid THEN NULL ELSE \"b\".\"probe_id\" END, \"probe_until\" = CASE WHEN \"b\".\"state\" = 'probing' AND \"b\".\"probe_id\" = $5::uuid THEN NULL ELSE \"b\".\"probe_until\" END RETURNING \"state\", \"failures\"";

/// How long a delivery released at `$2` was held, added to what it was
/// held before: what its `within:` doesn't count.
const HELD_MS: &str = "COALESCE(\"held_ms\", 0) + GREATEST(0, (extract(epoch FROM ($2::timestamptz - COALESCE(\"held_since\", $2::timestamptz))) * 1000)::bigint)";

/// Closes connection `$1`'s breaker at `$2` after a call its provider
/// answered, ending the run, and releases every delivery it held:
/// `pending`, due at once. How many breakers it closed, and how many
/// deliveries it released.
pub fn succeeded_sql() -> String {
    format!(
        "WITH \"b\" AS (UPDATE \"jadpo_circuit_breakers\" SET \"state\" = 'closed', \"failures\" = 0, \"opened_at\" = NULL, \"pauses\" = 0, \"probe_at\" = NULL, \"probe_id\" = NULL, \"probe_until\" = NULL, \"alerted_at\" = NULL WHERE \"connection\" = $1 AND (\"state\" <> 'closed' OR \"failures\" > 0) RETURNING \"connection\"), \"d\" AS (UPDATE \"jadpo_deliveries\" SET \"status\" = 'pending', \"next_attempt_at\" = $2, \"held_ms\" = {HELD_MS}, \"held_for\" = NULL, \"held_since\" = NULL WHERE \"status\" = 'held' AND \"held_for\" IN (SELECT \"connection\" FROM \"b\") RETURNING \"id\") SELECT (SELECT count(*) FROM \"b\"), (SELECT count(*) FROM \"d\")"
    )
}

/// Gives back the probe delivery `$2` took of connection `$1`'s breaker,
/// whose run didn't call it: open again, its pause passed, for the next.
pub const PROBE_UNUSED: &str = "UPDATE \"jadpo_circuit_breakers\" SET \"state\" = 'open', \"probe_id\" = NULL, \"probe_until\" = NULL WHERE \"connection\" = $1 AND \"state\" = 'probing' AND \"probe_id\" = $2 RETURNING \"connection\"";

/// What the sweep releases at `$1`, before it reads what is due: each
/// delivery held by a breaker that is closed, or that no longer has a
/// row, as a release racing a hold leaves one; and, for each breaker
/// whose pause has passed, or whose probe's claim has, with no probe
/// running, the delivery held longest, to be the probe. `pending`, due at
/// once.
pub fn wake_sql() -> String {
    format!(
        "UPDATE \"jadpo_deliveries\" AS \"d\" SET \"status\" = 'pending', \"next_attempt_at\" = $1, \"held_ms\" = {}, \"held_for\" = NULL, \"held_since\" = NULL WHERE \"d\".\"status\" = 'held' AND (NOT EXISTS (SELECT 1 FROM \"jadpo_circuit_breakers\" AS \"b\" WHERE \"b\".\"connection\" = \"d\".\"held_for\" AND \"b\".\"state\" <> 'closed') OR \"d\".\"id\" IN (SELECT DISTINCT ON (\"h\".\"held_for\") \"h\".\"id\" FROM \"jadpo_deliveries\" AS \"h\" JOIN \"jadpo_circuit_breakers\" AS \"b\" ON \"b\".\"connection\" = \"h\".\"held_for\" WHERE \"h\".\"status\" = 'held' AND ((\"b\".\"state\" = 'open' AND \"b\".\"probe_at\" <= $1) OR (\"b\".\"state\" = 'probing' AND (\"b\".\"probe_until\" IS NULL OR \"b\".\"probe_until\" <= $1))) ORDER BY \"h\".\"held_for\", \"h\".\"held_since\", \"h\".\"id\")) RETURNING \"d\".\"id\"",
        HELD_MS
            .replace("$2", "$1")
            .replace("\"held_", "\"d\".\"held_")
    )
}

/// Marks each breaker that isn't closed and hasn't been alerted on as
/// alerted at `$1`: the alerts this statement raises, each with its
/// connection, when it opened and how many deliveries it holds.
pub const RAISE: &str = "UPDATE \"jadpo_circuit_breakers\" AS \"b\" SET \"alerted_at\" = $1 WHERE \"b\".\"state\" <> 'closed' AND \"b\".\"alerted_at\" IS NULL RETURNING \"b\".\"connection\", \"b\".\"opened_at\", (SELECT count(*) FROM \"jadpo_deliveries\" AS \"d\" WHERE \"d\".\"status\" = 'held' AND \"d\".\"held_for\" = \"b\".\"connection\")";

/// Every breaker that isn't closed, by connection: its state, when it
/// opened, its next probe and how many deliveries it holds.
pub const UNAVAILABLE: &str = "SELECT \"b\".\"connection\", \"b\".\"state\", \"b\".\"opened_at\", \"b\".\"probe_at\", (SELECT count(*) FROM \"jadpo_deliveries\" AS \"d\" WHERE \"d\".\"status\" = 'held' AND \"d\".\"held_for\" = \"b\".\"connection\") FROM \"jadpo_circuit_breakers\" AS \"b\" WHERE \"b\".\"state\" <> 'closed' ORDER BY \"b\".\"connection\"";

/// The pause after the probe that ended pause number `pauses` failed:
/// [`PAUSE_MS`] doubled for each pause so far, at most [`PAUSE_CAP_MS`].
pub fn pause_after(pauses: u32) -> i64 {
    PAUSE_MS
        .saturating_mul(1_i64 << pauses.min(20))
        .min(PAUSE_CAP_MS)
}

/// How a delivery attempt's calls through one connection went.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Called {
    /// Its provider answered, a refusal included.
    Answered,
    /// It found its provider unavailable: the call surely did nothing, or
    /// no answer came back.
    Unavailable,
}

/// How each connection the calls in `calls` went through went, by the
/// last call through it that reached its own driver and ended; calls a
/// fake or a recording answered, or still out, or refused before they
/// left, say nothing about a provider.
pub fn called(calls: &[Logged]) -> BTreeMap<String, Called> {
    let mut out = BTreeMap::new();
    for call in calls.iter().filter(|call| call.source == Source::Live) {
        let went = match call.ended {
            Ended::Unavailable | Ended::Unknown => Called::Unavailable,
            Ended::Answered | Ended::Failed(_) | Ended::Misconfigured => Called::Answered,
            Ended::InFlight | Ended::Refused => continue,
        };
        out.insert(call.connection.clone(), went);
    }
    out
}

/// What [`gate_snapshot`] decided for a delivery.
#[derive(Debug, Default)]
pub(crate) struct Watch {
    /// The connections it is the probe of, with the pauses so far.
    probes: Vec<(String, u32)>,
}

/// Whether a claimed delivery runs or is held.
#[derive(Debug)]
pub(crate) enum Gate {
    Run(Watch),
    /// Held, its attempt given back: not run.
    Held,
}

/// One breaker's row, as [`gate_snapshot`] reads it.
struct Row {
    connection: String,
    state: String,
    probe_at: Option<i64>,
    probe_until: Option<i64>,
}

fn millis(value: Option<&SqlValue>) -> Option<i64> {
    match value {
        Some(SqlValue::Timestamp(micros)) => Some(micros.div_euclid(1000)),
        _ => None,
    }
}

/// Decides from the breaker's snapshot returned by the delivery claim
/// whether it runs. The snapshot never grants a probe or holds a delivery:
/// those changes still check the current breaker in conditional statements.
pub(crate) async fn gate_snapshot(
    database: &dyn Database,
    rows: &[crate::Row],
    id: [u8; 16],
    attempt: u32,
    budget: &Budget<'_>,
    now: i64,
) -> Gate {
    let mut watch = Watch::default();
    let rows: Vec<Row> = rows
        .iter()
        .filter_map(|row| match (row.first(), row.get(1)) {
            (Some(SqlValue::Text(connection)), Some(SqlValue::Text(state))) => Some(Row {
                connection: connection.clone(),
                state: state.clone(),
                probe_at: millis(row.get(3)),
                probe_until: millis(row.get(4)),
            }),
            _ => None,
        })
        .collect();
    let mut hold: Option<&str> = None;
    let mut due = Vec::new();
    for row in &rows {
        match row.state.as_str() {
            OPEN if row.probe_at.is_none_or(|at| at <= now) => due.push(row),
            PROBING if row.probe_until.is_none_or(|until| until <= now) => due.push(row),
            OPEN | PROBING => hold = hold.or(Some(row.connection.as_str())),
            _ => {}
        }
    }
    if let Some(connection) = hold {
        if held(database, id, attempt, connection, now).await {
            return Gate::Held;
        }
        // Its breaker closed meanwhile: the delivery runs.
        return Gate::Run(watch);
    }
    let until = crate::lease::until(budget, now);
    for row in due {
        let taken = database
            .query_once(
                TAKE_PROBE,
                &[
                    SqlValue::Text(row.connection.clone()),
                    SqlValue::Uuid(id),
                    micros(until),
                    micros(now),
                ],
            )
            .await;
        match taken.ok().and_then(|rows| rows.into_iter().next()) {
            Some(taken) => {
                let pauses = match taken.first() {
                    Some(SqlValue::Int4(n)) => u32::try_from(*n).unwrap_or(0),
                    _ => 0,
                };
                watch.probes.push((row.connection.clone(), pauses));
            }
            // Another delivery took the probe first: this one waits.
            None => {
                if held(database, id, attempt, &row.connection, now).await {
                    for (connection, _) in &watch.probes {
                        give_back(database, connection, id).await;
                    }
                    return Gate::Held;
                }
            }
        }
    }
    Gate::Run(watch)
}

/// Holds attempt `attempt` of delivery `id` for `connection` at `now`:
/// whether it was held, its breaker still open.
async fn held(
    database: &dyn Database,
    id: [u8; 16],
    attempt: u32,
    connection: &str,
    now: i64,
) -> bool {
    database
        .query_once(
            HOLD,
            &[
                SqlValue::Uuid(id),
                SqlValue::Int4(i32::try_from(attempt).unwrap_or(i32::MAX)),
                SqlValue::Text(connection.to_string()),
                micros(now),
            ],
        )
        .await
        .is_ok_and(|rows| !rows.is_empty())
}

async fn give_back(database: &dyn Database, connection: &str, id: [u8; 16]) {
    let _ = database
        .query_once(
            PROBE_UNUSED,
            &[SqlValue::Text(connection.to_string()), SqlValue::Uuid(id)],
        )
        .await;
}

/// Records at `now` how delivery `id`'s attempt, run under `watch`, went
/// through each connection its `calls` went through: a failure in the
/// run, or an answer that ends it, closing a breaker and releasing what
/// it held; a probe whose run didn't call its connection gives it back.
/// What can't be written is left: the next attempt counts again.
pub(crate) async fn record(
    database: &dyn Database,
    watch: &Watch,
    id: [u8; 16],
    calls: &[Logged],
    now: i64,
) {
    let went = called(calls);
    for (connection, called) in &went {
        let probe = watch
            .probes
            .iter()
            .find(|(probing, _)| probing == connection);
        match called {
            Called::Unavailable => {
                let reopen = now.saturating_add(pause_after(probe.map_or(0, |(_, n)| *n)));
                let _ = database
                    .query_once(
                        FAILED,
                        &[
                            SqlValue::Text(connection.clone()),
                            micros(now),
                            SqlValue::Int4(i32::try_from(OPEN_AFTER).unwrap_or(i32::MAX)),
                            micros(now.saturating_add(PAUSE_MS)),
                            probe.map_or(SqlValue::Null(SqlType::Uuid), |_| SqlValue::Uuid(id)),
                            micros(reopen),
                        ],
                    )
                    .await;
            }
            // Whatever its breaker was at the claim: one that opened while
            // the delivery ran closes on its answer too (area 12; chunk
            // 7.9c). A closed breaker with no run is left as it is.
            Called::Answered => {
                let _ = database
                    .query_once(
                        &succeeded_sql(),
                        &[SqlValue::Text(connection.clone()), micros(now)],
                    )
                    .await;
            }
        }
    }
    for (connection, _) in &watch.probes {
        if !went.contains_key(connection) {
            give_back(database, connection, id).await;
        }
    }
}

/// Releases at `now` in `database` what [`wake_sql`] wakes: deliveries a
/// closed breaker still holds, and each due probe.
pub(crate) async fn wake(database: &dyn Database, now: i64) -> Result<(), StoreError> {
    database.query_once(&wake_sql(), &[micros(now)]).await?;
    Ok(())
}

/// A connection whose breaker isn't closed, as an operator is shown it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unavailable {
    pub connection: String,
    /// Whether a probe is being tried now.
    pub probing: bool,
    /// When it opened.
    pub since: Option<i64>,
    /// When the next probe may go.
    pub probe_at: Option<i64>,
    /// How many deliveries it holds.
    pub held: u64,
}

/// Every connection in `database` whose breaker isn't closed.
pub async fn unavailable(database: &dyn Database) -> Result<Vec<Unavailable>, StoreError> {
    let rows = database.query_once(UNAVAILABLE, &[]).await?;
    Ok(rows
        .iter()
        .filter_map(|row| {
            let (Some(SqlValue::Text(connection)), Some(SqlValue::Text(state))) =
                (row.first(), row.get(1))
            else {
                return None;
            };
            Some(Unavailable {
                connection: connection.clone(),
                probing: state == PROBING,
                since: millis(row.get(2)),
                probe_at: millis(row.get(3)),
                held: count(row.get(4)),
            })
        })
        .collect())
}

fn count(value: Option<&SqlValue>) -> u64 {
    match value {
        Some(SqlValue::Int8(n)) => u64::try_from(*n).unwrap_or(0),
        Some(SqlValue::Int4(n)) => u64::try_from(*n).unwrap_or(0),
        _ => 0,
    }
}

/// Raises at `now` the alerts of `store`'s breakers that have opened
/// since a tick last looked, which no other instance's tick raised first
/// ([`RAISE`]): one per outage, until the breaker closes.
pub(crate) async fn raise(
    host: &dyn Host,
    store: &str,
    now: i64,
) -> Result<Vec<crate::schedules::watch::Alert>, StoreError> {
    use crate::schedules::watch::{Alert, AlertKind};
    let database = host
        .database(store)
        .ok_or_else(|| StoreError::Fault(format!("no store called `{store}`")))?;
    let rows = database.query_once(RAISE, &[micros(now)]).await?;
    Ok(rows
        .iter()
        .filter_map(|row| match row.first() {
            Some(SqlValue::Text(connection)) => Some(Alert {
                schedule: None,
                kind: AlertKind::ConnectionUnavailable {
                    connection: connection.clone(),
                    since: millis(row.get(1)).unwrap_or(now),
                    held: count(row.get(2)),
                },
            }),
            _ => None,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::Repeat;

    fn logged(connection: &str, source: Source, ended: Ended) -> Logged {
        Logged {
            connection: connection.to_string(),
            operation: "parcels.get".to_string(),
            repeat: Repeat::Safe,
            source,
            at: 0,
            took_ms: None,
            ended,
            step: None,
        }
    }

    #[test]
    fn each_pause_after_a_failed_probe_doubles_up_to_ten_minutes() {
        assert_eq!(pause_after(0), PAUSE_MS);
        assert_eq!(pause_after(1), 120_000);
        assert_eq!(pause_after(2), 240_000);
        assert_eq!(pause_after(3), 480_000);
        assert_eq!(pause_after(4), PAUSE_CAP_MS);
        assert_eq!(pause_after(u32::MAX), PAUSE_CAP_MS);
    }

    #[test]
    fn the_last_live_call_through_a_connection_says_how_it_went() {
        let calls = [
            logged("couriers", Source::Live, Ended::Unavailable),
            logged("couriers", Source::Live, Ended::Answered),
            logged("ledger", Source::Live, Ended::Failed("refused".into())),
            logged("ledger", Source::Live, Ended::Unknown),
            logged("mail", Source::Fake, Ended::Unavailable),
            logged("files", Source::Live, Ended::Refused),
            logged("maps", Source::Live, Ended::InFlight),
            logged("tax", Source::Live, Ended::Misconfigured),
        ];
        let went = called(&calls);
        assert_eq!(went.get("couriers"), Some(&Called::Answered));
        assert_eq!(went.get("ledger"), Some(&Called::Unavailable));
        assert_eq!(went.get("tax"), Some(&Called::Answered));
        // A fake's answer, a refused call and one still out say nothing.
        assert_eq!(went.get("mail"), None);
        assert_eq!(went.get("files"), None);
        assert_eq!(went.get("maps"), None);
    }

    /// A store that keeps each statement run on its own and answers none.
    #[derive(Default)]
    struct Statements(std::cell::RefCell<Vec<String>>);

    impl Database for Statements {
        fn begin_at(
            &self,
            _isolation: crate::Isolation,
        ) -> crate::LocalFuture<'_, Result<Box<dyn crate::Transaction>, StoreError>> {
            Box::pin(async { Err(StoreError::Unavailable) })
        }

        fn query_once<'a>(
            &'a self,
            sql: &'a str,
            _params: &'a [SqlValue],
        ) -> crate::LocalFuture<'a, Result<Vec<crate::Row>, StoreError>> {
            self.0.borrow_mut().push(sql.to_string());
            Box::pin(async { Ok(Vec::new()) })
        }
    }

    /// The claim already read every relevant breaker; a closed/absent
    /// snapshot runs without another database round trip.
    #[test]
    fn a_claimed_breaker_snapshot_needs_no_separate_read() {
        let store = Statements::default();
        let host = crate::MemoryHost::new();
        let budget = Budget::start(&host, crate::budget::Invocation::Tick);
        let rows = vec![vec![
            SqlValue::Text("couriers".into()),
            SqlValue::Text(CLOSED.into()),
            SqlValue::Int4(0),
        ]];
        assert!(matches!(
            crate::block_on(gate_snapshot(&store, &rows, [1; 16], 1, &budget, 0)),
            Ok(Gate::Run(_))
        ));
        assert!(store.0.borrow().is_empty());
    }

    struct Script(std::cell::RefCell<std::collections::VecDeque<(&'static str, Vec<crate::Row>)>>);

    impl Database for Script {
        fn begin_at(
            &self,
            _: crate::Isolation,
        ) -> crate::LocalFuture<'_, Result<Box<dyn crate::Transaction>, StoreError>> {
            Box::pin(async { panic!("breaker changes are conditional statements") })
        }
        fn query_once<'a>(
            &'a self,
            sql: &'a str,
            params: &'a [SqlValue],
        ) -> crate::LocalFuture<'a, Result<Vec<crate::Row>, StoreError>> {
            Box::pin(async move {
                let (expected, rows) = self
                    .0
                    .borrow_mut()
                    .pop_front()
                    .expect("unexpected statement");
                assert_eq!(sql, expected, "{params:?}");
                Ok(rows)
            })
        }
    }

    fn snapshot(connection: &str, state: &str, at: Option<i64>) -> crate::Row {
        vec![
            SqlValue::Text(connection.into()),
            SqlValue::Text(state.into()),
            SqlValue::Int4(5),
            at.map_or(SqlValue::Null(SqlType::Timestamp), micros),
            at.map_or(SqlValue::Null(SqlType::Timestamp), micros),
            SqlValue::Int4(1),
        ]
    }

    #[test]
    fn a_later_connection_can_hold_the_whole_delivery_before_any_probe_is_taken() {
        let host = crate::MemoryHost::new();
        let budget = Budget::start(&host, crate::budget::Invocation::Tick);
        let store = Script(std::cell::RefCell::new(
            [(HOLD, vec![vec![SqlValue::Uuid([1; 16])]])].into(),
        ));
        let rows = [
            snapshot("a", OPEN, Some(0)),
            snapshot("b", OPEN, Some(1000)),
        ];
        assert!(matches!(
            crate::block_on(gate_snapshot(&store, &rows, [1; 16], 1, &budget, 1)),
            Ok(Gate::Held)
        ));
        assert!(store.0.borrow().is_empty());
    }

    #[test]
    fn losing_a_later_probe_returns_every_earlier_probe() {
        let host = crate::MemoryHost::new();
        let budget = Budget::start(&host, crate::budget::Invocation::Tick);
        let store = Script(std::cell::RefCell::new(
            [
                (TAKE_PROBE, vec![vec![SqlValue::Int4(1)]]),
                (TAKE_PROBE, vec![vec![SqlValue::Int4(2)]]),
                (TAKE_PROBE, vec![]),
                (HOLD, vec![vec![SqlValue::Uuid([1; 16])]]),
                (PROBE_UNUSED, vec![]),
                (PROBE_UNUSED, vec![]),
            ]
            .into(),
        ));
        let rows = [
            snapshot("a", OPEN, Some(0)),
            snapshot("b", PROBING, Some(0)),
            snapshot("c", OPEN, Some(0)),
        ];
        assert!(matches!(
            crate::block_on(gate_snapshot(&store, &rows, [1; 16], 1, &budget, 1)),
            Ok(Gate::Held)
        ));
        assert!(store.0.borrow().is_empty());
    }

    #[test]
    fn a_snapshot_does_not_override_a_breaker_that_closed_before_the_hold() {
        let host = crate::MemoryHost::new();
        let budget = Budget::start(&host, crate::budget::Invocation::Tick);
        let store = Script(std::cell::RefCell::new([(HOLD, vec![])].into()));
        let rows = [
            snapshot("a", OPEN, Some(1000)),
            vec![SqlValue::Null(SqlType::Text)],
        ];
        assert!(matches!(
            crate::block_on(gate_snapshot(&store, &rows, [1; 16], 1, &budget, 1)),
            Ok(Gate::Run(_))
        ));
        assert!(store.0.borrow().is_empty());
    }

    /// A delivery claimed while its connection's breaker was closed with
    /// no failures, which the breaker opened on while it ran, closes it
    /// when its provider answers, releasing what it held (area 12, "A
    /// delivery already running when the breaker opened finishes, and its
    /// answer closes the breaker too"; Phase 7 review, chunk 7.9c).
    #[test]
    fn an_answer_to_a_delivery_claimed_before_the_breaker_opened_closes_it() {
        let store = Statements::default();
        let calls = [logged("couriers", Source::Live, Ended::Answered)];
        crate::block_on(record(&store, &Watch::default(), [1; 16], &calls, 0))
            .expect("nothing waits");
        assert_eq!(store.0.borrow().as_slice(), [succeeded_sql()]);
    }

    #[test]
    fn waking_and_releasing_count_the_time_held_from_their_own_parameter() {
        assert!(succeeded_sql().contains("$2::timestamptz - COALESCE(\"held_since\""));
        let wake = wake_sql();
        assert!(
            wake.contains("$1::timestamptz - COALESCE(\"d\".\"held_since\", $1::timestamptz)"),
            "{wake}"
        );
        assert!(!wake.contains("$2"), "{wake}");
    }
}
