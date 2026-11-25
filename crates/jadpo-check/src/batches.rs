//! What a batch's run is let in to (area 12, "A batch's tenant"; area 9,
//! "Batches and `Access.public`"; chunk 6.4c).
//!
//! A run works as the system. In an application with a tenant it is
//! limited to the tenant its one saved value holding a tenant's id names
//! (a `from:` argument, a value `each:` or `finished:` captures, or
//! `key:`), as a subscriber is limited to its event's one field holding
//! one ([`crate::events::Tenant`]); with none, or values naming different
//! tenants, it reaches no tenant's records. What its query reads and its
//! actions reach whose list doesn't name `Access.system` is
//! `policy.system_grant_missing`, as for a schedule or a subscriber.

use jadpo_db::{Db, File, Project};
use jadpo_diag::{Diagnostic, Span};
use jadpo_graph::body::{Body, Callee, Expr, Node, SiteId};
use jadpo_graph::{Key, Kind, TypeRef};
use jadpo_std::batch::BatchTenant;

use crate::types::{Ty, TypeView, callables_of, typed_body};

/// The type a value holding the tenant's id has, `Household.id`, by its
/// field's key: `None` when the application has no tenant.
pub fn tenant_id(db: &dyn Db, project: Project) -> Option<Key> {
    let tenant = crate::policy::root_policy(db, project).tenant.clone()?;
    Some(crate::graph::entity_field_key(
        crate::graph::registry(db, project),
        &tenant,
        &["id".to_string()],
    ))
}

/// Whether a value of type `ty` holds the tenant's id, `none` or not:
/// one of the tenant's `id` type, or of a field declared with it, as a
/// reference to the tenant is (`RentRound.society_id`, chunk 6.4d).
pub(crate) fn holds_tenant_id(types: &dyn TypeView, ty: &TypeRef, tenant_id: &Key) -> bool {
    match ty {
        TypeRef::Nullable(inner) => holds_tenant_id(types, inner, tenant_id),
        TypeRef::Field(key) if key == tenant_id => true,
        TypeRef::Field(key) => {
            let Some(tenant) = tenant_id.name().strip_suffix(".id").map(str::to_string) else {
                return false;
            };
            key.name().rsplit_once('.').is_some_and(|(host, field)| {
                types
                    .field(host, &[field.to_string()])
                    .is_some_and(|field| declared_tenant_id(types, &field.declared, &tenant, 0))
            })
        }
        _ => false,
    }
}

/// Whether a field declared `ty` holds the id of the entity `tenant`.
fn declared_tenant_id(types: &dyn TypeView, ty: &Ty, tenant: &str, depth: usize) -> bool {
    match ty {
        Ty::Nullable(inner) => declared_tenant_id(types, inner, tenant, depth),
        Ty::Field(host, path) if host == tenant && path.as_slice() == ["id"] => true,
        // A field declared as another field's type, a few deep at most.
        Ty::Field(host, path) if depth < 8 => types
            .field(host, path)
            .is_some_and(|field| declared_tenant_id(types, &field.declared, tenant, depth + 1)),
        _ => false,
    }
}

/// Where a run's tenant comes from, given the types of the values it
/// saves, in order, and of its `key:`.
pub fn run_tenant(
    db: &dyn Db,
    project: Project,
    saved: &[TypeRef],
    key: Option<&TypeRef>,
) -> BatchTenant {
    let Some(tenant_id) = tenant_id(db, project) else {
        return BatchTenant::Untenanted;
    };
    let types = crate::lookups::Lookup::new(db, project);
    BatchTenant::Saved {
        captured: saved
            .iter()
            .enumerate()
            .filter(|(_, ty)| holds_tenant_id(&types, ty, &tenant_id))
            .map(|(at, _)| at as u32)
            .collect(),
        key: key.is_some_and(|ty| holds_tenant_id(&types, ty, &tenant_id)),
    }
}

/// `policy.system_grant_missing` for each batch an action in `file`
/// starts: its `from:` query reading, or the action `each:` or
/// `finished:` calls reaching, what a list doesn't let the system in to
/// (area 9), at that query's or action's call.
#[salsa::tracked(returns(ref))]
pub fn batch_grant_problems(db: &dyn Db, project: Project, file: File) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    let callables = callables_of(db, file);
    if callables.is_empty() || !crate::policy::policy_clean(db, project) {
        return out;
    }
    let path = file.path(db);
    for callable in callables {
        let checked = typed_body(db, project, callable);
        if checked.key.as_ref().and_then(Key::kind) != Some(Kind::Action) {
            continue;
        }
        let body = &checked.body;
        for site in &body.sites {
            let Node::Expr(Expr::Call {
                callee: Callee::Batch(op),
                args,
                ..
            }) = &site.node
            else {
                continue;
            };
            if !op.starts() {
                continue;
            }
            // A file helper's read (chunk 6.4b) walks no query: its
            // `each:` and `finished:` follow its file and `as:`; a write's
            // (chunk 6.4b2) follow its store, `name:` and `format:`.
            let (from, each, finished) = crate::types::run_places(*op);
            for place in from.into_iter().chain([each, finished]) {
                let Some(arg) = args.get(place).copied().flatten() else {
                    continue;
                };
                let query = Some(place) == from;
                let Some((at, name, query)) = run_call(body, arg, query) else {
                    continue;
                };
                let found = if query {
                    crate::policy::reach::system_refused_query(db, project, &name)
                } else {
                    crate::policy::reach::system_refused(db, project, &name)
                };
                if let Some(found) = found {
                    let site = body.site(at);
                    let span = Span::new(path, site.start, site.end);
                    out.push(crate::events::system_refused("batch", &name, &span, &found));
                }
            }
            if let Some(problem) = tenant_missing(db, project, path, checked, args, *op) {
                out.push(problem);
            }
        }
    }
    out
}

