//! Policy compiled into the plan's statements (area 9, "When policy says
//! no"; area 6: policy is part of each query's `WHERE`): every read,
//! update and delete reaches only the records its grants let the caller
//! in to, and a create writes its row only when the new record's grants
//! hold, in the same statement, so a membership removed a moment earlier
//! can't slip through. The runtime binds who is calling
//! ([`jadpo_std::policy::CallerPart`]).
//!
//! The grants are the entity's own, or for a write an action listed under
//! root `policy { actions: }` makes in its own body, that line's. Tenancy
//! limits every grant to the tenants where the caller has a membership: a
//! tenant's own record being created belongs to nobody yet, a new
//! membership in a tenant nobody belongs to yet makes its member the
//! first, and a record moved to another tenant needs the caller's
//! membership there too.

use jadpo_check::data::EntityModel;
use jadpo_check::policy::{Effect, Grant, LineGate};
use jadpo_sql::policy::{Membership, Who, any_of, qualified};
use jadpo_std::http::PrincipalKind;
use jadpo_std::policy::{CallerPart, Gate};
use jadpo_std::read::Source;

use crate::queries::{Planner, Statement, field};

/// A statement that can bind who is calling, as well as a condition's
/// values.
pub(crate) trait CallerBind: jadpo_sql::Bind {
    /// `$n` for part `part` of the caller.
    fn caller(&mut self, part: CallerPart) -> String;
}

impl CallerBind for Statement {
    fn caller(&mut self, part: CallerPart) -> String {
        self.param(Source::Caller(part), part.ty())
    }
}

/// Where the record a grant is tested on is.
pub(crate) enum Row<'r> {
    /// The row of the table the statement reaches.
    Table,
    /// A record being created: each field's value as SQL, by field name.
    New(&'r [(String, String)]),
}

/// The part of the caller a grant compares a field with.
fn part(kind: PrincipalKind) -> CallerPart {
    match kind {
        PrincipalKind::User => CallerPart::User,
        PrincipalKind::Service => CallerPart::Service,
    }
}

