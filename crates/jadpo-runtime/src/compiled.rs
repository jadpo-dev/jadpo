//! What every compiled program shares (roadmap chunk 8.1f;
//! docs/compiler-runtime.md §2.5, "Compiled code"). `jadpo-codegen`
//! writes a project's callables as Rust functions over `jadpo_value::Value`,
//! holding control flow and numbers only; everything else a body does goes
//! through the runtime's entry points the interpreter uses, and what is
//! common to every compiled program is here:
//!
//! - [`Program`]: one thread's compiled program, the runtime's
//!   [`Executor`], made from the [`Code`] the generated crate holds (its
//!   plan's canonical bytes, its frame table and its callables by number),
//!   the plan decoded once per process and loaded on each thread as the
//!   interpreter's is.
//! - [`invoke`] and the action body it hands the boundary: a call into an
//!   action or fixture, or any callable by number.
//! - [`Stop`], what ends a body early, and the small functions each site
//!   calls ([`binary`], [`field`], [`build`], [`write`] and the rest),
//!   which turn what the runtime and `jadpo-value` give into the stop the
//!   interpreter's walker raises at the same site, with the same frames.
//!
//! None of it is generic, so a generated crate monomorphises nothing of
//! the runtime's (§2.5, "Keeping rustc time down").

use std::cell::Cell;
use std::rc::Rc;
use std::sync::{Arc, OnceLock};

use jadpo_std::{Op, RuleKey, RuntimePlan, Unreadable};
use jadpo_value::shape::Shape;
use jadpo_value::{Rounding, Validator, Value};

use crate::calls::{self, Called};
use crate::effects::{self, Effect, Refused, Standard};
use crate::operators::{Operator, operate};
use crate::{
    ActionBody, AssertionDetail, CallablePlace, Ctx, Executor, Failure, FaultKind, Frame, Got,
    Isolation, Loaded, LocalFuture, Outcome, Places, SitePlace, Started, StoreOp, Stored,
};

/// What ends a compiled body before its last statement: what the
/// interpreter's walker stops with, but a `return`, which is Rust's own.
#[derive(Debug)]
pub enum Stop {
    /// A declared failure on its way to whatever handles it.
    Failed(Box<Failure>),
    /// A fault on its way out.
    Fault(Box<crate::Fault>),
    /// A test's failed assertion.
    Assertion(Box<crate::Assertion>),
    /// A construction's value broke a rule, for the `attempt` whose block
    /// maps it: the rule's key, the rule as written for the report, a
    /// `json.read`'s first problem, and the name of a rule across an
    /// object's fields.
    Invalid {
        rule: RuleKey,
        written: String,
        problem: Option<Value>,
        name: Option<String>,
    },
    /// The store refused a write with the rule its `conflict:` arm at
    /// `arm` maps, for the write's site to raise that arm's failure with
    /// the refusal as its cause (area 6).
    Refused { arm: u32, cause: Box<Failure> },
    /// A `return` inside an inline function, or inside a block that
    /// catches stops, on its way out of what it is in: the callable's end
    /// ([`left`]) gives its value, as the interpreter's `return` unwinds to
    /// its frame's end.
    Returned(Value),
}

/// What a compiled body, or one of its sites, gives.
pub type Ran = Result<Value, Stop>;

