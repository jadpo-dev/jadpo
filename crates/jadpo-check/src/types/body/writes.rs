//! An entity's operations in a body (area 6, "The operations every entity
//! gets"): `Entity.get(id)` reads one record or fails with the entity's
//! not-found failure; `create`, `update` and `delete` write, each with a
//! block saying what it changes and how its refusals map. A query or `get`
//! inside a `for` loop is refused (area 6, "Reading in a loop"); the
//! lifecycle's transitions come with chunk 3.4.
//!
//! What a write can fail with is known here (area 6, "When a write is
//! refused"): an `update` or `delete` by `id` finds no record; a reference
//! names a record that isn't there, unless its value is read from a
//! record loaded earlier, which holds; and each unique rule and
//! restricting reference it can break, which its `conflict:` maps, keyed
//! by the rule, with no catch-all.

use std::collections::{BTreeMap, BTreeSet};

use jadpo_db::ItemKind;
use jadpo_diag::{Diagnostic, Span, nearest, rule};
use jadpo_std::write::WriteOp;
use jadpo_syntax::ast::{self, AstNode, Expr, MemberItem};
use jadpo_syntax::{SyntaxKind, SyntaxNode};

use super::super::shape::FieldsBase;
use super::super::table::{Def, FieldDef};
use super::super::ty::{Prim, Ty};
use super::facts::Reach;
use super::failing::Arm;
use super::{Body, Fallible, LocalKind, did_you_mean, node_key, text};
use crate::data::WriteRules;
use crate::tree::{self, dotted};

/// What the walk decided about one write, for its typed form.
#[derive(Clone, Debug)]
pub(crate) struct WriteFact {
    pub entity: String,
    pub op: WriteOp,
    /// An `update` or `delete` naming its record by `id`.
    pub by_id: bool,
    /// The block after the write.
    pub block: Option<ast::MemberList>,
    /// The `conflict:` arms that map a rule the write can break: each
    /// arm's member, the rule and the failure.
    pub arms: Vec<(ast::Member, Breach, String)>,
    /// Each reference whose value may name a record that isn't there, with
    /// that entity's not-found failure.
    pub missing: Vec<(String, String)>,
    /// By `id`: the entity's not-found failure.
    pub not_found: Option<String>,
    pub fallible: Fallible,
}

/// A rule a write can break (area 6, "When a write is refused").
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Breach {
    /// A unique rule: a field's own (`User.email`, `field`) or a named one
    /// (`Project.name_per_owner`).
    Unique {
        entity: String,
        name: String,
        field: bool,
    },
    /// A reference that refuses a delete: `Todo.project_id`.
    Reference { entity: String, field: String },
}

impl Breach {
    /// As a `conflict:` arm writes it: `Project.name_per_owner`.
    pub fn written(&self) -> String {
        match self {
            Breach::Unique { entity, name, .. } => format!("{entity}.{name}"),
            Breach::Reference { entity, field } => format!("{entity}.{field}"),
        }
    }
}

/// The sections each write takes, in the order a reader meets them.
fn sections(op: WriteOp) -> &'static [&'static str] {
    match op {
        WriteOp::Create => &["set", "conflict"],
        WriteOp::Update => &["set", "patch", "when_supplied", "conflict"],
        WriteOp::Delete => &["conflict"],
    }
}

/// "`set:` and `conflict:`".
fn sections_list(op: WriteOp) -> String {
    let listed: Vec<String> = sections(op).iter().map(|s| format!("`{s}:`")).collect();
    crate::and_list(&listed)
}

/// The fields a write sets, with where.
struct Sets {
    /// Each field set in `set:`, `patch:` or `when_supplied:`.
    fields: BTreeSet<String>,
}

