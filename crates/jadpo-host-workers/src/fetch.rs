//! HTTP out of a Worker (docs/compiler-runtime.md §2.6, "The Workers
//! host"): [`WorkersFetch`], what the package driver sends a provider's
//! call with, and [`WorkersOutbound`], what the runtime fetches on its own
//! account, both over the Worker's global `fetch()`.
//!
//! - **A call ends with its timeout.** Its exchange, from sending to the
//!   last byte of its answer, is raised against a timer on `setTimeout`,
//!   and when the timer wins the request is aborted through the
//!   `AbortSignal` it was sent with, so the connection is let go rather
//!   than read for a call no one waits for. A streamed answer's head is
//!   waited for that long, and then each piece.
//! - **No redirect is followed** (`redirect: "manual"`): a `3xx` is an
//!   answer like any other, as natively.
//! - **What failed is told apart only by when.** A Worker's `fetch()`
//!   rejects with a `TypeError` whether nothing was sent or the
//!   connection broke after the request left, so every rejection is
//!   [`FetchError::NoAnswer`], a call that may have acted: a keyed or
//!   safe call is sent again, and a change with no key is never sent
//!   twice. An address `fetch()` can't send to is refused before it is
//!   tried.
//! - **At most 10 MB of an answer is read** ([`MOST_ANSWER`]), as natively.
//! - **A request waiting for its answer's head takes one of the
//!   invocation's six places** ([`Opening`]; chunk 8.2j), given back once
//!   the head comes or the request fails. A seventh waits for one within
//!   its own timeout, and one whose timeout passes while it waits never
//!   left: [`FetchError::NotSent`], safe to send again.
//! - **The outbound guard** sends a `GET` over HTTPS only, never to an
//!   address written as one that isn't public (loopback, private,
//!   link-local and the rest, [`jadpo_runtime::public_address`]) nor to
//!   `localhost`, follows no redirect and reads at most the bytes asked
//!   for. A name is not looked up first, as natively: a Worker has no
//!   resolver, and Cloudflare's network routes a Worker's requests to the
//!   public internet only.

use std::cell::Cell;
use std::rc::Rc;

use jadpo_drivers::package::{Fetch, FetchError, Opened};
use jadpo_runtime::provider::{MOST_ANSWER, Reading, Request, Response};
use jadpo_runtime::stream::{Chunk, Coming};
use jadpo_runtime::{LocalFuture, Outbound};
use jadpo_std::HttpMethod;
use js_sys::{Array, Reflect, Uint8Array};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    AbortController, Headers, ReadableStreamDefaultReader, RequestInit, RequestRedirect,
};

use crate::js;
use crate::opening::{Opening, Place};
use crate::timer::within;

/// The package driver's HTTP on Workers, counting its requests among its
/// invocation's six. A default one has six places of its own; an
/// invocation's host gives its drivers one sharing the invocation's
/// ([`crate::WorkersHost::supplies`]).
#[derive(Clone, Debug, Default)]
pub struct WorkersFetch {
    opening: Rc<Opening>,
}

impl WorkersFetch {
    /// A fetch whose requests take places among `opening`'s.
    pub fn counted(opening: Rc<Opening>) -> WorkersFetch {
        WorkersFetch { opening }
    }
}

/// Why a request whose timeout passed while it waited for a place never
/// went out.
const NO_PLACE: &str = "no connection of the invocation's six came free in time";

/// What a request whose time ran out was: never sent while it waited for
/// a place, else aborted, which may have acted.
fn ran_out(went: &Cell<bool>, abort: &AbortController) -> FetchError {
    if !went.get() {
        return FetchError::NotSent(NO_PLACE.into());
    }
    abort.abort();
    FetchError::TimedOut
}

impl Fetch for WorkersFetch {
    fn fetch<'a>(&'a self, request: &'a Request) -> LocalFuture<'a, Result<Response, FetchError>> {
        Box::pin(async move {
            sendable(&request.url)?;
            let abort = AbortController::new().map_err(|error| refused(&error))?;
            let went = Cell::new(false);
            let exchange = async {
                let place = self.opening.place().await;
                went.set(true);
                let answer = send(request, &abort, place).await?;
                let body = whole(&answer, MOST_ANSWER).await?;
                Ok(Response {
                    status: answer.status(),
                    headers: headers(&answer),
                    body,
                })
            };
            match within(request.timeout_ms, exchange).await {
                Some(done) => done,
                None => Err(ran_out(&went, &abort)),
            }
        })
    }

