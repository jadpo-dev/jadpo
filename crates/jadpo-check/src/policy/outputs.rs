//! Policy on what a query returns (area 9, "Field restrictions"): an output
//! type has one shape for everyone, so every caller who may read a query's
//! records may read each field its output carries, and the parent record
//! of every include, or the query doesn't compile
//! (`policy.output_field_unproved`). Which records pass a condition, and
//! the order they come in, show the fields they test too, so every field a
//! `where:`, `where_present:` or `order:` names, the query's own or an
//! include's, is one every caller who reaches those records may read
//! (area 9, "Field restrictions").
//!
//! A query's records are the ones its entity's `read:` lets the caller see,
//! a child's those its own entity's `read:` lets in, and a parent's those
//! both the record that refers to it and the parent's own `read:` let in
//! ([`super::proof`]). A query whose result is records or another object,
//! rather than an `Output`, carries what it reads to an action, which
//! [`super::flow`] follows; its includes' parents are proved all the same,
//! since a parent the caller may not read could only fail the request.

use jadpo_db::{Db, File, ItemKind, Project, item_tree};
use jadpo_diag::{Diagnostic, Span, rule};
use jadpo_syntax::ast::{self, AstNode, Decl, MemberItem};

use super::proof::{Prover, Reached, Way};
use crate::data::queries::{
    Condition, Element, Include, IncludeKind, OrderField, QueryModel, Source, query_read,
};
use crate::lookups::{Lookup, Name};
use crate::tree::{self, node_span};
use crate::types::{Def, FieldsBase, TypeView};

/// The policy problems of the queries `file` declares.
#[salsa::tracked(returns(ref))]
pub fn query_policy_problems(db: &dyn Db, project: Project, file: File) -> Vec<Diagnostic> {
    if !super::policy_clean(db, project) {
        return Vec::new();
    }
    let path = file.path(db);
    let mut out = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for item in &item_tree(db, file).items {
        if item.kind != ItemKind::Query || !seen.insert(item.name.as_str()) {
            continue;
        }
        let Some(read) = query_read(db, project, Name::new(db, item.name.clone())) else {
            continue;
        };
        if read.path != *path || !read.problems.is_empty() {
            continue;
        }
        let Some(model) = &read.model else { continue };
        out.extend(query(db, project, file, item.index, model));
    }
    out.extend(visible_problems(db, project, file));
    out.extend(backfill_problems(db, project, file));
    out
}

/// A lifecycle's `visible:` is a condition on every read of its entity,
/// so which records a caller sees shows the fields it tests: each one is
/// a field everyone the entity's `read:` lets in may read (area 9, "Field
/// restrictions", "Conditions and orders").
fn visible_problems(db: &dyn Db, project: Project, file: File) -> Vec<Diagnostic> {
    let path = file.path(db);
    let mut out = Vec::new();
    let prover = Prover::new(db, project);
    for item in &item_tree(db, file).items {
        if item.kind != ItemKind::Entity {
            continue;
        }
        let Some(model) =
            crate::data::entity_model(db, project, Name::new(db, item.name.clone())).as_deref()
        else {
            continue;
        };
        let Some(visible) = model
            .lifecycle
            .as_ref()
            .and_then(|lifecycle| lifecycle.visible.as_ref())
        else {
            continue;
        };
        let mut fields = std::collections::BTreeSet::new();
        visible.fields(&mut fields);
        let entity = &model.name;
        let reached = prover.read(entity);
        let Some(field) = fields
            .into_iter()
            .find(|field| !prover.field_proved(entity, field, &reached))
        else {
            continue;
        };
        // The syntax is read only for a problem, so an edit elsewhere in
        // the file doesn't run this again.
        let Some(source) = tree::source(db, file) else {
            continue;
        };
        let at = match tree::decl_at(&source, item.index) {
            Some(Decl::EntityDecl(decl)) => section(decl.member_list(), "lifecycle")
                .and_then(|lifecycle| section(lifecycle.block(), "visible"))
                .map(|member| node_span(path, member.syntax())),
            _ => None,
        };
        let Some(at) = at else { continue };
        let who = prover.counterexample(&reached, |facts| {
            prover.field_readable(entity, &field, facts)
        });
        let readers = prover.readers(entity, &field);
        let among = who
            .map(|who| format!(", {who} among them,"))
            .unwrap_or_default();
        out.push(
            Diagnostic::new(
                &rule::POLICY_OUTPUT_FIELD_UNPROVED,
                format!("`{entity}`'s `visible:` tests `{entity}.{field}`, which not every caller may read"),
            )
            .at(at, format!("tests `{field}`"))
            .reason(format!(
                "Anyone `{entity}`'s `read:` lets in may read its records{among} and {readers} `{field}`. `visible:` decides which records every read sees, which shows each one's `{field}`, so it tests only fields every reader may read."
            ))
            .next(format!(
                "Test a field every reader may read, or give `{field}`'s `read:` everyone `{entity}`'s `read:` names"
            )),
        );
    }
    out
}