/// A function, a library function or a constant's body: called as it is,
/// never waiting (area 3).
pub type Function = fn(&mut Ctx<'_>, &Program, Vec<Value>) -> Ran;

/// An action, fixture, test or seed: it may wait wherever its body pauses.
pub type Body = for<'a, 'p> fn(&'a mut Ctx<'p>, &'a Program, Vec<Value>) -> LocalFuture<'a, Ran>;

/// A file helper's write's row (area 12, "Batches"; chunk 6.4b2), compiled
/// as a function of its own: the inline function's body, in the action
/// that started the run, worked out with each local it reads given its
/// value, by the local's number.
pub type Row = fn(&mut Ctx<'_>, &Program, Vec<(u32, Value)>) -> Ran;

/// A compiled callable, as a generated program's table gives it.
#[derive(Clone, Copy, Debug)]
pub enum Callable {
    Function(Function),
    /// An action or a fixture, run through the action boundary.
    Action(Body),
    /// A test or a seed, run as it is called.
    Body(Body),
}

/// One callable's place in the frame table (§2.5, "Frames"): how a frame
/// names it, its file and line, and each site's line, start, end and the
/// anchor a report hangs its value at, by site number.
#[derive(Clone, Copy, Debug)]
pub struct Place {
    pub label: &'static str,
    pub file: &'static str,
    pub line: u32,
    pub sites: &'static [[u32; 4]],
}

/// What a generated crate holds: its plan's canonical bytes, its frame
/// table, its callables by number, its file writes' rows by the callable
/// and site of their inline function's body, each route's typed entry by
/// its number (chunk 8.1p), and where the process keeps what it decodes
/// from them once.
#[derive(Debug)]
pub struct Code {
    pub plan: &'static [u8],
    pub places: &'static [Place],
    pub callable: fn(u32) -> Option<Callable>,
    pub row: fn(u32, u32) -> Option<Row>,
    pub routes: crate::http::Entries,
    pub shared: &'static Shared,
}

/// What a process decodes of a [`Code`] once, on the first thread that
/// makes a [`Program`] of it: a generated crate's `static`.
#[derive(Debug, Default)]
pub struct Shared(OnceLock<Result<Arc<Process>, Unreadable>>);

impl Shared {
    pub const fn new() -> Shared {
        Shared(OnceLock::new())
    }
}

/// A program's plan, decoded; its validators, compiled; and its frame
/// table, as the runtime reads it: what every thread shares.
#[derive(Debug)]
struct Process {
    plan: Arc<RuntimePlan>,
    validators: Arc<[Validator]>,
    places: Places,
}

impl Process {
    fn of(code: &Code) -> Result<Process, Unreadable> {
        let plan = Arc::new(RuntimePlan::decode(code.plan)?);
        let validators = Loaded::compile(&plan);
        let mut places = Places::new();
        for place in code.places {
            places.push(CallablePlace {
                label: place.label.to_string(),
                file: place.file.to_string(),
                line: place.line,
                sites: place
                    .sites
                    .iter()
                    .map(|[line, start, end, anchor]| SitePlace {
                        line: *line,
                        start: *start,
                        end: *end,
                        anchor: *anchor,
                    })
                    .collect(),
            });
        }
        Ok(Process {
            plan,
            validators,
            places,
        })
    }
}

/// How many calls, loop turns and inline-function calls a thread's
/// compiled code makes between looks at the request's deadline: the
/// interpreter's interval, in tasks (area 7, "Time limits").
const DEADLINE_EVERY: u32 = 4096;

/// A compiled program on one thread: the runtime's [`Executor`].
#[derive(Debug)]
pub struct Program {
    code: &'static Code,
    process: Arc<Process>,
    /// The plan on this thread: its shapes made, its validators shared.
    loaded: Loaded,
    /// The plan's constants as values, by number.
    constants: Vec<Value>,
    started: Rc<Started>,
    /// Calls made since the deadline was last looked at.
    steps: Cell<u32>,
}

impl Program {
    /// `code` on this thread, its plan decoded the first time any thread
    /// asks, reading the settings a test run does until it is given the
    /// ones an application started with ([`Program::with_config`]), as
    /// the interpreter's program does.
    pub fn new(code: &'static Code) -> Result<Program, Unreadable> {
        let process = match code
            .shared
            .0
            .get_or_init(|| Process::of(code).map(Arc::new))
        {
            Ok(process) => Arc::clone(process),
            Err(unreadable) => return Err(unreadable.clone()),
        };
        let loaded = Loaded::sharing(Arc::clone(&process.plan), Arc::clone(&process.validators));
        let constants = process
            .plan
            .sites
            .constants
            .iter()
            .map(Value::constant)
            .collect();
        let values = crate::config::load(
            &process.plan.config,
            &process.validators,
            &|_: &str| None,
            crate::config::Mode::Test,
            &|_: &mut [u8]| false,
        )
        .ok();
        let started = Rc::new(Started::new(
            &loaded.plan(),
            values.as_ref(),
            crate::http::address::Target::Native,
        ));
        Ok(Program {
            code,
            process,
            loaded,
            constants,
            started,
            steps: Cell::new(0),
        })
    }

    /// The same program reading `values` as its settings (area 10).
    pub fn with_config(self, values: &crate::config::ConfigValues) -> Program {
        self.with_config_on(values, crate::http::address::Target::Native)
    }

    /// The same program reading `values` as its settings on `target`,
    /// which decides how a request's client address is chosen (area 10,
    /// "Client addresses"): a Workers deployable's (chunk 8.2d).
    pub fn with_config_on(
        mut self,
        values: &crate::config::ConfigValues,
        target: crate::http::address::Target,
    ) -> Program {
        self.started = Rc::new(Started::new(&self.loaded.plan(), Some(values), target));
        self
    }

    /// Its plan, as decoded.
    pub fn runtime(&self) -> &RuntimePlan {
        &self.process.plan
    }

    /// Constant `n`, as this thread holds it.
    pub fn constant(&self, n: u32) -> Value {
        self.constants
            .get(n as usize)
            .cloned()
            .unwrap_or(Value::Unit)
    }

    /// Validator `n`; one with no rules when the plan has none of that
    /// number, which it always has.
    pub fn validator(&self, n: u32) -> &Validator {
        static NONE: OnceLock<Validator> = OnceLock::new();
        self.process
            .validators
            .get(n as usize)
            .unwrap_or_else(|| NONE.get_or_init(|| Validator::new(None, Vec::new())))
    }

    /// Every validator, compiled once for the process: what a deployable
    /// checks its settings with as it starts (`config::load`, chunk 8.1o).
    pub fn validators(&self) -> &[Validator] {
        &self.process.validators
    }

    fn shape(&self, n: u32) -> Option<&Shape> {
        self.loaded.shapes().get(n as usize)
    }

    fn callable(&self, n: u32) -> Option<Callable> {
        (self.code.callable)(n)
    }

    /// What the HTTP component answers its routes with, every route read
    /// and written by the codec walk, as the interpreter reads and writes
    /// them: what the differential harness compares the typed entries
    /// ([`Executor::app`]) with.
    pub fn walked_app(&self) -> crate::http::App<'_> {
        self.loaded.plan().app()
    }

    fn effects(&self) -> effects::Effects<'_> {
        let plan = &self.process.plan;
        effects::Effects {
            json: self.loaded.plan().json,
            event_numbers: &plan.json.events,
            events: &plan.events,
            config: &plan.config,
        }
    }

    /// Counts a call, a loop's turn or an inline function's call: once
    /// every [`DEADLINE_EVERY`], whether the request's deadline has passed,
    /// or whether to give the thread back.
    fn pace(&self, cx: &mut Ctx<'_>) -> Pace {
        let steps = self.steps.get() + 1;
        if steps < DEADLINE_EVERY {
            self.steps.set(steps);
            return Pace::Running;
        }
        self.steps.set(0);
        if cx.out_of_time() {
            Pace::Passed(Stop::Fault(
                cx.fault(FaultKind::DeadlineExceeded, Frame::ENTRY),
            ))
        } else if cx.has_deadline() {
            Pace::Yield
        } else {
            Pace::Running
        }
    }
}

/// What a look at the deadline found: carry on, give the thread back
/// first (a body that waits does; one that doesn't carries on), or stop
/// with the deadline's fault.
#[derive(Debug)]
pub enum Pace {
    Running,
    Yield,
    Passed(Stop),
}

/// A `for` loop's next turn (area 7, "Time limits"; chunk 8.4a): counted
/// with the calls, so work that loops without calling anything is stopped
/// at the deadline as the interpreter's walker stops it, every
/// [`DEADLINE_EVERY`] tasks. A body that waits gives the thread back on
/// [`Pace::Yield`]; a function's loop carries on.
pub fn pace(cx: &mut Ctx<'_>, p: &Program) -> Pace {
    p.pace(cx)
}

impl Executor for Program {
    fn places(&self) -> &Places {
        &self.process.places
    }

    fn plan(&self) -> crate::Plan<'_> {
        self.loaded.plan()
    }

    /// Each route read and written by its typed entry (chunk 8.1p), in a
    /// deployable as in a route test (chunk 8.4c).
    fn app(&self) -> crate::http::App<'_> {
        crate::http::App {
            entries: Some(self.code.routes),
            ..self.loaded.plan().app()
        }
    }

    fn started(&self) -> Option<Rc<Started>> {
        Some(Rc::clone(&self.started))
    }

    fn run<'a, 'p: 'a>(
        &'a self,
        ctx: &'a mut Ctx<'p>,
        callable: u32,
        args: Vec<Value>,
    ) -> LocalFuture<'a, Outcome> {
        Box::pin(
            async move { outcome(invoke(ctx, self, callable, args, Frame::ENTRY, false).await) },
        )
    }

    /// A provider package's own function, run at a call's site (chunk
    /// 7.2c2), described as the interpreter describes it.
    fn function(
        &self,
        ctx: &mut Ctx<'_>,
        callable: u32,
        args: Vec<Value>,
        at: u32,
    ) -> Result<Value, String> {
        let ran = match self.callable(callable) {
            Some(Callable::Function(run)) => match enter(ctx, self, callable, at) {
                Ok(()) => {
                    let ran = run(ctx, self, args);
                    left(ctx, ran)
                }
                Err(stop) => Err(stop),
            },
            Some(Callable::Action(_) | Callable::Body(_)) => {
                return Err("a fault, `defect`".to_string());
            }
            None => Err(defect(ctx, "a call to a callable that has no body", at)),
        };
        match ran {
            Ok(value) => Ok(value),
            Err(Stop::Failed(failure)) => Err(format!("`{}`", failure.code)),
            Err(Stop::Fault(fault)) => Err(format!("a fault, `{}`", fault.kind.classification())),
            Err(_) => Err("a fault, `defect`".to_string()),
        }
    }

    /// A file helper's write's row for `item` (chunk 6.4b2): its compiled
    /// function, in a frame of the action whose body holds it, given the
    /// item and what the run saved for the locals it reads, as the
    /// interpreter runs it.
    fn row<'a, 'p: 'a>(
        &'a self,
        ctx: &'a mut Ctx<'p>,
        row: &'a jadpo_std::batch::BatchRow,
        item: Value,
        saved: &'a [Value],
    ) -> LocalFuture<'a, Outcome> {
        Box::pin(async move {
            let locals = crate::batches::row_locals(row, item, saved);
            let ran = match (self.code.row)(row.callable, row.body) {
                Some(run) => match ctx.enter(row.callable, row.body) {
                    Ok(()) => {
                        let ran = run(ctx, self, locals);
                        ctx.leave();
                        ran
                    }
                    Err(fault) => Err(Stop::Fault(fault)),
                },
                None => Err(defect(
                    ctx,
                    "a row in a callable that has no body",
                    row.body,
                )),
            };
            outcome(ran)
        })
    }
}

