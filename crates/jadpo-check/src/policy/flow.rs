//! Following values through actions (area 9, "Field restrictions"):
//! "passing a value through a variable or another type does not escape the
//! field's rules; the compiler tracks where it came from."
//!
//! Each action's and function's body is walked for where every value may
//! have come from ([`Prov`]): a record its body loaded, created or changed,
//! with how the caller reached it ([`Reached`]); a field of one; the caller
//! itself, a `Principal` parameter or part of one; or one of its
//! parameters, which whoever calls it passes. Two checks read it:
//!
//! - **What a caller is shown** (`policy.output_field_unproved`). Each
//!   `Output` the body builds, each failure's client field, a failure
//!   raised on a condition, and each value used to find a record (a
//!   `get`'s `id`, a query's arguments, a write's target) carry only
//!   fields every caller who reached their record may read. A parameter
//!   that reaches one is checked where the action or function is called,
//!   against what the caller passes.
//! - **What a write stores** (`policy.output_field_unproved`, chunk
//!   5.1d). Each value a create, update or transition stores may be read
//!   by everyone who may read the field it goes in (`stores`), followed
//!   to the record it came from by [`Origin`]: which record a body read,
//!   created or changed.
//! - **Creates** (`policy.role_field_create_forbidden`). A create whose
//!   entity's `create:` names only user fields sets one of them from the
//!   caller, on every path: set from the request, it could only ever
//!   succeed for the caller anyway.
//!
//! The walk follows copies (assignments, fields, objects, lists, `match`,
//! the collection operations, and what a write sets, into the record it
//! gives back) and what decides a value: whatever an `if`, a `match` or a
//! loop branched on, and a filter's predicate, is carried by every value
//! made under it ([`Src::Decided`]). A function or library call carries
//! all of its arguments into its result, and a count only what decided
//! which items it counts. A local holds everything ever assigned to it,
//! worked out until nothing changes.
//!
//! Each action and function is its own query (chunk 4.11): its summary
//! ([`flow_summary`]) holds no positions, so a caller reads it again only
//! when what flows through it changes, and each file's problems
//! ([`flow_problems`]) walk only that file's bodies. Calls that go round,
//! a function calling itself or functions calling each other, are a cycle
//! of summaries, solved by salsa's fixpoint iteration: each round starts
//! from the last, so a summary only grows, and the walk is repeated until
//! it stops changing. The summary of a call into the cycle is then
//! everything that may flow through it, however many times round, never
//! less. A part followed deeper than [`DEPTH`] fields, which only a
//! recursive type can give, is widened to everything inside it
//! ([`Src::Deep`]), so the rounds end.

use std::collections::{BTreeMap, BTreeSet};

use jadpo_db::{Db, File, Project};
use jadpo_diag::{Diagnostic, Span, rule};
use jadpo_graph::body::{ArmBody, Body, Callee, Expr, LocalId, Node, SiteId, Stmt};
use jadpo_graph::{Key, Kind, TypeRef};
use jadpo_std::Op;
use jadpo_std::batch::{BatchOp, IMPORT_SUMMARY_FIELDS};
use jadpo_std::http::PrincipalKind;
use jadpo_std::read::Cardinality;
use jadpo_std::write::WriteOp;

mod stores;

use stores::{Dest, Store};

use super::proof::{Fact, Prover, Reached, Way};
use super::{Effect, Grant, LineGate};
use crate::data::Entities;
use crate::data::queries::{Element, Include, IncludeKind, Source};
use crate::lookups::{Lookup, Name};
use crate::types::{
    Callable, Def, FieldsBase, TypeView, body_facts, callables_of, resolve, typed_body,
};

/// How many fields deep a summary in a cycle follows a part before it
/// widens it to everything inside ([`Src::Deep`]).
const DEPTH: usize = 8;

/// Where part of a value may have come from.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Src {
    /// The caller: a `Principal` value, or a part of one.
    Caller(PrincipalKind),
    /// A record of `entity`, which the caller reached as `reached`: each
    /// of its fields is that record's, except those a write just `given`,
    /// which hold what it set them to. `origin` is which record it is,
    /// when the body knows ([`Origin`]).
    Record {
        entity: String,
        reached: Reached,
        given: Vec<String>,
        origin: Origin,
    },
    /// A field of a record of `entity`.
    Field {
        entity: String,
        field: String,
        reached: Reached,
        origin: Origin,
    },
    /// The part `path` names (each field in turn; none for all of it) of
    /// the parameter at this position.
    Param(usize, Vec<String>),
    /// What decided which value this is, not the value itself: a condition
    /// an `if`, a `match` or a loop branched on, or a filter's predicate.
    /// Never a whole record: whether a record is there shows only what
    /// its reader may see.
    Decided(Box<Src>),
    /// This, anywhere inside the value: a part a summary in a cycle
    /// followed deeper than [`DEPTH`], widened to all of it. Following a
    /// field keeps it, and a record stays whole, so nothing it holds is
    /// lost. Never inside [`Src::Decided`] or itself.
    Deep(Box<Src>),
}

/// Which record a [`Src::Record`] or [`Src::Field`] is, when the body
/// knows: the site that read, created or changed it, which runs at most
/// once each time the body does, or once in each turn of the loop it is
/// in. Two sources with the same origin are the same record, so a value
/// copied from a record into a field of that record, or of one referring
/// to it, is proved against how its readers reach it (chunk 5.1d). A
/// summary holds none: another body's records are someone else's.
type Origin = Option<u32>;

/// Whether `reached` is how a status's reader holds what decides a
/// batch's failures ([`Walk::status`]): one way with a grant nothing is
/// known of, which no statement's grants give.
fn by_id(reached: &Reached) -> bool {
    reached
        .ways
        .first()
        .is_some_and(|way| way.len() == 1 && way[0].is_empty())
}

/// `src` as the caller who reached it as `reached` holds it.
fn as_held_by(src: &Src, reached: &Reached) -> Src {
    match src {
        Src::Record {
            entity,
            given,
            origin,
            ..
        } => Src::Record {
            entity: entity.clone(),
            reached: reached.clone(),
            given: given.clone(),
            origin: *origin,
        },
        Src::Field {
            entity,
            field,
            origin,
            ..
        } => Src::Field {
            entity: entity.clone(),
            field: field.clone(),
            reached: reached.clone(),
            origin: *origin,
        },
        Src::Decided(inner) => Src::Decided(Box::new(as_held_by(inner, reached))),
        Src::Deep(inner) => Src::Deep(Box::new(as_held_by(inner, reached))),
        other => other.clone(),
    }
}

/// `src` with no [`Origin`].
fn erased(src: &Src) -> Src {
    retold(src, &|_| None)
}

/// `src` with each [`Origin`] as `change` says.
fn retold(src: &Src, change: &dyn Fn(u32) -> Origin) -> Src {
    match src {
        Src::Record {
            entity,
            reached,
            given,
            origin,
        } => Src::Record {
            entity: entity.clone(),
            reached: reached.clone(),
            given: given.clone(),
            origin: origin.and_then(change),
        },
        Src::Field {
            entity,
            field,
            reached,
            origin,
        } => Src::Field {
            entity: entity.clone(),
            field: field.clone(),
            reached: reached.clone(),
            origin: origin.and_then(change),
        },
        Src::Decided(inner) => Src::Decided(Box::new(retold(inner, change))),
        Src::Deep(inner) => Src::Deep(Box::new(retold(inner, change))),
        other => other.clone(),
    }
}

/// The parameter `src` is part of, decided by or deep inside.
fn param_in(src: &Src) -> Option<usize> {
    match src {
        Src::Param(at, _) => Some(*at),
        Src::Decided(inner) | Src::Deep(inner) => param_in(inner),
        _ => None,
    }
}

/// `src` as what decided a value; nothing for a whole record.
fn decided(src: Src) -> Option<Src> {
    match src {
        Src::Record { .. } => None,
        Src::Decided(_) => Some(src),
        Src::Deep(inner) => decided(*inner).map(deep),
        other => Some(Src::Decided(Box::new(other))),
    }
}

/// `src` anywhere inside a value.
fn deep(src: Src) -> Src {
    match src {
        Src::Deep(_) => src,
        other => Src::Deep(Box::new(other)),
    }
}

/// `src` without [`Src::Deep`]: what it is, wherever it is.
fn shallow(src: Src) -> Src {
    match src {
        Src::Deep(inner) => *inner,
        other => other,
    }
}

/// `src` with a parameter's path longer than [`DEPTH`] cut there and
/// made deep.
fn widen(src: Src) -> Src {
    match src {
        Src::Param(at, path) if path.len() > DEPTH => deep(Src::Param(at, path[..DEPTH].to_vec())),
        Src::Decided(inner) => match *inner {
            Src::Param(at, path) if path.len() > DEPTH => deep(Src::Decided(Box::new(Src::Param(
                at,
                path[..DEPTH].to_vec(),
            )))),
            other => Src::Decided(Box::new(other)),
        },
        Src::Deep(inner) => deep(widen(*inner)),
        other => other,
    }
}

/// Everything in `prov` as what decided a value.
fn decide(prov: &Prov) -> Vec<Src> {
    prov.flat().into_iter().filter_map(decided).collect()
}

/// What decides whether a callable with `summary` fails, given `args`.
fn failed(summary: &Summary, args: &[Prov]) -> Prov {
    Prov {
        whole: summary.fails.clone(),
        fields: BTreeMap::new(),
    }
    .substitute(args)
}

/// Where a value may have come from: as a whole, and by field.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Prov {
    whole: Vec<Src>,
    fields: BTreeMap<String, Prov>,
}

impl Prov {
    fn of(src: Src) -> Prov {
        Prov {
            whole: vec![src],
            fields: BTreeMap::new(),
        }
    }

    fn add(&mut self, src: Src) {
        if !self.whole.contains(&src) {
            self.whole.push(src);
        }
    }

    fn union(&mut self, other: &Prov) {
        for src in &other.whole {
            self.add(src.clone());
        }
        for (name, prov) in &other.fields {
            self.fields.entry(name.clone()).or_default().union(prov);
        }
    }

    /// The field called `name`'s.
    fn field(&self, name: &str) -> Prov {
        let mut out = self.fields.get(name).cloned().unwrap_or_default();
        for src in &self.whole {
            match src {
                Src::Record {
                    entity,
                    reached,
                    given,
                    origin,
                } => {
                    if !given.iter().any(|g| g == name) {
                        out.add(Src::Field {
                            entity: entity.clone(),
                            field: name.to_string(),
                            reached: reached.clone(),
                            origin: *origin,
                        });
                    }
                }
                Src::Param(at, path) => {
                    let mut path = path.clone();
                    path.push(name.to_string());
                    out.add(Src::Param(*at, path));
                }
                other => out.add(other.clone()),
            }
        }
        out
    }

    /// The part `path` names: each field in turn.
    fn at(&self, path: &[String]) -> Prov {
        let mut out = self.clone();
        for name in path {
            out = out.field(name);
        }
        out
    }

    /// Every source of every part.
    fn flat(&self) -> Vec<Src> {
        let mut out = self.whole.clone();
        for prov in self.fields.values() {
            for src in prov.flat() {
                if !out.contains(&src) {
                    out.push(src);
                }
            }
        }
        out
    }