/// A computed backfill copies its row's fields into the field it fills,
/// and a copy doesn't escape a field's rules, so each field it reads is
/// one everyone who may read the filled field may read (area 9, "Field
/// restrictions", "Copies"; area 5, "Computed backfills"). Only the
/// latest migration's backfills are read with the project's code.
fn backfill_problems(db: &dyn Db, project: Project, file: File) -> Vec<Diagnostic> {
    let path = file.path(db);
    let mut out = Vec::new();
    if !path.ends_with("/changes.jadpo") {
        return out;
    }
    let latest = crate::data::migrations::migrations(db, project)
        .list
        .last()
        .map(|migration| migration.changes_path());
    if latest.as_deref() != Some(path.as_str()) {
        return out;
    }
    let Some(source) = tree::source(db, file) else {
        return out;
    };
    let prover = Prover::new(db, project);
    for decl in source.decls() {
        let Decl::SingletonDecl(migration) = decl else {
            continue;
        };
        let count = migration
            .member_list()
            .map_or(0, |list| list.member_items().count());
        for member in 0..count {
            let Some((entity, filled, value)) =
                crate::data::migrations::backfill_member(&migration, member)
            else {
                continue;
            };
            let mut reached = prover.read(&entity);
            if let Some(grants) = prover
                .policies
                .entity(&entity)
                .and_then(|policy| policy.field_grants(&filled, super::Effect::Read))
            {
                reached = reached.and(Reached::by(prover.way(&entity, grants)));
            }
            let prefix = format!("{entity}.");
            let copied = value
                .syntax()
                .descendants()
                .filter(|node| ast::FieldExpr::can_cast(node.kind()))
                .find_map(|node| {
                    let written = tree::written(&node);
                    let field = written.strip_prefix(&prefix)?.to_string();
                    (!field.contains('.')
                        && field != filled
                        && !prover.field_proved(&entity, &field, &reached))
                    .then(|| (field, node_span(path, &node)))
                });
            let Some((field, at)) = copied else { continue };
            let readers = prover.readers(&entity, &field);
            out.push(
                Diagnostic::new(
                    &rule::POLICY_OUTPUT_FIELD_UNPROVED,
                    format!(
                        "The backfill for `{entity}.{filled}` copies `{entity}.{field}`, which not everyone who may read `{entity}.{filled}` may read"
                    ),
                )
                .at(at, format!("reads `{field}`"))
                .reason(format!(
                    "Everyone who may read `{entity}.{filled}` would see what the backfill worked out from `{field}`, and {readers} `{field}`. A copy doesn't escape a field's rules, a migration's included."
                ))
                .next(format!(
                    "Give `{filled}` a `read:` no wider than `{field}`'s, or work the backfill out from fields every reader of `{filled}` may read"
                )),
            );
        }
    }
    out
}