/// How a call ended, as the runtime reports it.
pub fn outcome(ran: Ran) -> Outcome {
    let defect = |what: &str| {
        Outcome::Fault(Box::new(crate::Fault {
            kind: FaultKind::Defect(what.into()),
            frames: Vec::new(),
        }))
    };
    match ran {
        Ok(value) => Outcome::Value(value),
        Err(Stop::Failed(failure)) => Outcome::Failed(failure),
        Err(Stop::Fault(fault)) => Outcome::Fault(fault),
        Err(Stop::Assertion(assertion)) => Outcome::Assertion(assertion),
        Err(Stop::Invalid { .. }) => defect("a construction failed outside an `attempt`"),
        Err(Stop::Refused { .. }) => defect("a refused write outside its own site"),
        Err(Stop::Returned(value)) => Outcome::Value(value),
    }
}

/// An outcome as what stops the caller.
fn result(outcome: Outcome) -> Ran {
    match outcome {
        Outcome::Value(value) => Ok(value),
        Outcome::Failed(failure) => Err(Stop::Failed(failure)),
        Outcome::Fault(fault) => Err(Stop::Fault(fault)),
        Outcome::Assertion(assertion) => Err(Stop::Assertion(assertion)),
    }
}

// --- calls -------------------------------------------------------------------

/// Calls callable `callee` with `args` from site `at` of the frame on top
/// of `cx`: an action or a fixture through the runtime's action boundary
/// (`caught` saying the caller handles its failure), anything else as it
/// is. Its future is boxed, so a body that calls itself through an action
/// has a size.
pub fn invoke<'a, 'p>(
    cx: &'a mut Ctx<'p>,
    p: &'a Program,
    callee: u32,
    args: Vec<Value>,
    at: u32,
    caught: bool,
) -> LocalFuture<'a, Ran> {
    Box::pin(async move {
        if let Some(Callable::Action(_)) = p.callable(callee) {
            let mut action = Action {
                program: p,
                callee,
                args,
                at,
                caught,
            };
            return result(crate::action(cx, &mut action).await);
        }
        body(cx, p, callee, args, at).await
    })
}

/// Runs callable `callee`'s body with `args`, entered from site `at`.
async fn body(cx: &mut Ctx<'_>, p: &Program, callee: u32, args: Vec<Value>, at: u32) -> Ran {
    match p.callable(callee) {
        Some(Callable::Function(run)) => {
            enter(cx, p, callee, at)?;
            let ran = run(cx, p, args);
            left(cx, ran)
        }
        Some(Callable::Action(run) | Callable::Body(run)) => {
            match p.pace(cx) {
                Pace::Running => {}
                Pace::Yield => cx.host().yield_now().await,
                Pace::Passed(stop) => return Err(stop),
            }
            cx.enter(callee, at).map_err(Stop::Fault)?;
            let ran = run(cx, p, args).await;
            left(cx, ran)
        }
        None => Err(defect(cx, "a call to a callable that has no body", at)),
    }
}

/// Enters callable `callee` from site `at`: its frame on `cx`, the
/// recursion limit and the request's deadline checked.
pub fn enter(cx: &mut Ctx<'_>, p: &Program, callee: u32, at: u32) -> Result<(), Stop> {
    if let Pace::Passed(stop) = p.pace(cx) {
        return Err(stop);
    }
    cx.enter(callee, at).map_err(Stop::Fault)
}

/// Leaves the callable on top of `cx`, which gave `ran`: a broken rule no
/// `attempt` caught is a defect at its entry, as the interpreter has it,
/// and a `return` that left as a stop its value.
pub fn left(cx: &mut Ctx<'_>, ran: Ran) -> Ran {
    let ran = match ran {
        Err(Stop::Invalid { .. }) => Err(defect(
            cx,
            "a construction failed outside an `attempt`",
            Frame::ENTRY,
        )),
        Err(Stop::Returned(value)) => Ok(value),
        other => other,
    };
    cx.leave();
    ran
}

