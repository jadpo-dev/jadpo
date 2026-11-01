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
use crate::data::{TransitionRule, WriteRules};
use crate::tree::{self, dotted};

/// What the walk decided about one write, for its typed form.
#[derive(Clone, Debug)]
pub(crate) struct WriteFact {
    pub entity: String,
    pub op: WriteOp,
    /// The lifecycle transition it runs: a transition's own operation, or
    /// a `delete` of an entity whose lifecycle has a `delete` transition.
    pub transition: Option<String>,
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
    /// A person's erasure, `User.erase(user_id)` (area 15; chunk P3): a
    /// `delete` of the person's record by `id`, as policy sees it, which
    /// decides what happens to every record of theirs too.
    pub erase: bool,
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
    /// A lifecycle transition whose `when:` doesn't hold: `Loan.check_in`
    /// (area 5, "Lifecycle"; chosen in chunk 3.4).
    Transition { entity: String, name: String },
}

impl Breach {
    /// As a `conflict:` arm writes it: `Project.name_per_owner`.
    pub fn written(&self) -> String {
        match self {
            Breach::Unique { entity, name, .. } => format!("{entity}.{name}"),
            Breach::Reference { entity, field } => format!("{entity}.{field}"),
            Breach::Transition { entity, name } => format!("{entity}.{name}"),
        }
    }

    /// The entity whose rule it is.
    pub fn holder(&self) -> &str {
        match self {
            Breach::Unique { entity, .. }
            | Breach::Reference { entity, .. }
            | Breach::Transition { entity, .. } => entity,
        }
    }
}

/// The sections each write takes, in the order a reader meets them: a
/// lifecycle transition's changes are fixed in the lifecycle, so it takes
/// only `conflict:`.
fn sections(op: WriteOp, transition: bool) -> &'static [&'static str] {
    match op {
        _ if transition => &["conflict"],
        WriteOp::Create => &["set", "conflict"],
        WriteOp::Update => &["set", "patch", "when_supplied", "conflict"],
        WriteOp::Delete => &["conflict"],
    }
}

/// "`set:` and `conflict:`".
fn sections_list(op: WriteOp, transition: bool) -> String {
    let listed: Vec<String> = sections(op, transition)
        .iter()
        .map(|s| format!("`{s}:`"))
        .collect();
    crate::and_list(&listed)
}

/// The fields a write sets, with where.
struct Sets {
    /// Each field set in `set:`, `patch:` or `when_supplied:`.
    fields: BTreeSet<String>,
}

