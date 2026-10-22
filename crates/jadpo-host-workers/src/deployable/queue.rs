//! The Worker's `queue` (chunk 8.2k; area 11, "Chosen in chunk 7.7c2";
//! area 14, "A Workers deployable"): the consumer of each queue on
//! Cloudflare Queues the deployable receives, which Cloudflare hands a
//! batch of messages at a time.
//!
//! It starts the isolate as `fetch` does and, once its stores are current,
//! hands the batch to the runtime's seam, [`receiving::consume`]: the
//! queue's name and each message's body as text, with how many times it
//! has been handed over. Each message is then answered on its own, never
//! the batch whole: `ack()` once its event is kept, as a repeat or parked;
//! `retry({ delaySeconds })` when the store couldn't take it now, the
//! delay growing with each attempt, so Cloudflare hands it again and moves
//! it to the queue's dead-letter queue once its retries are spent. Only
//! then are the kept events' deliveries run, in `waitUntil` within the 30
//! seconds after a response, what they don't reach handed on to a fresh
//! invocation as a response's work is ([`runner::after_response`]).
//!
//! An isolate that can't start, or whose stores aren't current, retries
//! each message after the same delay: nothing is kept until the stores are
//! the build's, and a message is never acknowledged before it is kept.
//! Each message retried is logged once a batch as `queue.retried`, with
//! the queue's name, how many, and why by a code, never a body.
//!
//! The Worker consumes each such queue's dead-letter queue as well (chunk
//! 8.4b), the runtime parking each message it hands back for an operator.
//! A message whose answer can't be given, its `retry()` throwing, would be
//! acknowledged by Cloudflare once the handler ended well, so the handler
//! then throws once every message has been answered: Cloudflare hands
//! again each one not acknowledged ([`answer_messages`]).

use std::rc::Rc;

use jadpo_runtime::receiving::{self, Answer, Pushed, retry_delay};
use jadpo_runtime::{Executor, Host, runner};
use js_sys::{Array, Function, Object, Reflect, Uint8Array};
use serde_json::Value;
use wasm_bindgen::{JsCast, JsValue};

use super::{Deployable, drivers, isolate, ready};
use crate::WorkersHost;

/// Why a batch's messages were retried, as `queue.retried` gives it.
const NOT_STARTED: &str = "not_started";
const NOT_RECEIVED: &str = "not_received";
const STORE_UNAVAILABLE: &str = "store_unavailable";

/// The Worker's `queue` for `deployable`: a promise that settles once
/// every message of `batch` has been answered, the kept events'
/// deliveries going on after it in `waitUntil`, and rejects when a
/// message's answer couldn't be given ([`answer_messages`]).
pub fn queue(deployable: &Deployable, batch: JsValue, env: JsValue, ctx: JsValue) -> JsValue {
    let deployable = *deployable;
    wasm_bindgen_futures::future_to_promise(async move {
        consume(&deployable, &batch, env, ctx).await?;
        Ok(JsValue::UNDEFINED)
    })
    .into()
}

/// One batch, on a host of its own: the error to end the handler with
/// when a message couldn't be answered.
async fn consume(
    deployable: &Deployable,
    batch: &JsValue,
    env: JsValue,
    ctx: JsValue,
) -> Result<(), JsValue> {
    let name = Reflect::get(batch, &JsValue::from_str("queue"))
        .ok()
        .and_then(|name| name.as_string())
        .unwrap_or_default();
    let messages: Vec<JsValue> = Reflect::get(batch, &JsValue::from_str("messages"))
        .ok()
        .filter(Array::is_array)
        .map(|messages| messages.unchecked_into::<Array>().iter().collect())
        .unwrap_or_default();
    let pushed: Vec<Pushed> = messages.iter().map(pushed).collect();
    let host = WorkersHost::new(env, ctx);
    let Some(isolate) = isolate(deployable, &host) else {
        return retried(&messages, &pushed, None, &name, NOT_STARTED);
    };
    let drivers = drivers(&isolate.program, &host);
    let host = host.with_drivers(drivers);
    if !ready(deployable, &host, &isolate).await {
        return retried(&messages, &pushed, Some(&isolate.log), &name, NOT_STARTED);
    }
    let Some(now) = host.now_ms() else {
        return retried(&messages, &pushed, Some(&isolate.log), &name, NOT_STARTED);
    };
    let program = Rc::clone(&isolate.program);
    let received = program.plan().receiving().is_some_and(|receiving| {
        receiving.queues.iter().any(|queue| {
            matches!(
                &queue.source,
                jadpo_std::receiving::QueueSource::Cloudflare(cloudflare)
                    if cloudflare.queue == name || cloudflare.dead_letter_queue == name
            )
        })
    });
    let consumed = receiving::consume(program.as_ref(), &host, &name, &pushed, now).await;
    let retries = consumed
        .answers
        .iter()
        .filter(|answer| matches!(answer, Answer::Retry { .. }))
        .count();
    let answered = answer_messages(&messages, &consumed.answers);
    if retries > 0 {
        let why = if received {
            STORE_UNAVAILABLE
        } else {
            NOT_RECEIVED
        };
        logged(&isolate.log, &name, retries, why);
    }
    runner::after_response(
        program as Rc<dyn Executor>,
        Rc::new(host) as Rc<dyn Host>,
        consumed.received.deferred,
    );
    answered
}

