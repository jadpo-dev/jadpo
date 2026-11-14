//! Events published outward (area 12, "Where events travel"; area 16,
//! "Outgoing webhooks" and "Sending audit entries elsewhere").
//!
//! `application { publishing: { <connection>: { events: [...] } } }`
//! chooses the public events each connection publishes. The connection is
//! one with `driver: Driver.webhook`, which names one address, `url:`, and
//! the secret each delivery is signed with, `secret:`
//! ([`webhook_connections`]). Each listed event is a `public` event of one
//! of the app's own domains, or the built-in `AuditEntryRecorded` while an
//! entity keeps an audit trail; it holds no `File` or `FileContents`,
//! whose store and key mean nothing outside the app; and each is named
//! outside by its `wire_name:`, or its domain and name
//! (`planning.TodoCompleted`), no two alike through one connection.

use std::collections::BTreeMap;

use jadpo_db::{Db, Ownership, Project};
use jadpo_diag::{Diagnostic, Span, nearest, rule};
use jadpo_graph::Key;
use jadpo_syntax::ast::{self, AstNode, Expr};

use crate::config::{config, singletons};
use crate::data::ConnectionKind;
use crate::lookups::{Lookup, NameView};
use crate::resolve::{Names, Want};
use crate::std_lib::StdNames;
use crate::tree::{self, name_text, node_span};
use crate::types::{Const, FieldsBase, Prim, Ty, TypeRef, TypeView};

/// What a connection with `driver: Driver.webhook` gives.
pub const WEBHOOK_KEYS: [&str; 3] = ["driver", "url", "secret"];

/// What each connection in `publishing:` gives.
pub const PUBLISHING_KEYS: [&str; 1] = ["events"];

/// Where a connection with `driver: Driver.webhook` posts.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum HookUrl {
    /// A `Url` setting, by name.
    Setting(String),
    /// An address written out: HTTPS, or plain HTTP to this machine.
    Written(String),
}

/// One connection with `driver: Driver.webhook`, as far as it reads.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct WebhookConnection {
    pub name: String,
    pub url: Option<HookUrl>,
    /// The secret setting its deliveries are signed with, by name.
    pub secret: Option<String>,
}

impl WebhookConnection {
    /// Whether every part of it read, so a driver can be built from it.
    pub fn complete(&self) -> bool {
        self.url.is_some() && self.secret.is_some()
    }
}

/// Every connection with `driver: Driver.webhook`, and the problems with
/// them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WebhookConnections {
    pub list: Vec<WebhookConnection>,
    pub problems: Vec<Diagnostic>,
}

const URL_REASON: &str = "`url:` is the one address a connection with `driver: Driver.webhook` posts each event to: a `Url` setting, `config.<name>`, read from each deployment's environment, or an address written out. It is HTTPS; plain HTTP reaches only this machine (`localhost`, `127.x.x.x`, `[::1]`), where a test's receiver listens.";

const SECRET_REASON: &str = "Every event a connection with `driver: Driver.webhook` posts is signed as Standard Webhooks signs, so its receiver can tell it came from the app: `secret:` is the key it is signed with, a `secret: true` setting read from each deployment's environment, `whsec_` and its Base64 or any text, which the receiver holds too.";

