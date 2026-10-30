//! A call the listing doesn't cover (area 9, "A call the listing doesn't
//! cover"): a line under `actions:` replaces its entities' grants only on
//! the creates, updates, deletes and transitions in its action's own body,
//! and an action it calls keeps its own grants, with the same caller. So a
//! listed action can call one that refuses some of the people its line
//! lets in, which they meet as 403 `not_permitted`. `jadpo check` reports
//! that call as `policy.call_ungranted`, naming those people: a declinable
//! error, for a call only some callers are meant to pass.
//!
//! What the called action refuses is read from what it and every action
//! it calls do ([`super::reach::acts_of`]): each create, update and delete
//! under its entity's grants, or under the line of a listed action whose
//! own body makes it, a field's own grants where they narrow the record's,
//! and the line a listed action or query checks as it starts. Each person
//! the line names is held to each of these by the grants' shape alone, as
//! [`super::proof`] proves who may read: an entry that depends on a record
//! (`Todo.owner_id`) stands for the caller that grant names on a record of
//! the same kind, and for someone signed in on any other. Tenancy is left
//! out, since a line never lifts it.

use std::collections::BTreeMap;

use jadpo_db::{Db, File, Project};
use jadpo_diag::{Diagnostic, Span, rule};
use jadpo_graph::Kind;

use super::proof::{Fact, Facts, Prover};
use super::reach::{Act, acts, calls_from, line_gate, written_action};
use super::{ActionGrants, ActionWho, Effect, Grant, LineGate, Membership, Policies};
use crate::data::Entities;
use crate::lookups::Lookup;
use crate::types::{Fact as Does, callables_of, facts_at, typed_body};

/// One check the work of a called action makes of its caller.
#[derive(Clone, Debug)]
enum Guard {
    /// A create, update or delete of `entity`'s records by the action
    /// `by`, which only `grants` let in: the record's, a listed action's
    /// line on it, or with `field`, that field's own.
    Write {
        by: String,
        entity: String,
        effect: Effect,
        field: Option<String>,
        grants: Vec<Grant>,
    },
    /// The line of a listed action or query, `written`, checked as it
    /// starts; `system` when it names `Access.system`.
    Line {
        written: String,
        query: bool,
        gate: LineGate,
        system: bool,
    },
}

/// Every check the action called `callee` makes of its caller, itself and
/// through every action it calls.
fn guards(db: &dyn Db, project: Project, access: &Policies<'_>, callee: &str) -> Vec<Guard> {
    let entities = Entities::new(db, project);
    let root = access.root();
    let mut out = Vec::new();
    for action in calls_from(db, project, callee) {
        let listed = root.action(&action);
        if let Some(line) = listed
            && let Some(gate) = line_gate(db, project, &action)
            && matches!(
                gate,
                LineGate::Signed | LineGate::Roles(_) | LineGate::Nobody
            )
        {
            out.push(Guard::Line {
                written: line.written.clone(),
                query: false,
                gate,
                system: line.who.contains(&ActionWho::System),
            });
        }
        for act in acts(db, project, &action) {
            match act {
                Act::Entity(
                    entity,
                    effect @ (Effect::Create | Effect::Update | Effect::Delete),
                ) => {
                    let grants = match (listed, entities.get(entity)) {
                        (Some(line), Some(model)) => line.on(model, access.memberships()),
                        (Some(_), None) => Vec::new(),
                        (None, _) => access
                            .entity(entity)
                            .map(|policy| policy.grants(*effect).to_vec())
                            .unwrap_or_default(),
                    };
                    out.push(Guard::Write {
                        by: action.clone(),
                        entity: entity.clone(),
                        effect: *effect,
                        field: None,
                        grants,
                    });
                }
                Act::Field(entity, field, effect) => {
                    if let Some(grants) = access
                        .entity(entity)
                        .and_then(|policy| policy.field_grants(field, *effect))
                    {
                        out.push(Guard::Write {
                            by: action.clone(),
                            entity: entity.clone(),
                            effect: *effect,
                            field: Some(field.clone()),
                            grants: grants.to_vec(),
                        });
                    }
                }
                Act::Queries(query) => {
                    if let Some(line) = root.query(query)
                        && let Some(gate) = root.query_gate(query, access.memberships())
                    {
                        out.push(Guard::Line {
                            written: line.written.clone(),
                            query: true,
                            gate,
                            system: false,
                        });
                    }
                }
                Act::Entity(..) | Act::Calls(_) | Act::SignsIn => {}
            }
        }
    }
    out
}

