//! Area 7's routes: each `routes` group read into a [`GroupModel`] (its
//! prefix, its API and each route's method, path, input, output, status,
//! deadline and the action or query it runs), with the problems found on
//! the way, then the project's routes checked together ([`table_problems`]:
//! two routes that could answer one request) and the APIs `app.jadpo`
//! declares ([`apis`]).
//!
//! A route is a thin door (area 7): it says which method and URL, what
//! comes in, what goes out and what runs. Its placeholders are typed in
//! its path and reach what it runs by name, its one `input:` comes from
//! the query string for `GET` and `DELETE` and the JSON body otherwise,
//! its `output:` is exactly what it runs returns, and a `GET` runs a named
//! query while every other method runs a named action.
//!
//! Each group is read by name ([`group_read`]), reading its file and the
//! names it uses, so an edit reads again only the group it is in or one
//! that names what changed; [`route_problems`] gives one file's share and
//! [`groups`] every model, for the graph, the plan, OpenAPI and the API
//! record ([`record`], [`openapi`]).

pub mod apis;
pub mod openapi;
pub mod record;
pub mod served;
pub mod wire;

use std::collections::BTreeSet;
use std::sync::Arc;

use jadpo_db::{Db, File, ItemKind, Project, Symbol, item_tree, location};
use jadpo_diag::{Diagnostic, Span, nearest, rule};
use jadpo_graph::{Key, Kind};
use jadpo_std::http::{Authentication, PrincipalKind, SessionOp};
use jadpo_std::{HttpMethod, RouteAuthentication, SuccessStatus};
use jadpo_syntax::ast::{self, AstNode, Decl, Expr, MemberItem};

pub use apis::{Api, Apis, api_problems, api_table};

use crate::lookups::{Lookup, Name, NameView, SigView, file_at};
use crate::resolve::{Names, Want};
use crate::std_lib::StdNames;
use crate::tree::{self, node_span};
use crate::types::{Def, FieldsBase, Prim, Ty, TypeRef, TypeView};

/// Why a route sends only an `Output` (`route.output_not_output`).
const OUTPUT_ONLY: &str = "A route returns only an `Output` type (area 2), a `List` or `Page` of one as a query gives them, a `FeatureCollection<G, P>` whose properties `P` hold only what an `Output` may (area 20), or an `AiStream`, a model's answer sent as it is written, so only fields declared for clients are ever sent.";

/// What a route entry may hold, in the order `jadpo fmt` writes them
/// (area 14, "Formatting").
pub const ROUTE_ITEMS: [&str; 8] = [
    "authentication",
    "rate_limit",
    "input",
    "output",
    "status",
    "deadline",
    "run",
    "settle",
];

/// What a route group holds besides its routes.
pub const GROUP_ITEMS: [&str; 2] = ["prefix", "api"];

/// The route items a later chunk builds, with what builds them.
const LATER_ITEMS: [(&str, &str); 1] = [(
    "rate_limit",
    "A route's own `rate_limit:` comes with the rest of area 16's rate limits; every route already has the default limits",
)];

/// Why a streaming route answers 200 and waits for nothing
/// (`route.stream_invalid`).
const STREAM_REASON: &str = "A route whose action returns an `AiStream` sends the model's answer as server-sent events as it is written, answering 200 once its first piece has come; it has no other success to report, and nothing to wait for with `settle:`.";

/// Whether a route sending `ty` streams a model's answer: an `AiStream`
/// (area 7, "Streaming a model's answer"; chunk 5.5v6).
pub fn streams(ty: &Ty) -> bool {
    matches!(ty, Ty::Opaque(name, args) if name == jadpo_std::ai::AI_STREAM && args.is_empty())
}

/// What `settle:` holds (`route.settle_invalid`).
const SETTLE_REASON: &str = "`settle:` holds `answer:`, a named query given `result` (what the action returns) or fields read through it, and `within:`, how long the route waits for the work its action set off, a duration shorter than the route's `deadline:`.";

/// One segment of a route's full path.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Segment {
    Fixed(String),
    /// A typed placeholder: its name and type.
    Param {
        name: String,
        ty: Ty,
    },
}

impl Segment {
    pub fn is_param(&self) -> bool {
        matches!(self, Segment::Param { .. })
    }
}

/// A path as a route writes it, placeholders by name: `/todos/{todo_id}`.
pub fn path_text(segments: &[Segment]) -> String {
    if segments.is_empty() {
        return "/".to_string();
    }
    segments
        .iter()
        .map(|segment| match segment {
            Segment::Fixed(text) => format!("/{text}"),
            Segment::Param { name, .. } => format!("/{{{name}}}"),
        })
        .collect()
}

/// What one argument of a route's call is made from.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ArgRoot {
    /// A placeholder, by name.
    Param(String),
    Input,
    /// `current_principal`, the signed-in caller (area 8); the variant a
    /// route selects is its [`RouteModel::caller`], left out of the path.
    Caller,
    /// `result`, what the action returned: only in `settle:`'s `answer:`
    /// (area 7, "Waiting for follow-on work").
    Result,
}

/// One argument: a placeholder or the input, or a field read through them.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Arg {
    pub root: ArgRoot,
    pub path: Vec<String>,
}

/// What a route runs.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Run {
    /// A named query, rather than an action.
    pub query: bool,
    /// What it calls as written: `todo_view`, or `sessions.tokens` for a
    /// generated query.
    pub name: String,
    pub args: Vec<Arg>,
    /// For a generated query (area 8, "Listing tokens and keys from a
    /// `GET`"): `sessions.tokens` or the service entity's `keys`, which
    /// no declaration holds.
    pub generated: Option<jadpo_std::http::SessionOp>,
}

/// How a route waits for the work its action set off (area 7, "Waiting
/// for follow-on work"; chunk 6.3).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Settle {
    /// The named query `answer:` runs, each argument `result` or a field
    /// read through it.
    pub answer: Run,
    /// The query's parameters' types, in order: what each argument is
    /// kept as beside an `Idempotency-Key`, for a repeat to run it again.
    pub params: Vec<Ty>,
    /// How long the route waits, from when the request came in.
    pub within_ms: u32,
}

/// One route, without positions.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct RouteModel {
    pub key: Key,
    pub method: HttpMethod,
    /// Its full path, its group's prefix first.
    pub segments: Vec<Segment>,
    /// Its `Input` type.
    pub input: Option<String>,
    /// Its output type; `None` for `no_content`.
    pub output: Option<Ty>,
    pub status: SuccessStatus,
    pub deadline_ms: u32,
    /// Whether it needs a signed-in caller (area 8).
    pub authentication: Authentication,
    /// The kind of caller it passes on (`current_principal.user`).
    pub caller: Option<PrincipalKind>,
    pub run: Run,
    /// What it waits for before it answers, with `settle:`; its `output:`
    /// is then what `answer:` gives.
    pub settle: Option<Settle>,
    /// The failures what it runs can produce, its `fails` list, in order,
    /// then those of `settle:`'s `answer:`.
    pub fails: Vec<String>,
    /// Its `///` description.
    pub doc: Option<String>,
}

impl RouteModel {
    pub fn path(&self) -> String {
        path_text(&self.segments)
    }

    /// `GET /todos/{todo_id}`.
    pub fn label(&self) -> String {
        format!("{} {}", self.method.word(), self.path())
    }

    /// Its placeholders, in path order.
    pub fn params(&self) -> impl Iterator<Item = (&str, &Ty)> {
        self.segments.iter().filter_map(|segment| match segment {
            Segment::Param { name, ty } => Some((name.as_str(), ty)),
            Segment::Fixed(_) => None,
        })
    }
}

/// One route group, without positions: the routes that read without a
/// problem.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct GroupModel {
    pub name: String,
    pub key: Key,
    pub path: String,
    pub domain: Option<String>,
    pub api: String,
    pub prefix: Vec<Segment>,
    pub routes: Vec<RouteModel>,
    /// Its `///` description.
    pub doc: Option<String>,
}

/// One group read from its file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GroupRead {
    pub path: String,
    pub model: Option<Arc<GroupModel>>,
    /// Each route that read cleanly: its label and where its method and
    /// path are written, for problems found across groups.
    pub spans: Vec<(String, Span)>,
    pub problems: Vec<Diagnostic>,
}

/// The project's route groups, in name order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Groups {
    pub groups: Vec<Arc<GroupModel>>,
}

impl Groups {
    /// Every route, with its group.
    pub fn routes(&self) -> impl Iterator<Item = (&GroupModel, &RouteModel)> {
        self.groups.iter().flat_map(|group| {
            group
                .routes
                .iter()
                .map(move |route| (group.as_ref(), route))
        })
    }

    pub fn is_empty(&self) -> bool {
        self.groups.iter().all(|group| group.routes.is_empty())
    }
}

/// The declared types an API's record names, as the type tables know them
/// ([`jadpo_db::Symbol::canonical`]): each route's input and output and
/// placeholders, and every type reached through their fields. Such a type
/// is an OpenAPI schema name, so its name stays unique across the project
/// even when it is private (area 1, "Files and names").
#[salsa::tracked(returns(ref))]
pub fn api_named(db: &dyn Db, project: Project) -> std::collections::BTreeSet<String> {
    use crate::types::{Def, TypeView};
    let table = crate::types::type_table(db, project);
    let contested = jadpo_db::contested(db, project);
    let mut pending: Vec<Ty> = Vec::new();
    for (group, route) in groups(db, project).routes() {
        if let Some(input) = &route.input {
            pending.push(Ty::Named(
                contested.canonical(input, group.domain.as_deref()),
            ));
        }
        pending.extend(route.output.iter().cloned());
        pending.extend(route.params().map(|(_, ty)| ty.clone()));
    }
    let mut out = std::collections::BTreeSet::new();
    let mut seen_fields: std::collections::BTreeSet<(String, Vec<String>)> = Default::default();
    while let Some(ty) = pending.pop() {
        match ty {
            Ty::Named(name) => {
                if !out.insert(name.clone()) {
                    continue;
                }
                match table.def(&name) {
                    Some(Def::Fields { fields, .. }) => {
                        pending.extend(fields.iter().map(|field| field.declared.clone()));
                    }
                    Some(Def::Enum { variants }) => pending.extend(
                        variants
                            .iter()
                            .flat_map(|variant| variant.fields.iter().flatten())
                            .map(|field| field.declared.clone()),
                    ),
                    Some(Def::Scalar { parent, .. }) => pending.push(parent.clone()),
                    _ => {}
                }
            }
            Ty::Field(host, path) => {
                if seen_fields.insert((host.clone(), path.clone())) {
                    if let Some(field) = table.field(&host, &path) {
                        pending.push(field.declared.clone());
                    }
                    pending.push(Ty::Named(host));
                }
            }
            Ty::List(inner) | Ty::Set(inner) | Ty::Nullable(inner) => pending.push(*inner),
            Ty::Map(key, value) => {
                pending.push(*key);
                pending.push(*value);
            }
            Ty::Opaque(_, args) => pending.extend(args),
            Ty::Variant(host, _) => pending.push(Ty::Named(host)),
            _ => {}
        }
    }
    out
}

/// The names of the project's route groups, in name order.
#[salsa::tracked(returns(ref))]
pub fn group_names(db: &dyn Db, project: Project) -> Vec<String> {
    jadpo_db::symbols(db, project)
        .declarations
        .iter()
        .filter(|(_, found)| found.iter().any(|s| s.kind == ItemKind::Routes))
        .map(|(name, _)| name.clone())
        .collect()
}

