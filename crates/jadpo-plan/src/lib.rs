//! The plan (docs/compiler-runtime.md §2.1 and §2.5): what both executors
//! need besides control flow, read from a checked project once. The
//! interpreter consumes it now; the code generator writes Rust from the
//! same plan in Phase 8, so nothing semantic lives in only one executor.
//!
//! - Callables are numbered ([`Plan::callables`]), each with its typed
//!   body, the label a frame names it by, its file and line, and every
//!   site decoded ([`SitePlan`]): the frame table compiled code holds as a
//!   static array and the interpreter turns into the runtime's `Places`;
//!   an action or fixture also carries the [`Isolation`] its transaction
//!   begins at, decided from the graph (area 6, chunk 3.7).
//! - [`Types`]: type plans (declared field order, validator chains, field
//!   defaults, set and list kinds, variants' places in their enum) and
//!   failure shapes (client and internal sections, codes, kinds).
//! - [`Constant`]s: every literal and field default decoded once into the
//!   value the runtime holds.
//!
//! - [`QueryPlan`]s and [`EntityPlan`]s: the SQL each query and `get`
//!   runs, with its parameters, row codecs and the failure when a read of
//!   one record finds none (chunk 3.2, `queries`), and [`WriteSitePlan`]s:
//!   what each `create`, `update` and `delete` runs and what its refusals
//!   raise (chunk 3.3, `writes`), with a lifecycle's transitions, and the
//!   [`PurgePlan`] of each lifecycle's `purge:` (chunk 3.4, `purge`).
//!
//! Built in chunk 2.10 from the checker's graph and typed bodies; reads
//! and creates joined in chunk 3.2, the other writes in chunk 3.3 and
//! lifecycles in chunk 3.4; policy, codecs for HTTP and the route,
//! schedule and subscriber tables join it with their areas. What it decides
//! is `jadpo-std`'s plain data (type and failure plans, constants, read,
//! write and purge plans), which the runtime runs without a compiler crate,
//! and its SQL text is written by `jadpo-sql` (chunk 3.12). It is shared by
//! every thread that runs the project.

mod config;
mod connections;
mod constant;
mod deployable;
mod emails;
mod identity;
mod policy;
mod providers;
mod purge;
mod queries;
mod routes;
mod sites;
mod types;
mod writes;

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use jadpo_check::types::{callables_of, typed_body};
use jadpo_db::{Db, Project};
use jadpo_graph::body::Body;
use jadpo_graph::{Key, Kind, TypeRef};
pub use jadpo_std::http::RouteTable;
pub use jadpo_std::{Isolation, RuntimePlan};

pub use constant::Constant;
pub use deployable::{Keep, Unkept};
pub use jadpo_std::read::{MissingPlan, MissingValue};
pub use jadpo_std::write::{PurgePlan, Raises, Refusal};
pub use queries::{EntityPlan, QueryPlan};
pub use routes::{StandardCodecs, standard_codecs};
pub use sites::{ArmPattern, ConstId, Decoded, Given, Raise, SitePlan};
pub use types::{
    FailureId, FailurePlan, FieldPlan, ShapeId, ShapePlan, Types, ValidatorId, ValidatorPlan,
    VariantPlan, last,
};
pub use writes::WriteSitePlan;

/// One callable with a body: a function, action, fixture, library
/// function, test or seed.
#[derive(Debug)]
pub struct CallablePlan {
    pub key: Key,
    pub kind: Kind,
    /// Its project-relative file.
    pub file: String,
    /// How a frame names it: `action place_order`, `function
    /// rounding.half_even`, `test "a large size is rejected"`.
    pub label: String,
    /// The line its declaration starts on, for a frame at no site yet.
    pub line: u32,
    pub body: Arc<Body>,
    /// Each of the body's sites, decoded, by site number.
    pub sites: Vec<SitePlan>,
    /// The level a transaction of its own begins at, from the graph: for
    /// an action or fixture, read committed when it loads no record,
    /// itself or through any action it calls, otherwise repeatable read
    /// (area 6, "Counters and other one-step updates"). Every other
    /// callable says repeatable read, and never begins one.
    pub isolation: Isolation,
    /// For an action listed under root `policy { actions: }` whose line
    /// doesn't depend on a record: who may call it, checked as it starts
    /// (area 9, "Actions").
    pub gate: Option<jadpo_std::policy::Gate>,
    /// The parameter whose type is a `Principal` (`Principal.user`,
    /// `Principal.service` or either, or one of those or `none`), when it
    /// has one: the caller a test's, fixture's or seed's call of it runs as
    /// (area 9, chunk 4.10). Read from its signature, never from the value
    /// passed.
    pub caller: Option<u32>,
    /// For an action that emits, itself or through what it calls: the
    /// store its outbox is in when it begins no transaction of its own,
    /// the one it writes or the one `events: { store: }` names (area 12,
    /// chunk 6.1b).
    pub outbox: Option<String>,
    /// For an action whose longest way makes several changing calls, run
    /// where its key survives a retry: where the runtime keeps its record
    /// and its step shape (area 12, "Several changing calls in one
    /// action"; chunk 7.5b).
    pub record: Option<jadpo_std::steps::RecordPlan>,
}

impl CallablePlan {
    /// Whether the runtime enters it through its action boundary (area 6,
    /// "All-or-nothing actions"): an action, or a fixture, which is a
    /// test-only action (area 13).
    pub fn is_action(&self) -> bool {
        matches!(self.kind, Kind::Action | Kind::Fixture)
    }
}

/// Whether a parameter of type `ty` is a caller: `Principal`,
/// `Principal.user` or `Principal.service`, or one of them or `none`.
fn holds_caller(ty: &jadpo_graph::TypeRef) -> bool {
    use jadpo_graph::TypeRef;
    match ty {
        TypeRef::Nullable(inner) => holds_caller(inner),
        TypeRef::Std(name, args) => args.is_empty() && is_principal(name),
        _ => false,
    }
}

/// The same, for a query's parameter as the checker types it.
pub(crate) fn holds_caller_ty(ty: &jadpo_check::types::Ty) -> bool {
    use jadpo_check::types::Ty;
    match ty {
        Ty::Nullable(inner) => holds_caller_ty(inner),
        Ty::Opaque(name, args) => args.is_empty() && is_principal(name),
        _ => false,
    }
}

/// `Principal`, or one of its variants' types.
fn is_principal(name: &str) -> bool {
    name == jadpo_check::identity::PRINCIPAL
        || [
            jadpo_std::http::PrincipalKind::User,
            jadpo_std::http::PrincipalKind::Service,
        ]
        .iter()
        .any(|kind| kind.type_name() == name)
}

/// Every event type the project declares, and every email type, which is
/// sent by emitting it (area 16), by key, in key order.
fn event_types(graph: &jadpo_graph::Graph) -> Vec<Key> {
    let mut keys: Vec<Key> = graph
        .nodes()
        .iter()
        .filter(|node| {
            node.kind == Kind::Type
                && matches!(
                    node.detail,
                    jadpo_graph::Detail::Type {
                        base: jadpo_graph::Base::Event | jadpo_graph::Base::EmailMessage,
                        ..
                    }
                )
        })
        .map(|node| node.key.clone())
        .collect();
    keys.sort();
    keys
}