    /// The answer's head once it comes, within the call's timeout, and its
    /// body as it comes, each piece waited for that long.
    fn open<'a>(&'a self, request: &'a Request) -> LocalFuture<'a, Result<Opened, FetchError>> {
        Box::pin(async move {
            sendable(&request.url)?;
            let abort = AbortController::new().map_err(|error| refused(&error))?;
            let went = Cell::new(false);
            let head = async {
                let place = self.opening.place().await;
                went.set(true);
                send(request, &abort, place).await
            };
            let answer = match within(request.timeout_ms, head).await {
                Some(answer) => answer?,
                None => return Err(ran_out(&went, &abort)),
            };
            let reader = match answer.body() {
                Some(body) => {
                    Some(ReadableStreamDefaultReader::new(&body).map_err(|error| broken(&error))?)
                }
                None => None,
            };
            Ok(Opened {
                status: answer.status(),
                headers: headers(&answer),
                body: Box::new(Body {
                    reader,
                    abort,
                    wait: request.timeout_ms,
                    read: 0,
                    ended: false,
                }),
            })
        })
    }
}

/// Sends `request` through `fetch()`, abortable through `abort`, and gives
/// its answer's head, giving its `place` back once the head has come or it
/// failed.
async fn send(
    request: &Request,
    abort: &AbortController,
    place: Place<'_>,
) -> Result<web_sys::Response, FetchError> {
    let headers = Headers::new().map_err(|error| refused(&error))?;
    for (name, value) in &request.headers {
        headers
            .append(name, value)
            .map_err(|error| refused(&error))?;
    }
    let init = RequestInit::new();
    init.set_method(request.method.word());
    init.set_headers(&headers);
    if let Some(body) = &request.body {
        init.set_body(&JsValue::from_str(body));
    }
    init.set_redirect(RequestRedirect::Manual);
    init.set_signal(Some(&abort.signal()));
    let promise = js::fetch(&request.url, &init).map_err(|error| refused(&error))?;
    let answer = JsFuture::from(promise).await;
    drop(place);
    let answer = answer.map_err(|error| rejected(js::name(&error).as_deref(), js::said(&error)))?;
    answer
        .dyn_into::<web_sys::Response>()
        .map_err(|_| FetchError::NoAnswer("fetch() gave something other than a Response".into()))
}

/// Whether `fetch()` can send to `url`: an absolute HTTP or HTTPS address.
fn sendable(url: &str) -> Result<(), FetchError> {
    let parsed = web_sys::Url::new(url)
        .map_err(|_| FetchError::Refused(format!("`{url}` is not an address")))?;
    match parsed.protocol().as_str() {
        "http:" | "https:" => Ok(()),
        other => Err(FetchError::Refused(format!(
            "`{other}` is not HTTP or HTTPS"
        ))),
    }
}

/// What a rejected `fetch()` was: one aborted by its timer, which only a
/// timer that already ended the call aborts, ran out of time; anything
/// else may have reached the server, since Workers' `TypeError` doesn't
/// say whether the request left.
fn rejected(name: Option<&str>, said: String) -> FetchError {
    match name {
        Some("AbortError" | "TimeoutError") => FetchError::TimedOut,
        _ => FetchError::NoAnswer(said),
    }
}

/// A request the platform wouldn't make, refused before it is sent.
fn refused(error: &JsValue) -> FetchError {
    FetchError::Refused(js::said(error))
}

fn broken(error: &JsValue) -> FetchError {
    FetchError::NoAnswer(js::said(error))
}

/// Each header's name in lowercase, and its value.
fn headers(answer: &web_sys::Response) -> Vec<(String, String)> {
    listed(&answer.headers())
}

/// Each of `headers`' names in lowercase, and its value: a request's to a
/// Worker too (chunk 8.2d).
pub(crate) fn listed(headers: &Headers) -> Vec<(String, String)> {
    let Ok(Some(entries)) = js_sys::try_iter(headers) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let pair = Array::from(&entry);
            Some((
                pair.get(0).as_string()?.to_ascii_lowercase(),
                pair.get(1).as_string()?.trim().to_string(),
            ))
        })
        .collect()
}

