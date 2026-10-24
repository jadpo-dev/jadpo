//! The outbox and its deliveries (area 12, "Emitting an event",
//! "Delivery: at least once, safe to repeat" and "Retries and giving up";
//! chunk 6.1b): what the runtime keeps of an event, in the store the
//! emitting action writes, and of each delivery of it to a subscriber.
//!
//! - An action's events are written to `jadpo_outbox` in the same
//!   transaction as its writes, as it commits ([`crate::boundary`]), each
//!   with a `jadpo_deliveries` row per subscriber, `pending` and due at
//!   once; nothing is written for an action that rolls back.
//! - Once it commits, each delivery is left for the runner
//!   ([`crate::runner::Deferred::Deliveries`]), which claims its record
//!   (`running`, its lease what is left of the runner's budget and a
//!   minute, [`crate::lease`]; chunk 6.7), runs the subscriber's action as
//!   the system limited to the event's tenant, and records how it ended:
//!   `succeeded`; `pending` again with its next attempt after a backoff
//!   with full jitter, while its budget lasts; or `failed`.
//! - A delivery to a capability's operation (an email, chunk 7.3) hands
//!   the event to it through the capability seam ([`crate::effects::call`];
//!   chunk 7.0c). One whose outcome is unknown, and that sent again could
//!   act twice, is parked as `outcome_unknown` for an operator, never sent
//!   again by itself (area 12, "Retries and giving up"; area 16): an
//!   unkeyed send is marked `sending` before it goes out, so one whose
//!   worker was lost meanwhile is parked by [`crate::runner::sweep`] rather
//!   than claimed again. A subscriber's action that made such a call and
//!   then ended in a way that would be retried is parked the same way.
//! - A subscriber's action commits a handled record
//!   (`jadpo_handled_deliveries`, keyed by the delivery's `id`) in the
//!   store it writes, with its writes, so a repeated delivery finds it and
//!   does nothing (area 12, "Outbox placement, settled").
//! - What deferred work dropped (a crash between commit and delivery, a
//!   host with nothing to run it on, a worker that died holding a lease)
//!   stays `pending` or `running` past its time, and [`sweep`] picks it
//!   up; the once-a-minute tick that calls it is chunk 6.2's.
//!
//! The statements are the runtime's own, on its own system tables, as the
//! idempotency keys' are ([`crate::http::idempotency`]).

use jadpo_std::subscriber::{EventsPlan, SubscriberPlan, TenantFrom};
use jadpo_value::Value;

use crate::read::JsonCodecs;
use crate::{SqlType, SqlValue, StoreError, Transaction};

/// The most events one action emits (area 12, "Limits"): past it the
/// action fails as a defect and rolls back.
pub const MAX_EVENTS: usize = 1_000;
/// The largest one event's payload is, in bytes of its JSON.
pub const MAX_EVENT_BYTES: usize = 64 * 1024;
/// How deep events causing events go from one request before a delivery
/// fails with `event_chain_limit`, which stops a loop.
pub const CHAIN_LIMIT: u32 = 16;
/// How many due deliveries one [`sweep`] takes.
pub const SWEEP_BATCH: u32 = 100;
/// A retry's first wait's ceiling, doubled after each attempt up to
/// [`BACKOFF_CAP_MS`]; each wait is drawn evenly from zero to its
/// ceiling (full jitter), and is never under a second.
pub const BACKOFF_BASE_MS: i64 = 30_000;
pub const BACKOFF_CAP_MS: i64 = 6 * 60 * 60 * 1000;
/// The tenant a delivery whose event names none is limited to: an
/// identity no record has, so it reaches no tenant's records (area 12,
/// "Background work and tenants").
pub const NO_TENANT: [u8; 16] = [0; 16];

/// A delivery record's states (area 12, "What an operator sees").
pub const PENDING: &str = "pending";
pub const RUNNING: &str = "running";
pub const SUCCEEDED: &str = "succeeded";
pub const FAILED: &str = "failed";
/// Parked for an operator: an outside call may have acted, and sending it
/// again could act twice (chunk 7.0c).
pub const OUTCOME_UNKNOWN: &str = "outcome_unknown";
/// `running`, with its unkeyed call on its way out (chunk 7.0c): never
/// claimed again, and parked once its lease has passed.
pub const SENDING: &str = "sending";
/// Held for an operator: a rerun of its action, which makes several
/// changing calls, no longer matches the record of its steps (area 12,
/// "A rerun replays"; chunk 7.5b). Never claimed again by itself; an
/// operator's retry, once a fix is deployed, finishes it.
pub const DIVERGED: &str = "diverged";

