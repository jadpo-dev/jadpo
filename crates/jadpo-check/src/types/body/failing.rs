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
use super::known::Origin;
use super::{Arm as MatchArm, Body, did_you_mean, is_literal, line_span, node_key, text};
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
    /// A construction whose value is only known at runtime and which
    /// neither the declared ranges nor the conditions around it prove
    /// safe: it fails when the value breaks
    /// one of these rules, by their keys (`max`), with the failure its
    /// `attempt` maps that rule to.
    Construction(Vec<String>),
}

/// One arm of an `attempt`'s block: its key, where it is written, and the
/// failure it names.
pub(super) struct Arm {
    pub key: String,
    pub key_span: Span,
    pub member: ast::Member,
    pub value: Option<Expr>,
}

/// What a `temporal` operation's conditions are, by their keys: "a local
/// time the zone's clocks skip or show twice fails".
fn conditions_meant(keys: &[String]) -> String {
    let meant: Vec<String> = keys
        .iter()
        .map(|key| {
            match key.as_str() {
                "dst" => "a local time the zone's clocks skip or show twice",
                "invalid_day" => "a day the month it lands in doesn't have",
                _ => "a condition",
            }
            .to_string()
        })
        .collect();
    format!("{} fails", crate::and_list(&meant).replace(" and ", " or "))
}

/// "`dst:`", or "`invalid_day:` or `dst:`".
fn options_named(keys: &[String]) -> String {
    let listed: Vec<String> = keys.iter().map(|k| format!("`{k}:`")).collect();
    crate::and_list(&listed).replace(" and ", " or ")
}

/// Why a standard operation's conditions may fail, and what would settle
/// them: "A local time ... fails unless `dst:` says what to do instead",
/// or, for one only a proof settles, as `decimal.ln`'s `out_of_range`
/// (area 19, "Numbers"), what proves it.
fn condition_clause(keys: &[String]) -> String {
    if keys.iter().any(|key| key == "invalid_signature") {
        let claims = if keys.iter().any(|key| key == "mismatch") {
            ", and claims that aren't the type `as:` names `mismatch`"
        } else {
            ""
        };
        format!(
            "What a token says is only known when the program runs: one not signed by a key it is given fails `invalid_signature`, one past its time `expired` or `not_yet_valid`, one for another app `wrong_audience` or `wrong_issuer`, and one that isn't a token `malformed`{claims}"
        )
    } else if keys.iter().any(|key| key == "mismatch") {
        "What a document holds is only known when the program runs: text that isn't one JSON document fails `malformed`, and a document that isn't the type `as:` names fails `mismatch`".to_string()
    } else if keys.iter().any(|key| key == "malformed") {
        "What the text or bytes hold is only known when the program runs, and they may not be the encoding at all, which fails `malformed`".to_string()
    } else if keys.iter().any(|key| key == "same_point") {
        "Two equal points have no direction between them, and whether these two are equal is only known when the program runs, which fails `same_point`".to_string()
    } else if keys.iter().any(|key| key == "out_of_range") {
        "There is no logarithm of a value at or below `0`, and nothing proves this one above `0`, as a literal above `0` or a type whose `min` is above `0` would".to_string()
    } else {
        format!(
            "{} unless {} says what to do instead",
            capital_first(&conditions_meant(keys)),
            options_named(keys)
        )
    }
}

/// What a call can give to settle the conditions itself, after "or": the
/// options, or a value proved above `0`; nothing settles what decoding
/// gives (area 19, "Encodings" and "Data formats").
fn settle_by(keys: &[String]) -> String {
    if keys.iter().any(|key| {
        key == "malformed" || key == "mismatch" || key == "invalid_signature" || key == "same_point"
    }) {
        String::new()
    } else if keys.iter().any(|key| key == "out_of_range") {
        ", or pass a value whose type has a `min` above `0`".to_string()
    } else {
        format!(", or give {}", options_named(keys))
    }
}

