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
//! - **Erasing one person never clears another's data** (chunks P4 and
//!   P4b; area 15's notes, "Records that belong to more than one person").
//!   A record belongs to the people it refers to directly, and keeps its
//!   marked fields while any other of them remains: erasure clears only
//!   the erased person's references, and the marked fields once none of
//!   the record's own references to a person holds one it keeps
//!   ([`Kept::through`]). Another person's own record loses only the
//!   reference, never its fields.
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
//! - **A `File`'s bytes** go with the records and fields erasure deletes
//!   and clears, counted by area 16's trigger like any other change
//!   (chunk B3). Every record erasure reaches is in the person's store,
//!   since no reference crosses stores.
//! - **The person's own record** (chunk P4b) is kept field by field when a
//!   field of theirs that isn't a reference says `erase: Erasure.retained`
//!   ([`Erasure::own`]): every other marked field is cleared, the time is
//!   noted in the column [`jadpo_std::ERASED_AT`] names, and the record is
//!   hidden like one a lifecycle hides, until the entity's `retention:`
//!   ends it. Only a query saying `include_erased: true` finds it
//!   (`data::queries`).
//! - **What isn't built yet** is refused where `User.erase` is called, so
//!   no erasure is ever partial: keeping an audited record, whose history
//!   would still hold what erasure clears.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use jadpo_db::{Db, File, ItemKind, Project, item_tree};
use jadpo_diag::{Diagnostic, Span, rule};
use jadpo_std::http::PrincipalKind;
use jadpo_std::{Erasure as Outcome, OnDelete};

use crate::data::{DataModel, EntityModel, FieldModel, data_model, entity_read};
use crate::lookups::Name;

/// What erasing one record of a person entity does in its store.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Erasure {
    /// The entities whose records it deletes, the person's own first. A
    /// record's place here is the number its entity is tagged with. Those
    /// [`Erasure::own`] names keep their records field by field instead.
    pub deleted: Vec<String>,
    /// Each reference it follows from a record it deletes to the records
    /// referring to it, which it deletes too.
    pub follows: Vec<Follow>,
    /// Each entity whose records it keeps while they refer to a record it
    /// deletes, with what it clears on them.
    pub kept: Vec<Kept>,
    /// Each person entity it reaches that keeps its records field by field
    /// rather than deleting them.
    pub own: Vec<Own>,
    /// Every entity whose records it deletes, or would once each
    /// `restricted` reference that doesn't say has answered
    /// `data.erase_required`: what it can find.
    pub reach: BTreeSet<String>,
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
    /// `Erasure.anonymised` references keeps, in field order, once no
    /// other person the record belongs to remains. None on another
    /// person's own record, whose fields are that person's.
    pub clears: Vec<String>,
    /// Each of its own references to a person, in field order, with the
    /// person's entity: a record keeps its marked fields while one of them
    /// holds a person the erasure keeps. Empty when the record refers to no
    /// person, or only through the one reference erasure clears.
    pub through: Vec<(String, String)>,
}