/// The outbox rows of one commit: their ids, event keys and payloads
/// unnested, then the time, the trace context, the encoding and the
/// depth they share.
pub const INSERT_OUTBOX: &str = "INSERT INTO \"jadpo_outbox\" (\"id\", \"event\", \"payload\", \"emitted_at\", \"traceparent\", \"encoding\", \"depth\") SELECT \"o\".\"id\", \"o\".\"event\", \"o\".\"payload\"::jsonb, $4, $5, $6, $7 FROM unnest($1::uuid[], $2::text[], $3::text[]) AS \"o\"(\"id\", \"event\", \"payload\")";

/// The delivery records of one commit, each `pending` and due at `$4`.
pub const INSERT_DELIVERIES: &str = "INSERT INTO \"jadpo_deliveries\" (\"id\", \"outbox_id\", \"subscriber\", \"status\", \"attempts\", \"next_attempt_at\") SELECT \"d\".\"id\", \"d\".\"outbox_id\", \"d\".\"subscriber\", 'pending', 0, $4 FROM unnest($1::uuid[], $2::uuid[], $3::text[]) AS \"d\"(\"id\", \"outbox_id\", \"subscriber\")";

/// A subscriber's handled record, in its action's transaction.
pub const INSERT_HANDLED: &str =
    "INSERT INTO \"jadpo_handled_deliveries\" (\"id\", \"handled_at\") VALUES ($1, $2)";

/// Whether delivery `$1` was handled already.
pub const FIND_HANDLED: &str = "SELECT \"id\" FROM \"jadpo_handled_deliveries\" WHERE \"id\" = $1";

/// Claims delivery `$1` at `$2` until `$3` when it is due, giving its
/// attempt's number: one statement, so two runners never both have it.
/// One still `running` was lost with its worker once its lease passed:
/// its last failure becomes `$4`, [`LEASE_EXPIRED`] (chunk 6.6a).
pub const CLAIM: &str = "UPDATE \"jadpo_deliveries\" SET \"status\" = 'running', \"attempts\" = \"attempts\" + 1, \"last_tried_at\" = $2, \"next_attempt_at\" = $3, \"last_failure\" = CASE WHEN \"status\" = 'running' THEN $4::jsonb ELSE \"last_failure\" END WHERE \"id\" = $1 AND \"status\" IN ('pending', 'running') AND \"next_attempt_at\" <= $2 RETURNING \"attempts\"";

/// Gives up delivery `$1`, claimed for attempt `$2` over its budget or
/// past `within:`, without running it (chunk 6.6a): `failed`, the claim
/// not counted as an attempt, keeping its last attempt's failure, or `$3`
/// when it was never tried.
pub const GIVE_UP: &str = "UPDATE \"jadpo_deliveries\" SET \"status\" = 'failed', \"attempts\" = \"attempts\" - 1, \"next_attempt_at\" = NULL, \"last_failure\" = COALESCE(\"last_failure\", $3) WHERE \"id\" = $1 AND \"status\" = 'running' AND \"attempts\" = $2 RETURNING \"id\"";

/// The failure a delivery record keeps for an attempt lost with its
/// worker, as a schedule's run record does (chunk 6.2).
pub const LEASE_EXPIRED: &str = "lease_expired";

/// Records how attempt `$2` of delivery `$1` ended, while it is still the
/// claimer's, its call on its way out or not: its state `$3`, its next
/// attempt `$4` and its failure `$5`.
pub const SETTLE: &str = "UPDATE \"jadpo_deliveries\" SET \"status\" = $3, \"next_attempt_at\" = $4, \"last_failure\" = $5 WHERE \"id\" = $1 AND \"status\" IN ('running', 'sending') AND \"attempts\" = $2 RETURNING \"id\"";

/// Marks attempt `$2` of delivery `$1` as `sending` before its unkeyed
/// call goes out (chunk 7.0c), while it is still the claimer's.
pub const MARK_SENDING: &str = "UPDATE \"jadpo_deliveries\" SET \"status\" = 'sending' WHERE \"id\" = $1 AND \"status\" = 'running' AND \"attempts\" = $2 RETURNING \"id\"";

/// Parks every delivery still `sending` once its lease passed at `$1`, its
/// worker lost while its unkeyed call was out: `outcome_unknown`, with
/// failure `$2`, for an operator (chunk 7.0c).
pub const PARK_LOST: &str = "UPDATE \"jadpo_deliveries\" SET \"status\" = 'outcome_unknown', \"next_attempt_at\" = NULL, \"last_failure\" = $2 WHERE \"status\" = 'sending' AND \"next_attempt_at\" <= $1 RETURNING \"id\"";

