//! Failures inside bodies (area 4): which failures each call can give,
//! `reject` with its fields, `attempt` and its mappings, `propagate`, and
//! the `fails` list, which names exactly what can leave the callable.
//!
//! A caller reads a callee's `fails` list, never its body: the callee's own
//! check makes sure the list is exact, so one mistake gives one error and
//! an edit inside a body never rechecks its callers.

use std::collections::BTreeMap;

use jadpo_db::ItemKind;
use jadpo_diag::{Diagnostic, Span, nearest, rule};
use jadpo_syntax::ast::{self, AstNode, Expr, MemberItem};

use super::super::shape::FieldsBase;
use super::super::table::{Def, FieldDef};
use super::super::ty::Ty;
use super::facts::Reach;
use super::{Arm, Body, did_you_mean, is_literal, line_span, node_key, text};
use crate::tree::{self, dotted};

/// What a call can do besides give its result.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Fallible {
    /// It always gives its result.
    Never,
    /// Not known here: a later area names its failures (an entity's
    /// operations, area 6), or the call is already reported.
    Unknown,
    /// It can fail with these declared failures, in the order its `fails`
    /// list names them.
    Fails(Vec<String>),
    /// A construction whose value is only known at runtime and which the
    /// declared ranges don't prove safe: it fails when the value breaks
    /// the type's rules, with the failure `invalid:` names.
    Construction,
}