/// What is known of the people `who` lets in, on a record of `entity`:
/// the grants it amounts to there, or for an entry that depends on a
/// record of another kind, that they are signed in. Their tenancy is
/// taken as met, since a line never lifts it.
fn facts(
    prover: &Prover<'_>,
    entities: &Entities<'_>,
    who: &ActionWho,
    entity: &str,
    memberships: &[Membership],
) -> Facts {
    let mut out: Facts = match who {
        ActionWho::Signed => vec![Fact::Grant(Grant::Signed)],
        ActionWho::Public => vec![Fact::Grant(Grant::Public)],
        ActionWho::System => vec![Fact::Grant(Grant::System)],
        ActionWho::Role { membership, role }
            if memberships
                .iter()
                .any(|m| m.entity == *membership && m.scope.is_none()) =>
        {
            vec![Fact::Grant(Grant::Role {
                membership: membership.clone(),
                role: role.clone(),
                via: None,
            })]
        }
        ActionWho::Field { .. } | ActionWho::Role { .. } => {
            let line = ActionGrants {
                action: String::new(),
                written: String::new(),
                who: vec![who.clone()],
                written_who: Vec::new(),
            };
            let on = entities
                .get(entity)
                .map(|model| line.on(model, memberships))
                .unwrap_or_default();
            if on.is_empty() {
                vec![Fact::Grant(Grant::Signed)]
            } else {
                on.into_iter().map(Fact::Grant).collect()
            }
        }
    };
    if prover
        .policies
        .entity(entity)
        .is_some_and(|policy| policy.tenancy.is_some())
    {
        out.push(Fact::Tenant);
    }
    out
}

/// Whether the people `who` lets in pass `guard`.
fn passes(
    prover: &Prover<'_>,
    entities: &Entities<'_>,
    who: &ActionWho,
    guard: &Guard,
    memberships: &[Membership],
) -> bool {
    match guard {
        Guard::Write { entity, grants, .. } => prover.passes(
            entity,
            &facts(prover, entities, who, entity, memberships),
            grants,
        ),
        Guard::Line { gate, system, .. } => match (who, gate) {
            (ActionWho::System, _) => *system,
            (_, LineGate::Anyone | LineGate::Statements) => true,
            (ActionWho::Public, _) => false,
            (_, LineGate::Signed) => true,
            (ActionWho::Role { membership, role }, LineGate::Roles(roles)) => roles
                .iter()
                .any(|(held, named)| held == membership && named == role),
            (_, LineGate::Roles(_) | LineGate::Nobody) => false,
        },
    }
}

