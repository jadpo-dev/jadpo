//! The permissions report (area 9, "The permissions report"): who may do
//! what to every entity's records, the one global view of the policy
//! written beside each entity, which `jadpo check` writes to
//! `build/report/permissions.md` with a `.json` twin.
//!
//! It is read to answer "who may delete a project?" and, once the approval
//! review comes (area 14), to see what a change widened, so it is built
//! for small diffs: no times or totals, entities in a fixed order, one
//! entity's grants on one table row, and each grant exactly as written.

use jadpo_db::{Db, ItemKind, Ownership, Project};
use jadpo_graph::json::Value;

use super::{Access, ActionGrants, Effect, EntityPolicy, Membership};
use crate::lookups::{Lookup, NameView};

/// The report as Markdown and as JSON.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Report {
    pub markdown: String,
    pub json: Value,
}

/// `domain.Entity`, or the bare name outside a domain.
fn qualified(lookup: &Lookup<'_>, name: &str) -> String {
    match lookup
        .declared(name)
        .iter()
        .find(|s| s.kind == ItemKind::Entity)
        .map(|s| &s.ownership)
    {
        Some(Ownership::Domain(domain)) => format!("{domain}.{name}"),
        _ => name.to_string(),
    }
}

/// One cell: the list as written, or "nobody".
fn cell(policy: &EntityPolicy, effect: Effect) -> String {
    match policy.lists.get(&effect) {
        Some(list) if !list.written.is_empty() => list
            .written
            .iter()
            .map(|who| format!("`{who}`"))
            .collect::<Vec<_>>()
            .join(", "),
        _ => "nobody".to_string(),
    }
}

fn and_list(items: &[String]) -> String {
    crate::and_list(items)
}

/// The line under an entity's table: where its records belong, the role
/// it holds when it is a membership, and the fields only listed actions
/// change.
fn remarks(access: &Access, policy: &EntityPolicy, membership: Option<&Membership>) -> String {
    let mut out: Vec<String> = Vec::new();
    match (&access.root.tenant, &policy.tenancy) {
        (Some(tenant), Some(tenancy)) if tenancy.via == "id" => out.push(format!(
            "The tenant: a caller reaches only the `{tenant}` records they hold a membership in."
        )),
        (Some(tenant), Some(tenancy)) => out.push(format!(
            "Belongs to a `{tenant}` through `{}`.",
            tenancy.via
        )),
        (Some(tenant), None) => out.push(format!("Shared by every `{tenant}`.")),
        (None, _) => {}
    }
    if let Some(membership) = membership {
        out.push(match (&membership.scope, &membership.scope_entity) {
            (Some(scope), Some(entity)) => format!(
                "A membership: `{}` holds a `{}` (`{}`) in the `{entity}` `{scope}` names.",
                membership.member, membership.roles, membership.role
            ),
            _ => format!(
                "A membership: `{}` holds a `{}` (`{}`) across the whole application.",
                membership.member, membership.roles, membership.role
            ),
        });
    }
    if !policy.fixed.is_empty() {
        let fields: Vec<String> = policy.fixed.iter().map(|f| format!("`{f}`")).collect();
        out.push(format!(
            "Only an action listed under `actions:` changes {}.",
            and_list(&fields)
        ));
    }
    out.join(" ")
}

/// The project's permissions report.
pub fn report(db: &dyn Db, project: Project) -> Report {
    let access = super::access(db, project);
    let lookup = Lookup::new(db, project);
    let mut markdown = String::from(
        "# Permissions\n\n\
         Who may do what to each entity's records, from the `policy:` beside each entity \
         and root `policy.jadpo`. Written by `jadpo check`.\n\n\
         A list means any of the callers it names, and nobody else. `Access.authenticated` \
         is anyone signed in, `Access.public` anyone at all, and `Access.system` schedules \
         and subscribers.\n\n",
    );
    if let Some(tenant) = &access.root.tenant {
        markdown.push_str(&format!(
            "The tenant is `{tenant}`: every grant on a record that belongs to one applies \
             only within the `{tenant}`s where the caller holds a membership.\n\n"
        ));
    }
    // Entities in name order of their qualified names, so a domain's sit
    // together.
    let mut entities: Vec<(String, &EntityPolicy)> = access
        .entities
        .values()
        .map(|policy| (qualified(&lookup, &policy.entity), policy))
        .collect();
    entities.sort_by(|a, b| a.0.cmp(&b.0));
    if entities.is_empty() {
        markdown.push_str("This project declares no entities yet.\n\n");
    }
    let mut json_entities = Vec::new();
    for (name, policy) in &entities {
        markdown.push_str(&format!(
            "| `{name}` | create | read | update | delete |\n|---|---|---|---|---|\n| the record | {} | {} | {} | {} |\n",
            cell(policy, Effect::Create),
            cell(policy, Effect::Read),
            cell(policy, Effect::Update),
            cell(policy, Effect::Delete),
        ));
        let membership = access.membership(&policy.entity);
        let remarks = remarks(access, policy, membership);
        if !remarks.is_empty() {
            markdown.push_str(&format!("\n{remarks}\n"));
        }
        markdown.push('\n');
        let mut entry = Value::object();
        entry.set("entity", name.as_str());
        let mut grants = Value::object();
        for effect in Effect::ALL {
            let written: Vec<String> = policy
                .lists
                .get(&effect)
                .map(|list| list.written.clone())
                .unwrap_or_default();
            grants.set(effect.word(), written);
        }
        entry.set("grants", grants);
        match (&access.root.tenant, &policy.tenancy) {
            (Some(_), Some(tenancy)) => entry.set("tenant", tenancy.via.as_str()),
            (Some(_), None) => entry.set("tenant", "shared"),
            (None, _) => {}
        }
        if let Some(membership) = membership {
            let mut held = Value::object();
            held.set("member", membership.member.as_str());
            held.set("role", membership.role.as_str());
            held.set("roles", membership.roles.as_str());
            held.set_some("scope", membership.scope.as_deref());
            entry.set("membership", held);
        }
        entry.set_some("fixed", policy.fixed.clone());
        json_entities.push(entry);
    }
    markdown.push_str("## Actions listed separately\n\n");
    let mut actions: Vec<&ActionGrants> = access.root.actions.iter().collect();
    actions.sort_by(|a, b| a.written.cmp(&b.written));
    if actions.is_empty() {
        markdown
            .push_str("None: every action takes its permission from the entities it reaches.\n");
    } else {
        markdown.push_str(
            "Each replaces its entities' grants on the writes it makes itself; an action \
             that reaches no entity may be called only by these.\n\n\
             | action | who may call it |\n|---|---|\n",
        );
        for line in &actions {
            let who: Vec<String> = line.written_who.iter().map(|w| format!("`{w}`")).collect();
            markdown.push_str(&format!(
                "| `{}` | {} |\n",
                line.written,
                if who.is_empty() {
                    "nobody".to_string()
                } else {
                    who.join(", ")
                }
            ));
        }
    }
    let mut json = Value::object();
    json.set("format", jadpo_graph::manifest::FORMAT);
    json.set_some("tenant", access.root.tenant.as_deref());
    json.set("entities", json_entities);
    json.set(
        "actions",
        actions
            .iter()
            .map(|line| {
                let mut entry = Value::object();
                entry.set("action", line.written.as_str());
                entry.set("who", line.written_who.clone());
                entry
            })
            .collect::<Vec<_>>(),
    );
    Report { markdown, json }
}
