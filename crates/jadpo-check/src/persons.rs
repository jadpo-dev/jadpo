//! The person and the records that belong to them (area 15, "The person"
//! and "What the compiler checks"; chunk P1).
//!
//! - **A person** is an entity marked `person: true`
//!   ([`crate::data::EntityModel::person`]). An app may have several.
//! - **A record belongs to a person** when it refers to them, directly or
//!   through a chain of references: a todo through its `owner_id`, a todo's
//!   attachment through the todo. A record that refers to two people
//!   belongs to both, a chain through another person's records goes on
//!   through them, and one that comes back to an entity it passed ends
//!   there. [`persons`] finds every one once for the project, for erasure
//!   and export to reach and the graph's `person` nodes to show.
//! - **No personal data erasure can't reach.** A field marked `personal:`
//!   on an entity that is no person and belongs to none is
//!   `data.personal_unreachable` ([`personal_problems`]): erasing or
//!   exporting a person could never find it. Only `personal:` marks a
//!   field here, as area 15 defines a marked field; a standard type that is
//!   personal data whatever holds it (area 19) doesn't make the entity
//!   holding it a person's, so a shop's own `PhoneNumber` needs none.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use jadpo_db::{Db, File, ItemKind, Project, item_tree};
use jadpo_diag::{Diagnostic, rule};

use crate::data::{EntityModel, entity_read};
use crate::lookups::Name;

/// One person entity's records.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Person {
    /// Every other entity whose records belong to the person, by name.
    pub records: BTreeSet<String>,
    /// Each reference a record belongs to the person through, as its
    /// entity and field: a reference to the person, or to an entity of
    /// `records`. The person's own references to itself are among them.
    pub through: BTreeSet<(String, String)>,
}

/// Every person entity, by name, with its records.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Persons {
    pub people: BTreeMap<String, Person>,
}

impl Persons {
    /// Whether the records of the entity called `entity` are people, or
    /// belong to one, so erasing and exporting a person can reach them.
    pub fn reaches(&self, entity: &str) -> bool {
        self.people.contains_key(entity)
            || self
                .people
                .values()
                .any(|person| person.records.contains(entity))
    }
}

/// Each person entity's records, from the entities without positions, so
/// only a change to an entity reads them again.
#[salsa::tracked(returns(ref))]
pub fn persons(db: &dyn Db, project: Project) -> Arc<Persons> {
    let entities = &crate::data::data_model(db, project).entities;
    let people = entities
        .iter()
        .filter(|entity| entity.person)
        .map(|person| (person.name.clone(), records_of(entities, &person.name)))
        .collect();
    Arc::new(Persons { people })
}

/// The records that belong to the person `person`: every entity with a
/// reference to it or to one of those already found, until no reference
/// adds another.
fn records_of(entities: &[EntityModel], person: &str) -> Person {
    let mut reached: BTreeSet<&str> = BTreeSet::from([person]);
    let mut through = BTreeSet::new();
    loop {
        let mut grew = false;
        for entity in entities {
            for field in &entity.fields {
                let Some(reference) = &field.reference else {
                    continue;
                };
                if !reached.contains(reference.target.as_str()) {
                    continue;
                }
                grew |= through.insert((entity.name.clone(), field.name.clone()));
                grew |= reached.insert(entity.name.as_str());
            }
        }
        if !grew {
            break;
        }
    }
    Person {
        records: reached
            .into_iter()
            .filter(|name| *name != person)
            .map(str::to_string)
            .collect(),
        through,
    }
}

/// Area 15's problems in one file: each entity it declares holding a
/// field marked `personal:` that no person's erasure can reach, reported
/// once at its first such field, naming the others.
#[salsa::tracked(returns(ref))]
pub fn personal_problems(db: &dyn Db, project: Project, file: File) -> Vec<Diagnostic> {
    let path = file.path(db);
    let people = persons(db, project);
    let marks = crate::ai_personal::marks(db, project);
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    for item in &item_tree(db, file).items {
        if item.kind != ItemKind::Entity || !seen.insert(item.name.as_str()) {
            continue;
        }
        let Some(read) = entity_read(db, project, Name::new(db, item.name.clone())) else {
            continue;
        };
        let entity = &read.model;
        // A `person:` written wrongly is reported where it is written.
        if entity.path != *path || people.reaches(&entity.name) || read.spans.person.is_some() {
            continue;
        }
        let marked: Vec<usize> = entity
            .fields
            .iter()
            .enumerate()
            .filter(|(_, field)| {
                marks
                    .fields
                    .get(&(entity.name.clone(), field.name.clone()))
                    .is_some_and(|mark| mark.category.is_some())
            })
            .map(|(at, _)| at)
            .collect();
        let Some((&first, rest)) = marked.split_first() else {
            continue;
        };
        let Some(span) = read.spans.fields.get(first) else {
            continue;
        };
        let name = &entity.name;
        let field = &entity.fields[first].name;
        let mut problem = Diagnostic::new(
            &rule::DATA_PERSONAL_UNREACHABLE,
            format!("Erasing a person can't find `{name}.{field}`"),
        )
        .at(span.clone(), "personal data that belongs to no person");
        for &at in rest {
            if let Some(span) = read.spans.fields.get(at) {
                problem = problem.also(span.clone(), "personal data too");
            }
        }
        out.push(
            problem
                .reason(format!(
                    "Erasing or exporting a person reaches the records that refer to them, directly or through records that do. `{name}` isn't a person (`person: true`), and none of its references leads to one, so nothing could ever erase or export what it holds."
                ))
                .next(format!(
                    "Refer to the person `{name}.{field}` is about, or mark `{name}` `person: true` if each record is one"
                )),
        );
    }
    out
}