/// The deliveries due at `$1`, oldest first, at most `$2`, with what
/// their outbox row keeps, the encoding its payload was written in last
/// (chunk 6.8). The time a delivery's `within:` counts from is its
/// event's, or an operator's retry's (chunk 6.12), moved on by the time a
/// circuit breaker held it (chunk 7.2b).
pub const DUE: &str = "SELECT \"d\".\"id\", \"d\".\"outbox_id\", \"d\".\"subscriber\", \"o\".\"event\", \"o\".\"payload\"::text, COALESCE(\"d\".\"retried_at\", \"o\".\"emitted_at\") + COALESCE(\"d\".\"held_ms\", 0) * interval '1 millisecond', \"o\".\"traceparent\", \"o\".\"depth\", \"o\".\"encoding\" FROM \"jadpo_deliveries\" AS \"d\" JOIN \"jadpo_outbox\" AS \"o\" ON \"o\".\"id\" = \"d\".\"outbox_id\" WHERE \"d\".\"status\" IN ('pending', 'running') AND \"d\".\"next_attempt_at\" <= $1 ORDER BY \"d\".\"next_attempt_at\", \"d\".\"id\" LIMIT $2";

/// The deliveries among `$1` still to run, with what their outbox row
/// keeps, read as [`DUE`] reads them: the deliveries an invocation handed
/// on to a fresh one by their records (chunk 8.2h,
/// [`crate::runner::carry_on`]). One run or given up since is left out.
pub const HANDED_ON: &str = "SELECT \"d\".\"id\", \"d\".\"outbox_id\", \"d\".\"subscriber\", \"o\".\"event\", \"o\".\"payload\"::text, COALESCE(\"d\".\"retried_at\", \"o\".\"emitted_at\") + COALESCE(\"d\".\"held_ms\", 0) * interval '1 millisecond', \"o\".\"traceparent\", \"o\".\"depth\", \"o\".\"encoding\" FROM \"jadpo_deliveries\" AS \"d\" JOIN \"jadpo_outbox\" AS \"o\" ON \"o\".\"id\" = \"d\".\"outbox_id\" WHERE \"d\".\"id\" = ANY($1::uuid[]) AND \"d\".\"status\" IN ('pending', 'running')";

/// Gives up delivery `$1` whose payload no longer reads, with its
/// problem `$2`: parked, never dropped (area 12, "A payload is the
/// contract's JSON").
pub const PARK: &str = "UPDATE \"jadpo_deliveries\" SET \"status\" = 'failed', \"next_attempt_at\" = NULL, \"last_failure\" = $2 WHERE \"id\" = $1 AND \"status\" IN ('pending', 'running') RETURNING \"id\"";

/// Gives back attempt `$2` of delivery `$1`, which a route that waits
/// for its follow-on work stopped waiting for while its subscriber's
/// action ran (area 7, `settle:`; chunk 6.3), or whose host began to
/// shut down while it ran (chunk 6.7): `pending` and due at `$3`, the
/// attempt not counted, for deferred work or the sweep to run it again.
pub const RELEASE: &str = "UPDATE \"jadpo_deliveries\" SET \"status\" = 'pending', \"attempts\" = \"attempts\" - 1, \"next_attempt_at\" = $3 WHERE \"id\" = $1 AND \"status\" = 'running' AND \"attempts\" = $2 RETURNING \"id\"";

/// Claims a delivery and reads all its connections' breakers in the same
/// statement. A missing breaker still returns the claim; a lost claim
/// returns no rows. Only the conditional claim changes delivery state.
pub fn claim_breakers_sql() -> String {
    format!(
        "WITH \"claimed\" AS ({CLAIM}) SELECT \"c\".\"attempts\", \"b\".\"connection\", \"b\".\"state\", \"b\".\"failures\", \"b\".\"probe_at\", \"b\".\"probe_until\", \"b\".\"pauses\" FROM \"claimed\" AS \"c\" LEFT JOIN \"jadpo_circuit_breakers\" AS \"b\" ON \"b\".\"connection\" = ANY($5::text[]) ORDER BY \"b\".\"connection\""
    )
}

/// A delivery's claim and the breaker snapshot that accompanied it.
pub(crate) struct Claimed {
    pub attempt: u32,
    pub breakers: Vec<crate::Row>,
}

/// Only deliveries that call a connection need its breaker snapshot.
/// Schedules and batches retain their shared lease operation, and a
/// connectionless delivery keeps its lean, original claim statement.
pub(crate) async fn claim(
    database: &dyn crate::Database,
    budget: &crate::budget::Budget<'_>,
    lease: &Leased,
    connections: &[String],
    now: i64,
) -> Result<Option<Claimed>, crate::StoreError> {
    use crate::lease::Leased as _;
    let (sql, mut params) = lease.claim(now, crate::lease::until(budget, now));
    let joined;
    let sql = if connections.is_empty() {
        sql
    } else {
        params.push(SqlValue::Array(
            SqlType::Text,
            connections.iter().cloned().map(SqlValue::Text).collect(),
        ));
        joined = claim_breakers_sql();
        &joined
    };
    let rows = database.query_once(sql, &params).await?;
    let Some(first) = rows.first() else {
        return Ok(None);
    };
    let attempt = match first.first() {
        Some(SqlValue::Int4(n)) => u32::try_from(*n).unwrap_or(u32::MAX),
        _ => return Ok(None),
    };
    Ok(Some(Claimed {
        attempt,
        breakers: if connections.is_empty() {
            Vec::new()
        } else {
            rows.into_iter()
                .map(|row| row.into_iter().skip(1).collect())
                .collect()
        },
    }))
}

