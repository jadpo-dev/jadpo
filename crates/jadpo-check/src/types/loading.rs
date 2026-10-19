//! Isolation by what an action does (area 6, "Counters and other one-step
//! updates"; decided 2026-10-15, built in chunk 3.7): an action that loads
//! no record, itself or through any action it calls, runs at read
//! committed, and every other action at repeatable read. Authors write
//! nothing; the graph carries the decision on each action and fixture, and
//! the plan hands it to the runtime, so both executors begin the same
//! transaction.
//!
//! An action loads a record when it
//!
//! - reads one: `Entity.get`, or a query, with whatever it includes;
//! - is handed one: a parameter whose type holds an entity's record
//!   (`todo: Todo` is a loaded record, area 5), however deep in a list,
//!   an optional or an object's fields;
//! - writes one back: an `update`, `delete` or transition of a record
//!   rather than its `id`, so the write rests on a copy loaded earlier;
//! - runs a transition whose `when:` can refuse a visible record: when
//!   the `UPDATE` changes nothing, a second statement asks whether the
//!   record is there, which tells the refusal from not-found only when it
//!   sees the first statement's snapshot;
//! - or calls an action or fixture that does, since that action joins its
//!   transaction.
//!
//! Nothing else loads one: a create (a reference it sets is checked by the
//! store in the same statement), an update, delete or unguarded transition
//! by `id`, a counter (`Video.view_count + 1`, worked out by the database
//! in one step), and calls to functions, which never touch a store.
//!
//! Each file's actions and fixtures are read from what their bodies do
//! (`body_facts`, without positions, chunk 4.11) and their signatures
//! ([`file_loading`]); the decision through every call is worked out once
//! for the project ([`isolation`]), as the stores an action writes are
//! (`written_stores`).

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use jadpo_db::{Db, File, ItemKind, Project, item_tree};
use jadpo_graph::{Key, Kind};
use jadpo_std::Isolation;

use super::body::Callable;
use super::facts::{Fact, body_facts};
use super::shape::FieldsBase;
use super::table::{Def, FieldDef, TypeView};
use super::ty::Ty;
use crate::lookups::{Lookup, SigView, WriteView};

/// How an action or fixture loads a record, by what it names: the first
/// way found, its parameters first, then its body in source order.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Loads {
    /// `Entity.get` reads the entity's records: its name.
    Gets(String),
    /// A query reads records: its name.
    Queries(String),
    /// A parameter holds a record: its name.
    Handed(String),
    /// An `update`, `delete` or transition of a record, not its `id`: the
    /// entity's name.
    WritesBack(String),
    /// A transition whose `when:` can refuse a visible record:
    /// `Entity.transition`.
    Guarded(String),
    /// An action or fixture it calls loads one: its name.
    Calls(String),
}

impl Loads {
    /// Why, as a sentence ends: `reads Book with get`.
    pub fn describe(&self) -> String {
        match self {
            Loads::Gets(entity) => format!("reads `{entity}` with `get`"),
            Loads::Queries(query) => format!("calls the query `{query}`"),
            Loads::Handed(param) => format!("is handed a record in `{param}`"),
            Loads::WritesBack(entity) => format!("writes back a `{entity}` it was given"),
            Loads::Guarded(transition) => {
                format!("runs `{transition}`, whose `when:` can refuse a record")
            }
            Loads::Calls(action) => format!("calls `{action}`, which loads a record"),
        }
    }
}

/// What one action or fixture does that decides its isolation, read from
/// its own signature and body.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Loading {
    /// How it loads a record itself, if it does.
    pub itself: Option<Loads>,
    /// The actions and fixtures it calls, which join its transaction.
    pub calls: BTreeSet<Key>,
}

/// One action's or fixture's decision.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Decided {
    pub isolation: Isolation,
    /// Why it runs at repeatable read; `None` at read committed.
    pub because: Option<Loads>,
}

/// What each action and fixture in `file` does that decides its isolation,
/// by key.
#[salsa::tracked(returns(ref))]
pub fn file_loading(db: &dyn Db, project: Project, file: File) -> BTreeMap<Key, Loading> {
    let lookup = Lookup::for_file(db, project, file);
    let mut out = BTreeMap::new();
    for item in &item_tree(db, file).items {
        if !matches!(item.kind, ItemKind::Action | ItemKind::Fixture) {
            continue;
        }
        let facts = body_facts(db, project, Callable::new(db, file, item.index, None));
        let Some(key) = facts.key.clone() else {
            continue;
        };
        let mut loading = Loading::default();
        let found = |loads: Loads, loading: &mut Loading| {
            loading.itself.get_or_insert(loads);
        };
        if let Some(sig) = lookup.signature(&item.name) {
            for (param, ty) in &sig.params {
                if holds_record(&lookup, ty, &mut BTreeSet::new()) {
                    found(Loads::Handed(param.clone()), &mut loading);
                }
            }
        }
        for fact in &facts.facts {
            match fact {
                Fact::Gets(entity) => found(Loads::Gets(entity.clone()), &mut loading),
                Fact::Queries(query) => found(Loads::Queries(query.clone()), &mut loading),
                Fact::Action(callee) | Fact::Fixture(callee) => {
                    loading.calls.insert(callee.clone());
                }
                Fact::Writes(write) => {
                    if write.given {
                        found(Loads::WritesBack(write.entity.clone()), &mut loading);
                    }
                    if let Some(transition) = &write.transition
                        && guarded(&lookup, &write.entity, transition)
                    {
                        found(Loads::Guarded(transition.name().to_string()), &mut loading);
                    }
                }
                Fact::Function(_) | Fact::Session(_) | Fact::Emits(_) => {}
            }
        }
        out.insert(key, loading);
    }
    out
}

