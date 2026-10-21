//! The package driver (area 11, "Calling"; chunk 7.2c): one driver for
//! every provider package's connection, JSON over HTTP, as the package
//! declares it. The runtime makes each call's request from the package's
//! plan ([`jadpo_runtime::provider::Request`]); this driver sends it
//! through the [`Fetch`] its host gives (tokio and OpenSSL natively,
//! Workers' `fetch` there) and classifies what came back into the seam's
//! fixed set ([`jadpo_runtime::provider::classify`]): never sent, so
//! `unavailable`; sent with no answer, which may have acted, and told
//! apart when its timeout passed (chunk 7.2c2); or the provider's
//! answer.

use jadpo_runtime::LocalFuture;
use jadpo_runtime::capability::{Answer, Call, Capability};
use jadpo_runtime::provider::{Request, Response, classify};
use jadpo_runtime::stream::{Chunk, Coming, Read, Streamed};

/// Why an exchange gave no answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FetchError {
    /// It never left: the host wasn't found, nothing listened, or the
    /// connection was refused. Safe to send again.
    NotSent(String),
    /// It left and no whole answer came back: it may have acted.
    NoAnswer(String),
    /// It left and no answer came back within its timeout: it may have
    /// acted (chunk 7.2c2).
    TimedOut,
    /// The host can't send it at all: an address it can't reach this way,
    /// such as HTTPS where it has no TLS.
    Refused(String),
}

/// How a host sends an HTTP request and reads its answer: what the package
/// driver needs of it, as [`crate::Transport`] is what the PostgreSQL
/// driver needs.
pub trait Fetch {
    /// Sends `request` and reads the whole answer, waiting at most its
    /// `timeout_ms`.
    fn fetch<'a>(&'a self, request: &'a Request) -> LocalFuture<'a, Result<Response, FetchError>>;

    /// Sends `request` and gives its status and headers once they come,
    /// with its body to read as it comes (area 21, `ai.stream`; chunk
    /// 5.5v6): by default the whole answer, read first.
    fn open<'a>(&'a self, request: &'a Request) -> LocalFuture<'a, Result<Opened, FetchError>> {
        Box::pin(async move {
            let response = self.fetch(request).await?;
            Ok(Opened {
                status: response.status,
                headers: response.headers,
                body: Box::new(Read::new([Chunk::Bytes(response.body)])),
            })
        })
    }
}

/// A host's HTTP shared between drivers, as the factory holds it.
impl<F: Fetch + ?Sized> Fetch for std::rc::Rc<F> {
    fn fetch<'a>(&'a self, request: &'a Request) -> LocalFuture<'a, Result<Response, FetchError>> {
        (**self).fetch(request)
    }

    fn open<'a>(&'a self, request: &'a Request) -> LocalFuture<'a, Result<Opened, FetchError>> {
        (**self).open(request)
    }
}

/// An answer whose head has come, its body still coming.
pub struct Opened {
    pub status: u16,
    /// Each header's name in lowercase, and its value.
    pub headers: Vec<(String, String)>,
    pub body: Box<dyn Coming>,
}

impl std::fmt::Debug for Opened {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Opened")
            .field("status", &self.status)
            .finish_non_exhaustive()
    }
}

/// The package driver over `F`.
#[derive(Debug)]
pub struct JsonHttp<F> {
    fetch: F,
}

impl<F: Fetch> JsonHttp<F> {
    pub fn new(fetch: F) -> JsonHttp<F> {
        JsonHttp { fetch }
    }
}

impl<F: Fetch> Capability for JsonHttp<F> {
    fn call<'a>(&'a self, call: &'a Call) -> LocalFuture<'a, Answer> {
        Box::pin(async move {
            // Only a provider's operation comes with its request.
            let Some(request) = call.request.as_deref() else {
                return Answer::Misconfigured;
            };
            match self.fetch.fetch(request).await {
                Ok(response) => classify(request, &response),
                Err(FetchError::NotSent(_)) => Answer::Unavailable,
                Err(FetchError::NoAnswer(_)) => Answer::Unknown,
                Err(FetchError::TimedOut) => Answer::TimedOut,
                Err(FetchError::Refused(_)) => Answer::Misconfigured,
            }
        })
    }

