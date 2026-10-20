//! The interpreter's one walker over typed bodies (docs/compiler-runtime.md
//! §2.5): each statement and expression as the checker decided it, every
//! frame and failure through the runtime's [`Ctx`], which is handed in at
//! each step and never kept.
//!
//! The walk keeps its own stacks rather than recursing on Rust's: a stack
//! of tasks (what is left to do, innermost on top), a stack of values (what
//! the tasks below are waiting for) and the activations of the callables
//! it is in. So it can stop wherever the typed body says a site pauses (a
//! call into the runtime's action boundary, later a store operation) and
//! carry on later from exactly there: [`Machine::run`] gives back
//! [`Step::Pause`], the caller does the waiting, and [`Machine::resume`]
//! continues with the result. Functions never pause (area 3), so a call to
//! one is an activation on the same stacks, run synchronously, and a
//! recursion is bounded by the runtime's depth limit rather than by the
//! thread's stack.
//!
//! What stops a body early travels as a [`Stop`]: a `return`, a failure on
//! its way to an outcome `match` or an `assert … fails`, a fault on its
//! way out, a broken rule on its way to the `attempt` that maps it, or a
//! test's assertion. A stop unwinds the task stack to the first task that
//! handles it: the end of a callable for a `return`, an `attempt` for a
//! broken rule, and so on.

use std::rc::Rc;

use jadpo_graph::TypeRef;
use jadpo_graph::body::{
    Arm, ArmBody, BinaryOp, Body, Callee, ConflictArm, Expr, IfArm, LocalId, Node, Pattern,
    RuleArm, Site, SiteId, Stmt, UnaryOp,
};
use jadpo_plan::{ArmPattern, CallablePlan, Decoded};
use jadpo_runtime::effects::{self, Effect, Refused};
use jadpo_runtime::{Assertion, AssertionDetail, Ctx, Failure, Fault, FaultKind, Frame, Got};
use jadpo_std::{Kind as ParamKind, Op, RuleKey};
use jadpo_value::{Value, ops};

use crate::program::Program;

/// What ends a body before its last statement.
pub(crate) enum Stop {
    Return(Value),
    /// A construction's value broke a rule, for the `attempt` whose block
    /// maps it: the rule's key (`max`, or `format` when a built-in type
    /// such as `Email` refused the text), and the rule as written for the
    /// report.
    Invalid {
        rule: RuleKey,
        written: String,
        /// `json.read`'s first problem, as the `DecodeProblem` a failure
        /// mapped from `mismatch` receives (area 19, "Data formats").
        problem: Option<Value>,
        /// For a rule an object type names across its fields
        /// ([`RuleKey::Named`]), its name, which its `attempt` arm is keyed
        /// by (area 2, "Rules across fields").
        name: Option<String>,
    },
    Failed(Box<Failure>),
    Fault(Box<Fault>),
    Assertion(Box<Assertion>),
    /// The store refused a write with the rule its `conflict:` arm at
    /// `arm` maps, for the write's handler to raise that arm's failure
    /// with the refusal as its cause (area 6).
    Refused {
        arm: u32,
        cause: Box<Failure>,
    },
}

/// What a site that pauses waits on: a call into the runtime's action
/// boundary, or a read or write the runtime runs from its plan (area 6).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Target {
    /// An action, fixture or other callable, by number.
    Callable(u32),
    /// A query, by the number of its plan.
    Query(u32),
    /// `Entity.get`, by the number of the entity's plan.
    Get(u32),
    /// `create`, `update` or `delete`, by the number of the write's plan.
    Write(u32),
    /// A route test's request, by the route's place in the route table;
    /// `signed` when its last value is the caller it signs in as.
    /// A route test's request: with `keyed`, the `Idempotency-Key` it
    /// sends comes before the caller, which `signed` says it has.
    Request {
        route: u32,
        keyed: bool,
        signed: bool,
    },
    /// `sessions.start` or `sessions.end` (area 8).
    Session(jadpo_std::http::SessionOp),
    /// `batches.each`, by the number its site decodes, or with `None`
    /// `batches.status` and `batches.cancel` (area 12, chunk 6.4).
    Batch(jadpo_std::batch::BatchOp, Option<u32>),
    /// `store.put(bytes, type)` or `store.read(file)` through the `files`
    /// connection its site names (area 16, chunk 6.4b): an outside call.
    Files(jadpo_std::files::FilesOp),
    /// `connection.resource.operation(input)` through the connection its
    /// site names (area 11, chunk 7.2c): an outside call.
    Provider,
    /// A test's run of a schedule, by the callable number of the action it
    /// runs (area 13; chunk 6.2c).
    Schedule(u32),
    /// `clock.advance(...)` in a test, by the duration it is given: a
    /// step after which what has come due is delivered (area 13; chunk
    /// 6.11).
    AdvanceClock,
}

/// Where a run of the machine stopped.
pub(crate) enum Step {
    /// The invocation ended: its value, or the failure, fault or assertion
    /// that ended it.
    Done(Result<Value, Stop>),
    /// It reached a site that pauses: a call into the runtime's action
    /// boundary, or a read or write, which the caller makes, then resumes
    /// the machine with.
    Pause {
        target: Target,
        args: Vec<Value>,
        at: SiteId,
    },
    /// It checked the request's deadline, which hasn't passed: the caller
    /// gives the thread back once ([`jadpo_runtime::Host::yield_now`]), so other
    /// requests the thread serves carry on, then [`Machine::carry_on`].
    Yield,
}

/// What a look at the deadline found.
enum Deadline {
    /// Not looked at, or no request is waiting on one.
    Running,
    /// Looked at, not passed, and a request has one: give the thread back.
    Yield,
    Passed(Stop),
}

/// What a task leaves the loop to do next.
enum Flow {
    Next,
    Done(Result<Value, Stop>),
    Pause {
        target: Target,
        args: Vec<Value>,
        at: SiteId,
    },
    /// An inline function's body ran to its end.
    Nested(Result<Value, Stop>),
}

/// One running callable.
struct Activation<'p> {
    callable: &'p CallablePlan,
    locals: Vec<Value>,
    /// The failures the `match` arms being run handle, innermost last:
    /// what `propagate` passes on and what a `reject` there keeps as its
    /// cause.
    handling: Vec<Failure>,
}

/// What a call's outcome is caught for.
#[derive(Clone, Copy)]
enum Then<'p> {
    /// An outcome `match` at `at`.
    Outcome { at: SiteId, arms: &'p [Arm] },
    /// `assert … fails` at `at`, expecting the failure with key `key`.
    Fails { at: SiteId, key: &'p str },
}

/// What is left to do, innermost last. The first group walks the body;
/// the rest wait for values on the value stack; the handlers (marked)
/// also catch stops while a stop unwinds.
enum Task<'p> {
    Block {
        block: &'p [SiteId],
        next: usize,
    },
    Stmt(SiteId),
    Expr(SiteId),
    /// A statement without its value, or an operation without one of its
    /// arguments: the checker rules both out.
    Defect(&'static str, SiteId),
    /// Keeps the value on top for a failed assertion's report.
    Remember(SiteId),
    SetLocal {
        local: Option<LocalId>,
        at: SiteId,
    },
    Return,
    Discard,
    /// `emit`: the event on the stack goes in the action's outbox (area
    /// 12, chunk 6.1b).
    Emit(SiteId),
    PushUnit,
    /// `none`, for an option a standard-library call leaves out.
    PushNone,
    /// A `crypto` call's `key: config.<name>`: the setting's number, the
    /// handle the operation finds its key by; the key itself is never a
    /// value (area 19).
    PushKey(u32),
    /// `translations.<domain>.<message>(locale:, ...)` (area 10): the
    /// locale and the message's fields on the stack, in the entry's
    /// order.
    Translate {
        key: &'p str,
        count: usize,
        at: SiteId,
    },
    /// `prompts.render(prompt)` (area 21): the prompt's value on the
    /// stack.
    RenderPrompt(SiteId),
    /// A value an email's HTML template writes out (area 16): its text on
    /// the stack, escaped.
    Escape(SiteId),
    IfCheck {
        arms: &'p [IfArm],
        otherwise: &'p [SiteId],
        index: usize,
        at: SiteId,
    },
    ForStart {
        item: Option<LocalId>,
        body: &'p [SiteId],
        at: SiteId,
    },
    ForNext {
        item: Option<LocalId>,
        body: &'p [SiteId],
        items: Rc<[Value]>,
        index: usize,
        at: SiteId,
    },
    Field(SiteId),
    /// A test's `fakes:` scripting one answer of a provider operation
    /// (area 13; chunk 7.2d), with its value on top when it gives one.
    Fake(SiteId),
    /// `Principal.user(record)` (area 8): the caller made from the record
    /// on top.
    Caller {
        kind: jadpo_std::http::PrincipalKind,
        at: SiteId,
    },
    Apply {
        op: BinaryOp,
        at: SiteId,
    },
    Logic {
        op: BinaryOp,
        rhs: SiteId,
        at: SiteId,
    },
    LogicRhs(SiteId),
    Unary {
        op: UnaryOp,
        at: SiteId,
    },
    /// Calls `target` with the `count` values on top.
    Invoke {
        target: Target,
        count: usize,
        at: SiteId,
        /// For the subject of an outcome `match` or an `assert … fails`:
        /// only the call's own outcome is caught, not its arguments'.
        then: Option<Then<'p>>,
    },
    Std {
        op: Op,
        count: usize,
        inline: Option<(&'p [LocalId], SiteId)>,
        at: SiteId,
    },
    Construct(SiteId),
    BuildField {
        at: SiteId,
        index: usize,
        site: SiteId,
    },
    /// Keeps the value on top as the write's input at `index` keeps it: a
    /// value a field stores, the way the field's type stores it.
    WriteInput {
        plan: u32,
        index: usize,
        site: SiteId,
    },
    Build {
        at: SiteId,
        count: usize,
    },
    List {
        at: SiteId,
        count: usize,
    },
    MatchValue {
        at: SiteId,
        arms: &'p [Arm],
    },
    Reject {
        at: SiteId,
        count: usize,
    },
    /// Raises the failure an `attempt`'s arm maps a broken rule to, with
    /// its `count` fields on top.
    Raise {
        at: SiteId,
        inner: SiteId,
        arm: usize,
        written: String,
        problem: Option<Value>,
        count: usize,
    },
    /// Raises the failure an `attempt`'s arm maps a provider's failure to,
    /// with its `count` fields on top and the provider's as its cause
    /// (area 11, chunk 7.2c).
    RaiseMapped {
        at: SiteId,
        inner: SiteId,
        arm: usize,
        cause: Box<Failure>,
        count: usize,
    },
    /// Raises the failure a write's `conflict:` arm maps the store's
    /// refusal to, with its `count` fields on top.
    RaiseConflict {
        at: SiteId,
        arm: usize,
        cause: Box<Failure>,
        count: usize,
    },
    AssertTruth(SiteId),
    AssertCompare {
        at: SiteId,
        op: BinaryOp,
        value: SiteId,
    },
    // --- handlers ----------------------------------------------------------
    /// The end of a callable's activation; `base` is the value stack's
    /// height when it was entered.
    FrameEnd {
        base: usize,
    },
    /// A write, catching a refusal its `conflict:` maps to raise the arm's
    /// failure.
    Write {
        at: SiteId,
        arms: &'p [ConflictArm],
        base: usize,
    },
    /// An `attempt`, catching a broken rule its inner construction gives.
    Attempt {
        at: SiteId,
        inner: SiteId,
        arms: &'p [RuleArm],
        base: usize,
    },
    /// A call's outcome, caught for an outcome `match` or an `assert …
    /// fails`.
    Catch {
        base: usize,
        /// The call's site, whose value an assertion's report shows.
        remember: Option<SiteId>,
        then: Then<'p>,
    },
    /// The end of a failure arm: the failure is no longer handled.
    PopHandling,
    /// The end of an `assert`: its recording stops.
    AssertEnd {
        base: usize,
    },
    /// The end of an inline function's body, run inside an operation.
    Nested {
        base: usize,
    },
}

/// A comparison's operator as written.
fn symbol(op: BinaryOp) -> &'static str {
    match op {
        BinaryOp::Equal => "==",
        BinaryOp::NotEqual => "!=",
        BinaryOp::Less => "<",
        BinaryOp::LessEqual => "<=",
        BinaryOp::Greater => ">",
        BinaryOp::GreaterEqual => ">=",
        _ => "",
    }
}

fn is_comparison(op: BinaryOp) -> bool {
    matches!(
        op,
        BinaryOp::Equal
            | BinaryOp::NotEqual
            | BinaryOp::Less
            | BinaryOp::LessEqual
            | BinaryOp::Greater
            | BinaryOp::GreaterEqual
    )
}

/// The arm of an `attempt` that maps rule `rule`, or the rule across an
/// object's fields called `name`: its own, or `invalid:`.
fn rule_arm(arms: &[RuleArm], rule: RuleKey, name: Option<&str>) -> Option<usize> {
    let found = match name {
        Some(name) => RuleArm::for_named(arms, name)?,
        None => RuleArm::for_rule(arms, rule)?,
    };
    arms.iter().position(|arm| std::ptr::eq(arm, found))
}

/// One invocation's walk.
pub(crate) struct Machine<'p> {
    program: &'p Program,
    activations: Vec<Activation<'p>>,
    tasks: Vec<Task<'p>>,
    values: Vec<Value>,
    /// Tasks run since the deadline was last checked.
    steps: u32,
}