    /// With each parameter replaced by what `args` say was passed.
    fn substitute(&self, args: &[Prov]) -> Prov {
        let mut out = Prov::default();
        for src in &self.whole {
            match src {
                // Everything anywhere in what it stands for.
                Src::Deep(inner) => {
                    let part = Prov::of((**inner).clone()).substitute(args);
                    for src in part.flat() {
                        out.add(deep(src));
                    }
                }
                Src::Param(at, path) => {
                    if let Some(arg) = args.get(*at) {
                        out.union(&arg.at(path));
                    }
                }
                Src::Decided(inner) => match inner.as_ref() {
                    Src::Param(at, path) => {
                        let part = args.get(*at).map(|arg| arg.at(path)).unwrap_or_default();
                        for src in part.flat() {
                            if let Some(src) = decided(src) {
                                out.add(src);
                            }
                        }
                    }
                    _ => out.add(Src::Decided(inner.clone())),
                },
                other => out.add(other.clone()),
            }
        }
        for (name, prov) in &self.fields {
            out.fields
                .entry(name.clone())
                .or_default()
                .union(&prov.substitute(args));
        }
        out
    }

    /// With each source as `change` says.
    fn retold(&self, change: &dyn Fn(&Src) -> Src) -> Prov {
        let mut out = Prov::default();
        for src in &self.whole {
            out.add(change(src));
        }
        for (name, prov) in &self.fields {
            out.fields.insert(name.clone(), prov.retold(change));
        }
        out
    }

    /// Without what a parameter holds, wherever it is: an action's
    /// parameters are what its caller passes, which the caller's own walk
    /// holds, or what a request sent.
    fn unpassed(&self) -> Prov {
        let mut out = Prov::default();
        for src in &self.whole {
            if param_in(src).is_none() {
                out.add(src.clone());
            }
        }
        for (name, prov) in &self.fields {
            out.fields.insert(name.clone(), prov.unpassed());
        }
        out
    }

    /// With no [`Origin`]: what a summary holds.
    fn erased(&self) -> Prov {
        self.retold(&erased)
    }

    /// With every part deeper than [`DEPTH`] below `depth` made one deep
    /// whole, and every parameter's path cut there.
    fn widened(&self, depth: usize) -> Prov {
        let mut out = Prov::default();
        if depth >= DEPTH {
            for src in self.flat() {
                out.add(deep(widen(src)));
            }
            return out;
        }
        for src in &self.whole {
            out.add(widen(src.clone()));
        }
        for (name, prov) in &self.fields {
            out.fields.insert(name.clone(), prov.widened(depth + 1));
        }
        out
    }
}

/// What a caller of an action or function learns of it: where its result
/// may have come from, in terms of its parameters, which parts of which
/// parameters reach what a caller is shown, and which it stores in a
/// field, checked where it is called against everyone who may read that
/// field ([`Store`]).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Summary {
    returns: Prov,
    sends: BTreeSet<(usize, Vec<String>)>,
    stores: Vec<Store>,
    /// What each `emit` it runs, itself or through an action it calls,
    /// puts in its event, by the event's key: what a subscriber of it
    /// stores is checked against these (chunk 6.1c). With no [`Origin`]
    /// but [`stores::TENANT`].
    emits: Vec<(Key, Prov)>,
    /// What decides whether it fails: what decided each `reject` and
    /// `attempt` it runs, and what decides whether an action or function
    /// it calls fails. A batch's summary counts the items whose action
    /// failed, so it carries these (chunk 6.4d).
    fails: Vec<Src>,
    /// What decides the failures of each batch it starts itself, its
    /// items' and its `finished:`'s, as any record of their entities,
    /// with nothing of its parameters: what `batches.status` gives
    /// anywhere is held to these, every batch's together (chunk 6.6b).
    batches: Vec<Src>,
}

/// The summary of a callable nothing resolves to.
static NOTHING: Summary = Summary {
    returns: Prov {
        whole: Vec::new(),
        fields: BTreeMap::new(),
    },
    sends: BTreeSet::new(),
    stores: Vec::new(),
    emits: Vec::new(),
    fails: Vec::new(),
    batches: Vec::new(),
};

impl Summary {
    /// What either holds, `self`'s first, in its order.
    fn join(mut self, other: &Summary) -> Summary {
        self.returns.union(&other.returns);
        self.sends.extend(other.sends.iter().cloned());
        for store in &other.stores {
            if !self.stores.contains(store) {
                self.stores.push(store.clone());
            }
        }
        for emit in &other.emits {
            if !self.emits.contains(emit) {
                self.emits.push(emit.clone());
            }
        }
        for src in &other.fails {
            if !self.fails.contains(src) {
                self.fails.push(src.clone());
            }
        }
        for src in &other.batches {
            if !self.batches.contains(src) {
                self.batches.push(src.clone());
            }
        }
        self
    }

    /// Cut at [`DEPTH`], so a cycle's rounds end.
    fn widened(self) -> Summary {
        let mut stores: Vec<Store> = Vec::new();
        for store in self.stores {
            let store = store.widened();
            if !stores.contains(&store) {
                stores.push(store);
            }
        }
        Summary {
            returns: self.returns.widened(0),
            sends: self
                .sends
                .into_iter()
                .map(|(at, mut path)| {
                    path.truncate(DEPTH);
                    (at, path)
                })
                .collect(),
            stores,
            emits: self
                .emits
                .into_iter()
                .map(|(event, prov)| (event, prov.widened(0)))
                .fold(Vec::new(), |mut out, emit| {
                    if !out.contains(&emit) {
                        out.push(emit);
                    }
                    out
                }),
            fails: self
                .fails
                .into_iter()
                .map(widen)
                .fold(Vec::new(), |mut out, src| {
                    if !out.contains(&src) {
                        out.push(src);
                    }
                    out
                }),
            batches: self.batches,
        }
    }
}

/// What one action or function passes on to its callers, without
/// positions: its own query, so a caller is walked again only when this
/// changes. Calls that go round are a cycle, solved by fixpoint iteration:
/// it starts empty ([`summary_start`]) and each round is joined with the
/// last ([`summary_round`]) until it stops changing.
#[salsa::tracked(returns(ref), cycle_fn = summary_round, cycle_initial = summary_start)]
fn flow_summary<'db>(db: &'db dyn Db, project: Project, callable: Callable<'db>) -> Summary {
    walked(db, project, callable)
        .map(|(summary, _)| summary)
        .unwrap_or_default()
}

/// A summary in a cycle starts empty.
fn summary_start<'db>(
    _db: &'db dyn Db,
    _id: salsa::Id,
    _project: Project,
    _callable: Callable<'db>,
) -> Summary {
    Summary::default()
}

/// Each round of a cycle keeps everything the last found, so the rounds
/// only grow, and is cut at [`DEPTH`], so they end: what is left when a
/// round finds nothing new holds everything that may flow through the
/// cycle, never less.
fn summary_round<'db>(
    _db: &'db dyn Db,
    _cycle: &salsa::Cycle,
    last: &Summary,
    value: Summary,
    _project: Project,
    _callable: Callable<'db>,
) -> Summary {
    last.clone().join(&value).widened()
}

/// What decides the failures of every batch the application starts
/// (chunk 6.6b; area 12, "Chosen in chunk 6.6b"): what `batches.status`
/// gives is held to these, since where a status is read nothing says
/// which batch its `BatchRun` is. A status read inside a batch's own
/// actions goes round, solved as a summary's cycle is.
#[salsa::tracked(returns(ref), cycle_fn = failures_round, cycle_initial = failures_start)]
fn batch_failures(db: &dyn Db, project: Project) -> Vec<Src> {
    let mut out: Vec<Src> = Vec::new();
    for &file in project.files(db) {
        for callable in callables_of(db, file) {
            let facts = body_facts(db, project, callable);
            let Some(key) = facts.key_of(&[Kind::Action]) else {
                continue;
            };
            if resolve(db, project, key) != Some(callable)
                || !facts
                    .facts
                    .iter()
                    .any(|fact| matches!(fact, crate::types::Fact::Batch(_)))
            {
                continue;
            }
            for src in &flow_summary(db, project, callable).batches {
                if !out.contains(src) {
                    out.push(src.clone());
                }
            }
        }
    }
    out
}

/// Every batch's failures start unknown in a cycle.
fn failures_start(_db: &dyn Db, _id: salsa::Id, _project: Project) -> Vec<Src> {
    Vec::new()
}

/// Each round keeps what the last found, so the rounds only grow.
fn failures_round(
    _db: &dyn Db,
    _cycle: &salsa::Cycle,
    last: &Vec<Src>,
    mut value: Vec<Src>,
    _project: Project,
) -> Vec<Src> {
    for src in last {
        if !value.contains(src) {
            value.push(src.clone());
        }
    }
    value
}

/// The problems with what values carry in the actions and functions in
/// `file`.
#[salsa::tracked(returns(ref))]
pub fn flow_problems(db: &dyn Db, project: Project, file: File) -> Vec<Diagnostic> {
    // Every record's `read:` decides what a value from it may reach, so
    // the walk runs whenever policy reads cleanly, field policies or not.
    if !super::policy_clean(db, project) {
        return Vec::new();
    }
    let mut out = Vec::new();
    for callable in callables_of(db, file) {
        // Of two declarations with one name, the one a call runs.
        let Some(key) = body_facts(db, project, callable).key_of(&[Kind::Action, Kind::Function])
        else {
            continue;
        };
        if resolve(db, project, key) != Some(callable) {
            continue;
        }
        if let Some((_, problems)) = walked(db, project, callable) {
            out.extend(problems);
        }
    }
    out.dedup();
    out
}

/// What every `emit` of each event puts in it, by the event's key (area 9,
/// "Events and stores"; chunk 6.1c): each action's own `emit`s and those
/// of the actions it calls, in its terms, so what a parameter holds is
/// what each caller passes; a parameter of an action no action calls holds
/// what a request sent. A subscriber's action is passed parts of its
/// event, so what it emits from them is what the event it was delivered
/// held, followed along chains of events until nothing more is found.
#[salsa::tracked(returns(ref))]
fn payloads(db: &dyn Db, project: Project) -> BTreeMap<Key, Prov> {
    let mut out: BTreeMap<Key, Prov> = BTreeMap::new();
    for &file in project.files(db) {
        for callable in callables_of(db, file) {
            let Some(key) = body_facts(db, project, callable).key_of(&[Kind::Action]) else {
                continue;
            };
            if resolve(db, project, key) != Some(callable) {
                continue;
            }
            for (event, prov) in &flow_summary(db, project, callable).emits {
                out.entry(event.clone())
                    .or_default()
                    .union(&prov.unpassed());
            }
        }
    }
    let subscribers = crate::events::subscribers(db, project);
    // An event chain stops 16 deep (area 12, "Limits and runtime faults").
    for _ in 0..=16 {
        let mut changed = false;
        for subscriber in subscribers.iter() {
            let Some(payload) = out.get(&subscriber.event).cloned() else {
                continue;
            };
            let Some(callable) = resolve(db, project, &subscriber.run) else {
                continue;
            };
            let provs: Vec<Prov> = subscriber
                .args
                .iter()
                .map(|path| payload.at(path))
                .collect();
            for (event, prov) in &flow_summary(db, project, callable).emits {
                let prov = prov.substitute(&provs).unpassed();
                let entry = out.entry(event.clone()).or_default();
                let before = entry.clone();
                entry.union(&prov);
                changed |= *entry != before;
            }
        }
        if !changed {
            break;
        }
    }
    out
}