/// The next piece `reader` gives, or `None` at its end: an answer's, or a
/// request's to a Worker (chunk 8.2d).
pub(crate) async fn piece(
    reader: &ReadableStreamDefaultReader,
) -> Result<Option<Vec<u8>>, FetchError> {
    loop {
        let result = JsFuture::from(reader.read())
            .await
            .map_err(|error| broken(&error))?;
        let done = Reflect::get(&result, &JsValue::from_str("done"))
            .map(|done| done.is_truthy())
            .unwrap_or(true);
        if done {
            return Ok(None);
        }
        let value =
            Reflect::get(&result, &JsValue::from_str("value")).map_err(|error| broken(&error))?;
        let Some(bytes) = value.dyn_ref::<Uint8Array>() else {
            return Err(FetchError::NoAnswer(
                "a body gave something other than bytes".into(),
            ));
        };
        if bytes.length() > 0 {
            return Ok(Some(bytes.to_vec()));
        }
    }
}

/// The whole of an answer's body, at most `limit` bytes.
async fn whole(answer: &web_sys::Response, limit: usize) -> Result<Vec<u8>, FetchError> {
    let Some(body) = answer.body() else {
        return Ok(Vec::new());
    };
    let reader = ReadableStreamDefaultReader::new(&body).map_err(|error| broken(&error))?;
    let mut read = Vec::new();
    while let Some(bytes) = piece(&reader).await? {
        if read.len() + bytes.len() > limit {
            let _ = reader.cancel();
            return Err(FetchError::NoAnswer("an answer too long".into()));
        }
        read.extend_from_slice(&bytes);
    }
    Ok(read)
}

/// A streamed answer's body, read as it comes. Dropped before its end, or
/// broken off, its reader is cancelled and its request aborted.
struct Body {
    reader: Option<ReadableStreamDefaultReader>,
    abort: AbortController,
    /// The longest each piece is waited for.
    wait: u64,
    /// How much has been read.
    read: usize,
    /// Whether it has ended, whole or not.
    ended: bool,
}

impl Body {
    fn stop(&mut self) {
        self.ended = true;
        if let Some(reader) = self.reader.take() {
            let _ = reader.cancel();
        }
        self.abort.abort();
    }
}

impl Drop for Body {
    fn drop(&mut self) {
        if !self.ended {
            self.stop();
        }
    }
}

impl Coming for Body {
    fn next(&mut self) -> LocalFuture<'_, Chunk> {
        Box::pin(async move {
            if self.ended {
                return Chunk::Broken;
            }
            let Some(reader) = self.reader.clone() else {
                self.ended = true;
                return Chunk::End;
            };
            match within(self.wait, piece(&reader)).await {
                None => {
                    self.stop();
                    Chunk::TimedOut
                }
                Some(Err(_)) => {
                    self.stop();
                    Chunk::Broken
                }
                Some(Ok(None)) => {
                    self.ended = true;
                    self.reader = None;
                    Chunk::End
                }
                Some(Ok(Some(bytes))) => {
                    self.read += bytes.len();
                    if self.read > MOST_ANSWER {
                        self.stop();
                        return Chunk::Broken;
                    }
                    Chunk::Bytes(bytes)
                }
            }
        })
    }
}

/// What the runtime fetches on its own account on Workers (area 11,
/// "Receiving from outside"): AWS SNS's signing certificates and
/// subscription confirmations, guarded as the module says.
#[derive(Clone, Debug, Default)]
pub struct WorkersOutbound {
    /// A test's server let through in a public host's place: its origin,
    /// as `URL.origin` writes it.
    reach: Option<String>,
    /// The invocation's places, which its requests take one of each.
    opening: Rc<Opening>,
}

impl WorkersOutbound {
    /// Lets requests for `origin` (`https://localhost:8443`), a test's
    /// server on this machine, through though it isn't public: never built
    /// outside a test.
    #[cfg(any(test, feature = "test-outbound"))]
    pub fn reaching(origin: &str) -> WorkersOutbound {
        WorkersOutbound {
            reach: Some(origin.trim_end_matches('/').to_ascii_lowercase()),
            opening: Rc::default(),
        }
    }

    /// The same guard, its requests taking places among `opening`'s: the
    /// invocation's, when its host is given it.
    pub(crate) fn counted(self, opening: Rc<Opening>) -> WorkersOutbound {
        WorkersOutbound { opening, ..self }
    }
}

