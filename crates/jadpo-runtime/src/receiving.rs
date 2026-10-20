//! Queues received as events (area 12, "Events from outside"; area 11,
//! "Queues"; chunk 7.7c): each queue `receiving:` names is fetched by the
//! tick through its connection's package, as its `provides/receiving.jadpo`
//! maps the capability's two operations onto the package's own.
//!
//! - **Each message is its event.** Its body is JSON; its id is the text
//!   or whole number at `id_at:`, and its type the text at `type_at:`,
//!   matched against each listed event's `wire_name:` (a queue of one kind
//!   has no type: every message is its one event). The event's fields are
//!   read from the body's root with the event's codec, as a webhook's are.
//! - **Kept once, then acknowledged.** In one transaction in the events'
//!   store, the id is recorded under `queue <connection>` in
//!   `jadpo_received_events` and the event's outbox row and a delivery for
//!   each subscriber are written, as a webhook's are; only once that
//!   commits is the message acknowledged, so a message is never lost
//!   between the queue and the outbox. A repeat, its id already recorded,
//!   is acknowledged and nothing kept. The subscribers then run from the
//!   outbox with the usual retries, and one that gives up is parked as any
//!   delivery is.
//! - **Parked, never dropped.** A message whose type the queue doesn't
//!   list, one that can't be read as its event (not JSON, no id or type
//!   where the queue says, a field its event refuses), and one over 64 KB
//!   as its event are kept whole in `jadpo_parked_messages`, once per
//!   queue and body, and acknowledged: `jadpo deliveries` shows each as
//!   `failed`, and an operator retries it once the package or the app is
//!   fixed (the next tick reads it again) or dismisses it. A message the
//!   store can't take now is left unacknowledged, so the queue sends it
//!   again, and the queue's own dead-letter queue takes one that keeps
//!   failing.
//!
//! Every message is read by the build that receives it, and a parked
//! message retried is read by the build running then.

use jadpo_std::provider::CallSite;
use jadpo_std::receiving::{Mapped, QueueEvent, QueuePlan};
use jadpo_value::Value;
use jadpo_value::json::Problem;

use crate::budget::Budget;
use crate::outbox::{Delivery, micros};
use crate::provider::{Done, Functions, Providers, Site};
use crate::runner::Deferred;
use crate::webhooks::{Kept, keep, read};
use crate::{Clock, Ctx, Env, Executor, Host, SqlType, SqlValue, StoreError};

/// The most messages one receive asks for, and the most receives a queue
/// is given on one tick while each brings messages.
pub const BATCH: i64 = 10;
pub const ROUNDS: usize = 10;

/// The most parked messages an operator retried that one tick reads again.
pub const RETRIED_PER_TICK: i64 = 20;

/// How long a parked message an operator dismissed is kept, as a
/// dismissed delivery is: 30 days.
pub const DISMISSED_KEPT_MS: i64 = 30 * 24 * 60 * 60 * 1000;

/// Why a message is parked, as `jadpo deliveries` shows it.
pub const UNLISTED_TYPE: &str = "unlisted_type";
pub const UNREADABLE: &str = "unreadable";
pub const TOO_LARGE: &str = "too_large";

/// Parks a message: once per queue and body, so one sent again isn't a
/// second item. One an operator retried that parks again is `failed`
/// again with the reason found this time.
pub const PARK: &str = "INSERT INTO \"jadpo_parked_messages\" (\"id\", \"queue\", \"digest\", \"body\", \"status\", \"reason\", \"event\", \"message_id\", \"problems\", \"parked_at\") VALUES ($1, $2, $3, $4, 'failed', $5, $6, $7, $8, $9) ON CONFLICT (\"queue\", \"digest\") DO UPDATE SET \"status\" = 'failed', \"reason\" = EXCLUDED.\"reason\", \"event\" = EXCLUDED.\"event\", \"message_id\" = EXCLUDED.\"message_id\", \"problems\" = EXCLUDED.\"problems\" WHERE \"jadpo_parked_messages\".\"status\" = 'pending'";

/// The parked messages of queue `$1` an operator retried, oldest first, at
/// most `$2`.
pub const RETRIED: &str = "SELECT \"id\", \"body\" FROM \"jadpo_parked_messages\" WHERE \"queue\" = $1 AND \"status\" = 'pending' ORDER BY \"parked_at\", \"id\" LIMIT $2";

/// A retried parked message, once its event is kept.
pub const DONE: &str =
    "DELETE FROM \"jadpo_parked_messages\" WHERE \"id\" = $1 AND \"status\" = 'pending'";

