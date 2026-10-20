//! Operations with effects (docs/compiler-runtime.md §2.5, chunk 7.0b):
//! the one entry point both executors call for an operation that reads
//! the invocation rather than only its values. `clock.now` reads its
//! clock; `random` its random bytes, recorded for a replay; `crypto` the
//! key its `key:` names, its random bytes (secret for a nonce) and, to
//! check a token's times, its clock; `json.read`, `json.write` and a
//! token's claims the codec the plan numbers for their site; and `emit`
//! the running action's outbox. The interpreter runs them here and keeps
//! no copy, and compiled code calls the same function, so the two can't
//! differ in what an effect does, only in how they reach it.
//!
//! Each is an [`Effect`], never source text: `clock.now` is the graph's
//! own node (`Expr::Now`), and the rest the standard operation [`is_effect`]
//! names or the `emit` statement. What an effect needs of the program is
//! handed over as [`Effects`]: the interpreter builds it from its plan,
//! compiled code from its statics.
//!
//! Since chunk 7.0c a call that leaves the process (a provider's
//! operation, a store's file, an email) runs here too, through [`call`],
//! the capability seam's one entry point ([`crate::capability`]): refused
//! inside an open transaction, logged on its invocation through the
//! context's capabilities ([`Ctx::capabilities`]), its answer kept by a
//! recording and given back by a replay, and answered by the driver the
//! host gives, or a fake a test gives in its place. It is asynchronous,
//! since a call waits on the outside, where [`run`] never waits.

use std::rc::Rc;

use jadpo_std::config::ConfigPlan;
use jadpo_std::http::EventJson;
use jadpo_std::subscriber::EventsPlan;
use jadpo_std::{Kind, Op, RuleKey};
use jadpo_value::Value;
use jadpo_value::crypto::KeyRing;
use jadpo_value::json::{Problem, Unread};

use crate::capability::{Answer, Call, Repeat};
use crate::{Ctx, EmitRefused, Event, Failure, FaultKind, JsonCodecs, SqlValue};

/// An operation with effects, as its site names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect<'s> {
    /// `clock.now`: the time the invocation's clock reads (area 10), a
    /// test's fixed clock in a test (area 13).
    Now,
    /// A standard operation [`is_effect`] names, given its values in its
    /// parameters' order: `random`'s, `crypto`'s, `json.read` and
    /// `json.write`. `json` is the number the plan gives the type its site
    /// reads or writes: `json.read`'s and `json.write`'s, and
    /// `crypto.verify_jwt`'s with `as:`.
    Std { op: Op, json: Option<u32> },
    /// `emit` of an event of the type whose identity key is `event`
    /// (area 12), given the event's value.
    Emit { event: &'s str },
}

/// What an effect runs with besides its values: the program's tables.
#[derive(Clone, Copy, Debug)]
pub struct Effects<'a> {
    /// The plan's JSON table with the program's validators and shapes:
    /// what `json.read`, `json.write` and a token's claims are read and
    /// written with (area 19), and an event's payload (area 12).
    pub json: JsonCodecs<'a>,
    /// Each event type's number in the JSON table, by identity key.
    pub event_numbers: &'a [EventJson],
    /// The program's subscribers, which an event is delivered to.
    pub events: &'a EventsPlan,
    /// The settings, which name a key setting in a defect.
    pub config: &'a ConfigPlan,
    /// The key settings' keys.
    pub keys: &'a Keys,
}

/// The key settings' keys, by the setting's number: what a `crypto` call's
/// `key: config.<name>` gives it (area 19), held apart from the values code
/// reads, since code never reads a key.
#[derive(Clone, Debug, Default)]
pub struct Keys(Vec<Option<KeyRing>>);

impl Keys {
    /// The keys the key settings of `plan` hold in `values`.
    pub fn read(plan: &ConfigPlan, values: &crate::config::ConfigValues) -> Keys {
        Keys(
            (0..plan.fields.len())
                .map(|at| crate::config::key_ring(plan, values, at as u32))
                .collect(),
        )
    }

    /// The key setting number `at` holds; `None` for one that isn't a key
    /// or has no value in this run.
    pub fn get(&self, at: u32) -> Option<&KeyRing> {
        self.0.get(at as usize)?.as_ref()
    }
}

