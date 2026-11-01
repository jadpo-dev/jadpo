//! `retention: { after: since: }` (area 15, "Retention"; chunk P2): on a
//! field, the value is cleared once its time is up; on an entity, the
//! record is deleted with what cascades from it. The period is a
//! `Duration` setting (`after: config.signup_ip_retention`), never a
//! duration written in place, so each environment sets its own and the
//! personal-data report can show it, and `since:` names one of the
//! record's `Instant` fields, counted from when it holds a value. The
//! entity's built-in purge schedule applies it (area 12, "The purge
//! schedule"), in the plan `jadpo_plan` makes of it.
//!
//! Read once per retention by [`read`], once the entity's fields are
//! read, into the [`RetentionModel`] the schedule, the graph and the plan
//! read. What only the whole data model shows is [`retention_problems`]:
//! a record a `restricted` reference holds, which retention could never
//! delete (`data.retention_held`), and a deletion that would reach a
//! `File`'s bytes or someone who signs in through a cascade, neither of
//! which is removed yet (`toolchain.not_built_yet`). A field the
//! lifecycle owns or reads, a required field and a `File` field are
//! refused where the field is read ([`field_problems`]).

use std::collections::BTreeSet;

use jadpo_db::{Db, File, ItemKind, Project, item_tree};
use jadpo_diag::{Diagnostic, Span, nearest, rule};
use jadpo_std::OnDelete;
use jadpo_syntax::ast::{self, AstNode, Expr, MemberItem};

use super::{DataModel, EntityModel, FieldModel, entity_read};
use crate::lookups::Name;
use crate::tree::{self, dotted, node_span};
use crate::types::{Const, Prim, Ty, TypeView};

/// One `retention:`, as the schedule, the graph and the plan read it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct RetentionModel {
    /// As written: `config.signup_ip_retention`.
    pub after_written: String,
    /// The `Duration` setting that gives the period, whose value the
    /// application starts with (area 10, "Retention periods").
    pub setting: String,
    /// The `Instant` field it counts from.
    pub since: String,
}

impl RetentionModel {
    /// `after config.signup_ip_retention since created_at`: how the
    /// manifest and the editor show it.
    pub fn shown(&self) -> String {
        format!("after {} since {}", self.after_written, self.since)
    }
}

/// What a retention is on: a field, by name, or the whole entity.
#[derive(Clone, Copy, Debug)]
pub(super) enum On<'a> {
    Field(&'a str),
    Entity,
    /// An entity's `audit: { retention: }` (area 16; chunk B11): how long
    /// each entry of its history is kept.
    Audit,
}

const TAKES: &str = "`retention:` says how long a value or record is kept, `after:`, a `Duration` setting, counted from one of the record's `Instant` fields, `since:`.";

