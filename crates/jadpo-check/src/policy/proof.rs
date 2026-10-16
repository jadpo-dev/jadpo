//! Proving who may read what (area 9, "Field restrictions"): whether every
//! caller who reached a record may read one of its fields, or the parent
//! record a query includes through one of its references.
//!
//! A caller reaches a record by passing one of the grants a statement
//! checks, so what is known of them is one [`Facts`] per grant they could
//! have passed: the grant itself, and that they hold a membership in the
//! record's tenant. Facts move along a reference: the caller a record's
//! `owner_id` points at is the user that reference includes, and a role
//! held in the company a record refers to is held in that company. A
//! caller may have passed several checks on the way (the record's `read:`
//! and the include's own statement, or an update's grants and its fields'),
//! each a [`Way`]; a value is proved when, for one of the ways, every
//! grant that way lets in proves it.
//!
//! Proofs are by the grants' shape alone, never by the data: two records
//! that each refer to a company may refer to different ones, so a role
//! held in one proves nothing about the other. The one thing the data
//! always keeps is the tenant: a reference between two of its records
//! points inside it, checked on every write that sets it (chunk 4.7), so
//! what a caller holds in a record's tenant they hold in its parent's.

use std::collections::{BTreeMap, BTreeSet};

use jadpo_db::{Db, Project};
use jadpo_std::http::PrincipalKind;

use super::{Access, Effect, Grant};
use crate::lookups::Lookup;
use crate::types::{Def, TypeView};

/// What is known of a caller on one record.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Fact {
    /// They pass this grant on the record.
    Grant(Grant),
    /// They hold a current membership of this membership entity, in some
    /// role, held in the record itself.
    Member(String),
    /// They hold a membership in the tenant the record belongs to.
    Tenant,
    /// They created the record: what it holds is what they set, or new
    /// (its `id`, its generated fields), so its `read:` hides nothing
    /// from them; a field's own `read:` still applies.
    Fresh,
    /// The record was read as the parent an include follows a reference
    /// to, which its lifecycle may hide (area 6, question 10): a
    /// membership read this way may not be current, so it proves no role.
    MaybeHidden,
}

/// Everything known of a caller who passed one grant.
pub type Facts = Vec<Fact>;

/// One check a caller passed to reach a record: the facts for each grant
/// it lets in.
pub type Way = Vec<Facts>;

/// Every check a caller passed to reach a record. Proved when one of them
/// proves it for every grant it lets in; with none, nothing is proved.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Reached {
    pub ways: Vec<Way>,
}

impl Reached {
    pub fn by(way: Way) -> Reached {
        Reached { ways: vec![way] }
    }

    /// Both: a caller who passed every check of each.
    pub fn and(mut self, other: Reached) -> Reached {
        for way in other.ways {
            if !self.ways.contains(&way) {
                self.ways.push(way);
            }
        }
        self
    }
}

/// What proofs read: every entity's policy, the principal's entities, and
/// the roles each membership can hold.
#[derive(Debug)]
pub struct Prover<'a> {
    pub access: &'a Access,
    user: Option<String>,
    service: Option<String>,
    /// Each membership entity's role enum's values, in order, when its
    /// `role:` always holds one: a membership whose role may be `none`
    /// holds none of them.
    roles: BTreeMap<String, Vec<String>>,
    /// The entities whose lifecycle hides some records (`visible:`).
    hiding: BTreeSet<String>,
}