/// The subscriber table (area 12, chunk 6.1b): each subscriber that reads
/// cleanly, with the callable it runs and the store its handled record is
/// kept in. A deployable that doesn't run the subscribers (`running`
/// false) lists each without its action (chunk 8.1o3).
fn events_plan(
    db: &dyn Db,
    project: Project,
    callable: &dyn Fn(&Key) -> Option<u32>,
    numbering: &providers::Numbering,
    setting: &dyn Fn(&str) -> Option<u32>,
    running: bool,
) -> jadpo_std::subscriber::EventsPlan {
    use jadpo_check::events::Tenant;
    use jadpo_std::subscriber::{SubscriberPlan, Target, TenantFrom};
    let written = jadpo_check::types::written_stores(db, project);
    let called = jadpo_check::types::called_connections(db, project);
    let (emails, sending) = emails::plan(db, project, callable, numbering, setting, running);
    // A package's own subscriber listens to each connection receiving its
    // package's webhooks, and its action calls the API through that one
    // (chunk 7.6b): one plan for each, named by the connection.
    let receiving = jadpo_check::webhooks::webhooks(db, project);
    let mut subscribers: Vec<SubscriberPlan> = Vec::new();
    for model in jadpo_check::events::subscribers(db, project).iter() {
        let target = if running {
            let Some(callable) = callable(&model.run) else {
                continue;
            };
            Target::Action {
                callable,
                args: model.args.clone(),
            }
        } else {
            Target::Elsewhere
        };
        let throughs: Vec<Option<String>> = match &model.package {
            Some(folder) => receiving
                .routes
                .iter()
                .filter(|route| {
                    jadpo_check::connections::package_connections(db, project)
                        .connections
                        .iter()
                        .any(|connection| {
                            connection.name == route.connection && connection.folder == *folder
                        })
                })
                .map(|route| Some(route.connection.clone()))
                .collect(),
            None => vec![None],
        };
        for through in throughs {
            let subscriber = SubscriberPlan {
                key: match &through {
                    Some(connection) => format!("{} {connection}", model.key.as_str()),
                    None => model.key.as_str().to_string(),
                },
                name: model.name.clone(),
                event: model.event.as_str().to_string(),
                target: target.clone(),
                attempts: model.retry.attempts,
                within_ms: model.retry.within_ms,
                tenant: match &model.tenant {
                    Tenant::Untenanted => TenantFrom::Untenanted,
                    Tenant::Field(field) => TenantFrom::Field(field.clone()),
                    Tenant::None => TenantFrom::Nowhere,
                },
                // The store its action writes, or, writing none, the one
                // its events are kept in (chunk 6.6a).
                store: written
                    .get(model.run.name())
                    .and_then(|stores| stores.keys().next().cloned())
                    .or_else(|| jadpo_check::events::outbox_store(db, project, model.run.name())),
                // What its action calls out through, which a circuit
                // breaker holds it for (chunk 7.2b): a package's own, the
                // connection it hears.
                connections: match &through {
                    Some(connection) => vec![connection.clone()],
                    None => called
                        .get(model.run.name())
                        .map(|connections| connections.iter().cloned().collect())
                        .unwrap_or_default(),
                },
                through,
                file: model.path.clone(),
                line: model.line,
            };
            subscribers.push(if running {
                subscriber
            } else {
                subscriber.elsewhere()
            });
        }
    }
    // Each email's delivery (area 16; chunk 7.3).
    subscribers.extend(sending);
    subscribers.sort_by(|a, b| a.key.cmp(&b.key));
    jadpo_std::subscriber::EventsPlan {
        subscribers,
        store: jadpo_check::events::events_settings(db, project)
            .store
            .clone(),
        emails,
    }
}

/// The schedule table (area 12, chunk 6.2): each schedule that reads
/// cleanly, with the callable it runs, where their state is kept, and the
/// stores holding the runtime's own tables, which the tick sweeps.
fn schedules_plan(
    db: &dyn Db,
    project: Project,
    callable: &dyn Fn(&Key) -> Option<u32>,
    purges: &[PurgePlan],
    running: bool,
) -> jadpo_std::schedule::SchedulesPlan {
    use jadpo_check::schedules::ScheduleRun;
    use jadpo_std::schedule::{SchedulePlan, SchedulesPlan};
    // A deployable that doesn't run the schedules lists none (chunk
    // 8.1o3).
    let schedules: Vec<SchedulePlan> = jadpo_check::schedules::schedules(db, project)
        .iter()
        .filter(|_| running)
        .filter_map(|model| {
            let run = match &model.run {
                ScheduleRun::Action(key) => {
                    jadpo_std::schedule::ScheduleRun::Action(callable(key)?)
                }
                // An entity's built-in purge runs its purge plan (area 5;
                // chunk 6.2c).
                ScheduleRun::Purge(entity) => {
                    purges.iter().find(|plan| plan.entity == *entity)?;
                    jadpo_std::schedule::ScheduleRun::Purge(entity.clone())
                }
            };
            Some(SchedulePlan {
                key: model.key.as_str().to_string(),
                name: model.name.clone(),
                every_ms: model.every_ms,
                run,
                file: model.path.clone(),
                line: model.line,
            })
        })
        .collect();
    let store = (!schedules.is_empty())
        .then(|| {
            jadpo_check::schedules::schedules_settings(db, project)
                .store
                .clone()
        })
        .flatten();
    let schema = jadpo_check::data::schema(db, project);
    let holding = |table: &'static str| {
        schema
            .stores
            .iter()
            .filter(move |(_, store)| store.tables.contains_key(table))
            .map(|(name, _)| name.clone())
    };
    use jadpo_std::system::{
        IDEMPOTENCY_KEYS, RECEIVED_EVENTS, SERVICE_KEYS, SESSIONS, STEP_RECORDS,
    };
    SchedulesPlan {
        schedules,
        store,
        stores: holding(IDEMPOTENCY_KEYS.table).collect(),
        sessions: holding(SESSIONS.table).next(),
        keys: holding(SERVICE_KEYS.table).next(),
        received: holding(RECEIVED_EVENTS.table).next(),
        steps: holding(STEP_RECORDS.table).collect(),
        parked: holding(jadpo_std::system::PARKED_MESSAGES.table).next(),
        ai_usage: holding(jadpo_std::system::AI_USAGE.table).next(),
        trigger: jadpo_check::schedules::schedules_settings(db, project).trigger,
    }
}

/// A checked project's plan.
#[derive(Debug)]
pub struct Plan {
    callables: Vec<CallablePlan>,
    by_key: HashMap<Key, u32>,
    types: Types,
    constants: Vec<Constant>,
    /// Each file's text, by project-relative path, for reports.
    files: BTreeMap<String, String>,
    /// Every query's plan, in key order.
    queries: Vec<QueryPlan>,
    /// Every entity's operations, in key order.
    entities: Vec<EntityPlan>,
    /// Every write site's plan, in the order the callables are numbered.
    writes: Vec<WriteSitePlan>,
    /// What the runtime reads of the program (chunk 8.1a): the validator,
    /// shape and failure plans, the JSON table, every route with its
    /// codecs, the settings, subscribers, batches, schedules, purges,
    /// stores of files, providers, `ai:`, prompts and their recordings,
    /// shared with every thread's runtime.
    runtime: Arc<RuntimePlan>,
    /// The latest migration's backfills `jadpo migrate apply` works out in
    /// Jadpo (chunk 5.6b2), in a plan built for it.
    backfills: Vec<BackfillPlan>,
    /// The structured fields the latest migration reads or writes as JSON
    /// in Jadpo (chunk 5.9c), in a plan built for it.
    migration_json: Vec<MigrationJson>,
    /// Each `batches.each` site's number's place in the runtime plan's
    /// batches.
    batch_numbers: Vec<Option<u32>>,
    /// The `SigningKey` setting a record's answers are encrypted with,
    /// by its number: `application { steps: { encryption_key: } }` (area
    /// 12; chunk 7.5b).
    steps_key: Option<u32>,
    /// What a record of several changing calls keeps of the build it
    /// started under (area 11, "Deploys"; chunk 7.5b): a digest of every
    /// node's identity key and digest, so the same source gives the same
    /// one; area 18's build id, which adds the compiler, the target and the
    /// lock, replaces it when the build stamp is built.
    build: String,
    /// Each outside call's number in the runtime plan's calls, by its
    /// callable's number and its site's (chunk 8.1f).
    call_numbers: HashMap<(u32, u32), u32>,
}

/// A structured field the latest migration checks, rewrites or fills as
/// JSON in Jadpo (area 5, "Storage types"; chunk 5.9c): its type's number
/// in the plan's JSON table, whose codec reads and writes its values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MigrationJson {
    /// The migration's folder name.
    pub migration: String,
    /// `Parcel.label`, as the migration's SQL names it.
    pub field: String,
    pub number: u32,
}