/// An action's or fixture's body as the boundary runs it, once or, after
/// a concurrent change, again from the start.
struct Action<'a> {
    program: &'a Program,
    callee: u32,
    args: Vec<Value>,
    at: u32,
    caught: bool,
}

impl Action<'_> {
    fn plan(&self) -> Option<&jadpo_std::compiled::ActionPlan> {
        self.program.process.plan.actions.get(&self.callee)
    }
}

impl<'p> ActionBody<'p> for Action<'_> {
    fn isolation(&self) -> Isolation {
        self.plan()
            .map_or(Isolation::RepeatableRead, |plan| plan.isolation)
    }

    fn caught(&self) -> bool {
        self.caught
    }

    fn gate(&self) -> Option<&jadpo_std::policy::Gate> {
        self.plan().and_then(|plan| plan.gate.as_ref())
    }

    fn site(&self) -> u32 {
        self.at
    }

    fn callable(&self) -> Option<u32> {
        Some(self.callee)
    }

    fn outbox(&self) -> Option<&str> {
        self.plan().and_then(|plan| plan.outbox.as_deref())
    }

    fn record(&self) -> Option<crate::steps::Spec<'_>> {
        let plan = self.plan()?;
        let runtime = &self.program.process.plan;
        Some(crate::steps::Spec {
            plan: plan.record.as_ref()?,
            action: plan.key.as_str(),
            build: runtime.build.as_str(),
            setting: runtime.steps_key,
        })
    }

    fn run<'c>(&'c mut self, ctx: &'c mut Ctx<'p>) -> LocalFuture<'c, Outcome> {
        Box::pin(async move {
            outcome(body(ctx, self.program, self.callee, self.args.clone(), self.at).await)
        })
    }
}

/// The arguments a callable's table entry is given, one per parameter.
#[derive(Debug)]
pub struct Args(std::vec::IntoIter<Value>);

impl Args {
    pub fn new(args: Vec<Value>) -> Args {
        Args(args.into_iter())
    }

    /// The next parameter's value; `Unit` past the last, which a call by
    /// number with too few never gives.
    pub fn take(&mut self) -> Value {
        self.0.next().unwrap_or(Value::Unit)
    }
}

// --- stopping ----------------------------------------------------------------

/// A fault of `kind` at site `at` of the frame on top of `cx`.
pub fn fault(cx: &mut Ctx<'_>, kind: FaultKind, at: u32) -> Stop {
    Stop::Fault(cx.fault(kind, at))
}

/// A defect the checker rules out, at site `at`.
pub fn defect(cx: &mut Ctx<'_>, what: &str, at: u32) -> Stop {
    fault(cx, FaultKind::Defect(what.to_string()), at)
}

/// Something the language declares that a later chunk builds, as
/// written, at site `at`: what the interpreter faults with there too.
pub fn not_built_yet(cx: &mut Ctx<'_>, what: &str, at: u32) -> Stop {
    fault(cx, FaultKind::NotBuiltYet(what.to_string()), at)
}

fn value_fault(cx: &mut Ctx<'_>, fault: jadpo_value::Fault, at: u32) -> Stop {
    Stop::Fault(cx.fault(fault.into(), at))
}

/// A rule `invalid` broke, for the `attempt` that maps it: `field`
/// names the field a build gave it to.
fn broken(invalid: &jadpo_value::Invalid, field: Option<&str>) -> Stop {
    Stop::Invalid {
        rule: invalid.key(),
        written: invalid.broken(field),
        problem: None,
        name: invalid.name().map(str::to_string),
    }
}

// --- values ------------------------------------------------------------------

/// `receiver.name`, a record's field or a variant's data's.
pub fn field(cx: &mut Ctx<'_>, receiver: &Value, name: &str, at: u32) -> Ran {
    let found = match receiver {
        Value::Record(record) => record.get(name),
        Value::Variant(variant) => variant.get(name),
        _ => None,
    };
    match found {
        Some(value) => Ok(value.clone()),
        None => Err(defect(cx, "a field read from a value that hasn't it", at)),
    }
}

/// `left op right` at site `at`, through the runtime's one implementation
/// of the operators; an amount scaled by a number rounds by `rounding`.
pub fn binary(
    cx: &mut Ctx<'_>,
    op: Operator,
    left: &Value,
    right: &Value,
    rounding: Rounding,
    at: u32,
) -> Ran {
    operate(op, left, right, rounding).map_err(|fault| match fault {
        // Two amounts in different currencies, which the operator's
        // `attempt` maps (area 19).
        jadpo_value::Fault::Broke(rule) => Stop::Invalid {
            rule,
            written: format!("{}:", rule.word()),
            problem: None,
            name: None,
        },
        fault => value_fault(cx, fault, at),
    })
}

/// Where a `Bool` is read, for the defect when the value isn't one.
#[derive(Clone, Copy, Debug)]
pub enum Truth {
    /// An `if`'s condition.
    Condition,
    /// `and` or `or`.
    Logic,
    /// `not`.
    Not,
}

/// The `Bool` `value` holds, read as `of` reads it, at site `at`.
pub fn truth(cx: &mut Ctx<'_>, value: &Value, of: Truth, at: u32) -> Result<bool, Stop> {
    match value {
        Value::Bool(holds) => Ok(*holds),
        _ => Err(defect(
            cx,
            match of {
                Truth::Condition => "a condition that isn't a `Bool`",
                Truth::Logic => "`and` or `or` on something that isn't a `Bool`",
                Truth::Not => "`not` on something that isn't a `Bool`",
            },
            at,
        )),
    }
}

/// `-value` at site `at`.
pub fn negate(cx: &mut Ctx<'_>, value: &Value, at: u32) -> Ran {
    jadpo_value::ops::negate(value).map_err(|fault| value_fault(cx, fault, at))
}

/// A field's value as a build at site `at` gives it: kept as its type
/// keeps it (a decimal at its scale), and checked against its rules when
/// the build can fail, the failure named by the field `name`.
pub fn given(
    cx: &mut Ctx<'_>,
    p: &Program,
    validator: u32,
    value: Value,
    fallible: bool,
    name: &str,
    at: u32,
) -> Ran {
    let validator = p.validator(validator);
    let Some(value) = validator.store(value) else {
        return Err(fault(cx, FaultKind::DecimalOverflow, at));
    };
    if fallible && let Err(invalid) = validator.check(&value) {
        return Err(broken(&invalid, Some(name)));
    }
    Ok(value)
}

