//! `application { privacy: { } }` (area 15, "What erasure doesn't rewrite"
//! and "Data requests"): `erasure_log: { store: }`, where the erasure log
//! is kept, a store that holds no person's records, or a store of files,
//! so restoring a person's database can't take the log back with them
//! (`data.erasure_log_required`, where `erase` is written); and the
//! generated routes for a person's requests. Anything under `privacy:`
//! but the log turns those routes on. `self_service:` adds the routes
//! anyone can use by email address, `path:` and `generate_routes:` move or
//! switch them off, and `emails:` is built only as far as the address
//! check's levels; the rest of it, and `exports:`, are
//! `toolchain.not_built_yet` where they are written.

use std::sync::Arc;

use jadpo_db::{Db, Project};
use jadpo_diag::{Diagnostic, Span, nearest, rule};
use jadpo_std::http::PrincipalKind;
use jadpo_syntax::ast::{self, AstNode};

use crate::data::ConnectionKind;
use crate::tree::{self, name_text, node_span};
use crate::types::{Const, TypeView};

/// The section's name.
pub const PRIVACY: &str = "privacy";
/// Why the rest of it isn't accepted.
const NOT_BUILT: &str = "Delivering an export and carrying out an erasure aren't built yet: accepted, nothing would deliver what this setting describes.";

/// `self_service:`: the frontend pages the emails link to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelfService {
    pub link: String,
    pub cancel_link: String,
}

/// `privacy:`, read.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Privacy {
    /// The connection `erasure_log:` names, when it can keep the log, and
    /// what it is.
    pub log: Option<(String, ConnectionKind)>,
    /// Whether it says `erasure_log:` at all, read cleanly or not, so an
    /// erasure isn't told to add what is reported where it is written.
    pub log_written: bool,
    /// Whether anything under `privacy:` but the log is written, so its
    /// generated routes are on.
    pub on: bool,
    /// `self_service:`, when written.
    pub self_service: Option<SelfService>,
    /// `path:`'s segments, `/privacy` unless it says.
    pub path: Vec<String>,
    /// `generate_routes:`: `false` generates no routes, and the
    /// application writes its own over `privacy.request`, `.confirm` and
    /// `.cancel`.
    pub generate_routes: bool,
    /// Where `privacy:` is written, for what it needs.
    pub span: Option<Span>,
    pub problems: Vec<Diagnostic>,
}

impl Privacy {
    /// Whether its data requests are on: routes or operations.
    pub fn takes_requests(&self) -> bool {
        self.on
    }
    /// Whether the runtime answers the routes, rather than the
    /// application's own over the operations.
    pub fn routes(&self) -> bool {
        self.on && self.generate_routes
    }
}

/// `privacy:` in `application { }`.
#[salsa::tracked(returns(ref))]
pub fn privacy(db: &dyn Db, project: Project) -> Arc<Privacy> {
    let mut out = Privacy {
        path: vec![jadpo_std::http::PRIVACY_PATH.to_string()],
        generate_routes: true,
        ..Privacy::default()
    };
    let Some(file) = project
        .files(db)
        .iter()
        .find(|file| file.path(db) == jadpo_db::PROJECT_FILE)
    else {
        return Arc::new(out);
    };
    let Some(source) = tree::source(db, *file) else {
        return Arc::new(out);
    };
    let path = file.path(db);
    for decl in source.decls() {
        let ast::Decl::SingletonDecl(singleton) = decl else {
            continue;
        };
        if singleton.keyword().map(|t| t.text().to_string()).as_deref() != Some("application") {
            continue;
        }
        for (key, member) in tree::members(singleton.member_list()) {
            if name_text(&key).as_deref() != Some(PRIVACY) {
                continue;
            }
            out.span = Some(node_span(path, key.syntax()));
            for (setting, entry) in tree::members(member.block()) {
                let at = node_span(path, setting.syntax());
                let name = name_text(&setting).unwrap_or_default();
                match name.as_str() {
                    "erasure_log" => {
                        out.log_written = true;
                        erasure_log(db, project, path, at, &entry, &mut out);
                    }
                    "self_service" => {
                        out.on = true;
                        self_service(path, &entry, &mut out);
                    }
                    "path" => {
                        out.on = true;
                        written_path(path, at, &entry, &mut out);
                    }
                    "generate_routes" => {
                        out.on = true;
                        generate_routes(path, at, &entry, &mut out);
                    }
                    "emails" => {
                        out.on = true;
                        emails(path, &entry, &mut out);
                    }
                    // Named on the page, delivered with the exports.
                    "exports" => {
                        out.on = true;
                        out.problems.push(not_built(at, &name));
                    }
                    _ => {
                        out.problems.push(unknown(
                            at,
                            format!("`{name}` isn't a setting of `privacy:`"),
                            "`privacy:` says where the erasure log is kept and how a person makes a data request: `erasure_log:`, `self_service:`, `path:`, `generate_routes:`, `emails:` and `exports:`.",
                            format!("Remove `{name}:`"),
                        ));
                    }
                }
            }
        }
    }
    Arc::new(out)
}

