//! Restricted values stored elsewhere (area 9, "Field
//! restrictions"): a create or update stores a value in a
//! field only when everyone who may read that field may read the value,
//! proved as an output is; otherwise `policy.output_field_unproved` names
//! the write. What is stored is read later, from another request, by
//! whoever the field's grants let in, so the field's readers stand where
//! an output's caller does.
//!
//! Who reads the field is known only of the record it is stored in, so a
//! value's own record must be found from there, by the grants' shape and
//! never by the data ([`Rel`]): the same record (a field copied within
//! it, or a counter reading its stored value); the record a reference the
//! write sets points at, or one found by the `id` it holds; a record of
//! the same tenant, when the write sets its tenant from the value's
//! record. Of any other record its readers are known only to be signed
//! in, or to hold a role across the whole application.
//!
//! Everything a write stores is followed: `set:`, a patch's fields and
//! what `when_supplied:` sets, and the fields a transition sets, with
//! whatever decided that it runs and its `when:`. A value from a
//! parameter is checked where the action is called ([`Store`]). An
//! `emit` stores nothing a caller reads: an event's subscribers are server
//! work (area 12), as `Access.system` reading a field is.

use std::collections::{BTreeMap, BTreeSet};

use jadpo_diag::{Decider, Diagnostic, rule};
use jadpo_graph::body::{Callee, Expr, Node, SiteId};
use jadpo_graph::{Key, TypeRef};
use jadpo_std::http::PrincipalKind;
use jadpo_std::write::WriteOp;

use super::{
    DEPTH, Flow, Origin, Prov, Run, Src, Test, Walk, erased, field_name, retold, shallow,
    tenancy_of, widen,
};
use crate::policy::proof::{Fact, Facts, Reached, Way};
use crate::policy::{Effect, Grant};
use crate::types::{Fact as Did, TypeView, body_facts, resolve};

/// Where a stored value is written: a site of the body walked, or a span
/// elsewhere (a subscriber's argument, chunk 6.1c).
#[derive(Clone, Copy, Debug)]
pub(super) enum At<'s> {
    Site(SiteId),
    Span(&'s jadpo_diag::Span),
}

/// The [`Origin`] of a record of the event's tenant (chunk 6.1c): a value
/// an `emit` put in an event from a record whose tenancy reference is the
/// one the event's tenant field holds. The subscriber is limited to that
/// tenant, so any record of a tenanted entity it stores the value in is of
/// the same tenant; no other relation survives the event, since the record
/// may have changed between the `emit` and the delivery.
pub(super) const TENANT: u32 = u32::MAX;

/// One value known exactly: a parameter's part, or a field of a record
/// the body knows ([`Origin`]); a whole record stands for its `id`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Ident {
    Param(usize, Vec<String>),
    Field(String, String, u32),
}

fn ident(src: &Src) -> Option<Ident> {
    match src {
        Src::Param(at, path) => Some(Ident::Param(*at, path.clone())),
        Src::Field {
            entity,
            field,
            origin: Some(origin),
            ..
        } => Some(Ident::Field(entity.clone(), field.clone(), *origin)),
        Src::Record {
            entity,
            origin: Some(origin),
            ..
        } => Some(Ident::Field(entity.clone(), "id".to_string(), *origin)),
        _ => None,
    }
}

/// What a value holding only `srcs` (beside what decided it) is, when
/// that is one value known exactly.
pub(super) fn single(srcs: &[Src]) -> Option<Ident> {
    let values: Vec<&Src> = srcs
        .iter()
        .filter(|src| !matches!(src, Src::Decided(_)))
        .collect();
    match values.as_slice() {
        [one] => ident(one),
        _ => None,
    }
}

/// The record of `entity` a write's target `srcs` is, when it is one
/// record the body loaded. A `var mut` a change's result is put back in
/// holds that record as each write gave it back, with what each set, so
/// sources that differ only in that are still the one record.
pub(super) fn loaded(entity: &str, srcs: &[Src]) -> Origin {
    let mut origins = srcs
        .iter()
        .filter(|src| !matches!(src, Src::Decided(_)))
        .map(|src| match src {
            Src::Record {
                entity: of, origin, ..
            } if of == entity => *origin,
            _ => None,
        });
    let first = origins.next()??;
    origins.all(|origin| origin == Some(first)).then_some(first)
}

/// The values of `srcs` beside what decided them.
fn values(srcs: &[Src]) -> Vec<Src> {
    srcs.iter()
        .filter(|src| !matches!(src, Src::Decided(_)))
        .cloned()
        .collect()
}

/// The record a write stores values in: its entity, which record it is
/// (its own [`Origin`], and the target an update was given), each
/// reference the write sets with what it is set from (a reference it may
/// or may not set, a patch's or one `when_supplied:` sets, has nothing),
/// and the references it holds that nothing else the body runs may
/// change, so a record found by one is still the one it refers to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Dest {
    entity: String,
    own: Origin,
    target: Vec<Src>,
    refs: Vec<(String, Vec<Src>)>,
    held: Vec<String>,
}

impl Dest {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn of(
        flow: &Flow<'_>,
        entity: &str,
        target: Option<&Prov>,
        own: Origin,
        set: &[(String, SiteId, Prov)],
        patched: Vec<String>,
        supplied: &[(String, SiteId, Prov)],
    ) -> Dest {
        let reference = |name: &str| {
            flow.entities.get(entity).is_some_and(|model| {
                model
                    .fields
                    .iter()
                    .any(|field| field.name == name && field.reference.is_some())
            })
        };
        let mut refs: Vec<(String, Vec<Src>)> = set
            .iter()
            .filter(|(name, _, _)| reference(name))
            .map(|(name, _, prov)| (name.clone(), values(&prov.whole)))
            .collect();
        for name in patched
            .iter()
            .chain(supplied.iter().map(|(name, _, _)| name))
        {
            if reference(name) {
                refs.retain(|(set, _)| set != name);
                refs.push((name.clone(), Vec::new()));
            }
        }
        Dest {
            entity: entity.to_string(),
            own,
            target: target.map(|t| values(&t.whole)).unwrap_or_default(),
            refs,
            held: Vec::new(),
        }
    }

    /// As a summary holds it: no [`Origin`]. A record the body found by
    /// a parameter's `id` (`found`) is held as that parameter, so where
    /// the action is called it is the record the caller passed the `id`
    /// of: the record a caller read and the one its callee changes are
    /// then the same record (area 9, "Field restrictions").
    fn erased(&self, found: &BTreeMap<u32, Ident>) -> Dest {
        let by_param = |src: &Src| match src {
            Src::Record {
                entity,
                origin: Some(origin),
                ..
            } if *entity == self.entity => match found.get(origin) {
                Some(Ident::Param(at, path)) => Some(Src::Param(*at, path.clone())),
                _ => None,
            },
            _ => None,
        };
        let mut target: Vec<Src> = self
            .target
            .iter()
            .map(|src| by_param(src).unwrap_or_else(|| erased(src)))
            .collect();
        if let Some(Ident::Param(at, path)) = self.own.and_then(|own| found.get(&own))
            && target.is_empty()
        {
            target.push(Src::Param(*at, path.clone()));
        }
        Dest {
            entity: self.entity.clone(),
            own: None,
            target,
            refs: self
                .refs
                .iter()
                .map(|(name, srcs)| (name.clone(), srcs.iter().map(erased).collect()))
                .collect(),
            held: self.held.clone(),
        }
    }