/// An object or a variant of shape `shape` built at site `at` from
/// `values`, each going to the slot `slots` gives it; then, when `across`
/// names a validator whose rules across the fields can break here, those
/// rules checked.
pub fn build(
    cx: &mut Ctx<'_>,
    p: &Program,
    shape: u32,
    slots: &[Option<u32>],
    values: Vec<Value>,
    across: Option<u32>,
    at: u32,
) -> Ran {
    let built = p
        .shape(shape)
        .and_then(|shape| shape.build(slots.iter().copied().zip(values)));
    let Some(value) = built else {
        return Err(defect(cx, "a build without one of its fields", at));
    };
    if let Some(validator) = across
        && let Err(invalid) = p.validator(validator).check(&value)
    {
        return Err(broken(&invalid, None));
    }
    Ok(value)
}

/// Whether `value` is variant `name` of the enum or tagged union whose
/// identity key is `ty`: a value `match`'s arm.
pub fn is_variant(value: &Value, ty: &str, name: &str) -> bool {
    matches!(value, Value::Variant(variant) if &*variant.ty == ty && &*variant.name == name)
}

/// The items a `for` at site `at` walks: a list's in order, a set's in
/// the canonical order it is held in (area 2, "Set order").
pub fn items(cx: &mut Ctx<'_>, value: &Value, at: u32) -> Result<Rc<[Value]>, Stop> {
    match value {
        Value::List(items) | Value::Set(items) => Ok(Rc::clone(items)),
        _ => Err(defect(
            cx,
            "a loop over something that isn't a list or a set",
            at,
        )),
    }
}

/// A list written where a list, a set or (with no items) a map goes.
pub fn list(items: Vec<Value>, set: bool, map: bool) -> Value {
    if map && items.is_empty() {
        Value::Map(Rc::default())
    } else if set {
        Value::set(items)
    } else {
        Value::list(items)
    }
}

/// A value a write's input at site `at` stores, kept as its field's type
/// keeps it, by `validator`.
pub fn kept(cx: &mut Ctx<'_>, p: &Program, validator: u32, value: Value, at: u32) -> Ran {
    match p.validator(validator).store(value) {
        Some(value) => Ok(value),
        None => Err(fault(cx, FaultKind::DecimalOverflow, at)),
    }
}

/// `config.<name>`, setting `n`: a test's own value, else the one the
/// program started with.
pub fn setting(cx: &mut Ctx<'_>, p: &Program, n: u32, at: u32) -> Ran {
    if let Some(value) = cx.setting(n) {
        return Ok(value.clone());
    }
    let name = p
        .runtime()
        .config
        .fields
        .get(n as usize)
        .map(|field| field.name.clone())
        .unwrap_or_default();
    Err(defect(
        cx,
        &format!("`config.{name}` has no value in this run: give it a `test:` or `default:` value"),
        at,
    ))
}

/// What the runtime refused an operation at site `at` with, as what stops
/// the body: a condition for its call's `attempt` to map, or a fault.
fn refused(cx: &mut Ctx<'_>, refused: Refused, at: u32) -> Stop {
    match refused {
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
        Refused::Fault(fault) => value_fault(cx, fault, at),
        Refused::Defect(what) => defect(cx, &what, at),
        Refused::TooDeep => Stop::Fault(cx.fault(FaultKind::RecursionLimit, at)),
    }
}

/// An operation with effects, run by the runtime's one entry point for
/// them at site `at`.
fn effect(cx: &mut Ctx<'_>, p: &Program, effect: Effect<'_>, values: &[Value], at: u32) -> Ran {
    let effects = p.effects();
    effects::run(cx, &effects, effect, values).map_err(|why| refused(cx, why, at))
}

/// `clock.now` at site `at`: the invocation's clock.
pub fn now(cx: &mut Ctx<'_>, p: &Program, at: u32) -> Ran {
    effect(cx, p, Effect::Now, &[], at)
}

/// `emit` at site `at` of `event`, an event of the type whose identity key
/// is `key`, kept in the running action's outbox (area 12).
pub fn emit(cx: &mut Ctx<'_>, p: &Program, key: &str, event: Value, at: u32) -> Result<(), Stop> {
    effect(cx, p, Effect::Emit { event: key }, &[event], at).map(|_| ())
}

/// `email.sent` in a test, at site `at` (area 16).
pub fn sent_emails(cx: &mut Ctx<'_>, p: &Program, at: u32) -> Ran {
    effect(cx, p, Effect::SentEmails, &[], at)
}

// --- the standard library -----------------------------------------------------

/// Standard operation `op` with `values`, in its parameters' order, at
/// site `at`, through the runtime's one entry point for them, which the
/// interpreter calls too: `json` is the number of the type its site reads
/// or writes in the plan's JSON table.
pub fn standard(
    cx: &mut Ctx<'_>,
    p: &Program,
    op: Op,
    json: Option<u32>,
    values: &[Value],
    at: u32,
) -> Ran {
    let effects = p.effects();
    let locales = p.runtime().locales.as_ref();
    effects::standard(cx, &effects, locales, Standard { op, json }, values, None)
        .map_err(|why| refused(cx, why, at))
}

/// A collection operation `op` with `values` at site `at`, its inline
/// function `each` run per item, each call counted toward the deadline's
/// next look; a stop in it ends the operation with that stop.
pub fn standard_each(
    cx: &mut Ctx<'_>,
    p: &Program,
    op: Op,
    values: &[Value],
    each: &mut dyn FnMut(&mut Ctx<'_>, &[Value]) -> Ran,
    at: u32,
) -> Ran {
    let effects = p.effects();
    let locales = p.runtime().locales.as_ref();
    let mut stopped = None;
    // Each call counted, so an inline function run over a long list stops
    // at the deadline, as the interpreter's walker stops one; it runs
    // inside an operation, so it never gives the thread back.
    let mut given = |cx: &mut Ctx<'_>, items: &[Value]| {
        let ran = match p.pace(cx) {
            Pace::Passed(stop) => Err(stop),
            Pace::Running | Pace::Yield => each(cx, items),
        };
        match ran {
            Ok(value) => Some(value),
            Err(stop) => {
                stopped = Some(stop);
                None
            }
        }
    };
    let result = effects::standard(
        cx,
        &effects,
        locales,
        Standard { op, json: None },
        values,
        Some(&mut given),
    );
    match (result, stopped) {
        (Ok(value), _) => Ok(value),
        (Err(_), Some(stop)) => Err(stop),
        (Err(why), None) => Err(refused(cx, why, at)),
    }
}