/// Dropping the outer route while discovery is awaiting a JS promise
/// must abort that fetch too; dropping a controller alone does not.
struct AbortOnDrop(AbortController);
impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

impl Outbound for WorkersOutbound {
    fn get<'a>(
        &'a self,
        url: &'a str,
        limit: usize,
        timeout_ms: u64,
    ) -> LocalFuture<'a, Result<(u16, Vec<u8>), String>> {
        Box::pin(async move {
            let parsed =
                web_sys::Url::new(url).map_err(|_| format!("`{url}` is not an address"))?;
            if parsed.protocol() != "https:" {
                return Err("not HTTPS".into());
            }
            if self.reach.as_deref() != Some(parsed.origin().as_str()) {
                outbound_host(&parsed.hostname())?;
            }
            let request = Request {
                method: HttpMethod::Get,
                url: url.to_string(),
                headers: vec![("accept".into(), "*/*".into())],
                body: None,
                timeout_ms,
                reading: Reading::default(),
                repeatable: true,
            };
            let abort = AbortController::new().map_err(|error| js::said(&error))?;
            let _cancellation = AbortOnDrop(abort.clone());
            let went = Cell::new(false);
            let exchange = async {
                let place = self.opening.place().await;
                went.set(true);
                let answer = send(&request, &abort, place).await?;
                let body = whole(&answer, limit.min(MOST_ANSWER)).await?;
                Ok::<_, FetchError>((answer.status(), body))
            };
            match within(timeout_ms, exchange).await {
                Some(Ok(done)) => Ok(done),
                Some(Err(error)) => Err(said(error)),
                None => Err(said(ran_out(&went, &abort))),
            }
        })
    }
}

/// Why an exchange gave no answer, in the words the native guard uses.
fn said(error: FetchError) -> String {
    match error {
        FetchError::NotSent(why) | FetchError::NoAnswer(why) | FetchError::Refused(why) => why,
        FetchError::TimedOut => "no answer in time".into(),
    }
}

/// Whether the outbound guard may send to `host`, a URL's host name as
/// the WHATWG parser leaves it (an IPv6 address in its brackets, an IPv4
/// address in dotted decimal however it was written): not an address
/// that isn't public, and not `localhost`.
pub(crate) fn outbound_host(host: &str) -> Result<(), String> {
    let bare = host.trim_start_matches('[').trim_end_matches(']');
    let refused = || Err(format!("`{bare}` has no public address"));
    if bare.eq_ignore_ascii_case("localhost") || bare.to_ascii_lowercase().ends_with(".localhost") {
        return refused();
    }
    match bare.parse::<std::net::IpAddr>() {
        Ok(ip) if !jadpo_runtime::public_address(ip) => refused(),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Only an abort, which only a timer that already ended the call
    /// makes, ran out of time; every other rejection may have acted.
    #[test]
    fn a_rejection_may_have_acted_unless_its_timer_aborted_it() {
        assert_eq!(
            rejected(Some("AbortError"), "aborted".into()),
            FetchError::TimedOut
        );
        assert_eq!(
            rejected(Some("TimeoutError"), "timed out".into()),
            FetchError::TimedOut
        );
        assert_eq!(
            rejected(Some("TypeError"), "Network connection lost.".into()),
            FetchError::NoAnswer("Network connection lost.".into())
        );
        assert_eq!(rejected(None, "?".into()), FetchError::NoAnswer("?".into()));
    }

    /// The guard refuses an address written as one that isn't public, and
    /// `localhost`, and lets a name through for the platform to route.
    #[test]
    fn the_outbound_guard_refuses_addresses_that_arent_public() {
        for host in [
            "127.0.0.1",
            "10.0.0.8",
            "192.168.1.1",
            "169.254.169.254",
            "100.64.0.1",
            "[::1]",
            "[fe80::1]",
            "[fd00::1]",
            "[::ffff:127.0.0.1]",
            "localhost",
            "LOCALHOST",
            "api.localhost",
            "0.0.0.0",
        ] {
            let bare = host.trim_start_matches('[').trim_end_matches(']');
            assert_eq!(
                outbound_host(host),
                Err(format!("`{bare}` has no public address")),
                "{host}"
            );
        }
        for host in [
            "sns.us-east-1.amazonaws.com",
            "8.8.8.8",
            "[2606:4700::1111]",
        ] {
            assert_eq!(outbound_host(host), Ok(()), "{host}");
        }
    }
}