/// Why an effect gave no value.
#[derive(Clone, Debug, PartialEq)]
pub enum Refused {
    /// A condition its call's `attempt` maps (area 10, question 7): its
    /// rule; how a report names it; and, for `mismatch`, the
    /// `DecodeProblem` a failure mapped from it receives, its first
    /// problem's field and rule, never the value (area 19, "Data
    /// formats").
    Broke {
        rule: RuleKey,
        written: String,
        problem: Option<Value>,
    },
    /// The operation's own fault, which the executor raises at its site.
    Fault(jadpo_value::Fault),
    /// What the checker or the plan rules out, named, for the executor to
    /// raise as a defect at its site.
    Defect(String),
}

/// Whether standard operation `op` is an effect, run by [`run`] rather
/// than by `jadpo_value::run`, which refuses it: `json.read` and
/// `json.write`, which take the plan's codec for their call; `crypto`'s
/// operations, but `read_certificate`, which needs no key; and `random`'s,
/// which read the generator.
pub fn is_effect(op: Op) -> bool {
    matches!(op, Op::JsonRead | Op::JsonWrite)
        || jadpo_value::crypto::is_crypto(op)
        || op.reads_generator()
}

/// Runs `effect` with `values` in `ctx`, with what `effects` holds of the
/// program: its value (`Unit` for `emit`), or why it gave none.
pub fn run(
    ctx: &mut Ctx<'_>,
    effects: &Effects<'_>,
    effect: Effect<'_>,
    values: &[Value],
) -> Result<Value, Refused> {
    match effect {
        Effect::Now => Ok(Value::Instant(ctx.now())),
        Effect::Emit { event } => {
            let [value] = values else {
                return Err(defect("an `emit` without its event"));
            };
            emit(ctx, effects, event, value.clone()).map(|()| Value::Unit)
        }
        // JSON in memory runs with the codec the plan numbers for its call
        // (area 19, "Data formats").
        Effect::Std {
            op: op @ (Op::JsonRead | Op::JsonWrite),
            json,
        } => self::json(effects, op, json, values),
        // `crypto` runs with the key its `key:` names and the invocation's
        // random bytes (area 19, "Hashing and cryptography").
        Effect::Std { op, json } if jadpo_value::crypto::is_crypto(op) => {
            crypto(ctx, effects, op, json, values)
        }
        // `random` reads the invocation's random bytes: the host's
        // generator, a test's seed or a recorded run's (area 19, question
        // 19).
        Effect::Std { op, .. } if op.reads_generator() => {
            jadpo_value::random::run(op, values, &mut |bytes: &mut [u8]| ctx.random_bytes(bytes))
                .map_err(Refused::Fault)
        }
        Effect::Std { .. } => Err(defect("an operation without effects run as one")),
    }
}

/// Why an outside call gave no value ([`call`]).
#[derive(Clone, Debug, PartialEq)]
pub enum Unanswered {
    /// One of its operation's own failures, by its code: the condition
    /// its call's `attempt` maps, or a fault where it maps none (area 11,
    /// "Calling").
    Failed(String),
    /// What the runtime raises in its place (area 4): `unavailable` when it
    /// surely did nothing, or may have acted but sending it again is safe;
    /// `outcome_unknown` when it may have acted and, with no key, sending
    /// it again could act twice; `misconfigured` when no driver fills its
    /// connection or its credential was refused.
    Failure(Box<Failure>),
    /// Refused before it went out, for the executor to raise at its site:
    /// inside an open transaction, or in a replay that no longer matches
    /// its recording.
    Fault(FaultKind),
}

/// Sends `call` from `ctx`, the one way a call leaves the process (chunk
/// 7.0c): its value, or why it gave none. What the running action has
/// only read is a snapshot, closed before the call goes out once the
/// versions of the rows it loaded are read (chunk 7.4); refused while a
/// transaction that has written is open (area 6, "Outside calls hold no
/// transaction"); in a replayed run, answered from the recording without
/// going out (area 17); otherwise sent through the driver the host gives
/// for its connection, logged on the invocation as it goes out and again
/// as it is answered ([`crate::capability::Logged`]), and its answer kept
/// while recording. The log never holds its input or its answer.
pub async fn call(ctx: &mut Ctx<'_>, call: Call) -> Result<Value, Unanswered> {
    match send(ctx, call).await? {
        Given::Value(value) => Ok(value),
        // Only a provider's operation is answered as it was sent, or with
        // its generated default.
        Given::Wire(_) | Given::Generated | Given::Recorded => Err(Unanswered::Fault(
            FaultKind::Defect("an answer as sent where a capability's value was expected".into()),
        )),
    }
}