/// `Type(value)` at site `at`, or what `text.parse` read built as the type
/// `as:` names: `value` kept as type `validator` keeps it (a list made a
/// set when `set`), and checked against its rules only when `fallible`,
/// a broken one for the `attempt` that maps it. One the checker proved is
/// never checked again (area 2).
pub fn construct(
    cx: &mut Ctx<'_>,
    p: &Program,
    validator: u32,
    fallible: bool,
    set: bool,
    value: Value,
    at: u32,
) -> Ran {
    // A set built from a list drops its repeats, in the canonical order,
    // before its lengths are counted.
    let value = match value {
        Value::List(items) if set => Value::set(items.to_vec()),
        value => value,
    };
    let validator = p.validator(validator);
    let Some(value) = validator.store(value) else {
        return Err(fault(cx, FaultKind::DecimalOverflow, at));
    };
    if fallible && let Err(invalid) = validator.check(&value) {
        return Err(broken(&invalid, None));
    }
    Ok(value)
}

/// The failure numbered `failure`, its fields `values` placed by `slots`,
/// for site `at` to raise ([`raised`]); the defect `missing` when the plan
/// has no such failure or a field is left out.
pub fn failure(
    cx: &mut Ctx<'_>,
    p: &Program,
    failure: u32,
    slots: &[Option<u32>],
    values: Vec<Value>,
    missing: &str,
    at: u32,
) -> Result<Failure, Stop> {
    p.runtime()
        .failures
        .get(failure as usize)
        .and_then(|plan| Failure::of(plan, slots, values))
        .ok_or_else(|| defect(cx, missing, at))
}

/// The failure an `attempt`'s arm maps a broken rule to, numbered
/// `failure`, its fields `values` placed by `slots`, and, when the arm
/// names its `problem: DecodeProblem` field at `problem_slot`, `json.read`'s
/// first problem there (area 19); the defect when the plan has no such
/// failure.
#[allow(clippy::too_many_arguments)]
pub fn mapped(
    cx: &mut Ctx<'_>,
    p: &Program,
    failure: u32,
    slots: &[Option<u32>],
    mut values: Vec<Value>,
    problem_slot: Option<u32>,
    problem: Option<Value>,
    at: u32,
) -> Result<Failure, Stop> {
    let mut slots = slots.to_vec();
    if let (Some(slot), Some(problem)) = (problem_slot, problem) {
        slots.push(Some(slot));
        values.push(problem);
    }
    self::failure(
        cx,
        p,
        failure,
        &slots,
        values,
        "an arm naming something that isn't a failure",
        at,
    )
}

/// `failure`, mapped from the rule written as `written`, raised at the
/// site of the construction that broke it, `inner`.
pub fn broke(cx: &mut Ctx<'_>, mut failure: Failure, written: String, inner: u32) -> Stop {
    failure.broken = Some(written);
    Stop::Failed(cx.fail(failure, inner))
}

/// The arm of an `attempt` that maps rule `rule`, or the rule across an
/// object's fields called `name`, among `arms`, each arm's rule and name
/// in the order written: its own, or `invalid:`.
pub fn rule_arm(
    arms: &[(Option<RuleKey>, Option<&str>)],
    rule: RuleKey,
    name: Option<&str>,
) -> Option<u32> {
    jadpo_std::arm_for(arms.iter().copied(), rule, name).and_then(|at| u32::try_from(at).ok())
}

/// `failure` raised at site `at`, with `cause` as its cause.
pub fn raised(
    cx: &mut Ctx<'_>,
    mut failure: Failure,
    cause: Option<Box<Failure>>,
    at: u32,
) -> Stop {
    failure.cause = cause;
    Stop::Failed(cx.fail(failure, at))
}

// --- reads, writes and requests ----------------------------------------------

/// What a read or a write gave, as what stops its site.
fn stored(stored: Stored) -> Ran {
    match stored {
        Stored::Value(value) => Ok(value),
        Stored::Failed(failure) => Err(Stop::Failed(failure)),
        Stored::Conflict { arm, cause } => Err(Stop::Refused { arm, cause }),
        Stored::Fault(fault) => Err(Stop::Fault(fault)),
    }
}

/// The store's entry point for `op` with `args`, from site `at`.
async fn store(cx: &mut Ctx<'_>, p: &Program, op: StoreOp<'_>, args: &[Value], at: u32) -> Ran {
    let failures = &p.process.plan.failures;
    stored(crate::store(cx, op, failures, p.loaded.plan().json, args, at).await)
}

/// Query `n`'s read with `args`, from site `at`.
pub async fn query(cx: &mut Ctx<'_>, p: &Program, n: u32, args: &[Value], at: u32) -> Ran {
    match p.process.plan.sites.queries.get(n as usize) {
        Some(read) => store(cx, p, StoreOp::Read(read), args, at).await,
        None => Err(defect(cx, "a query with no plan", at)),
    }
}

/// Entity `n`'s `get` with `args`, from site `at`.
pub async fn get(cx: &mut Ctx<'_>, p: &Program, n: u32, args: &[Value], at: u32) -> Ran {
    match p.process.plan.sites.gets.get(n as usize) {
        Some(read) => store(cx, p, StoreOp::Read(read), args, at).await,
        None => Err(defect(cx, "an entity with no plan", at)),
    }
}

/// History `n` with `args`, the record's `id` and its `after:`, from site
/// `at` (area 16; chunk B11).
pub async fn history(cx: &mut Ctx<'_>, p: &Program, n: u32, args: &[Value], at: u32) -> Ran {
    match p.process.plan.sites.histories.get(n as usize) {
        Some(history) => store(cx, p, StoreOp::History(history), args, at).await,
        None => Err(defect(cx, "a history with no plan", at)),
    }
}

