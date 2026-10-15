//! An entity's `lifecycle:` (area 5, "Lifecycle"): which records normal
//! reads see (`visible:`), the starting values the compiler owns
//! (`initial:`), the named stage changes that are the only way to change
//! the fields it owns (`transitions:`), and the permanent deletion of
//! hidden records once they have been hidden long enough (`purge:`).
//!
//! `visible:` and each transition's `when:` are conditions on the record's
//! own fields, named bare as area 5 writes them (`deleted_at == none`):
//! comparisons, `and`, `or`, `none` checks and `clock.now`, never another
//! record or who is asking, so each becomes part of every statement's
//! `WHERE` and, without `clock.now`, a partial index. A transition's
//! `set:` and `initial:` give fixed values: literals, enum values, `none`
//! and `clock.now`.
//!
//! Read once per entity by [`read`], into the [`LifecycleModel`] the
//! write checks, the derived indexes and `jadpo-plan` all read.

use std::collections::BTreeSet;

use jadpo_diag::{Diagnostic, Span, nearest, rule};
use jadpo_syntax::ast::{self, AstNode, Expr, MemberItem};

use super::queries::{Compare, Condition, Value, comparable, ordered};
use super::{EntityModel, EntitySpans, FieldModel};
use crate::case::{is_lower_snake, starts_upper};
use crate::tree::{self, dotted, node_span};
use crate::types::{Const, Def, Prim, Ty, TypeView};

/// The sections a lifecycle holds, in the order a reader meets them.
pub const SECTIONS: [&str; 4] = ["visible", "initial", "transitions", "purge"];

/// The operations every entity has, which no transition may be named
/// after; `delete` is the one a transition may take over (area 6).
const OPERATIONS: [&str; 3] = ["get", "create", "update"];

/// An entity's lifecycle, as the checks, the indexes and the plans read it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LifecycleModel {
    /// The records normal reads see; every other record is hidden.
    pub visible: Option<Condition>,
    /// The starting value of each field it owns that has one, as written:
    /// `none` is `Value::Literal(Const::None)`.
    pub initial: Vec<(String, Value)>,
    pub transitions: Vec<TransitionModel>,
    pub purge: Option<PurgeModel>,
    /// The fields only its transitions change: every field a transition
    /// sets or `initial:` gives, in the entity's order.
    pub owned: Vec<String>,
}

/// One named stage change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransitionModel {
    pub name: String,
    /// When it is allowed.
    pub when: Option<Condition>,
    /// Whether `when:` can refuse a visible record: false when it is only
    /// what `visible:` says already, since a hidden record is not found.
    pub guarded: bool,
    /// Each field it sets, with its fixed value, as written.
    pub set: Vec<(String, Value)>,
}

/// Permanent deletion of hidden records (area 5).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PurgeModel {
    /// How long a record stays hidden before it goes, in milliseconds.
    pub after: i128,
    /// As written: `30d`.
    pub after_written: String,
    /// The field it counts from.
    pub since: String,
}

impl LifecycleModel {
    pub fn transition(&self, name: &str) -> Option<&TransitionModel> {
        self.transitions.iter().find(|t| t.name == name)
    }

    /// Whether only its transitions change `field`.
    pub fn owns(&self, field: &str) -> bool {
        self.owned.iter().any(|owned| owned == field)
    }
}

/// Whether a condition reads `clock.now`, so it changes with time and can't
/// be a partial index's predicate.
pub fn reads_now(condition: &Condition) -> bool {
    match condition {
        Condition::Compare { value, .. } | Condition::In { value, .. } => {
            matches!(value, Value::Now)
        }
        Condition::IsNone { .. } => false,
        Condition::And(all) | Condition::Or(all) => all.iter().any(reads_now),
        Condition::Not(inner) => reads_now(inner),
        Condition::Present { condition, .. } => reads_now(condition),
    }
}

/// The parts every one of which must hold: an `and`'s parts, or the
/// condition itself.
fn conjuncts(condition: &Condition) -> Vec<&Condition> {
    match condition {
        Condition::And(all) => all.iter().flat_map(conjuncts).collect(),
        other => vec![other],
    }
}

