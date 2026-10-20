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

    /// Reads `project` as [`Prepared::load`] does, with the latest
    /// migration's backfills that `jadpo migrate apply` works out in Jadpo
    /// (chunk 5.6b2), each a callable of its own.
    pub fn with_backfills(db: &dyn Db, project: Project) -> Result<Arc<Prepared>, Vec<Diagnostic>> {
        let errors: Vec<Diagnostic> = jadpo_check::check(db, project)
            .into_iter()
            .filter(jadpo_check::blocks_running)
            .collect();
        if !errors.is_empty() {
            return Err(errors);
        }
        Ok(Prepared::checked_with_backfills(db, project))
    }

    /// [`Prepared::with_backfills`], for a project the caller has checked
    /// has no errors.
    pub fn checked_with_backfills(db: &dyn Db, project: Project) -> Arc<Prepared> {
        Prepared::from_plan(Plan::with_backfills(db, project))
    }

    /// Reads `project`, which the caller has checked has no errors.
    pub fn checked(db: &dyn Db, project: Project) -> Arc<Prepared> {
        Prepared::from_plan(Plan::build(db, project))
    }

    fn from_plan(plan: Plan) -> Arc<Prepared> {
        let validators = plan
            .types()
            .validators()
            .iter()
            .map(|plan| {
                Validator::new(plan.builtin, plan.rules.clone()).with_across(plan.across.clone())
            })
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

    /// The plan's validators, compiled, by number.
    pub fn validators(&self) -> &[Validator] {
        &self.validators
    }

    /// How long a purge waits before it deletes a hidden record, in
    /// milliseconds: its `after:`, or the value `values` hold for the
    /// setting it reads (area 10, "Retention periods"). `None` when the
    /// setting holds no duration.
    pub fn purge_after(
        &self,
        values: &jadpo_runtime::config::ConfigValues,
        purge: &jadpo_std::write::PurgePlan,
    ) -> Option<i64> {
        match purge.after_setting {
            None => Some(purge.after),
            Some(at) => {
                jadpo_runtime::config::duration_ms(self.plan.config(), &self.validators, values, at)
            }
        }
    }

    /// How each setting stands against `variables` (area 10): set,
    /// defaulted, missing or invalid. Never holds a value.
    pub fn config_standing(
        &self,
        variables: &dyn jadpo_runtime::config::Variables,
        mode: jadpo_runtime::config::Mode,
        random: &dyn Fn(&mut [u8]) -> bool,
    ) -> Vec<jadpo_runtime::config::Standing> {
        jadpo_runtime::config::check(
            self.plan.config(),
            &self.validators,
            variables,
            mode,
            random,
        )
    }

    /// The settings the application starts with, read from `variables`:
    /// every one checked against its type, or each that stops it starting
    /// (area 10, "Checking values before the app starts").
    pub fn config(
        &self,
        variables: &dyn jadpo_runtime::config::Variables,
        mode: jadpo_runtime::config::Mode,
        random: &dyn Fn(&mut [u8]) -> bool,
    ) -> Result<jadpo_runtime::config::ConfigValues, Vec<jadpo_runtime::config::StartProblem>> {
        jadpo_runtime::config::load(
            self.plan.config(),
            &self.validators,
            variables,
            mode,
            random,
        )
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
    /// The settings' values, decoded, by the setting's number in the plan:
    /// `None` for one no value reached ([`Program::with_config`]).
    settings: Vec<Option<Value>>,
    /// The key settings' keys: what a `crypto` call's `key:
    /// config.<name>` gives it (area 19).
    keys: jadpo_runtime::effects::Keys,
    /// Each provider connection's credential, held apart as the keys are,
    /// since code never reads one (area 11; chunk 7.2c).
    credentials: jadpo_runtime::provider::Credentials,
    /// What signing in reads from them: the signing keys and the browser
    /// session's origin (area 8).
    auth: jadpo_runtime::http::signed::AuthSettings,
    /// How a request's client address is chosen (area 10, "Client
    /// addresses").
    client: jadpo_runtime::http::address::ClientAddress,
    /// How long each purge waits, in milliseconds, in the order of the
    /// plan's purges: its `after:`, or the value of the setting it reads
    /// (chunk 6.2c); `None` when that holds no duration.
    purge_after: Vec<Option<i64>>,
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
        // Until it is given the settings an application started with, a
        // program reads the ones a test run does: each `test:` value and
        // default.
        let values = prepared
            .config(
                &|_: &str| None,
                jadpo_runtime::config::Mode::Test,
                &|_: &mut [u8]| false,
            )
            .ok();
        let settings = values
            .as_ref()
            .map(|values| Self::decoded(&prepared, values))
            .unwrap_or_default();
        let keys = values
            .as_ref()
            .map(|values| jadpo_runtime::effects::Keys::read(prepared.plan.config(), values))
            .unwrap_or_default();
        let credentials = values
            .as_ref()
            .map(|values| {
                jadpo_runtime::provider::Credentials::read(prepared.plan.providers(), values)
            })
            .unwrap_or_default();
        let auth = values
            .as_ref()
            .map(|values| Self::auth(&prepared, values))
            .unwrap_or_default();
        let client = values
            .as_ref()
            .map(|values| Self::client(&prepared, values))
            .unwrap_or_default();
        let purge_after = Self::purge_periods(&prepared, values.as_ref());
        Program {
            prepared,
            constants,
            shapes,
            proof_checks: false,
            settings,
            keys,
            credentials,
            auth,
            client,
            purge_after,
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

    /// The same program reading `values` as its settings (area 10): what
    /// the application started with, or a test run's.
    pub fn with_config(mut self, values: &jadpo_runtime::config::ConfigValues) -> Program {
        self.settings = Self::decoded(&self.prepared, values);
        self.keys = jadpo_runtime::effects::Keys::read(self.prepared.plan.config(), values);
        self.credentials =
            jadpo_runtime::provider::Credentials::read(self.prepared.plan.providers(), values);
        self.auth = Self::auth(&self.prepared, values);
        self.client = Self::client(&self.prepared, values);
        self.purge_after = Self::purge_periods(&self.prepared, Some(values));
        self
    }

    /// How long each of the plan's purges waits with `values` as the
    /// settings, in milliseconds.
    fn purge_periods(
        prepared: &Prepared,
        values: Option<&jadpo_runtime::config::ConfigValues>,
    ) -> Vec<Option<i64>> {
        prepared
            .plan
            .purges()
            .iter()
            .map(|purge| match (purge.after_setting, values) {
                (None, _) => Some(purge.after),
                (Some(_), Some(values)) => prepared.purge_after(values, purge),
                (Some(_), None) => None,
            })
            .collect()
    }

    /// How the address of a client is chosen with `values` as the
    /// settings: what `client_address:` says on the native target, or the
    /// connection's own address.
    fn client(
        prepared: &Prepared,
        values: &jadpo_runtime::config::ConfigValues,
    ) -> jadpo_runtime::http::address::ClientAddress {
        jadpo_runtime::http::address::ClientAddress::read(
            prepared.plan.routes().client_address.as_ref(),
            values,
            jadpo_runtime::http::address::Target::Native,
        )
        .unwrap_or_default()
    }

    /// What signing in reads from `values`.
    fn auth(
        prepared: &Prepared,
        values: &jadpo_runtime::config::ConfigValues,
    ) -> jadpo_runtime::http::signed::AuthSettings {
        jadpo_runtime::http::signed::AuthSettings::read(
            prepared.plan.routes().identity.as_ref(),
            values,
        )
    }

    /// The settings `values` hold, decoded, by their numbers.
    fn decoded(
        prepared: &Prepared,
        values: &jadpo_runtime::config::ConfigValues,
    ) -> Vec<Option<Value>> {
        let config = prepared.plan.config();
        config
            .fields
            .iter()
            .enumerate()
            .map(|(at, field)| {
                // Code never reads a secret, so none is held here.
                if field.secret {
                    return None;
                }
                let text = values.text_at(at as u32)?;
                jadpo_runtime::config::decode(config, &prepared.validators, field, text).ok()
            })
            .collect()
    }

    /// The value of setting number `at`.
    pub(crate) fn setting(&self, at: u32) -> Option<Value> {
        self.settings.get(at as usize).cloned().flatten()
    }

    /// The name of setting number `at`.
    pub(crate) fn setting_name(&self, at: u32) -> String {
        self.prepared
            .plan
            .config()
            .fields
            .get(at as usize)
            .map(|field| field.name.clone())
            .unwrap_or_default()
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
        self.call_id(id, args)
    }

    /// [`Program::call`], for the callable numbered `id` in the plan.
    pub fn call_id(&self, id: u32, args: Vec<Value>) -> Result<Outcome, CallError> {
        let unknown = || CallError::Unknown(format!("callable {id}"));
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
            json: self.stored_json(),
            auth: &self.auth,
            client: &self.client,
        }
    }

    /// What the runtime's entry point for operations with effects runs
    /// them with (chunk 7.0b): the plan's JSON table, events and settings,
    /// and the keys.
    pub(crate) fn effects(&self) -> jadpo_runtime::effects::Effects<'_> {
        jadpo_runtime::effects::Effects {
            json: self.stored_json(),
            event_numbers: &self.plan().json().events,
            events: self.plan().events(),
            config: self.plan().config(),
            keys: &self.keys,
        }
    }

    /// What a provider call runs with (area 11, chunk 7.2c): the plan's
    /// connections and operations, the JSON codecs their types are written
    /// and read with, the failures they raise and the credentials.
    pub(crate) fn providers(&self) -> jadpo_runtime::provider::Providers<'_> {
        jadpo_runtime::provider::Providers {
            plan: self.plan().providers(),
            json: self.stored_json(),
            failures: self.plan().types().failures(),
            credentials: &self.credentials,
            application: &self.plan().providers().application,
        }
    }

    /// The plan's JSON codecs, with this thread's validators and shapes:
    /// what [`Program::stored_json`] numbers, and a prompt's fields are
    /// written with (area 21).
    pub(crate) fn json_codecs(&self) -> jadpo_value::json::Codecs<'_> {
        jadpo_value::json::Codecs {
            codecs: &self.plan().json().codecs,
            validators: &self.prepared.validators,
            shapes: &self.shapes,
        }
    }

    /// The codecs a JSON-held column is written and read with (chunk
    /// 5.5w2b): `json.read`'s, numbered by the plan's JSON table.
    pub fn stored_json(&self) -> jadpo_runtime::JsonCodecs<'_> {
        jadpo_runtime::JsonCodecs {
            codecs: self.json_codecs(),
            types: &self.plan().json().types,
        }
    }

    /// The latest migration's backfills worked out in Jadpo, in a program
    /// prepared [`Prepared::with_backfills`], as the runtime runs them
    /// (chunk 6.1a): each with its callable and its field's validator.
    pub fn backfills(&self) -> Vec<jadpo_runtime::runner::Backfill<'_>> {
        self.plan()
            .backfills()
            .iter()
            .map(|plan| jadpo_runtime::runner::Backfill {
                migration: plan.migration.clone(),
                field: plan.field.clone(),
                callable: plan.callable,
                reads: plan.reads.clone(),
                held: plan
                    .held
                    .iter()
                    .map(|held| {
                        held.as_ref()
                            .map(|held| (held.field.clone(), self.validator(held.validator)))
                    })
                    .collect(),
                write: plan.write,
                validator: Some(self.validator(plan.validator)),
            })
            .collect()
    }

    /// The structured fields the latest migration reads or writes as JSON
    /// in Jadpo, in a program prepared [`Prepared::with_backfills`] (chunk
    /// 5.9c), each with its type's number in the JSON table.
    pub fn migration_json(&self) -> Vec<jadpo_runtime::runner::StoredJson> {
        self.plan()
            .migration_json_fields()
            .iter()
            .map(|json| jadpo_runtime::runner::StoredJson {
                migration: json.migration.clone(),
                field: json.field.clone(),
                number: json.number,
            })
            .collect()
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

/// What a host serves (chunk 6.9): the interpreter's route table and codecs.
impl jadpo_runtime::serve::Application for Program {
    fn app(&self) -> jadpo_runtime::http::App<'_> {
        Program::app(self)
    }
}

