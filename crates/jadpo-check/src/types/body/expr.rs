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
use super::super::table::{Def, FieldDef, TypeView};
use super::super::ty::{Prim, Ty};
use super::{Body, did_you_mean, is_literal, node_key, text};
use crate::case::starts_upper;
use crate::std_lib::{Namespace, StdNames};
use crate::tree::{self, dotted};

/// The generated namespace whose one member is the time (area 10).
const CLOCK: &str = "clock";
use crate::config::CONFIG;

/// Which fields a block must give.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Required {
    /// Every field: an object, a variant's data, a created record.
    All,
    /// Every field an `Input` can't leave out.
    Input,
    /// None: an update's `set:` gives the fields it changes.
    None,
}

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
        // `Range { ... }` where a `Range<T>` is expected: its ends are `T?`.
        if let Expr::RecordExpr(record) = expr
            && let Some(head) = record.expr()
            && tree::written(head.syntax()) == jadpo_std::RANGE
            && let Ty::Opaque(range, args) = self.table.structure(expected)
            && range == jadpo_std::RANGE
            && let [item] = args.as_slice()
            && !self.table.is_nullable(expected)
        {
            self.resolved_by(|names, out| crate::resolve::record_head(names, record, out));
            let actual = self.range_literal(record, Some(item.clone()));
            self.facts.types.insert(node_key(expr.syntax()), actual);
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
                    } else if is_set {
                        self.set_length(expr, expected, &items);
                    }
                    return;
                }
                // `[]` where a map is expected is an empty map (area 19,
                // question 8), whose length a named map's rules check.
                Ty::Map(..) if list.exprs().next().is_none() => {
                    if let Err(problem) =
                        literal::check(self.table, expected, &Const::List(Vec::new()))
                    {
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

    /// A set literal's `min_length`: equal items collapse, so computed
    /// ones may hold fewer values than are written. It holds at least its
    /// distinct literal items (repeated ones are already an error), and at
    /// least one when it has any items at all; the length is proved only
    /// from that.
    fn set_length(&mut self, expr: &Expr, expected: &Ty, items: &[Const]) {
        if !items.iter().any(|item| matches!(item, Const::Other)) {
            return;
        }
        let literals = items
            .iter()
            .filter(|item| !matches!(item, Const::Other))
            .count();
        let fewest = literals.max(usize::from(!items.is_empty())) as u64;
        let Some((host, needed)) = self
            .table
            .rule_chain(expected)
            .into_iter()
            .filter_map(|(host, rules)| rules.min_length.map(|n| (host, n)))
            .max_by_key(|(_, n)| *n)
        else {
            return;
        };
        if needed <= fewest {
            return;
        }
        let written = tree::written(expr.syntax());
        let span = self.span(expr);
        let at_least = if fewest == 1 {
            "1 value".to_string()
        } else {
            format!("{fewest} values")
        };
        self.out.push(
            Diagnostic::new(
                &rule::TYPE_INVALID_LITERAL,
                format!("`{written}` might not be a valid `{expected}`"),
            )
            .at(span, format!("might break `min_length: {needed}`"))
            .reason(format!(
                "A set holds each value once, so when its computed items are equal it holds as few as {at_least}, and `{host}` declares `min_length: {needed}`. A literal is checked against every rule while compiling, so its length must hold however its items turn out."
            ))
            .next(format!(
                "Write at least {needed} different literals in the set, or drop `min_length` from `{host}`"
            )),
        );
    }

    /// Checks `expr` where `expected` is required, and records the type it
    /// takes there when nothing else did: a literal's is the type it goes
    /// into.
    pub(super) fn check(&mut self, expr: &Expr, expected: &Ty) {
        self.facts
            .expected
            .insert(node_key(expr.syntax()), expected.clone());
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
        // A number where a quantity goes needs its unit (area 19).
        if let Some(Prim::Quantity(kind)) = self.table.representation(expected)
            && self
                .table
                .representation(actual)
                .is_some_and(Prim::is_number)
            && !self.table.is_nullable(actual)
        {
            self.out.push(literal::unit_required(
                kind,
                &written,
                &expected.strip_nullable().to_string(),
                span,
            ));
            return;
        }
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
        } else if !actual.has_error()
            && !expected.has_unknown()
            && matches!(expected.strip_nullable(), Ty::Named(_) | Ty::Field(..))
            && self.table.assignable(expected, actual)
        {
            // A general value where a refinement of it is required: only a
            // named type or a field type is built from a value.
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
        if matches!(value, Const::Quantity(_)) && self.quantity_literal(expr, &value).is_error() {
            return;
        }
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

    /// A quantity literal's type, its unit's kind; `Error` once a suffix
    /// that names no unit is reported, once however often it is read
    /// (area 19, `type.unit_unknown`).
    pub(super) fn quantity_literal(&mut self, expr: &Expr, value: &Const) -> Ty {
        if let Some((unit, _)) = value.quantity() {
            return Ty::Prim(Prim::Quantity(unit.kind));
        }
        let span = self.span(expr);
        let reported = self.out.iter().any(|found| {
            found.rule.id == rule::TYPE_UNIT_UNKNOWN.id
                && found
                    .primary
                    .as_ref()
                    .is_some_and(|label| label.span == span)
        });
        if !reported {
            let written = tree::written(expr.syntax());
            self.out.push(literal::unit_unknown(&written, span));
        }
        Ty::Error
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
                value @ Const::Quantity(_) => self.quantity_literal(expr, &value),
                Const::Percentage(_) => Ty::Prim(Prim::Percentage),
                Const::None => Ty::None,
                _ => Ty::Error,
            },
            Expr::ParenExpr(paren) => paren.expr().map_or(Ty::Error, |inner| self.infer(&inner)),
            Expr::PrefixExpr(prefix) if is_literal(expr) => match Const::from_expr(expr) {
                Const::Number { decimal: true, .. } => Ty::Prim(Prim::Decimal),
                Const::Number { .. } => Ty::Prim(Prim::Int),
                value @ Const::Quantity(_) => self.quantity_literal(expr, &value),
                Const::Percentage(_) => Ty::Prim(Prim::Percentage),
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
            let clean = self.resolved_by(|names, out| names.value_name(expr, out));
            if let Some(ty) = self.stored_field(&head, &rest, expr) {
                return ty;
            }
            return self.type_member(&first, &rest, expr, clean);
        }
        if StdNames::get().namespace(&head) == Some(Namespace::Generated) {
            return self.generated_value(&head, &rest, expr);
        }
        if let [member] = rest.as_slice()
            && !self.is_local(&head)
            && self.symbols.library(&head).is_some()
            && let Some(ty) = self.constant_read(Some(&head), &text(member), expr.syntax())
        {
            return ty;
        }
        let Some(mut receiver) = self.read(&first) else {
            if !self.is_qualifier(&head) {
                self.unknown_value(&first);
            } else {
                let written = tree::written(expr.syntax());
                let span = self.span(expr);
                self.out
                    .push(crate::not_value::member_diagnostic(&written, span));
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
        if head == CONFIG {
            return self.setting_read(rest, expr);
        }
        match members.as_slice() {
            [member] if StdNames::get().generated_member(head, member) => {
                if head == CLOCK {
                    self.clock_read(span, &written);
                    self.facts.effects.insert(node_key(expr.syntax()));
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

    /// `config.<name>`: the setting's type. Anyone may read one, functions
    /// included, because a setting cannot change while the app runs (area
    /// 10); a name that is no setting, and a secret read where it could
    /// leave the server, are reported once per file by
    /// [`crate::config::use_problems`].
    fn setting_read(&mut self, rest: &[ast::NameRef], expr: &Expr) -> Ty {
        let Some(first) = rest.first() else {
            return Ty::Error;
        };
        let name = text(first);
        let fact = crate::config::setting_fact(
            self.db,
            self.project,
            crate::lookups::Name::new(self.db, name.clone()),
        );
        let Some(fact) = fact else {
            return Ty::Error;
        };
        let mut ty = fact.ty.clone();
        // `config.name.field` reads a field of the setting's value.
        let fields: Vec<ast::FieldExpr> = expr
            .syntax()
            .descendants()
            .filter_map(ast::FieldExpr::cast)
            .collect();
        for (field, node) in rest.iter().zip(fields.iter().rev()).skip(1) {
            if ty.is_error() {
                break;
            }
            ty = self.select(&ty, field, node);
        }
        if rest.len() == 1 {
            self.facts.settings.insert(node_key(expr.syntax()), name);
        }
        ty
    }

    /// `clock.now` is an effect only actions have (areas 3 and 10).
    pub(super) fn clock_read(&mut self, span: jadpo_diag::Span, written: &str) {
        if !self.context.is_pure() {
            return;
        }
        let caller = self.context.with_a();
        // An operation that reads the clock is called, as
        // `crypto.verify_jwt` is (area 19, chunk 5.5r); `clock.now` is read.
        let verb = if written.starts_with("clock.") {
            "read"
        } else {
            "call"
        };
        self.out.push(
            Diagnostic::new(
                &rule::TYPE_CLOCK_CONTEXT,
                format!("{} can't {verb} `{written}`", super::capital(caller)),
            )
            .at(span, "reads the clock")
            .reason(format!(
                "{} is pure: it gives the same result for the same arguments, so the time it needs arrives as a parameter; only actions read the clock.",
                super::capital(caller)
            ))
            .next("Pass the time in as a parameter, read in the action that calls this"),
        );
    }

    /// A `random` operation reads the generator, an effect only actions
    /// have, as the clock is (area 19, question 19).
    pub(super) fn random_read(&mut self, span: jadpo_diag::Span, written: &str) {
        if !self.context.is_pure() {
            return;
        }
        let caller = self.context.with_a();
        self.out.push(
            Diagnostic::new(
                &rule::TYPE_RANDOM_CONTEXT,
                format!("{} can't call `{written}`", super::capital(caller)),
            )
            .at(span, "reads the generator")
            .reason(format!(
                "{} is pure: it gives the same result for the same arguments, so a random value it needs arrives as a parameter; only actions read the generator, which a test seeds so that it repeats.",
                super::capital(caller)
            ))
            .next("Pass the value in as a parameter, made in the action that calls this"),
        );
    }

    /// `Type.member` as a value: an enum's variant.
    pub(super) fn type_member(
        &mut self,
        first: &ast::NameRef,
        rest: &[ast::NameRef],
        expr: &Expr,
        resolved: bool,
    ) -> Ty {
        let host = &text(first);
        if host == super::super::ty::TIME_FORMAT {
            // A built-in format is named only in `format:` (area 10).
            let written = tree::written(expr.syntax());
            let span = self.span(expr);
            self.out
                .push(super::super::formats::not_a_value(&written, span, false));
            return Ty::Error;
        }
        let Some(variants) = self.table.variants(host) else {
            // `Placed.level`: a type has no members to read as a value.
            if resolved {
                let span = self.span(first);
                self.declaration_as_value(host, span);
            }
            return Ty::Error;
        };
        let [variant] = rest else {
            return Ty::Error;
        };
        let name = text(variant);
        // `Enum.all` (area 19, question 22): every variant, in the enum's
        // order, for an enum whose variants carry no data; a closed list's
        // retired codes aren't offered.
        if name == "all" && !variants.iter().any(|v| v.name == name) {
            if variants.iter().any(|v| v.fields.is_some()) {
                let span = self.span(variant);
                let shown = crate::types::shown(host);
                self.out.push(
                    Diagnostic::new(
                        &rule::TYPE_UNKNOWN_ENUM_VARIANT,
                        format!("`{shown}` has no `all`"),
                    )
                    .at(span, "some variants carry data")
                    .reason(format!(
                        "`.all` lists an enum's variants as values, and only an enum whose variants carry no data has one value for each; a variant of `{shown}` carries fields, given when it is built."
                    ))
                    .next(format!("List the values of `{shown}` that are needed")),
                );
                return Ty::Error;
            }
            let entry = jadpo_std::standard_type(host);
            let listed: Vec<String> = variants
                .iter()
                .filter(|v| !entry.is_some_and(|entry| entry.is_retired(&v.name)))
                .map(|v| v.name.clone())
                .collect();
            self.facts
                .all_variants
                .insert(node_key(expr.syntax()), (host.to_string(), listed));
            return Ty::List(Box::new(Ty::Named(self.table.canonical(host))));
        }
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
                self.retired_variant(host, &found.name, expr);
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
                Ty::Named(self.table.canonical(host))
            }
        }
    }

    /// A closed list's withdrawn code written as a value (area 19, question
    /// 22): stored data still reads it, but new code doesn't write it,
    /// unless a decline gives the reason, such as reading old records.
    fn retired_variant(&mut self, host: &str, variant: &str, expr: &Expr) {
        let Some(list) = jadpo_std::standard::closed_list(host) else {
            return;
        };
        let Some(english) = jadpo_std::standard::retired(host, variant) else {
            return;
        };
        let code = jadpo_std::standard_type(host)
            .and_then(jadpo_std::StandardType::codes)
            .and_then(|codes| codes.iter().find(|(name, _)| *name == variant))
            .map_or(variant, |(_, code)| *code);
        let named = if english.is_empty() {
            String::new()
        } else {
            format!(" ({english})")
        };
        let span = self.span(expr);
        self.out.push(
            Diagnostic::new(
                &rule::TYPE_VARIANT_RETIRED,
                format!("`{host}.{variant}` is a retired code"),
            )
            .at(span, "only read")
            .reason(format!(
                "{} withdrew `{code}`{named}. `{host}.{variant}` stays so that stored data and old messages still read, but new code doesn't write it.",
                list.authority
            ))
            .next("Use the code that replaced it"),
        );
    }

    pub(super) fn unknown_variant(&mut self, host: &str, variant: &ast::NameRef) {
        let name = text(variant);
        let span = self.span(variant);
        let names: Vec<String> = self
            .table
            .variants(host)
            .map(|variants| variants.iter().map(|v| v.name.clone()).collect())
            .unwrap_or_default();
        let host = crate::types::shown(host);
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
        object_of(self.table, ty)
    }

    /// Reads field `name` from a value of type `receiver`.
    pub(super) fn select(
        &mut self,
        receiver: &Ty,
        name: &ast::NameRef,
        node: &ast::FieldExpr,
    ) -> Ty {
        let field = text(name);
        let span = self.span(name);
        // A page of a query's results (area 6, "Order and pages").
        if let Ty::Opaque(page, args) = receiver
            && page == PAGE
            && let [item] = args.as_slice()
        {
            let ty = match field.as_str() {
                "items" => Ty::List(Box::new(item.clone())),
                "next" => Ty::Prim(Prim::PageCursor).nullable(),
                _ => {
                    self.unknown_field(PAGE, &field, &page_fields(item), span, false);
                    return Ty::Error;
                }
            };
            self.facts
                .fields
                .insert(node_key(node.syntax()), (PAGE.to_string(), vec![field]));
            return ty;
        }
        // The instants a `temporal` bounds call gives (area 10), or a field
        // declared `InstantRange` (area 19, the standard types).
        let mut through = receiver.strip_nullable().clone();
        for _ in 0..64 {
            if !matches!(through, Ty::Field(..)) || self.table.is_nullable(&through) {
                break;
            }
            match self.table.parent(&through) {
                Some(parent) => through = parent,
                None => break,
            }
        }
        // A token or a key as `sessions.tokens` or `Service.keys` lists it
        // (area 8), or the problem a failure mapped from `json.read`'s
        // `mismatch` receives (area 19).
        if let Ty::Opaque(issued, args) = &through
            && args.is_empty()
            && let Some(object) = jadpo_std::http::builtin_object_fields(issued)
        {
            let found = object.iter().find(|(name, _)| *name == field);
            let Some((_, builtin)) = found else {
                let fields: Vec<FieldDef> = object
                    .iter()
                    .map(|(name, builtin)| plain_field(name, Ty::Prim(*builtin)))
                    .collect();
                self.unknown_field(issued, &field, &fields, span, false);
                return Ty::Error;
            };
            self.facts
                .fields
                .insert(node_key(node.syntax()), (issued.clone(), vec![field]));
            return Ty::Prim(*builtin);
        }
        // A `Range<T>`'s ends, each `none` when open (area 19).
        if let Ty::Opaque(range, args) = &through
            && range == jadpo_std::RANGE
            && let [item] = args.as_slice()
        {
            if !jadpo_std::RANGE_FIELDS.contains(&field.as_str()) {
                let fields: Vec<FieldDef> = jadpo_std::RANGE_FIELDS
                    .iter()
                    .map(|name| plain_field(name, item.clone().nullable()))
                    .collect();
                self.unknown_field(range, &field, &fields, span, false);
                return Ty::Error;
            }
            self.facts
                .fields
                .insert(node_key(node.syntax()), (range.clone(), vec![field]));
            return item.clone().nullable();
        }
        // A standard object type's fields (area 19): an `InstantRange`'s
        // instants, a `LocalTime`'s hour, minute and second, each read as
        // what it is built on.
        if let Ty::Opaque(object, args) = &through
            && args.is_empty()
            && let Some(fields) = super::super::standard_fields(object)
        {
            let Some(found) = fields.iter().find(|f| f.name == field) else {
                self.unknown_field(object, &field, fields, span, false);
                return Ty::Error;
            };
            let declared = found.declared.clone();
            self.facts
                .fields
                .insert(node_key(node.syntax()), (object.clone(), vec![field]));
            return declared;
        }
        // A route test's response (area 13).
        if let Some(found) = self.response_field(receiver, &field) {
            return match found {
                Some(ty) => {
                    self.facts.fields.insert(
                        node_key(node.syntax()),
                        ("Response".to_string(), vec![field]),
                    );
                    ty
                }
                None => {
                    let fields = response_fields();
                    self.unknown_field("Response", &field, &fields, span, false);
                    Ty::Error
                }
            };
        }
        // A caller (area 8).
        if let Some(ty) = self.caller_field(receiver, name, node) {
            return ty;
        }
        // A type a later area defines has fields this check doesn't know yet.
        if receiver.has_error() || matches!(receiver.strip_nullable(), Ty::Opaque(..) | Ty::Var(_))
        {
            return Ty::Error;
        }
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
                let ty = self.table.field_type(&host, &[field]);
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
        let host = crate::types::shown(host);
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
        // `request { ... }` in a route test (area 13).
        if let Some(Expr::NameRef(head)) = record.expr()
            && text(&head) == "request"
            && self.find("request").is_none()
        {
            return self.request(record);
        }
        self.resolved_by(|names, out| crate::resolve::record_head(names, record, out));
        let Some(head) = record.expr() else {
            self.walk(|walk| walk.member_values(record.member_list()));
            return Ty::Error;
        };
        // `Todo.update(todo) { ... }`: an update with its block, written
        // without `attempt` (area 6).
        if let Expr::CallExpr(call) = &head
            && self.write_call(call).is_some()
        {
            self.pending_block = record.member_list();
            let ty = self.call(call, None);
            self.pending_block = None;
            return ty;
        }
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
                let required = if *base == FieldsBase::Input {
                    Required::Input
                } else {
                    Required::All
                };
                let before = self.out.len();
                self.fields_given(
                    record.member_list(),
                    &name,
                    &[],
                    &fields,
                    required,
                    head_span,
                    &[],
                );
                // The rules across its fields are read once every field is
                // known to be right (area 2, "Rules across fields").
                if self.out.len() == before {
                    self.object_rules(record, &name);
                } else {
                    self.object_built(record);
                }
                Ty::Named(self.table.canonical(&name))
            }
            // `Range { start: ..., end: ... }` (area 19), its ends' type
            // from its first end that isn't `none`.
            ([], None) if name == jadpo_std::RANGE => self.range_literal(record, None),
            // A standard object type (`InstantRange`, area 19): its fields
            // are its data entry's, each built on a built-in type.
            ([], None) if let Some(fields) = standard_object_fields(&name) => {
                self.facts.builds.insert(
                    node_key(record.syntax()),
                    super::facts::BuildFact::Object(name.clone()),
                );
                self.fields_given(
                    record.member_list(),
                    &name,
                    &[],
                    &fields,
                    Required::All,
                    head_span,
                    &[],
                );
                self.part_rules_hold(&name, record);
                self.event_rules_hold(&name, record);
                self.media_rules_hold(&name, record);
                self.order_rules_hold(&name, record);
                self.object_built(record);
                Ty::Opaque(name, Vec::new())
            }
            ([], Some(Def::Scalar { .. })) | ([], None)
                if Prim::from_name(&name).is_some() || self.table.def(&name).is_some() =>
            {
                self.not_record(record, &name, "scalar");
                self.walk(|walk| walk.member_values(record.member_list()));
                Ty::Named(self.table.canonical(&name))
            }
            ([], Some(Def::Enum { .. })) => {
                self.not_record(record, &name, "enum");
                self.walk(|walk| walk.member_values(record.member_list()));
                Ty::Named(self.table.canonical(&name))
            }
            (
                [operation],
                Some(Def::Fields {
                    base: FieldsBase::Entity,
                    ..
                }),
            ) if text(operation) == "create" => {
                let written = format!("{name}.create");
                if self.context.is_pure() {
                    self.persistence(head_span.clone(), &written);
                    self.walk(|walk| walk.member_values(record.member_list()));
                    return Ty::Error;
                }
                let ty = self.write(
                    &name,
                    jadpo_std::write::WriteOp::Create,
                    record.syntax(),
                    None,
                    record.member_list(),
                    head_span,
                    None,
                );
                self.unhandled_write(record.syntax(), &written);
                ty
            }
            ([operation], Some(_)) if text(operation) == "create" => {
                self.write_not_entity(&name, jadpo_std::write::WriteOp::Create, head_span);
                self.walk(|walk| walk.member_values(record.member_list()));
                Ty::Error
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
                        self.fields_given(
                            record.member_list(),
                            &name,
                            &[variant_name],
                            &fields,
                            Required::All,
                            head_span,
                            &[],
                        );
                        self.object_built(record);
                    }
                    None => {
                        self.not_record(record, &format!("{name}.{variant_name}"), "plain variant");
                        self.walk(|walk| walk.member_values(record.member_list()));
                    }
                }
                Ty::Named(self.table.canonical(&name))
            }
            // A built-in object only the runtime builds: an issued token or
            // key (area 8), or the problem `json.read`'s `mismatch` gives
            // (area 19), which so never holds a value a program wrote.
            ([], None) if jadpo_std::http::builtin_object_fields(&name).is_some() => {
                let reason = if name == jadpo_std::http::DECODE_PROBLEM {
                    format!(
                        "Only the runtime builds a `{name}`: a failure mapped from `json.read`'s `mismatch` receives the first problem the document had, its field's path and its rule, never a value."
                    )
                } else {
                    format!(
                        "Only the runtime builds an `{name}`: it lists what was issued, never the secret itself."
                    )
                };
                let next = if name == jadpo_std::http::DECODE_PROBLEM {
                    "Map `json.read`'s `mismatch` to a failure that declares `problem: DecodeProblem`, and remove this value"
                } else {
                    "Read one from the list the runtime gives, and remove this value"
                };
                self.out.push(
                    Diagnostic::new(
                        &rule::TYPE_NOT_RECORD,
                        format!("`{name}` isn't built with `{{ ... }}`"),
                    )
                    .at(self.span(record), "only the runtime builds it")
                    .reason(reason)
                    .next(next),
                );
                self.walk(|walk| walk.member_values(record.member_list()));
                Ty::Error
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
    /// field adds rules of its own, the value must already be known to
    /// follow them: an object literal never fails (area 2, "Construction
    /// failures by rule").
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::too_many_arguments)]
    pub(super) fn fields_given(
        &mut self,
        list: Option<ast::MemberList>,
        host: &str,
        variant: &[String],
        fields: &[FieldDef],
        required: Required,
        head_span: Span,
        skip: &[&str],
    ) {
        let is_variant = !variant.is_empty();
        let shown = if is_variant {
            format!("{host}.{}", variant.join("."))
        } else {
            host.to_string()
        };
        let mut given: BTreeMap<String, Span> = BTreeMap::new();
        for item in list.iter().flat_map(|list| list.member_items()) {
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
            // Already reported by the caller; its value is still walked.
            if skip.contains(&name.as_str()) {
                if let Some(value) = value {
                    self.infer(&value);
                }
                continue;
            }
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
            // A standard object type's field has no field type of its own:
            // it is the type its data entry names, or, for a whole number
            // with a range, the field itself, whose rules are that range.
            let field_ty = if self.table.def(host).is_none() && field.rules == Rules::default() {
                field.declared.clone()
            } else {
                self.table.field_type(host, &path)
            };
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
                {
                    let verdict = self.verdict(&value, &actual, &field_ty);
                    if !verdict.breakable.is_empty() {
                        self.field_unproved(&value, &field_ty, &verdict.breakable);
                    }
                }
            }
        }
        let missing: Vec<String> = fields
            .iter()
            .filter(|f| !given.contains_key(&f.name))
            .filter(|f| match required {
                Required::All => true,
                Required::Input => !(f.optional || f.default.is_some()),
                Required::None => false,
            })
            .map(|f| format!("`{}`", f.name))
            .collect();
        if missing.is_empty() {
            return;
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
    }

    /// Records that building an object never fails: each field's value is
    /// already known to fit. Entity writes (`create`, `update`'s `set:`)
    /// are checked with area 6.
    /// `Range { start: ..., end: ... }` (area 19): both ends given, each a
    /// `T` or `none` for an open end. `item` is `T` where the literal's
    /// place says; otherwise its first end that isn't `none` gives it.
    pub(super) fn range_literal(&mut self, record: &ast::RecordExpr, item: Option<Ty>) -> Ty {
        let head_span = match record.expr() {
            Some(head) => self.span(&head),
            None => self.span(record),
        };
        let item = match item {
            Some(item) => item,
            None => {
                let first = tree::members(record.member_list())
                    .into_iter()
                    .filter(|(name, _)| {
                        tree::name_text(name)
                            .is_some_and(|name| jadpo_std::RANGE_FIELDS.contains(&name.as_str()))
                    })
                    .find_map(|(_, member)| {
                        let value = member.value()?;
                        (Const::from_expr(&value) != Const::None).then_some(value)
                    });
                let item = match first {
                    Some(value) => self.infer(&value).strip_nullable().clone(),
                    // Two open ends hold everything, of any type.
                    None => Ty::Error,
                };
                if !item.has_error() && !self.range_ordered(&item) {
                    self.out.push(range_unordered(&item, head_span.clone()));
                    return Ty::Error;
                }
                item
            }
        };
        let fields: Vec<FieldDef> = jadpo_std::RANGE_FIELDS
            .iter()
            .map(|name| plain_field(name, item.clone().nullable()))
            .collect();
        self.facts.builds.insert(
            node_key(record.syntax()),
            super::facts::BuildFact::Object(jadpo_std::RANGE.to_string()),
        );
        self.fields_given(
            record.member_list(),
            jadpo_std::RANGE,
            &[],
            &fields,
            Required::All,
            head_span,
            &[],
        );
        self.object_built(record);
        Ty::Opaque(jadpo_std::RANGE.to_string(), vec![item])
    }

    /// Whether a range's ends may be `item`: a type built on one that `<`
    /// orders (area 19, "Ranges").
    pub(super) fn range_ordered(&self, item: &Ty) -> bool {
        !self.table.is_nullable(item)
            && self
                .table
                .representation(item)
                .is_some_and(|prim| jadpo_std::RANGE_ITEMS.contains(&prim))
    }

    /// A standard object's rules across its parts (area 19, chunk 5.5l2),
    /// proved where its literal writes both parts: an address's postal
    /// code written as text and its country as a variant. One built from
    /// values waits on cross-field rules (chunk 5.5t).
    fn part_rules_hold(&mut self, name: &str, record: &ast::RecordExpr) {
        let Some(jadpo_std::standard::Holds::Object(parts)) =
            jadpo_std::standard_type(name).map(|entry| entry.holds)
        else {
            return;
        };
        let members = tree::members(record.member_list());
        let value_of = |part: &str| {
            members
                .iter()
                .find(|(field, _)| tree::name_text(field).as_deref() == Some(part))
                .and_then(|(_, member)| member.value())
        };
        'rules: for rule in jadpo_std::standard::part_rules(name) {
            let Some(written) = value_of(rule.part) else {
                continue;
            };
            let Const::Str(given) = Const::from_expr(&written) else {
                continue;
            };
            // Text that breaks the part's own rules is reported by them.
            let own = parts
                .iter()
                .find(|(field, _)| *field == rule.part)
                .map(|(_, part)| part.rules());
            if own.is_some_and(|rules| rules.check_text(&given).is_err()) {
                continue;
            }
            // Each part the rule reads, written out as a variant of its
            // enum: its code, and how the code names it.
            let mut codes = Vec::new();
            let mut named = Vec::new();
            for by in rule.by {
                let Some(value) = value_of(by) else {
                    continue 'rules;
                };
                let Some(jadpo_std::Part::Standard(list)) = parts
                    .iter()
                    .find(|(field, _)| field == by)
                    .map(|(_, part)| part.required().0)
                else {
                    continue 'rules;
                };
                let Some((head, rest)) = dotted(&value) else {
                    continue 'rules;
                };
                let ([variant], true) = (rest.as_slice(), text(&head) == list) else {
                    continue 'rules;
                };
                let variant = text(variant);
                let Some(code) = jadpo_std::standard_type(list)
                    .and_then(jadpo_std::StandardType::codes)
                    .and_then(|codes| codes.iter().find(|(listed, _)| *listed == variant))
                    .map(|(_, code)| *code)
                else {
                    continue 'rules;
                };
                codes.push(code);
                named.push(format!("`{list}.{variant}`"));
            }
            if (rule.fits)(&given, &codes) {
                continue;
            }
            let shown = tree::written(written.syntax());
            let by = rule
                .by
                .iter()
                .map(|by| format!("`{by}`"))
                .collect::<Vec<_>>()
                .join(" and ");
            let of = named.join(" and ");
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_INVALID_LITERAL,
                    format!("`{shown}` isn't {}", rule.says),
                )
                .at(self.span(&written), format!("not {} of {of}", rule.noun))
                .reason(format!(
                    "A `{name}`'s `{}` is in the form {} of its {by} writes, and a literal is checked while compiling.",
                    rule.part, rule.authority
                ))
                .next(format!("Write {} of {of} in its canonical text, or change {by}", rule.noun)),
            );
        }
    }

    fn object_built(&mut self, record: &ast::RecordExpr) {
        self.facts
            .constructions
            .insert(node_key(record.syntax()), super::Fallible::Never);
    }

    /// A field value neither the declared ranges nor the conditions around
    /// it prove follows the rules its field adds: it is built first, on its own line, where its failures
    /// are mapped.
    fn field_unproved(&mut self, value: &Expr, field_ty: &Ty, breakable: &[String]) {
        let field = name_of(field_ty);
        let local = match field_ty {
            Ty::Field(_, path) => path.last().cloned().unwrap_or_default(),
            _ => "value".to_string(),
        };
        let rules = self.rules_of(field_ty);
        let written_rules: Vec<String> = breakable
            .iter()
            .map(|key| {
                rules
                    .iter()
                    .find(|(k, _)| k == key)
                    .map_or_else(|| format!("`{key}`"), |(_, rule)| format!("`{rule}`"))
            })
            .collect();
        let shown = tree::written(value.syntax());
        let first = breakable.first().cloned().unwrap_or_default();
        let span = self.span(value);
        self.flow.open = true;
        self.out.push(
            Diagnostic::new(
                &rule::TYPE_FIELD_UNPROVED,
                format!("`{shown}` might not fit `{field}`"),
            )
            .at(span, format!("can break {}", crate::and_list(&written_rules)))
            .reason(format!(
                "An object literal never fails, so each field gets a value already known to fit; neither the declared ranges nor a condition before it prove this one follows {}, which `{field}` adds.",
                crate::and_list(&written_rules)
            ))
            .next(format!(
                "Build it first on its own line, `var {local}: {field} = attempt {field}({shown}) {{ {first}: SomeFailure }}`, then write `{local}: {local}` in the object"
            )),
        );
    }

    /// A declaration named where a value is read (area 3): reported, and
    /// whether `name` was one.
    pub(super) fn declaration_as_value(&mut self, name: &str, span: Span) -> bool {
        let Some(named) = crate::not_value::classify(self.symbols, self.table, name) else {
            return false;
        };
        self.out
            .push(crate::not_value::diagnostic(name, &named, span));
        true
    }

    /// A lower-case name used as a value: a local, or a mistake.
    pub(super) fn name_value(&mut self, name: &ast::NameRef) -> Ty {
        let written = text(name);
        // Python's `None` and `True`, SQL's `NULL`, unless the project
        // declares the name.
        if starts_upper(&written)
            && self.symbols.get(&written).is_none()
            && !StdNames::get().is_type(&written)
            && let Some(problem) = crate::foreign_value(&written, self.span(name))
        {
            self.unknown_name = true;
            self.out.push(problem);
            return Ty::Error;
        }
        if starts_upper(&written) {
            // A type, a failure or an entity is no value (area 3); a format
            // is named only in `format:` (area 10). A name that doesn't
            // resolve is reported by resolution (area 1).
            let expr = Expr::NameRef(name.clone());
            let resolved = self.resolved_by(|names, out| names.value_name(&expr, out));
            let span = self.span(name);
            if let Some(super::super::table::Def::Format { .. }) = self.table.def(&written) {
                self.out
                    .push(super::super::formats::not_a_value(&written, span, false));
            } else if resolved {
                self.declaration_as_value(&written, span);
            }
            return Ty::Error;
        }
        match self.read(name) {
            Some(ty) => ty,
            // A library's functions and constants read its constants
            // bare, as they call its functions.
            None if let Some(library) = self.library.clone()
                && let Some(ty) = self.constant_read(Some(&library), &written, name.syntax()) =>
            {
                ty
            }
            None if let Some(ty) = self.constant_read(None, &written, name.syntax()) => ty,
            None => {
                // `null`, `nil` and `undefined` from other languages.
                if let Some(problem) = crate::foreign_value(&written, self.span(name)) {
                    self.unknown_name = true;
                    self.out.push(problem);
                } else {
                    self.unknown_value(name);
                }
                Ty::Error
            }
        }
    }

    /// A constant's value (area 3): `name` beside its declaration, or
    /// `library.name`. Code reads a constant it can see: one in its own
    /// domain, `shared/` or the root, or a public library's.
    pub(super) fn constant_read(
        &mut self,
        library: Option<&str>,
        name: &str,
        node: &jadpo_syntax::SyntaxNode,
    ) -> Option<Ty> {
        let visible = match library {
            Some(library) => self
                .symbols
                .library_function(library, name)
                .filter(|symbol| symbol.kind == ItemKind::Constant)
                .is_some_and(|symbol| symbol.visible_from(self.ownership)),
            None => self.symbols.declared(name).iter().any(|symbol| {
                symbol.kind == ItemKind::Constant
                    && symbol.library.is_none()
                    && symbol.visible_from(self.ownership)
            }),
        };
        if !visible {
            return None;
        }
        let ty = self.calls.constant_type(library, name)?.clone();
        self.facts.constants.insert(
            node_key(node),
            (library.map(str::to_string), name.to_string()),
        );
        Some(ty)
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
        if self.declaration_as_value(&written, span.clone()) {
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
            return self
                .table
                .field_type(host, &[variant.to_string(), field.to_string()]);
        }
        self.unknown_field(&format!("{host}.{variant}"), field, &fields, span, true);
        Ty::Error
    }
}

