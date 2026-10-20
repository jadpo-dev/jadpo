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
                    Target::Files(op) => {
                        let connection = machine.files_connection(at).unwrap_or_default();
                        files(ctx, connection, op, &args, at.0).await
                    }
                    Target::Provider => {
                        let mut called = machine.provider_call(at).unwrap_or_default();
                        // A package action's own operation goes through the
                        // connection its action was called through.
                        let own: String;
                        if called.0 == jadpo_graph::body::OWN_CONNECTION {
                            own = ctx.package_connection().unwrap_or_default().to_string();
                            called.0 = &own;
                        }
                        let site = machine.provider_site(at).cloned().unwrap_or_default();
                        provider(program, ctx, called, &site, args, at.0).await
                    }
                    Target::Generate => {
                        let site = machine.generate_site(at).cloned();
                        generate(program, ctx, site.as_ref(), args, at.0).await
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
                | Target::Schedule(_)
                | Target::AdvanceClock => "a call run as a read",
            };
            return Err(Stop::Fault(
                ctx.fault(FaultKind::Defect(what.to_string()), at),
            ));
        };
        let failures = plan.types().failures();
        match jadpo_runtime::store(ctx, op, failures, program.stored_json(), &args, at).await {
            Stored::Value(value) => Ok(value),
            Stored::Failed(failure) => Err(Stop::Failed(failure)),
            Stored::Conflict { arm, cause } => Err(Stop::Refused { arm, cause }),
            Stored::Fault(fault) => Err(Stop::Fault(fault)),
        }
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
/// service key's operation, or magic links', at site `at` (area 8): the
/// runtime keeps, lists or deletes it in the running action's transaction.
async fn session(
    program: &Program,
    ctx: &mut Ctx<'_>,
    op: jadpo_std::http::SessionOp,
    args: &[Value],
    at: u32,
) -> Result<Value, Stop> {
    use jadpo_runtime::http::{identity, magic_links};
    use jadpo_std::http::SessionOp;
    let plan = program.plan().routes().identity.as_ref();
    let stored = match (op, args) {
        // Magic links' operations, for the application's own routes (area
        // 8, "Writing the routes yourself"; chunk 7.3b3), through the code
        // the generated routes use.
        (SessionOp::MagicLinkRequest, _) => {
            magic_links::request(program, ctx, plan, args, at).await
        }
        (SessionOp::MagicLinkRedeem, _) => magic_links::redeem(program, ctx, plan, args, at).await,
        (SessionOp::MagicLinkRedeemCode, _) => {
            magic_links::redeem_code(program, ctx, plan, args, at).await
        }
        // `test_passkeys`, which only a route test calls (chunk 5.5z2),
        // through the generated passkey routes.
        (SessionOp::TestPasskeyRegister, _) => {
            jadpo_runtime::http::test_passkeys::register(&program.app(), program, ctx, args, at)
                .await
        }
        (SessionOp::TestPasskeySignIn, _) => {
            jadpo_runtime::http::test_passkeys::sign_in(&program.app(), program, ctx, args, at)
                .await
        }
        (SessionOp::Start, [user]) => identity::start(ctx, plan, user, at).await,
        (SessionOp::End, []) => identity::end(ctx, plan, at).await,
        (SessionOp::IssueToken, [user]) => identity::issue_token(ctx, plan, user, at).await,
        (SessionOp::IssueKey, [service, expires_at]) => {
            identity::issue_key(ctx, plan, service, expires_at, at).await
        }
        (SessionOp::Tokens | SessionOp::Keys, [holder]) => {
            identity::issued(ctx, plan, op.on_service(), holder, at).await
        }
        (SessionOp::RevokeToken | SessionOp::RevokeKey, [id]) => {
            identity::revoke(ctx, plan, op.on_service(), id, at).await
        }
        _ => {
            return Err(Stop::Fault(ctx.fault(
                FaultKind::Defect(format!("`{}` called with the wrong arguments", op.name())),
                at,
            )));
        }
    };
    match stored {
        Stored::Value(value) => Ok(value),
        Stored::Failed(failure) => Err(Stop::Failed(failure)),
        Stored::Conflict { cause, .. } => Err(Stop::Failed(cause)),
        Stored::Fault(fault) => Err(Stop::Fault(fault)),
    }
}