/// One unproved part of a query: where it is, and what it carries.
enum Unproved {
    /// A field of a record of `entity`.
    Field {
        entity: String,
        field: String,
        reached: Reached,
    },
    /// A field of a record of `entity` that a condition tests, or with
    /// `order`, that the records are sorted by, in the include at
    /// `include` (none: the query's own).
    Condition {
        entity: String,
        field: String,
        order: bool,
        include: Vec<String>,
        reached: Reached,
    },
    /// The reference `via` of a record of `holder` an include follows,
    /// from it to its parent `to`, or with `child`, from `to` to the
    /// `holder` records that refer to it.
    Via {
        holder: String,
        via: String,
        to: String,
        child: bool,
        include: Vec<String>,
        reached: Reached,
    },
    /// The parent of `to` an include reads through `from`'s `via`.
    Parent {
        from: String,
        via: String,
        to: String,
        include: Vec<String>,
        reached: Reached,
    },
}

fn query(
    db: &dyn Db,
    project: Project,
    file: File,
    index: u32,
    model: &QueryModel,
) -> Vec<Diagnostic> {
    let prover = Prover::new(db, project);
    let lookup = Lookup::new(db, project);
    // A map layer is sent as it is, as an `Output` is (chunk 5.5u8).
    let output = model.geometry.is_some()
        || matches!(
            lookup.def(&model.element.ty),
            Some(Def::Fields {
                base: FieldsBase::Output,
                ..
            })
        );
    let mut found = Vec::new();
    // A listed query's callers passed its line as it started (area 9,
    // "Listing a query"), and its records are still its entity's `read:`.
    let line = prover.query_line(&model.name);
    let reached = Prover::lined(line.as_ref(), prover.read(&model.entity));
    // A `nearest` order and a `geo.distance` one stand in place of
    // `order`, and sort by their field as any order does (chunk 5.8a).
    let mut sorted = written(&model.order);
    sorted.extend(model.nearest.iter().map(|nearest| nearest.field.clone()));
    sorted.extend(model.distance.iter().map(|distance| distance.field.clone()));
    if let Some(geometry) = &model.geometry
        && !prover.field_proved(&model.entity, geometry, &reached)
    {
        found.push(Unproved::Field {
            entity: model.entity.clone(),
            field: geometry.clone(),
            reached: reached.clone(),
        });
    }
    conditions(
        &prover,
        &model.entity,
        &model.filter,
        sorted,
        &reached,
        &[],
        &mut found,
    );
    walk(
        &prover,
        line.as_ref(),
        &model.entity,
        &model.element,
        &model.includes,
        &reached,
        output,
        &mut Vec::new(),
        &mut found,
    );
    if found.is_empty() {
        return Vec::new();
    }
    let Some(source) = tree::source(db, file) else {
        return Vec::new();
    };
    let Some(Decl::QueryDecl(decl)) = tree::decl_at(&source, index) else {
        return Vec::new();
    };
    let path = file.path(db);
    // Who may call it, for a reason: its line under `actions:` when it is
    // listed, its records' `read:` otherwise.
    let listed = prover.policies.root().query(&model.name);
    let callers = |entity: &str| match listed {
        Some(line) => format!("`{}`'s line under `actions:`", line.written),
        None => format!("`{entity}`'s `read:`"),
    };
    let result = decl
        .ret_type()
        .and_then(|ret| ret.ty())
        .map(|ty| node_span(path, ty.syntax()))
        .unwrap_or_else(|| node_span(path, decl.syntax()));
    let mut out = Vec::new();
    // Each parent that can't be proved, then the fields, all in one.
    let mut fields: Vec<(String, String, Reached)> = Vec::new();
    for unproved in found {
        match unproved {
            Unproved::Parent {
                from,
                via,
                to,
                include,
                reached,
            } => {
                let at = include_span(path, &decl, &include).unwrap_or_else(|| result.clone());
                let who = prover.counterexample(&reached, |facts| {
                    prover.readable(&to, &prover.along(&from, &via, &to, facts))
                });
                let reason = match &who {
                    Some(who) => format!(
                        "`{from}`'s `read:` lets in {who}, who isn't shown to be someone `{to}`'s `read:` lets in, so the `{to}` a `{from}` refers to could be one its reader may not read. The proof follows the grants, not the data: two records that each refer to a third record may refer to different ones."
                    ),
                    None => format!(
                        "Not everyone `{from}`'s `read:` lets in is shown to be someone `{to}`'s `read:` lets in, so the `{to}` a `{from}` refers to could be one its reader may not read."
                    ),
                };
                out.push(
                    Diagnostic::new(
                        &rule::POLICY_OUTPUT_FIELD_UNPROVED,
                        format!(
                            "`{}` includes the `{to}` each `{from}` refers to, which not every caller may read",
                            model.name
                        ),
                    )
                    .at(at, format!("through `{via}`"))
                    .reason(reason)
                    .next(format!(
                        "Read the `{to}` with a query of its own, or give `{to}`'s `read:` the callers `{from}`'s `read:` names"
                    )),
                );
            }
            Unproved::Field {
                entity,
                field,
                reached,
            } => fields.push((entity, field, reached)),
            Unproved::Via {
                holder,
                via,
                to,
                child,
                include,
                reached,
            } => {
                let at = include_span(path, &decl, &include).unwrap_or_else(|| result.clone());
                let who = prover.counterexample(&reached, |facts| {
                    prover.field_readable(&holder, &via, facts)
                });
                let readers = prover.readers(&holder, &via);
                let among = who
                    .map(|who| format!(", {who} among them,"))
                    .unwrap_or_default();
                let headline = if child {
                    format!(
                        "`{}` includes the `{holder}`s that refer to each `{to}` through `{via}`, which not every caller may read",
                        model.name
                    )
                } else {
                    format!(
                        "`{}` includes the `{to}` each `{holder}` refers to through `{via}`, which not every caller may read",
                        model.name
                    )
                };
                out.push(
                    Diagnostic::new(&rule::POLICY_OUTPUT_FIELD_UNPROVED, headline)
                        .at(at, format!("through `{via}`"))
                        .reason(format!(
                            "Anyone `{holder}`'s `read:` lets in may see the records `{}` reads{among} and {readers} `{via}`. Which record an include reads shows the reference it follows, so a query includes only through references every caller may read.",
                            model.name
                        ))
                        .next(format!(
                            "Remove the include, or give `{holder}.{via}`'s `read:` everyone who may read a `{holder}`"
                        )),
                );
            }
            Unproved::Condition {
                entity,
                field,
                order,
                include,
                reached,
            } => {
                let at = condition_span(path, &decl, &include, order, &entity, &field)
                    .unwrap_or_else(|| result.clone());
                let who = prover.counterexample(&reached, |facts| {
                    prover.field_readable(&entity, &field, facts)
                });
                let readers = prover.readers(&entity, &field);
                let among = who
                    .map(|who| format!(", {who} among them,"))
                    .unwrap_or_default();
                let (does, label, shows, next) = if order {
                    (
                        "orders by",
                        format!("sorts by `{field}`"),
                        format!("The order records come in shows how their `{field}` compare"),
                        format!("Order by a field every caller may read, not `{entity}.{field}`"),
                    )
                } else {
                    (
                        "filters on",
                        format!("tests `{field}`"),
                        format!("Which records pass a condition shows each one's `{field}`"),
                        format!(
                            "Remove the condition on `{entity}.{field}`, or test a field every caller may read"
                        ),
                    )
                };
                out.push(
                    Diagnostic::new(
                        &rule::POLICY_OUTPUT_FIELD_UNPROVED,
                        format!(
                            "`{}` {does} `{entity}.{field}`, which not every caller may read",
                            model.name
                        ),
                    )
                    .at(at, label)
                    .reason(format!(
                        "Anyone {} lets in may see the records `{}` reads{among} and {readers} `{field}`. {shows}, so a query filters and orders only on fields every caller may read.",
                        callers(&entity),
                        model.name
                    ))
                    .next(next),
                );
            }
        }
    }
    if let Some((entity, field, reached)) = fields.first() {
        let named: Vec<String> = fields
            .iter()
            .map(|(e, f, _)| format!("`{e}.{f}`"))
            .collect();
        let who =
            prover.counterexample(reached, |facts| prover.field_readable(entity, field, facts));
        let readers = prover.readers(entity, field);
        let among = who
            .map(|who| format!(", {who} among them,"))
            .unwrap_or_default();
        let reason = format!(
            "Anyone {} lets in may call `{}`{among} and {readers} `{field}`. An output has one shape for everyone, so a field some callers may not read can't be in it.",
            callers(entity),
            model.name
        );
        out.push(
            Diagnostic::new(
                &rule::POLICY_OUTPUT_FIELD_UNPROVED,
                format!(
                    "`{}` sends {}, which not every caller may read",
                    model.name,
                    crate::and_list(&named)
                ),
            )
            .at(result, format!("carries `{field}`"))
            .reason(reason)
            .next(match &model.geometry {
                // A map layer (chunk 5.5u8): its geometry, or a property.
                Some(geometry) if geometry == field => format!(
                    "Give the features a geometry field every caller may read, in place of `{entity}.{field}`"
                ),
                Some(_) => format!(
                    "Leave `{field}` out of `{}`, the features' properties",
                    model.element.ty
                ),
                None => format!(
                    "Return an output without `{field}` from `{}`",
                    model.name
                ),
            }),
        );
    }
    out
}