/// What a call that was answered gives ([`send`]).
#[derive(Clone, Debug, PartialEq)]
pub enum Given {
    /// Its value: a capability's, or a fake's in a provider's place.
    Value(Value),
    /// A provider's answer as it was sent, for its site to decode (area
    /// 11, "Decoding"; chunk 7.2c).
    Wire(std::rc::Rc<str>),
    /// A provider operation's generated default, for its site to make
    /// from the operation's output type (chunk 7.2d).
    Generated,
    /// A provider operation's recording, for its site to fill (chunk
    /// 7.2d2).
    Recorded,
}

/// [`call`], giving a provider's answer as it was sent where its driver
/// answers so (chunk 7.2c).
pub async fn send(ctx: &mut Ctx<'_>, call: Call) -> Result<Given, Unanswered> {
    let repeat = call.repeat;
    answered(repeat, exchange(ctx, call).await?)
}

/// [`send`], giving the driver's answer as it classified it, for a caller
/// that decides whether to send it again (a provider's call, chunk 7.2c2):
/// refused inside an open transaction or with no driver, a replayed run's
/// recorded answer, or the driver's.
pub async fn exchange(ctx: &mut Ctx<'_>, call: Call) -> Result<Answer, Unanswered> {
    let now = ctx.now();
    // No transaction stays open while the call is out: one that has only
    // read closes, and the next store operation begins another (area 6,
    // "Outside calls hold no transaction"; chunk 7.4).
    if ctx.close_snapshot().await.is_err() {
        ctx.capabilities_mut().refused(&call, now);
        return Err(Unanswered::Failure(Box::new(crate::unavailable())));
    }
    if ctx.transaction_open() {
        ctx.capabilities_mut().refused(&call, now);
        return Err(Unanswered::Fault(FaultKind::CallInTransaction(call.name())));
    }
    if let Some(replayed) = ctx.capabilities_mut().replayed(&call) {
        return match replayed {
            Ok(answer) => {
                let capabilities = ctx.capabilities_mut();
                let place = capabilities.sent(&call, crate::capability::Source::Replayed, now);
                capabilities.ended(place, &answer, Some(0));
                Ok(answer)
            }
            Err(recorded) => {
                ctx.capabilities_mut().refused(&call, now);
                Err(Unanswered::Fault(FaultKind::Defect(format!(
                    "a replayed run calling `{}` where its recording has {recorded}",
                    call.name()
                ))))
            }
        };
    }
    let host = ctx.host();
    let Some(driver) = host.capability(&call.connection) else {
        ctx.capabilities_mut().refused(&call, now);
        return Err(Unanswered::Failure(Box::new(crate::misconfigured())));
    };
    // A step of an action with several changing calls is recorded before
    // it is sent (area 12, "The record"; chunk 7.5b).
    if let Some(at) = call.step
        && let Err(unsent) = crate::steps::sending(ctx, at).await
    {
        ctx.capabilities_mut().refused(&call, now);
        return Err(unsent);
    }
    let began = host.monotonic_ms();
    let place = ctx
        .capabilities_mut()
        .sent(&call, driver.source_of(&call), now);
    let answer = driver.call(&call).await;
    let took = began.and_then(|began| Some(host.monotonic_ms()?.saturating_sub(began)));
    ctx.capabilities_mut().ended(place, &answer, took);
    // A step answered, its answer not kept yet, which `jadpo test` may
    // crash after (chunk 7.5d).
    if call.step.is_some() && crate::crashes::due(ctx, crate::crashes::Boundary::AfterSend) {
        return Err(Unanswered::Fault(FaultKind::Crashed));
    }
    Ok(answer)
}

/// What `answer` gives a call that may be sent again as `repeat` says.
pub(crate) fn answered(repeat: Repeat, answer: Answer) -> Result<Given, Unanswered> {
    let raised = |failure: Failure| Err(Unanswered::Failure(Box::new(failure)));
    match answer {
        Answer::Value(value) => Ok(Given::Value(value)),
        Answer::Wire(text) => Ok(Given::Wire(text)),
        Answer::Generated => Ok(Given::Generated),
        Answer::Recorded => Ok(Given::Recorded),
        Answer::Failed(code) => Err(Unanswered::Failed(code)),
        Answer::Unavailable | Answer::RetryAfter(_) => raised(crate::unavailable()),
        // Sent again under its key, or a call that changes nothing, it
        // acts once: a retry is safe.
        Answer::Unknown | Answer::TimedOut if repeat != Repeat::Unkeyed => {
            raised(crate::unavailable())
        }
        Answer::Unknown | Answer::TimedOut => raised(crate::outcome_unknown()),
        Answer::Misconfigured => raised(crate::misconfigured()),
    }
}

