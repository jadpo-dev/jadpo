//! Driving the machine (docs/compiler-runtime.md §2.5): a body runs on the
//! [`Machine`] until it ends or reaches a site that pauses, which is where
//! the waiting happens, here, on the runtime's terms. A call into an
//! action or fixture goes through the runtime's action boundary
//! ([`jadpo_runtime::action`]), which runs the callee's body (a fresh
//! machine) inside the caller's transaction or one of its own; the caller
//! then resumes with what the callee gave. A read or a write goes to the
//! runtime's one entry point ([`jadpo_runtime::store`]), which gives its
//! value, its failure or the `conflict:` arm a refusal maps to. The future
//! of a nested call is the only one boxed, so a body that never pauses
//! never allocates one, and functions, which never pause, run on the
//! machine without awaiting anything.

use jadpo_graph::Kind;
use jadpo_graph::body::SiteId;
use jadpo_runtime::calls::{self, Asked, Called};
use jadpo_runtime::{
    ActionBody, Ctx, Fault, FaultKind, Isolation, LocalFuture, Outcome, StoreOp, Stored,
};
use jadpo_value::Value;

use crate::machine::{Machine, Step, Stop, Target};
use crate::program::Program;

/// Runs callable `callee`'s body with `args`, called from site `at` of the
/// frame on top of `ctx`, waiting wherever it pauses.
async fn body(
    program: &Program,
    ctx: &mut Ctx<'_>,
    callee: u32,
    args: Vec<Value>,
    at: u32,
) -> Result<Value, Stop> {
    let mut machine = Machine::new(program);
    let mut step = machine.start(ctx, callee, args, SiteId(at));
    // A test, fixture or seed runs each call as the caller it passes
    // (area 9): policy then applies as to that caller.
    let plan = program.plan();
    let kind = plan.callable(callee).map(|callable| callable.kind);
    let test_like = matches!(kind, Some(Kind::Test | Kind::Fixture | Kind::Seed));
    loop {
        match step {
            Step::Done(result) => return result,
            // Work that never waits gives the thread back as it checks the
            // request's deadline.
            Step::Yield => {
                ctx.host().yield_now().await;
                step = machine.carry_on(ctx);
            }
            Step::Pause { target, args, at } => {
                if test_like {
                    // The parameter the callee's plan names as its caller.
                    let caller = match target {
                        Target::Callable(id) => plan.callable(id).map(|callable| callable.caller),
                        Target::Query(id) => plan.query(id).map(|query| query.caller),
                        Target::Get(_) => Some(None),
                        _ => None,
                    };
                    if let Some(caller) = caller {
                        let passed = caller.and_then(|at| args.get(at as usize));
                        jadpo_runtime::call_as(ctx, passed);
                    }
                }
                let result = match target {
                    Target::Callable(callee) => {
                        let caught = machine.catching();
                        // A package's action goes through the connection
                        // it was called through (chunk 7.6).
                        let through = machine.package_connection(at);
                        if let Some(connection) = through {
                            ctx.enter_package(connection);
                        }
                        let result = invoke(program, ctx, callee, args, at.0, caught).await;
                        if through.is_some() {
                            ctx.leave_package();
                        }
                        result
                    }
                    Target::Request {
                        route,
                        keyed,
                        signed,
                    } => request(program, ctx, route, &args, keyed, signed).await,
                    Target::Session(op) => session(program, ctx, op, &args, at.0).await,
                    Target::Batch(op, number) => batch(program, ctx, op, number, &args, at.0).await,
                    // A store's put or read, a provider's operation and a
                    // model's call, through the runtime's entry points,
                    // which compiled code calls too (chunk 8.1i).
                    Target::Files(op) => {
                        let connection = machine.files_connection(at).unwrap_or_default();
                        called(calls::files(ctx, connection, op, &args, at.0).await)
                    }
                    Target::Provider => {
                        // The operation by its number; a package action's
                        // own through the connection its action was called
                        // through (chunk 8.1d2).
                        let site = machine.provider_site(at).cloned().unwrap_or_default();
                        let input = args.into_iter().next();
                        let plan = program.runtime_plan();
                        called(calls::provider(ctx, plan, program, &site, input, at.0).await)
                    }
                    Target::Generate => {
                        let site = machine.generate_site(at).cloned();
                        ai(program, ctx, Asked::Generate(site.as_ref()), args, at.0).await
                    }
                    Target::Stream => {
                        let site = machine.stream_site(at).cloned();
                        ai(program, ctx, Asked::Stream(site.as_ref()), args, at.0).await
                    }
                    Target::Embed => {
                        let site = machine.embed_site(at).cloned();
                        ai(program, ctx, Asked::Embed(site.as_ref()), args, at.0).await
                    }
                    Target::Schedule(action) => {
                        let caught = machine.catching();
                        schedule(program, ctx, action, at.0, caught).await
                    }
                    Target::AdvanceClock => advance(ctx, &args, at.0),
                    effect => store(program, ctx, effect, args, at.0).await,
                };
                // What a test's step emitted is delivered before its next
                // statement, and what the clock brought due once it moved
                // (area 13, "Caches and events"; chunk 6.11). A delivery
                // that failed for good ends the test there.
                if kind == Some(Kind::Test) && !matches!(result, Err(Stop::Fault(_))) {
                    let advanced = target == Target::AdvanceClock;
                    if let Err(ended) =
                        jadpo_runtime::test_steps::after_step(program, ctx, advanced, at.0).await
                    {
                        return self::result(ended);
                    }
                }
                step = machine.resume(ctx, result);
            }
        }
    }
}

