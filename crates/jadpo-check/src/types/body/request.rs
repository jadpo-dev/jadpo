//! `request { ... }` in a route test (area 13, "Kinds of test"; area 7):
//! a request through the real HTTP boundary, without a network. Its
//! `method:` is an `HttpMethod` and its `path:` the route's path as `jadpo
//! routes` prints it, placeholders by name (`"/todos/{todo_id}"`), each
//! given in `params:`, typed by the placeholder; `input:` is the route's
//! `Input`. It gives a `Response`: `status`, the failure's `code` (`none`
//! on success) and the route's `output` (`none` unless it succeeded).
//! Signing in with `as:` comes with area 8.

use jadpo_diag::{Diagnostic, rule};
use jadpo_std::HttpMethod;
use jadpo_syntax::ast::{self, AstNode, Expr};

use super::super::ty::{Prim, Ty};
use super::Body;
use super::facts::node_key;
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
const ITEMS: [&str; 5] = ["method", "path", "params", "input", "as"];

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
        Some(match field {
            "status" => Some(Ty::Prim(Prim::Int)),
            "code" => Some(Ty::Prim(Prim::Text).nullable()),
            "output" => Some(match args.first() {
                Some(output) => output.clone().nullable(),
                None => Ty::Error,
            }),
            _ => None,
        })
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
                "as" => {
                    self.out.push(
                        Diagnostic::new(&rule::TOOLCHAIN_NOT_BUILT_YET, "Signing in with `as:` isn't built yet")
                            .at(key_span, "decided, not built")
                            .reason("A route test signs in with `as:` through the application's real authentication, which comes with sign-in (area 8, roadmap chunk 4.3).")
                            .next("Leave `as:` out for now"),
                    );
                    self.skip_members(record);
                    return Ty::Error;
                }
                other => {
                    let near = jadpo_diag::nearest(other, ITEMS.iter().copied());
                    self.out.push(
                        Diagnostic::new(&rule::SEMANTIC_REQUEST_INVALID, format!("`{other}` isn't something a request holds"))
                            .at(key_span, "unknown item")
                            .reason(format!(
                                "A request holds `method:`, `path:`, `params:`, `input:` and later `as:`.{}",
                                near.map_or_else(String::new, |near| format!(" Did you mean `{near}`?"))
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
        let Some(groups) = self.routes.clone() else {
            return Ty::Error;
        };
        let found = groups
            .routes()
            .map(|(_, route)| route)
            .find(|route| route.method == method && route.path() == path);
        let Some(route) = found else {
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
            (Some(ty), Some(value)) => self.check(&value, &Ty::Named(ty.clone())),
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
                .reason("A request gives the route's method, its path as `jadpo routes` prints it, each placeholder in `params:` and, when the route takes one, its `input:`.")
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
