//! The webhook driver (area 12, "Where events travel"; area 16, "Outgoing
//! webhooks"): the `publishing` capability for a connection with `driver:
//! Driver.webhook`, posting each event a delivery hands it to the one
//! address `url:` gives, over the host's [`Fetch`], natively and on
//! Workers.
//!
//! - **What it sends.** A `POST` of the event's body
//!   ([`jadpo_std::publishing::body`]) as `application/json`, with
//!   `webhook-id` (the event's id), `webhook-timestamp` (the seconds since
//!   1970 on the host's clock as it is sent) and `webhook-signature`
//!   (Standard Webhooks' `v1,` and the Base64 of the HMAC-SHA-256 of
//!   `<id>.<timestamp>.<body>` under the connection's secret), and the
//!   delivering work's `traceparent`.
//! - **What its answer says.** A `2xx` is delivered. A `408`, a `429` or a
//!   `5xx`, a request that never left and one whose answer never came are
//!   tried again, under the same id, after the delivery's backoff; a
//!   redirect,
//!   never followed, and a `401` or `403` say the connection's address or
//!   secret is wrong (`misconfigured`); any other refusal is the
//!   receiver's answer to this event (`refused`), which the same event
//!   would get again. Each ends the delivery as a subscriber's failure of
//!   that kind does (area 12, "Retries and giving up").

use std::rc::Rc;

use jadpo_runtime::LocalFuture;
use jadpo_runtime::capability::{Answer, Call, Capability};
use jadpo_runtime::provider::{Reading, Request};
use jadpo_runtime::publishing::{Published, signature, signing_key};
use jadpo_std::HttpMethod;

use crate::package::{Fetch, FetchError};
use crate::s3::Clock;

/// The failure code of an event its receiver refused.
pub const REFUSED: &str = "refused";

/// A connection's webhook: its address, its key and the host's HTTP and
/// clock.
pub struct Webhook {
    fetch: Rc<dyn Fetch>,
    clock: Clock,
    url: String,
    key: Vec<u8>,
}

impl std::fmt::Debug for Webhook {
    /// Never its address or its key.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Webhook").finish_non_exhaustive()
    }
}

impl Webhook {
    /// The webhook posting to `url`, signed with `secret`; why not, never
    /// either value, when `url` isn't an address a call may be sent to.
    pub fn new(
        fetch: Rc<dyn Fetch>,
        clock: Clock,
        url: &str,
        secret: &str,
    ) -> Result<Webhook, String> {
        let url = jadpo_std::provider::sendable(url)
            .map(|_| url.trim().to_string())
            .ok_or("its `url:` isn't an HTTPS address, or plain HTTP to this machine")?;
        if secret.is_empty() {
            return Err("its `secret:` is empty".into());
        }
        Ok(Webhook {
            fetch,
            clock,
            url,
            key: signing_key(secret),
        })
    }

    /// The signed request `published` is posted as, at `now`.
    pub fn request(&self, published: &Published, now: i64) -> Request {
        let seconds = now.div_euclid(1000);
        let mut headers = vec![
            ("content-type".to_string(), "application/json".to_string()),
            ("webhook-id".to_string(), published.id.clone()),
            ("webhook-timestamp".to_string(), seconds.to_string()),
            (
                "webhook-signature".to_string(),
                signature(&self.key, &published.id, seconds, &published.body),
            ),
        ];
        if let Some(traceparent) = &published.traceparent {
            headers.push(("traceparent".to_string(), traceparent.clone()));
        }
        Request {
            method: HttpMethod::Post,
            url: self.url.clone(),
            headers,
            body: Some(published.body.clone()),
            raw_body: None,
            timeout_ms: jadpo_std::publishing::TIMEOUT_MS,
            reading: Reading::default(),
            // Sent under its id, a receiver keeps it once.
            repeatable: true,
        }
    }
}

impl Capability for Webhook {
    fn call<'a>(&'a self, call: &'a Call) -> LocalFuture<'a, Answer> {
        Box::pin(async move {
            let published = match (call.operation.as_str(), Published::from_value(&call.input)) {
                (jadpo_std::publishing::PUBLISH, Some(published)) => published,
                _ => return Answer::Misconfigured,
            };
            let request = self.request(&published, (self.clock)());
            match self.fetch.fetch(&request).await {
                Ok(response) => answer(response.status, &response.headers),
                Err(FetchError::NotSent(_)) => Answer::Unavailable,
                Err(FetchError::NoAnswer(_)) => Answer::Unknown,
                Err(FetchError::TimedOut) => Answer::TimedOut,
                Err(FetchError::Refused(_)) => Answer::Misconfigured,
            }
        })
    }
}