/// A delivery's record as the runner leases it ([`crate::lease`]):
/// claimed for its next attempt, and given back with that attempt, its
/// number known once it is claimed, not counted.
pub(crate) struct Leased {
    pub id: [u8; 16],
    /// The attempt the claim is for: 0 until it is claimed.
    pub attempt: u32,
}

impl crate::lease::Leased for Leased {
    fn claim(&self, now: i64, until: i64) -> (&'static str, Vec<SqlValue>) {
        (
            CLAIM,
            vec![
                SqlValue::Uuid(self.id),
                micros(now),
                micros(until),
                failure_json(LEASE_EXPIRED, None),
            ],
        )
    }

    fn release(&self, now: i64) -> (&'static str, Vec<SqlValue>) {
        (
            RELEASE,
            vec![
                SqlValue::Uuid(self.id),
                SqlValue::Int4(i32::try_from(self.attempt).unwrap_or(i32::MAX)),
                micros(now),
            ],
        )
    }
}

/// A W3C trace context (`traceparent`): the trace a request starts or
/// continues, and the span of the work under way, which an emitted event
/// carries into each subscriber's run (area 12, question 17; area 16).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Trace {
    pub trace_id: [u8; 16],
    pub span_id: [u8; 8],
    pub sampled: bool,
}

impl Trace {
    /// A `traceparent` header's context: version `00`'s four fields (a
    /// later version's first four), lowercase hex, neither id all zeros.
    /// `None` for anything else, which starts a trace of its own.
    pub fn parse(header: &str) -> Option<Trace> {
        let mut parts = header.trim().split('-');
        let version = parts.next()?;
        let trace = parts.next()?;
        let span = parts.next()?;
        let flags = parts.next()?;
        if version.len() != 2 || version == "ff" || !hex(version) {
            return None;
        }
        if version == "00" && parts.next().is_some() {
            return None;
        }
        if trace.len() != 32 || span.len() != 16 || flags.len() != 2 {
            return None;
        }
        let trace_id: [u8; 16] = unhex(trace)?.try_into().ok()?;
        let span_id: [u8; 8] = unhex(span)?.try_into().ok()?;
        let flags = *unhex(flags)?.first()?;
        if trace_id == [0; 16] || span_id == [0; 8] {
            return None;
        }
        Some(Trace {
            trace_id,
            span_id,
            sampled: flags & 1 == 1,
        })
    }

    /// Its `traceparent` header.
    pub fn header(&self) -> String {
        format!(
            "00-{}-{}-{:02x}",
            to_hex(&self.trace_id),
            to_hex(&self.span_id),
            u8::from(self.sampled)
        )
    }

    /// A span of its trace under it, with id `span_id`.
    pub fn child(&self, span_id: [u8; 8]) -> Trace {
        Trace { span_id, ..*self }
    }

    /// A new trace from 24 random bytes: its trace id, then its span's.
    pub fn from_bytes(bytes: [u8; 24]) -> Trace {
        let mut trace_id = [0; 16];
        let mut span_id = [0; 8];
        trace_id.copy_from_slice(bytes.get(..16).unwrap_or(&[1; 16]));
        span_id.copy_from_slice(bytes.get(16..).unwrap_or(&[1; 8]));
        if trace_id == [0; 16] {
            trace_id[15] = 1;
        }
        if span_id == [0; 8] {
            span_id[7] = 1;
        }
        Trace {
            trace_id,
            span_id,
            sampled: true,
        }
    }
}

fn hex(text: &str) -> bool {
    text.bytes()
        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn unhex(text: &str) -> Option<Vec<u8>> {
    if !hex(text) || !text.len().is_multiple_of(2) {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(text.get(at..at + 2)?, 16).ok())
        .collect()
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// What the runner needs of a program to deliver its events: the
/// subscriber table, and the codecs a kept payload is read back with.
#[derive(Clone, Copy, Debug)]
pub struct Events<'a> {
    pub plan: &'a EventsPlan,
    pub json: JsonCodecs<'a>,
    /// Each event type's number in the JSON table, by identity key.
    pub numbers: &'a [jadpo_std::http::EventJson],
}

impl Events<'_> {
    /// The value payload `payload` of event type `event` holds, written
    /// in `encoding` as its outbox row records it, read with its type as
    /// it is now, its rules applied; why not, by rule and field, never
    /// the value. A payload of an encoding this release doesn't read, a
    /// later release's, is never guessed at (chunk 6.8).
    pub fn read(&self, event: &str, payload: &str, encoding: &SqlValue) -> Result<Value, String> {
        crate::read::kept_encoding(encoding)?;
        // Jadpo's own magic-link email, whose payload is the runtime's
        // (chunk 7.3b).
        if event == jadpo_std::email::MAGIC_LINK_EMAIL {
            return crate::http::magic_links::read_payload(payload);
        }
        // An event sign-in publishes, whose payload is the runtime's too
        // (chunk 7.3b4).
        if let Some(read) = crate::http::sign_in_events::read_payload(event, payload) {
            return read;
        }
        let number = self
            .numbers
            .iter()
            .find(|known| known.key == event)
            .map(|known| known.number)
            .ok_or_else(|| "an event type this build doesn't declare".to_string())?;
        self.json.read(number, payload)
    }
}

