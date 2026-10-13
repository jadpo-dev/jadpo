//! Calls (areas 2 and 3): which callable a name means, what the caller may
//! call, how many arguments it takes and whether they are named, each
//! argument checked against its parameter, standard-library operations
//! with their type variables fixed at each call and their inline
//! functions, and constructors.

use std::collections::BTreeMap;

use jadpo_db::ItemKind;
use jadpo_diag::{Diagnostic, Span, nearest, rule};
use jadpo_syntax::ast::{self, AstNode, Expr};

use super::super::rules::Const;
use super::super::shape::FieldsBase;
use super::super::signatures::Sig;
use super::super::table::Def;
use super::super::ty::{Prim, Ty};
use super::{Body, CallKind, Context, Fallible, LocalKind, capital, is_literal, text};
use crate::case::starts_upper;
use crate::std_lib::{Namespace, Operation, StdNames};
use crate::tree::{self, dotted};

fn plural(n: usize, word: &str) -> String {
    if n == 1 {
        format!("1 {word}")
    } else {
        format!("{n} {word}s")
    }
}

impl Body<'_> {
    /// A call; `expected` is the type its position requires, which fixes a
    /// standard-library operation's type variables that its arguments
    /// don't.
    pub(super) fn call(&mut self, call: &ast::CallExpr, expected: Option<&Ty>) -> Ty {
        let (ty, kind) = self.call_kind(call, expected);
        self.last_call = kind;
        self.unhandled(call);
        ty
    }

    pub(super) fn call_kind(
        &mut self,
        call: &ast::CallExpr,
        expected: Option<&Ty>,
    ) -> (Ty, CallKind) {
        match call.expr() {
            Some(Expr::NameRef(name)) => {
                let callee = text(&name);
                if starts_upper(&callee) {
                    let ty = self.construct(&callee, None, call);
                    return (ty, CallKind::Pure(format!("{callee}(...)")));
                }
                if self.is_local(&callee) {
                    // Functions aren't values, so a local is never called;
                    // resolution reports it (area 1).
                    self.read(&callee);
                    self.args_only(call);
                    return (Ty::Error, CallKind::Other);
                }
                if let Some(library) = self.library.clone()
                    && let Some(sig) = self.calls.library.get(&(library, callee.clone())).cloned()
                {
                    self.args(&sig, call, &callee);
                    return (sig.result, CallKind::Pure(callee));
                }
                let Some(sig) = self.calls.plain.get(&callee).cloned() else {
                    self.args_only(call);
                    return (Ty::Error, CallKind::Other);
                };
                let kind = self.callable_kind(&callee);
                let span = self.span(&name);
                self.may_call(span, &callee, kind.as_ref());
                self.args(&sig, call, &callee);
                let call_kind = match kind {
                    Some(ItemKind::Function) => CallKind::Pure(callee),
                    _ => CallKind::Effect(callee),
                };
                (sig.result, call_kind)
            }
            Some(callee @ Expr::FieldExpr(_)) => self.dotted_call(&callee, call, expected),
            Some(other) => {
                self.infer(&other);
                self.args_only(call);
                (Ty::Error, CallKind::Other)
            }
            None => (Ty::Error, CallKind::Other),
        }
    }

    pub(super) fn dotted_call(
        &mut self,
        callee: &Expr,
        call: &ast::CallExpr,
        expected: Option<&Ty>,
    ) -> (Ty, CallKind) {
        let Some((first, rest)) = dotted(callee) else {
            self.infer(callee);
            self.args_only(call);
            return (Ty::Error, CallKind::Other);
        };
        let head = text(&first);
        let rest: Vec<String> = rest.iter().map(text).collect();
        let written = format!("{head}.{}", rest.join("."));
        if starts_upper(&head) {
            if self.table.field(&head, &rest).is_some() {
                let target = Ty::Field(head.clone(), rest.clone());
                let ty = self.construct(&written, Some(target), call);
                return (ty, CallKind::Pure(format!("{written}(...)")));
            }
            if self.callable_kind(&head) == Some(ItemKind::Entity) {
                // An entity's operations read and write stored data
                // (areas 5 and 6).
                if self.context.is_pure() {
                    let span = self.span(callee);
                    self.persistence(span, &written);
                }
                self.args_only(call);
                return (Ty::Error, CallKind::Effect(written));
            }
            self.args_only(call);
            return (Ty::Error, CallKind::Other);
        }
        if self.is_local(&head) {
            // Values have no methods; resolution and area 2 own `x.f(...)`.
            self.read(&head);
            self.args_only(call);
            return (Ty::Error, CallKind::Other);
        }
        let [member] = rest.as_slice() else {
            self.args_only(call);
            return (Ty::Error, CallKind::Effect(written));
        };
        let names = StdNames::get();
        if names.namespace(&head) == Some(Namespace::Standard) {
            if let Some(operation) = names.operation(&head, member) {
                let ty = self.std_call(operation, call, &written, expected);
                return (ty, CallKind::Pure(written));
            }
            self.args_only(call);
            return (Ty::Error, CallKind::Pure(written));
        }
        if self.symbols.library(&head).is_some() {
            return match self
                .calls
                .library
                .get(&(head.clone(), member.clone()))
                .cloned()
            {
                Some(sig) => {
                    self.args(&sig, call, &written);
                    (sig.result, CallKind::Pure(written))
                }
                None => {
                    self.args_only(call);
                    (Ty::Error, CallKind::Other)
                }
            };
        }
        if self.symbols.domains.contains(&head) {
            let kind = self.callable_kind(member);
            if let Some(sig) = self
                .calls
                .plain
                .get(member)
                .filter(|_| matches!(kind, Some(ItemKind::Action | ItemKind::Query)))
                .cloned()
            {
                let span = self.span(callee);
                self.may_call(span, &written, kind.as_ref());
                self.args(&sig, call, &written);
                return (sig.result, CallKind::Effect(written));
            }
        }
        self.args_only(call);
        (Ty::Error, CallKind::Effect(written))
    }

    /// The kind of the project declaration called `name`, not counting
    /// library functions.
    pub(super) fn callable_kind(&self, name: &str) -> Option<ItemKind> {
        self.symbols
            .declarations
            .get(name)?
            .iter()
            .find(|symbol| symbol.library.is_none())
            .map(|symbol| symbol.kind.clone())
    }

    /// Who may call whom (area 3): a function, or an inline function, calls
    /// only functions, library functions and the standard library.
    pub(super) fn may_call(&mut self, span: Span, callee: &str, kind: Option<&ItemKind>) {
        let Some(kind @ (ItemKind::Action | ItemKind::Query | ItemKind::Fixture)) = kind else {
            return;
        };
        if !self.context.is_pure() {
            return;
        }
        let caller = self.context.with_a();
        let word = kind.word();
        let next = match self.context {
            Context::Inline => format!("Call `{callee}` outside the inline function"),
            _ => format!(
                "Make the caller an action, or call `{callee}` from the action that uses this function"
            ),
        };
        self.out.push(
            Diagnostic::new(
                &rule::EFFECT_FUNCTION_CALLS_ACTION,
                format!("{} can't call {word} `{callee}`", capital(caller)),
            )
            .at(span, kind.with_a())
            .reason(format!(
                "{} is pure: it computes its result from its arguments alone, so it calls only functions, library functions and the standard library; {} may read and write data.",
                capital(caller),
                kind.with_a()
            ))
            .next(next),
        );
    }

    pub(super) fn persistence(&mut self, span: Span, written: &str) {
        let caller = self.context.with_a();
        self.out.push(
            Diagnostic::new(
                &rule::EFFECT_FUNCTION_PERSISTENCE,
                format!("{} can't call `{written}`", capital(caller)),
            )
            .at(span, "reads or writes stored data")
            .reason(format!(
                "{} is pure: it computes its result from its arguments alone, so reading and writing entities belongs in an action.",
                capital(caller)
            ))
            .next("Make the caller an action, or pass the data in as an argument"),
        );
    }

    pub(super) fn arguments(call: &ast::CallExpr) -> Vec<ast::Arg> {
        call.arg_list()
            .iter()
            .flat_map(|list| list.args())
            .collect()
    }

    pub(super) fn positional(call: &ast::CallExpr) -> Vec<(Option<String>, Expr)> {
        Self::arguments(call)
            .iter()
            .filter_map(|arg| Some((Self::arg_name(arg), arg.expr()?)))
            .collect()
    }

    pub(super) fn arg_name(arg: &ast::Arg) -> Option<String> {
        arg.name().as_ref().and_then(tree::name_text)
    }

    /// Types every argument without an expected type.
    pub(super) fn args_only(&mut self, call: &ast::CallExpr) {
        for arg in Self::arguments(call) {
            if let Some(value) = arg.expr() {
                self.infer(&value);
            }
        }
    }

    pub(super) fn arg_list_span(&self, call: &ast::CallExpr) -> Span {
        call.arg_list()
            .map_or_else(|| self.span(call), |list| self.span(&list))
    }

    /// Checks arguments against one of the project's signatures: given in
    /// order, without names, one for each parameter (area 3).
    pub(super) fn args(&mut self, sig: &Sig, call: &ast::CallExpr, callee: &str) {
        let args = Self::arguments(call);
        let named: Vec<&ast::Arg> = args.iter().filter(|arg| arg.name().is_some()).collect();
        if let Some(first) = named.first()
            && let Some(name) = first.name()
        {
            let written = tree::name_text(&name).unwrap_or_default();
            let span = self.span(&name);
            let mut problem = Diagnostic::new(
                &rule::TYPE_NAMED_ARGUMENT_UNSUPPORTED,
                format!("`{callee}` takes its arguments in order, without names"),
            )
            .at(span, "named")
            .reason("Named arguments exist only for standard-library operations; your own callables are called positionally, in the order of their parameters.")
            .next(if named.len() == 1 {
                format!("Remove `{written}:`")
            } else {
                "Remove the argument names".to_string()
            });
            for arg in &named {
                if let (Some(name), Some(value)) = (arg.name(), arg.expr()) {
                    let start = u32::from(name.syntax().text_range().start());
                    let end = u32::from(value.syntax().text_range().start());
                    problem = problem.edit(Span::new(self.path, start, end), "");
                }
            }
            self.out.push(problem);
        }
        for (index, arg) in args.iter().enumerate() {
            let Some(value) = arg.expr() else { continue };
            match sig.params.get(index) {
                Some((_, expected)) => {
                    let expected = expected.clone();
                    self.check(&value, &expected);
                }
                None => {
                    self.infer(&value);
                }
            }
        }
        if args.len() != sig.params.len() {
            let span = self.arg_list_span(call);
            let shown: Vec<String> = sig
                .params
                .iter()
                .map(|(name, ty)| format!("{name}: {ty}"))
                .collect();
            let next = if args.len() < sig.params.len() {
                let missing: Vec<String> = sig.params[args.len()..]
                    .iter()
                    .map(|(name, _)| format!("`{name}`"))
                    .collect();
                format!("Pass {} as well", crate::and_list(&missing))
            } else {
                let extra = args.len() - sig.params.len();
                if extra == 1 {
                    "Remove the extra value".to_string()
                } else {
                    format!("Remove the {extra} extra values")
                }
            };
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_ARGUMENT_COUNT,
                    format!(
                        "`{callee}` takes {}, not {}",
                        plural(sig.params.len(), "value"),
                        args.len()
                    ),
                )
                .at(span, format!("{} given", args.len()))
                .reason(format!(
                    "`{callee}({})` has {}; each is given, in order, and none has a default.",
                    shown.join(", "),
                    plural(sig.params.len(), "parameter")
                ))
                .next(next),
            );
        }
    }

    /// A standard-library call: named arguments name the stub's
    /// parameters, the type variables are fixed by the arguments that
    /// aren't literals and then by the type the call's position requires,
    /// every argument is checked, and inline functions are checked last
    /// with their parameter's type known.
    pub(super) fn std_call(
        &mut self,
        operation: &Operation,
        call: &ast::CallExpr,
        written: &str,
        expected: Option<&Ty>,
    ) -> Ty {
        let params: Vec<(String, Ty)> = operation
            .params
            .iter()
            .map(|(name, ty)| (name.clone(), self.table.resolve_std(ty)))
            .collect();
        let args = Self::arguments(call);
        let mut slots: Vec<Option<usize>> = Vec::new();
        let mut unknown = false;
        let mut filled = vec![false; params.len()];
        for (index, arg) in args.iter().enumerate() {
            let slot = match Self::arg_name(arg) {
                Some(name) => {
                    let found = params.iter().position(|(p, _)| *p == name);
                    if found.is_none()
                        && let Some(node) = arg.name()
                    {
                        unknown = true;
                        self.unknown_argument(written, &name, &params, self.span(&node));
                    }
                    found
                }
                None => (index < params.len()).then_some(index),
            };
            if let Some(slot) = slot {
                filled[slot] = true;
            }
            slots.push(slot);
        }
        let mut bound: BTreeMap<String, Ty> = BTreeMap::new();
        let mut actual: Vec<Option<Ty>> = Vec::new();
        for (arg, slot) in args.iter().zip(&slots) {
            let Some(value) = arg.expr() else {
                actual.push(None);
                continue;
            };
            let param = slot.map(|slot| &params[slot].1);
            let is_function = matches!(param, Some(Ty::Function(..)));
            if is_literal(&value) || matches!(value, Expr::ListExpr(_)) || is_function {
                actual.push(None);
                continue;
            }
            let ty = self.infer(&value);
            if let Some(param) = param {
                unify(param, &ty, &mut bound);
            }
            actual.push(Some(ty));
        }
        let result = self.table.resolve_std(&operation.result);
        if let Some(expected) = expected {
            unify(&result, expected, &mut bound);
        }
        let mut inline = Vec::new();
        for ((arg, slot), ty) in args.iter().zip(&slots).zip(actual) {
            let Some(value) = arg.expr() else { continue };
            let Some(slot) = slot else {
                if ty.is_none() {
                    self.infer(&value);
                }
                continue;
            };
            let param = params[*slot].1.clone();
            if let Ty::Function(from, to) = &param {
                match &value {
                    Expr::LambdaExpr(lambda) => {
                        inline.push((lambda.clone(), (**from).clone(), (**to).clone()))
                    }
                    other => {
                        let ty = self.infer(other);
                        let expected = substitute(&param, &bound);
                        self.require(other, &ty, &expected);
                    }
                }
                continue;
            }
            let expected = substitute(&param, &bound);
            match ty {
                Some(ty) => self.require(&value, &ty, &expected),
                None => self.check(&value, &expected),
            }
        }
        for (lambda, from, to) in inline {
            self.inline(&lambda, &from, &to, &mut bound);
        }
        let given = args.len();
        let extra = slots.iter().filter(|slot| slot.is_none()).count();
        let missing: Vec<String> = params
            .iter()
            .zip(&filled)
            .filter(|(_, filled)| !**filled)
            .map(|((name, _), _)| format!("`{name}`"))
            .collect();
        if !unknown && (extra > 0 || !missing.is_empty()) {
            let span = self.arg_list_span(call);
            let shown: Vec<String> = params.iter().map(|(name, _)| name.clone()).collect();
            let next = if missing.is_empty() {
                if extra == 1 {
                    "Remove the extra value".to_string()
                } else {
                    format!("Remove the {extra} extra values")
                }
            } else {
                format!("Pass {} as well", crate::and_list(&missing))
            };
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_ARGUMENT_COUNT,
                    format!(
                        "`{written}` takes {}, not {given}",
                        plural(params.len(), "value")
                    ),
                )
                .at(span, format!("{given} given"))
                .reason(format!(
                    "`{written}({})` has {}, each given once, by position or by name.",
                    shown.join(", "),
                    plural(params.len(), "parameter")
                ))
                .next(next),
            );
        }
        substitute(&result, &bound)
    }

    pub(super) fn unknown_argument(
        &mut self,
        written: &str,
        name: &str,
        params: &[(String, Ty)],
        span: Span,
    ) {
        let names: Vec<&str> = params.iter().map(|(p, _)| p.as_str()).collect();
        let near = nearest(name, names.iter().copied());
        let listed: Vec<String> = names.iter().map(|n| format!("`{n}`")).collect();
        let problem = Diagnostic::new(
            &rule::TYPE_NAMED_ARGUMENT_UNKNOWN,
            format!("`{written}` has no argument `{name}`"),
        )
        .at(span.clone(), "not a parameter")
        .reason(match listed.as_slice() {
            [only] => format!("Its one argument is {only}."),
            _ => format!(
                "Its arguments are {}.{}",
                crate::and_list(&listed),
                super::did_you_mean(near)
            ),
        });
        self.out.push(match near {
            Some(near) => problem
                .next(format!("Use `{near}:` instead of `{name}:`"))
                .edit(span, near),
            None => problem.next(format!("Remove `{name}:` and its value")),
        });
    }

    /// An inline function, `item -> expression`, given to a collection
    /// operation: its parameter takes the list's item type, its body is
    /// one pure expression (area 3, question 13).
    pub(super) fn inline(
        &mut self,
        lambda: &ast::LambdaExpr,
        from: &Ty,
        to: &Ty,
        bound: &mut BTreeMap<String, Ty>,
    ) {
        let previous = self.context;
        self.context = Context::Inline;
        self.scopes.push(Vec::new());
        self.declare(
            lambda.name(),
            substitute(from, bound),
            LocalKind::Inline,
            None,
        );
        if let Some(body) = lambda.expr() {
            match to {
                Ty::Var(name) if !bound.contains_key(name) => {
                    let ty = self.infer(&body);
                    bound.insert(name.clone(), ty);
                }
                Ty::Prim(Prim::Bool) => self.condition(&body),
                other => {
                    let expected = substitute(other, bound);
                    self.check(&body, &expected);
                }
            }
        }
        self.pop_scope();
        self.context = previous;
    }

    /// An inline function anywhere but as an argument to a collection
    /// operation.
    pub(super) fn misplaced_inline(&mut self, lambda: &ast::LambdaExpr) {
        let span = self.span(lambda);
        self.out.push(
            Diagnostic::new(
                &rule::TYPE_INLINE_FUNCTION_PLACEMENT,
                "An inline function is only an argument to a collection operation",
            )
            .at(span, "inline function")
            .reason("Functions aren't values: `item -> expression` is written only as an argument to a standard-library collection operation, such as `collection.map` or `collection.filter`.")
            .next("Write the expression on its own, or pass it to a collection operation"),
        );
        let previous = self.context;
        self.context = Context::Inline;
        self.scopes.push(Vec::new());
        self.declare(lambda.name(), Ty::Error, LocalKind::Inline, None);
        if let Some(body) = lambda.expr() {
            self.infer(&body);
        }
        self.pop_scope();
        self.context = previous;
    }

    // --- building values ----------------------------------------------------

    /// `Type(value)`: re-checks a value against a named type's rules, or a
    /// field type's (`Owner.field(value)`).
    pub(super) fn construct(
        &mut self,
        written: &str,
        field: Option<Ty>,
        call: &ast::CallExpr,
    ) -> Ty {
        let (ty, verdict) = self.construct_checked(written, field, call);
        self.constructions
            .insert(super::key(call.syntax()), verdict);
        ty
    }

    /// `Type(value)` checked, with whether it can fail at runtime.
    fn construct_checked(
        &mut self,
        written: &str,
        field: Option<Ty>,
        call: &ast::CallExpr,
    ) -> (Ty, Fallible) {
        self.last_call = CallKind::Pure(format!("{written}(...)"));
        let target = match field {
            Some(field) => field,
            None => {
                if let Some(prim) = Prim::from_name(written) {
                    if prim == Prim::Unit {
                        self.args_only(call);
                        return (Ty::Prim(Prim::Unit), Fallible::Never);
                    }
                    Ty::Prim(prim)
                } else {
                    match self.table.def(written) {
                        Some(Def::Scalar { .. }) => Ty::Named(written.to_string()),
                        Some(Def::Enum { variants }) => {
                            let variants = variants.clone();
                            self.enum_constructor(written, &variants, call);
                            return (Ty::Named(written.to_string()), Fallible::Unknown);
                        }
                        Some(Def::Fields { base, .. }) => {
                            if matches!(base, FieldsBase::Entity | FieldsBase::Failure) {
                                // Resolution reports these (area 1).
                                self.args_only(call);
                                return (Ty::Error, Fallible::Unknown);
                            }
                            self.args_only(call);
                            let span = call
                                .expr()
                                .map_or_else(|| self.span(call), |e| self.span(&e));
                            self.out.push(
                                Diagnostic::new(
                                    &rule::TYPE_CONSTRUCTOR_INPUT,
                                    format!("`{written}` is built with `{{ ... }}`"),
                                )
                                .at(span, "an object")
                                .reason(format!(
                                    "`{written}` has fields, so a value is written with its name and every field in a block."
                                ))
                                .next(format!("Write `{written} {{ ... }}` with each field")),
                            );
                            return (Ty::Named(written.to_string()), Fallible::Unknown);
                        }
                        None => {
                            self.args_only(call);
                            return (Ty::Error, Fallible::Unknown);
                        }
                    }
                }
            }
        };
        let args = Self::positional(call);
        if args.len() != 1 {
            for (_, value) in &args {
                self.infer(value);
            }
            let span = self.span(call);
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_CONSTRUCTOR_ARGUMENT_COUNT,
                    format!("`{written}(...)` takes one value, not {}", args.len()),
                )
                .at(span, format!("{} given", args.len()))
                .reason(format!(
                    "Constructing a `{written}` checks one value against its rules."
                ))
                .next(format!("Pass one value to `{written}(...)`")),
            );
            return (target, Fallible::Unknown);
        }
        let value = &args[0].1;
        if is_literal(value) {
            // Checked while compiling (area 2).
            self.literal(value, &target, true);
            return (target, Fallible::Never);
        }
        let before = self.out.len();
        let actual = self.infer(value);
        // A value already reported as wrong proves nothing either way.
        if actual.has_error() || target.has_error() || self.out.len() > before {
            return (target, Fallible::Unknown);
        }
        if self.table.assignable(&actual, &target) {
            return (target, Fallible::Never);
        }
        if let Ty::Prim(prim) = &target
            && prim.is_representation()
        {
            // `Instant("2026-10-25T08:30:00Z")` builds a built-in value from a
            // literal; nothing converts a named type back to one.
            let span = self.span(value);
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_CONSTRUCTOR_INPUT,
                    format!("`{written}` can't be built from `{actual}`"),
                )
                .at(span, format!("this is `{actual}`"))
                .reason(format!(
                    "A named type never becomes its built-in representation: there is no cast and no unwrap, only operators work on `{written}` values inside it."
                ))
                .next(format!("Use the `{actual}` value as it is")),
            );
            return (target, Fallible::Unknown);
        }
        let from = self.table.representation(&actual);
        let to = self.table.representation(&target);
        if from.is_none() || from != to {
            let span = self.span(value);
            let made_of = |ty: &Ty, prim: Option<Prim>| match prim {
                Some(prim) => format!("`{ty}` is built on `{}`", prim.name()),
                None => format!("`{ty}` is {}", super::super::literal::what(self.table, ty)),
            };
            let target_rep = to.map_or_else(|| target.to_string(), |prim| prim.name().to_string());
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_CONSTRUCTOR_INPUT,
                    format!("`{target}` can't be built from `{actual}`"),
                )
                .at(span, format!("this is `{actual}`"))
                .reason(format!(
                    "{} and {}; a constructor converts only between types with the same representation, re-checking the rules.",
                    made_of(&target, to),
                    made_of(&actual, from)
                ))
                .next(format!("Pass a value built on `{target_rep}`")),
            );
            return (target, Fallible::Unknown);
        }
        let verdict = if self.proves(value, &actual, &target) {
            Fallible::Never
        } else {
            Fallible::Construction
        };
        (target, verdict)
    }

    /// `Enum(value)`: an enum is never built from a value.
    pub(super) fn enum_constructor(
        &mut self,
        host: &str,
        variants: &[super::super::table::VariantDef],
        call: &ast::CallExpr,
    ) {
        let args = Self::positional(call);
        let matching = args
            .first()
            .and_then(|(_, value)| match Const::from_expr(value) {
                Const::Str(text) => variants
                    .iter()
                    .find(|v| v.name == text)
                    .map(|v| v.name.clone()),
                _ => None,
            });
        if matching.is_none() {
            for (_, value) in &args {
                self.infer(value);
            }
        }
        let span = self.span(call);
        let problem = Diagnostic::new(
            &rule::TYPE_CONSTRUCTOR_INPUT,
            format!("`{host}` can't be built from a value"),
        )
        .at(span.clone(), "an enum")
        .reason("An enum's values are its variants, written `Enum.variant`; text from outside becomes a variant only by being decoded at a boundary.");
        self.out.push(match matching {
            Some(variant) => problem
                .next(format!("Write `{host}.{variant}`"))
                .edit(span, format!("{host}.{variant}")),
            None => problem.next(format!(
                "Write one of `{host}`'s variants, such as `{host}.{}`",
                variants.first().map_or("", |v| v.name.as_str())
            )),
        });
    }
}

