//! The probe Worker the Workers host's tests run under workerd
//! (tests/workerd.rs; chunk 8.2a): each path uses one part of the host and
//! answers what it saw as one line of text, `error:` and why when a part
//! refused. It never panics, which would abort the isolate. The passkey
//! paths (chunk 8.2l) run one step of a ceremony each, the state and the
//! device's answer handed back and forth by the test as base64url.

use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::rc::Rc;
use std::task::Poll;

use jadpo_drivers::package::{Fetch, FetchError};
use jadpo_drivers::{Endpoint, Stream, Transport, Trust, within};
use jadpo_host_workers::{WorkersFetch, WorkersHost, WorkersOutbound, WorkersTransport};
use jadpo_runtime::budget::Invocation;
use jadpo_runtime::provider::{Reading, Request};
use jadpo_runtime::stream::Chunk;
use jadpo_runtime::{Allowed, Host, PasskeyError, PasskeyParts};
use jadpo_std::HttpMethod;
use jadpo_std::http::{RateLimit, RateLimitBinding, RateLimitStore};
use jadpo_value::encoding::{from_base64url, to_base64url};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use wasm_bindgen::prelude::*;

thread_local! {
    /// What deferred work has done, by key, across the isolate's requests.
    static DONE: RefCell<BTreeSet<String>> = const { RefCell::new(BTreeSet::new()) };
}

