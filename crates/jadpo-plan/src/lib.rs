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
use jadpo_graph::{Key, Kind};
pub use jadpo_std::Isolation;
pub use jadpo_std::http::RouteTable;

pub use constant::Constant;
pub use jadpo_std::read::{MissingPlan, MissingValue};
pub use jadpo_std::write::{PurgePlan, Raises, Refusal};
pub use queries::{EntityPlan, QueryPlan};
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
}

impl Plan {
    /// The plan of `project`, which checked without errors: a project with
    /// errors has bodies with nothing to run where the errors are.
    pub fn build(db: &dyn Db, project: Project) -> Plan {
        let graph = Arc::clone(jadpo_check::graph::graph(db, project));
        let mut files = BTreeMap::new();
        let mut found: Vec<(Key, Kind, String, Arc<Body>)> = Vec::new();
        // What each test's `config:` changes, by the test's key.
        let mut overrides: HashMap<Key, Vec<(String, String)>> = HashMap::new();
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
                }
                found.push((key, kind, path.clone(), Arc::clone(&checked.body)));
            }
        }
        // Numbered in key order, so the same project always numbers its
        // callables the same way; a key declared twice is an error the
        // checker reports, and the first is kept.
        found.sort_by(|a, b| a.0.cmp(&b.0));
        found.dedup_by(|a, b| a.0 == b.0);
        let by_key: HashMap<Key, u32> = found
            .iter()
            .enumerate()
            .map(|(i, (key, ..))| (key.clone(), i as u32))
            .collect();
        let mut types = Types::new(Arc::clone(&graph));
        let (queries, entities, identity) = queries::plans(db, project, &mut types);
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
        let (writes, write_ids) = writes::plans(db, project, &mut types, &bodies);
        let gates = policy::gates(db, project, &mut types);
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
        let mut callables = Vec::with_capacity(found.len());
        for (key, kind, file, body) in found {
            let node = graph.get(&key);
            let name = node.map_or(key.name(), |node| node.name.as_str());
            let label = match kind {
                Kind::Test => format!("test \"{name}\""),
                kind => format!("{} {name}", kind.word()),
            };
            let line = node
                .and_then(|node| node.place.as_ref())
                .map_or(1, |place| place.lines.0);
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
            callables.push(CallablePlan {
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
        let mut routes = routes::table(
            db,
            project,
            &mut types,
            &|key: &Key| by_key.get(key).copied(),
            &queries,
        );
        routes.identity = identity;
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
        }
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
        let at = self
            .callables
            .iter()
            .position(|callable| callable.key.as_str() == name)
            .or_else(|| {
                self.callables
                    .iter()
                    .position(|callable| callable.key.name() == name)
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

    /// The purge of the entity called `entity`, when its lifecycle has one.
    pub fn purge(&self, entity: &str) -> Option<&PurgePlan> {
        self.purges.iter().find(|plan| plan.entity == entity)
    }
}