/// Reads `lifecycle: { ... }` on `entity`, whose fields are read already.
pub(super) fn read(
    lookup: &dyn TypeView,
    path: &str,
    entity: &EntityModel,
    spans: &EntitySpans,
    member: &ast::Member,
    out: &mut Vec<Diagnostic>,
) -> Option<LifecycleModel> {
    let key_span = member.member_key().map_or_else(
        || node_span(path, member.syntax()),
        |k| node_span(path, k.syntax()),
    );
    let mut reader = Reader {
        lookup,
        path,
        entity,
        out,
    };
    let sections = reader.sections(
        member.block(),
        &SECTIONS,
        "A lifecycle holds `visible:`, `initial:`, `transitions:` and `purge:`.",
        "lifecycle",
    );
    let visible = sections
        .iter()
        .find(|(name, _)| name == "visible")
        .and_then(|(_, member)| reader.condition_of(member, "visible"));
    let initial = sections
        .iter()
        .find(|(name, _)| name == "initial")
        .map(|(_, member)| reader.values(member, "initial"))
        .unwrap_or_default();
    // Every field `initial:` gives, its value checked or not: one whose
    // value is wrong has had its error.
    let given: Vec<String> = sections
        .iter()
        .find(|(name, _)| name == "initial")
        .into_iter()
        .flat_map(|(_, member)| tree::members(member.block()))
        .filter_map(|(name, _)| tree::name_text(&name))
        .collect();
    // A `visible:` already reported leaves what it hides unknown, so
    // nothing reports a knock-on of it.
    let has_visible = sections.iter().any(|(name, _)| name == "visible");
    let unknown = has_visible && visible.is_none();
    let mut transitions = Vec::new();
    // Where each transition is written.
    let mut written: Vec<Span> = Vec::new();
    if let Some((_, member)) = sections.iter().find(|(name, _)| name == "transitions") {
        (transitions, written) = reader.transitions(member, visible.as_ref(), unknown);
    }
    let purge = sections
        .iter()
        .find(|(name, _)| name == "purge")
        .and_then(|(_, member)| reader.purge(member, has_visible))
        .filter(|_| visible.is_some());
    let has_transitions = sections.iter().any(|(name, _)| name == "transitions");
    if !has_visible && !has_transitions {
        reader.out.push(
            Diagnostic::new(
                &rule::DATA_LIFECYCLE_SECTION,
                format!("`{}`'s lifecycle hides nothing and changes nothing", entity.name),
            )
            .at(key_span.clone(), "no `visible:` or `transitions:`")
            .reason("A lifecycle says which records reads see (`visible:`) or the named changes its fields go through (`transitions:`); one with neither would only stop the records being deleted.")
            .next("Add `visible:` or `transitions:`, or remove `lifecycle:`"),
        );
    }
    // The fields it owns: every field a transition sets or `initial:` gives.
    let mut owned: BTreeSet<&str> = BTreeSet::new();
    for (field, _) in &initial {
        owned.insert(field);
    }
    for transition in &transitions {
        for (field, _) in &transition.set {
            owned.insert(field);
        }
    }
    let owned: Vec<String> = entity
        .fields
        .iter()
        .filter(|field| owned.contains(field.name.as_str()))
        .map(|field| field.name.clone())
        .collect();
    // A required field a transition sets needs a starting value: a create
    // can't give it, since only the lifecycle changes it.
    for name in &owned {
        let Some(index) = entity.fields.iter().position(|f| f.name == *name) else {
            continue;
        };
        let (field, field_span) = (&entity.fields[index], &spans.fields[index]);
        if field.nullable || given.contains(name) {
            continue;
        }
        let at = transitions
            .iter()
            .position(|t| t.set.iter().any(|(set, _)| set == name))
            .map_or_else(|| field_span.clone(), |at| written[at].clone());
        reader.out.push(
            Diagnostic::new(
                &rule::DATA_LIFECYCLE_INITIAL_REQUIRED,
                format!("`{}.{name}` has no starting value", entity.name),
            )
            .at(field_span.clone(), "required, and only transitions set it")
            .also(at, "set here")
            .reason(format!(
                "Only the lifecycle's transitions change `{name}`, so a create can't give it; a required field gets its first value from `initial:`, and an optional one starts as `none`."
            ))
            .next(format!("Add `{name}: <value>` to the lifecycle's `initial:`")),
        );
    }
    Some(LifecycleModel {
        visible,
        initial,
        transitions,
        purge,
        owned,
    })
}

struct Reader<'a, 'o> {
    lookup: &'a dyn TypeView,
    path: &'a str,
    entity: &'a EntityModel,
    out: &'o mut Vec<Diagnostic>,
}

