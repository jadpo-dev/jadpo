//! `audit: { retention: { after: } }` on an entity (area 16, "Audit
//! trail"; chunk B11): every change to one of its records is kept in its
//! history, written in the transaction that makes the change, and read
//! back with the generated `Entity.history(id)`. `retention:` says how
//! long each entry is kept, a `Duration` setting as area 15's periods
//! are, always counted from the entry's own time, so it names no
//! `since:`. Without it an entry is kept until its record is erased.
//!
//! Read once per entity by [`read`], once its fields are, into the
//! [`AuditModel`] the schema, the purge schedule, the graph, the plan and
//! the permissions report read. What only the whole data model shows is
//! [`audit_problems`], each `toolchain.not_built_yet` where `audit:` is
//! written, so nothing is copied into a history that the pages don't say
//! how to keep or remove:
//!
//! - a field marked `personal:`, or of a standard type that is personal
//!   data whatever holds it, which the history keeps as a keyed hash, as
//!   logs do, and that hash isn't built (area 15, "Marked fields in
//!   logs");
//! - a `File` field, whose value is a stored file's bytes;
//! - a field with a `retention:` of its own, whose value the history
//!   would keep past it;
//! - an entity whose records belong to a person and that something other
//!   than erasure removes (its own `retention:`, its lifecycle's `purge:`,
//!   or a cascade from another entity's): its history would outlive the
//!   record, beyond the reach of the person's erasure, which deletes a
//!   record's entries with the record.
//!
//! A `delete` in code that would remove or change an audited record
//! through a cascade, and an erasure that would keep an audited record,
//! are refused where they are written (`crate::types` and
//! `crate::erasure`).

use std::collections::BTreeSet;

use jadpo_db::{Db, File, ItemKind, Project, item_tree};
use jadpo_diag::{Diagnostic, Span, nearest, rule};
use jadpo_syntax::ast::{self, AstNode, MemberItem};

use super::retention::{On, period};
use super::{EntityModel, entity_read};
use crate::lookups::Name;
use crate::tree::{self, node_span};
use crate::types::TypeView;

/// One entity's `audit:`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct AuditModel {
    /// How long each entry is kept; `None` keeps it until its record is
    /// erased.
    pub retention: Option<AuditRetention>,
}

/// `audit: { retention: { after: } }`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct AuditRetention {
    /// As written: `config.audit_retention`.
    pub after_written: String,
    /// The `Duration` setting that gives the period.
    pub setting: String,
}

impl AuditModel {
    /// How the manifest and the editor show it: `kept after
    /// config.audit_retention`, or `kept until erased`.
    pub fn shown(&self) -> String {
        match &self.retention {
            Some(retention) => format!("kept after {}", retention.after_written),
            None => "kept until erased".to_string(),
        }
    }
}

const TAKES: &str = "`audit:` keeps a history of every change to the entity's records; its `retention:` says how long each entry is kept, `after:`, a `Duration` setting, counted from when the entry was written.";

/// Reads `audit: { ... }` on `entity`, whose fields are read already: its
/// model, and where its `retention:` is written when that reads cleanly.
pub(super) fn read(
    lookup: &dyn TypeView,
    settings: &dyn Fn(&str) -> Option<crate::config::SettingFact>,
    path: &str,
    entity: &EntityModel,
    member: &ast::Member,
    key_span: &Span,
    out: &mut Vec<Diagnostic>,
) -> Option<(AuditModel, Option<Span>)> {
    let Some(block) = member.block() else {
        let at = member
            .value()
            .map_or_else(|| key_span.clone(), |value| node_span(path, value.syntax()));
        out.push(
            Diagnostic::new(&rule::TYPE_SETTING_VALUE, "`audit:` takes `{ retention: }`")
                .at(at, "not an audit block")
                .reason(TAKES)
                .next("Write `audit: { retention: { after: config.<setting> } }`, or `audit: {}` to keep entries until their record is erased"),
        );
        return None;
    };
    let mut model = AuditModel::default();
    let mut retention_span = None;
    let mut seen: Vec<(String, Span)> = Vec::new();
    for (word, span, item) in parts(path, &block) {
        if repeated(&mut seen, &word, &span, "an audit", out) {
            continue;
        }
        match (&item, word.as_str()) {
            (MemberItem::Member(found), "retention") => {
                let read = retention(lookup, settings, path, entity, found, &span, out);
                if read.is_some() {
                    retention_span = Some(span);
                }
                model.retention = read;
            }
            _ => unknown(&word, span, &["retention"], "`audit:`", out),
        }
    }
    Some((model, retention_span))
}

