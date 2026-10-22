//! Jadpo's own bindings to what Workers gives a Worker beyond the Web
//! platform's APIs (`web-sys` binds those): `connect()` from
//! `cloudflare:sockets`, an invocation's `ctx.waitUntil`, the service
//! binding a Worker starts a fresh invocation of itself through, and the
//! globals a Worker has rather than a window's (`fetch`, `setTimeout`,
//! `crypto`).
//! Everything that can throw is bound with `catch`, so a JavaScript
//! exception comes back as an error and never unwinds through the module.
//!
//! Natively these compile but are never called: the crate builds there
//! so the workspace builds, lints and tests as one.

use js_sys::{Function, Promise, Uint8Array};
use wasm_bindgen::prelude::*;

#[wasm_bindgen(module = "cloudflare:sockets")]
extern "C" {
    /// `connect({ hostname, port }, options)`: a TCP socket, opening.
    #[wasm_bindgen(catch, js_name = connect)]
    pub(crate) fn connect(address: &JsValue, options: &JsValue) -> Result<Socket, JsValue>;
}

#[wasm_bindgen]
extern "C" {
    /// A socket `connect()` gave.
    #[derive(Debug, Clone)]
    pub(crate) type Socket;

    #[wasm_bindgen(method, getter)]
    pub(crate) fn readable(this: &Socket) -> web_sys::ReadableStream;

    #[wasm_bindgen(method, getter)]
    pub(crate) fn writable(this: &Socket) -> web_sys::WritableStream;

    /// Settles once the connection is made, or couldn't be.
    #[wasm_bindgen(method, getter)]
    pub(crate) fn opened(this: &Socket) -> Promise;

    /// The socket upgraded to TLS: a socket opened with `secureTransport:
    /// "starttls"`, whose streams nothing holds.
    #[wasm_bindgen(method, catch, js_name = startTls)]
    pub(crate) fn start_tls(this: &Socket, options: &JsValue) -> Result<Socket, JsValue>;

    #[wasm_bindgen(method, catch)]
    pub(crate) fn close(this: &Socket) -> Result<Promise, JsValue>;

    /// An invocation's context, the `ctx` a handler is given beside its
    /// event and `env`.
    #[derive(Debug, Clone)]
    pub type ExecutionContext;

    /// Keeps the invocation alive until `promise` settles, after its
    /// response has been sent.
    #[wasm_bindgen(method, catch, js_name = waitUntil)]
    pub(crate) fn wait_until(this: &ExecutionContext, promise: &Promise) -> Result<(), JsValue>;

    /// The Worker's own `JadpoHandOn` entrypoint as `ctx.exports` gives
    /// it, a loopback binding through which it starts a fresh invocation
    /// of itself (chunks 8.2h and 8.4b).
    #[derive(Debug, Clone)]
    pub(crate) type HandOnBinding;

    /// Starts a fresh invocation of the Worker for the work `work` names,
    /// a call through Workers' RPC: what it gives settles once that
    /// invocation has taken the work on.
    #[wasm_bindgen(method, catch, js_name = handOn)]
    pub(crate) fn hand_on(this: &HandOnBinding, work: &str) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(catch, js_name = setTimeout)]
    pub(crate) fn set_timeout(handler: &Function, millis: f64) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(catch, js_name = clearTimeout)]
    pub(crate) fn clear_timeout(id: &JsValue) -> Result<(), JsValue>;

    /// Fills `array`, at most 65,536 bytes, from the platform's
    /// cryptographically secure generator.
    #[wasm_bindgen(catch, js_namespace = crypto, js_name = getRandomValues)]
    pub(crate) fn get_random_values(array: &Uint8Array) -> Result<JsValue, JsValue>;

    /// The global `fetch`, a Worker's rather than a window's.
    #[wasm_bindgen(catch, js_name = fetch)]
    pub(crate) fn fetch(input: &str, init: &web_sys::RequestInit) -> Result<Promise, JsValue>;

    /// `console.log`: a line of the Worker's log, which Workers Logs and
    /// `wrangler tail` read, and workerd writes on its standard output.
    #[wasm_bindgen(catch, js_namespace = console, js_name = log)]
    fn console_log(line: &str) -> Result<(), JsValue>;
}

/// Writes `line` to the Worker's log: a deployable's sink (chunk 8.2d).
pub(crate) fn log(line: &str) {
    let _ = console_log(line);
}

/// What a JavaScript error says: its message, or the value itself.
pub(crate) fn said(error: &JsValue) -> String {
    if let Some(text) = error.as_string() {
        return text;
    }
    js_sys::Reflect::get(error, &JsValue::from_str("message"))
        .ok()
        .and_then(|message| message.as_string())
        .unwrap_or_else(|| format!("{error:?}"))
}

/// The name of a JavaScript error (`TypeError`, `AbortError`), if it has
/// one.
pub(crate) fn name(error: &JsValue) -> Option<String> {
    js_sys::Reflect::get(error, &JsValue::from_str("name"))
        .ok()
        .and_then(|name| name.as_string())
}