/// A backfill `jadpo migrate apply` works out in Jadpo (area 5, "Computed
/// backfills"; chunk 5.6b2): the callable that works out one row's value
/// from the stored fields it reads, and how they are read and it is
/// written.
#[derive(Clone, Debug)]
pub struct BackfillPlan {
    /// The migration's folder name.
    pub migration: String,
    /// `Todo.priority`.
    pub field: String,
    /// The callable, whose parameters are the stored fields it reads.
    pub callable: u32,
    /// How each of their columns is read, in parameter order.
    pub reads: Vec<jadpo_std::read::Column>,
    /// The rules each value read is held to, in the same order (chunk
    /// 5.9d): `None` for a field the migration retires.
    pub held: Vec<Option<jadpo_std::read::FieldRules>>,
    /// The column type its value is written as.
    pub write: jadpo_std::SqlType,
    /// Its field's rules, its type's included, which each value is kept
    /// and checked by, as a value of the field's type is (chunk 6.1a): a
    /// decimal at the type's places, rounded half to even.
    pub validator: ValidatorId,
}

impl Plan {
    /// The plan of `project`, which checked without errors: a project with
    /// errors has bodies with nothing to run where the errors are.
    pub fn build(db: &dyn Db, project: Project) -> Plan {
        Plan::build_with(db, project, false, None)
    }

    /// The plan of `project` with the latest migration's backfills that
    /// `jadpo migrate apply` works out in Jadpo (chunk 5.6b2), each a
    /// callable of its own.
    pub fn with_backfills(db: &dyn Db, project: Project) -> Plan {
        Plan::build_with(db, project, true, None)
    }

