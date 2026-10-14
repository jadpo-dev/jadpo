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
use super::facts::Reach;
use super::ops::Operand;
use super::{Body, CallKind, Context, Fallible, LocalKind, capital, is_literal, node_key, text};
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
        let fallible = self.call_fallible(call);
        self.facts
            .fallible
            .insert(node_key(call.syntax()), fallible);
        self.unhandled(call);
        ty
    }

    pub(super) fn call_kind(
        &mut self,
        call: &ast::CallExpr,
        expected: Option<&Ty>,
    ) -> (Ty, CallKind) {
        let before = self.out.len() + self.resolved.len();
        let (ty, kind, reach) = self.resolve_call(call, expected);
        // Nothing reaches a call no one has explained: it would check clean
        // and then fault as something that can't be called.
        if matches!(reach, Reach::Unknown) && self.out.len() + self.resolved.len() == before {
            self.uncallable(call);
        }
        self.facts.reaches.insert(node_key(call.syntax()), reach);
        (ty, kind)
    }

    /// A call that reaches nothing, when nothing more specific was said.
    fn uncallable(&mut self, call: &ast::CallExpr) {
        let written = call
            .expr()
            .map(|callee| tree::written(callee.syntax()))
            .unwrap_or_default();
        let span = call
            .expr()
            .map_or_else(|| self.span(call), |callee| self.span(&callee));
        self.out.push(
            Diagnostic::new(
                &rule::SEMANTIC_UNKNOWN_CALLEE,
                format!("`{written}` isn't something Jadpo can call"),
            )
            .at(span, "reaches nothing")
            .reason("A call reaches a function, action, query or fixture, a library's function, a standard-library operation, an entity's operation, or a named type being built; this names none of them.")
            .next("Call a declared function, action or query as `name(...)`, `domain.action(...)` or `library.function(...)`"),
        );
    }

    /// `List<Tag>(xs)`: a collection type isn't a constructor. Only a named
    /// type is built from a value (area 2), and a collection keeps its
    /// element type, so there is no conversion from one to another.
    fn collection_constructor(&mut self, callee: &Expr, call: &ast::CallExpr) {
        self.args_only(call);
        let written = tree::written(callee.syntax());
        let span = self.span(callee);
        self.out.push(
            Diagnostic::new(
                &rule::TYPE_CONSTRUCTOR_INPUT,
                format!("`{written}` isn't built from a value"),
            )
            .at(span, "a collection type")
            .reason(format!(
                "Only a named type is built from a value, re-checking its rules. A collection keeps its element type, so nothing converts another collection into a `{written}`, even where each item would fit."
            ))
            .next(format!(
                "Make each item the element type where it is made, then write the `{written}` as `[a, b]`"
            )),
        );
    }

    /// The one place a call is resolved: what it reaches, checked, with its
    /// result and kind. The typed body, the graph's `calls` edges, recursion
    /// and the language server read what this records.
    fn resolve_call(
        &mut self,
        call: &ast::CallExpr,
        expected: Option<&Ty>,
    ) -> (Ty, CallKind, Reach) {
        // Resolution's part (area 1): what the callee's names reach, and
        // whether this file may use it.
        let first = match call.expr() {
            Some(Expr::NameRef(name)) => Some(text(&name)),
            Some(callee @ Expr::FieldExpr(_)) => dotted(&callee).map(|(first, _)| text(&first)),
            _ => None,
        };
        let local = first.is_some_and(|first| self.is_local(&first));
        self.resolved_by(|names, out| names.callee(call, local, out));
        match call.expr() {
            Some(Expr::NameRef(name)) => {
                let callee = text(&name);
                if starts_upper(&callee) {
                    let ty = self.construct(&callee, None, call);
                    let reach = if ty.has_error() {
                        Reach::Unknown
                    } else {
                        Reach::Construct(ty.clone())
                    };
                    return (ty, CallKind::Pure(format!("{callee}(...)")), reach);
                }
                if self.is_local(&callee) {
                    // Functions aren't values, so a local is never called;
                    // resolution reports it (area 1).
                    self.read(&name);
                    self.args_only(call);
                    return (Ty::Error, CallKind::Other, Reach::Unknown);
                }
                if let Some(library) = self.library.clone()
                    && let Some(sig) = self.calls.library_signature(&library, &callee).cloned()
                {
                    self.args(&sig, call, &callee);
                    let reach = Reach::Library {
                        library,
                        function: callee.clone(),
                        fails: sig.fails.clone(),
                    };
                    return (sig.result, CallKind::Pure(callee), reach);
                }
                let Some(sig) = self.calls.signature(&callee).cloned() else {
                    self.args_only(call);
                    return (Ty::Error, CallKind::Other, Reach::Unknown);
                };
                let kind = self.callable_kind(&callee);
                let span = self.span(&name);
                self.may_call(span, &callee, kind.as_ref());
                self.args(&sig, call, &callee);
                let reach = match &kind {
                    Some(kind) => Reach::Callable {
                        kind: kind.clone(),
                        name: callee.clone(),
                        fails: sig.fails.clone(),
                    },
                    None => Reach::Unknown,
                };
                let call_kind = match kind {
                    Some(ItemKind::Function) => CallKind::Pure(callee),
                    _ => CallKind::Effect(callee),
                };
                (sig.result, call_kind, reach)
            }
            Some(callee @ Expr::FieldExpr(_)) => self.dotted_call(&callee, call, expected),
            Some(callee @ Expr::GenericExpr(_)) => {
                self.collection_constructor(&callee, call);
                (Ty::Error, CallKind::Other, Reach::Unknown)
            }
            Some(other) => {
                self.infer(&other);
                self.args_only(call);
                (Ty::Error, CallKind::Other, Reach::Unknown)
            }
            None => (Ty::Error, CallKind::Other, Reach::Unknown),
        }
    }

    fn dotted_call(
        &mut self,
        callee: &Expr,
        call: &ast::CallExpr,
        expected: Option<&Ty>,
    ) -> (Ty, CallKind, Reach) {
        let Some((first, rest)) = dotted(callee) else {
            self.infer(callee);
            self.args_only(call);
            return (Ty::Error, CallKind::Other, Reach::Unknown);
        };
        let head = text(&first);
        let rest: Vec<String> = rest.iter().map(text).collect();
        let written = format!("{head}.{}", rest.join("."));
        if starts_upper(&head) {
            if self.table.field(&head, &rest).is_some() {
                let target = Ty::Field(head.clone(), rest.clone());
                let ty = self.construct(&written, Some(target), call);
                let reach = Reach::Construct(Ty::Field(head.clone(), rest.clone()));
                return (ty, CallKind::Pure(format!("{written}(...)")), reach);
            }
            if self.callable_kind(&head) == Some(ItemKind::Entity) && rest.len() == 1 {
                // An entity's operations read and write stored data
                // (areas 5 and 6).
                if self.context.is_pure() {
                    let span = self.span(callee);
                    self.persistence(span, &written);
                }
                self.args_only(call);
                let reach = Reach::Entity {
                    entity: head.clone(),
                    operation: rest.join("."),
                };
                return (Ty::Error, CallKind::Effect(written), reach);
            }
            if self.symbols.is_package(&head) {
                // A package's operations come with area 11.
                let span = self.span(callee);
                self.not_built_yet(span, &written);
            }
            self.args_only(call);
            return (Ty::Error, CallKind::Other, Reach::Unknown);
        }
        if self.is_local(&head) {
            // Values have no methods.
            self.read(&first);
            self.args_only(call);
            let span = self.span(callee);
            self.out.push(
                Diagnostic::new(
                    &rule::SEMANTIC_NOT_CALLABLE,
                    format!("`{written}` is a value's member and can't be called"),
                )
                .at(span, "not callable")
                .reason(format!(
                    "`{head}` is a variable, and a value has no operations of its own: Jadpo calls functions, actions, queries, library functions and the standard library's operations, never a value's methods."
                ))
                .next(format!("Pass `{head}` to a function or an operation instead")),
            );
            return (Ty::Error, CallKind::Other, Reach::Unknown);
        }
        if self.symbols.is_connection(&head) {
            // A connection's operations, `connection.resource.operation`,
            // come with area 11.
            let span = self.span(callee);
            self.not_built_yet(span, &written);
            self.args_only(call);
            return (Ty::Error, CallKind::Effect(written), Reach::Unknown);
        }
        let [member] = rest.as_slice() else {
            self.args_only(call);
            return (Ty::Error, CallKind::Effect(written), Reach::Unknown);
        };
        let names = StdNames::get();
        if names.namespace(&head) == Some(Namespace::Generated) {
            // Nothing a generated namespace holds is called: `clock.now` is
            // a value, and the rest wait for their areas.
            let span = self.span(callee);
            self.not_built_yet(span, &written);
            self.args_only(call);
            return (Ty::Error, CallKind::Other, Reach::Unknown);
        }
        if names.namespace(&head) == Some(Namespace::Standard) {
            if let Some(operation) = names.operation(&head, member) {
                let ty = self.std_call(operation, call, &written, expected);
                let reach = jadpo_std::operation(&head, member)
                    .map_or(Reach::Unknown, |operation| Reach::Std(operation.op));
                return (ty, CallKind::Pure(written), reach);
            }
            if names.name_only(&head, member) {
                let span = self.span(callee);
                self.not_built_yet(span, &written);
            }
            self.args_only(call);
            return (Ty::Error, CallKind::Pure(written), Reach::Unknown);
        }
        if self.symbols.library(&head).is_some() {
            return match self.calls.library_signature(&head, member).cloned() {
                Some(sig) => {
                    self.args(&sig, call, &written);
                    let reach = Reach::Library {
                        library: head.clone(),
                        function: member.clone(),
                        fails: sig.fails.clone(),
                    };
                    (sig.result, CallKind::Pure(written), reach)
                }
                None => {
                    self.args_only(call);
                    (Ty::Error, CallKind::Other, Reach::Unknown)
                }
            };
        }
        if self.symbols.is_domain(&head) {
            let kind = self.callable_kind(member);
            if let Some(sig) = self
                .calls
                .signature(member)
                .filter(|_| {
                    matches!(
                        kind,
                        Some(ItemKind::Action | ItemKind::Query | ItemKind::Fixture)
                    )
                })
                .cloned()
            {
                let span = self.span(callee);
                self.may_call(span, &written, kind.as_ref());
                self.args(&sig, call, &written);
                let reach = Reach::Callable {
                    kind: kind.unwrap_or(ItemKind::Action),
                    name: member.clone(),
                    fails: sig.fails.clone(),
                };
                return (sig.result, CallKind::Effect(written), reach);
            }
        }
        self.args_only(call);
        (Ty::Error, CallKind::Effect(written), Reach::Unknown)
    }

    /// A standard-library operation the language declares but nothing runs
    /// yet: its area's chunk builds it.
    pub(super) fn not_built_yet(&mut self, span: Span, written: &str) {
        self.out.push(
            Diagnostic::new(
                &rule::TOOLCHAIN_NOT_BUILT_YET,
                format!("`{written}` isn't built yet"),
            )
            .at(span, "declared, not built")
            .reason(format!(
                "The language declares `{written}`, but this compiler has nothing to run it with yet, so a program that calls it couldn't run."
            ))
            .next(format!("Do without `{written}` for now")),
        );
    }

    /// The kind of the project declaration called `name`, not counting
    /// library functions.
    pub(super) fn callable_kind(&self, name: &str) -> Option<ItemKind> {
        self.symbols
            .declared(name)
            .iter()
            .find(|symbol| symbol.library.is_none())
            .map(|symbol| symbol.kind.clone())
    }

    /// Who may call whom (area 3): a function, or an inline function, calls
    /// only functions, library functions and the standard library; only
    /// tests, fixtures and seeds call a fixture (area 13).
    pub(super) fn may_call(&mut self, span: Span, callee: &str, kind: Option<&ItemKind>) {
        let Some(kind @ (ItemKind::Action | ItemKind::Query | ItemKind::Fixture)) = kind else {
            return;
        };
        if *kind == ItemKind::Fixture && self.context == Context::Action {
            self.out.push(
                Diagnostic::new(
                    &rule::SEMANTIC_NOT_CALLABLE,
                    format!("An action can't call fixture `{callee}`"),
                )
                .at(span, "test data")
                .reason("A fixture builds test data: only tests, other fixtures and seeds call it, and it is never compiled into a deployable.")
                .next(format!(
                    "Call the actions `{callee}` calls, or move this call into a test, fixture or seed"
                )),
            );
            return;
        }
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
        self.facts
            .slots
            .insert(node_key(call.syntax()), slots.clone());
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
                self.unify(param, &ty, &mut bound);
            }
            actual.push(Some(ty));
        }
        let result = self.table.resolve_std(&operation.result);
        if let Some(expected) = expected {
            self.unify(&result, expected, &mut bound);
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
            if let Some(accepts) = built_on(&expected) {
                let name = params[*slot].0.clone();
                self.built_on_argument(&value, ty, accepts, written, &name);
                continue;
            }
            match ty {
                Some(ty) => self.require(&value, &ty, &expected),
                None => self.check(&value, &expected),
            }
        }
        for (lambda, from, to) in inline {
            self.inline(&lambda, &from, &to, &mut bound);
        }
        self.divisors(&args, &slots, written);
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

    /// A standard-library divisor is checked as `/`'s is (area 3, question
    /// 7): a literal isn't `0`, and a value's type excludes zero.
    fn divisors(&mut self, args: &[ast::Arg], slots: &[Option<usize>], written: &str) {
        let Some(operation) = written
            .split_once('.')
            .and_then(|(namespace, name)| jadpo_std::operation(namespace, name))
        else {
            return;
        };
        for (arg, slot) in args.iter().zip(slots) {
            let (Some(value), Some(slot)) = (arg.expr(), slot) else {
                continue;
            };
            if !operation.params.get(*slot).is_some_and(|p| p.never_zero) {
                continue;
            }
            let operand = if is_literal(&value) {
                Operand::Literal(Const::from_expr(&value))
            } else {
                match self.facts.types.get(&node_key(value.syntax())).cloned() {
                    // A value that isn't a number is already reported.
                    Some(ty)
                        if !ty.has_error()
                            && !self.table.is_nullable(&ty)
                            && self.table.representation(&ty).is_some_and(Prim::is_number) =>
                    {
                        Operand::Value(ty)
                    }
                    _ => continue,
                }
            };
            self.divisor(&value, &operand);
        }
    }

    /// An argument to a parameter that takes any type built on one of
    /// `accepts`, as operators work on the representation: `Number`, or a
    /// built-in representation written in a stub.
    fn built_on_argument(
        &mut self,
        value: &Expr,
        ty: Option<Ty>,
        accepts: &[Prim],
        written: &str,
        param: &str,
    ) {
        let ty = match ty {
            Some(ty) => ty,
            None => {
                // A literal takes the representation it is written as.
                let literal = match Const::from_expr(value) {
                    Const::Number { decimal: false, .. } => Some(Prim::Int),
                    Const::Number { decimal: true, .. } => Some(Prim::Decimal),
                    _ => None,
                };
                match literal {
                    Some(prim) if accepts.contains(&prim) => {
                        self.check(value, &Ty::Prim(prim));
                        return;
                    }
                    _ if accepts.len() == 1 => {
                        self.check(value, &Ty::Prim(accepts[0]));
                        return;
                    }
                    _ => self.infer(value),
                }
            }
        };
        if ty.has_error() {
            return;
        }
        let nullable = self.table.is_nullable(&ty);
        if !nullable
            && self
                .table
                .representation(&ty)
                .is_some_and(|prim| accepts.contains(&prim))
        {
            return;
        }
        let wanted = match accepts {
            [only] => format!("a value built on `{}`", only.name()),
            _ => "a number".to_string(),
        };
        let built: Vec<String> = accepts
            .iter()
            .map(|prim| format!("`{}`", prim.name()))
            .collect();
        let span = self.span(value);
        let reason = if nullable {
            format!("`{written}`'s `{param}` is never `none`, and `{ty}` may be `none`.")
        } else {
            format!(
                "`{written}`'s `{param}` takes a value built on {}; `{ty}` is {}.",
                built.join(" or "),
                super::super::literal::what(self.table, &ty)
            )
        };
        self.out.push(
            Diagnostic::new(
                &rule::TYPE_MISMATCH,
                format!("`{ty}` can't be used where {wanted} is required"),
            )
            .at(span, format!("this is `{ty}`"))
            .reason(reason)
            .next(format!("Pass {wanted} here")),
        );
    }

    /// Binds the type variables in `param` from an argument's type, read
    /// through field types and refinements to the container they are.
    fn unify(&self, param: &Ty, actual: &Ty, bound: &mut BTreeMap<String, Ty>) {
        if let Ty::Var(name) = param {
            bound.entry(name.clone()).or_insert_with(|| actual.clone());
            return;
        }
        match (param, self.table.structure(actual)) {
            (Ty::List(p), Ty::List(a)) | (Ty::Set(p), Ty::Set(a)) => self.unify(p, &a, bound),
            (Ty::Nullable(p), a) => self.unify(p, &a, bound),
            (Ty::Map(pk, pv), Ty::Map(ak, av)) => {
                self.unify(pk, &ak, bound);
                self.unify(pv, &av, bound);
            }
            _ => {}
        }
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
            "parameter",
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
        self.declare(
            lambda.name(),
            Ty::Error,
            LocalKind::Inline,
            None,
            "parameter",
        );
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
        self.facts
            .constructions
            .insert(node_key(call.syntax()), verdict);
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
        if matches!(value, Expr::ListExpr(_)) && is_collection(&self.table.structure(&target)) {
            // A list written here takes the collection's type, and its
            // length is checked while compiling, as a literal's is.
            self.check(value, &target);
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
        if is_collection(&self.table.structure(&target)) {
            if !self.collection_input(value, &actual, &target) {
                return (target, Fallible::Unknown);
            }
            let fallible = self.proved(value, &actual, &target, call);
            return (target, fallible);
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
        let fallible = self.proved(value, &actual, &target, call);
        (target, fallible)
    }

    /// Whether `Type(value)` can fail: the rules of `target` neither the
    /// declared ranges nor the conditions around it prove `value`, of type
    /// `actual`, follows, with the rules the conditions prove recorded.
    fn proved(&mut self, value: &Expr, actual: &Ty, target: &Ty, call: &ast::CallExpr) -> Fallible {
        let verdict = self.verdict(value, actual, target);
        if !verdict.proved.is_empty() {
            self.facts
                .proofs
                .insert(node_key(call.syntax()), verdict.proved);
        }
        if verdict.breakable.is_empty() {
            Fallible::Never
        } else {
            Fallible::Construction(verdict.breakable)
        }
    }

    /// `Names(found)` (area 2, "Named collection construction"): a named
    /// list, set or map type, or a field type with collection rules, is
    /// built from a collection whose items are already its item type,
    /// checking only the collection's own rules. A list is built from a
    /// list, a set from a set or a list (its repeats collapse), a map from
    /// a map; the items are never converted in the same step, and
    /// collections stay invariant. Reports the value and gives `false`
    /// when it isn't such a collection.
    fn collection_input(&mut self, value: &Expr, actual: &Ty, target: &Ty) -> bool {
        let into = self.table.structure(target);
        let from = self.table.structure(actual);
        let nullable = self.table.is_nullable(actual);
        let same_kind = matches!(
            (&into, &from),
            (Ty::List(_), Ty::List(_))
                | (Ty::Set(_), Ty::Set(_) | Ty::List(_))
                | (Ty::Map(..), Ty::Map(..))
        );
        let items_fit = match (&into, &from) {
            (Ty::List(a) | Ty::Set(a), Ty::List(b) | Ty::Set(b)) => a == b,
            (Ty::Map(ka, va), Ty::Map(kb, vb)) => ka == kb && va == vb,
            _ => false,
        };
        if same_kind && items_fit && !nullable {
            return true;
        }
        let kind = collection_kind(&into);
        let built_from = match &into {
            Ty::Set(item) => format!("a `Set<{item}>` or a `List<{item}>`"),
            other => format!("a `{other}`"),
        };
        let (reason, next) = if same_kind && !items_fit {
            let converts = if matches!(into, Ty::Map(..)) {
                "the keys or values"
            } else {
                "the items"
            };
            let build = match &into {
                Ty::Map(key, value) => {
                    format!("Build each key as a `{key}` and each value as a `{value}`")
                }
                Ty::List(item) | Ty::Set(item) => format!("Build each item as a `{item}`"),
                _ => String::new(),
            };
            let from_kind = collection_kind(&from);
            let wanted = match (&into, &from) {
                (Ty::List(item) | Ty::Set(item), Ty::List(_)) => format!("List<{item}>"),
                (Ty::List(item) | Ty::Set(item), _) => format!("Set<{item}>"),
                (other, _) => other.to_string(),
            };
            (
                format!(
                    "`{target}` is {}, and this is {}. Building a `{target}` checks only the {kind}'s own rules; it never converts {converts}.",
                    collection_words(&into),
                    collection_words(&from)
                ),
                format!("{build} where it is made, so the {from_kind} is a `{wanted}`"),
            )
        } else if same_kind {
            (
                format!("`{actual}` may be `none`, and a `{target}` can't be."),
                format!("Use `match` to handle `none` before building a `{target}`"),
            )
        } else if let (Ty::List(item), Ty::Set(_)) = (&into, &from) {
            (
                format!(
                    "`{target}` is {}, and this is a set. A set has no order a list could keep, so a list is built only from a list.",
                    collection_words(&into)
                ),
                format!("Keep the items in a `List<{item}>` where they are collected"),
            )
        } else {
            let made_of = if is_collection(&from) {
                collection_words(&from)
            } else {
                super::super::literal::what(self.table, actual)
            };
            (
                format!(
                    "`{target}` is {}, so it is built from {built_from}; `{actual}` is {made_of}.",
                    collection_words(&into)
                ),
                format!("Pass {built_from}"),
            )
        };
        let span = self.span(value);
        self.out.push(
            Diagnostic::new(
                &rule::TYPE_CONSTRUCTOR_INPUT,
                format!("`{target}` can't be built from `{actual}`"),
            )
            .at(span, format!("this is `{actual}`"))
            .reason(reason)
            .next(next),
        );
        false
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

fn is_collection(ty: &Ty) -> bool {
    matches!(ty, Ty::List(_) | Ty::Set(_) | Ty::Map(..))
}

/// `list`, `set` or `map`.
fn collection_kind(ty: &Ty) -> &'static str {
    match ty {
        Ty::Set(_) => "set",
        Ty::Map(..) => "map",
        _ => "list",
    }
}

/// A collection in the reader's words: "a list of `Name`", "a map from
/// `Name` to `Count`".
fn collection_words(ty: &Ty) -> String {
    match ty {
        Ty::List(item) => format!("a list of `{item}`"),
        Ty::Set(item) => format!("a set of `{item}`"),
        Ty::Map(key, value) => format!("a map from `{key}` to `{value}`"),
        other => format!("`{other}`"),
    }
}

/// The built-in types a parameter takes values built on, when it is
/// `Number` or a built-in representation in a stub.
fn built_on(param: &Ty) -> Option<&'static [Prim]> {
    match param {
        Ty::Number => Some(&[Prim::Int, Prim::Decimal]),
        Ty::Prim(Prim::Int) => Some(&[Prim::Int]),
        Ty::Prim(Prim::Decimal) => Some(&[Prim::Decimal]),
        Ty::Prim(Prim::Text) => Some(&[Prim::Text]),
        _ => None,
    }
}

/// `param` with its bound type variables filled in; an unbound one is a
/// checked unknown, which fits anything but keeps its container's shape.
fn substitute(param: &Ty, bound: &BTreeMap<String, Ty>) -> Ty {
    match param {
        Ty::Var(name) => bound
            .get(name)
            .cloned()
            .unwrap_or_else(|| Ty::Unknown(name.clone())),
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