    /// A success's body as it comes; anything else read whole and
    /// classified as [`Capability::call`] does.
    fn stream<'a>(&'a self, call: &'a Call) -> LocalFuture<'a, Streamed> {
        Box::pin(async move {
            let Some(request) = call.request.as_deref() else {
                return Streamed::Whole(Answer::Misconfigured);
            };
            let mut opened = match self.fetch.open(request).await {
                Ok(opened) => opened,
                Err(FetchError::NotSent(_)) => return Streamed::Whole(Answer::Unavailable),
                Err(FetchError::NoAnswer(_)) => return Streamed::Whole(Answer::Unknown),
                Err(FetchError::TimedOut) => return Streamed::Whole(Answer::TimedOut),
                Err(FetchError::Refused(_)) => return Streamed::Whole(Answer::Misconfigured),
            };
            if (200..300).contains(&opened.status) {
                return Streamed::Coming(opened.body);
            }
            let mut body = Vec::new();
            loop {
                match opened.body.next().await {
                    Chunk::Bytes(bytes) => body.extend_from_slice(&bytes),
                    Chunk::End => break,
                    Chunk::TimedOut => return Streamed::Whole(Answer::TimedOut),
                    Chunk::Broken => return Streamed::Whole(Answer::Unknown),
                }
            }
            let response = Response {
                status: opened.status,
                headers: opened.headers,
                body,
            };
            Streamed::Whole(classify(request, &response))
        })
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use jadpo_runtime::provider::Reading;
    use jadpo_std::HttpMethod;
    use jadpo_std::subscriber::Repeat;
    use jadpo_value::Value;

    use super::*;

    /// A host that answers each request from a script, keeping what it was
    /// sent.
    #[derive(Default)]
    struct Scripted {
        answers: RefCell<Vec<Result<Response, FetchError>>>,
        sent: RefCell<Vec<Request>>,
    }

    impl Fetch for Rc<Scripted> {
        fn fetch<'a>(
            &'a self,
            request: &'a Request,
        ) -> LocalFuture<'a, Result<Response, FetchError>> {
            self.sent.borrow_mut().push(request.clone());
            let next = self.answers.borrow_mut().remove(0);
            Box::pin(async move { next })
        }
    }

    fn call() -> Call {
        Call::new("couriers", "bookings.create", Value::Unit, Repeat::Keyed).with_request(Request {
            method: HttpMethod::Post,
            url: "https://api.example/v1/bookings".into(),
            headers: Vec::new(),
            body: Some("{}".into()),
            timeout_ms: 1000,
            reading: Reading::default(),
            repeatable: false,
        })
    }

    /// A call's request may be sent again once it has left exactly when
    /// the call may: a change under its key, or one that changes nothing,
    /// never a change with no key.
    #[test]
    fn a_request_may_be_sent_again_as_its_call_may() {
        let repeatable = |repeat: Repeat| {
            let request = call().request.expect("a request");
            Call::new("couriers", "bookings.create", Value::Unit, repeat)
                .with_request(*request)
                .request
                .is_some_and(|request| request.repeatable)
        };
        assert!(repeatable(Repeat::Keyed));
        assert!(repeatable(Repeat::Safe));
        assert!(!repeatable(Repeat::Unkeyed));
    }

    /// What never left is safe to send again; what left with no answer may
    /// have acted; an address the host can't reach is the connection's
    /// settings; and a call without its request isn't a provider's.
    #[test]
    fn the_driver_classifies_what_its_host_could_send() {
        let host = Rc::new(Scripted::default());
        *host.answers.borrow_mut() = vec![
            Ok(Response {
                status: 201,
                headers: Vec::new(),
                body: b"{\"id\":\"b1\"}".to_vec(),
            }),
            Err(FetchError::NotSent("refused".into())),
            Err(FetchError::NoAnswer("timed out".into())),
            Err(FetchError::Refused("no TLS".into())),
        ];
        let driver = JsonHttp::new(Rc::clone(&host));
        let made = call();
        let answers: Vec<Answer> = (0..4)
            .map(|_| jadpo_runtime::block_on(driver.call(&made)).expect("nothing waits"))
            .collect();
        assert_eq!(
            answers,
            [
                Answer::Wire(Rc::from("{\"id\":\"b1\"}")),
                Answer::Unavailable,
                Answer::Unknown,
                Answer::Misconfigured,
            ]
        );
        assert_eq!(host.sent.borrow().len(), 4);
        let bare = Call::new("couriers", "bookings.create", Value::Unit, Repeat::Keyed);
        assert_eq!(
            jadpo_runtime::block_on(driver.call(&bare)).expect("nothing waits"),
            Answer::Misconfigured
        );
    }
}