/// How many tasks run between looks at the request's deadline: work that
/// never waits is stopped within this many of the deadline passing (area
/// 7, "Time limits"), at the cost of one read of the clock per batch.
const DEADLINE_EVERY: u32 = 4096;

impl<'p> Machine<'p> {
    pub fn new(program: &'p Program) -> Machine<'p> {
        Machine {
            program,
            activations: Vec::new(),
            tasks: Vec::new(),
            values: Vec::new(),
            steps: 0,
        }
    }

    /// Once every [`DEADLINE_EVERY`] tasks, a look at the request's
    /// deadline: the deadline fault when it has passed, otherwise, when
    /// there is one, a turn for whatever else the thread runs.
    fn deadline(&mut self, ctx: &mut Ctx<'_>) -> Deadline {
        self.steps += 1;
        if self.steps < DEADLINE_EVERY {
            return Deadline::Running;
        }
        self.steps = 0;
        if ctx.out_of_time() {
            return Deadline::Passed(Stop::Fault(
                ctx.fault(FaultKind::DeadlineExceeded, Frame::ENTRY),
            ));
        }
        if ctx.has_deadline() {
            Deadline::Yield
        } else {
            Deadline::Running
        }
    }

    /// Starts callable `callee` with `args`, called from site `at` of the
    /// frame on top of `ctx` (none for the invocation's first), and runs
    /// it until it ends or pauses.
    pub fn start(&mut self, ctx: &mut Ctx<'_>, callee: u32, args: Vec<Value>, at: SiteId) -> Step {
        match self.enter(ctx, callee, args, at) {
            Ok(()) => self.run(ctx),
            Err(stop) => Step::Done(Err(stop)),
        }
    }

    /// A file helper's write's row (area 12, "Batches"; chunk 6.4b2): the
    /// inline function body at `body` in callable `callee`'s body, run in
    /// a frame of that callable whose only locals are `locals`, the item
    /// and what the run saved for the action's locals it reads. An inline
    /// function is pure, so it never pauses.
    pub fn row(
        &mut self,
        ctx: &mut Ctx<'_>,
        callee: u32,
        body: SiteId,
        locals: Vec<(u32, Value)>,
    ) -> Result<Value, Stop> {
        let program = self.program;
        let Some(callable) = program.plan().callable(callee) else {
            return Err(self.defect(ctx, "a row in a callable that has no body", body));
        };
        ctx.enter(callee, body.0).map_err(Stop::Fault)?;
        let mut held = vec![Value::Unit; callable.body.locals.len()];
        for (local, value) in locals {
            if let Some(slot) = held.get_mut(local as usize) {
                *slot = value;
            }
        }
        self.activations.push(Activation {
            callable,
            locals: held,
            handling: Vec::new(),
        });
        let result = self.nested(ctx, body);
        self.activations.pop();
        ctx.leave();
        result
    }

    /// The `files` connection the call at `at` goes through (chunk 6.4b).
    pub fn files_connection(&self, at: SiteId) -> Option<&'p str> {
        match &self.site(at)?.node {
            Node::Expr(Expr::Call {
                callee: Callee::Files { connection, .. },
                ..
            }) => Some(connection.as_str()),
            _ => None,
        }
    }

    /// The connection and operation the provider call at `at` makes
    /// (chunk 7.2c).
    pub fn provider_call(&self, at: SiteId) -> Option<(&'p str, &'p str)> {
        match &self.site(at)?.node {
            Node::Expr(Expr::Call {
                callee:
                    Callee::Provider {
                        connection,
                        operation,
                    },
                ..
            }) => Some((connection.as_str(), operation.as_str())),
            _ => None,
        }
    }

    /// Gives the fake of the connection the `fakes:` answer at `at`
    /// scripts its answer (chunk 7.2d): a value, on top, written as the
    /// provider sends it where the operation's `output_at:` reads it, so
    /// it is read back through the operation's real decoding.
    fn script_fake(&mut self, ctx: &mut Ctx<'_>, at: SiteId) -> Result<(), &'static str> {
        use jadpo_graph::body::{FakeAnswer, FakeCondition};
        use jadpo_runtime::fakes::Script;
        let Some(Node::Expr(Expr::Call {
            callee:
                Callee::Fake {
                    connection,
                    operation,
                    answer,
                },
            ..
        })) = self.site(at).map(|site| &site.node)
        else {
            return Err("a scripted answer the plan doesn't have");
        };
        let given = match answer {
            FakeAnswer::Value => Script::Value(self.pop()),
            FakeAnswer::Failed(code) => Script::Failed(code.clone()),
            FakeAnswer::Condition(FakeCondition::Unavailable | FakeCondition::InUse) => {
                Script::Unavailable
            }
            FakeAnswer::Condition(FakeCondition::Lost) => Script::Lost,
            FakeAnswer::Condition(FakeCondition::Misconfigured) => Script::Misconfigured,
        };
        ctx.at(at.0);
        let providers = self.program.providers();
        jadpo_runtime::fakes::script(ctx, &providers, (connection, operation), given)
    }

    /// What the provider call at `at` reads of its answer, and whether it
    /// maps `timed_out:` (chunk 7.2c2).
    pub fn provider_site(&self, at: SiteId) -> Option<&'p jadpo_std::provider::CallSite> {
        match self.decoded(at)? {
            Decoded::Provider(site) => Some(site),
            _ => None,
        }
    }

    /// Whether `key` is a failure of the provider operation `operation` of
    /// `connection`: its package's, which its call maps.
    fn provider_failure(&self, connection: &str, operation: &str, key: &str) -> bool {
        self.program
            .plan()
            .providers()
            .operation(connection, operation)
            .is_some_and(|(_, op)| op.failures.iter().any(|known| known == key))
    }

    /// Whether the call it paused at is caught, its failure handled by an
    /// outcome `match` or an `assert … fails` rather than passed on.
    pub fn catching(&self) -> bool {
        matches!(self.tasks.last(), Some(Task::Catch { .. }))
    }

    /// Carries on after a pause with what the paused call gave.
    pub fn resume(&mut self, ctx: &mut Ctx<'_>, result: Result<Value, Stop>) -> Step {
        match result {
            Ok(value) => self.values.push(value),
            Err(stop) => match self.unwind(ctx, stop) {
                Flow::Next => {}
                Flow::Done(result) | Flow::Nested(result) => return Step::Done(result),
                Flow::Pause { target, args, at } => return Step::Pause { target, args, at },
            },
        }
        self.run(ctx)
    }

    /// Carries on after giving the thread back ([`Step::Yield`]).
    pub fn carry_on(&mut self, ctx: &mut Ctx<'_>) -> Step {
        self.run(ctx)
    }

    /// Runs tasks until the invocation ends, pauses or gives the thread
    /// back.
    fn run(&mut self, ctx: &mut Ctx<'_>) -> Step {
        loop {
            let Some(task) = self.tasks.pop() else {
                return Step::Done(Err(self.defect(
                    ctx,
                    "a walk with nothing left to do",
                    SiteId(Frame::ENTRY),
                )));
            };
            let stepped = match self.deadline(ctx) {
                Deadline::Passed(stop) => Err(stop),
                Deadline::Yield => {
                    self.tasks.push(task);
                    return Step::Yield;
                }
                Deadline::Running => self.step(ctx, task),
            };
            let flow = match stepped {
                Ok(flow) => flow,
                Err(stop) => self.unwind(ctx, stop),
            };
            match flow {
                Flow::Next => {}
                Flow::Done(result) | Flow::Nested(result) => return Step::Done(result),
                Flow::Pause { target, args, at } => return Step::Pause { target, args, at },
            }
        }
    }

