//! A `match` on a call's outcome (areas 3 and 4): its subject is a call
//! that can fail, one arm takes the success value, and every failure the
//! call can give has an arm of its own, with no `_`. A failure arm
//! recovers with a value, maps with `reject` or passes the failure on with
//! `propagate`.

use std::collections::BTreeMap;

use jadpo_diag::{Diagnostic, Span, nearest, rule};
use jadpo_syntax::ast::{self, AstNode, Expr, Pat};

use super::super::ty::Ty;
use super::facts::PatternFact;
use super::failing::Fallible;
use super::matching::Mode;
use super::{Arm, Body, LocalKind, did_you_mean, line_span, node_key, text};
use crate::case::starts_upper;
use crate::tree::{self, dotted};

/// What one arm's pattern is.
enum Pattern {
    Success(Option<ast::Name>, Option<Span>),
    Failure(String, Option<ast::Name>, Option<Span>),
    Wildcard,
    /// A pattern for values, or a name that isn't a failure.
    Other,
    /// A name resolution already reports.
    Unknown,
}

impl Body<'_> {
    pub(super) fn outcome_match(
        &mut self,
        node: &ast::MatchExpr,
        mode: Mode<'_>,
        arms: &[ast::MatchArm],
        head: Span,
    ) -> (Ty, bool) {
        // `match (risky(n))`: the brackets only group the call.
        let mut subject = node.expr();
        while let Some(Expr::ParenExpr(paren)) = &subject {
            subject = paren.expr();
        }
        let written = subject
            .as_ref()
            .map(|e| match e {
                Expr::CallExpr(call) => call
                    .expr()
                    .map(|callee| format!("{}(...)", tree::written(callee.syntax())))
                    .unwrap_or_default(),
                other => tree::written(other.syntax()),
            })
            .unwrap_or_default();
        self.facts.outcomes.insert(node_key(node.syntax()));
        let (success, fallible) = match &subject {
            Some(Expr::CallExpr(call)) => {
                self.handled.push(node_key(call.syntax()));
                let ty = self.call(call, None);
                self.handled.pop();
                (ty, self.call_fallible(call))
            }
            Some(Expr::RecordExpr(record)) if self.is_write(record) => (
                self.infer(&Expr::RecordExpr(record.clone())),
                Fallible::Unknown,
            ),
            Some(other) => {
                let ty = self.infer(other);
                if !ty.has_error() {
                    self.out.push(
                        Diagnostic::new(
                            &rule::FAILURE_OUTCOME_SUBJECT_REQUIRED,
                            format!("`{written}` isn't a call, so it has no outcome to match"),
                        )
                        .at(head.clone(), "not a call")
                        .reason("`success(...)` and failure arms match the outcome of a call that can fail; a value is matched by its own cases, such as an enum's variants.")
                        .next(format!("Match `{written}`'s cases, or match on the call that can fail")),
                    );
                }
                (Ty::Error, Fallible::Unknown)
            }
            None => (Ty::Error, Fallible::Unknown),
        };
        let (known, checked) = match &fallible {
            Fallible::Fails(names) => (
                names.iter().map(|name| self.named_here(name)).collect(),
                true,
            ),
            Fallible::Never => {
                if let Some(Expr::CallExpr(call)) = &subject
                    && let Some(callee) = self.callee_name(call)
                {
                    self.deferred.push((self.out.len(), callee));
                }
                self.out.push(
                    Diagnostic::new(
                        &rule::FAILURE_OUTCOME_INFALLIBLE,
                        format!("`{written}` can't fail, so it has no outcome to match"),
                    )
                    .at(head.clone(), "always succeeds")
                    .reason(format!(
                        "`{written}` lists no failures after `fails`, so it always gives its result and there is nothing for failure arms to handle."
                    ))
                    .next("Use the call's result directly"),
                );
                (Vec::new(), false)
            }
            Fallible::Construction(breakable) => {
                let first = breakable.first().cloned().unwrap_or_default();
                self.out.push(
                    Diagnostic::new(
                        &rule::FAILURE_OUTCOME_SUBJECT_REQUIRED,
                        format!("`{written}` has no failure to match yet"),
                    )
                    .at(head.clone(), "a construction")
                    .reason("A construction that can fail maps each rule it can break to a failure in its `attempt`'s block, and only a call's named failures can be matched.")
                    .next(format!(
                        "Write `attempt {written} {{ {first}: SomeFailure }}` in a `var` first, or match on a call"
                    )),
                );
                (Vec::new(), false)
            }
            Fallible::Unknown => (Vec::new(), false),
        };
        if !checked {
            self.flow.open = true;
        }
        let mut success_arm: Option<Span> = None;
        let mut handled: BTreeMap<String, Span> = BTreeMap::new();
        let mut wildcard = false;
        let mut completes = arms.is_empty();
        let mut value: Option<Ty> = None;
        let mut deferred: Vec<Expr> = Vec::new();
        // An arm whose name resolution refuses, such as a package's failure
        // without its version while two are held, leaves its failure
        // unknown, so the `match` isn't also told it misses one.
        let mut misread = false;
        for arm in arms {
            self.scopes.push(Vec::new());
            let refused = match arm.pat() {
                Some(Pat::CasePat(case)) => {
                    !self.resolved_by(|names, out| crate::resolve::pattern(names, &case, out))
                }
                _ => false,
            };
            misread |= refused;
            let pattern = match arm.pat() {
                Some(pat) if !refused => self.outcome_pattern(&pat),
                Some(Pat::CasePat(case)) => {
                    if let Some(name) = case.pat_binding().and_then(|b| b.name()) {
                        self.declare(Some(name), Ty::Error, LocalKind::Inline, None, "variable");
                    }
                    Pattern::Unknown
                }
                _ => Pattern::Unknown,
            };
            if let Some(pat) = arm.pat() {
                let fact = match &pattern {
                    Pattern::Success(..) => PatternFact::Success,
                    Pattern::Failure(failure, ..) => PatternFact::Failure(failure.clone()),
                    Pattern::Wildcard => PatternFact::Wildcard,
                    Pattern::Other | Pattern::Unknown => PatternFact::Unknown,
                };
                self.facts.patterns.insert(node_key(pat.syntax()), fact);
            }
            let span = arm
                .pat()
                .map_or_else(|| self.span(arm), |pat| self.span(&pat));
            let propagates = matches!(arm.expr(), Some(Expr::PropagateExpr(_)));
            let outer = self.arm.clone();
            match pattern {
                Pattern::Success(name, brackets) => {
                    if let Some(first) = &success_arm {
                        self.out.push(
                            Diagnostic::new(
                                &rule::FAILURE_OUTCOME_SUCCESS_DUPLICATE,
                                "The success arm is written twice",
                            )
                            .at(span.clone(), "matched again")
                            .also(first.clone(), "first matched here")
                            .reason("A call succeeds one way, so one `success(...)` arm takes its value; this one would never run.")
                            .next("Remove this arm, or merge its body into the first"),
                        );
                    } else {
                        success_arm = Some(span.clone());
                    }
                    if propagates {
                        let arm_span = line_span(self.path, arm.syntax());
                        self.out.push(
                            Diagnostic::new(
                                &rule::FAILURE_OUTCOME_SUCCESS_VALUE_REQUIRED,
                                "The success arm has no failure to pass on",
                            )
                            .at(arm_span, "`propagate` on success")
                            .reason("`propagate` passes a failure arm's failure on to the caller; the success arm has a value, which it gives, or a failure it `reject`s.")
                            .next("Give the success value, or `reject` a failure"),
                        );
                    }
                    if let (Some(name), Some(brackets)) = (name, brackets) {
                        self.declare(
                            Some(name),
                            success.clone(),
                            LocalKind::Binding,
                            Some(brackets),
                            "variable",
                        );
                        if propagates {
                            self.silence_last();
                        }
                    }
                    self.arm = Arm::Success;
                }
                Pattern::Failure(written_failure, name, brackets) => {
                    let failure = self.named_here(&self.symbols.canonical(&written_failure));
                    let mut armed = Some(written_failure.clone());
                    if let Some(first) = handled.get(&failure) {
                        self.out.push(
                            Diagnostic::new(
                                &rule::FAILURE_OUTCOME_DUPLICATE_ARM,
                                format!("`{failure}` is handled twice"),
                            )
                            .at(span.clone(), "matched again")
                            .also(first.clone(), "first matched here")
                            .reason(format!("Each failure has one arm; the first takes every `{failure}`, so this one never runs."))
                            .next("Remove this arm, or merge its body into the first"),
                        );
                        armed = None;
                    } else if checked && !known.contains(&failure) {
                        let near =
                            nearest(&failure, known.iter().map(String::as_str)).map(str::to_string);
                        let listed: Vec<String> = known.iter().map(|n| format!("`{n}`")).collect();
                        let problem = Diagnostic::new(
                            &rule::FAILURE_OUTCOME_UNKNOWN_ARM,
                            format!("`{written}` can't fail with `{failure}`"),
                        )
                        .at(span.clone(), "never happens here")
                        .reason(format!(
                            "`{written}` fails with {}, so an arm for anything else never runs.{}",
                            crate::and_list(&listed),
                            did_you_mean(near.as_deref())
                        ));
                        self.out.push(match near {
                            Some(near) if !handled.contains_key(&near) => problem
                                .next(format!("Use `{near}` instead of `{failure}`"))
                                .edit(span.clone(), near),
                            _ => problem.next("Remove this arm"),
                        });
                        armed = None;
                        handled.insert(failure.clone(), span.clone());
                    } else {
                        handled.insert(failure.clone(), span.clone());
                    }
                    if let (Some(name), Some(brackets)) = (name, brackets) {
                        let ty = Ty::Named(written_failure.clone());
                        self.declare(
                            Some(name),
                            ty,
                            LocalKind::Binding,
                            Some(brackets),
                            "variable",
                        );
                    }
                    self.arm = Arm::Failure(armed);
                }
                Pattern::Wildcard => {
                    wildcard = true;
                    let listed: Vec<String> = known
                        .iter()
                        .filter(|n| !handled.contains_key(*n))
                        .map(|n| format!("`{n}`"))
                        .collect();
                    self.out.push(
                        Diagnostic::new(
                            &rule::FAILURE_OUTCOME_WILDCARD,
                            "`_` can't stand for a call's failures",
                        )
                        .at(span.clone(), "wildcard")
                        .reason("Every failure a call can give has an arm of its own, so a failure added to the call later is caught at every `match` that has to consider it.")
                        .next(if listed.is_empty() {
                            "Remove the `_` arm".to_string()
                        } else {
                            format!("Replace `_` with arms for {}", crate::and_list(&listed))
                        }),
                    );
                    self.arm = Arm::Failure(None);
                }
                Pattern::Other => {
                    let shown = arm
                        .pat()
                        .map(|pat| tree::written(pat.syntax()))
                        .unwrap_or_default();
                    self.out.push(
                        Diagnostic::new(
                            &rule::FAILURE_OUTCOME_PATTERN_INVALID,
                            format!("`{shown}` can't match a call's outcome"),
                        )
                        .at(span.clone(), "a pattern for values")
                        .reason(format!(
                            "A `match` on `{written}` has a `success(name)` arm for its value and an arm for each failure it can give; to look at the value's own cases, match on the success value."
                        ))
                        .next("Remove this arm, or match on the success value inside `success(...)`"),
                    );
                    self.arm = Arm::Failure(None);
                }
                Pattern::Unknown => {
                    self.arm = Arm::Failure(None);
                }
            }
            completes |= self.arm_body(arm, mode, &mut value, &mut deferred);
            self.arm = outer;
            self.pop_scope();
        }
        for literal in deferred {
            match &value {
                Some(ty) => self.check(&literal, &ty.clone()),
                None => {
                    value = Some(self.infer(&literal));
                }
            }
        }
        if checked && success_arm.is_none() {
            self.out.push(
                Diagnostic::new(
                    &rule::FAILURE_OUTCOME_SUCCESS_MISSING,
                    format!("This `match` doesn't handle `{written}` succeeding"),
                )
                .at(head.clone(), "no `success(...)` arm")
                .reason("A `match` on a call's outcome handles its value too, in a `success(name)` arm, as well as each failure.")
                .next("Add `success(value) -> ...` as the first arm"),
            );
        }
        if checked && !wildcard && !misread {
            let missing: Vec<String> = known
                .iter()
                .filter(|n| !handled.contains_key(*n))
                .map(|n| format!("`{n}`"))
                .collect();
            if !missing.is_empty() {
                let next = if missing.len() == 1 {
                    format!("Add an arm for {}", missing[0])
                } else {
                    format!("Add arms for {}", crate::and_list(&missing))
                };
                self.out.push(
                    Diagnostic::new(
                        &rule::FAILURE_OUTCOME_MISSING_ARM,
                        format!("This `match` doesn't handle {}", crate::and_list(&missing)),
                    )
                    .at(head, "failures not handled")
                    .reason(format!(
                        "`{written}` can fail with {}, and a `match` on its outcome has an arm for every one: a value, `reject` or `propagate`.",
                        crate::and_list(
                            &known.iter().map(|n| format!("`{n}`")).collect::<Vec<_>>()
                        )
                    ))
                    .next(next),
                );
            }
        }
        let ty = match mode {
            Mode::Value(Some(expected)) => expected.clone(),
            _ => value.unwrap_or(Ty::Error),
        };
        (ty, completes)
    }

    fn outcome_pattern(&mut self, pat: &Pat) -> Pattern {
        match pat {
            Pat::WildcardPat(_) => Pattern::Wildcard,
            Pat::LiteralPat(_) => Pattern::Other,
            Pat::CasePat(case) => {
                let path: Vec<String> = case
                    .path()
                    .map(|path| path.name_refs().map(|n| text(&n)).collect())
                    .unwrap_or_default();
                let binding = case.pat_binding();
                let brackets = binding.as_ref().map(|b| self.span(b));
                let name = binding.and_then(|b| b.name());
                match path.as_slice() {
                    [only] if only == "success" => Pattern::Success(name, brackets),
                    [only] if starts_upper(only) && self.is_failure(only) => {
                        Pattern::Failure(only.clone(), name, brackets)
                    }
                    // A package's failure, written through its name as
                    // `fails` writes it: `AcmePay.CardDeclined`, or
                    // `Postcodes.v3.InvalidPostcode` while two are held
                    // (chunk 7.8d).
                    _ if self.is_package_failure(&path) => {
                        Pattern::Failure(path.join("."), name, brackets)
                    }
                    [only]
                        if starts_upper(only)
                            && !self.table.contains(only)
                            && self.symbols.get(only).is_none() =>
                    {
                        if let Some(name) = name {
                            self.declare(
                                Some(name),
                                Ty::Error,
                                LocalKind::Inline,
                                None,
                                "variable",
                            );
                        }
                        Pattern::Unknown
                    }
                    _ => {
                        if let Some(name) = name {
                            self.declare(
                                Some(name),
                                Ty::Error,
                                LocalKind::Inline,
                                None,
                                "variable",
                            );
                        }
                        Pattern::Other
                    }
                }
            }
        }
    }

    /// `failure`, as the tables know it, named as this code writes it: a
    /// package's own code names its failures bare (`CardDeclined`), the
    /// app through the package (`AcmePay.CardDeclined`), so a failure has
    /// one name inside the code that matches on it.
    fn named_here(&self, failure: &str) -> String {
        match failure.rsplit_once('.') {
            Some((_, bare)) if self.symbols.canonical(bare) == failure => bare.to_string(),
            _ => failure.to_string(),
        }
    }

    /// Whether a block-taking expression is an entity's write (area 6).
    fn is_write(&self, record: &ast::RecordExpr) -> bool {
        match record.expr() {
            Some(Expr::CallExpr(_)) => true,
            Some(head) => dotted(&head).is_some_and(|(first, _)| {
                self.callable_kind(&text(&first)) == Some(jadpo_db::ItemKind::Entity)
            }),
            None => false,
        }
    }

    /// Marks the local declared last as read: a binding whose arm is
    /// already reported for what it does with it.
    fn silence_last(&mut self) {
        if let Some(local) = self.scopes.last_mut().and_then(|scope| scope.last_mut()) {
            local.used = true;
        }
    }
}
