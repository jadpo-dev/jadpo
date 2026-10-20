//! What a batch's run is let in to (area 12, "A batch's tenant"; area 9,
//! "Batches and `Access.public`"; chunk 6.4c).
//!
//! A run works as the system. In an application with a tenant it is
//! limited to the tenant its one saved value holding a tenant's id names
//! (a `from:` argument, a value `each:` or `finished:` captures, or
//! `key:`), as a subscriber is limited to its event's one field holding
//! one ([`crate::events::Tenant`]); with none, or values naming different
//! tenants, it reaches no tenant's records. What its query reads and its
//! actions reach that only `Access.public` grants is
//! `policy.system_grant_missing`, as for a schedule or a subscriber.

use jadpo_db::{Db, File, Project};
use jadpo_diag::{Diagnostic, Span};
use jadpo_graph::body::{Body, Callee, Expr, Node, SiteId};
use jadpo_graph::{Key, Kind, TypeRef};
use jadpo_std::batch::{BatchOp, BatchTenant};

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
/// `finished:` calls reaching, what only `Access.public` grants (area 9),
/// at that query's or action's call.
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
                callee: Callee::Batch(op @ (BatchOp::Each | BatchOp::Read(_))),
                args,
                ..
            }) = &site.node
            else {
                continue;
            };
            // A file helper's read (chunk 6.4b) walks no query: its
            // `each:` and `finished:` follow its file and `as:`.
            let places: &[usize] = match op {
                BatchOp::Read(_) => &[2, 3],
                _ => &[0, 1, 2],
            };
            for &place in places {
                let Some(arg) = args.get(place).copied().flatten() else {
                    continue;
                };
                let query = matches!(op, BatchOp::Each) && place == 0;
                let Some((at, name, query)) = run_call(body, arg, query) else {
                    continue;
                };
                let found = if query {
                    crate::policy::reach::public_only_query(db, project, &name)
                } else {
                    crate::policy::reach::public_only(db, project, &name)
                };
                if let Some(found) = found {
                    let site = body.site(at);
                    let span = Span::new(path, site.start, site.end);
                    out.push(crate::events::public_only("batch", &name, &span, &found));
                }
            }
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