/// Every field and parent of a query's records, at `include` (the names of
/// the includes leading here), that a caller who reached them as
/// `reached` isn't proved to read.
#[allow(clippy::too_many_arguments)]
fn walk(
    prover: &Prover<'_>,
    line: Option<&Way>,
    entity: &str,
    element: &Element,
    includes: &[Include],
    reached: &Reached,
    output: bool,
    include: &mut Vec<String>,
    out: &mut Vec<Unproved>,
) {
    for (_, source) in &element.fields {
        match source {
            // A group or an aggregate shows what the field it reads holds
            // (area 6, "Counts, sums and groups"), as the field would.
            Source::Entry { field: None, .. } => {}
            Source::Column(field)
            | Source::Entry {
                field: Some(field), ..
            } => {
                if output && !prover.field_proved(entity, field, reached) {
                    out.push(Unproved::Field {
                        entity: entity.to_string(),
                        field: field.clone(),
                        reached: reached.clone(),
                    });
                }
            }
            Source::Include(at) => {
                let Some(inner) = includes.get(*at) else {
                    continue;
                };
                include.push(inner.name.clone());
                let next = match inner.kind {
                    IncludeKind::Parent => {
                        if !prover.field_proved(entity, &inner.via, reached) {
                            out.push(Unproved::Via {
                                holder: entity.to_string(),
                                via: inner.via.clone(),
                                to: inner.entity.clone(),
                                child: false,
                                include: include.clone(),
                                reached: reached.clone(),
                            });
                        } else if !prover.parent_readable(
                            entity,
                            &inner.via,
                            &inner.entity,
                            reached,
                        ) {
                            out.push(Unproved::Parent {
                                from: entity.to_string(),
                                via: inner.via.clone(),
                                to: inner.entity.clone(),
                                include: include.clone(),
                                reached: reached.clone(),
                            });
                        }
                        prover.parent(entity, &inner.via, &inner.entity, reached)
                    }
                    IncludeKind::Child | IncludeKind::Children => {
                        let next = Prover::lined(line, prover.read(&inner.entity));
                        if !prover.field_proved(&inner.entity, &inner.via, &next) {
                            out.push(Unproved::Via {
                                holder: inner.entity.clone(),
                                via: inner.via.clone(),
                                to: entity.to_string(),
                                child: true,
                                include: include.clone(),
                                reached: next.clone(),
                            });
                        }
                        next
                    }
                };
                conditions(
                    prover,
                    &inner.entity,
                    &inner.filter,
                    written(&inner.order),
                    &next,
                    include,
                    out,
                );
                walk(
                    prover,
                    line,
                    &inner.entity,
                    &inner.element,
                    &inner.includes,
                    &next,
                    output,
                    include,
                    out,
                );
                include.pop();
            }
        }
    }
}

