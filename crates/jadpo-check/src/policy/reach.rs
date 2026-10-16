//! What each route reaches, checked against policy (area 9, "Grants" and
//! "Anyone signed in, or anyone"): policy is always on, so every create,
//! read, update and delete a route reaches needs a grant, an action that
//! reaches no entity needs a line under root `policy { actions: }`, and a
//! route anyone may call (`authentication: RouteAuthentication.none`)
//! reaches only what `Access.public` is granted.
//!
//! Each file's actions and fixtures are read from their typed bodies, by
//! name and without positions, as the store rules read them
//! (`types::stores`): the entities each reads, creates, updates and
//! deletes in its own body, the actions and queries it calls, and whether
//! it signs a caller in or out. A route's action is checked against its
//! own line under `actions:` for the writes in its own body, and every
//! action it calls against that action's entities, with the same caller.

use std::collections::{BTreeMap, BTreeSet};

use jadpo_db::{Db, File, Project};
use jadpo_diag::{Diagnostic, rule};
use jadpo_graph::Kind;
use jadpo_graph::body::{Callee, Expr as BodyExpr, Node};
use jadpo_std::http::{PrincipalKind, SessionOp};
use jadpo_std::write::WriteOp;

use super::{Access, Effect, Grant};
use crate::lookups::{Lookup, Name};
use crate::types::{callables_of, typed_body};

/// One thing an action, fixture or query does that policy decides.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Act {
    /// An entity's records, by entity name, as its own body acts them.
    Entity(String, Effect),
    /// An action it calls, by name.
    Calls(String),
    /// A query it calls, by name.
    Queries(String),
    /// `sessions.start`, `.end` or `.issue_token`: signing a caller in or
    /// out, which the application's own checks decide (area 8).
    SignsIn,
}

/// What each action and fixture in `file` acts, by name.
#[salsa::tracked(returns(ref))]
pub fn acts(db: &dyn Db, project: Project, file: File) -> BTreeMap<String, BTreeSet<Act>> {
    let identity = crate::identity::identity(db, project);
    let user = identity
        .variant(PrincipalKind::User)
        .map(|v| v.entity.clone());
    let service = identity
        .variant(PrincipalKind::Service)
        .map(|v| v.entity.clone());
    let mut out = BTreeMap::new();
    for callable in callables_of(db, file) {
        let checked = typed_body(db, project, callable);
        let Some(key) = checked
            .key
            .as_ref()
            .filter(|key| matches!(key.kind(), Some(Kind::Action | Kind::Fixture)))
        else {
            continue;
        };
        let mut found = BTreeSet::new();
        for site in &checked.body.sites {
            let act = match &site.node {
                Node::Expr(BodyExpr::Write { entity, op, .. }) => {
                    let effect = match op {
                        WriteOp::Create => Effect::Create,
                        WriteOp::Update => Effect::Update,
                        WriteOp::Delete => Effect::Delete,
                    };
                    Act::Entity(entity.name().to_string(), effect)
                }
                Node::Expr(BodyExpr::Call {
                    callee: Callee::Action(callee),
                    ..
                }) => Act::Calls(callee.name().to_string()),
                Node::Expr(BodyExpr::Call {
                    callee: Callee::Entity { entity, operation },
                    ..
                }) if operation == "get" => Act::Entity(entity.name().to_string(), Effect::Read),
                Node::Expr(BodyExpr::Call {
                    callee: Callee::Query(query),
                    ..
                }) => Act::Queries(query.name().to_string()),
                Node::Expr(BodyExpr::Call {
                    callee: Callee::Session(op),
                    ..
                }) => match session_act(*op, user.as_deref(), service.as_deref()) {
                    Some(act) => act,
                    None => continue,
                },
                _ => continue,
            };
            found.insert(act);
        }
        out.insert(key.name().to_string(), found);
    }
    out
}

/// What a session or key operation acts (area 9, question 8): listing a
/// user's tokens or a service's keys reads its record, revoking one or
/// issuing a key changes it, and starting, ending and issuing a token
/// sign a caller in or out.
pub fn session_act(op: SessionOp, user: Option<&str>, service: Option<&str>) -> Option<Act> {
    Some(match op {
        SessionOp::Start | SessionOp::End | SessionOp::IssueToken => Act::SignsIn,
        SessionOp::Tokens => Act::Entity(user?.to_string(), Effect::Read),
        SessionOp::RevokeToken => Act::Entity(user?.to_string(), Effect::Update),
        SessionOp::Keys => Act::Entity(service?.to_string(), Effect::Read),
        SessionOp::IssueKey | SessionOp::RevokeKey => {
            Act::Entity(service?.to_string(), Effect::Update)
        }
    })
}

/// Every action's and fixture's acts, across the project.
#[salsa::tracked(returns(ref))]
pub fn all_acts(db: &dyn Db, project: Project) -> BTreeMap<String, BTreeSet<Act>> {
    let mut out: BTreeMap<String, BTreeSet<Act>> = BTreeMap::new();
    for &file in project.files(db) {
        for (name, found) in acts(db, project, file) {
            out.entry(name.clone())
                .or_default()
                .extend(found.iter().cloned());
        }
    }
    out
}