/// Each connection with `driver: Driver.webhook`, read.
#[salsa::tracked(returns(ref))]
pub fn webhook_connections(db: &dyn Db, project: Project) -> WebhookConnections {
    let mut out = WebhookConnections::default();
    let lookup = Lookup::new(db, project);
    for found in singletons(db, project, "application") {
        for (key, member) in tree::members(found.list.clone()) {
            if name_text(&key).as_deref() != Some("connections") {
                continue;
            }
            for (name, entry) in tree::members(member.block()) {
                let Some(connection) = name_text(&name) else {
                    continue;
                };
                if crate::data::connection_kind(entry.block()) != ConnectionKind::Webhook {
                    continue;
                }
                let mut hook = WebhookConnection {
                    name: connection.clone(),
                    url: None,
                    secret: None,
                };
                let (mut has_url, mut has_secret) = (false, false);
                for (setting, option) in tree::members(entry.block()) {
                    let word = name_text(&setting).unwrap_or_default();
                    let span = option.value().map_or_else(
                        || node_span(&found.path, option.syntax()),
                        |value| node_span(&found.path, value.syntax()),
                    );
                    match word.as_str() {
                        "driver" => {}
                        "url" => {
                            has_url = true;
                            hook.url = url(
                                db,
                                project,
                                &lookup,
                                &connection,
                                &option,
                                span,
                                &mut out.problems,
                            );
                        }
                        "secret" => {
                            has_secret = true;
                            hook.secret =
                                secret(db, project, &connection, &option, span, &mut out.problems);
                        }
                        other => {
                            let near = nearest(other, WEBHOOK_KEYS.iter().copied())
                                .filter(|near| *near != "driver");
                            let at = node_span(&found.path, setting.syntax());
                            let mut problem = Diagnostic::new(
                                &rule::CONNECTION_KEY_INVALID,
                                format!("A connection with `driver: Driver.webhook` takes no `{other}:`"),
                            )
                            .at(at.clone(), "not a setting of the driver")
                            .reason("A connection with `driver: Driver.webhook` gives `url:`, the address each published event is posted to, and `secret:`, the secret setting each one is signed with, and nothing else: which events it publishes is `publishing:`'s.");
                            problem = match near {
                                Some(near) => problem
                                    .next(format!("Use `{near}:` instead of `{other}:`"))
                                    .edit(at, near),
                                None => problem.next(format!("Remove `{other}:`")),
                            };
                            out.problems.push(problem);
                        }
                    }
                }
                let head = node_span(&found.path, name.syntax());
                if !has_url {
                    out.problems.push(
                        Diagnostic::new(
                            &rule::CONNECTION_KEY_MISSING,
                            format!("`{connection}` doesn't say where its events go"),
                        )
                        .at(head.clone(), "no `url:`")
                        .reason(URL_REASON)
                        .next(format!("Add `url: config.{connection}_url`, declaring `{connection}_url: Url` in `config.jadpo`")),
                    );
                }
                if !has_secret {
                    out.problems.push(
                        Diagnostic::new(
                            &rule::CONNECTION_KEY_MISSING,
                            format!("`{connection}` doesn't say what its events are signed with"),
                        )
                        .at(head, "no `secret:`")
                        .reason(SECRET_REASON)
                        .next(format!("Add `secret: config.{connection}_secret`, declaring `{connection}_secret: Text {{ secret: true }}` in `config.jadpo`")),
                    );
                }
                out.list.push(hook);
            }
        }
    }
    out.list.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// A webhook connection's `url:`: a `Url` setting, or an address written
/// out that a call may be sent to, HTTPS or plain HTTP to this machine.
fn url(
    db: &dyn Db,
    project: Project,
    lookup: &Lookup<'_>,
    connection: &str,
    option: &ast::Member,
    span: Span,
    out: &mut Vec<Diagnostic>,
) -> Option<HookUrl> {
    let invalid = |title: String, label: &str| {
        Diagnostic::new(&rule::CONNECTION_VALUE_INVALID, title).at(span.clone(), label.to_string())
    };
    let value = option.value()?;
    if let Some(field) = crate::emails::reads(&value) {
        let setting = config(db, project).setting(&field)?;
        if lookup.base_prim(&setting.ty) == Some(Prim::Url) {
            return Some(HookUrl::Setting(field));
        }
        out.push(
            invalid(
                format!("`{connection}`'s `url:` must be an address"),
                &format!("`{field}` is `{}`", setting.ty),
            )
            .reason(URL_REASON)
            .next(format!("Declare `{field}: Url` in `config.jadpo`")),
        );
        return None;
    }
    match Const::from_expr(&value) {
        Const::Str(text) if jadpo_std::provider::sendable(&text).is_some() => {
            Some(HookUrl::Written(text))
        }
        Const::Str(text) if text.trim().starts_with("http://") => {
            out.push(
                invalid(
                    format!("`{connection}`'s `url:` is plain HTTP to another machine"),
                    "not this machine",
                )
                .reason(URL_REASON)
                .next("Use the receiver's `https://` address"),
            );
            None
        }
        _ => {
            out.push(
                invalid(
                    format!("`{connection}`'s `url:` must be an address"),
                    "not an address",
                )
                .reason(URL_REASON)
                .next(format!("Write `url: config.{connection}_url`, declaring `{connection}_url: Url` in `config.jadpo`")),
            );
            None
        }
    }
}

/// A webhook connection's `secret:`: a secret setting, by name.
fn secret(
    db: &dyn Db,
    project: Project,
    connection: &str,
    option: &ast::Member,
    span: Span,
    out: &mut Vec<Diagnostic>,
) -> Option<String> {
    let value = option.value()?;
    let Some(field) = crate::emails::reads(&value) else {
        out.push(
            Diagnostic::new(
                &rule::CONNECTION_VALUE_INVALID,
                format!("`{connection}`'s `secret:` must be a setting"),
            )
            .at(span, "written in source")
            .reason(SECRET_REASON)
            .next(format!("Declare `{connection}_secret: Text {{ secret: true }}` in `config.jadpo`, and write `secret: config.{connection}_secret`")),
        );
        return None;
    };
    // An unknown setting is reported where it is read.
    let setting = config(db, project).setting(&field)?;
    if !setting.secret {
        out.push(
            Diagnostic::new(
                &rule::CONNECTION_VALUE_INVALID,
                format!("`{connection}`'s `secret:` must be a secret setting"),
            )
            .at(span, format!("`{field}` isn't secret"))
            .reason(SECRET_REASON)
            .next(format!("Add `secret: true` to `{field}` in `config.jadpo`")),
        );
        return None;
    }
    Some(field)
}

/// An event one connection publishes.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PublishedEvent {
    /// The event type's identity key.
    pub key: Key,
    /// As `events:` writes it.
    pub name: String,
    /// The type it is named by outside.
    pub type_name: String,
}