/// Retries every message of a batch nothing could be kept from, each
/// after the delay its attempts give, and logs it when there is a log.
fn retried(
    messages: &[JsValue],
    pushed: &[Pushed],
    log: Option<&jadpo_runtime::deployable::Log>,
    name: &str,
    why: &str,
) -> Result<(), JsValue> {
    let answers: Vec<Answer> = pushed
        .iter()
        .map(|pushed| Answer::Retry {
            delay_seconds: retry_delay(pushed.attempts),
        })
        .collect();
    let answered = answer_messages(messages, &answers);
    if let Some(log) = log
        && !messages.is_empty()
    {
        logged(log, name, messages.len(), why);
    }
    answered
}

/// `queue.retried`: the queue, how many of its batch's messages were
/// retried, and why.
fn logged(log: &jadpo_runtime::deployable::Log, name: &str, retried: usize, why: &str) {
    log.event(
        "queue.retried",
        [
            ("queue", Value::from(name)),
            ("retried", Value::from(retried)),
            ("reason", Value::from(why)),
        ],
    );
}

/// Answers each of `messages` with its answer of `answers`, in order:
/// `ack()`, or `retry({ delaySeconds })`. A message whose answer couldn't
/// be given, its method missing or throwing, is an error once every other
/// has been answered: the handler ends with it, so Cloudflare hands again
/// every message not acknowledged rather than acknowledging the batch's
/// unanswered ones as a handler that ends well does.
pub fn answer_messages(messages: &[JsValue], answers: &[Answer]) -> Result<(), JsValue> {
    let mut unanswered = 0usize;
    for (message, answer) in messages.iter().zip(answers) {
        if !answered(message, *answer) {
            unanswered += 1;
        }
    }
    if unanswered == 0 {
        return Ok(());
    }
    Err(js_sys::Error::new(&format!(
        "{unanswered} of the batch's messages couldn't be answered: Cloudflare hands again each one not acknowledged"
    ))
    .into())
}

/// Answers `message`: `ack()`, or `retry({ delaySeconds })`; whether it
/// was given.
fn answered(message: &JsValue, answer: Answer) -> bool {
    let (method, argument) = match answer {
        Answer::Ack => ("ack", JsValue::UNDEFINED),
        Answer::Retry { delay_seconds } => {
            let options = Object::new();
            let _ = Reflect::set(
                &options,
                &JsValue::from_str("delaySeconds"),
                &JsValue::from(delay_seconds),
            );
            ("retry", options.into())
        }
    };
    let Ok(function) = Reflect::get(message, &JsValue::from_str(method)) else {
        return false;
    };
    function
        .dyn_ref::<Function>()
        .is_some_and(|function| function.call1(message, &argument).is_ok())
}

/// A message as the runtime's seam takes it: its body as text, and how
/// many times Cloudflare has handed it over, this time among them.
fn pushed(message: &JsValue) -> Pushed {
    let attempts = Reflect::get(message, &JsValue::from_str("attempts"))
        .ok()
        .and_then(|attempts| attempts.as_f64())
        .filter(|attempts| attempts.is_finite() && *attempts >= 1.0)
        // A count of hand-overs, well inside a `u32`.
        .map_or(1, |attempts| attempts.min(f64::from(u32::MAX)) as u32);
    let body = Reflect::get(message, &JsValue::from_str("body"))
        .map(|body| text(&body))
        .unwrap_or_default();
    Pushed { body, attempts }
}

/// A message's body as text: a text body as it was sent, bytes read as
/// UTF-8, and any other value as its JSON; one with none, such as
/// `undefined`, is empty, which the runtime parks as unreadable.
fn text(body: &JsValue) -> String {
    if let Some(text) = body.as_string() {
        return text;
    }
    if body.is_instance_of::<js_sys::ArrayBuffer>() {
        return String::from_utf8_lossy(&Uint8Array::new(body).to_vec()).into_owned();
    }
    if js_sys::ArrayBuffer::is_view(body) {
        let view = body.unchecked_ref::<js_sys::DataView>();
        if let (Ok(offset), Ok(length)) = (
            u32::try_from(view.byte_offset()),
            u32::try_from(view.byte_length()),
        ) {
            let bytes = Uint8Array::new_with_byte_offset_and_length(&view.buffer(), offset, length);
            return String::from_utf8_lossy(&bytes.to_vec()).into_owned();
        }
    }
    js_sys::JSON::stringify(body)
        .ok()
        .and_then(|json| json.as_string())
        .unwrap_or_default()
}