/// Parked messages an operator dismissed at or before `$1`, at most `$2`,
/// for the tick.
pub const OLD_DISMISSED: &str = "DELETE FROM \"jadpo_parked_messages\" WHERE \"id\" IN (SELECT \"m\".\"id\" FROM \"jadpo_parked_messages\" AS \"m\" WHERE \"m\".\"status\" = 'dismissed' AND NOT EXISTS (SELECT 1 FROM \"jadpo_operator_actions\" AS \"a\" WHERE \"a\".\"item\" = \"m\".\"id\" AND \"a\".\"acted_at\" > $1) LIMIT $2) RETURNING 1";

/// An operator's log rows on parked messages, written at or before `$1`,
/// whose message is gone, at most `$2`.
pub const OLD_MESSAGE_ACTIONS: &str = "DELETE FROM \"jadpo_operator_actions\" WHERE \"id\" IN (SELECT \"a\".\"id\" FROM \"jadpo_operator_actions\" AS \"a\" WHERE \"a\".\"kind\" = 'message' AND \"a\".\"acted_at\" <= $1 AND NOT EXISTS (SELECT 1 FROM \"jadpo_parked_messages\" AS \"m\" WHERE \"m\".\"id\" = \"a\".\"item\") LIMIT $2) RETURNING 1";

/// What the runtime receives a program's queues with: their plan, and how
/// a call through their connections is made.
#[derive(Clone, Copy, Debug)]
pub struct Receiving<'a> {
    pub queues: &'a [QueuePlan],
    pub providers: Providers<'a>,
}

/// What became of one message.
#[derive(Debug)]
pub enum Taken {
    /// Its event was kept, with the deliveries to make.
    Kept(Vec<Delivery>),
    /// Its id was received already: nothing kept.
    Repeat,
    /// It was parked for an operator, for this reason.
    Parked(&'static str),
    /// The store couldn't take it now: it is left for the queue to send
    /// again.
    Left(StoreError),
}

impl Taken {
    /// Whether the queue may forget it.
    pub fn acknowledged(&self) -> bool {
        !matches!(self, Taken::Left(_))
    }
}

/// What one tick received.
#[derive(Debug, Default)]
pub struct Received {
    /// The work its kept events left: their deliveries.
    pub deferred: Vec<Deferred>,
    /// How many messages were kept, repeated, parked and left.
    pub kept: u64,
    pub repeats: u64,
    pub parked: u64,
    pub left: u64,
    /// Each queue or store the tick couldn't receive from, with why.
    pub problems: Vec<(String, StoreError)>,
}

impl Received {
    fn count(&mut self, taken: Taken) {
        match taken {
            Taken::Kept(deliveries) => {
                self.kept += 1;
                if !deliveries.is_empty() {
                    self.deferred.push(Deferred::Deliveries(deliveries));
                }
            }
            Taken::Repeat => self.repeats += 1,
            Taken::Parked(_) => self.parked += 1,
            Taken::Left(_) => self.left += 1,
        }
    }
}

/// Receives every queue of `executor`'s on the tick at `now`, within
/// `budget`: first the parked messages an operator retried, then the
/// queue's messages, a batch at a time while each brings some.
pub async fn receive(executor: &dyn Executor, budget: &Budget<'_>, now: i64) -> Received {
    let mut out = Received::default();
    let Some(receiving) = executor.receiving() else {
        return out;
    };
    let host = budget.host();
    for queue in receiving.queues {
        if budget.spent() {
            break;
        }
        retried(executor, &receiving, queue, host, now, &mut out).await;
        for _ in 0..ROUNDS {
            if budget.spent() {
                break;
            }
            let mut ctx = system_ctx(executor, host, now);
            let messages = match fetch(executor, &receiving, queue, &mut ctx).await {
                Ok(messages) => messages,
                Err(why) => {
                    out.problems.push((queue.source(), why));
                    break;
                }
            };
            let fetched = messages.len();
            for (body, handle) in messages {
                let taken = take(executor, queue, &mut ctx, &body).await;
                if taken.acknowledged() {
                    acknowledge(executor, &receiving, queue, &mut ctx, &handle).await;
                }
                if let Taken::Left(error) = &taken {
                    out.problems.push((queue.store.clone(), error.clone()));
                }
                out.count(taken);
            }
            if (fetched as i64) < BATCH {
                break;
            }
        }
    }
    out
}

/// A context for the runtime's own work at `now`: no request, no seed.
fn system_ctx<'p>(executor: &'p dyn Executor, host: &'p dyn Host, now: i64) -> Ctx<'p> {
    Ctx::new(
        executor.places(),
        host,
        Env {
            seed: None,
            clock: Clock::fixed(now),
        },
    )
}

/// Reads again each parked message of `queue` an operator retried: kept,
/// it is done; parked again, it is `failed` again with what was found.
async fn retried(
    executor: &dyn Executor,
    _receiving: &Receiving<'_>,
    queue: &QueuePlan,
    host: &dyn Host,
    now: i64,
    out: &mut Received,
) {
    let Some(database) = host.database(&queue.store) else {
        return;
    };
    let rows = match database
        .query_once(
            RETRIED,
            &[
                SqlValue::Text(queue.connection.clone()),
                SqlValue::Int8(RETRIED_PER_TICK),
            ],
        )
        .await
    {
        Ok(rows) => rows,
        Err(error) => {
            out.problems.push((queue.store.clone(), error));
            return;
        }
    };
    for row in rows {
        let [SqlValue::Uuid(id), SqlValue::Text(body)] = row.as_slice() else {
            continue;
        };
        let mut ctx = system_ctx(executor, host, now);
        let taken = take(executor, queue, &mut ctx, body).await;
        if matches!(taken, Taken::Kept(_) | Taken::Repeat) {
            let _ = database.query_once(DONE, &[SqlValue::Uuid(*id)]).await;
        }
        out.count(taken);
    }
}

/// Runs nothing: a queue's package is called with no functions of its
/// own (the checker keeps one with `decode:` out of a mapping).
struct NoFunctions;

impl Functions for NoFunctions {
    fn function(
        &self,
        _ctx: &mut Ctx<'_>,
        function: &str,
        _args: Vec<Value>,
        _at: u32,
    ) -> Result<Value, String> {
        Err(format!(
            "`{function}`, a package function, outside a call's site"
        ))
    }
}

/// The values of the settings a call through `queue`'s connection reads,
/// for its address, as the program started with them.
fn settings_of(
    executor: &dyn Executor,
    receiving: &Receiving<'_>,
    queue: &QueuePlan,
) -> Vec<(u32, Value)> {
    receiving
        .providers
        .plan
        .connections
        .iter()
        .find(|connection| connection.name == queue.connection)
        .map(crate::provider::settings_read)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|at| Some((at, executor.setting(at)?)))
        .collect()
}