impl<'a> Prover<'a> {
    pub fn new(db: &'a dyn Db, project: Project) -> Prover<'a> {
        let access = super::access(db, project);
        let identity = crate::identity::identity(db, project);
        let lookup = Lookup::new(db, project);
        let entities = crate::data::Entities::new(db, project);
        let model = crate::data::data_model(db, project);
        let hiding = model
            .entities
            .iter()
            .filter(|entity| {
                entity
                    .lifecycle
                    .as_ref()
                    .is_some_and(|lifecycle| lifecycle.visible.is_some())
            })
            .map(|entity| entity.name.clone())
            .collect();
        let roles = access
            .memberships
            .iter()
            .filter(|m| {
                entities.get(&m.entity).is_some_and(|entity| {
                    entity
                        .fields
                        .iter()
                        .any(|field| field.name == m.role && !field.nullable)
                })
            })
            .filter_map(|m| match lookup.def(&m.roles) {
                Some(Def::Enum { variants }) => Some((
                    m.entity.clone(),
                    variants.iter().map(|v| v.name.clone()).collect(),
                )),
                _ => None,
            })
            .collect();
        Prover {
            access,
            user: identity
                .variant(PrincipalKind::User)
                .map(|v| v.entity.clone()),
            service: identity
                .variant(PrincipalKind::Service)
                .map(|v| v.entity.clone()),
            roles,
            hiding,
        }
    }

    fn principal(&self, kind: PrincipalKind) -> Option<&str> {
        match kind {
            PrincipalKind::User => self.user.as_deref(),
            PrincipalKind::Service => self.service.as_deref(),
        }
    }

    /// `entity`'s grants for `effect`.
    pub fn grants(&self, entity: &str, effect: Effect) -> Vec<Grant> {
        self.access
            .entity(entity)
            .map(|policy| policy.grants(effect).to_vec())
            .unwrap_or_default()
    }

    /// The check of `grants` on a record of `entity`: each grant, and the
    /// record's tenancy, which every grant on it applies within.
    pub fn way(&self, entity: &str, grants: &[Grant]) -> Way {
        let tenant = self
            .access
            .entity(entity)
            .is_some_and(|policy| policy.tenancy.is_some());
        grants
            .iter()
            .map(|grant| {
                let mut facts = vec![Fact::Grant(grant.clone())];
                if tenant {
                    facts.push(Fact::Tenant);
                }
                facts
            })
            .collect()
    }

    /// A record of `entity` read through its `read:`.
    pub fn read(&self, entity: &str) -> Reached {
        Reached::by(self.way(entity, &self.grants(entity, Effect::Read)))
    }

    /// Whether `facts` show the caller passes `target` on the record.
    fn proves(&self, facts: &Facts, target: &Grant) -> bool {
        match target {
            Grant::Public => true,
            Grant::Signed => facts.iter().any(|fact| {
                matches!(
                    fact,
                    Fact::Member(_)
                        | Fact::Grant(Grant::Caller { .. } | Grant::Role { .. } | Grant::Signed)
                )
            }),
            other => facts.contains(&Fact::Grant(other.clone())),
        }
    }

    /// Whether `facts` show the caller passes one of `grants`: one of them
    /// alone, or a membership held in the record whose every role the list
    /// names.
    fn proves_any(&self, facts: &Facts, grants: &[Grant]) -> bool {
        grants.iter().any(|grant| self.proves(facts, grant))
            || facts.iter().any(|fact| match fact {
                Fact::Member(membership) => self.roles.get(membership).is_some_and(|roles| {
                    roles.iter().all(|role| {
                        grants.contains(&Grant::Role {
                            membership: membership.clone(),
                            role: role.clone(),
                            via: Some("id".to_string()),
                        })
                    })
                }),
                _ => false,
            })
    }

    /// Whether `facts` show the caller passes one of `grants` on a record of
    /// `entity`: as [`Self::proves_any`] says, or because they hold a
    /// membership in its tenant and the list names every role of every
    /// membership held in the tenant, through its tenancy's reference.
    fn proves_on(&self, entity: &str, facts: &Facts, grants: &[Grant]) -> bool {
        if self.proves_any(facts, grants) {
            return true;
        }
        let Some(tenancy) = self
            .access
            .entity(entity)
            .and_then(|policy| policy.tenancy.as_ref())
        else {
            return false;
        };
        let tenants = &self.access.root.tenant_memberships;
        facts.contains(&Fact::Tenant)
            && !tenants.is_empty()
            && tenants.iter().all(|membership| {
                self.roles.get(membership).is_some_and(|roles| {
                    roles.iter().all(|role| {
                        grants.contains(&Grant::Role {
                            membership: membership.clone(),
                            role: role.clone(),
                            via: Some(tenancy.via.clone()),
                        })
                    })
                })
            })
    }

    /// Whether `facts` show the caller belongs to the tenant a record of
    /// `entity` belongs to; always, for a record that belongs to none.
    fn tenant(&self, entity: &str, facts: &Facts) -> bool {
        let Some(tenancy) = self
            .access
            .entity(entity)
            .and_then(|policy| policy.tenancy.as_ref())
        else {
            return true;
        };
        let tenants = &self.access.root.tenant_memberships;
        facts.iter().any(|fact| match fact {
            Fact::Tenant => true,
            Fact::Grant(Grant::Role {
                membership,
                via: Some(via),
                ..
            }) => tenants.contains(membership) && *via == tenancy.via,
            Fact::Member(membership) => tenancy.via == "id" && tenants.contains(membership),
            _ => false,
        })
    }

    /// Whether `facts` let the caller read a record of `entity`.
    pub fn readable(&self, entity: &str, facts: &Facts) -> bool {
        self.proves_on(entity, facts, &self.grants(entity, Effect::Read)) && self.tenant(entity, facts)
    }

    /// Whether `facts` let the caller read the field `field` of a record
    /// of `entity`: the record's `read:` (unless they created it, or the
    /// field is its `id`, which they named to reach it), and the field's
    /// own `read:` when it narrows the record's.
    pub fn field_readable(&self, entity: &str, field: &str, facts: &Facts) -> bool {
        let record =
            field == "id" || facts.contains(&Fact::Fresh) || self.readable(entity, facts);
        record
            && match self
                .access
                .entity(entity)
                .and_then(|policy| policy.field_grants(field, Effect::Read))
            {
                None => true,
                Some(grants) => self.proves_on(entity, facts, grants),
            }
    }

    /// What a caller with `facts` on a record of `from` is known to pass on
    /// the record of `to` its reference `via` points at.
    pub fn along(&self, from: &str, via: &str, to: &str, facts: &Facts) -> Facts {
        let mut out: Facts = Vec::new();
        let mut push = |fact: Fact| {
            if !out.contains(&fact) {
                out.push(fact);
            }
        };
        // A membership read as an include's parent may be one its
        // lifecycle hides, which grants nothing.
        let held = self
            .access
            .membership(from)
            .filter(|_| !facts.contains(&Fact::MaybeHidden));
        // A reference between two of the tenant's records points inside
        // the tenant, so what the caller holds in the record's tenant they
        // hold in the parent's.
        let inside = self.access.kept_in_tenant(from, to);
        for fact in facts {
            match fact {
                Fact::Grant(grant @ (Grant::Public | Grant::Signed | Grant::System)) => {
                    push(Fact::Grant(grant.clone()));
                }
                Fact::Grant(Grant::Caller { field, kind }) => {
                    push(Fact::Grant(Grant::Signed));
                    if field == via && self.principal(*kind) == Some(to) {
                        push(Fact::Grant(Grant::Caller {
                            field: "id".to_string(),
                            kind: *kind,
                        }));
                    }
                    // The member of a membership holds it, in the record
                    // its scope points at.
                    if let Some(held) = held
                        && held.member == *field
                        && held.scope.as_deref() == Some(via)
                    {
                        push(Fact::Member(held.entity.clone()));
                    }
                }
                Fact::Grant(Grant::Role {
                    membership,
                    role,
                    via: through,
                }) => {
                    push(Fact::Grant(Grant::Signed));
                    let scope = self
                        .access
                        .membership(membership)
                        .and_then(|m| m.scope_entity.as_deref());
                    match through {
                        None => push(Fact::Grant(Grant::Role {
                            membership: membership.clone(),
                            role: role.clone(),
                            via: None,
                        })),
                        Some(through) if through == via && scope == Some(to) => {
                            push(Fact::Grant(Grant::Role {
                                membership: membership.clone(),
                                role: role.clone(),
                                via: Some("id".to_string()),
                            }));
                            push(Fact::Member(membership.clone()));
                        }
                        // A role held in the record's tenant is held in
                        // the tenant of the parent it points at.
                        Some(through)
                            if inside.is_some_and(|(own, _)| {
                                *through == own.via && scope == Some(own.entity.as_str())
                            }) =>
                        {
                            if let Some((_, theirs)) = inside {
                                push(Fact::Grant(Grant::Role {
                                    membership: membership.clone(),
                                    role: role.clone(),
                                    via: Some(theirs.via.clone()),
                                }));
                            }
                        }
                        Some(_) => {}
                    }
                }
                Fact::Member(_) => push(Fact::Grant(Grant::Signed)),
                Fact::Fresh | Fact::MaybeHidden => {}
                Fact::Tenant if inside.is_some() => push(Fact::Tenant),
                Fact::Tenant => {
                    let follows = self
                        .access
                        .entity(from)
                        .and_then(|policy| policy.tenancy.as_ref())
                        .is_some_and(|tenancy| tenancy.via == via);
                    let is_tenant = self
                        .access
                        .entity(to)
                        .and_then(|policy| policy.tenancy.as_ref())
                        .is_some_and(|tenancy| tenancy.via == "id");
                    if follows && is_tenant {
                        push(Fact::Tenant);
                    }
                }
            }
        }
        out
    }

    /// Whether every caller who reached a record of `from` may read the
    /// record of `to` its reference `via` points at.
    pub fn parent_readable(&self, from: &str, via: &str, to: &str, reached: &Reached) -> bool {
        reached.ways.iter().any(|way| {
            way.iter()
                .all(|facts| self.readable(to, &self.along(from, via, to, facts)))
        })
    }

    /// How a caller reached the parent an include reads: along each way to
    /// the record that refers to it, and through the parent's own `read:`,
    /// which the include's statement checks too.
    pub fn parent(&self, from: &str, via: &str, to: &str, reached: &Reached) -> Reached {
        let along = Reached {
            ways: reached
                .ways
                .iter()
                .map(|way| {
                    way.iter()
                        .map(|facts| self.along(from, via, to, facts))
                        .collect()
                })
                .collect(),
        };
        let mut reached = along.and(self.read(to));
        if self.hiding.contains(to) {
            for way in &mut reached.ways {
                for facts in way.iter_mut() {
                    if !facts.contains(&Fact::MaybeHidden) {
                        facts.push(Fact::MaybeHidden);
                    }
                }
            }
        }
        reached
    }

    /// Whether every caller who reached a record of `entity` may read its
    /// field `field`.
    pub fn field_proved(&self, entity: &str, field: &str, reached: &Reached) -> bool {
        reached.ways.iter().any(|way| {
            way.iter()
                .all(|facts| self.field_readable(entity, field, facts))
        })
    }

    /// For a reason: the first grant of the first way that doesn't prove
    /// `holds`, described.
    pub fn counterexample(
        &self,
        reached: &Reached,
        holds: impl Fn(&Facts) -> bool,
    ) -> Option<String> {
        let way = reached.ways.first()?;
        way.iter().find(|facts| !holds(facts)).and_then(|facts| {
            facts.iter().find_map(|fact| match fact {
                Fact::Grant(grant) => Some(self.described(grant)),
                _ => None,
            })
        })
    }

    /// A grant as a reason says who it lets in: "the user `owner_id`
    /// points at", `CompanyRole.editor`, "anyone signed in".
    pub fn described(&self, grant: &Grant) -> String {
        match grant {
            Grant::Caller { field, kind } => {
                let who = match kind {
                    PrincipalKind::User => "user",
                    PrincipalKind::Service => "service",
                };
                if field == "id" {
                    format!("the {who} the record is")
                } else {
                    format!("the {who} `{field}` points at")
                }
            }
            Grant::Role { .. } => format!("`{}`", super::written_grant(self.access, grant)),
            Grant::Signed => "anyone signed in".to_string(),
            Grant::Public => "anyone at all".to_string(),
            Grant::System => "schedules and subscribers".to_string(),
        }
    }

    /// Who may read field `field` of `entity`, for a reason: "only the
    /// user `reviewer_id` points at may read".
    pub fn readers(&self, entity: &str, field: &str) -> String {
        let Some(policy) = self.access.entity(entity) else {
            return "nobody may read".to_string();
        };
        let record = || {
            let grants = policy.grants(Effect::Read);
            if grants.is_empty() {
                "nobody may read".to_string()
            } else {
                format!(
                    "only {} may read",
                    crate::and_list(
                        &grants
                            .iter()
                            .map(|grant| self.described(grant))
                            .collect::<Vec<_>>()
                    )
                )
            }
        };
        match policy.field(field) {
            Some(own) if own.server_only => {
                "no client may read it, as it is `FieldAccess.server_only`".to_string()
            }
            Some(own) => match own.lists.get(&Effect::Read) {
                Some(list) if !list.grants.is_empty() => format!(
                    "only {} may read",
                    crate::and_list(
                        &list
                            .grants
                            .iter()
                            .map(|grant| self.described(grant))
                            .collect::<Vec<_>>()
                    )
                ),
                Some(_) => "nobody may read".to_string(),
                None => record(),
            },
            None => record(),
        }
    }
}
