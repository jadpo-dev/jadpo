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

/// The fields of the built-in objects sign-in gives code (area 8, area
/// 16): `FirstSignIn`, which the runtime gives `on_first_sign_in:`'s
/// action (the way in, the address and whether it was proved, the
/// provider's or token's subject and the name a provider's profile gives);
/// `MagicLinkRequest`, which `magic_link: { send: }`'s action is given; and
/// `MagicLinkEmail`, which the application's own sign-in email picks from.
pub(crate) fn first_sign_in_fields(
    identity: &crate::identity::Identity,
    ty: &Ty,
) -> Option<Vec<(&'static str, Ty)>> {
    match ty.strip_nullable() {
        Ty::Opaque(name, args) if args.is_empty() => sign_in_object_fields(identity, name),
        _ => None,
    }
}

/// The fields of the built-in sign-in object called `name`: with
/// `MagicLinkRedeemed`, which `magic_links.redeem` gives, the user's `id`
/// typed as the user entity's.
pub(crate) fn sign_in_object_fields(
    identity: &crate::identity::Identity,
    name: &str,
) -> Option<Vec<(&'static str, Ty)>> {
    use jadpo_std::email::{
        MAGIC_LINK_EMAIL_TYPE, MAGIC_LINK_EMAIL_TYPE_FIELDS, MAGIC_LINK_REQUEST,
    };
    match name {
        // What a test's provider says about the person (chunk A7b).
        jadpo_std::http::TEST_ACCOUNT => Some(vec![
            ("subject", Ty::Prim(Prim::Text)),
            ("email", Ty::Prim(Prim::Email).nullable()),
            ("email_verified", Ty::Prim(Prim::Bool)),
            ("name", Ty::Prim(Prim::Text).nullable()),
            // The company the account belongs to, which the provider's ID
            // token names (Google's `hd`, Microsoft's `tid`), and how
            // strongly the person signed in, its `amr` (chunk A10).
            ("organisation", Ty::Prim(Prim::Text).nullable()),
            (
                "strength",
                Ty::Named(jadpo_std::AuthenticationStrength::NAME.to_string()).nullable(),
            ),
        ]),
        jadpo_std::http::FIRST_SIGN_IN => Some(vec![
            ("method", method()),
            ("address", Ty::Prim(Prim::Email).nullable()),
            ("address_verified", Ty::Prim(Prim::Bool)),
            ("subject", Ty::Prim(Prim::Text).nullable()),
            ("name", Ty::Prim(Prim::Text).nullable()),
        ]),
        MAGIC_LINK_EMAIL_TYPE => Some(
            MAGIC_LINK_EMAIL_TYPE_FIELDS
                .iter()
                .map(|(field, builtin)| (*field, Ty::Prim(*builtin)))
                .collect(),
        ),
        MAGIC_LINK_REQUEST => Some(vec![
            ("address", Ty::Prim(Prim::Email)),
            ("sign_in_link", Ty::Prim(Prim::Url)),
            ("code", Ty::Prim(Prim::SignInCode)),
            ("expires_at", Ty::Prim(Prim::Instant)),
            ("locale", Ty::Named(jadpo_std::LOCALE.to_string())),
        ]),
        jadpo_std::http::MAGIC_LINK_REDEEMED => Some(vec![
            ("user_id", user_id(identity)),
            ("return_to", Ty::Prim(Prim::Text)),
        ]),
        // The events every sign-in publishes (area 8, "Events"; chunk
        // 7.3b4), in the order `test_built_fields` lists them.
        jadpo_std::http::SIGNED_IN => Some(vec![
            ("user_id", user_id(identity)),
            ("method", method()),
            ("first_time", Ty::Prim(Prim::Bool)),
        ]),
        jadpo_std::http::SIGN_IN_REFUSED => Some(vec![
            ("method", method()),
            (
                "reason",
                Ty::Named(jadpo_std::SignInRefusal::NAME.to_string()),
            ),
            ("user_id", user_id(identity).nullable()),
        ]),
        jadpo_std::http::SIGN_IN_CODES_DISABLED => Some(vec![
            ("user_id", user_id(identity).nullable()),
            ("until", Ty::Prim(Prim::Instant)),
        ]),
        // What a route test sends the generated magic-link routes, and
        // what the confirmation and the redemption answer (area 8, "In
        // development and tests"), as the API record names them.
        jadpo_std::http::MAGIC_LINK_ASK => Some(vec![
            ("address", Ty::Prim(Prim::Email)),
            ("return_to", Ty::Prim(Prim::Text).nullable()),
            (
                "locale",
                Ty::Named(jadpo_std::LOCALE.to_string()).nullable(),
            ),
        ]),
        jadpo_std::http::MAGIC_LINK_CONFIRMATION => Some(vec![
            ("token", Ty::Prim(Prim::SignInToken).nullable()),
            ("address", Ty::Prim(Prim::Email).nullable()),
            ("code", Ty::Prim(Prim::SignInCode).nullable()),
        ]),
        jadpo_std::http::MAGIC_LINK_CODE => Some(vec![
            ("address", Ty::Prim(Prim::Email)),
            ("code", Ty::Prim(Prim::SignInCode)),
        ]),
        jadpo_std::http::MAGIC_LINK_CONFIRMED => Some(vec![("return_to", Ty::Prim(Prim::Text))]),
        jadpo_std::http::MAGIC_LINK_TOKEN => Some(vec![("token", Ty::Prim(Prim::ApiToken))]),
        // A passkey's own events (chunk 5.5z2).
        jadpo_std::http::PASSKEY_ADDED | jadpo_std::http::PASSKEY_REMOVED => Some(vec![
            ("user_id", user_id(identity)),
            ("passkey_id", Ty::Prim(Prim::Uuid)),
        ]),
        _ => None,
    }
}