/// What the query called `name` reads: its entity and every included one.
pub fn query_reads(db: &dyn Db, project: Project, name: &str) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(query) =
        crate::data::queries::query_model(db, project, Name::new(db, name.to_string())).as_ref()
    {
        out.push(query.entity.clone());
        let mut stack: Vec<&crate::data::queries::Include> = query.includes.iter().collect();
        while let Some(include) = stack.pop() {
            if !out.contains(&include.entity) {
                out.push(include.entity.clone());
            }
            stack.extend(include.includes.iter());
        }
    }
    out
}

/// One effect a route reaches, with the grants that decide it.
struct Reached {
    entity: String,
    effect: Effect,
    grants: Vec<Grant>,
    /// Whether the entity's records belong to a tenant.
    tenant: bool,
}

/// Everything the route running `action` (or the query `query`) reaches,
/// with the grants deciding each; and whether it reaches any entity or
/// signs anyone in.
fn reached(
    db: &dyn Db,
    project: Project,
    access: &Access,
    run: &str,
    query: bool,
) -> (Vec<Reached>, bool) {
    let acts = all_acts(db, project);
    let entities = crate::data::Entities::new(db, project);
    let mut out: Vec<Reached> = Vec::new();
    let mut any = false;
    let push = |entity: &str, effect: Effect, grants: Vec<Grant>, out: &mut Vec<Reached>| {
        if out.iter().any(|r| r.entity == entity && r.effect == effect) {
            return;
        }
        let tenant = access
            .entity(entity)
            .is_some_and(|policy| policy.tenancy.is_some());
        out.push(Reached {
            entity: entity.to_string(),
            effect,
            grants,
            tenant,
        });
    };
    let own = |entity: &str, effect: Effect| -> Vec<Grant> {
        access
            .entity(entity)
            .map(|policy| policy.grants(effect).to_vec())
            .unwrap_or_default()
    };
    if query {
        for entity in query_reads(db, project, run) {
            any = true;
            push(&entity, Effect::Read, own(&entity, Effect::Read), &mut out);
        }
        return (out, any);
    }
    let listed = access.root.action(run);
    let mut seen = BTreeSet::from([run.to_string()]);
    let mut queue = vec![(run.to_string(), true)];
    while let Some((current, first)) = queue.pop() {
        for act in acts.get(&current).into_iter().flatten() {
            match act {
                Act::Entity(entity, effect) => {
                    any = true;
                    let grants = match (first, listed, effect) {
                        (true, Some(line), Effect::Create | Effect::Update | Effect::Delete) => {
                            match entities.get(entity) {
                                Some(model) => line.on(model, &access.memberships),
                                None => Vec::new(),
                            }
                        }
                        _ => own(entity, *effect),
                    };
                    push(entity, *effect, grants, &mut out);
                }
                Act::Calls(callee) => {
                    if seen.insert(callee.clone()) {
                        queue.push((callee.clone(), false));
                    }
                }
                Act::Queries(query) => {
                    for entity in query_reads(db, project, query) {
                        any = true;
                        push(&entity, Effect::Read, own(&entity, Effect::Read), &mut out);
                    }
                }
                Act::SignsIn => any = true,
            }
        }
    }
    (out, any)
}

/// What `effect` does to an entity, for a headline: "creates a `Note`".
fn doing(effect: Effect, entity: &str) -> String {
    match effect {
        Effect::Create => format!("creates a `{entity}`"),
        Effect::Read => format!("reads `{entity}` records"),
        Effect::Update => format!("changes `{entity}` records"),
        Effect::Delete => format!("deletes `{entity}` records"),
    }
}