/// `audit: { retention: { after: } }`'s period, read as area 15's are.
fn retention(
    lookup: &dyn TypeView,
    settings: &dyn Fn(&str) -> Option<crate::config::SettingFact>,
    path: &str,
    entity: &EntityModel,
    member: &ast::Member,
    key_span: &Span,
    out: &mut Vec<Diagnostic>,
) -> Option<AuditRetention> {
    let Some(block) = member.block() else {
        let at = member
            .value()
            .map_or_else(|| key_span.clone(), |value| node_span(path, value.syntax()));
        out.push(
            Diagnostic::new(
                &rule::TYPE_SETTING_VALUE,
                "An audit's `retention:` takes `{ after: }`",
            )
            .at(at, "not a retention block")
            .reason(TAKES)
            .next("Write `retention: { after: config.<setting> }`"),
        );
        return None;
    };
    let mut after = None;
    let mut seen: Vec<(String, Span)> = Vec::new();
    for (word, span, item) in parts(path, &block) {
        if repeated(&mut seen, &word, &span, "a retention", out) {
            continue;
        }
        match (&item, word.as_str()) {
            (MemberItem::Member(found), "after") => after = Some(found.clone()),
            (MemberItem::Member(found), "since") => {
                let removal = tree::removal(path, found);
                out.push(
                    Diagnostic::new(
                        &rule::TYPE_UNKNOWN_SETTING,
                        "An audit's retention counts from each entry's own time",
                    )
                    .at(span, "not a part of an audit's `retention:`")
                    .reason("Each entry of a history is kept for `after:` from when it was written, so an audit's `retention:` names no `since:` field.")
                    .next("Remove `since:`")
                    .edit(removal, ""),
                );
            }
            _ => unknown(&word, span, &["after"], "an audit's `retention:`", out),
        }
    }
    let Some(after) = after else {
        out.push(
            Diagnostic::new(
                &rule::DATA_RETENTION_PERIOD,
                "The retention has no `after:`",
            )
            .at(key_span.clone(), "no `after:`")
            .reason(TAKES)
            .next(format!(
                "Add `after: config.{}_audit_retention`, a `Duration` setting in `config.jadpo`",
                crate::case::to_snake(&entity.name)
            )),
        );
        return None;
    };
    let (after_written, setting) = period(lookup, settings, path, entity, On::Audit, &after, out)?;
    Some(AuditRetention {
        after_written,
        setting,
    })
}

/// A block's parts, each by its word and where it is written.
fn parts(path: &str, block: &ast::MemberList) -> Vec<(String, Span, MemberItem)> {
    let mut out = Vec::new();
    for item in block.member_items() {
        let (word, node) = match &item {
            MemberItem::Member(found) => match found.member_key() {
                Some(key) => (tree::written(key.syntax()), key.syntax().clone()),
                None => continue,
            },
            MemberItem::Elision(_) => continue,
            other => (tree::written(other.syntax()), other.syntax().clone()),
        };
        out.push((word, node_span(path, &node), item));
    }
    out
}

/// Whether `word` was given already, reporting it if so.
fn repeated(
    seen: &mut Vec<(String, Span)>,
    word: &str,
    span: &Span,
    what: &str,
    out: &mut Vec<Diagnostic>,
) -> bool {
    if let Some((_, first)) = seen.iter().find(|(name, _)| name == word) {
        out.push(
            Diagnostic::new(
                &rule::TYPE_CONSTRAINT_REPEATED,
                format!("`{word}` is given twice"),
            )
            .at(span.clone(), "given again")
            .also(first.clone(), "first given here")
            .reason(format!(
                "Each part of {what} is given once, so there is never a question of which counts."
            ))
            .next(format!("Remove one `{word}:`")),
        );
        return true;
    }
    seen.push((word.to_string(), span.clone()));
    false
}

fn unknown(word: &str, span: Span, takes: &[&str], what: &str, out: &mut Vec<Diagnostic>) {
    let near = nearest(word, takes.iter().copied());
    let problem = Diagnostic::new(
        &rule::TYPE_UNKNOWN_SETTING,
        format!("`{word}` isn't part of {what}"),
    )
    .at(span.clone(), format!("not a part of {what}"))
    .reason(match near {
        Some(near) => format!("{TAKES} Did you mean `{near}`?"),
        None => TAKES.to_string(),
    });
    out.push(match near {
        Some(near) => problem.next(format!("Use `{near}`")).edit(span, near),
        None => problem.next(format!("Remove `{word}` from {what}")),
    });
}

