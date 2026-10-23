//! The record of an action with several changing calls (area 12, "Several
//! changing calls in one action"; area 11, the same; chunk 7.5b).
//!
//! An action whose longest way makes several changing calls, run where its
//! key survives a retry (a subscriber's delivery, a route whose API
//! requires `Idempotency-Key`), keeps a record of its steps, keyed by the
//! action's key ([`crate::CallKey`]), in the store it writes, beside its
//! outbox: `jadpo_step_records` and `jadpo_steps` (area 5, "System
//! tables"). The boundary opens it as the action starts ([`open`]) and
//! deletes it in the action's commit ([`seal`]); every other write is the
//! runtime's own, one statement in a short transaction of its own.
//!
//! - **A step** is an outside call from the first changing call on (a
//!   provider operation with `changes: true`, a store's `put`), and a
//!   provider's read after it ([`step`]). Before it is sent its position,
//!   operation, a keyed fingerprint of each top-level field of its request
//!   and the time are written ([`sending`], from [`crate::effects::exchange`]);
//!   once answered, its answer, encrypted ([`answered`]). A step with no
//!   answer is in doubt.
//! - **A rerun replays.** Run again (a delivery's retry, a client's retry
//!   under its key, area 6's rerun after a concurrent change), an answered
//!   step is given back without being sent, once its operation and
//!   fingerprint match; and once a record exists, `clock.now` is the time
//!   its first run started. A step whose operation or request differs, and
//!   a run that ends before replaying every recorded step, stop as
//!   `diverged` ([`FaultKind::Diverged`]) before anything more is sent,
//!   held for an operator.
//! - **A step in doubt** is sent again under its key while its provider's
//!   `window:` lasts, less a margin; after it, or with no key, it parks as
//!   `outcome_unknown`.
//! - **One runner.** A record keeps the generation of the runner holding it
//!   (a delivery's attempt, a route's claim); every write takes it over
//!   only from an equal or older one, so a runner whose lease lapsed stops
//!   before sending another step.
//! - **What is kept.** Requests only as fingerprints, HMACs under a key
//!   derived from `application { steps: { encryption_key: } }`; answers
//!   sealed with another key derived from it ([`jadpo_value::crypto`]),
//!   bound to their record and position, at most 1 MB. Neither ever
//!   appears in a log or an error's text.

use std::collections::BTreeMap;
use std::rc::Rc;

use jadpo_std::steps::{MOST_ANSWER_BYTES, RecordPlan, window_margin_ms};
use jadpo_value::crypto::KeyRing;

use crate::capability::Repeat;
use crate::effects::Unanswered;
use crate::outbox::micros;
use crate::{Clock, Ctx, FaultKind, SqlValue, StoreError};

/// What the record of the action an outermost boundary runs is planned as
/// and kept with, as its body gives it ([`crate::ActionBody::record`]).
#[derive(Clone, Copy)]
pub struct Spec<'a> {
    pub plan: &'a RecordPlan,
    /// The identity key of the action that is run.
    pub action: &'a str,
    /// The build it runs under.
    pub build: &'a str,
    /// The number of the `SigningKey` setting `application { steps: {
    /// encryption_key: } }` names, whose ring, as the program started with
    /// it, the record's keys are derived from.
    pub setting: Option<u32>,
}

impl std::fmt::Debug for Spec<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Spec")
            .field("plan", self.plan)
            .field("action", &self.action)
            .finish_non_exhaustive()
    }
}

/// The labels the record's two keys are derived under.
const ANSWERS: &str = "jadpo step answers v1";
const FINGERPRINTS: &str = "jadpo step fingerprints v1";

/// A step's answer, as the record keeps it and a rerun gives it back.
#[derive(Clone, PartialEq, Eq)]
pub enum Answered {
    /// A provider's answer as it was sent, for its site to decode again
    /// with the current package.
    Wire(Rc<str>),
    /// One of its operation's own failures, by its code.
    Failed(String),
    /// Done, with nothing a rerun reads: a store's `put`.
    Done,
}

impl std::fmt::Debug for Answered {
    /// Never the answer.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Answered::Wire(_) => f.write_str("Wire(..)"),
            Answered::Failed(code) => f.debug_tuple("Failed").field(code).finish(),
            Answered::Done => f.write_str("done"),
        }
    }
}

impl Answered {
    /// What the record's `outcome` column says of it, for an operator.
    fn outcome(&self) -> &'static str {
        match self {
            Answered::Failed(_) => "failed",
            Answered::Wire(_) | Answered::Done => "answered",
        }
    }

    /// Its plain form, which is sealed.
    fn to_json(&self) -> String {
        let mut object = serde_json::Map::new();
        match self {
            Answered::Wire(text) => {
                object.insert("wire".into(), text.as_ref().into());
            }
            Answered::Failed(code) => {
                object.insert("failed".into(), code.as_str().into());
            }
            Answered::Done => {
                object.insert("done".into(), true.into());
            }
        }
        serde_json::Value::Object(object).to_string()
    }

    fn from_json(text: &[u8]) -> Option<Answered> {
        let json: serde_json::Value = serde_json::from_slice(text).ok()?;
        if let Some(wire) = json.get("wire").and_then(serde_json::Value::as_str) {
            return Some(Answered::Wire(Rc::from(wire)));
        }
        if let Some(code) = json.get("failed").and_then(serde_json::Value::as_str) {
            return Some(Answered::Failed(code.to_string()));
        }
        json.get("done").map(|_| Answered::Done)
    }
}

/// Where a step is, and the key generation it is sent under.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StepAt {
    /// From 1, along the way the action runs.
    pub position: u32,
    /// How many times its key was replaced: 0 until an operator's "not
    /// done" (roadmap 7.5c).
    pub key_generation: u32,
}

/// What a step proposes to send ([`step`]).
#[derive(Clone, Debug)]
pub struct Proposed<'a> {
    /// `pay.payments.create`, `sheets.put`.
    pub operation: &'a str,
    /// Each top-level field of its request, by name, as its bytes: what
    /// its fingerprint is made of, never kept.
    pub fields: Vec<(String, Vec<u8>)>,
    pub repeat: Repeat,
    /// Whether it changes something: a step from the first such call on.
    pub changes: bool,
    /// How long its provider keeps a key, when it takes one.
    pub window_ms: Option<u64>,
    /// The codes of its own failures whose kind says it did nothing
    /// (`unavailable`, `rate_limited`): never given back, since one sent
    /// again under its key may act (chunk 7.9a).
    pub did_nothing: &'a [String],
}

