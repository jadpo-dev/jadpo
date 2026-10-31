//! `request { ... }` in a route test (area 13, "Kinds of test"; area 7):
//! a request through the real HTTP boundary, without a network. Its
//! `method:` is an `HttpMethod` and its `path:` the route's path as `jadpo
//! routes` prints it, placeholders by name (`"/todos/{todo_id}"`), each
//! given in `params:`, typed by the placeholder; `input:` is the route's
//! `Input`. It gives a `Response`: `status`, the failure's `code` (`none`
//! on success) and the route's `output` (`none` unless it succeeded); a
//! streaming route's gives `pieces`, each `piece` event's text, in place
//! of `output`, and `code` the `error` event's that ended it (chunk 5.5v6).
//! `as:` signs in as a caller (area 8) through the route's real
//! authentication: a user with a browser session. Every request but a
//! `GET` sends a fresh `Idempotency-Key`, as a real client would, or the
//! text `idempotency_key:` gives, to test repeats and refusals (area 13,
//! chunk 4.7).

use jadpo_diag::{Diagnostic, rule};
use jadpo_std::HttpMethod;
use jadpo_syntax::ast::{self, AstNode, Expr};

use super::super::ty::{Prim, Ty};
use super::Body;
use super::facts::node_key;
use crate::lookups::Name;
use crate::routes::{Segment, path_text};
use crate::tree::{self, dotted};

/// The type a request gives: `Response`, carrying the route's output.
pub(crate) const RESPONSE: &str = "Response";

/// What the walk decided about one request, for the typed body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RequestFact {
    pub route: jadpo_graph::Key,
    /// The route's placeholders, in its order.
    pub params: Vec<String>,
}

/// What a `request` block holds.
const ITEMS: [&str; 8] = [
    "method",
    "path",
    "params",
    "input",
    "idempotency_key",
    "as",
    "sign_in",
    "strength",
];