/// One delivery of an event to one subscriber, left for the runner once
/// the action that emitted it committed, or found due by [`sweep`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Delivery {
    /// The store its record and its event's outbox row are in.
    pub store: String,
    /// Its record's `id`, which a handled record is keyed by.
    pub id: [u8; 16],
    /// Its event's outbox row.
    pub outbox: [u8; 16],
    /// The subscriber's identity key.
    pub subscriber: String,
    /// The event type's identity key.
    pub event: String,
    pub value: Value,
    /// When the event was emitted, or an operator retried the delivery
    /// (chunk 6.12): a delivery is given up `within:` it.
    pub emitted_at: i64,
    /// The trace context of the action that emitted it.
    pub trace: Option<Trace>,
    /// How many events led to it from the request that set the chain off.
    pub depth: u32,
}

/// What one commit writes to the outbox: each event's row and each of its
/// deliveries, made with their ids.
pub(crate) struct Sealed {
    pub outbox: Vec<([u8; 16], String, String)>,
    pub deliveries: Vec<Delivery>,
}

/// Writes `sealed`'s rows in `transaction`: one statement for the events
/// and one for their deliveries.
pub(crate) async fn write(
    transaction: &mut dyn Transaction,
    sealed: &Sealed,
    emitted_at: i64,
    trace: Option<Trace>,
    depth: u32,
) -> Result<(), StoreError> {
    if sealed.outbox.is_empty() {
        return Ok(());
    }
    let ids = sealed
        .outbox
        .iter()
        .map(|(id, _, _)| SqlValue::Uuid(*id))
        .collect();
    let keys = sealed
        .outbox
        .iter()
        .map(|(_, key, _)| SqlValue::Text(key.clone()))
        .collect();
    let payloads = sealed
        .outbox
        .iter()
        .map(|(_, _, payload)| SqlValue::Text(payload.clone()))
        .collect();
    transaction
        .execute(
            INSERT_OUTBOX,
            &[
                SqlValue::Array(SqlType::Uuid, ids),
                SqlValue::Array(SqlType::Text, keys),
                SqlValue::Array(SqlType::Text, payloads),
                micros(emitted_at),
                trace.map_or(SqlValue::Null(SqlType::Text), |trace| {
                    SqlValue::Text(trace.header())
                }),
                crate::read::encoding_value(),
                SqlValue::Int4(i32::try_from(depth).unwrap_or(i32::MAX)),
            ],
        )
        .await?;
    if sealed.deliveries.is_empty() {
        return Ok(());
    }
    let ids = sealed
        .deliveries
        .iter()
        .map(|delivery| SqlValue::Uuid(delivery.id))
        .collect();
    let outboxes = sealed
        .deliveries
        .iter()
        .map(|delivery| SqlValue::Uuid(delivery.outbox))
        .collect();
    let subscribers = sealed
        .deliveries
        .iter()
        .map(|delivery| SqlValue::Text(delivery.subscriber.clone()))
        .collect();
    transaction
        .execute(
            INSERT_DELIVERIES,
            &[
                SqlValue::Array(SqlType::Uuid, ids),
                SqlValue::Array(SqlType::Uuid, outboxes),
                SqlValue::Array(SqlType::Text, subscribers),
                micros(emitted_at),
            ],
        )
        .await?;
    Ok(())
}

/// A time as a `timestamptz` parameter.
pub(crate) fn micros(millis: i64) -> SqlValue {
    SqlValue::Timestamp(millis.saturating_mul(1000))
}

/// What a delivery record keeps of how an attempt ended: the failure's
/// code and kind, or the fault's classification, never a value (area 4's
/// logging rules).
pub(crate) fn failure_json(code: &str, kind: Option<&str>) -> SqlValue {
    let mut object = serde_json::Map::new();
    object.insert("code".into(), code.into());
    if let Some(kind) = kind {
        object.insert("kind".into(), kind.into());
    }
    SqlValue::Json(serde_json::Value::Object(object).to_string())
}

