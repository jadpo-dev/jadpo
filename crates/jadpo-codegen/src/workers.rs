//! What a Workers deployable is deployed from (roadmap chunk 8.2b;
//! docs/compiler-runtime.md §2.6, "A Workers deployable"; area 14, "A
//! Workers deployable"): the module cargo built for
//! `wasm32-unknown-unknown`, bound by wasm-bindgen's own library at the
//! lock's release ([`bind`]), so nothing is downloaded and no command-line
//! tool has to match the crate; Jadpo's module shim ([`shim`]), which
//! Workers runs first; and the Wrangler file ([`wrangler`]).
//!
//! The four files sit side by side in `build/target/<deployable>/`:
//!
//! - `worker.mjs`, the shim: the module instantiated once per isolate and
//!   the Worker's handlers, `fetch` and `scheduled`, handed to the ones the
//!   module exports, with the `JadpoHandOn` entrypoint a fresh invocation
//!   takes work handed on through;
//! - `<deployable>.js`, the JavaScript wasm-bindgen writes for the module
//!   (its `web` form, which takes the module it is given);
//! - `<deployable>.wasm`, the module;
//! - `wrangler.json`: the shim as the main module, uploaded as it is with
//!   the others beside it (no bundling, each kind by an explicit rule), the
//!   compatibility date Jadpo's Workers host is run under, each setting
//!   the deployable needs set, by its variable, the minute's cron trigger
//!   when its tick has something to do, and, when it runs background work,
//!   the service binding to its own `JadpoHandOn` through which it hands
//!   work on.

use std::path::Path;

/// The compatibility date a Worker runs under: the Workers runtime's
/// behaviour as of this date, which the Workers host is tested at under
/// the workerd pinned in `tools/workers/` (1.20261007.1).
pub const COMPATIBILITY_DATE: &str = "2026-10-01";

/// The shim's file: the Wrangler file's main module.
pub const SHIM: &str = "worker.mjs";

/// The Wrangler file's name.
pub const WRANGLER: &str = "wrangler.json";

/// A Worker's module and the JavaScript that binds it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bound {
    /// `<deployable>.js`.
    pub js: String,
    /// `<deployable>.wasm`.
    pub wasm: Vec<u8>,
}

/// The module cargo built at `module` bound for the Worker `name`: the
/// JavaScript wasm-bindgen writes for it and the module it leaves, or why
/// it couldn't.
pub fn bind(module: &Path, name: &str) -> Result<Bound, String> {
    let mut bindgen = wasm_bindgen_cli_support::Bindgen::new();
    bindgen
        .input_path(module)
        .web(true)
        .map_err(|error| error.to_string())?
        .typescript(false)
        .out_name(name);
    let mut output = bindgen
        .generate_output()
        .map_err(|error| error.to_string())?;
    // Everything the module imports is the platform's (`cloudflare:sockets`
    // and the globals): a snippet of JavaScript of its own would be a file
    // the Wrangler file doesn't upload.
    if !output.snippets().values().all(Vec::is_empty) || !output.local_modules().is_empty() {
        return Err("the module carries JavaScript of its own, which a Worker isn't given".into());
    }
    let js = output.js().to_string();
    let wasm = output.wasm_mut().emit_wasm();
    Ok(Bound { js, wasm })
}

/// The binding through which a Worker hands work on to a fresh invocation
/// of itself: a service binding to its own [`HAND_ON_ENTRYPOINT`] (chunk
/// 8.2h), as `jadpo_host_workers::HAND_ON_BINDING` reads it.
pub const HAND_ON_BINDING: &str = "JADPO_HAND_ON";

/// The shim's named entrypoint for work handed on, as
/// `jadpo_host_workers::HAND_ON_ENTRYPOINT` names it: not the default
/// export, so no request from outside reaches it.
pub const HAND_ON_ENTRYPOINT: &str = "JadpoHandOn";

