//! Area 8 in bodies: a caller made from its record in a test, fixture or
//! seed (`Principal.user(user)`, question 10), reading a caller's fields
//! (`caller.id`, `caller.subject`, `caller.strength`, question 2), and an
//! action starting or ending the request's session (`sessions.start(user_id)`,
//! `sessions.end()`, question 9).

use jadpo_diag::{Diagnostic, rule};
use jadpo_std::http::{PrincipalKind, SessionOp};
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

    /// `sessions.start(user_id)` and `sessions.end()`: only in an action,
    /// which a route runs; the runtime sets or clears the session's cookies
    /// on that route's response (area 8, question 9).
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
        if self.context != Context::Action {
            let who = super::capital(self.context.with_a());
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_AUTH_SESSIONS_CONTEXT,
                    format!("{who} can't call `{written}`"),
                )
                .at(span, "outside an action")
                .reason(format!(
                    "`{written}` {} the session of the request a route answers, so only an action, which a route runs, calls it.",
                    if op == SessionOp::Start { "starts" } else { "ends" }
                ))
                .next(format!("Call `{written}` in the action a sign-in or sign-out route runs")),
            );
            self.args_only(call);
            return (
                Ty::Error,
                CallKind::Effect(written.to_string()),
                Reach::Unknown,
            );
        }
        let identity = self.identity();
        let user = identity
            .variant(PrincipalKind::User)
            .cloned()
            .filter(|_| identity.cookie().is_some());
        let Some(user) = user else {
            // Nothing to start a session for: what is missing is reported
            // where it is declared, or here when nothing is.
            if !identity
                .methods
                .iter()
                .any(|m| m.method == jadpo_std::AuthenticationMethod::BrowserSession)
            {
                self.out.push(
                    Diagnostic::new(
                        &rule::TYPE_AUTH_SESSIONS_CONTEXT,
                        format!("`{written}` needs browser sessions"),
                    )
                    .at(span, "no browser session declared")
                    .reason("A session is a browser session, which `authentication { browser_session: { cookie: \"<name>\" } }` declares.")
                    .next("Declare `browser_session:` in `authentication.jadpo`"),
                );
            }
            self.args_only(call);
            return (
                Ty::Error,
                CallKind::Effect(written.to_string()),
                Reach::Unknown,
            );
        };
        let wanted = match op {
            SessionOp::Start => 1,
            SessionOp::End => 0,
        };
        if args.len() != wanted || args.iter().any(|arg| arg.name().is_some()) {
            let at = call
                .arg_list()
                .map_or_else(|| self.span(call), |list| self.span(&list));
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_ARGUMENT_COUNT,
                    match op {
                        SessionOp::Start => format!("`{written}` takes the user's `{}.id`", user.entity),
                        SessionOp::End => format!("`{written}` takes nothing"),
                    },
                )
                .at(at, format!("{} given", args.len()))
                .reason(match op {
                    SessionOp::Start => format!(
                        "A session starts for one user, named by `{}.id`, once the action has checked who they are.",
                        user.entity
                    ),
                    SessionOp::End => "A session ends for the request it came with.".to_string(),
                })
                .next(match op {
                    SessionOp::Start => format!("Pass the user's `{}.id`", user.entity),
                    SessionOp::End => "Call `sessions.end()`".to_string(),
                }),
            );
            self.args_only(call);
            return (
                Ty::Error,
                CallKind::Effect(written.to_string()),
                Reach::Unknown,
            );
        }
        if let Some(value) = args.first().and_then(ast::Arg::expr) {
            self.check(
                &value,
                &Ty::Field(user.entity.clone(), vec!["id".to_string()]),
            );
        }
        (
            Ty::Prim(Prim::Unit),
            CallKind::Effect(written.to_string()),
            Reach::Session(op),
        )
    }
}
