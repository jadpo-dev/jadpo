//! A compiled deployable on Workers (roadmap chunk 8.2b;
//! docs/compiler-runtime.md §2.6, "A Workers deployable"): what the module
//! `jadpo build` writes for a `Target.wasm` deployable runs. Its crate
//! exports the Worker's handlers, each one call into this module with the
//! [`Deployable`] it was built as (the program, the build stamp and each
//! store's migrations, as the native host is given them), and Jadpo's
//! module shim hands each the invocation's event, `env` and `ctx`.
//!
//! [`fetch`] makes the program once per isolate, which decodes its plan,
//! and answers every request `503` for now: serving a request, with the
//! settings read from the bindings, the stores' migrations compared and
//! the events logged with the stamp, is chunk 8.2d's; the cron's
//! `scheduled` handler is 8.2f's and the queue's 8.2k's.

use std::cell::OnceCell;

use jadpo_runtime::compiled::Program;
use wasm_bindgen::JsValue;

pub use jadpo_runtime::deployable::{Deployable, Migration, Stamp, Store};

thread_local! {
    /// The isolate's program, made by its first invocation: `None` when
    /// its plan couldn't be read.
    static PROGRAM: OnceCell<Option<Program>> = const { OnceCell::new() };
}

/// What [`fetch`] answers while a Worker serves no request.
pub const NOT_SERVED: &str = "This Worker doesn't answer requests yet.";

/// What [`fetch`] answers when the program's plan can't be read, which a
/// module `jadpo build` wrote never does.
pub const UNREADABLE: &str = "This Worker's program couldn't be read.";

/// The Worker's `fetch` for `deployable`: the `Response` to `request`.
pub fn fetch(deployable: &Deployable, request: JsValue, env: JsValue, ctx: JsValue) -> JsValue {
    let _ = (request, env, ctx);
    let made = PROGRAM.with(|program| {
        program
            .get_or_init(|| (deployable.program)().ok())
            .is_some()
    });
    answer(
        503,
        "unavailable",
        if made { NOT_SERVED } else { UNREADABLE },
    )
}

/// A JSON response in the runtime's failure envelope, without a request
/// id: the `Response`, or the error constructing it threw.
fn answer(status: u16, code: &str, message: &str) -> JsValue {
    let body = format!(
        "{{\"error\":{{\"code\":{},\"message\":{}}}}}",
        quoted(code),
        quoted(message)
    );
    let init = web_sys::ResponseInit::new();
    init.set_status(status);
    let headers = match web_sys::Headers::new() {
        Ok(headers) => headers,
        Err(error) => return error,
    };
    if let Err(error) = headers.set("content-type", "application/json") {
        return error;
    }
    init.set_headers(&headers);
    match web_sys::Response::new_with_opt_str_and_init(Some(&body), &init) {
        Ok(response) => response.into(),
        Err(error) => error,
    }
}

/// `text` as a JSON string.
fn quoted(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if u32::from(c) < 0x20 => out.push_str(&format!("\\u{:04x}", u32::from(c))),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_is_quoted_as_json() {
        assert_eq!(quoted("plain"), "\"plain\"");
        assert_eq!(quoted("a \"b\" \\ c\n"), "\"a \\\"b\\\" \\\\ c\\u000a\"");
    }
}
