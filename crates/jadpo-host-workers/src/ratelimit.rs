//! Rate limits counted in Cloudflare's rate-limiting binding (area 16,
//! "Rate limits"; chunk 8.2m2): the store `rate_limits:` names, when it is
//! a connection with `driver: Driver.cloudflare_rate_limits`. Each limit
//! the app counts is a binding of the Worker's own, which `jadpo build`
//! writes into the Wrangler file, and `limit({ key })` on it says whether
//! one more request for `key` is within it.

use js_sys::{Function, Object, Promise, Reflect};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

/// Whether binding `binding` in `env` lets one more request for `key`
/// through: `None` when it can't be asked, because the Worker has no such
/// binding, or asking it threw or failed, so the caller counts in the
/// isolate's memory instead and no request fails for it.
pub(crate) async fn within(env: &JsValue, binding: &str, key: &str) -> Option<bool> {
    let binding = Reflect::get(env, &JsValue::from_str(binding))
        .ok()
        .filter(JsValue::is_object)?;
    let limit = Reflect::get(&binding, &JsValue::from_str("limit"))
        .ok()?
        .dyn_into::<Function>()
        .ok()?;
    let options = Object::new();
    Reflect::set(&options, &JsValue::from_str("key"), &JsValue::from_str(key)).ok()?;
    let outcome = limit.call1(&binding, &options).ok()?;
    let outcome = JsFuture::from(Promise::resolve(&outcome)).await.ok()?;
    Reflect::get(&outcome, &JsValue::from_str("success"))
        .ok()?
        .as_bool()
}