    /// In the caller's terms, given what `args` say was passed.
    fn substitute(&self, args: &[Prov]) -> Dest {
        let over = |srcs: &[Src]| {
            let prov = Prov {
                whole: srcs.to_vec(),
                fields: Default::default(),
            };
            values(&prov.substitute(args).whole)
        };
        Dest {
            entity: self.entity.clone(),
            own: None,
            target: over(&self.target),
            refs: self
                .refs
                .iter()
                .map(|(name, srcs)| (name.clone(), over(srcs)))
                .collect(),
            held: self.held.clone(),
        }
    }

    fn widened(&self) -> Dest {
        let cut = |srcs: &[Src]| srcs.iter().cloned().map(widen).collect();
        Dest {
            entity: self.entity.clone(),
            own: None,
            target: cut(&self.target),
            refs: self
                .refs
                .iter()
                .map(|(name, srcs)| (name.clone(), cut(srcs)))
                .collect(),
            held: self.held.clone(),
        }
    }
}

/// A part of a parameter an action or function stores in `field` of
/// `dest`: whoever calls it is checked against everyone who may read that
/// field, with what they pass. Positionless, as a summary is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Store {
    dest: Dest,
    field: String,
    /// A [`Src::Param`], or what decided by one, or deep inside one.
    value: Src,
    /// When a batch's run stores it (chunk 6.4d): the parameters' parts
    /// holding the tenant ids the run saved. The value has left its
    /// record, but for one whose tenancy reference is among them.
    run: Option<Vec<(usize, Vec<String>)>>,
}

impl Store {
    pub(super) fn widened(self) -> Store {
        Store {
            dest: self.dest.widened(),
            field: self.field,
            value: widen(self.value),
            run: self.run.map(|tenant| {
                tenant
                    .into_iter()
                    .map(|(at, mut path)| {
                        path.truncate(DEPTH);
                        (at, path)
                    })
                    .collect()
            }),
        }
    }
}

/// How the record a value comes from is known from the one it is stored
/// in, whose readers' facts are moved along it.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Rel {
    /// The same record.
    Same,
    /// The record this reference of the stored one points at.
    Ref(String),
    /// A record of the same tenant: the stored record's tenancy reference
    /// (the first) was set from the value's record's (the second).
    Tenant(String, String),
    /// Any other record.
    Other,
}

/// A lookup a body stores that isn't yet known to be one (chunk 10.9c):
/// the key's parts of parameters, the other parts stored where a role
/// reads them who may not read `source`, the field compared, and the
/// problem it is should its key show.
pub(super) struct Pending {
    key: Vec<(usize, Vec<String>)>,
    others: Vec<(usize, Vec<String>)>,
    source: Src,
    problem: Diagnostic,
    from: String,
    into: String,
    subject: String,
}

/// A lookup an action stores, kept in its summary for its callers
/// (chunk 10.9c): each is checked where it is called, by an action
/// ([`Walk::passed_lookup`]) or a route ([`route_lookups`]), and is a copy
/// there unless the key passed is the caller's own parameters and none of
/// it is passed again as one of `others`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Held {
    pub(super) key: Vec<(usize, Vec<String>)>,
    pub(super) others: Vec<(usize, Vec<String>)>,
    source: Src,
    pub(super) from: String,
    pub(super) into: String,
    pub(super) subject: String,
}

impl Held {
    pub(super) fn widened(mut self) -> Held {
        for (_, path) in self.key.iter_mut().chain(self.others.iter_mut()) {
            path.truncate(DEPTH);
        }
        self
    }
}

/// Whether two parts of parameters may hold the same value: the same
/// parameter, one path inside the other.
pub(super) fn overlaps(a: &(usize, Vec<String>), b: &(usize, Vec<String>)) -> bool {
    a.0 == b.0 && (a.1.starts_with(&b.1) || b.1.starts_with(&a.1))
}

/// What a lookup a body stores tells apart (chunk 10.9c).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Chose {
    /// By equality with this key, made of parameters.
    Matched(Vec<Src>),
    /// By which turn of a loop made it.
    Turn,
}

/// The part of a parameter a store's value holds: not one that only
/// decided it, as a lookup's key decides the `id` it chose.
fn stored_param(src: &Src) -> Option<(usize, Vec<String>)> {
    match src {
        Src::Param(at, path) => Some((*at, path.clone())),
        Src::Deep(inner) => stored_param(inner),
        _ => None,
    }
}

/// The parameter a store's value is part of.
fn param_of(src: &Src) -> Option<usize> {
    match src {
        Src::Param(at, _) => Some(*at),
        Src::Decided(inner) | Src::Deep(inner) => param_of(inner),
        _ => None,
    }
}

/// Who may read `entity`'s `field` from a request: each check a reader
/// passes, the record's `read:` and the field's own. `Access.system` is
/// server work, so it reads nothing a caller is shown.
fn readers(flow: &Flow<'_>, entity: &str, field: &str) -> Reached {
    let prover = &flow.prover;
    let clients = |grants: &[Grant]| -> Vec<Grant> {
        grants
            .iter()
            .filter(|grant| !matches!(grant, Grant::System))
            .cloned()
            .collect()
    };
    let mut reached =
        Reached::by(prover.way(entity, &clients(&prover.grants(entity, Effect::Read))));
    if let Some(grants) = prover
        .policies
        .entity(entity)
        .and_then(|policy| policy.field_grants(field, Effect::Read))
    {
        reached = reached.and(Reached::by(prover.way(entity, &clients(grants))));
    }
    reached
}

/// A user field of `dest`'s `read:` that its write sets, from anything
/// but the caller: set from the caller, a copy into it would be proved.
fn owner_not_caller(flow: &Flow<'_>, dest: &Dest) -> Option<String> {
    let grants = flow.prover.grants(&dest.entity, Effect::Read);
    dest.refs.iter().find_map(|(name, srcs)| {
        let names = grants.iter().any(|grant| {
            matches!(grant, Grant::Caller { field, kind: PrincipalKind::User } if field == name)
        });
        (names && !matches!(srcs.as_slice(), [Src::Caller(_)])).then(|| name.clone())
    })
}

/// The user or service fields of `dest` its write sets from the caller:
/// a reader they let in is the caller of the request that stored it.
fn caller_fields(dest: &Dest) -> Vec<(String, PrincipalKind)> {
    dest.refs
        .iter()
        .filter_map(|(name, srcs)| match srcs.as_slice() {
            [Src::Caller(kind)] => Some((name.clone(), *kind)),
            _ => None,
        })
        .collect()
}

