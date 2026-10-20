//! The APIs an application declares (area 7, "APIs and versions"): an
//! `apis: { }` block in `application { }`, each API with its prefix. With
//! nothing declared there is one API, `main`, at `/`. Each API has its own
//! OpenAPI document and record, and may require an `Idempotency-Key` on its
//! writes (`idempotency: Idempotency.required`) and list the sign-in
//! methods it accepts (`authentication: [AuthenticationMethod.browser_session]`,
//! area 8), and serves its OpenAPI document at `<prefix>/openapi.json`
//! unless it says `serve_openapi: false`; its rate limits come with area
//! 16.

use std::collections::BTreeMap;

use jadpo_db::{Db, Project};
use jadpo_diag::{Diagnostic, Span, nearest, rule};
use jadpo_std::{AuthenticationMethod, Idempotency};
use jadpo_syntax::ast::{self, AstNode, Decl, Expr};

use crate::tree::{self, node_span};

/// The API a group belongs to when it names none.
pub const MAIN: &str = "main";

/// What an API's entry may hold.
pub const API_SETTINGS: [&str; 5] = [
    "prefix",
    "authentication",
    "idempotency",
    "serve_openapi",
    "rate_limit",
];

/// One API.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Api {
    pub name: String,
    /// Its prefix's segments; empty for `/`.
    pub prefix: Vec<String>,
    /// Whether its writes need an `Idempotency-Key`.
    pub idempotency: Idempotency,
    /// The sign-in methods it accepts, in written order; empty when it
    /// names none, and accepts every declared method.
    pub methods: Vec<AuthenticationMethod>,
    /// Whether it serves its OpenAPI document at `<prefix>/openapi.json`
    /// (area 7, "OpenAPI"): unless `serve_openapi: false`.
    pub serve_openapi: bool,
}

impl Api {
    /// `/v1`, or `/`.
    pub fn path(&self) -> String {
        if self.prefix.is_empty() {
            "/".to_string()
        } else {
            format!("/{}", self.prefix.join("/"))
        }
    }
}

/// The application's APIs and the problems in declaring them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Apis {
    /// In name order.
    pub apis: Vec<Api>,
    /// Whether `app.jadpo` declares them, rather than `main` being implied.
    pub declared: bool,
    pub problems: Vec<Diagnostic>,
}

impl Apis {
    pub fn get(&self, name: &str) -> Option<&Api> {
        self.apis.iter().find(|api| api.name == name)
    }
}

/// The APIs `app.jadpo` declares.
#[salsa::tracked(returns(ref))]
pub fn api_table(db: &dyn Db, project: Project) -> Apis {
    let app = project
        .files(db)
        .iter()
        .find(|file| file.path(db) == jadpo_db::PROJECT_FILE);
    let mut out = Apis::default();
    let mut declared: BTreeMap<String, Api> = BTreeMap::new();
    if let Some(source) = app.and_then(|file| tree::source(db, *file)) {
        let path = jadpo_db::PROJECT_FILE;
        for decl in source.decls() {
            let Decl::SingletonDecl(singleton) = decl else {
                continue;
            };
            if singleton.keyword().map(|t| t.text().to_string()).as_deref() != Some("application") {
                continue;
            }
            for (key, member) in tree::members(singleton.member_list()) {
                if tree::name_text(&key).as_deref() != Some("apis") {
                    continue;
                }
                out.declared = true;
                for (name, entry) in tree::members(member.block()) {
                    let Some(api) = tree::name_text(&name) else {
                        continue;
                    };
                    let span = node_span(path, name.syntax());
                    if let Some(Read {
                        prefix,
                        idempotency,
                        methods,
                        serve_openapi,
                    }) = read_api(
                        db,
                        project,
                        path,
                        &api,
                        span,
                        entry.block(),
                        &mut out.problems,
                    ) {
                        let overlap = declared
                            .values()
                            .find(|other| other.prefix == prefix)
                            .map(|other| other.name.clone());
                        let made = Api {
                            name: api.clone(),
                            prefix,
                            idempotency,
                            methods,
                            serve_openapi,
                        };
                        if let Some(other) = overlap {
                            out.problems.push(
                                Diagnostic::new(
                                    &rule::API_PREFIX_OVERLAP,
                                    format!("APIs `{other}` and `{api}` share the prefix `{}`", made.path()),
                                )
                                .at(node_span(path, name.syntax()), "same prefix")
                                .reason("Each API's routes start with its own prefix, so a URL says which API, OpenAPI document and record it belongs to.")
                                .next(format!("Give `{api}` a prefix of its own")),
                            );
                        }
                        declared.insert(api, made);
                    }
                }
            }
        }
    }
    out.apis = if out.declared {
        declared.into_values().collect()
    } else {
        vec![Api {
            name: MAIN.to_string(),
            prefix: Vec::new(),
            idempotency: Idempotency::Optional,
            methods: Vec::new(),
            serve_openapi: true,
        }]
    };
    out
}

