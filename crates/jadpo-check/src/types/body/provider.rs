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
use jadpo_syntax::ast::{self, Expr};

use jadpo_diag::Span;

use super::super::ty::{Prim, Ty};
use super::facts::Reach;
use super::failing::Arm;
use super::{Body, CallKind, Context};

/// The protocols and credential schemes the package driver sends with
/// (chunk 7.2c): JSON over HTTP, with a bearer token or basic credentials.
pub(crate) const BUILT_PROTOCOLS: [&str; 1] = ["json_http"];
pub(crate) const BUILT_CREDENTIALS: [&str; 2] = ["bearer", "basic"];

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
        let operations = crate::providers::operations(self.db, self.project);
        let found = match rest {
            [resource, name] => operations.find(&connection.folder, resource, name),
            _ => None,
        };
        let Some(operation) = found.cloned() else {
            // An operation that doesn't read is reported where it is
            // declared.
            if crate::providers::has_problems(self.db, self.project, &connection.folder) {
                return failed(self);
            }
            let known: Vec<String> = operations
                .of(&connection.folder)
                .filter(|operation| operation.public)
                .map(crate::providers::Operation::full_name)
                .collect();
            let asked = rest.join(".");
            let near = nearest(&asked, known.iter().map(String::as_str));
            let mut problem = Diagnostic::new(
                &rule::SEMANTIC_UNKNOWN_CALLEE,
                format!(
                    "`{}` has no operation `{asked}`",
                    connection.name
                ),
            )
            .at(span.clone(), "no such operation")
            .reason(format!(
                "A connection to `{}` calls its package's public operations as `{}.<resource>.<operation>(...)`{}.",
                connection.package,
                connection.name,
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
                .reason(format!(
                    "A connection calls a package's public resources; one without `public` is reached only by the package's own actions, subscribers and polls, and may change between versions."
                ))
                .next(format!(
                    "Mark `resource {}` `public` in the package, or call what it offers instead",
                    operation.resource
                )),
            );
            return failed(self);
        }
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
            return failed(self);
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
            return failed(self);
        }
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
            return failed(self);
        }
        if let (Some((ty, _)), Some(value)) = (&operation.input, args.first().and_then(ast::Arg::expr))
        {
            self.check(&value, ty);
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
            },
        )
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
        let codes: Vec<(String, String, Option<String>)> = names
            .iter()
            .filter_map(|name| {
                let info = self.failures.failure_info(name)?;
                Some((info.code.clone(), name.clone(), info.kind.clone()))
            })
            .collect();
        let mut mapped = Vec::new();
        for arm in arms {
            let failure = arm.value.as_ref().and_then(|value| self.failure_given(value));
            if arm.key == "invalid" {
                self.invalid_outside_construction(arm, inner, written);
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
                continue;
            }
            self.leave_mapped(&failure, span.clone());
            mapped.push((Some(code), failure));
        }
        mapped
    }
}
