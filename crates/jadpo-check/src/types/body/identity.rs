//! Area 8 in bodies: a caller made from its record in a test, fixture or
//! seed (`Principal.user(user)`, question 10), reading a caller's fields
//! (`caller.id`, `caller.subject`, `caller.strength`, question 2), and an
//! action starting or ending the request's session (`sessions.start(user_id)`,
//! `sessions.end()`, question 9).

use jadpo_diag::{Diagnostic, rule};
use jadpo_std::http::{ISSUED_KEY, ISSUED_TOKEN, PrincipalKind, SessionOp};
use jadpo_syntax::ast::{self, AstNode};

use super::facts::{Reach, node_key};
use super::{Body, CallKind, Context};
use crate::identity::{PRINCIPAL, PRINCIPAL_FIELDS};
use crate::types::ty::{Prim, Ty};

/// The kind of caller a type is: `Principal.user` is a user.
pub(super) fn caller_kind(ty: &Ty) -> Option<PrincipalKind> {
    match ty.strip_nullable() {
        Ty::Opaque(name, _) => [PrincipalKind::User, PrincipalKind::Service]
            .into_iter()
            .find(|kind| kind.type_name() == name),
        _ => None,
    }
}

impl Body<'_> {
    /// `caller.<field>` for a caller of kind `kind`; `None` when `receiver`
    /// isn't a caller.
    pub(super) fn caller_field(
        &mut self,
        receiver: &Ty,
        name: &ast::NameRef,
        node: &ast::FieldExpr,
    ) -> Option<Ty> {
        let field = super::text(name);
        let span = self.span(name);
        let listed: Vec<String> = PRINCIPAL_FIELDS.iter().map(|f| format!("`{f}`")).collect();
        if let Ty::Opaque(whole, _) = receiver.strip_nullable()
            && whole == PRINCIPAL
        {
            self.out.push(
                Diagnostic::new(&rule::TYPE_UNKNOWN_FIELD, format!("`Principal` has no field `{field}`"))
                    .at(span, "either kind of caller")
                    .reason(format!(
                        "A `Principal` is a user or a service, whose fields are typed by its entity, so code takes the kind it needs, `Principal.user`, and reads its {}.",
                        crate::and_list(&listed)
                    ))
                    .next("Take a `Principal.user` or a `Principal.service` instead"),
            );
            return Some(Ty::Error);
        }
        let kind = caller_kind(receiver)?;
        let found = self.identity().field_type(kind, &field);
        let Some(ty) = found else {
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_UNKNOWN_FIELD,
                    format!("`{}` has no field `{field}`", kind.type_name()),
                )
                .at(span, "not a caller's field")
                .reason(format!(
                    "A caller's fields are {}; a role, a tenant or an email is read from its entity, by its `id`.",
                    crate::and_list(&listed)
                ))
                .next("Read the field from the caller's record, found by its `id`"),
            );
            return Some(Ty::Error);
        };
        self.facts.fields.insert(
            node_key(node.syntax()),
            (kind.type_name().to_string(), vec![field]),
        );
        Some(ty)
    }

    /// `Principal.user(record)`: a caller made from its entity's record,
    /// only in tests, fixtures and seeds, since application code gets a
    /// caller only by signing in (area 8, question 10).
    pub(super) fn caller_call(
        &mut self,
        kind: PrincipalKind,
        call: &ast::CallExpr,
        callee: &ast::Expr,
        written: &str,
    ) -> (Ty, CallKind, Reach) {
        let span = self.span(callee);
        if self.context != Context::Test {
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_AUTH_PRINCIPAL_CONSTRUCTION,
                    format!("{} can't make a caller", super::capital(self.context.with_a())),
                )
                .at(span, "made from a record")
                .reason(format!(
                    "A caller only comes from signing in; tests, fixtures and seeds make one from a record with `{written}(...)`, and application code takes it as a parameter a route passes."
                ))
                .next(format!(
                    "Take a `{}` parameter, which the route passes as `current_principal.{}`",
                    kind.type_name(),
                    kind.name()
                )),
            );
            self.args_only(call);
            return (Ty::Error, CallKind::Other, Reach::Unknown);
        }
        let identity = self.identity();
        let Some(variant) = identity.variant(kind) else {
            if !identity.written.contains(&kind) {
                self.out.push(
                    Diagnostic::new(
                        &rule::TYPE_AUTH_PRINCIPAL_VARIANT_UNKNOWN,
                        format!("`principal {{ }}` declares no `{}:`", kind.name()),
                    )
                    .at(span, "not declared")
                    .reason(format!(
                        "A caller is made from the record of the entity the principal's `{}:` names; it declares {}.",
                        kind.name(),
                        identity.kinds_line()
                    ))
                    .next(format!("Declare `{}: {{ subject: <Entity>.<field> }}` in `principal {{ }}`", kind.name())),
                );
            }
            self.args_only(call);
            return (Ty::Error, CallKind::Other, Reach::Unknown);
        };
        let args: Vec<ast::Arg> = call
            .arg_list()
            .iter()
            .flat_map(|list| list.args())
            .collect();
        let record = Ty::Named(variant.entity.clone());
        match args.as_slice() {
            [arg] if arg.name().is_none() => {
                if let Some(value) = arg.expr() {
                    self.check(&value, &record);
                }
            }
            _ => {
                let at = call
                    .arg_list()
                    .map_or_else(|| self.span(call), |list| self.span(&list));
                self.out.push(
                    Diagnostic::new(
                        &rule::TYPE_ARGUMENT_COUNT,
                        format!("`{written}` takes one `{}`", variant.entity),
                    )
                    .at(at, format!("{} given", args.len()))
                    .reason(format!(
                        "A {} caller is made from its record, a `{}`.",
                        kind.name(),
                        variant.entity
                    ))
                    .next(format!("Pass the `{}` record", variant.entity)),
                );
                self.args_only(call);
                return (Ty::Error, CallKind::Other, Reach::Unknown);
            }
        }
        (
            Ty::Opaque(kind.type_name().to_string(), Vec::new()),
            CallKind::Pure(format!("{written}(...)")),
            Reach::Caller(kind),
        )
    }

    /// `sessions.start(user_id)`, `sessions.end()`, `sessions.issue_token`,
    /// `.tokens` and `.revoke_token`, and the service entity's
    /// `issue_key`, `keys` and `revoke_key`: only in an action, which keeps
    /// what they write in its transaction; for a session, the runtime sets
    /// or clears its cookies on the route's response (area 8, question 9).
    pub(super) fn session_call(
        &mut self,
        op: SessionOp,
        call: &ast::CallExpr,
        callee: &ast::Expr,
        written: &str,
    ) -> (Ty, CallKind, Reach) {
        let span = self.span(callee);
        let args: Vec<ast::Arg> = call
            .arg_list()
            .iter()
            .flat_map(|list| list.args())
            .collect();
        let failed = |body: &mut Self| {
            body.args_only(call);
            (
                Ty::Error,
                CallKind::Effect(written.to_string()),
                Reach::Unknown,
            )
        };
        if self.context != Context::Action {
            let who = super::capital(self.context.with_a());
            let does = match op {
                SessionOp::Start => "starts the session of the request a route answers".to_string(),
                SessionOp::End => "ends the session of the request a route answers".to_string(),
                SessionOp::Tokens | SessionOp::Keys => format!(
                    "reads the {} the runtime keeps, which only an action, run by a route or another action, reads",
                    if op == SessionOp::Tokens {
                        "tokens"
                    } else {
                        "keys"
                    }
                ),
                _ => format!(
                    "writes the {} the runtime keeps, in the transaction of the action that calls it",
                    if op.on_service() { "keys" } else { "tokens" }
                ),
            };
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_AUTH_SESSIONS_CONTEXT,
                    format!("{who} can't call `{written}`"),
                )
                .at(span, "outside an action")
                .reason(format!("`{written}` {does}, so only an action calls it."))
                .next(match op {
                    SessionOp::Start | SessionOp::End => {
                        format!("Call `{written}` in the action a sign-in or sign-out route runs")
                    }
                    _ => format!("Call `{written}` in an action"),
                }),
            );
            return failed(self);
        }
        let identity = self.identity();
        let method = op.method();
        let kind = if op.on_service() {
            PrincipalKind::Service
        } else {
            PrincipalKind::User
        };
        // `sessions.end()` ends a browser session or an API token.
        let available = match op {
            SessionOp::End => identity.sessions_store().is_some(),
            _ => identity.built(method),
        };
        let declared = identity.methods.iter().any(|m| {
            m.method == method
                || (op == SessionOp::End && m.method == jadpo_std::AuthenticationMethod::ApiToken)
        });
        let variant = identity.variant(kind).cloned().filter(|_| available);
        let Some(variant) = variant else {
            // Nothing to issue for: what is missing is reported where it
            // is declared, or here when nothing is.
            if !declared {
                let (needs, declare) = match method {
                    jadpo_std::AuthenticationMethod::BrowserSession if op == SessionOp::End => (
                        "browser sessions or API tokens",
                        "A session is a browser session, which `authentication { browser_session: { cookie: \"<name>\" } }` declares, or an API token, which `api_token: {}` declares.",
                    ),
                    jadpo_std::AuthenticationMethod::BrowserSession => (
                        "browser sessions",
                        "A session is a browser session, which `authentication { browser_session: { cookie: \"<name>\" } }` declares.",
                    ),
                    jadpo_std::AuthenticationMethod::ApiToken => (
                        "API tokens",
                        "An API token signs a user in from the application's own clients, which `authentication { api_token: {} }` declares.",
                    ),
                    _ => (
                        "service keys",
                        "A service key signs a service in, which `authentication { service_key: {} }` declares.",
                    ),
                };
                self.out.push(
                    Diagnostic::new(
                        &rule::TYPE_AUTH_SESSIONS_CONTEXT,
                        format!("`{written}` needs {needs}"),
                    )
                    .at(span, format!("no `{}` declared", method.name()))
                    .reason(declare)
                    .next(format!(
                        "Declare `{}:` in `authentication.jadpo`",
                        method.name()
                    )),
                );
            }
            return failed(self);
        };
        let entity = variant.entity.clone();
        let id = |of: &str| Ty::Field(of.to_string(), vec!["id".to_string()]);
        let (params, result): (Vec<(&str, Ty)>, Ty) = match op {
            SessionOp::Start => (vec![("the user's", id(&entity))], Ty::Prim(Prim::Unit)),
            SessionOp::End => (Vec::new(), Ty::Prim(Prim::Unit)),
            SessionOp::IssueToken => (vec![("the user's", id(&entity))], Ty::Prim(Prim::ApiToken)),
            SessionOp::Tokens => (
                vec![("the user's", id(&entity))],
                Ty::List(Box::new(Ty::Opaque(ISSUED_TOKEN.to_string(), Vec::new()))),
            ),
            SessionOp::RevokeToken => (
                vec![("the token's", Ty::Prim(Prim::Uuid))],
                Ty::Prim(Prim::Unit),
            ),
            SessionOp::IssueKey => (
                vec![
                    ("the service's", id(&entity)),
                    ("when it expires,", Ty::Prim(Prim::Instant)),
                ],
                Ty::Prim(Prim::ServiceKey),
            ),
            SessionOp::Keys => (
                vec![("the service's", id(&entity))],
                Ty::List(Box::new(Ty::Opaque(ISSUED_KEY.to_string(), Vec::new()))),
            ),
            SessionOp::RevokeKey => (
                vec![("the key's", Ty::Prim(Prim::Uuid))],
                Ty::Prim(Prim::Unit),
            ),
        };
        if args.len() != params.len() || args.iter().any(|arg| arg.name().is_some()) {
            let at = call
                .arg_list()
                .map_or_else(|| self.span(call), |list| self.span(&list));
            let takes: Vec<String> = params
                .iter()
                .map(|(whose, ty)| format!("{whose} `{ty}`"))
                .collect();
            let takes = if takes.is_empty() {
                "nothing".to_string()
            } else {
                crate::and_list(&takes)
            };
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_ARGUMENT_COUNT,
                    format!("`{written}` takes {takes}"),
                )
                .at(at, format!("{} given", args.len()))
                .reason(match op {
                    SessionOp::Start => format!(
                        "A session starts for one user, named by `{entity}.id`, once the action has checked who they are."
                    ),
                    SessionOp::End => "A session ends for the request it came with.".to_string(),
                    _ => format!("`{written}` takes {takes}, in that order."),
                })
                .next(match op {
                    SessionOp::End => "Call `sessions.end()`".to_string(),
                    _ => format!("Pass {takes}"),
                }),
            );
            return failed(self);
        }
        for (arg, (_, ty)) in args.iter().zip(&params) {
            if let Some(value) = arg.expr() {
                self.check(&value, ty);
            }
        }
        (
            result,
            CallKind::Effect(written.to_string()),
            Reach::Session(op),
        )
    }

    /// `Entity.issue_key(...)`, `Entity.keys(...)` or
    /// `Entity.revoke_key(...)`: the service entity's generated operations
    /// (area 8, question 7); on any other entity, no operation at all.
    pub(super) fn key_call(
        &mut self,
        entity: &str,
        op: SessionOp,
        call: &ast::CallExpr,
        callee: &ast::Expr,
        written: &str,
    ) -> (Ty, CallKind, Reach) {
        let identity = self.identity();
        let service = identity.variant(PrincipalKind::Service);
        if service.is_none_or(|variant| variant.entity != entity) {
            let span = self.span(callee);
            let message = format!("`{entity}` has no operation `{}`", op.name());
            self.out.push(
                Diagnostic::new(&rule::SEMANTIC_UNKNOWN_CALLEE, message)
                    .at(span, "not an operation")
                    .reason(match service {
                        Some(variant) => format!(
                            "`issue_key`, `keys` and `revoke_key` belong to the entity the principal's `service:` names, `{}`.",
                            variant.entity
                        ),
                        None => "`issue_key`, `keys` and `revoke_key` belong to the entity the principal's `service:` names, and `principal { }` declares none.".to_string(),
                    })
                    .next(match service {
                        Some(variant) => format!("Call `{}.{}`", variant.entity, op.name()),
                        None => format!(
                            "Declare `service: {{ subject: {entity}.<field> }}` in `principal {{ }}`"
                        ),
                    }),
            );
            self.args_only(call);
            return (
                Ty::Error,
                CallKind::Effect(written.to_string()),
                Reach::Unknown,
            );
        }
        self.session_call(op, call, callee, written)
    }
}
