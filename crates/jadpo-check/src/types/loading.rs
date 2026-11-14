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
//! - erases a person (area 15; chunk P3): its one statement reaches every
//!   record of theirs, and one added since its snapshot must be a
//!   concurrent change, which reruns the action, rather than a reference
//!   the database clears unseen;
//! - or calls an action or fixture that does, since that action joins its
//!   transaction.
//!
//! Nothing else loads one: a create (a reference it sets is checked by the
//! store in the same statement), an update, delete or unguarded transition
//! by `id`, a counter (`Video.view_count + 1`, worked out by the database
//! in one step), and calls to functions, which never touch a store.
//!
//! An action that writes a store counts only the records it loads there,
//! through every call: a record of another store is read outside its
//! transaction (area 6, "One store per action"; chunk Q5), so no level of
//! that transaction holds it. One that writes no store counts every record
//! it loads, since its transaction is whichever store it reads first.
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
    /// A person's erasure (area 15; chunk P3), whose one statement must
    /// meet a record added since its snapshot as a concurrent change: the
    /// person entity's name.
    Erases(String),
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
            Loads::Erases(entity) => format!("erases a `{entity}` with `{entity}.erase`"),
            Loads::Calls(action) => format!("calls `{action}`, which loads a record"),
        }
    }
}

/// What one action or fixture does that decides its isolation, read from
/// its own signature and body.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Loading {
    /// Each way it loads a record itself, its parameters first, then its
    /// body in source order, with the entities whose records it loads that
    /// way (none when the checker can't tell, which counts everywhere).
    pub itself: Vec<(Loads, BTreeSet<String>)>,
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
        let found = |loads: Loads, entities: BTreeSet<String>, loading: &mut Loading| {
            loading.itself.push((loads, entities));
        };
        let one = |entity: &str| BTreeSet::from([entity.to_string()]);
        if let Some(sig) = lookup.signature(&item.name) {
            for (param, ty) in &sig.params {
                let mut held = BTreeSet::new();
                if records_held(&lookup, ty, &mut BTreeSet::new(), &mut held) {
                    found(Loads::Handed(param.clone()), held, &mut loading);
                }
            }
        }
        for fact in &facts.facts {
            match fact {
                Fact::Gets(entity) => found(Loads::Gets(entity.clone()), one(entity), &mut loading),
                Fact::Queries(query) => {
                    let entity = super::stores::query_entity(
                        db,
                        project,
                        crate::lookups::Name::new(db, query.clone()),
                    );
                    found(
                        Loads::Queries(query.clone()),
                        entity.iter().cloned().collect(),
                        &mut loading,
                    );
                }
                Fact::Action(callee) | Fact::Fixture(callee) => {
                    loading.calls.insert(callee.clone());
                }
                Fact::Writes(write) => {
                    if write.erase {
                        found(
                            Loads::Erases(write.entity.clone()),
                            one(&write.entity),
                            &mut loading,
                        );
                    }
                    if write.given {
                        found(
                            Loads::WritesBack(write.entity.clone()),
                            one(&write.entity),
                            &mut loading,
                        );
                    }
                    if let Some(transition) = &write.transition
                        && guarded(&lookup, &write.entity, transition)
                    {
                        found(
                            Loads::Guarded(transition.name().to_string()),
                            one(&write.entity),
                            &mut loading,
                        );
                    }
                }
                // A run's own calls are the run's, never the action's
                // (area 12).
                Fact::Function(_)
                | Fact::Session(_)
                | Fact::Emits(_)
                | Fact::Batch(_)
                | Fact::Runs(_)
                | Fact::Uploads(_) => {}
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
/// object's, input's, output's, event's or variant's fields. Each entity
/// whose record it holds is added to `held`.
fn records_held(
    lookup: &Lookup<'_>,
    ty: &Ty,
    seen: &mut BTreeSet<String>,
    held: &mut BTreeSet<String>,
) -> bool {
    match ty {
        Ty::Named(name) | Ty::Variant(name, _) => {
            if !seen.insert(name.clone()) {
                return held.contains(name);
            }
            let fields =
                |fields: &[FieldDef], seen: &mut BTreeSet<String>, held: &mut BTreeSet<String>| {
                    let mut any = false;
                    for field in fields {
                        any |= records_held(lookup, &field.declared, seen, held);
                    }
                    any
                };
            match lookup.def(name) {
                Some(Def::Fields {
                    base: FieldsBase::Entity,
                    ..
                }) => {
                    held.insert(name.clone());
                    true
                }
                Some(Def::Fields { fields: own, .. }) => fields(own, seen, held),
                Some(Def::Enum { variants }) => {
                    let mut any = false;
                    for own in variants
                        .iter()
                        .filter_map(|variant| variant.fields.as_deref())
                    {
                        any |= fields(own, seen, held);
                    }
                    any
                }
                Some(Def::Scalar { .. } | Def::Format { .. }) | None => false,
            }
        }
        Ty::List(inner) | Ty::Set(inner) | Ty::Nullable(inner) => {
            records_held(lookup, inner, seen, held)
        }
        Ty::Map(key, value) => {
            let key = records_held(lookup, key, seen, held);
            records_held(lookup, value, seen, held) || key
        }
        Ty::Opaque(_, args) => {
            let mut any = false;
            for arg in args {
                any |= records_held(lookup, arg, seen, held);
            }
            any
        }
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
/// loads a record, itself or through any action or fixture it calls, in
/// the store it writes (anywhere, when it writes none), and why; read
/// committed otherwise.
#[salsa::tracked(returns(ref))]
pub fn isolation(db: &dyn Db, project: Project) -> BTreeMap<Key, Decided> {
    let mut loading: BTreeMap<Key, &Loading> = BTreeMap::new();
    for &file in project.files(db) {
        for (key, found) in file_loading(db, project, file) {
            loading.entry(key.clone()).or_insert(found);
        }
    }
    let written = super::written_stores(db, project);
    let store_of = |entity: &str| {
        super::stores::entity_store(
            db,
            project,
            crate::lookups::Name::new(db, entity.to_string()),
        )
        .clone()
    };
    let mut out = BTreeMap::new();
    for (key, own) in &loading {
        // The store its transaction is in, when it writes one: a record
        // loaded in another is read outside it.
        let store = written
            .get(key.name())
            .and_then(|stores| stores.keys().next());
        let counts = |entities: &BTreeSet<String>| {
            let Some(store) = store else {
                return true;
            };
            entities.is_empty()
                || entities
                    .iter()
                    .any(|entity| store_of(entity).is_none_or(|kept| kept == *store))
        };
        let first = |loading: &Loading| {
            loading
                .itself
                .iter()
                .find(|(_, entities)| counts(entities))
                .map(|(loads, _)| loads.clone())
        };
        let because = first(own).or_else(|| {
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
                if first(callee).is_some() {
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
