//! A deployable's plan, holding only what it reaches (area 14, "Declared
//! deployables"; roadmap chunk 8.1o3; docs/compiler-runtime.md §2.6, "A
//! deployable compiled from what it reaches").
//!
//! [`Plan::for_deployable`] plans the project as [`Plan::build`] does but
//! from what the deployable reaches ([`Keep`]): the callables
//! `jadpo_check::deployables::reached` follows from its routes, the
//! background work it runs and what the runtime runs by key or number;
//! the routes of the APIs it serves and those APIs alone; its schedules,
//! polls and purges only where it runs schedules; its subscribers'
//! actions, the queues it receives and the emails' templates only where
//! it runs subscribers, every subscriber still listed so that an event it
//! emits is left for the deployable that runs it
//! ([`jadpo_std::subscriber::Target::Elsewhere`]); the connections it
//! reaches and every database and Redis connection, which every
//! deployable's tick sweeps; and the settings what it holds reads. Each
//! table is numbered from what is kept, so a change to what it doesn't
//! reach changes none of its plan.
//!
//! The settings are found by planning twice: once with every setting,
//! noting each one a kept part asks for, then with only those. Whatever
//! a kept part names that the plan doesn't keep, a callable, a query, a
//! setting or an operation, is noted rather than left out quietly, and
//! the plan refused with what it named ([`Unkept`]): a gap between what
//! the checker follows and what the plan holds is Jadpo's own mistake,
//! never a program that faults where it reaches the gap.

use std::cell::RefCell;
use std::collections::{BTreeSet, HashSet};
use std::fmt;

use jadpo_check::data::ConnectionKind;
use jadpo_check::deployables::Deployable;
use jadpo_db::{Db, Project};
use jadpo_graph::body::{Body, Callee, Node, Stmt};
use jadpo_graph::{Key, TypeRef};
use jadpo_std::Runs;

use crate::Plan;

/// What a deployable's program holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Keep {
    /// The callables it reaches, by key.
    pub callables: BTreeSet<Key>,
    /// The APIs it serves.
    pub serves: Vec<String>,
    /// The background work it runs.
    pub runs: Runs,
    /// The connections it reaches, by name; every database and Redis
    /// connection is kept besides.
    pub connections: BTreeSet<String>,
}

impl Keep {
    /// What `deployable`, one of `project`'s, reaches.
    pub fn of(db: &dyn Db, project: Project, deployable: &Deployable) -> Keep {
        let reached = jadpo_check::deployables::reached(db, project, deployable);
        Keep {
            callables: reached.callables,
            serves: deployable.serves.clone(),
            runs: deployable.runs,
            connections: reached.connections.into_keys().collect(),
        }
    }

    /// Whether it serves API `api`.
    pub fn serves(&self, api: &str) -> bool {
        self.serves.iter().any(|served| served == api)
    }
}

/// What a deployable's plan named but didn't keep, each as a sentence's
/// end: `` the callable `action cafe.tidy` ``.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unkept(pub Vec<String>);

impl fmt::Display for Unkept {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "its plan names what it doesn't hold: {}",
            self.0.join(", ")
        )
    }
}

impl std::error::Error for Unkept {}

/// One planning of a deployable: what it keeps, and what the planning
/// asked for and found it didn't keep.
pub(crate) struct Pruning<'k> {
    pub(crate) keep: &'k Keep,
    /// The settings kept, by name: `None` on the first pass, which keeps
    /// every one.
    settings: Option<BTreeSet<String>>,
    /// The settings a kept part asked for, by name.
    asked: RefCell<BTreeSet<String>>,
    unkept: RefCell<BTreeSet<String>>,
}

