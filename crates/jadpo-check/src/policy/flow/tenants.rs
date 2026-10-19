//! The tenant background work names (area 9, "Background work"; chunk
//! 6.6c): an event's subscribers run as the system within the tenant its
//! one field of the tenant's `id` type names, and a batch's run within the
//! one its saved values of that type name. A caller could aim either at a
//! tenant they don't belong to by passing its id, so each such value must
//! come from a record of that tenant the action read or wrote under its
//! caller's policy; otherwise `policy.event_tenant_unproved`.
//!
//! What proves a value names a tenant the action reached ([`Walk::names_tenant`]):
//!
//! - the tenancy reference of a record the body read, created or changed,
//!   or the `id` of the tenant itself, each reached through a statement
//!   whose grants all apply only within the caller's tenants;
//! - the same of a record the action is handed whole, which only a body
//!   that read it, with the same caller, or a run limited to its tenant
//!   can hand (a route passes placeholders and input, never a record);
//! - what a create that runs whenever the body does set a tenanted
//!   record's tenancy reference from, since the create needs the caller's
//!   membership in that tenant and the action fails with it.
//!
//! Anything else, a parameter's id among them, isn't proved: what decided
//! the value is no part of it.

use jadpo_diag::{Diagnostic, Span, rule};
use jadpo_graph::body::{BinaryOp, Expr, Node, SiteId, Stmt};
use jadpo_graph::{Key, TypeRef};

use super::{Flow, Prov, Src, Walk, field_name};

/// What background work names its tenant with: an event's field or a
/// batch's saved value.
enum Named<'a> {
    Event { event: &'a str, field: &'a str },
    Run,
}

