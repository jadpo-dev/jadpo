//! Serving an application (docs/compiler-runtime.md §2.6; chunk 6.9):
//! what every host does with a request and with a tick, so a host only
//! reads and writes its platform's bytes and keeps its own background
//! work.
//!
//! [`Serving`] answers a request with [`serve_request`] and hands the work
//! it left to [`runner::after_response`], which the host runs once the
//! response is sent; it runs the tick ([`schedules::tick`]), which raises
//! the alerts that have just started. An [`Observer`] hears each request
//! answered and each tick: `jadpo dev` prints its lines from them, and a
//! deployable's log comes with the hosts' log sink (area 16). The native
//! server's handler wraps it (`jadpo_host_native::http::AppHandler`);
//! Workers' fetch event calls the same (chunk 8.2d), one `Serving` a
//! request on that invocation's host, sharing the isolate's [`Worker`]
//! ([`Serving::sharing`]), and its scheduled event will (chunk 8.2f).
//!
//! What it serves is any [`Executor`]: the interpreter, or compiled code
//! (Phase 8). The route table and codecs a request is answered with come
//! from the executor's plan ([`crate::Plan::app`], chunk 8.1a).

use std::rc::Rc;

use crate::http::{Request, Response, Worker, serve_request};
use crate::schedules::{self, Ticked};
use crate::{Executor, Host, LocalFuture, runner};

/// One request answered, as an [`Observer`] hears it.
pub struct Answered<'a> {
    pub request: &'a Request,
    /// The response as it is sent; the work it left has been handed on.
    pub response: &'a Response,
    /// The program that answered it, whose places a fault's frames name.
    pub program: &'a dyn Executor,
    /// How long answering it took, in milliseconds on the host's monotonic
    /// clock; 0 on a host without one.
    pub ms: u64,
}

/// What hears what [`Serving`] does, on the thread it serves on.
pub trait Observer {
    /// A request has been answered, before its response is sent.
    fn answered(&self, answered: &Answered<'_>) {
        let _ = answered;
    }

    /// A tick at `now` has ended on `program`: the schedules it ran and
    /// the alerts it raised.
    fn ticked(&self, program: &dyn Executor, ticked: &Ticked, now: i64) {
        let _ = (program, ticked, now);
    }
}

/// An application served on one host: the version of the program to serve
/// now, the host, what the runtime keeps between that host's requests, and
/// who hears about it.
pub struct Serving {
    program: Box<dyn Fn() -> Rc<dyn Executor>>,
    host: Rc<dyn Host>,
    worker: Rc<Worker>,
    observer: Option<Rc<dyn Observer>>,
}

impl std::fmt::Debug for Serving {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Serving")
            .field("worker", &self.worker)
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for Answered<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Answered")
            .field("request", self.request)
            .field("response", self.response)
            .field("ms", &self.ms)
            .finish_non_exhaustive()
    }
}

impl Serving {
    /// Serves on `host` the program `program` gives each time it is asked:
    /// always the same one for a deployable, the latest version that
    /// checked cleanly under `jadpo dev`.
    pub fn new(host: Rc<dyn Host>, program: impl Fn() -> Rc<dyn Executor> + 'static) -> Serving {
        Serving {
            program: Box::new(program),
            host,
            worker: Rc::default(),
            observer: None,
        }
    }

    /// Keeps what the HTTP component keeps between requests in `worker`,
    /// which other `Serving`s share: a Workers isolate serves each request
    /// on a host of its own, since a socket belongs to the request that
    /// opened it, and counts and checks readiness once for them all.
    pub fn sharing(mut self, worker: Rc<Worker>) -> Serving {
        self.worker = worker;
        self
    }

    /// Tells `observer` about each request and tick.
    pub fn observed_by(mut self, observer: Rc<dyn Observer>) -> Serving {
        self.observer = Some(observer);
        self
    }

    /// The version of the program being served now.
    pub fn program(&self) -> Rc<dyn Executor> {
        (self.program)()
    }

    /// Answers `request` with the version being served now; what the
    /// response left runs after it is sent, through the host's
    /// [`Host::defer`] (chunk 6.1a).
    pub async fn handle(&self, request: &Request) -> Response {
        let began = self.host.monotonic_ms();
        let program = self.program();
        let mut response = {
            let app = program.plan().app();
            serve_request(
                &app,
                program.as_ref(),
                self.host.as_ref(),
                &self.worker,
                request,
            )
            .await
        };
        // A model's answer the route streams is read on this host, whose
        // clock holds it to its time (chunk 5.5v6).
        if let Some(stream) = &response.stream {
            stream.attach(Rc::clone(&self.host));
        }
        runner::after_response(
            Rc::clone(&program),
            Rc::clone(&self.host),
            std::mem::take(&mut response.deferred),
        );
        if let Some(observer) = &self.observer {
            let ms = match (began, self.host.monotonic_ms()) {
                (Some(began), Some(end)) => end.saturating_sub(began),
                _ => 0,
            };
            observer.answered(&Answered {
                request,
                response: &response,
                program: program.as_ref(),
                ms,
            });
        }
        response
    }

    /// The runtime's tick on the version being served now, at the host's
    /// time: what it did, or `None` on a host without a clock.
    pub fn tick(&self) -> LocalFuture<'static, Option<Ticked>> {
        let program = self.program();
        let host = Rc::clone(&self.host);
        let observer = self.observer.clone();
        Box::pin(async move {
            let now = host.now_ms()?;
            let ticked = schedules::tick(program.as_ref(), host.as_ref(), now).await;
            if let Some(observer) = observer {
                observer.ticked(program.as_ref(), &ticked, now);
            }
            Some(ticked)
        })
    }

    /// How often the host's timer ticks for the version being served now.
    pub fn tick_every_ms(&self) -> u64 {
        schedules::tick_every_ms(self.program().plan().schedules())
    }
}