/// What the subscribers in `file` store from their events (area 9,
/// "Events and stores"; chunk 6.1c): what each one's action stores from
/// what it is passed, checked as a call is against what every `emit` of
/// the event put in the parts it passes.
#[salsa::tracked(returns(ref))]
pub fn subscriber_store_problems(db: &dyn Db, project: Project, file: File) -> Vec<Diagnostic> {
    let reads = crate::events::subscribers_in(db, project, file);
    if reads.iter().all(|read| read.model.is_none()) || !super::policy_clean(db, project) {
        return Vec::new();
    }
    let flow = Flow {
        db,
        project,
        prover: Prover::new(db, project),
        lookup: Lookup::new(db, project),
        entities: Entities::new(db, project),
    };
    let payloads = payloads(db, project);
    let body = Body::default();
    let mut out = Vec::new();
    for read in reads {
        let Some(model) = &read.model else { continue };
        let Some(payload) = payloads.get(&model.event) else {
            continue;
        };
        let provs: Vec<Prov> = model.args.iter().map(|path| payload.at(path)).collect();
        let mut walk = Walk::blank(&body, file.path(db).clone(), model.name.clone());
        walk.report = true;
        walk.event = Some(
            model
                .event
                .name()
                .rsplit('.')
                .next()
                .unwrap_or_default()
                .to_string(),
        );
        for store in &flow.summary(&model.run).stores {
            walk.event_store(&flow, model.run.name(), store, &read.args, &provs);
        }
        out.extend(walk.problems);
    }
    out.dedup();
    out
}

/// An action's or function's walk: its summary and its problems.
fn walked<'db>(
    db: &'db dyn Db,
    project: Project,
    callable: Callable<'db>,
) -> Option<(Summary, Vec<Diagnostic>)> {
    let checked = typed_body(db, project, callable);
    let key = checked.key.as_ref()?;
    let kind = key
        .kind()
        .filter(|kind| matches!(kind, Kind::Action | Kind::Function))?;
    let flow = Flow {
        db,
        project,
        prover: Prover::new(db, project),
        lookup: Lookup::new(db, project),
        entities: Entities::new(db, project),
    };
    Some(flow.walk(
        callable.file(db).path(db).clone(),
        key.name().to_string(),
        kind,
        &checked.body,
    ))
}

struct Flow<'a> {
    db: &'a dyn Db,
    project: Project,
    prover: Prover<'a>,
    lookup: Lookup<'a>,
    entities: Entities<'a>,
}

impl<'a> Flow<'a> {
    /// The summary of the action or function `key`.
    fn summary(&self, key: &Key) -> &'a Summary {
        match resolve(self.db, self.project, key) {
            Some(callable) => flow_summary(self.db, self.project, callable),
            None => &NOTHING,
        }
    }

    /// Walks the body of the action or function `name`, in the file at
    /// `path`.
    fn walk(
        &self,
        path: String,
        name: String,
        kind: Kind,
        body: &Body,
    ) -> (Summary, Vec<Diagnostic>) {
        let listed = kind == Kind::Action;
        let mut walk = Walk::blank(body, path, name.clone());
        walk.line = listed
            .then(|| self.prover.policies.root().action(&name).cloned())
            .flatten();
        walk.action = listed;
        if listed {
            walk.gate = super::reach::line_gate(self.db, self.project, &name)
                .and_then(|gate| gate_way(&gate));
        }
        for (at, param) in body.params.iter().enumerate() {
            let ty = &body.local(*param).ty;
            let start = match caller_kind(ty) {
                Some(kind) => Prov::of(Src::Caller(kind)),
                None => Prov::of(Src::Param(at, Vec::new())),
            };
            walk.locals[param.0 as usize] = start;
        }
        // Each local holds everything ever assigned to it: walk until
        // nothing changes, then once more to report.
        for _ in 0..=body.locals.len().max(1) {
            let before = walk.locals.clone();
            walk.returns = Prov::default();
            walk.pc.clear();
            walk.found.clear();
            walk.block(self, &body.block);
            if walk.locals == before {
                break;
            }
        }
        walk.report = true;
        walk.returns = Prov::default();
        walk.sends.clear();
        walk.stores.clear();
        walk.emits.clear();
        walk.pc.clear();
        walk.found.clear();
        walk.block(self, &body.block);
        // An action is all-or-nothing: whatever a `reject` tests decides
        // whether its events are kept.
        let failing: Vec<Src> = walk.failing.iter().map(kept_origin).collect();
        let emits = walk
            .emits
            .into_iter()
            .map(|(event, mut prov)| {
                for src in &failing {
                    prov.add(src.clone());
                }
                (event, prov)
            })
            .collect();
        let mut fails: Vec<Src> = Vec::new();
        for src in walk.fails.iter().map(erased) {
            if !fails.contains(&src) {
                fails.push(src);
            }
        }
        let summary = Summary {
            returns: walk.returns.erased(),
            sends: walk.sends,
            stores: walk.stores,
            emits,
            fails,
            batches: walk.batches,
        };
        (summary, walk.problems)
    }
}

/// The kind of caller a parameter's type is: `Principal.user`.
fn caller_kind(ty: &TypeRef) -> Option<PrincipalKind> {
    match ty {
        TypeRef::Nullable(inner) => caller_kind(inner),
        TypeRef::Std(name, args) if args.is_empty() => {
            [PrincipalKind::User, PrincipalKind::Service]
                .into_iter()
                .find(|kind| kind.type_name() == name)
        }
        _ => None,
    }
}

/// What a listed action's line checks as the action starts (area 9,
/// "Actions": anyone signed in, or roles held across the whole
/// application): every caller passed one of its entries. A line that lets
/// nobody in proves anything of its callers, since there are none.
fn gate_way(gate: &LineGate) -> Option<Way> {
    match gate {
        LineGate::Signed => Some(vec![vec![Fact::Grant(Grant::Signed)]]),
        LineGate::Roles(roles) => Some(
            roles
                .iter()
                .map(|(membership, role)| {
                    vec![Fact::Grant(Grant::Role {
                        membership: membership.clone(),
                        role: role.clone(),
                        via: None,
                    })]
                })
                .collect(),
        ),
        LineGate::Nobody => Some(Vec::new()),
        LineGate::Anyone | LineGate::Statements => None,
    }
}

/// The last part of a field's key: `budget` of `Project.budget`.
fn field_name(key: &Key) -> String {
    key.name()
        .rsplit('.')
        .next()
        .unwrap_or_default()
        .to_string()
}

struct Walk<'b> {
    body: &'b Body,
    path: String,
    name: String,
    /// Its line under root `policy { actions: }`, when listed.
    line: Option<super::ActionGrants>,
    /// What that line checks as it starts, when it depends on no record.
    gate: Option<Way>,
    locals: Vec<Prov>,
    returns: Prov,
    sends: BTreeSet<(usize, Vec<String>)>,
    /// Whether it is an action, whose map layer a route may send bare
    /// (area 7, "A map layer as output").
    action: bool,
    /// The last walk, which reports.
    report: bool,
    problems: Vec<Diagnostic>,
    /// What decided that the statements being walked run: the conditions
    /// of the `if`s and `match`es around them and the loops they are in.
    pc: Vec<Src>,
    /// While a write's values are worked out: its entity, each way the
    /// caller may have reached the record it changes, which a value
    /// reading a stored field (`Post.view_count + 1`) comes from, and
    /// which record that is.
    stored: Option<(String, Vec<Reached>, Origin)>,
    /// The fields already reported for deciding what a caller is shown:
    /// one problem for each, at the first value it decides.
    decided: BTreeSet<(String, String)>,
    /// What it stores in a field from its parameters, for its callers to
    /// check ([`Store`]).
    stores: Vec<Store>,
    /// The fields already reported for deciding a value stored: one
    /// problem for each, at the first value it decides.
    kept: BTreeSet<(String, String)>,
    /// What decided each `reject` in the body: an action is
    /// all-or-nothing, so these decide every value it stores, before them
    /// or after. Gathered in every walk, so the last walk, which reports,
    /// has them all.
    failing: Vec<Src>,
    /// How many loops (and inline functions, which run once per item) the
    /// statements being walked are in.
    looping: usize,
    /// The [`Origin`]s given inside a loop: each turn's own record, the
    /// same only within that turn, so a value carried into another turn
    /// loses it.
    turns: BTreeSet<u32>,
    /// The value each record read by `get` was found by, when that is one
    /// value known exactly: a record found by the `id` a write sets as a
    /// reference is the record it refers to.
    found: BTreeMap<u32, stores::Ident>,
    /// What each `emit` it runs puts in its event (chunk 6.1c).
    emits: Vec<(Key, Prov)>,
    /// The event a subscriber's walk stores from, by name: what it stores
    /// came from an `emit` of it (chunk 6.1c).
    event: Option<String>,
    /// What decides whether the body fails ([`Summary::fails`]): gathered
    /// in every walk, as `failing` is.
    fails: Vec<Src>,
    /// While a batch's run's calls are checked (chunk 6.4d).
    run: Option<Run>,
    /// Whether the call being checked is a batch's `finished:`, whose
    /// summary is decided by what decided its items' failures.
    summary: bool,
    /// What decides the failures of each batch the body starts
    /// ([`Summary::batches`]): gathered in every walk, as `failing` is.
    batches: Vec<Src>,
}

/// A batch's run, while its calls are checked (chunk 6.4d): it works as
/// the system limited to the tenant its saved tenant ids name.
#[derive(Clone, Debug, Default)]
struct Run {
    /// The [`Origin`]s of the records the run reads and passes on whole,
    /// `each:`'s item: the same record all through the action given it,
    /// and of the run's tenant.
    live: Vec<u32>,
    /// The parameters' parts holding a tenant's id the run saves: a value
    /// it stores from a parameter is checked where the starting action
    /// is called, with these ([`stores::Store`]).
    tenant: Vec<(usize, Vec<String>)>,
}

/// `each:`'s or `finished:`'s call: the inline function at `inline`, its
/// parameter, and the call at `site` of the action `key` with `args`.
struct RunCall {
    inline: SiteId,
    param: Option<LocalId>,
    site: SiteId,
    key: Key,
    args: Vec<Option<SiteId>>,
}

