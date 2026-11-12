//! Data builders and `assert emitted` in tests (area 13, "Data builders"
//! and "Events and query cost").
//!
//! `make.Todo { title: "Buy milk" }` creates one record through the
//! entity's real create, so the store's rules and policy apply as for any
//! create. It may set any field the store doesn't generate, a field only
//! the lifecycle changes included; every other field is filled with a
//! valid value: `none` where the field may be absent, a record made by the
//! referenced entity's own builder for a reference that can't, and
//! otherwise a value of the field's type keeping its rules, made as a
//! fake's generated default is, different each time for a field a unique
//! rule holds. A field no value can be made for (a file, a key, text that
//! must match a pattern) is given. Only tests, fixtures and seeds make
//! records, and a builder has no `fails` list: a failure inside one fails
//! the test, as a fixture's does.
//!
//! `assert emitted OrderPlaced { order_id: order.id }` holds when one of
//! the events of that type the test's committed steps kept has those
//! values; the fields it leaves out aren't compared.

use jadpo_diag::{Diagnostic, Span, rule};
use jadpo_syntax::ast::{self, AstNode, Expr};

use super::super::shape::FieldsBase;
use super::super::table::{Def, FieldDef};
use super::super::ty::Ty;
use super::{Body, Context, node_key, text};
use crate::resolve::Want;
use crate::tree;

/// The generated namespace a builder is written through.
pub(crate) const MAKE: &str = "make";

/// What checking one `make.Entity { ... }` decided, for its typed form.
#[derive(Clone, Debug)]
pub(crate) struct MakeFact {
    pub entity: String,
    /// Each field it leaves out, in the entity's order, with what fills
    /// it.
    pub filled: Vec<(String, Filled)>,
    /// Each reference given whose value may name a record that isn't
    /// there, with that entity's not-found failure.
    pub missing: Vec<(String, String)>,
    /// Whether a field given is a finished upload (area 16, "Uploading").
    pub attaches: bool,
}

/// What fills a field a builder leaves out.
#[derive(Clone, Debug)]
pub(crate) enum Filled {
    /// `none`, for a field that may be absent.
    Absent(Ty),
    /// A valid value of the field's type, `distinct` when a unique rule
    /// holds the field.
    Made { ty: Ty, distinct: bool },
    /// The `id` of a record of the referenced entity, made by its own
    /// builder with every field filled.
    Reference(Box<MakeFact>),
}