/// The problems in `app.jadpo`'s `apis:`.
pub fn api_problems(db: &dyn Db, project: Project) -> &[Diagnostic] {
    &api_table(db, project).problems
}

/// What an API's entry gives.
struct Read {
    prefix: Vec<String>,
    idempotency: Idempotency,
    methods: Vec<AuthenticationMethod>,
    serve_openapi: bool,
}

fn read_api(
    db: &dyn Db,
    project: Project,
    path: &str,
    api: &str,
    span: Span,
    list: Option<ast::MemberList>,
    out: &mut Vec<Diagnostic>,
) -> Option<Read> {
    let mut prefix = None;
    let mut idempotency = Idempotency::Optional;
    let mut methods = Vec::new();
    let mut serve_openapi = true;
    for (key, member) in tree::members(list) {
        let Some(setting) = tree::name_text(&key) else {
            continue;
        };
        let key_span = node_span(path, key.syntax());
        match setting.as_str() {
            "prefix" => match member.value() {
                Some(Expr::RoutePathExpr(written)) => {
                    let segments: Vec<String> = written
                        .route_path()
                        .iter()
                        .flat_map(|path| path.route_segments())
                        .filter_map(|segment| {
                            segment
                                .path_segment_token()
                                .map(|token| token.text().to_string())
                                .filter(|text| !text.is_empty())
                        })
                        .collect();
                    prefix = Some(segments);
                }
                _ => {
                    out.push(
                        Diagnostic::new(&rule::API_PREFIX_REQUIRED, format!("API `{api}`'s prefix isn't a path"))
                            .at(key_span, "not a path")
                            .reason("An API's prefix is the path its routes start with: `prefix: /admin`.")
                            .next("Write the prefix as a path starting with `/`"),
                    );
                    return None;
                }
            },
            "idempotency" => match read_idempotency(member.value().as_ref()) {
                Ok(read) => idempotency = read,
                Err(written) => {
                    let span = member
                        .value()
                        .map_or_else(|| key_span.clone(), |value| node_span(path, value.syntax()));
                    out.push(
                        Diagnostic::new(
                            &rule::API_IDEMPOTENCY_INVALID,
                            format!("`{written}` isn't an `Idempotency`"),
                        )
                        .at(span.clone(), "not an `Idempotency`")
                        .reason("`idempotency:` takes `Idempotency.required`, so a write without an `Idempotency-Key` header is refused, or `Idempotency.optional`, the default, where a client may send one.")
                        .next("Write `Idempotency.required`, or leave `idempotency:` out")
                        .edit(span, "Idempotency.required"),
                    );
                }
            },
            "authentication" => {
                methods = read_methods(db, project, path, api, &member, out);
            }
            "serve_openapi" => {
                let value = member.value();
                let written = value
                    .as_ref()
                    .map(|value| tree::written(value.syntax()))
                    .unwrap_or_default();
                match written.as_str() {
                    "true" => serve_openapi = true,
                    "false" => serve_openapi = false,
                    _ => {
                        let span = value
                            .map_or_else(|| key_span.clone(), |value| node_span(path, value.syntax()));
                        out.push(
                            Diagnostic::new(
                                &rule::API_SERVE_OPENAPI_INVALID,
                                format!("`{written}` isn't `true` or `false`"),
                            )
                            .at(span.clone(), "not a `Bool`")
                            .reason("`serve_openapi:` says whether the API serves its OpenAPI document at `<prefix>/openapi.json`: `false` turns it off, and `true`, the default, can be left out.")
                            .next("Write `serve_openapi: false`, or leave `serve_openapi:` out")
                            .edit(span, "false"),
                        );
                    }
                }
            }
            "rate_limit" => out.push(
                Diagnostic::new(
                    &rule::TOOLCHAIN_NOT_BUILT_YET,
                    format!("An API's `{setting}:` isn't built yet"),
                )
                .at(key_span, "decided, not built")
                .reason("An API's own `rate_limit:` comes with the rest of area 16's rate limits; every API already has the default limits.")
                .next(format!("Leave `{setting}:` out for now")),
            ),
            other => {
                let near = nearest(other, API_SETTINGS.iter().copied());
                let mut problem = Diagnostic::new(
                    &rule::API_SETTING_UNKNOWN,
                    format!("`{other}` isn't an API setting"),
                )
                .at(key_span.clone(), "unknown setting")
                .reason(format!(
                    "An API in `apis:` holds its `prefix:`, `idempotency:`, `authentication:` and `serve_openapi:`, and later its `rate_limit:`.{}",
                    near.map_or_else(String::new, |near| format!(" Did you mean `{near}`?"))
                ));
                problem = match near {
                    Some(near) => problem
                        .next(format!("Use `{near}` instead of `{other}`"))
                        .edit(key_span, near),
                    None => problem.next(format!("Remove `{other}`")),
                };
                out.push(problem);
            }
        }
    }
    if prefix.is_none() {
        out.push(
            Diagnostic::new(&rule::API_PREFIX_REQUIRED, format!("API `{api}` has no prefix"))
                .at(span, "no `prefix:`")
                .reason("Every declared API says the path its routes start with, so a URL says which API it belongs to: `main: { prefix: /v1 }`.")
                .next(format!("Add `prefix: /{api}` to `{api}`")),
        );
    }
    prefix.map(|prefix| Read {
        prefix,
        idempotency,
        methods,
        serve_openapi,
    })
}