impl Body<'_> {
    // --- history ------------------------------------------------------------

    /// `Entity.history(id)` and `Entity.history(id, after: cursor)` (area
    /// 16, "Audit trail"; chunk B11): a page of an audited record's
    /// entries, newest first.
    pub(super) fn entity_history(&mut self, entity: &str, call: &ast::CallExpr, span: Span) -> Ty {
        let written = format!("{entity}.{}", jadpo_std::audit::HISTORY);
        self.read_in_loop(span.clone(), &written);
        let audited = self
            .writes
            .write_rules(entity)
            .is_some_and(|rules| rules.audited);
        if !audited {
            self.args_only(call);
            self.out.push(
                Diagnostic::new(
                    &rule::SEMANTIC_UNKNOWN_CALLEE,
                    format!("`{entity}` keeps no history"),
                )
                .at(span, "not audited")
                .reason(format!(
                    "`{written}` reads the history of changes an entity's `audit:` keeps (area 16, \"Audit trail\"), and `{entity}` has no `audit:`."
                ))
                .next(format!(
                    "Give `{entity}` an `audit:` section to keep a history of its changes"
                )),
            );
            return Ty::Error;
        }
        let id = Ty::Field(entity.to_string(), vec!["id".to_string()]);
        let args = Self::arguments(call);
        let after = args.get(1);
        let fits = match args.as_slice() {
            [_] => true,
            [_, after] => Self::arg_name(after).as_deref() == Some("after"),
            _ => false,
        };
        if !fits || args.first().is_some_and(|arg| arg.name().is_some()) {
            self.args_only(call);
            let snake = crate::case::to_snake(entity);
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_ARGUMENT_COUNT,
                    format!("`{written}` takes a record's `id` and, for a later page, `after:`"),
                )
                .at(self.span(call), "an `id`, then `after:`")
                .reason(format!(
                    "`{written}` reads one page of the history of the `{entity}` with the `{entity}.id` it is given, newest first; the page's `next`, given back as `after:`, reads the page after it."
                ))
                .next(format!(
                    "Call `{written}({snake}_id)`, or `{written}({snake}_id, after: page.next)`"
                )),
            );
            return Ty::Error;
        }
        if let Some(value) = args.first().and_then(|arg| arg.expr()) {
            self.check(&value, &id);
        }
        if let Some(value) = after.and_then(|arg| arg.expr()) {
            self.check(&value, &Ty::Prim(crate::types::Prim::PageCursor).nullable());
        }
        Ty::Opaque(
            super::expr::PAGE.to_string(),
            vec![Ty::Opaque(
                jadpo_std::audit::AUDIT_ENTRY.to_string(),
                Vec::new(),
            )],
        )
    }

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
    /// the compiler provides when it declares none (area 6, "Write failures"). A
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

    /// A read inside a `for` loop reads once per item (area 6, "Reads in loops").
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
        let head = text(&first);
        if !crate::case::starts_upper(&head) || self.is_local(&head) {
            return None;
        }
        let op = match text(operation).as_str() {
            "update" => WriteOp::Update,
            "delete" => WriteOp::Delete,
            // A person's erasure deletes their record (area 15; chunk P3),
            // and a block after it is its own, which it refuses.
            crate::resolve::ERASE
                if self.callable_kind(&head) == Some(ItemKind::Entity)
                    && self
                        .writes
                        .write_rules(&head)
                        .is_some_and(|rules| rules.person) =>
            {
                WriteOp::Delete
            }
            // A lifecycle transition is an update of the fields it sets.
            name if self.callable_kind(&head) == Some(ItemKind::Entity)
                && self
                    .writes
                    .write_rules(&head)
                    .is_some_and(|rules| rules.transition(name).is_some()) =>
            {
                WriteOp::Update
            }
            _ => return None,
        };
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
                FieldsBase::EmailPartial => "email partial",
                FieldsBase::Message => "message",
                FieldsBase::Prompt => "prompt",
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
    #[allow(clippy::too_many_arguments)]
    pub(super) fn write(
        &mut self,
        entity: &str,
        op: WriteOp,
        node: &SyntaxNode,
        call: Option<&ast::CallExpr>,
        block: Option<ast::MemberList>,
        head_span: Span,
        transition: Option<&str>,
    ) -> Ty {
        let rules = self.writes.write_rules(entity).cloned().unwrap_or_default();
        // A delete runs the lifecycle's `delete` transition; with none, code
        // can't remove a record whose lifecycle keeps it (area 5).
        let mut transition = transition.map(str::to_string);
        if op == WriteOp::Delete && transition.is_none() {
            if rules.transition("delete").is_some() {
                transition = Some("delete".to_string());
            } else {
                self.hard_delete(entity, &rules, head_span.clone());
            }
        }
        let written = match &transition {
            Some(name) => format!("{entity}.{name}"),
            None => format!("{entity}.{}", op.word()),
        };
        if self.test {
            self.out.push(
                Diagnostic::new(
                    &rule::SEMANTIC_NOT_CALLABLE,
                    format!("A test can't call `{written}`"),
                )
                .at(head_span.clone(), "writes a record")
                .reason(format!(
                    "Only actions write records (area 6), so a test changes data through the action that does it, which is what it proves; a fixture, a test-only action, builds the data a test starts from. A test may still read, with `{entity}.get` and queries."
                ))
                .next(format!(
                    "Call the action that writes `{entity}`, or move the write into a fixture"
                )),
            );
        }
        let result = match op {
            WriteOp::Delete => Ty::Prim(Prim::Unit),
            _ => Ty::Named(entity.to_string()),
        };
        let fields: Vec<FieldDef> = self
            .table
            .fields(entity)
            .map(<[_]>::to_vec)
            .unwrap_or_default();
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
        let members = self.write_sections(
            entity,
            op,
            block.as_ref(),
            &head_span,
            &written,
            transition.is_some(),
        );
        let mut sets = Sets {
            fields: BTreeSet::new(),
        };
        if let Some(rule) = transition
            .as_deref()
            .and_then(|name| rules.transition(name))
        {
            sets.fields.extend(rule.sets.iter().cloned());
        }
        let mut missing: Vec<(String, String)> = Vec::new();
        // The fields a write gives: not the store's own, and not those only
        // the lifecycle's transitions change.
        let settable: Vec<FieldDef> = fields
            .iter()
            .filter(|field| field.name != "id" && !field.generated)
            .filter(|field| !rules.owned.contains(&field.name))
            .cloned()
            .collect();
        // `set:`.
        if let Some(set) = members.get("set") {
            self.write_set(
                entity, op, by_id, set, &fields, &settable, &head_span, &written, &rules,
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
            .and_then(|patch| self.write_patch(entity, by_id, patch, &settable, &written, &rules));
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
            for (field, value) in
                self.when_supplied(entity, by_id, supplied, input, &settable, &rules)
            {
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
        let breakable = breakable(entity, op, &rules, &sets.fields, transition.as_deref());
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
                transition,
                by_id,
                block,
                arms,
                missing,
                not_found,
                fallible,
                erase: false,
            },
        );
        result
    }

    // --- a person's erasure -----------------------------------------------

    /// `Person.erase(person_id)` (area 15, "Erasure"; chunk P3): deletes
    /// the person's record and decides what happens to every record of
    /// theirs, in one statement. It takes the person's `id` and no block,
    /// needs `delete` on the person's record as policy sees it, and fails
    /// with the person's not-found failure. An erasure reaching what isn't
    /// built yet is refused here, where it is written, so none is ever
    /// partial. Its type.
    pub(super) fn erase(
        &mut self,
        entity: &str,
        call: &ast::CallExpr,
        block: Option<ast::MemberList>,
        head_span: Span,
    ) -> Ty {
        let written = format!("{entity}.erase");
        let person = self
            .writes
            .write_rules(entity)
            .is_some_and(|rules| rules.person);
        if !person {
            self.args_only(call);
            self.walk(|walk| walk.member_values(block));
            self.out.push(
                Diagnostic::new(
                    &rule::SEMANTIC_UNKNOWN_CALLEE,
                    format!("`{entity}` has no operation `erase`"),
                )
                .at(head_span, "not a person")
                .reason(format!(
                    "`erase` is the operation generated for an entity marked `person: true`: it deletes the person's record and decides what happens to every record of theirs. `{entity}` isn't a person, so it has `get`, `create`, `update` and `delete`, with one more for each transition its lifecycle names."
                ))
                .next(format!(
                    "Delete the record with `{entity}.delete`, or mark `{entity}` `person: true` if each record is a person"
                )),
            );
            return Ty::Error;
        }
        if self.test {
            self.out.push(
                Diagnostic::new(
                    &rule::SEMANTIC_NOT_CALLABLE,
                    format!("A test can't call `{written}`"),
                )
                .at(head_span.clone(), "erases a person")
                .reason(format!(
                    "Only actions write records (area 6), so a test erases a person through the action that does it, which is what it proves. A test may still read, with `{entity}.get` and queries, to see what erasure left."
                ))
                .next(format!("Call the action that erases the `{entity}`")),
            );
        }
        let id = Ty::Field(entity.to_string(), vec!["id".to_string()]);
        let args = Self::arguments(call);
        match args.as_slice() {
            [arg] => {
                if let Some(value) = arg.expr() {
                    self.erase_target(entity, &written, &id, &value);
                }
            }
            _ => {
                self.args_only(call);
                let snake = crate::case::to_snake(entity);
                self.out.push(
                    Diagnostic::new(
                        &rule::TYPE_ARGUMENT_COUNT,
                        format!("`{written}` takes one argument, and {} given", args.len()),
                    )
                    .at(self.span(call), "one argument")
                    .reason(format!(
                        "`{written}` erases the person with the `{entity}.id` it is given."
                    ))
                    .next(format!("Call `{written}({snake}_id)`")),
                );
            }
        }
        // `erase` takes no block: nothing it does is written at the call.
        for item in block.iter().flat_map(|list| list.member_items()) {
            let MemberItem::Member(member) = item else {
                continue;
            };
            let Some(key) = member.member_key() else {
                continue;
            };
            let word = tree::written(key.syntax());
            let span = self.span(&key);
            let removal = crate::tree::removal(self.path, &member);
            self.walk(|walk| walk.member(&member));
            self.out.push(
                Diagnostic::new(
                    &rule::DATA_WRITE_SECTION_UNEXPECTED,
                    format!("`{written}` takes no `{word}:`"),
                )
                .at(span, "not a section here")
                .reason(format!(
                    "What `{written}` does to each record is written once, on the references that link it to the person (`erase:` and `on_delete:`), and nothing can refuse it, so it takes no block."
                ))
                .next(format!("Remove `{word}: ...`"))
                .edit(removal, ""),
            );
        }
        // What isn't built yet, refused where the erasure is written.
        let unbuilt = self
            .writes
            .erasure(entity)
            .map(|erasure| erasure.unbuilt.clone())
            .unwrap_or_default();
        if let Some(first) = unbuilt.first() {
            let mut first = first.clone();
            if let Some(head) = first.get(..1) {
                first = format!("{}{}", head.to_uppercase(), &first[1..]);
            }
            self.out.push(
                Diagnostic::new(
                    &rule::TOOLCHAIN_NOT_BUILT_YET,
                    format!("Erasing a `{entity}` here isn't built yet"),
                )
                .at(head_span.clone(), "decided, not built")
                .reason(format!(
                    "{first}, which erasure can't do yet: accepted, the app would believe a `{entity}` erased while data of theirs remains."
                ))
                .next(format!("Erase a `{entity}` in an action of your own for now")),
            );
        }
        let not_found = self.not_found_of(entity, head_span, &written);
        self.facts.writes.insert(
            node_key(call.syntax()),
            WriteFact {
                entity: entity.to_string(),
                op: WriteOp::Delete,
                transition: None,
                by_id: true,
                block,
                arms: Vec::new(),
                missing: Vec::new(),
                not_found: Some(not_found.clone()),
                fallible: Fallible::Fails(vec![not_found]),
                erase: true,
            },
        );
        Ty::Prim(Prim::Unit)
    }

    /// The person an erasure erases: their `id`. A record loaded earlier
    /// is passed by its `id`, so the call says which person whatever was
    /// read.
    fn erase_target(&mut self, entity: &str, written: &str, id: &Ty, value: &Expr) {
        if super::is_literal(value) {
            self.check(value, id);
            return;
        }
        let actual = self.infer(value);
        if self.table.assignable(&actual, id) || actual.has_error() {
            return;
        }
        let span = self.span(value);
        let record = Ty::Named(entity.to_string());
        let problem = Diagnostic::new(
            &rule::TYPE_MISMATCH,
            format!("`{written}` takes a `{entity}.id`, and this is `{actual}`"),
        )
        .at(span.clone(), format!("this is `{actual}`"))
        .reason(format!(
            "`{written}` erases the person with the `{entity}.id` it is given."
        ));
        self.out.push(if self.table.assignable(&actual, &record) {
            let shown = crate::tree::written(value.syntax());
            problem
                .next(format!("Pass `{shown}.id`"))
                .edit(span, format!("{shown}.id"))
        } else {
            problem.next(format!("Pass the `{entity}`'s `id` here"))
        });
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
        transition: bool,
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
            let removal = crate::tree::removal(self.path, &member);
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
            let known = sections(op, transition).contains(&word.as_str());
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
            } else if !known
                && transition
                && matches!(word.as_str(), "set" | "patch" | "when_supplied")
            {
                format!(
                    "`{written}` is a lifecycle transition: the fields it changes are fixed in the lifecycle's `set:`, so it takes only `conflict:`."
                )
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
                        sections_list(op, transition)
                    ),
                    _ => format!("`{written}` takes {}.", sections_list(op, transition)),
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
            _ if transition => None,
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
        rules: &WriteRules,
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
        let mut skip: Vec<&str> = generated.iter().copied().collect();
        if op == WriteOp::Update {
            for (name, _) in &given {
                let Some(field) = tree::name_text(name) else {
                    continue;
                };
                let span = self.span(name);
                self.fixed_field(entity, &field, span, written, rules);
            }
        }
        for (name, _) in &given {
            let Some(field) = tree::name_text(name) else {
                continue;
            };
            if rules.owned.contains(&field) {
                let span = self.span(name);
                self.owned_field(entity, &field, span, written, rules);
                if let Some(owned) = rules.owned.iter().find(|owned| **owned == field) {
                    skip.push(owned.as_str());
                }
            }
        }
        let required = if op == WriteOp::Create {
            super::expr::Required::All
        } else {
            super::expr::Required::None
        };
        let stored = by_id.then(|| entity.to_string());
        let outer = std::mem::replace(&mut self.stored, stored);
        // A key-type field keeps a key held as data sealed (area 19; chunk
        // P9): one of the two places it may go.
        for (name, member) in &given {
            let target = tree::name_text(name)
                .and_then(|name| fields.iter().find(|field| field.name == name));
            if let (Some(target), Some(value)) = (target, member.value())
                && self
                    .table
                    .base_prim(&target.declared)
                    .is_some_and(|prim| prim.is_key())
            {
                self.facts.key_sinks.insert(node_key(value.syntax()));
            }
        }
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
            for (name, member) in &given {
                if let Some(value) = member.value() {
                    self.one_step(entity, &value, written);
                    if let Some(field) = tree::name_text(name) {
                        self.stored_scale(entity, &field, &value, written);
                    }
                }
            }
        }
    }

    /// An update changing a field that never changes (`immutable: true`),
    /// or one policy reaches the caller or the tenant through, which only
    /// an action listed in root `policy { actions: }` changes (area 9).
    /// Whether it reported one.
    fn fixed_field(
        &mut self,
        entity: &str,
        field: &str,
        span: Span,
        written: &str,
        rules: &WriteRules,
    ) -> bool {
        let immutable = rules.immutable.iter().any(|f| f == field);
        let fixed = rules.fixed.iter().any(|f| f == field)
            && !self
                .action
                .as_deref()
                .is_some_and(|action| crate::policy::listed(self.db, self.project, action));
        if !immutable && !fixed {
            return false;
        }
        let problem = Diagnostic::new(
            &rule::TYPE_FIELD_UPDATE_FORBIDDEN,
            format!("`{written}` changes `{entity}.{field}`, which an update can't change"),
        )
        .at(
            span,
            if immutable {
                "immutable"
            } else {
                "named in policy"
            },
        );
        self.out.push(if immutable {
            problem
                .reason(format!(
                    "`{entity}.{field}` says `immutable: true`: it keeps the value its record was created with."
                ))
                .next(format!("Remove `{field}` from the update, or `immutable: true` from the field"))
        } else {
            problem
                .reason(format!(
                    "`{entity}`'s policy reaches the caller or the tenant through `{field}`, so changing it hands the record to someone else. That is a named action, listed with who may call it under `actions:` in `policy.jadpo`."
                ))
                .next(match &self.action {
                    Some(action) => format!(
                        "Remove `{field}` from the update, or list `{action}` under `actions:` in `policy.jadpo`"
                    ),
                    None => format!("Remove `{field}` from the update"),
                })
        });
        true
    }

    /// A write giving a field only the lifecycle's transitions change
    /// (area 5, "Lifecycle").
    fn owned_field(
        &mut self,
        entity: &str,
        field: &str,
        span: Span,
        written: &str,
        rules: &WriteRules,
    ) {
        let by: Vec<&TransitionRule> = rules
            .transitions
            .iter()
            .filter(|t| t.sets.iter().any(|set| set == field))
            .collect();
        let listed: Vec<String> = by.iter().map(|t| format!("`{}`", t.name)).collect();
        let changed_by = match listed.as_slice() {
            [] => format!("`{field}` keeps the value `initial:` gives it"),
            [one] => format!("only its transition {one} changes it"),
            _ => format!(
                "only its transitions {} change it",
                crate::and_list(&listed)
            ),
        };
        let start = if written.ends_with(".create") {
            " A new record gets it from `initial:`, or `none` when the lifecycle gives none."
        } else {
            ""
        };
        let snake = crate::case::to_snake(entity);
        self.out.push(
            Diagnostic::new(
                &rule::DATA_LIFECYCLE_OWNED_FIELD,
                format!("`{written}` sets `{field}`, which only `{entity}`'s lifecycle changes"),
            )
            .at(span, "owned by the lifecycle")
            .reason(format!(
                "`{entity}`'s lifecycle owns `{field}`: {changed_by}, so the record's stages always move as the lifecycle says.{start}"
            ))
            .next(match by.first() {
                Some(t) if !written.ends_with(".create") => format!(
                    "Remove `{field}`, and call `{entity}.{}({snake})` to change it",
                    t.name
                ),
                _ => format!("Remove `{field}` here"),
            }),
        );
    }

    /// A delete that would remove a record whose lifecycle keeps it: the
    /// entity has a lifecycle with no `delete` transition, or removing the
    /// record cascades to one that has a lifecycle (area 5: code cannot
    /// hard-delete a record that has a lifecycle).
    fn hard_delete(&mut self, entity: &str, rules: &WriteRules, span: Span) {
        let written = format!("{entity}.delete");
        let (message, reason, next) = if rules.lifecycle {
            (
                format!("`{written}` would remove a record `{entity}`'s lifecycle keeps"),
                format!(
                    "Code never removes a record that has a lifecycle: `delete` runs its `delete` transition, and `{entity}`'s lifecycle has none. Only purge, the compiler's own, removes such records, once they have been hidden long enough."
                ),
                format!(
                    "Add a `delete` transition to `{entity}`'s lifecycle, saying what deleting one changes"
                ),
            )
        } else if let Some((holder, field)) = &rules.cascades_to_lifecycle {
            (
                format!("`{written}` would remove `{holder}` records their lifecycle keeps"),
                format!(
                    "Removing a `{entity}` removes the `{holder}` records whose `{field}` points at it (`on_delete: OnDelete.cascade`), and `{holder}` has a lifecycle; code never removes a record that has one."
                ),
                format!(
                    "Give `{entity}` a lifecycle with a `delete` transition, or change `{holder}.{field}`'s `on_delete:`"
                ),
            )
        } else {
            self.audited_delete(entity, rules, span);
            return;
        };
        self.out.push(
            Diagnostic::new(&rule::DATA_LIFECYCLE_HARD_DELETE, message)
                .at(span, "a lifecycle keeps it")
                .reason(reason)
                .next(next),
        );
    }

    /// A delete the audit trail can't record yet (area 16, "Audit trail";
    /// chunk B11): one whose cascade removes or changes an audited record,
    /// which the database does with no entry, and the removal of an
    /// audited record of a person's, whose history would outlive it
    /// beyond the reach of the person's erasure.
    fn audited_delete(&mut self, entity: &str, rules: &WriteRules, span: Span) {
        let written = format!("{entity}.delete");
        let (reason, next) = if let Some((holder, field, cascades)) = &rules.reaches_audited {
            let what = if *cascades {
                format!(
                    "Removing a `{entity}` removes the `{holder}` records whose `{field}` points at it (`on_delete: OnDelete.cascade`)"
                )
            } else {
                format!(
                    "Removing a `{entity}` clears `{field}` on the `{holder}` records that point at it (`on_delete: OnDelete.cleared`)"
                )
            };
            (
                format!(
                    "{what}, and `{holder}` keeps a history of its changes (`audit:`), which doesn't yet record a change the database's cascade makes: accepted, the change would have no entry."
                ),
                format!(
                    "Make `{holder}.{field}` `OnDelete.restricted` and change those records first in the same action, or leave `audit:` out of `{holder}` for now"
                ),
            )
        } else if rules.audited && rules.person_owned {
            (
                format!(
                    "A `{entity}` belongs to a person, and its history (`audit:`) would be kept after the record is removed, where erasing the person, which deletes a record's history with the record, can't reach it: accepted, data of theirs would outlive their erasure."
                ),
                format!(
                    "Give `{entity}` a lifecycle with a `delete` transition, so a deleted record is kept hidden until its person is erased, or leave `audit:` out of `{entity}` for now"
                ),
            )
        } else {
            return;
        };
        self.out.push(
            Diagnostic::new(
                &rule::TOOLCHAIN_NOT_BUILT_YET,
                format!("`{written}` isn't built yet here"),
            )
            .at(span, "decided, not built")
            .reason(reason)
            .next(next),
        );
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
        rules: &WriteRules,
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
                if rules.owned.contains(&field.name) {
                    self.owned_field(entity, &field.name, span.clone(), written, rules);
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
            if self.fixed_field(entity, &field.name, span.clone(), written, rules) {
                continue;
            }
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
    /// patch's request supplied `field` (area 6, "Empty patch"). Each field
    /// it sets, with its value.
    fn when_supplied(
        &mut self,
        entity: &str,
        by_id: bool,
        supplied: &ast::Member,
        input: Option<(&str, &[String])>,
        settable: &[FieldDef],
        rules: &WriteRules,
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
                if self.fixed_field(entity, &field, span.clone(), &written, rules) {
                    continue;
                }
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
            let mut skip: Vec<String> = generated.iter().map(|f| f.name.clone()).collect();
            for (field_name, _) in tree::members(member.block()) {
                if let Some(field) = tree::name_text(&field_name)
                    && rules.owned.contains(&field)
                {
                    let span = self.span(&field_name);
                    self.owned_field(entity, &field, span, &written, rules);
                    skip.push(field);
                }
            }
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
                for (name, field_member) in tree::members(member.block()) {
                    if let Some(value) = field_member.value() {
                        self.one_step(entity, &value, &written);
                        if let Some(field) = tree::name_text(&name) {
                            self.stored_scale(entity, &field, &value, &written);
                        }
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
        // A caller's `id` (area 8): authentication read its record for
        // this request, as a record passed in was read before it.
        if let Some(kind) = [
            jadpo_std::http::PrincipalKind::User,
            jadpo_std::http::PrincipalKind::Service,
        ]
        .into_iter()
        .find(|kind| kind.type_name() == holder)
        {
            return name == "id"
                && self
                    .identity()
                    .variant(kind)
                    .is_some_and(|variant| variant.entity == target);
        }
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
                // A value stored whole as JSON is written through the
                // route codec, which the database doesn't run (chunk
                // 5.9b).
                let json = self.facts.types.get(&node_key(&node)).is_some_and(|ty| {
                    crate::data::column_type(self.table, ty).as_deref() == Ok("jsonb")
                });
                if json {
                    let span = crate::tree::node_span(self.path, &node);
                    let shown = crate::tree::written(&node);
                    self.out.push(
                        Diagnostic::new(
                            &rule::TOOLCHAIN_NOT_BUILT_YET,
                            format!("`{written}` by `id` working out `{shown}` isn't built yet"),
                        )
                        .at(span, "stored as JSON")
                        .reason(format!(
                            "`{shown}` is stored whole as its JSON in one `jsonb` column (area 5), which Jadpo writes through its type's codec and the database doesn't work out in one step yet."
                        ))
                        .next(format!(
                            "Load the record and update it, `{written}({}) {{ set: {{ ... }} }}`, for now",
                            crate::case::to_snake(entity)
                        )),
                    );
                    return;
                }
                // An amount of money is stored in two columns, which the
                // database doesn't work out in one step yet (area 19).
                let money = self
                    .facts
                    .types
                    .get(&node_key(&node))
                    .is_some_and(|ty| self.table.representation(ty) == Some(Prim::Money));
                if money {
                    let span = crate::tree::node_span(self.path, &node);
                    let shown = crate::tree::written(&node);
                    self.out.push(
                        Diagnostic::new(
                            &rule::TOOLCHAIN_NOT_BUILT_YET,
                            format!("`{written}` by `id` working out `{shown}` isn't built yet"),
                        )
                        .at(span, "an amount of money, stored")
                        .reason(format!(
                            "`{shown}` is an amount of money, stored as its minor units and its currency in two columns (area 19), which this compiler doesn't have the database work out in one step yet."
                        ))
                        .next(format!(
                            "Load the record and update it, `{written}({}) {{ set: {{ ... }} }}`, for now",
                            crate::case::to_snake(entity)
                        )),
                    );
                    return;
                }
                self.stored_operand(&node, &root, entity, written);
            }
        }
    }

    /// A decimal the database adds to, or subtracts from, a stored field
    /// keeps no more places than the field's `scale` (area 6, "Counters"):
    /// PostgreSQL rounds the sum half away from zero, Jadpo half to even,
    /// so a value with more places could be stored differently from how
    /// Jadpo would keep it.
    fn stored_scale(&mut self, entity: &str, field: &str, value: &Expr, written: &str) {
        let target = Ty::Field(entity.to_string(), vec![field.to_string()]);
        if self.table.representation(&target) != Some(Prim::Decimal) {
            return;
        }
        let Some(scale) = self
            .table
            .rule_chain(&target)
            .into_iter()
            .find_map(|(_, rules)| rules.scale)
        else {
            return;
        };
        let root = value.syntax().clone();
        let holds_stored = |node: &SyntaxNode| {
            std::iter::once(node.clone())
                .chain(node.descendants())
                .any(|n| self.facts.stored.contains(&node_key(&n)))
        };
        let mut found: Option<(Expr, Option<u32>)> = None;
        for node in std::iter::once(root.clone()).chain(root.descendants()) {
            let Some(binary) = ast::BinExpr::cast(node.clone()) else {
                continue;
            };
            if !binary.op().is_some_and(|op| matches!(op.text(), "+" | "-")) {
                continue;
            }
            let (Some(lhs), Some(rhs)) = (binary.lhs(), binary.rhs()) else {
                continue;
            };
            if !holds_stored(lhs.syntax()) && !holds_stored(rhs.syntax()) {
                continue;
            }
            for operand in [lhs, rhs] {
                if holds_stored(operand.syntax()) {
                    continue;
                }
                let places = self.decimal_places(&operand);
                if places.is_none_or(|places| places > scale) {
                    found = Some((operand, places));
                    break;
                }
            }
            if found.is_some() {
                break;
            }
        }
        let Some((operand, places)) = found else {
            return;
        };
        let shown = tree::written(operand.syntax());
        let span = self.span(&operand);
        let (label, how) = match places {
            Some(places) => (
                format!("{places} decimal places"),
                format!("`{shown}` has {places}"),
            ),
            None => (
                "any number of places".to_string(),
                format!("`{shown}` is a decimal whose type sets no `scale`"),
            ),
        };
        self.out.push(
            Diagnostic::new(
                &rule::DATA_STORED_DECIMAL_SCALE,
                format!("`{written}` adds a decimal with more places than `{entity}.{field}` keeps"),
            )
            .at(span, label)
            .reason(format!(
                "The database works `{entity}.{field}` out in one step, and keeps it at `scale: {scale}`; {how}, and PostgreSQL would round the result half away from zero where Jadpo rounds half to even, so the stored value could differ from Jadpo's."
            ))
            .next(format!(
                "Give the value at most {scale} decimal places, or a type with `scale: {scale}` or fewer"
            )),
        );
    }

    /// How many decimal places a value can have: a literal's own, a whole
    /// number's none, or its type's `scale`; `None` when nothing bounds it.
    fn decimal_places(&self, expr: &Expr) -> Option<u32> {
        let inner = match expr {
            Expr::ParenExpr(paren) => paren.expr()?,
            other => other.clone(),
        };
        if let crate::types::Const::Number { value, .. } = crate::types::Const::from_expr(&inner) {
            return Some(value.scale);
        }
        let ty = self.facts.types.get(&node_key(inner.syntax()))?;
        match self.table.representation(ty) {
            Some(Prim::Int) => Some(0),
            Some(Prim::Decimal) => self
                .table
                .rule_chain(ty)
                .into_iter()
                .find_map(|(_, rules)| rules.scale),
            _ => None,
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
        if self
            .table
            .field(head, std::slice::from_ref(&field))
            .is_none()
        {
            // A field this migration retires, which its entity's backfill
            // still reads (area 5, "Computed backfills").
            if self.stored.as_deref() == Some(head)
                && let Some(ty) = self.retired_field(&field)
            {
                self.facts.stored.insert(node_key(expr.syntax()));
                return Some(ty);
            }
            return None;
        }
        // No SQL reads a sealed field, so the database can't work a value
        // out from one (area 15; chunk P9).
        if crate::data::sealed::is_sealed(self.db, self.project, head, &field) {
            let span = self.span(expr);
            self.out.push(crate::data::sealed::used(
                head,
                &field,
                "A value the database works out",
                span,
                format!("Load the record, and read `{field}` from it in the action"),
            ));
            return Some(Ty::Error);
        }
        if self.stored.as_deref() == Some(head) {
            // A field this migration adds has no stored value yet (chunk
            // 5.8b).
            if self.context == super::Context::Backfill && self.added.contains(&field) {
                let span = self.span(expr);
                self.backfill_reads_added(span, &format!("{head}.{field}"));
                return Some(Ty::Error);
            }
            self.facts.stored.insert(node_key(expr.syntax()));
            return Some(self.table.field_type(head, &[field]));
        }
        let span = self.span(expr);
        if self.context == super::Context::Backfill {
            self.backfill_outside_row(span, &format!("{head}.{field}"), "another entity's field");
            return Some(Ty::Error);
        }
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
    /// once to a `conflict` failure, with no catch-all (area 6, "Write failures"). The arms that map a rule.
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
        let mut unbreakable: Vec<(Arm, Breach)> = Vec::new();
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
                    [holder, name] => self.breach(holder, name),
                    _ => None,
                };
                let Some(rule) = rule else {
                    self.conflict_unknown(&arm, entity, written, breakable, rules);
                    unknown = true;
                    continue;
                };
                if let Some(earlier) = first.get(&rule) {
                    self.duplicate_mapping(&arm, earlier.clone());
                    continue;
                }
                first.insert(rule.clone(), arm.key_span.clone());
                if !breakable.contains(&rule) {
                    unbreakable.push((arm, rule));
                    continue;
                }
                if let Some(failure) = failure {
                    self.mapping_kind(&arm, &failure, "conflict");
                    out.push((member, rule, failure));
                }
            }
        }
        let unmapped: Vec<String> = breakable
            .iter()
            .filter(|rule| !first.contains_key(rule))
            .map(Breach::written)
            .collect();
        // An arm for a rule this write can't break was most likely meant
        // for one it can: it says so, and the rule isn't reported again.
        let rejected = unknown || !unbreakable.is_empty();
        for (arm, rule) in &unbreakable {
            self.conflict_cannot_break(arm, rule, entity, op, written, unmapped.first());
        }
        let unmapped: Vec<String> = if rejected { Vec::new() } else { unmapped };
        if let Some(first_rule) = unmapped.first() {
            let listed: Vec<String> = unmapped.iter().map(|r| format!("`{r}`")).collect();
            let (them, each) = if unmapped.len() == 1 {
                ("it", "it needs")
            } else {
                ("them", "each needs")
            };
            let transition = breakable.iter().find(|rule| {
                matches!(rule, Breach::Transition { .. }) && !first.contains_key(rule)
            });
            let why = match (transition, op) {
                (Some(rule), _) => format!(
                    "`{}` is refused when its `when:` doesn't hold for the record, which is still there and visible",
                    rule.written()
                ),
                (None, WriteOp::Delete) => "A delete is refused while a `restricted` reference points at the record, or at one its delete cascades to".to_string(),
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
    /// `holder`, or one of its references.
    fn breach(&self, holder: &str, name: &str) -> Option<Breach> {
        let rules = self.writes.write_rules(holder)?;
        if let Some(rule) = rules.unique(name) {
            return Some(Breach::Unique {
                entity: holder.to_string(),
                name: name.to_string(),
                field: rule.field,
            });
        }
        if rules.transition(name).is_some() {
            return Some(Breach::Transition {
                entity: holder.to_string(),
                name: name.to_string(),
            });
        }
        rules.reference(name).map(|_| Breach::Reference {
            entity: holder.to_string(),
            field: name.to_string(),
        })
    }

    fn conflict_unknown(
        &mut self,
        arm: &Arm,
        entity: &str,
        written: &str,
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
            format!("`{written}` can break no rule.")
        } else {
            let listed: Vec<String> = breakable
                .iter()
                .map(|b| format!("`{}`", b.written()))
                .collect();
            format!("`{written}` can break {}.", crate::and_list(&listed))
        };
        let problem = Diagnostic::new(
            &rule::DATA_CONFLICT_UNKNOWN,
            format!("`{}` names no unique rule or reference", arm.key),
        )
        .at(arm.key_span.clone(), "not a rule")
        .reason(format!(
            "A `conflict:` arm is keyed by the rule the store refuses, written `Entity.name`: a field with `unique: true`, a rule named in `unique:`, a `restricted` reference that refuses a delete, or a lifecycle transition whose `when:` can refuse it. {rules_line}{}",
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
        meant: Option<&String>,
    ) {
        let reason = match (rule, op) {
            (
                Breach::Transition {
                    entity: holder,
                    name,
                },
                _,
            ) => {
                let guarded = self
                    .writes
                    .write_rules(holder)
                    .and_then(|rules| rules.transition(name))
                    .is_some_and(|t| t.guarded);
                if holder == entity && written == format!("{holder}.{name}") && !guarded {
                    format!(
                        "`{holder}.{name}`'s `when:` asks only what `visible:` says already: a record it doesn't hold for is hidden, which is `{holder}NotFound`, never a conflict."
                    )
                } else {
                    format!(
                        "Only `{holder}.{name}` itself is refused by that transition's `when:`; `{written}` doesn't run it."
                    )
                }
            }
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
        let removal = crate::tree::removal(self.path, &arm.member);
        let problem = Diagnostic::new(
            &rule::DATA_CONFLICT_CANNOT_BREAK,
            format!(
                "`{written}` can't break `{}`, so it takes no arm",
                rule.written()
            ),
        )
        .at(arm.key_span.clone(), "never runs");
        self.out.push(match meant {
            Some(meant) => problem
                .reason(format!(
                    "{reason} It can break `{meant}`, which no arm maps."
                ))
                .next(format!("Use `{meant}` instead of `{}`", arm.key))
                .edit(arm.key_span.clone(), meant.clone()),
            None => problem
                .reason(reason)
                .next(format!("Remove `{}: ...`", arm.key))
                .edit(removal, ""),
        });
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
    transition: Option<&str>,
) -> Vec<Breach> {
    if let Some(name) = transition {
        // A transition is an update of the fields it sets, refused when
        // its `when:` doesn't hold; even `delete` keeps the row.
        let mut out = breakable(entity, WriteOp::Update, rules, sets, None);
        if rules.transition(name).is_some_and(|rule| rule.guarded) {
            out.push(Breach::Transition {
                entity: entity.to_string(),
                name: name.to_string(),
            });
        }
        return out;
    }
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