/// Reads `retention: { ... }` on `entity`, whose fields are read already.
#[allow(clippy::too_many_arguments)]
pub(super) fn read(
    lookup: &dyn TypeView,
    settings: &dyn Fn(&str) -> Option<crate::config::SettingFact>,
    path: &str,
    entity: &EntityModel,
    on: On<'_>,
    member: &ast::Member,
    key_span: &Span,
    out: &mut Vec<Diagnostic>,
) -> Option<RetentionModel> {
    let Some(block) = member.block() else {
        let at = member
            .value()
            .map_or_else(|| key_span.clone(), |value| node_span(path, value.syntax()));
        out.push(
            Diagnostic::new(
                &rule::TYPE_SETTING_VALUE,
                "`retention:` takes `{ after: since: }`",
            )
            .at(at, "not a retention block")
            .reason(TAKES)
            .next("Write `retention: { after: config.<setting> since: <field> }`"),
        );
        return None;
    };
    let mut after = None;
    let mut since = None;
    let mut seen: Vec<(String, Span)> = Vec::new();
    for item in block.member_items() {
        let (word, node) = match &item {
            MemberItem::Member(found) => match found.member_key() {
                Some(key) => (tree::written(key.syntax()), key.syntax().clone()),
                None => continue,
            },
            MemberItem::Elision(_) => continue,
            other => (tree::written(other.syntax()), other.syntax().clone()),
        };
        let span = node_span(path, &node);
        if let Some((_, first)) = seen.iter().find(|(name, _)| *name == word) {
            out.push(
                Diagnostic::new(
                    &rule::TYPE_CONSTRAINT_REPEATED,
                    format!("`{word}` is given twice"),
                )
                .at(span, "given again")
                .also(first.clone(), "first given here")
                .reason("Each part of a retention is given once, so there is never a question of which counts.")
                .next(format!("Remove one `{word}:`")),
            );
            continue;
        }
        seen.push((word.clone(), span.clone()));
        match (&item, word.as_str()) {
            (MemberItem::Member(found), "after") => after = Some(found.clone()),
            (MemberItem::Member(found), "since") => since = Some(found.clone()),
            _ => {
                let near = nearest(&word, ["after", "since"]);
                let problem = Diagnostic::new(
                    &rule::TYPE_UNKNOWN_SETTING,
                    format!("`{word}` isn't part of a retention"),
                )
                .at(span.clone(), "not a part of `retention:`")
                .reason(match near {
                    Some(near) => format!("{TAKES} Did you mean `{near}`?"),
                    None => TAKES.to_string(),
                });
                out.push(match near {
                    Some(near) => problem.next(format!("Use `{near}`")).edit(span, near),
                    None => problem.next(format!("Remove `{word}` from the retention")),
                });
            }
        }
    }
    let after = match after {
        Some(found) => period(lookup, settings, path, entity, on, &found, out),
        None => {
            out.push(
                Diagnostic::new(
                    &rule::DATA_RETENTION_PERIOD,
                    "The retention has no `after:`",
                )
                .at(key_span.clone(), "no `after:`")
                .reason(TAKES)
                .next(format!(
                    "Add `after: config.{}`, a `Duration` setting in `config.jadpo`",
                    setting_name(entity, on)
                )),
            );
            None
        }
    };
    let since = match since {
        Some(found) => since_field(lookup, path, entity, &found, out),
        None => {
            out.push(
                Diagnostic::new(&rule::DATA_RETENTION_SINCE, "The retention has no `since:`")
                    .at(key_span.clone(), "no `since:`")
                    .reason(TAKES)
                    .next("Add `since: <field>`, naming one of the record's `Instant` fields"),
            );
            None
        }
    };
    let (after_written, setting) = after?;
    Some(RetentionModel {
        after_written,
        setting,
        since: since?,
    })
}

/// The setting a retention's period might be called: `signup_ip_retention`
/// for a field, `invoice_retention` for an entity.
fn setting_name(entity: &EntityModel, on: On<'_>) -> String {
    match on {
        On::Field(field) => format!("{field}_retention"),
        On::Entity => format!("{}_retention", crate::case::to_snake(&entity.name)),
        On::Audit => format!("{}_audit_retention", crate::case::to_snake(&entity.name)),
    }
}

/// `after:`: a `Duration` setting that isn't secret, as written and by
/// name.
pub(super) fn period(
    lookup: &dyn TypeView,
    settings: &dyn Fn(&str) -> Option<crate::config::SettingFact>,
    path: &str,
    entity: &EntityModel,
    on: On<'_>,
    member: &ast::Member,
    out: &mut Vec<Diagnostic>,
) -> Option<(String, String)> {
    let value = member.value()?;
    let span = node_span(path, value.syntax());
    let written = shown(value.syntax());
    let name = setting_name(entity, on);
    if let Some((first, rest)) = dotted(&value)
        && tree::name_ref_text(&first).as_deref() == Some("config")
        && let [setting] = rest.as_slice()
        && let Some(setting) = tree::name_ref_text(setting)
    {
        // A setting that doesn't exist is reported where it is read, and a
        // secret in the wrong place as such, once.
        let fact = settings(&setting)?;
        if fact.secret {
            return None;
        }
        if lookup.base_prim(&fact.ty) != Some(jadpo_std::Builtin::Duration) {
            out.push(
                Diagnostic::new(
                    &rule::DATA_RETENTION_PERIOD,
                    format!("`after: {written}` isn't a duration"),
                )
                .at(span, "not a `Duration` setting")
                .reason(format!(
                    "`after:` is how long a value or record is kept, a `Duration` setting; `{setting}` is `{}`.",
                    fact.ty
                ))
                .next(format!(
                    "Make `{setting}` a `Duration` in `config.jadpo`, such as `{setting}: Duration {{ default: 30d }}`"
                )),
            );
            return None;
        }
        return Some((written, setting));
    }
    let in_place = matches!(Const::from_expr(&value), Const::Duration(_));
    let (title, label) = if in_place {
        (
            format!("`after: {written}` is written in place"),
            "a duration, not a setting",
        )
    } else {
        (
            format!("`after: {written}` isn't a setting"),
            "not a setting",
        )
    };
    let example = if in_place { written.as_str() } else { "30d" };
    out.push(
        Diagnostic::new(&rule::DATA_RETENTION_PERIOD, title)
            .at(span, label)
            .reason("A retention period is a `Duration` setting in `config.jadpo`, so each environment sets its own and the personal-data report can say how long each kind of data is kept.")
            .next(format!(
                "Add `{name}: Duration {{ default: {example} }}` to `config.jadpo`, and write `after: config.{name}`"
            )),
    );
    None
}