/// Area 16's audit problems in one file that only the whole data model
/// shows, for each entity it declares with an `audit:`, each refused as
/// not built yet where `audit:` is written (see the module's comment).
#[salsa::tracked(returns(ref))]
pub fn audit_problems(db: &dyn Db, project: Project, file: File) -> Vec<Diagnostic> {
    let path = file.path(db);
    let model = super::data_model(db, project);
    let erasures = crate::erasure::erasures(db, project);
    let marks = crate::ai_personal::marks(db, project);
    let lookup = crate::lookups::Lookup::for_file(db, project, file);
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
            entity.path == *path && entity.audit.is_some(),
            read.spans.audit.as_ref(),
        ) else {
            continue;
        };
        let name = &entity.name;
        // Its generated `history` takes the name, as a person's `erase`
        // does.
        if entity
            .lifecycle
            .as_ref()
            .is_some_and(|lifecycle| lifecycle.transition(jadpo_std::audit::HISTORY).is_some())
        {
            out.push(
                Diagnostic::new(
                    &rule::DATA_LIFECYCLE_TRANSITION_NAME,
                    "A transition can't be called `history`",
                )
                .at(span.clone(), "keeps a history")
                .reason(format!(
                    "`{name}` is audited, and `{name}.history` is the query area 16 generates for it, so a transition can't take its name."
                ))
                .next("Rename the transition"),
            );
        }
        let unbuilt =
            |title: String, also: Option<(Span, String)>, reason: String, next: String| {
                let mut problem = Diagnostic::new(&rule::TOOLCHAIN_NOT_BUILT_YET, title)
                    .at(span.clone(), "decided, not built");
                if let Some((at, label)) = also {
                    problem = problem.also(at, label);
                }
                problem.reason(reason).next(next)
            };
        let field_at = |field: &str| {
            entity
                .fields
                .iter()
                .position(|f| f.name == field)
                .and_then(|at| read.spans.fields.get(at).cloned())
        };
        // A value the history would keep that the pages say it keeps only
        // as a keyed hash, or not at all yet.
        let mut found = None;
        for field in &entity.fields {
            let marked = marks
                .fields
                .get(&(name.clone(), field.name.clone()))
                .and_then(|mark| mark.category.clone());
            let standard = crate::ai_personal::standard_personal(&lookup, &field.ty);
            let why = if let Some(category) = marked {
                Some((
                    format!("`{}` is marked `PersonalData.{category}`", field.name),
                    "marked personal".to_string(),
                ))
            } else if let Some((ty, category)) = standard.first() {
                Some((
                    format!(
                        "`{}` is a `{ty}`, personal data (`PersonalData.{category}`) whatever holds it",
                        field.name
                    ),
                    format!("a `{ty}`"),
                ))
            } else {
                None
            };
            if let Some((what, label)) = why {
                found = Some(unbuilt(
                    format!("Auditing `{name}` isn't built yet"),
                    field_at(&field.name).map(|at| (at, label)),
                    format!(
                        "{what}, and its history keeps a personal value only as a keyed hash, as logs do, which isn't built yet (area 15, \"Marked fields in logs\"): accepted, the value would be copied into every entry."
                    ),
                    format!("Leave `audit:` out of `{name}` for now"),
                ));
                break;
            }
            if field.file.is_some() {
                found = Some(unbuilt(
                    format!("Auditing `{name}` isn't built yet"),
                    field_at(&field.name).map(|at| (at, "a `File`".to_string())),
                    format!(
                        "`{}` is a `File`, and what its history keeps of a stored file isn't decided: accepted, the entry would hold the file's place in its store.",
                        field.name
                    ),
                    format!("Leave `audit:` out of `{name}` for now"),
                ));
                break;
            }
            if field.retention.is_some() {
                found = Some(unbuilt(
                    format!("Auditing `{name}` isn't built yet"),
                    field_at(&field.name).map(|at| (at, "cleared when its time is up".to_string())),
                    format!(
                        "`{}` has a `retention:` of its own, and its history would keep each of its values for as long as the entry: accepted, a value retention clears would outlive it.",
                        field.name
                    ),
                    format!(
                        "Leave `audit:` out of `{name}` for now, or take `retention:` off `{}`",
                        field.name
                    ),
                ));
                break;
            }
        }
        if let Some(problem) = found {
            out.push(problem);
            continue;
        }
        // A person's record whose history would outlive it beyond the
        // reach of their erasure.
        if !erasures.reaches(entity) {
            continue;
        }
        let one = format!("{} {}", indefinite(name), words(name));
        let removes = if entity.retention.is_some() {
            Some(format!(
                "its `retention:` deletes {one} when its time is up"
            ))
        } else if entity
            .lifecycle
            .as_ref()
            .is_some_and(|lifecycle| lifecycle.purge.is_some())
        {
            Some(format!(
                "its lifecycle's `purge:` deletes {one} once it has been hidden long enough"
            ))
        } else {
            model
                .entities
                .iter()
                .filter(|other| other.name != *name)
                .filter(|other| {
                    other.retention.is_some()
                        || other
                            .lifecycle
                            .as_ref()
                            .is_some_and(|lifecycle| lifecycle.purge.is_some())
                })
                .find(|other| {
                    super::retention::reach(model, &other.name)
                        .entities
                        .contains(name)
                })
                .map(|other| {
                    format!(
                        "deleting {} {} when its time is up deletes the `{name}` records it cascades to",
                        indefinite(&other.name),
                        words(&other.name)
                    )
                })
        };
        if let Some(removes) = removes {
            out.push(unbuilt(
                format!("Auditing `{name}` isn't built yet"),
                None,
                format!(
                    "A `{name}` belongs to a person, and {removes}. Its history would be kept after it, where erasing the person, which deletes a record's history with the record, can't reach it: accepted, data of theirs would outlive their erasure."
                ),
                format!("Leave `audit:` out of `{name}` for now"),
            ));
        }
    }
    out
}

/// `invoice line` for `InvoiceLine`: an entity's name as words.
fn words(entity: &str) -> String {
    crate::case::to_snake(entity).replace('_', " ")
}

fn indefinite(entity: &str) -> &'static str {
    if entity.starts_with(['A', 'E', 'I', 'O', 'U']) {
        "an"
    } else {
        "a"
    }
}
