//! A compiled deployable on Workers (roadmap chunks 8.2b and 8.2d;
//! docs/compiler-runtime.md §2.6, "A Workers deployable"; area 14, "A
//! Workers deployable"): what the module `jadpo build` writes for a
//! `Target.wasm` deployable runs. Its crate exports the Worker's handlers,
//! each one call into this module with the [`Deployable`] it was built as
//! (the program, the build stamp and each store's migrations, as the
//! native host is given them), and Jadpo's module shim hands each the
//! invocation's event, `env` and `ctx`.
//!
//! [`fetch`] starts an isolate the way the native host starts a process,
//! at its first request, since a Worker has no start of its own: it makes
//! the program, which decodes its plan, reads the settings from the
//! bindings through `config::load`, and refuses a `client_address:`
//! source other than `cloudflare`; a problem there logs
//! `runtime.startup_failed` once and the isolate answers every request 503.
//! Then each request builds its drivers through the factory over
//! `connect()` and `fetch()`, on a host of its own, since a socket belongs
//! to the request that opened it; until each store has been compared with
//! the build's migrations and found current, which then holds for the
//! isolate, the request compares them first and, finding one behind,
//! ahead or out of reach, logs it and answers 503, so a migration applied
//! after the deploy is picked up by the next request. Served, it goes
//! through the runtime's `Serving`, the client's address read from
//! `CF-Connecting-IP` (`Target::Workers`), what it leaves run in
//! `waitUntil`, sharing the isolate's `Worker`, and each fault behind a
//! 500 is logged with its `.jadpo` frames. Every event is one JSON line
//! on the console, which Workers Logs reads, carrying the stamp.
//!
//! A streamed answer is sent as a `ReadableStream`, each event as it is
//! read ([`crate::streamed`]); and [`hand_on`] is the fresh invocation
//! work handed on starts, a request to the Worker itself through a service
//! binding to its `JadpoHandOn` entrypoint (chunk 8.2h). The cron's
//! `scheduled` handler is chunk 8.2f's and the queue's 8.2k's.

use std::cell::{Cell, OnceCell};
use std::fmt;
use std::rc::Rc;

use jadpo_drivers::factory::{Drivers, Why};
use jadpo_runtime::compiled::Program;
use jadpo_runtime::config::{self, HostVariables, Mode};
use jadpo_runtime::deployable::{
    Compared, Faults, Log, Then, client_address_refused, compare, problem, setting_problem,
    unaddressed,
};
use jadpo_runtime::http::address::{ClientAddress, Target};
use jadpo_runtime::http::{self, BODY_LIMIT, Request, Response, Worker};
use jadpo_runtime::runner;
use jadpo_runtime::serve::Serving;
use jadpo_runtime::{Executor, Host};
use js_sys::Uint8Array;
use serde_json::{Value, json};
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{Headers, ReadableStreamDefaultReader, ResponseInit};

use crate::{WorkersHost, js, supplies};

pub use jadpo_runtime::deployable::{Deployable, Migration, Stamp, Store};

/// What an isolate made of its deployable at its first request: the
/// program reading the settings the bindings gave, what the HTTP component
/// keeps between its requests, and its log.
struct Isolate {
    program: Rc<Program>,
    worker: Rc<Worker>,
    log: Log,
    /// Whether each store has been compared with the build's migrations
    /// and found current: until then each request compares them again.
    current: Cell<bool>,
}

thread_local! {
    /// The isolate's start: `None` when it failed, which it logged.
    static ISOLATE: OnceCell<Option<Rc<Isolate>>> = const { OnceCell::new() };
}

/// The Worker's `fetch` for `deployable`: a promise of the `Response` to
/// `request`.
pub fn fetch(deployable: &Deployable, request: JsValue, env: JsValue, ctx: JsValue) -> JsValue {
    let deployable = *deployable;
    wasm_bindgen_futures::future_to_promise(async move {
        let response = respond(&deployable, request, env, ctx).await;
        write(response).await
    })
    .into()
}

/// The `handOn` of the Worker's `JadpoHandOn` entrypoint for
/// `deployable` (chunk 8.2h; area 12, "Running background work"): a fresh
/// invocation taking on the work an invocation whose budget was spent
/// handed on, which `work` names by its records
/// ([`runner::HandOn::message`]). It starts the isolate as `fetch` does
/// and, once the stores are current, carries the work on in its own
/// `waitUntil`, with Workers' 30 seconds ([`runner::carry_on`]); it
/// answers at once, so the invocation that handed it on can end. Work it
/// can't take on waits in the store for the minute's cron.
pub fn hand_on(deployable: &Deployable, work: String, env: JsValue, ctx: JsValue) -> JsValue {
    let host = WorkersHost::new(env, ctx);
    let Some(isolate) = isolate(deployable, &host) else {
        return JsValue::UNDEFINED;
    };
    let deployable = *deployable;
    let host = Rc::new(host.with_drivers(drivers(&isolate.program)));
    let fresh = Rc::clone(&host);
    host.defer(Box::pin(async move {
        if !ready(&deployable, &fresh, &isolate).await {
            return;
        }
        let program = Rc::clone(&isolate.program) as Rc<dyn Executor>;
        if let Some(carried) = runner::carry_on(program, fresh as Rc<dyn Host>, &work) {
            carried.await;
        }
    }));
    JsValue::UNDEFINED
}