/// An API's `authentication:`: a list of the sign-in methods
/// `authentication.jadpo` declares, each once.
fn read_methods(
    db: &dyn Db,
    project: Project,
    path: &str,
    api: &str,
    member: &ast::Member,
    out: &mut Vec<Diagnostic>,
) -> Vec<AuthenticationMethod> {
    let identity = crate::identity::identity(db, project);
    let declared: Vec<String> = identity
        .methods
        .iter()
        .map(|method| format!("`AuthenticationMethod.{}`", method.method.name()))
        .collect();
    let declared_line = if declared.is_empty() {
        "`authentication.jadpo` declares no way to sign in yet".to_string()
    } else {
        format!(
            "`authentication.jadpo` declares {}",
            crate::and_list(&declared)
        )
    };
    let value = member.value();
    let invalid = |at: Span, message: String, label: &str, reason: String| {
        Diagnostic::new(&rule::API_AUTHENTICATION_INVALID, message)
            .at(at, label.to_string())
            .reason(reason)
            .next(
                "List the methods the API accepts, each one `authentication.jadpo` declares"
                    .to_string(),
            )
    };
    let Some(Expr::ListExpr(list)) = &value else {
        let span = value.as_ref().map_or_else(
            || node_span(path, member.syntax()),
            |v| node_span(path, v.syntax()),
        );
        out.push(invalid(
            span,
            format!("API `{api}`'s `authentication:` isn't a list"),
            "not a list",
            format!(
                "An API lists the sign-in methods it accepts, such as `[AuthenticationMethod.browser_session]`; {declared_line}."
            ),
        ));
        return Vec::new();
    };
    let mut methods = Vec::new();
    for item in list.exprs() {
        let span = node_span(path, item.syntax());
        let written = tree::written(item.syntax());
        let read = tree::dotted(&item).and_then(|(first, rest)| {
            let [variant] = rest.as_slice() else {
                return None;
            };
            (tree::name_ref_text(&first)? == AuthenticationMethod::NAME).then_some(())?;
            AuthenticationMethod::from_name(&tree::name_ref_text(variant)?)
        });
        let Some(method) = read else {
            out.push(invalid(
                span,
                format!("`{written}` isn't an `AuthenticationMethod`"),
                "not a sign-in method",
                format!("An API lists sign-in methods as `AuthenticationMethod.<method>`; {declared_line}."),
            ));
            continue;
        };
        if methods.contains(&method) {
            out.push(invalid(
                span,
                format!("`{written}` is listed twice"),
                "listed again",
                "An API lists each method it accepts once.".to_string(),
            ));
            continue;
        }
        if identity.method(method).is_none() {
            out.push(invalid(
                span,
                format!("API `{api}` accepts `{}`, which isn't declared", method.name()),
                "not declared",
                format!("An API accepts only the ways to sign in the application declares; {declared_line}."),
            ));
            continue;
        }
        methods.push(method);
    }
    methods
}

/// An `idempotency:` value: `Idempotency.required` or `.optional`, or
/// what was written instead.
fn read_idempotency(value: Option<&Expr>) -> Result<Idempotency, String> {
    let written = || value.map(|v| tree::written(v.syntax())).unwrap_or_default();
    let (first, rest) = value.and_then(tree::dotted).ok_or_else(written)?;
    let [variant] = rest.as_slice() else {
        return Err(written());
    };
    if tree::name_ref_text(&first).as_deref() != Some(Idempotency::NAME) {
        return Err(written());
    }
    tree::name_ref_text(variant)
        .and_then(|name| Idempotency::from_name(&name))
        .ok_or_else(written)
}

/// The sign-in methods API `api` accepts (area 8): its own
/// `authentication:` list, or every method `authentication.jadpo`
/// declares when it names none.
pub fn accepted_methods(
    db: &dyn Db,
    project: Project,
    api: &str,
) -> Vec<jadpo_std::AuthenticationMethod> {
    match api_table(db, project).get(api) {
        Some(declared) if !declared.methods.is_empty() => declared.methods.clone(),
        _ => crate::identity::identity(db, project)
            .methods
            .iter()
            .map(|method| method.method)
            .collect(),
    }
}