/// The Worker's `fetch`.
#[wasm_bindgen]
pub async fn fetch(
    request: web_sys::Request,
    env: JsValue,
    ctx: JsValue,
) -> Result<web_sys::Response, JsValue> {
    let url = web_sys::Url::new(&request.url())?;
    let query = url.search_params();
    let param = |name: &str| query.get(name).unwrap_or_default();
    let number = |name: &str| param(name).parse::<u64>().unwrap_or(0);
    let host = outbound_reaching(WorkersHost::new(env.clone(), ctx), &env);
    let answer = match url.pathname().as_str() {
        "/random" => random(&host),
        "/clock" => clock(&host).await,
        "/variable" => match host.variable(&param("name")) {
            Some(value) => format!("set:{value}"),
            None => "unset".to_string(),
        },
        "/budget" => format!(
            "after_response={} tick={} handed_on={}",
            host.budget_ms(Invocation::AfterResponse),
            host.budget_ms(Invocation::Tick),
            host.budget_ms(Invocation::HandedOn)
        ),
        "/echo" => echo(&param("host"), number("port"), &param("tls")).await,
        "/pooler" => {
            let endpoint = Endpoint::Tcp {
                host: param("host"),
                port: u16::try_from(number("port")).unwrap_or(0),
            };
            if WorkersTransport::default().is_pooler(&endpoint) {
                "pooler".to_string()
            } else {
                "not a pooler".to_string()
            }
        }
        #[cfg(feature = "postgres")]
        "/postgres" => postgres(number("port")).await,
        "/unix" => match WorkersTransport::default()
            .connect(&Endpoint::Unix {
                dir: "/tmp".to_string(),
                port: 5432,
            })
            .await
        {
            Ok(_) => "connected".to_string(),
            Err(error) => failed(&error),
        },
        "/authority" => authority(number("port")).await,
        "/fetch" => {
            let answer = fetched(&param("url"), number("timeout"), None).await;
            // Kept going after the call, so only the call's own abort, not
            // the invocation's end, lets its connection go.
            host.sleep(number("linger")).await;
            answer
        }
        "/post" => fetched(&param("url"), number("timeout"), Some("{\"sent\":1}")).await,
        "/open" => {
            let answer = opened(&param("url"), number("timeout")).await;
            host.sleep(number("linger")).await;
            answer
        }
        "/outbound" => match host.outbound() {
            Some(outbound) => match outbound
                .get(
                    &param("url"),
                    usize::try_from(number("limit")).unwrap_or(0),
                    5_000,
                )
                .await
            {
                Ok((status, body)) => {
                    format!("status:{status} body:{}", String::from_utf8_lossy(&body))
                }
                Err(error) => format!("error:{error}"),
            },
            None => "error:no outbound".to_string(),
        },
        "/defer" => {
            let key = param("key");
            let transport = WorkersTransport::default();
            host.defer(Box::pin(async move {
                transport.sleep(150).await;
                DONE.with(|done| done.borrow_mut().insert(key));
            }));
            "deferred".to_string()
        }
        "/deferred" => {
            let key = param("key");
            if DONE.with(|done| done.borrow().contains(&key)) {
                "done".to_string()
            } else {
                "not yet".to_string()
            }
        }
        "/rate" => {
            let key = param("key");
            let limit = RateLimit {
                requests: 2,
                window_ms: 60_000,
            };
            let mut seen = Vec::new();
            for _ in 0..2 {
                let now = host.now_ms().unwrap_or(0);
                seen.push(match host.rate_limit(&key, limit, now).await {
                    Allowed::Yes => "yes",
                    _ => "no",
                });
            }
            seen.join(" ")
        }
        "/rate-binding" => {
            // Two a minute, counted in the binding the test names: what
            // `rate_limits:` gives a host when its store is Cloudflare's
            // rate-limiting binding (chunk 8.2m2).
            let limit = RateLimit {
                requests: 2,
                window_ms: 60_000,
            };
            let store = RateLimitStore {
                connection: "limits".to_string(),
                bindings: vec![RateLimitBinding {
                    limit,
                    name: param("binding"),
                    namespace: 1001,
                }],
            };
            let host = WorkersHost::new(env.clone(), JsValue::UNDEFINED)
                .with_rate_limits(Some(Rc::new(store)));
            let key = param("key");
            let mut seen = Vec::new();
            for _ in 0..2 {
                let now = host.now_ms().unwrap_or(0);
                seen.push(match host.rate_limit(&key, limit, now).await {
                    Allowed::Yes => "yes".to_string(),
                    Allowed::No { retry_after } => format!("no:{retry_after}"),
                });
            }
            seen.join(" ")
        }
        "/spawn" => {
            let ran = Rc::new(Cell::new(false));
            let seen = Rc::clone(&ran);
            let transport = WorkersTransport::default();
            transport.spawn(Box::pin(async move {
                seen.set(true);
            }));
            transport.sleep(10).await;
            if ran.get() {
                "ran".to_string()
            } else {
                "didn't run".to_string()
            }
        }
        "/supplies" => {
            let supplies = host.supplies();
            format!(
                "target:{} transport:{} fetch:{} disk:{}",
                match supplies.target {
                    jadpo_std::connections::Target::Workers => "workers",
                    jadpo_std::connections::Target::Native => "native",
                },
                yes(supplies.transport.is_some()),
                yes(supplies.fetch.is_some()),
                yes(supplies.disk.is_some())
            )
        }
        "/passkey/register/options" => match host.passkeys() {
            Some(ceremonies) => started(ceremonies.start_registration(
                &param("origin"),
                300_000,
                [7; 16],
                "ada@example.com",
                &[],
            )),
            None => "error:no passkeys".to_string(),
        },
        "/passkey/register" => match (
            host.passkeys(),
            unpacked(&param("state")),
            unpacked(&param("credential")),
        ) {
            (Some(ceremonies), Some(state), Some(credential)) => {
                match ceremonies.finish_registration(&param("origin"), 300_000, &state, &credential)
                {
                    Ok(parts) => format!("kept:{}", kept(&parts)),
                    Err(error) => format!("error:{error:?}"),
                }
            }
            _ => "error:unreadable".to_string(),
        },
        "/passkey/sign-in/options" => match host.passkeys() {
            Some(ceremonies) => started(ceremonies.start_sign_in(&param("origin"), 300_000)),
            None => "error:no passkeys".to_string(),
        },
        "/passkey/sign-in" => match (
            host.passkeys(),
            unpacked(&param("state")),
            unpacked(&param("credential")),
            unpacked(&param("stored")).and_then(|stored| stored_parts(&stored)),
        ) {
            (Some(ceremonies), Some(state), Some(credential), Some(stored)) => {
                match ceremonies.finish_sign_in(
                    &param("origin"),
                    300_000,
                    &state,
                    &credential,
                    &stored,
                    [7; 16],
                ) {
                    Ok(parts) => format!("kept:{}", kept(&parts)),
                    Err(error) => format!("error:{error:?}"),
                }
            }
            _ => "error:unreadable".to_string(),
        },
        // Chunk 8.2j: the invocation's six places, and the isolate's slots.
        "/six" => six(&host, &param("url"), &param("timeouts"), &param("counted")).await,
        "/six-mixed" => six_mixed(&host, &param("url"), number("echo")).await,
        "/six-deferred" => six_deferred(&host, &param("url"), &param("key")).await,
        "/slot" => slot(&host, &param("url"), &param("connection")).await,
        "/slot-left" => {
            // An invocation the platform ends mid-call: its slot never
            // given back, nor said to be held again.
            std::mem::forget(jadpo_runtime::slots::wait(&host, &param("connection"), 1).await);
            "left".to_string()
        }
        path => format!("no probe at {path}"),
    };
    web_sys::Response::new_with_opt_str(Some(&answer))
}

