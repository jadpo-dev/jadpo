//! The APIs an application declares (area 7, "APIs and versions"): an
//! `apis: { }` block in `application { }`, each API with its prefix. With
//! nothing declared there is one API, `main`, at `/`. Each API has its own
//! OpenAPI document and record, and may require an `Idempotency-Key` on its
//! writes (`idempotency: Idempotency.required`); its authentication and
//! rate limits come with their chunks.

use std::collections::BTreeMap;

use jadpo_db::{Db, Project};
use jadpo_diag::{Diagnostic, Span, nearest, rule};
use jadpo_std::Idempotency;
use jadpo_syntax::ast::{self, AstNode, Decl, Expr};

use crate::tree::{self, node_span};

/// The API a group belongs to when it names none.
pub const MAIN: &str = "main";

/// What an API's entry may hold.
pub const API_SETTINGS: [&str; 4] = ["prefix", "authentication", "idempotency", "rate_limit"];

/// One API.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Api {
    pub name: String,
    /// Its prefix's segments; empty for `/`.
    pub prefix: Vec<String>,
    /// Whether its writes need an `Idempotency-Key`.
    pub idempotency: Idempotency,
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
                    if let Some((prefix, idempotency)) =
                        read_api(path, &api, span, entry.block(), &mut out.problems)
                    {
                        let overlap = declared
                            .values()
                            .find(|other| other.prefix == prefix)
                            .map(|other| other.name.clone());
                        let made = Api {
                            name: api.clone(),
                            prefix,
                            idempotency,
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
        }]
    };
    out
}

/// The problems in `app.jadpo`'s `apis:`.
pub fn api_problems(db: &dyn Db, project: Project) -> &[Diagnostic] {
    &api_table(db, project).problems
}

fn read_api(
    path: &str,
    api: &str,
    span: Span,
    list: Option<ast::MemberList>,
    out: &mut Vec<Diagnostic>,
) -> Option<(Vec<String>, Idempotency)> {
    let mut prefix = None;
    let mut idempotency = Idempotency::Optional;
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
            "authentication" | "rate_limit" => out.push(
                Diagnostic::new(
                    &rule::TOOLCHAIN_NOT_BUILT_YET,
                    format!("An API's `{setting}:` isn't built yet"),
                )
                .at(key_span, "decided, not built")
                .reason(match setting.as_str() {
                    "authentication" => "An API's authentication methods come with sign-in (area 8, roadmap chunk 4.3).",
                    _ => "An API's own `rate_limit:` comes with the rest of area 16's rate limits; every API already has the default limits.",
                })
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
                    "An API in `apis:` holds its `prefix:` and `idempotency:`, and later its `authentication:` and `rate_limit:`.{}",
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
    prefix.map(|prefix| (prefix, idempotency))
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