/// `readers` of `dest`, as what is known of them on the record the
/// caller reached as `reached`, when the write set one of `dest`'s user
/// fields from the caller (chunk 10.4b): a reader that field lets in is
/// that caller, who passed every check of `reached` to read the value,
/// so each check is one way the stored value is proved. Every other
/// reader is known only to be signed in, as on any other record. A
/// request's caller is never the system, so a grant to it is no way in.
fn as_caller(dest: &Dest, readers: &Reached, reached: &Reached) -> Vec<Reached> {
    let fields = caller_fields(dest);
    if fields.is_empty() {
        return Vec::new();
    }
    let is_caller = |facts: &Facts| {
        facts.iter().any(|fact| {
            matches!(fact, Fact::Grant(Grant::Caller { field, kind })
                if fields.iter().any(|(name, of)| name == field && of == kind))
        })
    };
    if !readers.ways.iter().flatten().any(&is_caller) {
        return Vec::new();
    }
    reached
        .ways
        .iter()
        .map(|passed| {
            let passed: Way = passed
                .iter()
                .filter(|facts| !facts.contains(&Fact::Grant(Grant::System)))
                .cloned()
                .collect();
            Reached {
                ways: readers
                    .ways
                    .iter()
                    .map(|way| {
                        let mut out: Way = Vec::new();
                        for facts in way {
                            if is_caller(facts) {
                                out.extend(passed.iter().cloned());
                            } else {
                                out.push(loose(facts));
                            }
                        }
                        out
                    })
                    .collect(),
            }
        })
        .collect()
}

/// What is known of a reader of one record on a record of any other: that
/// they are signed in, or hold a role across the whole application.
fn loose(facts: &Facts) -> Facts {
    let mut out: Facts = Vec::new();
    for fact in facts {
        let kept = match fact {
            Fact::Grant(grant @ (Grant::Public | Grant::Signed | Grant::System)) => {
                Some(Fact::Grant(grant.clone()))
            }
            Fact::Grant(grant @ Grant::Role { via: None, .. }) => Some(Fact::Grant(grant.clone())),
            Fact::Grant(Grant::Caller { .. } | Grant::Role { .. }) | Fact::Member(_) => {
                Some(Fact::Grant(Grant::Signed))
            }
            Fact::Tenant | Fact::Fresh | Fact::MaybeHidden => None,
        };
        if let Some(fact) = kept
            && !out.contains(&fact)
        {
            out.push(fact);
        }
    }
    out
}

/// Why a reader of the field a value is stored in isn't proved to read it
/// (chunk 10.7c): the error says which, and the way through follows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Cause {
    /// The reader's role, or anyone signed in, isn't let in to the source
    /// field in any record: only a person approving the copy lets it
    /// through.
    Role,
    /// The reader holds a role that reads the source field, but in the
    /// stored record's tenant, which nothing shows is the source's.
    Tenant,
    /// The reader is let in by a reference of the stored record, which
    /// says nothing of who may read the source's.
    Record,
}

/// A reader who may not read a stored value: who, described, and why.
pub(super) struct Unread {
    pub(super) who: Option<String>,
    pub(super) cause: Cause,
}

/// Whether `list` lets in everyone `grant` names, whichever record each
/// is held on: the same role of the same membership, anyone signed in for
/// a role, a user or anyone signed in, or anyone at all.
fn lets_in(list: &[Grant], grant: &Grant) -> bool {
    list.iter().any(|entry| match (entry, grant) {
        (Grant::Public, _) => true,
        (Grant::Signed, Grant::Role { .. } | Grant::Caller { .. } | Grant::Signed) => true,
        (
            Grant::Role {
                membership: a,
                role: r,
                ..
            },
            Grant::Role {
                membership: b,
                role: s,
                ..
            },
        ) => a == b && r == s,
        (a, b) => a == b,
    })
}

/// The reference `entity` names its tenant by, when it belongs to one.
fn tenancy_via(flow: &Flow<'_>, entity: &str) -> Option<String> {
    flow.prover
        .policies
        .entity(entity)
        .and_then(|policy| policy.tenancy.as_ref())
        .filter(|tenancy| tenancy.via != "id")
        .map(|tenancy| tenancy.via.clone())
}