/// A person entity whose records erasure keeps field by field (area 15,
/// "The person's own record"; chunk P4b).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Own {
    /// The place, in [`Erasure::deleted`], of the entity.
    pub entity: usize,
    /// The fields that say `erase: Erasure.retained`, in field order.
    pub retained: Vec<String>,
    /// Every other marked field, in field order, which it clears.
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

    /// Whether erasing a person can find the records of `entity`: it is a
    /// person, erasure deletes its records, or it refers to a record
    /// erasure deletes, whatever erasure then does to it. A record past one
    /// erasure keeps is out of reach.
    pub fn reaches(&self, entity: &EntityModel) -> bool {
        self.finds(&entity.name)
            || entity.fields.iter().any(|field| {
                field
                    .reference
                    .as_ref()
                    .is_some_and(|reference| self.finds(&reference.target))
            })
    }

    /// Whether an erasure deletes records of the entity called `name`, or
    /// would once each `restricted` reference that doesn't say has answered.
    fn finds(&self, name: &str) -> bool {
        self.people
            .values()
            .any(|erasure| erasure.reach.contains(name))
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
        reach: reach_of(model, person),
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
    for (name, references) in kept {
        let Some(entity) = model.entity(&name) else {
            continue;
        };
        // A kept record's history still holds what it held while it was
        // the person's (area 16, "Audit trail"; chunk B11), which the
        // pages don't yet say erasure rewrites.
        if entity.audit.is_some()
            && let Some(first) = references.first()
        {
            out.unbuilt.push(format!(
                "erasing a `{person}` keeps each `{name}` whose `{}` refers to them, and the history its `audit:` keeps would still hold that reference",
                first.field
            ));
        }
        let anonymised = references
            .iter()
            .any(|reference| reference.outcome == Outcome::Anonymised);
        let mut clears = Vec::new();
        // Another person's own record keeps its fields: only the
        // reference to the erased person goes.
        if anonymised && !entity.person {
            let references: BTreeSet<&str> = references.iter().map(|r| r.field.as_str()).collect();
            for field in &entity.fields {
                if references.contains(field.name.as_str()) || !marked(&name, &field.name) {
                    continue;
                }
                clears.push(field.name.clone());
            }
        }
        let through = if anonymised && !entity.person {
            people_of(model, entity, &references)
        } else {
            Vec::new()
        };
        out.kept.push(Kept {
            entity: name,
            references,
            clears,
            through,
        });
    }
    // A person entity whose field says it stays keeps each record of its
    // field by field, and its history would still hold what that clears.
    for (at, name) in out.deleted.iter().enumerate() {
        let Some(entity) = model.entity(name) else {
            continue;
        };
        if !entity.keeps_erased() {
            continue;
        }
        if entity.audit.is_some() {
            out.unbuilt.push(format!(
                "erasing a `{person}` keeps each `{name}` it reaches field by field, and the history its `audit:` keeps would still hold what that clears"
            ));
        }
        let retained = |field: &FieldModel| {
            field.reference.is_none() && field.erase == Some(Outcome::Retained)
        };
        out.own.push(Own {
            entity: at,
            retained: entity
                .fields
                .iter()
                .filter(|field| retained(field))
                .map(|field| field.name.clone())
                .collect(),
            clears: entity
                .fields
                .iter()
                .filter(|field| !retained(field) && marked(name, &field.name))
                .map(|field| field.name.clone())
                .collect(),
        });
    }
    out
}

/// Every entity whose records erasing `person` deletes, counting a
/// `restricted` reference that doesn't say as one that deletes, so one
/// missing `erase:` is reported once and not again for all that lies past it.
fn reach_of(model: &DataModel, person: &str) -> BTreeSet<String> {
    let mut reach = BTreeSet::from([person.to_string()]);
    loop {
        let mut grew = false;
        for holder in &model.entities {
            let deletes = holder.fields.iter().any(|field| {
                field
                    .reference
                    .as_ref()
                    .is_some_and(|reference| reach.contains(&reference.target))
                    && matches!(outcome(field), None | Some(Outcome::Deleted))
            });
            if deletes {
                grew |= reach.insert(holder.name.clone());
            }
        }
        if !grew {
            return reach;
        }
    }
}

/// The people a record of `entity`, which erasure clears through
/// `cleared`, belongs to: its own references to a person entity. None
/// when it has no such reference, or when the only one is the only
/// reference erasure clears, since a record it clears has lost that person.
fn people_of(
    model: &DataModel,
    entity: &EntityModel,
    cleared: &[KeptReference],
) -> Vec<(String, String)> {
    let people: Vec<(String, String)> = entity
        .fields
        .iter()
        .filter_map(|field| {
            let reference = field.reference.as_ref()?;
            model
                .entity(&reference.target)
                .is_some_and(|target| target.person)
                .then(|| (field.name.clone(), reference.target.clone()))
        })
        .collect();
    match (people.as_slice(), cleared) {
        ([(only, _)], [one]) if *only == one.field => Vec::new(),
        _ => people,
    }
}