impl Body<'_> {
    /// `make.<entity> { ... }`: the record it makes.
    pub(super) fn make(&mut self, record: &ast::RecordExpr, entity: &ast::NameRef) -> Ty {
        let name = text(entity);
        let written = format!("{MAKE}.{name}");
        let head_span = record
            .expr()
            .map_or_else(|| self.span(record), |head| self.span(&head));
        if self.context != Context::Test {
            self.out.push(
                Diagnostic::new(
                    &rule::SEMANTIC_NOT_CALLABLE,
                    format!("`{written}` is used outside a test, fixture or seed"),
                )
                .at(head_span, "not in a test")
                .reason(format!(
                    "`{written}` is a data builder: it makes a record with valid values for a test to start from, so only tests, fixtures and seeds use it; application code creates records with `{name}.create` in an action."
                ))
                .next(format!(
                    "Create the record with `{name}.create {{ set: {{ ... }} }}` in an action, or move this into a test, fixture or seed"
                )),
            );
            self.walk(|walk| walk.member_values(record.member_list()));
            return Ty::Error;
        }
        if !self.resolved_by(|names, out| names.type_name(entity, Want::Type, out)) {
            self.walk(|walk| walk.member_values(record.member_list()));
            return Ty::Error;
        }
        if !matches!(
            self.table.def(&name),
            Some(Def::Fields {
                base: FieldsBase::Entity,
                ..
            })
        ) {
            self.out.push(
                Diagnostic::new(
                    &rule::DATA_WRITE_NOT_ENTITY,
                    format!("`{name}` isn't an entity, so `{MAKE}` can't make one"),
                )
                .at(self.span(entity), "not an entity")
                .reason(format!(
                    "A data builder makes one stored record through its entity's create, so `{MAKE}` names an entity; `{name}` is a value, built with `{name} {{ ... }}`."
                ))
                .next(format!(
                    "Make an entity's record with `{MAKE}.<Entity> {{ ... }}`, or build the value with `{name} {{ ... }}`"
                )),
            );
            self.walk(|walk| walk.member_values(record.member_list()));
            return Ty::Error;
        }
        let rules = self.writes.write_rules(&name).cloned().unwrap_or_default();
        let fields: Vec<FieldDef> = self
            .table
            .fields(&name)
            .map(<[_]>::to_vec)
            .unwrap_or_default();
        // The fields it may set: any the store doesn't generate.
        let settable: Vec<FieldDef> = fields
            .iter()
            .filter(|field| field.name != "id" && !field.generated)
            .cloned()
            .map(super::writes::attachable)
            .collect();
        let given = tree::members(record.member_list());
        let mut skip: Vec<String> = Vec::new();
        for (field_name, member) in &given {
            let Some(field) = tree::name_text(field_name) else {
                continue;
            };
            if fields
                .iter()
                .any(|f| f.name == field && (f.name == "id" || f.generated))
            {
                let span = self.span(field_name);
                self.set_generated(&name, &field, span, &written, &format!("`{written}`"));
                skip.push(field);
                continue;
            }
            // A key held as data is kept sealed (area 19; chunk P9).
            if let (Some(target), Some(value)) =
                (fields.iter().find(|f| f.name == field), member.value())
                && self
                    .table
                    .base_prim(&target.declared)
                    .is_some_and(|prim| prim.is_key())
            {
                self.facts.key_sinks.insert(node_key(value.syntax()));
            }
        }
        let skipped: Vec<&str> = skip.iter().map(String::as_str).collect();
        self.fields_given(
            record.member_list(),
            &name,
            &[],
            &settable,
            super::expr::Required::None,
            head_span.clone(),
            &skipped,
        );
        let mut missing = Vec::new();
        let mut attaches = false;
        let mut named: Vec<String> = Vec::new();
        for (field_name, member) in &given {
            let Some(field) = tree::name_text(field_name) else {
                continue;
            };
            named.push(field.clone());
            let Some(value) = member.value() else {
                continue;
            };
            attaches |= self.attaches(&value);
            if let Some(target) = rules.reference(&field)
                && !self.holds(&value, target)
                && !missing.iter().any(|(given, _)| *given == field)
            {
                let failure = self.not_found_of(target, head_span.clone(), &written);
                missing.push((field, failure));
            }
        }
        let mut making = vec![name.clone()];
        let Some(filled) = self.filled(&name, &named, &mut making, &written, &head_span) else {
            return Ty::Named(name);
        };
        self.facts.makes.insert(
            node_key(record.syntax()),
            MakeFact {
                entity: name.clone(),
                filled,
                missing,
                attaches,
            },
        );
        Ty::Named(name)
    }

    /// What fills each field of `entity` a builder doesn't name in `given`,
    /// `making` being the entities whose records are being made around
    /// it; `None` when one can't be filled, which is reported.
    fn filled(
        &mut self,
        entity: &str,
        given: &[String],
        making: &mut Vec<String>,
        written: &str,
        span: &Span,
    ) -> Option<Vec<(String, Filled)>> {
        let rules = self.writes.write_rules(entity).cloned().unwrap_or_default();
        let fields: Vec<FieldDef> = self
            .table
            .fields(entity)
            .map(<[_]>::to_vec)
            .unwrap_or_default();
        let mut out = Vec::new();
        let mut unfillable: Vec<(String, &'static str)> = Vec::new();
        for field in &fields {
            // A field the lifecycle owns starts at its `initial:` value.
            if field.name == "id"
                || field.generated
                || given.contains(&field.name)
                || rules.owned.contains(&field.name)
            {
                continue;
            }
            let ty = self
                .table
                .field_type(entity, std::slice::from_ref(&field.name));
            if self.table.is_nullable(&field.declared) {
                out.push((field.name.clone(), Filled::Absent(ty)));
                continue;
            }
            if let Some(target) = rules.reference(&field.name) {
                let target = target.to_string();
                if making.contains(&target) {
                    unfillable.push((
                        field.name.clone(),
                        "a reference back to a record being made",
                    ));
                    continue;
                }
                making.push(target.clone());
                let nested = self.filled(&target, &[], making, written, span);
                making.pop();
                let nested = nested?;
                out.push((
                    field.name.clone(),
                    Filled::Reference(Box::new(MakeFact {
                        entity: target,
                        filled: nested,
                        missing: Vec::new(),
                        attaches: false,
                    })),
                ));
                continue;
            }
            if let Some(why) = self.unmakeable(&field.declared, &ty) {
                unfillable.push((field.name.clone(), why));
                continue;
            }
            let distinct = rules
                .unique
                .iter()
                .any(|unique| unique.fields.contains(&field.name));
            out.push((field.name.clone(), Filled::Made { ty, distinct }));
        }
        if unfillable.is_empty() {
            return Some(out);
        }
        let listed: Vec<String> = unfillable
            .iter()
            .map(|(name, _)| format!("`{name}`"))
            .collect();
        let reasons: Vec<String> = unfillable
            .iter()
            .map(|(name, why)| format!("`{entity}.{name}` is {why}"))
            .collect();
        let outer = making.first().map_or(entity, String::as_str);
        let (message, next) = if outer == entity {
            (
                format!("`{written}` can't fill {}", crate::and_list(&listed)),
                format!("Give {} in `{written}`'s block", crate::and_list(&listed)),
            )
        } else {
            let via = making.get(1).map_or(entity, String::as_str);
            (
                format!(
                    "`{written}` can't make the `{entity}` it needs: it can't fill {}",
                    crate::and_list(&listed)
                ),
                format!(
                    "Make the `{via}` with `{MAKE}.{via} {{ ... }}`, giving what it can't fill, and give its `id` in `{written}`'s block"
                ),
            )
        };
        self.out.push(
            Diagnostic::new(&rule::TYPE_MISSING_FIELD, message)
                .at(span.clone(), "fields it can't fill")
                .reason(format!(
                    "A data builder fills each field it isn't given with a valid value, but no value can be made for every field: {}.",
                    reasons.join("; ")
                ))
                .next(next),
        );
        None
    }

    /// Why no value of a field declared `declared`, typed `ty`, can be
    /// made: a file, a key, or text that must match a pattern.
    fn unmakeable(&self, declared: &Ty, ty: &Ty) -> Option<&'static str> {
        let opaque = |wanted: &str| matches!(declared.strip_nullable(), Ty::Opaque(name, _) if name == wanted);
        if opaque(jadpo_std::files::FILE) || opaque(jadpo_std::files::FILE_UPLOAD) {
            return Some("a file, which only a store makes");
        }
        if self
            .table
            .base_prim(declared)
            .is_some_and(|prim| prim.is_key())
        {
            return Some("a key, which only `crypto.new_key` makes");
        }
        if self
            .table
            .rule_chain(ty)
            .iter()
            .any(|(_, rules)| rules.pattern.is_some())
        {
            return Some("text that must match a pattern");
        }
        None
    }

    /// `assert emitted Event { ... }` (area 13): the fields given checked
    /// against the event's, any of them left out.
    pub(super) fn assert_emitted(&mut self, value: &Expr) {
        let Expr::RecordExpr(record) = value else {
            self.infer(value);
            self.emitted_form(self.span(value));
            return;
        };
        let head = match record.expr() {
            Some(Expr::NameRef(head)) => head,
            _ => {
                self.walk(|walk| walk.member_values(record.member_list()));
                self.emitted_form(self.span(record));
                return;
            }
        };
        if !self.resolved_by(|names, out| names.type_name(&head, Want::Type, out)) {
            self.walk(|walk| walk.member_values(record.member_list()));
            return;
        }
        let name = text(&head);
        let fields = match self.table.def(&name) {
            Some(Def::Fields {
                base: FieldsBase::Event,
                fields,
                ..
            }) => fields.clone(),
            _ => {
                self.walk(|walk| walk.member_values(record.member_list()));
                self.out.push(
                    Diagnostic::new(
                        &rule::EVENT_EMIT_INVALID,
                        format!("`assert emitted` is given `{name}`, which isn't an event"),
                    )
                    .at(self.span(&head), "not an event")
                    .reason("`assert emitted` checks that an event was emitted: it names a type with the `Event` base, declared in a domain's `events/` folder, with the fields to compare.")
                    .next("Name an event, `assert emitted <Event> { field: value }`"),
                );
                return;
            }
        };
        let head_span = self.span(&head);
        self.fields_given(
            record.member_list(),
            &name,
            &[],
            &fields,
            super::expr::Required::None,
            head_span,
            &[],
        );
        self.facts
            .types
            .insert(node_key(record.syntax()), Ty::Named(name.clone()));
        self.facts.emitted.insert(node_key(record.syntax()), name);
    }

    /// `assert emitted` written without an event's name and block.
    fn emitted_form(&mut self, span: Span) {
        self.out.push(
            Diagnostic::new(
                &rule::EVENT_EMIT_INVALID,
                "`assert emitted` is given something other than an event",
            )
            .at(span, "not an event")
            .reason("`assert emitted` names the event's type and the fields to compare, as an event is written: `assert emitted <Event> { field: value }`.")
            .next("Write `assert emitted <Event> { field: value }`"),
        );
    }
}