impl Body<'_> {
    /// The type of `response.<field>`; `None` when `receiver` isn't a
    /// response.
    pub(super) fn response_field(&mut self, receiver: &Ty, field: &str) -> Option<Option<Ty>> {
        let Ty::Opaque(name, args) = receiver.strip_nullable() else {
            return None;
        };
        if name != RESPONSE {
            return None;
        }
        Some(
            response_members(args)
                .into_iter()
                .find(|(name, _)| *name == field)
                .map(|(_, ty)| ty),
        )
    }

    pub(super) fn request(&mut self, record: &ast::RecordExpr) -> Ty {
        let span = record
            .expr()
            .map_or_else(|| self.span(record), |head| self.span(&head));
        if !self.test {
            self.out.push(
                Diagnostic::new(&rule::SEMANTIC_REQUEST_INVALID, "`request` is only for route tests")
                    .at(span, "outside a test")
                    .reason("A route test sends a request through the real HTTP boundary; application code calls the action or query a route runs instead.")
                    .next("Call the action or query itself"),
            );
            self.skip_members(record);
            return Ty::Error;
        }
        let mut method = None;
        let mut path = None;
        let mut params: Vec<(String, Expr, jadpo_diag::Span)> = Vec::new();
        let mut input = None;
        let mut caller: Option<(Expr, jadpo_diag::Span)> = None;
        let mut sent_key: Option<(Expr, jadpo_diag::Span)> = None;
        // How `as:`'s user signed in (chunk A10): by magic link with one
        // factor unless these say otherwise.
        let mut ways: Vec<(&str, Expr, jadpo_diag::Span)> = Vec::new();
        for (key, member) in tree::members(record.member_list()) {
            let Some(item) = tree::name_text(&key) else {
                continue;
            };
            let key_span = self.span(&key);
            match item.as_str() {
                "method" => {
                    let written = member.value().and_then(|value| {
                        let (first, rest) = dotted(&value)?;
                        let [variant] = rest.as_slice() else {
                            return None;
                        };
                        (tree::name_ref_text(&first)? == HttpMethod::NAME).then_some(())?;
                        HttpMethod::from_name(&tree::name_ref_text(variant)?)
                    });
                    if written.is_none() {
                        self.out.push(
                            Diagnostic::new(&rule::SEMANTIC_REQUEST_INVALID, "`method:` takes an `HttpMethod`")
                                .at(key_span, "not an `HttpMethod`")
                                .reason("A request's method is `HttpMethod.get`, `.post`, `.put`, `.patch` or `.delete`.")
                                .next("Write the method as `HttpMethod.<method>`"),
                        );
                        return Ty::Error;
                    }
                    method = written;
                }
                "path" => match member.value() {
                    Some(value @ Expr::Literal(_)) => {
                        match crate::types::Const::from_expr(&value) {
                            crate::types::Const::Str(text) => {
                                path = Some((text, self.span(&value)))
                            }
                            _ => {
                                self.request_problem(
                                    key_span,
                                    "`path:` takes the route's path as text",
                                );
                                return Ty::Error;
                            }
                        }
                    }
                    _ => {
                        self.request_problem(key_span, "`path:` takes the route's path as text");
                        return Ty::Error;
                    }
                },
                "params" => {
                    for (name, param) in tree::members(member.block()) {
                        if let (Some(text), Some(value)) = (tree::name_text(&name), param.value()) {
                            params.push((text, value, self.span(&name)));
                        }
                    }
                }
                "input" => input = member.value(),
                "idempotency_key" => {
                    if let Some(value) = member.value() {
                        sent_key = Some((value, key_span));
                    }
                }
                "as" => {
                    if let Some(value) = member.value() {
                        caller = Some((value, key_span));
                    }
                }
                "sign_in" | "strength" => {
                    if let Some(value) = member.value() {
                        let word = if item == "sign_in" {
                            "sign_in"
                        } else {
                            "strength"
                        };
                        ways.push((word, value, key_span));
                    }
                }
                other => {
                    let near = jadpo_diag::nearest(other, ITEMS.iter().copied());
                    self.out.push(
                        Diagnostic::new(
                            &rule::SEMANTIC_REQUEST_INVALID,
                            format!("`{other}` isn't something a request holds"),
                        )
                        .at(key_span, "unknown item")
                        .reason(format!(
                            "A request holds `method:`, `path:`, `params:`, `input:`, `idempotency_key:`, `as:`, and with `as:` `sign_in:` and `strength:`.{}",
                            near.map_or_else(String::new, |near| format!(
                                " Did you mean `{near}`?"
                            ))
                        ))
                        .next(match near {
                            Some(near) => format!("Use `{near}` instead of `{other}`"),
                            None => format!("Remove `{other}`"),
                        }),
                    );
                    return Ty::Error;
                }
            }
        }
        let (Some(method), Some((path, path_span))) = (method, path) else {
            self.request_problem(span, "A request needs its `method:` and `path:`");
            return Ty::Error;
        };
        // The one route it names, by label, so the test depends on that
        // route alone; only a mistake reads them all, to list the paths.
        let label = format!("{} {path}", method.word());
        let found = crate::routes::route_at(self.db, self.project, Name::new(self.db, label))
            .clone()
            .or_else(|| generated_magic_link(self.db, self.project, method, &path));
        let Some(route) = found else {
            let groups = crate::routes::groups(self.db, self.project);
            let paths: Vec<String> = groups
                .routes()
                .filter(|(_, route)| route.method == method)
                .map(|(_, route)| format!("`{}`", route.path()))
                .collect();
            self.out.push(
                Diagnostic::new(
                    &rule::SEMANTIC_REQUEST_ROUTE_UNKNOWN,
                    format!("No route answers `{} {path}`", method.word()),
                )
                .at(path_span, "no such route")
                .reason(if paths.is_empty() {
                    format!("The project has no `{}` route.", method.word())
                } else {
                    format!(
                        "A request names a route by its method and its path as `jadpo routes` prints it, placeholders by name; the `{}` routes are {}.",
                        method.word(),
                        crate::and_list(&paths)
                    )
                })
                .next("Write the path of the route this test exercises, as `jadpo routes` lists it"),
            );
            return Ty::Error;
        };
        // Each placeholder is given once, typed by the route.
        let mut ok = true;
        let mut order = Vec::new();
        for segment in &route.segments {
            let Segment::Param { name, ty } = segment else {
                continue;
            };
            order.push(name.clone());
            match params.iter().find(|(given, ..)| given == name) {
                Some((_, value, _)) => {
                    let value = value.clone();
                    self.check(&value, ty);
                }
                None => {
                    ok = false;
                    self.request_problem(
                        path_span.clone(),
                        &format!(
                            "`{}` needs `{name}` in `params:`",
                            path_text(&route.segments)
                        ),
                    );
                }
            }
        }
        for (given, value, name_span) in &params {
            if !order.contains(given) {
                ok = false;
                let _ = value;
                self.request_problem(
                    name_span.clone(),
                    &format!(
                        "`{}` has no placeholder `{given}`",
                        path_text(&route.segments)
                    ),
                );
            }
        }
        match (&route.input, input) {
            (Some(ty), Some(value)) => {
                // A generated magic-link route's input is built in.
                let expected = if jadpo_std::http::magic_link_input(ty) {
                    Ty::Opaque(ty.clone(), Vec::new())
                } else {
                    Ty::Named(ty.clone())
                };
                self.check(&value, &expected)
            }
            (Some(ty), None) => {
                ok = false;
                self.request_problem(
                    span.clone(),
                    &format!("`{}` takes `input:`, a `{ty}`", route.label()),
                );
            }
            (None, Some(value)) => {
                ok = false;
                self.request_problem(
                    self.span(&value),
                    &format!("`{}` takes no input", route.label()),
                );
            }
            (None, None) => {}
        }
        if let Some((value, key_span)) = sent_key {
            // A `GET` changes nothing, so the runtime reads no key for it
            // (area 7, "Idempotent writes").
            if method == HttpMethod::Get {
                ok = false;
                self.infer(&value);
                self.request_problem(
                    key_span,
                    &format!("`{}` reads no `Idempotency-Key`", route.label()),
                );
            } else {
                self.check(&value, &Ty::Prim(Prim::Text));
            }
        }
        let mut user_signs_in = false;
        if let Some((value, key_span)) = caller {
            let ty = self.infer(&value);
            match super::identity::caller_kind(&ty) {
                _ if ty.has_error() => ok = false,
                _ if !route.needs_caller => {
                    ok = false;
                    self.request_problem(
                        key_span,
                        &format!(
                            "`{}` reads no credential, so no one signs in to call it",
                            route.label()
                        ),
                    );
                }
                Some(kind) => {
                    // A caller signs in through a method that signs its
                    // kind in: a browser session or an API token for a
                    // user, a service key for a service (area 8).
                    let identity = self.identity();
                    let signs_in = match kind {
                        jadpo_std::http::PrincipalKind::User => identity.sessions_store().is_some(),
                        jadpo_std::http::PrincipalKind::Service => identity.keys_store().is_some(),
                    };
                    user_signs_in = signs_in && kind == jadpo_std::http::PrincipalKind::User;
                    if !signs_in {
                        ok = false;
                        let (methods, declare) = match kind {
                            jadpo_std::http::PrincipalKind::User => (
                                "a browser session or an API token",
                                "`browser_session:` or `api_token:`",
                            ),
                            jadpo_std::http::PrincipalKind::Service => {
                                ("a service key", "`service_key:`")
                            }
                        };
                        self.out.push(
                            Diagnostic::new(
                                &rule::SEMANTIC_REQUEST_INVALID,
                                format!("Nothing signs a {} in for `as:`", kind.name()),
                            )
                            .at(key_span, "nothing to sign in with")
                            .reason(format!(
                                "A request signs in as its caller through the application's real authentication: a {} with {methods}, which `authentication {{ }}` doesn't declare.",
                                kind.name()
                            ))
                            .next(format!("Declare {declare} in `authentication.jadpo`")),
                        );
                    }
                }
                None => {
                    ok = false;
                    self.out.push(
                        Diagnostic::new(&rule::SEMANTIC_REQUEST_INVALID, format!("`as:` takes a caller, not a `{ty}`"))
                            .at(self.span(&value), format!("a `{ty}`"))
                            .reason("A request signs in as a caller made with `Principal.user(user)` or `Principal.service(service)`, through the route's real authentication.")
                            .next("Pass a `Principal.user`, made from the user's record with `Principal.user(user)`"),
                    );
                }
            }
        }
        // `sign_in:` and `strength:` say how `as:`'s user signed in, so
        // each needs a user signing in (chunk A10).
        for (word, value, key_span) in std::mem::take(&mut ways) {
            let expected = if word == "sign_in" {
                jadpo_std::SignInMethod::NAME
            } else {
                jadpo_std::AuthenticationStrength::NAME
            };
            self.check(&value, &Ty::Named(expected.to_string()));
            if !user_signs_in && ok {
                ok = false;
                self.out.push(
                    Diagnostic::new(
                        &rule::SEMANTIC_REQUEST_INVALID,
                        format!("`{word}:` says how `as:`'s user signed in, and this request signs no user in"),
                    )
                    .at(key_span, "no user signs in")
                    .reason("A request signs in as `as:`'s caller; `sign_in:` and `strength:` say which way in and how strongly a user did, for a role that needs one (area 9). Without them the user signed in by magic link with one factor.")
                    .next("Give `as: Principal.user(...)`, or remove this line"),
                );
            }
        }
        if !ok {
            return Ty::Error;
        }
        self.facts.requests.insert(
            node_key(record.syntax()),
            RequestFact {
                route: route.key.clone(),
                params: order,
            },
        );
        Ty::Opaque(
            RESPONSE.to_string(),
            vec![route.output.clone().unwrap_or(Ty::Prim(Prim::Unit))],
        )
    }

    fn request_problem(&mut self, span: jadpo_diag::Span, message: &str) {
        self.out.push(
            Diagnostic::new(&rule::SEMANTIC_REQUEST_INVALID, message.to_string())
                .at(span, "doesn't fit the route")
                .reason("A request gives the route's method, its path as `jadpo routes` prints it, each placeholder in `params:` and, when the route takes one, its `input:`; a request but a `GET` may give the `Idempotency-Key` it sends in `idempotency_key:`.")
                .next("Give the request what its route takes"),
        );
    }

    /// The values inside a request already reported, read so their names
    /// count as used.
    fn skip_members(&mut self, record: &ast::RecordExpr) {
        for (_, member) in tree::members(record.member_list()) {
            if let Some(value) = member.value() {
                self.infer(&value);
            }
        }
    }
}