    /// The plan of `project`, or with `pruning` only what a deployable
    /// keeps of it (chunk 8.1o3; [`Plan::for_deployable`]): each table is
    /// built from what is kept, so its numbers depend on nothing else.
    fn build_with(
        db: &dyn Db,
        project: Project,
        with_backfills: bool,
        pruning: Option<&deployable::Pruning<'_>>,
    ) -> Plan {
        let graph = Arc::clone(jadpo_check::graph::graph(db, project));
        let mut files = BTreeMap::new();
        let mut found: Vec<(Key, Kind, String, Arc<Body>)> = Vec::new();
        // What each test's `config:` changes, by the test's key.
        let mut overrides: HashMap<Key, Vec<(String, String)>> = HashMap::new();
        // Where each test's `clock:` starts its clock.
        let mut clocks: HashMap<Key, i64> = HashMap::new();
        for &file in project.files(db) {
            let path = file.path(db).clone();
            files.insert(path.clone(), file.text(db).clone());
            for callable in callables_of(db, file) {
                let checked = typed_body(db, project, callable);
                let Some(key) = checked.key.clone() else {
                    continue;
                };
                let Some(kind) = key.kind() else { continue };
                if kind == Kind::Test {
                    let given = jadpo_check::config::test_settings(db, project, callable);
                    if !given.overrides.is_empty() {
                        overrides.insert(key.clone(), given.overrides.clone());
                    }
                    if let Some(start) = given.clock {
                        clocks.insert(key.clone(), start);
                    }
                }
                found.push((key, kind, path.clone(), Arc::clone(&checked.body)));
            }
        }
        // Each email template's callable (area 16; chunk 7.3), whose text
        // its reports quote.
        for (path, key) in emails::template_keys(db, project) {
            let checked = jadpo_check::types::template_body(
                db,
                project,
                jadpo_check::lookups::Name::new(db, path.clone()),
            );
            if let Some(text) = project
                .records(db)
                .iter()
                .find(|record| record.path == path)
                .and_then(|record| record.text.as_ref().ok())
            {
                files.insert(path.clone(), text.clone());
            }
            found.push((key, Kind::Function, path, Arc::clone(&checked.body)));
        }
        // Numbered in key order, so the same project always numbers its
        // callables the same way; a key declared twice is an error the
        // checker reports, and the first is kept.
        found.sort_by(|a, b| a.0.cmp(&b.0));
        found.dedup_by(|a, b| a.0 == b.0);
        // A deployable keeps only the callables it reaches (chunk 8.1o3);
        // a kept part naming one of the rest is noted.
        let every: std::collections::HashSet<Key> = match pruning {
            Some(pruning) => {
                let every = found.iter().map(|(key, ..)| key.clone()).collect();
                found.retain(|(key, ..)| pruning.keep.callables.contains(key));
                every
            }
            None => std::collections::HashSet::new(),
        };
        // Each backfill worked out in Jadpo, numbered after every callable
        // the project declares, keyed by its migration and field.
        let mut backfills = Vec::new();
        // The field each fills, by its entity's and its own name.
        let mut backfill_fields: Vec<(String, String)> = Vec::new();
        // The fields each reads, by their names now.
        let mut backfill_reads: Vec<Vec<Option<String>>> = Vec::new();
        let mut backfill_lines: HashMap<Key, (String, u32)> = HashMap::new();
        if with_backfills {
            for backfill in jadpo_check::data::migrations::jadpo_backfills(db, project) {
                let key = Key::new(
                    Kind::Function,
                    &format!("backfill {} {}", backfill.migration, backfill.field),
                );
                backfills.push(BackfillPlan {
                    migration: backfill.migration.clone(),
                    field: backfill.field.clone(),
                    callable: found.len() as u32,
                    reads: backfill
                        .plan
                        .reads
                        .iter()
                        .map(|read| read.decode.clone())
                        .collect(),
                    held: Vec::new(),
                    write: backfill.plan.write,
                    validator: ValidatorId(0),
                });
                backfill_reads.push(
                    backfill
                        .plan
                        .reads
                        .iter()
                        .map(|read| read.name.clone())
                        .collect::<Vec<_>>(),
                );
                backfill_fields.push(backfill.plan.fills.clone());
                backfill_lines.insert(key.clone(), (backfill.field.clone(), backfill.line));
                found.push((
                    key,
                    Kind::Function,
                    backfill.file.clone(),
                    Arc::clone(&backfill.plan.body),
                ));
            }
        }
        let by_key: HashMap<Key, u32> = found
            .iter()
            .enumerate()
            .map(|(i, (key, ..))| (key.clone(), i as u32))
            .collect();
        // A callable's number by its key.
        let callable_number = |key: &Key| -> Option<u32> {
            let number = by_key.get(key).copied();
            if number.is_none()
                && let Some(pruning) = pruning
                && every.contains(key)
            {
                pruning.unkept(format!("the callable `{}`", key.as_str()));
            }
            number
        };
        // Each rule across an object's fields, whose body the type's
        // validator runs (area 2, "Rules across fields").
        let mut rule_bodies = HashMap::new();
        for &file in project.files(db) {
            for callable in jadpo_check::types::rule_callables(db, file) {
                let checked = typed_body(db, project, callable);
                if let Some(key) = checked.key.clone() {
                    rule_bodies.insert(key, Arc::clone(&checked.body));
                }
            }
        }
        // Every provider package's types read its provider's answers, a
        // package with no connection's too, whose recordings `jadpo
        // package check` decodes (chunk 5.5v7).
        let mut types = Types::new(Arc::clone(&graph), rule_bodies).with_providers(
            jadpo_check::surface::homes(db, project)
                .into_values()
                .filter(|home| jadpo_check::connections::reach(db, project, home).is_some())
                .map(|home| home.folder)
                .collect(),
        );
        if !backfills.is_empty() {
            let lookup = jadpo_check::lookups::Lookup::new(db, project);
            for ((backfill, (entity, field)), reads) in backfills
                .iter_mut()
                .zip(&backfill_fields)
                .zip(&backfill_reads)
            {
                let key =
                    jadpo_check::graph::field_key(&lookup, entity, std::slice::from_ref(field));
                backfill.validator = types.validator_id(&TypeRef::Field(key));
                // Each stored value it reads is held to its field's rules,
                // as a query's read is (chunk 5.9d).
                backfill.held = reads
                    .iter()
                    .map(|read| {
                        let read = read.as_ref()?;
                        let key = jadpo_check::graph::field_key(
                            &lookup,
                            entity,
                            std::slice::from_ref(read),
                        );
                        Some(jadpo_std::read::FieldRules {
                            field: format!("{entity}.{read}"),
                            validator: types.validator_id(&TypeRef::Field(key)),
                        })
                    })
                    .collect();
            }
        }
        // The types the JSON table numbers: each JSON-held field's, which
        // its column reads and its writes write (chunk 5.5w2b), then each
        // `json.read` and `json.write` call's.
        let json_types = std::cell::RefCell::new(Vec::new());
        let every_query: std::collections::HashSet<Key> = match pruning {
            Some(_) => jadpo_check::data::queries::query_models(db, project)
                .queries
                .iter()
                .map(|query| query.key.clone())
                .collect(),
            None => std::collections::HashSet::new(),
        };
        // A deployable keeps the queries it reaches, the `get` of each
        // entity the callables it keeps read one record of by `id`, the
        // prompts they ask for and the event types they emit and hear.
        let used = pruning.map(|pruning| {
            pruning.used(
                db,
                project,
                found.iter().map(|(_, _, _, body)| body.as_ref()),
            )
        });
        let (queries, entities, identity) =
            queries::plans(db, project, &mut types, &json_types, &|key| {
                pruning.is_none_or(|pruning| pruning.keep.callables.contains(key))
                    || used.as_ref().is_some_and(|used| used.gets.contains(key))
            });
        let query_ids: HashMap<Key, u32> = queries
            .iter()
            .enumerate()
            .map(|(i, plan)| (plan.key.clone(), i as u32))
            .collect();
        let entity_ids: HashMap<Key, u32> = entities
            .iter()
            .enumerate()
            .map(|(i, plan)| (plan.key.clone(), i as u32))
            .collect();
        let bodies: Vec<(Key, &Body)> = found
            .iter()
            .map(|(key, _, _, body)| (key.clone(), body.as_ref()))
            .collect();
        let config = config::plan(db, project, &mut types, &|name| {
            pruning.is_none_or(|pruning| pruning.keeps_setting(name))
        });
        let registry = jadpo_check::graph::registry(db, project);
        let setting_ids: HashMap<Key, u32> = config
            .fields
            .iter()
            .enumerate()
            .map(|(i, field)| (config::key(registry, &field.name), i as u32))
            .collect();
        // A setting's number by its key, noting for a deployable each one
        // a kept part asks for, and one it doesn't keep (chunk 8.1o3).
        let every_setting: std::collections::HashSet<Key> = match pruning {
            Some(_) => jadpo_check::config::config(db, project)
                .settings
                .iter()
                .map(|setting| config::key(registry, &setting.name))
                .collect(),
            None => std::collections::HashSet::new(),
        };
        let setting_names: Vec<String> = config
            .fields
            .iter()
            .map(|field| field.name.clone())
            .collect();
        let setting_of = |key: &Key| -> Option<u32> {
            let number = setting_ids.get(key).copied();
            if let Some(pruning) = pruning {
                match number.and_then(|at| setting_names.get(at as usize)) {
                    Some(name) => pruning.asked(name),
                    None if every_setting.contains(key) => {
                        pruning.unkept(format!("the setting `{}`", key.name()));
                    }
                    None => {}
                }
            }
            number
        };
        let setting_number = |name: &str| setting_of(&config::key(registry, name));
        let (writes, write_ids) = writes::plans(db, project, &mut types, &bodies, &json_types);
        let gates = policy::gates(db, project, &mut types, &json_types, &|action| {
            pruning.is_none_or(|_| {
                found
                    .iter()
                    .any(|(key, kind, ..)| *kind == Kind::Action && key.name() == action)
            })
        });
        let model = jadpo_check::data::data_model(db, project);
        // An entity's purge runs where schedules run.
        let purges: Vec<PurgePlan> = if pruning.is_none_or(|pruning| pruning.runs().schedules) {
            model
                .entities
                .iter()
                .filter_map(|entity| purge::plan(model, entity, &setting_number))
                .collect()
        } else {
            Vec::new()
        };
        let mut constants = Vec::new();
        let mut json_types = json_types.into_inner();
        // Each prompt's entry, and the type of each field its text writes,
        // read from its type's field nodes (area 21).
        let mut prompts = jadpo_check::prompts::plan(db, project);
        // A kept call asking for a prompt a deployable doesn't keep is
        // noted.
        let every_prompt: std::collections::HashSet<String> = match pruning {
            Some(_) => prompts
                .entries
                .iter()
                .map(|entry| entry.key.clone())
                .collect(),
            None => std::collections::HashSet::new(),
        };
        if let Some(used) = &used {
            prompts
                .entries
                .retain(|entry| used.prompts.contains(&entry.key));
        }
        let prompt_fields: HashMap<String, (u32, Vec<TypeRef>)> = prompts
            .entries
            .iter()
            .enumerate()
            .filter_map(|(at, entry)| {
                let node = graph.get(&jadpo_check::graph::registry(db, project).key(
                    Kind::Prompt,
                    &entry.key,
                    None,
                ))?;
                let parent = node.parent.clone()?;
                let fields = entry
                    .fields
                    .iter()
                    .map(|field| {
                        graph
                            .nodes()
                            .iter()
                            .find(|child| {
                                child.kind == Kind::Field
                                    && child.parent.as_ref() == Some(&parent)
                                    && child.name.rsplit('.').next() == Some(field.name.as_str())
                            })
                            .map_or(TypeRef::None, |child| TypeRef::Field(child.key.clone()))
                    })
                    .collect();
                Some((entry.key.clone(), (at as u32, fields)))
            })
            .collect();
        // Every connection, with its driver and the targets it runs on
        // (area 11; chunk 8.1d), and each provider operation's number, which
        // a call's site names it by (chunk 8.1d2).
        let connections = connections::table(db, project, &setting_number, &|name, kind| {
            pruning.is_none_or(|pruning| pruning.keeps_connection(name, kind))
        });
        let numbering = providers::Numbering::new(db, project, &connections);
        // The operation a call's site calls, noting for a deployable one
        // through a connection it doesn't keep (chunk 8.1o3).
        let declared_connections = &jadpo_check::data::connections(db, project).declared;
        let called = |file: &str, connection: &str, operation: &str| {
            let called = numbering.called(file, connection, operation);
            if let Some(pruning) = pruning
                && called == jadpo_std::provider::Called::default()
                && declared_connections
                    .iter()
                    .any(|(name, _)| name == connection)
            {
                pruning.unkept(format!("the connection `{connection}`"));
            }
            called
        };
        // A query's number by its key.
        let query_number = |key: &Key| -> Option<u32> {
            let number = query_ids.get(key).copied();
            if number.is_none()
                && let Some(pruning) = pruning
                && every_query.contains(key)
            {
                pruning.unkept(format!("the query `{}`", key.as_str()));
            }
            number
        };
        let mut callables = Vec::with_capacity(found.len());
        // The dev and test section (chunk 8.1e): each test's key, and
        // what each test gives itself.
        let mut testing = jadpo_std::testing::TestingPlan::default();
        // Each callable's number by its key, but a test's.
        let mut keyed: BTreeMap<String, u32> = BTreeMap::new();
        // What the runtime keeps for an action with several changing calls
        // (area 12; chunk 7.5b).
        let record_plans = jadpo_check::types::record_plans(db, project);
        // Each `batches.each` call, as its site decodes it (area 12).
        let mut batch_sites: Vec<sites::BatchSite> = Vec::new();
        for (key, kind, file, body) in found {
            let node = graph.get(&key);
            let name = node.map_or(key.name(), |node| node.name.as_str());
            let backfill = backfill_lines.get(&key);
            let label = match (kind, backfill) {
                (_, Some((field, _))) => format!("backfill {field}"),
                // An email's template: `email accounts.WelcomeEmail html`.
                _ if node.is_none() && key.name().starts_with("email ") => key.name().to_string(),
                (Kind::Test, _) => format!("test \"{name}\""),
                (kind, _) => format!("{} {name}", kind.word()),
            };
            let line = node
                .and_then(|node| node.place.as_ref())
                .map_or(backfill.map_or(1, |(_, line)| *line), |place| place.lines.0);
            let isolation = node.and_then(|node| node.isolation()).unwrap_or_default();
            let text = files.get(&file).map(String::as_str).unwrap_or_default();
            let entity = |key: &Key| entity_ids.get(key).copied();
            let write = |site: u32| write_ids.get(&(key.clone(), site)).copied();
            let sites = sites::Decoder {
                types: &mut types,
                constants: &mut constants,
                callable: &callable_number,
                query: &query_number,
                entity: &entity,
                write: &write,
                setting: &setting_of,
                json: &mut json_types,
                prompt: &|key: &str| {
                    let fields = prompt_fields.get(key).cloned();
                    if fields.is_none()
                        && let Some(pruning) = pruning
                        && every_prompt.contains(key)
                    {
                        pruning.unkept(format!("the prompt `{key}`"));
                    }
                    fields
                },
                schema: &|ty: &TypeRef| {
                    jadpo_check::ai::schema(
                        &graph,
                        jadpo_check::locales::locales(db, project).plan.as_ref(),
                        ty,
                    )
                },
                starter: &key,
                batches: &mut batch_sites,
                called: &|connection: &str, operation: &str| called(&file, connection, operation),
            }
            .body(&body, text);
            let gate = (kind == Kind::Action)
                .then(|| gates.get(key.name()).cloned())
                .flatten();
            let caller = body
                .params
                .iter()
                .position(|param| {
                    body.locals
                        .get(param.0 as usize)
                        .is_some_and(|local| holds_caller(&local.ty))
                })
                .and_then(|at| u32::try_from(at).ok());
            let settings: Vec<(u32, String)> = overrides
                .get(&key)
                .into_iter()
                .flatten()
                .filter_map(|(name, text)| {
                    let at = setting_ids.get(&config::key(
                        jadpo_check::graph::registry(db, project),
                        name,
                    ))?;
                    Some((*at, text.clone()))
                })
                .collect();
            let clock = clocks.get(&key).copied();
            let number = u32::try_from(callables.len()).unwrap_or(u32::MAX);
            if kind == Kind::Test {
                testing.tests.insert(key.as_str().to_string(), number);
                if !settings.is_empty() || clock.is_some() {
                    testing
                        .setups
                        .insert(number, jadpo_std::testing::TestSetup { settings, clock });
                }
            } else {
                keyed.insert(key.as_str().to_string(), number);
            }
            let outbox = (kind == Kind::Action)
                .then(|| jadpo_check::events::outbox_store(db, project, key.name()))
                .flatten();
            let record = record_plans.get(&key).cloned();
            callables.push(CallablePlan {
                outbox,
                record,
                key,
                kind,
                file,
                label,
                line,
                body,
                sites,
                isolation,
                gate,
                caller,
            });
        }
        // Each batch's run: the query it walks, the store it is kept in and
        // the level its batches begin their transactions at, repeatable
        // read when an action it calls loads a record (area 12).
        let mut batches: Vec<jadpo_std::batch::BatchPlan> = Vec::new();
        let mut batch_numbers: Vec<Option<u32>> = Vec::new();
        let run_stores = jadpo_check::types::file_run_stores(db, project);
        // An action that makes outside calls runs each item, and
        // `finished:`, as a batch of its own (area 12, chunk 7.4b).
        let outside = jadpo_check::types::outside_calls(db, project);
        let calling =
            |(key, call): (Key, jadpo_std::batch::BatchCall)| jadpo_std::batch::BatchCall {
                calls: outside
                    .get(&key)
                    .is_some_and(|outside| outside.calls.is_some()),
                ..call
            };
        for plan in batch_sites.into_iter().map(|site| {
            // A query's pages, kept in its store; or a file's rows, kept
            // where the run's actions write (chunk 6.4b).
            let (from, store) = match &site.file {
                Some((format, row, ty)) => (
                    jadpo_std::batch::BatchFrom::File(jadpo_std::batch::FileRows {
                        format: *format,
                        row: *row,
                        columns: row_columns(db, project, ty),
                        items_at: site.items_at.clone(),
                        max_unpacked: site.max_unpacked,
                    }),
                    run_stores.get(&site.starter).cloned().flatten()?,
                ),
                None => {
                    let read = queries.get(site.query? as usize)?.read.clone();
                    let store = read.store.clone();
                    (jadpo_std::batch::BatchFrom::Query(Box::new(read)), store)
                }
            };
            // `each:` calls an action, or gives a writer's row (chunk
            // 6.4b2).
            let (each, to) = match &site.write {
                Some(write) => (
                    jadpo_std::batch::BatchEach::Row(write.each.clone()),
                    Some(jadpo_std::batch::FileWrite {
                        format: write.format,
                        store: write.store.clone(),
                        row: write.row,
                        columns: row_columns(db, project, &write.row_ty),
                        name: write.name,
                        csv_format: write.csv_format,
                    }),
                ),
                None => (
                    jadpo_std::batch::BatchEach::Action(calling(site.each.clone()?)),
                    None,
                ),
            };
            let isolation = [&site.each, &site.finished]
                .into_iter()
                .flatten()
                .map(|(key, _)| {
                    graph
                        .get(key)
                        .and_then(|node| node.isolation())
                        .unwrap_or_default()
                })
                .max()
                .unwrap_or(Isolation::ReadCommitted);
            Some(jadpo_std::batch::BatchPlan {
                name: site.name,
                shape: site.shape,
                store,
                isolation,
                from,
                captured: site.captured,
                from_args: site.from_args,
                each,
                to,
                finished: site.finished.map(calling),
                key: site.key,
                tenant: jadpo_check::batches::run_tenant(
                    db,
                    project,
                    &site.saved_types,
                    site.key_type.as_ref(),
                ),
            })
        }) {
            // A call whose query or action has no plan is refused by the
            // checker; its site finds no batch.
            batch_numbers.push(plan.map(|plan| {
                batches.push(plan);
                (batches.len() - 1) as u32
            }));
        }
        // Each event type, numbered after every other so their numbers
        // don't move: the outbox keeps its payload as the type's contract
        // JSON, written and read by its codec (area 12; chunk 5.9b). No
        // job takes arguments yet (a schedule's `run:` takes none); a
        // batch's saved values were numbered with its site (chunk 6.4).
        let events = event_types(&graph)
            .into_iter()
            .filter(|key| used.as_ref().is_none_or(|used| used.events.contains(key)))
            .map(|key| {
                let ty = TypeRef::Node(key.clone());
                let number = match json_types.iter().position(|known| *known == ty) {
                    Some(at) => at,
                    None => {
                        json_types.push(ty);
                        json_types.len() - 1
                    }
                };
                jadpo_std::http::EventJson {
                    key: key.as_str().to_string(),
                    number: u32::try_from(number).unwrap_or(u32::MAX),
                }
            })
            .collect();
        // Each structured field the latest migration reads or writes as
        // JSON, by its field's type, as a query reads it (chunk 5.9c).
        let mut migration_json = Vec::new();
        if with_backfills {
            for stored in jadpo_check::data::migrations::stored_json_fields(db, project) {
                let Some(id) =
                    graph.named(Kind::Field, &format!("{}.{}", stored.entity, stored.name))
                else {
                    continue;
                };
                let ty = TypeRef::Field(graph.node(id).key.clone());
                let number = match json_types.iter().position(|known| *known == ty) {
                    Some(at) => at,
                    None => {
                        json_types.push(ty);
                        json_types.len() - 1
                    }
                };
                migration_json.push(MigrationJson {
                    migration: stored.migration,
                    field: stored.field,
                    number: u32::try_from(number).unwrap_or(u32::MAX),
                });
            }
        }
        // Each provider operation's input and output (area 11, chunk 7.2c).
        // A deployable keeps a recording of nothing, and the webhooks, polls
        // and queues only where it serves or runs them (chunk 8.1o3).
        let parts = match pruning {
            Some(pruning) => providers::Parts {
                recordings: false,
                webhooks: pruning.keep.serves(jadpo_check::webhooks::DEFAULT_PREFIX),
                polls: pruning.runs().schedules,
                queues: pruning.runs().subscribers,
            },
            None => providers::Parts::EVERY,
        };
        let (providers, recordings) = providers::plan(
            db,
            project,
            &mut types,
            &setting_number,
            &mut json_types,
            &connections,
            &numbering,
            parts,
        );
        let mut json = routes::json_table(db, project, &mut types, &json_types);
        json.events = events;
        let mut routes =
            routes::table(db, project, &mut types, &callable_number, &queries, pruning);
        routes.identity = identity;
        // A deployable keeps a method's signing key only where an API it
        // serves takes that method, and the browser session's origin only
        // where one takes browser sessions, whose cookie writes it checks
        // and whose passkey and magic-link routes it serves (chunk 8.4c):
        // one serving none starts without them. Who signs in stays, for
        // the credentials an action issues.
        if pruning.is_some()
            && let Some(identity) = routes.identity.as_mut()
        {
            let taken = |method: jadpo_std::AuthenticationMethod| {
                routes.apis.iter().any(|api| api.methods.contains(&method))
            };
            identity.signing_keys.retain(|(method, _)| taken(*method));
            if !taken(jadpo_std::AuthenticationMethod::ExternalToken) {
                identity.external_token = None;
            }
            if !taken(jadpo_std::AuthenticationMethod::BrowserSession) {
                identity.origin = None;
            }
        }
        // The action `on_first_sign_in:` names, by its callable's number
        // (area 8, "A person the app has not seen"), where a deployable
        // keeps it: one that serves no API redeems no link.
        if let Some(magic) = routes
            .identity
            .as_mut()
            .and_then(|identity| identity.magic_links.as_mut())
            && let Some(name) = &jadpo_check::identity::first_sign_in_action(db, project).0
        {
            let lookup = jadpo_check::lookups::Lookup::new(db, project);
            magic.first_sign_in = jadpo_check::graph::declaration_key(&lookup, name)
                .filter(|key| {
                    pruning.is_none_or(|pruning| {
                        pruning.keep.callables.contains(key)
                            || pruning
                                .keep
                                .serves
                                .iter()
                                .any(|api| api != jadpo_check::webhooks::DEFAULT_PREFIX)
                    })
                })
                .and_then(|key| callable_number(&key));
        }
        let running = pruning.is_none_or(|pruning| pruning.runs().subscribers);
        let events = events_plan(
            db,
            project,
            &callable_number,
            &numbering,
            &setting_number,
            running,
        );
        let schedules = schedules_plan(
            db,
            project,
            &callable_number,
            &purges,
            pruning.is_none_or(|pruning| pruning.runs().schedules),
        );
        let build = graph.digest();
        // The key a record of several changing calls is sealed with, which
        // a deployable keeps only when it keeps such an action.
        let steps_key = jadpo_check::types::steps_settings(db, project)
            .encryption_key
            .as_ref()
            .filter(|_| {
                pruning.is_none() || callables.iter().any(|callable| callable.record.is_some())
            })
            .and_then(|name| setting_number(name));
        routes.client_address = jadpo_check::client_address::client_address(db, project)
            .plan
            .clone();
        routes.rate_limits = jadpo_check::rate_limits::store_plan(db, project);
        // The connection `ai:` names, its address and its models (area 21;
        // chunk 5.5v3).
        // A deployable keeps it only when it reaches the model (chunk
        // 8.1o3).
        let asks_a_model = jadpo_check::ai::settings(db, project)
            .connection
            .as_ref()
            .is_some_and(|name| {
                pruning.is_none_or(|pruning| pruning.keep.connections.contains(name))
            });
        let ai = if !asks_a_model {
            None
        } else {
            let settings = jadpo_check::ai::settings(db, project);
            let connections = jadpo_check::ai::model_connections(db, project);
            let number = &setting_number;
            // What its calls may spend, and where that is counted
            // (chunk 5.5v5).
            let limits = settings.limits.store.as_ref().and_then(|store| {
                Some(jadpo_std::ai::AiLimits {
                    store: store.clone(),
                    per_request: settings.limits.per_request,
                    per_day: settings.limits.per_day?,
                    cost: match &settings.limits.cost {
                        Some(setting) => Some(number(setting)?),
                        None => None,
                    },
                    per_caller_per_day: settings.limits.per_caller_per_day,
                })
            });
            // The prices it sets for its models (chunk 5.5v7).
            let prices: Vec<jadpo_std::ai::AiPriceSetting> = settings
                .prices
                .iter()
                .filter_map(|price| {
                    Some(jadpo_std::ai::AiPriceSetting {
                        model: price.model.clone(),
                        input: number(&price.input)?,
                        cached: match &price.cached {
                            Some(setting) => Some(number(setting)?),
                            None => None,
                        },
                        output: number(&price.output)?,
                    })
                })
                .collect();
            settings.connection.as_ref().and_then(|name| {
                let models = settings
                    .models
                    .iter()
                    .filter_map(|(model, setting)| Some((model.clone(), number(setting)?)))
                    .collect();
                if let Some(connection) = connections.list.iter().find(|found| found.name == *name)
                {
                    let url = match connection.url.as_ref()? {
                        jadpo_check::ai::ModelUrl::Setting(setting) => {
                            jadpo_std::ai::AiUrl::Setting(number(setting)?)
                        }
                        jadpo_check::ai::ModelUrl::Written(text) => {
                            jadpo_std::ai::AiUrl::Written(text.clone())
                        }
                    };
                    return Some(jadpo_std::ai::AiPlan {
                        connection: name.clone(),
                        driver: jadpo_std::ai::AiDriver::OpenAiCompatible(url),
                        models,
                        schema: jadpo_std::ai::SchemaMode::StrictSubset,
                        limits,
                        prices,
                    });
                }
                // A package filling `ai` (chunk 5.5v3b).
                let connection = jadpo_check::connections::package_connections(db, project)
                    .connections
                    .iter()
                    .find(|found| found.name == *name)?;
                let mapping = jadpo_check::ai_mapping::mappings(db, project)
                    .of
                    .get(&connection.folder)?;
                let called =
                    |operation: &str| numbering.number(name, operation).unwrap_or(u32::MAX);
                let package = ai_package(mapping, connection, &number, &called)?;
                Some(jadpo_std::ai::AiPlan {
                    connection: name.clone(),
                    driver: jadpo_std::ai::AiDriver::Package(Box::new(package)),
                    models,
                    schema: mapping.schema,
                    limits,
                    prices,
                })
            })
        };
        // Every recording of a prompt's answer (chunk 5.5v4), decoded by
        // `jadpo test` through each type the prompt's calls ask for.
        let prompt_recordings = {
            let analysis = jadpo_check::prompts::analysis(db, project);
            jadpo_check::prompt_recordings::recordings(db, project)
                .iter()
                .filter_map(|found| {
                    let text = found.text.clone().ok()?;
                    let beside =
                        jadpo_check::prompt_recordings::beside_its_prompt(db, project, found);
                    let mut answers: Vec<u32> = Vec::new();
                    for callable in &callables {
                        for site in &callable.sites {
                            if let Decoded::Generate(generate) = &site.decoded
                                && generate.prompt == found.prompt
                                && !answers.contains(&generate.answer)
                            {
                                answers.push(generate.answer);
                            }
                        }
                    }
                    Some(jadpo_std::embed::PromptRecording {
                        file: found.file.clone(),
                        prompt: found.prompt.clone(),
                        name: found.name.clone(),
                        version: beside
                            .then(|| analysis.prompt(&found.prompt))
                            .flatten()
                            .and_then(|facts| {
                                facts
                                    .digest
                                    .as_ref()
                                    .map(|digest| format!("{}@{digest}", facts.key))
                            }),
                        answers,
                        text,
                    })
                })
                .collect()
        };
        testing.operation_recordings = recordings.operations;
        testing.recorded = recordings.recorded;
        testing.prompt_recordings = prompt_recordings;
        let (validators, shapes, failures) = types.take_tables();
        // What compiled code's sites name by number (chunk 8.1f): each
        // outside call's site, numbered in callable and site order.
        let mut calls = Vec::new();
        let mut call_numbers = HashMap::new();
        for (number, callable) in callables.iter().enumerate() {
            for (at, site) in callable.sites.iter().enumerate() {
                use jadpo_std::compiled::CallPlan;
                let call = match &site.decoded {
                    Decoded::Provider(call) => CallPlan::Provider(call.clone()),
                    Decoded::Generate(generate) => CallPlan::Generate((**generate).clone()),
                    Decoded::Stream(stream) => CallPlan::Stream((**stream).clone()),
                    Decoded::Embed(embed) => CallPlan::Embed((**embed).clone()),
                    _ => continue,
                };
                let key = (
                    u32::try_from(number).unwrap_or(u32::MAX),
                    u32::try_from(at).unwrap_or(u32::MAX),
                );
                call_numbers.insert(key, u32::try_from(calls.len()).unwrap_or(u32::MAX));
                calls.push(call);
            }
        }
        let sites = jadpo_std::compiled::SitesPlan {
            constants: constants.clone(),
            queries: queries.iter().map(|query| query.read.clone()).collect(),
            gets: entities.iter().map(|entity| entity.get.clone()).collect(),
            writes: writes.iter().map(|write| write.plan.clone()).collect(),
            calls,
        };
        // What the boundary needs of each action and fixture.
        let actions = callables
            .iter()
            .enumerate()
            .filter(|(_, callable)| callable.is_action())
            .map(|(number, callable)| {
                (
                    u32::try_from(number).unwrap_or(u32::MAX),
                    jadpo_std::compiled::ActionPlan {
                        key: callable.key.as_str().to_string(),
                        isolation: callable.isolation,
                        gate: callable.gate.clone(),
                        outbox: callable.outbox.clone(),
                        record: callable.record.clone(),
                    },
                )
            })
            .collect();
        let runtime = RuntimePlan {
            validators,
            shapes,
            failures,
            json,
            config,
            routes,
            events,
            batches,
            schedules,
            purges,
            connections,
            providers,
            ai,
            prompts,
            locales: jadpo_check::locales::locales(db, project).plan.clone(),
            translations: jadpo_check::translations::plan(db, project),
            callables: keyed,
            sites,
            actions,
            build: build.clone(),
            steps_key,
            runs: jadpo_std::Runs::EVERYTHING,
            testing,
        };
        Plan {
            callables,
            by_key,
            types,
            constants,
            files,
            queries,
            entities,
            writes,
            runtime: Arc::new(runtime),
            backfills,
            migration_json,
            batch_numbers,
            steps_key,
            build,
            call_numbers,
        }
    }