/// `since:`: one of the record's `Instant` fields, by name.
fn since_field(
    lookup: &dyn TypeView,
    path: &str,
    entity: &EntityModel,
    member: &ast::Member,
    out: &mut Vec<Diagnostic>,
) -> Option<String> {
    let value = member.value()?;
    let span = node_span(path, value.syntax());
    let Expr::NameRef(name) = unparen(&value) else {
        out.push(
            Diagnostic::new(
                &rule::DATA_RETENTION_SINCE,
                format!("`since: {}` isn't a field", shown(value.syntax())),
            )
            .at(span, "not a field")
            .reason(format!(
                "`since:` names one of `{}`'s `Instant` fields bare, as a lifecycle names them: a value or record goes once that time is longer ago than `after:`. Its `Instant` fields are {}.",
                entity.name,
                instants(lookup, entity)
            ))
            .next("Name an `Instant` field of the record"),
        );
        return None;
    };
    let name = tree::name_ref_text(&name)?;
    let Some(field) = entity.fields.iter().find(|field| field.name == name) else {
        let names: Vec<&str> = entity.fields.iter().map(|f| f.name.as_str()).collect();
        let near = nearest(&name, names.iter().copied());
        let problem = Diagnostic::new(
            &rule::DATA_RETENTION_SINCE,
            format!("`{}` has no field `{name}`", entity.name),
        )
        .at(span.clone(), "not declared")
        .reason(format!(
            "`since:` names one of `{}`'s `Instant` fields, which are {}.{}",
            entity.name,
            instants(lookup, entity),
            near.map(|n| format!(" Did you mean `{n}`?"))
                .unwrap_or_default()
        ));
        out.push(match near {
            Some(near) => problem
                .next(format!("Use `{near}` instead of `{name}`"))
                .edit(span, near),
            None => problem.next("Name an `Instant` field of the record"),
        });
        return None;
    };
    if field.ty.has_error() {
        return None;
    }
    if lookup.representation(&field_ty(entity, field)) != Some(Prim::Instant) {
        out.push(
            Diagnostic::new(&rule::DATA_RETENTION_SINCE, format!("`{name}` isn't a time"))
                .at(span, format!("a `{}`", field.ty.strip_nullable()))
                .reason(format!(
                    "`since:` names the `Instant` field a retention counts from: a value or record goes once that time is longer ago than `after:`. `{}`'s `Instant` fields are {}.",
                    entity.name,
                    instants(lookup, entity)
                ))
                .next("Name an `Instant` field of the record"),
        );
        return None;
    }
    Some(name)
}

/// The entity's `Instant` fields, listed for a message: "`created_at` and
/// `issued_at`", or "none".
fn instants(lookup: &dyn TypeView, entity: &EntityModel) -> String {
    let found: Vec<String> = entity
        .fields
        .iter()
        .filter(|field| {
            !field.ty.has_error()
                && lookup.representation(&field_ty(entity, field)) == Some(Prim::Instant)
        })
        .map(|field| format!("`{}`", field.name))
        .collect();
    if found.is_empty() {
        "none".to_string()
    } else {
        crate::and_list(&found)
    }
}

fn field_ty(entity: &EntityModel, field: &FieldModel) -> Ty {
    let ty = Ty::Field(entity.name.clone(), vec![field.name.clone()]);
    if field.nullable { ty.nullable() } else { ty }
}