fn not_built(at: Span, name: &str) -> Diagnostic {
    Diagnostic::new(
        &rule::TOOLCHAIN_NOT_BUILT_YET,
        format!("`privacy: {{ {name}: }}` isn't built yet"),
    )
    .at(at, "decided, not built")
    .reason(NOT_BUILT)
    .next(format!("Remove `{name}:` for now"))
}

fn unknown(at: Span, title: String, reason: &str, next: String) -> Diagnostic {
    Diagnostic::new(&rule::DATA_PRIVACY_SETTING_INVALID, title)
        .at(at, "not a setting")
        .reason(reason.to_string())
        .next(next)
}

fn invalid(at: Span, title: String, reason: &str, next: &str) -> Diagnostic {
    Diagnostic::new(&rule::DATA_PRIVACY_SETTING_INVALID, title)
        .at(at, "not valid here")
        .reason(reason.to_string())
        .next(next.to_string())
}

/// `self_service: { link: cancel_link: }`, each a frontend page, `/privacy/
/// confirm` and `/privacy/cancel` unless they say.
fn self_service(path: &str, entry: &ast::Member, out: &mut Privacy) {
    let mut found = SelfService {
        link: jadpo_std::http::PRIVACY_LINK.to_string(),
        cancel_link: jadpo_std::http::PRIVACY_CANCEL_LINK.to_string(),
    };
    for (setting, value) in tree::members(entry.block()) {
        let at = node_span(path, setting.syntax());
        let word = name_text(&setting).unwrap_or_default();
        if word != "link" && word != "cancel_link" {
            out.problems.push(unknown(
                at,
                format!("`{word}` isn't a setting of `self_service:`"),
                "`self_service:` names the frontend's pages its links open: `link:` and `cancel_link:`, each a path on the site.",
                format!("Remove `{word}:`"),
            ));
            continue;
        }
        let text = value
            .value()
            .as_ref()
            .map(Const::from_expr)
            .and_then(|given| match given {
                Const::Str(text) if crate::identity::site_path(&text) => Some(text),
                _ => None,
            });
        match text {
            Some(text) if word == "link" => found.link = text,
            Some(text) => found.cancel_link = text,
            None => out.problems.push(invalid(
                at,
                format!("`{word}:` must be a page on your site, written as a path"),
                "`link:` and `cancel_link:` are frontend pages at the browser origin: a path starting with `/`, with no query.",
                "Write a path such as \"/privacy/confirm\"",
            )),
        }
    }
    out.self_service = Some(found);
}

/// `path:`, where the routes go.
fn written_path(_path: &str, at: Span, entry: &ast::Member, out: &mut Privacy) {
    let segments = match entry.value().as_ref().map(Const::from_expr) {
        Some(Const::Str(text)) => crate::identity::fixed_path(&text),
        _ => None,
    };
    match segments {
        Some(segments) => out.path = segments,
        None => out.problems.push(invalid(
            at,
            "`path:` must be a path of fixed segments".to_string(),
            "`path:` says where the generated routes go: `/` and segments of lower-case letters, digits, `-` and `_`.",
            "Write a path such as \"/privacy\"",
        )),
    }
}

/// `generate_routes: false`, the one spelling of switching off generated
/// routes.
fn generate_routes(_path: &str, at: Span, entry: &ast::Member, out: &mut Privacy) {
    match entry.value().as_ref().map(Const::from_expr) {
        Some(Const::Bool(on)) => out.generate_routes = on,
        _ => out.problems.push(invalid(
            at,
            "`generate_routes:` must be `true` or `false`".to_string(),
            "`generate_routes: false` leaves the application to write its own routes over `privacy.request`, `privacy.confirm` and `privacy.cancel`.",
            "Write `generate_routes: false`, or leave it out",
        )),
    }
}

