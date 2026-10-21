//! The probe Worker the Workers host's tests run under workerd
//! (tests/workerd.rs; chunk 8.2a): each path uses one part of the host and
//! answers what it saw as one line of text, `error:` and why when a part
//! refused. It never panics, which would abort the isolate. The passkey
//! paths (chunk 8.2l) run one step of a ceremony each, the state and the
//! device's answer handed back and forth by the test as base64url.

use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;
use std::io;
use std::rc::Rc;

use jadpo_drivers::package::{Fetch, FetchError};
use jadpo_drivers::{Endpoint, Stream, Transport, Trust, within};
use jadpo_host_workers::{WorkersFetch, WorkersHost, WorkersOutbound, WorkersTransport, supplies};
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
        "/unix" => match WorkersTransport
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
            let transport = WorkersTransport;
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
            WorkersTransport.spawn(Box::pin(async move {
                seen.set(true);
            }));
            WorkersTransport.sleep(10).await;
            if ran.get() {
                "ran".to_string()
            } else {
                "didn't run".to_string()
            }
        }
        "/supplies" => {
            let supplies = supplies();
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
    let transport = WorkersTransport;
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
    match within(&transport, 5_000, talk).await {
        Some(Ok(answer)) => format!("answer:{answer}"),
        Some(Err(error)) => failed(&error),
        None => "error:timed out".to_string(),
    }
}

/// An upgrade trusting an authority of the connection's own.
async fn authority(port: u64) -> String {
    let transport = WorkersTransport;
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
    let made = request(url, timeout_ms, body);
    match WorkersFetch.fetch(&made).await {
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
    let mut opened = match WorkersFetch.open(&made).await {
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