/// The group called `name`, read from the first file that declares one.
#[salsa::tracked(returns(ref))]
pub fn group_read<'db>(db: &'db dyn Db, project: Project, name: Name<'db>) -> Option<GroupRead> {
    let lookup = Lookup::new(db, project);
    let symbol = lookup
        .declared(name.text(db))
        .iter()
        .find(|s| s.kind == ItemKind::Routes)?
        .clone();
    let file = file_at(db, project, Name::new(db, symbol.path.clone()))?;
    let source = tree::source(db, file)?;
    let Some(Decl::RoutesDecl(decl)) = tree::decl_at(&source, symbol.index) else {
        return None;
    };
    let ownership = location(db, project, file).ownership.clone();
    // The group's own domain's private names are the ones it reads.
    let lookup = Lookup::within(db, project, &ownership);
    let names = Names {
        path: &symbol.path,
        ownership: &ownership,
        table: &lookup,
        types: &lookup,
        std: StdNames::get(),
        library: None,
    };
    let mut reader = Reader {
        db,
        project,
        lookup: &lookup,
        names: &names,
        caller: None,
        authentication: Authentication::Required,
        answering: None,
        apis: api_table(db, project),
        path: &symbol.path,
        out: Vec::new(),
        spans: Vec::new(),
    };
    let model = reader.group(&symbol, &decl).map(Arc::new);
    Some(GroupRead {
        path: symbol.path.clone(),
        model,
        spans: reader.spans,
        problems: reader.out,
    })
}

/// Every group's model, in name order.
#[salsa::tracked(returns(ref))]
pub fn groups(db: &dyn Db, project: Project) -> Arc<Groups> {
    Arc::new(Groups {
        groups: group_names(db, project)
            .iter()
            .filter_map(|name| {
                group_read(db, project, Name::new(db, name.clone()))
                    .as_ref()?
                    .model
                    .clone()
            })
            .collect(),
    })
}

/// What a test's `request` reads of the route it names (area 13): the
/// route's key, its placeholders, its input and output, and whether it
/// needs a caller.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct RouteShape {
    pub key: Key,
    pub method: HttpMethod,
    pub segments: Vec<Segment>,
    pub input: Option<String>,
    pub output: Option<Ty>,
    pub needs_caller: bool,
}

impl RouteShape {
    /// `POST /todos`.
    pub fn label(&self) -> String {
        format!("{} {}", self.method.word(), path_text(&self.segments))
    }
}

/// The route a request for `label` (`POST /todos`, its path as `jadpo
/// routes` prints it) reaches, read by label (chunk 4.11): a test body
/// depends on the routes it sends a request to, and on nothing else of
/// the routes.
#[salsa::tracked(returns(ref))]
pub fn route_at<'db>(db: &'db dyn Db, project: Project, label: Name<'db>) -> Option<RouteShape> {
    groups(db, project)
        .routes()
        .map(|(_, route)| route)
        .find(|route| route.label() == *label.text(db))
        .map(|route| RouteShape {
            key: route.key.clone(),
            method: route.method,
            segments: route.segments.clone(),
            input: route.input.clone(),
            output: route.output.clone(),
            needs_caller: route.authentication.needs_caller(),
        })
}

/// Whether every group read without a problem: what the API record and
/// OpenAPI are written from.
#[salsa::tracked(returns(copy))]
pub fn routes_clean(db: &dyn Db, project: Project) -> bool {
    api_problems(db, project).is_empty()
        && table_problems(db, project).is_empty()
        && signin_problems(db, project).is_empty()
        && group_names(db, project).iter().all(|name| {
            group_read(db, project, Name::new(db, name.clone()))
                .as_ref()
                .is_none_or(|read| read.problems.is_empty() && read.model.is_some())
        })
}

/// Area 7's problems in one file: those of the groups it declares.
#[salsa::tracked(returns(ref))]
pub fn route_problems(db: &dyn Db, project: Project, file: File) -> Vec<Diagnostic> {
    let path = file.path(db);
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    for item in &item_tree(db, file).items {
        if item.kind != ItemKind::Routes || !seen.insert(item.name.as_str()) {
            continue;
        }
        let Some(read) = group_read(db, project, Name::new(db, item.name.clone())) else {
            continue;
        };
        if read.path == *path {
            out.extend(read.problems.iter().cloned());
        }
    }
    out
}

/// Problems across groups: two routes that could answer one request (area
/// 7: "every URL matches exactly one group"). A fixed segment beats a
/// placeholder, so `GET /todos/archived` sits beside `GET
/// /todos/{todo_id}`; two placeholders, or the same fixed text, in every
/// segment, with one method, are two answers to one request.
#[salsa::tracked(returns(ref))]
pub fn table_problems(db: &dyn Db, project: Project) -> Vec<Diagnostic> {
    let mut routes: Vec<(String, &RouteModel, Span)> = Vec::new();
    let names = group_names(db, project);
    let reads: Vec<&GroupRead> = names
        .iter()
        .filter_map(|name| group_read(db, project, Name::new(db, name.clone())).as_ref())
        .collect();
    for read in &reads {
        let Some(model) = &read.model else { continue };
        for route in &model.routes {
            let Some((_, span)) = read.spans.iter().find(|(label, _)| *label == route.label())
            else {
                continue;
            };
            routes.push((model.name.clone(), route, span.clone()));
        }
    }
    // In file order, so the later route is the one reported.
    routes.sort_by(|a, b| (&a.2.path, a.2.start).cmp(&(&b.2.path, b.2.start)));
    let mut out = Vec::new();
    let exchanges = crate::identity::identity(db, project).exchanges();
    let exchange: Vec<Segment> = jadpo_std::http::EXCHANGE_PATH
        .iter()
        .map(|text| Segment::Fixed(text.to_string()))
        .collect();
    // Each API's OpenAPI document (area 7, "OpenAPI").
    let documents = served::served(db, project);
    for (i, (group, route, span)) in routes.iter().enumerate() {
        let document = documents.iter().find(|served| {
            let segments: Vec<Segment> = served
                .segments
                .iter()
                .map(|text| Segment::Fixed(text.clone()))
                .collect();
            route.method == jadpo_std::HttpMethod::Get && overlaps(&route.segments, &segments)
        });
        if let Some(served) = document {
            out.push(
                Diagnostic::new(
                    &rule::ROUTE_PATH_OVERLAP,
                    format!("`{}` is where API `{}` serves its OpenAPI document", route.label(), served.api),
                )
                .at(span.clone(), "answered by the runtime")
                .reason(format!(
                    "Each API serves its OpenAPI document at `{}` unless its entry in `apis:` says `serve_openapi: false`, so this route would never run.",
                    served.path()
                ))
                .next(format!("Give `{}` in `{group}` another path", route.label())),
            );
            continue;
        }
        // The runtime answers the generated exchange (area 8, "Ways to
        // sign in").
        if exchanges
            && route.method == jadpo_std::HttpMethod::Post
            && overlaps(&route.segments, &exchange)
        {
            out.push(
                Diagnostic::new(
                    &rule::ROUTE_PATH_OVERLAP,
                    format!("`{}` is the runtime's key exchange", route.label()),
                )
                .at(span.clone(), "answered by the runtime")
                .reason("With `revocation_delay:` and service keys, the runtime answers `POST /auth/exchange` itself, trading a service key for a short-lived token, so this route would never run.")
                .next(format!("Give `{}` in `{group}` another path", route.label())),
            );
            continue;
        }
        // The runtime answers the routes passkeys generate (area 8,
        // "Passkeys").
        if let Some(passkey) = crate::identity::identity(db, project).passkeys() {
            let generated = jadpo_std::http::PasskeyStep::ALL.into_iter().find(|step| {
                let segments: Vec<Segment> = step
                    .segments(&passkey.path)
                    .into_iter()
                    .map(|text| {
                        if text == jadpo_std::http::PASSKEY_ID {
                            Segment::Param {
                                name: "passkey_id".to_string(),
                                ty: Ty::Error,
                            }
                        } else {
                            Segment::Fixed(text)
                        }
                    })
                    .collect();
                route.method == step.method() && overlaps(&route.segments, &segments)
            });
            if let Some(step) = generated {
                out.push(
                    Diagnostic::new(
                        &rule::ROUTE_PATH_OVERLAP,
                        format!("`{}` is the runtime's {}", route.label(), step.label()),
                    )
                    .at(span.clone(), "answered by the runtime")
                    .reason("With `passkey:` in `sign_in:`, the runtime answers the passkey routes under its `path:` itself, so this route would never run.")
                    .next(format!(
                        "Give `{}` in `{group}` another path, or move the passkey routes with `path:`",
                        route.label()
                    )),
                );
                continue;
            }
        }
        // The runtime answers each connection's webhook itself, whatever
        // the method (area 11, "Receiving from outside"; chunk 7.7).
        let webhook = crate::webhooks::webhooks(db, project)
            .routes
            .iter()
            .find(|webhook| {
                let segments: Vec<Segment> = webhook
                    .segments
                    .iter()
                    .map(|text| Segment::Fixed(text.clone()))
                    .collect();
                overlaps(&route.segments, &segments)
            });
        if let Some(webhook) = webhook {
            out.push(
                Diagnostic::new(
                    &rule::ROUTE_PATH_OVERLAP,
                    format!("`{}` is where `{}` receives its webhooks", route.label(), webhook.connection),
                )
                .at(span.clone(), "answered by the runtime")
                .reason(format!(
                    "A connection given a webhook secret has `POST {}`, which the runtime answers itself, so this route would never run.",
                    webhook.path()
                ))
                .next(format!("Give `{}` in `{group}` another path", route.label())),
            );
            continue;
        }
        // The runtime answers the routes magic links generate (area 8,
        // "Magic links").
        if let Some(magic) = crate::identity::identity(db, project).magic_link_routes() {
            let generated = jadpo_std::http::MagicLinkStep::ALL
                .into_iter()
                .find(|step| {
                    let segments: Vec<Segment> = step
                        .segments(&magic.path)
                        .into_iter()
                        .map(Segment::Fixed)
                        .collect();
                    route.method == step.method() && overlaps(&route.segments, &segments)
                });
            if let Some(step) = generated {
                out.push(
                    Diagnostic::new(
                        &rule::ROUTE_PATH_OVERLAP,
                        format!("`{}` is the runtime's {}", route.label(), step.label()),
                    )
                    .at(span.clone(), "answered by the runtime")
                    .reason("With `magic_link:` in `sign_in:`, the runtime answers the magic-link routes under its `path:` itself, so this route would never run.")
                    .next(format!(
                        "Give `{}` in `{group}` another path, or move the magic-link routes with `path:`",
                        route.label()
                    )),
                );
                continue;
            }
        }
        // The runtime answers its health checks before any route (area 7,
        // "Health").
        let fixed: Vec<&str> = route
            .segments
            .iter()
            .map_while(|segment| match segment {
                Segment::Fixed(text) => Some(text.as_str()),
                Segment::Param { .. } => None,
            })
            .collect();
        if fixed.len() == route.segments.len()
            && (fixed == jadpo_std::http::HEALTH_LIVE || fixed == jadpo_std::http::HEALTH_READY)
        {
            out.push(
                Diagnostic::new(
                    &rule::ROUTE_PATH_OVERLAP,
                    format!("`{}` is the runtime's health check", route.path()),
                )
                .at(span.clone(), "answered by the runtime")
                .reason("The runtime answers `/health/live` and `/health/ready` itself, before any route, so this route would never run.")
                .next(format!("Give `{}` in `{group}` another path", route.label())),
            );
            continue;
        }
        let earlier = routes[..i].iter().find(|(_, other, _)| {
            other.method == route.method && overlaps(&other.segments, &route.segments)
        });
        if let Some((other_group, other, other_span)) = earlier {
            let same = other.path() == route.path();
            let reason = if same {
                format!(
                    "`{}` is already a route{}; a request can only be answered once.",
                    other.label(),
                    if other_group == group {
                        String::new()
                    } else {
                        format!(" in `{other_group}`")
                    }
                )
            } else {
                format!(
                    "A request such as `{}` matches both `{}` and `{}`: where two paths differ, one must have fixed text where the other has a placeholder, since a fixed segment beats a placeholder.",
                    example(&route.segments),
                    other.label(),
                    route.label()
                )
            };
            out.push(
                Diagnostic::new(
                    &rule::ROUTE_PATH_OVERLAP,
                    format!(
                        "`{}` could answer the same requests as `{}`",
                        route.label(),
                        other.label()
                    ),
                )
                .at(span.clone(), "answers the same requests")
                .also(other_span.clone(), "already answered here")
                .reason(reason)
                .next(if same {
                    "Remove one of the routes, or give it another path".to_string()
                } else {
                    "Give one of the paths fixed text where the other has a placeholder".to_string()
                }),
            );
        }
    }
    out
}