/// Area 15's erasure problems in one file, for each entity it declares:
/// an `erase:` that erasure never reaches, a person's own field's `erase:`
/// that says something it can't, a `restricted` reference into records
/// erasure deletes with no `erase:`, a field erasure clears that can't be
/// `none`, and a record kept with `Erasure.retained` that has no
/// `retention:`.
#[salsa::tracked(returns(ref))]
pub fn erasure_problems(db: &dyn Db, project: Project, file: File) -> Vec<Diagnostic> {
    let path = file.path(db);
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
        let identity = crate::identity::identity(db, project);
        let subject = |field: &str| {
            [PrincipalKind::User, PrincipalKind::Service]
                .into_iter()
                .filter_map(|kind| identity.variant(kind))
                .any(|variant| variant.entity == entity.name && variant.subject == field)
        };
        out.extend(own_record_problems(
            entity,
            &read.spans,
            erasures,
            &marked,
            &subject,
        ));
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
            if entity.person && field.reference.is_none() {
                // A person's own field, read above.
                continue;
            }
            if let (Some(_), Some((key, _))) = (field.erase, &written)
                && let Some(problem) = unreached(entity, field, key, erasures)
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

/// `entity.field`'s `erase:` when erasure never reads it: on a field that
/// isn't a reference, or on a reference to records erasing a person never
/// deletes.
fn unreached(
    entity: &EntityModel,
    field: &FieldModel,
    key: &Span,
    erasures: &Erasures,
) -> Option<Diagnostic> {
    let (name, field_name) = (&entity.name, &field.name);
    let target = field.reference.as_ref().map(|r| r.target.as_str());
    if target.is_some_and(|target| erasures.finds(target)) {
        return None;
    }
    let (title, label, reason) = match (&field.reference, target) {
        (Some(_), Some(target)) => (
            format!("Erasing a person never reaches `{name}.{field_name}`"),
            format!("erasing a person never deletes a `{target}`"),
            format!(
                "`erase:` says what erasing a person does to a `{name}` when erasure deletes the `{target}` it refers to. Erasure deletes a person's record and what its references say goes with it, and a `{target}` isn't either, so it never deletes one."
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

/// What a person entity's own fields say about keeping its record (area
/// 15, "The person's own record"; chunk P4b): each `erase:` on a field
/// that isn't a reference says `Erasure.retained`, on a marked field; and
/// an entity that keeps its records has a `retention:` saying when they
/// go, a column free for when they were erased, no reference that deletes
/// them with another entity's records, and every other marked field able
/// to be `none`.
fn own_record_problems(
    entity: &EntityModel,
    spans: &crate::data::EntitySpans,
    erasures: &Erasures,
    marked: &dyn Fn(&str) -> bool,
    subject: &dyn Fn(&str) -> bool,
) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    if !entity.person {
        return out;
    }
    let name = &entity.name;
    let one = words(name);
    let written = |field: &str| {
        spans
            .field_erasures
            .iter()
            .find(|(written, ..)| written == field)
    };
    for field in entity.fields.iter().filter(|f| f.reference.is_none()) {
        let (Some(erase), Some((_, key, _))) = (field.erase, written(&field.name)) else {
            continue;
        };
        let field_name = &field.name;
        if erase != Outcome::Retained {
            out.push(
                Diagnostic::new(
                    &rule::DATA_ERASE_OWN_FIELD,
                    format!(
                        "`{name}.{field_name}` is the person's own field, so only `Erasure.retained` means anything on it"
                    ),
                )
                .at(key.clone(), format!("`Erasure.{}`", erase.name()))
                .reason(format!(
                    "On a field of the person's own record, `erase:` says one thing: this field stays when they are erased. Every other marked field is cleared, and the record is deleted when nothing stays, so `Erasure.{}` says nothing a `{name}` doesn't already do.",
                    erase.name()
                ))
                .next(format!(
                    "Remove `erase:` from `{field_name}`, or say `erase: Erasure.retained` if the law makes you keep it"
                )),
            );
        } else if !marked(field_name) {
            out.push(
                Diagnostic::new(
                    &rule::DATA_ERASE_OWN_FIELD,
                    format!(
                        "`{name}.{field_name}` isn't marked `personal:`, so it stays without saying `erase:`"
                    ),
                )
                .at(key.clone(), "not marked")
                .reason(format!(
                    "Erasing a `{name}` clears the fields marked `personal:`, and leaves the others, so `erase: Erasure.retained` on `{field_name}` keeps nothing that wouldn't stay, and would keep the person's record for no reason."
                ))
                .next(format!(
                    "Mark `{field_name}` `personal:` if it is about the person, or remove `erase:`"
                )),
            );
        }
    }
    if !entity.keeps_erased() {
        return out;
    }
    let first = entity
        .fields
        .iter()
        .find(|field| field.reference.is_none() && field.erase == Some(Outcome::Retained));
    let (Some(first), Some(at)) = (
        first,
        first
            .and_then(|field| written(&field.name))
            .map(|(.., value)| value),
    ) else {
        return out;
    };
    let first_name = &first.name;
    if entity.retention.is_none() {
        out.push(
            Diagnostic::new(
                &rule::DATA_ERASE_RETENTION_REQUIRED,
                format!(
                    "`{name}` has no retention, so what {} {one}'s record keeps after erasure would never go",
                    an_or_a(&one)
                ),
            )
            .at(at.clone(), "keeps the record field by field")
            .reason(format!(
                "`Erasure.retained` on `{first_name}` keeps each {one}'s record when they are erased, for a reason the law gives, until its retention ends. `{name}` has no `retention:`, so it would be kept for ever."
            ))
            .next(format!(
                "Give `{name}` a `retention: {{ after: config.<setting> since: <field> }}` saying how long it is kept, or remove `erase:` from `{first_name}`"
            )),
        );
    }
    // A reference that deletes the record when someone else's records go
    // would take what must stay. One to its own entity only ever reaches
    // someone else this erases field by field too.
    for (index, field) in entity.fields.iter().enumerate() {
        let Some(reference) = &field.reference else {
            continue;
        };
        if reference.target == *name
            || outcome(field) != Some(Outcome::Deleted)
            || !erasures.finds(&reference.target)
        {
            continue;
        }
        let Some(span) = spans.fields.get(index) else {
            continue;
        };
        let (field_name, target) = (&field.name, &reference.target);
        out.push(
            Diagnostic::new(
                &rule::DATA_ERASE_OWN_FIELD,
                format!(
                    "`{name}.{field_name}` deletes a `{name}` that `{name}.{first_name}` says stays"
                ),
            )
            .at(span.clone(), format!("erasure deletes the `{name}` with its `{target}`"))
            .also(at.clone(), "keeps the record")
            .reason(format!(
                "`Erasure.retained` on `{first_name}` keeps each {one}'s record, because the law makes you keep it, and `{field_name}` deletes that record when erasure deletes the `{target}` it refers to."
            ))
            .next(format!(
                "Say `erase: Erasure.anonymised` on `{field_name}`, or remove `erase:` from `{first_name}`"
            )),
        );
    }
    for (index, field) in entity.fields.iter().enumerate() {
        if field.reference.is_none() && field.erase == Some(Outcome::Retained) {
            continue;
        }
        if field.nullable || !marked(&field.name) {
            continue;
        }
        let Some(span) = spans.fields.get(index) else {
            continue;
        };
        let (field_name, ty) = (&field.name, field.ty.strip_nullable());
        out.push(
            Diagnostic::new(
                &rule::DATA_ERASE_FIELD_REQUIRED,
                format!("Erasing a `{name}` can't clear `{name}.{field_name}`"),
            )
            .at(span.clone(), "personal data that can't be `none`")
            .also(at.clone(), "keeps the record without the person")
            .reason(format!(
                "`Erasure.retained` on `{first_name}` keeps each {one}'s record when they are erased, clearing every other marked field. `{field_name}` is marked `personal:`, and only an optional field (`{ty}?`) can be cleared{}.",
                if subject(field_name) {
                    format!(", which what a {one} signs in with can't be")
                } else {
                    String::new()
                }
            ))
            .next(if subject(field_name) {
                format!(
                    "Sign in with a field that holds nothing personal, or say `erase: Erasure.retained` on `{field_name}` if the law makes you keep it too"
                )
            } else {
                format!(
                    "Make `{field_name}` `{ty}?`, or say `erase: Erasure.retained` on it if the law makes you keep it too"
                )
            }),
        );
    }
    if let Some((index, field)) = entity
        .fields
        .iter()
        .enumerate()
        .find(|(_, field)| field.born == jadpo_std::ERASED_AT)
        && let Some(span) = spans.fields.get(index)
    {
        out.push(
            Diagnostic::new(
                &rule::DATA_ERASE_OWN_FIELD,
                format!(
                    "`{name}` has a field `{}`, the column it keeps the time someone was erased in",
                    field.name
                ),
            )
            .at(span.clone(), "the erasure's column")
            .reason(format!(
                "An entity that keeps a person's record after they are erased notes the time in a column called `{}`, so no field may be stored under that name.",
                jadpo_std::ERASED_AT
            ))
            .next(format!("Rename `{}`", field.name)),
        );
    }
    out
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
    let a = an_or_a(&one);
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
    let a = an_or_a(&one);
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

/// The indefinite article before `words`: `a` or `an`.
fn an_or_a(words: &str) -> &'static str {
    if words.starts_with(['a', 'e', 'i', 'o', 'u']) {
        "an"
    } else {
        "a"
    }
}
