//! The one entry point for a read or a write (area 6; docs/compiler-runtime.md
//! §2.5: `Ctx` `read` and `write`, with a plan and parameters). Either
//! executor hands the runtime a plan and the values its site worked out,
//! and gets back what the operation gave: its value, the failure it
//! raised, or the `conflict:` arm its site maps a refusal to. The plan says
//! what a missing record and each refused constraint raise
//! ([`jadpo_std::read::MissingPlan`], [`jadpo_std::write::Raises`]), so the
//! runtime raises them itself and an executor only dispatches the arm,
//! working out that arm's fields as its site writes them.
//!
//! Inside a running action the operation joins the action's transaction.
//! Called outside any action (a test's own read, later a route's display
//! read) it runs in a boundary of its own, as a one-statement action, at
//! repeatable read: a read loads what it reads, and its includes need the
//! first statement's snapshot.

use jadpo_std::read::{MissingPlan, MissingValue, ReadPlan};
use jadpo_std::types::FailurePlan;
use jadpo_std::write::{Raises, WritePlan};
use jadpo_value::Value;

use crate::read::{Found, Interrupt, JsonCodecs, read};
use crate::write::{Written, write};
use crate::{ActionBody, Ctx, Failure, Fault, FaultKind, Isolation, LocalFuture, Outcome, action};

/// What a store operation runs.
#[derive(Clone, Copy, Debug)]
pub enum StoreOp<'a> {
    /// A query, or an entity's `get`.
    Read(&'a ReadPlan),
    /// A `create`, `update` or `delete`, a lifecycle's transitions among
    /// them.
    Write(&'a WritePlan),
}

/// How a store operation ended.
#[derive(Debug)]
pub enum Stored {
    /// What it read, or the record it wrote (`Unit` for a delete).
    Value(Value),
    /// The failure it raised: the not-found failure its plan names, a
    /// page's invalid cursor, or `unavailable`.
    Failed(Box<Failure>),
    /// The store refused a write with a rule its site's `conflict:` arm at
    /// `arm` maps: the executor raises that arm's failure, with `cause`,
    /// the store's refusal, as its internal cause (area 4).
    Conflict {
        arm: u32,
        cause: Box<Failure>,
    },
    Fault(Box<Fault>),
}

/// Runs `op` with `args`, the values its site worked out (a read's
/// arguments, a write's inputs in the order its plan lists them), called
/// from site `site` of the frame on top of `ctx`. `failures` are the
/// program's failure plans, by number, which a missing record's failure is
/// raised from; `json` the codecs a JSON-held column is written and read
/// with, as `json.read` is given them (chunk 5.5w2b).
pub async fn store(
    ctx: &mut Ctx<'_>,
    op: StoreOp<'_>,
    failures: &[FailurePlan],
    json: JsonCodecs<'_>,
    args: &[Value],
    site: u32,
) -> Stored {
    if ctx.in_action() {
        return run(ctx, op, failures, json, args, site, false).await;
    }
    let last_read =
        ctx.can_end_read() && matches!(op, StoreOp::Read(plan) if crate::read::can_end(plan, json));
    let mut alone = Alone {
        last_read,
        op,
        failures,
        json,
        args,
        site,
        conflict: None,
    };
    match action(ctx, &mut alone).await {
        Outcome::Value(value) => Stored::Value(value),
        // The refusal it kept, unless the boundary ended otherwise, such as
        // `unavailable` after concurrent changes.
        Outcome::Failed(failure) => match alone.conflict.take() {
            Some((arm, cause)) if cause == failure => Stored::Conflict { arm, cause },
            _ => Stored::Failed(failure),
        },
        Outcome::Fault(fault) => Stored::Fault(fault),
        Outcome::Assertion(_) => Stored::Fault(ctx.fault(
            FaultKind::Defect("an assertion from a store operation".into()),
            site,
        )),
    }
}

/// A store operation run outside any action, as its own boundary's body.
struct Alone<'o> {
    last_read: bool,
    op: StoreOp<'o>,
    failures: &'o [FailurePlan],
    json: JsonCodecs<'o>,
    args: &'o [Value],
    site: u32,
    /// A refusal its site maps, kept while the boundary rolls back.
    conflict: Option<(u32, Box<Failure>)>,
}