/// Whether the isolate's stores are current, comparing them first through
/// `host`'s databases until they have been; `runtime.started` is logged
/// the first time they are.
async fn ready(deployable: &Deployable, host: &WorkersHost, isolate: &Isolate) -> bool {
    if isolate.current.get() {
        return true;
    }
    if !stores_current(deployable, host, &isolate.log).await {
        return false;
    }
    if !isolate.current.replace(true) {
        isolate
            .log
            .event("runtime.started", [("unbuilt", unbuilt(host))]);
    }
    true
}

/// The runtime's answer to `request`, on a host of its own.
async fn respond(
    deployable: &Deployable,
    request: JsValue,
    env: JsValue,
    ctx: JsValue,
) -> Response {
    let host = WorkersHost::new(env, ctx);
    let Some(isolate) = isolate(deployable, &host) else {
        return http::unavailable(&host);
    };
    let host = host.with_drivers(drivers(&isolate.program));
    if !isolate.current.get() {
        if !stores_current(deployable, &host, &isolate.log).await {
            return http::unavailable(&host);
        }
        if !isolate.current.replace(true) {
            isolate
                .log
                .event("runtime.started", [("unbuilt", unbuilt(&host))]);
        }
    }
    let Some(request) = read(request).await else {
        return http::unreadable(&host);
    };
    let program = Rc::clone(&isolate.program);
    let serving = Serving::new(Rc::new(host), move || -> Rc<dyn Executor> {
        Rc::clone(&program) as Rc<dyn Executor>
    })
    .sharing(Rc::clone(&isolate.worker))
    .observed_by(Rc::new(Faults(isolate.log)));
    serving.handle(&request).await
}

/// The isolate's start, made at its first request.
fn isolate(deployable: &Deployable, host: &WorkersHost) -> Option<Rc<Isolate>> {
    ISOLATE.with(|isolate| isolate.get_or_init(|| start(deployable, host)).clone())
}

/// Makes the program and reads its settings from `host`'s bindings, or
/// logs every problem that stops it.
fn start(deployable: &Deployable, host: &WorkersHost) -> Option<Rc<Isolate>> {
    let log = Log::new(deployable.stamp, js::log);
    match started(deployable, host) {
        Ok(program) => Some(Rc::new(Isolate {
            program: Rc::new(program),
            worker: Rc::default(),
            log,
            current: Cell::new(false),
        })),
        Err(problems) => {
            log.event(
                "runtime.startup_failed",
                [("problems", Value::Array(problems))],
            );
            None
        }
    }
}

/// The program, reading the settings the bindings give on Workers, or
/// what stops it.
fn started(deployable: &Deployable, host: &WorkersHost) -> Result<Program, Vec<Value>> {
    let program = (deployable.program)().map_err(|error| {
        vec![problem(
            "plan_unreadable",
            format!("The program's plan can't be read: {error}."),
        )]
    })?;
    let values = config::load(
        &program.runtime().config,
        program.validators(),
        &HostVariables(host),
        Mode::Environment,
        &|_: &mut [u8]| false,
    )
    .map_err(|problems| problems.iter().map(setting_problem).collect::<Vec<_>>())?;
    let refused = {
        let plan = program.plan();
        ClientAddress::read(
            plan.routes
                .and_then(|routes| routes.client_address.as_ref()),
            &values,
            Target::Workers,
        )
        .err()
    };
    if let Some(refused) = refused {
        return Err(vec![client_address_refused(&refused)]);
    }
    Ok(program.with_config_on(&values, Target::Workers))
}

/// The request's drivers, built by the factory from the program's
/// connections over `connect()` and `fetch()`.
fn drivers(program: &Program) -> Drivers {
    match program.started() {
        Some(started) => Drivers::build(&program.runtime().connections, &started, &supplies()),
        None => Drivers::default(),
    }
}

/// Whether each store's database has applied every migration the build
/// holds and nothing else, logging what it found when one hasn't
/// (`runtime.startup_failed`) or can't be reached (`runtime.waiting`).
async fn stores_current(deployable: &Deployable, host: &WorkersHost, log: &Log) -> bool {
    let mut problems = Vec::new();
    let mut unreachable = Vec::new();
    for store in deployable.stores {
        let Some(database) = host.database(store.name) else {
            let why = host
                .unbuilt()
                .iter()
                .find(|unbuilt| unbuilt.connection == store.name)
                .filter(|unbuilt| !matches!(unbuilt.why, Why::Unset(None)));
            problems.push(unaddressed(
                store.name,
                why.map(|why| why as &dyn fmt::Display),
            ));
            continue;
        };
        match compare(database, store, Then::NextRequest).await {
            Compared::Current => {}
            Compared::Refused(problem) => problems.push(problem),
            Compared::Unreachable => unreachable.push(store.name),
        }
    }
    for store in &unreachable {
        log.event(
            "runtime.waiting",
            [
                ("store", Value::from(*store)),
                (
                    "message",
                    Value::from(format!(
                        "`{store}` can't be reached yet: the Worker serves once it can check the database's migrations, trying again at each request."
                    )),
                ),
            ],
        );
    }
    if !problems.is_empty() {
        log.event(
            "runtime.startup_failed",
            [("problems", Value::Array(problems))],
        );
        return false;
    }
    unreachable.is_empty()
}