/// An operation's input from its mapping, as the provider's JSON, read
/// back with the input's codec as the call writes it; `handle` for
/// `message.handle`.
fn input(
    executor: &dyn Executor,
    receiving: &Receiving<'_>,
    queue: &QueuePlan,
    operation: &str,
    mapped: &[(String, Mapped)],
    handle: Option<&str>,
) -> Result<Option<Value>, StoreError> {
    let Some((_, op)) = receiving
        .providers
        .plan
        .operation(&queue.connection, operation)
    else {
        return Err(StoreError::Fault(format!(
            "`{}.{operation}`, which the plan doesn't have",
            queue.connection
        )));
    };
    let Some(number) = op.input else {
        return Ok(None);
    };
    let mut members = serde_json::Map::new();
    for (name, value) in mapped {
        let json = match value {
            Mapped::Text(text) => serde_json::Value::from(text.as_str()),
            Mapped::Int(n) => serde_json::Value::from(*n),
            Mapped::Bool(flag) => serde_json::Value::from(*flag),
            Mapped::Handle => serde_json::Value::from(handle.unwrap_or_default()),
            Mapped::Setting(at) => match executor.setting(*at) {
                Some(Value::Text(text)) => serde_json::Value::from(&*text),
                Some(Value::Int(n)) => serde_json::Value::from(n),
                Some(Value::Bool(flag)) => serde_json::Value::from(flag),
                // A setting nothing gives: the connection is misconfigured.
                _ => return Err(StoreError::Unavailable),
            },
        };
        members.insert(name.clone(), json);
    }
    let body = serde_json::Value::Object(members);
    let text = body.to_string();
    let Some((json, numbers)) = jadpo_value::json::parse(text.as_bytes()) else {
        return Err(StoreError::Fault(
            "a mapping's input that isn't JSON".into(),
        ));
    };
    read(receiving.providers.json, number, &json, &numbers)
        .map(Some)
        .map_err(|problems| {
            StoreError::Fault(format!(
                "`{}.{operation}`'s mapped input doesn't read: {}",
                queue.connection,
                described(&problems)
            ))
        })
}