/// `emails: { address_check: { } }`: the others are delivered with the
/// exports and erasures they tell a person about.
fn emails(path: &str, entry: &ast::Member, out: &mut Privacy) {
    const OTHERS: [&str; 3] = ["export_ready", "erasure_confirmed", "erasure_completed"];
    for (setting, value) in tree::members(entry.block()) {
        let at = node_span(path, setting.syntax());
        let word = name_text(&setting).unwrap_or_default();
        if word == "address_check" {
            if let Some((own, _)) = tree::members(value.block()).first() {
                let word = name_text(own).unwrap_or_default();
                out.problems.push(not_built(
                    node_span(path, own.syntax()),
                    &format!("emails: {{ address_check: {{ {word}"),
                ));
            }
        } else if OTHERS.contains(&word.as_str()) {
            out.problems
                .push(not_built(at, &format!("emails: {{ {word}")));
        } else {
            out.problems.push(unknown(
                at,
                format!("`{word}` isn't a privacy email"),
                "`emails:` holds `address_check:`, `export_ready:`, `erasure_confirmed:` and `erasure_completed:`.",
                format!("Remove `{word}:`"),
            ));
        }
    }
}

/// What `privacy:`'s data requests need from the rest of the application
/// (area 15, "Data requests"): a person the routes can find, and for the
/// emails an origin, a connection and a name.
#[salsa::tracked(returns(ref))]
pub fn request_problems(db: &dyn Db, project: Project) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    let found = privacy(db, project);
    if !found.on {
        return out;
    }
    let span = found
        .span
        .clone()
        .unwrap_or_else(|| Span::new(String::new(), 0, 0));
    let identity = crate::identity::identity(db, project);
    let model = crate::data::data_model(db, project);
    let person_required = |title: String, label: String, reason: String, next: &str| {
        Diagnostic::new(&rule::DATA_PRIVACY_PERSON_REQUIRED, title)
            .at(span.clone(), label)
            .reason(reason)
            .next(next.to_string())
    };
    let Some(user) = identity.variant(PrincipalKind::User) else {
        out.push(person_required(
            "`privacy:` needs a `user` who is a person".to_string(),
            "no user to act for".to_string(),
            "A data request acts on one person's own records, by who is signed in or by their email address, so `principal { }` must declare who a `user` is, on an entity marked `person: true`.".to_string(),
            "Declare `principal { user: { subject: <entity>.<field> } }` on the entity marked `person: true`",
        ));
        return out;
    };
    let Some(entity) = model.entity(&user.entity) else {
        return out;
    };
    if !entity.person {
        out.push(person_required(
            format!("`{}` signs in, but isn't a person", user.entity),
            "not `person: true`".to_string(),
            format!("A data request exports or erases one person's records, and `{}`, which callers sign in as, isn't marked `person: true`, so nothing says whose records they are.", user.entity),
            "Add `person: true` to the entity",
        ));
    }
    if found.self_service.is_some() || !found.generate_routes {
        let address = address_field(identity, user);
        let field = address.and_then(|name| entity.fields.iter().find(|f| f.name == name));
        let lookup = crate::lookups::Lookup::new(db, project);
        let email = field.is_some_and(|field| {
            let ty = crate::types::Ty::Field(entity.name.clone(), vec![field.name.clone()]);
            lookup.base_prim(&ty) == Some(jadpo_std::Builtin::Email)
        });
        if !email {
            out.push(person_required(
                format!("`privacy: {{ self_service: }}` can't find `{}` by address", user.entity),
                "no address to find them by".to_string(),
                "A request by email finds its person by address, the field `magic_link:` names or the user's own `subject:`, and neither is an `Email`.".to_string(),
                "Make the user's `subject:` an `Email` field",
            ));
        }
        if identity
            .method(jadpo_std::AuthenticationMethod::BrowserSession)
            .and_then(|method| method.origin.as_ref())
            .is_none()
        {
            out.push(
                Diagnostic::new(
                    &rule::SEMANTIC_AUTH_SETTING_REQUIRED,
                    "`self_service:` needs a browser session's origin",
                )
                .at(span.clone(), "no origin for its links")
                .reason("The address-check and cancel emails' links open your frontend's pages at the browser origin, which `browser_session: { origin: }` gives.")
                .next("Add `browser_session: { cookie: \"<name>\"  origin: config.<setting> }` to `authentication { }`"),
            );
        }
        let settings = crate::emails::settings(db, project);
        if settings.connection.is_none() {
            out.push(
                Diagnostic::new(
                    &rule::EMAIL_CONNECTION_REQUIRED,
                    "`self_service:` sends email, but nothing sends email",
                )
                .at(span.clone(), "no email connection")
                .reason("The address check is an email, sent through the connection `application { email: }` names, from the address its `from:` gives; `app.jadpo` names none.")
                .next("Declare a connection with `driver: Driver.smtp` in `connections:`, and add `email: { connection: <its name>  from: config.mail_sender }` to `application { }` in `app.jadpo`"),
            );
        }
        if settings.display_name.is_none() {
            out.push(
                Diagnostic::new(
                    &rule::SEMANTIC_AUTH_SETTING_REQUIRED,
                    "`self_service:` needs the application's `display_name:`",
                )
                .at(span, "no `display_name:`")
                .reason("Jadpo's address-check email names the application by `application { display_name: }`, the name its readers know it by.")
                .next("Add `display_name: \"<name>\"` to `application { }` in `app.jadpo`"),
            );
        }
    }
    out
}