/// What the record makes of a step ([`step`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Stepped {
    /// No record is kept for it: sent as any call is.
    Untracked,
    /// Answered before: given back without being sent.
    Replay(Answered),
    /// To be sent, at this place.
    Send(StepAt),
}

/// What became of one step in a run, as a developer sees it in `jadpo
/// dev` (chunk 7.5f): never its request, its answer or its key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StepState {
    /// Answered on an earlier run, given back without being sent.
    Replayed,
    /// Sent, and its answer kept.
    Answered,
    /// Sent, and answered with one of its operation's failures, by its
    /// code, which the record keeps.
    Failed(String),
    /// Sent, with no answer kept: sent again under its key on the next
    /// attempt.
    InDoubt,
    /// Its operation or request differs from the record: never sent.
    Diverged,
    /// In doubt and not to be sent again (no key, or past its provider's
    /// window): parked as `outcome_unknown`.
    OutcomeUnknown,
}

impl StepState {
    /// How `--json` names it.
    pub fn word(&self) -> &'static str {
        match self {
            StepState::Replayed => "replayed",
            StepState::Answered => "answered",
            StepState::Failed(_) => "failed",
            StepState::InDoubt => "in_doubt",
            StepState::Diverged => "diverged",
            StepState::OutcomeUnknown => "outcome_unknown",
        }
    }
}

/// One step a run reached: its place, its operation, what became of it
/// and, when it was sent or given back, where its answer came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shown {
    /// From 1.
    pub position: u32,
    /// `pay.payments.create`, `sheets.put`.
    pub operation: String,
    pub state: StepState,
    pub source: Option<crate::capability::Source>,
}

/// The steps one run of an action with several changing calls reached,
/// in order, as its record closed (chunk 7.5f).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunSteps {
    /// The action's name: `subscribe`.
    pub action: String,
    pub steps: Vec<Shown>,
}

/// One step of a record.
#[derive(Clone)]
struct Kept {
    operation: String,
    /// Each field's HMAC, in hexadecimal, by name.
    fingerprint: BTreeMap<String, String>,
    key_generation: u32,
    /// When it was first sent, on the host's clock.
    first_sent: i64,
    /// The current provider contract, checked again before each send.
    window_ms: Option<u64>,
    /// This key may already have gone out (also true when loaded).
    sent: bool,
    answer: Option<Answered>,
    /// Whether its row is in the store.
    written: bool,
}

/// The record of the running action, while its outermost boundary is
/// open.
pub(crate) struct Record {
    store: String,
    action: String,
    build: String,
    shape: String,
    scope: String,
    key: String,
    /// A digest of the scope and the key.
    id: [u8; 16],
    /// The runner's generation, which every write checks.
    generation: i64,
    answers: Option<KeyRing>,
    fingerprints: Option<KeyRing>,
    /// When its first run started.
    started_at: i64,
    /// Whether its row is in the store.
    kept: bool,
    /// By position, from 1.
    steps: Vec<Kept>,
    /// How many steps this run has reached.
    at: u32,
    /// The clock the invocation had before the record fixed it.
    clock: Clock,
    /// What became of each step this run reached, by position (chunk
    /// 7.5f).
    shown: BTreeMap<u32, (String, StepState)>,
    /// Where the invocation's call log stood as it opened, so the calls
    /// its steps made are found after it.
    calls_from: usize,
}

impl std::fmt::Debug for Record {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Record")
            .field("action", &self.action)
            .field("steps", &self.steps.len())
            .field("at", &self.at)
            .finish_non_exhaustive()
    }
}

impl Record {
    /// The action's name, as an error says it: `subscribe`, its identity
    /// key without its kind.
    fn name(&self) -> &str {
        self.action
            .split_once(' ')
            .map_or(self.action.as_str(), |(_, name)| name)
    }

    /// Notes what became of step `position`, `operation`. A step given
    /// back on area 6's rerun keeps what this invocation saw of it first.
    fn note(&mut self, position: u32, operation: &str, state: StepState) {
        if state == StepState::Replayed && self.shown.contains_key(&position) {
            return;
        }
        self.shown.insert(position, (operation.to_string(), state));
    }

    /// The step this run reached last, when it has reached one.
    pub(crate) fn reached(&self) -> Option<StepAt> {
        let kept = self.steps.get((self.at as usize).checked_sub(1)?)?;
        Some(StepAt {
            position: self.at,
            key_generation: kept.key_generation,
        })
    }
}

// --- statements -----------------------------------------------------------------

/// Takes record `$1` over for generation `$2` at `$3`, from an equal or
/// older one, as it is opened: its first run's start, and its state, that
/// it runs again. Nothing when it doesn't exist or a newer runner holds
/// it. One an operator abandoned is left as it is, and says so (chunk
/// 7.5c).
pub const OPEN: &str = "UPDATE \"jadpo_step_records\" SET \"generation\" = CASE WHEN \"state\" = 'abandoned' THEN \"generation\" ELSE $2 END, \"taken_at\" = CASE WHEN \"state\" = 'abandoned' THEN \"taken_at\" ELSE $3 END, \"problem\" = CASE WHEN \"state\" = 'abandoned' THEN \"problem\" ELSE NULL END, \"state\" = CASE WHEN \"state\" = 'abandoned' THEN \"state\" ELSE 'running' END WHERE \"id\" = $1 AND \"generation\" <= $2 RETURNING \"started_at\", \"state\", \"shape\"";

/// A record an operator abandoned (chunk 7.5c): it never runs again.
pub const ABANDONED: &str = "abandoned";

/// Record `$1`'s steps, in order.
pub const STEPS: &str = "SELECT \"position\", \"operation\", \"fingerprint\"::text, \"key_generation\", \"dispatched_at\", \"answer\" FROM \"jadpo_steps\" WHERE \"record_id\" = $1 ORDER BY \"position\"";

/// A new record, as its first step is about to go out; nothing when one
/// with its id was made meanwhile.
pub const CREATE: &str = "INSERT INTO \"jadpo_step_records\" (\"id\", \"scope\", \"key\", \"action\", \"build\", \"shape\", \"started_at\", \"generation\", \"state\", \"taken_at\") VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'running', $9) ON CONFLICT (\"id\") DO NOTHING";