/// Routes that need a signed-in caller when nothing signs anyone in (area
/// 8; area 14, question 11: `jadpo check` reports what couldn't run),
/// reported once, at the first in file order.
#[salsa::tracked(returns(ref))]
pub fn signin_problems(db: &dyn Db, project: Project) -> Vec<Diagnostic> {
    if crate::identity::identity(db, project).signs_in() {
        return Vec::new();
    }
    let mut protected: Vec<(String, Span)> = Vec::new();
    for name in group_names(db, project) {
        let Some(read) = group_read(db, project, Name::new(db, name.clone())) else {
            continue;
        };
        let Some(model) = &read.model else { continue };
        for route in &model.routes {
            if !route.authentication.needs_caller() {
                continue;
            }
            if let Some((label, span)) =
                read.spans.iter().find(|(label, _)| *label == route.label())
            {
                protected.push((label.clone(), span.clone()));
            }
        }
    }
    protected.sort_by(|a, b| (&a.1.path, a.1.start).cmp(&(&b.1.path, b.1.start)));
    let Some((label, span)) = protected.first() else {
        return Vec::new();
    };
    let others = protected.len() - 1;
    let reach = match others {
        0 => "this route".to_string(),
        1 => "this route or the other one that needs a caller".to_string(),
        n => format!("this route or the {n} others that need a caller"),
    };
    vec![
        Diagnostic::new(
            &rule::SEMANTIC_AUTH_NOT_SET_UP,
            format!("`{label}` needs a signed-in caller, but nothing signs anyone in"),
        )
        .at(span.clone(), "needs a signed-in caller")
        .reason(format!(
            "Every route needs a signed-in caller unless it says `authentication: RouteAuthentication.none`, and `authentication.jadpo` declares no way to sign in that is built, so no request could reach {reach}."
        ))
        .next("Declare how callers sign in, `authentication { browser_session: { cookie: \"<name>\" } }`, or let anyone call the route with `authentication: RouteAuthentication.none`"),
    ]
}

/// Whether one request could match both paths, a fixed segment beating a
/// placeholder.
fn overlaps(a: &[Segment], b: &[Segment]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|pair| match pair {
            (Segment::Fixed(x), Segment::Fixed(y)) => x == y,
            (Segment::Param { .. }, Segment::Param { .. }) => true,
            _ => false,
        })
}

/// A request path both overlapping routes match, for messages.
fn example(segments: &[Segment]) -> String {
    segments
        .iter()
        .map(|segment| match segment {
            Segment::Fixed(text) => format!("/{text}"),
            Segment::Param { .. } => "/…".to_string(),
        })
        .collect()
}

// --- reading a group ---------------------------------------------------------

struct Reader<'a> {
    db: &'a dyn Db,
    project: Project,
    lookup: &'a Lookup<'a>,
    names: &'a Names<'a>,
    /// The route being read: the caller variant its call selects, and
    /// whether it needs a signed-in caller.
    caller: Option<(PrincipalKind, Span)>,
    authentication: Authentication,
    /// While `settle:`'s `answer:` is read: the type of `result`, the
    /// only thing its arguments read.
    answering: Option<Ty>,
    apis: &'a Apis,
    path: &'a str,
    out: Vec<Diagnostic>,
    spans: Vec<(String, Span)>,
}

/// What a route's call returns and the failures it can raise.
struct RunSig {
    result: Ty,
    fails: Vec<String>,
    /// With `settle:`, the query `answer:` runs, whose result this is.
    answer: Option<String>,
}

/// What a generated query returns: `List<IssuedToken>` or
/// `List<IssuedKey>` (area 8).
fn generated_result(op: SessionOp) -> Ty {
    let name = if op.on_service() {
        jadpo_std::http::ISSUED_KEY
    } else {
        jadpo_std::http::ISSUED_TOKEN
    };
    Ty::List(Box::new(Ty::Opaque(name.to_string(), Vec::new())))
}

/// Whether `ty` is an `IssuedToken` or an `IssuedKey`, which an output
/// holds (area 8).
pub fn is_issued(ty: &Ty) -> bool {
    matches!(
        ty,
        Ty::Opaque(name, args)
            if args.is_empty()
                && (name == jadpo_std::http::ISSUED_TOKEN || name == jadpo_std::http::ISSUED_KEY)
    )
}

/// A route entry's items, as written.
#[derive(Default)]
struct Items {
    found: Vec<(String, ast::Member)>,
}

impl Items {
    fn get(&self, name: &str) -> Option<&ast::Member> {
        self.found
            .iter()
            .find(|(found, _)| found == name)
            .map(|(_, member)| member)
    }
}