/// A ceremony's options and state, each as base64url, for the test to
/// hand on: `options:… state:…`.
fn started(result: Result<(String, String), PasskeyError>) -> String {
    match result {
        Ok((options, state)) => format!(
            "options:{} state:{}",
            to_base64url(options.as_bytes()),
            to_base64url(state.as_bytes())
        ),
        Err(error) => format!("error:{error:?}"),
    }
}

/// Text the test sent as base64url.
fn unpacked(text: &str) -> Option<String> {
    String::from_utf8(from_base64url(text)?).ok()
}

/// What is kept of a passkey, as JSON in base64url.
fn kept(parts: &PasskeyParts) -> String {
    let json = serde_json::json!({
        "credential_id": to_base64url(&parts.credential_id),
        "public_key": to_base64url(&parts.public_key),
        "sign_count": parts.sign_count,
        "transports": parts.transports,
        "backup_eligible": parts.backup_eligible,
        "backed_up": parts.backed_up,
    });
    to_base64url(json.to_string().as_bytes())
}

/// What [`kept`] wrote, read back.
fn stored_parts(json: &str) -> Option<PasskeyParts> {
    let json: serde_json::Value = serde_json::from_str(json).ok()?;
    let bytes = |name: &str| json.get(name)?.as_str().and_then(from_base64url);
    Some(PasskeyParts {
        credential_id: bytes("credential_id")?,
        public_key: bytes("public_key")?,
        sign_count: u32::try_from(json.get("sign_count")?.as_u64()?).ok()?,
        transports: json
            .get("transports")?
            .as_array()?
            .iter()
            .filter_map(|name| name.as_str().map(str::to_string))
            .collect(),
        backup_eligible: json.get("backup_eligible")?.as_bool()?,
        backed_up: json.get("backed_up")?.as_bool()?,
    })
}