impl<'b> Walk<'b> {
    /// A walk of `body` that has found nothing yet.
    fn blank(body: &'b Body, path: String, name: String) -> Walk<'b> {
        Walk {
            locals: vec![Prov::default(); body.locals.len()],
            body,
            path,
            name,
            line: None,
            gate: None,
            returns: Prov::default(),
            sends: BTreeSet::new(),
            action: false,
            report: false,
            problems: Vec::new(),
            pc: Vec::new(),
            stored: None,
            decided: BTreeSet::new(),
            stores: Vec::new(),
            kept: BTreeSet::new(),
            failing: Vec::new(),
            looping: 0,
            turns: BTreeSet::new(),
            found: BTreeMap::new(),
            emits: Vec::new(),
            event: None,
            fails: Vec::new(),
            run: None,
            summary: false,
            batches: Vec::new(),
        }
    }

    /// An `emit` of `event` holding `prov`, or one an action it calls
    /// runs: kept for the summary, its sources from a record of the
    /// event's tenant marked [`stores::TENANT`] and every other [`Origin`]
    /// dropped, since the record may change before the delivery.
    fn emitted(&mut self, flow: &Flow<'_>, event: &Key, prov: Prov) {
        let prov = self.with_pc(prov);
        let prov = match tenant_record(flow, event, &prov) {
            Some(origin) => prov.retold(&|src| {
                retold(src, &|o| Some(if o == origin { stores::TENANT } else { o }))
            }),
            None => prov,
        };
        let prov = prov.retold(&kept_origin);
        let emit = (event.clone(), prov);
        if !self.emits.contains(&emit) {
            self.emits.push(emit);
        }
    }
}

/// `src` with no [`Origin`] but [`stores::TENANT`].
fn kept_origin(src: &Src) -> Src {
    retold(src, &|origin| (origin == stores::TENANT).then_some(origin))
}

/// The record an `emit` of `event` holding `prov` read the event's tenant
/// from: its tenant field holds that record's tenancy reference, and
/// nothing else (chunk 6.1c).
fn tenant_record(flow: &Flow<'_>, event: &Key, prov: &Prov) -> Option<u32> {
    let field = crate::events::subscribers(flow.db, flow.project)
        .iter()
        .find(|subscriber| subscriber.event == *event)
        .and_then(|subscriber| match &subscriber.tenant {
            crate::events::Tenant::Field(field) => Some(field.clone()),
            _ => None,
        })?;
    tenancy_of(flow, &stores::single(&prov.field(&field).whole)?)
}

/// The record whose tenancy reference `ident` is, when it is one: a
/// tenant's id read from it holds that record's tenant.
fn tenancy_of(flow: &Flow<'_>, ident: &stores::Ident) -> Option<u32> {
    let stores::Ident::Field(entity, via, origin) = ident else {
        return None;
    };
    flow.prover
        .policies
        .entity(entity)
        .and_then(|policy| policy.tenancy.as_ref())
        .is_some_and(|tenancy| tenancy.via == *via && tenancy.via != "id")
        .then_some(*origin)
}

/// `src` with a record it holds that the body doesn't know marked
/// [`stores::TENANT`]: one a batch's run reads, of the tenant it is
/// limited to (chunk 6.4d).
fn of_run(src: &Src) -> Src {
    match src {
        Src::Record {
            entity,
            reached,
            given,
            origin: None,
        } => Src::Record {
            entity: entity.clone(),
            reached: reached.clone(),
            given: given.clone(),
            origin: Some(stores::TENANT),
        },
        Src::Field {
            entity,
            field,
            reached,
            origin: None,
        } => Src::Field {
            entity: entity.clone(),
            field: field.clone(),
            reached: reached.clone(),
            origin: Some(stores::TENANT),
        },
        Src::Decided(inner) => Src::Decided(Box::new(of_run(inner))),
        Src::Deep(inner) => Src::Deep(Box::new(of_run(inner))),
        other => other.clone(),
    }
}

impl Walk<'_> {
    /// The first of `prov`'s sources a caller isn't proved to read, but a
    /// field already reported for deciding a value; one that only decides
    /// is reported once.
    fn first_unread(
        &mut self,
        flow: &Flow<'_>,
        prov: &Prov,
    ) -> Option<(String, String, Reached, bool)> {
        let found = unread(flow, prov)
            .into_iter()
            .find(|(entity, field, _, decided)| {
                !*decided || !self.decided.contains(&(entity.clone(), field.clone()))
            })?;
        if found.3 {
            self.decided.insert((found.0.clone(), found.1.clone()));
        }
        Some(found)
    }

    fn local(&mut self, id: Option<LocalId>, prov: &Prov) {
        if let Some(id) = id
            && let Some(held) = self.locals.get_mut(id.0 as usize)
        {
            held.union(prov);
        }
    }

    /// `prov`, with what decided that the statements being walked run.
    fn with_pc(&self, mut prov: Prov) -> Prov {
        for src in &self.pc {
            prov.add(src.clone());
        }
        prov
    }

    /// The body may fail here, decided by what decided that the
    /// statements being walked run and by `also` ([`Summary::fails`]).
    fn may_fail(&mut self, also: &[Src]) {
        let srcs: Vec<Src> = self.pc.iter().chain(also).cloned().collect();
        for src in srcs {
            let src = self.unturned_src(&src);
            if !self.fails.contains(&src) {
                self.fails.push(src);
            }
        }
    }

    /// The statements walked next run only as `prov` decides.
    fn decided_by(&mut self, prov: &Prov) {
        for src in decide(prov) {
            if !self.pc.contains(&src) {
                self.pc.push(src);
            }
        }
    }

    /// After the `if`, `match` or loop at `site`, walked with `saved` as
    /// what decided it runs: what it tested decides the statements after
    /// it too when a `return` or `reject` inside it may have left.
    fn after(&mut self, site: SiteId, saved: Vec<Src>) {
        if !self.leaves(site) {
            self.pc = saved;
        }
    }

    /// Whether a `return` or `reject` inside the statement at `site` may
    /// leave the body.
    fn leaves(&self, site: SiteId) -> bool {
        let at = self.body.site(site);
        self.body.sites.iter().any(|inner| {
            inner.start >= at.start
                && inner.end <= at.end
                && matches!(
                    inner.node,
                    Node::Stmt(Stmt::Return { .. }) | Node::Expr(Expr::Reject { .. })
                )
        })
    }

    /// The [`Origin`] of the record the site at `site` reads, creates or
    /// changes: the site itself, a turn's own inside a loop.
    fn mint(&mut self, site: SiteId) -> Origin {
        if self.looping > 0 {
            self.turns.insert(site.0);
        }
        Some(site.0)
    }

    /// `src` with no turn's own [`Origin`].
    fn unturned_src(&self, src: &Src) -> Src {
        retold(src, &|origin| {
            (!self.turns.contains(&origin)).then_some(origin)
        })
    }

    /// `prov` with no turn's own [`Origin`].
    fn unturned(&self, prov: &Prov) -> Prov {
        prov.retold(&|src| self.unturned_src(src))
    }

    /// Each item of the list `prov` the loop at `site` walks: when the
    /// list is the records one read gives, each turn's item is one of
    /// them, the same record all through the turn.
    fn item(&mut self, site: SiteId, prov: &Prov) -> Prov {
        let values: Vec<&Src> = prov
            .whole
            .iter()
            .filter(|src| !matches!(src, Src::Decided(_)))
            .collect();
        let [
            Src::Record {
                entity,
                reached,
                given,
                origin: None,
            },
        ] = values.as_slice()
        else {
            return prov.clone();
        };
        if !prov.fields.is_empty() {
            return prov.clone();
        }
        let origin = self.mint(site);
        let mut out = Prov::of(Src::Record {
            entity: entity.clone(),
            reached: reached.clone(),
            given: given.clone(),
            origin,
        });
        for src in &prov.whole {
            if matches!(src, Src::Decided(_)) {
                out.add(src.clone());
            }
        }
        out
    }

    fn span(&self, site: SiteId) -> Span {
        let at = self.body.site(site);
        Span::new(&self.path, at.start, at.end)
    }

    fn block(&mut self, flow: &Flow<'_>, sites: &[SiteId]) {
        for &site in sites {
            self.statement(flow, site);
        }
    }

    fn statement(&mut self, flow: &Flow<'_>, site: SiteId) {
        match &self.body.site(site).node {
            Node::Stmt(stmt) => match stmt {
                Stmt::Var { local, value } => {
                    if let Some(value) = value {
                        let prov = self.eval(flow, *value);
                        let prov = self.with_pc(prov);
                        self.local(*local, &prov);
                    }
                }
                // A variable changed may keep its value into the next turn
                // of a loop, where a turn's own record is another.
                Stmt::Assign { local, value } => {
                    if let Some(value) = value {
                        let prov = self.eval(flow, *value);
                        let prov = self.unturned(&self.with_pc(prov));
                        self.local(*local, &prov);
                    }
                }
                Stmt::Return { value } => {
                    if let Some(value) = value {
                        let prov = self.eval(flow, *value);
                        let prov = self.with_pc(prov);
                        if self.report
                            && self.action
                            && let Some(collection) = collection_type(&self.body.site(*value).ty)
                        {
                            self.collection(flow, &collection, *value, &prov);
                        }
                        self.returns.union(&prov);
                    }
                }
                Stmt::If {
                    arms, otherwise, ..
                } => {
                    // Reaching an arm shows its condition held and every
                    // one before it didn't.
                    let saved = self.pc.clone();
                    for arm in arms {
                        if let Some(condition) = arm.condition {
                            let prov = self.eval(flow, condition);
                            self.decided_by(&prov);
                        }
                        self.block(flow, &arm.then);
                    }
                    self.block(flow, otherwise);
                    self.after(site, saved);
                }
                Stmt::For { item, list, body } => {
                    let saved = self.pc.clone();
                    self.looping += 1;
                    if let Some(list) = list {
                        let prov = self.eval(flow, *list);
                        let each = self.item(site, &prov);
                        let each = self.with_pc(each);
                        self.local(*item, &each);
                        // The body runs once for each item.
                        let prov = self.with_pc(prov);
                        self.decided_by(&prov);
                    }
                    // A turn that may have left decides every turn after
                    // it, from the top of the body, as another item.
                    if self.leaves(site) {
                        let report = std::mem::replace(&mut self.report, false);
                        let before = self.pc.clone();
                        self.block(flow, body);
                        let left: Vec<Src> = self
                            .pc
                            .iter()
                            .filter(|src| !before.contains(src))
                            .map(|src| self.unturned_src(src))
                            .collect();
                        self.pc = before;
                        for src in left {
                            if !self.pc.contains(&src) {
                                self.pc.push(src);
                            }
                        }
                        self.report = report;
                    }
                    self.block(flow, body);
                    self.looping -= 1;
                    self.after(site, saved);
                    self.pc = self.pc.iter().map(|src| self.unturned_src(src)).collect();
                }
                Stmt::Emit { value: Some(value) } => {
                    let value = *value;
                    let prov = self.eval(flow, value);
                    if self.report
                        && let jadpo_graph::TypeRef::Node(event) = &self.body.site(value).ty
                    {
                        let event = event.clone();
                        self.emitted(flow, &event, prov);
                    }
                }
                Stmt::Emit { value } | Stmt::Assert { value, .. } | Stmt::Expr { value } => {
                    if let Some(value) = value {
                        self.eval(flow, *value);
                    }
                }
            },
            Node::Expr(_) => {
                self.eval(flow, site);
            }
        }
    }

