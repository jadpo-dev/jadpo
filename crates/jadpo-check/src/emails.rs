//! Email (area 16, "Email"; chunk 7.3): an email is a `type X =
//! EmailMessage { ... }` in a domain's `emails/` folder, its words in
//! `<name>.html` and `<name>.txt` beside it, sent by emitting it through
//! the connection `application { email: }` names.
//!
//! - [`settings`]: `application { email: { connection: from: } }` and
//!   `display_name:`.
//! - [`smtp_connections`]: each connection with `driver: Driver.smtp`, its
//!   `host:`, `port:` and `credential: { username: password: }` (area 11's
//!   settings for the driver, chosen in chunk 7.3).
//! - [`analysis`]: every email with its templates, the shared layout in
//!   `shared/emails/`, and a template with no email or an email with
//!   none.
//! - [`template_host`]: what a template is checked against, which
//!   [`crate::types::template_body`] reads: the email's fields, or the
//!   layout's `content`, `subject` and `locale`.
//! - [`email_problems`]: all of the above, each template's own, a field
//!   nothing writes (declinable), and an email emitted with no
//!   connection to send it.

use std::collections::BTreeSet;

use jadpo_db::{Base, Db, ItemKind, Ownership, Project, Symbol, symbols};
use jadpo_diag::{Diagnostic, Span, rule};
use jadpo_graph::Key;
use jadpo_syntax::ast::{self, AstNode, Decl};

use crate::case::to_snake;
use crate::config::{config, singletons};
use crate::data::ConnectionKind;
use crate::lookups::{Lookup, Name, file_at};
use crate::tree::{self, name_text, node_span};
use crate::types::{Def, Prim, Ty, TypeRef, TypeView};

/// The shared layout's files (area 16, "Level 3: branding").
pub const LAYOUT_HTML: &str = "shared/emails/layout.html";
pub const LAYOUT_TEXT: &str = "shared/emails/layout.txt";

/// The keys `email:` takes.
const EMAIL_KEYS: [&str; 2] = ["connection", "from"];

/// The keys a connection with `driver: Driver.smtp` takes.
const SMTP_KEYS: [&str; 4] = ["driver", "host", "port", "credential"];

/// Where `email: { from: }` comes from.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Sender {
    /// `config.<name>`, an `Email` setting.
    Setting(String),
    /// An address written out.
    Address(String),
}

/// `application { email: display_name: }`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Settings {
    /// The connection `email: { connection: }` names, when it sends email.
    pub connection: Option<String>,
    pub from: Option<Sender>,
    /// `display_name:`, the application's name as people read it.
    pub display_name: Option<String>,
    /// Whether `email:` is written at all.
    pub written: bool,
    pub problems: Vec<Diagnostic>,
}

/// Where an SMTP server's name comes from.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SmtpHost {
    Setting(String),
    Written(String),
}

/// A connection with `driver: Driver.smtp`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Smtp {
    pub name: String,
    pub host: Option<SmtpHost>,
    pub port: u16,
    /// The settings `credential: { username: password: }` reads.
    pub username: Option<String>,
    pub password: Option<String>,
}

/// Every SMTP connection, and what is wrong with them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SmtpConnections {
    pub list: Vec<Smtp>,
    pub problems: Vec<Diagnostic>,
}

/// One email type and its templates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EmailFacts {
    /// The name the checker's tables know it by.
    pub canonical: String,
    /// As written in its file.
    pub type_name: String,
    /// The `.jadpo` file declaring it.
    pub path: String,
    pub name_span: Span,
    pub ownership: Ownership,
    /// Its subject's key in its domain's language files: `welcome_email`.
    pub subject: String,
    /// Its templates that exist: `<name>.html` and `<name>.txt`.
    pub html: Option<String>,
    pub text: Option<String>,
    /// Its fields, `to` and `locale` first, with their types and where
    /// each is written (its type's name for the two every email has).
    pub fields: Vec<(String, Ty, Span)>,
    /// Its `File` fields, each an attachment.
    pub attachments: Vec<String>,
}

impl EmailFacts {
    /// The scope its subject is worded in: its domain, or `shared`.
    pub fn scope(&self) -> Option<String> {
        crate::translations::scope_of(&self.ownership)
    }