    /// Runs an inline function's body, `body`, to its end, inside an
    /// operation running on this machine: an inline function is pure, so
    /// it never pauses.
    fn nested(&mut self, ctx: &mut Ctx<'_>, body: SiteId) -> Result<Value, Stop> {
        self.tasks.push(Task::Nested {
            base: self.values.len(),
        });
        self.tasks.push(Task::Expr(body));
        loop {
            let Some(task) = self.tasks.pop() else {
                return Err(self.defect(ctx, "a walk with nothing left to do", body));
            };
            // An inline function runs inside an operation, so it can't
            // give the thread back; it only stops at the deadline.
            let stepped = match self.deadline(ctx) {
                Deadline::Passed(stop) => Err(stop),
                Deadline::Yield | Deadline::Running => self.step(ctx, task),
            };
            let flow = match stepped {
                Ok(flow) => flow,
                Err(stop) => self.unwind(ctx, stop),
            };
            match flow {
                Flow::Next => {}
                Flow::Nested(result) => return result,
                Flow::Pause { .. } => {
                    let stop = self.defect(ctx, "an inline function that pauses", body);
                    return match self.unwind(ctx, stop) {
                        Flow::Nested(result) | Flow::Done(result) => result,
                        _ => Err(self.defect(ctx, "an inline function that pauses", body)),
                    };
                }
                Flow::Done(result) => return result,
            }
        }
    }

    // --- activations and sites ----------------------------------------------

    /// Enters callable `callee` from site `at`: its frame on `ctx`, its
    /// activation, and its body's statements.
    fn enter(
        &mut self,
        ctx: &mut Ctx<'_>,
        callee: u32,
        args: Vec<Value>,
        at: SiteId,
    ) -> Result<(), Stop> {
        let program = self.program;
        let Some(callable) = program.plan().callable(callee) else {
            return Err(self.defect(ctx, "a call to a callable that has no body", at));
        };
        ctx.enter(callee, at.0).map_err(Stop::Fault)?;
        let body: &'p Body = &callable.body;
        let mut locals = vec![Value::Unit; body.locals.len()];
        for (param, arg) in body.params.iter().zip(args) {
            if let Some(slot) = locals.get_mut(param.0 as usize) {
                *slot = arg;
            }
        }
        self.activations.push(Activation {
            callable,
            locals,
            handling: Vec::new(),
        });
        self.tasks.push(Task::FrameEnd {
            base: self.values.len(),
        });
        self.tasks.push(Task::Block {
            block: &body.block,
            next: 0,
        });
        Ok(())
    }

