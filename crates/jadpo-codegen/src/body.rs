//! One callable's body as Rust (docs/compiler-runtime.md §2.5, "Compiled
//! code"): its statements and expressions in the order the interpreter's
//! walker runs them, each site that can stop calling the runtime with the
//! site's own number, so a fault, a failure or a failed assertion names the
//! same frames and a record the same sites whichever executor ran it.
//!
//! Only control flow is the generated code's own. A local is a `Value`
//! variable; an expression that can stop is `?` on the runtime's `Ran`, or,
//! inside an `assert`'s condition, a `break` out of the labelled block
//! that ends the assertion; a call into an action goes through the
//! runtime's boundary by number. A form not compiled yet is refused by
//! name ([`Refusal`]) and gives no code.

use jadpo_graph::Kind;
use jadpo_graph::body::{BinaryOp, Body, Callee, Expr, IfArm, Node, SiteId, Stmt, UnaryOp};
use jadpo_plan::{CallablePlan, Decoded, Plan};
use proc_macro2::{Literal, TokenStream};
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
    /// Inside an `assert`'s condition: the label a stop breaks out to, the
    /// assertion recording each value worked out.
    pub assertion: Option<TokenStream>,
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
    /// body, or the assertion being worked out.
    fn tri(&self, ran: TokenStream) -> TokenStream {
        match &self.assertion {
            None => quote!(#ran?),
            Some(label) => quote!(match #ran {
                Ok(value) => value,
                Err(stop) => break #label Err(stop),
            }),
        }
    }

    /// Stops with `stop`, a `Stop`.
    fn fail(&self, stop: TokenStream) -> TokenStream {
        match &self.assertion {
            None => quote!(return Err(#stop)),
            Some(label) => quote!(break #label Err(#stop)),
        }
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
            Stmt::Assert { value, fails: None } => self.assert(id, value),
            Stmt::Assert { fails: Some(_), .. } => {
                self.refuse("`assert … fails`".into(), id);
                TokenStream::new()
            }
            Stmt::For { .. } => {
                self.refuse("`for`".into(), id);
                TokenStream::new()
            }
            Stmt::Emit { .. } => {
                self.refuse("`emit`".into(), id);
                TokenStream::new()
            }
        }
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

    /// `assert condition`: its parts recorded while the condition is worked
    /// out, in a block a stop breaks out of, so the assertion's recording
    /// ends however it went.
    fn assert(&mut self, id: SiteId, value: Option<SiteId>) -> TokenStream {
        let Some(value) = value else {
            let defect = self.defect("an `assert` without a condition", id);
            return quote!(#defect;);
        };
        let label = {
            let label = proc_macro2::Punct::new('\'', proc_macro2::Spacing::Joint);
            let name = format_ident!("assert{}", id.0);
            quote!(#label #name)
        };
        let outer = self.assertion.replace(label.clone());
        let at = number(id.0);
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
        let held = match compared {
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
        };
        self.assertion = outer;
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
        if self.assertion.is_some() && !literal {
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
            Expr::Match { .. } => self.refuse("`match`".into(), id),
            Expr::Attempt { .. } => self.refuse("`attempt`".into(), id),
            Expr::Reject { .. } => self.refuse("`reject`".into(), id),
            Expr::Propagate { .. } => self.refuse("`propagate`".into(), id),
            Expr::SentEmails => self.refuse("`email.sent`".into(), id),
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
                    return self.function_call(id, target, args);
                }
                let (lets, names) = self.arguments(args, id);
                let caller = self.plan.callable(target).map(|callable| callable.caller);
                let call_as = self.call_as(caller, &names);
                let target = number(target);
                let ran = self.stepped(
                    quote!(rt::invoke(cx, p, #target, vec![#(#names),*], #at, false).await),
                    id,
                    false,
                );
                let called = self.tri(quote!(ran));
                quote!({
                    #(#lets)*
                    #call_as
                    let ran = #ran;
                    #called
                })
            }
            // A read the runtime runs from its plan (area 6).
            Callee::Query(_) | Callee::Entity { .. } => {
                let (read, n, caller) = match self.decoded(id) {
                    Some(Decoded::Query(Some(query))) => (
                        quote!(rt::query),
                        *query,
                        Some(self.plan.query(*query).and_then(|plan| plan.caller)),
                    ),
                    Some(Decoded::Get(Some(entity))) => (quote!(rt::get), *entity, Some(None)),
                    _ => {
                        let written = text(&self.written(id));
                        let stop = self.fail(quote!(rt::not_built_yet(cx, #written, #at)));
                        return quote!({ #stop });
                    }
                };
                let (lets, names) = self.arguments(args, id);
                let call_as = self.call_as(caller, &names);
                let n = number(n);
                let ran = self.stepped(
                    quote!(#read(cx, p, #n, &[#(#names),*], #at).await),
                    id,
                    false,
                );
                let read = self.tri(quote!(ran));
                quote!({
                    #(#lets)*
                    #call_as
                    let ran = #ran;
                    #read
                })
            }
            Callee::Construct(_) => self.refuse("`Type(value)`".into(), id),
            Callee::Object(_) | Callee::Variant(_) | Callee::Unknown => {
                let defect = self.defect("a call to something that can't be called", id);
                quote!({ #defect })
            }
            // Named by what is written where that names the operation, and
            // by its kind where what is written is the project's own.
            Callee::Std(_)
            | Callee::Caller(_)
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
        let at = number(id.0);
        let Some(callee) = self.plan.callable(target) else {
            let defect = self.defect("a call to a callable that has no body", id);
            return quote!({ #defect });
        };
        if Shape::of(callee.kind).waits() {
            let written = self.written(id);
            return self.refuse(
                format!("`{written}`, a call that waits, where nothing waits"),
                id,
            );
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
        let left = self.tri(quote!(rt::left(cx, ran)));
        quote!({
            #(#lets)*
            #frame;
            let ran = #path(cx, p, #(#given),*);
            #left
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
