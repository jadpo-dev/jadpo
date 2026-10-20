//! `match` (area 3): one construct for values and call outcomes, used as
//! a statement or as a value. The subject's type decides which patterns
//! apply: an enum's variants, with brackets naming a variant's data;
//! `true` and `false`; `some(name)` and `none`; literals and `_` for text
//! and numbers. Enums, `Bool` and nullable values list every case and
//! never use `_` (question 10).
//!
//! A `match` whose arms name `success(...)` or a failure is on a call's
//! outcome, checked in [`super::outcome`].

use std::collections::BTreeMap;

use jadpo_diag::{Diagnostic, Span, nearest, rule};
use jadpo_syntax::ast::{self, AstNode, Expr, Pat};

use super::super::rules::Const;
use super::super::shape::FieldsBase;
use super::super::table::Def;
use super::super::ty::{Prim, Ty};
use super::facts::PatternFact;
use super::{Body, LocalKind, capital, did_you_mean, is_literal, line_span, node_key, text};
use crate::case::starts_upper;
use crate::tree;

/// Where a `match` stands.
#[derive(Clone, Copy)]
pub(super) enum Mode<'e> {
    Statement,
    /// As a value, with the type its position requires, if known.
    Value(Option<&'e Ty>),
}

/// What is matched, by the patterns that apply to it.
#[derive(Clone, Debug)]
enum Subject {
    /// Already reported: every pattern is accepted.
    Error,
    Enum(String),
    Bool,
    /// A nullable value, with the type `some(...)` binds.
    Nullable(Ty),
    /// Text and numbers: literals, and `_` for the rest.
    Open(Ty),
    /// Anything else: only `_`.
    Other(Ty),
}

/// One case a pattern covers.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Case {
    Variant(String),
    True,
    False,
    Some,
    None,
    Literal(String),
}

impl Case {
    fn shown(&self, host: &str) -> String {
        match self {
            Case::Variant(name) => format!("`{host}.{name}`"),
            Case::True => "`true`".to_string(),
            Case::False => "`false`".to_string(),
            Case::Some => "`some(...)`".to_string(),
            Case::None => "`none`".to_string(),
            Case::Literal(text) => format!("`{text}`"),
        }
    }
}

/// What a pattern means for its arm.
struct Reading {
    case: Option<Case>,
    wildcard: bool,
    binding: Option<(ast::Name, Ty, Span)>,
}

impl Reading {
    fn none() -> Reading {
        Reading {
            case: None,
            wildcard: false,
            binding: None,
        }
    }
}