/// Calls callable `callee` with `args` from site `at`: an action or a
/// fixture through the runtime's action boundary, anything else as it is.
/// `caught` says the caller handles its failure.
pub(crate) fn invoke<'a, 'p: 'a>(
    program: &'a Program,
    ctx: &'a mut Ctx<'p>,
    callee: u32,
    args: Vec<Value>,
    at: u32,
    caught: bool,
) -> LocalFuture<'a, Result<Value, Stop>> {
    Box::pin(async move {
        let is_action = program
            .plan()
            .callable(callee)
            .is_some_and(|callable| callable.is_action());
        if !is_action {
            return body(program, ctx, callee, args, at).await;
        }
        let mut action = Action {
            program,
            callee,
            args,
            at,
            caught,
        };
        result(jadpo_runtime::action(ctx, &mut action).await)
    })
}

/// A test's run of a schedule (area 13, "Kinds of test"; chunk 6.2c):
/// its action, `action`, called with nothing as the system with no tenant,
/// as the tick runs it, at the test's clock, which stays fixed through
/// the run; what it emits is left as any step's is. It gives nothing, and
/// fails as its action fails.
async fn schedule(
    program: &Program,
    ctx: &mut Ctx<'_>,
    action: u32,
    at: u32,
    caught: bool,
) -> Result<Value, Stop> {
    let saved = jadpo_runtime::schedules::test_run(ctx);
    let result = invoke(program, ctx, action, Vec::new(), at, caught).await;
    jadpo_runtime::schedules::test_run_ended(ctx, saved);
    result.map(|_| Value::Unit)
}

/// `clock.advance(by)` in a test (area 13, "Controlling time"), which
/// the runtime moves the test's clock for.
fn advance(ctx: &mut Ctx<'_>, args: &[Value], at: u32) -> Result<Value, Stop> {
    jadpo_runtime::test_steps::advance_clock(ctx, args, at)
        .map(|()| Value::Unit)
        .map_err(Stop::Fault)
}

/// A read or a write (area 6), which the runtime runs from its plan,
/// inside the running action's transaction or a boundary of its own: what
/// is left here is finding the plan and dispatching a refusal to the
/// `conflict:` arm its site maps it to.
fn store<'a, 'p: 'a>(
    program: &'a Program,
    ctx: &'a mut Ctx<'p>,
    target: Target,
    args: Vec<Value>,
    at: u32,
) -> LocalFuture<'a, Result<Value, Stop>> {
    Box::pin(async move {
        let plan = program.plan();
        let op = match target {
            Target::Query(id) => plan.query(id).map(|query| StoreOp::Read(&query.read)),
            Target::Get(id) => plan.entity(id).map(|entity| StoreOp::Read(&entity.get)),
            Target::Write(id) => plan.write(id).map(|write| StoreOp::Write(&write.plan)),
            Target::Callable(_)
            | Target::Request { .. }
            | Target::Session(_)
            | Target::Batch(..)
            | Target::Files(..)
            | Target::Provider
            | Target::Generate
            | Target::Stream
            | Target::Embed
            | Target::Schedule(_)
            | Target::AdvanceClock => None,
        };
        let Some(op) = op else {
            let what = match target {
                Target::Query(_) => "a query with no plan",
                Target::Get(_) => "an entity with no plan",
                Target::Write(_) => "a write with no plan",
                Target::Callable(_)
                | Target::Request { .. }
                | Target::Session(_)
                | Target::Batch(..)
                | Target::Files(..)
                | Target::Provider
                | Target::Generate
                | Target::Stream
                | Target::Embed
                | Target::Schedule(_)
                | Target::AdvanceClock => "a call run as a read",
            };
            return Err(Stop::Fault(
                ctx.fault(FaultKind::Defect(what.to_string()), at),
            ));
        };
        let failures = plan.failures();
        stopped(jadpo_runtime::store(ctx, op, failures, program.stored_json(), &args, at).await)
    })
}