/// `batches.each`, `batches.status` or `batches.cancel` at site `at` (area
/// 12, chunk 6.4): the runtime keeps the run, reads it or asks it to stop,
/// in the running action's transaction.
async fn batch(
    program: &Program,
    ctx: &mut Ctx<'_>,
    op: jadpo_std::batch::BatchOp,
    number: Option<u32>,
    args: &[Value],
    at: u32,
) -> Result<Value, Stop> {
    use jadpo_runtime::batches;
    use jadpo_std::batch::BatchOp;
    let Some(program_batches) = jadpo_runtime::Executor::batches(program) else {
        return Err(Stop::Fault(ctx.fault(
            FaultKind::Defect("a batch in a program that runs none".into()),
            at,
        )));
    };
    let stored = match (op, number, args) {
        (BatchOp::Each | BatchOp::Read(_) | BatchOp::Write(_), Some(number), _) => {
            match program.plan().batch(number) {
                Some(plan) => batches::start(ctx, program_batches, plan, args, at).await,
                None => {
                    return Err(Stop::Fault(
                        ctx.fault(FaultKind::Defect("a batch with no plan".into()), at),
                    ));
                }
            }
        }
        (BatchOp::Status, _, [run]) => batches::status(ctx, program_batches, run, at).await,
        (BatchOp::Cancel, _, [run]) => batches::cancel(ctx, program_batches, run, at).await,
        _ => {
            return Err(Stop::Fault(ctx.fault(
                FaultKind::Defect(format!(
                    "`batches.{}` called with the wrong arguments",
                    op.name()
                )),
                at,
            )));
        }
    };
    match stored {
        Stored::Value(value) => Ok(value),
        Stored::Failed(failure) => Err(Stop::Failed(failure)),
        Stored::Conflict { cause, .. } => Err(Stop::Failed(cause)),
        Stored::Fault(fault) => Err(Stop::Fault(fault)),
    }
}

/// `store.put(bytes, type)` or `store.read(file)` at site `at` (area 16,
/// chunk 6.4b), through the runtime's `files` capability: an outside call,
/// which the seam refuses once the action's transaction is open.
async fn files(
    ctx: &mut Ctx<'_>,
    connection: &str,
    op: jadpo_std::files::FilesOp,
    args: &[Value],
    at: u32,
) -> Result<Value, Stop> {
    use jadpo_runtime::files::{self, Done};
    use jadpo_std::files::FilesOp;
    let done = match (op, args) {
        (FilesOp::Put, [bytes, file_type]) => files::put(ctx, connection, bytes, file_type).await,
        (FilesOp::Read, [file]) => files::read_whole(ctx, file).await,
        _ => Done::Fault(FaultKind::Defect(format!(
            "`{connection}.{}` called with the wrong arguments",
            op.name()
        ))),
    };
    match done {
        Done::Value(value) => Ok(value),
        Done::Broke(rule) => Err(Stop::Invalid {
            rule,
            written: rule.word().to_string(),
            problem: None,
            name: None,
        }),
        Done::Failed(failure) => Err(Stop::Failed(failure)),
        Done::Fault(kind) => Err(Stop::Fault(ctx.fault(kind, at))),
    }
}