/// A field type as written: `Badge.name`.
fn name_of(ty: &Ty) -> String {
    ty.to_string()
}

/// The standard type a page query returns (area 6): `Page<T>`.
pub(crate) const PAGE: &str = "Page";

/// `Page<T>`'s fields, as the unknown-field error lists them.
/// A route test's response's fields, for messages.
fn response_fields() -> Vec<FieldDef> {
    ["status", "code", "output"]
        .into_iter()
        .map(|name| FieldDef {
            name: name.to_string(),
            declared: Ty::Error,
            rules: Rules::default(),
            optional: false,
            default: None,
            header: None,
            copied: false,
            replaces: None,
            generated: false,
        })
        .collect()
}

/// A built-in type's field, with nothing declared about it.
/// The fields of the standard object type `name` (area 19), from its data
/// entry, or `None` when it isn't one.
fn standard_object_fields(name: &str) -> Option<Vec<FieldDef>> {
    if !StdNames::get().is_opaque(name) {
        return None;
    }
    super::super::standard_fields(name).map(<[FieldDef]>::to_vec)
}

fn plain_field(name: &str, declared: Ty) -> FieldDef {
    FieldDef {
        name: name.to_string(),
        declared,
        rules: Rules::default(),
        optional: false,
        default: None,
        header: None,
        copied: false,
        replaces: None,
        generated: false,
    }
}

