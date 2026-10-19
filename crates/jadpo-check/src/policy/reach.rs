//! What each route reaches, checked against policy (area 9, "Grants" and
//! "Anyone signed in, or anyone"): policy is always on, so every create,
//! read, update and delete a route reaches needs a grant, an action that
//! reaches no entity needs a line under root `policy { actions: }`, and a
//! route anyone may call (`authentication: RouteAuthentication.none`)
//! reaches only what `Access.public` is granted.
//!
//! Each action and fixture is read by name from what its body does
//! (`types::body_facts`, without positions), as the store rules read it:
//! the entities it reads, creates, updates and deletes in its own body,
//! the actions and queries it calls, and whether it signs a caller in or
//! out. A route's action is checked against its own line under `actions:`
//! for the writes in its own body, and every action it calls against that
//! action's entities, with the same caller. Each route group's file is
//! checked by its own query ([`reach_problems`]), reading the policies of
//! the entities its routes reach by name, so an edit rechecks only the
//! routes that reach what changed (chunk 4.11).

use std::collections::BTreeSet;

use jadpo_db::{Db, File, ItemKind, Project, item_tree};
use jadpo_diag::{Diagnostic, rule};
use jadpo_graph::Kind;
use jadpo_std::http::{PrincipalKind, SessionOp};
use jadpo_std::write::WriteOp;

use super::{ActionWho, Effect, Grant, LineGate, Policies};
use crate::lookups::{Lookup, Name};
use crate::types::{Fact, body_facts, named};

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
    /// A field its own body always changes, by entity and field name: an
    /// update's `set:`, or a create giving a field that can't be `none`.
    Field(String, String, Effect),
}

/// What the action or fixture called `name` acts, read from what its body
/// does: every declaration of that name's, together.
#[salsa::tracked(returns(ref))]
pub fn acts_of<'db>(db: &'db dyn Db, project: Project, name: Name<'db>) -> BTreeSet<Act> {
    let entities = crate::data::Entities::new(db, project);
    let mut found = BTreeSet::new();
    for callable in named(db, project, name.text(db)) {
        let facts = body_facts(db, project, callable);
        if facts.key_of(&[Kind::Action, Kind::Fixture]).is_none() {
            continue;
        }
        for fact in &facts.facts {
            let act = match fact {
                Fact::Writes(write) => {
                    let effect = match write.op {
                        WriteOp::Create => Effect::Create,
                        WriteOp::Update => Effect::Update,
                        WriteOp::Delete => Effect::Delete,
                    };
                    let name = &write.entity;
                    if write.transition.is_none() && effect != Effect::Delete {
                        for field in &write.set {
                            let always = effect == Effect::Update
                                || entities.get(name).is_some_and(|model| {
                                    model.fields.iter().any(|f| f.name == *field && !f.nullable)
                                });
                            if always {
                                found.insert(Act::Field(name.clone(), field.clone(), effect));
                            }
                        }
                    }
                    Act::Entity(name.clone(), effect)
                }
                Fact::Action(callee) => Act::Calls(callee.name().to_string()),
                Fact::Gets(entity) => Act::Entity(entity.clone(), Effect::Read),
                Fact::Queries(query) => Act::Queries(query.clone()),
                Fact::Session(op) => {
                    let identity = crate::identity::identity(db, project);
                    let user = identity
                        .variant(PrincipalKind::User)
                        .map(|v| v.entity.clone());
                    let service = identity
                        .variant(PrincipalKind::Service)
                        .map(|v| v.entity.clone());
                    match session_act(*op, user.as_deref(), service.as_deref()) {
                        Some(act) => act,
                        None => continue,
                    }
                }
                // A batch's run works as the system, after the action
                // commits (area 12), so nothing it does is the caller's.
                Fact::Fixture(_)
                | Fact::Function(_)
                | Fact::Emits(_)
                | Fact::Batch(_)
                | Fact::Runs(_) => continue,
            };
            found.insert(act);
        }
    }
    found
}