/// The module Workers runs first: `name`'s module instantiated once per
/// isolate, the Worker's `fetch` and `scheduled` the module's, and its
/// [`HAND_ON_ENTRYPOINT`], whose `handOn` takes on work another invocation
/// handed on.
pub fn shim(name: &str) -> String {
    format!(
        "// The deployable `{name}` for Cloudflare Workers, written by `jadpo build`; don't edit it.\n\
         // Its module is instantiated once per isolate, and each handler hands\n\
         // the module's its event, the Worker's `env` and the invocation's `ctx`.\n\
         import {{ WorkerEntrypoint }} from \"cloudflare:workers\";\n\
         import module from \"./{name}.wasm\";\n\
         import {{ initSync, fetch as handleFetch, scheduled as handleScheduled, hand_on as handleHandOn }} from \"./{name}.js\";\n\
         \n\
         initSync({{ module }});\n\
         \n\
         export default {{\n\
         \x20 fetch(request, env, ctx) {{\n\
         \x20   return handleFetch(request, env, ctx);\n\
         \x20 }},\n\
         \x20 scheduled(controller, env, ctx) {{\n\
         \x20   return handleScheduled(controller, env, ctx);\n\
         \x20 }},\n\
         }};\n\
         \n\
         // Work another invocation of this Worker handed on, which this one\n\
         // carries on in its own time; reached only through the Worker's\n\
         // binding to itself.\n\
         export class {HAND_ON_ENTRYPOINT} extends WorkerEntrypoint {{\n\
         \x20 handOn(work) {{\n\
         \x20   return handleHandOn(work, this.env, this.ctx);\n\
         \x20 }}\n\
         }}\n"
    )
}

/// The minute's cron, as Cloudflare's cron triggers write it: every minute
/// (area 12, "The tick").
pub const EVERY_MINUTE: &str = "* * * * *";

/// The Worker's name on Cloudflare for deployable `name`: letters, digits
/// and dashes only, so each `_` becomes `-` (`back_office`,
/// `back-office`).
pub fn worker_name(name: &str) -> String {
    name.replace('_', "-")
}

/// The Wrangler file of deployable `name`, whose settings without a
/// default are read from `required`, their variables: JSON. Each is a
/// required secret, Wrangler's one way to name a binding that must be set
/// without giving its value; a setting with a default is left to it. When
/// it `ticks`, its tick having a store to keep or sweep, the minute's cron
/// trigger runs its `scheduled` handler (chunk 8.2f). When it `hands_on`,
/// running background work, the Worker is bound to its own
/// [`HAND_ON_ENTRYPOINT`] as [`HAND_ON_BINDING`] (chunk 8.2h).
pub fn wrangler(name: &str, required: &[String], ticks: bool, hands_on: bool) -> String {
    let quoted = |text: &str| serde_json::Value::from(text).to_string();
    let mut out = format!(
        "{{\n\
         \x20 \"name\": {name},\n\
         \x20 \"main\": {main},\n\
         \x20 \"compatibility_date\": {date},\n\
         \x20 \"no_bundle\": true,\n\
         \x20 \"find_additional_modules\": true,\n\
         \x20 \"rules\": [\n\
         \x20   {{ \"type\": \"ESModule\", \"globs\": [\"**/*.mjs\", \"**/*.js\"] }},\n\
         \x20   {{ \"type\": \"CompiledWasm\", \"globs\": [\"**/*.wasm\"] }}\n\
         \x20 ]",
        name = quoted(&worker_name(name)),
        main = quoted(SHIM),
        date = quoted(COMPATIBILITY_DATE),
    );
    if !required.is_empty() {
        let secrets: Vec<String> = required.iter().map(|variable| quoted(variable)).collect();
        out.push_str(&format!(
            ",\n  \"secrets\": {{\n    \"required\": [{}]\n  }}",
            secrets.join(", ")
        ));
    }
    if ticks {
        out.push_str(&format!(
            ",\n  \"triggers\": {{\n    \"crons\": [{}]\n  }}",
            quoted(EVERY_MINUTE)
        ));
    }
    if hands_on {
        out.push_str(&format!(
            ",\n  \"services\": [\n    {{ \"binding\": {}, \"service\": {}, \"entrypoint\": {} }}\n  ]",
            quoted(HAND_ON_BINDING),
            quoted(&worker_name(name)),
            quoted(HAND_ON_ENTRYPOINT),
        ));
    }
    out.push_str("\n}\n");
    out
}