/// Binds the type variables in `param` from an argument's type.
fn unify(param: &Ty, actual: &Ty, bound: &mut BTreeMap<String, Ty>) {
    match (param, actual.strip_nullable()) {
        (Ty::Var(name), _) => {
            bound.entry(name.clone()).or_insert_with(|| actual.clone());
        }
        (Ty::List(p), Ty::List(a)) | (Ty::Set(p), Ty::Set(a)) => unify(p, a, bound),
        (Ty::Nullable(p), a) => unify(p, a, bound),
        (Ty::Map(pk, pv), Ty::Map(ak, av)) => {
            unify(pk, ak, bound);
            unify(pv, av, bound);
        }
        _ => {}
    }
}

/// `param` with its bound type variables filled in; unbound ones are
/// `Error`.
fn substitute(param: &Ty, bound: &BTreeMap<String, Ty>) -> Ty {
    match param {
        Ty::Var(name) => bound.get(name).cloned().unwrap_or(Ty::Error),
        Ty::List(inner) => Ty::List(Box::new(substitute(inner, bound))),
        Ty::Set(inner) => Ty::Set(Box::new(substitute(inner, bound))),
        Ty::Nullable(inner) => substitute(inner, bound).nullable(),
        Ty::Map(key, value) => Ty::Map(
            Box::new(substitute(key, bound)),
            Box::new(substitute(value, bound)),
        ),
        Ty::Function(from, to) => Ty::Function(
            Box::new(substitute(from, bound)),
            Box::new(substitute(to, bound)),
        ),
        other => other.clone(),
    }
}