fn yes(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

/// With `test-outbound`, the guard lets the test's server named by the
/// `REACH` binding stand in for a public host.
fn outbound_reaching(host: WorkersHost, env: &JsValue) -> WorkersHost {
    #[cfg(feature = "test-outbound")]
    if let Some(origin) = js_sys::Reflect::get(env, &JsValue::from_str("REACH"))
        .ok()
        .and_then(|origin| origin.as_string())
    {
        return host.with_outbound(WorkersOutbound::reaching(&origin));
    }
    let _ = env;
    host.with_outbound(WorkersOutbound::default())
}

fn random(host: &WorkersHost) -> String {
    let (mut first, mut second) = ([0u8; 32], [0u8; 32]);
    let mut whole = vec![0u8; 70_000];
    let filled = host.random(&mut first) && host.random(&mut second);
    let distinct = filled && first != second && first != [0u8; 32];
    let all = host.random(&mut whole) && whole.iter().skip(65_536).any(|byte| *byte != 0);
    format!(
        "{} {} {}",
        if filled { "filled" } else { "unfilled" },
        if distinct { "distinct" } else { "repeated" },
        if all { "whole" } else { "partly" }
    )
}

async fn clock(host: &WorkersHost) -> String {
    let (Some(now), Some(before)) = (host.now_ms(), host.monotonic_ms()) else {
        return "error:no clock".to_string();
    };
    host.sleep(120).await;
    let (Some(after), Some(later)) = (host.now_ms(), host.monotonic_ms()) else {
        return "error:no clock".to_string();
    };
    format!(
        "now={now} slept={} monotonic={}",
        after - now,
        later.saturating_sub(before)
    )
}

/// `error:` and the kind of an I/O error.
fn failed(error: &io::Error) -> String {
    format!("error:{:?}", error.kind())
}

/// Sends `ping` over a stream the transport opened to `host:port`,
/// upgraded to TLS for `tls` when it is given, and reads the line back.
async fn echo(host: &str, port: u64, tls: &str) -> String {
    echo_through(&WorkersTransport::default(), host, port, tls).await
}

/// [`echo`] over `transport`.
async fn echo_through(transport: &WorkersTransport, host: &str, port: u64, tls: &str) -> String {
    let endpoint = Endpoint::Tcp {
        host: host.to_string(),
        port: u16::try_from(port).unwrap_or(0),
    };
    let talk = async {
        let mut stream: Box<dyn Stream> = transport.connect(&endpoint).await?;
        if !tls.is_empty() {
            stream = transport.upgrade(stream, tls, Trust::Roots).await?;
        }
        stream.write_all(b"ping\n").await?;
        stream.flush().await?;
        let mut answer = Vec::new();
        let mut byte = [0u8; 1];
        while stream.read(&mut byte).await? == 1 && byte != *b"\n" {
            answer.extend_from_slice(&byte);
        }
        Ok::<_, io::Error>(String::from_utf8_lossy(&answer).into_owned())
    };
    match within(transport, 5_000, talk).await {
        Some(Ok(answer)) => format!("answer:{answer}"),
        Some(Err(error)) => failed(&error),
        None => "error:timed out".to_string(),
    }
}

/// The Workers transport, with every connection to `db.example.test` sent
/// to this machine, where the test's server is: the name is still what the
/// certificate is checked for, so the server counts as elsewhere.
#[cfg(feature = "postgres")]
struct Elsewhere;

#[cfg(feature = "postgres")]
impl Transport for Elsewhere {
    fn connect(
        &self,
        endpoint: &Endpoint,
    ) -> jadpo_runtime::LocalFuture<'_, io::Result<Box<dyn Stream>>> {
        let endpoint = match endpoint {
            Endpoint::Tcp { host, port } if host == "db.example.test" => Endpoint::Tcp {
                host: "localhost".to_string(),
                port: *port,
            },
            other => other.clone(),
        };
        Box::pin(async move { WorkersTransport::default().connect(&endpoint).await })
    }

    fn upgrade<'a>(
        &'a self,
        stream: Box<dyn Stream>,
        name: &'a str,
        trust: Trust<'a>,
    ) -> jadpo_runtime::LocalFuture<'a, io::Result<Box<dyn Stream>>> {
        Box::pin(async move {
            WorkersTransport::default()
                .upgrade(stream, name, trust)
                .await
        })
    }

    fn spawn(&self, task: jadpo_runtime::LocalFuture<'static, ()>) {
        WorkersTransport::default().spawn(task);
    }

    fn sleep(&self, millis: u64) -> jadpo_runtime::LocalFuture<'_, ()> {
        Box::pin(async move { WorkersTransport::default().sleep(millis).await })
    }
}

/// A statement run on PostgreSQL at `db.example.test` on `port`, over TLS
/// checked against workerd's roots: `ran`, or how the store refused.
#[cfg(feature = "postgres")]
async fn postgres(port: u64) -> String {
    use jadpo_runtime::StoreError;
    let url = format!("postgresql://jadpo@db.example.test:{port}/app");
    let store = match jadpo_drivers::postgres::Postgres::new(&url, Rc::new(Elsewhere)) {
        Ok(store) => store,
        Err(error) => return format!("error:{error:?}"),
    };
    let ran = within(
        &WorkersTransport::default(),
        10_000,
        store.batch_outside_transaction("SELECT 1"),
    )
    .await;
    match ran {
        Some(Ok(())) => "ran".to_string(),
        Some(Err(StoreError::Fault(why))) => format!("fault:{why}"),
        Some(Err(other)) => format!("error:{other:?}"),
        None => "error:timed out".to_string(),
    }
}

