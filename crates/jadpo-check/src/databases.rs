//! Each connection with `driver: Driver.postgres` and how it is secured
//! (area 5, "PostgreSQL over TLS"; chunk 8.1n): TLS always, checked against
//! Mozilla's roots compiled in, but to this machine; `authority:` a
//! `Certificate` setting its server's private authority is in; `tls:
//! false` a server elsewhere that offers no TLS, which the network report
//! lists. Its address, `url:`, is area 10's ([`crate::config`]).

use jadpo_db::{Db, Project};
use jadpo_diag::{Decider, Diagnostic, Span, rule};
use jadpo_syntax::ast::{self, AstNode};

use crate::config::{config, singletons};
use crate::data::ConnectionKind;
use crate::lookups::Lookup;
use crate::tree::{self, name_text, node_span};
use crate::types::{Const, Ty, TypeView};

/// How a database connection is secured.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum DatabaseTls {
    /// TLS checked against Mozilla's roots, but to this machine.
    #[default]
    Roots,
    /// TLS checked against the authority in this `Certificate` setting.
    Authority(String),
    /// `tls: false`: no TLS anywhere.
    Off,
}

/// A connection with `driver: Driver.postgres`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct DatabaseConnection {
    pub name: String,
    pub tls: DatabaseTls,
}

/// The database connections, in the order `app.jadpo` declares them, and
/// the problems with their settings.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DatabaseConnections {
    pub list: Vec<DatabaseConnection>,
    pub problems: Vec<Diagnostic>,
}

impl DatabaseConnections {
    /// How `connection` is secured: TLS against Mozilla's roots for one
    /// that says nothing, the implied `main` among them.
    pub fn tls_of(&self, connection: &str) -> DatabaseTls {
        self.list
            .iter()
            .find(|database| database.name == connection)
            .map(|database| database.tls.clone())
            .unwrap_or_default()
    }
}

/// `config.<name>` as the name it reads.
fn reads(value: &ast::Expr) -> Option<String> {
    let (first, rest) = tree::dotted(value)?;
    if tree::name_ref_text(&first).as_deref() != Some("config") || rest.len() != 1 {
        return None;
    }
    rest.first().and_then(tree::name_ref_text)
}

/// Each database connection's settings but its address.
#[salsa::tracked(returns(ref))]
pub fn database_connections(db: &dyn Db, project: Project) -> DatabaseConnections {
    let mut out = DatabaseConnections::default();
    let lookup = Lookup::new(db, project);
    for found in singletons(db, project, "application") {
        let path = found.path.clone();
        for (key, member) in tree::members(found.list.clone()) {
            if name_text(&key).as_deref() != Some("connections") {
                continue;
            }
            for (name, entry) in tree::members(member.block()) {
                let Some(connection) = name_text(&name) else {
                    continue;
                };
                if crate::data::connection_kind(entry.block()) != ConnectionKind::Postgres {
                    continue;
                }
                let mut authority: Option<(String, Span)> = None;
                let mut off: Option<Span> = None;
                for (setting, option) in tree::members(entry.block()) {
                    let word = name_text(&setting).unwrap_or_default();
                    let span = option.value().map_or_else(
                        || node_span(&path, option.syntax()),
                        |value| node_span(&path, value.syntax()),
                    );
                    match word.as_str() {
                        "driver" | "url" => {}
                        "authority" => {
                            let value = option.value();
                            let Some(field) = value.as_ref().and_then(reads) else {
                                out.problems.push(authority_invalid(
                                    &connection,
                                    span,
                                    "written in source",
                                    "Write `authority: config.database_authority`, declaring `database_authority: Certificate` in `config.jadpo`".to_string(),
                                ));
                                continue;
                            };
                            match config(db, project).setting(&field) {
                                Some(found) if is_certificate(&lookup, &found.ty) => {
                                    authority = Some((field, node_span(&path, setting.syntax())));
                                }
                                Some(found) => out.problems.push(authority_invalid(
                                    &connection,
                                    span,
                                    &format!("`{field}` is `{}`", found.ty),
                                    format!("Declare `{field}: Certificate` in `config.jadpo`"),
                                )),
                                // An unknown setting is reported where it is
                                // read.
                                None => {}
                            }
                        }
                        "tls" => match option.value().as_ref().map(Const::from_expr) {
                            Some(Const::Bool(false)) => off = Some(node_span(&path, setting.syntax())),
                            written => {
                                let default = written == Some(Const::Bool(true));
                                out.problems.push(
                                    Diagnostic::new(
                                        &rule::CONNECTION_VALUE_INVALID,
                                        format!("`{connection}`'s `tls:` is written only as `false`"),
                                    )
                                    .at(span, if default { "the default already" } else { "not `false`" })
                                    .reason("A database connection uses TLS unless it says `tls: false`, for a server elsewhere that offers none; it is decided in `app.jadpo`, where a review sees it, never by a setting.")
                                    .next_for(
                                        if default { "Remove `tls: true`" } else { "Write `tls: false`, or remove `tls:`" },
                                        if default { Decider::Agent } else { Decider::Human },
                                    ),
                                );
                            }
                        },
                        other => out.problems.push(
                            Diagnostic::new(
                                &rule::CONNECTION_KEY_INVALID,
                                format!("A connection with `driver: Driver.postgres` takes no `{other}:`"),
                            )
                            .at(node_span(&path, setting.syntax()), "not a database setting")
                            .reason("A connection to a PostgreSQL database gives `url:`, a `PostgresUrl` setting, and may give `authority:`, a `Certificate` setting its server's certificate is checked against, or `tls: false` for a server elsewhere that offers no TLS.")
                            .next(format!("Remove `{other}:`")),
                        ),
                    }
                }
                let tls = match (authority, off) {
                    (Some((_, at)), Some(_)) => {
                        out.problems.push(
                            Diagnostic::new(
                                &rule::CONNECTION_KEY_INVALID,
                                format!("`{connection}` names an authority and goes without TLS"),
                            )
                            .at(at, "never checked")
                            .reason("`authority:` is what the server's certificate is checked against, and a connection with `tls: false` never asks for one.")
                            .next_for("Remove `authority:`, or `tls: false`", Decider::Human),
                        );
                        DatabaseTls::Off
                    }
                    (Some((field, _)), None) => DatabaseTls::Authority(field),
                    (None, Some(_)) => DatabaseTls::Off,
                    (None, None) => DatabaseTls::Roots,
                };
                out.list.push(DatabaseConnection {
                    name: connection,
                    tls,
                });
            }
        }
    }
    out
}

fn authority_invalid(connection: &str, span: Span, label: &str, next: String) -> Diagnostic {
    Diagnostic::new(
        &rule::CONNECTION_VALUE_INVALID,
        format!("`{connection}`'s `authority:` must be a `Certificate` setting"),
    )
    .at(span, label.to_string())
    .reason("`authority:` is the authority its server's certificate is checked against, as PEM: a `Certificate` setting, `config.<name>`, read from each deployment's environment.")
    .next(next)
}

/// Whether `ty` is the standard `Certificate` (area 19), read from its PEM.
fn is_certificate(lookup: &Lookup<'_>, ty: &Ty) -> bool {
    matches!(
        lookup.structure(ty),
        Ty::Opaque(name, args)
            if args.is_empty() && name == jadpo_std::standard::certificates::CERTIFICATE
    )
}