/// The problems of a field's `retention:` that its field shows: one that
/// can't be `none`, one the lifecycle owns or reads, and a `File`, whose
/// bytes nothing deletes yet. `lifecycle_span` is where the lifecycle is
/// written.
pub(super) fn field_problems(
    entity: &EntityModel,
    field: &FieldModel,
    file: bool,
    key_span: &Span,
    lifecycle_span: Option<&Span>,
    out: &mut Vec<Diagnostic>,
) -> bool {
    let (name, field_name) = (&entity.name, &field.name);
    if file {
        out.push(
            Diagnostic::new(
                &rule::TOOLCHAIN_NOT_BUILT_YET,
                format!("Clearing `{name}.{field_name}` when its time is up isn't built yet"),
            )
            .at(key_span.clone(), "decided, not built")
            .reason(format!(
                "`{field_name}` is a `File`, and clearing it must delete its bytes from the store they are in, which isn't built yet (area 16, \"Deleting and erasing\"): accepted, the bytes would be kept for ever."
            ))
            .next(format!("Leave `retention:` off `{field_name}` for now")),
        );
        return false;
    }
    if !field.nullable {
        let ty = field.ty.strip_nullable();
        out.push(
            Diagnostic::new(
                &rule::DATA_RETENTION_FIELD_REQUIRED,
                format!("`{name}.{field_name}` can't be cleared"),
            )
            .at(key_span.clone(), "on a field that can't be `none`")
            .reason(format!(
                "A field's `retention:` clears its value when its time is up, which only an optional field (`{ty}?`) can be without; `retention:` on the entity deletes the whole record instead."
            ))
            .next(format!("Make it `{ty}?`")),
        );
        return false;
    }
    if let Some(lifecycle) = &entity.lifecycle {
        let mut read = BTreeSet::new();
        if let Some(visible) = &lifecycle.visible {
            visible.fields(&mut read);
        }
        for transition in &lifecycle.transitions {
            if let Some(when) = &transition.when {
                when.fields(&mut read);
            }
        }
        if let Some(purge) = &lifecycle.purge {
            read.insert(purge.since.clone());
        }
        let owned = lifecycle.owns(field_name);
        if owned || read.contains(field_name) {
            let what = if read.contains(field_name) {
                "its conditions read it, so clearing it would change which records are hidden or what a transition allows"
            } else {
                "only its transitions change it, so clearing it would undo what one of them set"
            };
            let mut problem = Diagnostic::new(
                &rule::DATA_RETENTION_LIFECYCLE_FIELD,
                format!("Clearing `{name}.{field_name}` would change its lifecycle"),
            )
            .at(key_span.clone(), "clears a field of the lifecycle's");
            if let Some(at) = lifecycle_span {
                problem = problem.also(at.clone(), "the lifecycle");
            }
            out.push(
                problem
                    .reason(format!(
                        "A field's `retention:` clears its value when its time is up. `{field_name}` belongs to `{name}`'s lifecycle: {what}."
                    ))
                    .next(format!(
                        "Take `retention:` off `{field_name}`, and say when the record goes with `retention:` on `{name}` or the lifecycle's `purge:`"
                    )),
            );
            return false;
        }
    }
    true
}

/// Area 15's retention problems in one file that only the whole data
/// model shows, for each entity it declares with a `retention:`: a record
/// a `restricted` reference holds, directly or through what deleting it
/// cascades to, which retention could never delete; and a deletion that
/// would reach a `File`'s bytes, or a person who signs in through a
/// cascade, neither of which is removed yet.
#[salsa::tracked(returns(ref))]
pub fn retention_problems(db: &dyn Db, project: Project, file: File) -> Vec<Diagnostic> {
    let path = file.path(db);
    let model = super::data_model(db, project);
    let identity = crate::identity::identity(db, project);
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
        let (true, Some(span)) = (
            entity.path == *path && entity.retention.is_some(),
            read.spans.retention.as_ref(),
        ) else {
            continue;
        };
        let reach = reach(model, &entity.name);
        let name = &entity.name;
        let one = format!("{} {}", indefinite(name), words(name));
        if let Some((holder, field, held)) = reach.holds.first() {
            let through = if held == name {
                String::new()
            } else {
                format!(" (`{held}`, which deleting {one} cascades to)")
            };
            let mut problem = Diagnostic::new(
                &rule::DATA_RETENTION_HELD,
                format!(
                    "`{name}`'s retention can't delete {one} {} `{holder}` holds",
                    indefinite(holder)
                ),
            )
            .at(
                span.clone(),
                format!("deletes each {} when its time is up", words(name)),
            );
            for (holder, field, _) in &reach.holds {
                if let Some(at) = field_span(db, project, holder, field) {
                    problem =
                        problem.also(at, format!("`{holder}.{field}` is `OnDelete.restricted`"));
                }
            }
            out.push(
                problem
                    .reason(format!(
                        "Retention deletes each record when its time is up, with what cascades from it. `{holder}.{field}` is `OnDelete.restricted`, which refuses to delete the record it refers to{through}, so {one} it holds would be kept past its retention."
                    ))
                    .next(format!(
                        "Say what deleting the record does to `{holder}`: `OnDelete.cascade` on `{field}` deletes it too, and `OnDelete.cleared` keeps it without the reference"
                    )),
            );
            continue;
        }
        if let Some((holder, field)) = reach.files.first() {
            let what = if holder == name {
                format!("`{name}.{field}` is a `File`")
            } else {
                format!("deleting {one} cascades to `{holder}`, whose `{field}` is a `File`")
            };
            out.push(
                Diagnostic::new(
                    &rule::TOOLCHAIN_NOT_BUILT_YET,
                    format!("Deleting `{name}` when its time is up isn't built yet"),
                )
                .at(span.clone(), "decided, not built")
                .reason(format!(
                    "Retention deletes each record with what cascades from it, and {what}, whose bytes must be deleted from the store they are in, which isn't built yet (area 16, \"Deleting and erasing\"): accepted, the bytes would be kept for ever."
                ))
                .next(format!("Leave `retention:` out of `{name}` for now")),
            );
            continue;
        }
        let signs_in = reach.entities.iter().skip(1).find(|reached| {
            identity
                .variants
                .iter()
                .any(|variant| variant.entity == **reached)
        });
        if let Some(reached) = signs_in {
            out.push(
                Diagnostic::new(
                    &rule::TOOLCHAIN_NOT_BUILT_YET,
                    format!("Deleting `{name}` when its time is up isn't built yet"),
                )
                .at(span.clone(), "decided, not built")
                .reason(format!(
                    "Retention deletes each record with what cascades from it, and deleting a `{name}` cascades to `{reached}`, who signs in. Retention on `{reached}` itself removes their sessions, passkeys and sign-in links with them; reached through a cascade it can't yet: accepted, those would be kept for ever."
                ))
                .next(format!(
                    "Leave `retention:` out of `{name}` for now, or give `{reached}` a `retention:` of its own"
                )),
            );
        }
    }
    out
}