/// One connection in `publishing:`, read.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Published {
    pub connection: String,
    pub events: Vec<PublishedEvent>,
}

/// Everything `publishing:` chooses, and the problems with it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Publishing {
    pub list: Vec<Published>,
    pub problems: Vec<Diagnostic>,
}

impl Publishing {
    /// Each connection that publishes the event whose key is `event`,
    /// with the type it is named by there.
    pub fn of_event<'a>(&'a self, event: &'a str) -> impl Iterator<Item = (&'a str, &'a str)> {
        self.list.iter().flat_map(move |published| {
            published
                .events
                .iter()
                .filter(move |known| known.key.as_str() == event)
                .map(move |known| (published.connection.as_str(), known.type_name.as_str()))
        })
    }
}

const REASON: &str = "`publishing:` names each connection with `driver: Driver.webhook` that events leave the app through, and lists, in `events:`, the events it publishes: each a `public` event of one of the app's domains, or `AuditEntryRecorded` while an entity keeps an audit trail. Every event listed becomes an outside system's contract, so only events a domain offers as `public` leave.";

/// The connections `publishing:` names, read.
#[salsa::tracked(returns(ref))]
pub fn publishing(db: &dyn Db, project: Project) -> Publishing {
    let mut out = Publishing::default();
    let hooks = webhook_connections(db, project);
    let kinds = crate::data::connections(db, project);
    let lookup = Lookup::within(db, project, &Ownership::Root);
    for found in singletons(db, project, "application") {
        for (key, member) in tree::members(found.list.clone()) {
            if name_text(&key).as_deref() != Some("publishing") {
                continue;
            }
            let mut reader = Reader {
                db,
                project,
                path: &found.path,
                lookup: &lookup,
                ownership: &Ownership::Root,
                reason: REASON,
                audit: true,
                out: &mut out.problems,
            };
            let Some(block) = member.block() else {
                reader.invalid(
                    node_span(&found.path, member.syntax()),
                    "`publishing:` isn't a block".to_string(),
                    "not a block",
                    "Write `publishing: { <connection>: { events: [<Event>] } }`",
                );
                continue;
            };
            for (name, entry) in tree::members(Some(block)) {
                let Some(connection) = name_text(&name) else {
                    continue;
                };
                let span = node_span(&found.path, name.syntax());
                if !hooks.list.iter().any(|hook| hook.name == connection) {
                    let next = match kinds.kind(&connection) {
                        None => format!(
                            "Add `{connection}` to `connections:` with `driver: Driver.webhook`, or name one that has it"
                        ),
                        Some(_) => "Name a connection with `driver: Driver.webhook`".to_string(),
                    };
                    reader.invalid(
                        span,
                        format!("`{connection}` isn't a connection events are published through"),
                        "not a webhook's connection",
                        &next,
                    );
                    continue;
                }
                if out.list.iter().any(|known| known.connection == connection) {
                    continue;
                }
                if let Some(published) = reader.connection(&connection, span, &entry) {
                    out.list.push(published);
                }
            }
        }
    }
    out.list.sort_by(|a, b| a.connection.cmp(&b.connection));
    out
}