/// Step `$4` of record `$1`, sent at `$3` by generation `$2`: its
/// operation `$5`, fingerprint `$6` and key generation `$7`. Nothing when
/// a newer runner holds the record or an operator abandoned it. An
/// already written step is fenced again without changing its dispatch time.
pub const SEND: &str = "WITH \"holder\" AS (UPDATE \"jadpo_step_records\" SET \"generation\" = $2, \"taken_at\" = $3 WHERE \"id\" = $1 AND \"generation\" <= $2 AND \"state\" <> 'abandoned' RETURNING \"id\") INSERT INTO \"jadpo_steps\" (\"record_id\", \"position\", \"operation\", \"fingerprint\", \"key_generation\", \"dispatched_at\") SELECT \"id\", $4, $5, $6::jsonb, $7, $3 FROM \"holder\" ON CONFLICT (\"record_id\", \"position\") DO UPDATE SET \"record_id\" = EXCLUDED.\"record_id\" RETURNING \"jadpo_steps\".\"position\"";

/// Step `$4` of record `$1` answered at `$3`, by generation `$2`: its
/// outcome `$5` and its answer, sealed, `$6`.
pub const ANSWER: &str = "WITH \"holder\" AS (UPDATE \"jadpo_step_records\" SET \"generation\" = $2, \"taken_at\" = $3 WHERE \"id\" = $1 AND \"generation\" <= $2 RETURNING \"id\") UPDATE \"jadpo_steps\" SET \"outcome\" = $5, \"answer\" = $6, \"answered_at\" = $3 FROM \"holder\" WHERE \"jadpo_steps\".\"record_id\" = \"holder\".\"id\" AND \"jadpo_steps\".\"position\" = $4 RETURNING \"jadpo_steps\".\"position\"";

/// Record `$1` held as `diverged` by generation `$2` at `$3`, with what
/// differed, `$4`: the step, its operation and the fields, never a value.
pub const DIVERGE: &str = "UPDATE \"jadpo_step_records\" SET \"state\" = 'diverged', \"problem\" = $4::jsonb, \"generation\" = $2, \"taken_at\" = $3 WHERE \"id\" = $1 AND \"generation\" <= $2 RETURNING \"id\"";

/// Record `$1` deleted in its action's commit, by generation `$2`, its
/// steps with it.
pub const DELETE: &str = "DELETE FROM \"jadpo_step_records\" WHERE \"id\" = $1 AND \"generation\" <= $2 RETURNING \"id\"";

// --- the record's life ----------------------------------------------------------

/// The id of the record keyed by `scope` and `key`: a digest of both,
/// each by its length, so two never run together.
pub fn record_id(scope: &str, key: &str) -> [u8; 16] {
    let mut hashed = Vec::new();
    for part in [scope, key] {
        hashed.extend_from_slice(&(part.len() as u64).to_be_bytes());
        hashed.extend_from_slice(part.as_bytes());
    }
    let digest = blake3::hash(&hashed);
    let mut id = [0u8; 16];
    for (out, byte) in id.iter_mut().zip(digest.as_bytes()) {
        *out = *byte;
    }
    id
}

/// The time on the host's clock, which a step's window and the runtime's
/// own timestamps count on (an event's `emitted_at`, a request's signature,
/// the day an `ai` budget counts): never the record's fixed `clock.now`,
/// which only code reads. On a host with no clock, the clock the
/// invocation had before a record fixed it (chunk 7.9a).
pub(crate) fn wall(ctx: &Ctx<'_>) -> i64 {
    ctx.host()
        .now_ms()
        .unwrap_or_else(|| match ctx.steps_record() {
            Some(record) => record.clock.now(),
            None => ctx.env().clock.now(),
        })
}

/// Opens the record of the action `spec` describes, as its outermost
/// boundary opens, when it runs where its key survives a retry
/// ([`Ctx::resumable`]): reads it, taking it over for this runner's
/// generation, and fixes `clock.now` at its first run's start. `Ok(false)`
/// when no record is kept for it.
pub(crate) async fn open(ctx: &mut Ctx<'_>, spec: Option<Spec<'_>>) -> Result<bool, Unanswered> {
    let (Some(spec), Some(generation)) = (spec, ctx.resumable()) else {
        return Ok(false);
    };
    let started = ctx.started();
    let (answers, fingerprints) = if spec.plan.replay {
        let Some(ring) = spec.setting.and_then(|at| started.keys().get(at)) else {
            return Err(Unanswered::Fault(FaultKind::Defect(
                "the key `application { steps: { encryption_key: } }` names has no value in this run".into(),
            )));
        };
        let derive = |label: &str| {
            ring.derived(label).map_err(|_| {
                Unanswered::Fault(FaultKind::Defect(
                    "a steps key that can't be derived".into(),
                ))
            })
        };
        (Some(derive(ANSWERS)?), Some(derive(FINGERPRINTS)?))
    } else {
        (None, None)
    };
    let Some(call_key) = ctx.call_key() else {
        return Err(Unanswered::Fault(FaultKind::Defect(
            "an action's record with no key to keep it under".into(),
        )));
    };
    let unavailable = || Unanswered::Failure(Box::new(crate::unavailable()));
    let Some(database) = ctx.capabilities().database(&spec.plan.store) else {
        return Err(unavailable());
    };
    let id = record_id(&call_key.scope, &call_key.key);
    let now = wall(ctx);
    let opened = database
        .query_once(
            OPEN,
            &[SqlValue::Uuid(id), SqlValue::Int8(generation), micros(now)],
        )
        .await
        .map_err(|_| unavailable())?;
    let started = opened.first().and_then(|row| match row.first() {
        Some(SqlValue::Timestamp(micros)) => Some(micros.div_euclid(1000)),
        _ => None,
    });
    // An operator abandoned it (chunk 7.5c): a client retrying under its
    // key finds it ended, and nothing more is sent.
    if matches!(opened.first().and_then(|row| row.get(1)), Some(SqlValue::Text(state)) if state == ABANDONED)
    {
        let action = spec
            .action
            .split_once(' ')
            .map_or(spec.action, |(_, name)| name);
        return Err(Unanswered::Fault(FaultKind::Diverged(format!(
            "the steps of `{action}` were abandoned by an operator"
        ))));
    }
    let mut steps = Vec::new();
    if started.is_some() {
        let rows = database
            .query_once(STEPS, &[SqlValue::Uuid(id)])
            .await
            .map_err(|_| unavailable())?;
        let dispatch = opened
            .first()
            .and_then(|row| match row.get(2) {
                Some(SqlValue::Text(shape)) => {
                    Some(shape.starts_with(jadpo_std::steps::DISPATCH_SHAPE_PREFIX))
                }
                _ => None,
            })
            .unwrap_or(false);
        if dispatch == spec.plan.replay {
            let operation = rows
                .first()
                .and_then(|row| match row.get(1) {
                    Some(SqlValue::Text(operation)) => Some(operation.clone()),
                    _ => None,
                })
                .unwrap_or_else(|| "outside call".into());
            let problem = Diverged {
                position: 1,
                operation,
                how: How::RecordMode {
                    replay: spec.plan.replay,
                },
            };
            let action = spec
                .action
                .split_once(' ')
                .map_or(spec.action, |(_, name)| name);
            let text = problem.text(action);
            let _ = database
                .query_once(
                    DIVERGE,
                    &[
                        SqlValue::Uuid(id),
                        SqlValue::Int8(generation),
                        micros(now),
                        SqlValue::Json(problem.json()),
                    ],
                )
                .await;
            return Err(Unanswered::Fault(FaultKind::Diverged(text)));
        }
        for (at, row) in rows.iter().enumerate() {
            let [
                SqlValue::Int4(position),
                SqlValue::Text(operation),
                SqlValue::Text(fingerprint),
                SqlValue::Int4(key_generation),
                SqlValue::Timestamp(first_sent),
                answer,
            ] = row.as_slice()
            else {
                return Err(Unanswered::Fault(FaultKind::Defect(
                    "a step's row of the wrong shape".into(),
                )));
            };
            // Steps are written in order, each after the last: a gap is a
            // record this runtime didn't write.
            if usize::try_from(*position).ok() != Some(at + 1) {
                return Err(Unanswered::Fault(FaultKind::Defect(
                    "a record whose steps aren't numbered in order".into(),
                )));
            }
            let answer = match answer {
                SqlValue::Bytes(sealed) => {
                    let context = context(&id, at as u32 + 1);
                    let opened = answers
                        .as_ref()
                        .and_then(|key| jadpo_value::crypto::decrypt(sealed, key, &context).ok())
                        .and_then(|plain| Answered::from_json(&plain));
                    match opened {
                        Some(answer) => Some(answer),
                        // Sealed under a key neither the steps key nor
                        // the one before it is: the deployment's keys
                        // were rotated past it.
                        None => {
                            return Err(Unanswered::Failure(Box::new(crate::misconfigured())));
                        }
                    }
                }
                _ => None,
            };
            let fingerprint: BTreeMap<String, String> =
                serde_json::from_str(fingerprint).unwrap_or_default();
            steps.push(Kept {
                operation: operation.clone(),
                fingerprint,
                key_generation: u32::try_from(*key_generation).unwrap_or(0),
                first_sent: first_sent.div_euclid(1000),
                window_ms: None,
                sent: true,
                answer,
                written: true,
            });
        }
    }
    let clock = ctx.env().clock;
    let started_at = started.unwrap_or_else(|| ctx.now());
    if started.is_some() && spec.plan.replay {
        // Once a replay record exists, `clock.now` is its first run's start for
        // every rerun (area 11, "Time").
        ctx.set_clock(Clock::fixed(started_at));
    }
    ctx.set_steps_record(Some(Record {
        store: spec.plan.store.clone(),
        action: spec.action.to_string(),
        build: spec.build.to_string(),
        shape: spec.plan.shape.clone(),
        scope: call_key.scope,
        key: call_key.key,
        id,
        generation,
        answers,
        fingerprints,
        started_at,
        kept: started.is_some(),
        steps,
        at: 0,
        clock,
        shown: BTreeMap::new(),
        calls_from: ctx.capabilities().calls().len(),
    }));
    Ok(true)
}