/// What the action called `name` acts.
fn acts<'db>(db: &'db dyn Db, project: Project, name: &str) -> &'db BTreeSet<Act> {
    acts_of(db, project, Name::new(db, name.to_string()))
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

/// The entities whose records the action called `action` creates, changes
/// or deletes in its own body, by name.
pub fn own_writes(db: &dyn Db, project: Project, action: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for act in acts(db, project, action) {
        if let Act::Entity(entity, Effect::Create | Effect::Update | Effect::Delete) = act
            && !out.contains(entity)
        {
            out.push(entity.clone());
        }
    }
    out
}

/// Who the line under `actions:` for the action called `action` lets call
/// it as it starts, when it is listed (area 9, "Actions").
pub fn line_gate(db: &dyn Db, project: Project, action: &str) -> Option<LineGate> {
    let policies = Policies::new(db, project);
    let line = policies.root().action(action)?;
    let entities = crate::data::Entities::new(db, project);
    let names = own_writes(db, project, action);
    let writes: Vec<&crate::data::EntityModel> =
        names.iter().filter_map(|name| entities.get(name)).collect();
    Some(line.gate(&writes, policies.memberships()))
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
    /// For a field whose own `policy:` narrows the record's: its name.
    field: Option<String>,
}

/// Everything the route running `action` (or the query `query`) reaches,
/// with the grants deciding each; and whether it reaches any entity or
/// signs anyone in.
fn reached(
    db: &dyn Db,
    project: Project,
    access: &Policies<'_>,
    run: &str,
    query: bool,
) -> (Vec<Reached>, bool) {
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
            field: None,
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
    let listed = access.root().action(run);
    let mut seen = BTreeSet::from([run.to_string()]);
    let mut queue = vec![(run.to_string(), true)];
    while let Some((current, first)) = queue.pop() {
        for act in acts(db, project, &current) {
            match act {
                Act::Entity(entity, effect) => {
                    any = true;
                    let grants = match (first, listed, effect) {
                        (true, Some(line), Effect::Create | Effect::Update | Effect::Delete) => {
                            match entities.get(entity) {
                                Some(model) => line.on(model, access.memberships()),
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
                Act::Field(entity, field, effect) => {
                    let narrowed = access
                        .entity(entity)
                        .and_then(|policy| policy.field_grants(field, *effect));
                    if let Some(grants) = narrowed
                        && !out
                            .iter()
                            .any(|r| r.field.as_deref() == Some(field) && r.entity == *entity)
                    {
                        out.push(Reached {
                            entity: entity.clone(),
                            effect: *effect,
                            grants: grants.to_vec(),
                            tenant: false,
                            field: Some(field.clone()),
                        });
                    }
                }
            }
        }
    }
    (out, any)
}

/// What a route that lists a user's tokens or a service's keys itself
/// (`sessions.tokens`, `Service.keys`; area 8) reaches: a read of the
/// holder's record, the grant its listing is limited by (area 9, question
/// 8).
fn generated_reach(
    db: &dyn Db,
    project: Project,
    access: &Policies<'_>,
    op: SessionOp,
) -> (Vec<Reached>, bool) {
    let identity = crate::identity::identity(db, project);
    let user = identity
        .variant(PrincipalKind::User)
        .map(|v| v.entity.clone());
    let service = identity
        .variant(PrincipalKind::Service)
        .map(|v| v.entity.clone());
    let Some(Act::Entity(entity, effect)) = session_act(op, user.as_deref(), service.as_deref())
    else {
        return (Vec::new(), true);
    };
    let tenant = access
        .entity(&entity)
        .is_some_and(|policy| policy.tenancy.is_some());
    let grants = access
        .entity(&entity)
        .map(|policy| policy.grants(effect).to_vec())
        .unwrap_or_default();
    (
        vec![Reached {
            entity,
            effect,
            grants,
            tenant,
            field: None,
        }],
        true,
    )
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

/// The policy problems of the routes `file` declares: one per route at
/// most, at the first thing it reaches that it may never do.
#[salsa::tracked(returns(ref))]
pub fn reach_problems(db: &dyn Db, project: Project, file: File) -> Vec<Diagnostic> {
    // Policy checks what the routes reach once the routes and the policy
    // read cleanly: a route table with problems is reported first.
    if !item_tree(db, file)
        .items
        .iter()
        .any(|item| item.kind == ItemKind::Routes)
        || !super::policy_clean(db, project)
        || !crate::routes::routes_clean(db, project)
    {
        return Vec::new();
    }
    let path = file.path(db);
    let access = &Policies::new(db, project);
    let lookup = Lookup::new(db, project);
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    for item in &item_tree(db, file).items {
        if item.kind != ItemKind::Routes || !seen.insert(item.name.as_str()) {
            continue;
        }
        let Some(read) = crate::routes::group_read(db, project, Name::new(db, item.name.clone()))
        else {
            continue;
        };
        if read.path != *path {
            continue;
        }
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
            // A query with problems of its own reaches nothing known; its
            // problems are reported where it is declared.
            if route.run.query
                && route.run.generated.is_none()
                && crate::data::queries::query_read(
                    db,
                    project,
                    Name::new(db, route.run.name.clone()),
                )
                .as_ref()
                .is_some_and(|read| read.model.is_none())
            {
                continue;
            }
            let anyone = !route.authentication.needs_caller();
            let run = if route.run.query {
                route.run.name.clone()
            } else {
                match crate::graph::declaration_key(&lookup, &route.run.name) {
                    Some(key) => key.name().to_string(),
                    None => continue,
                }
            };
            let (reached, any) = match route.run.generated {
                Some(op) => generated_reach(db, project, access, op),
                None => reached(db, project, access, &run, route.run.query),
            };
            let listed = (!route.run.query)
                .then(|| access.root().action(&run))
                .flatten();
            if let Some(line) = listed
                && line_gate(db, project, &run) == Some(LineGate::Nobody)
            {
                out.push(nobody_passes(&label, span, &route.run.name, line, access));
                continue;
            }
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
                    Some(line) if anyone && !line.who.contains(&ActionWho::Public) => out.push(
                        public_missing(&label, span, &format!("`{}`'s line under `actions:`", line.written), "Add `Access.public` to the line, or remove `authentication: RouteAuthentication.none` from the route"),
                    ),
                    Some(_) => {}
                }
                continue;
            }
            for reach in &reached {
                let real = reach.grants.iter().any(|g| !matches!(g, Grant::System));
                if !real && let Some(field) = &reach.field {
                    let server_only = access
                        .entity(&reach.entity)
                        .and_then(|policy| policy.field(field))
                        .is_some_and(|own| own.server_only);
                    let why = if server_only {
                        format!(
                            "`{}.{field}` is `FieldAccess.server_only`, which no client may change",
                            reach.entity
                        )
                    } else {
                        format!(
                            "`{}.{field}`'s own `{}:` names nobody",
                            reach.entity,
                            reach.effect.word()
                        )
                    };
                    out.push(
                        Diagnostic::new(
                            &rule::POLICY_EFFECT_UNGRANTED,
                            format!("`{label}` sets `{}.{field}`, which nobody may", reach.entity),
                        )
                        .at(span, format!("sets `{field}`"))
                        .reason(format!(
                            "Policy is always on, and a field's own `policy:` narrows who may change it: {why}, so this route could never succeed."
                        ))
                        .next(format!("Leave `{field}` out of what this route's action writes")),
                    );
                    break;
                }
                if reach.field.is_some() {
                    continue;
                }
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

/// What background work reaches that only `Access.public` lets it in to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PublicOnly {
    Entity {
        entity: String,
        effect: Effect,
    },
    /// The action's line under `actions:`.
    Line {
        written: String,
    },
}

/// The first thing the action called `run`, run as the system, reaches
/// whose grants name `Access.public` but not `Access.system` (area 9,
/// "Background work and `Access.public`", decided 2026-10-19): a
/// statement anyone at all may run tests no caller, so it would let the
/// system in without the entity ever granting background work.
pub fn public_only(db: &dyn Db, project: Project, run: &str) -> Option<PublicOnly> {
    let access = &Policies::new(db, project);
    if let Some(line) = access.root().action(run)
        && line.who.contains(&ActionWho::Public)
        && !line.who.contains(&ActionWho::System)
    {
        return Some(PublicOnly::Line {
            written: line.written.clone(),
        });
    }
    public_only_reached(db, project, access, run, false)
}

/// The same for the query called `query`, which a batch's run reads as
/// the system (chunk 6.4c).
pub fn public_only_query(db: &dyn Db, project: Project, query: &str) -> Option<PublicOnly> {
    public_only_reached(db, project, &Policies::new(db, project), query, true)
}

fn public_only_reached(
    db: &dyn Db,
    project: Project,
    access: &Policies<'_>,
    run: &str,
    query: bool,
) -> Option<PublicOnly> {
    let (reached, _) = reached(db, project, access, run, query);
    reached
        .into_iter()
        .find(|reach| {
            reach.grants.contains(&Grant::Public) && !reach.grants.contains(&Grant::System)
        })
        .map(|reach| PublicOnly::Entity {
            entity: reach.entity,
            effect: reach.effect,
        })
}

/// Whether policy may refuse the caller of the action called `run` with
/// 403 `not_permitted` (area 9, "When policy says no"): it, or an action
/// it calls, is listed under `actions:`, or creates, changes or deletes a
/// record. A record a read can't see is its code's own not-found failure
/// instead.
pub fn may_refuse(db: &dyn Db, project: Project, run: &str) -> bool {
    let root = super::root_policy(db, project);
    let mut seen = BTreeSet::from([run.to_string()]);
    let mut queue = vec![run.to_string()];
    while let Some(current) = queue.pop() {
        // An action listed under `actions:`, the route's or one it calls.
        if root.action(&current).is_some() {
            return true;
        }
        for act in acts(db, project, &current) {
            match act {
                Act::Entity(_, Effect::Create | Effect::Update | Effect::Delete) => return true,
                Act::Calls(callee) if seen.insert(callee.clone()) => queue.push(callee.clone()),
                _ => {}
            }
        }
    }
    false
}

/// Whether the action `run`'s own body does `effect` to `entity`.
fn touches_own(db: &dyn Db, project: Project, run: &str, entity: &str, effect: Effect) -> bool {
    acts(db, project, run).contains(&Act::Entity(entity.to_string(), effect))
}

/// `policy.effect_ungranted`: a route running an action whose line under
/// `actions:` lets nobody call it.
fn nobody_passes(
    label: &str,
    span: jadpo_diag::Span,
    run: &str,
    line: &super::ActionGrants,
    access: &Policies<'_>,
) -> Diagnostic {
    let system_only = line.who.iter().all(|who| *who == ActionWho::System);
    let why = if system_only {
        "names only `Access.system`: schedules and subscribers, which no request is".to_string()
    } else {
        let held: Vec<String> = line
            .who
            .iter()
            .zip(&line.written_who)
            .filter_map(|(who, text)| match who {
                ActionWho::Field { entity, .. } => Some(format!(
                    "`{text}`, the caller a `{entity}` the action changes points at"
                )),
                ActionWho::Role { membership, .. } => access
                    .membership(membership)
                    .and_then(|m| m.scope_entity.as_deref())
                    .map(|scope| {
                        format!(
                            "`{text}`, a role held in the `{scope}` a record the action changes belongs to"
                        )
                    }),
                _ => None,
            })
            .collect();
        format!(
            "names {}, and `{run}` changes no such record itself",
            crate::and_list(&held)
        )
    };
    Diagnostic::new(
        &rule::POLICY_EFFECT_UNGRANTED,
        format!("`{label}` runs `{run}`, which nobody may call"),
    )
    .at(span, "nobody passes its line")
    .reason(format!(
        "`{}`'s line under `actions:` {why}, so no caller passes the line as the action starts and this route could never succeed.",
        line.written
    ))
    .next(format!(
        "List `Access.authenticated` or a role held across the whole application for `{}` under `actions:`",
        line.written
    ))
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