/// An upgrade trusting an authority of the connection's own.
async fn authority(port: u64) -> String {
    let transport = WorkersTransport::default();
    let endpoint = Endpoint::Tcp {
        host: "localhost".to_string(),
        port: u16::try_from(port).unwrap_or(0),
    };
    let authorities = [vec![0u8; 4]];
    let upgraded = async {
        let stream = transport.connect(&endpoint).await?;
        transport
            .upgrade(stream, "localhost", Trust::Authorities(&authorities))
            .await
    };
    match upgraded.await {
        Ok(_) => "upgraded".to_string(),
        Err(error) => failed(&error),
    }
}

fn request(url: &str, timeout_ms: u64, body: Option<&str>) -> Request {
    Request {
        method: if body.is_some() {
            HttpMethod::Post
        } else {
            HttpMethod::Get
        },
        url: url.to_string(),
        headers: body
            .map(|_| vec![("content-type".to_string(), "application/json".to_string())])
            .unwrap_or_default(),
        body: body.map(str::to_string),
        timeout_ms,
        reading: Reading::default(),
        repeatable: body.is_none(),
    }
}

/// `error:` and what an exchange that gave no answer was.
fn no_answer(error: &FetchError) -> String {
    match error {
        FetchError::NotSent(_) => "error:NotSent".to_string(),
        FetchError::NoAnswer(_) => "error:NoAnswer".to_string(),
        FetchError::TimedOut => "error:TimedOut".to_string(),
        FetchError::Refused(_) => "error:Refused".to_string(),
    }
}

async fn fetched(url: &str, timeout_ms: u64, body: Option<&str>) -> String {
    fetched_through(&WorkersFetch::default(), url, timeout_ms, body).await
}

/// [`fetched`] through `fetch`.
async fn fetched_through(
    fetch: &WorkersFetch,
    url: &str,
    timeout_ms: u64,
    body: Option<&str>,
) -> String {
    let made = request(url, timeout_ms, body);
    match fetch.fetch(&made).await {
        Ok(response) => {
            let test = response
                .headers
                .iter()
                .find(|(name, _)| name == "x-test")
                .map_or("none", |(_, value)| value.as_str());
            format!(
                "status:{} x-test:{test} body:{}",
                response.status,
                String::from_utf8_lossy(&response.body)
            )
        }
        Err(error) => no_answer(&error),
    }
}

async fn opened(url: &str, timeout_ms: u64) -> String {
    let made = request(url, timeout_ms, None);
    let mut opened = match WorkersFetch::default().open(&made).await {
        Ok(opened) => opened,
        Err(error) => return no_answer(&error),
    };
    let mut pieces = Vec::new();
    loop {
        match opened.body.next().await {
            Chunk::Bytes(bytes) => pieces.push(String::from_utf8_lossy(&bytes).into_owned()),
            Chunk::End => {
                pieces.push("end".to_string());
                break;
            }
            Chunk::TimedOut => {
                pieces.push("timed_out".to_string());
                break;
            }
            Chunk::Broken => {
                pieces.push("broken".to_string());
                break;
            }
        }
    }
    format!("status:{} pieces:{}", opened.status, pieces.join("|"))
}

/// Runs `futures` together until each has finished: their outputs in order.
async fn join_all<F: Future>(futures: impl IntoIterator<Item = F>) -> Vec<F::Output> {
    let mut pending: Vec<Pin<Box<F>>> = futures.into_iter().map(Box::pin).collect();
    let mut done: Vec<Option<F::Output>> = pending.iter().map(|_| None).collect();
    std::future::poll_fn(|cx| {
        let mut all = true;
        for (future, out) in pending.iter_mut().zip(done.iter_mut()) {
            if out.is_none() {
                match future.as_mut().poll(cx) {
                    Poll::Ready(value) => *out = Some(value),
                    Poll::Pending => all = false,
                }
            }
        }
        if all { Poll::Ready(()) } else { Poll::Pending }
    })
    .await;
    done.into_iter().flatten().collect()
}

/// What a fetch answered, in a word: `status:200`, or `error:` and why.
fn short(answer: &str) -> &str {
    answer.split_whitespace().next().unwrap_or_default()
}

