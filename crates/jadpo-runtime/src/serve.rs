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
//! ([`Serving::sharing`]), and its scheduled event ticks through it
//! (chunk 8.2f).
//!
//! What it serves is any [`Executor`]: the interpreter, or compiled code
//! (Phase 8). A request is answered with what the executor answers its
//! routes with ([`Executor::app`]): the route table and codecs of its plan
//! ([`crate::Plan::app`], chunk 8.1a) and, from compiled code, each
//! route's typed entry (chunk 8.1p), so a deployable runs what its route
//! tests run (chunk 8.4c).

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
    /// What answering it did: a span for each action, read, write and
    /// outside call, in the order each started (chunk 10.3c).
    pub spans: &'a crate::spans::Spans,
}

/// What hears what [`Serving`] does, on the thread it serves on.
pub trait Observer {
    /// A request has been answered, before its response is sent.
    fn answered(&self, answered: &Answered<'_>) {
        let _ = answered;
    }

    /// A tick at `now` has ended on `program`, `ms` milliseconds after it
    /// started on the host's monotonic clock (0 on a host without one):
    /// the schedules it ran, the alerts it raised and the stores it
    /// couldn't work in.
    fn ticked(&self, program: &dyn Executor, ticked: &Ticked, now: i64, ms: u64) {
        let _ = (program, ticked, now, ms);
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
            .field("spans", self.spans)
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
        // The flags' state, read again when it is due (area 16).
        if let Some(now) = self.host.now_ms() {
            crate::flags::refresh(program.as_ref(), self.host.as_ref(), now).await;
        }
        let mut response = {
            let app = program.app();
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
        let spans = std::mem::take(&mut response.spans);
        if let Some(observer) = &self.observer {
            observer.answered(&Answered {
                request,
                response: &response,
                program: program.as_ref(),
                ms: since(self.host.as_ref(), began),
                spans: &spans,
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
            let began = host.monotonic_ms();
            crate::flags::refresh(program.as_ref(), host.as_ref(), now).await;
            let ticked = schedules::tick(program.as_ref(), host.as_ref(), now).await;
            if let Some(observer) = observer {
                let ms = since(host.as_ref(), began);
                observer.ticked(program.as_ref(), &ticked, now, ms);
            }
            Some(ticked)
        })
    }

    /// How often the host's timer ticks for the version being served now.
    pub fn tick_every_ms(&self) -> u64 {
        schedules::tick_every_ms(self.program().plan().schedules())
    }
}

/// The milliseconds since `began` on `host`'s monotonic clock; 0 on a
/// host without one.
fn since(host: &dyn Host, began: Option<u64>) -> u64 {
    match (began, host.monotonic_ms()) {
        (Some(began), Some(end)) => end.saturating_sub(began),
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use jadpo_std::http::{
        ApiPlan, Authentication, Codec, CodecId, RoutePlan, RouteTable, Run, Segment,
    };
    use jadpo_std::types::ValidatorId;
    use jadpo_std::{Builtin, HttpMethod, Idempotency, SuccessStatus};
    use jadpo_value::Value;
    use jadpo_value::json::Codecs;

    use super::*;
    use crate::http::{App, Entry, Routes};
    use crate::{Ctx, Database, JsonCodecs, NoStore, Outcome, Places, block_on};

    /// `GET /widgets`, answering the text its action gives.
    fn table() -> RouteTable {
        RouteTable {
            apis: vec![ApiPlan {
                name: "main".into(),
                prefix: Vec::new(),
                idempotency: Idempotency::Optional,
                methods: Vec::new(),
            }],
            routes: vec![RoutePlan {
                key: "route GET /widgets".into(),
                method: HttpMethod::Get,
                segments: vec![Segment::Fixed("widgets".into())],
                api: 0,
                input: None,
                output: Some(CodecId(0)),
                status: SuccessStatus::Ok,
                deadline_ms: 10_000,
                run: Run::Action(0),
                settle: None,
                stream: false,
                keys: None,
                args: Vec::new(),
                authentication: Authentication::Public,
                caller: None,
                file: "domains/widgets/routes/widgets.jadpo".into(),
                line: 1,
            }],
            ..RouteTable::default()
        }
    }

    const CODECS: [Codec; 1] = [Codec::Scalar {
        builtin: Builtin::Text,
        validator: ValidatorId(0),
        int_as_number: false,
    }];

    /// The route's typed entry, whose writer writes what the codec walk
    /// never would, so an answer shows which wrote it.
    fn written(_: &Codecs<'_>, _: &Value, out: &mut String) {
        out.push_str("\"by its entry\"");
    }

    static ENTRY: Entry = Entry {
        params: &[],
        input: None,
        output: Some(written),
    };

    fn entries(number: u32) -> Option<&'static Entry> {
        (number == 0).then_some(&ENTRY)
    }

    /// A program giving its routes typed entries, as compiled code does.
    struct Typed {
        places: Places,
        table: RouteTable,
        routes: Routes,
    }

    impl Executor for Typed {
        fn places(&self) -> &Places {
            &self.places
        }

        fn app(&self) -> App<'_> {
            App {
                table: &self.table,
                routes: &self.routes,
                codecs: Codecs {
                    codecs: &CODECS,
                    validators: &[],
                    shapes: &[],
                    coder: None,
                },
                failures: &[],
                json: JsonCodecs::NONE,
                entries: Some(entries),
            }
        }

        fn run<'a, 'p: 'a>(
            &'a self,
            _ctx: &'a mut Ctx<'p>,
            _callable: u32,
            _args: Vec<Value>,
        ) -> LocalFuture<'a, Outcome> {
            Box::pin(async { Outcome::Value(Value::Text("by the codec walk".into())) })
        }
    }

    /// A host with a clock and no store.
    struct Clocked(NoStore);

    impl Host for Clocked {
        fn database(&self, _store: &str) -> Option<&dyn Database> {
            Some(&self.0)
        }

        fn random(&self, _bytes: &mut [u8]) -> bool {
            false
        }

        fn now_ms(&self) -> Option<i64> {
            Some(1_792_486_800_000)
        }
    }

    /// An action that sends an outside call and calls an action of its
    /// own, as a route runs it: one span for each, in the order each
    /// started, timed on the host's monotonic clock.
    struct Calling {
        places: Places,
        table: RouteTable,
        routes: Routes,
    }

    /// Action `callable`'s body: action 1 inside action 0, then a call to
    /// `mailer.send`, which no driver fills.
    struct Nested(u32);

    impl<'p> crate::ActionBody<'p> for Nested {
        fn isolation(&self) -> jadpo_std::Isolation {
            jadpo_std::Isolation::ReadCommitted
        }

        fn callable(&self) -> Option<u32> {
            Some(self.0)
        }

        fn run<'c>(&'c mut self, ctx: &'c mut Ctx<'p>) -> LocalFuture<'c, Outcome> {
            Box::pin(async move {
                if self.0 == 0 {
                    let inner = crate::action(ctx, &mut Nested(1)).await;
                    if !matches!(inner, Outcome::Value(_)) {
                        return inner;
                    }
                    let call = crate::capability::Call::new(
                        "mailer",
                        "send",
                        Value::Unit,
                        jadpo_std::subscriber::Repeat::Safe,
                    );
                    let _ = crate::effects::call(ctx, call).await;
                }
                Outcome::Value(Value::Text("sent".into()))
            })
        }
    }

    impl Executor for Calling {
        fn places(&self) -> &Places {
            &self.places
        }

        fn app(&self) -> App<'_> {
            App {
                table: &self.table,
                routes: &self.routes,
                codecs: Codecs {
                    codecs: &CODECS,
                    validators: &[],
                    shapes: &[],
                    coder: None,
                },
                failures: &[],
                json: JsonCodecs::NONE,
                entries: None,
            }
        }

        fn run<'a, 'p: 'a>(
            &'a self,
            ctx: &'a mut Ctx<'p>,
            _callable: u32,
            _args: Vec<Value>,
        ) -> LocalFuture<'a, Outcome> {
            Box::pin(async move { crate::action(ctx, &mut Nested(0)).await })
        }
    }

    /// A host with a clock, a monotonic clock that moves a millisecond
    /// each time it is read, and no store.
    struct Timed(NoStore, std::cell::Cell<u64>);

    impl Host for Timed {
        fn database(&self, _store: &str) -> Option<&dyn Database> {
            Some(&self.0)
        }

        fn random(&self, _bytes: &mut [u8]) -> bool {
            false
        }

        fn now_ms(&self) -> Option<i64> {
            Some(1_792_486_800_000)
        }

        fn monotonic_ms(&self) -> Option<u64> {
            let now = self.1.get() + 1;
            self.1.set(now);
            Some(now)
        }
    }

    /// What it heard of each request.
    #[derive(Default)]
    struct Heard(std::cell::RefCell<Vec<crate::spans::Span>>);

    impl Observer for Heard {
        fn answered(&self, answered: &Answered<'_>) {
            self.0.borrow_mut().extend(answered.spans.iter().cloned());
        }
    }

    /// The observer hears what the request did as one list of spans
    /// (chunk 10.3c): each action and the outside call, in the order each
    /// started, each with its start and end on the host's monotonic clock
    /// and how it ended.
    #[test]
    fn the_observer_hears_each_span_a_request_made() {
        use crate::spans::{Ended, Kind};
        let table = table();
        let routes = Routes::new(&table, &[]);
        let program: Rc<dyn Executor> = Rc::new(Calling {
            places: Places::new(),
            table,
            routes,
        });
        let heard = Rc::new(Heard::default());
        let serving = Serving::new(
            Rc::new(Timed(NoStore, std::cell::Cell::new(0))),
            move || Rc::clone(&program),
        )
        .observed_by(Rc::clone(&heard) as Rc<dyn Observer>);
        let request = Request {
            method: "GET".into(),
            path: "/widgets".into(),
            ..Request::default()
        };
        let response = block_on(serving.handle(&request)).expect("nothing waits");
        assert_eq!(response.status, 200, "{response:?}");
        let spans = heard.0.borrow();
        let shown: Vec<(Kind, Option<u32>, Option<u64>, Ended)> = spans
            .iter()
            .map(|span| {
                (
                    span.kind.clone(),
                    span.callable,
                    span.rows,
                    span.ended.clone(),
                )
            })
            .collect();
        assert_eq!(
            shown,
            [
                (Kind::Action, Some(0), None, Ended::Value),
                (Kind::Action, Some(1), None, Ended::Value),
                (
                    Kind::Call {
                        connection: "mailer".into(),
                        operation: "send".into(),
                    },
                    None,
                    None,
                    Ended::Failed("misconfigured".into()),
                ),
            ],
            "{spans:#?}"
        );
        for span in spans.iter() {
            let (Some(start), Some(end)) = (span.start_ms, span.end_ms) else {
                panic!("an untimed span: {span:?}");
            };
            assert!(start < end, "{span:?}");
        }
        // The outer action holds the others.
        assert!(spans[0].start_ms < spans[1].start_ms && spans[2].end_ms < spans[0].end_ms);
        assert!(response.spans.is_empty(), "handed on to the observer");
    }

    /// What a host serves is what the program answers its routes with,
    /// compiled code's typed entries among it (chunk 8.4c), never the plan's
    /// route table alone.
    #[test]
    fn a_request_is_answered_through_the_programs_own_entries() {
        let table = table();
        let routes = Routes::new(&table, &[]);
        let program: Rc<dyn Executor> = Rc::new(Typed {
            places: Places::new(),
            table,
            routes,
        });
        let serving = Serving::new(Rc::new(Clocked(NoStore)), move || Rc::clone(&program));
        let request = Request {
            method: "GET".into(),
            path: "/widgets".into(),
            ..Request::default()
        };
        let response = block_on(serving.handle(&request)).expect("nothing waits");
        assert_eq!(response.status, 200, "{response:?}");
        assert_eq!(
            String::from_utf8_lossy(&response.body),
            "\"by its entry\"",
            "{response:?}"
        );
    }
}