/// What reads a list of events to publish: `publishing:`'s, or an
/// entity's `webhooks:` ([`crate::outgoing`]).
pub(crate) struct Reader<'r, 'db> {
    pub(crate) db: &'db dyn Db,
    pub(crate) project: Project,
    pub(crate) path: &'r str,
    pub(crate) lookup: &'r Lookup<'db>,
    /// Where the list is written, which its names are read in.
    pub(crate) ownership: &'r Ownership,
    /// What a refusal says the list is for.
    pub(crate) reason: &'static str,
    /// Whether `AuditEntryRecorded` may be listed.
    pub(crate) audit: bool,
    pub(crate) out: &'r mut Vec<Diagnostic>,
}

impl Reader<'_, '_> {
    pub(crate) fn invalid(&mut self, span: Span, message: String, label: &str, next: &str) {
        self.out.push(
            Diagnostic::new(&rule::PUBLISHING_INVALID, message)
                .at(span, label.to_string())
                .reason(self.reason)
                .next(next.to_string()),
        );
    }

    pub(crate) fn span(&self, node: &impl AstNode) -> Span {
        node_span(self.path, node.syntax())
    }

    fn connection(&mut self, name: &str, span: Span, entry: &ast::Member) -> Option<Published> {
        let Some(block) = entry.block() else {
            self.invalid(
                self.span(entry),
                format!("`{name}` in `publishing:` isn't a block"),
                "not a block",
                "Write `<connection>: { events: [<Event>] }`",
            );
            return None;
        };
        let mut events = None;
        let mut listed = false;
        let mut ok = true;
        for (key, member) in tree::members(Some(block)) {
            let written = name_text(&key).unwrap_or_default();
            if written == "events" {
                listed = true;
                events = self.events(&member);
                continue;
            }
            ok = false;
            let near = nearest(&written, PUBLISHING_KEYS.iter().copied());
            self.invalid(
                self.span(&key),
                format!("A connection in `publishing:` has no key `{written}:`"),
                "not a key of `publishing:`",
                &match near {
                    Some(near) => format!("Use `{near}:` instead of `{written}:`"),
                    None => format!("Remove `{written}:`"),
                },
            );
        }
        let Some(events) = events else {
            if ok && !listed {
                self.invalid(
                    span,
                    format!("`{name}` doesn't list the events it publishes"),
                    "no `events:`",
                    "Add `events: [<Event>]`, each a `public` event of one of the app's domains",
                );
            }
            return None;
        };
        if !ok {
            return None;
        }
        let mut named: BTreeMap<String, String> = BTreeMap::new();
        for (event, at) in &events {
            if let Some(first) = named.insert(event.type_name.clone(), event.name.clone()) {
                self.invalid(
                    at.clone(),
                    format!(
                        "`{}` and `{first}` are both published as `{}`",
                        event.name, event.type_name
                    ),
                    "named already",
                    &format!("Give `{}` a `wire_name:` of its own", event.name),
                );
                return None;
            }
        }
        Some(Published {
            connection: name.to_string(),
            events: events.into_iter().map(|(event, _)| event).collect(),
        })
    }

    /// `events: [...]`: each a public event, with where it is written.
    pub(crate) fn events(&mut self, member: &ast::Member) -> Option<Vec<(PublishedEvent, Span)>> {
        let Some(Expr::ListExpr(list)) = member.value() else {
            self.invalid(
                self.span(member),
                "`events:` isn't a list".to_string(),
                "not a list",
                "Write `events: [<Event>]`",
            );
            return None;
        };
        let mut out: Vec<(PublishedEvent, Span)> = Vec::new();
        let names = Names {
            path: self.path,
            ownership: self.ownership,
            table: self.lookup,
            types: self.lookup,
            std: StdNames::get(),
            library: None,
        };
        let mut ok = true;
        for expr in list.exprs() {
            let at = self.span(&expr);
            let written = tree::written(expr.syntax());
            // A name that doesn't resolve is reported where it is written.
            let mut problems = Vec::new();
            names.type_expr(&expr, Want::Type, &mut problems);
            if !problems.is_empty() {
                ok = false;
                continue;
            }
            let ty = self.lookup.resolve(&TypeRef::from_expr(&expr));
            let event = match self.event(&expr, &ty, &written, &at) {
                Some(event) => event,
                None => {
                    ok = false;
                    continue;
                }
            };
            if out.iter().any(|(known, _)| known.key == event.key) {
                ok = false;
                self.invalid(
                    at,
                    format!("`events:` lists `{written}` twice"),
                    "listed already",
                    "Remove the second",
                );
                continue;
            }
            out.push((event, at));
        }
        if ok && out.is_empty() {
            self.invalid(
                self.span(member),
                "`events:` lists no event".to_string(),
                "empty",
                "List the events it publishes, or remove the connection from `publishing:`",
            );
            return None;
        }
        ok.then_some(out)
    }