/// Problems by field and rule, never a value.
fn described(problems: &[Problem]) -> String {
    problems
        .iter()
        .map(|problem| format!("{} {}", problem.field, problem.rule))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Calls `operation` of `queue`'s connection with `input`: its output, or
/// why it gave none.
async fn call(
    executor: &dyn Executor,
    receiving: &Receiving<'_>,
    queue: &QueuePlan,
    ctx: &mut Ctx<'_>,
    operation: &str,
    input: Option<Value>,
) -> Result<Value, StoreError> {
    let settings = settings_of(executor, receiving, queue);
    let site = CallSite::default();
    let done = crate::provider::call(
        ctx,
        &receiving.providers,
        (&queue.connection, operation),
        input,
        &settings,
        Site {
            call: &site,
            functions: &NoFunctions,
            at: 0,
        },
    )
    .await;
    match done {
        Done::Value(value) => Ok(value),
        Done::Failed(failure) => Err(StoreError::Fault(format!(
            "`{}.{operation}` failed: {}",
            queue.connection, failure.code
        ))),
        // A provider's fault names the operation, the field and the rule,
        // never a value (area 11, "Decoding").
        Done::Fault(crate::FaultKind::Provider(why)) => Err(StoreError::Fault(why)),
        Done::Fault(kind) => Err(StoreError::Fault(format!(
            "`{}.{operation}` ended in a fault: {}",
            queue.connection,
            kind.classification()
        ))),
    }
}

/// One batch of `queue`'s messages: each one's body and handle.
async fn fetch(
    executor: &dyn Executor,
    receiving: &Receiving<'_>,
    queue: &QueuePlan,
    ctx: &mut Ctx<'_>,
) -> Result<Vec<(String, String)>, StoreError> {
    let plan = &queue.receive;
    let given = input(
        executor,
        receiving,
        queue,
        &plan.operation,
        &plan.input,
        None,
    )?;
    let answer = call(executor, receiving, queue, ctx, &plan.operation, given).await?;
    let list = match at(&answer, &plan.messages) {
        Some(Value::List(items)) => items.to_vec(),
        // A provider that leaves the list out when it is empty.
        Some(Value::None) | None => Vec::new(),
        Some(_) => {
            return Err(StoreError::Fault(format!(
                "`{}.{}`'s messages aren't a list",
                queue.connection, plan.operation
            )));
        }
    };
    Ok(list
        .iter()
        .filter_map(|message| {
            let text = |path: &[String]| match at(message, path) {
                Some(Value::Text(text)) => Some(text.to_string()),
                _ => None,
            };
            Some((text(&plan.body)?, text(&plan.handle)?))
        })
        .collect())
}

/// The value at `path`, by field names, in `value`.
fn at<'v>(value: &'v Value, path: &[String]) -> Option<&'v Value> {
    path.iter().try_fold(value, |at, name| match at {
        Value::Record(record) => record.get(name),
        _ => None,
    })
}

/// Acknowledges the message `handle` names: what goes wrong is left to
/// the queue, which sends it again to be dropped as a repeat.
async fn acknowledge(
    executor: &dyn Executor,
    receiving: &Receiving<'_>,
    queue: &QueuePlan,
    ctx: &mut Ctx<'_>,
    handle: &str,
) {
    let plan = &queue.acknowledge;
    let Ok(given) = input(
        executor,
        receiving,
        queue,
        &plan.operation,
        &plan.input,
        Some(handle),
    ) else {
        return;
    };
    let _ = call(executor, receiving, queue, ctx, &plan.operation, given).await;
}