/// Starts the action's run again from its first step: area 6's rerun
/// after a concurrent change replays what the last run recorded.
pub(crate) fn rewind(ctx: &mut Ctx<'_>) {
    if let Some(record) = ctx.steps_record_mut() {
        record.at = 0;
        // A step proposed but never written is proposed again.
        record.steps.retain(|kept| kept.written);
    }
}

/// Closes the record as the outermost boundary closes, giving the
/// invocation its own clock back.
/// What became of each step it reached is kept on the invocation, with
/// where each one's answer came from (chunk 7.5f).
pub(crate) fn close(ctx: &mut Ctx<'_>) {
    let Some(record) = ctx.take_steps_record() else {
        return;
    };
    ctx.set_clock(record.clock);
    if record.shown.is_empty() {
        return;
    }
    let calls = ctx
        .capabilities()
        .calls()
        .get(record.calls_from..)
        .unwrap_or(&[]);
    let steps = record
        .shown
        .iter()
        .map(|(position, (operation, state))| Shown {
            position: *position,
            operation: operation.clone(),
            state: state.clone(),
            source: calls
                .iter()
                .rev()
                .find(|call| call.step == Some(*position))
                .map(|call| call.source),
        })
        .collect();
    let action = record.name().to_string();
    ctx.push_steps(RunSteps { action, steps });
}