impl Walk<'_> {
    /// Whether the record of the body's [`Origin`] `origin` is the one with
    /// `id`, an entity of `entity`'s: its own `id`, or what it was found
    /// by.
    fn is_record(&self, entity: &str, origin: u32, id: &Ident) -> bool {
        *id == Ident::Field(entity.to_string(), "id".to_string(), origin)
            || self.found.get(&origin) == Some(id)
    }

    /// Whether `dest` is the record of [`Origin`] `origin`.
    fn is_dest(&self, dest: &Dest, origin: u32) -> bool {
        dest.own == Some(origin)
            || single(&dest.target).is_some_and(|id| self.is_record(&dest.entity, origin, &id))
    }

    /// How a record of `source` the body knows as `origin` is known from
    /// `dest`, the most telling first; [`Rel::Other`] always holds.
    fn relations(&self, flow: &Flow<'_>, dest: &Dest, source: &str, origin: Origin) -> Vec<Rel> {
        let mut out = Vec::new();
        let tenancy = |entity: &str| {
            flow.prover
                .policies
                .entity(entity)
                .and_then(|policy| policy.tenancy.as_ref())
                .filter(|tenancy| tenancy.via != "id")
        };
        if origin == Some(TENANT) {
            if let (Some(theirs), Some(own)) = (tenancy(&dest.entity), tenancy(source)) {
                out.push(Rel::Tenant(theirs.via.clone(), own.via.clone()));
            }
            out.push(Rel::Other);
            return out;
        }
        if let Some(origin) = origin {
            if dest.entity == source && self.is_dest(dest, origin) {
                out.push(Rel::Same);
            }
            let model = flow.entities.get(&dest.entity);
            let refers = |name: &str| {
                model.is_some_and(|model| {
                    model.fields.iter().any(|field| {
                        field.name == name
                            && field
                                .reference
                                .as_ref()
                                .is_some_and(|reference| reference.target == source)
                    })
                })
            };
            for (name, srcs) in &dest.refs {
                if refers(name)
                    && single(srcs).is_some_and(|id| self.is_record(source, origin, &id))
                {
                    out.push(Rel::Ref(name.clone()));
                }
            }
            // Found by the reference the record already holds, which this
            // write leaves as it is.
            if let Some(Ident::Field(entity, name, of)) = self.found.get(&origin)
                && *entity == dest.entity
                && refers(name)
                && dest.held.contains(name)
                && self.is_dest(dest, *of)
            {
                let rel = Rel::Ref(name.clone());
                if !out.contains(&rel) {
                    out.push(rel);
                }
            }
            if let (Some(theirs), Some(own)) = (tenancy(&dest.entity), tenancy(source))
                && (dest.refs.iter().any(|(name, srcs)| {
                    *name == theirs.via
                        && single(srcs)
                            == Some(Ident::Field(source.to_string(), own.via.clone(), origin))
                }) || self
                    .run
                    .as_ref()
                    .is_some_and(|run| run.live.contains(&origin)))
            {
                // Or a record a batch's run read, which is limited to its
                // tenant, as everything it writes is (chunk 6.4d).
                out.push(Rel::Tenant(theirs.via.clone(), own.via.clone()));
            }
        }
        out.push(Rel::Other);
        out
    }

    /// `readers` of a record of `dest`, as what is known of them on a
    /// record of `source` related to it by `rel`.
    fn moved(
        &self,
        flow: &Flow<'_>,
        readers: &Reached,
        rel: &Rel,
        dest: &str,
        source: &str,
    ) -> Reached {
        let tenant = flow.prover.policies.root().tenant.clone();
        Reached {
            ways: readers
                .ways
                .iter()
                .map(|way| {
                    way.iter()
                        .map(|facts| match rel {
                            Rel::Same => facts.clone(),
                            Rel::Ref(via) => flow.prover.along(dest, via, source, facts),
                            Rel::Tenant(theirs, own) => {
                                let mut out = loose(facts);
                                for fact in facts {
                                    let moved = match fact {
                                        Fact::Tenant => Some(Fact::Tenant),
                                        Fact::Grant(Grant::Role {
                                            membership,
                                            role,
                                            via: Some(via),
                                        }) if via == theirs
                                            && flow
                                                .prover
                                                .policies
                                                .membership(membership)
                                                .and_then(|m| m.scope_entity.as_deref())
                                                == tenant.as_deref() =>
                                        {
                                            Some(Fact::Grant(Grant::Role {
                                                membership: membership.clone(),
                                                role: role.clone(),
                                                via: Some(own.clone()),
                                            }))
                                        }
                                        _ => None,
                                    };
                                    if let Some(fact) = moved
                                        && !out.contains(&fact)
                                    {
                                        out.push(fact);
                                    }
                                }
                                out
                            }
                            Rel::Other => loose(facts),
                        })
                        .collect()
                })
                .collect(),
        }
    }

    /// The first field of `src`, a record's or one of its fields, that not
    /// everyone `readers` lets in may read, stored in `dest`: with who
    /// among them may not, described.
    fn unread_by(
        &self,
        flow: &Flow<'_>,
        dest: &Dest,
        readers: &Reached,
        src: &Src,
    ) -> Option<(String, String, Unread)> {
        let (entity, fields, origin) = match src {
            Src::Field {
                entity,
                field,
                origin,
                ..
            } => (entity, vec![field.clone()], *origin),
            Src::Record {
                entity,
                given,
                origin,
                ..
            } => (
                entity,
                flow.entities
                    .get(entity)
                    .map(|model| {
                        model
                            .fields
                            .iter()
                            .filter(|field| !given.contains(&field.name))
                            .map(|field| field.name.clone())
                            .collect()
                    })
                    .unwrap_or_default(),
                *origin,
            ),
            _ => return None,
        };
        let relations = self.relations(flow, dest, entity, origin);
        let mut moved: Vec<Reached> = relations
            .iter()
            .map(|rel| self.moved(flow, readers, rel, &dest.entity, entity))
            .collect();
        let related = moved.len();
        if let Src::Field { reached, .. } | Src::Record { reached, .. } = src {
            moved.extend(as_caller(dest, readers, reached));
        }
        let field = fields.into_iter().find(|field| {
            !moved
                .iter()
                .any(|reached| flow.prover.field_proved(entity, field, reached))
        })?;
        let unread = self.unread_reader(flow, dest, readers, &moved[..related], entity, &field);
        Some((entity.clone(), field, unread))
    }

    /// Who among `readers` of `dest` may not read `entity`'s `field`, as
    /// `moved` along each relation knows them, and why (chunk 10.7c): a
    /// reader whose role the field never lets in first, since nothing but
    /// a person's approval lets the copy through for them; then one who
    /// holds the right role in the stored record's tenant; then one a
    /// reference of the stored record lets in. Read from the narrowest
    /// check, the field's own `read:` when it has one.
    fn unread_reader(
        &self,
        flow: &Flow<'_>,
        dest: &Dest,
        readers: &Reached,
        moved: &[Reached],
        entity: &str,
        field: &str,
    ) -> Unread {
        let prover = &flow.prover;
        let mut lists = vec![prover.grants(entity, Effect::Read)];
        if let Some(own) = prover
            .policies
            .entity(entity)
            .and_then(|policy| policy.field_grants(field, Effect::Read))
        {
            lists.push(own.to_vec());
        }
        let tenancy = match (tenancy_via(flow, &dest.entity), tenancy_via(flow, entity)) {
            (Some(theirs), Some(own)) => Some(Rel::Tenant(theirs, own)),
            _ => None,
        };
        let mut found: Option<Unread> = None;
        for (way, readers_way) in readers.ways.iter().enumerate().rev() {
            for (at, facts) in readers_way.iter().enumerate() {
                let proved = moved.iter().any(|reached| {
                    reached
                        .ways
                        .get(way)
                        .and_then(|way| way.get(at))
                        .is_some_and(|facts| prover.field_readable(entity, field, facts))
                });
                if proved {
                    continue;
                }
                let Some(grant) = facts.iter().find_map(|fact| match fact {
                    Fact::Grant(grant) => Some(grant),
                    _ => None,
                }) else {
                    continue;
                };
                let cause = if matches!(grant, Grant::Role { .. } | Grant::Signed | Grant::Public)
                    && lists.iter().any(|list| !lets_in(list, grant))
                {
                    Cause::Role
                } else if let Some(rel) = &tenancy
                    && self
                        .moved(
                            flow,
                            &Reached::by(vec![facts.clone()]),
                            rel,
                            &dest.entity,
                            entity,
                        )
                        .ways
                        .iter()
                        .flatten()
                        .all(|facts| prover.field_readable(entity, field, facts))
                {
                    Cause::Tenant
                } else {
                    Cause::Record
                };
                if found.as_ref().is_none_or(|found| cause < found.cause) {
                    found = Some(Unread {
                        who: Some(prover.described(grant)),
                        cause,
                    });
                }
            }
            if found.is_some() {
                break;
            }
        }
        found.unwrap_or(Unread {
            who: None,
            cause: Cause::Record,
        })
    }

    /// Whether a person approved storing `entity`'s `from` in `dest`'s
    /// `field` (chunk 10.7c), in an acceptance naming that field flow.
    fn copy_accepted(
        &self,
        flow: &Flow<'_>,
        entity: &str,
        from: &str,
        dest: &Dest,
        field: &str,
    ) -> bool {
        let qualified = |entity: &str, field: &str| {
            format!(
                "{}.{field}",
                crate::policy::report::qualified(&flow.lookup, entity)
            )
        };
        crate::acceptances::copies::accepted(flow.db, flow.project)
            .contains(&(qualified(entity, from), qualified(&dest.entity, field)))
    }

    /// A value `prov`, given at `at`, stored in `field` of `dest`: every
    /// field it carries, or that decided it or any `reject` in the body,
    /// may be read by everyone who may read `field`. One a parameter holds is checked where the action
    /// is called; `callee` is the action it was passed to, when it was.
    pub(super) fn store(
        &mut self,
        flow: &Flow<'_>,
        dest: &Dest,
        field: &str,
        at: At<'_>,
        prov: &Prov,
        callee: Option<&str>,
    ) {
        // What reaches a model: the field keeps the marks of what it
        // stores, whoever reads it.
        if self.personal {
            self.stored_params(dest, field, prov);
            let span = match at {
                At::Site(site) => self.span(site),
                At::Span(span) => span.clone(),
            };
            self.stored_marked(flow, (&dest.entity, field), span, prov, callee);
            return;
        }
        let readers = readers(flow, &dest.entity, field);
        // A check no client passes: nobody reads what is stored.
        if readers.ways.iter().any(Vec::is_empty) {
            return;
        }
        let mut prov = prov.clone();
        for src in &self.failing {
            prov.add(src.clone());
        }
        self.stored_params(dest, field, &prov);
        let mut reported = false;
        for src in prov.flat() {
            let (inner, decided) = match shallow(src.clone()) {
                Src::Decided(inner) => (shallow(*inner), true),
                other => (other, false),
            };
            match &inner {
                Src::Field { .. } | Src::Record { .. } if self.report && !reported => {
                    let Some((entity, from, who)) = self.unread_by(flow, dest, &readers, &inner)
                    else {
                        continue;
                    };
                    if self.copy_accepted(flow, &entity, &from, dest, field) {
                        continue;
                    }
                    // A lookup isn't a copy (chunk 10.9c).
                    let chose = if decided {
                        self.chose(flow, &inner, &prov)
                    } else {
                        None
                    };
                    if matches!(chose, Some(Chose::Turn)) {
                        continue;
                    }
                    if chose.is_none()
                        && decided
                        && !self.kept.insert((entity.clone(), from.clone()))
                    {
                        continue;
                    }
                    let span = match at {
                        At::Site(site) => self.span(site),
                        At::Span(span) => span.clone(),
                    };
                    let problem = self.unstored(
                        flow, dest, field, span, &entity, &from, who, decided, callee,
                    );
                    if let Some(Chose::Matched(key)) = chose {
                        let qualified = |entity: &str, field: &str| {
                            format!(
                                "{}.{field}",
                                crate::policy::report::qualified(&flow.lookup, entity)
                            )
                        };
                        self.lookups.push(Pending {
                            key: key
                                .iter()
                                .filter_map(|part| match part {
                                    Src::Param(at, path) => Some((*at, path.clone())),
                                    _ => None,
                                })
                                .collect(),
                            others: Vec::new(),
                            source: inner.clone(),
                            problem,
                            from: format!("{entity}.{from}"),
                            into: format!("{}.{field}", dest.entity),
                            subject: crate::acceptances::copies::subject(
                                &qualified(&entity, &from),
                                &qualified(&dest.entity, field),
                            ),
                        });
                        continue;
                    }
                    reported = true;
                    self.problems.push(problem);
                }
                _ => {}
            }
        }
    }

    /// Whether `src`, a field that decided the value `prov`, only chose
    /// which record's `id` it is (chunk 10.9c, area 9 "What else shows a
    /// field"): each value it holds is an `id` of `src`'s entity, every
    /// caller may read `src`, as if it were sent them, and `src` decided
    /// it by equality with a key of parameters, or only by which turn of
    /// a loop over its records made it. Its readers then learn which
    /// existing record was chosen, which they read in the `id` anyway; a
    /// lookup is still a copy where they may know the key
    /// ([`Walk::kept_lookups`]). Only in a body walked for its own
    /// callers: an event, a batch's run and its summary have left them.
    fn chose(&self, flow: &Flow<'_>, src: &Src, prov: &Prov) -> Option<Chose> {
        let Src::Field {
            entity,
            field,
            reached,
            test,
            ..
        } = src
        else {
            return None;
        };
        if matches!(test, Test::Value) || self.event.is_some() || self.run.is_some() || self.summary
        {
            return None;
        }
        let mut values = prov
            .flat()
            .into_iter()
            .map(shallow)
            .filter(|src| !matches!(src, Src::Decided(_)))
            .peekable();
        values.peek()?;
        let ids = values.all(|value| match value {
            Src::Field {
                entity: of, field, ..
            } => of == *entity && field == "id",
            Src::Record { entity: of, .. } => of == *entity,
            _ => false,
        });
        if !ids || !flow.prover.field_proved(entity, field, reached) {
            return None;
        }
        Some(match test {
            Test::Matched(key) => Chose::Matched(key.clone()),
            _ => Chose::Turn,
        })
    }

    /// The lookups this body stores whose key it also stores where
    /// someone reads it whose role may not read the field it was compared
    /// with: they would learn which record holds what (chunk 10.9c). Each
    /// is still a copy, which only a person may approve.
    ///
    /// Every other lookup is kept for its callers ([`Held`]), with the
    /// other parts of parameters it stores where such a role reads them:
    /// a caller passing the key there too, or a key that isn't made only
    /// of its own parameters, makes it a copy where it is called.
    pub(super) fn kept_lookups(&mut self, flow: &Flow<'_>) {
        for pending in std::mem::take(&mut self.lookups) {
            let Pending {
                key,
                mut others,
                source,
                mut problem,
                from,
                into,
                subject,
            } = pending;
            // Each part of a parameter this body stores where a role reads
            // it who may not read the field compared.
            let mut shown: Vec<((usize, Vec<String>), String, String)> = Vec::new();
            for store in &self.stores {
                let Some(part) = stored_param(&store.value) else {
                    continue;
                };
                let readers = readers(flow, &store.dest.entity, &store.field);
                if self
                    .unread_by(flow, &store.dest, &readers, &source)
                    .is_some_and(|(_, _, unread)| unread.cause == Cause::Role)
                {
                    shown.push((part, store.dest.entity.clone(), store.field.clone()));
                }
            }
            let found = shown.iter().find_map(|(part, entity, field)| {
                key.iter()
                    .find(|own| overlaps(own, part))
                    .map(|own| (own.0, own.1.clone(), entity.clone(), field.clone()))
            });
            let Some((at, path, entity, field)) = found else {
                for (part, _, _) in shown {
                    if !others.contains(&part) {
                        others.push(part);
                    }
                }
                let held = Held {
                    key,
                    others,
                    source: erased(&source),
                    from,
                    into,
                    subject,
                };
                if !self.held.contains(&held) {
                    self.held.push(held);
                }
                continue;
            };
            let name = self
                .body
                .params
                .get(at)
                .map(|param| self.body.local(*param).name.clone())
                .unwrap_or_default();
            let key = std::iter::once(name)
                .chain(path)
                .collect::<Vec<_>>()
                .join(".");
            let reason = problem.reason.take().unwrap_or_default();
            problem.reason = Some(format!(
                "{reason} It decided which record by equality, but `{key}`, which it was compared with, is stored in `{entity}.{field}`, which they may read, so they learn which record holds it."
            ));
            self.problems.push(problem);
        }
    }

    /// A lookup `callee` keeps for its callers ([`Held`]), called at
    /// `site` with `provs` (chunk 10.9c): a copy unless the key passed is
    /// made only of this body's parameters, written as them (`loose` are
    /// the arguments that aren't), and none of it is passed again where
    /// `callee` stores it for the readers. Otherwise kept, with the key as
    /// this body has it, for its own stores and its callers.
    pub(super) fn passed_lookup(
        &mut self,
        site: SiteId,
        callee: &str,
        held: &Held,
        provs: &[Prov],
        loose: &BTreeSet<usize>,
    ) {
        let parts = |part: &(usize, Vec<String>)| -> Option<Vec<(usize, Vec<String>)>> {
            let given = provs.get(part.0)?.at(&part.1);
            let mut out = Vec::new();
            for src in given.flat() {
                match shallow(src) {
                    Src::Param(at, path) => {
                        if !out.contains(&(at, path.clone())) {
                            out.push((at, path));
                        }
                    }
                    Src::Decided(_) => {}
                    _ => return None,
                }
            }
            Some(out)
        };
        let mut key = Vec::new();
        let mut known = self.event.is_some() || self.run.is_some() || self.summary;
        for part in &held.key {
            match parts(part) {
                Some(given) if !loose.contains(&part.0) && !given.is_empty() => {
                    for part in given {
                        if !key.contains(&part) {
                            key.push(part);
                        }
                    }
                }
                _ => known = true,
            }
        }
        let mut others = Vec::new();
        for part in &held.others {
            if let Some(given) = parts(part) {
                for part in given {
                    if !others.contains(&part) {
                        others.push(part);
                    }
                }
            }
        }
        let again = key
            .iter()
            .any(|own| others.iter().any(|other| overlaps(own, other)));
        let problem = self.passed_lookup_problem(site, callee, held, known, again);
        if known || again {
            if self.report {
                self.problems.push(problem);
            }
            return;
        }
        self.lookups.push(Pending {
            key,
            others,
            source: held.source.clone(),
            problem,
            from: held.from.clone(),
            into: held.into.clone(),
            subject: held.subject.clone(),
        });
    }

    /// The problem with a lookup `callee` stores, passed a key at `site`
    /// that the readers of what it stores may know: `known`, when it isn't
    /// made only of this body's parameters, `again`, when it is also
    /// passed where `callee` stores it for them.
    fn passed_lookup_problem(
        &self,
        site: SiteId,
        callee: &str,
        held: &Held,
        known: bool,
        again: bool,
    ) -> Diagnostic {
        let Held {
            from,
            into,
            subject,
            ..
        } = held;
        let field = into.rsplit('.').next().unwrap_or_default();
        let why = if known {
            "this key isn't made only of its caller's parameters, so they may know it"
        } else if again {
            "the same value is passed again where it stores it for them to read"
        } else {
            "nothing here shows the key is stored nowhere they read"
        };
        Diagnostic::new(
            &rule::POLICY_OUTPUT_FIELD_UNPROVED,
            format!(
                "`{}` passes `{callee}` a key it matches against `{from}` to choose what it stores in `{into}`, which not everyone who may read `{field}` may read",
                self.name
            ),
        )
        .at(self.span(site), format!("chooses by `{from}`"))
        .reason(format!(
            "`{callee}` stores in `{into}` the `id` of the record whose `{from}` equals the key passed here. That shows someone who doesn't know the key only which record was chosen, but {why}, and with it they would learn what `{from}` holds. A key that reaches a lookup through a call is a copy unless the whole call shows it is stored nowhere they read."
        ))
        .next_for(
            format!(
                "Stop and ask your user whether everyone who may read `{into}` may learn what `{from}` holds: the decision is theirs, and a general go-ahead isn't one. Once they agree, add a file to `acceptances/` listing `{subject}: {}` with their words as its reason; otherwise pass `{callee}` only a key the request sent, and nowhere else it stores",
                crate::acceptances::copies::CHANGE
            ),
            Decider::Human,
        )
    }

    /// Each part of a parameter `prov` holds, or that decided it, stored
    /// in `field` of `dest`: checked where the action is called, with what
    /// is passed ([`Store`]).
    fn stored_params(&mut self, dest: &Dest, field: &str, prov: &Prov) {
        for src in prov.flat() {
            let inner = match shallow(src.clone()) {
                Src::Decided(inner) => shallow(*inner),
                other => other,
            };
            if !matches!(inner, Src::Param(..)) {
                continue;
            }
            let store = Store {
                dest: dest.erased(&self.found),
                field: field.to_string(),
                value: erased(&src),
                run: self.run.as_ref().map(|run| run.tenant.clone()),
            };
            if !self.stores.contains(&store) {
                self.stores.push(store);
            }
        }
    }

    /// The problem: `entity`'s `from` stored in `dest`'s `field`.
    #[allow(clippy::too_many_arguments)]
    fn unstored(
        &self,
        flow: &Flow<'_>,
        dest: &Dest,
        field: &str,
        span: jadpo_diag::Span,
        entity: &str,
        from: &str,
        unread: Unread,
        decided: bool,
        callee: Option<&str>,
    ) -> Diagnostic {
        let into = format!("`{}.{field}`", dest.entity);
        let what = if decided {
            format!("a value decided by `{entity}.{from}`")
        } else {
            format!("`{entity}.{from}`")
        };
        let headline = match callee {
            Some(callee) => format!(
                "`{}` passes {what} to `{callee}`, which stores it in {into}, though not everyone who may read `{field}` may read it",
                self.name
            ),
            None => format!(
                "`{}` stores {what} in {into}, which not everyone who may read `{field}` may read",
                self.name
            ),
        };
        let label = match (&self.event, decided) {
            (Some(_), true) => format!("an `emit` decided this by `{entity}.{from}`"),
            (Some(_), false) => format!("an `emit` filled this from `{entity}.{from}`"),
            (None, true) => format!("decided by `{entity}.{from}`"),
            (None, false) => format!("from `{entity}.{from}`"),
        };
        let source = format!("`{entity}.{from}`");
        // "only `Role.editor` may read", and "... may read it" for a
        // sentence that needs its object.
        let readers = flow.prover.readers(entity, from);
        let readers_it = if readers.ends_with("may read") {
            format!("{readers} it")
        } else {
            readers.clone()
        };
        let keeps = match &self.event {
            // What an event carries has left its record, which may change
            // before the delivery (chunk 6.1c).
            Some(_) => {
                let tenant = if flow.prover.policies.root().tenant.is_some() {
                    " of the event's tenant"
                } else {
                    ""
                };
                format!(
                    "An event carries what each `emit` of it put there away from the record it came from, so its subscriber stores it only where everyone who may read it may read `{from}` of any `{entity}`{tenant}."
                )
            }
            None if decided && self.summary => {
                let tenant = if flow.prover.policies.root().tenant.is_some() {
                    " of the run's tenant"
                } else {
                    ""
                };
                format!(
                    "A batch's summary counts the items whose action failed, which shows what decided each failure, so `finished:` stores it only where everyone who may read it may read `{from}` of any `{entity}`{tenant}."
                )
            }
            None if decided => "Which value is stored shows what decided it, so a value made under an `if`, a `match` or a loop, or kept by a filter, is stored only where everyone who may read it may read the fields they test.".to_string(),
            // The reason already says everyone who may read the field
            // reads what is stored there.
            None => String::new(),
        };
        // Passed to an action that changes a record of the same entity it
        // finds by an `id` it is given: proved when that is the `id` of
        // the record the value came from (area 9, "Field restrictions").
        if let Some(callee) = callee
            && self.event.is_none()
            && !decided
            && dest.entity == entity
            && single(&dest.target).is_some()
            && unread.cause != Cause::Role
        {
            return Diagnostic::new(&rule::POLICY_OUTPUT_FIELD_UNPROVED, headline)
                .at(span, label)
                .reason(format!(
                    "`{callee}` stores what it is given here in {into} of the `{entity}` it changes, and nothing here shows that is the `{entity}` `{from}` was read from. Another `{entity}` has other readers, and {readers_it}, so it is stored only in the record it came from."
                ))
                .next(format!(
                    "Pass `{callee}` the `id` of the `{entity}` `{from}` was read from, which proves it changes that record, or pass only what everyone who may read {into} may read"
                ));
        }
        let passed = callee
            .map(|callee| format!("`{callee}` stores what it is given here in {into}. "))
            .unwrap_or_default();
        let who = unread.who.unwrap_or_else(|| "someone".to_string());
        // Why, in the reader's terms: the role, the tenant or the record
        // (chunk 10.7c).
        let why = match unread.cause {
            Cause::Role => format!(
                "{who} may read {into} but not {source}, which {readers}. The cause is the role: no `{entity}` lets {who} read its `{from}`, whatever the record or its tenant, and everyone who may read {into} reads what is stored there, from any request."
            ),
            Cause::Tenant => format!(
                "Everyone who may read {into} reads what is stored there, from any request, and holds their role in the tenant of the `{}` it is stored in. The cause is the tenant: {who} may read {source}, but only in its own tenant, and nothing here shows the `{entity}` `{from}` is read from is in the same one.",
                dest.entity
            ),
            Cause::Record => format!(
                "Everyone who may read {into}, {who} among them, reads what is stored there, from any request. The cause is the record: nothing here shows they may read the `{entity}` `{from}` is read from, and of that `{entity}` {readers_it}."
            ),
        };
        let mut why = why;
        if let Some(first) = why.get(..1) {
            why = format!("{}{}", first.to_uppercase(), &why[1..]);
        }
        let reason = if keeps.is_empty() {
            format!("{passed}{why}")
        } else {
            format!("{passed}{why} {keeps}")
        };
        // Only a person lets a role read what its field keeps from it: an
        // acceptance naming this field flow (chunk 10.7c).
        if unread.cause == Cause::Role {
            let qualified = |entity: &str, field: &str| {
                format!(
                    "{}.{field}",
                    crate::policy::report::qualified(&flow.lookup, entity)
                )
            };
            let subject = crate::acceptances::copies::subject(
                &qualified(entity, from),
                &qualified(&dest.entity, field),
            );
            return Diagnostic::new(&rule::POLICY_OUTPUT_FIELD_UNPROVED, headline)
                .at(span, label)
                .reason(reason)
                .next_for(
                    format!(
                        "Stop and ask your user whether {who} may read what {source} holds once it is stored in {into}: the decision is theirs, and a general go-ahead isn't one. Once they agree, add a file to `acceptances/` listing `{subject}: {}` with their words as its reason; otherwise leave {source} out of {into}",
                        crate::acceptances::copies::CHANGE
                    ),
                    Decider::Human,
                );
        }
        let next = match (callee, decided) {
            (Some(callee), _) if self.event.is_some() => format!(
                "Pass the event's ids and have `{callee}` read what it stores, or give `{field}`'s `read:` only those who may read {source}"
            ),
            (Some(_), true) if self.summary => format!(
                "Decide which items fail by what everyone who may read {into} may read, or give `{field}`'s `read:` only those who may read {source}"
            ),
            (Some(callee), _) => format!(
                "Pass `{callee}` what everyone who may read {into} may read, or give `{field}`'s `read:` only those who may read {source}"
            ),
            (None, true) => {
                format!("Decide what is stored in {into} by what everyone who may read it may read")
            }
            (None, false) if unread.cause == Cause::Tenant => {
                let via = tenancy_via(flow, &dest.entity).unwrap_or_default();
                format!(
                    "Set `{}.{via}` from the `{entity}` that `{from}` is read from, so the copy stays in its tenant, or leave {source} out of {into}",
                    dest.entity
                )
            }
            // A new record a user field of whose grants the write sets
            // from somewhere else: owned by the caller, it would hold only
            // what the caller read (chunk 10.4b).
            (None, false) if self.event.is_none() && dest.target.is_empty() => {
                match owner_not_caller(flow, dest) {
                    Some(owner) => format!(
                        "Set `{owner}` from the caller, a `Principal.user` the action takes, so the copy is read by whoever read {source}, or leave {source} out of {into}"
                    ),
                    None => format!(
                        "Leave {source} out of {into}, or store it in a field only those who may read {source} may read"
                    ),
                }
            }
            (None, false) => format!(
                "Leave {source} out of {into}, or store it in a field only those who may read {source} may read"
            ),
        };
        Diagnostic::new(&rule::POLICY_OUTPUT_FIELD_UNPROVED, headline)
            .at(span, label)
            .reason(reason)
            .next(next)
    }

    /// The references of `dest` it keeps through the write at `site`,
    /// which sets `changes` itself, that nothing else this body runs may
    /// change: of `among` when given (what a called action kept), else
    /// every reference its entity has.
    pub(super) fn hold(
        &self,
        flow: &Flow<'_>,
        dest: &mut Dest,
        site: SiteId,
        changes: &[String],
        among: Option<&[String]>,
    ) {
        let all: Vec<String> = match among {
            Some(among) => among.to_vec(),
            None => flow
                .entities
                .get(&dest.entity)
                .map(|model| {
                    model
                        .fields
                        .iter()
                        .filter(|field| field.reference.is_some())
                        .map(|field| field.name.clone())
                        .collect()
                })
                .unwrap_or_default(),
        };
        dest.held = all
            .into_iter()
            .filter(|name| {
                !changes.contains(name)
                    && !dest.refs.iter().any(|(set, _)| set == name)
                    && !self.could_change(flow, &dest.entity, name, site)
            })
            .collect();
    }

    /// Whether anything this body runs but the site `except` may change
    /// `entity`'s `field`: a write of its own, or one an action it calls
    /// reaches. A called action's patch may change anything.
    fn could_change(&self, flow: &Flow<'_>, entity: &str, field: &str, except: SiteId) -> bool {
        let mut seen = BTreeSet::new();
        self.body.sites.iter().enumerate().any(|(at, site)| {
            if at as u32 == except.0 {
                return false;
            }
            match &site.node {
                Node::Expr(Expr::Write {
                    entity: key,
                    op,
                    transition,
                    set,
                    patch,
                    when_supplied,
                    ..
                }) if key.name() == entity && *op != WriteOp::Create => {
                    set.iter().any(|(key, _)| field_name(key) == field)
                        || patch.is_some_and(|patch| {
                            self.patch_fields(flow, patch).iter().any(|f| f == field)
                        })
                        || when_supplied.iter().any(|(_, fields)| {
                            fields.iter().any(|(key, _)| field_name(key) == field)
                        })
                        || transition.as_ref().is_some_and(|t| {
                            transition_sets(flow, entity, t).iter().any(|f| f == field)
                        })
                }
                Node::Expr(Expr::Call {
                    callee: Callee::Action(key),
                    ..
                }) => reaches_change(flow, key, entity, field, &mut seen),
                _ => false,
            }
        })
    }

    /// A call at `site` passing `args` to `callee`, which stores a part of
    /// one of them: checked here, with what was passed.
    pub(super) fn passed_store(
        &mut self,
        flow: &Flow<'_>,
        site: SiteId,
        callee: &str,
        store: &Store,
        args: &[Option<SiteId>],
        provs: &[Prov],
    ) {
        let Some(at) = param_of(&store.value) else {
            return;
        };
        let Some(Some(arg)) = args.get(at) else {
            return;
        };
        let mut dest = store.dest.substitute(provs);
        let kept = store.dest.held.clone();
        self.hold(flow, &mut dest, site, &[], Some(&kept));
        let mut value = Prov::of(store.value.clone()).substitute(provs);
        // A batch's run stores it later: it has left its record, but for
        // one whose tenancy reference the run's tenant id was read from,
        // which is of the tenant the run is limited to (chunk 6.4d).
        let outer = store.run.as_ref().map(|tenant| {
            let mut origins: Vec<u32> = Vec::new();
            let mut params: Vec<(usize, Vec<String>)> = Vec::new();
            for (at, path) in tenant {
                let Some(prov) = provs.get(*at) else {
                    continue;
                };
                match single(&prov.at(path).whole) {
                    Some(Ident::Param(at, path)) => params.push((at, path)),
                    Some(ident) => origins.extend(tenancy_of(flow, &ident)),
                    None => {}
                }
            }
            value = value.retold(&|src| {
                retold(src, &|origin| {
                    (origin == TENANT || origins.contains(&origin)).then_some(TENANT)
                })
            });
            self.run.replace(Run {
                live: Vec::new(),
                tenant: params,
            })
        });
        self.store(
            flow,
            &dest,
            &store.field,
            At::Site(*arg),
            &value,
            Some(callee),
        );
        if let Some(outer) = outer {
            self.run = outer;
        }
    }

    /// A subscriber passing its event's parts, `provs` (what every `emit`
    /// of it put there), at `spans` to `callee`, which stores a part of
    /// one of them: checked as a call is, with what was passed (chunk
    /// 6.1c).
    pub(super) fn event_store(
        &mut self,
        flow: &Flow<'_>,
        callee: &str,
        store: &Store,
        spans: &[jadpo_diag::Span],
        provs: &[Prov],
    ) {
        let Some(at) = param_of(&store.value) else {
            return;
        };
        let Some(span) = spans.get(at) else {
            return;
        };
        let mut dest = store.dest.substitute(provs);
        let kept = store.dest.held.clone();
        self.hold(flow, &mut dest, SiteId(0), &[], Some(&kept));
        let value = Prov::of(store.value.clone()).substitute(provs);
        self.store(
            flow,
            &dest,
            &store.field,
            At::Span(span),
            &value,
            Some(callee),
        );
    }

    /// A transition run at `site` on `dest`: each field it sets holds a
    /// fixed value, which shows what decided that it runs, and its `when:`,
    /// which tests the record it changes.
    pub(super) fn transition(
        &mut self,
        flow: &Flow<'_>,
        site: SiteId,
        dest: &Dest,
        transition: &Key,
        reached: Option<&Reached>,
    ) {
        let name = transition.name();
        let Some((_, short)) = name.split_once('.') else {
            return;
        };
        let Some(model) = flow
            .entities
            .get(&dest.entity)
            .and_then(|model| model.lifecycle.as_ref())
            .and_then(|lifecycle| lifecycle.transition(short))
        else {
            return;
        };
        let mut tested = BTreeSet::new();
        if let Some(when) = &model.when {
            when.fields(&mut tested);
        }
        let mut prov = self.with_pc(Prov::default());
        for field in tested {
            prov.add(Src::Decided(Box::new(Src::Field {
                entity: dest.entity.clone(),
                field,
                reached: reached.cloned().unwrap_or_default(),
                origin: dest.own,
                test: Test::Value,
            })));
        }
        let set: Vec<String> = model.set.iter().map(|(field, _)| field.clone()).collect();
        for field in set {
            self.store(flow, dest, &field, At::Site(site), &prov, None);
        }
    }

    /// The fields of the `Input` a patch at `patch` is given.
    pub(super) fn patch_fields(&self, flow: &Flow<'_>, patch: SiteId) -> Vec<String> {
        let mut ty = &self.body.site(patch).ty;
        while let TypeRef::Nullable(inner) = ty {
            ty = inner;
        }
        let TypeRef::Node(key) = ty else {
            return Vec::new();
        };
        flow.lookup
            .fields(key.name())
            .map(|fields| fields.iter().map(|field| field.name.clone()).collect())
            .unwrap_or_default()
    }
}