impl Planner<'_> {
    /// The grants deciding `effect` on `entity`'s records for the
    /// statement being planned: the entity's own, or for a write in the
    /// body of an action listed under `actions:`, that line's.
    pub(crate) fn grants(&self, entity: &EntityModel, effect: Effect) -> Vec<Grant> {
        if matches!(effect, Effect::Create | Effect::Update | Effect::Delete)
            && let Some(line) = self
                .action
                .as_deref()
                .and_then(|action| self.access.root.action(action))
        {
            return line.on(entity, &self.access.memberships);
        }
        self.access
            .entity(&entity.name)
            .map(|policy| policy.grants(effect).to_vec())
            .unwrap_or_default()
    }

    /// The record's field `name` as SQL: its column, qualified, or the new
    /// record's value.
    fn value_of(&self, entity: &EntityModel, name: &str, row: &Row<'_>) -> Option<String> {
        match row {
            Row::Table => Some(qualified(&entity.table, &field(entity, name)?.born)),
            Row::New(values) => values
                .iter()
                .find(|(field, _)| field == name)
                .map(|(_, value)| value.clone()),
        }
    }

    /// The caller's current membership in `membership`, as an `EXISTS`
    /// under `alias`.
    fn held(
        &self,
        bind: &mut dyn CallerBind,
        membership: &jadpo_check::policy::Membership,
        alias: &str,
        scope: Option<String>,
        roles: Option<Vec<String>>,
    ) -> Option<Membership> {
        let model = self.model.entity(&membership.entity)?;
        let visible = match model.lifecycle.as_ref().and_then(|l| l.visible.as_ref()) {
            Some(visible) => Some(jadpo_sql::condition(bind, model, visible, false)?),
            None => None,
        };
        let scope = match (&membership.scope, scope) {
            (Some(column), Some(value)) => Some((field(model, column)?.born.clone(), value)),
            _ => None,
        };
        let roles = match roles {
            Some(roles) => Some((field(model, &membership.role)?.born.clone(), roles)),
            None => None,
        };
        Some(Membership {
            table: model.table.clone(),
            alias: alias.to_string(),
            member: field(model, &membership.member)?.born.clone(),
            caller: bind.caller(CallerPart::User),
            scope,
            roles,
            visible,
        })
    }

    /// The condition `grants` add for a statement whose record is `row`:
    /// any of them, and the tenancy.
    pub(crate) fn granted(
        &self,
        bind: &mut dyn CallerBind,
        entity: &EntityModel,
        grants: &[Grant],
        row: &Row<'_>,
    ) -> Option<String> {
        let condition = self.any_grant(bind, entity, grants, row)?;
        self.in_tenant(bind, entity, condition, row)
    }

    /// Whether the caller passes any of `grants` on `row`, without the
    /// tenancy: what a field's own `policy:` adds beside its record's.
    pub(crate) fn any_grant(
        &self,
        bind: &mut dyn CallerBind,
        entity: &EntityModel,
        grants: &[Grant],
        row: &Row<'_>,
    ) -> Option<String> {
        let mut whos: Vec<Who> = Vec::new();
        // A membership's roles, gathered by the membership and the
        // reference they apply through, so each is one `EXISTS`.
        let mut roles: Vec<(String, Option<String>, Vec<String>)> = Vec::new();
        // Anyone at all: nothing else in the list adds a thing.
        let public = grants.contains(&Grant::Public);
        if public {
            whos.push(Who::Anyone);
        }
        for grant in grants.iter().filter(|_| !public) {
            match grant {
                Grant::Caller { field: name, kind } => {
                    let value = self.value_of(entity, name, row)?;
                    let caller = bind.caller(part(*kind));
                    whos.push(Who::Equals { value, caller });
                }
                Grant::Role {
                    membership,
                    role,
                    via,
                } => match roles
                    .iter_mut()
                    .find(|(m, v, _)| m == membership && v == via)
                {
                    Some((_, _, listed)) => listed.push(role.clone()),
                    None => roles.push((membership.clone(), via.clone(), vec![role.clone()])),
                },
                Grant::Signed => whos.push(Who::Condition(bind.caller(CallerPart::Signed))),
                Grant::Public => whos.push(Who::Anyone),
                // No request is a schedule or a subscriber yet (area 12).
                Grant::System => {}
            }
        }
        for (at, (membership, via, listed)) in roles.into_iter().enumerate() {
            let held = self.access.membership(&membership)?;
            let scope = match &via {
                Some(via) => Some(self.value_of(entity, via, row)?),
                None => None,
            };
            let alias = format!("jadpo_m{at}");
            whos.push(Who::Member(self.held(
                bind,
                held,
                &alias,
                scope,
                Some(listed),
            )?));
        }
        Some(any_of(&whos))
    }

    /// `condition`, limited to the tenants where the caller has a
    /// membership when `entity`'s records belong to one.
    fn in_tenant(
        &self,
        bind: &mut dyn CallerBind,
        entity: &EntityModel,
        mut condition: String,
        row: &Row<'_>,
    ) -> Option<String> {
        let anyone = condition == jadpo_sql::policy::ANYONE;
        if let Some(tenancy) = self
            .access
            .entity(&entity.name)
            .and_then(|policy| policy.tenancy.as_ref())
        {
            let creating_tenant = matches!(row, Row::New(_)) && tenancy.via == "id";
            if !creating_tenant {
                let at = self.value_of(entity, &tenancy.via, row)?;
                let mut tenants: Vec<Who> = Vec::new();
                for (i, name) in self.access.root.tenant_memberships.iter().enumerate() {
                    let held = self.access.membership(name)?;
                    let alias = format!("jadpo_t{i}");
                    tenants.push(Who::Member(self.held(
                        bind,
                        held,
                        &alias,
                        Some(at.clone()),
                        None,
                    )?));
                }
                // A new membership in a tenant nobody belongs to yet makes
                // its member the first (founding it); in any other tenant,
                // only someone who belongs to it already may add one.
                if let Row::New(_) = row
                    && let Some(held) = self.access.membership(&entity.name)
                    && held.scope.as_deref() == Some(tenancy.via.as_str())
                {
                    let member = self.value_of(entity, &held.member, row)?;
                    let caller = bind.caller(CallerPart::User);
                    let mut founding = vec![format!("{member} = {caller}")];
                    founding.extend(self.nobody_in(&at)?);
                    tenants.push(Who::Condition(format!("({})", founding.join(" AND "))));
                }
                let tenants = any_of(&tenants);
                condition = if anyone {
                    tenants
                } else {
                    format!("{condition} AND {tenants}")
                };
            }
        }
        Some(condition)
    }

    /// That no membership of any kind is held in the tenant `at` (SQL),
    /// hidden ones included, since a tenant that has had members isn't
    /// being founded: one `NOT EXISTS` per membership held in the tenant.
    fn nobody_in(&self, at: &str) -> Option<Vec<String>> {
        let mut out = Vec::new();
        for (i, name) in self.access.root.tenant_memberships.iter().enumerate() {
            let held = self.access.membership(name)?;
            let model = self.model.entity(&held.entity)?;
            let scope = &field(model, held.scope.as_deref()?)?.born;
            out.push(jadpo_sql::policy::unheld(
                &model.table,
                &format!("jadpo_f{i}"),
                scope,
                at,
            ));
        }
        Some(out)
    }

    /// The condition a write moving a record of `entity` to another tenant
    /// adds (area 9, "Tenancy"): the caller holds a current membership in
    /// the tenant `to` (SQL), its new tenancy, as a create there would.
    /// `None` for an entity that belongs to no tenant, or is the tenant.
    pub(crate) fn moved_to(
        &self,
        bind: &mut dyn CallerBind,
        entity: &EntityModel,
        to: &str,
    ) -> Option<Option<String>> {
        let moves = self
            .access
            .entity(&entity.name)
            .and_then(|policy| policy.tenancy.as_ref())
            .is_some_and(|tenancy| tenancy.via != "id");
        if !moves {
            return Some(None);
        }
        let mut tenants: Vec<Who> = Vec::new();
        for (i, name) in self.access.root.tenant_memberships.iter().enumerate() {
            let held = self.access.membership(name)?;
            let alias = format!("jadpo_n{i}");
            tenants.push(Who::Member(self.held(
                bind,
                held,
                &alias,
                Some(to.to_string()),
                None,
            )?));
        }
        Some(Some(any_of(&tenants)))
    }

    /// What keeps the references a write sets inside the tenant (area 9,
    /// "References stay inside the tenant"), checked with or without
    /// policy, since it is what the data is rather than who asks: for each
    /// reference in `changed` from one of the tenant's records to another,
    /// that the record it points at belongs to the new record's tenant.
    /// `new` gives a field's value once written, as SQL. A write that
    /// moves a record to another tenant (only an action listed under
    /// `actions:` may) checks every reference it keeps, and that no
    /// record of the old tenant refers to it.
    pub(crate) fn kept_in_tenant(
        &self,
        entity: &EntityModel,
        changed: &[String],
        new: &dyn Fn(&str) -> Option<String>,
        creating: bool,
    ) -> Option<Vec<String>> {
        let Some(own) = self
            .access
            .entity(&entity.name)
            .and_then(|policy| policy.tenancy.as_ref())
        else {
            return Some(Vec::new());
        };
        let moved = !creating && own.via != "id" && changed.contains(&own.via);
        let mut out = Vec::new();
        let mut tenancy = None;
        for at in &entity.fields {
            let Some(reference) = &at.reference else {
                continue;
            };
            if at.name == own.via || !(moved || changed.contains(&at.name)) {
                continue;
            }
            let Some((_, theirs)) = self.access.kept_in_tenant(&entity.name, &reference.target)
            else {
                continue;
            };
            let target = self.model.entity(&reference.target)?;
            if tenancy.is_none() {
                tenancy = Some(new(&own.via)?);
            }
            out.push(jadpo_sql::policy::outside_tenant(
                &target.table,
                &format!("jadpo_r{}", out.len()),
                &field(target, "id")?.born,
                &new(&at.name)?,
                &field(target, &theirs.via)?.born,
                tenancy.as_deref()?,
            ));
        }
        if moved {
            let tenancy = new(&own.via)?;
            let id = qualified(&entity.table, &field(entity, "id")?.born);
            for child in &self.model.entities {
                let Some((theirs, _)) = self.access.kept_in_tenant(&child.name, &entity.name)
                else {
                    continue;
                };
                for at in &child.fields {
                    if at.reference.as_ref().map(|r| r.target.as_str()) != Some(&entity.name) {
                        continue;
                    }
                    out.push(jadpo_sql::policy::outside_tenant(
                        &child.table,
                        &format!("jadpo_r{}", out.len()),
                        &at.born,
                        &id,
                        &field(child, &theirs.via)?.born,
                        &tenancy,
                    ));
                }
            }
        }
        Some(out)
    }

    /// The condition a field's own `policy:` adds to a write changing it
    /// (area 9, "Field restrictions"), when it narrows the record's grants
    /// for `effect`: who may change the field, over `row`.
    pub(crate) fn field_policy(
        &self,
        bind: &mut dyn CallerBind,
        entity: &EntityModel,
        field: &str,
        effect: Effect,
        row: &Row<'_>,
    ) -> Option<Option<String>> {
        let Some(grants) = self
            .access
            .entity(&entity.name)
            .and_then(|policy| policy.field_grants(field, effect))
        else {
            return Some(None);
        };
        Some(Some(self.any_grant(bind, entity, grants, row)?))
    }

    /// The condition policy adds to a statement doing `effect` to
    /// `entity`'s records, whose record is `row`.
    pub(crate) fn policy(
        &self,
        bind: &mut dyn CallerBind,
        entity: &EntityModel,
        effect: Effect,
        row: &Row<'_>,
    ) -> Option<String> {
        let grants = self.grants(entity, effect);
        self.granted(bind, entity, &grants, row)
    }

    /// What the plan checks as an action starts, from who its line under
    /// `actions:` lets call it (area 9, "Actions"): nothing for anyone at
    /// all or when the statements its own body writes decide, anyone
    /// signed in, a role held across the whole application, or nobody.
    /// A gate it can't plan refuses everyone rather than no one.
    pub(crate) fn gate(&self, gate: &LineGate) -> Option<Gate> {
        match gate {
            LineGate::Anyone | LineGate::Statements => None,
            LineGate::Signed => Some(Gate::Signed),
            LineGate::Nobody => Some(Gate::Nobody),
            LineGate::Roles(roles) => Some(self.roles_gate(roles).unwrap_or(Gate::Nobody)),
        }
    }

    /// A caller holding one of `roles`, each held across the whole
    /// application, as a `SELECT 1` of their membership.
    fn roles_gate(&self, roles: &[(String, String)]) -> Option<Gate> {
        let mut statement = Statement::default();
        let mut whos = Vec::new();
        let mut store = None;
        for (at, (membership, role)) in roles.iter().enumerate() {
            let held = self.access.membership(membership)?;
            store = Some(self.model.entity(&held.entity)?.store.clone()?);
            let alias = format!("jadpo_m{at}");
            whos.push(Who::Member(self.held(
                &mut statement,
                held,
                &alias,
                None,
                Some(vec![role.clone()]),
            )?));
        }
        Some(Gate::Roles {
            store: store?,
            sql: format!("SELECT 1 WHERE {}", any_of(&whos)),
            params: statement.into_params(),
        })
    }

    /// The condition limiting a credential statement to holders whose
    /// record `effect` lets the caller in to (area 9, question 8): its
    /// holder (`"principal_id"`, or a parameter) among them.
    pub(crate) fn holders(
        &self,
        bind: &mut dyn CallerBind,
        entity: &EntityModel,
        effect: Effect,
        holder: &str,
    ) -> Option<String> {
        let mut conditions = self.access(bind, entity, crate::queries::Reach::Read)?;
        conditions.push(self.policy(bind, entity, effect, &Row::Table)?);
        Some(jadpo_sql::policy::held_by(
            holder,
            &entity.table,
            &conditions,
        ))
    }

    /// A credential statement limited by its holder's grants: `sql`, whose
    /// own parameters are the first `own`, with `AND <holders>` where it
    /// says `{held}`.
    pub(crate) fn gated(
        &self,
        entity: &EntityModel,
        effect: Effect,
        own: usize,
        holder: &str,
        sql: &str,
    ) -> Option<jadpo_std::policy::Gated> {
        let mut bind = After {
            own,
            params: Vec::new(),
        };
        let held = self.holders(&mut bind, entity, effect, holder)?;
        Some(jadpo_std::policy::Gated {
            sql: sql.replace("{held}", &held),
            extra: bind.params,
        })
    }
}