    fn eval(&mut self, flow: &Flow<'_>, site: SiteId) -> Prov {
        let node = self.body.site(site).node.clone();
        let Node::Expr(expr) = node else {
            self.statement(flow, site);
            return Prov::default();
        };
        match expr {
            // A setting is the deployment's, never a record's (area 10).
            // A constant is fixed in the code (area 3).
            Expr::Literal(_)
            | Expr::Config { .. }
            | Expr::Constant { .. }
            | Expr::Unchecked
            | Expr::Propagate { .. } => Prov::default(),
            // A stored field an update by `id` reads is the record's.
            Expr::Stored { field } => {
                let mut out = Prov::default();
                if let Some((entity, ways, origin)) = &self.stored {
                    for reached in ways {
                        out.add(Src::Field {
                            entity: entity.clone(),
                            field: field_name(&field),
                            reached: reached.clone(),
                            origin: *origin,
                        });
                    }
                }
                out
            }
            Expr::Local(id) => self.locals.get(id.0 as usize).cloned().unwrap_or_default(),
            Expr::Field { receiver, field } => self.eval(flow, receiver).field(&field_name(&field)),
            Expr::Call { callee, args, .. } => self.call(flow, site, &callee, &args),
            Expr::Build { callee, fields, .. } => {
                let mut out = Prov::default();
                let mut given = Vec::new();
                for (key, value) in &fields {
                    let prov = self.eval(flow, *value);
                    let prov = self.with_pc(prov);
                    given.push((field_name(key), *value, prov.clone()));
                    out.fields.entry(field_name(key)).or_default().union(&prov);
                }
                if let Callee::Object(key) = &callee
                    && self.report
                    && is_output(&flow.lookup, key.name())
                {
                    for (name, value, prov) in given {
                        self.output(flow, key.name(), &name, value, &prov);
                    }
                }
                out
            }
            Expr::List { items } => {
                let mut out = Prov::default();
                for item in items {
                    out.union(&self.eval(flow, item));
                }
                self.with_pc(out)
            }
            Expr::Binary { lhs, rhs, .. } => {
                let mut out = self.eval(flow, lhs);
                out.union(&self.eval(flow, rhs));
                out
            }
            Expr::Unary { operand, .. } => self.eval(flow, operand),
            Expr::Match { subject, arms, .. } => {
                let subject = match subject {
                    Some(subject) => self.eval(flow, subject),
                    None => Prov::default(),
                };
                // Which arm runs shows what the subject holds.
                let saved = self.pc.clone();
                self.decided_by(&subject);
                let mut out = Prov::default();
                for arm in &arms {
                    let bound = self.with_pc(subject.clone());
                    self.local(arm.binding, &bound);
                    match &arm.body {
                        ArmBody::Value(value) => {
                            let prov = self.eval(flow, *value);
                            out.union(&self.with_pc(prov));
                        }
                        ArmBody::Block(sites) => {
                            self.block(flow, sites);
                            if let Some(last) = sites.last()
                                && matches!(self.body.site(*last).node, Node::Expr(_))
                            {
                                let prov = self.eval(flow, *last);
                                out.union(&self.with_pc(prov));
                            }
                        }
                        ArmBody::Empty => {}
                    }
                }
                self.after(site, saved);
                out
            }
            Expr::Attempt { inner, arms } => {
                // Whether it fails is decided by whether it runs.
                self.may_fail(&[]);
                for arm in &arms {
                    self.failure(flow, &arm.failure, &arm.fields, None);
                }
                match inner {
                    Some(inner) => self.eval(flow, inner),
                    None => Prov::default(),
                }
            }
            Expr::Reject { failure, fields } => {
                // The action is all-or-nothing: what may make it fail
                // decides everything it stores, in every turn.
                for src in self.pc.clone() {
                    let src = self.unturned_src(&src);
                    if !self.failing.contains(&src) {
                        self.failing.push(src);
                    }
                }
                self.may_fail(&[]);
                match &failure {
                    Some(failure) => self.failure(flow, failure, &fields, Some(site)),
                    None => {
                        for (_, value) in fields {
                            self.eval(flow, value);
                        }
                    }
                }
                Prov::default()
            }
            Expr::Write {
                entity,
                op,
                transition,
                target,
                set,
                patch,
                when_supplied,
                conflicts,
                ..
            } => {
                for arm in &conflicts {
                    self.failure(flow, &arm.failure, &arm.fields, None);
                }
                let entity = entity.name().to_string();
                let effect = match op {
                    WriteOp::Create => Effect::Create,
                    WriteOp::Update => Effect::Update,
                    WriteOp::Delete => Effect::Delete,
                };
                let set_names: Vec<String> = set.iter().map(|(key, _)| field_name(key)).collect();
                let at = target;
                let target = target.map(|target| self.eval(flow, target));
                // Each way the caller may have reached the record: a create's
                // is new, and a change to a record loaded earlier is proved
                // for every record it may be.
                let base = self.written(flow, &entity, effect, &set_names);
                let ways = match op {
                    WriteOp::Create => vec![fresh(base)],
                    WriteOp::Update | WriteOp::Delete => targets(&entity, base, target.as_ref()),
                };
                // Which record it is: a change to a record loaded earlier is
                // that record, and anything else is its own.
                let origin = match op {
                    WriteOp::Create => None,
                    WriteOp::Update | WriteOp::Delete => target
                        .as_ref()
                        .and_then(|target| stores::loaded(&entity, &target.whole)),
                }
                .or_else(|| self.mint(site));
                let outer = self.stored.replace((entity.clone(), ways.clone(), origin));
                let mut values: Vec<(String, SiteId, Prov)> = Vec::new();
                for (key, value) in &set {
                    let prov = self.eval(flow, *value);
                    values.push((field_name(key), *value, self.with_pc(prov)));
                }
                self.stored = outer;
                let patched = patch.map(|patch| (patch, self.eval(flow, patch)));
                let mut supplied: Vec<(String, SiteId, Prov)> = Vec::new();
                for (when, fields) in &when_supplied {
                    // Whether the request supplied it decides what is set.
                    let decided = patched
                        .as_ref()
                        .map(|(_, prov)| decide(&prov.field(&field_name(when))))
                        .unwrap_or_default();
                    for (key, value) in fields {
                        let mut prov = self.eval(flow, *value);
                        for src in &decided {
                            prov.add(src.clone());
                        }
                        supplied.push((field_name(key), *value, self.with_pc(prov)));
                    }
                }
                // What it stores, for everyone who may read each field.
                let mut dest = Dest::of(
                    flow,
                    &entity,
                    target.as_ref(),
                    origin,
                    &values,
                    patched
                        .as_ref()
                        .map(|(patch, _)| self.patch_fields(flow, *patch))
                        .unwrap_or_default(),
                    &supplied,
                );
                let changes = transition
                    .as_ref()
                    .map(|t| stores::transition_sets(flow, &entity, t))
                    .unwrap_or_default();
                self.hold(flow, &mut dest, site, &changes, None);
                for (name, value, prov) in values.iter().chain(&supplied) {
                    self.store(flow, &dest, name, stores::At::Site(*value), prov, None);
                }
                if let Some((patch, prov)) = &patched {
                    for name in self.patch_fields(flow, *patch) {
                        let field = self.with_pc(prov.field(&name));
                        self.store(flow, &dest, &name, stores::At::Site(*patch), &field, None);
                    }
                }
                if let Some(transition) = &transition {
                    self.transition(flow, site, &dest, transition, ways.first());
                }
                // Which record a change finds shows what found it.
                if self.report
                    && let (Some(at), Some(target)) = (at, &target)
                {
                    self.lookup(flow, at, target, &Finding::Changes(entity.clone()));
                }
                if let Some(transition) = &transition {
                    for reached in &ways {
                        if self.condition(flow, site, &entity, transition, reached) {
                            break;
                        }
                    }
                }
                if op == WriteOp::Create && self.report && self.line.is_none() {
                    self.create(flow, &entity, &values);
                }
                if op == WriteOp::Delete {
                    return Prov::default();
                }
                // The record it gives back holds what it set.
                let mut out = Prov::default();
                for reached in ways {
                    out.add(Src::Record {
                        entity: entity.clone(),
                        reached,
                        given: set_names.clone(),
                        origin,
                    });
                }
                for (name, _, prov) in &values {
                    out.fields.entry(name.clone()).or_default().union(prov);
                }
                out
            }
            Expr::Request { .. } => Prov::default(),
            // It runs once for each item, so what it gives keeps no
            // turn's own record.
            Expr::Inline { body, .. } => match body {
                Some(body) => {
                    self.looping += 1;
                    let prov = self.eval(flow, body);
                    self.looping -= 1;
                    self.unturned(&prov)
                }
                None => Prov::default(),
            },
        }
    }

    /// How a caller reached the record a write gives back: the grants its
    /// statement checked (the entity's, or the action's line), and each
    /// restricted field it sets.
    fn written(&self, flow: &Flow<'_>, entity: &str, effect: Effect, set: &[String]) -> Reached {
        let prover = &flow.prover;
        let grants = match (&self.line, flow.entities.get(entity)) {
            (Some(line), Some(model)) => line.on(model, prover.policies.memberships()),
            _ => prover.grants(entity, effect),
        };
        let mut reached = Reached::by(prover.way(entity, &grants));
        if let Some(policy) = prover.policies.entity(entity) {
            for name in set {
                if let Some(grants) = policy.field_grants(name, effect) {
                    reached = reached.and(Reached::by(prover.way(entity, grants)));
                }
            }
        }
        self.gated(reached)
    }

    /// A transition's `when:` tests fields of the record it changes, and
    /// refusing it shows what they hold, so every caller who may run it
    /// may read each one (area 9, "Restricted fields in conditions and
    /// orders").
    /// Whether it reported a problem.
    fn condition(
        &mut self,
        flow: &Flow<'_>,
        site: SiteId,
        entity: &str,
        transition: &Key,
        reached: &Reached,
    ) -> bool {
        if !self.report {
            return false;
        }
        let name = transition.name().to_string();
        let Some((_, short)) = name.split_once('.') else {
            return false;
        };
        let Some(when) = flow
            .entities
            .get(entity)
            .and_then(|model| model.lifecycle.as_ref())
            .and_then(|lifecycle| lifecycle.transition(short))
            .and_then(|transition| transition.when.as_ref())
        else {
            return false;
        };
        let mut fields = BTreeSet::new();
        when.fields(&mut fields);
        let Some(field) = fields
            .into_iter()
            .find(|field| !flow.prover.field_proved(entity, field, reached))
        else {
            return false;
        };
        let who = flow.prover.counterexample(reached, |facts| {
            flow.prover.field_readable(entity, &field, facts)
        });
        let readers = flow.prover.readers(entity, &field);
        let among = who
            .map(|who| format!(", {who} among them,"))
            .unwrap_or_default();
        self.problems.push(
            Diagnostic::new(
                &rule::POLICY_OUTPUT_FIELD_UNPROVED,
                format!(
                    "`{}` runs `{name}`, whose `when:` tests `{entity}.{field}`, which not every caller may read",
                    self.name
                ),
            )
            .at(self.span(site), format!("tests `{field}`"))
            .reason(format!(
                "Anyone `{}`'s grants let in may run `{name}`{among} and {readers} `{field}`. Whether its `when:` refuses the record shows what `{field}` holds, so a write's condition tests only fields every caller may read.",
                self.name
            ))
            .next(format!(
                "Run `{name}` from an action only callers who may read `{field}` reach, or give `{field}`'s `read:` everyone who may change the `{entity}`"
            )),
        );
        true
    }

    /// With what the action's line checks as it starts.
    fn gated(&self, reached: Reached) -> Reached {
        match &self.gate {
            Some(way) => reached.and(Reached::by(way.clone())),
            None => reached,
        }
    }