impl Reader<'_> {
    fn span(&self, node: &impl AstNode) -> Span {
        node_span(self.path, node.syntax())
    }

    fn group(&mut self, symbol: &Symbol, decl: &ast::RoutesDecl) -> Option<GroupModel> {
        let name_node = decl.name()?;
        let name = tree::name_text(&name_node)?;
        let name_span = self.span(&name_node);
        let list = decl.member_list();
        let mut prefix_member = None;
        let mut api_member = None;
        let mut entries = Vec::new();
        for item in list.iter().flat_map(|list| list.member_items()) {
            match item {
                MemberItem::RouteEntry(entry) => entries.push(entry),
                MemberItem::Member(member) => {
                    let Some(key) = member.member_key() else {
                        continue;
                    };
                    let written = tree::written(key.syntax());
                    let slot = match written.as_str() {
                        "prefix" => &mut prefix_member,
                        "api" => &mut api_member,
                        _ => {
                            self.unknown_item(&key, &written, &GROUP_ITEMS, "route group");
                            continue;
                        }
                    };
                    if let Some(first) = slot {
                        let first: &ast::Member = first;
                        self.duplicate(&written, &key, first, "route group");
                        continue;
                    }
                    *slot = Some(member);
                }
                MemberItem::BareMember(bare) => {
                    let written = tree::written(bare.syntax());
                    let span = self.span(&bare);
                    self.unknown_word(span, &written, &GROUP_ITEMS, "route group");
                }
                MemberItem::Decl(decl) => {
                    let span = self.span(&decl);
                    let written = tree::written(decl.syntax());
                    let first = written.split_whitespace().next().unwrap_or("").to_string();
                    self.unknown_word(span, &first, &GROUP_ITEMS, "route group");
                }
                // Reported where it is written (area 2, "Rules across
                // fields").
                MemberItem::Elision(_) | MemberItem::RuleMember(_) => {}
            }
        }

        // The API it belongs to.
        let api = match &api_member {
            Some(member) => {
                let value = member.value();
                match value.as_ref() {
                    Some(Expr::NameRef(api)) => tree::name_ref_text(api),
                    _ => None,
                }
                .or_else(|| {
                    let span = value.as_ref().map_or_else(|| self.span(member), |v| self.span(v));
                    self.out.push(
                        Diagnostic::new(&rule::ROUTE_API_UNKNOWN, "`api:` names an API by its name")
                            .at(span, "not an API's name")
                            .reason("A group names the API it belongs to as `app.jadpo` declares it under `apis:`, such as `api: admin`.")
                            .next("Write the API's name"),
                    );
                    None
                })?
            }
            None => apis::MAIN.to_string(),
        };
        let found = self.apis.get(&api).cloned();
        if found.is_none() {
            let span = api_member
                .as_ref()
                .and_then(|member| member.value())
                .map_or_else(|| name_span.clone(), |value| self.span(&value));
            let declared: Vec<String> = self.apis.apis.iter().map(|a| a.name.clone()).collect();
            let near = nearest(&api, declared.iter().map(String::as_str));
            let reason = if self.apis.declared {
                format!(
                    "`app.jadpo` declares {} under `apis:`.{}",
                    crate::and_list(
                        &declared
                            .iter()
                            .map(|a| format!("`{a}`"))
                            .collect::<Vec<_>>()
                    ),
                    near.map_or_else(String::new, |near| format!(" Did you mean `{near}`?"))
                )
            } else {
                "With no `apis:` in `app.jadpo` there is one API, `main`, at `/`.".to_string()
            };
            let mut problem = Diagnostic::new(
                &rule::ROUTE_API_UNKNOWN,
                format!("`{name}` belongs to API `{api}`, which isn't declared"),
            )
            .at(span.clone(), "not declared")
            .reason(reason);
            problem = match near {
                Some(near) => problem
                    .next(format!("Use `{near}` instead of `{api}`"))
                    .edit(span, near),
                None => problem.next(format!("Declare `{api}` under `apis:` in `app.jadpo`")),
            };
            self.out.push(problem);
        }

        // The prefix.
        let mut params_seen: Vec<(String, Span)> = Vec::new();
        let prefix = match &prefix_member {
            Some(member) => match member.value() {
                Some(Expr::RoutePathExpr(path)) => {
                    self.segments(path.route_path(), &mut params_seen)
                }
                other => {
                    let span = other
                        .as_ref()
                        .map_or_else(|| self.span(member), |v| self.span(v));
                    self.out.push(
                        Diagnostic::new(&rule::ROUTE_PREFIX_REQUIRED, "`prefix:` takes a path")
                            .at(span, "not a path")
                            .reason("A group's prefix is the path its routes follow, written as a path starting with `/`.")
                            .next("Write the prefix as a path starting with `/`"),
                    );
                    None
                }
            },
            None => {
                self.out.push(
                    Diagnostic::new(
                        &rule::ROUTE_PREFIX_REQUIRED,
                        format!("`{name}` has no prefix"),
                    )
                    .at(name_span.clone(), "no `prefix:`")
                    .reason("Every route in a group follows the group's prefix, and the prefix's fixed segments give the file its name, so a URL from a log leads to one file.")
                    .next("Add `prefix: /<path>` as the group's first line"),
                );
                None
            }
        };
        if let (Some(prefix), Some(api)) = (&prefix, &found) {
            let fixed: Vec<&str> = prefix
                .iter()
                .map(|segment| match segment {
                    Segment::Fixed(text) => text.as_str(),
                    Segment::Param { .. } => "",
                })
                .collect();
            let starts = api.prefix.len() <= fixed.len()
                && api.prefix.iter().zip(&fixed).all(|(a, b)| a == b);
            if !starts && let Some(member) = &prefix_member {
                let span = member
                    .value()
                    .map_or_else(|| self.span(member), |v| self.span(&v));
                let api_path = if api.prefix.is_empty() {
                    "/".to_string()
                } else {
                    format!("/{}", api.prefix.join("/"))
                };
                self.out.push(
                    Diagnostic::new(
                        &rule::ROUTE_PREFIX_API,
                        format!("`{name}`'s prefix doesn't start with API `{}`'s", api.name),
                    )
                    .at(span, format!("outside `{api_path}`"))
                    .reason(format!(
                        "A group's prefix starts with its API's (`{api_path}` for `{}`), so each API's routes, OpenAPI document and record stay its own.",
                        api.name
                    ))
                    .next(format!("Start the prefix with `{api_path}`, or name the group's API with `api:`")),
                );
            }
        }

        let mut routes = Vec::new();
        for entry in &entries {
            let mut params = params_seen.clone();
            if let Some(route) = self.route(&name, prefix.as_deref(), entry, &mut params) {
                routes.push(route);
            }
        }
        let key = self.lookup.registry().key(Kind::RouteGroup, &name, None);
        Some(GroupModel {
            name,
            key,
            path: symbol.path.clone(),
            domain: symbol.ownership.domain().map(str::to_string),
            api,
            prefix: prefix.unwrap_or_default(),
            routes,
            doc: crate::graph::text::doc(decl.syntax()),
        })
    }

    fn unknown_item(&mut self, key: &ast::MemberKey, written: &str, known: &[&str], what: &str) {
        let span = self.span(key);
        self.unknown_word(span, written, known, what);
    }

    /// An item a route or group doesn't take, with the one it probably
    /// means.
    fn unknown_word(&mut self, span: Span, written: &str, known: &[&str], what: &str) {
        let renamed = match written {
            "success" => Some((
                "status",
                "`success:` is now `status:`, which takes a `SuccessStatus`.",
            )),
            "auth" => Some((
                "authentication",
                "`auth:` is written in full, `authentication:`.",
            )),
            "action" | "call" | "handler" => Some((
                "run",
                "A route runs a named action or query with `run:`; there is no inline action.",
            )),
            "query" | "body" | "path" | "headers" | "params" => Some((
                "input",
                "A route takes one `input:`, an `Input` type; placeholders are typed in the path, and a field read from a header says so with `header:`.",
            )),
            "timeout" => Some(("deadline", "A route's time limit is `deadline:`.")),
            _ => None,
        };
        let near = renamed
            .map(|(word, _)| word)
            .filter(|word| known.contains(word))
            .or_else(|| nearest(written, known.iter().copied()));
        let list = crate::and_list(&known.iter().map(|k| format!("`{k}`")).collect::<Vec<_>>());
        let mut reason = format!(
            "A {what} holds {list}{}.",
            if what == "route group" {
                " and its routes"
            } else {
                ""
            }
        );
        if let Some((_, why)) = renamed {
            reason = format!("{why} {reason}");
        } else if let Some(near) = near {
            reason.push_str(&format!(" Did you mean `{near}`?"));
        }
        let mut problem = Diagnostic::new(
            &rule::ROUTE_ITEM_UNKNOWN,
            format!("`{written}` isn't something a {what} holds"),
        )
        .at(span.clone(), "unknown item")
        .reason(reason);
        problem = match near {
            Some(near) => problem
                .next(format!("Use `{near}` instead of `{written}`"))
                .edit(span, near),
            None => problem.next(format!("Remove `{written}`")),
        };
        self.out.push(problem);
    }

    fn duplicate(&mut self, word: &str, key: &ast::MemberKey, first: &ast::Member, what: &str) {
        let first = first
            .member_key()
            .map_or_else(|| self.span(first), |k| self.span(&k));
        self.out.push(
            Diagnostic::new(
                &rule::ROUTE_ITEM_DUPLICATE,
                format!("`{word}:` is given twice"),
            )
            .at(self.span(key), "given again")
            .also(first, "first given here")
            .reason(format!(
                "A {what} has one `{word}:`, so there is never a question of which counts."
            ))
            .next(format!("Remove one `{word}:`")),
        );
    }

    /// A path's segments; `None` when a placeholder is wrong, which is
    /// reported.
    fn segments(
        &mut self,
        path: Option<ast::RoutePath>,
        seen: &mut Vec<(String, Span)>,
    ) -> Option<Vec<Segment>> {
        let mut out = Vec::new();
        let mut ok = true;
        for segment in path.iter().flat_map(|path| path.route_segments()) {
            let text = segment
                .path_segment_token()
                .map(|token| token.text().to_string())
                .filter(|text| !text.is_empty());
            match (text, segment.path_param()) {
                (Some(text), None) => out.push(Segment::Fixed(text)),
                (None, None) => {}
                (text, Some(param)) => match self.placeholder(&param, text.is_some(), seen) {
                    Some(segment) => out.push(segment),
                    None => ok = false,
                },
            }
        }
        ok.then_some(out)
    }

    fn placeholder(
        &mut self,
        param: &ast::PathParam,
        shares: bool,
        seen: &mut Vec<(String, Span)>,
    ) -> Option<Segment> {
        let name_node = param.name()?;
        let name = tree::name_text(&name_node)?;
        let span = self.span(param);
        let name_span = self.span(&name_node);
        if shares {
            self.out.push(
                Diagnostic::new(
                    &rule::ROUTE_PLACEHOLDER_TYPE,
                    format!("`{name}` shares its segment with fixed text"),
                )
                .at(span, "a whole segment")
                .reason("A placeholder fills a whole segment between `/`s, so the value read from the URL is exactly the segment.")
                .next("Put the placeholder in a segment of its own"),
            );
            return None;
        }
        if let Some((_, first)) = seen.iter().find(|(seen, _)| *seen == name) {
            let first = first.clone();
            self.out.push(
                Diagnostic::new(
                    &rule::ROUTE_PLACEHOLDER_DUPLICATE,
                    format!("Placeholder `{name}` appears twice"),
                )
                .at(name_span, "named again")
                .also(first, "first named here")
                .reason("Each placeholder reaches what the route runs by its name, so a path names each once, its group's prefix included.")
                .next(format!("Rename one `{name}`")),
            );
            return None;
        }
        if name == "input" {
            self.out.push(
                Diagnostic::new(&rule::ROUTE_PLACEHOLDER_DUPLICATE, "A placeholder can't be called `input`")
                    .at(name_span, "the input's name")
                    .reason("`input` names the route's `input:`, so a placeholder of that name would be two values with one name.")
                    .next("Rename the placeholder"),
            );
            return None;
        }
        seen.push((name.clone(), name_span));
        let Some(written) = param.ty() else {
            self.out.push(
                Diagnostic::new(&rule::ROUTE_PLACEHOLDER_TYPE, format!("`{name}` has no type"))
                    .at(span, "untyped")
                    .reason("A placeholder is typed where it appears, so its segment is decoded and checked once: `{<name>: <Type>}`.")
                    .next(format!("Write `{{{name}: <Type>}}`")),
            );
            return None;
        };
        let mut problems = Vec::new();
        self.names
            .ty(Some(written.clone()), Want::Type, &mut problems);
        if !problems.is_empty() {
            self.out.extend(problems);
            return None;
        }
        let ty = self.lookup.resolve(&TypeRef::from_type(&written));
        if ty.has_error() {
            // `Note.nope`: the name resolves, the field doesn't (chunk
            // 6.6d), which nothing else reports.
            if let TypeRef::Path(names, _) = TypeRef::from_type(&written)
                && let [host, missing] = names.as_slice()
                && let Some(fields) = self.lookup.fields(host)
                && !fields.iter().any(|field| field.name == *missing)
            {
                self.out.push(unknown_placeholder_field(
                    host,
                    missing,
                    fields,
                    self.span(&written),
                ));
            }
            return None;
        }
        let type_span = self.span(&written);
        // Only the runtime builds a batch type, so no request carries one
        // (area 12; chunk 6.6b).
        if let Some(batch) = crate::types::decl::holds_batch(self.lookup, &ty, &mut Vec::new()) {
            self.out.push(crate::types::decl::batch_input(
                &format!("Placeholder `{name}`"),
                batch,
                type_span,
            ));
            return None;
        }
        let structure = self.lookup.structure(&ty);
        let named = matches!(ty, Ty::Named(_) | Ty::Field(..));
        let scalar = matches!(&structure, Ty::Prim(prim) if !sent_as_object(*prim));
        if !named || !scalar || self.lookup.is_nullable(&ty) {
            let why = if self.lookup.is_nullable(&ty) {
                "a placeholder is always there, so it is never optional"
            } else if !scalar {
                "a URL segment holds one value, never an object, an enum or a list"
            } else {
                "a built-in type says nothing about what the value means"
            };
            self.out.push(
                Diagnostic::new(
                    &rule::ROUTE_PLACEHOLDER_TYPE,
                    format!("`{name}` can't be typed `{ty}`"),
                )
                .at(type_span, "not a named scalar")
                .reason(format!(
                    "A placeholder's type is a named scalar or an `X.id`: {why}."
                ))
                .next("Type it with a named type built on a scalar, or an `X.id`"),
            );
            return None;
        }
        Some(Segment::Param { name, ty })
    }

    fn route(
        &mut self,
        group: &str,
        prefix: Option<&[Segment]>,
        entry: &ast::RouteEntry,
        params_seen: &mut Vec<(String, Span)>,
    ) -> Option<RouteModel> {
        let before = self.out.len();
        let method_token = entry.method()?;
        // A method no route is written with is a syntax error already.
        let method = HttpMethod::from_word(method_token.text())?;
        let own = self.segments(entry.route_path(), params_seen);
        let head_span = {
            let start = u32::from(method_token.text_range().start());
            let end = entry
                .route_path()
                .map_or(u32::from(method_token.text_range().end()), |path| {
                    u32::from(path.syntax().text_range().end())
                });
            Span::new(self.path, start, end)
        };

        // Its items.
        let mut items = Items::default();
        for item in entry
            .member_list()
            .iter()
            .flat_map(|list| list.member_items())
        {
            match item {
                MemberItem::Member(member) => {
                    let Some(key) = member.member_key() else {
                        continue;
                    };
                    let written = tree::written(key.syntax());
                    if !ROUTE_ITEMS.contains(&written.as_str()) {
                        self.unknown_item(&key, &written, &ROUTE_ITEMS, "route");
                        continue;
                    }
                    if let Some(first) = items.get(&written) {
                        let first = first.clone();
                        self.duplicate(&written, &key, &first, "route");
                        continue;
                    }
                    if let Some((_, why)) = LATER_ITEMS.iter().find(|(item, _)| *item == written) {
                        self.out.push(
                            Diagnostic::new(
                                &rule::TOOLCHAIN_NOT_BUILT_YET,
                                format!("A route's `{written}:` isn't built yet"),
                            )
                            .at(self.span(&key), "decided, not built")
                            .reason(format!("{why}."))
                            .next(format!("Leave `{written}:` out for now")),
                        );
                    }
                    items.found.push((written, member));
                }
                MemberItem::BareMember(bare) => {
                    let written = tree::written(bare.syntax());
                    let span = self.span(&bare);
                    self.unknown_word(span, &written, &ROUTE_ITEMS, "route");
                }
                MemberItem::Decl(decl) => {
                    let written = tree::written(decl.syntax());
                    let first = written.split_whitespace().next().unwrap_or("").to_string();
                    let span = self.span(&decl);
                    self.unknown_word(span, &first, &ROUTE_ITEMS, "route");
                }
                MemberItem::RouteEntry(inner) => {
                    let span = self.span(&inner);
                    self.out.push(
                        Diagnostic::new(&rule::ROUTE_ITEM_UNKNOWN, "A route can't hold another route")
                            .at(span, "a route inside a route")
                            .reason("Routes sit side by side in their group; a path that shares a start is written in full.")
                            .next("Move the inner route out to the group, with its full path after the prefix"),
                    );
                }
                // Reported where it is written (area 2, "Rules across
                // fields").
                MemberItem::Elision(_) | MemberItem::RuleMember(_) => {}
            }
        }

        // `authentication:`.
        self.caller = None;
        self.authentication = match items.get("authentication") {
            Some(member) => self.authentication(member)?,
            None => Authentication::Required,
        };
        // `input:`.
        let input = items
            .get("input")
            .and_then(|member| self.input(method, member));
        // `status:`.
        let status = match items.get("status") {
            Some(member) => self.status(member)?,
            None => SuccessStatus::Ok,
        };
        // `deadline:`.
        let deadline_ms = match items.get("deadline") {
            Some(member) => self
                .deadline(member)
                .unwrap_or(jadpo_std::http::DEFAULT_DEADLINE_MS),
            None => jadpo_std::http::DEFAULT_DEADLINE_MS,
        };
        // `output:`.
        let output = items.get("output").map(|member| self.output(member));

        let label_path = {
            let mut all = prefix.map(<[Segment]>::to_vec).unwrap_or_default();
            all.extend(own.clone().unwrap_or_default());
            all
        };
        let label = format!("{} {}", method.word(), path_text(&label_path));

        // `run:`.
        let Some(run_member) = items.get("run") else {
            self.out.push(
                Diagnostic::new(&rule::ROUTE_RUN_REQUIRED, format!("`{label}` doesn't say what it runs"))
                    .at(head_span, "no `run:`")
                    .reason("A route runs one named action, or a named query for a `GET`, with `run:`; the logic lives there, not in the route.")
                    .next(if method == HttpMethod::Get {
                        "Add `run: <query>(...)`"
                    } else {
                        "Add `run: <action>(...)`"
                    }),
            );
            return None;
        };
        let segments = {
            let mut all = prefix?.to_vec();
            all.extend(own?);
            all
        };
        let input = match (items.get("input"), input) {
            (Some(_), None) => return None,
            (_, input) => input,
        };
        let run = self.run(method, run_member, &segments, input.as_deref(), &label)?;
        let sig = match run.generated {
            Some(op) => RunSig {
                result: generated_result(op),
                fails: Vec::new(),
                answer: None,
            },
            None => {
                let sig = self.lookup.signature(&run.name)?;
                RunSig {
                    result: sig.result.clone(),
                    fails: sig.fails.clone(),
                    answer: None,
                }
            }
        };

        // What it waits for: then what comes out is what `answer:` gives.
        let settle = match items.get("settle") {
            Some(member) => {
                Some(self.settle(member, &run, &sig.result, status, deadline_ms, &label)?)
            }
            None => None,
        };
        let sig = match &settle {
            Some((answer, result, fails)) => {
                let mut all = sig.fails.clone();
                for fail in fails {
                    if !all.contains(fail) {
                        all.push(fail.clone());
                    }
                }
                RunSig {
                    result: result.clone(),
                    fails: all,
                    answer: Some(answer.answer.name.clone()),
                }
            }
            None => sig,
        };
        let run_name = sig.answer.clone().unwrap_or_else(|| run.name.clone());
        let mismatch_reason = if settle.is_some() {
            "With `settle:`, a route sends what `answer:` gives, whether the work settled or not, so `output:` is exactly the query's result."
        } else {
            "A route's `output:` is exactly what it runs returns, so the route shows its contract and never sends a field its type doesn't declare."
        };
        // What comes out.
        match (status, output) {
            (SuccessStatus::NoContent, Some(_)) => {
                let member = items.get("output")?;
                self.out.push(
                    Diagnostic::new(&rule::ROUTE_OUTPUT_UNEXPECTED, format!("`{label}` answers with no content"))
                        .at(self.span(member), "no body is sent")
                        .reason("`SuccessStatus.no_content` answers 204 with no body, so the route has no `output:` and what it runs returns `Unit`.")
                        .next("Remove `output:`, or choose another status"),
                );
                return None;
            }
            (SuccessStatus::NoContent, None) => {
                if sig.result != Ty::Prim(Prim::Unit) && !sig.result.has_error() {
                    self.out.push(
                        Diagnostic::new(
                            &rule::ROUTE_OUTPUT_MISMATCH,
                            format!("`{}` returns `{}`, but `{label}` sends nothing", run_name, sig.result),
                        )
                        .at(self.run_span(run_member), format!("returns `{}`", sig.result))
                        .reason("`SuccessStatus.no_content` answers 204 with no body, so what the route runs returns `Unit`; a result nobody receives is a mistake.")
                        .next("Give the route an `output:` and another status, or run something that returns `Unit`"),
                    );
                    return None;
                }
            }
            // What returns `Unit` sends nothing: the route answers 204,
            // since no route sends `Unit` (Phase 6 review).
            (_, None) if sig.result == Ty::Prim(Prim::Unit) => {
                self.out.push(
                    Diagnostic::new(&rule::ROUTE_OUTPUT_REQUIRED, format!("`{label}` has no `output:`"))
                        .at(head_span.clone(), "no `output:`")
                        .reason(format!(
                            "A route shows its contract without opening what it runs: `{run_name}` returns `Unit`, so the route sends nothing, which `status: SuccessStatus.no_content` says, answering 204 with no body."
                        ))
                        .next("Add `status: SuccessStatus.no_content`"),
                );
                return None;
            }
            (_, None) => {
                self.out.push(
                    Diagnostic::new(&rule::ROUTE_OUTPUT_REQUIRED, format!("`{label}` has no `output:`"))
                        .at(head_span.clone(), "no `output:`")
                        .reason(format!(
                            "A route shows its contract without opening what it runs: `output:` names what it sends, exactly what `{}` returns{}.",
                            run_name,
                            if sig.result.has_error() {
                                String::new()
                            } else {
                                format!(" (`{}`)", sig.result)
                            }
                        ))
                        .next(if sig.result.has_error() {
                            "Add `output: <Type>`".to_string()
                        } else {
                            format!("Add `output: {}`", sig.result)
                        }),
                );
                return None;
            }
            (_, Some(None)) => return None,
            (_, Some(Some(ty))) => {
                if ty != sig.result && !sig.result.has_error() {
                    let member = items.get("output")?;
                    let span = member
                        .value()
                        .map_or_else(|| self.span(member), |v| self.span(&v));
                    self.out.push(
                        Diagnostic::new(
                            &rule::ROUTE_OUTPUT_MISMATCH,
                            format!(
                                "`{label}` sends `{ty}`, but `{}` returns `{}`",
                                run_name, sig.result
                            ),
                        )
                        .at(span.clone(), format!("not `{}`", sig.result))
                        .reason(mismatch_reason)
                        .next(format!("Write `output: {}`", sig.result))
                        .edit(span, sig.result.to_string()),
                    );
                    return None;
                }
            }
        }
        // A model's answer sent as it comes (area 7, "Streaming a model's
        // answer"; chunk 5.5v6): `200` once its first piece has come, and
        // nothing to wait for.
        if streams(&sig.result) {
            let member = items.get("status").or(items.get("settle"));
            if let Some(member) = member.filter(|_| status != SuccessStatus::Ok || settle.is_some())
            {
                let key = member
                    .member_key()
                    .map_or_else(|| self.span(member), |key| self.span(&key));
                let (message, at, next) = if settle.is_some() {
                    (
                        format!("`{label}` streams a model's answer and waits with `settle:`"),
                        "waits for its work",
                        "Remove `settle:`; a stream answers as its pieces come",
                    )
                } else {
                    (
                        format!(
                            "`{label}` streams a model's answer with `SuccessStatus.{}`",
                            status.name()
                        ),
                        "not `SuccessStatus.ok`",
                        "Remove `status:`; a stream answers 200",
                    )
                };
                self.out.push(
                    Diagnostic::new(&rule::ROUTE_STREAM_INVALID, message)
                        .at(key, at.to_string())
                        .reason(STREAM_REASON)
                        .next(next.to_string()),
                );
                return None;
            }
        }
        let output = match status {
            SuccessStatus::NoContent => None,
            _ => Some(sig.result.clone()),
        };
        if self.out.len() > before {
            return None;
        }
        self.spans.push((label, head_span));
        let key = self.lookup.registry().key(
            Kind::Route,
            &format!("{} {}", method.word(), path_text(&segments)),
            None,
        );
        let _ = group;
        Some(RouteModel {
            key,
            method,
            segments,
            input,
            output,
            status,
            deadline_ms,
            authentication: self.authentication,
            caller: self.caller.as_ref().map(|(kind, _)| *kind),
            run,
            settle: settle.map(|(settle, _, _)| settle),
            fails: sig.fails.clone(),
            doc: crate::graph::text::doc(entry.syntax()),
        })
    }

    /// `authentication:`: `RouteAuthentication.none` or `.fresh` (area 8).
    /// Anything else is refused without a guess, since one fix keeps the
    /// route protected and the other opens it to anyone.
    fn authentication(&mut self, member: &ast::Member) -> Option<Authentication> {
        let value = member.value();
        let read = value.as_ref().and_then(|value| {
            let (first, rest) = tree::dotted(value)?;
            let [variant] = rest.as_slice() else {
                return None;
            };
            (tree::name_ref_text(&first)? == RouteAuthentication::NAME).then_some(())?;
            RouteAuthentication::from_name(&tree::name_ref_text(variant)?)
        });
        match read {
            Some(RouteAuthentication::None) => Some(Authentication::Public),
            Some(RouteAuthentication::Fresh) => Some(Authentication::Fresh),
            None => {
                let written = value
                    .as_ref()
                    .map(|v| tree::written(v.syntax()))
                    .unwrap_or_default();
                let span = value
                    .as_ref()
                    .map_or_else(|| self.span(member), |v| self.span(v));
                self.out.push(
                    Diagnostic::new(
                        &rule::ROUTE_AUTHENTICATION_INVALID,
                        format!("`{written}` isn't a `RouteAuthentication`"),
                    )
                    .at(span, "not `.none` or `.fresh`")
                    .reason("Every route needs a signed-in caller unless it says otherwise: `RouteAuthentication.none` lets anyone call it, and `RouteAuthentication.fresh` checks the caller's record on this request. One opens the route and the other keeps it protected, so neither is guessed.")
                    .next("Write `RouteAuthentication.fresh` to keep the route signed in, `RouteAuthentication.none` to let anyone call it, or remove `authentication:`"),
                );
                None
            }
        }
    }

    fn input(&mut self, method: HttpMethod, member: &ast::Member) -> Option<String> {
        let value = member.value()?;
        let span = self.span(&value);
        let mut problems = Vec::new();
        self.names.type_expr(&value, Want::Type, &mut problems);
        if !problems.is_empty() {
            self.out.extend(problems);
            return None;
        }
        let ty = self.lookup.resolve(&TypeRef::from_expr(&value));
        if ty.has_error() {
            return None;
        }
        let name = match &ty {
            Ty::Named(name) if self.lookup.base(name) == Some(FieldsBase::Input) => name.clone(),
            other => {
                self.out.push(
                    Diagnostic::new(&rule::ROUTE_INPUT_NOT_INPUT, format!("`{other}` isn't an `Input`"))
                        .at(span, "not an `Input`")
                        .reason("A route accepts only an `Input` type (area 2): the only objects that may leave fields out or fill them by default, and the ones a request's fields are checked against.")
                        .next("Declare `type <Name> = Input { ... }` and take it here"),
                );
                return None;
            }
        };
        if method.reads_query() {
            let fields = self
                .lookup
                .fields(&name)
                .map(<[_]>::to_vec)
                .unwrap_or_default();
            for field in fields {
                if field.header.is_some() || field.declared.has_error() {
                    continue;
                }
                // A batch type is reported where the input declares it.
                if crate::types::decl::holds_batch(self.lookup, &field.declared, &mut Vec::new())
                    .is_some()
                {
                    continue;
                }
                if !self.query_value(&field.declared) {
                    self.out.push(
                        Diagnostic::new(
                            &rule::ROUTE_QUERY_FIELD_UNSUPPORTED,
                            format!("`{name}.{}` can't come in a query string", field.name),
                        )
                        .at(span.clone(), format!("`{}` is `{}`", field.name, field.declared))
                        .reason(format!(
                            "A `{}` route reads its input from the query string, which holds scalars, enums and lists of them (`?status=open&status=done`); nested objects and maps need a body.",
                            method.word()
                        ))
                        .next(format!("Give `{}` a scalar type, or take this input on a `POST`", field.name)),
                    );
                    return None;
                }
            }
        }
        Some(name)
    }

    /// Whether a value of `ty` can be written in a query string: a scalar,
    /// an enum without data, or a list of them. A scalar the contract's
    /// JSON sends as an object, an amount of money, a quantity or a
    /// `ZonedTime`, is an object here too (chunk 5.5w2).
    fn query_value(&self, ty: &Ty) -> bool {
        let one = |ty: &Ty| match self.lookup.structure(ty) {
            Ty::Prim(prim) => !sent_as_object(prim),
            Ty::Error => true,
            Ty::Named(name) => self
                .lookup
                .variants(&name)
                .is_some_and(|variants| variants.iter().all(|v| v.fields.is_none())),
            _ => false,
        };
        match self.lookup.structure(ty) {
            Ty::List(inner) => one(&inner),
            _ => one(ty),
        }
    }

    fn status(&mut self, member: &ast::Member) -> Option<SuccessStatus> {
        let value = member.value();
        let found = value.as_ref().and_then(|value| {
            let (first, rest) = tree::dotted(value)?;
            let [variant] = rest.as_slice() else {
                return None;
            };
            (tree::name_ref_text(&first)? == SuccessStatus::NAME).then_some(())?;
            SuccessStatus::from_name(&tree::name_ref_text(variant)?)
        });
        if found.is_none() {
            let span = value
                .as_ref()
                .map_or_else(|| self.span(member), |v| self.span(v));
            let written = value
                .as_ref()
                .map(|v| tree::written(v.syntax()))
                .unwrap_or_default();
            let meant = match written.as_str() {
                "200" => Some("ok"),
                "201" => Some("created"),
                "202" => Some("accepted"),
                "204" => Some("no_content"),
                _ => None,
            };
            let mut problem = Diagnostic::new(
                &rule::ROUTE_STATUS_INVALID,
                format!("`{written}` isn't a success status"),
            )
            .at(span.clone(), "not a `SuccessStatus`")
            .reason("`status:` takes `SuccessStatus.ok`, `.created`, `.accepted` or `.no_content`; failure statuses come from the kinds of the failures the route can produce, never from the route.");
            problem = match meant {
                Some(variant) => problem
                    .next(format!("Write `SuccessStatus.{variant}`"))
                    .edit(span, format!("SuccessStatus.{variant}")),
                None => problem.next("Write one of the `SuccessStatus` variants"),
            };
            self.out.push(problem);
        }
        found
    }

    fn deadline(&mut self, member: &ast::Member) -> Option<u32> {
        let value = member.value();
        let written = value
            .as_ref()
            .map(|v| tree::written(v.syntax()))
            .unwrap_or_default();
        let millis = jadpo_std::Dur::parse(&written).map(|dur| dur.millis);
        match millis {
            Some(millis)
                if millis > 0 && millis <= i128::from(jadpo_std::http::MAX_DEADLINE_MS) =>
            {
                u32::try_from(millis).ok()
            }
            _ => {
                let span = value
                    .as_ref()
                    .map_or_else(|| self.span(member), |v| self.span(v));
                self.out.push(
                    Diagnostic::new(
                        &rule::ROUTE_DEADLINE_INVALID,
                        format!("`{written}` can't be a route's deadline"),
                    )
                    .at(span, if millis.is_some() { "outside 1 ms to 60 s" } else { "not a duration" })
                    .reason("A route's deadline is a duration above zero and at most 60 seconds; longer work answers `SuccessStatus.accepted` and carries on in a subscriber.")
                    .next("Write a duration such as `30s`, or leave `deadline:` out for the 10-second default"),
                );
                None
            }
        }
    }

    /// `settle: { answer: <query>(result...) within: <duration> }` (area 7,
    /// "Waiting for follow-on work"; chunk 6.3): the query the route
    /// answers with once the work its action set off has settled, its
    /// result type and its failures. An action route only, answering with
    /// a body, and waiting less than its deadline.
    fn settle(
        &mut self,
        member: &ast::Member,
        run: &Run,
        result: &Ty,
        status: SuccessStatus,
        deadline_ms: u32,
        label: &str,
    ) -> Option<(Settle, Ty, Vec<String>)> {
        let key_span = member
            .member_key()
            .map_or_else(|| self.span(member), |key| self.span(&key));
        let invalid = |span: Span, message: String, at: &str, next: &str| {
            Diagnostic::new(&rule::ROUTE_SETTLE_INVALID, message)
                .at(span, at.to_string())
                .reason(SETTLE_REASON)
                .next(next.to_string())
        };
        if run.query || run.generated.is_some() {
            self.out.push(invalid(
                key_span,
                format!("`{label}` runs a query, which sets off no work to wait for"),
                "nothing to wait for",
                "Remove `settle:`; a query answers at once",
            ));
            return None;
        }
        if matches!(status, SuccessStatus::NoContent | SuccessStatus::Accepted) {
            self.out.push(invalid(
                key_span,
                format!(
                    "`{label}` answers `SuccessStatus.{}` and waits with `settle:`",
                    status.name()
                ),
                "waits for its work",
                "Answer `SuccessStatus.ok` or `.created` once the work settles, or remove `settle:` and answer at once",
            ));
            return None;
        }
        let Some(block) = member.block() else {
            self.out.push(invalid(
                key_span,
                "`settle:` holds no block".to_string(),
                "not a block",
                "Write `settle: { answer: <query>(result.id) within: 5s }`",
            ));
            return None;
        };
        let mut answer: Option<ast::Member> = None;
        let mut within: Option<ast::Member> = None;
        let mut ok = true;
        for (setting, entry) in tree::members(Some(block)) {
            let name = tree::name_text(&setting).unwrap_or_default();
            let slot = match name.as_str() {
                "answer" => &mut answer,
                "within" => &mut within,
                _ => {
                    self.out.push(invalid(
                        self.span(&setting),
                        format!("`{name}` isn't part of `settle:`"),
                        "not a `settle:` setting",
                        "Leave only `answer:` and `within:`",
                    ));
                    ok = false;
                    continue;
                }
            };
            if slot.is_some() {
                self.out.push(invalid(
                    self.span(&setting),
                    format!("`settle:` gives `{name}:` twice"),
                    "given twice",
                    "Keep one",
                ));
                ok = false;
                continue;
            }
            *slot = Some(entry);
        }
        for (missing, what) in [(answer.is_none(), "answer"), (within.is_none(), "within")] {
            if missing {
                self.out.push(invalid(
                    key_span.clone(),
                    format!("`settle:` has no `{what}:`"),
                    "incomplete",
                    "Write `settle: { answer: <query>(result.id) within: 5s }`",
                ));
                ok = false;
            }
        }
        let (Some(answer), Some(within)) = (answer, within) else {
            return None;
        };
        // `within:`: above zero and short of the deadline, which leaves
        // the answer time to run.
        let value = within.value();
        let written = value
            .as_ref()
            .map(|v| tree::written(v.syntax()))
            .unwrap_or_default();
        let span = value
            .as_ref()
            .map_or_else(|| self.span(&within), |v| self.span(v));
        let millis = jadpo_std::Dur::parse(&written).map(|dur| dur.millis);
        let within_ms = match millis {
            Some(millis) if millis > 0 && millis < i128::from(deadline_ms) => {
                u32::try_from(millis).ok()
            }
            Some(_) => {
                self.out.push(invalid(
                    span,
                    format!(
                        "`within: {written}` doesn't fit inside the route's {} deadline",
                        jadpo_std::Dur {
                            millis: i128::from(deadline_ms)
                        }
                    ),
                    "not shorter than the deadline",
                    "Wait for less than the deadline, or give the route a longer `deadline:`",
                ));
                None
            }
            None => {
                self.out.push(invalid(
                    span,
                    format!("`within: {written}` isn't a duration"),
                    "not a duration",
                    "Write a duration such as `5s`",
                ));
                None
            }
        };
        // `answer:`: a named query, given `result` or its fields.
        let value = answer.value();
        let Some(Expr::CallExpr(call)) = value.clone() else {
            let span = value
                .as_ref()
                .map_or_else(|| self.span(&answer), |v| self.span(v));
            let written = value
                .as_ref()
                .map(|v| tree::written(v.syntax()))
                .unwrap_or_default();
            self.out.push(invalid(
                span,
                format!("`answer: {written}` isn't a call"),
                "not a call",
                "Write `answer: <query>(result.id)`",
            ));
            return None;
        };
        let callee_span = self.run_span(&answer);
        let mut problems = Vec::new();
        self.names.callee(&call, false, &mut problems);
        if !problems.is_empty() {
            self.out.extend(problems);
            return None;
        }
        let name = match call.expr()? {
            Expr::NameRef(name) => tree::name_ref_text(&name)?,
            callee @ Expr::FieldExpr(_) => {
                let (_, rest) = tree::dotted(&callee)?;
                tree::name_ref_text(rest.last()?)?
            }
            _ => return None,
        };
        let symbol = self.lookup.declared(&name).first()?.clone();
        if symbol.kind != ItemKind::Query {
            self.out.push(invalid(
                callee_span,
                format!("`answer: {name}` is {}", symbol.kind.with_a()),
                &format!("{} here", symbol.kind.with_a()),
                "Answer with a named query, which reads how far the work has got",
            ));
            return None;
        }
        let sig = self.lookup.signature(&name)?.clone();
        self.answering = Some(result.clone());
        let args = self.call_arguments(
            &call,
            &name,
            &sig.params,
            &[],
            None,
            callee_span,
            label,
            "is declared with",
        );
        self.answering = None;
        let args = args?;
        if !ok {
            return None;
        }
        Some((
            Settle {
                answer: Run {
                    query: true,
                    name,
                    args,
                    generated: None,
                },
                params: sig.params.iter().map(|(_, ty)| ty.clone()).collect(),
                within_ms: within_ms?,
            },
            sig.result.clone(),
            sig.fails.clone(),
        ))
    }

    /// `output:`: an `Output`, or a list or page of one, as a query returns
    /// them, or a map layer, a `FeatureCollection<G, P>` sent bare (area 7,
    /// "A map layer as output", chunk 5.5u7). `Some(None)` when it is
    /// wrong, which is reported.
    fn output(&mut self, member: &ast::Member) -> Option<Ty> {
        let value = member.value()?;
        let span = self.span(&value);
        let mut problems = Vec::new();
        self.names.type_expr(&value, Want::Type, &mut problems);
        if !problems.is_empty() {
            self.out.extend(problems);
            return None;
        }
        let ty = self.lookup.resolve(&TypeRef::from_expr(&value));
        if ty.has_error() {
            return None;
        }
        if let Ty::Opaque(name, args) = &ty
            && name == jadpo_std::standard::geo::FEATURE_COLLECTION
            && let [_, properties] = args.as_slice()
        {
            return match self.withheld(properties) {
                None => Some(ty),
                Some(what) => {
                    self.out.push(
                        Diagnostic::new(
                            &rule::ROUTE_OUTPUT_NOT_OUTPUT,
                            format!("`{ty}`'s properties hold {what}"),
                        )
                        .at(span, format!("its properties hold {what}"))
                        .reason(format!(
                            "{OUTPUT_ONLY} `{properties}` holds {what}, which an `Output` may not hold, so it would leave the server."
                        ))
                        .next(format!(
                            "Give the features properties with only the fields clients see, in place of `{properties}`"
                        )),
                    );
                    None
                }
            };
        }
        let element = match &ty {
            Ty::List(inner) => inner.as_ref(),
            Ty::Opaque(name, args) if name == "Page" && args.len() == 1 => &args[0],
            other => other,
        };
        match element {
            Ty::Named(name) if self.lookup.base(name) == Some(FieldsBase::Output) => Some(ty),
            // A model's answer, streamed (chunk 5.5v6).
            stream if streams(stream) && element == &ty => Some(ty),
            // The values `sessions.tokens` and `Service.keys` list (area 8).
            issued if is_issued(issued) => Some(ty),
            other => {
                self.out.push(
                    Diagnostic::new(&rule::ROUTE_OUTPUT_NOT_OUTPUT, format!("`{other}` isn't an `Output`"))
                        .at(span, "not an `Output`")
                        .reason(OUTPUT_ONLY)
                        .next("Declare `type <Name> = Output { ... }` with the fields clients see, and return it"),
                );
                None
            }
        }
    }

    /// What a layer's properties `properties` hold that an `Output` may
    /// not (area 7, "A map layer as output"): a caller, a password's hash
    /// or a secret value, however deep, as an `Output`'s own fields are
    /// refused them; `None` when they hold nothing of the kind.
    fn withheld(&self, properties: &Ty) -> Option<String> {
        let table: &dyn TypeView = self.lookup;
        if crate::types::decl::holds_principal(table, properties, &mut Vec::new()) {
            return Some("a caller".to_string());
        }
        if crate::types::decl::holds_password_hash(table, properties, &mut Vec::new()) {
            return Some("a password's hash".to_string());
        }
        crate::types::secrets::held_secret(table, properties, &mut Vec::new())
            .map(|secret| format!("a secret value, `{secret}`"))
    }

    fn run_span(&self, member: &ast::Member) -> Span {
        match member.value() {
            Some(Expr::CallExpr(call)) => call
                .expr()
                .map_or_else(|| self.span(&call), |callee| self.span(&callee)),
            Some(value) => self.span(&value),
            None => self.span(member),
        }
    }

    fn run(
        &mut self,
        method: HttpMethod,
        member: &ast::Member,
        segments: &[Segment],
        input: Option<&str>,
        label: &str,
    ) -> Option<Run> {
        let value = member.value();
        let Some(Expr::CallExpr(call)) = value.clone() else {
            let span = value
                .as_ref()
                .map_or_else(|| self.span(member), |v| self.span(v));
            let written = value
                .as_ref()
                .map(|v| tree::written(v.syntax()))
                .unwrap_or_default();
            self.out.push(
                Diagnostic::new(&rule::ROUTE_RUN_INVALID, format!("`run: {written}` isn't a call"))
                    .at(span, "not a call")
                    .reason("`run:` calls one named action, or a named query for a `GET`, passing the route's placeholders and `input`; there is no inline action in a route.")
                    .next("Write `run: <name>(...)`, moving any logic into an action"),
            );
            return None;
        };
        let callee_span = self.run_span(member);
        if let Some(op) = self.generated(&call) {
            return self.generated_run(method, &call, op, segments, input, label, callee_span);
        }
        // Resolution: an unknown callee, another domain's private one.
        let mut problems = Vec::new();
        self.names.callee(&call, false, &mut problems);
        if !problems.is_empty() {
            self.out.extend(problems);
            return None;
        }
        let (name, qualified) = match call.expr()? {
            Expr::NameRef(name) => (tree::name_ref_text(&name)?, false),
            callee @ Expr::FieldExpr(_) => {
                let (_, rest) = tree::dotted(&callee)?;
                (tree::name_ref_text(rest.last()?)?, true)
            }
            _ => return None,
        };
        let _ = qualified;
        let symbol = self.lookup.declared(&name).first()?.clone();
        let is_query = match symbol.kind {
            ItemKind::Action => false,
            ItemKind::Query => true,
            ref kind => {
                self.out.push(
                    Diagnostic::new(
                        &rule::ROUTE_RUN_INVALID,
                        format!("`{name}` is {}", kind.with_a()),
                    )
                    .at(callee_span, format!("{} here", kind.with_a()))
                    .reason("A route runs a named action, or a named query for a `GET`; a function is called from those, so the route stays a thin door.")
                    .next(format!("Call `{name}` from an action, and run that")),
                );
                return None;
            }
        };
        match (method == HttpMethod::Get, is_query) {
            (true, false) => {
                self.out.push(
                    Diagnostic::new(&rule::ROUTE_RUN_KIND, format!("`{label}` runs action `{name}`"))
                        .at(callee_span, "an action")
                        .reason("A `GET` is safe to repeat and never writes, so it runs a named query; every other method runs an action.")
                        .next(format!("Run a query here, or answer `{}` with another method", path_text(segments))),
                );
                return None;
            }
            (false, true) => {
                self.out.push(
                    Diagnostic::new(&rule::ROUTE_RUN_KIND, format!("`{label}` runs query `{name}`"))
                        .at(callee_span, "a query")
                        .reason(format!(
                            "A `{}` route runs a named action, which owns the writes and the transaction; a query only reads, and answers a `GET`.",
                            method.word()
                        ))
                        .next("Run an action here, or make the route a `GET`"),
                );
                return None;
            }
            _ => {}
        }
        let sig = self.lookup.signature(&name)?.clone();

        let params: Vec<(String, Ty)> = sig.params.clone();
        let args = self.call_arguments(
            &call,
            &name,
            &params,
            segments,
            input,
            callee_span,
            label,
            "is declared with",
        )?;
        Some(Run {
            query: is_query,
            name,
            args,
            generated: None,
        })
    }

    /// `sessions.tokens(...)` or the service entity's `keys(...)`: the
    /// generated queries a `GET` runs (area 8, "Listing tokens and keys
    /// from a `GET`"), and the generated writes beside them, which a
    /// route can't run.
    fn generated(&self, call: &ast::CallExpr) -> Option<SessionOp> {
        let callee = call.expr()?;
        let (first, rest) = tree::dotted(&callee)?;
        let [member] = rest.as_slice() else {
            return None;
        };
        let head = tree::name_ref_text(&first)?;
        let member = tree::name_ref_text(member)?;
        if head == "sessions" {
            return SessionOp::from_name(&member);
        }
        if head == jadpo_std::http::MAGIC_LINKS {
            return SessionOp::magic_link_op(&member);
        }
        let op = SessionOp::key_op(&member)?;
        let identity = crate::identity::identity(self.db, self.project);
        identity
            .variant(PrincipalKind::Service)
            .filter(|variant| variant.entity == head)
            .map(|_| op)
    }

    #[allow(clippy::too_many_arguments)]
    fn generated_run(
        &mut self,
        method: HttpMethod,
        call: &ast::CallExpr,
        op: SessionOp,
        segments: &[Segment],
        input: Option<&str>,
        label: &str,
        callee_span: Span,
    ) -> Option<Run> {
        let written = tree::written(call.expr()?.syntax());
        if !op.reads_only() {
            let keeps = if op.magic_link() {
                "the sign-in requests"
            } else {
                "the tokens or keys"
            };
            self.out.push(
                Diagnostic::new(
                    &rule::ROUTE_RUN_INVALID,
                    format!("`{written}` writes, and a route doesn't run it"),
                )
                .at(callee_span, "a generated write")
                .reason(format!(
                    "A route runs a named action, or a named query for a `GET`, and may run the generated listings `sessions.tokens` and the service entity's `keys` itself; `{written}` writes {keeps} the runtime keeps, in the transaction of the action that calls it."
                ))
                .next(format!("Call `{written}` in an action, and run that")),
            );
            return None;
        }
        if method != HttpMethod::Get {
            self.out.push(
                Diagnostic::new(
                    &rule::ROUTE_RUN_KIND,
                    format!("`{label}` runs query `{written}`"),
                )
                .at(callee_span, "a query")
                .reason(format!(
                    "A `{}` route runs a named action, which owns the writes and the transaction; a query only reads, and answers a `GET`.",
                    method.word()
                ))
                .next("Run an action here, or make the route a `GET`"),
            );
            return None;
        }
        let identity = crate::identity::identity(self.db, self.project);
        let needed = op.method();
        let kind = if op.on_service() {
            PrincipalKind::Service
        } else {
            PrincipalKind::User
        };
        let variant = identity
            .variant(kind)
            .filter(|_| identity.built(needed))
            .cloned();
        let Some(variant) = variant else {
            // What is missing is reported where it is declared, or here
            // when nothing is.
            if !identity.methods.iter().any(|m| m.method == needed) {
                self.out.push(crate::identity::needs_method(
                    &written,
                    needed,
                    false,
                    callee_span,
                ));
            }
            return None;
        };
        let holder = if op.on_service() {
            "service_id"
        } else {
            "user"
        };
        let params = vec![(
            holder.to_string(),
            Ty::Field(variant.entity.clone(), vec!["id".to_string()]),
        )];
        let args = self.call_arguments(
            call,
            &written,
            &params,
            segments,
            input,
            callee_span,
            label,
            "takes",
        )?;
        Some(Run {
            query: true,
            name: written,
            args,
            generated: Some(op),
        })
    }

    /// The arguments of a route's call: its placeholders, `input` and the
    /// caller, each typed against `expected_params`, with every
    /// placeholder and the input passed on.
    #[allow(clippy::too_many_arguments)]
    fn call_arguments(
        &mut self,
        call: &ast::CallExpr,
        name: &str,
        expected_params: &[(String, Ty)],
        segments: &[Segment],
        input: Option<&str>,
        callee_span: Span,
        label: &str,
        verb: &str,
    ) -> Option<Vec<Arg>> {
        let params: Vec<(&str, &Ty)> = segments
            .iter()
            .filter_map(|segment| match segment {
                Segment::Param { name, ty } => Some((name.as_str(), ty)),
                Segment::Fixed(_) => None,
            })
            .collect();
        let mut used: BTreeSet<String> = BTreeSet::new();
        let mut args = Vec::new();
        let mut typed = Vec::new();
        let written: Vec<ast::Arg> = call
            .arg_list()
            .iter()
            .flat_map(|list| list.args())
            .collect();
        let mut ok = true;
        for arg in &written {
            let Some(expr) = arg.expr() else { continue };
            if arg.name().is_some() {
                self.out
                    .push(self.argument_invalid(&expr, "a named argument"));
                ok = false;
                continue;
            }
            match self.argument(&expr, &params, input, &mut used) {
                Some((value, ty)) => {
                    args.push(value);
                    typed.push((expr, ty));
                }
                None => ok = false,
            }
        }
        if !ok {
            return None;
        }
        if typed.len() != expected_params.len() {
            let span = call
                .arg_list()
                .map_or_else(|| callee_span.clone(), |list| self.span(&list));
            let expected = expected_params.len();
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_ARGUMENT_COUNT,
                    format!(
                        "`{name}` takes {expected} {}, not {}",
                        if expected == 1 {
                            "argument"
                        } else {
                            "arguments"
                        },
                        typed.len()
                    ),
                )
                .at(span, format!("{} given", typed.len()))
                .reason(format!(
                    "`{name}` {verb} ({}).",
                    expected_params
                        .iter()
                        .map(|(param, ty)| format!("{param}: {ty}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
                .next(format!(
                    "Pass {expected} {}",
                    if expected == 1 { "value" } else { "values" }
                )),
            );
            return None;
        }
        for ((expr, actual), (param, expected)) in typed.iter().zip(expected_params) {
            if !self.lookup.assignable(actual, expected) {
                self.out.push(
                    Diagnostic::new(
                        &rule::TYPE_MISMATCH,
                        format!("`{param}` takes `{expected}`, not `{actual}`"),
                    )
                    .at(self.span(expr), format!("`{actual}`"))
                    .reason(format!(
                        "`{name}`'s parameter `{param}` is `{expected}`; a value of another type converts only by constructing (area 2), which a route leaves to what it runs."
                    ))
                    .next(format!("Pass a value of type `{expected}`")),
                );
                ok = false;
            }
        }
        if !ok {
            return None;
        }
        // Every placeholder and the input reach what it runs.
        let mut unused: Vec<String> = params
            .iter()
            .map(|(param, _)| param.to_string())
            .filter(|param| !used.contains(param))
            .collect();
        if input.is_some() && !used.contains("input") {
            unused.push("input".to_string());
        }
        if let Some(first) = unused.first() {
            let what = if first == "input" {
                "`input`".to_string()
            } else {
                format!("placeholder `{first}`")
            };
            self.out.push(
                Diagnostic::new(
                    &rule::ROUTE_BINDING_UNUSED,
                    format!("`{label}` reads {what} and never passes it on"),
                )
                .at(callee_span, format!("`{first}` not passed"))
                .reason("Everything a route reads from a request reaches what it runs by name; a value read and ignored is a URL or a field that does nothing.")
                .next(format!("Pass `{first}` to `{name}`, or remove it from the route")),
            );
            return None;
        }
        Some(args)
    }

    /// `current_principal`, a kind of caller selected from it
    /// (`current_principal.user`) or a field read through that: the
    /// signed-in caller (area 8). Selecting a kind narrows the route: a
    /// caller of the other kind is refused before it runs.
    fn caller_argument(
        &mut self,
        expr: &Expr,
        first: &ast::NameRef,
        rest: &[ast::NameRef],
        used: &mut BTreeSet<String>,
    ) -> Option<(Arg, Ty)> {
        let _ = expr;
        let span = self.span(first);
        if self.authentication == Authentication::Public {
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_AUTH_PUBLIC_PRINCIPAL,
                    "A route anyone may call passes `current_principal`",
                )
                .at(span, "no one is signed in here")
                .reason("`authentication: RouteAuthentication.none` reads no credential, so the route has no caller to pass on, and nothing it runs may use one.")
                .next("Remove `authentication: RouteAuthentication.none` so callers sign in, or stop passing `current_principal`"),
            );
            return None;
        }
        used.insert("current_principal".to_string());
        // Read only by a route that passes its caller, so an edit to the
        // principal reads again only those.
        let identity = std::sync::Arc::clone(crate::identity::identity(self.db, self.project));
        let mut names = rest.iter();
        let Some(selected) = names.next() else {
            return Some((
                Arg {
                    root: ArgRoot::Caller,
                    path: Vec::new(),
                },
                Ty::Opaque(crate::identity::PRINCIPAL.to_string(), Vec::new()),
            ));
        };
        let written = tree::name_ref_text(selected)?;
        let kind =
            PrincipalKind::from_name(&written).filter(|kind| identity.written.contains(kind));
        let Some(kind) = kind else {
            let reason = match PrincipalKind::from_name(&written) {
                Some(kind) => format!(
                    "`principal {{ }}` declares no `{}:`, so no {} ever signs in; it declares {}.",
                    kind.name(),
                    kind.name(),
                    identity.kinds_line()
                ),
                None if crate::identity::PRINCIPAL_FIELDS.contains(&written.as_str()) => format!(
                    "`current_principal` is either kind of caller, so a route selects one before reading its fields: `current_principal.user.{written}`."
                ),
                None => format!(
                    "A route selects a kind of caller the principal declares, {}: `current_principal.user` is the signed-in user.",
                    identity.kinds_line()
                ),
            };
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_AUTH_PRINCIPAL_VARIANT_UNKNOWN,
                    format!("`current_principal.{written}` isn't a kind of caller"),
                )
                .at(self.span(selected), "not declared")
                .reason(reason)
                .next(if identity.written.contains(&PrincipalKind::User) {
                    format!(
                        "Write `current_principal.user` instead of `current_principal.{written}`"
                    )
                } else {
                    "Select a kind of caller `principal { }` declares".to_string()
                }),
            );
            return None;
        };
        if let Some((other, other_span)) = &self.caller
            && *other != kind
        {
            let other_span = other_span.clone();
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_AUTH_PRINCIPAL_VARIANT_UNKNOWN,
                    "A route passes both a user and a service",
                )
                .at(self.span(selected), "the other kind")
                .also(other_span, "selected here")
                .reason("A request comes from one caller, a user or a service, so a route that needs both could never run.")
                .next("Select one kind of caller"),
            );
            return None;
        }
        self.caller = Some((kind, span));
        let mut ty = Ty::Opaque(kind.type_name().to_string(), Vec::new());
        let mut path = Vec::new();
        if let Some(field) = names.next() {
            let name = tree::name_ref_text(field)?;
            let Some(found) = identity.field_type(kind, &name) else {
                let listed: Vec<String> = crate::identity::PRINCIPAL_FIELDS
                    .iter()
                    .map(|f| format!("`{f}`"))
                    .collect();
                self.out.push(
                    Diagnostic::new(
                        &rule::TYPE_UNKNOWN_FIELD,
                        format!("`{}` has no field `{name}`", kind.type_name()),
                    )
                    .at(self.span(field), "not a caller's field")
                    .reason(format!(
                        "A caller's fields are {}; anything else is read from its entity by what the route runs.",
                        crate::and_list(&listed)
                    ))
                    .next(format!("Pass `current_principal.{}` and read the rest in what the route runs", kind.name())),
                );
                return None;
            };
            ty = found;
            path.push(name);
            if let Some(more) = names.next() {
                self.out.push(
                    Diagnostic::new(
                        &rule::TYPE_FIELD_ON_NON_RECORD,
                        format!("`{ty}` has no fields"),
                    )
                    .at(self.span(more), "no fields")
                    .reason("Only objects have fields to read.")
                    .next("Pass the value itself"),
                );
                return None;
            }
        }
        Some((
            Arg {
                root: ArgRoot::Caller,
                path,
            },
            ty,
        ))
    }

    fn argument_invalid(&self, expr: &Expr, what: &str) -> Diagnostic {
        Diagnostic::new(
            &rule::ROUTE_ARGUMENT_INVALID,
            format!("A route passes its placeholders and input, not {what}"),
        )
        .at(self.span(expr), what.to_string())
        .reason("A route is a thin door: what it runs gets the route's placeholders, its `input` and their fields as written, and works out anything else itself.")
        .next("Pass a placeholder, `input` or a field of them, and move the rest into what the route runs")
    }

    /// One argument: a placeholder or `input`, or fields read through them,
    /// with its type.
    fn argument(
        &mut self,
        expr: &Expr,
        params: &[(&str, &Ty)],
        input: Option<&str>,
        used: &mut BTreeSet<String>,
    ) -> Option<(Arg, Ty)> {
        if let Some(result) = self.answering.clone() {
            return self.answer_argument(expr, result);
        }
        let Some((first, rest)) = tree::dotted(expr) else {
            let what = match expr {
                Expr::Literal(_) => "a literal",
                Expr::CallExpr(_) => "a call",
                Expr::RecordExpr(_) => "a value built in the route",
                _ => "an expression",
            };
            self.out.push(self.argument_invalid(expr, what));
            return None;
        };
        let root = tree::name_ref_text(&first)?;
        let (arg_root, ty) = if root == "input" && input.is_some() {
            (ArgRoot::Input, Ty::Named(input?.to_string()))
        } else if let Some((_, ty)) = params.iter().find(|(name, _)| *name == root) {
            (ArgRoot::Param(root.clone()), (*ty).clone())
        } else if root == "current_principal" {
            return self.caller_argument(expr, &first, &rest, used);
        } else if let Some(named) = crate::not_value::classify(self.lookup, self.lookup, &root) {
            // A declaration's name is no value (area 3).
            self.out.push(crate::not_value::diagnostic(
                &root,
                &named,
                self.span(&first),
            ));
            return None;
        } else {
            let mut known: Vec<&str> = params.iter().map(|(name, _)| *name).collect();
            if input.is_some() {
                known.push("input");
            }
            let near = nearest(&root, known.iter().copied());
            let span = self.span(&first);
            let reason = if root == "input" {
                "This route has no `input:`, so there is no `input` to pass.".to_string()
            } else {
                format!(
                    "A route's call reads its placeholders and `input`{}.{}",
                    if known.is_empty() {
                        String::new()
                    } else {
                        format!(
                            ": {}",
                            crate::and_list(
                                &known.iter().map(|k| format!("`{k}`")).collect::<Vec<_>>()
                            )
                        )
                    },
                    near.map_or_else(String::new, |near| format!(" Did you mean `{near}`?"))
                )
            };
            let mut problem = Diagnostic::new(
                &rule::SEMANTIC_UNKNOWN_NAME,
                format!("`{root}` isn't something this route reads"),
            )
            .at(span.clone(), "not declared")
            .reason(reason);
            problem = match near {
                Some(near) => problem
                    .next(format!("Use `{near}` instead of `{root}`"))
                    .edit(span, near),
                None if root == "input" => problem.next("Add `input: <Input type>` to the route"),
                None => problem.next(format!(
                    "Add `{{{root}: <Type>}}` to the path, or pass something the route reads"
                )),
            };
            self.out.push(problem);
            return None;
        };
        used.insert(root);
        let (path, ty) = self.through(ty, &rest)?;
        Some((
            Arg {
                root: arg_root,
                path,
            },
            ty,
        ))
    }

    /// The fields `rest` read through a value of type `ty`, and the type
    /// they reach.
    fn through(&mut self, mut ty: Ty, rest: &[ast::NameRef]) -> Option<(Vec<String>, Ty)> {
        let mut path = Vec::new();
        for name in rest {
            let field = tree::name_ref_text(name)?;
            let host = match self.lookup.structure(&ty) {
                Ty::Named(host) if matches!(self.lookup.def(&host), Some(Def::Fields { .. })) => {
                    host
                }
                other => {
                    self.out.push(
                        Diagnostic::new(
                            &rule::TYPE_FIELD_ON_NON_RECORD,
                            format!("`{other}` has no fields"),
                        )
                        .at(self.span(name), "no fields")
                        .reason("Only objects have fields to read.")
                        .next("Pass the value itself"),
                    );
                    return None;
                }
            };
            let Some(def) = self
                .lookup
                .field(&host, std::slice::from_ref(&field))
                .cloned()
            else {
                let span = self.span(name);
                self.out.push(
                    Diagnostic::new(
                        &rule::TYPE_UNKNOWN_FIELD,
                        format!("`{host}` has no field `{field}`"),
                    )
                    .at(span, "not declared")
                    .reason(format!(
                        "`{host}`'s fields are {}.",
                        crate::and_list(
                            &self
                                .lookup
                                .fields(&host)
                                .unwrap_or_default()
                                .iter()
                                .map(|f| format!("`{}`", f.name))
                                .collect::<Vec<_>>()
                        )
                    ))
                    .next(format!("Name a field of `{host}`")),
                );
                return None;
            };
            ty = self.lookup.field_type(&host, std::slice::from_ref(&field));
            if def.optional && def.default.is_none() {
                ty = ty.nullable();
            }
            path.push(field);
        }
        Some((path, ty))
    }

    /// One argument of `settle:`'s `answer:`: `result`, what the action
    /// returned (of type `result`), or a field read through it.
    fn answer_argument(&mut self, expr: &Expr, result: Ty) -> Option<(Arg, Ty)> {
        let dotted = tree::dotted(expr);
        let root = dotted
            .as_ref()
            .and_then(|(first, _)| tree::name_ref_text(first));
        let Some(((_, rest), "result")) = dotted.as_ref().zip(root.as_deref()) else {
            let written = tree::written(expr.syntax());
            self.out.push(
                Diagnostic::new(
                    &rule::ROUTE_SETTLE_INVALID,
                    format!("`answer:` passes `{written}`, not `result` or its fields"),
                )
                .at(self.span(expr), "not `result`")
                .reason(SETTLE_REASON)
                .next("Pass `result` or a field of it, such as `result.id`"),
            );
            return None;
        };
        let rest = rest.clone();
        let (path, ty) = self.through(result, &rest)?;
        Some((
            Arg {
                root: ArgRoot::Result,
                path,
            },
            ty,
        ))
    }
}