/// What a message of `queue` with `body` becomes, in `ctx`.
pub async fn take(
    executor: &dyn Executor,
    queue: &QueuePlan,
    ctx: &mut Ctx<'_>,
    body: &str,
) -> Taken {
    let Some(events) = executor.events() else {
        return Taken::Left(StoreError::Fault("a program with no events".into()));
    };
    let json = events.json;
    let mut parked = Parked {
        queue,
        body,
        event: None,
        message_id: None,
        problems: Vec::new(),
    };
    let Some((parsed, numbers)) = jadpo_value::json::parse(body.as_bytes()) else {
        return parked.park(ctx, UNREADABLE).await;
    };
    let id = match crate::provider::at_path(&parsed, &queue.id_at) {
        Some(serde_json::Value::String(text)) if !text.is_empty() => text.clone(),
        Some(serde_json::Value::Number(number)) if number.is_u64() || number.is_i64() => {
            number.to_string()
        }
        _ => {
            parked.problems.push(Problem {
                field: queue.id_at.clone(),
                rule: "required".into(),
            });
            return parked.park(ctx, UNREADABLE).await;
        }
    };
    parked.message_id = Some(id.clone());
    let event: &QueueEvent = match &queue.type_at {
        None => match queue.events.first() {
            Some(event) => event,
            None => return parked.park(ctx, UNLISTED_TYPE).await,
        },
        Some(type_at) => {
            let Some(serde_json::Value::String(kind)) = crate::provider::at_path(&parsed, type_at)
            else {
                parked.problems.push(Problem {
                    field: type_at.clone(),
                    rule: "required".into(),
                });
                return parked.park(ctx, UNREADABLE).await;
            };
            match queue
                .events
                .iter()
                .find(|event| event.wire_name.as_deref() == Some(kind.as_str()))
            {
                Some(event) => event,
                None => return parked.park(ctx, UNLISTED_TYPE).await,
            }
        }
    };
    parked.event = Some(event.key.clone());
    let value = match lenient(json, event.number, parsed, &numbers) {
        Ok(value) => value,
        Err(problems) => {
            parked.problems = problems;
            return parked.park(ctx, UNREADABLE).await;
        }
    };
    let payload = match json.write(event.number, &value) {
        Ok(SqlValue::Json(text)) => text,
        _ => return parked.park(ctx, UNREADABLE).await,
    };
    if payload.len() > crate::outbox::MAX_EVENT_BYTES {
        return parked.park(ctx, TOO_LARGE).await;
    }
    let trace = ctx.new_trace();
    ctx.set_trace(Some(trace));
    let kept = Kept {
        store: &queue.store,
        source: queue.source(),
        event: &event.key,
        connection: None,
    };
    match keep(executor, &kept, ctx, &id, value, payload).await {
        Ok(Some(deliveries)) => Taken::Kept(deliveries),
        Ok(None) => Taken::Repeat,
        Err(error) => Taken::Left(error),
    }
}

/// The value `body` holds as the event numbered `number`, every member
/// its event doesn't declare left out, as a webhook's are: the queue's own
/// type and id among them, and whatever a sender adds.
fn lenient(
    json: crate::JsonCodecs<'_>,
    number: u32,
    mut body: serde_json::Value,
    numbers: &jadpo_value::json::Numbers,
) -> Result<Value, Vec<Problem>> {
    match read(json, number, &body, numbers) {
        Err(problems)
            if problems
                .iter()
                .any(|problem| problem.rule == "unknown_field") =>
        {
            if let serde_json::Value::Object(members) = &mut body {
                for problem in &problems {
                    if problem.rule == "unknown_field" && !problem.field.contains(['.', '[']) {
                        members.remove(&problem.field);
                    }
                }
            }
            read(json, number, &body, numbers)
        }
        read => read,
    }
}

/// A message to park, with what was read of it.
struct Parked<'a> {
    queue: &'a QueuePlan,
    body: &'a str,
    event: Option<String>,
    message_id: Option<String>,
    problems: Vec<Problem>,
}

impl Parked<'_> {
    async fn park(self, ctx: &mut Ctx<'_>, reason: &'static str) -> Taken {
        let now = ctx.now();
        let Some(id) = ctx.new_id() else {
            return Taken::Left(StoreError::Fault(
                "no random bytes for a parked message's id".into(),
            ));
        };
        let Some(database) = ctx.capabilities().database(&self.queue.store) else {
            return Taken::Left(StoreError::Fault(format!(
                "no store called `{}`",
                self.queue.store
            )));
        };
        let digest: String = jadpo_value::hash::sha256(self.body.as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        let text =
            |value: Option<String>| value.map_or(SqlValue::Null(SqlType::Text), SqlValue::Text);
        let problems = if self.problems.is_empty() {
            SqlValue::Null(SqlType::Json)
        } else {
            SqlValue::Json(
                serde_json::Value::Array(
                    self.problems
                        .iter()
                        .map(|problem| {
                            serde_json::json!({ "field": problem.field, "rule": problem.rule })
                        })
                        .collect(),
                )
                .to_string(),
            )
        };
        let parked = database
            .query_once(
                PARK,
                &[
                    SqlValue::Uuid(id),
                    SqlValue::Text(self.queue.connection.clone()),
                    SqlValue::Text(digest),
                    SqlValue::Text(self.body.to_string()),
                    SqlValue::Text(reason.to_string()),
                    text(self.event),
                    text(self.message_id),
                    problems,
                    micros(now),
                ],
            )
            .await;
        match parked {
            Ok(_) => Taken::Parked(reason),
            Err(error) => Taken::Left(error),
        }
    }
}