/// A request to `url` for each of `timeouts`, all sent at once through
/// the invocation's places, or, `counted=no`, each through a fetch with
/// places of its own, as though nothing counted them: what each answered,
/// in order (chunk 8.2j).
async fn six(host: &WorkersHost, url: &str, timeouts: &str, counted: &str) -> String {
    let shared = WorkersFetch::counted(Rc::clone(host.opening()));
    let timeouts: Vec<u64> = timeouts
        .split(',')
        .filter_map(|timeout| timeout.parse().ok())
        .collect();
    let answers = join_all(timeouts.iter().map(|&timeout| {
        let shared = &shared;
        async move {
            let answer = if counted == "no" {
                fetched_through(&WorkersFetch::default(), url, timeout, None).await
            } else {
                fetched_through(shared, url, timeout, None).await
            };
            short(&answer).to_string()
        }
    }))
    .await;
    answers.join(" ")
}

/// Six requests to `url` through the invocation's places and, once they
/// are out, a line to the echo on `port` over a connection the
/// invocation's transport opens: what they answered, and how long after
/// the start the echo's answer came.
async fn six_mixed(host: &WorkersHost, url: &str, port: u64) -> String {
    let began = host.monotonic_ms().unwrap_or(0);
    let fetch = WorkersFetch::counted(Rc::clone(host.opening()));
    let transport = WorkersTransport::counted(Rc::clone(host.opening()));
    let fetches = join_all((0..6).map(|_| {
        let fetch = &fetch;
        async move { short(&fetched_through(fetch, url, 5_000, None).await).to_string() }
    }));
    let echoed = async {
        host.sleep(50).await;
        let answer = echo_through(&transport, "127.0.0.1", port, "").await;
        let after = host.monotonic_ms().unwrap_or(0).saturating_sub(began);
        (answer, after)
    };
    let mut fetches = std::pin::pin!(fetches);
    let mut echoed = std::pin::pin!(echoed);
    let (mut answers, mut echo) = (None, None);
    std::future::poll_fn(|cx| {
        if answers.is_none()
            && let Poll::Ready(done) = fetches.as_mut().poll(cx)
        {
            answers = Some(done);
        }
        if echo.is_none()
            && let Poll::Ready(done) = echoed.as_mut().poll(cx)
        {
            echo = Some(done);
        }
        if answers.is_some() && echo.is_some() {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    })
    .await;
    let (answer, after) = echo.unwrap_or_default();
    format!(
        "fetches:{} echo:{answer} after:{after}",
        answers.unwrap_or_default().join(",")
    )
}

/// Four requests to `url` from the request and four from the work it
/// leaves in `waitUntil`, all counted among the invocation's six: how many
/// of the request's were answered, the deferred work keeping how many of
/// its own were under `<key>:<count>`, which `/deferred` reads.
async fn six_deferred(host: &WorkersHost, url: &str, key: &str) -> String {
    let fetch = Rc::new(WorkersFetch::counted(Rc::clone(host.opening())));
    let deferred = Rc::clone(&fetch);
    let (later, key) = (url.to_string(), key.to_string());
    host.defer(Box::pin(async move {
        let answers =
            join_all((0..4).map(|_| fetched_through(&deferred, &later, 5_000, None))).await;
        let answered = answers
            .iter()
            .filter(|answer| answer.starts_with("status:200"))
            .count();
        DONE.with(|done| done.borrow_mut().insert(format!("{key}:{answered}")));
    }));
    let answers = join_all((0..4).map(|_| fetched_through(&fetch, url, 5_000, None))).await;
    let answered = answers
        .iter()
        .filter(|answer| answer.starts_with("status:200"))
        .count();
    format!("answered:{answered}")
}

/// One call through `connection` under `max_concurrent: 1`, in the
/// isolate's slots, as the runtime makes it: the slot waited for in this
/// invocation, then a request to `url` while it is held. How long it
/// waited, and what the request answered.
async fn slot(host: &WorkersHost, url: &str, connection: &str) -> String {
    let began = host.monotonic_ms().unwrap_or(0);
    let held = jadpo_runtime::slots::wait(host, connection, 1).await;
    let waited = host.monotonic_ms().unwrap_or(0).saturating_sub(began);
    let fetch = WorkersFetch::counted(Rc::clone(host.opening()));
    let answer = jadpo_runtime::slots::keeping(
        host,
        Some(&held),
        fetched_through(&fetch, url, 60_000, None),
    )
    .await;
    drop(held);
    format!("waited:{waited} {}", short(&answer))
}
