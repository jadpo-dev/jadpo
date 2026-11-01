//! Erasing a person in their store (area 15, "Erasure" and "What the
//! compiler checks"; chunk P3).
//!
//! - **What erasure does to each record** follows from the reference
//!   that links it to a record erasure deletes ([`outcome`]): its
//!   `erase:`, or else its `on_delete:`. `OnDelete.cascade` deletes it
//!   (`Erasure.deleted`), `OnDelete.cleared` keeps it and clears the
//!   reference and every marked field (`Erasure.anonymised`), and an
//!   `OnDelete.restricted` reference must say, since a person's request
//!   to be forgotten can't be refused the way an ordinary delete is.
//!   `Erasure.retained` keeps the record as it is, clearing only the
//!   reference, until its own retention ends.
//! - **What erasing one person reaches** ([`erasures`]): the person's own
//!   record, and every record a reference with the outcome
//!   `Erasure.deleted` leads to from one erasure deletes, however deep;
//!   and each record kept that refers to one of those. A kept record's
//!   own children stay with it, as they do when an ordinary delete
//!   clears a reference.
//! - **Erasure is complete** ([`erasure_problems`]): every `restricted`
//!   reference into records erasure deletes has an `erase:`, a reference
//!   erasure clears can be `none`, so can each marked field of a record
//!   it anonymises, and a record it keeps with `Erasure.retained` has a
//!   `retention:` saying when it goes. An `erase:` erasure never reaches
//!   does nothing, so it is refused.
//! - **What isn't built yet** is refused where `User.erase` is called, so
//!   no erasure is ever partial: one that would delete a `File`'s bytes,
//!   or anonymise another person's own record. The person's own record
//!   kept field by field, with `erase:` on its own fields, is refused
//!   where that is written. Every record erasure reaches is in the
//!   person's store, since no reference crosses stores.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use jadpo_db::{Db, File, ItemKind, Project, item_tree};
use jadpo_diag::{Diagnostic, Span, rule};
use jadpo_std::{Erasure as Outcome, OnDelete};

use crate::data::{DataModel, EntityModel, FieldModel, data_model, entity_read};
use crate::lookups::Name;

/// What erasing one record of a person entity does in its store.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Erasure {
    /// The entities whose records it deletes, the person's own first. A
    /// record's place here is the number its entity is tagged with.
    pub deleted: Vec<String>,
    /// Each reference it follows from a record it deletes to the records
    /// referring to it, which it deletes too.
    pub follows: Vec<Follow>,
    /// Each entity whose records it keeps while they refer to a record it
    /// deletes, with what it clears on them.
    pub kept: Vec<Kept>,
    /// What it would reach that isn't built yet, each as the end of a
    /// sentence saying why: what refuses `User.erase` where it is called.
    pub unbuilt: Vec<String>,
}

/// A reference erasure follows to records it deletes.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Follow {
    /// The place, in [`Erasure::deleted`], of the entity it refers to.
    pub from: usize,
    /// The place of the entity holding it.
    pub to: usize,
    /// The reference field.
    pub field: String,
}

/// An entity whose records erasure keeps while they refer to one it
/// deletes.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Kept {
    pub entity: String,
    /// Each of its references to an entity erasure deletes, which it
    /// clears where it refers to a record erasure deletes.
    pub references: Vec<KeptReference>,
    /// The marked fields it clears on a record one of its
    /// `Erasure.anonymised` references keeps, in field order.
    pub clears: Vec<String>,
}

/// One reference of a kept entity.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct KeptReference {
    pub field: String,
    /// The place, in [`Erasure::deleted`], of the entity it refers to.
    pub target: usize,
    /// `Erasure.anonymised` or `Erasure.retained`.
    pub outcome: Outcome,
}

/// What erasure does to a record holding `field`, a reference, when
/// erasure deletes the record it refers to: its `erase:`, or what its
/// `on_delete:` implies. `None` for a `restricted` reference that doesn't
/// say, which is `data.erase_required`.
pub fn outcome(field: &FieldModel) -> Option<Outcome> {
    let reference = field.reference.as_ref()?;
    if let Some(erase) = field.erase {
        return Some(erase);
    }
    match reference.on_delete? {
        OnDelete::Cascade => Some(Outcome::Deleted),
        OnDelete::Cleared => Some(Outcome::Anonymised),
        OnDelete::Restricted => None,
    }
}

/// Each person entity's erasure, by name.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Erasures {
    pub people: BTreeMap<String, Erasure>,
}

impl Erasures {
    /// The first person, by name, whose erasure deletes records of
    /// `entity`.
    pub fn deleting(&self, entity: &str) -> Option<&str> {
        self.people
            .iter()
            .find(|(_, erasure)| erasure.deleted.iter().any(|name| name == entity))
            .map(|(person, _)| person.as_str())
    }
}

