//! The package driver (area 11, "Calling"; chunk 7.2c): one driver for
//! every provider package's connection, JSON over HTTP, as the package
//! declares it. The runtime makes each call's request from the package's
//! plan ([`jadpo_runtime::provider::Request`]); this driver sends it
//! through the [`Fetch`] its host gives (tokio and OpenSSL natively,
//! Workers' `fetch` there) and classifies what came back into the seam's
//! fixed set ([`jadpo_runtime::provider::classify`]): never sent, so
//! `unavailable`; sent with no answer, which may have acted; or the
//! provider's answer.

use jadpo_runtime::LocalFuture;
use jadpo_runtime::capability::{Answer, Call, Capability};
use jadpo_runtime::provider::{Request, Response, classify};

/// Why an exchange gave no answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FetchError {
    /// It never left: the host wasn't found, nothing listened, or the
    /// connection was refused. Safe to send again.
    NotSent(String),
    /// It left and no whole answer came back in time: it may have acted.
    NoAnswer(String),
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
                Err(FetchError::Refused(_)) => Answer::Misconfigured,
            }
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
        Call::new("couriers", "bookings.create", Value::Unit, Repeat::Keyed).with_request(
            Request {
                method: HttpMethod::Post,
                url: "https://api.example/v1/bookings".into(),
                headers: Vec::new(),
                body: Some("{}".into()),
                timeout_ms: 1000,
                reading: Reading::default(),
            },
        )
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