    fn call(
        &mut self,
        flow: &Flow<'_>,
        site: SiteId,
        callee: &Callee,
        args: &[Option<SiteId>],
    ) -> Prov {
        if matches!(callee, Callee::Batch(BatchOp::Each)) {
            return self.batch(flow, args);
        }
        // An inline function's parameter holds the list it is given.
        let first = match args.first() {
            Some(Some(first)) => self.eval(flow, *first),
            _ => Prov::default(),
        };
        let mut provs: Vec<Prov> = vec![first.clone()];
        // With two parameters (`zip_with`'s items, a map's key and value),
        // each holds all the values given before it.
        let mut given = first.clone();
        for arg in args.iter().skip(1) {
            let prov = match arg {
                Some(arg) => {
                    if let Node::Expr(Expr::Inline { params, .. }) = &self.body.site(*arg).node {
                        for param in params.clone() {
                            self.local(Some(param), &given);
                        }
                    }
                    self.eval(flow, *arg)
                }
                None => Prov::default(),
            };
            given.union(&prov);
            provs.push(prov);
        }
        let all = || {
            let mut out = Prov::default();
            for prov in &provs {
                out.union(prov);
            }
            out
        };
        match callee {
            Callee::Entity { entity, operation } if operation == "get" => {
                let entity = entity.name().to_string();
                let reached = self.gated(flow.prover.read(&entity));
                // Which record it finds shows the `id` it was given.
                if self.report
                    && let (Some(Some(arg)), Some(id)) = (args.first(), provs.first())
                {
                    self.lookup(flow, *arg, id, &Finding::Finds(entity.clone()));
                }
                // It finds its record by `id`: whether one is found shows
                // the `id`, so every caller may read it.
                if self.report
                    && !flow.prover.field_proved(&entity, "id", &reached)
                    && let Some(Some(arg)) = args.first()
                {
                    let readers = flow.prover.readers(&entity, "id");
                    self.problems.push(
                        Diagnostic::new(
                            &rule::POLICY_OUTPUT_FIELD_UNPROVED,
                            format!(
                                "`{}` finds a `{entity}` by `id`, which not every caller may read",
                                self.name
                            ),
                        )
                        .at(self.span(*arg), "tests `id`")
                        .reason(format!(
                            "Anyone `{entity}`'s `read:` lets in may run `{entity}.get`, and {readers} `id`. Whether a record is found shows its `id`, so a `get` needs an `id` every caller may read."
                        ))
                        .next(format!("Give `{entity}.id`'s `read:` everyone who may read the record")),
                    );
                }
                // The record it reads is the one with that `id`.
                let origin = self.mint(site);
                if let Some(origin) = origin {
                    match provs.first().and_then(|id| stores::single(&id.whole)) {
                        Some(by) => {
                            self.found.insert(origin, by);
                        }
                        None => {
                            self.found.remove(&origin);
                        }
                    }
                }
                Prov::of(Src::Record {
                    entity,
                    reached,
                    given: Vec::new(),
                    origin,
                })
            }
            Callee::Query(key) => {
                // What it finds shows each argument its conditions test.
                if self.report {
                    for (arg, prov) in args.iter().zip(&provs) {
                        if let Some(arg) = arg {
                            self.lookup(flow, *arg, prov, &Finding::Runs(key.name().to_string()));
                        }
                    }
                } else {
                    for prov in &provs {
                        self.params_in(prov);
                    }
                }
                self.query(flow, site, key)
            }
            Callee::Action(key) | Callee::Function(key) => {
                self.called(flow, site, key, args, &provs, true)
            }
            Callee::Caller(kind) => Prov::of(Src::Caller(*kind)),
            // How many there are shows only what decided which were kept.
            Callee::Std(Op::ListCount) => Prov {
                whole: decide(&first),
                fields: BTreeMap::new(),
            },
            // Which items pass shows what the predicate tests.
            Callee::Std(Op::ListFilter) => {
                let mut out = first;
                if let Some(predicate) = provs.get(1) {
                    for src in decide(predicate) {
                        out.add(src);
                    }
                }
                out
            }
            Callee::Std(Op::ListMap) => {
                let mut out = provs.get(1).cloned().unwrap_or_default();
                for src in decide(&first) {
                    out.add(src);
                }
                out
            }
            Callee::Batch(BatchOp::Status) => self.status(flow),
            Callee::Session(_) | Callee::Batch(_) | Callee::AdvanceClock | Callee::Unknown => {
                Prov::default()
            }
            _ => all(),
        }
    }

    /// `batches.each(from:, each:, finished:, key:)` (area 12; chunk
    /// 6.4d). Its run works later, as the system limited to the tenant its
    /// saved tenant ids name, so what its calls show their caller is shown
    /// to nobody; what they store is checked here, as a call's is. Each
    /// item is one of the records `from:` finds, the same record all
    /// through `each:`'s action and of the run's tenant. A value the run
    /// saves has left its record, as an event's has (chunk 6.1c), but for
    /// one whose tenancy reference a saved tenant id was read from, which
    /// is of the run's tenant. The summary `finished:` is given holds
    /// counts and problems only: how many items there were, which shows
    /// nothing of them, as a count of records doesn't, and which failed
    /// and why, decided by what decides whether `each:`'s action fails.
    fn batch(&mut self, flow: &Flow<'_>, args: &[Option<SiteId>]) -> Prov {
        let arg = |at: usize| args.get(at).copied().flatten();
        let page = match arg(0) {
            Some(from) => self.eval(flow, from),
            None => Prov::default(),
        };
        let each = self.run_call(arg(1));
        let finished = self.run_call(arg(2));
        // What the run saves: `from:`'s arguments (walked with its query
        // already, so again without reporting), what `each:` and
        // `finished:` capture, and `key:`.
        let mut saved: Vec<(SiteId, Prov)> = Vec::new();
        if let Some(from) = arg(0)
            && let Node::Expr(Expr::Call {
                callee: Callee::Query(_),
                args: given,
                ..
            }) = self.body.site(from).node.clone()
        {
            let report = std::mem::replace(&mut self.report, false);
            for at in given.into_iter().flatten() {
                let prov = self.eval(flow, at);
                saved.push((at, prov));
            }
            self.report = report;
        }
        let mut captured: BTreeMap<u32, Prov> = BTreeMap::new();
        for call in each.iter().chain(&finished) {
            for at in call.args.iter().flatten() {
                if call.param.is_some_and(|param| self.of_param(*at, param)) {
                    continue;
                }
                let prov = self.eval(flow, *at);
                captured.insert(at.0, prov.clone());
                saved.push((*at, prov));
            }
        }
        if let Some(key) = arg(3) {
            let prov = self.eval(flow, key);
            saved.push((key, prov));
        }
        // The run's tenant, from each saved value of the tenant's `id`
        // type: the record it was read from, or the parameter's part it
        // is, for the starting action's callers.
        let mut origins: Vec<u32> = Vec::new();
        let mut tenant: Vec<(usize, Vec<String>)> = Vec::new();
        if let Some(tenant_id) = crate::batches::tenant_id(flow.db, flow.project) {
            for (at, prov) in &saved {
                let ty = &self.body.site(*at).ty;
                if !crate::batches::holds_tenant_id(&flow.lookup, ty, &tenant_id) {
                    continue;
                }
                match stores::single(&prov.whole) {
                    Some(stores::Ident::Param(at, path)) => tenant.push((at, path)),
                    Some(ident) => origins.extend(tenancy_of(flow, &ident)),
                    None => {}
                }
            }
        }
        let left = |src: &Src| {
            retold(src, &|origin| {
                (origin == stores::TENANT || origins.contains(&origin)).then_some(stores::TENANT)
            })
        };
        for prov in captured.values_mut() {
            *prov = prov.retold(&left);
        }
        // Each item, one turn's own record.
        self.looping += 1;
        let items = page.field("items");
        let item = match &each {
            Some(call) => self.item(call.inline, &items),
            None => items,
        };
        let item = item.retold(&of_run);
        let live: Vec<u32> = item
            .whole
            .iter()
            .filter_map(|src| match src {
                Src::Record {
                    origin: Some(origin),
                    ..
                } if *origin != stores::TENANT => Some(*origin),
                _ => None,
            })
            .collect();
        // Once the item's turn is over, it has left its record.
        let gone = |src: &Src| {
            retold(src, &|origin| {
                Some(if live.contains(&origin) {
                    stores::TENANT
                } else {
                    origin
                })
            })
        };
        let mut fails: Vec<Src> = Vec::new();
        if let Some(call) = &each {
            let provs = self.run_args(flow, call, &item, &captured);
            let outer = self.run.replace(Run {
                live: live.clone(),
                tenant: tenant.clone(),
            });
            self.called(flow, call.site, &call.key, &call.args, &provs, false);
            self.run = outer;
            let summary = flow.summary(&call.key);
            let inner = Prov {
                whole: summary.fails.iter().map(of_run).collect(),
                fields: BTreeMap::new(),
            };
            fails = decide(&inner.substitute(&provs)).iter().map(gone).collect();
        }
        self.looping -= 1;
        // What decided that the run starts decides it all, read as the
        // run saves it.
        let counted = self
            .with_pc(Prov {
                whole: decide(&item).iter().map(gone).collect(),
                fields: BTreeMap::new(),
            })
            .retold(&left);
        let mut failed = counted.clone();
        for src in fails {
            failed.add(src);
        }
        self.batch_failed(&failed.whole);
        if let Some(call) = &finished {
            let mut summary = Prov::default();
            for (name, _) in IMPORT_SUMMARY_FIELDS {
                let prov = if name == "items" { &counted } else { &failed };
                summary.fields.insert(name.to_string(), prov.clone());
            }
            let provs = self.run_args(flow, call, &summary, &captured);
            let outer = self.run.replace(Run {
                live: Vec::new(),
                tenant,
            });
            let was = std::mem::replace(&mut self.summary, true);
            self.called(flow, call.site, &call.key, &call.args, &provs, false);
            self.summary = was;
            self.run = outer;
            // Whether `finished:`'s action fails decides the run's state.
            let finishing = decide(&self::failed(flow.summary(&call.key), &provs));
            self.batch_failed(&finishing);
        }
        Prov::default()
    }

    /// `srcs` among what decides the failures of a batch the body starts
    /// ([`Summary::batches`]): as any record of their entities, since a
    /// status is read where nothing says which run it is, and without
    /// the body's parameters or its caller, which aren't the reader's.
    fn batch_failed(&mut self, srcs: &[Src]) {
        for src in srcs {
            let held = match shallow(src.clone()) {
                Src::Decided(inner) => shallow(*inner),
                other => other,
            };
            if !matches!(held, Src::Field { .. } | Src::Record { .. }) {
                continue;
            }
            let src = erased(src);
            if !self.batches.contains(&src) {
                self.batches.push(src);
            }
        }
    }

    /// What `batches.status` gives (chunk 6.6b): held to what decides the
    /// failures of every batch the application starts, each as a caller
    /// of whom nothing is known but what the action's line checks as it
    /// starts would read it, since the run is found by its `id` alone.
    fn status(&self, flow: &Flow<'_>) -> Prov {
        let known = self.gated(Reached::by(vec![Vec::new()]));
        let mut out = Prov::default();
        for src in batch_failures(flow.db, flow.project) {
            let src = as_held_by(src, &known);
            out.add(decided(src.clone()).unwrap_or(src));
        }
        out
    }