/// `policy.batch_tenant_missing` for the batch started with `args`, in an
/// application with a tenant (area 12, "Which tenant a run reaches"; chunk
/// 10.7d): it saves no value of the tenant's `id` type, so its run reaches
/// no tenant's records, and its `from:` query or the action `each:` or
/// `finished:` calls reaches one. A run that reached nothing would finish
/// as if there were nothing to do, so the check refuses it, at the first
/// call reaching a tenant's records.
fn tenant_missing(
    db: &dyn Db,
    project: Project,
    path: &str,
    checked: &crate::types::Checked,
    args: &[Option<SiteId>],
    op: jadpo_std::batch::BatchOp,
) -> Option<Diagnostic> {
    let tenant_id = tenant_id(db, project)?;
    let tenant = tenant_id.name().strip_suffix(".id")?.to_string();
    let body = &checked.body;
    let types = crate::lookups::Lookup::new(db, project);
    let (from, each, finished) = crate::types::run_places(op);
    let arg = |at: usize| args.get(at).copied().flatten();
    // What the run saves, as the plan saves it: `from:`'s arguments, a
    // file helper's file or a writer's `name:` and `format:`, what
    // `each:` and `finished:` capture, and `key:`.
    let mut saved: Vec<SiteId> = Vec::new();
    if let Some(from) = from.and_then(arg)
        && let Node::Expr(Expr::Call { args: given, .. }) = &body.site(from).node
    {
        saved.extend(given.iter().flatten());
    }
    match op {
        jadpo_std::batch::BatchOp::Read(_) => saved.extend(arg(0)),
        jadpo_std::batch::BatchOp::Write(_) => saved.extend([arg(1), arg(2)].into_iter().flatten()),
        _ => {}
    }
    for place in [each, finished] {
        let Some(inline) = arg(place) else {
            continue;
        };
        // A call's arguments other than the item or summary and what is
        // inside it; a writer's row, what it reads of the action.
        match &body.site(inline).node {
            Node::Expr(Expr::Inline {
                params,
                body: Some(call),
            }) if let Node::Expr(Expr::Call { args: given, .. }) = &body.site(*call).node => {
                let param = params.first().copied();
                saved.extend(
                    given
                        .iter()
                        .flatten()
                        .filter(|at| param.is_none_or(|param| !of_param(body, **at, param))),
                );
            }
            _ => saved.extend(body.outer_reads(inline).into_iter().map(|(_, at)| at)),
        }
    }
    saved.extend(arg(finished + 1));
    if saved
        .iter()
        .any(|at| holds_tenant_id(&types, &body.site(*at).ty, &tenant_id))
    {
        return None;
    }
    // The first call of the run's that reaches a tenant's records.
    let (at, entity, effect) = from
        .into_iter()
        .map(|at| (at, true))
        .chain([(each, false), (finished, false)])
        .find_map(|(place, query)| {
            let (at, name, query) = run_call(body, arg(place)?, query)?;
            let (entity, effect) = crate::policy::reach::tenant_reached(db, project, &name, query)?;
            Some((at, entity, effect))
        })?;
    let action = checked
        .key
        .as_ref()
        .map(|key| {
            key.name()
                .rsplit('.')
                .next()
                .unwrap_or_default()
                .to_string()
        })
        .unwrap_or_default();
    let site = body.site(at);
    let span = Span::new(path, site.start, site.end);
    let record = snake(&tenant);
    Some(
        Diagnostic::new(
            &jadpo_diag::rule::POLICY_BATCH_TENANT_MISSING,
            format!("`{action}` starts a batch that names no `{tenant}`"),
        )
        .at(span, crate::policy::reach::doing(effect, &entity))
        .reason(format!(
            "A batch's run works as the system within the `{tenant}` its saved values of type `{tenant}.id` name, and `{action}` saves none, so its run would reach no `{tenant}`'s records: it would find no `{entity}` and finish as if there were nothing to do."
        ))
        .next(format!(
            "Save the `{tenant}` the run is for with `key:`, read from a record `{action}` reaches, such as `key: {record}.id`; for every `{tenant}` at once, do the work in a schedule's own action, which reaches them all, with a `limit: every` query and a loop of writes"
        )),
    )
}

/// Whether the value at `site` is `param`, or a field inside it.
fn of_param(body: &Body, site: SiteId, param: jadpo_graph::body::LocalId) -> bool {
    match &body.site(site).node {
        Node::Expr(Expr::Local(local)) => *local == param,
        Node::Expr(Expr::Field { receiver, .. }) => of_param(body, *receiver, param),
        _ => false,
    }
}

/// `Household` as a value's name: `household`.
fn snake(name: &str) -> String {
    let mut out = String::new();
    for (at, c) in name.chars().enumerate() {
        if c.is_uppercase() {
            if at > 0 {
                out.push('_');
            }
            out.extend(c.to_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// The call a batch's run makes at argument `arg`: `from:`'s query call
/// (`query`), or the action an inline function calls: its site, its name
/// and whether it is the query.
fn run_call(body: &Body, arg: SiteId, query: bool) -> Option<(SiteId, String, bool)> {
    let call = if query {
        arg
    } else {
        match &body.site(arg).node {
            Node::Expr(Expr::Inline {
                body: Some(inner), ..
            }) => *inner,
            _ => return None,
        }
    };
    match &body.site(call).node {
        Node::Expr(Expr::Call {
            callee: Callee::Query(key),
            ..
        }) if query => Some((call, key.name().to_string(), true)),
        Node::Expr(Expr::Call {
            callee: Callee::Action(key),
            ..
        }) if !query => Some((call, key.name().to_string(), false)),
        _ => None,
    }
}