/// What erasing each person entity does, worked out once for the project
/// from the entities without positions and their marked fields.
#[salsa::tracked(returns(ref))]
pub fn erasures(db: &dyn Db, project: Project) -> Arc<Erasures> {
    let model = data_model(db, project);
    let marks = crate::ai_personal::marks(db, project);
    let marked = |entity: &str, field: &str| {
        marks
            .fields
            .get(&(entity.to_string(), field.to_string()))
            .is_some_and(|mark| mark.category.is_some())
    };
    let people = model
        .entities
        .iter()
        .filter(|entity| entity.person)
        .map(|person| {
            (
                person.name.clone(),
                erasure_of(model, &person.name, &marked),
            )
        })
        .collect();
    Arc::new(Erasures { people })
}

/// What erasing one record of `person` does.
fn erasure_of(model: &DataModel, person: &str, marked: &dyn Fn(&str, &str) -> bool) -> Erasure {
    let mut out = Erasure {
        deleted: vec![person.to_string()],
        ..Erasure::default()
    };
    let mut kept: BTreeMap<String, Vec<KeptReference>> = BTreeMap::new();
    let mut at = 0;
    while let Some(current) = out.deleted.get(at).cloned() {
        for holder in &model.entities {
            for field in &holder.fields {
                if field
                    .reference
                    .as_ref()
                    .is_none_or(|reference| reference.target != current)
                {
                    continue;
                }
                match outcome(field) {
                    Some(Outcome::Deleted) => {
                        let to = match out.deleted.iter().position(|name| *name == holder.name) {
                            Some(to) => to,
                            None => {
                                out.deleted.push(holder.name.clone());
                                out.deleted.len() - 1
                            }
                        };
                        out.follows.push(Follow {
                            from: at,
                            to,
                            field: field.name.clone(),
                        });
                    }
                    Some(outcome) => {
                        kept.entry(holder.name.clone())
                            .or_default()
                            .push(KeptReference {
                                field: field.name.clone(),
                                target: at,
                                outcome,
                            })
                    }
                    // Reported where the reference is written
                    // (`data.erase_required`).
                    None => {}
                }
            }
        }
        at += 1;
    }
    for name in &out.deleted {
        let Some(entity) = model.entity(name) else {
            continue;
        };
        for field in entity.fields.iter().filter(|field| field.file.is_some()) {
            let what = if name == person {
                format!(
                    "a `{person}`'s `{}` is a `File`, whose bytes erasing them would delete",
                    field.name
                )
            } else {
                format!(
                    "erasing a `{person}` deletes their `{name}` records, whose `{}` is a `File`, with its bytes",
                    field.name
                )
            };
            out.unbuilt.push(what);
        }
    }
    for (name, references) in kept {
        let Some(entity) = model.entity(&name) else {
            continue;
        };
        let anonymised = references
            .iter()
            .filter(|reference| reference.outcome == Outcome::Anonymised)
            .collect::<Vec<_>>();
        let mut clears = Vec::new();
        if let Some(first) = anonymised.first() {
            if entity.person {
                out.unbuilt.push(format!(
                    "erasing a `{person}` would anonymise each `{name}` whose `{}` refers to them, clearing marked fields that are that person's own",
                    first.field
                ));
            } else {
                let references: BTreeSet<&str> =
                    references.iter().map(|r| r.field.as_str()).collect();
                for field in &entity.fields {
                    if references.contains(field.name.as_str()) || !marked(&name, &field.name) {
                        continue;
                    }
                    if field.file.is_some() {
                        out.unbuilt.push(format!(
                            "erasing a `{person}` clears `{name}.{}`, a `File`, with its bytes",
                            field.name
                        ));
                    }
                    clears.push(field.name.clone());
                }
            }
        }
        out.kept.push(Kept {
            entity: name,
            references,
            clears,
        });
    }
    out
}