/// Whether the contract's JSON sends a value of `prim` as an object: an
/// amount of money, a quantity and a `ZonedTime` (area 19), which a URL's
/// segment or a query string can't hold, as it can't hold any object.
fn sent_as_object(prim: Prim) -> bool {
    matches!(
        prim.representation(),
        Prim::Money | Prim::ZonedTime | Prim::Quantity(_)
    )
}

/// A placeholder typed `Host.missing`, a field `Host` doesn't declare
/// (chunk 6.6d), as a field type elsewhere is reported.
fn unknown_placeholder_field(
    host: &str,
    missing: &str,
    fields: &[crate::types::FieldDef],
    span: Span,
) -> Diagnostic {
    let near = nearest(missing, fields.iter().map(|field| field.name.as_str())).map(str::to_string);
    let mean = near
        .as_deref()
        .map_or_else(String::new, |near| format!(" Did you mean `{near}`?"));
    let problem = Diagnostic::new(
        &rule::TYPE_UNKNOWN_FIELD,
        format!("`{host}` has no field `{missing}`"),
    )
    .at(span.clone(), "not declared")
    .reason(format!(
        "A field type is written `Type.field`, naming a field the type declares.{mean}"
    ));
    match near {
        Some(near) => problem
            .next(format!("Use `{host}.{near}`"))
            .edit(span, format!("{host}.{near}")),
        None => problem.next(format!("Name a field of `{host}`")),
    }
}
