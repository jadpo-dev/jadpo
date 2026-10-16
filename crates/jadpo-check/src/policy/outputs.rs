//! Policy on what a query returns (area 9, "Field restrictions"): an output
//! type has one shape for everyone, so every caller who may read a query's
//! records may read each field its output carries, and the parent record
//! of every include, or the query doesn't compile
//! (`policy.output_field_unproved`). Which records pass a condition, and
//! the order they come in, show the fields they test too, so every field a
//! `where:`, `where_present:` or `order:` names, the query's own or an
//! include's, is one every caller who reaches those records may read
//! (area 9, "Restricted fields in conditions and orders").
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

use super::proof::{Prover, Reached};
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
    let output = matches!(
        lookup.def(&model.element.ty),
        Some(Def::Fields {
            base: FieldsBase::Output,
            ..
        })
    );
    let mut found = Vec::new();
    let reached = prover.read(&model.entity);
    conditions(
        &prover,
        &model.entity,
        &model.filter,
        &model.order,
        &reached,
        &[],
        &mut found,
    );
    walk(
        &prover,
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
            Unproved::Condition {
                entity,
                field,
                order,
                include,
                reached,
            } => {
                let at = condition_span(path, &decl, &include, order, &entity, &field)
                    .unwrap_or_else(|| result.clone());
                let who = prover
                    .counterexample(&reached, |facts| prover.field_readable(&entity, &field, facts));
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
                        format!("Remove the condition on `{entity}.{field}`, or test a field every caller may read"),
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
                        "Anyone `{entity}`'s `read:` lets in may see the records `{}` reads{among} and {readers} `{field}`. {shows}, so a query filters and orders only on fields every caller may read.",
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
            "Anyone `{entity}`'s `read:` lets in may call `{}`{among} and {readers} `{field}`. An output has one shape for everyone, so a field some callers may not read can't be in it.",
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
            .next(format!(
                "Return an output without `{field}` from `{}`",
                model.name
            )),
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
            Source::Column(field) => {
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
                        if !prover.parent_readable(entity, &inner.via, &inner.entity, reached) {
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
                    IncludeKind::Child | IncludeKind::Children => prover.read(&inner.entity),
                };
                conditions(
                    prover,
                    &inner.entity,
                    &inner.filter,
                    &inner.order,
                    &next,
                    include,
                    out,
                );
                walk(
                    prover,
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

/// Every field `filter` tests and `order` sorts by, on records of `entity`
/// a caller reached as `reached`, that they aren't proved to read: one
/// problem per field and section, in the order written.
fn conditions(
    prover: &Prover<'_>,
    entity: &str,
    filter: &[Condition],
    order: &[OrderField],
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
    let sorted = order
        .iter()
        .filter(|field| !field.added)
        .map(|field| field.field.clone());
    let fields = tested
        .into_iter()
        .map(|field| (field, false))
        .chain(sorted.map(|field| (field, true)));
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