/// A route test's request (area 13), through the runtime's HTTP component
/// as a client's would go, answered as a `Response` value
/// ([`jadpo_runtime::http::route_test`]). The site's values are the
/// route's arguments, then, when `keyed`, the `Idempotency-Key` the test
/// gives, then, when `signed`, the caller it signs in as.
async fn request(
    program: &Program,
    ctx: &mut Ctx<'_>,
    route: u32,
    args: &[Value],
    keyed: bool,
    signed: bool,
) -> Result<Value, Stop> {
    let (args, caller) = match (signed, args.split_last()) {
        (true, Some((caller, rest))) => (rest, Some(caller)),
        _ => (args, None),
    };
    let (args, key) = match (keyed, args.split_last()) {
        (true, Some((key, rest))) => (rest, Some(key)),
        _ => (args, None),
    };
    jadpo_runtime::http::route_test(&program.app(), program, ctx, route, args, key, caller)
        .await
        .map_err(Stop::Fault)
}

/// `sessions.start(user_id)`, `sessions.end()`, an API token's or a
/// service key's operation, or magic links', at site `at` (area 8),
/// through the runtime's one entry point for them, which compiled code
/// calls too.
async fn session(
    program: &Program,
    ctx: &mut Ctx<'_>,
    op: jadpo_std::http::SessionOp,
    args: &[Value],
    at: u32,
) -> Result<Value, Stop> {
    stopped(jadpo_runtime::http::session(program, ctx, op, args, at).await)
}

/// `batches.each`, `batches.status` or `batches.cancel` at site `at` (area
/// 12, chunk 6.4), through the runtime's one entry point for them, which
/// compiled code calls too: the run kept, read or asked to stop, in the
/// running action's transaction.
async fn batch(
    program: &Program,
    ctx: &mut Ctx<'_>,
    op: jadpo_std::batch::BatchOp,
    number: Option<u32>,
    args: &[Value],
    at: u32,
) -> Result<Value, Stop> {
    let plan = number.and_then(|number| program.plan().batch(number));
    let batches = program.runtime_plan().batches();
    stopped(jadpo_runtime::batches::call(ctx, batches, op, plan, args, at).await)
}

/// An `ai` call at site `at` (area 21), with its one argument, through the
/// runtime's entry point for them.
async fn ai(
    program: &Program,
    ctx: &mut Ctx<'_>,
    asked: Asked<'_>,
    args: Vec<Value>,
    at: u32,
) -> Result<Value, Stop> {
    let given = args.into_iter().next().unwrap_or(Value::Unit);
    called(calls::ai(ctx, program.runtime_plan(), asked, given, at).await)
}

/// What the runtime's entry point gave, as what stops the walk.
fn stopped(stored: Stored) -> Result<Value, Stop> {
    match stored {
        Stored::Value(value) => Ok(value),
        Stored::Failed(failure) => Err(Stop::Failed(failure)),
        Stored::Conflict { arm, cause } => Err(Stop::Refused { arm, cause }),
        Stored::Fault(fault) => Err(Stop::Fault(fault)),
    }
}

/// What one of the runtime's entry points for a body's sites of areas
/// 10, 11, 16 and 21 gave ([`jadpo_runtime::calls`], which compiled code
/// calls too), as what stops the walk.
pub(crate) fn called(called: Called) -> Result<Value, Stop> {
    match called {
        Called::Value(value) => Ok(value),
        Called::Broke {
            rule,
            written,
            problem,
        } => Err(Stop::Invalid {
            rule,
            written,
            problem,
            name: None,
        }),
        Called::Failed(failure) => Err(Stop::Failed(failure)),
        Called::Fault(fault) => Err(Stop::Fault(fault)),
    }
}

