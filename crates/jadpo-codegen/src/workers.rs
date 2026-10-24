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
//! - `<deployable>.wasm`, the module, without its names (chunk 8.4f);
//! - `wrangler.json`, written by [`wrangler`] as one value: the Worker
//!   named after its app and its deployable ([`worker_name`], chunk 8.4f),
//!   the shim as the main module, uploaded as it is with the others beside
//!   it (no bundling, each kind by an explicit rule), the compatibility
//!   date Jadpo's Workers host is run under, Workers Logs on, each setting
//!   the deployable needs set, by its variable, each database's address
//!   as a Hyperdrive binding (chunk 8.2i), the minute's cron trigger when
//!   its tick has something to do, the rate-limiting bindings its rate
//!   limits are counted in, and a consumer for each queue on Cloudflare
//!   Queues it receives and for each one's dead-letter queue (chunks 8.2k
//!   and 8.4b). No binding names the Worker itself: work is handed on to
//!   a fresh invocation through the loopback `ctx.exports` gives its
//!   `JadpoHandOn` (chunk 8.4b), which follows whatever name `wrangler
//!   deploy --name` or `--env` gives it. Wrangler's own `deploy
//!   --dry-run`, pinned in `tools/workers/`, accepts the file without a
//!   warning.

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

/// Whether a bound module keeps the names of its functions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Names {
    /// Its name section, demangled, kept, so a trap's stack names the
    /// Rust it was in: a test Worker's.
    Kept,
    /// Its name section and the tools' `producers` section left out, as a
    /// deployable's module ships, under a plan's limit on what it uploads
    /// (chunk 8.4f).
    Removed,
}

