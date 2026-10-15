//! A checked project ready to run, in two parts. [`Prepared`] is what
//! every thread shares behind an `Arc`: the plan (`jadpo-plan`), each type's
//! rules compiled into a validator once, and the frame table. [`Program`]
//! is one thread's view of it, with what can't cross threads because the
//! runtime's values are `Rc`-shared (§2.6): the plan's constants and shapes
//! decoded into values. The test runner prepares a project once and makes
//! a `Program` on each thread it runs tests on.

use std::fmt;
use std::sync::Arc;

use jadpo_db::{Db, Project};
use jadpo_diag::Diagnostic;
use jadpo_graph::Key;
use jadpo_plan::{ConstId, FailureId, Plan, ShapeId, ValidatorId};
use jadpo_runtime::{
    CallablePlace, Ctx, Env, Executor, Failure, FaultKind, Frame, LocalFuture, MemoryHost, Outcome,
    Places, Report, SitePlace,
};
use jadpo_value::{Validator, Value};

use crate::drive;
use crate::values::{Codec, Shape};

/// A checked project, shared by every thread that runs it.
#[derive(Debug)]
pub struct Prepared {
    plan: Plan,
    /// The plan's validators, compiled, by number.
    validators: Vec<Validator>,
    places: Places,
}

impl Prepared {
    /// Reads `project` if it checks without errors; its errors otherwise.
    /// An API record out of date doesn't stop the program running: it is a
    /// promise to clients, which `jadpo check` and CI hold to (area 7).
    pub fn load(db: &dyn Db, project: Project) -> Result<Arc<Prepared>, Vec<Diagnostic>> {
        let errors: Vec<Diagnostic> = jadpo_check::check(db, project)
            .into_iter()
            .filter(jadpo_check::blocks_running)
            .collect();
        if !errors.is_empty() {
            return Err(errors);
        }
        Ok(Prepared::checked(db, project))
    }

    /// Reads `project`, which the caller has checked has no errors.
    pub fn checked(db: &dyn Db, project: Project) -> Arc<Prepared> {
        let plan = Plan::build(db, project);
        let validators = plan
            .types()
            .validators()
            .iter()
            .map(|plan| Validator::new(plan.builtin, plan.rules.clone()))
            .collect();
        let mut places = Places::new();
        for callable in plan.callables() {
            places.push(CallablePlace {
                label: callable.label.clone(),
                file: callable.file.clone(),
                line: callable.line,
                sites: callable
                    .body
                    .sites
                    .iter()
                    .zip(&callable.sites)
                    .map(|(site, decoded)| SitePlace {
                        line: site.line,
                        start: site.start,
                        end: site.end,
                        anchor: decoded.anchor,
                    })
                    .collect(),
            });
        }
        Arc::new(Prepared {
            plan,
            validators,
            places,
        })
    }

    pub fn plan(&self) -> &Plan {
        &self.plan
    }

    /// Where every frame is.
    pub fn places(&self) -> &Places {
        &self.places
    }
}

/// A prepared project on one thread, ready to run its callables.
#[derive(Debug)]
pub struct Program {
    prepared: Arc<Prepared>,
    /// The plan's constants as values, by number.
    constants: Vec<Value>,
    /// The plan's shapes with their names as values hold them, by number.
    shapes: Vec<Shape>,
    /// Whether constructions the checker proved are checked again as they
    /// run ([`Program::with_proof_checks`]).
    proof_checks: bool,
}

/// Why a call couldn't start.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CallError {
    /// No callable with a body has this key or name.
    Unknown(String),
    /// It takes a different number of values.
    Arity {
        callable: String,
        expected: usize,
        given: usize,
    },
}

impl fmt::Display for CallError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CallError::Unknown(name) => {
                write!(f, "Nothing called `{name}` has a body that can run")
            }
            CallError::Arity {
                callable,
                expected,
                given,
            } => write!(f, "`{callable}` takes {expected} values, not {given}"),
        }
    }
}

impl Program {
    /// This thread's view of `prepared`.
    pub fn new(prepared: Arc<Prepared>) -> Program {
        let types = prepared.plan.types();
        let constants = prepared
            .plan
            .constants()
            .iter()
            .map(Value::constant)
            .collect();
        let shapes = types.shapes().iter().map(Shape::new).collect();
        Program {
            prepared,
            constants,
            shapes,
            proof_checks: false,
        }
    }

    /// Reads every callable of `project` and prepares it on this thread. A
    /// project with errors doesn't run: its errors come back instead.
    pub fn load(db: &dyn Db, project: Project) -> Result<Program, Vec<Diagnostic>> {
        Prepared::load(db, project).map(Program::new)
    }

    /// The same program, checking again every construction the checker
    /// proved follows its type's rules (`Type(value)`, an object's fields
    /// and a list written into a type with lengths): one that breaks a
    /// rule ends in a defect naming it. Compiled code never checks a proven
    /// construction, so a hole in a proof would pass an invalid value
    /// silently; tests run with this on to find one (chunk 2.7).
    pub fn with_proof_checks(mut self) -> Program {
        self.proof_checks = true;
        self
    }

    pub fn prepared(&self) -> &Arc<Prepared> {
        &self.prepared
    }

    pub fn plan(&self) -> &Plan {
        &self.prepared.plan
    }

