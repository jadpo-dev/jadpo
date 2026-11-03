//! The person and the records that belong to them (area 15, "The person"
//! and "What the compiler checks"; chunks P1 and P4b).
//!
//! - **A person** is an entity marked `person: true`
//!   ([`crate::data::EntityModel::person`]). An app may have several.
//! - **A record belongs to a person** when it refers to them directly: a
//!   todo through its `owner_id`, and a reply through its `author_id`, not
//!   through the todo it answers. A record that refers to two people
//!   belongs to both, and [`persons`] finds every one once for the project,
//!   for erasure to ask and the graph's `person` nodes to show.
//! - **No personal data erasure can't reach.** A field marked `personal:`
//!   on an entity that erasing a person never reaches is
//!   `data.personal_unreachable` ([`personal_problems`]): erasure could
//!   never find it. Erasure reaches a person's records and walks on through
//!   what it deletes ([`crate::erasure::Erasures::reaches`]), so a todo's
//!   attachment is found, though it belongs to no one. Only `personal:`
//!   marks a field here, as area 15 defines a marked field; a standard type
//!   that is personal data whatever holds it (area 19) doesn't make the
//!   entity holding it a person's, so a shop's own `PhoneNumber` needs none.

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
    /// entity and field: a reference to the person. The person's own
    /// references to itself are among them.
    pub through: BTreeSet<(String, String)>,
}

/// Every person entity, by name, with its records.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Persons {
    pub people: BTreeMap<String, Person>,
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
/// reference to it.
fn records_of(entities: &[EntityModel], person: &str) -> Person {
    let mut out = Person::default();
    for entity in entities {
        for field in &entity.fields {
            if field
                .reference
                .as_ref()
                .is_none_or(|reference| reference.target != person)
            {
                continue;
            }
            out.through
                .insert((entity.name.clone(), field.name.clone()));
            if entity.name != person {
                out.records.insert(entity.name.clone());
            }
        }
    }
    out
}

/// Area 15's problems in one file: each entity it declares holding a
/// field marked `personal:` that no person's erasure can reach, reported
/// once at its first such field, naming the others.
#[salsa::tracked(returns(ref))]
pub fn personal_problems(db: &dyn Db, project: Project, file: File) -> Vec<Diagnostic> {
    let path = file.path(db);
    let erasures = crate::erasure::erasures(db, project);
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
        if entity.path != *path || erasures.reaches(entity) || read.spans.person.is_some() {
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
                    "Erasing a person reaches the records that refer to them, and the records that refer to what it deletes. `{name}` isn't a person (`person: true`), and none of its references leads to a record erasing one deletes, so nothing could ever erase what it holds."
                ))
                .next(format!(
                    "Refer to the person `{name}.{field}` is about, or mark `{name}` `person: true` if each record is one"
                )),
        );
    }
    out
}