impl<'a> Reader<'a, '_> {
    fn span(&self, node: &impl AstNode) -> Span {
        node_span(self.path, node.syntax())
    }

    /// The sections of a block, each once and only those in `known`, by
    /// name in written order.
    fn sections(
        &mut self,
        list: Option<ast::MemberList>,
        known: &[&str],
        takes: &str,
        what: &str,
    ) -> Vec<(String, ast::Member)> {
        let mut found: Vec<(String, ast::Member)> = Vec::new();
        for item in list.iter().flat_map(|list| list.member_items()) {
            let (word, node) = match &item {
                MemberItem::Member(member) => match member.member_key() {
                    Some(key) => (tree::written(key.syntax()), key.syntax().clone()),
                    None => continue,
                },
                MemberItem::Elision(_) => continue,
                other => (tree::written(other.syntax()), other.syntax().clone()),
            };
            let span = node_span(self.path, &node);
            if let Some((_, first)) = found.iter().find(|(name, _)| *name == word) {
                let first = first
                    .member_key()
                    .map_or_else(|| self.span(first), |k| self.span(&k));
                self.out.push(
                    Diagnostic::new(
                        &rule::DATA_LIFECYCLE_SECTION,
                        format!("`{word}:` is given twice"),
                    )
                    .at(span, "given again")
                    .also(first, "first given here")
                    .reason(format!(
                        "Each part of a {what} is given once, so there is never a question of which counts."
                    ))
                    .next(format!("Remove one `{word}:`")),
                );
                continue;
            }
            let MemberItem::Member(member) = item else {
                self.unknown_section(span, &word, known, takes, what);
                continue;
            };
            if !known.contains(&word.as_str()) {
                self.unknown_section(span, &word, known, takes, what);
                continue;
            }
            found.push((word, member));
        }
        found
    }

    fn unknown_section(&mut self, span: Span, word: &str, known: &[&str], takes: &str, what: &str) {
        let near = nearest(word, known.iter().copied());
        let problem = Diagnostic::new(
            &rule::DATA_LIFECYCLE_SECTION,
            format!("`{word}` isn't part of a {what}"),
        )
        .at(span.clone(), "not a section here")
        .reason(match near {
            Some(near) => format!("{takes} Did you mean `{near}`?"),
            None => takes.to_string(),
        });
        self.out.push(match near {
            Some(near) => problem.next(format!("Use `{near}`")).edit(span, near),
            None => problem.next(format!("Remove `{word}` from the {what}")),
        });
    }

    /// A section that is required and missing.
    fn missing_section(&mut self, at: Span, word: &str, takes: &str, what: &str) {
        self.out.push(
            Diagnostic::new(
                &rule::DATA_LIFECYCLE_SECTION,
                format!("The {what} has no `{word}:`"),
            )
            .at(at, format!("no `{word}:`"))
            .reason(takes.to_string())
            .next(format!("Add `{word}: ...`")),
        );
    }

    // --- conditions ------------------------------------------------------------

    /// `visible:` or `when:`: one condition.
    fn condition_of(&mut self, member: &ast::Member, word: &str) -> Option<Condition> {
        let Some(value) = member.value() else {
            let at = member
                .member_key()
                .map_or_else(|| self.span(member), |k| self.span(&k));
            self.out.push(
                Diagnostic::new(
                    &rule::DATA_LIFECYCLE_CONDITION,
                    format!("`{word}:` holds no condition"),
                )
                .at(at, "a condition goes here")
                .reason(format!(
                    "`{word}:` is one condition on the record's own fields, such as `<field> == none`."
                ))
                .next(format!("Write `{word}: <field> == <value>`")),
            );
            return None;
        };
        self.condition(&value)
    }

    fn condition(&mut self, expr: &Expr) -> Option<Condition> {
        let expr = unparen(expr);
        match &expr {
            Expr::BinExpr(binary) => {
                let op = binary.op()?.text().to_string();
                let lhs = binary.lhs()?;
                let rhs = binary.rhs()?;
                match op.as_str() {
                    "and" | "or" => {
                        let left = self.condition(&lhs);
                        let right = self.condition(&rhs);
                        let (left, right) = (left?, right?);
                        let mut parts = Vec::new();
                        for part in [left, right] {
                            match (op.as_str(), part) {
                                ("and", Condition::And(inner)) | ("or", Condition::Or(inner)) => {
                                    parts.extend(inner)
                                }
                                (_, other) => parts.push(other),
                            }
                        }
                        Some(if op == "and" {
                            Condition::And(parts)
                        } else {
                            Condition::Or(parts)
                        })
                    }
                    token => {
                        let Some(compare) = compare_of(token) else {
                            self.unsupported(
                                &expr,
                                format!("`{token}` isn't one of a lifecycle condition's operators: it compares a field with `==`, `!=`, `<`, `<=`, `>` or `>=`, and joins comparisons with `and` and `or`."),
                                "Compare a field with `==`, `!=`, `<`, `<=`, `>` or `>=`".to_string(),
                            );
                            return None;
                        };
                        let field = self.condition_field(&lhs, &expr)?;
                        let field_ty = field_ty(self.entity, field);
                        if matches!(Const::from_expr(&rhs), Const::None)
                            && matches!(compare, Compare::Equal | Compare::NotEqual)
                        {
                            if !field.nullable {
                                self.out.push(
                                    Diagnostic::new(
                                        &rule::TYPE_INCOMPARABLE,
                                        format!("`{}` is never `none`", field.name),
                                    )
                                    .at(self.span(&expr), format!("`{field_ty}` compared with `none`"))
                                    .reason(format!(
                                        "Only a nullable field, declared `T?`, can be `none`; `{}.{}` is `{field_ty}`.",
                                        self.entity.name, field.name
                                    ))
                                    .next("Remove the condition, since it is always the same"),
                                );
                                return None;
                            }
                            return Some(Condition::IsNone {
                                field: field.name.clone(),
                                negated: compare == Compare::NotEqual,
                            });
                        }
                        if compare.is_ordering() && !ordered(self.lookup, &field_ty) {
                            self.unsupported(
                                &expr,
                                format!("`{token}` orders numbers, times, dates and durations, as it does in a body, and `{}` is `{}`.", field.name, field.ty.strip_nullable()),
                                format!("Compare `{}` with `==` or `!=`", field.name),
                            );
                            return None;
                        }
                        let value = self.value(&rhs, field, false)?;
                        Some(Condition::Compare {
                            field: field.name.clone(),
                            op: compare,
                            value,
                        })
                    }
                }
            }
            Expr::PrefixExpr(prefix) if prefix.op().is_some_and(|op| op.text() == "not") => {
                self.unsupported(
                    &expr,
                    "A lifecycle condition compares the record's fields with `==`, `!=`, `<`, `<=`, `>` and `>=`, `none` checks and `clock.now`, joined with `and` and `or` (area 5); `not` is written as the opposite comparison.".to_string(),
                    "Write the opposite comparison instead of `not`".to_string(),
                );
                None
            }
            _ => {
                let (reason, next) = match self.bare_field(&expr) {
                    Some(field) => (
                        format!(
                            "A condition compares a field with a value; `{}` alone says nothing to compare it with.",
                            field.name
                        ),
                        format!("Write `{} == true`", field.name),
                    ),
                    None => (
                        "A lifecycle condition compares one of the record's own fields with a value.".to_string(),
                        format!("Compare a field of `{}` with a value", self.entity.name),
                    ),
                };
                self.unsupported(&expr, reason, next);
                None
            }
        }
    }

    /// The field a comparison tests, named bare on its left.
    fn condition_field(&mut self, lhs: &Expr, whole: &Expr) -> Option<&'a FieldModel> {
        let lhs = unparen(lhs);
        let entity = self.entity;
        if let Expr::BinExpr(binary) = &lhs
            && binary.op().is_some_and(|op| op.text() == "in")
        {
            self.unsupported(
                whole,
                "A lifecycle condition compares the record's fields with `==`, `!=`, `<`, `<=`, `>` and `>=`, `none` checks and `clock.now`, joined with `and` and `or` (area 5).".to_string(),
                "Write the test as comparisons joined with `or`".to_string(),
            );
            return None;
        }
        match dotted(&lhs) {
            Some((first, rest)) if rest.is_empty() => {
                let name = tree::name_ref_text(&first)?;
                self.field_named(&name, &self.span(&first))
            }
            Some((first, rest))
                if rest.len() == 1
                    && tree::name_ref_text(&first).as_deref() == Some(entity.name.as_str()) =>
            {
                // `Todo.deleted_at`, as a query would write it.
                let field = tree::name_ref_text(&rest[0])?;
                let span = self.span(&lhs);
                let found = entity.fields.iter().find(|f| f.name == field);
                let problem = Diagnostic::new(
                    &rule::DATA_LIFECYCLE_CONDITION,
                    format!("`{}.{field}` names its own entity", entity.name),
                )
                .at(span.clone(), "written as a query writes it")
                .reason(format!(
                    "A lifecycle's conditions are about the record they are written in, so they name its fields bare: `{field}`, not `{}.{field}`.",
                    entity.name
                ));
                self.out.push(match found {
                    Some(_) => problem.next(format!("Write `{field}`")).edit(span, field),
                    None => problem.next(format!("Name a field of `{}`", entity.name)),
                });
                None
            }
            Some((first, rest)) if !rest.is_empty() => {
                let head = tree::name_ref_text(&first).unwrap_or_default();
                let reason = if starts_upper(&head) {
                    format!(
                        "A lifecycle condition can't mention other records: it tests `{}`'s own fields, so it can become part of every statement's `WHERE` and an index. Who may see another record is policy (area 9).",
                        entity.name
                    )
                } else {
                    format!(
                        "A lifecycle condition tests `{}`'s own fields, never who is asking or anything else: that is policy (area 9).",
                        entity.name
                    )
                };
                self.unsupported(
                    whole,
                    reason,
                    format!("Compare a field of `{}`", entity.name),
                );
                None
            }
            _ => {
                let reason = if self.bare_field(whole).is_some() || self.right_is_field(whole) {
                    "Write the field on the left and the value on the right: `<field> > value`."
                        .to_string()
                } else {
                    format!(
                        "A lifecycle condition compares one of `{}`'s own fields, on the left, with a value.",
                        entity.name
                    )
                };
                self.unsupported(whole, reason, "Write the field on the left".to_string());
                None
            }
        }
    }

    fn right_is_field(&self, expr: &Expr) -> bool {
        match unparen(expr) {
            Expr::BinExpr(binary) => binary
                .rhs()
                .is_some_and(|rhs| self.bare_field(&rhs).is_some()),
            _ => false,
        }
    }

    /// `expr` as one of the entity's fields named bare, without reporting.
    fn bare_field(&self, expr: &Expr) -> Option<&'a FieldModel> {
        let Expr::NameRef(name) = unparen(expr) else {
            return None;
        };
        let name = tree::name_ref_text(&name)?;
        let entity: &'a EntityModel = self.entity;
        entity.fields.iter().find(|f| f.name == name)
    }

    /// The field called `name`, or an error naming the entity's fields.
    fn field_named(&mut self, name: &str, span: &Span) -> Option<&'a FieldModel> {
        let entity = self.entity;
        if let Some(found) = entity.fields.iter().find(|f| f.name == name) {
            return Some(found);
        }
        let names: Vec<&str> = entity.fields.iter().map(|f| f.name.as_str()).collect();
        let near = nearest(name, names.iter().copied());
        let listed: Vec<String> = names.iter().map(|n| format!("`{n}`")).collect();
        let problem = Diagnostic::new(
            &rule::DATA_LIFECYCLE_UNKNOWN_FIELD,
            format!("`{}` has no field `{name}`", entity.name),
        )
        .at(span.clone(), "not declared")
        .reason(format!(
            "A lifecycle names the fields of the record it is written in, and `{}`'s fields are {}.{}",
            entity.name,
            crate::and_list(&listed),
            near.map(|n| format!(" Did you mean `{n}`?")).unwrap_or_default()
        ));
        self.out.push(match near {
            Some(near) => problem
                .next(format!("Use `{near}` instead of `{name}`"))
                .edit(span.clone(), near),
            None => problem.next(format!("Name a field of `{}`", entity.name)),
        });
        None
    }

    fn unsupported(&mut self, expr: &Expr, reason: String, next: String) {
        let written = shown(expr.syntax());
        self.out.push(
            Diagnostic::new(
                &rule::DATA_LIFECYCLE_CONDITION,
                format!("`{written}` isn't a condition a lifecycle can use"),
            )
            .at(self.span(expr), "not a test of the record's own fields")
            .reason(reason)
            .next(next),
        );
    }

    // --- values ----------------------------------------------------------------

    /// A value compared with `field`, or set into it when `set` is true:
    /// a literal, an enum value, `clock.now`, or for a nullable field set,
    /// `none`.
    fn value(&mut self, expr: &Expr, field: &FieldModel, set: bool) -> Option<Value> {
        let expr = unparen(expr);
        let expected = field_ty(self.entity, field);
        let constant = Const::from_expr(&expr);
        match &expr {
            Expr::Literal(_) | Expr::PrefixExpr(_)
                if !matches!(constant, Const::Other | Const::Path(_)) =>
            {
                if matches!(constant, Const::None) && field.nullable {
                    return Some(Value::Literal(Const::None));
                }
                if let Err(problem) =
                    crate::types::literal::check(self.lookup, expected.strip_nullable(), &constant)
                {
                    self.out.push(crate::types::literal::diagnostic(
                        self.lookup,
                        problem,
                        &shown(expr.syntax()),
                        expected.strip_nullable(),
                        self.span(&expr),
                        false,
                    ));
                    return None;
                }
                return Some(Value::Literal(constant));
            }
            _ => {}
        }
        if let Some((first, rest)) = dotted(&expr) {
            let head = tree::name_ref_text(&first).unwrap_or_default();
            let rest: Vec<String> = rest.iter().filter_map(tree::name_ref_text).collect();
            if head == "clock" && rest == ["now"] {
                let ty = Ty::Prim(Prim::Instant);
                if !comparable(self.lookup, &ty, &expected) {
                    self.mismatch(&expr, &ty, &expected);
                    return None;
                }
                return Some(Value::Now);
            }
            if starts_upper(&head)
                && let ([variant], Some(Def::Enum { variants })) =
                    (rest.as_slice(), self.lookup.def(&head))
            {
                if !variants.iter().any(|v| v.name == *variant) {
                    let listed: Vec<String> =
                        variants.iter().map(|v| format!("`{}`", v.name)).collect();
                    let near = nearest(variant, variants.iter().map(|v| v.name.as_str()));
                    self.out.push(
                        Diagnostic::new(
                            &rule::TYPE_UNKNOWN_ENUM_VARIANT,
                            format!("`{head}` has no variant `{variant}`"),
                        )
                        .at(self.span(&expr), "not a variant")
                        .reason(format!(
                            "`{head}`'s variants are {}.{}",
                            crate::and_list(&listed),
                            near.map(|n| format!(" Did you mean `{n}`?"))
                                .unwrap_or_default()
                        ))
                        .next(match near {
                            Some(near) => format!("Use `{head}.{near}`"),
                            None => format!("Use one of `{head}`'s variants"),
                        }),
                    );
                    return None;
                }
                let ty = Ty::Named(head.clone());
                if !comparable(self.lookup, &ty, &expected) {
                    self.mismatch(&expr, &ty, &expected);
                    return None;
                }
                return Some(Value::Variant {
                    ty: head,
                    variant: variant.clone(),
                });
            }
        }
        // A type that names nothing is resolution's one error.
        if let Some((first, _)) = dotted(&expr)
            && let Some(head) = tree::name_ref_text(&first)
            && starts_upper(&head)
            && self.lookup.def(&head).is_none()
            && !crate::std_lib::StdNames::get().is_type(&head)
        {
            return None;
        }
        let fixed = "a literal, an enum value, `none` or `clock.now`";
        let (message, label, reason) = if let Some(other) = self.bare_field(&expr) {
            (
                format!("`{}` is another field of the record", other.name),
                "a field, not a value",
                if set {
                    format!(
                        "A transition sets each field to a fixed value ({fixed}), the same every time it runs; copying one field into another isn't one."
                    )
                } else {
                    "A lifecycle condition compares a field with a fixed value; comparing two fields of a record can't be served by an index.".to_string()
                },
            )
        } else if set {
            (
                format!("`{}` isn't a fixed value", shown(expr.syntax())),
                "worked out, or from outside the record",
                format!(
                    "A transition sets fixed values: {fixed}. It takes no parameters and knows nothing of who is asking, so the same change happens wherever it runs."
                ),
            )
        } else {
            (
                format!(
                    "`{}` isn't something a lifecycle condition can compare with",
                    shown(expr.syntax())
                ),
                "from outside the record",
                "A lifecycle condition compares the record's own fields with fixed values: a literal, an enum value or `clock.now`. It never mentions other records or who is asking, which is policy (area 9), so it can join every statement's `WHERE` and an index.".to_string(),
            )
        };
        let rule = if set {
            &rule::DATA_LIFECYCLE_VALUE
        } else {
            &rule::DATA_LIFECYCLE_CONDITION
        };
        let next = if set {
            format!("Use {fixed}")
        } else {
            "Compare with a literal, an enum value or `clock.now`".to_string()
        };
        self.out.push(
            Diagnostic::new(rule, message)
                .at(self.span(&expr), label)
                .reason(reason)
                .next(next),
        );
        None
    }

    fn mismatch(&mut self, expr: &Expr, actual: &Ty, expected: &Ty) {
        let written = shown(expr.syntax());
        let expected = expected.strip_nullable();
        self.out.push(
            Diagnostic::new(
                &rule::TYPE_MISMATCH,
                format!("`{written}` can't be used where `{expected}` is required"),
            )
            .at(self.span(expr), format!("a `{actual}`"))
            .reason(format!(
                "A lifecycle compares or sets a field with a value of the field's type, after widening; `{written}` is `{actual}`."
            ))
            .next(format!("Use a `{expected}` value")),
        );
    }

    /// `initial:` or a transition's `set:`: each field once, none the store
    /// generates, each with a fixed value of its type.
    fn values(&mut self, member: &ast::Member, word: &str) -> Vec<(String, Value)> {
        let mut out: Vec<(String, Value)> = Vec::new();
        let mut seen: Vec<(String, Span)> = Vec::new();
        for (name, entry) in tree::members(member.block()) {
            let Some(field_name) = tree::name_text(&name) else {
                continue;
            };
            let span = self.span(&name);
            if let Some((_, first)) = seen.iter().find(|(seen, _)| *seen == field_name) {
                let first = first.clone();
                self.out.push(
                    Diagnostic::new(
                        &rule::DATA_LIFECYCLE_SECTION,
                        format!("`{field_name}` is given twice in `{word}:`"),
                    )
                    .at(span, "given again")
                    .also(first, "first given here")
                    .reason(format!("Each field of `{word}:` is given once."))
                    .next(format!("Remove one `{field_name}`")),
                );
                continue;
            }
            seen.push((field_name.clone(), span.clone()));
            let Some(field) = self.field_named(&field_name, &span) else {
                continue;
            };
            let field = field.clone();
            if field.name == "id" || field.generated.is_some() {
                let why = if field.name == "id" {
                    format!(
                        "`{}.id` is generated when the record is created and never changes.",
                        self.entity.name
                    )
                } else {
                    format!(
                        "`{}.{}` is generated from the operation's clock; nothing assigns it.",
                        self.entity.name, field.name
                    )
                };
                self.out.push(
                    Diagnostic::new(
                        &rule::DATA_SET_GENERATED_FIELD,
                        format!("`{word}:` sets `{}`, which the store generates", field.name),
                    )
                    .at(span, "generated")
                    .reason(why)
                    .next(format!("Remove `{}` from `{word}:`", field.name)),
                );
                continue;
            }
            let Some(value) = entry.value() else {
                continue;
            };
            if let Some(value) = self.value(&value, &field, true) {
                out.push((field.name.clone(), value));
            }
        }
        out
    }

    // --- transitions -------------------------------------------------------------

    fn transitions(
        &mut self,
        member: &ast::Member,
        visible: Option<&Condition>,
        unknown: bool,
    ) -> (Vec<TransitionModel>, Vec<Span>) {
        let mut out: Vec<TransitionModel> = Vec::new();
        let mut spans: Vec<Span> = Vec::new();
        let takes = "A transition has an optional `when:`, saying when it is allowed, and a `set:` giving the fields it changes.";
        let entity = self.entity;
        for (name, entry) in tree::members(member.block()) {
            let Some(transition) = tree::name_text(&name) else {
                continue;
            };
            let span = self.span(&name);
            if let Some(first) = out.iter().position(|t| t.name == transition) {
                let first = spans[first].clone();
                self.out.push(
                    Diagnostic::new(
                        &rule::DATA_LIFECYCLE_SECTION,
                        format!("The transition `{transition}` is given twice"),
                    )
                    .at(span, "given again")
                    .also(first, "first given here")
                    .reason("Each transition is an operation of its own, called by name, so each name is given once.")
                    .next(format!("Remove one `{transition}`")),
                );
                continue;
            }
            let clash = if OPERATIONS.contains(&transition.as_str()) {
                Some(format!(
                    "`{}.{transition}` is already an operation every entity has, so a transition can't take its name; `delete` is the one a transition may take over.",
                    entity.name
                ))
            } else if entity.fields.iter().any(|f| f.name == transition) {
                Some(format!(
                    "`{}.{transition}(value)` builds a value of the field's type, so a transition can't share the field's name.",
                    entity.name
                ))
            } else if !is_lower_snake(&transition) {
                Some(format!(
                    "A transition is called like an operation, `{}.{}(record)`, so its name is lower_snake_case.",
                    entity.name,
                    crate::case::to_snake(&transition)
                ))
            } else {
                None
            };
            if let Some(reason) = clash {
                self.out.push(
                    Diagnostic::new(
                        &rule::DATA_LIFECYCLE_TRANSITION_NAME,
                        format!("A transition can't be called `{transition}`"),
                    )
                    .at(span, "name in use")
                    .reason(reason)
                    .next("Name the transition after the change it makes, such as `archive`"),
                );
                continue;
            }
            let sections = self.sections(entry.block(), &["when", "set"], takes, "transition");
            let when = sections
                .iter()
                .find(|(word, _)| word == "when")
                .and_then(|(_, member)| self.condition_of(member, "when"));
            let set = match sections.iter().find(|(word, _)| word == "set") {
                Some((_, member)) => {
                    let set = self.values(member, "set");
                    if tree::members(member.block()).is_empty() {
                        let at = member
                            .member_key()
                            .map_or_else(|| self.span(member), |k| self.span(&k));
                        self.out.push(
                            Diagnostic::new(
                                &rule::DATA_LIFECYCLE_SECTION,
                                format!("The transition `{transition}`'s `set:` gives no fields"),
                            )
                            .at(at, "nothing to change")
                            .reason(
                                "A transition is a named change, so it sets at least one field.",
                            )
                            .next("Add the fields it changes to `set:`"),
                        );
                    }
                    set
                }
                None => {
                    self.missing_section(span.clone(), "set", takes, "transition");
                    Vec::new()
                }
            };
            let guarded = match (&when, visible) {
                _ if unknown => false,
                (None, _) => false,
                (Some(_), None) => true,
                (Some(when), Some(visible)) => {
                    let shown = conjuncts(visible);
                    !conjuncts(when).iter().all(|part| shown.contains(part))
                }
            };
            out.push(TransitionModel {
                name: transition,
                when,
                guarded,
                set,
            });
            spans.push(span);
        }
        (out, spans)
    }

    // --- purge -------------------------------------------------------------------

    fn purge(&mut self, member: &ast::Member, visible: bool) -> Option<PurgeModel> {
        let takes = "`purge:` says how long a hidden record is kept, `after:`, counted from one of its time fields, `since:`.";
        let key_span = member
            .member_key()
            .map_or_else(|| self.span(member), |k| self.span(&k));
        let sections = self.sections(member.block(), &["after", "since"], takes, "purge");
        if !visible {
            self.out.push(
                Diagnostic::new(
                    &rule::DATA_LIFECYCLE_PURGE_VISIBLE,
                    format!("`{}`'s lifecycle purges records but hides none", self.entity.name),
                )
                .at(key_span.clone(), "no `visible:` above")
                .reason("Purge permanently deletes records once they have been hidden for a while, so the lifecycle must say which records are hidden, with `visible:`.")
                .next("Add `visible: <condition>` to the lifecycle"),
            );
        }
        let after = match sections.iter().find(|(word, _)| word == "after") {
            Some((_, member)) => self.after(member),
            None => {
                self.missing_section(key_span.clone(), "after", takes, "purge");
                None
            }
        };
        let since = match sections.iter().find(|(word, _)| word == "since") {
            Some((_, member)) => self.since(member),
            None => {
                self.missing_section(key_span, "since", takes, "purge");
                None
            }
        };
        let (after, after_written) = after?;
        let since = since?;
        visible.then_some(PurgeModel {
            after,
            after_written,
            since,
        })
    }

    /// `after:`: a positive duration, `30d`.
    fn after(&mut self, member: &ast::Member) -> Option<(i128, String)> {
        let value = member.value()?;
        let span = self.span(&value);
        let written = shown(value.syntax());
        if let Some((first, rest)) = dotted(&value)
            && tree::name_ref_text(&first).as_deref() == Some("config")
            && rest.len() == 1
        {
            self.out.push(
                Diagnostic::new(
                    &rule::TOOLCHAIN_NOT_BUILT_YET,
                    format!("`{written}` isn't built yet"),
                )
                .at(span, "a setting")
                .reason("A retention period is a setting in `config.jadpo` (area 10), which comes with chunk 5.1; until then `after:` takes a duration written in place.")
                .next("Write the period, such as `after: 30d`, for now"),
            );
            return None;
        }
        let problem = match Const::from_expr(&value) {
            Const::Duration(text) => match jadpo_std::Dur::parse(&text) {
                Some(duration) if duration.millis > 0 => return Some((duration.millis, text)),
                Some(_) => "a hidden record would be deleted the moment it is hidden",
                None => "it isn't a duration Jadpo reads",
            },
            _ => "it isn't a duration",
        };
        self.out.push(
            Diagnostic::new(
                &rule::DATA_LIFECYCLE_PURGE_AFTER,
                format!("`after: {written}` isn't a positive duration"),
            )
            .at(span, "not a positive duration")
            .reason(format!(
                "`after:` is how long a hidden record is kept before purge deletes it, a duration such as `30d` or `12h`, longer than nothing; {problem}."
            ))
            .next("Write a duration, such as `after: 30d`"),
        );
        None
    }

    /// `since:`: a time field of the record.
    fn since(&mut self, member: &ast::Member) -> Option<String> {
        let value = member.value()?;
        let span = self.span(&value);
        let Expr::NameRef(name) = unparen(&value) else {
            self.out.push(
                Diagnostic::new(
                    &rule::DATA_LIFECYCLE_PURGE_SINCE,
                    format!("`since: {}` isn't a field", shown(value.syntax())),
                )
                .at(span, "not a field")
                .reason("`since:` names the `Instant` field of the record that purge counts from, such as the one the hiding transition sets.")
                .next("Name an `Instant` field of the record"),
            );
            return None;
        };
        let name = tree::name_ref_text(&name)?;
        let field = self.field_named(&name, &span)?.clone();
        let ty = field_ty(self.entity, &field);
        if self.lookup.representation(&ty) != Some(Prim::Instant) {
            self.out.push(
                Diagnostic::new(
                    &rule::DATA_LIFECYCLE_PURGE_SINCE,
                    format!("`{name}` isn't a time"),
                )
                .at(span, format!("a `{}`", field.ty.strip_nullable()))
                .reason("`since:` names the `Instant` field purge counts from: a record goes once it has been hidden for `after:` since that time.")
                .next("Name an `Instant` field of the record"),
            );
            return None;
        }
        Some(name)
    }
}

/// A field's type as a lifecycle compares it: `Todo.status`, or
/// `Todo.status?` for a nullable field.
fn field_ty(entity: &EntityModel, field: &FieldModel) -> Ty {
    // A field whose type names nothing absorbs what is compared with it.
    if field.ty.has_error() {
        return Ty::Error;
    }
    let ty = Ty::Field(entity.name.clone(), vec![field.name.clone()]);
    if field.nullable { ty.nullable() } else { ty }
}

fn compare_of(token: &str) -> Option<Compare> {
    Some(match token {
        "==" => Compare::Equal,
        "!=" => Compare::NotEqual,
        "<" => Compare::Less,
        "<=" => Compare::LessEqual,
        ">" => Compare::Greater,
        ">=" => Compare::GreaterEqual,
        _ => return None,
    })
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