    /// The event `expr`, of type `ty`, when it may be published.
    fn event(&mut self, expr: &Expr, ty: &Ty, written: &str, at: &Span) -> Option<PublishedEvent> {
        // Each audit trail's entry, emitted as it is kept (area 16,
        // "Sending audit entries elsewhere").
        if let Ty::Opaque(name, args) = ty
            && args.is_empty()
            && name == jadpo_std::audit::AUDIT_ENTRY_RECORDED
            && self.audit
        {
            let audited = crate::data::data_model(self.db, self.project)
                .entities
                .iter()
                .any(|entity| entity.audit.is_some());
            if !audited {
                self.invalid(
                    at.clone(),
                    format!("Nothing emits `{name}`"),
                    "no entity is audited",
                    "Give an entity an `audit:` section, or remove `AuditEntryRecorded`",
                );
                return None;
            }
            return Some(PublishedEvent {
                key: Key::from_text(jadpo_std::audit::AUDIT_EVENT_KEY),
                name: written.to_string(),
                type_name: jadpo_std::publishing::default_type(jadpo_std::audit::AUDIT_EVENT_KEY),
            });
        }
        // An audit entry is the runtime's own, naming no customer's record.
        if let Ty::Opaque(name, args) = ty
            && args.is_empty()
            && name == jadpo_std::audit::AUDIT_ENTRY_RECORDED
        {
            self.invalid(
                at.clone(),
                format!("`{name}` can't go to the addresses customers register"),
                "the runtime's own event",
                "List an event of your own that names the record whose addresses receive it",
            );
            return None;
        }
        let event = match ty {
            Ty::Named(name) if self.lookup.base(name) == Some(FieldsBase::Event) => name.clone(),
            Ty::Error => return None,
            _ => {
                self.invalid(
                    at.clone(),
                    format!("`{written}` isn't an event"),
                    "not an event",
                    "List events, each a type with the `Event` base",
                );
                return None;
            }
        };
        let symbol = self.lookup.declared(&event).first().cloned()?;
        if !matches!(symbol.ownership, Ownership::Domain(_)) {
            self.invalid(
                at.clone(),
                format!("`{written}` isn't one of the app's own events"),
                "not the app's",
                "List a `public` event of one of the app's domains",
            );
            return None;
        }
        if !symbol.public {
            self.invalid(
                at.clone(),
                format!("`{written}` isn't public"),
                "not public",
                &format!("Declare it `public type {written} = Event {{ ... }}`, offering it to whatever receives it"),
            );
            return None;
        }
        // A `File` is a store's key, which means nothing outside the app
        // (area 16, "What a `File` holds").
        if let Some(held) = crate::types::decl::held_file(self.lookup, ty, &mut Vec::new()) {
            self.out.push(crate::types::decl::file_taken(
                &format!("`{written}`"),
                &held,
                at.clone(),
            ));
            return None;
        }
        if let Some(held) = crate::types::decl::held_contents(self.lookup, ty, &mut Vec::new()) {
            self.out.push(crate::types::decl::contents_taken(
                &format!("`{written}`"),
                &held,
                at.clone(),
            ));
            return None;
        }
        let key = crate::graph::declaration_key(self.lookup, &event)?;
        let type_name = match crate::receiving::wire_name_of(self.db, self.project, &symbol) {
            Ok(Some(wire_name)) => wire_name,
            Ok(None) => jadpo_std::publishing::default_type(key.as_str()),
            Err(span) => {
                self.invalid(
                    span,
                    format!("`{event}`'s `wire_name:` isn't text"),
                    "not text",
                    "Write the type it is published as as text, such as `wire_name: \"order.placed\"`",
                );
                return None;
            }
        };
        let _ = expr;
        Some(PublishedEvent {
            key,
            name: written.to_string(),
            type_name,
        })
    }
}