/// What `proposed`, the running action's next outside call, is to its
/// record: untracked when it keeps none, or for a read before its first
/// changing call; given its answer back when this step was answered
/// before and matches; sent, at its place, when it is new or in doubt and
/// may go again. A step whose operation or request differs from its
/// record stops the action as `diverged`; one in doubt that may not be
/// sent again parks it as `outcome_unknown`.
pub async fn step(ctx: &mut Ctx<'_>, proposed: Proposed<'_>) -> Result<Stepped, Unanswered> {
    let now = wall(ctx);
    let Some(record) = ctx.steps_record_mut() else {
        return Ok(Stepped::Untracked);
    };
    if (record.answers.is_none() && proposed.repeat != Repeat::Keyed)
        || (!proposed.changes && record.at == 0)
    {
        return Ok(Stepped::Untracked);
    }
    record.at = record.at.saturating_add(1);
    let position = record.at;
    let fields: BTreeMap<&str, &[u8]> = proposed
        .fields
        .iter()
        .map(|(name, bytes)| (name.as_str(), bytes.as_slice()))
        .collect();
    let index = position as usize - 1;
    let Some(kept) = record.steps.get(index).cloned() else {
        // A new step: proposed now, written as it goes out.
        let fingerprint = record
            .fingerprints
            .as_ref()
            .map(|key| {
                fields
                    .iter()
                    .map(|(name, bytes)| {
                        let mac = jadpo_value::crypto::hmac(bytes, key)
                            .map(|mac| jadpo_value::encoding::to_hex(&mac))
                            .unwrap_or_default();
                        (name.to_string(), mac)
                    })
                    .collect()
            })
            .unwrap_or_default();
        record.steps.push(Kept {
            operation: proposed.operation.to_string(),
            fingerprint,
            key_generation: 0,
            first_sent: now,
            window_ms: (proposed.repeat == Repeat::Keyed)
                .then_some(proposed.window_ms.unwrap_or(0)),
            sent: false,
            answer: None,
            written: false,
        });
        return Ok(Stepped::Send(StepAt {
            position,
            key_generation: 0,
        }));
    };
    // A recorded step: the same operation with the same request, or the
    // action no longer runs as it did.
    if kept.operation != proposed.operation {
        let problem = Diverged {
            position,
            operation: kept.operation.clone(),
            how: How::Operation(proposed.operation.to_string()),
        };
        return Err(diverge(ctx, problem).await);
    }
    let mut differ: Vec<String> = Vec::new();
    if let Some(key) = &record.fingerprints {
        for (name, bytes) in &fields {
            let same = kept
                .fingerprint
                .get(*name)
                .and_then(|mac| jadpo_value::encoding::from_hex(mac))
                .is_some_and(|mac| jadpo_value::crypto::verify_hmac(bytes, &mac, key));
            if !same {
                differ.push((*name).to_string());
            }
        }
        for name in kept.fingerprint.keys() {
            if !fields.contains_key(name.as_str()) {
                differ.push(name.clone());
            }
        }
    }
    if !differ.is_empty() {
        differ.sort();
        differ.dedup();
        let problem = Diverged {
            position,
            operation: kept.operation.clone(),
            how: How::Fields(differ),
        };
        return Err(diverge(ctx, problem).await);
    }
    if let Some(step) = record.steps.get_mut(index) {
        step.window_ms =
            (proposed.repeat == Repeat::Keyed).then_some(proposed.window_ms.unwrap_or(0));
    }
    let at = StepAt {
        position,
        key_generation: kept.key_generation,
    };
    // A failure that says it did nothing is never given back: kept by a
    // runtime before chunk 7.9a, it leaves the step in doubt.
    if let Some(answer) = kept.answer
        && !matches!(&answer, Answered::Failed(code) if proposed.did_nothing.contains(code))
    {
        record.note(position, &kept.operation, StepState::Replayed);
        return Ok(Stepped::Replay(answer));
    }
    // In doubt: sent again under its key while its provider keeps it,
    // less the margin; never with no key.
    let parked = match proposed.repeat {
        Repeat::Unkeyed => true,
        Repeat::Keyed => {
            let window = proposed.window_ms.unwrap_or(0);
            let until = kept.first_sent.saturating_add(
                i64::try_from(window - window_margin_ms(window)).unwrap_or(i64::MAX),
            );
            now >= until
        }
        // Safe to send again as it is, whenever.
        Repeat::Safe => false,
    };
    if parked {
        record.note(position, &kept.operation, StepState::OutcomeUnknown);
        return Err(Unanswered::Failure(Box::new(crate::outcome_unknown())));
    }
    Ok(Stepped::Send(at))
}

/// Writes step `at` before it goes out, once: the record itself before
/// its first step, then the step, each taking the record over for this
/// runner's generation (called by [`crate::effects::exchange`] before the
/// driver). A step sent again rechecks its runner without replacing its
/// first dispatch time. `unavailable`, with
/// nothing sent, when the store can't be reached or a newer runner holds
/// the record.
pub(crate) async fn sending(ctx: &mut Ctx<'_>, at: StepAt) -> Result<(), Unanswered> {
    check_window(ctx, at)?;
    written(ctx, at).await?;
    // Metadata persistence is an await: the host clock can cross the
    // safe horizon while the generation fence is being written.
    check_window(ctx, at)?;
    going_out(ctx, at);
    // A step reached, which `jadpo test` may crash before (chunk 7.5d).
    #[cfg(feature = "testing")]
    let named = ctx.steps_record().and_then(|record| {
        let kept = record.steps.get((at.position as usize).checked_sub(1)?)?;
        Some((record.name().to_string(), kept.operation.clone()))
    });
    #[cfg(feature = "testing")]
    if let Some((action, operation)) = named {
        crate::crashes::reached(ctx, &action, at.position, &operation);
        if crate::crashes::due(ctx, crate::crashes::Boundary::BeforeSend) {
            return Err(Unanswered::Fault(FaultKind::Crashed));
        }
    }
    Ok(())
}

/// Checks host time both sides of the durable fence, before the driver.
fn check_window(ctx: &mut Ctx<'_>, at: StepAt) -> Result<(), Unanswered> {
    let now = wall(ctx);
    let expired = ctx.steps_record().and_then(|record| {
        let kept = record.steps.get(at.position as usize - 1)?;
        let window = kept.window_ms?;
        // No declared window permits the first send, but no blind resend.
        if !kept.written || (window == 0 && !kept.sent) {
            return None;
        }
        let until = kept
            .first_sent
            .saturating_add(i64::try_from(window - window_margin_ms(window)).unwrap_or(i64::MAX));
        (now >= until).then(|| kept.operation.clone())
    });
    if let Some(operation) = expired {
        if let Some(record) = ctx.steps_record_mut() {
            record.note(at.position, &operation, StepState::OutcomeUnknown);
        }
        return Err(Unanswered::Failure(Box::new(crate::outcome_unknown())));
    }
    Ok(())
}

/// [`sending`]'s writes.
async fn written(ctx: &mut Ctx<'_>, at: StepAt) -> Result<(), Unanswered> {
    let now = wall(ctx);
    let (database, record_params, step_params) = {
        let Some(record) = ctx.steps_record() else {
            return Ok(());
        };
        let Some(kept) = record.steps.get(at.position as usize - 1) else {
            return Ok(());
        };
        let Some(database) = ctx.capabilities().database(&record.store) else {
            return Err(Unanswered::Failure(Box::new(crate::unavailable())));
        };
        let record_params = (!record.kept).then(|| {
            vec![
                SqlValue::Uuid(record.id),
                SqlValue::Text(record.scope.clone()),
                SqlValue::Text(record.key.clone()),
                SqlValue::Text(record.action.clone()),
                SqlValue::Text(record.build.clone()),
                SqlValue::Text(record.shape.clone()),
                micros(record.started_at),
                SqlValue::Int8(record.generation),
                micros(now),
            ]
        });
        let fingerprint = serde_json::to_string(&kept.fingerprint).unwrap_or_default();
        let step_params = vec![
            SqlValue::Uuid(record.id),
            SqlValue::Int8(record.generation),
            micros(now),
            SqlValue::Int4(i32::try_from(at.position).unwrap_or(i32::MAX)),
            SqlValue::Text(kept.operation.clone()),
            SqlValue::Json(fingerprint),
            SqlValue::Int4(i32::try_from(at.key_generation).unwrap_or(0)),
        ];
        (database, record_params, step_params)
    };
    let unavailable = || Unanswered::Failure(Box::new(crate::unavailable()));
    if let Some(params) = record_params {
        database
            .query_once(CREATE, &params)
            .await
            .map_err(|_| unavailable())?;
    }
    let written = database
        .query_once(SEND, &step_params)
        .await
        .map_err(|_| unavailable())?;
    if written.is_empty() {
        return Err(unavailable());
    }
    if let Some(record) = ctx.steps_record_mut() {
        record.kept = true;
        if let Some(kept) = record.steps.get_mut(at.position as usize - 1) {
            if !kept.written {
                kept.first_sent = now;
            }
            kept.written = true;
        }
    }
    Ok(())
}

