//! Driving the machine (docs/compiler-runtime.md §2.5): a body runs on the
//! [`Machine`] until it ends or reaches a site that pauses, which is where
//! the waiting happens, here, on the runtime's terms. A call into an
//! action or fixture goes through the runtime's action boundary
//! ([`jadpo_runtime::action`]), which runs the callee's body (a fresh
//! machine) inside the caller's transaction or one of its own; the caller
//! then resumes with what the callee gave. A read or a write goes to the
//! runtime's one entry point ([`jadpo_runtime::store`]), which gives its
//! value, its failure or the `conflict:` arm a refusal maps to. The future of a nested call is
//! the only one boxed, so a body that never pauses never allocates one, and
//! functions, which never pause, run on the machine without awaiting
//! anything.

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
    loop {
        match step {
            Step::Done(result) => return result,
            Step::Pause { target, args, at } => {
                let result = match target {
                    Target::Callable(callee) => {
                        let caught = machine.catching();
                        invoke(program, ctx, callee, args, at.0, caught).await
                    }
                    effect => store(program, ctx, effect, args, at.0).await,
                };
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
            Target::Callable(_) => None,
        };
        let Some(op) = op else {
            let what = match target {
                Target::Query(_) => "a query with no plan",
                Target::Get(_) => "an entity with no plan",
                Target::Write(_) => "a write with no plan",
                Target::Callable(_) => "a call run as a read",
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