/// The fields an `order:` sorts by as written: the tie-break the compiler
/// adds isn't, so isn't checked.
fn written(order: &[OrderField]) -> Vec<String> {
    order
        .iter()
        .filter(|field| !field.added)
        .map(|field| field.field.clone())
        .collect()
}

/// Every field `filter` tests and `sorted` sorts by, on records of
/// `entity` a caller reached as `reached`, that they aren't proved to
/// read: one problem per field and section, in the order written.
fn conditions(
    prover: &Prover<'_>,
    entity: &str,
    filter: &[Condition],
    sorted: Vec<String>,
    reached: &Reached,
    include: &[String],
    out: &mut Vec<Unproved>,
) {
    let mut tested: Vec<String> = Vec::new();
    for condition in filter {
        let mut fields = std::collections::BTreeSet::new();
        condition.fields(&mut fields);
        for field in fields {
            if !tested.contains(&field) {
                tested.push(field);
            }
        }
    }
    let fields = tested
        .into_iter()
        .map(|field| (field, false))
        .chain(sorted.into_iter().map(|field| (field, true)));
    let mut seen = Vec::new();
    for (field, order) in fields {
        if seen.contains(&(field.clone(), order)) {
            continue;
        }
        seen.push((field.clone(), order));
        if !prover.field_proved(entity, &field, reached) {
            out.push(Unproved::Condition {
                entity: entity.to_string(),
                field,
                order,
                include: include.to_vec(),
                reached: reached.clone(),
            });
        }
    }
}