    /// The number compiled code names the outside call at site `site` of
    /// callable `callable` by: its place in the runtime plan's calls
    /// (chunk 8.1f).
    pub fn call_number(&self, callable: u32, site: u32) -> Option<u32> {
        self.call_numbers.get(&(callable, site)).copied()
    }

    /// The digest of the graph a record of several changing calls keeps
    /// as the build it started under (chunk 7.5b).
    pub fn build_id(&self) -> &str {
        &self.build
    }

    /// The same plan built as `build`, area 18's build id, which a
    /// deployable records in place of the graph's digest (chunk 8.1o).
    pub fn with_build(mut self, build: String) -> Plan {
        Arc::make_mut(&mut self.runtime).build = build.clone();
        self.build = build;
        self
    }

    /// The `SigningKey` setting a record's answers are encrypted with, by
    /// its number (area 12; chunk 7.5b).
    pub fn steps_key(&self) -> Option<u32> {
        self.steps_key
    }

    /// What the runtime reads of the program (chunk 8.1a), which every
    /// part below that the runtime runs with is read from.
    pub fn runtime(&self) -> &RuntimePlan {
        &self.runtime
    }

    /// The same, shared: what each thread's runtime loads (chunk 8.1a).
    pub fn shared_runtime(&self) -> &Arc<RuntimePlan> {
        &self.runtime
    }

