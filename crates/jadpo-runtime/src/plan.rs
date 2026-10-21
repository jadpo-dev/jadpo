//! The runtime's plan (chunks 7.9e and 8.1a). What the runtime reads of a
//! program is one owned value, [`RuntimePlan`] (`jadpo-std`'s), which
//! `jadpo-plan` builds and compiled code holds in its canonical bytes.
//! [`Loaded`] is it on one thread: its validators compiled, once for every
//! thread, and its shapes made, since values never cross threads (§2.6).
//! [`Plan`] is the view an [`crate::Executor`] gives through
//! [`crate::Executor::plan`], borrowed from one loaded plan, from which the
//! runtime builds what each of its parts runs with (a delivery's
//! subscribers, a batch's codecs, a provider call's connections and
//! credentials, the HTTP component's [`App`]) itself, rather than each
//! executor bundling them. What the program started with besides, its
//! settings' values and the secrets held apart from them, is
//! [`crate::Started`], reached through the context.

use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock};

pub use jadpo_std::RuntimePlan;
use jadpo_std::ai::AiPlan;
use jadpo_std::batch::BatchPlan;
use jadpo_std::config::ConfigPlan;
use jadpo_std::connections::Connection;
use jadpo_std::http::{EventJson, RouteTable};
use jadpo_std::prompt::PromptPlan;
use jadpo_std::provider::ProviderPlan;
use jadpo_std::schedule::SchedulesPlan;
use jadpo_std::subscriber::EventsPlan;
use jadpo_std::types::FailurePlan;
use jadpo_std::write::PurgePlan;
use jadpo_value::Validator;
use jadpo_value::shape::Shape;

use crate::http::App;
use crate::{JsonCodecs, Started};

/// A runtime plan loaded on one thread: see the module.
#[derive(Debug)]
pub struct Loaded {
    plan: Arc<RuntimePlan>,
    validators: Arc<[Validator]>,
    /// Each shape with its names as values hold them, by number.
    shapes: Vec<Shape>,
}

impl Loaded {
    /// Each of `plan`'s validators, compiled, by number: what every
    /// thread's [`Loaded`] shares, so a project's patterns are compiled
    /// once.
    pub fn compile(plan: &RuntimePlan) -> Arc<[Validator]> {
        plan.validators
            .iter()
            .map(|plan| {
                Validator::new(plan.builtin, plan.rules.clone()).with_across(plan.across.clone())
            })
            .collect()
    }

    /// `plan` loaded on this thread, its validators compiled.
    pub fn new(plan: Arc<RuntimePlan>) -> Loaded {
        let validators = Loaded::compile(&plan);
        Loaded::sharing(plan, validators)
    }

    /// `plan` loaded on this thread with `validators`, which
    /// [`Loaded::compile`] made from it, on this thread or another.
    pub fn sharing(plan: Arc<RuntimePlan>, validators: Arc<[Validator]>) -> Loaded {
        let shapes = plan.shapes.iter().map(Shape::new).collect();
        Loaded {
            plan,
            validators,
            shapes,
        }
    }

    /// The plan it loaded.
    pub fn runtime(&self) -> &Arc<RuntimePlan> {
        &self.plan
    }

    /// Its validators, compiled, by number: what another thread's
    /// [`Loaded::sharing`] takes.
    pub fn validators(&self) -> &Arc<[Validator]> {
        &self.validators
    }

    /// Its shapes, by number.
    pub fn shapes(&self) -> &[Shape] {
        &self.shapes
    }

    /// The view the runtime's parts read: every part of the plan, with
    /// this thread's validators and shapes.
    pub fn plan(&self) -> Plan<'_> {
        let plan = &*self.plan;
        Plan {
            json: JsonCodecs {
                codecs: jadpo_value::json::Codecs {
                    codecs: &plan.json.codecs,
                    validators: &self.validators,
                    shapes: &self.shapes,
                    coder: None,
                },
                types: &plan.json.types,
            },
            failures: &plan.failures,
            config: Some(&plan.config),
            routes: Some(&plan.routes),
            events: Some(&plan.events),
            event_numbers: &plan.json.events,
            batches: &plan.batches,
            schedules: Some(&plan.schedules),
            purges: &plan.purges,
            connections: &plan.connections,
            providers: Some(&plan.providers),
            ai: plan.ai.as_ref(),
            prompts: Some(&plan.prompts),
            callables: Some(&plan.callables),
            #[cfg(feature = "testing")]
            testing: Some(&plan.testing),
        }
    }
}