/// Whether `transition`, of `entity`, has a `when:` that can refuse a
/// visible record (area 5, "Lifecycle"; chunk 3.4).
fn guarded(lookup: &Lookup<'_>, entity: &str, transition: &Key) -> bool {
    let name = transition
        .name()
        .rsplit_once('.')
        .map_or(transition.name(), |(_, name)| name);
    lookup
        .write_rules(entity)
        .and_then(|rules| rules.transition(name))
        .is_some_and(|rule| rule.guarded)
}

/// Whether a value of `ty` holds an entity's record: the entity itself,
/// or one inside a list, set, map, optional, standard generic type, or an
/// object's, input's, output's, event's or variant's fields.
fn holds_record(lookup: &Lookup<'_>, ty: &Ty, seen: &mut BTreeSet<String>) -> bool {
    match ty {
        Ty::Named(name) | Ty::Variant(name, _) => {
            if !seen.insert(name.clone()) {
                return false;
            }
            let fields = |fields: &[FieldDef], seen: &mut BTreeSet<String>| {
                fields
                    .iter()
                    .any(|field| holds_record(lookup, &field.declared, seen))
            };
            match lookup.def(name) {
                Some(Def::Fields {
                    base: FieldsBase::Entity,
                    ..
                }) => true,
                Some(Def::Fields { fields: own, .. }) => fields(own, seen),
                Some(Def::Enum { variants }) => variants
                    .iter()
                    .filter_map(|variant| variant.fields.as_deref())
                    .any(|own| fields(own, seen)),
                Some(Def::Scalar { .. } | Def::Format { .. }) | None => false,
            }
        }
        Ty::List(inner) | Ty::Set(inner) | Ty::Nullable(inner) => holds_record(lookup, inner, seen),
        Ty::Map(key, value) => holds_record(lookup, key, seen) || holds_record(lookup, value, seen),
        Ty::Opaque(_, args) => args.iter().any(|arg| holds_record(lookup, arg, seen)),
        Ty::Error
        | Ty::Prim(_)
        | Ty::Field(..)
        | Ty::None
        | Ty::Var(_)
        | Ty::Unknown(_)
        | Ty::Number
        | Ty::Function(..) => false,
    }
}

/// Each action's and fixture's isolation, by key: repeatable read when it
/// loads a record, itself or through any action or fixture it calls, and
/// why; read committed otherwise.
#[salsa::tracked(returns(ref))]
pub fn isolation(db: &dyn Db, project: Project) -> BTreeMap<Key, Decided> {
    let mut loading: BTreeMap<Key, &Loading> = BTreeMap::new();
    for &file in project.files(db) {
        for (key, found) in file_loading(db, project, file) {
            loading.entry(key.clone()).or_insert(found);
        }
    }
    let mut out = BTreeMap::new();
    for (key, own) in &loading {
        let because = own.itself.clone().or_else(|| {
            // The nearest action it calls, directly or not, that loads a
            // record itself, named by the action it calls directly.
            let mut seen = BTreeSet::from([key.clone()]);
            let mut queue: VecDeque<(&Key, &Key)> =
                own.calls.iter().map(|callee| (callee, callee)).collect();
            while let Some((through, current)) = queue.pop_front() {
                if !seen.insert(current.clone()) {
                    continue;
                }
                let Some(callee) = loading.get(current) else {
                    continue;
                };
                if callee.itself.is_some() {
                    return Some(Loads::Calls(through.name().to_string()));
                }
                queue.extend(callee.calls.iter().map(|next| (through, next)));
            }
            None
        });
        out.insert(
            key.clone(),
            Decided {
                isolation: Isolation::of(because.is_some()),
                because,
            },
        );
    }
    out
}

/// Whether nodes of `kind` run through an action boundary of their own,
/// and so carry an isolation level: actions and fixtures (area 13: a
/// fixture is a test-only action).
pub fn has_isolation(kind: Kind) -> bool {
    matches!(kind, Kind::Action | Kind::Fixture)
}