/// The user entity's `id`, as a sign-in object names its user.
fn user_id(identity: &crate::identity::Identity) -> Ty {
    identity
        .variant(PrincipalKind::User)
        .map_or(Ty::Prim(Prim::Uuid), |variant| {
            Ty::Field(variant.entity.clone(), vec!["id".to_string()])
        })
}

fn method() -> Ty {
    Ty::Named(jadpo_std::SignInMethod::NAME.to_string())
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
        if op.magic_link() {
            return self.magic_link_call(op, call, callee, written);
        }
        if op.test_passkey() {
            return self.test_passkey_call(op, call, callee, written);
        }
        if op == SessionOp::TestProviderSignIn {
            return self.test_provider_call(call, callee, written);
        }
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
                    "reads the {} the runtime keeps, which an action reads, or a `GET` route that runs it",
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
                .reason(if op.reads_only() {
                    format!("`{written}` {does}.")
                } else {
                    format!("`{written}` {does}, so only an action calls it.")
                })
                .next(match op {
                    SessionOp::Start | SessionOp::End => {
                        format!("Call `{written}` in the action a sign-in or sign-out route runs")
                    }
                    SessionOp::Tokens | SessionOp::Keys => {
                        format!("Call `{written}` in an action, or run it from a `GET` route")
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
                self.out.push(crate::identity::needs_method(
                    written,
                    method,
                    op == SessionOp::End,
                    span,
                ));
            }
            return failed(self);
        };
        let entity = variant.entity.clone();
        let id = |of: &str| Ty::Field(of.to_string(), vec!["id".to_string()]);
        // Each parameter's name, how a message describes it, and its type.
        type Param<'a> = (&'a str, &'a str, Ty);
        let (params, result): (Vec<Param>, Ty) = match op {
            SessionOp::Start => (
                vec![("user", "the user's", id(&entity))],
                Ty::Prim(Prim::Unit),
            ),
            SessionOp::End => (Vec::new(), Ty::Prim(Prim::Unit)),
            SessionOp::IssueToken => (
                vec![("user", "the user's", id(&entity))],
                Ty::Prim(Prim::ApiToken),
            ),
            SessionOp::Tokens => (
                vec![("user", "the user's", id(&entity))],
                Ty::List(Box::new(Ty::Opaque(ISSUED_TOKEN.to_string(), Vec::new()))),
            ),
            SessionOp::RevokeToken => (
                vec![("token_id", "the token's", Ty::Prim(Prim::Uuid))],
                Ty::Prim(Prim::Unit),
            ),
            SessionOp::IssueKey => (
                vec![
                    ("service_id", "the service's", id(&entity)),
                    ("expires_at", "its expiry, an", Ty::Prim(Prim::Instant)),
                ],
                Ty::Prim(Prim::ServiceKey),
            ),
            SessionOp::Keys => (
                vec![("service_id", "the service's", id(&entity))],
                Ty::List(Box::new(Ty::Opaque(ISSUED_KEY.to_string(), Vec::new()))),
            ),
            SessionOp::RevokeKey => (
                vec![("key_id", "the key's", Ty::Prim(Prim::Uuid))],
                Ty::Prim(Prim::Unit),
            ),
            SessionOp::MagicLinkRequest
            | SessionOp::MagicLinkRedeem
            | SessionOp::MagicLinkRedeemCode => {
                return self.magic_link_call(op, call, callee, written);
            }
            SessionOp::TestPasskeyRegister | SessionOp::TestPasskeySignIn => {
                return self.test_passkey_call(op, call, callee, written);
            }
            SessionOp::TestProviderSignIn => {
                return self.test_provider_call(call, callee, written);
            }
        };
        if args.len() != params.len() || args.iter().any(|arg| arg.name().is_some()) {
            let at = call
                .arg_list()
                .map_or_else(|| self.span(call), |list| self.span(&list));
            let takes: Vec<String> = params
                .iter()
                .map(|(_, whose, ty)| format!("{whose} `{ty}`"))
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
        for (arg, (name, _, ty)) in args.iter().zip(&params) {
            let Some(value) = arg.expr() else { continue };
            match ty {
                // Any type built on it, as the standard library's
                // operations take them: an `Instant` or a `Uuid`.
                Ty::Prim(prim) => {
                    let inferred = (!super::is_literal(&value)).then(|| self.infer(&value));
                    self.built_on_argument(&value, inferred, &[*prim], written, name);
                }
                _ => self.check(&value, ty),
            }
        }
        (
            result,
            CallKind::Effect(written.to_string()),
            Reach::Session(op),
        )
    }

    /// `magic_links.request(address, locale, return_to)`,
    /// `magic_links.redeem(token)` and `magic_links.redeem_code(address,
    /// code)`: magic links' generated operations, which the application's
    /// own routes call once `generate_routes: false` switches the generated
    /// ones off (area 8, "Writing the routes yourself"; chunk 7.3b3). Only
    /// an action calls them, as it calls `sessions.start`; each keeps every
    /// protection of the generated routes, in the runtime.
    pub(super) fn magic_link_call(
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
            let does = if op == SessionOp::MagicLinkRequest {
                "asks for a link and a code, keeping the request in the transaction of the action that calls it"
            } else {
                "uses a link or a code once, starting no session itself"
            };
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_AUTH_SESSIONS_CONTEXT,
                    format!("{who} can't call `{written}`"),
                )
                .at(span, "outside an action")
                .reason(format!("`{written}` {does}, so only an action calls it."))
                .next(format!(
                    "Call `{written}` in the action a sign-in route runs"
                )),
            );
            return failed(self);
        }
        let identity = self.identity();
        let Some(magic) = identity.magic_links() else {
            self.out.push(
                Diagnostic::new(
                    &rule::SEMANTIC_AUTH_SETTING_REQUIRED,
                    format!("`{written}` needs magic links"),
                )
                .at(span, "no `magic_link:` declared")
                .reason("The `magic_links` operations ask for and use the links and codes `sign_in: { magic_link: }` sends, with a browser session's origin beside it, and `authentication { }` declares none.")
                .next("Declare `magic_link:` in `sign_in:`, with `generate_routes: false`"),
            );
            return failed(self);
        };
        if magic.generate_routes {
            self.out.push(
                Diagnostic::new(
                    &rule::SEMANTIC_AUTH_SETTING_REQUIRED,
                    format!("`{written}` needs `generate_routes: false`"),
                )
                .at(span, "the routes are generated")
                .reason("With its routes generated, the runtime asks for and uses magic links itself; `generate_routes: false` switches them off and leaves the application to write its own over these operations, so each link is asked for and used one way.")
                .next("Add `generate_routes: false` to `magic_link:`"),
            );
            return failed(self);
        }
        // Each parameter's name, how a message describes it, and its type.
        type Param<'a> = (&'a str, &'a str, Ty);
        let redeemed = Ty::Opaque(jadpo_std::http::MAGIC_LINK_REDEEMED.to_string(), Vec::new());
        let (params, result): (Vec<Param>, Ty) = match op {
            SessionOp::MagicLinkRequest => (
                vec![
                    ("address", "the address, an", Ty::Prim(Prim::Email)),
                    (
                        "locale",
                        "the email's",
                        Ty::Named(jadpo_std::LOCALE.to_string()),
                    ),
                    (
                        "return_to",
                        "the path to go back to, a",
                        Ty::Prim(Prim::Text).nullable(),
                    ),
                ],
                Ty::Prim(Prim::Unit),
            ),
            SessionOp::MagicLinkRedeem => (
                vec![("token", "the link's", Ty::Prim(Prim::SignInToken))],
                redeemed,
            ),
            _ => (
                vec![
                    ("address", "the address, an", Ty::Prim(Prim::Email)),
                    ("code", "the", Ty::Prim(Prim::SignInCode)),
                ],
                redeemed,
            ),
        };
        if args.len() != params.len() || args.iter().any(|arg| arg.name().is_some()) {
            let at = call
                .arg_list()
                .map_or_else(|| self.span(call), |list| self.span(&list));
            let takes: Vec<String> = params
                .iter()
                .map(|(_, whose, ty)| format!("{whose} `{ty}`"))
                .collect();
            let takes = crate::and_list(&takes);
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_ARGUMENT_COUNT,
                    format!("`{written}` takes {takes}"),
                )
                .at(at, format!("{} given", args.len()))
                .reason(if op == SessionOp::MagicLinkRequest {
                    format!("`{written}` takes {takes}, in that order; pass `none` for the path when the person goes to `/`.")
                } else {
                    format!("`{written}` takes {takes}, in that order.")
                })
                .next(format!("Pass {takes}")),
            );
            return failed(self);
        }
        for (arg, (name, _, ty)) in args.iter().zip(&params) {
            let Some(value) = arg.expr() else { continue };
            match ty {
                // The path to go back to: text, or `none` for `/`.
                Ty::Nullable(_) => {
                    let found = self.infer(&value);
                    if found.has_error() || found == Ty::None {
                        continue;
                    }
                    if self.table.representation(&found) != Some(Prim::Text) {
                        let at = self.span(&value);
                        self.out.push(
                            Diagnostic::new(
                                &rule::TYPE_MISMATCH,
                                format!("`{found}` can't be used where a value built on `Text` is required"),
                            )
                            .at(at, format!("this is `{found}`"))
                            .reason(format!(
                                "`{written}`'s `{name}` is the path on the application's own site the person goes back to once signed in: text, or `none` for `/`."
                            ))
                            .next("Pass the path, a value built on `Text`, or `none`"),
                        );
                    }
                }
                // An `Email`, a `SignInToken` or a `SignInCode`, or a type
                // built on one, such as the user's address field.
                _ => self.check(&value, ty),
            }
        }
        (
            result,
            CallKind::Effect(written.to_string()),
            Reach::Session(op),
        )
    }

    /// `test_passkeys.register(user, name)` and `test_passkeys.sign_in(
    /// passkey)`: a software authenticator driving the generated passkey
    /// routes for real (area 8, "In development and tests"; chunk 5.5z2).
    /// Only a test calls them, so only a test ever holds a `TestPasskey`.
    pub(super) fn test_passkey_call(
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
        if !self.test {
            self.out.push(
                Diagnostic::new(
                    &rule::SEMANTIC_REQUEST_INVALID,
                    format!("`{written}` is only for route tests"),
                )
                .at(span, "outside a test")
                .reason("`test_passkeys` is a software authenticator a route test drives the generated passkey routes with; a real person's device answers them anywhere else, so application code never holds one.")
                .next(format!("Call `{written}` in a test, or remove it")),
            );
            return failed(self);
        }
        if self.identity().passkeys().is_none() {
            self.out.push(
                Diagnostic::new(
                    &rule::SEMANTIC_AUTH_SETTING_REQUIRED,
                    format!("`{written}` needs passkeys"),
                )
                .at(span, "no `passkey:` declared")
                .reason("`test_passkeys` signs in and adds passkeys through the routes `sign_in: { passkey: }` generates, and `authentication { }` declares none.")
                .next("Declare `passkey: {}` in `sign_in:`"),
            );
            return failed(self);
        }
        // Each parameter's name, how a message describes it, and its type.
        type Param<'a> = (&'a str, &'a str, Ty);
        let (params, result): (Vec<Param>, Ty) = match op {
            SessionOp::TestPasskeyRegister => (
                vec![
                    (
                        "user",
                        "the",
                        Ty::Opaque(PrincipalKind::User.type_name().to_string(), Vec::new()),
                    ),
                    ("name", "the passkey's", Ty::Prim(Prim::Text)),
                ],
                Ty::Opaque(jadpo_std::http::TEST_PASSKEY.to_string(), Vec::new()),
            ),
            _ => (
                vec![(
                    "passkey",
                    "the device's",
                    Ty::Opaque(jadpo_std::http::TEST_PASSKEY.to_string(), Vec::new()),
                )],
                Ty::Opaque(
                    super::request::RESPONSE.to_string(),
                    vec![Ty::Prim(Prim::Unit)],
                ),
            ),
        };
        if args.len() != params.len() || args.iter().any(|arg| arg.name().is_some()) {
            let at = call
                .arg_list()
                .map_or_else(|| self.span(call), |list| self.span(&list));
            let takes: Vec<String> = params
                .iter()
                .map(|(_, whose, ty)| format!("{whose} `{ty}`"))
                .collect();
            let takes = crate::and_list(&takes);
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_ARGUMENT_COUNT,
                    format!("`{written}` takes {takes}"),
                )
                .at(at, format!("{} given", args.len()))
                .reason(if op == SessionOp::TestPasskeyRegister {
                    format!("`{written}` adds a passkey for a user, signed in as `as:` signs in, under the name its person gives it: {takes}, in that order.")
                } else {
                    format!("`{written}` signs in with the passkey a device holds: {takes}.")
                })
                .next(format!("Pass {takes}")),
            );
            return failed(self);
        }
        for (arg, (name, _, ty)) in args.iter().zip(&params) {
            let Some(value) = arg.expr() else { continue };
            match ty {
                // Text, or a type built on it.
                Ty::Prim(prim) => {
                    let inferred = (!super::is_literal(&value)).then(|| self.infer(&value));
                    self.built_on_argument(&value, inferred, &[*prim], written, name);
                }
                _ => self.check(&value, ty),
            }
        }
        (
            result,
            CallKind::Effect(written.to_string()),
            Reach::Session(op),
        )
    }

    /// `test_providers.sign_in(method, account)` (area 8, "Providers";
    /// chunk A7b): only a test signs in through a provider this way.
    pub(super) fn test_provider_call(
        &mut self,
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
        if !self.test {
            self.out.push(
                Diagnostic::new(
                    &rule::SEMANTIC_REQUEST_INVALID,
                    format!("`{written}` is only for route tests"),
                )
                .at(span, "outside a test")
                .reason("`test_providers` signs a route test in through a provider's routes with the test run's local issuer as the provider; a real provider signs people in anywhere else, so application code never calls it.")
                .next(format!("Call `{written}` in a test, or remove it")),
            );
            return failed(self);
        }
        if self.identity().providers.is_empty() {
            self.out.push(
                Diagnostic::new(
                    &rule::SEMANTIC_AUTH_SETTING_REQUIRED,
                    format!("`{written}` needs a sign-in provider"),
                )
                .at(span, "no `providers:` declared")
                .reason("`test_providers` signs in through the routes each provider in `sign_in: { providers: }` generates, and `authentication { }` declares none.")
                .next("Declare `providers: [<connection>]` in `sign_in:`"),
            );
            return failed(self);
        }
        let params = [
            (
                "method",
                Ty::Named(jadpo_std::SignInMethod::NAME.to_string()),
            ),
            (
                "account",
                Ty::Opaque(jadpo_std::http::TEST_ACCOUNT.to_string(), Vec::new()),
            ),
        ];
        if args.len() != params.len() || args.iter().any(|arg| arg.name().is_some()) {
            let at = call
                .arg_list()
                .map_or_else(|| self.span(call), |list| self.span(&list));
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_ARGUMENT_COUNT,
                    format!("`{written}` takes a `SignInMethod` and a `TestAccount`"),
                )
                .at(at, format!("{} given", args.len()))
                .reason(format!("`{written}` signs the person a `TestAccount` describes in through the provider a `SignInMethod` names, in that order."))
                .next("Pass `SignInMethod.<provider>` and a `TestAccount { ... }`"),
            );
            return failed(self);
        }
        for (arg, (_, ty)) in args.iter().zip(&params) {
            if let Some(value) = arg.expr() {
                self.check(&value, ty);
            }
        }
        (
            Ty::Opaque(
                super::request::RESPONSE.to_string(),
                vec![Ty::Prim(Prim::Unit)],
            ),
            CallKind::Effect(written.to_string()),
            Reach::Session(SessionOp::TestProviderSignIn),
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
