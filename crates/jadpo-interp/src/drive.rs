//! Driving the machine (docs/compiler-runtime.md §2.5): a body runs on the
//! [`Machine`] until it ends or reaches a site that pauses, which is where
//! the waiting happens, here, on the runtime's terms. A call into an
//! action or fixture goes through the runtime's action boundary
//! ([`jadpo_runtime::action`]), which runs the callee's body (a fresh
//! machine) inside the caller's transaction or one of its own; the caller
//! then resumes with what the callee gave. The future of a nested call is
//! the only one boxed, so a body that never pauses never allocates one, and
//! functions, which never pause, run on the machine without awaiting
//! anything.

use jadpo_graph::body::SiteId;
use jadpo_plan::{MissingPlan, MissingValue, Refusal};
use jadpo_runtime::{
    ActionBody, Ctx, Fault, FaultKind, Found, Interrupt, LocalFuture, Outcome, Written,
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
                    Target::Callable(callee) => invoke(program, ctx, callee, args, at.0).await,
                    effect => store(program, ctx, effect, args, at.0).await,
                };
                step = machine.resume(ctx, result);
            }
        }
    }
}

/// Calls callable `callee` with `args` from site `at`: an action or a
/// fixture through the runtime's action boundary, anything else as it is.
pub(crate) fn invoke<'a, 'p: 'a>(
    program: &'a Program,
    ctx: &'a mut Ctx<'p>,
    callee: u32,
    args: Vec<Value>,
    at: u32,
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
        };
        result(jadpo_runtime::action(ctx, &mut action).await)
    })
}

/// A read or a write (area 6): inside the running action's transaction,
/// or, called outside any action (by a test, or a route's display read),
/// in a boundary of its own, as a one-statement action.
fn store<'a, 'p: 'a>(
    program: &'a Program,
    ctx: &'a mut Ctx<'p>,
    target: Target,
    args: Vec<Value>,
    at: u32,
) -> LocalFuture<'a, Result<Value, Stop>> {
    Box::pin(async move {
        if ctx.in_action() {
            return effect(program, ctx, target, &args, at).await;
        }
        let mut body = Effect {
            program,
            target,
            args,
            at,
        };
        result(jadpo_runtime::action(ctx, &mut body).await)
    })
}

/// A read or write as the boundary runs it.
struct Effect<'a> {
    program: &'a Program,
    target: Target,
    args: Vec<Value>,
    at: u32,
}

impl<'p> ActionBody<'p> for Effect<'_> {
    fn run<'c>(&'c mut self, ctx: &'c mut Ctx<'p>) -> LocalFuture<'c, Outcome> {
        Box::pin(async move {
            outcome(effect(self.program, ctx, self.target, &self.args, self.at).await)
        })
    }
}

/// Runs a read or a write from its plan.
async fn effect(
    program: &Program,
    ctx: &mut Ctx<'_>,
    target: Target,
    args: &[Value],
    at: u32,
) -> Result<Value, Stop> {
    let plan = program.plan();
    let defect = |ctx: &mut Ctx<'_>, what: &str| {
        Stop::Fault(ctx.fault(FaultKind::Defect(what.to_string()), at))
    };
    let (read, missing) = match target {
        Target::Query(id) => match plan.query(id) {
            Some(query) => (&query.read, query.missing.as_ref()),
            None => return Err(defect(ctx, "a query with no plan")),
        },
        Target::Get(id) => match plan.entity(id) {
            Some(entity) => (&entity.get, entity.not_found.as_ref()),
            None => return Err(defect(ctx, "an entity with no plan")),
        },
        Target::Write(id) => {
            let Some(write) = plan.write(id) else {
                return Err(defect(ctx, "a write with no plan"));
            };
            return match jadpo_runtime::write(ctx, &write.plan, args, at).await {
                Ok(Written::Value(value)) => Ok(value),
                // By `id`, no record has it: the entity's not-found failure.
                Ok(Written::Missing) => {
                    match write
                        .missing
                        .as_ref()
                        .and_then(|missing| failure(program, missing, args, ctx))
                    {
                        Some(failure) => Err(Stop::Failed(ctx.fail(failure, at))),
                        None => Err(defect(ctx, "a write by id with no failure for none")),
                    }
                }
                Ok(Written::Refused { at: refusal, cause }) => {
                    match write.refusals.get(refusal as usize) {
                        Some(Refusal::Arm(arm)) => Err(Stop::Refused { arm: *arm, cause }),
                        Some(Refusal::NotFound(missing)) => {
                            match failure(program, missing, args, ctx) {
                                Some(mut failure) => {
                                    failure.cause = Some(cause);
                                    Err(Stop::Failed(ctx.fail(failure, at)))
                                }
                                None => Err(defect(ctx, "a reference with no failure for none")),
                            }
                        }
                        Some(Refusal::Gone(what)) => {
                            Err(Stop::Fault(ctx.fault(FaultKind::Store(what.clone()), at)))
                        }
                        None => Err(defect(ctx, "a refusal the plan doesn't list")),
                    }
                }
                Err(stop) => Err(interrupt(stop)),
            };
        }
        Target::Callable(_) => return Err(defect(ctx, "a call run as a read")),
    };
    match jadpo_runtime::read(ctx, read, args, at).await {
        Ok(Found::Value(value)) => Ok(value),
        Ok(Found::Missing) => {
            match missing.and_then(|missing| failure(program, missing, args, ctx)) {
                Some(failure) => Err(Stop::Failed(ctx.fail(failure, at))),
                None => Err(defect(
                    ctx,
                    "a read for one record with no failure for none",
                )),
            }
        }
        Err(stop) => Err(interrupt(stop)),
    }
}

/// The not-found failure a read raises, with its fields.
fn failure(
    program: &Program,
    missing: &MissingPlan,
    args: &[Value],
    ctx: &Ctx<'_>,
) -> Option<jadpo_runtime::Failure> {
    let slots: Vec<Option<u32>> = missing.fields.iter().map(|(slot, _)| *slot).collect();
    let values: Vec<Value> = missing
        .fields
        .iter()
        .map(|(_, value)| match value {
            MissingValue::Arg { index, path } => {
                let mut value = args.get(*index as usize).cloned().unwrap_or(Value::None);
                for name in path {
                    value = match &value {
                        Value::Record(record) => record.get(name).cloned().unwrap_or(Value::None),
                        _ => Value::None,
                    };
                }
                value
            }
            MissingValue::Constant(constant) => crate::values::value(constant),
            MissingValue::Now => Value::Instant(ctx.now()),
        })
        .collect();
    program.failure(missing.failure, &slots, values)
}

fn interrupt(stop: Interrupt) -> Stop {
    match stop {
        Interrupt::Failed(failure) => Stop::Failed(failure),
        Interrupt::Fault(fault) => Stop::Fault(fault),
    }
}

/// An action's body as the runtime's boundary runs it, once or, after a
/// concurrent change, again from the start.
struct Action<'a> {
    program: &'a Program,
    callee: u32,
    args: Vec<Value>,
    at: u32,
}

impl<'p> ActionBody<'p> for Action<'_> {
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