    /// Every validator plan, by number.
    pub fn validators(&self) -> &[ValidatorPlan] {
        &self.runtime.validators
    }

    /// Every shape plan, by number.
    pub fn shapes(&self) -> &[ShapePlan] {
        &self.runtime.shapes
    }

    /// Every failure plan, by number: what the runtime raises a missing
    /// record's failure from.
    pub fn failures(&self) -> &[FailurePlan] {
        &self.runtime.failures
    }

    pub fn failure(&self, id: FailureId) -> Option<&FailurePlan> {
        self.runtime.failures.get(id.0 as usize)
    }

    pub fn shape(&self, id: ShapeId) -> Option<&ShapePlan> {
        self.runtime.shapes.get(id.0 as usize)
    }

    /// The operations called through the connections to provider
    /// packages, their webhooks, queues and polls (area 11, chunk 7.2c).
    pub fn providers(&self) -> &jadpo_std::provider::ProviderPlan {
        &self.runtime.providers
    }

    /// The connection every `ai` call goes through, and its models (area
    /// 21; chunk 5.5v3).
    pub fn ai(&self) -> Option<&jadpo_std::ai::AiPlan> {
        self.runtime.ai.as_ref()
    }

    /// Every recording of a prompt's answer (chunk 5.5v4), in the dev
    /// and test section (chunk 8.1e).
    pub fn prompt_recordings(&self) -> &[jadpo_std::embed::PromptRecording] {
        &self.runtime.testing.prompt_recordings
    }

