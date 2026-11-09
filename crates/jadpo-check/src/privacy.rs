//! `application { privacy: { } }` (area 15, "What erasure doesn't rewrite"
//! and "Data requests"): `erasure_log: { store: }`, where the erasure log
//! is kept, a store that holds no person's records, or a store of files,
//! so restoring a person's database can't take the log back with them
//! (`data.erasure_log_required`, where `erase` is written); and the
//! generated routes for a person's requests. Anything under `privacy:`
//! but the log turns those routes on. `self_service:` adds the routes
//! anyone can use by email address, `path:` and `generate_routes:` move or
//! switch them off, `exports:` says where exports are kept, `erasures:`
//! how long an erasure waits, and `emails:` how each email is sent.

use std::sync::Arc;

use jadpo_db::{Db, Project};
use jadpo_diag::{Diagnostic, Span, nearest, rule};
use jadpo_std::http::PrincipalKind;
use jadpo_syntax::ast::{self, AstNode};

use crate::data::ConnectionKind;
use crate::identity::FirstSignIn;
use crate::tree::{self, name_text, node_span};
use crate::types::{Const, TypeView};

/// The section's name.
pub const PRIVACY: &str = "privacy";

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
    /// `exports:`, when it reads.
    pub exports: Option<Exports>,
    /// Whether it says `exports:` at all, read cleanly or not.
    pub exports_written: bool,
    /// `erasures: { cooling_off: }`, in milliseconds.
    pub cooling_off_ms: i64,
    /// `emails:`, each entry that says how its email is sent.
    pub emails: Vec<OwnMail>,
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

/// `exports:`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Exports {
    /// The store of files the encrypted files are kept in.
    pub store: String,
    /// How long a link and its file last, in milliseconds.
    pub keep_ms: i64,
    /// The frontend page a link opens.
    pub link: String,
}

/// One entry of `emails:`: how one email is sent, as written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnMail {
    /// `address_check`, `export_ready`, `erasure_confirmed` or
    /// `erasure_completed`.
    pub setting: String,
    /// `email:`, the application's own email (level 4).
    pub email: Option<FirstSignIn>,
    /// `send:`, the action that sends it itself (level 5).
    pub send: Option<FirstSignIn>,
}

impl Privacy {
    /// `emails:`'s entry for the email called `setting`, when it has one.
    pub fn mail(&self, setting: &str) -> Option<&OwnMail> {
        self.emails.iter().find(|mail| mail.setting == setting)
    }

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
        cooling_off_ms: jadpo_std::http::PRIVACY_COOLING_OFF_MS,
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
                    "exports" => {
                        out.on = true;
                        out.exports_written = true;
                        exports(db, project, path, &entry, &mut out);
                    }
                    "erasures" => {
                        out.on = true;
                        erasures(path, &entry, &mut out);
                    }
                    _ => {
                        out.problems.push(unknown(
                            at,
                            format!("`{name}` isn't a setting of `privacy:`"),
                            "`privacy:` says where the erasure log is kept and how a person makes a data request: `erasure_log:`, `self_service:`, `path:`, `generate_routes:`, `exports:`, `erasures:` and `emails:`.",
                            format!("Remove `{name}:`"),
                        ));
                    }
                }
            }
        }
    }
    Arc::new(out)
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

/// The name of each email `emails:` takes an entry for, with the built-in
/// types of each: what `email:` picks from and what `send:` is given.
const MAILS: [(&str, &str, &str); 4] = [
    (
        "address_check",
        jadpo_std::email::PRIVACY_ADDRESS_CHECK_EMAIL_TYPE,
        jadpo_std::email::PRIVACY_ADDRESS_CHECK_REQUEST,
    ),
    ("export_ready", "PrivacyExportEmail", "PrivacyExportRequest"),
    (
        "erasure_confirmed",
        "PrivacyErasureConfirmedEmail",
        "PrivacyErasureConfirmedRequest",
    ),
    (
        "erasure_completed",
        "PrivacyErasureCompletedEmail",
        "PrivacyErasureCompletedRequest",
    ),
];