    /// `each:`'s or `finished:`'s inline function at `at`: the action it
    /// calls, with what.
    fn run_call(&self, at: Option<SiteId>) -> Option<RunCall> {
        let inline = at?;
        let Node::Expr(Expr::Inline {
            params,
            body: Some(call),
        }) = &self.body.site(inline).node
        else {
            return None;
        };
        let Node::Expr(Expr::Call {
            callee: Callee::Action(key) | Callee::Function(key),
            args,
            ..
        }) = &self.body.site(*call).node
        else {
            return None;
        };
        Some(RunCall {
            inline,
            param: params.first().copied(),
            site: *call,
            key: key.clone(),
            args: args.clone(),
        })
    }

    /// Whether the value at `site` is `param`, or a field inside it.
    fn of_param(&self, site: SiteId, param: LocalId) -> bool {
        match &self.body.site(site).node {
            Node::Expr(Expr::Local(local)) => *local == param,
            Node::Expr(Expr::Field { receiver, .. }) => self.of_param(*receiver, param),
            _ => false,
        }
    }

    /// What a batch's `call` is passed: the item or the summary, `given`,
    /// or a field inside it, and what it saved, `captured`.
    fn run_args(
        &mut self,
        flow: &Flow<'_>,
        call: &RunCall,
        given: &Prov,
        captured: &BTreeMap<u32, Prov>,
    ) -> Vec<Prov> {
        self.local(call.param, given);
        call.args
            .iter()
            .map(|arg| match arg {
                Some(at) => match captured.get(&at.0) {
                    Some(prov) => prov.clone(),
                    None => self.eval(flow, *at),
                },
                None => Prov::default(),
            })
            .collect()
    }

    /// A call at `site` of the action or function `key`, passed `provs`
    /// at `args`: what it stores and emits from them, in the caller's
    /// terms, and, when it runs `here` rather than in a batch's run
    /// later, what it shows its caller and whether it fails; gives its
    /// result.
    fn called(
        &mut self,
        flow: &Flow<'_>,
        site: SiteId,
        key: &Key,
        args: &[Option<SiteId>],
        provs: &[Prov],
        here: bool,
    ) -> Prov {
        let summary = flow.summary(key);
        if here && self.report {
            // One problem per argument, at its first part sent.
            let mut reported = BTreeSet::new();
            for (at, path) in &summary.sends {
                let Some(Some(arg)) = args.get(*at) else {
                    continue;
                };
                let prov = provs.get(*at).map(|p| p.at(path)).unwrap_or_default();
                if reported.contains(at) {
                    self.params_in(&prov);
                } else if self.passed(flow, key.name(), *arg, &prov) {
                    reported.insert(*at);
                }
            }
        } else if here {
            for (at, path) in &summary.sends {
                if let Some(prov) = provs.get(*at) {
                    self.params_in(&prov.at(path));
                }
            }
        }
        for store in &summary.stores {
            self.passed_store(flow, site, key.name(), store, args, provs);
        }
        if self.report {
            for (event, prov) in &summary.emits {
                let prov = prov.substitute(provs);
                self.emitted(flow, event, prov);
            }
        }
        if here && !summary.fails.is_empty() {
            let fails = decide(&failed(summary, provs));
            self.may_fail(&fails);
        }
        summary.returns.substitute(provs)
    }

    /// What a query's result carries: an `Output` was proved where the
    /// query is declared; records and other objects are their entity's
    /// fields, read through its `read:`.
    fn query(&mut self, flow: &Flow<'_>, site: SiteId, key: &Key) -> Prov {
        let Some(model) = crate::data::queries::query_model(
            flow.db,
            flow.project,
            Name::new(flow.db, key.name().to_string()),
        ) else {
            return Prov::default();
        };
        // A map layer is proved where it is declared, as an `Output` is
        // (chunk 5.5u8).
        if model.geometry.is_some() || is_output(&flow.lookup, &model.element.ty) {
            return Prov::default();
        }
        let reached = self.gated(flow.prover.read(&model.entity));
        // One record is the one this read gives; a list's are many.
        let origin = match model.cardinality {
            Cardinality::One | Cardinality::Optional => self.mint(site),
            Cardinality::List | Cardinality::Page => None,
        };
        let element = element(
            flow,
            &model.entity,
            &model.element,
            &model.includes,
            &reached,
            origin,
            &|reached| self.gated(reached),
        );
        if model.cardinality == Cardinality::Page {
            let mut page = Prov::default();
            page.fields.insert("items".to_string(), element);
            return page;
        }
        element
    }

    /// Every parameter among `prov`'s sources: a caller is shown it.
    fn params_in(&mut self, prov: &Prov) {
        for src in prov.flat() {
            match shallow(src) {
                Src::Param(at, path) => {
                    self.sends.insert((at, path));
                }
                Src::Decided(inner) => {
                    if let Src::Param(at, path) = *inner {
                        self.sends.insert((at, path));
                    }
                }
                _ => {}
            }
        }
    }

    /// A failure given `fields`: its client fields are sent to the caller,
    /// as an output's are. Raised by a `reject` at `raised` with none, it
    /// shows the caller what decided it is raised.
    fn failure(
        &mut self,
        flow: &Flow<'_>,
        failure: &Key,
        fields: &[(Key, SiteId)],
        raised: Option<SiteId>,
    ) {
        let mut client = false;
        for (key, value) in fields {
            let prov = self.eval(flow, *value);
            let prov = self.with_pc(prov);
            if client_field(flow, key) {
                client = true;
                if self.report {
                    self.output(flow, failure.name(), &field_name(key), *value, &prov);
                } else {
                    self.params_in(&prov);
                }
            }
        }
        let Some(site) = raised.filter(|_| !client) else {
            return;
        };
        let decided = self.with_pc(Prov::default());
        self.params_in(&decided);
        if !self.report {
            return;
        }
        let Some((entity, from, reached, _)) = self.first_unread(flow, &decided) else {
            return;
        };
        let among = among(flow, &entity, &from, &reached);
        let readers = flow.prover.readers(&entity, &from);
        let raises = failure.name();
        self.problems.push(
            Diagnostic::new(
                &rule::POLICY_OUTPUT_FIELD_UNPROVED,
                format!(
                    "`{}` raises `{raises}` on a condition that tests `{entity}.{from}`, which not every caller may read",
                    self.name
                ),
            )
            .at(self.span(site), format!("decided by `{entity}.{from}`"))
            .reason(format!(
                "`{}` reaches the `{entity}` it comes from for anyone its grants let in{among} and {readers} `{from}`. Whether `{raises}` is raised shows what the condition tested, so a failure is raised only on what every caller may read.",
                self.name
            ))
            .next(format!(
                "Test a field every caller may read, or raise `{raises}` only where callers who may read `{from}` reach"
            )),
        );
    }

    /// A value used to find a record, `prov`, given at `value`: which
    /// record is found shows what it holds, so it is sent like an output
    /// (area 9, "Copies"). A whole record finds by its `id`.
    fn lookup(&mut self, flow: &Flow<'_>, value: SiteId, prov: &Prov, finding: &Finding) {
        let mut by = Prov::default();
        for src in prov.flat() {
            match src {
                Src::Record {
                    entity,
                    reached,
                    origin,
                    ..
                } => by.add(Src::Field {
                    entity,
                    field: "id".to_string(),
                    reached,
                    origin,
                }),
                other => by.add(other),
            }
        }
        self.params_in(&by);
        let Some((entity, from, reached, _)) = self.first_unread(flow, &by) else {
            return;
        };
        let among = among(flow, &entity, &from, &reached);
        let readers = flow.prover.readers(&entity, &from);
        let (headline, shows) = match finding {
            Finding::Finds(target) => (
                format!("finds a `{target}` by `{entity}.{from}`"),
                format!("Whether a `{target}` is found by it, and what it holds,"),
            ),
            Finding::Runs(query) => (
                format!("runs `{query}` with `{entity}.{from}`"),
                format!("What `{query}` finds with it"),
            ),
            Finding::Changes(target) => (
                format!("changes a `{target}` found by `{entity}.{from}`"),
                format!("Whether a `{target}` is found by it"),
            ),
        };
        self.problems.push(
            Diagnostic::new(
                &rule::POLICY_OUTPUT_FIELD_UNPROVED,
                format!(
                    "`{}` {headline}, which not every caller may read",
                    self.name
                ),
            )
            .at(self.span(value), format!("from `{entity}.{from}`"))
            .reason(format!(
                "`{}` reaches the `{entity}` it comes from for anyone its grants let in{among} and {readers} `{from}`. {shows} shows what `{from}` holds, so a value used to find a record keeps its field's rules, as one sent in an output does.",
                self.name
            ))
            .next(format!("Find it by a value every caller of `{}` may read", self.name)),
        );
    }

    /// An `Output`'s or a failure's client field `field` built from
    /// `value`.
    fn output(&mut self, flow: &Flow<'_>, output: &str, field: &str, value: SiteId, prov: &Prov) {
        self.params_in(prov);
        let Some((entity, from, reached, decided)) = self.first_unread(flow, prov) else {
            return;
        };
        let among = among(flow, &entity, &from, &reached);
        let readers = flow.prover.readers(&entity, &from);
        let (label, keeps, next) = if decided {
            (
                format!("decided by `{entity}.{from}`"),
                "Which value is sent shows what decided it, so a value made under an `if`, a `match` or a loop, or kept by a filter, keeps the rules of the fields they test.".to_string(),
                format!("Decide `{field}` by what every caller of `{}` may read", self.name),
            )
        } else {
            (
                format!("from `{entity}.{from}`"),
                "What a caller is sent has one shape for everyone, and a value passed through a variable or another type keeps its field's rules.".to_string(),
                format!("Leave `{field}` out of `{output}`"),
            )
        };
        let next = if by_id(&reached) {
            format!(
                "Let only those who may read `{from}` call `{}`, through its line under `actions:`, or decide which items fail by what its callers may read",
                self.name
            )
        } else {
            next
        };
        let reason = if by_id(&reached) {
            format!(
                "`batches.status` finds a run by its `id` alone, so what it gives shows what decides the failures of every batch the application starts, `{entity}.{from}` among them, to anyone who may call `{}`, and {readers} `{from}`.",
                self.name
            )
        } else {
            format!(
                "`{}` reaches the `{entity}` it comes from for anyone its grants let in{among} and {readers} `{from}`. {keeps}",
                self.name
            )
        };
        self.problems.push(
            Diagnostic::new(
                &rule::POLICY_OUTPUT_FIELD_UNPROVED,
                format!(
                    "`{output}.{field}` carries `{entity}.{from}`, which not every caller of `{}` may read",
                    self.name
                ),
            )
            .at(self.span(value), label)
            .reason(reason)
            .next(next),
        );
    }