/// Notes step `at` as going out with no answer kept yet: in doubt until
/// [`answered`] keeps one (chunk 7.5f).
fn going_out(ctx: &mut Ctx<'_>, at: StepAt) {
    if let Some(kept) = ctx
        .steps_record_mut()
        .and_then(|record| record.steps.get_mut(at.position as usize - 1))
    {
        kept.sent = true;
    }
    if let Some(record) = ctx.steps_record_mut()
        && let Some(operation) = record
            .steps
            .get(at.position as usize - 1)
            .map(|kept| kept.operation.clone())
    {
        record.note(at.position, &operation, StepState::InDoubt);
    }
}

/// Keeps step `at`'s answer, sealed, once it comes: a rerun gives it back
/// rather than sending the step again. An answer past 1 MB is a fault
/// naming the step; `unavailable` when the store can't keep it or a newer
/// runner holds the record, which sends the step again under its key.
pub async fn answered(ctx: &mut Ctx<'_>, at: StepAt, answer: Answered) -> Result<(), Unanswered> {
    let now = wall(ctx);
    // A single changing call records its dispatch, never its request or
    // answer. Its provider replays the stable key on a rerun.
    if let Some(record) = ctx.steps_record_mut()
        && record.answers.is_none()
    {
        if let Some(operation) = record
            .steps
            .get(at.position as usize - 1)
            .map(|kept| kept.operation.clone())
        {
            record.note(
                at.position,
                &operation,
                match answer {
                    Answered::Failed(code) => StepState::Failed(code),
                    _ => StepState::Answered,
                },
            );
        }
        return Ok(());
    }
    let plain = answer.to_json();
    let (database, id, generation, operation) = {
        let Some(record) = ctx.steps_record() else {
            return Ok(());
        };
        let Some(kept) = record.steps.get(at.position as usize - 1) else {
            return Ok(());
        };
        if plain.len() > MOST_ANSWER_BYTES {
            return Err(Unanswered::Fault(FaultKind::Provider(format!(
                "step {} of `{}`, `{}`, answered more than the 1 MB a step keeps",
                at.position,
                record.name(),
                kept.operation
            ))));
        }
        let Some(database) = ctx.capabilities().database(&record.store) else {
            return Err(Unanswered::Failure(Box::new(crate::unavailable())));
        };
        (
            database,
            record.id,
            record.generation,
            kept.operation.clone(),
        )
    };
    let sealed = {
        let Some(record) = ctx.steps_record() else {
            return Ok(());
        };
        let Some(answers) = record.answers.clone() else {
            return Ok(());
        };
        let context = context(&id, at.position);
        jadpo_value::crypto::encrypt(
            plain.as_bytes(),
            &answers,
            &context,
            &mut |bytes: &mut [u8]| ctx.secret_random_bytes(bytes),
        )
    };
    let Ok(sealed) = sealed else {
        return Err(Unanswered::Fault(FaultKind::Defect(format!(
            "step {}, `{operation}`, couldn't be sealed: no random bytes for its nonce",
            at.position
        ))));
    };
    let written = database
        .query_once(
            ANSWER,
            &[
                SqlValue::Uuid(id),
                SqlValue::Int8(generation),
                micros(now),
                SqlValue::Int4(i32::try_from(at.position).unwrap_or(i32::MAX)),
                SqlValue::Text(answer.outcome().to_string()),
                SqlValue::Bytes(sealed),
            ],
        )
        .await;
    match written {
        Ok(rows) if !rows.is_empty() => {}
        _ => return Err(Unanswered::Failure(Box::new(crate::unavailable()))),
    }
    let state = match &answer {
        Answered::Failed(code) => StepState::Failed(code.clone()),
        Answered::Wire(_) | Answered::Done => StepState::Answered,
    };
    if let Some(record) = ctx.steps_record_mut() {
        if let Some(kept) = record.steps.get_mut(at.position as usize - 1) {
            kept.answer = Some(answer);
        }
        record.note(at.position, &operation, state);
    }
    // Its answer kept, which `jadpo test` may crash after (chunk 7.5d).
    #[cfg(feature = "testing")]
    if crate::crashes::due(ctx, crate::crashes::Boundary::AfterAnswer) {
        return Err(Unanswered::Fault(FaultKind::Crashed));
    }
    Ok(())
}

/// Step `$4` of record `$1` moved to key generation `$5` at `$3`, by
/// generation `$2`: sent again under its next key from then on.
pub const NEW_KEY: &str = "WITH \"holder\" AS (UPDATE \"jadpo_step_records\" SET \"generation\" = $2, \"taken_at\" = $3 WHERE \"id\" = $1 AND \"generation\" <= $2 RETURNING \"id\") UPDATE \"jadpo_steps\" SET \"key_generation\" = $5, \"dispatched_at\" = $3 FROM \"holder\" WHERE \"jadpo_steps\".\"record_id\" = \"holder\".\"id\" AND \"jadpo_steps\".\"position\" = $4 RETURNING \"jadpo_steps\".\"position\"";

/// The provider answered step `at`, or an untracked keyed call, with its
/// package's `new_key_on:` code: its key can't be used again (chunk 7.5c).
/// The place to send it from under the next key generation: for a step,
/// recorded first, so a rerun sends under that key too; `unavailable`,
/// with nothing sent, when the store can't keep it or a newer runner
/// holds the record.
pub async fn new_key(ctx: &mut Ctx<'_>, at: StepAt) -> Result<StepAt, Unanswered> {
    let next = StepAt {
        position: at.position,
        key_generation: at.key_generation.saturating_add(1),
    };
    let now = wall(ctx);
    let (database, id, generation) = {
        let Some(record) = ctx.steps_record() else {
            return Ok(next);
        };
        if record.steps.get(at.position as usize - 1).is_none() {
            return Ok(next);
        }
        let Some(database) = ctx.capabilities().database(&record.store) else {
            return Err(Unanswered::Failure(Box::new(crate::unavailable())));
        };
        (database, record.id, record.generation)
    };
    let written = database
        .query_once(
            NEW_KEY,
            &[
                SqlValue::Uuid(id),
                SqlValue::Int8(generation),
                micros(now),
                SqlValue::Int4(i32::try_from(at.position).unwrap_or(i32::MAX)),
                SqlValue::Int4(i32::try_from(next.key_generation).unwrap_or(i32::MAX)),
            ],
        )
        .await;
    match written {
        Ok(rows) if !rows.is_empty() => {}
        _ => return Err(Unanswered::Failure(Box::new(crate::unavailable()))),
    }
    if let Some(record) = ctx.steps_record_mut()
        && let Some(kept) = record.steps.get_mut(at.position as usize - 1)
    {
        kept.key_generation = next.key_generation;
        kept.first_sent = now;
        kept.sent = false;
    }
    Ok(next)
}