/// `emails: { address_check: { } export_ready: { } ... }`, each entry
/// taking `email:` or `send:`, never both.
fn emails(path: &str, entry: &ast::Member, out: &mut Privacy) {
    for (setting, value) in tree::members(entry.block()) {
        let at = node_span(path, setting.syntax());
        let word = name_text(&setting).unwrap_or_default();
        if !MAILS.iter().any(|(name, _, _)| *name == word) {
            out.problems.push(unknown(
                at,
                format!("`{word}` isn't a privacy email"),
                "`emails:` holds `address_check:`, `export_ready:`, `erasure_confirmed:` and `erasure_completed:`.",
                format!("Remove `{word}:`"),
            ));
            continue;
        }
        let mut mail = OwnMail {
            setting: word.clone(),
            email: None,
            send: None,
        };
        for (own, written) in tree::members(value.block()) {
            let name = name_text(&own).unwrap_or_default();
            if name != "email" && name != "send" {
                out.problems.push(unknown(
                    node_span(path, own.syntax()),
                    format!("`{name}` isn't a setting of `{word}:`"),
                    "Each privacy email takes `email:`, the application's own email, or `send:`, an action of the application's that sends it.",
                    format!("Remove `{name}:`"),
                ));
                continue;
            }
            let span = written.value().map_or_else(
                || node_span(path, own.syntax()),
                |v| node_span(path, v.syntax()),
            );
            let names: Option<Vec<String>> = written.value().as_ref().and_then(|value| {
                tree::dotted(value).map(|(first, rest)| {
                    std::iter::once(first)
                        .chain(rest)
                        .filter_map(|name| tree::name_ref_text(&name))
                        .collect()
                })
            });
            let Some(names) = names.filter(|names: &Vec<String>| (1..=2).contains(&names.len()))
            else {
                let (what, form) = if name == "email" {
                    ("an email type", "<domain>.<Email>")
                } else {
                    ("an action", "<domain>.<action>")
                };
                out.problems.push(invalid(
                    span,
                    format!("`{name}:` must name {what}"),
                    "`email:` names an `EmailMessage` type of the application's, `send:` an action of the application's.",
                    &format!("Write `{form}`"),
                ));
                continue;
            };
            let named = FirstSignIn { names, span };
            if name == "email" {
                mail.email = Some(named);
            } else {
                mail.send = Some(named);
            }
        }
        if mail.email.is_some() && mail.send.is_some() {
            out.problems.push(invalid(
                at,
                format!("`{word}:` has both `email:` and `send:`"),
                "`email:` names an email of the application's that Jadpo fills and sends; `send:` names an action that sends it some other way. One email is sent one way.",
                "Keep `email:` or `send:`, not both",
            ));
            continue;
        }
        out.emails.push(mail);
    }
}

/// A duration written in place, from `least` to `most` milliseconds.
fn duration_of(entry: &ast::Member, least: i64, most: i64) -> Option<i64> {
    let written = entry
        .value()
        .as_ref()
        .map(|value| tree::written(value.syntax()))
        .unwrap_or_default();
    jadpo_std::Dur::parse(&written)
        .and_then(|dur| i64::try_from(dur.millis).ok())
        .filter(|millis| (least..=most).contains(millis))
}