/// Each connection whose driver wasn't built, for `runtime.started`: the
/// runtime answers it `misconfigured` when it is used.
fn unbuilt(host: &WorkersHost) -> Value {
    Value::Array(
        host.unbuilt()
            .iter()
            .map(|unbuilt| {
                json!({
                    "connection": unbuilt.connection,
                    "driver": unbuilt.driver,
                    "message": unbuilt.to_string(),
                })
            })
            .collect(),
    )
}

/// The runtime's request from Workers' `Request`: its body read whole or,
/// once it passes the runtime's limit (area 7, question 5), marked too
/// large and never held, so the runtime answers 413. On Workers there is
/// no connection's address: the client's comes from `CF-Connecting-IP`.
/// `None` when it can't be read.
async fn read(request: JsValue) -> Option<Request> {
    let request = request.dyn_into::<web_sys::Request>().ok()?;
    let (path, query) = target(&request.url());
    let headers = crate::fetch::listed(&request.headers());
    let declared = headers
        .iter()
        .find(|(name, _)| name == "content-length")
        .and_then(|(_, length)| length.parse::<usize>().ok());
    let mut too_large = declared.is_some_and(|length| length > BODY_LIMIT);
    let mut body = Vec::new();
    if !too_large && let Some(stream) = request.body() {
        let reader = ReadableStreamDefaultReader::new(&stream).ok()?;
        while let Some(piece) = crate::fetch::piece(&reader).await.ok()? {
            if body.len() + piece.len() > BODY_LIMIT {
                too_large = true;
                let _ = reader.cancel();
                break;
            }
            body.extend_from_slice(&piece);
        }
    }
    if too_large {
        body.clear();
    }
    Some(Request {
        method: request.method(),
        path,
        query,
        headers,
        body,
        address: None,
        too_large,
    })
}

/// A request's URL as its path, percent-encoded as sent, and its query
/// string without its `?`.
fn target(url: &str) -> (String, Option<String>) {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let rest = rest.split_once('#').map_or(rest, |(before, _)| before);
    let from_path = rest
        .find(['/', '?'])
        .and_then(|at| rest.get(at..))
        .unwrap_or("");
    let (path, query) = match from_path.split_once('?') {
        Some((path, query)) => (path, Some(query.to_string())),
        None => (from_path, None),
    };
    let path = if path.is_empty() { "/" } else { path };
    (path.to_string(), query)
}

/// The runtime's response as Workers' `Response`, or the error making it
/// threw. A streamed answer is its body as a `ReadableStream`, each event
/// sent as it is read (chunk 8.2h, [`crate::streamed`]).
async fn write(mut response: Response) -> Result<JsValue, JsValue> {
    let init = ResponseInit::new();
    init.set_status(response.status);
    let headers = Headers::new()?;
    for (name, value) in &response.headers {
        // A header Workers won't send is left out, as natively.
        let _ = headers.append(name, value);
    }
    init.set_headers(&headers);
    if let Some(stream) = response.stream.take() {
        let body = crate::streamed::body(stream)?;
        return web_sys::Response::new_with_opt_readable_stream_and_init(Some(&body), &init)
            .map(JsValue::from);
    }
    // A status that has no body is given none.
    let bodiless = response.body.is_empty() || matches!(response.status, 101 | 204 | 205 | 304);
    let body = (!bodiless).then(|| Uint8Array::from(response.body.as_slice()));
    web_sys::Response::new_with_opt_js_u8_array_and_init(body.as_ref(), &init).map(JsValue::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_url_is_its_path_and_query() {
        assert_eq!(
            target("https://kiosk.example/repairs"),
            ("/repairs".to_string(), None)
        );
        assert_eq!(
            target("http://127.0.0.1:8787/repairs/a%20b/thanks?x=1&y=%2F"),
            (
                "/repairs/a%20b/thanks".to_string(),
                Some("x=1&y=%2F".to_string())
            )
        );
        assert_eq!(
            target("https://kiosk.example/?"),
            ("/".to_string(), Some(String::new()))
        );
        assert_eq!(
            target("https://kiosk.example?page=2"),
            ("/".to_string(), Some("page=2".to_string()))
        );
        assert_eq!(
            target("https://kiosk.example/a#b"),
            ("/a".to_string(), None)
        );
        assert_eq!(target("https://kiosk.example"), ("/".to_string(), None));
    }
}