/// A provider package's own function `callee`, a field's decoder or
/// encoder or an operation's whole-answer decoder (area 11, "Decoding";
/// chunk 7.2c2), run on `args` from site `at` of the frame on top of
/// `ctx`. It is pure, so it never pauses: what it gives, or how it ended
/// without a value, its failure's code or its fault's classification.
pub(crate) fn package_function(
    program: &Program,
    ctx: &mut Ctx<'_>,
    callee: u32,
    args: Vec<Value>,
    at: u32,
) -> Result<Value, String> {
    let mut machine = Machine::new(program);
    let mut step = machine.start(ctx, callee, args, SiteId(at));
    loop {
        match step {
            Step::Done(Ok(value)) => return Ok(value),
            Step::Done(Err(Stop::Failed(failure))) => return Err(format!("`{}`", failure.code)),
            Step::Done(Err(Stop::Fault(fault))) => {
                return Err(format!("a fault, `{}`", fault.kind.classification()));
            }
            Step::Done(Err(_)) => return Err("a fault, `defect`".to_string()),
            Step::Yield => step = machine.carry_on(ctx),
            Step::Pause { .. } => return Err("a fault, `defect`".to_string()),
        }
    }
}

/// A file helper's write's row for `item` (chunk 6.4b2), as
/// [`jadpo_runtime::Executor::row`] runs it: `row`'s inline function, its
/// parameter given `item` and each local it reads its saved value.
pub(crate) fn row(
    program: &Program,
    ctx: &mut Ctx<'_>,
    row: &jadpo_std::batch::BatchRow,
    item: Value,
    saved: &[Value],
) -> Outcome {
    let locals = jadpo_runtime::batches::row_locals(row, item, saved);
    let mut machine = Machine::new(program);
    outcome(machine.row(ctx, row.callable, SiteId(row.body), locals))
}

/// An action's body as the runtime's boundary runs it, once or, after a
/// concurrent change, again from the start.
struct Action<'a> {
    program: &'a Program,
    callee: u32,
    args: Vec<Value>,
    at: u32,
    caught: bool,
}

impl<'p> ActionBody<'p> for Action<'_> {
    /// The level its plan decided from the graph (area 6).
    fn isolation(&self) -> Isolation {
        self.program
            .plan()
            .callable(self.callee)
            .map_or(Isolation::RepeatableRead, |callable| callable.isolation)
    }

    fn caught(&self) -> bool {
        self.caught
    }

    /// Who may call it, as its plan says; the boundary checks it.
    fn gate(&self) -> Option<&jadpo_std::policy::Gate> {
        self.program
            .plan()
            .callable(self.callee)
            .and_then(|callable| callable.gate.as_ref())
    }

    fn site(&self) -> u32 {
        self.at
    }

    /// The store its plan keeps its events in (area 12, chunk 6.1b).
    fn outbox(&self) -> Option<&str> {
        self.program
            .plan()
            .callable(self.callee)
            .and_then(|callable| callable.outbox.as_deref())
    }

    /// What its plan keeps of its steps when it makes several changing
    /// calls, and the key their answers are sealed with (chunk 7.5b).
    fn record(&self) -> Option<jadpo_runtime::steps::Spec<'_>> {
        let plan = self.program.plan();
        let callable = plan.callable(self.callee)?;
        Some(jadpo_runtime::steps::Spec {
            plan: callable.record.as_ref()?,
            action: callable.key.as_str(),
            build: plan.build_id(),
            setting: plan.steps_key(),
        })
    }

    fn run<'c>(&'c mut self, ctx: &'c mut Ctx<'p>) -> LocalFuture<'c, Outcome> {
        Box::pin(async move {
            outcome(body(self.program, ctx, self.callee, self.args.clone(), self.at).await)
        })
    }
}

/// How a call ended, as the runtime reports it. A `return` or a broken
/// rule never leaves a body: its end turns them into its value or a
/// defect.
pub(crate) fn outcome(result: Result<Value, Stop>) -> Outcome {
    match result {
        Ok(value) | Err(Stop::Return(value)) => Outcome::Value(value),
        Err(Stop::Failed(failure)) => Outcome::Failed(failure),
        Err(Stop::Fault(fault)) => Outcome::Fault(fault),
        Err(Stop::Assertion(assertion)) => Outcome::Assertion(assertion),
        Err(Stop::Invalid { .. }) => Outcome::Fault(Box::new(Fault {
            kind: FaultKind::Defect("a construction failed outside an `attempt`".into()),
            frames: Vec::new(),
        })),
        Err(Stop::Refused { .. }) => Outcome::Fault(Box::new(Fault {
            kind: FaultKind::Defect("a refused write outside its own site".into()),
            frames: Vec::new(),
        })),
    }
}

/// An outcome as what stops the caller's walk.
fn result(outcome: Outcome) -> Result<Value, Stop> {
    match outcome {
        Outcome::Value(value) => Ok(value),
        Outcome::Failed(failure) => Err(Stop::Failed(failure)),
        Outcome::Fault(fault) => Err(Stop::Fault(fault)),
        Outcome::Assertion(assertion) => Err(Stop::Assertion(assertion)),
    }
}