    /// Its templates, the HTML first.
    pub fn templates(&self) -> impl Iterator<Item = &String> {
        self.html.iter().chain(&self.text)
    }
}

/// Every email and the shared layout.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Analysis {
    pub emails: Vec<EmailFacts>,
    /// The layout's files that exist.
    pub layout_html: Option<String>,
    pub layout_text: Option<String>,
    pub problems: Vec<Diagnostic>,
}

impl Analysis {
    /// The email whose template is at `path`.
    pub fn email_of(&self, path: &str) -> Option<&EmailFacts> {
        self.emails
            .iter()
            .find(|email| email.templates().any(|template| template == path))
    }
}

/// What a template is written for (area 16).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Host {
    /// An email's `.html` or `.txt`, by the name the tables know its type.
    Email(String),
    /// The shared layout.
    Layout,
}

/// What one template is checked against.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TemplateHost {
    pub host: Host,
    /// Whether it is HTML, whose values are escaped, rather than text.
    pub html: bool,
    /// The names it may read, in order: what its callable is given.
    pub names: Vec<(String, Ty, Span)>,
    /// Who owns it, for what it may name.
    pub ownership: Ownership,
    /// How its callable is named: `email accounts.WelcomeEmail html`.
    pub label: String,
}

/// Whether a symbol is an email type the application sends.
fn is_email(symbol: &Symbol) -> bool {
    symbol.kind == ItemKind::Type(Base::EmailMessage)
        && !symbol.test
        && symbol.library.is_none()
        && symbol.provided.is_none()
        && symbol.ownership.package().is_none()
}

/// The folder a path is in, with its `/`, or nothing at the root.
fn folder_of(path: &str) -> &str {
    path.rfind('/')
        .and_then(|at| path.get(..=at))
        .unwrap_or_default()
}

/// The `application { }` block's members, with the file they are in.
fn application(db: &dyn Db, project: Project) -> Vec<(String, ast::Name, ast::Member)> {
    singletons(db, project, "application")
        .into_iter()
        .flat_map(|found| {
            tree::members(found.list.clone())
                .into_iter()
                .map(move |(key, member)| (found.path.clone(), key, member))
        })
        .collect()
}

fn settings_invalid(title: impl Into<String>, span: Span, label: &str) -> Diagnostic {
    Diagnostic::new(&rule::EMAIL_SETTINGS_INVALID, title).at(span, label.to_string())
}

/// `config.<name>` as the name it reads.
fn reads(value: &ast::Expr) -> Option<String> {
    let (first, rest) = tree::dotted(value)?;
    if tree::name_ref_text(&first).as_deref() != Some("config") || rest.len() != 1 {
        return None;
    }
    tree::name_ref_text(&rest[0])
}

/// The text a literal is, `"..."`.
fn literal_text(value: &ast::Expr) -> Option<String> {
    match crate::types::Const::from_expr(value) {
        crate::types::Const::Str(text) => Some(text),
        _ => None,
    }
}

