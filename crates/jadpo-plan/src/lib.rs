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
mod constant;
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
pub use jadpo_std::Isolation;
pub use jadpo_std::http::RouteTable;

pub use constant::Constant;
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
    /// For a test: the settings its `config:` changes, each by its number
    /// in the plan's settings with its value in a variable's spelling
    /// (area 13, "Configuration in tests").
    pub settings: Vec<(u32, String)>,
    /// For a test: where its `clock:` starts its clock, in milliseconds
    /// since 1970-01-01T00:00:00Z (area 13, "Test settings").
    pub clock: Option<i64>,
    /// For an action that emits, itself or through what it calls: the
    /// store its outbox is in when it begins no transaction of its own,
    /// the one it writes or the one `events: { store: }` names (area 12,
    /// chunk 6.1b).
    pub outbox: Option<String>,
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

/// Every event type the project declares, by key, in key order.
fn event_types(graph: &jadpo_graph::Graph) -> Vec<Key> {
    let mut keys: Vec<Key> = graph
        .nodes()
        .iter()
        .filter(|node| {
            node.kind == Kind::Type
                && matches!(
                    node.detail,
                    jadpo_graph::Detail::Type {
                        base: jadpo_graph::Base::Event,
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
/// kept in.
fn events_plan(
    db: &dyn Db,
    project: Project,
    by_key: &HashMap<Key, u32>,
) -> jadpo_std::subscriber::EventsPlan {
    use jadpo_check::events::Tenant;
    use jadpo_std::subscriber::{SubscriberPlan, Target, TenantFrom};
    let written = jadpo_check::types::written_stores(db, project);
    let subscribers = jadpo_check::events::subscribers(db, project)
        .iter()
        .filter_map(|model| {
            let callable = *by_key.get(&model.run)?;
            Some(SubscriberPlan {
                key: model.key.as_str().to_string(),
                name: model.name.clone(),
                event: model.event.as_str().to_string(),
                target: Target::Action {
                    callable,
                    args: model.args.clone(),
                },
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
                file: model.path.clone(),
                line: model.line,
            })
        })
        .collect();
    jadpo_std::subscriber::EventsPlan {
        subscribers,
        store: jadpo_check::events::events_settings(db, project)
            .store
            .clone(),
    }
}

/// The schedule table (area 12, chunk 6.2): each schedule that reads
/// cleanly, with the callable it runs, where their state is kept, and the
/// stores holding the runtime's own tables, which the tick sweeps.
fn schedules_plan(
    db: &dyn Db,
    project: Project,
    by_key: &HashMap<Key, u32>,
    purges: &[PurgePlan],
) -> jadpo_std::schedule::SchedulesPlan {
    use jadpo_check::schedules::ScheduleRun;
    use jadpo_std::schedule::{SchedulePlan, SchedulesPlan};
    let schedules: Vec<SchedulePlan> = jadpo_check::schedules::schedules(db, project)
        .iter()
        .filter_map(|model| {
            let run = match &model.run {
                ScheduleRun::Action(key) => {
                    jadpo_std::schedule::ScheduleRun::Action(*by_key.get(key)?)
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
    use jadpo_std::system::{IDEMPOTENCY_KEYS, SERVICE_KEYS, SESSIONS};
    SchedulesPlan {
        schedules,
        store,
        stores: holding(IDEMPOTENCY_KEYS.table).collect(),
        sessions: holding(SESSIONS.table).next(),
        keys: holding(SERVICE_KEYS.table).next(),
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
    /// Every lifecycle's purge, by entity name.
    purges: Vec<PurgePlan>,
    /// Every route, with the codecs its types cross the boundary with
    /// (area 7, chunk 4.1).
    routes: RouteTable,
    /// Every setting `config { }` declares, and the codecs their values
    /// are read by (area 10, chunk 5.1).
    config: jadpo_std::config::ConfigPlan,
    /// Every translation key and its message in each supported locale
    /// (area 10, chunk 5.3b), which the runtime formats a call from.
    translations: jadpo_std::translation::TranslationPlan,
    /// The locales the application supports, when it declares them (area
    /// 10, chunk 5.3c): what `locale.from_accept_language` chooses among.
    locales: Option<jadpo_std::LocalePlan>,
    /// The codecs `json.read` and `json.write` run with (area 19, chunk
    /// 5.5g): the wire's, for each type a call reads or writes.
    json: jadpo_std::http::JsonTable,
    /// Every prompt that checks, which `prompts.render` fills (area 21,
    /// chunk 5.5v2).
    prompts: jadpo_std::prompt::PromptPlan,
    /// Every subscriber, and the store an action that writes none keeps
    /// its events in (area 12, chunk 6.1b).
    events: jadpo_std::subscriber::EventsPlan,
    /// Every schedule, where their state is kept and the stores the tick
    /// sweeps (area 12, chunk 6.2).
    schedules: jadpo_std::schedule::SchedulesPlan,
    /// The latest migration's backfills `jadpo migrate apply` works out in
    /// Jadpo (chunk 5.6b2), in a plan built for it.
    backfills: Vec<BackfillPlan>,
    /// The structured fields the latest migration reads or writes as JSON
    /// in Jadpo (chunk 5.9c), in a plan built for it.
    migration_json: Vec<MigrationJson>,
    /// Every `batches.each` call, by the number its site decodes (area
    /// 12, chunk 6.4).
    batches: Vec<jadpo_std::batch::BatchPlan>,
    /// Each `batches.each` site's number's place in `batches`.
    batch_numbers: Vec<Option<u32>>,
    /// The connections that keep files (area 16, chunk 6.4b).
    file_stores: Vec<jadpo_std::files::FilesConnection>,
    /// The connections to provider packages and their operations (area
    /// 11, chunk 7.2c).
    providers: jadpo_std::provider::ProviderPlan,
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
        Plan::build_with(db, project, false)
    }

    /// The plan of `project` with the latest migration's backfills that
    /// `jadpo migrate apply` works out in Jadpo (chunk 5.6b2), each a
    /// callable of its own.
    pub fn with_backfills(db: &dyn Db, project: Project) -> Plan {
        Plan::build_with(db, project, true)
    }

    fn build_with(db: &dyn Db, project: Project, with_backfills: bool) -> Plan {
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
        // Numbered in key order, so the same project always numbers its
        // callables the same way; a key declared twice is an error the
        // checker reports, and the first is kept.
        found.sort_by(|a, b| a.0.cmp(&b.0));
        found.dedup_by(|a, b| a.0 == b.0);
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
        let mut types = Types::new(Arc::clone(&graph), rule_bodies).with_providers(
            jadpo_check::connections::package_connections(db, project)
                .connections
                .iter()
                .map(|connection| connection.folder.clone())
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
        let (queries, entities, identity) = queries::plans(db, project, &mut types, &json_types);
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
        let config = config::plan(db, project, &mut types);
        let setting_ids: HashMap<Key, u32> = config
            .fields
            .iter()
            .enumerate()
            .map(|(i, field)| {
                (
                    config::key(jadpo_check::graph::registry(db, project), &field.name),
                    i as u32,
                )
            })
            .collect();
        let (writes, write_ids) = writes::plans(db, project, &mut types, &bodies, &json_types);
        let gates = policy::gates(db, project, &mut types, &json_types);
        let model = jadpo_check::data::data_model(db, project);
        let purges: Vec<PurgePlan> = model
            .entities
            .iter()
            .filter_map(|entity| {
                purge::plan(model, entity, &|name| {
                    config
                        .fields
                        .iter()
                        .position(|field| field.name == name)
                        .and_then(|at| u32::try_from(at).ok())
                })
            })
            .collect();
        let mut constants = Vec::new();
        let mut json_types = json_types.into_inner();
        // Each prompt's entry, and the type of each field its text writes,
        // read from its type's field nodes (area 21).
        let prompts = jadpo_check::prompts::plan(db, project);
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
        let mut callables = Vec::with_capacity(found.len());
        // Each `batches.each` call, as its site decodes it (area 12).
        let mut batch_sites: Vec<sites::BatchSite> = Vec::new();
        for (key, kind, file, body) in found {
            let node = graph.get(&key);
            let name = node.map_or(key.name(), |node| node.name.as_str());
            let backfill = backfill_lines.get(&key);
            let label = match (kind, backfill) {
                (_, Some((field, _))) => format!("backfill {field}"),
                (Kind::Test, _) => format!("test \"{name}\""),
                (kind, _) => format!("{} {name}", kind.word()),
            };
            let line = node
                .and_then(|node| node.place.as_ref())
                .map_or(backfill.map_or(1, |(_, line)| *line), |place| place.lines.0);
            let isolation = node.and_then(|node| node.isolation()).unwrap_or_default();
            let text = files.get(&file).map(String::as_str).unwrap_or_default();
            let number = |key: &Key| by_key.get(key).copied();
            let query = |key: &Key| query_ids.get(key).copied();
            let entity = |key: &Key| entity_ids.get(key).copied();
            let write = |site: u32| write_ids.get(&(key.clone(), site)).copied();
            let sites = sites::Decoder {
                types: &mut types,
                constants: &mut constants,
                callable: &number,
                query: &query,
                entity: &entity,
                write: &write,
                setting: &|key: &Key| setting_ids.get(key).copied(),
                json: &mut json_types,
                prompt: &|key: &str| prompt_fields.get(key).cloned(),
                starter: &key,
                batches: &mut batch_sites,
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
            let settings = overrides
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
            let outbox = (kind == Kind::Action)
                .then(|| jadpo_check::events::outbox_store(db, project, key.name()))
                .flatten();
            callables.push(CallablePlan {
                clock,
                outbox,
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
                settings,
            });
        }
        // Each batch's run: the query it walks, the store it is kept in and
        // the level its batches begin their transactions at, repeatable
        // read when an action it calls loads a record (area 12).
        let mut batches: Vec<jadpo_std::batch::BatchPlan> = Vec::new();
        let mut batch_numbers: Vec<Option<u32>> = Vec::new();
        let run_stores = jadpo_check::types::file_run_stores(db, project);
        for plan in batch_sites.into_iter().map(|site| {
            // A query's pages, kept in its store; or a file's rows, kept
            // where the run's actions write (chunk 6.4b).
            let (from, store) = match &site.file {
                Some((format, row, ty)) => (
                    jadpo_std::batch::BatchFrom::File(jadpo_std::batch::FileRows {
                        format: *format,
                        row: *row,
                        columns: row_columns(db, project, ty),
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
                    jadpo_std::batch::BatchEach::Action(site.each.clone()?.1),
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
                finished: site.finished.map(|(_, call)| call),
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
        let providers = providers::plan(
            db,
            project,
            &mut types,
            &|name| {
                setting_ids
                    .get(&config::key(jadpo_check::graph::registry(db, project), name))
                    .copied()
            },
            &mut json_types,
        );
        let mut json = routes::json_table(db, project, &mut types, &json_types);
        json.events = events;
        let mut routes = routes::table(
            db,
            project,
            &mut types,
            &|key: &Key| by_key.get(key).copied(),
            &queries,
        );
        routes.identity = identity;
        let events = events_plan(db, project, &by_key);
        let schedules = schedules_plan(db, project, &by_key, &purges);
        routes.client_address = jadpo_check::client_address::client_address(db, project)
            .plan
            .clone();
        Plan {
            callables,
            by_key,
            types,
            constants,
            files,
            queries,
            entities,
            writes,
            purges,
            routes,
            config,
            translations: jadpo_check::translations::plan(db, project),
            locales: jadpo_check::locales::locales(db, project).plan.clone(),
            json,
            prompts,
            events,
            schedules,
            backfills,
            migration_json,
            batches,
            batch_numbers,
            file_stores: jadpo_check::data::files_connections(db, project).clone(),
            providers,
        }
    }

    /// The connections to provider packages and the operations called
    /// through them (area 11, chunk 7.2c).
    pub fn providers(&self) -> &jadpo_std::provider::ProviderPlan {
        &self.providers
    }

    /// Every `batches.each` call's run (area 12, chunk 6.4).
    pub fn batches(&self) -> &[jadpo_std::batch::BatchPlan] {
        &self.batches
    }

    /// The connections that keep files, which a host gives a driver for
    /// (area 16, chunk 6.4b).
    pub fn file_stores(&self) -> &[jadpo_std::files::FilesConnection] {
        &self.file_stores
    }

    /// The run of the `batches.each` call whose site decodes `number`.
    pub fn batch(&self, number: u32) -> Option<&jadpo_std::batch::BatchPlan> {
        let at = (*self.batch_numbers.get(number as usize)?)?;
        self.batches.get(at as usize)
    }

    /// Every subscriber, and where an action that writes no store keeps
    /// its events (area 12, chunk 6.1b).
    pub fn events(&self) -> &jadpo_std::subscriber::EventsPlan {
        &self.events
    }

    /// Every schedule, where their state is kept, and the stores the tick
    /// sweeps (area 12, chunk 6.2).
    pub fn schedules(&self) -> &jadpo_std::schedule::SchedulesPlan {
        &self.schedules
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
        &self.purges
    }

    /// Every route and the codecs its types use (area 7).
    pub fn routes(&self) -> &RouteTable {
        &self.routes
    }

    /// The settings and their codecs (area 10).
    pub fn config(&self) -> &jadpo_std::config::ConfigPlan {
        &self.config
    }

    /// The translation keys and their messages (area 10).
    pub fn translations(&self) -> &jadpo_std::translation::TranslationPlan {
        &self.translations
    }

    /// Every prompt that checks, by its place (area 21).
    pub fn prompts(&self) -> &jadpo_std::prompt::PromptPlan {
        &self.prompts
    }

    /// The locales the application supports, when it declares them.
    pub fn locales(&self) -> Option<&jadpo_std::LocalePlan> {
        self.locales.as_ref()
    }

    /// The codecs `json.read` and `json.write` run with (area 19).
    pub fn json(&self) -> &jadpo_std::http::JsonTable {
        &self.json
    }

    /// The purge of the entity called `entity`, when its lifecycle has one.
    pub fn purge(&self, entity: &str) -> Option<&PurgePlan> {
        self.purges.iter().find(|plan| plan.entity == entity)
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