impl Body<'_> {
    // --- get ----------------------------------------------------------------

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
                self.argument_count(call, &written, entity, args.len());
                return Ty::Error;
            }
        }
        self.not_found_of(entity, span, &written);
        Ty::Named(entity.to_string())
    }

    fn argument_count(&mut self, call: &ast::CallExpr, written: &str, entity: &str, given: usize) {
        let snake = crate::case::to_snake(entity);
        self.out.push(
            Diagnostic::new(
                &rule::TYPE_ARGUMENT_COUNT,
                format!("`{written}` takes one argument, and {given} given"),
            )
            .at(self.span(call), "one argument")
            .reason(if written.ends_with(".get") {
                format!("`{written}` reads the one record with the `{entity}.id` it is given.")
            } else {
                format!(
                    "`{written}` takes the `{entity}` loaded earlier in the action, or its `{entity}.id`."
                )
            })
            .next(if written.ends_with(".get") {
                format!("Call `{written}({snake}_id)`")
            } else {
                format!("Call `{written}({snake})`")
            }),
        );
    }

    /// `<Entity>NotFound`: the domain's failure of that name, or the one
    /// the compiler provides when it declares none (area 6, question 6). A
    /// read or write fills each of its fields typed `<Entity>.id` with the
    /// `id` it looked for; a declared one with other fields can't be
    /// raised yet.
    fn not_found_of(&mut self, entity: &str, span: Span, written: &str) -> String {
        let missing = format!("{entity}NotFound");
        let id = Ty::Field(entity.to_string(), vec!["id".to_string()]);
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
        missing
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

    // --- finding writes ---------------------------------------------------

    /// `X.update(...)` or `X.delete(...)` where `X` names a type: the type
    /// and the operation, whether or not `X` is an entity.
    pub(super) fn write_call(&self, call: &ast::CallExpr) -> Option<(String, WriteOp)> {
        let callee = call.expr()?;
        let (first, rest) = dotted(&callee)?;
        let [operation] = rest.as_slice() else {
            return None;
        };
        let op = match text(operation).as_str() {
            "update" => WriteOp::Update,
            "delete" => WriteOp::Delete,
            _ => return None,
        };
        let head = text(&first);
        if !crate::case::starts_upper(&head) || self.is_local(&head) {
            return None;
        }
        // `Todo.update(value)` builds a field called `update`.
        if self
            .table
            .field(&head, &rest.iter().map(text).collect::<Vec<_>>())
            .is_some()
        {
            return None;
        }
        let entity = self.callable_kind(&head) == Some(ItemKind::Entity);
        (entity || self.table.def(&head).is_some()).then_some((head, op))
    }

    /// `Draft.create`, `Draft.update` or `Draft.delete` where `Draft` is a
    /// type but not an entity.
    pub(super) fn write_not_entity(&mut self, name: &str, op: WriteOp, span: Span) {
        let kind = match self.table.def(name) {
            Some(Def::Fields { base, .. }) => match base {
                FieldsBase::Input => "input",
                FieldsBase::Output => "output",
                FieldsBase::Event => "event",
                FieldsBase::EmailMessage => "email",
                FieldsBase::Failure => "failure",
                FieldsBase::Object | FieldsBase::Entity => "object",
            },
            Some(Def::Enum { .. }) => "enum",
            _ => "type",
        };
        let an = if kind.starts_with(['a', 'e', 'i', 'o', 'u']) {
            "an"
        } else {
            "a"
        };
        self.out.push(
            Diagnostic::new(
                &rule::DATA_WRITE_NOT_ENTITY,
                format!("`{name}` isn't an entity, so it has no `{}`", op.word()),
            )
            .at(span, format!("{an} {kind}"))
            .reason(format!(
                "Only an entity is stored, so only an entity has `create`, `update` and `delete`; `{name}` is {an} {kind}, a value built with `{name} {{ ... }}`."
            ))
            .next(format!(
                "Write the entity this value is stored as, or build the value with `{name} {{ ... }}`"
            )),
        );
    }

    // --- a write ----------------------------------------------------------

    /// `Entity.create { ... }`, `Entity.update(target) { ... }` or
    /// `Entity.delete(target) { ... }`: `node` is the record for a create
    /// and the call otherwise, `block` the block after it. Its type.
    pub(super) fn write(
        &mut self,
        entity: &str,
        op: WriteOp,
        node: &SyntaxNode,
        call: Option<&ast::CallExpr>,
        block: Option<ast::MemberList>,
        head_span: Span,
    ) -> Ty {
        let written = format!("{entity}.{}", op.word());
        let result = match op {
            WriteOp::Delete => Ty::Prim(Prim::Unit),
            _ => Ty::Named(entity.to_string()),
        };
        let fields: Vec<FieldDef> = self
            .table
            .fields(entity)
            .map(<[_]>::to_vec)
            .unwrap_or_default();
        let rules = self.writes.write_rules(entity).cloned().unwrap_or_default();
        // The record it writes, loaded earlier or named by `id`.
        let mut by_id = false;
        if let Some(call) = call {
            let args = Self::arguments(call);
            match args.as_slice() {
                [arg] => {
                    if let Some(value) = arg.expr() {
                        by_id = self.write_target(entity, &written, &value);
                    }
                }
                _ => {
                    self.args_only(call);
                    self.argument_count(call, &written, entity, args.len());
                }
            }
        }
        let members = self.write_sections(entity, op, block.as_ref(), &head_span, &written);
        let mut sets = Sets {
            fields: BTreeSet::new(),
        };
        let mut missing: Vec<(String, String)> = Vec::new();
        let settable: Vec<FieldDef> = fields
            .iter()
            .filter(|field| field.name != "id" && !field.generated)
            .cloned()
            .collect();
        // `set:`.
        if let Some(set) = members.get("set") {
            self.write_set(
                entity, op, by_id, set, &fields, &settable, &head_span, &written,
            );
            for (name, member) in tree::members(set.block()) {
                let Some(field) = tree::name_text(&name) else {
                    continue;
                };
                if !settable.iter().any(|f| f.name == field) {
                    continue;
                }
                if let (Some(target), Some(value)) = (rules.reference(&field), member.value())
                    && !self.holds(&value, target)
                {
                    missing.push((field.clone(), target.to_string()));
                }
                sets.fields.insert(field);
            }
        }
        // `patch:` and `when_supplied:`.
        let patch_input = members
            .get("patch")
            .and_then(|patch| self.write_patch(entity, by_id, patch, &settable, &written));
        if let Some((_, input_fields)) = &patch_input {
            for field in input_fields {
                if settable.iter().any(|f| f.name == *field) {
                    if let Some(target) = rules.reference(field) {
                        missing.push((field.clone(), target.to_string()));
                    }
                    sets.fields.insert(field.clone());
                }
            }
        }
        if let Some(supplied) = members.get("when_supplied") {
            let input = patch_input
                .as_ref()
                .map(|(input, fields)| (input.as_str(), fields.as_slice()));
            for (field, value) in self.when_supplied(entity, by_id, supplied, input, &settable) {
                if let Some(target) = rules.reference(&field)
                    && !self.holds(&value, target)
                {
                    missing.push((field.clone(), target.to_string()));
                }
                sets.fields.insert(field);
            }
        }
        // What it can fail with: a record by `id` that isn't there, a
        // reference naming one that isn't, and the rules it can break.
        let not_found = by_id.then(|| self.not_found_of(entity, head_span.clone(), &written));
        let mut seen = BTreeSet::new();
        missing.retain(|(field, _)| seen.insert(field.clone()));
        let missing: Vec<(String, String)> = missing
            .into_iter()
            .map(|(field, target)| {
                let failure = self.not_found_of(&target, head_span.clone(), &written);
                (field, failure)
            })
            .collect();
        let breakable = breakable(entity, op, &rules, &sets.fields);
        let before = self.out.len();
        let arms = self.conflicts(
            entity,
            op,
            members.get("conflict"),
            &breakable,
            &rules,
            &head_span,
            &written,
        );
        // A mapping already reported leaves what the write fails with
        // unknown, so nothing reports a knock-on of it (an `attempt` that
        // looks needless, a `fails` entry that looks stale).
        let mapping_reported = self.out.len() > before;
        let mut failures: Vec<String> = Vec::new();
        for failure in not_found
            .iter()
            .chain(missing.iter().map(|(_, failure)| failure))
            .chain(arms.iter().map(|(_, _, failure)| failure))
        {
            if !failures.contains(failure) {
                failures.push(failure.clone());
            }
        }
        let fallible = if mapping_reported {
            self.flow.open = true;
            Fallible::Unknown
        } else if failures.is_empty() {
            Fallible::Never
        } else {
            Fallible::Fails(failures)
        };
        self.facts.writes.insert(
            node_key(node),
            WriteFact {
                entity: entity.to_string(),
                op,
                by_id,
                block,
                arms,
                missing,
                not_found,
                fallible,
            },
        );
        result
    }

    /// The record an `update` or `delete` writes: one loaded earlier in
    /// the action (`todo: Todo`), or its `id`. Whether it is the `id`.
    fn write_target(&mut self, entity: &str, written: &str, value: &Expr) -> bool {
        let id = Ty::Field(entity.to_string(), vec!["id".to_string()]);
        let record = Ty::Named(entity.to_string());
        if super::is_literal(value) {
            self.check(value, &id);
            return true;
        }
        let actual = self.infer(value);
        if self.table.assignable(&actual, &record) {
            return false;
        }
        if self.table.assignable(&actual, &id) {
            return true;
        }
        let span = self.span(value);
        self.out.push(
            Diagnostic::new(
                &rule::TYPE_MISMATCH,
                format!("`{written}` takes a `{entity}` or a `{entity}.id`, and this is `{actual}`"),
            )
            .at(span, format!("this is `{actual}`"))
            .reason(format!(
                "`{written}` writes the `{entity}` loaded earlier in the action, which it checks nobody changed since, or the record with the `{entity}.id` it is given."
            ))
            .next(format!("Pass the `{entity}` or its `id` here")),
        );
        false
    }

    /// The sections of a write's block, each once and only those it takes,
    /// by name.
    fn write_sections(
        &mut self,
        entity: &str,
        op: WriteOp,
        block: Option<&ast::MemberList>,
        head_span: &Span,
        written: &str,
    ) -> BTreeMap<String, ast::Member> {
        let mut found: BTreeMap<String, ast::Member> = BTreeMap::new();
        for item in block.iter().flat_map(|list| list.member_items()) {
            let MemberItem::Member(member) = item else {
                continue;
            };
            let Some(key) = member.member_key() else {
                continue;
            };
            let word = tree::written(key.syntax());
            let span = self.span(&key);
            let removal = removal(self.path, &member);
            if word == "invalid" {
                self.walk(|walk| walk.member(&member));
                self.out.push(
                    Diagnostic::new(
                        &rule::FAILURE_INVALID_OUTSIDE_CONSTRUCTION,
                        format!("`{written}` isn't a construction, so `invalid:` covers nothing"),
                    )
                    .at(span, "not a construction")
                    .reason(format!(
                        "`{written}` is an entity's write: each rule it can break is mapped in `conflict:`, keyed by the rule; `invalid:` is the catch-all only for the rules a construction's value can break."
                    ))
                    .next("Remove `invalid: ...`, and map each rule in `conflict:`")
                    .edit(removal, ""),
                );
                continue;
            }
            let known = sections(op).contains(&word.as_str());
            let reason = if let Some(first) = found.get(&word) {
                let first = self.span(first);
                self.walk(|walk| walk.member(&member));
                self.out.push(
                    Diagnostic::new(
                        &rule::DATA_WRITE_SECTION_UNEXPECTED,
                        format!("`{word}:` is given twice"),
                    )
                    .at(span, "given again")
                    .also(first, "first given here")
                    .reason(format!(
                        "Each section of `{written}` is given once, so there is never a question of which counts."
                    ))
                    .next(format!("Remove one `{word}:`"))
                    .edit(removal, ""),
                );
                continue;
            } else if !known {
                match word.as_str() {
                    "missing" => format!(
                        "A record that isn't there reports `{entity}NotFound` itself; `missing:` maps a query's \"nothing matched\"."
                    ),
                    "where" => format!(
                        "`{written}` writes one record: the one loaded earlier, or the one with the `id` it is given, never the records a condition finds."
                    ),
                    "empty" => "A patch that supplies no fields changes nothing and succeeds, as JSON Merge Patch does, so nothing maps it.".to_string(),
                    "set" | "patch" | "when_supplied" => format!(
                        "`{written}` takes {}.",
                        sections_list(op)
                    ),
                    _ => format!("`{written}` takes {}.", sections_list(op)),
                }
            } else if word == "patch" && found.contains_key("set")
                || word == "set" && found.contains_key("patch")
            {
                "An update gives its fields in `set:`, or changes the fields a request supplied with `patch:`, never both.".to_string()
            } else {
                found.insert(word, member);
                continue;
            };
            self.walk(|walk| walk.member(&member));
            self.out.push(
                Diagnostic::new(
                    &rule::DATA_WRITE_SECTION_UNEXPECTED,
                    format!("`{written}` takes no `{word}:`"),
                )
                .at(span, "not a section here")
                .reason(reason)
                .next(format!("Remove `{word}: ...`"))
                .edit(removal, ""),
            );
        }
        if let Some(supplied) = found.get("when_supplied")
            && !found.contains_key("patch")
        {
            let key = supplied.member_key();
            let span = key.map_or_else(|| self.span(supplied), |key| self.span(&key));
            self.walk(|walk| walk.member(supplied));
            self.out.push(
                Diagnostic::new(
                    &rule::DATA_WRITE_SECTION_UNEXPECTED,
                    format!("`{written}` has `when_supplied:` and no `patch:`"),
                )
                .at(span, "nothing supplied")
                .reason("`when_supplied:` sets further fields when the patch's request supplied a field; without `patch:` nothing is supplied.")
                .next("Change the fields with `patch: input`, or give them in `set:`"),
            );
            found.remove("when_supplied");
        }
        let required = match op {
            WriteOp::Create if !found.contains_key("set") => Some((
                format!("`{written}` gives no fields"),
                format!(
                    "A new `{entity}` gets every field the store doesn't generate, given in `set:`."
                ),
                "Add `set: { ... }` with the record's fields".to_string(),
            )),
            WriteOp::Update if !found.contains_key("set") && !found.contains_key("patch") => {
                Some((
                    format!("`{written}` doesn't say what it changes"),
                    "An update gives the fields it changes in `set:`, or the request's changes with `patch:`.".to_string(),
                    "Add `set: { ... }` with the fields it changes".to_string(),
                ))
            }
            _ => None,
        };
        if let Some((message, reason, next)) = required {
            self.out.push(
                Diagnostic::new(&rule::DATA_WRITE_SECTION_REQUIRED, message)
                    .at(head_span.clone(), "nothing to change")
                    .reason(reason)
                    .next(next),
            );
        }
        found
    }

    /// `set:`: each field once, none the store generates, each value of its
    /// field's type. A create gives every field the store doesn't
    /// generate; an update at least one. By `id`, a value may read the
    /// stored ones (area 6, "Counters and other one-step updates").
    #[allow(clippy::too_many_arguments)]
    fn write_set(
        &mut self,
        entity: &str,
        op: WriteOp,
        by_id: bool,
        set: &ast::Member,
        fields: &[FieldDef],
        settable: &[FieldDef],
        head_span: &Span,
        written: &str,
    ) {
        let generated: BTreeSet<&str> = fields
            .iter()
            .filter(|field| field.name == "id" || field.generated)
            .map(|field| field.name.as_str())
            .collect();
        let given = tree::members(set.block());
        if op == WriteOp::Update && given.is_empty() {
            let span = set
                .member_key()
                .map_or_else(|| self.span(set), |key| self.span(&key));
            self.out.push(
                Diagnostic::new(
                    &rule::DATA_WRITE_SECTION_REQUIRED,
                    format!("`{written}`'s `set:` gives no fields"),
                )
                .at(span, "nothing to change")
                .reason(
                    "An update changes at least one field: an empty `set:` would write nothing.",
                )
                .next("Add the fields it changes to `set:`"),
            );
        }
        for (name, _) in &given {
            let Some(field) = tree::name_text(name) else {
                continue;
            };
            if generated.contains(field.as_str()) {
                let span = self.span(name);
                self.set_generated(entity, &field, span, written);
            }
        }
        let skip: Vec<&str> = generated.iter().copied().collect();
        let required = if op == WriteOp::Create {
            super::expr::Required::All
        } else {
            super::expr::Required::None
        };
        let stored = by_id.then(|| entity.to_string());
        let outer = std::mem::replace(&mut self.stored, stored);
        self.fields_given(
            set.block(),
            entity,
            &[],
            settable,
            required,
            head_span.clone(),
            &skip,
        );
        self.stored = outer;
        if by_id {
            for (_, member) in &given {
                if let Some(value) = member.value() {
                    self.one_step(entity, &value, written);
                }
            }
        }
    }

    fn set_generated(&mut self, entity: &str, field: &str, span: Span, written: &str) {
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

    /// `patch: input`: an `Input` whose fields are optional and are the
    /// entity's (area 6). The input's type and the fields it changes.
    fn write_patch(
        &mut self,
        entity: &str,
        by_id: bool,
        patch: &ast::Member,
        settable: &[FieldDef],
        written: &str,
    ) -> Option<(String, Vec<String>)> {
        let value = patch.value()?;
        let actual = self.infer(&value);
        if actual.has_error() {
            return None;
        }
        let input = match actual.strip_nullable() {
            Ty::Named(name)
                if matches!(
                    self.table.def(name),
                    Some(Def::Fields {
                        base: FieldsBase::Input,
                        ..
                    })
                ) && !matches!(actual, Ty::Nullable(_)) =>
            {
                name.clone()
            }
            _ => {
                let span = self.span(&value);
                self.out.push(
                    Diagnostic::new(
                        &rule::DATA_PATCH_NOT_INPUT,
                        format!("`patch:` takes an `Input`, and this is `{actual}`"),
                    )
                    .at(span, format!("this is `{actual}`"))
                    .reason(format!(
                        "A patch changes only the fields a request supplied, so it takes an `Input` whose fields are optional and named after `{entity}`'s: one left out keeps its value, and one sent as `none` is cleared."
                    ))
                    .next(format!(
                        "Pass an `Input` such as `type Patch{entity} = Input {{ pick: [...]  optional: all }}`"
                    )),
                );
                return None;
            }
        };
        if by_id {
            self.one_step(entity, &value, written);
        }
        let input_fields: Vec<FieldDef> = self
            .table
            .fields(&input)
            .map(<[_]>::to_vec)
            .unwrap_or_default();
        let mut changed = Vec::new();
        let span = self.span(&value);
        for field in &input_fields {
            let Some(target) = settable.iter().find(|f| f.name == field.name) else {
                let all: Vec<FieldDef> = self
                    .table
                    .fields(entity)
                    .map(<[_]>::to_vec)
                    .unwrap_or_default();
                let generated = all
                    .iter()
                    .any(|f| f.name == field.name && (f.generated || f.name == "id"));
                if generated {
                    self.set_generated(entity, &field.name, span.clone(), written);
                    continue;
                }
                let names: Vec<&str> = settable.iter().map(|f| f.name.as_str()).collect();
                let near = nearest(&field.name, names.iter().copied());
                self.out.push(
                    Diagnostic::new(
                        &rule::DATA_PATCH_FIELD_UNKNOWN,
                        format!("`{input}` has `{}`, which `{entity}` doesn't", field.name),
                    )
                    .at(span.clone(), format!("`{input}`"))
                    .reason(format!(
                        "A patch sets each field its input supplies on the field of `{entity}` with the same name, so every field of `{input}` is one of `{entity}`'s.{}",
                        did_you_mean(near)
                    ))
                    .next(match near {
                        Some(near) => format!("Rename `{input}.{}` to `{near}`", field.name),
                        None => format!("Remove `{}` from `{input}`", field.name),
                    }),
                );
                continue;
            };
            if !field.optional {
                self.out.push(
                    Diagnostic::new(
                        &rule::DATA_PATCH_FIELD_REQUIRED,
                        format!("`{input}.{}` isn't optional, so a patch can't leave it out", field.name),
                    )
                    .at(span.clone(), format!("`{input}`"))
                    .reason(format!(
                        "A patch changes only the fields a request supplied, so each field of its input is optional: one left out keeps its value. A required `{}` would have to be sent every time.",
                        field.name
                    ))
                    .next(format!(
                        "Make `{}` optional in `{input}` (`optional: true`, or `optional: all`), or set it with `set:`",
                        field.name
                    )),
                );
                continue;
            }
            if !self.table.assignable(&field.declared, &target.declared) {
                self.out.push(
                    Diagnostic::new(
                        &rule::TYPE_MISMATCH,
                        format!(
                            "`{input}.{}` is `{}`, which `{entity}.{}` can't hold",
                            field.name, field.declared, field.name
                        ),
                    )
                    .at(span.clone(), format!("`{input}`"))
                    .reason(format!(
                        "A patch stores each supplied field as it is, so its type is the entity field's: `{}`.",
                        target.declared
                    ))
                    .next(format!(
                        "Declare `{}` in `{input}` as `{entity}.{}`, or `pick` it",
                        field.name, field.name
                    )),
                );
                continue;
            }
            changed.push(field.name.clone());
        }
        Some((input, changed))
    }

    /// `when_supplied: { field: { ... } }`: further fields set when the
    /// patch's request supplied `field` (area 6, question 7). Each field
    /// it sets, with its value.
    fn when_supplied(
        &mut self,
        entity: &str,
        by_id: bool,
        supplied: &ast::Member,
        input: Option<(&str, &[String])>,
        settable: &[FieldDef],
    ) -> Vec<(String, Expr)> {
        let written = format!("{entity}.update");
        let mut out: Vec<(String, Expr)> = Vec::new();
        let mut set_by: BTreeMap<String, Span> = BTreeMap::new();
        let input_fields: Vec<String> = input
            .map(|(name, _)| {
                self.table
                    .fields(name)
                    .unwrap_or(&[])
                    .iter()
                    .map(|f| f.name.clone())
                    .collect()
            })
            .unwrap_or_default();
        for (key, member) in tree::members(supplied.block()) {
            let Some(name) = tree::name_text(&key) else {
                continue;
            };
            let key_span = self.span(&key);
            if let Some((input_name, _)) = input
                && !input_fields.contains(&name)
            {
                let near = nearest(&name, input_fields.iter().map(String::as_str));
                let problem = Diagnostic::new(
                    &rule::DATA_PATCH_FIELD_UNKNOWN,
                    format!("`when_supplied:` names `{name}`, which `{input_name}` doesn't have"),
                )
                .at(key_span.clone(), "not a field of the input")
                .reason(format!(
                    "`when_supplied:` acts when the request supplied one of the patch's fields; `{input_name}` has no `{name}`.{}",
                    did_you_mean(near)
                ));
                self.out.push(match near {
                    Some(near) => problem
                        .next(format!("Use `{near}` instead of `{name}`"))
                        .edit(key_span.clone(), near),
                    None => problem.next(format!("Name one of `{input_name}`'s fields")),
                });
            }
            let patched: &[String] = input.map_or(&[], |(_, fields)| fields);
            for (field_name, field_member) in tree::members(member.block()) {
                let Some(field) = tree::name_text(&field_name) else {
                    continue;
                };
                let span = self.span(&field_name);
                if patched.contains(&field) {
                    let input_name = input.map_or("", |(name, _)| name);
                    self.out.push(
                        Diagnostic::new(
                            &rule::DATA_PATCH_OVERLAP,
                            format!("`{field}` is set by the patch and by `when_supplied:`"),
                        )
                        .at(span.clone(), "set twice")
                        .reason(format!(
                            "`{input_name}` supplies `{field}` already, so a value here would compete with the request's."
                        ))
                        .next(format!(
                            "Remove `{field}` from `when_supplied:`, or from `{input_name}`"
                        )),
                    );
                } else if let Some(first) = set_by.get(&field) {
                    self.out.push(
                        Diagnostic::new(
                            &rule::DATA_PATCH_OVERLAP,
                            format!("`{field}` is set twice by `when_supplied:`"),
                        )
                        .at(span.clone(), "set again")
                        .also(first.clone(), "first set here")
                        .reason(format!(
                            "When the request supplies both fields, two values would compete for `{field}`."
                        ))
                        .next(format!("Set `{field}` under one supplied field")),
                    );
                } else {
                    set_by.insert(field.clone(), span.clone());
                }
                if let Some(value) = field_member.value()
                    && settable.iter().any(|f| f.name == field)
                {
                    out.push((field.clone(), value));
                }
            }
            let generated: Vec<FieldDef> = self
                .table
                .fields(entity)
                .unwrap_or(&[])
                .iter()
                .filter(|f| f.generated || f.name == "id")
                .cloned()
                .collect();
            for (field_name, _) in tree::members(member.block()) {
                if let Some(field) = tree::name_text(&field_name)
                    && generated.iter().any(|f| f.name == field)
                {
                    let span = self.span(&field_name);
                    self.set_generated(entity, &field, span, &written);
                }
            }
            let skip: Vec<String> = generated.iter().map(|f| f.name.clone()).collect();
            let skip: Vec<&str> = skip.iter().map(String::as_str).collect();
            let head = self.span(&key);
            self.fields_given(
                member.block(),
                entity,
                &[],
                settable,
                super::expr::Required::None,
                head,
                &skip,
            );
            if by_id {
                for (_, field_member) in tree::members(member.block()) {
                    if let Some(value) = field_member.value() {
                        self.one_step(entity, &value, &written);
                    }
                }
            }
        }
        out
    }

    /// Whether a reference's value names a record that is surely there:
    /// `none`, or the `id` (or a reference to the same entity) of a record
    /// loaded earlier, which the store holds while the action's
    /// transaction lasts. Any other value may name one that isn't.
    fn holds(&self, value: &Expr, target: &str) -> bool {
        let value = match value {
            Expr::ParenExpr(paren) => match paren.expr() {
                Some(inner) => inner,
                None => return false,
            },
            other => other.clone(),
        };
        if matches!(value, Expr::Literal(_)) && tree::written(value.syntax()) == "none" {
            return true;
        }
        if !matches!(value, Expr::FieldExpr(_)) {
            return false;
        }
        // The field read, as its host and path: `Shelf` and `id`.
        let Some((holder, path)) = self.facts.fields.get(&node_key(value.syntax())) else {
            return false;
        };
        let [name] = path.as_slice() else {
            return false;
        };
        let name = name.clone();
        if self.callable_kind(holder) != Some(ItemKind::Entity) {
            return false;
        }
        if name == "id" {
            return holder == target;
        }
        self.writes
            .write_rules(holder)
            .and_then(|rules| rules.reference(&name))
            .is_some_and(|points| points == target)
    }

    /// A value an update by `id` sets (area 6, "Counters and other
    /// one-step updates"): it uses only the action's parameters, constants,
    /// `clock.now` and the record's stored fields, never a value read
    /// earlier in the action; and the database works out a stored field
    /// only with `+` and `-`, under constructions that can't fail.
    fn one_step(&mut self, entity: &str, value: &Expr, written: &str) {
        let root = value.syntax().clone();
        for node in std::iter::once(root.clone()).chain(root.descendants().skip(1)) {
            if let Some(name) = ast::NameRef::cast(node.clone())
                && let Some(local) = self.facts.reads.get(&node_key(name.syntax()))
                && let Some(fact) = self.facts.locals.get(*local)
                && fact.kind != LocalKind::Param
            {
                let span = self.span(&name);
                let what = match fact.kind {
                    LocalKind::Item => "a loop item",
                    LocalKind::Binding => "a binding",
                    LocalKind::Inline => "an inline parameter",
                    _ => "a variable",
                };
                self.out.push(
                    Diagnostic::new(
                        &rule::DATA_UPDATE_BY_ID_VALUE,
                        format!("`{written}` by `id` sets a value from `{}`, {what}", fact.name),
                    )
                    .at(span, what)
                    .reason(format!(
                        "An update by `id` is worked out by the database in one step, so its values use only the action's parameters, constants, `clock.now` and `{entity}`'s stored fields; a value read earlier could have changed by the time it writes."
                    ))
                    .next(format!(
                        "Load the record and update it, `{written}({}) {{ set: {{ ... }} }}`, or pass the value in as a parameter",
                        crate::case::to_snake(entity)
                    )),
                );
                return;
            }
            if let Some(call) = ast::CallExpr::cast(node.clone()) {
                let reach = self.facts.reaches.get(&node_key(call.syntax()));
                let effect = matches!(
                    reach,
                    Some(Reach::Entity { .. })
                        | Some(Reach::Callable {
                            kind: ItemKind::Action | ItemKind::Query | ItemKind::Fixture,
                            ..
                        })
                );
                if effect {
                    let span = self.span(&call);
                    self.out.push(
                        Diagnostic::new(
                            &rule::DATA_UPDATE_BY_ID_VALUE,
                            format!("`{written}` by `id` sets a value from a call that reads or writes"),
                        )
                        .at(span, "reads or writes")
                        .reason(format!(
                            "An update by `id` is worked out by the database in one step, so its values use only the action's parameters, constants, `clock.now` and `{entity}`'s stored fields."
                        ))
                        .next("Make the call before the update, and load the record to update it"),
                    );
                    return;
                }
            }
            if self.facts.stored.contains(&node_key(&node)) {
                self.stored_operand(&node, &root, entity, written);
            }
        }
    }

    /// A stored field inside a value: it may stand alone, or under `+`,
    /// `-`, brackets and constructions that can't fail, which the database
    /// works out exactly as Jadpo would.
    fn stored_operand(
        &mut self,
        stored: &SyntaxNode,
        root: &SyntaxNode,
        entity: &str,
        written: &str,
    ) {
        let mut child = stored.clone();
        while child != *root {
            let Some(parent) = child.parent() else {
                return;
            };
            let allowed = match parent.kind() {
                SyntaxKind::PAREN_EXPR | SyntaxKind::ARG | SyntaxKind::ARG_LIST => true,
                SyntaxKind::BIN_EXPR => ast::BinExpr::cast(parent.clone())
                    .and_then(|binary| binary.op())
                    .is_some_and(|op| matches!(op.text(), "+" | "-")),
                SyntaxKind::PREFIX_EXPR => ast::PrefixExpr::cast(parent.clone())
                    .and_then(|prefix| prefix.op())
                    .is_some_and(|op| op.kind() == SyntaxKind::MINUS),
                SyntaxKind::CALL_EXPR => {
                    let key = node_key(&parent);
                    match self.facts.reaches.get(&key) {
                        Some(Reach::Construct(_)) => {
                            if let Some(Fallible::Construction(rules)) =
                                self.facts.constructions.get(&key).cloned()
                            {
                                let span = crate::tree::node_span(self.path, &parent);
                                let rules = match ast::CallExpr::cast(parent.clone()) {
                                    Some(call) => self.written_rules(&call, &rules),
                                    None => crate::and_list(&rules),
                                };
                                self.out.push(
                                    Diagnostic::new(
                                        &rule::DATA_STORED_VALUE_UNPROVEN,
                                        format!(
                                            "`{written}` by `id` could store a value that breaks {rules}"
                                        ),
                                    )
                                    .at(span, "worked out by the database")
                                    .reason(format!(
                                        "The database works out a value from `{entity}`'s stored fields when it writes, so nothing checks it before it is stored: the declared ranges must prove it follows its type's rules."
                                    ))
                                    .next(format!(
                                        "Load the record and build the value with `attempt` before `{written}`, or loosen the type's rules"
                                    )),
                                );
                                return;
                            }
                            true
                        }
                        _ => false,
                    }
                }
                _ => false,
            };
            if !allowed {
                let span = crate::tree::node_span(self.path, stored);
                self.out.push(
                    Diagnostic::new(
                        &rule::DATA_UPDATE_BY_ID_VALUE,
                        format!(
                            "`{written}` by `id` works out a stored field with more than `+` and `-`"
                        ),
                    )
                    .at(span, "stored value")
                    .reason(format!(
                        "The database works out a value from `{entity}`'s stored fields in one step, with `+` and `-`, which give the same result in SQL as in Jadpo; anything else needs the record loaded first."
                    ))
                    .next(format!(
                        "Load the record and update it, `{written}({}) {{ set: {{ ... }} }}`",
                        crate::case::to_snake(entity)
                    )),
                );
                return;
            }
            child = parent;
        }
    }

    /// `Post.view_count` as a value: the stored field, which an update by
    /// `id` of that entity may read (area 6, "Counters"). Anywhere else it
    /// is a mistake. Its type, or `None` when `head` isn't an entity with
    /// that field.
    pub(super) fn stored_field(
        &mut self,
        head: &str,
        rest: &[ast::NameRef],
        expr: &Expr,
    ) -> Option<Ty> {
        if self.callable_kind(head) != Some(ItemKind::Entity) {
            return None;
        }
        let [field] = rest else { return None };
        let field = text(field);
        self.table.field(head, std::slice::from_ref(&field))?;
        if self.stored.as_deref() == Some(head) {
            self.facts.stored.insert(node_key(expr.syntax()));
            return Some(Ty::Field(head.to_string(), vec![field]));
        }
        let span = self.span(expr);
        let snake = crate::case::to_snake(head);
        let reason = match &self.stored {
            Some(other) => format!(
                "Only the record being updated is read as it is stored: this update by `id` writes `{other}`, not `{head}`."
            ),
            None => format!(
                "`{head}.{field}` is the value stored in the database when an update by `id` runs, which only that update's `set:` reads; elsewhere a record's field is read from the record loaded earlier."
            ),
        };
        self.out.push(
            Diagnostic::new(
                &rule::DATA_STORED_VALUE_OUTSIDE_UPDATE,
                format!("`{head}.{field}` is a stored value, read only by an update by `id`"),
            )
            .at(span, "stored value")
            .reason(reason)
            .next(format!(
                "Read it from a loaded record, `{snake}.{field}`, or use it in `{head}.update({snake}_id) {{ set: {{ ... }} }}`"
            )),
        );
        Some(Ty::Error)
    }

    // --- conflicts ----------------------------------------------------------

    /// `conflict: { Rule: Failure }`: each rule the write can break mapped
    /// once to a `conflict` failure, with no catch-all (area 6, question
    /// 6). The arms that map a rule.
    #[allow(clippy::too_many_arguments)]
    fn conflicts(
        &mut self,
        entity: &str,
        op: WriteOp,
        conflict: Option<&ast::Member>,
        breakable: &[Breach],
        rules: &WriteRules,
        head_span: &Span,
        written: &str,
    ) -> Vec<(ast::Member, Breach, String)> {
        let mut out: Vec<(ast::Member, Breach, String)> = Vec::new();
        let mut first: BTreeMap<Breach, Span> = BTreeMap::new();
        let mut unknown = false;
        if let Some(conflict) = conflict {
            if conflict.block().is_none()
                && let Some(value) = conflict.value()
            {
                self.walk(|walk| walk.expr(&value));
                let span = self.span(&value);
                self.out.push(
                    Diagnostic::new(
                        &rule::DATA_WRITE_SECTION_UNEXPECTED,
                        "`conflict:` maps each rule in a block",
                    )
                    .at(span, "one failure for every rule")
                    .reason("Each rule a write can break is mapped on its own line, keyed by the rule, so there is no catch-all.")
                    .next(format!(
                        "Write `conflict: {{ {}: ... }}`",
                        breakable.first().map_or_else(|| format!("{entity}.rule"), Breach::written)
                    )),
                );
            }
            for item in conflict.block().iter().flat_map(|list| list.member_items()) {
                let MemberItem::Member(member) = item else {
                    continue;
                };
                let value = member.value();
                if let Some(value) = &value {
                    self.resolved_by(|names, out| {
                        names.mapped(value, out);
                    });
                }
                let Some(key) = member.member_key() else {
                    continue;
                };
                let arm = Arm {
                    key: tree::written(key.syntax()),
                    key_span: self.span(&key),
                    member: member.clone(),
                    value: value.clone(),
                };
                let failure = value.as_ref().and_then(|v| self.failure_given(v));
                let names: Vec<String> = key.names().filter_map(|n| tree::name_text(&n)).collect();
                let rule = match names.as_slice() {
                    [owner, name] => self.breach(owner, name),
                    _ => None,
                };
                let Some(rule) = rule else {
                    self.conflict_unknown(&arm, entity, op, breakable, rules);
                    unknown = true;
                    continue;
                };
                if let Some(earlier) = first.get(&rule) {
                    self.duplicate_mapping(&arm, earlier.clone());
                    continue;
                }
                first.insert(rule.clone(), arm.key_span.clone());
                if !breakable.contains(&rule) {
                    self.conflict_cannot_break(&arm, &rule, entity, op, written);
                    continue;
                }
                if let Some(failure) = failure {
                    self.mapping_kind(&arm, &failure, "conflict");
                    out.push((member, rule, failure));
                }
            }
        }
        // An arm naming no rule was most likely meant for one of these.
        let unmapped: Vec<String> = if unknown {
            Vec::new()
        } else {
            breakable
                .iter()
                .filter(|rule| !first.contains_key(rule))
                .map(Breach::written)
                .collect()
        };
        if let Some(first_rule) = unmapped.first() {
            let listed: Vec<String> = unmapped.iter().map(|r| format!("`{r}`")).collect();
            let (them, each) = if unmapped.len() == 1 {
                ("it", "it needs")
            } else {
                ("them", "each needs")
            };
            let why = match op {
                WriteOp::Delete => "A delete is refused while a `restricted` reference points at the record, or at one its delete cascades to".to_string(),
                _ => "A write is refused when it would give two records the same values a unique rule covers".to_string(),
            };
            self.out.push(
                Diagnostic::new(
                    &rule::DATA_CONFLICT_UNMAPPED,
                    format!("`{written}` can break {}, and nothing maps {them}", crate::and_list(&listed)),
                )
                .at(head_span.clone(), "can be refused")
                .reason(format!(
                    "{why}; {each} an arm in `conflict:` naming the `conflict` failure the caller gets, since there is no catch-all."
                ))
                .next(if conflict.is_some() {
                    format!("Add `{first_rule}: SomeFailure` to `conflict:`")
                } else {
                    format!("Add `conflict: {{ {first_rule}: SomeFailure }}` to the block")
                }),
            );
        }
        out
    }

    /// The rule a `conflict:` arm's key names: a unique rule or field of
    /// `owner`, or one of its references.
    fn breach(&self, owner: &str, name: &str) -> Option<Breach> {
        let rules = self.writes.write_rules(owner)?;
        if let Some(rule) = rules.unique(name) {
            return Some(Breach::Unique {
                entity: owner.to_string(),
                name: name.to_string(),
                field: rule.field,
            });
        }
        rules.reference(name).map(|_| Breach::Reference {
            entity: owner.to_string(),
            field: name.to_string(),
        })
    }

    fn conflict_unknown(
        &mut self,
        arm: &Arm,
        entity: &str,
        op: WriteOp,
        breakable: &[Breach],
        rules: &WriteRules,
    ) {
        let mut known: Vec<String> = breakable.iter().map(Breach::written).collect();
        for rule in &rules.unique {
            let written = format!("{entity}.{}", rule.name);
            if !known.contains(&written) {
                known.push(written);
            }
        }
        let near = nearest(&arm.key, known.iter().map(String::as_str)).map(str::to_string);
        let rules_line = if breakable.is_empty() {
            format!("`{entity}.{}` can break no rule.", op.word())
        } else {
            let listed: Vec<String> = breakable
                .iter()
                .map(|b| format!("`{}`", b.written()))
                .collect();
            format!(
                "`{entity}.{}` can break {}.",
                op.word(),
                crate::and_list(&listed)
            )
        };
        let problem = Diagnostic::new(
            &rule::DATA_CONFLICT_UNKNOWN,
            format!("`{}` names no unique rule or reference", arm.key),
        )
        .at(arm.key_span.clone(), "not a rule")
        .reason(format!(
            "A `conflict:` arm is keyed by the rule the store refuses: a unique field (`User.email`), a named unique rule (`Project.name_per_owner`) or a reference that refuses a delete (`Todo.project_id`). {rules_line}{}",
            did_you_mean(near.as_deref())
        ));
        self.out.push(match near {
            Some(near) => problem
                .next(format!("Use `{near}` instead of `{}`", arm.key))
                .edit(arm.key_span.clone(), near),
            None => problem.next(format!("Remove `{}: ...`", arm.key)),
        });
    }

    fn conflict_cannot_break(
        &mut self,
        arm: &Arm,
        rule: &Breach,
        entity: &str,
        op: WriteOp,
        written: &str,
    ) {
        let reason = match (rule, op) {
            (
                Breach::Reference {
                    entity: holder,
                    field,
                },
                WriteOp::Create | WriteOp::Update,
            ) if holder == entity => {
                let target = self
                    .writes
                    .write_rules(holder)
                    .and_then(|rules| rules.reference(field))
                    .unwrap_or_default()
                    .to_string();
                format!(
                    "A reference naming a record that isn't there reports `{target}NotFound`, which `attempt` passes on; only a delete is refused by a reference, as a conflict."
                )
            }
            (
                Breach::Reference {
                    entity: holder,
                    field,
                },
                _,
            ) => format!(
                "Deleting a `{entity}` is refused only by a `restricted` reference to it, or to a record its delete cascades to; `{holder}.{field}` isn't one."
            ),
            (Breach::Unique { .. }, WriteOp::Delete) => {
                "A delete removes a record, so it never breaks a unique rule.".to_string()
            }
            (Breach::Unique { entity: holder, .. }, _) if holder != entity => format!(
                "`{written}` writes `{entity}`, so it can break only `{entity}`'s unique rules."
            ),
            (Breach::Unique { .. }, _) => format!(
                "`{written}` sets none of the fields `{}` covers, so it can't break it.",
                rule.written()
            ),
        };
        let removal = removal(self.path, &arm.member);
        self.out.push(
            Diagnostic::new(
                &rule::DATA_CONFLICT_CANNOT_BREAK,
                format!(
                    "`{written}` can't break `{}`, so it takes no arm",
                    rule.written()
                ),
            )
            .at(arm.key_span.clone(), "never runs")
            .reason(reason)
            .next(format!("Remove `{}: ...`", arm.key))
            .edit(removal, ""),
        );
    }
}

/// The rules a write can break (area 6, "When a write is refused"): a
/// create, every unique rule; an update, each unique rule over a field it
/// sets; a delete, each reference that refuses it.
pub(crate) fn breakable(
    entity: &str,
    op: WriteOp,
    rules: &WriteRules,
    sets: &BTreeSet<String>,
) -> Vec<Breach> {
    match op {
        WriteOp::Create | WriteOp::Update => rules
            .unique
            .iter()
            .filter(|rule| op == WriteOp::Create || rule.fields.iter().any(|f| sets.contains(f)))
            .map(|rule| Breach::Unique {
                entity: entity.to_string(),
                name: rule.name.clone(),
                field: rule.field,
            })
            .collect(),
        WriteOp::Delete => rules
            .restricting
            .iter()
            .map(|(holder, field)| Breach::Reference {
                entity: holder.clone(),
                field: field.clone(),
            })
            .collect(),
    }
}

/// The edit that removes a member's line from its block.
fn removal(path: &str, member: &ast::Member) -> Span {
    let node = member.syntax();
    let end = u32::from(node.text_range().end());
    let start = node
        .first_token()
        .and_then(|token| {
            std::iter::successors(token.prev_token(), |t| t.prev_token())
                .find(|t| !t.kind().is_trivia())
        })
        .map_or_else(
            || u32::from(node.text_range().start()),
            |token| u32::from(token.text_range().end()),
        );
    Span::new(path, start, end)
}
