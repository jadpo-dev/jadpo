//! Rate limits counted in Cloudflare's rate-limiting binding (area 16,
//! "Rate limits"; chunk 8.2m2): the store `rate_limits:` names, when it is
//! a connection with `driver: Driver.cloudflare_rate_limits`. Each limit
//! the app counts is a binding of the Worker's own, which `jadpo build`
//! writes into the Wrangler file, and `limit({ key })` on it says whether
//! one more request for `key` is within it.
//!
//! A binding that can't be asked leaves the count to the isolate's memory
//! and no request fails for it; since chunk 8.4b one that hasn't answered
//! within [`LIMIT_TIMEOUT_MS`] is not waited for, and each binding's
//! fallback is logged once an isolate as `rate_limits.fallback`, naming
//! the binding and why, never a key.

use std::cell::RefCell;
use std::collections::BTreeSet;

use jadpo_runtime::deployable::Log;
use js_sys::{Function, Object, Promise, Reflect};
use serde_json::Value;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

use crate::timer;

/// How long a request waits for the binding's answer before counting in
/// the isolate's memory instead (chunk 8.4b): the binding answers from a
/// count kept where the Worker runs, so one slower than this is in
/// trouble, and a request isn't held up by it.
pub const LIMIT_TIMEOUT_MS: u64 = 500;

/// Why a binding couldn't be asked.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Unasked {
    /// The Worker has no binding of that name.
    Missing,
    /// It has no `limit()`, asking it threw, or what it gave wasn't an
    /// answer.
    Failed,
    /// It didn't answer within [`LIMIT_TIMEOUT_MS`].
    TimedOut,
}

impl Unasked {
    fn word(self) -> &'static str {
        match self {
            Unasked::Missing => "missing",
            Unasked::Failed => "failed",
            Unasked::TimedOut => "timed_out",
        }
    }

    fn message(self, binding: &str) -> String {
        let why = match self {
            Unasked::Missing => format!("`{binding}` isn't one of the Worker's bindings"),
            Unasked::Failed => format!("`{binding}` failed when asked"),
            Unasked::TimedOut => format!(
                "`{binding}` didn't answer within {LIMIT_TIMEOUT_MS} ms"
            ),
        };
        format!(
            "{why}: each instance counts its limit in its own memory while it can't be asked."
        )
    }
}

thread_local! {
    /// Each binding and why it fell back that the isolate has logged.
    static LOGGED: RefCell<BTreeSet<(String, Unasked)>> = const { RefCell::new(BTreeSet::new()) };
}

/// Logs, once an isolate for each binding and reason, that `binding`
/// couldn't be asked and the count fell back to the isolate's memory.
pub(crate) fn fell_back(log: Option<&Log>, binding: &str, why: Unasked) {
    let Some(log) = log else {
        return;
    };
    let first = LOGGED.with(|logged| logged.borrow_mut().insert((binding.to_string(), why)));
    if first {
        log.event(
            "rate_limits.fallback",
            [
                ("binding", Value::from(binding)),
                ("reason", Value::from(why.word())),
                ("message", Value::from(why.message(binding))),
            ],
        );
    }
}

/// Whether binding `binding` in `env` lets one more request for `key`
/// through, or why it couldn't be asked: the Worker has no such binding,
/// asking it threw or failed, or it didn't answer within
/// [`LIMIT_TIMEOUT_MS`], so the caller counts in the isolate's memory
/// instead and no request fails for it.
pub(crate) async fn within(env: &JsValue, binding: &str, key: &str) -> Result<bool, Unasked> {
    let binding = Reflect::get(env, &JsValue::from_str(binding))
        .ok()
        .filter(JsValue::is_object)
        .ok_or(Unasked::Missing)?;
    let limit = Reflect::get(&binding, &JsValue::from_str("limit"))
        .ok()
        .and_then(|limit| limit.dyn_into::<Function>().ok())
        .ok_or(Unasked::Failed)?;
    let options = Object::new();
    Reflect::set(&options, &JsValue::from_str("key"), &JsValue::from_str(key))
        .map_err(|_| Unasked::Failed)?;
    let outcome = limit
        .call1(&binding, &options)
        .map_err(|_| Unasked::Failed)?;
    let outcome = timer::within(LIMIT_TIMEOUT_MS, JsFuture::from(Promise::resolve(&outcome)))
        .await
        .ok_or(Unasked::TimedOut)?
        .map_err(|_| Unasked::Failed)?;
    Reflect::get(&outcome, &JsValue::from_str("success"))
        .ok()
        .and_then(|success| success.as_bool())
        .ok_or(Unasked::Failed)
}