    /// The dev and test section (chunk 8.1e): each test's key and what it
    /// gives itself, and the recordings.
    pub fn testing(&self) -> &jadpo_std::testing::TestingPlan {
        &self.runtime.testing
    }

    /// Every `batches.each` call's run (area 12, chunk 6.4).
    pub fn batches(&self) -> &[jadpo_std::batch::BatchPlan] {
        &self.runtime.batches
    }

    /// Every connection, in name order, with its driver, the settings it
    /// is built from and the targets it runs on (area 11; chunk 8.1d).
    pub fn connections(&self) -> &[jadpo_std::connections::Connection] {
        &self.runtime.connections
    }

    /// The name of the connection numbered `at` in [`Plan::connections`].
    pub fn connection_name(&self, at: u32) -> Option<&str> {
        self.runtime
            .connections
            .get(at as usize)
            .map(|connection| connection.name.as_str())
    }

    /// The connections to provider packages, each with its number and
    /// name, and how calls through it are made (area 11, chunk 7.2c).
    pub fn package_connections(
        &self,
    ) -> impl Iterator<Item = (u32, &str, &jadpo_std::provider::ConnectionPlan)> {
        jadpo_std::connections::packages(&self.runtime.connections)
    }

    /// The connection to a provider package called `name`.
    pub fn package_connection(&self, name: &str) -> Option<&jadpo_std::provider::ConnectionPlan> {
        let (at, _) = jadpo_std::connections::named(&self.runtime.connections, name)?;
        jadpo_std::connections::package(&self.runtime.connections, at)
    }

    /// The run of the `batches.each` call whose site decodes `number`.
    pub fn batch(&self, number: u32) -> Option<&jadpo_std::batch::BatchPlan> {
        self.runtime.batches.get(self.batch_place(number)? as usize)
    }

    /// The place in [`Plan::batches`] of the run of the `batches.each` call
    /// whose site decodes `number`, which compiled code names it by.
    pub fn batch_place(&self, number: u32) -> Option<u32> {
        *self.batch_numbers.get(number as usize)?
    }

    /// Every subscriber, and where an action that writes no store keeps
    /// its events (area 12, chunk 6.1b).
    pub fn events(&self) -> &jadpo_std::subscriber::EventsPlan {
        &self.runtime.events
    }

    /// Every schedule, where their state is kept, and the stores the tick
    /// sweeps (area 12, chunk 6.2).
    pub fn schedules(&self) -> &jadpo_std::schedule::SchedulesPlan {
        &self.runtime.schedules
    }

    /// The structured fields the latest migration reads or writes as JSON
    /// in Jadpo, in a plan built [`Plan::with_backfills`] (chunk 5.9c).
    pub fn migration_json_fields(&self) -> &[MigrationJson] {
        &self.migration_json
    }

    /// The latest migration's backfills worked out in Jadpo, in a plan
    /// built [`Plan::with_backfills`].
    pub fn backfills(&self) -> &[BackfillPlan] {
        &self.backfills
    }

    /// Every callable with a body, by number.
    pub fn callables(&self) -> &[CallablePlan] {
        &self.callables
    }

    pub fn callable(&self, id: u32) -> Option<&CallablePlan> {
        self.callables.get(id as usize)
    }

    /// The number of the callable with key `key`.
    pub fn id(&self, key: &Key) -> Option<u32> {
        self.by_key.get(key).copied()
    }

    /// The callable a key's text or a name means: `function line_total`,
    /// `line_total`, `rounding.half_even`, or a test's key,
    /// `test main "a large size is rejected"`.
    pub fn find(&self, name: &str) -> Option<u32> {
        // A function declared in a domain is keyed by its domain too,
        // `function billing.format_address` (area 18): its name alone
        // finds it while only one domain declares one.
        let in_domain = |callable: &CallablePlan| {
            callable.kind == Kind::Function
                && callable
                    .key
                    .name()
                    .split_once('.')
                    .is_some_and(|(domain, own)| {
                        own == name && callable.file.starts_with(&format!("domains/{domain}/"))
                    })
        };
        let at = self
            .callables
            .iter()
            .position(|callable| callable.key.as_str() == name)
            .or_else(|| {
                self.callables
                    .iter()
                    .position(|callable| callable.key.name() == name)
            })
            .or_else(|| {
                let mut found = self
                    .callables
                    .iter()
                    .enumerate()
                    .filter(|(_, callable)| in_domain(callable));
                let (at, _) = found.next()?;
                found.next().is_none().then_some(at)
            })?;
        u32::try_from(at).ok()
    }

