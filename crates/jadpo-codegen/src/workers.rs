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
//!   the Worker's handlers, `fetch`, `scheduled` and `queue`, handed to the
//!   ones the module exports, with the `JadpoHandOn` entrypoint a fresh
//!   invocation takes work handed on through;
//! - `<deployable>.js`, the JavaScript wasm-bindgen writes for the module
//!   (its `web` form, which takes the module it is given);
//! - `<deployable>.wasm`, the module;
//! - `wrangler.json`: the shim as the main module, uploaded as it is with
//!   the others beside it (no bundling, each kind by an explicit rule), the
//!   compatibility date Jadpo's Workers host is run under, each setting
//!   the deployable needs set, by its variable, each database's address
//!   as a Hyperdrive binding (chunk 8.2i), the minute's cron trigger when
//!   its tick has something to do, the rate-limiting bindings its rate
//!   limits are counted in ([`rate_limited`]), and a consumer for each
//!   queue on Cloudflare Queues it receives and for each one's dead-letter
//!   queue ([`queued`], chunks 8.2k and 8.4b). No binding names the Worker
//!   itself: work is handed on to a fresh invocation through the loopback
//!   `ctx.exports` gives its `JadpoHandOn` (chunk 8.4b), which follows
//!   whatever name `wrangler deploy --name` or `--env` gives it. Wrangler's own
//!   `deploy --dry-run`, pinned in `tools/workers/`, accepts the file
//!   without a warning.

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

/// The shim's named entrypoint for work handed on, as
/// `jadpo_host_workers::HAND_ON_ENTRYPOINT` names it: not the default
/// export, so no request from outside reaches it, and reached by the
/// Worker itself through `ctx.exports` (chunk 8.4b).
pub const HAND_ON_ENTRYPOINT: &str = "JadpoHandOn";

/// The module Workers runs first: `name`'s module instantiated once per
/// isolate, the Worker's `fetch`, `scheduled` and `queue` the module's
/// (each batch Cloudflare Queues pushes to its consumer, chunk 8.2k), and
/// its
/// [`HAND_ON_ENTRYPOINT`], whose `handOn` takes on work another invocation
/// handed on.
pub fn shim(name: &str) -> String {
    format!(
        "// The deployable `{name}` for Cloudflare Workers, written by `jadpo build`; don't edit it.\n\
         // Its module is instantiated once per isolate, and each handler hands\n\
         // the module's its event, the Worker's `env` and the invocation's `ctx`.\n\
         import {{ WorkerEntrypoint }} from \"cloudflare:workers\";\n\
         import module from \"./{name}.wasm\";\n\
         import {{ initSync, fetch as handleFetch, scheduled as handleScheduled, queue as handleQueue, hand_on as handleHandOn }} from \"./{name}.js\";\n\
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
         \x20 queue(batch, env, ctx) {{\n\
         \x20   return handleQueue(batch, env, ctx);\n\
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

/// A database's address on Workers (area 5, "PostgreSQL through
/// Hyperdrive"; chunk 8.2i): its setting's variable, bound to a Hyperdrive
/// configuration, and that configuration's id when the build was given
/// one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hyperdrive {
    pub binding: String,
    pub id: Option<String>,
}

/// The Wrangler file of deployable `name`, whose settings without a
/// default are read from `required`, their variables, and whose
/// databases' addresses from `hyperdrive`: JSON. Each required setting is
/// a required secret, Wrangler's one way to name a binding that must be
/// set without giving its value; a setting with a default is left to it.
/// Each database's address is a Hyperdrive binding, with its id when it
/// has one; without one, Wrangler refuses to deploy until it is given
/// (chunk 8.2i). When it `ticks`, its tick having a store to keep or
/// sweep, the minute's cron trigger runs its `scheduled` handler (chunk
/// 8.2f).
pub fn wrangler(name: &str, required: &[String], hyperdrive: &[Hyperdrive], ticks: bool) -> String {
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
         \x20   {{ \"type\": \"CompiledWasm\", \"globs\": [\"**/*.wasm\"], \"fallthrough\": false }}\n\
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
    if !hyperdrive.is_empty() {
        let bindings: Vec<String> = hyperdrive
            .iter()
            .map(|binding| match &binding.id {
                Some(id) => format!(
                    "    {{ \"binding\": {}, \"id\": {} }}",
                    quoted(&binding.binding),
                    quoted(id)
                ),
                None => format!("    {{ \"binding\": {} }}", quoted(&binding.binding)),
            })
            .collect();
        out.push_str(&format!(
            ",\n  \"hyperdrive\": [\n{}\n  ]",
            bindings.join(",\n")
        ));
    }
    if ticks {
        out.push_str(&format!(
            ",\n  \"triggers\": {{\n    \"crons\": [{}]\n  }}",
            quoted(EVERY_MINUTE)
        ));
    }
    out.push_str("\n}\n");
    out
}

/// `wrangler`, a Wrangler file [`wrangler`] wrote, with Cloudflare's
/// rate-limiting bindings `bindings` (area 16, "Rate limits"; chunk
/// 8.2m2): each its name, its namespace id, as text as Wrangler takes it,
/// and the limit it counts, `simple: { limit, period }`, the period in
/// seconds. With none, `wrangler` as it is.
pub fn rate_limited(wrangler: String, bindings: &[jadpo_std::http::RateLimitBinding]) -> String {
    let Some(body) = wrangler.strip_suffix("\n}\n") else {
        return wrangler;
    };
    if bindings.is_empty() {
        return wrangler;
    }
    let entries: Vec<String> = bindings
        .iter()
        .map(|binding| {
            format!(
                "    {{ \"name\": {}, \"namespace_id\": {}, \"simple\": {{ \"limit\": {}, \"period\": {} }} }}",
                serde_json::Value::from(binding.name.as_str()),
                serde_json::Value::from(binding.namespace.to_string()),
                binding.limit.requests,
                binding.limit.window_ms / 1000
            )
        })
        .collect();
    format!(
        "{body},\n  \"ratelimits\": [\n{}\n  ]\n}}\n",
        entries.join(",\n")
    )
}

/// `wrangler`, a Wrangler file [`wrangler`] wrote, with a consumer for each
/// of `queues` on Cloudflare Queues (chunk 8.2k; area 11, "Chosen in chunk
/// 7.7c2"): `queues: { consumers: [ ] }`, as
/// [`jadpo_std::receiving::wrangler_queues`] writes them, by Wrangler's own
/// setting names. With none, `wrangler` as it is.
pub fn queued(wrangler: String, queues: &[jadpo_std::receiving::QueuePlan]) -> String {
    let Some(consumers) = jadpo_std::receiving::wrangler_queues(queues) else {
        return wrangler;
    };
    let Some(body) = wrangler.strip_suffix("\n}\n") else {
        return wrangler;
    };
    let Some(consumers) = consumers["consumers"].as_array() else {
        return wrangler;
    };
    // Each consumer on a line, its queue first.
    let consumers: Vec<String> = consumers
        .iter()
        .filter_map(serde_json::Value::as_object)
        .map(|consumer| {
            let mut settings: Vec<(&String, &serde_json::Value)> = consumer.iter().collect();
            settings.sort_by_key(|(key, _)| *key != "queue");
            let settings: Vec<String> = settings
                .iter()
                .map(|(key, value)| format!("{}: {value}", serde_json::Value::from(key.as_str())))
                .collect();
            format!("      {{ {} }}", settings.join(", "))
        })
        .collect();
    format!(
        "{body},\n  \"queues\": {{\n    \"consumers\": [\n{}\n    ]\n  }}\n}}\n",
        consumers.join(",\n")
    )
}