impl<'k> Pruning<'k> {
    fn new(keep: &'k Keep, settings: Option<BTreeSet<String>>) -> Pruning<'k> {
        Pruning {
            keep,
            settings,
            asked: RefCell::new(BTreeSet::new()),
            unkept: RefCell::new(BTreeSet::new()),
        }
    }

    pub(crate) fn runs(&self) -> Runs {
        self.keep.runs
    }

    /// Whether setting `name` is kept.
    pub(crate) fn keeps_setting(&self, name: &str) -> bool {
        self.settings
            .as_ref()
            .is_none_or(|settings| settings.contains(name))
    }

    /// Whether connection `name` of kind `kind` is kept: one it reaches,
    /// or a database or Redis, which every deployable's tick sweeps.
    pub(crate) fn keeps_connection(&self, name: &str, kind: &ConnectionKind) -> bool {
        matches!(kind, ConnectionKind::Postgres | ConnectionKind::Redis)
            || self.keep.connections.contains(name)
    }

    /// Notes that a kept part asked for setting `name`.
    pub(crate) fn asked(&self, name: &str) {
        self.asked.borrow_mut().insert(name.to_string());
    }

    /// Notes that a kept part named `what`, which isn't kept.
    pub(crate) fn unkept(&self, what: String) {
        self.unkept.borrow_mut().insert(what);
    }

    /// What the callables it keeps, whose typed bodies are `bodies`, use
    /// besides what they call: the entities whose `get` they call, the
    /// prompts they ask for and the event types they emit; and, where it
    /// runs the subscribers, the event types they hear and every email's.
    pub(crate) fn used<'b>(
        &self,
        db: &dyn Db,
        project: Project,
        bodies: impl Iterator<Item = &'b Body>,
    ) -> Used {
        let mut used = Used::default();
        for body in bodies {
            for (_, callee) in body.calls() {
                match callee {
                    Callee::Entity { entity, operation } if operation == "get" => {
                        used.gets.insert(entity.clone());
                    }
                    Callee::Prompt(prompt)
                    | Callee::Generate { prompt, .. }
                    | Callee::Stream { prompt, .. } => {
                        used.prompts.insert(prompt.clone());
                    }
                    _ => {}
                }
            }
            for site in &body.sites {
                if let Node::Stmt(Stmt::Emit { value: Some(value) }) = &site.node
                    && let Some(TypeRef::Node(key)) =
                        body.sites.get(value.0 as usize).map(|value| &value.ty)
                {
                    used.events.insert(key.clone());
                }
            }
        }
        if self.runs().subscribers {
            used.events.extend(
                jadpo_check::events::subscribers(db, project)
                    .iter()
                    .map(|subscriber| subscriber.event.clone()),
            );
            let lookup = jadpo_check::lookups::Lookup::new(db, project);
            used.events.extend(
                jadpo_check::emails::analysis(db, project)
                    .emails
                    .iter()
                    .filter_map(|email| {
                        jadpo_check::graph::declaration_key(&lookup, &email.canonical)
                    }),
            );
        }
        used
    }
}

/// What a deployable's callables use besides what they call
/// ([`Pruning::used`]).
#[derive(Debug, Default)]
pub(crate) struct Used {
    /// The entities whose `get` they call, by key.
    pub(crate) gets: HashSet<Key>,
    /// The prompts they ask for, by name.
    pub(crate) prompts: HashSet<String>,
    /// The event types they emit or hear, by key.
    pub(crate) events: HashSet<Key>,
}

impl Plan {
    /// The plan of `deployable`, one of `project`'s, which checked without
    /// errors: only what it reaches (see the module). `Err` naming what a
    /// kept part named that it doesn't hold, which is Jadpo's own mistake.
    pub fn for_deployable(
        db: &dyn Db,
        project: Project,
        deployable: &Deployable,
    ) -> Result<Plan, Unkept> {
        let keep = Keep::of(db, project, deployable);
        Plan::kept(db, project, &keep)
    }

    /// The plan holding what `keep` says.
    pub fn kept(db: &dyn Db, project: Project, keep: &Keep) -> Result<Plan, Unkept> {
        let every = Pruning::new(keep, None);
        let first = Plan::build_with(db, project, false, Some(&every));
        let mut settings = every.asked.into_inner();
        // What the runtime reads by a setting's name rather than its
        // number: signing in's keys and origin, and the proxies in front.
        settings.extend(
            first
                .runtime()
                .setting_names()
                .into_iter()
                .map(str::to_string),
        );
        let kept = Pruning::new(keep, Some(settings));
        let mut plan = Plan::build_with(db, project, false, Some(&kept));
        let mut unkept = kept.unkept.into_inner();
        unkept.extend(plan.unkept_by_key());
        unkept.extend(
            plan.runtime()
                .setting_names()
                .into_iter()
                .filter(|name| {
                    !plan
                        .runtime()
                        .config
                        .fields
                        .iter()
                        .any(|field| field.name == *name)
                })
                .map(|name| format!("the setting `{name}`")),
        );
        if !unkept.is_empty() {
            return Err(Unkept(unkept.into_iter().collect()));
        }
        // Its build is named once its build id is worked out from it
        // ([`Plan::with_build`]), never by the whole project's digest.
        let runtime = std::sync::Arc::make_mut(&mut plan.runtime);
        runtime.runs = keep.runs;
        runtime.build = String::new();
        plan.build = String::new();
        Ok(plan)
    }

    /// What the runtime finds in this plan by a callable's key that it
    /// doesn't hold: a provider package's `decode:` and `encode:` its
    /// codecs or operations name, and a webhook's reply action.
    fn unkept_by_key(&self) -> Vec<String> {
        let runtime = self.runtime();
        let mut named: Vec<&str> = Vec::new();
        for codecs in [
            &runtime.routes.codecs,
            &runtime.json.codecs,
            &runtime.config.codecs,
        ] {
            for codec in codecs {
                if let jadpo_std::http::Codec::Wired {
                    coding: jadpo_std::http::WireCoding::Functions { decode, encode, .. },
                    ..
                } = codec
                {
                    named.push(decode);
                    named.push(encode);
                }
            }
        }
        for operation in &runtime.providers.operations {
            if let Some((_, decode)) = &operation.decode {
                named.push(decode);
            }
        }
        for webhook in &runtime.providers.webhooks {
            for event in &webhook.events {
                if let Some(reply) = &event.reply {
                    named.push(&reply.action);
                }
            }
        }
        named
            .into_iter()
            .filter(|key| runtime.callable(key).is_none())
            .map(|key| format!("the callable `{key}`"))
            .collect()
    }
}