/// What the runtime reads of a program's plan, borrowed from its
/// executor's [`Loaded`]: `None` (or nothing) for a part an executor has
/// none of.
#[derive(Clone, Copy)]
pub struct Plan<'a> {
    /// The plan's JSON table with the program's validators and shapes:
    /// what a JSON-held column, an event's payload, a provider's input and
    /// answer, and a prompt's fields are written and read with.
    pub json: JsonCodecs<'a>,
    /// The failures the runtime raises from their plans.
    pub failures: &'a [FailurePlan],
    /// The settings, which [`Started::new`] reads the values of.
    pub config: Option<&'a ConfigPlan>,
    /// The route table, whose sign-in and `client_address:` settings
    /// [`Started::new`] reads, and whose routes and codecs the HTTP
    /// component answers with ([`Plan::app`]).
    pub routes: Option<&'a RouteTable>,
    /// The program's subscribers, and each event type's number in the JSON
    /// table.
    pub events: Option<&'a EventsPlan>,
    pub event_numbers: &'a [EventJson],
    /// Each `batches.each` call's plan.
    pub batches: &'a [BatchPlan],
    /// The schedules, where their state is kept, and the stores the tick
    /// sweeps.
    pub schedules: Option<&'a SchedulesPlan>,
    /// Each entity's purge, in the order [`Started`] keeps their periods.
    pub purges: &'a [PurgePlan],
    /// Every connection, with its driver and the targets it runs on
    /// (chunk 8.1d): what a host builds each connection's driver from.
    pub connections: &'a [Connection],
    /// The operations of the connections to provider packages, their
    /// webhooks, queues and polls.
    pub providers: Option<&'a ProviderPlan>,
    /// The connection `ai:` names, and the prompts.
    pub ai: Option<&'a AiPlan>,
    pub prompts: Option<&'a PromptPlan>,
    /// Each callable's number by its identity key, but a test's (chunk
    /// 8.1e): how the runtime finds one the plan names by its key.
    pub callables: Option<&'a BTreeMap<String, u32>>,
    /// The dev and test section (chunk 8.1e): each test's key and what it
    /// gives itself, and the recordings.
    #[cfg(feature = "testing")]
    pub testing: Option<&'a jadpo_std::testing::TestingPlan>,
}

impl std::fmt::Debug for Plan<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Plan")
            .field("events", &self.events.is_some())
            .field("batches", &self.batches.len())
            .field("purges", &self.purges.len())
            .finish_non_exhaustive()
    }
}

impl Plan<'static> {
    /// A plan with nothing in it: an executor's that only runs callables.
    pub const NONE: Plan<'static> = Plan {
        json: JsonCodecs::NONE,
        failures: &[],
        config: None,
        routes: None,
        events: None,
        event_numbers: &[],
        batches: &[],
        schedules: None,
        purges: &[],
        connections: &[],
        providers: None,
        ai: None,
        prompts: None,
        callables: None,
        #[cfg(feature = "testing")]
        testing: None,
    };
}

impl<'a> Plan<'a> {
    /// The number of the callable with identity key `key` (chunk 8.1e):
    /// a webhook reply's action, a package's own function, or, with the
    /// `testing` feature, a test.
    pub fn callable(self, key: &str) -> Option<u32> {
        if let Some(number) = self.callables.and_then(|callables| callables.get(key)) {
            return Some(*number);
        }
        #[cfg(feature = "testing")]
        if let Some(number) = self.testing.and_then(|testing| testing.tests.get(key)) {
            return Some(*number);
        }
        None
    }