/// What the AEAD binds a sealed answer to: its record and its place, so
/// an answer moved to another step doesn't open.
fn context(id: &[u8; 16], position: u32) -> String {
    format!(
        "jadpo step {} {position}",
        jadpo_value::encoding::to_hex(id)
    )
}

/// Why a rerun no longer matches its record.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Diverged {
    position: u32,
    /// The recorded step's operation.
    operation: String,
    how: How,
}

/// How a step differs from its record.
#[derive(Clone, Debug, PartialEq, Eq)]
enum How {
    /// The run makes this operation there instead.
    Operation(String),
    /// The request's top-level fields that differ.
    Fields(Vec<String>),
    /// The run ended before it.
    Missed,
    /// The provider says its key came with a different request.
    Reused,
    /// This build changed between encrypted replay and dispatch metadata.
    RecordMode { replay: bool },
}

impl Diverged {
    /// How a delivery's record and a fault say it: the step, its
    /// operation and the fields, never a value.
    fn text(&self, action: &str) -> String {
        let at = format!("step {} of `{action}`, `{}`", self.position, self.operation);
        match &self.how {
            How::Operation(now) => format!("{at}, is now `{now}`"),
            How::Missed => format!("{at}, wasn't reached"),
            How::Reused => format!("{at}: the provider says its key came with a different request"),
            How::RecordMode { replay } => {
                let (recorded, current) = if *replay {
                    ("dispatch metadata", "encrypted replay")
                } else {
                    ("encrypted replay", "dispatch metadata")
                };
                format!("{at}: its record uses {recorded}; this build uses {current}")
            }
            How::Fields(fields) => format!(
                "{at}: {} {} from the record",
                fields
                    .iter()
                    .map(|field| format!("`{field}`"))
                    .collect::<Vec<_>>()
                    .join(", "),
                if fields.len() == 1 {
                    "differs"
                } else {
                    "differ"
                },
            ),
        }
    }

    /// Read back from what the record keeps ([`Diverged::json`]).
    fn from_json(text: &str) -> Option<Diverged> {
        let json: serde_json::Value = serde_json::from_str(text).ok()?;
        let position = u32::try_from(json.get("step")?.as_u64()?).ok()?;
        let operation = json.get("operation")?.as_str()?.to_string();
        let how = if let Some(now) = json.get("now").and_then(serde_json::Value::as_str) {
            How::Operation(now.to_string())
        } else if let Some(fields) = json.get("fields").and_then(serde_json::Value::as_array) {
            How::Fields(
                fields
                    .iter()
                    .filter_map(|field| field.as_str().map(str::to_string))
                    .collect(),
            )
        } else if let Some(replay) = json.get("replay").and_then(serde_json::Value::as_bool) {
            How::RecordMode { replay }
        } else if json.get("reused").is_some() {
            How::Reused
        } else {
            How::Missed
        };
        Some(Diverged {
            position,
            operation,
            how,
        })
    }

    fn json(&self) -> String {
        let mut object = serde_json::Map::new();
        object.insert("step".into(), self.position.into());
        object.insert("operation".into(), self.operation.as_str().into());
        match &self.how {
            How::Operation(now) => {
                object.insert("now".into(), now.as_str().into());
            }
            How::Fields(fields) => {
                object.insert(
                    "fields".into(),
                    fields
                        .iter()
                        .map(|field| serde_json::Value::from(field.as_str()))
                        .collect::<Vec<_>>()
                        .into(),
                );
            }
            How::Missed => {
                object.insert("missed".into(), true.into());
            }
            How::Reused => {
                object.insert("reused".into(), true.into());
            }
            How::RecordMode { replay } => {
                object.insert("replay".into(), (*replay).into());
            }
        }
        serde_json::Value::Object(object).to_string()
    }
}

/// What differed when a record was held as `diverged`, as its `problem`
/// column keeps it, said as a delivery's record says it (chunk 7.5c):
/// the step, its operation and the fields, never a value. `action` is the
/// action's identity key.
pub fn problem_text(problem: &str, action: &str) -> Option<String> {
    let name = action.split_once(' ').map_or(action, |(_, name)| name);
    Diverged::from_json(problem).map(|diverged| diverged.text(name))
}

/// The provider answered step `at` that its key came with a different
/// request (its package's `reused:` code): the action no longer sends what
/// it sent under that key, and stops as `diverged`, never
/// `misconfigured` (area 11, "One runner per record").
pub async fn reused(ctx: &mut Ctx<'_>, at: StepAt) -> Unanswered {
    let operation = ctx
        .steps_record()
        .and_then(|record| record.steps.get(at.position as usize - 1))
        .map(|kept| kept.operation.clone())
        .unwrap_or_default();
    let problem = Diverged {
        position: at.position,
        operation,
        how: How::Reused,
    };
    diverge(ctx, problem).await
}

/// The id a store's `put` at step `at` keeps its object under: a digest of
/// the record, the step and its key generation, so a step sent again
/// writes the same object rather than a second one (area 16).
pub fn object_id(ctx: &Ctx<'_>, at: StepAt) -> Option<[u8; 16]> {
    let record = ctx.steps_record()?;
    let mut hashed = record.id.to_vec();
    hashed.extend_from_slice(&at.position.to_be_bytes());
    hashed.extend_from_slice(&at.key_generation.to_be_bytes());
    let digest = blake3::hash(&hashed);
    let mut id = [0u8; 16];
    for (out, byte) in id.iter_mut().zip(digest.as_bytes()) {
        *out = *byte;
    }
    Some(id)
}

