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
    let test_like = program
        .plan()
        .callable(callee)
        .is_some_and(|callable| matches!(callable.kind, Kind::Test | Kind::Fixture | Kind::Seed));
    loop {
        match step {
            Step::Done(result) => return result,
            Step::Pause { target, args, at } => {
                if test_like
                    && matches!(
                        target,
                        Target::Callable(_) | Target::Query(_) | Target::Get(_)
                    )
                {
                    calling_as(ctx, &args);
                }
                let result = match target {
                    Target::Callable(callee) => {
                        let caught = machine.catching();
                        invoke(program, ctx, callee, args, at.0, caught).await
                    }
                    Target::Request {
                        route,
                        keyed,
                        signed,
                    } => request(program, ctx, route, &args, keyed, signed).await,
                    Target::Session(op) => session(program, ctx, op, &args, at.0).await,
                    effect => store(program, ctx, effect, args, at.0).await,
                };
                step = machine.resume(ctx, result);
            }
        }
    }
}

/// Runs what a test, fixture or seed calls with `args` as the caller they
/// pass, a `Principal.user` or `Principal.service`, so policy applies as
/// to that caller; with none, without policy, so the call proves behaviour
/// and data and a route test proves policy (areas 9 and 13).
fn calling_as(ctx: &mut Ctx<'_>, args: &[Value]) {
    let caller = args.iter().find_map(|arg| match arg {
        Value::Record(record) if record.ty.starts_with("Principal.") => {
            let kind = record.ty.trim_start_matches("Principal.");
            let id = match record.get("id") {
                Some(Value::Uuid(id)) => id.to_string(),
                _ => String::new(),
            };
            Some(jadpo_runtime::Caller {
                value: arg.clone(),
                key: format!("{kind} {id}"),
                session: None,
            })
        }
        _ => None,
    });
    match caller {
        Some(caller) => ctx.set_caller(Some(caller)),
        None => ctx.set_unpoliced(),
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
            Target::Callable(_) | Target::Request { .. } | Target::Session(_) => None,
        };
        let Some(op) = op else {
            let what = match target {
                Target::Query(_) => "a query with no plan",
                Target::Get(_) => "an entity with no plan",
                Target::Write(_) => "a write with no plan",
                Target::Callable(_) | Target::Request { .. } | Target::Session(_) => {
                    "a call run as a read"
                }
            };
            return Err(Stop::Fault(
                ctx.fault(FaultKind::Defect(what.to_string()), at),
            ));
        };
        let failures = plan.types().failures();
        match jadpo_runtime::store(ctx, op, failures, &args, at).await {
            Stored::Value(value) => Ok(value),
            Stored::Failed(failure) => Err(Stop::Failed(failure)),
            Stored::Conflict { arm, cause } => Err(Stop::Refused { arm, cause }),
            Stored::Fault(fault) => Err(Stop::Fault(fault)),
        }
    })
}

/// A route test's request (area 13), through the runtime's HTTP component
/// as a client's would go, answered as a `Response` value.
/// A defect behind the response ends the test with that fault, as one
/// anywhere else in it would: a 500 is never what a test means to check.
/// Every request but a `GET` sends an `Idempotency-Key`, as a real client
/// would: the one the test gives, or a fresh one (area 13, chunk 4.7).
async fn request(
    program: &Program,
    ctx: &mut Ctx<'_>,
    route: u32,
    args: &[Value],
    keyed: bool,
    signed: bool,
) -> Result<Value, Stop> {
    let app = program.app();
    let (args, caller) = match (signed, args.split_last()) {
        (true, Some((caller, rest))) => (rest, Some(caller)),
        _ => (args, None),
    };
    let (args, key) = match (keyed, args.split_last()) {
        (true, Some((key, rest))) => (rest, Some(key)),
        _ => (args, None),
    };
    let Some(mut request) = jadpo_runtime::http::test_request(&app, route, args) else {
        return Ok(Value::None);
    };
    let writes = app
        .table
        .routes
        .get(route as usize)
        .is_some_and(|plan| plan.method != jadpo_std::HttpMethod::Get);
    if writes
        && request
            .header(jadpo_runtime::http::idempotency::HEADER)
            .is_none()
    {
        let key = match key {
            Some(Value::Text(text)) => Some(text.to_string()),
            Some(_) => None,
            None => ctx.new_id().map(|id| jadpo_runtime::uuid_text(&id)),
        };
        if let Some(key) = key {
            request.headers.push(("Idempotency-Key".to_string(), key));
        }
    }
    if let Some(caller) = caller {
        // `as:` signs in through the application's real authentication.
        let headers =
            jadpo_runtime::http::identity::test_credentials(ctx, app.table, route, caller)
                .await
                .map_err(Stop::Fault)?;
        request.headers.extend(headers);
    }
    let id = jadpo_runtime::http::request_id(ctx, u64::from(ctx.actions()));
    let mut response = jadpo_runtime::http::handle(&app, program, ctx, &request, &id, None).await;
    if let Some(fault) = response.fault.take() {
        return Err(Stop::Fault(fault));
    }
    Ok(jadpo_runtime::http::response_value(&app, route, &response))
}

/// `sessions.start(user_id)`, `sessions.end()`, an API token's or a
/// service key's operation at site `at` (area 8): the runtime keeps,
/// lists or deletes it in the running action's transaction.
async fn session(
    program: &Program,
    ctx: &mut Ctx<'_>,
    op: jadpo_std::http::SessionOp,
    args: &[Value],
    at: u32,
) -> Result<Value, Stop> {
    use jadpo_runtime::http::identity;
    use jadpo_std::http::SessionOp;
    let plan = program.plan().routes().identity.as_ref();
    let stored = match (op, args) {
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

    fn run<'c>(&'c mut self, ctx: &'c mut Ctx<'p>) -> LocalFuture<'c, Outcome> {
        Box::pin(async move {
            // An action whose line under `actions:` needs no record says
            // who may call it as it starts (area 9).
            if let Some(gate) = self
                .program
                .plan()
                .callable(self.callee)
                .and_then(|callable| callable.gate.as_ref())
            {
                match jadpo_runtime::admit(ctx, gate, self.at).await {
                    Stored::Value(_) => {}
                    Stored::Failed(failure) | Stored::Conflict { cause: failure, .. } => {
                        return Outcome::Failed(failure);
                    }
                    Stored::Fault(fault) => return Outcome::Fault(fault),
                }
            }
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
