//! The parts of a program's plan the runtime reads (chunk 7.9e): one
//! borrowed view, [`RuntimePlan`], which an [`crate::Executor`] gives
//! through [`crate::Executor::plan`], so the runtime builds what each of
//! its parts runs with (a delivery's subscribers, a batch's codecs, a
//! provider call's connections and credentials) itself, rather than each
//! executor bundling them. What the program started with besides, its
//! settings' values and the secrets held apart from them, is
//! [`crate::Started`], reached through the context.
//!
//! Phase 8 (chunk 8.1) makes it one plan with a canonical byte encoding,
//! which compiled code holds as constants.

use jadpo_std::ai::AiPlan;
use jadpo_std::batch::BatchPlan;
use jadpo_std::config::ConfigPlan;
use jadpo_std::embed::PromptRecording;
use jadpo_std::files::FilesConnection;
use jadpo_std::http::{EventJson, RouteTable};
use jadpo_std::prompt::PromptPlan;
use jadpo_std::provider::ProviderPlan;
use jadpo_std::schedule::SchedulesPlan;
use jadpo_std::subscriber::EventsPlan;
use jadpo_std::types::FailurePlan;
use jadpo_std::write::PurgePlan;

use crate::{JsonCodecs, Started};

/// What the runtime reads of a program's plan, borrowed from its executor:
/// `None` (or nothing) for a part an executor has none of.
#[derive(Clone, Copy)]
pub struct RuntimePlan<'a> {
    /// The plan's JSON table with the program's validators and shapes:
    /// what a JSON-held column, an event's payload, a provider's input and
    /// answer, and a prompt's fields are written and read with.
    pub json: JsonCodecs<'a>,
    /// The failures the runtime raises from their plans.
    pub failures: &'a [FailurePlan],
    /// The settings, which [`Started::new`] reads the values of.
    pub config: Option<&'a ConfigPlan>,
    /// The route table, whose sign-in and `client_address:` settings
    /// [`Started::new`] reads.
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
    /// The connections the program keeps files in.
    pub file_stores: &'a [FilesConnection],
    /// The connections to provider packages, their operations, webhooks,
    /// queues and polls.
    pub providers: Option<&'a ProviderPlan>,
    /// The connection `ai:` names, and the prompts.
    pub ai: Option<&'a AiPlan>,
    pub prompts: Option<&'a PromptPlan>,
    /// Each recording of a prompt's answer, which `jadpo test` reads.
    pub prompt_recordings: &'a [PromptRecording],
}

impl std::fmt::Debug for RuntimePlan<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RuntimePlan")
            .field("events", &self.events.is_some())
            .field("batches", &self.batches.len())
            .field("purges", &self.purges.len())
            .finish_non_exhaustive()
    }
}

impl RuntimePlan<'static> {
    /// A plan with nothing in it: an executor's that only runs callables.
    pub const NONE: RuntimePlan<'static> = RuntimePlan {
        json: JsonCodecs::NONE,
        failures: &[],
        config: None,
        routes: None,
        events: None,
        event_numbers: &[],
        batches: &[],
        schedules: None,
        purges: &[],
        file_stores: &[],
        providers: None,
        ai: None,
        prompts: None,
        prompt_recordings: &[],
    };
}

impl<'a> RuntimePlan<'a> {
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
            json: self.json,
            failures: self.failures,
            credentials: started.credentials(),
            application: &plan.application,
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
        self.providers
            .map(|plan| {
                plan.connections
                    .iter()
                    .map(|connection| connection.name.as_str())
                    .collect()
            })
            .unwrap_or_default()
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