/// Area 15's erasure problems in one file, for each entity it declares:
/// an `erase:` that erasure never reaches, the person's own field's
/// `erase:` (not built yet), a `restricted` reference into records
/// erasure deletes with no `erase:`, a field erasure clears that can't be
/// `none`, and a record kept with `Erasure.retained` that has no
/// `retention:`.
#[salsa::tracked(returns(ref))]
pub fn erasure_problems(db: &dyn Db, project: Project, file: File) -> Vec<Diagnostic> {
    let path = file.path(db);
    let people = crate::persons::persons(db, project);
    let erasures = erasures(db, project);
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
        if entity.path != *path {
            continue;
        }
        let marked = |field: &str| {
            marks
                .fields
                .get(&(entity.name.clone(), field.to_string()))
                .is_some_and(|mark| mark.category.is_some())
        };
        // Each marked field reported once, however many references
        // anonymise the record.
        let mut cleared: BTreeSet<String> = BTreeSet::new();
        for (at, field) in entity.fields.iter().enumerate() {
            let Some(field_span) = read.spans.fields.get(at) else {
                continue;
            };
            let written = read
                .spans
                .field_erasures
                .iter()
                .find(|(name, ..)| *name == field.name)
                .map(|(_, key, value)| (key.clone(), value.clone()));
            if let (Some(erase), Some((key, _))) = (field.erase, &written)
                && let Some(problem) = unreached(entity, field, erase, key, &people)
            {
                out.push(problem);
                continue;
            }
            let Some(reference) = &field.reference else {
                continue;
            };
            let Some(person) = erasures.deleting(&reference.target) else {
                continue;
            };
            let value_span = written.as_ref().map(|(_, value)| value.clone());
            match outcome(field) {
                // A reference with no `on_delete:` is reported where it is
                // written.
                None if reference.on_delete.is_none() => {}
                None => out.push(required(entity, field, person, field_span)),
                Some(Outcome::Deleted) => {}
                Some(kept) => {
                    if !field.nullable {
                        out.push(reference_required(
                            entity,
                            field,
                            kept,
                            person,
                            value_span.as_ref().unwrap_or(field_span),
                        ));
                    }
                    if kept == Outcome::Retained && entity.retention.is_none() {
                        out.push(retention_required(
                            entity,
                            field,
                            person,
                            value_span.as_ref().unwrap_or(field_span),
                        ));
                    }
                    if kept == Outcome::Anonymised && !entity.person {
                        for (other_at, other) in entity.fields.iter().enumerate() {
                            if other.name == field.name
                                || other.nullable
                                || other.file.is_some()
                                || !marked(&other.name)
                                || !cleared.insert(other.name.clone())
                            {
                                continue;
                            }
                            let Some(at) = read.spans.fields.get(other_at) else {
                                continue;
                            };
                            out.push(marked_required(
                                entity,
                                field,
                                other,
                                person,
                                at,
                                field_span,
                                written.is_some(),
                            ));
                        }
                    }
                }
            }
        }
    }
    out
}

/// `entity.field`'s `erase:` when erasure never reaches the field: one
/// that isn't a reference, or refers to records belonging to no person.
/// On a person's own field it is the person's own record kept field by
/// field, which isn't built yet.
fn unreached(
    entity: &EntityModel,
    field: &FieldModel,
    erase: Outcome,
    key: &Span,
    people: &crate::persons::Persons,
) -> Option<Diagnostic> {
    let (name, field_name) = (&entity.name, &field.name);
    let target = field.reference.as_ref().map(|r| r.target.as_str());
    if target.is_some_and(|target| people.reaches(target)) {
        return None;
    }
    if entity.person {
        return Some(
            Diagnostic::new(
                &rule::TOOLCHAIN_NOT_BUILT_YET,
                format!("Keeping `{name}.{field_name}` when a `{name}` is erased isn't built yet"),
            )
            .at(key.clone(), "decided, not built")
            .reason(format!(
                "`erase: Erasure.{}` on a person's own field keeps their record field by field when they are erased, which isn't built yet: erasing a `{name}` deletes their whole record. Accepted, the app would believe `{field_name}` is kept when it is deleted.",
                erase.name()
            ))
            .next(format!("Leave `erase:` off `{field_name}` for now")),
        );
    }
    let (title, label, reason) = match (&field.reference, target) {
        (Some(_), Some(target)) => (
            format!("Erasing a person never reaches `{name}.{field_name}`"),
            format!("`{target}` belongs to no person"),
            format!(
                "`erase:` says what erasing a person does to a `{name}` when erasure deletes the `{target}` it refers to. `{target}` isn't a person (`person: true`) and none of its references leads to one, so erasure never deletes one."
            ),
        ),
        _ => (
            format!("`{name}.{field_name}` isn't a reference, so `erase:` does nothing"),
            "not a reference".to_string(),
            format!(
                "`erase:` says what erasing a person does to a record that refers to them, or to one of their records, so it belongs on a field typed `X.id`; `{field_name}` is `{}`.",
                field.ty.strip_nullable()
            ),
        ),
    };
    Some(
        Diagnostic::new(&rule::DATA_ERASE_UNREACHED, title)
            .at(key.clone(), label)
            .reason(reason)
            .next(format!("Remove `erase:` from `{field_name}`")),
    )
}

