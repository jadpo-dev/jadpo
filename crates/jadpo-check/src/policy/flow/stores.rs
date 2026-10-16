//! Restricted values stored elsewhere (area 9, "Restricted values stored
//! in another record", chunk 5.1d): a create or update stores a value in a
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

use std::collections::BTreeSet;

use jadpo_diag::{Diagnostic, rule};
use jadpo_graph::body::{Callee, Expr, Node, SiteId};
use jadpo_graph::{Key, TypeRef};
use jadpo_std::write::WriteOp;

use super::{Flow, Origin, Prov, Src, Walk, erased, field_name, shallow, widen};
use crate::policy::proof::{Fact, Facts, Reached};
use crate::policy::{Effect, Grant};
use crate::types::{Fact as Did, TypeView, body_facts, resolve};

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
/// record the body loaded.
pub(super) fn loaded(entity: &str, srcs: &[Src]) -> Origin {
    let values: Vec<&Src> = srcs
        .iter()
        .filter(|src| !matches!(src, Src::Decided(_)))
        .collect();
    match values.as_slice() {
        [
            Src::Record {
                entity: of, origin, ..
            },
        ] if of == entity => *origin,
        _ => None,
    }
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

    /// As a summary holds it: no [`Origin`].
    fn erased(&self) -> Dest {
        Dest {
            entity: self.entity.clone(),
            own: None,
            target: self.target.iter().map(erased).collect(),
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
}

impl Store {
    pub(super) fn widened(self) -> Store {
        Store {
            dest: self.dest.widened(),
            field: self.field,
            value: widen(self.value),
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
            let tenancy = |entity: &str| {
                flow.prover
                    .policies
                    .entity(entity)
                    .and_then(|policy| policy.tenancy.as_ref())
                    .filter(|tenancy| tenancy.via != "id")
            };
            if let (Some(theirs), Some(own)) = (tenancy(&dest.entity), tenancy(source))
                && dest.refs.iter().any(|(name, srcs)| {
                    *name == theirs.via
                        && single(srcs)
                            == Some(Ident::Field(source.to_string(), own.via.clone(), origin))
                })
            {
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
    ) -> Option<(String, String, Option<String>)> {
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
        let moved: Vec<Reached> = relations
            .iter()
            .map(|rel| self.moved(flow, readers, rel, &dest.entity, entity))
            .collect();
        let field = fields.into_iter().find(|field| {
            !moved
                .iter()
                .any(|reached| flow.prover.field_proved(entity, field, reached))
        })?;
        // Who may not, from the most telling relation: a reader the
        // field's own `read:` lets in, or else the record's.
        let first = &moved[0];
        let who = (0..readers.ways.len()).rev().find_map(|way| {
            let at = first.ways[way]
                .iter()
                .position(|facts| !flow.prover.field_readable(entity, &field, facts))?;
            readers.ways[way][at].iter().find_map(|fact| match fact {
                Fact::Grant(grant) => Some(flow.prover.described(grant)),
                _ => None,
            })
        });
        Some((entity.clone(), field, who))
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
        at: SiteId,
        prov: &Prov,
        callee: Option<&str>,
    ) {
        let readers = readers(flow, &dest.entity, field);
        // A check no client passes: nobody reads what is stored.
        if readers.ways.iter().any(Vec::is_empty) {
            return;
        }
        let mut prov = prov.clone();
        for src in &self.failing {
            prov.add(src.clone());
        }
        let mut reported = false;
        for src in prov.flat() {
            let (inner, decided) = match shallow(src.clone()) {
                Src::Decided(inner) => (shallow(*inner), true),
                other => (other, false),
            };
            match &inner {
                Src::Param(..) => {
                    let store = Store {
                        dest: dest.erased(),
                        field: field.to_string(),
                        value: erased(&src),
                    };
                    if !self.stores.contains(&store) {
                        self.stores.push(store);
                    }
                }
                Src::Field { .. } | Src::Record { .. } if self.report && !reported => {
                    let Some((entity, from, who)) = self.unread_by(flow, dest, &readers, &inner)
                    else {
                        continue;
                    };
                    if decided && !self.kept.insert((entity.clone(), from.clone())) {
                        continue;
                    }
                    reported = true;
                    let problem =
                        self.unstored(flow, dest, field, at, &entity, &from, who, decided, callee);
                    self.problems.push(problem);
                }
                _ => {}
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
        at: SiteId,
        entity: &str,
        from: &str,
        who: Option<String>,
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
        let label = if decided {
            format!("decided by `{entity}.{from}`")
        } else {
            format!("from `{entity}.{from}`")
        };
        let among = who
            .map(|who| format!(", {who} among them,"))
            .unwrap_or_default();
        let readers = flow.prover.readers(entity, from);
        let keeps = if decided {
            "Which value is stored shows what decided it, so a value made under an `if`, a `match` or a loop, or kept by a filter, is stored only where everyone who may read it may read the fields they test."
        } else {
            "A value stored in a field keeps its own field's rules, as one sent in an output does, so it is stored only where everyone who may read it may read the value."
        };
        let passed = callee
            .map(|callee| format!("`{callee}` stores what it is given here in {into}, and "))
            .unwrap_or_default();
        let reason = format!(
            "{passed}{}veryone who may read {into}{among} reads what is stored there, from any request, and {readers} `{from}`. {keeps}",
            if passed.is_empty() { "E" } else { "e" }
        );
        let next = match (callee, decided) {
            (Some(callee), _) => format!(
                "Pass `{callee}` what everyone who may read {into} may read, or give `{field}`'s `read:` only those who may read `{from}`"
            ),
            (None, true) => format!(
                "Decide what is stored in `{field}` by what everyone who may read it may read"
            ),
            (None, false) => format!(
                "Leave `{from}` out of `{field}`, or store it in a field only those who may read `{from}` may read"
            ),
        };
        Diagnostic::new(&rule::POLICY_OUTPUT_FIELD_UNPROVED, headline)
            .at(self.span(at), label)
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
        let value = Prov::of(store.value.clone()).substitute(provs);
        self.store(flow, &dest, &store.field, *arg, &value, Some(callee));
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
            })));
        }
        let set: Vec<String> = model.set.iter().map(|(field, _)| field.clone()).collect();
        for field in set {
            self.store(flow, dest, &field, site, &prov, None);
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