/// Where `Entity.field` is written in the condition (or with `order`, the
/// order) of the include at `names`, or of the query itself with none.
fn condition_span(
    path: &str,
    decl: &ast::QueryDecl,
    names: &[String],
    order: bool,
    entity: &str,
    field: &str,
) -> Option<Span> {
    let mut list = decl.member_list();
    for name in names {
        let block = section(list, "include")?.block()?;
        let entry = block.member_items().find_map(|item| match item {
            MemberItem::Member(member)
                if member
                    .member_key()
                    .and_then(|key| key.names().next())
                    .and_then(|n| tree::name_text(&n))
                    .as_deref()
                    == Some(name.as_str()) =>
            {
                Some(member)
            }
            _ => None,
        })?;
        list = entry.block();
    }
    let words: &[&str] = if order {
        &["order"]
    } else {
        &["where", "where_present"]
    };
    let written = format!("{entity}.{field}");
    tree::members(list)
        .into_iter()
        .filter(|(name, _)| {
            tree::name_text(name).is_some_and(|word| words.contains(&word.as_str()))
        })
        .find_map(|(_, member)| {
            member
                .syntax()
                .descendants()
                .find(|node| {
                    ast::FieldExpr::can_cast(node.kind()) && tree::written(node) == written
                })
                .map(|node| node_span(path, &node))
        })
}

/// Where the include at `names` (each level's field) is written.
fn include_span(path: &str, decl: &ast::QueryDecl, names: &[String]) -> Option<Span> {
    let mut list = decl.member_list();
    let mut at = None;
    for name in names {
        let block = section(list, "include")?.block()?;
        let entry = block.member_items().find_map(|item| match item {
            MemberItem::Member(member)
                if member
                    .member_key()
                    .and_then(|key| key.names().next())
                    .and_then(|n| tree::name_text(&n))
                    .as_deref()
                    == Some(name.as_str()) =>
            {
                Some(member)
            }
            _ => None,
        })?;
        at = entry.member_key().map(|key| node_span(path, key.syntax()));
        list = entry.block();
    }
    at
}

fn section(list: Option<ast::MemberList>, word: &str) -> Option<ast::Member> {
    tree::members(list)
        .into_iter()
        .find(|(name, _)| tree::name_text(name).as_deref() == Some(word))
        .map(|(_, member)| member)
}
