//! A deployable's shape on the native host (chunk 8.1e): a program's
//! runtime plan read from its canonical bytes, each worker's drivers built
//! from it by the factory, and its routes and ticks served through
//! `Serving` on `NativeHost`, with no callable compiled in yet. What
//! `tools/verify.py` builds without the runtime's `testing` feature, to
//! show a deployable builds without it and reaches none of the fakes,
//! recordings or crash points; and what a release build of it, with and
//! without the feature, measures their size by.
//!
//! `deployable <plan file> [<address>]`: serves the plan's routes at
//! `<address>` (`127.0.0.1:0` when none is given) until it is stopped.

use std::net::TcpListener;
use std::rc::Rc;
use std::sync::Arc;

use jadpo_host_native::http::{AppHandler, MakeHandler, WorkerThread, serve};
use jadpo_host_native::{Drivers, deployable_supplies};
use jadpo_runtime::http::address::Target;
use jadpo_runtime::{
    Ctx, Executor, FaultKind, Frame, Loaded, LocalFuture, Outcome, Places, Plan, RuntimePlan,
    Started,
};
use jadpo_value::Value;

/// A program with a plan and no callables: compiled code gives the
/// callables (roadmap 8.1f).
struct Deployable {
    places: Places,
    loaded: Loaded,
    started: Rc<Started>,
}

impl Executor for Deployable {
    fn places(&self) -> &Places {
        &self.places
    }

    fn plan(&self) -> Plan<'_> {
        self.loaded.plan()
    }

    fn started(&self) -> Option<Rc<Started>> {
        Some(Rc::clone(&self.started))
    }

    fn run<'a, 'p: 'a>(
        &'a self,
        ctx: &'a mut Ctx<'p>,
        _callable: u32,
        _args: Vec<Value>,
    ) -> LocalFuture<'a, Outcome> {
        Box::pin(async move {
            Outcome::Fault(ctx.fault(
                FaultKind::Defect("no callable is compiled in".into()),
                Frame::ENTRY,
            ))
        })
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let file = args.next().ok_or("deployable <plan file> [<address>]")?;
    let address = args.next().unwrap_or_else(|| "127.0.0.1:0".to_string());
    let plan = Arc::new(RuntimePlan::decode(&std::fs::read(file)?)?);
    let validators = Loaded::compile(&plan);
    let make: MakeHandler = Arc::new(move |worker: &WorkerThread| {
        let loaded = Loaded::sharing(Arc::clone(&plan), Arc::clone(&validators));
        let started = Rc::new(Started::new(&loaded.plan(), None, Target::Native));
        let drivers = Drivers::build(&plan.connections, &started, &deployable_supplies());
        let program: Rc<dyn Executor> = Rc::new(Deployable {
            places: Places::new(),
            loaded,
            started,
        });
        let host = worker.host().with_drivers(drivers);
        Box::new(AppHandler::new(host, move || Rc::clone(&program)))
    });
    let stop = jadpo_host_native::stop_signals()?;
    let server = serve(TcpListener::bind(address)?, 1, make)?;
    println!("serving {}", server.address());
    while !stop.load(std::sync::atomic::Ordering::SeqCst) {
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    server.stop();
    Ok(())
}
