//! A provider package's operation called through its connection (area
//! 11, "Calling"; chunk 7.2c): `couriers.bookings.create(request)` in an
//! action of a domain the connection's `domains:` lists, given the
//! operation's input and giving its output, its package failures mapped
//! at the call by `attempt` or `match` as a library function's are. It
//! leaves the process, so only an action calls it, before its first read
//! or write (area 6, "Outside calls hold no transaction"), which the
//! runtime holds it to as it runs.

use jadpo_db::Ownership;
use jadpo_diag::{Diagnostic, nearest, rule};
use jadpo_syntax::ast::{self, AstNode, Expr};

use jadpo_diag::Span;

use super::super::ty::{Prim, Ty};
use super::facts::Reach;
use super::failing::Arm;
use super::{Body, CallKind, Context};

/// The protocols and credential schemes the package driver sends with
/// (chunk 7.2c): JSON over HTTP, with a bearer token or basic credentials,
/// and AWS's JSON protocol signed with Signature Version 4 (chunk 7.7c).
pub(crate) const BUILT_PROTOCOLS: [&str; 2] = ["json_http", "aws_json"];
pub(crate) const BUILT_CREDENTIALS: [&str; 4] = ["bearer", "basic", "aws_sigv4", "header"];

impl Body<'_> {
    /// `connection.resource.operation(input)` through the package
    /// connection `connection`.
    pub(super) fn provider_call(
        &mut self,
        connection: &crate::connections::PackageConnection,
        rest: &[String],
        call: &ast::CallExpr,
        callee: &Expr,
        written: &str,
    ) -> (Ty, CallKind, Reach) {
        let failed = |body: &mut Self| {
            body.args_only(call);
            (
                Ty::Error,
                CallKind::Effect(written.to_string()),
                Reach::Unknown,
            )
        };
        // Reported where it is declared: the call reaches it, unchecked.
        let reported = |body: &mut Self| {
            body.args_only(call);
            (
                Ty::Error,
                CallKind::Effect(written.to_string()),
                Reach::Provider {
                    connection: connection.name.clone(),
                    operation: rest.join("."),
                    fails: Vec::new(),
                    checked: false,
                    safe: true,
                    own: false,
                    action: None,
                },
            )
        };
        let span = self.span(callee);
        if self.context.is_pure() {
            let who = super::capital(self.context.with_a());
            self.out.push(
                Diagnostic::new(
                    &rule::EFFECT_FUNCTION_PERSISTENCE,
                    format!("{who} can't call `{written}`"),
                )
                .at(span, "a provider's operation")
                .reason(format!(
                    "{who} is pure, and `{written}` calls a provider's API, outside the program; only an action calls a provider."
                ))
                .next(format!("Call `{written}` in an action, and pass in what it gives")),
            );
            return failed(self);
        }
        if self.context == Context::Test {
            self.out.push(
                Diagnostic::new(
                    &rule::SEMANTIC_NOT_CALLABLE,
                    format!("A test can't call `{written}`"),
                )
                .at(span, "a provider's operation")
                .reason(format!(
                    "`{written}` calls a provider's API; a test reaches a provider through the application's actions, with the provider faked (area 13)."
                ))
                .next("Call an action that makes this call"),
            );
            return failed(self);
        }
        // A package's action, `connection.action(input)` (chunk 7.6).
        if let [name] = rest
            && let Some(action) =
                crate::package_code::action(self.db, self.project, &connection.folder, name)
        {
            let action = action.clone();
            return self.package_action_call(connection, &action, call, callee, written);
        }
        let operations = crate::providers::operations(self.db, self.project);
        let found = match rest {
            [resource, name] => operations.find(&connection.folder, resource, name),
            _ => None,
        };
        let Some(operation) = found.cloned() else {
            // An operation that doesn't read is reported where it is
            // declared.
            if crate::providers::has_problems(self.db, self.project, &connection.folder) {
                return reported(self);
            }
            let mut known: Vec<String> = operations
                .of(&connection.folder)
                .filter(|operation| operation.public)
                .map(crate::providers::Operation::full_name)
                .collect();
            let actions: Vec<String> = crate::package_code::actions(self.db, self.project)
                .iter()
                .filter(|action| action.folder == connection.folder && action.public)
                .map(|action| action.name.clone())
                .collect();
            let what = if actions.is_empty() {
                "operation"
            } else {
                "operation or action"
            };
            known.extend(actions.iter().cloned());
            let asked = rest.join(".");
            let near = nearest(&asked, known.iter().map(String::as_str));
            let mut problem = Diagnostic::new(
                &rule::SEMANTIC_UNKNOWN_CALLEE,
                format!(
                    "`{}` has no {what} `{asked}`",
                    connection.name
                ),
            )
            .at(span.clone(), format!("no such {what}"))
            .reason(format!(
                "A connection to `{}` calls its package's public operations as `{}.<resource>.<operation>(...)`{}{}.",
                connection.package,
                connection.name,
                if actions.is_empty() {
                    String::new()
                } else {
                    format!(" and its public actions as `{}.<action>(...)`", connection.name)
                },
                if known.is_empty() {
                    ", and it has none".to_string()
                } else {
                    format!(
                        ": {}",
                        crate::and_list(
                            &known
                                .iter()
                                .map(|name| format!("`{name}`"))
                                .collect::<Vec<_>>()
                        )
                    )
                }
            ));
            problem = match near {
                Some(near) => problem
                    .next(format!("Call `{}.{near}` instead", connection.name))
                    .edit(span, format!("{}.{near}", connection.name)),
                None => problem.next(format!(
                    "Call one of `{}`'s operations, or add `{asked}` to the package's `resources/`",
                    connection.package
                )),
            };
            self.out.push(problem);
            return failed(self);
        };
        if !operation.public {
            self.out.push(
                Diagnostic::new(
                    &rule::PACKAGE_NOT_PUBLIC,
                    format!(
                        "`{}`'s resource `{}` isn't public",
                        connection.package, operation.resource
                    ),
                )
                .at(span, "not part of the package's API")
                .reason(
                    "A connection calls a package's public resources; one without `public` is reached only by the package's own actions, subscribers and polls, and may change between versions."
                )
                .next(format!(
                    "Mark `resource {}` `public` in the package, or call what it offers instead",
                    operation.resource
                )),
            );
            return failed(self);
        }
        if !self.may_reach(connection, span, written) {
            return failed(self);
        }
        if !self.operation_input(&operation, call, written) {
            return failed(self);
        }
        let result = match &operation.output {
            Ty::Prim(Prim::Unit) => Ty::Prim(Prim::Unit),
            other => other.clone(),
        };
        (
            result,
            CallKind::Effect(written.to_string()),
            Reach::Provider {
                connection: connection.name.clone(),
                operation: operation.full_name(),
                fails: operation.failures.clone(),
                checked: true,
                safe: operation.safe_to_repeat(),
                own: false,
                action: None,
            },
        )
    }

    /// Whether `call` gives `operation` its input: one value of the
    /// package's type, without a name, or nothing when it takes none. The
    /// value is checked against it.
    fn operation_input(
        &mut self,
        operation: &crate::providers::Operation,
        call: &ast::CallExpr,
        written: &str,
    ) -> bool {
        let args = Self::arguments(call);
        let takes = operation.input.is_some() as usize;
        if args.len() != takes || args.iter().any(|arg| arg.name().is_some()) {
            let at = self.arg_list_span(call);
            let what = match &operation.input {
                Some((_, input)) => format!("one `{input}`"),
                None => "nothing".to_string(),
            };
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_ARGUMENT_COUNT,
                    format!("`{written}` takes {what}"),
                )
                .at(at, format!("{} given", args.len()))
                .reason(format!(
                    "A provider's operation takes its input as one value of the package's type, without a name{}.",
                    if takes == 0 { ", and this one takes none" } else { "" }
                ))
                .next(match &operation.input {
                    Some((_, input)) => format!("Build a `{input} {{ ... }}` and pass it: `{written}(request)`"),
                    None => format!("Write `{written}()`"),
                }),
            );
            self.args_only(call);
            return false;
        }
        if let (Some((ty, _)), Some(value)) =
            (&operation.input, args.first().and_then(ast::Arg::expr))
        {
            self.check(&value, ty);
        }
        true
    }

    /// `resource.operation(input)` in a provider package's own code (area
    /// 11, "What package code may do"; chunk 7.6): its operation, public
    /// or not, called by a package action through whichever connection
    /// the app called that action through. Its failures are the
    /// package's own, so they pass on to the action's `fails` as a
    /// library function's do.
    pub(super) fn own_operation_call(
        &mut self,
        folder: &str,
        resource: &str,
        rest: &[String],
        call: &ast::CallExpr,
        callee: &Expr,
        written: &str,
    ) -> (Ty, CallKind, Reach) {
        let failed = |body: &mut Self| {
            body.args_only(call);
            (
                Ty::Error,
                CallKind::Effect(written.to_string()),
                Reach::Unknown,
            )
        };
        let span = self.span(callee);
        if self.context.is_pure() || self.context == Context::Test {
            let who = super::capital(self.context.with_a());
            self.out.push(
                Diagnostic::new(
                    &rule::EFFECT_FUNCTION_PERSISTENCE,
                    format!("{who} can't call `{written}`"),
                )
                .at(span, "the provider's operation")
                .reason(format!(
                    "`{written}` calls the provider's API, outside the program; in a package only an action calls it, and a test reaches it through the app's actions with the provider faked."
                ))
                .next(format!("Call `{written}` in one of the package's actions")),
            );
            return failed(self);
        }
        let operations = crate::providers::operations(self.db, self.project);
        let found = match rest {
            [name] => operations.find(folder, resource, name),
            _ => None,
        };
        let Some(operation) = found.cloned() else {
            if crate::providers::has_problems(self.db, self.project, folder) {
                self.args_only(call);
                return (
                    Ty::Error,
                    CallKind::Effect(written.to_string()),
                    Reach::Provider {
                        connection: String::new(),
                        operation: format!("{resource}.{}", rest.join(".")),
                        fails: Vec::new(),
                        checked: false,
                        safe: true,
                        own: true,
                        action: None,
                    },
                );
            }
            let known: Vec<String> = operations
                .of(folder)
                .filter(|operation| operation.resource == resource)
                .map(|operation| operation.name.clone())
                .collect();
            let asked = rest.join(".");
            let near = nearest(&asked, known.iter().map(String::as_str));
            let mut problem = Diagnostic::new(
                &rule::SEMANTIC_UNKNOWN_CALLEE,
                format!("Resource `{resource}` has no operation `{asked}`"),
            )
            .at(span.clone(), "no such operation")
            .reason(format!(
                "A package's action calls its own operations as `<resource>.<operation>(...)`; `{resource}` has {}.",
                if known.is_empty() {
                    "none".to_string()
                } else {
                    crate::and_list(&known.iter().map(|name| format!("`{name}`")).collect::<Vec<_>>())
                }
            ));
            problem = match near {
                Some(near) => problem
                    .next(format!("Call `{resource}.{near}` instead"))
                    .edit(span, format!("{resource}.{near}")),
                None => problem.next(format!("Add `{asked}` to resource `{resource}`")),
            };
            self.out.push(problem);
            return failed(self);
        };
        if !self.operation_input(&operation, call, written) {
            return (
                Ty::Error,
                CallKind::Effect(written.to_string()),
                Reach::Unknown,
            );
        }
        (
            operation.output.clone(),
            CallKind::Effect(written.to_string()),
            Reach::Provider {
                connection: String::new(),
                operation: operation.full_name(),
                fails: operation.failures.clone(),
                checked: true,
                safe: operation.safe_to_repeat(),
                own: true,
                action: None,
            },
        )
    }

    /// `connection.action(...)`: a provider package's action called
    /// through its connection (area 11, "What package code may do"; chunk
    /// 7.6), only a `public` one, from an action of a listed domain. Its
    /// package failures are mapped at the call as an operation's are.
    fn package_action_call(
        &mut self,
        connection: &crate::connections::PackageConnection,
        action: &crate::package_code::PackageAction,
        call: &ast::CallExpr,
        callee: &Expr,
        written: &str,
    ) -> (Ty, CallKind, Reach) {
        let failed = |body: &mut Self| {
            body.args_only(call);
            (
                Ty::Error,
                CallKind::Effect(written.to_string()),
                Reach::Unknown,
            )
        };
        let span = self.span(callee);
        if !action.public {
            self.out.push(
                Diagnostic::new(
                    &rule::PACKAGE_NOT_PUBLIC,
                    format!(
                        "`{}`'s action `{}` isn't public",
                        connection.package, action.name
                    ),
                )
                .at(span, "not part of the package's API")
                .reason(
                    "A connection calls a package's public actions; one without `public` is run only by the package's own subscribers and polls, and may change between versions."
                )
                .next(format!(
                    "Mark `action {}` `public` in the package, or call what it offers instead",
                    action.name
                )),
            );
            return failed(self);
        }
        if !self.may_reach(connection, span, written) {
            return failed(self);
        }
        let Some(sig) = self.calls.signature(&action.canonical).cloned() else {
            return failed(self);
        };
        self.args(&sig, call, written);
        (
            sig.result.clone(),
            CallKind::Effect(written.to_string()),
            Reach::Provider {
                connection: connection.name.clone(),
                operation: action.name.clone(),
                fails: sig.fails.clone(),
                checked: true,
                // Its calls are several, or may be: one that gets no answer
                // in time may have acted.
                safe: false,
                own: false,
                action: Some(action.canonical.clone()),
            },
        )
    }

    /// `emit` in a package's action: never in a `public` one, which the
    /// app calls, since a package emits only about the outside world, and
    /// never one of the events its webhooks bring, which only its provider
    /// sends; one without `public` is run by the package's own
    /// subscribers (chunk 7.6b). Whether the `emit` is checked on as any
    /// action's is.
    pub(super) fn package_emits(&mut self, folder: &str, emit: &ast::EmitStmt) -> bool {
        let action = self.action.clone().unwrap_or_default();
        let bare = action.rsplit('.').next().unwrap_or_default().to_string();
        let public = crate::package_code::action(self.db, self.project, folder, &bare)
            .is_some_and(|found| found.public);
        let event = crate::package_code::emitted_name(emit).unwrap_or_default();
        let span = self.span(emit);
        if public {
            self.out.push(
                Diagnostic::new(
                    &rule::PACKAGE_ACTION_INVALID,
                    format!("`{bare}` is public and emits `{event}`"),
                )
                .at(span, "emitted when the app calls it")
                .reason("A package emits only about the outside world: its webhooks, what its own subscribers make of them, and its polls, never as a side effect of the app calling it. The app's own action emits there, where a reviewer sees it.")
                .next(format!(
                    "Remove the `emit` and let the app's action emit its own event, or take `public` off `{bare}` and run it from one of the package's subscribers"
                )),
            );
            return false;
        }
        let webhook = crate::webhooks::package_events(self.db, self.project)
            .of
            .get(folder)
            .is_some_and(|events| events.iter().any(|known| known.name == event));
        if webhook {
            let provider = match self.ownership {
                Ownership::Package(home) => format!("`{}`", home.versioned()),
                _ => "its provider".to_string(),
            };
            self.out.push(
                Diagnostic::new(
                    &rule::PACKAGE_ACTION_INVALID,
                    format!("`{bare}` emits `{event}`, which {provider}'s webhooks bring"),
                )
                .at(span, "a webhook's event")
                .reason("An event with `wire_name:` is one the provider sends, received through a connection's webhook and checked by its signature; a package's action emits its own events, declared without `wire_name:`, for what it makes of them.")
                .next(format!(
                    "Declare an event of the package's own for what `{bare}` found, without `wire_name:`, and emit that"
                )),
            );
            return false;
        }
        true
    }

    /// A package's action calling one of its package's actions: refused,
    /// since what a package action does in order is one way through it
    /// (chunk 7.6).
    pub(super) fn package_calls_action(&mut self, span: Span, called: &str) {
        let caller = self.action.clone().unwrap_or_default();
        self.out.push(
            Diagnostic::new(
                &rule::PACKAGE_ACTION_INVALID,
                format!("`{caller}` calls the package's action `{called}`"),
            )
            .at(span, "another action")
            .reason("A package's action calls only its own API: its operations, in any order, and its pure functions. The steps it makes are one way through it, which a retry replays in order.")
            .next(format!(
                "Make `{called}`'s calls here, or move what both share to a function"
            )),
        );
    }

    /// Whether this action may call through `connection`: it is in a
    /// domain the connection lists, and the package driver sends what the
    /// package needs.
    fn may_reach(
        &mut self,
        connection: &crate::connections::PackageConnection,
        span: Span,
        written: &str,
    ) -> bool {
        // Only the domains the connection lists call it (area 11, question
        // 18): a new caller is a one-line change a reviewer sees.
        let caller = match self.ownership {
            Ownership::Domain(domain) => Some(domain.as_str()),
            _ => None,
        };
        if !caller.is_some_and(|domain| connection.domains.iter().any(|listed| listed == domain)) {
            let listed = crate::and_list(
                &connection
                    .domains
                    .iter()
                    .map(|domain| format!("`{domain}`"))
                    .collect::<Vec<_>>(),
            );
            let (headline, next) = match caller {
                Some(domain) => (
                    format!(
                        "`{domain}` calls `{}`, whose `domains:` doesn't list it",
                        connection.name
                    ),
                    format!(
                        "Add `{domain}` to `{}`'s `domains:` in `app.jadpo`, or call one of {listed}'s public actions instead",
                        connection.name
                    ),
                ),
                None => (
                    format!("Code outside every domain calls `{}`", connection.name),
                    format!("Call `{written}` from an action of {listed}"),
                ),
            };
            self.out.push(
                Diagnostic::new(&rule::CONNECTION_DOMAIN_NOT_LISTED, headline)
                    .at(span, "not a listed domain")
                    .reason(format!(
                        "A connection lists the domains whose actions may call it, so a new caller is a one-line change a reviewer sees; `{}` lists {listed}, and other domains go through their public actions.",
                        connection.name
                    ))
                    .next(next),
            );
            return false;
        }
        // The package driver sends JSON over HTTP with a bearer token or
        // basic credentials (chunk 7.2c); the other protocols and schemes
        // come with the first package needing them.
        let protocol = connection.reach.protocol.as_deref().unwrap_or("json_http");
        let scheme = connection.reach.credential.as_deref();
        if !BUILT_PROTOCOLS.contains(&protocol)
            || scheme.is_some_and(|scheme| !BUILT_CREDENTIALS.contains(&scheme))
        {
            let what = if BUILT_PROTOCOLS.contains(&protocol) {
                format!("`CredentialScheme.{}`", scheme.unwrap_or_default())
            } else {
                format!("`Protocol.{protocol}`")
            };
            self.out.push(
                Diagnostic::new(
                    &rule::TOOLCHAIN_NOT_BUILT_YET,
                    format!("Calls with {what} aren't built yet"),
                )
                .at(span, "declared, not built")
                .reason(format!(
                    "`{}` is called with {what}, which the language declares but this compiler can't send yet: it sends JSON over HTTP with a bearer token or basic credentials.",
                    connection.package
                ))
                .next(format!("Do without `{written}` for now")),
            );
            return false;
        }
        true
    }

    /// An `attempt`'s arms on a provider's operation (area 11, "Calling"):
    /// each keyed by the code of one of its package's failures, mapping it
    /// to the app's own failure of the same kind, kept as that failure's
    /// cause. A failure no arm maps is a fault when it comes, so nothing
    /// is passed on. Gives each code with the failure it maps to.
    pub(super) fn provider_arms(
        &mut self,
        arms: &[Arm],
        names: &[String],
        inner: &Expr,
        written: &str,
        span: Span,
    ) -> Vec<(Option<String>, String)> {
        let safe = matches!(inner, Expr::CallExpr(call) if matches!(
            self.facts.reaches.get(&super::node_key(call.syntax())),
            Some(Reach::Provider { safe: true, .. })
        ));
        let codes: Vec<(String, String, Option<String>)> = names
            .iter()
            .filter_map(|name| {
                let info = self.failures.failure_info(name)?;
                Some((info.code.clone(), name.clone(), info.kind.clone()))
            })
            .collect();
        let mut mapped = Vec::new();
        for arm in arms {
            let failure = arm
                .value
                .as_ref()
                .and_then(|value| self.failure_given(value));
            if arm.key == "invalid" {
                self.invalid_outside_construction(arm, inner, written);
                continue;
            }
            // No answer in time, from an operation safe to send again, is
            // the runtime's `timed_out`, which the call may map to the
            // app's own `unavailable` failure (area 11, "Calling"; chunk
            // 7.2c2).
            if arm.key == TIMED_OUT {
                if let Some(failure) = self.timed_out_arm(arm, failure, safe, written) {
                    self.leave_mapped(&failure, span.clone());
                    mapped.push((Some(TIMED_OUT.to_string()), failure));
                }
                continue;
            }
            let Some((code, package_failure, kind)) =
                codes.iter().find(|(code, ..)| *code == arm.key).cloned()
            else {
                self.flow.open = true;
                let known: Vec<String> = codes.iter().map(|(code, ..)| code.clone()).collect();
                let near = nearest(&arm.key, known.iter().map(String::as_str));
                let listed = crate::and_list(
                    &known
                        .iter()
                        .map(|code| format!("`{code}:`"))
                        .collect::<Vec<_>>(),
                );
                let mut problem = Diagnostic::new(
                    &rule::FAILURE_UNKNOWN_MAPPING,
                    format!("`{written}` has no `{}:` condition to map", arm.key),
                )
                .at(arm.key_span.clone(), "not one of its failures")
                .reason(format!(
                    "A provider's operation is mapped at the call by the codes of the package failures its `failures:` lists{}.",
                    if known.is_empty() {
                        ", and it lists none".to_string()
                    } else {
                        format!(": {listed}")
                    }
                ));
                problem = match near {
                    Some(near) => problem
                        .next(format!("Use `{near}:` instead of `{}:`", arm.key))
                        .edit(arm.key_span.clone(), near),
                    None => problem.next(format!("Remove `{}: ...`", arm.key)),
                };
                self.out.push(problem);
                continue;
            };
            let Some(failure) = failure else {
                self.flow.open = true;
                continue;
            };
            let theirs = self
                .failures
                .failure_info(&failure)
                .and_then(|info| info.kind.clone());
            if let (Some(kind), Some(theirs)) = (&kind, &theirs)
                && kind != theirs
            {
                let at = arm
                    .value
                    .as_ref()
                    .map_or_else(|| arm.key_span.clone(), |value| self.span(value));
                let a = if kind.starts_with(['a', 'e', 'i', 'o', 'u']) {
                    "an"
                } else {
                    "a"
                };
                self.out.push(
                    Diagnostic::new(
                        &rule::FAILURE_MAPPING_KIND,
                        format!("`{code}:` needs {a} `{kind}` failure, and `{failure}` is `{theirs}`"),
                    )
                    .at(at, format!("kind `{theirs}`"))
                    .reason(format!(
                        "`{code}:` is `{package_failure}`, {a} `{kind}` failure, so the app's failure it is mapped to is `{kind}` too, and the client gets the status that means."
                    ))
                    .next(format!(
                        "Name {a} `{kind}` failure here, or make `{failure}`'s kind `FailureKind.{kind}`"
                    )),
                );
            }
            self.leave_mapped(&failure, span.clone());
            mapped.push((Some(code), failure));
        }
        mapped
    }

    /// `timed_out: Failure` on a provider's operation: only on one safe to
    /// send again, and to an `unavailable` failure. Gives the failure it
    /// maps to.
    fn timed_out_arm(
        &mut self,
        arm: &Arm,
        failure: Option<String>,
        safe: bool,
        written: &str,
    ) -> Option<String> {
        if !safe {
            self.flow.open = true;
            self.out.push(
                Diagnostic::new(
                    &rule::FAILURE_UNKNOWN_MAPPING,
                    format!("`{written}` can't map `timed_out:`"),
                )
                .at(arm.key_span.clone(), "it changes something, with no key")
                .reason("Only an operation safe to send again, one that changes nothing or is `idempotent: true`, maps a timeout to a failure; one that changes something and gets no answer in time may have acted, which is the runtime's `outcome_unknown`.")
                .next("Remove `timed_out: ...`, or mark the operation `idempotent: true` if sending it again changes nothing"),
            );
            return None;
        }
        let Some(failure) = failure else {
            self.flow.open = true;
            return None;
        };
        let theirs = self
            .failures
            .failure_info(&failure)
            .and_then(|info| info.kind.clone());
        if let Some(theirs) = theirs
            && theirs != "unavailable"
        {
            let at = arm
                .value
                .as_ref()
                .map_or_else(|| arm.key_span.clone(), |value| self.span(value));
            self.out.push(
                Diagnostic::new(
                    &rule::FAILURE_MAPPING_KIND,
                    format!("`timed_out:` needs an `unavailable` failure, and `{failure}` is `{theirs}`"),
                )
                .at(at, format!("kind `{theirs}`"))
                .reason("A provider that gives no answer in time is unavailable for now, so the app's failure it is mapped to is `unavailable` too, and the client gets the status that means.")
                .next(format!(
                    "Name an `unavailable` failure here, or make `{failure}`'s kind `FailureKind.unavailable`"
                )),
            );
        }
        Some(failure)
    }
}

/// The condition a provider's operation that gets no answer in time is
/// mapped by (area 4's runtime kind `timed_out`).
pub(crate) const TIMED_OUT: &str = "timed_out";