impl Walk<'_> {
    /// An `emit` of `event` at `value` holding `prov`: its field holding
    /// the tenant's id proved to name a tenant the action reached.
    pub(super) fn event_tenant(
        &mut self,
        flow: &Flow<'_>,
        event: &Key,
        value: SiteId,
        prov: &Prov,
    ) {
        if !self.report {
            return;
        }
        let crate::events::Tenant::Field(field) =
            crate::events::event_tenant(flow.db, flow.project, &flow.lookup, event.name())
        else {
            return;
        };
        let held = prov.field(&field);
        let Some(unproved) = held.whole.iter().find(|src| !self.names_tenant(flow, src)) else {
            return;
        };
        // The field's own value, when the event is built here.
        let at = match &self.body.site(value).node {
            Node::Expr(Expr::Build { fields, .. }) => fields
                .iter()
                .find(|(key, _)| field_name(key) == field)
                .map_or(value, |(_, at)| *at),
            _ => value,
        };
        let short = event
            .name()
            .rsplit('.')
            .next()
            .unwrap_or_default()
            .to_string();
        let problem = self.tenant_unproved(
            flow,
            Named::Event {
                event: &short,
                field: &field,
            },
            at,
            &unproved.clone(),
        );
        self.problems.push(problem);
    }

    /// A value a batch's run saves at `at`, holding `prov`, of the
    /// tenant's `id` type: proved to name a tenant the action reached.
    pub(super) fn run_tenant(&mut self, flow: &Flow<'_>, at: SiteId, prov: &Prov) {
        if !self.report {
            return;
        }
        let Some(unproved) = prov.whole.iter().find(|src| !self.names_tenant(flow, src)) else {
            return;
        };
        let problem = self.tenant_unproved(flow, Named::Run, at, &unproved.clone());
        self.problems.push(problem);
    }

    /// A create at `site` of `entity` setting `values`: what it sets the
    /// tenancy reference from names a tenant the caller holds a
    /// membership in, when it runs whenever the body does.
    pub(super) fn created_tenant(
        &mut self,
        flow: &Flow<'_>,
        site: SiteId,
        entity: &str,
        values: &[(String, SiteId, Prov)],
    ) {
        let Some(via) = tenancy(flow, entity) else {
            return;
        };
        if !self.always_runs(site) {
            return;
        }
        for (name, _, prov) in values {
            if *name != via {
                continue;
            }
            for src in &prov.whole {
                if !matches!(src, Src::Decided(_)) && !self.tenant_ids.contains(src) {
                    self.tenant_ids.push(src.clone());
                }
            }
        }
    }

    /// Whether `src` names a tenant the action reached under its
    /// caller's policy (see the module's documentation).
    fn names_tenant(&self, flow: &Flow<'_>, src: &Src) -> bool {
        if self.tenant_ids.contains(src) {
            return true;
        }
        match src {
            // What decided the value is no part of it.
            Src::Decided(_) => true,
            Src::Field { entity, field, .. } => {
                tenancy(flow, entity).is_some_and(|via| via == *field)
            }
            Src::Record { entity, .. } => tenancy(flow, entity).is_some_and(|via| via == "id"),
            Src::Param(at, path) => {
                let Some(entity) = self.record_param(flow, *at) else {
                    return false;
                };
                match path.as_slice() {
                    [field] => tenancy(flow, &entity).is_some_and(|via| via == *field),
                    [] => tenancy(flow, &entity).is_some_and(|via| via == "id"),
                    _ => false,
                }
            }
            Src::Caller(_) | Src::Deep(_) => false,
        }
    }

    /// The entity of the parameter at `at`, when it is handed a record.
    fn record_param(&self, flow: &Flow<'_>, at: usize) -> Option<String> {
        let param = self.body.params.get(at)?;
        let mut ty = &self.body.local(*param).ty;
        while let TypeRef::Nullable(inner) = ty {
            ty = inner;
        }
        let TypeRef::Node(key) = ty else {
            return None;
        };
        flow.entities
            .get(key.name())
            .is_some()
            .then(|| key.name().to_string())
    }

    /// Whether the site at `site` runs whenever the body does: it is
    /// inside no loop, `if`, `match`, inline function or `and`/`or`, and no
    /// `return` before it may leave first.
    fn always_runs(&self, site: SiteId) -> bool {
        if self.looping > 0 {
            return false;
        }
        let at = self.body.site(site);
        let mut top = None;
        for stmt in &self.body.block {
            let s = self.body.site(*stmt);
            if s.start <= at.start && at.end <= s.end {
                top = Some(*stmt);
                break;
            }
            if self.returns_in(*stmt) {
                return false;
            }
        }
        if top.is_none() {
            return false;
        }
        !self.body.sites.iter().any(|inner| {
            inner.start <= at.start
                && at.end <= inner.end
                && matches!(
                    inner.node,
                    Node::Stmt(Stmt::If { .. } | Stmt::For { .. })
                        | Node::Expr(
                            Expr::Match { .. }
                                | Expr::Inline { .. }
                                | Expr::Binary {
                                    op: BinaryOp::And | BinaryOp::Or,
                                    ..
                                }
                        )
                )
        })
    }

    /// Whether a `return` is inside the statement at `site`.
    fn returns_in(&self, site: SiteId) -> bool {
        let at = self.body.site(site);
        self.body.sites.iter().any(|inner| {
            inner.start >= at.start
                && inner.end <= at.end
                && matches!(inner.node, Node::Stmt(Stmt::Return { .. }))
        })
    }

    /// `policy.event_tenant_unproved` at `at`, whose value `src` doesn't
    /// name a tenant the action reached.
    fn tenant_unproved(
        &self,
        flow: &Flow<'_>,
        named: Named<'_>,
        at: SiteId,
        src: &Src,
    ) -> Diagnostic {
        let tenant = flow
            .prover
            .policies
            .root()
            .tenant
            .clone()
            .unwrap_or_default();
        let name = &self.name;
        let label = match src {
            Src::Param(at, _) => {
                let param = self
                    .body
                    .params
                    .get(*at)
                    .map(|param| self.body.local(*param).name.clone())
                    .unwrap_or_default();
                format!("`{param}`, whatever the caller passes")
            }
            Src::Caller(_) => "the caller's own".to_string(),
            Src::Field { entity, field, .. } => {
                format!("`{entity}.{field}`, not a `{entity}`'s tenant")
            }
            _ => format!("not read from a record of the `{tenant}`"),
        };
        let which = match named {
            Named::Event { .. } => "that id",
            Named::Run => "each",
        };
        let (headline, how) = match named {
            Named::Event { event, field } => (
                format!("`{name}` sends `{event}` to a `{tenant}` it never reached"),
                format!(
                    "`{event}`'s subscribers run as the system within the `{tenant}` its `{field}` names"
                ),
            ),
            Named::Run => (
                format!("`{name}` starts a batch in a `{tenant}` it never reached"),
                format!(
                    "A batch's run works as the system within the `{tenant}` its saved values of type `{tenant}.id` name"
                ),
            ),
        };
        let next = match src {
            Src::Param(at, path) if path.is_empty() => {
                let param = self
                    .body
                    .params
                    .get(*at)
                    .map(|param| self.body.local(*param).name.clone())
                    .unwrap_or_default();
                format!(
                    "Name the `{tenant}` of a record `{name}` reads or writes, or read it first with `attempt {tenant}.get({param})`"
                )
            }
            _ => format!("Name the `{tenant}` of a record `{name}` reads or writes"),
        };
        let span: Span = self.span(at);
        Diagnostic::new(&rule::POLICY_EVENT_TENANT_UNPROVED, headline)
            .at(span, label)
            .reason(format!(
                "{how}, so {which} must come from a `{tenant}`, or a record of one, that `{name}` read or wrote under its caller's policy; one the caller passes could name a `{tenant}` they don't belong to."
            ))
            .next(next)
    }
}

/// The field of a record of `entity` holding its tenant's id: its
/// tenancy reference, or `id` on the tenant itself; `None` for an entity
/// every tenant shares, or with no tenant.
fn tenancy(flow: &Flow<'_>, entity: &str) -> Option<String> {
    flow.prover
        .policies
        .entity(entity)
        .and_then(|policy| policy.tenancy.as_ref())
        .map(|tenancy| tenancy.via.clone())
}