/// `exports: { store: keep: link: }`: where a request's encrypted export
/// is kept, how long it and its link last, and the page the link opens.
fn exports(db: &dyn Db, project: Project, path: &str, entry: &ast::Member, out: &mut Privacy) {
    let mut found = Exports {
        store: String::new(),
        keep_ms: jadpo_std::http::PRIVACY_EXPORT_KEEP_MS,
        link: jadpo_std::http::PRIVACY_EXPORT_LINK.to_string(),
    };
    let mut named = false;
    let mut clean = true;
    for (setting, value) in tree::members(entry.block()) {
        let at = node_span(path, setting.syntax());
        let word = name_text(&setting).unwrap_or_default();
        match word.as_str() {
            "store" => {
                named = true;
                let written = value
                    .value()
                    .map(|value| tree::written(value.syntax()))
                    .unwrap_or_default();
                let connections = crate::data::connections(db, project);
                if connections.kind(&written) == Some(ConnectionKind::Files) {
                    found.store = written;
                } else {
                    clean = false;
                    let stores = connections.files();
                    let span = value
                        .value()
                        .map_or_else(|| at.clone(), |v| node_span(path, v.syntax()));
                    let mut problem = invalid(
                        span.clone(),
                        format!("`store: {written}` isn't a store of files"),
                        "`exports: { store: }` names the connection a person's encrypted export is kept in until its link ends: a store of files.",
                        "Name a connection whose `driver:` keeps files",
                    );
                    if let Some(near) = nearest(&written, stores.iter().map(String::as_str)) {
                        problem = problem
                            .next(format!("Use `{near}` instead of `{written}`"))
                            .edit(span, near);
                    }
                    out.problems.push(problem);
                }
            }
            "keep" => {
                match duration_of(
                    &value,
                    jadpo_std::http::PRIVACY_EXPORT_KEEP_MIN_MS,
                    jadpo_std::http::PRIVACY_EXPORT_KEEP_MAX_MS,
                ) {
                    Some(millis) => found.keep_ms = millis,
                    None => {
                        clean = false;
                        out.problems.push(invalid(
                            at,
                            "`keep:` must be a duration from `1d` to `30d`".to_string(),
                            "`keep:` is how long an export's link and its file last: long enough for a person to see the email, and no longer than the data needs to sit outside the application.",
                            "Write a duration such as `7d`",
                        ));
                    }
                }
            }
            "link" => {
                let text =
                    value
                        .value()
                        .as_ref()
                        .map(Const::from_expr)
                        .and_then(|given| match given {
                            Const::Str(text) if crate::identity::site_path(&text) => Some(text),
                            _ => None,
                        });
                match text {
                    Some(text) => found.link = text,
                    None => {
                        clean = false;
                        out.problems.push(invalid(
                            at,
                            "`link:` must be a page on your site, written as a path".to_string(),
                            "`link:` is the frontend page an export's link opens: a path starting with `/`, with no query.",
                            "Write a path such as \"/privacy/download\"",
                        ));
                    }
                }
            }
            _ => {
                clean = false;
                out.problems.push(unknown(
                    at,
                    format!("`{word}` isn't a setting of `exports:`"),
                    "`exports:` says where a person's exports are kept: `store:`, how long they last, `keep:`, and the page their link opens, `link:`.",
                    format!("Remove `{word}:`"),
                ));
            }
        }
    }
    if named && clean {
        out.exports = Some(found);
    } else if !named && clean {
        out.problems.push(invalid(
            node_span(path, entry.syntax()),
            "`exports:` doesn't say which store keeps them".to_string(),
            "A person's encrypted export is kept in a store of files until its link ends, so `exports:` names one.",
            "Write `exports: { store: <connection> }`",
        ));
    }
}

/// `erasures: { cooling_off: }`: how long an erasure waits to be cancelled.
fn erasures(path: &str, entry: &ast::Member, out: &mut Privacy) {
    for (setting, value) in tree::members(entry.block()) {
        let at = node_span(path, setting.syntax());
        let word = name_text(&setting).unwrap_or_default();
        if word != "cooling_off" {
            out.problems.push(unknown(
                at,
                format!("`{word}` isn't a setting of `erasures:`"),
                "`erasures:` says how long an erasure waits before it runs: `cooling_off:`.",
                format!("Remove `{word}:`"),
            ));
            continue;
        }
        match duration_of(
            &value,
            jadpo_std::http::PRIVACY_COOLING_OFF_MIN_MS,
            jadpo_std::http::PRIVACY_COOLING_OFF_MAX_MS,
        ) {
            Some(millis) => out.cooling_off_ms = millis,
            None => out.problems.push(invalid(
                at,
                "`cooling_off:` must be a duration from `1d` to `21d`".to_string(),
                "`cooling_off:` is how long a person has to cancel their erasure: at least a day to see the email, and at most 21 days, which leaves the providers a week of the shortest legal month to confirm.",
                "Write a duration such as `14d`",
            )),
        }
    }
}