/// `invoice line` for `InvoiceLine`: an entity's name as words.
fn words(entity: &str) -> String {
    crate::case::to_snake(entity).replace('_', " ")
}

/// The article before an entity's name as words or as written.
fn indefinite(entity: &str) -> &'static str {
    if entity.starts_with(['A', 'E', 'I', 'O', 'U']) {
        "an"
    } else {
        "a"
    }
}

/// Where `entity.field` is written.
fn field_span(db: &dyn Db, project: Project, entity: &str, field: &str) -> Option<Span> {
    let read = entity_read(db, project, Name::new(db, entity.to_string())).as_ref()?;
    let at = read.model.fields.iter().position(|f| f.name == field)?;
    read.spans.fields.get(at).cloned()
}

/// What deleting a record of an entity reaches.
#[derive(Debug, Default)]
pub struct Reach {
    /// Every entity deleting one cascades to, the entity's own first.
    pub entities: Vec<String>,
    /// Each `restricted` reference that would hold one: its entity, its
    /// field and the entity it refers to.
    pub holds: Vec<(String, String, String)>,
    /// Each `File` field among them: its entity and the field.
    pub files: Vec<(String, String)>,
}

/// What deleting a record of `entity` reaches through every reference that
/// cascades, however deep, each entity once: those entities, the
/// `restricted` references that hold one of them and their `File` fields.
pub fn reach(model: &DataModel, entity: &str) -> Reach {
    let mut out = Reach {
        entities: vec![entity.to_string()],
        ..Reach::default()
    };
    let mut at = 0;
    while let Some(current) = out.entities.get(at).cloned() {
        if let Some(found) = model.entity(&current) {
            for field in &found.fields {
                if field.file.is_some() {
                    out.files.push((current.clone(), field.name.clone()));
                }
            }
        }
        for child in &model.entities {
            for field in &child.fields {
                let Some(reference) = &field.reference else {
                    continue;
                };
                if reference.target != current {
                    continue;
                }
                match reference.on_delete {
                    Some(OnDelete::Cascade) => {
                        if !out.entities.contains(&child.name) {
                            out.entities.push(child.name.clone());
                        }
                    }
                    Some(OnDelete::Restricted) => {
                        out.holds
                            .push((child.name.clone(), field.name.clone(), current.clone()))
                    }
                    // `None` is reported where the reference is written.
                    Some(OnDelete::Cleared) | None => {}
                }
            }
        }
        at += 1;
    }
    out
}

fn unparen(expr: &Expr) -> Expr {
    let mut current = expr.clone();
    while let Expr::ParenExpr(paren) = &current {
        match paren.expr() {
            Some(inner) => current = inner,
            None => break,
        }
    }
    current
}

/// Source text for a message, on one line.
fn shown(node: &jadpo_syntax::SyntaxNode) -> String {
    node.text()
        .to_string()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