/// The module cargo built at `module` bound for the Worker `name`, with
/// its function names as `names` says: the JavaScript wasm-bindgen writes
/// for it and the module it leaves, or why it couldn't.
pub fn bind(module: &Path, name: &str, names: Names) -> Result<Bound, String> {
    let mut bindgen = wasm_bindgen_cli_support::Bindgen::new();
    bindgen
        .input_path(module)
        .web(true)
        .map_err(|error| error.to_string())?
        .typescript(false)
        .remove_name_section(names == Names::Removed)
        .remove_producers_section(names == Names::Removed)
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

/// The longest name Cloudflare gives a Worker.
pub const WORKER_NAME_LIMIT: usize = 63;

/// The Worker's name on Cloudflare for deployable `deployable` of the app
/// named `app` (chunk 8.4f): both, so two apps' `kiosk`s are two Workers
/// in one account, in the letters, digits and dashes Cloudflare takes,
/// each run of anything else one dash (`Repair Café` and `back_office`,
/// `repair-caf-back-office`). At most [`WORKER_NAME_LIMIT`] of them, the
/// app's name shortened to keep the deployable's whole; an app with no
/// name left, as a test's project, gives the deployable's alone.
pub fn worker_name(app: &str, deployable: &str) -> String {
    let deployable: String = dashed(deployable).chars().take(WORKER_NAME_LIMIT).collect();
    let deployable = deployable.trim_end_matches('-');
    let room = WORKER_NAME_LIMIT.saturating_sub(deployable.len() + 1);
    let app: String = dashed(app).chars().take(room).collect();
    match app.trim_end_matches('-') {
        "" => deployable.to_string(),
        app => format!("{app}-{deployable}"),
    }
}

/// `text` lower-cased, each run of anything but an ASCII letter or digit
/// one dash, none at either end.
fn dashed(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars().flat_map(char::to_lowercase) {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            out.push(c);
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_string()
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

/// What a Worker's Wrangler file says.
#[derive(Clone, Copy, Debug)]
pub struct Wrangler<'a> {
    /// The app's name, its folder's until `app.jadpo` gives one; empty
    /// where nothing named it.
    pub app: &'a str,
    /// The deployable's name.
    pub deployable: &'a str,
    /// The variable of each setting without a default.
    pub required: &'a [String],
    /// Each database's address, bound to Hyperdrive.
    pub hyperdrive: &'a [Hyperdrive],
    /// Whether its tick has a store to keep or sweep.
    pub ticks: bool,
    /// External identity discovery must use the public Internet on Workers.
    pub public_identity: bool,
    /// The rate-limiting bindings its rate limits are counted in.
    pub rate_limits: &'a [jadpo_std::http::RateLimitBinding],
    /// The queues it receives, those on Cloudflare Queues consumed.
    pub queues: &'a [jadpo_std::receiving::QueuePlan],
}

/// The Wrangler file `file` says, as JSON, built as one value and written
/// out once, so no binding can be left out for another (chunk 8.4f):
///
/// - `name`, [`worker_name`]'s; `main`, the shim; the compatibility date;
///   `no_bundle` with a rule for each kind of file;
/// - `observability: { enabled: true }`, since Workers Logs keeps what the
///   Worker logs only when it is enabled, and Wrangler sends nothing,
///   leaving it off, when the file doesn't say (chunk 8.4f);
/// - `secrets: { required }`, each setting without a default, Wrangler's
///   one way to name a binding that must be set without giving its value;
///   a setting with a default is left to it;
/// - `hyperdrive`, each database's address, with its id when the build was
///   given one; without one, Wrangler refuses to deploy until it is
///   (chunk 8.2i);
/// - `triggers: { crons }`, the minute's cron trigger, when it ticks
///   (chunk 8.2f);
/// - `ratelimits`, Cloudflare's rate-limiting bindings (area 16, "Rate
///   limits"; chunk 8.2m2): each its name, its namespace id, as text as
///   Wrangler takes it, and the limit it counts, `simple: { limit, period
///   }`, the period in seconds;
/// - `queues: { consumers }`, a consumer for each queue on Cloudflare
///   Queues and for its dead-letter queue, as
///   [`jadpo_std::receiving::wrangler_queues`] writes them (chunks 8.2k and
///   8.4b; area 11, "Chosen in chunk 7.7c2").
///
/// Each part it has nothing for is left out.
pub fn wrangler(file: &Wrangler<'_>) -> String {
    let text = |text: &str| Json::Value(serde_json::Value::from(text));
    let texts = |items: &[String]| Json::List(items.iter().map(|item| text(item)).collect());
    let mut entries = vec![
        ("name", text(&worker_name(file.app, file.deployable))),
        ("main", text(SHIM)),
        ("compatibility_date", text(COMPATIBILITY_DATE)),
        ("no_bundle", Json::Value(true.into())),
        ("find_additional_modules", Json::Value(true.into())),
        (
            "rules",
            Json::List(vec![
                object(vec![
                    ("type", text("ESModule")),
                    ("globs", texts(&["**/*.mjs".into(), "**/*.js".into()])),
                ]),
                object(vec![
                    ("type", text("CompiledWasm")),
                    ("globs", texts(&["**/*.wasm".into()])),
                    ("fallthrough", Json::Value(false.into())),
                ]),
            ]),
        ),
        (
            "observability",
            object(vec![("enabled", Json::Value(true.into()))]),
        ),
    ];
    if file.public_identity {
        entries.push((
            "compatibility_flags",
            texts(&["global_fetch_strictly_public".into()]),
        ));
    }
    if !file.required.is_empty() {
        entries.push(("secrets", object(vec![("required", texts(file.required))])));
    }
    if !file.hyperdrive.is_empty() {
        let bindings = file.hyperdrive.iter().map(|binding| {
            let mut entry = vec![("binding", text(&binding.binding))];
            if let Some(id) = &binding.id {
                entry.push(("id", text(id)));
            }
            object(entry)
        });
        entries.push(("hyperdrive", Json::List(bindings.collect())));
    }
    if file.ticks {
        entries.push((
            "triggers",
            object(vec![("crons", texts(&[EVERY_MINUTE.into()]))]),
        ));
    }
    if !file.rate_limits.is_empty() {
        let bindings = file.rate_limits.iter().map(|binding| {
            object(vec![
                ("name", text(&binding.name)),
                ("namespace_id", text(&binding.namespace.to_string())),
                (
                    "simple",
                    object(vec![
                        ("limit", Json::Value(binding.limit.requests.into())),
                        (
                            "period",
                            Json::Value((binding.limit.window_ms / 1000).into()),
                        ),
                    ]),
                ),
            ])
        });
        entries.push(("ratelimits", Json::List(bindings.collect())));
    }
    if let Some(queues) = jadpo_std::receiving::wrangler_queues(file.queues) {
        entries.push(("queues", Json::from(queues)));
    }
    let mut out = String::new();
    object(entries).write(0, 0, &mut out);
    out.push('\n');
    out
}

/// A JSON value whose objects keep the order they are written in, so the
/// Wrangler file reads as Wrangler's own documentation lays one out.
enum Json {
    Value(serde_json::Value),
    List(Vec<Json>),
    Object(Vec<(String, Json)>),
}

/// An object of `entries`, in their order.
fn object(entries: Vec<(&str, Json)>) -> Json {
    Json::Object(
        entries
            .into_iter()
            .map(|(key, value)| (key.to_string(), value))
            .collect(),
    )
}

/// The widest line [`Json::write`] keeps an object or a list on.
const LINE: usize = 80;

impl From<serde_json::Value> for Json {
    /// `value`, each object's `queue` first and the rest in key order.
    fn from(value: serde_json::Value) -> Self {
        match value {
            serde_json::Value::Array(items) => {
                Json::List(items.into_iter().map(Json::from).collect())
            }
            serde_json::Value::Object(map) => {
                let mut entries: Vec<(String, serde_json::Value)> = map.into_iter().collect();
                entries.sort_by_key(|(key, _)| key != "queue");
                Json::Object(
                    entries
                        .into_iter()
                        .map(|(key, value)| (key, Json::from(value)))
                        .collect(),
                )
            }
            value => Json::Value(value),
        }
    }
}

impl Json {
    /// The value on one line: `{ "a": 1 }`, `["x", "y"]`.
    fn inline(&self) -> String {
        match self {
            Json::Value(value) => value.to_string(),
            Json::List(items) => {
                let items: Vec<String> = items.iter().map(Json::inline).collect();
                format!("[{}]", items.join(", "))
            }
            Json::Object(entries) if entries.is_empty() => "{}".to_string(),
            Json::Object(entries) => {
                let entries: Vec<String> = entries
                    .iter()
                    .map(|(key, value)| {
                        format!(
                            "{}: {}",
                            serde_json::Value::from(key.as_str()),
                            value.inline()
                        )
                    })
                    .collect();
                format!("{{ {} }}", entries.join(", "))
            }
        }
    }

    /// Writes the value into `out`, which is at column `column` of a line
    /// indented by `indent`: on that line when it fits in [`LINE`] and
    /// isn't the file itself (column 0), else each entry on a line of its
    /// own.
    fn write(&self, indent: usize, column: usize, out: &mut String) {
        let inline = self.inline();
        let (open, close, children): (char, char, Vec<(Option<&str>, &Json)>) = match self {
            Json::Value(_) => (' ', ' ', Vec::new()),
            Json::List(items) => ('[', ']', items.iter().map(|item| (None, item)).collect()),
            Json::Object(entries) => (
                '{',
                '}',
                entries
                    .iter()
                    .map(|(key, value)| (Some(key.as_str()), value))
                    .collect(),
            ),
        };
        if children.is_empty() || column > 0 && column + inline.len() <= LINE {
            out.push_str(&inline);
            return;
        }
        let inner = indent + 2;
        out.push(open);
        out.push('\n');
        for (at, (key, value)) in children.iter().enumerate() {
            out.push_str(&" ".repeat(inner));
            let mut column = inner;
            if let Some(key) = key {
                let key = format!("{}: ", serde_json::Value::from(*key));
                column += key.len();
                out.push_str(&key);
            }
            value.write(inner, column, out);
            if at + 1 < children.len() {
                out.push(',');
            }
            out.push('\n');
        }
        out.push_str(&" ".repeat(indent));
        out.push(close);
    }
}