/// Why `guard` refuses some of the people a line lets in, from the call
/// to `callee`: "`lend` changes `Book` records, which only
/// `StaffRole.admin` may".
fn refusal(prover: &Prover<'_>, callee: &str, guard: &Guard) -> String {
    match guard {
        Guard::Write {
            by,
            entity,
            effect,
            field,
            grants,
        } => {
            let doer = if by == callee {
                format!("`{callee}`")
            } else {
                format!("`{by}`, which `{callee}` calls,")
            };
            let who = if grants.is_empty() {
                "nobody may".to_string()
            } else {
                format!(
                    "only {} may",
                    crate::and_list(
                        &grants
                            .iter()
                            .map(|grant| prover.described(grant))
                            .collect::<Vec<_>>()
                    )
                )
            };
            match field {
                Some(field) => format!(
                    "{doer} sets `{entity}.{field}`, which {who} {}",
                    effect.word()
                ),
                None => {
                    let does = match effect {
                        Effect::Create => format!("creates a `{entity}`"),
                        Effect::Update => format!("changes `{entity}` records"),
                        Effect::Delete | Effect::Read => format!("deletes `{entity}` records"),
                    };
                    format!("{doer} {does}, which {who}")
                }
            }
        }
        Guard::Line {
            written,
            query,
            gate,
            system,
        } => {
            let what = if *query { "the query" } else { "the action" };
            let who = match gate {
                LineGate::Signed => "anyone signed in".to_string(),
                LineGate::Roles(roles) => crate::and_list(
                    &roles
                        .iter()
                        .map(|(membership, role)| {
                            let grant = Grant::Role {
                                membership: membership.clone(),
                                role: role.clone(),
                                via: None,
                            };
                            prover.described(&grant)
                        })
                        .collect::<Vec<_>>(),
                ),
                LineGate::Anyone | LineGate::Statements | LineGate::Nobody => "nobody".to_string(),
            };
            let who = if *system && *gate != LineGate::Signed {
                format!("{who} and schedules and subscribers")
            } else {
                who
            };
            format!(
                "`{callee}` runs {what} `{written}`, whose line under `actions:` lets in only {who}"
            )
        }
    }
}

/// `policy.call_ungranted`: each call in `file`'s listed actions to an
/// action that refuses some of the people the action's line lets in.
#[salsa::tracked(returns(ref))]
pub fn ungranted_call_problems(db: &dyn Db, project: Project, file: File) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    let callables = callables_of(db, file);
    if callables.is_empty() || !super::policy_clean(db, project) {
        return out;
    }
    let access = Policies::new(db, project);
    let root = access.root();
    if root.actions.is_empty() {
        return out;
    }
    let prover = Prover::new(db, project);
    let entities = Entities::new(db, project);
    let lookup = Lookup::new(db, project);
    let memberships = access.memberships();
    let path = file.path(db);
    let mut known: BTreeMap<String, Vec<Guard>> = BTreeMap::new();
    for callable in callables {
        let checked = typed_body(db, project, callable);
        let Some(key) = checked
            .key
            .as_ref()
            .filter(|key| key.kind() == Some(Kind::Action))
        else {
            continue;
        };
        let name = key.name();
        let Some(line) = root.action(name) else {
            continue;
        };
        for (does, (start, end)) in facts_at(&checked.body) {
            let Does::Action(called) = does else {
                continue;
            };
            let callee = called.name();
            let guards = known
                .entry(callee.to_string())
                .or_insert_with(|| guards(db, project, &access, callee));
            // Each person the line names that some check refuses, with
            // the first check that does.
            let refused: Vec<(&str, &Guard)> = line
                .who
                .iter()
                .zip(&line.written_who)
                .filter_map(|(who, written)| {
                    guards
                        .iter()
                        .find(|guard| !passes(&prover, &entities, who, guard, memberships))
                        .map(|guard| (written.as_str(), guard))
                })
                .collect();
            let Some((_, guard)) = refused.first() else {
                continue;
            };
            let named: Vec<String> = refused
                .iter()
                .map(|(written, _)| format!("`{written}`"))
                .collect();
            let why = refusal(&prover, callee, guard);
            let listing = written_action(&lookup, callee);
            out.push(
                Diagnostic::new(
                    &rule::POLICY_CALL_UNGRANTED,
                    format!(
                        "`{name}` calls `{callee}`, which refuses some of the people `{}`'s line lets in",
                        line.written
                    ),
                )
                .at(
                    Span::new(path, start, end),
                    format!("refuses some callers {} lets in", crate::and_list(&named)),
                )
                .reason(format!(
                    "`{}`'s line under `actions:` covers only the creates, updates, deletes and transitions in `{name}`'s own body; `{callee}` keeps its own grants, with the same caller. {why}, so some callers {} lets in get 403 `not_permitted` from it.",
                    line.written,
                    crate::and_list(&named)
                ))
                .next(format!(
                    "Make the change in `{name}`'s own body, or list `{listing}` under `actions:` for exactly the people `{}`'s line names",
                    line.written
                )),
            );
        }
    }
    out
}