impl Executor for Program {
    fn places(&self) -> &Places {
        Program::places(self)
    }

    fn callable(&self, key: &str) -> Option<u32> {
        self.plan().find(key)
    }

    fn settings(&self, callable: u32) -> Vec<(u32, Value)> {
        let config = self.plan().config();
        self.plan()
            .callable(callable)
            .into_iter()
            .flat_map(|plan| plan.settings.iter())
            .filter_map(|(at, text)| {
                let field = config.fields.get(*at as usize)?;
                let value =
                    jadpo_runtime::config::decode(config, &self.prepared.validators, field, text)
                        .ok()?;
                Some((*at, value))
            })
            .collect()
    }

    fn clock(&self, callable: u32) -> Option<i64> {
        self.plan().callable(callable)?.clock
    }

    fn batches(&self) -> Option<jadpo_runtime::batches::Batches<'_>> {
        let plans = self.plan().batches();
        (!plans.is_empty()).then(|| jadpo_runtime::batches::Batches {
            plans,
            json: self.stored_json(),
            failures: self.plan().types().failures(),
        })
    }

    fn events(&self) -> Option<jadpo_runtime::outbox::Events<'_>> {
        Some(jadpo_runtime::outbox::Events {
            plan: self.plan().events(),
            json: self.stored_json(),
            numbers: &self.plan().json().events,
        })
    }

    fn schedules(&self) -> Option<&jadpo_std::schedule::SchedulesPlan> {
        Some(self.plan().schedules())
    }

    fn file_stores(&self) -> &[jadpo_std::files::FilesConnection] {
        self.plan().file_stores()
    }

    fn provider_connections(&self) -> Vec<&str> {
        self.plan()
            .providers()
            .connections
            .iter()
            .map(|connection| connection.name.as_str())
            .collect()
    }

    fn purge(&self, entity: &str) -> Option<(&jadpo_std::write::PurgePlan, Option<i64>)> {
        let purges = self.plan().purges();
        let at = purges.iter().position(|plan| plan.entity == entity)?;
        Some((&purges[at], self.purge_after.get(at).copied().flatten()))
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

    fn row<'a, 'p: 'a>(
        &'a self,
        ctx: &'a mut Ctx<'p>,
        row: &'a jadpo_std::batch::BatchRow,
        item: Value,
        saved: &'a [Value],
    ) -> LocalFuture<'a, Outcome> {
        Box::pin(async move { drive::row(self, ctx, row, item, saved) })
    }
}