/// `data.erase_required`: a `restricted` reference into records erasing
/// `person` deletes, with no `erase:`.
fn required(entity: &EntityModel, field: &FieldModel, person: &str, at: &Span) -> Diagnostic {
    let (name, field_name) = (&entity.name, &field.name);
    let target = field
        .reference
        .as_ref()
        .map_or("", |reference| reference.target.as_str());
    let one = words(name);
    let a = article(&one);
    let deletes = if target == person {
        format!(
            "Erasing a `{person}` deletes their record, and decides what happens to every record that refers to it"
        )
    } else {
        format!(
            "Erasing a `{person}` deletes each `{target}` of theirs, and decides what happens to every record that refers to one"
        )
    };
    Diagnostic::new(
        &rule::DATA_ERASE_REQUIRED,
        format!("`{name}.{field_name}` doesn't say what erasing a `{person}` does to it"),
    )
    .at(at.clone(), "`OnDelete.restricted`, with no `erase:`")
    .reason(format!(
        "{deletes}. `{field_name}` is `OnDelete.restricted`, which stops ordinary code deleting a `{target}` while {a} {one} refers to it; a person's request to be forgotten can't be refused that way, so the reference says what erasure does instead."
    ))
    .next(format!(
        "Add `erase:` to `{field_name}`: `Erasure.deleted` deletes the {one} too, `Erasure.anonymised` keeps it without the reference or its marked fields, and `Erasure.retained` keeps it as it is until its own retention ends"
    ))
}

/// `data.erase_field_required`: a reference erasure clears that can't be
/// `none`.
fn reference_required(
    entity: &EntityModel,
    field: &FieldModel,
    kept: Outcome,
    person: &str,
    at: &Span,
) -> Diagnostic {
    let (name, field_name) = (&entity.name, &field.name);
    let ty = field.ty.strip_nullable();
    let one = words(name);
    Diagnostic::new(
        &rule::DATA_ERASE_FIELD_REQUIRED,
        format!("`{name}.{field_name}` can't be cleared"),
    )
    .at(at.clone(), "keeps the record, clearing a reference that can't be `none`")
    .reason(format!(
        "`Erasure.{}` keeps each {one} when a `{person}` is erased and clears `{field_name}`, which only an optional reference (`{ty}?`) can be without.",
        kept.name()
    ))
    .next(format!(
        "Make it `{ty}?`, or use `Erasure.deleted` to delete the {one} instead"
    ))
}

/// `data.erase_field_required`: a marked field of a record erasure
/// anonymises that can't be `none`.
fn marked_required(
    entity: &EntityModel,
    reference: &FieldModel,
    field: &FieldModel,
    person: &str,
    at: &Span,
    reference_at: &Span,
    written: bool,
) -> Diagnostic {
    let (name, field_name) = (&entity.name, &field.name);
    let ty = field.ty.strip_nullable();
    let one = words(name);
    let why = if written {
        format!("`{}` is `Erasure.anonymised`", reference.name)
    } else {
        format!(
            "`{}` is `OnDelete.cleared`, which means `Erasure.anonymised`",
            reference.name
        )
    };
    Diagnostic::new(
        &rule::DATA_ERASE_FIELD_REQUIRED,
        format!("Erasing a `{person}` can't clear `{name}.{field_name}`"),
    )
    .at(at.clone(), "personal data that can't be `none`")
    .also(reference_at.clone(), "keeps the record without the person")
    .reason(format!(
        "{why}: erasing a `{person}` keeps each {one} that refers to them, clearing the reference and every marked field. `{field_name}` is marked `personal:`, and only an optional field (`{ty}?`) can be cleared."
    ))
    .next(format!(
        "Make `{field_name}` `{ty}?`, or say `erase: Erasure.deleted` on `{}` to delete the {one} instead",
        reference.name
    ))
}

/// `data.erase_retention_required`: a record kept with
/// `Erasure.retained` whose entity has no `retention:`.
fn retention_required(
    entity: &EntityModel,
    field: &FieldModel,
    person: &str,
    at: &Span,
) -> Diagnostic {
    let (name, field_name) = (&entity.name, &field.name);
    let one = words(name);
    let a = article(&one);
    Diagnostic::new(
        &rule::DATA_ERASE_RETENTION_REQUIRED,
        format!("`{name}` has no retention, so {a} {one} kept after erasure would never go"),
    )
    .at(at.clone(), "keeps the record as it is")
    .reason(format!(
        "`Erasure.retained` on `{field_name}` keeps each {one} as it is when a `{person}` is erased, for a reason the law gives, such as tax records, until its own retention ends. `{name}` has no `retention:`, so it would be kept for ever."
    ))
    .next(format!(
        "Give `{name}` a `retention: {{ after: config.<setting> since: <field> }}` saying how long it is kept, or use `Erasure.deleted` or `Erasure.anonymised` on `{field_name}`"
    ))
}

/// `invoice line` for `InvoiceLine`: an entity's name as words.
fn words(entity: &str) -> String {
    crate::case::to_snake(entity).replace('_', " ")
}

/// The article before `words`.
fn article(words: &str) -> &'static str {
    if words.starts_with(['a', 'e', 'i', 'o', 'u']) {
        "an"
    } else {
        "a"
    }
}