/// The fields the transition `transition` of `entity` sets.
pub(super) fn transition_sets(flow: &Flow<'_>, entity: &str, transition: &Key) -> Vec<String> {
    let Some((_, short)) = transition.name().split_once('.') else {
        return Vec::new();
    };
    flow.entities
        .get(entity)
        .and_then(|model| model.lifecycle.as_ref())
        .and_then(|lifecycle| lifecycle.transition(short))
        .map(|t| t.set.iter().map(|(field, _)| field.clone()).collect())
        .unwrap_or_default()
}

/// Whether the action `key`, or one it calls, may change `entity`'s
/// `field`; `seen` holds those already looked at.
fn reaches_change(
    flow: &Flow<'_>,
    key: &Key,
    entity: &str,
    field: &str,
    seen: &mut BTreeSet<String>,
) -> bool {
    if !seen.insert(key.name().to_string()) {
        return false;
    }
    let Some(callable) = resolve(flow.db, flow.project, key) else {
        return false;
    };
    body_facts(flow.db, flow.project, callable)
        .facts
        .iter()
        .any(|fact| match fact {
            Did::Writes(write) if write.entity == entity && write.op != WriteOp::Create => {
                write.set.iter().any(|f| f == field)
                    || match &write.transition {
                        Some(transition) => transition_sets(flow, entity, transition)
                            .iter()
                            .any(|f| f == field),
                        // Changing what a patch supplies.
                        None => write.op == WriteOp::Update && write.set.is_empty(),
                    }
            }
            Did::Action(key) => reaches_change(flow, key, entity, field, seen),
            _ => false,
        })
}
