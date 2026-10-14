//! Area 2's rules for values: checking a value against the type its
//! position expects, the type of an expression, reading fields and
//! building objects and variants.

use std::collections::BTreeMap;

use jadpo_db::ItemKind;
use jadpo_diag::{Diagnostic, Span, nearest, rule};
use jadpo_syntax::ast::{self, AstNode, Expr, MemberItem};

use super::super::literal;
use super::super::rules::{Const, Rules};
use super::super::shape::FieldsBase;
use super::super::table::{Def, FieldDef};
use super::super::ty::{Prim, Ty};
use super::{Body, did_you_mean, is_literal, node_key, text};
use crate::case::starts_upper;
use crate::std_lib::{Namespace, StdNames};
use crate::tree::{self, dotted};

/// The generated namespace whose one member is the time (area 10).
const CLOCK: &str = "clock";

impl Body<'_> {
    // --- checking against an expected type -------------------------------

    fn check_value(&mut self, expr: &Expr, expected: &Ty) {
        if let Expr::ParenExpr(paren) = expr {
            if let Some(inner) = paren.expr() {
                self.check(&inner, expected);
            }
            return;
        }
        if is_literal(expr) {
            self.literal(expr, expected, false);
            return;
        }
        if let Expr::MatchExpr(subject) = expr {
            self.match_value(subject, Some(expected));
            return;
        }
        if let Expr::LambdaExpr(lambda) = expr {
            self.misplaced_inline(lambda);
            return;
        }
        if let Expr::ListExpr(list) = expr {
            match &self.table.structure(expected) {
                Ty::List(item) | Ty::Set(item) => {
                    let item = (**item).clone();
                    for element in list.exprs() {
                        self.check(&element, &item);
                    }
                    let length = Const::List(list.exprs().map(|_| Const::Other).collect());
                    if let Err(problem) = literal::check(self.table, expected, &length) {
                        self.literal_problem(expr, expected, problem, false);
                        return;
                    }
                    let is_set = matches!(self.table.structure(expected), Ty::Set(_));
                    // Literals and variants; a computed value is only
                    // known when it runs.
                    let items: Vec<Const> = list
                        .exprs()
                        .map(|e| match Const::from_expr(&e) {
                            Const::Path(names)
                                if !names.first().is_some_and(|n| starts_upper(n)) =>
                            {
                                Const::Other
                            }
                            other => other,
                        })
                        .collect();
                    if is_set && let Some(twice) = literal::repeated(&items) {
                        let problem = literal::Problem::Builtin(format!(
                            "has {twice} twice, and a set holds each value once"
                        ));
                        self.literal_problem(expr, expected, problem, false);
                    }
                    return;
                }
                Ty::Error | Ty::Var(_) | Ty::Opaque(..) => {}
                _ => {
                    for element in list.exprs() {
                        self.infer(&element);
                    }
                    let actual = Ty::List(Box::new(Ty::Error));
                    self.mismatch(expr, &actual, expected);
                    return;
                }
            }
        }
        if let Expr::CallExpr(call) = expr {
            let actual = self.call(call, Some(expected));
            self.facts
                .types
                .insert(node_key(expr.syntax()), actual.clone());
            self.require(expr, &actual, expected);
            return;
        }
        let actual = self.infer(expr);
        self.require(expr, &actual, expected);
    }

    /// Checks `expr` where `expected` is required, and records the type it
    /// takes there when nothing else did: a literal's is the type it goes
    /// into.
    pub(super) fn check(&mut self, expr: &Expr, expected: &Ty) {
        self.check_value(expr, expected);
        self.facts
            .types
            .entry(node_key(expr.syntax()))
            .or_insert_with(|| expected.clone());
    }

    /// Reports `actual` where `expected` is required, unless it widens.
    pub(super) fn require(&mut self, expr: &Expr, actual: &Ty, expected: &Ty) {
        if self.table.assignable(actual, expected) {
            return;
        }
        self.mismatch(expr, actual, expected);
    }

    pub(super) fn mismatch(&mut self, expr: &Expr, actual: &Ty, expected: &Ty) {
        let span = self.span(expr);
        let written = expr.syntax().text().to_string();
        if self.table.siblings(actual, expected) {
            let common = self
                .table
                .parent(expected.strip_nullable())
                .map_or_else(|| "the same type".to_string(), |t| format!("`{t}`"));
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_SIBLING_MISMATCH,
                    format!("`{actual}` can't be used where `{expected}` is required"),
                )
                .at(span, format!("this is `{actual}`"))
                .reason(format!(
                    "Both refine {common}, but a field type widens only to its own declared type, so one type's values never stand in for another's."
                ))
                .next(format!("Pass a `{expected}` here")),
            );
            return;
        }
        let problem = Diagnostic::new(
            &rule::TYPE_MISMATCH,
            format!("`{actual}` can't be used where `{expected}` is required"),
        )
        .at(span.clone(), format!("this is `{actual}`"));
        let representation = self.table.representation(expected);
        let problem = if self.table.is_nullable(actual) && !self.table.is_nullable(expected) {
            problem
                .reason(format!(
                    "`{actual}` may be `none`, and `{expected}` can't be."
                ))
                .next(format!(
                    "Use `match` to handle `none` before using `{written}` as `{expected}`"
                ))
        } else if matches!(actual, Ty::Prim(prim) if Some(*prim) == representation && prim.is_representation())
        {
            let target = expected.strip_nullable().to_string();
            problem
                .reason(format!(
                    "Arithmetic works on the representation, so this is a plain `{actual}`; only `{target}(...)` makes it a `{target}`, checking its rules."
                ))
                .next(format!("Convert it with `{target}({written})`"))
                .edit(Span::new(self.path, span.start, span.start), format!("{target}("))
                .edit(Span::new(self.path, span.end, span.end), ")")
        } else if !actual.has_error() && self.table.assignable(expected, actual) {
            // A general value where a refinement of it is required.
            let target = expected.strip_nullable().to_string();
            problem
                .reason(format!(
                    "`{target}` is a refinement of `{actual}`; a general value becomes the specific one only through `{target}(...)`, which re-checks its rules."
                ))
                .next(format!("Convert it with `{target}({written})`"))
                .edit(Span::new(self.path, span.start, span.start), format!("{target}("))
                .edit(Span::new(self.path, span.end, span.end), ")")
        } else if matches!(
            (actual.strip_nullable(), expected.strip_nullable()),
            (Ty::List(_), Ty::List(_)) | (Ty::Set(_), Ty::Set(_)) | (Ty::Map(..), Ty::Map(..))
        ) {
            problem
                .reason(format!(
                    "Collections keep their element type, so a `{actual}` is never a `{expected}`, even where each element would widen."
                ))
                .next(format!("Build a `{expected}` here"))
        } else {
            problem
                .reason(format!(
                    "`{actual}` is {} and `{expected}` is {}; a value widens only along declared steps, from a field type to its declared type and from a named type to the type it builds on.",
                    literal::what(self.table, actual),
                    literal::what(self.table, expected)
                ))
                .next(format!("Use a `{expected}` value here"))
        };
        self.out.push(problem);
    }

    /// A literal where `expected` is required; `constructor` when it is the
    /// value given to `Type(...)`.
    pub(super) fn literal(&mut self, expr: &Expr, expected: &Ty, constructor: bool) {
        let value = Const::from_expr(expr);
        if let Err(problem) = literal::check(self.table, expected, &value) {
            self.literal_problem(expr, expected, problem, constructor);
        }
    }

    pub(super) fn literal_problem(
        &mut self,
        expr: &Expr,
        expected: &Ty,
        problem: literal::Problem,
        constructor: bool,
    ) {
        let written = tree::written(expr.syntax());
        let span = self.span(expr);
        self.out.push(literal::diagnostic(
            self.table,
            problem,
            &written,
            expected,
            span,
            constructor,
        ));
    }

    // --- the type of an expression ----------------------------------------

    /// The type of an expression, remembered by position so a
    /// construction can be proved safe from its parts (area 4, question 6).
    pub(super) fn infer(&mut self, expr: &Expr) -> Ty {
        let ty = self.infer_expr(expr);
        self.facts.types.insert(node_key(expr.syntax()), ty.clone());
        ty
    }

    fn infer_expr(&mut self, expr: &Expr) -> Ty {
        match expr {
            Expr::Literal(_) => match Const::from_expr(expr) {
                Const::Number { decimal: true, .. } => Ty::Prim(Prim::Decimal),
                Const::Number { .. } => Ty::Prim(Prim::Int),
                Const::Str(_) => Ty::Prim(Prim::Text),
                Const::Bool(_) => Ty::Prim(Prim::Bool),
                Const::Duration(_) => Ty::Prim(Prim::Duration),
                Const::None => Ty::None,
                _ => Ty::Error,
            },
            Expr::ParenExpr(paren) => paren.expr().map_or(Ty::Error, |inner| self.infer(&inner)),
            Expr::PrefixExpr(prefix) if is_literal(expr) => match Const::from_expr(expr) {
                Const::Number { decimal: true, .. } => Ty::Prim(Prim::Decimal),
                Const::Number { .. } => Ty::Prim(Prim::Int),
                _ => self.prefix(prefix),
            },
            Expr::PrefixExpr(prefix) => self.prefix(prefix),
            Expr::NameRef(name) => self.name_value(name),
            Expr::FieldExpr(field) => self.field_expr(expr, field),
            Expr::CallExpr(call) => self.call(call, None),
            Expr::RecordExpr(record) => self.record(record),
            Expr::ListExpr(list) => {
                let mut item = Ty::Error;
                for element in list.exprs() {
                    if item.is_error() && !is_literal(&element) {
                        item = self.infer(&element);
                    } else if !item.is_error() {
                        self.check(&element, &item.clone());
                    } else {
                        self.infer(&element);
                    }
                }
                Ty::List(Box::new(item))
            }
            Expr::BinExpr(binary) => self.binary(binary),
            Expr::AttemptExpr(attempt) => self.attempt(attempt, None),
            Expr::MatchExpr(subject) => self.match_value(subject, None),
            Expr::LambdaExpr(lambda) => {
                self.misplaced_inline(lambda);
                Ty::Error
            }
            // Both start a statement or end an arm, which handle them
            // before asking for a type; anywhere else they would stop the
            // callable in the middle of a line (area 3, question 11).
            Expr::RejectExpr(reject) => {
                self.misplaced_exit(expr, "reject");
                self.reject(reject);
                Ty::Error
            }
            Expr::PropagateExpr(_) => {
                self.misplaced_exit(expr, "propagate");
                self.flow.open = true;
                Ty::Error
            }
            // A type written as a value, or a later area's form: nothing
            // to check here, but the names in it are resolved.
            _ => {
                self.walk(|walk| walk.children(expr));
                Ty::Error
            }
        }
    }

    /// `reject` or `propagate` inside an expression.
    fn misplaced_exit(&mut self, expr: &Expr, word: &str) {
        let span = super::line_span(self.path, expr.syntax());
        let span = jadpo_diag::Span::new(self.path, span.start, span.start + word.len() as u32);
        let rule = if word == "reject" {
            &rule::FAILURE_REJECT_PLACEMENT
        } else {
            &rule::FAILURE_PROPAGATE_PLACEMENT
        };
        self.flow.open = true;
        self.out.push(
            Diagnostic::new(rule, format!("`{word}` can only start a statement"))
                .at(span, "inside an expression")
                .reason(format!(
                    "Every place a callable can stop early is at the start of a line: `{word}` is a statement of its own, or the result of a `match` arm, never part of a value."
                ))
                .next(if word == "reject" {
                    "Move `reject` to a line of its own, under an `if` or as a `match` arm".to_string()
                } else {
                    "Write `propagate` as the result of a failure arm of a `match`".to_string()
                }),
        );
    }

    // --- reading fields ---------------------------------------------------

    pub(super) fn field_expr(&mut self, expr: &Expr, field: &ast::FieldExpr) -> Ty {
        let Some((first, rest)) = dotted(expr) else {
            let receiver = field.expr().map_or(Ty::Error, |inner| self.infer(&inner));
            return match field.name_ref() {
                Some(name) => self.select(&receiver, &name, field),
                None => Ty::Error,
            };
        };
        let head = text(&first);
        if starts_upper(&head) {
            self.resolved_by(|names, out| names.value_name(expr, out));
            return self.type_member(&head, &rest, expr);
        }
        if StdNames::get().namespace(&head) == Some(Namespace::Generated) {
            return self.generated_value(&head, &rest, expr);
        }
        let Some(mut receiver) = self.read(&first) else {
            if !self.is_qualifier(&head) {
                self.unknown_value(&first);
            }
            return Ty::Error;
        };
        let fields: Vec<ast::FieldExpr> = expr
            .syntax()
            .descendants()
            .filter_map(ast::FieldExpr::cast)
            .collect();
        // Innermost first: `a.b` before `a.b.c`.
        for (name, node) in rest.iter().zip(fields.iter().rev()) {
            receiver = self.select(&receiver, name, node);
            if receiver.is_error() {
                break;
            }
        }
        receiver
    }

    /// A member of a namespace the compiler fills (`clock.now`,
    /// `config.api_key`): built ones have their type, and the rest wait for
    /// their area.
    pub(super) fn generated_value(&mut self, head: &str, rest: &[ast::NameRef], expr: &Expr) -> Ty {
        let members: Vec<String> = rest.iter().map(text).collect();
        let written = format!("{head}.{}", members.join("."));
        let span = self.span(expr);
        match members.as_slice() {
            [member] if StdNames::get().generated_member(head, member) => {
                if head == CLOCK {
                    self.clock_read(span, &written);
                    return Ty::Prim(Prim::Instant);
                }
                Ty::Error
            }
            _ => {
                self.not_built_yet(span, &written);
                Ty::Error
            }
        }
    }

    /// `clock.now` is an effect only actions have (areas 3 and 10).
    pub(super) fn clock_read(&mut self, span: jadpo_diag::Span, written: &str) {
        if !self.context.is_pure() {
            return;
        }
        let caller = self.context.with_a();
        self.out.push(
            Diagnostic::new(
                &rule::TYPE_CLOCK_CONTEXT,
                format!("{} can't read `{written}`", super::capital(caller)),
            )
            .at(span, "reads the clock")
            .reason(format!(
                "{} is pure: it gives the same result for the same arguments, so the time it needs arrives as a parameter; only actions read the clock.",
                super::capital(caller)
            ))
            .next("Pass the time in as a parameter, read in the action that calls this"),
        );
    }

    /// `Type.member` as a value: an enum's variant.
    pub(super) fn type_member(&mut self, host: &str, rest: &[ast::NameRef], expr: &Expr) -> Ty {
        let Some(variants) = self.table.variants(host) else {
            return Ty::Error;
        };
        let [variant] = rest else {
            return Ty::Error;
        };
        let name = text(variant);
        match variants.iter().find(|v| v.name == name) {
            None => {
                self.unknown_variant(host, variant);
                Ty::Error
            }
            Some(found) => {
                self.facts.variants.insert(
                    node_key(expr.syntax()),
                    (host.to_string(), found.name.clone()),
                );
                if let Some(fields) = &found.fields
                    && !fields.is_empty()
                {
                    let span = self.span(expr);
                    let names: Vec<String> =
                        fields.iter().map(|f| format!("`{}`", f.name)).collect();
                    self.out.push(
                        Diagnostic::new(
                            &rule::TYPE_MISSING_VARIANT_FIELD,
                            format!("`{host}.{name}` is missing {}", crate::and_list(&names)),
                        )
                        .at(span, "no fields given")
                        .reason(format!(
                            "`{host}.{name}` carries fields, given in a block when the value is built."
                        ))
                        .next(format!("Write `{host}.{name} {{ ... }}` with each field")),
                    );
                }
                Ty::Named(host.to_string())
            }
        }
    }

    pub(super) fn unknown_variant(&mut self, host: &str, variant: &ast::NameRef) {
        let name = text(variant);
        let span = self.span(variant);
        let names: Vec<String> = self
            .table
            .variants(host)
            .map(|variants| variants.iter().map(|v| v.name.clone()).collect())
            .unwrap_or_default();
        let near = nearest(&name, names.iter().map(String::as_str)).map(str::to_string);
        let problem = Diagnostic::new(
            &rule::TYPE_UNKNOWN_ENUM_VARIANT,
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
            None => problem.next(format!("Use one of `{host}`'s variants")),
        });
    }

    /// The object a value's fields come from, following field types to
    /// their declared types.
    pub(super) fn object_of(&self, ty: &Ty) -> Option<String> {
        let mut current = ty.strip_nullable().clone();
        for _ in 0..64 {
            match &current {
                Ty::Named(name) => {
                    return self.table.fields(name).map(|_| name.clone());
                }
                Ty::Field(..) => current = self.table.parent(&current)?,
                _ => return None,
            }
        }
        None
    }

    /// Reads field `name` from a value of type `receiver`.
    pub(super) fn select(
        &mut self,
        receiver: &Ty,
        name: &ast::NameRef,
        node: &ast::FieldExpr,
    ) -> Ty {
        // A type a later area defines has fields this check doesn't know yet.
        if receiver.has_error() || matches!(receiver.strip_nullable(), Ty::Opaque(..) | Ty::Var(_))
        {
            return Ty::Error;
        }
        let field = text(name);
        let span = self.span(name);
        if let Ty::Variant(host, variant) = receiver {
            let ty = self.variant_field(host, variant, &field, span);
            if !ty.has_error() {
                self.facts.fields.insert(
                    node_key(node.syntax()),
                    (host.clone(), vec![variant.clone(), field.clone()]),
                );
            }
            return ty;
        }
        let value = node
            .expr()
            .map(|e| tree::written(e.syntax()))
            .unwrap_or_default();
        if self.table.is_nullable(receiver) {
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_NULLABLE_SELECTION,
                    format!("`{value}` may be `none`, so `{field}` can't be read yet"),
                )
                .at(span, "may be `none`")
                .reason(format!(
                    "`{value}` is `{receiver}`; a field is read once `match` has found a value."
                ))
                .next(format!(
                    "Match on `{value}` and read `{field}` inside `some(...)`"
                )),
            );
            return Ty::Error;
        }
        let Some(host) = self.object_of(receiver) else {
            let start = node
                .dot_token()
                .map_or(span.start, |dot| u32::from(dot.text_range().start()));
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_FIELD_ON_NON_RECORD,
                    format!("`{receiver}` has no fields"),
                )
                .at(span.clone(), "no such field")
                .reason(format!(
                    "`{value}` is a `{receiver}`, {}; only objects, inputs, outputs, events, failures and entities have fields.",
                    literal::what(self.table, receiver)
                ))
                .next(format!("Use `{value}` itself"))
                .edit(Span::new(self.path, start, span.end), ""),
            );
            return Ty::Error;
        };
        let fields = self.table.fields(&host).unwrap_or(&[]);
        match fields.iter().find(|f| f.name == field) {
            Some(found) => {
                self.facts
                    .fields
                    .insert(node_key(node.syntax()), (host.clone(), vec![field.clone()]));
                let ty = Ty::Field(host, vec![field]);
                if found.optional { ty.nullable() } else { ty }
            }
            None => {
                self.unknown_field(&host, &field, fields, span, false);
                Ty::Error
            }
        }
    }

    pub(super) fn unknown_field(
        &mut self,
        host: &str,
        field: &str,
        fields: &[FieldDef],
        span: Span,
        variant: bool,
    ) {
        let names: Vec<&str> = fields.iter().map(|f| f.name.as_str()).collect();
        let near = nearest(field, names.iter().copied());
        let listed: Vec<String> = names.iter().map(|n| format!("`{n}`")).collect();
        let rule = if variant {
            &rule::TYPE_UNKNOWN_VARIANT_FIELD
        } else {
            &rule::TYPE_UNKNOWN_FIELD
        };
        let fields_line = if listed.is_empty() {
            format!("`{host}` has no fields.")
        } else {
            format!("`{host}`'s fields are {}.", crate::and_list(&listed))
        };
        let problem = Diagnostic::new(rule, format!("`{host}` has no field `{field}`"))
            .at(span.clone(), "not declared")
            .reason(format!("{fields_line}{}", did_you_mean(near)));
        self.out.push(match near {
            Some(near) => problem
                .next(format!("Use `{near}` instead of `{field}`"))
                .edit(span, near),
            None => problem.next(format!("Remove `{field}`, or declare it on `{host}`")),
        });
    }

    /// `Type { ... }` and `Enum.variant { ... }`.
    pub(super) fn record(&mut self, record: &ast::RecordExpr) -> Ty {
        self.resolved_by(|names, out| crate::resolve::record_head(names, record, out));
        let Some(head) = record.expr() else {
            self.walk(|walk| walk.member_values(record.member_list()));
            return Ty::Error;
        };
        let Some((first, rest)) = dotted(&head) else {
            self.infer(&head);
            self.failure_value(&Expr::RecordExpr(record.clone()));
            return Ty::Error;
        };
        let name = text(&first);
        if !starts_upper(&name) {
            self.failure_value(&Expr::RecordExpr(record.clone()));
            return Ty::Error;
        }
        let head_span = self.span(&head);
        match (rest.as_slice(), self.table.def(&name)) {
            ([], Some(Def::Fields { base, fields, .. })) => {
                if matches!(base, FieldsBase::Entity | FieldsBase::Failure) {
                    self.failure_value(&Expr::RecordExpr(record.clone()));
                    return Ty::Error;
                }
                let fields = fields.clone();
                self.facts.builds.insert(
                    node_key(record.syntax()),
                    super::facts::BuildFact::Object(name.clone()),
                );
                let input = *base == FieldsBase::Input;
                let rules = self.fields_given(record, &name, &[], &fields, input, head_span);
                self.object_built(record, &name, rules);
                Ty::Named(name)
            }
            ([], Some(Def::Scalar { .. })) | ([], None)
                if Prim::from_name(&name).is_some() || self.table.def(&name).is_some() =>
            {
                self.not_record(record, &name, "scalar");
                self.walk(|walk| walk.member_values(record.member_list()));
                Ty::Named(name)
            }
            ([], Some(Def::Enum { .. })) => {
                self.not_record(record, &name, "enum");
                self.walk(|walk| walk.member_values(record.member_list()));
                Ty::Named(name)
            }
            ([variant], Some(Def::Enum { variants })) => {
                let variant_name = text(variant);
                let Some(found) = variants.iter().find(|v| v.name == variant_name).cloned() else {
                    self.unknown_variant(&name, variant);
                    self.walk(|walk| walk.member_values(record.member_list()));
                    return Ty::Error;
                };
                self.facts.builds.insert(
                    node_key(record.syntax()),
                    super::facts::BuildFact::Variant(name.clone(), variant_name.clone()),
                );
                match &found.fields {
                    Some(fields) => {
                        let fields = fields.clone();
                        let shown = format!("{name}.{variant_name}");
                        let rules = self.fields_given(
                            record,
                            &name,
                            &[variant_name],
                            &fields,
                            false,
                            head_span,
                        );
                        self.object_built(record, &shown, rules);
                    }
                    None => {
                        self.not_record(record, &format!("{name}.{variant_name}"), "plain variant");
                        self.walk(|walk| walk.member_values(record.member_list()));
                    }
                }
                Ty::Named(name)
            }
            _ => {
                self.failure_value(&Expr::RecordExpr(record.clone()));
                Ty::Error
            }
        }
    }

    pub(super) fn not_record(&mut self, record: &ast::RecordExpr, written: &str, kind: &str) {
        let span = self.span(record);
        let (reason, next) = match kind {
            "enum" => (
                format!("`{written}` is an enum; its values are its variants."),
                format!(
                    "Write one of `{written}`'s variants, such as `{written}.{}`",
                    self.table
                        .variants(written)
                        .and_then(|v| v.first())
                        .map_or("", |v| v.name.as_str())
                ),
            ),
            "plain variant" => (
                format!("`{written}` carries no fields."),
                format!("Write `{written}` without a block"),
            ),
            _ => {
                let base = self
                    .table
                    .base_prim(&Ty::Named(written.to_string()))
                    .or_else(|| Prim::from_name(written))
                    .map_or_else(String::new, |prim| format!(", built on `{}`,", prim.name()));
                (
                    format!(
                        "Braces build objects and variants with fields; `{written}`{base} takes one value in brackets."
                    ),
                    format!("Write `{written}(...)` with one value"),
                )
            }
        };
        self.out.push(
            Diagnostic::new(
                &rule::TYPE_NOT_RECORD,
                format!("`{written}` isn't built with `{{ ... }}`"),
            )
            .at(span, "has no fields")
            .reason(reason)
            .next(next),
        );
    }

    /// The fields given in a block: each one declared, none twice, every
    /// required one present, each value checked against its field. A value
    /// may be anything that widens to the field's declared type; when the
    /// field adds rules of its own, they run as the object is built (area 2,
    /// question 2). Returns the fields whose own rules the declared ranges
    /// don't prove a computed value follows, so building can fail.
    pub(super) fn fields_given(
        &mut self,
        record: &ast::RecordExpr,
        host: &str,
        variant: &[String],
        fields: &[FieldDef],
        input: bool,
        head_span: Span,
    ) -> Vec<String> {
        let mut unproved: Vec<String> = Vec::new();
        let is_variant = !variant.is_empty();
        let shown = if is_variant {
            format!("{host}.{}", variant.join("."))
        } else {
            host.to_string()
        };
        let mut given: BTreeMap<String, Span> = BTreeMap::new();
        for item in record
            .member_list()
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
            let single = member.member_key().and_then(|key| {
                let mut names = key.names();
                match (names.next(), names.next()) {
                    (Some(first), None) => tree::name_text(&first).map(|name| (first, name)),
                    _ => None,
                }
            });
            let Some((first, name)) = single else {
                if let Some(value) = member.value() {
                    self.walk(|walk| walk.expr(&value));
                }
                continue;
            };
            let span = self.span(&first);
            let value = member.value();
            if let Some(earlier) = given.get(&name) {
                let rule = if is_variant {
                    &rule::TYPE_DUPLICATE_VARIANT_FIELD
                } else {
                    &rule::TYPE_DUPLICATE_FIELD
                };
                self.out.push(
                    Diagnostic::new(rule, format!("`{name}` is given twice"))
                        .at(span, "given again")
                        .also(earlier.clone(), "first given here")
                        .reason(format!(
                            "Each field of `{shown}` is given once, so there is never a question of which value counts."
                        ))
                        .next(format!("Remove one `{name}`")),
                );
                if let Some(value) = value {
                    self.infer(&value);
                }
                continue;
            }
            given.insert(name.clone(), span.clone());
            let Some(field) = fields.iter().find(|f| f.name == name) else {
                self.unknown_field(&shown, &name, fields, span, is_variant);
                if let Some(value) = value {
                    self.infer(&value);
                }
                continue;
            };
            let Some(value) = value else { continue };
            let mut path = variant.to_vec();
            path.push(name);
            let field_ty = Ty::Field(host.to_string(), path);
            if is_literal(&value) || matches!(value, Expr::ListExpr(_)) {
                // A literal takes the field type, so the field's own rules
                // are checked too.
                let expected = if field.optional && !self.table.is_nullable(&field_ty) {
                    field_ty.nullable()
                } else {
                    field_ty
                };
                self.check(&value, &expected);
            } else {
                let mut expected = field.declared.clone();
                if field.optional {
                    expected = expected.nullable();
                }
                let before = self.out.len();
                self.check(&value, &expected);
                let actual = self
                    .facts
                    .types
                    .get(&node_key(value.syntax()))
                    .cloned()
                    .unwrap_or(Ty::Error);
                if field.rules != Rules::default()
                    && self.out.len() == before
                    && !actual.has_error()
                    && !self.proves(&value, &actual, &field_ty)
                {
                    unproved.push(name_of(&field_ty));
                }
            }
        }
        let missing: Vec<String> = fields
            .iter()
            .filter(|f| !given.contains_key(&f.name))
            .filter(|f| !(input && (f.optional || f.default.is_some())))
            .map(|f| format!("`{}`", f.name))
            .collect();
        if missing.is_empty() {
            return unproved;
        }
        let rule = if is_variant {
            &rule::TYPE_MISSING_VARIANT_FIELD
        } else {
            &rule::TYPE_MISSING_FIELD
        };
        let first = missing[0].trim_matches('`').to_string();
        self.out.push(
            Diagnostic::new(rule, format!("`{shown}` is missing {}", crate::and_list(&missing)))
                .at(head_span, "fields missing")
                .reason(format!(
                    "Every field of `{shown}` is given when a value is built; a field that may be absent is declared `T?` and given as `none`."
                ))
                .next(format!("Add `{first}: ...` to the block")),
        );
        unproved
    }

    /// Records whether building an object can fail, and reports it when
    /// nothing handles it. Entity writes (`create`, `update`'s `set:`)
    /// follow the same rule and are checked with area 6.
    fn object_built(&mut self, record: &ast::RecordExpr, shown: &str, unproved: Vec<String>) {
        let at = node_key(record.syntax());
        if unproved.is_empty() {
            self.facts.constructions.insert(at, super::Fallible::Never);
            return;
        }
        self.facts
            .constructions
            .insert(at, super::Fallible::Construction);
        if self.handled.contains(&at) {
            return;
        }
        let listed: Vec<String> = unproved.iter().map(|f| format!("`{f}`")).collect();
        let span = super::line_span(self.path, record.syntax());
        self.flow.open = true;
        self.out.push(
            Diagnostic::new(
                &rule::FAILURE_ATTEMPT_REQUIRED,
                format!("Building `{shown}` can fail at runtime, and nothing handles it"),
            )
            .at(span, "checked when it runs")
            .reason(format!(
                "{} adds rules of its own, which run as the object is built, and the declared ranges don't prove the value follows them.",
                crate::and_list(&listed)
            ))
            .next(format!(
                "Write `attempt {shown} {{ ... }} invalid: SomeFailure` naming an `invalid_value` failure"
            )),
        );
    }

    /// A lower-case name used as a value: a local, or a mistake.
    pub(super) fn name_value(&mut self, name: &ast::NameRef) -> Ty {
        let written = text(name);
        if starts_upper(&written) {
            // A type or a failure as a value: resolved here, and the type
            // checks have nothing to add (area 1).
            let expr = Expr::NameRef(name.clone());
            self.resolved_by(|names, out| names.value_name(&expr, out));
            return Ty::Error;
        }
        match self.read(name) {
            Some(ty) => ty,
            None => {
                self.unknown_value(name);
                Ty::Error
            }
        }
    }

    /// Whether a lower-case name before a dot names an owner rather than a
    /// value: a standard or generated namespace, a domain, a library, a
    /// connection or a schedule (area 1 resolves those).
    pub(super) fn is_qualifier(&self, name: &str) -> bool {
        StdNames::get().namespace(name).is_some()
            || self.symbols.is_domain(name)
            || self.symbols.is_connection(name)
            || self.symbols.library(name).is_some()
            || self
                .symbols
                .declared(name)
                .iter()
                .any(|symbol| symbol.kind == ItemKind::Schedule)
    }

    /// `type.unknown_value`: a name that isn't a local (area 3; area 14's
    /// example message).
    pub(super) fn unknown_value(&mut self, name: &ast::NameRef) {
        self.unknown_name = true;
        let written = text(name);
        let span = self.span(name);
        let callable = self.symbols.get(&written).filter(|symbol| {
            matches!(
                symbol.kind,
                ItemKind::Function | ItemKind::Action | ItemKind::Query | ItemKind::Fixture
            )
        });
        if let Some(symbol) = callable {
            let word = symbol.kind.word();
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_UNKNOWN_VALUE,
                    format!("`{written}` is {}, not a value", symbol.kind.with_a()),
                )
                .at(span, format!("{word} used as a value"))
                .reason("Functions and actions aren't values: they are called, and only a standard-library collection operation takes an inline function, `item -> expression`.")
                .next(format!("Call it, as in `{written}(...)`")),
            );
            return;
        }
        let names = self.names_in_scope();
        let near = nearest(&written, names.iter().map(String::as_str)).map(str::to_string);
        let problem = Diagnostic::new(
            &rule::TYPE_UNKNOWN_VALUE,
            format!("`{written}` isn't defined in this scope"),
        )
        .at(span.clone(), "not defined")
        .reason(format!(
            "Jadpo could not find this name.{}",
            did_you_mean(near.as_deref())
        ));
        self.out.push(match near {
            Some(near) => problem
                .next(format!("Use `{near}` instead of `{written}`"))
                .edit(span, near),
            None => problem.next(format!(
                "Declare `{written}` with `var`, or use a name in scope"
            )),
        });
    }

    /// A field of a variant's data, bound by a `match` arm.
    pub(super) fn variant_field(
        &mut self,
        host: &str,
        variant: &str,
        field: &str,
        span: Span,
    ) -> Ty {
        let fields: Vec<FieldDef> = self
            .table
            .variants(host)
            .and_then(|variants| variants.iter().find(|v| v.name == variant))
            .and_then(|v| v.fields.clone())
            .unwrap_or_default();
        if fields.iter().any(|f| f.name == field) {
            return Ty::Field(
                host.to_string(),
                vec![variant.to_string(), field.to_string()],
            );
        }
        self.unknown_field(&format!("{host}.{variant}"), field, &fields, span, true);
        Ty::Error
    }
}

/// A field type as written: `Badge.name`.
fn name_of(ty: &Ty) -> String {
    ty.to_string()
}
