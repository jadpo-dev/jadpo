//! An entity's operations in a body (area 6, "The operations every entity
//! gets"): `Entity.get(id)` reads one record or fails with the entity's
//! not-found failure, and `Entity.create { set: { ... } }` adds one. The
//! rest (`update`, `delete` and the lifecycle's transitions) come with
//! chunks 3.3 and 3.4. A query or `get` inside a `for` loop is refused
//! (area 6, "Reading in a loop").

use std::collections::BTreeSet;

use jadpo_diag::{Diagnostic, Span, rule};
use jadpo_syntax::ast::{self, AstNode, MemberItem};

use super::super::table::FieldDef;
use super::super::ty::Ty;
use super::{Body, node_key};
use crate::tree;

impl Body<'_> {
    /// `Entity.get(id)`: its type, and whether it can be called.
    pub(super) fn entity_get(&mut self, entity: &str, call: &ast::CallExpr, span: Span) -> Ty {
        let written = format!("{entity}.get");
        self.read_in_loop(span.clone(), &written);
        let id = Ty::Field(entity.to_string(), vec!["id".to_string()]);
        let args = Self::arguments(call);
        match args.as_slice() {
            [arg] => {
                if let Some(value) = arg.expr() {
                    self.check(&value, &id);
                }
            }
            _ => {
                self.args_only(call);
                self.out.push(
                    Diagnostic::new(
                        &rule::TYPE_ARGUMENT_COUNT,
                        format!("`{written}` takes one argument, and {} given", args.len()),
                    )
                    .at(self.span(call), "one `id`")
                    .reason(format!(
                        "`{written}` reads the one record with the `{entity}.id` it is given."
                    ))
                    .next(format!(
                        "Call `{written}({}_id)`",
                        crate::case::to_snake(entity)
                    )),
                );
                return Ty::Error;
            }
        }
        // The entity's not-found failure: the domain's, until the compiler
        // provides one.
        let missing = format!("{entity}NotFound");
        if !self.failures.is_declared_failure(&missing) {
            self.out.push(
                Diagnostic::new(
                    &rule::TOOLCHAIN_NOT_BUILT_YET,
                    format!("`{written}` needs `{missing}` declared for now"),
                )
                .at(span, "no not-found failure")
                .reason(format!(
                    "`{written}` fails with the domain's `{missing}`, or one the compiler provides when the domain declares none (area 6); providing one isn't built yet."
                ))
                .next(format!(
                    "Declare `type {missing} = Failure {{ kind: FailureKind.not_found }}` in the domain's `failures/` folder"
                )),
            );
            return Ty::Named(entity.to_string());
        }
        // `get` fills only fields that hold the identity it looked for.
        let unfilled: Vec<String> = self
            .table
            .fields(&missing)
            .unwrap_or(&[])
            .iter()
            .filter(|field| !self.table.assignable(&id, field.declared.strip_nullable()))
            .map(|field| format!("`{}`", field.name))
            .collect();
        if !unfilled.is_empty() {
            self.out.push(
                Diagnostic::new(
                    &rule::TOOLCHAIN_NOT_BUILT_YET,
                    format!("`{written}` can't fill {} of `{missing}`", crate::and_list(&unfilled)),
                )
                .at(span, "fields it can't fill")
                .reason(format!(
                    "`{written}` raises `{missing}` with the `{entity}.id` it looked for in each field of that type; filling other fields isn't built."
                ))
                .next(format!(
                    "Give `{missing}` only fields typed `{entity}.id`, or read with a query whose `missing:` fills them"
                )),
            );
        }
        Ty::Named(entity.to_string())
    }

    /// `Entity.create { set: { ... } }`: every field the store doesn't
    /// generate is given once, each value fits its field.
    pub(super) fn create(
        &mut self,
        record: &ast::RecordExpr,
        entity: &str,
        fields: &[FieldDef],
        head_span: Span,
    ) {
        let written = format!("{entity}.create");
        if self.context.is_pure() {
            self.persistence(head_span.clone(), &written);
        }
        let mut set: Option<ast::Member> = None;
        for item in record
            .member_list()
            .iter()
            .flat_map(|list| list.member_items())
        {
            let MemberItem::Member(member) = item else {
                continue;
            };
            let Some(key) = member.member_key() else {
                continue;
            };
            let word = tree::written(key.syntax());
            if word == "set" && set.is_none() {
                set = Some(member.clone());
                continue;
            }
            self.walk(|walk| walk.member_values(member.block()));
            let span = self.span(&key);
            self.out.push(
                Diagnostic::new(
                    &rule::TOOLCHAIN_NOT_BUILT_YET,
                    format!("A create's `{word}:` isn't built yet"),
                )
                .at(span, "decided, not built")
                .reason(format!(
                    "`{written}` gives its fields in `set:`; mapping the rules a write can break (`conflict:`) comes with chunk 3.3."
                ))
                .next(format!("Write only `set:` in `{written}` for now")),
            );
        }
        let settable: Vec<FieldDef> = fields
            .iter()
            .filter(|field| field.name != "id" && !field.generated)
            .cloned()
            .collect();
        let Some(set) = set else {
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_MISSING_FIELD,
                    format!("`{written}` gives no fields"),
                )
                .at(head_span, "no `set:`")
                .reason(format!(
                    "A new `{entity}` gets every field the store doesn't generate, given in `set:`."
                ))
                .next("Add `set: { ... }` with the record's fields"),
            );
            return;
        };
        // The store's own fields are never given.
        let generated: BTreeSet<&str> = fields
            .iter()
            .filter(|field| field.name == "id" || field.generated)
            .map(|field| field.name.as_str())
            .collect();
        for (name, _) in tree::members(set.block()) {
            let Some(field) = tree::name_text(&name) else {
                continue;
            };
            if generated.contains(field.as_str()) {
                let span = self.span(&name);
                let why = if field == "id" {
                    format!(
                        "`{entity}.id` is generated when the record is created and never supplied by callers."
                    )
                } else {
                    format!(
                        "`{entity}.{field}` is generated from the operation's clock; code never assigns it."
                    )
                };
                self.out.push(
                    Diagnostic::new(
                        &rule::DATA_SET_GENERATED_FIELD,
                        format!("`{written}` sets `{field}`, which the store generates"),
                    )
                    .at(span, "generated")
                    .reason(why)
                    .next(format!("Remove `{field}` from `set:`")),
                );
            }
        }
        let skip: Vec<&str> = generated.iter().copied().collect();
        self.fields_given(set.block(), entity, &[], &settable, false, head_span, &skip);
        self.facts.builds.insert(
            node_key(record.syntax()),
            super::facts::BuildFact::Create(entity.to_string()),
        );
        self.facts
            .constructions
            .insert(node_key(record.syntax()), super::Fallible::Never);
    }

    /// A read inside a `for` loop reads once per item (area 6, question
    /// 11).
    pub(super) fn read_in_loop(&mut self, span: Span, written: &str) {
        if self.loops == 0 {
            return;
        }
        self.out.push(
            Diagnostic::new(
                &rule::DATA_READ_IN_LOOP,
                format!("`{written}` reads once for every item of the loop"),
            )
            .at(span, "inside `for`")
            .reason("A read inside a loop makes one database query per item. Read the whole set with one query, testing the field with `in` a list of the values, before the loop.")
            .next("Read the records with one query using `in` before the loop"),
        );
    }
}
