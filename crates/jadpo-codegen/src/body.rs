//! One callable's body as Rust (docs/compiler-runtime.md §2.5, "Compiled
//! code"): its statements and expressions in the order the interpreter's
//! walker runs them, each site that can stop calling the runtime with the
//! site's own number, so a fault, a failure or a failed assertion names the
//! same frames and a record the same sites whichever executor ran it.
//!
//! Only control flow is the generated code's own. A local is a `Value`
//! variable; an expression that can stop is `?` on the runtime's `Ran`, or,
//! inside a block that catches a stop (an `assert`'s condition, an
//! `attempt`'s construction, a `match`'s subject), a `break` out of that
//! labelled block; a call into an action goes through the runtime's
//! boundary by number; an inline function is a closure the runtime's
//! standard library calls per item. What a failure arm handles is a Rust
//! variable in scope while its arm runs, which `propagate` passes on and
//! `reject` keeps as its cause. A form not compiled yet is refused by name
//! ([`Refusal`]) and gives no code.

use jadpo_graph::body::{
    Arm, ArmBody, BinaryOp, Body, Callee, Expr, IfArm, LocalId, Node, Pattern, RuleArm, SiteId,
    Stmt, UnaryOp,
};
use jadpo_graph::{Kind, TypeRef};
use jadpo_plan::{ArmPattern, CallablePlan, Decoded, Plan};
use jadpo_std::Kind as ParamKind;
use proc_macro2::{Ident, Literal, TokenStream};
use quote::{format_ident, quote};

use crate::Refusal;

/// How a callable runs, which decides its function's shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Shape {
    /// A function, a library function or a constant's body: a plain `fn`.
    Function,
    /// An action or a fixture: an `async fn` the boundary runs.
    Action,
    /// A test or a seed: an `async fn` run as it is called.
    Body,
}

impl Shape {
    pub fn of(kind: Kind) -> Shape {
        match kind {
            Kind::Action | Kind::Fixture => Shape::Action,
            Kind::Test | Kind::Seed => Shape::Body,
            _ => Shape::Function,
        }
    }

    pub fn waits(self) -> bool {
        self != Shape::Function
    }
}

/// Whether a callable of `kind` is built only with the `testing` feature:
/// tests, fixtures (a test's own actions) and seeds, which only `jadpo
/// test` and `jadpo dev` run (chunk 8.1e).
pub(crate) fn testing_only(kind: Kind) -> bool {
    matches!(kind, Kind::Test | Kind::Fixture | Kind::Seed)
}

pub(crate) fn number(n: u32) -> Literal {
    Literal::u32_unsuffixed(n)
}

fn text(text: &str) -> Literal {
    Literal::string(text)
}

/// One body's compiler.
pub(crate) struct Emitter<'a> {
    pub plan: &'a Plan,
    pub callable: &'a CallablePlan,
    pub shape: Shape,
    /// Each callable's function, by number, as another module names it.
    pub paths: &'a [TokenStream],
    pub refusals: &'a mut Vec<Refusal>,
    /// Where a stop goes: out to the label of the innermost block that
    /// catches it (an `assert`'s condition, an `attempt`'s construction, a
    /// `match`'s subject), or out of the function with `None`.
    pub label: Option<TokenStream>,
    /// Inside an `assert`'s condition: each value worked out, but a
    /// literal's, is kept for its report.
    pub recording: bool,
    /// The failures the outcome `match` arms being compiled handle,
    /// innermost last, each a variable while its arm runs: what
    /// `propagate` passes on and what a `reject` keeps as its cause.
    pub handling: Vec<Ident>,
    /// Inside an inline function's body, its site: a closure, where
    /// nothing waits.
    pub inline: Option<SiteId>,
}