/// What a delivery record keeps of an attempt parked as
/// `outcome_unknown`: that, and how the attempt itself ended, when it ended
/// otherwise (`deadline_exceeded`, `unavailable`).
pub(crate) fn parked_json(cause: Option<&str>) -> SqlValue {
    let mut object = serde_json::Map::new();
    object.insert("code".into(), OUTCOME_UNKNOWN.into());
    object.insert("kind".into(), OUTCOME_UNKNOWN.into());
    if let Some(cause) = cause.filter(|cause| *cause != OUTCOME_UNKNOWN) {
        object.insert("cause".into(), cause.into());
    }
    SqlValue::Json(serde_json::Value::Object(object).to_string())
}

/// What a delivery record keeps of an attempt held as `diverged`: that,
/// and what differed, the step, its operation and the fields, never a
/// value (chunk 7.5b).
pub(crate) fn diverged_json(problem: &str) -> SqlValue {
    let mut object = serde_json::Map::new();
    object.insert("code".into(), DIVERGED.into());
    object.insert("problem".into(), problem.into());
    SqlValue::Json(serde_json::Value::Object(object).to_string())
}

/// What happens to a delivery after an attempt (area 12, "Retries and
/// giving up").
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum After {
    Succeeded,
    /// Tried again later, while its budget lasts: a failure of kind
    /// `unavailable` or `rate_limited`, or a fault before anything
    /// committed (a store that couldn't be reached, a run out of time).
    Retry {
        code: String,
        kind: Option<String>,
    },
    /// Not tried again: any other declared failure, which the same input
    /// gives again, or a defect, until someone fixes the code.
    Fail {
        code: String,
        kind: Option<String>,
    },
    /// Parked for an operator, never tried again by itself: an outside
    /// call may have acted, and sending it again could act twice (area 12,
    /// `outcome_unknown`; chunk 7.0c). How the attempt ended, when that
    /// was otherwise.
    Park {
        cause: String,
    },
    /// Held for an operator, never tried again by itself: a rerun of an
    /// action with several changing calls no longer matches its record
    /// (chunk 7.5b), with what differed.
    Diverge {
        problem: String,
    },
}

impl After {
    /// What an attempt that ended as `self` leaves once a call its run made
    /// may have acted outside ([`crate::capability::Logged::may_have_acted`]):
    /// one that would be retried is parked instead, since a retry would
    /// send that call again (area 16: "never resent blindly").
    pub fn once_acted(self) -> After {
        match self {
            After::Retry { code, .. } => After::Park { cause: code },
            other => other,
        }
    }
}

/// How `outcome` leaves a delivery.
pub fn after(outcome: &crate::Outcome) -> After {
    use crate::{FaultKind, Outcome};
    match outcome {
        Outcome::Value(_) => After::Succeeded,
        Outcome::Failed(failure) => {
            let kind = failure.kind.clone();
            if kind.as_deref() == Some(OUTCOME_UNKNOWN) {
                After::Park {
                    cause: failure.code.clone(),
                }
            } else if matches!(kind.as_deref(), Some("unavailable" | "rate_limited")) {
                After::Retry {
                    code: failure.code.clone(),
                    kind,
                }
            } else {
                After::Fail {
                    code: failure.code.clone(),
                    kind,
                }
            }
        }
        Outcome::Fault(fault) => match &fault.kind {
            FaultKind::Diverged(problem) => After::Diverge {
                problem: problem.clone(),
            },
            FaultKind::Store(_) | FaultKind::DeadlineExceeded => After::Retry {
                code: fault.kind.classification().to_string(),
                kind: None,
            },
            other => After::Fail {
                code: other.classification().to_string(),
                kind: None,
            },
        },
        Outcome::Assertion(_) => After::Fail {
            code: "internal_fault".to_string(),
            kind: None,
        },
    }
}

/// The ceiling of the wait after attempt `attempt` (1 for the first):
/// [`BACKOFF_BASE_MS`] doubled for each attempt before it, at most
/// [`BACKOFF_CAP_MS`].
pub fn backoff_ceiling(attempt: u32) -> i64 {
    let doublings = attempt.saturating_sub(1).min(20);
    BACKOFF_BASE_MS
        .saturating_mul(1_i64 << doublings)
        .min(BACKOFF_CAP_MS)
}

/// The wait after attempt `attempt`, drawn evenly up to its ceiling from
/// `random`, eight random bytes (full jitter), never under a second; the
/// ceiling itself without them.
pub fn backoff(attempt: u32, random: Option<[u8; 8]>) -> i64 {
    let ceiling = backoff_ceiling(attempt);
    let wait = match random {
        Some(bytes) => {
            let draw = u64::from_be_bytes(bytes);
            let span = u64::try_from(ceiling).unwrap_or(u64::MAX).max(1);
            i64::try_from(draw % (span + 1)).unwrap_or(ceiling)
        }
        None => ceiling,
    };
    wait.max(1_000)
}