/// `connection.resource.operation(input)` at site `at` (area 11, chunk
/// 7.2c): the runtime makes the request from the plan, sends it through
/// the capability seam and decodes the answer; a failure the operation
/// declares is the call's, for its `attempt` to map.
async fn provider(
    program: &Program,
    ctx: &mut Ctx<'_>,
    (connection, operation): (&str, &str),
    call: &jadpo_std::provider::CallSite,
    args: Vec<Value>,
    at: u32,
) -> Result<Value, Stop> {
    use jadpo_runtime::provider::{self, Done};
    let providers = program.providers();
    // The settings its address is read from: a test's own value, or the
    // application's.
    let settings: Vec<(u32, Value)> = providers
        .plan
        .operation(connection, operation)
        .map(|(plan, _)| provider::settings_read(plan))
        .unwrap_or_default()
        .into_iter()
        .filter_map(|number| {
            let value = ctx
                .setting(number)
                .cloned()
                .or_else(|| program.setting(number))?;
            Some((number, value))
        })
        .collect();
    let input = args.into_iter().next();
    // The package's own decoders and encoders run here, at the call's
    // site (chunk 7.2c2).
    let site = provider::Site {
        call,
        functions: program,
        at,
    };
    match provider::call(
        ctx,
        &providers,
        (connection, operation),
        input,
        &settings,
        site,
    )
    .await
    {
        Done::Value(value) => Ok(value),
        Done::Failed(failure) => Err(Stop::Failed(failure)),
        Done::Fault(kind) => Err(Stop::Fault(ctx.fault(kind, at))),
    }
}

/// `ai.generate(prompt, as: T)` at site `at` (area 21, chunk 5.5v3): the
/// runtime renders the prompt, asks the model through the connection
/// `ai:` names and decodes its answer; a condition it ends in is the
/// call's, for its `attempt` to map.
async fn generate(
    program: &Program,
    ctx: &mut Ctx<'_>,
    site: Option<&jadpo_std::ai::GenerateSite>,
    args: Vec<Value>,
    at: u32,
) -> Result<Value, Stop> {
    use jadpo_runtime::ai::{self, Done};
    let (Some(site), Some(ai)) = (site, program.ai()) else {
        return Err(Stop::Fault(ctx.fault(
            FaultKind::Defect("a model asked in a program that names none".into()),
            at,
        )));
    };
    // The settings its model and address are read from: a test's own
    // value, or the application's.
    let settings: Vec<(u32, Value)> = ai::settings_read(ai.plan, site)
        .into_iter()
        .filter_map(|number| {
            let value = ctx
                .setting(number)
                .cloned()
                .or_else(|| program.setting(number))?;
            Some((number, value))
        })
        .collect();
    let prompt = args.into_iter().next().unwrap_or(Value::Unit);
    match ai::generate(ctx, &ai, site, prompt, &settings).await {
        Done::Value(value) => Ok(value),
        Done::Broke { rule, problem } => Err(Stop::Invalid {
            rule,
            written: rule.word().to_string(),
            problem,
            name: None,
        }),
        Done::Failed(failure) => Err(Stop::Failed(failure)),
        Done::Fault(kind) => Err(Stop::Fault(ctx.fault(kind, at))),
    }
}

/// A provider package's own function `function`, a field's decoder or
/// encoder or an operation's whole-answer decoder (area 11, "Decoding";
/// chunk 7.2c2), run on `args` from site `at` of the frame on top of
/// `ctx`. It is pure, so it never pauses: what it gives, or how it ended
/// without a value, its failure's code or its fault's classification.
pub(crate) fn package_function(
    program: &Program,
    ctx: &mut Ctx<'_>,
    function: &str,
    args: Vec<Value>,
    at: u32,
) -> Result<Value, String> {
    let key = jadpo_graph::Key::from_text(function.to_string());
    let Some(callee) = program.plan().id(&key) else {
        return Err(format!("`{function}`, which the plan doesn't have"));
    };
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
    let mut locals = vec![(row.item, item)];
    for (local, at) in &row.locals {
        locals.push((
            *local,
            saved.get(*at as usize).cloned().unwrap_or(Value::None),
        ));
    }
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
            keys: self.program.effect_keys(),
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