/// The runtime-owned exception the addresses customers register are
/// reached through (area 16, "Outgoing webhooks"; chunk C1b), the
/// `outgoing_webhooks` capability: each call is one event already signed
/// for one address ([`jadpo_runtime::outgoing::Outgoing`]), posted over the
/// host's guarded HTTP, never its open one: natively `Outward`, HTTPS only
/// and connected only to a public address the name gives as the attempt
/// is made, no redirect followed. Its answer is read as [`Webhook`]'s; an
/// address the guard refuses is `misconfigured`, since it would be refused
/// again.
pub struct Outgoing {
    fetch: Rc<dyn Fetch>,
}

impl std::fmt::Debug for Outgoing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Outgoing").finish_non_exhaustive()
    }
}

impl Outgoing {
    /// The exception's driver over `fetch`, which must be the host's
    /// guarded HTTP ([`crate::factory::Supplies::outward`]).
    pub fn new(fetch: Rc<dyn Fetch>) -> Outgoing {
        Outgoing { fetch }
    }

    /// The request `outgoing` is posted as.
    pub fn request(outgoing: &jadpo_runtime::outgoing::Outgoing) -> Request {
        let published = &outgoing.published;
        let mut headers = vec![
            ("content-type".to_string(), "application/json".to_string()),
            ("webhook-id".to_string(), published.id.clone()),
            (
                "webhook-timestamp".to_string(),
                outgoing.timestamp.to_string(),
            ),
            ("webhook-signature".to_string(), outgoing.signature.clone()),
        ];
        if let Some(traceparent) = &published.traceparent {
            headers.push(("traceparent".to_string(), traceparent.clone()));
        }
        Request {
            method: HttpMethod::Post,
            url: outgoing.url.clone(),
            headers,
            body: Some(published.body.clone()),
            raw_body: None,
            timeout_ms: jadpo_std::publishing::TIMEOUT_MS,
            reading: Reading::default(),
            // Sent under its id, a receiver keeps it once.
            repeatable: true,
        }
    }
}

impl Capability for Outgoing {
    fn call<'a>(&'a self, call: &'a Call) -> LocalFuture<'a, Answer> {
        Box::pin(async move {
            let outgoing = match (
                call.operation.as_str(),
                jadpo_runtime::outgoing::Outgoing::from_value(&call.input),
            ) {
                (jadpo_std::outgoing::POST, Some(outgoing)) => outgoing,
                _ => return Answer::Misconfigured,
            };
            let request = Outgoing::request(&outgoing);
            match self.fetch.fetch(&request).await {
                Ok(response) => answer(response.status, &response.headers),
                Err(FetchError::NotSent(_)) => Answer::Unavailable,
                Err(FetchError::NoAnswer(_)) => Answer::Unknown,
                Err(FetchError::TimedOut) => Answer::TimedOut,
                // Not HTTPS, or no public address: refused again next time.
                Err(FetchError::Refused(_)) => Answer::Misconfigured,
            }
        })
    }
}