impl Body<'_> {
    // --- what a call can fail with ----------------------------------------

    /// What a call can fail with: a construction's verdict, or the
    /// `fails` list of what the call reaches, as the call was resolved.
    pub(super) fn call_fallible(&self, call: &ast::CallExpr) -> Fallible {
        let key = node_key(call.syntax());
        if let Some(verdict) = self.facts.constructions.get(&key) {
            return verdict.clone();
        }
        match self.facts.reaches.get(&key) {
            Some(Reach::Callable { fails, .. } | Reach::Library { fails, .. }) => {
                self.fails_fallible(fails)
            }
            // The standard library computes its results; nothing in it
            // fails with a domain failure.
            Some(Reach::Std(_)) => Fallible::Never,
            Some(Reach::Entity { entity, operation }) => self.entity_fallible(entity, operation),
            Some(Reach::Construct(_) | Reach::Unknown) | None => Fallible::Unknown,
        }
    }

    fn fails_fallible(&self, fails: &[String]) -> Fallible {
        if fails.is_empty() {
            return Fallible::Never;
        }
        let mut names: Vec<String> = Vec::new();
        for name in fails {
            if !self.failures.is_declared_failure(name) {
                // Not a failure: reported where the list is written.
                return Fallible::Unknown;
            }
            if !names.contains(name) {
                names.push(name.clone());
            }
        }
        Fallible::Fails(names)
    }

    /// An entity's operations (area 6): `get` fails with the entity's
    /// not-found failure when the project declares it; the compiler
    /// provides one otherwise, and the writes' failures come from their
    /// blocks, both built with area 6.
    fn entity_fallible(&self, entity: &str, operation: &str) -> Fallible {
        let missing = format!("{entity}NotFound");
        if operation == "get" && self.failures.is_declared_failure(&missing) {
            Fallible::Fails(vec![missing])
        } else {
            Fallible::Unknown
        }
    }

    /// A call nothing handles: one that can fail needs `attempt` or an
    /// outcome `match` (area 3, question 11).
    pub(super) fn unhandled(&mut self, call: &ast::CallExpr) {
        if self.handled.contains(&node_key(call.syntax())) {
            return;
        }
        let fallible = self.call_fallible(call);
        let written = call
            .expr()
            .map(|callee| tree::written(callee.syntax()))
            .unwrap_or_default();
        let span = self.span(call);
        let problem = match &fallible {
            Fallible::Fails(names) => {
                let listed: Vec<String> = names.iter().map(|n| format!("`{n}`")).collect();
                Diagnostic::new(
                    &rule::FAILURE_ATTEMPT_REQUIRED,
                    format!("`{written}` can fail, and nothing handles it"),
                )
                .at(span, format!("can fail with {}", crate::and_list(&listed)))
                .reason("A call that can fail says so where it is made: `attempt` passes its failures on to the caller's `fails` list, and `match` handles each one.")
                .next(format!(
                    "Start the statement with `attempt`, or `match` on `{written}(...)` and handle {}",
                    crate::and_list(&listed)
                ))
            }
            Fallible::Construction => Diagnostic::new(
                &rule::FAILURE_ATTEMPT_REQUIRED,
                format!("`{written}(...)` can fail at runtime, and nothing handles it"),
            )
            .at(span, "checked when it runs")
            .reason(format!(
                "The value is only known when the program runs, and the declared ranges don't prove it follows `{written}`'s rules, so building it can fail."
            ))
            .next(format!(
                "Write `attempt {written}(...) invalid: SomeFailure` naming an `invalid_value` failure"
            )),
            Fallible::Never | Fallible::Unknown => return,
        };
        self.flow.open = true;
        self.out.push(problem);
    }

    // --- leaving the callable ---------------------------------------------

    /// Records that `failure` can leave the callable here.
    fn leave(&mut self, failure: &str, span: Span, how: &'static str) {
        self.flow
            .leaving
            .entry(failure.to_string())
            .or_insert((span, how));
    }

    /// `reject Failure` or `reject Failure { ... }`.
    pub(super) fn reject(&mut self, reject: &ast::RejectExpr) {
        let span = line_span(self.path, reject.syntax());
        match reject.expr().and_then(|inner| self.failure_given(&inner)) {
            Some(name) => {
                self.facts
                    .rejects
                    .insert(node_key(reject.syntax()), name.clone());
                self.leave(&name, span, "rejected here");
            }
            None => self.flow.open = true,
        }
    }

    /// `propagate`: only in a failure arm of an outcome `match`, where it
    /// passes that failure on.
    pub(super) fn propagate(&mut self, propagate: &ast::PropagateExpr) {
        let span = self.span(propagate);
        match self.arm.clone() {
            Arm::Failure(Some(name)) => {
                self.facts
                    .propagates
                    .insert(node_key(propagate.syntax()), name.clone());
                self.leave(&name, span, "passed on here");
            }
            Arm::Failure(None) => {}
            Arm::Success => {
                // The outcome check reports it, at the arm.
            }
            Arm::None => {
                self.flow.open = true;
                self.out.push(
                    Diagnostic::new(
                        &rule::FAILURE_PROPAGATE_PLACEMENT,
                        "`propagate` has no failure to pass on here",
                    )
                    .at(span, "not in a failure arm")
                    .reason("`propagate` is an arm of a `match` on a call that can fail: it passes the failure that arm names on to the caller's `fails` list.")
                    .next("Give a value, `return` or `reject` a failure here instead"),
                );
            }
        }
    }

    /// The failure a `reject` or a mapping names, with its fields checked;
    /// `None` when it isn't a failure, which resolution reports.
    pub(super) fn failure_given(&mut self, expr: &Expr) -> Option<String> {
        let (head, record) = match expr {
            Expr::NameRef(name) => (name.clone(), None),
            Expr::RecordExpr(record) => match record.expr() {
                Some(Expr::NameRef(name)) => (name, Some(record.clone())),
                _ => {
                    self.failure_value(expr);
                    return None;
                }
            },
            other => {
                self.infer(other);
                return None;
            }
        };
        let name = text(&head);
        let fields = match self.table.def(&name) {
            Some(Def::Fields {
                base: FieldsBase::Failure,
                fields,
                ..
            }) => fields.clone(),
            _ => {
                if let Some(record) = &record {
                    self.failure_value(&Expr::RecordExpr(record.clone()));
                }
                return None;
            }
        };
        let head_span = self.span(&head);
        self.context_given(&name, record.as_ref(), &fields, head_span);
        Some(name)
    }

    /// A rejected failure's fields: every one given once, none unknown,
    /// each value checked against its field (area 4, "Raising a failure").
    fn context_given(
        &mut self,
        failure: &str,
        record: Option<&ast::RecordExpr>,
        fields: &[FieldDef],
        head_span: Span,
    ) {
        let mut given: BTreeMap<String, Span> = BTreeMap::new();
        for item in record
            .and_then(ast::RecordExpr::member_list)
            .iter()
            .flat_map(|list| list.member_items())
        {
            let MemberItem::Member(member) = item else {
                continue;
            };
            let mut names = member.member_key().into_iter().flat_map(|k| k.names());
            let (Some(first), None) = (names.next(), names.next()) else {
                continue;
            };
            let Some(field) = tree::name_text(&first) else {
                continue;
            };
            let span = self.span(&first);
            let value = member.value();
            if let Some(earlier) = given.get(&field) {
                self.out.push(
                    Diagnostic::new(
                        &rule::FAILURE_DUPLICATE_CONTEXT_FIELD,
                        format!("`{field}` is given twice"),
                    )
                    .at(span, "given again")
                    .also(earlier.clone(), "first given here")
                    .reason(format!(
                        "Each field of `{failure}` is given once, so the log and the client never have to guess which value counts."
                    ))
                    .next(format!("Remove one `{field}`")),
                );
                if let Some(value) = value {
                    self.infer(&value);
                }
                continue;
            }
            given.insert(field.clone(), span.clone());
            let Some(declared) = fields.iter().find(|f| f.name == field) else {
                let names: Vec<&str> = fields.iter().map(|f| f.name.as_str()).collect();
                let near = nearest(&field, names.iter().copied());
                let listed: Vec<String> = names.iter().map(|n| format!("`{n}`")).collect();
                let declared_line = if listed.is_empty() {
                    format!("`{failure}` has no fields.")
                } else {
                    format!("`{failure}`'s fields are {}.", crate::and_list(&listed))
                };
                let problem = Diagnostic::new(
                    &rule::FAILURE_UNKNOWN_CONTEXT_FIELD,
                    format!("`{failure}` has no field `{field}`"),
                )
                .at(span.clone(), "not declared")
                .reason(format!(
                    "{declared_line} A failure carries only the fields its `client` and `internal` blocks declare.{}",
                    did_you_mean(near)
                ));
                self.out.push(match near {
                    Some(near) => problem
                        .next(format!("Use `{near}` instead of `{field}`"))
                        .edit(span, near),
                    None => problem.next(format!(
                        "Remove `{field}`, or declare it in `{failure}`'s `client` or `internal` block"
                    )),
                });
                if let Some(value) = value {
                    self.infer(&value);
                }
                continue;
            };
            let Some(value) = value else { continue };
            if is_literal(&value) || matches!(value, Expr::ListExpr(_)) {
                let expected = Ty::Field(failure.to_string(), vec![field]);
                self.check(&value, &expected);
            } else {
                self.check(&value, &declared.declared.clone());
            }
        }
        let missing: Vec<String> = fields
            .iter()
            .filter(|f| !given.contains_key(&f.name))
            .map(|f| format!("`{}`", f.name))
            .collect();
        if missing.is_empty() {
            return;
        }
        let first = missing[0].trim_matches('`').to_string();
        let next = match record {
            Some(_) => format!("Add `{first}: ...` to the block"),
            None => format!("Give its fields in a block: `{failure} {{ {first}: ... }}`"),
        };
        self.out.push(
            Diagnostic::new(
                &rule::FAILURE_MISSING_CONTEXT_FIELD,
                format!("`{failure}` is missing {}", crate::and_list(&missing)),
            )
            .at(head_span, "fields missing")
            .reason("A failure is raised with every field its `client` and `internal` blocks declare, so the client and the log always get them.")
            .next(next),
        );
    }

    // --- attempt ----------------------------------------------------------

    /// `attempt call(...)`, with any mappings after it; the call's result.
    pub(super) fn attempt(&mut self, attempt: &ast::AttemptExpr, expected: Option<&Ty>) -> Ty {
        let span = line_span(self.path, attempt.syntax());
        let Some(inner) = attempt.expr() else {
            return Ty::Error;
        };
        let (ty, fallible) = match &inner {
            Expr::CallExpr(call) => {
                self.handled.push(node_key(call.syntax()));
                let ty = self.call(call, expected);
                self.handled.pop();
                (ty, self.call_fallible(call))
            }
            Expr::RecordExpr(record) => {
                let before = self.out.len();
                self.handled.push(node_key(record.syntax()));
                let ty = self.infer(&inner);
                self.handled.pop();
                let fallible = if self.out.len() > before {
                    Fallible::Unknown
                } else {
                    self.record_fallible(record)
                };
                (ty, fallible)
            }
            other => (self.infer(other), Fallible::Never),
        };
        let mappings: Vec<ast::Mapping> = attempt.mappings().collect();
        let written = match &inner {
            Expr::CallExpr(call) => call
                .expr()
                .map(|callee| format!("{}(...)", tree::written(callee.syntax())))
                .unwrap_or_default(),
            Expr::RecordExpr(record) => record
                .expr()
                .map(|head| format!("{} {{ ... }}", tree::written(head.syntax())))
                .unwrap_or_default(),
            other => tree::written(other.syntax()),
        };
        match &fallible {
            Fallible::Never => {
                for mapping in &mappings {
                    if let Some(value) = mapping.expr() {
                        self.failure_given(&value);
                    }
                }
                self.attempt_infallible(attempt, &inner, &written);
            }
            Fallible::Unknown => {
                self.flow.open = true;
                for mapping in &mappings {
                    if let Some(value) = mapping.expr()
                        && let Some(name) = self.failure_given(&value)
                    {
                        self.leave(&name, span.clone(), "mapped here");
                    }
                }
            }
            Fallible::Fails(names) => {
                for name in names {
                    self.leave(name, span.clone(), "passed on here");
                }
                for mapping in &mappings {
                    if let Some(value) = mapping.expr() {
                        self.failure_given(&value);
                    }
                    self.unknown_mapping(mapping, &written, false);
                }
            }
            Fallible::Construction => {
                if let Some(failure) = self.construction_mappings(&mappings, &written, span) {
                    self.facts
                        .mapped
                        .insert(node_key(attempt.syntax()), failure);
                }
            }
        }
        ty
    }

    /// An entity write with a block (area 6), or an object being built.
    fn record_fallible(&self, record: &ast::RecordExpr) -> Fallible {
        if let Some(verdict) = self.facts.constructions.get(&node_key(record.syntax())) {
            return verdict.clone();
        }
        match record.expr() {
            Some(Expr::CallExpr(_)) => Fallible::Unknown,
            Some(head) => match dotted(&head) {
                Some((first, _)) if self.callable_kind(&text(&first)) == Some(ItemKind::Entity) => {
                    Fallible::Unknown
                }
                Some((first, rest)) if rest.is_empty() => match self.table.def(&text(&first)) {
                    // Building an object checks each field's value against
                    // its type while compiling.
                    Some(Def::Fields { .. }) => Fallible::Never,
                    _ => Fallible::Unknown,
                },
                _ => Fallible::Unknown,
            },
            None => Fallible::Unknown,
        }
    }

    fn attempt_infallible(&mut self, attempt: &ast::AttemptExpr, inner: &Expr, written: &str) {
        let span = attempt
            .attempt_kw_token()
            .map(|token| {
                let range = token.text_range();
                Span::new(self.path, range.start().into(), range.end().into())
            })
            .unwrap_or_else(|| self.span(attempt));
        // The edit removes `attempt ` and any mappings after the call.
        let start = span.start;
        let inner_start = u32::from(inner.syntax().text_range().start());
        let inner_end = u32::from(inner.syntax().text_range().end());
        let end = u32::from(attempt.syntax().text_range().end());
        let reason = match inner {
            Expr::CallExpr(call)
                if self
                    .facts
                    .constructions
                    .contains_key(&node_key(call.syntax())) =>
            {
                format!(
                    "The declared ranges prove `{written}` always follows its type's rules, so building it never fails; `attempt` marks only what can."
                )
            }
            Expr::CallExpr(call) if self.is_standard(call) => format!(
                "`{written}` is a standard-library operation, which always gives its result; `attempt` marks only calls that can fail."
            ),
            Expr::CallExpr(_) => format!(
                "`{written}` lists no failures after `fails`, so it always gives its result; `attempt` marks only calls that can fail."
            ),
            Expr::RecordExpr(_) => format!(
                "Every field of `{written}` gets a value of its own type, or one the declared ranges prove follows the field's own rules, so building it never fails; `attempt` marks only what can."
            ),
            _ => "`attempt` marks a call that can fail, and this isn't a call.".to_string(),
        };
        let mut problem = Diagnostic::new(
            &rule::FAILURE_ATTEMPT_INFALLIBLE,
            format!("`{written}` can't fail, so it takes no `attempt`"),
        )
        .at(span.clone(), "nothing to pass on")
        .reason(reason)
        .next("Remove `attempt`")
        .edit(Span::new(self.path, start, inner_start), "");
        if end > inner_end {
            problem = problem.edit(Span::new(self.path, inner_end, end), "");
        }
        self.flow.open = true;
        self.out.push(problem);
    }

    /// Whether a call is to a standard-library operation.
    fn is_standard(&self, call: &ast::CallExpr) -> bool {
        matches!(
            self.facts.reaches.get(&node_key(call.syntax())),
            Some(Reach::Std(_))
        )
    }

    /// The mappings after a construction that can fail: `invalid:` names
    /// an `invalid_value` failure, and nothing else is mapped. The failure
    /// `invalid:` names, when it names one.
    fn construction_mappings(
        &mut self,
        mappings: &[ast::Mapping],
        written: &str,
        span: Span,
    ) -> Option<String> {
        let mut invalid = None;
        let mut named = false;
        let mut unknown = false;
        for mapping in mappings {
            let condition = mapping
                .name()
                .as_ref()
                .and_then(tree::name_text)
                .unwrap_or_default();
            let failure = mapping.expr().and_then(|value| self.failure_given(&value));
            if condition != "invalid" || named {
                unknown = true;
                self.unknown_mapping(mapping, written, true);
                if let Some(failure) = failure {
                    self.leave(&failure, span.clone(), "mapped here");
                }
                continue;
            }
            named = true;
            let Some(failure) = failure else {
                self.flow.open = true;
                continue;
            };
            self.leave(&failure, span.clone(), "mapped here");
            self.mapping_kind(mapping, &failure, "invalid", "invalid_value");
            invalid = Some(failure);
        }
        if named || unknown {
            return invalid;
        }
        self.flow.open = true;
        self.out.push(
            Diagnostic::new(
                &rule::FAILURE_INVALID_MAPPING_REQUIRED,
                format!("`{written}` can fail, but no failure is named for it"),
            )
            .at(span, "can fail at runtime")
            .reason("The value is only known when the program runs, and the declared ranges don't prove it follows the type's rules; `invalid:` names the `invalid_value` failure the caller gets when it doesn't.")
            .next(format!("Add `invalid: SomeFailure` after `{written}`")),
        );
        None
    }

    /// The failure a mapping names must have the kind its condition needs
    /// (area 4, question 7).
    fn mapping_kind(&mut self, mapping: &ast::Mapping, failure: &str, condition: &str, kind: &str) {
        let Some(info) = self.failures.failure_info(failure) else {
            return;
        };
        let Some(actual) = info.kind.clone() else {
            return;
        };
        if actual == kind {
            return;
        }
        let span = mapping
            .expr()
            .map_or_else(|| self.span(mapping), |value| self.span(&value));
        self.out.push(
            Diagnostic::new(
                &rule::FAILURE_MAPPING_KIND,
                format!("`{condition}:` needs an `{kind}` failure, and `{failure}` is `{actual}`"),
            )
            .at(span, format!("kind `{actual}`"))
            .reason(format!(
                "Each mapping's condition fixes the kind of failure it becomes: a value that breaks a type's rules is `{kind}`, so the client gets the status that means."
            ))
            .next(format!(
                "Name an `{kind}` failure here, or make `{failure}`'s kind `FailureKind.{kind}`"
            )),
        );
    }

    fn unknown_mapping(&mut self, mapping: &ast::Mapping, written: &str, construction: bool) {
        let condition = mapping
            .name()
            .as_ref()
            .and_then(tree::name_text)
            .unwrap_or_default();
        let span = mapping
            .name()
            .map_or_else(|| self.span(mapping), |name| self.span(&name));
        self.flow.open = true;
        let (reason, next) = if construction {
            (
                format!(
                    "A construction has one condition to map, `invalid:`, when its value breaks the type's rules; `{condition}:` isn't one `{written}` has, or is given twice."
                ),
                "Use `invalid:` once".to_string(),
            )
        } else {
            (
                format!(
                    "`{written}` fails with the failures its `fails` list names; `attempt` passes them on as they are, and `match` turns one into another."
                ),
                format!("Remove `{condition}: ...`, and `match` on the call to map its failures"),
            )
        };
        self.out.push(
            Diagnostic::new(
                &rule::FAILURE_UNKNOWN_MAPPING,
                format!("`{written}` has no `{condition}:` condition to map"),
            )
            .at(span, "not a condition here")
            .reason(reason)
            .next(next),
        );
    }

    // --- the `fails` list ---------------------------------------------------

    /// Checks the callable's `fails` list against what can leave it: every
    /// failure that can leave is listed, nothing else is, and nothing twice.
    pub(super) fn fails_list(
        &mut self,
        fails: Option<&ast::FailsClause>,
        params: Option<&ast::ParamList>,
        word: &str,
    ) {
        let name = self.flow.name.clone();
        let mut listed: BTreeMap<String, Span> = BTreeMap::new();
        let mut entries: Vec<(String, ast::Type)> = Vec::new();
        for ty in fails.iter().flat_map(|clause| clause.types()) {
            entries.push((tree::written(ty.syntax()), ty));
        }
        for (index, (written, ty)) in entries.iter().enumerate() {
            let span = self.span(ty);
            if let Some(first) = listed.get(written) {
                let previous_end = u32::from(entries[index - 1].1.syntax().text_range().end());
                self.out.push(
                    Diagnostic::new(
                        &rule::FAILURE_DUPLICATE_DECLARATION,
                        format!("`{written}` is listed twice after `fails`"),
                    )
                    .at(span.clone(), "listed again")
                    .also(first.clone(), "first listed here")
                    .reason("`fails` lists each failure that can leave once.")
                    .next(format!("Remove the second `{written}`"))
                    .edit(Span::new(self.path, previous_end, span.end), ""),
                );
                continue;
            }
            listed.insert(written.clone(), span);
        }
        // Undeclared: one error for the callable, at the first place one
        // leaves, naming every failure the list misses.
        let mut leaving: Vec<(String, Span, &'static str)> = self
            .flow
            .leaving
            .iter()
            .filter(|(failure, _)| !listed.contains_key(*failure))
            .map(|(failure, (span, how))| (failure.clone(), span.clone(), *how))
            .collect();
        leaving.sort_by_key(|(_, span, _)| (span.start, span.end));
        if let Some((_, first_span, first_how)) = leaving.first().cloned() {
            let names: Vec<String> = leaving.iter().map(|(f, _, _)| format!("`{f}`")).collect();
            let joined: Vec<&str> = leaving.iter().map(|(f, _, _)| f.as_str()).collect();
            let joined = joined.join(", ");
            let insert = match (entries.last(), params) {
                (Some((_, last)), _) => {
                    let end = u32::from(last.syntax().text_range().end());
                    Some((Span::new(self.path, end, end), format!(", {joined}")))
                }
                (None, Some(params)) => {
                    let end = u32::from(params.syntax().text_range().end());
                    Some((Span::new(self.path, end, end), format!(" fails {joined}")))
                }
                (None, None) => None,
            };
            let what = crate::and_list(&names);
            let verb = if names.len() == 1 { "can" } else { "can each" };
            let mut problem = Diagnostic::new(
                &rule::FAILURE_UNDECLARED_PROPAGATION,
                format!("{what} {verb} leave `{name}`, but its `fails` list doesn't say so"),
            )
            .at(first_span.clone(), first_how);
            for (_, span, how) in leaving.iter().skip(1) {
                if *span != first_span {
                    problem = problem.also(span.clone(), *how);
                }
            }
            problem = problem
                .reason(format!(
                    "`fails` lists exactly the failures that can leave {}, so every caller knows what it must handle.",
                    if word.starts_with(['a', 'e', 'i', 'o', 'u']) {
                        format!("an {word}")
                    } else {
                        format!("a {word}")
                    }
                ))
                .next(format!("Add {what} to `{name}`'s `fails` list"));
            if let Some((at, text)) = insert {
                problem = problem.edit(at, text);
            }
            self.out.push(problem);
        }
        if self.flow.open {
            return;
        }
        // Stale: listed, but nothing lets it out.
        for (index, (written, ty)) in entries.iter().enumerate() {
            if self.flow.leaving.contains_key(written)
                || !self.failures.is_declared_failure(written)
                || entries[..index]
                    .iter()
                    .any(|(earlier, _)| earlier == written)
            {
                continue;
            }
            let span = self.span(ty);
            let removal = if entries.len() == 1 {
                let start = params.map_or(span.start, |p| u32::from(p.syntax().text_range().end()));
                Span::new(self.path, start, span.end)
            } else if index == 0 {
                let next = u32::from(entries[1].1.syntax().text_range().start());
                Span::new(self.path, span.start, next)
            } else {
                let previous = u32::from(entries[index - 1].1.syntax().text_range().end());
                Span::new(self.path, previous, span.end)
            };
            self.out.push(
                Diagnostic::new(
                    &rule::FAILURE_STALE_DECLARATION,
                    format!("`{written}` can never leave `{name}`"),
                )
                .at(span, "never raised or passed on")
                .reason(format!(
                    "Nothing in `{name}` rejects `{written}` or passes it on from a call, and `fails` lists exactly the failures that can leave, so callers aren't made to handle one that never happens."
                ))
                .next(format!("Remove `{written}` from `fails`"))
                .edit(removal, ""),
            );
        }
    }
}