fn capital_first(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

/// "`max`", or "`min` and `max`".
fn keys_list(keys: &[String]) -> String {
    let listed: Vec<String> = keys.iter().map(|k| format!("`{k}`")).collect();
    crate::and_list(&listed)
}

/// "the condition `n > 12`", or "the conditions `n < 1` and `n > 12`",
/// in source order.
fn conditions_named(by: &[Origin]) -> String {
    let mut sorted: Vec<&Origin> = by.iter().collect();
    sorted.sort_by_key(|origin| origin.key);
    sorted.dedup_by_key(|origin| origin.key);
    let listed: Vec<String> = sorted
        .iter()
        .map(|origin| format!("`{}`", origin.written))
        .collect();
    let word = if listed.len() == 1 {
        "condition"
    } else {
        "conditions"
    };
    format!("the {word} {}", crate::and_list(&listed))
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
        if let Some(write) = self.facts.writes.get(&key) {
            return write.fallible.clone();
        }
        match self.facts.reaches.get(&key) {
            Some(
                Reach::Callable { fails, .. }
                | Reach::Library { fails, .. }
                | Reach::Schedule { fails, .. },
            ) => self.fails_fallible(fails),
            // The standard library computes its results; nothing in it
            // fails with a domain failure.
            Some(Reach::Std(_)) => Fallible::Never,
            // A caller made from a record, or a session started or ended,
            // never fails with a declared failure; a store that can't be
            // reached is the runtime's `unavailable`.
            Some(
                Reach::Caller(_)
                | Reach::Session(_)
                | Reach::Batch(_)
                | Reach::AdvanceClock
                | Reach::Translation { .. }
                | Reach::Prompt(_),
            ) => Fallible::Never,
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

    /// An entity's `get` (area 6) fails with the entity's not-found
    /// failure: the project's, or the one the compiler provides. A write's
    /// failures come from what it writes, recorded where it is checked.
    fn entity_fallible(&self, entity: &str, operation: &str) -> Fallible {
        // A function can't read the store, which is reported where it does.
        if self.context.is_pure() {
            return Fallible::Unknown;
        }
        let missing = format!("{entity}NotFound");
        if operation == "get" && self.failures.is_declared_failure(&missing) {
            Fallible::Fails(vec![missing])
        } else {
            Fallible::Unknown
        }
    }

    /// A write that can fail, written as a block-built value with nothing
    /// handling it: `Project.create { ... }` without `attempt`.
    pub(super) fn unhandled_write(&mut self, node: &jadpo_syntax::SyntaxNode, written: &str) {
        let key = node_key(node);
        if self.handled.contains(&key) {
            return;
        }
        let Some(Fallible::Fails(names)) = self.facts.writes.get(&key).map(|w| w.fallible.clone())
        else {
            return;
        };
        let listed: Vec<String> = names.iter().map(|n| format!("`{n}`")).collect();
        let span = crate::tree::node_span(self.path, node);
        self.flow.open = true;
        self.out.push(
            Diagnostic::new(
                &rule::FAILURE_ATTEMPT_REQUIRED,
                format!("`{written}` can fail, and nothing handles it"),
            )
            .at(span, format!("can fail with {}", crate::and_list(&listed)))
            .reason("A call that can fail says so where it is made: `attempt` passes its failures on to the caller's `fails` list.")
            .next(format!(
                "Start the statement with `attempt`, and list {} after `fails`",
                crate::and_list(&listed)
            )),
        );
    }

    /// A call nothing handles: one that can fail needs `attempt` or an
    /// outcome `match` (area 3, question 11).
    pub(super) fn unhandled(&mut self, call: &ast::CallExpr) {
        if self.handled.contains(&node_key(call.syntax())) {
            return;
        }
        // A backfill has nowhere to pass a failure (area 5, chunk 5.6b).
        if self.backfill_unhandled(call) {
            return;
        }
        // A construction over a stored field in an update by `id` is the
        // database's to work out, so nothing could handle it: the write
        // reports it (area 6, "Counters").
        if self.stored.is_some()
            && call
                .syntax()
                .descendants()
                .any(|node| self.facts.stored.contains(&node_key(&node)))
        {
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
            Fallible::Construction(breakable) if self.is_standard(call) => {
                let first = breakable.first().cloned().unwrap_or_default();
                Diagnostic::new(
                    &rule::FAILURE_ATTEMPT_REQUIRED,
                    format!("`{written}(...)` can fail at runtime, and nothing handles it"),
                )
                .at(span, "checked when it runs")
                .reason(format!(
                    "{}, so the call can fail.",
                    condition_clause(breakable)
                ))
                .next(format!(
                    "Write `attempt {written}(...) {{ {first}: SomeFailure }}`, mapping each condition to an `invalid_value` failure, or `invalid:` for all of them{}",
                    settle_by(breakable)
                ))
            }
            Fallible::Construction(breakable) => {
                let rules = self.written_rules(call, breakable);
                let first = breakable.first().cloned().unwrap_or_default();
                Diagnostic::new(
                    &rule::FAILURE_ATTEMPT_REQUIRED,
                    format!("`{written}(...)` can fail at runtime, and nothing handles it"),
                )
                .at(span, "checked when it runs")
                .reason(self.parse_reason(call, breakable, ", so reading it can fail.").unwrap_or_else(|| format!(
                    "The value is only known when the program runs, and neither the declared ranges nor a condition before it prove it follows {rules}, so building it can fail."
                )))
                .next(format!(
                    "Write `attempt {written}(...) {{ {first}: SomeFailure }}`, mapping each rule that can break to an `invalid_value` failure, or `invalid:` for all of them"
                ))
            }
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
        if let Some(inner) = reject.expr() {
            let mut call = false;
            self.resolved_by(|names, out| call = names.rejected(&inner, out));
            // `reject OutOfStock() { ... }`: nothing below checks the head
            // of the block, so its call is resolved here.
            if call
                && let Expr::RecordExpr(record) = &inner
                && let Some(head) = record.expr()
            {
                self.walk(|walk| walk.expr(&head));
            }
        }
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
            MatchArm::Failure(Some(name)) => {
                self.facts
                    .propagates
                    .insert(node_key(propagate.syntax()), name.clone());
                self.leave(&name, span, "passed on here");
            }
            MatchArm::Failure(None) => {}
            MatchArm::Success => {
                // The outcome check reports it, at the arm.
            }
            MatchArm::None => {
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
        self.failure_given_from(expr, None)
    }

    /// As [`Self::failure_given`], for an arm of an `attempt` on
    /// `json.read`: `fills` is the condition that fills a `problem:
    /// DecodeProblem` field (area 19, "Data formats"), `mismatch` or the
    /// `invalid:` covering only it, with its key's span; `None` where
    /// nothing does.
    fn failure_given_from(&mut self, expr: &Expr, fills: Option<(&str, Span)>) -> Option<String> {
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
        self.context_given(&name, record.as_ref(), &fields, head_span, fills);
        Some(name)
    }

    /// Whether failure `name` declares the field a failure mapped from
    /// `json.read`'s `mismatch` receives: `problem: DecodeProblem`.
    fn receives_problem(&self, name: &str) -> bool {
        matches!(
            self.table.def(name),
            Some(Def::Fields {
                base: FieldsBase::Failure,
                fields,
                ..
            }) if fields.iter().any(is_problem_field)
        )
    }

    /// A rejected failure's fields: every one given once, none unknown,
    /// each value checked against its field (area 4, "Raising a failure").
    fn context_given(
        &mut self,
        failure: &str,
        record: Option<&ast::RecordExpr>,
        fields: &[FieldDef],
        head_span: Span,
        fills: Option<(&str, Span)>,
    ) {
        let mut given: BTreeMap<String, Span> = BTreeMap::new();
        // The field `json.read`'s `mismatch` fills, when this arm maps it.
        let filled = fills
            .as_ref()
            .and_then(|_| fields.iter().find(|f| is_problem_field(f)))
            .map(|f| f.name.clone());
        for item in record
            .and_then(ast::RecordExpr::member_list)
            .iter()
            .flat_map(|list| list.member_items())
        {
            let MemberItem::Member(member) = item else {
                continue;
            };
            // A block inside a field is never checked here; a member that
            // isn't a field is reported elsewhere. Their names are
            // resolved.
            self.walk(|walk| walk.member_values(member.block()));
            let mut names = member.member_key().into_iter().flat_map(|k| k.names());
            let single = match (names.next(), names.next()) {
                (Some(first), None) => tree::name_text(&first).map(|field| (first, field)),
                _ => None,
            };
            let Some((first, field)) = single else {
                if let Some(value) = member.value() {
                    self.walk(|walk| walk.expr(&value));
                }
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
            if filled.as_deref() == Some(field.as_str())
                && let Some((condition, key_span)) = &fills
            {
                self.out.push(
                    Diagnostic::new(
                        &rule::FAILURE_DUPLICATE_CONTEXT_FIELD,
                        format!("`{field}` is filled in by `{condition}`"),
                    )
                    .at(span, "given as well")
                    .also(key_span.clone(), "fills it")
                    .reason(format!(
                        "A failure mapped from `json.read`'s `mismatch` receives the first problem in its `{field}: DecodeProblem` field, so the log and the client always get the problem the document had, never a value written here."
                    ))
                    .next(format!("Remove `{field}: ...` from the block")),
                );
                if let Some(value) = value {
                    self.infer(&value);
                }
                continue;
            }
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
                let expected = self.table.field_type(failure, &[field]);
                self.check(&value, &expected);
            } else {
                self.check(&value, &declared.declared.clone());
            }
        }
        let missing: Vec<String> = fields
            .iter()
            .filter(|f| !given.contains_key(&f.name) && filled.as_deref() != Some(f.name.as_str()))
            .map(|f| format!("`{}`", f.name))
            .collect();
        if missing.is_empty() {
            return;
        }
        // A `DecodeProblem` only ever comes from `json.read`'s `mismatch`
        // (area 19), so a failure that declares one is raised nowhere else.
        let unfilled = fields
            .iter()
            .find(|f| is_problem_field(f) && !given.contains_key(&f.name));
        let first = missing[0].trim_matches('`').to_string();
        let next = match (unfilled, record) {
            (Some(_), _) => format!(
                "Map `json.read`'s `mismatch` to `{failure}` on its own, or raise a failure that doesn't declare `problem` here"
            ),
            (None, Some(_)) => format!("Add `{first}: ...` to the block"),
            (None, None) => format!("Give its fields in a block: `{failure} {{ {first}: ... }}`"),
        };
        let reason = match unfilled {
            Some(field) => format!(
                "`{failure}` declares `{}: DecodeProblem`, which only a failure mapped from `json.read`'s `mismatch` receives, and nothing here fills it.",
                field.name
            ),
            None => "A failure is raised with every field its `client` and `internal` blocks declare, so the client and the log always get them.".to_string(),
        };
        self.out.push(
            Diagnostic::new(
                &rule::FAILURE_MISSING_CONTEXT_FIELD,
                format!("`{failure}` is missing {}", crate::and_list(&missing)),
            )
            .at(head_span, "fields missing")
            .reason(reason)
            .next(next),
        );
    }

    // --- attempt ----------------------------------------------------------

    /// The arms of an `attempt`'s block, with their failures' names
    /// resolved.
    fn arms(&mut self, attempt: &ast::AttemptExpr) -> Vec<Arm> {
        let mut arms = Vec::new();
        for item in attempt
            .member_list()
            .iter()
            .flat_map(ast::MemberList::member_items)
        {
            let MemberItem::Member(member) = item else {
                continue;
            };
            let value = member.value();
            if let Some(value) = &value {
                // A call written as the failure is resolved where the
                // mapping's value is checked, below.
                self.resolved_by(|names, out| {
                    names.mapped(value, out);
                });
            }
            let Some(key) = member.member_key() else {
                continue;
            };
            arms.push(Arm {
                key: tree::written(key.syntax()),
                key_span: self.span(&key),
                member,
                value,
            });
        }
        arms
    }

    /// `attempt call(...)`, with any block of mappings after it; the call's
    /// result.
    pub(super) fn attempt(&mut self, attempt: &ast::AttemptExpr, expected: Option<&Ty>) -> Ty {
        // `attempt Todo.update(todo) { ... }`: the block is the write's
        // own (area 6), not mappings.
        let write = matches!(attempt.expr(), Some(Expr::CallExpr(call)) if self.write_call(&call).is_some());
        let arms = if write {
            Vec::new()
        } else {
            self.arms(attempt)
        };
        let span = line_span(self.path, attempt.syntax());
        let Some(inner) = attempt.expr() else {
            return Ty::Error;
        };
        let (ty, fallible) = match &inner {
            Expr::CallExpr(call) => {
                self.handled.push(node_key(call.syntax()));
                if write {
                    self.pending_block = attempt.member_list();
                }
                let ty = self.call(call, expected);
                self.pending_block = None;
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
            // Operators on amounts whose currencies are known only while
            // running (area 19): the attempt covers every one of them in
            // the operator chain it wraps, `a + b + c` included.
            Expr::BinExpr(_) | Expr::ParenExpr(_) => {
                let covered = Self::operator_chain(&inner);
                let depth = self.handled.len();
                self.handled.extend(covered.iter().copied());
                let ty = self.infer(&inner);
                self.handled.truncate(depth);
                let fallible = if covered
                    .iter()
                    .any(|key| self.facts.fallible_ops.contains(key))
                {
                    Fallible::Construction(vec![
                        jadpo_std::RuleKey::CurrencyMismatch.word().to_string(),
                    ])
                } else {
                    Fallible::Never
                };
                (ty, fallible)
            }
            other => (self.infer(other), Fallible::Never),
        };
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
                for arm in &arms {
                    if let Some(value) = &arm.value {
                        self.failure_given(value);
                    }
                }
                self.attempt_infallible(attempt, &inner, &written);
            }
            Fallible::Unknown => {
                self.flow.open = true;
                // A construction, `text.parse` or `json.read`, whose type
                // is already reported as wrong, maps rules; nothing more is
                // known.
                let builds = matches!(&inner, Expr::CallExpr(call) if matches!(
                    self.facts.reaches.get(&node_key(call.syntax())),
                    Some(Reach::Construct(_) | Reach::Std(jadpo_std::Op::TextParse | jadpo_std::Op::JsonRead))
                ));
                let reads = matches!(&inner, Expr::CallExpr(call) if matches!(
                    self.facts.reaches.get(&node_key(call.syntax())),
                    Some(Reach::Std(jadpo_std::Op::JsonRead))
                ));
                for arm in &arms {
                    // What a mistyped `json.read` would have filled isn't
                    // reported as missing too.
                    let fills = (reads && (arm.key == "mismatch" || arm.key == "invalid"))
                        .then(|| (arm.key.as_str(), arm.key_span.clone()));
                    let failure = arm
                        .value
                        .as_ref()
                        .and_then(|v| self.failure_given_from(v, fills));
                    if arm.key == "invalid" && builds {
                        continue;
                    }
                    if arm.key == "invalid" {
                        self.invalid_outside_construction(arm, &inner, &written);
                    } else if let Some(name) = failure {
                        self.leave(&name, span.clone(), "mapped here");
                    }
                }
            }
            Fallible::Fails(names) => {
                for name in names {
                    self.leave(name, span.clone(), "passed on here");
                }
                for arm in &arms {
                    if let Some(value) = &arm.value {
                        self.failure_given(value);
                    }
                    if arm.key == "invalid" {
                        self.invalid_outside_construction(arm, &inner, &written);
                    } else {
                        self.unknown_mapping(arm, &written, None);
                    }
                }
            }
            Fallible::Construction(breakable) => {
                let rules: Vec<(String, String)> = match &inner {
                    Expr::CallExpr(call) => self
                        .construction_rules(call)
                        .into_iter()
                        .map(|(key, written)| (key.to_string(), written))
                        .collect(),
                    Expr::BinExpr(_) | Expr::ParenExpr(_) => {
                        let word = jadpo_std::RuleKey::CurrencyMismatch.word();
                        vec![(word.to_string(), word.to_string())]
                    }
                    // An object literal's rules are those across its fields
                    // (area 2, "Rules across fields").
                    Expr::RecordExpr(record) => record
                        .expr()
                        .and_then(|head| dotted(&head))
                        .map(|(first, _)| self.across_rules(&text(&first)))
                        .unwrap_or_default(),
                    _ => Vec::new(),
                };
                let proofs = match &inner {
                    Expr::CallExpr(call) => self
                        .facts
                        .proofs
                        .get(&node_key(call.syntax()))
                        .cloned()
                        .unwrap_or_default(),
                    Expr::RecordExpr(record) => self
                        .facts
                        .proofs
                        .get(&node_key(record.syntax()))
                        .cloned()
                        .unwrap_or_default(),
                    _ => Vec::new(),
                };
                let mapped = self
                    .construction_arms(attempt, &arms, breakable, &rules, &proofs, &written, span);
                if !mapped.is_empty() {
                    self.facts.mapped.insert(node_key(attempt.syntax()), mapped);
                }
            }
        }
        ty
    }

    /// Whether an operator in the chain `expr` is on two amounts of money.
    fn money_operator(&self, expr: &Expr) -> bool {
        Self::operator_chain(expr).iter().any(|key| {
            self.facts.types.get(key)
                == Some(&super::super::ty::Ty::Prim(super::super::ty::Prim::Money))
                || self.facts.proofs.contains_key(key)
        })
    }

    /// The operators an `attempt` around `expr` covers: it, and those its
    /// operands are, through brackets, but not inside a call.
    fn operator_chain(expr: &Expr) -> Vec<super::facts::NodeKey> {
        let mut out = Vec::new();
        let mut stack = vec![expr.clone()];
        while let Some(next) = stack.pop() {
            match next {
                Expr::ParenExpr(paren) => stack.extend(paren.expr()),
                Expr::BinExpr(binary) => {
                    out.push(node_key(binary.syntax()));
                    stack.extend(binary.lhs());
                    stack.extend(binary.rhs());
                }
                _ => {}
            }
        }
        out
    }

    /// The rules of the type a construction builds, by key and as written,
    /// or the conditions a `temporal` operation fails on, each written as
    /// the option that settles it (area 10).
    fn construction_rules(&self, call: &ast::CallExpr) -> Vec<(&'static str, String)> {
        match self.facts.reaches.get(&node_key(call.syntax())) {
            Some(Reach::Construct(target)) => self.rules_of(target),
            Some(Reach::Std(jadpo_std::Op::TextParse)) => self.parse_rules(call),
            Some(Reach::Std(op)) => op
                .operation()
                .conditions
                .iter()
                .map(|condition| {
                    let written = condition
                        .option
                        .map_or_else(|| condition.key.word().to_string(), |o| format!("{o}:"));
                    (condition.key.word(), written)
                })
                .collect(),
            _ => Vec::new(),
        }
    }

    /// "`max: 1000`", or "`min: 1` and `max: 1000`": the rules a
    /// construction can break, as its type writes them.
    pub(super) fn written_rules(&self, call: &ast::CallExpr, breakable: &[String]) -> String {
        let rules = self.construction_rules(call);
        let listed: Vec<String> = breakable
            .iter()
            .map(|key| {
                rules
                    .iter()
                    .find(|(k, _)| k == key)
                    .map_or_else(|| format!("`{key}`"), |(_, written)| format!("`{written}`"))
            })
            .collect();
        crate::and_list(&listed)
    }

    /// An entity write with a block (area 6), or an object being built.
    fn record_fallible(&self, record: &ast::RecordExpr) -> Fallible {
        if let Some(verdict) = self.facts.constructions.get(&node_key(record.syntax())) {
            return verdict.clone();
        }
        if let Some(write) = self.facts.writes.get(&node_key(record.syntax())) {
            return write.fallible.clone();
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
        // The comparisons that prove a construction, when ranges alone
        // don't (area 2, "Conditions as proof").
        let by: Vec<Origin> = match inner {
            Expr::CallExpr(_) | Expr::RecordExpr(_) => self
                .facts
                .proofs
                .get(&node_key(inner.syntax()))
                .into_iter()
                .flatten()
                .flat_map(|(_, by)| by.iter().cloned())
                .collect(),
            _ => Vec::new(),
        };
        let writes = match inner {
            Expr::CallExpr(call) => self.facts.writes.contains_key(&node_key(call.syntax())),
            Expr::RecordExpr(record) => self.facts.writes.contains_key(&node_key(record.syntax())),
            _ => false,
        };
        let reason = match inner {
            _ if writes => format!(
                "Nothing `{written}` writes can break a unique rule, be refused by a reference or name a record that isn't there, so it never fails; `attempt` marks only what can."
            ),
            Expr::CallExpr(_) | Expr::RecordExpr(_) if !by.is_empty() => format!(
                "The declared ranges and {} prove `{written}` always follows its type's rules, so building it never fails; `attempt` marks only what can.",
                conditions_named(&by)
            ),
            // An object whose rules across its fields the declared ranges
            // or its literals prove (area 2, "Rules across fields").
            Expr::RecordExpr(record)
                if record
                    .expr()
                    .and_then(|head| dotted(&head))
                    .is_some_and(|(first, _)| !self.across_rules(&text(&first)).is_empty()) =>
            {
                format!(
                    "The values given prove `{written}` always follows its type's rules, so building it never fails; `attempt` marks only what can."
                )
            }
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
            Expr::BinExpr(_) | Expr::ParenExpr(_) if self.money_operator(inner) => format!(
                "The amounts in `{written}` are known to be in one currency, by their types, how they were made or a condition around it, so it never fails with `currency_mismatch`; `attempt` marks only what can."
            ),
            _ => "`attempt` marks a call that can fail, and this isn't a call.".to_string(),
        };
        let mut problem = Diagnostic::new(
            &rule::FAILURE_ATTEMPT_INFALLIBLE,
            format!("`{written}` can't fail, so it takes no `attempt`"),
        )
        .at(span.clone(), "nothing to pass on");
        problem = self.checked_here(problem, &by);
        problem = problem
            .reason(reason)
            .next("Remove `attempt`")
            .edit(Span::new(self.path, start, inner_start), "");
        // A write's block stays: without `attempt` it is the write's.
        if end > inner_end && !writes {
            problem = problem.edit(Span::new(self.path, inner_end, end), "");
        }
        self.flow.open = true;
        if let Expr::CallExpr(call) = inner
            && let Some(callee) = self.callee_name(call)
        {
            self.deferred.push((self.out.len(), callee));
        }
        self.out.push(problem);
    }

    /// The name of the function or action a call reaches, as
    /// [`super::FailureSets`] names it: `library.function` for a
    /// library's.
    pub(super) fn callee_name(&self, call: &ast::CallExpr) -> Option<String> {
        match self.facts.reaches.get(&node_key(call.syntax()))? {
            Reach::Callable { name, .. } => Some(name.clone()),
            Reach::Library {
                library, function, ..
            } => Some(format!("{library}.{function}")),
            _ => None,
        }
    }

    /// Points at each comparison that proves a construction.
    fn checked_here(&self, mut problem: Diagnostic, by: &[Origin]) -> Diagnostic {
        let mut keys: Vec<_> = by.iter().map(|origin| origin.key).collect();
        keys.sort_unstable();
        keys.dedup();
        for (start, end, _) in keys {
            problem = problem.also(Span::new(self.path, start, end), "checked here");
        }
        problem
    }

    /// Whether a call is to a standard-library operation.
    fn is_standard(&self, call: &ast::CallExpr) -> bool {
        matches!(
            self.facts.reaches.get(&node_key(call.syntax())),
            Some(Reach::Std(op)) if *op != jadpo_std::Op::TextParse
        )
    }

    /// Why `text.parse` can fail as it can (area 19): the text may not be
    /// the type at all, and nothing proves the value follows the rules
    /// written.
    fn parse_reason(
        &self,
        call: &ast::CallExpr,
        breakable: &[String],
        tail: &str,
    ) -> Option<String> {
        let target = self.facts.parsed.get(&node_key(call.syntax()))?;
        let malformed = breakable.iter().any(|key| key == "malformed");
        let rules: Vec<String> = breakable
            .iter()
            .filter(|key| *key != "malformed")
            .cloned()
            .collect();
        let mut said = Vec::new();
        if malformed {
            said.push(format!(
                "the text is only known when the program runs, and may not be a `{target}` at all (`malformed`)"
            ));
        }
        if !rules.is_empty() {
            said.push(format!(
                "nothing proves the value read follows {}",
                self.written_rules(call, &rules)
            ));
        }
        Some(format!("{}{tail}", capital_first(&said.join(", and "))))
    }

    /// The arms of a construction that can fail (area 2, "Construction
    /// failures by rule"): each rule that can break has one, or `invalid:`
    /// covers every rule without its own; an arm for a rule that can't
    /// break, or an `invalid:` left covering nothing, is a mistake. Each
    /// failure is `invalid_value`. The failure each rule maps to, by key,
    /// `None` for `invalid:`. `proofs` are the rules only a condition
    /// proves, with the comparisons that do.
    #[allow(clippy::too_many_arguments)]
    fn construction_arms(
        &mut self,
        attempt: &ast::AttemptExpr,
        arms: &[Arm],
        breakable: &[String],
        rules: &[(String, String)],
        proofs: &[(String, Vec<Origin>)],
        written: &str,
        span: Span,
    ) -> Vec<(Option<String>, String)> {
        let mut seen: BTreeMap<String, Span> = BTreeMap::new();
        let mut mapped = Vec::new();
        let mut invalid: Option<&Arm> = None;
        // A misspelt or repeated key is reported on its own: what it was
        // meant to cover is unclear, so coverage isn't judged.
        let mut unsure = false;
        // `json.read`'s `mismatch` fills a `problem: DecodeProblem` field
        // of the failure it maps to (area 19, "Data formats"): its own
        // arm's, or `invalid:`'s when that covers `mismatch` alone.
        // `crypto.verify_jwt`'s `mismatch` too, its claims read as
        // `json.read` reads them (chunk 5.5r4).
        let reads = matches!(attempt.expr(), Some(Expr::CallExpr(call)) if matches!(
            self.facts.reaches.get(&node_key(call.syntax())),
            Some(Reach::Std(jadpo_std::Op::JsonRead | jadpo_std::Op::CryptoVerifyJwt))
        ));
        let own: Vec<&str> = arms
            .iter()
            .filter(|arm| arm.key != "invalid")
            .map(|arm| arm.key.as_str())
            .collect();
        let rest: Vec<&String> = breakable
            .iter()
            .filter(|key| !own.contains(&key.as_str()))
            .collect();
        let invalid_fills = !rest.is_empty() && rest.iter().all(|key| *key == "mismatch");
        for arm in arms {
            let fills = (reads
                && (arm.key == "mismatch" || (arm.key == "invalid" && invalid_fills)))
                .then(|| (arm.key.as_str(), arm.key_span.clone()));
            let filling = fills.is_some();
            let failure = arm
                .value
                .as_ref()
                .and_then(|v| self.failure_given_from(v, fills));
            if let Some(failure) = &failure
                && filling
                && self.receives_problem(failure)
            {
                self.facts
                    .receives
                    .insert((node_key(attempt.syntax()), arm.key.clone()));
            }
            if let Some(failure) = &failure {
                self.leave(failure, span.clone(), "mapped here");
                self.mapping_kind(arm, failure, "invalid_value");
            } else {
                self.flow.open = true;
            }
            if let Some(first) = seen.get(&arm.key) {
                unsure = true;
                self.duplicate_mapping(arm, first.clone());
                continue;
            }
            seen.insert(arm.key.clone(), arm.key_span.clone());
            if arm.key == "invalid" {
                invalid = Some(arm);
            } else if let Some((_, rule)) = rules.iter().find(|(k, _)| *k == arm.key) {
                if !breakable.contains(&arm.key) {
                    let by = proofs
                        .iter()
                        .find(|(key, _)| *key == arm.key)
                        .map(|(_, by)| by.as_slice())
                        .unwrap_or_default();
                    let settled = match attempt.expr() {
                        Some(Expr::CallExpr(call)) if self.is_standard(&call) => {
                            Some(Self::arguments(&call).iter().any(|arg| {
                                Self::arg_name(arg).as_deref() == rule.strip_suffix(':')
                            }))
                        }
                        _ => None,
                    };
                    match settled {
                        Some(given) => self.condition_cannot_arise(arm, given, written),
                        None => self.rule_cannot_break(arm, rule, written, by),
                    }
                    continue;
                }
            } else {
                unsure = true;
                self.unknown_mapping(arm, written, Some(rules));
                continue;
            }
            if let Some(failure) = failure {
                let key = (arm.key != "invalid").then(|| arm.key.clone());
                mapped.push((key, failure));
            }
        }
        if unsure {
            self.flow.open = true;
            return mapped;
        }
        let uncovered: Vec<String> = breakable
            .iter()
            .filter(|key| !seen.contains_key(*key))
            .cloned()
            .collect();
        match invalid {
            Some(arm) if uncovered.is_empty() => {
                self.invalid_covers_nothing(arm, breakable, written);
            }
            Some(_) => {}
            None if uncovered.is_empty() => {}
            None => {
                self.rule_unmapped(attempt, &uncovered, rules, written, span);
            }
        }
        mapped
    }

    /// The edit that removes an arm's line from its block.
    fn arm_removal(&self, arm: &Arm) -> Span {
        let node = arm.member.syntax();
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
        Span::new(self.path, start, end)
    }

    fn rule_unmapped(
        &mut self,
        attempt: &ast::AttemptExpr,
        uncovered: &[String],
        rules: &[(String, String)],
        written: &str,
        span: Span,
    ) {
        self.flow.open = true;
        let as_written: Vec<String> = uncovered
            .iter()
            .map(|key| {
                rules
                    .iter()
                    .find(|(k, _)| k == key)
                    .map_or_else(|| format!("`{key}`"), |(_, rule)| format!("`{rule}`"))
            })
            .collect();
        let first = &uncovered[0];
        let (them, it) = if uncovered.len() == 1 {
            ("it", "it")
        } else {
            ("them", "each")
        };
        let standard =
            matches!(attempt.expr(), Some(Expr::CallExpr(call)) if self.is_standard(&call));
        let next = if attempt.member_list().is_some() {
            format!(
                "Add `{first}: SomeFailure` to the block, naming an `invalid_value` failure, or `invalid: SomeFailure` to cover every {} without an arm",
                if standard { "condition" } else { "rule" }
            )
        } else {
            format!(
                "Add a block after `{written}`: `{{ {first}: SomeFailure }}`, naming an `invalid_value` failure, or `{{ invalid: SomeFailure }}` to cover every rule"
            )
        };
        self.out.push(
            Diagnostic::new(
                &rule::FAILURE_RULE_UNMAPPED,
                format!(
                    "`{written}` can {} {}, and nothing maps {them}",
                    if standard { "fail on" } else { "break" },
                    keys_list(uncovered)
                ),
            )
            .at(span, "can fail at runtime")
            .reason(if standard {
                format!(
                    "{}; {it} needs an arm naming the `invalid_value` failure the caller gets, or `invalid:` covers every condition without its own.",
                    condition_clause(uncovered)
                )
            } else {
                let tail = format!(
                    "; {it} needs an arm naming the `invalid_value` failure the caller gets, or `invalid:` covers every rule without its own."
                );
                match attempt.expr() {
                    Some(Expr::CallExpr(call)) => self.parse_reason(&call, uncovered, &tail),
                    _ => None,
                }
                .unwrap_or_else(|| format!(
                    "The value is only known when the program runs, and neither the declared ranges nor a condition before it prove it follows {}{tail}",
                    crate::and_list(&as_written)
                ))
            })
            .next(next),
        );
    }

    /// An arm for a rule that can't break: `by` are the comparisons that
    /// prove it, when the declared ranges alone don't.
    /// An arm for a `temporal` condition the call can't fail on: its
    /// option is given (`given`), or means nothing for its value.
    fn condition_cannot_arise(&mut self, arm: &Arm, given: bool, written: &str) {
        let key = &arm.key;
        let reason = if given {
            format!(
                "The call gives `{key}:` as one of its values other than `reject`, which settles the condition, so this arm could never run."
            )
        } else {
            format!(
                "`{key}` arises only for a local time, and `{written}` here moves a date, which has no clock time, so this arm could never run."
            )
        };
        let removal = self.arm_removal(arm);
        self.out.push(
            Diagnostic::new(
                &rule::FAILURE_RULE_CANNOT_BREAK,
                format!("`{written}` can't fail on `{key}`, so it takes no arm"),
            )
            .at(arm.key_span.clone(), "never runs")
            .reason(reason)
            .next(format!("Remove `{key}: ...`"))
            .edit(removal, ""),
        );
    }

    fn rule_cannot_break(&mut self, arm: &Arm, rule: &str, written: &str, by: &[Origin]) {
        let key = &arm.key;
        let reason = if key == "scale" {
            format!(
                "`{rule}` decides how a value is kept, rounding it half to even, so it never breaks and this arm could never run."
            )
        } else if matches!(key.as_str(), "pattern" | "format" | "version") {
            format!(
                "The value's own type already follows `{rule}`, so it never breaks and this arm could never run."
            )
        } else if !by.is_empty() {
            format!(
                "The declared ranges and {} prove the value always follows `{rule}`, so this arm could never run.",
                conditions_named(by)
            )
        } else {
            format!(
                "The declared ranges prove the value always follows `{rule}`, so this arm could never run."
            )
        };
        let removal = self.arm_removal(arm);
        let problem = Diagnostic::new(
            &rule::FAILURE_RULE_CANNOT_BREAK,
            format!("`{written}` can't break `{key}`, so it takes no arm"),
        )
        .at(arm.key_span.clone(), "never runs");
        let problem = self.checked_here(problem, by);
        self.out.push(
            problem
                .reason(reason)
                .next(format!("Remove `{key}: ...`"))
                .edit(removal, ""),
        );
    }

    fn invalid_covers_nothing(&mut self, arm: &Arm, breakable: &[String], written: &str) {
        let removal = self.arm_removal(arm);
        self.out.push(
            Diagnostic::new(
                &rule::FAILURE_INVALID_COVERS_NOTHING,
                "`invalid:` covers nothing here",
            )
            .at(arm.key_span.clone(), "no rule left to cover")
            .reason(format!(
                "Every rule `{written}` can break has its own arm ({}), so `invalid:` could never run.",
                keys_list(breakable)
            ))
            .next("Remove `invalid: ...`")
            .edit(removal, ""),
        );
    }

    pub(super) fn duplicate_mapping(&mut self, arm: &Arm, first: Span) {
        let key = &arm.key;
        let removal = self.arm_removal(arm);
        self.flow.open = true;
        self.out.push(
            Diagnostic::new(
                &rule::FAILURE_DUPLICATE_MAPPING,
                format!("`{key}:` is mapped twice"),
            )
            .at(arm.key_span.clone(), "mapped again")
            .also(first, "first mapped here")
            .reason("Each condition is mapped once, so there is never a question of which failure the caller gets.")
            .next(format!("Remove one `{key}:`"))
            .edit(removal, ""),
        );
    }

    /// `invalid:` on something that isn't a construction: it covers only a
    /// construction's rules, never another operation's conditions.
    fn invalid_outside_construction(&mut self, arm: &Arm, inner: &Expr, written: &str) {
        let entity = matches!(inner, Expr::CallExpr(call) if matches!(
            self.facts.reaches.get(&node_key(call.syntax())),
            Some(Reach::Entity { .. })
        ));
        let (reason, next) = if entity {
            (
                format!(
                    "`{written}` is an entity's operation, which maps each of its own conditions, such as `missing:` or `conflict:`, with an arm; `invalid:` is the catch-all only for the rules a construction's value can break."
                ),
                "Remove `invalid: ...`, and map each condition the operation has with its own arm"
                    .to_string(),
            )
        } else {
            (
                format!(
                    "`{written}` fails with the failures its `fails` list names; `invalid:` is the catch-all only for the rules a construction's value can break, never for another call's failures."
                ),
                format!(
                    "Remove `invalid: ...`; `attempt` passes `{written}`'s failures on, and `match` turns one into another"
                ),
            )
        };
        let removal = self.arm_removal(arm);
        self.flow.open = true;
        self.out.push(
            Diagnostic::new(
                &rule::FAILURE_INVALID_OUTSIDE_CONSTRUCTION,
                format!("`{written}` isn't a construction, so `invalid:` covers nothing"),
            )
            .at(arm.key_span.clone(), "not a construction")
            .reason(reason)
            .next(next)
            .edit(removal, ""),
        );
    }

    /// The failure a mapping names must have the kind its condition needs
    /// (area 4, question 7).
    pub(super) fn mapping_kind(&mut self, arm: &Arm, failure: &str, kind: &str) {
        let condition = arm.key.as_str();
        let Some(info) = self.failures.failure_info(failure) else {
            return;
        };
        let Some(actual) = info.kind.clone() else {
            return;
        };
        if actual == kind {
            return;
        }
        let span = arm
            .value
            .as_ref()
            .map_or_else(|| arm.key_span.clone(), |value| self.span(value));
        let a = if kind.starts_with(['a', 'e', 'i', 'o', 'u']) {
            "an"
        } else {
            "a"
        };
        let meaning = match kind {
            "conflict" => "a write the store refuses because of the records already there",
            "not_found" => "a read that finds nothing",
            _ => "a value that breaks a type's rules",
        };
        self.out.push(
            Diagnostic::new(
                &rule::FAILURE_MAPPING_KIND,
                format!("`{condition}:` needs {a} `{kind}` failure, and `{failure}` is `{actual}`"),
            )
            .at(span, format!("kind `{actual}`"))
            .reason(format!(
                "Each mapping's condition fixes the kind of failure it becomes: {meaning} is `{kind}`, so the client gets the status that means."
            ))
            .next(format!(
                "Name {a} `{kind}` failure here, or make `{failure}`'s kind `FailureKind.{kind}`"
            )),
        );
    }

    /// An arm whose key isn't a condition the operation has: for a
    /// construction, `rules` are its type's rules.
    fn unknown_mapping(&mut self, arm: &Arm, written: &str, rules: Option<&[(String, String)]>) {
        let condition = &arm.key;
        let span = arm.key_span.clone();
        self.flow.open = true;
        let problem = Diagnostic::new(
            &rule::FAILURE_UNKNOWN_MAPPING,
            format!("`{written}` has no `{condition}:` condition to map"),
        )
        .at(span.clone(), "not a condition here");
        let problem = match rules {
            Some(rules) => {
                let keys: Vec<String> = rules.iter().map(|(k, _)| k.clone()).collect();
                let near = nearest(condition, keys.iter().map(String::as_str));
                let problem = problem.reason(format!(
                    "A construction's arms are the rules of its type and the types it builds on; `{written}`'s are {}, and `invalid:` covers any without an arm of its own.{}",
                    keys_list(&keys),
                    did_you_mean(near)
                ));
                match near {
                    Some(near) => problem
                        .next(format!("Use `{near}:` instead of `{condition}:`"))
                        .edit(span, near),
                    None => problem.next(format!(
                        "Remove `{condition}: ...`, or map one of {} instead",
                        keys_list(&keys)
                    )),
                }
            }
            None => problem
                .reason(format!(
                    "`{written}` fails with the failures its `fails` list names; `attempt` passes them on as they are, and `match` turns one into another."
                ))
                .next(format!(
                    "Remove `{condition}: ...`, and `match` on the call to map its failures"
                )),
        };
        self.out.push(problem);
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
            self.undeclared = true;
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

/// Whether `field` is the one a failure mapped from `json.read`'s
/// `mismatch` receives (area 19, "Data formats"): `problem:
/// DecodeProblem`.
fn is_problem_field(field: &FieldDef) -> bool {
    field.name == jadpo_std::http::DECODE_PROBLEM_FIELD
        && matches!(
            &field.declared,
            Ty::Opaque(name, args) if name == jadpo_std::http::DECODE_PROBLEM && args.is_empty()
        )
}