    pub fn types(&self) -> &Types {
        &self.types
    }

    /// Every decoded literal, by number.
    pub fn constants(&self) -> &[Constant] {
        &self.constants
    }

    /// Each file's text, by project-relative path.
    pub fn files(&self) -> &BTreeMap<String, String> {
        &self.files
    }

    pub fn text(&self, path: &str) -> Option<&str> {
        self.files.get(path).map(String::as_str)
    }

    /// Every query's plan, by number.
    pub fn queries(&self) -> &[QueryPlan] {
        &self.queries
    }

    pub fn query(&self, id: u32) -> Option<&QueryPlan> {
        self.queries.get(id as usize)
    }

    /// The plan of the query called `name` now.
    pub fn query_named(&self, name: &str) -> Option<&QueryPlan> {
        self.queries.iter().find(|plan| plan.name == name)
    }

    /// Every entity's operations, by number.
    pub fn entities(&self) -> &[EntityPlan] {
        &self.entities
    }

    pub fn entity(&self, id: u32) -> Option<&EntityPlan> {
        self.entities.get(id as usize)
    }

    /// Every write site's plan, by number.
    pub fn writes(&self) -> &[WriteSitePlan] {
        &self.writes
    }

    pub fn write(&self, id: u32) -> Option<&WriteSitePlan> {
        self.writes.get(id as usize)
    }

    /// Every lifecycle's purge, by entity name (area 5): what the built-in
    /// purge schedule runs (area 12).
    pub fn purges(&self) -> &[PurgePlan] {
        &self.runtime.purges
    }

    /// Every route and the codecs its types use (area 7).
    pub fn routes(&self) -> &RouteTable {
        &self.runtime.routes
    }

    /// The settings and their codecs (area 10).
    pub fn config(&self) -> &jadpo_std::config::ConfigPlan {
        &self.runtime.config
    }

    /// The translation keys and their messages (area 10).
    pub fn translations(&self) -> &jadpo_std::translation::TranslationPlan {
        &self.runtime.translations
    }

    /// Every prompt that checks, by its place (area 21).
    pub fn prompts(&self) -> &jadpo_std::prompt::PromptPlan {
        &self.runtime.prompts
    }

    /// The locales the application supports, when it declares them.
    pub fn locales(&self) -> Option<&jadpo_std::LocalePlan> {
        self.runtime.locales.as_ref()
    }

    /// The codecs `json.read` and `json.write` run with (area 19).
    pub fn json(&self) -> &jadpo_std::http::JsonTable {
        &self.runtime.json
    }

    /// The purge of the entity called `entity`, when its lifecycle has one.
    pub fn purge(&self, entity: &str) -> Option<&PurgePlan> {
        self.runtime
            .purges
            .iter()
            .find(|plan| plan.entity == entity)
    }
}

/// The column each of a file row type's fields is read from, in its
/// fields' order: its `column:`, else its name (chunk 6.4b).
fn row_columns(db: &dyn Db, project: Project, ty: &TypeRef) -> Vec<String> {
    use jadpo_check::types::TypeView;
    let TypeRef::Node(key) = ty else {
        return Vec::new();
    };
    let lookup = jadpo_check::lookups::Lookup::new(db, project);
    let def = lookup
        .def(key.written())
        .or_else(|| lookup.def(key.name()))
        .cloned();
    match def {
        Some(jadpo_check::types::Def::Fields { fields, .. }) => fields
            .iter()
            .map(|field| field.column.clone().unwrap_or_else(|| field.name.clone()))
            .collect(),
        _ => Vec::new(),
    }
}

/// A package's `provide ai` as the runtime runs it through `connection`
/// (chunk 5.5v3b): each setting of the package it reads given by the
/// connection, from a setting of the app's or written out.
fn ai_package(
    mapping: &jadpo_check::ai_mapping::AiMapping,
    connection: &jadpo_check::connections::PackageConnection,
    number: &dyn Fn(&str) -> Option<u32>,
    called: &dyn Fn(&str) -> u32,
) -> Option<jadpo_std::ai::PackageAi> {
    use jadpo_check::ai_mapping::MappedInput;
    use jadpo_check::connections::Given;
    use jadpo_std::ai::AiInput;
    fn input(
        mapped: &MappedInput,
        connection: &jadpo_check::connections::PackageConnection,
        number: &dyn Fn(&str) -> Option<u32>,
    ) -> Option<AiInput> {
        Some(match mapped {
            MappedInput::Model => AiInput::Model,
            MappedInput::MaxOutput => AiInput::MaxOutput,
            MappedInput::Instructions => AiInput::Instructions,
            MappedInput::Role => AiInput::Role,
            MappedInput::Content => AiInput::Content,
            MappedInput::Texts => AiInput::Texts,
            MappedInput::Dimensions => AiInput::Dimensions,
            MappedInput::Text(text) => AiInput::Text(text.clone()),
            MappedInput::Int(n) => AiInput::Int(*n),
            MappedInput::Bool(flag) => AiInput::Bool(*flag),
            MappedInput::Messages(parts) => AiInput::Messages(
                parts
                    .iter()
                    .map(|(member, part)| Some((member.clone(), input(part, connection, number)?)))
                    .collect::<Option<_>>()?,
            ),
            MappedInput::Setting(setting) => match connection
                .settings
                .iter()
                .find(|(name, _)| name == setting)
                .map(|(_, given)| given)?
            {
                Given::Setting(name) => AiInput::Setting(number(name)?),
                Given::Text(text) => AiInput::Text(text.clone()),
                Given::Int(n) => AiInput::Int(*n),
                Given::Bool(flag) => AiInput::Bool(*flag),
                Given::Other => return None,
            },
        })
    }
    let inputs = |given: &[(String, MappedInput)]| -> Option<Vec<(String, AiInput)>> {
        given
            .iter()
            .map(|(member, mapped)| Some((member.clone(), input(mapped, connection, number)?)))
            .collect()
    };
    let stream = match &mapping.stream {
        Some(stream) => Some(jadpo_std::ai::PackageStream {
            operation: stream.operation.clone(),
            called: called(&stream.operation),
            input: inputs(&stream.input)?,
            instructions_apart: stream.instructions_apart,
            pieces: stream.pieces.clone(),
            refused_when: stream.refused_when.clone(),
            cut_off_when: stream.cut_off_when.clone(),
            done_when: stream.done_when.clone(),
        }),
        None => None,
    };
    let embed = match &mapping.embed {
        Some(embed) => Some(jadpo_std::ai::PackageEmbed {
            operation: embed.operation.clone(),
            called: called(&embed.operation),
            input: inputs(&embed.input)?,
            vectors: embed.vectors.clone(),
            vector: embed.vector.clone(),
            index: embed.index.clone(),
            batch: embed.batch,
            usage: Some(jadpo_std::ai::AiUsage {
                input: embed.usage.input.clone(),
                output: embed.usage.output.clone(),
                cached: embed.usage.cached.clone(),
            }),
        }),
        None => None,
    };
    Some(jadpo_std::ai::PackageAi {
        operation: mapping.operation.clone(),
        called: called(&mapping.operation),
        input: inputs(&mapping.input)?,
        instructions_apart: mapping.instructions_apart,
        answer: mapping.answer.clone(),
        refused_when: mapping.refused_when.clone(),
        cut_off_when: mapping.cut_off_when.clone(),
        usage: Some(jadpo_std::ai::AiUsage {
            input: mapping.usage.input.clone(),
            output: mapping.usage.output.clone(),
            cached: mapping.usage.cached.clone(),
        }),
        schema_at: mapping.schema_at.clone(),
        envelope: mapping.envelope,
        contexts: mapping.contexts.clone(),
        stream,
        embed,
        prices: mapping.prices.clone(),
    })
}