    /// The dev and test section (chunk 8.1e), empty for an executor with
    /// none.
    #[cfg(feature = "testing")]
    pub fn testing(self) -> &'a jadpo_std::testing::TestingPlan {
        static NONE: OnceLock<jadpo_std::testing::TestingPlan> = OnceLock::new();
        self.testing
            .unwrap_or_else(|| NONE.get_or_init(Default::default))
    }

    /// The settings test `callable` gives its own values, by their
    /// numbers (area 13, "Configuration in tests"; chunk 8.1e): its
    /// `config:`, each read as its setting's type reads one.
    #[cfg(feature = "testing")]
    pub fn test_settings(self, callable: u32) -> Vec<(u32, jadpo_value::Value)> {
        let Some(config) = self.config else {
            return Vec::new();
        };
        let Some(setup) = self.testing().setups.get(&callable) else {
            return Vec::new();
        };
        setup
            .settings
            .iter()
            .filter_map(|(at, text)| {
                let field = config.fields.get(*at as usize)?;
                let value =
                    crate::config::decode(config, self.json.codecs.validators, field, text).ok()?;
                Some((*at, value))
            })
            .collect()
    }

    /// Where test `callable`'s clock starts, in milliseconds since
    /// 1970-01-01T00:00:00Z: its `clock:` (area 13; chunk 8.1e).
    #[cfg(feature = "testing")]
    pub fn test_clock(self, callable: u32) -> Option<i64> {
        self.testing().setups.get(&callable)?.clock
    }

    /// What the HTTP component answers the program's routes with (area 7):
    /// the route table and its codecs, with this thread's validators and
    /// shapes; a program with no routes answers every path `404`.
    pub fn app(self) -> App<'a> {
        static NO_ROUTES: OnceLock<RouteTable> = OnceLock::new();
        let table = self
            .routes
            .unwrap_or_else(|| NO_ROUTES.get_or_init(RouteTable::default));
        App {
            table,
            codecs: jadpo_value::json::Codecs {
                codecs: &table.codecs,
                ..self.json.codecs
            },
            failures: self.failures,
            json: self.json,
        }
    }

    /// The program's subscribers and the codecs its events' payloads are
    /// read with (area 12, chunk 6.1b): what the runner delivers an event
    /// with. `None` for a program that has none to deliver to.
    pub fn events(self) -> Option<crate::outbox::Events<'a>> {
        Some(crate::outbox::Events {
            plan: self.events?,
            json: self.json,
            numbers: self.event_numbers,
        })
    }

    /// What the program's batches run with (area 12, chunk 6.4): each
    /// `batches.each` call's plan and the codecs its saved values are kept
    /// in. `None` for a program that starts none.
    pub fn batches(self) -> Option<crate::batches::Batches<'a>> {
        (!self.batches.is_empty()).then_some(crate::batches::Batches {
            plans: self.batches,
            json: self.json,
            failures: self.failures,
        })
    }

    /// The program's schedules, where their state is kept, and the stores
    /// the tick sweeps (area 12, chunk 6.2). `None` for a program the tick
    /// has nothing to do for.
    pub fn schedules(self) -> Option<&'a SchedulesPlan> {
        self.schedules
    }

    /// What a provider call runs with (area 11, chunk 7.2c): the plan's
    /// connections and operations, the JSON codecs their types are written
    /// and read with, the failures they raise and the credentials
    /// `started` holds. `None` for a program with no provider plan.
    pub fn providers(self, started: &'a Started) -> Option<crate::provider::Providers<'a>> {
        let plan = self.providers?;
        Some(crate::provider::Providers {
            plan,
            connections: self.connections,
            json: self.json,
            failures: self.failures,
            credentials: started.credentials(),
            application: &plan.application,
            #[cfg(feature = "testing")]
            recordings: &self.testing().operation_recordings,
        })
    }

    /// The program's webhooks, the secrets `started` holds for them and
    /// the codecs their events are read with (area 11, "Receiving from
    /// outside"; chunk 7.7). `None` for a program that receives none.
    pub fn webhooks(self, started: &'a Started) -> Option<crate::webhooks::Webhooks<'a>> {
        let plan = &self.providers?.webhooks;
        (!plan.is_empty()).then_some(crate::webhooks::Webhooks {
            plan,
            credentials: started.credentials(),
            json: self.json,
        })
    }

    /// The program's queues, with its connections' operations and the
    /// codecs a mapping's input is read with (area 12, "Events from
    /// outside"; chunk 7.7c). `None` for a program that receives none.
    pub fn receiving(self) -> Option<crate::receiving::Receiving<'a>> {
        let providers = self.providers?;
        let queues = &providers.queues;
        (!queues.is_empty()).then_some(crate::receiving::Receiving {
            queues,
            providers,
            connections: self.connections,
            json: self.json,
        })
    }

    /// The program's polls (area 11, "Polls"; chunk 7.6c). `None` for a
    /// program that runs none.
    pub fn polls(self) -> Option<&'a [jadpo_std::provider::PollPlan]> {
        let polls = &self.providers?.polls;
        (!polls.is_empty()).then_some(polls.as_slice())
    }

    /// What an `ai` call runs with (area 21, chunk 5.5v3): the connection
    /// `ai:` names and its models, the prompts, and the JSON codecs their
    /// fields and the answers' types are written and read with, with how a
    /// package filling `ai` is called. `None` for a program that asks no
    /// model.
    pub fn ai(self, started: &'a Started) -> Option<crate::ai::Ai<'a>> {
        Some(crate::ai::Ai {
            plan: self.ai?,
            prompts: self.prompts?,
            json: self.json,
            providers: self.providers(started),
        })
    }

    /// The purge of the entity called `entity` (area 5, "Lifecycle"), which
    /// its built-in schedule runs (chunk 6.2c), with how long it waits, in
    /// milliseconds, as `started` holds it: its `after:`, or the value of
    /// the setting it reads; `None` when that holds no duration. `None`
    /// for an entity that isn't purged.
    pub fn purge(self, entity: &str, started: &Started) -> Option<(&'a PurgePlan, Option<i64>)> {
        let at = self.purges.iter().position(|plan| plan.entity == entity)?;
        Some((self.purges.get(at)?, started.purge_after(at)))
    }

    /// The program's connections to provider packages (area 11; chunk
    /// 7.2d): what a test's host gives a package's fake for.
    pub fn provider_connections(self) -> Vec<&'a str> {
        jadpo_std::connections::packages(self.connections)
            .map(|(_, name, _)| name)
            .collect()
    }

    /// The program's stores of files (area 16): every `Driver.disk`
    /// connection, by name.
    pub fn file_stores(self) -> Vec<&'a str> {
        jadpo_std::connections::files(self.connections)
            .map(|(name, _)| name)
            .collect()
    }

    /// The connection `ai:` names (area 21; chunk 5.5v3): what a test's
    /// host gives the `ai` fake for, and a native host its driver for.
    pub fn ai_connection(self) -> Option<&'a str> {
        self.ai.map(|ai| ai.connection.as_str())
    }

    /// The email connection, whose mail a test's host catches whole (area
    /// 16; chunk 7.3c).
    pub fn mail_connection(self) -> Option<&'a str> {
        self.events?.emails.connection.as_deref()
    }
}