/// Binds a credential statement's parameters after its own `own`.
struct After {
    own: usize,
    params: Vec<jadpo_std::read::Param>,
}

impl After {
    fn param(&mut self, source: Source, ty: jadpo_std::read::SqlType) -> String {
        let param = jadpo_std::read::Param { source, ty };
        let at = match self.params.iter().position(|p| *p == param) {
            Some(at) => at,
            None => {
                self.params.push(param);
                self.params.len() - 1
            }
        };
        jadpo_sql::placeholder(self.own + at)
    }
}

impl jadpo_sql::Bind for After {
    fn bind(&mut self, value: &jadpo_sql::Value, ty: jadpo_std::read::SqlType) -> Option<String> {
        matches!(value, jadpo_sql::Value::Now).then(|| self.param(Source::Now, ty))
    }
}

impl CallerBind for After {
    fn caller(&mut self, part: CallerPart) -> String {
        self.param(Source::Caller(part), part.ty())
    }
}

/// Who may call each action listed under root `policy { actions: }` as it
/// starts, by action name, where its line says without a record.
pub(crate) fn gates(
    db: &dyn jadpo_db::Db,
    project: jadpo_db::Project,
    types: &mut crate::types::Types,
) -> std::collections::HashMap<String, Gate> {
    let model = jadpo_check::data::data_model(db, project);
    let lookup = jadpo_check::lookups::Lookup::new(db, project);
    let access = jadpo_check::policy::access(db, project);
    let planner = Planner {
        model,
        lookup: &lookup,
        types,
        access,
        action: None,
    };
    access
        .root
        .actions
        .iter()
        .filter_map(|line| {
            let gate = jadpo_check::policy::reach::line_gate(db, project, &line.action)
                .unwrap_or(LineGate::Nobody);
            Some((line.action.clone(), planner.gate(&gate)?))
        })
        .collect()
}