/// The magic-link route the runtime generates at `method` and `path`
/// (area 8, "Magic links"), which a route test calls as it calls one of
/// the app's own (area 8, "In development and tests"): named by its graph
/// node, taking the built-in input the API record names
/// (`MagicLinkAsk`, `MagicLinkConfirmation`, `MagicLinkCode`) and
/// answering `MagicLinkConfirmed` or `MagicLinkToken`. Anyone may call
/// each, so none signs in with `as:`.
fn generated_magic_link(
    db: &dyn jadpo_db::Db,
    project: jadpo_db::Project,
    method: HttpMethod,
    path: &str,
) -> Option<crate::routes::RouteShape> {
    use jadpo_std::http::MagicLinkStep;
    let generated = crate::routes::generated::generating(db, project);
    let (path_segments, api_tokens) = generated.values().flatten().find_map(|what| match what {
        crate::routes::generated::Generating::MagicLink { path, api_tokens } => {
            Some((path.clone(), *api_tokens))
        }
        _ => None,
    })?;
    let step = MagicLinkStep::ALL.into_iter().find(|step| {
        (*step != MagicLinkStep::Redeem || api_tokens)
            && step.method() == method
            && format!("/{}", step.segments(&path_segments).join("/")) == path
    })?;
    let label = format!("{} {path}", method.word());
    Some(crate::routes::RouteShape {
        key: crate::graph::registry(db, project).key(
            jadpo_graph::Kind::GeneratedRoute,
            &label,
            None,
        ),
        method,
        segments: step
            .segments(&path_segments)
            .into_iter()
            .map(Segment::Fixed)
            .collect(),
        input: Some(step.input().to_string()),
        output: step
            .output()
            .map(|output| Ty::Opaque(output.to_string(), Vec::new())),
        needs_caller: false,
    })
}

/// A route test's response's fields (`status`, `code`, `output`), with
/// their types for a response whose output is `args`' one.
pub(super) fn response_members(args: &[Ty]) -> Vec<(&'static str, Ty)> {
    // A streaming route's: the text of each `piece` event in place of an
    // output, and `code` the `error` event's (area 7, "Streaming a
    // model's answer"; chunk 5.5v6).
    if args.first().is_some_and(crate::routes::streams) {
        return vec![
            ("status", Ty::Prim(Prim::Int)),
            ("code", Ty::Prim(Prim::Text).nullable()),
            ("pieces", Ty::List(Box::new(Ty::Prim(Prim::Text)))),
        ];
    }
    vec![
        ("status", Ty::Prim(Prim::Int)),
        ("code", Ty::Prim(Prim::Text).nullable()),
        (
            "output",
            match args.first() {
                Some(output) => output.clone().nullable(),
                None => Ty::Error,
            },
        ),
    ]
}