/// A labelled block's label, `'name`.
fn label(name: &Ident) -> TokenStream {
    let tick = proc_macro2::Punct::new('\'', proc_macro2::Spacing::Joint);
    quote!(#tick #name)
}

impl Emitter<'_> {
    fn body(&self) -> &Body {
        &self.callable.body
    }

    fn decoded(&self, id: SiteId) -> Option<&Decoded> {
        self.callable
            .sites
            .get(id.0 as usize)
            .map(|site| &site.decoded)
    }

    /// Whether the body is a test's, whose steps deliver what they emitted
    /// before its next statement (area 13; chunk 6.11).
    fn test(&self) -> bool {
        self.callable.kind == Kind::Test
    }

    /// Whether the body is a test's, a fixture's or a seed's, which runs
    /// each call as the caller it passes (area 9).
    fn test_like(&self) -> bool {
        matches!(self.callable.kind, Kind::Test | Kind::Fixture | Kind::Seed)
    }

    /// Refuses `form`, written at site `at`: no code is written for it.
    fn refuse(&mut self, form: String, at: SiteId) -> TokenStream {
        let line = self
            .body()
            .sites
            .get(at.0 as usize)
            .map_or(self.callable.line, |site| site.line);
        self.refusals.push(Refusal {
            form,
            file: self.callable.file.clone(),
            line,
            callable: self.callable.label.clone(),
        });
        quote!(Value::Unit)
    }

    /// What site `at` is written as, up to its arguments or its block:
    /// `list.count`, `Todo.update`.
    fn written(&self, at: SiteId) -> String {
        let Some(site) = self.body().sites.get(at.0 as usize) else {
            return String::new();
        };
        let source = self
            .plan
            .text(&self.callable.file)
            .and_then(|text| text.get(site.start as usize..site.end as usize))
            .unwrap_or_default();
        let line = source.lines().next().unwrap_or_default();
        let end = line.find(['(', '{']).unwrap_or(line.len());
        line[..end].trim().to_string()
    }

    // --- stopping -------------------------------------------------------------

    /// `ran`, a `Result` of the runtime's, unwrapped: its stop ends the
    /// body, or goes to the block that catches it.
    fn tri(&self, ran: TokenStream) -> TokenStream {
        match &self.label {
            None => quote!(#ran?),
            Some(label) => quote!(match #ran {
                Ok(value) => value,
                Err(stop) => break #label Err(stop),
            }),
        }
    }

    /// Stops with `stop`, a `Stop`.
    fn fail(&self, stop: TokenStream) -> TokenStream {
        match &self.label {
            None => quote!(return Err(#stop)),
            Some(label) => quote!(break #label Err(#stop)),
        }
    }

    /// Expression `id` worked out inside a block labelled `name` that
    /// catches its stops: a Rust expression giving a `Ran`.
    fn caught(&mut self, name: &Ident, id: SiteId) -> TokenStream {
        let label = label(name);
        let outer = self.label.replace(label.clone());
        let value = self.expr(id);
        self.label = outer;
        quote!(#label: { Ok::<Value, Stop>(#value) })
    }

    /// The defect `what` at site `at`, as an expression that never gives.
    fn defect(&self, what: &str, at: SiteId) -> TokenStream {
        let (what, at) = (text(what), number(at.0));
        self.fail(quote!(rt::defect(cx, #what, #at)))
    }

    /// After a step of a test at site `at`, which gave `ran`: what it
    /// emitted delivered before the next statement; a delivery that failed
    /// for good ends the test there.
    fn stepped(&self, ran: TokenStream, at: SiteId, advanced: bool) -> TokenStream {
        if !self.test() {
            return ran;
        }
        let at = number(at.0);
        quote!({
            let ran = #ran;
            match rt::step(cx, p, #advanced, #at, ran).await {
                Ok(ran) => ran,
                Err(ended) => return ended,
            }
        })
    }

    /// A site that waits, `at`, inside an inline function, which is a
    /// closure nothing waits in: the interpreter's defect at the inline
    /// function's body, which the checker rules out.
    fn waits_inline(&self) -> Option<TokenStream> {
        let body = self.inline?;
        let defect = self.defect("an inline function that pauses", body);
        Some(quote!({ #defect }))
    }

    /// In a test, fixture or seed, the caller a call runs as: the value at
    /// `place` among `names`, or none (area 9).
    fn call_as(&self, place: Option<Option<u32>>, names: &[proc_macro2::Ident]) -> TokenStream {
        if !self.test_like() {
            return TokenStream::new();
        }
        match place {
            None => TokenStream::new(),
            Some(place) => match place.and_then(|at| names.get(at as usize)) {
                Some(name) => quote!(jadpo_runtime::call_as(cx, Some(&#name));),
                None => quote!(jadpo_runtime::call_as(cx, None);),
            },
        }
    }

    // --- callables --------------------------------------------------------------

    /// The callable's function and its table entry.
    pub fn callable(&mut self, number_of: u32, name: &proc_macro2::Ident) -> TokenStream {
        let body = &self.callable.body;
        let params: Vec<proc_macro2::Ident> = body
            .params
            .iter()
            .map(|param| format_ident!("l{}", param.0))
            .collect();
        let locals: Vec<proc_macro2::Ident> = (0..body.locals.len())
            .filter(|at| !body.params.iter().any(|param| param.0 as usize == *at))
            .map(|at| format_ident!("l{}", at))
            .collect();
        let pauses = self.callable.body.sites.iter().position(|site| site.pauses);
        if let (false, Some(at)) = (self.shape.waits(), pauses) {
            let at = SiteId(u32::try_from(at).unwrap_or(u32::MAX));
            let written = self.written(at);
            self.refuse(format!("`{written}` in a function, which never waits"), at);
        }
        let block = self.block(&self.callable.body.block.clone());
        let entry = crate::names::entry(number_of);
        let cfg = if testing_only(self.callable.kind) {
            quote!(#[cfg(feature = "testing")])
        } else {
            TokenStream::new()
        };
        let takes = params.iter().map(|_| quote!(args.take()));
        let doc = text(&format!(
            " `{}`, {}:{}.",
            self.callable.label, self.callable.file, self.callable.line
        ));
        if self.shape.waits() {
            let named: Vec<proc_macro2::Ident> = (0..params.len())
                .map(|at| format_ident!("a{}", at))
                .collect();
            quote! {
                #[doc = #doc]
                #cfg
                pub(crate) async fn #name(cx: &mut Ctx<'_>, p: &Program, #(mut #params: Value),*) -> Ran {
                    #(let mut #locals = Value::Unit;)*
                    #block
                    Ok(Value::Unit)
                }

                #cfg
                pub(crate) fn #entry<'a, 'p>(cx: &'a mut Ctx<'p>, p: &'a Program, args: Vec<Value>) -> LocalFuture<'a, Ran> {
                    let mut args = Args::new(args);
                    #(let #named = #takes;)*
                    Box::pin(#name(cx, p, #(#named),*))
                }
            }
        } else {
            quote! {
                #[doc = #doc]
                #cfg
                pub(crate) fn #name(cx: &mut Ctx<'_>, p: &Program, #(mut #params: Value),*) -> Ran {
                    #(let mut #locals = Value::Unit;)*
                    #block
                    Ok(Value::Unit)
                }

                #cfg
                pub(crate) fn #entry(cx: &mut Ctx<'_>, p: &Program, args: Vec<Value>) -> Ran {
                    let mut args = Args::new(args);
                    #name(cx, p, #(#takes),*)
                }
            }
        }
    }

    // --- statements -------------------------------------------------------------

    fn block(&mut self, stmts: &[SiteId]) -> TokenStream {
        let stmts: Vec<TokenStream> = stmts.iter().map(|stmt| self.stmt(*stmt)).collect();
        quote!(#(#stmts)*)
    }

    /// The value of statement `id`'s expression `value`, or the defect of
    /// a statement without one.
    fn needed(&mut self, value: Option<SiteId>, id: SiteId) -> TokenStream {
        match value {
            Some(value) => self.expr(value),
            None => {
                let defect = self.defect("a statement without its value", id);
                quote!({ #defect })
            }
        }
    }

    fn stmt(&mut self, id: SiteId) -> TokenStream {
        let Some(Node::Stmt(stmt)) = self.body().sites.get(id.0 as usize).map(|site| &site.node)
        else {
            let defect = self.defect("an expression where a statement belongs", id);
            return quote!(#defect;);
        };
        let stmt = stmt.clone();
        match stmt {
            Stmt::Var { local, value } | Stmt::Assign { local, value } => {
                let value = self.needed(value, id);
                match local {
                    Some(local) => {
                        let local = format_ident!("l{}", local.0);
                        quote!(#local = #value;)
                    }
                    None => {
                        let defect = self.defect("a value given to no local", id);
                        quote!({ let _ = #value; #defect; })
                    }
                }
            }
            Stmt::Return { value } => match value {
                Some(value) => {
                    let value = self.expr(value);
                    quote!(return Ok(#value);)
                }
                None => quote!(return Ok(Value::Unit);),
            },
            Stmt::If {
                arms, otherwise, ..
            } => {
                let chain = self.if_from(&arms, &otherwise, id);
                quote!(#chain)
            }
            Stmt::Expr { value } => {
                let value = self.needed(value, id);
                quote!(let _ = #value;)
            }
            Stmt::Assert { value, fails } => self.assert(id, value, fails.as_ref()),
            Stmt::For { item, list, body } => self.each(id, item, list, &body),
            Stmt::Emit { value } => {
                let built = value
                    .and_then(|built| self.body().sites.get(built.0 as usize))
                    .map(|site| site.ty.clone());
                let value = self.needed(value, id);
                let Some(TypeRef::Node(key)) = built else {
                    let defect = self.defect("an event of no declared type", id);
                    return quote!({ let _ = #value; #defect; });
                };
                let key = text(key.as_str());
                let at = number(id.0);
                let emitted = self.tri(quote!(rt::emit(cx, p, #key, event, #at)));
                quote!({
                    let event = #value;
                    #emitted;
                })
            }
        }
    }

    /// `for item in list { … }`: the list worked out, then the body run
    /// with `item` set to each of its items in turn, a set's in the
    /// canonical order it is held in.
    fn each(
        &mut self,
        id: SiteId,
        item: Option<LocalId>,
        list: Option<SiteId>,
        body: &[SiteId],
    ) -> TokenStream {
        let list = self.needed(list, id);
        let at = number(id.0);
        let items = self.tri(quote!(rt::items(cx, &list, #at)));
        let set = match item {
            Some(local) => {
                let local = format_ident!("l{}", local.0);
                quote!(#local = item.clone();)
            }
            None => {
                let defect = self.defect("a value given to no local", id);
                quote!(#defect;)
            }
        };
        let body = self.block(body);
        quote!({
            let list = #list;
            let items = #items;
            for item in items.iter() {
                #set
                #body
            }
        })
    }

    /// The `if` chain from its first condition: each condition worked out
    /// only when the ones before it were false, then the `else`.
    fn if_from(&mut self, arms: &[IfArm], otherwise: &[SiteId], at: SiteId) -> TokenStream {
        let Some((arm, rest)) = arms.split_first() else {
            let block = self.block(otherwise);
            return quote!({ #block });
        };
        let condition = self.needed(arm.condition, at);
        let then = self.block(&arm.then);
        let rest = self.if_from(rest, otherwise, at);
        let at = number(at.0);
        let holds = self.tri(quote!(rt::truth(cx, &condition, Truth::Condition, #at)));
        quote!({
            let condition = #condition;
            if #holds {
                #then
            } else #rest
        })
    }

    /// `assert condition`, or `assert call(…) fails Failure` (area 13): its
    /// parts recorded while the condition is worked out, in a block a stop
    /// breaks out of, so the assertion's recording ends however it went.
    fn assert(
        &mut self,
        id: SiteId,
        value: Option<SiteId>,
        fails: Option<&TypeRef>,
    ) -> TokenStream {
        let Some(value) = value else {
            let defect = self.defect("an `assert` without a condition", id);
            return quote!(#defect;);
        };
        let name = format_ident!("assert{}", id.0);
        let label = label(&name);
        let outer = (self.label.replace(label.clone()), self.recording);
        self.recording = true;
        let at = number(id.0);
        let held = match fails {
            Some(TypeRef::Node(expected)) => {
                let ran = self.subject(value);
                let key = text(expected.as_str());
                let named = match self.decoded(id) {
                    Some(Decoded::AssertFails { expected }) => expected.clone(),
                    _ => expected.as_str().to_string(),
                };
                let named = text(&named);
                quote!({
                    let ran = #ran;
                    rt::assert_fails(cx, ran, #key, #named, #at)
                })
            }
            Some(_) => {
                let defect = self.defect("an `assert … fails` naming no failure", id);
                quote!({ #defect })
            }
            None => {
                let compared = match self
                    .body()
                    .sites
                    .get(value.0 as usize)
                    .map(|site| &site.node)
                {
                    Some(Node::Expr(Expr::Binary { op, lhs, rhs })) => {
                        comparison(*op).map(|operator| (operator, *lhs, *rhs))
                    }
                    _ => None,
                };
                match compared {
                    Some((operator, lhs, rhs)) => {
                        let lhs = self.expr(lhs);
                        let rhs = self.expr(rhs);
                        let site = number(value.0);
                        quote!({
                            let left = #lhs;
                            let right = #rhs;
                            rt::assert_compare(cx, Operator::#operator, left, right, #site, #at)
                        })
                    }
                    None => {
                        let value = self.expr(value);
                        quote!({
                            let value = #value;
                            rt::assert_true(cx, &value, #at)
                        })
                    }
                }
            }
        };
        (self.label, self.recording) = outer;
        let ended = self.tri(quote!(rt::assertion_end(cx, held)));
        quote! {
            cx.assertion_begin();
            let held: Result<(), Stop> = #label: #held;
            #ended;
        }
    }

    // --- expressions ------------------------------------------------------------

    /// Expression `id`'s value: a Rust expression giving a `Value`.
    fn expr(&mut self, id: SiteId) -> TokenStream {
        let Some(site) = self.body().sites.get(id.0 as usize) else {
            let defect = self.defect("a site the body doesn't have", id);
            return quote!({ #defect });
        };
        let Node::Expr(expr) = site.node.clone() else {
            let defect = self.defect("a statement where a value belongs", id);
            return quote!({ #defect });
        };
        let literal = matches!(expr, Expr::Literal(_));
        let value = self.value(id, expr);
        // An assertion keeps the value of each part of its condition but a
        // literal, whose value is in the source already (area 13).
        if self.recording && !literal {
            let at = number(id.0);
            quote!({
                let value = #value;
                cx.record(#at, &value);
                value
            })
        } else {
            value
        }
    }

    fn value(&mut self, id: SiteId, expr: Expr) -> TokenStream {
        let at = number(id.0);
        match expr {
            Expr::Literal(_) => match self.decoded(id) {
                Some(Decoded::Constant(Some(constant))) => {
                    let constant = number(constant.0);
                    quote!(p.constant(#constant))
                }
                _ => {
                    let defect = self.defect("a literal the checker would have refused", id);
                    quote!({ #defect })
                }
            },
            Expr::Local(local) => {
                let local = format_ident!("l{}", local.0);
                quote!(#local.clone())
            }
            Expr::Field { receiver, .. } => {
                let name = match self.decoded(id) {
                    Some(Decoded::Field { name }) => name.clone(),
                    _ => String::new(),
                };
                let receiver = self.expr(receiver);
                let name = text(&name);
                let read = self.tri(quote!(rt::field(cx, &receiver, #name, #at)));
                quote!({
                    let receiver = #receiver;
                    #read
                })
            }
            Expr::Call { callee, args, .. } => self.call(id, &callee, &args),
            Expr::Build { callee, fields, .. } => {
                if !matches!(callee, Callee::Object(_) | Callee::Variant(_)) {
                    let defect =
                        self.defect("a build of something that isn't an object or a variant", id);
                    return quote!({ #defect });
                }
                self.build(id, &fields)
            }
            Expr::List { items } => {
                let (set, map) = match self.decoded(id) {
                    Some(Decoded::List { set, map, .. }) => (*set, *map),
                    _ => (false, false),
                };
                let items: Vec<TokenStream> = items.iter().map(|item| self.expr(*item)).collect();
                let names: Vec<proc_macro2::Ident> = (0..items.len())
                    .map(|at| format_ident!("i{}", at))
                    .collect();
                quote!({
                    #(let #names = #items;)*
                    rt::list(vec![#(#names),*], #set, #map)
                })
            }
            Expr::Binary { op, lhs, rhs } => self.binary(id, op, lhs, rhs),
            Expr::Unary { op, operand } => {
                let operand = self.expr(operand);
                match op {
                    UnaryOp::Not => {
                        let holds = self.tri(quote!(rt::truth(cx, &operand, Truth::Not, #at)));
                        quote!({
                            let operand = #operand;
                            Value::Bool(!#holds)
                        })
                    }
                    UnaryOp::Negate => {
                        let negated = self.tri(quote!(rt::negate(cx, &operand, #at)));
                        quote!({
                            let operand = #operand;
                            #negated
                        })
                    }
                }
            }
            Expr::Write { conflicts, .. } => self.write(id, &conflicts),
            Expr::Stored { .. } => {
                let defect = self.defect("a stored value outside its write", id);
                quote!({ #defect })
            }
            Expr::Request {
                route,
                params,
                input,
                idempotency_key,
                caller,
            } => {
                let Some(index) = self
                    .plan
                    .routes()
                    .routes
                    .iter()
                    .position(|plan| plan.key == route.as_str())
                else {
                    let defect = self.defect("a request to a route with no plan", id);
                    return quote!({ #defect });
                };
                let parts: Vec<SiteId> = params
                    .iter()
                    .copied()
                    .chain(input)
                    .chain(idempotency_key)
                    .chain(caller)
                    .collect();
                let values: Vec<TokenStream> = parts.iter().map(|part| self.expr(*part)).collect();
                let names: Vec<proc_macro2::Ident> = (0..values.len())
                    .map(|at| format_ident!("a{}", at))
                    .collect();
                if let Some(defect) = self.waits_inline() {
                    return quote!({ #(let #names = #values;)* #defect });
                }
                let route = number(u32::try_from(index).unwrap_or(u32::MAX));
                let (keyed, signed) = (idempotency_key.is_some(), caller.is_some());
                let ran = self.stepped(
                    quote!(rt::request(cx, p, #route, vec![#(#names),*], #keyed, #signed).await),
                    id,
                    false,
                );
                let answered = self.tri(quote!(ran));
                quote!({
                    #(let #names = #values;)*
                    let ran = #ran;
                    #answered
                })
            }
            Expr::Inline { .. } => {
                let defect = self.defect("an inline function outside a collection operation", id);
                quote!({ #defect })
            }
            // A constant's value: its body, which can't fail (area 3).
            Expr::Constant { .. } => match self.decoded(id) {
                Some(Decoded::Callable(Some(target))) => self.function_call(id, *target, &[]),
                _ => {
                    let defect = self.defect("a constant that has no body", id);
                    quote!({ #defect })
                }
            },
            Expr::Config { .. } => match self.decoded(id) {
                Some(Decoded::Config(Some(setting))) => {
                    let setting = number(*setting);
                    self.tri(quote!(rt::setting(cx, p, #setting, #at)))
                }
                _ => {
                    let defect = self.defect("a setting the checker would have refused", id);
                    quote!({ #defect })
                }
            },
            Expr::Now => self.tri(quote!(rt::now(cx, p, #at))),
            Expr::Unchecked => {
                let written = match self.decoded(id) {
                    Some(Decoded::Written(written)) => written.clone(),
                    _ => String::new(),
                };
                let written = text(&written);
                let stop = self.fail(quote!(rt::not_built_yet(cx, #written, #at)));
                quote!({ #stop })
            }
            Expr::Match {
                subject,
                outcome,
                arms,
            } => {
                let Some(subject) = subject else {
                    let defect = self.defect("a `match` without a subject", id);
                    return quote!({ #defect });
                };
                if outcome {
                    self.outcome(id, subject, &arms)
                } else {
                    self.matched(id, subject, &arms)
                }
            }
            Expr::Attempt { inner, arms } => {
                let Some(inner) = inner else {
                    let defect = self.defect("an `attempt` without a call", id);
                    return quote!({ #defect });
                };
                self.attempt(id, inner, &arms)
            }
            Expr::Reject { failure, fields } => {
                if failure.is_none() {
                    let defect = self.defect("a `reject` naming no failure", id);
                    return quote!({ #defect });
                }
                self.reject(id, &fields)
            }
            // The failure the arm being run handles, passed on as it is.
            Expr::Propagate { .. } => match self.handling.last() {
                Some(handled) => {
                    let stop = self.fail(quote!(Stop::Failed(Box::new(#handled.clone()))));
                    quote!({ #stop })
                }
                None => {
                    let defect = self.defect("`propagate` outside a failure arm", id);
                    quote!({ #defect })
                }
            },
            // Every email the test sent (area 16).
            Expr::SentEmails => self.tri(quote!(rt::sent_emails(cx, p, #at))),
        }
    }

    /// An operator: both sides worked out, then the runtime's operator at
    /// the operator's site; `and` and `or` stop as soon as the answer is
    /// known.
    fn binary(&mut self, id: SiteId, op: BinaryOp, lhs: SiteId, rhs: SiteId) -> TokenStream {
        let at = number(id.0);
        let left = self.expr(lhs);
        let right = self.expr(rhs);
        if let BinaryOp::And | BinaryOp::Or = op {
            let stops = op == BinaryOp::Or;
            let first = self.tri(quote!(rt::truth(cx, &left, Truth::Logic, #at)));
            let second = self.tri(quote!(rt::truth(cx, &right, Truth::Logic, #at)));
            return quote!({
                let left = #left;
                let first = #first;
                if first == #stops {
                    Value::Bool(first)
                } else {
                    let right = #right;
                    Value::Bool(#second)
                }
            });
        }
        let Some(operator) = operator(op) else {
            // `matches`, a query's condition, which no body runs yet: both
            // sides are worked out first, as the interpreter does.
            let stop = self.fail(quote!(rt::not_built_yet(cx, "matches", #at)));
            return quote!({
                let _ = #left;
                let _ = #right;
                #stop
            });
        };
        // An amount scaled by a number rounds by its type's `rounding:`
        // (area 19).
        let rounding = match self.decoded(id) {
            Some(Decoded::Rounding(rounding)) => format_ident!("{}", format!("{rounding:?}")),
            _ => format_ident!("HalfEven"),
        };
        let applied = self.tri(quote!(rt::binary(
            cx,
            Operator::#operator,
            &left,
            &right,
            Rounding::#rounding,
            #at
        )));
        quote!({
            let left = #left;
            let right = #right;
            #applied
        })
    }

    /// `Type { … }` or `Enum.variant { … }`: each field worked out and kept
    /// as its type keeps it, in the order written, then the value built
    /// and the rules across its fields checked.
    fn build(&mut self, id: SiteId, fields: &[(jadpo_graph::Key, SiteId)]) -> TokenStream {
        let at = number(id.0);
        let (shape, given, fallible, across) = match self.decoded(id) {
            Some(Decoded::Build {
                shape,
                fields,
                fallible,
                across,
            }) => (*shape, fields.clone(), *fallible, *across),
            _ => (None, Vec::new(), false, None),
        };
        let mut lets = Vec::new();
        let mut names = Vec::new();
        for (index, (_, site)) in fields.iter().enumerate() {
            let value = self.expr(*site);
            let name = format_ident!("f{}", index);
            let Some(given) = given.get(index) else {
                let defect = self.defect("a build the plan has no fields for", id);
                lets.push(quote!(let _ = #value; #defect;));
                return quote!({ #(#lets)* });
            };
            let validator = number(given.validator.0);
            let field = text(&given.name);
            let kept =
                self.tri(quote!(rt::given(cx, p, #validator, value, #fallible, #field, #at)));
            lets.push(quote!(let #name = { let value = #value; #kept };));
            names.push(name);
        }
        let Some(shape) = shape else {
            let defect = self.defect("a build without one of its fields", id);
            return quote!({ #(#lets)* #defect });
        };
        let shape = number(shape.0);
        let slots = given.iter().map(|given| slot(given.slot));
        // The rules across its fields, checked only where one can break
        // here: one the checker proved is never checked again.
        let across = match across {
            Some((validator, true)) => {
                let validator = number(validator.0);
                quote!(Some(#validator))
            }
            _ => quote!(None),
        };
        let built = self.tri(quote!(rt::build(
            cx,
            p,
            #shape,
            &[#(#slots),*],
            vec![#(#names),*],
            #across,
            #at
        )));
        quote!({
            #(#lets)*
            #built
        })
    }

    /// A call.
    fn call(&mut self, id: SiteId, callee: &Callee, args: &[Option<SiteId>]) -> TokenStream {
        match callee {
            Callee::Function(_)
            | Callee::Action(_)
            | Callee::Fixture(_)
            | Callee::LibraryFunction(_)
            | Callee::Query(_)
            | Callee::Entity { .. } => {
                let ran = self.call_ran(id, callee, args, false);
                self.tri(ran)
            }
            Callee::Construct(_) => {
                let [Some(arg)] = args else {
                    let defect = self.defect("a construction without one value", id);
                    return quote!({ #defect });
                };
                let value = self.expr(*arg);
                let constructed = self.construct(id);
                quote!({
                    let value = #value;
                    #constructed
                })
            }
            Callee::Std(op) => self.standard(id, *op, args),
            Callee::Object(_) | Callee::Variant(_) | Callee::Unknown => {
                let defect = self.defect("a call to something that can't be called", id);
                quote!({ #defect })
            }
            // Named by what is written where that names the operation, and
            // by its kind where what is written is the project's own.
            Callee::Caller(_)
            | Callee::Session(_)
            | Callee::Batch(_)
            | Callee::AdvanceClock
            | Callee::Generate { .. }
            | Callee::Stream { .. }
            | Callee::Embed { .. } => {
                let written = self.written(id);
                self.refuse(format!("`{written}`"), id)
            }
            Callee::PackageAction { .. } => self.refuse("a package's action".into(), id),
            Callee::Files { op, .. } => self.refuse(format!("`<store>.{}`", op.name()), id),
            Callee::Provider { .. } => self.refuse("a provider's operation".into(), id),
            Callee::Fake { .. } => self.refuse("a test's `fakes:`".into(), id),
            Callee::AiFake { .. } => self.refuse("a test's `fakes: { ai: }`".into(), id),
            Callee::AiRequests { .. } => self.refuse("`ai.<prompt>.requests`".into(), id),
            Callee::Schedule { .. } => self.refuse("`<schedule>.run()`".into(), id),
            Callee::Translation(_) => self.refuse("`translations.<message>`".into(), id),
            Callee::Prompt(_) => self.refuse("`prompts.render`".into(), id),
            Callee::Escape => self.refuse("an email template's `{{ value }}`".into(), id),
        }
    }

    /// A call of a function, action, fixture or library function, or a
    /// query's or entity's read, at site `id`: a Rust expression giving
    /// what it gave as a `Ran`, its arguments worked out first, their stops
    /// going where any stop goes. `caught` when its failure is handled, by
    /// an outcome `match` or an `assert … fails`, which runs an action
    /// called inside another in a savepoint (area 6).
    fn call_ran(
        &mut self,
        id: SiteId,
        callee: &Callee,
        args: &[Option<SiteId>],
        caught: bool,
    ) -> TokenStream {
        let at = number(id.0);
        match callee {
            Callee::Function(_)
            | Callee::Action(_)
            | Callee::Fixture(_)
            | Callee::LibraryFunction(_) => {
                let Some(Decoded::Callable(Some(target))) = self.decoded(id) else {
                    let defect = self.defect("a call to a callable that has no body", id);
                    return quote!({ #defect });
                };
                let target = *target;
                let pauses = self
                    .body()
                    .sites
                    .get(id.0 as usize)
                    .is_some_and(|site| site.pauses);
                if !pauses {
                    return self.function_ran(id, target, args);
                }
                let (lets, names) = self.arguments(args, id);
                if let Some(defect) = self.waits_inline() {
                    return quote!({ #(#lets)* #defect });
                }
                let caller = self.plan.callable(target).map(|callable| callable.caller);
                let call_as = self.call_as(caller, &names);
                let target = number(target);
                let ran = self.stepped(
                    quote!(rt::invoke(cx, p, #target, vec![#(#names),*], #at, #caught).await),
                    id,
                    false,
                );
                quote!({
                    #(#lets)*
                    #call_as
                    #ran
                })
            }
            // A read the runtime runs from its plan (area 6).
            _ => {
                let (read, n, caller) = match self.decoded(id) {
                    Some(Decoded::Query(Some(query))) => (
                        quote!(rt::query),
                        *query,
                        Some(self.plan.query(*query).and_then(|plan| plan.caller)),
                    ),
                    Some(Decoded::Get(Some(entity))) => (quote!(rt::get), *entity, Some(None)),
                    _ => {
                        let written = text(&self.written(id));
                        return quote!(Err::<Value, Stop>(rt::not_built_yet(cx, #written, #at)));
                    }
                };
                let (lets, names) = self.arguments(args, id);
                if let Some(defect) = self.waits_inline() {
                    return quote!({ #(#lets)* #defect });
                }
                let call_as = self.call_as(caller, &names);
                let n = number(n);
                let ran = self.stepped(
                    quote!(#read(cx, p, #n, &[#(#names),*], #at).await),
                    id,
                    false,
                );
                quote!({
                    #(#lets)*
                    #call_as
                    #ran
                })
            }
        }
    }

    /// The subject of an outcome `match` or an `assert … fails` (areas 3
    /// and 13): a Rust expression giving what it gave as a `Ran`. A call's
    /// arguments are worked out first, their failures leaving as they
    /// would anywhere else, so only the call's own outcome is caught, and
    /// a value it gives is kept for an assertion's report; any other
    /// subject is caught whole.
    fn subject(&mut self, subject: SiteId) -> TokenStream {
        let node = self
            .body()
            .sites
            .get(subject.0 as usize)
            .map(|site| site.node.clone());
        let call = match node {
            Some(Node::Expr(Expr::Call { callee, args, .. })) => match callee {
                Callee::Function(_)
                | Callee::Action(_)
                | Callee::Fixture(_)
                | Callee::LibraryFunction(_) => Some((callee, args)),
                Callee::Query(_) | Callee::Entity { .. }
                    if matches!(
                        self.decoded(subject),
                        Some(Decoded::Query(Some(_)) | Decoded::Get(Some(_)))
                    ) =>
                {
                    Some((callee, args))
                }
                Callee::Schedule { .. } => {
                    self.refuse("`<schedule>.run()`".into(), subject);
                    return quote!(Ok::<Value, Stop>(Value::Unit));
                }
                _ => None,
            },
            _ => None,
        };
        let Some((callee, args)) = call else {
            let name = format_ident!("caught{}", subject.0);
            return self.caught(&name, subject);
        };
        let ran = self.call_ran(subject, &callee, &args, true);
        if !self.recording {
            return ran;
        }
        let at = number(subject.0);
        quote!({
            let ran: Ran = #ran;
            if let Ok(value) = &ran {
                cx.record(#at, value);
            }
            ran
        })
    }

    /// An outcome `match` at `id` on `subject`: the `success` arm with the
    /// value it gave, or the arm of the failure it gave, which that arm
    /// handles while it runs; a failure no arm names passes on.
    fn outcome(&mut self, id: SiteId, subject: SiteId, arms: &[Arm]) -> TokenStream {
        let ran = self.subject(subject);
        let success = match arms.iter().find(|arm| arm.pattern == Pattern::Success) {
            Some(arm) => {
                let bound = bind(arm.binding, quote!(value));
                let body = self.arm(arm);
                quote!(Ok(value) => { #bound #body })
            }
            None => {
                let defect = self.defect("an outcome `match` without `success`", id);
                quote!(Ok(_) => { #defect })
            }
        };
        let mut failures = Vec::new();
        for (index, arm) in arms.iter().enumerate() {
            let Pattern::Failure(key) = &arm.pattern else {
                continue;
            };
            let key = text(key.as_str());
            let handled = format_ident!("h{}_{}", id.0, index);
            let bound = bind(arm.binding, quote!(#handled.record()));
            self.handling.push(handled.clone());
            let body = self.arm(arm);
            self.handling.pop();
            failures.push(quote! {
                Err(Stop::Failed(failure)) if failure.key == #key => {
                    let #handled: jadpo_runtime::Failure = *failure;
                    #bound
                    #body
                }
            });
        }
        let passed = self.fail(quote!(stop));
        quote!({
            let ran: Ran = #ran;
            match ran {
                #success
                #(#failures)*
                Err(stop) => #passed,
            }
        })
    }

    /// A value `match` at `id` on `subject`: the first arm whose pattern
    /// the value is, with the value bound to its name.
    fn matched(&mut self, id: SiteId, subject: SiteId, arms: &[Arm]) -> TokenStream {
        let value = self.expr(subject);
        let patterns = match self.decoded(id) {
            Some(Decoded::Match { arms }) => arms.clone(),
            _ => Vec::new(),
        };
        let mut chain = {
            let defect = self.defect("a `match` whose arms miss a case", id);
            quote!({ #defect })
        };
        let mut built = Vec::new();
        for (index, arm) in arms.iter().enumerate() {
            let test = match &arm.pattern {
                Pattern::Variant(_) => match patterns.get(index) {
                    Some(ArmPattern::Variant { ty, name }) => {
                        let (ty, name) = (text(ty), text(name));
                        quote!(rt::is_variant(&subject, #ty, #name))
                    }
                    _ => quote!(false),
                },
                Pattern::True => quote!(subject == Value::Bool(true)),
                Pattern::False => quote!(subject == Value::Bool(false)),
                Pattern::Some => quote!(subject != Value::None),
                Pattern::None => quote!(subject == Value::None),
                Pattern::Wildcard => quote!(true),
                Pattern::Literal(_) => match patterns.get(index) {
                    Some(ArmPattern::Literal(Some(constant))) => {
                        let constant = number(constant.0);
                        quote!(jadpo_value::ops::equal(&subject, &p.constant(#constant)))
                    }
                    _ => {
                        let defect = self.defect("a literal the checker would have refused", id);
                        quote!({ #defect })
                    }
                },
                Pattern::Success | Pattern::Failure(_) | Pattern::Unknown => {
                    let defect = self.defect("an outcome pattern on a value", id);
                    quote!({ #defect })
                }
            };
            let bound = bind(arm.binding, quote!(subject.clone()));
            let body = self.arm(arm);
            built.push((test, quote!({ #bound #body })));
        }
        for (test, body) in built.into_iter().rev() {
            chain = quote!(if #test #body else #chain);
        }
        quote!({
            let subject = #value;
            #chain
        })
    }

    /// An arm's body, which gives the `match`'s value: its value, or a
    /// block's statements and nothing.
    fn arm(&mut self, arm: &Arm) -> TokenStream {
        match &arm.body {
            ArmBody::Value(value) => self.expr(*value),
            ArmBody::Block(block) => {
                let block = self.block(block);
                quote!({ #block Value::Unit })
            }
            ArmBody::Empty => quote!(Value::Unit),
        }
    }

    /// `attempt inner` at `id`: a rule its construction breaks raised as
    /// the failure its arm maps it to, at the construction's site, with the
    /// arm's fields worked out only then; any other stop passes on.
    fn attempt(&mut self, id: SiteId, inner: SiteId, arms: &[RuleArm]) -> TokenStream {
        let at = number(id.0);
        let name = format_ident!("attempt{}", id.0);
        let ran = self.caught(&name, inner);
        let raises = match self.decoded(id) {
            Some(Decoded::Attempt { arms }) => arms.clone(),
            _ => Vec::new(),
        };
        let keys = arms.iter().map(|arm| {
            let rule = match arm.rule {
                Some(rule) => {
                    let rule = format_ident!("{}", format!("{rule:?}"));
                    quote!(Some(jadpo_std::RuleKey::#rule))
                }
                None => quote!(None),
            };
            let named = match &arm.named {
                Some(named) => {
                    let named = text(named);
                    quote!(Some(#named))
                }
                None => quote!(None),
            };
            quote!((#rule, #named))
        });
        let keys: Vec<TokenStream> = keys.collect();
        let mut cases = Vec::new();
        for (index, arm) in arms.iter().enumerate() {
            let mut lets = Vec::new();
            let mut fields = Vec::new();
            for (place, (_, site)) in arm.fields.iter().enumerate() {
                let value = self.expr(*site);
                let name = format_ident!("g{}", place);
                lets.push(quote!(let #name = #value;));
                fields.push(name);
            }
            let raised = match raises.get(index).and_then(Option::as_ref) {
                Some(raise) => {
                    let failure = number(raise.failure.0);
                    let slots = raise.slots.iter().map(|slot| self::slot(*slot));
                    let problem = slot(raise.problem);
                    let made = self.tri(quote!(rt::mapped(
                        cx,
                        p,
                        #failure,
                        &[#(#slots),*],
                        vec![#(#fields),*],
                        #problem,
                        problem,
                        #at
                    )));
                    let inner = number(inner.0);
                    let stop = self.fail(quote!(rt::broke(cx, failure, written, #inner)));
                    quote!(let failure = #made; #stop)
                }
                None => self.defect("an arm naming something that isn't a failure", id),
            };
            let index = number(u32::try_from(index).unwrap_or(u32::MAX));
            cases.push(quote!(Some(#index) => { #(#lets)* #raised }));
        }
        let unmapped = self.defect("a construction that can fail, with no failure for it", id);
        let passed = self.fail(quote!(stop));
        quote!({
            let ran: Ran = #ran;
            match ran {
                Ok(value) => value,
                Err(Stop::Invalid { rule, written, problem, name }) => {
                    match rt::rule_arm(&[#(#keys),*], rule, name.as_deref()) {
                        #(#cases)*
                        _ => #unmapped,
                    }
                }
                Err(stop) => #passed,
            }
        })
    }

    /// `reject Failure { … }` at `id`: its fields worked out in the order
    /// written, then the failure raised there, with the failure the arm
    /// being run handles, if any, as its cause (area 4).
    fn reject(&mut self, id: SiteId, fields: &[(jadpo_graph::Key, SiteId)]) -> TokenStream {
        let at = number(id.0);
        let mut lets = Vec::new();
        let mut names = Vec::new();
        for (place, (_, site)) in fields.iter().enumerate() {
            let value = self.expr(*site);
            let name = format_ident!("g{}", place);
            lets.push(quote!(let #name = #value;));
            names.push(name);
        }
        let Some(Decoded::Reject {
            failure: Some(failure),
            slots,
        }) = self.decoded(id).cloned()
        else {
            let defect = self.defect("a `reject` of something that isn't a failure", id);
            return quote!({ #(#lets)* #defect });
        };
        let failure = number(failure.0);
        let slots = slots.iter().map(|at| slot(*at));
        let made = self.tri(quote!(rt::failure(
            cx,
            p,
            #failure,
            &[#(#slots),*],
            vec![#(#names),*],
            "a `reject` of something that isn't a failure",
            #at
        )));
        let cause = match self.handling.last() {
            Some(handled) => quote!(Some(Box::new(#handled.clone()))),
            None => quote!(None),
        };
        let stop = self.fail(quote!(rt::raised(cx, failure, #cause, #at)));
        quote!({
            #(#lets)*
            let failure = #made;
            #stop
        })
    }

    /// `Type(value)` at `id`, or what `text.parse` read, `value`, built as
    /// its type: checked against its rules only where the checker couldn't
    /// prove them.
    fn construct(&mut self, id: SiteId) -> TokenStream {
        let at = number(id.0);
        let Some(Decoded::Construct {
            validator,
            fallible,
            set,
        }) = self.decoded(id).cloned()
        else {
            let defect = self.defect("a construction the plan has no rules for", id);
            return quote!({ #defect });
        };
        let validator = number(validator.0);
        self.tri(quote!(rt::construct(cx, p, #validator, #fallible, #set, value, #at)))
    }

    /// A standard-library operation at `id`: its arguments worked out in
    /// its parameters' order (a key by its setting's number, an option
    /// left out as `none`), then the runtime's one entry point for the
    /// standard library, an inline function a closure it calls per item.
    fn standard(&mut self, id: SiteId, op: jadpo_std::Op, args: &[Option<SiteId>]) -> TokenStream {
        let at = number(id.0);
        let mut lets = Vec::new();
        let mut names = Vec::new();
        let mut inline: Option<(Vec<LocalId>, SiteId)> = None;
        let mut stopped = None;
        for (param, arg) in op.operation().params.iter().zip(args) {
            let name = format_ident!("a{}", names.len());
            let Some(arg) = arg else {
                if param.optional {
                    lets.push(quote!(let #name = Value::None;));
                    names.push(name);
                    continue;
                }
                stopped = Some(self.defect("an operation without one of its values", id));
                break;
            };
            let node = self
                .body()
                .sites
                .get(arg.0 as usize)
                .map(|site| site.node.clone());
            // A key is given by its setting, never read as a value.
            if matches!(param.kind, ParamKind::Builtin(builtin) if builtin.is_key()) {
                match (node, self.decoded(*arg)) {
                    (
                        Some(Node::Expr(Expr::Config { .. })),
                        Some(Decoded::Config(Some(setting))),
                    ) => {
                        let setting = Literal::i64_unsuffixed(i64::from(*setting));
                        lets.push(quote!(let #name = Value::Int(#setting);));
                        names.push(name);
                    }
                    _ => {
                        stopped = Some(self.defect("a key that isn't read from its setting", id));
                        break;
                    }
                }
                continue;
            }
            if param.kind == ParamKind::Function {
                match node {
                    Some(Node::Expr(Expr::Inline {
                        params,
                        body: Some(body),
                    })) if !params.is_empty() => inline = Some((params, body)),
                    _ => {
                        stopped =
                            Some(self.defect("an operation's function that isn't inline", id));
                        break;
                    }
                }
                continue;
            }
            let value = self.expr(*arg);
            lets.push(quote!(let #name = #value;));
            names.push(name);
        }
        if let Some(stopped) = stopped {
            return quote!({ #(#lets)* #stopped });
        }
        let path = format_ident!("{}", format!("{op:?}"));
        let ran = match inline {
            Some((params, body)) => {
                let each = self.inline(&params, body);
                quote!(rt::standard_each(cx, p, jadpo_std::Op::#path, &[#(#names),*], &mut #each, #at))
            }
            None => {
                let json = match self.decoded(id) {
                    Some(Decoded::Json(number)) => {
                        let number = self::number(*number);
                        quote!(Some(#number))
                    }
                    _ => quote!(None),
                };
                quote!(rt::standard(cx, p, jadpo_std::Op::#path, #json, &[#(#names),*], #at))
            }
        };
        let value = self.tri(ran);
        // The value `text.parse` reads is then built as the type `as:`
        // names, checking its rules (area 19).
        if op == jadpo_std::Op::TextParse {
            let constructed = self.construct(id);
            return quote!({
                #(#lets)*
                let value = #value;
                #constructed
            });
        }
        quote!({
            #(#lets)*
            #value
        })
    }

    /// An inline function with parameters `params` and body `body`: a
    /// closure the runtime calls with each item's values, which sets its
    /// parameters, locals of the enclosing callable, and works the body
    /// out. Its stops end it, and the operation running it; an assertion
    /// keeps none of its values, which are per item.
    fn inline(&mut self, params: &[LocalId], body: SiteId) -> TokenStream {
        let outer = (
            self.label.take(),
            std::mem::replace(&mut self.recording, false),
            self.inline.replace(body),
        );
        let value = self.expr(body);
        (self.label, self.recording, self.inline) = outer;
        let set = params.iter().enumerate().map(|(place, param)| {
            let local = format_ident!("l{}", param.0);
            let place = number(u32::try_from(place).unwrap_or(u32::MAX));
            quote!(if let Some(value) = given.get(#place) { #local = value.clone(); })
        });
        quote!(|cx: &mut Ctx<'_>, given: &[Value]| -> Ran {
            #(#set)*
            Ok(#value)
        })
    }

    /// A call's arguments worked out in order, each into a variable: what
    /// declares them and their names.
    fn arguments(
        &mut self,
        args: &[Option<SiteId>],
        at: SiteId,
    ) -> (Vec<TokenStream>, Vec<proc_macro2::Ident>) {
        let mut lets = Vec::new();
        let mut names = Vec::new();
        for (index, arg) in args.iter().enumerate() {
            let value = self.needed(*arg, at);
            let name = format_ident!("a{}", index);
            lets.push(quote!(let #name = #value;));
            names.push(name);
        }
        (lets, names)
    }

    /// A call of function `target` from site `id`, which never waits: its
    /// arguments, then its frame entered, its body run and its frame left.
    fn function_call(&mut self, id: SiteId, target: u32, args: &[Option<SiteId>]) -> TokenStream {
        let ran = self.function_ran(id, target, args);
        self.tri(ran)
    }

    /// [`Emitter::function_call`], giving what the function gave as a
    /// `Ran`.
    fn function_ran(&mut self, id: SiteId, target: u32, args: &[Option<SiteId>]) -> TokenStream {
        let at = number(id.0);
        let Some(callee) = self.plan.callable(target) else {
            let defect = self.defect("a call to a callable that has no body", id);
            return quote!({ #defect });
        };
        if Shape::of(callee.kind).waits() {
            let written = self.written(id);
            self.refuse(
                format!("`{written}`, a call that waits, where nothing waits"),
                id,
            );
            return quote!(Ok::<Value, Stop>(Value::Unit));
        }
        let params = callee.body.params.len();
        let (lets, names) = self.arguments(args, id);
        let given: Vec<TokenStream> = (0..params)
            .map(|place| match names.get(place) {
                Some(name) => quote!(#name),
                None => quote!(Value::Unit),
            })
            .collect();
        let path = self.paths.get(target as usize).cloned().unwrap_or_default();
        let target = number(target);
        let frame = self.tri(quote!(rt::enter(cx, p, #target, #at)));
        quote!({
            #(#lets)*
            #frame;
            let ran = #path(cx, p, #(#given),*);
            rt::left(cx, ran)
        })
    }

    /// `create`, `update` or `delete`: its inputs worked out in the order
    /// its plan lists them, each kept as its field's type keeps it, then
    /// the store's entry point; a refusal its `conflict:` maps raises that
    /// arm's failure, its fields worked out only then.
    fn write(&mut self, id: SiteId, conflicts: &[jadpo_graph::body::ConflictArm]) -> TokenStream {
        let at = number(id.0);
        let (plan, arms) = match self.decoded(id) {
            Some(Decoded::Write {
                plan: Some(plan),
                arms,
            }) => (*plan, arms.clone()),
            _ => {
                let defect = self.defect("a write with no plan", id);
                return quote!({ #defect });
            }
        };
        let Some(write) = self.plan.write(plan) else {
            let defect = self.defect("a write with no plan", id);
            return quote!({ #defect });
        };
        let inputs = write.inputs.clone();
        let mut lets = Vec::new();
        let mut names = Vec::new();
        for (index, (site, validator)) in inputs.iter().enumerate() {
            let value = self.expr(*site);
            let name = format_ident!("w{}", index);
            let kept = match validator {
                Some(validator) => {
                    let (validator, site) = (number(validator.0), number(site.0));
                    let kept = self.tri(quote!(rt::kept(cx, p, #validator, value, #site)));
                    quote!({ let value = #value; #kept })
                }
                None => value,
            };
            lets.push(quote!(let #name = #kept;));
            names.push(name);
        }
        let mut cases = Vec::new();
        for (index, conflict) in conflicts.iter().enumerate() {
            let mut values = Vec::new();
            let mut fields = Vec::new();
            for (place, (_, site)) in conflict.fields.iter().enumerate() {
                let value = self.expr(*site);
                let name = format_ident!("g{}", place);
                values.push(quote!(let #name = #value;));
                fields.push(name);
            }
            let raised = match arms.get(index).and_then(Option::as_ref) {
                Some(raise) => {
                    let failure = number(raise.failure.0);
                    let slots = raise.slots.iter().map(|at| slot(*at));
                    let made = self.tri(quote!(rt::failure(
                        cx,
                        p,
                        #failure,
                        &[#(#slots),*],
                        vec![#(#fields),*],
                        "a conflict arm naming something that isn't a failure",
                        #at
                    )));
                    let stop = self.fail(quote!(rt::raised(cx, failure, Some(cause), #at)));
                    quote!(let failure = #made; #stop)
                }
                None => self.defect("a conflict arm naming something that isn't a failure", id),
            };
            let index = number(u32::try_from(index).unwrap_or(u32::MAX));
            cases.push(quote!(#index => { #(#values)* #raised }));
        }
        if let Some(defect) = self.waits_inline() {
            return quote!({ #(#lets)* #defect });
        }
        let plan = number(plan);
        let ran = self.stepped(
            quote!(rt::write(cx, p, #plan, &[#(#names),*], #at).await),
            id,
            false,
        );
        let unarmed = self.defect("a refusal with no arm for it", id);
        let written = self.tri(quote!(ran));
        quote!({
            #(#lets)*
            let ran = #ran;
            match ran {
                Err(Stop::Refused { arm, cause }) => match arm {
                    #(#cases)*
                    _ => { #unarmed }
                },
                ran => #written,
            }
        })
    }
}

/// `local = value;` for an arm that binds its value to `binding`.
fn bind(binding: Option<LocalId>, value: TokenStream) -> TokenStream {
    match binding {
        Some(local) => {
            let local = format_ident!("l{}", local.0);
            quote!(#local = #value;)
        }
        None => TokenStream::new(),
    }
}

/// `Some(n)` or `None`, as written in generated code.
fn slot(at: Option<u32>) -> TokenStream {
    match at {
        Some(at) => {
            let at = number(at);
            quote!(Some(#at))
        }
        None => quote!(None),
    }
}

/// The runtime's operator for `op`; `None` for `and`, `or` and `matches`.
fn operator(op: BinaryOp) -> Option<proc_macro2::Ident> {
    let name = match op {
        BinaryOp::Equal => "Equal",
        BinaryOp::NotEqual => "NotEqual",
        BinaryOp::Less => "Less",
        BinaryOp::LessEqual => "LessEqual",
        BinaryOp::Greater => "Greater",
        BinaryOp::GreaterEqual => "GreaterEqual",
        BinaryOp::In => "In",
        BinaryOp::Add => "Add",
        BinaryOp::Subtract => "Subtract",
        BinaryOp::Multiply => "Multiply",
        BinaryOp::Divide => "Divide",
        BinaryOp::Remainder => "Remainder",
        BinaryOp::And | BinaryOp::Or | BinaryOp::Matches => return None,
    };
    Some(format_ident!("{}", name))
}

/// The operator of an `assert` whose condition is a comparison, whose two
/// sides its report shows.
fn comparison(op: BinaryOp) -> Option<proc_macro2::Ident> {
    match op {
        BinaryOp::Equal
        | BinaryOp::NotEqual
        | BinaryOp::Less
        | BinaryOp::LessEqual
        | BinaryOp::Greater
        | BinaryOp::GreaterEqual => operator(op),
        _ => None,
    }
}