/// `application { email: display_name: }`.
#[salsa::tracked(returns(ref))]
pub fn settings(db: &dyn Db, project: Project) -> Settings {
    let mut out = Settings::default();
    let connections = crate::data::connections(db, project);
    let lookup = Lookup::new(db, project);
    for (path, key, member) in application(db, project) {
        match name_text(&key).as_deref() {
            Some("display_name") => {
                let span = member.value().map_or_else(
                    || node_span(&path, member.syntax()),
                    |value| node_span(&path, value.syntax()),
                );
                match member.value().as_ref().and_then(literal_text) {
                    Some(name) if !name.trim().is_empty() && !name.contains(['\n', '\r']) => {
                        out.display_name = Some(name);
                    }
                    _ => out.problems.push(
                        settings_invalid("`display_name:` must be the application's name, written out", span, "not a name")
                            .reason("`display_name:` is the application's name as people read it, in emails' layouts and Jadpo's own wording: plain text on one line, written in `app.jadpo`.")
                            .next("Write `display_name: \"Acme\"`, the name people know the application by"),
                    ),
                }
            }
            Some("email") => {
                out.written = true;
                let Some(block) = member.block() else {
                    out.problems.push(
                        settings_invalid("`email:` holds no block", node_span(&path, member.syntax()), "not a block")
                            .reason("`email: { connection: <name>  from: config.<name> }` names the connection every email goes through and the address it is from.")
                            .next("Write `email: { connection: mail  from: config.mail_sender }`"),
                    );
                    continue;
                };
                let mut given = BTreeSet::new();
                for (setting, entry) in tree::members(Some(block)) {
                    let name = name_text(&setting).unwrap_or_default();
                    let span = entry.value().map_or_else(
                        || node_span(&path, entry.syntax()),
                        |value| node_span(&path, value.syntax()),
                    );
                    if !EMAIL_KEYS.contains(&name.as_str()) {
                        out.problems.push(
                            settings_invalid(format!("`{name}` isn't an email setting"), node_span(&path, setting.syntax()), "not a setting")
                                .reason("`email:` holds `connection:`, the connection every email goes through, and `from:`, the address they are from; anything a provider offers besides is the connection's own setting.")
                                .next(format!("Remove `{name}:`")),
                        );
                        continue;
                    }
                    given.insert(name.clone());
                    let Some(value) = entry.value() else { continue };
                    if name == "connection" {
                        let written = tree::written(value.syntax());
                        match connections.kind(&written) {
                            Some(ConnectionKind::Smtp) => out.connection = Some(written),
                            found => {
                                let smtp: Vec<String> = connections
                                    .declared
                                    .iter()
                                    .filter(|(_, kind)| *kind == ConnectionKind::Smtp)
                                    .map(|(name, _)| format!("`{name}`"))
                                    .collect();
                                let what = match found {
                                    Some(ConnectionKind::Package) => format!(
                                        "`{written}` is a package's connection, and a package that provides `email` comes in a later release"
                                    ),
                                    Some(_) => {
                                        format!("`{written}` is a connection that sends no email")
                                    }
                                    None => format!("`{written}` is no connection"),
                                };
                                let reason = if smtp.is_empty() {
                                    format!(
                                        "`email: {{ connection: }}` names a connection that provides `email`, one with `driver: Driver.smtp`; {what}, and `app.jadpo` declares none."
                                    )
                                } else {
                                    format!(
                                        "`email: {{ connection: }}` names a connection that provides `email`, one with `driver: Driver.smtp`; {what}. The application's are {}.",
                                        crate::and_list(&smtp)
                                    )
                                };
                                out.problems.push(
                                    settings_invalid(format!("`connection: {written}` sends no email"), span, "not an email connection")
                                        .reason(reason)
                                        .next("Declare a connection with `driver: Driver.smtp` in `connections:`, and name it here"),
                                );
                            }
                        }
                        continue;
                    }
                    // `from:`: an `Email` setting, or an address written
                    // out on one line.
                    let headline = "`from:` must be the address every email is from";
                    let reason = "`from:` is the address every email is from: an `Email` setting, `config.<name>`, or an address written out, `\"hello@acme.test\"`.";
                    match (reads(&value), literal_text(&value)) {
                        (Some(field), _) => match config(db, project).setting(&field) {
                            Some(setting) if lookup.base_prim(&setting.ty) == Some(Prim::Email) => {
                                out.from = Some(Sender::Setting(field));
                            }
                            Some(setting) => out.problems.push(
                                settings_invalid(headline, span, &format!("`{field}` is `{}`", setting.ty))
                                    .reason(reason)
                                    .next(format!("Declare `{field}: Email` in `config.jadpo`")),
                            ),
                            // An unknown setting is reported where it is read.
                            None => {}
                        },
                        (None, Some(address))
                            if address.split('@').count() == 2
                                && address.split('@').all(|part| !part.is_empty())
                                && !address.chars().any(|c| c.is_control() || c.is_whitespace()) =>
                        {
                            out.from = Some(Sender::Address(address));
                        }
                        _ => out.problems.push(
                            settings_invalid(headline, span, "not an address")
                                .reason(reason)
                                .next("Write `from: config.mail_sender`, declaring `mail_sender: Email` in `config.jadpo`"),
                        ),
                    }
                }
                for needed in EMAIL_KEYS {
                    if !given.contains(needed) {
                        out.problems.push(
                            settings_invalid(format!("`email:` has no `{needed}:`"), node_span(&path, key.syntax()), "incomplete")
                                .reason("`email:` names `connection:`, the connection every email goes through, and `from:`, the address they are from.")
                                .next(format!("Add `{needed}:` to `email:`")),
                        );
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// Each connection with `driver: Driver.smtp`.
#[salsa::tracked(returns(ref))]
pub fn smtp_connections(db: &dyn Db, project: Project) -> SmtpConnections {
    let mut out = SmtpConnections::default();
    let lookup = Lookup::new(db, project);
    for (path, key, member) in application(db, project) {
        if name_text(&key).as_deref() != Some("connections") {
            continue;
        }
        for (name, entry) in tree::members(member.block()) {
            let Some(connection) = name_text(&name) else {
                continue;
            };
            if crate::data::connection_kind(entry.block()) != ConnectionKind::Smtp {
                continue;
            }
            let mut smtp = Smtp {
                name: connection.clone(),
                host: None,
                port: jadpo_std::email::SMTP_PORT,
                username: None,
                password: None,
            };
            let invalid = |title: String, span: Span, label: &str| {
                Diagnostic::new(&rule::CONNECTION_VALUE_INVALID, title).at(span, label.to_string())
            };
            for (setting, option) in tree::members(entry.block()) {
                let word = name_text(&setting).unwrap_or_default();
                let span = option.value().map_or_else(
                    || node_span(&path, option.syntax()),
                    |value| node_span(&path, value.syntax()),
                );
                match word.as_str() {
                    "driver" => {}
                    "host" => {
                        let value = option.value();
                        match (value.as_ref().and_then(reads), value.as_ref().and_then(literal_text)) {
                            (Some(field), _) => match config(db, project).setting(&field) {
                                Some(found) if lookup.base_prim(&found.ty) == Some(Prim::Text) => {
                                    smtp.host = Some(SmtpHost::Setting(field));
                                }
                                Some(found) => out.problems.push(
                                    invalid(format!("`{connection}`'s `host:` must be a server's name"), span, &format!("`{field}` is `{}`", found.ty))
                                        .reason("`host:` is the SMTP server's name, `smtp.example.com`: text written out, or a setting built on `Text`, `config.<name>`, read from each deployment's environment.")
                                        .next(format!("Declare `{field}: Hostname` in `config.jadpo`")),
                                ),
                                None => {}
                            },
                            (None, Some(host))
                                if !host.is_empty()
                                    && !host.chars().any(|c| c.is_control() || c.is_whitespace() || c == '/') =>
                            {
                                smtp.host = Some(SmtpHost::Written(host));
                            }
                            _ => out.problems.push(
                                invalid(format!("`{connection}`'s `host:` must be a server's name"), span, "not a server's name")
                                    .reason("`host:` is the SMTP server's name, `smtp.example.com`: text written out, or a setting built on `Text`, `config.<name>`, read from each deployment's environment.")
                                    .next("Write `host: \"smtp.example.com\"`, or `host: config.smtp_host`"),
                            ),
                        }
                    }
                    "port" => match option.value().as_ref().map(crate::types::Const::from_expr) {
                        Some(crate::types::Const::Number { decimal: false, .. })
                            if option
                                .value()
                                .and_then(|value| tree::written(value.syntax()).parse::<u16>().ok())
                                .is_some_and(|port| port > 0) =>
                        {
                            smtp.port = option
                                .value()
                                .and_then(|value| tree::written(value.syntax()).parse::<u16>().ok())
                                .unwrap_or(jadpo_std::email::SMTP_PORT);
                        }
                        _ => out.problems.push(
                            invalid(format!("`{connection}`'s `port:` must be a port number"), span, "not a port")
                                .reason("`port:` is the port the SMTP server listens on, a whole number from 1 to 65535: 587 for submission, which it is when left out, or 465 where the server starts with TLS. Cloudflare Workers can't reach port 25.")
                                .next("Write `port: 587`, or remove it"),
                        ),
                    },
                    "credential" => {
                        let Some(block) = option.block() else {
                            out.problems.push(
                                invalid(format!("`{connection}`'s `credential:` holds no block"), span, "not a block")
                                    .reason("An SMTP server's credential is a user name and a password, `credential: { username: config.<name>  password: config.<name> }`, each read from a setting.")
                                    .next("Write `credential: { username: config.smtp_username  password: config.smtp_password }`"),
                            );
                            continue;
                        };
                        for (part, value) in tree::members(Some(block)) {
                            let part_name = name_text(&part).unwrap_or_default();
                            let part_span = value.value().map_or_else(
                                || node_span(&path, value.syntax()),
                                |value| node_span(&path, value.syntax()),
                            );
                            if part_name != "username" && part_name != "password" {
                                out.problems.push(
                                    Diagnostic::new(&rule::CONNECTION_KEY_INVALID, format!("`{connection}`'s credential takes no `{part_name}:`"))
                                        .at(node_span(&path, part.syntax()), "not a part of the credential")
                                        .reason("An SMTP server's credential is a user name and a password: `username:` and `password:`, each read from a setting.")
                                        .next(format!("Remove `{part_name}:`")),
                                );
                                continue;
                            }
                            let Some(field) = value.value().as_ref().and_then(reads) else {
                                out.problems.push(
                                    invalid(format!("`{connection}`'s `{part_name}:` must be a setting"), part_span, "written in source")
                                        .reason("A credential is read from the deployment's environment, so each part is a setting, `config.<name>`, and the password a `secret: true` one (area 10, \"Secrets\").")
                                        .next(format!("Declare `smtp_{part_name}: Text` in `config.jadpo`, and write `{part_name}: config.smtp_{part_name}`")),
                                );
                                continue;
                            };
                            let Some(found) = config(db, project).setting(&field) else {
                                continue;
                            };
                            if part_name == "password" && !found.secret {
                                out.problems.push(
                                    invalid(format!("`{connection}`'s password must be a secret setting"), part_span, &format!("`{field}` isn't secret"))
                                        .reason("A password is read from the deployment's environment and never seen by code, so it is a `secret: true` setting (area 10, \"Secrets\").")
                                        .next(format!("Add `secret: true` to `{field}` in `config.jadpo`")),
                                );
                                continue;
                            }
                            if part_name == "username" {
                                smtp.username = Some(field);
                            } else {
                                smtp.password = Some(field);
                            }
                        }
                        if smtp.username.is_some() != smtp.password.is_some()
                            && !out.problems.iter().any(|p| p.rule.id == rule::CONNECTION_VALUE_INVALID.id)
                        {
                            out.problems.push(
                                Diagnostic::new(&rule::CONNECTION_KEY_MISSING, format!("`{connection}`'s credential needs a user name and a password"))
                                    .at(span, "one part given")
                                    .reason("An SMTP server's credential is a user name and a password, each read from a setting; one without the other can't sign in.")
                                    .next("Give both `username:` and `password:`"),
                            );
                        }
                    }
                    other => out.problems.push(
                        Diagnostic::new(&rule::CONNECTION_KEY_INVALID, format!("A connection with `driver: Driver.smtp` takes no `{other}:`"))
                            .at(node_span(&path, setting.syntax()), "not an SMTP setting")
                            .reason("A connection to an SMTP server gives `host:`, the server's name, and may give `port:` (587 when left out) and `credential: { username: password: }`.")
                            .next(format!("Remove `{other}:`")),
                    ),
                }
            }
            if !tree::members(entry.block())
                .iter()
                .any(|(setting, _)| name_text(setting).as_deref() == Some("host"))
            {
                out.problems.push(
                    Diagnostic::new(&rule::CONNECTION_KEY_MISSING, format!("`{connection}` doesn't say which server it reaches"))
                        .at(node_span(&path, name.syntax()), "no `host:`")
                        .reason("A connection with `driver: Driver.smtp` gives `host:`, its SMTP server's name: text written out, or a setting, `config.<name>`.")
                        .next("Add `host: config.smtp_host`, declaring `smtp_host: Hostname` in `config.jadpo`"),
                );
            }
            debug_assert!(SMTP_KEYS.contains(&"host"));
            out.list.push(smtp);
        }
    }
    out.list.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// Every email, its templates, and the shared layout.
#[salsa::tracked(returns(ref))]
pub fn analysis(db: &dyn Db, project: Project) -> Analysis {
    let mut out = Analysis::default();
    let mut found: Vec<&Symbol> = symbols(db, project)
        .declarations
        .values()
        .flatten()
        .filter(|symbol| is_email(symbol))
        .collect();
    found.sort_by(|a, b| (&a.path, a.index).cmp(&(&b.path, b.index)));
    let records = project.records(db);
    let exists = |path: &str| records.iter().any(|record| record.path == path);
    let mut claimed: Vec<String> = Vec::new();
    for symbol in found {
        let Some(file) = file_at(db, project, Name::new(db, symbol.path.clone())) else {
            continue;
        };
        let Some(source) = tree::source(db, file) else {
            continue;
        };
        let Some(Decl::TypeDecl(decl)) = tree::decl_at(&source, symbol.index) else {
            continue;
        };
        let name_span = decl.name().map_or_else(
            || node_span(&symbol.path, decl.syntax()),
            |name| node_span(&symbol.path, name.syntax()),
        );
        let written: Vec<(String, Span)> = tree::members(decl.member_list())
            .into_iter()
            .filter_map(|(name, _)| {
                Some((
                    tree::name_text(&name)?,
                    node_span(&symbol.path, name.syntax()),
                ))
            })
            .collect();
        let lookup = Lookup::within(db, project, &symbol.ownership);
        let canonical = lookup.canonical_name(&symbol.name);
        let defs = match lookup.def(&canonical) {
            Some(Def::Fields { fields, .. }) => fields.clone(),
            _ => Vec::new(),
        };
        let fields: Vec<(String, Ty, Span)> = defs
            .iter()
            .map(|field| {
                let span = written
                    .iter()
                    .find(|(name, _)| *name == field.name)
                    .map_or_else(|| name_span.clone(), |(_, span)| span.clone());
                (field.name.clone(), field.declared.clone(), span)
            })
            .collect();
        let attachments = defs
            .iter()
            .filter(|field| {
                matches!(field.declared.strip_nullable(), Ty::Opaque(held, args)
                    if held == jadpo_std::files::FILE && args.is_empty())
            })
            .map(|field| field.name.clone())
            .collect();
        let stem = format!("{}{}", folder_of(&symbol.path), to_snake(&symbol.name));
        let html = format!("{stem}.html");
        let text = format!("{stem}.txt");
        claimed.push(html.clone());
        claimed.push(text.clone());
        let html = exists(&html).then_some(html);
        let text = exists(&text).then_some(text);
        if html.is_none() && text.is_none() {
            let file = to_snake(&symbol.name);
            out.problems.push(
                Diagnostic::new(
                    &rule::EMAIL_TEMPLATE_MISSING,
                    format!("`{}` has no template", symbol.name),
                )
                .at(name_span.clone(), "nothing to send")
                .reason(format!(
                    "An email's words live beside its type: `{file}.html`, its HTML, and `{file}.txt`, its plain text, each reading its fields inside `{{{{ }}}}`. It needs at least one."
                ))
                .next(format!(
                    "Create `{stem}.html` and `{stem}.txt` beside `{}`",
                    symbol.path.rsplit('/').next().unwrap_or(&symbol.path)
                )),
            );
        }
        out.emails.push(EmailFacts {
            canonical,
            type_name: symbol.name.clone(),
            path: symbol.path.clone(),
            name_span,
            ownership: symbol.ownership.clone(),
            subject: to_snake(&symbol.name),
            html,
            text,
            fields,
            attachments,
        });
    }
    out.layout_html = exists(LAYOUT_HTML).then(|| LAYOUT_HTML.to_string());
    out.layout_text = exists(LAYOUT_TEXT).then(|| LAYOUT_TEXT.to_string());
    // A template that belongs to no email, and a file in `shared/emails/`
    // that isn't the layout.
    for record in records {
        let path = &record.path;
        if !jadpo_db::is_email_template(path)
            || claimed.contains(path)
            || path == LAYOUT_HTML
            || path == LAYOUT_TEXT
        {
            continue;
        }
        let file = path.rsplit('/').next().unwrap_or(path);
        let stem = file.rsplit_once('.').map_or(file, |(stem, _)| stem);
        let first = record
            .text
            .as_deref()
            .unwrap_or("")
            .lines()
            .next()
            .map_or(0, str::len);
        let at = Span::new(path, 0, u32::try_from(first).unwrap_or(0));
        let diagnostic = if path.starts_with("shared/emails/") {
            Diagnostic::new(
                &rule::EMAIL_TEMPLATE_MISSING,
                format!("`{file}` belongs to no email"),
            )
            .at(at, "not the layout")
            .reason("`shared/emails/` holds the layout every email is wrapped in, `layout.html` and `layout.txt`, and nothing else; an email's own templates sit beside its type in its domain's `emails/`.")
            .next(format!("Move `{file}` beside the email it belongs to, or remove it"))
        } else {
            let type_name = crate::case::to_camel(stem);
            Diagnostic::new(
                &rule::EMAIL_TEMPLATE_MISSING,
                format!("`{file}` belongs to no email"),
            )
            .at(at, "no type beside it")
            .reason(format!(
                "An email's templates sit beside its `type X = EmailMessage {{ ... }}`, named after it, so nothing sends `{file}` until a `{type_name}` is declared in `{}{stem}.jadpo`.",
                folder_of(path)
            ))
            .next(format!(
                "Declare `type {type_name} = EmailMessage {{ ... }}` beside it, or remove the file"
            ))
        };
        out.problems.push(diagnostic);
    }
    out.emails.sort_by(|a, b| a.canonical.cmp(&b.canonical));
    out
}

/// What the template at `path` is checked against, when it belongs to an
/// email or is the layout.
pub fn template_host(db: &dyn Db, project: Project, path: &str) -> Option<TemplateHost> {
    let analysis = analysis(db, project);
    let html = path.ends_with(".html");
    let kind = if html { "html" } else { "text" };
    if path == LAYOUT_HTML || path == LAYOUT_TEXT {
        let lookup = Lookup::new(db, project);
        let std =
            |name: &str| lookup.resolve_std(&TypeRef::Path(vec![name.to_string()], Vec::new()));
        let at = Span::new(path, 0, 0);
        let content = if html {
            std("Html")
        } else {
            Ty::Prim(Prim::Text)
        };
        return Some(TemplateHost {
            host: Host::Layout,
            html,
            names: vec![
                ("content".to_string(), content, at.clone()),
                (
                    "subject".to_string(),
                    Ty::Prim(Prim::PresentationText),
                    at.clone(),
                ),
                ("locale".to_string(), std(jadpo_std::LOCALE), at),
            ],
            ownership: Ownership::Shared,
            label: format!("email layout {kind}"),
        });
    }
    let email = analysis.email_of(path)?;
    Some(TemplateHost {
        host: Host::Email(email.canonical.clone()),
        html,
        names: email.fields.clone(),
        ownership: email.ownership.clone(),
        label: format!("email {} {kind}", email_name(email)),
    })
}

/// How a log names an email: `accounts.WelcomeEmail`.
pub fn email_name(email: &EmailFacts) -> String {
    match email.scope() {
        Some(scope) => format!("{scope}.{}", email.type_name),
        None => email.type_name.clone(),
    }
}

/// Every template's path: each email's, then the layout's.
pub fn template_paths(db: &dyn Db, project: Project) -> Vec<String> {
    let analysis = analysis(db, project);
    analysis
        .emails
        .iter()
        .flat_map(|email| email.templates().cloned())
        .chain(analysis.layout_html.clone())
        .chain(analysis.layout_text.clone())
        .collect()
}

/// The identity keys of every email type.
pub fn email_keys(db: &dyn Db, project: Project) -> BTreeSet<Key> {
    let lookup = Lookup::new(db, project);
    analysis(db, project)
        .emails
        .iter()
        .filter_map(|email| crate::graph::declaration_key(&lookup, &email.canonical))
        .collect()
}

/// Every problem with the project's email: its settings, its SMTP
/// connections, its templates and what they read, a field nothing
/// writes, and an email emitted with nothing to send it.
#[salsa::tracked(returns(ref))]
pub fn email_problems(db: &dyn Db, project: Project) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    out.extend(settings(db, project).problems.iter().cloned());
    out.extend(smtp_connections(db, project).problems.iter().cloned());
    let analysis = analysis(db, project);
    out.extend(analysis.problems.iter().cloned());
    for path in template_paths(db, project) {
        out.extend(
            crate::types::template_body(db, project, Name::new(db, path))
                .problems
                .iter()
                .cloned(),
        );
    }
    out.extend(unused_fields(db, project));
    out.extend(unsent(db, project));
    out
}

/// `email.field_unused`: a field of an email that no template writes and
/// its subject's wording leaves out in every locale, besides `to:`,
/// `locale:` and its attachments.
fn unused_fields(db: &dyn Db, project: Project) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    let plan = crate::translations::plan(db, project);
    for email in &analysis(db, project).emails {
        if email.html.is_none() && email.text.is_none() {
            continue;
        }
        let mut read: BTreeSet<String> = BTreeSet::new();
        let mut broken = false;
        for path in email.templates() {
            let checked = crate::types::template_body(db, project, Name::new(db, path.clone()));
            if !checked.problems.is_empty() {
                broken = true;
            }
            read.extend(crate::types::template_reads(&checked.body));
        }
        if broken {
            continue;
        }
        if let Some(scope) = email.scope()
            && let Some((_, entry)) = plan.entry(&format!("{scope}.{}", email.subject))
        {
            for (_, wording) in &entry.wordings {
                if let jadpo_std::translation::Wording::Plain(template) = wording {
                    read.extend(template.fields().map(str::to_string));
                }
            }
        }
        for (name, _, span) in &email.fields {
            if crate::types::EMAIL_FIELDS
                .iter()
                .any(|(own, _)| own == name)
                || email.attachments.contains(name)
                || read.contains(name)
            {
                continue;
            }
            out.push(
                Diagnostic::new(
                    &rule::EMAIL_FIELD_UNUSED,
                    format!("`{}.{name}` is written nowhere", email.type_name),
                )
                .at(span.clone(), "no template writes it")
                .reason(format!(
                    "An email's fields are what its templates and its subject read, so a field none of them reads is sent for nothing, or is a value a template was meant to show; no template of `{}` reads `{name}`, and no wording of its subject writes `{{{name}}}`.",
                    email.type_name
                ))
                .next(format!(
                    "Read `{name}` in a template or the subject, or remove it from `{}`",
                    email.type_name
                )),
            );
        }
    }
    out
}

/// `email.connection_required`: an action emits an email, and
/// `application { }` names no connection to send it through.
fn unsent(db: &dyn Db, project: Project) -> Vec<Diagnostic> {
    let settings = settings(db, project);
    if settings.written {
        return Vec::new();
    }
    let keys = email_keys(db, project);
    if keys.is_empty() {
        return Vec::new();
    }
    let smtp: Vec<String> = smtp_connections(db, project)
        .list
        .iter()
        .map(|connection| connection.name.clone())
        .collect();
    for &file in project.files(db) {
        for callable in crate::types::callables_of(db, file) {
            let checked = crate::types::typed_body(db, project, callable);
            for (fact, (start, end)) in crate::types::facts_at(&checked.body) {
                let crate::types::Fact::Emits(event) = fact else {
                    continue;
                };
                if !keys.contains(&event) {
                    continue;
                }
                let name = event
                    .name()
                    .rsplit('.')
                    .next()
                    .unwrap_or_default()
                    .to_string();
                let next = match smtp.first() {
                    Some(connection) => format!(
                        "Add `email: {{ connection: {connection}  from: config.mail_sender }}` to `application {{ }}` in `app.jadpo`"
                    ),
                    None => "Declare a connection with `driver: Driver.smtp` in `connections:`, and add `email: { connection: <its name>  from: config.mail_sender }` to `application { }` in `app.jadpo`".to_string(),
                };
                return vec![
                    Diagnostic::new(
                        &rule::EMAIL_CONNECTION_REQUIRED,
                        format!("`{name}` is emitted, but nothing sends email"),
                    )
                    .at(Span::new(file.path(db), start, end), "no email connection")
                    .reason("An email is sent once the action that emits it commits, through the connection `application { email: }` names, from the address its `from:` gives; `app.jadpo` names none.")
                    .next(next),
                ];
            }
        }
    }
    Vec::new()
}