/// Write `n` with its inputs, `args`, from site `at`: a refusal its site's
/// `conflict:` maps is [`Stop::Refused`], for the site to raise its arm's
/// failure.
pub async fn write(cx: &mut Ctx<'_>, p: &Program, n: u32, args: &[Value], at: u32) -> Ran {
    match p.process.plan.sites.writes.get(n as usize) {
        Some(write) => store(cx, p, StoreOp::Write(write), args, at).await,
        None => Err(defect(cx, "a write with no plan", at)),
    }
}

// --- identity, batches and the clock -------------------------------------------

/// `Principal.user(record)` or `Principal.service(record)` in a test,
/// fixture or seed, at site `at` (area 8): the caller made from its
/// record, through the runtime as the interpreter makes it.
pub fn caller(
    cx: &mut Ctx<'_>,
    p: &Program,
    kind: jadpo_std::http::PrincipalKind,
    record: &Value,
    at: u32,
) -> Ran {
    match crate::http::caller_from(p, kind, record) {
        Some(caller) => Ok(caller),
        None => Err(defect(
            cx,
            "a caller made from something that isn't its record",
            at,
        )),
    }
}

/// A session's, an API token's, a service key's, a magic link's or a
/// test passkey's operation `op` with `args`, at site `at` (area 8),
/// through the runtime's one entry point for them.
pub async fn session(
    cx: &mut Ctx<'_>,
    p: &Program,
    op: jadpo_std::http::SessionOp,
    args: &[Value],
    at: u32,
) -> Ran {
    stored(crate::http::session(p, cx, op, args, at).await)
}

/// Batch operation `op` with `args`, at site `at` (area 12), through the
/// runtime's one entry point for them: a call that starts a run gives
/// its run's plan by its place among the program's, `place`.
pub async fn batch(
    cx: &mut Ctx<'_>,
    p: &Program,
    op: jadpo_std::batch::BatchOp,
    place: Option<u32>,
    args: &[Value],
    at: u32,
) -> Ran {
    let plan = place.and_then(|place| p.runtime().batches.get(place as usize));
    let batches = p.loaded.plan().batches();
    stored(crate::batches::call(cx, batches, op, plan, args, at).await)
}

/// `clock.advance(by)` in a test, at site `at` (area 13, "Controlling
/// time"): the test's clock moved on, never back.
#[cfg(feature = "testing")]
pub fn advance(cx: &mut Ctx<'_>, by: Value, at: u32) -> Ran {
    crate::test_steps::advance_clock(cx, &[by], at)
        .map(|()| Value::Unit)
        .map_err(Stop::Fault)
}

/// A test's run of a schedule whose action is `action`, `<schedule>.run()`
/// at site `at` (area 13, "Kinds of test"): the action called with
/// nothing as the system with no tenant, as the tick runs it, at the
/// test's clock; `caught` when the test handles its failure. It gives
/// nothing.
#[cfg(feature = "testing")]
pub async fn schedule(cx: &mut Ctx<'_>, p: &Program, action: u32, at: u32, caught: bool) -> Ran {
    let saved = crate::schedules::test_run(cx);
    let ran = invoke(cx, p, action, Vec::new(), at, caught).await;
    crate::schedules::test_run_ended(cx, saved);
    ran.map(|_| Value::Unit)
}

/// A route test's request through route `route` (area 13): `args` are
/// the route's placeholders and input, then, as `given` says (`keyed`,
/// `signed`, `way`, `strong`), the `Idempotency-Key` it sends, the caller
/// it signs in as, and how that caller signed in (chunk A10).
pub async fn request(
    cx: &mut Ctx<'_>,
    p: &Program,
    route: u32,
    args: Vec<Value>,
    given: [bool; 4],
) -> Ran {
    let sent = crate::http::RequestValues::split(&args, given);
    crate::http::route_test(&p.app(), p, cx, route, sent)
        .await
        .map_err(Stop::Fault)
}

/// After a test's step at site `at`, which gave `ran`: what it emitted
/// delivered before the test's next statement (area 13, "Caches and
/// events"; chunk 6.11), and what its clock brought due when `advanced`.
/// `Err` ends the test with what a delivery that failed for good gave.
#[cfg(feature = "testing")]
pub async fn step(
    cx: &mut Ctx<'_>,
    p: &Program,
    advanced: bool,
    at: u32,
    ran: Ran,
) -> Result<Ran, Ran> {
    if matches!(ran, Err(Stop::Fault(_))) {
        return Ok(ran);
    }
    match crate::test_steps::after_step(p, cx, advanced, at).await {
        Ok(()) => Ok(ran),
        Err(ended) => Err(result(ended)),
    }
}

// --- translations, providers, files and models ------------------------------------