fn page_fields(item: &Ty) -> Vec<FieldDef> {
    [
        ("items", Ty::List(Box::new(item.clone()))),
        ("next", Ty::Prim(Prim::PageCursor).nullable()),
    ]
    .into_iter()
    .map(|(name, declared)| FieldDef {
        name: name.to_string(),
        declared,
        rules: Rules::default(),
        optional: false,
        default: None,
        header: None,
        copied: false,
        replaces: None,
        generated: false,
    })
    .collect()
}

/// The object a value's fields come from, following field types to their
/// declared types.
fn object_of(table: &dyn TypeView, ty: &Ty) -> Option<String> {
    let mut current = ty.strip_nullable().clone();
    for _ in 0..64 {
        match &current {
            Ty::Named(name) => {
                return table.fields(name).map(|_| name.clone());
            }
            Ty::Field(..) => current = table.parent(&current)?,
            _ => return None,
        }
    }
    None
}

/// Every field a value of type `receiver` has, with the type reading it
/// gives, in the order [`Body::select`] tries them: what the editor offers
/// after `value.` (area 14, "Completion"). Empty where `select` reads no
/// field: a value that may be `none`, a type a later area defines.
pub(crate) fn members_of(
    table: &dyn TypeView,
    identity: &crate::identity::Identity,
    receiver: &Ty,
) -> Vec<Member> {
    let plain = |name: &str, ty: Ty| Member {
        name: name.to_string(),
        declared: ty.clone(),
        ty,
    };
    let named = |fields: Vec<FieldDef>| -> Vec<Member> {
        fields
            .into_iter()
            .map(|f| plain(&f.name, f.declared))
            .collect()
    };
    if let Ty::Opaque(page, args) = receiver
        && page == PAGE
        && let [item] = args.as_slice()
    {
        return named(page_fields(item));
    }
    if let Ty::Opaque(issued, args) = receiver.strip_nullable()
        && args.is_empty()
        && let Some(object) = jadpo_std::http::builtin_object_fields(issued)
    {
        return object
            .iter()
            .map(|(name, builtin)| plain(name, Ty::Prim(*builtin)))
            .collect();
    }
    if let Ty::Opaque(range, args) = receiver.strip_nullable()
        && range == jadpo_std::RANGE
        && let [item] = args.as_slice()
    {
        return jadpo_std::RANGE_FIELDS
            .iter()
            .map(|name| plain(name, item.clone().nullable()))
            .collect();
    }
    if let Ty::Opaque(object, args) = receiver.strip_nullable()
        && args.is_empty()
        && let Some(fields) = super::super::standard_fields(object)
    {
        return fields
            .iter()
            .map(|field| plain(&field.name, field.declared.clone()))
            .collect();
    }
    if let Ty::Opaque(name, args) = receiver.strip_nullable()
        && name == super::request::RESPONSE
    {
        return super::request::response_members(args)
            .into_iter()
            .map(|(name, ty)| plain(name, ty))
            .collect();
    }
    if let Some(kind) = super::identity::caller_kind(receiver) {
        return crate::identity::PRINCIPAL_FIELDS
            .iter()
            .filter_map(|field| Some(plain(field, identity.field_type(kind, field)?)))
            .collect();
    }
    if receiver.has_error() || matches!(receiver.strip_nullable(), Ty::Opaque(..) | Ty::Var(_)) {
        return Vec::new();
    }
    if let Ty::Variant(host, variant) = receiver {
        let fields = table
            .variants(host)
            .and_then(|variants| variants.iter().find(|v| v.name == *variant))
            .and_then(|v| v.fields.clone())
            .unwrap_or_default();
        return fields
            .into_iter()
            .map(|f| Member {
                ty: table.field_type(host, &[variant.clone(), f.name.clone()]),
                name: f.name,
                declared: f.declared,
            })
            .collect();
    }
    if table.is_nullable(receiver) {
        return Vec::new();
    }
    let Some(host) = object_of(table, receiver) else {
        return Vec::new();
    };
    table
        .fields(&host)
        .unwrap_or(&[])
        .iter()
        .map(|f| {
            let ty = table.field_type(&host, std::slice::from_ref(&f.name));
            let ty = if f.optional { ty.nullable() } else { ty };
            Member {
                name: f.name.clone(),
                ty,
                declared: f.declared.clone(),
            }
        })
        .collect()
}

/// A field as completion offers it.
pub(crate) struct Member {
    pub name: String,
    /// The type reading it gives: `Note.body`, a refinement of its
    /// declared type.
    pub ty: Ty,
    /// The type it is declared with: `Text`.
    pub declared: Ty,
}

/// A range of `item`, which has no order (area 19, "Ranges").
pub(crate) fn range_unordered(item: &Ty, span: Span) -> Diagnostic {
    Diagnostic::new(
        &rule::TYPE_ORDERING_OPERAND,
        format!("A range of `{item}` has no order to hold its values in"),
    )
    .at(span, format!("its ends are `{item}`"))
    .reason(format!(
        "A range holds the values from its start up to its end, so its ends are a type `<` orders: a number, an `Instant`, a `CalendarDate` or a `Duration`, or a type built on one; `{item}` isn't."
    ))
    .next("Make the range of an ordered type")
}