fn defect(what: &str) -> Refused {
    Refused::Defect(what.to_string())
}

/// A condition its `attempt` maps, named as a construction's broken rule
/// is: `max:`.
fn broke(rule: RuleKey) -> Refused {
    Refused::Broke {
        rule,
        written: format!("{}:", rule.word()),
        problem: None,
    }
}

/// `json.read`'s `mismatch`, naming its first problem.
fn mismatch(problem: &Problem) -> Refused {
    Refused::Broke {
        rule: RuleKey::Mismatch,
        written: mismatch_written(problem),
        problem: Some(decode_problem(problem)),
    }
}

/// `emit`: `value`, an event of type `event`, kept in the running action's
/// outbox with its payload, its type's contract JSON, and the subscribers
/// it is delivered to once the action commits (area 12; chunk 6.1b).
fn emit(
    ctx: &mut Ctx<'_>,
    effects: &Effects<'_>,
    event: &str,
    value: Value,
) -> Result<(), Refused> {
    let Some(number) = effects
        .event_numbers
        .iter()
        .find(|known| known.key == event)
        .map(|known| known.number)
    else {
        return Err(defect("an event type the plan didn't number"));
    };
    let payload = match effects.json.write(number, &value) {
        Ok(SqlValue::Json(text)) => text,
        Ok(_) => return Err(defect("an event with no payload")),
        Err(why) => return Err(Refused::Defect(why)),
    };
    let subscribers = effects
        .events
        .hearing(event, None)
        .map(|subscriber| subscriber.key.clone())
        .collect();
    let event = Event {
        key: event.to_string(),
        value,
        payload,
        subscribers,
    };
    ctx.emit(event).map_err(|refused| match refused {
        EmitRefused::OutsideAction => defect("an `emit` outside an action"),
        EmitRefused::TooMany => defect("more than 1,000 events emitted by one action"),
        EmitRefused::TooLarge => defect("an event past 64 KB"),
    })
}

/// `json.read` or `json.write` (area 19, "Data formats"), through the
/// wire's codec for the type its site names. A document that isn't JSON
/// is `malformed`, and one that isn't the type `mismatch`, each for the
/// call's `attempt` to map; the report names the first problem's field
/// and rule, never the value.
fn json(
    effects: &Effects<'_>,
    op: Op,
    number: Option<u32>,
    values: &[Value],
) -> Result<Value, Refused> {
    let Some(codec) = number.and_then(|number| effects.json.types.get(number as usize).copied())
    else {
        return Err(defect("a JSON call the plan gave no codec"));
    };
    let codecs = effects.json.codecs;
    match (op, values) {
        (Op::JsonWrite, [value]) => Ok(Value::text(&codecs.write_value(codec, value))),
        (Op::JsonRead, [text, _]) => {
            codecs
                .read_value(codec, text)
                .map_err(|unread| match unread {
                    Unread::Malformed => Refused::Broke {
                        rule: RuleKey::Malformed,
                        written: format!("`{}`", RuleKey::Malformed.word()),
                        problem: None,
                    },
                    Unread::Mismatch(problem) => mismatch(&problem),
                })
        }
        _ => Err(defect("a JSON call without its values")),
    }
}