/// Holds the record as `diverged` with `problem`, for an operator, and
/// gives the fault that stops the action before anything more is sent.
async fn diverge(ctx: &mut Ctx<'_>, problem: Diverged) -> Unanswered {
    let now = wall(ctx);
    if let Some(record) = ctx.steps_record_mut() {
        record.note(problem.position, &problem.operation, StepState::Diverged);
    }
    let Some(record) = ctx.steps_record() else {
        return Unanswered::Fault(FaultKind::Diverged(problem.text("")));
    };
    let text = problem.text(record.name());
    if record.kept
        && let Some(database) = ctx.capabilities().database(&record.store)
    {
        let _ = database
            .query_once(
                DIVERGE,
                &[
                    SqlValue::Uuid(record.id),
                    SqlValue::Int8(record.generation),
                    micros(now),
                    SqlValue::Json(problem.json()),
                ],
            )
            .await;
    }
    Unanswered::Fault(FaultKind::Diverged(text))
}

/// Whether the run that is about to commit replayed every recorded step:
/// one that ended before reaching one no longer runs as the record's did,
/// and stops as `diverged` rather than committing.
pub(crate) async fn finished(ctx: &mut Ctx<'_>) -> Result<(), FaultKind> {
    let Some(record) = ctx.steps_record() else {
        return Ok(());
    };
    if record.answers.is_none() {
        return Ok(());
    }
    let reached = record.at as usize;
    let Some(missed) = record.steps.get(reached).filter(|kept| kept.written) else {
        return Ok(());
    };
    let problem = Diverged {
        position: record.at + 1,
        operation: missed.operation.clone(),
        how: How::Missed,
    };
    match diverge(ctx, problem).await {
        Unanswered::Fault(kind) => Err(kind),
        _ => Err(FaultKind::Defect("a record that diverged".into())),
    }
}

/// Deletes the record in the action's commit transaction, beside its
/// handled record or its route's stored response (area 11, "Ending"):
/// `Unavailable` when a newer runner took it over meanwhile, so nothing
/// this run did is kept.
pub(crate) async fn seal(ctx: &mut Ctx<'_>) -> Result<(), StoreError> {
    let Some(record) = ctx.steps_record() else {
        return Ok(());
    };
    if !record.kept {
        return Ok(());
    }
    let (store, id, generation) = (record.store.clone(), record.id, record.generation);
    let transaction = ctx.transaction(&store).await?;
    let deleted = transaction
        .query(DELETE, &[SqlValue::Uuid(id), SqlValue::Int8(generation)])
        .await?;
    if deleted.is_empty() {
        return Err(StoreError::Unavailable);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A host with a clock, when given one, and nothing else.
    struct Clocked(Option<i64>);

    impl crate::Host for Clocked {
        fn database(&self, _store: &str) -> Option<&dyn crate::Database> {
            None
        }

        fn random(&self, _bytes: &mut [u8]) -> bool {
            false
        }

        fn now_ms(&self) -> Option<i64> {
            self.0
        }
    }

    /// A record of action `charge`, its first run started at `started_at`.
    fn kept(started_at: i64, clock: Clock) -> Record {
        let ring = KeyRing::from_signing_text("a-signing-key-for-the-record-of-steps", None);
        Record {
            store: "main".into(),
            action: "action charge".into(),
            build: String::new(),
            shape: String::new(),
            scope: "delivery".into(),
            key: "d1".into(),
            id: record_id("delivery", "d1"),
            generation: 1,
            answers: Some(ring.clone()),
            fingerprints: Some(ring),
            started_at,
            kept: true,
            steps: Vec::new(),
            at: 0,
            clock,
            shown: BTreeMap::new(),
            calls_from: 0,
        }
    }

    /// Once a record fixes `clock.now` at its first run's start, the
    /// runtime's own time (an event's `emitted_at`, a request's AWS
    /// signature, an `ai` budget's day) is still the host's, or, on a host
    /// with no clock, the invocation's own (chunk 7.9a).
    #[test]
    fn the_runtimes_own_time_is_never_a_records_fixed_clock() {
        const STARTED: i64 = 1_000;
        const INVOKED: i64 = 5_000;
        const HOST: i64 = 9_000;
        let places = crate::Places::new();
        for (host, wanted) in [(Clocked(Some(HOST)), HOST), (Clocked(None), INVOKED)] {
            let env = crate::Env {
                seed: None,
                clock: Clock::fixed(INVOKED),
            };
            let mut ctx = Ctx::new(&places, &host, env);
            ctx.set_steps_record(Some(kept(STARTED, ctx.env().clock)));
            ctx.set_clock(Clock::fixed(STARTED));
            assert_eq!(ctx.now(), STARTED, "`clock.now` is the record's");
            assert_eq!(wall(&ctx), wanted);
        }
    }

    #[test]
    fn a_records_id_is_its_scope_and_key_never_run_together() {
        assert_eq!(record_id("delivery", "ab"), record_id("delivery", "ab"));
        assert_ne!(record_id("delivery", "ab"), record_id("deliver", "yab"));
        assert_ne!(record_id("user 1", "k"), record_id("user 2", "k"));
    }

    #[test]
    fn an_answer_is_kept_as_json_and_read_back() {
        for answer in [
            Answered::Wire(Rc::from(r#"{"id": "pay_1"}"#)),
            Answered::Failed("card_declined".into()),
            Answered::Done,
        ] {
            let text = answer.to_json();
            assert_eq!(Answered::from_json(text.as_bytes()), Some(answer.clone()));
            assert!(!format!("{answer:?}").contains("pay_1"));
        }
    }

    #[test]
    fn a_divergence_names_the_step_and_fields_never_a_value() {
        let fields = Diverged {
            position: 2,
            operation: "pay.payments.create".into(),
            how: How::Fields(vec!["amount".into(), "reference".into()]),
        };
        assert_eq!(
            fields.text("pay_invoice"),
            "step 2 of `pay_invoice`, `pay.payments.create`: `amount`, `reference` differ from the record"
        );
        let missed = Diverged {
            position: 3,
            operation: "sheets.put".into(),
            how: How::Missed,
        };
        assert_eq!(
            missed.text("pay_invoice"),
            "step 3 of `pay_invoice`, `sheets.put`, wasn't reached"
        );
    }

    #[test]
    fn a_kept_problem_reads_back_as_the_same_words() {
        for how in [
            How::Fields(vec!["plan".into()]),
            How::Operation("pay.refunds.create".into()),
            How::Missed,
            How::Reused,
        ] {
            let diverged = Diverged {
                position: 3,
                operation: "pay.subscriptions.create".into(),
                how,
            };
            assert_eq!(
                problem_text(&diverged.json(), "action subscribe"),
                Some(diverged.text("subscribe"))
            );
        }
        assert_eq!(problem_text("not json", "action subscribe"), None);
    }
}