    /// Builds values of the project's types from their parts.
    pub fn codec(&self) -> Codec<'_> {
        Codec {
            types: self.plan().types(),
        }
    }

    /// Every callable with a body, by key: functions, actions, fixtures,
    /// library functions, tests and seeds.
    pub fn keys(&self) -> impl Iterator<Item = &Key> + '_ {
        self.plan().callables().iter().map(|callable| &callable.key)
    }

    /// Arguments to probe callable `name` with: for each parameter, values
    /// its type allows at and near the edges of its rules, combined, at
    /// most `limit` sets in all. `None` when a parameter's type isn't a
    /// number, text, a UUID or a flag, or a list or set of them, which
    /// this doesn't sample.
    pub fn probe_arguments(&self, name: &str, limit: usize) -> Option<Vec<Vec<Value>>> {
        let callable = self.plan().callable(self.plan().find(name)?)?;
        let types = self.plan().types();
        let choices: Vec<Vec<Value>> = callable
            .body
            .params
            .iter()
            .map(|param| {
                let ty = &callable.body.locals.get(param.0 as usize)?.ty;
                let values = crate::probe::values(types, ty)?;
                (!values.is_empty()).then_some(values)
            })
            .collect::<Option<_>>()?;
        Some(crate::probe::combine(&choices, limit))
    }

    /// Where every frame is.
    pub fn places(&self) -> &Places {
        &self.prepared.places
    }

    /// A file's text, by its project-relative path.
    pub fn text(&self, path: &str) -> Option<&str> {
        self.plan().text(path)
    }

    /// Runs the callable `name` (a key's text or a name, as
    /// [`Plan::find`] reads it) with `args`, one per parameter, taken as
    /// values of the parameters' types, on a host with no store and the
    /// default environment: its value, the failure that left it, a fault,
    /// or a test's failed assertion.
    pub fn call(&self, name: &str, args: Vec<Value>) -> Result<Outcome, CallError> {
        let unknown = || CallError::Unknown(name.to_string());
        let id = self.plan().find(name).ok_or_else(unknown)?;
        let callable = self.plan().callable(id).ok_or_else(unknown)?;
        if callable.body.params.len() != args.len() {
            return Err(CallError::Arity {
                callable: callable.label.clone(),
                expected: callable.body.params.len(),
                given: args.len(),
            });
        }
        let host = MemoryHost::new();
        let mut ctx = Ctx::new(self.places(), &host, Env::default());
        let ran = jadpo_runtime::block_on(self.run(&mut ctx, id, args));
        Ok(ran.unwrap_or_else(|_| {
            Outcome::Fault(ctx.fault(
                FaultKind::Defect("a wait nothing would end".into()),
                Frame::ENTRY,
            ))
        }))
    }

    /// What the runtime's HTTP component answers this program's routes
    /// with (area 7): the route table and its codecs, with this thread's
    /// validators and shapes.
    pub fn app(&self) -> jadpo_runtime::http::App<'_> {
        let table = self.plan().routes();
        jadpo_runtime::http::App {
            table,
            codecs: jadpo_value::json::Codecs {
                codecs: &table.codecs,
                validators: &self.prepared.validators,
                shapes: &self.shapes,
            },
            failures: self.plan().types().failures(),
        }
    }

    /// The report a person reads for an outcome that isn't a value.
    pub fn report(&self, outcome: &Outcome) -> Option<Report> {
        Report::of(outcome, self.places(), self.plan().files())
    }

    // --- for the machine ----------------------------------------------------

    pub(crate) fn proof_checks(&self) -> bool {
        self.proof_checks
    }

    /// Validator `id`; one with no rules when the plan has none of that
    /// number, which it always has.
    pub(crate) fn validator(&self, id: ValidatorId) -> &Validator {
        static NONE: std::sync::OnceLock<Validator> = std::sync::OnceLock::new();
        self.prepared
            .validators
            .get(id.0 as usize)
            .unwrap_or_else(|| NONE.get_or_init(|| Validator::new(None, Vec::new())))
    }

    pub(crate) fn constant(&self, id: ConstId) -> Value {
        self.constants
            .get(id.0 as usize)
            .cloned()
            .unwrap_or(Value::Unit)
    }

    pub(crate) fn shape(&self, id: ShapeId) -> Option<&Shape> {
        self.shapes.get(id.0 as usize)
    }

    /// Failure `id`, with the fields given in `values` placed by `slots`,
    /// ready to raise; `None` when a field is missing.
    pub(crate) fn failure(
        &self,
        id: FailureId,
        slots: &[Option<u32>],
        values: Vec<Value>,
    ) -> Option<Failure> {
        Failure::of(self.plan().types().failure(id)?, slots, values)
    }
}

impl Executor for Program {
    fn places(&self) -> &Places {
        Program::places(self)
    }

    fn callable(&self, key: &str) -> Option<u32> {
        self.plan().find(key)
    }

    fn run<'a, 'p: 'a>(
        &'a self,
        ctx: &'a mut Ctx<'p>,
        callable: u32,
        args: Vec<Value>,
    ) -> LocalFuture<'a, Outcome> {
        Box::pin(async move {
            drive::outcome(drive::invoke(self, ctx, callable, args, Frame::ENTRY, false).await)
        })
    }
}