/// How each email is sent, each `email:` and `send:` checked against the
/// built-in type it is filled from: the canonical name of the application's
/// own email and action for each entry that names one.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Mail {
    /// `email:`, by entry: the application's own email's canonical name.
    pub own: Vec<(String, String)>,
    /// `send:`, by entry: the action's canonical name.
    pub send: Vec<(String, String)>,
    pub problems: Vec<Diagnostic>,
}

impl Mail {
    /// The application's own email for entry `setting`.
    pub fn own_of(&self, setting: &str) -> Option<&str> {
        self.own
            .iter()
            .find(|(name, _)| name == setting)
            .map(|(_, email)| email.as_str())
    }

    /// The action that sends entry `setting`.
    pub fn send_of(&self, setting: &str) -> Option<&str> {
        self.send
            .iter()
            .find(|(name, _)| name == setting)
            .map(|(_, action)| action.as_str())
    }
}

/// Every `email:` and `send:` under `emails:`, checked.
#[salsa::tracked(returns(ref))]
pub fn mail(db: &dyn Db, project: Project) -> Mail {
    let found = privacy(db, project);
    let lookup = crate::lookups::Lookup::new(db, project);
    let mut out = Mail::default();
    for own in &found.emails {
        let Some((_, email_type, request_type)) =
            MAILS.iter().find(|(name, _, _)| *name == own.setting)
        else {
            continue;
        };
        if let Some(written) = &own.email {
            match crate::identity::own_email_named(
                &lookup,
                written,
                email_type,
                &rule::DATA_PRIVACY_SETTING_INVALID,
            ) {
                Ok(canonical) => out.own.push((own.setting.clone(), canonical)),
                Err(problem) => out.problems.push(*problem),
            }
        }
        if let Some(written) = &own.send {
            match crate::identity::send_action_named(
                &lookup,
                written,
                request_type,
                &rule::DATA_PRIVACY_SETTING_INVALID,
            ) {
                Ok(canonical) => out.send.push((own.setting.clone(), canonical)),
                Err(problem) => out.problems.push(*problem),
            }
        }
    }
    out
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
    let mail = mail(db, project);
    out.extend(mail.problems.iter().cloned());
    // What a request can't carry out in full, refused where it is turned
    // on, as `User.erase` and `User.export` are where they are written.
    if let Some(erasure) = crate::erasure::erasures(db, project)
        .people
        .get(&user.entity)
    {
        let mut unbuilt = |first: &str, what: &str| {
            out.push(
                Diagnostic::new(
                    &rule::TOOLCHAIN_NOT_BUILT_YET,
                    format!("A data request can't {what} a `{}` yet", user.entity),
                )
                .at(span.clone(), "decided, not built")
                .reason(format!(
                    "{first}, which a data request can't do in full yet: accepted, the person would be told it was done while data of theirs remained."
                ))
                .next("Remove `privacy:` for now, or take the field out of what the person's records reach"),
            );
        };
        if let Some(first) = erasure.unbuilt.first() {
            unbuilt(first, "erase");
        }
        if let Some(first) = erasure.export_unbuilt.first() {
            unbuilt(first, "export");
        }
    }
    if !found.log_written {
        out.push(
            Diagnostic::new(
                &rule::DATA_ERASURE_LOG_REQUIRED,
                "`privacy:` erases people, and the application names no erasure log",
            )
            .at(span.clone(), "no `erasure_log:`")
            .reason(
                "Restoring a backup would bring back everyone erased since it was taken. The log is what erases them again as the app starts, so it is kept where the restore can't take it back: a database that holds no person's records, or a store of files.",
            )
            .next("Add `erasure_log: { store: <connection> }` to `privacy:`, naming a connection for it"),
        );
    }
    if !found.exports_written {
        out.push(invalid(
            span.clone(),
            "`privacy:` doesn't say where exports are kept".to_string(),
            "A person's export is kept encrypted in a store of files until its link ends, so a `privacy:` that takes requests names one.",
            "Add `exports: { store: <connection> }` to `privacy:`, naming a store of files",
        ));
    }
    let settings = crate::emails::settings(db, project);
    let sends_mail = settings.connection.is_some() || !mail.send.is_empty();
    let asks_by_address = found.self_service.is_some() || !found.generate_routes;
    if asks_by_address || sends_mail {
        let address = address_field(identity, user);
        let field = address.and_then(|name| entity.fields.iter().find(|f| f.name == name));
        let lookup = crate::lookups::Lookup::new(db, project);
        let email = field.is_some_and(|field| {
            let ty = crate::types::Ty::Field(entity.name.clone(), vec![field.name.clone()]);
            lookup.base_prim(&ty) == Some(jadpo_std::Builtin::Email)
        });
        if !email {
            out.push(person_required(
                format!("`privacy:` can't reach `{}` by address", user.entity),
                "no address to reach them at".to_string(),
                "A data request finds its person by address and its emails go to one, the field `magic_link:` names or the user's own `subject:`, and neither is an `Email`.".to_string(),
                "Make the user's `subject:` an `Email` field",
            ));
        }
    }
    if identity
        .method(jadpo_std::AuthenticationMethod::BrowserSession)
        .and_then(|method| method.origin.as_ref())
        .is_none()
    {
        out.push(
            Diagnostic::new(
                &rule::SEMANTIC_AUTH_SETTING_REQUIRED,
                "`privacy:` needs a browser session's origin",
            )
            .at(span.clone(), "no origin for its links")
            .reason("The links a data request sends, to check an address, to cancel an erasure and to download an export, open your frontend's pages at the browser origin, which `browser_session: { origin: }` gives.")
            .next("Add `browser_session: { cookie: \"<name>\"  origin: config.<setting> }` to `authentication { }`"),
        );
    }
    if asks_by_address && settings.connection.is_none() && mail.send_of("address_check").is_none() {
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
    if settings.connection.is_none() && mail.send_of("export_ready").is_none() {
        let hears = crate::events::subscribers(db, project).iter().any(|model| {
            model.event.as_str()
                == jadpo_std::http::sign_in_event_key(jadpo_std::http::PRIVACY_EXPORT_READY)
                    .unwrap_or_default()
                && model.package.is_none()
        });
        if !hears {
            out.push(
                Diagnostic::new(
                    &rule::EMAIL_CONNECTION_REQUIRED,
                    "Nothing delivers a person's export",
                )
                .at(span.clone(), "no email connection and no subscriber")
                .reason("An export is delivered as a link, and with no email connection no email carries it, so a subscriber must deliver `PrivacyExportReady` or the person never receives what they asked for.")
                .next("Add a `subscriber` on `PrivacyExportReady`, or declare an `email:` connection in `app.jadpo`"),
            );
        }
    }
    if settings.connection.is_some() && settings.display_name.is_none() {
        out.push(
            Diagnostic::new(
                &rule::SEMANTIC_AUTH_SETTING_REQUIRED,
                "`privacy:` needs the application's `display_name:`",
            )
            .at(span, "no `display_name:`")
            .reason("Jadpo's privacy emails name the application by `application { display_name: }`, the name their readers know it by.")
            .next("Add `display_name: \"<name>\"` to `application { }` in `app.jadpo`"),
        );
    }
    out
}

/// Each entity a data request's erasure keeps because a field or reference
/// says `Erasure.retained`, by name: the entities whose domain words them
/// for the completion email.
#[salsa::tracked(returns(ref))]
pub fn kept_entities(db: &dyn Db, project: Project) -> Vec<String> {
    if !privacy(db, project).takes_requests() {
        return Vec::new();
    }
    let identity = crate::identity::identity(db, project);
    let Some(user) = identity.variant(PrincipalKind::User) else {
        return Vec::new();
    };
    crate::erasure::retained_entities(db, project, &user.entity)
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