    /// A feature collection `collection` an action returns from `value`, which a route
    /// sends bare (area 7, "A map layer as output", chunk 5.5u7): what its
    /// caller is shown, as an `Output` is.
    fn collection(&mut self, flow: &Flow<'_>, collection: &str, value: SiteId, prov: &Prov) {
        self.params_in(prov);
        let Some((entity, from, reached, decided)) = self.first_unread(flow, prov) else {
            return;
        };
        let among = among(flow, &entity, &from, &reached);
        let readers = flow.prover.readers(&entity, &from);
        let (label, keeps) = if decided {
            (
                format!("decided by `{entity}.{from}`"),
                "Which features are sent shows what decided them, so a collection made under an `if`, a `match` or a loop, or kept by a filter, keeps the rules of the fields they test.",
            )
        } else {
            (
                format!("from `{entity}.{from}`"),
                "A route sends the feature collection an action returns as it is, so its properties have one shape for everyone, as an `Output` has.",
            )
        };
        self.problems.push(
            Diagnostic::new(
                &rule::POLICY_OUTPUT_FIELD_UNPROVED,
                format!(
                    "The `{collection}` `{}` returns carries `{entity}.{from}`, which not every caller may read",
                    self.name
                ),
            )
            .at(self.span(value), label)
            .reason(format!(
                "`{}` reaches the `{entity}` it comes from for anyone its grants let in{among} and {readers} `{from}`. {keeps}",
                self.name
            ))
            .next(format!(
                "Leave `{entity}.{from}` out of the features' properties, or decide them by what every caller of `{}` may read",
                self.name
            )),
        );
    }

    /// A call passing `value` to the action `callee`, whose parameter
    /// reaches an output it builds.
    /// Whether it reported a problem.
    fn passed(&mut self, flow: &Flow<'_>, callee: &str, value: SiteId, prov: &Prov) -> bool {
        self.params_in(prov);
        let Some((entity, from, reached)) = unproved(flow, prov).into_iter().next() else {
            return false;
        };
        let among = among(flow, &entity, &from, &reached);
        let readers = flow.prover.readers(&entity, &from);
        let reason = format!(
            "`{callee}` puts what it is given here in the output it returns, and `{}` reaches the `{entity}` it comes from for anyone its grants let in{among} while {readers} `{from}`. A value passed to another action keeps its field's rules.",
            self.name
        );
        self.problems.push(
            Diagnostic::new(
                &rule::POLICY_OUTPUT_FIELD_UNPROVED,
                format!(
                    "`{}` passes `{entity}.{from}` to `{callee}`, which sends it, though not every caller may read it",
                    self.name
                ),
            )
            .at(self.span(value), format!("from `{entity}.{from}`"))
            .reason(reason)
            .next(format!("Pass `{callee}` what every caller may read, or leave `{from}` out of its output")),
        );
        true
    }

    /// `policy.role_field_create_forbidden`: a create whose entity's
    /// `create:` names only user fields, none of them set from the caller.
    fn create(&mut self, flow: &Flow<'_>, entity: &str, values: &[(String, SiteId, Prov)]) {
        let grants = flow.prover.grants(entity, Effect::Create);
        let callers: Vec<(&String, PrincipalKind)> = grants
            .iter()
            .filter_map(|grant| match grant {
                Grant::Caller { field, kind } => Some((field, *kind)),
                _ => None,
            })
            .collect();
        if callers.is_empty() || callers.len() != grants.len() {
            return;
        }
        let set: Vec<&(String, SiteId, Prov)> = values
            .iter()
            .filter(|(name, _, _)| callers.iter().any(|(field, _)| *field == name))
            .collect();
        if set.is_empty() {
            return;
        }
        // Every value it may be set from is the caller, whatever decided
        // which.
        let from_caller = set.iter().any(|(name, _, prov)| {
            let kind = callers
                .iter()
                .find(|(field, _)| *field == name)
                .map(|(_, kind)| *kind);
            let values: Vec<Src> = prov
                .flat()
                .into_iter()
                .filter(|src| !matches!(src, Src::Decided(_)))
                .collect();
            !values.is_empty()
                && values
                    .iter()
                    .all(|src| matches!(src, Src::Caller(k) if Some(*k) == kind))
        });
        if from_caller {
            return;
        }
        let (field, value, _) = set[0];
        let kind = callers
            .iter()
            .find(|(f, _)| *f == field)
            .map_or(PrincipalKind::User, |(_, kind)| *kind);
        let listed: Vec<String> = callers.iter().map(|(f, _)| format!("`{f}`")).collect();
        let only = if listed.len() == 1 {
            "only "
        } else {
            "only the user fields "
        };
        self.problems.push(
            Diagnostic::new(
                &rule::POLICY_ROLE_FIELD_CREATE_FORBIDDEN,
                format!(
                    "`{}` sets `{entity}.{field}` from the request, not the caller",
                    self.name
                ),
            )
            .at(self.span(*value), "not the caller")
            .reason(format!(
                "`{entity}`'s `create:` names {only}{}, so a `{entity}` is only ever created by the caller it points at. Set from the request, the create would be refused for every value but the caller's own, so it is set from the caller: the `id` of a `{}` the action takes.",
                crate::and_list(&listed),
                kind.type_name()
            ))
            .next(format!(
                "Take the caller as a parameter typed `{}`, and set `{field}` to its `id`",
                kind.type_name()
            )),
        );
    }
}

/// What a value is used to find.
enum Finding {
    /// A `get`'s record, of this entity.
    Finds(String),
    /// What this query reads.
    Runs(String),
    /// The record an update or delete of this entity changes.
    Changes(String),
}

/// A create's way, marked as the caller's own new record.
fn fresh(mut reached: Reached) -> Reached {
    for way in &mut reached.ways {
        for facts in way.iter_mut() {
            if !facts.contains(&Fact::Fresh) {
                facts.push(Fact::Fresh);
            }
        }
    }
    reached
}

/// Each way a caller may have reached the record an update or delete of
/// `entity` changes, which `base` (its own statement's grants) reaches:
/// with each record of `entity` its target may be, and alone when it may
/// be found by `id` or passed in.
fn targets(entity: &str, base: Reached, target: Option<&Prov>) -> Vec<Reached> {
    let mut out: Vec<Reached> = Vec::new();
    let mut alone = target.is_none_or(|target| target.whole.is_empty());
    for src in target
        .map(|target| target.whole.as_slice())
        .unwrap_or_default()
    {
        match src {
            Src::Record {
                entity: loaded,
                reached,
                ..
            } if loaded == entity => {
                let both = base.clone().and(reached.clone());
                if !out.contains(&both) {
                    out.push(both);
                }
            }
            Src::Decided(_) => {}
            _ => alone = true,
        }
    }
    if alone || out.is_empty() {
        out.push(base);
    }
    out
}

/// What a query's element of `entity` carries, read as `reached`: its
/// records whole, or each field the element picks, and each include's
/// records as `gated` says the caller reached them.
#[allow(clippy::too_many_arguments)]
fn element(
    flow: &Flow<'_>,
    entity: &str,
    element_of: &Element,
    includes: &[Include],
    reached: &Reached,
    origin: Origin,
    gated: &dyn Fn(Reached) -> Reached,
) -> Prov {
    if element_of.ty == entity {
        return Prov::of(Src::Record {
            entity: entity.to_string(),
            reached: reached.clone(),
            given: Vec::new(),
            origin,
        });
    }
    let mut out = Prov::default();
    for (name, source) in &element_of.fields {
        let prov = match source {
            // `count()` carries no field; another aggregate or a group
            // carries what its field holds (chunk 5.6c).
            Source::Entry { field: None, .. } => Prov::default(),
            Source::Column(field)
            | Source::Entry {
                field: Some(field), ..
            } => Prov::of(Src::Field {
                entity: entity.to_string(),
                field: field.clone(),
                reached: reached.clone(),
                origin,
            }),
            Source::Include(at) => {
                let Some(inner) = includes.get(*at) else {
                    continue;
                };
                let next = match inner.kind {
                    IncludeKind::Parent => {
                        flow.prover
                            .parent(entity, &inner.via, &inner.entity, reached)
                    }
                    IncludeKind::Child | IncludeKind::Children => {
                        gated(flow.prover.read(&inner.entity))
                    }
                };
                element(
                    flow,
                    &inner.entity,
                    &inner.element,
                    &inner.includes,
                    &next,
                    None,
                    gated,
                )
            }
        };
        out.fields.insert(name.clone(), prov);
    }
    out
}

/// The fields among `prov`'s sources that a caller who reached their
/// record isn't proved to read, with how they reached it: for a record
/// passed whole, each of its fields but those a write just gave it.
fn unproved(flow: &Flow<'_>, prov: &Prov) -> Vec<(String, String, Reached)> {
    unread(flow, prov)
        .into_iter()
        .map(|(entity, field, reached, _)| (entity, field, reached))
        .collect()
}

/// [`unproved`], each with whether it only decided the value.
fn unread(flow: &Flow<'_>, prov: &Prov) -> Vec<(String, String, Reached, bool)> {
    let mut out = Vec::new();
    for src in prov.flat() {
        let (src, decided) = match shallow(src) {
            Src::Decided(inner) => (shallow(*inner), true),
            other => (other, false),
        };
        match src {
            Src::Field {
                entity,
                field,
                reached,
                ..
            } => {
                if !flow.prover.field_proved(&entity, &field, &reached) {
                    out.push((entity, field, reached, decided));
                }
            }
            Src::Record {
                entity,
                reached,
                given,
                ..
            } => {
                let Some(model) = flow.entities.get(&entity) else {
                    continue;
                };
                for field in &model.fields {
                    if !given.contains(&field.name)
                        && !flow.prover.field_proved(&entity, &field.name, &reached)
                    {
                        out.push((entity.clone(), field.name.clone(), reached.clone(), decided));
                    }
                }
            }
            Src::Caller(_) | Src::Param(..) | Src::Decided(_) | Src::Deep(_) => {}
        }
    }
    out
}

/// For a reason: ", <who> among them," naming a caller of `reached` who
/// may not read `entity`'s `field`.
fn among(flow: &Flow<'_>, entity: &str, field: &str, reached: &Reached) -> String {
    flow.prover
        .counterexample(reached, |facts| {
            flow.prover.field_readable(entity, field, facts)
        })
        .map(|who| format!(", {who} among them,"))
        .unwrap_or_default()
}

/// Whether the failure field `key` is one of its `client:` fields, which
/// the caller is sent: read from that one failure's declaration.
fn client_field(flow: &Flow<'_>, key: &Key) -> bool {
    let Some((failure, field)) = key.name().rsplit_once('.') else {
        return false;
    };
    crate::failures::client_fields(
        flow.db,
        flow.project,
        Name::new(flow.db, failure.to_string()),
    )
    .iter()
    .any(|name| name == field)
}

/// Whether the type called `name` is an `Output`.
/// A map layer's type as written, `FeatureCollection<Point, PinProperties>`,
/// when `ty` is one (area 20).
fn collection_type(ty: &TypeRef) -> Option<String> {
    match ty {
        TypeRef::Nullable(inner) => collection_type(inner),
        TypeRef::Std(name, args)
            if name == jadpo_std::standard::geo::FEATURE_COLLECTION && args.len() == 2 =>
        {
            Some(ty.to_string())
        }
        _ => None,
    }
}

fn is_output(lookup: &Lookup<'_>, name: &str) -> bool {
    matches!(
        lookup.def(name),
        Some(Def::Fields {
            base: FieldsBase::Output,
            ..
        })
    )
}
