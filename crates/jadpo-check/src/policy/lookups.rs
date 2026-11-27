//! What checks an action that looks a person up by email before the lookup
//! runs (area 15, "Looking up one person by email"; area 9, "Permissions
//! on actions"; chunk 10.13b).
//!
//! `<Entity>.lookup(email: x)` reads behind the calling action's own line
//! under root `policy { actions: }`, never the entity's `read:`. So the
//! line must be one the runtime checks before the lookup runs:
//!
//! - anyone signed in, or a role held across the whole application, which
//!   every listed action is checked against as it starts;
//! - or, for a line naming roles held in a record (`WorkspaceRole.owner`),
//!   each such role in the record whose `id` the action is given as its
//!   one parameter of that type, checked as the action starts, as a
//!   role held across the application is. An entry naming a user field of
//!   a record (`Todo.owner_id`) can't be checked before the action finds
//!   that record, so it lets no one past the lookup.
//!
//! An action that isn't listed, is listed for `Access.public`, or whose
//! line leaves nothing to check before the lookup is
//! `policy.lookup_unchecked`.

use jadpo_db::{Db, Project};
use jadpo_graph::TypeRef;

use super::{ActionWho, LineGate, Policies};

/// A role an action that looks someone up is checked against as it starts.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ScopedRole {
    /// The membership entity: `Membership`.
    pub membership: String,
    /// The role's value: `owner`.
    pub role: String,
    /// The action's parameter holding the `id` of the record the role is
    /// held in; `None` for a role held across the whole application.
    pub param: Option<usize>,
}

/// What checks an action that looks someone up before the lookup runs.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum LookupCheck {
    /// Its line is checked as it starts already: anyone signed in, roles
    /// held across the whole application, or nobody.
    AtStart,
    /// Its line names roles held in a record: these, checked as it starts.
    Scoped(Vec<ScopedRole>),
    /// Nothing checks it before the lookup.
    Unchecked(Unchecked),
}

/// Why nothing checks an action before its lookup.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Unchecked {
    /// It isn't listed under `actions:`.
    NotListed,
    /// Its line names `Access.public`.
    Public,
    /// Its line names only what a record it hasn't found yet decides: a
    /// user field, or a role held in a record whose `id` it isn't given as
    /// exactly one parameter.
    NoRecord,
}

/// What checks the action called `action`, whose parameters are typed
/// `params`, before a lookup in its body runs.
pub fn lookup_check(db: &dyn Db, project: Project, action: &str, params: &[TypeRef]) -> LookupCheck {
    let policies = Policies::new(db, project);
    let Some(line) = policies.root().action(action) else {
        return LookupCheck::Unchecked(Unchecked::NotListed);
    };
    match super::reach::line_gate(db, project, action) {
        Some(LineGate::Anyone) => return LookupCheck::Unchecked(Unchecked::Public),
        Some(LineGate::Signed | LineGate::Roles(_) | LineGate::Nobody) => {
            return LookupCheck::AtStart;
        }
        Some(LineGate::Statements) | None => {}
    }
    let mut roles = Vec::new();
    for who in &line.who {
        let ActionWho::Role { membership, role } = who else {
            continue;
        };
        let Some(held) = policies.membership(membership) else {
            continue;
        };
        let param = match &held.scope_entity {
            None => None,
            Some(scope) => {
                let id = format!("{scope}.id");
                let matching: Vec<usize> = params
                    .iter()
                    .enumerate()
                    .filter(|(_, ty)| matches!(ty, TypeRef::Field(key) if key.name() == id))
                    .map(|(at, _)| at)
                    .collect();
                match matching.as_slice() {
                    [one] => Some(*one),
                    _ => continue,
                }
            }
        };
        roles.push(ScopedRole {
            membership: membership.clone(),
            role: role.clone(),
            param,
        });
    }
    if roles.is_empty() {
        LookupCheck::Unchecked(Unchecked::NoRecord)
    } else {
        LookupCheck::Scoped(roles)
    }
}