/// What one of the runtime's entry points for a body's sites of areas 10,
/// 11, 16 and 21 gave ([`crate::calls`], which the interpreter calls too),
/// as what stops the site (chunk 8.1i).
fn called(called: Called) -> Ran {
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

/// `translations.<message>(locale, …)`, translation `key`, with `values`,
/// the locale first, at site `at` (area 10).
pub fn translate(cx: &mut Ctx<'_>, p: &Program, key: &str, values: &[Value], at: u32) -> Ran {
    called(calls::translate(cx, p.loaded.plan(), key, values, at))
}

/// `prompts.render(prompt)` at site `at` (area 21): the prompt at place
/// `entry` among the plan's, each field it writes by its type's number in
/// the JSON table, `fields`.
pub fn render_prompt(
    cx: &mut Ctx<'_>,
    p: &Program,
    entry: u32,
    fields: &[u32],
    prompt: &Value,
    at: u32,
) -> Ran {
    called(calls::render_prompt(
        cx,
        p.loaded.plan(),
        entry,
        fields,
        prompt,
        at,
    ))
}

/// A value an email's HTML template writes out at site `at` (area 16).
pub fn escape(cx: &mut Ctx<'_>, value: &Value, at: u32) -> Ran {
    called(calls::escape(cx, value, at))
}

/// `store.put(bytes, type)` or `store.read(file)`, `op`, of the store
/// that is connection `connection`, with `args`, at site `at` (area 16).
pub async fn files(
    cx: &mut Ctx<'_>,
    connection: &str,
    op: jadpo_std::files::FilesOp,
    args: &[Value],
    at: u32,
) -> Ran {
    called(calls::files(cx, connection, op, args, at).await)
}

/// The provider call whose site is outside call `call` in the plan, if it
/// is one.
fn provider_site(p: &Program, call: Option<u32>) -> Option<&jadpo_std::provider::CallSite> {
    match p.runtime().sites.calls.get(call? as usize)? {
        jadpo_std::compiled::CallPlan::Provider(site) => Some(site),
        _ => None,
    }
}

/// A provider operation's call, outside call `call` in the plan, with
/// `input`, at site `at` (area 11), its package's functions run by the
/// compiled program.
pub async fn provider(
    cx: &mut Ctx<'_>,
    p: &Program,
    call: Option<u32>,
    input: Option<Value>,
    at: u32,
) -> Ran {
    static NONE: OnceLock<jadpo_std::provider::CallSite> = OnceLock::new();
    let site = provider_site(p, call).unwrap_or_else(|| NONE.get_or_init(Default::default));
    called(calls::provider(cx, p.loaded.plan(), p, site, input, at).await)
}

/// Whether failure `key` of provider call `call` is one its `attempt`
/// maps by its code (area 11).
pub fn provider_maps(p: &Program, call: Option<u32>, key: &str) -> bool {
    provider_site(p, call).is_some_and(|site| calls::provider_maps(p.loaded.plan(), site, key))
}

/// The arm among `named` that maps `failure`, a provider operation's or a
/// package action's, by its code, the call `called` at site `inner`; the
/// fault there when none does (area 11).
pub fn mapped_code(
    cx: &mut Ctx<'_>,
    named: &[Option<&str>],
    failure: &Failure,
    called: (&str, &str),
    inner: u32,
) -> Result<u32, Stop> {
    calls::mapped(cx, named, failure, called, inner).map_err(Stop::Fault)
}

/// An `ai` call, outside call `call` in the plan, with `given`, at site
/// `at` (area 21); `embed` for `ai.embed` or `ai.embed_all`, which names
/// its fault apart when the program asks no model.
pub async fn ai(
    cx: &mut Ctx<'_>,
    p: &Program,
    call: Option<u32>,
    embed: bool,
    given: Value,
    at: u32,
) -> Ran {
    let site = call.and_then(|call| p.runtime().sites.calls.get(call as usize));
    let asked = match site.and_then(calls::Asked::of) {
        Some(asked) => asked,
        None if embed => calls::Asked::Embed(None),
        None => calls::Asked::Generate(None),
    };
    called(calls::ai(cx, p.loaded.plan(), asked, given, at).await)
}

/// A package's action `callee` called through connection `connection`
/// with `args`, from site `at` (area 11, chunk 7.6): its own operations go
/// through that connection; `caught` when the caller handles its failure.
pub async fn package(
    cx: &mut Ctx<'_>,
    p: &Program,
    connection: &str,
    callee: u32,
    args: Vec<Value>,
    at: u32,
    caught: bool,
) -> Ran {
    cx.enter_package(connection);
    let ran = invoke(cx, p, callee, args, at, caught).await;
    cx.leave_package();
    ran
}

/// A test's `fakes:` at site `at` scripting operation `operation` of
/// connection `connection` to answer `answer` (area 13).
#[cfg(feature = "testing")]
pub fn fake(
    cx: &mut Ctx<'_>,
    p: &Program,
    connection: &str,
    operation: &str,
    answer: calls::FakeAnswer,
    at: u32,
) -> Ran {
    called(calls::fake(
        cx,
        p.loaded.plan(),
        p,
        (connection, operation),
        answer,
        at,
    ))
}

/// A test's `fakes: { ai.<prompt>: }` at site `at` scripting prompt
/// `prompt`'s next answer, `answer` (area 21).
#[cfg(feature = "testing")]
pub fn ai_fake(
    cx: &mut Ctx<'_>,
    p: &Program,
    prompt: &str,
    answer: calls::AiAnswer<'_>,
    at: u32,
) -> Ran {
    called(calls::ai_fake(cx, p.loaded.plan(), prompt, answer, at))
}

/// `ai.<prompt>.requests` at site `at` (area 21).
#[cfg(feature = "testing")]
pub fn ai_requests(cx: &mut Ctx<'_>, p: &Program, prompt: &str, at: u32) -> Ran {
    called(calls::ai_requests(cx, p.loaded.plan(), prompt, at))
}

// --- assertions --------------------------------------------------------------

/// `assert value` at site `at`.
pub fn assert_true(cx: &mut Ctx<'_>, value: &Value, at: u32) -> Result<(), Stop> {
    if *value == Value::Bool(true) {
        return Ok(());
    }
    Err(Stop::Assertion(cx.assertion_failed(
        at,
        AssertionDetail::False { comparison: None },
    )))
}

/// `assert left op right` at site `at`, the comparison's own site being
/// `compared`.
pub fn assert_compare(
    cx: &mut Ctx<'_>,
    op: Operator,
    left: Value,
    right: Value,
    compared: u32,
    at: u32,
) -> Result<(), Stop> {
    if binary(cx, op, &left, &right, Rounding::HalfEven, compared)? == Value::Bool(true) {
        return Ok(());
    }
    Err(Stop::Assertion(cx.assertion_failed(
        at,
        AssertionDetail::False {
            comparison: Some((op.symbol(), left, right)),
        },
    )))
}

/// `assert call(…) fails Failure` at site `at`, the call having given
/// `ran`: it holds when the call failed with the failure whose identity
/// key is `key`; any other failure, or a value, fails the assertion,
/// naming `expected`, the failure's name now.
pub fn assert_fails(
    cx: &mut Ctx<'_>,
    ran: Ran,
    key: &str,
    expected: &str,
    at: u32,
) -> Result<(), Stop> {
    let got = match ran {
        Err(Stop::Failed(failure)) if failure.key == key => return Ok(()),
        Err(Stop::Failed(failure)) => Got::Failure(failure),
        Ok(value) => Got::Value(value),
        Err(stop) => return Err(stop),
    };
    Err(Stop::Assertion(cx.assertion_failed(
        at,
        AssertionDetail::Fails {
            expected: expected.to_string(),
            got,
        },
    )))
}

/// The end of an `assert` that gave `held`: its recording stops, and a
/// failed assertion takes the parts it recorded.
pub fn assertion_end(cx: &mut Ctx<'_>, held: Result<(), Stop>) -> Result<(), Stop> {
    let parts = cx.assertion_end();
    match held {
        Err(Stop::Assertion(mut assertion)) => {
            assertion.parts = parts;
            Err(Stop::Assertion(assertion))
        }
        other => other,
    }
}