/// The routes' policy problems: one per route at most, at the first thing
/// it reaches that it may never do.
#[salsa::tracked(returns(ref))]
pub fn route_problems(db: &dyn Db, project: Project) -> Vec<Diagnostic> {
    // Policy checks what the routes reach once the routes and the policy
    // read cleanly: a route table with problems is reported first.
    if !super::policy_clean(db, project) || !crate::routes::routes_clean(db, project) {
        return Vec::new();
    }
    let access = super::access(db, project);
    let lookup = Lookup::new(db, project);
    let mut out = Vec::new();
    for name in crate::routes::group_names(db, project) {
        let Some(read) = crate::routes::group_read(db, project, Name::new(db, name.clone())) else {
            continue;
        };
        let Some(model) = &read.model else { continue };
        for route in &model.routes {
            let label = route.label();
            let Some(span) = read
                .spans
                .iter()
                .find(|(l, _)| *l == label)
                .map(|(_, span)| span.clone())
            else {
                continue;
            };
            let anyone = !route.authentication.needs_caller();
            let run = if route.run.query {
                route.run.name.clone()
            } else {
                match crate::graph::declaration_key(&lookup, &route.run.name) {
                    Some(key) => key.name().to_string(),
                    None => continue,
                }
            };
            let (reached, any) = reached(db, project, access, &run, route.run.query);
            let listed = (!route.run.query)
                .then(|| access.root.action(&run))
                .flatten();
            if !any {
                match listed {
                    None => out.push(
                        Diagnostic::new(
                            &rule::POLICY_EFFECT_UNGRANTED,
                            format!("`{label}` runs `{}`, which nobody may call", route.run.name),
                        )
                        .at(span, "reaches no entity")
                        .reason(format!(
                            "An action takes its permission from the entities it reaches, and `{}` reaches none, so it is refused to everyone unless root `policy {{ }}` lists who may call it under `actions:`.",
                            route.run.name
                        ))
                        .next(format!(
                            "List `{}: [...]` under `actions:` in `policy.jadpo`",
                            written_action(&lookup, &route.run.name)
                        )),
                    ),
                    Some(line) if anyone && !line.who.contains(&super::ActionWho::Public) => out.push(
                        public_missing(&label, span, &format!("`{}`'s line under `actions:`", line.written), "Add `Access.public` to the line, or remove `authentication: RouteAuthentication.none` from the route"),
                    ),
                    Some(_) => {}
                }
                continue;
            }
            for reach in &reached {
                let real = reach.grants.iter().any(|g| !matches!(g, Grant::System));
                if !real {
                    let from = match listed {
                        Some(line)
                            if matches!(
                                reach.effect,
                                Effect::Create | Effect::Update | Effect::Delete
                            ) && touches_own(db, project, &run, &reach.entity, reach.effect) =>
                        {
                            format!(
                                "`{}`'s line under `actions:` grants nobody on `{}`",
                                line.written, reach.entity
                            )
                        }
                        _ => format!(
                            "`{}`'s policy grants nobody `{}`",
                            reach.entity,
                            reach.effect.word()
                        ),
                    };
                    out.push(
                        Diagnostic::new(
                            &rule::POLICY_EFFECT_UNGRANTED,
                            format!("`{label}` {}, which nobody may do", doing(reach.effect, &reach.entity)),
                        )
                        .at(span, format!("{} here", reach.effect.word()))
                        .reason(format!(
                            "Policy is always on: every create, read, update and delete a route reaches needs a grant, and {from}, so this route could never succeed."
                        ))
                        .next(format!(
                            "Add `{}: [...]` to `{}`'s `policy:`, naming who may {} one",
                            reach.effect.word(),
                            reach.entity,
                            reach.effect.word()
                        )),
                    );
                    break;
                }
                if anyone && (reach.tenant || !reach.grants.contains(&Grant::Public)) {
                    let why = if reach.tenant && reach.grants.contains(&Grant::Public) {
                        format!(
                            "`{}`'s records belong to a tenant, and every grant on them, `Access.public` included, applies only within the tenants where the caller has a membership",
                            reach.entity
                        )
                    } else {
                        format!(
                            "`{}`'s `{}:` doesn't name `Access.public`",
                            reach.entity,
                            reach.effect.word()
                        )
                    };
                    out.push(
                        Diagnostic::new(
                            &rule::POLICY_PUBLIC_GRANT_MISSING,
                            format!(
                                "`{label}` lets anyone call it, but not everyone may {} `{}`",
                                reach.effect.word(),
                                reach.entity
                            ),
                        )
                        .at(span, "anyone may call it")
                        .reason(format!(
                            "A route with `authentication: RouteAuthentication.none` has no caller, so everything it reaches needs `Access.public`; {why}. Either alone is not enough."
                        ))
                        .next(if reach.tenant {
                            "Remove `authentication: RouteAuthentication.none` from the route".to_string()
                        } else {
                            format!(
                                "Add `Access.public` to `{}`'s `{}:`, or remove `authentication: RouteAuthentication.none` from the route",
                                reach.entity,
                                reach.effect.word()
                            )
                        }),
                    );
                    break;
                }
            }
        }
    }
    out
}

/// Whether the action `run`'s own body does `effect` to `entity`.
fn touches_own(db: &dyn Db, project: Project, run: &str, entity: &str, effect: Effect) -> bool {
    all_acts(db, project)
        .get(run)
        .is_some_and(|found| found.contains(&Act::Entity(entity.to_string(), effect)))
}

fn public_missing(label: &str, span: jadpo_diag::Span, what: &str, next: &str) -> Diagnostic {
    Diagnostic::new(
        &rule::POLICY_PUBLIC_GRANT_MISSING,
        format!("`{label}` lets anyone call it, but {what} doesn't"),
    )
    .at(span, "anyone may call it")
    .reason("A route with `authentication: RouteAuthentication.none` has no caller, so what it runs needs `Access.public`. Either alone is not enough.")
    .next(next)
}

/// How `actions:` names the action called `name`: `domain.action`, or the
/// bare name in a single-file project.
fn written_action(lookup: &Lookup<'_>, name: &str) -> String {
    use crate::lookups::NameView;
    match lookup
        .declared(name)
        .iter()
        .find(|s| s.kind == jadpo_db::ItemKind::Action)
        .map(|s| &s.ownership)
    {
        Some(jadpo_db::Ownership::Domain(domain)) => format!("{domain}.{name}"),
        _ => name.to_string(),
    }
}