    fn callable(&self) -> Option<&'p CallablePlan> {
        self.activations
            .last()
            .map(|activation| activation.callable)
    }

    fn site(&self, id: SiteId) -> Option<&'p Site> {
        self.callable()?.body.sites.get(id.0 as usize)
    }

    fn decoded(&self, id: SiteId) -> Option<&'p Decoded> {
        self.callable()?
            .sites
            .get(id.0 as usize)
            .map(|site| &site.decoded)
    }

    fn locals(&mut self) -> Option<&mut Vec<Value>> {
        self.activations
            .last_mut()
            .map(|activation| &mut activation.locals)
    }

    fn pop(&mut self) -> Value {
        self.values.pop().unwrap_or(Value::Unit)
    }

    /// The `count` values on top, in the order they were pushed.
    fn take(&mut self, count: usize) -> Vec<Value> {
        let at = self.values.len().saturating_sub(count);
        self.values.split_off(at)
    }

    fn set(
        &mut self,
        ctx: &mut Ctx<'_>,
        local: Option<LocalId>,
        value: Value,
        at: SiteId,
    ) -> Result<(), Stop> {
        let slot = local.and_then(|local| {
            self.locals()
                .and_then(|locals| locals.get_mut(local.0 as usize))
        });
        match slot {
            Some(slot) => {
                *slot = value;
                Ok(())
            }
            None => Err(self.defect(ctx, "a value given to no local", at)),
        }
    }

    /// Pushes what works out a statement's value, or the defect of a
    /// statement without one.
    fn needed(&mut self, value: Option<SiteId>, at: SiteId) {
        self.tasks.push(match value {
            Some(value) => Task::Expr(value),
            None => Task::Defect("a statement without its value", at),
        });
    }

    /// Pushes what works out a call's arguments, first on top.
    fn arguments(&mut self, args: &'p [Option<SiteId>], at: SiteId) {
        for arg in args.iter().rev() {
            self.needed(*arg, at);
        }
    }

    // --- stopping -----------------------------------------------------------

    fn fault(&self, ctx: &mut Ctx<'_>, kind: FaultKind, at: SiteId) -> Stop {
        Stop::Fault(ctx.fault(kind, at.0))
    }

    /// A prompt's value as the messages a model is sent, each field it
    /// writes given as its type's JSON (area 21).
    fn render_prompt(&self, prompt: &Value, at: SiteId) -> Option<Value> {
        let Some(Decoded::Prompt { entry, fields }) = self.decoded(at) else {
            return None;
        };
        let plan = self.program.plan();
        let entry = plan.prompts().entries.get(*entry as usize)?;
        let codecs = self.program.json_codecs();
        jadpo_value::prompt::render(entry, prompt, &mut |place, value| {
            let number = fields.get(place)?;
            let codec = plan.json().codec_of(*number)?;
            Some(codecs.write_value(codec, value))
        })
    }

    fn defect(&self, ctx: &mut Ctx<'_>, what: &str, at: SiteId) -> Stop {
        self.fault(ctx, FaultKind::Defect(what.to_string()), at)
    }

    /// `emit`: `value`, the event its statement at `at` built, run by the
    /// runtime's entry point for effects with its type's identity key
    /// (area 12; chunk 6.1b).
    fn emit(&self, ctx: &mut Ctx<'_>, value: Value, at: SiteId) -> Result<(), Stop> {
        let key = match self.site(at).map(|site| &site.node) {
            Some(Node::Stmt(Stmt::Emit { value: Some(built) })) => {
                match self.site(*built).map(|site| &site.ty) {
                    Some(jadpo_graph::TypeRef::Node(key)) => key.as_str(),
                    _ => return Err(self.defect(ctx, "an event of no declared type", at)),
                }
            }
            _ => return Err(self.defect(ctx, "an `emit` without its event", at)),
        };
        self.run_effect(ctx, Effect::Emit { event: key }, &[value], at)
            .map(|_| ())
    }

    /// An operation with effects, run by the runtime's one entry point for
    /// them, which compiled code calls too (chunk 7.0b): what it refused
    /// raised at `at`, a condition for its call's `attempt` to map.
    fn run_effect(
        &self,
        ctx: &mut Ctx<'_>,
        effect: Effect<'_>,
        values: &[Value],
        at: SiteId,
    ) -> Result<Value, Stop> {
        let effects = self.program.effects();
        effects::run(ctx, &effects, effect, values).map_err(|refused| match refused {
            Refused::Broke {
                rule,
                written,
                problem,
            } => Stop::Invalid {
                rule,
                written,
                problem,
                name: None,
            },
            Refused::Fault(fault) => self.value_fault(ctx, fault, at),
            Refused::Defect(what) => self.defect(ctx, &what, at),
        })
    }

    fn not_built_yet(&self, ctx: &mut Ctx<'_>, what: impl Into<String>, at: SiteId) -> Stop {
        self.fault(ctx, FaultKind::NotBuiltYet(what.into()), at)
    }

    /// What a site something later builds is written as.
    fn written(&self, at: SiteId) -> String {
        match self.decoded(at) {
            Some(Decoded::Written(written)) => written.clone(),
            _ => String::new(),
        }
    }

    /// An operator's or operation's fault, at its site.
    fn value_fault(&self, ctx: &mut Ctx<'_>, fault: jadpo_value::Fault, at: SiteId) -> Stop {
        self.fault(ctx, fault.into(), at)
    }

    /// A value the checker proved follows its type's rules, breaking one:
    /// a hole in the proof, found only when proofs are checked again
    /// ([`Program::with_proof_checks`]).
    fn proof_broken(&self, ctx: &mut Ctx<'_>, invalid: &jadpo_value::Invalid, at: SiteId) -> Stop {
        self.defect(
            ctx,
            &format!(
                "a value the checker proved follows its type's rules, breaking `{}`",
                invalid.rule()
            ),
            at,
        )
    }

    // --- the loop's one step ------------------------------------------------

    fn step(&mut self, ctx: &mut Ctx<'_>, task: Task<'p>) -> Result<Flow, Stop> {
        match task {
            Task::Block { block, next } => {
                if let Some(&stmt) = block.get(next) {
                    self.tasks.push(Task::Block {
                        block,
                        next: next + 1,
                    });
                    self.tasks.push(Task::Stmt(stmt));
                }
            }
            Task::Stmt(id) => return self.stmt(ctx, id),
            Task::Expr(id) => return self.expr(ctx, id),
            Task::Defect(what, at) => return Err(self.defect(ctx, what, at)),
            Task::Remember(id) => {
                if let Some(value) = self.values.last() {
                    ctx.record(id.0, value);
                }
            }
            Task::SetLocal { local, at } => {
                let value = self.pop();
                self.set(ctx, local, value, at)?;
            }
            Task::Return => return Err(Stop::Return(self.pop())),
            Task::Discard => {
                self.pop();
            }
            Task::Emit(at) => {
                let value = self.pop();
                self.emit(ctx, value, at)?;
            }
            Task::PushUnit => self.values.push(Value::Unit),
            Task::PushNone => self.values.push(Value::None),
            Task::PushKey(setting) => self.values.push(Value::Int(i64::from(setting))),
            Task::Translate { key, count, at } => {
                let values = self.take(count);
                let Some((locale, rest)) = values.split_first() else {
                    return Err(self.defect(ctx, "a translation without its locale", at));
                };
                let plan = self.program.plan();
                let Some((_, entry)) = plan.translations().entry(key) else {
                    return Err(self.defect(ctx, "a translation key the plan doesn't have", at));
                };
                let Some(locales) = plan.locales() else {
                    return Err(self.defect(ctx, "a translation in a project with no locales", at));
                };
                match jadpo_value::present::translate(entry, locale, rest, locales) {
                    Ok(text) => self.values.push(text),
                    Err(fault) => return Err(self.value_fault(ctx, fault, at)),
                }
            }
            Task::Escape(at) => match self.pop() {
                Value::Text(text) => self
                    .values
                    .push(Value::text(&jadpo_std::email::escape_html(&text))),
                _ => {
                    return Err(self.defect(
                        ctx,
                        "a template writing out something that isn't text",
                        at,
                    ));
                }
            },
            Task::RenderPrompt(at) => {
                let prompt = self.pop();
                let rendered = self.render_prompt(&prompt, at);
                match rendered {
                    Some(messages) => self.values.push(messages),
                    None => {
                        return Err(self.defect(ctx, "a prompt the plan can't render", at));
                    }
                }
            }
            Task::IfCheck {
                arms,
                otherwise,
                index,
                at,
            } => match self.pop() {
                Value::Bool(true) => {
                    if let Some(arm) = arms.get(index) {
                        self.tasks.push(Task::Block {
                            block: &arm.then,
                            next: 0,
                        });
                    }
                }
                Value::Bool(false) => self.if_from(arms, otherwise, index + 1, at),
                _ => return Err(self.defect(ctx, "a condition that isn't a `Bool`", at)),
            },
            // A set is held in the canonical order, which it is walked in
            // (area 2, "Set order").
            Task::ForStart { item, body, at } => match self.pop() {
                Value::List(items) | Value::Set(items) => self.tasks.push(Task::ForNext {
                    item,
                    body,
                    items,
                    index: 0,
                    at,
                }),
                _ => {
                    return Err(self.defect(
                        ctx,
                        "a loop over something that isn't a list or a set",
                        at,
                    ));
                }
            },
            Task::ForNext {
                item,
                body,
                items,
                index,
                at,
            } => {
                if let Some(value) = items.get(index) {
                    self.set(ctx, item, value.clone(), at)?;
                    self.tasks.push(Task::ForNext {
                        item,
                        body,
                        items,
                        index: index + 1,
                        at,
                    });
                    self.tasks.push(Task::Block {
                        block: body,
                        next: 0,
                    });
                }
            }
            Task::Fake(at) => {
                if let Err(why) = self.script_fake(ctx, at) {
                    return Err(self.defect(ctx, why, at));
                }
                self.values.push(Value::Unit);
            }
            Task::Caller { kind, at } => {
                let record = self.pop();
                let made = self
                    .program
                    .plan()
                    .routes()
                    .identity
                    .as_ref()
                    .and_then(|identity| identity.principal(kind))
                    .and_then(|plan| {
                        jadpo_runtime::http::identity::caller_from_record(plan, kind, &record)
                    });
                match made {
                    Some(value) => self.values.push(value),
                    None => {
                        return Err(self.defect(
                            ctx,
                            "a caller made from something that isn't its record",
                            at,
                        ));
                    }
                }
            }
            Task::Field(at) => {
                let receiver = self.pop();
                let name = match self.decoded(at) {
                    Some(Decoded::Field { name }) => name.as_str(),
                    _ => "",
                };
                let found = match &receiver {
                    Value::Record(record) => record.get(name),
                    Value::Variant(variant) => variant.get(name),
                    _ => None,
                };
                match found {
                    Some(value) => self.values.push(value.clone()),
                    None => {
                        return Err(self.defect(
                            ctx,
                            "a field read from a value that hasn't it",
                            at,
                        ));
                    }
                }
            }
            Task::Apply { op, at } => {
                let right = self.pop();
                let left = self.pop();
                let value = self.apply(ctx, op, &left, &right, at)?;
                self.values.push(value);
            }
            Task::Logic { op, rhs, at } => {
                // `and` and `or` stop as soon as the answer is known.
                let stop_at = op == BinaryOp::Or;
                match self.pop() {
                    Value::Bool(b) if b == stop_at => self.values.push(Value::Bool(b)),
                    Value::Bool(_) => {
                        self.tasks.push(Task::LogicRhs(at));
                        self.tasks.push(Task::Expr(rhs));
                    }
                    _ => {
                        return Err(self.defect(
                            ctx,
                            "`and` or `or` on something that isn't a `Bool`",
                            at,
                        ));
                    }
                }
            }
            Task::LogicRhs(at) => match self.pop() {
                Value::Bool(b) => self.values.push(Value::Bool(b)),
                _ => {
                    return Err(self.defect(
                        ctx,
                        "`and` or `or` on something that isn't a `Bool`",
                        at,
                    ));
                }
            },
            Task::Unary { op, at } => {
                let value = self.pop();
                let result = match (op, &value) {
                    (UnaryOp::Not, Value::Bool(b)) => Value::Bool(!b),
                    (UnaryOp::Not, _) => {
                        return Err(self.defect(ctx, "`not` on something that isn't a `Bool`", at));
                    }
                    (UnaryOp::Negate, _) => {
                        ops::negate(&value).map_err(|fault| self.value_fault(ctx, fault, at))?
                    }
                };
                self.values.push(result);
            }
            Task::Invoke {
                target,
                count,
                at,
                then,
            } => {
                let args = self.take(count);
                if let Some(then) = then {
                    self.tasks.push(Task::Catch {
                        base: self.values.len(),
                        remember: Some(at),
                        then,
                    });
                }
                match target {
                    Target::Callable(callee) => {
                        if self.site(at).is_some_and(|site| site.pauses) {
                            return Ok(Flow::Pause { target, args, at });
                        }
                        self.enter(ctx, callee, args, at)?;
                    }
                    // A read or a write always waits on the store, and a
                    // request on the HTTP component.
                    Target::Query(_)
                    | Target::Get(_)
                    | Target::Write(_)
                    | Target::Request { .. }
                    | Target::Session(_)
                    | Target::Batch(..)
                    | Target::Files(..)
                    | Target::Provider
                    | Target::Schedule(_)
                    | Target::AdvanceClock => {
                        return Ok(Flow::Pause { target, args, at });
                    }
                }
            }
            Task::Std {
                op,
                count,
                inline,
                at,
            } => {
                let values = self.take(count);
                let value = self.standard(ctx, op, &values, inline, at)?;
                self.values.push(value);
            }
            Task::Construct(at) => {
                let value = self.pop();
                let (validator, fallible, set) = match self.decoded(at) {
                    Some(Decoded::Construct {
                        validator,
                        fallible,
                        set,
                    }) => (self.program.validator(*validator), *fallible, *set),
                    _ => {
                        return Err(self.defect(
                            ctx,
                            "a construction the plan has no rules for",
                            at,
                        ));
                    }
                };
                // A set built from a list drops its repeats, in the
                // canonical order, before its lengths are counted.
                let value = match value {
                    Value::List(items) if set => Value::set(items.to_vec()),
                    value => value,
                };
                let Some(value) = validator.store(value) else {
                    return Err(self.fault(ctx, FaultKind::DecimalOverflow, at));
                };
                if fallible {
                    if let Err(invalid) = validator.check(&value) {
                        return Err(Stop::Invalid {
                            rule: invalid.key(),
                            written: invalid.broken(None),
                            problem: None,
                            name: invalid.name().map(str::to_string),
                        });
                    }
                } else if self.program.proof_checks()
                    && let Err(invalid) = validator.check(&value)
                {
                    return Err(self.proof_broken(ctx, &invalid, at));
                }
                self.values.push(value);
            }
            Task::WriteInput { plan, index, site } => {
                let value = self.pop();
                let validator = self
                    .program
                    .plan()
                    .write(plan)
                    .and_then(|write| write.inputs.get(index))
                    .and_then(|(_, validator)| *validator);
                let Some(validator) = validator else {
                    self.values.push(value);
                    return Ok(Flow::Next);
                };
                // A value a field stores is kept as the field's type keeps
                // it, and is already proved, as an object literal's is.
                let validator = self.program.validator(validator);
                let Some(value) = validator.store(value) else {
                    return Err(self.fault(ctx, FaultKind::DecimalOverflow, site));
                };
                if self.program.proof_checks()
                    && let Err(invalid) = validator.check(&value)
                {
                    return Err(self.proof_broken(ctx, &invalid, site));
                }
                self.values.push(value);
            }
            Task::BuildField { at, index, site } => {
                let value = self.pop();
                let (given, fallible) = match self.decoded(at) {
                    Some(Decoded::Build {
                        fields, fallible, ..
                    }) => (fields.get(index), *fallible),
                    _ => (None, false),
                };
                let Some(given) = given else {
                    return Err(self.defect(ctx, "a build the plan has no fields for", at));
                };
                let validator = self.program.validator(given.validator);
                let Some(value) = validator.store(value) else {
                    return Err(self.fault(ctx, FaultKind::DecimalOverflow, at));
                };
                if fallible {
                    if let Err(invalid) = validator.check(&value) {
                        return Err(Stop::Invalid {
                            rule: invalid.key(),
                            written: invalid.broken(Some(&given.name)),
                            problem: None,
                            name: invalid.name().map(str::to_string),
                        });
                    }
                } else if self.program.proof_checks()
                    && let Err(invalid) = validator.check(&value)
                {
                    return Err(self.proof_broken(ctx, &invalid, site));
                }
                self.values.push(value);
            }
            Task::Build { at, count } => {
                let values = self.take(count);
                let (built, across) = match self.decoded(at) {
                    Some(Decoded::Build {
                        shape: Some(shape),
                        fields,
                        across,
                        ..
                    }) => (
                        self.program.shape(*shape).and_then(|shape| {
                            shape.build(fields.iter().map(|given| given.slot).zip(values))
                        }),
                        *across,
                    ),
                    _ => (None, None),
                };
                let Some(value) = built else {
                    return Err(self.defect(ctx, "a build without one of its fields", at));
                };
                // The rules across its fields, once every field has its
                // value (area 2, "Rules across fields"): one that can break
                // fails the construction by its name, for its `attempt`.
                if let Some((validator, breaks)) = across
                    && (breaks || self.program.proof_checks())
                    && let Err(invalid) = self.program.validator(validator).check(&value)
                {
                    if breaks {
                        return Err(Stop::Invalid {
                            rule: invalid.key(),
                            written: invalid.broken(None),
                            problem: None,
                            name: invalid.name().map(str::to_string),
                        });
                    }
                    return Err(self.proof_broken(ctx, &invalid, at));
                }
                self.values.push(value);
            }
            Task::List { at, count } => {
                let items = self.take(count);
                let (set, map, validator) = match self.decoded(at) {
                    Some(Decoded::List {
                        set,
                        map,
                        validator,
                    }) => (*set, *map, Some(*validator)),
                    _ => (false, false, None),
                };
                // `[]` where a map goes is an empty map; the checker allows
                // no items there (area 19, question 8).
                let list = if map && items.is_empty() {
                    Value::Map(Rc::default())
                } else if set {
                    Value::set(items)
                } else {
                    Value::list(items)
                };
                // A list written into a type with lengths is proved by the
                // checker, as a literal is.
                if self.program.proof_checks()
                    && let Some(validator) = validator
                    && let Err(invalid) = self.program.validator(validator).check(&list)
                {
                    return Err(self.proof_broken(ctx, &invalid, at));
                }
                self.values.push(list);
            }
            Task::MatchValue { at, arms } => {
                let value = self.pop();
                let patterns = match self.decoded(at) {
                    Some(Decoded::Match { arms }) => arms.as_slice(),
                    _ => &[],
                };
                for (index, arm) in arms.iter().enumerate() {
                    if self.matches(ctx, &arm.pattern, patterns.get(index), &value, at)? {
                        if let Some(binding) = arm.binding {
                            self.set(ctx, Some(binding), value.clone(), at)?;
                        }
                        self.arm(arm);
                        return Ok(Flow::Next);
                    }
                }
                return Err(self.defect(ctx, "a `match` whose arms miss a case", at));
            }
            Task::Reject { at, count } => {
                let values = self.take(count);
                let failure = match self.decoded(at) {
                    Some(Decoded::Reject {
                        failure: Some(failure),
                        slots,
                    }) => self.program.failure(*failure, slots, values),
                    _ => None,
                };
                let Some(mut failure) = failure else {
                    return Err(self.defect(
                        ctx,
                        "a `reject` of something that isn't a failure",
                        at,
                    ));
                };
                failure.cause = self
                    .activations
                    .last()
                    .and_then(|activation| activation.handling.last())
                    .cloned()
                    .map(Box::new);
                return Err(Stop::Failed(ctx.fail(failure, at.0)));
            }
            Task::Raise {
                at,
                inner,
                arm,
                written,
                problem,
                count,
            } => {
                let mut values = self.take(count);
                let failure = match self.decoded(at) {
                    Some(Decoded::Attempt { arms }) => {
                        arms.get(arm).and_then(Option::as_ref).and_then(|raise| {
                            // `json.read`'s first problem fills the
                            // failure's `problem: DecodeProblem` (area 19).
                            let mut slots = raise.slots.clone();
                            if let (Some(slot), Some(problem)) = (raise.problem, problem) {
                                slots.push(Some(slot));
                                values.push(problem);
                            }
                            self.program.failure(raise.failure, &slots, values)
                        })
                    }
                    _ => None,
                };
                let Some(mut failure) = failure else {
                    return Err(self.defect(
                        ctx,
                        "an arm naming something that isn't a failure",
                        at,
                    ));
                };
                failure.broken = Some(written);
                return Err(Stop::Failed(ctx.fail(failure, inner.0)));
            }
            Task::RaiseMapped {
                at,
                inner,
                arm,
                cause,
                count,
            } => {
                let values = self.take(count);
                let failure = match self.decoded(at) {
                    Some(Decoded::Attempt { arms }) => {
                        arms.get(arm).and_then(Option::as_ref).and_then(|raise| {
                            self.program.failure(raise.failure, &raise.slots, values)
                        })
                    }
                    _ => None,
                };
                let Some(mut failure) = failure else {
                    return Err(self.defect(
                        ctx,
                        "an arm naming something that isn't a failure",
                        at,
                    ));
                };
                failure.cause = Some(cause);
                return Err(Stop::Failed(ctx.fail(failure, inner.0)));
            }
            Task::RaiseConflict {
                at,
                arm,
                cause,
                count,
            } => {
                let values = self.take(count);
                let failure = match self.decoded(at) {
                    Some(Decoded::Write { arms, .. }) => {
                        arms.get(arm).and_then(Option::as_ref).and_then(|raise| {
                            self.program.failure(raise.failure, &raise.slots, values)
                        })
                    }
                    _ => None,
                };
                let Some(mut failure) = failure else {
                    return Err(self.defect(
                        ctx,
                        "a conflict arm naming something that isn't a failure",
                        at,
                    ));
                };
                failure.cause = Some(cause);
                return Err(Stop::Failed(ctx.fail(failure, at.0)));
            }
            Task::AssertTruth(at) => {
                if self.pop() != Value::Bool(true) {
                    return Err(Stop::Assertion(ctx.assertion_failed(
                        at.0,
                        AssertionDetail::False { comparison: None },
                    )));
                }
            }
            Task::AssertCompare { at, op, value } => {
                let right = self.pop();
                let left = self.pop();
                if self.apply(ctx, op, &left, &right, value)? != Value::Bool(true) {
                    return Err(Stop::Assertion(ctx.assertion_failed(
                        at.0,
                        AssertionDetail::False {
                            comparison: Some((symbol(op), left, right)),
                        },
                    )));
                }
            }
            Task::FrameEnd { .. } => {
                // The body ran to its end: an action that returns nothing.
                self.activations.pop();
                ctx.leave();
                if self.activations.is_empty() {
                    return Ok(Flow::Done(Ok(Value::Unit)));
                }
                self.values.push(Value::Unit);
            }
            // A construction that didn't break a rule, or a write the store
            // didn't refuse: its value passes.
            Task::Attempt { .. } | Task::Write { .. } => {}
            Task::Catch { remember, then, .. } => {
                let value = self.pop();
                if let Some(site) = remember {
                    ctx.record(site.0, &value);
                }
                return self.caught(ctx, then, Ok(value));
            }
            Task::PopHandling => {
                if let Some(activation) = self.activations.last_mut() {
                    activation.handling.pop();
                }
            }
            Task::AssertEnd { .. } => {
                ctx.assertion_end();
            }
            Task::Nested { .. } => return Ok(Flow::Nested(Ok(self.pop()))),
        }
        Ok(Flow::Next)
    }

    /// Unwinds the task stack with `stop` to the first task that handles
    /// it, and carries on from there.
    fn unwind(&mut self, ctx: &mut Ctx<'_>, mut stop: Stop) -> Flow {
        while let Some(task) = self.tasks.pop() {
            match task {
                Task::FrameEnd { base } => {
                    self.values.truncate(base);
                    if let Stop::Invalid { .. } = stop {
                        stop = self.defect(
                            ctx,
                            "a construction failed outside an `attempt`",
                            SiteId(Frame::ENTRY),
                        );
                    }
                    self.activations.pop();
                    ctx.leave();
                    let last = self.activations.is_empty();
                    match stop {
                        Stop::Return(value) if last => return Flow::Done(Ok(value)),
                        Stop::Return(value) => {
                            self.values.push(value);
                            return Flow::Next;
                        }
                        other if last => return Flow::Done(Err(other)),
                        other => stop = other,
                    }
                }
                Task::Attempt {
                    at,
                    inner,
                    arms,
                    base,
                } => {
                    if let Stop::Invalid {
                        rule,
                        written,
                        problem,
                        name,
                    } = stop
                    {
                        self.values.truncate(base);
                        let Some(index) = rule_arm(arms, rule, name.as_deref()) else {
                            stop = self.defect(
                                ctx,
                                "a construction that can fail, with no failure for it",
                                at,
                            );
                            continue;
                        };
                        // The failure's fields are worked out only when it
                        // is raised, as a `reject`'s are.
                        let fields = &arms[index].fields;
                        self.tasks.push(Task::Raise {
                            at,
                            inner,
                            arm: index,
                            written,
                            problem,
                            count: fields.len(),
                        });
                        for (_, site) in fields.iter().rev() {
                            self.tasks.push(Task::Expr(*site));
                        }
                        return Flow::Next;
                    }
                    // A provider's failure is mapped by its code (area 11,
                    // chunk 7.2c), with the provider's kept as the cause;
                    // one no arm maps is a fault, never passed on.
                    // So is its `timed_out` where its `attempt` maps it
                    // (chunk 7.2c2).
                    if let Stop::Failed(failure) = &stop
                        && let Some((connection, operation)) = self.provider_call(inner)
                        && (self.provider_failure(connection, operation, &failure.key)
                            || failure.key == "runtime timed_out")
                    {
                        let code = failure.code.clone();
                        self.values.truncate(base);
                        let Some(index) = arms
                            .iter()
                            .position(|arm| arm.named.as_deref() == Some(code.as_str()))
                        else {
                            stop = self.fault(
                                ctx,
                                FaultKind::Provider(format!(
                                    "`{connection}.{operation}` was refused with `{code}`, which its call maps to nothing"
                                )),
                                inner,
                            );
                            continue;
                        };
                        let Stop::Failed(cause) =
                            std::mem::replace(&mut stop, Stop::Return(Value::Unit))
                        else {
                            continue;
                        };
                        let fields = &arms[index].fields;
                        self.tasks.push(Task::RaiseMapped {
                            at,
                            inner,
                            arm: index,
                            cause,
                            count: fields.len(),
                        });
                        for (_, site) in fields.iter().rev() {
                            self.tasks.push(Task::Expr(*site));
                        }
                        return Flow::Next;
                    }
                }
                Task::Write { at, arms, base } => {
                    if let Stop::Refused { arm, cause } = stop {
                        self.values.truncate(base);
                        let Some(found) = arms.get(arm as usize) else {
                            stop = self.defect(ctx, "a refusal with no arm for it", at);
                            continue;
                        };
                        // The failure's fields are worked out only when it
                        // is raised, as a `reject`'s are.
                        self.tasks.push(Task::RaiseConflict {
                            at,
                            arm: arm as usize,
                            cause,
                            count: found.fields.len(),
                        });
                        for (_, site) in found.fields.iter().rev() {
                            self.tasks.push(Task::Expr(*site));
                        }
                        return Flow::Next;
                    }
                }
                Task::Catch { base, then, .. } => {
                    if let Stop::Failed(failure) = stop {
                        self.values.truncate(base);
                        match self.caught(ctx, then, Err(failure)) {
                            Ok(flow) => return flow,
                            Err(next) => stop = next,
                        }
                    }
                }
                Task::PopHandling => {
                    if let Some(activation) = self.activations.last_mut() {
                        activation.handling.pop();
                    }
                }
                Task::AssertEnd { base } => {
                    self.values.truncate(base);
                    let parts = ctx.assertion_end();
                    if let Stop::Assertion(assertion) = &mut stop {
                        assertion.parts = parts;
                    }
                }
                Task::Nested { base } => {
                    self.values.truncate(base);
                    return Flow::Nested(Err(stop));
                }
                _ => {}
            }
        }
        Flow::Done(Err(stop))
    }

    // --- statements ---------------------------------------------------------

    fn stmt(&mut self, ctx: &mut Ctx<'_>, id: SiteId) -> Result<Flow, Stop> {
        let Some(Node::Stmt(stmt)) = self.site(id).map(|site| &site.node) else {
            return Err(self.defect(ctx, "an expression where a statement belongs", id));
        };
        match stmt {
            Stmt::Var { local, value } | Stmt::Assign { local, value } => {
                self.tasks.push(Task::SetLocal {
                    local: *local,
                    at: id,
                });
                self.needed(*value, id);
            }
            Stmt::Return { value } => match value {
                Some(value) => {
                    self.tasks.push(Task::Return);
                    self.tasks.push(Task::Expr(*value));
                }
                None => return Err(Stop::Return(Value::Unit)),
            },
            Stmt::If {
                arms, otherwise, ..
            } => self.if_from(arms, otherwise, 0, id),
            Stmt::For { item, list, body } => {
                self.tasks.push(Task::ForStart {
                    item: *item,
                    body,
                    at: id,
                });
                self.needed(*list, id);
            }
            Stmt::Emit { value } => {
                self.tasks.push(Task::Emit(id));
                self.needed(*value, id);
            }
            Stmt::Assert { value, fails } => return self.assert(ctx, id, *value, fails.as_ref()),
            Stmt::Expr { value } => {
                self.tasks.push(Task::Discard);
                self.needed(*value, id);
            }
        }
        Ok(Flow::Next)
    }

    /// The `if` chain from its `index`th condition: that condition, or the
    /// `else` once none is left.
    fn if_from(&mut self, arms: &'p [IfArm], otherwise: &'p [SiteId], index: usize, at: SiteId) {
        match arms.get(index) {
            Some(arm) => {
                self.tasks.push(Task::IfCheck {
                    arms,
                    otherwise,
                    index,
                    at,
                });
                self.needed(arm.condition, at);
            }
            None => self.tasks.push(Task::Block {
                block: otherwise,
                next: 0,
            }),
        }
    }

    /// `assert condition`, or `assert call(...) fails Failure` (area 13):
    /// while the condition runs, the runtime records the values its frame
    /// works out, for the report if it fails.
    fn assert(
        &mut self,
        ctx: &mut Ctx<'_>,
        id: SiteId,
        value: Option<SiteId>,
        fails: Option<&'p TypeRef>,
    ) -> Result<Flow, Stop> {
        let Some(value) = value else {
            return Err(self.defect(ctx, "an `assert` without a condition", id));
        };
        ctx.assertion_begin();
        self.tasks.push(Task::AssertEnd {
            base: self.values.len(),
        });
        if let Some(expected) = fails {
            let TypeRef::Node(expected) = expected else {
                return Err(self.defect(ctx, "an `assert … fails` naming no failure", id));
            };
            return self.subject(
                ctx,
                value,
                Then::Fails {
                    at: id,
                    key: expected.as_str(),
                },
            );
        }
        match self.site(value).map(|site| &site.node) {
            Some(Node::Expr(Expr::Binary { op, lhs, rhs })) if is_comparison(*op) => {
                self.tasks.push(Task::AssertCompare {
                    at: id,
                    op: *op,
                    value,
                });
                self.tasks.push(Task::Expr(*rhs));
                self.tasks.push(Task::Expr(*lhs));
            }
            _ => {
                self.tasks.push(Task::AssertTruth(id));
                self.tasks.push(Task::Expr(value));
            }
        }
        Ok(Flow::Next)
    }

    // --- expressions --------------------------------------------------------

    fn expr(&mut self, ctx: &mut Ctx<'_>, id: SiteId) -> Result<Flow, Stop> {
        let Some(site) = self.site(id) else {
            return Err(self.defect(ctx, "a site the body doesn't have", id));
        };
        let Node::Expr(expr) = &site.node else {
            return Err(self.defect(ctx, "a statement where a value belongs", id));
        };
        // A literal's value is in the source already; anything else the
        // assertion running in this frame keeps for its report.
        if ctx.recording() && !matches!(expr, Expr::Literal(_)) {
            self.tasks.push(Task::Remember(id));
        }
        match expr {
            Expr::Literal(_) => match self.decoded(id) {
                Some(Decoded::Constant(Some(constant))) => {
                    let value = self.program.constant(*constant);
                    self.values.push(value);
                }
                _ => return Err(self.defect(ctx, "a literal the checker would have refused", id)),
            },
            Expr::Local(local) => {
                let value = self
                    .activations
                    .last()
                    .and_then(|activation| activation.locals.get(local.0 as usize))
                    .cloned();
                match value {
                    Some(value) => self.values.push(value),
                    None => return Err(self.defect(ctx, "a local the body doesn't have", id)),
                }
            }
            Expr::Field { receiver, .. } => {
                self.tasks.push(Task::Field(id));
                self.tasks.push(Task::Expr(*receiver));
            }
            Expr::Call { callee, args, .. } => return self.call(ctx, id, callee, args),
            // A route test's request (area 13): its placeholders and input
            // worked out, then the runtime's HTTP component answers it.
            Expr::Request {
                route,
                params,
                input,
                idempotency_key,
                caller,
            } => {
                let Some(index) = self
                    .program
                    .plan()
                    .routes()
                    .routes
                    .iter()
                    .position(|plan| plan.key == route.as_str())
                else {
                    return Err(self.defect(ctx, "a request to a route with no plan", id));
                };
                let parts: Vec<SiteId> = params
                    .iter()
                    .copied()
                    .chain(*input)
                    .chain(*idempotency_key)
                    .chain(*caller)
                    .collect();
                self.tasks.push(Task::Invoke {
                    target: Target::Request {
                        route: index as u32,
                        keyed: idempotency_key.is_some(),
                        signed: caller.is_some(),
                    },
                    count: parts.len(),
                    at: id,
                    then: None,
                });
                for site in parts.into_iter().rev() {
                    self.tasks.push(Task::Expr(site));
                }
            }
            Expr::Write { conflicts, .. } => {
                let Some(Decoded::Write {
                    plan: Some(plan), ..
                }) = self.decoded(id)
                else {
                    return Err(self.defect(ctx, "a write with no plan", id));
                };
                let Some(write) = self.program.plan().write(*plan) else {
                    return Err(self.defect(ctx, "a write with no plan", id));
                };
                self.tasks.push(Task::Write {
                    at: id,
                    arms: conflicts,
                    base: self.values.len(),
                });
                self.tasks.push(Task::Invoke {
                    target: Target::Write(*plan),
                    count: write.inputs.len(),
                    at: id,
                    then: None,
                });
                for (index, (site, _)) in write.inputs.iter().enumerate().rev() {
                    self.tasks.push(Task::WriteInput {
                        plan: *plan,
                        index,
                        site: *site,
                    });
                    self.tasks.push(Task::Expr(*site));
                }
            }
            // The database works a stored value out in the write's own
            // statement; nothing reads it here.
            Expr::Stored { .. } => {
                return Err(self.defect(ctx, "a stored value outside its write", id));
            }
            Expr::Build { callee, fields, .. } => {
                let (Callee::Object(_) | Callee::Variant(_)) = callee else {
                    return Err(self.defect(
                        ctx,
                        "a build of something that isn't an object or a variant",
                        id,
                    ));
                };
                self.tasks.push(Task::Build {
                    at: id,
                    count: fields.len(),
                });
                for (index, (_, site)) in fields.iter().enumerate().rev() {
                    self.tasks.push(Task::BuildField {
                        at: id,
                        index,
                        site: *site,
                    });
                    self.tasks.push(Task::Expr(*site));
                }
            }
            Expr::List { items } => {
                self.tasks.push(Task::List {
                    at: id,
                    count: items.len(),
                });
                for item in items.iter().rev() {
                    self.tasks.push(Task::Expr(*item));
                }
            }
            Expr::Binary { op, lhs, rhs } => {
                match op {
                    BinaryOp::And | BinaryOp::Or => self.tasks.push(Task::Logic {
                        op: *op,
                        rhs: *rhs,
                        at: id,
                    }),
                    _ => {
                        self.tasks.push(Task::Apply { op: *op, at: id });
                        self.tasks.push(Task::Expr(*rhs));
                    }
                }
                self.tasks.push(Task::Expr(*lhs));
            }
            Expr::Unary { op, operand } => {
                self.tasks.push(Task::Unary { op: *op, at: id });
                self.tasks.push(Task::Expr(*operand));
            }
            Expr::Match {
                subject,
                outcome,
                arms,
            } => {
                let Some(subject) = subject else {
                    return Err(self.defect(ctx, "a `match` without a subject", id));
                };
                if *outcome {
                    return self.subject(ctx, *subject, Then::Outcome { at: id, arms });
                }
                self.tasks.push(Task::MatchValue { at: id, arms });
                self.tasks.push(Task::Expr(*subject));
            }
            Expr::Attempt { inner, arms } => {
                let Some(inner) = inner else {
                    return Err(self.defect(ctx, "an `attempt` without a call", id));
                };
                self.tasks.push(Task::Attempt {
                    at: id,
                    inner: *inner,
                    arms,
                    base: self.values.len(),
                });
                self.tasks.push(Task::Expr(*inner));
            }
            Expr::Reject { failure, fields } => {
                if failure.is_none() {
                    return Err(self.defect(ctx, "a `reject` naming no failure", id));
                }
                self.tasks.push(Task::Reject {
                    at: id,
                    count: fields.len(),
                });
                for (_, site) in fields.iter().rev() {
                    self.tasks.push(Task::Expr(*site));
                }
            }
            Expr::Propagate { .. } => {
                let handled = self
                    .activations
                    .last()
                    .and_then(|activation| activation.handling.last())
                    .cloned();
                return Err(match handled {
                    Some(failure) => Stop::Failed(Box::new(failure)),
                    None => self.defect(ctx, "`propagate` outside a failure arm", id),
                });
            }
            Expr::Inline { .. } => {
                return Err(self.defect(
                    ctx,
                    "an inline function outside a collection operation",
                    id,
                ));
            }
            // A constant's value: its body, which was checked while
            // compiling and can't fail (area 3).
            Expr::Constant { .. } => {
                let Some(Decoded::Callable(Some(target))) = self.decoded(id) else {
                    return Err(self.defect(ctx, "a constant that has no body", id));
                };
                self.tasks.push(Task::Invoke {
                    target: Target::Callable(*target),
                    count: 0,
                    at: id,
                    then: None,
                });
            }
            Expr::Config { .. } => match self.decoded(id) {
                Some(Decoded::Config(Some(setting))) => match ctx
                    .setting(*setting)
                    .cloned()
                    .or_else(|| self.program.setting(*setting))
                {
                    Some(value) => self.values.push(value),
                    None => {
                        let name = self.program.setting_name(*setting);
                        return Err(self.defect(
                            ctx,
                            &format!(
                                "`config.{name}` has no value in this run: give it a `test:` or `default:` value"
                            ),
                            id,
                        ));
                    }
                },
                _ => return Err(self.defect(ctx, "a setting the checker would have refused", id)),
            },
            // `clock.now` reads the invocation's clock, the one a query's
            // `clock.now` and generated fields read.
            Expr::Now => {
                let now = self.run_effect(ctx, Effect::Now, &[], id)?;
                self.values.push(now);
            }
            Expr::Unchecked => {
                let written = self.written(id);
                return Err(self.not_built_yet(ctx, written, id));
            }
        }
        Ok(Flow::Next)
    }

    /// An operator on two values, at its site.
    fn apply(
        &self,
        ctx: &mut Ctx<'_>,
        op: BinaryOp,
        left: &Value,
        right: &Value,
        at: SiteId,
    ) -> Result<Value, Stop> {
        let ordered = |wanted: fn(std::cmp::Ordering) -> bool| {
            ops::order(left, right).map(|ordering| Value::Bool(wanted(ordering)))
        };
        // An amount scaled by a number rounds by its type's `rounding:`
        // (area 19).
        let rounding = match self.decoded(at) {
            Some(Decoded::Rounding(rounding)) => *rounding,
            _ => jadpo_value::Rounding::HalfEven,
        };
        let result = match op {
            BinaryOp::Equal => Ok(Value::Bool(ops::equal(left, right))),
            BinaryOp::NotEqual => Ok(Value::Bool(!ops::equal(left, right))),
            BinaryOp::Less => ordered(std::cmp::Ordering::is_lt),
            BinaryOp::LessEqual => ordered(std::cmp::Ordering::is_le),
            BinaryOp::Greater => ordered(std::cmp::Ordering::is_gt),
            BinaryOp::GreaterEqual => ordered(std::cmp::Ordering::is_ge),
            BinaryOp::Add => ops::add(left, right),
            BinaryOp::Subtract => ops::subtract(left, right),
            BinaryOp::Multiply => ops::multiply_rounded(left, right, rounding),
            BinaryOp::Divide => ops::divide_rounded(left, right, rounding),
            BinaryOp::Remainder => ops::remainder(left, right),
            BinaryOp::In => ops::contains(right, left)
                .map(Value::Bool)
                .ok_or(jadpo_value::Fault::Shape),
            // A query's condition (area 6).
            BinaryOp::Matches => return Err(self.not_built_yet(ctx, "matches", at)),
            BinaryOp::And | BinaryOp::Or => Err(jadpo_value::Fault::Shape),
        };
        result.map_err(|fault| match fault {
            // Two amounts in different currencies, which the operator's
            // `attempt` maps (area 19).
            jadpo_value::Fault::Broke(rule) => Stop::Invalid {
                rule,
                written: format!("{}:", rule.word()),
                problem: None,
                name: None,
            },
            fault => self.value_fault(ctx, fault, at),
        })
    }

    // --- calls --------------------------------------------------------------

    fn call(
        &mut self,
        ctx: &mut Ctx<'_>,
        id: SiteId,
        callee: &'p Callee,
        args: &'p [Option<SiteId>],
    ) -> Result<Flow, Stop> {
        match callee {
            Callee::Function(_)
            | Callee::Action(_)
            | Callee::Fixture(_)
            | Callee::LibraryFunction(_) => {
                let Some(Decoded::Callable(Some(target))) = self.decoded(id) else {
                    return Err(self.defect(ctx, "a call to a callable that has no body", id));
                };
                self.tasks.push(Task::Invoke {
                    target: Target::Callable(*target),
                    count: args.len(),
                    at: id,
                    then: None,
                });
                self.arguments(args, id);
            }
            // A read the runtime runs from its plan (area 6).
            Callee::Query(_) | Callee::Entity { .. } => {
                let Some(target) = self.effect(id) else {
                    let written = self.written(id);
                    return Err(self.not_built_yet(ctx, written, id));
                };
                self.tasks.push(Task::Invoke {
                    target,
                    count: args.len(),
                    at: id,
                    then: None,
                });
                self.arguments(args, id);
            }
            Callee::Std(op) => self.standard_arguments(*op, args, id),
            Callee::Construct(_) => {
                let [Some(arg)] = args else {
                    return Err(self.defect(ctx, "a construction without one value", id));
                };
                self.tasks.push(Task::Construct(id));
                self.tasks.push(Task::Expr(*arg));
            }
            // A test's `fakes:` scripting a provider operation's answer,
            // given to its connection's fake (area 13; chunk 7.2d).
            Callee::Fake { answer, .. } => {
                self.tasks.push(Task::Fake(id));
                if *answer == jadpo_graph::body::FakeAnswer::Value {
                    let [Some(arg)] = args else {
                        return Err(self.defect(ctx, "a scripted answer without its value", id));
                    };
                    self.tasks.push(Task::Expr(*arg));
                }
            }
            // A caller made from a record, in a test (area 8).
            Callee::Caller(kind) => {
                let [Some(arg)] = args else {
                    return Err(self.defect(ctx, "a caller made without its record", id));
                };
                self.tasks.push(Task::Caller {
                    kind: *kind,
                    at: id,
                });
                self.tasks.push(Task::Expr(*arg));
            }
            // A batch run started, its progress read or a cancel asked for,
            // in the action's transaction (area 12): `batches.each` works
            // out only what its run saves, never its query or its actions,
            // which the run calls later.
            Callee::Batch(op) if op.starts() => {
                let Some(Decoded::Batch { number, sites }) = self.decoded(id) else {
                    return Err(self.defect(ctx, "a batch with no plan", id));
                };
                self.tasks.push(Task::Invoke {
                    target: Target::Batch(*op, Some(*number)),
                    count: sites.len(),
                    at: id,
                    then: None,
                });
                for site in sites.iter().rev() {
                    self.tasks.push(Task::Expr(SiteId(*site)));
                }
            }
            Callee::Batch(op) => {
                self.tasks.push(Task::Invoke {
                    target: Target::Batch(*op, None),
                    count: args.len(),
                    at: id,
                    then: None,
                });
                self.arguments(args, id);
            }
            // A store's put or read: an outside call, before the action's
            // first write (area 16, chunk 6.4b; area 6, chunk 7.4).
            Callee::Files { op, .. } => {
                self.tasks.push(Task::Invoke {
                    target: Target::Files(*op),
                    count: args.len(),
                    at: id,
                    then: None,
                });
                self.arguments(args, id);
            }
            // A provider's operation: an outside call, before the action's
            // first write (area 11, chunk 7.2c; area 6, chunk 7.4).
            Callee::Provider { .. } => {
                self.tasks.push(Task::Invoke {
                    target: Target::Provider,
                    count: args.len(),
                    at: id,
                    then: None,
                });
                self.arguments(args, id);
            }
            // A session started or ended, in the action's transaction.
            Callee::Session(op) => {
                self.tasks.push(Task::Invoke {
                    target: Target::Session(*op),
                    count: args.len(),
                    at: id,
                    then: None,
                });
                self.arguments(args, id);
            }
            // A translation, formatted in the locale given (area 10).
            Callee::Translation(key) => {
                let mut work: Vec<Task<'p>> = Vec::new();
                for arg in args {
                    let Some(arg) = arg else {
                        return Err(self.defect(
                            ctx,
                            "a translation without one of its values",
                            id,
                        ));
                    };
                    work.push(Task::Expr(*arg));
                }
                self.tasks.push(Task::Translate {
                    key,
                    count: work.len(),
                    at: id,
                });
                self.tasks.extend(work.into_iter().rev());
            }
            // A value an email's HTML template writes out (area 16).
            Callee::Escape => {
                let [Some(arg)] = args else {
                    return Err(self.defect(ctx, "an escape of nothing", id));
                };
                self.tasks.push(Task::Escape(id));
                self.tasks.push(Task::Expr(*arg));
            }
            // A prompt rendered as the messages a model is sent (area 21).
            Callee::Prompt(_) => {
                let [Some(arg)] = args else {
                    return Err(self.defect(ctx, "a prompt rendered from nothing", id));
                };
                self.tasks.push(Task::RenderPrompt(id));
                self.tasks.push(Task::Expr(*arg));
            }
            // A schedule run once by a test (area 13).
            Callee::Schedule { .. } => {
                let Some(Decoded::Callable(Some(target))) = self.decoded(id) else {
                    return Err(self.defect(ctx, "a schedule run with no action", id));
                };
                self.tasks.push(Task::Invoke {
                    target: Target::Schedule(*target),
                    count: 0,
                    at: id,
                    then: None,
                });
            }
            // A test's clock moved on (area 13).
            Callee::AdvanceClock => {
                let [Some(arg)] = args else {
                    return Err(self.defect(ctx, "a clock moved by nothing", id));
                };
                self.tasks.push(Task::Invoke {
                    target: Target::AdvanceClock,
                    count: 1,
                    at: id,
                    then: None,
                });
                self.tasks.push(Task::Expr(*arg));
            }
            Callee::Object(_) | Callee::Variant(_) | Callee::Unknown => {
                return Err(self.defect(ctx, "a call to something that can't be called", id));
            }
        }
        Ok(Flow::Next)
    }

    /// The read a call site makes: a query, or an entity's `get`.
    fn effect(&self, at: SiteId) -> Option<Target> {
        match self.decoded(at)? {
            Decoded::Query(Some(query)) => Some(Target::Query(*query)),
            Decoded::Get(Some(entity)) => Some(Target::Get(*entity)),
            _ => None,
        }
    }

    /// The subject of an outcome `match` or of `assert ... fails`, split
    /// in two: its arguments, worked out first, whose failures leave the
    /// enclosing callable as they would anywhere else; and the call itself,
    /// the only outcome the `match` or the `assert` handles (areas 3 and
    /// 13).
    fn subject(
        &mut self,
        ctx: &mut Ctx<'_>,
        subject: SiteId,
        then: Then<'p>,
    ) -> Result<Flow, Stop> {
        match self.site(subject).map(|site| &site.node) {
            Some(Node::Expr(Expr::Call {
                callee:
                    Callee::Function(_)
                    | Callee::Action(_)
                    | Callee::Fixture(_)
                    | Callee::LibraryFunction(_),
                args,
                ..
            })) => {
                let Some(Decoded::Callable(Some(target))) = self.decoded(subject) else {
                    return Err(self.defect(ctx, "a call to a callable that has no body", subject));
                };
                self.tasks.push(Task::Invoke {
                    target: Target::Callable(*target),
                    count: args.len(),
                    at: subject,
                    then: Some(then),
                });
                self.arguments(args, subject);
            }
            Some(Node::Expr(Expr::Call {
                callee: Callee::Schedule { .. },
                ..
            })) => {
                let Some(Decoded::Callable(Some(target))) = self.decoded(subject) else {
                    return Err(self.defect(ctx, "a schedule run with no action", subject));
                };
                self.tasks.push(Task::Invoke {
                    target: Target::Schedule(*target),
                    count: 0,
                    at: subject,
                    then: Some(then),
                });
            }
            Some(Node::Expr(Expr::Call {
                callee: Callee::Query(_) | Callee::Entity { .. },
                args,
                ..
            })) if self.effect(subject).is_some() => {
                let target = self.effect(subject).unwrap_or(Target::Callable(0));
                self.tasks.push(Task::Invoke {
                    target,
                    count: args.len(),
                    at: subject,
                    then: Some(then),
                });
                self.arguments(args, subject);
            }
            // Nothing else an outcome is matched on has arguments that can
            // fail: the checker refuses the rest as subjects.
            _ => {
                self.tasks.push(Task::Catch {
                    base: self.values.len(),
                    remember: None,
                    then,
                });
                self.tasks.push(Task::Expr(subject));
            }
        }
        Ok(Flow::Next)
    }

    /// What a caught call's outcome does: an outcome `match`'s arm, or an
    /// `assert … fails` that holds or doesn't. `Err` passes a failure no
    /// arm handles on.
    fn caught(
        &mut self,
        ctx: &mut Ctx<'_>,
        then: Then<'p>,
        outcome: Result<Value, Box<Failure>>,
    ) -> Result<Flow, Stop> {
        match then {
            Then::Outcome { at, arms } => match outcome {
                Ok(value) => {
                    let Some(arm) = arms.iter().find(|arm| arm.pattern == Pattern::Success) else {
                        return Err(self.defect(ctx, "an outcome `match` without `success`", at));
                    };
                    if let Some(binding) = arm.binding {
                        self.set(ctx, Some(binding), value, at)?;
                    }
                    self.arm(arm);
                    Ok(Flow::Next)
                }
                Err(failure) => {
                    let Some(arm) = arms.iter().find(|arm| {
                        matches!(&arm.pattern, Pattern::Failure(key) if key.as_str() == failure.key)
                    }) else {
                        return Err(Stop::Failed(failure));
                    };
                    if let Some(binding) = arm.binding {
                        self.set(ctx, Some(binding), failure.record(), at)?;
                    }
                    if let Some(activation) = self.activations.last_mut() {
                        activation.handling.push(*failure);
                    }
                    self.tasks.push(Task::PopHandling);
                    self.arm(arm);
                    Ok(Flow::Next)
                }
            },
            Then::Fails { at, key } => {
                let got = match outcome {
                    Err(failure) if failure.key == key => return Ok(Flow::Next),
                    Err(failure) => Got::Failure(failure),
                    Ok(value) => Got::Value(value),
                };
                let expected = match self.decoded(at) {
                    Some(Decoded::AssertFails { expected }) => expected.clone(),
                    _ => key.to_string(),
                };
                Err(Stop::Assertion(ctx.assertion_failed(
                    at.0,
                    AssertionDetail::Fails { expected, got },
                )))
            }
        }
    }

    /// Pushes what runs an arm's body, which gives the `match`'s value.
    fn arm(&mut self, arm: &'p Arm) {
        match &arm.body {
            ArmBody::Value(value) => self.tasks.push(Task::Expr(*value)),
            ArmBody::Block(block) => {
                self.tasks.push(Task::PushUnit);
                self.tasks.push(Task::Block { block, next: 0 });
            }
            ArmBody::Empty => self.values.push(Value::Unit),
        }
    }

    /// Whether `value` is the case `pattern` (`decoded` by the plan) names.
    fn matches(
        &self,
        ctx: &mut Ctx<'_>,
        pattern: &Pattern,
        decoded: Option<&ArmPattern>,
        value: &Value,
        at: SiteId,
    ) -> Result<bool, Stop> {
        Ok(match pattern {
            Pattern::Variant(_) => match (decoded, value) {
                (Some(ArmPattern::Variant { ty, name }), Value::Variant(variant)) => {
                    &*variant.ty == ty.as_str() && &*variant.name == name.as_str()
                }
                _ => false,
            },
            Pattern::True => *value == Value::Bool(true),
            Pattern::False => *value == Value::Bool(false),
            Pattern::Some => *value != Value::None,
            Pattern::None => *value == Value::None,
            Pattern::Wildcard => true,
            Pattern::Literal(_) => match decoded {
                Some(ArmPattern::Literal(Some(constant))) => {
                    ops::equal(value, &self.program.constant(*constant))
                }
                _ => {
                    return Err(self.defect(ctx, "a literal the checker would have refused", at));
                }
            },
            Pattern::Success | Pattern::Failure(_) | Pattern::Unknown => {
                return Err(self.defect(ctx, "an outcome pattern on a value", at));
            }
        })
    }

    /// Pushes what works out a standard-library operation's arguments, in
    /// its parameters' order: an inline function's body runs per item
    /// once the operation does.
    fn standard_arguments(&mut self, op: Op, args: &'p [Option<SiteId>], id: SiteId) {
        let mut inline = None;
        let mut work: Vec<Task<'p>> = Vec::new();
        let mut count = 0;
        for (param, arg) in op.operation().params.iter().zip(args) {
            let Some(arg) = arg else {
                if param.optional {
                    work.push(Task::PushNone);
                    count += 1;
                    continue;
                }
                work.push(Task::Defect("an operation without one of its values", id));
                break;
            };
            // A key is given by its setting, never read as a value.
            if matches!(param.kind, ParamKind::Builtin(builtin) if builtin.is_key()) {
                match (self.site(*arg).map(|site| &site.node), self.decoded(*arg)) {
                    (
                        Some(Node::Expr(Expr::Config { .. })),
                        Some(Decoded::Config(Some(setting))),
                    ) => {
                        work.push(Task::PushKey(*setting));
                        count += 1;
                    }
                    _ => {
                        work.push(Task::Defect("a key that isn't read from its setting", id));
                        break;
                    }
                }
                continue;
            }
            if param.kind == ParamKind::Function {
                match self.site(*arg).map(|site| &site.node) {
                    Some(Node::Expr(Expr::Inline {
                        params,
                        body: Some(body),
                    })) if !params.is_empty() => inline = Some((params.as_slice(), *body)),
                    _ => {
                        work.push(Task::Defect(
                            "an operation's function that isn't inline",
                            id,
                        ));
                        break;
                    }
                }
            } else {
                work.push(Task::Expr(*arg));
                count += 1;
            }
        }
        // The value `text.parse` reads is then built as the type `as:`
        // names, checking its rules (area 19).
        if op == Op::TextParse {
            self.tasks.push(Task::Construct(id));
        }
        self.tasks.push(Task::Std {
            op,
            count,
            inline,
            at: id,
        });
        self.tasks.extend(work.into_iter().rev());
    }

    /// A standard-library operation, run by its one implementation in
    /// `jadpo-value`; an inline function runs in this frame, per item.
    fn standard(
        &mut self,
        ctx: &mut Ctx<'_>,
        op: Op,
        values: &[Value],
        inline: Option<(&'p [LocalId], SiteId)>,
        at: SiteId,
    ) -> Result<Value, Stop> {
        // `random`, `crypto` and JSON in memory read the invocation or the
        // plan's codec for their call, so the runtime runs them (chunk
        // 7.0b).
        if effects::is_effect(op) {
            let json = match self.decoded(at) {
                Some(Decoded::Json(number)) => Some(*number),
                _ => None,
            };
            return self.run_effect(ctx, Effect::Std { op, json }, values, at);
        }
        let mut stopped: Option<Stop> = None;
        let locales = self.program.plan().locales();
        // An inline function's expressions give a value per item, not one
        // an assertion's report could show.
        if inline.is_some() {
            ctx.pause_recording();
        }
        let result = {
            let mut each = |given: &[Value]| -> Option<Value> {
                let (params, body) = inline?;
                for (param, value) in params.iter().zip(given) {
                    if let Some(slot) = self
                        .locals()
                        .and_then(|locals| locals.get_mut(param.0 as usize))
                    {
                        *slot = value.clone();
                    }
                }
                match self.nested(ctx, body) {
                    Ok(value) => Some(value),
                    Err(stop) => {
                        stopped = Some(stop);
                        None
                    }
                }
            };
            jadpo_value::run(op, values, locales, &mut each)
        };
        if inline.is_some() {
            ctx.resume_recording();
        }
        match (result, stopped) {
            (Ok(value), _) => Ok(value),
            (Err(_), Some(stop)) => Err(stop),
            // A temporal condition its `attempt` maps, as a construction's
            // broken rule (area 10).
            (Err(jadpo_value::Fault::Broke(rule)), None) => Err(Stop::Invalid {
                rule,
                written: format!("{}:", rule.word()),
                problem: None,
                name: None,
            }),
            (Err(fault), None) => Err(self.value_fault(ctx, fault, at)),
        }
    }
}