/// A `crypto` operation (area 19, "Hashing and cryptography"), in
/// `jadpo-value`'s `crypto` module: given the key ring its `key:` names by
/// the setting's number, never the key as a value, and the invocation's
/// random bytes for a nonce or a salt. `decrypt`'s `unreadable` is for its
/// `attempt` to map.
fn crypto(
    ctx: &mut Ctx<'_>,
    effects: &Effects<'_>,
    op: Op,
    json: Option<u32>,
    values: &[Value],
) -> Result<Value, Refused> {
    let key_at = op
        .operation()
        .params
        .iter()
        .position(|param| matches!(param.kind, Kind::Builtin(builtin) if builtin.is_key()));
    let keys = match key_at.map(|place| values.get(place)) {
        None => None,
        Some(Some(Value::Int(setting))) => {
            let setting = u32::try_from(*setting).unwrap_or(u32::MAX);
            match effects.keys.get(setting) {
                Some(keys) => Some(keys),
                None => {
                    let name = effects
                        .config
                        .fields
                        .get(setting as usize)
                        .map(|field| field.name.as_str())
                        .unwrap_or_default();
                    return Err(Refused::Defect(format!(
                        "the key `config.{name}` has no value in this run"
                    )));
                }
            }
        }
        Some(_) => return Err(defect("a key that isn't read from its setting")),
    };
    // `verify_jwt` checks a token's times against the invocation's clock,
    // as `clock.now` reads it (area 19, chunk 5.5r).
    let now = if op.reads_clock() { ctx.now() } else { 0 };
    // With `as:`, its claims of the application's own type are read with
    // the codec the plan numbers for the call (chunk 5.5r4).
    if op == Op::CryptoVerifyJwt
        && let Some(number) = json
    {
        return claims(effects, values, number, now);
    }
    // An encryption's nonce is secret randomness, never kept on a
    // recording's tape nor taken from a replayed one, so a replay never
    // seals with a nonce production used under the same key (chunk 5.8a);
    // a password's salt is the invocation's random bytes, as `random`'s
    // are.
    let secret = op == Op::CryptoEncrypt;
    let made = jadpo_value::crypto::run(
        op,
        values,
        keys,
        &mut |bytes: &mut [u8]| {
            if secret {
                ctx.secret_random_bytes(bytes)
            } else {
                ctx.random_bytes(bytes)
            }
        },
        now,
    );
    made.map_err(|fault| match fault {
        jadpo_value::Fault::Broke(rule) => broke(rule),
        fault => Refused::Fault(fault),
    })
}

/// `crypto.verify_jwt(..., as:)` (chunk 5.5r4): the token checked as
/// without `as:`, then its own claims read through the codec for the type
/// `as:` names, `mismatch` naming the first problem's field and rule,
/// never the value, as `json.read`'s does.
fn claims(
    effects: &Effects<'_>,
    values: &[Value],
    number: u32,
    now: i64,
) -> Result<Value, Refused> {
    let codec = effects.json.types.get(number as usize).copied();
    let (Some(codec), [token, Value::List(keys), issuer, audience, ..]) = (codec, values) else {
        return Err(defect("a token's claims the plan gave no codec"));
    };
    let token = match token {
        Value::Secret(secret) => secret.expose().to_string(),
        Value::Text(text) => text.to_string(),
        _ => return Err(defect("a token that isn't a `Jwt`")),
    };
    let option = |value: &Value| match value {
        Value::Text(text) => Some(text.to_string()),
        _ => None,
    };
    let read = jadpo_value::jwt::verify_jwt_as(
        &token,
        keys,
        option(issuer).as_deref(),
        option(audience).as_deref(),
        now,
        &effects.json.codecs,
        codec,
    );
    read.map_err(|refusal| match refusal {
        jadpo_value::jwt::Refusal::Token(jadpo_value::Fault::Broke(rule)) => broke(rule),
        jadpo_value::jwt::Refusal::Token(fault) => Refused::Fault(fault),
        jadpo_value::jwt::Refusal::Mismatch(problem) => mismatch(&problem),
    })
}

/// `json.read`'s first problem as a `DecodeProblem` (area 19, "Data
/// formats"): its field's path and its rule, never the value.
fn decode_problem(problem: &Problem) -> Value {
    let [(field, _), (rule, _)] = jadpo_std::http::DECODE_PROBLEM_FIELDS;
    Value::record(
        jadpo_std::http::DECODE_PROBLEM,
        vec![
            (Rc::from(field), Value::text(&problem.field)),
            (Rc::from(rule), Value::text(&problem.rule)),
        ],
    )
}

/// How a report names `json.read`'s first problem: "`mismatch` (`max` on
/// `lines.2.quantity`)", or, for the document itself, "`mismatch`
/// (`type`)".
fn mismatch_written(problem: &Problem) -> String {
    let word = RuleKey::Mismatch.word();
    if problem.field.is_empty() {
        format!("`{word}` (`{}`)", problem.rule)
    } else {
        format!("`{word}` (`{}` on `{}`)", problem.rule, problem.field)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_effects_are_json_crypto_with_a_key_and_random() {
        for operation in jadpo_std::OPERATIONS {
            let op = operation.op;
            let expected = operation.namespace == "random"
                || operation.namespace == "json"
                || (operation.namespace == "crypto" && op != Op::CryptoReadCertificate);
            assert_eq!(is_effect(op), expected, "{}", operation.qualified());
        }
    }
}