/// The field a request by email finds its person by: `magic_link:`'s
/// `address:`, or else the user's own `subject:`.
pub fn address_field<'a>(
    identity: &'a crate::identity::Identity,
    user: &'a crate::identity::Variant,
) -> Option<&'a str> {
    identity
        .magic_links()
        .map(|magic| magic.address.as_str())
        .or(Some(user.subject.as_str()))
}

/// Whether `privacy.create_request` has a line under `actions:`.
pub fn granted(db: &dyn Db, project: Project) -> bool {
    crate::policy::Policies::new(db, project)
        .root()
        .action(jadpo_std::http::PRIVACY_CREATE_REQUEST)
        .is_some()
}

/// `erasure_log: { store: }`.
fn erasure_log(
    db: &dyn Db,
    project: Project,
    path: &str,
    at: Span,
    entry: &ast::Member,
    out: &mut Privacy,
) {
    let invalid = |title: String, at: Span, label: &str, next: String| {
        Diagnostic::new(&rule::DATA_ERASURE_LOG_STORE, title)
            .at(at, label.to_string())
            .reason("`erasure_log: { store: <connection> }` names where the log of everyone erased is kept: a database that holds no person's records, or a store of files.")
            .next(next)
    };
    let mut named = None;
    for (setting, value) in tree::members(entry.block()) {
        let word = name_text(&setting).unwrap_or_default();
        if word == "store" {
            named = Some(value);
        } else {
            out.problems.push(invalid(
                format!("`{word}` isn't a setting of `erasure_log:`"),
                node_span(path, setting.syntax()),
                "not a setting",
                format!("Remove `{word}:`"),
            ));
        }
    }
    let Some(value) = named else {
        out.problems.push(invalid(
            "`erasure_log:` doesn't say which store keeps it".to_string(),
            at,
            "no `store:`",
            "Write `erasure_log: { store: <connection> }`".to_string(),
        ));
        return;
    };
    let written = value
        .value()
        .map(|value| tree::written(value.syntax()))
        .unwrap_or_default();
    let span = value.value().map_or_else(
        || node_span(path, value.syntax()),
        |v| node_span(path, v.syntax()),
    );
    let connections = crate::data::connections(db, project);
    let kind = connections.kind(&written);
    if !matches!(kind, Some(ConnectionKind::Postgres | ConnectionKind::Files)) {
        let mut stores = connections.databases();
        stores.extend(connections.files());
        let mut problem = Diagnostic::new(
            &rule::DATA_ERASURE_LOG_STORE,
            format!("`store: {written}` is neither a database nor a store of files"),
        )
        .at(span.clone(), "can't keep the log")
        .reason(format!(
            "The erasure log must outlast a restore of a person's database, so it is kept in a database of its own or a store of files; this application has {}.",
            if stores.is_empty() {
                "none yet".to_string()
            } else {
                crate::and_list(&stores.iter().map(|s| format!("`{s}`")).collect::<Vec<_>>())
            }
        ));
        problem = match nearest(&written, stores.iter().map(String::as_str)) {
            Some(near) => problem
                .next(format!("Use `{near}` instead of `{written}`"))
                .edit(span, near),
            None => problem.next("Add a connection for it, and name that"),
        };
        out.problems.push(problem);
        return;
    }
    if kind == Some(ConnectionKind::Postgres) {
        let holding = holds_a_persons_records(db, project, &written);
        if let Some(entity) = holding {
            out.problems.push(
                Diagnostic::new(
                    &rule::DATA_ERASURE_LOG_STORE,
                    format!("`{written}` holds a person's records, so a restore would take the log back with them"),
                )
                .at(span, format!("`{entity}` is kept here"))
                .reason(format!(
                    "The log is what erases someone again after a backup brings them back. Restoring `{written}` would bring back their `{entity}` records and lose every entry written since the backup, so nobody erased since would be erased again."
                ))
                .next("Add a database that holds no person's records and name it, or name a store of files"),
            );
            return;
        }
    }
    out.log = kind.map(|kind| (written, kind));
}

/// The first entity, by name, in database `store` whose records erasing a
/// person reaches, or who is a person.
fn holds_a_persons_records(db: &dyn Db, project: Project, store: &str) -> Option<String> {
    let erasures = crate::erasure::erasures(db, project);
    crate::data::data_model(db, project)
        .entities
        .iter()
        .find(|entity| {
            entity.store.as_deref() == Some(store) && (entity.person || erasures.reaches(entity))
        })
        .map(|entity| entity.name.clone())
}