/// What a receiver answering `status` with `headers` says of the event.
fn answer(status: u16, headers: &[(String, String)]) -> Answer {
    match status {
        200..=299 => Answer::Value(jadpo_value::Value::Unit),
        // Never followed: the address is wrong.
        300..=399 | 401 | 403 => Answer::Misconfigured,
        408 | 429 | 500..=599 => headers
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case("retry-after"))
            .and_then(|(_, value)| value.trim().parse::<u64>().ok())
            .and_then(|seconds| seconds.checked_mul(1000))
            .map_or(Answer::Unavailable, Answer::RetryAfter),
        _ => Answer::Failed(REFUSED.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use jadpo_runtime::provider::Response;

    use super::*;

    #[derive(Default)]
    struct Scripted {
        answers: RefCell<Vec<Result<Response, FetchError>>>,
        sent: RefCell<Vec<Request>>,
    }

    impl Fetch for Scripted {
        fn fetch<'a>(
            &'a self,
            request: &'a Request,
        ) -> LocalFuture<'a, Result<Response, FetchError>> {
            self.sent.borrow_mut().push(request.clone());
            let next = self.answers.borrow_mut().remove(0);
            Box::pin(async move { next })
        }
    }

    fn status(status: u16) -> Result<Response, FetchError> {
        Ok(Response {
            status,
            headers: Vec::new(),
            body: Vec::new(),
        })
    }

    fn block_on<F: std::future::Future>(future: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("a runtime")
            .block_on(future)
    }

    /// A secret made for the test from `from`, never one written in
    /// source.
    fn secret_from(from: u8) -> String {
        let bytes: Vec<u8> = (0u8..32)
            .map(|at| at.wrapping_mul(29).wrapping_add(from))
            .collect();
        format!("whsec_{}", jadpo_value::encoding::to_base64(&bytes))
    }

    fn secret() -> String {
        secret_from(5)
    }

    fn published() -> Published {
        Published {
            id: "4a8b1c62-0d6f-4e0a-9b1d-7c3e2f1a0b9c".into(),
            type_name: "planning.TodoCompleted".into(),
            body: jadpo_std::publishing::body(
                "4a8b1c62-0d6f-4e0a-9b1d-7c3e2f1a0b9c",
                "planning.TodoCompleted",
                "{\"todo_id\":\"t-1\"}",
            ),
            traceparent: Some("00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01".into()),
        }
    }

    #[test]
    fn it_posts_the_body_signed_as_standard_webhooks_signs_with_its_trace() {
        let fetch = Rc::new(Scripted::default());
        fetch.answers.borrow_mut().push(status(204));
        let secret = secret();
        let hook = Webhook::new(
            Rc::clone(&fetch) as Rc<dyn Fetch>,
            Rc::new(|| 1_792_504_800_123),
            "https://hooks.example/jadpo",
            &secret,
        )
        .expect("a webhook");
        let answer = block_on(hook.call(&published().call("partner_hooks")));
        assert!(matches!(answer, Answer::Value(_)), "{answer:?}");
        let sent = fetch.sent.borrow();
        let request = sent.first().expect("one request");
        assert_eq!(request.method, HttpMethod::Post);
        assert_eq!(request.url, "https://hooks.example/jadpo");
        assert_eq!(request.body.as_deref(), Some(published().body.as_str()));
        assert_eq!(request.header("webhook-id"), Some(published().id.as_str()));
        assert_eq!(request.header("webhook-timestamp"), Some("1792504800"));
        assert_eq!(
            request.header("traceparent"),
            published().traceparent.as_deref()
        );
        // The runtime's own check of a Standard Webhooks signature takes it.
        let received = jadpo_runtime::http::Request {
            headers: request
                .headers
                .iter()
                .map(|(name, value)| (name.clone(), value.clone()))
                .collect(),
            body: request.body.clone().unwrap_or_default().into_bytes(),
            ..jadpo_runtime::http::Request::default()
        };
        assert_eq!(
            jadpo_runtime::webhooks::check(
                &jadpo_std::provider::SignatureScheme::StandardWebhooks,
                &[secret.as_str()],
                &received,
                1_792_504_801_000,
            ),
            Ok(Some(1_792_504_800_000))
        );
        // A different secret doesn't.
        assert!(
            jadpo_runtime::webhooks::check(
                &jadpo_std::provider::SignatureScheme::StandardWebhooks,
                &[secret_from(6).as_str()],
                &received,
                1_792_504_801_000,
            )
            .is_err()
        );
    }

    #[test]
    fn its_receivers_answers_say_whether_to_try_again() {
        let cases: [(Result<Response, FetchError>, &str); 9] = [
            (status(200), "value"),
            (status(503), "unavailable"),
            (status(429), "unavailable"),
            (status(408), "unavailable"),
            (status(302), "misconfigured"),
            (status(401), "misconfigured"),
            (status(422), "failed"),
            (
                Err(FetchError::NotSent("nothing listens".into())),
                "unavailable",
            ),
            (Err(FetchError::NoAnswer("closed".into())), "unknown"),
        ];
        for (answer, expected) in cases {
            let fetch = Rc::new(Scripted::default());
            fetch.answers.borrow_mut().push(answer);
            let hook = Webhook::new(
                Rc::clone(&fetch) as Rc<dyn Fetch>,
                Rc::new(|| 0),
                "http://127.0.0.1:9/hook",
                &secret(),
            )
            .expect("a webhook");
            let got = block_on(hook.call(&published().call("partner_hooks")));
            let word = match got {
                Answer::Value(_) => "value",
                Answer::Unavailable | Answer::RetryAfter(_) => "unavailable",
                Answer::Misconfigured => "misconfigured",
                Answer::Failed(code) if code == REFUSED => "failed",
                Answer::Unknown => "unknown",
                _ => "other",
            };
            assert_eq!(word, expected);
        }
    }

    #[test]
    fn it_refuses_an_address_a_call_may_not_go_to() {
        let fetch: Rc<dyn Fetch> = Rc::new(Scripted::default());
        assert!(
            Webhook::new(
                Rc::clone(&fetch),
                Rc::new(|| 0),
                "http://hooks.example/jadpo",
                &secret()
            )
            .is_err()
        );
        assert!(Webhook::new(fetch, Rc::new(|| 0), "https://hooks.example/jadpo", "").is_err());
    }

    /// An address a customer registered is posted the event already
    /// signed for it, and nothing else: the driver holds no secret. An
    /// address the guarded client refuses, not HTTPS or not public, is
    /// `misconfigured`; the rest of its answers are read as a
    /// connection's.
    #[test]
    fn an_outgoing_post_is_sent_as_signed_and_its_answer_read() {
        let secret = secret_from(9);
        let signed = jadpo_runtime::outgoing::Outgoing::signed(
            "https://hooks.customer.example/in?shop=4",
            &published(),
            &secret,
            1_792_504_800_250,
        );
        let fetch = Rc::new(Scripted::default());
        fetch.answers.borrow_mut().push(status(204));
        let outgoing = Outgoing::new(Rc::clone(&fetch) as Rc<dyn Fetch>);
        assert!(matches!(
            block_on(outgoing.call(&signed.call())),
            Answer::Value(_)
        ));
        let sent = fetch.sent.borrow();
        let request = sent.first().expect("one request");
        assert_eq!(request.method, HttpMethod::Post);
        assert_eq!(request.url, "https://hooks.customer.example/in?shop=4");
        assert!(request.repeatable, "sent again under its id");
        assert_eq!(request.header("webhook-timestamp"), Some("1792504800"));
        let received = jadpo_runtime::http::Request {
            headers: request.headers.clone(),
            body: request.body.clone().unwrap_or_default().into_bytes(),
            ..jadpo_runtime::http::Request::default()
        };
        assert_eq!(
            jadpo_runtime::webhooks::check(
                &jadpo_std::provider::SignatureScheme::StandardWebhooks,
                &[secret.as_str()],
                &received,
                1_792_504_801_000,
            ),
            Ok(Some(1_792_504_800_000))
        );
        assert!(
            !request
                .headers
                .iter()
                .any(|(_, value)| value.contains(secret.trim_start_matches("whsec_"))),
            "the secret is never sent"
        );
        drop(sent);
        for (answer, expected) in [
            (
                Err(FetchError::Refused(
                    "`localhost` has no public address".into(),
                )),
                "misconfigured",
            ),
            (
                Err(FetchError::Refused("not HTTPS".into())),
                "misconfigured",
            ),
            (status(302), "misconfigured"),
            (status(503), "unavailable"),
            (status(410), "failed"),
        ] {
            let fetch = Rc::new(Scripted::default());
            fetch.answers.borrow_mut().push(answer);
            let outgoing = Outgoing::new(Rc::clone(&fetch) as Rc<dyn Fetch>);
            let word = match block_on(outgoing.call(&signed.call())) {
                Answer::Value(_) => "value",
                Answer::Unavailable | Answer::RetryAfter(_) => "unavailable",
                Answer::Misconfigured => "misconfigured",
                Answer::Failed(code) if code == REFUSED => "failed",
                _ => "other",
            };
            assert_eq!(word, expected);
        }
        // A call that isn't a signed post is refused before anything goes.
        let fetch = Rc::new(Scripted::default());
        let outgoing = Outgoing::new(Rc::clone(&fetch) as Rc<dyn Fetch>);
        assert!(matches!(
            block_on(outgoing.call(&published().call("outgoing_webhooks"))),
            Answer::Misconfigured
        ));
        assert!(fetch.sent.borrow().is_empty());
    }
}