/// The arguments a subscriber's action takes from event `value`, each a
/// path of `args` read from it.
pub fn arguments(args: &[Vec<String>], value: &Value) -> Option<Vec<Value>> {
    args.iter()
        .map(|path| {
            let mut at = value.clone();
            for field in path {
                at = match &at {
                    Value::Record(record) => record.get(field)?.clone(),
                    _ => return None,
                };
            }
            Some(at)
        })
        .collect()
}

/// The tenant `subscriber`'s delivery of event `value` is limited to.
pub fn tenant(subscriber: &SubscriberPlan, value: &Value) -> Option<[u8; 16]> {
    match &subscriber.tenant {
        TenantFrom::Untenanted => None,
        TenantFrom::Nowhere => Some(NO_TENANT),
        TenantFrom::Field(field) => {
            let held = match value {
                Value::Record(record) => record.get(field),
                _ => None,
            };
            Some(named_tenant(held.into_iter()))
        }
    }
}

/// The tenant values holding a tenant's id name (an event's field, a
/// batch's saved values; chunk 6.4c): the one they all name, or
/// [`NO_TENANT`] when there is none, one is `none`, or they differ.
pub fn named_tenant<'a>(held: impl Iterator<Item = &'a Value>) -> [u8; 16] {
    let mut named: Option<[u8; 16]> = None;
    for value in held {
        match (crate::to_sql(value, SqlType::Uuid), named) {
            (Ok(SqlValue::Uuid(bytes)), None) => named = Some(bytes),
            (Ok(SqlValue::Uuid(bytes)), Some(already)) if bytes == already => {}
            _ => return NO_TENANT,
        }
    }
    named.unwrap_or(NO_TENANT)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct ClaimStore {
        rows: std::cell::RefCell<Option<Result<Vec<crate::Row>, crate::StoreError>>>,
        statements: std::cell::RefCell<Vec<(String, Vec<SqlValue>)>>,
    }

    impl crate::Database for ClaimStore {
        fn begin_at(
            &self,
            _: crate::Isolation,
        ) -> crate::LocalFuture<'_, Result<Box<dyn crate::Transaction>, crate::StoreError>>
        {
            Box::pin(async { panic!("claim must not open a transaction") })
        }
        fn query_once<'a>(
            &'a self,
            sql: &'a str,
            params: &'a [SqlValue],
        ) -> crate::LocalFuture<'a, Result<Vec<crate::Row>, crate::StoreError>> {
            self.statements
                .borrow_mut()
                .push((sql.into(), params.into()));
            Box::pin(async { self.rows.borrow_mut().take().expect("one claim") })
        }
    }

    #[test]
    fn a_delivery_claim_keeps_all_breakers_and_the_connectionless_fast_path() {
        let host = crate::MemoryHost::new();
        let budget = crate::budget::Budget::start(&host, crate::budget::Invocation::Tick);
        let lease = Leased {
            id: [1; 16],
            attempt: 0,
        };
        for connections in [vec![], vec!["a".into(), "b".into()]] {
            let rows = if connections.is_empty() {
                vec![vec![SqlValue::Int4(2)]]
            } else {
                vec![
                    vec![
                        SqlValue::Int4(2),
                        SqlValue::Text("a".into()),
                        SqlValue::Text("closed".into()),
                    ],
                    vec![
                        SqlValue::Int4(2),
                        SqlValue::Text("b".into()),
                        SqlValue::Text("open".into()),
                    ],
                ]
            };
            let store = ClaimStore {
                rows: std::cell::RefCell::new(Some(Ok(rows))),
                statements: Default::default(),
            };
            let claimed = crate::block_on(claim(&store, &budget, &lease, &connections, 123))
                .unwrap()
                .unwrap()
                .unwrap();
            assert_eq!(claimed.attempt, 2);
            assert_eq!(claimed.breakers.len(), connections.len());
            if !connections.is_empty() {
                assert_eq!(claimed.breakers[1][0], SqlValue::Text("b".into()));
            }
            let statements = store.statements.borrow();
            assert_eq!(statements.len(), 1);
            assert_eq!(
                statements[0].0,
                if connections.is_empty() {
                    CLAIM.into()
                } else {
                    claim_breakers_sql()
                }
            );
            assert_eq!(
                statements[0].1.len(),
                if connections.is_empty() { 4 } else { 5 }
            );
        }
    }

    #[test]
    fn a_missing_breaker_is_a_claim_but_a_lost_or_failed_claim_never_runs() {
        let host = crate::MemoryHost::new();
        let budget = crate::budget::Budget::start(&host, crate::budget::Invocation::Tick);
        let lease = Leased {
            id: [1; 16],
            attempt: 0,
        };
        let connections = vec!["absent".into()];
        for rows in [
            Ok(vec![]),
            Ok(vec![vec![SqlValue::Text("malformed attempt".into())]]),
            Err(crate::StoreError::Unavailable),
        ] {
            let store = ClaimStore {
                rows: std::cell::RefCell::new(Some(rows)),
                statements: Default::default(),
            };
            assert!(!matches!(
                crate::block_on(claim(&store, &budget, &lease, &connections, 123)),
                Ok(Ok(Some(_)))
            ));
        }
        let store = ClaimStore {
            rows: std::cell::RefCell::new(Some(Ok(vec![vec![
                SqlValue::Int4(1),
                SqlValue::Null(SqlType::Text),
            ]]))),
            statements: Default::default(),
        };
        let claimed = crate::block_on(claim(&store, &budget, &lease, &connections, 123))
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(claimed.attempt, 1);
        assert_eq!(claimed.breakers, vec![vec![SqlValue::Null(SqlType::Text)]]);
    }

    #[test]
    fn a_traceparent_header_reads_and_writes_back() {
        let header = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";
        let trace = Trace::parse(header).expect("reads");
        assert_eq!(trace.header(), header);
        assert!(trace.sampled);
        let child = trace.child([1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(
            child.header(),
            "00-4bf92f3577b34da6a3ce929d0e0e4736-0102030405060708-01"
        );
        for broken in [
            "",
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7",
            "00-00000000000000000000000000000000-00f067aa0ba902b7-01",
            "00-4bf92f3577b34da6a3ce929d0e0e4736-0000000000000000-01",
            "00-4BF92F3577B34DA6A3CE929D0E0E4736-00f067aa0ba902b7-01",
            "ff-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01-extra",
        ] {
            assert_eq!(Trace::parse(broken), None, "{broken}");
        }
        // A later version's first four fields are read.
        assert!(
            Trace::parse("01-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-00-what").is_some()
        );
    }

    #[test]
    fn retries_wait_longer_each_time_with_full_jitter_and_a_cap() {
        assert_eq!(backoff_ceiling(1), 30_000);
        assert_eq!(backoff_ceiling(2), 60_000);
        assert_eq!(backoff_ceiling(5), 480_000);
        assert_eq!(backoff_ceiling(20), BACKOFF_CAP_MS);
        // Without random bytes, the ceiling; with them, anywhere up to it,
        // never under a second.
        assert_eq!(backoff(3, None), 120_000);
        for seed in 0..64u64 {
            let wait = backoff(
                3,
                Some(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15).to_be_bytes()),
            );
            assert!((1_000..=120_000).contains(&wait), "{wait}");
        }
        // The default budget, 10 attempts, fits well inside 24 hours.
        let longest: i64 = (1..10).map(backoff_ceiling).sum();
        assert!(longest < 24 * 60 * 60 * 1000, "{longest}");
    }

    #[test]
    fn how_an_attempt_ended_decides_what_happens_next() {
        use crate::{Failure, Fault, FaultKind, Outcome};
        let failed = |kind: &str| {
            let mut failure = Failure::new("type Down", "Down", "down");
            failure.kind = Some(kind.to_string());
            Outcome::Failed(Box::new(failure))
        };
        assert_eq!(after(&Outcome::Value(Value::Unit)), After::Succeeded);
        assert!(matches!(after(&failed("unavailable")), After::Retry { .. }));
        assert!(matches!(
            after(&failed("rate_limited")),
            After::Retry { .. }
        ));
        assert!(matches!(after(&failed("not_found")), After::Fail { .. }));
        // An outside call may have acted: parked for an operator, never
        // tried again by itself (chunk 7.0c).
        assert_eq!(
            after(&Outcome::Failed(Box::new(crate::outcome_unknown()))),
            After::Park {
                cause: "outcome_unknown".into()
            }
        );
        // And so is what would be retried once a call the run made may
        // have acted; what fails for good stays failed.
        assert_eq!(
            after(&failed("unavailable")).once_acted(),
            After::Park {
                cause: "down".into()
            }
        );
        assert!(matches!(
            after(&failed("not_found")).once_acted(),
            After::Fail { .. }
        ));
        assert_eq!(
            after(&Outcome::Value(Value::Unit)).once_acted(),
            After::Succeeded
        );
        let fault = |kind| {
            Outcome::Fault(Box::new(Fault {
                kind,
                frames: Vec::new(),
            }))
        };
        assert!(matches!(
            after(&fault(FaultKind::Store("down".into()))),
            After::Retry { .. }
        ));
        assert!(matches!(
            after(&fault(FaultKind::DeadlineExceeded)),
            After::Retry { .. }
        ));
        assert_eq!(
            after(&fault(FaultKind::IntOverflow)),
            After::Fail {
                code: "int_overflow".into(),
                kind: None
            }
        );
    }
}