impl Body<'_> {
    /// A `match` standing as a statement; whether the code after it runs.
    pub(super) fn match_stmt(&mut self, subject: &ast::MatchExpr) -> bool {
        self.match_expr(subject, Mode::Statement).1
    }

    /// A `match` used as a value.
    pub(super) fn match_value(&mut self, subject: &ast::MatchExpr, expected: Option<&Ty>) -> Ty {
        self.match_expr(subject, Mode::Value(expected)).0
    }

    pub(super) fn is_failure(&self, name: &str) -> bool {
        matches!(
            self.table.def(name),
            Some(Def::Fields {
                base: FieldsBase::Failure,
                ..
            })
        ) || self
            .symbols
            .get(name)
            .is_some_and(|symbol| symbol.kind.is_failure())
    }

    pub(super) fn pattern_path(pat: &Pat) -> Option<Vec<ast::NameRef>> {
        match pat {
            Pat::CasePat(case) => Some(case.path()?.name_refs().collect()),
            _ => None,
        }
    }

    fn is_outcome(&self, arms: &[ast::MatchArm]) -> bool {
        arms.iter().any(|arm| {
            let Some(path) = arm.pat().as_ref().and_then(Self::pattern_path) else {
                return false;
            };
            match path.as_slice() {
                [only] => {
                    let name = text(only);
                    name == "success" || (starts_upper(&name) && self.is_failure(&name))
                }
                _ => false,
            }
        })
    }

    /// The type `some(name)` binds: the value without `none`.
    fn non_null(&self, ty: &Ty) -> Ty {
        match ty {
            Ty::Nullable(inner) => (**inner).clone(),
            Ty::Field(host, path) => match self.table.field(host, path) {
                Some(field) if self.table.is_nullable(&field.declared) => {
                    self.non_null(&field.declared)
                }
                _ => ty.clone(),
            },
            Ty::None => Ty::Error,
            other => other.clone(),
        }
    }

    fn subject(&mut self, expr: Option<Expr>) -> Subject {
        let ty = expr.map_or(Ty::Error, |expr| self.infer(&expr));
        if ty.has_error() {
            return Subject::Error;
        }
        if self.table.is_nullable(&ty) {
            return Subject::Nullable(self.non_null(&ty));
        }
        if let Ty::Named(name) = self.table.structure(&ty)
            && self.table.variants(&name).is_some()
        {
            return Subject::Enum(name);
        }
        match self.table.representation(&ty) {
            Some(Prim::Bool) => Subject::Bool,
            Some(Prim::Text | Prim::Int | Prim::Decimal) => Subject::Open(ty),
            _ => Subject::Other(ty),
        }
    }

    fn match_expr(&mut self, node: &ast::MatchExpr, mode: Mode<'_>) -> (Ty, bool) {
        let arms: Vec<ast::MatchArm> = node
            .match_arm_list()
            .into_iter()
            .flat_map(|list| list.match_arms())
            .collect();
        let outcome = self.is_outcome(&arms);
        let subject_expr = node.expr();
        let written = subject_expr
            .as_ref()
            .map(|e| tree::written(e.syntax()))
            .unwrap_or_default();
        let head = {
            let start = node
                .match_kw_token()
                .map_or(0, |t| u32::from(t.text_range().start()));
            let end = subject_expr
                .as_ref()
                .map_or(start, |e| u32::from(e.syntax().text_range().end()));
            Span::new(self.path, start, end)
        };
        if outcome {
            return self.outcome_match(node, mode, &arms, head);
        }
        let subject = self.subject(subject_expr);
        let mut seen: BTreeMap<Case, Span> = BTreeMap::new();
        let mut wildcard: Option<Span> = None;
        let mut closed_wildcard: Option<Span> = None;
        let mut after_wildcard = false;
        // A pattern already reported leaves its cases unknown, so whether
        // the arms cover everything isn't asked.
        let mut misread = false;
        let mut completes = arms.is_empty();
        let mut value: Option<Ty> = None;
        let mut deferred: Vec<Expr> = Vec::new();
        for arm in &arms {
            self.scopes.push(Vec::new());
            if let Some(ast::Pat::CasePat(case)) = arm.pat() {
                // A name resolution reports, such as a package's version
                // missing, leaves the case unknown too.
                misread |=
                    !self.resolved_by(|names, out| crate::resolve::pattern(names, &case, out));
            }
            if let Some(pat) = arm.pat() {
                let span = self.span(&pat);
                let before = self.out.len();
                let reading = self.pattern(&subject, &pat, &written);
                misread |= self.out.len() > before;
                let fact = match (&reading.case, &subject) {
                    (Some(Case::Variant(variant)), Subject::Enum(host)) => {
                        Some(PatternFact::Variant(host.clone(), variant.clone()))
                    }
                    (Some(Case::Some), _) => Some(PatternFact::Some),
                    _ if reading.wildcard => Some(PatternFact::Wildcard),
                    _ => None,
                };
                if let Some(fact) = fact {
                    self.facts.patterns.insert(node_key(pat.syntax()), fact);
                }
                if let Some(first) = &wildcard {
                    if !after_wildcard {
                        after_wildcard = true;
                        self.unreachable_arm(span.clone(), first.clone());
                    }
                } else if let Some(case) = reading.case.clone() {
                    match seen.get(&case) {
                        Some(first) => {
                            let host = match &subject {
                                Subject::Enum(name) => name.clone(),
                                _ => String::new(),
                            };
                            self.duplicate_arm(&case.shown(&host), span.clone(), first.clone());
                        }
                        None => {
                            seen.insert(case, span.clone());
                        }
                    }
                }
                if reading.wildcard {
                    match subject {
                        Subject::Enum(_) | Subject::Bool | Subject::Nullable(_) => {
                            closed_wildcard.get_or_insert(span);
                        }
                        _ => {
                            wildcard.get_or_insert(span);
                        }
                    }
                }
                if let Some((name, ty, brackets)) = reading.binding {
                    self.declare(
                        Some(name),
                        ty,
                        LocalKind::Binding,
                        Some(brackets),
                        "variable",
                    );
                }
            }
            completes |= self.arm_body(arm, mode, &mut value, &mut deferred);
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
        if !misread {
            self.exhaustive(&subject, &seen, wildcard.is_some(), closed_wildcard, head);
        }
        let ty = match mode {
            Mode::Value(Some(expected)) => expected.clone(),
            _ => value.unwrap_or(Ty::Error),
        };
        (ty, completes)
    }

    /// An arm's body; whether control can go on past the `match` through
    /// it.
    pub(super) fn arm_body(
        &mut self,
        arm: &ast::MatchArm,
        mode: Mode<'_>,
        value: &mut Option<Ty>,
        deferred: &mut Vec<Expr>,
    ) -> bool {
        if let Some(block) = arm.block() {
            let completes = self.block(Some(block));
            if completes && matches!(mode, Mode::Value(_)) {
                let span = line_span(self.path, arm.syntax());
                self.out.push(
                    Diagnostic::new(&rule::TYPE_MATCH_ARM_VALUE, "This arm gives no value")
                        .at(span, "ends without a value")
                        .reason("A `match` used as a value gives one from every arm: an expression, `reject` or `propagate`; this block can reach its end without leaving.")
                        .next("Write the arm's value after `->`, or end the block with `return` or `reject`"),
                );
            }
            return completes;
        }
        let Some(expr) = arm.expr() else {
            return true;
        };
        match mode {
            Mode::Statement => self.standalone(&expr),
            Mode::Value(expected) => match &expr {
                Expr::RejectExpr(reject) => {
                    self.reject(reject);
                    false
                }
                Expr::PropagateExpr(propagate) => {
                    self.propagate(propagate);
                    false
                }
                _ => {
                    match expected {
                        Some(expected) => self.check(&expr, expected),
                        None => match value.clone() {
                            Some(ty) => self.check(&expr, &ty),
                            None if is_literal(&expr) => deferred.push(expr.clone()),
                            None => *value = Some(self.infer(&expr)),
                        },
                    }
                    true
                }
            },
        }
    }

    /// Reads one pattern against the subject, reporting a pattern that
    /// can't match it.
    fn pattern(&mut self, subject: &Subject, pat: &Pat, written: &str) -> Reading {
        let span = self.span(pat);
        match pat {
            Pat::WildcardPat(_) => Reading {
                wildcard: true,
                ..Reading::none()
            },
            Pat::LiteralPat(literal) => {
                let Some(value) = literal.literal() else {
                    return Reading::none();
                };
                let constant = Const::from_literal(&value, literal.minus_token().is_some());
                self.literal_pattern(subject, &constant, span, written)
            }
            Pat::CasePat(case) => {
                let path: Vec<ast::NameRef> = case
                    .path()
                    .map(|path| path.name_refs().collect())
                    .unwrap_or_default();
                let binding = case.pat_binding();
                let brackets = binding.as_ref().map(|b| self.span(b));
                let name = binding.and_then(|b| b.name());
                self.case_pattern(subject, &path, name, brackets, span, written)
            }
        }
    }

    fn literal_pattern(
        &mut self,
        subject: &Subject,
        value: &Const,
        span: Span,
        written: &str,
    ) -> Reading {
        let case = |case: Case| Reading {
            case: Some(case),
            ..Reading::none()
        };
        match (subject, value) {
            (Subject::Error, _) => Reading::none(),
            (Subject::Nullable(_), Const::None) => case(Case::None),
            (_, Const::None) => {
                self.never_none(subject, span, written, "none");
                Reading::none()
            }
            (Subject::Bool, Const::Bool(true)) => case(Case::True),
            (Subject::Bool, Const::Bool(false)) => case(Case::False),
            (Subject::Open(ty), Const::Number { value, decimal }) => {
                let prim = self.table.representation(ty);
                let fits = match prim {
                    Some(Prim::Int) => !decimal,
                    Some(Prim::Decimal) => *decimal,
                    _ => false,
                };
                if fits {
                    case(Case::Literal(super::super::rules::show(*value)))
                } else {
                    let shown = Const::Number {
                        value: *value,
                        decimal: *decimal,
                    }
                    .describe()
                    .to_string();
                    self.pattern_type(subject, span, shown);
                    Reading::none()
                }
            }
            (Subject::Open(ty), Const::Str(text))
                if self.table.representation(ty) == Some(Prim::Text) =>
            {
                case(Case::Literal(format!("{text:?}")))
            }
            _ => {
                self.pattern_type(subject, span, value.describe().to_string());
                Reading::none()
            }
        }
    }

    fn case_pattern(
        &mut self,
        subject: &Subject,
        path: &[ast::NameRef],
        name: Option<ast::Name>,
        brackets: Option<Span>,
        span: Span,
        written: &str,
    ) -> Reading {
        let mut names: Vec<String> = path.iter().map(text).collect();
        // A package's enum is written through the package, with its version
        // while two are held (area 11; chunk 7.1e): `Postcodes.Round.morning`
        // matches as `Postcodes.Round`'s `morning`.
        let mut path = path;
        match self.package_path(&names) {
            Some(Some((host, taken))) => {
                names.splice(..taken, [host]);
                path = &path[taken - 1..];
            }
            Some(None) => return self.silenced(name),
            None => {}
        }
        let bind = |ty: Ty| match (name.clone(), brackets.clone()) {
            (Some(name), Some(brackets)) => Some((name, ty, brackets)),
            _ => None,
        };
        if let Subject::Error = subject {
            return Reading {
                binding: bind(Ty::Error),
                ..Reading::none()
            };
        }
        match names.as_slice() {
            [only] if only == "some" => match subject {
                Subject::Nullable(inner) => Reading {
                    case: Some(Case::Some),
                    binding: bind(inner.clone()),
                    ..Reading::none()
                },
                _ => {
                    self.never_none(subject, span, written, "some(...)");
                    let ty = match subject {
                        Subject::Open(ty) | Subject::Other(ty) => ty.clone(),
                        Subject::Enum(name) => Ty::Named(name.clone()),
                        Subject::Bool => Ty::Prim(Prim::Bool),
                        _ => Ty::Error,
                    };
                    Reading {
                        binding: bind(ty),
                        ..Reading::none()
                    }
                }
            },
            [host, variant] if starts_upper(host) => {
                let Subject::Enum(subject_name) = subject else {
                    self.pattern_type(subject, span, format!("`{host}.{variant}`"));
                    return self.silenced(name);
                };
                if host != subject_name {
                    self.pattern_type(subject, span, format!("`{host}.{variant}`"));
                    return self.silenced(name);
                }
                let Some(found) = self
                    .table
                    .variants(host)
                    .and_then(|variants| variants.iter().find(|v| v.name == *variant))
                    .cloned()
                else {
                    self.unknown_pattern_variant(host, &path[1]);
                    return self.silenced(name);
                };
                match (&found.fields, brackets.clone()) {
                    (None, Some(brackets)) => {
                        let shown = format!("{host}.{variant}");
                        self.out.push(
                            Diagnostic::new(
                                &rule::TYPE_MATCH_UNKNOWN_BINDING,
                                format!("`{shown}` carries no data to name"),
                            )
                            .at(brackets.clone(), "no data")
                            .reason(format!(
                                "`{shown}` is a plain variant; brackets after a case name the data it carries, and this one carries none."
                            ))
                            .next(format!("Write `{shown}` without brackets"))
                            .edit(brackets, ""),
                        );
                        let mut reading = self.silenced(name);
                        reading.case = Some(Case::Variant(variant.clone()));
                        reading
                    }
                    _ => Reading {
                        case: Some(Case::Variant(variant.clone())),
                        binding: bind(Ty::Variant(host.clone(), variant.clone())),
                        ..Reading::none()
                    },
                }
            }
            [only] if !starts_upper(only) => {
                let suggestion = match subject {
                    Subject::Enum(host)
                        if self
                            .table
                            .variants(host)
                            .is_some_and(|variants| variants.iter().any(|v| v.name == *only)) =>
                    {
                        Some(format!("{host}.{only}"))
                    }
                    _ => None,
                };
                let shown = format!("`{only}`");
                match suggestion {
                    Some(full) => {
                        let first = self.span(&path[0]);
                        let subject_ty = self.subject_shown(subject);
                        self.out.push(
                            Diagnostic::new(
                                &rule::TYPE_MATCH_PATTERN_TYPE,
                                format!("{shown} can't match {subject_ty}"),
                            )
                            .at(span, "not a pattern")
                            .reason("A variant is always written with its enum's name.")
                            .next(format!("Write `{full}`"))
                            .edit(first, full),
                        );
                    }
                    None => self.pattern_type(subject, span, shown),
                }
                self.silenced(name)
            }
            _ => {
                self.pattern_type(subject, span, format!("`{}`", names.join(".")));
                self.silenced(name)
            }
        }
    }

    /// A binding in a pattern already reported: in scope, so the arm's
    /// uses of it are not reported again, and counted as used.
    fn silenced(&mut self, name: Option<ast::Name>) -> Reading {
        if let Some(name) = name {
            self.declare(
                Some(name.clone()),
                Ty::Error,
                LocalKind::Inline,
                None,
                "variable",
            );
        }
        Reading::none()
    }

    fn subject_shown(&self, subject: &Subject) -> String {
        match subject {
            Subject::Enum(name) => format!("a `{name}`"),
            Subject::Bool => "a `Bool`".to_string(),
            Subject::Nullable(inner) => format!("a `{inner}?`"),
            Subject::Open(ty) | Subject::Other(ty) => format!("a `{ty}`"),
            Subject::Error => "this value".to_string(),
        }
    }

    fn pattern_type(&mut self, subject: &Subject, span: Span, shown: String) {
        let subject_ty = self.subject_shown(subject);
        let reason = match subject {
            Subject::Enum(name) => {
                let first = self
                    .table
                    .variants(name)
                    .and_then(|v| v.first())
                    .map_or(String::new(), |v| v.name.clone());
                format!("A `{name}` is matched by its variants, such as `{name}.{first}`.")
            }
            Subject::Bool => "A `Bool` is matched by `true` and `false`.".to_string(),
            Subject::Nullable(_) => {
                "A nullable value is matched by `some(name)` and `none`.".to_string()
            }
            Subject::Open(ty) => {
                let base = self
                    .table
                    .representation(ty)
                    .map_or("its base", |prim| prim.name());
                format!("A `{ty}` is matched by `{base}` literals, and `_` for the rest.")
            }
            Subject::Other(ty) => {
                format!("A `{ty}` has no cases to list, so only `_` matches it.")
            }
            Subject::Error => String::new(),
        };
        self.out.push(
            Diagnostic::new(
                &rule::TYPE_MATCH_PATTERN_TYPE,
                format!("{} can't match {subject_ty}", capital(&shown)),
            )
            .at(span, "wrong kind of pattern")
            .reason(reason)
            .next(format!("Write a pattern for {subject_ty}")),
        );
    }

    fn never_none(&mut self, subject: &Subject, span: Span, written: &str, pattern: &str) {
        let subject_ty = self.subject_shown(subject);
        self.out.push(
            Diagnostic::new(
                &rule::TYPE_MATCH_SOME_NON_OPTIONAL,
                format!("`{written}` is never `none`, so `{pattern}` can't match it"),
            )
            .at(span, "for nullable values")
            .reason(format!(
                "`some(name)` and `none` match a nullable value, written `T?`; `{written}` is {subject_ty}."
            ))
            .next(format!("Use `{written}` directly: it always has a value")),
        );
    }

    fn unknown_pattern_variant(&mut self, host: &str, variant: &ast::NameRef) {
        let name = text(variant);
        let span = self.span(variant);
        let names: Vec<String> = self
            .table
            .variants(host)
            .map(|variants| variants.iter().map(|v| v.name.clone()).collect())
            .unwrap_or_default();
        let near = nearest(&name, names.iter().map(String::as_str)).map(str::to_string);
        let problem = Diagnostic::new(
            &rule::TYPE_MATCH_UNKNOWN_VARIANT,
            format!("`{host}` has no variant `{name}`"),
        )
        .at(span.clone(), "not a variant")
        .reason(format!(
            "{}{}",
            super::variants_line(host, &names),
            did_you_mean(near.as_deref())
        ));
        self.out.push(match near {
            Some(near) => problem
                .next(format!("Use `{host}.{near}`"))
                .edit(span, near),
            None => problem.next(format!("Match one of `{host}`'s variants")),
        });
    }

    fn duplicate_arm(&mut self, shown: &str, span: Span, first: Span) {
        self.out.push(
            Diagnostic::new(
                &rule::TYPE_MATCH_DUPLICATE_PATTERN,
                format!("{shown} is matched twice"),
            )
            .at(span, "matched again")
            .also(first, "first matched here")
            .reason("Each case is matched once; the first arm takes every value this one would, so this arm never runs.")
            .next("Remove this arm, or merge its body into the first"),
        );
    }

    fn unreachable_arm(&mut self, span: Span, wildcard: Span) {
        self.out.push(
            Diagnostic::new(&rule::TYPE_MATCH_UNREACHABLE_PATTERN, "This arm never runs")
                .at(span, "after `_`")
                .also(wildcard, "matches everything left")
                .reason("`_` matches every value the arms above it don't, so no arm after it runs.")
                .next("Move this arm above `_`"),
        );
    }

    /// Every case covered (area 3, question 10).
    fn exhaustive(
        &mut self,
        subject: &Subject,
        seen: &BTreeMap<Case, Span>,
        wildcard: bool,
        closed_wildcard: Option<Span>,
        head: Span,
    ) {
        let (host, all, reason) = match subject {
            Subject::Enum(name) => (
                name.clone(),
                self.table
                    .variants(name)
                    .map(|variants| {
                        variants
                            .iter()
                            .map(|v| Case::Variant(v.name.clone()))
                            .collect()
                    })
                    .unwrap_or_default(),
                format!(
                    "A `match` on an enum lists every variant, so a variant added later is caught at every `match` that has to consider it; `_` isn't allowed on `{name}`."
                ),
            ),
            Subject::Bool => (
                String::new(),
                vec![Case::True, Case::False],
                "A `match` on a `Bool` lists `true` and `false`.".to_string(),
            ),
            Subject::Nullable(_) => (
                String::new(),
                vec![Case::Some, Case::None],
                "A `match` on a nullable value lists `some(name)` and `none`.".to_string(),
            ),
            Subject::Open(ty) | Subject::Other(ty) => {
                if !wildcard {
                    let what = match self.table.representation(ty) {
                        Some(prim @ (Prim::Text | Prim::Int | Prim::Decimal)) => {
                            format!(
                                "`{ty}` is built on `{}`, so its values can't all be listed",
                                prim.name()
                            )
                        }
                        _ => format!("`{ty}` has no cases to list"),
                    };
                    self.out.push(
                        Diagnostic::new(
                            &rule::TYPE_MATCH_WILDCARD_REQUIRED,
                            "This `match` needs a `_` arm",
                        )
                        .at(head, "not every value is matched")
                        .reason(format!(
                            "{what}; `_` matches every value the arms above it don't."
                        ))
                        .next("Add `_ -> ...` as the last arm"),
                    );
                }
                return;
            }
            Subject::Error => return,
        };
        let missing: Vec<String> = all
            .iter()
            .filter(|case| !seen.contains_key(case))
            .map(|case| case.shown(&host))
            .collect();
        if let Some(span) = closed_wildcard {
            let kind = match subject {
                Subject::Enum(name) => format!("a `{name}`"),
                Subject::Bool => "a `Bool`".to_string(),
                _ => "a nullable value".to_string(),
            };
            let next = if missing.is_empty() {
                "Remove the `_` arm".to_string()
            } else {
                format!("Replace `_` with arms for {}", crate::and_list(&missing))
            };
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_MATCH_WILDCARD_CLOSED,
                    format!("`_` can't stand for the rest of {kind}"),
                )
                .at(span, "wildcard")
                .reason("On enums, `Bool` and nullable values every case is listed, so a case added later is caught at every `match` that has to consider it.")
                .next(next),
            );
            return;
        }
        if missing.is_empty() {
            return;
        }
        let next = if missing.len() == 1 {
            format!("Add an arm for {}", missing[0])
        } else {
            format!("Add arms for {}", crate::and_list(&missing))
        };
        self.out.push(
            Diagnostic::new(
                &rule::TYPE_MATCH_NON_EXHAUSTIVE,
                format!("This `match` doesn't cover {}", crate::and_list(&missing)),
            )
            .at(head, format!("{} not matched", plural_cases(missing.len())))
            .reason(reason)
            .next(next),
        );
    }
}

fn plural_cases(n: usize) -> String {
    if n == 1 {
        "1 case".to_string()
    } else {
        format!("{n} cases")
    }
}