impl<'p> ActionBody<'p> for Alone<'_> {
    fn isolation(&self) -> Isolation {
        Isolation::RepeatableRead
    }

    fn run<'c>(&'c mut self, ctx: &'c mut Ctx<'p>) -> LocalFuture<'c, Outcome> {
        Box::pin(async move {
            self.conflict = None;
            match run(
                ctx,
                self.op,
                self.failures,
                self.json,
                self.args,
                self.site,
                self.last_read,
            )
            .await
            {
                Stored::Value(value) => Outcome::Value(value),
                Stored::Failed(failure) => Outcome::Failed(failure),
                // Failed, so the boundary rolls the refused statement back.
                Stored::Conflict { arm, cause } => {
                    self.conflict = Some((arm, cause.clone()));
                    Outcome::Failed(cause)
                }
                Stored::Fault(fault) => Outcome::Fault(fault),
            }
        })
    }
}

/// Runs `op` in the running action's transaction.
async fn run(
    ctx: &mut Ctx<'_>,
    op: StoreOp<'_>,
    failures: &[FailurePlan],
    json: JsonCodecs<'_>,
    args: &[Value],
    site: u32,
    last_read: bool,
) -> Stored {
    let defect = |ctx: &mut Ctx<'_>, what: &str| {
        Stored::Fault(ctx.fault(FaultKind::Defect(what.into()), site))
    };
    match op {
        StoreOp::Read(plan) => match read(ctx, plan, json, args, site, last_read).await {
            Ok(Found::Value(value)) => Stored::Value(value),
            // A read for exactly one record found none: the failure its
            // plan names, its fields from the read's arguments.
            Ok(Found::Missing(concealed)) => {
                match missing(ctx, plan.missing.as_ref(), failures, args) {
                    Some(failure) => Stored::Failed(ctx.fail(concealing(failure, concealed), site)),
                    None => defect(ctx, "a read for one record with no failure for none"),
                }
            }
            Err(stop) => interrupted(stop),
        },
        StoreOp::Write(plan) => match write(ctx, plan, json, args, site).await {
            Ok(Written::Value(value)) => Stored::Value(value),
            // By `id`, no record has it: the entity's not-found failure.
            Ok(Written::Missing(concealed)) => {
                match missing(ctx, plan.missing.as_ref(), failures, args) {
                    Some(failure) => Stored::Failed(ctx.fail(concealing(failure, concealed), site)),
                    None => defect(ctx, "a write by id with no failure for none"),
                }
            }
            Ok(Written::NotPermitted) => {
                Stored::Failed(ctx.fail(crate::read::not_permitted(), site))
            }
            Ok(Written::Refused { at, cause }) => {
                match plan
                    .refusals
                    .get(at as usize)
                    .map(|refusal| &refusal.raises)
                {
                    Some(Raises::Arm(arm)) => Stored::Conflict { arm: *arm, cause },
                    Some(Raises::NotFound(plan)) => {
                        match missing(ctx, Some(plan), failures, args) {
                            Some(mut failure) => {
                                failure.cause = Some(cause);
                                Stored::Failed(ctx.fail(failure, site))
                            }
                            None => defect(ctx, "a reference with no failure for none"),
                        }
                    }
                    // Deleted while an outside call was out, after the
                    // snapshot that loaded it closed: a concurrent change,
                    // as repeatable read would find it (chunk 7.4).
                    Some(Raises::Gone(_)) if ctx.loaded_before_call() => {
                        ctx.concurrent_change();
                        defect(ctx, "a concurrent change")
                    }
                    Some(Raises::Gone(what)) => {
                        Stored::Fault(ctx.fault(FaultKind::Store(what.clone()), site))
                    }
                    None => defect(ctx, "a refusal the plan doesn't list"),
                }
            }
            Err(stop) => interrupted(stop),
        },
    }
}

/// A not-found failure for a record policy hid, which its log says (area
/// 4, "Concealing records"): the client sees the same failure as for a
/// record that isn't there.
fn concealing(mut failure: Failure, concealed: bool) -> Failure {
    if concealed {
        failure
            .internal
            .push(("concealed_by".to_string(), Value::text("policy")));
    }
    failure
}

fn interrupted(stop: Interrupt) -> Stored {
    match stop {
        Interrupt::Failed(failure) => Stored::Failed(failure),
        Interrupt::Fault(fault) => Stored::Fault(fault),
    }
}

/// The failure `plan` names for a record that isn't there, its fields from
/// the operation's arguments, constants and clock, before it is raised.
fn missing(
    ctx: &Ctx<'_>,
    plan: Option<&MissingPlan>,
    failures: &[FailurePlan],
    args: &[Value],
) -> Option<Failure> {
    let plan = plan?;
    let slots: Vec<Option<u32>> = plan.fields.iter().map(|(slot, _)| *slot).collect();
    let values: Vec<Value> = plan
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
            MissingValue::Constant(constant) => Value::constant(constant),
            MissingValue::Now => Value::Instant(ctx.now()),
        })
        .collect();
    Failure::of(failures.get(plan.failure.0 as usize)?, &slots, values)
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
