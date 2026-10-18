//! Calls (areas 2 and 3): which callable a name means, what the caller may
//! call, how many arguments it takes and whether they are named, each
//! argument checked against its parameter, standard-library operations
//! with their type variables fixed at each call and their inline
//! functions, and constructors.

use std::collections::BTreeMap;

use jadpo_db::{ItemKind, Ownership};
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

pub(super) fn plural(n: usize, word: &str) -> String {
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
                if kind == Some(ItemKind::Query) {
                    self.read_in_loop(span.clone(), &callee);
                }
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
            if head == crate::identity::PRINCIPAL
                && let [kind] = rest.as_slice()
                && let Some(kind) = jadpo_std::http::PrincipalKind::from_name(kind)
            {
                return self.caller_call(kind, call, callee, &written);
            }
            if self.table.field(&head, &rest).is_some() {
                let target = self.table.field_type(&head, &rest);
                let ty = self.construct(&written, Some(target.clone()), call);
                let reach = Reach::Construct(target);
                return (ty, CallKind::Pure(format!("{written}(...)")), reach);
            }
            if self.callable_kind(&head) == Some(ItemKind::Entity) && rest.len() == 1 {
                // An entity's operations read and write stored data
                // (areas 5 and 6).
                let span = self.span(callee);
                let reach = Reach::Entity {
                    entity: head.clone(),
                    operation: rest.join("."),
                };
                // Another domain's entity is reported where it is named;
                // its operations aren't checked as well.
                let visible = self
                    .symbols
                    .declared(&head)
                    .iter()
                    .find(|symbol| symbol.kind == ItemKind::Entity)
                    .is_none_or(|symbol| match (&symbol.ownership, self.ownership) {
                        (Ownership::Domain(theirs), Ownership::Domain(ours)) => theirs == ours,
                        _ => true,
                    });
                if !visible {
                    self.args_only(call);
                    let block = self.pending_block.take();
                    self.walk(|walk| walk.member_values(block));
                    return (Ty::Error, CallKind::Effect(written), Reach::Unknown);
                }
                // The service entity's keys (area 8, question 7).
                if let Some(op) = jadpo_std::http::SessionOp::key_op(&rest[0]) {
                    let block = self.pending_block.take();
                    self.walk(|walk| walk.member_values(block));
                    return self.key_call(&head, op, call, callee, &written);
                }
                let block = self.pending_block.take();
                if self.context.is_pure() {
                    self.persistence(span, &written);
                    self.args_only(call);
                    self.walk(|walk| walk.member_values(block));
                    return (Ty::Error, CallKind::Effect(written), reach);
                }
                if rest[0] == "get" {
                    let ty = self.entity_get(&head, call, span);
                    return (ty, CallKind::Effect(written), reach);
                }
                let op = match rest[0].as_str() {
                    "update" => Some(jadpo_std::write::WriteOp::Update),
                    "delete" => Some(jadpo_std::write::WriteOp::Delete),
                    _ => None,
                };
                if let Some(op) = op {
                    let ty = self.write(&head, op, call.syntax(), Some(call), block, span, None);
                    return (ty, CallKind::Effect(written), reach);
                }
                // A lifecycle transition is an update of the fields it sets
                // (area 6, "The operations every entity gets").
                let transition = self
                    .writes
                    .write_rules(&head)
                    .and_then(|rules| rules.transition(&rest[0]))
                    .is_some();
                if transition && rest[0] != "delete" {
                    let ty = self.write(
                        &head,
                        jadpo_std::write::WriteOp::Update,
                        call.syntax(),
                        Some(call),
                        block,
                        span,
                        Some(&rest[0]),
                    );
                    return (ty, CallKind::Effect(written), reach);
                }
                // A name that is no operation is reported where it is
                // resolved; `create` is written with its block.
                self.args_only(call);
                self.walk(|walk| walk.member_values(block));
                return (Ty::Error, CallKind::Effect(written), reach);
            }
            if let Some((_, op)) = self.write_call(call) {
                // A write to a type that isn't an entity (area 6).
                let span = self.span(callee);
                let block = self.pending_block.take();
                self.write_not_entity(&head, op, span);
                self.args_only(call);
                self.walk(|walk| walk.member_values(block));
                return (Ty::Error, CallKind::Effect(written), Reach::Unknown);
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
        let names = StdNames::get();
        // A message's call takes the domain that words it and the message:
        // `translations.<domain>.<message>(...)`.
        if head == "translations" && names.namespace(&head) == Some(Namespace::Generated) {
            return self.translation_call(call, callee, &rest, &written);
        }
        let [member] = rest.as_slice() else {
            self.args_only(call);
            return (Ty::Error, CallKind::Effect(written), Reach::Unknown);
        };
        if names.namespace(&head) == Some(Namespace::Generated) {
            if head == "sessions"
                && let Some(op) = jadpo_std::http::SessionOp::from_name(member)
            {
                return self.session_call(op, call, callee, &written);
            }
            if head == "clock" && member == "advance" {
                return self.advance_clock(call, callee, &written);
            }
            if head == crate::prompts::PROMPTS && member == "render" {
                return self.prompt_render(call, &written);
            }
            // Nothing else a generated namespace holds is called: `clock.now`
            // is a value, and the rest wait for their areas.
            let span = self.span(callee);
            self.not_built_yet(span, &written);
            self.args_only(call);
            return (Ty::Error, CallKind::Other, Reach::Unknown);
        }
        if names.namespace(&head) == Some(Namespace::Standard) {
            if let Some(targets) = jadpo_std::moved_namespace(&head)
                && let [member] = rest.as_slice()
            {
                return self.moved_namespace_call(callee, call, (&head, member), targets, expected);
            }
            // An operation that moved is reported where it is resolved
            // (area 19) and checked as the operation it moved to.
            if let Some(to) = jadpo_std::moved(&head, member)
                && let Some((space, name)) = to.split_once('.')
                && let Some(operation) = names.operation(space, name)
            {
                let ty = self.std_call(operation, call, to, expected);
                let reach = jadpo_std::operation(space, name)
                    .map_or(Reach::Unknown, |operation| Reach::Std(operation.op));
                return (ty, CallKind::Pure(to.to_string()), reach);
            }
            if let Some(operation) = names.operation(&head, member) {
                let ty = self.std_call(operation, call, &written, expected);
                let table = jadpo_std::operation(&head, member);
                let reach = table.map_or(Reach::Unknown, |operation| Reach::Std(operation.op));
                // `random` reads the generator: an effect, only actions'
                // (area 19, question 19).
                if table.is_some_and(|operation| operation.op.reads_generator()) {
                    let span = self.span(call);
                    self.random_read(span, &written);
                    return (ty, CallKind::Effect(written), reach);
                }
                // `crypto.verify_jwt` reads the clock, as `clock.now` does
                // (area 19, chunk 5.5r).
                if table.is_some_and(|operation| operation.op.reads_clock()) {
                    let span = self.span(call);
                    self.clock_read(span, &written);
                    return (ty, CallKind::Effect(written), reach);
                }
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
                if kind == Some(ItemKind::Query) {
                    self.read_in_loop(span.clone(), &written);
                }
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

    /// A call through a namespace that moved whole (area 19, question 6):
    /// `collection.<name>` is reported with an edit to `list.`, `set.` or
    /// `map.`, by what its first value is, and checked as the operation it
    /// moved to where that one is built.
    fn moved_namespace_call(
        &mut self,
        callee: &Expr,
        call: &ast::CallExpr,
        (head, member): (&str, &str),
        targets: &'static [&'static str],
        expected: Option<&Ty>,
    ) -> (Ty, CallKind, Reach) {
        let first = Self::arguments(call).first().and_then(ast::Arg::expr);
        let kind = first
            .as_ref()
            .map_or("list", |value| self.collection_word(value));
        let space = targets
            .iter()
            .copied()
            .find(|space| *space == kind)
            .unwrap_or("list");
        let to = format!("{space}.{member}");
        let from = format!("{head}.{member}");
        let which = match &first {
            Some(value) => format!("`{}` is a {kind}", tree::written(value.syntax())),
            None => "a call with no values is a list's".to_string(),
        };
        let shown: Vec<String> = targets.iter().map(|space| format!("`{space}.`")).collect();
        let span = self.span(callee);
        self.out.push(
            Diagnostic::new(
                &rule::SEMANTIC_STANDARD_OPERATION_MOVED,
                format!("`{from}` is now `{to}`"),
            )
            .at(span.clone(), "moved")
            .reason(format!(
                "The standard library has one namespace for each type an operation works on, so `{head}.` became {}; {which}.",
                crate::and_list(&shown)
            ))
            .next(format!("Write `{to}`"))
            .edit(span, to.clone()),
        );
        if let Some(operation) = StdNames::get().operation(space, member) {
            let ty = self.std_call(operation, call, &to, expected);
            let reach = jadpo_std::operation(space, member)
                .map_or(Reach::Unknown, |operation| Reach::Std(operation.op));
            return (ty, CallKind::Pure(to), reach);
        }
        self.args_only(call);
        (Ty::Error, CallKind::Pure(to), Reach::Unknown)
    }

    /// Whether a value is a list, a set or a map, read without reporting
    /// anything, since the call that reads it reports what it finds: only
    /// a name or a field is read, and anything else is taken as a list.
    fn collection_word(&mut self, value: &Expr) -> &'static str {
        if !matches!(value, Expr::NameRef(_) | Expr::FieldExpr(_)) {
            return "list";
        }
        let before = (self.out.len(), self.resolved.len());
        let ty = self.infer(value);
        self.out.truncate(before.0);
        self.resolved.truncate(before.1);
        match self.table.structure(&ty) {
            Ty::Set(_) => "set",
            Ty::Map(..) => "map",
            _ => "list",
        }
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
        let table = written
            .split_once('.')
            .and_then(|(namespace, name)| jadpo_std::operation(namespace, name));
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
            // A format and a type are read as written, never as values.
            let is_format = slot
                .and_then(|slot| table.and_then(|op| op.params.get(slot)))
                .is_some_and(|p| matches!(p.kind, jadpo_std::Kind::Format | jadpo_std::Kind::Type));
            // A list written out takes the parameter's type, a set's
            // included, unless a list is expected and every item is a
            // value of its own, which then fixes the item type
            // (`list.sort_by([rye, bap], ...)`).
            let written_list = match &value {
                Expr::ListExpr(list) => {
                    !matches!(param, Some(Ty::List(_)))
                        || list.exprs().next().is_none()
                        || list
                            .exprs()
                            .any(|item| is_literal(&item) || matches!(item, Expr::ListExpr(_)))
                }
                _ => false,
            };
            if is_literal(&value) || written_list || is_function || is_format {
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
        let mut format: Option<(Expr, jadpo_std::TimeFormatSpec)> = None;
        // The contract's value and the standard type it is, and the type
        // `as:` names (area 19).
        // A number has no entry (area 19, "Numbers").
        let mut contract: Option<(Ty, Option<&'static jadpo_std::StandardType>)> = None;
        let mut parsed: Option<(Ty, Option<Expr>)> = None;
        // The type `json.read`'s `as:` names (area 19, "Data formats").
        let mut read_as: Option<Ty> = None;
        // `crypto.verify_jwt`'s `as:`, when given: the type, or `None` once
        // what is wrong with it is reported (chunk 5.5r4).
        let mut claims_as: Option<Option<Ty>> = None;
        // What `list.sum`'s items are built on, which its result is.
        let mut summed: Option<Prim> = None;
        // The range type a `range` call's ranges are, which its result is.
        let mut ranged: Option<Ty> = None;
        // The quantity kind a `units` call's value or items are, and the
        // kind its unit is, with the unit as written (area 19).
        let mut quantity: Option<jadpo_std::QuantityKind> = None;
        let mut unit: Option<(jadpo_std::QuantityKind, Expr)> = None;
        for ((arg, slot), ty) in args.iter().zip(&slots).zip(actual) {
            let Some(value) = arg.expr() else { continue };
            let Some(slot) = slot else {
                if ty.is_none() {
                    self.infer(&value);
                }
                continue;
            };
            let param = params[*slot].1.clone();
            // A secret value goes only to the operation made to take it
            // (area 19, chunk 5.5r2); `json.write` says why it can't write
            // one itself.
            if let (Some(given), Some(table)) = (&ty, table)
                && table.op != jadpo_std::Op::JsonWrite
                && let Some(secret) = super::super::secrets::secret_in(self.table, given)
                && !super::super::secrets::taken_by(table.op, *slot, secret)
            {
                let span = self.span(&value);
                self.out
                    .push(super::super::secrets::argument(written, secret, span));
                continue;
            }
            if let Ty::Function(from, to) = &param {
                match &value {
                    Expr::LambdaExpr(lambda) => {
                        inline.push((lambda.clone(), from.clone(), (**to).clone(), *slot))
                    }
                    other => {
                        let ty = self.infer(other);
                        let expected = substitute(&param, &bound);
                        self.require(other, &ty, &expected);
                    }
                }
                continue;
            }
            let kind = table.and_then(|op| op.params.get(*slot)).map(|p| p.kind);
            // A `crypto` call's key is a setting, written `key:
            // config.<name>` (area 19, "Hashing and cryptography"): the
            // one place code gives a key, which the compiler owns.
            if let Some(jadpo_std::Kind::Builtin(key)) = kind
                && key.is_key()
                && !is_setting_read(&value)
            {
                if ty.is_none() {
                    self.infer(&value);
                }
                self.key_argument(&value, written, key);
                continue;
            }
            if kind == Some(jadpo_std::Kind::Range) {
                let given = match ty {
                    Some(ty) => ty,
                    None => self.infer(&value),
                };
                let name = params[*slot].0.clone();
                self.range_argument(&value, &given, &mut ranged, (written, &name));
                continue;
            }
            if kind == Some(jadpo_std::Kind::Quantity) {
                let given = match ty {
                    Some(ty) => ty,
                    None => self.infer(&value),
                };
                let name = params[*slot].0.clone();
                quantity = self.quantity_argument(&value, &given, written, &name);
                continue;
            }
            if kind == Some(jadpo_std::Kind::QuantityUnit) {
                let name = params[*slot].0.clone();
                unit = self
                    .unit_argument(&value, ty, written, &name)
                    .map(|found| (found, value.clone()));
                continue;
            }
            if table.is_some_and(|table| table.op == jadpo_std::Op::UnitsSum) {
                let given = match ty {
                    Some(ty) => ty,
                    None => self.infer(&value),
                };
                quantity = self.quantities_argument(&value, &given, written);
                continue;
            }
            if table
                .and_then(|op| op.params.get(*slot))
                .is_some_and(|p| p.written_out)
            {
                let name = params[*slot].0.clone();
                self.written_out_argument(&value, written, &name);
                continue;
            }
            if table
                .and_then(|op| op.params.get(*slot))
                .is_some_and(|p| p.character)
            {
                let name = params[*slot].0.clone();
                self.character_argument(&value, written, &name);
                continue;
            }
            if let Some(jadpo_std::Kind::OneOf(accepts)) = kind {
                let name = params[*slot].0.clone();
                self.built_on_argument(&value, ty, accepts, written, &name);
                continue;
            }
            if kind == Some(jadpo_std::Kind::Local) {
                let name = params[*slot].0.clone();
                self.local_argument(&value, ty, written, &name);
                continue;
            }
            if kind == Some(jadpo_std::Kind::Clock) {
                self.clock_argument(&value, ty, written);
                continue;
            }
            if kind == Some(jadpo_std::Kind::Format) {
                if let Some(spec) = self.format_argument(&value, written) {
                    format = Some((value.clone(), spec));
                }
                continue;
            }
            if let (Some(jadpo_std::Kind::Contract), Some(table)) = (kind, table) {
                let given = match ty {
                    Some(ty) => ty,
                    None => self.infer(&value),
                };
                if let Some(entry) =
                    self.contract_value(table.op, &value, Some(given.clone()), written)
                {
                    // `text.format` is told which standard type held as
                    // text its value is, or which named collection (chunk
                    // 5.5o2), which the runtime can't see.
                    // So is a type built on a quantity shown in units of
                    // its own (a heart rate, chunk 5.5m2), and one built on
                    // `Decimal` (a latitude, chunk 5.5s).
                    if let Some(entry) = entry
                        && (matches!(
                            entry.holds,
                            jadpo_std::standard::Holds::Text { .. }
                                | jadpo_std::standard::Holds::Whole { .. }
                                | jadpo_std::standard::Holds::List(_)
                                | jadpo_std::standard::Holds::Map(..)
                        ) || matches!(
                            entry.format.key,
                            jadpo_std::standard::FormatKey::UnitsIn(_)
                        ) || matches!(
                            entry.holds,
                            jadpo_std::standard::Holds::Bounded {
                                of: jadpo_std::Builtin::Decimal,
                                ..
                            }
                        ))
                    {
                        self.facts.shown.insert(node_key(call.syntax()), entry.name);
                    }
                    contract = Some((given, entry));
                }
                continue;
            }
            if kind == Some(jadpo_std::Kind::Type)
                && table.is_some_and(|table| table.op == jadpo_std::Op::CryptoVerifyJwt)
            {
                claims_as = Some(self.claims_type_argument(&value, written));
                continue;
            }
            if kind == Some(jadpo_std::Kind::Type)
                && table.is_some_and(|table| table.op == jadpo_std::Op::JsonRead)
            {
                read_as = self.json_type_argument(&value, written);
                continue;
            }
            if kind == Some(jadpo_std::Kind::Type) {
                if let Some(target) = self.type_argument(&value, written) {
                    parsed = Some((target, None));
                }
                continue;
            }
            // `json.write`'s value is one an `Output` may hold.
            if table.is_some_and(|table| table.op == jadpo_std::Op::JsonWrite) {
                let given = match ty {
                    Some(ty) => ty,
                    None => self.infer(&value),
                };
                self.json_value(&value, &given, written);
                continue;
            }
            // A list of numbers (`list.sum`): any list whose items are
            // built on `Int` or `Decimal`, as operators take them.
            if let Ty::List(item) = &param
                && **item == Ty::Number
            {
                let given = match ty {
                    Some(ty) => ty,
                    None => self.infer(&value),
                };
                let name = params[*slot].0.clone();
                summed = self.numbers_argument(&value, &given, written, &name);
                continue;
            }
            // A list of text (`text.join`), of amounts (`money.sum`) or of
            // whole numbers (`money.allocate`'s ratios): any list whose
            // items are built on `Text`, `Money` or `Int`.
            if let Ty::List(item) = &param
                && matches!(**item, Ty::Prim(Prim::Text | Prim::Money | Prim::Int))
            {
                let given = match ty {
                    Some(ty) => ty,
                    None => self.infer(&value),
                };
                let name = params[*slot].0.clone();
                self.texts_argument(&value, &given, written, &name);
                continue;
            }
            let expected = substitute(&param, &bound);
            // A value that may be `none` (`header: Text?`) takes a value
            // built on the type, or one that may be `none`.
            if let Ty::Nullable(inner) = &expected
                && let Some(accepts) = built_on(inner)
            {
                self.nullable_built_on_argument(&value, ty, accepts, &expected);
                continue;
            }
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
        for (lambda, from, to, slot) in inline {
            let name = &params[slot].0;
            if !self.inline(&lambda, &from, &to, &mut bound, (written, name)) {
                continue;
            }
            if table
                .and_then(|op| op.params.get(slot))
                .is_some_and(|param| param.ordered)
            {
                let key = substitute(&to, &bound);
                self.ordered_key(&lambda, &key, written);
                if let Some(table) = table {
                    self.sort_locale(table, &args, &slots, &key, written);
                }
            }
        }
        self.divisors(&args, &slots, written);
        if let Some(table) = table {
            self.counts(table, &args, &slots, written);
            self.bounded(table, &args, &slots, written);
            self.never_empty(table, &args, &slots, written);
        }
        if let Some(table) = table {
            self.std_options(table, call, &args, &slots, written);
        }
        if let Some((format, spec)) = &format
            && let Some(value) = args
                .iter()
                .zip(&slots)
                .find(|(_, slot)| **slot == Some(0))
                .and_then(|(arg, _)| arg.expr())
        {
            self.format_needs_time(&value, format, spec, written);
        }
        let given = args.len();
        let extra = slots.iter().filter(|slot| slot.is_none()).count();
        // An option may be left out (area 10's `dst:`).
        let optional = |slot: usize| {
            table
                .and_then(|op| op.params.get(slot))
                .is_some_and(|param| param.optional)
        };
        let missing: Vec<String> = params
            .iter()
            .zip(&filled)
            .enumerate()
            .filter(|(slot, (_, filled))| !**filled && !optional(*slot))
            .map(|(_, ((name, _), _))| format!("`{name}`"))
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
        // `text.pick` chooses text for a person as formatting writes it
        // (chunk 5.5o2), so its locale is checked the same way.
        if let Some(table) = table
            && matches!(
                table.op,
                jadpo_std::Op::TextFormat
                    | jadpo_std::Op::TemporalFormatRelative
                    | jadpo_std::Op::TextPick
            )
        {
            self.locale_argument(call, &args, &slots, table, written);
        }
        if let (Some(table), Some((shown, entry))) = (table, &contract) {
            self.contract_options(table, call, &args, &slots, (*entry, shown), written);
        }
        if let (Some(table), Some((target, _))) = (table, &parsed)
            && table.op == jadpo_std::Op::TextParse
        {
            let entry = jadpo_std::standard_type(&target.to_string())
                .or_else(|| self.table.standard_text(target))
                .or_else(|| match self.table.structure(target) {
                    Ty::Named(name) | Ty::Opaque(name, _) => jadpo_std::standard_type(&name),
                    _ => None,
                });
            self.form_option(table, &args, &slots, entry, target);
            self.country_option(table, &args, &slots, entry, target);
        }
        // `text.format`'s `unit:` is a unit of its value's kind (area 19),
        // or of the kind its measures are shown in (chunk 5.5m2).
        if let Some((_, Some(entry))) = &contract
            && let Some(kind) = entry.unit_kind()
        {
            quantity = Some(kind);
        }
        if let (Some(kind), Some((unit_kind, unit_expr))) = (quantity, &unit)
            && kind != *unit_kind
        {
            self.unit_of_another_kind(unit_expr, (*unit_kind, kind), written);
            if table.is_some_and(|table| table.result == jadpo_std::Kind::Quantity) {
                return Ty::Error;
            }
        }
        // A `units` call gives its unit's kind, or its items' (area 19).
        if table.is_some_and(|table| table.result == jadpo_std::Kind::Quantity) {
            return unit
                .map(|(kind, _)| kind)
                .or(quantity)
                .map_or(Ty::Error, |kind| Ty::Prim(Prim::Quantity(kind)));
        }
        if let Some((target, _)) = parsed {
            let raw = args
                .iter()
                .zip(&slots)
                .find(|(_, slot)| **slot == Some(0))
                .and_then(|(arg, _)| arg.expr());
            if let Some(raw) = raw {
                self.parse_fallible(call, &raw, &target);
            }
            return target;
        }
        if table.is_some_and(|table| table.op == jadpo_std::Op::JsonRead) {
            return self.json_read_result(call, read_as);
        }
        if table.is_some_and(|table| table.op == jadpo_std::Op::CryptoVerifyJwt)
            && let Some(claims) = claims_as
        {
            return self.claims_result(call, claims);
        }
        if table.is_some_and(|table| table.op == jadpo_std::Op::TextParse) {
            // `as:` was missing or isn't a type, which is reported; what
            // the call can fail with isn't known.
            self.facts
                .constructions
                .insert(node_key(call.syntax()), Fallible::Unknown);
            return Ty::Error;
        }
        if let Some(table) = table
            && table.op == jadpo_std::Op::LocaleFromAcceptLanguage
        {
            self.locales_for_choosing(call, written);
        }
        // `min` and `max` give one of their values, of one type (area 19).
        if let Some(table) = table {
            let given: Vec<(usize, Expr)> = args
                .iter()
                .zip(&slots)
                .filter_map(|(arg, slot)| Some(((*slot)?, arg.expr()?)))
                .collect();
            if let Some(shared) = self.shared_type(table, &given, written) {
                return shared;
            }
        }
        // A sum is its items' representation, as `+` gives (area 19): a
        // named type is built back from it by construction.
        if result == Ty::Number {
            return summed.map_or(Ty::Error, Ty::Prim);
        }
        // Occurrences from a date are days (chunk 5.5k4).
        if let Some(table) = table
            && table.op == jadpo_std::Op::TemporalOccurrences
            && self.given_a_date(table, &args, &slots)
        {
            return Ty::List(Box::new(Ty::Prim(Prim::CalendarDate)));
        }
        // A range is the range type the call's ranges are (area 19).
        if table.is_some_and(|table| table.result == jadpo_std::Kind::Range) {
            return match ranged {
                Some(range) if matches!(result, Ty::Nullable(_)) => range.nullable(),
                Some(range) => range,
                None => Ty::Error,
            };
        }
        substitute(&result, &bound)
    }

    /// A list of numbers, for `list.sum`: the representation its items are
    /// built on, or `None` once what is wrong is reported.
    fn numbers_argument(
        &mut self,
        value: &Expr,
        given: &Ty,
        written: &str,
        param: &str,
    ) -> Option<Prim> {
        if given.has_error() {
            return None;
        }
        let item = match self.table.structure(given) {
            Ty::List(item) if !self.table.is_nullable(given) => Some(*item),
            _ => None,
        };
        let numbers = item.as_ref().and_then(|item| {
            self.table
                .representation(item)
                .filter(|prim| prim.is_number() && !self.table.is_nullable(item))
        });
        if numbers.is_some() {
            return numbers;
        }
        let span = self.span(value);
        let what = match &item {
            Some(item) => format!("a list of `{item}`"),
            None => super::super::literal::what(self.table, given),
        };
        self.out.push(
            Diagnostic::new(
                &rule::TYPE_MISMATCH,
                format!("`{given}` can't be used where a list of numbers is required"),
            )
            .at(span, format!("this is `{given}`"))
            .reason(format!(
                "`{written}`'s `{param}` takes a list of values built on `Int` or `Decimal`, which `+` adds; `{given}` is {what}."
            ))
            .next("Pass a list of numbers here"),
        );
        None
    }

    /// The key an inline function gives to order items (`sort_by`,
    /// `min_by`, `max_by`): a number, a time, a date, a duration or text,
    /// which the canonical order compares as what they are (area 19).
    fn ordered_key(&mut self, lambda: &ast::LambdaExpr, key: &Ty, written: &str) {
        let Some(body) = lambda.expr() else { return };
        if key.has_error() || matches!(key, Ty::Var(_)) {
            return;
        }
        let span = self.span(&body);
        let nullable = self.table.is_nullable(key);
        let prim = self.table.representation(key);
        let (message, reason, next) = match prim {
            _ if nullable => (
                format!("`{written}`'s key may be `none`, so it can't order the items"),
                format!("The key gives `{key}`, and `none` has no place in an order."),
                "Give a key that is never `none`".to_string(),
            ),
            Some(prim) if super::ops::ordered(prim) || prim == Prim::Text => return,
            _ if self.ordered_standard(key) => return,
            Some(Prim::ZonedTime) => (
                format!("`{written}`'s key gives `{key}`, which has no order of its own"),
                "A `ZonedTime` is an instant and the zone it is seen in, and two in different zones have no one order; their instants do.".to_string(),
                "Order by the instant, with `temporal.instant`".to_string(),
            ),
            _ => (
                format!("`{written}`'s key gives `{key}`, which has no order"),
                format!(
                    "A key orders the items, so it gives a number, a time, a date, a duration or text, each compared as what it is, text by code point; `{key}` is {}.",
                    super::super::literal::what(self.table, key)
                ),
                "Give a key that is a number, a time, a date, a duration or text".to_string(),
            ),
        };
        self.out.push(
            Diagnostic::new(&rule::TYPE_ORDERING_OPERAND, message)
                .at(span, format!("this is `{key}`"))
                .reason(reason)
                .next(next),
        );
    }

    /// A count or a position is never negative (area 19, question 7),
    /// checked as a divisor's zero is: a literal isn't below `0`, and a
    /// value's type has a `min` of at least `0`. So are `decimal.sqrt`'s
    /// value and `decimal.power`'s exponent (area 19, "Numbers").
    fn counts(
        &mut self,
        table: &jadpo_std::Operation,
        args: &[ast::Arg],
        slots: &[Option<usize>],
        written: &str,
    ) {
        for (arg, slot) in args.iter().zip(slots) {
            let (Some(value), Some(slot)) = (arg.expr(), slot) else {
                continue;
            };
            let Some(param) = table.params.get(*slot).filter(|p| p.never_negative) else {
                continue;
            };
            // A list's items, each never negative: `money.allocate`'s ratios
            // (area 19).
            if param.kind == jadpo_std::Kind::List {
                self.items_never_negative(&value, param.name, written);
                continue;
            }
            let span = self.span(&value);
            let shown = tree::written(value.syntax());
            let (message, label, why) = if is_literal(&value) {
                match Const::from_expr(&value) {
                    Const::Number { value: number, .. } if number.mantissa < 0 => (
                        format!("`{written}`'s `{}` is never negative", param.name),
                        "below `0`".to_string(),
                        format!("`{shown}` is below `0`."),
                    ),
                    _ => continue,
                }
            } else {
                match self.facts.types.get(&node_key(value.syntax())).cloned() {
                    // A value that isn't a whole number is already reported.
                    Some(ty)
                        if !ty.has_error()
                            && !self.table.is_nullable(&ty)
                            && matches!(
                                self.table.representation(&ty),
                                Some(Prim::Int | Prim::Decimal)
                            ) =>
                    {
                        if self.never_below_zero(&ty) {
                            continue;
                        }
                        let why = match &ty {
                            Ty::Prim(prim) => format!(
                                "`{shown}` is a plain `{}`, which can be below `0`.",
                                prim.name()
                            ),
                            other => format!("`{other}` allows values below `0`."),
                        };
                        (
                            format!("`{shown}` may be negative"),
                            format!("`{ty}` can be below `0`"),
                            why,
                        )
                    }
                    _ => continue,
                }
            };
            let (meaning, next) = match table.op {
                jadpo_std::Op::DecimalSqrt => (
                    "is a number whose square root is taken, and one below `0` has none",
                    "Pass a value whose type has `min: 0`, building it where a negative one is handled",
                ),
                jadpo_std::Op::DecimalPower => (
                    "says how many times `value` is multiplied by itself, so the result stays exact",
                    "Pass a value whose type has `min: 0`, building it where a negative one is handled",
                ),
                _ => (
                    "counts from the start",
                    "Pass a value whose type has `min: 0`, building it where a negative one is handled; count from the end with `keep_last` or `drop_last`",
                ),
            };
            self.out.push(
                Diagnostic::new(&rule::TYPE_ARGUMENT_NEGATIVE, message)
                    .at(span, label)
                    .reason(format!(
                        "`{written}`'s `{}` {meaning}, so it is never negative, which is proved while compiling: a literal isn't below `0` and a value's type has a `min` of at least `0`; {why}",
                        param.name
                    ))
                    .next(next),
            );
        }
    }

    /// A whole number within bounds (`temporal.occurrences`' `count`, from 1
    /// to 1000, area 19), proved as a count's sign is: a literal within
    /// them, or a value whose type's `min` and `max` are. Anything else is
    /// `type.mismatch`, as a `scale:` out of its range is.
    fn bounded(
        &mut self,
        table: &jadpo_std::Operation,
        args: &[ast::Arg],
        slots: &[Option<usize>],
        written: &str,
    ) {
        for (arg, slot) in args.iter().zip(slots) {
            let (Some(value), Some(slot)) = (arg.expr(), slot) else {
                continue;
            };
            let Some((param, (low, high))) = table
                .params
                .get(*slot)
                .and_then(|param| Some((param, param.within?)))
            else {
                continue;
            };
            let shown = tree::written(value.syntax());
            let (label, why) = if is_literal(&value) {
                match Const::from_expr(&value) {
                    Const::Number {
                        value: number,
                        decimal: false,
                    } if number.scale == 0
                        && (i128::from(low)..=i128::from(high)).contains(&number.mantissa) =>
                    {
                        continue;
                    }
                    Const::Number { .. } => (
                        format!("not from {low} to {high}"),
                        format!("`{shown}` isn't."),
                    ),
                    // Not a number at all is already reported.
                    _ => continue,
                }
            } else {
                match self.facts.types.get(&node_key(value.syntax())).cloned() {
                    Some(ty)
                        if !ty.has_error()
                            && !self.table.is_nullable(&ty)
                            && self.table.representation(&ty) == Some(Prim::Int) =>
                    {
                        let chain = self.table.rule_chain(&ty);
                        let at_least = chain.iter().any(|(_, rules)| {
                            rules.min.is_some_and(|min| {
                                jadpo_std::compare_exact(
                                    min.mantissa,
                                    min.scale,
                                    i128::from(low),
                                    0,
                                )
                                .is_ge()
                            })
                        });
                        let at_most = chain.iter().any(|(_, rules)| {
                            rules.max.is_some_and(|max| {
                                jadpo_std::compare_exact(
                                    max.mantissa,
                                    max.scale,
                                    i128::from(high),
                                    0,
                                )
                                .is_le()
                            })
                        });
                        if at_least && at_most {
                            continue;
                        }
                        let why = match &ty {
                            Ty::Prim(prim) => format!(
                                "`{shown}` is a plain `{}`, which can be any whole number.",
                                prim.name()
                            ),
                            other => format!(
                                "`{other}` allows values outside them: it needs `min: {low}` or more and `max: {high}` or less."
                            ),
                        };
                        (format!("`{ty}` can be outside {low} to {high}"), why)
                    }
                    _ => continue,
                }
            };
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_MISMATCH,
                    format!("`{written}`'s `{}` is a whole number from {low} to {high}", param.name),
                )
                .at(self.span(&value), label)
                .reason(format!(
                    "`{written}`'s `{}` bounds what it gives, so it is from {low} to {high}, which is proved while compiling: a literal within them, or a value whose type's `min` and `max` are; {why}",
                    param.name
                ))
                .next(format!(
                    "Pass a value whose type has `min: {low}` and `max: {high}`, or narrower bounds"
                )),
            );
        }
    }

    /// A list whose items are never negative (`money.allocate`'s ratios,
    /// area 19): each item of a list written out isn't below `0`, and a
    /// value's item type has a `min` of at least `0`.
    fn items_never_negative(&mut self, value: &Expr, param: &str, written: &str) {
        let shown = tree::written(value.syntax());
        let reason = |why: String| {
            format!(
                "`{written}`'s `{param}` are whole numbers never below `0`, which is proved while compiling: an item written out isn't below `0` and a list's item type has a `min` of at least `0`; {why}"
            )
        };
        if let Expr::ListExpr(list) = value {
            for item in list.exprs() {
                if let Const::Number { value: number, .. } = Const::from_expr(&item)
                    && number.mantissa < 0
                {
                    let item_shown = tree::written(item.syntax());
                    self.out.push(
                        Diagnostic::new(
                            &rule::TYPE_ARGUMENT_NEGATIVE,
                            format!("`{written}`'s `{param}` are never negative"),
                        )
                        .at(self.span(&item), "below `0`")
                        .reason(reason(format!("`{item_shown}` is below `0`.")))
                        .next("Give each item as `0` or more"),
                    );
                }
            }
            return;
        }
        let Some(ty) = self.facts.types.get(&node_key(value.syntax())).cloned() else {
            return;
        };
        if ty.has_error() || self.table.is_nullable(&ty) {
            return;
        }
        let Ty::List(item) = self.table.structure(&ty) else {
            return;
        };
        if self.table.is_nullable(&item) || self.never_below_zero(&item) {
            return;
        }
        self.out.push(
            Diagnostic::new(
                &rule::TYPE_ARGUMENT_NEGATIVE,
                format!("`{shown}`'s items may be negative"),
            )
            .at(self.span(value), format!("`{item}` can be below `0`"))
            .reason(reason(format!("`{item}` allows values below `0`.")))
            .next("Pass a list whose item type has `min: 0`"),
        );
    }

    /// Whether a value is proved above zero, as `decimal.ln`'s must be not
    /// to fail (area 19, "Numbers"): a literal above `0`, or a value whose
    /// type has a `min` above `0` somewhere along its chain.
    pub(super) fn above_zero(&self, value: &Expr) -> bool {
        if is_literal(value) {
            return matches!(Const::from_expr(value), Const::Number { value: number, .. } if number.mantissa > 0);
        }
        match self.facts.types.get(&node_key(value.syntax())) {
            Some(ty) if !ty.has_error() && !self.table.is_nullable(ty) => self
                .table
                .rule_chain(ty)
                .iter()
                .any(|(_, rules)| rules.min.is_some_and(|min| min.mantissa > 0)),
            _ => false,
        }
    }

    /// Whether a type's rules leave out every value below zero: a `min` of
    /// at least zero anywhere along its chain.
    fn never_below_zero(&self, ty: &Ty) -> bool {
        self.table
            .rule_chain(ty)
            .iter()
            .any(|(_, rules)| rules.min.is_some_and(|min| min.mantissa >= 0))
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
    pub(super) fn built_on_argument(
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
                // Text written as one of the times accepted takes it:
                // `occurrences`' `from:` is an instant or a date.
                let literal = literal.or_else(|| match Const::from_expr(value) {
                    Const::Str(text) => accepts.iter().copied().find(|prim| match prim {
                        Prim::Instant => jadpo_std::instant_millis(&text).is_some(),
                        Prim::CalendarDate => jadpo_std::date_number(&text).is_some(),
                        _ => false,
                    }),
                    _ => None,
                });
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
        let built: Vec<String> = accepts
            .iter()
            .map(|prim| format!("`{}`", prim.name()))
            .collect();
        let wanted = match accepts {
            [only] => format!("a value built on `{}`", only.name()),
            _ if accepts.iter().all(|prim| prim.is_number()) => "a number".to_string(),
            _ => format!("a value built on {}", built.join(" or ")),
        };
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

    /// An argument to a parameter that takes `none` or a value built on one
    /// of `accepts`: `Text?` takes `none`, a `Text`, a named type built on
    /// `Text`, and either of those that may be `none`.
    fn nullable_built_on_argument(
        &mut self,
        value: &Expr,
        ty: Option<Ty>,
        accepts: &[Prim],
        expected: &Ty,
    ) {
        // A literal is `none` or written as the type is.
        let Some(ty) = ty else {
            self.check(value, expected);
            return;
        };
        if ty.has_error() || matches!(ty, Ty::None) {
            return;
        }
        let built = self
            .table
            .representation(ty.strip_nullable())
            .is_some_and(|prim| accepts.contains(&prim));
        if !built {
            self.require(value, &ty, expected);
        }
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
            // A range's ends, from `Range<T>` or a standard range type
            // (area 19).
            (Ty::Opaque(name, args), actual) if name == jadpo_std::RANGE => {
                if let ([p], Some(item)) = (args.as_slice(), range_item(&actual)) {
                    self.unify(p, &item, bound);
                }
            }
            (Ty::Nullable(p), a) => self.unify(p, &a, bound),
            (Ty::Map(pk, pv), Ty::Map(ak, av)) => {
                self.unify(pk, &ak, bound);
                self.unify(pv, &av, bound);
            }
            _ => {}
        }
    }

    /// A range given to a `range` operation (area 19): a `Range<T>` or a
    /// standard range type (`InstantRange`), of the same range type as the
    /// call's other ranges, since each keeps its own end rule.
    fn range_argument(
        &mut self,
        value: &Expr,
        given: &Ty,
        ranged: &mut Option<Ty>,
        (written, param): (&str, &str),
    ) {
        if given.has_error() {
            return;
        }
        let structure = self.table.structure(given);
        let nullable = self.table.is_nullable(given);
        let why = if nullable {
            Some(format!(
                "`{given}` may be `none`, and `{written}` takes a range"
            ))
        } else if range_item(&structure).is_none() {
            Some(format!(
                "`{written}`'s `{param}` takes a `Range<T>`, an `InstantRange` or a `DateRange`; `{given}` is {}",
                super::super::literal::what(self.table, given)
            ))
        } else {
            None
        };
        if let Some(why) = why {
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_MISMATCH,
                    format!("`{given}` can't be used where a range is required"),
                )
                .at(self.span(value), format!("this is `{given}`"))
                .reason(format!("{why}."))
                .next("Pass a range here"),
            );
            return;
        }
        match ranged {
            Some(first) if *first != structure && !first.has_error() => {
                let first = first.clone();
                self.out.push(
                    Diagnostic::new(
                        &rule::TYPE_MISMATCH,
                        format!("`{given}` can't be used where `{first}` is required"),
                    )
                    .at(self.span(value), format!("this is `{given}`"))
                    .reason(format!(
                        "`{written}`'s ranges are all one range type, since each keeps its own end rule; its first is `{first}`."
                    ))
                    .next(format!("Pass a range of the same type, `{first}`, here")),
                );
            }
            Some(_) => {}
            None => *ranged = Some(structure),
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
    /// one pure expression (area 3, question 13). With two parameters,
    /// `(key, value) -> expression`, each takes the type of the value the
    /// operation gives it in that place (area 19). False when it has a
    /// different number of parameters than the operation gives, which is
    /// reported, its body checked with them unknown.
    pub(super) fn inline(
        &mut self,
        lambda: &ast::LambdaExpr,
        from: &[Ty],
        to: &Ty,
        bound: &mut BTreeMap<String, Ty>,
        (written, param): (&str, &str),
    ) -> bool {
        let names: Vec<ast::Name> = lambda.names().collect();
        if names.len() != from.len() {
            self.inline_parameters(lambda, &names, from.len(), written, param);
            self.misplaced_body(lambda);
            return false;
        }
        let previous = self.context;
        self.context = Context::Inline;
        self.scopes.push(Vec::new());
        for (name, from) in names.into_iter().zip(from) {
            self.declare(
                Some(name),
                substitute(from, bound),
                LocalKind::Inline,
                None,
                "parameter",
            );
        }
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
        true
    }

    /// An inline function given to an operation that gives its function a
    /// different number of values (area 19): one item at a time, or two
    /// for `list.zip_with`'s pairs and a map's keys and values.
    fn inline_parameters(
        &mut self,
        lambda: &ast::LambdaExpr,
        names: &[ast::Name],
        takes: usize,
        written: &str,
        param: &str,
    ) {
        let span = self.span(lambda);
        let number = |n: usize| match n {
            1 => "one".to_string(),
            2 => "two".to_string(),
            n => n.to_string(),
        };
        let count = |n: usize| match n {
            1 => "one parameter".to_string(),
            n => format!("{} parameters", number(n)),
        };
        let shown: Vec<String> = names.iter().filter_map(tree::name_text).collect();
        let (reason, next) = if takes == 1 {
            let first = shown.first().map_or("item", String::as_str);
            (
                format!(
                    "`{written}` gives its function one value at a time; two parameters, `(a, b) -> ...`, are only for the operations that give two, `list.zip_with`, `map.filter` and `map.map_values`."
                ),
                format!("Write `{first} -> ...`, with one parameter"),
            )
        } else {
            (
                if written.starts_with("map.") {
                    format!(
                        "`{written}` gives its function each entry's key and its value, so it is written `(key, value) -> ...`, with a name for each."
                    )
                } else {
                    format!(
                        "`{written}` gives its function an item from each list at a time, so it is written `(a, b) -> ...`, with a name for each."
                    )
                },
                if written.starts_with("map.") {
                    "Write `(key, value) -> ...`, naming both".to_string()
                } else {
                    "Write `(a, b) -> ...`, naming both".to_string()
                },
            )
        };
        self.out.push(
            Diagnostic::new(
                &rule::TYPE_INLINE_FUNCTION_PARAMETERS,
                format!(
                    "`{written}`'s `{param}` takes {}, not {}",
                    count(takes),
                    number(names.len())
                ),
            )
            .at(span, count(names.len()))
            .reason(reason)
            .next(next),
        );
    }

    /// An inline function anywhere but as an argument to a collection
    /// operation.
    pub(super) fn misplaced_inline(&mut self, lambda: &ast::LambdaExpr) {
        let span = self.span(lambda);
        let reason = if lambda.names().count() > 1 {
            "Functions aren't values: `(key, value) -> expression` is written only as an argument to a standard operation that gives its function two values, `list.zip_with`, `map.filter` or `map.map_values`."
        } else {
            "Functions aren't values: `item -> expression` is written only as an argument to a standard-library collection operation, such as `list.map` or `list.filter`."
        };
        self.out.push(
            Diagnostic::new(
                &rule::TYPE_INLINE_FUNCTION_PLACEMENT,
                "An inline function is only an argument to a collection operation",
            )
            .at(span, "inline function")
            .reason(reason)
            .next("Write the expression on its own, or pass it to a collection operation"),
        );
        self.misplaced_body(lambda);
    }

    /// An inline function's body, checked with its parameters unknown once
    /// what is wrong with the function is reported.
    fn misplaced_body(&mut self, lambda: &ast::LambdaExpr) {
        let previous = self.context;
        self.context = Context::Inline;
        self.scopes.push(Vec::new());
        for name in lambda.names() {
            self.declare(Some(name), Ty::Error, LocalKind::Inline, None, "parameter");
        }
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
        let (ty, mut verdict) = self.construct_checked(written, field, call);
        // A construction in a constant's value is worked out and checked
        // against its rules while compiling (area 3), so it never fails.
        if self.context == Context::Constant {
            verdict = Fallible::Never;
        }
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
                        Some(Def::Scalar { .. }) => Ty::Named(self.table.canonical(written)),
                        Some(Def::Enum { variants }) => {
                            let variants = variants.clone();
                            self.enum_constructor(written, &variants, call);
                            return (Ty::Named(self.table.canonical(written)), Fallible::Unknown);
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
                            return (Ty::Named(self.table.canonical(written)), Fallible::Unknown);
                        }
                        Some(Def::Format { .. }) => {
                            self.args_only(call);
                            let span = call
                                .expr()
                                .map_or_else(|| self.span(call), |e| self.span(&e));
                            self.out
                                .push(super::super::formats::not_a_value(written, span, false));
                            return (Ty::Error, Fallible::Unknown);
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
        // A key comes only from its setting, given where the compiler owns
        // the place, as a `crypto` call's `key:` (area 19, "Hashing and
        // cryptography"): code never makes one, from a literal or from
        // data.
        if let Some(key) = self.table.base_prim(&target)
            && key.is_key()
        {
            self.infer(value);
            let span = self.span(value);
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_CONSTRUCTOR_INPUT,
                    format!("`{written}` isn't built from a value"),
                )
                .at(span, "a key made in code")
                .reason(format!(
                    "{} `{}` is a secret that comes only from its setting, so it never lands in the source, a log or the database; code never holds one, and gives the setting, `config.<name>`, only where the compiler owns the place, such as a `crypto` call's `key:`.",
                    if key.name().starts_with('E') { "An" } else { "A" },
                    key.name()
                ))
                .next(format!(
                    "Declare `<name>: {}` in `config.jadpo`, and give `config.<name>` where the key is needed, such as a `crypto` call's `key:`",
                    key.name()
                )),
            );
            return (target, Fallible::Unknown);
        }
        let into_percentage = self.table.representation(&target) == Some(Prim::Percentage);
        // A ratio becomes a percentage by construction, `Percentage(0.25)`
        // being 25% (area 19, "Chosen in chunk 5.5d").
        if into_percentage
            && is_literal(value)
            && let Const::Number {
                value: number,
                decimal,
            } = Const::from_expr(value)
        {
            if !decimal {
                self.not_a_ratio(value, &target, Some(number));
                return (target, Fallible::Unknown);
            }
            let as_percent = Const::Percentage(jadpo_std::percentage::canonical(
                number.mantissa,
                number.scale,
            ));
            if let Err(problem) = super::super::literal::check(self.table, &target, &as_percent) {
                self.literal_problem(value, &target, problem, true);
            }
            self.facts
                .types
                .insert(node_key(value.syntax()), target.clone());
            return (target, Fallible::Never);
        }
        if is_literal(value) {
            // Checked while compiling (area 2), and decoded as the type it
            // builds: `DueAt("2026-10-25T08:30:00Z")` is an instant.
            self.literal(value, &target, true);
            self.facts
                .types
                .insert(node_key(value.syntax()), target.clone());
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
        // A secret value builds only a type built on the same secret
        // (area 19, chunk 5.5r2): `StaffPassword(password)`, never text.
        if let Some(secret) = super::super::secrets::secret_in(self.table, &actual)
            && super::super::secrets::secret_of(self.table, &target) != Some(secret)
        {
            let span = self.span(value);
            self.out.push(super::super::secrets::made_from(
                &format!("`{written}(...)`"),
                secret,
                span,
            ));
            return (target, Fallible::Unknown);
        }
        if self.table.is_nullable(&actual) && !self.table.is_nullable(&target) {
            // `none` follows no type's rules, so the absent case is handled
            // before anything is built from the value.
            let span = self.span(value);
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_CONSTRUCTOR_INPUT,
                    format!("`{target}` can't be built from `{actual}`"),
                )
                .at(span, format!("this is `{actual}`"))
                .reason(format!(
                    "`{actual}` may be `none`, and a `{target}` can't be."
                ))
                .next(format!(
                    "Use `match` to handle `none` before building a `{target}`"
                )),
            );
            return (target, Fallible::Unknown);
        }
        if self.table.assignable(&actual, &target) {
            return (target, Fallible::Never);
        }
        if into_percentage {
            match self.table.representation(&actual) {
                Some(Prim::Decimal) => {
                    let fallible = self.proved(value, &actual, &target, call);
                    return (target, fallible);
                }
                Some(Prim::Percentage) => {}
                _ => {
                    self.not_a_ratio(value, &target, None);
                    return (target, Fallible::Unknown);
                }
            }
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
        // A type built on an object is built from a value of that object,
        // its representation, re-checking the rules it adds: a rate type's
        // currencies (area 19, chunk 5.5h4).
        let object = self.table.structure(&target);
        let is_object = match &object {
            Ty::Named(name) => matches!(
                self.table.def(name),
                Some(Def::Fields {
                    base: FieldsBase::Object,
                    ..
                })
            ),
            Ty::Opaque(name, args) => {
                args.is_empty() && super::super::standard_fields(name).is_some()
            }
            _ => false,
        };
        if is_object && self.table.structure(&actual) == object {
            let fallible = self.proved(value, &actual, &target, call);
            return (target, fallible);
        }
        let from = self.table.representation(&actual);
        let to = self.table.representation(&target);
        // A number where a quantity goes needs its unit (area 19).
        if let Some(Prim::Quantity(kind)) = to
            && from.is_some_and(Prim::is_number)
        {
            let span = self.span(value);
            let given = tree::written(value.syntax());
            self.out.push(super::super::literal::unit_required(
                kind,
                &given,
                &target.to_string(),
                span,
            ));
            return (target, Fallible::Unknown);
        }
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

    /// A `crypto` call's `key:` given anything but a key setting.
    fn key_argument(&mut self, value: &Expr, written: &str, key: Prim) {
        let given = tree::written(value.syntax());
        let span = self.span(value);
        self.out.push(
            Diagnostic::new(
                &rule::TYPE_MISMATCH,
                format!("`{written}`'s `key:` takes a setting, not `{given}`"),
            )
            .at(span, "not `config.<name>`")
            .reason(format!(
                "A key is opaque and comes only from its setting, so a `crypto` call's key is written `key: config.<name>`, reading a `{}` setting, the one place code gives a key; no other value holds one.",
                key.name()
            ))
            .next(format!(
                "Declare `<name>: {}` in `config.jadpo`, and write `key: config.<name>`",
                key.name()
            )),
        );
    }

    /// `Percentage(value)` from something that isn't a ratio: a whole
    /// number, which could be a ratio or a percentage already, or a value
    /// that isn't a `Decimal` (area 19).
    fn not_a_ratio(&mut self, value: &Expr, target: &Ty, whole: Option<jadpo_std::Num>) {
        let written = tree::written(value.syntax());
        let span = self.span(value);
        let problem = Diagnostic::new(
            &rule::TYPE_CONSTRUCTOR_INPUT,
            format!("`{target}` can't be built from `{written}`"),
        );
        let problem = match whole {
            Some(number) => problem
                .at(span, "a whole number")
                .reason(format!(
                    "A percentage is built from a ratio, a `Decimal` where 0.25 is 25%, and `{written}` is a whole number, which could be a ratio or a percentage already."
                ))
                .next(super::super::literal::percent_readings(number, &written)),
            None => {
                let actual = self
                    .facts
                    .types
                    .get(&node_key(value.syntax()))
                    .map_or_else(|| "this".to_string(), |ty| format!("`{ty}`"));
                problem
                    .at(span, format!("this is {actual}"))
                    .reason(format!(
                        "A percentage is built from a ratio, a value built on `Decimal` where 0.25 is 25%, or from another percentage; {actual} is neither."
                    ))
                    .next("Pass a `Decimal` ratio, or write the percentage with `%`")
            }
        };
        self.out.push(problem);
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
    /// when it isn't such a collection; one that may be `none` is reported
    /// before, as for every construction.
    fn collection_input(&mut self, value: &Expr, actual: &Ty, target: &Ty) -> bool {
        let into = self.table.structure(target);
        let from = self.table.structure(actual);
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
        if same_kind && items_fit {
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
/// The type a range's ends are: `T` for a `Range<T>`, `Instant` for an
/// `InstantRange`, `CalendarDate` for a `DateRange`; `None` for anything
/// but a range (area 19).
pub(super) fn range_item(ty: &Ty) -> Option<Ty> {
    match ty {
        Ty::Opaque(name, args) if name == jadpo_std::RANGE => match args.as_slice() {
            [item] => Some(item.clone()),
            _ => None,
        },
        // A standard range type: an `InstantRange`'s instants, a
        // `DateRange`'s dates.
        Ty::Opaque(name, args) if args.is_empty() => {
            jadpo_std::range_type(name).map(|(item, _)| Ty::Prim(item))
        }
        _ => None,
    }
}

fn built_on(param: &Ty) -> Option<&'static [Prim]> {
    match param {
        Ty::Number => Some(&[Prim::Int, Prim::Decimal]),
        Ty::Prim(Prim::Int) => Some(&[Prim::Int]),
        Ty::Prim(Prim::Decimal) => Some(&[Prim::Decimal]),
        Ty::Prim(Prim::Text) => Some(&[Prim::Text]),
        Ty::Prim(Prim::Bytes) => Some(&[Prim::Bytes]),
        Ty::Prim(Prim::Instant) => Some(&[Prim::Instant]),
        Ty::Prim(Prim::CalendarDate) => Some(&[Prim::CalendarDate]),
        Ty::Prim(Prim::ZonedTime) => Some(&[Prim::ZonedTime]),
        Ty::Prim(Prim::Duration) => Some(&[Prim::Duration]),
        Ty::Prim(Prim::Money) => Some(&[Prim::Money]),
        // A percentage or a quantity of one kind, as `colour.from_hsl`'s
        // hue and saturation take (chunk 5.5p).
        Ty::Prim(Prim::Percentage) => Some(&[Prim::Percentage]),
        Ty::Prim(prim @ Prim::Quantity(_)) => QUANTITIES
            .iter()
            .find(|each| *each == prim)
            .map(std::slice::from_ref),
        _ => None,
    }
}

/// Each quantity kind as the built-in type a value of it is built on.
const QUANTITIES: [Prim; jadpo_std::QuantityKind::ALL.len()] = {
    let mut out = [Prim::Int; jadpo_std::QuantityKind::ALL.len()];
    let mut at = 0;
    while at < out.len() {
        out[at] = Prim::Quantity(jadpo_std::QuantityKind::ALL[at]);
        at += 1;
    }
    out
};

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
            from.iter().map(|from| substitute(from, bound)).collect(),
            Box::new(substitute(to, bound)),
        ),
        Ty::Opaque(name, args) if !args.is_empty() => Ty::Opaque(
            name.clone(),
            args.iter().map(|arg| substitute(arg, bound)).collect(),
        ),
        other => other.clone(),
    }
}

/// Whether `value` reads a setting as it is, `config.<name>`.
fn is_setting_read(value: &Expr) -> bool {
    dotted(value).is_some_and(|(first, rest)| {
        tree::name_ref_text(&first).as_deref() == Some("config") && rest.len() == 1
    })
}
